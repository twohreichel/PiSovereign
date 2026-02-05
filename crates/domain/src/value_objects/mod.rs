//! Value Objects - Immutable, identity-less domain primitives

mod approval_id;
mod conversation_id;
mod draft_id;
mod email_address;
mod geo_location;
mod phone_number;
mod priority;
mod timezone;
mod user_id;

pub use approval_id::ApprovalId;
pub use conversation_id::ConversationId;
pub use draft_id::DraftId;
pub use email_address::EmailAddress;
pub use geo_location::{GeoLocation, InvalidCoordinates};
pub use phone_number::PhoneNumber;
pub use priority::Priority;
pub use timezone::Timezone;
pub use user_id::UserId;
