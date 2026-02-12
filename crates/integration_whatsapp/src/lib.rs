#![forbid(unsafe_code)]
//! WhatsApp integration
//!
//! Handles WhatsApp Business API webhooks and message sending.
//! Supports both text and audio (voice) messages.

pub mod client;
pub mod webhook;

pub use client::{WhatsAppClient, WhatsAppClientConfig, WhatsAppError};
pub use webhook::{
    AudioMessage, IncomingMessage, WebhookConfig, WebhookPayload, extract_all_messages,
    extract_audio_messages, extract_messages, verify_signature,
};
