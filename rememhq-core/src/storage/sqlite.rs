//! SQLite storage backend with WAL mode and FTS5 full-text search.

use crate::memory::types::{KnowledgeGraphUpdate, MemoryRecord, MemoryType};
use crate::storage::{MemoryStore, StoreStats};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// SQLite-backed memory store with WAL mode for concurrent reads.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Open or create a SQLite database at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Enable WAL mode for concurrent reads
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Initialize the database schema synchronously before sharing the connection.
    fn init_schema(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id              TEXT PRIMARY KEY,
                content         TEXT NOT NULL,
                importance      REAL NOT NULL DEFAULT 5.0,
                tags            TEXT NOT NULL DEFAULT '[]',
                memory_type     TEXT NOT NULL DEFAULT 'fact',
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL,
                decay_score     REAL NOT NULL DEFAULT 1.0,
                source_session  TEXT,
                ttl_days        INTEGER,
                archived        INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
            CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC);
            CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(source_session);

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                content,
                tags,
                content='memories',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS index in sync
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, content, tags)
                VALUES (new.rowid, new.content, new.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content, tags)
                VALUES ('delete', old.rowid, old.content, old.tags);
            END;

            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, content, tags)
                VALUES ('delete', old.rowid, old.content, old.tags);
                INSERT INTO memories_fts(rowid, content, tags)
                VALUES (new.rowid, new.content, new.tags);
            END;

            -- Knowledge graph triples
            CREATE TABLE IF NOT EXISTS knowledge_graph (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                subject     TEXT NOT NULL,
                predicate   TEXT NOT NULL,
                object      TEXT NOT NULL,
                memory_id   TEXT REFERENCES memories(id) ON DELETE CASCADE,
                created_at  TEXT NOT NULL,
                UNIQUE(subject, predicate, object)
            );

            CREATE INDEX IF NOT EXISTS idx_kg_subject ON knowledge_graph(subject);
            CREATE INDEX IF NOT EXISTS idx_kg_object ON knowledge_graph(object);

            -- Sessions table
            CREATE TABLE IF NOT EXISTS sessions (
                id              TEXT PRIMARY KEY,
                project         TEXT NOT NULL,
                started_at      TEXT NOT NULL,
                ended_at        TEXT,
                consolidated    INTEGER NOT NULL DEFAULT 0,
                memory_count    INTEGER NOT NULL DEFAULT 0
            );
            ",
        )?;

        Ok(())
    }

    /// Serialize tags to JSON string for storage.
    fn serialize_tags(tags: &[String]) -> String {
        serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
    }

    /// Deserialize tags from JSON string.
    fn deserialize_tags(raw: &str) -> Vec<String> {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Parse a memory record from a SQLite row.
    fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
        let id_str: String = row.get(0)?;
        let content: String = row.get(1)?;
        let importance: f64 = row.get(2)?;
        let tags_raw: String = row.get(3)?;
        let type_str: String = row.get(4)?;
        let created_str: String = row.get(5)?;
        let updated_str: String = row.get(6)?;
        let decay_score: f64 = row.get(7)?;
        let source_session: Option<String> = row.get(8)?;
        let ttl_days: Option<u32> = row.get(9)?;

        Ok(MemoryRecord {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            content,
            embedding: None, // Embeddings stored in vector index, not SQLite
            importance: importance as f32,
            tags: SqliteStore::deserialize_tags(&tags_raw),
            memory_type: type_str.parse().unwrap_or(MemoryType::Fact),
            created_at: DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            decay_score: decay_score as f32,
            source_session,
            ttl_days,
        })
    }
}

#[async_trait::async_trait]
impl MemoryStore for SqliteStore {
    async fn insert(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (id, content, importance, tags, memory_type, created_at, updated_at, decay_score, source_session, ttl_days)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.id.to_string(),
                record.content,
                record.importance as f64,
                Self::serialize_tags(&record.tags),
                record.memory_type.to_string(),
                record.created_at.to_rfc3339(),
                record.updated_at.to_rfc3339(),
                record.decay_score as f64,
                record.source_session,
                record.ttl_days,
            ],
        )?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> anyhow::Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, content, importance, tags, memory_type, created_at, updated_at, decay_score, source_session, ttl_days
             FROM memories WHERE id = ?1 AND archived = 0",
        )?;

        let result = stmt
            .query_row(params![id.to_string()], Self::row_to_record)
            .optional()?;

        Ok(result)
    }

    async fn update(&self, record: &MemoryRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE memories SET content = ?1, importance = ?2, tags = ?3, memory_type = ?4, updated_at = ?5, decay_score = ?6
             WHERE id = ?7",
            params![
                record.content,
                record.importance as f64,
                Self::serialize_tags(&record.tags),
                record.memory_type.to_string(),
                Utc::now().to_rfc3339(),
                record.decay_score as f64,
                record.id.to_string(),
            ],
        )?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> anyhow::Result<bool> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "DELETE FROM memories WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(rows > 0)
    }

    async fn insert_knowledge_triple(
        &self,
        triple: &KnowledgeGraphUpdate,
        memory_id: Uuid,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR IGNORE INTO knowledge_graph (subject, predicate, object, memory_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                triple.subject,
                triple.predicate,
                triple.object,
                memory_id.to_string(),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_knowledge_for_entity(
        &self,
        entity: &str,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT subject, predicate, object FROM knowledge_graph 
             WHERE subject = ?1 OR object = ?1",
        )?;

        let triples = stmt
            .query_map(params![entity], |row| {
                Ok(KnowledgeGraphUpdate {
                    subject: row.get(0)?,
                    predicate: row.get(1)?,
                    object: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(triples)
    }

    async fn query_knowledge(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object: Option<&str>,
    ) -> anyhow::Result<Vec<KnowledgeGraphUpdate>> {
        let conn = self.conn.lock().await;
        let mut sql =
            String::from("SELECT subject, predicate, object FROM knowledge_graph WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(s) = subject {
            sql.push_str(" AND subject = ?");
            params_vec.push(Box::new(s.to_string()));
        }
        if let Some(p) = predicate {
            sql.push_str(" AND predicate = ?");
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(o) = object {
            sql.push_str(" AND object = ?");
            params_vec.push(Box::new(o.to_string()));
        }

        let mut stmt = conn.prepare(&sql)?;
        let triples = stmt
            .query_map(rusqlite::params_from_iter(params_vec), |row| {
                Ok(KnowledgeGraphUpdate {
                    subject: row.get(0)?,
                    predicate: row.get(1)?,
                    object: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(triples)
    }

    async fn list_recent_entities(&self, limit: usize) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT name FROM (
                SELECT subject AS name, created_at FROM knowledge_graph
                UNION ALL
                SELECT object AS name, created_at FROM knowledge_graph
            ) ORDER BY created_at DESC LIMIT ?1",
        )?;

        let entities = stmt
            .query_map(params![limit as i64], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entities)
    }

    async fn search_fts(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT m.id, m.content, m.importance, m.tags, m.memory_type, m.created_at, m.updated_at, m.decay_score, m.source_session, m.ttl_days
             FROM memories m
             JOIN memories_fts fts ON m.rowid = fts.rowid
             WHERE memories_fts MATCH ?1 AND m.archived = 0
             ORDER BY rank
             LIMIT ?2",
        )?;

        let records = stmt
            .query_map(params![query, limit as i64], Self::row_to_record)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    async fn list(
        &self,
        filter_tags: &[String],
        memory_type: Option<MemoryType>,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryRecord>> {
        let conn = self.conn.lock().await;

        let mut sql = String::from(
            "SELECT id, content, importance, tags, memory_type, created_at, updated_at, decay_score, source_session, ttl_days
             FROM memories WHERE archived = 0"
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(mt) = memory_type {
            sql.push_str(" AND memory_type = ?");
            param_values.push(Box::new(mt.to_string()));
        }

        if let Some(since_dt) = since {
            sql.push_str(" AND created_at >= ?");
            param_values.push(Box::new(since_dt.to_rfc3339()));
        }

        // Tag filtering: check if any filter tag is contained in the JSON array
        for tag in filter_tags {
            sql.push_str(" AND tags LIKE ?");
            param_values.push(Box::new(format!("%\"{}\"%", tag)));
        }

        sql.push_str(" ORDER BY importance DESC, created_at DESC LIMIT ?");
        param_values.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();
        let records = stmt
            .query_map(params_ref.as_slice(), Self::row_to_record)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    async fn stats(&self) -> anyhow::Result<StoreStats> {
        let conn = self.conn.lock().await;

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE archived = 0",
            [],
            |row| row.get(0),
        )?;
        let total = total as usize;

        let avg_importance: f64 = conn.query_row(
            "SELECT COALESCE(AVG(importance), 0.0) FROM memories WHERE archived = 0",
            [],
            |row| row.get(0),
        )?;

        let mut by_type = HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT memory_type, COUNT(*) FROM memories WHERE archived = 0 GROUP BY memory_type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;
        for (k, v) in rows.flatten() {
            by_type.insert(k, v);
        }

        Ok(StoreStats {
            total_memories: total,
            by_type,
            avg_importance: avg_importance as f32,
            db_size_bytes: 0, // TODO: get actual file size
        })
    }

    async fn archive(&self, id: Uuid) -> anyhow::Result<bool> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE memories SET archived = 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id.to_string()],
        )?;
        Ok(rows > 0)
    }

    async fn apply_decay(&self, decay_factor: f32) -> anyhow::Result<usize> {
        let conn = self.conn.lock().await;
        // Decay score decreases faster for low-importance memories
        let rows = conn.execute(
            "UPDATE memories SET decay_score = decay_score * (?1 + (importance / 20.0))
             WHERE archived = 0 AND decay_score > 0.01",
            params![decay_factor as f64],
        )?;
        Ok(rows)
    }

    async fn get_decayed_ids(&self, threshold: f32) -> anyhow::Result<Vec<Uuid>> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT id FROM memories WHERE archived = 0 AND decay_score < ?1")?;

        let ids = stmt
            .query_map(params![threshold as f64], |row| {
                let id_str: String = row.get(0)?;
                Ok(Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }
}

// We need rusqlite::OptionalExtension
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_get() {
        let store = SqliteStore::open_in_memory().unwrap();
        let record = MemoryRecord::new("test fact", MemoryType::Fact)
            .with_tags(vec!["test".into()])
            .with_importance(8.0);

        let id = record.id;
        store.insert(&record).await.unwrap();

        let retrieved = store.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "test fact");
        assert_eq!(retrieved.importance, 8.0);
        assert_eq!(retrieved.tags, vec!["test".to_string()]);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = SqliteStore::open_in_memory().unwrap();
        let record = MemoryRecord::new("to delete", MemoryType::Fact);
        let id = record.id;

        store.insert(&record).await.unwrap();
        assert!(store.delete(id).await.unwrap());
        assert!(store.get(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_fts_search() {
        let store = SqliteStore::open_in_memory().unwrap();

        let r1 = MemoryRecord::new("PostgreSQL database on RDS", MemoryType::Fact);
        let r2 = MemoryRecord::new("Redis cache for sessions", MemoryType::Fact);
        store.insert(&r1).await.unwrap();
        store.insert(&r2).await.unwrap();

        let results = store.search_fts("PostgreSQL", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("PostgreSQL"));
    }

    #[tokio::test]
    async fn test_stats() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .insert(&MemoryRecord::new("fact1", MemoryType::Fact).with_importance(8.0))
            .await
            .unwrap();
        store
            .insert(&MemoryRecord::new("proc1", MemoryType::Procedure).with_importance(6.0))
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.total_memories, 2);
        assert_eq!(stats.by_type.get("fact"), Some(&1));
        assert_eq!(stats.by_type.get("procedure"), Some(&1));
    }
}
