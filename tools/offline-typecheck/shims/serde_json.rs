//! `serde_json` の JSON 専用ミニ実装 (`serde.rs` の `Json` を使う)。
//!
//! **本物と違い、フォーマット非依存の抽象を持たない。**
//! バイト列が実 `serde_json` と一致する保証も無い (`serde.rs` の注意を参照)。
//! 目的は `core/` のテストをこの環境で実際に走らせること。

use serde::de::DeserializeOwned;
use serde::{Json, Serialize};

#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "json error: {}", self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn to_string<T: Serialize + ?Sized>(v: &T) -> Result<String> {
    Ok(v.to_json().to_json_string(false))
}

pub fn to_string_pretty<T: Serialize + ?Sized>(v: &T) -> Result<String> {
    Ok(v.to_json().to_json_string(true))
}

pub fn to_vec<T: Serialize + ?Sized>(v: &T) -> Result<Vec<u8>> {
    Ok(to_string(v)?.into_bytes())
}

pub fn from_str<T: DeserializeOwned>(s: &str) -> Result<T> {
    let v = Json::parse(s).map_err(Error)?;
    <T as DeserializeOwned>::from_json(&v).map_err(Error)
}

pub fn from_slice<T: DeserializeOwned>(b: &[u8]) -> Result<T> {
    let s = std::str::from_utf8(b).map_err(|e| Error(e.to_string()))?;
    from_str(s)
}

/// `serde_json::Value` 相当。中身は `serde::Json` そのもの。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Value(pub Json);

impl serde::Serialize for Value {
    fn to_json(&self) -> Json {
        self.0.clone()
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn from_json(v: &Json) -> std::result::Result<Self, String> {
        Ok(Value(v.clone()))
    }
}

/// ⚠️ **`json!` の中身は型検査されない。** 任意のトークン列を受け取って
/// 空の `Value` を返すだけ。`json!` 内の式の誤りはこのスタブでは分からない。
#[macro_export]
macro_rules! json {
    ($($tt:tt)*) => {
        $crate::Value::default()
    };
}
