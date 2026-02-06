//! HTTP utilities and clients with correlation support
//!
//! This module provides HTTP client utilities that automatically propagate
//! request correlation IDs (X-Request-Id) through outgoing requests.

mod correlated_client;

pub use correlated_client::{
    CorrelatedClientConfig, CorrelatedHttpClient, CorrelatedRequestBuilder, RequestBuilderExt,
    RequestIdProvider, SharedCorrelatedClient, X_REQUEST_ID, create_shared_client,
    create_shared_client_with_config,
};
