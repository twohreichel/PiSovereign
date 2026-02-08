//! Circuit breaker pattern for external service calls
//!
//! Implements the circuit breaker pattern to prevent cascading failures
//! when external services are unavailable.
//!
//! # States
//!
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Service is down, requests fail fast without calling the service
//! - **Half-Open**: Testing if the service has recovered
//!
//! # Persistence
//!
//! Circuit breaker state can be persisted to a file so that the state
//! survives application restarts. Use `CircuitBreaker::with_persistence()`
//! to enable this feature.
//!
//! # Example
//!
//! ```rust,ignore
//! use infrastructure::adapters::CircuitBreaker;
//!
//! let cb = CircuitBreaker::new("email-service");
//! let result = cb.call(|| async {
//!     external_service.call().await
//! }).await;
//! ```

use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Configuration for a circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Number of consecutive successes to close the circuit
    pub success_threshold: u32,
    /// Time in seconds to wait before transitioning from Open to Half-Open
    pub half_open_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            half_open_timeout_secs: 30,
        }
    }
}

impl CircuitBreakerConfig {
    /// Creates a configuration for a sensitive/critical service (lower thresholds)
    #[must_use]
    pub const fn sensitive() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 1,
            half_open_timeout_secs: 10,
        }
    }

    /// Creates a configuration for a resilient service (higher thresholds)
    #[must_use]
    pub const fn resilient() -> Self {
        Self {
            failure_threshold: 10,
            success_threshold: 3,
            half_open_timeout_secs: 60,
        }
    }

    /// Creates a custom configuration
    #[must_use]
    pub const fn custom(
        failure_threshold: u32,
        success_threshold: u32,
        half_open_timeout_secs: u64,
    ) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            half_open_timeout_secs,
        }
    }
}

/// State of a circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, requests pass through
    Closed,
    /// Service is down, requests fail fast
    Open,
    /// Testing if the service has recovered
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
            Self::Open => write!(f, "open"),
            Self::HalfOpen => write!(f, "half-open"),
        }
    }
}

/// Serializable state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedCircuitState {
    /// Name of the circuit breaker
    name: String,
    /// Current state
    state: String,
    /// Number of consecutive failures
    failure_count: u32,
    /// Number of consecutive successes
    success_count: u32,
    /// When the circuit was opened (Unix timestamp in seconds)
    opened_at_secs: Option<u64>,
}

impl PersistedCircuitState {
    /// Convert internal state to persistable format
    fn from_internal(name: &str, state: &CircuitBreakerState) -> Self {
        let opened_at_secs = state.opened_at_system.and_then(|t| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        });

        Self {
            name: name.to_string(),
            state: match state.state {
                CircuitState::Closed => "closed".to_string(),
                CircuitState::Open => "open".to_string(),
                CircuitState::HalfOpen => "half_open".to_string(),
            },
            failure_count: state.failure_count,
            success_count: state.success_count,
            opened_at_secs,
        }
    }

    /// Convert back to internal state
    fn to_internal(&self) -> CircuitBreakerState {
        let state = match self.state.as_str() {
            "open" => CircuitState::Open,
            "half_open" => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        };

        let opened_at_system = self
            .opened_at_secs
            .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs));

        // Calculate Instant from SystemTime if we have an opened_at
        let opened_at = opened_at_system.and_then(|system_time| {
            // Convert SystemTime to Instant by calculating elapsed time
            system_time.elapsed().ok().map(|elapsed| {
                // Instant::now() - elapsed gives us when it was opened
                Instant::now()
                    .checked_sub(elapsed)
                    .unwrap_or_else(Instant::now)
            })
        });

        CircuitBreakerState {
            state,
            failure_count: self.failure_count,
            success_count: self.success_count,
            opened_at,
            opened_at_system,
        }
    }
}

/// Error returned when the circuit is open
#[derive(Debug, Clone)]
pub struct CircuitOpenError {
    /// Name of the service
    pub service_name: String,
}

impl std::error::Error for CircuitOpenError {}

impl fmt::Display for CircuitOpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Circuit breaker open for service '{}': service is temporarily unavailable",
            self.service_name
        )
    }
}

/// Internal state tracking
struct CircuitBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    opened_at: Option<Instant>,
    /// SystemTime version for persistence (None when closed)
    opened_at_system: Option<SystemTime>,
}

/// Circuit breaker wrapper for external service calls
///
/// Wraps any async operation with circuit breaker protection,
/// preventing cascading failures when services are unavailable.
pub struct CircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: RwLock<CircuitBreakerState>,
    /// Optional path to persist state
    persistence_path: Option<PathBuf>,
}

impl fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("name", &self.name)
            .field("state", &self.state())
            .field("persistence", &self.persistence_path)
            .finish_non_exhaustive()
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        let state = self.state.read();
        Self {
            name: self.name.clone(),
            config: self.config.clone(),
            persistence_path: self.persistence_path.clone(),
            state: RwLock::new(CircuitBreakerState {
                state: state.state,
                failure_count: state.failure_count,
                success_count: state.success_count,
                opened_at: state.opened_at,
                opened_at_system: state.opened_at_system,
            }),
        }
    }
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with default configuration
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_config(name, CircuitBreakerConfig::default())
    }

    /// Creates a new circuit breaker with custom configuration
    #[must_use]
    pub fn with_config(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            persistence_path: None,
            state: RwLock::new(CircuitBreakerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                opened_at: None,
                opened_at_system: None,
            }),
        }
    }

    /// Creates a new circuit breaker with state persistence
    ///
    /// The state will be loaded from the file if it exists, and saved
    /// on every state transition.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the circuit breaker
    /// * `config` - Circuit breaker configuration
    /// * `path` - Path to the state file
    #[must_use]
    pub fn with_persistence(
        name: impl Into<String>,
        config: CircuitBreakerConfig,
        path: impl AsRef<Path>,
    ) -> Self {
        let name = name.into();
        let path = path.as_ref().to_path_buf();

        // Try to load existing state
        let initial_state = Self::load_state(&path, &name).unwrap_or_else(|e| {
            tracing::debug!(
                circuit = %name,
                error = %e,
                "No existing state found, starting fresh"
            );
            CircuitBreakerState {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                opened_at: None,
                opened_at_system: None,
            }
        });

        tracing::info!(
            circuit = %name,
            state = %initial_state.state,
            "Loaded circuit breaker state"
        );

        Self {
            name,
            config,
            persistence_path: Some(path),
            state: RwLock::new(initial_state),
        }
    }

    /// Load state from file
    fn load_state(path: &Path, name: &str) -> Result<CircuitBreakerState, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let persisted: PersistedCircuitState = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Verify the name matches
        if persisted.name != name {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "State file is for circuit '{}', expected '{}'",
                    persisted.name, name
                ),
            ));
        }

        Ok(persisted.to_internal())
    }

    /// Save state to file
    fn save_state(&self) {
        if let Some(ref path) = self.persistence_path {
            let state = self.state.read();
            let persisted = PersistedCircuitState::from_internal(&self.name, &state);

            match serde_json::to_string_pretty(&persisted) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(path, json) {
                        tracing::error!(
                            circuit = %self.name,
                            path = %path.display(),
                            error = %e,
                            "Failed to persist circuit breaker state"
                        );
                    } else {
                        tracing::debug!(
                            circuit = %self.name,
                            state = %state.state,
                            "Persisted circuit breaker state"
                        );
                    }
                },
                Err(e) => {
                    tracing::error!(
                        circuit = %self.name,
                        error = %e,
                        "Failed to serialize circuit breaker state"
                    );
                },
            }
        }
    }

    /// Returns the name of this circuit breaker
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the current state of the circuit breaker
    #[must_use]
    pub fn state(&self) -> CircuitState {
        let mut state = self.state.write();

        // Check if we should transition from Open to HalfOpen
        if state.state == CircuitState::Open {
            if let Some(opened_at) = state.opened_at {
                let elapsed = opened_at.elapsed();
                if elapsed >= Duration::from_secs(self.config.half_open_timeout_secs) {
                    tracing::debug!(
                        service = %self.name,
                        elapsed_secs = elapsed.as_secs(),
                        "Circuit transitioning from Open to HalfOpen"
                    );
                    state.state = CircuitState::HalfOpen;
                    state.success_count = 0;
                }
            }
        }

        state.state
    }

    /// Returns true if the circuit is closed (normal operation)
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.state() == CircuitState::Closed
    }

    /// Returns true if the circuit is open (service unavailable)
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.state() == CircuitState::Open
    }

    /// Records a successful call
    fn on_success(&self) {
        let mut state_changed = false;
        {
            let mut state = self.state.write();
            state.failure_count = 0;

            match state.state {
                CircuitState::HalfOpen => {
                    state.success_count += 1;
                    if state.success_count >= self.config.success_threshold {
                        tracing::info!(
                            service = %self.name,
                            successes = state.success_count,
                            "Circuit transitioning from HalfOpen to Closed"
                        );
                        state.state = CircuitState::Closed;
                        state.success_count = 0;
                        state.opened_at = None;
                        state.opened_at_system = None;
                        state_changed = true;
                    }
                },
                CircuitState::Closed | CircuitState::Open => {},
            }
        }

        if state_changed {
            self.save_state();
        }
    }

    /// Records a failed call
    fn on_failure(&self) {
        let mut state_changed = false;
        {
            let mut state = self.state.write();
            state.failure_count += 1;
            state.success_count = 0;

            match state.state {
                CircuitState::Closed => {
                    if state.failure_count >= self.config.failure_threshold {
                        tracing::warn!(
                            service = %self.name,
                            failures = state.failure_count,
                            "Circuit transitioning from Closed to Open"
                        );
                        state.state = CircuitState::Open;
                        state.opened_at = Some(Instant::now());
                        state.opened_at_system = Some(SystemTime::now());
                        state.failure_count = 0;
                        state_changed = true;
                    }
                },
                CircuitState::HalfOpen => {
                    tracing::warn!(
                        service = %self.name,
                        "Circuit transitioning from HalfOpen to Open after failure"
                    );
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    state.opened_at_system = Some(SystemTime::now());
                    state.failure_count = 0;
                    state_changed = true;
                },
                CircuitState::Open => {},
            }
        }

        if state_changed {
            self.save_state();
        }
    }

    /// Calls an async operation through the circuit breaker
    ///
    /// If the circuit is open, returns `CircuitOpenError` immediately.
    /// Otherwise, executes the operation and tracks its success/failure.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The circuit is open (`CircuitOpenError`)
    /// - The inner operation fails (the original error)
    pub async fn call<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        use tracing::{debug, warn};

        // Check if circuit is open
        let current_state = self.state();
        if current_state == CircuitState::Open {
            warn!(
                service = %self.name,
                state = %current_state,
                "Circuit breaker preventing call to service"
            );
            return Err(CircuitBreakerError::CircuitOpen(CircuitOpenError {
                service_name: self.name.clone(),
            }));
        }

        debug!(
            service = %self.name,
            state = %current_state,
            "Calling service through circuit breaker"
        );

        // Execute the operation
        match f().await {
            Ok(result) => {
                debug!(service = %self.name, "Service call succeeded");
                self.on_success();
                Ok(result)
            },
            Err(e) => {
                warn!(service = %self.name, error = ?e, "Service call failed");
                self.on_failure();
                Err(CircuitBreakerError::ServiceError(e))
            },
        }
    }

    /// Calls a synchronous operation through the circuit breaker
    ///
    /// This is useful for operations that don't need async, but still
    /// benefit from circuit breaker protection.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The circuit is open (`CircuitOpenError`)
    /// - The inner operation fails (the original error)
    #[allow(clippy::cognitive_complexity)]
    pub fn call_sync<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
        E: std::fmt::Debug,
    {
        use tracing::{debug, warn};

        // Check if circuit is open
        let current_state = self.state();
        if current_state == CircuitState::Open {
            warn!(
                service = %self.name,
                state = %current_state,
                "Circuit breaker preventing call to service"
            );
            return Err(CircuitBreakerError::CircuitOpen(CircuitOpenError {
                service_name: self.name.clone(),
            }));
        }

        debug!(
            service = %self.name,
            state = %current_state,
            "Calling service through circuit breaker"
        );

        match f() {
            Ok(result) => {
                debug!(service = %self.name, "Service call succeeded");
                self.on_success();
                Ok(result)
            },
            Err(e) => {
                warn!(service = %self.name, error = ?e, "Service call failed");
                self.on_failure();
                Err(CircuitBreakerError::ServiceError(e))
            },
        }
    }
}

/// Error type for circuit breaker operations
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// The circuit is open, preventing the call
    CircuitOpen(CircuitOpenError),
    /// The underlying service returned an error
    ServiceError(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CircuitOpen(e) => write!(f, "{e}"),
            Self::ServiceError(e) => write!(f, "{e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitBreakerError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CircuitOpen(e) => Some(e),
            Self::ServiceError(e) => Some(e),
        }
    }
}

impl<E> CircuitBreakerError<E> {
    /// Returns true if this is a circuit open error
    #[must_use]
    pub const fn is_circuit_open(&self) -> bool {
        matches!(self, Self::CircuitOpen(_))
    }

    /// Returns true if this is a service error
    #[must_use]
    pub const fn is_service_error(&self) -> bool {
        matches!(self, Self::ServiceError(_))
    }

    /// Converts the inner service error if present
    #[must_use]
    pub fn into_service_error(self) -> Option<E> {
        match self {
            Self::ServiceError(e) => Some(e),
            Self::CircuitOpen(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_breaker_creation() {
        let cb = CircuitBreaker::new("test-service");
        assert_eq!(cb.name(), "test-service");
        assert!(cb.is_closed());
    }

    #[test]
    fn circuit_breaker_debug() {
        let cb = CircuitBreaker::new("test");
        let debug = format!("{cb:?}");
        assert!(debug.contains("CircuitBreaker"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn circuit_breaker_clone() {
        let cb1 = CircuitBreaker::new("test");
        #[allow(clippy::redundant_clone)]
        let cb2 = cb1.clone();
        assert_eq!(cb1.name(), cb2.name());
    }

    #[test]
    fn circuit_state_display() {
        assert_eq!(format!("{}", CircuitState::Closed), "closed");
        assert_eq!(format!("{}", CircuitState::Open), "open");
        assert_eq!(format!("{}", CircuitState::HalfOpen), "half-open");
    }

    #[test]
    fn circuit_open_error_display() {
        let err = CircuitOpenError {
            service_name: "my-service".to_string(),
        };
        assert!(err.to_string().contains("my-service"));
        assert!(err.to_string().contains("temporarily unavailable"));
    }

    #[test]
    fn config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.half_open_timeout_secs, 30);
    }

    #[test]
    fn config_sensitive() {
        let config = CircuitBreakerConfig::sensitive();
        assert_eq!(config.failure_threshold, 3);
    }

    #[test]
    fn config_resilient() {
        let config = CircuitBreakerConfig::resilient();
        assert_eq!(config.failure_threshold, 10);
    }

    #[test]
    fn config_custom() {
        let config = CircuitBreakerConfig::custom(7, 4, 45);
        assert_eq!(config.failure_threshold, 7);
        assert_eq!(config.success_threshold, 4);
        assert_eq!(config.half_open_timeout_secs, 45);
    }

    #[tokio::test]
    async fn call_succeeds_when_closed() {
        let cb = CircuitBreaker::new("test");

        let result = cb
            .call(|| async { Ok::<_, std::io::Error>("success") })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn call_returns_service_error() {
        let cb = CircuitBreaker::new("test");

        let result = cb
            .call(|| async { Err::<(), _>(std::io::Error::other("test error")) })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_service_error());
    }

    #[tokio::test]
    async fn circuit_opens_after_failures() {
        let cb = CircuitBreaker::with_config("test", CircuitBreakerConfig::custom(3, 1, 1));

        // Generate enough failures to open the circuit
        for _ in 0..3 {
            let _ = cb
                .call(|| async { Err::<(), _>(std::io::Error::other("fail")) })
                .await;
        }

        // Circuit should now be open
        assert!(cb.is_open());
    }

    #[test]
    fn circuit_breaker_error_is_circuit_open() {
        let err: CircuitBreakerError<std::io::Error> =
            CircuitBreakerError::CircuitOpen(CircuitOpenError {
                service_name: "test".to_string(),
            });
        assert!(err.is_circuit_open());
        assert!(!err.is_service_error());
    }

    #[test]
    fn circuit_breaker_error_is_service_error() {
        let err: CircuitBreakerError<std::io::Error> =
            CircuitBreakerError::ServiceError(std::io::Error::other("test"));
        assert!(err.is_service_error());
        assert!(!err.is_circuit_open());
    }

    #[test]
    fn circuit_breaker_error_into_service_error() {
        let err: CircuitBreakerError<String> =
            CircuitBreakerError::ServiceError("test".to_string());
        assert_eq!(err.into_service_error(), Some("test".to_string()));

        let err: CircuitBreakerError<String> = CircuitBreakerError::CircuitOpen(CircuitOpenError {
            service_name: "test".to_string(),
        });
        assert_eq!(err.into_service_error(), None);
    }

    #[test]
    fn initial_state_is_closed() {
        let cb = CircuitBreaker::new("test");
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn call_sync_succeeds() {
        let cb = CircuitBreaker::new("test");
        let result: Result<&str, CircuitBreakerError<std::io::Error>> =
            cb.call_sync(|| Ok("success"));
        assert!(result.is_ok());
    }

    #[test]
    fn call_sync_returns_service_error() {
        let cb = CircuitBreaker::new("test");
        let result: Result<(), CircuitBreakerError<std::io::Error>> =
            cb.call_sync(|| Err(std::io::Error::other("test")));
        assert!(result.is_err());
        assert!(result.unwrap_err().is_service_error());
    }

    #[test]
    fn persistence_saves_and_loads_state() {
        // Create a temp file
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cb_test_{}.json", std::process::id()));

        // Create circuit breaker with persistence
        let cb = CircuitBreaker::with_persistence(
            "persistent-test",
            CircuitBreakerConfig::custom(2, 1, 30),
            &path,
        );

        // Initially closed
        assert!(cb.is_closed());

        // Trigger failures to open the circuit
        let _: Result<(), _> = cb.call_sync(|| Err::<(), _>("fail1"));
        let _: Result<(), _> = cb.call_sync(|| Err::<(), _>("fail2"));

        // Circuit should be open
        assert!(cb.is_open());

        // State file should exist
        assert!(path.exists(), "State file should be created");

        // Read and verify state file content
        let content = std::fs::read_to_string(&path).expect("read state file");
        let persisted: PersistedCircuitState = serde_json::from_str(&content).expect("parse state");
        assert_eq!(persisted.name, "persistent-test");
        assert_eq!(persisted.state, "open");
        assert!(persisted.opened_at_secs.is_some());

        // Create a new circuit breaker from the same file
        drop(cb);
        let cb2 = CircuitBreaker::with_persistence(
            "persistent-test",
            CircuitBreakerConfig::custom(2, 1, 30),
            &path,
        );

        // Should restore the open state
        assert!(cb2.is_open());

        // Clean up
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn persistence_clears_state_on_recovery() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cb_test_recovery_{}.json", std::process::id()));

        let cb = CircuitBreaker::with_persistence(
            "recovery-test",
            CircuitBreakerConfig::custom(2, 1, 1), // 1 second timeout
            &path,
        );

        // Open the circuit
        let _: Result<(), _> = cb.call_sync(|| Err::<(), _>("fail1"));
        let _: Result<(), _> = cb.call_sync(|| Err::<(), _>("fail2"));
        assert!(cb.is_open());

        // Wait for half-open timeout
        std::thread::sleep(Duration::from_secs(2));

        // Check state (should transition to half-open)
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Successful call should close it
        let _: Result<&str, _> = cb.call_sync(|| Ok::<_, &str>("success"));
        assert!(cb.is_closed());

        // Verify state was persisted as closed
        let content = std::fs::read_to_string(&path).expect("read state file");
        let persisted: PersistedCircuitState = serde_json::from_str(&content).expect("parse state");
        assert_eq!(persisted.state, "closed");
        assert!(persisted.opened_at_secs.is_none());

        std::fs::remove_file(&path).ok();
    }
}
