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
//! ## Usage
//!
//! ```no_run
//! use integration_proton::{ProtonBridgeClient, ProtonClient, ProtonConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ProtonConfig {
//!         email: "user@proton.me".to_string(),
//!         password: "bridge-password".to_string(),
//!         ..Default::default()
//!     };
//!
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

pub use client::{
    EmailComposition, EmailSummary, ProtonBridgeClient, ProtonClient, ProtonConfig, ProtonError,
};
