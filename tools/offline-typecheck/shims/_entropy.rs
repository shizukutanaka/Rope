// 共有の擬似乱数源 (shim 内部専用)。
//
// ⚠️ **暗号用途ではない。** テストが「異なる入力 → 異なる出力」を確かめられる
// だけの分布を持たせるためのもの。実 crate は本物の CSPRNG / BLAKE3 を使う。
use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static STATE: Cell<u64> = Cell::new(0);
}

fn seed() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    t ^ 0xA076_1D64_78BD_642F
}

/// xorshift64* — 分布は十分だが暗号強度は無い。
pub fn next_u64() -> u64 {
    STATE.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = seed() | 1;
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

pub fn fill(dest: &mut [u8]) {
    let mut i = 0;
    while i < dest.len() {
        let v = next_u64().to_le_bytes();
        let n = (dest.len() - i).min(8);
        dest[i..i + n].copy_from_slice(&v[..n]);
        i += n;
    }
}
