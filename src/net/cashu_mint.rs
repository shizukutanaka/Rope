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

/// ループバック宛か (ローカル mint でのテストは常に許す)。
///
/// `localhost` / `127.0.0.0/8` / `::1` を見る。名前解決はしない —
/// **DNS で実 mint に向けられる余地を残さないため**、文字列の時点で判定する。
pub fn is_loopback_mint(url: &str) -> bool {
    let host = match url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
    {
        Some(h) => h,
        None => return false,
    };
    // ポートとユーザ情報を落とす
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = if let Some(stripped) = host.strip_prefix('[') {
        // IPv6 リテラル
        stripped.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host.strip_prefix("127.").is_some_and(|rest| {
            rest.split('.').count() == 3 && rest.split('.').all(|o| o.parse::<u8>().is_ok())
        })
}

/// **プレースホルダ ecash のまま実 mint に接続してよいか。**
///
/// 既定は false。`ROPE_ALLOW_PLACEHOLDER_ECASH=1` で明示的に外せる。
///
/// ## なぜこの門が要るのか
///
/// `core::ecash::build_proof` と本モジュールの `build_blinded_outputs` は
/// **BDHKE のプレースホルダ**である (`docs/SURPLUS_AND_GAPS.md` §1.1)。
/// `k256` をこの環境に追加できず、§1.1 が楕円曲線演算の手書きを禁じているため。
///
/// この状態で**実 mint に Lightning で入金する**と、返ってくる署名は
/// でたらめな blinded message に対するものになり、**unblind できない proof**
/// しか手に入らない。**入金した sats は取り戻せない。**
///
/// 転送層が `ROPE_ALLOW_PLAINTEXT` で平文を既定拒否しているのと同じ規律を、
/// 金銭側にも適用する。判断は利用者に返すが、**黙って実 mint へ繋がない**。
pub fn placeholder_ecash_allowed() -> bool {
    std::env::var("ROPE_ALLOW_PLACEHOLDER_ECASH")
        .map(|v| v == "1")
        .unwrap_or(false)
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

        // 🔴 BDHKE がプレースホルダのまま実 mint へ繋ぐと、入金した sats に
        // 対して **unblind できない proof** しか返らず、資金が失われる
        // (`placeholder_ecash_allowed` の doc を参照)。
        // ループバック以外は既定で拒否する。
        if !is_loopback_mint(&config.mint_url) && !placeholder_ecash_allowed() {
            anyhow::bail!(
                "実 mint ({}) への接続は既定で拒否しています。\n\
                 BDHKE がプレースホルダのため、入金しても unblind できない proof \
                 しか受け取れず、資金を失います (docs/SURPLUS_AND_GAPS.md §1.1)。\n\
                 承知の上で試すなら ROPE_ALLOW_PLACEHOLDER_ECASH=1 を設定してください。",
                config.mint_url
            );
        }

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

    /// proof の状態を mint に問い合わせる。
    ///
    /// `ys` は各 proof の `hash_to_curve(secret)` (secp256k1 point, hex 圧縮形式) —
    /// 呼び出し元が生 secret ではなくこの変換済み値を渡す必要がある (NUT-07)。
    pub async fn check_state(&self, ys: Vec<String>) -> Result<CheckStateResponse> {
        let url = self.endpoint("/v1/checkstate");
        let req = CheckStateRequest { ys };
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
///
/// wire field は `Ys` — 各要素は `hash_to_curve(secret)` の結果 (secp256k1 point,
/// hex 圧縮形式) であり、proof の生 `secret` そのものではない。旧実装は
/// `{"secrets": [...]}` を送信しており、NUT-07 準拠 mint には 400 か
/// state 不一致で拒否される (本モジュールは未結線の pure DTO 層のため実害無いが、
/// v0.3 で結線する際は呼び出し元が hash_to_curve を実装して `ys` に渡す必要がある —
/// 本実装は secp256k1 依存を追加しないため未実装のまま)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckStateRequest {
    #[serde(rename = "Ys")]
    pub ys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckStateResponse {
    pub states: Vec<ProofState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofState {
    /// `hash_to_curve(secret)` の hex — proof の生 secret ではない (NUT-07)
    #[serde(rename = "Y")]
    pub y: String,
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
///   - `expected_amounts`: 自分が要求した額面群 (build_blinded_outputs に渡したもの)
///   - `signatures`: mint からの応答
///
/// 出力: ecash wallet に格納可能な Proof 群
///
/// ## 完全性検証 (問⑲)
/// mint の応答を無検証で信頼すると、悪意ある/バグった mint が額面 (`amount`) を
/// すり替えたり別 keyset で署名したものをウォレットが受理してしまう。
/// blinded message には額面が commit されている (build_blinded_outputs の B' は
/// amount を含む) ので、戻り値の `sig.amount` が要求額面と一致し、`sig.id` が
/// 期待 keyset と一致することを位置ごとに検証する。
///
/// 注意: 実 BDHHKE では C = unblinded(C') を計算するが、本実装は
/// pure logic ゲートウェイとして blinded C をそのまま signature に格納する。
/// 真の unblind は将来 secp256k1 + blind factor で実装。
pub fn translate_signatures_to_proofs(
    mint_id: &str,
    keyset_id: &str,
    secrets: &[Vec<u8>],
    expected_amounts: &[u64],
    signatures: &[BlindSignature],
) -> anyhow::Result<Vec<Proof>> {
    if secrets.len() != signatures.len() {
        anyhow::bail!(
            "secrets と signatures の数が不一致: {} vs {}",
            secrets.len(),
            signatures.len()
        );
    }
    if expected_amounts.len() != signatures.len() {
        anyhow::bail!(
            "expected_amounts と signatures の数が不一致: {} vs {}",
            expected_amounts.len(),
            signatures.len()
        );
    }
    let mut proofs = Vec::with_capacity(signatures.len());
    for ((secret_bytes, sig), &expected) in secrets
        .iter()
        .zip(signatures.iter())
        .zip(expected_amounts.iter())
    {
        if secret_bytes.len() != 32 {
            anyhow::bail!("secret は 32 byte 必須 ({})", secret_bytes.len());
        }
        // 問⑲: mint が額面をすり替えていないか検証
        if sig.amount != expected {
            anyhow::bail!(
                "mint が要求と異なる額面を署名: 要求 {} != 応答 {} (mint 不正/バグ)",
                expected,
                sig.amount
            );
        }
        // 問⑲: mint が期待 keyset で署名しているか検証
        if sig.id != keyset_id {
            anyhow::bail!(
                "mint が別 keyset で署名: 期待 {} != 応答 {} (信頼境界違反)",
                keyset_id,
                sig.id
            );
        }
        // C は 33 byte 圧縮 secp256k1 point (hex 66文字、02/03 prefix) の形を取る
        // (Proof.c の doc comment 参照)。mint が空文字列や壊れた形の C' を返しても
        // 無検証で Proof に埋め込むと、下流コードが「妥当な署名」と誤認する。
        if sig.c_.len() != 66
            || !(sig.c_.starts_with("02") || sig.c_.starts_with("03"))
            || !sig.c_.chars().all(|c| c.is_ascii_hexdigit())
        {
            anyhow::bail!(
                "mint 応答の C が不正な形式: {} byte (66 byte hex, 02/03 prefix 必須)",
                sig.c_.len()
            );
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

    /// 遠隔 mint を使うテストは**門を明示的に外して**書く。
    /// 外さないと `new` が拒否する — それがこの門の目的だから。
    #[cfg(feature = "http")]
    fn with_placeholder_ecash<T>(f: impl FnOnce() -> T) -> T {
        std::env::set_var("ROPE_ALLOW_PLACEHOLDER_ECASH", "1");
        let r = f();
        std::env::remove_var("ROPE_ALLOW_PLACEHOLDER_ECASH");
        r
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_url_validation_accepts_https() {
        with_placeholder_ecash(|| {
            let cfg = CashuClientConfig {
                mint_url: "https://mint.example.com".to_string(),
                ..Default::default()
            };
            assert!(CashuClient::new(cfg).is_ok());
        });
    }

    /// 🔴 **BDHKE がプレースホルダのまま実 mint へ繋がせない。**
    /// これを外すと、入金した sats に対して unblind できない proof しか
    /// 返らず資金を失う (`docs/SURPLUS_AND_GAPS.md` §1.1)。
    #[cfg(feature = "http")]
    #[test]
    fn test_remote_mint_is_refused_by_default() {
        std::env::remove_var("ROPE_ALLOW_PLACEHOLDER_ECASH");
        let cfg = CashuClientConfig {
            mint_url: "https://mint.example.com".to_string(),
            ..Default::default()
        };
        // `CashuClient` は Debug を持たないので `unwrap_err()` は使えない
        let msg = match CashuClient::new(cfg) {
            Ok(_) => panic!("遠隔 mint が既定で通ってしまった"),
            Err(e) => format!("{}", e),
        };
        assert!(msg.contains("既定で拒否"), "理由が利用者に分かる: {msg}");
        assert!(
            msg.contains("ROPE_ALLOW_PLACEHOLDER_ECASH"),
            "外し方も示す: {msg}"
        );
    }

    /// ループバックは門を外さなくても通る (ローカル mint での開発を妨げない)。
    #[cfg(feature = "http")]
    #[test]
    fn test_loopback_mint_needs_no_opt_in() {
        std::env::remove_var("ROPE_ALLOW_PLACEHOLDER_ECASH");
        for url in [
            "http://localhost:3338",
            "http://127.0.0.1:3338",
            "http://[::1]:3338",
        ] {
            let cfg = CashuClientConfig {
                mint_url: url.to_string(),
                ..Default::default()
            };
            assert!(CashuClient::new(cfg).is_ok(), "{url} は通るべき");
        }
    }

    /// ループバック判定が**文字列で**行われること。
    /// DNS 解決に頼ると `localhost.evil.com` のような名前で実 mint へ
    /// 向けられる余地が残る。
    #[test]
    fn test_loopback_detection_is_not_fooled_by_lookalikes() {
        assert!(is_loopback_mint("http://localhost:3338"));
        assert!(is_loopback_mint("https://127.0.0.1"));
        assert!(is_loopback_mint("http://[::1]:3338"));
        for bad in [
            "https://localhost.evil.com",
            "https://127.0.0.1.evil.com",
            "https://mint.example.com",
            "https://notlocalhost",
            "https://user@evil.com",
            "not a url",
        ] {
            assert!(!is_loopback_mint(bad), "{bad} をループバック扱いしない");
        }
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
        let client = with_placeholder_ecash(|| {
            let cfg = CashuClientConfig {
                mint_url: "https://mint.example.com/".to_string(),
                ..Default::default()
            };
            CashuClient::new(cfg).unwrap()
        });
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
        // NUT-07: レスポンスの proof 識別子フィールドは `Y` (hash_to_curve(secret))
        // であり `secret` ではない。
        let real = r#"{
            "states": [
                {"Y": "02aa...", "state": "UNSPENT", "witness": null},
                {"Y": "02bb...", "state": "SPENT", "witness": null}
            ]
        }"#;
        let r: CheckStateResponse = serde_json::from_str(real).unwrap();
        assert_eq!(r.states.len(), 2);
        assert_eq!(r.states[0].state, "UNSPENT");
        assert_eq!(r.states[1].state, "SPENT");
        assert_eq!(r.states[0].y, "02aa...");
    }

    /// NUT-07: リクエストの wire field は `Ys` (secrets ではない)。
    /// 旧実装は `{"secrets": [...]}` を送信しており、準拠 mint に拒否されていた。
    #[test]
    fn test_check_state_request_serializes_as_ys_not_secrets() {
        let req = CheckStateRequest {
            ys: vec!["02aa...".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            json.contains("\"Ys\""),
            "wire field は Ys であるべき: {}",
            json
        );
        assert!(
            !json.contains("\"secrets\""),
            "旧い secrets field が残ってはならない: {}",
            json
        );
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
        let r = translate_signatures_to_proofs("m1", "k", &secrets, &[1], &sigs);
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
        let proofs =
            translate_signatures_to_proofs("m1", "keyset_aa", &secrets, &[2, 8], &sigs).unwrap();
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

    /// 問⑲: mint が要求と異なる額面を署名したら拒否する
    #[test]
    fn test_translate_rejects_amount_tampering() {
        let (secrets, _) = build_blinded_outputs("keyset_aa", &[2, 8]);
        // mint が 2 番目の額面を 8 → 64 にすり替え
        let sigs = vec![
            BlindSignature {
                amount: 2,
                id: "keyset_aa".to_string(),
                c_: "02aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
                    .to_string(),
            },
            BlindSignature {
                amount: 64, // すり替え!
                id: "keyset_aa".to_string(),
                c_: "0311223344556677889900aabbccddeeff112233445566778899aabbccddeeff00"
                    .to_string(),
            },
        ];
        let r = translate_signatures_to_proofs("m1", "keyset_aa", &secrets, &[2, 8], &sigs);
        assert!(r.is_err(), "額面すり替えは拒否されるべき");
        assert!(r.unwrap_err().to_string().contains("額面"));
    }

    /// 問⑲: mint が別 keyset で署名したら拒否する
    #[test]
    fn test_translate_rejects_wrong_keyset() {
        let (secrets, _) = build_blinded_outputs("keyset_aa", &[2]);
        let sigs = vec![BlindSignature {
            amount: 2,
            id: "keyset_EVIL".to_string(), // 別 keyset
            c_: "02aa".to_string(),
        }];
        let r = translate_signatures_to_proofs("m1", "keyset_aa", &secrets, &[2], &sigs);
        assert!(r.is_err(), "別 keyset 署名は拒否されるべき");
        assert!(r.unwrap_err().to_string().contains("keyset"));
    }

    /// 問⑲: expected_amounts の件数不一致を検出する
    #[test]
    fn test_translate_rejects_expected_amount_count_mismatch() {
        let (secrets, _) = build_blinded_outputs("k", &[2, 8]);
        let sigs = vec![
            BlindSignature {
                amount: 2,
                id: "k".to_string(),
                c_: "a".to_string(),
            },
            BlindSignature {
                amount: 8,
                id: "k".to_string(),
                c_: "b".to_string(),
            },
        ];
        // expected_amounts が 1 件しかない
        let r = translate_signatures_to_proofs("m", "k", &secrets, &[2], &sigs);
        assert!(r.is_err());
    }

    /// mint が空文字列や短い C' を返した場合、無検証で Proof に埋め込まず拒否する。
    /// 旧実装は sig.c_ をそのまま Proof.c にコピーしており、mint のバグ/悪意で
    /// 壊れた C を返されると下流が「妥当な署名」と誤認していた。
    #[test]
    fn test_translate_rejects_malformed_c() {
        let (secrets, _) = build_blinded_outputs("k", &[2]);
        let sigs = vec![BlindSignature {
            amount: 2,
            id: "k".to_string(),
            c_: String::new(), // mint が空の C を返した
        }];
        let r = translate_signatures_to_proofs("m", "k", &secrets, &[2], &sigs);
        assert!(r.is_err(), "空の C は拒否されるべき");
        assert!(r.unwrap_err().to_string().contains("C"));
    }

    #[test]
    fn test_translate_rejects_wrong_secret_size() {
        let secrets = vec![vec![1u8; 16]]; // 半端
        let sigs = vec![BlindSignature {
            amount: 1,
            id: "k".to_string(),
            c_: "x".to_string(),
        }];
        assert!(translate_signatures_to_proofs("m", "k", &secrets, &[1], &sigs).is_err());
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
