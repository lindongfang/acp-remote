//! Single-use IO controls around the original duplex; no protocol or delivery facts.
use super::FixtureError;
use std::{
    io,
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Waker},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf},
    sync::Notify,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureIoPlan {
    NextFlush,
    WritePrefix { forwarded: NonZeroUsize },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureIoBoundary {
    FlushPending,
    WritePrefixPending,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureIoFault {
    BrokenPipe,
    ConnectionReset,
}
impl FixtureIoFault {
    const fn kind(self) -> io::ErrorKind {
        match self {
            Self::BrokenPipe => io::ErrorKind::BrokenPipe,
            Self::ConnectionReset => io::ErrorKind::ConnectionReset,
        }
    }
}
pub struct FixtureIoSnapshot {
    identity: Arc<()>,
    revision: u64,
    forwarded: u64,
    flushes: u64,
    closed: bool,
}
impl FixtureIoSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn forwarded_bytes(&self) -> u64 {
        self.forwarded
    }
    #[must_use]
    pub const fn delegated_flushes(&self) -> u64 {
        self.flushes
    }
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixtureIoObservation {
    boundary: FixtureIoBoundary,
    forwarded: u64,
    flushes: u64,
}
impl FixtureIoObservation {
    #[must_use]
    pub const fn boundary(&self) -> FixtureIoBoundary {
        self.boundary
    }
    #[must_use]
    pub const fn forwarded_since_arm(&self) -> u64 {
        self.forwarded
    }
    #[must_use]
    pub const fn delegated_flushes(&self) -> u64 {
        self.flushes
    }
}
#[derive(Clone)]
pub struct FixtureIoController {
    inner: Arc<Control>,
}
struct Control {
    identity: Arc<()>,
    state: Mutex<State>,
    changed: Notify,
}
struct State {
    revision: u64,
    next: u64,
    forwarded: u64,
    flushes: u64,
    polling: bool,
    error: Option<FixtureError>,
    arm: Option<Arm>,
    wake: Option<Waker>,
}
struct Arm {
    nonce: u64,
    plan: FixtureIoPlan,
    forwarded: u64,
    observed: Option<FixtureIoObservation>,
    fault: Option<FixtureIoFault>,
}
pub struct FixtureIoGate {
    control: FixtureIoController,
    nonce: u64,
    active: bool,
}
impl State {
    fn bump(&mut self) {
        match self.revision.checked_add(1) {
            Some(revision) => self.revision = revision,
            None => self.error = self.error.or(Some(FixtureError::Io)),
        }
    }
}
impl Control {
    fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.error = Some(FixtureError::Io);
                state
            }
        }
    }
}
impl FixtureIoController {
    pub(super) fn new() -> Self {
        Self {
            inner: Arc::new(Control {
                identity: Arc::new(()),
                changed: Notify::new(),
                state: Mutex::new(State {
                    revision: 0,
                    next: 0,
                    forwarded: 0,
                    flushes: 0,
                    polling: false,
                    error: None,
                    arm: None,
                    wake: None,
                }),
            }),
        }
    }
    #[must_use]
    pub fn snapshot(&self) -> FixtureIoSnapshot {
        let state = self.inner.lock();
        FixtureIoSnapshot {
            identity: Arc::clone(&self.inner.identity),
            revision: state.revision,
            forwarded: state.forwarded,
            flushes: state.flushes,
            closed: state.error.is_some(),
        }
    }
    /// # Errors
    /// Rejects an active arm, an in-progress underlying poll or a stopped original resource.
    pub fn arm(&self, plan: FixtureIoPlan) -> Result<FixtureIoGate, FixtureError> {
        let mut state = self.inner.lock();
        if let Some(error) = state.error {
            return Err(error);
        }
        if state.polling || state.arm.is_some() {
            return Err(FixtureError::ControlBusy);
        }
        if state.next == u64::MAX || state.revision == u64::MAX {
            drop(state);
            self.stop(FixtureError::Io);
            return Err(FixtureError::Io);
        }
        let nonce = state.next.checked_add(1).ok_or(FixtureError::Io)?;
        state.next = nonce;
        state.arm = Some(Arm {
            nonce,
            plan,
            forwarded: 0,
            observed: None,
            fault: None,
        });
        state.bump();
        let error = state.error;
        drop(state);
        self.inner.changed.notify_waiters();
        if let Some(error) = error {
            return Err(error);
        }
        Ok(FixtureIoGate {
            control: self.clone(),
            nonce,
            active: true,
        })
    }
    /// # Errors
    /// Foreign snapshots are unsupported; a stopped resource retains its actual close error.
    pub async fn changed_after(
        &self,
        previous: &FixtureIoSnapshot,
    ) -> Result<FixtureIoSnapshot, FixtureError> {
        if !Arc::ptr_eq(&self.inner.identity, &previous.identity) {
            return Err(FixtureError::Unsupported);
        }
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let current = self.snapshot();
            if current.closed {
                return Err(self.inner.lock().error.unwrap_or(FixtureError::Cancelled));
            }
            if current.revision != previous.revision {
                return Ok(current);
            }
            notified.await;
        }
    }
    pub(super) fn close(&self) {
        self.stop(FixtureError::Cancelled);
    }
    fn stop(&self, error: FixtureError) {
        let wake = {
            let mut state = self.inner.lock();
            if state.error.is_none() {
                state.error = Some(error);
                state.bump();
            }
            state.arm = None;
            state.wake.take()
        };
        self.inner.changed.notify_waiters();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    fn resolve(&self, nonce: u64, fault: Option<FixtureIoFault>) -> Result<(), FixtureError> {
        let wake = {
            let mut state = self.inner.lock();
            if let Some(error) = state.error {
                return Err(error);
            }
            let arm = state
                .arm
                .as_mut()
                .filter(|arm| arm.nonce == nonce)
                .ok_or(FixtureError::Cancelled)?;
            if let Some(fault) = fault {
                arm.fault = Some(fault);
            } else {
                state.arm = None;
            }
            state.bump();
            state.wake.take()
        };
        self.inner.changed.notify_waiters();
        if let Some(wake) = wake {
            wake.wake();
        }
        Ok(())
    }
}
impl FixtureIoGate {
    /// # Errors
    /// Only an actual matching IO boundary produces an observation; cancellation leaves this gate owned.
    pub async fn arrived(&mut self) -> Result<FixtureIoObservation, FixtureError> {
        loop {
            let notified = self.control.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let observation = {
                let state = self.control.inner.lock();
                if let Some(error) = state.error {
                    return Err(error);
                }
                state
                    .arm
                    .as_ref()
                    .filter(|arm| arm.nonce == self.nonce)
                    .ok_or(FixtureError::Cancelled)?
                    .observed
            };
            if let Some(observation) = observation {
                return Ok(observation);
            }
            notified.await;
        }
    }
    /// # Errors
    /// A stopped resource or stale arm cannot affect another nonce.
    pub fn release(mut self) -> Result<(), FixtureError> {
        self.active = false;
        self.control.resolve(self.nonce, None)
    }
    /// # Errors
    /// Resolves only this arm; the error is returned by a later actual IO poll.
    pub fn fail(mut self, error: FixtureIoFault) -> Result<(), FixtureError> {
        self.active = false;
        self.control.resolve(self.nonce, Some(error))
    }
}
impl Drop for FixtureIoGate {
    fn drop(&mut self) {
        if self.active {
            let _ = self.control.resolve(self.nonce, None);
        }
    }
}
pub struct ControlledDuplex {
    stream: DuplexStream,
    control: FixtureIoController,
}
enum Decision {
    Forward { limit: usize, nonce: Option<u64> },
    Pending,
    Fail(io::ErrorKind),
}
impl ControlledDuplex {
    pub(super) const fn new(stream: DuplexStream, control: FixtureIoController) -> Self {
        Self { stream, control }
    }
    fn begin(&self, cx: &Context<'_>, length: Option<usize>) -> Decision {
        let replacement = cx.waker().clone();
        let (decision, discarded) = {
            let mut state = self.control.inner.lock();
            if let Some(error) = state.error {
                return Decision::Fail(if error == FixtureError::Io {
                    io::ErrorKind::Other
                } else {
                    io::ErrorKind::Interrupted
                });
            }
            if state.revision == u64::MAX
                || (length.is_some() && state.forwarded == u64::MAX)
                || (length.is_none() && state.flushes == u64::MAX)
            {
                state.error = Some(FixtureError::Io);
                let discarded = state.wake.take();
                (Decision::Fail(io::ErrorKind::Other), [discarded, None])
            } else {
                let discarded = state.wake.replace(replacement);
                let flushes = state.flushes;
                let counter_room =
                    usize::try_from(u64::MAX - state.forwarded).unwrap_or(usize::MAX);
                let mut limit = length.unwrap_or(0).min(counter_room);
                let mut boundary = None;
                if let Some(arm) = state.arm.as_mut() {
                    match (arm.plan, length) {
                        (FixtureIoPlan::NextFlush, None) => {
                            boundary = Some(FixtureIoBoundary::FlushPending);
                        }
                        (FixtureIoPlan::WritePrefix { forwarded }, Some(length)) if length != 0 => {
                            let remaining = u64::try_from(forwarded.get())
                                .unwrap_or(u64::MAX)
                                .saturating_sub(arm.forwarded);
                            if remaining == 0 {
                                boundary = Some(FixtureIoBoundary::WritePrefixPending);
                            } else {
                                limit = limit.min(usize::try_from(remaining).unwrap_or(usize::MAX));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(boundary) = boundary {
                    let arm = state.arm.as_mut();
                    let fault = arm.as_ref().and_then(|arm| arm.fault);
                    if let Some(fault) = fault {
                        state.arm = None;
                        state.bump();
                        (Decision::Fail(fault.kind()), [state.wake.take(), discarded])
                    } else {
                        if let Some(arm) = arm
                            && arm.observed.is_none()
                        {
                            arm.observed = Some(FixtureIoObservation {
                                boundary,
                                forwarded: arm.forwarded,
                                flushes,
                            });
                            state.bump();
                        }
                        (Decision::Pending, [discarded, None])
                    }
                } else {
                    state.polling = true;
                    (
                        Decision::Forward {
                            limit,
                            nonce: state.arm.as_ref().map(|arm| arm.nonce),
                        },
                        [discarded, None],
                    )
                }
            }
        };
        drop(discarded);
        self.control.inner.changed.notify_waiters();
        decision
    }
    fn finish(&self, nonce: Option<u64>, accepted: Option<usize>, flushed: bool, ready: bool) {
        let discarded = {
            let mut state = self.control.inner.lock();
            state.polling = false;
            if let Some(accepted) = accepted {
                let accepted = u64::try_from(accepted).unwrap_or(u64::MAX);
                if let Some(total) = state.forwarded.checked_add(accepted) {
                    state.forwarded = total;
                } else {
                    state.error = Some(FixtureError::Io);
                }
                if let Some(arm) = state.arm.as_mut().filter(|arm| Some(arm.nonce) == nonce) {
                    if let Some(total) = arm.forwarded.checked_add(accepted) {
                        arm.forwarded = total;
                    } else {
                        state.error = Some(FixtureError::Io);
                    }
                }
                state.bump();
            }
            if flushed {
                if let Some(total) = state.flushes.checked_add(1) {
                    state.flushes = total;
                } else {
                    state.error = Some(FixtureError::Io);
                }
                state.bump();
            }
            if ready || state.error.is_some() {
                state.wake.take()
            } else {
                None
            }
        };
        drop(discarded);
        self.control.inner.changed.notify_waiters();
    }
}
impl AsyncRead for ControlledDuplex {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buffer)
    }
}
impl AsyncWrite for ControlledDuplex {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.begin(cx, Some(bytes.len())) {
            Decision::Pending => Poll::Pending,
            Decision::Fail(error) => Poll::Ready(Err(error.into())),
            Decision::Forward { limit, nonce } => {
                let result = Pin::new(&mut self.stream).poll_write(cx, &bytes[..limit]);
                let accepted = match &result {
                    Poll::Ready(Ok(n)) => Some(*n),
                    _ => None,
                };
                self.finish(nonce, accepted, false, result.is_ready());
                result
            }
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.begin(cx, None) {
            Decision::Pending => Poll::Pending,
            Decision::Fail(error) => Poll::Ready(Err(error.into())),
            Decision::Forward { nonce, .. } => {
                let result = Pin::new(&mut self.stream).poll_flush(cx);
                self.finish(
                    nonce,
                    None,
                    matches!(result, Poll::Ready(Ok(()))),
                    result.is_ready(),
                );
                result
            }
        }
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
#[cfg(test)]
mod tests;
