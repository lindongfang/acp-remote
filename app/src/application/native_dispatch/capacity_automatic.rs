use super::{
    Arc, AutomaticReleaseOutcome, AutomaticReleasePhase, AutomaticReleaseReason,
    AutomaticReleaseRecord, CapacityRecord, ConfigurationPort, CurrentTargetCloseBinding,
    DispatchError, DispatchFenceRejection, DispatchIoProgress, DispatchPollIo, DispatchWriteStep,
    EncodedPromptFrame, EntityId, LifecycleKind, LifecycleWriteBoundary, LifecycleWriteIdentity,
    NativeApprovalPort, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, Ordering, OwnedNativeSource, PromptTicket, ReservedPrompt,
    ResponseWait, SourceLaunchPort, TransportFact,
    capacity::CapacityResource,
    environment::TargetFacts,
    runtime::{LifecycleOperation, LifecycleResponseValue, ResourceSlot, SourcePhase, SourceSlot},
};
use crate::{
    application::queue_owner::AutomaticCloseGuard,
    domain::{
        activity::WorkActivity,
        session_capacity::{Coverage, DEFAULT_IDLE_TIMEOUT, RecycleEligibility},
    },
};
use std::{
    sync::{Mutex, Weak},
    task::Poll,
    time::Instant,
};

#[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
#[path = "capacity_automatic_tests.rs"]
mod tests;
#[path = "capacity_automatic_tree.rs"]
mod tree;
use tree::AutomaticTree;
#[path = "capacity_automatic_rank.rs"]
mod rank;
use rank::PressureSelection;

pub(super) struct AutomaticOperation<P: NativeApprovalPort> {
    pub(super) id: EntityId,
    pub(super) original: CapacityRecord,
    pub(super) reason: AutomaticReleaseReason,
    pressure_selection: Option<PressureSelection>,
    queue: Arc<dyn AutomaticCloseGuard>,
    tree: Arc<AutomaticTree<P>>,
    record: Mutex<Option<AutomaticReleaseRecord>>,
    lifecycle: Mutex<Option<Arc<LifecycleOperation<P>>>>,
}

pub(super) fn retained_reason<P: NativeApprovalPort>(
    capacity: &CapacityResource<P>,
) -> Result<Option<AutomaticReleaseReason>, DispatchError> {
    let operation = capacity
        .automatic
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .clone();
    let Some(operation) = operation else {
        return Ok(None);
    };
    let pending = operation
        .record
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .as_ref()
        .is_none_or(|record| record.phase == AutomaticReleasePhase::Pending);
    Ok(pending.then_some(operation.reason))
}

pub(super) fn permission_settled<P: NativeApprovalPort>(
    permission: &super::environment::SourcePermissionSlot<P>,
) -> Result<bool, DispatchError> {
    let state = permission.snapshot()?;
    Ok(permission_state_settled(state))
}

const fn permission_state_settled(state: super::environment::PermissionState) -> bool {
    state.retired || state.written
}

fn target_reconciled<P: NativeApprovalPort>(
    facts: Option<&TargetFacts<P>>,
) -> Result<bool, DispatchError> {
    let Some(facts) = facts else { return Ok(true) };
    let mut permissions_ready = true;
    for permission in facts.permissions.values() {
        permissions_ready &= permission_settled(permission)?;
    }
    Ok(facts.foreground == WorkActivity::Idle
        && !facts.interval_open
        && facts.pending_input.is_none()
        && facts.activity.input().is_none()
        && facts.requirements.is_empty()
        && facts.failed_requirements.is_empty()
        && facts.control_holds.is_empty()
        && permissions_ready)
}

#[expect(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "One ordered critical section must retain queue, facts, activity and source checks through actual IO"
)]
fn with_candidate<P, L, T>(
    protocol: &P,
    source: &SourceSlot<P, L>,
    capacity: &CapacityResource<P>,
    tree: &AutomaticTree<P>,
    original: &CapacityRecord,
    now: Instant,
    revision: u64,
    action: impl FnOnce(Option<Instant>) -> Result<T, DispatchError>,
) -> Result<T, DispatchError>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    let facts = tree
        .resources
        .iter()
        .map(|resource| {
            resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut activity = source
        .capacity_activity
        .lock()
        .map_err(|_| DispatchError::RequiredStateUnavailable)?;
    let now = now.max(Instant::now());
    let mut previous = capacity
        .queue_revision
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    *previous = Some((tree.members.clone(), revision));
    drop(previous);
    if !tree.matches(&activity, capacity.pre.native.native_id())? {
        return action(None);
    }
    let _gate = source
        .pre
        .authority
        .gate
        .lock()
        .map_err(|_| DispatchError::RequiredStateUnavailable)?;
    let current = capacity
        .record
        .lock()
        .map_err(|_| DispatchError::Stopped)?
        .clone()
        .ok_or(DispatchError::InvalidResource)?;
    if !current.same_reservation(original)
        || current.native.as_ref() != Some(capacity.pre.native())
        || current.source != source.pre.metadata.source_id
        || current.instance != source.pre.metadata.instance_id
        || current.runtime != source.pre.metadata.runtime
        || !Arc::ptr_eq(&source.pre.authority, &capacity.pre.authority.source)
        || tree
            .resources
            .iter()
            .any(|resource| !Arc::ptr_eq(&resource.pre.authority, &capacity.pre.authority))
    {
        return Err(DispatchError::InvalidResource);
    }
    let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
    let initialized = memory.initialized.as_ref();
    let initialized_valid = memory.phase == SourcePhase::Initialized
        && memory.error.is_none()
        && memory.requirements.is_empty()
        && initialized.is_some_and(|initialized| {
            initialized.record.wire == Some(initialized.metadata.wire)
                && initialized.record.metadata == source.pre.metadata
                && initialized.record.lifecycle == super::SourceLifecycle::Initialized
                && initialized.record.close.is_none()
                && initialized.record.scope_binding == memory.binding
                && memory.scope.is_some()
        });
    drop(memory);
    let close_supported = source
        .source
        .get()
        .map(|source| protocol.automatic_close_supported(source))
        .transpose()?
        .unwrap_or(false);
    let permissions = source
        .permissions
        .lock()
        .map_err(|_| DispatchError::Stopped)?;
    let mut pending_approvals = 0_u32;
    for permission in permissions
        .values()
        .filter(|permission| tree.contains(&permission.session))
    {
        if !permission_settled(permission)? {
            pending_approvals = pending_approvals
                .checked_add(1)
                .ok_or(DispatchError::SequenceExhausted)?;
        }
    }
    drop(permissions);
    let ready = initialized_valid
        && tree.resources.iter().all(|resource| {
            resource.recorded.load(Ordering::Acquire) && resource.loaded.get().is_some()
        })
        && source.source.get().is_some()
        && source.gate.get().is_some()
        && source.recorded.load(Ordering::Acquire)
        && !source.pre.authority.closed.load(Ordering::Acquire)
        && !source.pre.authority.closing.load(Ordering::Acquire)
        && !source.pre.authority.quarantined.load(Ordering::Acquire)
        && source.pre.authority.required.load(Ordering::Acquire) == 0
        && !capacity.pre.authority.closed.load(Ordering::Acquire)
        && !capacity.pre.authority.releasing.load(Ordering::Acquire)
        && !current.physically_closed
        && !activity.has_unowned()
        && facts
            .iter()
            .map(|facts| target_reconciled(Some(facts)))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .all(|ready| ready);
    let eligibility = RecycleEligibility {
        foreground: Coverage::Unknown,
        background: Coverage::Unknown,
        children: Coverage::Unknown,
        pending_approvals: Some(pending_approvals),
        root_verified: true,
        management_reconciled: ready,
        close_supported,
    };
    let candidate = activity.evaluate_tree(capacity.pre.native.native_id(), now, eligibility)?;
    let result = action(candidate);
    drop(activity);
    drop(facts);
    result
}

fn request_due(reason: AutomaticReleaseReason, since: Instant, now: Instant) -> bool {
    reason == AutomaticReleaseReason::CapacityPressure
        || now
            .checked_duration_since(since)
            .is_some_and(|elapsed| elapsed >= DEFAULT_IDLE_TIMEOUT)
}

struct AutomaticBoundary<P, L>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    protocol: Arc<P>,
    source: Weak<SourceSlot<P, L>>,
    capacity: Weak<CapacityResource<P>>,
    tree: Arc<AutomaticTree<P>>,
    original: CapacityRecord,
    operation: EntityId,
    reason: AutomaticReleaseReason,
    pressure_selection: Option<PressureSelection>,
    queue: Arc<dyn AutomaticCloseGuard>,
    scope: P::Scope,
    rpc: super::RpcScalar,
}

impl<P, L> LifecycleWriteBoundary<P::Scope> for AutomaticBoundary<P, L>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    fn check_and_poll(
        &self,
        identity: &LifecycleWriteIdentity<P::Scope>,
        step: DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        if identity.kind != LifecycleKind::Close
            || identity.scope != self.scope
            || identity.rpc != self.rpc
        {
            return Err(DispatchFenceRejection::StaleTarget);
        }
        let source = self
            .source
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let capacity = self
            .capacity
            .upgrade()
            .ok_or(DispatchFenceRejection::Stopped)?;
        let result = if matches!(step, DispatchWriteStep::Chunk { offset: 0, .. }) {
            let pressure_cut = self
                .pressure_selection
                .as_ref()
                .map(PressureSelection::refresh)
                .transpose()
                .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
            self.queue.poll_idle_revision(self.operation, &mut |revision| {
                let now = Instant::now();
                with_candidate(&*self.protocol, &source, &capacity, &self.tree, &self.original, now, revision, |candidate| {
                    if !candidate.is_some_and(|since| request_due(self.reason, since, now)
                        && (self.reason != AutomaticReleaseReason::CapacityPressure
                            || self.pressure_selection.as_ref().zip(pressure_cut.as_ref())
                                .is_some_and(|(selection, cut)| selection.permits(since, cut)))) {
                        return Err(DispatchError::NotReady);
                    }
                    let result = io.poll(cx);
                    if matches!(result, Poll::Ready(Ok(DispatchIoProgress::Accepted(count))) if count > 0) {
                        capacity.pre.authority.releasing.store(true, Ordering::Release);
                        self.tree.seal(&source);
                    }
                    Ok(result)
                })
            })
        } else {
            self.queue.poll_frozen(self.operation, &mut || {
                let _gate = source
                    .pre
                    .authority
                    .gate
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                if source.pre.authority.closed.load(Ordering::Acquire)
                    || capacity.pre.authority.closed.load(Ordering::Acquire)
                    || !capacity.pre.authority.releasing.load(Ordering::Acquire)
                {
                    return Err(DispatchError::Stopped);
                }
                Ok(io.poll(cx))
            })
        };
        result.map(|_| ()).map_err(|error| match error {
            DispatchError::NotReady => DispatchFenceRejection::NotReady,
            DispatchError::Stopped | DispatchError::NodeStopping => DispatchFenceRejection::Stopped,
            _ => DispatchFenceRejection::RequiredStateUnavailable,
        })
    }
}

impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    pub(super) fn automatic_candidate(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        now: Instant,
    ) -> Result<Option<Instant>, DispatchError> {
        let result = self.inspect_automatic_candidate(source, capacity, now);
        if !matches!(result, Ok(Some(_))) {
            source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .invalidate_candidate(capacity.pre.native.native_id(), now.max(Instant::now()))?;
        }
        result
    }

    fn inspect_automatic_candidate(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        now: Instant,
    ) -> Result<Option<Instant>, DispatchError> {
        self.running()?;
        if retained_reason(capacity)?.is_some() {
            return Ok(None);
        }
        let original = capacity
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let Some(original) = original else {
            return Ok(None);
        };
        let tree = match self.automatic_tree(source, capacity) {
            Ok(tree) => tree,
            Err(DispatchError::SessionTreeUnverified) => return Ok(None),
            Err(error) => return Err(error),
        };
        let queue = &tree.queue;
        let mut candidate = None;
        let checked = queue.poll_idle_revision(EntityId::from_bytes([0; 16]), &mut |revision| {
            candidate = with_candidate(
                &*self.protocol,
                source,
                capacity,
                &tree,
                &original,
                now,
                revision,
                Ok,
            )?;
            Ok(Poll::Ready(Ok(DispatchIoProgress::Flushed)))
        });
        match checked {
            Ok(_) => Ok(candidate),
            Err(DispatchError::NotReady) => {
                let mut activity = source
                    .capacity_activity
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                activity.evaluate_tree(
                    capacity.pre.native.native_id(),
                    now.max(Instant::now()),
                    RecycleEligibility {
                        foreground: Coverage::Unknown,
                        background: Coverage::Unknown,
                        children: Coverage::Unknown,
                        pending_approvals: None,
                        root_verified: false,
                        management_reconciled: false,
                        close_supported: false,
                    },
                )?;
                drop(activity);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) async fn automatic_release(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        reason: AutomaticReleaseReason,
    ) -> Result<bool, DispatchError> {
        self.automatic_release_selected(source, capacity, reason, None)
            .await
    }

    pub(super) async fn automatic_release_selected(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        reason: AutomaticReleaseReason,
        selection: Option<PressureSelection>,
    ) -> Result<bool, DispatchError> {
        if source.metadata_close.result() == Some(Ok(())) {
            let _close = capacity.close.lock().await;
            let operation = capacity
                .automatic
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            let Some(operation) = operation else {
                return Ok(false);
            };
            let record = self
                .repository
                .finish_automatic_release(&operation.original, operation.id, &self.deadline()?)
                .await?;
            *operation
                .record
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(record);
            operation.queue.finish(operation.id)?;
            return Ok(true);
        }
        let core = self.clone();
        let slot = source.clone();
        let capacity = capacity.clone();
        self.run_source_job(source.clone(), async move {
            let deadline = core.effective_close_deadline(&slot)?;
            let _source_close = tokio::time::timeout_at(deadline, slot.close.lock())
                .await
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            let _install = tokio::time::timeout_at(deadline, slot.install.lock())
                .await
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            let _capacity_close = tokio::time::timeout_at(deadline, capacity.close.lock())
                .await
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            core.automatic_release_locked(&slot, &capacity, reason, selection)
                .await
        })
        .await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Preserve the original system operation, transport ticket and durable finish order as one ownership path"
    )]
    async fn automatic_release_locked(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        reason: AutomaticReleaseReason,
        selection: Option<PressureSelection>,
    ) -> Result<bool, DispatchError> {
        let existing = capacity
            .automatic
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let operation = if let Some(existing) = existing.filter(|operation| {
            operation.record.lock().map_or(true, |record| {
                record
                    .as_ref()
                    .is_none_or(|record| record.phase != AutomaticReleasePhase::Withdrawn)
            })
        }) {
            existing
        } else {
            let now = Instant::now();
            let Some(since) = self.automatic_candidate(source, capacity, now)? else {
                return Ok(false);
            };
            if !request_due(reason, since, now) {
                return Ok(false);
            }
            let pressure_selection =
                (reason == AutomaticReleaseReason::CapacityPressure).then(|| {
                    selection.unwrap_or_else(|| {
                        self.automatic_pressure_selection(source, capacity, since)
                    })
                });
            let original = capacity
                .record
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone()
                .ok_or(DispatchError::InvalidResource)?;
            let tree = self.automatic_tree(source, capacity)?;
            let operation = Arc::new(AutomaticOperation {
                id: Self::new_id()?,
                original,
                reason,
                pressure_selection,
                queue: tree.queue.clone(),
                tree,
                record: Mutex::new(None),
                lifecycle: Mutex::new(None),
            });
            *capacity
                .automatic
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(operation.clone());
            operation
        };
        let stored = self
            .repository
            .begin_automatic_release(
                &operation.original,
                operation.id,
                operation.reason,
                &self.deadline()?,
            )
            .await?;
        *operation
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(stored.clone());
        if stored.phase == AutomaticReleasePhase::Finished {
            operation.queue.finish(operation.id)?;
            return Ok(true);
        }
        if stored.phase == AutomaticReleasePhase::Withdrawn {
            return Ok(false);
        }
        let deadline = self.effective_close_deadline(source)?;
        let mut resource_closes = Vec::new();
        for resource in &operation.tree.resources {
            resource_closes.push(
                tokio::time::timeout_at(deadline, resource.close.lock())
                    .await
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?,
            );
        }
        let retained = operation
            .lifecycle
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let lifecycle = if let Some(retained) = retained {
            retained
        } else {
            if stored.outcome.is_some() {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            match self.prepare_automatic_close(source, capacity, &operation, deadline) {
                Ok(lifecycle) => lifecycle,
                Err(error) => {
                    // A retained lifecycle can have reached the writer. Only a returned
                    // reservation or absence of a lifecycle proves it was not enqueued.
                    let retained = operation
                        .lifecycle
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .clone();
                    let unsent = match retained {
                        None => true,
                        Some(lifecycle) => lifecycle
                            .close_objects
                            .lock()
                            .map_err(|_| DispatchError::Stopped)?
                            .is_some(),
                    };
                    if unsent {
                        return self.withdraw_automatic(source, capacity, &operation).await;
                    }
                    self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Unknown)
                        .await?;
                    return Err(error);
                }
            }
        };
        let ticket = lifecycle
            .ticket
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let Some(ticket) = ticket else {
            let unsent = lifecycle
                .close_objects
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .is_some();
            return if unsent {
                self.withdraw_automatic(source, capacity, &operation).await
            } else {
                Err(DispatchError::ResourceUnconfirmed)
            };
        };
        let Ok(fact) = tokio::time::timeout_at(deadline, ticket.wait()).await else {
            ticket.request_cancel();
            self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Unknown)
                .await?;
            return Err(DispatchError::ResourceUnconfirmed);
        };
        match fact {
            TransportFact::Zero { .. } => {
                return self.withdraw_automatic(source, capacity, &operation).await;
            }
            TransportFact::Unknown { .. } => {
                self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Unknown)
                    .await?;
            }
            TransportFact::Written => {
                if stored.outcome.is_none()
                    || stored.outcome == Some(AutomaticReleaseOutcome::Requested)
                {
                    self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Requested)
                        .await?;
                }
            }
        }
        let Ok(Ok(response)) =
            tokio::time::timeout_at(deadline, self.wait_response(source, &lifecycle)).await
        else {
            self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Unknown)
                .await?;
            return Err(DispatchError::ResourceUnconfirmed);
        };
        if matches!(response.value, LifecycleResponseValue::Error) {
            self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Failed)
                .await?;
            return Err(DispatchError::ProtocolRejected);
        }
        if !matches!(&response.value, LifecycleResponseValue::Closed(session) if session.as_ref() == capacity.pre.native.native_id())
        {
            self.save_automatic_outcome(&operation, AutomaticReleaseOutcome::Unknown)
                .await?;
            return Err(DispatchError::ResourceUnconfirmed);
        }
        for resource in &operation.tree.resources {
            let run = self
                .runs
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(resource.pre.native())
                .cloned();
            let action = if let Some(run) = &run {
                Some(
                    tokio::time::timeout_at(deadline, run.action_for_resource_close())
                        .await
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?,
                )
            } else {
                None
            };
            self.record_confirmed_tree_resource_close(
                source,
                resource,
                &response,
                capacity.pre.native(),
            )
            .await?;
            drop(action);
            self.retire_converged_run(source, resource, None).await?;
            self.environment
                .retire_closed_target(resource.pre.native(), &resource.target);
        }
        capacity.pre.authority.closed.store(true, Ordering::Release);
        operation.tree.seal(source);
        self.join_capacity_reads(source, capacity).await?;
        {
            let members = source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .members(capacity.pre.native.native_id())?;
            let permissions = source
                .permissions
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            for permission in permissions
                .values()
                .filter(|permission| members.contains(&permission.session))
            {
                if !permission_settled(permission)? {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
            }
        }
        self.capacity_closed(capacity, &operation.original).await?;
        let stored = self
            .repository
            .finish_automatic_release(&operation.original, operation.id, &self.deadline()?)
            .await?;
        *operation
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(stored);
        operation.queue.finish(operation.id)?;
        drop(resource_closes);
        Ok(true)
    }

    fn prepare_automatic_close(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
        operation: &Arc<AutomaticOperation<P>>,
        deadline: super::Instant,
    ) -> Result<Arc<LifecycleOperation<P>>, DispatchError> {
        let connection = source
            .source
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        let gate = source
            .gate
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        let scope = self.protocol.source_scope(connection);
        let reservation = self.protocol.reserve_target_close(
            connection,
            &CurrentTargetCloseBinding {
                scope: scope.clone(),
                native: capacity.pre.native.clone(),
                resource: capacity.pre.clone(),
            },
            ResponseWait::At(deadline),
        )?;
        let rpc = reservation.rpc().clone();
        let frame = reservation.encode()?;
        let lifecycle = Arc::new(LifecycleOperation::new(
            rpc.clone(),
            LifecycleKind::Close,
            frame.byte_len_checked()?,
            self.clock.unix_millis(),
        ));
        source
            .operations
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .insert(rpc.clone(), lifecycle.clone());
        *operation
            .lifecycle
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(lifecycle.clone());
        let boundary = Arc::new(AutomaticBoundary {
            protocol: Arc::clone(&self.protocol),
            source: Arc::downgrade(source),
            capacity: Arc::downgrade(capacity),
            tree: operation.tree.clone(),
            original: operation.original.clone(),
            operation: operation.id,
            reason: operation.reason,
            pressure_selection: operation.pressure_selection.clone(),
            queue: operation.queue.clone(),
            scope,
            rpc,
        });
        let ticket = match self.protocol.enqueue_automatic_close(
            connection,
            gate,
            reservation,
            frame,
            boundary,
            deadline,
        ) {
            Ok(ticket) => Arc::new(ticket),
            Err(rejected) => {
                *lifecycle
                    .close_objects
                    .lock()
                    .map_err(|_| DispatchError::Stopped)? =
                    Some((rejected.reservation, rejected.frame));
                return Err(rejected.error);
            }
        };
        *lifecycle
            .ticket
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(ticket);
        Ok(lifecycle)
    }

    async fn save_automatic_outcome(
        &self,
        operation: &AutomaticOperation<P>,
        outcome: AutomaticReleaseOutcome,
    ) -> Result<(), DispatchError> {
        let previous = operation
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .as_ref()
            .and_then(|record| record.outcome);
        let outcome = match previous {
            Some(AutomaticReleaseOutcome::Unknown) => AutomaticReleaseOutcome::Unknown,
            Some(AutomaticReleaseOutcome::Failed) => AutomaticReleaseOutcome::Failed,
            _ => outcome,
        };
        let record = self
            .repository
            .record_automatic_release_outcome(
                &operation.original,
                operation.id,
                outcome,
                &self.deadline()?,
            )
            .await?;
        *operation
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(record);
        Ok(())
    }

    async fn withdraw_automatic(
        &self,
        source: &SourceSlot<P, L>,
        capacity: &CapacityResource<P>,
        operation: &AutomaticOperation<P>,
    ) -> Result<bool, DispatchError> {
        self.save_automatic_outcome(operation, AutomaticReleaseOutcome::Withdrawn)
            .await?;
        let withdrawn = operation
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .as_ref()
            .is_some_and(|record| record.phase == AutomaticReleasePhase::Withdrawn);
        if !withdrawn {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        let restore = if capacity.pre.authority.closed.load(Ordering::Acquire) {
            Ok(())
        } else {
            capacity
                .pre
                .authority
                .releasing
                .store(false, Ordering::Release);
            if !source.pre.authority.closed.load(Ordering::Acquire)
                && !source.pre.authority.closing.load(Ordering::Acquire)
            {
                operation.tree.members.iter().try_for_each(|member| {
                    source
                        .view_attachment
                        .restore_target(member)
                        .map_err(|_| DispatchError::ResourceUnconfirmed)
                })
            } else {
                Ok(())
            }
        };
        drop(gate);
        operation.queue.finish(operation.id)?;
        restore?;
        Ok(false)
    }
}
