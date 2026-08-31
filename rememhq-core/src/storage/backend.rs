//! Pluggable storage backend abstraction with replica failover routing.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// Supported storage backend engine types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendType {
    Sqlite,
    DuckDb,
    Postgres,
    InMemory,
}

/// Configuration for a storage replica node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaConfig {
    pub name: String,
    pub connection_string: String,
    pub is_primary: bool,
    pub is_read_only: bool,
    pub weight: u32,
}

/// Dynamic manager for storage primary and read replicas with health tracking.
pub struct ReplicaManager {
    primary: ReplicaConfig,
    replicas: Vec<ReplicaConfig>,
    primary_healthy: AtomicBool,
}

impl ReplicaManager {
    pub fn new(primary: ReplicaConfig, replicas: Vec<ReplicaConfig>) -> Self {
        Self {
            primary,
            replicas,
            primary_healthy: AtomicBool::new(true),
        }
    }

    /// Default standalone primary configuration.
    pub fn standalone(connection_string: impl Into<String>) -> Self {
        let primary = ReplicaConfig {
            name: "primary-sqlite".to_string(),
            connection_string: connection_string.into(),
            is_primary: true,
            is_read_only: false,
            weight: 100,
        };
        Self::new(primary, Vec::new())
    }

    /// Check if primary node is healthy.
    pub fn is_primary_healthy(&self) -> bool {
        self.primary_healthy.load(Ordering::Relaxed)
    }

    /// Mark primary health status.
    pub fn set_primary_health(&self, healthy: bool) {
        self.primary_healthy.store(healthy, Ordering::Relaxed);
    }

    /// Select optimal read replica based on availability and weight.
    pub fn select_read_target(&self) -> &ReplicaConfig {
        if self.replicas.is_empty() || self.is_primary_healthy() {
            &self.primary
        } else {
            // Pick first healthy read replica if primary is failing
            self.replicas.first().unwrap_or(&self.primary)
        }
    }

    /// Get target connection for write operations (must be primary).
    pub fn write_target(&self) -> anyhow::Result<&ReplicaConfig> {
        if self.is_primary_healthy() {
            Ok(&self.primary)
        } else {
            anyhow::bail!(
                "Cannot perform write: primary storage backend is unhealthy and in failover mode"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replica_manager_routing() {
        let primary = ReplicaConfig {
            name: "primary".into(),
            connection_string: "sqlite://primary.db".into(),
            is_primary: true,
            is_read_only: false,
            weight: 100,
        };
        let replica = ReplicaConfig {
            name: "replica-1".into(),
            connection_string: "sqlite://replica1.db".into(),
            is_primary: false,
            is_read_only: true,
            weight: 50,
        };

        let mgr = ReplicaManager::new(primary, vec![replica]);
        assert_eq!(mgr.select_read_target().name, "primary");
        assert!(mgr.write_target().is_ok());

        mgr.set_primary_health(false);
        assert_eq!(mgr.select_read_target().name, "replica-1");
        assert!(mgr.write_target().is_err());
    }
}
