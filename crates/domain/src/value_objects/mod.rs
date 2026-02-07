//! Value Objects - Immutable, identity-less domain primitives

mod approval_id;
mod conversation_id;
mod draft_id;
mod email_address;
mod geo_location;
mod humidity;
mod phone_number;
mod priority;
pub mod tenant;
mod tenant_id;
mod task_status;
mod timezone;
mod user_id;

pub use approval_id::ApprovalId;
pub use conversation_id::ConversationId;
pub use draft_id::DraftId;
pub use email_address::EmailAddress;
pub use geo_location::{GeoLocation, InvalidCoordinates};
pub use humidity::{Humidity, InvalidHumidity};
pub use phone_number::PhoneNumber;
pub use priority::Priority;
pub use task_status::TaskStatus;
pub use tenant::{TenantAware, TenantContext, TenantFilter};
pub use tenant_id::TenantId;
pub use timezone::{InvalidTimezone, Timezone};
pub use user_id::UserId;
