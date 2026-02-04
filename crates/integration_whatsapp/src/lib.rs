//! WhatsApp integration
//!
//! Handles WhatsApp Business API webhooks and message sending.

pub mod client;
pub mod webhook;

pub use client::{WhatsAppClient, WhatsAppClientConfig, WhatsAppError};
pub use webhook::{WebhookConfig, WebhookPayload, extract_messages, verify_signature};
