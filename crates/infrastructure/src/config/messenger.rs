//! Messenger configuration: WhatsApp, Signal, conversation persistence.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use super::default_true;

/// WhatsApp integration configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Meta Graph API access token (sensitive - uses SecretString)
    #[serde(default, skip_serializing)]
    pub access_token: Option<SecretString>,

    /// Phone number ID from WhatsApp Business
    #[serde(default)]
    pub phone_number_id: Option<String>,

    /// App secret for webhook signature verification (sensitive - uses SecretString)
    #[serde(default, skip_serializing)]
    pub app_secret: Option<SecretString>,

    /// Verify token for webhook setup
    #[serde(default)]
    pub verify_token: Option<String>,

    /// Whether signature verification is required (default: true)
    #[serde(default = "default_true")]
    pub signature_required: bool,

    /// API version (default: v18.0)
    #[serde(default = "default_api_version")]
    pub api_version: String,

    /// Phone numbers allowed to send messages (empty = allow all)
    #[serde(default)]
    pub whitelist: Vec<String>,

    /// Conversation persistence configuration
    #[serde(default)]
    pub persistence: MessengerPersistenceConfig,
}

impl std::fmt::Debug for WhatsAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConfig")
            .field(
                "access_token",
                &if self.access_token.is_some() {
                    Some("[REDACTED]")
                } else {
                    None
                },
            )
            .field("phone_number_id", &self.phone_number_id)
            .field(
                "app_secret",
                &if self.app_secret.is_some() {
                    Some("[REDACTED]")
                } else {
                    None
                },
            )
            .field("verify_token", &self.verify_token)
            .field("signature_required", &self.signature_required)
            .field("api_version", &self.api_version)
            .field("whitelist", &format!("[{} entries]", self.whitelist.len()))
            .field("persistence", &self.persistence)
            .finish()
    }
}

fn default_api_version() -> String {
    "v18.0".to_string()
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            access_token: None,
            phone_number_id: None,
            app_secret: None,
            verify_token: None,
            signature_required: true,
            api_version: default_api_version(),
            whitelist: Vec::new(),
            persistence: MessengerPersistenceConfig::default(),
        }
    }
}

impl WhatsAppConfig {
    /// Get the access token as a string reference (for API calls)
    #[must_use]
    pub fn access_token_str(&self) -> Option<&str> {
        self.access_token.as_ref().map(ExposeSecret::expose_secret)
    }

    /// Get the app secret as a string reference (for signature verification)
    #[must_use]
    pub fn app_secret_str(&self) -> Option<&str> {
        self.app_secret.as_ref().map(ExposeSecret::expose_secret)
    }
}

/// Signal integration configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Phone number registered with Signal (E.164 format, e.g., "+1234567890")
    #[serde(default)]
    pub phone_number: String,

    /// Path to the signal-cli JSON-RPC socket
    #[serde(default = "default_signal_socket_path")]
    pub socket_path: String,

    /// Path to signal-cli data directory (optional)
    #[serde(default)]
    pub data_path: Option<String>,

    /// Connection timeout in milliseconds
    #[serde(default = "default_signal_timeout")]
    pub timeout_ms: u64,

    /// Phone numbers allowed to send messages (empty = allow all)
    #[serde(default)]
    pub whitelist: Vec<String>,

    /// Conversation persistence configuration
    #[serde(default)]
    pub persistence: MessengerPersistenceConfig,
}

fn default_signal_socket_path() -> String {
    "/var/run/signal-cli/socket".to_string()
}

const fn default_signal_timeout() -> u64 {
    30_000
}

impl std::fmt::Debug for SignalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalConfig")
            .field("phone_number", &self.phone_number)
            .field("socket_path", &self.socket_path)
            .field("data_path", &self.data_path)
            .field("timeout_ms", &self.timeout_ms)
            .field("whitelist", &format!("[{} entries]", self.whitelist.len()))
            .field("persistence", &self.persistence)
            .finish()
    }
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            phone_number: String::new(),
            socket_path: default_signal_socket_path(),
            data_path: None,
            timeout_ms: default_signal_timeout(),
            whitelist: Vec::new(),
            persistence: MessengerPersistenceConfig::default(),
        }
    }
}

/// Messenger conversation persistence configuration
///
/// Controls how messenger (WhatsApp/Signal) conversations are stored,
/// encrypted, and integrated with the memory/RAG system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct MessengerPersistenceConfig {
    /// Enable conversation persistence (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enable encryption for stored messages (default: true)
    /// Uses the same encryption as the memory system
    #[serde(default = "default_true")]
    pub enable_encryption: bool,

    /// Enable RAG context retrieval for conversations (default: true)
    #[serde(default = "default_true")]
    pub enable_rag: bool,

    /// Enable automatic learning from interactions (default: true)
    /// Stores Q&A pairs as memories for future context
    #[serde(default = "default_true")]
    pub enable_learning: bool,

    /// Maximum number of days to retain conversations (Optional)
    /// If set, conversations older than this will be cleaned up
    #[serde(default)]
    pub retention_days: Option<u32>,

    /// Maximum messages per conversation before FIFO truncation (Optional)
    /// If set, oldest messages are removed when this limit is exceeded
    #[serde(default)]
    pub max_messages_per_conversation: Option<usize>,

    /// Number of recent messages to include as context for new messages (default: 50)
    #[serde(default = "default_context_window")]
    pub context_window: usize,
}

const fn default_context_window() -> usize {
    50
}

impl Default for MessengerPersistenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_encryption: true,
            enable_rag: true,
            enable_learning: true,
            retention_days: None,
            max_messages_per_conversation: None,
            context_window: default_context_window(),
        }
    }
}
