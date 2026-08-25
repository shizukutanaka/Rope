//! `base64` の型検査専用スタブ。
//!
//! **STANDARD の符号化は本物と同じ結果になる** — base64 は仕様が一意なので
//! ここを正しく書ける。おかげで鍵の保存/読込を通る実テストが実際に走る。

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug)]
pub struct DecodeError;

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("base64 error")
    }
}

impl std::error::Error for DecodeError {}

fn val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

pub trait Engine {
    fn encode<T: AsRef<[u8]>>(&self, input: T) -> String {
        let data = input.as_ref();
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    fn decode<T: AsRef<[u8]>>(&self, input: T) -> Result<Vec<u8>, DecodeError> {
        let data: Vec<u8> = input
            .as_ref()
            .iter()
            .copied()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        if data.len() % 4 != 0 {
            return Err(DecodeError);
        }
        let mut out = Vec::new();
        for chunk in data.chunks_exact(4) {
            let pad = chunk.iter().filter(|c| **c == b'=').count();
            let mut n = 0u32;
            for (i, c) in chunk.iter().enumerate() {
                let v = if *c == b'=' {
                    0
                } else {
                    val(*c).ok_or(DecodeError)?
                };
                n |= (v as u32) << (18 - 6 * i);
            }
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
        }
        Ok(out)
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
