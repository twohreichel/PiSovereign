//! Morning briefing handler with calendar, email, task, and weather integration

use std::fmt::Write as _;

use chrono::Utc;
use domain::{GeoLocation, TaskItem, UserId};
use tracing::{debug, warn};

use super::{AgentService, ExecutionResult};
use crate::{
    error::ApplicationError,
    ports::{Task, TaskPort, WeatherPort},
    services::briefing_service::WeatherSummary,
};

impl AgentService {
    /// Handle morning briefing command
    pub(super) async fn handle_morning_briefing(
        &self,
        date: Option<chrono::NaiveDate>,
        user_id: Option<UserId>,
    ) -> Result<ExecutionResult, ApplicationError> {
        use chrono::Local;

        use crate::services::briefing_service::{
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

    /// Get the user's timezone from their profile, or default to Europe/Berlin
    ///
    /// For now uses a default user ID since we don't have per-request user context.
    pub(super) async fn get_user_timezone(&self) -> domain::value_objects::Timezone {
        use domain::value_objects::Timezone;

        if let Some(ref profile_store) = self.user_profile_store {
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
            domain::value_objects::Timezone::berlin()
        }
    }

    /// Fetch task brief for the briefing
    ///
    /// Retrieves tasks due today, overdue tasks, and high priority tasks,
    /// converting them to the domain TaskBrief structure.
    pub(super) async fn fetch_task_brief(
        &self,
        task_svc: &dyn TaskPort,
        user_id: &UserId,
    ) -> crate::services::briefing_service::TaskBrief {
        let today = Utc::now().date_naive();

        // Fetch tasks due today
        let today_tasks = match task_svc.get_tasks_due_today(user_id).await {
            Ok(tasks) => tasks,
            Err(e) => {
                warn!(error = %e, "Failed to get tasks due today");
                return crate::services::briefing_service::TaskBrief::default();
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
            if !today_tasks.iter().any(|t| t.id == task.id) {
                domain_high_priority.push(Self::task_to_item(task, today));
            }
        }

        crate::services::briefing_service::TaskBrief {
            due_today: u32::try_from(domain_today.len()).unwrap_or(0),
            overdue: u32::try_from(domain_overdue.len()).unwrap_or(0),
            high_priority: domain_high_priority
                .iter()
                .map(|i| i.title.clone())
                .collect(),
        }
    }

    /// Convert a Task port type to a domain TaskItem
    pub(super) fn task_to_item(task: &Task, today: chrono::NaiveDate) -> TaskItem {
        let is_overdue = task.due_date.is_some_and(|due| due < today);

        let mut item = TaskItem::new(&task.id, &task.summary);
        item = item.with_priority(task.priority);

        if let Some(due) = task.due_date {
            item = item.with_due(due);
        }

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
    pub(super) async fn fetch_weather_summary(
        &self,
        weather_svc: &dyn WeatherPort,
    ) -> Option<WeatherSummary> {
        let location = self.get_weather_location().await;

        let Some(location) = location else {
            warn!("No location available for weather (user profile or config default)");
            return None;
        };

        match weather_svc.get_weather_summary(&location, 1).await {
            Ok((current, forecast)) => {
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
    pub(super) async fn get_weather_location(&self) -> Option<GeoLocation> {
        if let Some(ref profile_store) = self.user_profile_store {
            let user_id = UserId::default();
            if let Ok(Some(profile)) = profile_store.get(&user_id).await {
                if let Some(location) = profile.location() {
                    debug!("Using location from user profile");
                    return Some(location);
                }
            }
        }

        if let Some(ref default_location) = self.default_weather_location {
            debug!("Using default weather location from config");
            return Some(*default_location);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{NaiveDate, Utc};
    use domain::{AgentCommand, GeoLocation, UserId};

    use super::super::{AgentService, test_support::MockInferenceEngine};
    use crate::{
        error::ApplicationError,
        ports::{
            CurrentWeather, DailyForecast, MockWeatherPort, Task, TaskStatus, UserProfileStore,
            WeatherCondition,
        },
    };

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
        use crate::ports::MockTaskPort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_task = MockTaskPort::new();

        let today = Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);

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

        assert_eq!(brief.due_today, 1);
        assert_eq!(brief.overdue, 1);
    }

    #[tokio::test]
    async fn task_to_item_converts_correctly() {
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
        let mock_inference = MockInferenceEngine::new();
        let mock_weather = MockWeatherPort::new();

        let service = AgentService::new(Arc::new(mock_inference))
            .with_weather_service(Arc::new(mock_weather));

        let summary = service
            .fetch_weather_summary(service.weather_service.as_ref().unwrap().as_ref())
            .await;

        assert!(summary.is_none());
    }

    #[tokio::test]
    async fn get_weather_location_prefers_user_profile() {
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

        let service = AgentService::new(Arc::new(mock_inference))
            .with_user_profile_store(Arc::new(TestProfileStore))
            .with_default_weather_location(GeoLocation::london());

        let location = service.get_weather_location().await;

        assert!(location.is_some());
        let loc = location.unwrap();
        assert!((loc.latitude() - 52.52).abs() < 0.01);
        assert!((loc.longitude() - 13.405).abs() < 0.01);
    }

    #[tokio::test]
    async fn get_weather_location_falls_back_to_default() {
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

        let service = AgentService::new(Arc::new(mock_inference))
            .with_user_profile_store(Arc::new(NoLocationProfileStore))
            .with_default_weather_location(GeoLocation::london());

        let location = service.get_weather_location().await;

        assert!(location.is_some());
        let loc = location.unwrap();
        assert!((loc.latitude() - 51.5074).abs() < 0.01);
        assert!((loc.longitude() - (-0.1278)).abs() < 0.01);
    }
}
