//! `base64` の型検査専用スタブ。実 base64 ではない (`README.md` 参照)。

#[derive(Debug)]
pub struct DecodeError;

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("base64 error")
    }
}

impl std::error::Error for DecodeError {}

pub trait Engine {
    fn encode<T: AsRef<[u8]>>(&self, _input: T) -> String {
        String::new()
    }
    fn decode<T: AsRef<[u8]>>(&self, _input: T) -> Result<Vec<u8>, DecodeError> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Copy)]
pub struct GeneralPurpose;

impl Engine for GeneralPurpose {}

pub mod engine {
    pub mod general_purpose {
        pub const STANDARD: super::super::GeneralPurpose = super::super::GeneralPurpose;
        pub const STANDARD_NO_PAD: super::super::GeneralPurpose = super::super::GeneralPurpose;
        pub const URL_SAFE: super::super::GeneralPurpose = super::super::GeneralPurpose;
        pub const URL_SAFE_NO_PAD: super::super::GeneralPurpose = super::super::GeneralPurpose;
    }
    pub use super::{Engine, GeneralPurpose};
}
