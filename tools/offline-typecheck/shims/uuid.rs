//! `uuid` の型検査専用スタブ。実 uuid ではない (`README.md` 参照)。

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Uuid(u128);

/// 単調増加カウンタ。同一プロセス内で必ず異なる ID を出すため。
static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique() -> u128 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
    (t << 16) ^ n ^ (n << 96)
}

impl Uuid {
    /// **毎回異なる値を返す。** 固定値だと「2 つのセッションの ID は異なる」
    /// 「intent キューが重複を弾く」といったテストが偽陽性になる。
    /// ⚠️ v7 のビットレイアウトは再現していない (一意性だけが目的)。
    pub fn now_v7() -> Uuid {
        Uuid(unique())
    }
    pub fn new_v4() -> Uuid {
        Uuid(unique())
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

impl serde::Serialize for Uuid {
    fn to_json(&self) -> serde::Json {
        serde::Json::Str(self.to_string())
    }
}
impl<'de> serde::Deserialize<'de> for Uuid {
    fn from_json(v: &serde::Json) -> Result<Self, String> {
        match v {
            serde::Json::Str(s) => u128::from_str_radix(s, 16)
                .map(Uuid)
                .map_err(|_| format!("uuid として読めない: {}", s)),
            other => Err(format!("文字列を期待: {:?}", other)),
        }
    }
}
