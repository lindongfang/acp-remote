#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use p256::ecdsa::VerifyingKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, watch};

use crate::{error::StorageError, jws::decode_fixed_base64url, random};

pub const CLIENT_PAGE_SIZE: i64 = 32;
const CLIENT_PAGE_SIZE_USIZE: usize = 32;
const CURSOR_LIFETIME: Duration = Duration::from_secs(60);
const MAX_ACTIVE_CURSORS: usize = 1_024;
const REVOKE_RESULT_LIFETIME_SECONDS: i64 = 24 * 60 * 60;
const MAX_REVOKE_RESULTS: i64 = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientStatus {
    Trusted,
    Revoked,
}

impl ClientStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ClientSummary {
    pub client_id: String,
    pub display_name: String,
    pub status: ClientStatus,
    pub public_key_fingerprint: String,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ClientPage {
    pub items: Vec<ClientSummary>,
    pub next_cursor: Option<String>,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RevokeResult {
    pub client_id: String,
    pub status: ClientStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedClientRecord {
    pub client_id: String,
    pub public_key: Vec<u8>,
    pub display_name: String,
    pub status: ClientStatus,
    pub auth_generation: i64,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
}

#[derive(Debug, Error)]
pub enum TrustError {
    #[error("client was not found")]
    NotFound,
    #[error("request is invalid")]
    InvalidRequest,
    #[error("request identifier was reused with different content")]
    RequestConflict,
    #[error("client public key is already registered")]
    PublicKeyAlreadyRegistered,
    #[error("generated client identifier collided")]
    ClientIdCollision,
    #[error("cursor is stale or does not belong to this actor")]
    StaleCursor,
    #[error("trust storage is unavailable")]
    StorageUnavailable,
    #[error("trust storage capacity is exhausted")]
    CapacityExhausted,
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub trait TrustClock: Send + Sync {
    /// Returns the current UTC Unix timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when the clock is unavailable or outside the supported range.
    fn unix_seconds(&self) -> Result<u64, String>;

    /// Returns a monotonic instant used for cursor expiry.
    ///
    /// # Errors
    ///
    /// Returns an error when the monotonic clock is unavailable.
    fn monotonic_now(&self) -> Result<Instant, String>;
}

#[derive(Debug)]
struct SystemClock;

impl TrustClock for SystemClock {
    fn unix_seconds(&self) -> Result<u64, String> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|error| error.to_string())
    }

    fn monotonic_now(&self) -> Result<Instant, String> {
        Ok(Instant::now())
    }
}

#[derive(Clone, Debug)]
struct CursorEntry {
    actor: String,
    revision: i64,
    offset: i64,
    deadline: Instant,
}

#[derive(Debug, Default)]
struct CursorState {
    entries: HashMap<String, CursorEntry>,
}

impl CursorState {
    fn prune_expired(&mut self, now: Instant) {
        self.entries.retain(|_, entry| entry.deadline > now);
    }
}

#[derive(Clone)]
pub struct TrustStore {
    pool: SqlitePool,
    cursors: Arc<Mutex<CursorState>>,
    client_gates: Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>,
    clock: Arc<dyn TrustClock>,
    revision_notify: Arc<watch::Sender<u64>>,
    #[cfg(test)]
    fail_next_commit: Arc<AtomicBool>,
    #[cfg(test)]
    list_page_barrier: Arc<Mutex<Option<crate::test_support::SharedAsyncBarrier>>>,
}

impl TrustStore {
    #[must_use]
    pub(crate) fn new(pool: SqlitePool) -> Self {
        let (revision_notify, _) = watch::channel(0);
        Self {
            pool,
            cursors: Arc::new(Mutex::new(CursorState::default())),
            client_gates: Arc::new(Mutex::new(HashMap::new())),
            clock: Arc::new(SystemClock),
            revision_notify: Arc::new(revision_notify),
            #[cfg(test)]
            fail_next_commit: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            list_page_barrier: Arc::new(Mutex::new(None)),
        }
    }

    /// Subscribes to committed device-list revision changes.
    #[must_use]
    pub fn revision_receiver(&self) -> watch::Receiver<u64> {
        self.revision_notify.subscribe()
    }

    pub(crate) fn client_gate(&self, client_id: &str) -> Arc<AsyncMutex<()>> {
        let mut gates = self
            .client_gates
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if gates.len() > 64 {
            gates.retain(|_, gate| gate.strong_count() > 0);
        }
        if let Some(gate) = gates.get(client_id).and_then(Weak::upgrade) {
            return gate;
        }
        let gate = Arc::new(AsyncMutex::new(()));
        gates.insert(client_id.to_owned(), Arc::downgrade(&gate));
        gate
    }

    #[cfg(test)]
    pub(crate) fn fail_next_commit_for_test(&self) {
        self.fail_next_commit.store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn pause_next_list_page(&self) -> crate::test_support::SharedAsyncBarrier {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slot = self
            .list_page_barrier
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        assert!(
            slot.is_none(),
            "only one list-page barrier can be installed"
        );
        *slot = Some(Arc::clone(&barrier));
        barrier
    }

    /// Reads the current committed device-list revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision row cannot be read or is invalid.
    pub async fn current_revision(&self) -> Result<u64, TrustError> {
        let revision = self.revision().await?;
        u64::try_from(revision).map_err(|_| TrustError::StorageUnavailable)
    }

    /// Lists one bounded page of registered clients.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid actor, stale cursor, storage failure, or unavailable
    /// randomness when a continuation cursor is required.
    pub async fn list_page(
        &self,
        actor: &str,
        cursor: Option<&str>,
    ) -> Result<ClientPage, TrustError> {
        if actor.is_empty() {
            return Err(TrustError::InvalidRequest);
        }
        let mut transaction = self.pool.begin().await?;
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        #[cfg(test)]
        {
            let barrier = self
                .list_page_barrier
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            if let Some(barrier) = barrier {
                barrier.pause().await;
            }
        }
        let offset = match cursor {
            None => 0,
            Some(cursor) => self.resolve_cursor(actor, revision, cursor)?,
        };
        let rows = sqlx::query(
            "SELECT client_id, display_name, status, public_key, created_at, revoked_at \
             FROM clients ORDER BY client_id ASC LIMIT ? OFFSET ?",
        )
        .bind(CLIENT_PAGE_SIZE + 1)
        .bind(offset)
        .fetch_all(&mut *transaction)
        .await?;

        let has_more = rows.len() > usize::try_from(CLIENT_PAGE_SIZE).unwrap_or(usize::MAX);
        let mut items = Vec::with_capacity(rows.len().min(CLIENT_PAGE_SIZE_USIZE));
        for row in rows.into_iter().take(CLIENT_PAGE_SIZE_USIZE) {
            items.push(client_summary_from_row(&row)?);
        }
        let response_revision =
            u64::try_from(revision).map_err(|_| TrustError::StorageUnavailable)?;
        transaction.commit().await?;
        let next_cursor = if has_more {
            Some(self.issue_cursor(actor, revision, offset + CLIENT_PAGE_SIZE)?)
        } else {
            None
        };
        Ok(ClientPage {
            items,
            next_cursor,
            revision: response_revision,
        })
    }

    /// Revokes one client and records the idempotent management result.
    ///
    /// # Errors
    ///
    /// Returns an error when the request is malformed, the client does not exist, the request
    /// identifier was reused with different content, capacity is exhausted, or the transaction
    /// cannot be committed.
    #[allow(clippy::too_many_lines)]
    pub async fn revoke(
        &self,
        actor: &str,
        request_id: &str,
        client_id: &str,
    ) -> Result<RevokeResult, TrustError> {
        if actor.is_empty() || !is_id128(request_id) || !is_id128(client_id) {
            return Err(TrustError::InvalidRequest);
        }
        let gate = self.client_gate(client_id);
        let _gate = gate.lock().await;
        let now = self.unix_seconds()?;
        let digest = revoke_payload_digest(client_id);
        let mut transaction = self.pool.begin().await?;

        if let Some(row) = sqlx::query(
            "SELECT payload_digest, result_json, expires_at \
             FROM management_commands WHERE actor_id = ? AND request_id = ?",
        )
        .bind(actor)
        .bind(request_id)
        .fetch_optional(&mut *transaction)
        .await?
        {
            let expires_at: i64 = row.get("expires_at");
            if expires_at > now_i64(now)? {
                let stored_digest: String = row.get("payload_digest");
                if stored_digest != digest {
                    return Err(TrustError::RequestConflict);
                }
                let result_json: String = row.get("result_json");
                return serde_json::from_str(&result_json)
                    .map_err(|_| TrustError::StorageUnavailable);
            }
        }

        sqlx::query("DELETE FROM management_commands WHERE expires_at <= ?")
            .bind(now_i64(now)?)
            .execute(&mut *transaction)
            .await?;
        let result_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM management_commands")
            .fetch_one(&mut *transaction)
            .await?;
        if result_count >= MAX_REVOKE_RESULTS {
            return Err(TrustError::CapacityExhausted);
        }

        let client = sqlx::query("SELECT status, revoked_at FROM clients WHERE client_id = ?")
            .bind(client_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(TrustError::NotFound)?;
        let stored_status: String = client.get("status");
        let outcome = if stored_status == ClientStatus::Trusted.as_str() {
            sqlx::query(
                "UPDATE clients SET status = 'revoked', auth_generation = auth_generation + 1, \
                 revoked_at = ? WHERE client_id = ? AND status = 'trusted'",
            )
            .bind(now_i64(now)?)
            .bind(client_id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query("UPDATE node_state SET revision = revision + 1 WHERE singleton = 1")
                .execute(&mut *transaction)
                .await?;
            "revoked"
        } else {
            "already_revoked"
        };

        sqlx::query(
            "INSERT INTO security_audit(actor_id, action, target_id, outcome, occurred_at) \
             VALUES (?, 'device.revoke', ?, ?, ?)",
        )
        .bind(actor)
        .bind(client_id)
        .bind(outcome)
        .bind(now_i64(now)?)
        .execute(&mut *transaction)
        .await?;

        let result = RevokeResult {
            client_id: client_id.to_owned(),
            status: ClientStatus::Revoked,
        };
        let result_json =
            serde_json::to_string(&result).map_err(|_| TrustError::StorageUnavailable)?;
        sqlx::query(
            "INSERT INTO management_commands(
                actor_id, request_id, action, payload_digest, result_json, expires_at
             ) VALUES (?, ?, 'device.revoke', ?, ?, ?) \
             ON CONFLICT(actor_id, request_id) DO UPDATE SET \
                action = excluded.action, payload_digest = excluded.payload_digest, \
                result_json = excluded.result_json, expires_at = excluded.expires_at",
        )
        .bind(actor)
        .bind(request_id)
        .bind(digest)
        .bind(result_json)
        .bind(now_i64(now)?.saturating_add(REVOKE_RESULT_LIFETIME_SECONDS))
        .execute(&mut *transaction)
        .await?;
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        let revision = u64::try_from(revision).map_err(|_| TrustError::StorageUnavailable)?;
        #[cfg(test)]
        if self.fail_next_commit.swap(false, Ordering::SeqCst) {
            return Err(TrustError::StorageUnavailable);
        }
        transaction.commit().await?;
        self.publish_revision(revision);
        Ok(result)
    }

    /// Registers a newly paired client in one durable transaction.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier or public key is already registered, the request is
    /// malformed, or the transaction cannot be committed.
    pub async fn register_client(
        &self,
        client_id: &str,
        public_key: &[u8],
        display_name: &str,
    ) -> Result<TrustedClientRecord, TrustError> {
        if !is_id128(client_id)
            || public_key.len() != 65
            || public_key.first() != Some(&4)
            || VerifyingKey::from_sec1_bytes(public_key).is_err()
            || display_name.is_empty()
        {
            return Err(TrustError::InvalidRequest);
        }
        let now = self.unix_seconds()?;
        let now = now_i64(now)?;
        let mut transaction = self.pool.begin().await?;
        if sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM clients WHERE client_id = ? OR public_key = ?",
        )
        .bind(client_id)
        .bind(public_key)
        .fetch_one(&mut *transaction)
        .await?
            != 0
        {
            let key_exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE public_key = ?")
                    .bind(public_key)
                    .fetch_one(&mut *transaction)
                    .await?;
            return if key_exists == 0 {
                Err(TrustError::ClientIdCollision)
            } else {
                Err(TrustError::PublicKeyAlreadyRegistered)
            };
        }
        sqlx::query(
            "INSERT INTO clients(
                client_id, public_key, display_name, status, auth_generation, created_at
             ) VALUES (?, ?, ?, 'trusted', 0, ?)",
        )
        .bind(client_id)
        .bind(public_key)
        .bind(display_name)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE node_state SET revision = revision + 1 WHERE singleton = 1")
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO security_audit(actor_id, action, target_id, outcome, occurred_at) \
             VALUES ('local-owner', 'device.pair.approve', ?, 'trusted', ?)",
        )
        .bind(client_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let revision: i64 =
            sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
                .fetch_one(&mut *transaction)
                .await?;
        let revision = u64::try_from(revision).map_err(|_| TrustError::StorageUnavailable)?;
        transaction.commit().await?;
        self.publish_revision(revision);
        Ok(TrustedClientRecord {
            client_id: client_id.to_owned(),
            public_key: public_key.to_vec(),
            display_name: display_name.to_owned(),
            status: ClientStatus::Trusted,
            auth_generation: 0,
            created_at: u64::try_from(now).map_err(|_| TrustError::StorageUnavailable)?,
            revoked_at: None,
        })
    }

    /// Returns a registered client by public key, including revoked records.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage query fails or a stored value is invalid.
    pub async fn client_by_public_key(
        &self,
        public_key: &[u8],
    ) -> Result<Option<TrustedClientRecord>, TrustError> {
        let row = sqlx::query(
            "SELECT client_id, public_key, display_name, status, auth_generation, \
                    created_at, revoked_at FROM clients WHERE public_key = ?",
        )
        .bind(public_key)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(client_record_from_row).transpose()
    }

    /// Returns a registered client by stable client identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage query fails or a stored value is invalid.
    pub async fn client_by_id(
        &self,
        client_id: &str,
    ) -> Result<Option<TrustedClientRecord>, TrustError> {
        let row = sqlx::query(
            "SELECT client_id, public_key, display_name, status, auth_generation, \
                    created_at, revoked_at FROM clients WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(client_record_from_row).transpose()
    }

    async fn revision(&self) -> Result<i64, TrustError> {
        sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
            .fetch_one(&self.pool)
            .await
            .map_err(TrustError::from)
    }

    fn publish_revision(&self, revision: u64) {
        self.revision_notify.send_replace(revision);
    }

    fn resolve_cursor(&self, actor: &str, revision: i64, cursor: &str) -> Result<i64, TrustError> {
        let now = self.monotonic_now()?;
        let mut state = self.cursors.lock().unwrap_or_else(PoisonError::into_inner);
        state.prune_expired(now);
        let entry = state.entries.get(cursor).ok_or(TrustError::StaleCursor)?;
        if entry.actor != actor || entry.revision != revision || entry.deadline <= now {
            return Err(TrustError::StaleCursor);
        }
        let offset = entry.offset;
        drop(state);
        Ok(offset)
    }

    fn issue_cursor(&self, actor: &str, revision: i64, offset: i64) -> Result<String, TrustError> {
        let now = self.monotonic_now()?;
        let deadline = now
            .checked_add(CURSOR_LIFETIME)
            .ok_or(TrustError::StorageUnavailable)?;
        let cursor = random::hex(16).map_err(|_| TrustError::StorageUnavailable)?;
        let mut state = self.cursors.lock().unwrap_or_else(PoisonError::into_inner);
        state.prune_expired(now);
        if state.entries.len() >= MAX_ACTIVE_CURSORS {
            return Err(TrustError::CapacityExhausted);
        }
        state.entries.insert(
            cursor.clone(),
            CursorEntry {
                actor: actor.to_owned(),
                revision,
                offset,
                deadline,
            },
        );
        drop(state);
        Ok(cursor)
    }

    fn unix_seconds(&self) -> Result<u64, TrustError> {
        self.clock
            .unix_seconds()
            .map_err(|_| TrustError::StorageUnavailable)
    }

    fn monotonic_now(&self) -> Result<Instant, TrustError> {
        self.clock
            .monotonic_now()
            .map_err(|_| TrustError::StorageUnavailable)
    }
}

fn client_summary_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ClientSummary, TrustError> {
    let status: String = row.get("status");
    let status = match status.as_str() {
        "trusted" => ClientStatus::Trusted,
        "revoked" => ClientStatus::Revoked,
        _ => return Err(TrustError::StorageUnavailable),
    };
    let public_key: Vec<u8> = row.get("public_key");
    let created_at: i64 = row.get("created_at");
    let revoked_at: Option<i64> = row.get("revoked_at");
    Ok(ClientSummary {
        client_id: row.get("client_id"),
        display_name: row.get("display_name"),
        status,
        public_key_fingerprint: hex::encode(Sha256::digest(public_key)),
        created_at: u64::try_from(created_at).map_err(|_| TrustError::StorageUnavailable)?,
        revoked_at: revoked_at
            .map(u64::try_from)
            .transpose()
            .map_err(|_| TrustError::StorageUnavailable)?,
    })
}

fn client_record_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<TrustedClientRecord, TrustError> {
    let status: String = row.get("status");
    let status = match status.as_str() {
        "trusted" => ClientStatus::Trusted,
        "revoked" => ClientStatus::Revoked,
        _ => return Err(TrustError::StorageUnavailable),
    };
    let created_at: i64 = row.get("created_at");
    let revoked_at: Option<i64> = row.get("revoked_at");
    Ok(TrustedClientRecord {
        client_id: row.get("client_id"),
        public_key: row.get("public_key"),
        display_name: row.get("display_name"),
        status,
        auth_generation: row.get("auth_generation"),
        created_at: u64::try_from(created_at).map_err(|_| TrustError::StorageUnavailable)?,
        revoked_at: revoked_at
            .map(u64::try_from)
            .transpose()
            .map_err(|_| TrustError::StorageUnavailable)?,
    })
}

fn revoke_payload_digest(client_id: &str) -> String {
    // The closed v1 payload contains only fixed ASCII field names and a canonical Id128.
    let canonical =
        format!(r#"{{"action":"device.revoke","payload":{{"clientId":"{client_id}"}}}}"#);
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn now_i64(now: u64) -> Result<i64, TrustError> {
    i64::try_from(now).map_err(|_| TrustError::StorageUnavailable)
}

fn is_id128(value: &str) -> bool {
    decode_fixed_base64url(value, 16).is_ok()
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::claims::Id128;
    use crate::storage::Storage;

    struct TestClock {
        unix: Mutex<u64>,
        monotonic: Mutex<Instant>,
    }

    impl TestClock {
        fn new() -> Self {
            Self {
                unix: Mutex::new(1_700_000_000),
                monotonic: Mutex::new(Instant::now()),
            }
        }

        fn advance(&self, seconds: u64) {
            *self.unix.lock().expect("test wall clock") += seconds;
            *self.monotonic.lock().expect("test monotonic clock") += Duration::from_secs(seconds);
        }
    }

    impl TrustClock for TestClock {
        fn unix_seconds(&self) -> Result<u64, String> {
            Ok(*self.unix.lock().map_err(|error| error.to_string())?)
        }

        fn monotonic_now(&self) -> Result<Instant, String> {
            Ok(*self.monotonic.lock().map_err(|error| error.to_string())?)
        }
    }

    fn test_database() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "acp-remote-trust-{}-{nonce}.sqlite3",
            std::process::id()
        ))
    }

    async fn insert_client(storage: &Storage, client_id: &str, name: &str, status: &str) {
        let first = Sha256::digest(client_id.as_bytes());
        let second = Sha256::digest(first);
        let mut public_key = Vec::with_capacity(65);
        public_key.push(4);
        public_key.extend_from_slice(&first);
        public_key.extend_from_slice(&second);
        sqlx::query(
            "INSERT INTO clients(
                client_id, public_key, display_name, status, auth_generation, created_at, revoked_at
             ) VALUES (?, ?, ?, ?, 1, 1_700_000_000, ?)",
        )
        .bind(client_id)
        .bind(public_key)
        .bind(name)
        .bind(status)
        .bind((status == "revoked").then_some(1_700_000_001_i64))
        .execute(storage.pool_for_test())
        .await
        .expect("insert client");
    }

    #[tokio::test]
    async fn paginates_at_32_and_binds_cursors_to_actor_and_revision() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        for value in 0_u8..70 {
            insert_client(
                &storage,
                &Id128::new([value; 16]).to_base64url(),
                &format!("client-{value}"),
                "trusted",
            )
            .await;
        }
        let trust = TrustStore::new(storage.pool_for_test().clone());
        let first = trust.list_page("owner", None).await.expect("first page");
        assert_eq!(first.items.len(), 32);
        assert_eq!(first.revision, 0);
        let cursor = first.next_cursor.expect("continuation");

        assert!(matches!(
            trust.list_page("different-actor", Some(&cursor)).await,
            Err(TrustError::StaleCursor)
        ));
        let second = trust
            .list_page("owner", Some(&cursor))
            .await
            .expect("second page");
        assert_eq!(second.items.len(), 32);
        let third = trust
            .list_page("owner", second.next_cursor.as_deref())
            .await
            .expect("third page");
        assert_eq!(third.items.len(), 6);
        assert!(third.next_cursor.is_none());

        storage.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn list_page_uses_one_snapshot_for_revision_items_and_cursor() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        for value in 0_u8..34 {
            insert_client(
                &storage,
                &Id128::new([value; 16]).to_base64url(),
                &format!("client-{value}"),
                "trusted",
            )
            .await;
        }
        let writer = Storage::open(&path).await.expect("writer storage");
        let trust = TrustStore::new(storage.pool_for_test().clone());
        let barrier = trust.pause_next_list_page();
        let list_trust = trust.clone();
        let listing = tokio::spawn(async move { list_trust.list_page("owner", None).await });
        barrier.wait_until_entered().await;

        let first_id = Id128::new([0; 16]).to_base64url();
        let mut transaction = writer.pool_for_test().begin().await.expect("writer tx");
        sqlx::query("UPDATE clients SET display_name = 'changed' WHERE client_id = ?")
            .bind(&first_id)
            .execute(&mut *transaction)
            .await
            .expect("change client");
        sqlx::query("UPDATE node_state SET revision = revision + 1 WHERE singleton = 1")
            .execute(&mut *transaction)
            .await
            .expect("change revision");
        tokio::time::timeout(Duration::from_secs(2), transaction.commit())
            .await
            .expect("writer must not wait for the page snapshot")
            .expect("commit writer");

        barrier.release();
        let page = listing.await.expect("listing task").expect("snapshot page");
        assert_eq!(page.revision, 0);
        assert_eq!(page.items[0].display_name, "client-0");
        let cursor = page.next_cursor.expect("snapshot cursor");
        assert!(matches!(
            trust.list_page("owner", Some(&cursor)).await,
            Err(TrustError::StaleCursor)
        ));
        let fresh = trust.list_page("owner", None).await.expect("fresh page");
        assert_eq!(fresh.revision, 1);
        assert_eq!(fresh.items[0].display_name, "changed");

        storage.close().await;
        writer.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn revoke_is_atomic_idempotent_and_persistent() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        let client_id = Id128::new([0; 16]).to_base64url();
        insert_client(&storage, &client_id, "browser", "trusted").await;
        let clock = Arc::new(TestClock::new());
        let trust = TrustStore {
            pool: storage.pool_for_test().clone(),
            cursors: Arc::new(Mutex::new(CursorState::default())),
            client_gates: Arc::new(Mutex::new(HashMap::new())),
            clock,
            revision_notify: Arc::new(watch::channel(0).0),
            fail_next_commit: Arc::new(AtomicBool::new(false)),
            list_page_barrier: Arc::new(Mutex::new(None)),
        };
        let request_id = Id128::new([1; 16]).to_base64url();

        let first = trust
            .revoke("owner", &request_id, &client_id)
            .await
            .expect("revoke");
        assert_eq!(first.status, ClientStatus::Revoked);
        let repeated = trust
            .revoke("owner", &request_id, &client_id)
            .await
            .expect("same request");
        assert_eq!(repeated, first);
        assert!(matches!(
            trust
                .revoke("owner", &request_id, &Id128::new([2; 16]).to_base64url())
                .await,
            Err(TrustError::RequestConflict)
        ));

        let page = trust.list_page("owner", None).await.expect("list");
        assert_eq!(page.revision, 1);
        assert_eq!(page.items[0].status, ClientStatus::Revoked);
        assert!(page.items[0].revoked_at.is_some());

        let second_request = Id128::new([3; 16]).to_base64url();
        trust
            .revoke("owner", &second_request, &client_id)
            .await
            .expect("repeat revoke with a new request");
        assert_eq!(
            trust.list_page("owner", None).await.expect("list").revision,
            1
        );

        storage.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn expired_revoke_request_id_accepts_changed_content_at_the_deadline() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        let first_id = Id128::new([10; 16]).to_base64url();
        let second_id = Id128::new([11; 16]).to_base64url();
        insert_client(&storage, &first_id, "first", "trusted").await;
        insert_client(&storage, &second_id, "second", "trusted").await;
        let clock = Arc::new(TestClock::new());
        let trust = TrustStore {
            pool: storage.pool_for_test().clone(),
            cursors: Arc::new(Mutex::new(CursorState::default())),
            client_gates: Arc::new(Mutex::new(HashMap::new())),
            clock: clock.clone(),
            revision_notify: Arc::new(watch::channel(0).0),
            fail_next_commit: Arc::new(AtomicBool::new(false)),
            list_page_barrier: Arc::new(Mutex::new(None)),
        };
        let request_id = Id128::new([12; 16]).to_base64url();
        trust
            .revoke("owner", &request_id, &first_id)
            .await
            .expect("first revoke");
        clock.advance(86_399);
        assert!(matches!(
            trust.revoke("owner", &request_id, &second_id).await,
            Err(TrustError::RequestConflict)
        ));
        clock.advance(1);
        let result = trust
            .revoke("owner", &request_id, &second_id)
            .await
            .expect("expired ID is reusable");
        assert_eq!(result.client_id, second_id);
        assert_eq!(
            trust
                .revoke("owner", &request_id, &second_id)
                .await
                .expect("new result is cached"),
            result
        );
        assert_eq!(
            trust.list_page("owner", None).await.expect("list").revision,
            2
        );
        drop(trust);
        storage.close().await;
        let reopened = Storage::open(&path).await.expect("reopen storage");
        let revoked: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE status = 'revoked'")
                .fetch_one(reopened.pool_for_test())
                .await
                .expect("persisted revoked clients");
        assert_eq!(revoked, 2);
        reopened.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[tokio::test]
    async fn revoke_statement_and_commit_failures_roll_back_entirely() {
        let cases = [
            (
                "client update",
                "CREATE TRIGGER fail_revoke_client BEFORE UPDATE ON clients \
                 WHEN NEW.status = 'revoked' \
                 BEGIN SELECT RAISE(ABORT, 'injected client update failure'); END",
            ),
            (
                "revision update",
                "CREATE TRIGGER fail_revoke_revision BEFORE UPDATE OF revision ON node_state \
                 BEGIN SELECT RAISE(ABORT, 'injected revision update failure'); END",
            ),
            (
                "audit insert",
                "CREATE TRIGGER fail_revoke_audit BEFORE INSERT ON security_audit \
                 WHEN NEW.action = 'device.revoke' \
                 BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END",
            ),
            (
                "idempotent result insert",
                "CREATE TRIGGER fail_revoke_result BEFORE INSERT ON management_commands \
                 WHEN NEW.action = 'device.revoke' \
                 BEGIN SELECT RAISE(ABORT, 'injected result failure'); END",
            ),
            ("commit", ""),
        ];

        for (index, (label, trigger)) in cases.into_iter().enumerate() {
            let path = test_database();
            let storage = Storage::open(&path).await.expect("storage");
            let offset = u8::try_from(index).expect("bounded failure case");
            let client_id = Id128::new([0xA0 + offset; 16]).to_base64url();
            insert_client(&storage, &client_id, label, "trusted").await;
            let trust = TrustStore::new(storage.pool_for_test().clone());
            if label == "commit" {
                trust.fail_next_commit_for_test();
            } else {
                sqlx::query(trigger)
                    .execute(storage.pool_for_test())
                    .await
                    .expect("failure trigger");
            }

            let error = trust
                .revoke(
                    "owner",
                    &Id128::new([0xB0 + offset; 16]).to_base64url(),
                    &client_id,
                )
                .await
                .expect_err(label);
            assert!(
                matches!(
                    error,
                    TrustError::Database(_) | TrustError::StorageUnavailable
                ),
                "{label}: {error}"
            );
            let status: String =
                sqlx::query_scalar("SELECT status FROM clients WHERE client_id = ?")
                    .bind(&client_id)
                    .fetch_one(storage.pool_for_test())
                    .await
                    .expect("client status");
            let generation: i64 =
                sqlx::query_scalar("SELECT auth_generation FROM clients WHERE client_id = ?")
                    .bind(&client_id)
                    .fetch_one(storage.pool_for_test())
                    .await
                    .expect("auth generation");
            let revision: i64 =
                sqlx::query_scalar("SELECT revision FROM node_state WHERE singleton = 1")
                    .fetch_one(storage.pool_for_test())
                    .await
                    .expect("revision");
            let audits: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM security_audit WHERE action = 'device.revoke'",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .expect("audit count");
            let commands: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM management_commands WHERE action = 'device.revoke'",
            )
            .fetch_one(storage.pool_for_test())
            .await
            .expect("command count");
            assert_eq!(status, "trusted", "{label}");
            assert_eq!(generation, 1, "{label}");
            assert_eq!(revision, 0, "{label}");
            assert_eq!(audits, 0, "{label}");
            assert_eq!(commands, 0, "{label}");

            drop(trust);
            storage.close().await;
            std::fs::remove_file(path).expect("cleanup");
        }
    }

    #[tokio::test]
    async fn full_revoke_budget_preserves_results_until_the_exact_clock_deadline() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        let first_id = Id128::new([0xD3; 16]).to_base64url();
        let second_id = Id128::new([0xD4; 16]).to_base64url();
        insert_client(&storage, &first_id, "first", "trusted").await;
        insert_client(&storage, &second_id, "second", "trusted").await;
        let clock = Arc::new(TestClock::new());
        let trust = TrustStore {
            pool: storage.pool_for_test().clone(),
            cursors: Arc::new(Mutex::new(CursorState::default())),
            client_gates: Arc::new(Mutex::new(HashMap::new())),
            clock: clock.clone(),
            revision_notify: Arc::new(watch::channel(0).0),
            fail_next_commit: Arc::new(AtomicBool::new(false)),
            list_page_barrier: Arc::new(Mutex::new(None)),
        };
        let request = Id128::new([0xD5; 16]).to_base64url();
        let first = trust
            .revoke("owner", &request, &first_id)
            .await
            .expect("first result");
        sqlx::query(
            "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<9999) \
             INSERT INTO management_commands SELECT 'budget-fixture', CAST(x AS TEXT), \
             'device.revoke', 'fixture', '{}', 1700086401 FROM n",
        )
        .execute(storage.pool_for_test())
        .await
        .expect("9999 future results");
        clock.advance(86399);
        assert_eq!(
            trust
                .revoke("owner", &request, &first_id)
                .await
                .expect("retained replay"),
            first
        );
        assert!(matches!(
            trust.revoke("owner", &request, &second_id).await,
            Err(TrustError::RequestConflict)
        ));
        assert!(matches!(
            trust
                .revoke("owner", &Id128::new([0xD6; 16]).to_base64url(), &second_id)
                .await,
            Err(TrustError::CapacityExhausted)
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM management_commands")
            .fetch_one(storage.pool_for_test())
            .await
            .expect("full count");
        assert_eq!(count, 10000);
        clock.advance(1);
        let next = trust
            .revoke("owner", &request, &second_id)
            .await
            .expect("one expired slot reused at 24h");
        assert_eq!(next.client_id, second_id);
        assert_eq!(
            trust
                .revoke("owner", &request, &second_id)
                .await
                .expect("new replay"),
            next
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM management_commands")
            .fetch_one(storage.pool_for_test())
            .await
            .expect("still full");
        assert_eq!(count, 10000);
        for client in [&first_id, &second_id] {
            assert_eq!(
                trust
                    .client_by_id(client)
                    .await
                    .expect("stored client")
                    .expect("present")
                    .status,
                ClientStatus::Revoked
            );
        }
        drop(trust);
        storage.close().await;
        std::fs::remove_file(path).expect("cleanup owned database");
    }

    #[tokio::test]
    async fn sqlite_full_rolls_back_revocation_and_allows_retry_after_capacity_recovers() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        let pool = storage.pool_for_test();
        let client_id = Id128::new([0xD1; 16]).to_base64url();
        let request_id = Id128::new([0xD2; 16]).to_base64url();
        insert_client(&storage, &client_id, "disk-full fixture", "trusted").await;
        let trust = TrustStore::new(pool.clone());
        // Bound only this database's pager; never exhaust the host filesystem.
        sqlx::raw_sql(
            "CREATE TABLE full_probe(payload BLOB); \
             CREATE TRIGGER fill_on_revoke AFTER INSERT ON security_audit \
             WHEN NEW.action = 'device.revoke' \
             BEGIN INSERT INTO full_probe VALUES (zeroblob(131072)); END",
        )
        .execute(pool)
        .await
        .expect("capacity adapter");
        let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(pool)
            .await
            .expect("page count");
        let limit: i64 = sqlx::query_scalar(&format!("PRAGMA max_page_count = {pages}"))
            .fetch_one(pool)
            .await
            .expect("bounded pager");
        assert_eq!(limit, pages);
        let error = trust
            .revoke("owner", &request_id, &client_id)
            .await
            .expect_err("actual SQLite allocation failure");
        let TrustError::Database(sqlx::Error::Database(database)) = error else {
            panic!("expected SQLite SQLITE_FULL, got {error}");
        };
        assert_eq!(database.code().as_deref(), Some("13"));
        let state: (String, i64, Option<i64>) = sqlx::query_as(
            "SELECT status, auth_generation, revoked_at FROM clients WHERE client_id = ?",
        )
        .bind(&client_id)
        .fetch_one(pool)
        .await
        .expect("state after failure");
        assert_eq!(state, ("trusted".into(), 1, None));
        for query in [
            "SELECT revision FROM node_state WHERE singleton = 1",
            "SELECT COUNT(*) FROM security_audit",
            "SELECT COUNT(*) FROM management_commands",
            "SELECT COUNT(*) FROM full_probe",
        ] {
            let value: i64 = sqlx::query_scalar(query)
                .fetch_one(pool)
                .await
                .expect("atomic rollback");
            assert_eq!(value, 0, "{query}");
        }
        sqlx::query("PRAGMA max_page_count = 1048576")
            .execute(pool)
            .await
            .expect("restore isolated capacity");
        sqlx::raw_sql("DROP TRIGGER fill_on_revoke; DROP TABLE full_probe")
            .execute(pool)
            .await
            .expect("remove adapter");
        let result = trust
            .revoke("owner", &request_id, &client_id)
            .await
            .expect("same request succeeds after capacity recovery");
        assert_eq!(result.status, ClientStatus::Revoked);
        assert_eq!(
            trust
                .revoke("owner", &request_id, &client_id)
                .await
                .expect("replay committed result"),
            result
        );
        drop(trust);
        storage.close().await;
        std::fs::remove_file(path).expect("cleanup owned database");
    }

    #[tokio::test]
    async fn cursors_expire_and_revision_changes_invalidate_them() {
        let path = test_database();
        let storage = Storage::open(&path).await.expect("storage");
        for value in 0_u8..34 {
            insert_client(
                &storage,
                &Id128::new([value; 16]).to_base64url(),
                &format!("client-{value}"),
                "trusted",
            )
            .await;
        }
        let clock = Arc::new(TestClock::new());
        let trust = TrustStore {
            pool: storage.pool_for_test().clone(),
            cursors: Arc::new(Mutex::new(CursorState::default())),
            client_gates: Arc::new(Mutex::new(HashMap::new())),
            clock: clock.clone(),
            revision_notify: Arc::new(watch::channel(0).0),
            fail_next_commit: Arc::new(AtomicBool::new(false)),
            list_page_barrier: Arc::new(Mutex::new(None)),
        };
        let first = trust.list_page("owner", None).await.expect("first page");
        let cursor = first.next_cursor.expect("cursor");

        clock.advance(61);
        assert!(matches!(
            trust.list_page("owner", Some(&cursor)).await,
            Err(TrustError::StaleCursor)
        ));

        let first = trust.list_page("owner", None).await.expect("fresh cursor");
        let cursor = first.next_cursor.expect("fresh continuation");
        sqlx::query("UPDATE node_state SET revision = revision + 1 WHERE singleton = 1")
            .execute(storage.pool_for_test())
            .await
            .expect("revision change");
        assert!(matches!(
            trust.list_page("owner", Some(&cursor)).await,
            Err(TrustError::StaleCursor)
        ));

        storage.close().await;
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn revoke_digest_uses_the_closed_v1_shape() {
        assert_eq!(
            revoke_payload_digest("AAAAAAAAAAAAAAAAAAAAAA"),
            hex::encode(Sha256::digest(format!(
                r#"{{"action":"device.revoke","payload":{{"clientId":"{}"}}}}"#,
                "AAAAAAAAAAAAAAAAAAAAAA"
            )))
        );
    }
}
