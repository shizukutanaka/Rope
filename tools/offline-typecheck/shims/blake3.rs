//! `blake3` の型検査専用スタブ。
//!
//! 🔴 **これは BLAKE3 ではない。** 出力値は本物と一致しない。
//!
//! ただし**入力が違えば出力も違う**という性質は持たせてある (well-distributed
//! な非暗号ハッシュ)。おかげで「異なる proof は異なる nullifier を持つ」
//! 「nonce が違えば署名も違う」といった**ロジックのテストが実際に走る**。
//!
//! 🔴 **暗号的性質 (衝突耐性・原像計算困難性) は一切無い。**
//! セキュリティの検証にこのスタブを使ってはいけない。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
            s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
        }
        s
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl From<Hash> for [u8; 32] {
    fn from(h: Hash) -> [u8; 32] {
        h.0
    }
}

/// FNV-1a で畳んでから xorshift で 32 バイトへ伸ばす。
/// 決定論的で、入力が 1 ビット違えば全体が変わる。
fn digest(key: u64, input: &[u8]) -> Hash {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ key;
    for b in input {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    // 長さも混ぜる (伸長攻撃対策ではなく、単に分布のため)
    h ^= input.len() as u64;
    h = h.wrapping_mul(0x0000_0100_0000_01b3);

    let mut out = [0u8; 32];
    let mut x = h | 1;
    for chunk in out.chunks_mut(8) {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
    Hash(out)
}

pub fn hash(input: &[u8]) -> Hash {
    digest(0, input)
}

pub fn keyed_hash(key: &[u8; 32], input: &[u8]) -> Hash {
    let mut k: u64 = 0;
    for (i, b) in key.iter().enumerate() {
        k ^= (*b as u64) << ((i % 8) * 8);
    }
    digest(k | 0x8000_0000_0000_0000, input)
}

#[derive(Default, Clone)]
pub struct Hasher {
    buf: Vec<u8>,
}

impl Hasher {
    pub fn new() -> Hasher {
        Hasher { buf: Vec::new() }
    }
    pub fn update(&mut self, input: &[u8]) -> &mut Hasher {
        self.buf.extend_from_slice(input);
        self
    }
    pub fn finalize(&self) -> Hash {
        hash(&self.buf)
    }
}
