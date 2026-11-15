//! Original handles stay in these slots even when a join waiter is cancelled.
use super::{
    Arc, Future, Mutex, Notify, Pin, PreparationFailure, PreparationTaskIdentity,
    PreparationTaskObservation, PreparationTaskPhase, State, TaskExit, TaskJoinState,
    lifecycle_lock,
};
use std::{sync::Condvar, task::Poll};
use tokio::task::JoinHandle;
pub(super) struct Installed {
    ready: Mutex<bool>,
    blocking: Condvar,
    changed: Notify,
}
impl Installed {
    fn new() -> Self {
        Self {
            ready: Mutex::new(false),
            blocking: Condvar::new(),
            changed: Notify::new(),
        }
    }
    fn open(&self) {
        *lifecycle_lock(&self.ready) = true;
        self.blocking.notify_all();
        self.changed.notify_waiters();
    }
    fn wait_blocking(&self) {
        let mut g = lifecycle_lock(&self.ready);
        while !*g {
            g = match self.blocking.wait(g) {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
        }
        drop(g);
    }
    async fn wait(&self) {
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if *lifecycle_lock(&self.ready) {
                return;
            }
            n.await;
        }
    }
}
struct SlotInner {
    handle: Option<JoinHandle<()>>,
    join: TaskJoinState,
}
pub(super) struct TaskSlot {
    inner: Mutex<SlotInner>,
    join_owner: tokio::sync::Mutex<()>,
    installed: Arc<Installed>,
    identity: PreparationTaskIdentity,
}
impl TaskSlot {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SlotInner {
                handle: None,
                join: TaskJoinState::NeverStarted,
            }),
            join_owner: tokio::sync::Mutex::new(()),
            installed: Arc::new(Installed::new()),
            identity: PreparationTaskIdentity::new(),
        })
    }
    pub(super) fn state(&self) -> TaskJoinState {
        lifecycle_lock(&self.inner).join
    }
    pub(super) fn observation(&self, phase: PreparationTaskPhase) -> PreparationTaskObservation {
        PreparationTaskObservation::from_owner_state(self.identity.clone(), phase, self.state())
    }
    fn install(&self, handle: JoinHandle<()>) {
        let mut s = lifecycle_lock(&self.inner);
        s.handle = Some(handle);
        s.join = TaskJoinState::Pending;
        drop(s);
        self.installed.open();
    }
    pub(super) async fn join(&self) -> TaskJoinState {
        let _owner = self.join_owner.lock().await;
        self.installed.wait().await;
        std::future::poll_fn(|cx| {
            let mut s = lifecycle_lock(&self.inner);
            if matches!(s.join, TaskJoinState::Joined(_)) {
                return Poll::Ready(s.join);
            }
            let Some(h) = s.handle.as_mut() else {
                return Poll::Pending;
            };
            match Pin::new(h).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(outcome) => {
                    let exit = if outcome.is_ok() {
                        TaskExit::Completed
                    } else {
                        TaskExit::Failed(PreparationFailure::WorkerFailed)
                    };
                    s.join = TaskJoinState::Joined(exit);
                    s.handle.take();
                    Poll::Ready(s.join)
                }
            }
        })
        .await
    }
    pub(super) fn spawn_blocking(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        work: impl FnOnce() + Send + 'static,
    ) {
        let gate = self.installed.clone();
        self.launch(|| {
            runtime.spawn_blocking(move || {
                gate.wait_blocking();
                work();
            })
        });
    }
    pub(super) fn spawn_supervisor(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        state: Arc<State>,
    ) {
        let gate = self.installed.clone();
        self.launch(|| {
            runtime.spawn(async move {
                gate.wait().await;
                state.supervise().await;
            })
        });
    }
    fn launch(self: &Arc<Self>, spawn: impl FnOnce() -> JoinHandle<()>) {
        if let Ok(handle) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(spawn)) {
            let guard = InstallationGuard {
                slot: self.clone(),
                handle: Some(handle),
            };
            drop(guard);
        } else {
            lifecycle_lock(&self.inner).join = TaskJoinState::StartUnconfirmed;
            self.installed.open();
        }
    }
}
struct InstallationGuard {
    slot: Arc<TaskSlot>,
    handle: Option<JoinHandle<()>>,
}
impl Drop for InstallationGuard {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.slot.install(h);
        }
    }
}
