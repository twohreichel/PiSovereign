//! Streaming response handling for Ollama-compatible servers

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

    #[test]
    fn non_final_chunk_has_no_model() {
        let json = r#"{"model":"qwen2.5-1.5b-instruct","message":{"content":"Hi"},"done":false}"#;
        let chunks = parse_chunks(json.as_bytes());

        let chunk = chunks[0].as_ref().unwrap();
        assert!(!chunk.done);
        assert!(chunk.model.is_none());
    }

    #[test]
    fn handles_invalid_utf8() {
        let invalid_bytes = &[0xff, 0xfe, 0x00];
        let chunks = parse_chunks(invalid_bytes);

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_err());
    }

    #[test]
    fn handles_invalid_json() {
        let invalid_json = b"not valid json";
        let chunks = parse_chunks(invalid_json);

        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].is_err());
    }

    #[test]
    fn handles_empty_lines() {
        let json = r#"{"model":"qwen","message":{"content":"Hi"},"done":false}

{"model":"qwen","message":{"content":"!"},"done":true}"#;
        let chunks = parse_chunks(json.as_bytes());

        // Empty lines are filtered out
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn handles_empty_input() {
        let chunks = parse_chunks(b"");
        assert!(chunks.is_empty());
    }

    #[test]
    fn ollama_stream_chunk_deserializes() {
        let json = r#"{"model":"test","message":{"content":"hello"},"done":false}"#;
        let chunk: OllamaStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.model, "test");
        assert_eq!(chunk.message.content, "hello");
        assert!(!chunk.done);
    }
}
