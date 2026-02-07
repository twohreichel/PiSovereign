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
//! - OpenAI Whisper (STT) and TTS API
//! - Future: Local whisper.cpp integration
//!
//! # Example
//!
//! ```ignore
//! use ai_speech::{OpenAISpeechProvider, SpeechToText, TextToSpeech, AudioData, AudioFormat, AudioConverter};
//!
//! let provider = OpenAISpeechProvider::new(config)?;
//! let converter = AudioConverter::new();
//!
//! // Convert WhatsApp audio (OGG/Opus) to Whisper-compatible format
//! let whatsapp_audio = AudioData::new(bytes, AudioFormat::Opus)?;
//! let whisper_audio = converter.convert_for_whisper(&whatsapp_audio).await?;
//!
//! // Transcribe audio
//! let transcription = provider.transcribe(whisper_audio).await?;
//! println!("Transcribed: {}", transcription.text);
//!
//! // Synthesize speech
//! let audio = provider.synthesize("Hello, world!", None).await?;
//! ```

pub mod config;
pub mod converter;
pub mod error;
pub mod ports;
pub mod providers;
pub mod types;

pub use config::SpeechConfig;
pub use converter::AudioConverter;
pub use error::SpeechError;
pub use ports::{SpeechToText, TextToSpeech};
pub use providers::openai::OpenAISpeechProvider;
pub use types::{AudioData, AudioFormat, Transcription, VoiceInfo};
