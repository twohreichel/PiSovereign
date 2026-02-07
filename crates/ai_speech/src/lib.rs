//! AI Speech - Speech-to-Text and Text-to-Speech abstractions
//!
//! Provides traits and implementations for speech processing:
//! - `SpeechToText` - Transcribe audio to text (STT)
//! - `TextToSpeech` - Synthesize speech from text (TTS)
//! - `AudioConverter` - Convert audio between formats
//!
//! # Architecture
//!
//! This crate follows the ports & adapters pattern:
//! - `ports` module defines the traits (ports)
//! - `providers` module contains concrete implementations (adapters)
//! - `converter` module handles audio format conversion
//!
//! # Supported Providers
//!
//! | Provider | STT | TTS | Local | Notes |
//! |----------|-----|-----|-------|-------|
//! | `HybridSpeechProvider` | ✅ | ✅ | ✅ | **Default** - Local first, cloud fallback |
//! | `WhisperCppProvider` | ✅ | ❌ | ✅ | Local STT via whisper.cpp |
//! | `PiperProvider` | ❌ | ✅ | ✅ | Local TTS via Piper |
//! | `OpenAISpeechProvider` | ✅ | ✅ | ❌ | Cloud via OpenAI API |
//!
//! # Example
//!
//! ```ignore
//! use ai_speech::{
//!     HybridSpeechProvider, SpeechToText, TextToSpeech,
//!     AudioData, AudioFormat, AudioConverter,
//!     LocalSttConfig, LocalTtsConfig, HybridConfig,
//! };
//!
//! // Create hybrid provider (local first, cloud fallback)
//! let provider = HybridSpeechProvider::new(
//!     Some(LocalSttConfig::default()),
//!     Some(LocalTtsConfig::default()),
//!     None, // No cloud fallback
//!     HybridConfig::default(),
//! )?;
//!
//! // Or create local-only provider
//! let local_provider = HybridSpeechProvider::local_only(
//!     LocalSttConfig::default(),
//!     LocalTtsConfig::default(),
//! )?;
//!
//! // Transcribe audio
//! let transcription = provider.transcribe(audio).await?;
//! println!("Transcribed: {}", transcription.text);
//!
//! // Synthesize speech
//! let audio = provider.synthesize("Hallo Welt!", None).await?;
//! ```

pub mod config;
pub mod converter;
pub mod error;
pub mod ports;
pub mod providers;
pub mod types;

pub use config::{
    HybridConfig, LocalSttConfig, LocalTtsConfig, ResponseFormatPreference, SpeechConfig,
    SpeechProvider,
};
pub use converter::AudioConverter;
pub use error::SpeechError;
pub use ports::{SpeechToText, TextToSpeech};
pub use providers::hybrid::HybridSpeechProvider;
pub use providers::openai::OpenAISpeechProvider;
pub use providers::piper::PiperProvider;
pub use providers::whisper_cpp::WhisperCppProvider;
pub use types::{AudioData, AudioFormat, Transcription, VoiceInfo};

