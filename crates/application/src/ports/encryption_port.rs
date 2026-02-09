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
    use std::io::Write;
    let mut output = String::new();
    let mut encoder = Base64Encoder::new(&mut output);
    encoder.write_all(data).unwrap_or_default();
    encoder.finish();
    output
}

/// Base64 decode string to bytes
#[allow(clippy::cast_possible_truncation)] // Intentional truncation in base64 decoding
fn base64_decode(data: &str) -> Result<Vec<u8>, Base64DecodeError> {
    let mut output = Vec::with_capacity(data.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for c in data.chars() {
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            '=' => continue, // padding
            _ => return Err(Base64DecodeError::InvalidCharacter(c)),
        };

        buffer = (buffer << 6) | value;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

/// Base64 decoding error
#[derive(Debug)]
pub enum Base64DecodeError {
    /// Invalid character in input
    InvalidCharacter(char),
}

impl std::fmt::Display for Base64DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCharacter(c) => write!(f, "Invalid base64 character: {c}"),
        }
    }
}

impl std::error::Error for Base64DecodeError {}

/// Simple base64 encoder
struct Base64Encoder<'a> {
    output: &'a mut String,
    buffer: u32,
    bits: u8,
}

impl<'a> Base64Encoder<'a> {
    const fn new(output: &'a mut String) -> Self {
        Self {
            output,
            buffer: 0,
            bits: 0,
        }
    }

    #[allow(clippy::cast_possible_truncation)] // Intentional truncation in base64 encoding
    fn finish(mut self) {
        if self.bits > 0 {
            self.buffer <<= 6 - self.bits;
            self.write_char(self.buffer as u8);
            // Add padding
            while self.bits < 6 {
                self.output.push('=');
                self.bits += 2;
            }
        }
    }

    fn write_char(&mut self, value: u8) {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        self.output.push(ALPHABET[(value & 0x3F) as usize] as char);
    }
}

impl std::io::Write for Base64Encoder<'_> {
    #[allow(clippy::cast_possible_truncation)] // Intentional truncation in base64 encoding
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            self.buffer = (self.buffer << 8) | u32::from(byte);
            self.bits += 8;

            while self.bits >= 6 {
                self.bits -= 6;
                self.write_char((self.buffer >> self.bits) as u8);
                self.buffer &= (1 << self.bits) - 1;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
