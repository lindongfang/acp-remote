//! Hybrid adaptive coordination between local display snapshots, live ACP shared views,
//! capability discovery, background singleflight revalidation, lease protection, and purge.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use tokio::{
    runtime::Handle,
    sync::{Mutex as AsyncMutex, Notify, watch},
    task::JoinHandle,
};

use crate::{
    adapters::acp::{
        ConnectionScope, RawMessage, Version,
        native_views::{AcpReadService, AcpSocketReadService, AcpTranscriptTap},
        shared_view_capture::RetainedAcpRaw,
        shared_view_projection::AcpViewProjector,
    },
    application::{
        native_dispatch::{BoxFuture, CachedHybridFrame, TranscriptRecoveryObserver},
        shared_views::{
            SharedViewOwner,
            local_authority::{AuthenticatedLocalViewClient, LocalViewAuthority},
            reader::OpenedView,
            socket_authority::{SocketViewAuthority, SocketViewClient},
        },
    },
    domain::{
        native::{NativeSessionKey, NativeSourceId},
        shared_view::{
            Freshness, TranscriptCacheViewStatus, TranscriptCapabilityBoundary, TranscriptSource,
            TranscriptSyncState, ViewError,
        },
    },
};

use super::{
    CacheHealth, CapabilityProfile, LoadedTranscriptSnapshot, NativeRecoveryState,
    ProtectedSessionsPort, TranscriptCacheError, TranscriptCacheStore, TranscriptCacheWriter,
};

type ProtectedCheckFn = Arc<dyn Fn(&NativeSessionKey) -> bool + Send + Sync>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevalidationOutcome {
    Pending,
    SyncedReady,
    StayCached,
}

struct CachedStreamRecord {
    key: NativeSessionKey,
    owner: Arc<SharedViewOwner<ConnectionScope, RetainedAcpRaw>>,
    status_tx: watch::Sender<TranscriptCacheViewStatus>,
    rebind_tx: watch::Sender<RevalidationOutcome>,
}

/// Handle retained inside an opened cached view read to query/observe live status updates,
/// receive background `stream_rebound` wakeups, and release owned view resources cleanly.
#[derive(Clone)]
pub struct CachedViewHandle {
    record: Arc<CachedStreamRecord>,
    coordinator: Arc<TranscriptCacheCoordinator>,
}

impl CachedViewHandle {
    #[must_use]
    pub fn key(&self) -> &NativeSessionKey {
        &self.record.key
    }

    #[must_use]
    pub fn status(&self) -> TranscriptCacheViewStatus {
        *self.record.status_tx.borrow()
    }

    #[must_use]
    pub fn revalidation_receiver(&self) -> watch::Receiver<RevalidationOutcome> {
        self.record.rebind_tx.subscribe()
    }

    #[must_use]
    pub fn status_receiver(&self) -> watch::Receiver<TranscriptCacheViewStatus> {
        self.record.status_tx.subscribe()
    }

    /// Marks this cached handle ready for live rebind (`stream_rebound`) and seals the cached
    /// send epoch without destroying the underlying `SocketTransferStream`.
    pub fn trigger_rebind_ready(&self) {
        let mut status = *self.record.status_tx.borrow();
        status.sync_state = TranscriptSyncState::Synced;
        status.capability_boundary = TranscriptCapabilityBoundary::ReplayCapable;
        status.continuable = true;
        let _ = self.record.status_tx.send(status);
        let _ = self.record.rebind_tx.send(RevalidationOutcome::SyncedReady);
        let _ = self
            .record
            .owner
            .handle()
            .seal_target_send(&self.record.key);
    }

    pub fn update_status_and_push(&self, next_status: TranscriptCacheViewStatus) {
        self.coordinator
            .update_cached_record(&self.record, next_status);
    }

    pub fn seal_on_release(&self) {
        let _ = self
            .record
            .owner
            .handle()
            .seal_target_send(&self.record.key);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let owner = Arc::clone(&self.record.owner);
        if let Ok(handle) = Handle::try_current() {
            handle.spawn(async move {
                let _ = owner.shutdown(deadline).await;
            });
        }
    }
}

/// Coordinates isolated `transcript-cache.sqlite3` reads/writes, singleflight background
/// revalidation, lease protection, health warnings, and immediate native delete purge.
pub struct TranscriptCacheCoordinator {
    store: Arc<TranscriptCacheStore>,
    writer: TranscriptCacheWriter,
    runtime: Handle,
    protected_hook: Mutex<Option<ProtectedCheckFn>>,
    active_cached: Mutex<Vec<Weak<CachedStreamRecord>>>,
    hybrid_sessions: Mutex<HashSet<NativeSessionKey>>,
    remote_missing: Mutex<HashSet<NativeSessionKey>>,
    source_capabilities: Mutex<HashMap<NativeSourceId, (Version, CapabilityProfile)>>,
    pending_replays: Arc<Mutex<HashMap<NativeSessionKey, Arc<Notify>>>>,
    inflight_revalidations: AsyncMutex<HashMap<NativeSessionKey, JoinHandle<()>>>,
}

impl TranscriptCacheCoordinator {
    #[must_use]
    pub fn new(store: &Arc<TranscriptCacheStore>, runtime: Handle) -> Arc<Self> {
        let writer = store.writer();
        let coordinator = Arc::new(Self {
            store: Arc::clone(store),
            writer,
            runtime,
            protected_hook: Mutex::new(None),
            active_cached: Mutex::new(Vec::new()),
            hybrid_sessions: Mutex::new(HashSet::new()),
            remote_missing: Mutex::new(HashSet::new()),
            source_capabilities: Mutex::new(HashMap::new()),
            pending_replays: Arc::new(Mutex::new(HashMap::new())),
            inflight_revalidations: AsyncMutex::new(HashMap::new()),
        });
        let port: Arc<dyn ProtectedSessionsPort> = coordinator.clone();
        store.set_protected_sessions(port);
        coordinator
    }

    #[must_use]
    pub const fn store(&self) -> &Arc<TranscriptCacheStore> {
        &self.store
    }

    #[must_use]
    pub fn has_write_warning(&self) -> bool {
        !matches!(self.store.health(), CacheHealth::Healthy)
    }

    pub fn install_protected_check(&self, check: ProtectedCheckFn) {
        let mut guard = self
            .protected_hook
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(check);
    }

    #[must_use]
    pub fn is_hybrid_resumed(&self, key: &NativeSessionKey) -> bool {
        self.hybrid_sessions
            .lock()
            .is_ok_and(|set| set.contains(key))
    }

    #[must_use]
    pub fn is_remote_missing(&self, key: &NativeSessionKey) -> bool {
        self.remote_missing
            .lock()
            .is_ok_and(|set| set.contains(key))
    }

    pub fn known_source_capabilities(
        &self,
        source_id: NativeSourceId,
    ) -> Option<(Version, CapabilityProfile)> {
        self.source_capabilities
            .lock()
            .ok()
            .and_then(|map| map.get(&source_id).copied())
    }

    #[must_use]
    pub const fn capability_boundary_for(
        profile: CapabilityProfile,
    ) -> TranscriptCapabilityBoundary {
        match profile {
            CapabilityProfile::Unknown => TranscriptCapabilityBoundary::Unknown,
            CapabilityProfile::ReplayCapable => TranscriptCapabilityBoundary::ReplayCapable,
            CapabilityProfile::ResumeOnly => TranscriptCapabilityBoundary::ResumeOnly,
            CapabilityProfile::PureMemory => TranscriptCapabilityBoundary::PureMemory,
        }
    }

    #[must_use]
    pub fn live_status(
        &self,
        key: &NativeSessionKey,
        hybrid_override: bool,
        synced_override: bool,
    ) -> TranscriptCacheViewStatus {
        let is_hybrid = hybrid_override || self.is_hybrid_resumed(key);
        let boundary = self
            .known_source_capabilities(key.source())
            .map_or(TranscriptCapabilityBoundary::Unknown, |(_, profile)| {
                Self::capability_boundary_for(profile)
            });
        TranscriptCacheViewStatus {
            transcript_source: if is_hybrid {
                TranscriptSource::HybridResumed
            } else {
                TranscriptSource::Live
            },
            sync_state: if synced_override {
                TranscriptSyncState::Synced
            } else {
                TranscriptSyncState::Idle
            },
            capability_boundary: if synced_override {
                TranscriptCapabilityBoundary::ReplayCapable
            } else {
                boundary
            },
            continuable: !matches!(boundary, TranscriptCapabilityBoundary::PureMemory),
            cache_warning: self.has_write_warning(),
        }
    }

    #[must_use]
    pub fn initial_cold_status(
        &self,
        session: &super::PersistedTranscriptSession,
    ) -> (TranscriptCacheViewStatus, bool) {
        let boundary = Self::capability_boundary_for(session.capability_profile);
        let warning = self.has_write_warning();
        if session.native_recovery_state == NativeRecoveryState::NativeUnrecoverable
            || self.is_remote_missing(&session.key)
        {
            return (
                TranscriptCacheViewStatus {
                    transcript_source: TranscriptSource::TranscriptCache,
                    sync_state: TranscriptSyncState::NativeUnrecoverable,
                    capability_boundary: boundary,
                    continuable: false,
                    cache_warning: warning,
                },
                false,
            );
        }
        match session.capability_profile {
            CapabilityProfile::PureMemory => (
                TranscriptCacheViewStatus {
                    transcript_source: TranscriptSource::TranscriptCache,
                    sync_state: TranscriptSyncState::Idle,
                    capability_boundary: TranscriptCapabilityBoundary::PureMemory,
                    continuable: false,
                    cache_warning: warning,
                },
                false,
            ),
            CapabilityProfile::ResumeOnly => (
                TranscriptCacheViewStatus {
                    transcript_source: TranscriptSource::TranscriptCache,
                    sync_state: TranscriptSyncState::Idle,
                    capability_boundary: TranscriptCapabilityBoundary::ResumeOnly,
                    continuable: true,
                    cache_warning: warning,
                },
                false,
            ),
            CapabilityProfile::ReplayCapable | CapabilityProfile::Unknown => (
                TranscriptCacheViewStatus {
                    transcript_source: TranscriptSource::TranscriptCache,
                    sync_state: TranscriptSyncState::Revalidating,
                    capability_boundary: boundary,
                    continuable: true,
                    cache_warning: warning,
                },
                true,
            ),
        }
    }

    /// Opens an offline `SocketViewClient` view stream directly from a persisted snapshot without
    /// consuming any Harness process or 50-slot execution capacity.
    ///
    /// # Errors
    /// Returns `ViewError` if offline projection or client authorization fails.
    pub async fn open_cached_socket_view(
        self: &Arc<Self>,
        key: &NativeSessionKey,
        client: &SocketViewClient,
        authority: &Arc<SocketViewAuthority>,
        snapshot: LoadedTranscriptSnapshot,
        status: TranscriptCacheViewStatus,
    ) -> Result<(OpenedView, Arc<AcpSocketReadService>, CachedViewHandle), ViewError> {
        let owner = self.build_cached_owner(key, snapshot, status)?;
        let reader = Arc::new(AcpSocketReadService::new(
            owner.handle(),
            Arc::clone(authority),
            owner.supervisor.clone(),
        ));
        let opened = reader.open_current(client, key).await?;
        let handle = self.register_cached_owner(key, owner, status);
        Ok((opened, reader, handle))
    }

    /// Opens an offline `AuthenticatedLocalViewClient` view stream directly from a persisted
    /// snapshot without consuming any Harness process or 50-slot execution capacity.
    ///
    /// # Errors
    /// Returns `ViewError` if offline projection or client authorization fails.
    pub async fn open_cached_local_view(
        self: &Arc<Self>,
        key: &NativeSessionKey,
        client: &AuthenticatedLocalViewClient,
        authority: &Arc<LocalViewAuthority>,
        snapshot: LoadedTranscriptSnapshot,
        status: TranscriptCacheViewStatus,
    ) -> Result<(OpenedView, Arc<AcpReadService>, CachedViewHandle), ViewError> {
        let owner = self.build_cached_owner(key, snapshot, status)?;
        let reader = Arc::new(AcpReadService::new(
            owner.handle(),
            Arc::clone(authority),
            owner.supervisor.clone(),
        ));
        let opened = reader.open_current(client, key).await?;
        let handle = self.register_cached_owner(key, owner, status);
        Ok((opened, reader, handle))
    }

    fn build_cached_owner(
        &self,
        key: &NativeSessionKey,
        snapshot: LoadedTranscriptSnapshot,
        status: TranscriptCacheViewStatus,
    ) -> Result<Arc<SharedViewOwner<ConnectionScope, RetainedAcpRaw>>, ViewError> {
        let projector = AcpViewProjector::for_version(snapshot.session.protocol_version);
        let session_arc: Arc<str> = Arc::from(key.native_id());
        let mut frames = Vec::with_capacity(snapshot.records.len());
        for record in snapshot.records {
            let retained = RetainedAcpRaw::from_cached_raw(record.raw, Arc::clone(&session_arc));
            frames.push((record.connection_sequence, record.ordinal, retained));
        }
        let freshness = if status.sync_state == TranscriptSyncState::Revalidating {
            Freshness::Revalidating
        } else {
            Freshness::CurrentObservation
        };
        let owner = SharedViewOwner::open_cached_target(
            ConnectionScope::synthetic_for_cache(),
            key.source(),
            key,
            &projector,
            frames,
            freshness,
            self.runtime.clone(),
        )?;
        Ok(Arc::new(owner))
    }

    fn register_cached_owner(
        self: &Arc<Self>,
        key: &NativeSessionKey,
        owner: Arc<SharedViewOwner<ConnectionScope, RetainedAcpRaw>>,
        initial_status: TranscriptCacheViewStatus,
    ) -> CachedViewHandle {
        let (status_tx, _) = watch::channel(initial_status);
        let (rebind_tx, _) = watch::channel(RevalidationOutcome::Pending);
        let record = Arc::new(CachedStreamRecord {
            key: key.clone(),
            owner,
            status_tx,
            rebind_tx,
        });
        if let Ok(mut list) = self.active_cached.lock() {
            list.retain(|item| item.strong_count() > 0);
            list.push(Arc::downgrade(&record));
        }
        CachedViewHandle {
            record,
            coordinator: Arc::clone(self),
        }
    }

    pub fn push_status_for_key(
        &self,
        key: &NativeSessionKey,
        mut next_status: TranscriptCacheViewStatus,
    ) {
        if self.has_write_warning() {
            next_status.cache_warning = true;
        }
        let records = self.matching_cached_records(key);
        for record in records {
            self.update_cached_record(&record, next_status);
        }
    }

    pub fn notify_synced_rebound(&self, key: &NativeSessionKey) {
        for record in self.matching_cached_records(key) {
            let mut status = *record.status_tx.borrow();
            status.sync_state = TranscriptSyncState::Synced;
            status.capability_boundary = TranscriptCapabilityBoundary::ReplayCapable;
            status.continuable = true;
            status.cache_warning = status.cache_warning || self.has_write_warning();
            let _ = record.status_tx.send(status);
            let _ = record.rebind_tx.send(RevalidationOutcome::SyncedReady);
            let _ = record.owner.handle().seal_target_send(&record.key);
        }
    }

    fn update_cached_record(
        &self,
        record: &CachedStreamRecord,
        mut next_status: TranscriptCacheViewStatus,
    ) {
        if self.has_write_warning() {
            next_status.cache_warning = true;
        }
        let _ = record.status_tx.send(next_status);
        if next_status.sync_state != TranscriptSyncState::Revalidating
            && next_status.sync_state != TranscriptSyncState::Synced
        {
            let _ = record.rebind_tx.send(RevalidationOutcome::StayCached);
        }
        if let Ok(bytes) = encode_cache_state_json(next_status) {
            let _ = record.owner.handle().push_custom_status(&record.key, bytes);
        }
    }

    fn matching_cached_records(&self, key: &NativeSessionKey) -> Vec<Arc<CachedStreamRecord>> {
        let Ok(mut list) = self.active_cached.lock() else {
            return Vec::new();
        };
        list.retain(|weak| weak.strong_count() > 0);
        list.iter()
            .filter_map(Weak::upgrade)
            .filter(|record| record.key == *key)
            .collect()
    }

    /// Returns whether a singleflight background revalidation task is currently running for `key`.
    pub async fn is_revalidating(&self, key: &NativeSessionKey) -> bool {
        let mut inflight = self.inflight_revalidations.lock().await;
        inflight.retain(|_, handle| !handle.is_finished());
        inflight.contains_key(key)
    }

    /// Runs at most one concurrent background revalidation task per `NativeSessionKey`.
    pub async fn spawn_singleflight_revalidation<F>(&self, key: NativeSessionKey, work: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut inflight = self.inflight_revalidations.lock().await;
        inflight.retain(|_, handle| !handle.is_finished());
        if inflight.contains_key(&key) {
            return;
        }
        let handle = self.runtime.spawn(work);
        inflight.insert(key, handle);
    }

    pub async fn clear_inflight(&self, key: &NativeSessionKey) {
        let mut inflight = self.inflight_revalidations.lock().await;
        inflight.remove(key);
    }

    /// Waits briefly for any in-flight replay persistence for `key` to finish so subsequent reads
    /// observe committed snapshot state.
    pub async fn wait_pending_replay_persist(&self, key: &NativeSessionKey) {
        let notify = {
            let guard = self.pending_replays.lock().ok();
            guard.and_then(|map| map.get(key).cloned())
        };
        if let Some(notify) = notify {
            let notified = notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let still_pending = self
                .pending_replays
                .lock()
                .is_ok_and(|map| map.contains_key(key));
            if still_pending {
                let _ = tokio::time::timeout(Duration::from_secs(5), notified).await;
            }
        }
    }

    /// Synchronously seals in-memory cached views, stops revalidations for `key`, and erases all
    /// rows and WAL pages for a confirmed-deleted native session.
    ///
    /// # Errors
    /// Returns `TranscriptCacheError` if deleting rows or checkpointing SQLite WAL fails.
    pub async fn purge_deleted_native_session(
        &self,
        key: &NativeSessionKey,
    ) -> Result<(), TranscriptCacheError> {
        let aborted = {
            let mut inflight = self.inflight_revalidations.lock().await;
            inflight.remove(key)
        };
        if let Some(handle) = aborted {
            handle.abort();
            let _ = handle.await;
        }
        if let Ok(mut set) = self.hybrid_sessions.lock() {
            set.remove(key);
        }
        if let Ok(mut set) = self.remote_missing.lock() {
            set.remove(key);
        }
        for record in self.matching_cached_records(key) {
            let _ = record.owner.handle().seal_target_send(key);
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            let _ = record.owner.shutdown(deadline).await;
        }
        self.store.purge_deleted_native_session(key).await
    }

    /// Cancels background revalidations, shuts down all active cached view owners, and closes the
    /// underlying SQLite pool/writer cleanly.
    ///
    /// # Errors
    /// Returns `TranscriptCacheError` if closing the store fails.
    pub async fn shutdown(&self) -> Result<(), TranscriptCacheError> {
        let handles: Vec<JoinHandle<()>> = {
            let mut inflight = self.inflight_revalidations.lock().await;
            inflight.drain().map(|(_, handle)| handle).collect()
        };
        for handle in handles {
            handle.abort();
            let _ = handle.await;
        }
        let records: Vec<Arc<CachedStreamRecord>> = {
            let mut list = self
                .active_cached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let active = list.iter().filter_map(Weak::upgrade).collect();
            list.clear();
            active
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        for record in records {
            let _ = record.owner.shutdown(deadline).await;
        }
        let _ = self.writer.flush().await;
        self.store.shutdown().await
    }
}

impl ProtectedSessionsPort for TranscriptCacheCoordinator {
    fn is_protected(&self, key: &NativeSessionKey) -> bool {
        let live_protected = self
            .protected_hook
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
            .is_some_and(|check| check(key));
        if live_protected {
            return true;
        }
        self.matching_cached_records(key)
            .iter()
            .any(|record| record.owner.handle().has_active_leases_for(key))
    }
}

impl AcpTranscriptTap for TranscriptCacheCoordinator {
    fn on_source_capabilities(
        &self,
        source_id: NativeSourceId,
        version: Version,
        profile: CapabilityProfile,
    ) {
        if let Ok(mut map) = self.source_capabilities.lock() {
            map.insert(source_id, (version, profile));
        }
    }

    fn on_live_update(
        &self,
        key: &NativeSessionKey,
        version: Version,
        profile: CapabilityProfile,
        sequence: u64,
        raw: &RawMessage,
    ) {
        if let Ok(mut map) = self.source_capabilities.lock() {
            map.insert(key.source(), (version, profile));
        }
        let enqueued = self.writer.try_enqueue_frame_with_profile(
            key,
            version,
            profile,
            sequence,
            raw.clone(),
        );
        if (!enqueued || self.has_write_warning()) && !self.matching_cached_records(key).is_empty()
        {
            for record in self.matching_cached_records(key) {
                let mut status = *record.status_tx.borrow();
                status.cache_warning = true;
                self.update_cached_record(&record, status);
            }
        }
    }

    fn on_replay_completed(
        &self,
        key: &NativeSessionKey,
        version: Version,
        profile: CapabilityProfile,
        frames: Vec<(u64, RawMessage)>,
    ) {
        if let Ok(mut map) = self.source_capabilities.lock() {
            map.insert(key.source(), (version, profile));
        }
        if let Ok(mut set) = self.hybrid_sessions.lock() {
            set.remove(key);
        }
        if let Ok(mut set) = self.remote_missing.lock() {
            set.remove(key);
        }
        let notify = Arc::new(Notify::new());
        if let Ok(mut pending) = self.pending_replays.lock() {
            pending.insert(key.clone(), Arc::clone(&notify));
        }
        let store = Arc::clone(&self.store);
        let key_owned = key.clone();
        let pending_map = Arc::clone(&self.pending_replays);
        let cached_records = self.matching_cached_records(key);
        let converted: Vec<(RawMessage, u64)> =
            frames.into_iter().map(|(seq, raw)| (raw, seq)).collect();
        self.runtime.spawn(async move {
            let res = store
                .replace_with_replay(&key_owned, version, profile, converted)
                .await;
            if let Err(error) = res {
                tracing::warn!(%error, "transcript cache replay persistence failed; continuing in memory");
                for record in &cached_records {
                    let mut st = *record.status_tx.borrow();
                    st.cache_warning = true;
                    let _ = record.status_tx.send(st);
                    if let Ok(bytes) = encode_cache_state_json(st) {
                        let _ = record.owner.handle().push_custom_status(&key_owned, bytes);
                    }
                }
            } else {
                let _ = store.enforce_retention_and_budget(None).await;
            }
            if let Ok(mut pending) = pending_map.lock()
                && pending
                    .get(&key_owned)
                    .is_some_and(|entry| Arc::ptr_eq(entry, &notify))
            {
                pending.remove(&key_owned);
            }
            notify.notify_waiters();
        });
    }

    fn on_replay_remote_missing(&self, key: &NativeSessionKey) {
        if let Ok(mut set) = self.remote_missing.lock() {
            set.insert(key.clone());
        }
        let store = Arc::clone(&self.store);
        let key_owned = key.clone();
        let status = TranscriptCacheViewStatus {
            transcript_source: TranscriptSource::TranscriptCache,
            sync_state: TranscriptSyncState::NativeUnrecoverable,
            capability_boundary: self
                .known_source_capabilities(key.source())
                .map_or(TranscriptCapabilityBoundary::ReplayCapable, |(_, p)| {
                    Self::capability_boundary_for(p)
                }),
            continuable: false,
            cache_warning: self.has_write_warning(),
        };
        self.push_status_for_key(key, status);
        self.runtime.spawn(async move {
            let _ = store
                .update_session_status(
                    &key_owned,
                    None,
                    Some(NativeRecoveryState::NativeUnrecoverable),
                )
                .await;
        });
    }
}

impl TranscriptRecoveryObserver for TranscriptCacheCoordinator {
    fn on_resume_rejected(&self, key: &NativeSessionKey) {
        if let Ok(mut set) = self.remote_missing.lock() {
            set.insert(key.clone());
        }
        let store = Arc::clone(&self.store);
        let key_owned = key.clone();
        let status = TranscriptCacheViewStatus {
            transcript_source: TranscriptSource::TranscriptCache,
            sync_state: TranscriptSyncState::NativeUnrecoverable,
            capability_boundary: self
                .known_source_capabilities(key.source())
                .map_or(TranscriptCapabilityBoundary::ResumeOnly, |(_, p)| {
                    Self::capability_boundary_for(p)
                }),
            continuable: false,
            cache_warning: self.has_write_warning(),
        };
        self.push_status_for_key(key, status);
        self.runtime.spawn(async move {
            let _ = store
                .update_session_status(
                    &key_owned,
                    None,
                    Some(NativeRecoveryState::NativeUnrecoverable),
                )
                .await;
        });
    }

    fn cached_frames_for_hybrid_resume(
        &self,
        key: &NativeSessionKey,
    ) -> BoxFuture<'_, Option<Vec<CachedHybridFrame>>> {
        let store = Arc::clone(&self.store);
        let key_owned = key.clone();
        Box::pin(async move {
            let Ok(Some(snapshot)) = store.load_snapshot(&key_owned).await else {
                return None;
            };
            if store.is_snapshot_expired(&snapshot.session) {
                return None;
            }
            Some(
                snapshot
                    .records
                    .into_iter()
                    .map(|record| CachedHybridFrame {
                        sequence: record.connection_sequence,
                        receipt_order: record.ordinal,
                        raw_json: record.raw.bytes().to_vec(),
                    })
                    .collect(),
            )
        })
    }

    fn on_session_resumed_without_replay(&self, key: &NativeSessionKey) {
        if let Ok(mut set) = self.hybrid_sessions.lock() {
            set.insert(key.clone());
        }
        let boundary = self
            .known_source_capabilities(key.source())
            .map_or(TranscriptCapabilityBoundary::ResumeOnly, |(_, p)| {
                Self::capability_boundary_for(p)
            });
        for record in self.matching_cached_records(key) {
            let mut status = *record.status_tx.borrow();
            status.transcript_source = TranscriptSource::HybridResumed;
            status.sync_state = TranscriptSyncState::Idle;
            status.capability_boundary = boundary;
            status.continuable = true;
            status.cache_warning = status.cache_warning || self.has_write_warning();
            let _ = record.status_tx.send(status);
            let _ = record.rebind_tx.send(RevalidationOutcome::SyncedReady);
            let _ = record.owner.handle().seal_target_send(&record.key);
        }
    }
}

/// Encodes one live `transcript_cache_state` `RecordKind::Status` JSON payload.
///
/// # Errors
/// Returns `ViewError::AllocationFailure` if serialization fails.
pub fn encode_cache_state_json(status: TranscriptCacheViewStatus) -> Result<Vec<u8>, ViewError> {
    serde_json::to_vec(&serde_json::json!({
        "type": "transcript_cache_state",
        "transcriptSource": status.transcript_source.as_wire_str(),
        "syncState": status.sync_state.as_wire_str(),
        "capabilityBoundary": status.capability_boundary.as_wire_str(),
        "continuable": status.continuable,
        "cacheWarning": status.cache_warning,
    }))
    .map_err(|_| ViewError::AllocationFailure)
}
