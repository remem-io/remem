//! Audit logging and immutable provenance trail for memory updates, deletions, and archives.
//! Supports compliance, GDPR auditing, and history investigation.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// An audit entry recording a memory modification event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditEntry {
    pub id: Uuid,
    pub actor: String,
    pub action: String, // "insert", "update", "delete", "archive", "unarchive"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl AuditEntry {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        memory_id: Option<Uuid>,
        old_value: Option<String>,
        new_value: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            actor: actor.into(),
            action: action.into(),
            memory_id,
            old_value,
            new_value,
            timestamp: Utc::now(),
        }
    }
}

/// Audit retention policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRetentionPolicy {
    pub retention_days: u32,
    pub auto_prune: bool,
}

impl Default for AuditRetentionPolicy {
    fn default() -> Self {
        Self {
            retention_days: 90,
            auto_prune: true,
        }
    }
}

impl AuditRetentionPolicy {
    pub fn cutoff_timestamp(&self) -> DateTime<Utc> {
        Utc::now() - Duration::days(self.retention_days as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_creation() {
        let mem_id = Uuid::new_v4();
        let entry = AuditEntry::new(
            "test_user",
            "update",
            Some(mem_id),
            Some("old content".into()),
            Some("new content".into()),
        );

        assert_eq!(entry.actor, "test_user");
        assert_eq!(entry.action, "update");
        assert_eq!(entry.memory_id, Some(mem_id));
        assert_eq!(entry.old_value, Some("old content".into()));
        assert_eq!(entry.new_value, Some("new content".into()));
    }

    #[test]
    fn test_audit_retention_cutoff() {
        let policy = AuditRetentionPolicy {
            retention_days: 30,
            auto_prune: true,
        };
        let cutoff = policy.cutoff_timestamp();
        assert!(cutoff < Utc::now());
    }
}
