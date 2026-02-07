//! Speech processing provider implementations
//!
//! Contains concrete implementations of the `SpeechToText` and `TextToSpeech` traits.
//!
//! # Available Providers
//!
//! - [`OpenAISpeechProvider`] - Cloud-based using OpenAI Whisper and TTS
//! - [`WhisperCppProvider`] - Local STT using whisper.cpp
//! - [`PiperProvider`] - Local TTS using Piper
//! - [`HybridSpeechProvider`] - Combines local and cloud with automatic fallback

pub mod hybrid;
pub mod openai;
pub mod piper;
pub mod whisper_cpp;

pub use hybrid::HybridSpeechProvider;
pub use openai::OpenAISpeechProvider;
pub use piper::PiperProvider;
pub use whisper_cpp::WhisperCppProvider;
