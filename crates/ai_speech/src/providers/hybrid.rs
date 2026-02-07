//! Hybrid Speech Provider
//!
//! Combines local and cloud providers with automatic fallback:
//! - Tries local provider first (whisper.cpp / Piper)
//! - Falls back to OpenAI if local fails or is not available
//! - Can be configured to use only local (no fallback)
//!
//! # Architecture
//!
//! ```text
//! Audio Input
//!     │
//!     ▼
//! ┌─────────────────────────────┐
//! │     Hybrid Provider         │
//! │                             │
//! │  ┌─────────┐   ┌─────────┐  │
//! │  │ Local   │──▶│ OpenAI  │  │
//! │  │(primary)│   │(fallback)│  │
//! │  └─────────┘   └─────────┘  │
//! └─────────────────────────────┘
//!     │
//!     ▼
//! Transcription/Audio
//! ```

use async_trait::async_trait;
use tracing::{debug, info, instrument, warn};

use crate::config::{HybridConfig, LocalSttConfig, LocalTtsConfig, SpeechConfig};
use crate::error::SpeechError;
use crate::ports::{SpeechToText, TextToSpeech};
use crate::providers::openai::OpenAISpeechProvider;
use crate::providers::piper::PiperProvider;
use crate::providers::whisper_cpp::WhisperCppProvider;
use crate::types::{AudioData, AudioFormat, Transcription, VoiceInfo};

/// Hybrid speech provider with local-first, cloud fallback
pub struct HybridSpeechProvider {
    /// Local STT provider (whisper.cpp)
    local_stt: Option<WhisperCppProvider>,
    /// Local TTS provider (Piper)
    local_tts: Option<PiperProvider>,
    /// Cloud fallback provider (OpenAI)
    cloud: Option<OpenAISpeechProvider>,
    /// Configuration
    config: HybridConfig,
}

impl std::fmt::Debug for HybridSpeechProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridSpeechProvider")
            .field("local_stt", &self.local_stt.is_some())
            .field("local_tts", &self.local_tts.is_some())
            .field("cloud", &self.cloud.is_some())
            .field("config", &self.config)
            .finish()
    }
}

impl HybridSpeechProvider {
    /// Create a new hybrid provider with all components
    ///
    /// # Arguments
    ///
    /// * `local_stt_config` - Configuration for local STT (whisper.cpp)
    /// * `local_tts_config` - Configuration for local TTS (Piper)
    /// * `cloud_config` - Configuration for cloud fallback (OpenAI)
    /// * `hybrid_config` - Hybrid mode configuration
    ///
    /// # Returns
    ///
    /// Returns the hybrid provider.
    pub fn new(
        local_stt_config: Option<LocalSttConfig>,
        local_tts_config: Option<LocalTtsConfig>,
        cloud_config: Option<SpeechConfig>,
        hybrid_config: HybridConfig,
    ) -> Result<Self, SpeechError> {
        let local_stt = local_stt_config
            .map(WhisperCppProvider::new)
            .transpose()?;

        let local_tts = local_tts_config
            .map(PiperProvider::new)
            .transpose()?;

        let cloud = cloud_config
            .map(OpenAISpeechProvider::new)
            .transpose()?;

        // Validate that we have at least one provider
        if local_stt.is_none() && local_tts.is_none() && cloud.is_none() {
            return Err(SpeechError::Configuration(
                "At least one speech provider must be configured".to_string(),
            ));
        }

        info!(
            "Hybrid provider initialized: local_stt={}, local_tts={}, cloud={}",
            local_stt.is_some(),
            local_tts.is_some(),
            cloud.is_some()
        );

        Ok(Self {
            local_stt,
            local_tts,
            cloud,
            config: hybrid_config,
        })
    }

    /// Create a local-only provider (no cloud fallback)
    pub fn local_only(
        stt_config: LocalSttConfig,
        tts_config: LocalTtsConfig,
    ) -> Result<Self, SpeechError> {
        Self::new(
            Some(stt_config),
            Some(tts_config),
            None,
            HybridConfig {
                prefer_local: true,
                allow_cloud_fallback: false,
                ..Default::default()
            },
        )
    }

    /// Create a cloud-only provider (OpenAI)
    pub fn cloud_only(config: SpeechConfig) -> Result<Self, SpeechError> {
        Self::new(
            None,
            None,
            Some(config),
            HybridConfig {
                prefer_local: false,
                allow_cloud_fallback: true,
                ..Default::default()
            },
        )
    }

    /// Check if local STT is available
    pub async fn is_local_stt_available(&self) -> bool {
        if let Some(ref local) = self.local_stt {
            local.is_available().await
        } else {
            false
        }
    }

    /// Check if local TTS is available
    pub async fn is_local_tts_available(&self) -> bool {
        if let Some(ref local) = self.local_tts {
            local.is_available().await
        } else {
            false
        }
    }

    /// Check if cloud provider is available
    pub async fn is_cloud_available(&self) -> bool {
        if let Some(ref cloud) = self.cloud {
            SpeechToText::is_available(cloud).await
        } else {
            false
        }
    }
}

#[async_trait]
impl SpeechToText for HybridSpeechProvider {
    #[instrument(skip(self, audio), fields(format = ?audio.format()))]
    async fn transcribe(&self, audio: AudioData) -> Result<Transcription, SpeechError> {
        let mut last_error: Option<SpeechError> = None;

        // Try local first if preferred and available
        if self.config.prefer_local {
            if let Some(ref local) = self.local_stt {
                if local.is_available().await {
                    debug!("Attempting local STT with whisper.cpp");
                    match local.transcribe(audio.clone()).await {
                        Ok(result) => {
                            info!("Local STT succeeded");
                            return Ok(result);
                        }
                        Err(e) => {
                            warn!("Local STT failed: {e}");
                            last_error = Some(e);
                        }
                    }
                } else {
                    debug!("Local STT not available");
                }
            }
        }

        // Fall back to cloud if allowed
        if self.config.allow_cloud_fallback {
            if let Some(ref cloud) = self.cloud {
                debug!("Attempting cloud STT with OpenAI");
                match cloud.transcribe(audio).await {
                    Ok(result) => {
                        info!("Cloud STT succeeded (fallback)");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("Cloud STT failed: {e}");
                        last_error = Some(e);
                    }
                }
            }
        }

        // All providers failed
        Err(last_error.unwrap_or_else(|| {
            SpeechError::NotAvailable("No STT provider available".to_string())
        }))
    }

    #[instrument(skip(self, audio), fields(format = ?audio.format(), language = %language))]
    async fn transcribe_with_language(
        &self,
        audio: AudioData,
        language: &str,
    ) -> Result<Transcription, SpeechError> {
        let mut last_error: Option<SpeechError> = None;

        // Try local first if preferred
        if self.config.prefer_local {
            if let Some(ref local) = self.local_stt {
                if local.is_available().await {
                    debug!("Attempting local STT with language hint: {}", language);
                    match local.transcribe_with_language(audio.clone(), language).await {
                        Ok(result) => {
                            info!("Local STT succeeded");
                            return Ok(result);
                        }
                        Err(e) => {
                            warn!("Local STT failed: {e}");
                            last_error = Some(e);
                        }
                    }
                }
            }
        }

        // Fall back to cloud
        if self.config.allow_cloud_fallback {
            if let Some(ref cloud) = self.cloud {
                debug!("Attempting cloud STT with language hint: {}", language);
                match cloud.transcribe_with_language(audio, language).await {
                    Ok(result) => {
                        info!("Cloud STT succeeded (fallback)");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("Cloud STT failed: {e}");
                        last_error = Some(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SpeechError::NotAvailable("No STT provider available".to_string())
        }))
    }

    async fn is_available(&self) -> bool {
        self.is_local_stt_available().await || self.is_cloud_available().await
    }

    fn model_name(&self) -> &str {
        // Return the model name of the preferred/available provider
        if self.config.prefer_local {
            if let Some(ref local) = self.local_stt {
                return local.model_name();
            }
        }
        if let Some(ref cloud) = self.cloud {
            return SpeechToText::model_name(cloud);
        }
        "hybrid"
    }
}

#[async_trait]
impl TextToSpeech for HybridSpeechProvider {
    #[instrument(skip(self, text), fields(text_len = text.len()))]
    async fn synthesize(&self, text: &str, voice: Option<&str>) -> Result<AudioData, SpeechError> {
        let mut last_error: Option<SpeechError> = None;

        // Try local first if preferred
        if self.config.prefer_local {
            if let Some(ref local) = self.local_tts {
                if local.is_available().await {
                    debug!("Attempting local TTS with Piper");
                    match local.synthesize(text, voice).await {
                        Ok(result) => {
                            info!("Local TTS succeeded");
                            return Ok(result);
                        }
                        Err(e) => {
                            warn!("Local TTS failed: {e}");
                            last_error = Some(e);
                        }
                    }
                } else {
                    debug!("Local TTS not available");
                }
            }
        }

        // Fall back to cloud
        if self.config.allow_cloud_fallback {
            if let Some(ref cloud) = self.cloud {
                debug!("Attempting cloud TTS with OpenAI");
                match cloud.synthesize(text, voice).await {
                    Ok(result) => {
                        info!("Cloud TTS succeeded (fallback)");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("Cloud TTS failed: {e}");
                        last_error = Some(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SpeechError::NotAvailable("No TTS provider available".to_string())
        }))
    }

    #[instrument(skip(self, text), fields(text_len = text.len(), format = ?format))]
    async fn synthesize_with_format(
        &self,
        text: &str,
        voice: Option<&str>,
        format: AudioFormat,
    ) -> Result<AudioData, SpeechError> {
        let mut last_error: Option<SpeechError> = None;

        // Try local first if preferred
        if self.config.prefer_local {
            if let Some(ref local) = self.local_tts {
                if local.is_available().await {
                    debug!("Attempting local TTS with format {:?}", format);
                    match local.synthesize_with_format(text, voice, format).await {
                        Ok(result) => {
                            info!("Local TTS succeeded");
                            return Ok(result);
                        }
                        Err(e) => {
                            warn!("Local TTS failed: {e}");
                            last_error = Some(e);
                        }
                    }
                }
            }
        }

        // Fall back to cloud
        if self.config.allow_cloud_fallback {
            if let Some(ref cloud) = self.cloud {
                debug!("Attempting cloud TTS with format {:?}", format);
                match cloud.synthesize_with_format(text, voice, format).await {
                    Ok(result) => {
                        info!("Cloud TTS succeeded (fallback)");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("Cloud TTS failed: {e}");
                        last_error = Some(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SpeechError::NotAvailable("No TTS provider available".to_string())
        }))
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, SpeechError> {
        let mut all_voices = Vec::new();

        // Collect local voices
        if let Some(ref local) = self.local_tts {
            if let Ok(voices) = local.list_voices().await {
                for mut voice in voices {
                    voice.description = Some(format!(
                        "[Local] {}",
                        voice.description.unwrap_or_default()
                    ));
                    all_voices.push(voice);
                }
            }
        }

        // Collect cloud voices
        if self.config.allow_cloud_fallback {
            if let Some(ref cloud) = self.cloud {
                if let Ok(voices) = cloud.list_voices().await {
                    for mut voice in voices {
                        voice.description = Some(format!(
                            "[Cloud] {}",
                            voice.description.unwrap_or_default()
                        ));
                        all_voices.push(voice);
                    }
                }
            }
        }

        Ok(all_voices)
    }

    async fn is_available(&self) -> bool {
        self.is_local_tts_available().await || self.is_cloud_available().await
    }

    fn model_name(&self) -> &str {
        if self.config.prefer_local {
            if let Some(ref local) = self.local_tts {
                return local.model_name();
            }
        }
        if let Some(ref cloud) = self.cloud {
            return TextToSpeech::model_name(cloud);
        }
        "hybrid"
    }

    fn default_voice(&self) -> &str {
        if self.config.prefer_local {
            if let Some(ref local) = self.local_tts {
                return local.default_voice();
            }
        }
        if let Some(ref cloud) = self.cloud {
            return TextToSpeech::default_voice(cloud);
        }
        "default"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;

    fn test_local_stt_config() -> LocalSttConfig {
        LocalSttConfig {
            executable_path: PathBuf::from("whisper-cpp"),
            model_path: PathBuf::from("/models/ggml-base.bin"),
            threads: 4,
            default_language: Some("de".to_string()),
        }
    }

    fn test_local_tts_config() -> LocalTtsConfig {
        LocalTtsConfig {
            executable_path: PathBuf::from("piper"),
            default_model_path: PathBuf::from("/models/de_DE-thorsten-medium.onnx"),
            default_voice: "de_DE-thorsten-medium".to_string(),
            voices: HashMap::new(),
            output_format: AudioFormat::Wav,
            length_scale: 1.0,
            sentence_silence: 0.2,
        }
    }

    fn test_cloud_config() -> SpeechConfig {
        SpeechConfig {
            openai_api_key: Some("test-key".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn creates_hybrid_provider() {
        let provider = HybridSpeechProvider::new(
            Some(test_local_stt_config()),
            Some(test_local_tts_config()),
            Some(test_cloud_config()),
            HybridConfig::default(),
        );
        assert!(provider.is_ok());
    }

    #[test]
    fn creates_local_only_provider() {
        let provider = HybridSpeechProvider::local_only(
            test_local_stt_config(),
            test_local_tts_config(),
        );
        assert!(provider.is_ok());
    }

    #[test]
    fn creates_cloud_only_provider() {
        let provider = HybridSpeechProvider::cloud_only(test_cloud_config());
        assert!(provider.is_ok());
    }

    #[test]
    fn fails_without_any_provider() {
        let provider = HybridSpeechProvider::new(
            None,
            None,
            None,
            HybridConfig::default(),
        );
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn is_available_checks_all_providers() {
        let provider = HybridSpeechProvider::new(
            Some(test_local_stt_config()),
            Some(test_local_tts_config()),
            Some(test_cloud_config()),
            HybridConfig::default(),
        ).unwrap();

        // Will return false since local providers aren't actually installed
        // but the method shouldn't panic
        let _ = SpeechToText::is_available(&provider).await;
    }
}
