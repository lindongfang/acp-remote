//! Linear observation ownership and startup gate for the node-owned dispatch pump.
use super::{
    AcpError, Arc, AsyncRead, AsyncWrite, Connection, ConnectionScope, Core, EncodedFrame,
    EnqueueError, EventReceiver, Instant, IoOwner, Received, Reservation, WriteFence, WriteTicket,
    fmt, lifecycle,
};
#[cfg(test)]
use super::{
    FenceRejection, PollIo, ResponseDeadline, WriteIdentity, WriteProgress, WriteReport, WriteStep,
    ZeroReason,
};
#[cfg(test)]
use crate::adapters::acp::ExtensionFields;
#[cfg(test)]
use std::task::Context;
pub struct ObservedStart {
    core: Arc<Core>,
    armed: bool,
}
pub struct ObservationReceiver {
    inner: EventReceiver,
}
pub struct ObservationLease {
    core: Arc<Core>,
    received: Received,
    armed: bool,
}
#[derive(Clone)]
pub struct ObservationAcknowledger {
    core: Arc<Core>,
}
#[derive(Clone)]
pub struct ObservedWriteGate {
    core: Arc<Core>,
}
pub struct AcknowledgeError {
    error: AcpError,
    lease: ObservationLease,
}
pub struct IoAdoptionError {
    error: AcpError,
    owner: IoOwner,
}
#[derive(Clone, Copy, Debug)]
pub enum ObservedPreparePhase {
    BeforeAdopt,
    AfterAdopt,
}
#[derive(Debug)]
pub struct ObservedPrepareError {
    error: AcpError,
    phase: ObservedPreparePhase,
}
impl ObservedPrepareError {
    pub(super) const fn before(error: AcpError) -> Self {
        Self {
            error,
            phase: ObservedPreparePhase::BeforeAdopt,
        }
    }
    #[must_use]
    pub const fn error(&self) -> AcpError {
        self.error
    }
    #[must_use]
    pub const fn phase(&self) -> ObservedPreparePhase {
        self.phase
    }
}
impl IoAdoptionError {
    #[must_use]
    pub const fn new(error: AcpError, owner: IoOwner) -> Self {
        Self { error, owner }
    }
    #[must_use]
    pub fn into_parts(self) -> (AcpError, IoOwner) {
        (self.error, self.owner)
    }
}
impl Connection {
    /// Prepares zero-poll IO; the original owner is adopted before any task is spawned.
    /// # Errors
    /// `BeforeAdopt` failures own no tasks. Successful adoption retains all cleanup in that owner.
    pub fn prepare_observed<R, W, A>(
        reader: R,
        writer: W,
        adopt: A,
    ) -> Result<
        (
            ObservedStart,
            ObservationReceiver,
            ObservationAcknowledger,
            ObservedWriteGate,
        ),
        ObservedPrepareError,
    >
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
        A: FnOnce(IoOwner) -> Result<(), IoAdoptionError>,
    {
        let core = lifecycle::prepare(
            reader,
            writer,
            true,
            adopt,
            #[cfg(test)]
            None,
        )?;
        Ok((
            ObservedStart {
                core: core.clone(),
                armed: true,
            },
            ObservationReceiver {
                inner: EventReceiver {
                    core: core.clone(),
                    ended: false,
                },
            },
            ObservationAcknowledger { core: core.clone() },
            ObservedWriteGate { core },
        ))
    }
    /// # Errors
    /// Returns both original inputs on invalid scope, state, frame or write deadline.
    pub fn enqueue(
        &self,
        reservation: Reservation,
        frame: EncodedFrame,
        fence: Arc<dyn WriteFence>,
        deadline: Instant,
    ) -> Result<WriteTicket, EnqueueError> {
        self.enqueue_inner(
            reservation,
            frame,
            fence,
            deadline,
            super::EnqueueMode::Ordinary,
        )
    }
    /// # Errors
    /// Rejects a gate from another Core and returns the original reservation/frame.
    pub fn enqueue_observed(
        &self,
        reservation: Reservation,
        frame: EncodedFrame,
        fence: Arc<dyn WriteFence>,
        deadline: Instant,
        gate: &ObservedWriteGate,
    ) -> Result<WriteTicket, EnqueueError> {
        if !Arc::ptr_eq(&self.core, &gate.core) {
            return Err(EnqueueError {
                error: AcpError::InvalidRequestScope,
                reservation,
                frame,
            });
        }
        self.enqueue_inner(
            reservation,
            frame,
            fence,
            deadline,
            super::EnqueueMode::ObservedPrompt,
        )
    }
    /// # Errors
    /// Returns the original close inputs if the original observation gate is foreign.
    pub(crate) fn enqueue_automatic_close(
        &self,
        reservation: Reservation,
        frame: EncodedFrame,
        fence: Arc<dyn WriteFence>,
        deadline: Instant,
        gate: &ObservedWriteGate,
    ) -> Result<WriteTicket, EnqueueError> {
        if !Arc::ptr_eq(&self.core, &gate.core)
            || !matches!(
                reservation.kind(),
                super::OutboundKind::Request(super::RequestKind::Close(_))
            )
        {
            return Err(EnqueueError {
                error: AcpError::InvalidRequestScope,
                reservation,
                frame,
            });
        }
        self.enqueue_inner(
            reservation,
            frame,
            fence,
            deadline,
            super::EnqueueMode::AutomaticClose,
        )
    }
}
impl ObservedStart {
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.core.scope.clone()
    }
    #[must_use]
    pub fn start(mut self) -> Connection {
        self.core.lock().started = true;
        self.armed = false;
        self.core.notify.notify_waiters();
        Connection {
            core: self.core.clone(),
        }
    }
}
impl Drop for ObservedStart {
    fn drop(&mut self) {
        if self.armed {
            self.core.stop(None);
        }
    }
}
impl ObservationReceiver {
    /// # Errors
    /// Publishes the original receiver terminal error after queued observations have drained.
    pub async fn next(&mut self) -> Result<Option<ObservationLease>, AcpError> {
        let event = self.inner.next().await?;
        let lease = event.map(|received| ObservationLease {
            core: self.inner.core.clone(),
            received,
            armed: true,
        });
        #[cfg(test)]
        if lease.is_some() {
            test_pause(
                &self.inner.core,
                ObservationHookPoint::AfterEmitBeforeLeaseDelivery,
            )
            .await;
        }
        Ok(lease)
    }
}
impl ObservationLease {
    #[must_use]
    pub const fn received(&self) -> &Received {
        &self.received
    }
}
impl Drop for ObservationLease {
    fn drop(&mut self) {
        if self.armed {
            self.core.stop(Some(AcpError::InvalidMessage));
        }
    }
}
impl ObservationAcknowledger {
    /// # Errors
    /// A foreign, duplicate or non-next observation keeps its original lease and frontier.
    #[expect(
        clippy::result_large_err,
        reason = "Return the same original linear objects on rejection without a new fallible allocation or loss of cleanup ownership."
    )]
    pub fn acknowledge(&self, mut lease: ObservationLease) -> Result<(), AcknowledgeError> {
        let mut state = self.core.lock();
        if !Arc::ptr_eq(&self.core, &lease.core)
            || state.acknowledged.checked_add(1) != Some(lease.received.sequence())
        {
            drop(state);
            return Err(AcknowledgeError {
                error: AcpError::InvalidRequestScope,
                lease,
            });
        }
        state.acknowledged = lease.received.sequence();
        lease.armed = false;
        drop(state);
        self.core.notify.notify_waiters();
        Ok(())
    }
}
impl AcknowledgeError {
    #[must_use]
    pub const fn error(&self) -> AcpError {
        self.error
    }
    #[must_use]
    pub fn into_lease(self) -> ObservationLease {
        self.lease
    }
}
macro_rules! hidden {($($t:ty),*)=>{$(impl fmt::Debug for $t {
    fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(concat!(stringify!($t),"(..)"))}
})*}}
hidden!(
    ObservedStart,
    ObservationReceiver,
    ObservationLease,
    ObservationAcknowledger,
    ObservedWriteGate,
    AcknowledgeError,
    IoAdoptionError
);

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObservationHookPoint {
    AfterReadCapturedBeforeRoute,
    AfterEmitBeforeLeaseDelivery,
    AfterLeaseDeliveryBeforeApply,
    AfterApplyBeforeAck,
}
#[cfg(test)]
#[derive(Default)]
pub struct AcpObservationHooks {
    pauses: std::sync::Mutex<
        std::collections::HashMap<ObservationHookPoint, Arc<crate::test_support::AsyncBarrier>>,
    >,
    polls: std::sync::atomic::AtomicU64,
}
#[cfg(test)]
impl AcpObservationHooks {
    /// # Errors
    /// Rejects a poisoned hook registry or an already armed one-shot pause.
    pub fn pause_next(
        &self,
        point: ObservationHookPoint,
    ) -> Result<Arc<crate::test_support::AsyncBarrier>, AcpError> {
        let barrier = Arc::new(crate::test_support::AsyncBarrier::default());
        let mut slots = self.pauses.lock().map_err(|_| AcpError::Io)?;
        if slots.contains_key(&point) {
            return Err(AcpError::InvalidMessage);
        }
        slots.insert(point, barrier.clone());
        Ok(barrier)
    }
    #[must_use]
    pub fn writer_polls(&self) -> u64 {
        self.polls.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(super) fn writer_polled(&self) {
        self.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
#[cfg(test)]
pub(super) async fn test_pause(core: &Core, point: ObservationHookPoint) {
    let barrier = core
        .hooks
        .as_ref()
        .and_then(|h| h.pauses.lock().ok()?.remove(&point));
    if let Some(barrier) = barrier {
        let stopped = async {
            loop {
                let signal = core.notify.notified();
                tokio::pin!(signal);
                signal.as_mut().enable();
                if core.lock().stopped {
                    break;
                }
                signal.await;
            }
        };
        tokio::select! {()=barrier.pause()=>{},()=stopped=>{}}
    }
}
#[cfg(test)]
impl Connection {
    /// # Errors
    /// Returns preparation/adoption errors without starting observed IO tasks before adoption.
    pub fn prepare_observed_with_hooks<R, W, A>(
        reader: R,
        writer: W,
        hooks: Arc<AcpObservationHooks>,
        adopt: A,
    ) -> Result<
        (
            ObservedStart,
            ObservationReceiver,
            ObservationAcknowledger,
            ObservedWriteGate,
        ),
        ObservedPrepareError,
    >
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
        A: FnOnce(IoOwner) -> Result<(), IoAdoptionError>,
    {
        let core = lifecycle::prepare(reader, writer, true, adopt, Some(hooks))?;
        Ok((
            ObservedStart {
                core: core.clone(),
                armed: true,
            },
            ObservationReceiver {
                inner: EventReceiver {
                    core: core.clone(),
                    ended: false,
                },
            },
            ObservationAcknowledger { core: core.clone() },
            ObservedWriteGate { core },
        ))
    }
}
#[cfg(test)]
impl ObservationLease {
    pub async fn before_apply_for_test(&self) {
        test_pause(
            &self.core,
            ObservationHookPoint::AfterLeaseDeliveryBeforeApply,
        )
        .await;
    }
    pub async fn before_ack_for_test(&self) {
        test_pause(&self.core, ObservationHookPoint::AfterApplyBeforeAck).await;
    }
}
#[cfg(test)]
mod tests;
