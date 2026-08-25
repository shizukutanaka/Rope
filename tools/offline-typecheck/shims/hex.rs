//! `hex` の型検査専用スタブ。
//!
//! **符号化そのものは本物と同じ結果になる** — hex は仕様が一意なので、
//! ここを正しく書くと `run-tests.sh` で実際のテストが通る。

#[derive(Debug)]
pub struct FromHexError;

impl std::fmt::Display for FromHexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("hex error")
    }
}

impl std::error::Error for FromHexError {}

pub fn encode<T: AsRef<[u8]>>(data: T) -> String {
    let mut s = String::with_capacity(data.as_ref().len() * 2);
    for b in data.as_ref() {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    s
}

pub fn decode<T: AsRef<[u8]>>(data: T) -> Result<Vec<u8>, FromHexError> {
    let d = data.as_ref();
    if d.len() % 2 != 0 {
        return Err(FromHexError);
    }
    let mut out = Vec::with_capacity(d.len() / 2);
    for pair in d.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16).ok_or(FromHexError)?;
        let lo = (pair[1] as char).to_digit(16).ok_or(FromHexError)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **「本物と同じ符号化」と主張している以上、既知ベクタで裏を取る。**
    #[test]
    fn known_vectors() {
        assert_eq!(encode([]), "");
        assert_eq!(encode([0x00]), "00");
        assert_eq!(encode([0xff]), "ff");
        assert_eq!(encode([0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(encode(b"Hello"), "48656c6c6f");
        assert_eq!(decode("deadbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(decode("DEADBEEF").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(decode("abc").is_err(), "奇数長");
        assert!(decode("zz").is_err(), "hex でない");
    }

    #[test]
    fn roundtrips_all_bytes() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(encode(&all)).unwrap(), all);
    }
}
