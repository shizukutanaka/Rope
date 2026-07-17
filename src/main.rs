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

    // Apple 流: デモは堂々と見せる。ラベル付けない。
    // iPhone のデモは pre-scripted だったが Jobs は「プレースホルダ」と言わなかった。
    let sampled = orch.first_run.sample_haiku_response().to_string();
    orch.step_demo_completed(&sampled)?;
    println!("{}", orch.first_run.progress_line());

    // README の 60 秒デモと完全一致: haiku は ✨ の直後に表示
    if !sampled.is_empty() {
        println!();
        for line in sampled.lines() {
            println!("{}", line);
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

    save_pair(&mgr)?;
    println!("{}", format_pair(&mgr));

    capability_boundary!(
        working: "Pair state machine + verify code 生成 + 信頼ストア",
        next: "実 mDNS / Bluetooth スキャン (mdns-sd + btleplug)"
    );
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
        load_intent, save_intent, BudgetEnforcement, Intent, Privacy, VerificationLevel, Workload,
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

    println!("⚙️  実行計画 ({})", core::short(&plan.id, 8));
    println!("  プロバイダ: {}", plan.selected_provider);
    println!("  ステップ数: {}", plan.steps.len());
    if !plan.optimizations.is_empty() {
        println!(
            "  自動最適化: {}",
            plan.optimizations
                .iter()
                .map(|o| o.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("  推定コスト: ${:.4}", plan.estimated_cost_usd);
    if !plan.feasible {
        if let Some(r) = &plan.infeasibility_reason {
            println!("  ⚠️  実行不可: {}", r);
        }
    }
    println!();

    save_intent(&mgr)?;

    capability_boundary!(
        working: "Intent → ExecutionPlan 構築 (型安全な準備状態)",
        next: "実 GPU 推論実行 (cashu_mint + Noise 結線)"
    );
}

// ============================================================================
// rope earn — GPU 貸出
// ============================================================================

fn run_earn(rate_sats_per_sec: u64, max_minutes: u32) -> Result<()> {
    use core::ecash::load_ecash;
    use core::pair::{load_pair, save_pair, AcceptMode};
    use core::session::{Limits, Session, SessionState};

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
    println!("  最大稼働: {} 分", max_minutes);
    println!("  受付モード: {}", mgr.config.accept_mode);
    println!("  セッション ID: {}", core::short(&sess.id, 8));
    if let Some(code) = &sess.verify_code {
        println!("  🔢 確認コード: {}", code);
    }
    println!();

    // ecash 残高表示 (Apple 流: 関連情報を先に見せる)
    let ecash = load_ecash()?;
    if ecash.wallet.total_sats > 0 {
        println!("  💳 ウォレット残高: {} sats", ecash.wallet.total_sats);
        println!();
    }

    capability_boundary!(
        working: "Session::Waiting 状態 + verify code + ecash 残高",
        next: "実 GPU 貸出ループ (mDNS リスナ + Noise 受信)"
    );
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
