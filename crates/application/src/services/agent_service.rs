//! Agent service - Command execution and orchestration

use std::{fmt, fmt::Write as _, sync::Arc, time::Instant};

use chrono::Utc;
use domain::{
    AgentCommand, GeoLocation, PersistedEmailDraft, ReminderId, SystemCommand, TaskItem, UserId,
};
use tracing::{debug, info, instrument, warn};

use crate::{
    command_parser::CommandParser,
    error::ApplicationError,
    ports::{
        DraftStorePort, InferencePort, ReminderPort, ReminderQuery, Task, TaskPort, TransitPort,
        UserProfileStore, WeatherPort, WebSearchPort, format_connections,
    },
    services::{
        briefing_service::WeatherSummary, format_acknowledge_confirmation, format_custom_reminder,
        format_reminder_list, format_snooze_confirmation,
    },
};

/// Result of executing an agent command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The command that was executed
    pub command: AgentCommand,
    /// Whether the command succeeded
    pub success: bool,
    /// Response message to send back to the user
    pub response: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether approval was required and granted
    pub approval_status: Option<ApprovalStatus>,
}

/// Status of approval for commands that require it
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// No approval needed for this command
    NotRequired,
    /// Approval is pending
    Pending,
    /// Approval was granted
    Granted,
    /// Approval was denied
    Denied,
}

/// Service for handling agent commands
pub struct AgentService {
    inference: Arc<dyn InferencePort>,
    parser: CommandParser,
    /// Optional calendar service for briefing integration
    calendar_service: Option<Arc<super::CalendarService>>,
    /// Optional email service for briefing integration
    email_service: Option<Arc<super::EmailService>>,
    /// Optional draft store for email draft persistence
    draft_store: Option<Arc<dyn DraftStorePort>>,
    /// Optional user profile store for personalization
    user_profile_store: Option<Arc<dyn UserProfileStore>>,
    /// Optional task service for todo/task integration
    task_service: Option<Arc<dyn TaskPort>>,
    /// Optional weather service for weather integration
    weather_service: Option<Arc<dyn WeatherPort>>,
    /// Optional web search service for internet search
    websearch_service: Option<Arc<dyn WebSearchPort>>,
    /// Optional reminder service for reminder management
    reminder_service: Option<Arc<dyn ReminderPort>>,
    /// Optional transit service for ÖPNV connections
    transit_service: Option<Arc<dyn TransitPort>>,
    /// Default location for weather when user profile has no location
    default_weather_location: Option<GeoLocation>,
    /// Home location for transit searches (used when "from" is not specified)
    home_location: Option<GeoLocation>,
}

impl fmt::Debug for AgentService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentService")
            .field("parser", &self.parser)
            .field("has_calendar", &self.calendar_service.is_some())
            .field("has_email", &self.email_service.is_some())
            .field("has_draft_store", &self.draft_store.is_some())
            .field("has_user_profile", &self.user_profile_store.is_some())
            .field("has_task", &self.task_service.is_some())
            .field("has_weather", &self.weather_service.is_some())
            .field("has_websearch", &self.websearch_service.is_some())
            .field("has_reminder", &self.reminder_service.is_some())
            .field("has_transit", &self.transit_service.is_some())
            .finish_non_exhaustive()
    }
}

impl AgentService {
    /// Create a new agent service
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            parser: CommandParser::new(),
            calendar_service: None,
            email_service: None,
            draft_store: None,
            user_profile_store: None,
            task_service: None,
            weather_service: None,
            websearch_service: None,
            reminder_service: None,
            transit_service: None,
            default_weather_location: None,
            home_location: None,
        }
    }

    /// Add calendar service for briefing integration
    #[must_use]
    pub fn with_calendar_service(mut self, service: Arc<super::CalendarService>) -> Self {
        self.calendar_service = Some(service);
        self
    }

    /// Add email service for briefing integration
    #[must_use]
    pub fn with_email_service(mut self, service: Arc<super::EmailService>) -> Self {
        self.email_service = Some(service);
        self
    }

    /// Add draft store for email draft persistence
    #[must_use]
    pub fn with_draft_store(mut self, store: Arc<dyn DraftStorePort>) -> Self {
        self.draft_store = Some(store);
        self
    }

    /// Add user profile store for personalization
    #[must_use]
    pub fn with_user_profile_store(mut self, store: Arc<dyn UserProfileStore>) -> Self {
        self.user_profile_store = Some(store);
        self
    }

    /// Add task service for todo/task integration
    #[must_use]
    pub fn with_task_service(mut self, service: Arc<dyn TaskPort>) -> Self {
        self.task_service = Some(service);
        self
    }

    /// Add weather service for weather integration
    #[must_use]
    pub fn with_weather_service(mut self, service: Arc<dyn WeatherPort>) -> Self {
        self.weather_service = Some(service);
        self
    }

    /// Add web search service for internet search capabilities
    #[must_use]
    pub fn with_websearch_service(mut self, service: Arc<dyn WebSearchPort>) -> Self {
        self.websearch_service = Some(service);
        self
    }

    /// Add reminder service for reminder management
    #[must_use]
    pub fn with_reminder_service(mut self, service: Arc<dyn ReminderPort>) -> Self {
        self.reminder_service = Some(service);
        self
    }

    /// Add transit service for ÖPNV connection searches
    #[must_use]
    pub fn with_transit_service(mut self, service: Arc<dyn TransitPort>) -> Self {
        self.transit_service = Some(service);
        self
    }

    /// Set default weather location (fallback when user profile has no location)
    #[must_use]
    pub const fn with_default_weather_location(mut self, location: GeoLocation) -> Self {
        self.default_weather_location = Some(location);
        self
    }

    /// Set home location for transit searches (used when "from" is not specified)
    #[must_use]
    pub const fn with_home_location(mut self, location: GeoLocation) -> Self {
        self.home_location = Some(location);
        self
    }

    /// Parse and execute a command from natural language input
    #[instrument(skip(self, input), fields(input_len = input.len()))]
    pub async fn handle_input(&self, input: &str) -> Result<CommandResult, ApplicationError> {
        self.handle_input_with_user(input, None).await
    }

    /// Parse and execute a command with explicit user context
    ///
    /// This method should be used when the caller has access to the authenticated
    /// user's identity (e.g., from `RequestContext` in HTTP handlers).
    #[instrument(skip(self, input, user_id), fields(input_len = input.len()))]
    pub async fn handle_input_with_user(
        &self,
        input: &str,
        user_id: Option<UserId>,
    ) -> Result<CommandResult, ApplicationError> {
        let start = Instant::now();

        // First, try to parse the command using the LLM
        let command = self.parser.parse_with_llm(&self.inference, input).await?;

        info!(command = ?command, "Parsed command from input");

        // Check if approval is required
        if command.requires_approval() {
            debug!(command = ?command, "Command requires approval");
            #[allow(clippy::cast_possible_truncation)]
            return Ok(CommandResult {
                command: command.clone(),
                success: false,
                response: format!(
                    "⚠️ Diese Aktion erfordert Bestätigung: {}\n\nBitte bestätige mit 'OK' oder breche ab mit 'Abbrechen'.",
                    command.description()
                ),
                execution_time_ms: start.elapsed().as_millis() as u64,
                approval_status: Some(ApprovalStatus::Pending),
            });
        }

        // Execute the command with user context
        let result = self.execute_command_with_user(&command, user_id).await?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(CommandResult {
            command,
            success: result.success,
            response: result.response,
            execution_time_ms: start.elapsed().as_millis() as u64,
            approval_status: Some(ApprovalStatus::NotRequired),
        })
    }

    /// Execute a specific command (after parsing/approval)
    #[instrument(skip(self, command))]
    pub async fn execute_command(
        &self,
        command: &AgentCommand,
    ) -> Result<ExecutionResult, ApplicationError> {
        self.execute_command_with_user(command, None).await
    }

    /// Execute a specific command with explicit user context
    ///
    /// This method should be used when the caller has access to the authenticated
    /// user's identity. Commands that need user context (like fetching tasks)
    /// will use the provided user ID instead of the default.
    #[instrument(skip(self, command, user_id))]
    pub async fn execute_command_with_user(
        &self,
        command: &AgentCommand,
        user_id: Option<UserId>,
    ) -> Result<ExecutionResult, ApplicationError> {
        match command {
            AgentCommand::Echo { message } => Ok(ExecutionResult {
                success: true,
                response: format!("🔊 {message}"),
            }),

            AgentCommand::Help { command: cmd } => {
                let help_text = self.generate_help(cmd.as_deref());
                Ok(ExecutionResult {
                    success: true,
                    response: help_text,
                })
            },

            AgentCommand::System(sys_cmd) => self.handle_system_command(sys_cmd).await,

            AgentCommand::Ask { question } => {
                let response = self.inference.generate(question).await?;
                Ok(ExecutionResult {
                    success: true,
                    response: response.content,
                })
            },

            AgentCommand::MorningBriefing { date } => {
                self.handle_morning_briefing(*date, user_id).await
            },

            AgentCommand::SummarizeInbox {
                count,
                only_important,
            } => self.handle_summarize_inbox(*count, *only_important).await,

            AgentCommand::Unknown { original_input } => {
                warn!(input = %original_input, "Unknown command received");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❓ I could not understand the command: '{original_input}'\n\n\
                         Type 'help' for a list of available commands."
                    ),
                })
            },

            // Draft email - create and store draft
            AgentCommand::DraftEmail { to, subject, body } => {
                self.handle_draft_email(to.as_str(), subject.as_deref(), body)
                    .await
            },

            // List tasks - read-only, doesn't require approval
            AgentCommand::ListTasks {
                status,
                priority,
                list,
            } => {
                self.handle_list_tasks(status.as_ref(), priority.as_ref(), list.as_deref())
                    .await
            },

            // List task lists - read-only, doesn't require approval
            AgentCommand::ListTaskLists => self.handle_list_task_lists().await,

            // Commands that require approval - should not reach here without approval
            AgentCommand::CreateCalendarEvent { .. }
            | AgentCommand::UpdateCalendarEvent { .. }
            | AgentCommand::CreateTask { .. }
            | AgentCommand::CompleteTask { .. }
            | AgentCommand::UpdateTask { .. }
            | AgentCommand::DeleteTask { .. }
            | AgentCommand::CreateTaskList { .. }
            | AgentCommand::SendEmail { .. } => {
                Err(ApplicationError::ApprovalRequired(command.description()))
            },

            // Web search - requires websearch service to be configured
            AgentCommand::WebSearch { query, max_results } => {
                self.handle_web_search(query, *max_results).await
            },

            // Reminder commands
            AgentCommand::CreateReminder {
                title,
                remind_at,
                description,
            } => {
                self.handle_create_reminder(title, remind_at, description.as_deref())
                    .await
            },
            AgentCommand::ListReminders { include_done } => {
                self.handle_list_reminders(*include_done).await
            },
            AgentCommand::SnoozeReminder {
                reminder_id,
                duration_minutes,
            } => {
                self.handle_snooze_reminder(reminder_id, *duration_minutes)
                    .await
            },
            AgentCommand::AcknowledgeReminder { reminder_id } => {
                self.handle_acknowledge_reminder(reminder_id).await
            },
            AgentCommand::DeleteReminder { reminder_id } => {
                self.handle_delete_reminder(reminder_id).await
            },

            // Transit search
            AgentCommand::SearchTransit {
                from,
                to,
                departure,
            } => {
                self.handle_search_transit(from, to, departure.as_deref())
                    .await
            },
        }
    }

    /// Handle system commands
    async fn handle_system_command(
        &self,
        cmd: &SystemCommand,
    ) -> Result<ExecutionResult, ApplicationError> {
        match cmd {
            SystemCommand::Status => {
                let healthy = self.inference.is_healthy().await;
                let status = if healthy {
                    "🟢 Online"
                } else {
                    "🔴 Offline"
                };
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "📊 System Status:\n\n\
                         Hailo Inference: {}\n\
                         Model: {}\n\
                         Version: {}",
                        status,
                        self.inference.current_model(),
                        env!("CARGO_PKG_VERSION")
                    ),
                })
            },

            SystemCommand::Version => Ok(ExecutionResult {
                success: true,
                response: format!(
                    "🤖 PiSovereign v{}\n\
                     Rust Edition 2024\n\
                     Hailo-10H AI HAT+ 2",
                    env!("CARGO_PKG_VERSION")
                ),
            }),

            SystemCommand::ListModels => {
                let current_model = self.inference.current_model();
                match self.inference.list_available_models().await {
                    Ok(models) => {
                        let model_list: String = models
                            .iter()
                            .map(|m| {
                                if m == &current_model {
                                    format!("• {m} (active)")
                                } else {
                                    format!("• {m}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        Ok(ExecutionResult {
                            success: true,
                            response: format!(
                                "📦 Available Models:\n\n{model_list}\n\nCurrent: {current_model}"
                            ),
                        })
                    },
                    Err(e) => {
                        warn!(error = %e, "Failed to list models from inference service");
                        Ok(ExecutionResult {
                            success: false,
                            response: format!(
                                "⚠️ Could not retrieve model list: {e}\n\nCurrent: {current_model}"
                            ),
                        })
                    },
                }
            },

            SystemCommand::SwitchModel { model_name } => {
                // Switch to the requested model
                match self.inference.switch_model(model_name).await {
                    Ok(()) => {
                        info!(model = %model_name, "Model switched successfully");
                        Ok(ExecutionResult {
                            success: true,
                            response: format!("✅ Model successfully switched to '{model_name}'."),
                        })
                    },
                    Err(e) => {
                        warn!(model = %model_name, error = %e, "Model switch failed");
                        Ok(ExecutionResult {
                            success: false,
                            response: format!("❌ Model switch failed: {e}"),
                        })
                    },
                }
            },

            SystemCommand::ReloadConfig => {
                // Config reload is handled at the HTTP layer via SIGHUP
                // Here we just acknowledge the request
                Ok(ExecutionResult {
                    success: true,
                    response: "🔄 Configuration is being reloaded. Send SIGHUP to the server or use the API.".to_string(),
                })
            },
        }
    }

    /// Generate help text
    #[allow(clippy::unused_self)]
    fn generate_help(&self, command: Option<&str>) -> String {
        match command {
            Some("briefing" | "morning") => "☀️ **Morning Briefing**\n\n\
                 Shows an overview of appointments, emails, and tasks.\n\n\
                 Examples:\n\
                 • 'briefing'\n\
                 • 'briefing for tomorrow'\n\
                 • 'what's on today?'"
                .to_string(),
            Some("email" | "mail") => "📧 **Email Commands**\n\n\
                 • 'summarize inbox' - Summarize emails\n\
                 • 'write mail to X' - Create email draft\n\
                 • 'important mails' - Show only important emails"
                .to_string(),
            Some("calendar" | "appointment") => "📅 **Calendar Commands**\n\n\
                 • 'appointment on X at Y' - Create new appointment\n\
                 • 'appointments today' - Show today's appointments\n\
                 • 'next appointment' - Show next appointment"
                .to_string(),
            Some("status" | "system") => "🔧 **System Commands**\n\n\
                 • 'status' - Show system status\n\
                 • 'version' - Version information\n\
                 • 'models' - Available AI models"
                .to_string(),
            _ => "🤖 **PiSovereign Help**\n\n\
                 Available commands:\n\n\
                 • 'help [topic]' - This help\n\
                 • 'briefing' - Daily overview\n\
                 • 'inbox' - Email summary\n\
                 • 'appointment ...' - Calendar functions\n\
                 • 'status' - System status\n\
                 • 'echo [text]' - Return text\n\n\
                 You can also just ask questions!"
                .to_string(),
        }
    }

    /// Handle morning briefing command
    async fn handle_morning_briefing(
        &self,
        date: Option<chrono::NaiveDate>,
        user_id: Option<UserId>,
    ) -> Result<ExecutionResult, ApplicationError> {
        use chrono::Local;

        use super::briefing_service::{
            BriefingService, CalendarBrief, EmailBrief, EmailHighlight, TaskBrief,
        };

        let briefing_date = date.unwrap_or_else(|| Local::now().date_naive());
        let date_str = if date.is_none() {
            "today".to_string()
        } else {
            briefing_date.format("%Y-%m-%d").to_string()
        };

        // Get user timezone from profile if available
        let user_timezone = self.get_user_timezone().await;

        // Collect calendar data if service available
        let calendar_brief = if let Some(ref calendar_svc) = self.calendar_service {
            match calendar_svc.get_calendar_brief(briefing_date).await {
                Ok(brief) => brief,
                Err(e) => {
                    warn!(error = %e, "Failed to get calendar brief");
                    CalendarBrief::default()
                },
            }
        } else {
            CalendarBrief::default()
        };

        // Collect email data if service available
        let email_brief = if let Some(ref email_svc) = self.email_service {
            match email_svc.get_inbox_summary(5, false).await {
                Ok(summary) => EmailBrief {
                    unread_count: summary.unread_count,
                    #[allow(clippy::cast_possible_truncation)]
                    important_count: summary.emails.iter().filter(|e| e.is_starred).count() as u32,
                    top_senders: summary
                        .emails
                        .iter()
                        .take(3)
                        .map(|e| e.from.clone())
                        .collect(),
                    highlights: summary
                        .emails
                        .iter()
                        .take(3)
                        .map(|e| EmailHighlight {
                            from: e.from.clone(),
                            subject: e.subject.clone(),
                            preview: e.snippet.clone(),
                        })
                        .collect(),
                },
                Err(e) => {
                    warn!(error = %e, "Failed to get email summary");
                    EmailBrief::default()
                },
            }
        } else {
            EmailBrief::default()
        };

        // Collect task data if service available
        // Use provided user_id from request context, fall back to default
        let task_brief = if let Some(ref task_svc) = self.task_service {
            let effective_user_id = user_id.unwrap_or_default();
            self.fetch_task_brief(task_svc.as_ref(), &effective_user_id)
                .await
        } else {
            TaskBrief::default()
        };

        // Collect weather data if service available
        let weather_summary = if let Some(ref weather_svc) = self.weather_service {
            self.fetch_weather_summary(weather_svc.as_ref()).await
        } else {
            None
        };

        // Generate briefing using BriefingService with user's timezone
        let briefing_service = BriefingService::new(user_timezone);
        let briefing = briefing_service.generate_briefing(
            calendar_brief,
            email_brief,
            task_brief,
            weather_summary,
        );

        // Format briefing response
        let mut response = format!("☀️ Good morning! Here is your briefing for {date_str}:\n\n");

        // Add calendar section
        response.push_str("📅 **Appointments**\n");
        if briefing.calendar.event_count == 0 {
            response.push_str("No appointments scheduled for today.\n");
        } else {
            let _ = writeln!(
                response,
                "{} appointment(s) today:",
                briefing.calendar.event_count
            );
            for event in &briefing.calendar.events {
                if event.all_day {
                    let _ = writeln!(response, "  • {} (all-day)", event.title);
                } else {
                    let _ = writeln!(response, "  • {} at {}", event.title, event.start_time);
                }
            }
            if !briefing.calendar.conflicts.is_empty() {
                let _ = writeln!(
                    response,
                    "  ⚠️ {} conflict(s) detected",
                    briefing.calendar.conflicts.len()
                );
            }
        }

        // Add email section
        response.push_str("\n📧 **Emails**\n");
        if briefing.email.unread_count == 0 {
            response.push_str("No unread emails.\n");
        } else {
            let _ = write!(response, "{} unread email(s)", briefing.email.unread_count);
            if briefing.email.important_count > 0 {
                let _ = write!(response, ", {} important", briefing.email.important_count);
            }
            response.push('\n');
            for highlight in &briefing.email.highlights {
                let _ = writeln!(response, "  • {}: {}", highlight.from, highlight.subject);
            }
        }

        // Add task section if available
        if briefing.tasks.due_today > 0 || briefing.tasks.overdue > 0 {
            response.push_str("\n✅ **Tasks**\n");
            if briefing.tasks.due_today > 0 {
                let _ = writeln!(response, "{} task(s) due today", briefing.tasks.due_today);
            }
            if briefing.tasks.overdue > 0 {
                let _ = writeln!(response, "⚠️ {} overdue task(s)", briefing.tasks.overdue);
            }
        }

        // Add weather section if available
        if let Some(ref weather) = briefing.weather {
            response.push_str("\n🌤️ **Weather**\n");
            let _ = writeln!(
                response,
                "{}, {:.0}°C (High: {:.0}°C, Low: {:.0}°C)",
                weather.condition, weather.temperature, weather.high, weather.low
            );
        }

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle inbox summarization command
    async fn handle_summarize_inbox(
        &self,
        count: Option<u32>,
        only_important: Option<bool>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let email_count = count.unwrap_or(10);
        let important_only = only_important.unwrap_or(false);

        // Use email service if available
        if let Some(ref email_svc) = self.email_service {
            match email_svc.summarize_inbox(email_count, important_only).await {
                Ok(summary) => {
                    return Ok(ExecutionResult {
                        success: true,
                        response: summary,
                    });
                },
                Err(e) => {
                    warn!(error = %e, "Failed to summarize inbox, falling back to placeholder");
                },
            }
        }

        // Fallback when service not available
        let filter_msg = if important_only {
            ", important only"
        } else {
            ""
        };
        Ok(ExecutionResult {
            success: true,
            response: format!(
                "📧 Inbox summary (last {email_count} emails{filter_msg}):\n\n\
                 (Email integration not configured. Please set up Proton Bridge.)"
            ),
        })
    }

    /// Get the user's timezone from their profile, or default to Europe/Berlin
    ///
    /// For now uses a default user ID since we don't have per-request user context.
    async fn get_user_timezone(&self) -> domain::value_objects::Timezone {
        use domain::value_objects::Timezone;

        if let Some(ref profile_store) = self.user_profile_store {
            // Use default user ID for now - future versions will have proper user context
            let default_user_id = UserId::default();
            match profile_store.get(&default_user_id).await {
                Ok(Some(profile)) => profile.timezone().clone(),
                Ok(None) => {
                    debug!("User profile not found, using default timezone");
                    Timezone::berlin()
                },
                Err(e) => {
                    warn!(error = %e, "Failed to get user profile, using default timezone");
                    Timezone::berlin()
                },
            }
        } else {
            // No profile store configured, use default
            domain::value_objects::Timezone::berlin()
        }
    }

    /// Fetch task brief for the briefing
    ///
    /// Retrieves tasks due today, overdue tasks, and high priority tasks,
    /// converting them to the domain TaskBrief structure.
    async fn fetch_task_brief(
        &self,
        task_svc: &dyn TaskPort,
        user_id: &UserId,
    ) -> super::briefing_service::TaskBrief {
        let today = Utc::now().date_naive();

        // Fetch tasks due today
        let today_tasks = match task_svc.get_tasks_due_today(user_id).await {
            Ok(tasks) => tasks,
            Err(e) => {
                warn!(error = %e, "Failed to get tasks due today");
                return super::briefing_service::TaskBrief::default();
            },
        };

        // Fetch high priority tasks
        let high_priority_tasks = match task_svc.get_high_priority_tasks(user_id).await {
            Ok(tasks) => tasks,
            Err(e) => {
                warn!(error = %e, "Failed to get high priority tasks");
                vec![]
            },
        };

        // Convert tasks to TaskItems and separate overdue
        let mut domain_today: Vec<TaskItem> = Vec::new();
        let mut domain_overdue: Vec<TaskItem> = Vec::new();
        let mut domain_high_priority: Vec<TaskItem> = Vec::new();

        for task in &today_tasks {
            let item = Self::task_to_item(task, today);
            if item.is_overdue {
                domain_overdue.push(item);
            } else {
                domain_today.push(item);
            }
        }

        for task in &high_priority_tasks {
            // Avoid duplicates if task is already in today or overdue
            if !today_tasks.iter().any(|t| t.id == task.id) {
                domain_high_priority.push(Self::task_to_item(task, today));
            }
        }

        // Build the TaskBrief using the application-layer type
        super::briefing_service::TaskBrief {
            due_today: u32::try_from(domain_today.len()).unwrap_or(0),
            overdue: u32::try_from(domain_overdue.len()).unwrap_or(0),
            high_priority: domain_high_priority
                .iter()
                .map(|i| i.title.clone())
                .collect(),
        }
    }

    /// Convert a Task port type to a domain TaskItem
    fn task_to_item(task: &Task, today: chrono::NaiveDate) -> TaskItem {
        let is_overdue = task.due_date.is_some_and(|due| due < today);

        let mut item = TaskItem::new(&task.id, &task.summary);

        // Set priority (convert from domain Priority to TaskItem's expected type)
        item = item.with_priority(task.priority);

        // Set due date if present
        if let Some(due) = task.due_date {
            item = item.with_due(due);
        }

        // Mark as overdue if applicable
        if is_overdue {
            item = item.overdue();
        }

        item
    }

    /// Fetches weather summary for the morning briefing.
    ///
    /// Location resolution order:
    /// 1. User profile location (if available)
    /// 2. Default weather location from config (if configured)
    /// 3. None if neither is available
    async fn fetch_weather_summary(&self, weather_svc: &dyn WeatherPort) -> Option<WeatherSummary> {
        // Determine location: user profile > config default
        let location = self.get_weather_location().await;

        let Some(location) = location else {
            warn!("No location available for weather (user profile or config default)");
            return None;
        };

        // Fetch current weather and today's forecast
        match weather_svc.get_weather_summary(&location, 1).await {
            Ok((current, forecast)) => {
                // Get today's forecast for high/low temps
                // Temperature values are well within f32 range (-273.15°C to ~1000°C),
                // so truncation is acceptable and expected
                #[allow(clippy::cast_possible_truncation)]
                let (high, low) = forecast.first().map_or_else(
                    || {
                        (
                            current.temperature as f32,
                            current.apparent_temperature as f32,
                        )
                    },
                    |f| (f.temperature_max as f32, f.temperature_min as f32),
                );

                #[allow(clippy::cast_possible_truncation)]
                let temperature = current.temperature as f32;

                Some(WeatherSummary {
                    temperature,
                    condition: current.condition.to_string(),
                    high,
                    low,
                })
            },
            Err(e) => {
                warn!(error = %e, "Failed to fetch weather data");
                None
            },
        }
    }

    /// Gets the location for weather, preferring user profile over config default.
    async fn get_weather_location(&self) -> Option<GeoLocation> {
        // First try to get location from user profile
        if let Some(ref profile_store) = self.user_profile_store {
            // Use default user ID for now
            let user_id = UserId::default();
            if let Ok(Some(profile)) = profile_store.get(&user_id).await {
                if let Some(location) = profile.location() {
                    debug!("Using location from user profile");
                    return Some(location);
                }
            }
        }

        // Fall back to config default
        if let Some(ref default_location) = self.default_weather_location {
            debug!("Using default weather location from config");
            return Some(*default_location);
        }

        None
    }

    /// Handle draft email command - create and store the draft
    ///
    /// For now uses a default user ID. Future versions will map API keys to users.
    async fn handle_draft_email(
        &self,
        to: &str,
        subject: Option<&str>,
        body: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        // Use default user ID for now (will be replaced with API key mapping)
        let user_id = UserId::default();

        // Generate subject if not provided
        let subject = subject.map_or_else(
            || format!("Re: {}", to.split('@').next().unwrap_or("Contact")),
            String::from,
        );

        // Check if draft store is configured
        let Some(ref draft_store) = self.draft_store else {
            return Ok(ExecutionResult {
                success: false,
                response: "📧 Email draft creation failed:\n\n\
                          Draft storage is not configured. Please set up database persistence."
                    .to_string(),
            });
        };

        // Create and save the draft
        let draft =
            PersistedEmailDraft::new(user_id, to.to_string(), subject.clone(), body.to_string());
        let draft_id = draft.id;

        draft_store.save(&draft).await?;

        info!(draft_id = %draft_id, to = %to, subject = %subject, "Created email draft");

        Ok(ExecutionResult {
            success: true,
            response: format!(
                "📝 Email draft created:\n\n\
                 **To:** {to}\n\
                 **Subject:** {subject}\n\n\
                 Draft ID: `{draft_id}`\n\n\
                 To send this email, say 'send email {draft_id}' or 'approve send'."
            ),
        })
    }

    /// Handle web search command
    ///
    /// Performs a web search and returns results formatted with citations.
    /// Uses the LLM to summarize the search results.
    async fn handle_web_search(
        &self,
        query: &str,
        max_results: Option<u32>,
    ) -> Result<ExecutionResult, ApplicationError> {
        // Check if websearch service is configured
        let Some(ref websearch_service) = self.websearch_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "🔍 Web search is not available.\n\n\
                          Web search service is not configured. \
                          Please set up the Brave Search API key in your configuration."
                    .to_string(),
            });
        };

        let max_results = max_results.unwrap_or(5);

        info!(query = %query, max_results = %max_results, "Performing web search");

        // Perform the search
        let search_response = websearch_service.search_for_llm(query, max_results).await?;

        // If no results, return early
        if search_response.contains("No web search results found") {
            return Ok(ExecutionResult {
                success: true,
                response: format!(
                    "🔍 No results found for: **{query}**\n\n\
                     Try rephrasing your search query or using different keywords."
                ),
            });
        }

        // Use LLM to summarize the search results with proper citation
        let summary_prompt = format!(
            "Based on the following web search results, provide a concise and helpful answer \
             to the query: \"{query}\"\n\n\
             Include relevant information from the sources and cite them using [number] notation \
             at the end of sentences that use information from that source.\n\n\
             Search Results:\n{search_response}\n\n\
             Provide a clear, informative summary with proper source citations."
        );

        let llm_response = self.inference.generate(&summary_prompt).await?;

        Ok(ExecutionResult {
            success: true,
            response: format!(
                "🔍 **Web Search Results for:** {query}\n\n{}\n\n\
                 ---\n*Search powered by {}*",
                llm_response.content,
                websearch_service.provider_name()
            ),
        })
    }

    // =========================================================================
    // Reminder Handlers
    // =========================================================================

    /// Handle creating a custom reminder
    async fn handle_create_reminder(
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
                // Try parsing with common formats
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
    async fn handle_list_reminders(
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
    async fn handle_snooze_reminder(
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
    async fn handle_acknowledge_reminder(
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
    async fn handle_delete_reminder(
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

    // =========================================================================
    // Transit Handler
    // =========================================================================

    /// Handle searching for transit connections
    async fn handle_search_transit(
        &self,
        from: &str,
        to: &str,
        departure: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref transit_service) = self.transit_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🚆 Transit service not yet configured. Cannot search: {from} → {to}"
                ),
            });
        };

        // Determine the origin - use home location if "from" is empty
        let from_location = if from.is_empty() {
            match &self.home_location {
                Some(loc) => *loc,
                None => {
                    return Ok(ExecutionResult {
                        success: false,
                        response:
                            "🚆 Keine Startadresse angegeben und keine Heimadresse konfiguriert.\n\
                                   Bitte geben Sie einen Startpunkt an."
                                .to_string(),
                    });
                },
            }
        } else {
            // Geocode the address
            match transit_service.geocode_address(from).await {
                Ok(Some(loc)) => loc,
                Ok(None) => {
                    return Ok(ExecutionResult {
                        success: false,
                        response: format!(
                            "📍 Startadresse konnte nicht gefunden werden: **{from}**\n\n\
                             Bitte versuchen Sie eine genauere Adresse."
                        ),
                    });
                },
                Err(e) => {
                    warn!(error = %e, address = %from, "Failed to geocode from address");
                    return Ok(ExecutionResult {
                        success: false,
                        response: format!("❌ Fehler bei der Geolokalisierung: {e}"),
                    });
                },
            }
        };

        // Parse departure time if provided
        #[allow(clippy::option_if_let_else)] // Complex parsing chain doesn't simplify well
        let departure_time = if let Some(dep) = departure {
            chrono::DateTime::parse_from_rfc3339(dep)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(dep, "%Y-%m-%dT%H:%M:%S")
                        .or_else(|_| chrono::NaiveDateTime::parse_from_str(dep, "%Y-%m-%dT%H:%M"))
                        .map(|ndt| ndt.and_utc())
                })
                .ok()
        } else {
            None
        };

        info!(from = %from, to = %to, departure = ?departure_time, "Searching transit connections");

        // Search for connections (default to 5 results)
        match transit_service
            .find_connections_to_address(&from_location, to, departure_time, 5)
            .await
        {
            Ok(connections) => {
                if connections.is_empty() {
                    return Ok(ExecutionResult {
                        success: true,
                        response: format!(
                            "🚆 Keine Verbindungen gefunden.\n\n\
                             **Von:** {}\n\
                             **Nach:** {to}\n\n\
                             Versuchen Sie einen anderen Zeitpunkt oder prüfen Sie die Adressen.",
                            if from.is_empty() { "Heimadresse" } else { from }
                        ),
                    });
                }

                let response = format!(
                    "🚆 **ÖPNV-Verbindungen nach {to}**\n\n\
                     **Von:** {}\n\n\
                     {}",
                    if from.is_empty() { "Heimadresse" } else { from },
                    format_connections(&connections)
                );

                Ok(ExecutionResult {
                    success: true,
                    response,
                })
            },
            Err(e) => {
                warn!(error = %e, "Transit search failed");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❌ Fehler bei der Verbindungssuche: {e}\n\n\
                         Bitte versuchen Sie es später erneut."
                    ),
                })
            },
        }
    }

    /// Handle listing tasks
    ///
    /// Lists tasks from the configured task service, optionally filtered by status, priority, and list.
    async fn handle_list_tasks(
        &self,
        status: Option<&domain::TaskStatus>,
        priority: Option<&domain::Priority>,
        list: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        // Check if task service is configured
        let Some(ref task_service) = self.task_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📋 Tasks are not available.\n\n\
                          Task service is not configured. \
                          Please set up CalDAV with task support in your configuration."
                    .to_string(),
            });
        };

        // Build query from filters
        let query = crate::ports::TaskQuery {
            status: status.copied(),
            priority: priority.copied(),
            include_completed: status.is_some_and(domain::TaskStatus::is_done),
            list: list.map(ToString::to_string),
            ..Default::default()
        };

        // Get tasks
        let user_id = UserId::default_user();
        let tasks = task_service.list_tasks(&user_id, &query).await?;

        // Format task list
        let mut response = String::from("📋 **Tasks**\n\n");

        if tasks.is_empty() {
            let filter_desc = match (status, priority) {
                (Some(s), Some(p)) => format!(" matching status '{s}' and priority '{p}'"),
                (Some(s), None) => format!(" with status '{s}'"),
                (None, Some(p)) => format!(" with priority '{p}'"),
                (None, None) => String::new(),
            };
            response.push_str(&format!("No tasks found{filter_desc}.\n"));
        } else {
            let today = chrono::Local::now().date_naive();
            for task in &tasks {
                let status_emoji = match task.status {
                    domain::TaskStatus::Completed => "✅",
                    domain::TaskStatus::InProgress => "🔄",
                    domain::TaskStatus::Cancelled => "❌",
                    domain::TaskStatus::NeedsAction => "⬜",
                };
                let priority_emoji = match task.priority {
                    domain::Priority::High => "🔴",
                    domain::Priority::Medium => "🟡",
                    domain::Priority::Low => "🟢",
                };

                let is_overdue = task.due_date.is_some_and(|d| d < today);
                let overdue_marker = if is_overdue { " ⚠️ OVERDUE" } else { "" };
                let due_str = task
                    .due_date
                    .map_or(String::new(), |d| format!(" (due: {d})"));

                response.push_str(&format!(
                    "{} {} **{}**{}{}\n  ID: `{}`\n",
                    status_emoji, priority_emoji, task.summary, due_str, overdue_marker, task.id
                ));
            }
        }

        // Count summary
        let active_count = tasks.iter().filter(|t| t.status.is_active()).count();
        let completed_count = tasks.iter().filter(|t| t.status.is_done()).count();
        response.push_str(&format!(
            "\n---\n*{active_count} active task(s), {completed_count} completed*"
        ));

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle listing task lists
    ///
    /// Lists all available task lists/calendars from the configured task service.
    async fn handle_list_task_lists(&self) -> Result<ExecutionResult, ApplicationError> {
        // Check if task service is configured
        let Some(ref task_service) = self.task_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📋 Task lists are not available.\n\n\
                          Task service is not configured. \
                          Please set up CalDAV with task support in your configuration."
                    .to_string(),
            });
        };

        let user_id = UserId::default_user();
        let lists = task_service.list_task_lists(&user_id).await?;

        let mut response = String::from("📋 **Task Lists**\n\n");

        if lists.is_empty() {
            response.push_str("No task lists found.\n");
        } else {
            for list in &lists {
                response.push_str(&format!("• **{}**\n  ID: `{}`\n", list.name, list.id));
            }
        }

        response.push_str(&format!("\n---\n*{} list(s) available*", lists.len()));

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }
}

/// Result of command execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub response: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_result_creation() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: None,
        };
        assert!(result.success);
        assert_eq!(result.execution_time_ms, 100);
    }

    #[test]
    fn command_result_with_approval_status() {
        let result = CommandResult {
            command: AgentCommand::Help { command: None },
            success: true,
            response: "Help text".to_string(),
            execution_time_ms: 50,
            approval_status: Some(ApprovalStatus::NotRequired),
        };
        assert_eq!(result.approval_status, Some(ApprovalStatus::NotRequired));
    }

    #[test]
    fn approval_status_pending() {
        let status = ApprovalStatus::Pending;
        assert_eq!(status, ApprovalStatus::Pending);
        assert_ne!(status, ApprovalStatus::Granted);
    }

    #[test]
    fn approval_status_granted() {
        let status = ApprovalStatus::Granted;
        assert_eq!(status, ApprovalStatus::Granted);
    }

    #[test]
    fn approval_status_denied() {
        let status = ApprovalStatus::Denied;
        assert_eq!(status, ApprovalStatus::Denied);
    }

    #[test]
    fn approval_status_not_required() {
        let status = ApprovalStatus::NotRequired;
        assert_eq!(status, ApprovalStatus::NotRequired);
    }

    #[test]
    fn approval_status_clone() {
        let status = ApprovalStatus::Pending;
        #[allow(clippy::redundant_clone)]
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn command_result_clone() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: Some(ApprovalStatus::NotRequired),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = result.clone();
        assert_eq!(result.success, cloned.success);
        assert_eq!(result.response, cloned.response);
    }

    #[test]
    fn command_result_has_debug() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: None,
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("CommandResult"));
        assert!(debug.contains("success"));
    }

    #[test]
    fn approval_status_has_debug() {
        let status = ApprovalStatus::Pending;
        let debug = format!("{status:?}");
        assert!(debug.contains("Pending"));
    }

    #[test]
    fn execution_result_creation() {
        let result = ExecutionResult {
            success: true,
            response: "Done".to_string(),
        };
        assert!(result.success);
        assert_eq!(result.response, "Done");
    }

    #[test]
    fn execution_result_failure() {
        let result = ExecutionResult {
            success: false,
            response: "Failed".to_string(),
        };
        assert!(!result.success);
    }

    #[test]
    fn execution_result_has_debug() {
        let result = ExecutionResult {
            success: true,
            response: "OK".to_string(),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("ExecutionResult"));
    }
}

#[cfg(test)]
mod async_tests {
    use mockall::mock;

    use super::*;
    use crate::ports::InferenceResult;

    mock! {
        pub InferenceEngine {}

        #[async_trait::async_trait]
        impl InferencePort for InferenceEngine {
            async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_context(&self, conversation: &domain::Conversation) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_system(&self, system_prompt: &str, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_stream(&self, message: &str) -> Result<crate::ports::InferenceStream, ApplicationError>;
            async fn generate_stream_with_system(&self, system_prompt: &str, message: &str) -> Result<crate::ports::InferenceStream, ApplicationError>;
            async fn is_healthy(&self) -> bool;
            fn current_model(&self) -> String;
            async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError>;
            async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError>;
        }
    }

    fn mock_inference_result(content: &str) -> InferenceResult {
        InferenceResult {
            content: content.to_string(),
            model: "test-model".to_string(),
            tokens_used: Some(42),
            latency_ms: 100,
        }
    }

    #[tokio::test]
    async fn agent_service_new() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("AgentService"));
    }

    #[tokio::test]
    async fn execute_echo_command() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Echo {
                message: "Hello".to_string(),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Hello"));
    }

    #[tokio::test]
    async fn execute_help_general() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help { command: None })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("PiSovereign"));
        assert!(result.response.contains("Help"));
    }

    #[tokio::test]
    async fn execute_help_briefing() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("briefing".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Briefing"));
    }

    #[tokio::test]
    async fn execute_help_email() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("email".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Email"));
    }

    #[tokio::test]
    async fn execute_help_calendar() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("calendar".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Calendar"));
    }

    #[tokio::test]
    async fn execute_help_status() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("status".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("System"));
    }

    #[tokio::test]
    async fn execute_ask_command() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate()
            .returning(|_| Ok(mock_inference_result("AI Response")));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Ask {
                question: "What is the weather?".to_string(),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.response, "AI Response");
    }

    #[tokio::test]
    async fn execute_morning_briefing() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::MorningBriefing { date: None })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Good morning"));
    }

    #[tokio::test]
    async fn execute_morning_briefing_with_date() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let date = chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let result = service
            .execute_command(&AgentCommand::MorningBriefing { date: Some(date) })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("2025-01-15"));
    }

    #[tokio::test]
    async fn execute_summarize_inbox() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: Some(5),
                only_important: Some(true),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Inbox"));
        assert!(result.response.contains('5'));
    }

    #[tokio::test]
    async fn execute_summarize_inbox_defaults() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: None,
                only_important: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("10"));
    }

    #[tokio::test]
    async fn execute_unknown_command() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Unknown {
                original_input: "gibberish".to_string(),
            })
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("gibberish"));
    }

    #[tokio::test]
    async fn execute_system_status() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| true);
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Status))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Online"));
        assert!(result.response.contains("qwen2.5-1.5b"));
    }

    #[tokio::test]
    async fn execute_system_status_unhealthy() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| false);
        mock.expect_current_model()
            .returning(|| "test-model".to_string());

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Status))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Offline"));
    }

    #[tokio::test]
    async fn execute_system_version() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Version))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("PiSovereign"));
    }

    #[tokio::test]
    async fn execute_system_list_models_success() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());
        mock.expect_list_available_models().returning(|| {
            Ok(vec![
                "qwen2.5-1.5b".to_string(),
                "llama3.2-1b".to_string(),
                "mistral-7b".to_string(),
            ])
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Available Models"));
        assert!(result.response.contains("qwen2.5-1.5b (active)"));
        assert!(result.response.contains("• llama3.2-1b"));
        assert!(result.response.contains("• mistral-7b"));
        assert!(!result.response.contains("llama3.2-1b (active)"));
    }

    #[tokio::test]
    async fn execute_system_list_models_error_fallback() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());
        mock.expect_list_available_models().returning(|| {
            Err(ApplicationError::ExternalService(
                "Circuit breaker open".to_string(),
            ))
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("Could not retrieve model list"));
        assert!(result.response.contains("Current: qwen2.5-1.5b"));
    }

    #[tokio::test]
    async fn execute_system_list_models_empty() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "unknown".to_string());
        mock.expect_list_available_models().returning(|| Ok(vec![]));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Available Models"));
        assert!(result.response.contains("Current: unknown"));
    }

    #[tokio::test]
    async fn execute_system_switch_model() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_switch_model()
            .with(mockall::predicate::eq("llama"))
            .returning(|_| Ok(()));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "llama".to_string(),
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("llama"));
    }

    #[tokio::test]
    async fn execute_system_switch_model_error() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_switch_model().returning(|_| {
            Err(ApplicationError::Configuration(
                "Model not found".to_string(),
            ))
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "invalid".to_string(),
            }))
            .await
            .unwrap();

        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_system_reload_config() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ReloadConfig))
            .await
            .unwrap();

        // Config reload succeeds (placeholder implementation)
        assert!(result.success);
        assert!(result.response.contains("Configuration"));
    }

    #[tokio::test]
    async fn execute_calendar_event_requires_approval() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::CreateCalendarEvent {
                title: "Meeting".to_string(),
                date: chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
                time: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
                duration_minutes: None,
                attendees: None,
                location: None,
            })
            .await;

        assert!(result.is_err());
        if let Err(ApplicationError::ApprovalRequired(desc)) = result {
            assert!(desc.contains("Meeting"));
        }
    }

    #[tokio::test]
    async fn execute_send_email_requires_approval() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SendEmail {
                draft_id: "draft-123".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_draft_email_without_store_fallback() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::DraftEmail {
                to: domain::EmailAddress::new("test@example.com").unwrap(),
                subject: Some("Test".to_string()),
                body: "Body content".to_string(),
            })
            .await;

        // DraftEmail is now handled but returns unsuccessful when no store is configured
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.success);
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn generate_help_for_morning() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("morning".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Briefing"));
    }

    #[tokio::test]
    async fn generate_help_for_mail() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("mail".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Email"));
    }

    #[tokio::test]
    async fn generate_help_for_appointment() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("appointment".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Calendar"));
    }

    #[tokio::test]
    async fn generate_help_for_system() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("system".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("System"));
    }

    #[tokio::test]
    async fn agent_service_debug_output() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("AgentService"));
        assert!(debug.contains("parser"));
    }

    #[tokio::test]
    async fn draft_email_without_store_returns_error() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .handle_draft_email("test@example.com", Some("Test Subject"), "Test body")
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn draft_email_with_store_creates_draft() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_store = MockDraftStorePort::new();

        // Expect save to be called and return the draft ID
        mock_store.expect_save().returning(|draft| Ok(draft.id));

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let result = service
            .handle_draft_email(
                "recipient@example.com",
                Some("Test Subject"),
                "Email body content",
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Draft ID:"));
        assert!(result.response.contains("recipient@example.com"));
        assert!(result.response.contains("Test Subject"));
    }

    #[tokio::test]
    async fn draft_email_generates_subject_when_not_provided() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_store = MockDraftStorePort::new();

        mock_store.expect_save().returning(|draft| Ok(draft.id));

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let result = service
            .handle_draft_email("john@example.com", None, "Hello!")
            .await
            .unwrap();

        assert!(result.success);
        // Should generate subject like "Re: john"
        assert!(result.response.contains("Re: john"));
    }

    #[tokio::test]
    async fn agent_service_has_draft_store_in_debug() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mock_store = MockDraftStorePort::new();

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_draft_store"));
    }

    #[tokio::test]
    async fn agent_service_with_task_service() {
        use crate::ports::MockTaskPort;

        let mock_inference = MockInferenceEngine::new();
        let mock_task = MockTaskPort::new();

        let service =
            AgentService::new(Arc::new(mock_inference)).with_task_service(Arc::new(mock_task));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_task: true"));
    }

    #[tokio::test]
    async fn fetch_task_brief_returns_default_on_error() {
        use crate::ports::MockTaskPort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_task = MockTaskPort::new();

        mock_task.expect_get_tasks_due_today().returning(|_| {
            Err(ApplicationError::ExternalService(
                "Task service down".into(),
            ))
        });

        let service =
            AgentService::new(Arc::new(mock_inference)).with_task_service(Arc::new(mock_task));

        let user_id = UserId::default();
        let brief = service
            .fetch_task_brief(service.task_service.as_ref().unwrap().as_ref(), &user_id)
            .await;

        assert_eq!(brief.due_today, 0);
        assert_eq!(brief.overdue, 0);
        assert!(brief.high_priority.is_empty());
    }

    #[tokio::test]
    async fn fetch_task_brief_with_tasks() {
        use chrono::Utc;

        use crate::ports::{MockTaskPort, Task, TaskStatus};

        let mock_inference = MockInferenceEngine::new();
        let mut mock_task = MockTaskPort::new();

        let today = Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);

        // Task due today
        let task_today = Task {
            id: "task-1".into(),
            summary: "Fix bug".into(),
            description: None,
            priority: domain::Priority::High,
            status: TaskStatus::NeedsAction,
            due_date: Some(today),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            calendar: None,
        };

        // Overdue task
        let task_overdue = Task {
            id: "task-2".into(),
            summary: "Review PR".into(),
            description: None,
            priority: domain::Priority::Medium,
            status: TaskStatus::NeedsAction,
            due_date: Some(yesterday),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            calendar: None,
        };

        mock_task
            .expect_get_tasks_due_today()
            .returning(move |_| Ok(vec![task_today.clone(), task_overdue.clone()]));

        mock_task
            .expect_get_high_priority_tasks()
            .returning(|_| Ok(vec![]));

        let service =
            AgentService::new(Arc::new(mock_inference)).with_task_service(Arc::new(mock_task));

        let user_id = UserId::default();
        let brief = service
            .fetch_task_brief(service.task_service.as_ref().unwrap().as_ref(), &user_id)
            .await;

        // One due today, one overdue
        assert_eq!(brief.due_today, 1);
        assert_eq!(brief.overdue, 1);
    }

    #[tokio::test]
    async fn task_to_item_converts_correctly() {
        use chrono::Utc;

        use crate::ports::{Task, TaskStatus};

        let today = Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);

        let task = Task {
            id: "task-123".into(),
            summary: "Important task".into(),
            description: Some("Description".into()),
            priority: domain::Priority::High,
            status: TaskStatus::NeedsAction,
            due_date: Some(yesterday),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            calendar: None,
        };

        let item = AgentService::task_to_item(&task, today);

        assert_eq!(item.id, "task-123");
        assert_eq!(item.title, "Important task");
        assert_eq!(item.priority, domain::Priority::High);
        assert_eq!(item.due, Some(yesterday));
        assert!(item.is_overdue);
    }

    #[tokio::test]
    async fn task_to_item_not_overdue_when_due_today() {
        use chrono::Utc;

        use crate::ports::{Task, TaskStatus};

        let today = Utc::now().date_naive();

        let task = Task {
            id: "task-456".into(),
            summary: "Due today".into(),
            description: None,
            priority: domain::Priority::Medium,
            status: TaskStatus::NeedsAction,
            due_date: Some(today),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            calendar: None,
        };

        let item = AgentService::task_to_item(&task, today);

        assert!(!item.is_overdue);
    }

    #[tokio::test]
    async fn fetch_weather_summary_returns_weather_data() {
        use chrono::{NaiveDate, Utc};

        use crate::ports::{CurrentWeather, DailyForecast, MockWeatherPort, WeatherCondition};

        let mock_inference = MockInferenceEngine::new();
        let mut mock_weather = MockWeatherPort::new();

        let current = CurrentWeather {
            temperature: 20.5,
            apparent_temperature: 19.0,
            humidity: 65,
            wind_speed: 10.0,
            condition: WeatherCondition::PartlyCloudy,
            observed_at: Utc::now(),
        };

        let forecast = vec![DailyForecast {
            date: NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
            temperature_max: 25.0,
            temperature_min: 15.0,
            condition: WeatherCondition::PartlyCloudy,
            precipitation_probability: 20,
            precipitation_sum: 0.0,
            sunrise: None,
            sunset: None,
        }];

        mock_weather
            .expect_get_weather_summary()
            .returning(move |_, _| Ok((current.clone(), forecast.clone())));

        let service = AgentService::new(Arc::new(mock_inference))
            .with_weather_service(Arc::new(mock_weather))
            .with_default_weather_location(GeoLocation::berlin());

        let summary = service
            .fetch_weather_summary(service.weather_service.as_ref().unwrap().as_ref())
            .await;

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!((summary.temperature - 20.5).abs() < 0.01);
        assert!((summary.high - 25.0).abs() < 0.01);
        assert!((summary.low - 15.0).abs() < 0.01);
        assert_eq!(summary.condition, "Partly cloudy");
    }

    #[tokio::test]
    async fn fetch_weather_summary_returns_none_on_error() {
        use crate::ports::MockWeatherPort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_weather = MockWeatherPort::new();

        mock_weather
            .expect_get_weather_summary()
            .returning(|_, _| Err(ApplicationError::ExternalService("Weather API".into())));

        let service = AgentService::new(Arc::new(mock_inference))
            .with_weather_service(Arc::new(mock_weather))
            .with_default_weather_location(GeoLocation::berlin());

        let summary = service
            .fetch_weather_summary(service.weather_service.as_ref().unwrap().as_ref())
            .await;

        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn fetch_weather_summary_returns_none_without_location() {
        use crate::ports::MockWeatherPort;

        let mock_inference = MockInferenceEngine::new();
        let mock_weather = MockWeatherPort::new();

        // No default location, no user profile store
        let service = AgentService::new(Arc::new(mock_inference))
            .with_weather_service(Arc::new(mock_weather));

        let summary = service
            .fetch_weather_summary(service.weather_service.as_ref().unwrap().as_ref())
            .await;

        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn get_weather_location_prefers_user_profile() {
        use std::sync::Arc;

        // Create a simple mock for UserProfileStore that returns a profile with location
        struct TestProfileStore;

        #[async_trait::async_trait]
        impl UserProfileStore for TestProfileStore {
            async fn save(
                &self,
                _profile: &domain::entities::UserProfile,
            ) -> Result<(), ApplicationError> {
                Ok(())
            }

            async fn get(
                &self,
                _user_id: &UserId,
            ) -> Result<Option<domain::entities::UserProfile>, ApplicationError> {
                // Return a profile with Berlin location
                Ok(Some(domain::entities::UserProfile::with_defaults(
                    UserId::default(),
                    GeoLocation::berlin(),
                    domain::value_objects::Timezone::berlin(),
                )))
            }

            async fn delete(&self, _user_id: &UserId) -> Result<bool, ApplicationError> {
                Ok(true)
            }

            async fn update_location(
                &self,
                _user_id: &UserId,
                _location: Option<&GeoLocation>,
            ) -> Result<bool, ApplicationError> {
                Ok(true)
            }

            async fn update_timezone(
                &self,
                _user_id: &UserId,
                _timezone: &domain::value_objects::Timezone,
            ) -> Result<bool, ApplicationError> {
                Ok(true)
            }
        }

        let mock_inference = MockInferenceEngine::new();

        // Service with both user profile and default location
        let service = AgentService::new(Arc::new(mock_inference))
            .with_user_profile_store(Arc::new(TestProfileStore))
            .with_default_weather_location(GeoLocation::london()); // London as fallback

        let location = service.get_weather_location().await;

        // Should get Berlin (user profile) not London (default)
        assert!(location.is_some());
        let loc = location.unwrap();
        assert!((loc.latitude() - 52.52).abs() < 0.01);
        assert!((loc.longitude() - 13.405).abs() < 0.01);
    }

    #[tokio::test]
    async fn get_weather_location_falls_back_to_default() {
        use std::sync::Arc;

        // Profile store that returns no location
        struct NoLocationProfileStore;

        #[async_trait::async_trait]
        impl UserProfileStore for NoLocationProfileStore {
            async fn save(
                &self,
                _profile: &domain::entities::UserProfile,
            ) -> Result<(), ApplicationError> {
                Ok(())
            }

            async fn get(
                &self,
                _user_id: &UserId,
            ) -> Result<Option<domain::entities::UserProfile>, ApplicationError> {
                // Return a profile without location
                Ok(Some(domain::entities::UserProfile::new(UserId::default())))
            }

            async fn delete(&self, _user_id: &UserId) -> Result<bool, ApplicationError> {
                Ok(true)
            }

            async fn update_location(
                &self,
                _user_id: &UserId,
                _location: Option<&GeoLocation>,
            ) -> Result<bool, ApplicationError> {
                Ok(true)
            }

            async fn update_timezone(
                &self,
                _user_id: &UserId,
                _timezone: &domain::value_objects::Timezone,
            ) -> Result<bool, ApplicationError> {
                Ok(true)
            }
        }

        let mock_inference = MockInferenceEngine::new();

        // Service with user profile without location, but with default
        let service = AgentService::new(Arc::new(mock_inference))
            .with_user_profile_store(Arc::new(NoLocationProfileStore))
            .with_default_weather_location(GeoLocation::london()); // London as default

        let location = service.get_weather_location().await;

        // Should get London (default) since user profile has no location
        assert!(location.is_some());
        let loc = location.unwrap();
        assert!((loc.latitude() - 51.5074).abs() < 0.01);
        assert!((loc.longitude() - (-0.1278)).abs() < 0.01);
    }

    // Web search tests

    #[tokio::test]
    async fn execute_websearch_without_service_returns_error_message() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "rust programming".to_string(),
                max_results: None,
            })
            .await
            .unwrap();

        // Without websearch service configured, should return user-friendly error
        assert!(!result.success);
        assert!(result.response.contains("not available"));
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn execute_websearch_with_mock_service() {
        use crate::ports::MockWebSearchPort;

        let mut mock_inference = MockInferenceEngine::new();
        mock_inference.expect_generate().returning(|_| {
            Ok(mock_inference_result(
                "Here is a summary with citations [1][2].",
            ))
        });

        let mut mock_websearch = MockWebSearchPort::new();
        mock_websearch
            .expect_search_for_llm()
            .returning(|query, _| {
                Ok(format!(
                    "[1] Result 1 - example.com: Info about {query}\n\
                     [2] Result 2 - test.org: More info about {query}"
                ))
            });
        mock_websearch
            .expect_provider_name()
            .return_const("mock-provider".to_string());

        let service = AgentService::new(Arc::new(mock_inference))
            .with_websearch_service(Arc::new(mock_websearch));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "rust async patterns".to_string(),
                max_results: Some(5),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("rust async patterns"));
        assert!(result.response.contains("mock-provider"));
    }

    #[tokio::test]
    async fn execute_websearch_no_results() {
        use crate::ports::MockWebSearchPort;

        let mock_inference = MockInferenceEngine::new();

        let mut mock_websearch = MockWebSearchPort::new();
        mock_websearch
            .expect_search_for_llm()
            .returning(|query, _| Ok(format!("No web search results found for: {query}")));
        mock_websearch
            .expect_provider_name()
            .return_const("mock".to_string());

        let service = AgentService::new(Arc::new(mock_inference))
            .with_websearch_service(Arc::new(mock_websearch));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "xyznonexistent12345".to_string(),
                max_results: None,
            })
            .await
            .unwrap();

        assert!(result.success); // No results is still a successful execution
        assert!(result.response.contains("No results found"));
    }

    #[tokio::test]
    async fn websearch_service_builder() {
        use crate::ports::MockWebSearchPort;

        let mock = MockInferenceEngine::new();
        let mock_websearch = MockWebSearchPort::new();

        let service =
            AgentService::new(Arc::new(mock)).with_websearch_service(Arc::new(mock_websearch));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_websearch: true"));
    }
}
