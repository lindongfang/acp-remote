//! The only writer. `PollIo` records the actual provider result under the protocol short lock.
use super::{
    AcpError, Arc, AsyncWrite, Context, Instant, IoProgress, OutboundKind, PollIo, RequestKind,
    WriteCell, WriteFailure, WriteProgress, WriteReport, WriteStep, ZeroReason,
    state::{Command, Core, accepted_count},
};
use std::{future::poll_fn, pin::Pin, task::Poll};

fn control(c: &Command) -> bool {
    !matches!(c.cell.identity.kind(),OutboundKind::Request(k) if !matches!(k,RequestKind::Close(_)))
}
struct Proxy<'a, W> {
    core: &'a Arc<Core>,
    command: &'a Command,
    writer: &'a mut W,
    step: WriteStep,
    calls: usize,
    actual: bool,
    outcome: Option<ProxyOutcome>,
}
#[derive(Clone, Copy)]
enum ProxyOutcome {
    ApplicationParked,
    ProtocolWait,
    Stopped,
    Provider(Poll<Result<IoProgress, AcpError>>),
}
impl<W: AsyncWrite + Unpin> PollIo for Proxy<'_, W> {
    fn park(&mut self) -> Result<(), AcpError> {
        self.calls += 1;
        if self.calls != 1 {
            return Err(AcpError::InvalidFence);
        }
        self.outcome = Some(ProxyOutcome::ApplicationParked);
        Ok(())
    }
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<IoProgress, AcpError>> {
        self.calls += 1;
        if self.calls != 1 {
            return Poll::Ready(Err(AcpError::InvalidFence));
        }
        let mut s = self.core.lock();
        s.expire(Instant::now());
        let cell = &self.command.cell;
        if s.stopped || matches!(cell.lock().progress, WriteProgress::Finished(_)) {
            let out = Poll::Ready(Err(AcpError::Closed));
            self.outcome = Some(ProxyOutcome::Stopped);
            return out;
        }
        if cell.lock().cancel.is_some()
            || self.command.deadline <= Instant::now()
            || s.expiry_waiting.contains(&cell.key)
        {
            let out = Poll::Pending;
            self.outcome = Some(ProxyOutcome::ProtocolWait);
            return out;
        }
        if s.observed
            && (matches!(
                cell.identity.kind(),
                OutboundKind::Request(RequestKind::Prompt(_))
            ) || (self.command.automatic_close
                && matches!(self.step, WriteStep::Chunk { offset: 0, .. })))
            && (s.acknowledged != s.sequence || s.held.as_ref().is_some_and(|h| h.remaining != 0))
        {
            let out = Poll::Pending;
            self.outcome = Some(ProxyOutcome::ProtocolWait);
            return out;
        }
        cell.lock().in_poll = true;
        self.actual = true;
        #[cfg(test)]
        if let Some(hooks) = &core_hooks(self.core) {
            hooks.writer_polled();
        }
        let out = match self.step {
            WriteStep::Chunk { offset, total } => {
                match poll_write(self.writer, self.command, cx, offset, total) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(_)) => Poll::Ready(Err(AcpError::Io)),
                    Poll::Ready(Ok(n)) => {
                        if n == 0 || n > total - offset {
                            Poll::Ready(Err(AcpError::Io))
                        } else {
                            let accepted = offset + n;
                            #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
                            if let Some(fault) = &self.command.capacity_close_fault {
                                fault.accepted(accepted);
                            }
                            cell.lock().progress = if accepted == total {
                                WriteProgress::Flushing { total }
                            } else {
                                WriteProgress::Writing { accepted, total }
                            };
                            s.spent(cell);
                            if accepted == total {
                                s.accepted(cell);
                            }
                            Poll::Ready(Ok(IoProgress::Accepted(n)))
                        }
                    }
                }
            }
            WriteStep::Flush { .. } => poll_flush(self.writer, self.command, cx)
                .map(|r| r.map(|()| IoProgress::Flushed).map_err(|_| AcpError::Io)),
        };
        cell.lock().in_poll = false;
        match out {
            Poll::Ready(Err(_)) => {
                let accepted = accepted_count(cell.lock().progress);
                s.spent(cell);
                s.finish(
                    cell,
                    WriteReport::Unknown {
                        accepted,
                        cause: WriteFailure::Io,
                    },
                );
                s.stop(Some(AcpError::Io));
                self.core.notify.notify_waiters();
            }
            Poll::Ready(Ok(IoProgress::Flushed)) => {
                s.finish(cell, WriteReport::Written);
            }
            _ => {}
        }
        self.outcome = Some(ProxyOutcome::Provider(out));
        out
    }
}
enum StepResult {
    Done,
    Park,
}
fn poll_write<W: AsyncWrite + Unpin>(
    writer: &mut W,
    command: &Command,
    cx: &mut Context<'_>,
    offset: usize,
    total: usize,
) -> Poll<Result<usize, AcpError>> {
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    let total = if let Some(fault) = &command.capacity_close_fault {
        match fault.write_end(offset, total) {
            Ok(end) => end,
            Err(error) => return Poll::Ready(Err(error)),
        }
    } else {
        total
    };
    Pin::new(writer)
        .poll_write(cx, &command.bytes[offset..total])
        .map(|result| result.map_err(|_| AcpError::Io))
}

fn poll_flush<W: AsyncWrite + Unpin>(
    writer: &mut W,
    command: &Command,
    cx: &mut Context<'_>,
) -> Poll<Result<(), AcpError>> {
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    if command
        .capacity_close_fault
        .as_ref()
        .is_some_and(|fault| fault.fail_flush())
    {
        return Poll::Ready(Err(AcpError::Io));
    }
    #[cfg(not(all(test, feature = "native-dispatch-dev-fixtures")))]
    let _ = command;
    Pin::new(writer)
        .poll_flush(cx)
        .map(|result| result.map_err(|_| AcpError::Io))
}
fn failure(
    core: &Core,
    cell: &Arc<WriteCell>,
    cause: WriteFailure,
    error: AcpError,
    known_zero: bool,
    zero: ZeroReason,
) {
    let mut s = core.lock();
    if matches!(cell.lock().progress, WriteProgress::Finished(_)) {
        return;
    }
    let accepted = accepted_count(cell.lock().progress);
    if accepted == 0 && known_zero {
        s.finish_zero(cell, zero);
    } else {
        s.spent(cell);
        s.finish(cell, WriteReport::Unknown { accepted, cause });
        s.stop(Some(error));
    }
    core.notify.notify_waiters();
}
#[allow(
    clippy::too_many_lines,
    reason = "One writer attempt keeps actual poll, fence verification and cancellation ordering together"
)]
async fn write<W: AsyncWrite + Unpin>(
    core: &Arc<Core>,
    writer: &mut W,
    command: &Command,
) -> StepResult {
    loop {
        let n = core.notify.notified();
        tokio::pin!(n);
        n.as_mut().enable();
        let attempt = poll_fn(|cx| {
            let step = {
                let mut s = core.lock();
                s.expire(Instant::now());
                let c = command.cell.lock();
                let progress = c.progress;
                let cancel = c.cancel;
                drop(c);
                if matches!(progress, WriteProgress::Finished(_)) {
                    return Poll::Ready(StepResult::Done);
                }
                let accepted = accepted_count(progress);
                if let Some(reason) = cancel {
                    drop(s);
                    failure(
                        core,
                        &command.cell,
                        WriteFailure::Cancelled,
                        AcpError::Cancelled,
                        true,
                        reason,
                    );
                    return Poll::Ready(StepResult::Done);
                }
                if command.deadline <= Instant::now() {
                    drop(s);
                    failure(
                        core,
                        &command.cell,
                        WriteFailure::Deadline,
                        AcpError::Deadline,
                        true,
                        ZeroReason::Deadline,
                    );
                    return Poll::Ready(StepResult::Done);
                }
                if s.stopped {
                    return Poll::Ready(StepResult::Done);
                }
                if s.expiry_waiting.contains(&command.cell.key) {
                    return Poll::Pending;
                }
                if accepted == 0 && !control(command) && s.queue.iter().any(control) {
                    return Poll::Ready(StepResult::Park);
                }
                if let WriteProgress::Flushing { total } = progress {
                    WriteStep::Flush { total }
                } else {
                    command.cell.lock().progress = WriteProgress::Writing {
                        accepted,
                        total: command.bytes.len(),
                    };
                    WriteStep::Chunk {
                        offset: accepted,
                        total: command.bytes.len(),
                    }
                }
            };
            let mut io = Proxy {
                core,
                command,
                writer,
                step,
                calls: 0,
                actual: false,
                outcome: None,
            };
            #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
            let reject_test_fault = command
                .capacity_close_fault
                .as_ref()
                .is_some_and(|fault| fault.reject_before_poll(step));
            #[cfg(not(all(test, feature = "native-dispatch-dev-fixtures")))]
            let reject_test_fault = false;
            let fence = if reject_test_fault {
                Err(super::FenceRejection::RequiredStateUnavailable)
            } else {
                command
                    .fence
                    .check_and_poll(&command.cell.identity, step, cx, &mut io)
            };
            let invalid = io.calls > 1
                || (fence.is_ok() && io.calls != 1)
                || (fence.is_err() && io.calls != 0);
            let actual = io.actual;
            let outcome = io.outcome;

            if invalid {
                failure(
                    core,
                    &command.cell,
                    WriteFailure::InvalidFence,
                    AcpError::InvalidFence,
                    !actual || matches!(outcome, Some(ProxyOutcome::Provider(Poll::Pending))),
                    ZeroReason::FenceRejected,
                );
                core.stop(Some(AcpError::InvalidFence));
                return Poll::Ready(StepResult::Done);
            }
            if fence.is_err() {
                failure(
                    core,
                    &command.cell,
                    WriteFailure::FenceRejected,
                    AcpError::Cancelled,
                    true,
                    ZeroReason::FenceRejected,
                );
                return Poll::Ready(StepResult::Done);
            }
            match outcome {
                Some(
                    ProxyOutcome::ApplicationParked
                    | ProxyOutcome::ProtocolWait
                    | ProxyOutcome::Provider(Poll::Pending),
                ) => Poll::Pending,
                Some(ProxyOutcome::Provider(Poll::Ready(Ok(IoProgress::Accepted(_))))) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Some(ProxyOutcome::Provider(Poll::Ready(Ok(IoProgress::Flushed)))) => {
                    let mut s = core.lock();
                    s.finish(&command.cell, WriteReport::Written);
                    Poll::Ready(StepResult::Done)
                }
                Some(ProxyOutcome::Stopped) => {
                    if matches!(command.cell.lock().progress, WriteProgress::Finished(_)) {
                        Poll::Ready(StepResult::Done)
                    } else {
                        Poll::Pending
                    }
                }
                Some(ProxyOutcome::Provider(Poll::Ready(Err(e)))) => {
                    failure(
                        core,
                        &command.cell,
                        WriteFailure::Io,
                        e,
                        false,
                        ZeroReason::ConnectionClosed,
                    );
                    Poll::Ready(StepResult::Done)
                }
                None => {
                    core.stop(Some(AcpError::InvalidFence));
                    Poll::Ready(StepResult::Done)
                }
            }
        });
        tokio::pin!(attempt);
        let deadline = tokio::time::sleep_until(command.deadline);
        tokio::pin!(deadline);
        tokio::select! {biased;result=&mut attempt=>return result,()=&mut n=>{},()=&mut deadline=>{}}
    }
}
pub(super) async fn run<W: AsyncWrite + Unpin>(core: Arc<Core>, mut writer: W) {
    loop {
        let n = core.notify.notified();
        tokio::pin!(n);
        n.as_mut().enable();
        let command = {
            let mut s = core.lock();
            s.expire(Instant::now());
            if s.stopped {
                break;
            }
            let pos = s.queue.iter().position(control).unwrap_or(0);
            s.queue.remove(pos)
        };
        if let Some(command) = command {
            if matches!(write(&core, &mut writer, &command).await, StepResult::Park) {
                core.lock().queue.push_front(command);
            }
            continue;
        }
        n.await;
    }
}

#[cfg(test)]
fn core_hooks(core: &Core) -> Option<Arc<super::AcpObservationHooks>> {
    core.hooks.clone()
}
