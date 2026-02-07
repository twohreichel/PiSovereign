//! Security validation for application configuration
//!
//! Validates configuration for security issues and provides warnings at startup.
//! Critical issues in production will prevent startup unless explicitly allowed.

use crate::config::{AppConfig, Environment};
use std::fmt;

/// Severity level for security warnings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarningSeverity {
    /// Informational - no action required
    Info,
    /// Warning - should be addressed but not critical
    Warning,
    /// Critical - must be addressed in production
    Critical,
}

impl fmt::Display for WarningSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A security warning with severity and description
#[derive(Debug, Clone)]
pub struct SecurityWarning {
    /// Severity level of the warning
    pub severity: WarningSeverity,
    /// Short code identifying the warning type
    pub code: String,
    /// Human-readable description of the issue
    pub message: String,
    /// Recommended action to resolve the issue
    pub recommendation: String,
}

impl SecurityWarning {
    /// Create a new security warning
    #[must_use]
    pub fn new(
        severity: WarningSeverity,
        code: impl Into<String>,
        message: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            recommendation: recommendation.into(),
        }
    }

    /// Create a critical warning
    #[must_use]
    pub fn critical(
        code: impl Into<String>,
        message: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self::new(WarningSeverity::Critical, code, message, recommendation)
    }

    /// Create a warning-level issue
    #[must_use]
    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self::new(WarningSeverity::Warning, code, message, recommendation)
    }

    /// Create an informational notice
    #[must_use]
    pub fn info(
        code: impl Into<String>,
        message: impl Into<String>,
        recommendation: impl Into<String>,
    ) -> Self {
        Self::new(WarningSeverity::Info, code, message, recommendation)
    }

    /// Check if this warning is critical
    #[must_use]
    pub const fn is_critical(&self) -> bool {
        matches!(self.severity, WarningSeverity::Critical)
    }
}

impl fmt::Display for SecurityWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {} - {}",
            self.severity, self.code, self.message, self.recommendation
        )
    }
}

/// Validates application configuration for security issues
#[derive(Debug, Clone, Copy, Default)]
pub struct SecurityValidator;

impl SecurityValidator {
    /// Validate configuration and return all security warnings
    ///
    /// Returns a list of warnings sorted by severity (critical first).
    #[must_use]
    pub fn validate(config: &AppConfig) -> Vec<SecurityWarning> {
        let mut warnings = Vec::new();
        let is_production = config.environment == Some(Environment::Production);

        // Check TLS certificate verification
        Self::check_tls_verification(config, is_production, &mut warnings);

        // Check CORS configuration
        Self::check_cors_configuration(config, is_production, &mut warnings);

        // Check for plaintext secrets
        Self::check_plaintext_secrets(config, is_production, &mut warnings);

        // Check API key configuration
        Self::check_api_key_configuration(config, is_production, &mut warnings);

        // Check rate limiting
        Self::check_rate_limiting(config, is_production, &mut warnings);

        // Check database configuration
        Self::check_database_configuration(config, is_production, &mut warnings);

        // Sort by severity (critical first)
        warnings.sort_by(|a, b| b.severity.cmp(&a.severity));

        warnings
    }

    /// Check if startup should be blocked due to critical security issues
    ///
    /// Returns `true` if the server should refuse to start.
    #[must_use]
    pub fn should_block_startup(config: &AppConfig, warnings: &[SecurityWarning]) -> bool {
        let is_production = config.environment == Some(Environment::Production);
        let has_critical = warnings.iter().any(SecurityWarning::is_critical);
        let allow_insecure = std::env::var("PISOVEREIGN_ALLOW_INSECURE_CONFIG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        is_production && has_critical && !allow_insecure
    }

    /// Log all warnings using tracing
    pub fn log_warnings(warnings: &[SecurityWarning]) {
        for warning in warnings {
            match warning.severity {
                WarningSeverity::Critical => {
                    tracing::error!(
                        code = %warning.code,
                        message = %warning.message,
                        recommendation = %warning.recommendation,
                        "Security configuration issue"
                    );
                },
                WarningSeverity::Warning => {
                    tracing::warn!(
                        code = %warning.code,
                        message = %warning.message,
                        recommendation = %warning.recommendation,
                        "Security configuration warning"
                    );
                },
                WarningSeverity::Info => {
                    tracing::info!(
                        code = %warning.code,
                        message = %warning.message,
                        recommendation = %warning.recommendation,
                        "Security configuration notice"
                    );
                },
            }
        }
    }

    fn check_tls_verification(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        if !config.security.tls_verify_certs {
            let severity = if is_production {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Warning
            };

            warnings.push(SecurityWarning::new(
                severity,
                "SEC001",
                "TLS certificate verification is disabled",
                "Enable tls_verify_certs in production to prevent MITM attacks",
            ));
        }
    }

    fn check_cors_configuration(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        if config.server.cors_enabled && config.server.allowed_origins.is_empty() {
            let severity = if is_production {
                WarningSeverity::Critical
            } else {
                WarningSeverity::Info
            };

            warnings.push(SecurityWarning::new(
                severity,
                "SEC002",
                "CORS is enabled with no origin restrictions (allows all origins)",
                "Specify allowed_origins in production to restrict cross-origin requests",
            ));
        }
    }

    fn check_plaintext_secrets(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        // Severity is Critical in production to block startup with plaintext secrets
        let severity = if is_production {
            WarningSeverity::Critical
        } else {
            WarningSeverity::Warning
        };

        // Check for plaintext API keys (not properly hashed with Argon2)
        let plaintext_count = config.security.count_plaintext_keys();
        if plaintext_count > 0 {
            warnings.push(SecurityWarning::new(
                severity,
                "SEC003",
                &format!("{plaintext_count} API key(s) are not properly hashed with Argon2"),
                "Run `pisovereign-cli migrate-keys` to convert plaintext keys to secure hashes",
            ));
        }

        // Check WhatsApp secrets - SecretString handles zeroization but we still
        // want to warn if they're configured (they can't be hashed, just protected)
        if config.whatsapp.access_token.is_some() && is_production {
            warnings.push(SecurityWarning::info(
                "SEC004",
                "WhatsApp access token is configured",
                "Ensure token is loaded from environment variables for enhanced security",
            ));
        }
    }

    fn check_api_key_configuration(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        // Check if no authentication is configured
        let has_api_keys = config.security.has_api_keys();

        if !has_api_keys {
            let severity = if is_production {
                WarningSeverity::Warning
            } else {
                WarningSeverity::Info
            };

            warnings.push(SecurityWarning::new(
                severity,
                "SEC006",
                "No API authentication configured",
                "Configure security.api_keys for API authentication",
            ));
        }
    }

    fn check_rate_limiting(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        if !config.security.rate_limit_enabled && is_production {
            warnings.push(SecurityWarning::warning(
                "SEC008",
                "Rate limiting is disabled in production",
                "Enable rate_limit_enabled to protect against abuse",
            ));
        }
    }

    fn check_database_configuration(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        // Warn about auto-migrations in production
        if config.database.run_migrations && is_production {
            warnings.push(SecurityWarning::info(
                "SEC009",
                "Database migrations run automatically on startup",
                "Consider running migrations manually in production for better control",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> AppConfig {
        AppConfig::default()
    }

    fn create_production_config() -> AppConfig {
        AppConfig {
            environment: Some(Environment::Production),
            ..Default::default()
        }
    }

    #[test]
    fn validate_returns_empty_for_secure_config() {
        let mut config = create_test_config();
        config.server.allowed_origins = vec!["https://example.com".to_string()];
        // Use hashed API key in new format
        config.security.api_keys = vec![crate::ApiKeyEntry {
            hash: "$argon2id$v=19$m=19456,t=2,p=1$test".to_string(),
            user_id: "test-user".to_string(),
        }];

        let warnings = SecurityValidator::validate(&config);

        // Should only have info-level warnings
        assert!(warnings.iter().all(|w| !w.is_critical()));
    }

    #[test]
    fn validate_warns_on_tls_disabled() {
        let mut config = create_test_config();
        config.security.tls_verify_certs = false;

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC001"));
    }

    #[test]
    fn validate_critical_tls_in_production() {
        let mut config = create_production_config();
        config.security.tls_verify_certs = false;

        let warnings = SecurityValidator::validate(&config);

        let tls_warning = warnings.iter().find(|w| w.code == "SEC001").unwrap();
        assert!(tls_warning.is_critical());
    }

    #[test]
    fn validate_warns_on_empty_cors_origins() {
        let config = create_test_config();

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC002"));
    }

    #[test]
    fn validate_critical_cors_in_production() {
        let config = create_production_config();

        let warnings = SecurityValidator::validate(&config);

        let cors_warning = warnings.iter().find(|w| w.code == "SEC002").unwrap();
        assert!(cors_warning.is_critical());
    }

    #[test]
    fn validate_warns_on_plaintext_api_key() {
        let mut config = create_test_config();
        config.security.api_keys.push(crate::config::ApiKeyEntry {
            hash: "plaintext-not-hashed".to_string(),
            user_id: "user1".to_string(),
        });

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC003").unwrap();
        assert_eq!(warning.severity, WarningSeverity::Warning);
    }

    #[test]
    fn validate_critical_plaintext_api_key_in_production() {
        let mut config = create_production_config();
        config.security.api_keys.push(crate::config::ApiKeyEntry {
            hash: "plaintext-not-hashed".to_string(),
            user_id: "user1".to_string(),
        });

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC003").unwrap();
        assert!(warning.is_critical());
    }

    #[test]
    fn validate_no_warning_for_hashed_api_key() {
        let mut config = create_test_config();
        config.security.api_keys.push(crate::config::ApiKeyEntry {
            hash: "$argon2id$v=19$m=19456,t=2,p=1$abc$def".to_string(),
            user_id: "user1".to_string(),
        });

        let warnings = SecurityValidator::validate(&config);

        assert!(!warnings.iter().any(|w| w.code == "SEC003"));
    }

    #[test]
    fn validate_warns_on_no_auth() {
        let config = create_test_config();

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC006"));
    }

    #[test]
    fn validate_no_warning_when_api_keys_configured() {
        let mut config = create_test_config();
        config.security.api_keys.push(crate::config::ApiKeyEntry {
            hash: "$argon2id$v=19$m=19456,t=2,p=1$abc$def".to_string(),
            user_id: "user1".to_string(),
        });

        let warnings = SecurityValidator::validate(&config);

        assert!(!warnings.iter().any(|w| w.code == "SEC006"));
    }

    #[test]
    fn validate_warns_on_disabled_rate_limit_in_production() {
        let mut config = create_production_config();
        config.security.rate_limit_enabled = false;

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC008"));
    }

    #[test]
    fn validate_warns_on_auto_migrations_in_production() {
        let config = create_production_config();

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC009"));
    }

    #[test]
    fn should_block_startup_in_production_with_critical() {
        let config = create_production_config();
        let warnings = vec![SecurityWarning::critical("TEST", "Test critical", "Fix it")];

        assert!(SecurityValidator::should_block_startup(&config, &warnings));
    }

    #[test]
    fn should_not_block_startup_in_development() {
        let config = create_test_config();
        let warnings = vec![SecurityWarning::critical("TEST", "Test critical", "Fix it")];

        assert!(!SecurityValidator::should_block_startup(&config, &warnings));
    }

    #[test]
    fn should_not_block_startup_without_critical() {
        let config = create_production_config();
        let warnings = vec![SecurityWarning::warning("TEST", "Test warning", "Fix it")];

        assert!(!SecurityValidator::should_block_startup(&config, &warnings));
    }

    #[test]
    fn warnings_sorted_by_severity() {
        let mut config = create_production_config();
        config.security.tls_verify_certs = false;
        config.security.api_keys.push(crate::config::ApiKeyEntry {
            hash: "plaintext-key".to_string(),
            user_id: "user1".to_string(),
        });

        let warnings = SecurityValidator::validate(&config);

        // First warnings should be critical
        if warnings.len() > 1 {
            assert!(warnings[0].severity >= warnings[1].severity);
        }
    }

    #[test]
    fn warning_display_format() {
        let warning = SecurityWarning::critical("SEC001", "Test message", "Test recommendation");

        let display = format!("{warning}");

        assert!(display.contains("CRITICAL"));
        assert!(display.contains("SEC001"));
        assert!(display.contains("Test message"));
        assert!(display.contains("Test recommendation"));
    }

    #[test]
    fn severity_ordering() {
        assert!(WarningSeverity::Critical > WarningSeverity::Warning);
        assert!(WarningSeverity::Warning > WarningSeverity::Info);
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", WarningSeverity::Critical), "CRITICAL");
        assert_eq!(format!("{}", WarningSeverity::Warning), "WARNING");
        assert_eq!(format!("{}", WarningSeverity::Info), "INFO");
    }
}
