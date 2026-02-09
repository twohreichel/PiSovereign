//! ChaCha20Poly1305 encryption adapter
//!
//! Provides authenticated encryption for sensitive memory content using
//! the XChaCha20-Poly1305 AEAD cipher.

use std::path::Path;

use application::{error::ApplicationError, ports::EncryptionPort};
use async_trait::async_trait;
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use tracing::{debug, instrument, warn};

/// Nonce size for XChaCha20-Poly1305 (24 bytes)
const NONCE_SIZE: usize = 24;

/// Key size for XChaCha20-Poly1305 (256 bits = 32 bytes)
const KEY_SIZE: usize = 32;

/// XChaCha20-Poly1305 encryption adapter
///
/// Uses XChaCha20-Poly1305 for authenticated encryption with:
/// - 256-bit key
/// - 192-bit nonce (safer than ChaCha20's 96-bit nonce)
/// - Poly1305 MAC for authentication
pub struct ChaChaEncryptionAdapter {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for ChaChaEncryptionAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChaChaEncryptionAdapter")
            .field("cipher", &"[XChaCha20Poly1305]")
            .finish()
    }
}

impl ChaChaEncryptionAdapter {
    /// Create a new encryption adapter with the given key
    ///
    /// # Errors
    ///
    /// Returns an error if the key is not exactly 32 bytes.
    pub fn new(key: &[u8]) -> Result<Self, ApplicationError> {
        if key.len() != KEY_SIZE {
            return Err(ApplicationError::Configuration(format!(
                "Encryption key must be {KEY_SIZE} bytes, got {}",
                key.len()
            )));
        }

        let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| {
            ApplicationError::Configuration(format!("Invalid encryption key: {e}"))
        })?;

        debug!("Initialized ChaCha20Poly1305 encryption adapter");

        Ok(Self { cipher })
    }

    /// Create from a key file
    ///
    /// The key file should contain exactly 32 bytes of random data.
    pub fn from_key_file(path: &Path) -> Result<Self, ApplicationError> {
        let key = std::fs::read(path).map_err(|e| {
            ApplicationError::Configuration(format!(
                "Failed to read encryption key file '{}': {e}",
                path.display()
            ))
        })?;

        Self::new(&key)
    }

    /// Generate a new random encryption key
    #[must_use]
    pub fn generate_key() -> [u8; KEY_SIZE] {
        let mut key = [0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key);
        key
    }

    /// Generate a random nonce
    fn generate_nonce() -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce);
        nonce
    }
}

#[async_trait]
impl EncryptionPort for ChaChaEncryptionAdapter {
    #[instrument(skip(self, plaintext), fields(plaintext_len = plaintext.len()))]
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, ApplicationError> {
        // Generate a random nonce
        let nonce = Self::generate_nonce();
        let nonce_arr = chacha20poly1305::XNonce::from_slice(&nonce);

        // Encrypt the plaintext
        let ciphertext = self.cipher.encrypt(nonce_arr, plaintext).map_err(|e| {
            warn!(error = %e, "Encryption failed");
            ApplicationError::Internal(format!("Encryption failed: {e}"))
        })?;

        // Prepend nonce to ciphertext (nonce || ciphertext)
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&ciphertext);

        debug!(
            ciphertext_len = result.len(),
            "Successfully encrypted data"
        );

        Ok(result)
    }

    #[instrument(skip(self, ciphertext), fields(ciphertext_len = ciphertext.len()))]
    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, ApplicationError> {
        if ciphertext.len() < NONCE_SIZE {
            return Err(ApplicationError::Internal(
                "Ciphertext too short - missing nonce".to_string(),
            ));
        }

        // Extract nonce and ciphertext
        let (nonce_bytes, encrypted) = ciphertext.split_at(NONCE_SIZE);
        let nonce = chacha20poly1305::XNonce::from_slice(nonce_bytes);

        // Decrypt
        let plaintext = self.cipher.decrypt(nonce, encrypted).map_err(|e| {
            warn!(error = %e, "Decryption failed - data may be corrupted or key mismatch");
            ApplicationError::Internal(format!("Decryption failed: {e}"))
        })?;

        debug!(
            plaintext_len = plaintext.len(),
            "Successfully decrypted data"
        );

        Ok(plaintext)
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_adapter() -> ChaChaEncryptionAdapter {
        let key = ChaChaEncryptionAdapter::generate_key();
        ChaChaEncryptionAdapter::new(&key).unwrap()
    }

    #[tokio::test]
    async fn encrypt_decrypt_roundtrip() {
        let adapter = create_test_adapter();
        let plaintext = b"Hello, World!";

        let encrypted = adapter.encrypt(plaintext).await.unwrap();
        let decrypted = adapter.decrypt(&encrypted).await.unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn encrypt_produces_different_ciphertext() {
        let adapter = create_test_adapter();
        let plaintext = b"Same message";

        // Due to random nonce, each encryption should produce different output
        let encrypted1 = adapter.encrypt(plaintext).await.unwrap();
        let encrypted2 = adapter.encrypt(plaintext).await.unwrap();

        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same plaintext
        let decrypted1 = adapter.decrypt(&encrypted1).await.unwrap();
        let decrypted2 = adapter.decrypt(&encrypted2).await.unwrap();

        assert_eq!(decrypted1, decrypted2);
    }

    #[tokio::test]
    async fn decrypt_detects_tampering() {
        let adapter = create_test_adapter();
        let plaintext = b"Secret message";

        let mut encrypted = adapter.encrypt(plaintext).await.unwrap();

        // Tamper with the ciphertext
        if let Some(byte) = encrypted.last_mut() {
            *byte ^= 0xFF;
        }

        // Decryption should fail due to authentication
        let result = adapter.decrypt(&encrypted).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn decrypt_rejects_short_input() {
        let adapter = create_test_adapter();

        // Input shorter than nonce size
        let result = adapter.decrypt(&[0u8; 10]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn encrypt_empty_data() {
        let adapter = create_test_adapter();
        let plaintext = b"";

        let encrypted = adapter.encrypt(plaintext).await.unwrap();
        let decrypted = adapter.decrypt(&encrypted).await.unwrap();

        assert!(decrypted.is_empty());
    }

    #[tokio::test]
    async fn encrypt_large_data() {
        let adapter = create_test_adapter();
        let plaintext = vec![0xABu8; 1024 * 1024]; // 1 MB

        let encrypted = adapter.encrypt(&plaintext).await.unwrap();
        let decrypted = adapter.decrypt(&encrypted).await.unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn generate_key_is_random() {
        let key1 = ChaChaEncryptionAdapter::generate_key();
        let key2 = ChaChaEncryptionAdapter::generate_key();

        assert_ne!(key1, key2);
        assert_eq!(key1.len(), KEY_SIZE);
    }

    #[test]
    fn rejects_invalid_key_size() {
        let short_key = [0u8; 16];
        let result = ChaChaEncryptionAdapter::new(&short_key);
        assert!(result.is_err());

        let long_key = [0u8; 64];
        let result = ChaChaEncryptionAdapter::new(&long_key);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn string_encryption_roundtrip() {
        let adapter = create_test_adapter();
        let plaintext = "Hello, 世界! 🌍";

        let encrypted = adapter.encrypt_string(plaintext).await.unwrap();
        let decrypted = adapter.decrypt_string(&encrypted).await.unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn is_enabled_returns_true() {
        let adapter = create_test_adapter();
        assert!(adapter.is_enabled());
    }
}
