//! Turn boundaries derived from the same raw frames as the snapshot digest.
use super::TranscriptCacheError;
use crate::{
    adapters::acp::{RawMessage, Version, turn_classify::classify},
    domain::history_page::TurnIndexer,
};
use sqlx::{Sqlite, Transaction};

pub(super) struct TurnRows<'a> {
    pub source: &'a [u8],
    pub native: &'a str,
    pub generation: i64,
}

impl TurnRows<'_> {
    pub async fn clear(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
    ) -> Result<(), TranscriptCacheError> {
        sqlx::query("DELETE FROM transcript_turns WHERE source_id=? AND native_session_id=?")
            .bind(self.source)
            .bind(self.native)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn observe(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        index: &mut TurnIndexer,
        version: Version,
        raw: &RawMessage,
        ordinal: i64,
    ) -> Result<(), TranscriptCacheError> {
        let before = index.provable();
        if let Some(turn) = index.observe(classify(version, raw)) {
            let turn = i64::try_from(turn)
                .map_err(|_| TranscriptCacheError::Protocol("turn index exceeds i64".into()))?;
            sqlx::query("INSERT INTO transcript_turns(source_id,native_session_id,generation,turn_index,first_ordinal) VALUES(?,?,?,?,?)")
                .bind(self.source).bind(self.native).bind(self.generation).bind(turn).bind(ordinal)
                .execute(&mut **tx).await?;
        } else if before && !index.provable() {
            self.clear(tx).await?;
        }
        Ok(())
    }

    pub async fn finish(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        index: &TurnIndexer,
    ) -> Result<(), TranscriptCacheError> {
        let state = if index.provable() {
            "provable"
        } else {
            "unprovable"
        };
        let count = i64::try_from(index.turn_count())
            .map_err(|_| TranscriptCacheError::Protocol("turn count exceeds i64".into()))?;
        sqlx::query("UPDATE transcript_sessions SET turn_state=?,turn_count=? WHERE source_id=? AND native_session_id=? AND active_generation=?")
            .bind(state).bind(count).bind(self.source).bind(self.native).bind(self.generation)
            .execute(&mut **tx).await?;
        Ok(())
    }
}
