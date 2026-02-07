//! Speech processing provider implementations
//!
//! Contains concrete implementations of the `SpeechToText` and `TextToSpeech` traits.

pub mod openai;

pub use openai::OpenAISpeechProvider;
