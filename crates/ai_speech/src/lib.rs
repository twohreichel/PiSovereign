//! AI Speech - Speech-to-Text and Text-to-Speech abstractions
//!
//! Provides traits and implementations for speech processing:
//! - `SpeechToText` - Transcribe audio to text (STT)
//! - `TextToSpeech` - Synthesize speech from text (TTS)
//!
//! # Architecture
//!
//! This crate follows the ports & adapters pattern:
//! - `ports` module defines the traits (ports)
//! - `providers` module contains concrete implementations (adapters)
//!
//! # Supported Providers
//!
//! - OpenAI Whisper (STT) and TTS API
//! - Future: Local whisper.cpp integration
//!
//! # Example
//!
//! ```ignore
//! use ai_speech::{OpenAISpeechProvider, SpeechToText, TextToSpeech, AudioData, AudioFormat};
//!
//! let provider = OpenAISpeechProvider::new(config)?;
//!
//! // Transcribe audio
//! let audio = AudioData::new(bytes, AudioFormat::Opus);
//! let transcription = provider.transcribe(audio).await?;
//! println!("Transcribed: {}", transcription.text);
//!
//! // Synthesize speech
//! let audio = provider.synthesize("Hello, world!", None).await?;
//! ```

pub mod config;
pub mod error;
pub mod ports;
pub mod providers;
pub mod types;

pub use config::SpeechConfig;
pub use error::SpeechError;
pub use ports::{SpeechToText, TextToSpeech};
pub use providers::openai::OpenAISpeechProvider;
pub use types::{AudioData, AudioFormat, Transcription, VoiceInfo};
