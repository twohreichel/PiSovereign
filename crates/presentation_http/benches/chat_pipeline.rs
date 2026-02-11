//! Benchmarks for the chat pipeline
//!
//! These benchmarks measure the performance of the chat service and HTTP handlers
//! using a mock inference engine to isolate the pipeline overhead.

#![allow(clippy::expect_used)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use application::{
    AgentService, ChatService,
    error::ApplicationError,
    ports::{ConversationStore, InferencePort, InferenceResult, StreamingChunk},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use domain::{ChatMessage, Conversation, ConversationId, ConversationSource};
use futures::stream;
use infrastructure::AppConfig;
use presentation_http::{
    handlers::metrics::MetricsCollector, routes::create_router, state::AppState,
};
use tokio::{runtime::Runtime, sync::RwLock};

/// Mock inference engine with configurable latency
struct MockInference {
    response: String,
    model: String,
}

impl MockInference {
    fn new() -> Self {
        Self {
            response: "This is a mock AI response for benchmarking purposes. It contains enough text to be realistic but doesn't actually invoke any ML model.".to_string(),
            model: "mock-benchmark-model".to_string(),
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
            latency_ms: 1,
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
            latency_ms: 1,
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
            latency_ms: 1,
        })
    }

    async fn generate_stream(
        &self,
        _message: &str,
    ) -> Result<application::ports::InferenceStream, ApplicationError> {
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
        true
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

/// Mock conversation store for benchmarking
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

    async fn get_by_phone_number(
        &self,
        source: ConversationSource,
        phone_number: &str,
    ) -> Result<Option<Conversation>, ApplicationError> {
        let store = self.conversations.read().await;
        Ok(store
            .values()
            .find(|c| c.source == source && c.phone_number.as_deref() == Some(phone_number))
            .cloned())
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

fn create_benchmark_state() -> AppState {
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());

    let chat_service = Arc::new(ChatService::with_conversation_store(
        inference.clone(),
        conversation_store,
    ));

    let agent_service = Arc::new(AgentService::new(inference));

    AppState {
        chat_service,
        agent_service,
        approval_service: None,
        health_service: None,
        voice_message_service: None,
        messenger_adapter: None,
        signal_client: None,
        prompt_sanitizer: None,
        suspicious_activity_tracker: None,
        conversation_store: None,
        config: presentation_http::ReloadableConfig::new(AppConfig::default()),
        metrics: Arc::new(MetricsCollector::new()),
    }
}

/// Benchmark the chat service directly (no HTTP layer)
fn bench_chat_service(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
    let chat_service = ChatService::new(inference);

    let mut group = c.benchmark_group("chat_service");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    // Single message (stateless)
    group.bench_function("single_message", |b| {
        b.to_async(&rt).iter(|| async {
            chat_service
                .chat("Hello, how are you?")
                .await
                .expect("Chat should succeed")
        });
    });

    // With conversation context
    let conversation_store: Arc<dyn ConversationStore> = Arc::new(MockConversationStore::new());
    let chat_service_with_store =
        ChatService::with_conversation_store(Arc::new(MockInference::new()), conversation_store);

    group.bench_function("with_context", |b| {
        b.to_async(&rt).iter(|| async {
            chat_service_with_store
                .chat_with_context("Tell me about Rust", None)
                .await
                .expect("Chat should succeed")
        });
    });

    group.finish();
}

/// Benchmark the HTTP handler layer
fn bench_http_handler(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");

    let mut group = c.benchmark_group("http_handler");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    // Chat endpoint
    group.bench_function("chat_endpoint", |b| {
        b.to_async(&rt).iter(|| async {
            let state = create_benchmark_state();
            let router = create_router(state);
            let server = axum_test::TestServer::new(router).expect("Failed to create server");

            server
                .post("/v1/chat")
                .json(&serde_json::json!({
                    "message": "What is the capital of France?"
                }))
                .await
        });
    });

    // Health endpoint (baseline for HTTP overhead)
    group.bench_function("health_endpoint", |b| {
        b.to_async(&rt).iter(|| async {
            let state = create_benchmark_state();
            let router = create_router(state);
            let server = axum_test::TestServer::new(router).expect("Failed to create server");

            server.get("/health").await
        });
    });

    // Command parsing endpoint
    group.bench_function("command_parse_endpoint", |b| {
        b.to_async(&rt).iter(|| async {
            let state = create_benchmark_state();
            let router = create_router(state);
            let server = axum_test::TestServer::new(router).expect("Failed to create server");

            server
                .post("/v1/commands/parse")
                .json(&serde_json::json!({
                    "input": "set reminder for tomorrow at 3pm"
                }))
                .await
        });
    });

    group.finish();
}

/// Benchmark different message sizes
fn bench_message_sizes(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let inference: Arc<dyn InferencePort> = Arc::new(MockInference::new());
    let chat_service = ChatService::new(inference);

    let mut group = c.benchmark_group("message_sizes");
    group.measurement_time(Duration::from_secs(10));

    for size in [10, 100, 1000, 5000] {
        let message: String = "x".repeat(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &message, |b, msg| {
            b.to_async(&rt)
                .iter(|| async { chat_service.chat(msg).await.expect("Chat should succeed") });
        });
    }

    group.finish();
}

/// Benchmark conversation history accumulation
fn bench_conversation_accumulation(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");

    let mut group = c.benchmark_group("conversation_accumulation");
    group.measurement_time(Duration::from_secs(15));

    for message_count in [1, 5, 10, 25, 50] {
        group.bench_with_input(
            BenchmarkId::from_parameter(message_count),
            &message_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let conversation_store: Arc<dyn ConversationStore> =
                        Arc::new(MockConversationStore::new());
                    let chat_service = ChatService::with_conversation_store(
                        Arc::new(MockInference::new()),
                        conversation_store,
                    );

                    // Build up conversation history
                    let mut conv_id = None;
                    for i in 0..count {
                        let (_, id) = chat_service
                            .chat_with_context(&format!("Message number {i}"), conv_id.as_deref())
                            .await
                            .expect("Chat should succeed");
                        conv_id = Some(id.to_string());
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_chat_service,
    bench_http_handler,
    bench_message_sizes,
    bench_conversation_accumulation,
);
criterion_main!(benches);
