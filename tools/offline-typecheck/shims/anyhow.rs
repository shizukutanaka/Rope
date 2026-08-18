//! `anyhow` の型検査専用スタブ。実 anyhow ではない (`README.md` 参照)。

use std::fmt;

pub struct Error {
    msg: String,
}

impl Error {
    pub fn msg<M: fmt::Display>(m: M) -> Error {
        Error {
            msg: m.to_string(),
        }
    }
    pub fn chain(&self) -> Chain<'_> {
        Chain {
            _p: std::marker::PhantomData,
        }
    }
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        unimplemented!("型検査専用スタブ — 実行はできない")
    }
}

/// `Error::chain()` が返す反復子 (本物は原因チェーンを辿る)。
pub struct Chain<'a> {
    _p: std::marker::PhantomData<&'a ()>,
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn std::error::Error + 'static);
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

// 本物の anyhow と同じく `std::error::Error` は **実装しない**。
// 実装すると下の blanket From が標準の反射的 `From<T> for T` と衝突する。
impl<E: std::error::Error + Send + Sync + 'static> From<E> for Error {
    fn from(e: E) -> Error {
        Error { msg: e.to_string() }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait Context<T, E> {
    fn context<C: fmt::Display + Send + Sync + 'static>(self, c: C) -> Result<T, Error>;
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E: std::error::Error + Send + Sync + 'static> Context<T, E> for std::result::Result<T, E> {
    fn context<C: fmt::Display + Send + Sync + 'static>(self, c: C) -> Result<T, Error> {
        self.map_err(|_| Error::msg(c))
    }
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|_| Error::msg(f()))
    }
}

impl<T> Context<T, Error> for std::result::Result<T, Error> {
    fn context<C: fmt::Display + Send + Sync + 'static>(self, c: C) -> Result<T, Error> {
        self.map_err(|_| Error::msg(c))
    }
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|_| Error::msg(f()))
    }
}

impl<T> Context<T, std::convert::Infallible> for Option<T> {
    fn context<C: fmt::Display + Send + Sync + 'static>(self, c: C) -> Result<T, Error> {
        self.ok_or_else(|| Error::msg(c))
    }
    fn with_context<C, F>(self, f: F) -> Result<T, Error>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| Error::msg(f()))
    }
}

#[macro_export]
macro_rules! anyhow {
    ($($arg:tt)*) => { $crate::Error::msg(format!($($arg)*)) };
}

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::Error::msg(format!($($arg)*))) };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond { return Err($crate::Error::msg(format!($($arg)*))); }
    };
}
