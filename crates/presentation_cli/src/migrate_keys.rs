//! API key migration utilities
//!
//! Converts plaintext API keys in configuration files to secure Argon2id hashes.
//! This module supports batch migration with dry-run capability.
//!
//! # Usage
//!
//! ```bash
//! # Dry-run to see what would change
//! pisovereign-cli migrate-keys --input config.toml --dry-run
//!
//! # Actually migrate the keys
//! pisovereign-cli migrate-keys --input config.toml --output config.toml
//! ```

use std::fs;
use std::path::Path;

use infrastructure::{ApiKeyEntry, ApiKeyHasher};
use thiserror::Error;

/// Errors that can occur during key migration
#[derive(Debug, Error)]
pub enum MigrationError {
    /// Failed to read input file
    #[error("Failed to read input file: {0}")]
    ReadError(#[from] std::io::Error),

    /// Failed to parse TOML
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),

    /// Failed to serialize TOML
    #[error("Failed to serialize TOML: {0}")]
    SerializeError(#[from] toml::ser::Error),
}

/// Result of the migration process
#[derive(Debug)]
pub struct MigrationResult {
    /// Number of keys that were already hashed (no change needed)
    pub already_hashed: usize,
    /// Number of keys that were migrated from plaintext to hashed
    pub migrated: usize,
    /// Number of keys that failed to migrate
    pub failed: usize,
    /// The migrated configuration content
    pub output: String,
}

/// Migrate API keys from plaintext to hashed format
///
/// # Arguments
///
/// * `input_path` - Path to the input configuration file
/// * `dry_run` - If true, don't write output, just show what would change
///
/// # Returns
///
/// Migration result with statistics and the migrated content
pub fn migrate_config(input_path: &Path, dry_run: bool) -> Result<MigrationResult, MigrationError> {
    let content = fs::read_to_string(input_path)?;
    let mut config: toml::Table = toml::from_str(&content)?;

    let hasher = ApiKeyHasher::new();
    let mut already_hashed = 0;
    let mut migrated = 0;
    let mut failed = 0;
    let mut new_api_keys: Vec<ApiKeyEntry> = Vec::new();

    // Extract security section if it exists
    if let Some(security) = config.get_mut("security") {
        if let Some(security_table) = security.as_table_mut() {
            // Check for legacy api_key (single key mode)
            if let Some(api_key) = security_table.remove("api_key") {
                if let Some(key_str) = api_key.as_str() {
                    if !key_str.is_empty() && !key_str.starts_with("$argon2") {
                        // Generate a default user ID for legacy single-key mode
                        let default_user_id = "00000000-0000-0000-0000-000000000001";
                        match hasher.hash(key_str) {
                            Ok(hash) => {
                                new_api_keys.push(ApiKeyEntry {
                                    hash,
                                    user_id: default_user_id.to_string(),
                                });
                                migrated += 1;
                                println!(
                                    "  ✅ Migrated legacy api_key → api_keys[0] (user_id: {})",
                                    default_user_id
                                );
                            },
                            Err(e) => {
                                failed += 1;
                                println!("  ❌ Failed to hash legacy api_key: {e}");
                            },
                        }
                    }
                }
            }

            // Check for legacy api_key_users map
            if let Some(api_key_users) = security_table.remove("api_key_users") {
                if let Some(users_table) = api_key_users.as_table() {
                    for (key, user_id) in users_table {
                        if let Some(user_id_str) = user_id.as_str() {
                            if ApiKeyHasher::is_hashed(key) {
                                // Already hashed, preserve as-is
                                new_api_keys.push(ApiKeyEntry {
                                    hash: key.clone(),
                                    user_id: user_id_str.to_string(),
                                });
                                already_hashed += 1;
                                println!("  ⏭️  Already hashed: {}", &key[..20.min(key.len())]);
                            } else {
                                // Plaintext key, hash it
                                match hasher.hash(key) {
                                    Ok(hash) => {
                                        new_api_keys.push(ApiKeyEntry {
                                            hash,
                                            user_id: user_id_str.to_string(),
                                        });
                                        migrated += 1;
                                        println!(
                                            "  ✅ Migrated api_key_users[{}...] → api_keys (user_id: {})",
                                            &key[..8.min(key.len())],
                                            user_id_str
                                        );
                                    },
                                    Err(e) => {
                                        failed += 1;
                                        println!(
                                            "  ❌ Failed to hash key {}...: {}",
                                            &key[..8.min(key.len())],
                                            e
                                        );
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Check for existing api_keys array (new format)
            if let Some(existing_keys) = security_table.get("api_keys") {
                if let Some(keys_array) = existing_keys.as_array() {
                    for key_entry in keys_array {
                        if let Some(entry_table) = key_entry.as_table() {
                            let hash = entry_table
                                .get("hash")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            let user_id = entry_table
                                .get("user_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();

                            if ApiKeyHasher::is_hashed(hash) {
                                // Already properly hashed
                                new_api_keys.push(ApiKeyEntry {
                                    hash: hash.to_string(),
                                    user_id: user_id.to_string(),
                                });
                                already_hashed += 1;
                                println!("  ⏭️  Already hashed: {}", &hash[..20.min(hash.len())]);
                            } else if !hash.is_empty() {
                                // Hash field exists but contains plaintext (unusual)
                                match hasher.hash(hash) {
                                    Ok(new_hash) => {
                                        new_api_keys.push(ApiKeyEntry {
                                            hash: new_hash,
                                            user_id: user_id.to_string(),
                                        });
                                        migrated += 1;
                                        println!(
                                            "  ✅ Migrated plaintext hash → proper hash (user_id: {})",
                                            user_id
                                        );
                                    },
                                    Err(e) => {
                                        failed += 1;
                                        println!("  ❌ Failed to hash: {e}");
                                    },
                                }
                            }
                        }
                    }
                }
                // Remove old api_keys to replace with new
                security_table.remove("api_keys");
            }

            // Add new api_keys array if we have any
            if !new_api_keys.is_empty() {
                let keys_array: Vec<toml::Value> = new_api_keys
                    .iter()
                    .map(|entry| {
                        let mut table = toml::Table::new();
                        table.insert("hash".to_string(), toml::Value::String(entry.hash.clone()));
                        table.insert(
                            "user_id".to_string(),
                            toml::Value::String(entry.user_id.clone()),
                        );
                        toml::Value::Table(table)
                    })
                    .collect();
                security_table.insert("api_keys".to_string(), toml::Value::Array(keys_array));
            }
        }
    }

    let output = toml::to_string_pretty(&config)?;

    if dry_run {
        println!("\n📋 Dry run - no changes written");
    }

    Ok(MigrationResult {
        already_hashed,
        migrated,
        failed,
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_config(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn migrate_legacy_api_key() {
        let config = r#"
[security]
api_key = "sk-secret-key-12345"
rate_limit_enabled = true
"#;
        let file = create_temp_config(config);
        let result = migrate_config(file.path(), true).unwrap();

        assert_eq!(result.migrated, 1);
        assert_eq!(result.already_hashed, 0);
        assert_eq!(result.failed, 0);
        assert!(result.output.contains("api_keys"));
        assert!(result.output.contains("$argon2"));
        assert!(!result.output.contains("api_key ="));
    }

    #[test]
    fn migrate_api_key_users() {
        let config = r#"
[security]
rate_limit_enabled = true

[security.api_key_users]
"sk-user1" = "550e8400-e29b-41d4-a716-446655440001"
"sk-user2" = "550e8400-e29b-41d4-a716-446655440002"
"#;
        let file = create_temp_config(config);
        let result = migrate_config(file.path(), true).unwrap();

        assert_eq!(result.migrated, 2);
        assert_eq!(result.already_hashed, 0);
        assert_eq!(result.failed, 0);
        assert!(result.output.contains("api_keys"));
        assert!(result.output.contains("$argon2"));
        assert!(!result.output.contains("api_key_users"));
    }

    #[test]
    fn skip_already_hashed_keys() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-test").unwrap();

        let config = format!(
            r#"
[security]
rate_limit_enabled = true

[[security.api_keys]]
hash = "{}"
user_id = "550e8400-e29b-41d4-a716-446655440001"
"#,
            hash
        );
        let file = create_temp_config(&config);
        let result = migrate_config(file.path(), true).unwrap();

        assert_eq!(result.migrated, 0);
        assert_eq!(result.already_hashed, 1);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn preserve_other_config_sections() {
        let config = r#"
[server]
host = "0.0.0.0"
port = 3000

[security]
api_key = "sk-test"
rate_limit_enabled = true
rate_limit_rpm = 60

[inference]
base_url = "http://localhost:11434"
"#;
        let file = create_temp_config(config);
        let result = migrate_config(file.path(), true).unwrap();

        assert!(result.output.contains("[server]"));
        assert!(result.output.contains("host = \"0.0.0.0\""));
        assert!(result.output.contains("[inference]"));
        assert!(result.output.contains("rate_limit_enabled = true"));
    }

    #[test]
    fn handle_empty_security_section() {
        let config = r#"
[security]
rate_limit_enabled = true
"#;
        let file = create_temp_config(config);
        let result = migrate_config(file.path(), true).unwrap();

        assert_eq!(result.migrated, 0);
        assert_eq!(result.already_hashed, 0);
        assert_eq!(result.failed, 0);
    }
}
