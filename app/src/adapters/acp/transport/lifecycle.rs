//! Startup barrier, task ownership and cancel-safe close/join. No protocol decisions here.
use super::{
    AcpError, Arc, AsyncRead, AsyncWrite, Connection, ConnectionScope, EventReceiver, Instant,
    IoOwner, IoShutdownReport, WriteProbe, reader,
    state::{Core, State, TaskSlot},
    writer,
};
use futures_util::FutureExt;
use std::{panic::AssertUnwindSafe, sync::Mutex};
use tokio::sync::{Mutex as AsyncMutex, Notify};
fn tasks(core: &Core) -> std::sync::MutexGuard<'_, Vec<Arc<TaskSlot>>> {
    match core.io_tasks.lock() {
        Ok(v) => v,
        Err(p) => p.into_inner(),
    }
}
fn collector(core: &Core) -> std::sync::MutexGuard<'_, Option<Arc<TaskSlot>>> {
    match core.collector.lock() {
        Ok(v) => v,
        Err(p) => p.into_inner(),
    }
}
async fn started(core: &Core) -> bool {
    loop {
        let n = core.notify.notified();
        tokio::pin!(n);
        n.as_mut().enable();
        {
            let s = core.lock();
            if s.started {
                return true;
            }
            if s.stopped {
                return false;
            }
        }
        n.await;
    }
}
pub(super) fn prepare<R, W, A>(
    reader: R,
    writer: W,
    observed: bool,
    adopt: A,
    #[cfg(test)] hooks: Option<Arc<super::AcpObservationHooks>>,
) -> Result<Arc<Core>, super::ObservedPrepareError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    A: FnOnce(IoOwner) -> Result<(), super::IoAdoptionError>,
{
    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| super::ObservedPrepareError::before(AcpError::RuntimeUnavailable))?;
    let notify = Arc::new(Notify::new());
    let core = Arc::new(Core {
        scope: ConnectionScope(Arc::new(())),
        state: Mutex::new(State::new(notify.clone())),
        notify,
        events_notify: Notify::new(),
        io_tasks: Mutex::new(Vec::new()),
        collector: Mutex::new(None),
        shutdown: AsyncMutex::new(()),
        installation_complete: std::sync::atomic::AtomicBool::new(false),
        installation_notify: Notify::new(),
        #[cfg(test)]
        hooks,
        #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
        capacity_close_fault: Mutex::new(None),
        #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
        capacity_resume_fault: Mutex::new(None),
    });
    core.lock().observed = observed;
    tasks(&core)
        .try_reserve(2)
        .map_err(|_| super::ObservedPrepareError::before(AcpError::Allocation))?;
    let installation = Installation(core.clone());
    if let Err(rejected) = adopt(IoOwner { core: core.clone() }) {
        let (error, owner) = rejected.into_parts();
        core.stop(Some(error));
        drop(owner);
        return Err(super::ObservedPrepareError::before(error));
    }
    let r = core.clone();
    let task = runtime.spawn(async move {
        let work = async {
            if started(&r).await {
                reader::run(r.clone(), reader).await;
            }
        };
        if AssertUnwindSafe(work).catch_unwind().await.is_err() {
            r.stop(Some(AcpError::Io));
        }
    });
    tasks(&core).push(Arc::new(TaskSlot {
        handle: AsyncMutex::new(Some(task)),
    }));
    let w = core.clone();
    let task = runtime.spawn(async move {
        let work = async {
            if started(&w).await {
                writer::run(w.clone(), writer).await;
            }
        };
        if AssertUnwindSafe(work).catch_unwind().await.is_err() {
            w.stop(Some(AcpError::Io));
        }
    });
    tasks(&core).push(Arc::new(TaskSlot {
        handle: AsyncMutex::new(Some(task)),
    }));
    let c = core.clone();
    let task = runtime.spawn(async move {
        if !started(&c).await {
            return;
        }
        drain_io(&c).await;
        publish_terminal(&c);
    });
    *collector(&core) = Some(Arc::new(TaskSlot {
        handle: AsyncMutex::new(Some(task)),
    }));
    drop(installation);
    Ok(core)
}
struct Installation(Arc<Core>);
impl Drop for Installation {
    fn drop(&mut self) {
        self.0
            .installation_complete
            .store(true, std::sync::atomic::Ordering::Release);
        if std::thread::panicking() {
            self.0.stop(Some(AcpError::Io));
        }
        self.0.installation_notify.notify_waiters();
    }
}
pub(super) fn spawn<R, W>(
    reader: R,
    writer: W,
) -> Result<(Connection, EventReceiver, IoOwner), AcpError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut owner = None;
    let core = prepare(
        reader,
        writer,
        false,
        |value| {
            owner = Some(value);
            Ok(())
        },
        #[cfg(test)]
        None,
    )
    .map_err(|e| e.error())?;
    let owner = owner.ok_or(AcpError::Io)?;
    core.lock().started = true;
    core.notify.notify_waiters();
    Ok((
        Connection { core: core.clone() },
        EventReceiver { core, ended: false },
        owner,
    ))
}

pub(super) async fn shutdown(core: &Arc<Core>, deadline: Instant) -> IoShutdownReport {
    core.stop(None);
    let joined = tokio::time::timeout_at(deadline, async {
        loop {
            let ready = core.installation_notify.notified();
            tokio::pin!(ready);
            ready.as_mut().enable();
            if core
                .installation_complete
                .load(std::sync::atomic::Ordering::Acquire)
            {
                break;
            }
            ready.await;
        }
        let _serial = core.shutdown.lock().await;
        let record = collector(core).clone();
        if let Some(record) = record {
            let mut slot = record.handle.lock().await;
            if let Some(handle) = slot.as_mut() {
                if handle.await.is_err() {
                    core.stop(Some(AcpError::Io));
                }
                slot.take();
            }
        }
        drain_io(core).await;
        publish_terminal(core);
    })
    .await
    .is_ok();
    let s = core.lock();
    let outstanding = s
        .cells
        .values()
        .filter_map(std::sync::Weak::upgrade)
        .map(|cell| WriteProbe { cell })
        .collect();
    let unresolved_requests = s.pending.keys().cloned().collect();
    IoShutdownReport {
        joined,
        outstanding,
        unresolved_requests,
        error: s.error,
    }
}

async fn drain_io(core: &Core) {
    let records = tasks(core).clone();
    for record in records {
        let mut slot = record.handle.lock().await;
        if let Some(handle) = slot.as_mut() {
            if handle.await.is_err() {
                core.stop(Some(AcpError::Io));
            }
            slot.take();
        }
    }
    tasks(core).clear();
}
fn publish_terminal(core: &Core) {
    core.lock().terminal = true;
    core.events_notify.notify_waiters();
}
