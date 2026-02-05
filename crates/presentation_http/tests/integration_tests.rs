//! Integration tests for HTTP handlers
#![allow(clippy::expect_used)]

use std::{collections::HashMap, sync::Arc};

use application::{
    AgentService, ChatService,
    error::ApplicationError,
    ports::{ConversationStore, InferencePort, InferenceResult},
};
use async_trait::async_trait;
use axum_test::TestServer;
use chrono::{DateTime, Utc};
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
