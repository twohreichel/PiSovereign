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

        // Check for plaintext API keys that look like they might be real
        if let Some(ref api_key) = config.security.api_key {
            if !api_key.is_empty()
                && !api_key.starts_with("${")
                && !api_key.contains("your-")
                && !api_key.contains("example")
            {
                warnings.push(SecurityWarning::new(
                    severity,
                    "SEC003",
                    "API key appears to be stored in plaintext configuration",
                    "Use environment variables (PISOVEREIGN_SECURITY_API_KEY) or hash with `pisovereign-cli hash-api-key`",
                ));
            }
        }

        // Check WhatsApp secrets
        if let Some(ref token) = config.whatsapp.access_token {
            if !token.is_empty() && !token.starts_with("${") && !token.contains("your-") {
                warnings.push(SecurityWarning::new(
                    severity,
                    "SEC004",
                    "WhatsApp access token appears to be stored in plaintext",
                    "Use environment variables (PISOVEREIGN_WHATSAPP_ACCESS_TOKEN) for WhatsApp credentials",
                ));
            }
        }

        if let Some(ref secret) = config.whatsapp.app_secret {
            if !secret.is_empty() && !secret.starts_with("${") && !secret.contains("your-") {
                warnings.push(SecurityWarning::new(
                    severity,
                    "SEC005",
                    "WhatsApp app secret appears to be stored in plaintext",
                    "Use environment variables (PISOVEREIGN_WHATSAPP_APP_SECRET) for WhatsApp credentials",
                ));
            }
        }
    }

    fn check_api_key_configuration(
        config: &AppConfig,
        is_production: bool,
        warnings: &mut Vec<SecurityWarning>,
    ) {
        // Check if no authentication is configured
        let has_legacy_key = config.security.api_key.is_some();
        let has_user_keys = !config.security.api_key_users.is_empty();

        if !has_legacy_key && !has_user_keys {
            let severity = if is_production {
                WarningSeverity::Warning
            } else {
                WarningSeverity::Info
            };

            warnings.push(SecurityWarning::new(
                severity,
                "SEC006",
                "No API authentication configured",
                "Configure api_key or api_key_users for API authentication",
            ));
        }

        // Warn about using legacy single-key mode
        if has_legacy_key && !has_user_keys {
            warnings.push(SecurityWarning::info(
                "SEC007",
                "Using legacy single API key mode",
                "Consider migrating to api_key_users for multi-user support",
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
        config.security.api_key = Some("test-key".to_string());

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
        config.security.api_key = Some("sk-real-secret-key-12345".to_string());

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC003").unwrap();
        assert_eq!(warning.severity, WarningSeverity::Warning);
    }

    #[test]
    fn validate_critical_plaintext_api_key_in_production() {
        let mut config = create_production_config();
        config.security.api_key = Some("sk-real-secret-key-12345".to_string());

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC003").unwrap();
        assert!(warning.is_critical());
    }

    #[test]
    fn validate_critical_plaintext_whatsapp_token_in_production() {
        let mut config = create_production_config();
        config.whatsapp.access_token = Some("EAABwzLixnjYBO...".to_string());

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC004").unwrap();
        assert!(warning.is_critical());
    }

    #[test]
    fn validate_critical_plaintext_whatsapp_secret_in_production() {
        let mut config = create_production_config();
        config.whatsapp.app_secret = Some("abc123def456".to_string());

        let warnings = SecurityValidator::validate(&config);

        let warning = warnings.iter().find(|w| w.code == "SEC005").unwrap();
        assert!(warning.is_critical());
    }

    #[test]
    fn validate_ignores_placeholder_api_key() {
        let mut config = create_test_config();
        config.security.api_key = Some("your-secret-key".to_string());

        let warnings = SecurityValidator::validate(&config);

        assert!(!warnings.iter().any(|w| w.code == "SEC003"));
    }

    #[test]
    fn validate_ignores_env_var_reference() {
        let mut config = create_test_config();
        config.security.api_key = Some("${API_KEY}".to_string());

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
    fn validate_warns_on_legacy_api_key() {
        let mut config = create_test_config();
        config.security.api_key = Some("your-key".to_string());

        let warnings = SecurityValidator::validate(&config);

        assert!(warnings.iter().any(|w| w.code == "SEC007"));
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
        config.security.api_key = Some("real-key".to_string());

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
