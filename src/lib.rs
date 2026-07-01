//! rope — core state machine + I/O translation layer, as a library.
//!
//! `core/` is a pure state machine (no I/O); `net/` is the HTTP/wire-format
//! translation layer (opt-in via the `http` feature). The `rope` binary
//! (`src/main.rs`) is a thin CLI wrapper around these modules — everything
//! here is also usable directly by external code or tests, without going
//! through the CLI. See `examples/library_usage.rs` for a runnable example.

// core/ の state machine は完全な API を公開するが、現行の 4 動詞からは
// 一部のみ到達する。残りは v0.3 (実 I/O 結線) で使う予約 API。
// 削除すると state machine の完全性が崩れるため allow で明示保持。
#![allow(dead_code)]

pub mod core;
pub mod net;
