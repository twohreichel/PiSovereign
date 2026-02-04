//! CalDAV integration
//!
//! Client for CalDAV servers (Baïkal, Radicale, Nextcloud).

pub mod client;

pub use client::{CalDavClient, CalDavConfig, CalDavError, CalendarEvent, HttpCalDavClient};
