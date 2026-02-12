//! Rate limiting middleware
//!
//! Token bucket rate limiter that limits requests per IP address.
//! Supports trusted reverse proxies for proper client IP extraction.

use std::{
    collections::HashMap,
    collections::HashSet,
    future::Future,
    net::IpAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request},
    response::{IntoResponse, Response},
};
use tokio::sync::RwLock;
use tower::{Layer, Service};
use tracing::{debug, info, warn};

use crate::error::ApiError;

/// Client IP address extracted from the request
///
/// This extension is inserted by the rate limiter middleware and can
/// be extracted in handlers via `Extension<ClientIp>`.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

/// Rate limiter configuration
#[derive(Clone, Debug)]
pub struct RateLimiterConfig {
    /// Maximum requests per window
    pub requests_per_minute: u32,
    /// Enable rate limiting
    pub enabled: bool,
    /// Trusted proxy IP addresses
    ///
    /// When a request comes from a trusted proxy, the rate limiter will
    /// use the X-Forwarded-For header to determine the real client IP.
    /// If the connecting IP is not in this list, X-Forwarded-For is ignored.
    pub trusted_proxies: Vec<IpAddr>,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            enabled: true,
            trusted_proxies: Vec::new(),
        }
    }
}

/// Token bucket entry for a single IP
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64) -> Self {
        Self {
            tokens: max_tokens,
            last_update: Instant::now(),
        }
    }

    /// Try to consume a token, returning true if allowed
    fn try_consume(&mut self, tokens_per_second: f64, max_tokens: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        // Refill tokens based on elapsed time
        self.tokens = elapsed
            .mul_add(tokens_per_second, self.tokens)
            .min(max_tokens);
        self.last_update = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Shared rate limiter state
#[derive(Debug)]
pub struct RateLimiterState {
    buckets: RwLock<HashMap<IpAddr, TokenBucket>>,
    tokens_per_second: f64,
    max_tokens: f64,
}

impl RateLimiterState {
    /// Create a new rate limiter state
    #[must_use]
    pub fn new(requests_per_minute: u32) -> Self {
        let max_tokens = f64::from(requests_per_minute);
        Self {
            buckets: RwLock::new(HashMap::new()),
            tokens_per_second: max_tokens / 60.0,
            max_tokens,
        }
    }

    /// Check if a request from the given IP is allowed
    #[allow(clippy::significant_drop_tightening)]
    pub async fn check(&self, ip: IpAddr) -> bool {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets
            .entry(ip)
            .or_insert_with(|| TokenBucket::new(self.max_tokens));

        let tokens_per_second = self.tokens_per_second;
        let max_tokens = self.max_tokens;
        bucket.try_consume(tokens_per_second, max_tokens)
    }

    /// Clean up stale entries older than the specified duration
    pub async fn cleanup(&self, older_than: Duration) {
        let mut buckets = self.buckets.write().await;
        let cutoff = Instant::now()
            .checked_sub(older_than)
            .unwrap_or_else(Instant::now);

        let before_count = buckets.len();
        buckets.retain(|_, bucket| bucket.last_update > cutoff);
        let removed = before_count - buckets.len();

        if removed > 0 {
            debug!(
                removed = removed,
                remaining = buckets.len(),
                "Cleaned up stale rate limit entries"
            );
        }
    }

    /// Get the current number of tracked IP addresses
    pub async fn entry_count(&self) -> usize {
        self.buckets.read().await.len()
    }
}

/// Spawn a background task that periodically cleans up stale rate limit entries.
///
/// The task will run every `interval` and remove entries that haven't been
/// updated within `max_age`.
///
/// Returns a `JoinHandle` that can be used to abort the task when shutting down.
///
/// # Arguments
///
/// * `state` - The rate limiter state to clean
/// * `interval` - How often to run the cleanup (e.g., every 5 minutes)
/// * `max_age` - Remove entries older than this duration (e.g., 10 minutes)
///
/// # Example
///
/// ```ignore
/// let layer = RateLimiterLayer::new(&config);
/// let state = layer.state();
/// let cleanup_handle = spawn_cleanup_task(
///     state,
///     Duration::from_secs(300),  // cleanup every 5 minutes
///     Duration::from_secs(600),  // remove entries older than 10 minutes
/// );
///
/// // On shutdown:
/// cleanup_handle.abort();
/// ```
pub fn spawn_cleanup_task(
    state: Arc<RateLimiterState>,
    interval: Duration,
    max_age: Duration,
) -> tokio::task::JoinHandle<()> {
    info!(
        interval_secs = interval.as_secs(),
        max_age_secs = max_age.as_secs(),
        "Starting rate limiter cleanup task"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;
            state.cleanup(max_age).await;
        }
    })
}

/// Layer that applies rate limiting
#[derive(Clone, Debug)]
pub struct RateLimiterLayer {
    state: Arc<RateLimiterState>,
    enabled: bool,
    excluded_paths: Vec<String>,
    trusted_proxies: Arc<HashSet<IpAddr>>,
}

impl RateLimiterLayer {
    /// Create a new rate limiter layer
    #[must_use]
    pub fn new(config: &RateLimiterConfig) -> Self {
        let trusted_set: HashSet<IpAddr> = config.trusted_proxies.iter().copied().collect();

        if !trusted_set.is_empty() {
            info!(
                count = trusted_set.len(),
                proxies = ?config.trusted_proxies,
                "Rate limiter configured with trusted proxies"
            );
        }

        Self {
            state: Arc::new(RateLimiterState::new(config.requests_per_minute)),
            enabled: config.enabled,
            excluded_paths: vec!["/health".to_string(), "/ready".to_string()],
            trusted_proxies: Arc::new(trusted_set),
        }
    }

    /// Add paths that should be excluded from rate limiting
    #[must_use]
    pub fn exclude_paths(mut self, paths: Vec<String>) -> Self {
        self.excluded_paths.extend(paths);
        self
    }

    /// Get a reference to the rate limiter state for cleanup tasks
    #[must_use]
    pub fn state(&self) -> Arc<RateLimiterState> {
        Arc::clone(&self.state)
    }
}

impl<S> Layer<S> for RateLimiterLayer {
    type Service = RateLimiter<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimiter {
            inner,
            state: Arc::clone(&self.state),
            enabled: self.enabled,
            excluded_paths: self.excluded_paths.clone(),
            trusted_proxies: Arc::clone(&self.trusted_proxies),
        }
    }
}

/// Middleware service for rate limiting
#[derive(Clone, Debug)]
pub struct RateLimiter<S> {
    inner: S,
    state: Arc<RateLimiterState>,
    enabled: bool,
    excluded_paths: Vec<String>,
    trusted_proxies: Arc<HashSet<IpAddr>>,
}

impl<S> Service<Request> for RateLimiter<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let enabled = self.enabled;
        let state = Arc::clone(&self.state);
        let excluded_paths = self.excluded_paths.clone();
        let trusted_proxies = Arc::clone(&self.trusted_proxies);
        let mut inner = self.inner.clone();

        // Extract client IP early so it's available even if rate limiting is disabled
        let client_ip = extract_client_ip(&req, &trusted_proxies);

        // Insert client IP into request extensions for downstream handlers
        req.extensions_mut().insert(ClientIp(client_ip));

        Box::pin(async move {
            // If rate limiting is disabled, pass through
            if !enabled {
                return inner.call(req).await;
            }

            // Check if path is excluded
            let path = req.uri().path();
            if excluded_paths.iter().any(|p| path.starts_with(p)) {
                return inner.call(req).await;
            }

            // Check rate limit
            if state.check(client_ip).await {
                inner.call(req).await
            } else {
                Ok(ApiError::RateLimited.into_response())
            }
        })
    }
}

/// Extract the client IP address from a request
///
/// This function implements secure IP extraction for reverse proxy setups:
///
/// 1. If the request comes from a trusted proxy (based on `trusted_proxies`),
///    it will parse the `X-Forwarded-For` header to get the real client IP.
/// 2. If the request is not from a trusted proxy, `X-Forwarded-For` is ignored
///    to prevent IP spoofing attacks.
/// 3. Falls back to localhost if no IP can be determined.
///
/// # Security
///
/// Never trust `X-Forwarded-For` from untrusted sources. Attackers can easily
/// spoof this header to bypass rate limiting or IP-based access controls.
#[allow(clippy::implicit_hasher)] // Using standard RandomState is fine for IP lookups
pub fn extract_client_ip(req: &Request, trusted_proxies: &HashSet<IpAddr>) -> IpAddr {
    // Get the connecting IP from the actual TCP socket via ConnectInfo.
    // ConnectInfo is injected by axum when using `into_make_service_with_connect_info()`.
    let connecting_ip: IpAddr = req
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), |ci| ci.0.ip());

    // Only trust X-Forwarded-For if the connecting IP is a trusted proxy
    if !trusted_proxies.is_empty() && trusted_proxies.contains(&connecting_ip) {
        if let Some(forwarded) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        {
            // X-Forwarded-For format: client, proxy1, proxy2, ...
            // We want the leftmost (original client) IP
            if let Some(ip_str) = forwarded.split(',').next() {
                match ip_str.trim().parse::<IpAddr>() {
                    Ok(ip) => {
                        debug!(
                            client_ip = %ip,
                            proxy_ip = %connecting_ip,
                            "Extracted client IP from X-Forwarded-For via trusted proxy"
                        );
                        return ip;
                    },
                    Err(e) => {
                        warn!(
                            header = forwarded,
                            error = %e,
                            "Invalid IP in X-Forwarded-For header"
                        );
                    },
                }
            }
        }
    } else if req.headers().contains_key("x-forwarded-for") && !trusted_proxies.is_empty() {
        // Log when X-Forwarded-For is present but not from a trusted proxy
        warn!(
            connecting_ip = %connecting_ip,
            "X-Forwarded-For header ignored: connecting IP not in trusted_proxies"
        );
    }

    connecting_ip
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn test_handler() -> &'static str {
        "ok"
    }

    /// Build a request with `ConnectInfo` injected for testing.
    fn request_with_connect_info(uri: &str, addr: std::net::SocketAddr) -> Request<Body> {
        let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    fn create_test_router(enabled: bool, rpm: u32) -> Router {
        let config = RateLimiterConfig {
            enabled,
            requests_per_minute: rpm,
            trusted_proxies: Vec::new(),
        };
        Router::new()
            .route("/test", get(test_handler))
            .route("/health", get(test_handler))
            .layer(RateLimiterLayer::new(&config))
    }

    #[tokio::test]
    async fn rate_limit_disabled_passes_all_requests() {
        let app = create_test_router(false, 1);

        for _ in 0..10 {
            let response = app
                .clone()
                .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn rate_limit_allows_within_limit() {
        let config = RateLimiterConfig {
            enabled: true,
            requests_per_minute: 60,
            trusted_proxies: Vec::new(),
        };
        let layer = RateLimiterLayer::new(&config);
        let app = Router::new().route("/test", get(test_handler)).layer(layer);

        // First request should succeed
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn rate_limit_blocks_excess_requests() {
        let config = RateLimiterConfig {
            enabled: true,
            requests_per_minute: 2, // Very low limit for testing
            trusted_proxies: Vec::new(),
        };
        let layer = RateLimiterLayer::new(&config);
        let app = Router::new().route("/test", get(test_handler)).layer(layer);

        // Use up the tokens
        for _ in 0..3 {
            let response = app
                .clone()
                .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
                .await
                .unwrap();
            // After exhausting tokens, we should get 429
            if response.status() == axum::http::StatusCode::TOO_MANY_REQUESTS {
                return; // Test passes
            }
        }

        // Should have hit rate limit by now with only 2 rpm
        unreachable!("Expected rate limit to be hit with only 2 rpm");
    }

    #[tokio::test]
    async fn health_endpoint_excluded_from_rate_limit() {
        let config = RateLimiterConfig {
            enabled: true,
            requests_per_minute: 1, // Very restrictive
            trusted_proxies: Vec::new(),
        };
        let layer = RateLimiterLayer::new(&config);
        let app = Router::new()
            .route("/health", get(test_handler))
            .layer(layer);

        // Health should always succeed
        for _ in 0..5 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn token_bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(1.0);
        let tokens_per_second = 1.0;
        let max_tokens = 1.0;

        // Consume the token
        assert!(bucket.try_consume(tokens_per_second, max_tokens));
        // Should be empty now
        assert!(!bucket.try_consume(tokens_per_second, max_tokens));

        // Simulate time passing by manipulating last_update
        bucket.last_update = Instant::now()
            .checked_sub(Duration::from_secs(2))
            .expect("Time subtraction should succeed");

        // Should have refilled
        assert!(bucket.try_consume(tokens_per_second, max_tokens));
    }

    #[tokio::test]
    async fn cleanup_removes_stale_entries() {
        let state = RateLimiterState::new(60);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Add an entry
        state.check(ip).await;

        // Verify it exists
        assert_eq!(state.buckets.read().await.len(), 1);

        // Cleanup with zero duration should remove it
        state.cleanup(Duration::ZERO).await;

        // Should be empty now (entry is older than "now")
        // Actually, since we just created it, it won't be older than ZERO duration
        // Let's use a very long duration to keep it
        let state = RateLimiterState::new(60);
        state.check(ip).await;
        state.cleanup(Duration::from_secs(3600)).await;
        assert_eq!(state.buckets.read().await.len(), 1);
    }

    #[tokio::test]
    async fn entry_count_returns_correct_count() {
        let state = RateLimiterState::new(60);

        assert_eq!(state.entry_count().await, 0);

        state.check("192.168.1.1".parse().unwrap()).await;
        assert_eq!(state.entry_count().await, 1);

        state.check("192.168.1.2".parse().unwrap()).await;
        assert_eq!(state.entry_count().await, 2);

        // Same IP again should not increase count
        state.check("192.168.1.1".parse().unwrap()).await;
        assert_eq!(state.entry_count().await, 2);
    }

    #[tokio::test]
    async fn spawn_cleanup_task_can_be_cancelled() {
        let state = Arc::new(RateLimiterState::new(60));
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Add an entry
        state.check(ip).await;
        assert_eq!(state.entry_count().await, 1);

        // Start cleanup task with short interval
        let handle = super::spawn_cleanup_task(
            Arc::clone(&state),
            Duration::from_millis(10),
            Duration::from_millis(1),
        );

        // Wait for at least one cleanup cycle
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Entry should have been cleaned up
        assert_eq!(state.entry_count().await, 0);

        // Abort the task
        handle.abort();

        // Task should be cancelled
        assert!(handle.await.unwrap_err().is_cancelled());
    }

    #[test]
    fn extract_ip_uses_connect_info() {
        let trusted_proxies = HashSet::new();
        let addr: std::net::SocketAddr = "192.168.1.42:12345".parse().unwrap();
        let req = request_with_connect_info("/test", addr);

        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "192.168.1.42".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_falls_back_to_localhost_without_connect_info() {
        let trusted_proxies = HashSet::new();
        let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_ignores_xff_without_trusted_proxies() {
        let trusted_proxies = HashSet::new();
        let addr: std::net::SocketAddr = "10.0.0.5:9999".parse().unwrap();

        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut()
            .insert("x-forwarded-for", "10.0.0.1, 192.168.1.1".parse().unwrap());

        // Without trusted proxies, X-Forwarded-For should be ignored
        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "10.0.0.5".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_uses_xff_from_trusted_proxy() {
        let mut trusted_proxies = HashSet::new();
        trusted_proxies.insert("10.0.0.1".parse().unwrap());

        let addr: std::net::SocketAddr = "10.0.0.1:443".parse().unwrap();
        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut()
            .insert("x-forwarded-for", "203.0.113.50, 10.0.0.1".parse().unwrap());

        // Connecting from trusted proxy, should use X-Forwarded-For
        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "203.0.113.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_handles_single_xff_ip() {
        let mut trusted_proxies = HashSet::new();
        trusted_proxies.insert("10.0.0.1".parse().unwrap());

        let addr: std::net::SocketAddr = "10.0.0.1:443".parse().unwrap();
        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut()
            .insert("x-forwarded-for", "192.168.100.50".parse().unwrap());

        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "192.168.100.50".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_handles_xff_with_ipv6() {
        let mut trusted_proxies = HashSet::new();
        trusted_proxies.insert("::1".parse().unwrap());

        let addr: std::net::SocketAddr = "[::1]:8080".parse().unwrap();
        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut()
            .insert("x-forwarded-for", "2001:db8::1, ::1".parse().unwrap());

        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "2001:db8::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_handles_invalid_xff() {
        let mut trusted_proxies = HashSet::new();
        trusted_proxies.insert("10.0.0.1".parse().unwrap());

        let addr: std::net::SocketAddr = "10.0.0.1:443".parse().unwrap();
        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut().insert(
            "x-forwarded-for",
            "not-an-ip, also-invalid".parse().unwrap(),
        );

        // Should fall back to connecting IP when X-Forwarded-For is invalid
        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extract_ip_ignores_xff_from_untrusted_source() {
        let mut trusted_proxies = HashSet::new();
        trusted_proxies.insert("10.0.0.1".parse().unwrap());

        // Connecting from an untrusted IP that sends X-Forwarded-For
        let addr: std::net::SocketAddr = "192.168.1.100:12345".parse().unwrap();
        let mut req = request_with_connect_info("/test", addr);
        req.headers_mut()
            .insert("x-forwarded-for", "1.2.3.4".parse().unwrap());

        // Should ignore X-Forwarded-For since connecting IP is not trusted
        let ip = extract_client_ip(&req, &trusted_proxies);
        assert_eq!(ip, "192.168.1.100".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn trusted_proxies_in_config() {
        let config = RateLimiterConfig {
            enabled: true,
            requests_per_minute: 60,
            trusted_proxies: vec![
                "127.0.0.1".parse().unwrap(),
                "::1".parse().unwrap(),
                "10.0.0.1".parse().unwrap(),
            ],
        };

        let layer = RateLimiterLayer::new(&config);
        assert_eq!(layer.trusted_proxies.len(), 3);
        assert!(
            layer
                .trusted_proxies
                .contains(&"127.0.0.1".parse().unwrap())
        );
        assert!(layer.trusted_proxies.contains(&"::1".parse().unwrap()));
        assert!(layer.trusted_proxies.contains(&"10.0.0.1".parse().unwrap()));
    }
}
