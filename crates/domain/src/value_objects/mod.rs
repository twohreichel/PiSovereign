//! Value Objects - Immutable, identity-less domain primitives

mod approval_id;
mod conversation_id;
mod email_address;
mod phone_number;
mod user_id;

pub use approval_id::ApprovalId;
pub use conversation_id::ConversationId;
pub use email_address::EmailAddress;
pub use phone_number::PhoneNumber;
pub use user_id::UserId;
