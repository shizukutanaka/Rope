//! Rope のノード間ワイヤ形式 — フレーミングと署名の**論理**だけを持つ層。
//!
//! ## この層が暗号プリミティブを持たない理由
//!
//! [`docs/SURPLUS_AND_GAPS.md`] §1.1 は「検証済み crate (`snow`) 無しに Noise を
//! **手書きするな**」と明示している。これは正しい。しかしその禁止が掛かるのは
//! **プリミティブ**であって、**フレーミング・メッセージ定義・リプレイ防止・
//! サイズ上限といったプロトコル論理ではない**。
//!
//! 本モジュールはプリミティブを一切実装せず、[`FrameSigner`] /
//! [`FrameVerifier`] として**外から注入**させる。結果:
//!
//! - 実装は `ed25519-dalek` (検証済み・既存の依存) の 10 行程度のアダプタで済む
//! - プロトコル論理は**依存ゼロで単体テストできる** (この環境で実際に走る)
//! - `snow` が使えるようになったら、同じトレイトの裏に差し替えるだけ
//!
//! ## ⚠️ この層は秘匿性を提供しない
//!
//! 署名は**完全性と真正性**を与える (第三者が中身を差し替えられない) が、
//! **暗号化ではない**。LAN の受動的な盗聴者はプロンプトを読める。
//! v1 は既に「貸し手はプロンプトを見られる」と開示している
//! ([`SECURITY.md`]) が、**それを LAN 全体へ広げるかは製品判断**であり、
//! 上位層 (`transport`) が明示的な opt-in を要求する。

use std::fmt;

/// フレーム先頭のマジック。別プロトコルの誤接続を即座に落とす。
pub const MAGIC: [u8; 4] = *b"ROPE";
/// ワイヤ形式のバージョン。上げたら旧版は繋がらない (曖昧に壊れるより良い)。
pub const VERSION: u8 = 1;

/// 1 フレームのペイロード上限 (A9: 貸し手の資源を守る)。
pub const MAX_PAYLOAD: usize = 1 << 20; // 1 MiB
/// 署名の上限長。
pub const MAX_SIG: usize = 128;
/// 文字列フィールドの上限。
pub const MAX_STR: usize = 64 * 1024;

// ============================================================================
// エラー
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// マジックが違う (Rope のフレームではない)
    NotRope,
    /// バージョンが違う
    UnsupportedVersion(u8),
    /// フレームが途中で終わっている
    Truncated,
    /// 宣言された長さが上限超過
    TooLarge { len: usize, max: usize },
    /// 未知のメッセージ種別
    UnknownKind(u8),
    /// 署名検証に失敗した
    BadSignature,
    /// 文字列が UTF-8 でない
    NotUtf8,
    /// 必須フィールドが欠けている / 値が不正
    Malformed(&'static str),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::NotRope => f.write_str("Rope のフレームではない"),
            WireError::UnsupportedVersion(v) => write!(f, "非対応のワイヤ版 {}", v),
            WireError::Truncated => f.write_str("フレームが途中で終わっている"),
            WireError::TooLarge { len, max } => write!(f, "サイズ超過 ({} > {})", len, max),
            WireError::UnknownKind(k) => write!(f, "未知のメッセージ種別 {}", k),
            WireError::BadSignature => f.write_str("署名が一致しない"),
            WireError::NotUtf8 => f.write_str("文字列が UTF-8 でない"),
            WireError::Malformed(w) => write!(f, "フレームが不正 ({})", w),
        }
    }
}

impl std::error::Error for WireError {}

// ============================================================================
// 署名の注入点
// ============================================================================

/// フレームに署名する。**実装は本モジュールの外**
/// (`ed25519-dalek` アダプタ、テスト用の決定論的スタブ等)。
pub trait FrameSigner {
    fn sign(&self, msg: &[u8]) -> Vec<u8>;
}

/// フレームの署名を検証する。
pub trait FrameVerifier {
    fn verify(&self, msg: &[u8], sig: &[u8]) -> bool;
}

// ============================================================================
// メッセージ
// ============================================================================

/// ノード間で流れるメッセージ。**v1 はこれで全部**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 接続直後の名乗り。`pubkey` は TOFU の突き合わせに使う
    Hello {
        node_id: String,
        pubkey: String,
        /// リプレイ防止のためのワンタイム値 (16 進)
        nonce: String,
    },
    /// 推論の依頼
    JobRequest {
        job_id: String,
        model: String,
        prompt: String,
        max_output_tokens: u32,
        budget_sats: u64,
    },
    /// 推論の結果
    JobResult {
        job_id: String,
        text: String,
        prompt_tokens: u32,
        output_tokens: u32,
        /// 実際に回した forward の回数 (A4 の足がかり)
        forward_passes: u32,
    },
    /// 依頼を受けられない
    Reject { job_id: String, reason: String },
    /// 対価の支払い — **bearer token をそのまま渡す** (A5)。
    ///
    /// これを受け取った側は `EcashManager::receive_proofs` に通す。
    /// **トークンを持っていること自体が支払いの証明**なので、
    /// 口座も台帳も相手の同意も要らない。
    Payment {
        job_id: String,
        proofs: Vec<WireProof>,
    },
}

/// 1 枚の bearer token をワイヤに載せた形。
///
/// `core::ecash::Proof` と同じ内容だが、**この層は `core` にも serde にも
/// 依存しない** (依存ゼロを保つため)。変換は呼び出し側で行う。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WireProof {
    pub id: String,
    pub amount_sats: u64,
    pub mint_id: String,
    pub keyset_id: String,
    pub secret: String,
    pub c: String,
    pub nullifier: String,
}

/// 1 回の支払いに載せられる proof の枚数上限 (A9: 相手が無限に送ってこない)。
pub const MAX_PROOFS: usize = 256;

const KIND_HELLO: u8 = 1;
const KIND_JOB_REQUEST: u8 = 2;
const KIND_JOB_RESULT: u8 = 3;
const KIND_REJECT: u8 = 4;
const KIND_PAYMENT: u8 = 5;

impl Message {
    fn kind(&self) -> u8 {
        match self {
            Message::Hello { .. } => KIND_HELLO,
            Message::JobRequest { .. } => KIND_JOB_REQUEST,
            Message::JobResult { .. } => KIND_JOB_RESULT,
            Message::Reject { .. } => KIND_REJECT,
            Message::Payment { .. } => KIND_PAYMENT,
        }
    }

    fn encode_payload(&self) -> Vec<u8> {
        let mut p = Vec::new();
        match self {
            Message::Hello {
                node_id,
                pubkey,
                nonce,
            } => {
                put_str(&mut p, node_id);
                put_str(&mut p, pubkey);
                put_str(&mut p, nonce);
            }
            Message::JobRequest {
                job_id,
                model,
                prompt,
                max_output_tokens,
                budget_sats,
            } => {
                put_str(&mut p, job_id);
                put_str(&mut p, model);
                put_str(&mut p, prompt);
                p.extend_from_slice(&max_output_tokens.to_be_bytes());
                p.extend_from_slice(&budget_sats.to_be_bytes());
            }
            Message::JobResult {
                job_id,
                text,
                prompt_tokens,
                output_tokens,
                forward_passes,
            } => {
                put_str(&mut p, job_id);
                put_str(&mut p, text);
                p.extend_from_slice(&prompt_tokens.to_be_bytes());
                p.extend_from_slice(&output_tokens.to_be_bytes());
                p.extend_from_slice(&forward_passes.to_be_bytes());
            }
            Message::Reject { job_id, reason } => {
                put_str(&mut p, job_id);
                put_str(&mut p, reason);
            }
            Message::Payment { job_id, proofs } => {
                put_str(&mut p, job_id);
                p.extend_from_slice(&(proofs.len() as u32).to_be_bytes());
                for pr in proofs {
                    put_str(&mut p, &pr.id);
                    p.extend_from_slice(&pr.amount_sats.to_be_bytes());
                    put_str(&mut p, &pr.mint_id);
                    put_str(&mut p, &pr.keyset_id);
                    put_str(&mut p, &pr.secret);
                    put_str(&mut p, &pr.c);
                    put_str(&mut p, &pr.nullifier);
                }
            }
        }
        p
    }

    pub(crate) fn decode_payload(kind: u8, p: &[u8]) -> Result<Message, WireError> {
        let mut c = Reader::new(p);
        let m = match kind {
            KIND_HELLO => Message::Hello {
                node_id: c.str()?,
                pubkey: c.str()?,
                nonce: c.str()?,
            },
            KIND_JOB_REQUEST => Message::JobRequest {
                job_id: c.str()?,
                model: c.str()?,
                prompt: c.str()?,
                max_output_tokens: c.u32()?,
                budget_sats: c.u64()?,
            },
            KIND_JOB_RESULT => Message::JobResult {
                job_id: c.str()?,
                text: c.str()?,
                prompt_tokens: c.u32()?,
                output_tokens: c.u32()?,
                forward_passes: c.u32()?,
            },
            KIND_REJECT => Message::Reject {
                job_id: c.str()?,
                reason: c.str()?,
            },
            KIND_PAYMENT => {
                let job_id = c.str()?;
                let n = c.u32()? as usize;
                // 枚数を信じて確保しない — 上限を超えていればここで落とす
                if n > MAX_PROOFS {
                    return Err(WireError::TooLarge {
                        len: n,
                        max: MAX_PROOFS,
                    });
                }
                let mut proofs = Vec::with_capacity(n);
                for _ in 0..n {
                    proofs.push(WireProof {
                        id: c.str()?,
                        amount_sats: c.u64()?,
                        mint_id: c.str()?,
                        keyset_id: c.str()?,
                        secret: c.str()?,
                        c: c.str()?,
                        nullifier: c.str()?,
                    });
                }
                Message::Payment { job_id, proofs }
            }
            other => return Err(WireError::UnknownKind(other)),
        };
        // 余分なバイトを黙って捨てない — 曖昧なパースは攻撃面になる
        if !c.is_empty() {
            return Err(WireError::Malformed("末尾に余分なバイト"));
        }
        Ok(m)
    }

    /// 署名付きフレームへ符号化する。
    pub fn encode(&self, signer: &dyn FrameSigner) -> Result<Vec<u8>, WireError> {
        let payload = self.encode_payload();
        if payload.len() > MAX_PAYLOAD {
            return Err(WireError::TooLarge {
                len: payload.len(),
                max: MAX_PAYLOAD,
            });
        }
        // 署名対象 = バージョン + 種別 + 長さ + ペイロード。
        // **マジックを含めない**のは意味を持たないバイトだから。長さを含めるのは
        // 長さだけ書き換えて切り詰める攻撃を防ぐため。
        let signed_region = signed_region(self.kind(), &payload);
        let sig = signer.sign(&signed_region);
        if sig.len() > MAX_SIG {
            return Err(WireError::TooLarge {
                len: sig.len(),
                max: MAX_SIG,
            });
        }

        let mut out = Vec::with_capacity(payload.len() + sig.len() + 16);
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.push(self.kind());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.push(sig.len() as u8);
        out.extend_from_slice(&payload);
        out.extend_from_slice(&sig);
        Ok(out)
    }

    /// フレームを検証しつつ復号する。
    ///
    /// **署名を検証してからペイロードを解釈する** — 検証前に構造を信じない。
    pub fn decode(frame: &[u8], verifier: &dyn FrameVerifier) -> Result<Message, WireError> {
        let (kind, payload, sig) = split_frame(frame)?;
        let signed_region = signed_region(kind, payload);
        if !verifier.verify(&signed_region, sig) {
            return Err(WireError::BadSignature);
        }
        Message::decode_payload(kind, payload)
    }
}

/// 署名対象のバイト列を組み立てる (符号化側・復号側で必ず同じ手順を使う)。
fn signed_region(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 6);
    v.push(VERSION);
    v.push(kind);
    v.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    v.extend_from_slice(payload);
    v
}

/// ヘッダを検査してフレームを 3 分割する。**割り当てはしない**。
fn split_frame(frame: &[u8]) -> Result<(u8, &[u8], &[u8]), WireError> {
    const HEADER: usize = 4 + 1 + 1 + 4 + 1;
    if frame.len() < HEADER {
        return Err(WireError::Truncated);
    }
    if frame[..4] != MAGIC {
        return Err(WireError::NotRope);
    }
    if frame[4] != VERSION {
        return Err(WireError::UnsupportedVersion(frame[4]));
    }
    let kind = frame[5];
    let plen = u32::from_be_bytes([frame[6], frame[7], frame[8], frame[9]]) as usize;
    let slen = frame[10] as usize;
    if plen > MAX_PAYLOAD {
        return Err(WireError::TooLarge {
            len: plen,
            max: MAX_PAYLOAD,
        });
    }
    let end = HEADER
        .checked_add(plen)
        .and_then(|v| v.checked_add(slen))
        .ok_or(WireError::Truncated)?;
    if frame.len() != end {
        return Err(WireError::Truncated);
    }
    Ok((
        kind,
        &frame[HEADER..HEADER + plen],
        &frame[HEADER + plen..end],
    ))
}

/// ヘッダだけを見てフレーム全体の長さを返す (ストリームからの読み出し用)。
pub fn frame_len(header: &[u8]) -> Result<usize, WireError> {
    const HEADER: usize = 11;
    if header.len() < HEADER {
        return Err(WireError::Truncated);
    }
    if header[..4] != MAGIC {
        return Err(WireError::NotRope);
    }
    if header[4] != VERSION {
        return Err(WireError::UnsupportedVersion(header[4]));
    }
    let plen = u32::from_be_bytes([header[6], header[7], header[8], header[9]]) as usize;
    let slen = header[10] as usize;
    if plen > MAX_PAYLOAD {
        return Err(WireError::TooLarge {
            len: plen,
            max: MAX_PAYLOAD,
        });
    }
    Ok(HEADER + plen + slen)
}

/// ヘッダの長さ (ストリーム読み出しでまずこれだけ読む)。
pub const HEADER_LEN: usize = 11;

// ============================================================================
// 小さな符号化ヘルパ
// ============================================================================

fn put_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(MAX_STR);
    out.extend_from_slice(&(n as u32).to_be_bytes());
    out.extend_from_slice(&b[..n]);
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf, pos: 0 }
    }
    fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).ok_or(WireError::Truncated)?;
        let s = self.buf.get(self.pos..end).ok_or(WireError::Truncated)?;
        self.pos = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, WireError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn u64(&mut self) -> Result<u64, WireError> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn str(&mut self) -> Result<String, WireError> {
        let n = self.u32()? as usize;
        if n > MAX_STR {
            return Err(WireError::TooLarge {
                len: n,
                max: MAX_STR,
            });
        }
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| WireError::NotUtf8)
    }
}

// ============================================================================
// ストリーム入出力
// ============================================================================

use std::io::{Read, Write};

/// フレームを 1 つ書き出す。
pub fn write_frame(
    w: &mut impl Write,
    msg: &Message,
    signer: &dyn FrameSigner,
) -> std::io::Result<()> {
    let frame = msg
        .encode(signer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    w.write_all(&frame)?;
    w.flush()
}

/// フレームを 1 つ読み込む。
///
/// **長さはヘッダから取るが、上限を超えていれば読む前に落とす** —
/// 「4 GiB のフレームだ」と主張されて素直に確保しない (A9)。
pub fn read_frame_bytes(r: &mut impl Read) -> Result<Vec<u8>, WireError> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header)
        .map_err(|_| WireError::Truncated)?;
    let total = frame_len(&header)?;
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&header);
    buf.resize(total, 0);
    r.read_exact(&mut buf[HEADER_LEN..])
        .map_err(|_| WireError::Truncated)?;
    Ok(buf)
}

/// フレームを 1 つ読み、署名を検証して復号する。
pub fn read_frame(r: &mut impl Read, verifier: &dyn FrameVerifier) -> Result<Message, WireError> {
    let buf = read_frame_bytes(r)?;
    Message::decode(&buf, verifier)
}

/// **署名を検証せずに** 復号する。
///
/// ⚠️ 使ってよいのは **`Hello` の 1 回だけ**。相手の公開鍵は `Hello` の中にあり、
/// それを取り出さなければ検証しようがないという鶏卵問題があるため。
/// 取り出した鍵で**必ず同じフレームを検証し直す** ([`verify_hello`])。
/// それ以外のメッセージでこれを呼ばないこと。
pub fn decode_unverified(frame: &[u8]) -> Result<Message, WireError> {
    let (kind, payload, _sig) = split_frame(frame)?;
    Message::decode_payload(kind, payload)
}

/// `Hello` フレームを「その中の公開鍵で」検証する。
///
/// これが証明するのは **「送信者はその秘密鍵を持っている」**ことだけ。
/// **その鍵を信じてよいかは別問題** — TOFU (`core::pair`) の仕事である。
pub fn verify_hello(
    frame: &[u8],
    make_verifier: &dyn Fn(&str) -> Option<Box<dyn FrameVerifier>>,
) -> Result<Message, WireError> {
    let msg = decode_unverified(frame)?;
    let pubkey = match &msg {
        Message::Hello { pubkey, .. } => pubkey.clone(),
        _ => {
            return Err(WireError::Malformed(
                "最初のメッセージは Hello でなければならない",
            ))
        }
    };
    let verifier = make_verifier(&pubkey).ok_or(WireError::BadSignature)?;
    Message::decode(frame, verifier.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 決定論的な偽署名器。**暗号ではない** — プロトコル論理を試すためだけのもの。
    pub(super) struct FakeSig(pub u8);
    impl FrameSigner for FakeSig {
        fn sign(&self, msg: &[u8]) -> Vec<u8> {
            let mut acc = [self.0; 8];
            for (i, b) in msg.iter().enumerate() {
                acc[i % 8] ^= *b;
            }
            acc.to_vec()
        }
    }
    impl FrameVerifier for FakeSig {
        fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
            self.sign(msg) == sig
        }
    }

    fn job() -> Message {
        Message::JobRequest {
            job_id: "job-1".into(),
            model: "demo".into(),
            prompt: "こんにちは".into(),
            max_output_tokens: 64,
            budget_sats: 100,
        }
    }

    #[test]
    fn every_message_roundtrips() {
        let k = FakeSig(7);
        let msgs = vec![
            Message::Hello {
                node_id: "n1".into(),
                pubkey: "ab".into(),
                nonce: "00ff".into(),
            },
            job(),
            Message::JobResult {
                job_id: "job-1".into(),
                text: "出力".into(),
                prompt_tokens: 3,
                output_tokens: 5,
                forward_passes: 8,
            },
            Message::Reject {
                job_id: "job-1".into(),
                reason: "予算不足".into(),
            },
        ];
        for m in msgs {
            let f = m.encode(&k).unwrap();
            assert_eq!(Message::decode(&f, &k).unwrap(), m);
        }
    }

    #[test]
    fn a_different_key_is_rejected() {
        let f = job().encode(&FakeSig(1)).unwrap();
        assert_eq!(
            Message::decode(&f, &FakeSig(2)),
            Err(WireError::BadSignature)
        );
    }

    #[test]
    fn tampering_with_any_byte_is_detected() {
        let k = FakeSig(3);
        let f = job().encode(&k).unwrap();
        let mut detected = 0;
        for i in 0..f.len() {
            let mut bad = f.clone();
            bad[i] ^= 0xFF;
            // 改竄はエラーになる (署名不一致・ヘッダ不正・切り詰めのいずれか)。
            // **元のメッセージとして通ってしまう**ことが無いのが要点。
            match Message::decode(&bad, &k) {
                Ok(m) => assert_ne!(m, job(), "改竄が素通りした (byte {})", i),
                Err(_) => detected += 1,
            }
        }
        assert!(
            detected > f.len() / 2,
            "大半の改竄を検出する: {}/{}",
            detected,
            f.len()
        );
    }

    #[test]
    fn payload_is_not_interpreted_before_the_signature_is_checked() {
        // 壊れたペイロード + 正しくない署名 → 署名エラーが先に出る
        let mut f = job().encode(&FakeSig(1)).unwrap();
        let n = f.len();
        f[n - 3] ^= 0xFF; // 署名の中身を壊す
        assert_eq!(
            Message::decode(&f, &FakeSig(1)),
            Err(WireError::BadSignature)
        );
    }

    #[test]
    fn foreign_protocols_are_rejected_immediately() {
        let k = FakeSig(1);
        assert_eq!(
            Message::decode(b"GET / HTTP/1.1\r\n\r\n", &k),
            Err(WireError::NotRope)
        );
        let mut f = job().encode(&k).unwrap();
        f[4] = 99;
        assert_eq!(
            Message::decode(&f, &k),
            Err(WireError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn absurd_length_is_rejected_before_allocating() {
        let mut f = Vec::new();
        f.extend_from_slice(b"ROPE");
        f.push(1);
        f.push(2);
        f.extend_from_slice(&u32::MAX.to_be_bytes()); // 4 GiB と主張
        f.push(0);
        match Message::decode(&f, &FakeSig(1)) {
            Err(WireError::TooLarge { .. }) => {}
            other => panic!("上限で弾くべき: {:?}", other),
        }
        match frame_len(&f) {
            Err(WireError::TooLarge { .. }) => {}
            other => panic!("frame_len も弾くべき: {:?}", other),
        }
    }

    #[test]
    fn trailing_garbage_in_payload_is_rejected() {
        // 「余分なバイトを黙って捨てる」パーサは、同じフレームに 2 通りの意味を許す
        let k = FakeSig(5);
        let m = Message::Reject {
            job_id: "j".into(),
            reason: "r".into(),
        };
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.push(b'j');
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.push(b'r');
        payload.push(0xAA); // 余分
        let mut region = vec![1u8, 4u8];
        region.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        region.extend_from_slice(&payload);
        let sig = k.sign(&region);
        let mut f = Vec::new();
        f.extend_from_slice(b"ROPE");
        f.push(1);
        f.push(4);
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        f.push(sig.len() as u8);
        f.extend_from_slice(&payload);
        f.extend_from_slice(&sig);
        assert_eq!(
            Message::decode(&f, &k),
            Err(WireError::Malformed("末尾に余分なバイト"))
        );
        let _ = m;
    }

    #[test]
    fn truncations_never_panic() {
        let k = FakeSig(9);
        let f = job().encode(&k).unwrap();
        for n in 0..f.len() {
            let _ = Message::decode(&f[..n], &k);
        }
    }

    #[test]
    fn random_garbage_never_panics() {
        let k = FakeSig(11);
        let mut seed: u64 = 0x1234_5678;
        for _ in 0..3000 {
            let mut buf = Vec::new();
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let n = (seed % 120) as usize;
            // 半分はマジック付きにして、ヘッダ以降のパスも踏ませる
            if seed & 1 == 0 {
                buf.extend_from_slice(b"ROPE");
                buf.push(1);
            }
            for _ in 0..n {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                buf.push((seed >> 24) as u8);
            }
            let _ = Message::decode(&buf, &k);
            let _ = frame_len(&buf);
        }
    }

    #[test]
    fn frame_len_agrees_with_the_real_frame() {
        let k = FakeSig(1);
        for m in [
            job(),
            Message::Hello {
                node_id: "a".into(),
                pubkey: "b".into(),
                nonce: "c".into(),
            },
        ] {
            let f = m.encode(&k).unwrap();
            assert_eq!(frame_len(&f[..HEADER_LEN]).unwrap(), f.len());
        }
    }

    #[test]
    fn unknown_message_kinds_are_rejected_not_ignored() {
        let k = FakeSig(1);
        let payload: Vec<u8> = vec![];
        let mut region = vec![1u8, 77u8];
        region.extend_from_slice(&0u32.to_be_bytes());
        let sig = k.sign(&region);
        let mut f = Vec::new();
        f.extend_from_slice(b"ROPE");
        f.push(1);
        f.push(77);
        f.extend_from_slice(&0u32.to_be_bytes());
        f.push(sig.len() as u8);
        f.extend_from_slice(&payload);
        f.extend_from_slice(&sig);
        assert_eq!(Message::decode(&f, &k), Err(WireError::UnknownKind(77)));
    }
}

#[cfg(test)]
mod payment_tests {
    use super::tests::FakeSig;
    use super::*;

    fn proof(n: u64) -> WireProof {
        WireProof {
            id: format!("p{}", n),
            amount_sats: n,
            mint_id: "mint1".into(),
            keyset_id: "ks".into(),
            secret: "s3cr3t".into(),
            c: "0233".into(),
            nullifier: format!("null{}", n),
        }
    }

    #[test]
    fn payment_roundtrips_with_many_proofs() {
        let k = FakeSig(21);
        let m = Message::Payment {
            job_id: "job-9".into(),
            proofs: (0..64).map(proof).collect(),
        };
        let f = m.encode(&k).unwrap();
        assert_eq!(Message::decode(&f, &k).unwrap(), m);
    }

    #[test]
    fn empty_payment_roundtrips() {
        let k = FakeSig(1);
        let m = Message::Payment {
            job_id: "j".into(),
            proofs: vec![],
        };
        let f = m.encode(&k).unwrap();
        assert_eq!(Message::decode(&f, &k).unwrap(), m);
    }

    /// 「proof が 40 億枚ある」と主張されても確保しない。
    #[test]
    fn absurd_proof_count_is_rejected_before_allocating() {
        let k = FakeSig(3);
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.push(b'j');
        payload.extend_from_slice(&u32::MAX.to_be_bytes()); // 枚数
        let mut region = vec![VERSION, 5u8];
        region.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        region.extend_from_slice(&payload);
        let sig = k.sign(&region);
        let mut f = Vec::new();
        f.extend_from_slice(&MAGIC);
        f.push(VERSION);
        f.push(5);
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        f.push(sig.len() as u8);
        f.extend_from_slice(&payload);
        f.extend_from_slice(&sig);
        match Message::decode(&f, &k) {
            Err(WireError::TooLarge { max, .. }) => assert_eq!(max, MAX_PROOFS),
            other => panic!("枚数上限で弾くべき: {:?}", other),
        }
    }

    /// 支払いの 1 バイトでも書き換われば検出される (金額のすり替え防止)。
    #[test]
    fn tampering_with_a_payment_is_detected() {
        let k = FakeSig(7);
        let m = Message::Payment {
            job_id: "j".into(),
            proofs: vec![proof(64)],
        };
        let f = m.encode(&k).unwrap();
        for i in 0..f.len() {
            let mut bad = f.clone();
            bad[i] ^= 0xFF;
            if let Ok(got) = Message::decode(&bad, &k) {
                assert_ne!(got, m, "支払いの改竄が素通りした (byte {})", i);
            }
        }
    }
}
