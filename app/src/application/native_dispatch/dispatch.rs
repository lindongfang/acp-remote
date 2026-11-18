//! The retained selected owner, original reservation, frame and ticket have one task owner.
use super::{
    Arc, AtomicBool, AttemptMetadata, AttemptProbe, BoundWrite, ConfigurationPort, DispatchError,
    DispatchFenceRejection, DispatchPlan, DispatchPollIo, DispatchProbe, DispatchWriteBoundary,
    DispatchWriteStep, EncodedPromptFrame, EntityId, KnownStopReason, NativeControlRepository,
    NativeCoordinator, NativeDispatchRepository, NativeResourceRepository, NativeRuntimeOptions,
    NativeSessionKey, Ordering, OwnedNativeSource, PromptTicket, ProtocolVersion, ReleaseKind,
    ReleaseProof, ReservedPrompt, SelectedProof, SelectionKind, SelectionStamp, SessionBinding,
    SourceLaunchPort, StartupProof, TerminalTransportProof, TransportFact, TransportProgress,
    environment::{NativeWake, PendingInput, RetiredPrompt},
    runtime::{ResourceSlot, SourceSlot},
};
use crate::{
    application::queue_owner::{
        NativeQueuePolicy, PreparedCapture, SelectedContext, SelectedFence,
    },
    domain::{
        activity::{ForegroundOutcome, WorkActivity},
        input_queue::QueueDisposition,
    },
};
use std::{sync::Mutex, task::Context};
use tokio::sync::{Notify, mpsc};

pub(super) struct RunningDispatch<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) probe: Arc<DispatchProbe>,
    pub(super) state: Mutex<RunningState<P>>,
    action: tokio::sync::Mutex<()>,
    priority_cancelled: AtomicBool,
}
pub(super) struct RunningState<P: crate::application::native_dispatch::NativeApprovalPort> {
    prepared: Option<PreparedCapture<P::Retained>>,
    selected: Option<SelectedContext<P::Retained>>,
    reservation: Option<P::Reservation>,
    frame: Option<P::Frame>,
    attempt: Option<Arc<AttemptProbe>>,
    ticket: Option<Arc<P::Ticket>>,
    terminal: Option<TransportFact>,
    transport_stored: bool,
    settled: bool,
    selection: Option<SelectionStamp>,
    attempt_identity: Option<EntityId>,
    terminal_record: Option<Arc<TerminalTransportProof>>,
    previous: Vec<(Arc<AttemptProbe>, Arc<P::Ticket>, TransportFact)>,
    retiring: bool,
    retired_probe: Option<P::Probe>,
    release: Option<Arc<ReleaseProof>>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> RunningDispatch<P> {
    pub(super) fn request_cancel(&self) -> Result<(), DispatchError> {
        self.priority_cancelled.store(true, Ordering::SeqCst);
        let state = self.state.lock().map_err(|_| DispatchError::Stopped)?;
        if let Some(ticket) = &state.ticket {
            ticket.request_cancel();
        }
        drop(state);
        Ok(())
    }
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> RunningDispatch<P> {
    pub(super) fn cancel_for_resource_close(&self) -> Result<(), DispatchError> {
        let state = self.state.lock().map_err(|_| DispatchError::Stopped)?;
        if let Some(ticket) = &state.ticket {
            ticket.request_cancel();
        }
        drop(state);
        Ok(())
    }
    pub(super) async fn action_for_resource_close(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.action.lock().await
    }
}
#[cfg(test)]
impl<P: crate::application::native_dispatch::NativeApprovalPort> RunningDispatch<P> {
    pub(super) fn transport_for_test(&self) -> Result<Option<TransportProgress>, DispatchError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .ticket
            .as_ref()
            .map(|ticket| ticket.progress()))
    }
}
struct PromptBoundary<P: crate::application::native_dispatch::NativeApprovalPort> {
    selected: Arc<dyn SelectedFence>,
    target: Arc<super::environment::NativeTarget<P>>,
    binding: Arc<SessionBinding<P::Scope>>,
    attempt: Arc<AttemptProbe>,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> DispatchWriteBoundary<P::Scope>
    for PromptBoundary<P>
{
    #[expect(
        clippy::significant_drop_tightening,
        reason = "Keep target facts and source fence locked through the same actual IO poll or resource-waker registration."
    )]
    fn check_and_poll(
        &self,
        w: &BoundWrite<P::Scope>,
        _: DispatchWriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        self.selected
            .with_current(&mut || {
                let mut f = self
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                let _gate = self
                    .binding
                    .authority
                    .source
                    .gate
                    .lock()
                    .map_err(|_| DispatchError::RequiredStateUnavailable)?;
                if w.scope != self.binding.scope
                    || w.rpc != self.attempt.metadata.rpc
                    || w.native != self.binding.native
                    || w.resource != self.binding.resource
                    || w.config_identity != self.binding.config_identity
                    || w.config_revision != self.binding.config_revision
                    || self.binding.authority.releasing.load(Ordering::SeqCst)
                    || self.binding.authority.closed.load(Ordering::SeqCst)
                    || self.binding.authority.source.closed.load(Ordering::SeqCst)
                {
                    return Err(DispatchError::StaleSelection);
                }
                if self.binding.authority.source.closing.load(Ordering::SeqCst)
                    || self
                        .binding
                        .authority
                        .source
                        .quarantined
                        .load(Ordering::SeqCst)
                    || !f
                        .pending_input
                        .as_ref()
                        .is_some_and(|p| p.attempt == self.attempt.metadata.attempt_id)
                {
                    return Err(DispatchError::RequiredStateUnavailable);
                }
                if self
                    .binding
                    .authority
                    .source
                    .required
                    .load(Ordering::SeqCst)
                    != 0
                    || !f.requirements.is_empty()
                    || !f.failed_requirements.is_empty()
                    || !f.control_holds.is_empty()
                {
                    f.writer_parked = Some(cx.waker().clone());
                    io.park().map_err(|_| DispatchError::ProtocolUnavailable)?;
                    return Ok(());
                }
                let _ = io.poll(cx);
                Ok(())
            })
            .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)
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
    pub(crate) fn launch(
        self: &Arc<Self>,
        mut receiver: mpsc::UnboundedReceiver<NativeWake>,
    ) -> Result<(), DispatchError> {
        let core = self.clone();
        let failed = self.clone();
        self.tasks.spawn_guarded(async move{
            let result = async {
                let deadline = core.deadline()?;
                core.repository.interrupt_previous_runtime(
                    &StartupProof { runtime: core.runtime, exclusive: core.claim.clone() },
                    &core.context(), &deadline,
                ).await?;
                core.status.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                    .map_err(|_| DispatchError::NodeStopping)?;
                core.start_capacity_scan()
            }.await;
            if let Err(error) = result {
                core.fail_all(error);
                return;
            }
            loop{let stopped=core.stop.notified();tokio::pin!(stopped);stopped.as_mut().enable();if core.running().is_err(){return;}tokio::select!{biased;()=&mut stopped=>return,wake=receiver.recv()=>{let Some(wake)=wake else{return};let result=match wake{NativeWake::Worker(key)=>core.wake_worker(key),NativeWake::Priority(key)=>core.launch_priority(key.clone()).and_then(|()|core.wake_worker(key))};if let Err(error)=result{core.fail_all(error);return;}}}}
        },move||failed.fail_all(DispatchError::RequiredStateUnavailable))?;
        Ok(())
    }
    pub(super) fn fail_all(&self, error: DispatchError) {
        self.status.store(2, Ordering::SeqCst);
        self.environment.stopping.store(true, Ordering::SeqCst);
        self.stop.notify_waiters();
        if let Ok(sources) = self.sources.lock() {
            for source in &*sources {
                self.fail_source(source, error);
            }
        }
    }
    fn wake_worker(self: &Arc<Self>, key: NativeSessionKey) -> Result<(), DispatchError> {
        let mut workers = self.workers.lock().map_err(|_| DispatchError::Stopped)?;
        if let Some(worker) = workers.get(&key) {
            worker.notify_one();
            return Ok(());
        }
        let worker = Arc::new(Notify::new());
        workers.insert(key.clone(), worker.clone());
        drop(workers);
        let core = self.clone();
        let failed = self.clone();
        self.tasks.spawn_guarded(
            async move {
                loop {
                    let notified = worker.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    let stopped = core.stop.notified();
                    tokio::pin!(stopped);
                    stopped.as_mut().enable();
                    if core.running().is_err() {
                        return;
                    }
                    if let Ok(Some(target)) = core.queue.target_for_native(&key) {
                        let _ = core.drive_target(target, &key).await;
                    }
                    tokio::select! {()=&mut notified=>{},()=&mut stopped=>return}
                }
            },
            move || failed.fail_all(DispatchError::RequiredStateUnavailable),
        )?;
        Ok(())
    }
    async fn drive_target(
        self: &Arc<Self>,
        target: EntityId,
        key: &NativeSessionKey,
    ) -> Result<(), DispatchError> {
        let current = self
            .runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(key)
            .cloned();
        if let Some(run) = current {
            let (source, resource) = self.installed_for_run(key, &run)?;
            let core = self.clone();
            let slot = source.clone();
            let native = key.clone();
            return self
                .run_source_job(source, async move {
                    let _action = run.action.lock().await;
                    if run
                        .state
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .terminal
                        .is_some()
                    {
                        core.advance_run(&native, &run).await
                    } else {
                        core.start_attempt(&slot, &resource, &run).await
                    }
                })
                .await;
        }
        let (source, resource) = self.ensure_resource(target).await?;
        let core = self.clone();
        let slot = source.clone();
        let native = key.clone();
        self.run_source_job(source, async move {
            core.prepare_dispatch(target, &native, &slot, &resource)
                .await
        })
        .await
    }
    async fn prepare_dispatch(
        self: &Arc<Self>,
        target: EntityId,
        key: &NativeSessionKey,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        let loaded = resource.loaded.get().ok_or(DispatchError::NotReady)?;
        let (prepared, selected, origin, kind) =
            if let Some(selected) = self.queue.direct(target).await? {
                let origin = selected.origin;
                (None, Some(selected), origin, SelectionKind::ExistingActive)
            } else {
                let Some(prepared) = self.queue.prepare(target).await? else {
                    return Ok(());
                };
                let origin = prepared.origin();
                (Some(prepared), None, origin, SelectionKind::Waiting)
            };
        let metadata = resource.pre.metadata();
        let binding = loaded.binding();
        let probe = Arc::new(DispatchProbe {
            plan: DispatchPlan {
                dispatch_id: Self::new_id()?,
                planned_run_id: Self::new_id()?,
                canonical_target: target,
                native: key.clone(),
                runtime: self.runtime,
                owner_id: metadata.owner,
                instance_id: metadata.source.instance_id,
                resource: binding.resource,
                item_id: origin.submit_request.entity(),
                revision: origin.revision,
                origin,
                config_identity: metadata.source.config_identity,
                config_revision: metadata.source.config_revision,
                selection_kind: kind,
                prepared_at_ms: self.clock.unix_millis(),
            },
            resource: resource.pre.clone(),
            nonce: Arc::new(()),
        });
        let run = Arc::new(RunningDispatch {
            probe,
            state: Mutex::new(RunningState {
                prepared,
                selected,
                reservation: None,
                frame: None,
                attempt: None,
                ticket: None,
                terminal: None,
                transport_stored: false,
                settled: false,
                selection: None,
                attempt_identity: None,
                terminal_record: None,
                previous: Vec::new(),
                retiring: false,
                retired_probe: None,
                release: None,
            }),
            action: tokio::sync::Mutex::new(()),
            priority_cancelled: AtomicBool::new(false),
        });
        source
            .dispatches
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .push(run.clone());
        self.runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .insert(key.clone(), run.clone());
        let _action = run.action.lock().await;
        self.start_attempt(source, resource, &run).await
    }
    #[expect(
        clippy::type_complexity,
        reason = "Return the original paired source/resource capabilities rather than reconstructing them from IDs."
    )]
    fn installed_for_run(
        &self,
        key: &NativeSessionKey,
        run: &RunningDispatch<P>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<ResourceSlot<P>>), DispatchError> {
        let source = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .find(|source| source.pre.metadata.instance_id == run.probe.plan.instance_id)
            .cloned()
            .ok_or(DispatchError::InvalidSource)?;
        let resource = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(key)
            .cloned()
            .ok_or(DispatchError::InvalidResource)?;
        Ok((source, resource))
    }
    #[expect(
        clippy::too_many_lines,
        reason = "One original run action serializes retained body, selection, SQL arm and its actual reservation/frame/ticket."
    )]
    async fn start_attempt(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        run: &Arc<RunningDispatch<P>>,
    ) -> Result<(), DispatchError> {
        if run
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .retiring
        {
            return self.retire_unselected(run).await;
        }
        let existing = run
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .ticket
            .clone();
        if let Some(ticket) = existing {
            let fact = ticket.wait().await;
            run.state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .terminal = Some(fact);
            return self.advance_run(resource.pre.native(), run).await;
        }
        self.repository
            .reserve(&run.probe, &self.context(), &self.deadline()?)
            .await?;
        let loaded = resource.loaded.get().ok_or(DispatchError::NotReady)?;
        let binding = loaded.binding();
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            if state.reservation.is_none() {
                let attempt_id = if let Some(id) = state.attempt_identity {
                    id
                } else {
                    let id = Self::new_id()?;
                    state.attempt_identity = Some(id);
                    id
                };
                let retained = if let Some(selected) = &state.selected {
                    selected.retained.clone()
                } else {
                    state
                        .prepared
                        .as_ref()
                        .ok_or(DispatchError::StaleSelection)?
                        .body()
                        .clone()
                };
                let reservation = self.protocol.reserve_prompt(
                    binding,
                    attempt_id,
                    retained,
                    NativeRuntimeOptions::response(self.options.prompt_response)?,
                )?;
                state.reservation = Some(reservation);
            }
            if state.frame.is_none() {
                state.frame = Some(
                    state
                        .reservation
                        .as_ref()
                        .ok_or(DispatchError::InvalidPromptReservation)?
                        .encode()?,
                );
            }
        }
        let capture = run
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .prepared
            .take();
        if let Some(capture) = capture {
            match self.queue.select(capture).await {
                Ok(selected) => {
                    run.state
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .selected = Some(selected);
                }
                Err((error, capture)) => {
                    {
                        let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                        state.prepared = Some(capture);
                        if error == DispatchError::StaleSelection {
                            state.retiring = true;
                        }
                    }
                    if error == DispatchError::StaleSelection {
                        return self.retire_unselected(run).await;
                    }
                    return Err(error);
                }
            }
        }
        let (attempt, selected_proof, fence) = {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            let selected = state
                .selected
                .as_ref()
                .ok_or(DispatchError::StaleSelection)?;
            if selected.native != run.probe.plan.native
                || selected.target != run.probe.plan.canonical_target
                || selected.active.item() != run.probe.plan.item_id
                || selected.active.revision() != run.probe.plan.revision
                || selected.active.resource() != run.probe.plan.resource
            {
                return Err(DispatchError::StaleSelection);
            }
            selected.with_current(&mut || Ok(()))?;
            let fence = selected.fence.clone();
            let owner_order = selected.active.selection_order();
            let stamp = if let Some(stamp) = state.selection {
                stamp
            } else {
                let stamp = SelectionStamp {
                    selection_id: Self::new_id()?,
                    owner_order,
                    at: self.clock.unix_millis(),
                };
                state.selection = Some(stamp);
                stamp
            };
            let attempt = if let Some(attempt) = &state.attempt {
                attempt.clone()
            } else {
                let wire = source
                    .memory
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .initialized
                    .as_ref()
                    .map(|value| value.metadata.wire)
                    .ok_or(DispatchError::NotReady)?;
                let reservation = state
                    .reservation
                    .as_ref()
                    .ok_or(DispatchError::InvalidPromptReservation)?;
                let attempt = Arc::new(AttemptProbe {
                    dispatch: run.probe.clone(),
                    metadata: AttemptMetadata {
                        attempt_id: state
                            .attempt_identity
                            .ok_or(DispatchError::InvalidPromptReservation)?,
                        dispatch_id: run.probe.plan.dispatch_id,
                        selection: stamp,
                        item_id: run.probe.plan.item_id,
                        revision: run.probe.plan.revision,
                        connection_epoch: run.probe.plan.resource.connection.0,
                        wire,
                        rpc: reservation.rpc().clone(),
                        frame_length: state
                            .frame
                            .as_ref()
                            .ok_or(DispatchError::InvalidPromptReservation)?
                            .byte_len_checked()?,
                        armed_at: self.clock.unix_millis(),
                    },
                });
                state.attempt = Some(attempt.clone());
                drop(state);
                attempt
            };
            (
                attempt,
                SelectedProof {
                    dispatch: run.probe.clone(),
                    stamp,
                },
                fence,
            )
        };
        self.repository
            .arm(
                &attempt,
                &selected_proof,
                &self.context(),
                &self.deadline()?,
            )
            .await?;
        let admitted_sequence = source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .observed_sequence;
        fence.with_current(&mut || {
            let mut facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if let Some(input) = &facts.pending_input {
                return if input.attempt == attempt.metadata.attempt_id {
                    Ok(())
                } else {
                    Err(DispatchError::InvalidResource)
                };
            }
            if !facts.requirements.is_empty() || facts.foreground != WorkActivity::Idle {
                return Err(DispatchError::NotReady);
            }
            let work = facts
                .activity
                .reserve_input(run.probe.plan.item_id, run.probe.plan.planned_run_id)
                .map_err(|_| DispatchError::NotReady)?;
            facts.pending_input = Some(PendingInput {
                dispatch: run.probe.plan.dispatch_id,
                attempt: attempt.metadata.attempt_id,
                rpc: attempt.metadata.rpc.clone(),
                inserted: false,
                transport: None,
                native_reason: None,
                work,
                admitted_sequence,
                requirements: Vec::new(),
            });
            drop(facts);
            Ok(())
        })?;
        let (reservation, frame) = {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
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
        let boundary = Arc::new(PromptBoundary {
            selected: fence,
            target: resource.target.clone(),
            binding: binding.clone(),
            attempt: attempt.clone(),
        });
        let ticket = match self.protocol.enqueue(
            reservation,
            frame,
            boundary,
            NativeRuntimeOptions::at(self.options.prompt_write_wait)?,
        ) {
            Ok(ticket) => Arc::new(ticket),
            Err(rejected) => {
                let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                state.reservation = Some(rejected.reservation);
                state.frame = Some(rejected.frame);
                drop(state);
                return Err(rejected.error);
            }
        };
        run.state.lock().map_err(|_| DispatchError::Stopped)?.ticket = Some(ticket.clone());
        let fact = ticket.wait().await;
        run.state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .terminal = Some(fact);
        self.advance_run(resource.pre.native(), run).await
    }
    pub(super) async fn retry_zero(
        self: &Arc<Self>,
        target: EntityId,
    ) -> Result<(), DispatchError> {
        self.running()?;
        let run = self
            .runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .find(|run| run.probe.plan.canonical_target == target)
            .cloned()
            .ok_or(DispatchError::InvalidTarget)?;
        let (source, resource) = self.installed_for_run(&run.probe.plan.native, &run)?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            core.retry_selected_zero(&slot, &resource, &run).await
        })
        .await
    }
    async fn retry_selected_zero(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        run: &Arc<RunningDispatch<P>>,
    ) -> Result<(), DispatchError> {
        let _action = run.action.lock().await;
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            let fact = state.terminal.ok_or(DispatchError::ResourceUnconfirmed)?;
            if !state.transport_stored
                || !matches!(fact, TransportFact::Zero { .. })
                || run.priority_cancelled.load(Ordering::SeqCst)
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            let ticket = state
                .ticket
                .as_ref()
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            if ticket.progress() != TransportProgress::Finished(fact) {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            state
                .selected
                .as_ref()
                .ok_or(DispatchError::StaleSelection)?
                .with_current(&mut || {
                    let mut facts = resource
                        .target
                        .facts
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    if source.pre.authority.closed.load(Ordering::SeqCst)
                        || source.pre.authority.quarantined.load(Ordering::SeqCst)
                        || source.pre.authority.required.load(Ordering::SeqCst) != 0
                        || facts.foreground != WorkActivity::Idle
                        || !facts.control_holds.is_empty()
                        || !facts.requirements.is_empty()
                        || !facts.failed_requirements.is_empty()
                    {
                        return Err(DispatchError::NotReady);
                    }
                    let input = facts
                        .pending_input
                        .as_ref()
                        .ok_or(DispatchError::InvalidResource)?;
                    if input.attempt
                        != state
                            .attempt
                            .as_ref()
                            .ok_or(DispatchError::InvalidResource)?
                            .metadata
                            .attempt_id
                    {
                        return Err(DispatchError::InvalidResource);
                    }
                    let work = input.work.clone();
                    facts
                        .activity
                        .reject_input(&work)
                        .map_err(|_| DispatchError::InvalidResource)?;
                    facts.pending_input = None;
                    drop(facts);
                    Ok(())
                })?;
            let old_attempt = state.attempt.take().ok_or(DispatchError::InvalidResource)?;
            let old_ticket = state.ticket.take().ok_or(DispatchError::InvalidResource)?;
            state.previous.push((old_attempt, old_ticket, fact));
            state.attempt_identity = None;
            state.terminal = None;
            state.terminal_record = None;
            state.transport_stored = false;
        }
        self.start_attempt(source, resource, run).await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Reconcile one original transport/native/metadata tuple before changing the exact queue disposition."
    )]
    async fn advance_run(
        &self,
        key: &NativeSessionKey,
        run: &Arc<RunningDispatch<P>>,
    ) -> Result<(), DispatchError> {
        let (attempt, terminal, stored) = {
            let state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            (
                state
                    .attempt
                    .clone()
                    .ok_or(DispatchError::RequiredStateUnavailable)?,
                state
                    .terminal
                    .ok_or(DispatchError::RequiredStateUnavailable)?,
                state.transport_stored,
            )
        };
        if !stored {
            let proof = {
                let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                state
                    .terminal_record
                    .get_or_insert_with(|| {
                        Arc::new(TerminalTransportProof {
                            attempt: attempt.clone(),
                            fact: terminal,
                            recorded_at: self.clock.unix_millis(),
                        })
                    })
                    .clone()
            };
            self.repository
                .record_transport(&attempt, &proof, &self.context(), &self.deadline()?)
                .await?;
            run.state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .transport_stored = true;
        }
        let resource = self
            .environment
            .target(key)
            .ok_or(DispatchError::InvalidResource)?;
        {
            let mut facts = resource.facts.lock().map_err(|_| DispatchError::Stopped)?;
            if let Some(input) = &mut facts.pending_input {
                input.transport = Some(terminal);
            }
        }
        resource.changed.notify_waiters();
        if matches!(terminal, TransportFact::Zero { .. }) {
            if !run.priority_cancelled.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.record_dispatch_release(
                run,
                ReleaseKind::KnownZero,
                super::source::metadata_stamp(
                    b"native-dispatch-zero",
                    attempt.metadata.attempt_id,
                    0,
                    0,
                ),
                Vec::new(),
            )
            .await?;
            let selected = run
                .state
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .selected
                .as_ref()
                .ok_or(DispatchError::StaleSelection)?
                .active
                .clone();
            let source = self
                .sources
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .iter()
                .find(|s| s.pre.metadata.instance_id == run.probe.plan.instance_id)
                .cloned()
                .ok_or(DispatchError::InvalidSource)?;
            let slot = source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(key)
                .cloned()
                .ok_or(DispatchError::InvalidResource)?;
            slot.queue.apply(run.probe.plan.resource, &mut |_| {
                let mut facts = resource.facts.lock().map_err(|_| DispatchError::Stopped)?;
                if let Some(input) = facts.pending_input.take() {
                    facts
                        .activity
                        .reject_input(&input.work)
                        .map_err(|_| DispatchError::InvalidResource)?;
                    drop(facts);
                }
                Ok(NativeQueuePolicy::Settle {
                    active: selected.clone(),
                    disposition: QueueDisposition::Advance,
                    reason: KnownStopReason::Unknown,
                })
            })?;
            self.runs
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .remove(key);
            self.environment.wake(key);
            return Ok(());
        }
        let (requirements, reason) = {
            let f = resource.facts.lock().map_err(|_| DispatchError::Stopped)?;
            let pending = f
                .pending_input
                .as_ref()
                .ok_or(DispatchError::InvalidResource)?;
            if pending.attempt != attempt.metadata.attempt_id {
                return Err(DispatchError::InvalidResource);
            }
            let reliable = if attempt.metadata.wire == ProtocolVersion::V1 {
                pending
                    .native_reason
                    .is_some_and(|reason| reason.reliable() && reason != KnownStopReason::Error)
            } else {
                f.foreground == WorkActivity::Idle
                    && matches!(
                        f.foreground_evidence,
                        super::environment::ForegroundEvidence::ReliableTerminal
                    )
                    && f.terminal_sequence > pending.admitted_sequence
                    && f.terminal.is_some_and(KnownStopReason::reliable)
                    && (pending.inserted || run.priority_cancelled.load(Ordering::SeqCst))
            };
            if terminal != TransportFact::Written
                || !reliable
                || !f.requirements.is_empty()
                || !f.failed_requirements.is_empty()
                || !f.control_holds.is_empty()
            {
                return Ok(());
            }
            let requirements = pending.requirements.clone();
            let reason = if attempt.metadata.wire == ProtocolVersion::V1 {
                pending.native_reason
            } else {
                f.terminal
            }
            .ok_or(DispatchError::ResourceUnconfirmed)?;
            (requirements, reason)
        };
        self.record_dispatch_release(
            run,
            ReleaseKind::NativeSettled,
            super::source::metadata_stamp(
                b"native-dispatch-settled",
                run.probe.plan.dispatch_id,
                0,
                0,
            ),
            requirements,
        )
        .await?;
        let selected = run
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .selected
            .as_ref()
            .ok_or(DispatchError::StaleSelection)?
            .active
            .clone();
        let source = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .find(|s| s.pre.metadata.instance_id == run.probe.plan.instance_id)
            .cloned()
            .ok_or(DispatchError::InvalidSource)?;
        let slot = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(key)
            .cloned()
            .ok_or(DispatchError::InvalidResource)?;
        slot.queue.apply(run.probe.plan.resource, &mut |_| {
            let mut f = resource.facts.lock().map_err(|_| DispatchError::Stopped)?;
            if f.foreground != WorkActivity::Idle
                || !f.requirements.is_empty()
                || !f.failed_requirements.is_empty()
                || !f.control_holds.is_empty()
            {
                return Err(DispatchError::NotReady);
            }
            let input = f
                .pending_input
                .as_ref()
                .filter(|pending| pending.attempt == attempt.metadata.attempt_id)
                .ok_or(DispatchError::InvalidResource)?;
            let work = input.work.clone();
            let inserted = input.inserted;
            let rpc = input.rpc.clone();
            if attempt.metadata.wire == ProtocolVersion::V1 || !inserted {
                f.activity
                    .finish_work(
                        &work,
                        if reason == KnownStopReason::EndTurn {
                            ForegroundOutcome::Completed
                        } else {
                            ForegroundOutcome::Cancelled
                        },
                    )
                    .map_err(|_| DispatchError::InvalidResource)?;
            } else {
                f.activity
                    .release_inserted_slot(&work)
                    .map_err(|_| DispatchError::InvalidResource)?;
            }
            if attempt.metadata.wire == ProtocolVersion::V2 && !inserted {
                f.retired_prompts.insert(
                    rpc,
                    RetiredPrompt {
                        attempt: attempt.metadata.attempt_id,
                    },
                );
            }
            f.pending_input.take();
            Ok(NativeQueuePolicy::Settle {
                active: selected.clone(),
                disposition: if reason == KnownStopReason::EndTurn {
                    QueueDisposition::Advance
                } else {
                    QueueDisposition::Suspend
                },
                reason,
            })
        })?;
        run.state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .settled = true;
        self.runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .remove(key);
        self.environment.wake(key);
        Ok(())
    }
    pub(super) async fn reconcile_source_dispatches(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let runs = source
            .dispatches
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone();
        let mut fingerprints = Vec::with_capacity(runs.len());
        for run in runs {
            if run.probe.plan.instance_id != source.pre.metadata.instance_id
                || !run.probe.resource.metadata.source.eq(&source.pre.metadata)
            {
                return Err(DispatchError::InvalidSource);
            }
            let _action = run.action.lock().await;
            fingerprints.push(self.reconcile_dispatch_transport(&run).await?);
        }
        *source
            .dispatch_closures
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(fingerprints);
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The closure inventory must inspect every historical original ticket and matching arm row before fingerprinting."
    )]
    async fn reconcile_dispatch_transport(
        &self,
        run: &Arc<RunningDispatch<P>>,
    ) -> Result<[u8; 32], DispatchError> {
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            if let Some(ticket) = &state.ticket {
                if state.reservation.is_some() || state.frame.is_some() {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                match ticket.progress() {
                    TransportProgress::Finished(fact) => state.terminal = Some(fact),
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            } else if let Some(reservation) = state.reservation.take() {
                let probe = reservation.probe();
                drop(reservation);
                let TransportProgress::Finished(fact @ TransportFact::Zero { .. }) =
                    self.protocol.probe_progress(&probe)
                else {
                    return Err(DispatchError::ResourceUnconfirmed);
                };
                state.retired_probe = Some(probe);
                state.frame.take();
                state.terminal = Some(fact);
            }
        }
        let inspection = self
            .repository
            .inspect(&run.probe, &self.deadline()?)
            .await?;
        let (current, fact, previous, has_ticket) = {
            let state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            (
                state.attempt.clone(),
                state.terminal,
                state.previous.clone(),
                state.ticket.is_some(),
            )
        };
        let mut inspection = if let Some(inspection) = inspection {
            if inspection.dispatch.plan != run.probe.plan {
                return Err(DispatchError::Conflict);
            }
            let current_stored = current.as_ref().is_some_and(|attempt| {
                inspection
                    .attempts
                    .iter()
                    .any(|row| row.metadata == attempt.metadata)
            });
            if has_ticket && !current_stored {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            if current_stored {
                let attempt = current.as_ref().ok_or(DispatchError::ResourceUnconfirmed)?;
                let fact = fact.ok_or(DispatchError::ResourceUnconfirmed)?;
                let proof = {
                    let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                    state
                        .terminal_record
                        .get_or_insert_with(|| {
                            Arc::new(TerminalTransportProof {
                                attempt: attempt.clone(),
                                fact,
                                recorded_at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                if proof.fact != fact || proof.attempt.metadata != attempt.metadata {
                    return Err(DispatchError::Conflict);
                }
                self.repository
                    .record_transport(attempt, &proof, &self.context(), &self.deadline()?)
                    .await?;
                run.state
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .transport_stored = true;
            }
            let inspection = self
                .repository
                .inspect(&run.probe, &self.deadline()?)
                .await?
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            for row in &inspection.attempts {
                let actual = if current
                    .as_ref()
                    .is_some_and(|attempt| attempt.metadata == row.metadata)
                {
                    fact
                } else {
                    previous
                        .iter()
                        .find(|(attempt, ticket, fact)| {
                            attempt.metadata == row.metadata
                                && ticket.progress() == TransportProgress::Finished(*fact)
                        })
                        .map(|(_, _, fact)| *fact)
                }
                .ok_or(DispatchError::ResourceUnconfirmed)?;
                if row.transport != super::StoredTransport::Terminal(actual)
                    || row.recorded_at.is_none()
                {
                    return Err(DispatchError::RequiredStateUnavailable);
                }
            }
            for (attempt, ticket, fact) in &previous {
                if ticket.progress() != TransportProgress::Finished(*fact)
                    || !inspection.attempts.iter().any(|row| {
                        row.metadata == attempt.metadata
                            && row.transport == super::StoredTransport::Terminal(*fact)
                    })
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
            }
            Some(inspection)
        } else {
            if has_ticket || !previous.is_empty() {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            None
        };
        let mut digest = super::shutdown::ClosureDigest::new();
        let plan = &run.probe.plan;
        for id in [
            plan.dispatch_id,
            plan.planned_run_id,
            plan.canonical_target,
            plan.item_id,
            plan.instance_id,
            plan.owner_id,
            plan.config_identity,
        ] {
            digest.id(id);
        }
        digest.u64(plan.revision);
        digest.u64(plan.config_revision);
        digest.raw(&plan.origin.digest);
        digest.id(plan.origin.content_request.entity());
        digest.id(plan.origin.submit_request.entity());
        digest.flag(inspection.is_some());
        if let Some(inspection) = &mut inspection {
            digest.raw(&[match inspection.dispatch.lifecycle {
                super::DispatchLifecycle::PreparedZero => 0,
                super::DispatchLifecycle::Selected => 1,
                super::DispatchLifecycle::Closed => 2,
            }]);
            digest.flag(inspection.dispatch.has_slot);
            digest.flag(inspection.dispatch.close.is_some());
            if let Some(close) = &inspection.dispatch.close {
                digest.text(&close.reason)?;
                digest.raw(&close.fingerprint);
                digest.i64(close.at);
            }
            inspection
                .attempts
                .sort_by_key(|row| row.metadata.attempt_id.encode());
            digest.length(inspection.attempts.len())?;
            for row in &inspection.attempts {
                digest.id(row.metadata.attempt_id);
                digest.id(row.metadata.selection.selection_id);
                digest.u64(row.metadata.selection.owner_order);
                digest.i64(row.metadata.selection.at);
                digest.u64(row.metadata.wire.number().cast_unsigned());
                digest.rpc(Some(&row.metadata.rpc))?;
                digest.u64(row.metadata.frame_length);
                digest.i64(row.metadata.armed_at);
                digest.transport(Some(row.transport));
                digest.optional_i64(row.recorded_at);
            }
        }
        let state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
        digest.flag(state.selected.is_some());
        digest.flag(state.prepared.is_some());
        digest.flag(state.settled);
        digest.flag(state.retiring);
        digest.optional_id(state.attempt_identity);
        digest.transport(state.terminal.map(super::StoredTransport::Terminal));
        drop(state);
        Ok(digest.finish())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep physical and SQL release separate from native completion while retiring the original coordination token."
    )]
    pub(super) async fn retire_converged_run(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        retirement: Option<&super::RetiredResourceProof>,
    ) -> Result<(), DispatchError> {
        let convergence = resource
            .convergence
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        if !resource.pre.authority.closed.load(Ordering::SeqCst)
            || !Arc::ptr_eq(&convergence.authority, &resource.pre.authority)
            || convergence.metadata.resource != resource.pre.metadata
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        // Reconfirm this owner's actual proof, never turn a Released row into a grant.
        self.repository
            .record_convergence(&convergence, &self.context(), &self.deadline()?)
            .await?;
        let key = resource.pre.native();
        let run = self
            .runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(key)
            .filter(|run| run.probe.resource.same(&resource.pre))
            .cloned();
        let Some(run) = run else {
            if let Some(proof) = retirement {
                resource.queue.retire_closed_resource(proof)?;
            }
            return Ok(());
        };
        let _action = run.action.lock().await;
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            if state.settled {
                return Ok(());
            }
            if let Some(ticket) = &state.ticket {
                match ticket.progress() {
                    TransportProgress::Finished(fact) => state.terminal = Some(fact),
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            } else if let Some(reservation) = state.reservation.take() {
                let probe = reservation.probe();
                drop(reservation);
                let TransportProgress::Finished(fact @ TransportFact::Zero { .. }) =
                    self.protocol.probe_progress(&probe)
                else {
                    return Err(DispatchError::ResourceUnconfirmed);
                };
                state.retired_probe = Some(probe);
                state.frame.take();
                state.terminal = Some(fact);
            }
        }
        let inspection = self
            .repository
            .inspect(&run.probe, &self.deadline()?)
            .await?;
        if let Some(inspection) = inspection {
            let (attempt, fact) = {
                let state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                (state.attempt.clone(), state.terminal)
            };
            if let Some(attempt) = attempt
                && inspection
                    .attempts
                    .iter()
                    .any(|row| row.metadata == attempt.metadata)
            {
                let fact = fact.ok_or(DispatchError::ResourceUnconfirmed)?;
                let proof = {
                    let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
                    state
                        .terminal_record
                        .get_or_insert_with(|| {
                            Arc::new(TerminalTransportProof {
                                attempt: attempt.clone(),
                                fact,
                                recorded_at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                self.repository
                    .record_transport(&attempt, &proof, &self.context(), &self.deadline()?)
                    .await?;
                run.state
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .transport_stored = true;
            }
            self.record_dispatch_release(
                &run,
                ReleaseKind::ResourcesConverged,
                convergence.metadata.fingerprint,
                vec![convergence.metadata.requirement],
            )
            .await?;
        }
        let selected = run
            .state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .selected
            .as_ref()
            .map(|selected| selected.active.clone());
        if let Some(selected) = selected {
            resource.queue.apply(run.probe.plan.resource, &mut |_| {
                let mut facts = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                if let Some(input) = facts.pending_input.as_ref() {
                    if input.dispatch != run.probe.plan.dispatch_id {
                        return Err(DispatchError::InvalidResource);
                    }
                    let work = input.work.clone();
                    facts
                        .activity
                        .release_converged_input(&work)
                        .map_err(|_| DispatchError::InvalidResource)?;
                    facts.pending_input.take();
                }
                facts.foreground = WorkActivity::Unknown;
                facts.foreground_evidence = super::environment::ForegroundEvidence::Unknown;
                drop(facts);
                if retirement.is_some() {
                    Ok(NativeQueuePolicy::None)
                } else {
                    Ok(NativeQueuePolicy::Settle {
                        active: selected.clone(),
                        disposition: QueueDisposition::Advance,
                        reason: KnownStopReason::Unknown,
                    })
                }
            })?;
        }
        if let Some(proof) = retirement {
            resource.queue.retire_closed_resource(proof)?;
        }
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            state.settled = true;
            state.prepared.take();
        }
        source
            .retired_runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .push(run.clone());
        let mut runs = self.runs.lock().map_err(|_| DispatchError::Stopped)?;
        if runs
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, &run))
        {
            runs.remove(key);
        }
        drop(runs);
        self.environment.wake(key);
        Ok(())
    }
    async fn record_dispatch_release(
        &self,
        run: &Arc<RunningDispatch<P>>,
        kind: ReleaseKind,
        fingerprint: [u8; 32],
        requirements: Vec<EntityId>,
    ) -> Result<(), DispatchError> {
        let proof = {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            state
                .release
                .get_or_insert_with(|| {
                    Arc::new(ReleaseProof {
                        dispatch: run.probe.clone(),
                        reason: kind,
                        fingerprint,
                        at: self.clock.unix_millis(),
                        requirements,
                    })
                })
                .clone()
        };
        self.repository
            .close(&run.probe, &proof, &self.context(), &self.deadline()?)
            .await?;
        Ok(())
    }
    async fn retire_unselected(&self, run: &Arc<RunningDispatch<P>>) -> Result<(), DispatchError> {
        {
            let mut state = run.state.lock().map_err(|_| DispatchError::Stopped)?;
            if state.selected.is_some() || state.attempt.is_some() || state.ticket.is_some() {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            state.retiring = true;
            if let Some(reservation) = state.reservation.take() {
                let probe = reservation.probe();
                drop(reservation);
                state.retired_probe = Some(probe);
                state.frame.take();
            }
            if let Some(probe) = &state.retired_probe
                && !matches!(
                    self.protocol.probe_progress(probe),
                    TransportProgress::Finished(TransportFact::Zero { .. })
                )
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        self.record_dispatch_release(
            run,
            ReleaseKind::KnownZero,
            super::source::metadata_stamp(
                b"native-unselected-retired",
                run.probe.plan.dispatch_id,
                0,
                0,
            ),
            Vec::new(),
        )
        .await?;
        run.state
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .prepared
            .take();
        self.runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .remove(&run.probe.plan.native);
        self.environment.wake(&run.probe.plan.native);
        Ok(())
    }
}
