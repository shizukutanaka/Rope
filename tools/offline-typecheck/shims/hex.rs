//! `hex` の型検査専用スタブ。実 hex ではない (`README.md` 参照)。

#[derive(Debug)]
pub struct FromHexError;

impl std::fmt::Display for FromHexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("hex error")
    }
}

impl std::error::Error for FromHexError {}

pub fn encode<T: AsRef<[u8]>>(_data: T) -> String {
    String::new()
}

pub fn decode<T: AsRef<[u8]>>(_data: T) -> Result<Vec<u8>, FromHexError> {
    Ok(Vec::new())
}
