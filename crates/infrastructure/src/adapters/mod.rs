//! Infrastructure adapters
//!
//! Adapters connect application ports to concrete implementations.

mod caldav_calendar_adapter;
mod hailo_inference_adapter;
mod proton_email_adapter;

pub use caldav_calendar_adapter::CalDavCalendarAdapter;
pub use hailo_inference_adapter::HailoInferenceAdapter;
pub use proton_email_adapter::ProtonEmailAdapter;
