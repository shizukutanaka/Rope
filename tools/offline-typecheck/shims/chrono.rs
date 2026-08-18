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
    pub fn now() -> DateTime<Utc> {
        DateTime {
            secs: 0,
            tz: std::marker::PhantomData,
        }
    }
}

impl<Tz> DateTime<Tz> {
    pub fn timestamp(&self) -> i64 {
        self.secs
    }
    pub fn timestamp_millis(&self) -> i64 {
        self.secs * 1000
    }
    pub fn timestamp_nanos_opt(&self) -> Option<i64> {
        Some(self.secs)
    }
    pub fn timestamp_micros(&self) -> i64 {
        self.secs
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

impl Duration {
    pub fn seconds(n: i64) -> Duration {
        Duration { secs: n }
    }
    pub fn minutes(n: i64) -> Duration {
        Duration { secs: n * 60 }
    }
    pub fn hours(n: i64) -> Duration {
        Duration { secs: n * 3600 }
    }
    pub fn days(n: i64) -> Duration {
        Duration {
            secs: n * 86_400,
        }
    }
    pub fn milliseconds(n: i64) -> Duration {
        Duration { secs: n / 1000 }
    }
    pub fn num_seconds(&self) -> i64 {
        self.secs
    }
    pub fn num_minutes(&self) -> i64 {
        self.secs / 60
    }
    pub fn num_hours(&self) -> i64 {
        self.secs / 3600
    }
    pub fn num_days(&self) -> i64 {
        self.secs / 86_400
    }
    pub fn num_milliseconds(&self) -> i64 {
        self.secs * 1000
    }
}

// serde 境界を満たすためだけの impl (実 chrono も serde feature で提供する)。
impl<Tz> serde::Serialize for DateTime<Tz> {}
impl<'de, Tz> serde::Deserialize<'de> for DateTime<Tz> {}
