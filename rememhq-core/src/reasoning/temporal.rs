//! Temporal Reasoning & Fact Validity Windows.
//!
//! Provides point-in-time querying, temporal boundary extraction, and resolution
//! distinguishing factual evolution from logical contradictions.

use crate::memory::types::MemoryRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Temporal validity window of a fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalWindow {
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
}

/// Result of evaluating a temporal update against an existing fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalResolution {
    /// The newer fact supersedes the older fact and marks the older fact's `valid_to` as now.
    Superseded {
        superseded_id: uuid::Uuid,
        transition_point: DateTime<Utc>,
    },
    /// The facts apply to different non-overlapping time windows and both remain valid.
    ConcurrentDifferentWindows,
    /// Logical contradiction requiring manual or LLM arbitration.
    DirectContradiction,
}

/// Extract temporal boundaries from natural language text using heuristics.
pub fn extract_temporal_boundaries(text: &str) -> TemporalWindow {
    let lower = text.to_lowercase();
    let mut valid_from = None;
    let mut valid_to = None;
    let now = Utc::now();

    if lower.contains("currently") || lower.contains("as of today") || lower.contains("right now") {
        valid_from = Some(now);
    } else if lower.contains("formerly")
        || lower.contains("previously")
        || lower.contains("in the past")
    {
        valid_to = Some(now);
    } else if lower.contains("temporary") || lower.contains("until further notice") {
        valid_from = Some(now);
    }

    TemporalWindow {
        valid_from,
        valid_to,
    }
}

/// Filter memory records to those valid as of a specific timestamp.
pub fn filter_by_validity(memories: &[MemoryRecord], as_of: DateTime<Utc>) -> Vec<MemoryRecord> {
    memories
        .iter()
        .filter(|m| m.is_valid_at(as_of))
        .cloned()
        .collect()
}

/// Resolve whether a newer fact evolves an older fact's timeline or logically contradicts it.
pub fn resolve_temporal_conflict(older: &MemoryRecord, newer: &MemoryRecord) -> TemporalResolution {
    // If the newer fact explicitly states a transition or past tense
    let lower_new = newer.content.to_lowercase();
    if lower_new.contains("now ")
        || lower_new.contains("migrated to")
        || lower_new.contains("switched to")
        || lower_new.contains("deprecated")
        || lower_new.contains("replaced by")
    {
        return TemporalResolution::Superseded {
            superseded_id: older.id,
            transition_point: newer.created_at,
        };
    }

    // If both have explicit non-overlapping validity windows
    if let (Some(old_to), Some(new_from)) = (older.valid_to, newer.valid_from) {
        if old_to <= new_from {
            return TemporalResolution::ConcurrentDifferentWindows;
        }
    }

    TemporalResolution::DirectContradiction
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::types::MemoryType;
    use chrono::Duration;

    #[test]
    fn test_temporal_extraction_and_filtering() {
        let now = Utc::now();
        let mut rec_current = MemoryRecord::new("Currently using Postgres 16", MemoryType::Fact);
        rec_current.valid_from = Some(now - Duration::days(10));
        rec_current.valid_to = None;

        let mut rec_old = MemoryRecord::new("Formerly used MySQL 5.7", MemoryType::Fact);
        rec_old.valid_from = Some(now - Duration::days(100));
        rec_old.valid_to = Some(now - Duration::days(10));

        let memories = vec![rec_current.clone(), rec_old.clone()];

        let current_valid = filter_by_validity(&memories, now);
        assert_eq!(current_valid.len(), 1);
        assert_eq!(current_valid[0].content, "Currently using Postgres 16");

        let past_valid = filter_by_validity(&memories, now - Duration::days(50));
        assert_eq!(past_valid.len(), 1);
        assert_eq!(past_valid[0].content, "Formerly used MySQL 5.7");
    }

    #[test]
    fn test_temporal_conflict_evolution() {
        let older = MemoryRecord::new("Database is MySQL 5.7", MemoryType::Fact);
        let mut newer = MemoryRecord::new("Migrated to Postgres 16", MemoryType::Fact);
        newer.created_at = Utc::now();

        let resolution = resolve_temporal_conflict(&older, &newer);
        match resolution {
            TemporalResolution::Superseded { superseded_id, .. } => {
                assert_eq!(superseded_id, older.id);
            }
            _ => panic!("Expected Superseded resolution"),
        }
    }
}
