//! first_run — `rope` 引数無しの60秒wow moment
//!
//! Apple origin story: 封筒から MacBook Air が出てくる瞬間。
//! iPod がジーンズのポケットに入る瞬間。
//! iPhone が「widescreen iPod + phone + internet」と宣言される瞬間。
//!
//! ROPE_2028 第一動詞「`rope` 引数無しで正しいこと勝手にやる」の実装核。
//! これは新機能ではない。既存 121 モジュールが果たすべき約束の履行。
//!
//! ## 原則 (Markkula 1977 + Jobs 1997)
//!
//! 1. **Empathy**: ユーザーの文脈を推測し、質問を最小化
//! 2. **Focus**: 必須3ステップのみ(identity / pair / first-job)、他は全部後回し
//! 3. **Impute**: 「洗練された体験」を演出。エラーメッセージすら品質
//! 4. **Work backwards**: 60秒後の「wow」から逆算、技術は後
//!
//! ## 段階
//!
//! Stage 1 (0-10s): 鍵生成、hostname 表示、peer 募集開始
//! Stage 2 (10-40s): mDNS 発見、QR フォールバック、握手
//! Stage 3 (40-60s): 既定小ジョブ実行 (ハローワールド推論)
//! Stage 4 (after):  オプション説明 (earn / run / pair)
//!
//! ## ultrathink: Ropeにおける封筒
//!
//! Apple は MacBook Air を封筒から出した。Rope は何を?
//! → **「他人のGPUでLLMが動いた」を60秒以内に体験**させる。
//! 無引数 `rope` がそれを自動で見せる。Rope 未体験者が最初にすること。
//! ユーザーは何も設定しない。起動するだけ。

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::confidential::ConfidentialManager;
use super::ecash::EcashManager;
use super::intent::{BudgetEnforcement, Intent, Privacy, Workload};
use super::pair::{DiscoveryMethod, PairManager};

/// First-run オーケストレータ
///
/// 状態機械。各 stage が完了条件と timeout を持つ。
#[derive(Debug, Serialize, Deserialize)]
pub struct FirstRun {
    pub id: String,
    pub user_display_name: String,
    pub started_at: DateTime<Utc>,
    pub current_stage: Stage,
    pub stages_completed: Vec<StageOutcome>,
    /// Intent が解決された場合の結果 ID
    pub demo_intent_id: Option<String>,
    /// 最終的に選ばれた提供元の説明 (ユーザー表示用)
    pub provider_chosen_human: Option<String>,
    /// 完了した場合の total 時間 (ms)
    pub completed_ms: Option<u64>,
    /// 失敗または離脱時の段階
    pub abort_reason: Option<String>,
    /// デモで生成された haiku (welcome_letter の封筒モーメント用)
    pub demo_haiku: Option<String>,
    /// 設定
    pub config: FirstRunConfig,
}

impl FirstRun {
    /// 新規開始
    pub fn new(user_display_name: &str) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            user_display_name: user_display_name.to_string(),
            started_at: Utc::now(),
            current_stage: Stage::Welcome,
            stages_completed: Vec::new(),
            demo_intent_id: None,
            provider_chosen_human: None,
            completed_ms: None,
            abort_reason: None,
            demo_haiku: None,
            config: FirstRunConfig::default(),
        }
    }
}

/// Stage — Apple-style 遷移
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// 挨拶 + 「何が起きるか」1 文宣言 (iPhone = "widescreen iPod + phone + internet")
    Welcome,
    /// 鍵生成、hostname 確認 — 質問はしない
    IdentityReady,
    /// ピア発見開始 (mDNS → Bluetooth → QR)
    Discovering,
    /// ピア選択 (1 人でも見つかれば即進む)
    Paired,
    /// **プロンプトの行き先を利用者に開示する段** (§1.24)。
    ///
    /// 🔴 **variant 名は嘘だが、意図的に残している。** 以前ここは
    /// シミュレートした TEE attestation を走らせ、「GPU は安全 (プロンプトは
    /// 相手に見えません)」と表示していた — `V1_SCOPE.md` が A6 (TEE 秘匿) を
    /// v1 から削除し、`SECURITY.md` が「貸し手はプロンプトを見られる」と
    /// 明記しているのに、である。**製品が自分について嘘の安全性を主張して
    /// いた。** 中身は消したが、名前は `~/.rope/first_run.json` に既に
    /// 書かれているので変えない (規範4: 既存ファイルの読込を壊さない)。
    AttestVerified,
    /// 既定デモジョブ作成 — ユーザーは何も入力しない
    DemoIntentCreated,
    /// ジョブ実行、回答表示
    DemoCompleted,
    /// 終了画面: 3 つの選択肢を提示 (keep / earn / run)
    Finale,
    /// 完全終了
    Done,
    /// 失敗または中断
    Aborted,
}

impl Stage {
    /// Apple 的アイコン (CLI 出力用)
    pub fn icon(&self) -> &'static str {
        match self {
            Stage::Welcome => "👋",
            Stage::IdentityReady => "🔑",
            Stage::Discovering => "📡",
            Stage::Paired => "🤝",
            // 盾ではない。**守っていないものを守っているように見せない。**
            Stage::AttestVerified => "🔍",
            Stage::DemoIntentCreated => "💭",
            Stage::DemoCompleted => "✨",
            Stage::Finale => "🎬",
            Stage::Done => "✅",
            Stage::Aborted => "🚫",
        }
    }

    /// ユーザー表示用の 1 文 (Apple keynote tone)
    pub fn human_message(&self) -> &'static str {
        match self {
            Stage::Welcome => "Rope へようこそ。60秒で他人のGPUでAIを動かします。",
            Stage::IdentityReady => "鍵の準備ができました。",
            Stage::Discovering => "近くのピアを探しています…",
            Stage::Paired => "接続しました。",
            Stage::AttestVerified => "プロンプトの行き先を確認しました。",
            Stage::DemoIntentCreated => "ジョブを組み立て中…",
            Stage::DemoCompleted => "動きました。",
            Stage::Finale => "3 つのうちどれを次にやりますか?",
            Stage::Done => "準備完了。",
            Stage::Aborted => "中断しました。",
        }
    }
}

/// 各 stage の成果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutcome {
    pub stage: Stage,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub success: bool,
    pub details: String,
}

/// 設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunConfig {
    /// mDNS 試行の秒数上限
    pub mdns_timeout_seconds: u32,
    /// Bluetooth 試行
    pub bluetooth_timeout_seconds: u32,
    /// QR フォールバック発動までの秒数
    pub fallback_to_qr_seconds: u32,
    /// ピアが 1 人も見つからない時にローカル単独実行にフォールバック
    pub fallback_to_local_if_no_peer: bool,
    /// デモ用既定モデル
    pub demo_model_name: String,
    /// デモ用プロンプト (これ自体が wow moment)
    pub demo_prompt: String,
    /// デモ用候補レスポンス群 (実推論前のプレースホルダ)
    /// 実 GPU 推論が結線されるまで、ここから sample されて表示される
    pub demo_haiku_responses: Vec<String>,
    /// 既定予算 (Cashu sats)
    pub demo_budget_sats: u64,
    /// 完了とみなす最大時間 (秒)
    pub wow_target_seconds: u32,
}

impl Default for FirstRunConfig {
    fn default() -> Self {
        Self {
            mdns_timeout_seconds: 3,
            bluetooth_timeout_seconds: 3,
            fallback_to_qr_seconds: 6,
            fallback_to_local_if_no_peer: true,
            demo_model_name: "llama-3.2-1b-instruct".to_string(),
            // ultrathink: "Write a haiku about lending GPUs to strangers."
            // これは rope の product philosophy を 1 文で体現する
            // 応答自体がマーケティング素材になる
            demo_prompt: "Write a haiku about lending GPUs to strangers.".to_string(),
            demo_haiku_responses: vec![
                "A quiet click, a distant fan,\nSilicon warms for a stranger,\nMy thanks ride on light.".to_string(),
                "Idle GPU hums—\nThree cents cross the cipher tide,\nSomeone else's dream.".to_string(),
                "Fan spin in another room,\nMy haiku borrows their power,\nRope ties us briefly.".to_string(),
                "Across the ocean,\nA card I will never see\nDraws my prompt from sleep.".to_string(),
            ],
            demo_budget_sats: 100, // 1 サトシ未満の haiku = 約 $0.0005
            wow_target_seconds: 60,
        }
    }
}

impl FirstRun {
    /// Stage 遷移 + 成果記録
    pub fn transition(&mut self, to: Stage, details: &str, success: bool) -> Result<()> {
        let now = Utc::now();
        let stage_start = self
            .stages_completed
            .last()
            .map(|o| o.completed_at)
            .unwrap_or(self.started_at);

        let outcome = StageOutcome {
            stage: self.current_stage,
            started_at: stage_start,
            completed_at: now,
            duration_ms: (now - stage_start).num_milliseconds() as u64,
            success,
            details: details.to_string(),
        };
        self.stages_completed.push(outcome);
        self.current_stage = to;

        if to == Stage::Done {
            self.completed_ms = Some((now - self.started_at).num_milliseconds() as u64);
        }
        if to == Stage::Aborted {
            self.abort_reason = Some(details.to_string());
        }
        Ok(())
    }

    /// 「今どこ?」の 1 行表示 (CLI flow 用)
    ///
    /// README の 60 秒デモと完全一致する形式:
    ///   🔑 鍵の準備ができました。
    ///
    /// タイミング詳細は welcome_letter (終了画面) に集約。
    /// Apple keynote: デモ中はクリーン、スペックは最後。
    pub fn progress_line(&self) -> String {
        format!(
            "{} {}",
            self.current_stage.icon(),
            self.current_stage.human_message()
        )
    }

    /// Apple-style タイムライン (終了画面でもう一度見せる)
    pub fn timeline(&self) -> Vec<TimelineEntry> {
        self.stages_completed
            .iter()
            .map(|o| TimelineEntry {
                icon: o.stage.icon().to_string(),
                label: o.stage.human_message().to_string(),
                elapsed_ms: o.duration_ms,
                success: o.success,
            })
            .collect()
    }

    /// 総経過 ms
    pub fn elapsed_ms(&self) -> u64 {
        (Utc::now() - self.started_at).num_milliseconds() as u64
    }

    /// wow target 内に完了したか
    pub fn met_wow_target(&self) -> bool {
        self.completed_ms
            .map(|ms| ms <= (self.config.wow_target_seconds as u64 * 1000))
            .unwrap_or(false)
    }

    /// デモ用 haiku レスポンスを 1 つ選ぶ
    ///
    /// 実 GPU 推論が結線されるまでのプレースホルダ。
    /// `started_at` の nanos を seed にして deterministic-per-session に sample。
    /// 同じ first_run インスタンス内では常に同じ haiku が返る (再現性)。
    pub fn sample_haiku_response(&self) -> &str {
        let pool = &self.config.demo_haiku_responses;
        if pool.is_empty() {
            return "";
        }
        let nanos = self.started_at.timestamp_nanos_opt().unwrap_or(0) as u64;
        let idx = (nanos as usize) % pool.len();
        &pool[idx]
    }
}

/// 終了画面で見せるタイムライン項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub icon: String,
    pub label: String,
    pub elapsed_ms: u64,
    pub success: bool,
}

/// オーケストレーション — 既存モジュール合成
///
/// 本関数は既存 `PairManager` / `EcashManager` / `IntentManager` /
/// `ConfidentialManager` を呼び分けるだけ。新抽象ゼロ。
/// Apple 的に「機能の追加なし、合成の追加」。
pub struct FirstRunOrchestrator<'a> {
    pub pair: &'a mut PairManager,
    pub ecash: &'a mut EcashManager,
    pub confidential: &'a mut ConfidentialManager,
    pub first_run: &'a mut FirstRun,
}

impl<'a> FirstRunOrchestrator<'a> {
    /// Stage 1→2: 鍵の準備 (既に `peer.rs` が持つ機能を ping するだけ)
    pub fn step_identity(&mut self) -> Result<()> {
        if self.first_run.current_stage != Stage::Welcome {
            anyhow::bail!(
                "不正な stage: {}",
                self.first_run.current_stage.human_message()
            );
        }
        // 実運用: peer::ensure_keys_exist() を呼ぶ。本実装はダミー。
        self.first_run
            .transition(Stage::IdentityReady, "Ed25519 鍵準備完了", true)?;
        Ok(())
    }

    /// Stage 2→3: 発見フェーズ開始 (mDNS → BT → QR カスケード)
    pub fn step_discovery_start(&mut self) -> Result<()> {
        if self.first_run.current_stage != Stage::IdentityReady {
            anyhow::bail!(
                "不正な stage: {}",
                self.first_run.current_stage.human_message()
            );
        }
        self.first_run
            .transition(Stage::Discovering, "mDNS scan started", true)?;
        Ok(())
    }

    /// Stage 3→4: 発見完了 → 握手完了
    ///
    /// pair.rs の `paired` から 1 件選んで Paired stage に進む。
    /// 誰も居ない場合はフォールバック (loopback peer を作るか、ローカル単独実行)。
    pub fn step_discovery_complete(&mut self) -> Result<DiscoveryOutcome> {
        if self.first_run.current_stage != Stage::Discovering {
            anyhow::bail!("不正な stage");
        }

        let peers = &self.pair.paired;
        let outcome = if !peers.is_empty() {
            // VRAM と信頼度で最良を選ぶ
            let best = peers
                .iter()
                .max_by_key(|p| p.capabilities.vram_gb)
                .unwrap()
                .clone();
            self.first_run.provider_chosen_human = Some(format!(
                "{} ({} GB VRAM)",
                best.display_name, best.capabilities.vram_gb
            ));
            DiscoveryOutcome::PeerChosen {
                peer_id: best.id.clone(),
                display_name: best.display_name,
                method: DiscoveryMethod::Mdns, // 実運用は best の method を引く
            }
        } else if self.first_run.config.fallback_to_local_if_no_peer {
            self.first_run.provider_chosen_human = Some("this device (local)".to_string());
            DiscoveryOutcome::LocalFallback
        } else {
            DiscoveryOutcome::NoPeerAborted
        };

        match &outcome {
            DiscoveryOutcome::PeerChosen { display_name, .. } => {
                self.first_run.transition(
                    Stage::Paired,
                    &format!("Paired with {}", display_name),
                    true,
                )?;
            }
            DiscoveryOutcome::LocalFallback => {
                self.first_run.transition(
                    Stage::Paired,
                    "ピア未検出 — ローカル実行にフォールバック",
                    true,
                )?;
            }
            DiscoveryOutcome::NoPeerAborted => {
                self.first_run.transition(
                    Stage::Aborted,
                    "ピア未検出、ローカルフォールバック無効のため中止",
                    false,
                )?;
            }
        }
        Ok(outcome)
    }

    /// Stage 4→5: **プロンプトの行き先を確かめて、利用者に開示する** (§1.24)。
    ///
    /// ## なぜ attestation を消したか (Musk ①→②)
    ///
    /// 以前ここは、ピアの GPU 名を文字列で見て TEE 種別を推定し、
    /// シミュレートした attestation を走らせ、成功すれば
    /// 「🛡️ GPU は安全 (プロンプトは相手に見えません)」と表示していた。
    ///
    /// **3 つとも間違っていた:**
    ///
    /// 1. **v1 に TEE 秘匿は無い。** `V1_SCOPE.md` §2 が A6 ごと削除し、
    ///    `SECURITY.md` は「貸し手はプロンプトを見られる」と明記している。
    ///    製品が自分について**嘘の安全性を主張していた** (規範6 の違反)
    /// 2. **attestation は本物ではなかった。** `confidential.rs` の
    ///    シミュレーションで、NRAS にも Intel Trust Authority にも触れない
    /// 3. **民生 GPU で中止していた。** RTX 4090 のピアは「TEE 非対応」として
    ///    `Aborted` になった — **v1 が狙っているのはまさにその層である。**
    ///    初回体験が、想定利用者のハードウェアで止まっていた
    ///
    /// 今ここがするのは 1 つだけ: **プロンプトがどこへ行くかを判定して返す。**
    /// 表示は呼び出し側 (`main.rs`) が posture に応じて行う。
    /// 中止はしない — v1 の答えは「秘匿は無い、と伝えた上で走らせる」だから。
    pub fn step_privacy_check(&mut self) -> Result<PrivacyPosture> {
        if self.first_run.current_stage != Stage::Paired {
            anyhow::bail!("不正な stage");
        }

        let posture = match self.pair.paired.first() {
            Some(p) => PrivacyPosture::PeerCanReadPrompt {
                peer_gpu_model: p.capabilities.gpu_model.clone(),
            },
            None => PrivacyPosture::LocalOnly,
        };

        self.first_run
            .transition(Stage::AttestVerified, posture.detail(), true)?;
        Ok(posture)
    }

    /// Stage 5→6: Intent 組立
    ///
    /// ユーザーには「ジョブを組み立て中」としか見せない。
    /// 裏では demo_prompt と demo_model_name で Intent を作る。
    pub fn step_build_intent(&mut self) -> Result<Intent> {
        if self.first_run.current_stage != Stage::AttestVerified {
            anyhow::bail!("不正な stage: プライバシー開示前に Intent は組めない");
        }

        let cfg = &self.first_run.config;
        let intent = Intent::new(
            Workload::Inference {
                model: cfg.demo_model_name.clone(),
                prompt_tokens_est: estimate_tokens(&cfg.demo_prompt),
                max_output_tokens: 30, // haiku は短い
            },
            &self.first_run.user_display_name,
        )
        .with_budget(
            super::sats_to_usd(cfg.demo_budget_sats), // sats → USD 概算
            BudgetEnforcement::Hard,
        )
        .with_privacy(Privacy::ConfidentialCompute);

        self.first_run.demo_intent_id = Some(intent.id.clone());
        self.first_run.transition(
            Stage::DemoIntentCreated,
            &format!("Intent {} 構築完了", super::short(&intent.id, 8)),
            true,
        )?;
        Ok(intent)
    }

    /// Stage 5→6: 実行完了
    ///
    /// 実際の実行は別システムが行う想定。本関数はその「完了報告」を受ける。
    pub fn step_demo_completed(&mut self, result_preview: &str) -> Result<()> {
        if self.first_run.current_stage != Stage::DemoIntentCreated {
            anyhow::bail!("不正な stage");
        }
        // haiku を専用フィールドに保存 (welcome_letter の封筒モーメント用)
        // stages_completed は遷移元を記録するため、ここで明示的に保持する
        self.first_run.demo_haiku = Some(result_preview.to_string());
        self.first_run
            .transition(Stage::DemoCompleted, result_preview, true)?;
        Ok(())
    }

    /// Stage 6→7: 終了画面
    pub fn step_finale(&mut self) -> Result<FinaleOptions> {
        if self.first_run.current_stage != Stage::DemoCompleted {
            anyhow::bail!("不正な stage");
        }
        self.first_run
            .transition(Stage::Finale, "次のステップを提示", true)?;
        Ok(FinaleOptions::default())
    }

    /// Stage 7→8: 完全終了
    pub fn step_done(&mut self) -> Result<()> {
        if self.first_run.current_stage != Stage::Finale {
            anyhow::bail!("不正な stage");
        }
        self.first_run
            .transition(Stage::Done, "初回体験完了", true)?;
        Ok(())
    }

    /// 強制中断 (任意 stage から)
    pub fn abort(&mut self, reason: &str) -> Result<()> {
        self.first_run.transition(Stage::Aborted, reason, false)?;
        Ok(())
    }
}

/// 発見フェーズの結果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryOutcome {
    PeerChosen {
        peer_id: String,
        display_name: String,
        method: DiscoveryMethod,
    },
    LocalFallback,
    NoPeerAborted,
}

/// **このデモのプロンプトが、実際にどこまで見えるか** (§1.24)。
///
/// v1 は秘匿を提供しない。だからこの型が答えるのは「安全か」ではなく
/// **「誰が読めるか」**である。嘘をつかないための型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPosture {
    /// この端末だけで走る。**プロンプトは外に出ない。**
    LocalOnly,
    /// ピアへ送る。**相手はプロンプトを平文で読める。**
    PeerCanReadPrompt { peer_gpu_model: String },
}

impl PrivacyPosture {
    /// 履歴に残す 1 行 (機械向け)。
    pub fn detail(&self) -> &'static str {
        match self {
            PrivacyPosture::LocalOnly => "local-only: prompt does not leave this device",
            PrivacyPosture::PeerCanReadPrompt { .. } => {
                "peer execution: the lender can read the prompt (v1 has no confidentiality)"
            }
        }
    }

    /// 利用者に見せる 1 行。**ここで初めて本当のことを言う。**
    pub fn user_line(&self) -> String {
        match self {
            PrivacyPosture::LocalOnly => {
                "この端末だけで実行します — プロンプトは外に出ません。".to_string()
            }
            PrivacyPosture::PeerCanReadPrompt { peer_gpu_model } => format!(
                "⚠️  相手 ({}) はプロンプトを読めます。v1 は秘匿を提供しません (SECURITY.md)。",
                peer_gpu_model
            ),
        }
    }
}

/// 終了画面で提示する 3 選択肢
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinaleOptions {
    pub keep_exploring: FinaleChoice,
    pub earn: FinaleChoice,
    pub run_again: FinaleChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinaleChoice {
    pub verb: String,
    pub one_line_description: String,
}

impl Default for FinaleOptions {
    fn default() -> Self {
        Self {
            keep_exploring: FinaleChoice {
                verb: "explore".to_string(),
                one_line_description: "もう少し眺める (ダッシュボードを開く)".to_string(),
            },
            earn: FinaleChoice {
                verb: "earn".to_string(),
                one_line_description: "自分の GPU を貸し出しに登録する".to_string(),
            },
            run_again: FinaleChoice {
                verb: "run".to_string(),
                one_line_description: "別のモデルを試す".to_string(),
            },
        }
    }
}

/// Apple-style "welcome letter" — 終了画面のテキスト
///
/// MacBook Air 封筒の現代版: 成功体験を 1 画面に凝縮。
pub fn format_welcome_letter(run: &FirstRun) -> String {
    let mut out = String::new();
    out.push_str("═══════════════════════════════════════════════════\n");
    out.push_str("  Rope 初回体験 — 完了\n");
    out.push_str("═══════════════════════════════════════════════════\n\n");

    // Apple の封筒モーメント: 結果を最初に見せる
    // Jobs は MacBook Air を封筒から取り出した「後」にスペックを語った
    let demo_result = run.demo_haiku.as_deref();
    if let Some(haiku) = demo_result {
        if !haiku.is_empty() {
            // ボックス幅を最長行に合わせる (固定幅だと将来溢れる)
            let lines: Vec<&str> = haiku.lines().collect();
            let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let w = max_len.max(20); // 最低 20 文字幅
            let border: String = "─".repeat(w + 2);
            out.push_str(&format!("  ┌{}┐\n", border));
            for line in &lines {
                let pad = w - line.chars().count();
                out.push_str(&format!("  │ {}{} │\n", line, " ".repeat(pad)));
            }
            out.push_str(&format!("  └{}┘\n", border));
            out.push('\n');
        }
    }

    let elapsed_sec = run.completed_ms.unwrap_or(run.elapsed_ms()) / 1000;
    out.push_str(&format!("⏱  所要時間: {}秒", elapsed_sec));
    if run.met_wow_target() {
        out.push_str(&format!(
            " (目標 {}s 以内 ✅)\n\n",
            run.config.wow_target_seconds
        ));
    } else {
        out.push_str("\n\n");
    }

    out.push_str("📋 タイムライン:\n");
    for entry in run.timeline() {
        let status = if entry.success { "✓" } else { "✗" };
        out.push_str(&format!(
            "  {} {} {:>5}ms  {}\n",
            status, entry.icon, entry.elapsed_ms, entry.label
        ));
    }

    if let Some(provider) = &run.provider_chosen_human {
        out.push_str(&format!("\n🏗  提供元: {}\n", provider));
    }
    if let Some(intent_id) = &run.demo_intent_id {
        out.push_str(&format!("📎 Intent: {}\n", super::short(intent_id, 8)));
    }

    out.push_str("\n次の 1 歩:\n");
    out.push_str("  rope earn    ← 自分の GPU を貸し出しに出す\n");
    out.push_str("  rope run ... ← 別のモデルを試す\n");
    out.push_str("  rope pair    ← 友人を直接招待する\n");
    out.push('\n');

    out
}

fn estimate_tokens(text: &str) -> u32 {
    // 粗い見積もり: 単語の 1.3 倍
    let words = text.split_whitespace().count() as u32;
    (words as f64 * 1.3).ceil() as u32
}

// Storage
/// first_run 記録の永続化パスを返す
pub fn first_run_path() -> std::path::PathBuf {
    super::config::config_dir().join("first_run.json")
}

/// 既存の first_run 記録を読込 (未実施なら None)
pub fn load_first_run() -> Result<Option<FirstRun>> {
    let path = first_run_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    match serde_json::from_str::<FirstRun>(&content) {
        Ok(fr) => Ok(Some(fr)),
        Err(_) => {
            // 破損時は退避して「初回未実行」扱い (G7.3)
            let backup = path.with_extension(format!("corrupt-{}", Utc::now().timestamp()));
            std::fs::rename(path, backup).ok();
            eprintln!("⚠️  初回体験記録 (first_run.json) が破損. 退避し最初からやり直し.");
            Ok(None)
        }
    }
}

/// first_run 記録を永続化
pub fn save_first_run(fr: &FirstRun) -> Result<()> {
    crate::core::config::atomic_write(&first_run_path(), &serde_json::to_string_pretty(fr)?)
}

/// 2 回目以降のユーザーには first-run を見せない
///
/// Apple の「設定は 1 度」哲学。以前完了してたらスキップ。
pub fn should_show_first_run() -> Result<bool> {
    let existing = load_first_run()?;
    match existing {
        None => Ok(true),
        Some(fr) => Ok(fr.current_stage != Stage::Done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let fr = FirstRun::new("alice");
        assert_eq!(fr.current_stage, Stage::Welcome);
        assert_eq!(fr.user_display_name, "alice");
        assert!(fr.completed_ms.is_none());
    }

    #[test]
    fn test_stage_icons_exist() {
        for s in [
            Stage::Welcome,
            Stage::IdentityReady,
            Stage::Discovering,
            Stage::Paired,
            Stage::DemoIntentCreated,
            Stage::DemoCompleted,
            Stage::Finale,
            Stage::Done,
            Stage::Aborted,
        ] {
            assert!(!s.icon().is_empty());
            assert!(!s.human_message().is_empty());
        }
    }

    #[test]
    fn test_happy_path_orchestration() {
        let mut fr = FirstRun::new("alice");
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();

        // H100 = TEE 対応、本番条件
        pair.paired.push(super::super::pair::PairedPeer {
            id: "p1".to_string(),
            display_name: "Bob's H100".to_string(),
            verified_pubkey: "pk".to_string(),
            endpoint: "192.168.1.2:9001".parse().unwrap(),
            session_key_hash: "h".to_string(),
            capabilities: super::super::pair::PeerCapabilities {
                gpu_count: 1,
                gpu_model: "NVIDIA H100".to_string(),
                vram_gb: 80,
                accepts_jobs: true,
                accepts_payment: true,
                payment_address: None,
                protocol_version: "rope/1.0".to_string(),
                tee_capable: true,
                tee_attested: false, // first_run demo: 未実証。実 NRAS 通過後に true
            },
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level: super::super::pair::TrustLevel::Unknown,
        });

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };

        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        let disc = orch.step_discovery_complete().unwrap();
        assert!(matches!(disc, DiscoveryOutcome::PeerChosen { .. }));

        // 新stage: プロンプトの行き先を開示する
        let posture = orch.step_privacy_check().unwrap();
        assert!(matches!(posture, PrivacyPosture::PeerCanReadPrompt { .. }));

        let intent = orch.step_build_intent().unwrap();
        assert!(!intent.id.is_empty());
        assert_eq!(intent.privacy, Privacy::ConfidentialCompute);

        orch.step_demo_completed("A quiet click, a distant fan, / Silicon warms for a stranger,\n/ My thanks ride on light.").unwrap();
        orch.step_finale().unwrap();
        orch.step_done().unwrap();

        assert_eq!(fr.current_stage, Stage::Done);
        assert!(fr.completed_ms.is_some());
        assert!(fr.provider_chosen_human.is_some());
    }

    #[test]
    fn test_no_peer_local_fallback() {
        let mut fr = FirstRun::new("alice");
        fr.config.fallback_to_local_if_no_peer = true;
        let mut pair = PairManager::default(); // 空
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };

        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        let disc = orch.step_discovery_complete().unwrap();
        assert!(matches!(disc, DiscoveryOutcome::LocalFallback));
        assert_eq!(fr.current_stage, Stage::Paired);
    }

    #[test]
    fn test_no_peer_no_fallback_aborts() {
        let mut fr = FirstRun::new("alice");
        fr.config.fallback_to_local_if_no_peer = false;
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };

        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        let disc = orch.step_discovery_complete().unwrap();
        assert!(matches!(disc, DiscoveryOutcome::NoPeerAborted));
        assert_eq!(fr.current_stage, Stage::Aborted);
    }

    #[test]
    fn test_stage_transition_enforces_order() {
        let mut fr = FirstRun::new("alice");
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();
        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };
        // Welcome から直接 Paired へは行けない
        assert!(orch.step_discovery_complete().is_err());
    }

    /// **民生 GPU のピアでも中止しない** — v1 が狙うのはまさにその層だから
    /// (§1.24)。以前はここで `Aborted` になっており、**想定利用者のハードウェアで
    /// 初回体験が止まっていた。** 代わりに「相手は読める」と正直に伝える。
    #[test]
    fn a_consumer_gpu_peer_is_not_aborted_but_disclosed() {
        let mut fr = FirstRun::new("alice");
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();

        // RTX 4090 = 民生 GPU、TEE 非対応
        pair.paired.push(super::super::pair::PairedPeer {
            id: "p1".to_string(),
            display_name: "Bob's RTX 4090".to_string(),
            verified_pubkey: "pk".to_string(),
            endpoint: "192.168.1.2:9001".parse().unwrap(),
            session_key_hash: "h".to_string(),
            capabilities: super::super::pair::PeerCapabilities {
                gpu_count: 1,
                gpu_model: "RTX 4090".to_string(),
                vram_gb: 24,
                accepts_jobs: true,
                accepts_payment: true,
                payment_address: None,
                protocol_version: "rope/1.0".to_string(),
                tee_capable: false, // RTX 4090 は CC 非対応 (Hopper/Blackwell 以外)
                tee_attested: false,
            },
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level: super::super::pair::TrustLevel::Unknown,
        });

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };
        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        orch.step_discovery_complete().unwrap();
        let posture = orch.step_privacy_check().unwrap();

        assert_eq!(
            posture,
            PrivacyPosture::PeerCanReadPrompt {
                peer_gpu_model: "RTX 4090".to_string()
            },
            "民生 GPU でも中止しない"
        );
        assert_eq!(fr.current_stage, Stage::AttestVerified);
        assert!(
            posture.user_line().contains("読めます"),
            "利用者に**読まれることを**伝える: {}",
            posture.user_line()
        );
    }

    /// ピアが居なければローカル実行 — **その時だけ「外に出ない」と言える**
    #[test]
    fn a_local_run_is_disclosed_as_staying_on_this_device() {
        let mut fr = FirstRun::new("alice");
        fr.config.fallback_to_local_if_no_peer = true;
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };
        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        orch.step_discovery_complete().unwrap();
        let posture = orch.step_privacy_check().unwrap();
        assert_eq!(posture, PrivacyPosture::LocalOnly);
        assert_eq!(fr.current_stage, Stage::AttestVerified);
        assert!(
            posture.user_line().contains("外に出ません"),
            "ローカルなら**そう言える** — ここだけは本当に安全: {}",
            posture.user_line()
        );
    }

    #[test]
    fn test_timeline_captures_all_stages() {
        let mut fr = FirstRun::new("alice");
        fr.transition(Stage::IdentityReady, "ok", true).unwrap();
        fr.transition(Stage::Discovering, "ok", true).unwrap();
        fr.transition(Stage::Paired, "ok", true).unwrap();

        let tl = fr.timeline();
        assert_eq!(tl.len(), 3);
        assert!(tl.iter().all(|e| e.success));
    }

    #[test]
    fn test_wow_target() {
        let mut fr = FirstRun::new("alice");
        // 各 stage を即座に通す → completed_ms 極小
        for to in [
            Stage::IdentityReady,
            Stage::Discovering,
            Stage::Paired,
            Stage::DemoIntentCreated,
            Stage::DemoCompleted,
            Stage::Finale,
            Stage::Done,
        ] {
            fr.transition(to, "ok", true).unwrap();
        }
        assert!(fr.completed_ms.is_some());
        assert!(fr.met_wow_target());
    }

    #[test]
    fn test_welcome_letter_renders() {
        let mut fr = FirstRun::new("alice");
        fr.transition(Stage::IdentityReady, "keys ok", true)
            .unwrap();
        fr.transition(Stage::Discovering, "mdns ok", true).unwrap();
        fr.transition(Stage::Paired, "got peer", true).unwrap();
        fr.transition(Stage::AttestVerified, "TEE ok", true)
            .unwrap();
        fr.transition(Stage::DemoIntentCreated, "intent ok", true)
            .unwrap();
        fr.demo_haiku = Some("Idle GPU hums—\nThree cents cross the cipher tide".to_string());
        fr.transition(Stage::DemoCompleted, "demo done", true)
            .unwrap();
        fr.provider_chosen_human = Some("Bob's M2 Mac".to_string());

        let letter = format_welcome_letter(&fr);
        assert!(letter.contains("Rope 初回体験"));
        assert!(letter.contains("Bob's M2 Mac"));
        assert!(letter.contains("rope earn"));
        // Apple 封筒モーメント: haiku が letter に表示される
        assert!(
            letter.contains("Idle GPU hums"),
            "welcome_letter は haiku 結果を表示すべき"
        );
        assert!(letter.contains("cipher tide"));
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        let t = estimate_tokens("Write a haiku about lending GPUs to strangers.");
        assert!(t > 0 && t < 20);
    }

    /// Round 21: haiku sampler は config プールから返す
    #[test]
    fn test_sample_haiku_returns_from_pool() {
        let fr = FirstRun::new("alice");
        let h = fr.sample_haiku_response();
        assert!(!h.is_empty());
        assert!(fr.config.demo_haiku_responses.iter().any(|x| x == h));
    }

    /// 同じ session 内では同じ haiku (deterministic per session)
    #[test]
    fn test_sample_haiku_deterministic_per_session() {
        let fr = FirstRun::new("alice");
        let h1 = fr.sample_haiku_response().to_string();
        let h2 = fr.sample_haiku_response().to_string();
        assert_eq!(h1, h2);
    }

    /// 空プールで panic しない
    #[test]
    fn test_sample_haiku_handles_empty_pool() {
        let mut fr = FirstRun::new("alice");
        fr.config.demo_haiku_responses = vec![];
        assert_eq!(fr.sample_haiku_response(), "");
    }

    #[test]
    fn test_intent_built_with_tee_privacy() {
        let mut fr = FirstRun::new("alice");
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();
        fr.config.fallback_to_local_if_no_peer = true;

        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };
        orch.step_identity().unwrap();
        orch.step_discovery_start().unwrap();
        orch.step_discovery_complete().unwrap();
        orch.step_privacy_check().unwrap(); // ピアなし → ローカルのみ
        let intent = orch.step_build_intent().unwrap();
        assert_eq!(intent.privacy, Privacy::ConfidentialCompute);
    }

    #[test]
    fn test_finale_offers_three_options() {
        let opts = FinaleOptions::default();
        assert_eq!(opts.earn.verb, "earn");
        assert_eq!(opts.run_again.verb, "run");
        assert_eq!(opts.keep_exploring.verb, "explore");
    }

    #[test]
    fn test_abort_records_reason() {
        let mut fr = FirstRun::new("alice");
        let mut pair = PairManager::default();
        let mut ecash = EcashManager::default();
        let mut conf = ConfidentialManager::default();
        let mut orch = FirstRunOrchestrator {
            pair: &mut pair,
            ecash: &mut ecash,
            confidential: &mut conf,
            first_run: &mut fr,
        };
        orch.abort("user pressed ctrl-c").unwrap();
        assert_eq!(fr.current_stage, Stage::Aborted);
        assert_eq!(fr.abort_reason, Some("user pressed ctrl-c".to_string()));
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut fr = FirstRun::new("alice");
        fr.transition(Stage::IdentityReady, "ok", true).unwrap();
        fr.transition(Stage::Discovering, "ok", true).unwrap();
        let json1 = serde_json::to_string(&fr).unwrap();
        let back: FirstRun = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "FirstRun roundtrip");
    }
}
