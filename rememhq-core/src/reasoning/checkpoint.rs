//! Checkpoint and recovery manager for long-running consolidation jobs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Status of a consolidation job checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    InProgress,
    Completed,
    Failed,
    Interrupted,
}

/// State checkpoint for a consolidation job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationCheckpoint {
    pub session_id: String,
    pub step_index: usize,
    pub total_steps: usize,
    pub processed_observations: usize,
    pub extracted_facts_count: usize,
    pub last_updated: DateTime<Utc>,
    pub status: CheckpointStatus,
    pub intermediate_facts: Vec<String>,
}

/// Persistent checkpoint manager storing recovery snapshots to disk.
pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
}

impl CheckpointManager {
    pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let checkpoint_dir = dir.into();
        fs::create_dir_all(&checkpoint_dir)?;
        Ok(Self { checkpoint_dir })
    }

    fn file_path(&self, session_id: &str) -> PathBuf {
        self.checkpoint_dir
            .join(format!("{}.checkpoint.json", session_id))
    }

    /// Save a consolidation job checkpoint to disk.
    pub fn save_checkpoint(&self, checkpoint: &ConsolidationCheckpoint) -> std::io::Result<()> {
        let path = self.file_path(&checkpoint.session_id);
        let json = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load an existing checkpoint if one exists for resumption.
    pub fn load_checkpoint(
        &self,
        session_id: &str,
    ) -> std::io::Result<Option<ConsolidationCheckpoint>> {
        let path = self.file_path(session_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)?;
        let cp = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(cp))
    }

    /// Remove a completed checkpoint from disk.
    pub fn clear_checkpoint(&self, session_id: &str) -> std::io::Result<()> {
        let path = self.file_path(session_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// List all active checkpoints found in the directory.
    pub fn list_active_checkpoints(&self) -> std::io::Result<Vec<ConsolidationCheckpoint>> {
        let mut list = Vec::new();
        if !self.checkpoint_dir.exists() {
            return Ok(list);
        }
        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                || path.to_string_lossy().ends_with(".checkpoint.json")
            {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cp) = serde_json::from_str::<ConsolidationCheckpoint>(&content) {
                        list.push(cp);
                    }
                }
            }
        }
        Ok(list)
    }

    /// Clean up any checkpoint files older than the specified retention days (e.g. 7 days).
    pub fn cleanup_old_checkpoints(&self, older_than_days: u32) -> std::io::Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days as i64);
        let mut cleaned = 0;

        for entry in fs::read_dir(&self.checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.to_string_lossy().ends_with(".checkpoint.json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(cp) = serde_json::from_str::<ConsolidationCheckpoint>(&content) {
                        if cp.last_updated < cutoff {
                            let _ = fs::remove_file(&path);
                            cleaned += 1;
                        }
                    }
                }
            }
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let mgr = CheckpointManager::new(temp.path()).unwrap();

        assert!(mgr.load_checkpoint("sess-1").unwrap().is_none());

        let cp = ConsolidationCheckpoint {
            session_id: "sess-1".to_string(),
            step_index: 2,
            total_steps: 5,
            processed_observations: 20,
            extracted_facts_count: 8,
            last_updated: Utc::now(),
            status: CheckpointStatus::InProgress,
            intermediate_facts: vec!["Fact 1".into(), "Fact 2".into()],
        };

        mgr.save_checkpoint(&cp).unwrap();

        let loaded = mgr
            .load_checkpoint("sess-1")
            .unwrap()
            .expect("checkpoint exists");
        assert_eq!(loaded.step_index, 2);
        assert_eq!(loaded.extracted_facts_count, 8);
        assert_eq!(loaded.status, CheckpointStatus::InProgress);

        mgr.clear_checkpoint("sess-1").unwrap();
        assert!(mgr.load_checkpoint("sess-1").unwrap().is_none());
    }
}
