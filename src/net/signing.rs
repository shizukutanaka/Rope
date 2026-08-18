//! `wire` の署名トレイトを実 ed25519 に繋ぐアダプタ。
//!
//! **このファイルだけが暗号 crate に触れる。** `wire`/`transport` を依存ゼロに
//! 保つための境界であり、そのおかげで**プロトコル論理はこの環境で実際に
//! テストできる** (`docs/SURPLUS_AND_GAPS.md` §0)。
//!
//! 使うのは `ed25519-dalek` — **検証済みの既存依存**であって、
//! プリミティブを手書きしてはいない (§1.1 の禁止事項に触れない)。
//!
//! ⚠️ 署名は**完全性と真正性**のみ。**暗号化ではない** —
//! 中身は平文で流れる (`transport` の冒頭を参照)。

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use super::wire::{FrameSigner, FrameVerifier};

/// 自分の秘密鍵でフレームに署名する。
pub struct Ed25519Signer {
    key: SigningKey,
}

impl Ed25519Signer {
    pub fn new(key: SigningKey) -> Ed25519Signer {
        Ed25519Signer { key }
    }
}

impl FrameSigner for Ed25519Signer {
    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.key.sign(msg).to_bytes().to_vec()
    }
}

/// 相手の公開鍵でフレームを検証する。
pub struct Ed25519Verifier {
    key: VerifyingKey,
}

impl Ed25519Verifier {
    pub fn new(key: VerifyingKey) -> Ed25519Verifier {
        Ed25519Verifier { key }
    }

    /// hex 文字列 (32 バイト = 64 文字) から作る。
    ///
    /// 相手が名乗った鍵は**外から来た文字列**なので、長さも形式も検証する。
    pub fn from_hex(hex_str: &str) -> Option<Ed25519Verifier> {
        let raw = hex::decode(hex_str).ok()?;
        let arr: [u8; 32] = raw.try_into().ok()?;
        VerifyingKey::from_bytes(&arr)
            .ok()
            .map(Ed25519Verifier::new)
    }
}

impl FrameVerifier for Ed25519Verifier {
    fn verify(&self, msg: &[u8], sig: &[u8]) -> bool {
        let arr: [u8; 64] = match sig.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        // verify_strict: 展性のある署名を拒否する厳格版
        self.key
            .verify_strict(msg, &Signature::from_bytes(&arr))
            .is_ok()
    }
}
