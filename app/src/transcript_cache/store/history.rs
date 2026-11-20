//! Read one cache generation with stable turn boundaries and its late tool updates.
use super::{TranscriptCacheStore, parse_session_row};
use crate::{
    adapters::acp::{RawMessage, shared_view_projection::AcpViewProjector},
    application::shared_views::{indexed_journal::ViewProjector, journal::ContentIdentity},
    domain::{
        history_page::{PageError, TurnIndexer, TurnRange},
        native::NativeSessionKey,
    },
    transcript_cache::{
        CachedRawRecord, LoadedTranscriptSnapshot, TranscriptCacheError, entity_id_to_bytes,
        turns::TurnRows,
    },
};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum PageReadError {
    #[error(transparent)]
    Cache(#[from] TranscriptCacheError),
    #[error(transparent)]
    Page(#[from] PageError),
}
impl From<sqlx::Error> for PageReadError {
    fn from(error: sqlx::Error) -> Self {
        Self::Cache(error.into())
    }
}

pub struct CachedPage {
    pub snapshot: LoadedTranscriptSnapshot,
    pub generation: [u8; 16],
    pub total: u64,
    pub range: Option<TurnRange>,
    /// Turn number and original ordinal, including the original first turn after any prelude.
    pub boundaries: Vec<(u64, u64)>,
    /// Later same-tool patches retained for the projected state of anchors on this page.
    pub tool_updates: Vec<CachedRawRecord>,
    pub prior_tool_anchors: Vec<CachedRawRecord>,
}

const SESSION:&str="SELECT active_generation,protocol_version,content_digest,last_updated_at_ms,last_synced_at_ms,
    capability_profile,native_recovery_state,record_count,byte_size,generation_token,turn_state,turn_count
    FROM transcript_sessions WHERE source_id=? AND native_session_id=?";

impl TranscriptCacheStore {
    /// Returns the token only when the live transcript matches the authoritative snapshot.
    pub(crate) async fn matching_generation(
        &self,
        key: &NativeSessionKey,
        digest: [u8; 32],
    ) -> Result<Option<[u8; 16]>, TranscriptCacheError> {
        let Some((pool, _)) = self.open_handles().await else {
            return Ok(None);
        };
        let row = sqlx::query(SESSION)
            .bind(entity_id_to_bytes(key.source().0)?.as_slice())
            .bind(key.native_id())
            .fetch_optional(&pool)
            .await?;
        let Some(row) = row else { return Ok(None) };
        let session = parse_session_row(key.clone(), &row)?;
        if session.content_digest != digest || self.is_snapshot_expired(&session) {
            return Ok(None);
        }
        let token: Vec<u8> = row.try_get("generation_token")?;
        let token = token.try_into().map_err(|_| {
            TranscriptCacheError::CorruptDatabase("invalid history generation".into())
        })?;
        Ok(Some(token))
    }
    /// Resolves and validates coordinates against the generation pinned by this read transaction.
    /// An ordinary open may request the complete unprovable view by returning `None`.
    ///
    /// # Errors
    /// Propagates storage failures and the caller's cursor or range validation errors.
    pub async fn read_turn_page<F>(
        &self,
        key: &NativeSessionKey,
        select: F,
    ) -> Result<Option<CachedPage>, PageReadError>
    where
        F: FnOnce([u8; 16], u64) -> Result<Option<TurnRange>, PageError>,
    {
        let Some((pool, write_mutex)) = self.open_handles().await else {
            return Ok(None);
        };
        let source = entity_id_to_bytes(key.source().0)?;
        let mut tx = pool.begin().await?;
        let mut row = sqlx::query(SESSION)
            .bind(source.as_slice())
            .bind(key.native_id())
            .fetch_optional(&mut *tx)
            .await?;
        if row
            .as_ref()
            .map(|r| r.try_get::<String, _>("turn_state"))
            .transpose()?
            .as_deref()
            == Some("pending")
        {
            tx.rollback().await?;
            let guard = write_mutex.lock().await;
            tx = pool.begin().await?;
            row = sqlx::query(SESSION)
                .bind(source.as_slice())
                .bind(key.native_id())
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(old) = &row {
                let session = parse_session_row(key.clone(), old)?;
                if self.is_snapshot_expired(&session) {
                    return Ok(None);
                }
                if old.try_get::<String, _>("turn_state")? == "pending" {
                    let keys = TurnRows {
                        source: &source,
                        native: key.native_id(),
                        generation: old.try_get("active_generation")?,
                    };
                    let mut index = TurnIndexer::default();
                    keys.clear(&mut tx).await?;
                    let mut after = 0;
                    loop {
                        let chunk = read_chunk(&mut tx, &keys, after, i64::MAX).await?;
                        if chunk.is_empty() {
                            break;
                        }
                        for rec in chunk {
                            after = checked_ordinal(rec.ordinal)?;
                            keys.observe(
                                &mut tx,
                                &mut index,
                                session.protocol_version,
                                &rec.raw,
                                after,
                            )
                            .await?;
                        }
                    }
                    keys.finish(&mut tx, &index).await?;
                }
            }
            tx.commit().await?;
            drop(guard);
            tx = pool.begin().await?;
            row = sqlx::query(SESSION)
                .bind(source.as_slice())
                .bind(key.native_id())
                .fetch_optional(&mut *tx)
                .await?;
        }
        let Some(row) = row else { return Ok(None) };
        let session = parse_session_row(key.clone(), &row)?;
        if self.is_snapshot_expired(&session) {
            return Ok(None);
        }
        let generation: Vec<u8> = row.try_get("generation_token")?;
        let generation = generation.try_into().map_err(|_| {
            TranscriptCacheError::CorruptDatabase("invalid history generation".into())
        })?;
        let total = nonnegative(row.try_get("turn_count")?)?;
        let range = select(generation, total)?;
        let keys = TurnRows {
            source: &source,
            native: key.native_id(),
            generation: row.try_get("active_generation")?,
        };
        let (boundaries, start, end) = page_boundaries(&mut tx, &keys, range, total).await?;
        let (records, tool_updates, prior_tool_anchors) =
            page_records(&mut tx, &keys, session.protocol_version, start, end).await?;
        tx.commit().await?;
        Ok(Some(CachedPage {
            snapshot: LoadedTranscriptSnapshot { session, records },
            generation,
            total,
            range,
            boundaries,
            tool_updates,
            prior_tool_anchors,
        }))
    }
}
fn nonnegative(n: i64) -> Result<u64, TranscriptCacheError> {
    n.try_into()
        .map_err(|_| TranscriptCacheError::CorruptDatabase("negative history coordinate".into()))
}
fn checked_ordinal(n: u64) -> Result<i64, TranscriptCacheError> {
    n.try_into()
        .map_err(|_| TranscriptCacheError::Protocol("history coordinate exceeds i64".into()))
}
async fn read_chunk(
    tx: &mut Transaction<'_, Sqlite>,
    keys: &TurnRows<'_>,
    after: i64,
    end: i64,
) -> Result<Vec<CachedRawRecord>, TranscriptCacheError> {
    let rows=sqlx::query("SELECT ordinal,connection_sequence,raw_json FROM transcript_records WHERE source_id=? AND native_session_id=? AND generation=? AND ordinal>? AND ordinal<=? ORDER BY ordinal LIMIT 64")
        .bind(keys.source).bind(keys.native).bind(keys.generation).bind(after).bind(end).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(|r| {
            Ok(CachedRawRecord {
                ordinal: nonnegative(r.try_get("ordinal")?)?,
                connection_sequence: nonnegative(r.try_get("connection_sequence")?)?,
                raw: RawMessage::parse(r.try_get::<Vec<u8>, _>("raw_json")?).map_err(|_| {
                    TranscriptCacheError::CorruptDatabase("invalid cached ACP frame".into())
                })?,
            })
        })
        .collect()
}

async fn page_boundaries(
    tx: &mut Transaction<'_, Sqlite>,
    keys: &TurnRows<'_>,
    range: Option<TurnRange>,
    total: u64,
) -> Result<(Vec<(u64, u64)>, i64, i64), PageReadError> {
    let mut boundaries = Vec::new();
    let (mut start, mut end) = (1, i64::MAX);
    if let Some(range) = range {
        if range.first == 0 || range.last > total || range.first > range.last {
            return Err(PageError::NotFound.into());
        }
        let rows=sqlx::query("SELECT turn_index,first_ordinal FROM transcript_turns WHERE source_id=? AND native_session_id=? AND generation=? AND turn_index BETWEEN ? AND ? ORDER BY turn_index")
                .bind(keys.source).bind(keys.native).bind(keys.generation).bind(checked_ordinal(range.first)?).bind(checked_ordinal(range.last.saturating_add(1))?).fetch_all(&mut **tx).await?;
        for r in rows {
            let turn = nonnegative(r.try_get("turn_index")?)?;
            let ordinal: i64 = r.try_get("first_ordinal")?;
            if turn > range.last {
                end = ordinal - 1;
                break;
            }
            if turn == range.first && turn > 1 {
                start = ordinal;
            }
            boundaries.push((turn, nonnegative(ordinal)?));
        }
        if boundaries.len() as u64 != range.last - range.first + 1 {
            return Err(
                TranscriptCacheError::CorruptDatabase("incomplete turn index".into()).into(),
            );
        }
    }
    Ok((boundaries, start, end))
}
async fn page_records(
    tx: &mut Transaction<'_, Sqlite>,
    keys: &TurnRows<'_>,
    version: crate::adapters::acp::Version,
    start: i64,
    end: i64,
) -> Result<
    (
        Vec<CachedRawRecord>,
        Vec<CachedRawRecord>,
        Vec<CachedRawRecord>,
    ),
    TranscriptCacheError,
> {
    let projector = AcpViewProjector::for_version(version);
    let mut tools = HashSet::new();
    let mut records = Vec::new();
    let mut tool_updates = Vec::new();
    let mut after = start - 1;
    loop {
        let chunk = read_chunk(tx, keys, after, end).await?;
        if chunk.is_empty() {
            break;
        }
        for rec in chunk {
            after = checked_ordinal(rec.ordinal)?;
            if start > 1 || end < i64::MAX {
                let env = projector.inspect(&rec.raw);
                if let Some(ContentIdentity::Tool(id)) = env.identity {
                    tools.insert((env.session, id));
                }
            }
            records.push(rec);
        }
    }
    // Seed an earlier tool identity in the private projector so an in-page patch
    // retains its original anchor. The reader skips these seed records and anchors.
    let mut prior_tool_anchors = Vec::new();
    let mut before = 0;
    while start > 1 && !tools.is_empty() {
        let chunk = read_chunk(tx, keys, before, start - 1).await?;
        if chunk.is_empty() {
            break;
        }
        for rec in chunk {
            before = checked_ordinal(rec.ordinal)?;
            let env = projector.inspect(&rec.raw);
            if let Some(ContentIdentity::Tool(id)) = env.identity
                && tools.remove(&(env.session, id))
            {
                prior_tool_anchors.push(rec);
            }
        }
    }
    while end < i64::MAX && !tools.is_empty() {
        let chunk = read_chunk(tx, keys, after, i64::MAX).await?;
        if chunk.is_empty() {
            break;
        }
        for rec in chunk {
            after = checked_ordinal(rec.ordinal)?;
            let env = projector.inspect(&rec.raw);
            if let Some(ContentIdentity::Tool(id)) = env.identity
                && tools.contains(&(env.session, id))
            {
                tool_updates.push(rec);
            }
        }
    }
    Ok((records, tool_updates, prior_tool_anchors))
}
