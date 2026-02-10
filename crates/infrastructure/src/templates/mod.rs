//! Template engine module for rendering email drafts and assistant responses
//!
//! Uses Tera templating engine with custom filters and functions for:
//! - Email draft formatting
//! - WhatsApp message templates
//! - Calendar event summaries
//! - Weather reports
//!
//! # Template Locations
//!
//! Templates can be loaded from:
//! - Embedded templates (compile-time)
//! - File system (runtime, configurable)
//!
//! # Example
//!
//! ```rust,ignore
//! use infrastructure::templates::{TemplateEngine, TemplateContext};
//!
//! let engine = TemplateEngine::new()?;
//!
//! let mut ctx = TemplateContext::new();
//! ctx.insert("recipient", "John");
//! ctx.insert("subject", "Meeting Tomorrow");
//! ctx.insert("body", "Let's discuss the project.");
//!
//! let email = engine.render("email/draft.txt", &ctx)?;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tera::{Context, Tera, Value};
use thiserror::Error;
use tracing::{debug, info};

/// Error type for template operations
#[derive(Debug, Error)]
pub enum TemplateError {
    /// Template not found
    #[error("Template not found: {0}")]
    NotFound(String),

    /// Template rendering failed
    #[error("Template rendering failed: {0}")]
    Render(String),

    /// Template compilation failed
    #[error("Template compilation failed: {0}")]
    Compile(String),

    /// Invalid template context
    #[error("Invalid context: {0}")]
    Context(String),
}

impl From<tera::Error> for TemplateError {
    fn from(e: tera::Error) -> Self {
        match e.kind {
            tera::ErrorKind::TemplateNotFound(name) => Self::NotFound(name),
            _ => Self::Render(e.to_string()),
        }
    }
}

/// Template context wrapper for type-safe context building
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    inner: Context,
}

impl TemplateContext {
    /// Create a new empty template context
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Context::new(),
        }
    }

    /// Insert a value into the context
    pub fn insert<T: Serialize>(&mut self, key: &str, value: &T) {
        self.inner.insert(key, value);
    }

    /// Insert all values from another context
    pub fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
    }

    /// Get the inner Tera context
    #[must_use]
    pub fn into_inner(self) -> Context {
        self.inner
    }
}

/// Email draft template data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDraftData {
    /// Recipient name
    pub recipient: String,
    /// Recipient email address
    pub recipient_email: String,
    /// Email subject
    pub subject: String,
    /// Email body
    pub body: String,
    /// Sender name
    pub sender: String,
    /// Optional CC recipients
    #[serde(default)]
    pub cc: Vec<String>,
    /// Optional attachments
    #[serde(default)]
    pub attachments: Vec<String>,
}

/// Weather report template data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherReportData {
    /// Location name
    pub location: String,
    /// Current temperature
    pub temperature: f64,
    /// Temperature unit (C or F)
    pub unit: String,
    /// Weather condition description
    pub condition: String,
    /// Weather emoji
    pub emoji: String,
    /// Humidity percentage
    pub humidity: u8,
    /// Wind speed
    pub wind_speed: f64,
    /// Forecast for coming days
    #[serde(default)]
    pub forecast: Vec<ForecastDay>,
}

/// Forecast for a single day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastDay {
    /// Day name
    pub day: String,
    /// High temperature
    pub high: f64,
    /// Low temperature
    pub low: f64,
    /// Condition
    pub condition: String,
    /// Emoji
    pub emoji: String,
}

/// Calendar event template data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEventData {
    /// Event title
    pub title: String,
    /// Event start time (formatted)
    pub start_time: String,
    /// Event end time (formatted)
    pub end_time: String,
    /// Event location
    #[serde(default)]
    pub location: Option<String>,
    /// Event description
    #[serde(default)]
    pub description: Option<String>,
    /// Attendees
    #[serde(default)]
    pub attendees: Vec<String>,
}

/// Assistant response template data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantResponseData {
    /// The main response content
    pub content: String,
    /// Optional suggested actions
    #[serde(default)]
    pub suggestions: Vec<String>,
    /// Whether this requires approval
    #[serde(default)]
    pub requires_approval: bool,
    /// Approval command if applicable
    #[serde(default)]
    pub approval_command: Option<String>,
}

/// Template engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateConfig {
    /// Path to custom templates directory (optional)
    #[serde(default)]
    pub templates_dir: Option<String>,

    /// Whether to use embedded templates as fallback
    #[serde(default = "default_true")]
    pub use_embedded_fallback: bool,

    /// Whether to auto-escape HTML by default
    #[serde(default = "default_true")]
    pub auto_escape: bool,
}

const fn default_true() -> bool {
    true
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            templates_dir: None,
            use_embedded_fallback: true,
            auto_escape: true,
        }
    }
}

/// Embedded templates - compiled into the binary
mod embedded {
    pub const EMAIL_DRAFT: &str = r#"To: {{ recipient_email }}
Subject: {{ subject }}
{% if cc %}Cc: {{ cc | join(sep=", ") }}
{% endif %}
Dear {{ recipient }},

{{ body }}

Best regards,
{{ sender }}
"#;

    pub const EMAIL_DRAFT_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body { font-family: Arial, sans-serif; line-height: 1.6; }
        .signature { color: #666; margin-top: 20px; }
    </style>
</head>
<body>
    <p>Dear {{ recipient }},</p>
    {{ body | linebreaksbr }}
    <div class="signature">
        <p>Best regards,<br>{{ sender }}</p>
    </div>
</body>
</html>
"#;

    pub const WEATHER_REPORT: &str = r"{{ emoji }} Weather for {{ location }}

🌡️ Temperature: {{ temperature }}°{{ unit }}
💧 Humidity: {{ humidity }}%
💨 Wind: {{ wind_speed }} km/h
📝 Condition: {{ condition }}
{% if forecast %}
📅 Forecast:
{% for day in forecast %}
  {{ day.day }}: {{ day.emoji }} {{ day.high }}°/{{ day.low }}° - {{ day.condition }}
{% endfor %}
{% endif %}";

    #[allow(clippy::needless_raw_string_hashes)]
    pub const CALENDAR_EVENT: &str = r#"📅 {{ title }}

🕐 {{ start_time }} - {{ end_time }}
{% if location %}📍 {{ location }}
{% endif %}{% if description %}
📝 {{ description }}
{% endif %}{% if attendees %}
👥 Attendees: {{ attendees | join(sep=", ") }}
{% endif %}"#;

    pub const ASSISTANT_RESPONSE: &str = r"{{ content }}
{% if suggestions %}
💡 Suggestions:
{% for suggestion in suggestions %}
  • {{ suggestion }}
{% endfor %}
{% endif %}{% if requires_approval %}
⚠️ This action requires approval.
{% if approval_command %}Use: {{ approval_command }}{% endif %}
{% endif %}";

    pub const APPROVAL_REQUEST: &str = r#"🔐 Approval Required

Command: {{ command }}
Description: {{ description }}
Requested by: {{ user }}
Expires: {{ expires_at }}

Reply with:
✅ "approve" to allow
❌ "deny" to reject"#;
}

/// Template engine using Tera
#[derive(Clone)]
pub struct TemplateEngine {
    tera: Arc<Tera>,
    config: TemplateConfig,
}

impl std::fmt::Debug for TemplateEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateEngine")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl TemplateEngine {
    /// Create a new template engine with default configuration
    pub fn new() -> Result<Self, TemplateError> {
        Self::with_config(TemplateConfig::default())
    }

    /// Create a new template engine with custom configuration
    pub fn with_config(config: TemplateConfig) -> Result<Self, TemplateError> {
        let mut tera = Tera::default();

        // Set auto-escape based on config
        tera.autoescape_on(if config.auto_escape {
            vec![".html", ".htm", ".xml"]
        } else {
            vec![]
        });

        // Load embedded templates
        tera.add_raw_template("email/draft.txt", embedded::EMAIL_DRAFT)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;
        tera.add_raw_template("email/draft.html", embedded::EMAIL_DRAFT_HTML)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;
        tera.add_raw_template("weather/report.txt", embedded::WEATHER_REPORT)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;
        tera.add_raw_template("calendar/event.txt", embedded::CALENDAR_EVENT)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;
        tera.add_raw_template("assistant/response.txt", embedded::ASSISTANT_RESPONSE)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;
        tera.add_raw_template("assistant/approval.txt", embedded::APPROVAL_REQUEST)
            .map_err(|e| TemplateError::Compile(e.to_string()))?;

        // Load custom templates from directory if specified
        if let Some(ref dir) = config.templates_dir {
            let path = Path::new(dir);
            if path.exists() {
                let pattern = format!("{dir}/**/*");
                match Tera::parse(&pattern) {
                    Ok(custom_tera) => {
                        // Merge custom templates by re-parsing them
                        for name in custom_tera.get_template_names() {
                            if let Ok(template) = custom_tera.render(name, &Context::new()) {
                                debug!(template = %name, "Loaded custom template");
                                // For custom templates, we add them as raw templates
                                if let Err(e) = tera.add_raw_template(name, &template) {
                                    debug!(error = %e, "Failed to add custom template {name}");
                                }
                            }
                        }
                        info!(dir = %dir, "Loaded custom templates");
                    },
                    Err(e) => {
                        if !config.use_embedded_fallback {
                            return Err(TemplateError::Compile(e.to_string()));
                        }
                        debug!(error = %e, "Custom templates failed to load, using embedded");
                    },
                }
            }
        }

        // Register custom filters
        tera.register_filter("linebreaksbr", linebreaksbr_filter);
        tera.register_filter("truncate_words", truncate_words_filter);

        Ok(Self {
            tera: Arc::new(tera),
            config,
        })
    }

    /// Render a template with the given context
    pub fn render(
        &self,
        template_name: &str,
        context: &TemplateContext,
    ) -> Result<String, TemplateError> {
        self.tera
            .render(template_name, &context.inner)
            .map_err(TemplateError::from)
    }

    /// Render an email draft
    pub fn render_email_draft(
        &self,
        data: &EmailDraftData,
        html: bool,
    ) -> Result<String, TemplateError> {
        let mut ctx = TemplateContext::new();
        ctx.insert("recipient", &data.recipient);
        ctx.insert("recipient_email", &data.recipient_email);
        ctx.insert("subject", &data.subject);
        ctx.insert("body", &data.body);
        ctx.insert("sender", &data.sender);
        ctx.insert("cc", &data.cc);
        ctx.insert("attachments", &data.attachments);

        let template = if html {
            "email/draft.html"
        } else {
            "email/draft.txt"
        };

        self.render(template, &ctx)
    }

    /// Render a weather report
    pub fn render_weather_report(&self, data: &WeatherReportData) -> Result<String, TemplateError> {
        let mut ctx = TemplateContext::new();
        ctx.insert("location", &data.location);
        ctx.insert("temperature", &data.temperature);
        ctx.insert("unit", &data.unit);
        ctx.insert("condition", &data.condition);
        ctx.insert("emoji", &data.emoji);
        ctx.insert("humidity", &data.humidity);
        ctx.insert("wind_speed", &data.wind_speed);
        ctx.insert("forecast", &data.forecast);

        self.render("weather/report.txt", &ctx)
    }

    /// Render a calendar event summary
    pub fn render_calendar_event(&self, data: &CalendarEventData) -> Result<String, TemplateError> {
        let mut ctx = TemplateContext::new();
        ctx.insert("title", &data.title);
        ctx.insert("start_time", &data.start_time);
        ctx.insert("end_time", &data.end_time);
        ctx.insert("location", &data.location);
        ctx.insert("description", &data.description);
        ctx.insert("attendees", &data.attendees);

        self.render("calendar/event.txt", &ctx)
    }

    /// Render an assistant response
    pub fn render_assistant_response(
        &self,
        data: &AssistantResponseData,
    ) -> Result<String, TemplateError> {
        let mut ctx = TemplateContext::new();
        ctx.insert("content", &data.content);
        ctx.insert("suggestions", &data.suggestions);
        ctx.insert("requires_approval", &data.requires_approval);
        ctx.insert("approval_command", &data.approval_command);

        self.render("assistant/response.txt", &ctx)
    }

    /// Render an approval request notification
    pub fn render_approval_request(
        &self,
        command: &str,
        description: &str,
        user: &str,
        expires_at: &str,
    ) -> Result<String, TemplateError> {
        let mut ctx = TemplateContext::new();
        ctx.insert("command", &command);
        ctx.insert("description", &description);
        ctx.insert("user", &user);
        ctx.insert("expires_at", &expires_at);

        self.render("assistant/approval.txt", &ctx)
    }

    /// Check if a template exists
    #[must_use]
    pub fn template_exists(&self, name: &str) -> bool {
        self.tera.get_template_names().any(|n| n == name)
    }

    /// List all available template names
    #[must_use]
    pub fn list_templates(&self) -> Vec<&str> {
        self.tera.get_template_names().collect()
    }
}

/// Custom filter: Convert newlines to <br> tags
fn linebreaksbr_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("linebreaksbr requires a string"))?;
    Ok(Value::String(s.replace('\n', "<br>\n")))
}

/// Custom filter: Truncate to a number of words
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn truncate_words_filter(value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    let s = value
        .as_str()
        .ok_or_else(|| tera::Error::msg("truncate_words requires a string"))?;

    let count = args.get("count").and_then(Value::as_i64).unwrap_or(20) as usize;

    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() <= count {
        return Ok(Value::String(s.to_string()));
    }

    let truncated = words[..count].join(" ");
    Ok(Value::String(format!("{truncated}...")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_engine_creation() {
        let engine = TemplateEngine::new();
        assert!(engine.is_ok());
    }

    #[test]
    fn test_email_draft_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let data = EmailDraftData {
            recipient: "John".to_string(),
            recipient_email: "john@example.com".to_string(),
            subject: "Meeting Tomorrow".to_string(),
            body: "Let's discuss the project.".to_string(),
            sender: "Alice".to_string(),
            cc: vec![],
            attachments: vec![],
        };

        let result = engine.render_email_draft(&data, false);
        assert!(result.is_ok());
        let email = result.unwrap();
        assert!(email.contains("john@example.com"));
        assert!(email.contains("Dear John"));
        assert!(email.contains("Meeting Tomorrow"));
        assert!(email.contains("Alice"));
    }

    #[test]
    fn test_weather_report_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let data = WeatherReportData {
            location: "Berlin".to_string(),
            temperature: 22.5,
            unit: "C".to_string(),
            condition: "Partly cloudy".to_string(),
            emoji: "⛅".to_string(),
            humidity: 65,
            wind_speed: 15.0,
            forecast: vec![],
        };

        let result = engine.render_weather_report(&data);
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.contains("Berlin"));
        assert!(report.contains("22.5"));
        assert!(report.contains("⛅"));
    }

    #[test]
    fn test_calendar_event_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let data = CalendarEventData {
            title: "Team Meeting".to_string(),
            start_time: "2024-01-15 10:00".to_string(),
            end_time: "2024-01-15 11:00".to_string(),
            location: Some("Conference Room A".to_string()),
            description: Some("Weekly sync".to_string()),
            attendees: vec!["Alice".to_string(), "Bob".to_string()],
        };

        let result = engine.render_calendar_event(&data);
        assert!(result.is_ok());
        let event = result.unwrap();
        assert!(event.contains("Team Meeting"));
        assert!(event.contains("Conference Room A"));
        assert!(event.contains("Alice"));
    }

    #[test]
    fn test_assistant_response_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let data = AssistantResponseData {
            content: "Here's your answer.".to_string(),
            suggestions: vec!["Option A".to_string(), "Option B".to_string()],
            requires_approval: false,
            approval_command: None,
        };

        let result = engine.render_assistant_response(&data);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.contains("Here's your answer"));
        assert!(response.contains("Option A"));
    }

    #[test]
    fn test_approval_request_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let result = engine.render_approval_request(
            "send_email",
            "Send email to client",
            "alice",
            "2024-01-15 12:00",
        );

        assert!(result.is_ok());
        let request = result.unwrap();
        assert!(request.contains("send_email"));
        assert!(request.contains("Approval Required"));
    }

    #[test]
    fn test_template_listing() {
        let engine = TemplateEngine::new().unwrap();
        let templates = engine.list_templates();

        assert!(templates.contains(&"email/draft.txt"));
        assert!(templates.contains(&"weather/report.txt"));
        assert!(templates.contains(&"calendar/event.txt"));
    }

    #[test]
    fn test_template_exists() {
        let engine = TemplateEngine::new().unwrap();

        assert!(engine.template_exists("email/draft.txt"));
        assert!(!engine.template_exists("nonexistent/template.txt"));
    }

    #[test]
    fn test_custom_context() {
        let engine = TemplateEngine::new().unwrap();

        let mut ctx = TemplateContext::new();
        ctx.insert("recipient", &"Bob");
        ctx.insert("recipient_email", &"bob@example.com");
        ctx.insert("subject", &"Hello");
        ctx.insert("body", &"Hi there!");
        ctx.insert("sender", &"Alice");
        ctx.insert("cc", &Vec::<String>::new());
        ctx.insert("attachments", &Vec::<String>::new());

        let result = engine.render("email/draft.txt", &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_linebreaksbr_filter() {
        let value = Value::String("Line 1\nLine 2".to_string());
        let result = linebreaksbr_filter(&value, &HashMap::new()).unwrap();
        assert_eq!(result.as_str().unwrap(), "Line 1<br>\nLine 2");
    }

    #[test]
    fn test_truncate_words_filter() {
        let value = Value::String("one two three four five six".to_string());
        let mut args = HashMap::new();
        args.insert("count".to_string(), Value::Number(3.into()));

        let result = truncate_words_filter(&value, &args).unwrap();
        assert_eq!(result.as_str().unwrap(), "one two three...");
    }

    #[test]
    fn test_truncate_words_filter_short() {
        // Test when text has fewer words than count
        let value = Value::String("one two".to_string());
        let mut args = HashMap::new();
        args.insert("count".to_string(), Value::Number(10.into()));

        let result = truncate_words_filter(&value, &args).unwrap();
        assert_eq!(result.as_str().unwrap(), "one two");
    }

    #[test]
    fn test_truncate_words_filter_default_count() {
        // Test default count (20)
        let value = Value::String("word ".repeat(25).trim().to_string());
        let args = HashMap::new();

        let result = truncate_words_filter(&value, &args).unwrap();
        assert!(result.as_str().unwrap().ends_with("..."));
    }

    #[test]
    fn test_template_config_default() {
        let config = TemplateConfig::default();
        assert!(config.templates_dir.is_none());
        assert!(config.use_embedded_fallback);
        assert!(config.auto_escape);
    }

    #[test]
    fn test_template_context_extend() {
        let mut ctx1 = TemplateContext::new();
        ctx1.insert("key1", &"value1");

        let mut ctx2 = TemplateContext::new();
        ctx2.insert("key2", &"value2");

        ctx1.extend(ctx2);
        // After extend, ctx1 should have both keys
        let inner = ctx1.into_inner();
        assert!(inner.contains_key("key1"));
        assert!(inner.contains_key("key2"));
    }

    #[test]
    fn test_email_draft_html_rendering() {
        let engine = TemplateEngine::new().unwrap();

        let data = EmailDraftData {
            recipient: "John".to_string(),
            recipient_email: "john@example.com".to_string(),
            subject: "Meeting Tomorrow".to_string(),
            body: "Line 1\nLine 2".to_string(),
            sender: "Alice".to_string(),
            cc: vec![],
            attachments: vec![],
        };

        let result = engine.render_email_draft(&data, true);
        assert!(result.is_ok());
        let email = result.unwrap();
        assert!(email.contains("<!DOCTYPE html>"));
        assert!(email.contains("<br>"));
        assert!(email.contains("Dear John"));
    }

    #[test]
    fn test_template_engine_debug() {
        let engine = TemplateEngine::new().unwrap();
        let debug = format!("{engine:?}");
        assert!(debug.contains("TemplateEngine"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn test_template_engine_clone() {
        let engine = TemplateEngine::new().unwrap();
        #[allow(clippy::redundant_clone)]
        let cloned = engine.clone();
        assert!(cloned.template_exists("email/draft.txt"));
    }

    #[test]
    fn test_template_error_display() {
        let err = TemplateError::NotFound("test".to_string());
        assert!(format!("{err}").contains("Template not found"));

        let err = TemplateError::Render("render fail".to_string());
        assert!(format!("{err}").contains("rendering failed"));

        let err = TemplateError::Compile("compile fail".to_string());
        assert!(format!("{err}").contains("compilation failed"));

        let err = TemplateError::Context("ctx fail".to_string());
        assert!(format!("{err}").contains("Invalid context"));
    }

    #[test]
    fn test_weather_report_with_forecast() {
        let engine = TemplateEngine::new().unwrap();

        let data = WeatherReportData {
            location: "Munich".to_string(),
            temperature: 18.0,
            unit: "C".to_string(),
            condition: "Sunny".to_string(),
            emoji: "☀️".to_string(),
            humidity: 50,
            wind_speed: 10.0,
            forecast: vec![
                ForecastDay {
                    day: "Monday".to_string(),
                    high: 20.0,
                    low: 12.0,
                    condition: "Sunny".to_string(),
                    emoji: "☀️".to_string(),
                },
                ForecastDay {
                    day: "Tuesday".to_string(),
                    high: 18.0,
                    low: 10.0,
                    condition: "Cloudy".to_string(),
                    emoji: "☁️".to_string(),
                },
            ],
        };

        let result = engine.render_weather_report(&data);
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.contains("Monday"));
        assert!(report.contains("Tuesday"));
        assert!(report.contains("Forecast"));
    }

    #[test]
    fn test_calendar_event_minimal() {
        let engine = TemplateEngine::new().unwrap();

        let data = CalendarEventData {
            title: "Quick Meeting".to_string(),
            start_time: "10:00".to_string(),
            end_time: "10:30".to_string(),
            location: None,
            description: None,
            attendees: vec![],
        };

        let result = engine.render_calendar_event(&data);
        assert!(result.is_ok());
        let event = result.unwrap();
        assert!(event.contains("Quick Meeting"));
        assert!(!event.contains("Attendees"));
    }

    #[test]
    fn test_assistant_response_with_approval() {
        let engine = TemplateEngine::new().unwrap();

        let data = AssistantResponseData {
            content: "Ready to send".to_string(),
            suggestions: vec![],
            requires_approval: true,
            approval_command: Some("approve 123".to_string()),
        };

        let result = engine.render_assistant_response(&data);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.contains("requires approval"));
        assert!(response.contains("approve 123"));
    }

    #[test]
    fn test_email_draft_with_cc() {
        let engine = TemplateEngine::new().unwrap();

        let data = EmailDraftData {
            recipient: "John".to_string(),
            recipient_email: "john@example.com".to_string(),
            subject: "Update".to_string(),
            body: "Please review.".to_string(),
            sender: "Alice".to_string(),
            cc: vec![
                "bob@example.com".to_string(),
                "carol@example.com".to_string(),
            ],
            attachments: vec![],
        };

        let result = engine.render_email_draft(&data, false);
        assert!(result.is_ok());
        let email = result.unwrap();
        assert!(email.contains("bob@example.com"));
        assert!(email.contains("carol@example.com"));
    }

    #[test]
    fn test_linebreaksbr_filter_no_newlines() {
        let value = Value::String("No newlines here".to_string());
        let result = linebreaksbr_filter(&value, &HashMap::new()).unwrap();
        assert_eq!(result.as_str().unwrap(), "No newlines here");
    }

    #[test]
    fn test_with_config_no_auto_escape() {
        let config = TemplateConfig {
            auto_escape: false,
            ..Default::default()
        };
        let engine = TemplateEngine::with_config(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_data_structs_debug() {
        let email = EmailDraftData {
            recipient: "Test".to_string(),
            recipient_email: "test@test.com".to_string(),
            subject: "Subject".to_string(),
            body: "Body".to_string(),
            sender: "Sender".to_string(),
            cc: vec![],
            attachments: vec![],
        };
        assert!(format!("{email:?}").contains("EmailDraftData"));

        let weather = WeatherReportData {
            location: "Test".to_string(),
            temperature: 20.0,
            unit: "C".to_string(),
            condition: "Clear".to_string(),
            emoji: "☀️".to_string(),
            humidity: 50,
            wind_speed: 10.0,
            forecast: vec![],
        };
        assert!(format!("{weather:?}").contains("WeatherReportData"));

        let calendar = CalendarEventData {
            title: "Event".to_string(),
            start_time: "10:00".to_string(),
            end_time: "11:00".to_string(),
            location: None,
            description: None,
            attendees: vec![],
        };
        assert!(format!("{calendar:?}").contains("CalendarEventData"));

        let response = AssistantResponseData {
            content: "Content".to_string(),
            suggestions: vec![],
            requires_approval: false,
            approval_command: None,
        };
        assert!(format!("{response:?}").contains("AssistantResponseData"));
    }
}
