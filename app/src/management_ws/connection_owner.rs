//! Network task custody. Only the supervisor polls the original IO task handles.
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    sync::{MutexGuard, Weak},
    task::{Context, Poll},
};

use tokio::{runtime::Handle, sync::oneshot, task::JoinHandle, time::Sleep};

use super::{
    Arc, CLOSE_GOING_AWAY, CLOSE_RATE_LIMITED, CloseDirective, HashMap, Id128,
    MAX_CONNECTIONS_PER_CLIENT, MAX_MANAGEMENT_CONNECTIONS, MAX_UNAUTHENTICATED_CONNECTIONS,
    ManagementState, Mutex, Notify, SocketLifetime, WRITER_CLOSE_TIMEOUT, WebSocket,
    WriterCloseError, WriterSettlement, close, close_immediately, run_owned_connection, watch,
};

#[cfg(test)]
use {super::pause_boundary, std::sync::PoisonError};

struct RegistryState {
    total: usize,
    unauthenticated: usize,
    authenticated: HashMap<Id128, usize>,
    closing: bool,
    healthy: bool,
    next_slot: u64,
    slots: HashMap<u64, ConnectionSlot>,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            total: 0,
            unauthenticated: 0,
            authenticated: HashMap::new(),
            closing: false,
            healthy: true,
            next_slot: 0,
            slots: HashMap::new(),
        }
    }
}

pub(super) struct ConnectionRegistry {
    state: Mutex<RegistryState>,
    changed: Arc<Notify>,
    shutdown: watch::Sender<bool>,
    runtime: Option<Handle>,
    supervisor: tokio::sync::Mutex<Supervisor>,
    #[cfg(test)]
    exit_barrier: Mutex<Option<crate::test_support::SharedAsyncBarrier>>,
}

struct ConnectionSlot {
    pending_upgrade: bool,
    lease: bool,
    authenticated: Option<Id128>,
    stop: watch::Sender<bool>,
    complete: watch::Sender<Option<ConnectionSettlement>>,
    kind: ConnectionKind,
    protocol: Option<JoinHandle<()>>,
    protocol_result: Option<ProtocolSettlement>,
    writer: Option<WriterTask>,
}

struct WriterTask {
    handle: Option<JoinHandle<Result<(), WriterCloseError>>>,
    close: watch::Sender<Option<CloseDirective>>,
    lifetime: Arc<SocketLifetime>,
    result: watch::Sender<Option<WriterSettlement>>,
    deadline: Option<Pin<Box<Sleep>>>,
    abort_requested: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AcquireError {
    Closing,
    Limit,
}

pub(super) struct ConnectionLease {
    registry: Arc<ConnectionRegistry>,
    authenticated: Option<Id128>,
    slot: Option<u64>,
}

/// Receipt cancellation requests stop; it never owns an IO task handle or lease.
pub(super) struct ConnectionObservation {
    outcome: watch::Receiver<Option<ConnectionSettlement>>,
    stop: watch::Sender<bool>,
    wait: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for ConnectionObservation {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let result = self.wait.as_mut().poll(cx);
        if result.is_ready()
            && let Some(outcome) = self.settlement()
            && outcome.protocol != ProtocolSettlement::Joined
        {
            tracing::warn!(?outcome, "management protocol task did not finish cleanly");
        }
        result
    }
}

impl ConnectionObservation {
    pub(super) fn settlement(&self) -> Option<ConnectionSettlement> {
        *self.outcome.borrow()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProtocolSettlement {
    Joined,
    Aborted,
    TaskFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConnectionKind {
    Protocol,
    RejectedClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConnectionSettlement {
    pub(super) kind: ConnectionKind,
    pub(super) protocol: ProtocolSettlement,
    pub(super) writer: Option<WriterSettlement>,
}

struct Supervisor {
    handle: Option<JoinHandle<()>>,
    settlement: Option<ProtocolSettlement>,
}

impl Drop for ConnectionObservation {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}

pub(super) struct PendingUpgradeTicket {
    registry: Arc<ConnectionRegistry>,
    slot: u64,
    armed: bool,
}

pub(super) struct ConnectionContext {
    registry: Arc<ConnectionRegistry>,
    slot: u64,
}

pub(super) struct WriterReceipt(watch::Receiver<Option<WriterSettlement>>);

impl WriterReceipt {
    pub(super) async fn wait(mut self) -> WriterSettlement {
        loop {
            let result = *self.0.borrow_and_update();
            if let Some(result) = result {
                return result;
            }
            if self.0.changed().await.is_err() {
                // Losing an owner is not evidence that its original task joined.
                std::future::pending::<()>().await;
            }
        }
    }
}

pub(super) struct ConnectionStop {
    node: watch::Receiver<bool>,
    slot: watch::Receiver<bool>,
}

impl ConnectionStop {
    pub(super) fn stopped(&self) -> bool {
        *self.node.borrow() || *self.slot.borrow()
    }

    pub(super) async fn wait(&mut self) {
        loop {
            if self.stopped() {
                return;
            }
            tokio::select! {
                changed = self.node.changed() => {
                    if changed.is_err() { return; }
                }
                changed = self.slot.changed() => {
                    if changed.is_err() { return; }
                }
            }
        }
    }
}

impl ConnectionRegistry {
    pub(super) fn new_shared() -> Arc<Self> {
        let runtime = Handle::try_current().ok();
        let changed = Arc::new(Notify::new());
        let (shutdown, _) = watch::channel(false);
        let (start, started) = oneshot::channel();
        let registry = Arc::new_cyclic(|weak: &Weak<Self>| {
            let supervisor = runtime.as_ref().map(|runtime| {
                let owner = weak.clone();
                let changed = changed.clone();
                runtime.spawn(async move {
                    if started.await.is_ok() {
                        Self::supervise(owner, changed).await;
                    }
                })
            });
            Self {
                state: Mutex::new(RegistryState::default()),
                changed,
                shutdown,
                runtime,
                supervisor: tokio::sync::Mutex::new(Supervisor {
                    handle: supervisor,
                    settlement: None,
                }),
                #[cfg(test)]
                exit_barrier: Mutex::new(None),
            }
        });
        let _ = start.send(());
        registry
    }

    /// Poison recovery is solely for retaining and cleaning up the original handles.
    fn cleanup_state(&self) -> MutexGuard<'_, RegistryState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.healthy = false;
                state.closing = true;
                self.shutdown.send_replace(true);
                state
            }
        }
    }

    pub(super) fn acquire(self: &Arc<Self>) -> Result<ConnectionLease, AcquireError> {
        let mut state = self.state.lock().map_err(|_| AcquireError::Closing)?;
        if state.closing || !state.healthy {
            return Err(AcquireError::Closing);
        }
        if state.total >= MAX_MANAGEMENT_CONNECTIONS
            || state.unauthenticated >= MAX_UNAUTHENTICATED_CONNECTIONS
        {
            return Err(AcquireError::Limit);
        }
        state.total += 1;
        state.unauthenticated += 1;
        drop(state);
        Ok(ConnectionLease {
            registry: self.clone(),
            authenticated: None,
            slot: None,
        })
    }

    pub(super) fn begin_shutdown(&self) {
        {
            let mut state = self.cleanup_state();
            state.closing = true;
        }
        self.shutdown.send_replace(true);
        self.changed.notify_waiters();
    }

    pub(super) async fn wait_empty(&self) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let (empty, closing) = {
                let state = self.cleanup_state();
                (
                    state.healthy && state.total == 0 && state.slots.is_empty(),
                    state.closing,
                )
            };
            if empty {
                if closing && self.join_supervisor().await.is_err() {
                    continue;
                }
                return;
            }
            notified.await;
        }
    }

    async fn join_supervisor(&self) -> Result<(), ProtocolSettlement> {
        let mut supervisor = self.supervisor.lock().await;
        if let Some(handle) = supervisor.handle.as_mut() {
            let result = match handle.await {
                Ok(()) => ProtocolSettlement::Joined,
                Err(error) if error.is_cancelled() => ProtocolSettlement::Aborted,
                Err(_) => ProtocolSettlement::TaskFailed,
            };
            supervisor.handle = None;
            supervisor.settlement = Some(result);
        }
        let result = supervisor.settlement.unwrap_or_else(|| {
            if self.runtime.is_none() {
                ProtocolSettlement::Joined
            } else {
                ProtocolSettlement::TaskFailed
            }
        });
        drop(supervisor);
        if result != ProtocolSettlement::Joined {
            self.cleanup_state().healthy = false;
            return Err(result);
        }
        if !self.cleanup_state().healthy {
            return Err(ProtocolSettlement::TaskFailed);
        }
        Ok(())
    }

    pub(super) fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub(super) fn prepare_upgrade(self: &Arc<Self>) -> Result<PendingUpgradeTicket, ()> {
        if self.runtime.is_none() {
            return Err(());
        }
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.closing || !state.healthy {
            return Err(());
        }
        let id = state.next_slot.checked_add(1).ok_or(())?;
        state.next_slot = id;
        let (stop, _) = watch::channel(false);
        let (complete, _) = watch::channel(None);
        state.slots.insert(
            id,
            ConnectionSlot {
                pending_upgrade: true,
                lease: false,
                authenticated: None,
                stop,
                complete,
                kind: ConnectionKind::Protocol,
                protocol: None,
                protocol_result: None,
                writer: None,
            },
        );
        drop(state);
        self.changed.notify_waiters();
        Ok(PendingUpgradeTicket {
            registry: self.clone(),
            slot: id,
            armed: true,
        })
    }

    fn observe(slot: &ConnectionSlot) -> ConnectionObservation {
        let mut complete = slot.complete.subscribe();
        ConnectionObservation {
            outcome: complete.clone(),
            stop: slot.stop.clone(),
            wait: Box::pin(async move {
                if complete.wait_for(Option::is_some).await.is_err() {
                    std::future::pending::<()>().await;
                }
            }),
        }
    }

    /// Called synchronously by the upgrade `FnOnce`, before its observation future exists.
    fn adopt(
        self: &Arc<Self>,
        id: u64,
        socket: WebSocket,
        state: Arc<ManagementState>,
        existing: Option<ConnectionLease>,
        shutdown: watch::Receiver<bool>,
    ) -> ConnectionObservation {
        let accepted = existing.map_or_else(|| self.acquire(), Ok);
        let mut registry = self.cleanup_state();
        let Some(slot) = registry.slots.get_mut(&id) else {
            // An internal custody failure cannot produce a successful receipt.
            registry.healthy = false;
            drop(registry);
            drop((socket, accepted));
            return Self::unconfirmed_observation();
        };
        let observation = Self::observe(slot);
        let Some(runtime) = &self.runtime else {
            drop(registry);
            drop((socket, accepted));
            return observation;
        };
        let (start, started) = oneshot::channel();
        let task = match accepted {
            Ok(mut lease) => {
                lease.slot = Some(id);
                slot.lease = true;
                slot.authenticated = lease.authenticated;
                let context = ConnectionContext {
                    registry: self.clone(),
                    slot: id,
                };
                let mut stop = ConnectionStop {
                    node: shutdown,
                    slot: slot.stop.subscribe(),
                };
                runtime.spawn(async move {
                    if started.await.is_ok() {
                        run_owned_connection(socket, state, lease, &mut stop, context).await;
                    }
                })
            }
            Err(error) => {
                slot.kind = ConnectionKind::RejectedClose;
                let frame = match error {
                    AcquireError::Closing => close(CLOSE_GOING_AWAY, "node stopping"),
                    AcquireError::Limit => close(CLOSE_RATE_LIMITED, "limit exceeded"),
                };
                runtime.spawn(async move {
                    if started.await.is_ok() {
                        #[cfg(test)]
                        pause_boundary(&state.test_hooks.rejected_close).await;
                        close_immediately(socket, frame).await;
                    }
                })
            }
        };
        slot.protocol = Some(task);
        slot.pending_upgrade = false;
        drop(registry);
        let _ = start.send(());
        self.changed.notify_waiters();
        observation
    }

    fn unconfirmed_observation() -> ConnectionObservation {
        let (stop, _) = watch::channel(true);
        let (_, outcome) = watch::channel(None);
        ConnectionObservation {
            outcome,
            stop,
            wait: Box::pin(std::future::pending()),
        }
    }

    #[cfg(test)]
    pub(super) fn adopt_existing(
        self: &Arc<Self>,
        socket: WebSocket,
        state: Arc<ManagementState>,
        lease: ConnectionLease,
        shutdown: watch::Receiver<bool>,
    ) -> ConnectionObservation {
        if let Ok(mut ticket) = self.prepare_upgrade() {
            let observation = self.adopt(ticket.slot, socket, state, Some(lease), shutdown);
            ticket.armed = false;
            observation
        } else {
            drop((socket, lease));
            Self::unconfirmed_observation()
        }
    }

    async fn supervise(owner: Weak<Self>, changed: Arc<Notify>) {
        loop {
            let notified = changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let finished = poll_fn(|cx| {
                let Some(owner) = owner.upgrade() else {
                    return Poll::Ready(true);
                };
                if owner.poll_jobs(cx) {
                    return Poll::Ready(true);
                }
                if notified.as_mut().poll(cx).is_ready() {
                    Poll::Ready(false)
                } else {
                    Poll::Pending
                }
            })
            .await;
            if finished {
                #[cfg(test)]
                if let Some(owner) = owner.upgrade() {
                    let barrier = owner
                        .exit_barrier
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    if let Some(barrier) = barrier {
                        barrier.pause().await;
                    }
                }
                return;
            }
        }
    }

    fn poll_jobs(&self, cx: &mut Context<'_>) -> bool {
        let mut state = self.cleanup_state();
        let mut finished = Vec::new();
        let mut progress = false;
        let mut seals = Vec::new();
        for (id, slot) in &mut state.slots {
            if let Some(protocol) = slot.protocol.as_mut()
                && let Poll::Ready(result) = Pin::new(protocol).poll(cx)
            {
                slot.protocol_result = Some(match result {
                    Ok(()) => ProtocolSettlement::Joined,
                    Err(error) if error.is_cancelled() => ProtocolSettlement::Aborted,
                    Err(_) => ProtocolSettlement::TaskFailed,
                });
                slot.protocol = None;
                progress = true;
            }
            if let Some(writer) = slot.writer.as_mut() {
                if slot.protocol_result.is_some()
                    && writer.handle.is_some()
                    && writer.deadline.is_none()
                {
                    writer.begin_close(CloseDirective::Initiate(close(
                        CLOSE_GOING_AWAY,
                        "node stopping",
                    )));
                    seals.push(writer.lifetime.clone());
                }
                progress |= writer.poll_join(cx);
            }
            if !slot.pending_upgrade
                && slot.protocol_result.is_some()
                && slot
                    .writer
                    .as_ref()
                    .is_none_or(|writer| writer.handle.is_none())
            {
                finished.push(*id);
            }
        }
        let mut receipts = Vec::new();
        for id in finished {
            if let Some(slot) = state.slots.remove(&id) {
                if slot.lease {
                    Self::release_count(&mut state, slot.authenticated);
                }
                let result = ConnectionSettlement {
                    kind: slot.kind,
                    protocol: slot
                        .protocol_result
                        .unwrap_or(ProtocolSettlement::TaskFailed),
                    writer: slot
                        .writer
                        .as_ref()
                        .and_then(|writer| *writer.result.borrow()),
                };
                receipts.push((slot.complete, result));
                progress = true;
            }
        }
        let done = state.healthy && state.closing && state.total == 0 && state.slots.is_empty();
        drop(state);
        for lifetime in seals {
            lifetime.seal();
        }
        for (receipt, result) in receipts {
            receipt.send_replace(Some(result));
        }
        if progress {
            self.changed.notify_waiters();
        }
        done
    }

    fn release_count(state: &mut RegistryState, authenticated: Option<Id128>) {
        state.total = state.total.saturating_sub(1);
        if let Some(client) = authenticated {
            if let Some(count) = state.authenticated.get_mut(&client) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.authenticated.remove(&client);
                }
            }
        } else {
            state.unauthenticated = state.unauthenticated.saturating_sub(1);
        }
    }
}

impl Drop for ConnectionRegistry {
    fn drop(&mut self) {
        self.shutdown.send_replace(true);
        self.changed.notify_waiters();
    }
}

impl PendingUpgradeTicket {
    pub(super) fn adopt_socket(
        mut self,
        socket: WebSocket,
        state: Arc<ManagementState>,
    ) -> ConnectionObservation {
        let shutdown = self.registry.shutdown_receiver();
        let observation = self
            .registry
            .adopt(self.slot, socket, state, None, shutdown);
        self.armed = false;
        observation
    }
}

impl Drop for PendingUpgradeTicket {
    fn drop(&mut self) {
        if self.armed {
            let mut state = self.registry.cleanup_state();
            if state
                .slots
                .get(&self.slot)
                .is_some_and(|slot| slot.pending_upgrade)
            {
                state.slots.remove(&self.slot);
            }
            drop(state);
            self.registry.changed.notify_waiters();
        }
    }
}

impl ConnectionLease {
    pub(super) fn authenticate(&mut self, client_id: Id128) -> bool {
        let Ok(mut state) = self.registry.state.lock() else {
            return false;
        };
        if state.closing || !state.healthy || self.authenticated.is_some() {
            return false;
        }
        if state.authenticated.get(&client_id).copied().unwrap_or(0) >= MAX_CONNECTIONS_PER_CLIENT {
            return false;
        }
        if let Some(id) = self.slot {
            let Some(slot) = state.slots.get_mut(&id) else {
                return false;
            };
            slot.authenticated = Some(client_id);
        }
        state.unauthenticated = state.unauthenticated.saturating_sub(1);
        *state.authenticated.entry(client_id).or_default() += 1;
        self.authenticated = Some(client_id);
        true
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        if self.slot.is_none() {
            ConnectionRegistry::release_count(
                &mut self.registry.cleanup_state(),
                self.authenticated,
            );
            self.registry.changed.notify_waiters();
        }
    }
}

impl ConnectionContext {
    pub(super) fn start_writer<F>(
        &self,
        future: F,
        close: watch::Sender<Option<CloseDirective>>,
        lifetime: Arc<SocketLifetime>,
    ) -> Result<WriterReceipt, ()>
    where
        F: Future<Output = Result<(), WriterCloseError>> + Send + 'static,
    {
        let Some(runtime) = &self.registry.runtime else {
            return Err(());
        };
        let mut state = self.registry.cleanup_state();
        let slot = state.slots.get_mut(&self.slot).ok_or(())?;
        if slot.writer.is_some() {
            return Err(());
        }
        let (result, received) = watch::channel(None);
        let (start, started) = oneshot::channel();
        let handle = runtime.spawn(async move {
            if started.await.is_err() {
                return Err(WriterCloseError::Stopped);
            }
            future.await
        });
        slot.writer = Some(WriterTask {
            handle: Some(handle),
            close,
            lifetime,
            result,
            deadline: None,
            abort_requested: false,
        });
        drop(state);
        let _ = start.send(());
        self.registry.changed.notify_waiters();
        Ok(WriterReceipt(received))
    }

    pub(super) fn close_writer(&self, directive: CloseDirective) {
        let lifetime = {
            let mut state = self.registry.cleanup_state();
            state
                .slots
                .get_mut(&self.slot)
                .and_then(|slot| slot.writer.as_mut())
                .map(|writer| {
                    writer.begin_close(directive);
                    writer.lifetime.clone()
                })
        };
        if let Some(lifetime) = lifetime {
            lifetime.seal();
        }
        self.registry.changed.notify_waiters();
    }
}

impl WriterTask {
    fn begin_close(&mut self, directive: CloseDirective) {
        self.close.send_replace(Some(directive));
        if self.deadline.is_none() {
            self.deadline = Some(Box::pin(tokio::time::sleep(WRITER_CLOSE_TIMEOUT)));
        }
    }

    fn poll_join(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(handle) = self.handle.as_mut() else {
            return false;
        };
        if let Poll::Ready(result) = Pin::new(&mut *handle).poll(cx) {
            let result = match result {
                Ok(result) => WriterSettlement::Joined(result),
                Err(error) if error.is_cancelled() => WriterSettlement::Aborted,
                Err(_) => WriterSettlement::TaskFailed,
            };
            self.result.send_replace(Some(result));
            self.handle = None;
            return true;
        }
        if !self.abort_requested
            && self
                .deadline
                .as_mut()
                .is_some_and(|deadline| deadline.as_mut().poll(cx).is_ready())
        {
            handle.abort();
            self.abort_requested = true;
        }
        false
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct RegistrySnapshot {
    pub(super) total: usize,
    pub(super) pending_upgrades: usize,
    pub(super) slots: usize,
    pub(super) protocol_handles: usize,
    pub(super) writer_handles: usize,
    pub(super) healthy: bool,
}
#[cfg(test)]
impl ConnectionRegistry {
    pub(super) fn snapshot_for_test(&self) -> RegistrySnapshot {
        let state = self.cleanup_state();
        RegistrySnapshot {
            total: state.total,
            pending_upgrades: state
                .slots
                .values()
                .filter(|slot| slot.pending_upgrade)
                .count(),
            slots: state.slots.len(),
            protocol_handles: state
                .slots
                .values()
                .filter(|slot| slot.protocol.is_some())
                .count(),
            writer_handles: state
                .slots
                .values()
                .filter(|slot| {
                    slot.writer
                        .as_ref()
                        .is_some_and(|writer| writer.handle.is_some())
                })
                .count(),
            healthy: state.healthy,
        }
    }
}

#[cfg(test)]
mod settlement_development {
    use super::*;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn connection_owner_development_supervisor_failure_stays_failed_for_all_waiters() {
        let registry = ConnectionRegistry::new_shared();
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        *registry.exit_barrier.lock().unwrap() = Some(barrier.clone());
        registry.begin_shutdown();
        timeout(Duration::from_secs(5), barrier.wait_until_entered())
            .await
            .unwrap();
        let abort = registry
            .supervisor
            .lock()
            .await
            .handle
            .as_ref()
            .unwrap()
            .abort_handle();
        abort.abort();
        let (first, second) = tokio::join!(registry.join_supervisor(), registry.join_supervisor());
        assert_eq!(first, Err(ProtocolSettlement::Aborted));
        assert_eq!(second, first);
        assert!(!registry.snapshot_for_test().healthy);
        assert!(
            timeout(Duration::from_millis(50), registry.wait_empty())
                .await
                .is_err()
        );
        let state = registry.supervisor.lock().await;
        assert!(
            state.handle.is_none(),
            "original failed supervisor was actually joined"
        );
        assert_eq!(state.settlement, Some(ProtocolSettlement::Aborted));
    }
}
