//! Latest-value history persistence; no scheduling, protocol dispatch or network authority.
use crate::{
    application::history::HistoryRepository,
    domain::{
        EntityId,
        history::{
            CommitAck, CommittedItem, ConnectionEpoch, ContentVersion, HistoryError, HistoryItem,
            InstanceId, IntegrityChange, IntegrityFault, ItemId, ItemIdentity, MAX_VERSION,
            MissingRange, NativeSessionId, NativeToolCallId, OutputScope, Position,
            PositionReservation, RawContent, ReadPin, SourceId, SourceOrigin, ToolReference,
        },
    },
};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::sync::Arc;
#[derive(Clone)]
pub struct HistoryStore {
    pool: SqlitePool,
}
impl HistoryStore {
    pub(crate) const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    /// Reserves positions durably before an observation enters an asynchronous output path.
    /// # Errors
    /// Rejects invalid scopes, exhausted counters and failed transactions.
    pub async fn reserve_positions(
        &self,
        scope: OutputScope,
        count: u32,
    ) -> Result<PositionReservation, HistoryError> {
        if count == 0 || count > 4096 {
            return Err(HistoryError::InvalidState);
        }
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_scope(&mut transaction, scope).await?;
        let next: i64 =
            sqlx::query_scalar("SELECT next_position FROM session_state WHERE session_id=?")
                .bind(scope.session.encode())
                .fetch_one(&mut *transaction)
                .await
                .map_err(storage)?;
        let start = u64::try_from(next).map_err(|_| HistoryError::InvalidState)?;
        let end = start
            .checked_add(u64::from(count))
            .filter(|value| *value <= MAX_VERSION)
            .ok_or(HistoryError::CounterExhausted)?;
        sqlx::query("UPDATE session_state SET next_position=? WHERE session_id=?")
            .bind(i64::try_from(end).map_err(|_| HistoryError::CounterExhausted)?)
            .bind(scope.session.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        transaction.commit().await.map_err(storage)?;
        Ok(PositionReservation { next: start, end })
    }
    /// Commits a batch atomically, keeping only the newest version of each stable item.
    /// # Errors
    /// Rejects conflicting identities/versions and leaves the entire batch unchanged on failure.
    // Identity checks, object replacement and revision acknowledgement share this transaction.
    #[allow(clippy::too_many_lines)]
    pub async fn commit_items(
        &self,
        scope: OutputScope,
        items: &[HistoryItem],
        fault: IntegrityChange,
    ) -> Result<Vec<CommitAck>, HistoryError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        let actor = require_scope(&mut transaction, scope).await?;
        let mut acknowledgements = Vec::with_capacity(items.len());
        let mut changed = false;
        for item in items {
            validate_item(scope, item)?;
            let reserved: i64 =
                sqlx::query_scalar("SELECT next_position FROM session_state WHERE session_id=?")
                    .bind(scope.session.encode())
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(storage)?;
            if i64::try_from(item.identity.position.value())
                .map_err(|_| HistoryError::CounterExhausted)?
                >= reserved
            {
                return Err(HistoryError::InvalidState);
            }
            let old=sqlx::query("SELECT i.session_id,i.turn_id,i.source_id,i.first_position,i.kind,i.protocol,i.protocol_version,i.native_tool_id,i.tool_instance_id,i.tool_generation,i.tool_native_session_id,i.content_version,i.content_ref,o.sha256 FROM conversation_items i JOIN content_objects o ON o.content_ref=i.content_ref WHERE i.item_id=?")
                .bind(item.identity.item.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?;
            let digest = hex::encode(Sha256::digest(&item.content.0));
            let old_reference = if let Some(row) = old {
                let same = row.try_get::<String, _>("session_id").map_err(storage)?
                    == scope.session.encode()
                    && row.try_get::<String, _>("turn_id").map_err(storage)? == scope.turn.encode()
                    && row.try_get::<String, _>("source_id").map_err(storage)?
                        == item.identity.source.encode()
                    && row.try_get::<i64, _>("first_position").map_err(storage)?
                        == i64::try_from(item.identity.position.value())
                            .map_err(|_| HistoryError::CounterExhausted)?
                    && row.try_get::<String, _>("kind").map_err(storage)?
                        == item.identity.kind.as_ref()
                    && row.try_get::<String, _>("protocol").map_err(storage)?
                        == item.identity.protocol.as_ref()
                    && row
                        .try_get::<String, _>("protocol_version")
                        .map_err(storage)?
                        == item.identity.protocol_version.as_ref()
                    && row
                        .try_get::<Option<String>, _>("native_tool_id")
                        .map_err(storage)?
                        .as_deref()
                        == item
                            .identity
                            .tool_call_id
                            .as_ref()
                            .map(NativeToolCallId::as_str)
                    && stored_origin(&row)? == item.identity.tool_origin;
                if !same {
                    return Err(HistoryError::IdentityConflict);
                }
                let version = ContentVersion::new(
                    u64::try_from(row.try_get::<i64, _>("content_version").map_err(storage)?)
                        .map_err(|_| HistoryError::InvalidState)?,
                )?;
                if version >= item.version {
                    if version == item.version
                        && row.try_get::<String, _>("sha256").map_err(storage)? != digest
                    {
                        return Err(HistoryError::VersionConflict);
                    }
                    acknowledgements.push(CommitAck {
                        item: item.identity.item,
                        version,
                    });
                    continue;
                }
                Some(row.try_get::<String, _>("content_ref").map_err(storage)?)
            } else {
                None
            };
            let reference = new_id()?;
            sqlx::query("INSERT INTO content_objects(content_ref,session_id,actor_id,auth_generation,purpose,body,sha256,total_bytes) VALUES(?,?,?,?,'output',?,?,?)")
                .bind(reference.encode()).bind(scope.session.encode()).bind(&actor.0).bind(actor.1).bind(item.content.0.as_ref()).bind(digest)
                .bind(i64::try_from(item.content.0.len()).map_err(|_|HistoryError::Capacity)?).execute(&mut *transaction).await.map_err(storage)?;
            sqlx::query("INSERT INTO conversation_items(item_id,session_id,turn_id,source_id,first_position,kind,protocol,protocol_version,native_tool_id,native_tool_digest,tool_instance_id,tool_generation,tool_native_session_id,tool_native_session_digest,content_ref,content_version,committed_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(item_id) DO UPDATE SET content_ref=excluded.content_ref,content_version=excluded.content_version,committed_version=excluded.committed_version WHERE excluded.content_version>conversation_items.content_version")
                .bind(item.identity.item.encode()).bind(scope.session.encode()).bind(scope.turn.encode()).bind(item.identity.source.encode())
                .bind(i64::try_from(item.identity.position.value()).map_err(|_|HistoryError::CounterExhausted)?).bind(item.identity.kind.as_ref()).bind(item.identity.protocol.as_ref()).bind(item.identity.protocol_version.as_ref())
                .bind(item.identity.tool_call_id.as_ref().map(NativeToolCallId::as_str))
                .bind(item.identity.tool_call_id.as_ref().map(|id|hex::encode(Sha256::digest(id.as_str().as_bytes())))).bind(item.identity.tool_origin.as_ref().map(|o|o.connection.instance.encode())).bind(item.identity.tool_origin.as_ref().map(|o|i64::try_from(o.connection.generation)).transpose().map_err(|_|HistoryError::CounterExhausted)?).bind(item.identity.tool_origin.as_ref().map(|o|o.native_session.as_str())).bind(item.identity.tool_origin.as_ref().map(|o|hex::encode(Sha256::digest(o.native_session.as_str().as_bytes()))))
                .bind(reference.encode()).bind(i64::try_from(item.version.value()).map_err(|_|HistoryError::CounterExhausted)?).bind(i64::try_from(item.version.value()).map_err(|_|HistoryError::CounterExhausted)?)
                .execute(&mut *transaction).await.map_err(storage)?;
            if let Some(reference) = old_reference {
                delete_unreferenced(&mut transaction, &reference).await?;
            }
            acknowledgements.push(CommitAck {
                item: item.identity.item,
                version: item.version,
            });
            changed = true;
        }
        match fault {
            IntegrityChange::Keep => {}
            IntegrityChange::Set(fault) => {
                write_fault(&mut transaction, scope, fault).await?;
                changed = true;
            }
        }
        if changed {
            let count=sqlx::query("UPDATE session_state SET revision=revision+1 WHERE session_id=? AND revision<9007199254740991").bind(scope.session.encode()).execute(&mut *transaction).await.map_err(storage)?;
            if count.rows_affected() != 1 {
                return Err(HistoryError::CounterExhausted);
            }
        }
        transaction.commit().await.map_err(storage)?;
        Ok(acknowledgements)
    }
    /// Reads a committed item without rewriting or flattening its original bytes.
    /// This repository method is internal data access, not a network permission grant.
    /// # Errors
    /// Rejects missing, inconsistent or unavailable history.
    pub async fn committed_item(
        &self,
        scope: OutputScope,
        item: ItemId,
    ) -> Result<CommittedItem, HistoryError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        require_scope(&mut transaction, scope).await?;
        let row=sqlx::query("SELECT i.source_id,i.first_position,i.kind,i.protocol,i.protocol_version,i.native_tool_id,i.tool_instance_id,i.tool_generation,i.tool_native_session_id,i.content_version,i.committed_version,i.content_ref,o.body,o.sha256 FROM conversation_items i JOIN content_objects o ON o.content_ref=i.content_ref WHERE i.item_id=? AND i.session_id=? AND i.turn_id=?")
            .bind(item.encode()).bind(scope.session.encode()).bind(scope.turn.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?.ok_or(HistoryError::InvalidState)?;
        let version: i64 = row.try_get("content_version").map_err(storage)?;
        if row
            .try_get::<i64, _>("committed_version")
            .map_err(storage)?
            != version
        {
            return Err(HistoryError::InvalidState);
        }
        let bytes: Vec<u8> = row.try_get("body").map_err(storage)?;
        if hex::encode(Sha256::digest(&bytes))
            != row.try_get::<String, _>("sha256").map_err(storage)?
        {
            return Err(HistoryError::InvalidState);
        }
        let identity = ItemIdentity {
            scope,
            item,
            source: SourceId(parse_id(
                &row.try_get::<String, _>("source_id").map_err(storage)?,
            )?),
            position: Position(
                u64::try_from(row.try_get::<i64, _>("first_position").map_err(storage)?)
                    .map_err(|_| HistoryError::InvalidState)?,
            ),
            kind: Arc::from(row.try_get::<String, _>("kind").map_err(storage)?),
            protocol: Arc::from(row.try_get::<String, _>("protocol").map_err(storage)?),
            protocol_version: Arc::from(
                row.try_get::<String, _>("protocol_version")
                    .map_err(storage)?,
            ),
            tool_origin: stored_origin(&row)?,
            tool_call_id: row
                .try_get::<Option<String>, _>("native_tool_id")
                .map_err(storage)?
                .map(|id| NativeToolCallId::new(Arc::from(id)))
                .transpose()?,
        };
        let reference = parse_id(&row.try_get::<String, _>("content_ref").map_err(storage)?)?;
        transaction.commit().await.map_err(storage)?;
        Ok(CommittedItem {
            item: HistoryItem {
                identity: Arc::new(identity),
                version: ContentVersion::new(
                    u64::try_from(version).map_err(|_| HistoryError::InvalidState)?,
                )?,
                content: RawContent(Arc::from(bytes)),
            },
            content_ref: reference,
        })
    }
    /// Looks up explicit native tool identity within one platform session, without reading mutable body.
    /// # Errors
    /// Rejects malformed metadata and storage errors; this is not a network authorization entry.
    pub async fn tool_reference(
        &self,
        scope: OutputScope,
        origin: &SourceOrigin,
        native: &NativeToolCallId,
    ) -> Result<Option<ToolReference>, HistoryError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        let result = resolve_tool_ref(&mut transaction, scope, origin, native).await?;
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }
    /// Reads one Run's durable integrity fact; other Runs' errors are never cleared by a new save.
    /// # Errors
    /// Rejects invalid scopes or malformed facts without inventing complete history.
    pub async fn integrity(
        &self,
        scope: OutputScope,
    ) -> Result<Option<IntegrityFault>, HistoryError> {
        let mut transaction = self.pool.begin().await.map_err(storage)?;
        require_scope(&mut transaction, scope).await?;
        let row=sqlx::query("SELECT reason,first_position,last_position,dropped_bytes FROM history_integrity WHERE session_id=? AND run_id=?").bind(scope.session.encode()).bind(scope.run.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?;
        let result = if let Some(row) = row {
            let reason = match row
                .try_get::<String, _>("reason")
                .map_err(storage)?
                .as_str()
            {
                "save_failed" => crate::domain::history::IntegrityReason::SaveFailed,
                "persistent_failure" => crate::domain::history::IntegrityReason::PersistentFailure,
                "capacity" => crate::domain::history::IntegrityReason::Capacity,
                "interrupted" => crate::domain::history::IntegrityReason::Interrupted,
                "cancelled" => crate::domain::history::IntegrityReason::Cancelled,
                _ => return Err(HistoryError::InvalidState),
            };
            let first: Option<i64> = row.try_get("first_position").map_err(storage)?;
            let last: Option<i64> = row.try_get("last_position").map_err(storage)?;
            let possible_loss = match (first, last) {
                (Some(first), Some(last)) => Some(MissingRange {
                    first: Position(u64::try_from(first).map_err(|_| HistoryError::InvalidState)?),
                    last: Position(u64::try_from(last).map_err(|_| HistoryError::InvalidState)?),
                    dropped_bytes: u64::try_from(
                        row.try_get::<i64, _>("dropped_bytes").map_err(storage)?,
                    )
                    .map_err(|_| HistoryError::InvalidState)?,
                }),
                (None, None) => None,
                _ => return Err(HistoryError::InvalidState),
            };
            Some(IntegrityFault {
                reason,
                possible_loss,
            })
        } else {
            None
        };
        transaction.commit().await.map_err(storage)?;
        Ok(result)
    }
    /// Pins immutable bytes for an owning reader; the owner must release or clear pins on startup.
    /// Protocol expiry and per-block authentication remain gateway responsibilities.
    /// # Errors
    /// Rejects unavailable versions and failed pin transactions.
    pub async fn pin_version(
        &self,
        scope: OutputScope,
        item: ItemId,
    ) -> Result<ReadPin, HistoryError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        require_scope(&mut transaction, scope).await?;
        let row=sqlx::query("SELECT content_ref,content_version FROM conversation_items WHERE item_id=? AND session_id=? AND turn_id=?").bind(item.encode()).bind(scope.session.encode()).bind(scope.turn.encode()).fetch_optional(&mut *transaction).await.map_err(storage)?.ok_or(HistoryError::InvalidState)?;
        let reference = parse_id(&row.try_get::<String, _>("content_ref").map_err(storage)?)?;
        let pin = new_id()?;
        sqlx::query("INSERT INTO history_content_pins VALUES(?,?)")
            .bind(pin.encode())
            .bind(reference.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        let version = ContentVersion::new(
            u64::try_from(row.try_get::<i64, _>("content_version").map_err(storage)?)
                .map_err(|_| HistoryError::InvalidState)?,
        )?;
        transaction.commit().await.map_err(storage)?;
        Ok(ReadPin {
            pin,
            content_ref: reference,
            version,
        })
    }
    /// # Errors
    /// Returns storage errors without deleting a still-referenced object.
    pub async fn release_pin(&self, pin: ReadPin) -> Result<(), HistoryError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage)?;
        sqlx::query("DELETE FROM history_content_pins WHERE pin_id=? AND content_ref=?")
            .bind(pin.pin.encode())
            .bind(pin.content_ref.encode())
            .execute(&mut *transaction)
            .await
            .map_err(storage)?;
        delete_unreferenced(&mut transaction, &pin.content_ref.encode()).await?;
        transaction.commit().await.map_err(storage)
    }
}
impl HistoryRepository for HistoryStore {
    async fn reserve(
        &self,
        scope: OutputScope,
        count: u32,
    ) -> Result<PositionReservation, HistoryError> {
        self.reserve_positions(scope, count).await
    }
    async fn save(
        &self,
        scope: OutputScope,
        items: &[HistoryItem],
        fault: IntegrityChange,
    ) -> Result<Vec<CommitAck>, HistoryError> {
        self.commit_items(scope, items, fault).await
    }
}
fn stored_origin(row: &sqlx::sqlite::SqliteRow) -> Result<Option<SourceOrigin>, HistoryError> {
    let instance: Option<String> = row.try_get("tool_instance_id").map_err(storage)?;
    instance
        .map(|id| {
            Ok(SourceOrigin {
                connection: ConnectionEpoch {
                    instance: InstanceId(parse_id(&id)?),
                    generation: u64::try_from(
                        row.try_get::<i64, _>("tool_generation").map_err(storage)?,
                    )
                    .map_err(|_| HistoryError::InvalidState)?,
                },
                native_session: NativeSessionId::new(Arc::from(
                    row.try_get::<String, _>("tool_native_session_id")
                        .map_err(storage)?,
                ))?,
            })
        })
        .transpose()
}
pub(super) async fn resolve_tool_ref(
    connection: &mut SqliteConnection,
    scope: OutputScope,
    origin: &SourceOrigin,
    native: &NativeToolCallId,
) -> Result<Option<ToolReference>, HistoryError> {
    require_scope(connection, scope).await?;
    let rows=sqlx::query("SELECT item_id,source_id,native_tool_id,tool_native_session_id FROM conversation_items WHERE session_id=? AND turn_id=? AND tool_instance_id=? AND tool_generation=? AND tool_native_session_digest=? AND native_tool_digest=? AND kind='tool' LIMIT 2").bind(scope.session.encode()).bind(scope.turn.encode()).bind(origin.connection.instance.encode()).bind(i64::try_from(origin.connection.generation).map_err(|_|HistoryError::CounterExhausted)?).bind(hex::encode(Sha256::digest(origin.native_session.as_str().as_bytes()))).bind(hex::encode(Sha256::digest(native.as_str().as_bytes()))).fetch_all(connection).await.map_err(storage)?;
    if rows.len() > 1 {
        return Err(HistoryError::IdentityConflict);
    }
    rows.into_iter()
        .next()
        .map(|row| {
            if row
                .try_get::<String, _>("native_tool_id")
                .map_err(storage)?
                != native.as_str()
                || row
                    .try_get::<String, _>("tool_native_session_id")
                    .map_err(storage)?
                    != origin.native_session.as_str()
            {
                return Err(HistoryError::IdentityConflict);
            }
            Ok(ToolReference {
                session: scope.session,
                item: ItemId(parse_id(
                    &row.try_get::<String, _>("item_id").map_err(storage)?,
                )?),
                source: SourceId(parse_id(
                    &row.try_get::<String, _>("source_id").map_err(storage)?,
                )?),
            })
        })
        .transpose()
}
fn storage(_: sqlx::Error) -> HistoryError {
    HistoryError::StorageUnavailable
}
fn parse_id(value: &str) -> Result<EntityId, HistoryError> {
    EntityId::decode(value).map_err(|_| HistoryError::InvalidState)
}
fn new_id() -> Result<EntityId, HistoryError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| HistoryError::RandomUnavailable)
}
async fn require_scope(
    connection: &mut SqliteConnection,
    scope: OutputScope,
) -> Result<(String, i64), HistoryError> {
    let row=sqlx::query("SELECT c.actor_id,cl.auth_generation FROM runs r JOIN session_commands c ON c.command_id=r.command_id JOIN clients cl ON cl.client_id=c.actor_id WHERE r.run_id=? AND r.session_id=? AND r.turn_id=?")
        .bind(scope.run.encode()).bind(scope.session.encode()).bind(scope.turn.encode()).fetch_optional(connection).await.map_err(storage)?.ok_or(HistoryError::InvalidState)?;
    Ok((
        row.try_get("actor_id").map_err(storage)?,
        row.try_get("auth_generation").map_err(storage)?,
    ))
}
fn validate_item(scope: OutputScope, item: &HistoryItem) -> Result<(), HistoryError> {
    if item.identity.scope != scope
        || item.identity.kind.is_empty()
        || item.identity.kind.len() > 128
        || item.identity.protocol.is_empty()
        || item.identity.protocol.len() > 64
        || item.identity.protocol_version.is_empty()
        || item.identity.protocol_version.len() > 64
    {
        return Err(HistoryError::IdentityConflict);
    }
    if item.content.0.len() > 16 * 1024 * 1024 {
        return Err(HistoryError::Capacity);
    }
    Ok(())
}
async fn delete_unreferenced(
    connection: &mut SqliteConnection,
    reference: &str,
) -> Result<(), HistoryError> {
    sqlx::query("DELETE FROM content_objects WHERE content_ref=? AND purpose='output' AND NOT EXISTS(SELECT 1 FROM conversation_items WHERE content_ref=?) AND NOT EXISTS(SELECT 1 FROM history_content_pins WHERE content_ref=?)")
        .bind(reference).bind(reference).bind(reference).execute(connection).await.map_err(storage)?;
    Ok(())
}
pub(super) async fn write_fault(
    connection: &mut SqliteConnection,
    scope: OutputScope,
    fault: IntegrityFault,
) -> Result<(), HistoryError> {
    let (first, last, dropped) = match fault.possible_loss {
        Some(MissingRange {
            first,
            last,
            dropped_bytes,
        }) => (
            Some(i64::try_from(first.value()).map_err(|_| HistoryError::CounterExhausted)?),
            Some(i64::try_from(last.value()).map_err(|_| HistoryError::CounterExhausted)?),
            i64::try_from(dropped_bytes).map_err(|_| HistoryError::CounterExhausted)?,
        ),
        None => (None, None, 0),
    };
    sqlx::query("INSERT INTO history_integrity VALUES(?,?,?,?,?,?) ON CONFLICT(session_id,run_id) DO UPDATE SET reason=excluded.reason,first_position=CASE WHEN history_integrity.first_position IS NULL THEN excluded.first_position WHEN excluded.first_position IS NULL THEN history_integrity.first_position ELSE MIN(history_integrity.first_position,excluded.first_position) END,last_position=CASE WHEN history_integrity.last_position IS NULL THEN excluded.last_position WHEN excluded.last_position IS NULL THEN history_integrity.last_position ELSE MAX(history_integrity.last_position,excluded.last_position) END,dropped_bytes=MAX(history_integrity.dropped_bytes,excluded.dropped_bytes)")
        .bind(scope.session.encode()).bind(scope.run.encode()).bind(fault.reason.code()).bind(first).bind(last).bind(dropped).execute(connection).await.map_err(storage)?;
    Ok(())
}
#[cfg(test)]
mod tests;
