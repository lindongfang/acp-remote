//! Close a Create/replay owner through its original writer without a preparatory resume.
use super::{
    Arc, ConfigurationPort, CurrentTargetCloseBinding, DispatchError, EncodedPromptFrame, EntityId,
    LifecycleKind, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeResourceRepository, NativeRuntimeHandle, NativeSessionKey, Ordering, OwnedNativeSource,
    PromptTicket, ReservedPrompt, ResponseWait, SourceLaunchPort, TransportFact,
    capacity::CapacityResource,
    resource_close::ResourceCloseBoundary,
    runtime::{LifecycleOperation, LifecycleResponseValue, SourceSlot},
};
impl<P, L, G, N> NativeRuntimeHandle<P, L, G, N>
where
    P: super::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    pub(crate) async fn release_native(
        &self,
        operation: EntityId,
        target: EntityId,
        native: NativeSessionKey,
    ) -> Result<(), DispatchError> {
        let (send, wait) = tokio::sync::oneshot::channel();
        let core = self.core.clone();
        let failed = core.clone();
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.release_native(operation, target, &native).await;
                let _ = send.send(result);
            },
            move || failed.fail_all(DispatchError::ResourceUnconfirmed),
        )?;
        wait.await.map_err(|_| DispatchError::ResourceUnconfirmed)?
    }
    pub(crate) async fn inspect_capacity(&self) -> Result<super::CapacitySnapshot, DispatchError> {
        self.core
            .repository
            .inspect_capacity(&self.core.deadline()?)
            .await
    }
}
impl<P, L, G, N> NativeCoordinator<P, L, G, N>
where
    P: super::NativeApprovalPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: ConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: NativeResourceRepository
        + NativeDispatchRepository
        + NativeControlRepository
        + super::NativeApprovalRepository,
{
    async fn release_native(
        self: &Arc<Self>,
        operation: EntityId,
        target: EntityId,
        native: &NativeSessionKey,
    ) -> Result<(), DispatchError> {
        self.running()?;
        let owner = match self
            .repository
            .capacity_release_basis(operation, native, &self.deadline()?)
            .await?
        {
            super::CapacityReleaseBasis::Owned(owner) => owner,
            super::CapacityReleaseBasis::SourceClosed => return Ok(()),
        };
        let source = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .find(|source| {
                source.pre.metadata.source_id == owner.source
                    && source.pre.metadata.instance_id == owner.instance
                    && source.pre.metadata.runtime == owner.runtime
            })
            .cloned()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        let capacity = source
            .capacity
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(native)
            .cloned()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        let matches_owner = capacity
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .as_ref()
            .is_some_and(|record| record.same_reservation(&owner));
        if !matches_owner {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let resource = source
            .targets
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(native)
            .filter(|resource| Arc::ptr_eq(&resource.pre.authority, &capacity.pre.authority))
            .cloned();
        if resource
            .as_ref()
            .is_some_and(|resource| resource.pre.metadata.target != target)
        {
            return Err(DispatchError::InvalidTarget);
        }
        let core = self.clone();
        let slot = source.clone();
        let original_authority = source.pre.authority.clone();
        let result = self
            .run_source_job(source, async move {
                if let Some(resource) = resource {
                    let deadline = core.effective_close_deadline(&slot)?;
                    let _admission = loop {
                        let guard = tokio::time::timeout_at(deadline, capacity.close.lock())
                            .await
                            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                        let Some(reason) = super::capacity_automatic::retained_reason(&capacity)?
                        else {
                            break guard;
                        };
                        drop(guard);
                        if core.automatic_release(&slot, &capacity, reason).await? {
                            return Ok(());
                        }
                    };
                    core.release_installed_target(&slot, &resource).await?;
                } else {
                    core.close_capacity_only(&slot, &capacity).await?;
                }
                core.join_capacity_reads(&slot, &capacity).await?;
                core.capacity_closed(&capacity, &owner).await
            })
            .await;
        if result == Err(DispatchError::Stopped)
            && (original_authority.closed.load(Ordering::Acquire)
                || original_authority.closing.load(Ordering::Acquire))
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        result
    }
    pub(super) async fn join_capacity_reads(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
    ) -> Result<(), DispatchError> {
        let members = {
            let activity = source
                .capacity_activity
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            match activity.members(capacity.pre.native.native_id()) {
                Ok(members) => members,
                Err(DispatchError::SessionTreeUnverified) => {
                    vec![Arc::from(capacity.pre.native.native_id())]
                }
                Err(error) => return Err(error),
            }
        };
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if capacity.pre.authority.releasing.load(Ordering::Acquire)
                || capacity.pre.authority.closed.load(Ordering::Acquire)
            {
                for member in &members {
                    let native =
                        NativeSessionKey::from_config(Some(source.pre.metadata.source_id), member)
                            .map_err(|_| DispatchError::InvalidTarget)?;
                    source.view_attachment.seal_target(&native);
                }
            }
        }
        let reads: Vec<_> = capacity
            .reads
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect();
        let deadline = self.effective_close_deadline(source)?;
        for read in reads {
            if !read.join(deadline).await || read.failed() || !read.confirmed_joined() {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        let permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        for permission in permissions
            .values()
            .filter(|p| members.contains(&p.session))
        {
            if !super::capacity_automatic::permission_settled(permission)? {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        drop(permissions);
        if source.pre.authority.required.load(Ordering::Acquire) != 0 {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the retained original close reservation, ticket and same-native response visible in one linear ownership path"
    )]
    async fn close_capacity_only(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        capacity: &Arc<CapacityResource<P>>,
    ) -> Result<(), DispatchError> {
        let deadline = self.effective_close_deadline(source)?;
        let _close = loop {
            let guard = tokio::time::timeout_at(deadline, capacity.close.lock())
                .await
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            let Some(reason) = super::capacity_automatic::retained_reason(capacity)? else {
                break guard;
            };
            drop(guard);
            if self.automatic_release(source, capacity, reason).await? {
                return Ok(());
            }
        };
        let record = capacity
            .record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone()
            .ok_or(DispatchError::InvalidResource)?;
        if capacity.pre.authority.closed.load(Ordering::Acquire) || record.physically_closed {
            return Ok(());
        }
        {
            let _gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            capacity
                .pre
                .authority
                .releasing
                .store(true, Ordering::Release);
            source.view_attachment.seal_target(capacity.pre.native());
        }
        let connection = source
            .source
            .get()
            .ok_or(DispatchError::ProtocolUnavailable)?;
        let scope = self.protocol.source_scope(connection);
        let existing = capacity
            .operation
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let operation = if let Some(operation) = existing {
            operation
        } else {
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
            let operation = Arc::new(LifecycleOperation::new(
                rpc.clone(),
                LifecycleKind::Close,
                frame.byte_len_checked()?,
                self.clock.unix_millis(),
            ));
            *operation
                .close_objects
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some((reservation, frame));
            source
                .operations
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .insert(rpc, operation.clone());
            *capacity
                .operation
                .lock()
                .map_err(|_| DispatchError::Stopped)? = Some(operation.clone());
            operation
        };
        let existing = operation
            .ticket
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let ticket = if let Some(ticket) = existing {
            ticket
        } else {
            let (reservation, frame) = operation
                .close_objects
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .take()
                .ok_or(DispatchError::InvalidPromptReservation)?;
            let boundary = Arc::new(ResourceCloseBoundary {
                scope,
                rpc: operation.rpc.clone(),
                source: source.pre.authority.clone(),
                resource: capacity.pre.authority.clone(),
            });
            let ticket = match self.protocol.enqueue_target_close(
                connection,
                reservation,
                frame,
                boundary,
                deadline,
            ) {
                Ok(ticket) => Arc::new(ticket),
                Err(rejected) => {
                    *operation
                        .close_objects
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
        if tokio::time::timeout_at(deadline, ticket.wait())
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            != TransportFact::Written
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let response = tokio::time::timeout_at(deadline, self.wait_response(source, &operation))
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)??;
        if !matches!(&response.value, LifecycleResponseValue::Closed(session)
            if session.as_ref() == capacity.pre.native.native_id())
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        capacity.pre.authority.closed.store(true, Ordering::Release);
        Ok(())
    }
}
