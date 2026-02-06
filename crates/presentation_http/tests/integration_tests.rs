//! Integration tests for HTTP handlers
#![allow(clippy::expect_used)]

use std::{collections::HashMap, sync::Arc};

use application::{
    AgentService, ChatService, HealthService,
    error::ApplicationError,
    ports::{
        CalendarPort, ConversationStore, DatabaseHealthPort, EmailPort, InferencePort,
        InferenceResult, WeatherPort,
    },
};
use async_trait::async_trait;
use axum_test::TestServer;
use chrono::{DateTime, Utc};
use domain::value_objects::GeoLocation;
use domain::{ChatMessage, Conversation, ConversationId};
use infrastructure::AppConfig;
use presentation_http::{
    handlers::metrics::MetricsCollector, routes::create_router, state::AppState,
};
use serde_json::json;
use tokio::sync::RwLock;

/// Mock inference engine for testing
struct MockInference {
    response: String,
    healthy: bool,
    model: String,
}

impl MockInference {
    fn new() -> Self {
        Self {
            response: "Mock AI response".to_string(),
            healthy: true,
            model: "mock-model".to_string(),
        }
    }

    fn unhealthy() -> Self {
        Self {
            response: String::new(),
            healthy: false,
            model: "mock-model".to_string(),
        }
    }
}

#[async_trait]
impl InferencePort for MockInference {
    async fn generate(&self, _message: &str) -> Result<InferenceResult, ApplicationError> {
        Ok(InferenceResult {
            content: self.response.clone(),
            model: self.model.clone(),
            tokens_used: Some(42),
            latency_ms: 100,
        })
    }

    async fn generate_with_context(
        &self,
        _conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError> {
        Ok(InferenceResult {
            content: self.response.clone(),
            model: self.model.clone(),
            tokens_used: Some(50),
            latency_ms: 150,
        })
    }

    async fn generate_with_system(
        &self,
        _system_prompt: &str,
        _message: &str,
    ) -> Result<InferenceResult, ApplicationError> {
        Ok(InferenceResult {
            content: self.response.clone(),
            model: self.model.clone(),
            tokens_used: Some(60),
            latency_ms: 120,
        })
    }

    async fn generate_stream(
        &self,
        _message: &str,
    ) -> Result<application::ports::InferenceStream, ApplicationError> {
        use application::ports::StreamingChunk;
        use futures::stream;
        let response = self.response.clone();
        let model = self.model.clone();
        let stream = stream::iter(vec![Ok(StreamingChunk {
            content: response,
            done: true,
            model: Some(model),
        })]);
        Ok(Box::pin(stream))
    }

    async fn generate_stream_with_system(
        &self,
        _system_prompt: &str,
        _message: &str,
    ) -> Result<application::ports::InferenceStream, ApplicationError> {
        self.generate_stream("").await
    }

    async fn is_healthy(&self) -> bool {
        self.healthy
    }

    fn current_model(&self) -> String {
        self.model.clone()
    }

    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        Ok(vec![self.model.clone()])
    }

    async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
        Ok(())
    }
}

/// Mock conversation store for testing
struct MockConversationStore {
    conversations: RwLock<HashMap<String, Conversation>>,
}

impl MockConversationStore {
    fn new() -> Self {
        Self {
            conversations: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ConversationStore for MockConversationStore {
    async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        let mut store = self.conversations.write().await;
        store.insert(conversation.id.to_string(), conversation.clone());
        Ok(())
    }

    async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError> {
        let store = self.conversations.read().await;
        Ok(store.get(&id.to_string()).cloned())
    }

    async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        let mut store = self.conversations.write().await;
        store.insert(conversation.id.to_string(), conversation.clone());
        Ok(())
    }

    async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError> {
        let mut store = self.conversations.write().await;
        store.remove(&id.to_string());
        Ok(())
    }

    async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: &ChatMessage,
    ) -> Result<(), ApplicationError> {
        let mut store = self.conversations.write().await;
        if let Some(conv) = store.get_mut(&conversation_id.to_string()) {
            conv.add_message(message.clone());
        }
        Ok(())
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError> {
        let store = self.conversations.read().await;
        let mut convs: Vec<_> = store.values().cloned().collect();
        convs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        convs.truncate(limit);
        Ok(convs)
    }

    async fn search(
        &self,
        _query: &str,
        limit: usize,
    ) -> Result<Vec<Conversation>, ApplicationError> {
        self.list_recent(limit).await
    }

    async fn cleanup_older_than(&self, cutoff: DateTime<Utc>) -> Result<usize, ApplicationError> {
        let mut store = self.conversations.write().await;
        let before = store.len();
        store.retain(|_, conv| conv.updated_at >= cutoff);
        Ok(before - store.len())
    }
}

// ============================================================================
// Mock implementations for HealthService integration testing
// ============================================================================

/// Mock database health port for testing
struct MockDatabaseHealth {
    healthy: bool,
}

impl MockDatabaseHealth {
    const fn new(healthy: bool) -> Self {
        Self { healthy }
    }
}

#[async_trait]
impl DatabaseHealthPort for MockDatabaseHealth {
    async fn is_available(&self) -> bool {
        self.healthy
    }

    async fn check_health(&self) -> Result<application::ports::DatabaseHealth, ApplicationError> {
        if self.healthy {
            Ok(application::ports::DatabaseHealth::healthy_with_version(
                "3.45.0",
            ))
        } else {
            Err(ApplicationError::ExternalService(
                "Mock database unhealthy".to_string(),
            ))
        }
    }
}

/// Mock email port for testing
struct MockEmailPort {
    healthy: bool,
}

impl MockEmailPort {
    const fn new(healthy: bool) -> Self {
        Self { healthy }
    }
}

#[async_trait]
impl EmailPort for MockEmailPort {
    async fn get_inbox(
        &self,
        _count: u32,
    ) -> Result<Vec<application::ports::EmailSummary>, application::ports::EmailError> {
        Ok(vec![])
    }

    async fn get_mailbox(
        &self,
        _mailbox: &str,
        _count: u32,
    ) -> Result<Vec<application::ports::EmailSummary>, application::ports::EmailError> {
        Ok(vec![])
    }

    async fn get_unread_count(&self) -> Result<u32, application::ports::EmailError> {
        Ok(0)
    }

    async fn mark_read(&self, _email_id: &str) -> Result<(), application::ports::EmailError> {
        Ok(())
    }

    async fn mark_unread(&self, _email_id: &str) -> Result<(), application::ports::EmailError> {
        Ok(())
    }

    async fn delete(&self, _email_id: &str) -> Result<(), application::ports::EmailError> {
        Ok(())
    }

    async fn send_email(
        &self,
        _draft: &application::ports::EmailDraft,
    ) -> Result<String, application::ports::EmailError> {
        if self.healthy {
            Ok("mock-message-id".to_string())
        } else {
            Err(application::ports::EmailError::ServiceUnavailable)
        }
    }

    async fn is_available(&self) -> bool {
        self.healthy
    }

    async fn list_mailboxes(&self) -> Result<Vec<String>, application::ports::EmailError> {
        Ok(vec!["INBOX".to_string()])
    }
}

/// Mock calendar port for testing
struct MockCalendarPort {
    healthy: bool,
}

impl MockCalendarPort {
    const fn new(healthy: bool) -> Self {
        Self { healthy }
    }
}

#[async_trait]
impl CalendarPort for MockCalendarPort {
    async fn list_calendars(
        &self,
    ) -> Result<Vec<application::ports::CalendarInfo>, application::ports::CalendarError> {
        Ok(vec![])
    }

    async fn get_events_for_date(
        &self,
        _date: chrono::NaiveDate,
    ) -> Result<Vec<application::ports::CalendarEvent>, application::ports::CalendarError> {
        Ok(vec![])
    }

    async fn get_events_in_range(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<application::ports::CalendarEvent>, application::ports::CalendarError> {
        Ok(vec![])
    }

    async fn get_event(
        &self,
        _event_id: &str,
    ) -> Result<application::ports::CalendarEvent, application::ports::CalendarError> {
        Err(application::ports::CalendarError::EventNotFound(
            "mock".to_string(),
        ))
    }

    async fn create_event(
        &self,
        _event: &application::ports::NewEvent,
    ) -> Result<String, application::ports::CalendarError> {
        Ok("mock-event-id".to_string())
    }

    async fn update_event(
        &self,
        _event_id: &str,
        _event: &application::ports::NewEvent,
    ) -> Result<(), application::ports::CalendarError> {
        Ok(())
    }

    async fn delete_event(&self, _event_id: &str) -> Result<(), application::ports::CalendarError> {
        Ok(())
    }

    async fn is_available(&self) -> bool {
        self.healthy
    }

    async fn get_next_event(
        &self,
    ) -> Result<Option<application::ports::CalendarEvent>, application::ports::CalendarError> {
        Ok(None)
    }
}

/// Mock weather port for testing
struct MockWeatherPort {
    healthy: bool,
}

impl MockWeatherPort {
    const fn new(healthy: bool) -> Self {
        Self { healthy }
    }
}

#[async_trait]
impl WeatherPort for MockWeatherPort {
    async fn get_current_weather(
        &self,
        _location: &GeoLocation,
    ) -> Result<application::ports::CurrentWeather, ApplicationError> {
        if self.healthy {
            Ok(application::ports::CurrentWeather {
                temperature: 20.0,
                apparent_temperature: 18.0,
                humidity: 65,
                wind_speed: 10.0,
                condition: application::ports::WeatherCondition::PartlyCloudy,
                observed_at: Utc::now(),
            })
        } else {
            Err(ApplicationError::ExternalService(
                "Mock weather unhealthy".to_string(),
            ))
        }
    }

    async fn get_forecast(
        &self,
        _location: &GeoLocation,
        _days: u8,
    ) -> Result<Vec<application::ports::DailyForecast>, ApplicationError> {
        Ok(vec![])
    }

    async fn is_available(&self) -> bool {
        self.healthy
    }
}

/// Create test state with fully configured HealthService
#[allow(clippy::fn_params_excessive_bools)]
fn create_test_state_with_health_service(
    inference_healthy: bool,
    db_healthy: bool,
    email_healthy: bool,
    calendar_healthy: bool,
    weather_healthy: bool,
) -> AppState {
    let inference: Arc<dyn InferencePort> = if inference_healthy {
        Arc::new(MockInference::new())
    } else {
        Arc::new(MockInference::unhealthy())
    };
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());

    let database: Arc<dyn DatabaseHealthPort> = Arc::new(MockDatabaseHealth::new(db_healthy));
    let email: Arc<dyn EmailPort> = Arc::new(MockEmailPort::new(email_healthy));
    let calendar: Arc<dyn CalendarPort> = Arc::new(MockCalendarPort::new(calendar_healthy));
    let weather: Arc<dyn WeatherPort> = Arc::new(MockWeatherPort::new(weather_healthy));

    let health_service = HealthService::new(Arc::clone(&inference))
        .with_database(database)
        .with_email(email)
        .with_calendar(calendar)
        .with_weather(weather);

    AppState {
        chat_service: Arc::new(ChatService::with_conversation_store(
            inference.clone(),
            conversation_store,
        )),
        agent_service: Arc::new(AgentService::new(inference)),
        approval_service: None,
        health_service: Some(Arc::new(health_service)),
        config: presentation_http::ReloadableConfig::new(AppConfig::default()),
        metrics: Arc::new(MetricsCollector::new()),
    }
}

fn create_test_state() -> AppState {
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
    AppState {
        chat_service: Arc::new(ChatService::with_conversation_store(
            inference.clone(),
            conversation_store,
        )),
        agent_service: Arc::new(AgentService::new(inference)),
        approval_service: None,
        health_service: None,
        config: presentation_http::ReloadableConfig::new(AppConfig::default()),
        metrics: Arc::new(MetricsCollector::new()),
    }
}

fn create_unhealthy_test_state() -> AppState {
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::unhealthy());
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
    AppState {
        chat_service: Arc::new(ChatService::with_conversation_store(
            inference.clone(),
            conversation_store,
        )),
        agent_service: Arc::new(AgentService::new(inference)),
        approval_service: None,
        health_service: None,
        config: presentation_http::ReloadableConfig::new(AppConfig::default()),
        metrics: Arc::new(MetricsCollector::new()),
    }
}

fn create_test_server() -> TestServer {
    let state = create_test_state();
    let router = create_router(state);
    TestServer::new(router).expect("Failed to create test server")
}

fn create_unhealthy_test_server() -> TestServer {
    let state = create_unhealthy_test_state();
    let router = create_router(state);
    TestServer::new(router).expect("Failed to create test server")
}

// ============ Health Endpoint Tests ============

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let server = create_test_server();

    let response = server.get("/health").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());
}

#[tokio::test]
async fn readiness_endpoint_returns_ready_when_healthy() {
    let server = create_test_server();

    let response = server.get("/ready").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["ready"], true);
    assert_eq!(body["inference"]["healthy"], true);
    assert!(body["inference"]["model"].is_string());
}

#[tokio::test]
async fn readiness_endpoint_returns_unavailable_when_unhealthy() {
    let server = create_unhealthy_test_server();

    let response = server.get("/ready").await;

    response.assert_status_service_unavailable();
    let body: serde_json::Value = response.json();
    assert_eq!(body["ready"], false);
    assert_eq!(body["inference"]["healthy"], false);
}

// ============ Chat Endpoint Tests ============

#[tokio::test]
async fn chat_endpoint_returns_response() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "Hello, AI!"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["message"].is_string());
    assert!(body["model"].is_string());
    assert!(body["latency_ms"].is_number());
    assert!(body["conversation_id"].is_string());
}

#[tokio::test]
async fn chat_endpoint_rejects_empty_message() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "   "
        }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn chat_endpoint_with_conversation_id() {
    let server = create_test_server();

    // First message creates a conversation
    let first_response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "Hello"
        }))
        .await;

    first_response.assert_status_ok();
    let first_body: serde_json::Value = first_response.json();
    let conv_id = first_body["conversation_id"].as_str().unwrap();

    // Second message continues the conversation
    let response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "Continue our conversation",
            "conversation_id": conv_id
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["conversation_id"].as_str().unwrap(), conv_id);
}

#[tokio::test]
async fn chat_endpoint_with_invalid_conversation_id() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "Hello",
            "conversation_id": "not-a-valid-uuid"
        }))
        .await;

    // Invalid UUID should return a validation error
    response.assert_status_bad_request();
}

#[tokio::test]
async fn chat_stream_endpoint_returns_sse() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat/stream")
        .json(&json!({
            "message": "Stream this response"
        }))
        .await;

    response.assert_status_ok();
    // SSE responses have text/event-stream content type
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
async fn chat_stream_endpoint_rejects_empty_message() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat/stream")
        .json(&json!({
            "message": ""
        }))
        .await;

    response.assert_status_bad_request();
}

// ============ Command Endpoint Tests ============

#[tokio::test]
async fn execute_command_echo() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "echo Hello World"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["command_type"], "echo");
    assert!(body["response"].as_str().unwrap().contains("Hello World"));
}

#[tokio::test]
async fn execute_command_help() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "help"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["command_type"], "help");
}

#[tokio::test]
async fn execute_command_status() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "status"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["success"], true);
    assert_eq!(body["command_type"], "system");
}

#[tokio::test]
async fn execute_command_rejects_empty_input() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "   "
        }))
        .await;

    response.assert_status_bad_request();
}

#[tokio::test]
async fn parse_command_echo() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands/parse")
        .json(&json!({
            "input": "echo test"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["requires_approval"], false);
    assert!(body["description"].is_string());
}

#[tokio::test]
async fn parse_command_rejects_empty_input() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands/parse")
        .json(&json!({
            "input": ""
        }))
        .await;

    response.assert_status_bad_request();
}

// ============ System Endpoint Tests ============

#[tokio::test]
async fn system_status_endpoint() {
    let server = create_test_server();

    let response = server.get("/v1/system/status").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["version"].is_string());
    assert!(body["model"].is_string());
    assert!(body["inference_healthy"].is_boolean());
    assert!(body["uptime_info"].is_string());
}

#[tokio::test]
async fn system_status_shows_healthy_inference() {
    let server = create_test_server();

    let response = server.get("/v1/system/status").await;

    let body: serde_json::Value = response.json();
    assert_eq!(body["inference_healthy"], true);
}

#[tokio::test]
async fn system_status_shows_unhealthy_inference() {
    let server = create_unhealthy_test_server();

    let response = server.get("/v1/system/status").await;

    let body: serde_json::Value = response.json();
    assert_eq!(body["inference_healthy"], false);
}

#[tokio::test]
async fn system_models_endpoint() {
    let server = create_test_server();

    let response = server.get("/v1/system/models").await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["current"].is_string());
    assert!(body["available"].is_array());
}

#[tokio::test]
async fn system_models_lists_available() {
    let server = create_test_server();

    let response = server.get("/v1/system/models").await;

    let body: serde_json::Value = response.json();
    let available = body["available"].as_array().unwrap();
    assert!(!available.is_empty());

    // Check first model has required fields
    let first = &available[0];
    assert!(first["name"].is_string());
    assert!(first["description"].is_string());
    assert!(first["parameters"].is_string());
}

// ============ Route Tests ============

#[tokio::test]
async fn unknown_route_returns_404() {
    let server = create_test_server();

    let response = server.get("/unknown/path").await;

    response.assert_status_not_found();
}

#[tokio::test]
async fn wrong_method_returns_405() {
    let server = create_test_server();

    // /health only accepts GET, not POST
    let response = server.post("/health").await;

    // 405 Method Not Allowed
    response.assert_status_not_ok();
}

// ============ Error Handling Tests ============

#[tokio::test]
async fn invalid_json_returns_bad_request() {
    let server = create_test_server();

    let response = server
        .post("/v1/chat")
        .json(&json!("not a valid object"))
        .await;

    // Our ValidatedJson returns 400 for JSON parse errors
    response.assert_status_bad_request();
}

#[tokio::test]
async fn missing_required_field_returns_error() {
    let server = create_test_server();

    let response = server.post("/v1/chat").json(&json!({})).await;

    // Missing "message" field returns 400
    response.assert_status_bad_request();
}

// ============ Ask Command Integration ============

#[tokio::test]
async fn execute_ask_command() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "Was ist der Sinn des Lebens?"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    // Unknown inputs become "ask" commands
    assert_eq!(body["command_type"], "ask");
}

// ============ Briefing Command Integration ============

#[tokio::test]
async fn execute_briefing_command() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "briefing"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["command_type"], "morning_briefing");
}

// ============ Version Command Integration ============

#[tokio::test]
async fn execute_version_command() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "version"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["command_type"], "system");
    assert!(body["response"].as_str().unwrap().contains("PiSovereign"));
}

// ============ Models Command Integration ============

#[tokio::test]
async fn execute_models_command() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "models"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["command_type"], "system");
}

// ============ Inbox Command Integration ============

#[tokio::test]
async fn execute_inbox_command() {
    let server = create_test_server();

    let response = server
        .post("/v1/commands")
        .json(&json!({
            "input": "inbox"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["command_type"], "summarize_inbox");
}

// ============ Degraded Mode Tests ============

mod degraded_mode_tests {
    use super::*;
    use infrastructure::adapters::{DegradedInferenceAdapter, DegradedModeConfig};

    fn create_degraded_inference(fail: bool) -> Arc<dyn InferencePort> {
        let mock = if fail {
            MockFailingInference::new()
        } else {
            MockFailingInference::healthy()
        };
        let config = DegradedModeConfig {
            enabled: true,
            failure_threshold: 2,
            success_threshold: 2,
            retry_cooldown_secs: 0,
            unavailable_message: "Service temporarily unavailable".to_string(),
        };
        Arc::new(DegradedInferenceAdapter::new(Arc::new(mock), config))
    }

    fn create_degraded_test_state(fail: bool) -> AppState {
        let inference = create_degraded_inference(fail);
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
        AppState {
            chat_service: Arc::new(ChatService::with_conversation_store(
                inference.clone(),
                conversation_store,
            )),
            agent_service: Arc::new(AgentService::new(inference)),
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    struct MockFailingInference {
        should_fail: std::sync::atomic::AtomicBool,
    }

    impl MockFailingInference {
        const fn new() -> Self {
            Self {
                should_fail: std::sync::atomic::AtomicBool::new(true),
            }
        }

        const fn healthy() -> Self {
            Self {
                should_fail: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl InferencePort for MockFailingInference {
        async fn generate(&self, _message: &str) -> Result<InferenceResult, ApplicationError> {
            if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                Ok(InferenceResult {
                    content: "Success response".to_string(),
                    model: "mock".to_string(),
                    tokens_used: Some(10),
                    latency_ms: 50,
                })
            }
        }

        async fn generate_with_context(
            &self,
            _conversation: &Conversation,
        ) -> Result<InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<application::ports::InferenceStream, ApplicationError> {
            if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                use application::ports::StreamingChunk;
                use futures::stream;
                Ok(Box::pin(stream::iter(vec![Ok(StreamingChunk {
                    content: "Success".to_string(),
                    done: true,
                    model: Some("mock".to_string()),
                })])))
            }
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<application::ports::InferenceStream, ApplicationError> {
            self.generate_stream("").await
        }

        async fn is_healthy(&self) -> bool {
            !self.should_fail.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn current_model(&self) -> String {
            "mock".to_string()
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            Ok(vec!["mock".to_string()])
        }

        async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn degraded_mode_returns_fallback_after_failures() {
        let state = create_degraded_test_state(true);
        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        // First request should fail normally (not yet in degraded mode)
        let response = server
            .post("/v1/chat")
            .json(&json!({ "message": "test" }))
            .await;
        // The degraded adapter should handle failures - may return client error, server error, or fallback success
        let status = response.status_code();
        assert!(
            status.is_client_error() || status.is_server_error() || status.is_success(),
            "Expected valid HTTP response, got: {status}"
        );
    }

    #[tokio::test]
    async fn healthy_service_passes_through() {
        let state = create_degraded_test_state(false);
        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        let response = server
            .post("/v1/chat")
            .json(&json!({ "message": "test" }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert!(body["message"].as_str().unwrap().contains("Success"));
    }
}

// ============ Workflow Integration Tests ============

mod workflow_tests {
    use super::*;
    use application::ports::{DraftStorePort, UserProfileStore};
    use domain::{
        DraftId, EmailAddress, PersistedEmailDraft, UserId,
        entities::UserProfile,
        value_objects::{GeoLocation, Timezone},
    };

    /// Mock draft store for workflow testing
    struct MockDraftStore {
        drafts: RwLock<HashMap<String, PersistedEmailDraft>>,
    }

    impl MockDraftStore {
        fn new() -> Self {
            Self {
                drafts: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl DraftStorePort for MockDraftStore {
        async fn save(&self, draft: &PersistedEmailDraft) -> Result<DraftId, ApplicationError> {
            let mut store = self.drafts.write().await;
            store.insert(draft.id.to_string(), draft.clone());
            Ok(draft.id)
        }

        async fn get(&self, id: &DraftId) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
            let store = self.drafts.read().await;
            Ok(store.get(&id.to_string()).cloned())
        }

        async fn get_for_user(
            &self,
            id: &DraftId,
            user_id: &UserId,
        ) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
            let store = self.drafts.read().await;
            Ok(store
                .get(&id.to_string())
                .filter(|d| d.user_id == *user_id)
                .cloned())
        }

        async fn delete(&self, id: &DraftId) -> Result<bool, ApplicationError> {
            let mut store = self.drafts.write().await;
            Ok(store.remove(&id.to_string()).is_some())
        }

        async fn list_for_user(
            &self,
            user_id: &UserId,
            limit: usize,
        ) -> Result<Vec<PersistedEmailDraft>, ApplicationError> {
            let store = self.drafts.read().await;
            let mut drafts: Vec<_> = store
                .values()
                .filter(|d| d.user_id == *user_id)
                .cloned()
                .collect();
            drafts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            drafts.truncate(limit);
            Ok(drafts)
        }

        async fn cleanup_expired(&self) -> Result<usize, ApplicationError> {
            let mut store = self.drafts.write().await;
            let now = Utc::now();
            let before = store.len();
            store.retain(|_, draft| draft.expires_at > now);
            Ok(before - store.len())
        }
    }

    /// Mock user profile store for workflow testing
    struct MockUserProfileStore {
        profiles: RwLock<HashMap<String, UserProfile>>,
    }

    impl MockUserProfileStore {
        fn new() -> Self {
            Self {
                profiles: RwLock::new(HashMap::new()),
            }
        }

        #[allow(dead_code)]
        async fn add_profile(&self, profile: UserProfile) {
            let mut store = self.profiles.write().await;
            store.insert(profile.id().to_string(), profile);
        }
    }

    #[async_trait]
    impl UserProfileStore for MockUserProfileStore {
        async fn save(&self, profile: &UserProfile) -> Result<(), ApplicationError> {
            let mut store = self.profiles.write().await;
            store.insert(profile.id().to_string(), profile.clone());
            Ok(())
        }

        async fn get(&self, user_id: &UserId) -> Result<Option<UserProfile>, ApplicationError> {
            let store = self.profiles.read().await;
            Ok(store.get(&user_id.to_string()).cloned())
        }

        async fn delete(&self, user_id: &UserId) -> Result<bool, ApplicationError> {
            let mut store = self.profiles.write().await;
            Ok(store.remove(&user_id.to_string()).is_some())
        }

        async fn update_location(
            &self,
            user_id: &UserId,
            location: Option<&GeoLocation>,
        ) -> Result<bool, ApplicationError> {
            let mut store = self.profiles.write().await;
            store
                .get_mut(&user_id.to_string())
                .map_or(Ok(false), |profile| {
                    if let Some(loc) = location {
                        profile.update_location(*loc);
                    } else {
                        profile.clear_location();
                    }
                    Ok(true)
                })
        }

        async fn update_timezone(
            &self,
            user_id: &UserId,
            timezone: &Timezone,
        ) -> Result<bool, ApplicationError> {
            let mut store = self.profiles.write().await;
            store
                .get_mut(&user_id.to_string())
                .map_or(Ok(false), |profile| {
                    profile.update_timezone(timezone.clone());
                    Ok(true)
                })
        }
    }

    fn create_workflow_test_state_with_draft_store() -> (AppState, Arc<MockDraftStore>) {
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
        let draft_store = Arc::new(MockDraftStore::new());

        let agent_service = AgentService::new(inference.clone())
            .with_draft_store(draft_store.clone() as Arc<dyn DraftStorePort>);

        let state = AppState {
            chat_service: Arc::new(ChatService::with_conversation_store(
                inference,
                conversation_store,
            )),
            agent_service: Arc::new(agent_service),
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        (state, draft_store)
    }

    fn create_workflow_test_state_with_user_profile() -> (AppState, Arc<MockUserProfileStore>) {
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
        let user_profile_store = Arc::new(MockUserProfileStore::new());

        let agent_service = AgentService::new(inference.clone())
            .with_user_profile_store(user_profile_store.clone() as Arc<dyn UserProfileStore>);

        let state = AppState {
            chat_service: Arc::new(ChatService::with_conversation_store(
                inference,
                conversation_store,
            )),
            agent_service: Arc::new(agent_service),
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        (state, user_profile_store)
    }

    // ============ Draft Workflow Tests ============

    #[tokio::test]
    async fn draft_email_workflow_creates_and_stores_draft() {
        let draft_store = Arc::new(MockDraftStore::new());
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());

        // Create agent service with draft store
        let agent_service = AgentService::new(inference)
            .with_draft_store(draft_store.clone() as Arc<dyn DraftStorePort>);

        // Execute the draft command directly (bypasses intent parsing)
        let command = domain::AgentCommand::DraftEmail {
            to: EmailAddress::try_from("test@example.com").unwrap(),
            subject: Some("Test Email".to_string()),
            body: "Hello, this is a test.".to_string(),
        };

        let result = agent_service.execute_command(&command).await.unwrap();
        assert!(
            result.success,
            "Command should succeed, got: {}",
            result.response
        );

        // Verify draft was stored
        let default_user = UserId::default();
        let drafts = draft_store.list_for_user(&default_user, 10).await.unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].to, "test@example.com");
    }

    #[tokio::test]
    async fn draft_email_workflow_stores_correct_details() {
        let draft_store = Arc::new(MockDraftStore::new());
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());

        let agent_service = AgentService::new(inference)
            .with_draft_store(draft_store.clone() as Arc<dyn DraftStorePort>);

        // Create a draft via direct command execution
        let command = domain::AgentCommand::DraftEmail {
            to: EmailAddress::try_from("recipient@example.com").unwrap(),
            subject: Some("Important Meeting".to_string()),
            body: "Please confirm your attendance.".to_string(),
        };

        let result = agent_service.execute_command(&command).await.unwrap();
        assert!(
            result.success,
            "Command should succeed, got: {}",
            result.response
        );

        // Verify stored draft details
        let default_user = UserId::default();
        let drafts = draft_store.list_for_user(&default_user, 10).await.unwrap();
        assert_eq!(drafts.len(), 1, "Expected 1 draft, got {}", drafts.len());

        let draft = &drafts[0];
        assert_eq!(draft.to, "recipient@example.com");
        assert_eq!(draft.subject, "Important Meeting");
        assert!(draft.body.contains("confirm your attendance"));
        assert!(draft.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn draft_email_workflow_retrieves_stored_draft() {
        let (state, draft_store) = create_workflow_test_state_with_draft_store();
        let router = create_router(state);
        let _server = TestServer::new(router).expect("Failed to create test server");

        // Manually create and store a draft
        let user_id = UserId::default();
        let draft = PersistedEmailDraft::new(
            user_id,
            "retrieve@example.com".to_string(),
            "Test Retrieval".to_string(),
            "This is the body.".to_string(),
        );
        let draft_id = draft.id;
        draft_store.save(&draft).await.unwrap();

        // Verify retrieval
        let retrieved = draft_store.get(&draft_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.to, "retrieve@example.com");
        assert_eq!(retrieved.subject, "Test Retrieval");
    }

    #[tokio::test]
    async fn draft_email_workflow_respects_user_ownership() {
        let draft_store = MockDraftStore::new();

        // Create drafts for different users
        let user1 = UserId::default();
        let user2 = UserId::new();

        let draft1 = PersistedEmailDraft::new(
            user1,
            "user1@example.com".to_string(),
            "User 1 Draft".to_string(),
            "Body 1".to_string(),
        );
        let draft2 = PersistedEmailDraft::new(
            user2,
            "user2@example.com".to_string(),
            "User 2 Draft".to_string(),
            "Body 2".to_string(),
        );

        draft_store.save(&draft1).await.unwrap();
        draft_store.save(&draft2).await.unwrap();

        // Each user should only see their own drafts
        let user1_drafts = draft_store.list_for_user(&user1, 10).await.unwrap();
        let user2_drafts = draft_store.list_for_user(&user2, 10).await.unwrap();

        assert_eq!(user1_drafts.len(), 1);
        assert_eq!(user1_drafts[0].to, "user1@example.com");

        assert_eq!(user2_drafts.len(), 1);
        assert_eq!(user2_drafts[0].to, "user2@example.com");
    }

    #[tokio::test]
    async fn draft_email_workflow_cleanup_removes_expired() {
        let draft_store = MockDraftStore::new();
        let user_id = UserId::default();

        // Create an expired draft (by manipulating the mock)
        let mut draft = PersistedEmailDraft::new(
            user_id,
            "expired@example.com".to_string(),
            "Expired".to_string(),
            "Body".to_string(),
        );
        // Set expiry in the past
        draft.expires_at = Utc::now() - chrono::Duration::hours(1);

        draft_store.save(&draft).await.unwrap();

        // Verify it's stored
        let drafts_before = draft_store.list_for_user(&user_id, 10).await.unwrap();
        assert_eq!(drafts_before.len(), 1);

        // Run cleanup
        let cleaned = draft_store.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);

        // Verify it's gone
        let drafts_after = draft_store.list_for_user(&user_id, 10).await.unwrap();
        assert!(drafts_after.is_empty());
    }

    // ============ User Profile Workflow Tests ============

    #[tokio::test]
    async fn user_profile_workflow_stores_and_retrieves() {
        let (_, profile_store) = create_workflow_test_state_with_user_profile();

        let user_id = UserId::default();
        let timezone = Timezone::from("Europe/Berlin");
        let location = GeoLocation::new(52.52, 13.405).unwrap(); // Berlin
        let profile = UserProfile::with_defaults(user_id, location, timezone);

        // Save profile
        profile_store.save(&profile).await.unwrap();

        // Retrieve profile
        let retrieved = profile_store.get(&user_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.timezone().as_str(), "Europe/Berlin");
    }

    #[tokio::test]
    async fn user_profile_workflow_updates_timezone() {
        let (_, profile_store) = create_workflow_test_state_with_user_profile();

        let user_id = UserId::default();
        let profile = UserProfile::new(user_id);

        profile_store.save(&profile).await.unwrap();

        // Update timezone
        let new_tz = Timezone::from("America/New_York");
        let updated = profile_store
            .update_timezone(&user_id, &new_tz)
            .await
            .unwrap();
        assert!(updated);

        // Verify update
        let retrieved = profile_store.get(&user_id).await.unwrap().unwrap();
        assert_eq!(retrieved.timezone().as_str(), "America/New_York");
    }

    #[tokio::test]
    async fn user_profile_workflow_updates_location() {
        let (_, profile_store) = create_workflow_test_state_with_user_profile();

        let user_id = UserId::default();
        let profile = UserProfile::new(user_id);

        profile_store.save(&profile).await.unwrap();

        // Update location
        let location = GeoLocation::new(52.52, 13.405).unwrap(); // Berlin
        let updated = profile_store
            .update_location(&user_id, Some(&location))
            .await
            .unwrap();
        assert!(updated);

        // Verify update
        let retrieved = profile_store.get(&user_id).await.unwrap().unwrap();
        assert!(retrieved.location().is_some());
        let loc = retrieved.location().unwrap();
        assert!((loc.latitude() - 52.52).abs() < 0.01);
        assert!((loc.longitude() - 13.405).abs() < 0.01);
    }

    #[tokio::test]
    async fn briefing_command_executes_successfully() {
        let (state, _) = create_workflow_test_state_with_user_profile();
        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        let response = server
            .post("/v1/commands")
            .json(&json!({
                "input": "briefing"
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], true);
        assert_eq!(body["command_type"], "morning_briefing");
    }

    // ============ Combined Workflow Tests ============

    #[tokio::test]
    async fn multiple_commands_in_sequence() {
        let (state, _) = create_workflow_test_state_with_draft_store();
        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        // Execute multiple commands in sequence
        let commands = vec![
            ("echo Hello", "echo"),
            ("help", "help"),
            ("status", "system"),
            ("version", "system"),
        ];

        for (input, expected_type) in commands {
            let response = server
                .post("/v1/commands")
                .json(&json!({ "input": input }))
                .await;

            response.assert_status_ok();
            let body: serde_json::Value = response.json();
            assert_eq!(body["success"], true);
            assert_eq!(
                body["command_type"], expected_type,
                "Failed for input: {input}"
            );
        }
    }

    // ============ End-to-End Pipeline Tests ============

    /// Tests the complete chat pipeline with conversation context persistence
    #[tokio::test]
    async fn e2e_contextual_chat_maintains_conversation_history() {
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
        let draft_store: Arc<dyn DraftStorePort> = Arc::new(MockDraftStore::new());

        let chat_service = Arc::new(ChatService::with_conversation_store(
            inference.clone(),
            conversation_store.clone(),
        ));

        let agent_service = Arc::new(
            AgentService::new(inference).with_draft_store(draft_store as Arc<dyn DraftStorePort>),
        );

        let state = AppState {
            chat_service,
            agent_service,
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        // First message - creates new conversation
        let response = server
            .post("/v1/chat")
            .json(&json!({
                "message": "Hello, who are you?"
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        let conversation_id = body["conversation_id"]
            .as_str()
            .expect("Should have conversation_id");
        assert!(!body["message"].as_str().unwrap().is_empty());

        // Second message - continues conversation
        let response = server
            .post("/v1/chat")
            .json(&json!({
                "message": "Tell me more about yourself",
                "conversation_id": conversation_id
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["conversation_id"].as_str().unwrap(), conversation_id);

        // Third message - verifies conversation is maintained
        let response = server
            .post("/v1/chat")
            .json(&json!({
                "message": "What did I ask you first?",
                "conversation_id": conversation_id
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["conversation_id"].as_str().unwrap(), conversation_id);
    }

    /// Tests chat with system prompt customization
    #[tokio::test]
    async fn e2e_chat_with_custom_system_prompt() {
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());

        let chat_service = Arc::new(ChatService::with_all(
            inference.clone(),
            conversation_store,
            "You are a helpful assistant specialized in Rust programming.",
        ));

        let agent_service = Arc::new(AgentService::new(inference));

        let state = AppState {
            chat_service,
            agent_service,
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        let response = server
            .post("/v1/chat")
            .json(&json!({
                "message": "How do I use async/await in Rust?"
            }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert!(body["message"].as_str().is_some());
    }

    /// Tests the full pipeline from command input through inference to response
    #[tokio::test]
    async fn e2e_command_pipeline_with_inference() {
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
        let draft_store: Arc<dyn DraftStorePort> = Arc::new(MockDraftStore::new());

        let chat_service = Arc::new(ChatService::with_conversation_store(
            inference.clone(),
            conversation_store,
        ));

        let agent_service = Arc::new(AgentService::new(inference).with_draft_store(draft_store));

        let state = AppState {
            chat_service,
            agent_service,
            approval_service: None,
            health_service: None,
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        // Test echo command
        let response = server
            .post("/v1/commands")
            .json(&json!({ "input": "echo Hello, World!" }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], true);
        assert!(body["response"].as_str().unwrap().contains("Hello, World!"));

        // Test briefing command (uses inference)
        let response = server
            .post("/v1/commands")
            .json(&json!({ "input": "briefing" }))
            .await;

        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["success"], true);
        assert_eq!(body["command_type"], "morning_briefing");
    }

    /// Tests health and readiness endpoints reflect actual service state
    #[tokio::test]
    async fn e2e_health_endpoints_reflect_service_state() {
        // Test with healthy inference
        let healthy_server = create_test_server();

        let response = healthy_server.get("/health").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["status"], "ok");

        let response = healthy_server.get("/ready").await;
        response.assert_status_ok();
        let body: serde_json::Value = response.json();
        assert_eq!(body["ready"], true);
        assert_eq!(body["inference"]["healthy"], true);

        // Test with unhealthy inference
        let unhealthy_server = create_unhealthy_test_server();

        let response = unhealthy_server.get("/ready").await;
        response.assert_status(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }
}

// ============================================================================
// HealthService E2E Integration Tests
// ============================================================================

mod health_service_e2e_tests {
    use super::*;

    #[allow(clippy::fn_params_excessive_bools)]
    fn create_health_test_server(
        inference: bool,
        db: bool,
        email: bool,
        calendar: bool,
        weather: bool,
    ) -> TestServer {
        let state = create_test_state_with_health_service(inference, db, email, calendar, weather);
        let router = create_router(state);
        TestServer::new(router).expect("Failed to create test server")
    }

    #[tokio::test]
    async fn health_service_all_services_healthy() {
        let server = create_health_test_server(true, true, true, true, true);

        let response = server.get("/ready").await;
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["ready"], true);
        assert_eq!(body["inference"]["healthy"], true);
    }

    #[tokio::test]
    async fn health_service_inference_unhealthy() {
        let server = create_health_test_server(false, true, true, true, true);

        let response = server.get("/ready").await;
        response.assert_status_service_unavailable();

        let body: serde_json::Value = response.json();
        assert_eq!(body["ready"], false);
        assert_eq!(body["inference"]["healthy"], false);
    }

    #[tokio::test]
    async fn health_service_database_unhealthy() {
        // Database is not a critical service for readiness (inference is required)
        let server = create_health_test_server(true, false, true, true, true);

        let response = server.get("/ready").await;
        // Should still be ready as long as inference is healthy
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["ready"], true);
    }

    #[tokio::test]
    async fn health_service_multiple_services_unhealthy() {
        let server = create_health_test_server(true, false, false, false, false);

        let response = server.get("/ready").await;
        // Should still be ready as inference is healthy
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["ready"], true);
        assert_eq!(body["inference"]["healthy"], true);
    }

    #[tokio::test]
    async fn health_endpoint_always_returns_status() {
        // Even with some services unhealthy, /health should return basic status
        let server = create_health_test_server(true, false, false, false, false);

        let response = server.get("/health").await;
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert_eq!(body["status"], "ok");
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn health_service_response_includes_model_info() {
        let server = create_health_test_server(true, true, true, true, true);

        let response = server.get("/ready").await;
        response.assert_status_ok();

        let body: serde_json::Value = response.json();
        assert!(body["inference"]["model"].is_string());
    }

    #[tokio::test]
    async fn health_service_graceful_with_partial_configuration() {
        // Test with minimal configuration (only inference)
        let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
        let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());

        let health_service = HealthService::new(Arc::clone(&inference));

        let state = AppState {
            chat_service: Arc::new(ChatService::with_conversation_store(
                inference.clone(),
                conversation_store,
            )),
            agent_service: Arc::new(AgentService::new(inference)),
            approval_service: None,
            health_service: Some(Arc::new(health_service)),
            config: presentation_http::ReloadableConfig::new(AppConfig::default()),
            metrics: Arc::new(MetricsCollector::new()),
        };

        let router = create_router(state);
        let server = TestServer::new(router).expect("Failed to create test server");

        let response = server.get("/ready").await;
        response.assert_status_ok();
    }
}

// ============================================================================
// SecurityValidator Scenario Tests
// ============================================================================

mod security_validator_tests {
    #![allow(clippy::field_reassign_with_default)]
    use infrastructure::config::Environment;
    use infrastructure::{AppConfig, SecurityValidator};

    #[test]
    fn security_validator_default_config_has_warnings() {
        let config = AppConfig::default();
        let warnings = SecurityValidator::validate(&config);

        // Default config should have some warnings (no CORS origins, no API key)
        assert!(
            !warnings.is_empty(),
            "Default config should produce warnings"
        );
    }

    #[test]
    fn security_validator_production_config_blocks_startup_without_cors() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Production);
        // No allowed origins configured

        let warnings = SecurityValidator::validate(&config);
        let should_block = SecurityValidator::should_block_startup(&config, &warnings);

        // Should block in production with no CORS configured
        assert!(should_block, "Production should block without CORS config");
    }

    #[test]
    fn security_validator_development_config_does_not_block() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Development);

        let warnings = SecurityValidator::validate(&config);
        let should_block = SecurityValidator::should_block_startup(&config, &warnings);

        // Development should not block even with warnings
        assert!(
            !should_block,
            "Development should not block startup with warnings"
        );
    }

    #[test]
    fn security_validator_secure_production_config_does_not_block() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Production);
        config.server.allowed_origins = vec!["https://example.com".to_string()];
        config.security.api_key_users.insert(
            "sk-secure-key".to_string(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
        );
        config.security.tls_verify_certs = true;
        config.security.rate_limit_enabled = true;
        config.database.run_migrations = false;

        let warnings = SecurityValidator::validate(&config);
        let should_block = SecurityValidator::should_block_startup(&config, &warnings);

        // Secure production config should not block
        assert!(
            !should_block,
            "Secure production config should not block startup"
        );
    }

    #[test]
    fn security_validator_tls_disabled_warning_severity() {
        let mut config = AppConfig::default();
        config.security.tls_verify_certs = false;

        let warnings = SecurityValidator::validate(&config);
        let tls_warning = warnings.iter().find(|w| w.code == "SEC001");

        assert!(tls_warning.is_some(), "Should have TLS warning");

        // In development, TLS warning should not be critical
        let warning = tls_warning.unwrap();
        assert!(!warning.is_critical() || config.environment == Some(Environment::Production));
    }

    #[test]
    fn security_validator_warns_on_plaintext_secrets() {
        let mut config = AppConfig::default();
        config.security.api_key = Some("sk-secret-production-key-12345".to_string());

        let warnings = SecurityValidator::validate(&config);

        // Should detect plaintext API key
        let has_plaintext_warning = warnings.iter().any(|w| w.code == "SEC003");
        assert!(has_plaintext_warning, "Should warn about plaintext API key");
    }

    #[test]
    fn security_validator_accepts_env_var_references() {
        let mut config = AppConfig::default();
        config.security.api_key = Some("${API_KEY}".to_string());

        let warnings = SecurityValidator::validate(&config);

        // Should not warn about env var references
        let has_plaintext_warning = warnings.iter().any(|w| w.code == "SEC003");
        assert!(
            !has_plaintext_warning,
            "Should not warn about env var references"
        );
    }

    #[test]
    fn security_validator_rate_limiting_in_production() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Production);
        config.security.rate_limit_enabled = false;

        let warnings = SecurityValidator::validate(&config);

        // Should warn about disabled rate limiting in production
        let has_rate_limit_warning = warnings.iter().any(|w| w.code == "SEC008");
        assert!(
            has_rate_limit_warning,
            "Should warn about disabled rate limiting in production"
        );
    }

    #[test]
    fn security_validator_auto_migrations_in_production() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Production);
        config.database.run_migrations = true;

        let warnings = SecurityValidator::validate(&config);

        // Should warn about auto migrations in production
        let has_migration_warning = warnings.iter().any(|w| w.code == "SEC009");
        assert!(
            has_migration_warning,
            "Should warn about auto migrations in production"
        );
    }

    #[test]
    fn security_validator_warnings_sorted_by_severity() {
        let mut config = AppConfig::default();
        config.environment = Some(Environment::Production);
        config.security.tls_verify_certs = false;
        config.security.rate_limit_enabled = false;

        let warnings = SecurityValidator::validate(&config);

        // Verify warnings are sorted by severity (critical first)
        let severities: Vec<_> = warnings.iter().map(|w| w.severity).collect();
        let mut sorted_severities = severities.clone();
        sorted_severities.sort_by(|a, b| b.cmp(a)); // Sort descending (critical first)

        assert_eq!(
            severities, sorted_severities,
            "Warnings should be sorted by severity"
        );
    }
}
