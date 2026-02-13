//! Signal message polling background task
//!
//! Periodically polls the signal-cli daemon for incoming messages
//! and processes them through the agent pipeline.

use std::sync::Arc;
use std::time::Duration;

use application::AgentService;
use application::VoiceMessageService;
use application::ports::ConversationStore;
use domain::PhoneNumber;
use domain::entities::{Conversation, ConversationSource};
use integration_signal::SignalClient;
use tracing::{debug, error, info, warn};

use crate::handlers::common::{conversation_id_from_phone, parse_audio_format};

/// Spawn a background task that periodically polls Signal for new messages.
///
/// The task calls `signal_client.receive()` at the configured interval,
/// processes incoming text and audio messages through the agent service,
/// and sends responses back via Signal.
///
/// Returns a `JoinHandle` that can be used to abort the task on shutdown.
///
/// # Arguments
///
/// * `signal_client` - The Signal client for receiving/sending messages
/// * `agent_service` - The agent service for processing messages
/// * `conversation_store` - Optional conversation persistence store
/// * `voice_message_service` - Optional voice message processor (STT/TTS)
/// * `poll_interval` - How often to poll for new messages
#[allow(clippy::too_many_arguments)]
pub fn spawn_signal_polling_task(
    signal_client: Arc<SignalClient>,
    agent_service: Arc<AgentService>,
    conversation_store: Option<Arc<dyn ConversationStore>>,
    voice_message_service: Option<Arc<VoiceMessageService>>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    info!(
        interval_secs = poll_interval.as_secs(),
        "Starting Signal auto-polling background task"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(poll_interval);
        // The first tick completes immediately — skip it to give
        // the signal-cli daemon time to fully start up.
        ticker.tick().await;

        loop {
            ticker.tick().await;
            poll_and_process(
                &signal_client,
                &agent_service,
                conversation_store.as_ref(),
                voice_message_service.as_ref(),
            )
            .await;
        }
    })
}

/// Single poll iteration: receive envelopes and process each message.
async fn poll_and_process(
    signal_client: &SignalClient,
    agent_service: &AgentService,
    conversation_store: Option<&Arc<dyn ConversationStore>>,
    voice_message_service: Option<&Arc<VoiceMessageService>>,
) {
    // Non-blocking poll (timeout = 1s to avoid long blocking)
    let envelopes = match signal_client.receive(1).await {
        Ok(envs) => envs,
        Err(e) => {
            debug!(error = %e, "Signal poll: failed to receive (daemon may be unavailable)");
            return;
        },
    };

    if envelopes.is_empty() {
        return;
    }

    info!(
        count = envelopes.len(),
        "Signal auto-poll: processing messages"
    );

    for envelope in envelopes {
        let sender = envelope
            .source
            .as_deref()
            .or(envelope.source_uuid.as_deref())
            .unwrap_or("unknown");

        // Whitelist check
        if !signal_client.is_whitelisted(sender) {
            debug!(sender = %sender, "Signal auto-poll: ignoring non-whitelisted sender");
            continue;
        }

        if let Some(data_message) = envelope.data_message {
            let timestamp = data_message.timestamp;

            // Handle text messages
            if let Some(ref body) = data_message.body {
                handle_text_message(
                    signal_client,
                    agent_service,
                    conversation_store,
                    sender,
                    timestamp,
                    body,
                )
                .await;
            }

            // Handle audio attachments
            for attachment in &data_message.attachments {
                if attachment.content_type.starts_with("audio/") {
                    handle_audio_message(
                        signal_client,
                        agent_service,
                        voice_message_service,
                        sender,
                        timestamp,
                        attachment,
                    )
                    .await;
                }
            }

            // Send read receipt
            if let Err(e) = signal_client
                .send_read_receipt(sender, vec![timestamp])
                .await
            {
                warn!(error = %e, "Signal auto-poll: failed to send read receipt");
            }
        }
    }
}

/// Process a text message through the agent and reply via Signal.
async fn handle_text_message(
    signal_client: &SignalClient,
    agent_service: &AgentService,
    conversation_store: Option<&Arc<dyn ConversationStore>>,
    from: &str,
    timestamp: i64,
    text: &str,
) {
    debug!(
        from = %from,
        timestamp = timestamp,
        text_len = text.len(),
        "Signal auto-poll: processing text message"
    );

    let phone = match PhoneNumber::new(from) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, from = %from, "Signal auto-poll: invalid sender phone number");
            return;
        },
    };

    // Load or create conversation
    let mut conversation = if let Some(store) = conversation_store {
        match store
            .get_by_phone_number(ConversationSource::Signal, from)
            .await
        {
            Ok(Some(conv)) => {
                debug!(
                    conversation_id = %conv.id,
                    message_count = conv.messages.len(),
                    "Signal auto-poll: resuming existing conversation"
                );
                conv
            },
            Ok(None) => {
                debug!("Signal auto-poll: creating new conversation");
                Conversation::for_messenger(ConversationSource::Signal, phone.clone())
            },
            Err(e) => {
                warn!(error = %e, "Signal auto-poll: failed to load conversation");
                Conversation::for_messenger(ConversationSource::Signal, phone.clone())
            },
        }
    } else {
        Conversation::for_messenger(ConversationSource::Signal, phone)
    };

    conversation.add_user_message(text);

    // Process through agent
    let result = agent_service.handle_input(text).await;

    match result {
        Ok(agent_result) => {
            let response_text = agent_result.response.clone();
            conversation.add_assistant_message(&response_text);

            // Persist conversation
            if let Some(store) = conversation_store {
                if let Err(e) = store.save(&conversation).await {
                    error!(
                        error = %e,
                        conversation_id = %conversation.id,
                        "Signal auto-poll: failed to persist conversation"
                    );
                }
            }

            // Send response back
            if let Err(e) = signal_client.send_text(from, &response_text).await {
                error!(
                    error = %e,
                    from = %from,
                    "Signal auto-poll: failed to send response"
                );
            } else {
                info!(
                    from = %from,
                    timestamp = timestamp,
                    conversation_id = %conversation.id,
                    success = agent_result.success,
                    "Signal auto-poll: text message processed and response sent"
                );
            }
        },
        Err(e) => {
            error!(
                error = %e,
                from = %from,
                timestamp = timestamp,
                "Signal auto-poll: failed to process text message"
            );

            // Persist conversation with user message only
            if let Some(store) = conversation_store {
                if let Err(persist_err) = store.save(&conversation).await {
                    warn!(
                        error = %persist_err,
                        "Signal auto-poll: failed to persist conversation after error"
                    );
                }
            }

            // Send error message to user
            let _ = signal_client
                .send_text(
                    from,
                    "Sorry, I couldn't process your message right now. Please try again later.",
                )
                .await;
        },
    }
}

/// Process an audio/voice message through STT → agent → TTS and reply.
async fn handle_audio_message(
    signal_client: &SignalClient,
    _agent_service: &AgentService,
    voice_message_service: Option<&Arc<VoiceMessageService>>,
    from: &str,
    timestamp: i64,
    attachment: &integration_signal::Attachment,
) {
    debug!(
        from = %from,
        timestamp = timestamp,
        content_type = ?attachment.content_type,
        "Signal auto-poll: processing audio message"
    );

    let Some(voice_service) = voice_message_service else {
        warn!("Signal auto-poll: voice service not configured, sending text fallback");
        let _ = signal_client
            .send_text(
                from,
                "Voice messages are not supported yet. Please send a text message.",
            )
            .await;
        return;
    };

    let Some(file_path) = signal_client.get_attachment_path(attachment) else {
        error!(
            timestamp = timestamp,
            "Signal auto-poll: no file path for attachment"
        );
        return;
    };

    let audio_data = match tokio::fs::read(&file_path).await {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, path = %file_path, "Signal auto-poll: failed to read audio file");
            return;
        },
    };

    info!(
        audio_size = audio_data.len(),
        content_type = %attachment.content_type,
        "Signal auto-poll: read audio attachment"
    );

    let format = parse_audio_format(&attachment.content_type);
    let conversation_id = conversation_id_from_phone("signal", from);

    let result = voice_service
        .process_voice_message(
            audio_data,
            format,
            conversation_id,
            Some(timestamp.to_string()),
        )
        .await;

    match result {
        Ok(voice_result) => {
            info!(
                from = %from,
                timestamp = timestamp,
                transcription_len = voice_result.transcription.len(),
                has_audio = voice_result.response_audio.is_some(),
                "Signal auto-poll: voice message processed"
            );

            // Send audio response if available, otherwise text
            if let Some(ref audio_response) = voice_result.response_audio {
                match send_audio_response(signal_client, from, audio_response).await {
                    Ok(()) => {},
                    Err(e) => {
                        warn!(error = %e, "Signal auto-poll: audio send failed, falling back to text");
                        let _ = signal_client
                            .send_text(from, &voice_result.response_text)
                            .await;
                    },
                }
            } else if let Err(e) = signal_client
                .send_text(from, &voice_result.response_text)
                .await
            {
                error!(error = %e, "Signal auto-poll: failed to send text response");
            }
        },
        Err(e) => {
            error!(
                error = %e,
                from = %from,
                timestamp = timestamp,
                "Signal auto-poll: voice processing failed"
            );
            let _ = signal_client
                .send_text(
                    from,
                    "Sorry, I couldn't process your voice message. Please try again or send a text message.",
                )
                .await;
        },
    }
}

/// Write audio to a temp file and send it via Signal.
async fn send_audio_response(
    signal_client: &SignalClient,
    to: &str,
    audio_response: &application::ports::SynthesisResult,
) -> Result<(), String> {
    use crate::handlers::common::format_extension;

    let temp_dir = std::env::temp_dir().join("pisovereign-signal");
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("Failed to create temp dir: {e}"))?;

    let ext = format_extension(audio_response.format);
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let temp_path = temp_dir.join(&filename);

    tokio::fs::write(&temp_path, &audio_response.audio_data)
        .await
        .map_err(|e| format!("Failed to write temp audio: {e}"))?;

    let result = signal_client
        .send_audio(to, &temp_path.to_string_lossy())
        .await
        .map_err(|e| format!("Failed to send audio: {e}"));

    let _ = tokio::fs::remove_file(&temp_path).await;

    result.map(|_| ())
}
