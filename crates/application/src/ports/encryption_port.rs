//! Encryption port - Interface for encrypting and decrypting sensitive data
//!
//! This port defines how the application layer handles encryption of
//! sensitive memory content at rest.

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Port for encrypting and decrypting data
#[cfg_attr(test, automock)]
#[async_trait]
pub trait EncryptionPort: Send + Sync {
    /// Encrypt plaintext data
    ///
    /// Returns the encrypted data as bytes (includes nonce/IV).
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, ApplicationError>;

    /// Decrypt ciphertext data
    ///
    /// Returns the original plaintext data.
    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, ApplicationError>;

    /// Encrypt a string and return base64-encoded ciphertext
    ///
    /// Convenience method for encrypting string content.
    async fn encrypt_string(&self, plaintext: &str) -> Result<String, ApplicationError> {
        let encrypted = self.encrypt(plaintext.as_bytes()).await?;
        Ok(base64_encode(&encrypted))
    }

    /// Decrypt base64-encoded ciphertext and return the original string
    ///
    /// Convenience method for decrypting string content.
    async fn decrypt_string(&self, ciphertext: &str) -> Result<String, ApplicationError> {
        let decoded = base64_decode(ciphertext)
            .map_err(|e| ApplicationError::Internal(format!("Failed to decode base64: {e}")))?;
        let decrypted = self.decrypt(&decoded).await?;
        String::from_utf8(decrypted).map_err(|e| {
            ApplicationError::Internal(format!("Decrypted data is not valid UTF-8: {e}"))
        })
    }

    /// Check if encryption is enabled
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Base64 encode bytes to string
fn base64_encode(data: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(data)
}

/// Base64 decode string to bytes
fn base64_decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.decode(data)
}

/// No-op encryption implementation for when encryption is disabled
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpEncryption;

#[async_trait]
impl EncryptionPort for NoOpEncryption {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, ApplicationError> {
        Ok(plaintext.to_vec())
    }

    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, ApplicationError> {
        Ok(ciphertext.to_vec())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip() {
        let original = b"Hello, World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original.as_slice(), decoded.as_slice());
    }

    #[test]
    fn base64_empty() {
        let original = b"";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn base64_binary_data() {
        let original: Vec<u8> = (0..=255).collect();
        let encoded = base64_encode(&original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn base64_with_padding() {
        // "a" should encode to "YQ=="
        let encoded = base64_encode(b"a");
        assert!(encoded.ends_with("==") || encoded.ends_with('='));
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(b"a".as_slice(), decoded.as_slice());
    }

    #[test]
    fn base64_decode_invalid_char() {
        let result = base64_decode("abc!def");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn noop_encryption_passthrough() {
        let encryption = NoOpEncryption;

        let plaintext = b"test data";
        let encrypted = encryption.encrypt(plaintext).await.unwrap();
        assert_eq!(plaintext.as_slice(), encrypted.as_slice());

        let decrypted = encryption.decrypt(&encrypted).await.unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[tokio::test]
    async fn noop_encryption_string_passthrough() {
        let encryption = NoOpEncryption;

        let plaintext = "Hello, 世界!";
        let encrypted = encryption.encrypt_string(plaintext).await.unwrap();
        let decrypted = encryption.decrypt_string(&encrypted).await.unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn noop_encryption_reports_disabled() {
        let encryption = NoOpEncryption;
        assert!(!encryption.is_enabled());
    }
}
