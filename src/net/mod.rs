//! net — 外部 I/O 層
//!
//! Round 18 STATUS が宣言した「core はステートマシンのみ、I/O は外」
//! 原則の物理実装。core/* は pure state、本層は実 HTTP / 暗号 / wire 形式。
//!
//! Apple 流分割設計: AppKit と Foundation が分かれているのと同じ。
//! UI ロジックは Foundation に出さない。逆も然り。
//!
//! ## モジュール
//! - `cashu_mint` — Cashu protocol HTTP クライアント
//! - `inference`  — A3: 実際に推論を実行する層 (依存ゼロ・CPU)
//! - `mdns`       — A1: LAN のピアを実際に見つける層 (依存ゼロ・UDP マルチキャスト)
//! - `wire`       — ノード間フレーム形式と署名の論理 (依存ゼロ)
//! - `transport`  — ジョブ転送 (TCP)。⚠️ 暗号化は無い — 既定で無効
//! - `signing`    — `wire` の署名トレイトを実 ed25519 に繋ぐ唯一の場所

pub mod cashu_mint;
pub mod inference;
pub mod mdns;
pub mod signing;
pub mod transport;
pub mod wire;
