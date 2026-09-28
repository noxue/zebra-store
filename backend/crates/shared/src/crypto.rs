//! AES-256-GCM encryption compatible with the original `internal/crypto` package.
//!
//! Key = SHA-256(secret); ciphertext = hex(nonce ‖ sealed).

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};

/// GCM standard nonce length in bytes.
const NONCE_LEN: usize = 12;

/// Errors from [`Cipher`].
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("ciphertext is not valid hex")]
    Hex(#[from] hex::FromHexError),
    #[error("ciphertext too short")]
    TooShort,
    #[error("decryption failed")]
    Decrypt,
    #[error("encryption failed")]
    Encrypt,
    #[error("plaintext is not utf-8")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Symmetric cipher derived from the application secret.
#[derive(Clone)]
pub struct Cipher {
    inner: Aes256Gcm,
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cipher(..)")
    }
}

impl Cipher {
    /// Derives the key as SHA-256 of `secret`.
    pub fn from_secret(secret: &str) -> Self {
        let key = Sha256::digest(secret.as_bytes());
        Self {
            inner: Aes256Gcm::new(&key),
        }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, CryptoError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let sealed = self
            .inner
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| CryptoError::Encrypt)?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&sealed);
        Ok(hex::encode(out))
    }

    pub fn decrypt(&self, ciphertext_hex: &str) -> Result<String, CryptoError> {
        let bytes = hex::decode(ciphertext_hex)?;
        if bytes.len() < NONCE_LEN {
            return Err(CryptoError::TooShort);
        }
        let (nonce, sealed) = bytes.split_at(NONCE_LEN);
        let plain = self
            .inner
            .decrypt(Nonce::from_slice(nonce), sealed)
            .map_err(|_| CryptoError::Decrypt)?;
        Ok(String::from_utf8(plain)?)
    }
}

/// Returns lowercase hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = Cipher::from_secret("secret");
        let enc = c.encrypt("hello 世界").unwrap();
        assert_ne!(enc, "hello 世界");
        assert_eq!(c.decrypt(&enc).unwrap(), "hello 世界");
    }

    #[test]
    fn wrong_key_fails() {
        let enc = Cipher::from_secret("a").encrypt("x").unwrap();
        assert!(Cipher::from_secret("b").decrypt(&enc).is_err());
    }
}
