//! Safe native controls retain the original protocol objects and their actual write reports.
#![allow(
    clippy::significant_drop_tightening,
    clippy::type_complexity,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, CancelledControlReservation, ConfigurationPort, ControlIdentity, ControlIntentKind,
    ControlKind, ControlMetadata, ControlPhase, ControlRecordProof, ControlWriteBoundary,
    CurrentCancelBinding, DispatchError, DispatchFenceRejection, DispatchPollIo, DispatchWriteStep,
    EncodedPromptFrame, EntityId, KnownStopReason, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeRuntimeOptions, NativeSessionKey,
    Ordering, OwnedNativeSource, PermissionCancelBinding, PromptTicket, ReservedControl, RpcScalar,
    SourceLaunchPort, TransportFact, TransportProgress, UnsupportedReplyBinding,
    control_fingerprint,
    runtime::{ResourceSlot, SourceSlot},
};
use crate::{
    application::queue_owner::{PriorityFailure, PriorityHandoff},
    domain::activity::{CancelScope, WorkActivity},
};
use std::{
    sync::{Mutex, atomic::AtomicBool},
    task::Context,
};
use tokio::sync::Notify;

pub(super) struct ControlOperation<P: crate::application::native_dispatch::NativeApprovalPort> {
    metadata: ControlMetadata,
    nonce: Arc<()>,
    state: Mutex<ControlState<P>>,
    changed: Notify,
    permission: Option<EntityId>,
    authority: Arc<ControlAuthority<P>>,
    scope: P::Scope,
    cancel: Option<(CancelScope, u64)>,
    deadline: tokio::time::Instant,
    retry: tokio::sync::Mutex<()>,
    next: Mutex<Option<Arc<Self>>>,
    retry_consumed: AtomicBool,
    hold_released: AtomicBool,
    worker: std::sync::OnceLock<Arc<super::tasks::TaskSlot>>,
}
struct ControlCapture<P: crate::application::native_dispatch::NativeApprovalPort> {
    reservation: P::ControlReservation,
    authority: Arc<ControlAuthority<P>>,
    intent: ControlIntentKind,
    reference: EntityId,
    cancel: Option<(CancelScope, u64)>,
    permission: Option<EntityId>,
    lineage: Option<ControlRetryLineage>,
}
#[derive(Clone, Copy)]
struct ControlRetryLineage {
    obligation: EntityId,
    serial: u64,
    deadline: tokio::time::Instant,
}
pub(super) struct ControlStaging<P: crate::application::native_dispatch::NativeApprovalPort> {
    scope: P::Scope,
    rpc: Option<RpcScalar>,
    kind: ControlKind,
    resource: Option<super::ResourceMetadata>,
    authority: Arc<ControlAuthority<P>>,
    state: Mutex<ControlStagingState<P>>,
    installed: std::sync::OnceLock<Arc<ControlOperation<P>>>,
}
struct ControlStagingState<P: crate::application::native_dispatch::NativeApprovalPort> {
    reservation: Option<P::ControlReservation>,
    frame: Option<P::Frame>,
    cancelled: Option<CancelledControlReservation<P::Scope, P::Probe>>,
    error: Option<DispatchError>,
}
pub(super) struct ControlStagingEvidence {
    pub(super) kind: ControlKind,
    pub(super) rpc: Option<RpcScalar>,
    pub(super) resource: Option<super::ResourceMetadata>,
    pub(super) error: DispatchError,
    pub(super) zero: TransportFact,
    pub(super) intent: ControlIntentKind,
    pub(super) reference: Option<EntityId>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> ControlStaging<P> {
    pub(super) fn closure_evidence(
        &self,
        scope: &P::Scope,
        protocol: &P,
    ) -> Result<Option<ControlStagingEvidence>, DispatchError> {
        if &self.scope != scope {
            return Err(DispatchError::InvalidScope);
        }
        if self.installed.get().is_some() {
            return Ok(None);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if state.reservation.is_some() || state.frame.is_some() {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let receipt = state
            .cancelled
            .as_ref()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        if receipt.identity().scope() != scope || receipt.identity().rpc() != self.rpc.as_ref() {
            return Err(DispatchError::InvalidScope);
        }
        let TransportProgress::Finished(zero @ TransportFact::Zero { .. }) =
            protocol.probe_progress(receipt.probe())
        else {
            return Err(DispatchError::ResourceUnconfirmed);
        };
        let (intent, reference) = match &*self.authority {
            ControlAuthority::Unsupported(request) => {
                let identity = protocol.unsupported_identity(request);
                if identity.scope != self.scope
                    || Some(&identity.rpc) != self.rpc.as_ref()
                    || self.kind != ControlKind::Unsupported
                {
                    return Err(DispatchError::InvalidScope);
                }
                (ControlIntentKind::Unsupported, None)
            }
            ControlAuthority::Priority(handoff) => {
                (ControlIntentKind::Priority, Some(handoff.operation()))
            }
            ControlAuthority::Shutdown(reference) | ControlAuthority::TargetClosing(reference) => {
                (ControlIntentKind::Shutdown, Some(*reference))
            }
        };
        Ok(Some(ControlStagingEvidence {
            kind: self.kind,
            rpc: self.rpc.clone(),
            resource: self.resource.clone(),
            error: state.error.ok_or(DispatchError::ResourceUnconfirmed)?,
            zero,
            intent,
            reference,
        }))
    }
}
struct ControlState<P: crate::application::native_dispatch::NativeApprovalPort> {
    reservation: Option<P::ControlReservation>,
    frame: Option<P::Frame>,
    ticket: Option<Arc<P::Ticket>>,
    terminal: Option<TransportFact>,
    stored: bool,
    reserved_stored: bool,
    terminal_record: Option<Arc<ControlRecordProof>>,
    cancelled: Option<CancelledControlReservation<P::Scope, P::Probe>>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> ControlOperation<P> {
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(super) const fn kind_for_test(&self) -> ControlKind {
        self.metadata.kind
    }
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "Used only by optional Native development test fixtures."
    )]
    pub(super) const fn id_for_test(&self) -> EntityId {
        self.metadata.control_id
    }
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "Used only by optional Native development test fixtures."
    )]
    pub(super) fn transport_for_test(
        &self,
    ) -> Result<(Option<TransportProgress>, bool), DispatchError> {
        let state = self.state.lock().map_err(|_| DispatchError::Stopped)?;
        Ok((
            state.ticket.as_ref().map(|ticket| ticket.progress()),
            state.stored,
        ))
    }
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "Used only by optional Native development test fixtures."
    )]
    pub(super) fn cancel_write_for_test(&self) -> Result<(), DispatchError> {
        let state = self.state.lock().map_err(|_| DispatchError::Stopped)?;
        state
            .ticket
            .as_ref()
            .ok_or(DispatchError::NotReady)?
            .request_cancel();
        drop(state);
        Ok(())
    }
    pub(super) fn approval_reservation(
        &self,
    ) -> Result<
        Option<(
            P::Scope,
            EntityId,
            ControlMetadata,
            crate::domain::approval_owner::ReplyOrigin,
        )>,
        DispatchError,
    > {
        if self.metadata.kind != ControlKind::PermissionCancelled {
            return Ok(None);
        }
        let obligation = self.permission.ok_or(DispatchError::InvalidScope)?;
        let origin = match &*self.authority {
            ControlAuthority::Shutdown(_) => {
                crate::domain::approval_owner::ReplyOrigin::SourceClosing
            }
            ControlAuthority::TargetClosing(_) => {
                crate::domain::approval_owner::ReplyOrigin::TargetClosing
            }
            ControlAuthority::Priority(_) => {
                crate::domain::approval_owner::ReplyOrigin::ProtocolCancel
            }
            ControlAuthority::Unsupported(_) => return Err(DispatchError::InvalidScope),
        };
        Ok(Some((
            self.scope.clone(),
            obligation,
            self.metadata.clone(),
            origin,
        )))
    }
    pub(super) fn approval_terminal(&self) -> Result<Option<ControlMetadata>, DispatchError> {
        let state = self
            .state
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?;
        let Some(fact) = state.terminal else {
            return Ok(None);
        };
        if self.metadata.phase == ControlPhase::Terminal(fact) {
            return Ok(Some(self.metadata.clone()));
        }
        let Some(proof) = &state.terminal_record else {
            return Ok(None);
        };
        if proof.metadata.phase != ControlPhase::Terminal(fact) {
            return Err(DispatchError::Conflict);
        }
        Ok(Some(proof.metadata.clone()))
    }
    pub(super) fn persisted_fingerprints(&self) -> Result<([u8; 32], [u8; 32]), DispatchError> {
        let state = self.state.lock().map_err(|_| DispatchError::Stopped)?;
        if !state.stored || !state.reserved_stored {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let terminal = if let Some(proof) = &state.terminal_record {
            proof.metadata.fingerprint
        } else if matches!(self.metadata.phase, ControlPhase::Terminal(_)) {
            self.metadata.fingerprint
        } else {
            return Err(DispatchError::ResourceUnconfirmed);
        };
        drop(state);
        Ok((self.metadata.fingerprint, terminal))
    }
}
enum ControlAuthority<P: crate::application::native_dispatch::NativeApprovalPort> {
    Unsupported(P::UnsupportedRequest),
    Priority(Arc<PriorityHandoff>),
    Shutdown(EntityId),
    TargetClosing(EntityId),
}
struct ControlFence<P: crate::application::native_dispatch::NativeApprovalPort, L: SourceLaunchPort>
where
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    source: Arc<SourceSlot<P, L>>,
    resource: Option<Arc<ResourceSlot<P>>>,
    scope: P::Scope,
    kind: ControlKind,
    rpc: Option<RpcScalar>,
    cancel: Option<CancelScope>,
    cancel_interval: Option<u64>,
    permission: Option<EntityId>,
    control_id: EntityId,
    authority: Arc<ControlAuthority<P>>,
}
impl<P, L> ControlWriteBoundary<P::Scope> for ControlFence<P, L>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
{
    #[expect(
        clippy::significant_drop_tightening,
        reason = "Keep current target facts locked through the same actual IO poll to prevent stale cancellation."
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "Keep all original authority checks adjacent to the single actual IO poll under the same boundary."
    )]
    fn check_and_poll(
        &self,
        identity: &ControlIdentity<P::Scope>,
        _: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        if identity.scope() != &self.scope
            || identity.rpc() != self.rpc.as_ref()
            || self.source.pre.authority.closed.load(Ordering::SeqCst)
        {
            return Err(DispatchFenceRejection::Stopped);
        }
        if let Some(resource) = &self.resource {
            if identity.native_session() != Some(resource.pre.native.native_id())
                || resource.pre.authority.closed.load(Ordering::SeqCst)
            {
                return Err(DispatchFenceRejection::StaleTarget);
            }
            let mut poll = || {
                let facts = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                let _gate = self
                    .source
                    .pre
                    .authority
                    .gate
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                if self.source.pre.authority.closed.load(Ordering::SeqCst)
                    || resource.pre.authority.closed.load(Ordering::SeqCst)
                {
                    return Err(DispatchError::Stopped);
                }
                if self.kind == ControlKind::NativeCancel
                    && (self.cancel_interval != Some(facts.interval)
                        || self.cancel.as_ref().is_none_or(|scope| {
                            !facts.activity.cancellation_is_current(scope) || old_work_ended(&facts)
                        }))
                {
                    return Err(DispatchError::StaleSelection);
                }
                if self.kind == ControlKind::PermissionCancelled
                    && !self.rpc.as_ref().is_some_and(|rpc| {
                        facts.permissions.get(rpc).is_some_and(|p| {
                            p.snapshot().is_ok_and(|state| {
                                state.obligation == self.permission
                                    && state.claimed == Some(self.control_id)
                                    && !state.written
                                    && !state.retired
                            })
                        })
                    })
                {
                    return Err(DispatchError::StaleSelection);
                }
                if matches!(&*self.authority, ControlAuthority::Priority(_))
                    && resource.pre.authority.releasing.load(Ordering::SeqCst)
                {
                    return Err(DispatchError::Stopped);
                }
                match &*self.authority {
                    ControlAuthority::Shutdown(reference) => {
                        if !self.source.pre.authority.closing.load(Ordering::SeqCst)
                            || self.source.close_intent.get() != Some(reference)
                        {
                            return Err(DispatchError::Stopped);
                        }
                    }
                    ControlAuthority::TargetClosing(reference)
                        if (!resource.pre.authority.releasing.load(Ordering::SeqCst)
                            || resource.close_intent.get() != Some(reference)) =>
                    {
                        return Err(DispatchError::Stopped);
                    }
                    _ => {}
                }
                let _ = io.poll(cx);
                Ok(())
            };
            return match &*self.authority {
                ControlAuthority::Priority(handoff) => {
                    resource.queue.priority_with_current(handoff, &mut poll)
                }
                _ => poll(),
            }
            .map_err(|_| DispatchFenceRejection::Cancelled);
        }
        match &*self.authority {
            ControlAuthority::Unsupported(original) => {
                let actual = self
                    .source
                    .source
                    .get()
                    .ok_or(DispatchFenceRejection::Stopped)?;
                let _ = actual;
                let _ = original;
            }
            ControlAuthority::Shutdown(reference)
                if self.kind == ControlKind::PermissionCancelled =>
            {
                if !self.source.pre.authority.closing.load(Ordering::SeqCst)
                    || self.source.close_intent.get() != Some(reference)
                {
                    return Err(DispatchFenceRejection::Stopped);
                }
            }
            _ => return Err(DispatchFenceRejection::StaleTarget),
        }
        let _gate = self
            .source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
        if self.source.pre.authority.closed.load(Ordering::SeqCst) {
            return Err(DispatchFenceRejection::Stopped);
        }
        if self.kind == ControlKind::PermissionCancelled {
            let permission = self
                .rpc
                .as_ref()
                .and_then(|rpc| self.source.permissions.lock().ok()?.get(rpc).cloned())
                .ok_or(DispatchFenceRejection::StaleTarget)?;
            let state = permission
                .snapshot()
                .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
            if permission.scope != self.scope
                || identity.native_session() != Some(permission.session.as_ref())
                || state.obligation != self.permission
                || state.claimed != Some(self.control_id)
                || state.written
                || state.retired
            {
                return Err(DispatchFenceRejection::StaleTarget);
            }
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
fn old_work_ended<P: crate::application::native_dispatch::NativeApprovalPort>(
    facts: &super::environment::TargetFacts<P>,
) -> bool {
    use super::environment::ForegroundEvidence;
    if facts.foreground != WorkActivity::Idle || facts.interval_open {
        return false;
    }
    facts.pending_input.as_ref().map_or(
        matches!(
            facts.foreground_evidence,
            ForegroundEvidence::IdleSnapshot
                | ForegroundEvidence::OwnedV1Idle
                | ForegroundEvidence::ReliableTerminal
        ),
        |input| {
            input
                .native_reason
                .is_some_and(|reason| reason.reliable() && reason != KnownStopReason::Error)
                || matches!(input.transport, Some(TransportFact::Zero { .. }))
                || matches!(
                    facts.foreground_evidence,
                    ForegroundEvidence::ReliableTerminal
                ) && facts.terminal_sequence > input.admitted_sequence
                    && facts.terminal.is_some_and(KnownStopReason::reliable)
        },
    )
}
impl<P, L, G, N> super::NativeRuntimeHandle<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    /// Retries the original safe control only after its actual Zero and exact stored terminal.
    /// # Errors
    /// Written, unknown, expired, replaced or no longer authorized original operations cannot retry.
    pub async fn retry_zero_control(
        &self,
        control: EntityId,
        deadline: tokio::time::Instant,
    ) -> Result<EntityId, DispatchError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let core = self.core.clone();
        let failed = self.core.clone();
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.retry_control(control, deadline).await;
                let _ = tx.send(result);
            },
            move || failed.fail_all(DispatchError::RequiredStateUnavailable),
        )?;
        rx.await
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
    }
}
impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: crate::application::native_dispatch::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + crate::application::native_dispatch::NativeApprovalRepository,
{
    async fn retry_control(
        self: &Arc<Self>,
        id: EntityId,
        deadline: tokio::time::Instant,
    ) -> Result<EntityId, DispatchError> {
        self.running()?;
        let sources = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let mut found = None;
        for source in sources {
            let operation = source
                .controls
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .iter()
                .find(|operation| operation.metadata.control_id == id)
                .cloned();
            if let Some(operation) = operation {
                found = Some((source, operation));
                break;
            }
        }
        let (source, operation) = found.ok_or(DispatchError::InvalidPromptReservation)?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            core.retry_installed_control(&slot, &operation, deadline)
                .await
        })
        .await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The original retry lock covers terminal ticket, stored fact and current intent validation before the next reservation."
    )]
    async fn retry_installed_control(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        operation: &Arc<ControlOperation<P>>,
        deadline: tokio::time::Instant,
    ) -> Result<EntityId, DispatchError> {
        let _retry = operation.retry.lock().await;
        if let Some(next) = operation
            .next
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .as_ref()
        {
            return Ok(next.metadata.control_id);
        }
        if operation.retry_consumed.load(Ordering::SeqCst)
            || source.pre.authority.closed.load(Ordering::SeqCst)
            || tokio::time::Instant::now() >= deadline
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let worker = operation
            .worker
            .get()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        if !worker.join(deadline).await {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let terminal = {
            let state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
            let fact = state
                .terminal
                .filter(|fact| matches!(fact, TransportFact::Zero { .. }))
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            if !state.stored || !state.reserved_stored {
                return Err(DispatchError::RequiredStateUnavailable);
            }
            let actual = if let Some(ticket) = &state.ticket {
                ticket.progress()
            } else if let Some(cancelled) = &state.cancelled {
                if cancelled.identity().scope() != &operation.scope
                    || cancelled.identity().rpc() != operation.metadata.rpc.as_ref()
                {
                    return Err(DispatchError::InvalidScope);
                }
                self.protocol.probe_progress(cancelled.probe())
            } else {
                return Err(DispatchError::ResourceUnconfirmed);
            };
            if actual != TransportProgress::Finished(fact) {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            state.terminal_record.as_ref().map_or_else(
                || operation.metadata.clone(),
                |proof| proof.metadata.clone(),
            )
        };
        let original = ControlRecordProof {
            metadata: operation.metadata.clone(),
            nonce: operation.nonce.clone(),
        };
        let probe = original.probe();
        if !original.matches(&probe) {
            return Err(DispatchError::Conflict);
        }
        let inspection = self
            .repository
            .inspect_control(&probe, &self.deadline()?)
            .await?;
        if inspection.terminal.as_ref().map(|row| &row.metadata) != Some(&terminal)
            || operation.metadata.phase == ControlPhase::Reserved
                && inspection.reserved.as_ref().map(|row| &row.metadata)
                    != Some(&operation.metadata)
        {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        if self.protocol.source_scope(connection) != operation.scope {
            return Err(DispatchError::InvalidScope);
        }
        let resource = operation
            .metadata
            .resource
            .as_ref()
            .map(|metadata| {
                source
                    .targets
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .values()
                    .find(|resource| resource.pre.metadata == *metadata)
                    .cloned()
                    .ok_or(DispatchError::InvalidResource)
            })
            .transpose()?;
        if resource
            .as_ref()
            .is_some_and(|resource| resource.pre.authority.closed.load(Ordering::SeqCst))
        {
            return Err(DispatchError::InvalidResource);
        }
        match &*operation.authority {
            ControlAuthority::Priority(handoff) => resource
                .as_ref()
                .ok_or(DispatchError::InvalidResource)?
                .queue
                .priority_current(handoff)?,
            ControlAuthority::Shutdown(reference) => {
                if source.close_intent.get() != Some(reference)
                    || !source.pre.authority.closing.load(Ordering::SeqCst)
                {
                    return Err(DispatchError::Stopped);
                }
            }
            ControlAuthority::TargetClosing(reference) => {
                let resource = resource.as_ref().ok_or(DispatchError::InvalidResource)?;
                if resource.close_intent.get() != Some(reference)
                    || !resource.pre.authority.releasing.load(Ordering::SeqCst)
                {
                    return Err(DispatchError::Stopped);
                }
            }
            ControlAuthority::Unsupported(_) => {}
        }
        let serial = operation
            .metadata
            .attempt_serial
            .checked_add(1)
            .ok_or(DispatchError::SequenceExhausted)?;
        let reservation = match operation.metadata.kind {
            ControlKind::Unsupported => {
                let ControlAuthority::Unsupported(original) = &*operation.authority else {
                    return Err(DispatchError::InvalidPromptReservation);
                };
                let identity = self.protocol.unsupported_identity(original);
                if identity.scope != operation.scope
                    || Some(&identity.rpc) != operation.metadata.rpc.as_ref()
                {
                    return Err(DispatchError::InvalidScope);
                }
                self.protocol.reserve_unsupported_reply(
                    connection,
                    original,
                    &UnsupportedReplyBinding {
                        scope: operation.scope.clone(),
                        rpc: identity.rpc,
                    },
                )?
            }
            ControlKind::NativeCancel => {
                let resource = resource.as_ref().ok_or(DispatchError::InvalidResource)?;
                let facts = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                let (cancel, interval) = operation
                    .cancel
                    .as_ref()
                    .ok_or(DispatchError::InvalidResource)?;
                if facts.interval != *interval
                    || old_work_ended(&facts)
                    || !facts.activity.cancellation_is_current(cancel)
                {
                    return Err(DispatchError::StaleSelection);
                }
                drop(facts);
                self.protocol.reserve_native_cancel(
                    connection,
                    &CurrentCancelBinding {
                        scope: operation.scope.clone(),
                        native: resource.pre.native.clone(),
                        resource: resource.pre.clone(),
                    },
                )?
            }
            ControlKind::PermissionCancelled => {
                let rpc = operation
                    .metadata
                    .rpc
                    .as_ref()
                    .ok_or(DispatchError::InvalidPromptReservation)?;
                let permission = source
                    .permissions
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .get(rpc)
                    .cloned()
                    .filter(|permission| {
                        permission.snapshot().is_ok_and(|state| {
                            state.obligation == operation.permission
                                && !state.written
                                && !state.retired
                                && state.claimed.is_none()
                        })
                    })
                    .ok_or(DispatchError::StaleSelection)?;
                let request = permission.request.clone();
                if let Some(resource) = &resource {
                    if !permission
                        .association
                        .as_ref()
                        .is_some_and(|pre| pre.same(&resource.pre))
                    {
                        return Err(DispatchError::InvalidResource);
                    }
                    self.protocol.reserve_permission_cancelled(
                        connection,
                        &request,
                        &PermissionCancelBinding {
                            scope: operation.scope.clone(),
                            native: resource.pre.native.clone(),
                            rpc: rpc.clone(),
                            resource: resource.pre.clone(),
                        },
                    )?
                } else {
                    let ControlAuthority::Shutdown(reference) = &*operation.authority else {
                        return Err(DispatchError::InvalidScope);
                    };
                    self.protocol.reserve_source_permission_cancelled(
                        connection,
                        &request,
                        &super::SourcePermissionCancelBinding {
                            scope: operation.scope.clone(),
                            rpc: rpc.clone(),
                            source: source.pre.clone(),
                            intent_reference: *reference,
                        },
                    )?
                }
            }
        };

        operation.retry_consumed.store(true, Ordering::SeqCst);
        let result = self.start_control(
            source,
            resource.clone(),
            ControlCapture {
                reservation,
                authority: operation.authority.clone(),
                intent: operation.metadata.intent,
                reference: operation.metadata.intent_reference,
                cancel: operation.cancel.clone(),
                permission: operation.permission,
                lineage: Some(ControlRetryLineage {
                    obligation: operation.metadata.obligation_id,
                    serial,
                    deadline,
                }),
            },
        );
        match result {
            Ok(next) => {
                let id = next.metadata.control_id;
                *operation.next.lock().map_err(|_| DispatchError::Stopped)? = Some(next);
                self.release_control_hold(source, resource.as_ref(), operation)?;
                Ok(id)
            }
            Err(error) => {
                self.fail_source(source, error);
                Err(error)
            }
        }
    }
    pub(super) fn unsupported(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        original: P::UnsupportedRequest,
    ) -> Result<(), DispatchError> {
        let identity = self.protocol.unsupported_identity(&original);
        let reference = Self::new_id()?;
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        let reservation = self.protocol.reserve_unsupported_reply(
            connection,
            &original,
            &UnsupportedReplyBinding {
                scope: identity.scope.clone(),
                rpc: identity.rpc,
            },
        )?;
        let authority = Arc::new(ControlAuthority::Unsupported(original));
        self.start_control(
            source,
            None,
            ControlCapture {
                reservation,
                authority,
                intent: ControlIntentKind::Unsupported,
                reference,
                cancel: None,
                permission: None,
                lineage: None,
            },
        )
        .map(|_| ())
    }
    fn start_control(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<Arc<ResourceSlot<P>>>,
        capture: ControlCapture<P>,
    ) -> Result<Arc<ControlOperation<P>>, DispatchError> {
        let ControlCapture {
            reservation,
            authority,
            intent,
            reference: intent_reference,
            cancel: captured_cancel,
            permission,
            lineage,
        } = capture;
        let identity = reservation.identity();
        let kind = match identity {
            ControlIdentity::NativeCancel { .. } => ControlKind::NativeCancel,
            ControlIdentity::PermissionCancelled { .. } => ControlKind::PermissionCancelled,
            ControlIdentity::Unsupported { .. } => ControlKind::Unsupported,
        };
        let staged = Arc::new(ControlStaging {
            scope: identity.scope().clone(),
            rpc: identity.rpc().cloned(),
            kind,
            resource: resource
                .as_ref()
                .map(|resource| resource.pre.metadata.clone()),
            authority: authority.clone(),
            state: Mutex::new(ControlStagingState {
                reservation: Some(reservation),
                frame: None,
                cancelled: None,
                error: None,
            }),
            installed: std::sync::OnceLock::new(),
        });
        match source.control_staging.lock() {
            Ok(mut slots) => slots.push(staged.clone()),
            Err(error) => {
                error.into_inner().push(staged.clone());
                self.cancel_staged_control(&staged, DispatchError::RequiredStateUnavailable);
                self.fail_source(source, DispatchError::RequiredStateUnavailable);
                return Err(DispatchError::RequiredStateUnavailable);
            }
        }
        let result = self.prepare_staged_control(
            source,
            resource,
            &staged,
            &authority,
            intent,
            intent_reference,
            captured_cancel,
            permission,
            lineage,
        );
        if let Err(error) = result {
            self.cancel_staged_control(&staged, error);
            self.fail_source(source, error);
        }
        result
    }
    fn cancel_staged_control(&self, staged: &ControlStaging<P>, error: DispatchError) {
        if staged.installed.get().is_some() {
            return;
        }
        let mut state = match staged.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.error.get_or_insert(error);
        if let Some(reservation) = state.reservation.take() {
            state.cancelled = Some(self.protocol.cancel_reserved_control(reservation));
        }
        state.frame.take();
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep ownership of the same staged reservation and frame explicit through every publication or cleanup exit."
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep separately owned protocol, persistence and lifecycle authorities explicit at this composition boundary."
    )]
    fn prepare_staged_control(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<Arc<ResourceSlot<P>>>,
        staged: &Arc<ControlStaging<P>>,
        authority: &Arc<ControlAuthority<P>>,
        intent: ControlIntentKind,
        intent_reference: EntityId,
        captured_cancel: Option<(CancelScope, u64)>,
        permission: Option<EntityId>,
        lineage: Option<ControlRetryLineage>,
    ) -> Result<Arc<ControlOperation<P>>, DispatchError> {
        let mut deadline = match &lineage {
            Some(previous) => previous.deadline,
            None => NativeRuntimeOptions::at(self.options.close_wait)?,
        };
        match &**authority {
            ControlAuthority::Shutdown(_) => {
                let closing = source
                    .close_deadline
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .ok_or(DispatchError::Stopped)?;
                deadline = deadline.min(closing);
            }
            ControlAuthority::TargetClosing(_) => {
                let closing = resource
                    .as_ref()
                    .ok_or(DispatchError::InvalidResource)?
                    .close_deadline
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .ok_or(DispatchError::Stopped)?;
                deadline = deadline.min(closing);
            }
            _ => {}
        }
        let cancel = captured_cancel.clone();
        let (cancel, cancel_interval) = cancel.map_or((None, None), |(scope, interval)| {
            (Some(scope), Some(interval))
        });
        let scope = staged.scope.clone();
        let rpc = staged.rpc.clone();
        let kind = staged.kind;
        let binding = source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .binding
            .ok_or(DispatchError::InvalidScope)?;
        let id = Self::new_id()?;
        let obligation = lineage.as_ref().map_or(id, |previous| previous.obligation);
        let mut metadata = ControlMetadata {
            control_id: id,
            obligation_id: obligation,
            attempt_serial: lineage.as_ref().map_or(0, |previous| previous.serial),
            requirement: Self::new_id()?,
            source: source.pre.metadata.clone(),
            scope_binding: binding,
            resource: resource.as_ref().map(|r| r.pre.metadata.clone()),
            work_serial: cancel
                .as_ref()
                .and_then(CancelScope::work)
                .map(crate::domain::activity::WorkToken::serial),
            intent,
            intent_reference,
            kind,
            rpc: rpc.clone(),
            frame_length: None,
            phase: ControlPhase::Reserved,
            fingerprint: [0; 32],
            at: self.clock.unix_millis(),
        };
        let encoded = {
            let state = staged.state.lock().map_err(|_| DispatchError::Stopped)?;
            state
                .reservation
                .as_ref()
                .ok_or(DispatchError::InvalidPromptReservation)?
                .encode()
        };
        match encoded {
            Ok(frame) => {
                let length = frame.byte_len_checked();
                staged
                    .state
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .frame = Some(frame);
                match length {
                    Ok(length) if length > 0 => metadata.frame_length = Some(length),
                    Ok(_) => {
                        self.cancel_staged_control(staged, DispatchError::InvalidPromptReservation);
                    }
                    Err(error) => self.cancel_staged_control(staged, error),
                }
            }
            Err(error) => self.cancel_staged_control(staged, error),
        }
        let zero = {
            let state = staged.state.lock().map_err(|_| DispatchError::Stopped)?;
            if let Some(cancelled) = &state.cancelled {
                if cancelled.identity().scope() != &scope
                    || cancelled.identity().rpc() != rpc.as_ref()
                {
                    return Err(DispatchError::InvalidScope);
                }
                match self.protocol.probe_progress(cancelled.probe()) {
                    TransportProgress::Finished(fact @ TransportFact::Zero { .. }) => {
                        metadata.phase = ControlPhase::Terminal(fact);
                        Some(fact)
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            } else {
                None
            }
        };
        metadata.fingerprint = control_fingerprint(&metadata)?;
        let (reservation, frame, cancelled) = {
            let mut state = staged.state.lock().map_err(|_| DispatchError::Stopped)?;
            (
                state.reservation.take(),
                state.frame.take(),
                state.cancelled.take(),
            )
        };
        let operation = Arc::new(ControlOperation {
            metadata,
            nonce: Arc::new(()),
            state: Mutex::new(ControlState {
                reservation,
                frame,
                ticket: None,
                terminal: zero,
                stored: false,
                reserved_stored: false,
                terminal_record: None,
                cancelled,
            }),
            changed: Notify::new(),
            permission,
            authority: authority.clone(),
            scope: scope.clone(),
            cancel: captured_cancel,
            deadline,
            retry: tokio::sync::Mutex::new(()),
            next: Mutex::new(None),
            retry_consumed: AtomicBool::new(false),
            hold_released: AtomicBool::new(false),
            worker: std::sync::OnceLock::new(),
        });
        let healthy = match source.controls.lock() {
            Ok(mut controls) => {
                controls.push(operation.clone());
                true
            }
            Err(error) => {
                error.into_inner().push(operation.clone());
                false
            }
        };
        staged
            .installed
            .set(operation.clone())
            .map_err(|_| DispatchError::Conflict)?;
        source.pre.authority.required.fetch_add(1, Ordering::SeqCst);
        if !healthy {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        if let Some(resource) = &resource {
            let mut facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            facts.control_holds.insert(id);
        }
        if kind == ControlKind::PermissionCancelled {
            let request = rpc
                .as_ref()
                .and_then(|rpc| source.permissions.lock().ok()?.get(rpc).cloned())
                .ok_or(DispatchError::InvalidScope)?;
            let mut state = request
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if state.obligation != permission
                || state.claimed.is_some()
                || state.written
                || state.retired
            {
                return Err(DispatchError::StaleSelection);
            }
            *request
                .response_owner
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)? =
                Some(super::environment::PermissionResponseOwner::Cancelled {
                    id,
                    original: operation.clone(),
                });
            state.claimed = Some(id);
        }
        let _ = self.retain_approval_control(source, &operation);
        let boundary = Arc::new(ControlFence {
            source: source.clone(),
            resource: resource.clone(),
            scope,
            kind,
            rpc,
            cancel,
            cancel_interval,
            permission,
            control_id: id,
            authority: authority.clone(),
        });
        let core = self.clone();
        let slot = source.clone();
        let op = operation.clone();
        let failed = self.clone();
        let failed_source = source.clone();
        let worker = source.tasks.spawn_guarded(
            async move {
                let _ = core
                    .run_control(&slot, resource.as_ref(), &op, boundary)
                    .await;
                op.changed.notify_waiters();
            },
            move || failed.fail_source(&failed_source, DispatchError::RequiredStateUnavailable),
        )?;
        operation
            .worker
            .set(worker)
            .map_err(|_| DispatchError::Conflict)?;
        Ok(operation)
    }
    async fn run_control(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<&Arc<ResourceSlot<P>>>,
        operation: &Arc<ControlOperation<P>>,
        boundary: Arc<ControlFence<P, L>>,
    ) -> Result<(), DispatchError> {
        let reserved_result = self
            .repository
            .record_control(
                &ControlRecordProof {
                    metadata: operation.metadata.clone(),
                    nonce: operation.nonce.clone(),
                },
                &self.context(),
                &self.deadline()?,
            )
            .await;
        if reserved_result.is_ok() {
            operation
                .state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .reserved_stored = true;
        }
        let _ = self.persist_approval_control_arm(source, operation).await;
        // Safety IO remains allowed when necessary SQL is unavailable. Ordinary input keeps its gap.
        let terminal = operation
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .terminal;
        let fact = if let Some(fact) = terminal {
            fact
        } else {
            let (reservation, frame) = {
                let mut state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
                (
                    state
                        .reservation
                        .take()
                        .ok_or(DispatchError::InvalidPromptReservation)?,
                    state
                        .frame
                        .take()
                        .ok_or(DispatchError::InvalidPromptReservation)?,
                )
            };
            let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
            let ticket = match self.protocol.enqueue_control(
                connection,
                reservation,
                frame,
                boundary.clone(),
                operation.deadline,
            ) {
                Ok(ticket) => Arc::new(ticket),
                Err(rejected) => {
                    let mut state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
                    state.reservation = Some(rejected.reservation);
                    state.frame = Some(rejected.frame);
                    drop(state);
                    return Err(rejected.error);
                }
            };
            operation
                .state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .ticket = Some(ticket.clone());
            loop {
                let changed = resource.map(|r| r.target.changed.notified());
                tokio::pin!(changed);
                if let Some(changed) = changed.as_mut().as_pin_mut() {
                    changed.enable();
                }
                if operation.metadata.kind == ControlKind::NativeCancel
                    && resource
                        .is_some_and(|r| r.target.facts.lock().map_or(true, |f| old_work_ended(&f)))
                {
                    ticket.request_cancel();
                }
                if let Some(changed) = changed.as_mut().as_pin_mut() {
                    tokio::select! {fact=ticket.wait()=>break fact,()=changed=>{}}
                } else {
                    break ticket.wait().await;
                }
            }
        };
        operation
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .terminal = Some(fact);
        operation.changed.notify_waiters();
        if matches!(fact, TransportFact::Unknown { .. }) {
            self.fail_source(source, DispatchError::ResourceUnconfirmed);
        }
        self.persist_control_terminal(source, resource, operation)
            .await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the original terminal fact, SQL confirmation and required-gap clearing in their ordered transition."
    )]
    async fn persist_control_terminal(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<&Arc<ResourceSlot<P>>>,
        operation: &Arc<ControlOperation<P>>,
    ) -> Result<(), DispatchError> {
        let (fact, was_stored, reserved_stored) = {
            let state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
            (
                state.terminal.ok_or(DispatchError::ResourceUnconfirmed)?,
                state.stored,
                state.reserved_stored,
            )
        };
        if !was_stored {
            if !reserved_stored {
                self.repository
                    .record_control(
                        &ControlRecordProof {
                            metadata: operation.metadata.clone(),
                            nonce: operation.nonce.clone(),
                        },
                        &self.context(),
                        &self.deadline()?,
                    )
                    .await?;
                operation
                    .state
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .reserved_stored = true;
            }
            if operation.metadata.phase != ControlPhase::Terminal(fact) {
                let proof = {
                    let mut state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
                    if let Some(proof) = &state.terminal_record {
                        proof.clone()
                    } else {
                        let mut terminal = operation.metadata.clone();
                        terminal.requirement = Self::new_id()?;
                        terminal.phase = ControlPhase::Terminal(fact);
                        terminal.at = self.clock.unix_millis();
                        terminal.fingerprint = control_fingerprint(&terminal)?;
                        let proof = Arc::new(ControlRecordProof {
                            metadata: terminal,
                            nonce: operation.nonce.clone(),
                        });
                        state.terminal_record = Some(proof.clone());
                        proof
                    }
                };
                self.repository
                    .record_control(&proof, &self.context(), &self.deadline()?)
                    .await?;
            }
            operation
                .state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .stored = true;
        }
        self.persist_approval_control_terminal(source, operation)
            .await?;
        self.release_control_hold_if_safe(source, resource, operation)?;
        if operation.metadata.kind == ControlKind::PermissionCancelled {
            let request = operation
                .metadata
                .rpc
                .as_ref()
                .and_then(|rpc| source.permissions.lock().ok()?.get(rpc).cloned())
                .ok_or(DispatchError::InvalidScope)?;
            if fact == TransportFact::Written {
                source
                    .capacity_activity
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?
                    .touch(&request.session, std::time::Instant::now())
                    .map_err(|_| DispatchError::ObservationOrder)?;
            }
            let mut state = request
                .state
                .lock()
                .map_err(|_| DispatchError::RequiredStateUnavailable)?;
            if state.obligation != operation.permission {
                return Err(DispatchError::InvalidScope);
            }
            if fact == TransportFact::Written {
                state.written = true;
                state.retired = true;
            } else if matches!(fact, TransportFact::Zero { .. })
                && state.claimed == Some(operation.metadata.control_id)
            {
                state.claimed = None;
                request
                    .response_owner
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?
                    .take();
            }
        }
        if let Some(resource) = resource {
            let mut f = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if !matches!(fact, TransportFact::Unknown { .. })
                && operation.metadata.kind == ControlKind::PermissionCancelled
                && let Some(rpc) = &operation.metadata.rpc
                && f.permissions.get(rpc).is_some_and(|p| {
                    p.snapshot()
                        .is_ok_and(|state| state.obligation == operation.permission)
                })
                && fact == TransportFact::Written
            {
                f.permissions.remove(rpc);
                let epoch = f.activity.epoch();
                f.activity
                    .retire_permission(
                        epoch,
                        operation.permission.ok_or(DispatchError::InvalidResource)?,
                    )
                    .map_err(|_| DispatchError::InvalidResource)?;
            }
            drop(f);
            resource.target.wake_writer();
            resource.target.changed.notify_waiters();
            self.environment.wake(resource.pre.native());
        }
        if matches!(fact, TransportFact::Unknown { .. }) {
            self.fail_source(source, DispatchError::ResourceUnconfirmed);
        }
        Ok(())
    }
    fn release_control_hold_if_safe(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<&Arc<ResourceSlot<P>>>,
        operation: &ControlOperation<P>,
    ) -> Result<(), DispatchError> {
        let state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
        if !state.stored || !state.reserved_stored {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let fact = state.terminal.ok_or(DispatchError::ResourceUnconfirmed)?;
        drop(state);
        let physical = source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .is_some();
        let ended = if operation.metadata.kind == ControlKind::NativeCancel
            && matches!(fact, TransportFact::Zero { .. })
        {
            resource
                .map(|resource| {
                    resource
                        .target
                        .facts
                        .lock()
                        .map(|facts| old_work_ended(&facts))
                        .map_err(|_| DispatchError::Stopped)
                })
                .transpose()?
                .unwrap_or(false)
        } else {
            false
        };
        if physical || fact == TransportFact::Written || ended {
            self.release_control_hold(source, resource, operation)?;
        }
        Ok(())
    }
    #[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
    pub(super) fn repeat_control_release_for_test(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        operation: &ControlOperation<P>,
    ) -> Result<(), DispatchError> {
        self.release_control_hold(source, Some(resource), operation)
    }
    fn release_control_hold(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: Option<&Arc<ResourceSlot<P>>>,
        operation: &ControlOperation<P>,
    ) -> Result<(), DispatchError> {
        if let Some(resource) = resource {
            let removed = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .control_holds
                .remove(&operation.metadata.control_id);
            if removed {
                resource.target.changed.notify_waiters();
            }
        }
        if operation.hold_released.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        source.pre.authority.required.fetch_sub(1, Ordering::SeqCst);
        let targets = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for target in targets {
            target.target.wake_writer();
            self.environment.wake(target.pre.native());
        }
        Ok(())
    }
    pub(super) async fn reconcile_source_controls(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let staged = source
            .control_staging
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for stage in staged {
            self.cancel_staged_control(&stage, DispatchError::RequiredStateUnavailable);
        }
        let operations = source
            .controls
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let resources = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for operation in operations {
            {
                let mut state = operation.state.lock().map_err(|_| DispatchError::Stopped)?;
                if state.terminal.is_none() {
                    if let Some(ticket) = &state.ticket {
                        if let TransportProgress::Finished(fact) = ticket.progress() {
                            state.terminal = Some(fact);
                        }
                    } else if let Some(reservation) = state.reservation.take() {
                        let cancelled = self.protocol.cancel_reserved_control(reservation);
                        let TransportProgress::Finished(fact @ TransportFact::Zero { .. }) =
                            self.protocol.probe_progress(cancelled.probe())
                        else {
                            return Err(DispatchError::ResourceUnconfirmed);
                        };
                        if cancelled.identity().rpc() != operation.metadata.rpc.as_ref() {
                            return Err(DispatchError::InvalidScope);
                        }
                        state.frame.take();
                        state.cancelled = Some(cancelled);
                        state.terminal = Some(fact);
                    }
                }
            }
            let resource = operation.metadata.resource.as_ref().and_then(|metadata| {
                resources
                    .iter()
                    .find(|resource| resource.pre.metadata == *metadata)
            });
            self.persist_control_terminal(source, resource, &operation)
                .await?;
        }
        Ok(())
    }
    pub(super) fn launch_priority(
        self: &Arc<Self>,
        key: NativeSessionKey,
    ) -> Result<(), DispatchError> {
        let core = self.clone();
        let failed = self.clone();
        self.tasks.spawn_guarded(
            async move {
                let native = key.clone();
                if let Err(error) = core.apply_priority(&key).await {
                    let failure = if error == DispatchError::StaleSelection {
                        PriorityFailure::SelectionChanged
                    } else {
                        PriorityFailure::InterruptUnconfirmed
                    };
                    if let Err(cleanup) =
                        core.queue.fail_priority_for_native(&native, failure).await
                    {
                        core.fail_all(cleanup);
                    }
                }
            },
            move || failed.fail_all(DispatchError::RequiredStateUnavailable),
        )?;
        Ok(())
    }
    async fn apply_priority(self: &Arc<Self>, key: &NativeSessionKey) -> Result<(), DispatchError> {
        let Some(target) = self.queue.target_for_native(key)? else {
            return Ok(());
        };
        let (source, resource) = self.ensure_resource(target).await?;
        let core = self.clone();
        let slot = source.clone();
        let native = key.clone();
        self.run_source_job(source, async move {
            core.apply_installed_priority(&native, target, &slot, &resource)
                .await
        })
        .await
    }
    async fn apply_installed_priority(
        self: &Arc<Self>,
        key: &NativeSessionKey,
        target: EntityId,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        let Some(handoff) = self.queue.priority(target).await? else {
            return Ok(());
        };
        let handoff = Arc::new(handoff);
        resource.queue.priority_current(&handoff)?;
        let run = self
            .runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(key)
            .cloned();
        if let Some(run) = run {
            run.request_cancel()?;
        }
        let reference = handoff.operation();
        let authority = Arc::new(ControlAuthority::Priority(handoff));
        self.cancel_scope_controls(
            source,
            resource,
            authority,
            ControlIntentKind::Priority,
            reference,
        )
        .await?;
        loop {
            let changed = resource.target.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let controls = source
                .controls
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .iter()
                .filter(|operation| {
                    operation.metadata.kind == ControlKind::NativeCancel
                        && operation.metadata.resource.as_ref() == Some(&resource.pre.metadata)
                        && operation.state.lock().is_ok_and(|state| state.stored)
                })
                .cloned()
                .collect::<Vec<_>>();
            for operation in controls {
                self.release_control_hold_if_safe(source, Some(resource), &operation)?;
            }
            let ready = {
                let mut f = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                if old_work_ended(&f)
                    && f.permissions.is_empty()
                    && f.control_holds.iter().all(|id| *id == reference)
                {
                    f.control_holds.remove(&reference);
                    true
                } else {
                    false
                }
            };
            if ready {
                resource.target.wake_writer();
                self.environment.wake(key);
                return Ok(());
            }
            if source.pre.authority.closed.load(Ordering::SeqCst) {
                return Err(DispatchError::Stopped);
            }
            changed.await;
        }
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Capture all obligations of one accepted interruption while retaining its original intent authority."
    )]
    async fn cancel_scope_controls(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        authority: Arc<ControlAuthority<P>>,
        intent: ControlIntentKind,
        reference: EntityId,
    ) -> Result<(), DispatchError> {
        let (scope, cancel, interval, permissions, needs_cancel) = {
            let f = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            let binding = f.binding.as_ref().ok_or(DispatchError::NotReady)?;
            let cancel = f
                .activity
                .cancel_scope()
                .map_err(|_| DispatchError::InvalidResource)?;
            let permissions = f
                .permissions
                .iter()
                .filter_map(|(rpc, p)| {
                    p.snapshot()
                        .ok()
                        .filter(|state| state.claimed.is_none() && !state.written && !state.retired)
                        .and_then(|state| {
                            state
                                .obligation
                                .map(|obligation| (rpc.clone(), p.request.clone(), obligation))
                        })
                })
                .collect::<Vec<_>>();
            (
                binding.scope.clone(),
                cancel,
                f.interval,
                permissions,
                !old_work_ended(&f),
            )
        };
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        let mut controls = Vec::new();
        if needs_cancel {
            let existing = source
                .controls
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .iter()
                .find(|operation| {
                    operation.metadata.intent_reference == reference
                        && operation.metadata.kind == ControlKind::NativeCancel
                        && operation.metadata.resource.as_ref() == Some(&resource.pre.metadata)
                        && operation.metadata.work_serial
                            == cancel
                                .work()
                                .map(crate::domain::activity::WorkToken::serial)
                })
                .cloned();
            if let Some(operation) = existing {
                controls.push(operation);
            } else {
                let reservation = self.protocol.reserve_native_cancel(
                    connection,
                    &CurrentCancelBinding {
                        scope: scope.clone(),
                        native: resource.pre.native.clone(),
                        resource: resource.pre.clone(),
                    },
                )?;
                controls.push(self.start_control(
                    source,
                    Some(resource.clone()),
                    ControlCapture {
                        reservation,
                        authority: authority.clone(),
                        intent,
                        reference,
                        cancel: Some((cancel, interval)),
                        permission: None,
                        lineage: None,
                    },
                )?);
            }
        }
        for (rpc, request, obligation) in permissions {
            let reservation = self.protocol.reserve_permission_cancelled(
                connection,
                &request,
                &PermissionCancelBinding {
                    scope: scope.clone(),
                    native: resource.pre.native.clone(),
                    rpc: rpc.clone(),
                    resource: resource.pre.clone(),
                },
            )?;
            let operation = self.start_control(
                source,
                Some(resource.clone()),
                ControlCapture {
                    reservation,
                    authority: authority.clone(),
                    intent,
                    reference,
                    cancel: None,
                    permission: Some(obligation),
                    lineage: None,
                },
            )?;
            controls.push(operation);
        }
        for control in controls {
            loop {
                let changed = control.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if control
                    .state
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .terminal
                    .is_some()
                {
                    break;
                }
                if source.pre.authority.closed.load(Ordering::SeqCst) {
                    return Err(DispatchError::Stopped);
                }
                changed.await;
            }
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Validate the same original request claim, prior Zero and whole-source closing intent in one capture path."
    )]
    pub(super) fn close_source_permission(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        permission: &Arc<super::environment::SourcePermissionSlot<P>>,
    ) -> Result<Option<Arc<ControlOperation<P>>>, DispatchError> {
        let _permission_action = permission
            .action
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?;
        let reference = *source.close_intent.get().ok_or(DispatchError::Stopped)?;
        if !source.pre.authority.closing.load(Ordering::SeqCst)
            || source.pre.authority.closed.load(Ordering::SeqCst)
        {
            return Err(DispatchError::Stopped);
        }
        let original = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(&permission.rpc)
            .cloned()
            .ok_or(DispatchError::InvalidScope)?;
        if !Arc::ptr_eq(&original, permission) {
            return Err(DispatchError::InvalidScope);
        }
        let state = permission.snapshot()?;
        if state.written || state.retired {
            return Ok(None);
        }
        let obligation = state
            .obligation
            .ok_or(DispatchError::RequiredStateUnavailable)?;
        if let Some(claimed) = state.claimed {
            let owner = permission
                .response_owner
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            let Some(super::environment::PermissionResponseOwner::Cancelled {
                id,
                original: current,
            }) = owner.as_ref()
            else {
                return Err(DispatchError::ResourceUnconfirmed);
            };
            if *id != claimed
                || current.metadata.control_id != claimed
                || current.permission != Some(obligation)
            {
                return Err(DispatchError::InvalidScope);
            }
            let current = current.clone();
            return Ok(Some(current));
        }
        let previous = source
            .controls
            .lock()
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
            .iter()
            .rev()
            .find(|operation| {
                operation.permission == Some(obligation)
                    && operation.metadata.intent_reference == reference
                    && operation.metadata.kind == ControlKind::PermissionCancelled
            })
            .cloned();
        if let Some(previous) = previous {
            // The same closing intent cannot turn a retained Zero into a fresh serial0 obligation.
            return Ok(Some(previous));
        }
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        if self.protocol.source_scope(connection) != permission.scope {
            return Err(DispatchError::InvalidScope);
        }
        let resource = permission.association.as_ref().and_then(|pre| {
            source
                .all_resources
                .lock()
                .ok()?
                .iter()
                .find(|resource| {
                    resource.pre.same(pre)
                        && resource.recorded.load(Ordering::SeqCst)
                        && !resource.pre.authority.closed.load(Ordering::SeqCst)
                })
                .cloned()
        });
        let reservation = if let Some(resource) = &resource {
            self.protocol.reserve_permission_cancelled(
                connection,
                &permission.request,
                &PermissionCancelBinding {
                    scope: permission.scope.clone(),
                    native: resource.pre.native.clone(),
                    rpc: permission.rpc.clone(),
                    resource: resource.pre.clone(),
                },
            )?
        } else {
            self.protocol.reserve_source_permission_cancelled(
                connection,
                &permission.request,
                &super::SourcePermissionCancelBinding {
                    scope: permission.scope.clone(),
                    rpc: permission.rpc.clone(),
                    source: source.pre.clone(),
                    intent_reference: reference,
                },
            )?
        };
        self.start_control(
            source,
            resource,
            ControlCapture {
                reservation,
                authority: Arc::new(ControlAuthority::Shutdown(reference)),
                intent: ControlIntentKind::Shutdown,
                reference,
                cancel: None,
                permission: Some(obligation),
                lineage: None,
            },
        )
        .map(Some)
    }
    pub(super) async fn shutdown_controls(self: &Arc<Self>) -> Result<(), DispatchError> {
        let sources = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for source in sources {
            if source.close_intent.get().is_none() {
                let _ = source.close_intent.set(Self::new_id()?);
            }
            let reference = *source.close_intent.get().ok_or(DispatchError::Stopped)?;
            let resources = source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for resource in resources {
                if resource.loaded.get().is_some() {
                    self.cancel_scope_controls(
                        &source,
                        &resource,
                        Arc::new(ControlAuthority::Shutdown(reference)),
                        ControlIntentKind::Shutdown,
                        reference,
                    )
                    .await?;
                }
            }
            let permissions = source
                .permissions
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let mut controls = Vec::new();
            for permission in permissions {
                self.settle_selected_for_safety(&source, &permission)
                    .await?;
                if let Some(operation) = self.close_source_permission(&source, &permission)? {
                    controls.push(operation);
                }
            }
            for operation in controls {
                loop {
                    let changed = operation.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if operation
                        .state
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .terminal
                        .is_some()
                    {
                        break;
                    }
                    changed.await;
                }
            }
        }
        Ok(())
    }
    pub(super) async fn close_resource_controls(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        if resource.close_intent.get().is_none() {
            let _ = resource.close_intent.set(Self::new_id()?);
        }
        let reference = *resource.close_intent.get().ok_or(DispatchError::Stopped)?;
        self.cancel_scope_controls(
            source,
            resource,
            Arc::new(ControlAuthority::TargetClosing(reference)),
            ControlIntentKind::Shutdown,
            reference,
        )
        .await
    }
    pub(super) async fn reconcile_resource_controls(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        let operations = source
            .controls
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .filter(|operation| {
                operation.metadata.resource.as_ref() == Some(&resource.pre.metadata)
            })
            .cloned()
            .collect::<Vec<_>>();
        for operation in operations {
            self.persist_control_terminal(source, Some(resource), &operation)
                .await?;
        }
        Ok(())
    }
}
