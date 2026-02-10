//! Integration tests for Proton Mail integration
//!
//! Tests cover:
//! - Configuration validation
//! - TLS configuration
//! - Email composition and validation
//! - IMAP/SMTP client construction
//! - Email summary parsing

#![allow(
    clippy::redundant_clone,
    clippy::implicit_clone,
    clippy::panic,
    clippy::missing_const_for_fn
)]

use integration_proton::{
    EmailComposition, EmailSummary, ProtonBridgeClient, ProtonConfig, ProtonError, TlsConfig,
};
use std::path::PathBuf;

// ============================================================================
// TlsConfig Tests
// ============================================================================

mod tls_config_tests {
    use super::*;

    #[test]
    fn default_tls_config_verifies_certificates() {
        let config = TlsConfig::default();
        assert!(config.should_verify());
        assert!(config.verify_certificates.is_none());
        assert!(config.ca_cert_path.is_none());
        assert_eq!(config.min_tls_version, "1.2");
    }

    #[test]
    fn insecure_tls_config_disables_verification() {
        let config = TlsConfig::insecure();
        assert!(!config.should_verify());
        assert_eq!(config.verify_certificates, Some(false));
    }

    #[test]
    fn strict_tls_config_enables_verification() {
        let config = TlsConfig::strict();
        assert!(config.should_verify());
        assert_eq!(config.verify_certificates, Some(true));
    }

    #[test]
    fn tls_config_with_ca_cert() {
        let config = TlsConfig::with_ca_cert("/path/to/ca.pem");
        assert!(config.should_verify());
        assert_eq!(config.ca_cert_path, Some(PathBuf::from("/path/to/ca.pem")));
    }

    #[test]
    fn should_verify_returns_true_for_none() {
        let config = TlsConfig {
            verify_certificates: None,
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        };
        assert!(config.should_verify());
    }

    #[test]
    fn should_verify_returns_true_for_explicit_true() {
        let config = TlsConfig {
            verify_certificates: Some(true),
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        };
        assert!(config.should_verify());
    }

    #[test]
    fn should_verify_returns_false_for_explicit_false() {
        let config = TlsConfig {
            verify_certificates: Some(false),
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        };
        assert!(!config.should_verify());
    }

    #[test]
    fn tls_config_serialization_roundtrip() {
        let config = TlsConfig {
            verify_certificates: Some(true),
            ca_cert_path: Some(PathBuf::from("/tmp/ca.pem")),
            min_tls_version: "1.3".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TlsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.verify_certificates, deserialized.verify_certificates);
        assert_eq!(config.ca_cert_path, deserialized.ca_cert_path);
        assert_eq!(config.min_tls_version, deserialized.min_tls_version);
    }
}

// ============================================================================
// ProtonConfig Tests
// ============================================================================

mod proton_config_tests {
    use super::*;

    #[test]
    fn default_config_has_localhost_defaults() {
        let config = ProtonConfig::default();
        assert_eq!(config.imap_host, "127.0.0.1");
        assert_eq!(config.imap_port, 1143);
        assert_eq!(config.smtp_host, "127.0.0.1");
        assert_eq!(config.smtp_port, 1025);
        assert!(config.email.is_empty());
        assert!(config.password.is_empty());
    }

    #[test]
    fn with_credentials_creates_config() {
        let config = ProtonConfig::with_credentials("test@proton.me", "bridge-password");
        assert_eq!(config.email, "test@proton.me");
        assert_eq!(config.password, "bridge-password");
        assert_eq!(config.imap_port, 1143); // defaults
        assert_eq!(config.smtp_port, 1025);
    }

    #[test]
    fn with_imap_sets_imap_details() {
        let config = ProtonConfig::with_credentials("test@proton.me", "pass")
            .with_imap("192.168.1.100", 993);

        assert_eq!(config.imap_host, "192.168.1.100");
        assert_eq!(config.imap_port, 993);
    }

    #[test]
    fn with_smtp_sets_smtp_details() {
        let config =
            ProtonConfig::with_credentials("test@proton.me", "pass").with_smtp("mail.local", 587);

        assert_eq!(config.smtp_host, "mail.local");
        assert_eq!(config.smtp_port, 587);
    }

    #[test]
    fn with_tls_sets_tls_config() {
        let config = ProtonConfig::with_credentials("test@proton.me", "pass")
            .with_tls(TlsConfig::insecure());

        assert!(!config.tls.should_verify());
    }

    #[test]
    fn validate_success_for_valid_config() {
        let config = ProtonConfig::with_credentials("user@domain.com", "password123");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_fails_for_empty_email() {
        let config = ProtonConfig::with_credentials("", "password");
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::InvalidAddress(_))));
    }

    #[test]
    fn validate_fails_for_email_without_at() {
        let config = ProtonConfig::with_credentials("invalid-email", "password");
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::InvalidAddress(_))));
    }

    #[test]
    fn validate_fails_for_empty_password() {
        let config = ProtonConfig::with_credentials("user@domain.com", "");
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::AuthenticationFailed)));
    }

    #[test]
    fn config_serialization_excludes_password() {
        let config = ProtonConfig::with_credentials("user@domain.com", "secret123");
        let json = serde_json::to_string(&config).unwrap();

        // Password should not appear in serialized output
        assert!(!json.contains("secret123"));
        assert!(json.contains("user@domain.com"));
    }

    #[test]
    fn config_deserialization_works() {
        let json = r#"{
            "imap_host": "127.0.0.1",
            "imap_port": 1143,
            "smtp_host": "127.0.0.1",
            "smtp_port": 1025,
            "email": "test@example.com",
            "password": "test-pass"
        }"#;

        let config: ProtonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.email, "test@example.com");
        assert_eq!(config.password, "test-pass");
    }

    #[test]
    fn config_full_builder_chain() {
        let config = ProtonConfig::with_credentials("user@proton.me", "bridge-pw")
            .with_imap("mail.proton.local", 993)
            .with_smtp("smtp.proton.local", 465)
            .with_tls(TlsConfig::strict());

        assert_eq!(config.email, "user@proton.me");
        assert_eq!(config.imap_host, "mail.proton.local");
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_host, "smtp.proton.local");
        assert_eq!(config.smtp_port, 465);
        assert!(config.tls.should_verify());
    }
}

// ============================================================================
// EmailSummary Tests
// ============================================================================

mod email_summary_tests {
    use super::*;

    #[test]
    fn email_summary_new() {
        let summary = EmailSummary::new("123", "sender@example.com", "Test Subject");
        assert_eq!(summary.id, "123");
        assert_eq!(summary.from, "sender@example.com");
        assert_eq!(summary.subject, "Test Subject");
        assert!(summary.snippet.is_empty());
        assert!(!summary.is_read);
        assert!(!summary.is_important);
    }

    #[test]
    fn email_summary_with_snippet() {
        let summary =
            EmailSummary::new("1", "a@b.com", "Subject").with_snippet("This is the preview...");

        assert_eq!(summary.snippet, "This is the preview...");
    }

    #[test]
    fn email_summary_with_received_at() {
        let summary =
            EmailSummary::new("1", "a@b.com", "Subject").with_received_at("2025-01-01T12:00:00Z");

        assert_eq!(summary.received_at, "2025-01-01T12:00:00Z");
    }

    #[test]
    fn email_summary_with_read() {
        let summary = EmailSummary::new("1", "a@b.com", "Subject").with_read(true);

        assert!(summary.is_read);
    }

    #[test]
    fn email_summary_with_important() {
        let summary = EmailSummary::new("1", "a@b.com", "Subject").with_important(true);

        assert!(summary.is_important);
    }

    #[test]
    fn email_summary_builder_chain() {
        let summary = EmailSummary::new("42", "alice@example.com", "Important Message")
            .with_snippet("Hello, this is a test...")
            .with_received_at("2025-06-15T10:30:00Z")
            .with_read(false)
            .with_important(true);

        assert_eq!(summary.id, "42");
        assert_eq!(summary.from, "alice@example.com");
        assert_eq!(summary.subject, "Important Message");
        assert_eq!(summary.snippet, "Hello, this is a test...");
        assert_eq!(summary.received_at, "2025-06-15T10:30:00Z");
        assert!(!summary.is_read);
        assert!(summary.is_important);
    }

    #[test]
    fn email_summary_equality() {
        let summary1 = EmailSummary::new("1", "a@b.com", "Subject");
        let _summary2 = EmailSummary::new("1", "a@b.com", "Subject");

        // Note: received_at is set to current time, so we need to create with same time
        let summary3 = summary1.clone();
        assert_eq!(summary1, summary3);
    }

    #[test]
    fn email_summary_serialization_roundtrip() {
        let summary = EmailSummary::new("999", "test@test.com", "Serialization Test")
            .with_snippet("Preview text")
            .with_received_at("2025-01-01T00:00:00Z")
            .with_read(true)
            .with_important(false);

        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: EmailSummary = serde_json::from_str(&json).unwrap();

        assert_eq!(summary, deserialized);
    }

    #[test]
    fn email_summary_clone() {
        let original = EmailSummary::new("1", "from@test.com", "Test")
            .with_snippet("snippet")
            .with_read(true)
            .with_important(true);

        let cloned = original.clone();

        assert_eq!(original.id, cloned.id);
        assert_eq!(original.from, cloned.from);
        assert_eq!(original.subject, cloned.subject);
        assert_eq!(original.snippet, cloned.snippet);
        assert_eq!(original.is_read, cloned.is_read);
        assert_eq!(original.is_important, cloned.is_important);
    }
}

// ============================================================================
// EmailComposition Tests
// ============================================================================

mod email_composition_tests {
    use super::*;

    #[test]
    fn email_composition_new() {
        let email = EmailComposition::new("recipient@example.com", "Test Subject", "Email body");

        assert_eq!(email.to, "recipient@example.com");
        assert_eq!(email.subject, "Test Subject");
        assert_eq!(email.body, "Email body");
        assert!(email.cc.is_empty());
    }

    #[test]
    fn email_composition_with_cc() {
        let email = EmailComposition::new("to@example.com", "Subject", "Body")
            .with_cc("cc1@example.com")
            .with_cc("cc2@example.com");

        assert_eq!(email.cc.len(), 2);
        assert_eq!(email.cc[0], "cc1@example.com");
        assert_eq!(email.cc[1], "cc2@example.com");
    }

    #[test]
    fn email_composition_with_cc_list() {
        let cc_list = vec!["a@b.com".to_string(), "c@d.com".to_string()];
        let email =
            EmailComposition::new("to@example.com", "Subject", "Body").with_cc_list(cc_list);

        assert_eq!(email.cc.len(), 2);
    }

    #[test]
    fn email_composition_combined_cc() {
        let email = EmailComposition::new("to@example.com", "Subject", "Body")
            .with_cc("first@example.com")
            .with_cc_list(vec!["second@example.com".to_string()]);

        assert_eq!(email.cc.len(), 2);
        assert_eq!(email.cc[0], "first@example.com");
        assert_eq!(email.cc[1], "second@example.com");
    }

    #[test]
    fn validate_success_for_valid_composition() {
        let email = EmailComposition::new("recipient@domain.com", "Subject", "Body");
        assert!(email.validate().is_ok());
    }

    #[test]
    fn validate_fails_for_empty_recipient() {
        let email = EmailComposition::new("", "Subject", "Body");
        let result = email.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::InvalidAddress(_))));
    }

    #[test]
    fn validate_fails_for_recipient_without_at() {
        let email = EmailComposition::new("invalid-recipient", "Subject", "Body");
        let result = email.validate();
        assert!(result.is_err());
    }

    #[test]
    fn validate_fails_for_invalid_cc() {
        let email =
            EmailComposition::new("valid@domain.com", "Subject", "Body").with_cc("invalid-cc");

        let result = email.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::InvalidAddress(_))));
    }

    #[test]
    fn validate_fails_for_empty_subject() {
        let email = EmailComposition::new("valid@domain.com", "", "Body");
        let result = email.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(ProtonError::RequestFailed(_))));
    }

    #[test]
    fn validate_allows_empty_body() {
        let email = EmailComposition::new("valid@domain.com", "Subject", "");
        assert!(email.validate().is_ok());
    }

    #[test]
    fn email_composition_serialization_roundtrip() {
        let email =
            EmailComposition::new("to@example.com", "Test", "Body text").with_cc("cc@example.com");

        let json = serde_json::to_string(&email).unwrap();
        let deserialized: EmailComposition = serde_json::from_str(&json).unwrap();

        assert_eq!(email.to, deserialized.to);
        assert_eq!(email.subject, deserialized.subject);
        assert_eq!(email.body, deserialized.body);
        assert_eq!(email.cc, deserialized.cc);
    }

    #[test]
    fn email_composition_clone() {
        let original =
            EmailComposition::new("to@test.com", "Subject", "Body").with_cc("cc@test.com");

        let cloned = original.clone();

        assert_eq!(original.to, cloned.to);
        assert_eq!(original.subject, cloned.subject);
        assert_eq!(original.body, cloned.body);
        assert_eq!(original.cc, cloned.cc);
    }
}

// ============================================================================
// ProtonError Tests
// ============================================================================

mod proton_error_tests {
    use super::*;

    #[test]
    fn error_display_bridge_unavailable() {
        let err = ProtonError::BridgeUnavailable("Connection refused".to_string());
        assert_eq!(err.to_string(), "Bridge not available: Connection refused");
    }

    #[test]
    fn error_display_authentication_failed() {
        let err = ProtonError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");
    }

    #[test]
    fn error_display_mailbox_not_found() {
        let err = ProtonError::MailboxNotFound("Drafts".to_string());
        assert_eq!(err.to_string(), "Mailbox not found: Drafts");
    }

    #[test]
    fn error_display_message_not_found() {
        let err = ProtonError::MessageNotFound("12345".to_string());
        assert_eq!(err.to_string(), "Message not found: 12345");
    }

    #[test]
    fn error_display_connection_failed() {
        let err = ProtonError::ConnectionFailed("Timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: Timeout");
    }

    #[test]
    fn error_display_request_failed() {
        let err = ProtonError::RequestFailed("Invalid response".to_string());
        assert_eq!(err.to_string(), "Request failed: Invalid response");
    }

    #[test]
    fn error_display_smtp_error() {
        let err = ProtonError::SmtpError("550 User unknown".to_string());
        assert_eq!(err.to_string(), "SMTP error: 550 User unknown");
    }

    #[test]
    fn error_display_imap_error() {
        let err = ProtonError::ImapError("NO SELECT failed".to_string());
        assert_eq!(err.to_string(), "IMAP error: NO SELECT failed");
    }

    #[test]
    fn error_display_invalid_address() {
        let err = ProtonError::InvalidAddress("bad-email".to_string());
        assert_eq!(err.to_string(), "Invalid email address: bad-email");
    }

    #[test]
    fn error_is_debug() {
        let err = ProtonError::AuthenticationFailed;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("AuthenticationFailed"));
    }
}

// ============================================================================
// Client Construction Tests
// ============================================================================

mod client_construction_tests {
    use super::*;

    #[test]
    fn proton_bridge_client_new() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password");
        let _client = ProtonBridgeClient::new(config);
        // Client should be created without errors
    }

    #[test]
    fn proton_bridge_client_with_custom_config() {
        let config = ProtonConfig::with_credentials("user@domain.com", "pass")
            .with_imap("192.168.1.50", 993)
            .with_smtp("192.168.1.50", 587)
            .with_tls(TlsConfig::insecure());

        let _client = ProtonBridgeClient::new(config);
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn config_validation_email_with_at_doesnt_panic(
            email in "[a-z]{1,10}@[a-z]{1,10}\\.[a-z]{2,4}",
            password in "[a-zA-Z0-9]{8,20}"
        ) {
            let config = ProtonConfig::with_credentials(&email, &password);
            let _ = config.validate(); // May succeed or fail based on complexity
        }

        #[test]
        fn config_validation_empty_email_fails(
            password in "[a-zA-Z0-9]{1,20}"
        ) {
            let config = ProtonConfig::with_credentials("", &password);
            assert!(config.validate().is_err());
        }

        #[test]
        fn config_validation_empty_password_fails(
            email in "[a-z]{1,10}@[a-z]{1,10}\\.[a-z]{2,4}"
        ) {
            let config = ProtonConfig::with_credentials(&email, "");
            assert!(config.validate().is_err());
        }

        #[test]
        fn email_summary_serialization_roundtrip(
            id in "[0-9]{1,10}",
            from in "[a-z]{1,10}@[a-z]{1,10}\\.[a-z]{2,4}",
            subject in "[a-zA-Z0-9 ]{1,50}",
            snippet in "[a-zA-Z0-9 ]{0,100}",
            is_read in any::<bool>(),
            is_important in any::<bool>()
        ) {
            let summary = EmailSummary::new(&id, &from, &subject)
                .with_snippet(&snippet)
                .with_received_at("2025-01-01T00:00:00Z")
                .with_read(is_read)
                .with_important(is_important);

            let json = serde_json::to_string(&summary).unwrap();
            let deserialized: EmailSummary = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(summary, deserialized);
        }

        #[test]
        fn email_composition_validation_with_valid_email(
            to in "[a-z]{1,10}@[a-z]{1,10}\\.[a-z]{2,4}",
            subject in "[a-zA-Z0-9]{1,50}",
            body in "[a-zA-Z0-9 ]{0,200}"
        ) {
            let email = EmailComposition::new(&to, &subject, &body);
            prop_assert!(email.validate().is_ok());
        }

        #[test]
        fn email_composition_validation_without_at_fails(
            to in "[a-zA-Z0-9]{1,20}",
            subject in "[a-zA-Z0-9]{1,50}",
            body in "[a-zA-Z0-9 ]{0,200}"
        ) {
            // Only test strings without @
            prop_assume!(!to.contains('@'));
            let email = EmailComposition::new(&to, &subject, &body);
            prop_assert!(email.validate().is_err());
        }

        #[test]
        fn tls_config_min_version_preserved(
            version in prop_oneof!["1.0", "1.1", "1.2", "1.3"]
        ) {
            let config = TlsConfig {
                verify_certificates: Some(true),
                ca_cert_path: None,
                min_tls_version: version.to_string(),
            };

            let json = serde_json::to_string(&config).unwrap();
            let deserialized: TlsConfig = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(config.min_tls_version, deserialized.min_tls_version);
        }

        #[test]
        fn port_numbers_preserved_in_config(
            imap_port in 1u16..=65535u16,
            smtp_port in 1u16..=65535u16
        ) {
            let config = ProtonConfig::with_credentials("test@test.com", "pass")
                .with_imap("localhost", imap_port)
                .with_smtp("localhost", smtp_port);

            prop_assert_eq!(config.imap_port, imap_port);
            prop_assert_eq!(config.smtp_port, smtp_port);
        }
    }
}

// ============================================================================
// IMAP Client Tests
// ============================================================================

mod imap_client_tests {
    use super::*;
    use integration_proton::ProtonImapClient;

    #[test]
    fn imap_client_creation() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password");
        let _client = ProtonImapClient::new(config);
    }

    #[test]
    fn imap_client_with_insecure_tls() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password")
            .with_tls(TlsConfig::insecure());
        let _client = ProtonImapClient::new(config);
    }

    #[tokio::test]
    async fn imap_client_connection_fails_to_nonexistent_server() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password")
            .with_imap("127.0.0.1", 19999) // Non-listening port
            .with_tls(TlsConfig::insecure());

        let client = ProtonImapClient::new(config);
        let result = client.fetch_mailbox("INBOX", 10).await;

        assert!(result.is_err());
        // Should fail with connection error
        match result {
            Err(ProtonError::ConnectionFailed(_)) => {},
            Err(e) => panic!("Expected ConnectionFailed, got {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }
}

// ============================================================================
// SMTP Client Tests
// ============================================================================

mod smtp_client_tests {
    use super::*;
    use integration_proton::ProtonSmtpClient;

    #[test]
    fn smtp_client_creation() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password");
        let _client = ProtonSmtpClient::new(config);
    }

    #[test]
    fn smtp_client_with_custom_port() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password")
            .with_smtp("localhost", 465);
        let _client = ProtonSmtpClient::new(config);
    }

    #[tokio::test]
    async fn smtp_client_connection_fails_to_nonexistent_server() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password")
            .with_smtp("127.0.0.1", 19998) // Non-listening port
            .with_tls(TlsConfig::insecure());

        let client = ProtonSmtpClient::new(config);
        let email = EmailComposition::new("recipient@example.com", "Test", "Body");
        let result = client.send_email(&email).await;

        assert!(result.is_err());
        // Should fail with connection error
        match result {
            Err(ProtonError::ConnectionFailed(_)) => {},
            Err(e) => panic!("Expected ConnectionFailed, got {:?}", e),
            Ok(_) => panic!("Expected error, got success"),
        }
    }
}

// ============================================================================
// Reconnect Config Tests
// ============================================================================

mod reconnect_tests {
    use integration_proton::ReconnectConfig;

    #[test]
    fn reconnect_config_default() {
        let config = ReconnectConfig::default();
        // Default values: max_retries = 0 means infinite retries
        assert_eq!(config.max_retries, 0); // 0 = infinite retries
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_case_tests {
    use super::*;

    #[test]
    fn email_with_unicode_subject() {
        let email = EmailComposition::new("to@example.com", "日本語テスト 🎉", "Body");
        assert_eq!(email.subject, "日本語テスト 🎉");
    }

    #[test]
    fn email_with_unicode_body() {
        let email = EmailComposition::new("to@example.com", "Test", "Привет мир! 世界你好!");
        assert_eq!(email.body, "Привет мир! 世界你好!");
    }

    #[test]
    fn email_summary_with_unicode() {
        let summary = EmailSummary::new("1", "üser@tëst.com", "Rë: Tëst Mëssägë")
            .with_snippet("Cöntënt wïth spëcïäl chäräctërs");

        assert_eq!(summary.from, "üser@tëst.com");
        assert_eq!(summary.subject, "Rë: Tëst Mëssägë");
    }

    #[test]
    fn email_with_very_long_body() {
        let long_body = "x".repeat(100_000);
        let email = EmailComposition::new("to@example.com", "Long Email", &long_body);
        assert_eq!(email.body.len(), 100_000);
    }

    #[test]
    fn email_summary_with_empty_fields() {
        let summary = EmailSummary::new("", "", "");
        assert!(summary.id.is_empty());
        assert!(summary.from.is_empty());
        assert!(summary.subject.is_empty());
    }

    #[test]
    fn config_with_ipv6_host() {
        let config = ProtonConfig::with_credentials("test@test.com", "pass")
            .with_imap("::1", 993)
            .with_smtp("::1", 587);

        assert_eq!(config.imap_host, "::1");
        assert_eq!(config.smtp_host, "::1");
    }

    #[test]
    fn email_composition_with_many_cc() {
        let mut email = EmailComposition::new("to@example.com", "Subject", "Body");
        for i in 0..100 {
            email = email.with_cc(format!("cc{}@example.com", i));
        }
        assert_eq!(email.cc.len(), 100);
    }

    #[test]
    fn email_with_special_characters_in_body() {
        let body = r#"Line 1
Line 2
.Hidden line
..Double dot
Tab:	here
Special: <>&"'"#;

        let email = EmailComposition::new("to@example.com", "Special", body);
        assert!(email.body.contains("..Double dot"));
        assert!(email.body.contains(".Hidden line"));
    }

    #[test]
    fn tls_config_with_nonexistent_cert_path() {
        let config = TlsConfig::with_ca_cert("/nonexistent/path/to/cert.pem");
        assert_eq!(
            config.ca_cert_path,
            Some(PathBuf::from("/nonexistent/path/to/cert.pem"))
        );
        // The actual error happens during connection, not configuration
    }

    #[test]
    fn config_validation_with_minimal_email() {
        // Minimal valid email address
        let config = ProtonConfig::with_credentials("a@b.c", "p");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn email_composition_with_multiline_subject() {
        // Subject should be single line in real emails, but we accept it
        let email = EmailComposition::new("to@example.com", "Line1\nLine2", "Body");
        assert!(email.subject.contains('\n'));
    }
}

// ============================================================================
// Async Client Trait Tests
// ============================================================================

mod async_trait_tests {
    use super::*;
    use integration_proton::ProtonClient;

    // Test that ProtonBridgeClient implements ProtonClient trait
    fn assert_implements_trait<T: ProtonClient>(_: &T) {}

    #[test]
    fn bridge_client_implements_trait() {
        let config = ProtonConfig::with_credentials("test@test.com", "pass");
        let client = ProtonBridgeClient::new(config);
        assert_implements_trait(&client);
    }

    // Test that trait methods exist (compilation check)
    #[tokio::test]
    async fn trait_methods_exist() {
        let config = ProtonConfig::with_credentials("test@test.com", "pass")
            .with_imap("127.0.0.1", 19997)
            .with_tls(TlsConfig::insecure());
        let client = ProtonBridgeClient::new(config);

        // These will fail due to connection, but we're testing the API exists
        let _ = client.get_inbox(10).await;
        let _ = client.check_connection().await;
    }
}
