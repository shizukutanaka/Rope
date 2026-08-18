//! `serde` の型検査専用スタブ。実 serde ではない (`README.md` 参照)。

pub use serde_derive_shim::{Deserialize, Serialize};

/// 本物と違い required method を持たない — 空 impl で満たせるようにするため。
pub trait Serialize {}

pub trait Deserialize<'de>: Sized {}

pub mod de {
    pub trait DeserializeOwned {}
    impl<T> DeserializeOwned for T where T: for<'de> super::Deserialize<'de> {}
}

pub mod ser {
    pub use super::Serialize;
}

// 標準型: 本物の serde は多数の型に impl を持つ。ここは Rope が
// 実際に serde 境界へ渡す型だけを埋める (足りなければ rustc が教えてくれる)。
impl Serialize for str {}
impl Serialize for String {}
impl Serialize for bool {}
impl Serialize for u8 {}
impl Serialize for u32 {}
impl Serialize for u64 {}
impl Serialize for i64 {}
impl Serialize for f64 {}
impl Serialize for usize {}
impl<T: Serialize + ?Sized> Serialize for &T {}
impl<T: Serialize> Serialize for Vec<T> {}
impl<T: Serialize> Serialize for Option<T> {}
impl<K, V: Serialize> Serialize for std::collections::BTreeMap<K, V> {}
impl<K, V: Serialize> Serialize for std::collections::HashMap<K, V> {}

impl<'de> Deserialize<'de> for String {}
impl<'de> Deserialize<'de> for bool {}
impl<'de> Deserialize<'de> for u64 {}
impl<'de> Deserialize<'de> for i64 {}
impl<'de> Deserialize<'de> for f64 {}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Vec<T> {}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Option<T> {}

// 本物の serde はタプルにも impl を持つ (session.rs が署名対象を
// タプルで組み立てて to_vec に渡す)。
macro_rules! tuple_serialize {
    ($($name:ident),+) => {
        impl<$($name: Serialize),+> Serialize for ($($name,)+) {}
        impl<'de, $($name: Deserialize<'de>),+> Deserialize<'de> for ($($name,)+) {}
    };
}
tuple_serialize!(A);
tuple_serialize!(A, B);
tuple_serialize!(A, B, C);
tuple_serialize!(A, B, C, D);
tuple_serialize!(A, B, C, D, E);
tuple_serialize!(A, B, C, D, E, F);
tuple_serialize!(A, B, C, D, E, F, G);
tuple_serialize!(A, B, C, D, E, F, G, H);
tuple_serialize!(A, B, C, D, E, F, G, H, I);
tuple_serialize!(A, B, C, D, E, F, G, H, I, J);
