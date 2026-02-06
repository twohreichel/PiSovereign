//! API key hashing utilities using Argon2
//!
//! Provides secure hashing and verification of API keys using the Argon2id
//! algorithm, which is recommended for password/key hashing due to its
//! resistance to GPU-based attacks and side-channel attacks.
//!
//! # Examples
//!
//! ```
//! use infrastructure::adapters::ApiKeyHasher;
//!
//! let hasher = ApiKeyHasher::new();
//!
//! // Hash an API key
//! let hash = hasher.hash("sk-my-secret-key").unwrap();
//!
//! // Verify the key later
//! assert!(hasher.verify("sk-my-secret-key", &hash).unwrap());
//! assert!(!hasher.verify("wrong-key", &hash).unwrap());
//! ```

use argon2::{
    Argon2, PasswordHash, PasswordHasher as ArgonPasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Errors that can occur during API key hashing operations
#[derive(Debug, Error)]
pub enum ApiKeyHashError {
    /// Failed to hash the API key
    #[error("Failed to hash API key: {0}")]
    HashingFailed(String),

    /// Failed to verify the API key
    #[error("Failed to verify API key: {0}")]
    VerificationFailed(String),

    /// Invalid hash format
    #[error("Invalid hash format: {0}")]
    InvalidHashFormat(String),
}

/// Secure API key hasher using Argon2id
///
/// Uses Argon2id which combines Argon2i's resistance to side-channel attacks
/// with Argon2d's resistance to GPU cracking attacks.
#[derive(Debug, Clone, Default)]
pub struct ApiKeyHasher {
    /// Argon2 configuration (uses sensible defaults)
    _config: Argon2Config,
}

/// Configuration for Argon2 hashing
#[derive(Debug, Clone, Default)]
struct Argon2Config {
    // Using Argon2 defaults which are secure for most use cases
    // Memory: 19 MiB, Iterations: 2, Parallelism: 1
}

impl ApiKeyHasher {
    /// Create a new API key hasher with default configuration
    ///
    /// # Examples
    ///
    /// ```
    /// use infrastructure::adapters::ApiKeyHasher;
    ///
    /// let hasher = ApiKeyHasher::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Hash an API key using Argon2id
    ///
    /// Returns a PHC-formatted string containing the hash, salt, and parameters.
    /// This format is self-describing and portable.
    ///
    /// # Arguments
    ///
    /// * `api_key` - The plaintext API key to hash
    ///
    /// # Returns
    ///
    /// A PHC-formatted hash string that can be stored in configuration.
    ///
    /// # Errors
    ///
    /// Returns `ApiKeyHashError::HashingFailed` if hashing fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use infrastructure::adapters::ApiKeyHasher;
    ///
    /// let hasher = ApiKeyHasher::new();
    /// let hash = hasher.hash("sk-my-secret-key").unwrap();
    /// assert!(hash.starts_with("$argon2"));
    /// ```
    #[instrument(skip(self, api_key))]
    pub fn hash(&self, api_key: &str) -> Result<String, ApiKeyHashError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let hash = argon2
            .hash_password(api_key.as_bytes(), &salt)
            .map_err(|e| ApiKeyHashError::HashingFailed(e.to_string()))?;

        debug!("Successfully hashed API key");
        Ok(hash.to_string())
    }

    /// Verify an API key against a stored hash
    ///
    /// Uses constant-time comparison internally to prevent timing attacks.
    ///
    /// # Arguments
    ///
    /// * `api_key` - The plaintext API key to verify
    /// * `hash` - The stored PHC-formatted hash
    ///
    /// # Returns
    ///
    /// `true` if the key matches, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns `ApiKeyHashError::InvalidHashFormat` if the hash format is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use infrastructure::adapters::ApiKeyHasher;
    ///
    /// let hasher = ApiKeyHasher::new();
    /// let hash = hasher.hash("sk-my-secret-key").unwrap();
    ///
    /// assert!(hasher.verify("sk-my-secret-key", &hash).unwrap());
    /// assert!(!hasher.verify("wrong-key", &hash).unwrap());
    /// ```
    #[instrument(skip(self, api_key, hash))]
    pub fn verify(&self, api_key: &str, hash: &str) -> Result<bool, ApiKeyHashError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| ApiKeyHashError::InvalidHashFormat(e.to_string()))?;

        let argon2 = Argon2::default();
        let result = argon2
            .verify_password(api_key.as_bytes(), &parsed_hash)
            .is_ok();

        if result {
            debug!("API key verification successful");
        } else {
            debug!("API key verification failed");
        }

        Ok(result)
    }

    /// Check if a string looks like a hashed API key (PHC format)
    ///
    /// This is useful for detecting plaintext keys in configuration
    /// that need to be migrated to hashed format.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to check
    ///
    /// # Returns
    ///
    /// `true` if the string appears to be a valid PHC hash, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use infrastructure::adapters::ApiKeyHasher;
    ///
    /// assert!(ApiKeyHasher::is_hashed("$argon2id$v=19$m=19456,t=2,p=1$..."));
    /// assert!(!ApiKeyHasher::is_hashed("sk-plaintext-key"));
    /// ```
    #[must_use]
    pub fn is_hashed(value: &str) -> bool {
        // PHC format starts with $algorithm$
        value.starts_with("$argon2")
    }

    /// Detect plaintext API keys in a configuration map and log warnings
    ///
    /// This is useful during startup to alert administrators about
    /// API keys that should be migrated to hashed format.
    ///
    /// # Arguments
    ///
    /// * `api_keys` - Iterator of API key strings to check
    ///
    /// # Returns
    ///
    /// Number of plaintext keys detected.
    #[must_use]
    pub fn detect_plaintext_keys<'a, I>(api_keys: I) -> usize
    where
        I: Iterator<Item = &'a str>,
    {
        let mut count = 0;
        for key in api_keys {
            if !Self::is_hashed(key) {
                count += 1;
            }
        }

        if count > 0 {
            warn!(
                plaintext_count = count,
                "Detected plaintext API keys in configuration. \
                 Consider migrating to hashed format using 'pisovereign-cli hash-api-key <key>'"
            );
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_creates_valid_phc_format() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-test-key").unwrap();

        assert!(hash.starts_with("$argon2"));
        assert!(hash.contains("$v="));
        assert!(hash.contains("$m="));
    }

    #[test]
    fn verify_correct_key_succeeds() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-test-key").unwrap();

        assert!(hasher.verify("sk-test-key", &hash).unwrap());
    }

    #[test]
    fn verify_wrong_key_fails() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-test-key").unwrap();

        assert!(!hasher.verify("sk-wrong-key", &hash).unwrap());
    }

    #[test]
    fn verify_invalid_hash_returns_error() {
        let hasher = ApiKeyHasher::new();
        let result = hasher.verify("sk-test-key", "invalid-hash");

        assert!(result.is_err());
        assert!(matches!(result, Err(ApiKeyHashError::InvalidHashFormat(_))));
    }

    #[test]
    fn is_hashed_detects_argon2_format() {
        assert!(ApiKeyHasher::is_hashed(
            "$argon2id$v=19$m=19456,t=2,p=1$abc$def"
        ));
        assert!(ApiKeyHasher::is_hashed(
            "$argon2i$v=19$m=65536,t=3,p=4$abc$def"
        ));
        assert!(ApiKeyHasher::is_hashed(
            "$argon2d$v=19$m=65536,t=3,p=4$abc$def"
        ));
    }

    #[test]
    fn is_hashed_rejects_plaintext() {
        assert!(!ApiKeyHasher::is_hashed("sk-plaintext-key"));
        assert!(!ApiKeyHasher::is_hashed("api-key-12345"));
        assert!(!ApiKeyHasher::is_hashed(""));
    }

    #[test]
    fn detect_plaintext_keys_counts_correctly() {
        let keys: &[&str] = &[
            "sk-plaintext-1",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$def",
            "sk-plaintext-2",
        ];

        let count = ApiKeyHasher::detect_plaintext_keys(keys.iter().copied());
        assert_eq!(count, 2);
    }

    #[test]
    fn hash_produces_different_hashes_for_same_input() {
        let hasher = ApiKeyHasher::new();
        let hash1 = hasher.hash("sk-test-key").unwrap();
        let hash2 = hasher.hash("sk-test-key").unwrap();

        // Different salts should produce different hashes
        assert_ne!(hash1, hash2);

        // But both should verify
        assert!(hasher.verify("sk-test-key", &hash1).unwrap());
        assert!(hasher.verify("sk-test-key", &hash2).unwrap());
    }

    #[test]
    fn default_creates_valid_hasher() {
        let hasher = ApiKeyHasher::default();
        let hash = hasher.hash("test").unwrap();
        assert!(hasher.verify("test", &hash).unwrap());
    }
}
