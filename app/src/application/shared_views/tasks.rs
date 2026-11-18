//! Registered view tasks and blocking destruction, with cancel-safe joins.
use crate::domain::{native::NativeSessionKey, shared_view::ViewError};
use std::{
    future::Future,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{Mutex as AsyncMutex, oneshot, watch},
    task::JoinHandle,
    time::{Instant, timeout_at},
};
#[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
#[path = "capacity_registration_race_hook.rs"]
pub mod capacity_registration_race_hook;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ViewTaskKind {
    Worker,
    Reader,
    Cleanup,
    NativeRead,
}
struct TaskSlot {
    kind: ViewTaskKind,
    handle: AsyncMutex<Option<JoinHandle<Result<(), ViewError>>>>,
    result: Mutex<Option<Result<(), ViewError>>>,
    joined: AtomicBool,
    cleanup: AtomicBool,
    started: AtomicBool,
    finished: AtomicBool,
    unconfirmed: AtomicBool,
}
struct State {
    runtime: Handle,
    stop: watch::Sender<bool>,
    tasks: Mutex<Vec<Arc<TaskSlot>>>,
    target_owner: OnceLock<Arc<dyn TargetTaskOwnership>>,
    #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
    capacity_registration_gate:
        Mutex<Option<Arc<capacity_registration_race_hook::RegistrationRaceGate>>>,
}
pub trait TargetTaskOwnership: Send + Sync {
    fn adopt(
        &self,
        target: &NativeSessionKey,
        generation: super::send_epoch::OwnedDisplaySendView,
        joined: super::BoxReadFuture<'static, Result<(), ViewError>>,
    ) -> Result<(), ViewError>;
}
#[derive(Clone)]
pub struct ViewTaskSupervisor {
    state: Arc<State>,
}
pub struct ViewTaskContext {
    state: Arc<State>,
    slot: Arc<TaskSlot>,
}
#[derive(Clone, Copy)]
pub struct SupervisorReport {
    pub workers: usize,
    pub readers: usize,
    pub cleanup: usize,
    pub native_reads: usize,
    pub failure: Option<ViewError>,
}
impl ViewTaskSupervisor {
    pub(crate) fn new(runtime: Handle) -> Self {
        let (stop, _) = watch::channel(false);
        Self {
            state: Arc::new(State {
                runtime,
                stop,
                tasks: Mutex::new(Vec::new()),
                target_owner: OnceLock::new(),
                #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
                capacity_registration_gate: Mutex::new(None),
            }),
        }
    }
    pub(crate) fn subscribe_stop(&self) -> watch::Receiver<bool> {
        self.state.stop.subscribe()
    }
    pub(crate) fn request_stop(&self) {
        self.state.stop.send_replace(true);
    }
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn spawn<F>(&self, kind: ViewTaskKind, work: F) -> Result<(), ViewError>
    where
        F: Future<Output = Result<(), ViewError>> + Send + 'static,
    {
        self.spawn_owned(kind, |_| work)
    }
    pub(crate) fn spawn_owned<F, M>(&self, kind: ViewTaskKind, make: M) -> Result<(), ViewError>
    where
        F: Future<Output = Result<(), ViewError>> + Send + 'static,
        M: FnOnce(ViewTaskContext) -> F + Send + 'static,
    {
        self.spawn_registered(kind, None, make)
    }
    pub(crate) fn install_target_owner(
        &self,
        owner: Arc<dyn TargetTaskOwnership>,
    ) -> Result<(), ViewError> {
        self.state
            .target_owner
            .set(owner)
            .map_err(|_| ViewError::AlreadyAttached)
    }
    pub(crate) fn spawn_target<F, M>(
        &self,
        kind: ViewTaskKind,
        target: &NativeSessionKey,
        generation: super::send_epoch::OwnedDisplaySendView,
        make: M,
    ) -> Result<(), ViewError>
    where
        F: Future<Output = Result<(), ViewError>> + Send + 'static,
        M: FnOnce(ViewTaskContext) -> F + Send + 'static,
    {
        self.spawn_registered(kind, Some((target, generation)), make)
    }
    fn spawn_registered<F, M>(
        &self,
        kind: ViewTaskKind,
        target: Option<(&NativeSessionKey, super::send_epoch::OwnedDisplaySendView)>,
        make: M,
    ) -> Result<(), ViewError>
    where
        F: Future<Output = Result<(), ViewError>> + Send + 'static,
        M: FnOnce(ViewTaskContext) -> F + Send + 'static,
    {
        let mut slots = self
            .state
            .tasks
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if *self.state.stop.borrow() && kind != ViewTaskKind::Cleanup {
            return Err(ViewError::NodeStopping);
        }
        slots
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let (start, ready) = oneshot::channel();
        let slot = Arc::new(TaskSlot {
            kind,
            handle: AsyncMutex::new(None),
            result: Mutex::new(None),
            joined: AtomicBool::new(false),
            cleanup: AtomicBool::new(false),
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            unconfirmed: AtomicBool::new(false),
        });
        let context = ViewTaskContext {
            state: Arc::clone(&self.state),
            slot: Arc::clone(&slot),
        };
        let observed = Arc::clone(&slot);
        let task = self.state.runtime.spawn(async move {
            if ready.await.is_err() {
                return Ok(());
            }
            observed.started.store(true, Ordering::Release);
            let result = make(context).await;
            observed.finished.store(true, Ordering::Release);
            result
        });
        *slot.handle.try_lock().map_err(|_| ViewError::ReadFailure)? = Some(task);
        slots.push(slot.clone());
        drop(slots);
        #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
        let capacity_race_target = target.as_ref().map(|(target, _)| (*target).clone());
        #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
        if let Some(target) = &capacity_race_target {
            self.capacity_registration_cut(
                capacity_registration_race_hook::RegistrationCut::BeforeAdopt,
                target,
            );
        }
        if let (Some(owner), Some((target, generation))) = (self.state.target_owner.get(), target) {
            owner.adopt(
                target,
                generation,
                Box::pin(async move { slot.join_original().await }),
            )?;
        }
        #[cfg(all(test, feature = "native-dispatch-dev-fixtures"))]
        if let Some(target) = &capacity_race_target {
            self.capacity_registration_cut(
                capacity_registration_race_hook::RegistrationCut::AfterAdoptBeforeStart,
                target,
            );
        }
        let _ = start.send(());
        Ok(())
    }
    pub(crate) async fn join_until(&self, deadline: Instant) -> SupervisorReport {
        self.join_owned(Some(deadline)).await
    }
    pub(crate) async fn join_stopped(&self) -> SupervisorReport {
        self.join_owned(None).await
    }
    async fn join_owned(&self, deadline: Option<Instant>) -> SupervisorReport {
        let mut failure = None;
        loop {
            let slots = match self.state.tasks.lock() {
                Ok(slots) => slots.clone(),
                Err(_) => {
                    return SupervisorReport {
                        workers: 1,
                        readers: 1,
                        cleanup: 1,
                        native_reads: 0,
                        failure: Some(ViewError::ReadFailure),
                    };
                }
            };
            let mut timed_out = false;
            for slot in &slots {
                let join = slot.join_original();
                let result = match deadline {
                    Some(deadline) => timeout_at(deadline, join).await.map_err(|_| ()),
                    None => Ok(join.await),
                };
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        failure.get_or_insert(error);
                    }
                    Err(()) => {
                        slot.unconfirmed.store(true, Ordering::Release);
                        timed_out = true;
                        failure.get_or_insert(ViewError::ReadFailure);
                    }
                }
            }
            let all = match self.state.tasks.lock() {
                Ok(slots) => slots.clone(),
                Err(_) => {
                    return SupervisorReport {
                        workers: 1,
                        readers: 1,
                        cleanup: 1,
                        native_reads: 0,
                        failure: Some(ViewError::ReadFailure),
                    };
                }
            };
            let mut report = SupervisorReport {
                workers: 0,
                readers: 0,
                cleanup: 0,
                native_reads: 0,
                failure,
            };
            for slot in &all {
                if !slot.joined.load(Ordering::Acquire) {
                    match slot.kind {
                        ViewTaskKind::Worker => report.workers += 1,
                        ViewTaskKind::Reader => report.readers += 1,
                        ViewTaskKind::Cleanup => report.cleanup += 1,
                        ViewTaskKind::NativeRead => report.native_reads += 1,
                    }
                    if slot.cleanup.load(Ordering::Acquire) && slot.kind != ViewTaskKind::Cleanup {
                        report.cleanup += 1;
                    }
                }
            }
            if timed_out || all.len() == slots.len() {
                return report;
            }
        }
    }
}
impl TaskSlot {
    async fn join_original(&self) -> Result<(), ViewError> {
        let mut handle = self.handle.lock().await;
        if self.joined.load(Ordering::Acquire) {
            return self
                .result
                .lock()
                .map_err(|_| ViewError::ReadFailure)?
                .unwrap_or(Err(ViewError::ReadFailure));
        }
        let task = handle.as_mut().ok_or(ViewError::ReadFailure)?;
        let result = task.await.unwrap_or(Err(ViewError::ReadFailure));
        handle.take();
        *self.result.lock().map_err(|_| ViewError::ReadFailure)? = Some(result);
        self.joined.store(true, Ordering::Release);
        drop(handle);
        result
    }
}
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
impl ViewTaskSupervisor {
    pub(crate) fn task_state(&self, kind: ViewTaskKind) -> super::probe::ViewTaskState {
        use super::probe::ViewTaskState;
        let Ok(slots) = self.state.tasks.lock() else {
            return ViewTaskState::JoinUnconfirmed;
        };
        let mut any = false;
        let mut all_joined = true;
        let mut unconfirmed = false;
        let mut sealing = false;
        let mut started = false;
        for slot in slots.iter().filter(|slot| slot.kind == kind) {
            any = true;
            all_joined &= slot.joined.load(Ordering::Acquire);
            unconfirmed |= slot.unconfirmed.load(Ordering::Acquire);
            sealing |=
                slot.finished.load(Ordering::Acquire) || slot.cleanup.load(Ordering::Acquire);
            started |= slot.started.load(Ordering::Acquire);
        }
        if !any {
            return ViewTaskState::Registered;
        }
        if all_joined {
            return ViewTaskState::Joined;
        }
        if unconfirmed {
            return ViewTaskState::JoinUnconfirmed;
        }
        if sealing {
            return ViewTaskState::Sealing;
        }
        if !started {
            return ViewTaskState::Registered;
        }
        ViewTaskState::Running
    }
}
impl ViewTaskContext {
    /// Runs destruction under the already registered task slot, never a detached closer.
    pub(crate) async fn retire<T: Send + 'static>(&self, value: T) -> Result<(), ViewError> {
        self.slot.cleanup.store(true, Ordering::Release);
        let result = self
            .state
            .runtime
            .spawn_blocking(move || drop(value))
            .await
            .map_err(|_| ViewError::ReadFailure);
        self.slot.cleanup.store(false, Ordering::Release);
        result
    }
}
