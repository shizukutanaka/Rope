//! `clap` の型検査専用スタブ。実 clap ではない (`README.md` 参照)。
//!
//! ⚠️ `parse()` 系は `unimplemented!()` — 型は通るが**実行はできない**。

pub use clap_derive_shim::{Args, Parser, Subcommand, ValueEnum};

use std::ffi::OsString;

#[derive(Debug)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("clap error")
    }
}

impl std::error::Error for Error {}

pub trait Parser: Sized {
    fn parse() -> Self {
        unimplemented!("型検査専用スタブ — 実行はできない")
    }
    fn try_parse() -> Result<Self, Error> {
        unimplemented!("型検査専用スタブ — 実行はできない")
    }
    fn parse_from<I, T>(_itr: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        unimplemented!("型検査専用スタブ — 実行はできない")
    }
    fn try_parse_from<I, T>(_itr: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        unimplemented!("型検査専用スタブ — 実行はできない")
    }
}

pub trait Subcommand: Sized {}
pub trait Args: Sized {}
pub trait ValueEnum: Sized {}
