//! Intent - The single primitive that makes Rope feel like one product
//!
//! Philosophy
//! ----------
//! Rope has ~117 internal modules covering KV caches, MoE routing, spot
//! markets, federation, verification, RAG, and everything between. Users
//! should never see this complexity.
//!
//! An `Intent` is a declaration of what the user wants: a model to run,
//! a budget, a latency target, a privacy requirement. Rope is responsible
//! for composing the right modules to honor it. Like Shortcuts, like
//! Kubernetes declarative API, like Apple Intelligence's on-device-first
//! routing — but for GPU compute.
//!
//! A well-designed Intent is readable in five seconds and resolvable in
//! milliseconds. The resolver's job is to say no clearly when the Intent
//! is infeasible, and to pick the minimum-cost plan when it is.
//!
//! Non-goals
//! ---------
//! - This module is not a workflow engine. If you want a DAG, use Ray.
//! - It is not a prompt router. See `model_routing.rs`.
//! - It is not a chat interface. It is the machine-readable contract
//!   between user intent and Rope's internal modules.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The core primitive: what the user wants.
///
/// Every field is optional except `workload`. Rope fills in reasonable
/// defaults and the resolver's output makes every implicit choice explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: String,
    /// What computation to run. Mandatory.
    pub workload: Workload,
    /// Ceiling on money spent. If unset, no cap.
    pub budget: Option<Budget>,
    /// Latency expectations. If unset, best-effort.
    pub latency: Option<LatencyTarget>,
    /// Privacy constraints. Default: `OnDevicePreferred`.
    pub privacy: Privacy,
    /// Energy / sustainability preferences. Default: unconstrained.
    pub energy: EnergyPreference,
    /// Geographic or regulatory constraints.
    pub region: RegionConstraint,
    /// How long the intent is valid. Default: one hour.
    pub duration: Duration,
    /// Verification requirements: do we need proof-of-execution?
    pub verification: VerificationLevel,
    /// Free-form tags for user organization.
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl Intent {
    /// Minimal constructor: "run this workload, best effort, default everything."
    pub fn new(workload: Workload, created_by: &str) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            workload,
            budget: None,
            latency: None,
            privacy: Privacy::OnDevicePreferred,
            energy: EnergyPreference::Unconstrained,
            region: RegionConstraint::Any,
            duration: Duration::default(),
            verification: VerificationLevel::None,
            tags: Vec::new(),
            created_by: created_by.to_string(),
            created_at: Utc::now(),
        }
    }

    /// Fluent setters — writable in one line at the call site.
    pub fn with_budget(mut self, usd: f64, enforcement: BudgetEnforcement) -> Self {
        self.budget = Some(Budget {
            max_usd: usd,
            enforcement,
        });
        self
    }

    /// レイテンシ目標を設定 (ms)
    pub fn with_latency_ms(mut self, target_ms: u32, tolerance: LatencyTolerance) -> Self {
        self.latency = Some(LatencyTarget {
            p50_ms: target_ms,
            p99_ms: target_ms * 4,
            tolerance,
        });
        self
    }

    /// プライバシー要件を設定
    pub fn with_privacy(mut self, privacy: Privacy) -> Self {
        self.privacy = privacy;
        self
    }

    /// 検証レベルを設定
    pub fn with_verification(mut self, level: VerificationLevel) -> Self {
        self.verification = level;
        self
    }

    /// リージョン制約を設定
    pub fn with_region(mut self, region: RegionConstraint) -> Self {
        self.region = region;
        self
    }
}

/// What work is being requested.
///
/// Only a handful of verbs: Rope is not a general job scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Workload {
    /// Single inference call (prompt → completion).
    Inference {
        model: String,
        prompt_tokens_est: u32,
        max_output_tokens: u32,
    },
    /// Fine-tuning or continued pretraining.
    Train {
        base_model: String,
        dataset_uri: String,
        method: TrainingMethod,
        target_steps: u64,
    },
    /// RAG query over a document set.
    Retrieve {
        corpus_id: String,
        query: String,
        top_k: u32,
    },
    /// Batch inference job (many prompts).
    Batch {
        model: String,
        num_items: u64,
        avg_tokens_per_item: u32,
    },
    /// Long-running agent session.
    Agent {
        agent_id: String,
        expected_turns: u32,
    },
}

impl Workload {
    /// Rough cost-estimation hook — uses token counts to classify.
    pub fn complexity_class(&self) -> ComplexityClass {
        match self {
            Workload::Inference {
                prompt_tokens_est,
                max_output_tokens,
                ..
            } => {
                let total = prompt_tokens_est + max_output_tokens;
                if total < 2_000 {
                    ComplexityClass::Small
                } else if total < 32_000 {
                    ComplexityClass::Medium
                } else {
                    ComplexityClass::Large
                }
            }
            Workload::Batch {
                num_items,
                avg_tokens_per_item,
                ..
            } => {
                let total = num_items * (*avg_tokens_per_item as u64);
                if total < 1_000_000 {
                    ComplexityClass::Medium
                } else {
                    ComplexityClass::Large
                }
            }
            Workload::Train { target_steps, .. } => {
                if *target_steps < 1_000 {
                    ComplexityClass::Medium
                } else {
                    ComplexityClass::Large
                }
            }
            Workload::Retrieve { .. } => ComplexityClass::Small,
            Workload::Agent { expected_turns, .. } => {
                if *expected_turns < 10 {
                    ComplexityClass::Small
                } else {
                    ComplexityClass::Medium
                }
            }
        }
    }
}

/// Workload complexity bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComplexityClass {
    Small,
    Medium,
    Large,
}

/// Training-specific method selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingMethod {
    LoraAdapter,
    QloraAdapter,
    FullFineTune,
    ContinuedPretrain,
}

/// Spending ceiling with enforcement semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    pub max_usd: f64,
    pub enforcement: BudgetEnforcement,
}

/// How to react when the budget is about to be breached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetEnforcement {
    /// Refuse to start if estimated cost > budget.
    Hard,
    /// Start anyway, pause at 90%, checkpoint, require confirmation.
    Soft,
    /// Notify only.
    Warn,
}

/// Latency target with tolerance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyTarget {
    pub p50_ms: u32,
    pub p99_ms: u32,
    pub tolerance: LatencyTolerance,
}

/// How strict the latency requirement is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatencyTolerance {
    /// Fail if p99 is missed.
    Hard,
    /// Prefer but accept degradation.
    Soft,
    /// Best-effort informational.
    Informational,
}

/// Privacy / data-residency preference. This is the single most important
/// field for forward-looking AI: 2028+ users default to on-device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    /// Data may leave the device.
    AnyCompute,
    /// Prefer on-device, fall back to trusted peers, never hyperscaler.
    OnDevicePreferred,
    /// Must stay on this device.
    OnDeviceOnly,
    /// On-device or inside a trusted federation.
    FederatedOnly,
    /// Must run inside a confidential-compute enclave (TEE).
    ConfidentialCompute,
}

impl std::fmt::Display for Privacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Privacy::AnyCompute => write!(f, "制限なし"),
            Privacy::OnDevicePreferred => write!(f, "ローカル優先"),
            Privacy::OnDeviceOnly => write!(f, "ローカル限定"),
            Privacy::FederatedOnly => write!(f, "連邦のみ"),
            Privacy::ConfidentialCompute => write!(f, "TEE 必須"),
        }
    }
}

impl Privacy {
    /// Can this intent be served by a hyperscaler endpoint?
    pub fn allows_hyperscaler(&self) -> bool {
        matches!(self, Privacy::AnyCompute)
    }

    /// Must this intent stay on-device?
    pub fn requires_local(&self) -> bool {
        matches!(self, Privacy::OnDeviceOnly)
    }

    /// Does this intent require a TEE?
    pub fn requires_tee(&self) -> bool {
        matches!(self, Privacy::ConfidentialCompute)
    }
}

/// Energy / sustainability preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyPreference {
    Unconstrained,
    /// Prefer renewable grids when available.
    PreferRenewable,
    /// Only schedule when renewable mix is above threshold.
    RenewableOnly,
    /// Optimize for lowest watt-hours end-to-end.
    MinimizeWatts,
}

/// Region / regulatory constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegionConstraint {
    Any,
    /// Must run in a specified set of regions (e.g., EU only for GDPR).
    AllowList {
        regions: Vec<String>,
    },
    /// Must not run in a specified set (e.g., deny US for data-residency).
    DenyList {
        regions: Vec<String>,
    },
    /// Must comply with a named regulatory regime.
    Regime {
        regime: String,
    },
}

/// Temporal validity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Duration {
    pub max_seconds: u32,
    pub deadline: Option<DateTime<Utc>>,
}

impl Default for Duration {
    fn default() -> Self {
        Self {
            max_seconds: 3600,
            deadline: None,
        }
    }
}

/// Verification requirement for proof-of-execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationLevel {
    /// No verification required.
    None,
    /// Execution must be attested (node identity, model hash).
    Attested,
    /// ZK proof of correct execution.
    ZeroKnowledge,
    /// TEE attestation + ZK proof.
    Full,
}

impl std::fmt::Display for VerificationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationLevel::None => write!(f, "検証なし"),
            VerificationLevel::Attested => write!(f, "Attestation"),
            VerificationLevel::ZeroKnowledge => write!(f, "ZK 証明"),
            VerificationLevel::Full => write!(f, "完全検証"),
        }
    }
}

// =============================================================================
// Resolver: Intent → ExecutionPlan
// =============================================================================

/// The resolver's output: an explicit plan.
///
/// Every choice is made visible here. If the user asked "run Llama under
/// $5 with 200ms latency", the plan spells out: which modules compose
/// the answer, what the estimated cost is, which peer serves it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub intent_id: String,
    pub steps: Vec<PlanStep>,
    pub estimated_cost_usd: f64,
    pub estimated_latency_ms: u32,
    pub estimated_energy_wh: f64,
    pub selected_provider: ProviderChoice,
    pub selected_model_variant: String,
    pub optimizations: Vec<Optimization>,
    pub confidence: f64,
    pub feasible: bool,
    pub infeasibility_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One step of the plan, naming the Rope module that owns it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: u32,
    /// Name of the Rope module responsible (e.g., "spot", "kv_cache").
    pub module: String,
    /// Human-readable action.
    pub action: String,
    pub estimated_ms: u32,
    pub estimated_cost_usd: f64,
}

/// Who actually serves this intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderChoice {
    /// The user's own device.
    LocalDevice { device_id: String },
    /// A peer in the federation.
    FederatedPeer { peer_id: String, trust_score: f64 },
    /// The spot market.
    SpotMarket { market: String, provider_id: String },
    /// A hyperscaler endpoint (last resort).
    Hyperscaler { provider: String },
}

impl std::fmt::Display for ProviderChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderChoice::LocalDevice { .. } => write!(f, "ローカル"),
            ProviderChoice::FederatedPeer { peer_id, .. } => {
                write!(f, "ピア ({})", &peer_id[..8.min(peer_id.len())])
            }
            ProviderChoice::SpotMarket { provider_id, .. } => {
                write!(f, "スポット ({})", &provider_id[..8.min(provider_id.len())])
            }
            ProviderChoice::Hyperscaler { provider } => write!(f, "クラウド ({})", provider),
        }
    }
}

/// Applied optimizations (transparency for the user).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Optimization {
    Quantization,        // quantization.rs
    SpeculativeDecoding, // spec_decode.rs
    PrefixCaching,       // kv_cache.rs
    SemanticCache,       // semantic_cache.rs
    MoeExpertRouting,    // moe.rs
    MultiLora,           // lora.rs
    SpotPricing,         // spot.rs
    EdgeOffload,         // edge_grid.rs
}

impl std::fmt::Display for Optimization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Optimization::Quantization => write!(f, "量子化"),
            Optimization::SpeculativeDecoding => write!(f, "投機デコード"),
            Optimization::PrefixCaching => write!(f, "プレフィックスキャッシュ"),
            Optimization::SemanticCache => write!(f, "セマンティックキャッシュ"),
            Optimization::MoeExpertRouting => write!(f, "MoE ルーティング"),
            Optimization::MultiLora => write!(f, "マルチ LoRA"),
            Optimization::SpotPricing => write!(f, "スポット価格"),
            Optimization::EdgeOffload => write!(f, "エッジオフロード"),
        }
    }
}

// =============================================================================
// Manager
// =============================================================================

/// Intent lifecycle manager.
#[derive(Debug, Serialize, Deserialize)]
pub struct IntentManager {
    pub intents: Vec<Intent>,
    pub plans: Vec<ExecutionPlan>,
    pub history: Vec<Intent>,
    pub config: IntentConfig,
    pub stats: IntentStats,
    pub updated_at: DateTime<Utc>,
}

impl Default for IntentManager {
    fn default() -> Self {
        Self {
            intents: Vec::new(),
            plans: Vec::new(),
            history: Vec::new(),
            config: IntentConfig::default(),
            stats: IntentStats::default(),
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    pub enabled: bool,
    /// Default privacy when user omits it.
    pub default_privacy: Privacy,
    /// Default budget when user omits it (USD, 0 = no cap).
    pub default_budget_usd: f64,
    /// Default verification.
    pub default_verification: VerificationLevel,
    /// Maximum retained history entries.
    pub max_history: usize,
    /// Reject intents without a budget when estimated cost > this.
    pub require_budget_above_usd: f64,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_privacy: Privacy::OnDevicePreferred,
            default_budget_usd: 0.0,
            default_verification: VerificationLevel::None,
            max_history: 10_000,
            require_budget_above_usd: 10.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentStats {
    pub total_intents: u64,
    pub total_plans: u64,
    pub feasible_plans: u64,
    pub infeasible_plans: u64,
    pub served_locally: u64,
    pub served_federated: u64,
    pub served_spot: u64,
    pub served_hyperscaler: u64,
    pub total_estimated_cost_usd: f64,
    pub optimization_counts: BTreeMap<String, u64>,
}

impl IntentManager {
    /// Submit an intent. Does not plan yet.
    pub fn submit(&mut self, intent: Intent) -> Result<String> {
        if self.intents.iter().any(|i| i.id == intent.id) {
            anyhow::bail!("Intent 既登録: {}", intent.id);
        }
        // 入力検証: 空モデル名を拒否
        match &intent.workload {
            Workload::Inference { model, .. } => {
                if model.is_empty() {
                    anyhow::bail!("モデル名が空");
                }
            }
            Workload::Train { base_model, .. } => {
                if base_model.is_empty() {
                    anyhow::bail!("ベースモデル名が空");
                }
            }
            _ => {}
        }
        let id = intent.id.clone();
        self.intents.push(intent);
        self.stats.total_intents += 1;
        // 直近 50 件のみ保持 (I2: 100年運用でも intent.json が肥大しない)
        const MAX_INTENTS: usize = 50;
        if self.intents.len() > MAX_INTENTS {
            let drop = self.intents.len() - MAX_INTENTS;
            self.intents.drain(0..drop);
        }
        self.updated_at = Utc::now();
        Ok(id)
    }

    /// Resolve an intent into an explicit plan.
    ///
    /// This is deliberately synchronous and fast: the point is that the
    /// user sees exactly what will happen before it happens.
    pub fn resolve(&mut self, intent_id: &str) -> Result<ExecutionPlan> {
        let intent = self
            .intents
            .iter()
            .find(|i| i.id == intent_id)
            .context("Intent 無し")?
            .clone();

        let mut steps = Vec::new();
        let mut optimizations = Vec::new();
        let mut order = 0u32;
        let mut estimated_cost = 0.0;
        let mut estimated_ms = 0u32;
        let mut estimated_wh = 0.0;

        // Step 1: classify workload → pick model variant
        order += 1;
        let (variant, quant_applied) = Self::select_model_variant(&intent);
        if quant_applied {
            optimizations.push(Optimization::Quantization);
        }
        steps.push(PlanStep {
            order,
            module: "quantization".to_string(),
            action: format!("{} variant 選択", variant),
            estimated_ms: 5,
            estimated_cost_usd: 0.0,
        });

        // Step 2: pick provider honoring privacy
        order += 1;
        let provider = Self::select_provider(&intent);
        let (provider_ms, provider_cost) = Self::estimate_provider_cost(&provider, &intent);
        estimated_cost += provider_cost;
        estimated_ms += provider_ms;
        estimated_wh += Self::estimate_energy(&provider, &intent);

        match &provider {
            ProviderChoice::LocalDevice { .. } => {
                self.stats.served_locally += 1;
                steps.push(PlanStep {
                    order,
                    module: "edge_grid".to_string(),
                    action: "ローカルデバイスで実行".to_string(),
                    estimated_ms: provider_ms,
                    estimated_cost_usd: 0.0,
                });
                optimizations.push(Optimization::EdgeOffload);
            }
            ProviderChoice::FederatedPeer { peer_id, .. } => {
                self.stats.served_federated += 1;
                steps.push(PlanStep {
                    order,
                    module: "federation".to_string(),
                    action: format!("Delegate to peer {}", peer_id),
                    estimated_ms: provider_ms,
                    estimated_cost_usd: provider_cost,
                });
            }
            ProviderChoice::SpotMarket { market, .. } => {
                self.stats.served_spot += 1;
                steps.push(PlanStep {
                    order,
                    module: "spot".to_string(),
                    action: format!("Acquire GPU on {}", market),
                    estimated_ms: provider_ms,
                    estimated_cost_usd: provider_cost,
                });
                optimizations.push(Optimization::SpotPricing);
            }
            ProviderChoice::Hyperscaler { provider: p } => {
                self.stats.served_hyperscaler += 1;
                steps.push(PlanStep {
                    order,
                    module: "federation".to_string(),
                    action: format!("{} エンドポイント呼出", p),
                    estimated_ms: provider_ms,
                    estimated_cost_usd: provider_cost,
                });
            }
        }

        // Step 3: inference-time optimizations (extract: build_inference_steps)
        let (opt_steps, opt_list, cost_factor, ms_halved) =
            Self::build_inference_steps(&intent, order);
        for s in &opt_steps {
            order = order.max(s.order);
        }
        steps.extend(opt_steps);
        optimizations.extend(opt_list);
        estimated_cost *= cost_factor;
        if ms_halved > 0 {
            estimated_ms = (estimated_ms as f64 * 0.5) as u32;
        }

        // Step 4: verification
        if intent.verification != VerificationLevel::None {
            order += 1;
            let (module, verify_ms) = match intent.verification {
                VerificationLevel::Attested => ("verifiable", 50u32),
                VerificationLevel::ZeroKnowledge => ("verifiable", 500),
                VerificationLevel::Full => ("confidential", 800),
                VerificationLevel::None => ("", 0), // guard: 外側 if で除外済
            };
            if !module.is_empty() {
                steps.push(PlanStep {
                    order,
                    module: module.to_string(),
                    action: format!("{} proof 生成", intent.verification),
                    estimated_ms: verify_ms,
                    estimated_cost_usd: 0.01,
                });
                estimated_cost += 0.01;
                estimated_ms += verify_ms;
            }
        }

        // Feasibility check
        let (feasible, reason) = self.check_feasibility(&intent, estimated_cost, estimated_ms);

        let plan = ExecutionPlan {
            id: uuid::Uuid::now_v7().to_string(),
            intent_id: intent_id.to_string(),
            steps,
            estimated_cost_usd: estimated_cost,
            estimated_latency_ms: estimated_ms,
            estimated_energy_wh: estimated_wh,
            selected_provider: provider,
            selected_model_variant: variant,
            optimizations: optimizations.clone(),
            confidence: 0.85,
            feasible,
            infeasibility_reason: reason,
            created_at: Utc::now(),
        };

        if feasible {
            self.stats.feasible_plans += 1;
            for opt in &optimizations {
                let key = format!("{}", opt);
                *self.stats.optimization_counts.entry(key).or_insert(0) += 1;
            }
        } else {
            self.stats.infeasible_plans += 1;
        }

        self.stats.total_plans += 1;
        self.stats.total_estimated_cost_usd += estimated_cost;
        self.plans.push(plan.clone());
        self.updated_at = Utc::now();
        Ok(plan)
    }

    fn select_model_variant(intent: &Intent) -> (String, bool) {
        match &intent.workload {
            Workload::Inference { model, .. } | Workload::Batch { model, .. } => {
                // Pick quantization based on privacy (on-device benefits most).
                let quant = matches!(
                    intent.privacy,
                    Privacy::OnDeviceOnly | Privacy::OnDevicePreferred
                );
                if quant {
                    (format!("{}-int4", model), true)
                } else {
                    (model.clone(), false)
                }
            }
            Workload::Train { base_model, .. } => (base_model.clone(), false),
            Workload::Retrieve { .. } => ("embed-small".to_string(), false),
            Workload::Agent { agent_id, .. } => (format!("agent-{}", agent_id), false),
        }
    }

    fn select_provider(intent: &Intent) -> ProviderChoice {
        // The provider cascade encodes the Apple-like preference:
        // on-device → federation → spot → hyperscaler.
        if intent.privacy.requires_local() {
            return ProviderChoice::LocalDevice {
                device_id: "this-device".to_string(),
            };
        }
        if intent.privacy.requires_tee() {
            return ProviderChoice::FederatedPeer {
                peer_id: "tee-peer".to_string(),
                trust_score: 1.0,
            };
        }

        let complexity = intent.workload.complexity_class();
        match (intent.privacy, complexity) {
            (Privacy::OnDevicePreferred, ComplexityClass::Small) => ProviderChoice::LocalDevice {
                device_id: "this-device".to_string(),
            },
            (Privacy::OnDevicePreferred, _) | (Privacy::FederatedOnly, _) => {
                ProviderChoice::FederatedPeer {
                    peer_id: "peer-1".to_string(),
                    trust_score: 0.9,
                }
            }
            (Privacy::AnyCompute, ComplexityClass::Large) => ProviderChoice::SpotMarket {
                market: "rope-spot".to_string(),
                provider_id: "provider-1".to_string(),
            },
            (Privacy::AnyCompute, _) => ProviderChoice::SpotMarket {
                market: "rope-spot".to_string(),
                provider_id: "provider-1".to_string(),
            },
            _ => ProviderChoice::LocalDevice {
                device_id: "this-device".to_string(),
            },
        }
    }

    fn estimate_provider_cost(provider: &ProviderChoice, intent: &Intent) -> (u32, f64) {
        let complexity_mult = match intent.workload.complexity_class() {
            ComplexityClass::Small => 1.0,
            ComplexityClass::Medium => 10.0,
            ComplexityClass::Large => 100.0,
        };
        match provider {
            ProviderChoice::LocalDevice { .. } => ((50.0 * complexity_mult) as u32, 0.0),
            ProviderChoice::FederatedPeer { .. } => {
                ((80.0 * complexity_mult) as u32, 0.001 * complexity_mult)
            }
            ProviderChoice::SpotMarket { .. } => {
                ((120.0 * complexity_mult) as u32, 0.01 * complexity_mult)
            }
            ProviderChoice::Hyperscaler { .. } => {
                ((100.0 * complexity_mult) as u32, 0.05 * complexity_mult)
            }
        }
    }

    fn estimate_energy(provider: &ProviderChoice, intent: &Intent) -> f64 {
        let base = match intent.workload.complexity_class() {
            ComplexityClass::Small => 0.01,
            ComplexityClass::Medium => 0.5,
            ComplexityClass::Large => 10.0,
        };
        let mult = match provider {
            ProviderChoice::LocalDevice { .. } => 0.5,
            ProviderChoice::FederatedPeer { .. } => 0.8,
            ProviderChoice::SpotMarket { .. } => 1.0,
            ProviderChoice::Hyperscaler { .. } => 1.2,
        };
        base * mult
    }

    /// 推論時最適化ステップ構築 (resolve() から抽出)
    ///
    /// speculative decoding / semantic cache / prefix caching の自動適用判定。
    /// Carmack 原則: 50 行超の処理は関数抽出。
    fn build_inference_steps(
        intent: &Intent,
        base_order: u32,
    ) -> (Vec<PlanStep>, Vec<Optimization>, f64, u32) {
        let mut steps = Vec::new();
        let mut opts = Vec::new();
        let mut order = base_order;
        let mut cost_factor = 1.0_f64;
        let mut ms_factor = 1.0_f64;

        if !matches!(
            intent.workload,
            Workload::Inference { .. } | Workload::Batch { .. }
        ) {
            return (steps, opts, cost_factor, 0);
        }

        // 低レイテンシ要求 → speculative decoding
        if intent.latency.as_ref().map_or(false, |l| l.p50_ms < 150) {
            order += 1;
            steps.push(PlanStep {
                order,
                module: "spec_decode".to_string(),
                action: "Speculative decoding (Eagle-3)".to_string(),
                estimated_ms: 0,
                estimated_cost_usd: 0.0,
            });
            opts.push(Optimization::SpeculativeDecoding);
            ms_factor *= 0.5;
        }

        // バッチ処理 → semantic cache
        if matches!(intent.workload, Workload::Batch { .. }) {
            order += 1;
            steps.push(PlanStep {
                order,
                module: "semantic_cache".to_string(),
                action: "セマンティックキャッシュ重複除去".to_string(),
                estimated_ms: 0,
                estimated_cost_usd: 0.0,
            });
            opts.push(Optimization::SemanticCache);
            cost_factor *= 0.7;
        }

        // 常時: prefix caching
        order += 1;
        steps.push(PlanStep {
            order,
            module: "kv_cache".to_string(),
            action: "PagedAttention プレフィックスキャッシュ".to_string(),
            estimated_ms: 0,
            estimated_cost_usd: 0.0,
        });
        opts.push(Optimization::PrefixCaching);

        let ms_delta = if ms_factor < 1.0 { 1 } else { 0 }; // flag for caller
        (steps, opts, cost_factor, ms_delta)
    }

    fn check_feasibility(
        &self,
        intent: &Intent,
        cost: f64,
        latency: u32,
    ) -> (bool, Option<String>) {
        if let Some(budget) = &intent.budget {
            if matches!(budget.enforcement, BudgetEnforcement::Hard) && cost > budget.max_usd {
                return (
                    false,
                    Some(format!(
                        "推定 ${:.4} がハード予算 ${:.4} を超過",
                        cost, budget.max_usd
                    )),
                );
            }
        }
        if self.config.require_budget_above_usd > 0.0
            && cost > self.config.require_budget_above_usd
            && intent.budget.is_none()
        {
            return (
                false,
                Some(format!(
                    "推定コスト ${:.4} が閾値 ${:.4} を超過 (予算未設定)",
                    cost, self.config.require_budget_above_usd
                )),
            );
        }
        if let Some(latency_target) = &intent.latency {
            if matches!(latency_target.tolerance, LatencyTolerance::Hard)
                && latency > latency_target.p99_ms
            {
                return (
                    false,
                    Some(format!(
                        "推定 {}ms がハード p99 目標 {}ms を超過",
                        latency, latency_target.p99_ms
                    )),
                );
            }
        }
        (true, None)
    }

    /// Retrieve a plan by its ID.
    pub fn get_plan(&self, plan_id: &str) -> Option<&ExecutionPlan> {
        self.plans.iter().find(|p| p.id == plan_id)
    }

    /// Archive an intent to history.
    pub fn complete(&mut self, intent_id: &str) -> Result<()> {
        let idx = self
            .intents
            .iter()
            .position(|i| i.id == intent_id)
            .context("Intent 無し")?;
        let intent = self.intents.remove(idx);
        self.history.push(intent);
        if self.history.len() > self.config.max_history {
            self.history.drain(..100);
        }
        self.updated_at = Utc::now();
        Ok(())
    }
}

// Storage
/// intent マネージャーの永続化パスを返す
pub fn intent_path() -> std::path::PathBuf {
    crate::core::config::config_dir().join("intent.json")
}

/// intent マネージャーを永続化ファイルから読込
pub fn load_intent() -> Result<IntentManager> {
    Ok(crate::core::config::load_or_recover(
        &intent_path(),
        "Intent 履歴 (intent.json)",
    ))
}

/// intent マネージャーを永続化ファイルに保存
pub fn save_intent(m: &IntentManager) -> Result<()> {
    crate::core::config::atomic_write(&intent_path(), &serde_json::to_string_pretty(m)?)
}

/// ExecutionPlan をターミナル表示用文字列に変換
pub fn format_plan(plan: &ExecutionPlan) -> String {
    let mut out = String::new();
    out.push_str("実行計画:\n");
    out.push_str("═══════════════════════════════════════════════════════════\n");
    if !plan.feasible {
        out.push_str(&format!(
            "❌ 実行不可: {}\n",
            plan.infeasibility_reason.as_deref().unwrap_or("unknown")
        ));
        return out;
    }
    out.push_str(&format!("プロバイダ: {}\n", plan.selected_provider));
    out.push_str(&format!("モデル: {}\n", plan.selected_model_variant));
    out.push_str(&format!("推定コスト: ${:.4}\n", plan.estimated_cost_usd));
    out.push_str(&format!("推定遅延: {}ms\n", plan.estimated_latency_ms));
    out.push_str(&format!("推定消費: {:.3} Wh\n", plan.estimated_energy_wh));
    out.push_str("\nステップ:\n");
    for step in &plan.steps {
        out.push_str(&format!(
            "  {}. [{}] {} ({}ms, ${:.4})\n",
            step.order, step.module, step.action, step.estimated_ms, step.estimated_cost_usd
        ));
    }
    out.push_str("\n適用最適化:\n");
    for opt in &plan.optimizations {
        out.push_str(&format!("  ✓ {}\n", opt));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_inference() -> Intent {
        Intent::new(
            Workload::Inference {
                model: "llama-70b".to_string(),
                prompt_tokens_est: 500,
                max_output_tokens: 200,
            },
            "monu",
        )
    }

    #[test]
    fn test_intent_construction() {
        let i = sample_inference()
            .with_budget(5.0, BudgetEnforcement::Hard)
            .with_latency_ms(200, LatencyTolerance::Soft)
            .with_privacy(Privacy::OnDeviceOnly);
        assert!(i.budget.is_some());
        assert_eq!(i.privacy, Privacy::OnDeviceOnly);
    }

    #[test]
    fn test_privacy_cascade_local_only() {
        let mut m = IntentManager::default();
        let i = sample_inference().with_privacy(Privacy::OnDeviceOnly);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(matches!(
            plan.selected_provider,
            ProviderChoice::LocalDevice { .. }
        ));
        // ローカル実行 + verification なし → コストゼロ (PrefixCaching は無料)
        assert_eq!(plan.estimated_cost_usd, 0.0);
    }

    #[test]
    fn test_hard_budget_makes_infeasible() {
        let mut m = IntentManager::default();
        let i = sample_inference()
            .with_privacy(Privacy::AnyCompute)
            .with_budget(0.00001, BudgetEnforcement::Hard);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(!plan.feasible);
        assert!(plan.infeasibility_reason.is_some());
    }

    #[test]
    fn test_quantization_applied_on_device() {
        let mut m = IntentManager::default();
        let i = sample_inference().with_privacy(Privacy::OnDeviceOnly);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(plan.selected_model_variant.ends_with("-int4"));
        assert!(plan.optimizations.contains(&Optimization::Quantization));
    }

    #[test]
    fn test_speculative_decoding_on_low_latency() {
        let mut m = IntentManager::default();
        let i = sample_inference()
            .with_privacy(Privacy::AnyCompute)
            .with_latency_ms(100, LatencyTolerance::Soft);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(plan
            .optimizations
            .contains(&Optimization::SpeculativeDecoding));
    }

    #[test]
    fn test_verification_adds_step() {
        let mut m = IntentManager::default();
        let i = sample_inference().with_verification(VerificationLevel::ZeroKnowledge);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(plan.steps.iter().any(|s| s.action.contains("proof")));
    }

    #[test]
    fn test_complexity_classification() {
        let small = Workload::Inference {
            model: "m".to_string(),
            prompt_tokens_est: 100,
            max_output_tokens: 100,
        };
        assert_eq!(small.complexity_class(), ComplexityClass::Small);

        let large = Workload::Batch {
            model: "m".to_string(),
            num_items: 1_000_000,
            avg_tokens_per_item: 100,
        };
        assert_eq!(large.complexity_class(), ComplexityClass::Large);
    }

    #[test]
    fn test_privacy_helpers() {
        assert!(Privacy::OnDeviceOnly.requires_local());
        assert!(!Privacy::OnDeviceOnly.allows_hyperscaler());
        assert!(Privacy::ConfidentialCompute.requires_tee());
        assert!(Privacy::AnyCompute.allows_hyperscaler());
    }

    #[test]
    fn test_batch_enables_semantic_cache() {
        let mut m = IntentManager::default();
        let i = Intent::new(
            Workload::Batch {
                model: "llama".to_string(),
                num_items: 10_000,
                avg_tokens_per_item: 200,
            },
            "test",
        )
        .with_privacy(Privacy::AnyCompute);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(plan.optimizations.contains(&Optimization::SemanticCache));
    }

    #[test]
    fn test_federation_for_medium_workload() {
        let mut m = IntentManager::default();
        let i = sample_inference(); // default privacy is OnDevicePreferred
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        // 500+200 = 700 tokens = Small → LocalDevice
        assert!(matches!(
            plan.selected_provider,
            ProviderChoice::LocalDevice { .. }
        ));
    }

    #[test]
    fn test_tee_requirement_routes_to_confidential_peer() {
        let mut m = IntentManager::default();
        let i = sample_inference().with_privacy(Privacy::ConfidentialCompute);
        let id = m.submit(i).unwrap();
        let plan = m.resolve(&id).unwrap();
        assert!(matches!(
            plan.selected_provider,
            ProviderChoice::FederatedPeer { .. }
        ));
    }

    // ====== Round 23: extracted helper tests ======

    #[test]
    fn test_build_inference_steps_includes_prefix_caching() {
        let i = sample_inference();
        let (steps, opts, _, _) = IntentManager::build_inference_steps(&i, 0);
        assert!(
            opts.contains(&Optimization::PrefixCaching),
            "推論ワークロードは常に PrefixCaching を含むべき"
        );
        assert!(steps.iter().any(|s| s.module == "kv_cache"));
    }

    #[test]
    fn test_build_inference_steps_skip_for_training() {
        let i = Intent::new(
            Workload::Train {
                base_model: "llama".to_string(),
                dataset_uri: "s3://data".to_string(),
                method: TrainingMethod::LoraAdapter,
                target_steps: 1000,
            },
            "test",
        );
        let (steps, opts, _, _) = IntentManager::build_inference_steps(&i, 0);
        assert!(steps.is_empty(), "Training ワークロードには推論最適化不要");
        assert!(opts.is_empty());
    }

    /// Display はユーザー向け日本語 — Debug 形式ではない
    #[test]
    fn test_privacy_display() {
        assert_eq!(format!("{}", Privacy::ConfidentialCompute), "TEE 必須");
        assert_eq!(format!("{}", Privacy::OnDeviceOnly), "ローカル限定");
        assert_eq!(format!("{}", Privacy::AnyCompute), "制限なし");
        assert_ne!(
            format!("{}", Privacy::ConfidentialCompute),
            format!("{:?}", Privacy::ConfidentialCompute)
        );
    }

    #[test]
    fn test_provider_choice_display() {
        let local = ProviderChoice::LocalDevice {
            device_id: "dev-abc".to_string(),
        };
        assert_eq!(format!("{}", local), "ローカル");

        let peer = ProviderChoice::FederatedPeer {
            peer_id: "peer-12345678-long".to_string(),
            trust_score: 0.9,
        };
        assert!(format!("{}", peer).contains("ピア"));

        let hyper = ProviderChoice::Hyperscaler {
            provider: "AWS".to_string(),
        };
        assert!(format!("{}", hyper).contains("AWS"));
    }

    #[test]
    fn test_optimization_display() {
        assert_eq!(format!("{}", Optimization::Quantization), "量子化");
        assert_eq!(
            format!("{}", Optimization::SpeculativeDecoding),
            "投機デコード"
        );
        // join で並べた時に読めること
        let opts = [Optimization::Quantization, Optimization::PrefixCaching];
        let joined = opts
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(joined, "量子化, プレフィックスキャッシュ");
    }

    // ====== Round 26: 境界値テスト ======

    /// 空モデル名 → submit 拒否
    #[test]
    fn test_empty_model_rejected() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Inference {
                model: "".to_string(),
                prompt_tokens_est: 10,
                max_output_tokens: 100,
            },
            "user",
        );
        assert!(mgr.submit(intent).is_err());
    }

    /// 空ベースモデル名 (Train) → submit 拒否
    #[test]
    fn test_empty_base_model_rejected() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Train {
                base_model: "".to_string(),
                dataset_uri: "s3://data".to_string(),
                method: TrainingMethod::FullFineTune,
                target_steps: 1000,
            },
            "user",
        );
        assert!(mgr.submit(intent).is_err());
    }

    /// intent キューは MAX_INTENTS で頭打ち (無限蓄積しない)
    #[test]
    fn test_intent_queue_capped() {
        let mut mgr = IntentManager::default();
        for i in 0..60 {
            let intent = Intent::new(
                Workload::Inference {
                    model: format!("m{}", i),
                    prompt_tokens_est: 10,
                    max_output_tokens: 100,
                },
                "user",
            );
            mgr.submit(intent).unwrap();
        }
        // 60 件 submit しても 50 件まで
        assert!(mgr.intents.len() <= 50, "intent キューは 50 件上限");
        // 累計カウントは正しく 60
        assert_eq!(mgr.stats.total_intents, 60);
    }

    /// 同じ Intent 2 回 submit → 拒否
    #[test]
    fn test_duplicate_submit_rejected() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Inference {
                model: "llama".to_string(),
                prompt_tokens_est: 10,
                max_output_tokens: 100,
            },
            "user",
        );
        // 1 回目は成功、同一 Intent の 2 回目は重複でエラー
        mgr.submit(intent.clone()).unwrap();
        assert!(mgr.submit(intent).is_err());
    }

    #[test]
    fn test_format_plan_contains_key_fields() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Inference {
                model: "llama-3.2-1b".to_string(),
                prompt_tokens_est: 10,
                max_output_tokens: 50,
            },
            "test",
        );
        let id = mgr.submit(intent).unwrap();
        let plan = mgr.resolve(&id).unwrap();
        let out = format_plan(&plan);
        assert!(out.contains('═'), "ヘッダ罫線");
        assert!(out.contains("プロバイダ"), "プロバイダ");
        assert!(out.contains('$'), "コスト");
        assert!(out.contains("ステップ"), "ステップ");
    }

    #[test]
    fn test_format_plan_uses_display_not_debug() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Inference {
                model: "m".to_string(),
                prompt_tokens_est: 5,
                max_output_tokens: 10,
            },
            "u",
        );
        let id = mgr.submit(intent).unwrap();
        let plan = mgr.resolve(&id).unwrap();
        let out = format_plan(&plan);
        // Display 出力 "ローカル" を使用、Debug 出力 "LocalDevice" ではない
        assert!(out.contains("ローカル"), "Display impl 使用");
        assert!(!out.contains("LocalDevice"), "Debug 出力は不可");
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut mgr = IntentManager::default();
        let intent = Intent::new(
            Workload::Inference {
                model: "m".into(),
                prompt_tokens_est: 5,
                max_output_tokens: 10,
            },
            "u",
        );
        mgr.submit(intent).unwrap();
        let json1 = serde_json::to_string(&mgr).unwrap();
        let back: IntentManager = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "IntentManager roundtrip");
    }
}
