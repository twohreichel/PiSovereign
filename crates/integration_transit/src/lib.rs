//! Public transit integration for PiSovereign
//!
//! Provides public transit routing via the [transport.rest](https://v6.db.transport.rest) API
//! (HAFAS-based, covering all German public transit) and address geocoding via
//! [Nominatim/OpenStreetMap](https://nominatim.openstreetmap.org).
//!
//! # Architecture
//!
//! The crate follows a client-trait pattern consistent with other integration crates.
//! [`TransitClient`] defines the interface for journey planning and stop search,
//! implemented by [`HafasTransitClient`]. [`GeocodingClient`] handles address-to-coordinate
//! conversion via [`NominatimGeocodingClient`].
//!
//! # Example
//!
//! ```rust,ignore
//! use integration_transit::{HafasTransitClient, TransitConfig};
//!
//! let config = TransitConfig::default();
//! let client = HafasTransitClient::new(&config)?;
//!
//! let journeys = client.search_journeys(
//!     52.520, 13.405, // Berlin origin
//!     52.525, 13.369, // Berlin destination
//!     None,           // depart now
//!     3,              // max results
//! ).await?;
//! ```

mod client;
mod config;
mod error;
mod geocoding;
mod models;

pub use client::{HafasTransitClient, TransitClient};
pub use config::TransitConfig;
pub use error::TransitError;
pub use geocoding::{GeocodingClient, GeocodingError, NominatimConfig, NominatimGeocodingClient};
pub use models::{Journey, Leg, LineInfo, Stop, TransitMode, TransitResponse};
