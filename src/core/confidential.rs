//! Confidential Computing - GPU TEEサポート
//!
//! NVIDIA H100/H200 Confidential Computing、セキュアエンクレーブ、アテステーション
//!
//! ## 背景
//! - 2026年: 70%の企業がTEEを採用
//! - NVIDIA Hopper/Blackwell: GPU TEEをサポート
//! - データ保護: 処理中のデータも暗号化
//!
//! ## 機能
//! - GPU TEE管理
//! - アテステーション（リモート検証）
//! - セキュアエンクレーブ
//! - 暗号化メモリ
//! - ゼロトラストアーキテクチャ

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Confidential Computingマネージャー
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfidentialManager {
    /// TEEインスタンス
    pub tee_instances: Vec<TeeInstance>,
    /// アテステーションレポート
    pub attestation_reports: Vec<AttestationReport>,
    /// セキュアセッション
    pub secure_sessions: Vec<SecureSession>,
    /// ポリシー
    pub policies: Vec<SecurityPolicy>,
    /// 設定
    pub config: ConfidentialConfig,
    /// 統計
    pub stats: ConfidentialStats,
    /// 最終更新
    pub updated_at: DateTime<Utc>,
}

impl Default for ConfidentialManager {
    fn default() -> Self {
        Self {
            tee_instances: Vec::new(),
            attestation_reports: Vec::new(),
            secure_sessions: Vec::new(),
            policies: Vec::new(),
            config: ConfidentialConfig::default(),
            stats: ConfidentialStats::default(),
            updated_at: Utc::now(),
        }
    }
}

/// TEEインスタンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeInstance {
    /// インスタンスID
    pub id: String,
    /// TEEタイプ
    pub tee_type: TeeType,
    /// GPUタイプ
    pub gpu_type: String,
    /// GPU ID
    pub gpu_id: String,
    /// ステータス
    pub status: TeeStatus,
    /// セキュリティレベル
    pub security_level: SecurityLevel,
    /// アテステーション状態
    pub attestation_status: AttestationStatus,
    /// 最終アテステーション時刻
    pub last_attestation: Option<DateTime<Utc>>,
    /// メモリ暗号化有効
    pub memory_encryption: bool,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// メタデータ
    pub metadata: TeeMetadata,
}

/// TEEタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeeType {
    /// NVIDIA GPU TEE (H100/H200/Blackwell)
    NvidiaGpuTee,
    /// Intel SGX
    IntelSgx,
    /// Intel TDX
    IntelTdx,
    /// AMD SEV-SNP
    AmdSevSnp,
    /// ARM CCA
    ArmCca,
}

impl std::fmt::Display for TeeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeeType::NvidiaGpuTee => write!(f, "NVIDIA GPU TEE"),
            TeeType::IntelSgx => write!(f, "Intel SGX"),
            TeeType::IntelTdx => write!(f, "Intel TDX"),
            TeeType::AmdSevSnp => write!(f, "AMD SEV-SNP"),
            TeeType::ArmCca => write!(f, "ARM CCA"),
        }
    }
}

impl TeeType {
    /// GPU対応かどうか
    /// GPU メモリ内で機密計算を保証できる TEE か
    ///
    /// arXiv:2507.02770 (NVIDIA GPU CC Demystified) 準拠:
    /// GPU TEE は NVIDIA Hopper/Blackwell の GPU 内 TEE のみが提供する。
    /// Intel TDX / AMD SEV-SNP は **CPU TEE** であり、ホストメモリは守るが
    /// GPU メモリ単独の機密性は保証しない (composite attestation が別途必要)。
    pub fn supports_gpu(&self) -> bool {
        matches!(self, TeeType::NvidiaGpuTee)
    }

    /// CPU TEE か (composite attestation の root として機能)
    ///
    /// GPU TEE は CPU TEE root と組み合わせて初めて完全な保証となる。
    pub fn is_cpu_tee(&self) -> bool {
        matches!(
            self,
            TeeType::IntelTdx | TeeType::AmdSevSnp | TeeType::IntelSgx
        )
    }

    /// アテステーションサービスURL
    pub fn attestation_service_url(&self) -> Option<&'static str> {
        match self {
            TeeType::NvidiaGpuTee => Some("https://nras.attestation.nvidia.com"),
            TeeType::IntelSgx => Some("https://api.trustedservices.intel.com/sgx/attestation"),
            TeeType::IntelTdx => Some("https://api.trustedservices.intel.com/tdx/attestation"),
            TeeType::AmdSevSnp => None, // オンプレミス検証
            TeeType::ArmCca => None,
        }
    }
}

/// TEEステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeeStatus {
    /// 初期化中
    Initializing,
    /// 準備完了
    Ready,
    /// 稼働中
    Running,
    /// 一時停止
    Paused,
    /// エラー
    Error,
    /// 終了
    Terminated,
}

impl TeeStatus {
    /// 状態アイコンを返す
    pub fn icon(&self) -> &'static str {
        match self {
            TeeStatus::Initializing => "🔄",
            TeeStatus::Ready => "✅",
            TeeStatus::Running => "🟢",
            TeeStatus::Paused => "⏸️",
            TeeStatus::Error => "🔴",
            TeeStatus::Terminated => "⚪",
        }
    }
}

/// セキュリティレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecurityLevel {
    /// 基本（メモリ暗号化のみ）
    Basic,
    /// 標準（+ アテステーション）
    Standard,
    /// 高（+ セキュアブート）
    High,
    /// 最高（+ ハードウェアルートオブトラスト）
    Maximum,
}

impl std::fmt::Display for SecurityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityLevel::Basic => write!(f, "基本"),
            SecurityLevel::Standard => write!(f, "標準"),
            SecurityLevel::High => write!(f, "高"),
            SecurityLevel::Maximum => write!(f, "最高"),
        }
    }
}

/// アテステーションステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AttestationStatus {
    /// 未検証
    Unverified,
    /// 検証中
    Verifying,
    /// 検証済み
    Verified,
    /// 検証失敗
    Failed,
    /// 期限切れ
    Expired,
}

impl AttestationStatus {
    /// 状態アイコンを返す
    pub fn icon(&self) -> &'static str {
        match self {
            AttestationStatus::Unverified => "❓",
            AttestationStatus::Verifying => "🔄",
            AttestationStatus::Verified => "✅",
            AttestationStatus::Failed => "❌",
            AttestationStatus::Expired => "⏰",
        }
    }
}

/// TEEメタデータ
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeeMetadata {
    /// ファームウェアバージョン
    pub firmware_version: Option<String>,
    /// TCBバージョン
    pub tcb_version: Option<String>,
    /// セキュリティパッチレベル
    pub security_patch_level: Option<String>,
    /// ハードウェアID
    pub hardware_id: Option<String>,
    /// 証明書チェーン
    pub certificate_chain: Option<String>,
}

/// アテステーションレポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    /// レポートID
    pub id: String,
    /// TEEインスタンスID
    pub tee_instance_id: String,
    /// レポートタイプ
    pub report_type: AttestationReportType,
    /// 検証結果
    pub verification_result: VerificationResult,
    /// 測定値
    pub measurements: Measurements,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 有効期限
    pub expires_at: DateTime<Utc>,
    /// 署名
    pub signature: Option<String>,
    /// 生レポートデータ
    pub raw_report: Option<String>,
}

/// アテステーションレポートタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestationReportType {
    /// ローカルアテステーション
    Local,
    /// リモートアテステーション
    Remote,
    /// プラットフォームアテステーション
    Platform,
}

/// 検証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 成功
    pub success: bool,
    /// 詳細
    pub details: String,
    /// 警告
    pub warnings: Vec<String>,
    /// エラー
    pub errors: Vec<String>,
    /// 信頼スコア（0-100）
    pub trust_score: u32,
}

/// 測定値
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Measurements {
    /// プラットフォーム構成レジスタ（PCR）
    pub pcr_values: HashMap<u32, String>,
    /// エンクレーブ測定値
    pub enclave_measurement: Option<String>,
    /// セキュアブート状態
    pub secure_boot_enabled: bool,
    /// デバッグモード
    pub debug_mode: bool,
    /// 仮想化拡張
    pub virtualization_extensions: bool,
}

/// セキュアセッション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureSession {
    /// セッションID
    pub id: String,
    /// TEEインスタンスID
    pub tee_instance_id: String,
    /// ユーザーID
    pub user_id: String,
    /// ステータス
    pub status: SessionStatus,
    /// 暗号化アルゴリズム
    pub encryption_algorithm: EncryptionAlgorithm,
    /// セッションキー（暗号化済み）
    pub encrypted_session_key: Option<String>,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 最終アクティブ
    pub last_active: DateTime<Utc>,
    /// 有効期限
    pub expires_at: DateTime<Utc>,
}

/// セッションステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    /// 確立中
    Establishing,
    /// アクティブ
    Active,
    /// 一時停止
    Suspended,
    /// 終了
    Terminated,
}

/// 暗号化アルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM
    Aes256Gcm,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
    /// AES-256-CBC
    Aes256Cbc,
}

impl std::fmt::Display for EncryptionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionAlgorithm::Aes256Gcm => write!(f, "AES-256-GCM"),
            EncryptionAlgorithm::ChaCha20Poly1305 => write!(f, "ChaCha20-Poly1305"),
            EncryptionAlgorithm::Aes256Cbc => write!(f, "AES-256-CBC"),
        }
    }
}

/// セキュリティポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// ポリシーID
    pub id: String,
    /// 名前
    pub name: String,
    /// 説明
    pub description: String,
    /// 最小セキュリティレベル
    pub min_security_level: SecurityLevel,
    /// アテステーション必須
    pub attestation_required: bool,
    /// アテステーション有効期間（秒）
    pub attestation_validity_seconds: u32,
    /// 許可されたTEEタイプ
    pub allowed_tee_types: Vec<TeeType>,
    /// デバッグモード許可
    pub allow_debug_mode: bool,
    /// 最小TCBバージョン
    pub min_tcb_version: Option<String>,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 有効
    pub enabled: bool,
}

/// Confidential Computing設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidentialConfig {
    /// デフォルトTEEタイプ
    pub default_tee_type: TeeType,
    /// デフォルトセキュリティレベル
    pub default_security_level: SecurityLevel,
    /// アテステーション自動更新
    pub auto_attestation: bool,
    /// アテステーション更新間隔（秒）
    pub attestation_interval_seconds: u32,
    /// デフォルト暗号化アルゴリズム
    pub default_encryption: EncryptionAlgorithm,
    /// セッションタイムアウト（秒）
    pub session_timeout_seconds: u32,
    /// ログ暗号化
    pub encrypt_logs: bool,
    /// NVIDIAアテステーションサービス
    pub nvidia_attestation_enabled: bool,
}

impl Default for ConfidentialConfig {
    fn default() -> Self {
        Self {
            default_tee_type: TeeType::NvidiaGpuTee,
            default_security_level: SecurityLevel::Standard,
            auto_attestation: true,
            attestation_interval_seconds: 3600, // 1時間
            default_encryption: EncryptionAlgorithm::Aes256Gcm,
            session_timeout_seconds: 86400, // 24時間
            encrypt_logs: true,
            nvidia_attestation_enabled: true,
        }
    }
}

/// Confidential Computing統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfidentialStats {
    /// 総TEEインスタンス
    pub total_tee_instances: u32,
    /// アクティブTEEインスタンス
    pub active_tee_instances: u32,
    /// 総アテステーション
    pub total_attestations: u64,
    /// 成功アテステーション
    pub successful_attestations: u64,
    /// 失敗アテステーション
    pub failed_attestations: u64,
    /// アクティブセッション
    pub active_sessions: u32,
    /// 暗号化データ量（GB）
    pub encrypted_data_gb: f64,
}

impl ConfidentialManager {
    /// TEEインスタンスを作成
    pub fn create_tee_instance(
        &mut self,
        gpu_id: &str,
        gpu_type: &str,
        tee_type: TeeType,
        security_level: SecurityLevel,
    ) -> TeeInstance {
        let instance = TeeInstance {
            id: uuid::Uuid::now_v7().to_string(),
            tee_type,
            gpu_type: gpu_type.to_string(),
            gpu_id: gpu_id.to_string(),
            status: TeeStatus::Initializing,
            security_level,
            attestation_status: AttestationStatus::Unverified,
            last_attestation: None,
            memory_encryption: true,
            created_at: Utc::now(),
            metadata: TeeMetadata::default(),
        };

        self.tee_instances.push(instance.clone());
        self.stats.total_tee_instances += 1;
        self.updated_at = Utc::now();

        instance
    }

    /// アテステーションを実行
    ///
    /// 5 段階の検証を直列実行し、全結果を `AttestationReport` に集約。
    /// 各段階は private helper に分離 (G4.S: 関数 ≤40 行)。
    pub fn perform_attestation(&mut self, instance_id: &str) -> Result<AttestationReport> {
        let instance = self
            .tee_instances
            .iter_mut()
            .find(|i| i.id == instance_id)
            .context("TEE インスタンス無し")?;
        instance.attestation_status = AttestationStatus::Verifying;

        let gpu_lower = instance.gpu_type.to_lowercase();
        let tee_type = instance.tee_type;
        let security_level = instance.security_level;
        let memory_encryption = instance.memory_encryption;
        let gpu_type = instance.gpu_type.clone();
        let inst_id = instance.id.clone();

        // 5 段階検証
        let (mut warnings, mut errors, mut trust) =
            Self::validate_tee_gpu_compat(tee_type, &gpu_lower, &gpu_type);
        let (w2, e2, t2) = Self::validate_security_posture(memory_encryption, security_level);
        warnings.extend(w2);
        errors.extend(e2);
        trust = trust.min(t2);

        let (w3, t3) = self.detect_replay_anomaly(instance_id);
        warnings.extend(w3);
        trust = trust.saturating_sub(100 - t3); // t3 is penalty

        let success = errors.is_empty() && trust >= 50;
        let measurements = Self::build_measurements(tee_type, &inst_id, memory_encryption);
        // report id を先に確定し、署名 nonce に流用 (再 attestation で digest が変わる)。
        let report_id = uuid::Uuid::now_v7().to_string();
        let signature =
            Self::build_evidence_signature(&inst_id, tee_type, success, trust, &report_id);

        let report = AttestationReport {
            id: report_id,
            tee_instance_id: instance_id.to_string(),
            report_type: AttestationReportType::Remote,
            verification_result: VerificationResult {
                success,
                details: if success {
                    format!("Validated {} on {} (trust {})", tee_type, gpu_type, trust)
                } else {
                    format!("Validation failed: {}", errors.join("; "))
                },
                warnings,
                errors,
                trust_score: trust,
            },
            measurements,
            timestamp: Utc::now(),
            expires_at: Utc::now()
                + chrono::Duration::seconds(self.config.attestation_interval_seconds as i64),
            signature: Some(signature),
            raw_report: None,
        };

        // 状態更新
        let instance = self
            .tee_instances
            .iter_mut()
            .find(|i| i.id == instance_id)
            .unwrap();
        instance.attestation_status = if success {
            AttestationStatus::Verified
        } else {
            AttestationStatus::Failed
        };
        instance.last_attestation = Some(Utc::now());

        self.attestation_reports.push(report.clone());
        self.stats.total_attestations += 1;
        if success {
            self.stats.successful_attestations += 1;
        }
        self.updated_at = Utc::now();

        Ok(report)
    }

    /// 検証 1/5: TEE 種別と GPU の組合せ妥当性
    fn validate_tee_gpu_compat(
        tee: TeeType,
        gpu_lower: &str,
        gpu_display: &str,
    ) -> (Vec<String>, Vec<String>, u32) {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let mut trust: u32 = 100;

        match tee {
            TeeType::NvidiaGpuTee => {
                let ok = [
                    "h100",
                    "h200",
                    "b100",
                    "b200",
                    "gh200",
                    "blackwell",
                    "hopper",
                ]
                .iter()
                .any(|k| gpu_lower.contains(k));
                if !ok {
                    errors.push(format!(
                        "GPU '{}' は NvidiaGpuTee 非対応 (要 Hopper/Blackwell)",
                        gpu_display
                    ));
                    trust = 0;
                }
            }
            TeeType::IntelTdx => {
                if !gpu_lower.contains("intel") && !gpu_lower.contains("xeon") {
                    warnings.push("Intel TDX は通常 Intel CPU 必須".into());
                    trust = trust.saturating_sub(15);
                }
            }
            TeeType::AmdSevSnp => {
                if !gpu_lower.contains("amd") && !gpu_lower.contains("epyc") {
                    warnings.push("AMD SEV-SNP は通常 AMD CPU 必須".into());
                    trust = trust.saturating_sub(15);
                }
            }
            TeeType::IntelSgx => {
                warnings.push("Intel SGX は AI 推論に推奨されない (small EPC)".into());
                trust = trust.saturating_sub(20);
            }
            TeeType::ArmCca => {
                warnings.push("ARM CCA は GPU 推論での実績が少ない".into());
                trust = trust.saturating_sub(20);
            }
        }
        (warnings, errors, trust)
    }

    /// 検証 2/5: メモリ暗号化 + セキュリティレベル
    fn validate_security_posture(
        memory_encryption: bool,
        level: SecurityLevel,
    ) -> (Vec<String>, Vec<String>, u32) {
        let mut errors = Vec::new();
        let level_score = match level {
            SecurityLevel::Maximum => 100,
            SecurityLevel::High => 85,
            SecurityLevel::Standard => 60,
            SecurityLevel::Basic => 30,
        };
        if !memory_encryption {
            errors.push("メモリ暗号化が無効 — TEE 保証無効".into());
            return (Vec::new(), errors, 0);
        }
        (Vec::new(), errors, level_score)
    }

    /// 検証 3/5: 反復異常検出 (replay 攻撃の兆候)
    ///
    /// 問⑰: 旧実装は窓を 60 秒固定にしていたため、13 秒以上間隔を空けた slow-drip
    /// 攻撃では閾値 (5 回) に引っかからず無限に replay できた。
    /// 窓を `attestation_interval_seconds` に合わせることで、設定された更新周期全体を
    /// 1 つの監視窓として扱い slow-drip 耐性を確保する。
    fn detect_replay_anomaly(&self, instance_id: &str) -> (Vec<String>, u32) {
        let window = self.config.attestation_interval_seconds as i64;
        let cutoff = Utc::now() - chrono::Duration::seconds(window);
        let recent = self
            .attestation_reports
            .iter()
            .filter(|r| r.tee_instance_id == instance_id && r.timestamp > cutoff)
            .count();
        if recent >= 5 {
            let msg = format!(
                "{} 秒以内に {} 回の反復 attestation — replay 攻撃の可能性",
                window, recent
            );
            (vec![msg], 75) // 75 = 100 - 25 penalty
        } else {
            (Vec::new(), 100)
        }
    }

    /// 検証 4/5: Measurements 構築 (PCR / RTMR)
    fn build_measurements(tee: TeeType, inst_id: &str, mem_enc: bool) -> Measurements {
        let mut pcr = HashMap::new();
        match tee {
            TeeType::IntelTdx => {
                pcr.insert(0, "rtmr0:pending".into());
                pcr.insert(1, "rtmr1:pending".into());
            }
            TeeType::NvidiaGpuTee => {
                pcr.insert(0, format!("gpu_evidence:{}", super::short(inst_id, 8)));
            }
            _ => {}
        }
        Measurements {
            pcr_values: pcr,
            enclave_measurement: Some(format!("instance:{}", inst_id)),
            secure_boot_enabled: mem_enc,
            debug_mode: false,
            virtualization_extensions: matches!(tee, TeeType::IntelTdx | TeeType::AmdSevSnp),
        }
    }

    /// 検証 5/5: Evidence digest 構築 (v0.2 placeholder)
    ///
    /// ⚠️ これは依然として暗号学的「署名」ではない。v0.3 で実 GPU attestation に
    /// 結線する際、NVIDIA の attestation report に含まれる実署名 (ECDSA over
    /// device cert chain) に置換する。プレフィックスを `unverified-digest:` に
    /// して本物の署名と取り違えないようにする。
    ///
    /// v0.2 改善: FNV (衝突耐性なし) を blake3 (衝突耐性あり) に置換し、`nonce`
    /// (attestation ごとに一意な report id) を含めて、同一インスタンスの再
    /// attestation でも digest が変わるようにした (リプレイ識別性)。
    fn build_evidence_signature(
        inst_id: &str,
        tee: TeeType,
        ok: bool,
        trust: u32,
        nonce: &str,
    ) -> String {
        let evidence = format!("{}|{}|{}|{}|{}", inst_id, tee as u8, ok, trust, nonce);
        let digest = blake3::hash(evidence.as_bytes());
        format!("unverified-digest:{}", hex::encode(&digest.as_bytes()[..8]))
    }

    /// セキュアセッションを作成
    pub fn create_secure_session(
        &mut self,
        tee_instance_id: &str,
        user_id: &str,
    ) -> Result<SecureSession> {
        // TEEインスタンスを検証
        let instance = self
            .tee_instances
            .iter()
            .find(|i| i.id == tee_instance_id)
            .context("TEE インスタンス無し")?;

        if instance.attestation_status != AttestationStatus::Verified {
            anyhow::bail!("TEE instance not attested");
        }
        // 問⑯: Verified ステータスは refresh_expired_attestations() を呼ぶまで
        // 古いまま残り、期限切れの attestation でセッションが作られる。
        // 外部からの refresh 呼び出しに依存せず、鮮度を直接確認する。
        let stale = instance
            .last_attestation
            .map(|last| {
                (Utc::now() - last).num_seconds() > self.config.attestation_interval_seconds as i64
            })
            .unwrap_or(true); // last_attestation = None → 未 attest → stale
        if stale {
            anyhow::bail!(
                "TEE attestation 期限切れ — perform_attestation を再実行してください \
                 (interval: {}s)",
                self.config.attestation_interval_seconds
            );
        }

        let now = Utc::now();
        let session = SecureSession {
            id: uuid::Uuid::now_v7().to_string(),
            tee_instance_id: tee_instance_id.to_string(),
            user_id: user_id.to_string(),
            status: SessionStatus::Establishing,
            encryption_algorithm: self.config.default_encryption,
            encrypted_session_key: Some("encrypted:...".to_string()),
            created_at: now,
            last_active: now,
            expires_at: now + chrono::Duration::seconds(self.config.session_timeout_seconds as i64),
        };

        self.secure_sessions.push(session.clone());
        self.stats.active_sessions += 1;

        self.updated_at = Utc::now();

        Ok(session)
    }

    /// ポリシーを追加
    pub fn add_policy(&mut self, policy: SecurityPolicy) {
        self.policies.push(policy);
        self.updated_at = Utc::now();
    }

    /// ポリシーを検証
    pub fn validate_against_policy(&self, instance: &TeeInstance, policy_id: &str) -> Result<bool> {
        let policy = self
            .policies
            .iter()
            .find(|p| p.id == policy_id)
            .context("ポリシー無し")?;

        if !policy.enabled {
            return Ok(true);
        }

        // セキュリティレベルチェック
        if instance.security_level < policy.min_security_level {
            return Ok(false);
        }

        // TEEタイプチェック
        if !policy.allowed_tee_types.is_empty()
            && !policy.allowed_tee_types.contains(&instance.tee_type)
        {
            return Ok(false);
        }

        // アテステーションチェック
        if policy.attestation_required && instance.attestation_status != AttestationStatus::Verified
        {
            return Ok(false);
        }

        Ok(true)
    }

    /// 期限切れアテステーションを更新
    pub fn refresh_expired_attestations(&mut self) -> Vec<String> {
        let now = Utc::now();
        let mut refreshed = Vec::new();

        for instance in &mut self.tee_instances {
            if instance.status != TeeStatus::Running {
                continue;
            }

            // 最終アテステーションからの経過時間をチェック
            let needs_refresh = instance
                .last_attestation
                .map(|last| {
                    (now - last).num_seconds() > self.config.attestation_interval_seconds as i64
                })
                .unwrap_or(true);

            if needs_refresh {
                instance.attestation_status = AttestationStatus::Expired;
                refreshed.push(instance.id.clone());
            }
        }

        self.updated_at = Utc::now();
        refreshed
    }

    /// 検証済みTEE数
    pub fn verified_tee_count(&self) -> usize {
        self.tee_instances
            .iter()
            .filter(|i| i.attestation_status == AttestationStatus::Verified)
            .count()
    }

    /// アクティブTEE数
    pub fn active_tee_count(&self) -> usize {
        self.tee_instances
            .iter()
            .filter(|i| i.status == TeeStatus::Running)
            .count()
    }

    /// 統計を更新
    pub fn update_stats(&mut self) {
        self.stats.active_tee_instances = self
            .tee_instances
            .iter()
            .filter(|i| i.status == TeeStatus::Running)
            .count() as u32;

        self.stats.active_sessions = self
            .secure_sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Active)
            .count() as u32;

        self.updated_at = Utc::now();
    }
}

// ストレージ関数
/// confidential 設定の永続化パスを返す
pub fn confidential_path() -> std::path::PathBuf {
    crate::core::config::config_dir().join("confidential.json")
}

/// confidential マネージャーを永続化ファイルから読込
pub fn load_confidential() -> Result<ConfidentialManager> {
    Ok(crate::core::config::load_or_recover(
        &confidential_path(),
        "TEE 情報 (confidential.json)",
    ))
}

/// confidential マネージャーを永続化ファイルに保存
pub fn save_confidential(manager: &ConfidentialManager) -> Result<()> {
    crate::core::config::atomic_write(
        &confidential_path(),
        &serde_json::to_string_pretty(manager)?,
    )
}

/// Confidential Computing状態をフォーマット
pub fn format_confidential(manager: &ConfidentialManager) -> String {
    let mut output = String::new();

    output.push_str("機密計算:\n");
    output.push_str("═══════════════════════════════════════════════════════════\n\n");

    // 統計
    output.push_str("🔐 概要:\n");
    output.push_str(&format!(
        "  TEEインスタンス: {}/{}\n",
        manager.stats.active_tee_instances, manager.stats.total_tee_instances
    ));
    output.push_str(&format!("  検証済み: {}\n", manager.verified_tee_count()));
    output.push_str(&format!(
        "  アクティブセッション: {}\n",
        manager.stats.active_sessions
    ));
    output.push_str(&format!(
        "  成功アテステーション: {}%\n",
        if manager.stats.total_attestations > 0 {
            (manager.stats.successful_attestations as f64 / manager.stats.total_attestations as f64
                * 100.0) as u32
        } else {
            0
        }
    ));
    output.push('\n');

    // TEEインスタンス
    output.push_str(&format!(
        "🖥️ TEEインスタンス ({}):\n",
        manager.tee_instances.len()
    ));
    for instance in &manager.tee_instances {
        output.push_str(&format!(
            "  {} {} - {} ({}) {}\n",
            instance.status.icon(),
            &instance.id[..instance.id.len().min(8)],
            instance.tee_type,
            instance.security_level,
            instance.attestation_status.icon()
        ));
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tee_creation() {
        let mut manager = ConfidentialManager::default();

        let instance = manager.create_tee_instance(
            "gpu-001",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::High,
        );

        assert_eq!(instance.tee_type, TeeType::NvidiaGpuTee);
        assert_eq!(instance.security_level, SecurityLevel::High);
        assert_eq!(instance.attestation_status, AttestationStatus::Unverified);
    }

    #[test]
    fn test_attestation() {
        let mut manager = ConfidentialManager::default();

        let instance = manager.create_tee_instance(
            "gpu-001",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Standard,
        );

        let report = manager.perform_attestation(&instance.id).unwrap();

        assert!(report.verification_result.success);
        assert_eq!(
            manager.tee_instances[0].attestation_status,
            AttestationStatus::Verified
        );
    }

    #[test]
    fn test_secure_session() {
        let mut manager = ConfidentialManager::default();

        let instance = manager.create_tee_instance(
            "gpu-001",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Standard,
        );

        // アテステーションなしではセッション作成不可
        assert!(manager
            .create_secure_session(&instance.id, "user-001")
            .is_err());

        // アテステーション実行
        manager.perform_attestation(&instance.id).unwrap();

        // セッション作成可能
        let session = manager
            .create_secure_session(&instance.id, "user-001")
            .unwrap();
        assert_eq!(session.tee_instance_id, instance.id);
    }

    /// 問⑯: attestation は Verified でも last_attestation が期限切れなら
    /// create_secure_session は Err を返す。
    /// 旧実装は refresh_expired_attestations() を呼ぶまで
    /// Verified のままセッション作成が通っていた。
    #[test]
    fn test_create_secure_session_rejects_stale_attestation() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Standard,
        );
        m.perform_attestation(&inst.id).unwrap();

        // 正常: 直後はセッション作成可能
        assert!(m.create_secure_session(&inst.id, "u1").is_ok());

        // last_attestation を interval + 1 秒前に巻き戻す
        let interval = m.config.attestation_interval_seconds as i64;
        m.tee_instances
            .iter_mut()
            .find(|i| i.id == inst.id)
            .unwrap()
            .last_attestation = Some(Utc::now() - chrono::Duration::seconds(interval + 1));

        // status は Verified のまま — でも鮮度チェックで拒否されるべき
        let stale_inst = m.tee_instances.iter().find(|i| i.id == inst.id).unwrap();
        assert_eq!(stale_inst.attestation_status, AttestationStatus::Verified);

        let result = m.create_secure_session(&inst.id, "u1");
        assert!(
            result.is_err(),
            "期限切れ attestation でセッションを作成できてはならない"
        );
        assert!(result.unwrap_err().to_string().contains("期限切れ"));
    }

    /// 問⑰: detect_replay_anomaly の窓が attestation_interval に合わせて拡張される。
    /// 小さいインターバル設定 (2s) で 5 回実行すると 2 秒窓で全件検出される。
    #[test]
    fn test_replay_anomaly_uses_configured_interval_window() {
        let mut m = ConfidentialManager::default();
        m.config.attestation_interval_seconds = 2; // 窓 = 2 秒 (テスト用に短縮)
        let inst = m.create_tee_instance(
            "gpu-1",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Maximum,
        );
        // 5 回 attestation — 全て 2 秒窓内
        for _ in 0..5 {
            m.perform_attestation(&inst.id).unwrap();
        }
        // 6 回目で閾値超過 → 反復警告
        let r = m.perform_attestation(&inst.id).unwrap();
        assert!(
            r.verification_result
                .warnings
                .iter()
                .any(|w| w.contains("反復")),
            "閾値超過で反復 attestation 警告が出るべき"
        );
        // 警告メッセージに設定済み窓 (2s) が含まれる
        assert!(
            r.verification_result
                .warnings
                .iter()
                .any(|w| w.contains("2 秒")),
            "窓サイズがメッセージに反映されるべき"
        );
    }

    #[test]
    fn test_policy_validation() {
        let mut manager = ConfidentialManager::default();

        let instance = manager.create_tee_instance(
            "gpu-001",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Standard,
        );

        manager.perform_attestation(&instance.id).unwrap();

        manager.add_policy(SecurityPolicy {
            id: "policy-001".to_string(),
            name: "High Security".to_string(),
            description: "Requires high security level".to_string(),
            min_security_level: SecurityLevel::High,
            attestation_required: true,
            attestation_validity_seconds: 3600,
            allowed_tee_types: vec![TeeType::NvidiaGpuTee],
            allow_debug_mode: false,
            min_tcb_version: None,
            created_at: Utc::now(),
            enabled: true,
        });

        // Standardレベルなのでポリシー違反
        let instance = &manager.tee_instances[0];
        let valid = manager
            .validate_against_policy(instance, "policy-001")
            .unwrap();
        assert!(!valid);
    }

    #[test]
    fn test_tee_type_supports_gpu() {
        assert!(TeeType::NvidiaGpuTee.supports_gpu());
        assert!(!TeeType::IntelSgx.supports_gpu());
        assert!(!TeeType::AmdSevSnp.supports_gpu());
    }

    #[test]
    fn test_tee_type_attestation_url() {
        assert!(TeeType::NvidiaGpuTee.attestation_service_url().is_some());
        assert!(TeeType::IntelTdx.attestation_service_url().is_some());
    }

    #[test]
    fn test_attestation_marks_instance_verified() {
        let mut m = ConfidentialManager::default();
        let inst =
            m.create_tee_instance("gpu-1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        let report = m.perform_attestation(&inst.id).unwrap();
        assert!(report.verification_result.success);

        let updated = m.tee_instances.iter().find(|i| i.id == inst.id).unwrap();
        assert_eq!(updated.attestation_status, AttestationStatus::Verified);
    }

    #[test]
    fn test_high_security_policy_passes_for_h100() {
        let mut m = ConfidentialManager::default();
        let inst =
            m.create_tee_instance("gpu-1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        m.perform_attestation(&inst.id).unwrap();

        m.add_policy(SecurityPolicy {
            id: "p-high".to_string(),
            name: "High".to_string(),
            description: "".to_string(),
            min_security_level: SecurityLevel::High,
            attestation_required: true,
            attestation_validity_seconds: 3600,
            allowed_tee_types: vec![TeeType::NvidiaGpuTee],
            allow_debug_mode: false,
            min_tcb_version: None,
            created_at: Utc::now(),
            enabled: true,
        });

        // Re-fetch the verified instance
        let inst = m
            .tee_instances
            .iter()
            .find(|i| i.id == inst.id)
            .unwrap()
            .clone();
        let ok = m.validate_against_policy(&inst, "p-high").unwrap();
        assert!(ok);
    }

    #[test]
    fn test_count_helpers() {
        let mut m = ConfidentialManager::default();
        assert_eq!(m.active_tee_count(), 0);
        assert_eq!(m.verified_tee_count(), 0);

        let inst =
            m.create_tee_instance("gpu-1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        // 作成直後は Initializing (Running ではない) → active=0
        assert_eq!(m.active_tee_count(), 0);
        assert_eq!(m.verified_tee_count(), 0);

        m.perform_attestation(&inst.id).unwrap();
        assert_eq!(m.verified_tee_count(), 1);
    }

    #[test]
    fn test_unknown_instance_attestation_fails() {
        let mut m = ConfidentialManager::default();
        assert!(m.perform_attestation("nope").is_err());
    }

    // ====================================================================
    // Round 20: Honest attestation tests (not just "always success")
    // ====================================================================

    /// 民生 GPU + NvidiaGpuTee = 拒否 (LANDMINES 2 物理対応の核)
    #[test]
    fn test_consumer_gpu_with_tee_claim_is_rejected() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "RTX 4090",
            TeeType::NvidiaGpuTee,
            SecurityLevel::High,
        );
        let report = m.perform_attestation(&inst.id).unwrap();
        assert!(
            !report.verification_result.success,
            "RTX 4090 が NvidiaGpuTee で承認されたら全システム破綻"
        );
        assert!(report
            .verification_result
            .errors
            .iter()
            .any(|e| e.contains("非対応")));
        assert_eq!(report.verification_result.trust_score, 0);
    }

    /// データセンター GPU + NvidiaGpuTee = 承認
    #[test]
    fn test_h100_with_tee_passes() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "NVIDIA H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::High,
        );
        let report = m.perform_attestation(&inst.id).unwrap();
        assert!(report.verification_result.success);
        assert!(report.verification_result.trust_score >= 80);
    }

    /// Blackwell B200 も TEE 対応
    #[test]
    fn test_blackwell_with_tee_passes() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "NVIDIA B200",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Maximum,
        );
        let report = m.perform_attestation(&inst.id).unwrap();
        assert!(report.verification_result.success);
        assert_eq!(report.verification_result.trust_score, 100);
    }

    /// SecurityLevel が低いと trust_score も低い
    #[test]
    fn test_low_security_level_caps_trust() {
        let mut m = ConfidentialManager::default();
        let inst =
            m.create_tee_instance("gpu-1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::Basic);
        let report = m.perform_attestation(&inst.id).unwrap();
        // Minimal は max 30
        assert!(report.verification_result.trust_score <= 30);
    }

    /// 1 分以内に 5 回以上は warning + trust 減点
    #[test]
    fn test_replay_pattern_lowers_trust() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "H100",
            TeeType::NvidiaGpuTee,
            SecurityLevel::Maximum,
        );
        // 5 回連続 attestation
        for _ in 0..5 {
            m.perform_attestation(&inst.id).unwrap();
        }
        let r = m.perform_attestation(&inst.id).unwrap();
        assert!(r
            .verification_result
            .warnings
            .iter()
            .any(|w| w.contains("反復")));
    }

    /// SGX は warning (AI 推論に EPC 不足)
    #[test]
    fn test_sgx_warns_about_epc() {
        let mut m = ConfidentialManager::default();
        let inst = m.create_tee_instance(
            "gpu-1",
            "Intel Xeon",
            TeeType::IntelSgx,
            SecurityLevel::High,
        );
        let r = m.perform_attestation(&inst.id).unwrap();
        assert!(r
            .verification_result
            .warnings
            .iter()
            .any(|w| w.contains("SGX")));
    }

    /// 再 attestation では digest が変わり (リプレイ識別性)、かつ「未検証」と明示される
    #[test]
    fn test_signature_varies_per_attestation_and_marks_unverified() {
        let mut m = ConfidentialManager::default();
        let inst =
            m.create_tee_instance("gpu-1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        let r1 = m.perform_attestation(&inst.id).unwrap();
        let r2 = m.perform_attestation(&inst.id).unwrap();
        // 同じインスタンスでも report ごとに nonce が異なるため digest は変わる。
        // 旧実装は同一 digest を返し、古い attestation report を使い回せた。
        assert_ne!(
            r1.signature.as_ref().unwrap(),
            r2.signature.as_ref().unwrap()
        );
        // v0.2 placeholder は暗号署名でないと明示されている
        assert!(
            r1.signature
                .as_ref()
                .unwrap()
                .starts_with("unverified-digest:"),
            "placeholder は本物の署名と取り違えないよう unverified を明示すべき"
        );
    }

    // ================================================================
    // Round 26: 分割 helper の直接テスト
    // ================================================================

    #[test]
    fn test_helper_tee_compat_rtx4090() {
        let (_, errors, trust) = ConfidentialManager::validate_tee_gpu_compat(
            TeeType::NvidiaGpuTee,
            "rtx 4090",
            "RTX 4090",
        );
        assert!(!errors.is_empty());
        assert_eq!(trust, 0);
    }

    #[test]
    fn test_helper_tee_compat_h100() {
        let (w, e, t) = ConfidentialManager::validate_tee_gpu_compat(
            TeeType::NvidiaGpuTee,
            "nvidia h100",
            "NVIDIA H100",
        );
        assert!(e.is_empty());
        assert!(w.is_empty());
        assert_eq!(t, 100);
    }

    #[test]
    fn test_helper_security_no_encryption() {
        let (_, e, t) =
            ConfidentialManager::validate_security_posture(false, SecurityLevel::Maximum);
        assert!(!e.is_empty());
        assert_eq!(t, 0);
    }

    #[test]
    fn test_helper_measurements_nvidia() {
        let m =
            ConfidentialManager::build_measurements(TeeType::NvidiaGpuTee, "inst-12345678", true);
        assert!(m.pcr_values.get(&0).unwrap().contains("gpu_evidence"));
    }

    #[test]
    fn test_helper_signature_differs_on_trust() {
        let s1 = ConfidentialManager::build_evidence_signature(
            "id",
            TeeType::NvidiaGpuTee,
            true,
            85,
            "n",
        );
        let s2 = ConfidentialManager::build_evidence_signature(
            "id",
            TeeType::NvidiaGpuTee,
            true,
            60,
            "n",
        );
        assert_ne!(s1, s2);
        assert!(s1.starts_with("unverified-digest:"));
    }

    #[test]
    fn test_helper_signature_differs_on_nonce() {
        // 同一インスタンスの再 attestation でも nonce が異なれば digest が変わる
        // (リプレイ識別性)。
        let s1 = ConfidentialManager::build_evidence_signature(
            "id",
            TeeType::NvidiaGpuTee,
            true,
            85,
            "a",
        );
        let s2 = ConfidentialManager::build_evidence_signature(
            "id",
            TeeType::NvidiaGpuTee,
            true,
            85,
            "b",
        );
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut m = ConfidentialManager::default();
        m.create_tee_instance("g1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        m.perform_attestation(&m.tee_instances[0].id.clone())
            .unwrap();
        let json1 = serde_json::to_string(&m).unwrap();
        let back: ConfidentialManager = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "ConfidentialManager roundtrip");
    }

    #[test]
    fn test_format_confidential_contains_key_sections() {
        let mut m = ConfidentialManager::default();
        m.create_tee_instance("g1", "H100", TeeType::NvidiaGpuTee, SecurityLevel::High);
        m.perform_attestation(&m.tee_instances[0].id.clone())
            .unwrap();
        let out = format_confidential(&m);
        assert!(out.contains('═'), "ヘッダ罫線");
        assert!(out.contains("概要"), "概要セクション");
        assert!(out.contains("検証済み"), "検証数");
        assert!(out.contains("成功"), "成功率");
    }
}
