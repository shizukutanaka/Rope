//! cashu_mint — Cashu protocol HTTP クライアント
//!
//! `core::ecash::EcashManager` が pure state machine として持つ操作を、
//! 実 mint サーバへの HTTP 呼び出しに翻訳する I/O 層。
//!
//! ## 仕様
//!
//! Cashu NUTs (NUT = "Notation, Usage, and Terminology") の最小 subset:
//!
//! - **NUT-06** GET `/v1/info` — mint メタデータ
//! - **NUT-01** GET `/v1/keys` — アクティブ keyset
//! - **NUT-04** POST `/v1/mint/quote/bolt11` — mint quote 取得
//! - **NUT-04** POST `/v1/mint/bolt11` — proof 発行
//! - **NUT-05** POST `/v1/melt/quote/bolt11` — melt quote (LN 出金)
//! - **NUT-05** POST `/v1/melt/bolt11` — melt 実行
//! - **NUT-07** POST `/v1/checkstate` — proof 二重使用検証
//!
//! ## Apple 流ガードレール
//!
//! 1. **タイムアウト固定 10 秒** — ハングしない
//! 2. **TLS 必須** — rustls 経由、`http://` は明示的に拒否しない (テスト用)
//!    が `mint.url` のスキーム検証を行う
//! 3. **エラーは詳細にラップ** — anyhow::Context で「どの URL でどの段階」が
//!    判別可能
//! 4. **state-mutating 操作は冪等** — 同じ quote を 2 回呼んでも問題なし
//!    (mint 側が同じ proof を返す前提)
//!
//! ## 既存 ecash.rs との接続
//!
//! 本モジュールは ecash::EcashManager に直接依存しない (循環回避)。
//! 代わりに ecash::Proof / ecash::Mint と互換な軽量 DTO を返し、
//! 呼び出し側 (main.rs) が翻訳する。

#[cfg(feature = "http")]
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// HTTP クライアント設定
#[derive(Debug, Clone)]
pub struct CashuClientConfig {
    /// mint URL (例: `https://mint.minibits.cash`)
    pub mint_url: String,
    /// HTTP タイムアウト
    pub timeout: Duration,
    /// User-Agent (mint 側ログ識別用)
    pub user_agent: String,
}

impl Default for CashuClientConfig {
    fn default() -> Self {
        Self {
            mint_url: String::new(),
            timeout: Duration::from_secs(10),
            user_agent: format!("rope/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Cashu HTTP クライアント
///
/// 実ネットワーク I/O。デフォルトビルドには含まれない (`--features http` で有効化)。
/// 純粋ロジック (translate_signatures_to_proofs 等) と DTO は feature 不問で利用可。
#[cfg(feature = "http")]
pub struct CashuClient {
    config: CashuClientConfig,
    http: reqwest::Client,
}

#[cfg(feature = "http")]
impl CashuClient {
    /// 新規クライアント作成 (URL 検証含む)
    pub fn new(config: CashuClientConfig) -> Result<Self> {
        if config.mint_url.is_empty() {
            anyhow::bail!("mint_url が空");
        }
        let parsed = reqwest::Url::parse(&config.mint_url)
            .context(format!("不正な mint URL: {}", config.mint_url))?;
        if parsed.scheme() != "https" && parsed.scheme() != "http" {
            anyhow::bail!("mint URL は http(s) のみ");
        }
        // 本番では http:// 拒否すべきだが、ローカル mint テストで使うので許可

        let http = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .context("HTTP クライアント構築失敗")?;

        Ok(Self { config, http })
    }

    /// URL 結合 (末尾 / の有無を吸収)
    fn endpoint(&self, path: &str) -> String {
        let base = self.config.mint_url.trim_end_matches('/');
        let p = path.trim_start_matches('/');
        format!("{}/{}", base, p)
    }

    // ====================================================================
    // NUT-06 — info
    // ====================================================================

    /// mint の能力 / 情報を取得
    pub async fn get_info(&self) -> Result<MintInfo> {
        let url = self.endpoint("/v1/info");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context(format!("GET {} 失敗", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("GET {} 返信 {}", url, resp.status());
        }

        let info: MintInfo = resp.json().await.context("MintInfo JSON parse 失敗")?;
        Ok(info)
    }

    // ====================================================================
    // NUT-01 — keys
    // ====================================================================

    /// アクティブ keyset を取得
    pub async fn get_keys(&self) -> Result<KeysResponse> {
        let url = self.endpoint("/v1/keys");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context(format!("GET {} 失敗", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("GET {} 返信 {}", url, resp.status());
        }

        let keys: KeysResponse = resp.json().await.context("KeysResponse JSON parse 失敗")?;
        Ok(keys)
    }

    // ====================================================================
    // NUT-04 — mint (LN 預入 → proof)
    // ====================================================================

    /// mint quote 取得 (LN invoice を貰う)
    pub async fn request_mint_quote(&self, amount_sats: u64) -> Result<MintQuoteResponse> {
        let url = self.endpoint("/v1/mint/quote/bolt11");
        let req = MintQuoteRequest {
            amount: amount_sats,
            unit: "sat".to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context(format!("POST {} 失敗", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST {} {}: {}", url, status, body);
        }

        let q: MintQuoteResponse = resp.json().await.context("MintQuoteResponse parse 失敗")?;
        Ok(q)
    }

    /// LN 預入完了後、proof を実発行
    ///
    /// `outputs` は blinded message 群 (BlindedMessage)
    pub async fn mint(&self, quote_id: &str, outputs: Vec<BlindedMessage>) -> Result<MintResponse> {
        let url = self.endpoint("/v1/mint/bolt11");
        let req = MintRequest {
            quote: quote_id.to_string(),
            outputs,
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context(format!("POST {} 失敗", url))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("POST {} {}: {}", url, status, body);
        }

        let m: MintResponse = resp.json().await.context("MintResponse parse 失敗")?;
        Ok(m)
    }

    // ====================================================================
    // NUT-07 — proof 状態確認 (二重使用検出)
    // ====================================================================

    /// proof secret 群の状態を mint に問い合わせ
    pub async fn check_state(&self, secrets: Vec<String>) -> Result<CheckStateResponse> {
        let url = self.endpoint("/v1/checkstate");
        let req = CheckStateRequest { secrets };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context(format!("POST {} 失敗", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("POST {} 返信 {}", url, resp.status());
        }

        let s: CheckStateResponse = resp.json().await.context("CheckStateResponse parse 失敗")?;
        Ok(s)
    }

    // ====================================================================
    // NUT-05 — melt (proof → LN 出金)
    // ====================================================================

    /// melt quote 取得 (LN invoice を払う見積もり)
    pub async fn request_melt_quote(&self, bolt11_invoice: &str) -> Result<MeltQuoteResponse> {
        let url = self.endpoint("/v1/melt/quote/bolt11");
        let req = MeltQuoteRequest {
            request: bolt11_invoice.to_string(),
            unit: "sat".to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .context(format!("POST {} 失敗", url))?;

        if !resp.status().is_success() {
            anyhow::bail!("POST {} 返信 {}", url, resp.status());
        }

        let q: MeltQuoteResponse = resp.json().await.context("MeltQuoteResponse parse 失敗")?;
        Ok(q)
    }

    /// 接続疎通テスト (info を取って timeout 内に返るか確認)
    pub async fn health_check(&self) -> Result<MintHealthReport> {
        let started = std::time::Instant::now();
        match self.get_info().await {
            Ok(info) => Ok(MintHealthReport {
                mint_url: self.config.mint_url.clone(),
                reachable: true,
                latency_ms: started.elapsed().as_millis() as u64,
                version: Some(info.version),
                error: None,
            }),
            Err(e) => Ok(MintHealthReport {
                mint_url: self.config.mint_url.clone(),
                reachable: false,
                latency_ms: started.elapsed().as_millis() as u64,
                version: None,
                error: Some(e.to_string()),
            }),
        }
    }
}

// ========================================================================
// DTO — Cashu wire format
// ========================================================================

/// NUT-06 mint info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintInfo {
    pub name: String,
    pub pubkey: Option<String>,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_long: String,
    #[serde(default)]
    pub contact: Vec<ContactInfo>,
    #[serde(default)]
    pub motd: String,
    #[serde(default)]
    pub nuts: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    pub method: String,
    pub info: String,
}

/// NUT-01 keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysResponse {
    pub keysets: Vec<Keyset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyset {
    pub id: String,
    pub unit: String,
    /// amount → pubkey hex
    pub keys: std::collections::HashMap<String, String>,
}

/// NUT-04 mint quote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintQuoteRequest {
    pub amount: u64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintQuoteResponse {
    pub quote: String,
    /// LN invoice (BOLT-11)
    pub request: String,
    /// quote 状態: UNPAID / PAID / ISSUED
    pub state: String,
    pub expiry: u64,
}

/// NUT-04 mint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintRequest {
    pub quote: String,
    pub outputs: Vec<BlindedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindedMessage {
    pub amount: u64,
    pub id: String,
    pub b_: String, // blinded secret
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintResponse {
    pub signatures: Vec<BlindSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindSignature {
    pub amount: u64,
    pub id: String,
    pub c_: String, // blind sig
}

/// NUT-07 proof 状態確認
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckStateRequest {
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckStateResponse {
    pub states: Vec<ProofState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofState {
    pub secret: String,
    /// UNSPENT / PENDING / SPENT
    pub state: String,
    pub witness: Option<String>,
}

/// NUT-05 melt quote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeltQuoteRequest {
    pub request: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeltQuoteResponse {
    pub quote: String,
    pub amount: u64,
    pub fee_reserve: u64,
    pub state: String,
    pub expiry: u64,
}

/// 接続診断結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintHealthReport {
    pub mint_url: String,
    pub reachable: bool,
    pub latency_ms: u64,
    pub version: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// Translation layer: net DTO ↔ core::ecash 型
//
// Round 21 (PROS_CONS_R21 #1): cashu_mint と core/ecash が型レベルで
// 独立したまま、データ flow だけ繋ぐ。新モジュール無し、関数追加のみ。
// ============================================================================

use crate::core::ecash::{Mint as EcashMint, Proof};

/// 実 mint からの BlindSignature 群を ecash::Proof 群に変換
///
/// 入力:
///   - `mint_id`: rope 内 mint registry の ID
///   - `secrets`: 自分が出力した blinded message に対応する 32 byte secret 群 (mint_tokens 呼出側で生成保管)
///   - `signatures`: mint からの応答
///
/// 出力: ecash wallet に格納可能な Proof 群
///
/// 注意: 実 BDHHKE では C = unblinded(C') を計算するが、本実装は
/// pure logic ゲートウェイとして blinded C をそのまま signature に格納する。
/// 真の unblind は将来 secp256k1 + blind factor で実装。
pub fn translate_signatures_to_proofs(
    mint_id: &str,
    keyset_id: &str,
    secrets: &[Vec<u8>],
    signatures: &[BlindSignature],
) -> anyhow::Result<Vec<Proof>> {
    if secrets.len() != signatures.len() {
        anyhow::bail!(
            "secrets と signatures の数が不一致: {} vs {}",
            secrets.len(),
            signatures.len()
        );
    }
    let mut proofs = Vec::with_capacity(signatures.len());
    for (secret_bytes, sig) in secrets.iter().zip(signatures.iter()) {
        if secret_bytes.len() != 32 {
            anyhow::bail!("secret は 32 byte 必須 ({})", secret_bytes.len());
        }
        let secret_hex = hex::encode(secret_bytes);
        let null_hash = blake3::hash(secret_bytes);
        let nullifier = hex::encode(&null_hash.as_bytes()[..32]);

        proofs.push(Proof {
            id: uuid::Uuid::now_v7().to_string(),
            amount_sats: sig.amount,
            mint_id: mint_id.to_string(),
            keyset_id: keyset_id.to_string(),
            secret: secret_hex,
            c: sig.c_.clone(),
            signature: sig.c_.clone(),
            nullifier,
            created_at: chrono::Utc::now(),
        })
    }
    Ok(proofs)
}

/// 32 byte secret 群を生成し、対応する BlindedMessage を構築 (mint quote → mint 用)
///
/// 戻り値:
///   - secrets: 後で `translate_signatures_to_proofs` に渡す
///   - outputs: mint の `/v1/mint/bolt11` の `outputs` field に渡す
///
/// 額面分解は呼出側 (例: 100 sats → [4, 32, 64])。
pub fn build_blinded_outputs(
    keyset_id: &str,
    amounts_sats: &[u64],
) -> (Vec<Vec<u8>>, Vec<BlindedMessage>) {
    use rand::RngCore;
    let mut secrets = Vec::with_capacity(amounts_sats.len());
    let mut outputs = Vec::with_capacity(amounts_sats.len());

    for &amount in amounts_sats {
        let mut secret = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);

        // 真の BDHHKE では Y = HashToCurve(secret), B' = Y + r*G。
        // 本実装は wire format 準拠の placeholder (B' = HashToCurve 風 hex)。
        let b_prime = blake3::hash(
            format!("B'|{}|{}|{}", keyset_id, amount, hex::encode(&secret)).as_bytes(),
        );
        let mut compressed = [0u8; 33];
        compressed[0] = 0x02;
        compressed[1..33].copy_from_slice(&b_prime.as_bytes()[..32]);

        outputs.push(BlindedMessage {
            amount,
            id: keyset_id.to_string(),
            b_: hex::encode(compressed),
        });
        secrets.push(secret);
    }
    (secrets, outputs)
}

/// rope の mint registry エントリを net レイヤ用 URL に解決
pub fn resolve_mint_url(mint: &EcashMint) -> String {
    mint.url.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_rejects_empty() {
        let cfg = CashuClientConfig::default();
        assert!(CashuClient::new(cfg).is_err());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_rejects_garbage() {
        let cfg = CashuClientConfig {
            mint_url: "not a url".to_string(),
            ..Default::default()
        };
        assert!(CashuClient::new(cfg).is_err());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_rejects_ftp() {
        let cfg = CashuClientConfig {
            mint_url: "ftp://example.com".to_string(),
            ..Default::default()
        };
        assert!(CashuClient::new(cfg).is_err());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_accepts_https() {
        let cfg = CashuClientConfig {
            mint_url: "https://mint.example.com".to_string(),
            ..Default::default()
        };
        assert!(CashuClient::new(cfg).is_ok());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_accepts_http_for_local() {
        let cfg = CashuClientConfig {
            mint_url: "http://localhost:3338".to_string(),
            ..Default::default()
        };
        assert!(CashuClient::new(cfg).is_ok());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_endpoint_construction_dedupes_slash() {
        let cfg = CashuClientConfig {
            mint_url: "https://mint.example.com/".to_string(),
            ..Default::default()
        };
        let client = CashuClient::new(cfg).unwrap();
        assert_eq!(
            client.endpoint("/v1/info"),
            "https://mint.example.com/v1/info"
        );
        assert_eq!(
            client.endpoint("v1/keys"),
            "https://mint.example.com/v1/keys"
        );
    }

    #[test]
    fn test_default_user_agent_has_version() {
        let cfg = CashuClientConfig::default();
        assert!(cfg.user_agent.starts_with("rope/"));
        assert!(cfg.user_agent.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn test_default_timeout_10s() {
        let cfg = CashuClientConfig::default();
        assert_eq!(cfg.timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_dto_roundtrip() {
        // Cashu wire format との互換性最低保証
        let info = MintInfo {
            name: "test".to_string(),
            pubkey: Some("abc".to_string()),
            version: "Nutshell/0.16.5".to_string(),
            description: "".to_string(),
            description_long: "".to_string(),
            contact: vec![],
            motd: "".to_string(),
            nuts: serde_json::json!({}),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: MintInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
    }

    #[test]
    fn test_mint_quote_response_parse_real_format() {
        // Nutshell 0.16 が返す実フォーマット
        let real = r#"{
            "quote": "abc123",
            "request": "lnbc100u1pn...",
            "state": "UNPAID",
            "expiry": 1700000000
        }"#;
        let q: MintQuoteResponse = serde_json::from_str(real).unwrap();
        assert_eq!(q.quote, "abc123");
        assert_eq!(q.state, "UNPAID");
    }

    #[test]
    fn test_check_state_response_parse() {
        let real = r#"{
            "states": [
                {"secret": "s1", "state": "UNSPENT", "witness": null},
                {"secret": "s2", "state": "SPENT", "witness": null}
            ]
        }"#;
        let r: CheckStateResponse = serde_json::from_str(real).unwrap();
        assert_eq!(r.states.len(), 2);
        assert_eq!(r.states[0].state, "UNSPENT");
        assert_eq!(r.states[1].state, "SPENT");
    }

    // ====================================================================
    // Round 21: Translation layer tests (PROS_CONS_R21 #1)
    // ====================================================================

    #[test]
    fn test_build_blinded_outputs_shapes() {
        let (secrets, outputs) = build_blinded_outputs("00abcdef12345678", &[1, 4, 16]);
        assert_eq!(secrets.len(), 3);
        assert_eq!(outputs.len(), 3);
        // 各 secret は 32 byte
        for s in &secrets {
            assert_eq!(s.len(), 32);
        }
        // 各 B' は 66 hex (33 byte 圧縮 point)
        for o in &outputs {
            assert_eq!(o.b_.len(), 66);
            assert!(o.b_.starts_with("02"));
            assert_eq!(o.id, "00abcdef12345678");
        }
        assert_eq!(outputs[0].amount, 1);
        assert_eq!(outputs[2].amount, 16);
    }

    #[test]
    fn test_blinded_outputs_have_distinct_secrets() {
        let (secrets, _) = build_blinded_outputs("k1", &[1, 1, 1, 1]);
        let mut set = std::collections::HashSet::new();
        for s in &secrets {
            set.insert(s.clone());
        }
        assert_eq!(set.len(), 4, "乱数 secret が衝突");
    }

    #[test]
    fn test_translate_signatures_count_mismatch_errors() {
        let secrets = vec![vec![0u8; 32]];
        let sigs = vec![
            BlindSignature {
                amount: 1,
                id: "k".to_string(),
                c_: "x".to_string(),
            },
            BlindSignature {
                amount: 2,
                id: "k".to_string(),
                c_: "y".to_string(),
            },
        ];
        let r = translate_signatures_to_proofs("m1", "k", &secrets, &sigs);
        assert!(r.is_err());
    }

    #[test]
    fn test_translate_signatures_round_trip() {
        let (secrets, _outputs) = build_blinded_outputs("keyset_aa", &[2, 8]);
        let sigs = vec![
            BlindSignature {
                amount: 2,
                id: "keyset_aa".to_string(),
                c_: "02aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
                    .to_string(),
            },
            BlindSignature {
                amount: 8,
                id: "keyset_aa".to_string(),
                c_: "0311223344556677889900aabbccddeeff112233445566778899aabbccddeeff00"
                    .to_string(),
            },
        ];
        let proofs = translate_signatures_to_proofs("m1", "keyset_aa", &secrets, &sigs).unwrap();
        assert_eq!(proofs.len(), 2);
        assert_eq!(proofs[0].amount_sats, 2);
        assert_eq!(proofs[0].keyset_id, "keyset_aa");
        assert_eq!(proofs[0].mint_id, "m1");
        // nullifier は secret から決定論的に生成
        let expected_null0 = hex::encode(&blake3::hash(&secrets[0]).as_bytes()[..32]);
        assert_eq!(proofs[0].nullifier, expected_null0);
        // signature == c == 引数の c_
        assert_eq!(proofs[0].c, sigs[0].c_);
        assert_eq!(proofs[0].signature, sigs[0].c_);
    }

    #[test]
    fn test_translate_rejects_wrong_secret_size() {
        let secrets = vec![vec![1u8; 16]]; // 半端
        let sigs = vec![BlindSignature {
            amount: 1,
            id: "k".to_string(),
            c_: "x".to_string(),
        }];
        assert!(translate_signatures_to_proofs("m", "k", &secrets, &sigs).is_err());
    }

    /// MintQuoteResponse の serde roundtrip — wire 互換性保証
    #[test]
    fn test_mint_quote_response_roundtrip() {
        let q = MintQuoteResponse {
            quote: "abc".into(),
            request: "lnbc100u1...".into(),
            state: "UNPAID".into(),
            expiry: 1700000000,
        };
        let json = serde_json::to_string(&q).unwrap();
        let back: MintQuoteResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }

    /// BlindedMessage の serde roundtrip
    #[test]
    fn test_blinded_message_roundtrip() {
        let msg = BlindedMessage {
            amount: 64,
            id: "keyset_01".into(),
            b_: "02aabb".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let back: BlindedMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
