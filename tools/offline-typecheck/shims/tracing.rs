//! `tracing` の型検査専用スタブ。実 tracing ではない (`README.md` 参照)。
//!
//! 引数は `format_args!` に通して型検査だけ行い、出力はしない
//! (引数側の型エラー・未定義変数はここで検出される)。

#[macro_export]
macro_rules! trace { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }
#[macro_export]
macro_rules! debug { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }
#[macro_export]
macro_rules! info  { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }
#[macro_export]
macro_rules! warn  { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }
#[macro_export]
macro_rules! error { ($($arg:tt)*) => {{ let _ = format_args!($($arg)*); }}; }
