//! `chrono` の型検査専用スタブ。実 chrono ではない (`README.md` 参照)。

use std::fmt;
use std::ops::{Add, Sub};

/// Howard Hinnant の civil_from_days (proleptic Gregorian、厳密)。
/// 1970-01-01 からの日数 → (年, 月, 日)。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 上記の逆。
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// ナノ秒 → (年, 月, 日, 時, 分, 秒, ナノ秒余り)
fn breakdown(nanos: i64) -> (i64, u32, u32, u32, u32, u32, u32) {
    let secs = nanos.div_euclid(1_000_000_000);
    let sub = nanos.rem_euclid(1_000_000_000) as u32;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    (
        y,
        mo,
        d,
        (tod / 3600) as u32,
        ((tod % 3600) / 60) as u32,
        (tod % 60) as u32,
        sub,
    )
}

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
    /// RFC3339 (ナノ秒精度、UTC)。**実 chrono の serde 表現と同じ形。**
    pub fn to_rfc3339(&self) -> String {
        let (y, mo, d, h, mi, sec, nano) = breakdown(self.secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
            y, mo, d, h, mi, sec, nano
        )
    }

    /// 本物は `DelayedFormat` を返すが、Rope は `format!` 経由でしか使わないので
    /// `String` で等価。対応する指定子はこのリポジトリが使う分だけ。
    pub fn format(&self, fmt: &str) -> String {
        let (y, mo, d, h, mi, sec, _) = breakdown(self.secs);
        let mut out = String::new();
        let mut it = fmt.chars().peekable();
        while let Some(c) = it.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match it.next() {
                Some('Y') => out.push_str(&format!("{:04}", y)),
                Some('m') => out.push_str(&format!("{:02}", mo)),
                Some('d') => out.push_str(&format!("{:02}", d)),
                Some('H') => out.push_str(&format!("{:02}", h)),
                Some('M') => out.push_str(&format!("{:02}", mi)),
                Some('S') => out.push_str(&format!("{:02}", sec)),
                Some('%') => out.push('%'),
                // 未対応の指定子は**そのまま残す** (黙って消すと嘘になる)
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        }
        out
    }

    /// RFC3339 文字列から復元する。秒の小数部は任意。
    pub fn parse_rfc3339(s: &str) -> Option<DateTime<Tz>> {
        let b = s.as_bytes();
        if b.len() < 20 {
            return None;
        }
        let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse().ok() };
        let y = num(0, 4)?;
        let mo = num(5, 7)? as u32;
        let d = num(8, 10)? as u32;
        let h = num(11, 13)?;
        let mi = num(14, 16)?;
        let sec = num(17, 19)?;
        let mut nano = 0i64;
        if b.get(19) == Some(&b'.') {
            let frac: String = s[20..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .take(9)
                .collect();
            let scale = 10i64.pow(9 - frac.len() as u32);
            nano = frac.parse::<i64>().ok()? * scale;
        }
        let days = days_from_civil(y, mo, d);
        Some(DateTime {
            secs: (days * 86_400 + h * 3600 + mi * 60 + sec) * 1_000_000_000 + nano,
            tz: std::marker::PhantomData,
        })
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
        f.write_str(&self.to_rfc3339())
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

// **実 chrono の serde と同じ RFC3339 文字列**で読み書きする。
// 以前はナノ秒の数値で済ませていたが、それだと「このスタブを通した往復」しか
// 検証できず、実 serde_json が書いたファイルとの互換は分からなかった。
// 暦の変換は Howard Hinnant のアルゴリズムで厳密。
impl<Tz> serde::Serialize for DateTime<Tz> {
    fn to_json(&self) -> serde::Json {
        serde::Json::Str(self.to_rfc3339())
    }
}
impl<'de, Tz> serde::Deserialize<'de> for DateTime<Tz> {
    fn from_json(v: &serde::Json) -> Result<Self, String> {
        match v {
            serde::Json::Str(s) => {
                DateTime::parse_rfc3339(s).ok_or_else(|| format!("RFC3339 として読めない: {}", s))
            }
            other => Err(format!("RFC3339 文字列を期待: {:?}", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既知の瞬間と一致するか。**独立に検算できる値だけを並べる。**
    /// 暦の変換が狂うと deadman・TTL・表示が全部静かに狂うので、ここは厳密に。
    #[test]
    fn rfc3339_matches_known_instants() {
        let cases: &[(i64, &str)] = &[
            (0, "1970-01-01T00:00:00.000000000Z"),
            (1, "1970-01-01T00:00:01.000000000Z"),
            (86_399, "1970-01-01T23:59:59.000000000Z"),
            (86_400, "1970-01-02T00:00:00.000000000Z"),
            (951_782_400, "2000-02-29T00:00:00.000000000Z"), // 400 年閏
            (1_078_012_800, "2004-02-29T00:00:00.000000000Z"),
            (1_709_164_800, "2024-02-29T00:00:00.000000000Z"),
            (1_735_689_599, "2024-12-31T23:59:59.000000000Z"),
            (1_735_689_600, "2025-01-01T00:00:00.000000000Z"),
            (4_102_444_800, "2100-01-01T00:00:00.000000000Z"), // 100 年非閏
            (-1, "1969-12-31T23:59:59.000000000Z"),            // エポック以前
            (-86_400, "1969-12-31T00:00:00.000000000Z"),
        ];
        for (secs, want) in cases {
            let d: DateTime<Utc> = DateTime::parse_rfc3339(want).expect(want);
            assert_eq!(d.timestamp(), *secs, "parse: {}", want);
            assert_eq!(&d.to_rfc3339(), want, "format: {}", want);
        }
    }

    /// 1970..2070 を 1 日ずつ往復 (閏年・月末を全部踏む)。
    #[test]
    fn every_day_for_a_century_roundtrips() {
        let epoch: DateTime<Utc> = DateTime::parse_rfc3339("1970-01-01T00:00:00.000000000Z")
            .expect("epoch");
        for day in 0..36_525i64 {
            let d = epoch + Duration::days(day);
            let s = d.to_rfc3339();
            let back: DateTime<Utc> = DateTime::parse_rfc3339(&s)
                .unwrap_or_else(|| panic!("day {} -> {}", day, s));
            assert_eq!(back.timestamp(), d.timestamp(), "day {} ({})", day, s);
        }
    }

    /// 秒未満も保つ。
    #[test]
    fn subsecond_precision_survives() {
        let s = "2026-08-18T09:05:03.123456789Z";
        let d: DateTime<Utc> = DateTime::parse_rfc3339(s).expect(s);
        assert_eq!(d.to_rfc3339(), s);
    }

    #[test]
    fn format_specifiers_work() {
        let d: DateTime<Utc> =
            DateTime::parse_rfc3339("2026-08-18T09:05:03.000000000Z").expect("parse");
        assert_eq!(d.format("%Y-%m-%d %H:%M:%S UTC"), "2026-08-18 09:05:03 UTC");
        // 未対応の指定子は消さずに残す (黙って消すと嘘の出力になる)
        assert_eq!(d.format("%Q"), "%Q");
    }

    /// `Duration` と `DateTime` の単位が揃っていること。
    /// 一度ここを片方だけ直して、課金と経過時間を壊した。
    #[test]
    fn duration_and_datetime_agree_on_units() {
        let a = Utc::now();
        let b = a + Duration::seconds(90);
        assert_eq!((b - a).num_seconds(), 90);
        assert_eq!((b - a).num_minutes(), 1);
        assert_eq!((b - a).num_milliseconds(), 90_000);
        assert_eq!((a + Duration::minutes(2) - a).num_seconds(), 120);
        assert_eq!((a + Duration::days(1) - a).num_hours(), 24);
    }
}
