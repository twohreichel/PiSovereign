//! Shared TLS connector builder for Proton Bridge connections
//!
//! Used by both the IMAP and SMTP clients to avoid duplicating the
//! TLS configuration logic.

use std::fs;

use native_tls::Certificate;
use tracing::{debug, warn};

use crate::{ProtonError, TlsConfig};

/// Builds a `native_tls::TlsConnector` from the shared TLS configuration
///
/// Handles custom CA certificates, certificate verification toggle,
/// and minimum TLS version. Callers wrap the result as needed
/// (e.g. SMTP wraps in `tokio_native_tls::TlsConnector`).
pub fn build_native_tls_connector(
    tls_config: &TlsConfig,
) -> Result<native_tls::TlsConnector, ProtonError> {
    let mut builder = native_tls::TlsConnector::builder();

    // Configure certificate verification
    if !tls_config.should_verify() {
        warn!(
            "⚠️ TLS certificate verification disabled - only recommended for local Proton Bridge"
        );
        builder.danger_accept_invalid_certs(true);
    } else if let Some(ca_cert_path) = &tls_config.ca_cert_path {
        // Load custom CA certificate
        debug!(path = %ca_cert_path.display(), "Loading custom CA certificate");
        let cert_data = fs::read(ca_cert_path).map_err(|e| {
            ProtonError::ConnectionFailed(format!(
                "Failed to read CA certificate at {}: {e}",
                ca_cert_path.display()
            ))
        })?;
        let cert = Certificate::from_pem(&cert_data).map_err(|e| {
            ProtonError::ConnectionFailed(format!("Failed to parse CA certificate: {e}"))
        })?;
        builder.add_root_certificate(cert);
    }

    // Configure minimum TLS version
    let min_protocol = match tls_config.min_tls_version.as_str() {
        "1.0" => native_tls::Protocol::Tlsv10,
        "1.1" => native_tls::Protocol::Tlsv11,
        _ => native_tls::Protocol::Tlsv12,
    };
    builder.min_protocol_version(Some(min_protocol));

    builder
        .build()
        .map_err(|e| ProtonError::ConnectionFailed(format!("TLS builder failed: {e}")))
}
