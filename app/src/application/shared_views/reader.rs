#![allow(
    clippy::significant_drop_tightening,
    reason = "Intent/position guards intentionally cover one atomic publication; every wait, IO and async hook is outside these guards"
)]
//! Qualified immutable H/tail readers. Transport commits remain owned by the actual sender.
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
use super::local_transfer::{LocalViewStream, PreparedLocalChunk};
use super::{
    CurrentViewAuthority, ReadClientBinding, RetainedRaw,
    budget::ReadReason,
    read_control::{NativeReadControl, NativeReadRequest},
    source::{SharedViews, TargetGeneration, TargetSnapshot},
    tasks::{ViewTaskKind, ViewTaskSupervisor},
    transfer_state::{PreparedTransferChunk, ReadTransferStream},
    wire::{MAX_PAYLOAD_BYTES, RecordKind},
};
use crate::domain::{
    EntityId,
    native::NativeSessionKey,
    shared_view::{RawRecordId, ViewDescriptor, ViewEpoch, ViewError, ViewRevision},
};
use std::{
    any::Any,
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::watch;
mod original_read;
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
pub(crate) use original_read::OriginalReadObserver;
pub(crate) use original_read::OriginalReadProgress;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CursorPhase {
    Snapshot,
    Tail,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    Raw,
    Item,
    Evidence,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Position {
    phase: CursorPhase,
    index: usize,
    offset: usize,
    section: Section,
    item: usize,
    delta: usize,
    evidence_field: usize,
    evidence_index: usize,
    projection_record: u64,
    after: u64,
    announced: usize,
    ended: usize,
    status_delta: usize,
    status_record: u64,
    active_segment: Option<ViewEpoch>,
    native_failure: Option<ViewError>,
}
struct SelectionLeases {
    active: std::sync::atomic::AtomicUsize,
    changed: tokio::sync::Notify,
}
impl SelectionLeases {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            active: std::sync::atomic::AtomicUsize::new(0),
            changed: tokio::sync::Notify::new(),
        })
    }
    fn claim(self: &Arc<Self>) -> Result<SelectionLease, ViewError> {
        self.active
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |value| value.checked_add(1),
            )
            .map_err(|_| ViewError::SequenceExhausted)?;
        Ok(SelectionLease(Arc::clone(self)))
    }
    async fn wait_empty(&self) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.active.load(std::sync::atomic::Ordering::Acquire) == 0 {
                return;
            }
            changed.await;
        }
    }
}
struct SelectionLease(Arc<SelectionLeases>);
impl Drop for SelectionLease {
    fn drop(&mut self) {
        self.0
            .active
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        self.0.changed.notify_waiters();
    }
}
struct SelectedRaw<R> {
    raw: R,
    _lease: SelectionLease,
}
impl<R: RetainedRaw> SelectedRaw<R> {
    fn len(&self) -> usize {
        self.raw.len()
    }
    fn bytes(&self) -> &[u8] {
        self.raw.bytes()
    }
}
struct DeferredRetirement<T> {
    value: Option<T>,
    send: Option<tokio::sync::oneshot::Sender<T>>,
}
impl<T> Drop for DeferredRetirement<T> {
    fn drop(&mut self) {
        if let (Some(value), Some(send)) = (self.value.take(), self.send.take()) {
            let _ = send.send(value);
        }
    }
}
enum Selection<R> {
    Raw {
        raw: SelectedRaw<R>,
        id: u64,
        total: u64,
        position: Position,
        after_on_finish: Option<u64>,
        source_range: SourceRange,
        segment: ViewEpoch,
    },
    Metadata {
        bytes: Vec<u8>,
        position: Position,
        next: Position,
        id: u64,
        kind: RecordKind,
        segment: ViewEpoch,
    },
    End {
        position: Position,
        delta: usize,
        status_delta: usize,
    },
    Wait,
    Finished,
    Credit,
}
struct PendingPosition {
    packet: u64,
    next: Position,
}
struct ReadData<R> {
    snapshot: Arc<TargetSnapshot<R>>,
    generation: Arc<TargetGeneration<R>>,
    position: Position,
    pending: Option<PendingPosition>,
    last_descriptor: ViewDescriptor,
}
struct ReadSlot<R> {
    data: Mutex<Option<ReadData<R>>>,
    stream: ReadTransferStream,
    samples: Mutex<(SnapshotCursor, DeltaCursor)>,
    leases: Arc<SelectionLeases>,
}
trait CursorSampler: Send + Sync {
    fn sample(&self) -> (SnapshotCursor, DeltaCursor);
    fn freshness(&self) -> crate::domain::shared_view::Freshness;
}
impl<R: RetainedRaw> CursorSampler for ReadSlot<R> {
    fn freshness(&self) -> crate::domain::shared_view::Freshness {
        let data = match self.data.lock() {
            Ok(data) => data,
            Err(poison) => poison.into_inner(),
        };
        let Some(data) = data.as_ref() else {
            return crate::domain::shared_view::Freshness::Stale;
        };
        let current = match data.generation.snapshot.read() {
            Ok(current) => current,
            Err(poison) => poison.into_inner(),
        };
        current.descriptor.freshness()
    }
    fn sample(&self) -> (SnapshotCursor, DeltaCursor) {
        let committed = self
            .stream
            .observation()
            .map_or(0, |state| state.highest_committed);
        let mut samples = match self.samples.lock() {
            Ok(samples) => samples,
            Err(poison) => poison.into_inner(),
        };
        let mut data = match self.data.lock() {
            Ok(data) => data,
            Err(poison) => poison.into_inner(),
        };
        if let Some(data) = data.as_mut() {
            synchronize_position(data, committed);
            let position = data.position;
            let id = if position.phase == CursorPhase::Snapshot {
                data.snapshot
                    .projection
                    .records
                    .get(position.index)
                    .ok()
                    .map_or(RawRecordId(0), |raw| raw.id)
            } else {
                RawRecordId(0)
            };
            samples.0.next = id;
            samples.0.offset = position.offset as u64;
            samples.0.exhausted = position.phase == CursorPhase::Tail;
            samples.1.after = ViewRevision(position.after);
            if position.phase == CursorPhase::Tail {
                let generation = match data.generation.snapshot.read() {
                    Ok(generation) => generation,
                    Err(poison) => poison.into_inner(),
                };
                samples.1.next = generation
                    .projection
                    .records
                    .get(position.index)
                    .ok()
                    .map_or(RawRecordId(0), |raw| raw.id);
                samples.1.offset = position.offset as u64;
            }
        }
        *samples
    }
}
fn synchronize_position<R>(data: &mut ReadData<R>, committed: u64) {
    if data
        .pending
        .as_ref()
        .is_some_and(|pending| pending.packet <= committed)
        && let Some(pending) = data.pending.take()
    {
        data.position = pending.next;
    }
}
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
struct ReaderIoHooks<S> {
    controller: super::controller::ViewTestController<S>,
    context: super::controller::HookContext<S>,
}
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
impl<S: Clone + Eq + Send + Sync + 'static> super::local_transfer::LocalIoHooks
    for ReaderIoHooks<S>
{
    fn observe(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> Result<(), ViewError> {
        let mut context = self.context.clone();
        context.packet = Some(packet);
        self.controller.observe(point, context)
    }
    fn pause(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> super::BoxReadFuture<'_, Result<(), ViewError>> {
        let mut context = self.context.clone();
        context.packet = Some(packet);
        Box::pin(self.controller.hit(point, context))
    }
}
struct ReadHandle {
    binding: ReadClientBinding,
    target: NativeSessionKey,
    stream: ReadTransferStream,
    descriptor: ViewDescriptor,
    data: Weak<dyn Any + Send + Sync>,
    close: watch::Sender<bool>,
    service: Arc<()>,
    intent: Mutex<Option<AcceptedIntent>>,
    sampler: Arc<dyn CursorSampler>,
    source_sealed: Arc<std::sync::atomic::AtomicBool>,
}
impl ReadHandle {
    fn native_failure(&self) -> Option<ViewError> {
        let intent = self.intent.lock().ok()?;
        let request = &intent.as_ref()?.request;
        let metadata = request.subscribe().borrow().clone();
        (metadata.read_mode() == crate::domain::shared_view::ReadMode::ReplayIncomplete)
            .then(|| metadata.failure())
            .flatten()
    }
}
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
impl super::probe::StreamInspectionPort for ReadHandle {
    fn inspect(&self) -> Option<super::probe::StreamInspection> {
        let (snapshot, delta) = self.sampler.sample();
        Some(super::probe::StreamInspection {
            stream: EntityId::from_bytes(*self.stream.id().as_bytes()),
            epoch: self.descriptor.view_epoch(),
            read: self.descriptor.read_epoch(),
            observed: self.stream.observation().ok()?,
            snapshot,
            delta,
        })
    }
}
enum RebindStage {
    Available,
    Preparing(EntityId),
    Consumed,
}
struct AcceptedIntent {
    request: Arc<NativeReadRequest>,
    stage: RebindStage,
}
/// Unique consumer owner. Sampled cursors never grant advancement.
pub struct OpenedView {
    handle: Arc<ReadHandle>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadStreamState {
    AwaitInitialCredit,
    Ready,
    Prepared,
    Started,
    Flushing,
    Sent,
    Closed,
    ClosingAfterStarted,
}
#[derive(Clone)]
pub struct ReadCloseReceipt {
    stream: EntityId,
    already: bool,
    committed: u64,
    started: Option<u64>,
    state: ReadStreamState,
}
impl ReadCloseReceipt {
    #[must_use]
    pub const fn stream_id(&self) -> EntityId {
        self.stream
    }
    #[must_use]
    pub const fn was_already_closed(&self) -> bool {
        self.already
    }
    #[must_use]
    pub const fn last_committed_packet(&self) -> u64 {
        self.committed
    }
    #[must_use]
    pub const fn started_packet(&self) -> Option<u64> {
        self.started
    }
    #[must_use]
    pub const fn state(&self) -> ReadStreamState {
        self.state
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotCursor {
    epoch: ViewEpoch,
    highwater: ViewRevision,
    next: RawRecordId,
    offset: u64,
    exhausted: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeltaCursor {
    epoch: ViewEpoch,
    after: ViewRevision,
    next: RawRecordId,
    offset: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordAddress {
    epoch: ViewEpoch,
    record: u64,
    kind: RecordKind,
    offset: u64,
    total: u64,
    length: u32,
    last: bool,
    segment: ViewEpoch,
}
impl RecordAddress {
    pub(crate) fn from_header(
        epoch: ViewEpoch,
        header: super::wire::TransferHeader,
    ) -> Result<Self, ViewError> {
        Ok(Self {
            epoch,
            segment: epoch,
            record: header.record(),
            kind: header.record_kind().ok_or(ViewError::ProtocolFailure)?,
            offset: header.offset(),
            total: header.total(),
            length: header.payload_length(),
            last: header.is_last(),
        })
    }
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn segment_id(self) -> EntityId {
        self.segment.as_entity_id()
    }
    #[must_use]
    pub const fn segment_view_epoch(self) -> ViewEpoch {
        self.segment
    }
    pub(crate) const fn in_segment(mut self, segment: ViewEpoch) -> Self {
        self.segment = segment;
        self
    }
    #[must_use]
    pub const fn record_id(self) -> u64 {
        self.record
    }
    #[must_use]
    pub const fn kind(self) -> RecordKind {
        self.kind
    }
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.offset
    }
    #[must_use]
    pub const fn total_length(self) -> u64 {
        self.total
    }
    #[must_use]
    pub const fn payload_length(self) -> u32 {
        self.length
    }
    #[must_use]
    pub const fn is_last(self) -> bool {
        self.last
    }
}
#[derive(Clone, Copy)]
pub struct ChunkMetadata {
    address: RecordAddress,
    snapshot: ViewRevision,
    source: crate::domain::shared_view::ViewSourceStamp,
    range: Option<SourceRange>,
}
#[derive(Clone, Copy)]
pub struct SourceRange {
    sequence: u64,
    order: u64,
}
impl SourceRange {
    #[must_use]
    pub const fn first_sequence(self) -> u64 {
        self.sequence
    }
    #[must_use]
    pub const fn last_sequence(self) -> u64 {
        self.sequence
    }
    #[must_use]
    pub const fn first_receipt_order(self) -> u64 {
        self.order
    }
    #[must_use]
    pub const fn last_receipt_order(self) -> u64 {
        self.order
    }
}
impl ChunkMetadata {
    #[must_use]
    pub const fn source_stamp(self) -> crate::domain::shared_view::ViewSourceStamp {
        self.source
    }
    #[must_use]
    pub const fn source_range(self) -> Option<SourceRange> {
        self.range
    }
    #[must_use]
    pub const fn address(self) -> RecordAddress {
        self.address
    }
    #[must_use]
    pub const fn snapshot_revision(self) -> ViewRevision {
        self.snapshot
    }
}
pub struct PreparedViewChunk {
    local: PreparedTransferChunk,
    metadata: ChunkMetadata,
}
impl PreparedViewChunk {
    pub(crate) fn into_socket(
        self,
    ) -> Result<super::socket_transfer::PreparedSocketChunk, ViewError> {
        self.local.into_socket()
    }
    #[must_use]
    pub const fn metadata(&self) -> ChunkMetadata {
        self.metadata
    }
    #[must_use]
    pub const fn address(&self) -> RecordAddress {
        self.metadata.address
    }
    #[must_use]
    pub const fn stream_id(&self) -> EntityId {
        EntityId::from_bytes(*self.local.header().stream().as_bytes())
    }
    #[must_use]
    pub const fn packet_sequence(&self) -> u64 {
        self.local.packet_sequence()
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn into_local(self) -> PreparedLocalChunk {
        self.local.into_local()
    }
}
#[allow(
    clippy::large_enum_variant,
    reason = "The frozen linear API transfers the prepared owned packet directly without an extra fallible allocation"
)]
pub enum PrepareNext {
    Chunk(PreparedViewChunk),
    AwaitCredit,
    AwaitData,
    SnapshotFinished,
    Closed,
}
pub(crate) struct ReadRebind {
    read: OpenedView,
    metadata: ReadRebindMetadata,
}
#[derive(Clone, Copy)]
pub(crate) struct ReadRebindMetadata {
    pub previous_stream: EntityId,
    pub stream: EntityId,
    pub epoch: ViewEpoch,
    pub read: crate::domain::shared_view::ReadEpoch,
    pub revision: ViewRevision,
    pub source: crate::domain::shared_view::ViewSourceStamp,
}
impl ReadRebind {
    pub(crate) fn into_parts(self) -> (OpenedView, ReadRebindMetadata) {
        (self.read, self.metadata)
    }
}
struct RebindClaim {
    handle: Arc<ReadHandle>,
    nonce: EntityId,
    finished: bool,
}
impl Drop for RebindClaim {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Ok(mut intent) = self.handle.intent.lock()
            && let Some(intent) = intent.as_mut()
            && matches!(intent.stage,RebindStage::Preparing(nonce) if nonce==self.nonce)
        {
            intent.stage = RebindStage::Available;
        }
    }
}
pub struct ViewReadService<S, R, A, C> {
    views: SharedViews<S, R>,
    authority: Arc<A>,
    supervisor: ViewTaskSupervisor,
    client: PhantomData<fn(&C)>,
    native_reads: Option<Arc<dyn NativeReadControl>>,
    identity: Arc<()>,
}
impl SnapshotCursor {
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn snapshot_revision(self) -> ViewRevision {
        self.highwater
    }
    #[must_use]
    pub const fn next_record(self) -> RawRecordId {
        self.next
    }
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.offset
    }
    #[must_use]
    pub const fn exhausted(self) -> bool {
        self.exhausted
    }
}
impl DeltaCursor {
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn after_revision(self) -> ViewRevision {
        self.after
    }
    #[must_use]
    pub const fn next_record(self) -> RawRecordId {
        self.next
    }
    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.offset
    }
}
impl OpenedView {
    pub(crate) fn socket_stream(
        &self,
    ) -> Result<super::socket_transfer::SocketViewStream, ViewError> {
        self.handle.stream.socket()
    }
    #[must_use]
    pub fn snapshot_cursor(&self) -> SnapshotCursor {
        self.handle.sampler.sample().0
    }
    #[must_use]
    pub fn delta_cursor(&self) -> DeltaCursor {
        self.handle.sampler.sample().1
    }
    #[must_use]
    pub fn state(&self) -> ReadStreamState {
        use super::local_transfer::LocalTransferPhase;
        let Some(observed) = self.handle.stream.observation().ok() else {
            return ReadStreamState::Closed;
        };
        if self.handle.stream.ready().is_err() {
            return if matches!(
                observed.phase,
                LocalTransferPhase::Started | LocalTransferPhase::Flushing
            ) {
                ReadStreamState::ClosingAfterStarted
            } else {
                ReadStreamState::Closed
            };
        }
        match observed.phase {
            LocalTransferPhase::AwaitCredit => ReadStreamState::AwaitInitialCredit,
            LocalTransferPhase::Ready => ReadStreamState::Ready,
            LocalTransferPhase::Prepared => ReadStreamState::Prepared,
            LocalTransferPhase::Started => ReadStreamState::Started,
            LocalTransferPhase::Flushing => ReadStreamState::Flushing,
            LocalTransferPhase::Sent => ReadStreamState::Sent,
            LocalTransferPhase::Closed => ReadStreamState::Closed,
        }
    }
    #[must_use]
    pub fn stream_id(&self) -> EntityId {
        EntityId::from_bytes(*self.handle.stream.id().as_bytes())
    }
    #[must_use]
    pub fn descriptor(&self) -> ViewDescriptor {
        let mut descriptor = self.handle.descriptor;
        descriptor.freshness = self.handle.sampler.freshness();
        descriptor.failure = descriptor.failure.or_else(|| self.handle.native_failure());
        if self
            .handle
            .source_sealed
            .load(std::sync::atomic::Ordering::Acquire)
            || !self.handle.stream.view_current().unwrap_or(false)
        {
            descriptor.freshness = crate::domain::shared_view::Freshness::Stale;
        }
        descriptor
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn stream(&self) -> LocalViewStream {
        self.handle.stream.local()
    }
}
impl Drop for OpenedView {
    fn drop(&mut self) {
        let _ = self.handle.stream.seal();
        self.handle.close.send_replace(true);
    }
}
impl<S, R, A, C> ViewReadService<S, R, A, C>
where
    S: Clone + Eq + Send + Sync + 'static,
    R: RetainedRaw,
    A: CurrentViewAuthority<C>,
    C: Send + Sync + 'static,
{
    fn retirement<T: Send + 'static>(&self, value: T) -> Result<DeferredRetirement<T>, ViewError> {
        let (send, receive) = tokio::sync::oneshot::channel();
        self.supervisor
            .spawn_owned(ViewTaskKind::Cleanup, move |context| async move {
                if let Ok(value) = receive.await {
                    context.retire(value).await?;
                }
                Ok(())
            })?;
        Ok(DeferredRetirement {
            value: Some(value),
            send: Some(send),
        })
    }
    pub(crate) fn new(
        views: SharedViews<S, R>,
        authority: Arc<A>,
        supervisor: ViewTaskSupervisor,
    ) -> Self {
        Self {
            views,
            authority,
            supervisor,
            client: PhantomData,
            native_reads: None,
            identity: Arc::new(()),
        }
    }
    pub(crate) fn uses_authority(&self, authority: &A) -> bool {
        std::ptr::eq(self.authority.as_ref(), authority)
    }
    pub(crate) fn set_read_control(&mut self, reads: Arc<dyn NativeReadControl>) {
        self.native_reads = Some(reads);
    }
    fn slot(&self, client: &C, read: &OpenedView) -> Result<Arc<ReadSlot<R>>, ViewError> {
        if !Arc::ptr_eq(&self.identity, &read.handle.service)
            || !read
                .handle
                .binding
                .same_client(&self.authority.client_binding(client)?)
        {
            return Err(ViewError::InvalidClient);
        }
        if *read.handle.close.borrow() {
            return Err(ViewError::StreamSealed);
        }
        read.handle
            .data
            .upgrade()
            .ok_or(ViewError::StaleCursor)?
            .downcast::<ReadSlot<R>>()
            .map_err(|_| ViewError::InvalidClient)
    }
    /// Internal current-observation capture. External open also registers its actual read intent.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep consumer ownership registration and cancellation-before-publication in one auditable transaction"
    )]
    pub(crate) async fn open_current(
        &self,
        client: &C,
        target: &NativeSessionKey,
    ) -> Result<OpenedView, ViewError> {
        self.open_selected(client, target, None).await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep original consumer ownership and cancellation publication together for an actual selected generation"
    )]
    async fn open_selected(
        &self,
        client: &C,
        target: &NativeSessionKey,
        selected: Option<Arc<TargetGeneration<R>>>,
    ) -> Result<OpenedView, ViewError> {
        let binding = self.authority.client_binding(client)?;
        let (cut, route) = self
            .authority
            .with_current(client, target, || {
                if self.views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                Ok((
                    self.views.core.probe.retained_frontier(),
                    self.views.core.probe.route_version(),
                ))
            })
            .await?;
        loop {
            tokio::select! {
                result=self.views.core.probe.wait_barrier(cut,route)=>{result?;break;},
                changed=self.authority.wait_client_change(client)=>{changed?;}
            }
        }
        let publication = self.views.core.target_publication(target)?;
        let mut retained = self.retirement((
            None::<Arc<super::source::TargetPublication<R>>>,
            None::<Arc<TargetGeneration<R>>>,
            None::<Arc<TargetSnapshot<R>>>,
        ))?;
        let stream = self
            .authority
            .with_current(client, target, || {
                if self.views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                let (retained_publication, retained_generation, retained_snapshot) =
                    retained.value.as_mut().ok_or(ViewError::ReadFailure)?;
                *retained_publication = publication.upgrade();
                let publication = retained_publication
                    .as_ref()
                    .ok_or(ViewError::SourceReleased)?;
                *retained_generation = Some(if let Some(generation) = &selected {
                    Arc::clone(generation)
                } else {
                    Arc::clone(
                        &*publication
                            .current
                            .read()
                            .map_err(|_| ViewError::ReadFailure)?,
                    )
                });
                let generation = retained_generation.as_ref().ok_or(ViewError::ReadFailure)?;
                *retained_snapshot = Some(Arc::clone(
                    &*generation
                        .snapshot
                        .read()
                        .map_err(|_| ViewError::ReadFailure)?,
                ));
                #[cfg(not(all(windows, any(test, feature = "shared-view-acceptance-fixtures"))))]
                retained_snapshot.as_ref().ok_or(ViewError::ReadFailure)?;
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                {
                    let snapshot = retained_snapshot.as_ref().ok_or(ViewError::ReadFailure)?;
                    self.views.core.controller.fault(
                        super::controller::ViewFalliblePoint::NewStreamIdRandom,
                        super::controller::HookContext {
                            scope: Some(self.views.core.scope.clone()),
                            target: Some(target.clone()),
                            epoch: Some(snapshot.descriptor.view_epoch()),
                            read: snapshot.descriptor.read_epoch(),
                            ..Default::default()
                        },
                    )?;
                }
                let stream = ReadTransferStream::open(
                    &generation.send,
                    binding.clone(),
                    self.authority.as_ref(),
                    client,
                    target,
                )?;
                Ok(stream)
            })
            .await?;
        let (_, generation, snapshot) = retained.value.as_mut().ok_or(ViewError::ReadFailure)?;
        let generation = generation.take().ok_or(ViewError::ReadFailure)?;
        let snapshot = snapshot.take().ok_or(ViewError::ReadFailure)?;
        let descriptor = snapshot.descriptor;
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        stream.install_hooks(Arc::new(ReaderIoHooks {
            controller: self.views.core.controller.clone(),
            context: super::controller::HookContext {
                scope: Some(self.views.core.scope.clone()),
                target: Some(target.clone()),
                stream: Some(EntityId::from_bytes(*stream.id().as_bytes())),
                epoch: Some(descriptor.view_epoch()),
                revision: Some(descriptor.revision()),
                read: descriptor.read_epoch(),
                ..Default::default()
            },
        }))?;
        let slot = Arc::new(ReadSlot {
            leases: SelectionLeases::new(),
            stream: stream.clone(),
            samples: Mutex::new((
                SnapshotCursor {
                    epoch: descriptor.view_epoch(),
                    highwater: descriptor.revision(),
                    next: snapshot
                        .projection
                        .records
                        .get(0)
                        .ok()
                        .map_or(RawRecordId(0), |record| record.id),
                    offset: 0,
                    exhausted: false,
                },
                DeltaCursor {
                    epoch: descriptor.view_epoch(),
                    after: descriptor.revision(),
                    next: RawRecordId(0),
                    offset: 0,
                },
            )),
            data: Mutex::new(Some(ReadData {
                snapshot,
                generation,
                position: Position {
                    phase: CursorPhase::Snapshot,
                    index: 0,
                    offset: 0,
                    section: Section::Raw,
                    item: 0,
                    delta: 0,
                    evidence_field: 0,
                    evidence_index: 0,
                    projection_record: 1,
                    after: descriptor.revision().as_u64(),
                    announced: 0,
                    ended: 0,
                    status_delta: 0,
                    status_record: 2,
                    active_segment: None,
                    native_failure: None,
                },
                pending: None,
                last_descriptor: descriptor,
            })),
        });
        let erased: Arc<dyn Any + Send + Sync> = slot.clone();
        let (close, mut closed) = watch::channel(false);
        let handle = Arc::new(ReadHandle {
            binding,
            target: target.clone(),
            stream: stream.clone(),
            descriptor,
            data: Arc::downgrade(&erased),
            close,
            service: Arc::clone(&self.identity),
            intent: Mutex::new(None),
            sampler: slot.clone(),
            source_sealed: self.views.core.probe.sealed_flag(),
        });
        drop(erased);
        let mut stop = self.supervisor.subscribe_stop();
        let core = Arc::clone(&self.views.core);
        self.supervisor
            .spawn_owned(ViewTaskKind::Reader, move |context| async move {
                loop {
                    let changed = core.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    let generation = slot.data.lock().map_or_else(
                        |_| None,
                        |data| data.as_ref().map(|data| Arc::clone(&data.generation)),
                    );
                    let live = generation
                        .is_some_and(|generation| generation.send.current().unwrap_or(false));
                    if !live || *closed.borrow() || *stop.borrow() {
                        break;
                    }
                    tokio::select! {()=changed=>{},_ = closed.changed()=>{},_ = stop.changed()=>{},()=stream.wait_transport_closed()=>{break;}}
                }
                // View-epoch sealing leaves only the accepted small read intent eligible for
                // same-operation continuation. It must not masquerade as explicit Detach.
                if *closed.borrow() || *stop.borrow() {
                    let _ = stream.seal();
                }
                let _ = slot.sample();
                let retired = slot.data.lock().map_err(|_| ViewError::ReadFailure)?.take();
                slot.leases.wait_empty().await;
                context.retire(retired).await?;
                Ok(())
            })?;
        let opened = OpenedView { handle };
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        {
            let erased: Arc<dyn super::probe::StreamInspectionPort> = opened.handle.clone();
            let mut streams = self
                .views
                .core
                .streams
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            streams.retain(|(_, stream)| stream.strong_count() > 0);
            streams
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            streams.push((opened.stream_id(), Arc::downgrade(&erased)));
        }
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.views
            .core
            .controller
            .hit(
                super::controller::ViewHookPoint::AfterSnapshotH,
                self.hook_context(&opened),
            )
            .await?;
        Ok(opened)
    }
    /// # Errors
    /// Reports current authentication, source, scheduling or immutable snapshot failures.
    pub async fn open(
        &self,
        client: &C,
        target: &NativeSessionKey,
        reason: ReadReason,
    ) -> Result<OpenedView, ViewError> {
        let reads = self
            .native_reads
            .as_ref()
            .ok_or(ViewError::CaptureUnavailable)?;
        self.authority
            .with_current(client, target, || Ok(()))
            .await?;
        let request = reads.request(target, reason)?;
        let read = self.open_current(client, target).await?;
        *read
            .handle
            .intent
            .lock()
            .map_err(|_| ViewError::ReadFailure)? = Some(AcceptedIntent {
            request: Arc::new(request),
            stage: RebindStage::Available,
        });
        Ok(read)
    }
    #[must_use]
    pub fn release(&self, read: OpenedView) -> ReadCloseReceipt {
        let already = *read.handle.close.borrow()
            || read.handle.stream.explicitly_sealed().unwrap_or(true)
            || !read.handle.stream.view_current().unwrap_or(false);
        let _ = read.handle.stream.seal();
        let observation = read.handle.stream.observation().ok();
        let started = observation.and_then(|state| {
            (state.highest_started > state.highest_flushed).then_some(state.highest_started)
        });
        let receipt = ReadCloseReceipt {
            stream: read.stream_id(),
            already,
            committed: observation.map_or(0, |state| state.highest_committed),
            started,
            state: if started.is_some() {
                ReadStreamState::ClosingAfterStarted
            } else {
                ReadStreamState::Closed
            },
        };
        drop(read);
        receipt
    }
    pub(crate) async fn wait_original_completion(
        &self,
        client: &C,
        read: &OpenedView,
    ) -> Result<(), ViewError> {
        let mut changes = {
            let intent = read
                .handle
                .intent
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            intent
                .as_ref()
                .ok_or(ViewError::InvalidStage)?
                .request
                .subscribe()
        };
        loop {
            self.authority
                .with_current(client, &read.handle.target, || {
                    if self.views.core.probe.is_sealed() {
                        return Err(ViewError::SourceReleased);
                    }
                    if !Arc::ptr_eq(&self.identity, &read.handle.service)
                        || !read
                            .handle
                            .binding
                            .same_client(&self.authority.client_binding(client)?)
                    {
                        return Err(ViewError::InvalidClient);
                    }
                    if *read.handle.close.borrow() || read.handle.stream.terminally_sealed()? {
                        return Err(ViewError::StreamSealed);
                    }
                    Ok(())
                })
                .await?;
            let status = changes.borrow_and_update().clone();
            match status.read_mode() {
                crate::domain::shared_view::ReadMode::ReplayComplete => return Ok(()),
                crate::domain::shared_view::ReadMode::ReplayIncomplete => {
                    return Err(status.failure().unwrap_or(ViewError::ReadFailure));
                }
                _ => {}
            }
            let source = self.views.core.changed.notified();
            tokio::pin!(source);
            source.as_mut().enable();
            if self.views.core.probe.is_sealed() {
                return Err(ViewError::SourceReleased);
            }
            tokio::select! {
                wake=changes.changed()=>wake.map_err(|_|ViewError::SourceReleased)?,
                ()=source=>{},
                result=self.authority.wait_client_change(client)=>{result?;}
            }
        }
    }
    #[allow(
        clippy::too_many_lines,
        reason = "The first intent claim and final same-nonce source fence must remain visible together; waiting is outside both"
    )]
    pub(crate) async fn rebind_same_read(
        &self,
        client: &C,
        previous: &OpenedView,
    ) -> Result<ReadRebind, ViewError> {
        let binding = self.authority.client_binding(client)?;
        if !Arc::ptr_eq(&self.identity, &previous.handle.service)
            || !previous.handle.binding.same_client(&binding)
        {
            return Err(ViewError::InvalidClient);
        }
        let nonce =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let reads = self
            .native_reads
            .as_ref()
            .ok_or(ViewError::CaptureUnavailable)?;
        let publication = self
            .views
            .core
            .target_publication(&previous.handle.target)?;
        let mut retained = self.retirement(None::<Arc<super::source::TargetPublication<R>>>)?;
        let expected = self
            .authority
            .with_current(client, &previous.handle.target, || {
                if self.views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                if *previous.handle.close.borrow() || previous.handle.stream.terminally_sealed()? {
                    return Err(ViewError::StreamSealed);
                }
                let mut intent = previous
                    .handle
                    .intent
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                let intent = intent.as_mut().ok_or(ViewError::InvalidStage)?;
                if !matches!(intent.stage, RebindStage::Available) {
                    return Err(ViewError::InvalidStage);
                }
                let original = reads.status(&intent.request);
                if original.operation_id() != intent.request.pinned_operation() {
                    return Err(ViewError::InvalidStage);
                }
                let expected_epoch = original.read_epoch().ok_or(ViewError::InvalidStage)?;
                if original.read_mode() != crate::domain::shared_view::ReadMode::ReplayComplete {
                    return Err(ViewError::InvalidStage);
                }
                *retained.value.as_mut().ok_or(ViewError::ReadFailure)? = publication.upgrade();
                let publication = retained
                    .value
                    .as_ref()
                    .and_then(Option::as_ref)
                    .ok_or(ViewError::SourceReleased)?;
                let current = publication
                    .current
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?;
                let descriptor = current
                    .snapshot
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?
                    .descriptor;
                if descriptor.source_stamp() != previous.handle.descriptor.source_stamp()
                    || descriptor.read_epoch() != Some(expected_epoch)
                    || descriptor.read_mode()
                        != crate::domain::shared_view::ReadMode::ReplayComplete
                    || descriptor.view_epoch() == previous.handle.descriptor.view_epoch()
                {
                    return Err(ViewError::StaleCursor);
                }
                intent.stage = RebindStage::Preparing(nonce);
                Ok((
                    intent.request.pinned_operation(),
                    expected_epoch,
                    descriptor.view_epoch(),
                ))
            })
            .await?;
        drop(retained);
        let mut claim = RebindClaim {
            handle: Arc::clone(&previous.handle),
            nonce,
            finished: false,
        };
        let new = self.open_current(client, &previous.handle.target).await?;
        let publication = self
            .views
            .core
            .target_publication(&previous.handle.target)?;
        let mut retained = self.retirement(None::<Arc<super::source::TargetPublication<R>>>)?;
        self.authority
            .with_current(client, &previous.handle.target, || {
                let source_fence = self
                    .views
                    .core
                    .send
                    .inner
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                if self.views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                if source_fence.sealed {
                    return Err(ViewError::SourceReleased);
                }
                if *previous.handle.close.borrow() || previous.handle.stream.terminally_sealed()? {
                    return Err(ViewError::StreamSealed);
                }
                let mut intent = previous
                    .handle
                    .intent
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                let intent = intent.as_mut().ok_or(ViewError::InvalidStage)?;
                if !matches!(intent.stage,RebindStage::Preparing(value) if value==nonce)
                    || intent.request.pinned_operation() != expected.0
                {
                    return Err(ViewError::InvalidStage);
                }
                let original = reads.status(&intent.request);
                *retained.value.as_mut().ok_or(ViewError::ReadFailure)? = publication.upgrade();
                let publication = retained
                    .value
                    .as_ref()
                    .and_then(Option::as_ref)
                    .ok_or(ViewError::SourceReleased)?;
                let current = publication
                    .current
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?;
                if !current.send.current_in(&source_fence) {
                    return Err(ViewError::StaleCursor);
                }
                let descriptor = current
                    .snapshot
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?
                    .descriptor;
                if original.read_epoch() != Some(expected.1)
                    || original.read_mode() != crate::domain::shared_view::ReadMode::ReplayComplete
                    || descriptor.read_epoch() != Some(expected.1)
                    || descriptor.view_epoch() != expected.2
                    || new.handle.descriptor.view_epoch() != expected.2
                    || new.handle.descriptor.source_stamp()
                        != previous.handle.descriptor.source_stamp()
                {
                    return Err(ViewError::StaleCursor);
                }
                intent.stage = RebindStage::Consumed;
                Ok(())
            })
            .await?;
        claim.finished = true;
        let metadata = ReadRebindMetadata {
            previous_stream: previous.stream_id(),
            stream: new.stream_id(),
            epoch: expected.2,
            read: expected.1,
            revision: new.handle.descriptor.revision(),
            source: new.handle.descriptor.source_stamp(),
        };
        Ok(ReadRebind {
            read: new,
            metadata,
        })
    }
    fn synchronize(data: &mut ReadData<R>, committed: u64) {
        synchronize_position(data, committed);
    }
    fn select_status(
        position: Position,
        mut next: Position,
        metadata: super::segments::SegmentMetadata,
        phase: super::segments::SegmentPhase,
    ) -> Result<Selection<R>, ViewError> {
        next.status_record = position
            .status_record
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        next.active_segment = Some(metadata.epoch);
        Ok(Selection::Metadata {
            bytes: super::segments::status(metadata, phase)?,
            position,
            next,
            id: position.status_record,
            kind: RecordKind::Status,
            segment: metadata.epoch,
        })
    }
    #[allow(
        clippy::too_many_lines,
        reason = "Selection keeps fixed-H and tail/raw/projection ordering explicit without mutation or authorization side effects"
    )]
    fn select(
        data: &ReadData<R>,
        current: &TargetSnapshot<R>,
        leases: &Arc<SelectionLeases>,
        native_failure: Option<ViewError>,
    ) -> Result<Selection<R>, ViewError> {
        let original = data.position;
        let mut position = original;
        let journal = &current.projection;
        let tail = position.phase == CursorPhase::Tail;
        if let Some(error) = native_failure
            && position.native_failure != Some(error)
        {
            let mut metadata = *current
                .segments
                .metadata(0, current.descriptor.revision().as_u64())?;
            metadata.descriptor.failure = Some(error);
            metadata.limitation = Some("requested_replay_failed");
            let mut next = position;
            next.native_failure = Some(error);
            let phase = if !tail && position.announced == 0 {
                next.announced = 1;
                super::segments::SegmentPhase::Start
            } else {
                super::segments::SegmentPhase::Update
            };
            return Self::select_status(original, next, metadata, phase);
        }
        if !tail && position.announced < current.segments.publications.len() {
            let metadata = *current
                .segments
                .metadata(position.announced, current.descriptor.revision().as_u64())?;
            let mut next = position;
            next.announced = position
                .announced
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            return Self::select_status(
                original,
                next,
                metadata,
                super::segments::SegmentPhase::Start,
            );
        }
        if position.section == Section::Raw {
            let raw = if position.index < journal.records.len() {
                Some(journal.records.get(position.index)?)
            } else {
                None
            };
            let delta = if tail && position.delta < journal.versions.len() {
                Some(journal.versions.get(position.delta)?)
            } else {
                None
            };
            if tail && position.status_delta < current.segments.events.len() {
                let event = current.segments.events.get(position.status_delta)?;
                if current.segments.publications.get(event.index)?.epoch != event.metadata.epoch {
                    return Err(ViewError::InvalidCursor);
                }
                if raw
                    .as_ref()
                    .is_none_or(|raw| event.metadata.revision <= raw.revision)
                    && delta
                        .as_ref()
                        .is_none_or(|delta| event.metadata.revision <= delta.version.revision())
                {
                    let mut next = position;
                    next.status_delta = position
                        .status_delta
                        .checked_add(1)
                        .ok_or(ViewError::SequenceExhausted)?;
                    return Self::select_status(original, next, event.metadata, event.phase);
                }
            }
            if let Some(raw) = raw
                && (!tail
                    || delta
                        .as_ref()
                        .is_none_or(|delta| raw.revision <= delta.version.revision()))
            {
                if position.active_segment != Some(raw.segment) {
                    let metadata = *current
                        .segments
                        .metadata(raw.segment_index, current.descriptor.revision().as_u64())?;
                    return Self::select_status(
                        original,
                        position,
                        metadata,
                        super::segments::SegmentPhase::Update,
                    );
                }
                return Ok(Selection::Raw {
                    segment: raw.segment,
                    raw: SelectedRaw {
                        raw: raw.raw.clone(),
                        _lease: leases.claim()?,
                    },
                    id: raw.id.as_u64(),
                    total: u64::try_from(raw.raw.len())
                        .map_err(|_| ViewError::LengthUnrepresentable)?,
                    position: original,
                    after_on_finish: (tail
                        && delta
                            .as_ref()
                            .is_none_or(|delta| delta.version.revision() > raw.revision))
                    .then_some(raw.revision),
                    source_range: SourceRange {
                        sequence: raw.sequence,
                        order: raw.receipt_order,
                    },
                });
            }
            if tail && delta.is_none() {
                return Ok(Selection::Wait);
            }
            position.section = Section::Item;
        }
        let (anchor, first, version) = if tail {
            let delta = journal.versions.get(position.delta)?;
            (
                delta.anchor,
                delta.first_position,
                Arc::clone(&delta.version),
            )
        } else {
            if position.item >= journal.anchors.len() {
                if position.ended < current.segments.publications.len() {
                    let metadata = *current
                        .segments
                        .metadata(position.ended, current.descriptor.revision().as_u64())?;
                    let mut next = position;
                    next.ended = position
                        .ended
                        .checked_add(1)
                        .ok_or(ViewError::SequenceExhausted)?;
                    return Self::select_status(
                        original,
                        next,
                        metadata,
                        super::segments::SegmentPhase::End,
                    );
                }
                return Ok(Selection::End {
                    position: original,
                    delta: journal.versions.len(),
                    status_delta: current.segments.events.len(),
                });
            }
            let anchor = journal.anchors.get(position.item)?;
            (
                crate::domain::shared_view::AnchorId(position.item),
                anchor.first_revision,
                journal.item(position.item)?,
            )
        };
        let publication = journal.anchors.get(anchor.as_usize())?;
        if position.active_segment != Some(publication.segment) {
            let metadata = *current.segments.metadata(
                publication.segment_index,
                current.descriptor.revision().as_u64(),
            )?;
            return Self::select_status(
                original,
                position,
                metadata,
                super::segments::SegmentPhase::Update,
            );
        }
        let mut next = position;
        let bytes = if position.section == Section::Item {
            if let Some((field, index)) = super::projection_wire::first_evidence(&version, 0) {
                next.section = Section::Evidence;
                next.evidence_field = field;
                next.evidence_index = index;
            } else {
                Self::advance_item(&mut next)?;
            }
            super::projection_wire::item(anchor, first, &version)?
        } else {
            let field = version
                .fields
                .get(position.evidence_field)
                .ok_or(ViewError::InvalidCursor)?;
            let index = position
                .evidence_index
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            if index < field.evidence_count() {
                next.evidence_index = index;
            } else if let Some((field, index)) =
                super::projection_wire::first_evidence(&version, position.evidence_field + 1)
            {
                next.evidence_field = field;
                next.evidence_index = index;
            } else {
                Self::advance_item(&mut next)?;
            }
            super::projection_wire::evidence(
                anchor,
                &version,
                position.evidence_field,
                position.evidence_index,
            )?
        };
        next.projection_record = position
            .projection_record
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        if tail && next.delta > position.delta {
            next.after = version.revision();
        }
        Ok(Selection::Metadata {
            bytes,
            position: original,
            next,
            id: position.projection_record,
            kind: RecordKind::ItemProjection,
            segment: publication.segment,
        })
    }
    fn advance_item(position: &mut Position) -> Result<(), ViewError> {
        if position.phase == CursorPhase::Tail {
            position.delta = position
                .delta
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            position.section = Section::Raw;
        } else {
            position.item = position
                .item
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)?;
            position.section = Section::Item;
        }
        position.evidence_index = 0;
        position.evidence_field = 0;
        Ok(())
    }
    #[allow(
        clippy::too_many_lines,
        reason = "One linear flow connects qualified immutable selection, one bounded copy and its actual pending commit position"
    )]
    async fn prepare(
        &self,
        client: &C,
        read: &OpenedView,
        snapshot: bool,
    ) -> Result<PrepareNext, ViewError> {
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        self.views
            .core
            .controller
            .hit(
                super::controller::ViewHookPoint::BeforeChunkCopy,
                self.hook_context(read),
            )
            .await?;
        self.authority
            .with_current(client, &read.handle.target, || Ok(()))
            .await?;
        let slot = self.slot(client, read)?;
        let selection = self
            .authority
            .with_current(client, &read.handle.target, || {
                if self.views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                let ready = read.handle.stream.ready()?;
                let native_failure = read.handle.native_failure();
                let committed = read.handle.stream.observation()?.highest_committed;
                let mut data = slot.data.lock().map_err(|_| ViewError::ReadFailure)?;
                let data = data.as_mut().ok_or(ViewError::StaleCursor)?;
                Self::synchronize(data, committed);
                if snapshot && data.position.phase == CursorPhase::Tail {
                    return Ok(Selection::Finished);
                }
                if !snapshot && data.position.phase == CursorPhase::Snapshot {
                    return Err(ViewError::InvalidStage);
                }
                if !ready {
                    return Ok(Selection::Credit);
                }
                let current = if snapshot {
                    Arc::clone(&data.snapshot)
                } else {
                    Arc::clone(
                        &*data
                            .generation
                            .snapshot
                            .read()
                            .map_err(|_| ViewError::ReadFailure)?,
                    )
                };
                Self::select(data, &current, &slot.leases, native_failure)
            })
            .await?;
        let (position, next, record, total, kind, raw, metadata_bytes, source_range, segment) =
            match selection {
                Selection::Wait => return Ok(PrepareNext::AwaitData),
                Selection::Finished => return Ok(PrepareNext::SnapshotFinished),
                Selection::Credit => return Ok(PrepareNext::AwaitCredit),
                Selection::End {
                    position,
                    delta,
                    status_delta,
                } => (
                    position,
                    Position {
                        phase: CursorPhase::Tail,
                        section: Section::Raw,
                        delta,
                        status_delta,
                        ..position
                    },
                    1,
                    0,
                    RecordKind::SnapshotEnd,
                    None,
                    None,
                    None,
                    read.handle.descriptor.view_epoch(),
                ),
                Selection::Metadata {
                    bytes,
                    position,
                    next,
                    id,
                    kind,
                    segment,
                } => {
                    let total =
                        u64::try_from(bytes.len()).map_err(|_| ViewError::LengthUnrepresentable)?;
                    (
                        position,
                        next,
                        id,
                        total,
                        kind,
                        None,
                        Some(bytes),
                        None,
                        segment,
                    )
                }
                Selection::Raw {
                    raw,
                    id,
                    total,
                    position,
                    after_on_finish,
                    source_range,
                    segment,
                } => {
                    let count = raw
                        .len()
                        .checked_sub(position.offset)
                        .ok_or(ViewError::InvalidCursor)?
                        .min(MAX_PAYLOAD_BYTES);
                    let end = position
                        .offset
                        .checked_add(count)
                        .ok_or(ViewError::LengthUnrepresentable)?;
                    let next = if end == raw.len() {
                        Position {
                            index: position
                                .index
                                .checked_add(1)
                                .ok_or(ViewError::SequenceExhausted)?,
                            offset: 0,
                            after: after_on_finish.unwrap_or(position.after),
                            ..position
                        }
                    } else {
                        Position {
                            offset: end,
                            ..position
                        }
                    };
                    (
                        position,
                        next,
                        id,
                        total,
                        RecordKind::OriginalRaw,
                        Some(raw),
                        None,
                        Some(source_range),
                        segment,
                    )
                }
            };
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        if kind == RecordKind::SnapshotEnd {
            self.views
                .core
                .controller
                .hit(
                    super::controller::ViewHookPoint::BeforeSnapshotEnd,
                    self.hook_context(read),
                )
                .await?;
        }
        let chunk = read
            .handle
            .stream
            .prepare_copy(
                self.authority.as_ref(),
                client,
                &read.handle.target,
                record,
                if kind == RecordKind::OriginalRaw {
                    position.offset as u64
                } else {
                    0
                },
                total,
                kind,
                || {
                    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                    self.views.core.controller.fault(
                        super::controller::ViewFalliblePoint::ReadChunkReserve,
                        self.hook_context(read),
                    )?;
                    let data = slot.data.lock().map_err(|_| ViewError::ReadFailure)?;
                    let data = data.as_ref().ok_or(ViewError::StaleCursor)?;
                    if data.position != position || data.pending.is_some() {
                        return Err(ViewError::InvalidStage);
                    }
                    if let Some(bytes) = metadata_bytes {
                        return Ok(bytes);
                    }
                    let Some(raw) = raw else {
                        return Ok(Vec::new());
                    };
                    let remainder = raw
                        .bytes()
                        .get(position.offset..)
                        .ok_or(ViewError::InvalidCursor)?;
                    let count = remainder.len().min(MAX_PAYLOAD_BYTES);
                    let mut payload = Vec::new();
                    payload
                        .try_reserve_exact(count)
                        .map_err(|_| ViewError::AllocationFailure)?;
                    payload.extend_from_slice(&remainder[..count]);
                    Ok(payload)
                },
            )
            .await?;
        let mut data = slot.data.lock().map_err(|_| ViewError::ReadFailure)?;
        let data = data.as_mut().ok_or(ViewError::StaleCursor)?;
        data.pending = Some(PendingPosition {
            packet: chunk.packet_sequence(),
            next,
        });
        let metadata = ChunkMetadata {
            address: RecordAddress::from_header(
                read.handle.descriptor.view_epoch(),
                chunk.header(),
            )?
            .in_segment(segment),
            snapshot: read.handle.descriptor.revision(),
            source: read.handle.descriptor.source_stamp(),
            range: source_range,
        };
        Ok(PrepareNext::Chunk(PreparedViewChunk {
            local: chunk,
            metadata,
        }))
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    fn hook_context(&self, read: &OpenedView) -> super::controller::HookContext<S> {
        super::controller::HookContext {
            scope: Some(self.views.core.scope.clone()),
            target: Some(read.handle.target.clone()),
            stream: Some(read.stream_id()),
            epoch: Some(read.handle.descriptor.view_epoch()),
            revision: Some(read.handle.descriptor.revision()),
            read: read.handle.descriptor.read_epoch(),
            ..Default::default()
        }
    }
    /// # Errors
    /// Requalifies this client and rejects stale/sealed generations or actual copy failure.
    pub async fn next_snapshot(
        &self,
        client: &C,
        read: &OpenedView,
    ) -> Result<PrepareNext, ViewError> {
        self.prepare(client, read, true).await
    }
    /// # Errors
    /// Also rejects a tail request before the actual snapshot-end commit.
    pub async fn next_delta(
        &self,
        client: &C,
        read: &OpenedView,
    ) -> Result<PrepareNext, ViewError> {
        self.prepare(client, read, false).await
    }
    /// # Errors
    /// Returns revocation, source loss or stale generation even while no new output arrives.
    pub async fn wait_change(
        &self,
        client: &C,
        read: &OpenedView,
    ) -> Result<ViewDescriptor, ViewError> {
        let mut native_changes = read
            .handle
            .intent
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .as_ref()
            .map(|intent| intent.request.subscribe());
        loop {
            let changed = self.views.core.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            self.authority
                .with_current(client, &read.handle.target, || Ok(()))
                .await?;
            let slot = self.slot(client, read)?;
            let ready = self
                .authority
                .with_current(client, &read.handle.target, || {
                    if self.views.core.probe.is_sealed() {
                        return Err(ViewError::SourceReleased);
                    }
                    read.handle.stream.ready()?;
                    let native_failure = read.handle.native_failure();
                    let mut data = slot.data.lock().map_err(|_| ViewError::ReadFailure)?;
                    let data = data.as_mut().ok_or(ViewError::StaleCursor)?;
                    let current = data
                        .generation
                        .snapshot
                        .read()
                        .map_err(|_| ViewError::ReadFailure)?;
                    let mut descriptor = current.descriptor;
                    descriptor.failure = descriptor.failure.or(native_failure);
                    let changed = descriptor != data.last_descriptor;
                    data.last_descriptor = descriptor;
                    Ok((descriptor, changed))
                })
                .await?;
            if ready.1 {
                return Ok(ready.0);
            }
            let operation_update = async {
                if let Some(changes) = native_changes.as_mut() {
                    changes
                        .changed()
                        .await
                        .map_err(|_| ViewError::SourceReleased)
                } else {
                    std::future::pending().await
                }
            };
            tokio::select! {()=changed=>{},result=operation_update=>{result?;},result=self.authority.wait_client_change(client)=>{result?;}}
        }
    }
}
