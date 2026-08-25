//! `ed25519-dalek` の型検査専用スタブ。実 ed25519-dalek ではない (`README.md` 参照)。
//!
//! ⚠️ **署名も検証もしない** (`verify` は常に Ok)。暗号的性質の検証には使えない。

#[path = "_entropy.rs"]
mod entropy;

#[derive(Debug)]
pub struct SignatureError;

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("signature error")
    }
}

impl std::error::Error for SignatureError {}

pub const PUBLIC_KEY_LENGTH: usize = 32;
pub const SECRET_KEY_LENGTH: usize = 32;
pub const SIGNATURE_LENGTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature([u8; 64]);

impl Signature {
    pub fn from_bytes(bytes: &[u8; 64]) -> Signature {
        Signature(*bytes)
    }
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifyingKey([u8; 32]);

impl VerifyingKey {
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<VerifyingKey, SignatureError> {
        Ok(VerifyingKey(*bytes))
    }
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    /// 本物は展性のある署名を拒否する厳格版。ここは型のみ。
    pub fn verify_strict(
        &self,
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), SignatureError> {
        <VerifyingKey as Verifier<Signature>>::verify(self, message, signature)
    }
}

#[derive(Clone)]
pub struct SigningKey([u8; 32]);

impl SigningKey {
    /// **毎回異なる鍵を返す。** 固定値だと「別の鍵は別の指紋を持つ」
    /// といったテストが偽陽性になる。
    /// 🔴 暗号強度は無い (`_entropy` は xorshift)。
    pub fn generate<R>(_rng: &mut R) -> SigningKey {
        let mut k = [0u8; 32];
        entropy::fill(&mut k);
        SigningKey(k)
    }
    pub fn from_bytes(bytes: &[u8; 32]) -> SigningKey {
        SigningKey(*bytes)
    }
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.0)
    }
}

pub trait Signer<S> {
    fn sign(&self, message: &[u8]) -> S;
    fn try_sign(&self, message: &[u8]) -> Result<S, SignatureError> {
        Ok(self.sign(message))
    }
}

pub trait Verifier<S> {
    fn verify(&self, message: &[u8], signature: &S) -> Result<(), SignatureError>;
}

impl Signer<Signature> for SigningKey {
    /// 鍵とメッセージから決定論的に導出する。**本物の Ed25519 ではない**が、
    /// 「違うメッセージ/鍵なら違う署名」というロジックのテストは通る。
    fn sign(&self, message: &[u8]) -> Signature {
        let mut sig = [0u8; 64];
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in self.0.iter().chain(message.iter()) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let mut x = h | 1;
        for chunk in sig.chunks_mut(8) {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            chunk.copy_from_slice(&x.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
        }
        Signature(sig)
    }
}

impl Verifier<Signature> for VerifyingKey {
    /// 対応する `SigningKey` が作る署名と一致するかを見る。
    /// **常に Ok を返すと改竄検出のテストが全て偽陽性になる**ため。
    /// 🔴 本物の Ed25519 検証ではない。
    fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        let expected = SigningKey(self.0).sign(message);
        if expected.0 == signature.0 {
            Ok(())
        } else {
            Err(SignatureError)
        }
    }
}
