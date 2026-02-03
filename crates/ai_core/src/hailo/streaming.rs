//! Streaming response handling for Hailo-Ollama

use futures::stream::{self, StreamExt};
use reqwest::Response;
use serde::Deserialize;
use tracing::trace;

use crate::{
    error::InferenceError,
    ports::{StreamingChunk, StreamingResponse},
};

/// Ollama streaming response chunk
#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    model: String,
    message: OllamaStreamMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamMessage {
    content: String,
}

/// Create a streaming response from an HTTP response
pub fn create_stream(response: Response) -> StreamingResponse {
    let byte_stream = response.bytes_stream();

    let chunk_stream = byte_stream
        .map(|result| match result {
            Ok(bytes) => parse_chunks(&bytes),
            Err(e) => vec![Err(InferenceError::StreamError(e.to_string()))],
        })
        .flat_map(stream::iter);

    Box::pin(chunk_stream)
}

/// Parse NDJSON chunks from bytes
fn parse_chunks(bytes: &[u8]) -> Vec<Result<StreamingChunk, InferenceError>> {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            return vec![Err(InferenceError::InvalidResponse(format!(
                "Invalid UTF-8: {e}"
            )))];
        },
    };

    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            trace!(line = %line, "Parsing stream chunk");

            let chunk: OllamaStreamChunk = serde_json::from_str(line)
                .map_err(|e| InferenceError::InvalidResponse(format!("JSON parse error: {e}")))?;

            Ok(StreamingChunk {
                content: chunk.message.content,
                done: chunk.done,
                model: if chunk.done { Some(chunk.model) } else { None },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_chunk() {
        let json =
            r#"{"model":"qwen2.5-1.5b-instruct","message":{"content":"Hello"},"done":false}"#;
        let chunks = parse_chunks(json.as_bytes());

        assert_eq!(chunks.len(), 1);
        let chunk = chunks[0].as_ref().unwrap();
        assert_eq!(chunk.content, "Hello");
        assert!(!chunk.done);
    }

    #[test]
    fn parses_multiple_chunks() {
        let json = r#"{"model":"qwen2.5-1.5b-instruct","message":{"content":"Hello"},"done":false}
{"model":"qwen2.5-1.5b-instruct","message":{"content":" world"},"done":false}
{"model":"qwen2.5-1.5b-instruct","message":{"content":"!"},"done":true}"#;

        let chunks = parse_chunks(json.as_bytes());

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].as_ref().unwrap().content, "Hello");
        assert_eq!(chunks[1].as_ref().unwrap().content, " world");
        assert!(chunks[2].as_ref().unwrap().done);
    }

    #[test]
    fn final_chunk_includes_model() {
        let json = r#"{"model":"qwen2.5-1.5b-instruct","message":{"content":""},"done":true}"#;
        let chunks = parse_chunks(json.as_bytes());

        let chunk = chunks[0].as_ref().unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.model.as_deref(), Some("qwen2.5-1.5b-instruct"));
    }
}
