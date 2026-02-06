//! Tenant context abstraction for multi-tenant data isolation
//!
//! This module provides the foundational abstractions for multi-tenant support:
//!
//! - [`TenantContext`] - A context carrying tenant information through the request lifecycle
//! - [`TenantAware`] - A trait for entities that belong to a specific tenant
//! - [`TenantFilter`] - A trait for repositories to filter data by tenant
//!
//! # Single-Tenant vs Multi-Tenant
//!
//! The system supports both deployment models:
//!
//! - **Single-tenant**: Use [`TenantId::default()`] for all operations
//! - **Multi-tenant**: Extract tenant ID from authentication/request context
//!
//! # Examples
//!
//! ```
//! use domain::tenant::{TenantContext, TenantAware};
//! use domain::TenantId;
//!
//! // Create a tenant context
//! let context = TenantContext::new(TenantId::default());
//! assert!(context.tenant_id().is_default());
//!
//! // For multi-tenant, use extracted tenant ID
//! let tenant_id = TenantId::new();
//! let multi_tenant_context = TenantContext::new(tenant_id);
//! ```

use super::TenantId;

/// Context carrying tenant information through the request lifecycle
///
/// This struct is designed to be passed through service layers to ensure
/// all data access is scoped to the correct tenant.
///
/// # Thread Safety
///
/// `TenantContext` is `Send + Sync` and can be safely shared across threads.
///
/// # Examples
///
/// ```
/// use domain::tenant::TenantContext;
/// use domain::TenantId;
///
/// let context = TenantContext::new(TenantId::default());
/// assert!(context.tenant_id().is_default());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantContext {
    tenant_id: TenantId,
}

impl TenantContext {
    /// Create a new tenant context with the given tenant ID
    pub const fn new(tenant_id: TenantId) -> Self {
        Self { tenant_id }
    }

    /// Get the tenant ID from this context
    pub const fn tenant_id(&self) -> TenantId {
        self.tenant_id
    }

    /// Create a tenant context for single-tenant deployments
    ///
    /// This is a convenience method that creates a context with the default tenant.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::tenant::TenantContext;
    ///
    /// let context = TenantContext::single_tenant();
    /// assert!(context.tenant_id().is_default());
    /// ```
    pub fn single_tenant() -> Self {
        Self::new(TenantId::default())
    }
}

impl Default for TenantContext {
    /// Returns a single-tenant context by default
    fn default() -> Self {
        Self::single_tenant()
    }
}

impl From<TenantId> for TenantContext {
    fn from(tenant_id: TenantId) -> Self {
        Self::new(tenant_id)
    }
}

/// Trait for entities that belong to a specific tenant
///
/// Implement this trait for all entities that need tenant isolation.
/// This enables automatic tenant filtering in repositories.
///
/// # Examples
///
/// ```
/// use domain::tenant::TenantAware;
/// use domain::TenantId;
///
/// struct Document {
///     id: String,
///     tenant_id: TenantId,
///     content: String,
/// }
///
/// impl TenantAware for Document {
///     fn tenant_id(&self) -> TenantId {
///         self.tenant_id
///     }
/// }
/// ```
pub trait TenantAware {
    /// Get the tenant ID this entity belongs to
    fn tenant_id(&self) -> TenantId;

    /// Check if this entity belongs to the given tenant
    fn belongs_to(&self, tenant_id: TenantId) -> bool {
        self.tenant_id() == tenant_id
    }

    /// Check if this entity belongs to the given context's tenant
    fn belongs_to_context(&self, context: &TenantContext) -> bool {
        self.belongs_to(context.tenant_id())
    }
}

/// Trait for repository operations that require tenant filtering
///
/// This trait provides a standard interface for repositories to ensure
/// all data access is properly scoped to the requesting tenant.
///
/// # Implementation Guidelines
///
/// - All queries MUST include tenant ID in WHERE clauses
/// - Create operations MUST set the tenant ID on new records
/// - Updates MUST verify the tenant ID before modification
/// - Deletes MUST verify the tenant ID before removal
///
/// # Examples
///
/// ```ignore
/// // Example repository implementation (pseudocode)
/// impl TenantFilter for UserRepository {
///     fn apply_tenant_filter(&self, context: &TenantContext) -> Self {
///         self.clone().with_filter(|q| q.where_tenant_id(context.tenant_id()))
///     }
/// }
/// ```
pub trait TenantFilter {
    /// The output type after applying the tenant filter
    type Output;

    /// Apply tenant filtering to this repository/query
    ///
    /// Returns a new instance with the tenant filter applied.
    fn with_tenant(&self, context: &TenantContext) -> Self::Output;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_creation() {
        let tenant_id = TenantId::new();
        let context = TenantContext::new(tenant_id);
        assert_eq!(context.tenant_id(), tenant_id);
    }

    #[test]
    fn test_single_tenant_context() {
        let context = TenantContext::single_tenant();
        assert!(context.tenant_id().is_default());
    }

    #[test]
    fn test_default_is_single_tenant() {
        let context = TenantContext::default();
        assert!(context.tenant_id().is_default());
    }

    #[test]
    fn test_from_tenant_id() {
        let tenant_id = TenantId::new();
        let context: TenantContext = tenant_id.into();
        assert_eq!(context.tenant_id(), tenant_id);
    }

    #[test]
    fn test_tenant_aware_trait() {
        struct TestEntity {
            tenant_id: TenantId,
        }

        impl TenantAware for TestEntity {
            fn tenant_id(&self) -> TenantId {
                self.tenant_id
            }
        }

        let tenant_id = TenantId::new();
        let entity = TestEntity { tenant_id };

        assert_eq!(entity.tenant_id(), tenant_id);
        assert!(entity.belongs_to(tenant_id));
        assert!(!entity.belongs_to(TenantId::new()));
    }

    #[test]
    fn test_belongs_to_context() {
        struct TestEntity {
            tenant_id: TenantId,
        }

        impl TenantAware for TestEntity {
            fn tenant_id(&self) -> TenantId {
                self.tenant_id
            }
        }

        let tenant_id = TenantId::new();
        let entity = TestEntity { tenant_id };
        let context = TenantContext::new(tenant_id);
        let other_context = TenantContext::new(TenantId::new());

        assert!(entity.belongs_to_context(&context));
        assert!(!entity.belongs_to_context(&other_context));
    }

    #[test]
    fn test_context_equality() {
        let tenant_id = TenantId::new();
        let context1 = TenantContext::new(tenant_id);
        let context2 = TenantContext::new(tenant_id);
        let context3 = TenantContext::new(TenantId::new());

        assert_eq!(context1, context2);
        assert_ne!(context1, context3);
    }

    #[test]
    fn test_context_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TenantContext>();
    }
}
