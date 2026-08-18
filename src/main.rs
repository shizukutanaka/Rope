//! rope — 他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる方法
//!
//! ROPE_2028 4 動詞のみ (Apple-clean):
//!   rope            引数無し: first_run (60秒 wow moment)
//!   rope pair       AirDrop 式ピア発見
//!   rope run <model>  ローカル優先、足りなきゃピア
//!   rope earn       自分の GPU を貸し出す

// core/ の state machine は完全な API を公開するが、現行の 4 動詞からは
// 一部のみ到達する。残りは v0.3 (実 I/O 結線) で使う予約 API。
// 削除すると state machine の完全性が崩れるため allow で明示保持。
// CI は RUSTFLAGS="-D warnings" のため、dead_code を許可しないとビルドが落ちる。
#![allow(dead_code)]

// core/net は src/lib.rs (rope ライブラリクレート) が所有する。
// ここでは再宣言せず import するだけ — module tree の二重コンパイル
// (テストの二重実行含む) を避けるため。net は現行 4 動詞からは未使用
// (http 機能配下、v0.3 で結線) だが lib クレート経由で外部からは使える。
use rope::core;

use anyhow::Result;
use clap::{Parser, Subcommand};

// ============================================================================
// Apple-style capability boundary
// ============================================================================
//
// iPhone 初代はコピー無し・MMS 無し。ただし「⚠️ 未実装」ダイアログは
// 出さなかった。ボタン自体が存在しなかった。
//
// Rope v0.2 は pair/run/earn の state machine が動く。
// 実 I/O (mDNS, GPU inference, Noise) は v0.3 で結線。
// v0.2 ユーザーには「今動く部分」を正直に見せ、graceful に exit(0) する。
// crash (exit 非ゼロ) は絶対に出さない。出荷品質の最低条件。
macro_rules! capability_boundary {
    (working: $have:expr, next: $coming:expr) => {{
        println!();
        println!("✅ 動作中: {}", $have);
        println!("📋 次の版 (v0.3): {}", $coming);
        println!();
        return Ok(());
    }};
}

/// 他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる方法
#[derive(Parser)]
#[command(name = "rope")]
#[command(version)]
#[command(about = "他人のアイドル GPU を、安全に、1 ドル未満で 60 秒借りる方法")]
#[command(after_help = "\
例:
  rope                          初回体験 (60秒デモ)
  rope pair                     近くのピアを自動発見
  rope run                      デフォルトモデルで推論
  rope run mistral-7b -p 'こんにちは'
  rope earn                     GPU 貸出開始 (1 sat/秒)
  rope earn --rate 5 --max-minutes 60")]
struct Cli {
    #[command(subcommand)]
    command: Option<Verb>,
}

/// ROPE_2028 4 動詞
#[derive(Subcommand)]
enum Verb {
    /// AirDrop 式ピア発見 (mDNS / Bluetooth / QR)
    Pair {
        /// 受付モード (lan-only / contacts-only / open / off)
        #[arg(long, default_value = "lan-only")]
        accept: String,
    },

    /// 推論実行 (ローカル優先、足りなきゃピア)
    Run {
        /// モデル名または HuggingFace URL (デフォルト: llama-3.2-1b-instruct)
        #[arg(default_value = "llama-3.2-1b-instruct")]
        model: String,

        /// プロンプト (省略時は対話的に入力)
        #[arg(short, long, default_value = "Explain what Rope does in one sentence.")]
        prompt: String,

        /// 最大予算 (sats、デフォルト 1000)
        #[arg(long, default_value = "1000")]
        budget: u64,

        /// プライバシー要求 (any / on-device / tee-only)
        #[arg(long, default_value = "tee-only")]
        privacy: String,

        /// 検証レベル (none / attested / zk / full)。tee-only は attested 以上が必須
        #[arg(long, default_value = "none")]
        verification: String,
    },

    /// 自分の GPU を貸し出す
    Earn {
        /// 1 秒あたりのレート (sats、デフォルト 1)
        #[arg(long, default_value = "1")]
        rate: u64,

        /// 最大稼働時間 (分)
        #[arg(long, default_value = "120")]
        max_minutes: u32,
    },
}

fn main() {
    if let Err(e) = run() {
        // Apple 流エラー表示: アイコン + 本文 + cause chain (1 行ずつ)
        // raw anyhow trace をユーザーに見せない
        eprintln!();
        eprintln!("✗ {}", e);
        for cause in e.chain().skip(1) {
            eprintln!("  ← {}", cause);
        }
        eprintln!();
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Apple 流: 「初期化してください」と聞かない。黙ってやる。
    // 初回 rope 実行で ~/.rope を作成、鍵を生成、設定を置く。
    // 失敗しても panic しない (read-only fs 等ではメモリ内 default で続行)。
    if !core::config::is_initialized() {
        if let Err(e) = core::config::init() {
            eprintln!("⚠️  初期化スキップ ({})", e);
            // 続行: 設定 dir 無しでも state machine は動く
        }
    }

    let cli = Cli::parse();

    match cli.command {
        // 引数無し → ROPE_2028 第一動詞: 60秒 wow moment
        None => run_first_run(),
        Some(Verb::Pair { accept }) => run_pair(&accept),
        Some(Verb::Run {
            model,
            prompt,
            budget,
            privacy,
            verification,
        }) => run_inference(&model, &prompt, budget, &privacy, &verification),
        Some(Verb::Earn { rate, max_minutes }) => run_earn(rate, max_minutes),
    }
}

// ============================================================================
// rope (無引数) — first_run 60秒 wow moment
// ============================================================================

/// 初回デモで使うモデル名とプロンプト。
///
/// モデルが未配置なら用意した応答に落ちる (規範6: どちらだったかは必ず表示する)。
const FIRST_RUN_DEMO_MODEL: &str = "demo";
const FIRST_RUN_DEMO_PROMPT: &str = "Write a haiku about borrowing a stranger's GPU.";

fn run_first_run() -> Result<()> {
    use core::confidential::{load_confidential, save_confidential};
    use core::ecash::{load_ecash, save_ecash};
    use core::first_run::*;
    use core::pair::{load_pair, save_pair};

    if !should_show_first_run()? {
        println!("✅ Rope 準備完了");
        println!();
        println!("  rope pair       📡 近くのピアを発見");
        println!("  rope run MODEL  💭 推論を実行");
        println!("  rope earn       💰 GPU を貸し出す");
        return Ok(());
    }

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "you".to_string());

    let mut fr = FirstRun::new(&user);
    // Apple 流: 既存状態があれば引き継ぐ、なければ新規
    let mut pair = load_pair().unwrap_or_default();
    let mut ecash = load_ecash().unwrap_or_default();
    let mut conf = load_confidential().unwrap_or_default();

    // Welcome: icon + message on 1 line (README の 60 秒デモと完全一致)
    println!(
        "{} {}",
        fr.current_stage.icon(),
        fr.current_stage.human_message()
    );

    let mut orch = FirstRunOrchestrator {
        pair: &mut pair,
        ecash: &mut ecash,
        confidential: &mut conf,
        first_run: &mut fr,
    };

    orch.step_identity()?;
    println!("{}", orch.first_run.progress_line());

    orch.step_discovery_start()?;
    println!("{}", orch.first_run.progress_line());

    let _disc = orch.step_discovery_complete()?;
    println!("{}", orch.first_run.progress_line());

    if orch.first_run.current_stage == Stage::Aborted {
        if let Some(reason) = &orch.first_run.abort_reason {
            println!("  理由: {}", reason);
        }
        save_first_run(orch.first_run)?;
        return Ok(());
    }

    let _verdict = orch.step_attest()?;
    println!("{}", orch.first_run.progress_line());

    if orch.first_run.current_stage == Stage::Aborted {
        if let Some(reason) = &orch.first_run.abort_reason {
            println!("  理由: {}", reason);
        }
        save_first_run(orch.first_run)?;
        return Ok(());
    }

    let _intent = orch.step_build_intent()?;
    println!("{}", orch.first_run.progress_line());

    // モデルが置いてあれば**本当に推論する**。無ければ用意した応答を見せる。
    //
    // Apple 流: デモは堂々と見せる。ラベル付けない。iPhone 初代のデモは
    // pre-scripted だったが Jobs は「プレースホルダ」と言わなかった。
    // ただし Rope の規範6 は「動くフリをしない」— なので**実行できる時は
    // 必ず実行し**、できない時だけ用意した応答に落ちる。どちらだったかは
    // 下の 1 行で利用者に分かるようにする。
    let (sampled, was_real) = match run_local_inference(FIRST_RUN_DEMO_MODEL, FIRST_RUN_DEMO_PROMPT)
    {
        Ok(Some(c)) => (c.text, true),
        // モデル未配置・壊れたモデル等はデモを止める理由にならない
        Ok(None) | Err(_) => (orch.first_run.sample_haiku_response().to_string(), false),
    };
    orch.step_demo_completed(&sampled)?;
    println!("{}", orch.first_run.progress_line());

    // README の 60 秒デモと完全一致: haiku は ✨ の直後に表示
    if !sampled.is_empty() {
        println!();
        for line in sampled.lines() {
            println!("{}", line);
        }
        println!();
        if was_real {
            println!("  ↑ このテキストは今このマシンで計算されたものです。");
        } else {
            println!(
                "  ↑ これは用意された応答です ({} にモデルが未配置)。",
                model_dir().display()
            );
            println!("     `rope run` の説明どおりモデルを置くと、ここも実推論になります。");
        }
    }

    orch.step_finale()?;
    orch.step_done()?;

    // 全状態を永続化 (次の rope pair / run / earn で引き継ぐため)
    save_first_run(orch.first_run)?;
    save_pair(orch.pair)?;
    save_ecash(orch.ecash)?;
    save_confidential(orch.confidential)?;

    println!();
    println!("{}", format_welcome_letter(orch.first_run));
    Ok(())
}

// ============================================================================
// rope pair — AirDrop 式ピア発見
// ============================================================================

fn run_pair(accept_mode: &str) -> Result<()> {
    use core::pair::{format_pair, load_pair, save_pair, AcceptMode};
    use core::session::{Limits, Session, SessionState};

    let _lock = match acquire_lock_gracefully("pair") {
        Some(l) => l,
        None => return Ok(()),
    };
    let mut mgr = load_pair()?;
    mgr.config.accept_mode = match accept_mode {
        "open" => AcceptMode::Open,
        "lan-only" => AcceptMode::LanOnly,
        "contacts-only" => AcceptMode::ContactsOnly,
        "off" => AcceptMode::Off,
        _ => anyhow::bail!("不明な accept モード: {}", accept_mode),
    };

    // 孤児セッション掃除 (前回の waiting が溜まらないように)。
    // 失敗しても本動詞の本質ではないため non-fatal — crash させない。
    if let Err(e) = core::session::prune_stale() {
        eprintln!("⚠️  孤児セッション掃除スキップ ({})", e);
    }
    // 古い discovery エントリ・完了済チャレンジも同様に掃除 (v0.3 で record_discovery/
    // issue_capability_challenge が実結線されて discovered/pending_challenges が
    // 実際に増え始めたときの無限増長を防ぐ。両関数とも doc comment で「定期的に
    // 呼び出すこと」を前提としていたが、これまでどこからも呼ばれていなかった)。
    mgr.prune_stale_discoveries(60);
    mgr.prune_completed_challenges();

    // セッション開始: Idle → Waiting + 6桁 verify code
    // Apple 流 AirDrop UX: 同じ番号が両側に出れば本物
    let mut sess = Session::new(Limits::default());
    sess.transition(SessionState::Waiting)?;
    sess.generate_verify_code();
    sess.save()?;

    println!("📡 ピア発見準備完了 (受付: {})", mgr.config.accept_mode);
    if let Some(code) = &sess.verify_code {
        println!("🔢 確認コード: {}", code);
        println!("   相手側に同じ番号が表示されたら本物です。");
    }
    println!();

    // A1: 実際に LAN を探す。ここから先は本物の UDP マルチキャスト。
    let found = discover_lan_peers(&mut mgr, DISCOVERY_WINDOW)?;
    println!();

    save_pair(&mgr)?;
    println!("{}", format_pair(&mgr));

    if found == 0 {
        capability_boundary!(
            working: "実 mDNS 探索 (ピアは見つからず) + verify code + 信頼ストア",
            next: "Noise 鍵交換とジョブ転送 (発見の次段。まだ暗号化されない経路は作らない)"
        );
    }
    capability_boundary!(
        working: "実 mDNS 探索でピアを発見 + verify code + 信頼ストア",
        next: "Noise 鍵交換とジョブ転送 (発見はできたが、まだ安全に話せない)"
    );
}

/// LAN 探索に使う時間。AirDrop 的な体感を壊さない範囲で。
const DISCOVERY_WINDOW: std::time::Duration = std::time::Duration::from_millis(1500);

/// 自分の名乗りと署名器を組み立てる。
///
/// 鍵が無い (初期化に失敗した) 環境では `None` — 転送機能を諦めるだけで、
/// 他の動詞は動き続ける。
fn node_identity() -> Option<(
    rope::net::transport::NodeIdentity,
    rope::net::signing::Ed25519Signer,
)> {
    use rope::net::signing::Ed25519Signer;
    use rope::net::transport::NodeIdentity;

    let cfg: core::config::Config =
        core::config::load_or_recover(&core::config::config_path(), "config");
    let pubkey = hex::encode(core::config::load_public_key().ok()?.to_bytes());
    let signing = core::config::load_signing_key().ok()?;
    Some((
        NodeIdentity {
            node_id: cfg.node.id,
            pubkey,
        },
        Ed25519Signer::new(signing),
    ))
}

/// 相手が名乗った公開鍵から検証器を作る。
///
/// **これが証明するのは「相手はその秘密鍵を持っている」ことだけ。**
/// その鍵を信じてよいかは別問題で、v1 では**確認コードの目視照合 (TOFU)** が
/// 唯一の判断材料である (`SECURITY.md`)。ここで誰の鍵でも受けるのは、
/// 初対面を許すという TOFU の定義そのもの。
fn tofu_verifier(pubkey: &str) -> Option<Box<dyn rope::net::wire::FrameVerifier>> {
    rope::net::signing::Ed25519Verifier::from_hex(pubkey)
        .map(|v| Box::new(v) as Box<dyn rope::net::wire::FrameVerifier>)
}

/// 平文転送が無効な時に、理由と有効化方法を 1 度だけ表示する。
fn explain_plaintext_gate() {
    println!("🔒 ピア転送は無効です (プロンプトが平文で流れるため)。");
    println!("   Noise 鍵交換が未実装で、検証済みの暗号 crate をこの環境に追加できません。");
    println!("   同じ LAN を信頼できる場合のみ、次で明示的に有効化してください:");
    println!("     ROPE_ALLOW_PLAINTEXT=1 rope …");
}

/// mDNS で LAN のピアを探し、見つかった分を `PairManager` に記録する。
///
/// 返り値は**新たに記録できたピア数**。探索そのものが失敗しても
/// (マルチキャスト不可の環境等)、`rope pair` を止める理由にはしない。
fn discover_lan_peers(
    mgr: &mut core::pair::PairManager,
    window: std::time::Duration,
) -> Result<u32> {
    use core::pair::{DiscoveryMethod, PeerCapabilities};
    use rope::net::mdns::{Advertisement, Mdns};

    let cfg: core::config::Config =
        core::config::load_or_recover(&core::config::config_path(), "config");
    let pubkey = core::config::load_public_key()
        .map(|k| hex::encode(k.to_bytes()))
        .unwrap_or_default();
    if pubkey.is_empty() {
        println!("🔍 LAN 探索をスキップ (公開鍵が未生成)");
        return Ok(0);
    }

    let instance = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "rope".to_string());
    let adv = Advertisement {
        instance,
        // 待ち受けポートは Noise 実装時に確定する。今は広告するだけで listen しない。
        port: 0,
        node_id: cfg.node.id.clone(),
        fingerprint: cfg.node.fingerprint.clone(),
        pubkey,
    };

    let sock = match Mdns::open() {
        Ok(s) => s,
        Err(e) => {
            println!("🔍 LAN 探索をスキップ ({})", e);
            return Ok(0);
        }
    };
    println!("🔍 LAN を探索中… ({} ms)", window.as_millis());
    let result = match sock.discover(Some(&adv), window, &cfg.node.id) {
        Ok(r) => r,
        Err(e) => {
            println!("🔍 LAN 探索を中断 ({})", e);
            return Ok(0);
        }
    };
    if !result.responder {
        println!("   (ポート 5353 が使用中 — こちらからは探せるが、相手からは見つからない)");
    }

    let mut recorded = 0u32;
    for p in &result.peers {
        if p.pubkey.is_empty() {
            continue; // 鍵を名乗らないピアは TOFU に載せられない
        }
        let addr = match p.addr {
            Some(a) => std::net::SocketAddr::from((a, p.port)),
            None => continue,
        };
        let caps = PeerCapabilities {
            protocol_version: "mdns/1".to_string(),
            ..Default::default()
        };
        match mgr.record_discovery(&p.instance, &p.pubkey, addr, DiscoveryMethod::Mdns, caps) {
            Ok(_) => {
                recorded += 1;
                println!(
                    "   ✅ {} ({}) fp={}",
                    p.instance,
                    addr,
                    core::short(&p.fingerprint, 16)
                );
            }
            Err(e) => println!("   ⚠️  {} を記録できず ({})", p.instance, e),
        }
    }
    if result.peers.is_empty() {
        println!("   ピアは見つかりませんでした。");
    }
    Ok(recorded)
}

// ============================================================================
// rope run — 推論実行
// ============================================================================

fn run_inference(
    model: &str,
    prompt: &str,
    budget_sats: u64,
    privacy_str: &str,
    verification_str: &str,
) -> Result<()> {
    use core::confidential::load_confidential;
    use core::intent::{
        format_plan, load_intent, save_intent, BudgetEnforcement, Intent, Privacy,
        VerificationLevel, Workload,
    };
    use core::pair::load_pair;

    let privacy = match privacy_str {
        "any" => Privacy::AnyCompute,
        "on-device" => Privacy::OnDeviceOnly,
        "tee-only" => Privacy::ConfidentialCompute,
        _ => anyhow::bail!("不明な privacy: {}", privacy_str),
    };
    let verification = match verification_str {
        "none" => VerificationLevel::None,
        "attested" => VerificationLevel::Attested,
        "zk" => VerificationLevel::ZeroKnowledge,
        "full" => VerificationLevel::Full,
        _ => anyhow::bail!("不明な verification: {}", verification_str),
    };

    let user = std::env::var("USER").unwrap_or_else(|_| "you".to_string());

    let prompt_tokens = (prompt.split_whitespace().count() as f64 * 1.3) as u32;

    let intent = Intent::new(
        Workload::Inference {
            model: model.to_string(),
            prompt_tokens_est: prompt_tokens,
            max_output_tokens: 256,
        },
        &user,
    )
    .with_verification(verification)
    .with_budget(core::sats_to_usd(budget_sats), BudgetEnforcement::Hard)
    .with_privacy(privacy);

    println!("💭 Intent 構築済 ({})", core::short(&intent.id, 8));
    println!("  モデル: {}", model);
    println!("  プロンプト: {}", truncate_str(prompt, 60));
    println!("  プライバシー: {}", intent.privacy);
    println!("  検証レベル: {}", intent.verification);
    println!("  予算: {} sats", budget_sats);
    println!();

    // ROPE_2028: Intent::resolve を実行 (前は TODO だった)
    // ロックで load→submit→save を排他化 (G5: 並行 run の更新喪失を防ぐ)
    let _lock = match acquire_lock_gracefully("intent") {
        Some(l) => l,
        None => return Ok(()),
    };
    let mut mgr = load_intent()?;
    let intent_id = mgr.submit(intent)?;
    // 機密計算ルーティングの実ゲートに使う。読み込めなくても (未初期化含む)
    // None として渡し、安全側 (検証済み TEE 無し扱い) に倒す。
    let confidential = load_confidential().ok();
    // 非TEE の federation ルーティングが `rope pair` で実際にペアリング済みの
    // ピアを使うために渡す。無ければ (未ペアリング含む) 同様に安全側に倒れる。
    let pair = load_pair().ok();
    let plan = mgr.resolve(&intent_id, confidential.as_ref(), pair.as_ref())?;

    // 整形は core::intent::format_plan が単独で持つ (v1 の単純化で main.rs 側の
    // 手書き整形を削除 — 同じ ExecutionPlan を 2 箇所で整形しない)。
    print!("{}", format_plan(&plan));
    println!();

    save_intent(&mgr)?;

    // A1+A3: **まず他人に頼む** — それがこの製品の存在理由。
    // 頼めなければローカルで走らせ、どちらだったかを必ず表示する。
    if let Some(c) = try_offload_to_peer(model, prompt, budget_sats)? {
        println!("{}", c.text);
        println!();
        println!(
            "  ({} プロンプトトークン → {} 生成トークン / forward {} 回 / **ピアが計算**)",
            c.prompt_tokens, c.output_tokens, c.forward_passes
        );
        println!();
        return Ok(());
    }

    // A3: ここから先は実際に計算する。モデルが無ければ「無い」と言う (規範6)。
    match run_local_inference(model, prompt)? {
        Some(c) => {
            println!("{}", c.text);
            println!();
            println!(
                "  ({} プロンプトトークン → {} 生成トークン / forward {} 回 / 停止: {:?})",
                c.prompt_tokens, c.output_tokens, c.forward_passes, c.stop_reason
            );
            println!();
            Ok(())
        }
        None => {
            println!(
                "ℹ️  ローカルモデルが見つかりません: {}",
                model_dir().display()
            );
            println!(
                "   `{}.bin` と `tokenizer.bin` (llama2.c 形式) を置くと、",
                model
            );
            println!("   この 4 動詞の中で実際に推論が走ります。");
            capability_boundary!(
                working: "Intent → ExecutionPlan → ローカル推論 (モデル未配置のため未実行)",
                next: "ピアへのオフロード (mDNS + Noise + ecash 結線)"
            );
        }
    }
}

/// 発見済みピアに推論を頼んでみる。
///
/// **これが成功して初めて「他人の GPU を借りた」ことになる。**
/// 頼める相手が居ない・平文が許可されていない・全員失敗した場合は `Ok(None)` で、
/// 呼び出し元はローカル実行へ落ちる。
///
/// ⚠️ 現状この経路は**暗号化されていない**。`ROPE_ALLOW_PLAINTEXT=1` が
/// 無ければ何もせず `Ok(None)` を返す。
fn try_offload_to_peer(
    model: &str,
    prompt: &str,
    budget_sats: u64,
) -> Result<Option<rope::net::transport::Executed>> {
    use rope::net::transport::{plaintext_allowed, request_job};

    if !plaintext_allowed() {
        return Ok(None);
    }
    let (me, signer) = match node_identity() {
        Some(v) => v,
        None => return Ok(None),
    };
    let mgr = match core::pair::load_pair() {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    // ポート 0 を広告しているピア (待受していない) は飛ばす
    let candidates: Vec<_> = mgr
        .discovered
        .iter()
        .filter(|p| p.endpoint.port() != 0 && !p.advertised_pubkey.is_empty())
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }

    let job_id = uuid_like_id();
    for peer in candidates {
        println!("📡 {} に推論を依頼中…", peer.endpoint);
        let mut stream = match std::net::TcpStream::connect_timeout(
            &peer.endpoint,
            std::time::Duration::from_secs(5),
        ) {
            Ok(s) => s,
            Err(e) => {
                println!("   繋がりませんでした ({})", e);
                continue;
            }
        };
        match request_job(
            &mut stream,
            &me,
            &signer,
            &tofu_verifier,
            &job_id,
            model,
            prompt,
            256,
            budget_sats,
        ) {
            Ok(ex) => return Ok(Some(ex)),
            Err(e) => println!("   断られました ({})", e),
        }
    }
    Ok(None)
}

/// ジョブ ID。`uuid` crate を使わないのは、ここでは一意でありさえすればよく、
/// `core` の永続 ID とは用途が違うため。
fn uuid_like_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("job-{:x}-{:x}", now, std::process::id())
}

/// 貸し手がモデルを置く場所。**借り手はここに影響できない** (A9)。
///
/// `ROPE_MODEL_DIR` で上書きできるが、これは貸し手の環境変数であって
/// 借り手が送れる値ではない。
fn model_dir() -> std::path::PathBuf {
    match std::env::var_os("ROPE_MODEL_DIR") {
        Some(d) => std::path::PathBuf::from(d),
        None => core::config::config_dir().join("models"),
    }
}

/// ローカルモデルで推論を実行する。
///
/// モデルが配置されていなければ `Ok(None)` — **失敗ではなく「まだ無い」**。
/// 壊れたモデルや上限超過は `Err` で、理由をそのまま利用者に見せる。
fn run_local_inference(
    model: &str,
    prompt: &str,
) -> Result<Option<rope::net::inference::Completion>> {
    run_local_inference_limited(model, prompt, 256)
}

/// 生成トークン数の上限を指定してローカル推論を実行する。
///
/// 貸し手として他人のジョブを走らせる時は、相手の希望値をそのまま信じず
/// 自分の上限で頭打ちにする (A9)。
fn run_local_inference_limited(
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
) -> Result<Option<rope::net::inference::Completion>> {
    use rope::net::inference::{
        safe_model_stem, CheckpointLimits, CpuEngine, ExecutionLimits, InferenceEngine, Sampler,
    };

    // モデル名は外から来た文字列として扱う (パス結合の前に必ず無害化する)
    let stem = safe_model_stem(model).map_err(|e| anyhow::anyhow!("{}", e))?;
    let dir = model_dir();
    let model_path = dir.join(format!("{}.bin", stem));
    let tokenizer_path = dir.join("tokenizer.bin");
    if !model_path.exists() || !tokenizer_path.exists() {
        return Ok(None);
    }

    println!("🧮 ローカル推論を実行中 ({})…", model_path.display());
    let engine = CpuEngine::load(&model_path, &tokenizer_path, CheckpointLimits::default())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let limits = ExecutionLimits {
        max_prompt_tokens: 2048,
        max_output_tokens,
    };
    // seed は固定。同じプロンプトで同じ出力になる方が、この段階では検証しやすい。
    let completion = engine
        .generate(prompt, limits, Sampler::default(), 0x526F7065)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(Some(completion))
}

// ============================================================================
// rope earn — GPU 貸出
// ============================================================================

fn run_earn(rate_sats_per_sec: u64, max_minutes: u32) -> Result<()> {
    use core::ecash::{format_ecash, load_ecash};
    use core::pair::{load_pair, save_pair, AcceptMode};
    use core::session::{format_session, Limits, Session, SessionState};

    let _lock = match acquire_lock_gracefully("pair") {
        Some(l) => l,
        None => return Ok(()),
    };
    let mut mgr = load_pair()?;
    if mgr.config.accept_mode == AcceptMode::Off {
        mgr.config.accept_mode = AcceptMode::LanOnly;
    }
    // 古い discovery エントリ・完了済チャレンジを掃除 (run_pair と同じ理由 — 詳細はそちら参照)。
    mgr.prune_stale_discoveries(60);
    mgr.prune_completed_challenges();
    save_pair(&mgr)?;

    // 孤児セッション掃除 (前回の waiting が溜まらないように)。
    // 失敗しても本動詞の本質ではないため non-fatal — crash させない。
    if let Err(e) = core::session::prune_stale() {
        eprintln!("⚠️  孤児セッション掃除スキップ ({})", e);
    }

    // 貸出セッション = Waiting 状態を永続化
    let limits = Limits {
        max_time_minutes: max_minutes,
        ..Default::default()
    };
    let mut sess = Session::new(limits);
    sess.transition(SessionState::Waiting)?;
    sess.generate_verify_code();
    sess.save()?;

    println!("💰 GPU 貸出モード");
    println!("  レート: {} sats/秒", rate_sats_per_sec);
    println!("  受付モード: {}", mgr.config.accept_mode);
    println!();

    // セッションの ID / 状態 / 確認コード / 上限は core::session::format_session が
    // 単独で持つ (v1 の単純化で main.rs 側の手書き整形を削除 — pair/run と同じ扱い)。
    print!("{}", format_session(&sess));
    println!();

    // ecash 状態表示 (Apple 流: 関連情報を先に見せる)。
    // 空ウォレットでゼロだらけのダッシュボードを出さないため、残高がある時だけ。
    let ecash = load_ecash()?;
    if ecash.wallet.total_sats > 0 {
        print!("{}", format_ecash(&ecash));
        println!();
    }

    // A1+A3: 実際にジョブを受ける。平文が許可されていなければ理由を出して終わる。
    if !rope::net::transport::plaintext_allowed() {
        explain_plaintext_gate();
        capability_boundary!(
            working: "Session::Waiting + verify code + ecash 残高 + 実 mDNS 広告",
            next: "Noise 鍵交換 (暗号化された転送)"
        );
    }
    serve_jobs(max_minutes)?;
    Ok(())
}

/// 貸し手のループ本体: TCP で待ち受け、mDNS で自分を広告し、来たジョブを実行する。
///
/// **1 接続ずつ順に処理する。** 並行実行は貸し手の資源を予測不能にするので、
/// v1 では素直に直列にする (A9)。
fn serve_jobs(max_minutes: u32) -> Result<()> {
    use rope::net::mdns::{Advertisement, Mdns};
    use rope::net::transport::{serve_connection, Executed, JobPolicy};

    let (me, signer) = match node_identity() {
        Some(v) => v,
        None => {
            println!("⚠️  鍵が読めないため貸出を開始できません。");
            return Ok(());
        }
    };

    let listener = std::net::TcpListener::bind(("0.0.0.0", 0))?;
    let port = listener.local_addr()?.port();

    let cfg: core::config::Config =
        core::config::load_or_recover(&core::config::config_path(), "config");
    let adv = Advertisement {
        instance: std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "rope".to_string()),
        port,
        node_id: me.node_id.clone(),
        fingerprint: cfg.node.fingerprint.clone(),
        pubkey: me.pubkey.clone(),
    };
    // 1 回告知しておく。以後は問い合わせに答える形で見つけてもらう。
    if let Ok(m) = Mdns::open() {
        let _ = m.announce(&adv);
    }

    println!("🛰  ジョブ待受中: ポート {} (最大 {} 分)", port, max_minutes);
    println!("   ⚠️  この経路は暗号化されていません。プロンプトは LAN 上で平文です。");
    println!("   停止は Ctrl-C。");
    println!();

    struct LocalPolicy {
        max_output_tokens: u32,
    }
    impl JobPolicy for LocalPolicy {
        fn accept(
            &self,
            _peer_pubkey: &str,
            model: &str,
            _prompt: &str,
            budget_sats: u64,
        ) -> Result<(), String> {
            // モデル名は外から来る文字列 — パスにする前に必ず無害化する (A9)
            rope::net::inference::safe_model_stem(model).map_err(|e| e.to_string())?;
            if budget_sats == 0 {
                return Err("予算が 0 sats".to_string());
            }
            Ok(())
        }

        fn execute(
            &self,
            model: &str,
            prompt: &str,
            max_output_tokens: u32,
        ) -> Result<Executed, String> {
            // 相手の希望値をそのまま信じず、自分の上限で頭打ちにする (A9)
            let capped = max_output_tokens.min(self.max_output_tokens);
            match run_local_inference_limited(model, prompt, capped) {
                Ok(Some(c)) => Ok(Executed {
                    text: c.text,
                    prompt_tokens: c.prompt_tokens,
                    output_tokens: c.output_tokens,
                    forward_passes: c.forward_passes,
                }),
                Ok(None) => Err(format!("モデル {} を持っていません", model)),
                Err(e) => Err(e.to_string()),
            }
        }
    }

    let policy = LocalPolicy {
        max_output_tokens: 256,
    };
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(max_minutes as u64 * 60);

    for incoming in listener.incoming() {
        if std::time::Instant::now() >= deadline {
            println!("⏱  最大稼働時間に達しました。");
            break;
        }
        let mut stream = match incoming {
            Ok(s) => s,
            Err(e) => {
                eprintln!("⚠️  接続を受けられませんでした ({})", e);
                continue;
            }
        };
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "?".to_string());
        // 1 接続の失敗で貸出全体を止めない
        match serve_connection(&mut stream, &me, &signer, &tofu_verifier, &policy) {
            Ok(job) if job.accepted => {
                let ex = job.executed.unwrap_or(Executed {
                    text: String::new(),
                    prompt_tokens: 0,
                    output_tokens: 0,
                    forward_passes: 0,
                });
                println!(
                    "✅ {} からのジョブを実行 ({} tok 出力 / forward {} 回)",
                    peer, ex.output_tokens, ex.forward_passes
                );
            }
            Ok(job) => println!("🚫 {} のジョブを謝絶 (job {})", peer, job.job_id),
            Err(e) => println!("⚠️  {} との通信に失敗 ({})", peer, e),
        }
    }
    Ok(())
}

/// プロセス間ロックを取得。取得できなければ (別 rope プロセスが実行中等)
/// crash させず友好的なメッセージを出して `None` を返す。
/// 呼び出し元は `None` の場合 `Ok(())` で graceful に exit(0) する
/// (「crash (exit 非ゼロ) は絶対に出さない」という capability_boundary の方針を
/// ロック競合時にも一貫させる — 旧実装は `?` で伝播し exit(1) していた)。
fn acquire_lock_gracefully(name: &str) -> Option<core::config::LockGuard> {
    match core::config::LockGuard::acquire(name) {
        Ok(lock) => Some(lock),
        Err(e) => {
            println!();
            println!("⚠️  別の rope プロセスが実行中の可能性があります ({})", e);
            println!("   完了を待つか、そちらのプロセスを終了してから再実行してください。");
            println!();
            None
        }
    }
}

/// 文字列を指定文字数で切り詰め (ユーザー表示用)
fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

// ============================================================================
// main.rs integration tests
// ============================================================================
//
// Apple 的出荷前 smoke test: 4 動詞 + ヘルパーの基本動作確認。
// 各 core モジュールの単体テスト (153 件) を補完する統合レイヤ。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let t = truncate_str("hello world", 5);
        assert!(t.ends_with('…'));
        assert_eq!(t.chars().count(), 6); // 5 + ellipsis
    }

    #[test]
    fn test_truncate_str_unicode() {
        // 日本語は 1 文字 = 1 char (chars count ベース)
        let t = truncate_str("こんにちは世界", 3);
        assert!(t.starts_with("こんに"));
        assert!(t.ends_with('…'));
    }

    #[test]
    fn test_cli_no_args_parses() {
        // 引数無し → None (first_run)
        let cli = Cli::try_parse_from(["rope"]);
        assert!(cli.is_ok());
        assert!(cli.unwrap().command.is_none());
    }

    #[test]
    fn test_cli_pair_parses() {
        let cli = Cli::try_parse_from(["rope", "pair"]).unwrap();
        assert!(matches!(cli.command, Some(Verb::Pair { .. })));
    }

    #[test]
    fn test_cli_pair_with_accept_parses() {
        let cli = Cli::try_parse_from(["rope", "pair", "--accept", "open"]).unwrap();
        match cli.command {
            Some(Verb::Pair { accept }) => assert_eq!(accept, "open"),
            _ => panic!("Expected Pair"),
        }
    }

    #[test]
    fn test_cli_run_defaults() {
        let cli = Cli::try_parse_from(["rope", "run"]).unwrap();
        match cli.command {
            Some(Verb::Run {
                model,
                prompt,
                budget,
                privacy,
                verification,
            }) => {
                assert_eq!(model, "llama-3.2-1b-instruct");
                assert!(!prompt.is_empty());
                assert_eq!(budget, 1000);
                assert_eq!(privacy, "tee-only");
                assert_eq!(verification, "none");
            }
            _ => panic!("Expected Run"),
        }
    }

    #[test]
    fn test_cli_run_custom() {
        let cli = Cli::try_parse_from([
            "rope",
            "run",
            "mistral-7b",
            "-p",
            "Say hello",
            "--budget",
            "500",
            "--privacy",
            "any",
            "--verification",
            "attested",
        ])
        .unwrap();
        match cli.command {
            Some(Verb::Run {
                model,
                prompt,
                budget,
                privacy,
                verification,
            }) => {
                assert_eq!(model, "mistral-7b");
                assert_eq!(prompt, "Say hello");
                assert_eq!(budget, 500);
                assert_eq!(privacy, "any");
                assert_eq!(verification, "attested");
            }
            _ => panic!("Expected Run"),
        }
    }

    #[test]
    fn test_cli_earn_defaults() {
        let cli = Cli::try_parse_from(["rope", "earn"]).unwrap();
        match cli.command {
            Some(Verb::Earn { rate, max_minutes }) => {
                assert_eq!(rate, 1);
                assert_eq!(max_minutes, 120);
            }
            _ => panic!("Expected Earn"),
        }
    }

    #[test]
    fn test_cli_earn_custom() {
        let cli =
            Cli::try_parse_from(["rope", "earn", "--rate", "5", "--max-minutes", "30"]).unwrap();
        match cli.command {
            Some(Verb::Earn { rate, max_minutes }) => {
                assert_eq!(rate, 5);
                assert_eq!(max_minutes, 30);
            }
            _ => panic!("Expected Earn"),
        }
    }

    #[test]
    fn test_cli_unknown_command_is_error() {
        assert!(Cli::try_parse_from(["rope", "destroy"]).is_err());
    }

    #[test]
    fn test_cli_help_does_not_panic() {
        // --help causes clap to exit, but try_parse should handle it
        let result = Cli::try_parse_from(["rope", "--help"]);
        // clap returns Err for --help (it's a "success" error)
        assert!(result.is_err());
    }
}
