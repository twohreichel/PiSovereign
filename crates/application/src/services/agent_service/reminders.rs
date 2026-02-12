//! Reminder CRUD handlers: create, list, snooze, acknowledge, delete

use chrono::Utc;
use domain::{ReminderId, UserId};
use tracing::info;

use super::{AgentService, ExecutionResult};
use crate::{
    error::ApplicationError,
    ports::ReminderQuery,
    services::{
        format_acknowledge_confirmation, format_custom_reminder, format_reminder_list,
        format_snooze_confirmation,
    },
};

impl AgentService {
    /// Handle creating a custom reminder
    pub(super) async fn handle_create_reminder(
        &self,
        title: &str,
        remind_at: &str,
        description: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref reminder_service) = self.reminder_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🔔 Reminder service not yet configured. Cannot create: '{title}'"
                ),
            });
        };

        // Parse the remind_at time
        let remind_at_time = chrono::DateTime::parse_from_rfc3339(remind_at)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(remind_at, "%Y-%m-%dT%H:%M:%S")
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(remind_at, "%Y-%m-%dT%H:%M"))
                    .map(|ndt| ndt.and_utc())
            })
            .map_err(|_| {
                ApplicationError::CommandFailed(format!(
                    "Invalid remind_at time format: '{remind_at}'"
                ))
            })?;

        // Create the reminder
        let mut reminder = domain::Reminder::new(
            UserId::default(),
            domain::ReminderSource::Custom,
            title.to_string(),
            remind_at_time,
        );
        if let Some(desc) = description {
            reminder.description = Some(desc.to_string());
        }

        let reminder_id = reminder.id.to_string();
        reminder_service.save(&reminder).await?;

        let formatted = format_custom_reminder(&reminder);
        info!(reminder_id = %reminder_id, title = %title, "Created reminder");

        Ok(ExecutionResult {
            success: true,
            response: format!("✅ Erinnerung erstellt!\n\n{formatted}"),
        })
    }

    /// Handle listing reminders
    pub(super) async fn handle_list_reminders(
        &self,
        include_done: Option<bool>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref reminder_service) = self.reminder_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "🔔 Reminder service not yet configured.".to_string(),
            });
        };

        let query = ReminderQuery {
            include_terminal: include_done.unwrap_or(false),
            ..Default::default()
        };

        let reminders = reminder_service.query(&query).await?;
        let response = format_reminder_list(&reminders);

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle snoozing a reminder
    pub(super) async fn handle_snooze_reminder(
        &self,
        reminder_id: &str,
        duration_minutes: Option<u32>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref reminder_service) = self.reminder_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🔔 Reminder service not yet configured. Cannot snooze: {reminder_id}"
                ),
            });
        };

        let rid = ReminderId::parse(reminder_id).map_err(|_| {
            ApplicationError::NotFound(format!("Invalid reminder ID: {reminder_id}"))
        })?;
        let Some(mut reminder) = reminder_service.get(&rid).await? else {
            return Ok(ExecutionResult {
                success: false,
                response: format!("❌ Erinnerung nicht gefunden: {reminder_id}"),
            });
        };

        let duration_mins = i64::from(duration_minutes.unwrap_or(15));
        let new_remind_at = Utc::now() + chrono::Duration::minutes(duration_mins);
        if reminder.snooze(new_remind_at) {
            reminder_service.update(&reminder).await?;
            let new_time = reminder.remind_at.format("%H:%M");
            let response = format_snooze_confirmation(&reminder, new_remind_at);
            info!(reminder_id = %reminder_id, new_time = %new_time, "Snoozed reminder");
            Ok(ExecutionResult {
                success: true,
                response,
            })
        } else {
            Ok(ExecutionResult {
                success: false,
                response: format!(
                    "❌ Maximale Snooze-Anzahl erreicht für: **{}**\n\n\
                     Diese Erinnerung kann nicht mehr verschoben werden.",
                    reminder.title
                ),
            })
        }
    }

    /// Handle acknowledging (completing) a reminder
    pub(super) async fn handle_acknowledge_reminder(
        &self,
        reminder_id: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref reminder_service) = self.reminder_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🔔 Reminder service not yet configured. Cannot acknowledge: {reminder_id}"
                ),
            });
        };

        let rid = ReminderId::parse(reminder_id).map_err(|_| {
            ApplicationError::NotFound(format!("Invalid reminder ID: {reminder_id}"))
        })?;
        let Some(mut reminder) = reminder_service.get(&rid).await? else {
            return Ok(ExecutionResult {
                success: false,
                response: format!("❌ Erinnerung nicht gefunden: {reminder_id}"),
            });
        };

        reminder.acknowledge();
        reminder_service.update(&reminder).await?;
        let response = format_acknowledge_confirmation(&reminder);
        info!(reminder_id = %reminder_id, "Acknowledged reminder");

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle deleting a reminder
    pub(super) async fn handle_delete_reminder(
        &self,
        reminder_id: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref reminder_service) = self.reminder_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🔔 Reminder service not yet configured. Cannot delete: {reminder_id}"
                ),
            });
        };

        let rid = ReminderId::parse(reminder_id).map_err(|_| {
            ApplicationError::NotFound(format!("Invalid reminder ID: {reminder_id}"))
        })?;
        let Some(reminder) = reminder_service.get(&rid).await? else {
            return Ok(ExecutionResult {
                success: false,
                response: format!("❌ Erinnerung nicht gefunden: {reminder_id}"),
            });
        };

        let title = reminder.title.clone();
        reminder_service.delete(&rid).await?;
        info!(reminder_id = %reminder_id, title = %title, "Deleted reminder");

        Ok(ExecutionResult {
            success: true,
            response: format!("🗑️ Erinnerung gelöscht: **{title}**"),
        })
    }
}
