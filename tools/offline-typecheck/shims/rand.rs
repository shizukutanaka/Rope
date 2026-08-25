//! `rand` の型検査専用スタブ。実 rand ではない (`README.md` 参照)。
//!
//! **擬似乱数を返す** (xorshift64*)。固定値だと「2 つの proof は異なる secret を
//! 持つ」「確認コードは毎回違う」といったテストが全て偽陽性になるため。
//!
//! 🔴 **暗号強度は無い。** 実 `OsRng` は OS の CSPRNG を使う。
//! 鍵生成や乱雑性の**セキュリティ検証**にこのスタブを使ってはいけない。

#[path = "_entropy.rs"]
mod entropy;

pub trait RngCore {
    fn next_u32(&mut self) -> u32 {
        entropy::next_u64() as u32
    }
    fn next_u64(&mut self) -> u64 {
        entropy::next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        entropy::fill(dest);
    }
}

pub trait Rng: RngCore {
    fn gen<T: Default>(&mut self) -> T {
        T::default()
    }
    /// `Range<u32>` / `Range<u64>` / `Range<usize>` に対して**実際に範囲内の値**を
    /// 返す。常に 0 だと「確認コードは 6 桁」「2 回呼べば違う値」といった
    /// テストが偽陽性になる。
    fn gen_range<T, R>(&mut self, range: R) -> T
    where
        R: RangeLike<T>,
    {
        range.pick(entropy::next_u64())
    }
    fn gen_bool(&mut self, _p: f64) -> bool {
        entropy::next_u64() & 1 == 1
    }
}

impl<T: RngCore + ?Sized> Rng for T {}

/// `gen_range` が受け取れる範囲。`std::ops::Range` にだけ実装する。
pub trait RangeLike<T> {
    fn pick(&self, entropy: u64) -> T;
}

macro_rules! range_like {
    ($($t:ty),*) => {$(
        impl RangeLike<$t> for std::ops::Range<$t> {
            fn pick(&self, entropy: u64) -> $t {
                if self.end <= self.start {
                    return self.start;
                }
                let span = (self.end - self.start) as u64;
                self.start + (entropy % span) as $t
            }
        }
        impl RangeLike<$t> for std::ops::RangeInclusive<$t> {
            fn pick(&self, entropy: u64) -> $t {
                let (lo, hi) = (*self.start(), *self.end());
                if hi <= lo {
                    return lo;
                }
                let span = (hi - lo) as u64 + 1;
                lo + (entropy % span) as $t
            }
        }
    )*};
}
range_like!(u8, u16, u32, u64, usize, i32, i64);

#[derive(Clone, Copy, Debug, Default)]
pub struct OsRng;

impl RngCore for OsRng {}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThreadRng;

impl RngCore for ThreadRng {}

pub fn thread_rng() -> ThreadRng {
    ThreadRng
}

pub mod rngs {
    pub use super::{OsRng, ThreadRng};
}

pub mod prelude {
    pub use super::{thread_rng, Rng, RngCore};
}
