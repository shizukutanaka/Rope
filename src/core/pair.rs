//! Pair — AirDrop-style zero-config GPU peer discovery
//!
//! ROPE_2028 の 60 秒デモの心臓部。`rope pair` の実装核。
//!
//! 設計原則 (Apple-brief):
//! - ログイン無し・設定無し・IP入力無し
//! - mDNS で同一LAN発見、後で DHT で広域発見
//! - Noise Protocol (XX pattern) で暗号化握手
//! - 発見から利用可能まで < 5秒
//!
//! これは新機能ではない。既存 115 モジュールが持つ能力を
//! 「ユーザーが触れる入口」として露出させるだけ。

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

/// ピアリング状態マネージャ
#[derive(Debug, Serialize, Deserialize)]
pub struct PairManager {
    /// 発見済みピア (LAN + DHT)
    pub discovered: Vec<DiscoveredPeer>,
    /// 握手完了済ピア (暗号接続確立)
    pub paired: Vec<PairedPeer>,
    /// 進行中の握手セッション
    pub handshakes: Vec<HandshakeSession>,
    /// 信頼ストア (以前繋いだピアの公開鍵)
    pub trust_store: HashMap<String, TrustedIdentity>,
    /// 発見モード設定
    pub config: PairConfig,
    /// 統計
    pub stats: PairStats,
    /// 進行中の能力証明チャレンジ
    pub pending_challenges: HashMap<String, CapabilityChallenge>,
    pub updated_at: DateTime<Utc>,
}

impl Default for PairManager {
    fn default() -> Self {
        Self {
            discovered: Vec::new(),
            paired: Vec::new(),
            handshakes: Vec::new(),
            trust_store: HashMap::new(),
            config: PairConfig::default(),
            stats: PairStats::default(),
            pending_challenges: HashMap::new(),
            updated_at: Utc::now(),
        }
    }
}

/// 発見経路 (どう見つけたか)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMethod {
    /// 同一LAN、mDNS `_rope._tcp.local`
    Mdns,
    /// Bluetooth LE 広告 (近接)
    Bluetooth,
    /// Kademlia DHT (広域)
    Dht,
    /// 直接URL指定 (手動)
    Direct,
    /// 連絡先経由 (友人リスト)
    Contact,
}

/// 発見されたピア (まだ握手してない)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    pub id: String,
    /// ピアの自己申告名 (Alice's M3 Ultra 等)
    pub display_name: String,
    /// TXT record から得た公開鍵 (未検証)
    pub advertised_pubkey: String,
    pub endpoint: SocketAddr,
    pub method: DiscoveryMethod,
    pub advertised_capabilities: PeerCapabilities,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub seen_count: u32,
}

/// ピアがTXT/handshakeで宣言する能力 (未検証・参考値)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub gpu_count: u32,
    pub gpu_model: String,
    pub vram_gb: u32,
    pub accepts_jobs: bool,
    pub accepts_payment: bool,
    /// BTC lightning address (オプション)
    pub payment_address: Option<String>,
    pub protocol_version: String,
}

/// 握手完了ピア
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedPeer {
    pub id: String,
    pub display_name: String,
    /// 検証済み静的公開鍵
    pub verified_pubkey: String,
    pub endpoint: SocketAddr,
    /// 握手で得た対称鍵ハッシュ (実鍵はメモリ内)
    pub session_key_hash: String,
    pub capabilities: PeerCapabilities,
    pub paired_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub jobs_run: u64,
    pub bytes_transferred: u64,
    pub trust_level: TrustLevel,
}

/// 信頼度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// 初回、TOFU (Trust On First Use)
    Unknown,
    /// 以前成功ジョブあり
    Familiar,
    /// ユーザーが明示的に信頼
    Trusted,
    /// 自分のデバイス (Apple ID 相当)
    OwnDevice,
}

/// 信頼ストアエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedIdentity {
    pub pubkey: String,
    pub display_name: String,
    pub first_paired: DateTime<Utc>,
    pub last_paired: DateTime<Utc>,
    pub pair_count: u32,
    pub trust_level: TrustLevel,
    /// ユーザーメモ (この人信頼する理由)
    pub note: Option<String>,
    /// 能力証明状態
    pub capability_proven: bool,
    /// 最後に能力証明を試みた時刻
    pub challenged_at: Option<DateTime<Utc>>,
    /// 成功したジョブ数 / 試みたジョブ数 (評判スコア用)
    pub successful_jobs: u32,
    pub failed_jobs: u32,
}

/// Noise XX 握手セッション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeSession {
    pub id: String,
    pub peer_id: String,
    pub remote_endpoint: SocketAddr,
    pub pattern: NoisePattern,
    pub state: HandshakeState,
    pub started_at: DateTime<Utc>,
    pub messages_exchanged: u32,
    pub error: Option<String>,
}

/// Noise プロトコルパターン
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoisePattern {
    /// XX: 相互認証、両側の静的鍵交換、新規ペア用
    Xx,
    /// IK: イニシエータが相手の静的鍵を事前に知ってる、再接続用
    Ik,
    /// NN: 匿名、両側の静的鍵なし、テスト用のみ
    Nn,
}

/// 握手状態機械
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandshakeState {
    /// 初期化、-> e 送信前
    Initializing,
    /// -> e 送信済、<- e, ee, s, es 待ち
    EphemeralSent,
    /// <- e, ee, s, es 受信済、-> s, se 送信中
    ResponderIdentified,
    /// 握手完了、トランスポート鍵確立
    Established,
    /// 失敗
    Failed,
    /// タイムアウト
    TimedOut,
}

/// 設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairConfig {
    /// mDNS 有効
    pub mdns_enabled: bool,
    /// mDNS サービス名
    pub mdns_service: String,
    /// Bluetooth LE 発見有効
    pub bluetooth_enabled: bool,
    /// DHT bootstrap ノード
    pub dht_bootstrap_nodes: Vec<String>,
    /// 握手タイムアウト秒
    pub handshake_timeout_seconds: u32,
    /// TOFU 許可 (初回握手を自動信頼)
    pub allow_tofu: bool,
    /// 表示名 (他ピアに見える)
    pub local_display_name: String,
    /// 受付モード (公開/友人のみ/オフ)
    pub accept_mode: AcceptMode,
    /// 自動切断: 無活動秒数
    pub idle_disconnect_seconds: u32,
    /// 発見時の自動握手
    pub auto_handshake_on_discovery: bool,
}

impl Default for PairConfig {
    fn default() -> Self {
        Self {
            mdns_enabled: true,
            mdns_service: "_rope._tcp.local.".to_string(),
            bluetooth_enabled: false,
            dht_bootstrap_nodes: Vec::new(),
            handshake_timeout_seconds: 10,
            allow_tofu: true,
            local_display_name: hostname_fallback(),
            accept_mode: AcceptMode::LanOnly,
            idle_disconnect_seconds: 300,
            auto_handshake_on_discovery: false,
        }
    }
}

fn hostname_fallback() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "rope-peer".to_string())
}

/// 受付モード
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptMode {
    /// 誰からでも (公開ノード)
    Open,
    /// 同一LANのみ
    LanOnly,
    /// 信頼ストアにあるピアのみ
    ContactsOnly,
    /// 受付停止
    Off,
}

impl std::fmt::Display for AcceptMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceptMode::Open => write!(f, "公開 (誰でも)"),
            AcceptMode::LanOnly => write!(f, "LAN のみ"),
            AcceptMode::ContactsOnly => write!(f, "連絡先のみ"),
            AcceptMode::Off => write!(f, "受付停止"),
        }
    }
}

/// 統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairStats {
    pub total_discovered: u64,
    pub currently_discovered: u32,
    pub total_handshakes_attempted: u64,
    pub total_handshakes_succeeded: u64,
    pub total_handshakes_failed: u64,
    pub currently_paired: u32,
    pub total_peers_ever_paired: u64,
    pub avg_handshake_ms: f64,
    pub tofu_accepts: u64,
    pub pubkey_mismatch_rejections: u64,
    pub qr_tokens_issued: u64,
    pub qr_tokens_consumed: u64,
    pub qr_tokens_expired: u64,
    pub qr_hmac_rejections: u64,
}

// ============================================================================
// QR ペアリング (AirDrop 式フォールバック)
// ============================================================================
//
// mDNS が 40% のネットで死ぬので、画面表示 QR → スキャンで確実発見。
// Apple AirDrop の真の秘伝: 同 Apple ID 無しでもここに落ちて成立する。

/// QR コードに埋め込むペアリング情報 (base32 エンコードされる)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingToken {
    /// 実ペイロード
    pub payload: PairingTokenPayload,
    /// HMAC-SHA256 (payload + 共有 secret でリプレイ防止)
    pub hmac: String,
    /// base32 文字列 (QR スキャン互換)
    pub encoded: String,
}

/// トークンペイロード (HMAC で保護)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingTokenPayload {
    /// 一意識別子
    pub nonce: String,
    /// 発行者公開鍵
    pub issuer_pubkey: String,
    /// 発行者表示名
    pub issuer_display_name: String,
    /// 受信可能エンドポイント
    pub endpoint: SocketAddr,
    /// 宣伝能力
    pub capabilities: PeerCapabilities,
    /// 発行時刻
    pub issued_at: DateTime<Utc>,
    /// 有効期限 (デフォルト 60 秒)
    pub expires_at: DateTime<Utc>,
}

impl PairingToken {
    /// QR 文字列から復元 (スキャナ側が呼ぶ)
    pub fn decode(encoded: &str) -> Result<Self> {
        let bytes = base32_decode(encoded).context("base32 デコード失敗")?;
        let token: PairingToken = serde_json::from_slice(&bytes).context("ペイロードパース失敗")?;
        Ok(token)
    }

    /// QR 文字列へ (発行者側が画面表示)
    pub fn to_qr_string(&self) -> &str {
        &self.encoded
    }

    /// HMAC 検証 (共有 secret が必要)
    pub fn verify_hmac(&self, shared_secret: &[u8]) -> bool {
        let expected = compute_hmac(&self.payload, shared_secret);
        constant_time_eq(expected.as_bytes(), self.hmac.as_bytes())
    }

    /// 期限切れか
    pub fn is_expired(&self) -> bool {
        self.payload.expires_at < Utc::now()
    }
}

/// 能力証明チャレンジ（FORTYTWO 式の Proof-of-Capability）
///
/// 初回ピアに対して、実際に計算タスクをこなせるか確認する仕組み。
/// リプレイ攻撃を防ぐため nonce + タイムスタンプで一意性を確保。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityChallenge {
    /// チャレンジ一意識別子
    pub challenge_id: String,
    /// 対象ピア ID
    pub peer_id: String,
    /// チャレンジ内容 (例: "run llama-3.2-1b with seed=42")
    pub task_spec: String,
    /// 期待される出力ハッシュ (blake3)
    pub expected_output_hash: String,
    /// チャレンジ発行時刻
    pub issued_at: DateTime<Utc>,
    /// タイムアウト（秒）
    pub timeout_seconds: u32,
    /// 検証状態
    pub state: ChallengeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChallengeState {
    /// チャレンジ発行済、応答待ち
    Pending,
    /// 正答者が応答してきた
    Verified,
    /// タイムアウト
    TimedOut,
    /// 不正答
    Failed,
}

impl CapabilityChallenge {
    /// チャレンジの有効期限内か
    pub fn is_valid(&self) -> bool {
        let elapsed = (Utc::now() - self.issued_at).num_seconds() as u32;
        elapsed < self.timeout_seconds
    }

    /// チャレンジが応答を待ってるか
    pub fn is_pending(&self) -> bool {
        self.state == ChallengeState::Pending && self.is_valid()
    }
}

// --- base32 (Crockford, QR 互換、混同文字無し) ---
const B32_ALPHA: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    let mut buf: u64 = 0;
    let mut bits: u8 = 0;
    for &b in data {
        buf = (buf << 8) | (b as u64);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buf >> bits) & 0x1f) as usize;
            out.push(B32_ALPHA[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buf << (5 - bits)) & 0x1f) as usize;
        out.push(B32_ALPHA[idx] as char);
    }
    out
}

fn base32_decode(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let mut buf: u64 = 0;
    let mut bits: u8 = 0;
    for c in s.chars() {
        let upper = c.to_ascii_uppercase();
        let idx = B32_ALPHA
            .iter()
            .position(|&a| a == upper as u8)
            .context("base32 範囲外文字")?;
        buf = (buf << 5) | (idx as u64);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

// --- blake3 keyed-hash MAC ---
//
// 以前は FNV-1a を「HMAC」として使っていたが、FNV は非暗号学的 (衝突耐性なし)
// で、攻撃者が payload を改竄しつつ同じタグを作れるため QR ペアリングの偽造を
// 防げなかった。blake3::keyed_hash は正式な keyed MAC (PRF) であり、依存追加なし
// (blake3 は既存の直接依存) で本物の完全性保証を得られる。
//
// keyed_hash は厳密に 32-byte 鍵を要求するので、任意長の共有 secret を
// blake3::hash で 32-byte に正規化してから鍵に用いる。
fn compute_hmac(payload: &PairingTokenPayload, secret: &[u8]) -> String {
    let json = serde_json::to_vec(payload).unwrap_or_default();
    let key = blake3::hash(secret);
    let tag = blake3::keyed_hash(key.as_bytes(), &json);
    hex::encode(tag.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl PairManager {
    /// mDNS/Bluetooth/DHT で発見したピアを記録
    pub fn record_discovery(
        &mut self,
        display_name: &str,
        pubkey: &str,
        endpoint: SocketAddr,
        method: DiscoveryMethod,
        capabilities: PeerCapabilities,
    ) -> Result<&DiscoveredPeer> {
        if pubkey.is_empty() {
            anyhow::bail!("公開鍵が空 — 不正な発見応答");
        }
        if display_name.is_empty() {
            anyhow::bail!("表示名が空 — 不正な発見応答");
        }
        // 既存ピアのインデックスを先に確定 (借用を即座に解放)
        let existing_idx = self
            .discovered
            .iter()
            .position(|p| p.advertised_pubkey == pubkey);
        if let Some(idx) = existing_idx {
            let existing = &mut self.discovered[idx];
            existing.last_seen = Utc::now();
            existing.seen_count += 1;
            existing.endpoint = endpoint;
            self.updated_at = Utc::now();
            return Ok(&self.discovered[idx]);
        }

        let peer = DiscoveredPeer {
            id: uuid::Uuid::now_v7().to_string(),
            display_name: display_name.to_string(),
            advertised_pubkey: pubkey.to_string(),
            endpoint,
            method,
            advertised_capabilities: capabilities,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            seen_count: 1,
        };

        self.discovered.push(peer);
        self.stats.total_discovered += 1;
        self.stats.currently_discovered = self.discovered.len() as u32;
        self.updated_at = Utc::now();
        Ok(self.discovered.last().unwrap())
    }

    // ========================================================================
    // QR ペアリング (AirDrop 式フォールバック)
    // ========================================================================

    /// 自分のペアリング QR を発行 (Alice が画面表示)
    ///
    /// `shared_secret` は共有秘密 (例: "rope-v1" のような固定 magic 値、
    /// または前回ペア済ならペア鍵)。同プロトコル版の相手のみ復号できる。
    /// `ttl_seconds` のデフォルト推奨値は 60 (AirDrop と同じ)。
    pub fn create_pairing_token(
        &mut self,
        my_pubkey: &str,
        my_endpoint: SocketAddr,
        my_capabilities: PeerCapabilities,
        shared_secret: &[u8],
        ttl_seconds: i64,
    ) -> Result<PairingToken> {
        let nonce = uuid::Uuid::now_v7().to_string();
        let now = Utc::now();
        let payload = PairingTokenPayload {
            nonce,
            issuer_pubkey: my_pubkey.to_string(),
            issuer_display_name: self.config.local_display_name.clone(),
            endpoint: my_endpoint,
            capabilities: my_capabilities,
            issued_at: now,
            expires_at: now + chrono::Duration::seconds(ttl_seconds),
        };

        let hmac = compute_hmac(&payload, shared_secret);

        let draft = PairingToken {
            payload: payload.clone(),
            hmac: hmac.clone(),
            encoded: String::new(),
        };
        let bytes = serde_json::to_vec(&draft)?;
        let encoded = base32_encode(&bytes);

        let token = PairingToken {
            payload,
            hmac,
            encoded,
        };
        self.stats.qr_tokens_issued += 1;
        self.updated_at = Utc::now();
        Ok(token)
    }

    /// 相手の QR を読み取って発見登録 (Bob が Alice の QR をスキャン)
    ///
    /// 失敗モード:
    /// - 期限切れ → Expired エラー + stats 加算
    /// - HMAC 不一致 → HmacMismatch エラー + stats 加算
    /// - nonce リプレイ → 既発見と重複、無視で通す
    pub fn accept_pairing_token(
        &mut self,
        qr_string: &str,
        shared_secret: &[u8],
    ) -> Result<&DiscoveredPeer> {
        let token = PairingToken::decode(qr_string)?;

        if token.is_expired() {
            self.stats.qr_tokens_expired += 1;
            anyhow::bail!("QR 期限切れ");
        }

        if !token.verify_hmac(shared_secret) {
            self.stats.qr_hmac_rejections += 1;
            anyhow::bail!("HMAC 不一致 — プロトコル不整合か改ざん");
        }

        let p = token.payload;
        let peer_id = self
            .record_discovery(
                &p.issuer_display_name,
                &p.issuer_pubkey,
                p.endpoint,
                DiscoveryMethod::Direct, // QR = Direct 経路
                p.capabilities,
            )?
            .id
            .clone();

        self.stats.qr_tokens_consumed += 1;
        // record_discovery で追加済のピアを再取得 (借用衝突回避)
        Ok(self
            .discovered
            .iter()
            .find(|p| p.id == peer_id)
            .expect("record_discovery が追加したピアが見つからない"))
    }

    /// 発見されたピアとの握手を開始
    pub fn begin_handshake(&mut self, peer_id: &str) -> Result<HandshakeSession> {
        let peer = self
            .discovered
            .iter()
            .find(|p| p.id == peer_id)
            .context("発見リストにピアなし")?;

        // 受付モード確認
        match self.config.accept_mode {
            AcceptMode::Off => anyhow::bail!("受付停止中"),
            AcceptMode::LanOnly
                if peer.method != DiscoveryMethod::Mdns
                    && peer.method != DiscoveryMethod::Bluetooth
                    && peer.method != DiscoveryMethod::Direct =>
            // QR = 明示ユーザー操作
            {
                anyhow::bail!("LAN外のピア拒否")
            }
            AcceptMode::ContactsOnly if !self.trust_store.contains_key(&peer.advertised_pubkey) => {
                anyhow::bail!("信頼ストア外のピア拒否")
            }
            _ => {}
        }

        // Noise パターン決定: 既知なら IK、新規なら XX
        let pattern = if self.trust_store.contains_key(&peer.advertised_pubkey) {
            NoisePattern::Ik
        } else {
            NoisePattern::Xx
        };

        let session = HandshakeSession {
            id: uuid::Uuid::now_v7().to_string(),
            peer_id: peer_id.to_string(),
            remote_endpoint: peer.endpoint,
            pattern,
            state: HandshakeState::Initializing,
            started_at: Utc::now(),
            messages_exchanged: 0,
            error: None,
        };

        self.handshakes.push(session.clone());
        self.stats.total_handshakes_attempted += 1;
        self.updated_at = Utc::now();
        Ok(session)
    }

    /// 握手メッセージ受信を記録、状態遷移
    pub fn advance_handshake(&mut self, session_id: &str) -> Result<HandshakeState> {
        let session = self
            .handshakes
            .iter_mut()
            .find(|s| s.id == session_id)
            .context("握手セッションなし")?;

        // ガード: 終端状態 or 完了後の再呼出を拒否
        match session.state {
            HandshakeState::Established => {
                anyhow::bail!("握手既に完了 — complete_handshake を呼ぶべき");
            }
            HandshakeState::Failed => {
                anyhow::bail!("握手失敗済 — 新規セッションを開始してください");
            }
            HandshakeState::TimedOut => {
                anyhow::bail!("握手タイムアウト済 — 新規セッションを開始してください");
            }
            _ => {}
        }

        session.messages_exchanged += 1;
        session.state = match session.state {
            HandshakeState::Initializing => HandshakeState::EphemeralSent,
            HandshakeState::EphemeralSent => HandshakeState::ResponderIdentified,
            HandshakeState::ResponderIdentified => HandshakeState::Established,
            other => other, // unreachable due to guard above
        };

        let new_state = session.state;
        self.updated_at = Utc::now();
        Ok(new_state)
    }

    /// 握手完了、ペア確立
    pub fn complete_handshake(
        &mut self,
        session_id: &str,
        verified_pubkey: &str,
        session_key_hash: &str,
    ) -> Result<PairedPeer> {
        let session_idx = self
            .handshakes
            .iter()
            .position(|s| s.id == session_id)
            .context("握手セッションなし")?;
        let session = self.handshakes[session_idx].clone();

        if session.state != HandshakeState::Established {
            anyhow::bail!("握手未完了");
        }

        let discovered = self
            .discovered
            .iter()
            .find(|p| p.id == session.peer_id)
            .context("発見ピア消失")?
            .clone();

        // 公開鍵一致確認 (XX パターンで advertised が初めて検証される)
        if discovered.advertised_pubkey != verified_pubkey
            && !discovered.advertised_pubkey.is_empty()
        {
            self.stats.pubkey_mismatch_rejections += 1;
            anyhow::bail!("公開鍵不一致 — MITM 疑い");
        }

        // 信頼ストア確認/追加
        let trust_level = if let Some(entry) = self.trust_store.get_mut(verified_pubkey) {
            entry.last_paired = Utc::now();
            entry.pair_count += 1;
            entry.trust_level
        } else if self.config.allow_tofu {
            let ident = TrustedIdentity {
                pubkey: verified_pubkey.to_string(),
                display_name: discovered.display_name.clone(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            };
            self.trust_store.insert(verified_pubkey.to_string(), ident);
            self.stats.tofu_accepts += 1;
            TrustLevel::Unknown
        } else {
            anyhow::bail!("TOFU 無効、未知ピア拒否");
        };

        let paired = PairedPeer {
            id: uuid::Uuid::now_v7().to_string(),
            display_name: discovered.display_name.clone(),
            verified_pubkey: verified_pubkey.to_string(),
            endpoint: discovered.endpoint,
            session_key_hash: session_key_hash.to_string(),
            capabilities: discovered.advertised_capabilities,
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level,
        };

        self.paired.push(paired.clone());
        self.handshakes.remove(session_idx);
        self.stats.total_handshakes_succeeded += 1;
        self.stats.currently_paired = self.paired.len() as u32;
        self.stats.total_peers_ever_paired += 1;

        let elapsed_ms = (Utc::now() - session.started_at).num_milliseconds() as f64;
        let n = self.stats.total_handshakes_succeeded as f64;
        self.stats.avg_handshake_ms = (self.stats.avg_handshake_ms * (n - 1.0) + elapsed_ms) / n;

        self.updated_at = Utc::now();
        Ok(paired)
    }

    /// 握手失敗を記録
    pub fn fail_handshake(&mut self, session_id: &str, reason: &str) -> Result<()> {
        let session = self
            .handshakes
            .iter_mut()
            .find(|s| s.id == session_id)
            .context("握手セッションなし")?;
        session.state = HandshakeState::Failed;
        session.error = Some(reason.to_string());
        self.stats.total_handshakes_failed += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// ピア切断 (相手側離脱 or idle timeout)
    pub fn unpair(&mut self, paired_id: &str) -> Result<()> {
        let before = self.paired.len();
        self.paired.retain(|p| p.id != paired_id);
        if self.paired.len() == before {
            anyhow::bail!("ペア無し");
        }
        self.stats.currently_paired = self.paired.len() as u32;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// ユーザーが明示的に信頼 (Trusted にアップグレード)
    pub fn mark_trusted(&mut self, pubkey: &str, note: Option<String>) -> Result<()> {
        let entry = self
            .trust_store
            .get_mut(pubkey)
            .context("信頼ストアに未登録")?;
        entry.trust_level = TrustLevel::Trusted;
        if note.is_some() {
            entry.note = note;
        }

        for p in self.paired.iter_mut() {
            if p.verified_pubkey == pubkey {
                p.trust_level = TrustLevel::Trusted;
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    // ========================================================================
    // 能力証明 (Proof-of-Capability, FORTYTWO 式 Sybil 耐性)
    // ========================================================================

    /// 初回ピアに能力証明チャレンジを発行 (Sybil 攻撃防止)
    pub fn issue_capability_challenge(
        &mut self,
        peer_id: &str,
        task_spec: &str,
        expected_output_hash: &str,
        timeout_seconds: u32,
    ) -> Result<CapabilityChallenge> {
        let challenge_id = uuid::Uuid::now_v7().to_string();
        let challenge = CapabilityChallenge {
            challenge_id: challenge_id.clone(),
            peer_id: peer_id.to_string(),
            task_spec: task_spec.to_string(),
            expected_output_hash: expected_output_hash.to_string(),
            issued_at: Utc::now(),
            timeout_seconds,
            state: ChallengeState::Pending,
        };

        self.pending_challenges
            .insert(challenge_id, challenge.clone());
        self.updated_at = Utc::now();
        Ok(challenge)
    }

    /// チャレンジ応答を検証 (ピアが正しい出力をハッシュで提示)
    pub fn verify_capability_proof(
        &mut self,
        challenge_id: &str,
        proof_output_hash: &str,
    ) -> Result<bool> {
        let challenge = self
            .pending_challenges
            .get_mut(challenge_id)
            .context("チャレンジ無し")?;

        if challenge.state != ChallengeState::Pending {
            anyhow::bail!("チャレンジは既に完了");
        }

        if !challenge.is_valid() {
            challenge.state = ChallengeState::TimedOut;
            self.updated_at = Utc::now();
            anyhow::bail!("チャレンジタイムアウト");
        }

        let verified = challenge.expected_output_hash == proof_output_hash;
        challenge.state = if verified {
            ChallengeState::Verified
        } else {
            ChallengeState::Failed
        };

        if verified {
            if let Some(identity) = self.trust_store.get_mut(&challenge.peer_id) {
                identity.capability_proven = true;
                identity.challenged_at = Some(Utc::now());
                identity.successful_jobs = identity.successful_jobs.saturating_add(1);
            }
        } else {
            if let Some(identity) = self.trust_store.get_mut(&challenge.peer_id) {
                identity.failed_jobs = identity.failed_jobs.saturating_add(1);
            }
        }

        self.updated_at = Utc::now();
        Ok(verified)
    }

    /// ピアの評判スコア (成功/全試行)
    pub fn reputation_score(&self, pubkey: &str) -> Option<f64> {
        self.trust_store.get(pubkey).map(|identity| {
            let total = (identity.successful_jobs + identity.failed_jobs) as f64;
            if total == 0.0 {
                0.0
            } else {
                identity.successful_jobs as f64 / total
            }
        })
    }

    /// ピアが能力証明済みか、または信頼されているか
    pub fn is_capability_proven_or_trusted(&self, pubkey: &str) -> bool {
        self.trust_store
            .get(pubkey)
            .map(|identity| {
                identity.capability_proven
                    || matches!(
                        identity.trust_level,
                        TrustLevel::Familiar | TrustLevel::Trusted | TrustLevel::OwnDevice
                    )
            })
            .unwrap_or(false)
    }

    /// 古い発見エントリを掃除 (デフォルト: 60秒以上見てない)
    pub fn prune_stale_discoveries(&mut self, stale_seconds: i64) {
        let cutoff = Utc::now() - chrono::Duration::seconds(stale_seconds);
        self.discovered.retain(|p| p.last_seen > cutoff);
        self.stats.currently_discovered = self.discovered.len() as u32;
    }

    /// 能力でピア推薦 (rope run が使う)
    pub fn recommend_for_job(&self, min_vram_gb: u32, needs_payment: bool) -> Vec<&PairedPeer> {
        let mut candidates: Vec<&PairedPeer> = self
            .paired
            .iter()
            .filter(|p| p.capabilities.vram_gb >= min_vram_gb)
            .filter(|p| p.capabilities.accepts_jobs)
            .filter(|p| !needs_payment || p.capabilities.accepts_payment)
            .collect();

        // 信頼度 → 最新活動で並び替え
        candidates.sort_by(|a, b| {
            b.trust_level
                .cmp(&a.trust_level)
                .then(b.last_active.cmp(&a.last_active))
        });
        candidates
    }
}

// Storage
/// pair マネージャーの永続化パスを返す
pub fn pair_path() -> std::path::PathBuf {
    crate::core::config::config_dir().join("pair.json")
}

/// pair マネージャーを永続化ファイルから読込
pub fn load_pair() -> Result<PairManager> {
    Ok(crate::core::config::load_or_recover(
        &pair_path(),
        "ピア情報 (pair.json)",
    ))
}

/// pair マネージャーを永続化ファイルに保存
pub fn save_pair(m: &PairManager) -> Result<()> {
    crate::core::config::atomic_write(&pair_path(), &serde_json::to_string_pretty(m)?)
}

/// pair ダッシュボード文字列を生成
pub fn format_pair(m: &PairManager) -> String {
    let mut out = String::new();
    out.push_str("ピア接続:\n");
    out.push_str("═══════════════════════════════════════════════════\n\n");
    out.push_str(&format!(
        "📡 発見中: {} / 握手中: {} / ペア済: {}\n",
        m.stats.currently_discovered,
        m.handshakes.len(),
        m.stats.currently_paired
    ));
    out.push_str(&format!(
        "  累計発見: {} / 成功握手: {} / 失敗: {}\n",
        m.stats.total_discovered,
        m.stats.total_handshakes_succeeded,
        m.stats.total_handshakes_failed
    ));
    out.push_str(&format!(
        "  平均握手時間: {:.1} ms\n",
        m.stats.avg_handshake_ms
    ));
    out.push_str(&format!(
        "  信頼ストア: {} identities\n",
        m.trust_store.len()
    ));
    out.push_str(&format!("  受付モード: {}\n", m.config.accept_mode));
    out
}

#[cfg(test)]
mod tests {
    // session.id.clone() は &m 借用を解放して後続の &mut m 呼出を可能にするため必須。
    // clippy は session 未使用と見て redundant とするが borrow checker 上は必要。
    #![allow(clippy::redundant_clone)]
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_endpoint(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), port)
    }

    fn test_caps() -> PeerCapabilities {
        PeerCapabilities {
            gpu_count: 1,
            gpu_model: "RTX 4090".to_string(),
            vram_gb: 24,
            accepts_jobs: true,
            accepts_payment: true,
            payment_address: Some("bc1q...".to_string()),
            protocol_version: "rope/1.0".to_string(),
        }
    }

    #[test]
    fn test_discovery_dedup() {
        let mut m = PairManager::default();
        m.record_discovery(
            "Bob's PC",
            "pk-bob",
            test_endpoint(9001),
            DiscoveryMethod::Mdns,
            test_caps(),
        )
        .unwrap();
        m.record_discovery(
            "Bob's PC",
            "pk-bob",
            test_endpoint(9001),
            DiscoveryMethod::Mdns,
            test_caps(),
        )
        .unwrap();

        assert_eq!(m.discovered.len(), 1);
        assert_eq!(m.discovered[0].seen_count, 2);
    }

    #[test]
    fn test_handshake_flow_xx_tofu() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "Bob",
                "pk-bob",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();

        let session = m.begin_handshake(&peer.id).unwrap();
        assert_eq!(session.pattern, NoisePattern::Xx);
        assert_eq!(session.state, HandshakeState::Initializing);

        let sid = session.id.clone();
        m.advance_handshake(&sid).unwrap();
        m.advance_handshake(&sid).unwrap();
        let state = m.advance_handshake(&sid).unwrap();
        assert_eq!(state, HandshakeState::Established);

        let paired = m.complete_handshake(&sid, "pk-bob", "key-hash").unwrap();
        assert_eq!(paired.trust_level, TrustLevel::Unknown); // TOFU
        assert_eq!(m.stats.tofu_accepts, 1);
        assert!(m.trust_store.contains_key("pk-bob"));
    }

    #[test]
    fn test_handshake_returning_peer_uses_ik() {
        let mut m = PairManager::default();
        m.trust_store.insert(
            "pk-bob".to_string(),
            TrustedIdentity {
                pubkey: "pk-bob".to_string(),
                display_name: "Bob".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 5,
                trust_level: TrustLevel::Familiar,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        let peer = m
            .record_discovery(
                "Bob",
                "pk-bob",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();

        let session = m.begin_handshake(&peer.id).unwrap();
        assert_eq!(session.pattern, NoisePattern::Ik);
    }

    #[test]
    fn test_pubkey_mismatch_rejected() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "Bob",
                "pk-advertised",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();

        let session = m.begin_handshake(&peer.id).unwrap();
        let sid = session.id.clone();
        for _ in 0..3 {
            m.advance_handshake(&sid).unwrap();
        }

        // 握手で得た鍵が広告と違う → MITM疑い
        let result = m.complete_handshake(&sid, "pk-different", "hash");
        assert!(result.is_err());
        assert_eq!(m.stats.pubkey_mismatch_rejections, 1);
    }

    #[test]
    fn test_accept_mode_lan_only_blocks_dht() {
        let mut m = PairManager::default();
        m.config.accept_mode = AcceptMode::LanOnly;

        let peer = m
            .record_discovery(
                "remote",
                "pk-x",
                test_endpoint(9001),
                DiscoveryMethod::Dht,
                test_caps(),
            )
            .unwrap()
            .clone();

        assert!(m.begin_handshake(&peer.id).is_err());
    }

    #[test]
    fn test_accept_mode_off_blocks_all() {
        let mut m = PairManager::default();
        m.config.accept_mode = AcceptMode::Off;
        let peer = m
            .record_discovery(
                "x",
                "pk-x",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();
        assert!(m.begin_handshake(&peer.id).is_err());
    }

    #[test]
    fn test_recommend_for_job_filters_and_ranks() {
        let mut m = PairManager::default();

        let mut low_vram = test_caps();
        low_vram.vram_gb = 8;
        let mut big = test_caps();
        big.vram_gb = 80;

        // Low VRAM peer (should filter out for 24GB job)
        m.trust_store.insert(
            "pk-low".to_string(),
            TrustedIdentity {
                pubkey: "pk-low".to_string(),
                display_name: "low".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        m.paired.push(PairedPeer {
            id: "p1".to_string(),
            display_name: "low".to_string(),
            verified_pubkey: "pk-low".to_string(),
            endpoint: test_endpoint(1),
            session_key_hash: "h".to_string(),
            capabilities: low_vram,
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level: TrustLevel::Unknown,
        });

        // High VRAM, trusted
        m.paired.push(PairedPeer {
            id: "p2".to_string(),
            display_name: "big".to_string(),
            verified_pubkey: "pk-big".to_string(),
            endpoint: test_endpoint(2),
            session_key_hash: "h".to_string(),
            capabilities: big,
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 10,
            bytes_transferred: 0,
            trust_level: TrustLevel::Trusted,
        });

        let recs = m.recommend_for_job(24, true);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "p2");
    }

    #[test]
    fn test_prune_stale() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "x",
                "pk",
                test_endpoint(1),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap();
        let id = peer.id.clone();
        // Force last_seen into past
        if let Some(p) = m.discovered.iter_mut().find(|p| p.id == id) {
            p.last_seen = Utc::now() - chrono::Duration::seconds(120);
        }
        m.prune_stale_discoveries(60);
        assert_eq!(m.discovered.len(), 0);
    }

    #[test]
    fn test_mark_trusted_upgrades_paired() {
        let mut m = PairManager::default();
        m.trust_store.insert(
            "pk".to_string(),
            TrustedIdentity {
                pubkey: "pk".to_string(),
                display_name: "x".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        m.paired.push(PairedPeer {
            id: "p".to_string(),
            display_name: "x".to_string(),
            verified_pubkey: "pk".to_string(),
            endpoint: test_endpoint(1),
            session_key_hash: "h".to_string(),
            capabilities: test_caps(),
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level: TrustLevel::Unknown,
        });

        m.mark_trusted("pk", Some("met at conference".to_string()))
            .unwrap();
        assert_eq!(m.paired[0].trust_level, TrustLevel::Trusted);
        assert_eq!(
            m.trust_store["pk"].note,
            Some("met at conference".to_string())
        );
    }

    #[test]
    fn test_unpair() {
        let mut m = PairManager::default();
        m.paired.push(PairedPeer {
            id: "p".to_string(),
            display_name: "x".to_string(),
            verified_pubkey: "pk".to_string(),
            endpoint: test_endpoint(1),
            session_key_hash: "h".to_string(),
            capabilities: test_caps(),
            paired_at: Utc::now(),
            last_active: Utc::now(),
            jobs_run: 0,
            bytes_transferred: 0,
            trust_level: TrustLevel::Unknown,
        });
        m.stats.currently_paired = 1;
        m.unpair("p").unwrap();
        assert_eq!(m.paired.len(), 0);
        assert_eq!(m.stats.currently_paired, 0);
    }

    // ====== QR ペアリング ======

    #[test]
    fn test_qr_roundtrip() {
        let mut alice = PairManager::default();
        let mut bob = PairManager::default();
        let secret = b"rope-v1-magic";

        let token = alice
            .create_pairing_token("pk-alice", test_endpoint(9001), test_caps(), secret, 60)
            .unwrap();

        let qr = token.to_qr_string().to_string();
        assert!(!qr.is_empty());

        let peer = bob.accept_pairing_token(&qr, secret).unwrap();
        assert_eq!(peer.advertised_pubkey, "pk-alice");
        assert_eq!(peer.method, DiscoveryMethod::Direct);
        assert_eq!(alice.stats.qr_tokens_issued, 1);
        assert_eq!(bob.stats.qr_tokens_consumed, 1);
    }

    #[test]
    fn test_qr_expired_rejected() {
        let mut alice = PairManager::default();
        let mut bob = PairManager::default();
        let secret = b"rope-v1";

        let token = alice
            .create_pairing_token(
                "pk",
                test_endpoint(1),
                test_caps(),
                secret,
                -10, // 既に期限切れ
            )
            .unwrap();

        let result = bob.accept_pairing_token(token.to_qr_string(), secret);
        assert!(result.is_err());
        assert_eq!(bob.stats.qr_tokens_expired, 1);
    }

    #[test]
    fn test_qr_wrong_secret_rejected() {
        let mut alice = PairManager::default();
        let mut bob = PairManager::default();

        let token = alice
            .create_pairing_token("pk", test_endpoint(1), test_caps(), b"alice-secret", 60)
            .unwrap();

        // Bob uses different secret → HMAC mismatch
        let result = bob.accept_pairing_token(token.to_qr_string(), b"bob-secret");
        assert!(result.is_err());
        assert_eq!(bob.stats.qr_hmac_rejections, 1);
    }

    #[test]
    fn test_qr_tampered_rejected() {
        let mut alice = PairManager::default();
        let mut bob = PairManager::default();
        let secret = b"s";

        let token = alice
            .create_pairing_token("pk", test_endpoint(1), test_caps(), secret, 60)
            .unwrap();

        // Corrupt the QR string by flipping a char
        let mut corrupted = token.to_qr_string().to_string();
        let mid = corrupted.len() / 2;
        corrupted.replace_range(mid..mid + 1, "Z");

        // Either base32 decode fails or HMAC fails — both are errors
        let result = bob.accept_pairing_token(&corrupted, secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_qr_bypasses_lan_only_mode() {
        let mut alice = PairManager::default();
        let mut bob = PairManager::default();
        bob.config.accept_mode = AcceptMode::LanOnly;
        let secret = b"s";

        // Alice is on remote network, would be blocked by mDNS/LAN-only
        let token = alice
            .create_pairing_token("pk-alice", test_endpoint(443), test_caps(), secret, 60)
            .unwrap();

        // QR scan is explicit user action → should go through
        let peer = bob
            .accept_pairing_token(token.to_qr_string(), secret)
            .unwrap();
        let pid = peer.id.clone();

        // Begin handshake should also work (Direct bypasses LanOnly)
        assert!(bob.begin_handshake(&pid).is_ok());
    }

    #[test]
    fn test_base32_roundtrip() {
        let inputs: &[&[u8]] = &[
            b"",
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            b"abcde",
            &[0, 255, 128, 1, 2, 3],
        ];
        for data in inputs {
            let encoded = base32_encode(data);
            let decoded = base32_decode(&encoded).unwrap();
            assert_eq!(&decoded, data, "roundtrip failed for {:?}", data);
        }
    }

    // ====== Round 22: State machine guard tests ======

    /// Established 後に advance_handshake → エラー (黙って成功しない)
    #[test]
    fn test_advance_after_established_errors() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "Bob",
                "pk",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();
        let session = m.begin_handshake(&peer.id).unwrap();
        let sid = session.id.clone();
        m.advance_handshake(&sid).unwrap(); // → EphemeralSent
        m.advance_handshake(&sid).unwrap(); // → ResponderIdentified
        m.advance_handshake(&sid).unwrap(); // → Established

        assert!(
            m.advance_handshake(&sid).is_err(),
            "Established 後の advance は拒否されるべき"
        );
    }

    /// Failed 後に advance_handshake → エラー
    #[test]
    fn test_advance_after_failed_errors() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "Bob",
                "pk",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();
        let session = m.begin_handshake(&peer.id).unwrap();
        m.fail_handshake(&session.id, "test").unwrap();

        assert!(
            m.advance_handshake(&session.id).is_err(),
            "Failed 後の advance は拒否されるべき"
        );
    }

    /// complete_handshake を Established 以前に呼ぶ → エラー
    #[test]
    fn test_complete_before_established_errors() {
        let mut m = PairManager::default();
        let peer = m
            .record_discovery(
                "Bob",
                "pk",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps(),
            )
            .unwrap()
            .clone();
        let session = m.begin_handshake(&peer.id).unwrap();
        // まだ Initializing
        assert!(
            m.complete_handshake(&session.id, "pk", "hash").is_err(),
            "Initializing で complete は拒否されるべき"
        );
    }

    /// unpair で存在しない ID → エラー
    #[test]
    fn test_unpair_nonexistent_errors() {
        let mut m = PairManager::default();
        assert!(m.unpair("ghost").is_err());
    }

    /// Display はユーザー向け日本語、Debug 形式ではない
    #[test]
    fn test_accept_mode_display_is_human_readable() {
        assert_eq!(format!("{}", AcceptMode::LanOnly), "LAN のみ");
        assert_eq!(format!("{}", AcceptMode::Open), "公開 (誰でも)");
        assert_eq!(format!("{}", AcceptMode::ContactsOnly), "連絡先のみ");
        assert_eq!(format!("{}", AcceptMode::Off), "受付停止");
        // Debug とは異なることを保証
        assert_ne!(
            format!("{}", AcceptMode::LanOnly),
            format!("{:?}", AcceptMode::LanOnly)
        );
    }

    // ====== Round 26: 境界値テスト ======

    /// 空 pubkey 拒否
    #[test]
    fn test_empty_pubkey_rejected() {
        let mut m = PairManager::default();
        assert!(m
            .record_discovery(
                "Bob",
                "",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps()
            )
            .is_err());
    }

    /// 空 display_name 拒否
    #[test]
    fn test_empty_display_name_rejected() {
        let mut m = PairManager::default();
        assert!(m
            .record_discovery(
                "",
                "pk-bob",
                test_endpoint(9001),
                DiscoveryMethod::Mdns,
                test_caps()
            )
            .is_err());
    }

    /// 同じ pubkey の重複 discovery は update (エラーではない)
    #[test]
    fn test_duplicate_pubkey_updates_not_errors() {
        let mut m = PairManager::default();
        m.record_discovery(
            "Bob",
            "pk-bob",
            test_endpoint(9001),
            DiscoveryMethod::Mdns,
            test_caps(),
        )
        .unwrap();
        m.record_discovery(
            "Bob v2",
            "pk-bob",
            test_endpoint(9002),
            DiscoveryMethod::Dht,
            test_caps(),
        )
        .unwrap();
        assert_eq!(m.discovered.len(), 1);
        assert_eq!(m.discovered[0].seen_count, 2);
        // endpoint は更新されてる
        assert_eq!(m.discovered[0].endpoint.port(), 9002);
    }

    #[test]
    fn test_format_pair_contains_key_sections() {
        let mut m = PairManager::default();
        m.record_discovery(
            "Alice's Mac",
            "pk-a",
            test_endpoint(9001),
            DiscoveryMethod::Mdns,
            test_caps(),
        )
        .unwrap();
        let out = format_pair(&m);
        assert!(out.contains('═'), "ヘッダ罫線");
        assert!(out.contains("発見"), "発見数");
        assert!(out.contains("ペア"), "ペア数");
        assert!(out.contains("受付モード"), "受付モード");
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut m = PairManager::default();
        m.record_discovery(
            "Alice",
            "pk-a",
            test_endpoint(9001),
            DiscoveryMethod::Mdns,
            test_caps(),
        )
        .unwrap();
        m.trust_store.insert(
            "pk-a".to_string(),
            TrustedIdentity {
                pubkey: "pk-a".into(),
                display_name: "Alice".into(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Trusted,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        let json1 = serde_json::to_string(&m).unwrap();
        let back: PairManager = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json1, json2, "PairManager roundtrip");
    }

    #[test]
    fn test_capability_challenge_issue_and_verify() {
        let mut m = PairManager::default();
        let peer_pubkey = "pk-peer";
        m.trust_store.insert(
            peer_pubkey.to_string(),
            TrustedIdentity {
                pubkey: peer_pubkey.to_string(),
                display_name: "Test Peer".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );

        // Issue challenge
        let challenge = m
            .issue_capability_challenge(
                peer_pubkey,
                "run llama-3.2-1b with seed=42",
                "expected-hash-deadbeef",
                30,
            )
            .unwrap();
        assert_eq!(challenge.state, ChallengeState::Pending);
        assert!(challenge.is_pending());

        // Verify with wrong output → Failed
        let result = m
            .verify_capability_proof(&challenge.challenge_id, "wrong-hash")
            .unwrap();
        assert!(!result);
        assert_eq!(
            m.pending_challenges[&challenge.challenge_id].state,
            ChallengeState::Failed
        );
        assert_eq!(m.trust_store[peer_pubkey].failed_jobs, 1);

        // Issue new challenge
        let challenge2 = m
            .issue_capability_challenge(
                peer_pubkey,
                "run llama-3.2-1b with seed=42",
                "expected-hash-deadbeef",
                30,
            )
            .unwrap();

        // Verify with correct output → Verified
        let result = m
            .verify_capability_proof(&challenge2.challenge_id, "expected-hash-deadbeef")
            .unwrap();
        assert!(result);
        assert_eq!(
            m.pending_challenges[&challenge2.challenge_id].state,
            ChallengeState::Verified
        );
        assert!(m.trust_store[peer_pubkey].capability_proven);
        assert_eq!(m.trust_store[peer_pubkey].successful_jobs, 1);
        assert!(m.trust_store[peer_pubkey].challenged_at.is_some());
    }

    #[test]
    fn test_reputation_score_calculation() {
        let mut m = PairManager::default();
        let peer_pubkey = "pk-reputation-test";
        m.trust_store.insert(
            peer_pubkey.to_string(),
            TrustedIdentity {
                pubkey: peer_pubkey.to_string(),
                display_name: "Test".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 7,
                failed_jobs: 3,
            },
        );

        let score = m.reputation_score(peer_pubkey).unwrap();
        assert!((score - 0.7).abs() < 0.001, "7/10 = 0.7");
    }

    #[test]
    fn test_is_capability_proven_or_trusted() {
        let mut m = PairManager::default();

        // Unknown peer, not proven
        m.trust_store.insert(
            "pk-unknown".to_string(),
            TrustedIdentity {
                pubkey: "pk-unknown".to_string(),
                display_name: "Unknown".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        assert!(!m.is_capability_proven_or_trusted("pk-unknown"));

        // Proven peer
        m.trust_store.insert(
            "pk-proven".to_string(),
            TrustedIdentity {
                pubkey: "pk-proven".to_string(),
                display_name: "Proven".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Unknown,
                note: None,
                capability_proven: true,
                challenged_at: Some(Utc::now()),
                successful_jobs: 1,
                failed_jobs: 0,
            },
        );
        assert!(m.is_capability_proven_or_trusted("pk-proven"));

        // Trusted peer
        m.trust_store.insert(
            "pk-trusted".to_string(),
            TrustedIdentity {
                pubkey: "pk-trusted".to_string(),
                display_name: "Trusted".to_string(),
                first_paired: Utc::now(),
                last_paired: Utc::now(),
                pair_count: 1,
                trust_level: TrustLevel::Trusted,
                note: None,
                capability_proven: false,
                challenged_at: None,
                successful_jobs: 0,
                failed_jobs: 0,
            },
        );
        assert!(m.is_capability_proven_or_trusted("pk-trusted"));
    }
}
