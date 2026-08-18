//! `serde_json` の型検査専用スタブ。実 serde_json ではない (`README.md` 参照)。
//!
//! ⚠️ デシリアライズ系は `unimplemented!()` を返す — 型は通るが**実行はできない**。
//! このハーネスが `cargo check` 相当であって `cargo test` 相当ではない理由がこれ。

use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("json error")
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn to_string<T: Serialize + ?Sized>(_v: &T) -> Result<String> {
    Ok(String::new())
}

pub fn to_string_pretty<T: Serialize + ?Sized>(_v: &T) -> Result<String> {
    Ok(String::new())
}

pub fn to_vec<T: Serialize + ?Sized>(_v: &T) -> Result<Vec<u8>> {
    Ok(Vec::new())
}

pub fn from_str<T: DeserializeOwned>(_s: &str) -> Result<T> {
    unimplemented!("型検査専用スタブ — 実行はできない")
}

pub fn from_slice<T: DeserializeOwned>(_s: &[u8]) -> Result<T> {
    unimplemented!("型検査専用スタブ — 実行はできない")
}

#[derive(Clone, Debug, Default)]
pub struct Value;

impl serde::Serialize for Value {}
impl<'de> serde::Deserialize<'de> for Value {}

/// ⚠️ **本物と最も乖離している箇所**: `json!` の中身のトークンは
/// 型検査されない (任意のトークン列を受け取って `Value` を返すだけ)。
/// `json!` 内の式の型エラー・未定義変数は**このハーネスでは検出できない**。
#[macro_export]
macro_rules! json {
    ($($tt:tt)*) => {
        $crate::Value
    };
}
