//! `serde` の型検査 **兼 実行** 用スタブ。
//!
//! ## 本物との違い (これを読まずに使わないこと)
//!
//! 実 serde はフォーマット非依存の `Serializer`/`Deserializer` 抽象を持つが、
//! ここは **JSON 専用のミニ実装**である。目的はただ一つ:
//! **`core/` のテストをこの環境で実際に走らせること**。
//!
//! 🔴 **バイト列が実 `serde_json` と一致する保証は無い。**
//! 特に `chrono::DateTime` は実 serde が RFC3339 文字列で書くのに対し、
//! ここは**エポックからのナノ秒 (数値)** で書く。したがって round-trip
//! テストが通っても、それは**このスタブを通した往復**が正しいという意味で
//! あって、実 serde_json が書いたファイルと互換だという意味ではない。
//! 実ファイル互換の検証は CI でしかできない。
//!
//! 対応している serde 属性は、このリポジトリが実際に使う 6 形だけ:
//! `rename_all = "snake_case" | "lowercase"` / `rename = "..."` /
//! `default` / `tag = "..."`。

pub use serde_derive_shim::{Deserialize, Serialize};

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::Hash;

// ============================================================================
// JSON 値
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Default for Json {
    fn default() -> Self {
        Json::Null
    }
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        let pad = |o: &mut String, d: usize| {
            if let Some(w) = indent {
                o.push('\n');
                for _ in 0..(w * d) {
                    o.push(' ');
                }
            }
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Json::Str(s) => write_str(out, s),
            Json::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, depth + 1);
                    v.write(out, indent, depth + 1);
                }
                pad(out, depth);
                out.push(']');
            }
            Json::Obj(kv) => {
                if kv.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in kv.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    pad(out, depth + 1);
                    write_str(out, k);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    v.write(out, indent, depth + 1);
                }
                pad(out, depth);
                out.push('}');
            }
        }
    }

    pub fn to_json_string(&self, pretty: bool) -> String {
        let mut s = String::new();
        self.write(&mut s, if pretty { Some(2) } else { None }, 0);
        s
    }

    pub fn parse(src: &str) -> Result<Json, String> {
        let b = src.as_bytes();
        let mut i = 0usize;
        let v = parse_value(b, &mut i)?;
        skip_ws(b, &mut i);
        if i != b.len() {
            return Err(format!("末尾に余分な入力 (offset {})", i));
        }
        Ok(v)
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn skip_ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && (b[*i] as char).is_ascii_whitespace() {
        *i += 1;
    }
}

fn parse_value(b: &[u8], i: &mut usize) -> Result<Json, String> {
    skip_ws(b, i);
    match b.get(*i) {
        None => Err("入力が空".to_string()),
        Some(b'n') => lit(b, i, "null", Json::Null),
        Some(b't') => lit(b, i, "true", Json::Bool(true)),
        Some(b'f') => lit(b, i, "false", Json::Bool(false)),
        Some(b'"') => parse_string(b, i).map(Json::Str),
        Some(b'[') => {
            *i += 1;
            let mut items = Vec::new();
            skip_ws(b, i);
            if b.get(*i) == Some(&b']') {
                *i += 1;
                return Ok(Json::Arr(items));
            }
            loop {
                items.push(parse_value(b, i)?);
                skip_ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b']') => {
                        *i += 1;
                        return Ok(Json::Arr(items));
                    }
                    _ => return Err(format!("配列が閉じない (offset {})", i)),
                }
            }
        }
        Some(b'{') => {
            *i += 1;
            let mut kv = Vec::new();
            skip_ws(b, i);
            if b.get(*i) == Some(&b'}') {
                *i += 1;
                return Ok(Json::Obj(kv));
            }
            loop {
                skip_ws(b, i);
                let k = parse_string(b, i)?;
                skip_ws(b, i);
                if b.get(*i) != Some(&b':') {
                    return Err(format!("':' が無い (offset {})", i));
                }
                *i += 1;
                kv.push((k, parse_value(b, i)?));
                skip_ws(b, i);
                match b.get(*i) {
                    Some(b',') => *i += 1,
                    Some(b'}') => {
                        *i += 1;
                        return Ok(Json::Obj(kv));
                    }
                    _ => return Err(format!("オブジェクトが閉じない (offset {})", i)),
                }
            }
        }
        Some(_) => parse_number(b, i),
    }
}

fn lit(b: &[u8], i: &mut usize, word: &str, v: Json) -> Result<Json, String> {
    if b[*i..].starts_with(word.as_bytes()) {
        *i += word.len();
        Ok(v)
    } else {
        Err(format!("不正なリテラル (offset {})", i))
    }
}

fn parse_string(b: &[u8], i: &mut usize) -> Result<String, String> {
    if b.get(*i) != Some(&b'"') {
        return Err(format!("文字列でない (offset {})", i));
    }
    *i += 1;
    let mut out = String::new();
    while let Some(&c) = b.get(*i) {
        *i += 1;
        match c {
            b'"' => return Ok(out),
            b'\\' => {
                let e = *b.get(*i).ok_or("エスケープが途中")?;
                *i += 1;
                match e {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'u' => {
                        let hex = b.get(*i..*i + 4).ok_or("\\u が途中")?;
                        let n = u32::from_str_radix(
                            std::str::from_utf8(hex).map_err(|_| "不正な \\u")?,
                            16,
                        )
                        .map_err(|_| "不正な \\u")?;
                        *i += 4;
                        out.push(char::from_u32(n).unwrap_or('\u{fffd}'));
                    }
                    _ => return Err("未知のエスケープ".to_string()),
                }
            }
            _ => {
                // UTF-8 の続きバイトをそのまま拾う
                let start = *i - 1;
                let len = utf8_len(c);
                let slice = b.get(start..start + len).ok_or("UTF-8 が途中")?;
                out.push_str(std::str::from_utf8(slice).map_err(|_| "不正な UTF-8")?);
                *i = start + len;
            }
        }
    }
    Err("文字列が閉じない".to_string())
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn parse_number(b: &[u8], i: &mut usize) -> Result<Json, String> {
    let start = *i;
    if b.get(*i) == Some(&b'-') {
        *i += 1;
    }
    while let Some(&c) = b.get(*i) {
        if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
            *i += 1;
        } else {
            break;
        }
    }
    std::str::from_utf8(&b[start..*i])
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map(Json::Num)
        .ok_or_else(|| format!("数値が不正 (offset {})", start))
}

// ============================================================================
// トレイト
// ============================================================================

pub trait Serialize {
    fn to_json(&self) -> Json;
}

pub trait Deserialize<'de>: Sized {
    fn from_json(v: &Json) -> Result<Self, String>;
}

pub mod de {
    /// 実 serde と同じく「借用しない」デシリアライズ。
    /// 生成コードはこちらを呼ぶ (ライフタイム推論を避けるため)。
    pub trait DeserializeOwned: Sized {
        fn from_json(v: &super::Json) -> Result<Self, String>;
    }
    impl<T> DeserializeOwned for T
    where
        T: for<'de> super::Deserialize<'de>,
    {
        fn from_json(v: &super::Json) -> Result<Self, String> {
            <T as super::Deserialize<'_>>::from_json(v)
        }
    }
}

pub mod ser {
    pub use super::Serialize;
}

// ============================================================================
// 標準型の実装
// ============================================================================

macro_rules! num_impl {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn to_json(&self) -> Json { Json::Num(*self as f64) }
        }
        impl<'de> Deserialize<'de> for $t {
            fn from_json(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Num(n) => Ok(*n as $t),
                    other => Err(format!("数値を期待: {:?}", other)),
                }
            }
        }
    )*};
}
num_impl!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

impl Serialize for bool {
    fn to_json(&self) -> Json {
        Json::Bool(*self)
    }
}
impl<'de> Deserialize<'de> for bool {
    fn from_json(v: &Json) -> Result<Self, String> {
        match v {
            Json::Bool(b) => Ok(*b),
            other => Err(format!("真偽値を期待: {:?}", other)),
        }
    }
}

impl Serialize for String {
    fn to_json(&self) -> Json {
        Json::Str(self.clone())
    }
}
impl<'de> Deserialize<'de> for String {
    fn from_json(v: &Json) -> Result<Self, String> {
        match v {
            Json::Str(s) => Ok(s.clone()),
            other => Err(format!("文字列を期待: {:?}", other)),
        }
    }
}

impl Serialize for str {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }
}

impl<T: Serialize + ?Sized> Serialize for &T {
    fn to_json(&self) -> Json {
        (**self).to_json()
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn to_json(&self) -> Json {
        match self {
            Some(v) => v.to_json(),
            None => Json::Null,
        }
    }
}
impl<'de, T: de::DeserializeOwned> Deserialize<'de> for Option<T> {
    fn from_json(v: &Json) -> Result<Self, String> {
        if v.is_null() {
            Ok(None)
        } else {
            Ok(Some(<T as de::DeserializeOwned>::from_json(v)?))
        }
    }
}

macro_rules! seq_impl {
    ($ty:ident) => {
        impl<T: Serialize> Serialize for $ty<T> {
            fn to_json(&self) -> Json {
                Json::Arr(self.iter().map(|v| v.to_json()).collect())
            }
        }
        impl<'de, T: de::DeserializeOwned> Deserialize<'de> for $ty<T> {
            fn from_json(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Arr(items) => items
                        .iter()
                        .map(|x| <T as de::DeserializeOwned>::from_json(x))
                        .collect(),
                    other => Err(format!("配列を期待: {:?}", other)),
                }
            }
        }
    };
}
seq_impl!(Vec);
seq_impl!(VecDeque);

impl<T: Serialize + Eq + std::hash::Hash> Serialize for HashSet<T> {
    fn to_json(&self) -> Json {
        Json::Arr(self.iter().map(|v| v.to_json()).collect())
    }
}
impl<'de, T: de::DeserializeOwned + Eq + std::hash::Hash> Deserialize<'de> for HashSet<T> {
    fn from_json(v: &Json) -> Result<Self, String> {
        match v {
            Json::Arr(items) => items
                .iter()
                .map(|x| <T as de::DeserializeOwned>::from_json(x))
                .collect(),
            other => Err(format!("配列を期待: {:?}", other)),
        }
    }
}

/// マップ。**キーは文字列化して JSON のキーにする** (実 serde と同じ規約)。
macro_rules! map_impl {
    ($ty:ident $(, $bound:ident)*) => {
        impl<K: ToString $(+ $bound)*, V: Serialize> Serialize for $ty<K, V> {
            fn to_json(&self) -> Json {
                Json::Obj(
                    self.iter()
                        .map(|(k, v)| (k.to_string(), v.to_json()))
                        .collect(),
                )
            }
        }
        impl<'de, K, V> Deserialize<'de> for $ty<K, V>
        where
            K: std::str::FromStr $(+ $bound)*,
            V: de::DeserializeOwned,
        {
            fn from_json(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Obj(kv) => kv
                        .iter()
                        .map(|(k, val)| {
                            let key = k
                                .parse::<K>()
                                .map_err(|_| format!("キーとして読めない: {}", k))?;
                            Ok((key, <V as de::DeserializeOwned>::from_json(val)?))
                        })
                        .collect(),
                    other => Err(format!("オブジェクトを期待: {:?}", other)),
                }
            }
        }
    };
}
map_impl!(HashMap, Eq, Hash);
map_impl!(BTreeMap, Ord);

// 文字列としてやり取りする標準型 (実 serde も同じ形)
macro_rules! via_string {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn to_json(&self) -> Json { Json::Str(self.to_string()) }
        }
        impl<'de> Deserialize<'de> for $t {
            fn from_json(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Str(s) => s.parse::<$t>()
                        .map_err(|_| format!("{} として読めない: {}", stringify!($t), s)),
                    other => Err(format!("文字列を期待: {:?}", other)),
                }
            }
        }
    )*};
}
via_string!(
    std::net::SocketAddr,
    std::net::SocketAddrV4,
    std::net::IpAddr,
    std::net::Ipv4Addr,
    char
);

macro_rules! tuple_impl {
    ($($n:tt $name:ident),+) => {
        impl<$($name: Serialize),+> Serialize for ($($name,)+) {
            fn to_json(&self) -> Json { Json::Arr(vec![$(self.$n.to_json()),+]) }
        }
        impl<'de, $($name: de::DeserializeOwned),+> Deserialize<'de> for ($($name,)+) {
            fn from_json(v: &Json) -> Result<Self, String> {
                match v {
                    Json::Arr(items) => Ok(($(
                        <$name as de::DeserializeOwned>::from_json(
                            items.get($n).ok_or("タプルの要素が足りない")?
                        )?,
                    )+)),
                    other => Err(format!("配列を期待: {:?}", other)),
                }
            }
        }
    };
}
tuple_impl!(0 A);
tuple_impl!(0 A, 1 B);
tuple_impl!(0 A, 1 B, 2 C);
tuple_impl!(0 A, 1 B, 2 C, 3 D);
tuple_impl!(0 A, 1 B, 2 C, 3 D, 4 E);
tuple_impl!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
tuple_impl!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
