//! Types for speech processing
//!
//! Contains data structures for audio data, formats, transcriptions, and voice information.

use serde::{Deserialize, Serialize};

/// Supported audio formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    /// Opus codec (used by WhatsApp voice messages)
    Opus,
    /// OGG container (typically with Opus codec)
    Ogg,
    /// MP3 format
    Mp3,
    /// WAV format (uncompressed)
    Wav,
    /// FLAC format (lossless)
    Flac,
    /// WebM format
    Webm,
    /// M4A/AAC format
    M4a,
}

impl AudioFormat {
    /// Get the MIME type for this audio format
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        match self {
            Self::Opus => "audio/opus",
            Self::Ogg => "audio/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Webm => "audio/webm",
            Self::M4a => "audio/m4a",
        }
    }

    /// Get the file extension for this audio format
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Webm => "webm",
            Self::M4a => "m4a",
        }
    }

    /// Parse audio format from MIME type
    #[must_use]
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        // Handle compound MIME types like "audio/ogg; codecs=opus"
        let base_mime = mime.split(';').next().unwrap_or(mime).trim();

        match base_mime {
            "audio/opus" => Some(Self::Opus),
            "audio/ogg" => {
                // Check for codec specification
                if mime.contains("codecs=opus") {
                    Some(Self::Opus)
                } else {
                    Some(Self::Ogg)
                }
            },
            "audio/mpeg" | "audio/mp3" => Some(Self::Mp3),
            "audio/wav" | "audio/x-wav" | "audio/wave" => Some(Self::Wav),
            "audio/flac" | "audio/x-flac" => Some(Self::Flac),
            "audio/webm" => Some(Self::Webm),
            "audio/m4a" | "audio/mp4" | "audio/x-m4a" => Some(Self::M4a),
            _ => None,
        }
    }

    /// Check if this format is supported by OpenAI Whisper
    #[must_use]
    pub const fn is_whisper_supported(&self) -> bool {
        matches!(
            self,
            Self::Mp3 | Self::Wav | Self::Flac | Self::Webm | Self::M4a
        )
    }
}

/// Container for audio data with metadata
#[derive(Debug, Clone)]
pub struct AudioData {
    /// Raw audio bytes
    data: Vec<u8>,
    /// Audio format
    format: AudioFormat,
    /// Duration in milliseconds (if known)
    duration_ms: Option<u64>,
    /// Sample rate in Hz (if known)
    sample_rate: Option<u32>,
}

impl AudioData {
    /// Create new audio data
    #[must_use]
    pub const fn new(data: Vec<u8>, format: AudioFormat) -> Self {
        Self {
            data,
            format,
            duration_ms: None,
            sample_rate: None,
        }
    }

    /// Create audio data with duration
    #[must_use]
    pub const fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Create audio data with sample rate
    #[must_use]
    pub const fn with_sample_rate(mut self, sample_rate: u32) -> Self {
        self.sample_rate = Some(sample_rate);
        self
    }

    /// Get the raw audio bytes
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consume and return the raw audio bytes
    #[must_use]
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// Get the audio format
    #[must_use]
    pub const fn format(&self) -> AudioFormat {
        self.format
    }

    /// Get the duration in milliseconds (if known)
    #[must_use]
    pub const fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    /// Get the sample rate (if known)
    #[must_use]
    pub const fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    /// Get the size of the audio data in bytes
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }

    /// Check if the audio data is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the MIME type for this audio
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }

    /// Generate a filename with appropriate extension
    #[must_use]
    pub fn filename(&self, base: &str) -> String {
        format!("{}.{}", base, self.format.extension())
    }
}

/// Result of speech-to-text transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    /// Transcribed text
    pub text: String,
    /// Detected language (ISO 639-1 code)
    pub language: Option<String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: Option<f32>,
    /// Duration of the audio in milliseconds
    pub duration_ms: Option<u64>,
    /// Word-level timestamps (if available)
    pub words: Option<Vec<WordTimestamp>>,
}

impl Transcription {
    /// Create a simple transcription with just text
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            language: None,
            confidence: None,
            duration_ms: None,
            words: None,
        }
    }

    /// Set the detected language
    #[must_use]
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Set the confidence score
    #[must_use]
    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }

    /// Set the duration
    #[must_use]
    pub const fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Check if transcription is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// Word-level timestamp from transcription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordTimestamp {
    /// The word
    pub word: String,
    /// Start time in milliseconds
    pub start_ms: u64,
    /// End time in milliseconds
    pub end_ms: u64,
    /// Confidence for this word
    pub confidence: Option<f32>,
}

/// Information about an available voice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    /// Voice identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of the voice
    pub description: Option<String>,
    /// Supported languages
    pub languages: Vec<String>,
    /// Voice gender (if known)
    pub gender: Option<VoiceGender>,
    /// Preview URL (if available)
    pub preview_url: Option<String>,
}

impl VoiceInfo {
    /// Create a new voice info
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            languages: Vec::new(),
            gender: None,
            preview_url: None,
        }
    }
}

/// Voice gender classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceGender {
    /// Male voice
    Male,
    /// Female voice
    Female,
    /// Neutral/androgynous voice
    Neutral,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod audio_format {
        use super::*;

        #[test]
        fn mime_types_are_correct() {
            assert_eq!(AudioFormat::Opus.mime_type(), "audio/opus");
            assert_eq!(AudioFormat::Ogg.mime_type(), "audio/ogg");
            assert_eq!(AudioFormat::Mp3.mime_type(), "audio/mpeg");
            assert_eq!(AudioFormat::Wav.mime_type(), "audio/wav");
            assert_eq!(AudioFormat::Flac.mime_type(), "audio/flac");
            assert_eq!(AudioFormat::Webm.mime_type(), "audio/webm");
            assert_eq!(AudioFormat::M4a.mime_type(), "audio/m4a");
        }

        #[test]
        fn extensions_are_correct() {
            assert_eq!(AudioFormat::Opus.extension(), "opus");
            assert_eq!(AudioFormat::Ogg.extension(), "ogg");
            assert_eq!(AudioFormat::Mp3.extension(), "mp3");
            assert_eq!(AudioFormat::Wav.extension(), "wav");
            assert_eq!(AudioFormat::Flac.extension(), "flac");
            assert_eq!(AudioFormat::Webm.extension(), "webm");
            assert_eq!(AudioFormat::M4a.extension(), "m4a");
        }

        #[test]
        fn from_mime_type_simple() {
            assert_eq!(
                AudioFormat::from_mime_type("audio/opus"),
                Some(AudioFormat::Opus)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/ogg"),
                Some(AudioFormat::Ogg)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/mpeg"),
                Some(AudioFormat::Mp3)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/mp3"),
                Some(AudioFormat::Mp3)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/wav"),
                Some(AudioFormat::Wav)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/x-wav"),
                Some(AudioFormat::Wav)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/flac"),
                Some(AudioFormat::Flac)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/webm"),
                Some(AudioFormat::Webm)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/m4a"),
                Some(AudioFormat::M4a)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/mp4"),
                Some(AudioFormat::M4a)
            );
        }

        #[test]
        fn from_mime_type_with_codecs() {
            // WhatsApp sends this format
            assert_eq!(
                AudioFormat::from_mime_type("audio/ogg; codecs=opus"),
                Some(AudioFormat::Opus)
            );
        }

        #[test]
        fn from_mime_type_unknown() {
            assert_eq!(AudioFormat::from_mime_type("audio/unknown"), None);
            assert_eq!(AudioFormat::from_mime_type("text/plain"), None);
        }

        #[test]
        fn whisper_supported_formats() {
            assert!(!AudioFormat::Opus.is_whisper_supported());
            assert!(!AudioFormat::Ogg.is_whisper_supported());
            assert!(AudioFormat::Mp3.is_whisper_supported());
            assert!(AudioFormat::Wav.is_whisper_supported());
            assert!(AudioFormat::Flac.is_whisper_supported());
            assert!(AudioFormat::Webm.is_whisper_supported());
            assert!(AudioFormat::M4a.is_whisper_supported());
        }
    }

    mod audio_data {
        use super::*;

        #[test]
        fn new_creates_audio_data() {
            let data = vec![1, 2, 3, 4];
            let audio = AudioData::new(data.clone(), AudioFormat::Mp3);

            assert_eq!(audio.data(), &data);
            assert_eq!(audio.format(), AudioFormat::Mp3);
            assert_eq!(audio.duration_ms(), None);
            assert_eq!(audio.sample_rate(), None);
        }

        #[test]
        fn with_duration_sets_duration() {
            let audio = AudioData::new(vec![1, 2, 3], AudioFormat::Opus).with_duration(5000);
            assert_eq!(audio.duration_ms(), Some(5000));
        }

        #[test]
        fn with_sample_rate_sets_sample_rate() {
            let audio = AudioData::new(vec![1, 2, 3], AudioFormat::Wav).with_sample_rate(44100);
            assert_eq!(audio.sample_rate(), Some(44100));
        }

        #[test]
        fn size_bytes_returns_data_length() {
            let audio = AudioData::new(vec![0; 1024], AudioFormat::Mp3);
            assert_eq!(audio.size_bytes(), 1024);
        }

        #[test]
        fn is_empty_returns_true_for_empty_data() {
            let audio = AudioData::new(vec![], AudioFormat::Mp3);
            assert!(audio.is_empty());
        }

        #[test]
        fn is_empty_returns_false_for_non_empty_data() {
            let audio = AudioData::new(vec![1], AudioFormat::Mp3);
            assert!(!audio.is_empty());
        }

        #[test]
        fn into_data_consumes_and_returns_bytes() {
            let original = vec![1, 2, 3, 4, 5];
            let audio = AudioData::new(original.clone(), AudioFormat::Opus);
            let data = audio.into_data();
            assert_eq!(data, original);
        }

        #[test]
        fn filename_includes_extension() {
            let audio = AudioData::new(vec![], AudioFormat::Mp3);
            assert_eq!(audio.filename("voice_message"), "voice_message.mp3");

            let audio = AudioData::new(vec![], AudioFormat::Opus);
            assert_eq!(audio.filename("audio"), "audio.opus");
        }

        #[test]
        fn mime_type_delegates_to_format() {
            let audio = AudioData::new(vec![], AudioFormat::Wav);
            assert_eq!(audio.mime_type(), "audio/wav");
        }
    }

    mod transcription {
        use super::*;

        #[test]
        fn new_creates_simple_transcription() {
            let transcription = Transcription::new("Hello, world!");
            assert_eq!(transcription.text, "Hello, world!");
            assert!(transcription.language.is_none());
            assert!(transcription.confidence.is_none());
            assert!(transcription.duration_ms.is_none());
        }

        #[test]
        fn with_language_sets_language() {
            let transcription = Transcription::new("Hallo").with_language("de");
            assert_eq!(transcription.language, Some("de".to_string()));
        }

        #[test]
        fn with_confidence_sets_confidence() {
            let transcription = Transcription::new("Test").with_confidence(0.95);
            assert_eq!(transcription.confidence, Some(0.95));
        }

        #[test]
        fn with_duration_sets_duration() {
            let transcription = Transcription::new("Test").with_duration(3500);
            assert_eq!(transcription.duration_ms, Some(3500));
        }

        #[test]
        fn is_empty_returns_true_for_empty_text() {
            let transcription = Transcription::new("");
            assert!(transcription.is_empty());
        }

        #[test]
        fn is_empty_returns_true_for_whitespace_only() {
            let transcription = Transcription::new("   \n\t  ");
            assert!(transcription.is_empty());
        }

        #[test]
        fn is_empty_returns_false_for_text() {
            let transcription = Transcription::new("Hello");
            assert!(!transcription.is_empty());
        }
    }

    mod voice_info {
        use super::*;

        #[test]
        fn new_creates_voice_info() {
            let voice = VoiceInfo::new("alloy", "Alloy");
            assert_eq!(voice.id, "alloy");
            assert_eq!(voice.name, "Alloy");
            assert!(voice.description.is_none());
            assert!(voice.languages.is_empty());
            assert!(voice.gender.is_none());
        }
    }
}
