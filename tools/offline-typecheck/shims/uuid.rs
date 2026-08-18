//! `uuid` の型検査専用スタブ。実 uuid ではない (`README.md` 参照)。

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Uuid(u128);

impl Uuid {
    pub fn now_v7() -> Uuid {
        Uuid(0)
    }
    pub fn new_v4() -> Uuid {
        Uuid(0)
    }
    pub fn nil() -> Uuid {
        Uuid(0)
    }
    pub fn as_u128(&self) -> u128 {
        self.0
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl serde::Serialize for Uuid {}
impl<'de> serde::Deserialize<'de> for Uuid {}
