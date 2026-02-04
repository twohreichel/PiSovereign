//! Integration tests for HTTP handlers
#![allow(clippy::expect_used)]

use std::sync::Arc;

use application::{
    AgentService, ChatService,
    error::ApplicationError,
    ports::{InferencePort, InferenceResult},
};
use async_trait::async_trait;
use axum_test::TestServer;
use domain::Conversation;
use infrastructure::AppConfig;
use presentation_http::{
    handlers::metrics::MetricsCollector, routes::create_router, state::AppState,
};
use serde_json::json;

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

    fn current_model(&self) -> &str {
        &self.model
    }

    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        Ok(vec![self.model.clone()])
    }
}

fn create_test_state() -> AppState {
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
    AppState {
        chat_service: Arc::new(ChatService::new(inference.clone())),
        agent_service: Arc::new(AgentService::new(inference)),
        config: presentation_http::ReloadableConfig::new(AppConfig::default()),
        metrics: Arc::new(MetricsCollector::new()),
    }
}

fn create_unhealthy_test_state() -> AppState {
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::unhealthy());
    AppState {
        chat_service: Arc::new(ChatService::new(inference.clone())),
        agent_service: Arc::new(AgentService::new(inference)),
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

    let response = server
        .post("/v1/chat")
        .json(&json!({
            "message": "Continue our conversation",
            "conversation_id": "test-conv-123"
        }))
        .await;

    response.assert_status_ok();
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
            "input": "hilfe"
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
            "input": "modelle"
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
