//! `ed25519-dalek` の型検査専用スタブ。実 ed25519-dalek ではない (`README.md` 参照)。
//!
//! ⚠️ **署名も検証もしない** (`verify` は常に Ok)。暗号的性質の検証には使えない。

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
        _message: &[u8],
        _signature: &Signature,
    ) -> Result<(), SignatureError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct SigningKey([u8; 32]);

impl SigningKey {
    pub fn generate<R>(_rng: &mut R) -> SigningKey {
        SigningKey([0u8; 32])
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
    fn sign(&self, _message: &[u8]) -> Signature {
        Signature([0u8; 64])
    }
}

impl Verifier<Signature> for VerifyingKey {
    fn verify(&self, _message: &[u8], _signature: &Signature) -> Result<(), SignatureError> {
        Ok(())
    }
}
