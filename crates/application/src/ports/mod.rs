//! Port definitions for application layer
//!
//! Ports are interfaces that define how the application interacts with
//! external systems. Adapters in the infrastructure layer implement these ports.

mod approval_queue;
mod audit_log;
mod calendar_port;
mod conversation_store;
mod email_port;
mod inference_port;
mod message_gateway_port;
mod secret_store;

pub use approval_queue::ApprovalQueuePort;
pub use audit_log::{AuditLogPort, AuditQuery};
pub use calendar_port::{CalendarError, CalendarEvent, CalendarInfo, CalendarPort, NewEvent};
pub use conversation_store::ConversationStore;
pub use email_port::{EmailDraft, EmailError, EmailPort, EmailSummary};
pub use inference_port::{InferencePort, InferenceResult, InferenceStream, StreamingChunk};
pub use message_gateway_port::{IncomingMessage, MessageGatewayPort, OutgoingMessage};
pub use secret_store::{SecretStoreExt, SecretStorePort};
