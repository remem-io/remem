//! Multi-Tenant Isolation, Quotas, and Directory Partitioning.
//!
//! Enforces strong isolation boundaries between organizational tenants,
//! managing per-tenant key derivation, directory sandboxing, and request quotas.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Context defining the active tenant for memory operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantContext {
    pub tenant_id: String,
    pub project_id: String,
    pub user_id: Option<String>,
    pub is_admin: bool,
}

impl TenantContext {
    pub fn new(tenant_id: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            project_id: project_id.into(),
            user_id: None,
            is_admin: false,
        }
    }

    pub fn default_local() -> Self {
        Self::new("default", "default")
    }

    /// Derive an isolated tenant filesystem path under the data root.
    pub fn data_path(&self, root: &std::path::Path) -> PathBuf {
        root.join("tenants")
            .join(&self.tenant_id)
            .join(&self.project_id)
    }

    /// Derive a tenant-specific master key for envelope encryption.
    pub fn derive_tenant_key(&self, master_secret: &str) -> String {
        format!("{}:{}:{}", master_secret, self.tenant_id, self.project_id)
    }
}

/// Tenant quota configuration and real-time usage tracker.
#[derive(Debug)]
pub struct TenantQuotaManager {
    pub max_memories_per_tenant: usize,
    pub max_requests_per_minute: usize,
    current_memories: AtomicUsize,
    request_counter: AtomicUsize,
}

impl TenantQuotaManager {
    pub fn new(max_memories: usize, max_rpm: usize) -> Self {
        Self {
            max_memories_per_tenant: max_memories,
            max_requests_per_minute: max_rpm,
            current_memories: AtomicUsize::new(0),
            request_counter: AtomicUsize::new(0),
        }
    }

    pub fn check_memory_limit(&self) -> bool {
        self.current_memories.load(Ordering::Relaxed) < self.max_memories_per_tenant
    }

    pub fn record_memory_added(&self) {
        self.current_memories.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_memory_removed(&self) {
        self.current_memories.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn check_and_record_request(&self) -> bool {
        let count = self.request_counter.fetch_add(1, Ordering::Relaxed);
        count < self.max_requests_per_minute
    }

    pub fn reset_rate_limit(&self) {
        self.request_counter.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_path_derivation() {
        let ctx = TenantContext::new("org-spectre", "proj-alpha");
        let root = PathBuf::from("/data/remem");
        let path = ctx.data_path(&root);

        assert!(path.to_string_lossy().contains("org-spectre"));
        assert!(path.to_string_lossy().contains("proj-alpha"));
    }

    #[test]
    fn test_tenant_quota_tracking() {
        let quota = TenantQuotaManager::new(2, 5);
        assert!(quota.check_memory_limit());
        quota.record_memory_added();
        quota.record_memory_added();
        assert!(!quota.check_memory_limit());
    }
}
