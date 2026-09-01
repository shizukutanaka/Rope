//! 2 プロセス E2E — v1 のループ全体をプロセス境界を越えて回す。
//!
//! ## なぜこれが要るのか (ソクラテス問答の記録)
//!
//! 「ジョブは実際に TCP を渡り、相手のマシンで実行されて返る」という主張に
//! 「それを独立したプロセス間で確認したか?」と問うたら、答えは否だった。
//! transport のテストは全て **1 プロセス内の 2 スレッド**である。
//! スレッド間で通ることとプロセス間で通ることは、ソケットの継承・環境変数・
//! グローバル状態の共有など、微妙に違う。
//!
//! 本バイナリは同じ実コード (`mdns`/`wire`/`transport`/`inference`) を
//! **2 つの OS プロセス**として動かす:
//!
//! - `lender` 役: TCP で待受け、mDNS で自分を広告し、来たジョブを
//!   **本物の Transformer** で実行して、支払いを受け取る
//! - `borrower` 役: **実 UDP マルチキャストで** lender を発見し、
//!   発見した鍵でピン留めして接続し、結果を受け取り、支払う
//!
//! ⚠️ 署名器はテスト用の決定論的スタブ (`TestSigner`)。**暗号ではない** —
//! ここで検証するのはプロトコルとプロセス境界であって、暗号強度ではない。
//! (実行ファイルは `rope` 本体ではない: 本体は clap/serde に依存し、
//! この環境では cargo が使えないためビルドできない。依存ゼロの 4 モジュール
//! だけで v1 ループの I/O 部分を全て賄えることが、逆にここで実証される。)

#[path = "../../src/net/inference.rs"]
mod inference;
#[path = "../../src/net/mdns.rs"]
mod mdns;
#[path = "../../src/net/transport.rs"]
mod transport;
#[path = "../../src/net/wire.rs"]
mod wire;

use inference::*;
use std::time::Duration;
use transport::*;
use wire::*;

/// テスト用の決定論的署名器。**暗号ではない** (上記モジュールコメント参照)。
struct TestSigner(u8);
impl FrameSigner for TestSigner {
    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let mut acc = [self.0; 8];
        for (i, b) in msg.iter().enumerate() {
            acc[i % 8] ^= *b;
        }
        acc.to_vec()
    }
}
impl FrameVerifier for TestSigner {
    fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
        self.sign(msg) == sig
    }
}
fn verifier_for(pk: &str) -> Option<Box<dyn FrameVerifier>> {
    u8::from_str_radix(pk, 16)
        .ok()
        .map(|n| Box::new(TestSigner(n)) as Box<dyn FrameVerifier>)
}

/// 「b」を出し続ける小さな本物のモデル (transport のテストと同じ構成)。
fn engine() -> CpuEngine {
    let (dim, hid, lay, voc, seq) = (4usize, 4usize, 1usize, 8usize, 16usize);
    let hs = 2usize;
    let kv = hs * 2;
    let q = hs * 2;
    let secs: Vec<(&str, usize)> = vec![
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
    let mut ck = Vec::new();
    for v in [
        dim as i32,
        hid as i32,
        lay as i32,
        2i32,
        2i32,
        -(voc as i32),
        seq as i32,
    ] {
        ck.extend_from_slice(&v.to_le_bytes());
    }
    for (name, n) in secs {
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
            ck.extend_from_slice(&v.to_le_bytes());
        }
    }
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
    let mut tk = Vec::new();
    tk.extend_from_slice(&16i32.to_le_bytes());
    for (p, s) in pieces {
        tk.extend_from_slice(&s.to_le_bytes());
        tk.extend_from_slice(&(p.len() as i32).to_le_bytes());
        tk.extend_from_slice(p.as_bytes());
    }
    CpuEngine {
        model: Model::from_bytes(&ck, CheckpointLimits::default()).expect("model"),
        tokenizer: Tokenizer::from_bytes(&tk, 8).expect("tokenizer"),
    }
}

struct LenderPolicy {
    engine: CpuEngine,
    received: std::cell::Cell<u64>,
}
impl JobPolicy for LenderPolicy {
    fn accept(
        &self,
        _pk: &str,
        _job: &str,
        model: &str,
        _p: &str,
        _max_out: u32,
        budget: u64,
    ) -> Result<(), String> {
        if model != "demo" {
            return Err(format!("知らないモデル: {model}"));
        }
        if budget == 0 {
            return Err("予算 0".into());
        }
        Ok(())
    }
    fn execute(&self, _m: &str, prompt: &str, max_out: u32) -> Result<Executed, String> {
        let c = self
            .engine
            .generate(
                prompt,
                ExecutionLimits {
                    max_prompt_tokens: 64,
                    max_output_tokens: max_out,
                },
                Sampler::deterministic(),
                1,
            )
            .map_err(|e| e.to_string())?;
        Ok(Executed {
            text: c.text,
            prompt_tokens: c.prompt_tokens,
            output_tokens: c.output_tokens,
            forward_passes: c.forward_passes,
        })
    }
    fn receive_payment(&self, _pk: &str, _job: &str, proofs: &[WireProof]) -> Result<u64, String> {
        let total: u64 = proofs.iter().map(|p| p.amount_sats).sum();
        self.received.set(total);
        Ok(total)
    }
}

fn run_lender(node_id: &str) -> ! {
    let me = NodeIdentity {
        node_id: node_id.to_string(),
        pubkey: "0a".into(),
    };
    let signer = TestSigner(0x0a);
    // **全インタフェースに bind する** (製品の `serve_jobs` と同じ)。
    // 127.0.0.1 だけに bind すると、mDNS で広告されるアドレスは
    // マルチキャストの送信元 IP (= 実 NIC のアドレス) なので接続が拒否される。
    // これはスレッド内テストでは絶対に出ない不具合で、E2E が最初に捕まえた。
    let listener = std::net::TcpListener::bind("0.0.0.0:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    println!("LENDER_READY port={port}");

    let adv = mdns::Advertisement {
        instance: "e2e-lender".into(),
        port,
        node_id: me.node_id.clone(),
        fingerprint: "E2:E2".into(),
        pubkey: me.pubkey.clone(),
    };
    // mDNS 応答スレッド (借り手が実マルチキャストで見つけられるように)
    {
        let adv = adv.clone();
        std::thread::spawn(move || {
            if let Ok(m) = mdns::Mdns::open() {
                let _ = m.discover(Some(&adv), Duration::from_secs(30), &adv.node_id);
            }
        });
    }

    let policy = LenderPolicy {
        engine: engine(),
        received: std::cell::Cell::new(0),
    };
    let (mut stream, _) = listener.accept().expect("accept");
    match serve_connection(&mut stream, &me, &signer, &verifier_for, &policy) {
        Ok(job) => {
            let ex = job.executed.expect("executed");
            println!(
                "LENDER_DONE accepted={} text={} paid={}",
                job.accepted, ex.text, job.paid_sats
            );
            std::process::exit(0);
        }
        Err(e) => {
            println!("LENDER_FAIL {e}");
            std::process::exit(1);
        }
    }
}

fn run_borrower(lender_node_id: &str) -> ! {
    // 実 UDP マルチキャストで lender を発見する
    let sock = mdns::Mdns::open().expect("mdns open");
    let mut found = None;
    for _ in 0..10 {
        let d = sock
            .discover(None, Duration::from_millis(700), "e2e-borrower-node")
            .expect("discover");
        if let Some(p) = d.peers.into_iter().find(|p| p.node_id == lender_node_id) {
            found = Some(p);
            break;
        }
    }
    let peer = match found {
        Some(p) => p,
        None => {
            println!("BORROWER_FAIL mdns で lender が見つからない");
            std::process::exit(1);
        }
    };
    println!(
        "BORROWER_FOUND instance={} port={} pubkey={}",
        peer.instance, peer.port, peer.pubkey
    );

    let addr = std::net::SocketAddr::from((peer.addr.expect("addr"), peer.port));
    let mut stream = std::net::TcpStream::connect(addr).expect("connect");
    let me = NodeIdentity {
        node_id: "e2e-borrower-node".into(),
        pubkey: "0b".into(),
    };
    let signer = TestSigner(0x0b);
    let mut pay = |_ex: &Executed| {
        vec![WireProof {
            id: "p1".into(),
            amount_sats: 7,
            mint_id: "mint1".into(),
            keyset_id: "ks".into(),
            secret: "s".into(),
            c: "02".into(),
            nullifier: "n1".into(),
        }]
    };
    match request_job(
        &mut stream,
        &me,
        &signer,
        &verifier_for,
        Some(&peer.pubkey), // 発見した鍵でピン留め
        "e2e-job",
        "demo",
        "ab",
        4,
        100,
        &mut pay,
    ) {
        Ok((ex, delivery)) => {
            println!(
                "BORROWER_DONE text={} delivery={:?} forward={}",
                ex.text, delivery, ex.forward_passes
            );
            std::process::exit(0);
        }
        Err(e) => {
            println!("BORROWER_FAIL {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("lender") => run_lender(args.get(2).map(|s| s.as_str()).unwrap_or("e2e-lender-node")),
        Some("borrower") => {
            run_borrower(args.get(2).map(|s| s.as_str()).unwrap_or("e2e-lender-node"))
        }
        _ => {
            eprintln!("usage: e2e (lender|borrower) [node_id]");
            std::process::exit(2);
        }
    }
}
