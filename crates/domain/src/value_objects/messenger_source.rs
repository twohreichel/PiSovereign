//! Messenger source - Identifies the messaging platform

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported messaging platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessengerSource {
    /// Meta's WhatsApp Business API
    WhatsApp,
    /// Signal via signal-cli
    Signal,
}

impl MessengerSource {
    /// Get the display name for this messenger
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::WhatsApp => "WhatsApp",
            Self::Signal => "Signal",
        }
    }

    /// Get the config key for this messenger
    #[must_use]
    pub const fn config_key(&self) -> &'static str {
        match self {
            Self::WhatsApp => "whatsapp",
            Self::Signal => "signal",
        }
    }

    /// Parse from config string (case-insensitive)
    #[must_use]
    pub fn from_config(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "whatsapp" => Some(Self::WhatsApp),
            "signal" => Some(Self::Signal),
            _ => None,
        }
    }
}

impl fmt::Display for MessengerSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl Default for MessengerSource {
    fn default() -> Self {
        Self::WhatsApp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_returns_correct_values() {
        assert_eq!(MessengerSource::WhatsApp.display_name(), "WhatsApp");
        assert_eq!(MessengerSource::Signal.display_name(), "Signal");
    }

    #[test]
    fn config_key_returns_lowercase() {
        assert_eq!(MessengerSource::WhatsApp.config_key(), "whatsapp");
        assert_eq!(MessengerSource::Signal.config_key(), "signal");
    }

    #[test]
    fn from_config_parses_valid_values() {
        assert_eq!(
            MessengerSource::from_config("whatsapp"),
            Some(MessengerSource::WhatsApp)
        );
        assert_eq!(
            MessengerSource::from_config("signal"),
            Some(MessengerSource::Signal)
        );
    }

    #[test]
    fn from_config_is_case_insensitive() {
        assert_eq!(
            MessengerSource::from_config("WhatsApp"),
            Some(MessengerSource::WhatsApp)
        );
        assert_eq!(
            MessengerSource::from_config("SIGNAL"),
            Some(MessengerSource::Signal)
        );
        assert_eq!(
            MessengerSource::from_config("SiGnAl"),
            Some(MessengerSource::Signal)
        );
    }

    #[test]
    fn from_config_returns_none_for_invalid() {
        assert_eq!(MessengerSource::from_config("telegram"), None);
        assert_eq!(MessengerSource::from_config(""), None);
        assert_eq!(MessengerSource::from_config("whats app"), None);
    }

    #[test]
    fn display_matches_display_name() {
        assert_eq!(format!("{}", MessengerSource::WhatsApp), "WhatsApp");
        assert_eq!(format!("{}", MessengerSource::Signal), "Signal");
    }

    #[test]
    fn default_is_whatsapp() {
        assert_eq!(MessengerSource::default(), MessengerSource::WhatsApp);
    }

    #[test]
    fn serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&MessengerSource::WhatsApp).unwrap(),
            "\"whatsapp\""
        );
        assert_eq!(
            serde_json::to_string(&MessengerSource::Signal).unwrap(),
            "\"signal\""
        );
    }

    #[test]
    fn deserializes_from_lowercase() {
        assert_eq!(
            serde_json::from_str::<MessengerSource>("\"whatsapp\"").unwrap(),
            MessengerSource::WhatsApp
        );
        assert_eq!(
            serde_json::from_str::<MessengerSource>("\"signal\"").unwrap(),
            MessengerSource::Signal
        );
    }
}
