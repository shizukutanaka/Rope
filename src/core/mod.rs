//! Rope Core — ROPE_2028 7 モジュール構成
//!
//! 状態 (2026-04-18): 122 → 7 モジュール (-94%)
//! 全モジュール配線済 (dead code ゼロ)
//!
//! ## 4 動詞 (main.rs) と core モジュール対応
//!
//! ```text
//! rope            (無引数)  → first_run + pair + ecash + confidential + intent
//! rope pair                 → pair
//! rope run <model>          → intent + confidential
//! rope earn                 → pair + ecash
//! ```
//!
//! ## 7 モジュールの役割
//!
//! - `config`        — 鍵生成、設定パス、storage 基盤 (27 deps)
//! - `pair`          — mDNS+Noise 発見、TOFU 信頼ストア (10 deps)
//! - `confidential`  — GPU TEE attestation、LANDMINES 2 対応 (2 deps)
//! - `ecash`         — Cashu bearer token + escrow + streaming (2 deps)
//! - `intent`        — Workload/Privacy/Budget の上位抽象 (2 deps)
//! - `session`       — 1:1 GPU 貸借セッション状態 (2 deps)
//! - `first_run`     — 60秒 wow moment オーケストレータ (1 dep)
//!
//! ## 削除されたもの (RETIREMENT_MANIFEST 完了)
//!
//! 「とりあえず後で使うかも」の死蔵在庫を含む 115 モジュールを削除。
//! Apple 流: 約束されてない機能のためにスロットを残さない。
//! 必要になったら lib から import すればよい。

pub mod confidential;
pub mod config;
pub mod ecash;
pub mod first_run;
pub mod intent;
pub mod pair;
pub mod session;
