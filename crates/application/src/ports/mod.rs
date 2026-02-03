//! Port definitions for application layer
//!
//! Ports are interfaces that define how the application interacts with
//! external systems. Adapters in the infrastructure layer implement these ports.

mod inference_port;
mod message_gateway_port;

pub use inference_port::{InferencePort, InferenceResult};
pub use message_gateway_port::{IncomingMessage, MessageGatewayPort, OutgoingMessage};
