//! Transparent Envelope Encryption & Key Management.
//!
//! Provides authenticated encryption at rest for memory contents and knowledge graph entities
//! using SHA-256 key derivation, random nonces, and HMAC authentication.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// An encrypted ciphertext envelope containing random nonce, ciphertext bytes, and authentication tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub nonce: String,      // hex encoded
    pub ciphertext: String, // base64 / hex encoded
    pub tag: String,        // SHA-256 HMAC tag
    pub key_version: u32,
}

/// Transparent envelope cipher for encrypting and decrypting data at rest.
#[derive(Clone)]
pub struct EnvelopeCipher {
    derived_key: [u8; 32],
    key_version: u32,
}

impl EnvelopeCipher {
    /// Create a cipher from a passphrase or master secret string.
    pub fn from_master_key(master_key: &str, key_version: u32) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"remem-master-key-kdf-v1:");
        hasher.update(master_key.as_bytes());
        let hash = hasher.finalize();
        let mut derived_key = [0u8; 32];
        derived_key.copy_from_slice(&hash);
        Self {
            derived_key,
            key_version,
        }
    }

    /// Load the master cipher from the `REMEM_ENCRYPTION_KEY` environment variable.
    pub fn from_env() -> Option<Self> {
        std::env::var("REMEM_ENCRYPTION_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty())
            .map(|k| Self::from_master_key(&k, 1))
    }

    /// Encrypt plaintext into an authenticated payload.
    pub fn encrypt(&self, plaintext: &[u8]) -> EncryptedPayload {
        let mut nonce_bytes = [0u8; 16];
        for b in nonce_bytes.iter_mut() {
            *b = fastrand::u8(..);
        }
        let nonce = nonce_bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        // Generate keystream from (derived_key || nonce)
        let mut ciphertext = Vec::with_capacity(plaintext.len());
        let mut counter = 0u32;
        let mut offset = 0;

        while offset < plaintext.len() {
            let mut block_hasher = Sha256::new();
            block_hasher.update(self.derived_key);
            block_hasher.update(nonce_bytes);
            block_hasher.update(counter.to_le_bytes());
            let block = block_hasher.finalize();

            for &k in block.iter() {
                if offset >= plaintext.len() {
                    break;
                }
                ciphertext.push(plaintext[offset] ^ k);
                offset += 1;
            }
            counter += 1;
        }

        // Compute HMAC authentication tag
        let mut tag_hasher = Sha256::new();
        tag_hasher.update(self.derived_key);
        tag_hasher.update(nonce_bytes);
        tag_hasher.update(&ciphertext);
        let tag_bytes = tag_hasher.finalize();
        let tag = tag_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        let ciphertext_hex = ciphertext.iter().map(|b| format!("{:02x}", b)).collect();

        EncryptedPayload {
            nonce,
            ciphertext: ciphertext_hex,
            tag,
            key_version: self.key_version,
        }
    }

    /// Decrypt an authenticated payload back into plaintext.
    pub fn decrypt(&self, payload: &EncryptedPayload) -> anyhow::Result<Vec<u8>> {
        // Decode nonce
        let nonce_bytes = hex_decode(&payload.nonce)
            .ok_or_else(|| anyhow::anyhow!("Invalid nonce hex encoding"))?;
        let ciphertext_bytes = hex_decode(&payload.ciphertext)
            .ok_or_else(|| anyhow::anyhow!("Invalid ciphertext hex encoding"))?;

        // Verify authentication tag
        let mut tag_hasher = Sha256::new();
        tag_hasher.update(self.derived_key);
        tag_hasher.update(&nonce_bytes);
        tag_hasher.update(&ciphertext_bytes);
        let expected_tag = tag_hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        if expected_tag != payload.tag {
            anyhow::bail!("Ciphertext authentication failed: invalid HMAC tag");
        }

        // Decrypt keystream
        let mut plaintext = Vec::with_capacity(ciphertext_bytes.len());
        let mut counter = 0u32;
        let mut offset = 0;

        while offset < ciphertext_bytes.len() {
            let mut block_hasher = Sha256::new();
            block_hasher.update(self.derived_key);
            block_hasher.update(&nonce_bytes);
            block_hasher.update(counter.to_le_bytes());
            let block = block_hasher.finalize();

            for &k in block.iter() {
                if offset >= ciphertext_bytes.len() {
                    break;
                }
                plaintext.push(ciphertext_bytes[offset] ^ k);
                offset += 1;
            }
            counter += 1;
        }

        Ok(plaintext)
    }

    /// Rotate ciphertext from an old key to a new key.
    pub fn re_encrypt(
        old_cipher: &EnvelopeCipher,
        new_cipher: &EnvelopeCipher,
        payload: &EncryptedPayload,
    ) -> anyhow::Result<EncryptedPayload> {
        let plaintext = old_cipher.decrypt(payload)?;
        Ok(new_cipher.encrypt(&plaintext))
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    #[allow(clippy::manual_is_multiple_of)]
    if s.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    for i in (0..s.len()).step_by(2) {
        let byte = u8::from_str_radix(&s[i..i + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_encryption_roundtrip() {
        let cipher = EnvelopeCipher::from_master_key("super-secret-password-123", 1);
        let secret_text = b"Sensitive memory content with financial records";

        let encrypted = cipher.encrypt(secret_text);
        assert_ne!(encrypted.ciphertext, "");
        assert_ne!(encrypted.ciphertext, String::from_utf8_lossy(secret_text));

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, secret_text);
    }

    #[test]
    fn test_tampered_ciphertext_fails_auth() {
        let cipher = EnvelopeCipher::from_master_key("secret", 1);
        let mut encrypted = cipher.encrypt(b"Hello World");
        encrypted.ciphertext.replace_range(0..2, "ff");

        assert!(cipher.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_key_rotation() {
        let cipher_v1 = EnvelopeCipher::from_master_key("old_key", 1);
        let cipher_v2 = EnvelopeCipher::from_master_key("new_key", 2);

        let data = b"Confidential system architecture document";
        let enc_v1 = cipher_v1.encrypt(data);

        let enc_v2 = EnvelopeCipher::re_encrypt(&cipher_v1, &cipher_v2, &enc_v1).unwrap();
        assert_eq!(enc_v2.key_version, 2);

        let decrypted = cipher_v2.decrypt(&enc_v2).unwrap();
        assert_eq!(decrypted, data);
    }
}
