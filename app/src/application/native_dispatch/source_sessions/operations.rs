//! Stable operations on the original Native task registry. Caller cancellation only drops a wait.
#![allow(
    clippy::future_not_send,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::super::runtime::SourceSlot;
use super::super::tasks::TaskSlot;
use super::super::{
    DispatchError, DispatchFenceRejection, DispatchPollIo, DispatchWriteStep, EncodedPromptFrame,
    LifecycleWriteBoundary, LifecycleWriteIdentity, NativeApprovalPort, NativeApprovalRepository,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeHandle, NativeRuntimeOptions, OwnedNativeSource, PromptTicket, ReservedPrompt,
    ResponseWait, SourceLaunchPort,
};
use super::reply::SourceReplyCell;
use super::{
    ActorContext, Arc, BoxFuture, EntityId, Mutex, NativeSourceSessionPort, Ordering,
    OriginalSourceResult, PreSourceProbe, QueueRuntimeIdentity, RegistryDeadline,
    RegistryOwnerScope, RpcScalar, SourceAccessProof, SourceAckState, SourceAdmissionProof,
    SourceAdmissionResult, SourceConfigurationPort, SourceFailureClass, SourceLaunchSelection,
    SourceReaderProof, SourceReconcileProof, SourceRecoveryProof, SourceRegistrationProof,
    SourceRequestKind, SourceResponseFact, SourceResponseKind, SourceResultPublisher,
    SourceSessionAction, SourceSessionAuthority, SourceSessionError, SourceSessionFact,
    SourceSessionFailure, SourceSessionOperationId, SourceSessionOutcome,
    SourceSessionRegistration, SourceSessionRegistrationMetadata, SourceSessionRepository,
    SourceSessionState, SourceSqlClosurePort, SourceSqlInventory, SourceSqlObservation,
    SourceSqlReceipt, SourceSqlRole, SourceStoredOperation, SourceTaskObservation, SourceUpdate,
    SourceUpdateView, SourceWriteProof, TransportFact, source_intent_digest,
};
#[cfg(test)]
use super::{
    BlockingTestBarrier, SourceSqlInvocation, SourceSqlOriginalSnapshot, SourceSqlTestFault,
};
use crate::application::document_disposal::DocumentDisposalOwner;
use std::{
    collections::HashMap,
    sync::{OnceLock, Weak},
};
use tokio::sync::{Notify, oneshot};

pub trait SourceRepository:
    NativeResourceRepository
    + NativeDispatchRepository
    + NativeControlRepository
    + NativeApprovalRepository
    + SourceSessionRepository
{
}
impl<
    T: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + NativeApprovalRepository
        + SourceSessionRepository,
> SourceRepository for T
{
}
struct Operation<P: NativeApprovalPort + NativeSourceSessionPort> {
    registration: SourceSessionRegistration,
    admitted_at: i64,
    job: Arc<OnceLock<Weak<TaskSlot>>>,
    access: OnceLock<Arc<SourceAccessProof>>,
    changed: Arc<Notify>,
    result: Mutex<Option<Result<SourceSessionFact, SourceSessionFailure>>>,
    original_error: Mutex<Option<SourceSessionFailure>>,
    fact: Mutex<SourceSessionFact>,
    sql: Mutex<Vec<Arc<dyn SourceSqlReceipt>>>,
    cpu: Arc<Mutex<Vec<Weak<TaskSlot>>>>,
    reply: OnceLock<Arc<SourceReplyCell<P>>>,
    staged: Mutex<Option<(P::SessionReservation, P::Frame)>>,
    ticket: Mutex<Option<Arc<P::Ticket>>>,
}
#[derive(Clone, Debug)]
pub struct SourceSessionObservation {
    pub(crate) fact: SourceSessionFact,
    pub(crate) response: Option<SourceResponseFact>,
    pub(crate) task: SourceTaskObservation,
    pub(crate) original_error: Option<SourceSessionFailure>,
    pub(crate) sql: Vec<Option<SourceSqlObservation>>,
    pub(crate) cpu: Vec<SourceTaskObservation>,
}
pub struct SourceSessionReceipt<P: NativeApprovalPort + NativeSourceSessionPort> {
    operation: Arc<Operation<P>>,
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> Clone for SourceSessionReceipt<P> {
    fn clone(&self) -> Self {
        Self {
            operation: self.operation.clone(),
        }
    }
}
fn task_observation(slot: Option<Arc<TaskSlot>>) -> SourceTaskObservation {
    slot.map_or(
        SourceTaskObservation {
            installed: false,
            joined: false,
            failed: false,
        },
        |slot| SourceTaskObservation {
            installed: true,
            joined: slot.confirmed_joined(),
            failed: slot.failed(),
        },
    )
}
impl<P: NativeApprovalPort + NativeSourceSessionPort> SourceSessionReceipt<P> {
    pub(crate) fn identity(&self) -> SourceSessionOperationId {
        self.operation.registration.id
    }
    pub(crate) fn observe(&self) -> SourceSessionObservation {
        SourceSessionObservation {
            fact: self
                .operation
                .fact
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
            response: self
                .operation
                .reply
                .get()
                .and_then(|reply| reply.response()),
            task: task_observation(self.operation.job.get().and_then(Weak::upgrade)),
            original_error: self
                .operation
                .original_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
            sql: self
                .operation
                .sql
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .map(|slot| slot.observe())
                .collect(),
            cpu: self
                .operation
                .cpu
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .map(|slot| task_observation(slot.upgrade()))
                .collect(),
        }
    }
    pub(crate) fn try_result(&self) -> Option<Result<SourceSessionFact, SourceSessionFailure>> {
        let mut result = self
            .operation
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(Ok(fact)) = &mut result {
            fact.result_available = self
                .operation
                .reply
                .get()
                .and_then(|reply| reply.result().upgrade())
                .is_some_and(|root| root.available());
        }
        if result.is_some() {
            return result;
        }
        let observation = task_observation(self.operation.job.get().and_then(Weak::upgrade));
        if observation.failed {
            Some(Err(SourceSessionFailure::WorkerUnconfirmed {
                operation: Some(self.identity()),
                observation,
                sql: self
                    .operation
                    .sql
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .last()
                    .and_then(|slot| slot.observe()),
            }))
        } else {
            None
        }
    }
    pub(crate) async fn wait(&self) -> Result<SourceSessionFact, SourceSessionFailure> {
        loop {
            let changed = self.operation.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(result) = self.try_result() {
                return result;
            }
            changed.await;
        }
    }
}
struct ManagementCore<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    native: NativeRuntimeHandle<P, L, G, N>,
    scope: RegistryOwnerScope,
    queue: QueueRuntimeIdentity,
    disposal: DocumentDisposalOwner<P::ResultRaw>,
    operations: Mutex<HashMap<SourceSessionOperationId, Arc<Operation<P>>>>,
}
pub struct SourceSessionManager<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    core: Arc<ManagementCore<P, L, G, N>>,
}
impl<P, L, G, N> Clone for SourceSessionManager<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
        }
    }
}
pub struct SourcePreparedExecution<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    manager: SourceSessionManager<P, L, G, N>,
    stored: SourceStoredOperation,
    actor: ActorContext,
}
impl<P, L, G, N> SourceSessionManager<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    pub(crate) fn new(
        native: NativeRuntimeHandle<P, L, G, N>,
        disposal: DocumentDisposalOwner<P::ResultRaw>,
        scope: RegistryOwnerScope,
        queue: QueueRuntimeIdentity,
    ) -> Result<Self, SourceSessionFailure> {
        let original = native.core.repository.source_owner_scope();
        if !scope.matches(&original)
            || !scope.matches(&native.core.configuration.source_configuration_scope())
        {
            return Err(SourceSessionError::InvalidScope.into());
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        native
            .core
            .source_facade
            .set(())
            .map_err(|()| SourceSessionError::Conflict)?;
        let manager = Self {
            core: Arc::new(ManagementCore {
                native,
                scope,
                queue,
                disposal,
                operations: Mutex::new(HashMap::new()),
            }),
        };
        manager
            .core
            .native
            .core
            .source_sql_closer
            .set(Arc::new(OriginalSourceSqlCloser {
                manager: Arc::downgrade(&manager.core),
            }))
            .map_err(|_| SourceSessionError::Conflict)?;
        Ok(manager)
    }
    fn deadline(&self) -> Result<RegistryDeadline, SourceSessionFailure> {
        Ok(self.core.native.core.deadline()?)
    }
    async fn call<
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, SourceSessionFailure>> + Send + 'static,
    >(
        &self,
        id: Option<SourceSessionOperationId>,
        sql: Arc<N::SqlOperation>,
        future: F,
    ) -> Result<T, SourceSessionFailure> {
        self.call_recorded(id, sql, None, None, false, future).await
    }
    async fn call_recorded<
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, SourceSessionFailure>> + Send + 'static,
    >(
        &self,
        id: Option<SourceSessionOperationId>,
        sql: Arc<N::SqlOperation>,
        update: Option<Arc<SourceWriteProof>>,
        repair: Option<SourceReconcileProof>,
        source_close_repair: bool,
        future: F,
    ) -> Result<T, SourceSessionFailure> {
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let ready = Arc::new(Notify::new());
        let original = Arc::new(OnceLock::<Weak<TaskSlot>>::new());
        let tx = sender.clone();
        let failure_tx = sender.clone();
        let gate = ready.clone();
        let installed = original.clone();
        let failed_slot = original.clone();
        let failure_sql = sql.clone();
        let invocation =
            self.core
                .native
                .core
                .source_sql
                .begin_invocation(id, sql.clone(), original.clone());
        #[cfg(test)]
        if let Some(error) = self
            .core
            .native
            .core
            .source_sql
            .take_rejected_dispatch_for_test(sql.role())
        {
            invocation.mark_never_scheduled(error);
            return Err(error.into());
        }
        let close_repair = source_close_repair && repair.is_some();
        if self.core.scope.require_open().is_err()
            || (!close_repair && self.core.native.core.tasks.is_sealed())
        {
            invocation.mark_never_scheduled(SourceSessionError::NodeStopping);
            return Err(SourceSessionError::NodeStopping.into());
        }
        let remembered = if let Some(permit) = &repair {
            self.core.native.core.source_sql.remember_repair(
                permit.id,
                sql.clone(),
                original.clone(),
                update,
                permit,
            )
        } else {
            self.core
                .native
                .core
                .source_sql
                .remember(id, sql.clone(), original.clone(), update)
        };
        if let Err(error) = remembered {
            invocation.mark_never_scheduled(SourceSessionError::NodeStopping);
            return Err(error);
        }
        #[cfg(test)]
        let pause_inventory = self.core.native.core.source_sql.clone();
        #[cfg(test)]
        let pause_role = sql.role();
        let role = sql.role();
        let tasks = if close_repair {
            &self.core.native.core.source_close_tasks
        } else {
            &self.core.native.core.tasks
        };
        let spawned = tasks.spawn_guarded(
            async move {
                loop {
                    let changed = gate.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if installed.get().is_some() {
                        break;
                    }
                    changed.await;
                }
                #[cfg(test)]
                pause_inventory.pause_before(pause_role).await;
                let result = future.await;
                if let Some(sender) = tx
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    let _ = sender.send(result);
                }
            },
            move || {
                let error = SourceSessionFailure::WorkerUnconfirmed {
                    operation: id,
                    observation: task_observation(failed_slot.get().and_then(Weak::upgrade)),
                    sql: failure_sql.observe(),
                };
                if let Some(sender) = failure_tx
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    let _ = sender.send(Err(error));
                }
            },
        );
        let slot = match spawned {
            Ok(slot) => slot,
            Err(error) => {
                // The future never ran; this explicit empty weak records rejection before IO.
                invocation.mark_never_scheduled(SourceSessionError::NodeStopping);
                return Err(error.into());
            }
        };
        invocation.mark_scheduled(&slot);
        ready.notify_waiters();
        let result = receiver
            .await
            .map_err(|_| SourceSessionFailure::WorkerUnconfirmed {
                operation: id,
                observation: task_observation(Some(slot.clone())),
                sql: sql.observe(),
            })?;
        if !slot.join(self.deadline()?.deadline()).await {
            return Err(SourceSessionFailure::WorkerUnconfirmed {
                operation: id,
                observation: task_observation(Some(slot)),
                sql: sql.observe(),
            });
        }
        if matches!(role, SourceSqlRole::Query | SourceSqlRole::Register)
            && let Some(id) = id
            && let Some(instance) = self.core.native.core.source_sql.bound_source(id)?
        {
            let sources = self
                .core
                .native
                .core
                .sources
                .lock()
                .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
                .clone();
            for source in sources {
                if source.pre.metadata().instance_id == instance
                    && source.pre.authority.closed.load(Ordering::Acquire)
                {
                    NativeCoordinator::<P, L, G, N>::request_source_metadata_retry(&source);
                }
            }
        }
        result
    }
    pub(crate) async fn recover(&self) -> Result<(), SourceSessionFailure> {
        let proof = SourceRecoveryProof {
            scope: self.core.scope.clone(),
            runtime: self.core.native.core.runtime,
            at: self.core.native.core.clock.unix_millis(),
        };
        let sql = self
            .core
            .native
            .core
            .repository
            .source_sql_operation(SourceSqlRole::Recover, None)?;
        let observed = sql.clone();
        let native = self.core.native.clone();
        let d = self.deadline()?;
        let barriers = self
            .call(None, sql, async move {
                native
                    .core
                    .repository
                    .recover_source_sessions(&proof, observed, &d)
                    .await
            })
            .await?;
        self.core
            .native
            .core
            .environment
            .ownership
            .restore(barriers)?;
        Ok(())
    }
    pub(crate) async fn register<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        selection: SourceLaunchSelection,
        action: SourceSessionAction,
    ) -> Result<SourceSessionRegistrationMetadata, SourceSessionFailure> {
        let id =
            SourceSessionOperationId::new(super::super::NativeCoordinator::<P, L, G, N>::new_id()?);
        self.register_inner(authority, actor, id, true, selection, action)
            .await
    }
    pub(crate) async fn register_with_id<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
        selection: SourceLaunchSelection,
        action: SourceSessionAction,
    ) -> Result<SourceSessionRegistrationMetadata, SourceSessionFailure> {
        self.register_inner(authority, actor, id, false, selection, action)
            .await
    }
    async fn register_inner<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
        generated_id: bool,
        selection: SourceLaunchSelection,
        action: SourceSessionAction,
    ) -> Result<SourceSessionRegistrationMetadata, SourceSessionFailure> {
        authority.verify_owner_scope(&self.core.scope)?;
        let actor = authority.current(actor).await?;
        if selection.launch_path.is_empty()
            || selection.launch_path.contains('\0')
            || selection.launch_path.len() > 4096
            || action
                .cursor()
                .is_some_and(|cursor| cursor.is_empty() || cursor.len() > 4096)
        {
            return Err(SourceSessionError::InvalidSelection.into());
        }
        let digest = source_intent_digest(&selection, &action)?;
        let registration = SourceSessionRegistration {
            id,
            actor: actor.client_id,
            auth_generation: actor.auth_generation,
            selection,
            action,
            intent_digest: digest,
            registered_at_ms: self.core.native.core.clock.unix_millis(),
        };
        let proof = SourceRegistrationProof {
            registration,
            scope: self.core.scope.clone(),
            generated_id,
        };
        let sql = self
            .core
            .native
            .core
            .repository
            .source_sql_operation(SourceSqlRole::Register, Some(id))?;
        let observed = sql.clone();
        let native = self.core.native.clone();
        let d = self.deadline()?;
        let stored = self
            .call(Some(id), sql, async move {
                native
                    .core
                    .repository
                    .register_source_session(&proof, observed, &d)
                    .await
            })
            .await?;
        Ok(stored.registration.metadata())
    }
    async fn stored<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
    ) -> Result<(SourceStoredOperation, ActorContext), SourceSessionFailure> {
        authority.verify_owner_scope(&self.core.scope)?;
        let actor = authority.current(actor).await?;
        let proof = SourceReaderProof {
            scope: self.core.scope.clone(),
            actor,
        };
        let sql = self
            .core
            .native
            .core
            .repository
            .source_sql_operation(SourceSqlRole::Query, Some(id))?;
        let observed = sql.clone();
        let native = self.core.native.clone();
        let d = self.deadline()?;
        Ok((
            self.call(Some(id), sql, async move {
                native
                    .core
                    .repository
                    .query_source_session(&proof, id, observed, &d)
                    .await
            })
            .await?,
            actor,
        ))
    }
    pub(crate) async fn query<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
    ) -> Result<SourceSessionFact, SourceSessionFailure> {
        let (mut stored, _) = self.stored(authority, actor, id).await?;
        stored.fact.result_available = self.result(id).is_some_and(|root| root.available());
        Ok(stored.fact)
    }
    pub(crate) async fn prepare_execute<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
    ) -> Result<SourcePreparedExecution<P, L, G, N>, SourceSessionFailure> {
        let (stored, actor) = self.stored(authority, actor, id).await?;
        // Before any existing slot, offer or preparation insertion: a reader cannot steal another actor's registration.
        if stored.registration.actor != actor.client_id {
            return Err(SourceSessionError::InvalidActor.into());
        }
        Ok(SourcePreparedExecution {
            manager: self.clone(),
            stored,
            actor,
        })
    }
    pub(crate) fn result(
        &self,
        id: SourceSessionOperationId,
    ) -> Option<Arc<OriginalSourceResult<P::ResultRaw>>> {
        let operation = self.core.operations.lock().ok()?.get(&id).cloned()?;
        operation.reply.get()?.result().upgrade()
    }
}

impl<P, L, G, N> SourcePreparedExecution<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    pub(crate) fn require_actor(&self, actor: ActorContext) -> Result<(), SourceSessionFailure> {
        if actor.client_id != self.actor.client_id
            || actor.auth_generation != self.actor.auth_generation
        {
            return Err(SourceSessionError::InvalidActor.into());
        }
        Ok(())
    }
    /// Called only in the real socket lifetime fence, while its original current guard is held.
    pub(crate) fn begin(
        self,
        publisher: Arc<dyn SourceResultPublisher<P::ResultRaw>>,
    ) -> Result<SourceSessionReceipt<P>, SourceSessionFailure> {
        let admission = self
            .manager
            .core
            .native
            .core
            .source_admission
            .lock()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        self.manager.core.native.core.running()?;
        let mut operations = self
            .manager
            .core
            .operations
            .lock()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        if let Some(original) = operations.get(&self.stored.registration.id) {
            if original.registration != self.stored.registration {
                return Err(SourceSessionError::Conflict.into());
            }
            publisher.no_result();
            return Ok(SourceSessionReceipt {
                operation: original.clone(),
            });
        }
        operations
            .try_reserve(1)
            .map_err(|_| SourceSessionError::Allocation)?;
        let operation = Arc::new(Operation {
            registration: self.stored.registration.clone(),
            admitted_at: self.manager.core.native.core.clock.unix_millis(),
            job: Arc::new(OnceLock::new()),
            access: OnceLock::new(),
            changed: Arc::new(Notify::new()),
            result: Mutex::new(None),
            original_error: Mutex::new(None),
            fact: Mutex::new(self.stored.fact.clone()),
            sql: Mutex::new(Vec::new()),
            cpu: Arc::new(Mutex::new(Vec::new())),
            reply: OnceLock::new(),
            staged: Mutex::new(None),
            ticket: Mutex::new(None),
        });
        if self.stored.fact.state != SourceSessionState::Registered {
            *operation
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Ok(self.stored.fact));
            publisher.no_result();
            operations.insert(operation.registration.id, operation.clone());
            return Ok(SourceSessionReceipt { operation });
        }
        let native = self.manager.core.native.clone();
        let disposal = self.manager.core.disposal.clone();
        let owner = self.manager.core.scope.clone();
        let queue = self.manager.core.queue.clone();
        let original = operation.clone();
        let failed = Arc::downgrade(&operation);
        let actor = self.actor;
        let slot = self.manager.core.native.core.tasks.spawn_guarded(
            async move {
                loop {
                    let changed = original.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if original.job.get().is_some() {
                        break;
                    }
                    changed.await;
                }
                let mut source = None;
                let result = drive(
                    &native,
                    &disposal,
                    &owner,
                    &queue,
                    &original,
                    actor,
                    publisher.clone(),
                    &mut source,
                )
                .await;
                if result.is_err()
                    && original
                        .reply
                        .get()
                        .and_then(|reply| reply.response())
                        .is_none()
                {
                    publisher.no_result();
                }
                {
                    let mut cached = original
                        .result
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if cached.is_none() {
                        *cached = Some(result);
                    }
                }
                original.changed.notify_waiters();
                cleanup(&native, &original, source.as_ref()).await;
            },
            move || {
                if let Some(operation) = failed.upgrade() {
                    operation.changed.notify_waiters();
                }
            },
        )?;
        let _ = operation.job.set(Arc::downgrade(&slot));
        operations.insert(operation.registration.id, operation.clone());
        drop(operations);
        drop(admission);
        operation.changed.notify_waiters();
        Ok(SourceSessionReceipt { operation })
    }
}
fn remember_sql<P: NativeApprovalPort + NativeSourceSessionPort, N: SourceRepository>(
    repository: &N,
    inventory: &SourceSqlInventory,
    operation: &Operation<P>,
    role: SourceSqlRole,
    proof: Arc<SourceWriteProof>,
) -> Result<Arc<N::SqlOperation>, SourceSessionFailure> {
    let slot = repository.source_sql_operation(role, Some(operation.registration.id))?;
    inventory.remember(
        Some(operation.registration.id),
        slot.clone(),
        operation.job.clone(),
        Some(proof),
    )?;
    let mut slots = operation
        .sql
        .lock()
        .map_err(|_| SourceSessionError::Allocation)?;
    slots
        .try_reserve(1)
        .map_err(|_| SourceSessionError::Allocation)?;
    slots.push(slot.clone());
    Ok(slot)
}
async fn blocking<P, L, G, N, T, F>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    operation: &Arc<Operation<P>>,
    source: Option<&Arc<SourceSlot<P, L>>>,
    work: F,
) -> Result<T, SourceSessionFailure>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
    T: Send + 'static,
    F: FnOnce() -> Result<T, SourceSessionFailure> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let changed = operation.changed.clone();
    let slot = native.core.tasks.spawn_blocking_guarded(
        move || {
            let result = work();
            let _ = tx.send(result);
        },
        move || changed.notify_waiters(),
    )?;
    operation
        .cpu
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(Arc::downgrade(&slot));
    if let Some(source) = source {
        let _ = source.jobs.register_existing(slot.clone());
    }
    let result = rx.await;
    slot.join_unbounded().await;
    if slot.failed() {
        return Err(SourceSessionFailure::WorkerUnconfirmed {
            operation: Some(operation.registration.id),
            observation: task_observation(Some(slot)),
            sql: None,
        });
    }
    result.map_err(|_| SourceSessionError::WorkerUnconfirmed)?
}
struct CoordinatorCpuSpawner<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    native: NativeRuntimeHandle<P, L, G, N>,
    source: Arc<SourceSlot<P, L>>,
    operation_cpu: Arc<Mutex<Vec<Weak<TaskSlot>>>>,
}
impl<P, L, G, N> super::reply::SourceCpuSpawner for CoordinatorCpuSpawner<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    fn spawn(
        &self,
        work: Box<dyn FnOnce() + Send>,
        on_failure: Box<dyn FnOnce() + Send>,
    ) -> Result<(), DispatchError> {
        let slot = self
            .native
            .core
            .tasks
            .spawn_blocking_guarded(work, on_failure)?;
        self.operation_cpu
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::downgrade(&slot));
        let _ = self.source.jobs.register_existing(slot);
        Ok(())
    }
}
async fn dispose<P, L, G, N, T>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    operation: &Arc<Operation<P>>,
    source: Option<&Arc<SourceSlot<P, L>>>,
    value: T,
) where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
    T: Send + 'static,
{
    let cell = Arc::new(Mutex::new(Some(value)));
    let original = cell.clone();
    let result = blocking(native, operation, source, move || {
        let value = original
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(value);
        Ok(())
    })
    .await;
    if result.is_err() {
        // A failed registration/worker cannot cause the last large value to fall onto this task.
        // Keep this original owned task and cell unjoined; normal shutdown reports Unknown.
        let _root = cell;
        std::future::pending::<()>().await;
    }
}
struct SourceBoundary<S> {
    scope: S,
    rpc: RpcScalar,
    kind: super::super::LifecycleKind,
    source: Arc<PreSourceProbe>,
}
impl<S: Clone + Eq + Send + Sync + 'static> LifecycleWriteBoundary<S> for SourceBoundary<S> {
    fn check_and_poll(
        &self,
        write: &LifecycleWriteIdentity<S>,
        _: DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        let _gate = self
            .source
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
        if write.scope != self.scope
            || write.rpc != self.rpc
            || write.kind != self.kind
            || self.source.authority.closed.load(Ordering::Acquire)
            || self.source.authority.closing.load(Ordering::Acquire)
        {
            return Err(DispatchFenceRejection::Stopped);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
async fn write_fact<P, L, G, N>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    operation: &Operation<P>,
    access: &Arc<SourceAccessProof>,
    update: SourceUpdate,
    role: SourceSqlRole,
) -> Result<SourceSessionFact, SourceSessionFailure>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    let proof = Arc::new(SourceWriteProof {
        access: access.clone(),
        update,
        at: native.core.clock.unix_millis(),
    });
    let slot = remember_sql(
        &*native.core.repository,
        &native.core.source_sql,
        operation,
        role,
        proof.clone(),
    )?;
    #[cfg(test)]
    native.core.source_sql.pause_before(role).await;
    let d = native.core.deadline()?;
    let stored = native
        .core
        .repository
        .update_source_session(&proof, slot, &d)
        .await?;
    *operation
        .fact
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = stored.fact.clone();
    operation.changed.notify_waiters();
    Ok(stored.fact)
}

async fn drive<P, L, G, N>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    disposal: &DocumentDisposalOwner<P::ResultRaw>,
    owner: &RegistryOwnerScope,
    queue: &QueueRuntimeIdentity,
    operation: &Arc<Operation<P>>,
    actor: ActorContext,
    publisher: Arc<dyn SourceResultPublisher<P::ResultRaw>>,
    source_out: &mut Option<Arc<SourceSlot<P, L>>>,
) -> Result<SourceSessionFact, SourceSessionFailure>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    let registration = Arc::new(SourceRegistrationProof {
        registration: operation.registration.clone(),
        scope: owner.clone(),
        generated_id: false,
    });
    let proof = SourceAdmissionProof {
        original: registration,
        runtime: native.core.runtime,
        actor,
        at: operation.admitted_at,
    };
    let slot = native
        .core
        .repository
        .source_sql_operation(SourceSqlRole::Admit, Some(operation.registration.id))?;
    native.core.source_sql.remember(
        Some(operation.registration.id),
        slot.clone(),
        operation.job.clone(),
        None,
    )?;
    operation
        .sql
        .lock()
        .map_err(|_| SourceSessionError::Allocation)?
        .push(slot.clone());
    #[cfg(test)]
    native
        .core
        .source_sql
        .pause_before(SourceSqlRole::Admit)
        .await;
    let d = native.core.deadline()?;
    let admitted = native
        .core
        .repository
        .admit_source_session(&proof, slot.clone(), &d)
        .await?;
    let stored = match admitted {
        SourceAdmissionResult::Existing(stored) => return Ok(stored.fact),
        SourceAdmissionResult::Admitted(stored) => stored,
    };
    let observation = slot
        .observe()
        .ok_or(SourceSessionError::CommitUnconfirmed)?;
    if observation.transaction
        != crate::domain::native_catalog::TransactionKnowledge::CommitConfirmed
        || observation.cleanup != crate::domain::native_catalog::CleanupKnowledge::ConfirmedOriginal
        || stored.fact.runtime != Some(native.core.runtime)
        || stored.fact.state != SourceSessionState::Owned
    {
        return Err(SourceSessionError::CommitUnconfirmed.into());
    }
    *operation
        .fact
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = stored.fact;
    let access = Arc::new(SourceAccessProof {
        registration: stored.registration,
        runtime: native.core.runtime,
        scope: owner.clone(),
        admission: actor,
    });
    let _ = operation.access.set(access.clone());
    let result = drive_admitted(
        native, disposal, owner, queue, operation, &access, publisher, source_out,
    )
    .await;
    match result {
        Ok(fact) => Ok(fact),
        Err(error) => {
            *operation
                .original_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(error.clone());
            let class = match &error {
                SourceSessionFailure::Configuration(_) => SourceFailureClass::Configuration,
                SourceSessionFailure::NecessarySql(_) => SourceFailureClass::Metadata,
                SourceSessionFailure::Rejected(SourceSessionError::OwnershipUnconfirmed) => {
                    SourceFailureClass::Ownership
                }
                _ => SourceFailureClass::Protocol,
            };
            let possible = operation
                .ticket
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some();
            write_fact(
                native,
                operation,
                &access,
                SourceUpdate::Finish {
                    outcome: if possible {
                        SourceSessionOutcome::Unknown
                    } else {
                        SourceSessionOutcome::Refused
                    },
                    failure: Some(class),
                },
                SourceSqlRole::Finish,
            )
            .await?;
            Err(error)
        }
    }
}
async fn drive_admitted<P, L, G, N>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    disposal: &DocumentDisposalOwner<P::ResultRaw>,
    owner: &RegistryOwnerScope,
    queue: &QueueRuntimeIdentity,
    operation: &Arc<Operation<P>>,
    access: &Arc<SourceAccessProof>,
    publisher: Arc<dyn SourceResultPublisher<P::ResultRaw>>,
    source_out: &mut Option<Arc<SourceSlot<P, L>>>,
) -> Result<SourceSessionFact, SourceSessionFailure>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    native.core.running()?;
    let configuration = native.core.configuration.clone();
    let original = access.clone();
    let runtime = tokio::runtime::Handle::current();
    let plan = Arc::new(
        blocking(native, operation, None, move || {
            runtime.block_on(configuration.resolve_source(&original))
        })
        .await?,
    );
    let source = match native.core.ensure_source_slot(plan.clone()) {
        Ok(source) => source,
        Err(error) => {
            dispose(native, operation, None, plan).await;
            return Err(error.into());
        }
    };
    *source_out = Some(source.clone());
    let registered = (|| -> Result<(), SourceSessionFailure> {
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        if source.pre.authority.closed.load(Ordering::Acquire)
            || source.pre.authority.closing.load(Ordering::Acquire)
        {
            return Err(DispatchError::Stopped.into());
        }
        let job = operation
            .job
            .get()
            .and_then(Weak::upgrade)
            .ok_or(SourceSessionError::WorkerUnconfirmed)?;
        native
            .core
            .source_sql
            .bind(operation.registration.id, source.pre.metadata().instance_id)?;
        source.jobs.register_existing(job)?;
        for job in operation
            .cpu
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .iter()
            .filter_map(Weak::upgrade)
        {
            source.jobs.register_existing(job)?;
        }
        Ok(())
    })();
    // The candidate's last large drop runs on the original blocking owner, including cache reuse.
    dispose(native, operation, Some(&source), plan).await;
    registered?;
    {
        let install = source.install.lock().await;
        native.core.running()?;
        access.require_source(&source.pre)?;
        native
            .core
            .repository
            .reserve_source(&source.pre, &native.core.deadline()?)
            .await?;
        source.recorded.store(true, Ordering::Release);
        let initialized = native
            .core
            .initialize_source_checked(&source, || access.require_source(&source.pre))
            .await;
        if let Err(error) = initialized {
            native.core.fail_source(&source, error);
            drop(install);
            return Err(error.into());
        }
        drop(install);
    }
    let wire = source
        .memory
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .initialized
        .as_ref()
        .ok_or(DispatchError::NotReady)?
        .metadata
        .wire;
    write_fact(
        native,
        operation,
        access,
        SourceUpdate::Attach {
            source: source.pre.clone(),
            wire,
        },
        SourceSqlRole::SourceAttach,
    )
    .await?;
    let connection = source
        .source
        .get()
        .ok_or(DispatchError::ProtocolUnavailable)?
        .clone();
    let setup = source
        .plan
        .setups
        .iter()
        .find(|(version, _)| *version == wire)
        .map(|(_, setup)| setup.clone())
        .ok_or(DispatchError::InvalidConfiguration)?;
    let protocol = native.core.protocol.clone();
    let binding = super::super::ConfigurationKey {
        identity: source.plan.configuration.identity,
        revision: source.plan.configuration.revision,
    };
    let original_connection = connection.clone();
    let setup = Arc::new(
        blocking(native, operation, Some(&source), move || {
            protocol
                .bind_setup(&original_connection, wire, &binding, setup)
                .map_err(Into::into)
        })
        .await?,
    );
    let handoff = disposal
        .reserve_handoff()
        .map_err(|_| SourceSessionError::ResultUnavailable)?;
    let protocol = native.core.protocol.clone();
    let action = operation.registration.action.clone();
    let original_connection = connection.clone();
    let ((reservation, frame), kind) = blocking(native, operation, Some(&source), move || {
        let (reservation, kind) = match action {
            SourceSessionAction::Discover { cursor } => (
                protocol.reserve_discover(
                    &original_connection,
                    &setup,
                    cursor.as_deref(),
                    ResponseWait::Unbounded,
                )?,
                SourceRequestKind::Discover,
            ),
            SourceSessionAction::Create => (
                protocol.reserve_create(&original_connection, &setup, ResponseWait::Unbounded)?,
                SourceRequestKind::Create,
            ),
        };
        let frame = reservation.encode()?;
        Ok(((reservation, frame), kind))
    })
    .await?;
    if kind == SourceRequestKind::Create {
        {
            let mut reservations = source
                .capacity_reservations
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            reservations
                .try_reserve(1)
                .map_err(|_| DispatchError::StorageUnavailable)?;
            reservations.push(operation.registration.id.entity());
        }
        native
            .core
            .admit_capacity(&super::super::CapacityReservation {
                id: operation.registration.id.entity(),
                source: source.pre.clone(),
                native: None,
                create_operation: Some(operation.registration.id.entity()),
            })
            .await?;
    }
    let rpc = reservation.rpc().clone();
    let scope = reservation.scope();
    let frame_length = frame.byte_len_checked()?;
    let tracking = native.core.protocol.source_reply_tracking(&reservation);
    let cell = Arc::new(SourceReplyCell::new(
        native.core.protocol.clone(),
        source.pre.clone(),
        scope.clone(),
        rpc.clone(),
        kind,
        wire,
        frame_length,
        tracking,
        native.core.environment.clone(),
        owner.clone(),
        queue.clone(),
        publisher,
        Arc::new(CoordinatorCpuSpawner {
            native: native.clone(),
            source: source.clone(),
            operation_cpu: operation.cpu.clone(),
        }),
        native.core.source_sql.clone(),
        handoff,
        operation.changed.clone(),
    ));
    *operation
        .staged
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((reservation, frame));
    let _ = operation.reply.set(cell.clone());
    {
        let mut replies = source
            .source_replies
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        replies
            .try_reserve(1)
            .map_err(|_| SourceSessionError::Allocation)?;
        if replies.contains_key(&rpc) {
            return Err(SourceSessionError::Conflict.into());
        }
        replies.insert(rpc.clone(), cell.clone());
    }
    write_fact(
        native,
        operation,
        access,
        SourceUpdate::Reserve {
            source: source.pre.clone(),
            wire,
            rpc: rpc.clone(),
            frame_length,
        },
        SourceSqlRole::RpcReserve,
    )
    .await?;
    write_fact(
        native,
        operation,
        access,
        SourceUpdate::Arm { rpc: rpc.clone() },
        SourceSqlRole::Arm,
    )
    .await?;
    let (reservation, frame) = operation
        .staged
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .take()
        .ok_or(DispatchError::InvalidPromptReservation)?;
    let boundary = Arc::new(SourceBoundary {
        scope,
        rpc: rpc.clone(),
        kind: match kind {
            SourceRequestKind::Create => super::super::LifecycleKind::Create,
            SourceRequestKind::Discover => super::super::LifecycleKind::Discover,
        },
        source: source.pre.clone(),
    });
    let ticket = match native.core.protocol.enqueue_source_session(
        &connection,
        reservation,
        frame,
        boundary,
        NativeRuntimeOptions::at(native.core.options.load_write_wait)?,
    ) {
        Ok(ticket) => Arc::new(ticket),
        Err(rejected) => {
            *operation
                .staged
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some((rejected.reservation, rejected.frame));
            return Err(rejected.error.into());
        }
    };
    *operation
        .ticket
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ticket.clone());
    let transport = ticket.wait().await;
    write_fact(
        native,
        operation,
        access,
        SourceUpdate::Transport {
            rpc: rpc.clone(),
            frame_length,
            fact: transport,
        },
        SourceSqlRole::Transport,
    )
    .await?;
    if matches!(transport, TransportFact::Zero { .. }) {
        cell.release_after_original_zero(&ticket)?;
        let fact = write_fact(
            native,
            operation,
            access,
            SourceUpdate::Finish {
                outcome: SourceSessionOutcome::Refused,
                failure: Some(SourceFailureClass::Protocol),
            },
            SourceSqlRole::Finish,
        )
        .await?;
        if kind == SourceRequestKind::Create {
            native
                .core
                .repository
                .abandon_capacity(
                    operation.registration.id.entity(),
                    &source.pre,
                    &native.core.deadline()?,
                )
                .await?;
        }
        return Ok(fact);
    }
    loop {
        let changed = cell.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if let Some(response) = cell.response()
            && response.ack != SourceAckState::Pending
        {
            write_fact(
                native,
                operation,
                access,
                SourceUpdate::Response {
                    rpc: rpc.clone(),
                    fact: response.clone(),
                },
                SourceSqlRole::Response,
            )
            .await?;
            if kind == SourceRequestKind::Create && cell.tree_unverified() {
                return Err(DispatchError::SessionTreeUnverified.into());
            }
            let (outcome, failure) = if cell.ownership_failed() {
                (
                    SourceSessionOutcome::Unknown,
                    Some(SourceFailureClass::Ownership),
                )
            } else if response.ack != SourceAckState::Confirmed {
                (
                    SourceSessionOutcome::Unknown,
                    Some(SourceFailureClass::Protocol),
                )
            } else if response.kind == SourceResponseKind::RemoteError {
                (SourceSessionOutcome::Refused, None)
            } else {
                (SourceSessionOutcome::Succeeded, None)
            };
            if kind == SourceRequestKind::Create
                && outcome == SourceSessionOutcome::Succeeded
                && response.kind == SourceResponseKind::Created
            {
                let key = crate::domain::native::NativeSessionKey::from_config(
                    Some(source.pre.metadata.source_id),
                    response
                        .native_session_id
                        .as_deref()
                        .ok_or(DispatchError::InvalidTarget)?,
                )
                .map_err(|_| DispatchError::InvalidTarget)?;
                let record = native
                    .core
                    .repository
                    .bind_created_capacity(
                        operation.registration.id.entity(),
                        &key,
                        &source.pre,
                        &native.core.deadline()?,
                    )
                    .await?;
                let capacity = native.core.capacity_resource(&source, &key)?;
                *capacity.record.lock().map_err(|_| DispatchError::Stopped)? = Some(record);
                {
                    let mut activity = source
                        .capacity_activity
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    activity.created(key.native_id(), wire, std::time::Instant::now());
                    activity.verify_root(key.native_id())?;
                    let _gate = source
                        .pre
                        .authority
                        .gate
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    source
                        .pre
                        .authority
                        .quarantined
                        .store(activity.has_unowned(), std::sync::atomic::Ordering::Release);
                }
                if let Some(display_name) = native
                    .core
                    .repository
                    .trusted_client_display_name(
                        operation.registration.actor,
                        &native.core.deadline()?,
                    )
                    .await?
                {
                    let claimed = native.core.session_control.claim_initial(
                        &key,
                        operation.registration.actor,
                        &display_name,
                        true,
                    );
                    let outcome_json = serde_json::json!({
                        "controllerClientId": operation.registration.actor.encode(),
                        "controllerDisplayName": display_name,
                        "controlEpoch": claimed.snapshot.control_epoch.to_string(),
                        "rootTargetId": claimed.snapshot.root_target_id.map(EntityId::encode),
                    })
                    .to_string();
                    let occurred_at = native.core.clock.unix_millis().div_euclid(1000);
                    native
                        .core
                        .repository
                        .record_control_security_audit(
                            Some(operation.registration.actor),
                            "session.control.initial",
                            &key,
                            outcome_json,
                            occurred_at,
                            &native.core.deadline()?,
                        )
                        .await?;
                } else {
                    native.core.session_control.mark_loaded(&key);
                }
            }
            return write_fact(
                native,
                operation,
                access,
                SourceUpdate::Finish { outcome, failure },
                SourceSqlRole::Finish,
            )
            .await;
        }
        if cell.decode_failed() {
            return Err(DispatchError::ProtocolRejected.into());
        }
        if cell.ended() && !cell.decode_pending() {
            return write_fact(
                native,
                operation,
                access,
                SourceUpdate::Finish {
                    outcome: SourceSessionOutcome::Unknown,
                    failure: Some(SourceFailureClass::Source),
                },
                SourceSqlRole::Finish,
            )
            .await;
        }
        changed.await;
    }
}
include!("reconciliation.rs");
async fn cleanup<P, L, G, N>(
    native: &NativeRuntimeHandle<P, L, G, N>,
    operation: &Arc<Operation<P>>,
    source: Option<&Arc<SourceSlot<P, L>>>,
) where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    let staged = operation
        .staged
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(pair) = staged {
        if let Some(reply) = operation.reply.get() {
            let _ = reply.abandon_unenqueued(&pair.0);
        }
        dispose(native, operation, source, pair).await;
    }
    if let Some(reply) = operation.reply.get()
        && reply.has_pending_handoff()
    {
        loop {
            let changed = reply.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if !reply.decode_pending() && (reply.ended() || !reply.has_pending_handoff()) {
                break;
            }
            changed.await;
        }
        if reply.has_pending_handoff()
            && let Some(source) = source
        {
            if let Some(cleanup) = source.cleanup.get() {
                cleanup.join_unbounded().await;
            }
            if source
                .io_closed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|observation| observation.joined)
            {
                reply.release_empty_after_io_join();
            } else {
                std::future::pending::<()>().await;
            }
        }
    }
    let cpu_slots = operation
        .cpu
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect::<Vec<_>>();
    for slot in cpu_slots {
        slot.join_unbounded().await;
    }
}
