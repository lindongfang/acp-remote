//! Schema definition and integrity checks for the isolated `transcript-cache.sqlite3` database.

use sqlx::{Row, SqlitePool};

use super::TranscriptCacheError;

pub const TRANSCRIPT_CACHE_SCHEMA_VERSION: i64 = 2;

const BASE_SCHEMA_SQL: &str = r"
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

const TURN_SCHEMA_SQL: &str = r"
ALTER TABLE transcript_sessions ADD COLUMN generation_token BLOB NOT NULL DEFAULT X'';
ALTER TABLE transcript_sessions ADD COLUMN turn_state TEXT NOT NULL DEFAULT 'pending'
    CHECK(turn_state IN ('pending', 'provable', 'unprovable'));
ALTER TABLE transcript_sessions ADD COLUMN turn_count INTEGER NOT NULL DEFAULT 0 CHECK(turn_count >= 0);
UPDATE transcript_sessions SET generation_token = randomblob(16);
CREATE TABLE transcript_turns (
    source_id BLOB NOT NULL,
    native_session_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation >= 1),
    turn_index INTEGER NOT NULL CHECK(turn_index >= 1),
    first_ordinal INTEGER NOT NULL CHECK(first_ordinal >= 1),
    PRIMARY KEY(source_id, native_session_id, generation, turn_index),
    FOREIGN KEY(source_id, native_session_id) REFERENCES transcript_sessions(source_id, native_session_id)
        ON DELETE CASCADE
) WITHOUT ROWID;
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
    for statement in BASE_SCHEMA_SQL
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(statement).execute(&mut *tx).await?;
    }

    let existing = sqlx::query("SELECT schema_version FROM transcript_cache_meta WHERE id = 1")
        .fetch_optional(&mut *tx)
        .await?;

    let version = existing
        .map(|row| row.try_get::<i64, _>("schema_version"))
        .transpose()?;
    match version {
        None | Some(1) => {
            for statement in TURN_SCHEMA_SQL
                .split(';')
                .map(str::trim)
                .filter(|sql| !sql.is_empty())
            {
                sqlx::query(statement).execute(&mut *tx).await?;
            }
            sqlx::query(
                "INSERT OR REPLACE INTO transcript_cache_meta (id, schema_version) VALUES (1, ?)",
            )
            .bind(TRANSCRIPT_CACHE_SCHEMA_VERSION)
            .execute(&mut *tx)
            .await?;
        }
        Some(TRANSCRIPT_CACHE_SCHEMA_VERSION) => {}
        Some(actual) => {
            return Err(TranscriptCacheError::SchemaMismatch {
                expected: TRANSCRIPT_CACHE_SCHEMA_VERSION,
                actual,
            });
        }
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn migrates_v1_metadata_and_leaves_original_frames_and_timestamps_intact() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(BASE_SCHEMA_SQL).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO transcript_cache_meta VALUES(1,1)")
            .execute(&pool)
            .await
            .unwrap();
        for id in ["a", "b"] {
            sqlx::query("INSERT INTO transcript_sessions VALUES(zeroblob(16),?,1,1,zeroblob(32),17,NULL,'unknown','normal',1,2)").bind(id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO transcript_records VALUES(zeroblob(16),?,1,1,0,X'00ff')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        initialize_schema(&pool).await.unwrap();
        let result:(i64,i64,i64)=sqlx::query_as("SELECT COUNT(*),COUNT(DISTINCT generation_token),SUM(last_updated_at_ms) FROM transcript_sessions WHERE turn_state='pending' AND turn_count=0 AND length(generation_token)=16").fetch_one(&pool).await.unwrap();
        assert_eq!(result, (2, 2, 34));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM transcript_records WHERE raw_json=X'00ff'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        initialize_schema(&pool).await.unwrap();
        pool.close().await;
    }
}
