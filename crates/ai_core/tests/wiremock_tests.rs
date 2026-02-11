//! Integration tests for Ollama inference engine using WireMock
//!
//! These tests mock the Ollama HTTP API to verify client behavior without
//! requiring an actual Ollama server.

use ai_core::{
    EmbeddingConfig, InferenceConfig, InferenceRequest, OllamaEmbeddingEngine,
    OllamaInferenceEngine,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// =============================================================================
// Test Helpers
// =============================================================================

fn inference_config_for_mock(base_url: &str) -> InferenceConfig {
    InferenceConfig {
        base_url: base_url.to_string(),
        default_model: "test-model".to_string(),
        temperature: 0.7,
        max_tokens: 100,
        top_p: 0.9,
        timeout_ms: 5000,
        system_prompt: None,
    }
}

fn embedding_config_for_mock(base_url: &str) -> EmbeddingConfig {
    EmbeddingConfig {
        base_url: base_url.to_string(),
        model: "nomic-embed-text".to_string(),
        timeout_ms: 5000,
        dimensions: 384,
    }
}

/// Sample Ollama chat success response
fn chat_success_response() -> serde_json::Value {
    serde_json::json!({
        "model": "test-model",
        "message": {
            "role": "assistant",
            "content": "Hello! How can I help you today?"
        },
        "done": true,
        "prompt_eval_count": 10,
        "eval_count": 15
    })
}

/// Sample Ollama models list response
fn models_list_response() -> serde_json::Value {
    serde_json::json!({
        "models": [
            {"name": "llama2:7b"},
            {"name": "qwen2.5-1.5b-instruct"},
            {"name": "nomic-embed-text"}
        ]
    })
}

/// Sample Ollama embed response (single)
#[allow(clippy::cast_precision_loss)]
fn embed_single_response() -> serde_json::Value {
    // Generate a 384-dimensional embedding
    let embedding: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
    serde_json::json!({
        "embeddings": [embedding]
    })
}

/// Sample Ollama embed response (batch)
#[allow(clippy::cast_precision_loss)]
fn embed_batch_response(count: usize) -> serde_json::Value {
    let embeddings: Vec<Vec<f32>> = (0..count)
        .map(|i| (0..384).map(|j| ((i + j) as f32) / 384.0).collect())
        .collect();
    serde_json::json!({
        "embeddings": embeddings
    })
}

// =============================================================================
// Inference Engine Tests
// =============================================================================

mod inference_tests {
    use super::*;
    use ai_core::InferenceEngine;

    #[tokio::test]
    async fn generate_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_success_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let request = InferenceRequest::simple("Hello");
        let response = engine.generate(request).await;

        assert!(response.is_ok());
        let response = response.unwrap();
        assert_eq!(response.model, "test-model");
        assert!(response.content.contains("Hello"));
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 15);
        assert_eq!(usage.total_tokens, 25);
    }

    #[tokio::test]
    async fn generate_with_system_prompt() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_success_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let request = InferenceRequest::with_system("You are helpful", "Hello");
        let response = engine.generate(request).await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn generate_with_custom_model() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_success_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let request = InferenceRequest::simple("Hello").with_model("custom-model");
        let response = engine.generate(request).await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn generate_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let request = InferenceRequest::simple("Hello");
        let response = engine.generate(request).await;

        assert!(response.is_err());
        let err = response.unwrap_err();
        assert!(err.to_string().contains("500") || err.to_string().contains("Server"));
    }

    #[tokio::test]
    async fn generate_invalid_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let request = InferenceRequest::simple("Hello");
        let response = engine.generate(request).await;

        assert!(response.is_err());
    }

    #[tokio::test]
    async fn health_check_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(models_list_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let healthy = engine.health_check().await;
        assert!(healthy.is_ok());
        assert!(healthy.unwrap());
    }

    #[tokio::test]
    async fn health_check_server_down() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let healthy = engine.health_check().await;
        assert!(healthy.is_ok());
        assert!(!healthy.unwrap());
    }

    #[tokio::test]
    async fn list_models_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(models_list_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let models = engine.list_models().await;
        assert!(models.is_ok());
        let models = models.unwrap();
        assert_eq!(models.len(), 3);
        assert!(models.contains(&"llama2:7b".to_string()));
        assert!(models.contains(&"qwen2.5-1.5b-instruct".to_string()));
    }

    #[tokio::test]
    async fn list_models_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = inference_config_for_mock(&mock_server.uri());
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        let models = engine.list_models().await;
        assert!(models.is_err());
    }

    #[test]
    fn default_model_getter() {
        let config = InferenceConfig::default();
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        assert!(engine.default_model().contains("qwen"));
    }

    #[test]
    fn set_default_model() {
        let config = InferenceConfig::default();
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        engine.set_default_model("new-model");
        assert_eq!(engine.default_model(), "new-model");
    }

    #[test]
    fn api_url_construction() {
        let config = InferenceConfig {
            base_url: "http://example.com:11434".to_string(),
            ..Default::default()
        };
        let engine = OllamaInferenceEngine::new(config).expect("Failed to create engine");

        // Test through debug output since api_url is private
        let debug = format!("{engine:?}");
        assert!(debug.contains("example.com"));
    }
}

// =============================================================================
// Embedding Engine Tests
// =============================================================================

mod embedding_tests {
    use super::*;

    #[tokio::test]
    async fn embed_single_text_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(embed_single_response()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = embedding_config_for_mock(&mock_server.uri());
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let embedding = engine.embed("Hello world").await;
        assert!(embedding.is_ok());
        let embedding = embedding.unwrap();
        assert_eq!(embedding.len(), 384);
    }

    #[tokio::test]
    async fn embed_batch_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(embed_batch_response(3)))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = embedding_config_for_mock(&mock_server.uri());
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let texts = vec!["Hello".to_string(), "World".to_string(), "Test".to_string()];
        let embeddings = engine.embed_batch(&texts).await;

        assert!(embeddings.is_ok());
        let embeddings = embeddings.unwrap();
        assert_eq!(embeddings.len(), 3);
        for embedding in &embeddings {
            assert_eq!(embedding.len(), 384);
        }
    }

    #[tokio::test]
    async fn embed_batch_empty() {
        let config = embedding_config_for_mock("http://localhost:11434");
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let texts: Vec<String> = vec![];
        let embeddings = engine.embed_batch(&texts).await;

        assert!(embeddings.is_ok());
        assert!(embeddings.unwrap().is_empty());
    }

    #[tokio::test]
    async fn embed_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Model not found"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = embedding_config_for_mock(&mock_server.uri());
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let embedding = engine.embed("Hello").await;
        assert!(embedding.is_err());
    }

    #[tokio::test]
    async fn embed_invalid_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = embedding_config_for_mock(&mock_server.uri());
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let embedding = engine.embed("Hello").await;
        assert!(embedding.is_err());
    }

    #[tokio::test]
    async fn embed_empty_response() {
        let mock_server = MockServer::start().await;

        // Response with empty embeddings array
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": []
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = embedding_config_for_mock(&mock_server.uri());
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        let embedding = engine.embed("Hello").await;
        assert!(embedding.is_err());
    }

    #[test]
    fn embedding_config_variants() {
        let nomic = EmbeddingConfig::nomic_embed_text();
        assert_eq!(nomic.model, "nomic-embed-text");
        assert_eq!(nomic.dimensions, 384);

        let mxbai = EmbeddingConfig::mxbai_embed_large();
        assert_eq!(mxbai.model, "mxbai-embed-large");
        assert_eq!(mxbai.dimensions, 1024);

        let bge = EmbeddingConfig::bge_m3();
        assert_eq!(bge.model, "bge-m3");
        assert_eq!(bge.dimensions, 1024);
    }

    #[test]
    fn cosine_similarity_calculations() {
        // Identical vectors
        let v1 = vec![1.0, 0.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&v1, &v1);
        assert!((similarity - 1.0).abs() < 0.001);

        // Orthogonal vectors
        let v2 = vec![0.0, 1.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&v1, &v2);
        assert!(similarity.abs() < 0.001);

        // Opposite vectors
        let v3 = vec![-1.0, 0.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&v1, &v3);
        assert!((similarity + 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_edge_cases() {
        // Empty vectors
        let empty: Vec<f32> = vec![];
        assert!(OllamaEmbeddingEngine::cosine_similarity(&empty, &empty).abs() < f32::EPSILON);

        // Different lengths
        let short = vec![1.0, 0.0];
        let long = vec![1.0, 0.0, 0.0];
        assert!(OllamaEmbeddingEngine::cosine_similarity(&short, &long).abs() < f32::EPSILON);

        // Zero vectors
        let zero = vec![0.0, 0.0, 0.0];
        assert!(OllamaEmbeddingEngine::cosine_similarity(&zero, &zero).abs() < f32::EPSILON);
    }

    #[test]
    fn model_and_dimensions_getters() {
        let config = EmbeddingConfig::nomic_embed_text();
        let engine = OllamaEmbeddingEngine::new(config).expect("Failed to create engine");

        assert_eq!(engine.model(), "nomic-embed-text");
        assert_eq!(engine.dimensions(), 384);
    }
}

// =============================================================================
// InferenceRequest Tests
// =============================================================================

mod request_tests {
    use super::*;

    #[test]
    fn simple_request() {
        let request = InferenceRequest::simple("Hello");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, "user");
        assert_eq!(request.messages[0].content, "Hello");
        assert!(request.model.is_none());
        assert!(!request.stream);
    }

    #[test]
    fn request_with_system() {
        let request = InferenceRequest::with_system("Be helpful", "Hello");
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[0].content, "Be helpful");
        assert_eq!(request.messages[1].role, "user");
        assert_eq!(request.messages[1].content, "Hello");
    }

    #[test]
    fn request_with_model() {
        let request = InferenceRequest::simple("Hello").with_model("custom-model");
        assert_eq!(request.model, Some("custom-model".to_string()));
    }

    #[test]
    fn request_streaming() {
        let request = InferenceRequest::simple("Hello").streaming();
        assert!(request.stream);
    }

    #[test]
    fn request_serialization() {
        let request = InferenceRequest::simple("Hello").with_model("test");
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("messages"));
        assert!(json.contains("Hello"));
        assert!(json.contains("test"));
    }
}

// =============================================================================
// Config Tests
// =============================================================================

mod config_tests {
    use super::*;

    #[test]
    fn inference_config_default() {
        let config = InferenceConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert!(config.temperature > 0.0);
        assert!(config.max_tokens > 0);
    }

    #[test]
    fn inference_config_hailo() {
        let config = InferenceConfig::hailo_qwen();
        assert!(config.default_model.contains("qwen"));
    }

    #[test]
    fn embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimensions, 384);
    }

    #[test]
    fn embedding_config_serialization() {
        let config = EmbeddingConfig::nomic_embed_text();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.model, parsed.model);
        assert_eq!(config.dimensions, parsed.dimensions);
    }
}

// =============================================================================
// Error Tests
// =============================================================================

mod error_tests {
    use ai_core::InferenceError;

    #[test]
    fn error_display_connection_failed() {
        let err = InferenceError::ConnectionFailed("timeout".to_string());
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn error_display_server_error() {
        let err = InferenceError::ServerError("500 Internal".to_string());
        assert!(err.to_string().contains("500"));
    }

    #[test]
    fn error_display_invalid_response() {
        let err = InferenceError::InvalidResponse("bad json".to_string());
        assert!(err.to_string().contains("json"));
    }

    #[test]
    fn error_display_request_failed() {
        let err = InferenceError::RequestFailed("network issue".to_string());
        assert!(err.to_string().contains("network"));
    }
}

// =============================================================================
// Property-Based Tests
// =============================================================================

mod proptest_tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn cosine_similarity_self_is_one(
            v in prop::collection::vec(-1000.0f32..1000.0f32, 1..100)
        ) {
            // Filter out zero vectors
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > f32::EPSILON {
                let similarity = ai_core::OllamaEmbeddingEngine::cosine_similarity(&v, &v);
                prop_assert!((similarity - 1.0).abs() < 0.001, "Self-similarity should be ~1.0, got {}", similarity);
            }
        }

        #[test]
        fn cosine_similarity_symmetric(
            a in prop::collection::vec(-100.0f32..100.0f32, 1..50),
            b in prop::collection::vec(-100.0f32..100.0f32, 1..50)
        ) {
            if a.len() == b.len() {
                let sim_ab = ai_core::OllamaEmbeddingEngine::cosine_similarity(&a, &b);
                let sim_ba = ai_core::OllamaEmbeddingEngine::cosine_similarity(&b, &a);
                prop_assert!((sim_ab - sim_ba).abs() < 0.001, "Cosine similarity should be symmetric");
            }
        }

        #[test]
        fn cosine_similarity_bounds(
            a in prop::collection::vec(-100.0f32..100.0f32, 1..50),
            b in prop::collection::vec(-100.0f32..100.0f32, 1..50)
        ) {
            if a.len() == b.len() {
                let similarity = ai_core::OllamaEmbeddingEngine::cosine_similarity(&a, &b);
                prop_assert!((-1.0..=1.0).contains(&similarity), "Similarity should be in [-1, 1], got {}", similarity);
            }
        }

        #[test]
        fn inference_request_serialization_roundtrip(
            content in "[a-zA-Z0-9 ]{1,100}",
            model in "[a-z0-9-]{1,20}"
        ) {
            let request = ai_core::InferenceRequest::simple(&content).with_model(&model);
            let json = serde_json::to_string(&request).unwrap();
            let parsed: ai_core::InferenceRequest = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(request.messages.len(), parsed.messages.len());
            prop_assert_eq!(request.model, parsed.model);
        }

        #[test]
        fn embedding_config_serialization_roundtrip(
            model in "[a-z0-9-]{1,30}",
            dimensions in 1usize..2048
        ) {
            let config = ai_core::EmbeddingConfig {
                base_url: "http://localhost:11434".to_string(),
                model,
                timeout_ms: 30000,
                dimensions,
            };
            let json = serde_json::to_string(&config).unwrap();
            let parsed: ai_core::EmbeddingConfig = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(config.model, parsed.model);
            prop_assert_eq!(config.dimensions, parsed.dimensions);
        }
    }
}
