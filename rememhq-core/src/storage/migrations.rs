//! Robust Database Schema Versioning and Migration Framework.
//!
//! Tracks applied schema migrations in the `schema_version` table with SHA-256 checksums,
//! ensuring safe upgrades and schema integrity verification.

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

/// A single structured database schema migration.
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub up_sql: &'static str,
    pub down_sql: &'static str,
}

impl Migration {
    pub fn checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.up_sql.as_bytes());
        let hash = hasher.finalize();
        hash.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }
}

/// Status of applied migrations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MigrationStatus {
    pub current_version: u32,
    pub target_version: u32,
    pub applied_count: usize,
    pub pending_count: usize,
    pub is_up_to_date: bool,
    pub checksum_valid: bool,
}

/// The official list of sequential migrations for remem.
pub fn get_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            name: "initial_schema",
            up_sql: "
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
                    archived        INTEGER NOT NULL DEFAULT 0,
                    store_id        TEXT,
                    path            TEXT,
                    observation_kind TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
                CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC);
                CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at DESC);
                CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(source_session);

                -- FTS5 virtual table
                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    content,
                    tags,
                    content='memories',
                    content_rowid='rowid'
                );

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

                CREATE TABLE IF NOT EXISTS sessions (
                    id              TEXT PRIMARY KEY,
                    project         TEXT NOT NULL,
                    started_at      TEXT NOT NULL,
                    ended_at        TEXT,
                    consolidated    INTEGER NOT NULL DEFAULT 0,
                    memory_count    INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS session_summaries (
                    session_id      TEXT PRIMARY KEY,
                    project         TEXT NOT NULL,
                    summary         TEXT NOT NULL,
                    files_touched   TEXT NOT NULL DEFAULT '[]',
                    key_decisions   TEXT NOT NULL DEFAULT '[]',
                    timestamp       TEXT NOT NULL
                );
                
                CREATE TABLE IF NOT EXISTS session_logs (
                    id               TEXT PRIMARY KEY,
                    parent_id        TEXT REFERENCES session_logs(id),
                    session_id       TEXT NOT NULL,
                    observation_type TEXT NOT NULL,
                    content          TEXT NOT NULL,
                    timestamp        TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_session_logs_session ON session_logs(session_id);
                
                CREATE TABLE IF NOT EXISTS memory_stores (
                    id              TEXT PRIMARY KEY,
                    name            TEXT NOT NULL,
                    description     TEXT,
                    created_at      TEXT NOT NULL,
                    archived_at     TEXT
                );

                CREATE TABLE IF NOT EXISTS memory_versions (
                    id              TEXT PRIMARY KEY,
                    store_id        TEXT NOT NULL,
                    memory_id       TEXT NOT NULL,
                    operation       TEXT NOT NULL,
                    content         TEXT NOT NULL,
                    content_sha256  TEXT NOT NULL,
                    created_at      TEXT NOT NULL
                );
            ",
            down_sql: "
                DROP TABLE IF EXISTS memory_versions;
                DROP TABLE IF EXISTS memory_stores;
                DROP TABLE IF EXISTS session_logs;
                DROP TABLE IF EXISTS session_summaries;
                DROP TABLE IF EXISTS sessions;
                DROP TABLE IF EXISTS knowledge_graph;
                DROP TRIGGER IF EXISTS memories_au;
                DROP TRIGGER IF EXISTS memories_ad;
                DROP TRIGGER IF EXISTS memories_ai;
                DROP TABLE IF EXISTS memories_fts;
                DROP TABLE IF EXISTS memories;
            ",
        },
        Migration {
            version: 2,
            name: "hierarchical_and_temporal_memory",
            up_sql: "
                ALTER TABLE memories ADD COLUMN parent_fact_id TEXT;
                ALTER TABLE memories ADD COLUMN hierarchy_level INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE memories ADD COLUMN citations TEXT NOT NULL DEFAULT '[]';
                ALTER TABLE memories ADD COLUMN valid_from TEXT;
                ALTER TABLE memories ADD COLUMN valid_to TEXT;
                CREATE INDEX IF NOT EXISTS idx_memories_parent ON memories(parent_fact_id);
                CREATE INDEX IF NOT EXISTS idx_memories_validity ON memories(valid_from, valid_to);
            ",
            down_sql: "
                -- SQLite does not support DROP COLUMN in older versions, handled gracefully
                DROP INDEX IF EXISTS idx_memories_validity;
                DROP INDEX IF EXISTS idx_memories_parent;
            ",
        },
        Migration {
            version: 3,
            name: "audit_log",
            up_sql: "
                CREATE TABLE IF NOT EXISTS audit_log (
                    id          TEXT PRIMARY KEY,
                    actor       TEXT NOT NULL DEFAULT 'system',
                    action      TEXT NOT NULL,
                    memory_id   TEXT,
                    old_value   TEXT,
                    new_value   TEXT,
                    timestamp   TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_audit_memory ON audit_log(memory_id);
                CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp DESC);
            ",
            down_sql: "
                DROP TABLE IF EXISTS audit_log;
            ",
        },
        Migration {
            version: 4,
            name: "dead_letter_events",
            up_sql: "
                CREATE TABLE IF NOT EXISTS dead_letter_events (
                    id              TEXT PRIMARY KEY,
                    operation       TEXT NOT NULL,
                    payload         TEXT NOT NULL,
                    error_message   TEXT NOT NULL,
                    retry_count     INTEGER NOT NULL DEFAULT 0,
                    status          TEXT NOT NULL DEFAULT 'pending',
                    created_at      TEXT NOT NULL,
                    last_retried_at TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_dlq_status ON dead_letter_events(status);
            ",
            down_sql: "
                DROP TABLE IF EXISTS dead_letter_events;
            ",
        },
        Migration {
            version: 5,
            name: "webhooks_and_events",
            up_sql: "
                CREATE TABLE IF NOT EXISTS webhooks (
                    id          TEXT PRIMARY KEY,
                    url         TEXT NOT NULL,
                    events      TEXT NOT NULL DEFAULT '[\"*\"]',
                    secret      TEXT NOT NULL,
                    enabled     INTEGER NOT NULL DEFAULT 1,
                    created_at  TEXT NOT NULL
                );
            ",
            down_sql: "
                DROP TABLE IF EXISTS webhooks;
            ",
        },
        Migration {
            version: 6,
            name: "persistent_embedding_cache",
            up_sql: "
                CREATE TABLE IF NOT EXISTS embedding_cache (
                    content_hash     TEXT PRIMARY KEY,
                    embedding_bytes  BLOB NOT NULL,
                    model            TEXT NOT NULL,
                    created_at       TEXT NOT NULL,
                    last_accessed_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_emb_cache_accessed ON embedding_cache(last_accessed_at);
            ",
            down_sql: "
                DROP TABLE IF EXISTS embedding_cache;
            ",
        },
    ]
}

/// Initialize the schema version table if not present.
pub fn init_version_table(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version     INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            checksum    TEXT NOT NULL,
            applied_at  TEXT NOT NULL
        );
        ",
    )?;
    Ok(())
}

/// Query current applied schema version and status.
pub fn check_status(conn: &Connection) -> anyhow::Result<MigrationStatus> {
    init_version_table(conn)?;
    let migrations = get_migrations();
    let target_version = migrations.last().map(|m| m.version).unwrap_or(0);

    let mut stmt =
        conn.prepare("SELECT version, checksum FROM schema_version ORDER BY version ASC")?;
    let applied_rows = stmt.query_map([], |row| {
        Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut current_version = 0;
    let mut applied_count = 0;
    let mut checksum_valid = true;

    for row in applied_rows {
        let (ver, stored_checksum) = row?;
        current_version = current_version.max(ver);
        applied_count += 1;

        if let Some(m) = migrations.iter().find(|m| m.version == ver) {
            if m.checksum() != stored_checksum {
                checksum_valid = false;
                tracing::warn!(
                    "Migration checksum mismatch on version {}: stored='{}', expected='{}'",
                    ver,
                    stored_checksum,
                    m.checksum()
                );
            }
        }
    }

    let pending_count = migrations.len().saturating_sub(applied_count);
    let is_up_to_date = current_version == target_version && pending_count == 0;

    Ok(MigrationStatus {
        current_version,
        target_version,
        applied_count,
        pending_count,
        is_up_to_date,
        checksum_valid,
    })
}

/// Apply all pending migrations in sequential order.
pub fn migrate_up(conn: &Connection) -> anyhow::Result<usize> {
    init_version_table(conn)?;
    let migrations = get_migrations();
    let mut applied = 0;

    for m in migrations {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM schema_version WHERE version = ?1",
                params![m.version],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !exists {
            tracing::info!("Applying migration v{}: {}", m.version, m.name);
            conn.execute_batch(m.up_sql)?;
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO schema_version (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
                params![m.version, m.name, m.checksum(), now],
            )?;
            applied += 1;
        }
    }

    Ok(applied)
}

/// Rollback migrations down to a target version.
pub fn migrate_down(conn: &Connection, target_version: u32) -> anyhow::Result<usize> {
    init_version_table(conn)?;
    let mut migrations = get_migrations();
    migrations.sort_by_key(|b| std::cmp::Reverse(b.version));
    let mut rolled_back = 0;

    for m in migrations {
        if m.version > target_version {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM schema_version WHERE version = ?1",
                    params![m.version],
                    |_| Ok(true),
                )
                .unwrap_or(false);

            if exists {
                tracing::info!("Rolling back migration v{}: {}", m.version, m.name);
                conn.execute_batch(m.down_sql)?;
                conn.execute(
                    "DELETE FROM schema_version WHERE version = ?1",
                    params![m.version],
                )?;
                rolled_back += 1;
            }
        }
    }

    Ok(rolled_back)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_lifecycle() {
        let conn = Connection::open_in_memory().unwrap();
        let status = check_status(&conn).unwrap();
        assert_eq!(status.current_version, 0);
        assert!(!status.is_up_to_date);

        let applied = migrate_up(&conn).unwrap();
        assert!(applied > 0);

        let status2 = check_status(&conn).unwrap();
        assert!(status2.is_up_to_date);
        assert!(status2.checksum_valid);
        assert_eq!(status2.pending_count, 0);

        // Re-running migrate_up applies 0 new migrations
        let applied_again = migrate_up(&conn).unwrap();
        assert_eq!(applied_again, 0);
    }
}
