use super::{
    ChildReadCapability, ReadAcquisitionState, ReadTargetKind, RootReadCapability,
    SourceReadCapability,
};
use crate::application::native_dispatch::{
    Arc, BoundResourceProbe, ConfigurationPort, DispatchError, EncodedPromptFrame, LifecycleKind,
    NativeControlRepository, NativeCoordinator, NativeDispatchRepository, NativeResourceRepository,
    NativeRuntimeOptions, NativeSessionKey, Ordering, OwnedNativeSource, PromptTicket,
    ReservedPrompt, SessionTreePath, SourceInitializationPlan, SourceLaunchPort, TransportFact,
    runtime::{LifecycleOperation, SourceSlot},
    source::SourceBoundary,
};
use std::sync::OnceLock;

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
    pub(in crate::application::native_dispatch) async fn ensure_child_read<C>(
        self: &Arc<Self>,
        subject: NativeSessionKey,
        path: SessionTreePath,
        history: bool,
        plan: Arc<SourceInitializationPlan<L::Plan, P::Setup>>,
        read: Arc<ReadAcquisitionState<P>>,
        check: C,
    ) -> Result<(Arc<SourceSlot<P, L>>, Arc<SourceReadCapability<P>>), DispatchError>
    where
        C: Fn() -> Result<(), DispatchError> + Send + Sync + 'static,
    {
        check()?;
        let root = path
            .root
            .as_ref()
            .ok_or(DispatchError::SessionTreeUnverified)?;
        if path.subject != subject
            || root == &subject
            || path.ancestors.last() != Some(root)
            || root.source() != subject.source()
        {
            return Err(DispatchError::SessionTreeUnverified);
        }
        let source = self.capacity_source(root, plan).await?;
        Self::install_tree_path(&source, &path)?;
        let capacity = self.capacity_resource(&source, root)?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            check()?;
            if source.pre.authority.closed.load(Ordering::Acquire)
                || source.pre.authority.closing.load(Ordering::Acquire)
                || capacity.pre.authority.closed.load(Ordering::Acquire)
                || capacity.pre.authority.releasing.load(Ordering::Acquire)
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            let mut reads = capacity.reads.lock().map_err(|_| DispatchError::Stopped)?;
            reads
                .try_reserve(1)
                .map_err(|_| DispatchError::StorageUnavailable)?;
            let core = self.clone();
            let slot = source.clone();
            let owner = capacity.clone();
            let failed = self.clone();
            let failed_source = source.clone();
            let task = source.jobs.spawn_guarded(
                async move {
                    let result = core
                        .install_child_read(&slot, &owner, subject, history, read, check)
                        .await;
                    let _ = sender.send(result);
                },
                move || failed.fail_source(&failed_source, DispatchError::RequiredStateUnavailable),
            )?;
            reads.push(Arc::downgrade(&task));
        }
        let capability = receiver
            .await
            .map_err(|_| DispatchError::RequiredStateUnavailable)??;
        Ok((source, capability))
    }

    async fn install_child_read<C>(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<crate::application::native_dispatch::capacity::CapacityResource<P>>,
        subject: NativeSessionKey,
        history: bool,
        read: Arc<ReadAcquisitionState<P>>,
        check: C,
    ) -> Result<Arc<SourceReadCapability<P>>, DispatchError>
    where
        C: Fn() -> Result<(), DispatchError> + Send + Sync,
    {
        self.repository
            .reserve_source(&source.pre, &self.deadline()?)
            .await?;
        source.recorded.store(true, Ordering::Release);
        self.reserve_native_capacity(source, capacity).await?;
        let installed = source.install.lock().await;
        check()?;
        self.initialize_source_checked(source, &check).await?;
        let owner = self.child_read_owner(source, capacity)?;
        drop(installed);
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            check()?;
            owner.require_current().map_err(view_dispatch)?;
            source
                .view_attachment
                .bind_target(&subject)
                .map_err(view_dispatch)?;
        }
        if !history || !self.protocol.has_child_history(owner.source()) {
            self.activate_child_owner(source, &owner).await?;
        }
        let capability = Arc::new(SourceReadCapability::ChildAutoDelivered(
            ChildReadCapability {
                view_subject: subject.clone(),
                owner_resource: owner,
            },
        ));
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            check()?;
            capability.require_current().map_err(view_dispatch)?;
            source
                .external_reads
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(subject, capability.clone());
            read.publish(capability.clone())?;
        }
        Ok(capability)
    }

    fn child_read_owner(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<crate::application::native_dispatch::capacity::CapacityResource<P>>,
    ) -> Result<Arc<SourceReadCapability<P>>, DispatchError> {
        let root = &capacity.pre.native;
        let target = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(root)
            .and_then(|resource| resource.read_capability.get())
            .filter(|capability| capability.require_current().is_ok())
            .cloned();
        let external = source
            .external_reads
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(root)
            .filter(|capability| capability.require_current().is_ok())
            .cloned();
        if let Some(owner) = target.or(external) {
            return Ok(owner);
        }
        let (scope, binding, wire) = {
            let memory = source.memory.lock().map_err(|_| DispatchError::Stopped)?;
            (
                memory.scope.clone().ok_or(DispatchError::InvalidScope)?,
                memory.binding.ok_or(DispatchError::InvalidScope)?,
                memory
                    .initialized
                    .as_ref()
                    .ok_or(DispatchError::NotReady)?
                    .metadata
                    .wire,
            )
        };
        let connection = source
            .source
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        let setup = source
            .plan
            .setups
            .iter()
            .find(|(version, _)| *version == wire)
            .map(|(_, setup)| setup.clone())
            .ok_or(DispatchError::InvalidConfiguration)?;
        let owner = Arc::new(SourceReadCapability::RootResource(RootReadCapability {
            kind: ReadTargetKind::SourceOnly,
            source: connection.clone(),
            pre_source: source.pre.clone(),
            pre_resource: Some(capacity.pre.clone()),
            capacity: capacity.clone(),
            capacity_admission: self.read_capacity(source, capacity),
            native: root.clone(),
            scope,
            binding,
            wire,
            setup: Arc::new(self.protocol.bind_setup(
                connection,
                wire,
                &source.plan.configuration,
                setup,
            )?),
            resume: OnceLock::new(),
            activation_lock: tokio::sync::Mutex::new(()),
            changed: source.changed.clone(),
            view_handoff: Arc::downgrade(&source.view_attachment),
        }));
        source
            .external_reads
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .insert(root.clone(), owner.clone());
        Ok(owner)
    }

    async fn activate_child_owner(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        owner: &Arc<SourceReadCapability<P>>,
    ) -> Result<(), DispatchError> {
        let _activation = owner.activation_lock.lock().await;
        owner.require_current().map_err(view_dispatch)?;
        if owner.resume.get().is_none() {
            let bound = BoundResourceProbe {
                pre: owner.capacity.pre.clone(),
                scope: owner.scope.clone(),
                binding: owner.binding,
                wire: owner.wire,
            };
            let reservation = self.protocol.reserve_resume(
                &owner.source,
                &bound,
                &owner.setup,
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
            source
                .operations
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(rpc, operation.clone());
            owner
                .resume
                .set(operation.clone())
                .map_err(|_| DispatchError::Conflict)?;
            let result = self.send_child_owner_resume(owner, &operation).await;
            if let Err(error) = result {
                *operation
                    .read_failure
                    .lock()
                    .map_err(|_| DispatchError::Stopped)? = Some(error);
                operation.changed.notify_waiters();
                return Err(error);
            }
        }
        super::wait_read_binding(owner.clone())
            .await
            .map_err(view_dispatch)?;
        Ok(())
    }

    async fn send_child_owner_resume(
        &self,
        owner: &SourceReadCapability<P>,
        operation: &Arc<LifecycleOperation<P>>,
    ) -> Result<(), DispatchError> {
        let (reservation, frame) = operation
            .resume
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .take()
            .ok_or(DispatchError::InvalidPromptReservation)?;
        let boundary = Arc::new(SourceBoundary {
            scope: owner.scope.clone(),
            source: owner.pre_source.authority.clone(),
            resource: Some(owner.capacity.pre.authority.clone()),
            kind: LifecycleKind::Resume,
            rpc: operation.rpc.clone(),
        });
        let ticket = match self.protocol.enqueue_resume(
            &owner.source,
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
        if ticket.wait().await != TransportFact::Written {
            return Err(DispatchError::ProtocolUnavailable);
        }
        Ok(())
    }
}

const fn view_dispatch(error: crate::domain::shared_view::ViewError) -> DispatchError {
    match error {
        crate::domain::shared_view::ViewError::NativeFailure(error) => error,
        crate::domain::shared_view::ViewError::SourceReleased => DispatchError::ResourceUnconfirmed,
        _ => DispatchError::RequiredStateUnavailable,
    }
}
