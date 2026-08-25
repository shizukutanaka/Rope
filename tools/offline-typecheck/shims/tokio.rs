//! `tokio` の型検査専用スタブ。
//!
//! Rope のコードは現状 **`tokio` を直接参照していない** (`http` feature が
//! `reqwest` を引くために依存に入っているだけ)。将来 async 化する時に
//! 使いそうな面だけ最小限置いてある。
//!
//! 🔴 **ランタイムではない。** `spawn` は何も実行しない。

pub mod sync {
    /// `std::sync::RwLock` をそのまま使う。**非同期ではない。**
    pub type RwLock<T> = std::sync::RwLock<T>;
    pub type Mutex<T> = std::sync::Mutex<T>;
}

pub mod time {
    pub use std::time::Duration;

    /// 🔴 実際には待たない。
    pub async fn sleep(_d: Duration) {}
}

pub mod task {
    /// 🔴 何も起動しない。
    pub fn spawn<F: std::future::Future>(_f: F) {}
}
