//! Node-only queue adaptation. Slots retain the original consume future until its owner takes over.
#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    input_upload::{NodeInputUploads, NodePreparedBodyLease, UploadPurpose},
    session_services::ProductQueue,
};
use crate::{
    adapters::acp::PromptDocument,
    application::{
        queue_owner::{ConsumeReply, ConsumptionCustody, ValidatedMutation},
        shared_views::socket_authority::{
            LiveSocketContext, SocketLifetimeParticipant, SocketViewAuthority, SocketViewClient,
        },
    },
    claims::Id128,
    domain::{
        EntityId,
        request_registry::{
            QueueAction, RegisteredIntent, RegistrationFact, RegistrationId, RegistryError,
            RequestFact,
        },
        shared_view::ViewError,
    },
    management_ws::input_upload::UploadError,
};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
};
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NodeQueueError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Upload(#[from] UploadError),
    #[error("socket is closed")]
    SocketClosed,
    #[error("node queue management is stopping")]
    NodeStopping,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeQueueMutation {
    Submit {
        body: Id128,
    },
    Edit {
        item: EntityId,
        revision: u64,
        body: Id128,
    },
    Cancel {
        item: EntityId,
        revision: u64,
    },
    Prioritize {
        item: EntityId,
        revision: u64,
    },
    Resume,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeConsumeResult {
    Observed {
        fact: RequestFact,
    },
    Resolved {
        fact: RequestFact,
        returned: bool,
    },
    Pending {
        request: RegistrationId,
    },
    Refused {
        error: RegistryError,
        returned: bool,
    },
}
type OriginalConsume = Pin<Box<dyn Future<Output = ConsumeReply<PromptDocument>> + Send>>;
struct Slot {
    future: Option<OriginalConsume>,
    lease: Option<NodePreparedBodyLease>,
    result: Option<Result<NodeConsumeResult, NodeQueueError>>,
}
struct OperationWake {
    changed: Notify,
}
impl Wake for OperationWake {
    fn wake(self: Arc<Self>) {
        self.changed.notify_waiters();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.changed.notify_waiters();
    }
}
struct Operation {
    context: Arc<LiveSocketContext>,
    authority: Arc<SocketViewAuthority>,
    intent: RegisteredIntent,
    body: Option<Id128>,
    custody: Arc<ConsumptionCustody>,
    wake: Arc<OperationWake>,
    slot: Mutex<Slot>,
}
struct Table {
    sealed: bool,
    operations: HashMap<RegistrationId, Arc<Operation>>,
}
struct Core {
    queue: ProductQueue,
    uploads: Arc<NodeInputUploads>,
    authority: Arc<SocketViewAuthority>,
    table: Mutex<Table>,
}
#[derive(Clone)]
pub struct NodeQueueManagement {
    core: Arc<Core>,
}
#[derive(Clone)]
pub struct NodeConsumeReceipt {
    operation: Arc<Operation>,
    observed: Option<RequestFact>,
}
fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, NodeQueueError> {
    mutex
        .lock()
        .map_err(|_| RegistryError::StorageUnavailable.into())
}
fn socket_error(error: ViewError) -> NodeQueueError {
    match error {
        ViewError::NodeStopping => NodeQueueError::NodeStopping,
        ViewError::StreamSealed => NodeQueueError::SocketClosed,
        ViewError::ReadFailure => RegistryError::StorageUnavailable.into(),
        _ => RegistryError::InvalidActor.into(),
    }
}
fn assemble(
    operation: NodeQueueMutation,
    lease: Option<&NodePreparedBodyLease>,
) -> Result<ValidatedMutation<PromptDocument>, NodeQueueError> {
    Ok(match operation {
        NodeQueueMutation::Submit { .. } => ValidatedMutation::Submit(
            lease
                .ok_or(RegistryError::InvalidIntent)?
                .shared_validated(),
        ),
        NodeQueueMutation::Edit { item, revision, .. } => ValidatedMutation::Edit {
            item,
            revision,
            body: lease
                .ok_or(RegistryError::InvalidIntent)?
                .shared_validated(),
        },
        NodeQueueMutation::Cancel { item, revision } => {
            ValidatedMutation::Cancel { item, revision }
        }
        NodeQueueMutation::Prioritize { item, revision } => {
            ValidatedMutation::Prioritize { item, revision }
        }
        NodeQueueMutation::Resume => ValidatedMutation::Resume,
    })
}
impl NodeQueueManagement {
    pub(super) fn new(
        queue: ProductQueue,
        uploads: Arc<NodeInputUploads>,
        authority: Arc<SocketViewAuthority>,
    ) -> Self {
        Self {
            core: Arc::new(Core {
                queue,
                uploads,
                authority,
                table: Mutex::new(Table {
                    sealed: false,
                    operations: HashMap::new(),
                }),
            }),
        }
    }
    fn check(&self, client: &SocketViewClient) -> Result<(), NodeQueueError> {
        if !Arc::ptr_eq(client.context.authority(), &self.core.authority) {
            return Err(RegistryError::InvalidActor.into());
        }
        client
            .context
            .lifetime
            .with_live(|| Ok(()))
            .map_err(socket_error)?;
        if lock(&self.core.table)?.sealed {
            return Err(NodeQueueError::NodeStopping);
        }
        Ok(())
    }
    // The authorization future is awaited outside every synchronous lifetime/slot lock.
    async fn socket_wait<F: Future>(
        &self,
        client: &SocketViewClient,
        future: F,
    ) -> Result<F::Output, NodeQueueError> {
        tokio::pin!(future);
        let wake = Arc::new(OperationWake {
            changed: Notify::new(),
        });
        let waker = Waker::from(wake.clone());
        loop {
            let notified = wake.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            self.check(client)?;
            let (_, guard) = self
                .core
                .authority
                .current_actor_guard(client)
                .await
                .map_err(socket_error)?;
            let polled = client
                .context
                .lifetime
                .with_live(|| Ok(future.as_mut().poll(&mut Context::from_waker(&waker))))
                .map_err(socket_error)?;
            drop(guard);
            match polled {
                Poll::Ready(result) => return Ok(result),
                Poll::Pending => notified.await,
            }
        }
    }
    async fn mutation(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        operation: NodeQueueMutation,
    ) -> Result<
        (
            ValidatedMutation<PromptDocument>,
            Option<NodePreparedBodyLease>,
        ),
        NodeQueueError,
    > {
        self.check(client)?;
        let body = match operation {
            NodeQueueMutation::Submit { body } => Some((body, UploadPurpose::Submit)),
            NodeQueueMutation::Edit { body, .. } => Some((body, UploadPurpose::Edit)),
            _ => None,
        };
        let lease = match body {
            Some((body, purpose)) => Some(
                self.core
                    .uploads
                    .open(client)?
                    .lease_body(client, body, target, purpose)
                    .await?,
            ),
            None => None,
        };
        let mutation = assemble(operation, lease.as_ref())?;
        Ok((mutation, lease))
    }
    pub(crate) async fn register(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        operation: NodeQueueMutation,
    ) -> Result<RegistrationFact, NodeQueueError> {
        self.register_with_id(client, target, operation, None).await
    }
    pub(crate) async fn register_with_id(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        operation: NodeQueueMutation,
        requested: Option<RegistrationId>,
    ) -> Result<RegistrationFact, NodeQueueError> {
        let (mutation, lease) = self.mutation(client, target, operation).await?;
        // Original register copies the complete descriptor before returning its future.
        let registration =
            self.core
                .queue
                .register_with_id(client.context.actor(), target, &mutation, requested);
        drop(mutation);
        drop(lease);
        self.socket_wait(client, registration)
            .await?
            .map_err(Into::into)
    }
    pub(crate) async fn query(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        id: RegistrationId,
    ) -> Result<RequestFact, NodeQueueError> {
        self.socket_wait(
            client,
            self.core
                .queue
                .query_request(client.context.actor(), target, id),
        )
        .await?
        .map_err(Into::into)
    }
    pub(crate) async fn begin_consume(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        id: RegistrationId,
        body: Option<Id128>,
    ) -> Result<NodeConsumeReceipt, NodeQueueError> {
        self.check(client)?;
        let fact = self.query(client, target, id).await?;
        let intent = fact
            .registration()
            .ok_or(RegistryError::Retired)?
            .intent()
            .clone();
        if intent.target() != target {
            return Err(RegistryError::Conflict.into());
        }
        // Shared-target read authority does not let another actor reserve this registration.
        let client_id = EntityId::decode(client.context.actor().client_id())
            .map_err(|_| RegistryError::InvalidActor)?;
        if intent.actor() != client_id {
            return Err(RegistryError::InvalidActor.into());
        }
        let descriptor = match (intent.action(), body) {
            (QueueAction::Submit, Some(body)) => NodeQueueMutation::Submit { body },
            (QueueAction::Edit, Some(body)) => NodeQueueMutation::Edit {
                item: intent.item().ok_or(RegistryError::InvalidIntent)?,
                revision: intent
                    .expected_revision()
                    .ok_or(RegistryError::InvalidIntent)?,
                body,
            },
            (QueueAction::Cancel, None) => NodeQueueMutation::Cancel {
                item: intent.item().ok_or(RegistryError::InvalidIntent)?,
                revision: intent
                    .expected_revision()
                    .ok_or(RegistryError::InvalidIntent)?,
            },
            (QueueAction::Prioritize, None) => NodeQueueMutation::Prioritize {
                item: intent.item().ok_or(RegistryError::InvalidIntent)?,
                revision: intent
                    .expected_revision()
                    .ok_or(RegistryError::InvalidIntent)?,
            },
            (QueueAction::Resume, None) => NodeQueueMutation::Resume,
            _ => return Err(RegistryError::InvalidIntent.into()),
        };
        if let Some(receipt) = self.existing(client, id, &intent, body, &fact)? {
            return Ok(receipt);
        }
        // Temporary leases may overlap; the complete mutation is constructed only inside the
        // stable identity insertion fence. An invalid/cancelled draft never reserves the ID.
        let prepared = match descriptor {
            NodeQueueMutation::Submit { body } => Some(
                self.core
                    .uploads
                    .open(client)?
                    .lease_body(client, body, target, UploadPurpose::Submit)
                    .await?,
            ),
            NodeQueueMutation::Edit { body, .. } => Some(
                self.core
                    .uploads
                    .open(client)?
                    .lease_body(client, body, target, UploadPurpose::Edit)
                    .await?,
            ),
            _ => None,
        };
        if prepared.as_ref().map(|lease| lease.validated().digest()) != intent.body_digest() {
            return Err(RegistryError::Conflict.into());
        }
        let candidate = Arc::new(Operation {
            context: client.context.clone(),
            authority: self.core.authority.clone(),
            intent: intent.clone(),
            body,
            custody: ConsumptionCustody::new(),
            wake: Arc::new(OperationWake {
                changed: Notify::new(),
            }),
            slot: Mutex::new(Slot {
                future: None,
                lease: None,
                result: None,
            }),
        });
        let participant: Arc<dyn SocketLifetimeParticipant> = candidate.clone();
        client
            .context
            .lifetime
            .register_participant(Arc::downgrade(&participant))
            .map_err(socket_error)?;
        client
            .context
            .lifetime
            .with_live(|| {
                let mut table = self.core.table.lock().map_err(|_| ViewError::ReadFailure)?;
                Ok(if table.sealed {
                    Err(NodeQueueError::NodeStopping)
                } else if let Some(existing) = table.operations.get(&id) {
                    if !Arc::ptr_eq(&existing.context, &client.context)
                        || existing.intent != intent
                        || existing.body != body
                    {
                        Err(RegistryError::Conflict.into())
                    } else {
                        repeat_receipt(existing.clone(), &fact)
                    }
                } else {
                    table
                        .operations
                        .try_reserve(1)
                        .map_err(|_| ViewError::AllocationFailure)?;
                    let mutation = assemble(descriptor, prepared.as_ref())
                        .map_err(|_| ViewError::ReadFailure)?;
                    let queue = self.core.queue.clone();
                    let actor = client.context.actor().clone();
                    let custody = candidate.custody.clone();
                    let mut slot = candidate.slot.lock().map_err(|_| ViewError::ReadFailure)?;
                    slot.lease = prepared;
                    slot.future = Some(Box::pin(async move {
                        queue
                            .consume_observed(&actor, target, id, mutation, custody)
                            .await
                    }));
                    drop(slot);
                    table.operations.insert(id, candidate.clone());
                    Ok(NodeConsumeReceipt {
                        operation: candidate,
                        observed: None,
                    })
                })
            })
            .map_err(socket_error)?
    }
    fn existing(
        &self,
        client: &SocketViewClient,
        id: RegistrationId,
        intent: &RegisteredIntent,
        body: Option<Id128>,
        fact: &RequestFact,
    ) -> Result<Option<NodeConsumeReceipt>, NodeQueueError> {
        client
            .context
            .lifetime
            .with_live(|| {
                let table = self.core.table.lock().map_err(|_| ViewError::ReadFailure)?;
                Ok(if table.sealed {
                    Err(NodeQueueError::NodeStopping)
                } else {
                    table
                        .operations
                        .get(&id)
                        .map(|operation| {
                            if !Arc::ptr_eq(&operation.context, &client.context)
                                || operation.intent != *intent
                                || operation.body != body
                            {
                                Err(RegistryError::Conflict.into())
                            } else {
                                repeat_receipt(operation.clone(), fact)
                            }
                        })
                        .transpose()
                })
            })
            .map_err(socket_error)?
    }
    pub(super) fn seal(&self) {
        let mut table = self
            .core
            .table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        table.sealed = true;
        for operation in table.operations.values() {
            operation.stop(NodeQueueError::NodeStopping, true);
        }
        table.operations.clear();
    }
}
fn repeat_receipt(
    operation: Arc<Operation>,
    fact: &RequestFact,
) -> Result<NodeConsumeReceipt, NodeQueueError> {
    // This SELECT belongs to a new command. Preserve the old unknown receipt and its original
    // Queue coordination; never promote that receipt or create a second consume mutation.
    let pending = matches!(
        lock(&operation.slot)?.result,
        Some(Ok(NodeConsumeResult::Pending { .. }))
    );
    let observed = pending.then(|| fact.clone());
    Ok(NodeConsumeReceipt {
        operation,
        observed,
    })
}
impl Operation {
    fn stop(&self, error: NodeQueueError, all: bool) {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if all || !self.custody.transferred() {
            slot.future = None;
            slot.lease = None;
            if slot.result.is_none() {
                slot.result = Some(Err(error));
            }
        }
        drop(slot);
        self.wake.changed.notify_waiters();
    }
    fn result(&self) -> Result<Option<Result<NodeConsumeResult, NodeQueueError>>, NodeQueueError> {
        Ok(lock(&self.slot)?.result.clone())
    }
    fn poll_slot(
        &self,
        cx: &mut Context<'_>,
    ) -> Result<Poll<Result<NodeConsumeResult, NodeQueueError>>, NodeQueueError> {
        let mut slot = lock(&self.slot)?;
        if let Some(result) = &slot.result {
            return Ok(Poll::Ready(result.clone()));
        }
        let polled = slot
            .future
            .as_mut()
            .ok_or(RegistryError::NoOwnedOperation)?
            .as_mut()
            .poll(cx);
        if self.custody.transferred() {
            slot.lease = None;
        }
        match polled {
            Poll::Pending => Ok(Poll::Pending),
            Poll::Ready(reply) => {
                let result = match reply {
                    ConsumeReply::Resolved { fact, returned } => NodeConsumeResult::Resolved {
                        fact,
                        returned: returned.is_some(),
                    },
                    ConsumeReply::Refused { error, returned } => {
                        drop(returned);
                        NodeConsumeResult::Refused {
                            error,
                            returned: true,
                        }
                    }
                    ConsumeReply::Pending { request } => NodeConsumeResult::Pending { request },
                };
                slot.future = None;
                slot.lease = None;
                slot.result = Some(Ok(result.clone()));
                drop(slot);
                self.wake.changed.notify_waiters();
                Ok(Poll::Ready(Ok(result)))
            }
        }
    }
    fn poll_once(
        &self,
        cx: &mut Context<'_>,
    ) -> Result<Poll<Result<NodeConsumeResult, NodeQueueError>>, NodeQueueError> {
        if self.custody.transferred() {
            return self.poll_slot(cx);
        }
        match self.context.lifetime.with_live(|| Ok(self.poll_slot(cx))) {
            Ok(result) => result,
            Err(_) if self.custody.transferred() => self.poll_slot(cx),
            Err(error) => {
                self.stop(socket_error(error), false);
                self.poll_slot(cx)
            }
        }
    }
}
impl SocketLifetimeParticipant for Operation {
    fn seal(&self) {
        self.stop(NodeQueueError::SocketClosed, false);
    }
}
impl NodeConsumeReceipt {
    pub(crate) async fn wait(&self) -> Result<NodeConsumeResult, NodeQueueError> {
        if let Some(fact) = &self.observed {
            return Ok(NodeConsumeResult::Observed { fact: fact.clone() });
        }
        let operation = &self.operation;
        let waker = Waker::from(operation.wake.clone());
        loop {
            let notified = operation.wake.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = operation.result()? {
                return result;
            }
            // current_actor_guard calls lifetime.check itself, so await it outside all std locks.
            let guard = if operation.custody.transferred() {
                None
            } else {
                match operation
                    .authority
                    .current_actor_guard(&operation.context.client())
                    .await
                {
                    Ok((_, guard)) => Some(guard),
                    Err(_) if operation.custody.transferred() => None,
                    Err(error) => {
                        operation.stop(socket_error(error), false);
                        continue;
                    }
                }
            };
            let polled = operation.poll_once(&mut Context::from_waker(&waker))?;
            drop(guard);
            match polled {
                Poll::Ready(result) => return result,
                Poll::Pending => notified.await,
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn transferred(&self) -> bool {
        self.operation.custody.transferred()
    }
    #[cfg(test)]
    pub(crate) fn same_operation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.operation, &other.operation)
    }
}
