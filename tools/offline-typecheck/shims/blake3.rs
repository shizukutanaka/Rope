//! `blake3` の型検査専用スタブ。実 blake3 ではない (`README.md` 参照)。
//!
//! ⚠️ ハッシュ値は返さない。**暗号的性質の検証には一切使えない。**

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn to_hex(&self) -> String {
        String::new()
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("0")
    }
}

impl From<Hash> for [u8; 32] {
    fn from(h: Hash) -> [u8; 32] {
        h.0
    }
}

pub fn hash(_input: &[u8]) -> Hash {
    Hash([0u8; 32])
}

pub fn keyed_hash(_key: &[u8; 32], _input: &[u8]) -> Hash {
    Hash([0u8; 32])
}

#[derive(Default, Clone)]
pub struct Hasher;

impl Hasher {
    pub fn new() -> Hasher {
        Hasher
    }
    pub fn update(&mut self, _input: &[u8]) -> &mut Hasher {
        self
    }
    pub fn finalize(&self) -> Hash {
        Hash([0u8; 32])
    }
}
