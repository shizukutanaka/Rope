//! ノード間のジョブ転送 — `wire` のフレームを TCP に載せる層。
//!
//! ## 何が本物で、何がまだ無いか
//!
//! - **本物**: TCP 接続、フレーミング、署名による完全性・真正性、TOFU による
//!   鍵の突き合わせ、タイムアウトとサイズ上限 (A9)
//! - **無い**: **暗号化**。中身は平文で流れる。`wire` の署名は
//!   「誰が送ったか」と「途中で書き換えられていないか」を保証するが、
//!   「誰にも読まれない」は保証しない
//!
//! ## なぜ暗号化が無いのに出せるのか / 出せないのか
//!
//! [`docs/SURPLUS_AND_GAPS.md`] §1.1 は「検証済み crate 無しに Noise を
//! 手書きするな」と明示しており、この環境では `snow` も `x25519-dalek` も
//! 追加できない (crates.io が塞がれている)。**手書きはしない。**
//!
//! 代わりに、この層は **既定で平文転送を拒否する**。使うには
//! `ROPE_ALLOW_PLAINTEXT=1` を明示的に立てる必要がある
//! ([`plaintext_allowed`])。判断を利用者に返すのであって、黙って
//! 平文で流すのではない。

use std::io::{self};
use std::net::TcpStream;
use std::time::Duration;

use super::wire::{
    read_frame, read_frame_bytes, verify_hello, write_frame, FrameSigner, FrameVerifier, Message,
    WireError,
};

/// 接続・読み書きのタイムアウト (A9: 相手が黙り込んでも貸し手が固まらない)。
pub const IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Wire(WireError),
    /// 相手の名乗りが TOFU と食い違う / 未知の鍵
    UntrustedPeer(String),
    /// 期待しない順序のメッセージ
    Protocol(&'static str),
    /// 相手が依頼を断った
    Rejected(String),
    /// 平文転送が許可されていない
    PlaintextNotAllowed,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "通信エラー: {}", e),
            TransportError::Wire(e) => write!(f, "フレーム不正: {}", e),
            TransportError::UntrustedPeer(k) => write!(f, "信頼していないピア: {}", k),
            TransportError::Protocol(m) => write!(f, "プロトコル違反: {}", m),
            TransportError::Rejected(r) => write!(f, "依頼を断られた: {}", r),
            TransportError::PlaintextNotAllowed => f.write_str(
                "平文転送は既定で無効 (ROPE_ALLOW_PLAINTEXT=1 で明示的に有効化してください)",
            ),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        TransportError::Io(e)
    }
}
impl From<WireError> for TransportError {
    fn from(e: WireError) -> Self {
        TransportError::Wire(e)
    }
}

/// 平文転送が明示的に許可されているか。
///
/// **既定は false。** 暗号化が無い経路にプロンプトを流すかどうかは
/// 利用者の判断であって、こちらが黙って決めることではない。
pub fn plaintext_allowed() -> bool {
    std::env::var("ROPE_ALLOW_PLAINTEXT")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// 自分の名乗り。
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    pub node_id: String,
    pub pubkey: String,
}

/// 実行結果 (借り手にも貸し手にも同じ形で見える)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executed {
    pub text: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub forward_passes: u32,
}

/// 貸し手の受け入れ方針と実行。**A9 の判断はすべてここに集まる。**
pub trait JobPolicy {
    /// この依頼を受けるか。`Err(理由)` なら `Reject` を返す。
    fn accept(
        &self,
        peer_pubkey: &str,
        model: &str,
        prompt: &str,
        budget_sats: u64,
    ) -> Result<(), String>;

    /// 実際に走らせる。
    fn execute(
        &self,
        model: &str,
        prompt: &str,
        max_output_tokens: u32,
    ) -> Result<Executed, String>;
}

/// 1 件処理した記録 (貸し手側の戻り値)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedJob {
    pub peer_node_id: String,
    pub peer_pubkey: String,
    pub job_id: String,
    /// 受けて実行したか (false = Reject を返した)
    pub accepted: bool,
    pub executed: Option<Executed>,
}

/// 貸し手: 1 接続を最後まで処理する。
///
/// 手順は固定で、順序違反はその場で切る:
/// 1. `Hello` を受け取り、**その中の鍵で自己署名を検証**する
///    (鍵を持っている証明。信じてよいかは `make_verifier` = TOFU が決める)
/// 2. 自分の `Hello` を返す
/// 3. `JobRequest` を受け取り、`policy.accept` に諮る
/// 4. 通れば `policy.execute` して `JobResult`、駄目なら `Reject`
pub fn serve_connection(
    stream: &mut TcpStream,
    me: &NodeIdentity,
    signer: &dyn FrameSigner,
    make_verifier: &dyn Fn(&str) -> Option<Box<dyn FrameVerifier>>,
    policy: &dyn JobPolicy,
) -> Result<ServedJob, TransportError> {
    if !plaintext_allowed() {
        return Err(TransportError::PlaintextNotAllowed);
    }
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    // 1. Hello (自己署名) — 鍵はメッセージの中にある
    let hello_frame = read_frame_bytes(stream)?;
    let hello = verify_hello(&hello_frame, make_verifier)?;
    let (peer_node_id, peer_pubkey) = match hello {
        Message::Hello {
            node_id, pubkey, ..
        } => (node_id, pubkey),
        _ => return Err(TransportError::Protocol("最初は Hello でなければならない")),
    };

    // 2. こちらの名乗り
    write_frame(
        stream,
        &Message::Hello {
            node_id: me.node_id.clone(),
            pubkey: me.pubkey.clone(),
            nonce: String::new(),
        },
        signer,
    )?;

    // 3. 依頼。ここから先は相手の鍵で検証する
    let verifier = make_verifier(&peer_pubkey)
        .ok_or_else(|| TransportError::UntrustedPeer(peer_pubkey.clone()))?;
    let req = read_frame(stream, verifier.as_ref())?;
    let (job_id, model, prompt, max_output_tokens, budget_sats) = match req {
        Message::JobRequest {
            job_id,
            model,
            prompt,
            max_output_tokens,
            budget_sats,
        } => (job_id, model, prompt, max_output_tokens, budget_sats),
        _ => return Err(TransportError::Protocol("Hello の次は JobRequest")),
    };

    // 4. 受けるか判断して、受けるなら走らせる
    let decision = policy
        .accept(&peer_pubkey, &model, &prompt, budget_sats)
        .and_then(|_| policy.execute(&model, &prompt, max_output_tokens));

    match decision {
        Ok(ex) => {
            write_frame(
                stream,
                &Message::JobResult {
                    job_id: job_id.clone(),
                    text: ex.text.clone(),
                    prompt_tokens: ex.prompt_tokens,
                    output_tokens: ex.output_tokens,
                    forward_passes: ex.forward_passes,
                },
                signer,
            )?;
            Ok(ServedJob {
                peer_node_id,
                peer_pubkey,
                job_id,
                accepted: true,
                executed: Some(ex),
            })
        }
        Err(reason) => {
            write_frame(
                stream,
                &Message::Reject {
                    job_id: job_id.clone(),
                    reason,
                },
                signer,
            )?;
            Ok(ServedJob {
                peer_node_id,
                peer_pubkey,
                job_id,
                accepted: false,
                executed: None,
            })
        }
    }
}

/// 借り手: 1 件の推論を依頼して結果を受け取る。
#[allow(clippy::too_many_arguments)]
pub fn request_job(
    stream: &mut TcpStream,
    me: &NodeIdentity,
    signer: &dyn FrameSigner,
    make_verifier: &dyn Fn(&str) -> Option<Box<dyn FrameVerifier>>,
    job_id: &str,
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
    budget_sats: u64,
) -> Result<Executed, TransportError> {
    if !plaintext_allowed() {
        return Err(TransportError::PlaintextNotAllowed);
    }
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    write_frame(
        stream,
        &Message::Hello {
            node_id: me.node_id.clone(),
            pubkey: me.pubkey.clone(),
            nonce: String::new(),
        },
        signer,
    )?;

    // 相手の Hello も自己署名で検証する (対称)
    let their_hello_frame = read_frame_bytes(stream)?;
    let their_hello = verify_hello(&their_hello_frame, make_verifier)?;
    let their_pubkey = match their_hello {
        Message::Hello { pubkey, .. } => pubkey,
        _ => return Err(TransportError::Protocol("相手の Hello が来ない")),
    };
    let verifier = make_verifier(&their_pubkey)
        .ok_or_else(|| TransportError::UntrustedPeer(their_pubkey.clone()))?;

    write_frame(
        stream,
        &Message::JobRequest {
            job_id: job_id.to_string(),
            model: model.to_string(),
            prompt: prompt.to_string(),
            max_output_tokens,
            budget_sats,
        },
        signer,
    )?;

    match read_frame(stream, verifier.as_ref())? {
        Message::JobResult {
            job_id: got,
            text,
            prompt_tokens,
            output_tokens,
            forward_passes,
        } => {
            if got != job_id {
                return Err(TransportError::Protocol("job_id が一致しない"));
            }
            Ok(Executed {
                text,
                prompt_tokens,
                output_tokens,
                forward_passes,
            })
        }
        Message::Reject { reason, .. } => Err(TransportError::Rejected(reason)),
        _ => Err(TransportError::Protocol("JobResult か Reject を期待した")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::inference::*;
    use super::super::wire::*;
    use super::*;
    use std::net::{TcpListener, TcpStream};

    struct FakeSig(u8);
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
    /// 鍵 hex "0N" → FakeSig(N) を返す TOFU 相当のルックアップ
    fn tofu(pk: &str) -> Option<Box<dyn FrameVerifier>> {
        u8::from_str_radix(pk, 16)
            .ok()
            .map(|n| Box::new(FakeSig(n)) as Box<dyn FrameVerifier>)
    }
    fn tofu_denies_everyone(_pk: &str) -> Option<Box<dyn FrameVerifier>> {
        None
    }

    /// 本物の推論エンジンを積んだ貸し手の方針。
    struct RealPolicy {
        engine: CpuEngine,
        max_prompt: usize,
    }
    impl JobPolicy for RealPolicy {
        fn accept(&self, _pk: &str, model: &str, prompt: &str, budget: u64) -> Result<(), String> {
            if model != "demo" {
                return Err(format!("知らないモデル: {}", model));
            }
            if prompt.len() > self.max_prompt {
                return Err("プロンプトが長すぎる".into());
            }
            if budget == 0 {
                return Err("予算 0".into());
            }
            Ok(())
        }
        fn execute(&self, _model: &str, prompt: &str, max_out: u32) -> Result<Executed, String> {
            let limits = ExecutionLimits {
                max_prompt_tokens: 512,
                max_output_tokens: max_out,
            };
            let c = self
                .engine
                .generate(prompt, limits, Sampler::deterministic(), 1)
                .map_err(|e| e.to_string())?;
            Ok(Executed {
                text: c.text,
                prompt_tokens: c.prompt_tokens,
                output_tokens: c.output_tokens,
                forward_passes: c.forward_passes,
            })
        }
    }

    /// 「b」を出し続ける小さな本物のモデル (inference.rs のテストと同じ構成)
    fn engine() -> CpuEngine {
        let c = Config {
            dim: 4,
            hidden_dim: 4,
            n_layers: 1,
            n_heads: 2,
            n_kv_heads: 2,
            vocab_size: 8,
            seq_len: 16,
            shared_classifier: false,
        };
        let bytes = build_ck(c);
        let tok = tok_bytes();
        CpuEngine {
            model: Model::from_bytes(&bytes, CheckpointLimits::default()).unwrap(),
            tokenizer: Tokenizer::from_bytes(&tok, 8).unwrap(),
        }
    }
    fn build_ck(c: Config) -> Vec<u8> {
        let (dim, hid, lay, voc, seq) = (4usize, 4usize, 1usize, 8usize, 16usize);
        let hs = 2usize;
        let kv = 4usize;
        let q = 4usize;
        let mut secs: Vec<(&str, usize)> = vec![
            ("emb", voc * dim),
            ("rms_att", lay * dim),
            ("wq", lay * dim * q),
            ("wk", lay * dim * kv),
            ("wv", lay * dim * kv),
            ("wo", lay * q * dim),
            ("rms_ffn", lay * dim),
            ("w1", lay * hid * dim),
            ("w2", lay * dim * hid),
            ("w3", lay * hid * dim),
            ("rms_final", dim),
            ("fr", seq * hs / 2),
            ("fi", seq * hs / 2),
            ("wcls", voc * dim),
        ];
        let mut out = Vec::new();
        for v in [
            dim as i32,
            hid as i32,
            lay as i32,
            2i32,
            2i32,
            -(voc as i32),
            seq as i32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for (name, n) in secs.drain(..) {
            let vals: Vec<f32> = match name {
                "emb" => (0..n).map(|i| if i % 4 == 0 { 2.0 } else { 0.0 }).collect(),
                "rms_att" | "rms_ffn" | "rms_final" => vec![1.0; n],
                "wcls" => {
                    let mut v = vec![0.0; n];
                    v[5 * 4] = 1.0;
                    v
                }
                _ => vec![0.0; n],
            };
            for v in vals {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        let _ = c;
        out
    }
    fn tok_bytes() -> Vec<u8> {
        let pieces: [(&str, f32); 8] = [
            ("<unk>", 0.0),
            ("<s>", 0.0),
            ("</s>", 0.0),
            ("\u{2581}", -1.0),
            ("a", -2.0),
            ("b", -3.0),
            ("ab", 5.0),
            ("\u{2581}a", 4.0),
        ];
        let mut out = Vec::new();
        out.extend_from_slice(&16i32.to_le_bytes());
        for (p, s) in pieces {
            out.extend_from_slice(&s.to_le_bytes());
            out.extend_from_slice(&(p.len() as i32).to_le_bytes());
            out.extend_from_slice(p.as_bytes());
        }
        out
    }

    fn with_plaintext<T>(f: impl FnOnce() -> T) -> T {
        std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
        let r = f();
        std::env::remove_var("ROPE_ALLOW_PLAINTEXT");
        r
    }

    /// 実 TCP で、借り手 → 貸し手 → **本物の推論** → 結果 を通す。
    #[test]
    fn a_real_job_crosses_a_real_socket_and_is_really_computed() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();

            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });

            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let out = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                "job-1",
                "demo",
                "ab",
                4,
                100,
            )
            .unwrap();

            let served = server.join().unwrap();
            assert!(served.accepted);
            assert_eq!(served.peer_node_id, "borrower");
            assert_eq!(
                out.text, "bbbb",
                "貸し手側で実際に推論された結果が返る: {:?}",
                out
            );
            assert!(
                out.forward_passes >= out.output_tokens,
                "計算量が記録されている: {:?}",
                out
            );
            assert_eq!(served.executed.unwrap(), out, "両側が同じ結果を見ている");
        });
    }

    /// TOFU が知らない鍵は拒否される。
    #[test]
    fn an_unknown_peer_is_refused() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu_denies_everyone, &policy)
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let _ = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                "j",
                "demo",
                "ab",
                4,
                100,
            );
            let r = server.join().unwrap();
            assert!(r.is_err(), "未知の鍵は受け付けない");
        });
    }

    /// 方針が断れば Reject が返り、推論は走らない。
    #[test]
    fn a_rejected_job_is_never_executed() {
        with_plaintext(|| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                std::env::set_var("ROPE_ALLOW_PLAINTEXT", "1");
                let (mut s, _) = listener.accept().unwrap();
                let me = NodeIdentity {
                    node_id: "lender".into(),
                    pubkey: "0a".into(),
                };
                let policy = RealPolicy {
                    engine: engine(),
                    max_prompt: 4096,
                };
                serve_connection(&mut s, &me, &FakeSig(0x0a), &tofu, &policy).unwrap()
            });
            let mut c = TcpStream::connect(addr).unwrap();
            let me = NodeIdentity {
                node_id: "borrower".into(),
                pubkey: "0b".into(),
            };
            let err = request_job(
                &mut c,
                &me,
                &FakeSig(0x0b),
                &tofu,
                "j",
                "知らないモデル",
                "ab",
                4,
                100,
            )
            .unwrap_err();
            match err {
                TransportError::Rejected(r) => assert!(r.contains("知らないモデル"), "{r}"),
                o => panic!("Reject を期待: {o}"),
            }
            let served = server.join().unwrap();
            assert!(!served.accepted);
            assert!(served.executed.is_none(), "断った依頼は実行しない");
        });
    }

    /// 既定では平文転送を拒否する。
    #[test]
    fn plaintext_is_refused_by_default() {
        std::env::remove_var("ROPE_ALLOW_PLAINTEXT");
        assert!(!plaintext_allowed());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _ = listener.accept();
        });
        let mut c = TcpStream::connect(addr).unwrap();
        let me = NodeIdentity {
            node_id: "b".into(),
            pubkey: "0b".into(),
        };
        let r = request_job(&mut c, &me, &FakeSig(1), &tofu, "j", "demo", "x", 1, 1);
        assert!(
            matches!(r, Err(TransportError::PlaintextNotAllowed)),
            "既定で拒否する"
        );
    }
}
