//! `rand` の型検査専用スタブ。実 rand ではない (`README.md` 参照)。
//!
//! ⚠️ **乱数を返さない (常に 0)**。乱雑性・鍵生成の検証には一切使えない。

pub trait RngCore {
    fn next_u32(&mut self) -> u32 {
        0
    }
    fn next_u64(&mut self) -> u64 {
        0
    }
    fn fill_bytes(&mut self, _dest: &mut [u8]) {}
}

pub trait Rng: RngCore {
    fn gen<T: Default>(&mut self) -> T {
        T::default()
    }
    fn gen_range<T, R>(&mut self, _range: R) -> T
    where
        T: Default,
    {
        T::default()
    }
    fn gen_bool(&mut self, _p: f64) -> bool {
        false
    }
}

impl<T: RngCore + ?Sized> Rng for T {}

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
