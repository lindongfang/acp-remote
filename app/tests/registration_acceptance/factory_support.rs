//! Shared test-only decorator; parent supplies either public-crate or same-crate port imports.
use super::{
    BeginRecord, ConsumeLease, CurrentRequestActor, EntityId, OperationContext, OperationProbe,
    QueueOutcome, RegisteredIntent, RegistrationFact, RegistrationId, RegistryDeadline,
    RegistryError, RequestFact, RequestRegistryRepository, RuntimeEpoch,
};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::Notify;

pub struct Gate {
    reached: AtomicBool,
    released: AtomicBool,
    arrived: Notify,
    resume: Notify,
}
impl Gate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: AtomicBool::new(false),
            released: AtomicBool::new(false),
            arrived: Notify::new(),
            resume: Notify::new(),
        })
    }
    pub async fn pause(&self) {
        self.reached.store(true, Ordering::SeqCst);
        self.arrived.notify_one();
        while !self.released.load(Ordering::SeqCst) {
            self.resume.notified().await;
        }
    }
    pub async fn arrived(&self) {
        while !self.reached.load(Ordering::SeqCst) {
            self.arrived.notified().await;
        }
    }
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.resume.notify_one();
    }
}
pub enum Stage {
    Pass,
    Before(Arc<Gate>),
    After(Arc<Gate>),
}
#[derive(Default)]
pub struct Stats {
    pub wrapped: AtomicU64,
    pub startup_calls: AtomicU64,
    pub inner_returned: AtomicU64,
    pub witness: Mutex<Weak<()>>,
}
#[derive(Clone)]
pub struct Tracked<R> {
    inner: R,
    stats: Arc<Stats>,
    stage: Arc<Stage>,
    // Owning this guard until the repository clone drops is the observable lifetime.
    _lifetime: Arc<()>,
}
impl<R> Tracked<R> {
    pub fn new(inner: R, stats: Arc<Stats>, stage: Stage) -> Self {
        stats.wrapped.fetch_add(1, Ordering::SeqCst);
        let lifetime = Arc::new(());
        *stats.witness.lock().unwrap() = Arc::downgrade(&lifetime);
        Self {
            inner,
            stats,
            stage: Arc::new(stage),
            _lifetime: lifetime,
        }
    }
}
impl<R: RequestRegistryRepository> RequestRegistryRepository for Tracked<R> {
    async fn register(
        &self,
        a: &CurrentRequestActor,
        i: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<RegistrationFact, RegistryError> {
        self.inner.register(a, i, id, at, d).await
    }
    async fn begin_consume(
        &self,
        a: &CurrentRequestActor,
        id: RegistrationId,
        i: &RegisteredIntent,
        op: EntityId,
        at: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<BeginRecord, RegistryError> {
        self.inner.begin_consume(a, id, i, op, at, d).await
    }
    async fn settle(
        &self,
        l: &ConsumeLease,
        o: &QueueOutcome,
        at: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.settle(l, o, at, d).await
    }
    async fn read(
        &self,
        r: &CurrentRequestActor,
        t: EntityId,
        id: RegistrationId,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.read(r, t, id, d).await
    }
    async fn inspect_operation(
        &self,
        p: &OperationProbe,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.inspect_operation(p, d).await
    }
    async fn interrupt_previous_runtime(
        &self,
        r: RuntimeEpoch,
        at: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<u64, RegistryError> {
        self.stats.startup_calls.fetch_add(1, Ordering::SeqCst);
        if let Stage::Before(g) = self.stage.as_ref() {
            g.pause().await;
        }
        let result = self.inner.interrupt_previous_runtime(r, at, d).await?;
        self.stats.inner_returned.fetch_add(1, Ordering::SeqCst);
        if let Stage::After(g) = self.stage.as_ref() {
            g.pause().await;
        }
        Ok(result)
    }
}
