#![forbid(unsafe_code)]
//! CardDAV integration
//!
//! Client for CardDAV servers (Baïkal, Radicale, Nextcloud).
//! Supports contact management via vCard 3.0 (RFC 2426).

pub mod client;
pub mod contact;

pub use client::{CardDavClient, CardDavConfig, CardDavError, HttpCardDavClient};
pub use contact::{Contact, ContactAddress, ContactEmail, ContactPhone};
