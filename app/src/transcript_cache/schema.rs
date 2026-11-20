//! Schema definition and integrity checks for the isolated `transcript-cache.sqlite3` database.

use sqlx::{Row, SqlitePool};

use super::TranscriptCacheError;

pub const TRANSCRIPT_CACHE_SCHEMA_VERSION: i64 = 1;

const SCHEMA_SQL: &str = r"
CREATE TABLE IF NOT EXISTS transcript_cache_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transcript_sessions (
    source_id BLOB NOT NULL CHECK(length(source_id) = 16),
    native_session_id TEXT NOT NULL,
    active_generation INTEGER NOT NULL CHECK(active_generation >= 1),
    protocol_version INTEGER NOT NULL CHECK(protocol_version IN (1, 2)),
    content_digest BLOB NOT NULL CHECK(length(content_digest) = 32),
    last_updated_at_ms INTEGER NOT NULL,
    last_synced_at_ms INTEGER,
    capability_profile TEXT NOT NULL CHECK(
        capability_profile IN ('unknown', 'replay_capable', 'resume_only', 'pure_memory')
    ),
    native_recovery_state TEXT NOT NULL CHECK(
        native_recovery_state IN ('normal', 'native_unrecoverable')
    ),
    record_count INTEGER NOT NULL CHECK(record_count >= 0),
    byte_size INTEGER NOT NULL CHECK(byte_size >= 0),
    PRIMARY KEY (source_id, native_session_id)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS transcript_records (
    source_id BLOB NOT NULL CHECK(length(source_id) = 16),
    native_session_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation >= 1),
    ordinal INTEGER NOT NULL CHECK(ordinal >= 1),
    connection_sequence INTEGER NOT NULL CHECK(connection_sequence >= 0),
    raw_json BLOB NOT NULL,
    PRIMARY KEY (source_id, native_session_id, generation, ordinal),
    FOREIGN KEY (source_id, native_session_id)
        REFERENCES transcript_sessions(source_id, native_session_id)
        ON DELETE CASCADE
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_transcript_sessions_updated
    ON transcript_sessions (
        last_updated_at_ms ASC,
        source_id ASC,
        native_session_id ASC
    );
";

pub async fn initialize_schema(pool: &SqlitePool) -> Result<(), TranscriptCacheError> {
    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check(1)")
        .fetch_one(pool)
        .await?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(TranscriptCacheError::CorruptDatabase(format!(
            "sqlite quick_check returned {quick_check}"
        )));
    }

    let fk_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await?;
    if fk_enabled != 1 {
        return Err(TranscriptCacheError::CorruptDatabase(
            "sqlite foreign_keys pragma failed to enable".to_owned(),
        ));
    }

    let mut tx = pool.begin().await?;
    for statement in SCHEMA_SQL
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(statement).execute(&mut *tx).await?;
    }

    let existing = sqlx::query("SELECT schema_version FROM transcript_cache_meta WHERE id = 1")
        .fetch_optional(&mut *tx)
        .await?;

    match existing {
        Some(row) => {
            let version: i64 = row.try_get("schema_version")?;
            if version != TRANSCRIPT_CACHE_SCHEMA_VERSION {
                return Err(TranscriptCacheError::SchemaMismatch {
                    expected: TRANSCRIPT_CACHE_SCHEMA_VERSION,
                    actual: version,
                });
            }
        }
        None => {
            sqlx::query("INSERT INTO transcript_cache_meta (id, schema_version) VALUES (1, ?)")
                .bind(TRANSCRIPT_CACHE_SCHEMA_VERSION)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
