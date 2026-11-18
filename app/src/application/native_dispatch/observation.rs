//! Applies the real sequential lease before acknowledgement; necessary SQL remains a gap.
#![allow(
    clippy::unused_self,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    AppliedObservation, Arc, AtomicBool, ConfigurationPort, DispatchError, FactKind,
    KnownStopReason, LifecycleKind, LossTargetMetadata, MetadataBatch, NativeControlRepository,
    NativeCoordinator, NativeDispatchRepository, NativeFactMetadata, NativeIncoming, NativeIoOwner,
    NativeResourceRepository, ObservationMetadata, ObservationOrigin, ObservationProbe, Ordering,
    OwnedNativeSource, ProtocolRead, ResultKind, SourceLaunchPort, SourceLossProbe, SourceTerminal,
    TransitionKind,
    environment::{ForegroundEvidence, PermissionState, SourcePermissionSlot},
    observation_fingerprint,
    runtime::{LifecycleResponse, LifecycleResponseValue, ResourceSlot, SourcePhase, SourceSlot},
};
use crate::{
    application::queue_owner::{NativeQueuePolicy, QueueOwnerEvent},
    domain::{
        activity::WorkActivity,
        input_queue::QueueDisposition,
        native::{HarnessConnectionEpoch, LoadEpoch, ResourceEpoch},
    },
};
pub(super) struct PendingObservation {
    probe: ObservationProbe,
    batch: MetadataBatch,
    stored: AtomicBool,
    action: tokio::sync::Mutex<()>,
    source_gap: bool,
}
impl PendingObservation {
    pub(super) fn persisted_fingerprint(&self) -> Result<[u8; 32], DispatchError> {
        if !self.stored.load(Ordering::SeqCst) {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        Ok(self.batch.header.fingerprint)
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
    pub(super) fn fail_source(&self, source: &SourceSlot<P, L>, error: DispatchError) {
        #[cfg(windows)]
        let wakes;
        {
            let _gate = match source.pre.authority.gate.lock() {
                Ok(v) => v,
                Err(e) => e.into_inner(),
            };
            source.pre.authority.closed.store(true, Ordering::SeqCst);
            #[cfg(windows)]
            {
                wakes = source.pre.authority.invalidate_results();
            }
            source.view_attachment.seal();
            if let Ok(mut memory) = source.memory.lock() {
                memory.error = Some(error);
                memory.phase = SourcePhase::Unknown;
            }
            if let Ok(io) = source.io.lock()
                && let Some(io) = &*io
            {
                io.request_stop();
            }
        }
        #[cfg(windows)]
        for wake in wakes {
            wake.notify();
        }
        #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
        self.repository
            .stop_approval_test_source(source.pre.metadata().instance_id);
        Self::notify_approval_source_change(source);
        source.changed.notify_waiters();
        if let Ok(targets) = source.targets.lock() {
            for resource in targets.values() {
                resource.target.wake_writer();
                resource.target.changed.notify_waiters();
                self.environment.wake(resource.pre.native());
            }
        }
    }
    #[cfg(windows)]
    pub(super) fn end_source_replies(&self, source: &SourceSlot<P, L>) {
        let replies = match source.source_replies.lock() {
            Ok(replies) => replies.values().cloned().collect::<Vec<_>>(),
            Err(error) => error.into_inner().values().cloned().collect::<Vec<_>>(),
        };
        for reply in replies {
            reply.receiver_ended();
        }
    }
    pub(super) async fn pump_source(
        self: Arc<Self>,
        source: Arc<SourceSlot<P, L>>,
        connection: P::Source,
        mut receiver: P::Receiver,
        ack: P::Acknowledger,
    ) {
        loop {
            match self.protocol.next(&connection, &mut receiver).await {
                ProtocolRead::Received(lease) => {
                    let applied = self.apply_observation(&source, &lease);
                    match applied {
                        Ok(mut applied) => {
                            if let Err(rejected) = self.protocol.acknowledge(&ack, lease, &applied)
                            {
                                #[cfg(windows)]
                                applied.source_reply_ack_rejected();
                                let terminal = self.protocol.fail_observation(
                                    &connection,
                                    rejected.lease,
                                    rejected.error,
                                );
                                self.source_lost(&source, terminal).await;
                                return;
                            }
                            #[cfg(windows)]
                            applied.source_reply_acknowledged();
                        }
                        Err(error) => {
                            let terminal =
                                self.protocol.fail_observation(&connection, lease, error);
                            self.source_lost(&source, terminal).await;
                            return;
                        }
                    }
                }
                ProtocolRead::Terminal(terminal) => {
                    self.source_lost(&source, terminal).await;
                    return;
                }
            }
        }
    }
    #[expect(
        clippy::too_many_lines,
        reason = "One original Received receipt is retained and classified into its complete atomic fact batch before acknowledgement."
    )]
    fn apply_observation(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        lease: &P::Lease,
    ) -> Result<AppliedObservation<P::Scope>, DispatchError> {
        let view = self.protocol.view(lease)?;
        if source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .scope
            .as_ref()
            != Some(&view.scope)
        {
            return Err(DispatchError::InvalidScope);
        }
        source
            .raw
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .push(view.raw.clone());
        if matches!(view.incoming, NativeIncoming::Unsupported { .. }) {
            let original = self
                .protocol
                .unsupported_from_lease(lease)?
                .ok_or(DispatchError::CorruptState)?;
            self.unsupported(source, original)?;
        }
        let rpc = match &view.incoming {
            NativeIncoming::Created { rpc, .. }
            | NativeIncoming::Sessions { rpc }
            | NativeIncoming::Initialized { rpc, .. }
            | NativeIncoming::Resumed { rpc, .. }
            | NativeIncoming::Closed { rpc, .. }
            | NativeIncoming::RemoteError { rpc, .. }
            | NativeIncoming::OtherResponse { rpc }
            | NativeIncoming::PromptV1 { rpc, .. }
            | NativeIncoming::PromptV2 { rpc, .. } => Some(rpc),
            _ => None,
        };
        #[cfg(windows)]
        let source_reply_ack = if let Some(rpc) = rpc {
            let operation = source
                .source_replies
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(rpc)
                .cloned();
            operation
                .filter(|operation| operation.matches_incoming(&view.scope, &view.incoming))
                .map(|operation| operation.apply(lease, view.sequence, view.receipt_order))
                .transpose()?
        } else {
            None
        };
        if let Some(operation) =
            rpc.and_then(|rpc| source.operations.lock().ok()?.get(rpc).cloned())
        {
            let value = match &view.incoming {
                NativeIncoming::Initialized { wire, .. }
                    if operation.kind == LifecycleKind::Initialize =>
                {
                    Some(LifecycleResponseValue::Initialized(*wire))
                }
                NativeIncoming::Resumed { session, .. }
                    if operation.kind == LifecycleKind::Resume =>
                {
                    Some(LifecycleResponseValue::Resumed(session.clone()))
                }
                NativeIncoming::Closed { session, .. }
                    if operation.kind == LifecycleKind::Close =>
                {
                    Some(LifecycleResponseValue::Closed(session.clone()))
                }
                NativeIncoming::RemoteError { .. } => Some(LifecycleResponseValue::Error),
                _ => None,
            };
            if let Some(value) = value {
                let mut response = operation
                    .response
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                if response.is_some() {
                    return Err(DispatchError::ObservationOrder);
                }
                *response = Some(LifecycleResponse {
                    sequence: view.sequence,
                    receipt: view.receipt_order,
                    value,
                });
                drop(response);
                operation.changed.notify_waiters();
            }
        }
        let targets: Vec<_> = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .filter(|r| {
                r.recorded.load(Ordering::SeqCst) && !r.pre.authority.closed.load(Ordering::SeqCst)
            })
            .cloned()
            .collect();
        let session = match &view.incoming {
            NativeIncoming::State { session, .. }
            | NativeIncoming::Permission { session, .. }
            | NativeIncoming::OtherSession { session } => Some(session.as_ref()),
            _ => None,
        };
        let resource = targets
            .iter()
            .find(|r| {
                session.is_some_and(|s| r.pre.native().native_id() == s)
                    || rpc.is_some_and(|rpc| {
                        r.target.facts.lock().is_ok_and(|f| {
                            f.pending_input.as_ref().is_some_and(|i| &i.rpc == rpc)
                                || f.retired_prompts.contains_key(rpc)
                        })
                    })
            })
            .cloned();
        let known_external_read = if matches!(view.incoming, NativeIncoming::State { .. }) {
            if let Some(native_id) = session {
                source
                    .external_reads
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .keys()
                    .any(|key| key.native_id() == native_id)
            } else {
                false
            }
        } else {
            false
        };
        let known_capacity = if let Some(session) = session {
            let key =
                super::NativeSessionKey::from_config(Some(source.pre.metadata.source_id), session)
                    .map_err(|_| DispatchError::InvalidTarget)?;
            let recorded = source
                .capacity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(&key)
                .map(|capacity| {
                    capacity
                        .record
                        .lock()
                        .map(|record| record.is_some())
                        .map_err(|_| DispatchError::Stopped)
                })
                .transpose()?
                .unwrap_or(false);
            recorded
                || self.environment.ownership.owns_live(&key, &source.pre)?
                || source
                    .capacity_activity
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .recognizes_member(session)
        } else {
            false
        };
        let permission = if let NativeIncoming::Permission { rpc, session } = &view.incoming {
            let original = self
                .protocol
                .permission_from_lease(lease)?
                .ok_or(DispatchError::CorruptState)?;
            let identity = self.protocol.permission_identity(&original)?;
            if identity.scope != view.scope
                || &identity.rpc != rpc
                || &identity.native_session != session
            {
                return Err(DispatchError::InvalidScope);
            }
            let slot = {
                let _source_gate = source
                    .pre
                    .authority
                    .gate
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                let association = resource
                    .as_ref()
                    .filter(|resource| !resource.pre.authority.closed.load(Ordering::SeqCst))
                    .map(|resource| resource.pre.clone());
                let slot = Arc::new(SourcePermissionSlot {
                    approval: std::sync::OnceLock::new(),
                    response_owner: std::sync::Mutex::new(None),
                    request: original,
                    scope: view.scope.clone(),
                    session: session.clone(),
                    rpc: rpc.clone(),
                    sequence: view.sequence,
                    receipt: view.receipt_order,
                    association,
                    action: std::sync::Mutex::new(()),
                    state: std::sync::Mutex::new(PermissionState {
                        obligation: None,
                        claimed: None,
                        written: false,
                        retired: false,
                    }),
                });
                let mut retained = source
                    .permissions
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                if retained.contains_key(rpc) {
                    return Err(DispatchError::Conflict);
                }
                retained.insert(rpc.clone(), slot.clone());
                slot
            };
            // The original branded request is already in its stable source owner before RNG/SQL can fail.
            slot.state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .obligation = Some(Self::new_id()?);
            self.retain_approval(source, &slot)?;
            source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .touch(session, std::time::Instant::now())
                .map_err(|_| DispatchError::ObservationOrder)?;
            Some(slot)
        } else {
            None
        };
        let mut facts = Vec::new();
        if let Some(resource) = resource {
            let requirement = if let Some(permission) = &permission {
                permission
                    .snapshot()?
                    .obligation
                    .ok_or(DispatchError::RequiredStateUnavailable)?
            } else {
                Self::new_id()?
            };
            let incoming = view.incoming.clone();
            let metadata = resource.pre.metadata();
            let epoch = ResourceEpoch {
                runtime: metadata.source.runtime,
                connection: HarnessConnectionEpoch(metadata.source.connection_epoch),
                load: LoadEpoch(metadata.load_epoch),
            };
            resource.queue.apply(epoch, &mut |order| {
                let mut f = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                f.raw.push(view.raw.clone());
                let mut kind = None;
                let mut result = None;
                let mut reason = None;
                let mut message = None;
                let mut request_rpc = None;
                let mut transition = TransitionKind::Repeated;
                let mut policy = NativeQueuePolicy::None;
                let mut observed_attempt = None;
                match &incoming {
                    NativeIncoming::PromptV1 { rpc, reason: value } => {
                        f.live_observed = true;
                        let classified = KnownStopReason::classify(Some(value));
                        let stop = if classified == KnownStopReason::Error {
                            KnownStopReason::Unknown
                        } else {
                            classified
                        };
                        reason = Some(stop);
                        result = Some(result_kind(stop));
                        kind = Some(FactKind::V1PromptResult);
                        if let Some(input) =
                            f.pending_input.as_mut().filter(|input| &input.rpc == rpc)
                        {
                            observed_attempt = Some(input.attempt);
                            input.native_reason = Some(stop);
                        }
                        observed_attempt.ok_or(DispatchError::CorruptState)?;
                        if stop.reliable() {
                            f.terminal = Some(stop);
                            f.terminal_sequence = view.sequence;
                            f.foreground = WorkActivity::Idle;
                            f.foreground_evidence = ForegroundEvidence::ReliableTerminal;
                        } else {
                            f.foreground = WorkActivity::Unknown;
                            f.foreground_evidence = ForegroundEvidence::UnreportedStop;
                        }
                    }
                    NativeIncoming::PromptV2 {
                        rpc,
                        message: value,
                    } => {
                        message = Some(value.clone());
                        kind = Some(FactKind::V2Inserted);
                        if let Some(input) =
                            f.pending_input.as_mut().filter(|input| &input.rpc == rpc)
                        {
                            observed_attempt = Some(input.attempt);
                            input.inserted = true;
                        } else {
                            observed_attempt =
                                f.retired_prompts.remove(rpc).map(|retired| retired.attempt);
                        }
                        observed_attempt.ok_or(DispatchError::CorruptState)?;
                    }
                    NativeIncoming::RemoteError { rpc, .. } => {
                        kind = Some(FactKind::PromptRejected);
                        result = Some(ResultKind::Rejected);
                        reason = None;
                        if let Some(input) =
                            f.pending_input.as_mut().filter(|input| &input.rpc == rpc)
                        {
                            observed_attempt = Some(input.attempt);
                            input.native_reason = Some(KnownStopReason::Error);
                        } else {
                            observed_attempt =
                                f.retired_prompts.remove(rpc).map(|retired| retired.attempt);
                        }
                        observed_attempt.ok_or(DispatchError::CorruptState)?;
                    }
                    NativeIncoming::State {
                        state,
                        reason: value,
                        origin,
                        ..
                    } => {
                        let before = f.foreground;
                        let snapshot = *origin != ObservationOrigin::Live;
                        let context_only = snapshot
                            && (f.live_observed || f.pending_input.is_some() || f.interval_live);
                        if context_only {
                            transition = TransitionKind::Snapshot;
                            kind = Some(match state.as_ref() {
                                "running" => FactKind::ForegroundRunning,
                                "requires_action" => FactKind::ForegroundRequiresAction,
                                "idle" => {
                                    reason = Some(KnownStopReason::classify(value.as_deref()));
                                    FactKind::ForegroundIdle
                                }
                                _ => FactKind::ForegroundUnknown,
                            });
                        } else {
                            match state.as_ref() {
                                "running" | "requires_action" => {
                                    f.foreground = WorkActivity::Active;
                                    f.foreground_evidence = ForegroundEvidence::Active;
                                    kind = Some(if state.as_ref() == "running" {
                                        FactKind::ForegroundRunning
                                    } else {
                                        FactKind::ForegroundRequiresAction
                                    });
                                    if !f.interval_open {
                                        f.interval = f
                                            .interval
                                            .checked_add(1)
                                            .ok_or(DispatchError::SequenceExhausted)?;
                                        f.interval_open = true;
                                        f.interval_live = false;
                                        f.terminal = None;
                                        transition = if snapshot {
                                            TransitionKind::Snapshot
                                        } else {
                                            TransitionKind::New
                                        };
                                        policy = NativeQueuePolicy::BeginExternal;
                                    }
                                    if !snapshot {
                                        f.interval_live = true;
                                    }
                                }
                                "idle" => {
                                    let stop = KnownStopReason::classify(value.as_deref());
                                    reason = Some(stop);
                                    f.foreground = WorkActivity::Idle;
                                    kind = Some(FactKind::ForegroundIdle);
                                    if f.interval_open
                                        && f.interval_live
                                        && !snapshot
                                        && stop.reliable()
                                    {
                                        kind = Some(FactKind::ForegroundTerminal);
                                        result = Some(result_kind(stop));
                                        f.terminal = Some(stop);
                                        f.terminal_sequence = view.sequence;
                                        f.interval_open = false;
                                        f.interval_live = false;
                                        f.foreground_evidence =
                                            ForegroundEvidence::ReliableTerminal;
                                        transition = TransitionKind::New;
                                        policy = NativeQueuePolicy::EndExternal {
                                            disposition: if stop == KnownStopReason::EndTurn {
                                                QueueDisposition::Advance
                                            } else {
                                                QueueDisposition::Suspend
                                            },
                                            reason: stop,
                                        };
                                    } else if snapshot && !f.interval_live {
                                        f.interval_open = false;
                                        f.foreground_evidence = ForegroundEvidence::IdleSnapshot;
                                        transition = TransitionKind::Snapshot;
                                        policy = NativeQueuePolicy::ClearExternal;
                                    } else if f.interval_open {
                                        f.foreground = WorkActivity::Unknown;
                                        f.foreground_evidence = ForegroundEvidence::UnreportedStop;
                                    } else if snapshot && before == WorkActivity::Unknown {
                                        f.foreground_evidence = ForegroundEvidence::IdleSnapshot;
                                        transition = TransitionKind::Snapshot;
                                    }
                                }
                                _ => {
                                    f.foreground = WorkActivity::Unknown;
                                    f.foreground_evidence = ForegroundEvidence::Unknown;
                                    kind = Some(FactKind::ForegroundUnknown);
                                    transition = TransitionKind::Unknown;
                                }
                            }
                        }
                        if !snapshot {
                            f.live_observed = true;
                        }
                    }
                    NativeIncoming::Permission { rpc, .. } => {
                        let original = permission.clone().ok_or(DispatchError::CorruptState)?;
                        if f.permissions.contains_key(rpc) {
                            return Err(DispatchError::Conflict);
                        }
                        f.permissions.insert(rpc.clone(), original);
                        kind = Some(FactKind::ApprovalPresent);
                        request_rpc = Some(rpc.clone());
                    }
                    _ => {}
                }
                if let Some(kind) = kind {
                    let foreground = f.foreground;
                    f.activity
                        .observe_foreground(epoch, foreground)
                        .map_err(|_| DispatchError::InvalidResource)?;
                    if kind == FactKind::V2Inserted
                        && let Some(work) = f.pending_input.as_ref().map(|p| p.work.clone())
                        && f.pending_input
                            .as_ref()
                            .is_some_and(|input| Some(input.attempt) == observed_attempt)
                    {
                        f.activity
                            .confirm_insertion(&work)
                            .map_err(|_| DispatchError::InvalidResource)?;
                    }
                    if kind == FactKind::ApprovalPresent {
                        f.activity
                            .observe_permission(epoch, requirement, None)
                            .map_err(|_| DispatchError::InvalidResource)?;
                    }
                    if matches!(
                        kind,
                        FactKind::ForegroundIdle
                            | FactKind::ForegroundTerminal
                            | FactKind::V1PromptResult
                    ) && f.foreground == WorkActivity::Idle
                    {
                        f.last_idle_sequence = view.sequence;
                    }
                    if (observed_attempt.is_none()
                        || f.pending_input
                            .as_ref()
                            .is_some_and(|input| Some(input.attempt) == observed_attempt))
                        && let Some(input) = &mut f.pending_input
                    {
                        input.requirements.push(requirement);
                    }
                    f.requirements.insert(requirement);
                    facts.push(NativeFactMetadata {
                        requirement,
                        target: metadata.target,
                        owner: metadata.owner,
                        load_epoch: metadata.load_epoch,
                        owner_order: order,
                        attempt: observed_attempt,
                        kind,
                        message,
                        result,
                        reason,
                        transition,
                        interval: Some(f.interval),
                        request: request_rpc,
                    });
                }
                f.events.push_back(QueueOwnerEvent::ActivityChanged);
                drop(f);
                Ok(policy)
            })?;
            resource.target.wake_writer();
            resource.target.changed.notify_waiters();
            self.environment.wake(resource.pre.native());
        } else if !known_external_read
            && !known_capacity
            && matches!(
                view.incoming,
                NativeIncoming::State { .. } | NativeIncoming::Permission { .. }
            )
        {
            if let Some(session) = session {
                source
                    .capacity_activity
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .unrecognized(session);
            }
            source
                .pre
                .authority
                .quarantined
                .store(true, Ordering::SeqCst);
            // Unknown source-wide facts cannot leave a previously idle resource sendable.
            for resource in &targets {
                let metadata = resource.pre.metadata();
                let epoch = ResourceEpoch {
                    runtime: metadata.source.runtime,
                    connection: HarnessConnectionEpoch(metadata.source.connection_epoch),
                    load: LoadEpoch(metadata.load_epoch),
                };
                resource.queue.apply(epoch, &mut |_| {
                    let mut f = resource
                        .target
                        .facts
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    f.foreground = WorkActivity::Unknown;
                    f.foreground_evidence = ForegroundEvidence::Unknown;
                    drop(f);
                    Ok(NativeQueuePolicy::None)
                })?;
                resource.target.wake_writer();
                resource.target.changed.notify_waiters();
            }
        }
        if !facts.is_empty() || permission.is_some() {
            let mut header = ObservationMetadata {
                observation_id: Self::new_id()?,
                source_id: source.pre.metadata().source_id,
                runtime: self.runtime,
                instance_id: source.pre.metadata().instance_id,
                connection_epoch: source.pre.metadata().connection_epoch,
                sequence: view.sequence,
                receipt: view.receipt_order,
                observed_at: self.clock.unix_millis(),
                fingerprint: [0; 32],
            };
            header.fingerprint = observation_fingerprint(&header, &facts)?;
            let probe = ObservationProbe {
                header: header.clone(),
                source: source.pre.clone(),
            };
            let batch = MetadataBatch { header, facts };
            let pending = Arc::new(PendingObservation {
                probe,
                batch,
                stored: AtomicBool::new(false),
                action: tokio::sync::Mutex::new(()),
                source_gap: permission.is_some(),
            });
            if pending.source_gap {
                source
                    .memory
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .requirements
                    .insert(
                        pending.batch.header.observation_id,
                        DispatchError::RequiredStateUnavailable,
                    );
                source.pre.authority.required.fetch_add(1, Ordering::SeqCst);
            }
            source
                .observations
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .push(pending.clone());
            let core = self.clone();
            let slot = source.clone();
            let failed_core = self.clone();
            let failed_source = source.clone();
            source.tasks.spawn_guarded(
                async move {
                    let _ = core.persist_observation(&slot, &pending).await;
                },
                move || {
                    failed_core
                        .fail_source(&failed_source, DispatchError::RequiredStateUnavailable);
                },
            )?;
        }
        if let Some(permission) = permission
            && source.pre.authority.closing.load(Ordering::SeqCst)
            && source.close_intent.get().is_some()
        {
            self.close_source_permission(source, &permission)?;
        }
        let wire = source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .initialized
            .as_ref()
            .map(|initialized| initialized.metadata.wire);
        if let Some(wire) = wire
            && let Some(observation) = self.protocol.capacity_observation(lease, wire)?
        {
            if let super::CapacityUpdate::Child { session: child, .. } = &observation.update {
                self.accept_tree_observation(
                    source,
                    &observation.session,
                    child,
                    view.sequence,
                    view.receipt_order,
                )?;
            }
            let mut activity = source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            activity
                .observe(&observation, std::time::Instant::now())
                .map_err(|_| DispatchError::ObservationOrder)?;
            #[cfg(all(
                test,
                feature = "native-dispatch-dev-fixtures",
                feature = "approval-owner-dev-fixtures",
                feature = "approval-owner-acceptance-fixtures"
            ))]
            if matches!(
                observation.update,
                super::CapacityUpdate::State(_) | super::CapacityUpdate::Tool { .. }
            ) && let Ok(raw) = source.raw.lock()
                && let Some(raw_index) = raw.len().checked_sub(1)
            {
                use super::runtime_child_source_clock_probe::{
                    L6CapacityReceipt, l6_record_capacity_receipt,
                };
                l6_record_capacity_receipt(
                    Arc::as_ptr(self) as usize,
                    &L6CapacityReceipt {
                        source: source.pre.metadata.source_id,
                        instance: source.pre.metadata.instance_id,
                        session: observation.session.clone(),
                        raw_index,
                        sequence: view.sequence,
                        receipt: view.receipt_order,
                        freshness: match observation.freshness {
                            super::CapacityFreshness::Current => "current",
                            super::CapacityFreshness::History => "history",
                            super::CapacityFreshness::Unconfirmed => "unconfirmed",
                        },
                        post: activity.l6_session_post_for_test(&observation.session),
                    },
                );
            }
            if known_capacity || known_external_read {
                activity.recognize(&observation.session);
            }
        }
        source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .observed_sequence = view.sequence;
        Ok(AppliedObservation {
            scope: view.scope,
            sequence: view.sequence,
            receipt: view.receipt_order,
            #[cfg(windows)]
            source_reply_ack,
        })
    }
    async fn source_lost(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        terminal: SourceTerminal<P::Scope>,
    ) {
        self.fail_source(source, DispatchError::Stopped);
        #[cfg(windows)]
        self.end_source_replies(source);
        let result = (|| {
            let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
            if memory.scope.as_ref() != Some(terminal.scope()) {
                return Err(DispatchError::InvalidScope);
            }
            let binding = memory.binding.ok_or(DispatchError::InvalidScope)?;
            drop(memory);
            let mut resources: Vec<_> = source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .filter(|r| r.recorded.load(Ordering::SeqCst))
                .cloned()
                .collect();
            resources.sort_by_key(|r| r.pre.metadata().target.encode());
            let mut targets = Vec::new();
            for resource in resources {
                let requirement = Self::new_id()?;
                let metadata = resource.pre.metadata();
                let epoch = ResourceEpoch {
                    runtime: self.runtime,
                    connection: HarnessConnectionEpoch(metadata.source.connection_epoch),
                    load: LoadEpoch(metadata.load_epoch),
                };
                resource.queue.apply(epoch, &mut |order| {
                    let mut f = resource
                        .target
                        .facts
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    f.foreground = WorkActivity::Unknown;
                    f.foreground_evidence = ForegroundEvidence::Unknown;
                    f.requirements.insert(requirement);
                    drop(f);
                    targets.push(LossTargetMetadata {
                        target: metadata.target,
                        load_epoch: metadata.load_epoch,
                        owner: metadata.owner,
                        owner_order: order,
                        requirement,
                        fingerprint: super::source::metadata_stamp(
                            b"native-source-loss-target",
                            requirement,
                            order,
                            0,
                        ),
                    });
                    Ok(NativeQueuePolicy::None)
                })?;
            }
            Ok(SourceLossProbe {
                source: source.pre.clone(),
                binding,
                loss_id: terminal.id(),
                requirement: Self::new_id()?,
                reason: terminal.reason(),
                targets,
                fingerprint: super::source::metadata_stamp(
                    b"native-source-loss",
                    terminal.id(),
                    0,
                    0,
                ),
                at: self.clock.unix_millis(),
            })
        })();
        if let Ok(probe) = result {
            let probe = Arc::new(probe);
            if let Ok(mut stored) = source.loss.lock() {
                *stored = Some(probe.clone());
            }
            if let Ok(mut memory) = source.memory.lock() {
                memory
                    .requirements
                    .insert(probe.requirement, DispatchError::RequiredStateUnavailable);
            }
            let _ = self.persist_source_loss(source, &probe).await;
        }
        // This cleanup continues inside the original registered pump, including Storage Drop.
        // The physical report is retained; loss itself never becomes a release capability.
        if let Ok(deadline) = self.effective_close_deadline(source) {
            let _ = self.collect_source_physical(source, deadline).await;
        }
    }
    async fn persist_observation(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        pending: &PendingObservation,
    ) -> Result<(), DispatchError> {
        let _action = pending.action.lock().await;
        if pending.stored.load(Ordering::SeqCst) {
            return Ok(());
        }
        let result = self
            .repository
            .record_observation(
                &pending.probe,
                &pending.batch,
                &self.context(),
                &self.deadline()?,
            )
            .await;
        let resources = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for resource in resources {
            let mut facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            for fact in &pending.batch.facts {
                if fact.target == resource.pre.metadata.target
                    && fact.load_epoch == resource.pre.metadata.load_epoch
                {
                    if result.is_ok() {
                        facts.requirements.remove(&fact.requirement);
                        facts.failed_requirements.remove(&fact.requirement);
                    } else {
                        facts.failed_requirements.insert(fact.requirement);
                    }
                }
            }
            drop(facts);
            resource.target.wake_writer();
            resource.target.changed.notify_waiters();
            self.environment.wake(resource.pre.native());
        }
        result?;
        if pending.source_gap {
            source
                .memory
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .requirements
                .remove(&pending.batch.header.observation_id);
            source.pre.authority.required.fetch_sub(1, Ordering::SeqCst);
            let resources = source
                .all_resources
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            for resource in resources {
                resource.target.wake_writer();
                resource.target.changed.notify_waiters();
                self.environment.wake(resource.pre.native());
            }
        }
        pending.stored.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn persist_source_loss(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        probe: &SourceLossProbe,
    ) -> Result<(), DispatchError> {
        let saved = self
            .repository
            .record_source_loss(probe, &self.context(), &self.deadline()?)
            .await;
        if let Err(error) = saved {
            source
                .memory
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .requirements
                .insert(probe.requirement, error);
            return Err(error);
        }
        source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .requirements
            .remove(&probe.requirement);
        let resources = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for resource in resources {
            let mut facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            for target in &probe.targets {
                if target.load_epoch == resource.pre.metadata.load_epoch {
                    facts.requirements.remove(&target.requirement);
                    facts.failed_requirements.remove(&target.requirement);
                }
            }
            drop(facts);
            resource.target.wake_writer();
            resource.target.changed.notify_waiters();
        }
        Ok(())
    }
    pub(super) async fn reconcile_source_observations(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let pending = source
            .observations
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for record in pending {
            self.persist_observation(source, &record).await?;
        }
        let loss = source
            .loss
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(loss) = loss {
            self.persist_source_loss(source, &loss).await?;
        }
        Ok(())
    }
    pub(super) async fn reconcile_resource_observations(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        let pending = source
            .observations
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .filter(|record| {
                record
                    .batch
                    .facts
                    .iter()
                    .any(|fact| fact.load_epoch == resource.pre.metadata.load_epoch)
            })
            .cloned()
            .collect::<Vec<_>>();
        for record in pending {
            self.persist_observation(source, &record).await?;
        }
        Ok(())
    }
}
const fn result_kind(reason: KnownStopReason) -> ResultKind {
    match reason {
        KnownStopReason::EndTurn => ResultKind::Completed,
        KnownStopReason::Error => ResultKind::Failed,
        KnownStopReason::Unknown => ResultKind::Unknown,
        _ => ResultKind::Stopped,
    }
}
