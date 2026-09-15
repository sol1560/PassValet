//! Key hierarchy:
//!
//! - `KEK` (key-encryption key, 32 bytes) lives only in memory while the vault is unlocked.
//!   It is derived from the passkey PRF output via HKDF, or held in the Keychain for the
//!   Touch ID fallback provider.
//! - Every secret has its own random `DEK` (data-encryption key). The DEK is wrapped with the
//!   KEK and the secret value is sealed with the DEK. Rewrapping the KEK therefore never
//!   touches ciphertext.
//! - All AEAD is XChaCha20-Poly1305 with a random 24-byte nonce and an AAD that binds the
//!   ciphertext to its purpose (secret id, "verifier", "recovery").

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::{CoreError, CoreResult};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;

const KEK_INFO: &[u8] = b"passvalet/kek/v1";
const RECOVERY_INFO: &[u8] = b"passvalet/recovery/v1";

/// A 32-byte symmetric key that is zeroized on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SymKey(pub [u8; KEY_LEN]);

impl SymKey {
    pub fn random() -> Self {
        let mut k = [0u8; KEY_LEN];
        rand::thread_rng().fill_bytes(&mut k);
        SymKey(k)
    }

    pub fn from_slice(bytes: &[u8]) -> CoreResult<Self> {
        if bytes.len() != KEY_LEN {
            return Err(CoreError::Crypto(format!(
                "expected {KEY_LEN}-byte key, got {}",
                bytes.len()
            )));
        }
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(bytes);
        Ok(SymKey(k))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for SymKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SymKey(<redacted>)")
    }
}

/// Key-encryption key.
pub type Kek = SymKey;

pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut v);
    v
}

/// Derive the KEK from a passkey PRF output (or any high-entropy secret) and a per-vault salt.
pub fn derive_kek(prf_output: &[u8], salt: &[u8]) -> CoreResult<Kek> {
    hkdf_expand(prf_output, salt, KEK_INFO)
}

/// Derive the wrapping key for the recovery copy of the KEK from the user's recovery key.
pub fn derive_recovery_key(recovery_secret: &[u8], salt: &[u8]) -> CoreResult<SymKey> {
    hkdf_expand(recovery_secret, salt, RECOVERY_INFO)
}

fn hkdf_expand(ikm: &[u8], salt: &[u8], info: &[u8]) -> CoreResult<SymKey> {
    if ikm.is_empty() {
        return Err(CoreError::Crypto("empty input key material".into()));
    }
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = [0u8; KEY_LEN];
    hk.expand(info, &mut okm)
        .map_err(|e| CoreError::Crypto(format!("hkdf expand: {e}")))?;
    Ok(SymKey(okm))
}

/// Nonce + ciphertext, stored as a single blob.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sealed {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sealed({} bytes)", self.ciphertext.len())
    }
}

impl Sealed {
    pub fn to_blob(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(NONCE_LEN + self.ciphertext.len());
        out.extend_from_slice(&self.nonce);
        out.extend_from_slice(&self.ciphertext);
        out
    }

    pub fn from_blob(blob: &[u8]) -> CoreResult<Self> {
        if blob.len() < NONCE_LEN + 16 {
            return Err(CoreError::Crypto("sealed blob too short".into()));
        }
        Ok(Sealed {
            nonce: blob[..NONCE_LEN].to_vec(),
            ciphertext: blob[NONCE_LEN..].to_vec(),
        })
    }
}

pub fn seal(key: &SymKey, plaintext: &[u8], aad: &[u8]) -> CoreResult<Sealed> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let nonce_bytes = random_bytes(NONCE_LEN);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CoreError::Crypto("encrypt failed".into()))?;
    Ok(Sealed {
        nonce: nonce_bytes,
        ciphertext,
    })
}

pub fn open(key: &SymKey, sealed: &Sealed, aad: &[u8]) -> CoreResult<Zeroizing<Vec<u8>>> {
    if sealed.nonce.len() != NONCE_LEN {
        return Err(CoreError::Crypto("bad nonce length".into()));
    }
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let nonce = XNonce::from_slice(&sealed.nonce);
    let pt = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &sealed.ciphertext,
                aad,
            },
        )
        .map_err(|_| CoreError::WrongKey)?;
    Ok(Zeroizing::new(pt))
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let digest = Sha256::digest(data);
    hex_encode(&digest)
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let k = SymKey::random();
        let sealed = seal(&k, b"hello", b"aad").unwrap();
        let pt = open(&k, &sealed, b"aad").unwrap();
        assert_eq!(&pt[..], b"hello");
        assert!(open(&k, &sealed, b"other").is_err());
        assert!(open(&SymKey::random(), &sealed, b"aad").is_err());
    }

    #[test]
    fn kek_derivation_is_deterministic() {
        let a = derive_kek(b"prf-output", b"salt").unwrap();
        let b = derive_kek(b"prf-output", b"salt").unwrap();
        let c = derive_kek(b"prf-output", b"salt2").unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), c.as_bytes());
    }

    #[test]
    fn blob_roundtrip() {
        let k = SymKey::random();
        let sealed = seal(&k, b"x", b"").unwrap();
        let blob = sealed.to_blob();
        let back = Sealed::from_blob(&blob).unwrap();
        assert_eq!(sealed, back);
    }
}
