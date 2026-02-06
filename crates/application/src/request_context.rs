//! Request context for propagating authentication and request metadata
//!
//! This module provides a `RequestContext` struct that carries user identity
//! and request metadata through the application layer. It should be extracted
//! from the HTTP middleware (e.g., from API key authentication) and passed
//! to service methods that require user context.
//!
//! # Examples
//!
//! ```
//! use application::RequestContext;
//! use domain::UserId;
//!
//! // Create a request context for an authenticated user
//! let user_id = UserId::new();
//! let ctx = RequestContext::new(user_id);
//!
//! assert_eq!(ctx.user_id(), user_id);
//! assert!(!ctx.request_id().is_nil());
//! ```

use chrono::{DateTime, Utc};
use domain::UserId;
use uuid::Uuid;

/// Context for a single request, carrying authentication and metadata
///
/// `RequestContext` is created by the HTTP middleware after successful
/// authentication and passed through to application services. It provides:
///
/// - `user_id`: The authenticated user making the request
/// - `request_id`: A unique identifier for tracing/logging
/// - `timestamp`: When the request was received
///
/// # Examples
///
/// ```
/// use application::RequestContext;
/// use domain::UserId;
///
/// let ctx = RequestContext::new(UserId::new());
/// println!("Request {} from user {}", ctx.request_id(), ctx.user_id());
/// ```
#[derive(Debug, Clone)]
pub struct RequestContext {
    user_id: UserId,
    request_id: Uuid,
    timestamp: DateTime<Utc>,
}

impl RequestContext {
    /// Create a new request context for the given user
    ///
    /// Generates a new random request ID and captures the current timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::RequestContext;
    /// use domain::UserId;
    ///
    /// let ctx = RequestContext::new(UserId::new());
    /// assert!(!ctx.request_id().is_nil());
    /// ```
    #[must_use]
    pub fn new(user_id: UserId) -> Self {
        Self {
            user_id,
            request_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        }
    }

    /// Create a request context with a specific request ID
    ///
    /// Useful when the request ID is provided by an upstream service
    /// or needs to be correlated with external systems.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::RequestContext;
    /// use domain::UserId;
    /// use uuid::Uuid;
    ///
    /// let request_id = Uuid::new_v4();
    /// let ctx = RequestContext::with_request_id(UserId::new(), request_id);
    /// assert_eq!(ctx.request_id(), request_id);
    /// ```
    #[must_use]
    pub fn with_request_id(user_id: UserId, request_id: Uuid) -> Self {
        Self {
            user_id,
            request_id,
            timestamp: Utc::now(),
        }
    }

    /// Create a request context with all fields specified
    ///
    /// Primarily used for testing or when restoring context from storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use application::RequestContext;
    /// use domain::UserId;
    /// use uuid::Uuid;
    /// use chrono::Utc;
    ///
    /// let ctx = RequestContext::restore(
    ///     UserId::new(),
    ///     Uuid::new_v4(),
    ///     Utc::now(),
    /// );
    /// ```
    #[must_use]
    pub fn restore(user_id: UserId, request_id: Uuid, timestamp: DateTime<Utc>) -> Self {
        Self {
            user_id,
            request_id,
            timestamp,
        }
    }

    /// Get the authenticated user ID
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Get the unique request identifier
    #[must_use]
    pub const fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Get the timestamp when the request was received
    #[must_use]
    pub const fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_unique_request_id() {
        let user_id = UserId::new();
        let ctx1 = RequestContext::new(user_id);
        let ctx2 = RequestContext::new(user_id);

        assert_ne!(ctx1.request_id(), ctx2.request_id());
    }

    #[test]
    fn new_captures_current_timestamp() {
        let before = Utc::now();
        let ctx = RequestContext::new(UserId::new());
        let after = Utc::now();

        assert!(ctx.timestamp() >= before);
        assert!(ctx.timestamp() <= after);
    }

    #[test]
    fn with_request_id_uses_provided_id() {
        let user_id = UserId::new();
        let request_id = Uuid::new_v4();
        let ctx = RequestContext::with_request_id(user_id, request_id);

        assert_eq!(ctx.request_id(), request_id);
        assert_eq!(ctx.user_id(), user_id);
    }

    #[test]
    fn restore_preserves_all_fields() {
        let user_id = UserId::new();
        let request_id = Uuid::new_v4();
        let timestamp = Utc::now() - chrono::Duration::hours(1);

        let ctx = RequestContext::restore(user_id, request_id, timestamp);

        assert_eq!(ctx.user_id(), user_id);
        assert_eq!(ctx.request_id(), request_id);
        assert_eq!(ctx.timestamp(), timestamp);
    }

    #[test]
    fn user_id_getter_returns_correct_value() {
        let user_id = UserId::new();
        let ctx = RequestContext::new(user_id);

        assert_eq!(ctx.user_id(), user_id);
    }

    #[test]
    fn request_id_is_not_nil() {
        let ctx = RequestContext::new(UserId::new());

        assert!(!ctx.request_id().is_nil());
    }

    #[test]
    fn clone_produces_equal_context() {
        let ctx = RequestContext::new(UserId::new());
        #[allow(clippy::redundant_clone)]
        let cloned = ctx.clone();

        assert_eq!(ctx.user_id(), cloned.user_id());
        assert_eq!(ctx.request_id(), cloned.request_id());
        assert_eq!(ctx.timestamp(), cloned.timestamp());
    }

    #[test]
    fn debug_format_contains_fields() {
        let ctx = RequestContext::new(UserId::new());
        let debug = format!("{ctx:?}");

        assert!(debug.contains("RequestContext"));
        assert!(debug.contains("user_id"));
        assert!(debug.contains("request_id"));
        assert!(debug.contains("timestamp"));
    }
}
