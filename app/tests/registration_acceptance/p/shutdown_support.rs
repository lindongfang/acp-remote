//! Ownership-only observers around genuine SQL and audit futures.
use super::{
    AuditFailure, AuditMetadata, AuditPort, BeginRecord, ConsumeLease, CurrentRequestActor,
    EntityId, OperationContext, OperationProbe, QueueOutcome, RegisteredIntent, RegistrationFact,
    RegistrationId, RegistryDeadline, RegistryError, RequestFact, RequestRegistryRepository,
    RuntimeEpoch,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopPoint {
    Begin,
    Settle,
    Audit,
}
pub struct Gate {
    reached: AtomicBool,
    released: AtomicBool,
    arrived: Notify,
    resume: Notify,
    pub work: Mutex<Weak<()>>,
}
impl Gate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            reached: AtomicBool::new(false),
            released: AtomicBool::new(false),
            arrived: Notify::new(),
            resume: Notify::new(),
            work: Mutex::new(Weak::new()),
        })
    }
    pub async fn wait(&self) {
        while !self.reached.load(Ordering::SeqCst) {
            self.arrived.notified().await;
        }
    }
    pub async fn pause(&self) {
        let witness = Arc::new(());
        *self.work.lock().unwrap() = Arc::downgrade(&witness);
        self.reached.store(true, Ordering::SeqCst);
        self.arrived.notify_one();
        while !self.released.load(Ordering::SeqCst) {
            self.resume.notified().await;
        }
        drop(witness);
    }
    pub fn live(&self) -> bool {
        self.work.lock().unwrap().upgrade().is_some()
    }
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.resume.notify_one();
    }
}
#[derive(Default)]
pub struct Counts {
    pub begin: AtomicU64,
    pub settle: AtomicU64,
    pub audit: AtomicU64,
}
#[derive(Clone)]
pub struct ClosingRepository<R> {
    pub inner: R,
    pub gate: Arc<Gate>,
    pub point: StopPoint,
    pub counts: Arc<Counts>,
}
impl<R: RequestRegistryRepository> RequestRegistryRepository for ClosingRepository<R> {
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
        self.counts.begin.fetch_add(1, Ordering::SeqCst);
        let fact = self.inner.begin_consume(a, id, i, op, at, d).await?;
        if self.point == StopPoint::Begin {
            self.gate.pause().await;
        }
        Ok(fact)
    }
    async fn settle(
        &self,
        l: &ConsumeLease,
        o: &QueueOutcome,
        at: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.counts.settle.fetch_add(1, Ordering::SeqCst);
        let fact = self.inner.settle(l, o, at, d).await?;
        if self.point == StopPoint::Settle {
            self.gate.pause().await;
        }
        Ok(fact)
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
        self.inner.interrupt_previous_runtime(r, at, d).await
    }
}
pub struct ClosingAudit {
    pub gate: Arc<Gate>,
    pub point: StopPoint,
    pub counts: Arc<Counts>,
}
impl AuditPort for ClosingAudit {
    fn append<'a>(
        &'a self,
        _: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.counts.audit.fetch_add(1, Ordering::SeqCst);
            if self.point == StopPoint::Audit {
                self.gate.pause().await;
            }
            Ok(())
        })
    }
}
