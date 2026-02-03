//! Domain entities - Objects with identity and lifecycle

mod chat_message;
mod conversation;

pub use chat_message::{ChatMessage, MessageMetadata, MessageRole};
pub use conversation::Conversation;
