//! Bounded history owner. Implements session-contract §5–6 through storage and output ports.
//!
//! Cache accounting charges each distinct retained (item,version), including in-flight versions,
//! plus 1024 conservative bytes and complete UTF-8 metadata per entry, and 1024 per scope.
//! SQLite's own bounded working buffers are outside this retained-cache counter.
use crate::{
    application::{InputRepository, InputService},
    domain::{
        ActorContext, Admission, EntityId, InputRequest, SessionError,
        history::{
            CommitAck, ContentVersion, FlushReason, HistoryError, HistoryItem, IntegrityChange,
            IntegrityFault, IntegrityReason, ItemDescriptor, ItemId, ItemIdentity, MAX_VERSION,
            MissingRange, OutputScope, Position, PositionReservation, RawContent, SessionId,
            SourceId, StopReceipt, ToolDescriptor,
        },
    },
};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, watch},
    task::{JoinHandle, JoinSet},
};
pub const SESSION_CACHE_LIMIT: usize = 16 * 1024 * 1024;
pub const NODE_CACHE_LIMIT: usize = 128 * 1024 * 1024;
const SAVE_BYTES: usize = 64 * 1024;
const SAVE_MILLIS: u64 = 500;
const RETRY_MILLIS: u64 = 250;
const FAILURE_MILLIS: u64 = 5000;
const SCOPE_OVERHEAD: usize = 1024;
const POSITION_BLOCK: u32 = 64;
// Kept separate from payload accounting: ceil(4 * limit / 5), avoiding floating point.
const fn high(limit: usize) -> usize {
    (limit / 5) * 4 + ((limit % 5) * 4).div_ceil(5)
}

pub trait HistoryRepository: Send + Sync + 'static {
    fn reserve(
        &self,
        scope: OutputScope,
        count: u32,
    ) -> impl Future<Output = Result<PositionReservation, HistoryError>> + Send;
    fn save(
        &self,
        scope: OutputScope,
        items: &[HistoryItem],
        integrity: IntegrityChange,
    ) -> impl Future<Output = Result<Vec<CommitAck>, HistoryError>> + Send;
}
pub trait HistoryClock: Send + Sync + 'static {
    fn millis(&self) -> u64;
}
pub struct MonotonicClock {
    start: Instant,
}
impl Default for MonotonicClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}
impl HistoryClock for MonotonicClock {
    fn millis(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}
/// The owner retains real process/task handles. Requested is not a claim that output has stopped.
pub trait OutputControl: Send + Sync + 'static {
    fn request_stop(&self, scope: OutputScope, fault: IntegrityFault) -> StopReceipt;
    fn finish_stop(
        &self,
        scope: OutputScope,
    ) -> impl Future<Output = Result<(), HistoryError>> + Send;
}
#[derive(Clone, Debug)]
pub struct ItemHandle {
    identity: Arc<ItemIdentity>,
    current: Arc<AtomicU64>,
    committed: Arc<AtomicU64>,
    owner_tag: Arc<()>,
}
impl ItemHandle {
    #[must_use]
    pub fn identity(&self) -> &ItemIdentity {
        &self.identity
    }
    #[must_use]
    pub fn current_version(&self) -> u64 {
        self.current.load(Ordering::Acquire)
    }
    #[must_use]
    pub fn committed_version(&self) -> u64 {
        self.committed.load(Ordering::Acquire)
    }
}
#[derive(Clone)]
struct Entry {
    item: HistoryItem,
    handle: ItemHandle,
}
struct Flight {
    ticket: u64,
    entries: Vec<Entry>,
}
struct Buffer {
    observation_revision: u64,
    checkpoint_owner: Arc<()>,
    positions: PositionReservation,
    pending: HashMap<ItemId, Entry>,
    flight: Option<Flight>,
    first_dirty: Option<u64>,
    dirty_bytes: usize,
    last_attempt: Option<u64>,
    first_failure: Option<u64>,
    failures: u32,
    fault: Option<IntegrityFault>,
    fault_dirty: bool,
    stopped: bool,
    stop_receipt: Option<StopReceipt>,
}
impl Buffer {
    fn retained(&self) -> usize {
        let mut retained = SCOPE_OVERHEAD;
        for entry in self.pending.values() {
            retained = retained.saturating_add(entry.item.retained_bytes());
        }
        if let Some(flight) = &self.flight {
            for entry in &flight.entries {
                if self
                    .pending
                    .get(&entry.item.identity.item)
                    .is_none_or(|current| current.item.version != entry.item.version)
                {
                    retained = retained.saturating_add(entry.item.retained_bytes());
                }
            }
        }
        retained
    }
    fn loss(&self) -> Option<MissingRange> {
        let mut loss = self.fault.and_then(|fault| fault.possible_loss);
        for entry in self
            .pending
            .values()
            .chain(self.flight.iter().flat_map(|flight| flight.entries.iter()))
        {
            include_loss(&mut loss, entry.item.identity.position, 0);
        }
        loss
    }
    fn failure_due(&self, now: u64) -> bool {
        self.failures >= 3
            || self
                .first_failure
                .is_some_and(|first| now.saturating_sub(first) >= FAILURE_MILLIS)
    }
}
struct NodeState {
    buffers: HashMap<OutputScope, Buffer>,
    last_clock: u64,
    next_ticket: u64,
    closed: bool,
}
impl NodeState {
    fn retained(&self) -> usize {
        self.buffers
            .values()
            .fold(0usize, |sum, buffer| sum.saturating_add(buffer.retained()))
    }
    fn session_retained(&self, session: SessionId) -> usize {
        self.buffers
            .iter()
            .filter(|(scope, _)| scope.session == session)
            .fold(0usize, |sum, (_, buffer)| {
                sum.saturating_add(buffer.retained())
            })
    }
    const fn clock(&mut self, now: u64) -> Result<(), HistoryError> {
        if now < self.last_clock {
            return Err(HistoryError::ClockReversed);
        }
        self.last_clock = now;
        Ok(())
    }
}
#[cfg(test)]
struct WorkerScheduleBarrier {
    reached: tokio::sync::Barrier,
    resume: tokio::sync::Barrier,
    scheduled: Mutex<Option<(Duration, usize)>>,
}
#[cfg(test)]
impl WorkerScheduleBarrier {
    fn new() -> Self {
        Self {
            reached: tokio::sync::Barrier::new(2),
            resume: tokio::sync::Barrier::new(2),
            scheduled: Mutex::new(None),
        }
    }
}
struct Core<R, C, O> {
    repository: R,
    clock: C,
    output: O,
    state: Mutex<NodeState>,
    admission: AsyncMutex<()>,
    changed: Notify,
    owner_tag: Arc<()>,
    #[cfg(test)]
    after_schedule: Mutex<Option<Arc<WorkerScheduleBarrier>>>,
}
#[cfg(test)]
async fn worker_schedule_barrier<R: HistoryRepository, C: HistoryClock, O: OutputControl>(
    core: &Core<R, C, O>,
    delay: Duration,
    scopes: usize,
) -> Result<(), HistoryError> {
    let barrier = core
        .after_schedule
        .lock()
        .map_err(|_| HistoryError::OwnerUnavailable)?
        .take();
    if let Some(barrier) = barrier {
        *barrier
            .scheduled
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)? = Some((delay, scopes));
        tokio::time::timeout(Duration::from_secs(10), async {
            barrier.reached.wait().await;
            barrier.resume.wait().await;
        })
        .await
        .map_err(|_| HistoryError::CloseTimeout)?;
    }
    Ok(())
}
pub struct HistoryCoordinator<R: HistoryRepository, C: HistoryClock, O: OutputControl> {
    core: Arc<Core<R, C, O>>,
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> Clone
    for HistoryCoordinator<R, C, O>
{
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaveStatus {
    pub retained_bytes: usize,
    pub node_retained_bytes: usize,
    pub input_allowed: bool,
    pub in_flight: bool,
    pub stopped: bool,
    pub fault: Option<IntegrityFault>,
    pub stop_receipt: Option<StopReceipt>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlushOutcome {
    Clean,
    Committed,
    NotDue,
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> HistoryCoordinator<R, C, O> {
    #[must_use]
    pub fn new(repository: R, clock: C, output: O) -> Self {
        Self {
            core: Arc::new(Core {
                repository,
                clock,
                output,
                state: Mutex::new(NodeState {
                    buffers: HashMap::new(),
                    last_clock: 0,
                    next_ticket: 0,
                    closed: false,
                }),
                admission: AsyncMutex::new(()),
                changed: Notify::new(),
                owner_tag: Arc::new(()),
                #[cfg(test)]
                after_schedule: Mutex::new(None),
            }),
        }
    }
    /// Captures a clean history boundary after a forced flush, outside the delivery fence.
    /// # Errors
    /// Rejects dirty, failed, stopped or closed histories.
    pub fn approval_checkpoint(
        &self,
        scope: OutputScope,
    ) -> Result<super::approvals::HistoryCheckpoint, HistoryError> {
        let state = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?;
        let buffer = state
            .buffers
            .get(&scope)
            .ok_or(HistoryError::InvalidState)?;
        if state.closed
            || buffer.stopped
            || buffer.fault.is_some()
            || !buffer.pending.is_empty()
            || buffer.flight.is_some()
        {
            return Err(HistoryError::InvalidState);
        }
        let revision = buffer.observation_revision;
        let owner = Arc::clone(&buffer.checkpoint_owner);
        drop(state);
        Ok(super::approvals::HistoryCheckpoint {
            scope,
            revision,
            owner,
        })
    }
    /// Rechecks the prepared version while the caller holds its source owner fence.
    #[must_use]
    pub fn approval_checkpoint_current(
        &self,
        checkpoint: &super::approvals::HistoryCheckpoint,
    ) -> bool {
        self.approval_checkpoint(checkpoint.scope)
            .is_ok_and(|current| {
                current.revision == checkpoint.revision
                    && Arc::ptr_eq(&current.owner, &checkpoint.owner)
            })
    }
    /// Attaches one output owner and durably reserves its initial position interval.
    /// # Errors
    /// Rejects closed/stopped owners, exhausted budgets or failed position reservations.
    pub async fn attach(&self, scope: OutputScope) -> Result<(), HistoryError> {
        let _admission = self.core.admission.lock().await;
        {
            let state = self
                .core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?;
            if state.closed {
                return Err(HistoryError::Closed);
            }
            if state.buffers.contains_key(&scope) {
                return Ok(());
            }
            if state.retained().saturating_add(SCOPE_OVERHEAD) > NODE_CACHE_LIMIT
                || state
                    .session_retained(scope.session)
                    .saturating_add(SCOPE_OVERHEAD)
                    > SESSION_CACHE_LIMIT
            {
                return Err(HistoryError::Capacity);
            }
        }
        let positions = self.core.repository.reserve(scope, POSITION_BLOCK).await?;
        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?;
        if state.closed {
            return Err(HistoryError::Closed);
        }
        state.buffers.insert(
            scope,
            Buffer {
                observation_revision: 0,
                checkpoint_owner: Arc::new(()),
                positions,
                pending: HashMap::new(),
                flight: None,
                first_dirty: None,
                dirty_bytes: 0,
                last_attempt: None,
                first_failure: None,
                failures: 0,
                fault: None,
                fault_dirty: false,
                stopped: false,
                stop_receipt: None,
            },
        );
        drop(state);
        Ok(())
    }
    /// Assigns source position before publishing the observation or starting asynchronous decoding.
    /// Distinct visible sources always receive distinct item identities, even with equal bytes.
    /// # Errors
    /// Rejects invalid metadata, stopped owners, exhausted positions and cache overflow.
    pub async fn begin_item(
        &self,
        scope: OutputScope,
        source: SourceId,
        kind: Arc<str>,
        protocol: Arc<str>,
        protocol_version: Arc<str>,
        bytes: Vec<u8>,
    ) -> Result<ItemHandle, HistoryError> {
        self.begin_observation(
            scope,
            ItemDescriptor {
                source,
                kind,
                protocol,
                protocol_version,
                tool_call_id: None,
                tool_origin: None,
            },
            bytes,
        )
        .await
    }
    /// Observes a structured tool with its explicit protocol ID; it never guesses an ID from text.
    /// # Errors
    /// Returns the same source, lifecycle and budget errors as other history observations.
    pub async fn begin_tool(
        &self,
        scope: OutputScope,
        descriptor: ToolDescriptor,
        bytes: Vec<u8>,
    ) -> Result<ItemHandle, HistoryError> {
        self.begin_observation(
            scope,
            ItemDescriptor {
                source: descriptor.source,
                kind: Arc::from("tool"),
                protocol: descriptor.protocol,
                protocol_version: descriptor.protocol_version,
                tool_call_id: Some(descriptor.tool_call_id),
                tool_origin: Some(descriptor.origin),
            },
            bytes,
        )
        .await
    }
    async fn begin_observation(
        &self,
        scope: OutputScope,
        descriptor: ItemDescriptor,
        bytes: Vec<u8>,
    ) -> Result<ItemHandle, HistoryError> {
        let ItemDescriptor {
            source,
            kind,
            protocol,
            protocol_version,
            tool_call_id,
            tool_origin,
        } = descriptor;
        if kind.is_empty()
            || kind.len() > 128
            || protocol.is_empty()
            || protocol.len() > 64
            || protocol_version.is_empty()
            || protocol_version.len() > 64
        {
            return Err(HistoryError::IdentityConflict);
        }
        let _admission = self.core.admission.lock().await;
        let mut position = {
            let mut state = self
                .core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?;
            if state.closed {
                return Err(HistoryError::Closed);
            }
            let buffer = state
                .buffers
                .get_mut(&scope)
                .ok_or(HistoryError::InvalidState)?;
            if buffer.stopped {
                let mut loss = buffer.loss();
                include_loss(
                    &mut loss,
                    Position(buffer.positions.end.saturating_sub(1)),
                    bytes.len(),
                );
                if let Some(mut fault) = buffer.fault {
                    fault.possible_loss = loss;
                    buffer.fault = Some(fault);
                    buffer.fault_dirty = true;
                }
                self.core.changed.notify_waiters();
                return Err(HistoryError::Stopped);
            }
            let position = buffer.positions.take();
            drop(state);
            position
        };
        if position.is_none() {
            let mut positions = self.core.repository.reserve(scope, POSITION_BLOCK).await?;
            position = positions.take();
            self.core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?
                .buffers
                .get_mut(&scope)
                .ok_or(HistoryError::InvalidState)?
                .positions = positions;
        }
        let identity = Arc::new(ItemIdentity {
            scope,
            item: ItemId(new_id()?),
            source,
            position: position.ok_or(HistoryError::CounterExhausted)?,
            kind,
            protocol,
            protocol_version,
            tool_call_id,
            tool_origin,
        });
        let handle = ItemHandle {
            identity,
            current: Arc::new(AtomicU64::new(1)),
            committed: Arc::new(AtomicU64::new(0)),
            owner_tag: Arc::clone(&self.core.owner_tag),
        };
        let result = self.enqueue(&handle, ContentVersion::new(1)?, bytes);
        self.core.changed.notify_waiters();
        result?;
        Ok(handle)
    }
    /// Stores a complete structured snapshot of the same source; it never merges another item.
    /// # Errors
    /// Rejects closed/stopped owners, exhausted versions and cache overflow.
    pub async fn update_item(
        &self,
        handle: &ItemHandle,
        bytes: Vec<u8>,
    ) -> Result<(), HistoryError> {
        let _admission = self.core.admission.lock().await;
        if !Arc::ptr_eq(&handle.owner_tag, &self.core.owner_tag) {
            return Err(HistoryError::InvalidState);
        }
        let previous = handle
            .current
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1).filter(|next| *next <= MAX_VERSION)
            })
            .map_err(|_| HistoryError::CounterExhausted)?;
        let result = self.enqueue(handle, ContentVersion::new(previous + 1)?, bytes);
        self.core.changed.notify_waiters();
        result
    }
    // Keep the shared node/session capacity decision and mutation in one auditable transition.
    #[allow(clippy::too_many_lines)]
    fn enqueue(
        &self,
        handle: &ItemHandle,
        version: ContentVersion,
        bytes: Vec<u8>,
    ) -> Result<(), HistoryError> {
        let scope = handle.identity.scope;
        if bytes.len() > SESSION_CACHE_LIMIT {
            let mut stops = Vec::new();
            {
                let mut state = self
                    .core
                    .state
                    .lock()
                    .map_err(|_| HistoryError::OwnerUnavailable)?;
                if state.closed {
                    return Err(HistoryError::Closed);
                }
                let buffer = state
                    .buffers
                    .get_mut(&scope)
                    .ok_or(HistoryError::InvalidState)?;
                let mut loss = buffer.loss();
                include_loss(&mut loss, handle.identity.position, bytes.len());
                mark_stopped(
                    scope,
                    buffer,
                    IntegrityFault {
                        reason: IntegrityReason::Capacity,
                        possible_loss: loss,
                    },
                    &mut stops,
                );
                drop(state);
            }
            self.request_stops(stops);
            return Err(HistoryError::Capacity);
        }
        let item = HistoryItem {
            identity: Arc::clone(&handle.identity),
            version,
            content: RawContent(Arc::from(bytes)),
        };

        let mut stops = Vec::new();
        let mut error = None;
        {
            let mut state = self
                .core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?;
            let now = self.core.clock.millis();
            state.clock(now)?;
            if state.closed {
                return Err(HistoryError::Closed);
            }
            let buffer = state
                .buffers
                .get(&scope)
                .ok_or(HistoryError::InvalidState)?;
            if buffer.stopped {
                let buffer = state
                    .buffers
                    .get_mut(&scope)
                    .ok_or(HistoryError::InvalidState)?;
                let mut loss = buffer.loss();
                include_loss(&mut loss, item.identity.position, item.content.0.len());
                if let Some(mut fault) = buffer.fault {
                    fault.possible_loss = loss;
                    buffer.fault = Some(fault);
                    buffer.fault_dirty = true;
                }
                return Err(HistoryError::Stopped);
            }
            let removable = buffer
                .pending
                .get(&item.identity.item)
                .filter(|old| {
                    !buffer.flight.as_ref().is_some_and(|flight| {
                        flight.entries.iter().any(|entry| {
                            entry.item.identity.item == old.item.identity.item
                                && entry.item.version == old.item.version
                        })
                    })
                })
                .map_or(0, |old| old.item.retained_bytes());
            let prospective_session = state
                .session_retained(scope.session)
                .saturating_sub(removable)
                .saturating_add(item.retained_bytes());
            let prospective_node = state
                .retained()
                .saturating_sub(removable)
                .saturating_add(item.retained_bytes());
            let buffer = state
                .buffers
                .get_mut(&scope)
                .ok_or(HistoryError::InvalidState)?;
            if prospective_session > SESSION_CACHE_LIMIT
                || prospective_node > NODE_CACHE_LIMIT
                || item.content.0.len() > SESSION_CACHE_LIMIT
            {
                let mut loss = buffer.loss();
                include_loss(&mut loss, item.identity.position, item.content.0.len());
                mark_stopped(
                    scope,
                    buffer,
                    IntegrityFault {
                        reason: IntegrityReason::Capacity,
                        possible_loss: loss,
                    },
                    &mut stops,
                );
                error = Some(HistoryError::Capacity);
            } else {
                if buffer
                    .pending
                    .get(&item.identity.item)
                    .is_none_or(|current| current.item.version < version)
                {
                    buffer.observation_revision = buffer
                        .observation_revision
                        .checked_add(1)
                        .ok_or(HistoryError::CounterExhausted)?;
                    buffer.dirty_bytes = buffer.dirty_bytes.saturating_add(item.content.0.len());
                    buffer.first_dirty.get_or_insert(now);
                    buffer.pending.insert(
                        item.identity.item,
                        Entry {
                            item,
                            handle: handle.clone(),
                        },
                    );
                }
                if prospective_session == SESSION_CACHE_LIMIT {
                    mark_stopped(
                        scope,
                        buffer,
                        IntegrityFault {
                            reason: IntegrityReason::Capacity,
                            possible_loss: buffer.loss(),
                        },
                        &mut stops,
                    );
                }
                if prospective_node == NODE_CACHE_LIMIT {
                    for (scope, buffer) in &mut state.buffers {
                        mark_stopped(
                            *scope,
                            buffer,
                            IntegrityFault {
                                reason: IntegrityReason::Capacity,
                                possible_loss: buffer.loss(),
                            },
                            &mut stops,
                        );
                    }
                }
            }
        }
        self.request_stops(stops);
        error.map_or(Ok(()), Err)
    }
    /// Serializes input admission with retained-cache mutations at the application boundary.
    /// # Errors
    /// Rejects inputs while high-water, stopped or closing, without invoking the input repository.
    pub async fn submit_input<I: InputRepository>(
        &self,
        service: &InputService<I>,
        actor: ActorContext,
        request: InputRequest,
    ) -> Result<Admission, SessionError> {
        let _admission = self.core.admission.lock().await;
        {
            let state = self
                .core
                .state
                .lock()
                .map_err(|_| SessionError::InvalidState)?;
            if state.closed
                || state
                    .buffers
                    .iter()
                    .any(|(scope, buffer)| scope.session.0 == request.session_id && buffer.stopped)
            {
                return Err(SessionError::InvalidState);
            }
            if state.retained() >= high(NODE_CACHE_LIMIT)
                || state.session_retained(SessionId(request.session_id))
                    >= high(SESSION_CACHE_LIMIT)
            {
                return Err(SessionError::Busy);
            }
        }
        service.submit(actor, request).await
    }
    /// # Errors
    /// Returns owner-state errors; this is an internal status projection, not a normal-page badge.
    pub fn status(&self, scope: OutputScope) -> Result<SaveStatus, HistoryError> {
        let state = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?;
        let buffer = state
            .buffers
            .get(&scope)
            .ok_or(HistoryError::InvalidState)?;
        let retained = state.session_retained(scope.session);
        let node = state.retained();
        Ok(SaveStatus {
            retained_bytes: retained,
            node_retained_bytes: node,
            input_allowed: !state.closed
                && !buffer.stopped
                && retained < high(SESSION_CACHE_LIMIT)
                && node < high(NODE_CACHE_LIMIT),
            in_flight: buffer.flight.is_some(),
            stopped: buffer.stopped,
            fault: buffer.fault,
            stop_receipt: buffer.stop_receipt,
        })
    }
    fn request_stops(&self, stops: Vec<(OutputScope, IntegrityFault)>) {
        let changed = !stops.is_empty();
        for (scope, fault) in stops {
            let receipt = self.core.output.request_stop(scope, fault);
            if let Ok(mut state) = self.core.state.lock()
                && let Some(buffer) = state.buffers.get_mut(&scope)
            {
                buffer.stop_receipt = Some(receipt);
            }
        }
        if changed {
            self.core.changed.notify_waiters();
        }
    }
    /// Runs one eligible flush. A forced boundary waits for an existing flight, never overlaps it.
    /// # Errors
    /// Returns actual commit failures or retry deferral; callers must not report release/approval success.
    // The prepare/commit/ack phases deliberately share one cancellation guard.
    #[allow(clippy::too_many_lines)]
    pub async fn flush(
        &self,
        scope: OutputScope,
        reason: FlushReason,
    ) -> Result<FlushOutcome, HistoryError> {
        loop {
            let notified = self.core.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let mut stops = Vec::new();
            let mut wait = false;
            let mut deferred = false;
            let prepared = {
                let mut state = self
                    .core
                    .state
                    .lock()
                    .map_err(|_| HistoryError::OwnerUnavailable)?;
                let now = self.core.clock.millis();
                state.clock(now)?;
                let node_high = state.retained() >= high(NODE_CACHE_LIMIT);
                let session_high =
                    state.session_retained(scope.session) >= high(SESSION_CACHE_LIMIT);
                let buffer = state
                    .buffers
                    .get_mut(&scope)
                    .ok_or(HistoryError::InvalidState)?;
                if buffer.failure_due(now) && !buffer.stopped {
                    mark_stopped(
                        scope,
                        buffer,
                        IntegrityFault {
                            reason: IntegrityReason::PersistentFailure,
                            possible_loss: buffer.loss(),
                        },
                        &mut stops,
                    );
                }
                let decision = if buffer.flight.is_some() {
                    wait = reason.forced();
                    None
                } else if buffer.pending.is_empty() && !buffer.fault_dirty {
                    return Ok(FlushOutcome::Clean);
                } else if buffer.first_failure.is_some()
                    && buffer
                        .last_attempt
                        .is_some_and(|last| now.saturating_sub(last) < RETRY_MILLIS)
                {
                    deferred = reason.forced();
                    None
                } else {
                    let due = reason.forced()
                        || node_high
                        || session_high
                        || buffer.stopped
                        || buffer.first_failure.is_some()
                        || buffer.dirty_bytes >= SAVE_BYTES
                        || buffer
                            .first_dirty
                            .is_some_and(|first| now.saturating_sub(first) >= SAVE_MILLIS);
                    if due {
                        let entries: Vec<Entry> = buffer.pending.values().cloned().collect();
                        let integrity = match buffer.fault {
                            Some(fault) if buffer.stopped => IntegrityChange::Set(fault),
                            _ => IntegrityChange::Keep,
                        };
                        state.next_ticket = state
                            .next_ticket
                            .checked_add(1)
                            .ok_or(HistoryError::CounterExhausted)?;
                        let ticket = state.next_ticket;
                        let buffer = state
                            .buffers
                            .get_mut(&scope)
                            .ok_or(HistoryError::InvalidState)?;
                        buffer.flight = Some(Flight {
                            ticket,
                            entries: entries.clone(),
                        });
                        buffer.last_attempt = Some(now);
                        buffer.dirty_bytes = 0;
                        buffer.first_dirty = None;
                        Some((ticket, entries, integrity))
                    } else {
                        None
                    }
                };
                drop(state);
                decision
            };
            self.request_stops(stops);
            if deferred {
                return Err(HistoryError::RetryDeferred);
            }
            if let Some((ticket, entries, integrity)) = prepared {
                let mut guard = FlightGuard {
                    owner: self.clone(),
                    scope,
                    ticket,
                    items: entries.iter().map(|entry| entry.item.clone()).collect(),
                    armed: true,
                };
                drop(entries);
                let result = self
                    .core
                    .repository
                    .save(scope, &guard.items, integrity)
                    .await
                    .and_then(|acks| {
                        if guard.items.iter().all(|item| {
                            acks.iter().any(|ack| {
                                ack.item == item.identity.item && ack.version >= item.version
                            })
                        }) {
                            Ok(acks)
                        } else {
                            Err(HistoryError::InvalidState)
                        }
                    });
                guard.items.clear();

                let mut stops = Vec::new();
                {
                    let mut state = self
                        .core
                        .state
                        .lock()
                        .map_err(|_| HistoryError::OwnerUnavailable)?;
                    let now = self.core.clock.millis();
                    state.clock(now)?;
                    let buffer = state
                        .buffers
                        .get_mut(&scope)
                        .ok_or(HistoryError::InvalidState)?;
                    let flight = buffer
                        .flight
                        .take()
                        .filter(|flight| flight.ticket == ticket)
                        .ok_or(HistoryError::InvalidState)?;
                    if let Ok(acks) = &result {
                        for entry in &flight.entries {
                            let ack = acks
                                .iter()
                                .find(|ack| ack.item == entry.item.identity.item)
                                .ok_or(HistoryError::InvalidState)?;
                            if ack.version < entry.item.version {
                                return Err(HistoryError::InvalidState);
                            }
                            entry
                                .handle
                                .committed
                                .fetch_max(ack.version.value(), Ordering::AcqRel);
                            if buffer
                                .pending
                                .get(&entry.item.identity.item)
                                .is_some_and(|current| current.item.version <= ack.version)
                            {
                                buffer.pending.remove(&entry.item.identity.item);
                            }
                        }
                        buffer.failures = 0;
                        buffer.first_failure = None;
                        if !buffer.stopped {
                            buffer.fault = None;
                            buffer.fault_dirty = false;
                        } else if matches!(integrity,IntegrityChange::Set(fault) if Some(fault)==buffer.fault)
                        {
                            buffer.fault_dirty = false;
                        }
                    } else {
                        buffer.failures = buffer.failures.saturating_add(1);
                        buffer.first_failure.get_or_insert(now);
                        buffer.first_dirty.get_or_insert(now);
                        buffer.last_attempt = Some(now);
                        if !buffer.stopped {
                            buffer.fault = Some(IntegrityFault {
                                reason: IntegrityReason::SaveFailed,
                                possible_loss: buffer.loss(),
                            });
                        }
                        buffer.fault_dirty = true;
                        if reason.forced() || buffer.failure_due(now) {
                            mark_stopped(
                                scope,
                                buffer,
                                IntegrityFault {
                                    reason: if reason.forced() {
                                        IntegrityReason::SaveFailed
                                    } else {
                                        IntegrityReason::PersistentFailure
                                    },
                                    possible_loss: buffer.loss(),
                                },
                                &mut stops,
                            );
                        }
                    }
                    drop(flight);
                    drop(state);
                }
                guard.armed = false;
                self.request_stops(stops);
                self.core.changed.notify_waiters();
                result?;
                return Ok(FlushOutcome::Committed);
            }
            if wait {
                notified.await;
            } else {
                return Ok(FlushOutcome::NotDue);
            }
        }
    }
    /// Called only after the protocol owner confirms a reliable Run terminal and quiesces its source.
    /// Flushes the boundary and retires clean cache metadata, without releasing Harness resources.
    /// # Errors
    /// Retains scopes with failures, remaining observations or unresolved output responsibility.
    pub async fn finish_run(&self, scope: OutputScope) -> Result<(), HistoryError> {
        self.flush(scope, FlushReason::RunEnd).await?;
        let _admission = self.core.admission.lock().await;
        let mut state = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?;
        let buffer = state
            .buffers
            .get(&scope)
            .ok_or(HistoryError::InvalidState)?;
        if buffer.stopped
            || buffer.fault.is_some()
            || buffer.flight.is_some()
            || !buffer.pending.is_empty()
        {
            return Err(HistoryError::InvalidState);
        }
        state.buffers.remove(&scope);
        drop(state);
        Ok(())
    }
    /// Performs periodic maintenance using the injected monotonic clock.
    /// # Errors
    /// Returns the first actual flush error after attempting every attached scope.
    pub async fn tick(&self) -> Result<(), HistoryError> {
        let scopes: Vec<_> = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?
            .buffers
            .keys()
            .copied()
            .collect();
        let mut first = None;
        for scope in scopes {
            if let Err(error) = self.flush(scope, FlushReason::Periodic).await
                && first.is_none()
            {
                first = Some(error);
            }
        }
        first.map_or(Ok(()), Err)
    }
    /// Checks failure-duration stops independently from any blocked storage future.
    /// # Errors
    /// Returns clock/owner errors; stop receipts never assert a Harness has exited.
    pub fn check_deadlines(&self) -> Result<(), HistoryError> {
        let mut stops = Vec::new();
        {
            let mut state = self
                .core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?;
            let now = self.core.clock.millis();
            state.clock(now)?;
            for (scope, buffer) in &mut state.buffers {
                if buffer.failure_due(now) && !buffer.stopped {
                    mark_stopped(
                        *scope,
                        buffer,
                        IntegrityFault {
                            reason: IntegrityReason::PersistentFailure,
                            possible_loss: buffer.loss(),
                        },
                        &mut stops,
                    );
                }
            }
        }
        self.request_stops(stops);
        Ok(())
    }
    fn schedule(
        &self,
        active: &HashSet<OutputScope>,
    ) -> Result<(Duration, Vec<OutputScope>), HistoryError> {
        let state = self
            .core
            .state
            .lock()
            .map_err(|_| HistoryError::OwnerUnavailable)?;
        let now = self.core.clock.millis();
        let mut deadline = now.saturating_add(60_000);
        let mut due_scopes = Vec::new();
        let node_high = state.retained() >= high(NODE_CACHE_LIMIT);
        for (scope, buffer) in &state.buffers {
            if let Some(first) = buffer.first_failure
                && !buffer.stopped
            {
                deadline = deadline.min(first.saturating_add(FAILURE_MILLIS));
            }
            if buffer.flight.is_some() || active.contains(scope) {
                continue;
            }
            if !buffer.pending.is_empty() || buffer.fault_dirty {
                let mut due = if buffer.first_failure.is_some() {
                    buffer
                        .last_attempt
                        .map_or(now, |last| last.saturating_add(RETRY_MILLIS))
                } else if node_high
                    || state.session_retained(scope.session) >= high(SESSION_CACHE_LIMIT)
                    || buffer.dirty_bytes >= SAVE_BYTES
                    || buffer.stopped
                {
                    now
                } else {
                    buffer
                        .first_dirty
                        .map_or(now, |first| first.saturating_add(SAVE_MILLIS))
                };
                if buffer.first_failure.is_some() {
                    due = due.max(
                        buffer
                            .last_attempt
                            .map_or(now, |last| last.saturating_add(RETRY_MILLIS)),
                    );
                }
                deadline = deadline.min(due);
                if due <= now {
                    due_scopes.push(*scope);
                }
            }
        }
        Ok((
            Duration::from_millis(deadline.saturating_sub(now)),
            due_scopes,
        ))
    }
    fn seal(&self, abort: bool) {
        let mut stops = Vec::new();
        if let Ok(mut state) = self.core.state.lock() {
            state.closed = true;
            if abort {
                for (scope, buffer) in &mut state.buffers {
                    if !buffer.pending.is_empty() || buffer.flight.is_some() {
                        mark_stopped(
                            *scope,
                            buffer,
                            IntegrityFault {
                                reason: IntegrityReason::Cancelled,
                                possible_loss: buffer.loss(),
                            },
                            &mut stops,
                        );
                    }
                }
            }
        }
        self.request_stops(stops);
    }
    /// Seals observations, flushes, then joins output-stop owners within the node close budget.
    /// # Errors
    /// Returns failures without claiming complete closure or releasing unresolved responsibility.
    pub async fn close(&self, deadline: Duration) -> Result<(), HistoryError> {
        let work = async {
            {
                let _admission = self.core.admission.lock().await;
                self.seal(false);
            }
            let scopes: Vec<_> = self
                .core
                .state
                .lock()
                .map_err(|_| HistoryError::OwnerUnavailable)?
                .buffers
                .keys()
                .copied()
                .collect();
            let mut first = None;
            for scope in &scopes {
                if let Err(error) = self.flush(*scope, FlushReason::Shutdown).await
                    && first.is_none()
                {
                    first = Some(error);
                }
            }
            for scope in scopes {
                if self.status(scope)?.stopped
                    && let Err(error) = self.core.output.finish_stop(scope).await
                    && first.is_none()
                {
                    first = Some(error);
                }
            }
            first.map_or(Ok(()), Err)
        };
        tokio::time::timeout(deadline.min(Duration::from_secs(15)), work)
            .await
            .unwrap_or_else(|_| {
                self.seal(true);
                Err(HistoryError::CloseTimeout)
            })
    }
}
struct FlightGuard<R: HistoryRepository, C: HistoryClock, O: OutputControl> {
    owner: HistoryCoordinator<R, C, O>,
    scope: OutputScope,
    ticket: u64,
    items: Vec<HistoryItem>,
    armed: bool,
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> Drop for FlightGuard<R, C, O> {
    fn drop(&mut self) {
        self.items.clear();
        if !self.armed {
            return;
        }
        let mut stops = Vec::new();
        if let Ok(mut state) = self.owner.core.state.lock()
            && let Some(buffer) = state.buffers.get_mut(&self.scope)
            && buffer
                .flight
                .as_ref()
                .is_some_and(|flight| flight.ticket == self.ticket)
        {
            let loss = buffer.loss();
            buffer.flight = None;
            buffer
                .first_dirty
                .get_or_insert_with(|| self.owner.core.clock.millis());
            mark_stopped(
                self.scope,
                buffer,
                IntegrityFault {
                    reason: IntegrityReason::Cancelled,
                    possible_loss: loss,
                },
                &mut stops,
            );
        }
        self.owner.request_stops(stops);
    }
}
fn mark_stopped(
    scope: OutputScope,
    buffer: &mut Buffer,
    mut fault: IntegrityFault,
    stops: &mut Vec<(OutputScope, IntegrityFault)>,
) {
    if buffer.stopped
        && let Some(previous) = buffer.fault
    {
        fault.reason = previous.reason;
    }
    buffer.fault = Some(fault);
    buffer.fault_dirty = true;
    if !buffer.stopped {
        buffer.stopped = true;
        stops.push((scope, fault));
    }
}
fn include_loss(loss: &mut Option<MissingRange>, position: Position, bytes: usize) {
    if let Some(loss) = loss {
        loss.include(position, bytes);
    } else {
        *loss = Some(MissingRange {
            first: position,
            last: position,
            dropped_bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
        });
    }
}
fn new_id() -> Result<EntityId, HistoryError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| HistoryError::RandomUnavailable)
}

/// Owns the sole background scheduling task. Drop cancels it and requests output stop;
/// explicit shutdown additionally joins the task and flushes within the shared 15-second budget.
pub struct HistoryTask<R: HistoryRepository, C: HistoryClock, O: OutputControl> {
    owner: HistoryCoordinator<R, C, O>,
    cancel: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> HistoryTask<R, C, O> {
    #[must_use]
    pub fn start(owner: HistoryCoordinator<R, C, O>) -> Self {
        let (cancel, mut receiver) = watch::channel(false);
        let worker = owner.clone();
        let task = tokio::spawn(async move {
            let mut jobs = JoinSet::new();
            let mut active = HashSet::new();
            loop {
                // Register before inspecting state: notify_waiters does not retain a future permit.
                let changed = worker.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if worker.check_deadlines().is_err() {
                    worker.seal(true);
                    break;
                }
                let Ok((delay, scopes)) = worker.schedule(&active) else {
                    worker.seal(true);
                    break;
                };
                // Tests suspend exactly at the former schedule-to-registration window.
                #[cfg(test)]
                if worker_schedule_barrier(&worker.core, delay, scopes.len())
                    .await
                    .is_err()
                {
                    worker.seal(true);
                    break;
                }
                for scope in scopes {
                    if active.insert(scope) {
                        let owner = worker.clone();
                        jobs.spawn(async move {
                            let result = owner.flush(scope, FlushReason::Periodic).await;
                            (scope, result)
                        });
                    }
                }
                tokio::select! {biased;
                    changed=receiver.changed()=>{if changed.is_err()||*receiver.borrow(){break;}},
                    result=jobs.join_next(),if !jobs.is_empty()=>{if let Some(Ok((scope,_)))=result {active.remove(&scope);}else {worker.seal(true);break;}},
                    ()=&mut changed=>{},
                    ()=tokio::time::sleep(delay)=>{}
                }
            }
            jobs.abort_all();
            while jobs.join_next().await.is_some() {}
        });
        Self {
            owner,
            cancel,
            task: Some(task),
        }
    }
    /// # Errors
    /// Returns task/flush/stop failures without claiming full resource cleanup.
    pub async fn shutdown(mut self) -> Result<(), HistoryError> {
        self.owner.seal(false);
        let start = tokio::time::Instant::now();
        let _ = self.cancel.send(true);
        if let Some(mut task) = self.task.take() {
            match tokio::time::timeout(Duration::from_secs(15), &mut task).await {
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    self.owner.seal(true);
                    return Err(HistoryError::CloseTimeout);
                }
                Ok(Err(_)) => {
                    self.owner.seal(true);
                    return Err(HistoryError::OwnerUnavailable);
                }
                Ok(Ok(())) => {}
            }
        }
        self.owner
            .close(Duration::from_secs(15).saturating_sub(start.elapsed()))
            .await
    }
}
impl<R: HistoryRepository, C: HistoryClock, O: OutputControl> Drop for HistoryTask<R, C, O> {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        if let Some(task) = self.task.take() {
            self.owner.seal(true);
            task.abort();
        }
    }
}
#[cfg(test)]
mod tests;
