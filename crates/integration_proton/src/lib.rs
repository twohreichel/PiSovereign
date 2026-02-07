//! Proton Mail integration
//!
//! Sidecar interface for Proton Mail Bridge.
//!
//! ## Architecture
//!
//! This crate provides integration with Proton Mail via the Proton Bridge
//! application. The Bridge must be running locally and exposes IMAP/SMTP
//! servers on configurable ports (default 1143/1025).
//!
//! The integration consists of:
//! - `ProtonImapClient` - IMAP client for reading and managing emails
//! - `ProtonSmtpClient` - SMTP client for sending emails
//! - `ProtonBridgeClient` - Unified client implementing `ProtonClient` trait
//!
//! ## Usage
//!
//! ```no_run
//! use integration_proton::{ProtonBridgeClient, ProtonClient, ProtonConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ProtonConfig::with_credentials("user@proton.me", "bridge-password");
//!     let client = ProtonBridgeClient::new(config);
//!
//!     // Check if Bridge is running
//!     if client.check_connection().await.unwrap_or(false) {
//!         let emails = client.get_inbox(10).await.unwrap();
//!         println!("Found {} emails", emails.len());
//!     }
//! }
//! ```

mod client;
mod imap_client;
mod reconnect;
mod smtp_client;

pub use client::{
    EmailComposition, EmailSummary, ProtonBridgeClient, ProtonClient, ProtonConfig, ProtonError,
    TlsConfig,
};
pub use imap_client::ProtonImapClient;
pub use reconnect::{ReconnectConfig, ReconnectingProtonClient};
pub use smtp_client::ProtonSmtpClient;
