//! CalDAV integration
//!
//! Client for CalDAV servers (Baïkal, Radicale, Nextcloud).
//! Supports calendar events (VEVENT) and tasks (VTODO).

pub mod client;
pub mod task;

pub use client::{CalDavClient, CalDavConfig, CalDavError, CalendarEvent, HttpCalDavClient};
pub use task::{CalDavTaskClient, CalendarTask, TaskPriority, TaskStatus};
