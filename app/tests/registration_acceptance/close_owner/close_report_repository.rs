//! Exact real-repository delegation; only the frozen post-ack report hook differs.
use crate::{
    application::request_registry::*,
    domain::{EntityId, native::RuntimeEpoch},
    session_storage::request_registry::SqliteRequestRegistry,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Copy)]
pub(super) enum FailAt {
    Begin,
    Settle,
}
#[derive(Clone)]
pub(super) struct CloseReportRepository {
    pub(super) inner: SqliteRequestRegistry,
    pub(super) point: FailAt,
    pub(super) armed: Arc<AtomicBool>,
}
impl RequestRegistryRepository for CloseReportRepository {
    async fn register(
        &self,
        a: &CurrentRequestActor,
        i: &RegisteredIntent,
        id: RegistrationId,
        c: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<RegistrationFact, RegistryError> {
        self.inner.register(a, i, id, c, d).await
    }
    async fn begin_consume(
        &self,
        a: &CurrentRequestActor,
        id: RegistrationId,
        i: &RegisteredIntent,
        operation: EntityId,
        c: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<BeginRecord, RegistryError> {
        if matches!(self.point, FailAt::Begin) && !self.armed.swap(true, Ordering::SeqCst) {
            self.inner
                .connection_closer_for_test()
                .fail_next_result_after_ack()?;
        }
        self.inner.begin_consume(a, id, i, operation, c, d).await
    }
    async fn settle(
        &self,
        lease: &ConsumeLease,
        outcome: &QueueOutcome,
        c: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        if matches!(self.point, FailAt::Settle) && !self.armed.swap(true, Ordering::SeqCst) {
            self.inner
                .connection_closer_for_test()
                .fail_next_result_after_ack()?;
        }
        self.inner.settle(lease, outcome, c, d).await
    }
    async fn read(
        &self,
        a: &CurrentRequestActor,
        target: EntityId,
        id: RegistrationId,
        d: &RegistryDeadline,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.read(a, target, id, d).await
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
        c: &OperationContext,
        d: &RegistryDeadline,
    ) -> Result<u64, RegistryError> {
        self.inner.interrupt_previous_runtime(r, c, d).await
    }
}
