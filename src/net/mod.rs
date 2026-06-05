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

pub mod cashu_mint;
