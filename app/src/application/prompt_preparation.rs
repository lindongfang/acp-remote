//! Internal lifecycle metadata; no prompt or authorization data crosses this port.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::time::Instant;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationFailure {
    MalformedInput,
    Allocation,
    Unavailable,
    Stopping,
    WorkerFailed,
    OwnerUnavailable,
    ResultAlreadyClaimed,
    Validation(super::request_registry::RegistryError),
    StartUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskExit {
    Completed,
    Failed(PreparationFailure),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskJoinState {
    NeverStarted,
    Pending,
    Joined(TaskExit),
    StartUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationTaskPhase {
    Registered,
    CpuEntered,
    ParseReturned,
    HashComputed,
    Disposing,
    Terminal,
}
#[derive(Clone)]
pub struct PreparationTaskIdentity(Arc<()>);
impl PreparationTaskIdentity {
    pub(crate) fn new() -> Self {
        Self(Arc::new(()))
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl std::fmt::Debug for PreparationTaskIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparationTaskIdentity([opaque])")
    }
}
#[derive(Clone, Debug)]
pub struct PreparationTaskObservation {
    identity: PreparationTaskIdentity,
    phase: PreparationTaskPhase,
    join: TaskJoinState,
}
impl PreparationTaskObservation {
    pub(crate) const fn from_owner_state(
        identity: PreparationTaskIdentity,
        phase: PreparationTaskPhase,
        join: TaskJoinState,
    ) -> Self {
        Self {
            identity,
            phase,
            join,
        }
    }
    pub(crate) const fn identity(&self) -> &PreparationTaskIdentity {
        &self.identity
    }
    pub(crate) const fn phase(&self) -> PreparationTaskPhase {
        self.phase
    }
    pub(crate) const fn join(&self) -> TaskJoinState {
        self.join
    }
}
#[derive(Clone, Debug)]
pub struct PreparationShutdownObservation {
    sealed: bool,
    cpu: Vec<PreparationTaskObservation>,
    disposal: Vec<PreparationTaskObservation>,
    active_use: usize,
    supervisor: TaskJoinState,
    failures: Vec<PreparationFailure>,
}
impl PreparationShutdownObservation {
    pub(crate) const fn from_owner_state(
        sealed: bool,
        cpu: Vec<PreparationTaskObservation>,
        disposal: Vec<PreparationTaskObservation>,
        active_use: usize,
        supervisor: TaskJoinState,
        failures: Vec<PreparationFailure>,
    ) -> Self {
        Self {
            sealed,
            cpu,
            disposal,
            active_use,
            supervisor,
            failures,
        }
    }
    pub(crate) const fn sealed(&self) -> bool {
        self.sealed
    }
    pub(crate) const fn cpu(&self) -> &[PreparationTaskObservation] {
        self.cpu.as_slice()
    }
    pub(crate) const fn disposal(&self) -> &[PreparationTaskObservation] {
        self.disposal.as_slice()
    }
    pub(crate) const fn active_use(&self) -> usize {
        self.active_use
    }
    pub(crate) const fn supervisor(&self) -> TaskJoinState {
        self.supervisor
    }
    pub(crate) const fn failures(&self) -> &[PreparationFailure] {
        self.failures.as_slice()
    }
    pub(crate) fn all_joined(&self) -> bool {
        self.sealed
            && self.active_use == 0
            && matches!(self.supervisor, TaskJoinState::Joined(_))
            && self
                .cpu
                .iter()
                .chain(&self.disposal)
                .all(|t| matches!(t.join, TaskJoinState::Joined(_)))
            && self
                .cpu
                .iter()
                .all(|t| t.phase == PreparationTaskPhase::Terminal)
    }
}
#[derive(Clone, Debug)]
pub struct PreparationShutdownReport {
    observation: PreparationShutdownObservation,
}
impl PreparationShutdownReport {
    pub(crate) fn from_owner_observation(
        observation: PreparationShutdownObservation,
    ) -> Option<Self> {
        if observation.all_joined() {
            Some(Self { observation })
        } else {
            None
        }
    }
    pub(crate) const fn observation(&self) -> &PreparationShutdownObservation {
        &self.observation
    }
}
#[derive(Clone, Debug)]
pub enum PreparationCloseWait {
    Joined(PreparationShutdownReport),
    WaitExpired(PreparationShutdownObservation),
}
pub trait PromptPreparationShutdownPort: Send + Sync {
    fn seal(&self);
    fn observe(&self) -> PreparationShutdownObservation;
    fn join(&self) -> Pin<Box<dyn Future<Output = PreparationShutdownReport> + Send + '_>>;
    fn wait_until(
        &self,
        deadline: Instant,
    ) -> Pin<Box<dyn Future<Output = PreparationCloseWait> + Send + '_>> {
        Box::pin(async move {
            tokio::time::timeout_at(deadline, self.join())
                .await
                .map_or_else(
                    |_| PreparationCloseWait::WaitExpired(self.observe()),
                    PreparationCloseWait::Joined,
                )
        })
    }
}
pub fn lifecycle_lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}
#[derive(Default)]
pub struct PreparationShutdownSlot(Mutex<Option<Arc<dyn PromptPreparationShutdownPort>>>);
impl PreparationShutdownSlot {
    pub(crate) fn get(&self) -> Option<Arc<dyn PromptPreparationShutdownPort>> {
        lifecycle_lock(&self.0).clone()
    }
    pub(crate) fn install(
        &self,
        receipt: Arc<dyn PromptPreparationShutdownPort>,
    ) -> Result<(), super::request_registry::RegistryError> {
        let mut s = lifecycle_lock(&self.0);
        if s.is_some() {
            return Err(super::request_registry::RegistryError::RuntimeAlreadyStarted);
        }
        *s = Some(receipt);
        drop(s);
        Ok(())
    }
    pub(crate) fn seal(&self) {
        if let Some(r) = self.get() {
            r.seal();
        }
    }
    pub(crate) async fn join(&self) {
        if let Some(r) = self.get() {
            let _ = r.join().await;
        }
    }
}
