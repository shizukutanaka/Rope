//! `chrono` の型検査専用スタブ。実 chrono ではない (`README.md` 参照)。

use std::fmt;
use std::ops::{Add, Sub};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Utc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct DateTime<Tz> {
    secs: i64,
    tz: std::marker::PhantomData<Tz>,
}

impl Utc {
    /// **実時刻を返す。** 固定値だと deadman / 経過時間 / TTL のテストが
    /// 全て偽陽性になる。分解能はナノ秒。
    pub fn now() -> DateTime<Utc> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        DateTime {
            secs: nanos,
            tz: std::marker::PhantomData,
        }
    }
}

impl<Tz> DateTime<Tz> {
    pub fn timestamp(&self) -> i64 {
        self.secs / 1_000_000_000
    }
    pub fn timestamp_millis(&self) -> i64 {
        self.secs / 1_000_000
    }
    pub fn timestamp_nanos_opt(&self) -> Option<i64> {
        Some(self.secs)
    }
    pub fn timestamp_micros(&self) -> i64 {
        self.secs / 1_000
    }
    pub fn timestamp_subsec_nanos(&self) -> u32 {
        0
    }
    pub fn to_rfc3339(&self) -> String {
        String::new()
    }
    /// 本物は `DelayedFormat` を返すが、Rope は常に `format!`/`to_string` 経由で
    /// 使うため Display を満たす型なら型検査上は等価。
    pub fn format(&self, _fmt: &str) -> String {
        String::new()
    }
    pub fn checked_add_signed(self, rhs: Duration) -> Option<DateTime<Tz>> {
        Some(DateTime {
            secs: self.secs + rhs.secs,
            tz: self.tz,
        })
    }
    pub fn checked_sub_signed(self, rhs: Duration) -> Option<DateTime<Tz>> {
        Some(DateTime {
            secs: self.secs - rhs.secs,
            tz: self.tz,
        })
    }
    pub fn signed_duration_since(self, rhs: DateTime<Tz>) -> Duration {
        Duration {
            secs: self.secs - rhs.secs,
        }
    }
}

impl<Tz> fmt::Display for DateTime<Tz> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.secs)
    }
}

impl<Tz> Sub for DateTime<Tz> {
    type Output = Duration;
    fn sub(self, rhs: DateTime<Tz>) -> Duration {
        Duration {
            secs: self.secs - rhs.secs,
        }
    }
}

impl<Tz> Add<Duration> for DateTime<Tz> {
    type Output = DateTime<Tz>;
    fn add(self, rhs: Duration) -> DateTime<Tz> {
        DateTime {
            secs: self.secs + rhs.secs,
            tz: self.tz,
        }
    }
}

impl<Tz> Sub<Duration> for DateTime<Tz> {
    type Output = DateTime<Tz>;
    fn sub(self, rhs: Duration) -> DateTime<Tz> {
        DateTime {
            secs: self.secs - rhs.secs,
            tz: self.tz,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Duration {
    secs: i64,
}

/// ⚠️ `secs` フィールドは**ナノ秒**を保持する (名前は歴史的経緯)。
/// `DateTime` と単位を揃えないと加減算が壊れる — 実際に一度壊した。
impl Duration {
    pub fn seconds(n: i64) -> Duration {
        Duration {
            secs: n.saturating_mul(1_000_000_000),
        }
    }
    pub fn minutes(n: i64) -> Duration {
        Duration::seconds(n.saturating_mul(60))
    }
    pub fn hours(n: i64) -> Duration {
        Duration::seconds(n.saturating_mul(3_600))
    }
    pub fn days(n: i64) -> Duration {
        Duration::seconds(n.saturating_mul(86_400))
    }
    pub fn milliseconds(n: i64) -> Duration {
        Duration {
            secs: n.saturating_mul(1_000_000),
        }
    }
    pub fn num_seconds(&self) -> i64 {
        self.secs / 1_000_000_000
    }
    pub fn num_minutes(&self) -> i64 {
        self.secs / 60_000_000_000
    }
    pub fn num_hours(&self) -> i64 {
        self.secs / 3_600_000_000_000
    }
    pub fn num_days(&self) -> i64 {
        self.secs / 86_400_000_000_000
    }
    pub fn num_milliseconds(&self) -> i64 {
        self.secs / 1_000_000
    }
}

// 🔴 **実 chrono は RFC3339 文字列で書く。ここはエポックからのナノ秒 (数値)。**
// このスタブを通した round-trip は通るが、実 serde_json が書いたファイルとは
// 互換ではない (`serde.rs` の注意を参照)。
impl<Tz> serde::Serialize for DateTime<Tz> {
    fn to_json(&self) -> serde::Json {
        serde::Json::Num(self.secs as f64)
    }
}
impl<'de, Tz> serde::Deserialize<'de> for DateTime<Tz> {
    fn from_json(v: &serde::Json) -> Result<Self, String> {
        match v {
            serde::Json::Num(n) => Ok(DateTime {
                secs: *n as i64,
                tz: std::marker::PhantomData,
            }),
            // 実 chrono 形式 (RFC3339 文字列) は解釈しない — 読めたふりをしない
            other => Err(format!("ナノ秒の数値を期待: {:?}", other)),
        }
    }
}
