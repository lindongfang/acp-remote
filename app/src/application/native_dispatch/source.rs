//! The only Loaded producer: actual launch, actual protocol lifecycle and confirmed metadata.
#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, AtomicBool, BoundResourceProbe, ConfigurationPort, DispatchError, DispatchFenceRejection,
    DispatchPollIo, DispatchWriteStep, EncodedPromptFrame, EntityId, InitializeMetadata,
    IoAdoptionRejected, LaunchAdopter, LaunchCompletion, LifecycleKind, LifecycleWriteBoundary,
    LifecycleWriteIdentity, LoadTransportProof, LoadedResourceProof, NativeControlRepository,
    NativeCoordinator, NativeDispatchRepository, NativeResourceRepository, NativeRuntimeOptions,
    Ordering, OwnedNativeSource, PreResourceProbe, PreSourceProbe, PromptTicket, ProtocolVersion,
    ReservedLoadProof, ReservedPrompt, ResourceAuthority, ResourceLifecycle, ResourceMetadata,
    RpcScalar, SessionBinding, SourceAuthority, SourceBoundRecordProof, SourceLaunchController,
    SourceLaunchPort, SourceLifecycle, SourceMetadata, SourceStateProof, StoredTransport,
    TransportFact, VerifiedLoadedBinding,
};
use super::{LaunchPhase, resource_origin::ResourceTargetOrigin};
use super::{
    environment::{ForegroundEvidence, NativeTarget},
    runtime::{
        InitializedSource, LifecycleOperation, LifecycleResponseValue, ResourceSlot, SourceMemory,
        SourcePhase, SourceSlot,
    },
};
use crate::application::queue_owner::QueueOwnerEvent;
use crate::domain::{
    activity::WorkActivity,
    native::{HarnessConnectionEpoch, LoadEpoch, ResourceEpoch},
};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

pub(super) struct SourceBoundary<S> {
    pub(super) scope: S,
    pub(super) source: Arc<SourceAuthority>,
    pub(super) resource: Option<Arc<ResourceAuthority>>,
    pub(super) kind: LifecycleKind,
    pub(super) rpc: RpcScalar,
}
impl<S: Clone + Eq + Send + Sync + 'static> LifecycleWriteBoundary<S> for SourceBoundary<S> {
    fn check_and_poll(
        &self,
        w: &LifecycleWriteIdentity<S>,
        _: DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        let _gate = self
            .source
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
        if w.scope != self.scope
            || w.rpc != self.rpc
            || w.kind != self.kind
            || self.source.closing.load(Ordering::SeqCst)
            || self.source.closed.load(Ordering::SeqCst)
            || self.resource.as_ref().is_some_and(|r| {
                r.closed.load(Ordering::SeqCst)
                    || (self.kind != LifecycleKind::Close && r.releasing.load(Ordering::SeqCst))
            })
        {
            return Err(DispatchFenceRejection::Stopped);
        }
        let _ = io.poll(cx);
        Ok(())
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
    pub(super) fn ensure_source_slot(
        self: &Arc<Self>,
        plan: Arc<super::SourceInitializationPlan<L::Plan, P::Setup>>,
    ) -> Result<Arc<SourceSlot<P, L>>, DispatchError> {
        let mut sources = self.sources.lock().map_err(|_| DispatchError::Stopped)?;
        self.running()?;
        for source in sources.iter() {
            if source.pre.authority.closed.load(Ordering::Acquire)
                || source.pre.metadata().source_id != plan.source_id
                || source.plan.configuration.identity != plan.configuration.identity
                || source.plan.configuration.revision != plan.configuration.revision
                || !self.launcher.same_plan(&source.plan.launch, &plan.launch)
                || source.plan.setups.len() != plan.setups.len()
            {
                continue;
            }
            let mut equal = true;
            for (wire, setup) in &plan.setups {
                let Some((_, current)) = source.plan.setups.iter().find(|(v, _)| v == wire) else {
                    equal = false;
                    break;
                };
                if !self.protocol.setups_equal(current, setup)? {
                    equal = false;
                    break;
                }
            }
            if equal {
                return Ok(source.clone());
            }
        }
        if sources.iter().any(|source| {
            source.pre.metadata().source_id == plan.source_id
                && source.plan.configuration.identity == plan.configuration.identity
                && source.plan.configuration.revision == plan.configuration.revision
                && self.launcher.same_plan(&source.plan.launch, &plan.launch)
                && source
                    .memory
                    .lock()
                    .map_or(true, |memory| memory.phase == SourcePhase::Unknown)
        }) {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let now = self.clock.unix_millis();
        let authority = Arc::new(SourceAuthority {
            closed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            quarantined: AtomicBool::new(false),
            required: std::sync::atomic::AtomicUsize::new(0),
            gate: Mutex::new(()),
            #[cfg(windows)]
            result_epochs: Mutex::new(Vec::new()),
        });
        let pre = Arc::new(PreSourceProbe {
            metadata: SourceMetadata {
                source_id: plan.source_id,
                instance_id: Self::new_id()?,
                runtime: self.runtime,
                connection_epoch: Self::new_id()?,
                config_identity: plan.configuration.identity,
                config_revision: plan.configuration.revision,
                owner_kind: self.launcher.owner_kind(),
                created_at_ms: now,
            },
            authority,
        });
        let (controller, permit) = SourceLaunchController::create();
        let progress = Arc::new(super::tasks::CloseProgress::child(
            self.close_progress.clone(),
        ));
        let changed = Arc::new(tokio::sync::Notify::new());
        let view_attachment = Arc::new(super::view_attachment::ViewAttachmentHandoff::new(
            changed.clone(),
        ));
        if self.view_factory.get().is_none() {
            view_attachment.disable(None);
        }
        let source = Arc::new(SourceSlot {
            pre,
            plan,
            recorded: AtomicBool::new(false),
            controller,
            launch_outcome: Mutex::new(None),
            permit: Mutex::new(Some(permit)),
            owned: Mutex::new(None),
            io: Mutex::new(None),
            source: OnceLock::new(),
            gate: OnceLock::new(),
            memory: Mutex::new(SourceMemory {
                phase: SourcePhase::Reserved,
                scope: None,
                binding: None,
                initialized: None,
                error: None,
                requirements: HashMap::new(),
                observed_sequence: 0,
            }),
            targets: Mutex::new(HashMap::new()),
            capacity: Mutex::new(HashMap::new()),
            capacity_reservations: Arc::new(Mutex::new(Vec::new())),
            capacity_activity: Mutex::new(super::capacity_activity::CapacityActivity::default()),
            external_reads: Mutex::new(HashMap::new()),
            all_resources: Mutex::new(Vec::new()),
            close_intent: OnceLock::new(),
            close_deadline: Mutex::new(None),
            operations: Mutex::new(HashMap::new()),
            #[cfg(windows)]
            source_replies: Mutex::new(HashMap::new()),
            tasks: super::tasks::TaskRegistry::with_progress(progress.clone()),
            jobs: super::tasks::TaskRegistry::with_progress(progress.clone()),
            approval_recoveries: Mutex::new(Vec::new()),
            metadata_closer: OnceLock::new(),
            metadata_close: super::source_jobs::SourceCloseOperation::new(),
            progress,
            changed,
            view_attachment,
            install: tokio::sync::Mutex::new(()),
            close: tokio::sync::Mutex::new(()),
            raw: Mutex::new(Vec::new()),
            loss: Mutex::new(None),
            pump: OnceLock::new(),
            cleanup: OnceLock::new(),
            observations: Mutex::new(Vec::new()),
            tree_observations: Mutex::new(Vec::new()),
            physical_closed: Mutex::new(None),
            owner_closed: Mutex::new(None),
            owner_witness: Mutex::new(None),
            io_closed: Mutex::new(None),
            closure: Mutex::new(None),
            controls: Mutex::new(Vec::new()),
            control_staging: Mutex::new(Vec::new()),
            retired_runs: Mutex::new(Vec::new()),
            dispatches: Mutex::new(Vec::new()),
            dispatch_closures: Mutex::new(None),
            permissions: Mutex::new(HashMap::new()),
            prepared: Mutex::new(None),
            prepare_failure: Mutex::new(None),
            bound_record: Mutex::new(None),
            spawned_record: Mutex::new(None),
            failure_record: Mutex::new(None),
            initialize: Mutex::new(None),
        });
        if self.view_factory.get().is_some()
            && super::view_attachment::register_closer(
                &self.tasks,
                &source.view_attachment,
                &source.pre,
            )
            .is_err()
        {
            source.view_attachment.disable(Some(
                crate::domain::shared_view::ViewError::CaptureUnavailable,
            ));
        }
        sources
            .try_reserve(1)
            .map_err(|_| DispatchError::StorageUnavailable)?;
        sources.push(source.clone());
        self.start_source_cleanup(&source)?;
        self.start_source_metadata_closer(&source)?;
        Ok(source)
    }
    pub(super) async fn ensure_resource(
        self: &Arc<Self>,
        target: EntityId,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<ResourceSlot<P>>), DispatchError> {
        self.running()?;
        let accepted = self
            .queue
            .accepted(target)
            .await?
            .ok_or(DispatchError::InvalidTarget)?;
        let plan = self.configuration.resolve(&accepted).await?;
        self.ensure_planned_resource(ResourceTargetOrigin::Dispatch(accepted), plan, None)
            .await
    }

    pub(super) async fn ensure_read_resource(
        self: &Arc<Self>,
        proof: crate::application::queue_owner::ReadTargetProof,
        read: Arc<super::read_source::ReadAcquisitionState<P>>,
    ) -> Result<(), DispatchError>
    where
        G: super::ReadConfigurationPort,
    {
        self.running()?;
        self.queue.verify_read_target(&proof)?;
        let plan = self.configuration.resolve_read(&proof).await?;
        self.queue.verify_read_target(&proof)?;
        if plan.target != proof.target()
            || plan.native != *proof.native()
            || plan.source_id != proof.native().source().0
        {
            return Err(DispatchError::InvalidConfiguration);
        }
        let path = self
            .repository
            .resolve_session_tree(proof.native(), &self.deadline()?)
            .await?;
        if !path.ancestors.is_empty() {
            let subject = proof.native().clone();
            self.ensure_child_read(
                subject,
                path,
                false,
                Arc::new(plan.into_source()),
                read,
                move || proof.require_current(),
            )
            .await?;
            return Ok(());
        }
        self.ensure_planned_resource(ResourceTargetOrigin::Read(proof), plan, Some(read))
            .await
            .map(|_| ())
    }

    pub(super) async fn ensure_target_replay_read(
        self: &Arc<Self>,
        proof: crate::application::queue_owner::ReadTargetProof,
        child: Option<super::NativeSessionKey>,
        read: Arc<super::read_source::ReadAcquisitionState<P>>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<super::SourceReadCapability<P>>), DispatchError>
    where
        G: super::ReadConfigurationPort,
    {
        self.running()?;
        self.queue.verify_read_target(&proof)?;
        let plan = self.configuration.resolve_read(&proof).await?;
        self.queue.verify_read_target(&proof)?;
        if plan.target != proof.target()
            || plan.native != *proof.native()
            || plan.source_id != proof.native().source().0
        {
            return Err(DispatchError::InvalidConfiguration);
        }
        let path = self
            .repository
            .resolve_session_tree(
                child.as_ref().unwrap_or_else(|| proof.native()),
                &self.deadline()?,
            )
            .await?;
        if child.is_some() && !path.ancestors.contains(proof.native()) {
            return Err(DispatchError::SessionTreeUnverified);
        }
        if !path.ancestors.is_empty() {
            let subject = child.unwrap_or_else(|| proof.native().clone());
            let queue = self.queue.clone();
            return self
                .ensure_child_read(
                    subject,
                    path,
                    true,
                    Arc::new(plan.into_source()),
                    read,
                    move || queue.verify_read_target(&proof),
                )
                .await;
        }
        let source = self
            .capacity_source(proof.native(), Arc::new(plan.into_source()))
            .await?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            let result = core
                .ensure_installed_replay_read(
                    &slot,
                    proof.native(),
                    || core.queue.verify_read_target(&proof),
                    read,
                )
                .await;
            if let Err(error) = result.as_ref() {
                let bootstrap = slot
                    .memory
                    .lock()
                    .is_ok_and(|memory| memory.scope.is_none() && memory.initialized.is_none());
                if bootstrap
                    && !matches!(
                        error,
                        DispatchError::SessionCapacityFull | DispatchError::SessionTreeUnverified
                    )
                    && !slot.pre.authority.closed.load(Ordering::Acquire)
                {
                    core.fail_source(&slot, *error);
                }
            }
            result
        })
        .await
    }

    pub(super) async fn ensure_source_read(
        self: &Arc<Self>,
        proof: super::SourceReadSelectionProof,
        history: bool,
        read: Arc<super::read_source::ReadAcquisitionState<P>>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<super::SourceReadCapability<P>>), DispatchError>
    where
        G: super::ReadConfigurationPort,
    {
        self.running()?;
        proof.require_current()?;
        let plan = self.configuration.resolve_source_read(&proof).await?;
        if plan.source_id != proof.selection().source
            || plan.workspace != proof.selection().workspace
            || plan.harness != proof.selection().harness
            || plan.configuration.identity != proof.selection().profile.id()
            || plan.configuration.revision != proof.selection().profile.revision()
        {
            return Err(DispatchError::InvalidConfiguration);
        }
        proof.require_current()?;
        let path = match self
            .repository
            .resolve_session_tree(proof.native(), &self.deadline()?)
            .await
        {
            Ok(path) => Some(path),
            Err(DispatchError::SessionTreeUnverified) => None,
            Err(error) => return Err(error),
        };
        if let Some(path) = path.filter(|path| !path.ancestors.is_empty()) {
            let subject = proof.native().clone();
            return self
                .ensure_child_read(subject, path, history, Arc::new(plan), read, move || {
                    proof.require_current()
                })
                .await;
        }
        let source = self.capacity_source(proof.native(), Arc::new(plan)).await?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            let result = core
                .ensure_installed_replay_read(
                    &slot,
                    proof.native(),
                    || proof.require_current(),
                    read,
                )
                .await;
            if let Err(error) = result.as_ref() {
                let bootstrap = slot
                    .memory
                    .lock()
                    .is_ok_and(|memory| memory.scope.is_none() && memory.initialized.is_none());
                if bootstrap
                    && !matches!(
                        error,
                        DispatchError::SessionCapacityFull | DispatchError::SessionTreeUnverified
                    )
                    && !slot.pre.authority.closed.load(Ordering::Acquire)
                {
                    core.fail_source(&slot, *error);
                }
            }
            result
        })
        .await
    }

    async fn ensure_installed_replay_read<C>(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        target_key: &super::NativeSessionKey,
        check: C,
        read: Arc<super::read_source::ReadAcquisitionState<P>>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<super::SourceReadCapability<P>>), DispatchError>
    where
        C: Fn() -> Result<(), DispatchError> + Send + Sync,
    {
        let install_guard = source.install.lock().await;
        if source.pre.authority.closed.load(Ordering::Acquire) {
            return Err(DispatchError::Stopped);
        }
        self.running()?;
        check()?;
        if !source.recorded.load(Ordering::Acquire) {
            let deadline = self.deadline()?;
            self.repository
                .reserve_source(&source.pre, &deadline)
                .await?;
            source.recorded.store(true, Ordering::SeqCst);
        }
        if let Err(error) = self.initialize_source_checked(source, &check).await {
            self.fail_source(source, error);
            if source.controller.current_phase()? == LaunchPhase::Entered {
                let state_proof = {
                    let mut saved = source
                        .failure_record
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    saved
                        .get_or_insert_with(|| {
                            Arc::new(SourceStateProof {
                                source: source.pre.clone(),
                                state: SourceLifecycle::Unknown,
                                initialized: None,
                                at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                let _ = self
                    .repository
                    .record_source_state(&state_proof, &self.deadline()?)
                    .await;
            }
            drop(install_guard);
            let _ = self
                .collect_source_physical(source, self.effective_close_deadline(source)?)
                .await;
            return Err(error);
        }
        let target_capability = {
            let targets = source.targets.lock().map_err(|_| DispatchError::Stopped)?;
            targets
                .get(target_key)
                .filter(|resource| {
                    !resource.pre.authority.closed.load(Ordering::Acquire)
                        && !resource.pre.authority.releasing.load(Ordering::Acquire)
                })
                .and_then(|resource| resource.read_capability.get().cloned())
        };
        let capability = if let Some(existing) = target_capability {
            existing
        } else {
            let existing = source
                .external_reads
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(target_key)
                .filter(|capability| capability.require_current().is_ok())
                .cloned();
            if let Some(existing) = existing {
                existing
            } else {
                let (scope, binding, initialized) = {
                    let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
                    (
                        memory.scope.clone().ok_or(DispatchError::InvalidScope)?,
                        memory.binding.ok_or(DispatchError::InvalidScope)?,
                        memory.initialized.clone().ok_or(DispatchError::NotReady)?,
                    )
                };
                let wire = initialized.metadata.wire;
                let connection = source
                    .source
                    .get()
                    .ok_or(DispatchError::ProtocolUnavailable)?;
                let raw_setup = source
                    .plan
                    .setups
                    .iter()
                    .find(|(version, _)| *version == wire)
                    .map(|(_, setup)| setup.clone())
                    .ok_or(DispatchError::InvalidConfiguration)?;
                let setup = Arc::new(self.protocol.bind_setup(
                    connection,
                    wire,
                    &source.plan.configuration,
                    raw_setup,
                )?);
                let capacity = self.capacity_resource(source, target_key)?;
                let created = Arc::new(super::SourceReadCapability::RootResource(
                    super::read_source::RootReadCapability {
                        kind: super::read_source::ReadTargetKind::SourceOnly,
                        source: connection.clone(),
                        pre_source: source.pre.clone(),
                        pre_resource: Some(capacity.pre.clone()),
                        capacity_admission: self.read_capacity(source, &capacity),
                        capacity,
                        native: target_key.clone(),
                        scope,
                        binding,
                        wire,
                        setup,
                        resume: OnceLock::new(),
                        activation_lock: tokio::sync::Mutex::new(()),
                        changed: source.changed.clone(),
                        view_handoff: Arc::downgrade(&source.view_attachment),
                    },
                ));
                let mut external_reads = source
                    .external_reads
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                external_reads
                    .try_reserve(1)
                    .map_err(|_| DispatchError::StorageUnavailable)?;
                external_reads.insert(target_key.clone(), created.clone());
                created
            }
        };
        drop(install_guard);
        read.publish(capability.clone())?;
        Ok((source.clone(), capability))
    }

    pub(super) async fn initialize_probe_source<C>(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        check: C,
    ) -> Result<P::Source, DispatchError>
    where
        C: Fn() -> Result<(), DispatchError> + Send + Sync,
    {
        let install_guard = source.install.lock().await;
        if source.pre.authority.closed.load(Ordering::Acquire) {
            return Err(DispatchError::Stopped);
        }
        self.running()?;
        check()?;
        if !source.recorded.load(Ordering::Acquire) {
            self.repository
                .reserve_source(&source.pre, &self.deadline()?)
                .await?;
            source.recorded.store(true, Ordering::SeqCst);
        }
        if let Err(error) = self.initialize_source_checked(source, check).await {
            let bootstrap = source
                .memory
                .lock()
                .is_ok_and(|memory| memory.scope.is_none() && memory.initialized.is_none());
            if bootstrap
                && !matches!(
                    error,
                    DispatchError::SessionCapacityFull | DispatchError::SessionTreeUnverified
                )
            {
                self.fail_source(source, error);
                if source.controller.current_phase()? == LaunchPhase::Entered {
                    let state_proof = {
                        let mut saved = source
                            .failure_record
                            .lock()
                            .map_err(|_| DispatchError::Stopped)?;
                        saved
                            .get_or_insert_with(|| {
                                Arc::new(SourceStateProof {
                                    source: source.pre.clone(),
                                    state: SourceLifecycle::Unknown,
                                    initialized: None,
                                    at: self.clock.unix_millis(),
                                })
                            })
                            .clone()
                    };
                    let _ = self
                        .repository
                        .record_source_state(&state_proof, &self.deadline()?)
                        .await;
                }
                drop(install_guard);
                let _ = self
                    .collect_source_physical(source, self.effective_close_deadline(source)?)
                    .await;
            }
            return Err(error);
        }
        drop(install_guard);
        source
            .source
            .get()
            .cloned()
            .ok_or(DispatchError::ProtocolUnavailable)
    }

    async fn ensure_planned_resource(
        self: &Arc<Self>,
        accepted: ResourceTargetOrigin,
        plan: super::ConfiguredSourcePlan<L::Plan, P::Setup>,
        read: Option<Arc<super::read_source::ReadAcquisitionState<P>>>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<ResourceSlot<P>>), DispatchError> {
        if plan.target != accepted.target()
            || plan.native != *accepted.native()
            || plan.source_id != accepted.native().source().0
        {
            return Err(DispatchError::InvalidConfiguration);
        }
        accepted.require_current()?;
        let source = self
            .capacity_source(accepted.native(), Arc::new(plan.into_source()))
            .await?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            let result = core.ensure_installed_resource(&slot, accepted, read).await;
            if let Err(error) = result.as_ref() {
                let bootstrap = slot
                    .memory
                    .lock()
                    .is_ok_and(|memory| memory.scope.is_none() && memory.initialized.is_none());
                if bootstrap
                    && !matches!(
                        error,
                        DispatchError::SessionCapacityFull | DispatchError::SessionTreeUnverified
                    )
                    && !slot.pre.authority.closed.load(Ordering::Acquire)
                {
                    core.fail_source(&slot, *error);
                }
            }
            result
        })
        .await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep durable source/resource reservation and bootstrap failure cleanup under the original install operation."
    )]
    async fn ensure_installed_resource(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        accepted: ResourceTargetOrigin,
        read: Option<Arc<super::read_source::ReadAcquisitionState<P>>>,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<ResourceSlot<P>>), DispatchError> {
        let target = accepted.target();
        let install_guard = source.install.lock().await;
        if source.pre.authority.closed.load(Ordering::Acquire) {
            return Err(DispatchError::Stopped);
        }
        self.running()?;
        accepted.require_current()?;
        let deadline = self.deadline()?;
        self.repository
            .reserve_source(&source.pre, &deadline)
            .await?;
        source.recorded.store(true, Ordering::SeqCst);
        let capacity = self.capacity_resource(source, accepted.native())?;
        // Admission may close another tree on this same source. Retain the original
        // reservation owner, but release the install lock across that coordination.
        drop(install_guard);
        self.reserve_native_capacity(source, &capacity).await?;
        let install_guard = source.install.lock().await;
        self.running()?;
        accepted.require_current()?;
        if source.pre.authority.closed.load(Ordering::Acquire)
            || source.pre.authority.closing.load(Ordering::Acquire)
            || capacity.pre.authority.closed.load(Ordering::Acquire)
            || capacity.pre.authority.releasing.load(Ordering::Acquire)
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let existing_resource = {
            source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .get(accepted.native())
                .filter(|resource| !resource.pre.authority.closed.load(Ordering::SeqCst))
                .cloned()
        };
        let resource = if let Some(existing) = existing_resource {
            existing
        } else {
            let pre = Arc::new(PreResourceProbe {
                metadata: ResourceMetadata {
                    source: source.pre.metadata().clone(),
                    target,
                    owner: accepted.owner(),
                    load_epoch: Self::new_id()?,
                    reserved_at_ms: self.clock.unix_millis(),
                },
                native: accepted.native().clone(),
                authority: capacity.pre.authority.clone(),
            });
            let resource = Arc::new(ResourceSlot {
                target: Arc::new(NativeTarget::new(pre.clone())),
                pre,
                queue: accepted.link(),
                loaded: OnceLock::new(),
                read_capability: OnceLock::new(),
                load: Mutex::new(None),
                setup: OnceLock::new(),
                recorded: AtomicBool::new(false),
                install: tokio::sync::Mutex::new(()),
                close: tokio::sync::Mutex::new(()),
                close_intent: OnceLock::new(),
                close_deadline: Mutex::new(None),
                close_operation: Mutex::new(None),
                convergence: Mutex::new(None),
                release: Mutex::new(None),
                releasing_record: Mutex::new(None),
            });
            source
                .all_resources
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .push(resource.clone());
            source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(accepted.native().clone(), resource.clone());
            resource
        };
        self.repository
            .reserve_resource(&resource.pre, &self.deadline()?)
            .await?;
        resource.recorded.store(true, Ordering::SeqCst);
        self.environment
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .insert(accepted.native().clone(), resource.target.clone());
        if resource.pre.authority.releasing.load(Ordering::SeqCst) {
            return Err(DispatchError::Stopped);
        }
        if resource.loaded.get().is_some() {
            if let Some(read) = &read {
                read.publish(
                    resource
                        .read_capability
                        .get()
                        .cloned()
                        .ok_or(DispatchError::RequiredStateUnavailable)?,
                )?;
            }
            return Ok((source.clone(), resource));
        }
        if let Err(error) = self.initialize_source(source, &accepted).await {
            self.fail_source(source, error);
            if source.controller.current_phase()? == LaunchPhase::Entered {
                let proof = {
                    let mut saved = source
                        .failure_record
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    saved
                        .get_or_insert_with(|| {
                            Arc::new(SourceStateProof {
                                source: source.pre.clone(),
                                state: SourceLifecycle::Unknown,
                                initialized: None,
                                at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                let _ = self
                    .repository
                    .record_source_state(&proof, &self.deadline()?)
                    .await;
            }
            drop(install_guard);
            let _ = self
                .collect_source_physical(source, self.effective_close_deadline(source)?)
                .await;
            return Err(error);
        }
        drop(install_guard);
        let load_guard = resource.install.lock().await;
        if resource.pre.authority.releasing.load(Ordering::SeqCst) {
            return Err(DispatchError::Stopped);
        }
        if resource.loaded.get().is_none() {
            let loaded = self
                .load_resource(source, &resource, &accepted, read.as_deref())
                .await;
            if let Err(error) = loaded {
                if let Some(operation) = resource
                    .load
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .as_ref()
                {
                    *operation
                        .read_failure
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? = Some(error);
                    operation.changed.notify_waiters();
                }
                return Err(error);
            }
        }
        drop(load_guard);
        if let Some(read) = read {
            read.publish(
                resource
                    .read_capability
                    .get()
                    .cloned()
                    .ok_or(DispatchError::RequiredStateUnavailable)?,
            )?;
        }
        Ok((source.clone(), resource))
    }
    async fn initialize_source(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        accepted: &ResourceTargetOrigin,
    ) -> Result<(), DispatchError> {
        self.initialize_source_checked(source, || accepted.require_current())
            .await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The same initialize reservation, frame, ticket, response and SQL proof are retained through the complete handshake."
    )]
    pub(super) async fn initialize_source_checked<C>(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        check: C,
    ) -> Result<(), DispatchError>
    where
        C: Fn() -> Result<(), DispatchError> + Send + Sync,
    {
        if source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .initialized
            .is_some()
        {
            return Ok(());
        }
        check()?;
        self.running()?;
        self.install_protocol_source(source).await?;
        let connection = source
            .source
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        let existing = source
            .initialize
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let operation = if let Some(operation) = existing {
            operation
        } else {
            let reservation = self.protocol.reserve_initialize(
                connection,
                NativeRuntimeOptions::at(self.options.initialize_response_wait)?,
            )?;
            let rpc = reservation.rpc().clone();
            let frame = reservation.encode()?;
            let operation = Arc::new(LifecycleOperation::<P>::new(
                rpc.clone(),
                LifecycleKind::Initialize,
                frame.byte_len_checked()?,
                self.clock.unix_millis(),
            ));
            *operation
                .initialize
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some((reservation, frame));
            source
                .operations
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(rpc, operation.clone());
            *source
                .initialize
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(operation.clone());
            operation
        };
        let ticket = operation
            .ticket
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let ticket = if let Some(ticket) = ticket {
            ticket
        } else {
            let (reservation, frame) = operation
                .initialize
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .take()
                .ok_or(DispatchError::InvalidPromptReservation)?;
            let scope = self.protocol.source_scope(connection);
            let boundary = Arc::new(SourceBoundary {
                scope,
                source: source.pre.authority.clone(),
                resource: None,
                kind: LifecycleKind::Initialize,
                rpc: operation.rpc.clone(),
            });
            let ticket = match self.protocol.enqueue_initialize(
                connection,
                reservation,
                frame,
                boundary,
                NativeRuntimeOptions::at(self.options.initialize_write_wait)?,
            ) {
                Ok(ticket) => Arc::new(ticket),
                Err(rejected) => {
                    *operation
                        .initialize
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? =
                        Some((rejected.reservation, rejected.frame));
                    return Err(rejected.error);
                }
            };
            *operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(ticket.clone());
            ticket
        };
        if ticket.wait().await != TransportFact::Written {
            return Err(DispatchError::ProtocolUnavailable);
        }
        let response = self.wait_response(source, &operation).await?;
        let LifecycleResponseValue::Initialized(wire) = response.value else {
            return Err(DispatchError::ProtocolRejected);
        };
        let metadata = InitializeMetadata {
            rpc: operation.rpc.clone(),
            wire,
            sequence: response.sequence,
            receipt: response.receipt,
            fingerprint: metadata_stamp(
                b"native-initialize",
                source.pre.metadata.instance_id,
                response.sequence,
                response.receipt,
            ),
        };
        let proof = {
            let mut value = operation
                .initialize_record
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            value
                .get_or_insert_with(|| {
                    Arc::new(SourceStateProof {
                        source: source.pre.clone(),
                        state: SourceLifecycle::Initialized,
                        initialized: Some(metadata.clone()),
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        let record = self
            .repository
            .record_source_state(&proof, &self.deadline()?)
            .await?;
        let _gate = source
            .pre
            .authority
            .gate
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || source.pre.authority.closing.load(Ordering::SeqCst)
        {
            return Err(DispatchError::Stopped);
        }
        if record.metadata != source.pre.metadata
            || record.lifecycle != SourceLifecycle::Initialized
            || record.initialize_rpc.as_ref() != Some(&metadata.rpc)
            || record.wire != Some(metadata.wire)
            || record.initialize_sequence != Some(metadata.sequence)
            || record.initialize_receipt != Some(metadata.receipt)
            || record.evidence != Some(metadata.fingerprint)
        {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let mut memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
        memory.initialized = Some(InitializedSource { metadata, record });
        memory.phase = SourcePhase::Initialized;
        drop(memory);
        Ok(())
    }

    async fn install_protocol_source(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        if source.source.get().is_some() {
            return Ok(());
        }
        self.ensure_launched_owner(source).await?;
        if source.pre.authority.closed.load(Ordering::SeqCst) {
            return Err(DispatchError::Stopped);
        }
        self.record_spawned_owner(source).await?;
        self.prepare_original_source_io(source).await?;
        self.start_bound_source_io(source).await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The original process owner is adopted before the driver runs and retained through every launch result."
    )]
    async fn ensure_launched_owner(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        if matches!(
            source
                .launch_outcome
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .as_ref(),
            Some(super::source_ports::LaunchOutcome::Failed)
        ) {
            return Err(DispatchError::Stopped);
        }
        let existing_owner = source
            .owned
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if existing_owner.is_none() {
            let permit = source
                .permit
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .take()
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            let slot = source.clone();
            let adopter = LaunchAdopter {
                adopt: Box::new(move |owner: L::Owned| {
                    let kind = owner.owner_kind();
                    if slot.pre.authority.closed.load(Ordering::SeqCst) {
                        owner.request_stop();
                    }
                    let owned = Arc::new(tokio::sync::Mutex::new(owner));
                    match slot.owned.lock() {
                        Ok(mut value) => *value = Some(owned),
                        Err(error) => {
                            *error.into_inner() = Some(owned);
                            slot.pre.authority.closed.store(true, Ordering::SeqCst);
                            slot.view_attachment.seal();
                        }
                    }
                    match slot.launch_outcome.lock() {
                        Ok(mut outcome) => {
                            *outcome = Some(super::source_ports::LaunchOutcome::Adopted);
                        }
                        Err(error) => {
                            *error.into_inner() = Some(super::source_ports::LaunchOutcome::Adopted);
                            slot.pre.authority.closed.store(true, Ordering::SeqCst);
                            slot.view_attachment.seal();
                        }
                    }
                    if let Ok(mut memory) = slot.memory.lock() {
                        memory.phase = SourcePhase::Owned;
                        if kind != slot.pre.metadata.owner_kind {
                            memory.error = Some(DispatchError::InvalidSource);
                            slot.pre.authority.closed.store(true, Ordering::SeqCst);
                            slot.view_attachment.seal();
                        }
                    }
                    slot.changed.notify_waiters();
                }),
            };
            match self
                .launcher
                .launch(
                    source.plan.launch.clone(),
                    permit,
                    adopter,
                    NativeRuntimeOptions::at(self.options.spawn_wait)?,
                )
                .await
            {
                LaunchCompletion::OwnedRunning(proof) => {
                    let owner = source
                        .owned
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?
                        .clone()
                        .ok_or(DispatchError::ResourceUnconfirmed)?;
                    self.launcher.verify_running(
                        &*owner.lock().await,
                        &proof,
                        &source.controller,
                    )?;
                    *source
                        .launch_outcome
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)? =
                        Some(super::source_ports::LaunchOutcome::Running(Arc::new(proof)));
                }
                LaunchCompletion::OwnedFailed(error) => {
                    *source
                        .launch_outcome
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)? =
                        Some(super::source_ports::LaunchOutcome::Failed);
                    return Err(error);
                }
                LaunchCompletion::BeforeEntryFailed { error, permit } => {
                    *source
                        .permit
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(permit);
                    *source
                        .launch_outcome
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)? =
                        Some(super::source_ports::LaunchOutcome::Failed);
                    return Err(error);
                }
                LaunchCompletion::Revoked => return Err(DispatchError::Stopped),
                LaunchCompletion::EnteredUnconfirmed(error) => {
                    *source
                        .launch_outcome
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)? =
                        Some(super::source_ports::LaunchOutcome::Failed);
                    source.pre.authority.closed.store(true, Ordering::SeqCst);
                    source.view_attachment.seal();
                    let mut memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
                    memory.phase = SourcePhase::Unknown;
                    memory.error = Some(error);
                    drop(memory);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    async fn record_spawned_owner(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let running = match source
            .launch_outcome
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .as_ref()
        {
            Some(super::source_ports::LaunchOutcome::Running(proof)) => proof.clone(),
            _ => return Err(DispatchError::ResourceUnconfirmed),
        };
        let owner = source
            .owned
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        self.launcher
            .verify_running(&*owner.lock().await, &running, &source.controller)?;
        let spawned = {
            let mut value = source
                .spawned_record
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            value
                .get_or_insert_with(|| {
                    Arc::new(SourceStateProof {
                        source: source.pre.clone(),
                        state: SourceLifecycle::Spawned,
                        initialized: None,
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        self.repository
            .record_source_state(&spawned, &self.deadline()?)
            .await?;
        Ok(())
    }
    async fn prepare_original_source_io(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let prepared_exists = source
            .prepared
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .is_some();
        if !prepared_exists {
            let owned = source
                .owned
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone()
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            let streams = owned.lock().await.take_streams()?;
            let slot = source.clone();
            let prepared = self
                .protocol
                .prepare_source(streams, move |owner| match slot.io.lock() {
                    Ok(mut io) => {
                        if io.is_some() {
                            return Err(IoAdoptionRejected {
                                error: DispatchError::Conflict,
                                owner,
                            });
                        }
                        *io = Some(Arc::new(owner));
                        Ok(())
                    }
                    Err(_) => Err(IoAdoptionRejected {
                        error: DispatchError::Stopped,
                        owner,
                    }),
                })
                .map_err(|failure| {
                    match source.prepare_failure.lock() {
                        Ok(mut stage) => *stage = Some(failure.phase),
                        Err(error) => *error.into_inner() = Some(failure.phase),
                    }
                    failure.error
                })?;
            let scope = self.protocol.prepared_scope(&prepared.start);
            let binding = {
                let mut memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
                if memory.scope.as_ref().is_some_and(|old| old != &scope) {
                    return Err(DispatchError::InvalidScope);
                }
                let binding = match memory.binding {
                    Some(value) => value,
                    None => Self::new_id()?,
                };
                memory.scope = Some(scope);
                memory.binding = Some(binding);
                memory.phase = SourcePhase::Prepared;
                binding
            };
            *source.prepared.lock().map_err(|_| DispatchError::Stopped)? = Some(prepared);
            *source
                .bound_record
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(Arc::new(SourceBoundRecordProof {
                source: source.pre.clone(),
                binding,
                at: self.clock.unix_millis(),
            }));
        }
        Ok(())
    }
    async fn start_bound_source_io(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let bound = source
            .bound_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone()
            .ok_or(DispatchError::InvalidScope)?;
        self.repository
            .record_source_bound(&bound, &self.deadline()?)
            .await?;
        source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .phase = SourcePhase::Bound;
        self.running()?;
        if source.pre.authority.closed.load(Ordering::SeqCst) {
            return Err(DispatchError::Stopped);
        }
        let prepared = source
            .prepared
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .take()
            .ok_or(DispatchError::InvalidScope)?;
        source
            .gate
            .set(prepared.gate)
            .map_err(|_| DispatchError::Conflict)?;
        let connection = self.protocol.start_source(prepared.start);
        source
            .source
            .set(connection.clone())
            .map_err(|_| DispatchError::Conflict)?;
        source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .phase = SourcePhase::Started;
        if let Some(factory) = self.view_factory.get()
            && let Ok(guard) = source.view_attachment.begin(source.pre.clone())
        {
            // Capture failures affect this view only; the original receiver and ack continue.
            if let Err(error) = factory.attach(self.view_context(source, &connection), guard) {
                source.view_attachment.note_failure(error);
            }
        }
        let core = self.clone();
        let slot = source.clone();
        let failed = self.clone();
        let failed_slot = source.clone();
        let pump = source.tasks.spawn_guarded(
            async move {
                core.pump_source(slot, connection, prepared.receiver, prepared.acknowledger)
                    .await;
            },
            move || {
                #[cfg(windows)]
                failed.end_source_replies(&failed_slot);
                failed.fail_source(&failed_slot, DispatchError::ObservationDropped);
            },
        )?;
        source.pump.set(pump).map_err(|_| DispatchError::Conflict)?;
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the original load reservation/frame/ticket, native response and reliable Loaded transition in one owner operation."
    )]
    async fn load_resource(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
        accepted: &ResourceTargetOrigin,
        read: Option<&super::read_source::ReadAcquisitionState<P>>,
    ) -> Result<(), DispatchError> {
        accepted.require_current()?;
        self.running()?;
        let (scope, binding, initialized) = {
            let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
            (
                memory.scope.clone().ok_or(DispatchError::InvalidScope)?,
                memory.binding.ok_or(DispatchError::InvalidScope)?,
                memory.initialized.clone().ok_or(DispatchError::NotReady)?,
            )
        };
        let wire = initialized.metadata.wire;
        let connection = source
            .source
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        if resource.setup.get().is_none() {
            let setup = source
                .plan
                .setups
                .iter()
                .find(|(version, _)| *version == wire)
                .map(|(_, setup)| setup.clone())
                .ok_or(DispatchError::InvalidConfiguration)?;
            resource
                .setup
                .set(Arc::new(self.protocol.bind_setup(
                    connection,
                    wire,
                    &source.plan.configuration,
                    setup,
                )?))
                .map_err(|_| DispatchError::Conflict)?;
        }
        let setup = resource
            .setup
            .get()
            .ok_or(DispatchError::InvalidConfiguration)?
            .clone();
        let existing = resource
            .load
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let operation = if let Some(operation) = existing {
            operation
        } else {
            let bound = BoundResourceProbe {
                pre: resource.pre.clone(),
                scope: scope.clone(),
                binding,
                wire,
            };
            let reservation = self.protocol.reserve_resume(
                connection,
                &bound,
                &setup,
                NativeRuntimeOptions::response(self.options.load_response)?,
            )?;
            let rpc = reservation.rpc().clone();
            let frame = reservation.encode()?;
            let operation = Arc::new(LifecycleOperation::<P>::new(
                rpc.clone(),
                LifecycleKind::Resume,
                frame.byte_len_checked()?,
                self.clock.unix_millis(),
            ));
            *operation
                .resume
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some((reservation, frame));
            *operation
                .load_record
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(Arc::new(ReservedLoadProof {
                resource: resource.pre.clone(),
                binding,
                wire,
                rpc: rpc.clone(),
                frame_length: operation.frame_length,
                at: operation.at,
            }));
            source
                .operations
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(rpc, operation.clone());
            *resource.load.lock().map_err(|_| DispatchError::Stopped)? = Some(operation.clone());
            operation
        };
        let capacity = self.capacity_resource(source, resource.pre.native())?;
        let capability = resource.read_capability.get_or_init(|| {
            Arc::new(super::SourceReadCapability::RootResource(
                super::read_source::RootReadCapability {
                    kind: super::read_source::ReadTargetKind::Target,
                    source: connection.clone(),
                    pre_source: source.pre.clone(),
                    pre_resource: Some(resource.pre.clone()),
                    capacity_admission: self.read_capacity(source, &capacity),
                    capacity: capacity.clone(),
                    native: resource.pre.native.clone(),
                    scope: scope.clone(),
                    binding,
                    wire,
                    setup: setup.clone(),
                    resume: OnceLock::from(operation.clone()),
                    activation_lock: tokio::sync::Mutex::new(()),
                    changed: source.changed.clone(),
                    view_handoff: Arc::downgrade(&source.view_attachment),
                },
            ))
        });
        if let Some(read) = read {
            read.publish(capability.clone())?;
        }
        let load = operation
            .load_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone()
            .ok_or(DispatchError::InvalidResource)?;
        self.repository.begin_load(&load, &self.deadline()?).await?;
        self.running()?;
        if source.pre.authority.closed.load(Ordering::SeqCst)
            || resource.pre.authority.closed.load(Ordering::SeqCst)
        {
            return Err(DispatchError::Stopped);
        }
        let ticket = operation
            .ticket
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let ticket = if let Some(ticket) = ticket {
            ticket
        } else {
            let (reservation, frame) = operation
                .resume
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .take()
                .ok_or(DispatchError::InvalidPromptReservation)?;
            let boundary = Arc::new(SourceBoundary {
                scope: scope.clone(),
                source: source.pre.authority.clone(),
                resource: Some(resource.pre.authority.clone()),
                kind: LifecycleKind::Resume,
                rpc: operation.rpc.clone(),
            });
            let ticket = match self.protocol.enqueue_resume(
                connection,
                reservation,
                frame,
                boundary,
                NativeRuntimeOptions::at(self.options.load_write_wait)?,
            ) {
                Ok(ticket) => Arc::new(ticket),
                Err(rejected) => {
                    *operation
                        .resume
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? =
                        Some((rejected.reservation, rejected.frame));
                    return Err(rejected.error);
                }
            };
            *operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(ticket.clone());
            ticket
        };
        let transport = ticket.wait().await;
        let transport_proof = {
            let mut value = operation
                .transport_record
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            value
                .get_or_insert_with(|| {
                    Arc::new(LoadTransportProof {
                        resource: resource.pre.clone(),
                        rpc: operation.rpc.clone(),
                        frame_length: operation.frame_length,
                        fact: transport,
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        self.repository
            .record_load_transport(&transport_proof, &self.deadline()?)
            .await?;
        if transport != TransportFact::Written {
            return Err(DispatchError::ProtocolUnavailable);
        }
        let response = self
            .wait_resource_response(source, resource, &operation)
            .await?;
        if !matches!(&response.value,LifecycleResponseValue::Resumed(id) if id.as_ref()==resource.pre.native.native_id())
        {
            if let Some(observer) = self.transcript_observer.get() {
                observer.on_resume_rejected(resource.pre.native());
            }
            return Err(DispatchError::ProtocolRejected);
        }
        let loaded_proof = {
            let mut value = operation
                .loaded_record
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            value
                .get_or_insert_with(|| {
                    Arc::new(LoadedResourceProof {
                        resource: resource.pre.clone(),
                        rpc: operation.rpc.clone(),
                        sequence: response.sequence,
                        receipt: response.receipt,
                        fingerprint: metadata_stamp(
                            b"native-resumed",
                            resource.pre.metadata.load_epoch,
                            response.sequence,
                            response.receipt,
                        ),
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        let loaded = self
            .repository
            .record_loaded(&loaded_proof, &self.deadline()?)
            .await?;
        if loaded.metadata != resource.pre.metadata
            || loaded.lifecycle != ResourceLifecycle::Loaded
            || loaded.transport != Some(StoredTransport::Terminal(TransportFact::Written))
            || loaded.scope_binding != Some(binding)
            || loaded.wire != Some(wire)
            || loaded.load_rpc.as_ref() != Some(&operation.rpc)
            || loaded.loaded_sequence != Some(response.sequence)
            || loaded.loaded_receipt != Some(response.receipt)
            || loaded.evidence != Some(loaded_proof.fingerprint)
            || initialized.record.lifecycle != SourceLifecycle::Initialized
        {
            return Err(DispatchError::NotReady);
        }
        {
            let mut facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            self.running()?;
            if source.pre.authority.closed.load(Ordering::SeqCst)
                || source.pre.authority.closing.load(Ordering::SeqCst)
                || resource.pre.authority.closed.load(Ordering::SeqCst)
                || resource.pre.authority.releasing.load(Ordering::SeqCst)
            {
                return Err(DispatchError::Stopped);
            }
            let metadata = resource.pre.metadata();
            let epoch = ResourceEpoch {
                runtime: metadata.source.runtime,
                connection: HarnessConnectionEpoch(metadata.source.connection_epoch),
                load: LoadEpoch(metadata.load_epoch),
            };
            let binding = Arc::new(SessionBinding {
                scope,
                native: resource.pre.native.clone(),
                target: metadata.target,
                resource: epoch,
                instance: metadata.source.instance_id,
                owner: metadata.owner,
                config_identity: metadata.source.config_identity,
                config_revision: metadata.source.config_revision,
                authority: resource.pre.authority.clone(),
            });
            let verified = Arc::new(VerifiedLoadedBinding {
                binding: binding.clone(),
                setup,
            });
            self.protocol.attach_prompt_binding(
                connection,
                &verified,
                source.gate.get().ok_or(DispatchError::InvalidScope)?,
            )?;
            resource
                .loaded
                .set(verified)
                .map_err(|_| DispatchError::Conflict)?;
            facts.binding = Some(binding);
            if wire == ProtocolVersion::V1
                && facts.foreground == WorkActivity::Unknown
                && matches!(facts.foreground_evidence, ForegroundEvidence::Unknown)
                && facts.permissions.is_empty()
            {
                facts.foreground = WorkActivity::Idle;
                facts.foreground_evidence = ForegroundEvidence::OwnedV1Idle;
            }
            let foreground = facts.foreground;
            facts
                .activity
                .observe_foreground(epoch, foreground)
                .map_err(|_| DispatchError::InvalidResource)?;
            facts.events.push_back(QueueOwnerEvent::ActivityChanged);
        }
        if let Some(observer) = self.transcript_observer.get()
            && let Some(frames) = observer
                .cached_frames_for_hybrid_resume(resource.pre.native())
                .await
        {
            let _ = source
                .view_attachment
                .preload_cached_target(resource.pre.native(), frames);
            observer.on_session_resumed_without_replay(resource.pre.native());
        }
        self.environment.wake(resource.pre.native());
        Ok(())
    }
}

pub(super) fn metadata_stamp(
    purpose: &[u8],
    id: EntityId,
    sequence: u64,
    receipt: u64,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(purpose);
    h.update(id.encode().as_bytes());
    h.update(sequence.to_be_bytes());
    h.update(receipt.to_be_bytes());
    h.finalize().into()
}
