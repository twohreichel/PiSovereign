//! Domain entities - Objects with identity and lifecycle

mod chat_message;
mod conversation;

pub use chat_message::{ChatMessage, MessageRole};
pub use conversation::Conversation;
