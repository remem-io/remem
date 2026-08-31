//! Event-sourced session logging.
//!
//! Every significant action (memory store, recall, consolidation, decay) is
//! appended to a JSONL event log file. A lightweight metadata record is
//! returned for optional SQLite indexing, enabling fast queries without
//! loading full log files.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The type of event being logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    MemoryStore,
    MemoryRecall,
    MemoryUpdate,
    MemoryDelete,
    MemoryArchive,
    SessionConsolidate,
    DecayPass,
    Compaction,
    KnowledgeGraphUpdate,
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{}", s)
    }
}

/// A single event in the append-only log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Unique event ID.
    pub id: Uuid,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// The kind of event.
    pub kind: EventKind,
    /// The project this event belongs to.
    pub project: String,
    /// Optional session ID this event is associated with.
    pub session_id: Option<String>,
    /// Optional memory ID involved in this event.
    pub memory_id: Option<Uuid>,
    /// Free-form summary of what happened (for quick display).
    pub summary: String,
    /// Optional structured payload (e.g. the stored content, query text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

impl SessionEvent {
    /// Create a new event with the current timestamp.
    pub fn new(kind: EventKind, project: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            kind,
            project: project.into(),
            session_id: None,
            memory_id: None,
            summary: summary.into(),
            payload: None,
        }
    }

    /// Attach a session ID to this event.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Attach a memory ID to this event.
    pub fn with_memory(mut self, memory_id: Uuid) -> Self {
        self.memory_id = Some(memory_id);
        self
    }

    /// Attach a structured payload to this event.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Lightweight metadata record for SQLite indexing.
///
/// This is a subset of [`SessionEvent`] without the payload, suitable for
/// fast querying and display without loading the full JSONL log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub kind: EventKind,
    pub project: String,
    pub session_id: Option<String>,
    pub memory_id: Option<Uuid>,
    pub summary: String,
}

impl From<&SessionEvent> for EventMetadata {
    fn from(event: &SessionEvent) -> Self {
        Self {
            id: event.id,
            timestamp: event.timestamp,
            kind: event.kind,
            project: event.project.clone(),
            session_id: event.session_id.clone(),
            memory_id: event.memory_id,
            summary: event.summary.clone(),
        }
    }
}

/// Append-only JSONL event log writer.
///
/// Each project gets its own log file at `<data_dir>/projects/<project>/events.jsonl`.
/// Events are appended one-per-line, ensuring crash resilience (partial writes
/// only lose the last incomplete line).
pub struct EventLog {
    log_path: PathBuf,
}

impl EventLog {
    /// Open (or create) the event log for the given project data directory.
    pub fn open(project_data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(project_data_dir)?;
        let log_path = project_data_dir.join("events.jsonl");
        Ok(Self { log_path })
    }

    /// Append an event to the log file.
    ///
    /// Returns the [`EventMetadata`] for optional SQLite indexing.
    pub fn append(&self, event: &SessionEvent) -> io::Result<EventMetadata> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        let mut writer = BufWriter::new(file);

        let line = serde_json::to_string(event)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writeln!(writer, "{}", line)?;
        writer.flush()?;

        Ok(EventMetadata::from(event))
    }

    /// Read all events from the log file.
    ///
    /// Skips malformed lines (e.g. from incomplete writes after a crash).
    pub fn read_all(&self) -> io::Result<Vec<SessionEvent>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.log_path)?;
        let reader = io::BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<SessionEvent>(&line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!("Skipping malformed event log line: {}", e);
                }
            }
        }

        Ok(events)
    }

    /// Scan backwards from the end of the log file, returning the last `n` events.
    ///
    /// This avoids reading the entire file into memory for large logs.
    /// Falls back to `read_all` + truncation for files smaller than 64KB.
    pub fn read_last_n(&self, n: usize) -> io::Result<Vec<SessionEvent>> {
        if !self.log_path.exists() {
            return Ok(Vec::new());
        }

        let metadata = fs::metadata(&self.log_path)?;

        // For small files, just read everything
        if metadata.len() < 65536 {
            let mut all = self.read_all()?;
            let start = all.len().saturating_sub(n);
            return Ok(all.split_off(start));
        }

        // For large files, read from the end
        use std::io::{Read, Seek, SeekFrom};
        let mut file = fs::File::open(&self.log_path)?;

        // Read the last chunk (generous buffer to capture n events)
        let chunk_size = std::cmp::min(metadata.len(), (n as u64) * 4096);
        file.seek(SeekFrom::End(-(chunk_size as i64)))?;

        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;

        // Skip the first (potentially partial) line
        let buffer = if let Some(pos) = buffer.find('\n') {
            &buffer[pos + 1..]
        } else {
            &buffer
        };

        let mut events: Vec<SessionEvent> = buffer
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();

        let start = events.len().saturating_sub(n);
        Ok(events.split_off(start))
    }

    /// Get the path to the underlying log file.
    pub fn path(&self) -> &Path {
        &self.log_path
    }
}

/// A record in the Dead Letter Queue for unrecoverable errors during memory operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub payload: serde_json::Value,
    pub error_message: String,
    pub retry_count: usize,
}

/// Dead Letter Queue (DLQ) persisted as a JSONL file for error recovery and replay.
pub struct DeadLetterQueue {
    path: PathBuf,
}

impl DeadLetterQueue {
    pub fn open(dir: &Path) -> io::Result<Self> {
        let path = dir.join("dlq.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }

    /// Push a failed operation record to the dead letter queue.
    pub fn push(
        &self,
        operation: impl Into<String>,
        payload: serde_json::Value,
        error_message: impl Into<String>,
    ) -> io::Result<DeadLetterRecord> {
        let record = DeadLetterRecord {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            operation: operation.into(),
            payload,
            error_message: error_message.into(),
            retry_count: 0,
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string(&record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writeln!(writer, "{}", json)?;
        writer.flush()?;

        Ok(record)
    }

    /// Read all unhandled dead letter records.
    pub fn read_all(&self) -> io::Result<Vec<DeadLetterRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = io::BufReader::new(file);
        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(rec) = serde_json::from_str::<DeadLetterRecord>(&line) {
                records.push(rec);
            }
        }
        Ok(records)
    }

    /// Clear the dead letter queue.
    pub fn clear(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::write(&self.path, "")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = SessionEvent::new(
            EventKind::MemoryStore,
            "test-project",
            "Stored a new memory",
        )
        .with_memory(Uuid::new_v4())
        .with_session("session-123");

        assert_eq!(event.kind, EventKind::MemoryStore);
        assert_eq!(event.project, "test-project");
        assert!(event.session_id.is_some());
        assert!(event.memory_id.is_some());
    }

    #[test]
    fn test_event_metadata_from() {
        let event = SessionEvent::new(EventKind::MemoryRecall, "proj", "Recalled memories")
            .with_payload(serde_json::json!({"query": "test"}));

        let meta = EventMetadata::from(&event);
        assert_eq!(meta.id, event.id);
        assert_eq!(meta.summary, "Recalled memories");
    }

    #[test]
    fn test_event_log_append_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(dir.path()).unwrap();

        let event1 = SessionEvent::new(EventKind::MemoryStore, "proj", "First event");
        let event2 = SessionEvent::new(EventKind::MemoryRecall, "proj", "Second event");

        let meta1 = log.append(&event1).unwrap();
        let meta2 = log.append(&event2).unwrap();

        assert_eq!(meta1.kind, EventKind::MemoryStore);
        assert_eq!(meta2.kind, EventKind::MemoryRecall);

        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].summary, "First event");
        assert_eq!(events[1].summary, "Second event");
    }

    #[test]
    fn test_event_log_read_empty() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(dir.path()).unwrap();
        let events = log.read_all().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_event_log_read_last_n() {
        let dir = tempfile::tempdir().unwrap();
        let log = EventLog::open(dir.path()).unwrap();

        for i in 0..10 {
            let event = SessionEvent::new(EventKind::MemoryStore, "proj", format!("Event {}", i));
            log.append(&event).unwrap();
        }

        let last_3 = log.read_last_n(3).unwrap();
        assert_eq!(last_3.len(), 3);
        assert_eq!(last_3[0].summary, "Event 7");
        assert_eq!(last_3[1].summary, "Event 8");
        assert_eq!(last_3[2].summary, "Event 9");
    }

    #[test]
    fn test_event_log_skips_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("events.jsonl");

        // Write a valid event, then a malformed line, then another valid event
        let event1 = SessionEvent::new(EventKind::MemoryStore, "proj", "Good event 1");
        let event2 = SessionEvent::new(EventKind::MemoryRecall, "proj", "Good event 2");

        let mut file = fs::File::create(&log_path).unwrap();
        writeln!(file, "{}", serde_json::to_string(&event1).unwrap()).unwrap();
        writeln!(file, "this is not valid json").unwrap();
        writeln!(file, "{}", serde_json::to_string(&event2).unwrap()).unwrap();
        drop(file);

        let log = EventLog::open(dir.path()).unwrap();
        let events = log.read_all().unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_event_kind_display() {
        assert_eq!(EventKind::MemoryStore.to_string(), "memory_store");
        assert_eq!(
            EventKind::SessionConsolidate.to_string(),
            "session_consolidate"
        );
    }

    #[test]
    fn test_dead_letter_queue_push_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let dlq = DeadLetterQueue::open(dir.path()).unwrap();

        assert!(dlq.read_all().unwrap().is_empty());

        let rec = dlq
            .push(
                "consolidation",
                serde_json::json!({"session": "s-123"}),
                "LLM rate limit reached",
            )
            .unwrap();

        assert_eq!(rec.operation, "consolidation");
        assert_eq!(rec.error_message, "LLM rate limit reached");

        let all = dlq.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, rec.id);

        dlq.clear().unwrap();
        assert!(dlq.read_all().unwrap().is_empty());
    }
}
