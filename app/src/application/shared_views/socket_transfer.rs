#![allow(
    clippy::significant_drop_tightening,
    reason = "The explicit short lock scopes linearize stream seals, actual start_send and same-nonce commits"
)]
#![allow(
    clippy::or_fun_call,
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
//! The original WebSocket writer is the sole constructor of a flush fact.
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
use super::local_transfer::LocalIoHooks;
use super::{
    CurrentViewAuthority,
    local_transfer::{LocalCreditResult, LocalTransferObservation, LocalTransferPhase as Phase},
    send_epoch::OwnedDisplaySendView,
    socket_authority::{LiveSocketContext, SocketViewClient},
    wire::{PacketKind, RecordKind, TransferHeader},
};
use crate::{
    claims::Id128,
    domain::{native::NativeSessionKey, shared_view::ViewError},
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, stream::SplitSink};
use std::{
    future::poll_fn,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::time::{Instant, timeout_at};
fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, ViewError> {
    m.lock().map_err(|_| ViewError::ReadFailure)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StaleRebindState {
    Unsent(u64),
    NeedsCredit,
    Ready(u64),
    Emitted,
}
#[derive(Clone)]
pub struct SocketTransferStream {
    pub(crate) inner: Arc<Stream>,
}
pub struct Stream {
    source: SocketSendSource,
    context: Arc<LiveSocketContext>,
    id: Id128,
    state: Mutex<State>,
    closed: tokio::sync::Notify,
    ready_notify: tokio::sync::Notify,
    document_commit: Mutex<
        Option<std::sync::Weak<dyn crate::application::document_source::DocumentCommitObserver>>,
    >,
    #[cfg(test)]
    flush_polls: FlushPollCounters,
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    hooks: Mutex<Option<Arc<dyn LocalIoHooks>>>,
}
#[cfg(test)]
#[derive(Default)]
struct FlushPollCounters {
    entered: std::sync::atomic::AtomicU64,
    pending: std::sync::atomic::AtomicU64,
    success: std::sync::atomic::AtomicU64,
    error: std::sync::atomic::AtomicU64,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub struct FlushPollObservation {
    pub(crate) entered: u64,
    pub(crate) pending: u64,
    pub(crate) success: u64,
    pub(crate) error: u64,
}
struct State {
    phase: Phase,
    packet: u64,
    started: u64,
    flushed: u64,
    committed: u64,
    cursor: u64,
    pending: u64,
    early: bool,
    sealed: bool,
    queued: bool,
    stale_credit: Option<u64>,
    stale_rebind: Option<StaleRebindState>,
    nonce: Option<Arc<()>>,
}
pub struct PreparedSocketChunk {
    stream: SocketTransferStream,
    bytes: Option<Vec<u8>>,
    header: TransferHeader,
    nonce: Arc<()>,
    finished: bool,
}
pub struct QueuedSocketChunk {
    chunk: PreparedSocketChunk,
}
// No public constructor, Clone, boolean setter or local-delivery conversion exists.
struct ActualSocketFlushFact {
    packet: u64,
    nonce: Arc<()>,
    stream: Id128,
}
// Native and Document share only the actual socket transport and its linear token.
pub type SocketViewStream = SocketTransferStream;
enum SocketSendSource {
    Native {
        view: OwnedDisplaySendView,
        target: NativeSessionKey,
    },
    Document(crate::application::document_source::DocumentSourceLease),
}
enum SourceGuard<'a> {
    Native {
        guard: MutexGuard<'a, super::send_epoch::Group>,
        view: &'a OwnedDisplaySendView,
    },
    Document(crate::application::document_source::DocumentEpochGuard<'a>),
}
impl SourceGuard<'_> {
    fn current(&self) -> bool {
        match self {
            Self::Native { guard, view } => view.current_in(guard),
            Self::Document(guard) => guard.current(),
        }
    }
}
impl SocketSendSource {
    fn lock(&self) -> Result<SourceGuard<'_>, ViewError> {
        match self {
            Self::Native { view, .. } => Ok(SourceGuard::Native {
                guard: lock(&view.group.inner)?,
                view,
            }),
            Self::Document(source) => source.epoch().lock_for_send().map(SourceGuard::Document),
        }
    }
    fn current(&self) -> Result<bool, ViewError> {
        Ok(self.lock()?.current())
    }
    fn native_matches(&self, target: &NativeSessionKey) -> bool {
        matches!(self,Self::Native{target:original,..}if original==target)
    }
    fn retire_document(&self) {
        if let Self::Document(source) = self {
            source.retire();
        }
    }
    async fn guard(
        &self,
        context: &Arc<LiveSocketContext>,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, ViewError> {
        let client = context.client();
        match self {
            Self::Native { target, .. } => context.authority().current_guard(&client, target).await,
            Self::Document(source) => {
                source.require_current().await?;
                let (_, guard) = context.authority().current_actor_guard(&client).await?;
                source.require_current().await?;
                Ok(guard)
            }
        }
    }
}
impl crate::application::document_source::DocumentStreamSeal for Stream {
    fn seal_from_epoch(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.sealed = true;
            state.stale_rebind = None;
            if !matches!(state.phase, Phase::Started | Phase::Flushing) {
                state.phase = Phase::Closed;
            }
        }
        self.source.retire_document();
        self.closed.notify_waiters();
        self.ready_notify.notify_waiters();
    }
}
#[cfg(test)]
#[derive(Clone)]
pub struct SocketTokenIdentity(Arc<()>);
#[cfg(test)]
impl SocketTokenIdentity {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
#[cfg(test)]
pub struct SocketTokenObservation {
    pub(crate) packet: u64,
    pub(crate) identity: SocketTokenIdentity,
    pub(crate) queued: bool,
}
impl Stream {
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        {
            let _g = self.source.lock()?;
            let mut s = lock(&self.state)?;
            s.sealed = true;
            s.stale_rebind = None;
            if !matches!(s.phase, Phase::Started | Phase::Flushing) {
                s.phase = Phase::Closed;
            }
        }
        self.source.retire_document();
        self.closed.notify_waiters();
        self.ready_notify.notify_waiters();
        Ok(())
    }
    fn seal_from_chunk_drop(&self) -> Result<(), ViewError> {
        {
            let _g = self.source.lock()?;
            let mut s = lock(&self.state)?;
            s.sealed = true;
            if !matches!(s.phase, Phase::Started | Phase::Flushing) {
                s.phase = Phase::Closed;
            }
        }
        self.source.retire_document();
        self.closed.notify_waiters();
        self.ready_notify.notify_waiters();
        Ok(())
    }
}
impl SocketTransferStream {
    pub(crate) fn open(
        view: OwnedDisplaySendView,
        context: Arc<LiveSocketContext>,
        target: NativeSessionKey,
    ) -> Result<Self, ViewError> {
        Self::open_source(SocketSendSource::Native { view, target }, context)
    }
    pub(crate) fn open_document(
        source: crate::application::document_source::DocumentSourceLease,
        context: Arc<LiveSocketContext>,
    ) -> Result<Self, ViewError> {
        Self::open_source(SocketSendSource::Document(source), context)
    }
    fn open_source(
        source: SocketSendSource,
        context: Arc<LiveSocketContext>,
    ) -> Result<Self, ViewError> {
        let id = Id128::new(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let stream = Self {
            inner: Arc::new(Stream {
                source,
                context,
                id,
                closed: tokio::sync::Notify::new(),
                ready_notify: tokio::sync::Notify::new(),
                document_commit: Mutex::new(None),
                #[cfg(test)]
                flush_polls: FlushPollCounters::default(),
                state: Mutex::new(State {
                    phase: Phase::AwaitCredit,
                    packet: 0,
                    started: 0,
                    flushed: 0,
                    committed: 0,
                    cursor: 0,
                    pending: 0,
                    early: false,
                    sealed: false,
                    queued: false,
                    stale_credit: None,
                    stale_rebind: None,
                    nonce: None,
                }),
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                hooks: Mutex::new(None),
            }),
        };
        stream.inner.context.lifetime.register(&stream.inner)?;
        if let SocketSendSource::Document(source) = &stream.inner.source {
            let participant: Arc<dyn crate::application::document_source::DocumentStreamSeal> =
                stream.inner.clone();
            source.epoch().register(Arc::downgrade(&participant))?;
        }
        Ok(stream)
    }
    pub(crate) fn ready_notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.inner.ready_notify.notified()
    }
    pub(crate) fn belongs_to(&self, client: &SocketViewClient) -> bool {
        Arc::ptr_eq(&self.inner.context, &client.context)
    }
    pub(crate) async fn wait_closed(&self) {
        loop {
            let closed = self.inner.closed.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            if self.explicitly_sealed().unwrap_or(true) {
                return;
            }
            closed.await;
        }
    }
    pub(crate) async fn wait_terminally_sealed(&self) {
        loop {
            let closed = self.inner.closed.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            if self.terminally_sealed().unwrap_or(true) {
                return;
            }
            closed.await;
        }
    }
    #[cfg(test)]
    pub(crate) fn flush_poll_observation(&self) -> FlushPollObservation {
        use std::sync::atomic::Ordering::SeqCst;
        FlushPollObservation {
            entered: self.inner.flush_polls.entered.load(SeqCst),
            pending: self.inner.flush_polls.pending.load(SeqCst),
            success: self.inner.flush_polls.success.load(SeqCst),
            error: self.inner.flush_polls.error.load(SeqCst),
        }
    }
    pub(crate) fn id(&self) -> Id128 {
        self.inner.id
    }
    pub(crate) fn seal(&self) -> Result<(), ViewError> {
        self.inner.seal()
    }
    pub(crate) fn explicitly_sealed(&self) -> Result<bool, ViewError> {
        Ok(lock(&self.inner.state)?.sealed)
    }
    pub(crate) fn terminally_sealed(&self) -> Result<bool, ViewError> {
        let s = lock(&self.inner.state)?;
        Ok(s.sealed && s.stale_rebind.is_none())
    }
    pub(crate) fn can_emit_stale_rebound(&self) -> Result<bool, ViewError> {
        let s = lock(&self.inner.state)?;
        Ok(!s.sealed
            || matches!(
                s.stale_rebind,
                Some(
                    StaleRebindState::Unsent(_)
                        | StaleRebindState::NeedsCredit
                        | StaleRebindState::Ready(_)
                )
            ))
    }
    pub(crate) fn mark_stale_rebound_emitted(&self) -> Result<(), ViewError> {
        let mut s = lock(&self.inner.state)?;
        if s.sealed && s.stale_rebind.is_none() {
            return Err(ViewError::StreamSealed);
        }
        s.sealed = true;
        s.stale_rebind = Some(StaleRebindState::Emitted);
        Ok(())
    }
    pub(crate) fn view_current(&self) -> Result<bool, ViewError> {
        self.inner.source.current()
    }
    pub(super) fn target_generation_current_in(&self, group: &super::send_epoch::Group) -> bool {
        matches!(&self.inner.source, SocketSendSource::Native { view, .. } if view.target_generation_current_in(group))
    }
    pub(crate) fn next_stale_rebound_packet(&self) -> Result<Option<u64>, ViewError> {
        let s = lock(&self.inner.state)?;
        match s.stale_rebind {
            Some(StaleRebindState::Unsent(packet) | StaleRebindState::Ready(packet)) => {
                return Ok(Some(packet));
            }
            Some(StaleRebindState::NeedsCredit) => {
                if s.stale_credit == Some(s.packet) {
                    return s
                        .packet
                        .checked_add(1)
                        .map(Some)
                        .ok_or(ViewError::SequenceExhausted);
                }
                return Ok(None);
            }
            Some(StaleRebindState::Emitted) => {
                return Err(ViewError::StreamSealed);
            }
            None => {}
        }
        if s.sealed {
            return Err(ViewError::StreamSealed);
        }
        if s.phase == Phase::Ready || s.stale_credit == Some(s.packet) {
            return s
                .packet
                .checked_add(1)
                .map(Some)
                .ok_or(ViewError::SequenceExhausted);
        }
        Ok(None)
    }
    pub(crate) fn observation(&self) -> Result<LocalTransferObservation, ViewError> {
        let s = lock(&self.inner.state)?;
        Ok(LocalTransferObservation {
            phase: s.phase,
            highest_started: s.started,
            highest_flushed: s.flushed,
            actual_flush_successes: s.flushed,
            highest_committed: s.committed,
            pending_early_credit: s.early,
            cursor: s.cursor,
        })
    }
    pub(crate) fn ready(&self) -> Result<bool, ViewError> {
        self.inner.context.lifetime.with_live(|| {
            let g = self.inner.source.lock()?;
            let s = lock(&self.inner.state)?;
            if s.sealed || !g.current() {
                return Err(ViewError::StaleCursor);
            }
            Ok(s.phase == Phase::Ready)
        })
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn install_hooks(&self, hooks: Arc<dyn LocalIoHooks>) -> Result<(), ViewError> {
        let mut h = lock(&self.inner.hooks)?;
        if h.is_some() {
            return Err(ViewError::AlreadyAttached);
        }
        *h = Some(hooks);
        Ok(())
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    async fn pause(
        &self,
        p: super::controller::ViewHookPoint,
        packet: u64,
    ) -> Result<(), ViewError> {
        let h = lock(&self.inner.hooks)?.clone();
        if let Some(h) = h {
            h.pause(p, packet).await?;
        }
        Ok(())
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    fn observe(&self, p: super::controller::ViewHookPoint, packet: u64) -> Result<(), ViewError> {
        let hook = lock(&self.inner.hooks)?.clone();
        if let Some(h) = hook {
            h.observe(p, packet)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn prepare_using<A, C, F>(
        &self,
        authority: &A,
        client: &C,
        target: &NativeSessionKey,
        record: u64,
        offset: u64,
        total: u64,
        kind: RecordKind,
        copy: F,
    ) -> Result<PreparedSocketChunk, ViewError>
    where
        A: CurrentViewAuthority<C>,
        C: Send + Sync,
        F: FnOnce(u64) -> Result<Vec<u8>, ViewError> + Send,
    {
        let binding = authority.client_binding(client)?;
        if !binding.same_client(&self.inner.context.binding())
            || !self.inner.source.native_matches(target)
        {
            return Err(ViewError::InvalidClient);
        }
        authority
            .with_current(client, target, || {
                self.inner.context.lifetime.with_live(|| {
                    let g = self.inner.source.lock()?;
                    let mut s = lock(&self.inner.state)?;
                    if !g.current() || s.sealed {
                        return Err(ViewError::StaleCursor);
                    }
                    if s.phase != Phase::Ready {
                        return Err(ViewError::InvalidCursor);
                    }
                    let packet = s
                        .packet
                        .checked_add(1)
                        .ok_or(ViewError::SequenceExhausted)?;
                    let payload = copy(packet)?;
                    let header = TransferHeader::data(
                        self.id(),
                        packet,
                        record,
                        offset,
                        total,
                        kind,
                        payload.len(),
                    )
                    .map_err(|_| ViewError::InvalidCursor)?;
                    let bytes = header
                        .encode(&payload)
                        .map_err(|_| ViewError::AllocationFailure)?;
                    let nonce = Arc::new(());
                    let pending = offset
                        .checked_add(payload.len() as u64)
                        .ok_or(ViewError::LengthUnrepresentable)?;
                    s.queued = false;
                    s.phase = Phase::Prepared;
                    s.packet = packet;
                    s.pending = pending;
                    s.nonce = Some(nonce.clone());
                    Ok(PreparedSocketChunk {
                        stream: self.clone(),
                        bytes: Some(bytes),
                        header,
                        nonce,
                        finished: false,
                    })
                })
            })
            .await
    }
    pub(crate) fn install_document_commit(
        &self,
        observer: std::sync::Weak<dyn crate::application::document_source::DocumentCommitObserver>,
    ) -> Result<(), ViewError> {
        if !self.is_document() {
            return Err(ViewError::InvalidStage);
        }
        let mut slot = lock(&self.inner.document_commit)?;
        if slot.is_some() {
            return Err(ViewError::AlreadyAttached);
        }
        *slot = Some(observer);
        Ok(())
    }
    fn notify_document_commit(&self) {
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("D:\\Project\\acp-remote\\.tmp\\cp026_server_recv.log")
            {
                let _ = f.write_all(
                    b"notify_document_commit: called
",
                );
            }
        }
        let observer = self
            .inner
            .document_commit
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .and_then(std::sync::Weak::upgrade);
        if let Some(observer) = observer {
            observer.socket_committed();
        }
        self.inner.ready_notify.notify_waiters();
    }
    #[cfg(test)]
    pub(crate) fn token_observation(&self) -> Result<Option<SocketTokenObservation>, ViewError> {
        let state = lock(&self.inner.state)?;
        Ok(state.nonce.as_ref().map(|nonce| SocketTokenObservation {
            packet: state.packet,
            identity: SocketTokenIdentity(nonce.clone()),
            queued: state.queued,
        }))
    }
    pub(crate) fn is_document(&self) -> bool {
        matches!(self.inner.source, SocketSendSource::Document(_))
    }
    pub(crate) async fn prepare_document(
        &self,
        client: &SocketViewClient,
        offset: u64,
        end: bool,
    ) -> Result<PreparedSocketChunk, ViewError> {
        if !self.belongs_to(client) {
            return Err(ViewError::InvalidClient);
        }
        let SocketSendSource::Document(source) = &self.inner.source else {
            return Err(ViewError::InvalidStage);
        };
        let guard = self.inner.source.guard(&self.inner.context).await?;
        let packet = self.inner.context.lifetime.with_live(|| {
            let g = self.inner.source.lock()?;
            let state = lock(&self.inner.state)?;
            if !g.current() || state.sealed {
                return Err(ViewError::StaleCursor);
            }
            if state.phase != Phase::Ready {
                return Err(ViewError::InvalidCursor);
            }
            state
                .packet
                .checked_add(1)
                .ok_or(ViewError::SequenceExhausted)
        })?;
        // No owner/epoch/stream mutex is held while copying/encoding this bounded slice.
        let total = if end { 0 } else { source.total_bytes() };
        let offset = if end { 0 } else { offset };
        let payload = if end {
            Vec::new()
        } else {
            let length = total
                .checked_sub(offset)
                .ok_or(ViewError::InvalidCursor)?
                .min(super::wire::MAX_PAYLOAD_BYTES as u64);
            source
                .copy_range(
                    usize::try_from(offset).map_err(|_| ViewError::LengthUnrepresentable)?,
                    usize::try_from(length).map_err(|_| ViewError::LengthUnrepresentable)?,
                )
                .map_err(|_| ViewError::ReadFailure)?
        };
        let header = TransferHeader::data(
            self.id(),
            packet,
            if end { 2 } else { 1 },
            offset,
            total,
            if end {
                RecordKind::SnapshotEnd
            } else {
                RecordKind::OriginalRaw
            },
            payload.len(),
        )
        .map_err(|_| ViewError::InvalidCursor)?;
        let bytes = header
            .encode(&payload)
            .map_err(|_| ViewError::AllocationFailure)?;
        let result = self.inner.context.lifetime.with_live(|| {
            let g = self.inner.source.lock()?;
            let mut state = lock(&self.inner.state)?;
            if !g.current()
                || state.sealed
                || state.phase != Phase::Ready
                || state.packet.checked_add(1) != Some(packet)
            {
                return Err(ViewError::StaleCursor);
            }
            let nonce = Arc::new(());
            state.queued = false;
            state.phase = Phase::Prepared;
            state.packet = packet;
            state.pending = offset
                .checked_add(payload.len() as u64)
                .ok_or(ViewError::LengthUnrepresentable)?;
            state.nonce = Some(nonce.clone());
            Ok(PreparedSocketChunk {
                stream: self.clone(),
                bytes: Some(bytes),
                header,
                nonce,
                finished: false,
            })
        });
        drop(guard);
        result
    }
    pub(crate) async fn accept_control(
        &self,
        client: &SocketViewClient,
        bytes: &[u8],
    ) -> Result<LocalCreditResult, ViewError> {
        if !Arc::ptr_eq(&client.context, &self.inner.context) {
            return Err(ViewError::InvalidClient);
        }
        let (h, payload) = TransferHeader::decode(bytes).map_err(|_| ViewError::InvalidCursor)?;
        if h.stream() != self.id() || !payload.is_empty() {
            return Err(ViewError::InvalidClient);
        }
        let context = &self.inner.context;
        // These operations grant no read permission and cannot commit a cursor.
        // In particular an early acknowledgement must not wait behind its own
        // writer's actor gate while that writer is flushing this fixed chunk.
        let immediate = context.lifetime.with_live(|| {
            let g = self.inner.source.lock()?;
            let mut s = lock(&self.inner.state)?;
            if h.kind() == PacketKind::Detach {
                s.sealed = true;
                if !matches!(s.stale_rebind, Some(StaleRebindState::Emitted)) {
                    s.stale_rebind = None;
                }
                if !matches!(s.phase, Phase::Started | Phase::Flushing) {
                    s.phase = Phase::Closed;
                }
                self.inner.closed.notify_waiters();
                return Ok(Some(LocalCreditResult::Sealed));
            }
            if s.sealed {
                if matches!(s.stale_rebind, Some(StaleRebindState::NeedsCredit))
                    && h.kind() == PacketKind::Credit
                    && h.packet() == s.packet
                {
                    s.stale_credit = Some(
                        s.stale_credit
                            .map_or(h.packet(), |previous| previous.max(h.packet())),
                    );
                    s.stale_rebind = s.packet.checked_add(1).map(StaleRebindState::Ready);
                }
                return Ok(Some(LocalCreditResult::Sealed));
            }
            if !g.current() {
                if h.kind() == PacketKind::Credit && h.packet() == s.packet {
                    s.stale_credit = Some(
                        s.stale_credit
                            .map_or(h.packet(), |previous| previous.max(h.packet())),
                    );
                }
                return Ok(Some(LocalCreditResult::Sealed));
            }
            if h.kind() != PacketKind::Credit {
                return Err(ViewError::InvalidCursor);
            }
            if matches!(s.phase, Phase::Started | Phase::Flushing) {
                if h.packet() <= s.committed {
                    return Ok(Some(LocalCreditResult::Duplicate));
                }
                if h.packet() != s.packet {
                    return Err(ViewError::InvalidCursor);
                }
                s.early = true;
                return Ok(Some(LocalCreditResult::HeldEarly));
            }
            if s.phase != Phase::Sent && !(s.phase == Phase::AwaitCredit && h.packet() == 0) {
                if h.packet() <= s.committed {
                    return Ok(Some(LocalCreditResult::Duplicate));
                }
                return Err(ViewError::InvalidCursor);
            }
            Ok(None)
        })?;
        if let Some(receipt) = immediate {
            if matches!(receipt, LocalCreditResult::Sealed) {
                self.inner.source.retire_document();
                self.inner.closed.notify_waiters();
                self.inner.ready_notify.notify_waiters();
            }
            return Ok(receipt);
        }
        let _gate = self.inner.source.guard(context).await?;
        let result = context.lifetime.with_live(|| {
            let g = self.inner.source.lock()?;
            let mut s = lock(&self.inner.state)?;
            if h.kind() == PacketKind::Detach {
                s.sealed = true;
                if !matches!(s.stale_rebind, Some(StaleRebindState::Emitted)) {
                    s.stale_rebind = None;
                }
                s.phase = Phase::Closed;
                self.inner.closed.notify_waiters();
                return Ok(LocalCreditResult::Sealed);
            }
            if s.sealed || !g.current() {
                if !s.sealed
                    && !g.current()
                    && h.kind() == PacketKind::Credit
                    && h.packet() == s.packet
                {
                    s.stale_credit = Some(
                        s.stale_credit
                            .map_or(h.packet(), |previous| previous.max(h.packet())),
                    );
                }
                if s.sealed
                    && matches!(s.stale_rebind, Some(StaleRebindState::NeedsCredit))
                    && h.kind() == PacketKind::Credit
                    && h.packet() == s.packet
                {
                    s.stale_credit = Some(
                        s.stale_credit
                            .map_or(h.packet(), |previous| previous.max(h.packet())),
                    );
                    s.stale_rebind = s.packet.checked_add(1).map(StaleRebindState::Ready);
                }
                return Ok(LocalCreditResult::Sealed);
            }
            if h.kind() != PacketKind::Credit {
                return Err(ViewError::InvalidCursor);
            }
            let ack = h.packet();
            if s.phase == Phase::AwaitCredit && ack == 0 {
                s.phase = Phase::Ready;
                return Ok(LocalCreditResult::Initial);
            }
            if ack <= s.committed {
                return Ok(LocalCreditResult::Duplicate);
            }
            if ack != s.packet {
                return Err(ViewError::InvalidCursor);
            }
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            {
                use std::io::Write;
                let line = format!(
                    "accept_control_phase: ack={}, phase={:?}, committed={}, packet={}
",
                    ack, s.phase, s.committed, s.packet
                );
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("D:\\Project\\acp-remote\\.tmp\\cp026_server_recv.log")
                {
                    let _ = f.write_all(line.as_bytes());
                }
            }
            match s.phase {
                Phase::Started | Phase::Flushing => {
                    s.early = true;
                    Ok(LocalCreditResult::HeldEarly)
                }
                Phase::Sent => {
                    s.committed = ack;
                    s.cursor = s.pending;
                    s.phase = Phase::Ready;
                    Ok(LocalCreditResult::Committed)
                }
                _ => Err(ViewError::InvalidCursor),
            }
        });
        drop(_gate);
        match &result {
            Ok(LocalCreditResult::Committed) => {
                self.notify_document_commit();
            }
            Ok(LocalCreditResult::Initial) => {
                self.inner.ready_notify.notify_waiters();
            }
            Ok(LocalCreditResult::Sealed) => {
                self.inner.source.retire_document();
                self.inner.closed.notify_waiters();
                self.inner.ready_notify.notify_waiters();
            }
            _ => {}
        }
        result
    }
}
impl PreparedSocketChunk {
    pub(crate) fn is_document(&self) -> bool {
        self.stream.is_document()
    }

    pub(crate) const fn header(&self) -> TransferHeader {
        self.header
    }
    pub(crate) fn byte_len(&self) -> usize {
        self.bytes.as_ref().map_or(0, Vec::len)
    }
    pub(crate) fn belongs_to(&self, client: &SocketViewClient) -> bool {
        Arc::ptr_eq(&self.stream.inner.context, &client.context)
            && self.stream.inner.context.connection_id() == client.context.connection_id()
    }
    pub(crate) fn queue(self) -> Result<QueuedSocketChunk, Self> {
        let valid = (|| {
            let mut s = lock(&self.stream.inner.state)?;
            if s.phase != Phase::Prepared || s.queued || s.sealed {
                return Err(ViewError::StaleCursor);
            }
            s.queued = true;
            Ok(())
        })();
        if valid.is_err() {
            return Err(self);
        }
        Ok(QueuedSocketChunk { chunk: self })
    }
}
impl Drop for PreparedSocketChunk {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.stream.inner.seal_from_chunk_drop();
        }
    }
}
impl QueuedSocketChunk {
    pub(crate) fn belongs_to(&self, client: &SocketViewClient) -> bool {
        self.chunk.belongs_to(client)
    }
    pub(crate) fn rejected(self) -> PreparedSocketChunk {
        if let Ok(mut s) = self.chunk.stream.inner.state.lock() {
            s.queued = false;
        }
        self.chunk
    }
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the original linear token, genuine sink operations and private flush fact in one auditable owner method"
    )]
    pub(crate) async fn send(
        mut self,
        sink: &mut SplitSink<WebSocket, Message>,
        send_timeout: std::time::Duration,
    ) -> Result<(), ViewError> {
        let deadline = Instant::now() + send_timeout;
        let stream = self.chunk.stream.clone();
        let packet = self.chunk.header.packet();
        let context = stream.inner.context.clone();
        let _client = context.client();
        let sending = async {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream
                .pause(
                    super::controller::ViewHookPoint::AfterChunkQueuedBeforeWriterGate,
                    packet,
                )
                .await?;
            let guard = stream.inner.source.guard(&context).await?;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream.observe(super::controller::ViewHookPoint::WriterGateHeld, packet)?;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream
                .pause(super::controller::ViewHookPoint::BeforeSinkReady, packet)
                .await?;

            poll_fn(|cx| Pin::new(&mut *sink).poll_ready(cx))
                .await
                .map_err(|_| ViewError::ReadFailure)?;

            context.lifetime.with_live(|| {
                let g = stream.inner.source.lock()?;
                let mut s = lock(&stream.inner.state)?;
                if !g.current()
                    || s.sealed
                    || s.phase != Phase::Prepared
                    || !s.queued
                    || s.packet != packet
                    || !s
                        .nonce
                        .as_ref()
                        .is_some_and(|n| Arc::ptr_eq(n, &self.chunk.nonce))
                {
                    if !g.current()
                        && !s.sealed
                        && s.phase == Phase::Prepared
                        && s.queued
                        && s.packet == packet
                        && s.nonce
                            .as_ref()
                            .is_some_and(|n| Arc::ptr_eq(n, &self.chunk.nonce))
                    {
                        s.stale_rebind = Some(StaleRebindState::Unsent(packet));
                    }
                    return Err(ViewError::StaleCursor);
                }
                let bytes = self.chunk.bytes.take().ok_or(ViewError::InvalidStage)?;
                Pin::new(&mut *sink)
                    .start_send(Message::Binary(bytes.into()))
                    .map_err(|_| ViewError::ReadFailure)?;
                s.phase = Phase::Started;
                s.started = packet;
                Ok(())
            })?;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream.observe(
                super::controller::ViewHookPoint::AfterStartSendBeforeFlush,
                packet,
            )?;
            {
                let mut s = lock(&stream.inner.state)?;
                s.phase = Phase::Flushing;
            }
            let flushed = poll_fn(|cx| {
                #[cfg(test)]
                stream
                    .inner
                    .flush_polls
                    .entered
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let actual = Pin::new(&mut *sink).poll_flush(cx);
                #[cfg(test)]
                {
                    use std::{sync::atomic::Ordering::SeqCst, task::Poll};
                    let counter = match &actual {
                        Poll::Pending => &stream.inner.flush_polls.pending,
                        Poll::Ready(Ok(())) => &stream.inner.flush_polls.success,
                        Poll::Ready(Err(_)) => &stream.inner.flush_polls.error,
                    };
                    counter.fetch_add(1, SeqCst);
                }
                actual
            })
            .await;
            // The same TrustStore guard covers ready, start_send and the actual
            // flush outcome. timeout_at also drops this guard on the original
            // fixed deadline; source/stream seals never acquire this gate.
            drop(guard);
            flushed.map_err(|_| ViewError::ReadFailure)?;
            let fact = ActualSocketFlushFact {
                packet,
                nonce: self.chunk.nonce.clone(),
                stream: stream.id(),
            };
            // Actual flush is retained even if authority disappears before publication.
            {
                let mut s = lock(&stream.inner.state)?;
                s.flushed = packet;
            }
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream
                .pause(
                    super::controller::ViewHookPoint::AfterFlushFactBeforePublish,
                    packet,
                )
                .await?;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            stream
                .pause(super::controller::ViewHookPoint::BeforeCommitCursor, packet)
                .await?;
            let current = stream.inner.source.guard(&context).await;
            let publication = context.lifetime.with_live(|| {
                let g = stream.inner.source.lock()?;
                let mut s = lock(&stream.inner.state)?;
                if fact.stream != stream.id()
                    || fact.packet != s.packet
                    || !s
                        .nonce
                        .as_ref()
                        .is_some_and(|n| Arc::ptr_eq(n, &fact.nonce))
                {
                    return Err(ViewError::InvalidCursor);
                }
                if current.is_err() || s.sealed {
                    s.stale_rebind = None;
                    s.sealed = true;
                    s.phase = Phase::Closed;
                } else if !g.current() {
                    s.stale_rebind = if s.early || s.stale_credit == Some(packet) {
                        packet.checked_add(1).map(StaleRebindState::Ready)
                    } else {
                        Some(StaleRebindState::NeedsCredit)
                    };
                    s.sealed = true;
                    s.phase = Phase::Closed;
                } else if s.early {
                    s.committed = packet;
                    s.cursor = s.pending;
                    s.phase = Phase::Ready;
                } else {
                    s.phase = Phase::Sent;
                }
                s.early = false;
                Ok(())
            });
            drop(current);
            if publication.is_err() {
                // Publication failure seals the stream and clears any stale rebind hint.
                stream.seal()?;
            } else if stream.explicitly_sealed().unwrap_or(true) {
                // Publication can close the stream while retaining the real flush fact.
                // Complete the original seal side effects even when the closure returned Ok.
                stream.inner.seal_from_chunk_drop()?;
            }
            stream.notify_document_commit();
            self.chunk.finished = true;
            Ok(())
        };
        timeout_at(deadline, sending)
            .await
            .unwrap_or(Err(ViewError::ReadFailure))
    }
}
