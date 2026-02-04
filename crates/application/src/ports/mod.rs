//! Port definitions for application layer
//!
//! Ports are interfaces that define how the application interacts with
//! external systems. Adapters in the infrastructure layer implement these ports.

mod approval_queue;
mod audit_log;
mod conversation_store;
mod inference_port;
mod message_gateway_port;

pub use approval_queue::ApprovalQueuePort;
pub use audit_log::{AuditLogPort, AuditQuery};
pub use conversation_store::ConversationStore;
pub use inference_port::{InferencePort, InferenceResult};
pub use message_gateway_port::{IncomingMessage, MessageGatewayPort, OutgoingMessage};
