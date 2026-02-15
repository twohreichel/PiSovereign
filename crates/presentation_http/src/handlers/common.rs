//! Shared helper functions for HTTP handlers
//!
//! Eliminates duplication between signal, whatsapp, commands, and approvals handlers.

use domain::entities::AudioFormat;
use domain::value_objects::ConversationId;
use domain::{AgentCommand, SystemCommand};

/// Get the snake_case type name of an `AgentCommand` for metrics/logging
pub fn command_type_name(command: &AgentCommand) -> String {
    match command {
        AgentCommand::MorningBriefing { .. } => "morning_briefing",
        AgentCommand::SummarizeInbox { .. } => "summarize_inbox",
        AgentCommand::Ask { .. } => "ask",
        AgentCommand::DraftEmail { .. } => "draft_email",
        AgentCommand::SendEmail { .. } => "send_email",
        AgentCommand::CreateCalendarEvent { .. } => "create_calendar_event",
        AgentCommand::UpdateCalendarEvent { .. } => "update_calendar_event",
        AgentCommand::ListTasks { .. } => "list_tasks",
        AgentCommand::CreateTask { .. } => "create_task",
        AgentCommand::CompleteTask { .. } => "complete_task",
        AgentCommand::UpdateTask { .. } => "update_task",
        AgentCommand::DeleteTask { .. } => "delete_task",
        AgentCommand::ListTaskLists => "list_task_lists",
        AgentCommand::CreateTaskList { .. } => "create_task_list",
        AgentCommand::Echo { .. } => "echo",
        AgentCommand::Help { .. } => "help",
        AgentCommand::System(sys) => match sys {
            SystemCommand::Status => "status",
            SystemCommand::Version => "version",
            SystemCommand::ListModels => "list_models",
            SystemCommand::ReloadConfig => "reload_config",
            SystemCommand::SwitchModel { .. } => "switch_model",
        },
        AgentCommand::Unknown { .. } => "unknown",
        AgentCommand::WebSearch { .. } => "web_search",
        AgentCommand::CreateReminder { .. } => "create_reminder",
        AgentCommand::ListReminders { .. } => "list_reminders",
        AgentCommand::SnoozeReminder { .. } => "snooze_reminder",
        AgentCommand::AcknowledgeReminder { .. } => "acknowledge_reminder",
        AgentCommand::DeleteReminder { .. } => "delete_reminder",
        AgentCommand::SearchTransit { .. } => "search_transit",
        AgentCommand::ListContacts { .. } => "list_contacts",
        AgentCommand::GetContact { .. } => "get_contact",
        AgentCommand::CreateContact { .. } => "create_contact",
        AgentCommand::UpdateContact { .. } => "update_contact",
        AgentCommand::DeleteContact { .. } => "delete_contact",
        AgentCommand::SearchContacts { .. } => "search_contacts",
    }
    .to_string()
}

/// Create a deterministic `ConversationId` from a channel name and phone number
///
/// Uses a hash-based approach to produce a stable UUID v4 for each
/// (channel, phone) pair, ensuring the same conversation is reused
/// across requests from the same phone on the same channel.
pub fn conversation_id_from_phone(channel: &str, phone: &str) -> ConversationId {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    channel.hash(&mut hasher);
    phone.hash(&mut hasher);
    let hash = hasher.finish();

    // Create UUID bytes from hash
    let bytes: [u8; 16] = {
        let mut b = [0u8; 16];
        b[0..8].copy_from_slice(&hash.to_be_bytes());
        b[8..16].copy_from_slice(&hash.wrapping_mul(31).to_be_bytes());
        // Set version 4 (random) and variant bits
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        b
    };

    ConversationId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

/// Parse an `AudioFormat` from a MIME type string
///
/// Defaults to `Ogg` for unrecognized formats (common for messenger voice messages).
pub fn parse_audio_format(mime_type: &str) -> AudioFormat {
    let mime_lower = mime_type.to_lowercase();

    if mime_lower.contains("opus") {
        AudioFormat::Opus
    } else if mime_lower.contains("ogg") {
        AudioFormat::Ogg
    } else if mime_lower.contains("mp3") || mime_lower.contains("mpeg") {
        AudioFormat::Mp3
    } else if mime_lower.contains("wav") {
        AudioFormat::Wav
    } else {
        // Default to Ogg for voice messages
        AudioFormat::Ogg
    }
}

/// Get the file extension for an `AudioFormat`
pub const fn format_extension(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Opus => "opus",
        AudioFormat::Ogg => "ogg",
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav => "wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_type_names() {
        assert_eq!(
            command_type_name(&AgentCommand::Ask {
                question: "hi".into()
            }),
            "ask"
        );
        assert_eq!(
            command_type_name(&AgentCommand::System(SystemCommand::Status)),
            "status"
        );
        assert_eq!(
            command_type_name(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "x".into()
            })),
            "switch_model"
        );
    }

    #[test]
    fn conversation_id_deterministic() {
        let id1 = conversation_id_from_phone("whatsapp", "+491234567890");
        let id2 = conversation_id_from_phone("whatsapp", "+491234567890");
        assert_eq!(id1, id2);
    }

    #[test]
    fn conversation_id_channel_differs() {
        let wa = conversation_id_from_phone("whatsapp", "+491234567890");
        let sig = conversation_id_from_phone("signal", "+491234567890");
        assert_ne!(wa, sig);
    }

    #[test]
    fn audio_format_parsing() {
        assert_eq!(parse_audio_format("audio/opus"), AudioFormat::Opus);
        assert_eq!(parse_audio_format("audio/ogg"), AudioFormat::Ogg);
        assert_eq!(parse_audio_format("audio/mpeg"), AudioFormat::Mp3);
        assert_eq!(parse_audio_format("audio/wav"), AudioFormat::Wav);
        assert_eq!(parse_audio_format("unknown/type"), AudioFormat::Ogg);
    }

    #[test]
    fn format_extensions() {
        assert_eq!(format_extension(AudioFormat::Opus), "opus");
        assert_eq!(format_extension(AudioFormat::Ogg), "ogg");
        assert_eq!(format_extension(AudioFormat::Mp3), "mp3");
        assert_eq!(format_extension(AudioFormat::Wav), "wav");
    }
}
