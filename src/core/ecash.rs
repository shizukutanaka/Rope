//! ecash — Cashu-style bearer tokens + escrow + streaming + LN bridge
//!
//! ROPE_2028 + LANDMINES で承認された唯一の新規モジュール。
//! 競合8製品の穴4つを一発で埋める:
//!
//! 1. 他人同士の決済 (EXO 範囲外、Petals 無償)
//! 2. ジョブ中断耐性 (Vast.ai 最大不満)
//! 3. ストリーミング決済 (全競合が時間単位、1秒単位なし)
//! 4. Lightning 初回チャネル$5 > 3¢問題 (Cashu mint 経由で回避)
//!
//! ## 旧 spot.rs 代替宣言 (Round 16)
//!
//! 当初 ROPE_2028 核30 に「spot」(スポット市場決済) を含めていたが、
//! `Escrow` + `deadman` + `dispute_resolution` の組合せで完全に代替可能。
//! - 中断耐性: deadman_at で自動返金 (spot ジョブの突然終了)
//! - 価格保証: amount_sats 固定 (オークション不要、bearer 性質で十分)
//! - 信頼最小化: mint federation + 3-way escrow (中央決済所不要)
//! 本モジュールが ROPE_2028 「核 30」のスポット概念を吸収する。
//!
//! ## 設計
//! - Cashu プロトコル(bearer ecash) を中核
//! - mint = 信頼最小化された LN ↔ ecash 変換ゲートウェイ
//! - bearer token = チャネル不要、オフライン検証可、匿名
//! - escrow = 3者保管 (Alice/Bob/mint) で dispute 時に判定
//! - streaming = 小額 token を秒単位で stream、断絶時に自動返金
//!
//! ## 外部依存なし原則
//! 本モジュールは型とステートマシンのみ。実際の暗号は後で接続。
//! トランザクションは全て状態遷移として記録、ネット I/O は含めない。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ecash マネージャ (ウォレット + escrow + streaming の複合)
#[derive(Debug, Serialize, Deserialize)]
pub struct EcashManager {
    /// 自分の ecash 残高 (未使用トークン)
    pub wallet: Wallet,
    /// 使用済みトークン(二重使用防止のナル履歴)
    pub spent_nullifiers: Vec<String>,
    /// 登録された mint
    pub mints: Vec<Mint>,
    /// アクティブ escrow (Rope ジョブ単位)
    pub escrows: Vec<Escrow>,
    /// escrow 履歴
    pub escrow_history: Vec<Escrow>,
    /// アクティブ streaming セッション
    pub streams: Vec<StreamSession>,
    /// streaming 履歴
    pub stream_history: Vec<StreamSession>,
    /// Lightning 接続 (mint経由、直接チャネル不要)
    pub lightning_links: Vec<LightningLink>,
    /// 設定
    pub config: EcashConfig,
    /// 統計
    pub stats: EcashStats,
    pub updated_at: DateTime<Utc>,
}

impl Default for EcashManager {
    fn default() -> Self {
        Self {
            wallet: Wallet::default(),
            spent_nullifiers: Vec::new(),
            mints: Vec::new(),
            escrows: Vec::new(),
            escrow_history: Vec::new(),
            streams: Vec::new(),
            stream_history: Vec::new(),
            lightning_links: Vec::new(),
            config: EcashConfig::default(),
            stats: EcashStats::default(),
            updated_at: Utc::now(),
        }
    }
}

/// ウォレット — 未使用 ecash proof の集合
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Wallet {
    /// 所持 proof の mint 別グループ
    pub proofs_by_mint: HashMap<String, Vec<Proof>>,
    /// 現在のクラウド合計 (satoshis)
    pub total_sats: u64,
}

/// Cashu proof — 使用可能な bearer token の構成要素
///
/// Cashu NUT-00 仕様 (V4 wire format) に準拠:
///   proof = { amount, secret, C, id }
/// 本構造体は仕様 + 内部用フィールドの拡張版:
///   - `id` (内部 UUID) ≠ Cashu の `id` (keyset id, hex8)
///   - `keyset_id` で Cashu spec の id を保持
///   - `secret` は base16 32バイト (Cashu 仕様)
///   - `c` は unblinded signature C (33バイト圧縮 secp256k1 point)
///   - `nullifier` は BLAKE3(secret) ベースで再現可能 (二重使用検出用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    /// 内部 UUID (rope wallet 管理用)
    pub id: String,
    /// 額面 (satoshis、常に 2^n)
    pub amount_sats: u64,
    /// 発行した mint の ID (rope 内 mint registry の ID)
    pub mint_id: String,
    /// Cashu keyset id (mint の `/v1/keys` エンドポイントから取得、hex 16文字)
    pub keyset_id: String,
    /// Cashu secret (base16 64文字 = 32バイト乱数)
    pub secret: String,
    /// Cashu unblinded signature C (33バイト = 圧縮 secp256k1 point hex 66文字)
    pub c: String,
    /// 既存互換: signature 表示用 (= C と同じ値)
    pub signature: String,
    /// 二重使用検出用 nullifier (= BLAKE3-256 first 32 bytes of `secret`、hex)
    pub nullifier: String,
    /// 作成日時
    pub created_at: DateTime<Utc>,
}

/// Mint — LN 預入 ↔ ecash 発行ゲートウェイ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mint {
    pub id: String,
    pub url: String,
    pub pubkey: String,
    pub supported_denominations_sats: Vec<u64>, // 2^0 〜 2^30
    pub lightning_supported: bool,
    pub onchain_supported: bool,
    pub trust_level: MintTrust,
    pub total_issued_sats: u64,
    pub total_redeemed_sats: u64,
    pub last_seen: DateTime<Utc>,
    pub verified_keyset_at: Option<DateTime<Utc>>,
}

/// mint 信頼度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MintTrust {
    /// 未確認、使用非推奨
    Unknown,
    /// 稼働中観測、情報のみ
    Observed,
    /// federated mint (複数 guardian)
    Federated,
    /// ユーザー明示信頼
    Trusted,
}

/// Escrow — ジョブごとの預託
///
/// 3状態遷移:
///   Deposited (Alice 預入)
///     → Released (Bob 完遂 → Bob 受取)
///     → Refunded (Bob 失敗/タイムアウト → Alice 返金)
///     → Disputed (主観争い → mint 調停)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escrow {
    pub id: String,
    /// 関連 Rope ジョブ ID
    pub job_id: String,
    /// 支払い者 (Alice)
    pub payer_pubkey: String,
    /// 受取者 (Bob)
    pub payee_pubkey: String,
    /// 仲介 mint
    pub mint_id: String,
    pub amount_sats: u64,
    pub state: EscrowState,
    /// deadman スイッチ — この時刻までに完遂しなければ自動返金
    pub deadman_at: DateTime<Utc>,
    /// 完遂条件 (例: "inference.output.hash == $HASH")
    pub completion_condition: String,
    /// 完遂証拠 (verifiable.rs 由来の attestation)
    pub completion_proof: Option<String>,
    /// dispute 理由
    pub dispute_reason: Option<String>,
    /// mint の調停結果
    pub mint_resolution: Option<DisputeResolution>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// escrow 状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowState {
    /// Alice が資金を mint にロック
    Deposited,
    /// Bob がジョブ開始、証拠待ち
    InProgress,
    /// Bob が完遂証拠提出 → Bob へ解放
    Released,
    /// deadman または Bob キャンセルで Alice 返金
    Refunded,
    /// 係争中、mint 調停待ち
    Disputed,
    /// 調停完了、どちらかに解決
    Resolved,
}

impl std::fmt::Display for EscrowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscrowState::Deposited => write!(f, "預託済"),
            EscrowState::InProgress => write!(f, "進行中"),
            EscrowState::Released => write!(f, "解放済"),
            EscrowState::Refunded => write!(f, "返金済"),
            EscrowState::Disputed => write!(f, "係争中"),
            EscrowState::Resolved => write!(f, "解決済"),
        }
    }
}

/// 係争判定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisputeResolution {
    PayeeWins,
    PayerWins,
    Split,
}

/// Streaming — 継続的な少額支払い
///
/// 例: "inference を 1 秒ごとに 5 sat 払う"
/// - Alice が 10,000 sat を stream にロック
/// - 1秒ごとに 5 sat を Bob に滲出
/// - Alice が止めるとそこまで支払い、残金返却
/// - 全競合は「時間単位」決済、これが 1 秒単位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSession {
    pub id: String,
    pub job_id: String,
    pub payer_pubkey: String,
    pub payee_pubkey: String,
    pub mint_id: String,
    /// 総ロック額
    pub total_locked_sats: u64,
    /// 流出済み額
    pub drained_sats: u64,
    /// 単位時間あたり流量
    pub rate_sats_per_second: u64,
    /// 開始時刻
    pub started_at: DateTime<Utc>,
    /// 最終 tick 時刻
    pub last_tick_at: DateTime<Utc>,
    pub state: StreamState,
    /// 自動停止条件
    pub max_duration_seconds: u32,
    pub stop_on_payer_idle_seconds: u32,
}

/// streaming 状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamState {
    Opening,
    Active,
    Paused,
    Closing,
    Closed,
    /// 異常終了 (ネット断、受取側応答なし)
    Stalled,
}

/// Lightning リンク — mint 経由間接接続
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightningLink {
    pub id: String,
    pub mint_id: String,
    /// LN ノード情報 (情報のみ、直接チャネルは持たない)
    pub ln_node_pubkey: String,
    pub ln_node_alias: String,
    /// 最近の交換レート情報 (参考値)
    pub exchange_rate_sats_per_usd: f64,
    pub last_deposit_at: Option<DateTime<Utc>>,
    pub last_withdrawal_at: Option<DateTime<Utc>>,
    pub lifetime_deposits_sats: u64,
    pub lifetime_withdrawals_sats: u64,
}

/// 設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcashConfig {
    pub enabled: bool,
    /// デフォルト mint (複数設定、フェイルオーバー順)
    pub default_mint_ids: Vec<String>,
    /// 1 escrow の最大額 (sat)
    pub max_single_escrow_sats: u64,
    /// deadman デフォルト
    pub default_deadman_minutes: u32,
    /// stream デフォルトレート
    pub default_stream_rate_sats_per_sec: u64,
    /// streaming が自動停止する idle 時間
    pub stream_idle_stop_seconds: u32,
    /// 最大 stream 額
    pub max_stream_total_sats: u64,
    /// mint federation 最低 guardian 数
    pub min_federated_guardians: u32,
    /// 係争時に自動的に Split
    pub auto_split_on_dispute: bool,
    /// nullifier キャッシュ容量
    pub max_nullifier_history: usize,
}

impl Default for EcashConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_mint_ids: Vec::new(),
            max_single_escrow_sats: 100_000, // 約 $50
            default_deadman_minutes: 60,
            default_stream_rate_sats_per_sec: 1,
            stream_idle_stop_seconds: 30,
            max_stream_total_sats: 1_000_000,
            min_federated_guardians: 3,
            auto_split_on_dispute: false,
            max_nullifier_history: 100_000,
        }
    }
}

/// 統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EcashStats {
    pub total_mints_registered: u32,
    pub total_proofs_received_sats: u64,
    pub total_proofs_spent_sats: u64,
    pub current_balance_sats: u64,
    pub total_escrows_opened: u64,
    pub total_escrows_released: u64,
    pub total_escrows_refunded: u64,
    pub total_escrows_disputed: u64,
    pub avg_escrow_sats: f64,
    pub total_streams_opened: u64,
    pub total_stream_sats_drained: u64,
    pub deadman_auto_refunds: u64,
    pub double_spend_attempts_blocked: u64,
}

impl EcashManager {
    // ========================================================================
    // Mint management
    // ========================================================================

    /// mint を登録
    pub fn add_mint(&mut self, mint: Mint) -> Result<()> {
        if self.mints.iter().any(|m| m.id == mint.id) {
            anyhow::bail!("mint 既登録");
        }
        self.mints.push(mint);
        self.stats.total_mints_registered = self.mints.len() as u32;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// mint の信頼度を昇格
    pub fn trust_mint(&mut self, mint_id: &str, level: MintTrust) -> Result<()> {
        let mint = self
            .mints
            .iter_mut()
            .find(|m| m.id == mint_id)
            .context("mint 無し")?;
        // 連邦 guardian 不足時は Federated 不可
        if level == MintTrust::Federated && self.config.min_federated_guardians == 0 {
            anyhow::bail!("federated 運用は設定で無効");
        }
        mint.trust_level = level;
        mint.verified_keyset_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    // ========================================================================
    // Wallet operations (Cashu protocol 要約)
    // ========================================================================

    /// mint から ecash を発行 (LN 預入後に呼ぶ想定)
    pub fn mint_tokens(&mut self, mint_id: &str, amount_sats: u64) -> Result<Vec<Proof>> {
        let mint = self
            .mints
            .iter()
            .find(|m| m.id == mint_id)
            .context("mint 無し")?;
        if mint.trust_level == MintTrust::Unknown {
            anyhow::bail!("mint 未検証、trust_mint 先行");
        }

        // 2^n 分解 (Cashu 標準)
        let denominations = Self::decompose_powers_of_two(amount_sats);
        let mut proofs = Vec::new();

        // Cashu keyset id は本来 mint の `/v1/keys` から取得。
        // 本実装は pure state machine のため、mint id の先頭 16 hex を keyset id として代用。
        // 実 mint 接続時 (net/cashu_mint.rs 翻訳層) で正しい keyset id に置換される。
        let keyset_id = mint_id
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(16)
            .collect::<String>();
        let keyset_id = if keyset_id.len() == 16 {
            keyset_id
        } else {
            // mint_id が hex でない場合は decode 可能な 16 hex に変換
            let mut s = String::new();
            for b in mint_id.bytes().take(8) {
                s.push_str(&format!("{:02x}", b));
            }
            s
        };

        for d in denominations {
            // 32 byte 乱数 secret (Cashu NUT-00 V4)
            let mut secret_bytes = [0u8; 32];
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut secret_bytes);
            let secret = hex::encode(secret_bytes);

            // BLAKE3(secret) で nullifier 計算 — 本物 hash、replay 検知に必要
            let null_hash = blake3::hash(&secret_bytes);
            let nullifier = hex::encode(&null_hash.as_bytes()[..32]);

            // C (unblinded signature) は本来 mint が BDHHKE で計算する。
            // pure state machine では deterministic placeholder を生成し、
            // 実 mint 接続時 (net/cashu_mint.rs 翻訳層) で本物 C に置換する。
            // 33 byte 圧縮 secp256k1 point 形式: 02/03 prefix + 32 byte x-coordinate
            let c_bytes = blake3::hash(format!("C|{}|{}|{}", keyset_id, d, secret).as_bytes());
            let mut c_compressed = [0u8; 33];
            c_compressed[0] = 0x02; // even-y prefix
            c_compressed[1..33].copy_from_slice(&c_bytes.as_bytes()[..32]);
            let c_hex = hex::encode(c_compressed);

            let proof = Proof {
                id: uuid::Uuid::now_v7().to_string(),
                amount_sats: d,
                mint_id: mint_id.to_string(),
                keyset_id: keyset_id.clone(),
                secret,
                c: c_hex.clone(),
                signature: c_hex,
                nullifier,
                created_at: Utc::now(),
            };
            proofs.push(proof);
        }

        // wallet に追加
        let bucket = self
            .wallet
            .proofs_by_mint
            .entry(mint_id.to_string())
            .or_default();
        bucket.extend(proofs.iter().cloned());
        self.wallet.total_sats += amount_sats;

        self.stats.total_proofs_received_sats += amount_sats;
        self.stats.current_balance_sats = self.wallet.total_sats;
        self.updated_at = Utc::now();
        Ok(proofs)
    }

    /// bearer proof を送信 (第三者に渡す前に wallet から消す)
    pub fn spend_proofs(&mut self, mint_id: &str, proof_ids: &[String]) -> Result<Vec<Proof>> {
        let bucket = self
            .wallet
            .proofs_by_mint
            .get_mut(mint_id)
            .context("mint バケット無し")?;

        let mut spent = Vec::new();
        let before_len = bucket.len();
        bucket.retain(|p| {
            if proof_ids.contains(&p.id) {
                spent.push(p.clone());
                false
            } else {
                true
            }
        });
        if spent.len() != proof_ids.len() {
            anyhow::bail!("一部 proof 見つからず");
        }

        let spent_total: u64 = spent.iter().map(|p| p.amount_sats).sum();
        self.wallet.total_sats = self.wallet.total_sats.saturating_sub(spent_total);

        // nullifier 記録
        for p in &spent {
            self.spent_nullifiers.push(p.nullifier.clone());
        }
        if self.spent_nullifiers.len() > self.config.max_nullifier_history {
            let drop_count = self.spent_nullifiers.len() - self.config.max_nullifier_history;
            self.spent_nullifiers.drain(..drop_count);
        }

        self.stats.total_proofs_spent_sats += spent_total;
        self.stats.current_balance_sats = self.wallet.total_sats;
        let _ = before_len;
        self.updated_at = Utc::now();
        Ok(spent)
    }

    /// 受信 proof を検証 (二重使用ブロック)
    pub fn receive_proofs(&mut self, proofs: Vec<Proof>) -> Result<u64> {
        let mut received = 0u64;
        for p in &proofs {
            if self.spent_nullifiers.contains(&p.nullifier) {
                self.stats.double_spend_attempts_blocked += 1;
                anyhow::bail!("二重使用検出: {}", p.nullifier);
            }
            // mint 信頼度チェック
            let mint = self
                .mints
                .iter()
                .find(|m| m.id == p.mint_id)
                .context("未知の mint が発行した proof")?;
            if mint.trust_level == MintTrust::Unknown {
                anyhow::bail!("mint 未検証");
            }
            received += p.amount_sats;
        }

        // swap 推奨: 受信 proof を自分の mint で再発行して盗難耐性高める
        // (本実装では proof そのまま保管)
        for p in proofs {
            let bucket = self
                .wallet
                .proofs_by_mint
                .entry(p.mint_id.clone())
                .or_default();
            bucket.push(p);
        }
        self.wallet.total_sats += received;
        self.stats.total_proofs_received_sats += received;
        self.stats.current_balance_sats = self.wallet.total_sats;
        self.updated_at = Utc::now();
        Ok(received)
    }

    fn decompose_powers_of_two(mut amount: u64) -> Vec<u64> {
        let mut result = Vec::new();
        let mut power = 1u64;
        while amount > 0 {
            if amount & 1 == 1 {
                result.push(power);
            }
            amount >>= 1;
            power <<= 1;
        }
        result
    }

    // ========================================================================
    // Escrow (Vast.ai 「ジョブ突然切断」問題を解決)
    // ========================================================================

    /// escrow を開設 (Alice が Bob にジョブ依頼)
    pub fn open_escrow(
        &mut self,
        job_id: &str,
        payer_pubkey: &str,
        payee_pubkey: &str,
        mint_id: &str,
        amount_sats: u64,
        completion_condition: &str,
    ) -> Result<Escrow> {
        if amount_sats == 0 {
            anyhow::bail!("escrow 額は 1 sat 以上必須");
        }
        if amount_sats > self.config.max_single_escrow_sats {
            anyhow::bail!(
                "escrow 最大額超過 ({} > {})",
                amount_sats,
                self.config.max_single_escrow_sats
            );
        }
        if self.wallet.total_sats < amount_sats {
            anyhow::bail!("残高不足");
        }
        let mint = self
            .mints
            .iter()
            .find(|m| m.id == mint_id)
            .context("mint 無し")?;
        if mint.trust_level == MintTrust::Unknown {
            anyhow::bail!("mint 未検証");
        }

        // 金額分の proof をロック (wallet から別枠へ)
        self.lock_funds(mint_id, amount_sats)?;

        let deadman =
            Utc::now() + chrono::Duration::minutes(self.config.default_deadman_minutes as i64);

        let escrow = Escrow {
            id: uuid::Uuid::now_v7().to_string(),
            job_id: job_id.to_string(),
            payer_pubkey: payer_pubkey.to_string(),
            payee_pubkey: payee_pubkey.to_string(),
            mint_id: mint_id.to_string(),
            amount_sats,
            state: EscrowState::Deposited,
            deadman_at: deadman,
            completion_condition: completion_condition.to_string(),
            completion_proof: None,
            dispute_reason: None,
            mint_resolution: None,
            created_at: Utc::now(),
            resolved_at: None,
        };

        self.escrows.push(escrow.clone());
        self.stats.total_escrows_opened += 1;

        let total = self.stats.total_escrows_opened as f64;
        self.stats.avg_escrow_sats =
            (self.stats.avg_escrow_sats * (total - 1.0) + amount_sats as f64) / total;

        self.updated_at = Utc::now();
        Ok(escrow)
    }

    /// escrow 内の資金を wallet から差し引く (実装簡略化: total 減額のみ)
    fn lock_funds(&mut self, mint_id: &str, amount_sats: u64) -> Result<()> {
        let bucket = self
            .wallet
            .proofs_by_mint
            .get_mut(mint_id)
            .context("mint バケット無し")?;
        let mut locked = 0u64;
        let mut kept = Vec::new();
        for p in bucket.drain(..) {
            if locked < amount_sats {
                locked += p.amount_sats;
                // locked proof は escrow 側に保管 (簡略化: 破棄)
            } else {
                kept.push(p);
            }
        }
        *bucket = kept;
        self.wallet.total_sats = self.wallet.total_sats.saturating_sub(amount_sats);
        self.stats.current_balance_sats = self.wallet.total_sats;
        Ok(())
    }

    /// Bob がジョブ開始
    pub fn mark_escrow_in_progress(&mut self, escrow_id: &str) -> Result<()> {
        let e = self
            .escrows
            .iter_mut()
            .find(|e| e.id == escrow_id)
            .context("escrow 無し")?;
        if e.state != EscrowState::Deposited {
            anyhow::bail!("状態不整合: {}", e.state);
        }
        e.state = EscrowState::InProgress;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Bob が完遂、証拠提出 → 解放
    pub fn release_escrow(&mut self, escrow_id: &str, completion_proof: &str) -> Result<()> {
        let e = self
            .escrows
            .iter_mut()
            .find(|e| e.id == escrow_id)
            .context("escrow 無し")?;
        if !matches!(e.state, EscrowState::Deposited | EscrowState::InProgress) {
            anyhow::bail!("解放不可状態: {}", e.state);
        }
        e.completion_proof = Some(completion_proof.to_string());
        e.state = EscrowState::Released;
        e.resolved_at = Some(Utc::now());

        self.stats.total_escrows_released += 1;
        self.archive_escrow(escrow_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Alice が返金要求 (deadman 経過 or Bob 同意)
    pub fn refund_escrow(&mut self, escrow_id: &str) -> Result<()> {
        let e = self
            .escrows
            .iter_mut()
            .find(|e| e.id == escrow_id)
            .context("escrow 無し")?;
        if !matches!(e.state, EscrowState::Deposited | EscrowState::InProgress) {
            anyhow::bail!("返金不可状態: {}", e.state);
        }
        e.state = EscrowState::Refunded;
        e.resolved_at = Some(Utc::now());

        // 返金額を wallet に戻す (簡略化)
        self.wallet.total_sats += e.amount_sats;
        self.stats.current_balance_sats = self.wallet.total_sats;

        self.stats.total_escrows_refunded += 1;
        self.archive_escrow(escrow_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// 係争を開始 (mint 調停待ち)
    pub fn dispute_escrow(&mut self, escrow_id: &str, reason: &str) -> Result<()> {
        let e = self
            .escrows
            .iter_mut()
            .find(|e| e.id == escrow_id)
            .context("escrow 無し")?;
        if !matches!(e.state, EscrowState::Deposited | EscrowState::InProgress) {
            anyhow::bail!("係争不可状態");
        }
        e.state = EscrowState::Disputed;
        e.dispute_reason = Some(reason.to_string());
        self.stats.total_escrows_disputed += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// mint が係争を調停
    pub fn resolve_dispute(
        &mut self,
        escrow_id: &str,
        resolution: DisputeResolution,
    ) -> Result<()> {
        let e = self
            .escrows
            .iter_mut()
            .find(|e| e.id == escrow_id)
            .context("escrow 無し")?;
        if e.state != EscrowState::Disputed {
            anyhow::bail!("非係争状態");
        }
        e.mint_resolution = Some(resolution);
        e.state = EscrowState::Resolved;
        e.resolved_at = Some(Utc::now());

        let amount = e.amount_sats;
        match resolution {
            DisputeResolution::PayerWins => {
                self.wallet.total_sats += amount;
                self.stats.total_escrows_refunded += 1;
            }
            DisputeResolution::PayeeWins => {
                self.stats.total_escrows_released += 1;
            }
            DisputeResolution::Split => {
                self.wallet.total_sats += amount / 2;
                self.stats.total_escrows_refunded += 1;
            }
        }
        self.stats.current_balance_sats = self.wallet.total_sats;
        self.archive_escrow(escrow_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// deadman 経過 escrow を自動処理 (定期呼び出し想定)
    pub fn process_deadman(&mut self) -> Result<u32> {
        let now = Utc::now();
        let expired_ids: Vec<String> = self
            .escrows
            .iter()
            .filter(|e| {
                e.deadman_at < now
                    && matches!(e.state, EscrowState::Deposited | EscrowState::InProgress)
            })
            .map(|e| e.id.clone())
            .collect();

        let count = expired_ids.len() as u32;
        for id in expired_ids {
            self.refund_escrow(&id).ok();
            self.stats.deadman_auto_refunds += 1;
        }
        Ok(count)
    }

    fn archive_escrow(&mut self, escrow_id: &str) {
        if let Some(pos) = self.escrows.iter().position(|e| e.id == escrow_id) {
            let e = self.escrows.remove(pos);
            self.escrow_history.push(e);
            if self.escrow_history.len() > 10_000 {
                self.escrow_history.drain(..500);
            }
        }
    }

    // ========================================================================
    // Streaming (1秒単位課金、競合全員「時間単位」)
    // ========================================================================

    /// stream 開設
    pub fn open_stream(
        &mut self,
        job_id: &str,
        payer_pubkey: &str,
        payee_pubkey: &str,
        mint_id: &str,
        total_sats: u64,
        rate_sats_per_sec: u64,
    ) -> Result<StreamSession> {
        if total_sats == 0 {
            anyhow::bail!("stream 額は 1 sat 以上必須");
        }
        if total_sats > self.config.max_stream_total_sats {
            anyhow::bail!("stream 最大額超過");
        }
        if rate_sats_per_sec == 0 {
            anyhow::bail!("rate 0 無効");
        }
        if self.wallet.total_sats < total_sats {
            anyhow::bail!("残高不足");
        }
        self.lock_funds(mint_id, total_sats)?;

        let max_duration = (total_sats / rate_sats_per_sec) as u32;
        let session = StreamSession {
            id: uuid::Uuid::now_v7().to_string(),
            job_id: job_id.to_string(),
            payer_pubkey: payer_pubkey.to_string(),
            payee_pubkey: payee_pubkey.to_string(),
            mint_id: mint_id.to_string(),
            total_locked_sats: total_sats,
            drained_sats: 0,
            rate_sats_per_second: rate_sats_per_sec,
            started_at: Utc::now(),
            last_tick_at: Utc::now(),
            state: StreamState::Active,
            max_duration_seconds: max_duration,
            stop_on_payer_idle_seconds: self.config.stream_idle_stop_seconds,
        };

        self.streams.push(session.clone());
        self.stats.total_streams_opened += 1;
        self.updated_at = Utc::now();
        Ok(session)
    }

    /// stream を tick (定期呼び出し、支払い時間を進める)
    pub fn tick_stream(&mut self, stream_id: &str) -> Result<u64> {
        let now = Utc::now();
        let s = self
            .streams
            .iter_mut()
            .find(|s| s.id == stream_id)
            .context("stream 無し")?;

        if s.state != StreamState::Active {
            return Ok(0);
        }

        let elapsed_sec = (now - s.last_tick_at).num_seconds().max(0) as u64;
        let delta = elapsed_sec * s.rate_sats_per_second;
        let remaining = s.total_locked_sats - s.drained_sats;
        let to_drain = delta.min(remaining);

        s.drained_sats += to_drain;
        s.last_tick_at = now;

        // 残額 0 なら自動クローズ
        if s.drained_sats >= s.total_locked_sats {
            s.state = StreamState::Closed;
        }

        self.stats.total_stream_sats_drained += to_drain;
        self.updated_at = Utc::now();
        Ok(to_drain)
    }

    /// stream 停止 (Alice が意図的に止める or ジョブ終了)
    pub fn close_stream(&mut self, stream_id: &str) -> Result<u64> {
        self.tick_stream(stream_id).ok();

        let s = self
            .streams
            .iter_mut()
            .find(|s| s.id == stream_id)
            .context("stream 無し")?;

        s.state = StreamState::Closed;
        let refund = s.total_locked_sats - s.drained_sats;

        // 未使用分を wallet に返す
        self.wallet.total_sats += refund;
        self.stats.current_balance_sats = self.wallet.total_sats;

        // 履歴へ移動
        if let Some(pos) = self.streams.iter().position(|s| s.id == stream_id) {
            let s = self.streams.remove(pos);
            self.stream_history.push(s);
            if self.stream_history.len() > 10_000 {
                self.stream_history.drain(..500);
            }
        }

        self.updated_at = Utc::now();
        Ok(refund)
    }

    /// 払い手 idle 検出 → stalled 判定 (ネット断対応)
    pub fn check_idle_streams(&mut self) -> u32 {
        let now = Utc::now();
        let mut stalled = 0u32;
        for s in self.streams.iter_mut() {
            if s.state != StreamState::Active {
                continue;
            }
            let idle = (now - s.last_tick_at).num_seconds();
            if idle > s.stop_on_payer_idle_seconds as i64 {
                s.state = StreamState::Stalled;
                stalled += 1;
            }
        }
        stalled
    }
}

// Storage
/// ecash ウォレットの永続化パスを返す
pub fn ecash_path() -> std::path::PathBuf {
    crate::core::config::config_dir().join("ecash.json")
}

/// ecash マネージャーを永続化ファイルから読込
pub fn load_ecash() -> Result<EcashManager> {
    Ok(crate::core::config::load_or_recover(
        &ecash_path(),
        "ecash 残高 (ecash.json)",
    ))
}

/// ecash マネージャーを永続化ファイルに保存
pub fn save_ecash(m: &EcashManager) -> Result<()> {
    crate::core::config::atomic_write(&ecash_path(), &serde_json::to_string_pretty(m)?)
}

/// ecash ダッシュボード文字列を生成
pub fn format_ecash(m: &EcashManager) -> String {
    let mut out = String::new();
    out.push_str("ecash ウォレット:\n");
    out.push_str("═══════════════════════════════════════════════════\n\n");
    out.push_str(&format!("💰 残高: {} sats\n", m.wallet.total_sats));
    out.push_str(&format!(
        "  登録 mint: {} (federated: {})\n",
        m.stats.total_mints_registered,
        m.mints
            .iter()
            .filter(|m| m.trust_level == MintTrust::Federated)
            .count()
    ));
    out.push_str(&format!(
        "  累計入金: {} sats / 支出: {} sats\n",
        m.stats.total_proofs_received_sats, m.stats.total_proofs_spent_sats
    ));
    out.push_str(&format!(
        "\n🔒 エスクロー: active={} / released={} / refunded={} / disputed={}\n",
        m.escrows.len(),
        m.stats.total_escrows_released,
        m.stats.total_escrows_refunded,
        m.stats.total_escrows_disputed
    ));
    out.push_str(&format!(
        "  平均額: {:.0} sats / deadman自動返金: {}\n",
        m.stats.avg_escrow_sats, m.stats.deadman_auto_refunds
    ));
    out.push_str(&format!(
        "\n🌊 ストリーム: active={} / 総流出: {} sats\n",
        m.streams.len(),
        m.stats.total_stream_sats_drained
    ));
    out.push_str(&format!(
        "\n🛡  二重使用ブロック: {}\n",
        m.stats.double_spend_attempts_blocked
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mint(id: &str) -> Mint {
        Mint {
            id: id.to_string(),
            url: format!("https://{}.example.com", id),
            pubkey: format!("pk-{}", id),
            supported_denominations_sats: vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024],
            lightning_supported: true,
            onchain_supported: false,
            trust_level: MintTrust::Unknown,
            total_issued_sats: 0,
            total_redeemed_sats: 0,
            last_seen: Utc::now(),
            verified_keyset_at: None,
        }
    }

    #[test]
    fn test_powers_of_two_decomposition() {
        assert_eq!(EcashManager::decompose_powers_of_two(0), Vec::<u64>::new());
        assert_eq!(EcashManager::decompose_powers_of_two(1), vec![1]);
        assert_eq!(EcashManager::decompose_powers_of_two(5), vec![1, 4]);
        assert_eq!(EcashManager::decompose_powers_of_two(100), vec![4, 32, 64]);
        assert_eq!(
            EcashManager::decompose_powers_of_two(255),
            vec![1, 2, 4, 8, 16, 32, 64, 128]
        );
    }

    #[test]
    fn test_mint_requires_trust_before_use() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        // trust_level == Unknown → mint 不可
        assert!(m.mint_tokens("mint1", 100).is_err());
    }

    #[test]
    fn test_mint_and_spend_flow() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();

        let proofs = m.mint_tokens("mint1", 100).unwrap();
        let total: u64 = proofs.iter().map(|p| p.amount_sats).sum();
        assert_eq!(total, 100);
        assert_eq!(m.wallet.total_sats, 100);

        let ids: Vec<String> = proofs.iter().take(2).map(|p| p.id.clone()).collect();
        let spent = m.spend_proofs("mint1", &ids).unwrap();
        assert_eq!(spent.len(), 2);
    }

    /// Round 21: Cashu NUT-00 spec 準拠検証
    #[test]
    fn test_proofs_have_cashu_shaped_fields() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("deadbeef00000000")).unwrap();
        m.trust_mint("deadbeef00000000", MintTrust::Trusted)
            .unwrap();

        let proofs = m.mint_tokens("deadbeef00000000", 64).unwrap();
        let p = &proofs[0];

        // secret は 64 hex (= 32 byte)
        assert_eq!(p.secret.len(), 64, "Cashu secret は 32 byte hex");
        assert!(p.secret.chars().all(|c| c.is_ascii_hexdigit()));

        // C は 66 hex (= 33 byte 圧縮 secp256k1 point)
        assert_eq!(p.c.len(), 66, "C は 33 byte 圧縮 point hex");
        assert!(
            p.c.starts_with("02") || p.c.starts_with("03"),
            "C は 02 または 03 prefix"
        );

        // nullifier は 64 hex (= 32 byte BLAKE3)
        assert_eq!(p.nullifier.len(), 64);
        assert!(p.nullifier.chars().all(|c| c.is_ascii_hexdigit()));

        // keyset_id は 16 hex
        assert_eq!(p.keyset_id.len(), 16);
        assert!(p.keyset_id.chars().all(|c| c.is_ascii_hexdigit()));

        // signature は C と同値 (互換性)
        assert_eq!(p.signature, p.c);
    }

    /// nullifier は secret の決定論的 hash (replay 検知の核)
    #[test]
    fn test_nullifier_is_deterministic_blake3_of_secret() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        let proofs = m.mint_tokens("mint1", 1).unwrap();
        let p = &proofs[0];

        // 自分で再計算して一致確認
        let secret_bytes = hex::decode(&p.secret).unwrap();
        let expected = hex::encode(&blake3::hash(&secret_bytes).as_bytes()[..32]);
        assert_eq!(p.nullifier, expected);
    }

    /// 異なる proof は異なる secret/nullifier (乱数性確認)
    #[test]
    fn test_proofs_have_distinct_secrets() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        let proofs = m.mint_tokens("mint1", 7).unwrap(); // 1+2+4

        let mut secrets = std::collections::HashSet::new();
        let mut nulls = std::collections::HashSet::new();
        for p in &proofs {
            assert!(secrets.insert(p.secret.clone()), "secret 重複");
            assert!(nulls.insert(p.nullifier.clone()), "nullifier 重複");
        }
    }

    #[test]
    fn test_double_spend_blocked() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();

        let proofs = m.mint_tokens("mint1", 50).unwrap();
        let ids: Vec<String> = proofs.iter().map(|p| p.id.clone()).collect();
        let spent = m.spend_proofs("mint1", &ids).unwrap();

        // 他人が同じ proof を使おうとする → ブロック
        let mut m2 = EcashManager::default();
        m2.add_mint(test_mint("mint1")).unwrap();
        m2.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m2.receive_proofs(spent.clone()).unwrap();
        // 改めて受け取り試みる → まだ m2 の nullifier 履歴に入ってない
        // このテストはマネージャ間ではなく、同一マネージャで spend 後の再 receive
        m.spent_nullifiers
            .extend(spent.iter().map(|p| p.nullifier.clone()));
        let result = m.receive_proofs(spent);
        assert!(result.is_err());
        assert_eq!(m.stats.double_spend_attempts_blocked, 1);
    }

    #[test]
    fn test_escrow_happy_path() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let e = m
            .open_escrow("job1", "pk-alice", "pk-bob", "mint1", 500, "hash==deadbeef")
            .unwrap();
        assert_eq!(m.wallet.total_sats, 500);
        assert_eq!(e.state, EscrowState::Deposited);

        m.mark_escrow_in_progress(&e.id).unwrap();
        m.release_escrow(&e.id, "attestation-xyz").unwrap();

        assert_eq!(m.stats.total_escrows_released, 1);
        assert!(m.escrows.is_empty()); // archived
    }

    #[test]
    fn test_escrow_refund_returns_funds() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let e = m
            .open_escrow("job1", "pk-a", "pk-b", "mint1", 300, "cond")
            .unwrap();
        assert_eq!(m.wallet.total_sats, 700);

        m.refund_escrow(&e.id).unwrap();
        assert_eq!(m.wallet.total_sats, 1000);
        assert_eq!(m.stats.total_escrows_refunded, 1);
    }

    #[test]
    fn test_deadman_auto_refund() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let e = m
            .open_escrow("job1", "pk-a", "pk-b", "mint1", 200, "cond")
            .unwrap();
        // deadman を過去に設定
        m.escrows
            .iter_mut()
            .find(|x| x.id == e.id)
            .unwrap()
            .deadman_at = Utc::now() - chrono::Duration::minutes(10);

        let processed = m.process_deadman().unwrap();
        assert_eq!(processed, 1);
        assert_eq!(m.stats.deadman_auto_refunds, 1);
        assert_eq!(m.wallet.total_sats, 1000);
    }

    #[test]
    fn test_dispute_resolution_payer_wins() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let e = m
            .open_escrow("job1", "pk-a", "pk-b", "mint1", 500, "cond")
            .unwrap();
        m.dispute_escrow(&e.id, "Bob never produced output")
            .unwrap();
        m.resolve_dispute(&e.id, DisputeResolution::PayerWins)
            .unwrap();

        assert_eq!(m.wallet.total_sats, 1000); // full refund
        assert_eq!(m.stats.total_escrows_disputed, 1);
    }

    #[test]
    fn test_dispute_split() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let e = m
            .open_escrow("job1", "pk-a", "pk-b", "mint1", 400, "cond")
            .unwrap();
        m.dispute_escrow(&e.id, "partial completion").unwrap();
        m.resolve_dispute(&e.id, DisputeResolution::Split).unwrap();

        assert_eq!(m.wallet.total_sats, 600 + 200); // 600 残り + 200 半分返金
    }

    #[test]
    fn test_stream_drains_over_time() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let s = m
            .open_stream("job1", "pk-a", "pk-b", "mint1", 100, 10)
            .unwrap();
        // 手動で過去の last_tick_at を設定して 3 秒経過を模擬
        m.streams
            .iter_mut()
            .find(|x| x.id == s.id)
            .unwrap()
            .last_tick_at = Utc::now() - chrono::Duration::seconds(3);

        let drained = m.tick_stream(&s.id).unwrap();
        assert_eq!(drained, 30); // 10 sat/sec * 3 sec
    }

    #[test]
    fn test_stream_close_refunds_unused() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let s = m
            .open_stream("job1", "pk-a", "pk-b", "mint1", 200, 5)
            .unwrap();
        assert_eq!(m.wallet.total_sats, 800);
        m.streams
            .iter_mut()
            .find(|x| x.id == s.id)
            .unwrap()
            .last_tick_at = Utc::now() - chrono::Duration::seconds(4);

        let refund = m.close_stream(&s.id).unwrap();
        assert_eq!(refund, 200 - 20); // 200 ロック、4秒で20消費、180返金
        assert_eq!(m.wallet.total_sats, 800 + 180);
    }

    #[test]
    fn test_stream_idle_detection() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();

        let s = m
            .open_stream("job1", "pk-a", "pk-b", "mint1", 500, 1)
            .unwrap();
        m.streams
            .iter_mut()
            .find(|x| x.id == s.id)
            .unwrap()
            .last_tick_at = Utc::now() - chrono::Duration::seconds(60);

        let stalled = m.check_idle_streams();
        assert_eq!(stalled, 1);
        assert_eq!(m.streams[0].state, StreamState::Stalled);
    }

    #[test]
    fn test_escrow_amount_cap_enforced() {
        let mut m = EcashManager::default();
        m.config.max_single_escrow_sats = 1000;
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 100_000).unwrap();

        assert!(m.open_escrow("j", "a", "b", "mint1", 5000, "c").is_err());
    }

    #[test]
    fn test_escrow_requires_trusted_mint() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        // trust しない
        let r = m.open_escrow("j", "a", "b", "mint1", 100, "c");
        assert!(r.is_err());
    }

    // ====== Round 22: Escrow state machine guard tests ======

    fn setup_escrow(m: &mut EcashManager) -> String {
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 10_000).unwrap();
        let e = m
            .open_escrow("job1", "pk-a", "pk-b", "mint1", 500, "cond")
            .unwrap();
        e.id
    }

    /// release 後に再度 release → エラー (二重支払い防止)
    #[test]
    fn test_double_release_rejected() {
        let mut m = EcashManager::default();
        let eid = setup_escrow(&mut m);
        m.release_escrow(&eid, "proof").unwrap();
        // 2回目 (アーカイブ済で見つからない)
        assert!(m.release_escrow(&eid, "proof").is_err());
    }

    /// refund 後に release → エラー
    #[test]
    fn test_release_after_refund_rejected() {
        let mut m = EcashManager::default();
        let eid = setup_escrow(&mut m);
        m.refund_escrow(&eid).unwrap();
        assert!(m.release_escrow(&eid, "proof").is_err());
    }

    /// release 後に dispute → エラー (既にアーカイブ済)
    #[test]
    fn test_dispute_after_release_rejected() {
        let mut m = EcashManager::default();
        let eid = setup_escrow(&mut m);
        m.release_escrow(&eid, "proof").unwrap();
        assert!(m.dispute_escrow(&eid, "too late").is_err());
    }

    /// dispute 後に再度 dispute → エラー
    #[test]
    fn test_double_dispute_rejected() {
        let mut m = EcashManager::default();
        let eid = setup_escrow(&mut m);
        m.dispute_escrow(&eid, "reason1").unwrap();
        assert!(
            m.dispute_escrow(&eid, "reason2").is_err(),
            "Disputed 状態で再係争は拒否されるべき"
        );
    }

    /// in_progress 後に再度 in_progress → エラー
    #[test]
    fn test_double_in_progress_rejected() {
        let mut m = EcashManager::default();
        let eid = setup_escrow(&mut m);
        m.mark_escrow_in_progress(&eid).unwrap();
        assert!(m.mark_escrow_in_progress(&eid).is_err());
    }

    /// stream close 後に tick → 0 sats (stale session 安全)
    #[test]
    fn test_tick_after_close_returns_zero() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 10_000).unwrap();
        let s = m.open_stream("job1", "a", "b", "mint1", 500, 10).unwrap();
        m.close_stream(&s.id).unwrap();
        // 閉じた stream は sessions から消えてるので tick はエラー
        assert!(m.tick_stream(&s.id).is_err());
    }

    // ====== Round 26: 境界値テスト ======

    /// 0 額 escrow は拒否
    #[test]
    fn test_zero_amount_escrow_rejected() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();
        assert!(m.open_escrow("j", "a", "b", "mint1", 0, "c").is_err());
    }

    /// 0 額 stream は拒否
    #[test]
    fn test_zero_amount_stream_rejected() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 1000).unwrap();
        assert!(m.open_stream("j", "a", "b", "mint1", 0, 1).is_err());
    }

    /// 残高超過 escrow は拒否
    #[test]
    fn test_insufficient_balance_escrow_rejected() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 10).unwrap();
        assert!(m.open_escrow("j", "a", "b", "mint1", 100, "c").is_err());
    }

    /// mint_tokens(0) は空 proof 返却
    #[test]
    fn test_mint_zero_returns_empty() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        let proofs = m.mint_tokens("mint1", 0).unwrap();
        assert!(proofs.is_empty());
        assert_eq!(m.wallet.total_sats, 0);
    }

    #[test]
    fn test_format_ecash_contains_key_sections() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 500).unwrap();

        let out = format_ecash(&m);
        assert!(out.contains('═'), "ヘッダ罫線");
        assert!(out.contains("500"), "残高表示");
        assert!(out.contains("mint"), "mint 情報");
        assert!(out.contains("エスクロー"), "escrow セクション");
        assert!(out.contains("ストリーム"), "stream セクション");
    }

    /// save → load roundtrip: 永続化が壊れてないことの保証
    #[test]
    fn test_serde_roundtrip() {
        let mut m = EcashManager::default();
        m.add_mint(test_mint("mint1")).unwrap();
        m.trust_mint("mint1", MintTrust::Trusted).unwrap();
        m.mint_tokens("mint1", 255).unwrap();

        let json1 = serde_json::to_string(&m).unwrap();
        let deserialized: EcashManager = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json1, json2, "roundtrip で JSON が変わったらデータ損失");
    }
}
