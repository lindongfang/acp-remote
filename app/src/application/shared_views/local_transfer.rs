#![allow(
    clippy::significant_drop_tightening,
    reason = "Explicit short fence scopes linearize seals, first actual IO and cursor commits; early guard drops would open races"
)]
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
//! Actual duplex chunk transport for owned component data. It never produces a WebSocket fact.
//!
//! A production Native bridge must connect display-group/view seals to its genuine lifecycle.
//! This owned component is not a native source slot, a loaded flag, or a globally reachable read handle.
use super::send_epoch::Group;
pub use super::send_epoch::{OwnedDisplaySendGroup, OwnedDisplaySendView};
use super::{
    CurrentViewAuthority, ReadClientBinding,
    local_authority::{AuthenticatedLocalViewClient, LocalViewAuthority},
    wire::{HEADER_BYTES, PacketKind, RecordKind, TransferHeader},
};
use crate::{
    claims::Id128,
    domain::{native::NativeSessionKey, shared_view::ViewError},
};
use std::{
    future::poll_fn,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::Poll,
};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout_at},
};
#[derive(Clone)]
pub struct LocalViewStream {
    inner: Arc<Stream>,
}
struct Stream {
    view: OwnedDisplaySendView,
    binding: ReadClientBinding,
    id: Id128,
    state: Mutex<StreamState>,
    #[cfg(test)]
    changed: tokio::sync::Notify,
    #[cfg(test)]
    after_flush: Mutex<Option<TestFlushPause>>,
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    hooks: Mutex<Option<Arc<dyn LocalIoHooks>>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalTransferPhase {
    AwaitCredit,
    Ready,
    Prepared,
    Started,
    Flushing,
    Sent,
    Closed,
}
struct StreamState {
    phase: LocalTransferPhase,
    packet: u64,
    started: u64,
    flushed: u64,
    actual_flushes: u64,
    committed: u64,
    early: bool,
    sealed: bool,
    cursor: u64,
    pending_cursor: u64,
    bound: bool,
}
pub struct PreparedLocalChunk {
    stream: Arc<Stream>,
    bytes: Vec<u8>,
    packet: u64,
    next_cursor: u64,
    handed_off: bool,
    header: TransferHeader,
}
pub struct LocalChunkWriter {
    stream: Arc<Stream>,
    writer: WriteHalf<DuplexStream>,
}
pub struct LocalCreditReader {
    stream: Arc<Stream>,
    reader: ReadHalf<DuplexStream>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalCreditResult {
    Initial,
    Duplicate,
    HeldEarly,
    Committed,
    Sealed,
}
/// Parsed wire data, not a permission or a flush/commit constructor.
#[derive(Clone, Copy)]
pub(crate) struct LocalCreditRead {
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    packet: u64,
    result: LocalCreditResult,
}
impl LocalCreditRead {
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) const fn packet(self) -> u64 {
        self.packet
    }
    pub(crate) const fn result(self) -> LocalCreditResult {
        self.result
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalTransferObservation {
    pub phase: LocalTransferPhase,
    pub highest_started: u64,
    pub highest_flushed: u64,
    pub actual_flush_successes: u64,
    pub highest_committed: u64,
    pub pending_early_credit: bool,
    pub cursor: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalFlushObservation {
    pub packet: u64,
    pub committed: bool,
    pub sealed_after_start: bool,
}
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
pub(crate) trait LocalIoHooks: Send + Sync {
    fn observe(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> Result<(), ViewError>;
    fn pause(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> super::BoxReadFuture<'_, Result<(), ViewError>>;
}
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
impl Stream {
    fn hook_observe(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> Result<(), ViewError> {
        let hooks = lock(&self.hooks)?.clone();
        if let Some(hooks) = hooks {
            hooks.observe(point, packet)?;
        }
        Ok(())
    }
    async fn hook_pause(
        &self,
        point: super::controller::ViewHookPoint,
        packet: u64,
    ) -> Result<(), ViewError> {
        let hooks = lock(&self.hooks)?.clone();
        if let Some(hooks) = hooks {
            hooks.pause(point, packet).await?;
        }
        Ok(())
    }
}

fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>, ViewError> {
    value.lock().map_err(|_| ViewError::ReadFailure)
}
impl OwnedDisplaySendView {
    /// # Errors
    /// Reports random identity failure or an already sealed display generation.
    pub fn open(&self, binding: ReadClientBinding) -> Result<LocalViewStream, ViewError> {
        let group = lock(&self.group.inner)?;
        if !self.current_in(&group) {
            return Err(ViewError::StaleCursor);
        }
        let id = Id128::new(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        Ok(LocalViewStream {
            inner: Arc::new(Stream {
                view: self.clone(),
                binding,
                id,
                state: Mutex::new(StreamState {
                    phase: LocalTransferPhase::AwaitCredit,
                    packet: 0,
                    started: 0,
                    flushed: 0,
                    actual_flushes: 0,
                    committed: 0,
                    early: false,
                    sealed: false,
                    cursor: 0,
                    pending_cursor: 0,
                    bound: false,
                }),
                #[cfg(test)]
                changed: tokio::sync::Notify::new(),
                #[cfg(test)]
                after_flush: Mutex::new(None),
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                hooks: Mutex::new(None),
            }),
        })
    }
}
impl Stream {
    fn live(&self, group: &Group, state: &StreamState) -> bool {
        self.view.current_in(group) && !state.sealed
    }
    fn credit(
        &self,
        bytes: &[u8],
        current_commit: bool,
    ) -> Result<Option<LocalCreditResult>, ViewError> {
        let (h, payload) = TransferHeader::decode(bytes).map_err(|_| ViewError::InvalidCursor)?;
        if h.kind() != PacketKind::Credit || h.stream() != self.id || !payload.is_empty() {
            return Err(ViewError::InvalidClient);
        }
        let group = lock(&self.view.group.inner)?;
        let mut state = lock(&self.state)?;
        if !self.live(&group, &state) {
            return Ok(Some(LocalCreditResult::Sealed));
        }
        let ack = h.packet();
        if state.phase == LocalTransferPhase::AwaitCredit && ack == 0 {
            if !current_commit {
                return Ok(None);
            }
            state.phase = LocalTransferPhase::Ready;
            return Ok(Some(LocalCreditResult::Initial));
        }
        if ack <= state.committed {
            return Ok(Some(LocalCreditResult::Duplicate));
        }
        if ack != state.packet {
            return Err(ViewError::InvalidCursor);
        }
        match state.phase {
            LocalTransferPhase::Started | LocalTransferPhase::Flushing => {
                state.early = true;
                Ok(Some(LocalCreditResult::HeldEarly))
            }
            LocalTransferPhase::Sent => {
                if !current_commit {
                    return Ok(None);
                }
                state.committed = ack;
                state.cursor = state.pending_cursor;
                state.phase = LocalTransferPhase::Ready;
                Ok(Some(LocalCreditResult::Committed))
            }
            _ => Err(ViewError::InvalidCursor),
        }
    }
}
impl LocalViewStream {
    pub(crate) fn view_current(&self) -> Result<bool, ViewError> {
        self.inner.view.current()
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn install_hooks(&self, hooks: Arc<dyn LocalIoHooks>) -> Result<(), ViewError> {
        let mut current = lock(&self.inner.hooks)?;
        if current.is_some() {
            return Err(ViewError::AlreadyAttached);
        }
        *current = Some(hooks);
        Ok(())
    }
    pub(crate) fn explicitly_sealed(&self) -> Result<bool, ViewError> {
        Ok(lock(&self.inner.state)?.sealed)
    }
    pub(crate) fn ready(&self) -> Result<bool, ViewError> {
        let group = lock(&self.inner.view.group.inner)?;
        let state = lock(&self.inner.state)?;
        if !self.inner.live(&group, &state) {
            return Err(ViewError::StaleCursor);
        }
        Ok(state.phase == LocalTransferPhase::Ready)
    }
    #[must_use]
    pub fn id(&self) -> Id128 {
        self.inner.id
    }
    /// # Errors
    /// Rejects an unavailable stream state.
    pub fn observation(&self) -> Result<LocalTransferObservation, ViewError> {
        let s = lock(&self.inner.state)?;
        Ok(LocalTransferObservation {
            phase: s.phase,
            highest_started: s.started,
            highest_flushed: s.flushed,
            actual_flush_successes: s.actual_flushes,
            highest_committed: s.committed,
            pending_early_credit: s.early,
            cursor: s.cursor,
        })
    }
    /// # Errors
    /// Rejects an unavailable stream state.
    pub fn seal(&self) -> Result<(), ViewError> {
        let _group = lock(&self.inner.view.group.inner)?;
        let mut s = lock(&self.inner.state)?;
        s.sealed = true;
        if s.phase != LocalTransferPhase::Started && s.phase != LocalTransferPhase::Flushing {
            s.phase = LocalTransferPhase::Closed;
        }
        Ok(())
    }
    /// A local test endpoint has its own authenticated binding, never a WebSocket identity.
    /// # Errors
    /// Rejects a wrong client, an already installed endpoint, or a sealed stream.
    pub fn bind_duplex(
        &self,
        stream: DuplexStream,
        binding: &ReadClientBinding,
    ) -> Result<(LocalChunkWriter, LocalCreditReader), ViewError> {
        if !self.inner.binding.same_client(binding) {
            return Err(ViewError::InvalidClient);
        }
        let group = lock(&self.inner.view.group.inner)?;
        let mut state = lock(&self.inner.state)?;
        if !self.inner.live(&group, &state) || state.bound {
            return Err(ViewError::InvalidClient);
        }
        state.bound = true;
        let (reader, writer) = tokio::io::split(stream);
        Ok((
            LocalChunkWriter {
                stream: Arc::clone(&self.inner),
                writer,
            },
            LocalCreditReader {
                stream: Arc::clone(&self.inner),
                reader,
            },
        ))
    }
    /// The copy closure runs under current qualification, not a retained per-body grant.
    /// # Errors
    /// Rejects wrong clients, missing credit, sealed generations and actual copy/encoding failure.
    #[allow(
        clippy::too_many_arguments,
        reason = "Owned component API keeps actual authorization and the fixed record address explicit at its copy boundary"
    )]
    pub async fn prepare_copy<A, C, F>(
        &self,
        authority: &A,
        client: &C,
        target: &NativeSessionKey,
        record: u64,
        offset: u64,
        total: u64,
        kind: RecordKind,
        copy: F,
    ) -> Result<PreparedLocalChunk, ViewError>
    where
        A: CurrentViewAuthority<C>,
        C: Send + Sync,
        F: FnOnce() -> Result<Vec<u8>, ViewError> + Send,
    {
        self.prepare_using(
            authority,
            client,
            target,
            record,
            offset,
            total,
            kind,
            |_| copy(),
        )
        .await
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keeps actual client/target and exact record address explicit at the one qualified copy boundary"
    )]
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
    ) -> Result<PreparedLocalChunk, ViewError>
    where
        A: CurrentViewAuthority<C>,
        C: Send + Sync,
        F: FnOnce(u64) -> Result<Vec<u8>, ViewError> + Send,
    {
        let binding = authority.client_binding(client)?;
        if !self.inner.binding.same_client(&binding) {
            return Err(ViewError::InvalidClient);
        }
        authority
            .with_current(client, target, || {
                let group = lock(&self.inner.view.group.inner)?;
                let mut state = lock(&self.inner.state)?;
                if !self.inner.live(&group, &state) {
                    return Err(ViewError::StaleCursor);
                }
                if state.phase != LocalTransferPhase::Ready {
                    return Err(ViewError::InvalidCursor);
                }
                let packet = state
                    .packet
                    .checked_add(1)
                    .ok_or(ViewError::SequenceExhausted)?;
                let payload = copy(packet)?;
                let header = TransferHeader::data(
                    self.inner.id,
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
                let next_cursor = offset
                    .checked_add(payload.len() as u64)
                    .ok_or(ViewError::LengthUnrepresentable)?;
                state.phase = LocalTransferPhase::Prepared;
                state.packet = packet;
                state.pending_cursor = next_cursor;
                Ok(PreparedLocalChunk {
                    stream: Arc::clone(&self.inner),
                    bytes,
                    packet,
                    next_cursor,
                    handed_off: false,
                    header,
                })
            })
            .await
    }
}
impl PreparedLocalChunk {
    pub(crate) const fn header(&self) -> TransferHeader {
        self.header
    }
    pub(crate) const fn packet_sequence(&self) -> u64 {
        self.packet
    }
}
impl Drop for PreparedLocalChunk {
    fn drop(&mut self) {
        if !self.handed_off {
            let handle = LocalViewStream {
                inner: Arc::clone(&self.stream),
            };
            let _ = handle.seal();
        }
    }
}
impl LocalCreditReader {
    /// # Errors
    /// Rejects invalid actual wire, stale qualification, IO failure or the total read deadline.
    pub async fn read_credit(
        &mut self,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
        deadline: Instant,
    ) -> Result<LocalCreditResult, ViewError> {
        self.read_credit_until(authority, client, target, Some(deadline))
            .await
            .map(LocalCreditRead::result)
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) async fn read_credit_unbounded(
        &mut self,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
    ) -> Result<LocalCreditRead, ViewError> {
        self.read_credit_until(authority, client, target, None)
            .await
    }
    async fn read_credit_until(
        &mut self,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
        deadline: Option<Instant>,
    ) -> Result<LocalCreditRead, ViewError> {
        if !self
            .stream
            .binding
            .same_client(&authority.client_binding(client)?)
        {
            return Err(ViewError::InvalidClient);
        }
        let mut bytes = [0; HEADER_BYTES];
        let reading = self.reader.read_exact(&mut bytes);
        match deadline {
            Some(deadline) => {
                timeout_at(deadline, reading)
                    .await
                    .map_err(|_| ViewError::ReadFailure)?
                    .map_err(|_| ViewError::ReadFailure)?;
            }
            None => {
                reading.await.map_err(|_| ViewError::ReadFailure)?;
            }
        }
        let (header, payload) =
            TransferHeader::decode(&bytes).map_err(|_| ViewError::InvalidCredit)?;
        if header.kind() == PacketKind::Detach
            && header.stream() == self.stream.id
            && payload.is_empty()
        {
            LocalViewStream {
                inner: Arc::clone(&self.stream),
            }
            .seal()?;
            return Ok(LocalCreditRead {
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                packet: header.packet(),
                result: LocalCreditResult::Sealed,
            });
        }
        if let Some(receipt) = self.stream.credit(&bytes, false)? {
            return Ok(LocalCreditRead {
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                packet: header.packet(),
                result: receipt,
            });
        }
        #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
        self.stream
            .hook_pause(
                super::controller::ViewHookPoint::BeforeCommitCursor,
                header.packet(),
            )
            .await?;
        let _guard = authority.current_guard(client, target).await?;
        let result = authority.with_live_issuer(client, || {
            self.stream
                .credit(&bytes, true)?
                .ok_or(ViewError::ReadFailure)
        })?;
        Ok(LocalCreditRead {
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            packet: header.packet(),
            result,
        })
    }
}
struct SendCancellation {
    stream: Arc<Stream>,
    finished: bool,
}
impl Drop for SendCancellation {
    fn drop(&mut self) {
        if !self.finished {
            let handle = LocalViewStream {
                inner: Arc::clone(&self.stream),
            };
            let _ = handle.seal();
        }
    }
}
impl LocalChunkWriter {
    /// Real local IO only. The enclosing fixture/gateway task owner must own this future.
    /// # Errors
    /// Returns actual authorization, seal, IO, or deadline failure; never a Socket flush fact.
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the actual first-write, flush and seal/credit publication ordering reviewable in one ownership flow"
    )]
    pub async fn write_chunk(
        &mut self,
        chunk: PreparedLocalChunk,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
        deadline: Instant,
    ) -> Result<LocalFlushObservation, ViewError> {
        self.write_until(chunk, authority, client, target, Some(deadline))
            .await
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) async fn write_chunk_unbounded(
        &mut self,
        chunk: PreparedLocalChunk,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
    ) -> Result<LocalFlushObservation, ViewError> {
        self.write_until(chunk, authority, client, target, None)
            .await
    }
    #[allow(
        clippy::too_many_lines,
        reason = "Preserves the single original chunk's first accepted byte, actual flush, seal and early-credit publication ordering"
    )]
    async fn write_until(
        &mut self,
        mut chunk: PreparedLocalChunk,
        authority: &LocalViewAuthority,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
        deadline: Option<Instant>,
    ) -> Result<LocalFlushObservation, ViewError> {
        if !Arc::ptr_eq(&self.stream, &chunk.stream)
            || !self
                .stream
                .binding
                .same_client(&authority.client_binding(client)?)
        {
            return Err(ViewError::InvalidClient);
        }
        let mut cancellation = SendCancellation {
            stream: Arc::clone(&self.stream),
            finished: false,
        };
        chunk.handed_off = true;
        let writing = async {
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            {
                self.stream
                    .hook_pause(
                        super::controller::ViewHookPoint::AfterChunkQueuedBeforeWriterGate,
                        chunk.packet,
                    )
                    .await?;
                self.stream
                    .hook_pause(
                        super::controller::ViewHookPoint::BeforeSinkReady,
                        chunk.packet,
                    )
                    .await?;
            }
            let actor_guard = authority.current_guard(client, target).await?;
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            self.stream.hook_observe(
                super::controller::ViewHookPoint::WriterGateHeld,
                chunk.packet,
            )?;
            let mut offset = 0;
            while offset < chunk.bytes.len() {
                let accepted = poll_fn(|cx| {
                    let result = authority.with_live_issuer(client, || {
                        let group = lock(&self.stream.view.group.inner)?;
                        let mut state = lock(&self.stream.state)?;
                        if state.packet != chunk.packet {
                            return Ok(Poll::Ready(Err(ViewError::StaleCursor)));
                        }
                        if offset == 0
                            && (state.phase != LocalTransferPhase::Prepared
                                || !self.stream.live(&group, &state))
                        {
                            return Ok(Poll::Ready(Err(ViewError::StaleCursor)));
                        }
                        if offset > 0 && state.phase != LocalTransferPhase::Started {
                            return Ok(Poll::Ready(Err(ViewError::StaleCursor)));
                        }
                        Ok(
                            match Pin::new(&mut self.writer).poll_write(cx, &chunk.bytes[offset..])
                            {
                                Poll::Ready(Ok(0) | Err(_)) => {
                                    Poll::Ready(Err(ViewError::ReadFailure))
                                }
                                Poll::Ready(Ok(n)) => {
                                    state.phase = LocalTransferPhase::Started;
                                    state.started = chunk.packet;
                                    Poll::Ready(Ok(n))
                                }
                                Poll::Pending => Poll::Pending,
                            },
                        )
                    });
                    match result {
                        Ok(poll) => poll,
                        Err(error) => Poll::Ready(Err(error)),
                    }
                })
                .await?;
                offset += accepted;
                #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
                self.stream.hook_observe(
                    super::controller::ViewHookPoint::AfterStartSendBeforeFlush,
                    chunk.packet,
                )?;
                #[cfg(test)]
                self.stream.changed.notify_waiters();
            }
            {
                let mut state = lock(&self.stream.state)?;
                state.phase = LocalTransferPhase::Flushing;
            }
            poll_fn(|cx| Pin::new(&mut self.writer).poll_flush(cx))
                .await
                .map_err(|_| ViewError::ReadFailure)?;
            drop(actor_guard);
            {
                let mut state = lock(&self.stream.state)?;
                state.actual_flushes = chunk.packet;
            }
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            self.stream
                .hook_pause(
                    super::controller::ViewHookPoint::AfterFlushFactBeforePublish,
                    chunk.packet,
                )
                .await?;
            #[cfg(test)]
            {
                let pause = lock(&self.stream.after_flush)?.take();
                if let Some(pause) = pause {
                    let _ = pause.arrived.send(());
                    let _ = pause.release.await;
                }
            }
            // A local issuer has no socket revocation supervisor. Requalify before publishing
            // a cursor commit, while preserving the actual flush fact on later revocation.
            #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
            {
                let early = { lock(&self.stream.state)?.early };
                if early {
                    self.stream
                        .hook_pause(
                            super::controller::ViewHookPoint::BeforeCommitCursor,
                            chunk.packet,
                        )
                        .await?;
                }
            }
            let current = authority.current_guard(client, target).await;
            let group = lock(&self.stream.view.group.inner)?;
            let mut state = lock(&self.stream.state)?;
            state.flushed = chunk.packet;
            let live = self.stream.live(&group, &state) && current.is_ok();
            if current.is_err() {
                state.sealed = true;
            }
            let committed = live && state.early;
            if committed {
                state.committed = chunk.packet;
                state.cursor = chunk.next_cursor;
                state.phase = LocalTransferPhase::Ready;
            } else {
                state.phase = if live {
                    LocalTransferPhase::Sent
                } else {
                    LocalTransferPhase::Closed
                };
            }
            state.early = false;
            Ok(LocalFlushObservation {
                packet: chunk.packet,
                committed,
                sealed_after_start: !live,
            })
        };
        let actual = match deadline {
            Some(deadline) => timeout_at(deadline, writing)
                .await
                .map_err(|_| ViewError::ReadFailure)?,
            None => writing.await,
        };
        if actual.is_ok() {
            cancellation.finished = true;
        }
        actual
    }
}

#[cfg(test)]
struct TestFlushPause {
    arrived: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
}
#[cfg(test)]
pub(crate) struct TestFlushGate {
    arrived: tokio::sync::oneshot::Receiver<()>,
    release: Option<tokio::sync::oneshot::Sender<()>>,
}
#[cfg(test)]
impl TestFlushGate {
    pub(crate) async fn arrived(&mut self) -> Result<(), ViewError> {
        (&mut self.arrived)
            .await
            .map_err(|_| ViewError::ReadFailure)
    }
    pub(crate) fn release(mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
#[cfg(test)]
impl LocalViewStream {
    pub(crate) fn test_pause_after_flush(&self) -> Result<TestFlushGate, ViewError> {
        let mut slot = lock(&self.inner.after_flush)?;
        if slot.is_some() {
            return Err(ViewError::ReadFailure);
        }
        let (arrived_tx, arrived) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel();
        *slot = Some(TestFlushPause {
            arrived: arrived_tx,
            release: release_rx,
        });
        Ok(TestFlushGate {
            arrived,
            release: Some(release),
        })
    }
    pub(crate) async fn test_wait_started(&self) {
        loop {
            let n = self.inner.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.observation().is_ok_and(|s| s.highest_started > 0) {
                return;
            }
            n.await;
        }
    }
}
