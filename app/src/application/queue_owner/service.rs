#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    ActiveKey, AuditTask, ConsumeReply, DriverError, NativeQueueDriver, OwnerCell, OwnerState,
    Pending, PendingWorker, ProcessingPhase, QueueEnvironmentPort, QueueRead, RuntimeCore,
    RuntimeSupervisor, ShutdownReport, StartupStatus, TargetResolver, TaskRecord, lock,
    pending_ids, release_waiters, teardown_lock,
};
#[cfg(test)]
use crate::application::request_registry::random_hooks;
use crate::{
    application::request_registry::{
        AuditFailure, AuditMetadata, AuditPort, AuditWarning, BeginRecord, ConsumeLease,
        CurrentRequestActor, NodeClock, OperationContext, OperationProbe, QueueAction,
        QueueOutcome, QueueRejection, RegisteredIntent, RegistrationFact, RegistrationId,
        RegistryDeadline, RegistryError, RequestAuthority, RequestFact, RequestRegistryRepository,
        StoredApplying, ValidatedBody, ValidatedBodyPort, ValidatedMutation, random_id,
    },
    domain::{
        EntityId,
        input_queue::{DispatchReadiness, QueueError, QueueInput, Submission},
        native::{NativeSessionKey, RuntimeEpoch},
    },
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};
use tokio::{
    sync::{Notify, oneshot},
    time::{Duration, Instant},
};

#[path = "read_commands.rs"]
pub(super) mod read_commands;

enum Installation<T> {
    Wait,
    Existing,
    Started(oneshot::Receiver<ConsumeReply<T>>),
}

// A single I/O wait is bounded; this never expires a registered identity or a queued input.
fn io_deadline() -> RegistryDeadline {
    RegistryDeadline::new(Instant::now() + Duration::from_secs(30))
}
struct AuthorityResolver<A>(Arc<A>);
impl<A: RequestAuthority> TargetResolver for AuthorityResolver<A> {
    fn resolve(
        &self,
        target: EntityId,
    ) -> Pin<Box<dyn Future<Output = Result<NativeSessionKey, RegistryError>> + Send + '_>> {
        Box::pin(self.0.resolve_target(target))
    }
}
struct ServiceInner<A, V: ValidatedBodyPort, R, C> {
    authority: Arc<A>,
    validator: V,
    repository: R,
    clock: Arc<C>,
    audit: Arc<dyn AuditPort>,
    core: Arc<RuntimeCore<V::Body>>,
    startup: tokio::sync::Mutex<()>,
    #[cfg(test)]
    randoms: Arc<random_hooks::Controller>,
}
pub struct QueueService<A, V: ValidatedBodyPort, R, C> {
    inner: Arc<ServiceInner<A, V, R, C>>,
}
impl<A, V: ValidatedBodyPort, R, C> Clone for QueueService<A, V, R, C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<A, V, R, C> QueueService<A, V, R, C>
where
    A: RequestAuthority + 'static,
    V: ValidatedBodyPort + 'static,
    R: RequestRegistryRepository + 'static,
    C: NodeClock + 'static,
{
    #[expect(
        clippy::too_many_arguments,
        reason = "Reviewed trusted dependencies and startup ownership stay explicit; tests add one sealed random source"
    )]
    pub(crate) fn new(
        authority: A,
        validator: V,
        repository: R,
        registry_owner: crate::application::request_registry::RegistryOwnerScope,
        clock: C,
        runtime: RuntimeEpoch,
        environment: Arc<dyn QueueEnvironmentPort>,
        audit: Arc<dyn AuditPort>,
        claim: Arc<AtomicBool>,
        #[cfg(test)] randoms: Arc<random_hooks::Controller>,
    ) -> (Self, NativeQueueDriver<V::Body>) {
        let authority = Arc::new(authority);
        let core = Arc::new(RuntimeCore {
            identity: Arc::new(()),
            registry_owner,
            owners: Mutex::new(HashMap::new()),
            resolver: Arc::new(AuthorityResolver(authority.clone())),
            environment,
            runtime,
            native_enabled: AtomicBool::new(false),
            status: AtomicU8::new(StartupStatus::Starting as u8),
            claim,
            tasks: Mutex::new(Vec::new()),
            #[cfg(windows)]
            claim_release_deferred: AtomicBool::new(false),
            shutdown: tokio::sync::Mutex::new(()),
            stop: Notify::new(),
            warnings: Mutex::new(Vec::new()),
            audit_cancelled: AtomicU64::new(0),
            task_failures: AtomicU64::new(0),
        });
        (
            Self {
                inner: Arc::new(ServiceInner {
                    authority,
                    validator,
                    repository,
                    clock: Arc::new(clock),
                    audit,
                    core: core.clone(),
                    startup: tokio::sync::Mutex::new(()),
                    #[cfg(test)]
                    randoms,
                }),
            },
            NativeQueueDriver { core },
        )
    }
    pub(crate) fn native_clock(&self) -> Arc<dyn NodeClock> {
        self.inner.clock.clone()
    }
    #[must_use]
    pub fn startup_status(&self) -> StartupStatus {
        self.inner.core.status()
    }
    pub(crate) async fn prepare_native_read(
        &self,
        actor: &A::Actor,
        target: EntityId,
        _reason: crate::application::shared_views::budget::ReadReason,
    ) -> Result<super::ReadTargetProof, RegistryError> {
        self.inner.core.running()?;
        let current = self.actor(actor, target, None).await?;
        self.inner.core.running()?;
        let owner = self.inner.core.owner_for(current.native().clone())?;
        Ok(super::ReadTargetProof::from_owner(
            &self.inner.core,
            &owner,
            target,
        ))
    }
    pub(crate) fn launch_startup(
        &self,
    ) -> Result<oneshot::Receiver<Result<StartupStatus, RegistryError>>, RegistryError> {
        let (tx, rx) = oneshot::channel();
        let service = self.clone();
        let mut tasks = lock(&self.inner.core.tasks)?;
        self.inner.core.reap_completed(&mut tasks);
        tasks.push(TaskRecord::new(
            tokio::spawn(async move {
                let result = service.reconcile_startup().await;
                let _ = tx.send(result);
            }),
            None,
            true,
        ));
        drop(tasks);
        Ok(rx)
    }
    /// # Errors
    /// Keeps new actions closed until old applying facts are durably interrupted.
    pub async fn reconcile_startup(&self) -> Result<StartupStatus, RegistryError> {
        let _startup = self.inner.startup.lock().await;
        match self.startup_status() {
            StartupStatus::Running => return Ok(StartupStatus::Running),
            StartupStatus::Stopping | StartupStatus::Stopped => {
                return Err(RegistryError::NodeStopping);
            }
            _ => {}
        }
        let at = self.context();
        let deadline = io_deadline();
        let result = deadline
            .run(
                self.inner
                    .repository
                    .interrupt_previous_runtime(at.runtime, &at, &deadline),
            )
            .await;
        let tasks = lock(&self.inner.core.tasks)?;
        if matches!(
            self.startup_status(),
            StartupStatus::Stopping | StartupStatus::Stopped
        ) {
            return Err(RegistryError::NodeStopping);
        }
        let resolved = match result {
            Ok(_) => {
                self.inner
                    .core
                    .status
                    .store(StartupStatus::Running as u8, Ordering::SeqCst);
                Ok(StartupStatus::Running)
            }
            Err(error) => {
                let status = if matches!(
                    error,
                    RegistryError::CommitUnconfirmed | RegistryError::Cancelled
                ) {
                    StartupStatus::Unconfirmed
                } else {
                    StartupStatus::Starting
                };
                self.inner.core.status.store(status as u8, Ordering::SeqCst);
                Err(error)
            }
        };
        drop(tasks);
        resolved
    }
    fn context(&self) -> OperationContext {
        OperationContext {
            runtime: self.inner.core.runtime,
            observed_at_ms: self.inner.clock.unix_millis(),
        }
    }
    #[cfg_attr(
        not(test),
        expect(
            clippy::unused_self,
            reason = "Per-runtime sealed random injection exists only in test builds"
        )
    )]
    fn registration_id(&self) -> Result<EntityId, RegistryError> {
        #[cfg(test)]
        if let Some(value) = self.inner.randoms.take(random_hooks::Point::Registration)? {
            return value;
        }
        random_id()
    }
    async fn actor(
        &self,
        actor: &A::Actor,
        target: EntityId,
        action: Option<QueueAction>,
    ) -> Result<CurrentRequestActor, RegistryError> {
        self.inner.core.running()?;
        let context = match action {
            Some(a) => self.inner.authority.authorize(actor, target, a).await?,
            None => self.inner.authority.authorize_read(actor, target).await?,
        };
        let native = self.inner.authority.resolve_target(target).await?;
        Ok(CurrentRequestActor::new(context, target, native, action))
    }
    /// # Errors
    /// The trusted validator rejects invalid complete structures without exposing their body.
    pub fn validate_body(&self, raw: &V::Raw) -> Result<ValidatedBody<V::Body>, RegistryError> {
        #[cfg(all(test, windows))]
        super::validation_test_hooks::point(
            super::validation_test_hooks::Point::Entered,
            self.queue_runtime_identity(),
        );
        let result = self
            .inner
            .validator
            .validate(raw)
            .map(|(body, digest)| ValidatedBody::new(body, digest));
        #[cfg(all(test, windows))]
        super::validation_test_hooks::point(
            super::validation_test_hooks::Point::Returned,
            self.queue_runtime_identity(),
        );
        result
    }
    /// # Errors
    /// Registers only a node-minted identity, not a queue item or dispatch qualification.
    pub fn register<'a>(
        &'a self,
        actor: &'a A::Actor,
        target: EntityId,
        mutation: &ValidatedMutation<V::Body>,
    ) -> impl Future<Output = Result<RegistrationFact, RegistryError>> + Send + 'a {
        self.register_with_id(actor, target, mutation, None)
    }
    /// # Errors
    /// Returns a registry error when authorization, identity validation, or registration fails.
    pub fn register_with_id<'a>(
        &'a self,
        actor: &'a A::Actor,
        target: EntityId,
        mutation: &ValidatedMutation<V::Body>,
        requested: Option<RegistrationId>,
    ) -> impl Future<Output = Result<RegistrationFact, RegistryError>> + Send + 'a {
        // Copy metadata synchronously so the async future never requires Body: Sync or Clone.
        let descriptor = mutation.intent(EntityId::from_bytes([0; 16]), target);
        async move {
            let descriptor = descriptor?;
            let current = self.actor(actor, target, Some(descriptor.action())).await?;
            let intent = RegisteredIntent::new(
                current.context().client_id,
                descriptor.action(),
                target,
                descriptor.item(),
                descriptor.expected_revision(),
                descriptor.body_digest().copied(),
            )?;
            let at = self.context();
            let deadline = io_deadline();
            loop {
                if Instant::now() >= deadline.deadline() {
                    return Err(RegistryError::Cancelled);
                }
                let id = match requested {
                    Some(id) => id,
                    None => RegistrationId::new(self.registration_id()?),
                };
                let registration = async {
                    if requested.is_some() {
                        self.inner
                            .repository
                            .register(&current, &intent, id, &at, &deadline)
                            .await
                    } else {
                        self.inner
                            .repository
                            .register_generated(&current, &intent, id, &at, &deadline)
                            .await
                    }
                };
                match deadline.run(registration).await {
                    Err(RegistryError::Conflict) if requested.is_none() => {}
                    result => return result,
                }
            }
        }
    }
    pub async fn consume(
        &self,
        actor: &A::Actor,
        target: EntityId,
        id: RegistrationId,
        mutation: ValidatedMutation<V::Body>,
    ) -> ConsumeReply<V::Body> {
        self.consume_inner(actor, target, id, mutation, None).await
    }
    pub(crate) async fn consume_observed(
        &self,
        actor: &A::Actor,
        target: EntityId,
        id: RegistrationId,
        mutation: ValidatedMutation<V::Body>,
        custody: Arc<super::ConsumptionCustody>,
    ) -> ConsumeReply<V::Body> {
        self.consume_inner(actor, target, id, mutation, Some(custody))
            .await
    }
    async fn consume_inner(
        &self,
        actor: &A::Actor,
        target: EntityId,
        id: RegistrationId,
        mutation: ValidatedMutation<V::Body>,
        custody: Option<Arc<super::ConsumptionCustody>>,
    ) -> ConsumeReply<V::Body> {
        let current = match self.actor(actor, target, Some(mutation.action())).await {
            Ok(actor) => Arc::new(actor),
            Err(error) => {
                return ConsumeReply::Refused {
                    error,
                    returned: mutation,
                };
            }
        };
        let intent = match mutation.intent(current.context().client_id, target) {
            Ok(i) => i,
            Err(error) => {
                return ConsumeReply::Refused {
                    error,
                    returned: mutation,
                };
            }
        };
        let owner = match self.inner.core.owner_for(current.native().clone()) {
            Ok(o) => o,
            Err(error) => {
                return ConsumeReply::Refused {
                    error,
                    returned: mutation,
                };
            }
        };
        let mut mutation = Some(mutation);
        loop {
            let notified = owner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let installed =
                match self.install_pending(&owner, current.clone(), id, &intent, &mut mutation) {
                    Ok(value) => value,
                    Err(error) => return refused(error, mutation, id),
                };
            match installed {
                Installation::Started(rx) => {
                    // install_pending has moved the mutation and registered the original worker.
                    if let Some(custody) = &custody {
                        custody.transferred.store(true, Ordering::Release);
                    }
                    return rx
                        .await
                        .unwrap_or_else(|_| ConsumeReply::Pending { request: id });
                }
                Installation::Wait => notified.await,
                Installation::Existing => {
                    let result = self.query_request(actor, target, id).await;
                    return match result {
                        Ok(fact) => ConsumeReply::Resolved {
                            fact,
                            returned: mutation.take(),
                        },
                        Err(error) => refused(error, mutation, id),
                    };
                }
            }
        }
    }
    fn install_pending(
        &self,
        owner: &Arc<OwnerCell<V::Body>>,
        current: Arc<CurrentRequestActor>,
        id: RegistrationId,
        intent: &RegisteredIntent,
        mutation: &mut Option<ValidatedMutation<V::Body>>,
    ) -> Result<Installation<V::Body>, RegistryError> {
        let mut tasks = lock(&self.inner.core.tasks)?;
        self.inner.core.reap_completed(&mut tasks);
        let mut state = lock(&owner.state)?;
        self.inner.core.running()?;
        let installed = match state.pending.as_ref() {
            Some(p) if p.probe.registration_id() == id => {
                if p.probe.intent() != intent {
                    return Err(RegistryError::Conflict);
                }
                Installation::Existing
            }
            Some(_) => Installation::Wait,
            None => {
                let operation = random_id()?;
                let context = self.context();
                let ticket = Arc::new(());
                let probe = Arc::new(OperationProbe::new(
                    id,
                    operation,
                    &context,
                    &current,
                    intent.clone(),
                ));
                let (tx, rx) = oneshot::channel();
                state.pending = Some(Pending {
                    probe,
                    actor: current,
                    context,
                    phase: ProcessingPhase::AwaitBegin,
                    applied: false,
                    worker: true,
                    worker_ticket: ticket.clone(),
                    mutation: mutation.take(),
                    returned: None,
                    retired: Vec::new(),
                    outcome: None,
                    lease: None,
                    response: Some(tx),
                });
                let service = self.clone();
                let job_owner = owner.clone();
                let worker = PendingWorker {
                    owner: owner.clone(),
                    operation,
                    ticket,
                };
                let handle = tokio::spawn(async move {
                    service.run_begin(job_owner, operation, worker).await;
                });
                tasks.push(TaskRecord::new(handle, None, false));
                Installation::Started(rx)
            }
        };
        drop(state);
        drop(tasks);
        Ok(installed)
    }
    /// # Errors
    /// Rechecks the current reader; it never resumes an operation.
    pub async fn query_request(
        &self,
        actor: &A::Actor,
        target: EntityId,
        id: RegistrationId,
    ) -> Result<RequestFact, RegistryError> {
        let reader = self.actor(actor, target, None).await?;
        let deadline = io_deadline();
        deadline
            .run(self.inner.repository.read(&reader, target, id, &deadline))
            .await
    }
    /// # Errors
    /// Exposes a borrowed, explicitly tentative view while a management operation is pending.
    #[cfg(windows)]
    pub(crate) async fn snapshot_queued_body<P>(
        &self,
        actor: &A::Actor,
        target: EntityId,
        item: EntityId,
        revision: u64,
        destination: &crate::application::document_disposal::DocumentPayloadSlot<P>,
    ) -> Result<super::QueuedBodySnapshot<V::Body, P>, super::QueueDocumentError>
    where
        V::Body: crate::application::shared_views::RetainedRaw,
        P: crate::application::shared_views::RetainedRaw + From<V::Body>,
    {
        use crate::application::{document_source::DocumentEpoch, shared_views::RetainedRaw};
        let current = self.actor(actor, target, None).await?;
        destination.verify_brand(
            &self.inner.core.registry_owner,
            &self.queue_runtime_identity(),
        )?;
        let owner = self.inner.core.owner_for(current.native().clone())?;
        let mut state = lock(&owner.state)?;
        if state.queue.active().is_some_and(|e| e.id() == item) {
            return Err(QueueError::DispatchStarted.into());
        }
        let entry = state
            .queue
            .waiting()
            .find(|e| e.id() == item)
            .ok_or(QueueError::NotFound)?;
        if entry.revision() != revision {
            return Err(QueueError::RevisionChanged.into());
        }
        let digest = *entry.input().body.digest();
        let total = u64::try_from(entry.input().body.body().len())
            .map_err(|_| crate::application::document_source::DocumentError::Allocation)?;
        destination.install_from_queue(
            &self.inner.core.registry_owner,
            &self.queue_runtime_identity(),
            || entry.input().body.body().clone().into(),
        )?;
        let epoch = if let Some(epoch) = state
            .document_epochs
            .get(&(item, revision))
            .and_then(std::sync::Weak::upgrade)
        {
            epoch
        } else {
            state
                .document_epochs
                .try_reserve(1)
                .map_err(|_| crate::application::document_source::DocumentError::Allocation)?;
            let epoch = DocumentEpoch::new();
            state
                .document_epochs
                .insert((item, revision), Arc::downgrade(&epoch));
            epoch
        };
        let processing =
            state
                .pending
                .as_ref()
                .map(|p| super::document_read::QueueSnapshotProcessing {
                    phase: p.phase,
                    applied: p.applied,
                    request: p.probe.registration_id(),
                });
        drop(state);
        Ok(super::document_read::QueuedBodySnapshot::from_original(
            &self.inner.core,
            &owner,
            target,
            item,
            revision,
            total,
            digest,
            epoch,
            destination.clone(),
            processing,
        ))
    }
    /// # Errors
    /// Returns a registry error when authorization or the authoritative queue lookup fails.
    pub async fn inspect_queue<U: Send>(
        &self,
        actor: &A::Actor,
        target: EntityId,
        f: impl for<'a> FnOnce(QueueRead<'a, V::Body>) -> U + Send,
    ) -> Result<U, RegistryError> {
        let reader = self.actor(actor, target, None).await?;
        let owner = self.inner.core.owner_for(reader.native().clone())?;
        let state = lock(&owner.state)?;
        let view = state.pending.as_ref().map_or_else(
            || QueueRead::Stable {
                queue: &state.queue,
            },
            |p| QueueRead::Processing {
                phase: p.phase,
                applied: p.applied,
                request: p.probe.registration_id(),
                queue: &state.queue,
            },
        );
        let result = f(view);
        drop(state);
        Ok(result)
    }
    #[cfg(test)]
    pub(crate) async fn suspend_waiting_for_test(
        &self,
        actor: &A::Actor,
        target: EntityId,
        reason: crate::domain::native_dispatch::KnownStopReason,
    ) -> Result<usize, RegistryError> {
        let reader = self.actor(actor, target, None).await?;
        let owner = self.inner.core.owner_for(reader.native().clone())?;
        let changed = {
            let mut state = lock(&owner.state)?;
            state.queue.suspend_waiting_with_reason(reason)
        };
        owner.changed.notify_waiters();
        Ok(changed)
    }
    async fn run_begin(
        &self,
        owner: Arc<OwnerCell<V::Body>>,
        operation: EntityId,
        worker: PendingWorker<V::Body>,
    ) {
        let ticket = worker.ticket.clone();
        let _worker = worker;
        let args = lock(&owner.state).ok().and_then(|s| {
            s.pending
                .as_ref()
                .filter(|p| p.probe.operation() == operation)
                .map(|p| (p.actor.clone(), p.probe.clone(), p.context.clone()))
        });
        let Some((actor, probe, at)) = args else {
            return;
        };
        let deadline = io_deadline();
        let result = deadline
            .run(self.inner.repository.begin_consume(
                &actor,
                probe.registration_id(),
                probe.intent(),
                operation,
                &at,
                &deadline,
            ))
            .await;
        match result {
            Ok(BeginRecord::Started(record)) => {
                match self.accept_started(&owner, operation, record) {
                    Ok(()) => {
                        let _ = self.settle_pending(&owner, operation).await;
                    }
                    Err(error) => Self::record_error(&owner, operation, &ticket, error),
                }
            }
            Ok(BeginRecord::Existing(fact)) => {
                let _ = self.finish(&owner, operation, fact);
            }
            Err(error) => Self::record_error(&owner, operation, &ticket, error),
        }
    }
    fn accept_started(
        &self,
        owner: &OwnerCell<V::Body>,
        operation: EntityId,
        record: StoredApplying,
    ) -> Result<(), RegistryError> {
        #[cfg(windows)]
        let _document_wake = super::document_read::DocumentWakeOnExit::new(owner);
        let mut state = lock(&owner.state)?;
        self.inner
            .core
            .drain(owner, &mut state)
            .map_err(driver_registry)?;
        let mut pending = state
            .pending
            .take()
            .ok_or(RegistryError::NoOwnedOperation)?;
        if pending.probe.operation() != operation {
            state.pending = Some(pending);
            return Err(RegistryError::StaleObservation);
        }
        let mut failed_priority = None;
        let result = (|| {
            if !pending.probe.matches(&record) {
                return Err(RegistryError::CorruptState);
            }
            if pending.applied {
                if pending
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.stored() != &record)
                {
                    return Err(RegistryError::CorruptState);
                }
            } else {
                // Closing may finish an existing applied receipt, but must not mint a fresh
                // queue application from a merely committed/unconfirmed begin.
                self.inner.core.running()?;
                let lease = ConsumeLease::mint(&pending.probe, record)?;
                pending.phase = ProcessingPhase::ReadyToApply;
                let mutation = pending.mutation.take().ok_or(RegistryError::CorruptState)?;
                let applied = apply(
                    owner,
                    &mut state,
                    mutation,
                    &pending.probe,
                    self.inner.core.readiness(owner),
                )?;
                pending.applied = true;
                pending.outcome = Some(applied.outcome);
                pending.returned = applied.returned;
                pending.retired = applied.retired;
                pending.lease = Some(Arc::new(lease));
                failed_priority = applied.failed_priority;
            }
            pending.phase = ProcessingPhase::AwaitSettle;
            Ok(())
        })();
        state.pending = Some(pending);
        drop(state);
        #[cfg(windows)]
        owner.notify_documents();
        if let Some(intent) = failed_priority {
            self.inner
                .core
                .environment
                .priority_failed(&owner.native, intent);
        }
        result
    }
    async fn settle_pending(
        &self,
        owner: &OwnerCell<V::Body>,
        operation: EntityId,
    ) -> Result<RequestFact, RegistryError> {
        let (lease, outcome, ticket) = {
            let state = lock(&owner.state)?;
            let p = state
                .pending
                .as_ref()
                .ok_or(RegistryError::NoOwnedOperation)?;
            if p.probe.operation() != operation {
                return Err(RegistryError::StaleObservation);
            }
            let args = (
                p.lease.clone().ok_or(RegistryError::CorruptState)?,
                p.outcome.ok_or(RegistryError::CorruptState)?,
                p.worker_ticket.clone(),
            );
            drop(state);
            args
        };
        let at = self.context();
        let deadline = io_deadline();
        let result = deadline
            .run(
                self.inner
                    .repository
                    .settle(&lease, &outcome, &at, &deadline),
            )
            .await;
        match result {
            Ok(fact) => match &fact {
                RequestFact::Settled {
                    registration,
                    outcome: actual,
                } if registration == &lease.stored().registration && *actual == outcome => {
                    self.finish(owner, operation, fact.clone())?;
                    Ok(fact)
                }
                _ => {
                    Self::record_error(owner, operation, &ticket, RegistryError::CorruptState);
                    Err(RegistryError::CorruptState)
                }
            },
            Err(error) => {
                Self::record_error(owner, operation, &ticket, error);
                Err(error)
            }
        }
    }
    fn finish(
        &self,
        owner: &OwnerCell<V::Body>,
        operation: EntityId,
        fact: RequestFact,
    ) -> Result<(), RegistryError> {
        let mut p = {
            let mut state = lock(&owner.state)?;
            let p = state
                .pending
                .as_ref()
                .ok_or(RegistryError::NoOwnedOperation)?;
            if p.probe.operation() != operation {
                return Err(RegistryError::StaleObservation);
            }
            if self.inner.core.native_enabled.load(Ordering::SeqCst)
                && p.applied
                && let RequestFact::Settled {
                    registration,
                    outcome: QueueOutcome::Prioritized { item, revision },
                } = &fact
                && registration
                    == &p
                        .lease
                        .as_ref()
                        .ok_or(RegistryError::CorruptState)?
                        .stored()
                        .registration
                && state.queue.priority_matches(*item, *revision)
            {
                if state.priority_handoff.is_some() {
                    return Err(RegistryError::Conflict);
                }
                let selection = (p.probe.operation(), *item, *revision);
                let handoff = super::PriorityHandoff {
                    identity: owner.identity.clone(),
                    target: registration.intent().target(),
                    item: *item,
                    revision: *revision,
                    resource: self.inner.core.environment.resource_epoch(&owner.native),
                    operation: p.probe.operation(),
                    registration: p.probe.registration_id(),
                };
                self.inner
                    .core
                    .environment
                    .priority_accepted(&owner.native, selection.0);
                state.selected_priority = Some(selection);
                state.priority_handoff = Some(handoff);
            }
            let p = state
                .pending
                .take()
                .ok_or(RegistryError::NoOwnedOperation)?;
            drop(state);
            p
        };
        let audit = p.outcome.map(|outcome| AuditMetadata {
            request: p.probe.registration_id(),
            actor: p.probe.intent().actor(),
            action: p.probe.intent().action(),
            target: p.probe.intent().target(),
            outcome,
            observed_at_ms: self.inner.clock.unix_millis(),
        });
        let returned = p.returned.take().or_else(|| p.mutation.take());
        if let Some(response) = p.response.take() {
            let _ = response.send(ConsumeReply::Resolved { fact, returned });
        }
        drop(p);
        owner.changed.notify_waiters();
        self.inner.core.environment.queue_changed(&owner.native);
        if let Some(audit) = audit {
            self.spawn_audit(audit);
        }
        Ok(())
    }
    fn record_error(
        owner: &OwnerCell<V::Body>,
        operation: EntityId,
        ticket: &Arc<()>,
        error: RegistryError,
    ) {
        let Ok(mut state) = lock(&owner.state) else {
            return;
        };
        let Some(p) = state
            .pending
            .as_mut()
            .filter(|p| p.probe.operation() == operation && Arc::ptr_eq(&p.worker_ticket, ticket))
        else {
            return;
        };
        if !p.applied
            && matches!(
                error,
                RegistryError::InvalidActor
                    | RegistryError::InvalidTarget
                    | RegistryError::InvalidIntent
                    | RegistryError::UnknownRegistration
                    | RegistryError::Conflict
                    | RegistryError::Retired
                    | RegistryError::RandomUnavailable
            )
        {
            let pending = state.pending.take();
            drop(state);
            if let Some(mut p) = pending
                && let (Some(response), Some(returned)) = (p.response.take(), p.mutation.take())
            {
                let _ = response.send(ConsumeReply::Refused { error, returned });
            }
            owner.changed.notify_waiters();
            return;
        }
        p.worker = false;
        p.phase = if error == RegistryError::CorruptState {
            ProcessingPhase::Faulted
        } else if p.applied {
            ProcessingPhase::SettleUnconfirmed
        } else {
            ProcessingPhase::BeginUnconfirmed
        };
        if let Some(response) = p.response.take() {
            let _ = response.send(ConsumeReply::Pending {
                request: p.probe.registration_id(),
            });
        }
    }
    /// # Errors
    /// Reconciles only this owner's existing operation, never reconstructing a queue from SQL.
    pub async fn reconcile(
        &self,
        target: EntityId,
        id: RegistrationId,
    ) -> Result<RequestFact, RegistryError> {
        self.inner.core.running()?;
        let owners = self.inner.core.owners_snapshot()?;
        let owner = owners
            .into_iter()
            .find(|o| {
                lock(&o.state).is_ok_and(|s| {
                    s.pending.as_ref().is_some_and(|p| {
                        p.probe.registration_id() == id && p.probe.intent().target() == target
                    })
                })
            })
            .ok_or(RegistryError::NoOwnedOperation)?;
        let (tx, rx) = oneshot::channel();
        let inspection = {
            let mut tasks = lock(&self.inner.core.tasks)?;
            self.inner.core.reap_completed(&mut tasks);
            self.inner.core.running()?;
            let mut state = lock(&owner.state)?;
            let p = state
                .pending
                .as_mut()
                .filter(|p| p.probe.registration_id() == id && p.probe.intent().target() == target)
                .ok_or(RegistryError::NoOwnedOperation)?;
            // The identity, probe and worker transition belong to this one pending slot.
            // A previous operation may have completed since the candidate owner was found.
            let probe = p.probe.clone();
            if p.worker {
                drop(state);
                drop(tasks);
                Some(probe)
            } else {
                p.worker = true;
                p.worker_ticket = Arc::new(());
                let ticket = p.worker_ticket.clone();
                drop(state);
                let service = self.clone();
                let job_owner = owner.clone();
                let operation = probe.operation();
                let worker = PendingWorker {
                    owner: owner.clone(),
                    operation,
                    ticket: ticket.clone(),
                };
                tasks.push(TaskRecord::new(
                    tokio::spawn(async move {
                        let _worker = worker;
                        let result = service.run_reconcile(&job_owner, &probe).await;
                        if let Err(error) = result {
                            Self::record_error(&job_owner, operation, &ticket, error);
                        }
                        let _ = tx.send(result);
                    }),
                    None,
                    false,
                ));
                drop(tasks);
                None
            }
        };
        if let Some(probe) = inspection {
            let deadline = io_deadline();
            return deadline
                .run(self.inner.repository.inspect_operation(&probe, &deadline))
                .await;
        }
        rx.await.map_err(|_| RegistryError::CommitUnconfirmed)?
    }
    async fn run_reconcile(
        &self,
        owner: &Arc<OwnerCell<V::Body>>,
        probe: &OperationProbe,
    ) -> Result<RequestFact, RegistryError> {
        let deadline = io_deadline();
        let fact = deadline
            .run(self.inner.repository.inspect_operation(probe, &deadline))
            .await?;
        match fact {
            RequestFact::Applying(record) => {
                self.accept_started(owner, probe.operation(), record)?;
                self.settle_pending(owner, probe.operation()).await
            }
            RequestFact::Registered(registration) => {
                if lock(&owner.state)?
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.applied)
                {
                    return Err(RegistryError::CorruptState);
                }
                let fact = RequestFact::Registered(registration);
                self.finish(owner, probe.operation(), fact.clone())?;
                Ok(fact)
            }
            RequestFact::Settled {
                ref registration,
                outcome,
            } => {
                let valid = lock(&owner.state)?.pending.as_ref().is_some_and(|p| {
                    p.applied
                        && p.outcome == Some(outcome)
                        && p.lease
                            .as_ref()
                            .is_some_and(|l| &l.stored().registration == registration)
                });
                if !valid {
                    return Err(RegistryError::CorruptState);
                }
                self.finish(owner, probe.operation(), fact.clone())?;
                Ok(fact)
            }
            _ => Err(RegistryError::CorruptState),
        }
    }
    /// # Errors
    /// Processes trusted observations without waiting for any pending SQL operation.
    pub async fn poll_target(&self, target: EntityId) -> Result<(), RegistryError> {
        let owner = self.inner.core.owner(target).await?;
        #[cfg(windows)]
        let _document_wake = super::document_read::DocumentWakeOnExit::new(&owner);
        let mut state = lock(&owner.state)?;
        self.inner
            .core
            .drain(&owner, &mut state)
            .map_err(driver_registry)
    }
    fn spawn_audit(&self, metadata: AuditMetadata) {
        let Ok(mut tasks) = lock(&self.inner.core.tasks) else {
            return;
        };
        self.inner.core.reap_completed(&mut tasks);
        if matches!(
            self.startup_status(),
            StartupStatus::Stopping | StartupStatus::Stopped
        ) {
            self.audit_warning(metadata.request, AuditFailure::Cancelled, None);
            return;
        }
        let service = self.clone();
        let audit_task = Arc::new(AuditTask {
            request: metadata.request,
            cancelled: AtomicBool::new(false),
        });
        let observed_task = audit_task.clone();
        tasks.push(TaskRecord::new(tokio::spawn(async move {
            let stop=service.inner.core.stop.notified();tokio::pin!(stop);stop.as_mut().enable();
            let result=if matches!(service.startup_status(),StartupStatus::Stopping|StartupStatus::Stopped){Err(AuditFailure::Cancelled)}else{
                tokio::select! {
                    ()=&mut stop=>Err(AuditFailure::Cancelled),
                    result=tokio::time::timeout(Duration::from_secs(5),service.inner.audit.append(&metadata))=>result.map_err(|_|AuditFailure::Cancelled).and_then(|v|v),
                }
            };
            if let Err(failure)=result {service.audit_warning(metadata.request,failure,Some(&observed_task));}
        }),Some(audit_task),false));
    }
    fn audit_warning(
        &self,
        request: RegistrationId,
        failure: AuditFailure,
        task: Option<&AuditTask>,
    ) {
        if failure == AuditFailure::Cancelled {
            if task.is_some_and(|task| task.cancelled.swap(true, Ordering::SeqCst)) {
                return;
            }
            let _ = self.inner.core.audit_cancelled.fetch_update(
                Ordering::SeqCst,
                Ordering::SeqCst,
                |n| n.checked_add(1),
            );
        }
        if let Ok(mut warnings) = lock(&self.inner.core.warnings) {
            warnings.push(AuditWarning { request, failure });
        }
    }
    #[must_use]
    pub fn take_audit_warnings(&self) -> Vec<AuditWarning> {
        lock(&self.inner.core.warnings)
            .map(|mut w| std::mem::take(&mut *w))
            .unwrap_or_default()
    }
    fn release_claim_under_fence(&self, _fence: &std::sync::MutexGuard<'_, Vec<Arc<TaskRecord>>>) {
        #[cfg(all(test, windows))]
        super::validation_test_hooks::point(
            super::validation_test_hooks::Point::StoppedBeforeClaimRelease,
            self.queue_runtime_identity(),
        );
        #[cfg(windows)]
        let deferred = self
            .inner
            .core
            .claim_release_deferred
            .load(Ordering::SeqCst);
        #[cfg(not(windows))]
        let deferred = false;
        if !self.inner.core.native_enabled.load(Ordering::SeqCst) && !deferred {
            self.inner.core.claim.store(false, Ordering::SeqCst);
        }
    }
    #[cfg(windows)]
    pub(crate) fn release_preparation_claim(&self) {
        let _fence = super::teardown_lock(&self.inner.core.tasks);
        if self.startup_status() == StartupStatus::Stopped
            && self
                .inner
                .core
                .claim_release_deferred
                .swap(false, Ordering::SeqCst)
        {
            self.inner.core.claim.store(false, Ordering::SeqCst);
        }
    }
    #[must_use]
    pub fn task_failures(&self) -> u64 {
        if let Ok(mut tasks) = lock(&self.inner.core.tasks) {
            self.inner.core.reap_completed(&mut tasks);
        }
        self.inner.core.task_failures.load(Ordering::SeqCst)
    }
    pub async fn shutdown(&self, deadline: RegistryDeadline) -> ShutdownReport {
        #[cfg(all(test, windows))]
        super::validation_test_hooks::point(
            super::validation_test_hooks::Point::ShutdownBeforeTaskFence,
            self.queue_runtime_identity(),
        );
        {
            let tasks = teardown_lock(&self.inner.core.tasks);
            if self.startup_status() == StartupStatus::Stopped {
                return ShutdownReport {
                    joined: true,
                    unresolved: Vec::new(),
                    audit_cancelled: self.inner.core.audit_cancelled.load(Ordering::SeqCst),
                };
            }
            // The same registration fence is held by consume/reconcile/audit when adding jobs.
            self.inner
                .core
                .status
                .store(StartupStatus::Stopping as u8, Ordering::SeqCst);
            #[cfg(all(test, windows))]
            super::validation_test_hooks::point(
                super::validation_test_hooks::Point::StoppingUnderTaskFence,
                self.queue_runtime_identity(),
            );
            for task in &*tasks {
                if task.startup {
                    task.abort.abort();
                }
            }
        }
        self.inner.core.stop.notify_waiters();
        let owners = teardown_lock(&self.inner.core.owners)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        #[cfg(windows)]
        for owner in &owners {
            let mut state = teardown_lock(&owner.state);
            let epochs = std::mem::take(&mut state.document_epochs);
            for epoch in epochs.into_values().filter_map(|e| e.upgrade()) {
                state.document_wakes.push(epoch.invalidate());
            }
            drop(state);
            owner.notify_documents();
        }
        let Ok(shutdown) = deadline
            .run(async { Ok(self.inner.core.shutdown.lock().await) })
            .await
        else {
            for task in &*teardown_lock(&self.inner.core.tasks) {
                task.abort.abort();
            }
            release_waiters(&owners);
            return ShutdownReport {
                joined: false,
                unresolved: pending_ids(&owners),
                audit_cancelled: self.inner.core.audit_cancelled.load(Ordering::SeqCst),
            };
        };
        if self.startup_status() == StartupStatus::Stopped {
            return ShutdownReport {
                joined: true,
                unresolved: Vec::new(),
                audit_cancelled: self.inner.core.audit_cancelled.load(Ordering::SeqCst),
            };
        }
        self.finish_applied_on_close(&owners, &deadline).await;
        let tasks = teardown_lock(&self.inner.core.tasks).clone();
        let mut joined = true;
        for task in &tasks {
            if task.join(&deadline).await.is_err() {
                joined = false;
                if let Some(audit) = &task.audit {
                    self.audit_warning(audit.request, AuditFailure::Cancelled, Some(audit));
                }
                task.abort.abort();
            } else {
                self.inner.core.record_task_failure(task);
            }
        }
        let unresolved = pending_ids(&owners);
        if joined {
            let mut registered = teardown_lock(&self.inner.core.tasks);
            registered.clear();
            teardown_lock(&self.inner.core.owners).clear();
            self.inner
                .core
                .status
                .store(StartupStatus::Stopped as u8, Ordering::SeqCst);
            self.release_claim_under_fence(&registered);
            drop(registered);
        } else {
            release_waiters(&owners);
        }
        drop(shutdown);
        ShutdownReport {
            joined,
            unresolved,
            audit_cancelled: self.inner.core.audit_cancelled.load(Ordering::SeqCst),
        }
    }

    async fn finish_applied_on_close(
        &self,
        owners: &[Arc<OwnerCell<V::Body>>],
        deadline: &RegistryDeadline,
    ) {
        for owner in owners {
            owner.changed.notify_waiters();
            let pending = {
                let mut state = teardown_lock(&owner.state);
                state
                    .pending
                    .as_mut()
                    .filter(|p| !p.worker && p.applied)
                    .map(|p| {
                        p.worker = true;
                        p.worker_ticket = Arc::new(());
                        (p.probe.clone(), p.worker_ticket.clone())
                    })
            };
            if let Some((probe, ticket)) = pending {
                // This future owns no detached task. Cancellation drops its SQL connection and
                // this guard restores the same pending fence; no body/lease leaves the owner.
                let _worker = PendingWorker {
                    owner: owner.clone(),
                    operation: probe.operation(),
                    ticket: ticket.clone(),
                };
                if let Err(error) = deadline.run(self.run_reconcile(owner, &probe)).await {
                    Self::record_error(owner, probe.operation(), &ticket, error);
                }
            }
        }
    }
}

impl<A, V, R, C> RuntimeSupervisor for QueueService<A, V, R, C>
where
    A: RequestAuthority + 'static,
    V: ValidatedBodyPort + 'static,
    R: RequestRegistryRepository + 'static,
    C: NodeClock + 'static,
{
    #[cfg(windows)]
    fn queue_runtime_identity(&self) -> super::QueueRuntimeIdentity {
        super::QueueRuntimeIdentity(self.inner.core.identity.clone())
    }
    #[cfg(windows)]
    fn register_preparation(
        &self,
        expected: &super::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        // Also held by direct shutdown for Running -> Stopping and Stopped/claim release.
        let _fence = lock(&self.inner.core.tasks)?;
        self.inner.core.running()?;
        if !self.queue_runtime_identity().same(expected)
            || !self.inner.core.claim.load(Ordering::SeqCst)
        {
            return Err(RegistryError::InvalidActor);
        }
        if self
            .inner
            .core
            .claim_release_deferred
            .load(Ordering::SeqCst)
        {
            return Err(RegistryError::RuntimeAlreadyStarted);
        }
        install()?;
        #[cfg(all(test, windows))]
        super::validation_test_hooks::point(
            super::validation_test_hooks::Point::PreparationInstalledUnderTaskFence,
            self.queue_runtime_identity(),
        );
        self.inner
            .core
            .claim_release_deferred
            .store(true, Ordering::SeqCst);
        Ok(())
    }
    #[cfg(windows)]
    fn register_document(
        &self,
        expected: &super::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        let _fence = lock(&self.inner.core.tasks)?;
        self.inner.core.running()?;
        if !self.queue_runtime_identity().same(expected)
            || !self.inner.core.claim.load(Ordering::SeqCst)
        {
            return Err(RegistryError::InvalidActor);
        }
        install()?;
        self.inner
            .core
            .claim_release_deferred
            .store(true, Ordering::SeqCst);
        Ok(())
    }
    #[cfg(windows)]
    fn register_catalog(
        &self,
        expected: &super::QueueRuntimeIdentity,
        install: &mut dyn FnMut() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        let _fence = lock(&self.inner.core.tasks)?;
        self.inner.core.running()?;
        if !self.queue_runtime_identity().same(expected)
            || !self.inner.core.claim.load(Ordering::SeqCst)
        {
            return Err(RegistryError::InvalidActor);
        }
        install()?;
        self.inner
            .core
            .claim_release_deferred
            .store(true, Ordering::SeqCst);
        Ok(())
    }
    fn cancel(&self) {
        let tasks = teardown_lock(&self.inner.core.tasks);
        if self.startup_status() == StartupStatus::Stopped {
            return;
        }
        self.inner
            .core
            .status
            .store(StartupStatus::Stopping as u8, Ordering::SeqCst);
        self.inner.core.stop.notify_waiters();
        for task in &*tasks {
            task.abort.abort();
        }
        drop(tasks);
    }
    fn stop_and_join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            loop {
                let report = self
                    .shutdown(RegistryDeadline::new(
                        Instant::now() + Duration::from_secs(5),
                    ))
                    .await;
                if report.joined {
                    break;
                }
            }
        })
    }
}

fn refused<T>(
    error: RegistryError,
    mutation: Option<ValidatedMutation<T>>,
    id: RegistrationId,
) -> ConsumeReply<T> {
    // A missing mutation is already owned by a node task; never invent a body to return.
    mutation.map_or_else(
        || ConsumeReply::Pending { request: id },
        |returned| ConsumeReply::Refused { error, returned },
    )
}
const fn driver_registry(error: DriverError) -> RegistryError {
    match error {
        DriverError::Registry(e) => e,
        _ => RegistryError::CorruptState,
    }
}
const fn rejected(error: QueueError) -> Result<QueueRejection, RegistryError> {
    match error {
        QueueError::Full => Ok(QueueRejection::Full),
        QueueError::DuplicateId => Ok(QueueRejection::DuplicateId),
        QueueError::NotFound => Ok(QueueRejection::NotFound),
        QueueError::RevisionChanged => Ok(QueueRejection::RevisionChanged),
        QueueError::RevisionExhausted => Ok(QueueRejection::RevisionExhausted),
        QueueError::DispatchStarted => Ok(QueueRejection::DispatchStarted),
        QueueError::Suspended => Ok(QueueRejection::Suspended),
        QueueError::SequenceExhausted => Ok(QueueRejection::SequenceExhausted),
        _ => Err(RegistryError::CorruptState),
    }
}
struct Applied<T> {
    outcome: QueueOutcome,
    returned: Option<ValidatedMutation<T>>,
    retired: Vec<ValidatedBody<T>>,
    failed_priority: Option<EntityId>,
}
#[expect(
    clippy::too_many_lines,
    reason = "The accepted queue mutation and its origin bookkeeping execute as one original R3 state transition."
)]
fn apply<T>(
    owner: &OwnerCell<T>,
    state: &mut OwnerState<T>,
    mutation: ValidatedMutation<T>,
    probe: &OperationProbe,
    readiness: DispatchReadiness,
) -> Result<Applied<T>, RegistryError> {
    let next_order = state
        .native_order
        .checked_add(1)
        .ok_or(RegistryError::StorageUnavailable)?;
    state.native_order = next_order;
    state
        .canonical_target
        .get_or_insert_with(|| probe.intent().target());
    let mut retired = Vec::new();
    let mut returned = None;
    let mut failed_priority = None;
    let outcome = match mutation {
        ValidatedMutation::Submit(body) => match state.queue.submit(
            QueueInput {
                id: probe.registration_id().entity(),
                submitter: probe.intent().actor(),
                body,
            },
            readiness,
        ) {
            Ok(Submission::Queued { id, revision }) => QueueOutcome::Queued { item: id, revision },
            Ok(Submission::Dispatching(token)) => {
                let DispatchReadiness::Ready(resource) = readiness else {
                    return Err(RegistryError::CorruptState);
                };
                let entry = state
                    .queue
                    .dispatch_input(&token)
                    .map_err(|_| RegistryError::CorruptState)?;
                let key = ActiveKey {
                    identity: owner.identity.clone(),
                    item: entry.id(),
                    revision: entry.revision(),
                    resource,
                    selection_order: next_order,
                };
                let outcome = QueueOutcome::DirectSelected {
                    item: key.item,
                    revision: key.revision,
                };
                state.active = Some((key, token));
                outcome
            }
            Err(error) => {
                returned = Some(ValidatedMutation::Submit(error.value.body));
                QueueOutcome::Rejected(rejected(error.error)?)
            }
        },
        ValidatedMutation::Edit {
            item,
            revision,
            body,
        } => match state
            .queue
            .edit(item, revision, probe.intent().actor(), body)
        {
            Ok(old) => {
                #[cfg(windows)]
                super::document_read::invalidate(state, item, revision);
                retired.push(old);
                if state
                    .selected_priority
                    .is_some_and(|(_, selected, _)| selected == item)
                {
                    failed_priority =
                        state.fail_selected_priority(super::PriorityFailure::SelectionChanged);
                }
                QueueOutcome::Edited {
                    item,
                    revision: revision.checked_add(1).ok_or(RegistryError::CorruptState)?,
                }
            }
            Err(error) => {
                returned = Some(ValidatedMutation::Edit {
                    item,
                    revision,
                    body: error.value,
                });
                QueueOutcome::Rejected(rejected(error.error)?)
            }
        },
        ValidatedMutation::Cancel { item, revision } => match state.queue.cancel(item, revision) {
            Ok(entry) => {
                #[cfg(windows)]
                super::document_read::invalidate(state, item, revision);
                retired.push(entry.into_input().body);
                if state
                    .selected_priority
                    .is_some_and(|(_, selected, _)| selected == item)
                {
                    failed_priority =
                        state.fail_selected_priority(super::PriorityFailure::SelectionChanged);
                }
                QueueOutcome::Cancelled { item, revision }
            }
            Err(error) => QueueOutcome::Rejected(rejected(error)?),
        },
        ValidatedMutation::Prioritize { item, revision } => {
            match if state.priority_handoff.is_some() {
                Err(QueueError::Busy)
            } else {
                state.queue.prioritize(item, revision)
            } {
                Ok(()) => QueueOutcome::Prioritized { item, revision },
                Err(error) => QueueOutcome::Rejected(rejected(error)?),
            }
        }
        ValidatedMutation::Resume => QueueOutcome::Resumed {
            restored: u8::try_from(state.queue.resume())
                .map_err(|_| RegistryError::CorruptState)?,
        },
    };
    probe
        .intent()
        .validate_outcome(probe.registration_id(), outcome)?;
    match outcome {
        QueueOutcome::Queued { item, revision }
        | QueueOutcome::DirectSelected { item, revision } => {
            state.origins.insert(
                item,
                crate::domain::native_dispatch::ContentOrigin {
                    submit_request: probe.registration_id(),
                    content_request: probe.registration_id(),
                    revision,
                    digest: *probe
                        .intent()
                        .body_digest()
                        .ok_or(RegistryError::CorruptState)?,
                },
            );
        }
        QueueOutcome::Edited { item, revision } => {
            let origin = state
                .origins
                .get_mut(&item)
                .ok_or(RegistryError::CorruptState)?;
            origin.content_request = probe.registration_id();
            origin.revision = revision;
            origin.digest = *probe
                .intent()
                .body_digest()
                .ok_or(RegistryError::CorruptState)?;
        }
        QueueOutcome::Cancelled { item, .. } => {
            state.origins.remove(&item);
        }
        _ => {}
    }
    Ok(Applied {
        outcome,
        returned,
        retired,
        failed_priority,
    })
}
