//! One actual connection's shared current/candidate views, driven only by its FIFO worker.
#[cfg(test)]
mod capacity_development_tests;
use super::{
    RetainedRaw,
    indexed_journal::{IndexedJournal, JournalSnapshot, ProjectionEnvelope, ViewProjector},
    ingress::{
        Frame, IngressProbe, ReadCompleteMarker, ReadFailureEvidence, ReadFailureMarker,
        RouteMarker, SourceConsumer, SourceEntry,
    },
    segments::{SegmentLog, SegmentMetadata, SegmentOrigin, SegmentPhase, SegmentSnapshot},
    send_epoch::{OwnedDisplaySendGroup, OwnedDisplaySendView},
    tasks::{ViewTaskContext, ViewTaskKind, ViewTaskSupervisor},
};
use crate::domain::{
    EntityId,
    native::{NativeSessionKey, NativeSourceId},
    shared_view::{
        Freshness, JoinState, OrderBasis, ProjectionState, RawRecordId, ReadEpoch, ReadMode,
        ViewDescriptor, ViewEpoch, ViewError, ViewRevision, ViewSourceStamp,
    },
    shared_view_log::{AppendLog, LogSnapshot},
};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex, RwLock},
};
use tokio::{runtime::Handle, sync::Notify, time::Instant};

pub(crate) struct SourceRecord<R> {
    pub id: RawRecordId,
    pub sequence: u64,
    pub receipt_order: u64,
    pub raw: R,
    pub envelope: ProjectionEnvelope,
    pub assignment: Mutex<Option<ViewEpoch>>,
}
pub(crate) struct TargetSnapshot<R> {
    pub raw: LogSnapshot<SourceRecord<R>>,
    pub projection: JournalSnapshot<R>,
    pub descriptor: ViewDescriptor,
    pub segments: SegmentSnapshot,
}
impl<R> Clone for TargetSnapshot<R> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            projection: self.projection.clone(),
            descriptor: self.descriptor,
            segments: self.segments.clone(),
        }
    }
}
pub(crate) struct TargetGeneration<R> {
    pub send: OwnedDisplaySendView,
    pub snapshot: RwLock<Arc<TargetSnapshot<R>>>,
    pub custom_statuses: Mutex<AppendLog<super::segments::CustomStatusEvent>>,
}
pub(crate) struct TargetPublication<R> {
    pub key: NativeSessionKey,
    pub current: RwLock<Arc<TargetGeneration<R>>>,
    pub candidate: RwLock<std::sync::Weak<TargetGeneration<R>>>,
    pub candidate_failure: RwLock<Option<(ReadEpoch, ViewError)>>,
}
pub(crate) struct SourceCore<S, R> {
    pub scope: S,
    pub source: NativeSourceId,
    pub stamp: ViewSourceStamp,
    pub send: OwnedDisplaySendGroup,
    pub probe: IngressProbe<R>,
    pub catalog: RwLock<Option<LogSnapshot<TargetPublication<R>>>>,
    pub changed: Arc<Notify>,
    pub active_leases: Mutex<
        Vec<(
            NativeSessionKey,
            std::sync::Weak<super::reader::SelectionLeases>,
        )>,
    >,
    idle_producer: Mutex<Option<super::ingress::SourceProducer<S, R>>>,
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub controller: super::controller::ViewTestController<S>,
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub streams: Mutex<
        Vec<(
            EntityId,
            std::sync::Weak<dyn super::probe::StreamInspectionPort>,
        )>,
    >,
}
#[derive(Clone)]
pub(crate) struct SourceViewSeal {
    send: OwnedDisplaySendGroup,
    flag: Arc<std::sync::atomic::AtomicBool>,
    ingress: Arc<Notify>,
    views: Arc<Notify>,
}
impl SourceViewSeal {
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        let result = self.send.seal();
        self.flag.store(true, std::sync::atomic::Ordering::Release);
        self.ingress.notify_waiters();
        self.views.notify_waiters();
        result
    }
}
pub struct SharedViews<S, R> {
    pub(crate) core: Arc<SourceCore<S, R>>,
}
impl<S, R> Clone for SharedViews<S, R> {
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}
pub struct SharedViewOwner<S, R> {
    views: SharedViews<S, R>,
    pub(crate) supervisor: ViewTaskSupervisor,
}
#[allow(
    clippy::struct_excessive_bools,
    reason = "Source seal and independent worker/reader/destructor joins are separate actual evidence axes required by the frozen report"
)]
pub struct ViewShutdownReport {
    sealed: bool,
    workers: bool,
    readers: bool,
    cleanup: bool,
    pending: usize,
    failure: Option<ViewError>,
}
impl ViewShutdownReport {
    #[must_use]
    pub const fn capture_sealed(&self) -> bool {
        self.sealed
    }
    #[must_use]
    pub const fn worker_joined(&self) -> bool {
        self.workers
    }
    #[must_use]
    pub const fn readers_joined(&self) -> bool {
        self.readers
    }
    #[must_use]
    pub const fn cleanup_joined(&self) -> bool {
        self.cleanup
    }
    #[must_use]
    pub const fn pending_native_reads(&self) -> usize {
        self.pending
    }
    #[must_use]
    pub const fn failure(&self) -> Option<ViewError> {
        self.failure
    }
}
struct GenerationWorker<R> {
    journal: IndexedJournal<R>,
    published: Arc<TargetGeneration<R>>,
    rpc: Option<super::read_control::RpcScalar>,
    segments: SegmentLog,
    principal: SegmentMetadata,
    limited: bool,
    route: u64,
    cut: u64,
    failure_evidence: Option<ReadFailureEvidence<R>>,
}
struct TargetWorker<R> {
    published: Arc<TargetPublication<R>>,
    current: GenerationWorker<R>,
    candidate: Option<GenerationWorker<R>>,
    live: Option<(usize, SegmentMetadata)>,
    separate_live: bool,
}
struct Worker<S, R> {
    core: Arc<SourceCore<S, R>>,
    consumer: SourceConsumer<R>,
    projector: Arc<dyn ViewProjector<R>>,
    records: AppendLog<SourceRecord<R>>,
    catalog: AppendLog<TargetPublication<R>>,
    targets: HashMap<NativeSessionKey, TargetWorker<R>>,
    retired: Vec<GenerationWorker<R>>,
    revision: u64,
    yield_every: NonZeroUsize,
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> SharedViewOwner<S, R> {
    #[cfg(any(test, all(windows, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn start_source(
        scope: S,
        source: NativeSourceId,
        consumer: SourceConsumer<R>,
        probe: IngressProbe<R>,
        projector: Arc<dyn ViewProjector<R>>,
        yield_every: NonZeroUsize,
        runtime: Handle,
    ) -> Result<Self, ViewError> {
        let (owner, worker) = Self::prepare_source(
            scope,
            source,
            consumer,
            probe,
            projector,
            yield_every,
            runtime,
        )?;
        worker.start(&owner)?;
        Ok(owner)
    }
    pub(crate) fn prepare_source(
        scope: S,
        source: NativeSourceId,
        consumer: SourceConsumer<R>,
        probe: IngressProbe<R>,
        projector: Arc<dyn ViewProjector<R>>,
        yield_every: NonZeroUsize,
        runtime: Handle,
    ) -> Result<(Self, PendingViewWorker<S, R>), ViewError> {
        let catalog = AppendLog::new()?;
        let records = AppendLog::new()?;
        let binding =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let core = Arc::new(SourceCore {
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            controller: super::controller::ViewTestController::new(),
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            streams: Mutex::new(Vec::new()),
            scope,
            source,
            stamp: ViewSourceStamp { binding, source },
            send: OwnedDisplaySendGroup::new(),
            probe,
            catalog: RwLock::new(Some(catalog.snapshot()?)),
            changed: Arc::new(Notify::new()),
            active_leases: Mutex::new(Vec::new()),
            idle_producer: Mutex::new(None),
        });
        let supervisor = ViewTaskSupervisor::new(runtime);
        let stop = supervisor.subscribe_stop();
        let worker = Worker {
            core: Arc::clone(&core),
            consumer,
            projector,
            records,
            catalog,
            targets: HashMap::new(),
            retired: Vec::new(),
            revision: 0,
            yield_every,
        };
        Ok((
            Self {
                views: SharedViews { core },
                supervisor,
            },
            PendingViewWorker { worker, stop },
        ))
    }
    #[must_use]
    pub fn handle(&self) -> SharedViews<S, R> {
        self.views.clone()
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    #[must_use]
    pub fn test_controller(&self) -> super::controller::ViewTestController<S> {
        self.views.core.controller.clone()
    }
    pub async fn shutdown(&self, deadline: Instant) -> ViewShutdownReport {
        if let Ok(mut producer) = self.views.core.idle_producer.lock() {
            drop(producer.take());
        }
        let seal = self.views.core.seal();
        self.supervisor.request_stop();
        let report = self.supervisor.join_until(deadline).await;
        ViewShutdownReport {
            sealed: self.views.core.probe.is_sealed(),
            workers: report.workers == 0,
            readers: report.readers == 0,
            cleanup: report.cleanup == 0,
            pending: report.native_reads,
            failure: seal.err().or(report.failure),
        }
    }
    /// Constructs an offline managed `SharedViewOwner` populated directly from persisted
    /// transcript-cache frames without launching a live Harness subprocess or worker task.
    ///
    /// # Errors
    /// Returns `ViewError` when random generation, projection, or send-group creation fails.
    #[allow(
        clippy::too_many_lines,
        reason = "Offline snapshot construction initializes catalog, projection, and send epoch atomically"
    )]
    pub(crate) fn open_cached_target(
        scope: S,
        source: NativeSourceId,
        key: &NativeSessionKey,
        projector: &dyn ViewProjector<R>,
        frames: Vec<(u64, u64, R)>,
        freshness: Freshness,
        runtime: Handle,
    ) -> Result<Self, ViewError> {
        let (producer, _consumer, probe) = super::ingress::source_ingress(scope.clone());
        let mut catalog = AppendLog::new()?;
        let mut records = AppendLog::new()?;
        let binding =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let stamp = ViewSourceStamp { binding, source };
        let epoch = ViewEpoch::fresh()?;
        let mut journal = IndexedJournal::new()?;
        let mut revision: u64 = 0;
        let mut begin = None;
        let mut through = None;
        let mut failure = None;
        let mut descriptor = ViewDescriptor {
            epoch,
            read: None,
            source: stamp,
            revision: ViewRevision(0),
            mode: ReadMode::ReplayComplete,
            freshness,
            projection: ProjectionState::Available,
            join: JoinState::RebuiltNoStableIds,
            order: OrderBasis::ConnectionObservation,
            cache_recoverable: true,
            failure: None,
        };
        for (idx, (sequence, receipt_order, raw)) in frames.into_iter().enumerate() {
            revision = revision
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            let id = RawRecordId(u64::try_from(idx + 1).map_err(|_| ViewError::SequenceExhausted)?);
            let envelope = projector.inspect(&raw);
            failure = failure.or(envelope.failure);
            if envelope.projection != ProjectionState::Available {
                descriptor.projection = envelope.projection;
            }
            begin.get_or_insert(sequence);
            through = Some(sequence);
            records.append(SourceRecord {
                id,
                sequence,
                receipt_order,
                raw: raw.clone(),
                envelope: envelope.clone(),
                assignment: Mutex::new(Some(epoch)),
            })?;
            journal.append_segment_at(
                epoch,
                0,
                id,
                revision,
                raw,
                sequence,
                receipt_order,
                envelope,
            )?;
        }
        let projected = journal.snapshot()?;
        descriptor.revision = ViewRevision(revision);
        descriptor.failure = failure;
        if projected.projection != ProjectionState::Available {
            descriptor.projection = projected.projection;
        }
        let principal = SegmentMetadata {
            epoch,
            read: None,
            origin: SegmentOrigin::RetainedCurrent,
            descriptor,
            begin,
            through,
            completion: through,
            open: false,
            context: super::read_observation::ReadContextKind::OutsideReadWindow,
            revision,
            limitation: Some("transcript_cache_snapshot"),
        };
        let mut segments = SegmentLog::new()?;
        segments.append(principal)?;
        let send = OwnedDisplaySendGroup::new();
        let target_send = send.original_view_for_target(epoch, key)?;
        let snapshot = Arc::new(TargetSnapshot {
            raw: records.snapshot()?,
            projection: projected,
            descriptor,
            segments: segments.snapshot()?,
        });
        let published = Arc::new(TargetGeneration {
            send: target_send,
            snapshot: RwLock::new(snapshot),
            custom_statuses: Mutex::new(AppendLog::new()?),
        });
        catalog.append(TargetPublication {
            key: key.clone(),
            current: RwLock::new(published),
            candidate: RwLock::new(std::sync::Weak::new()),
            candidate_failure: RwLock::new(None),
        })?;
        let core = Arc::new(SourceCore {
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            controller: super::controller::ViewTestController::new(),
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            streams: Mutex::new(Vec::new()),
            scope,
            source,
            stamp,
            send,
            probe,
            catalog: RwLock::new(Some(catalog.snapshot()?)),
            changed: Arc::new(Notify::new()),
            active_leases: Mutex::new(Vec::new()),
            idle_producer: Mutex::new(Some(producer)),
        });
        let supervisor = ViewTaskSupervisor::new(runtime);
        Ok(Self {
            views: SharedViews { core },
            supervisor,
        })
    }
}
pub(crate) struct PendingViewWorker<S, R> {
    worker: Worker<S, R>,
    stop: tokio::sync::watch::Receiver<bool>,
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> PendingViewWorker<S, R> {
    pub(crate) fn start(self, owner: &SharedViewOwner<S, R>) -> Result<(), ViewError> {
        owner
            .supervisor
            .spawn_owned(ViewTaskKind::Worker, move |context| {
                self.worker.run(context, self.stop)
            })
    }
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> SharedViewOwner<S, R> {
    pub(crate) async fn join_stopped(&self) -> super::tasks::SupervisorReport {
        let _ = self.views.core.seal();
        self.supervisor.request_stop();
        self.supervisor.join_stopped().await
    }
}
impl<S, R> Drop for SharedViewOwner<S, R> {
    fn drop(&mut self) {
        if let Ok(mut producer) = self.views.core.idle_producer.lock() {
            drop(producer.take());
        }
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.views.core.controller.close();
        let _ = self.views.core.send.seal();
        self.views.core.probe.seal();
        self.views.core.changed.notify_waiters();
        self.supervisor.request_stop();
    }
}
impl<S, R: RetainedRaw> SharedViews<S, R> {
    pub(crate) fn register_lease(
        &self,
        target: &NativeSessionKey,
        leases: std::sync::Weak<super::reader::SelectionLeases>,
    ) {
        if let Ok(mut guard) = self.core.active_leases.lock() {
            guard.retain(|(_, entry)| entry.strong_count() > 0);
            guard.push((target.clone(), leases));
        }
    }
    #[must_use]
    pub(crate) fn has_active_leases_for(&self, target: &NativeSessionKey) -> bool {
        let Ok(mut guard) = self.core.active_leases.lock() else {
            return false;
        };
        guard.retain(|(_, entry)| entry.strong_count() > 0);
        guard.iter().any(|(key, entry)| {
            key == target && entry.upgrade().is_some_and(|leases| leases.has_active())
        })
    }
    pub(crate) fn push_custom_status(
        &self,
        target: &NativeSessionKey,
        bytes: Vec<u8>,
    ) -> Result<(), ViewError> {
        let publication = self
            .core
            .target_publication(target)?
            .upgrade()
            .ok_or(ViewError::SourceReleased)?;
        let current = Arc::clone(
            &*publication
                .current
                .read()
                .map_err(|_| ViewError::ReadFailure)?,
        );
        let segment = current
            .snapshot
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .descriptor
            .epoch;
        let mut statuses = current
            .custom_statuses
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        statuses.append(super::segments::CustomStatusEvent { bytes, segment })?;
        drop(statuses);
        self.core.changed.notify_waiters();
        Ok(())
    }
    pub(crate) fn seal_target_send(&self, target: &NativeSessionKey) -> Result<(), ViewError> {
        let publication = self
            .core
            .target_publication(target)?
            .upgrade()
            .ok_or(ViewError::SourceReleased)?;
        let current = Arc::clone(
            &*publication
                .current
                .read()
                .map_err(|_| ViewError::ReadFailure)?,
        );
        current.send.seal()?;
        self.core.changed.notify_waiters();
        Ok(())
    }
}
impl<S, R> SourceCore<S, R> {
    pub(crate) fn seal_handle(&self) -> SourceViewSeal {
        let (flag, ingress) = self.probe.seal_signal();
        SourceViewSeal {
            send: self.send.clone(),
            flag,
            ingress,
            views: Arc::clone(&self.changed),
        }
    }
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.controller.close();
        self.seal_handle().seal()
    }
    pub(crate) fn target(
        &self,
        key: &NativeSessionKey,
    ) -> Result<Arc<TargetGeneration<R>>, ViewError> {
        let target = self
            .target_publication(key)?
            .upgrade()
            .ok_or(ViewError::SourceReleased)?;
        let current = Arc::clone(&*target.current.read().map_err(|_| ViewError::ReadFailure)?);
        Ok(current)
    }
    pub(crate) fn candidate_for_read(
        &self,
        key: &NativeSessionKey,
        read: ReadEpoch,
    ) -> Result<Arc<TargetGeneration<R>>, ViewError> {
        if self.probe.is_sealed() {
            return Err(ViewError::SourceReleased);
        }
        let publication = self
            .target_publication(key)?
            .upgrade()
            .ok_or(ViewError::SourceReleased)?;
        let candidate = publication
            .candidate
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .upgrade()
            .ok_or(ViewError::InvalidStage)?;
        let descriptor = candidate
            .snapshot
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .descriptor;
        if descriptor.source_stamp() != self.stamp
            || descriptor.read_epoch() != Some(read)
            || descriptor.read_mode() != ReadMode::Replaying
            || !candidate.send.current()?
        {
            return Err(ViewError::StaleCursor);
        }
        Ok(candidate)
    }
    #[allow(
        clippy::significant_drop_tightening,
        reason = "The catalog guard prevents last-owner tree destruction during the weak publication lookup"
    )]
    pub(crate) fn target_publication(
        &self,
        key: &NativeSessionKey,
    ) -> Result<std::sync::Weak<TargetPublication<R>>, ViewError> {
        if key.source() != self.source {
            return Err(ViewError::InvalidTarget);
        }
        let guard = self.catalog.read().map_err(|_| ViewError::ReadFailure)?;
        let catalog = guard.as_ref().ok_or(ViewError::SourceReleased)?;
        for index in 0..catalog.len() {
            let target = catalog.get(index)?;
            if target.key == *key {
                return Ok(Arc::downgrade(&target));
            }
        }
        Err(ViewError::InvalidTarget)
    }
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> Worker<S, R> {
    async fn run(
        mut self,
        context: ViewTaskContext,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), ViewError> {
        let result = self.drive(&mut stop).await;
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.core
            .controller
            .hit(
                super::controller::ViewHookPoint::BeforeViewWorkerJoin,
                super::controller::HookContext {
                    scope: Some(self.core.scope.clone()),
                    ..Default::default()
                },
            )
            .await?;
        let _ = self.core.seal();
        let publication = match self.core.catalog.write() {
            Ok(mut catalog) => catalog.take(),
            Err(poison) => poison.into_inner().take(),
        };
        self.core.changed.notify_waiters();
        let remaining = self.consumer.detach()?;
        context.retire((remaining, publication, self)).await?;
        result
    }
    async fn drive(
        &mut self,
        stop: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), ViewError> {
        let mut failure = None;
        loop {
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            self.core
                .controller
                .hit(
                    super::controller::ViewHookPoint::BeforeWorkerDetach,
                    super::controller::HookContext {
                        scope: Some(self.core.scope.clone()),
                        ..Default::default()
                    },
                )
                .await?;
            let batch = self.consumer.detach()?;
            if let Some(error) = self.consumer.take_display_fault() {
                self.mark_display_fault(error)?;
            }
            if batch.is_empty() {
                if self.core.probe.is_sealed() || *stop.borrow() {
                    break;
                }
                tokio::select! {()=self.consumer.wait_work()=>{},_ = stop.changed()=>{let _=self.core.seal();}}
                continue;
            }
            for (index, entry) in batch.into_iter().enumerate() {
                self.revision = self
                    .revision
                    .checked_add(1)
                    .ok_or(ViewError::SequenceExhausted)?;
                match entry {
                    SourceEntry::Raw(frame) => {
                        let sequence = frame.sequence;
                        let context = frame.context.clone();
                        #[cfg(all(
                            windows,
                            any(test, feature = "shared-view-acceptance-fixtures")
                        ))]
                        self.core
                            .controller
                            .hit(
                                super::controller::ViewHookPoint::AfterCaptureEnqueue,
                                super::controller::HookContext {
                                    scope: Some(self.core.scope.clone()),
                                    sequence: Some(sequence),
                                    ..Default::default()
                                },
                            )
                            .await?;
                        let result = self.record(frame);
                        self.consumer.processed(sequence, result, &context);
                    }
                    other => self.apply_control_entry(other, &mut failure),
                }
                self.core.changed.notify_waiters();
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                self.core
                    .controller
                    .hit(
                        super::controller::ViewHookPoint::AfterWorkerCutProcessed,
                        super::controller::HookContext {
                            scope: Some(self.core.scope.clone()),
                            sequence: Some(self.core.probe.processed_frontier()),
                            revision: Some(ViewRevision(self.revision)),
                            ..Default::default()
                        },
                    )
                    .await?;
                if (index + 1) % self.yield_every.get() == 0 {
                    tokio::task::yield_now().await;
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
    fn apply_control_entry(&mut self, entry: SourceEntry<R>, failure: &mut Option<ViewError>) {
        match entry {
            SourceEntry::Raw(_) => {}
            SourceEntry::Bind(marker) => {
                let version = marker.version;
                if let Err(error) = self.bind(marker) {
                    failure.get_or_insert(error);
                    self.consumer.route_failed(error);
                }
                self.consumer.routed(version);
            }
            SourceEntry::ReadComplete(marker) => {
                let version = marker.version;
                if let Err(error) = self.complete(marker) {
                    failure.get_or_insert(error);
                    self.consumer.route_failed(error);
                }
                self.consumer.routed(version);
            }
            SourceEntry::ReadFailure(marker) => {
                let version = marker.version;
                if let Err(error) = self.fail(marker) {
                    failure.get_or_insert(error);
                    self.consumer.route_failed(error);
                }
                self.consumer.routed(version);
            }
            SourceEntry::PreloadCached { key, frames } => {
                if let Err(error) = self.preload_cached(&key, frames) {
                    failure.get_or_insert(error);
                }
            }
        }
    }
    fn preload_cached(
        &mut self,
        key: &NativeSessionKey,
        frames: Vec<Frame<R>>,
    ) -> Result<(), ViewError> {
        if key.source() != self.core.source {
            return Err(ViewError::InvalidTarget);
        }
        self.ensure_target(key)?;
        let already_populated = self.targets.get(key).is_some_and(|target| {
            !target
                .current
                .journal
                .snapshot()
                .map_or(true, |snap| snap.records.len() == 0)
        });
        if already_populated || frames.is_empty() {
            return Ok(());
        }
        if let Some(target) = self.targets.get_mut(key) {
            target.current.principal.descriptor.cache_recoverable = true;
            target.current.principal.origin = SegmentOrigin::RetainedCurrent;
            target.current.principal.limitation = Some("hybrid_resumed_snapshot");
        }
        for frame in frames {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            let envelope = self.projector.inspect(&frame.raw);
            let id = u64::try_from(self.records.snapshot()?.len())
                .map_err(|_| ViewError::SequenceExhausted)?
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            let position = self.records.append(SourceRecord {
                id: RawRecordId(id),
                sequence: frame.sequence,
                receipt_order: frame.receipt_order,
                raw: frame.raw,
                envelope,
                assignment: Mutex::new(None),
            })?;
            let records = self.records.snapshot()?;
            let record = records.get(position)?;
            let target = self.targets.get_mut(key).ok_or(ViewError::InvalidTarget)?;
            Self::route_record(&self.core, target, record, &records, self.revision)?;
        }
        Ok(())
    }
    fn mark_display_fault(&mut self, error: ViewError) -> Result<(), ViewError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        let records = self.records.snapshot()?;
        for target in self.targets.values_mut() {
            target.separate_live = true;
            target.current.principal.descriptor.freshness = Freshness::Stale;
            target.current.principal.descriptor.failure = Some(error);
            target.current.principal.revision = self.revision;
            target.current.principal.descriptor.revision = ViewRevision(self.revision);
            target
                .current
                .segments
                .update(0, target.current.principal, SegmentPhase::Update)?;
            let mut descriptor = target
                .current
                .published
                .snapshot
                .read()
                .map_err(|_| ViewError::ReadFailure)?
                .descriptor;
            descriptor.failure = Some(error);
            descriptor.freshness = Freshness::Stale;
            descriptor.revision = ViewRevision(self.revision);
            descriptor.join = JoinState::UnassociatedSegments;
            if descriptor.mode == ReadMode::ReplayComplete {
                descriptor.mode = ReadMode::ReplayIncomplete;
            }
            Self::publish(&target.current, &records, descriptor)?;
        }
        self.core.changed.notify_waiters();
        Ok(())
    }
    fn record(&mut self, frame: Frame<R>) -> Result<(), ViewError> {
        let envelope = self.projector.inspect(&frame.raw);
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        let envelope = {
            let mut envelope = envelope;
            if let Err(error) = self.core.controller.fault(
                super::controller::ViewFalliblePoint::ProjectionMetadataReserve,
                super::controller::HookContext {
                    scope: Some(self.core.scope.clone()),
                    sequence: Some(frame.sequence),
                    target: envelope.session.as_deref().and_then(|session| {
                        NativeSessionKey::from_config(Some(self.core.source.0), session).ok()
                    }),
                    ..Default::default()
                },
            ) {
                envelope.failure = Some(error);
                envelope.projection = ProjectionState::ProjectionUnavailable;
            }
            envelope
        };
        let id = u64::try_from(self.records.snapshot()?.len())
            .map_err(|_| ViewError::SequenceExhausted)?
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        let position = self.records.append(SourceRecord {
            id: RawRecordId(id),
            sequence: frame.sequence,
            receipt_order: frame.receipt_order,
            raw: frame.raw,
            envelope,
            assignment: Mutex::new(None),
        })?;
        let records = self.records.snapshot()?;
        let record = records.get(position)?;
        if let Some(session) = &record.envelope.session {
            let key = NativeSessionKey::from_config(Some(self.core.source.0), session)
                .map_err(|_| ViewError::InvalidTarget)?;
            if let Some(target) = self.targets.get_mut(&key)
                && let Err(error) =
                    Self::route_record(&self.core, target, record, &records, self.revision)
            {
                if let Some(candidate) = target.candidate.as_mut() {
                    candidate.limited = true;
                }
                self.consumer.route_failed(error);
            }
        }
        Ok(())
    }
    fn generation(
        &self,
        key: &NativeSessionKey,
        read: Option<ReadEpoch>,
        rpc: Option<super::read_control::RpcScalar>,
        restored: Option<&super::send_epoch::RestoredDisplayTarget>,
    ) -> Result<GenerationWorker<R>, ViewError> {
        if key.source() != self.core.source {
            return Err(ViewError::InvalidTarget);
        }
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.core.controller.fault(
            super::controller::ViewFalliblePoint::NewViewEpochRandom,
            super::controller::HookContext {
                scope: Some(self.core.scope.clone()),
                target: Some(key.clone()),
                read,
                ..Default::default()
            },
        )?;
        let journal = IndexedJournal::new()?;
        let projected = journal.snapshot()?;
        let descriptor = ViewDescriptor {
            epoch: projected.epoch,
            read,
            source: self.core.stamp,
            revision: ViewRevision(self.revision),
            mode: if read.is_some() {
                ReadMode::Replaying
            } else {
                ReadMode::CurrentOnly
            },
            freshness: Freshness::CurrentObservation,
            projection: ProjectionState::Available,
            join: if read.is_some() {
                JoinState::RebuiltNoStableIds
            } else {
                JoinState::UnassociatedSegments
            },
            order: OrderBasis::ConnectionObservation,
            cache_recoverable: false,
            failure: None,
        };
        let principal = SegmentMetadata {
            epoch: descriptor.epoch,
            read,
            origin: if read.is_some() {
                SegmentOrigin::RetainedCurrent
            } else {
                SegmentOrigin::LiveObservation
            },
            descriptor,
            begin: None,
            through: None,
            completion: None,
            open: true,
            context: if read.is_some() {
                super::read_observation::ReadContextKind::UniqueReceiptWindow
            } else {
                super::read_observation::ReadContextKind::OutsideReadWindow
            },
            revision: self.revision,
            limitation: read.is_none().then_some("unassociated_observation_range"),
        };
        let mut segments = SegmentLog::new()?;
        segments.append(principal)?;
        let snapshot = Arc::new(TargetSnapshot {
            raw: self.records.snapshot()?,
            projection: projected,
            descriptor,
            segments: segments.snapshot()?,
        });
        let published = Arc::new(TargetGeneration {
            send: match restored {
                Some(restored) => restored.view(&self.core.send, descriptor.epoch)?,
                None => match self.targets.get(key) {
                    Some(target) => target.current.published.send.next_epoch(descriptor.epoch),
                    None => self
                        .core
                        .send
                        .original_view_for_target(descriptor.epoch, key)?,
                },
            },
            snapshot: RwLock::new(snapshot),
            custom_statuses: Mutex::new(AppendLog::new()?),
        });
        Ok(GenerationWorker {
            journal,
            published,
            rpc,
            segments,
            principal,
            limited: false,
            route: 0,
            cut: 0,
            failure_evidence: None,
        })
    }

    fn publish(
        generation: &GenerationWorker<R>,
        records: &LogSnapshot<SourceRecord<R>>,
        descriptor: ViewDescriptor,
    ) -> Result<(), ViewError> {
        let snapshot = Arc::new(TargetSnapshot {
            raw: records.clone(),
            projection: generation.journal.snapshot()?,
            descriptor,
            segments: generation.segments.snapshot()?,
        });
        *generation
            .published
            .snapshot
            .write()
            .map_err(|_| ViewError::ReadFailure)? = snapshot;
        Ok(())
    }
    fn route_record(
        core: &SourceCore<S, R>,
        target: &mut TargetWorker<R>,
        record: Arc<SourceRecord<R>>,
        records: &LogSnapshot<SourceRecord<R>>,
        revision: u64,
    ) -> Result<(), ViewError> {
        if target.published.key.source() != core.source {
            return Err(ViewError::InvalidTarget);
        }
        if let Some(candidate) = target.candidate.as_mut() {
            if Self::belongs(&record, candidate) {
                Self::assign(record, candidate, 0, candidate.principal, records, revision)?;
                return Ok(());
            }
            candidate.limited = true;
        }
        if !target.separate_live
            && target.candidate.is_none()
            && target.current.principal.read.is_none()
            && target.live.is_none()
            && target.current.segments.snapshot()?.publications.len() == 1
        {
            let principal = target.current.principal;
            return Self::assign(record, &mut target.current, 0, principal, records, revision)
                .map(|_| ());
        }
        if target.live.is_none() {
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            core.controller.fault(
                super::controller::ViewFalliblePoint::NewViewEpochRandom,
                super::controller::HookContext {
                    scope: Some(core.scope.clone()),
                    target: Some(target.published.key.clone()),
                    read: target
                        .candidate
                        .as_ref()
                        .and_then(|candidate| candidate.principal.read),
                    ..Default::default()
                },
            )?;
            let epoch = ViewEpoch::fresh()?;
            let mut descriptor = target.current.principal.descriptor;
            descriptor.epoch = epoch;
            descriptor.read = None;
            descriptor.mode = ReadMode::CurrentOnly;
            descriptor.freshness = Freshness::CurrentObservation;
            descriptor.join = JoinState::UnassociatedSegments;
            descriptor.failure = None;
            descriptor.projection = ProjectionState::Available;
            descriptor.revision = ViewRevision(revision);
            let context = record.envelope.observation.kind;
            let metadata = SegmentMetadata {
                epoch,
                read: None,
                origin: SegmentOrigin::LiveObservation,
                descriptor,
                begin: None,
                through: None,
                completion: None,
                open: true,
                context,
                revision,
                limitation: Some(
                    if context == super::read_observation::ReadContextKind::AmbiguousReceiptWindow {
                        "ambiguous_receipt_window"
                    } else {
                        "unassociated_observation_range"
                    },
                ),
            };
            let index = target.current.segments.append(metadata)?;
            target.live = Some((index, metadata));
        }
        let (index, metadata) = target.live.ok_or(ViewError::ReadFailure)?;
        let updated = Self::assign(
            record,
            &mut target.current,
            index,
            metadata,
            records,
            revision,
        )?;
        target.live = Some((index, updated));
        let mut descriptor = target
            .current
            .published
            .snapshot
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .descriptor;
        descriptor.revision = ViewRevision(revision);
        descriptor.join = JoinState::UnassociatedSegments;
        if descriptor.mode != ReadMode::CurrentOnly {
            descriptor.mode = ReadMode::ReplayIncomplete;
        }
        Self::publish(&target.current, records, descriptor)
    }
    fn ensure_target(&mut self, key: &NativeSessionKey) -> Result<(), ViewError> {
        if !self.targets.contains_key(key) {
            let current = self.generation(key, None, None, None)?;
            self.install_target(key, current)?;
        }
        Ok(())
    }
    fn install_target(
        &mut self,
        key: &NativeSessionKey,
        current: GenerationWorker<R>,
    ) -> Result<(), ViewError> {
        self.targets
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let index = self.catalog.append(TargetPublication {
            key: key.clone(),
            current: RwLock::new(Arc::clone(&current.published)),
            candidate: RwLock::new(std::sync::Weak::new()),
            candidate_failure: RwLock::new(None),
        })?;
        let published = self.catalog.snapshot()?.get(index)?;
        self.targets.insert(
            key.clone(),
            TargetWorker {
                published,
                current,
                candidate: None,
                live: None,
                separate_live: false,
            },
        );
        *self
            .core
            .catalog
            .write()
            .map_err(|_| ViewError::ReadFailure)? = Some(self.catalog.snapshot()?);
        Ok(())
    }
    fn bind(&mut self, marker: RouteMarker) -> Result<(), ViewError> {
        if marker.key.source() != self.core.source {
            return Err(ViewError::InvalidTarget);
        }
        if let Some(restored) = marker.restore {
            let result = self.restore_generation(&marker.key, &restored);
            if result.is_err() {
                restored.seal()?;
            }
            return result;
        }
        if self
            .targets
            .get(&marker.key)
            .is_some_and(|target| marker.read.is_some() && target.candidate.is_some())
        {
            return Err(ViewError::ReadFailure);
        }
        self.ensure_target(&marker.key)?;
        if marker.read.is_some()
            && let Some(target) = self.targets.get_mut(&marker.key)
        {
            target.separate_live = true;
        }
        let mut candidate = if marker.read.is_some() {
            Some(self.generation(&marker.key, marker.read, marker.rpc, None)?)
        } else {
            None
        };
        if let Some(candidate) = candidate.as_mut() {
            candidate.route = marker.version;
            candidate.cut = marker.cut;
        }
        let records = self.records.snapshot()?;
        let target = self
            .targets
            .get_mut(&marker.key)
            .ok_or(ViewError::InvalidTarget)?;
        if marker.read.is_some() {
            *target
                .published
                .candidate_failure
                .write()
                .map_err(|_| ViewError::ReadFailure)? = None;
            if let Some((index, mut live)) = target.live.take() {
                live.open = false;
                live.revision = self.revision;
                live.descriptor.revision = ViewRevision(self.revision);
                target
                    .current
                    .segments
                    .update(index, live, SegmentPhase::End)?;
            }
            let mut old = target
                .current
                .published
                .snapshot
                .read()
                .map_err(|_| ViewError::ReadFailure)?
                .descriptor;
            if old.failure.is_none() {
                old.freshness = Freshness::Revalidating;
            }
            old.revision = ViewRevision(self.revision);
            Self::publish(&target.current, &records, old)?;
            *target
                .published
                .candidate
                .write()
                .map_err(|_| ViewError::ReadFailure)? = candidate
                .as_ref()
                .map_or_else(std::sync::Weak::new, |candidate| {
                    Arc::downgrade(&candidate.published)
                });
            target.candidate = candidate;
        }
        for index in 0..records.len() {
            let record = records.get(index)?;
            if record.sequence > marker.cut
                || record.envelope.session.as_deref() != Some(marker.key.native_id())
            {
                continue;
            }
            let unassigned = record
                .assignment
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .is_none();
            if unassigned {
                self.revision = self
                    .revision
                    .checked_add(1)
                    .ok_or(ViewError::SequenceExhausted)?;
                Self::route_record(&self.core, target, record, &records, self.revision)?;
            }
        }
        Ok(())
    }
    /// Only an explicitly admitted restore publishes this marker into the original FIFO.
    /// Retired generations retain their sealed send flags and original journals.
    fn restore_generation(
        &mut self,
        key: &NativeSessionKey,
        restored: &super::send_epoch::RestoredDisplayTarget,
    ) -> Result<(), ViewError> {
        let current = self.generation(key, None, None, Some(restored))?;
        if !self.targets.contains_key(key) {
            return self.install_target(key, current);
        }
        self.retired
            .try_reserve(2)
            .map_err(|_| ViewError::AllocationFailure)?;
        let target = self.targets.get_mut(key).ok_or(ViewError::InvalidTarget)?;
        let mut published = target
            .published
            .current
            .write()
            .map_err(|_| ViewError::ReadFailure)?;
        let mut candidate = target
            .published
            .candidate
            .write()
            .map_err(|_| ViewError::ReadFailure)?;
        let mut failure = target
            .published
            .candidate_failure
            .write()
            .map_err(|_| ViewError::ReadFailure)?;
        *published = Arc::clone(&current.published);
        *candidate = std::sync::Weak::new();
        *failure = None;
        drop(failure);
        drop(candidate);
        drop(published);
        self.retired
            .push(std::mem::replace(&mut target.current, current));
        if let Some(old) = target.candidate.take() {
            self.retired.push(old);
        }
        target.live = None;
        target.separate_live = false;
        Ok(())
    }
    fn belongs(record: &SourceRecord<R>, generation: &GenerationWorker<R>) -> bool {
        let observation = &record.envelope.observation;
        observation.kind == super::read_observation::ReadContextKind::UniqueReceiptWindow
            && observation.window.as_ref().is_some_and(|window| {
                generation.rpc.as_ref() == Some(&window.rpc)
                    && record.envelope.session.as_deref() == Some(window.session.as_ref())
                    && window.kind == super::read_observation::ReadWindowKind::Replay
            })
    }
    fn assign(
        record: Arc<SourceRecord<R>>,
        generation: &mut GenerationWorker<R>,
        index: usize,
        mut metadata: SegmentMetadata,
        records: &LogSnapshot<SourceRecord<R>>,
        revision: u64,
    ) -> Result<SegmentMetadata, ViewError> {
        *record
            .assignment
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = Some(metadata.epoch);
        let platform_revision = revision;
        let projected = generation.journal.append_segment_at(
            metadata.epoch,
            index,
            record.id,
            platform_revision,
            record.raw.clone(),
            record.sequence,
            record.receipt_order,
            record.envelope.clone(),
        );
        if metadata.begin.is_none() {
            metadata.context = record.envelope.observation.kind;
        }
        metadata.begin.get_or_insert(record.sequence);
        metadata.through = Some(record.sequence);
        metadata.revision = platform_revision;
        metadata.descriptor.revision = ViewRevision(platform_revision);
        metadata.descriptor.failure = metadata.descriptor.failure.or(record.envelope.failure);
        if record.envelope.projection != ProjectionState::Available {
            metadata.descriptor.projection = record.envelope.projection;
        }
        if let Err(error) = projected {
            metadata.descriptor.failure = Some(error);
            metadata.descriptor.projection = ProjectionState::ProjectionUnavailable;
        }
        generation
            .segments
            .update(index, metadata, SegmentPhase::Update)?;
        if index == 0 {
            generation.principal = metadata;
        }
        let mut descriptor = generation
            .published
            .snapshot
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .descriptor;
        descriptor.revision = ViewRevision(platform_revision);
        descriptor.failure = descriptor.failure.or(record.envelope.failure);
        descriptor.projection = generation.journal.snapshot()?.projection;
        Self::publish(generation, records, descriptor)?;
        drop(record);
        Ok(metadata)
    }
    fn check_response_record(&self, sequence: u64, order: u64) -> Result<(), ViewError> {
        if self.core.probe.processed_frontier() < sequence {
            return Err(ViewError::CaptureGap);
        }
        let records = self.records.snapshot()?;
        let mut low = 0;
        let mut high = records.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if records.get(middle)?.sequence < sequence {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let record = records.get(low)?;
        if record.sequence != sequence || record.receipt_order != order {
            return Err(ViewError::CaptureGap);
        }
        Ok(())
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "The exclusive worker consumes the original completion command exactly once"
    )]
    fn complete(&mut self, marker: ReadCompleteMarker) -> Result<(), ViewError> {
        self.check_response_record(marker.sequence, marker.receipt_order)?;
        let records = self.records.snapshot()?;
        let target = self
            .targets
            .get_mut(&marker.key)
            .ok_or(ViewError::InvalidTarget)?;
        let candidate = target.candidate.as_mut().ok_or(ViewError::StaleCursor)?;
        if candidate.principal.read != Some(marker.read) {
            return Err(ViewError::StaleCursor);
        }
        let fault = marker.prefix.failure();
        candidate.limited |= marker.prefix.association_limited();
        if fault.is_some() {
            candidate.principal.origin = SegmentOrigin::FailedReplay;
            candidate.principal.limitation = Some("capture_gap");
        } else if candidate.limited {
            candidate.principal.limitation = Some("unassociated_observation_range");
            candidate.principal.descriptor.join = JoinState::UnassociatedSegments;
        }
        candidate.principal.descriptor.mode = if fault.is_none() {
            ReadMode::ReplayComplete
        } else {
            ReadMode::ReplayIncomplete
        };
        candidate.principal.descriptor.failure = candidate.principal.descriptor.failure.or(fault);
        candidate.principal.completion = fault.is_none().then_some(marker.sequence);
        candidate.principal.open = false;
        candidate.principal.revision = self.revision;
        candidate.principal.descriptor.revision = ViewRevision(self.revision);
        if self.core.probe.is_sealed() {
            candidate.principal.descriptor.freshness = Freshness::Stale;
        }
        candidate
            .segments
            .update(0, candidate.principal, SegmentPhase::End)?;
        let mut descriptor = candidate.principal.descriptor;
        if candidate.limited || fault.is_some() {
            descriptor.mode = ReadMode::ReplayIncomplete;
            descriptor.join = JoinState::UnassociatedSegments;
        }
        Self::publish(candidate, &records, descriptor)?;
        if candidate.limited || fault.is_some() {
            let candidate = target.candidate.take().ok_or(ViewError::ReadFailure)?;
            candidate.published.send.seal()?;
            *target
                .published
                .candidate_failure
                .write()
                .map_err(|_| ViewError::ReadFailure)? =
                Some((marker.read, fault.unwrap_or(ViewError::StaleCursor)));
            *target
                .published
                .candidate
                .write()
                .map_err(|_| ViewError::ReadFailure)? = std::sync::Weak::new();
            Self::append_retired_segment(target, &candidate, &records, &mut self.revision)?;
            self.retired
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            self.retired.push(candidate);
        } else {
            self.retired
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            let candidate = target.candidate.take().ok_or(ViewError::ReadFailure)?;
            target.current.published.send.seal()?;
            *target
                .published
                .current
                .write()
                .map_err(|_| ViewError::ReadFailure)? = Arc::clone(&candidate.published);
            *target
                .published
                .candidate
                .write()
                .map_err(|_| ViewError::ReadFailure)? = std::sync::Weak::new();
            let old = std::mem::replace(&mut target.current, candidate);
            self.retired.push(old);
            target.live = None;
        }
        Ok(())
    }
    fn append_retired_segment(
        target: &mut TargetWorker<R>,
        candidate: &GenerationWorker<R>,
        records: &LogSnapshot<SourceRecord<R>>,
        revision: &mut u64,
    ) -> Result<(), ViewError> {
        target.current.principal.descriptor.freshness = Freshness::Stale;
        target.current.principal.descriptor.failure = target
            .current
            .principal
            .descriptor
            .failure
            .or(candidate.principal.descriptor.failure);
        target.current.principal.revision = *revision;
        target.current.principal.descriptor.revision = ViewRevision(*revision);
        target
            .current
            .segments
            .update(0, target.current.principal, SegmentPhase::Update)?;
        let mut metadata = candidate.principal;
        metadata.revision = *revision;
        metadata.descriptor.revision = ViewRevision(*revision);
        let index = target.current.segments.append(metadata)?;
        let raw = candidate.journal.snapshot()?.records;
        for position in 0..raw.len() {
            *revision = revision
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            let id = raw
                .get(position)?
                .id
                .as_u64()
                .checked_sub(1)
                .ok_or(ViewError::InvalidCursor)?;
            let record =
                records.get(usize::try_from(id).map_err(|_| ViewError::LengthUnrepresentable)?)?;
            metadata = Self::assign(
                record,
                &mut target.current,
                index,
                metadata,
                records,
                *revision,
            )?;
        }
        metadata.revision = *revision;
        target
            .current
            .segments
            .update(index, metadata, SegmentPhase::End)?;
        let mut descriptor = target
            .current
            .published
            .snapshot
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .descriptor;
        descriptor.mode = ReadMode::ReplayIncomplete;
        descriptor.join = JoinState::UnassociatedSegments;
        descriptor.freshness = Freshness::Stale;
        descriptor.failure = descriptor.failure.or(metadata.descriptor.failure);
        descriptor.revision = ViewRevision(*revision);
        Self::publish(&target.current, records, descriptor)
    }
    fn fail(&mut self, marker: ReadFailureMarker<R>) -> Result<(), ViewError> {
        match &marker.evidence {
            ReadFailureEvidence::Response {
                raw,
                sequence,
                order,
            } => {
                if self.core.probe.observed_frontier() < *sequence {
                    return Err(ViewError::CaptureGap);
                }
                let _ = order;
                let _ = raw.len();
            }
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            ReadFailureEvidence::Transport(original) | ReadFailureEvidence::Lifecycle(original) => {
                let _ = Arc::strong_count(original);
            }
            #[cfg(not(all(windows, any(test, feature = "shared-view-acceptance-fixtures"))))]
            ReadFailureEvidence::Transport(original) => {
                let _ = Arc::strong_count(original);
            }
        }
        let records = self.records.snapshot()?;
        let target = self
            .targets
            .get_mut(&marker.key)
            .ok_or(ViewError::InvalidTarget)?;
        let matches = |generation: &GenerationWorker<R>| {
            generation.principal.read == Some(marker.read)
                && generation.rpc.as_ref() == Some(&marker.rpc)
                && generation.route == marker.route
                && generation.cut == marker.cut
        };
        if let Some(candidate) = target
            .candidate
            .as_mut()
            .filter(|candidate| matches(candidate))
        {
            if candidate.principal.descriptor.mode == ReadMode::ReplayComplete {
                candidate.failure_evidence = Some(marker.evidence);
                return Ok(());
            }
            candidate.principal.descriptor.mode = ReadMode::ReplayIncomplete;
            candidate.principal.descriptor.freshness = Freshness::Stale;
            candidate.principal.descriptor.failure = Some(marker.error);
            candidate.principal.origin = SegmentOrigin::FailedReplay;
            candidate.principal.open = false;
            candidate.principal.revision = self.revision;
            candidate.principal.descriptor.revision = ViewRevision(self.revision);
            candidate.failure_evidence = Some(marker.evidence);
            candidate
                .segments
                .update(0, candidate.principal, SegmentPhase::End)?;
            Self::publish(candidate, &records, candidate.principal.descriptor)?;
            let candidate = target.candidate.take().ok_or(ViewError::ReadFailure)?;
            candidate.published.send.seal()?;
            *target
                .published
                .candidate_failure
                .write()
                .map_err(|_| ViewError::ReadFailure)? = Some((marker.read, marker.error));
            *target
                .published
                .candidate
                .write()
                .map_err(|_| ViewError::ReadFailure)? = std::sync::Weak::new();
            Self::append_retired_segment(target, &candidate, &records, &mut self.revision)?;
            self.retired
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            self.retired.push(candidate);
        } else if matches(&target.current) {
            // A later transport/source failure cannot withdraw the actual completed segment.
            target.current.failure_evidence = Some(marker.evidence);
        } else if let Some(original) = self
            .retired
            .iter_mut()
            .find(|generation| matches(generation))
        {
            original.failure_evidence = Some(marker.evidence);
        }
        Ok(())
    }
}
