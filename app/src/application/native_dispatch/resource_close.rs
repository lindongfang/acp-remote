//! Isolated resource cleanup uses its original scope, load operation and real close response.
use super::{
    Arc, ConfigurationPort, ConvergenceKind, ConvergenceMetadata, ConvergenceProbe,
    CurrentTargetCloseBinding, DispatchError, DispatchFenceRejection, DispatchPollIo,
    DispatchWriteStep, EncodedPromptFrame, EntityId, LifecycleKind, LifecycleWriteBoundary,
    LifecycleWriteIdentity, LoadTransportProof, NativeControlRepository, NativeCoordinator,
    NativeDispatchRepository, NativeResourceRepository, NativeRuntimeHandle, Ordering,
    OwnedNativeSource, PromptTicket, ReservedPrompt, ResourceAuthority, ResourceCloseBasis,
    ResourceCloseStage, ResourceClosureProof, ResourceLifecycle, ResourceStateProof, ResponseWait,
    RpcScalar, SourceAuthority, SourceLaunchPort, TransportFact, TransportProgress,
    runtime::{
        LifecycleOperation, LifecycleResponse, LifecycleResponseValue, ResourceSlot, SourceSlot,
    },
};
use crate::application::queue_owner::NativeQueuePolicy;

impl<P, L, G, N> NativeRuntimeHandle<P, L, G, N>
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
    /// Releases an actually owned resource without killing other targets on its connection.
    /// # Errors
    /// Unconfirmed restore, old writes, unavailable close capability or metadata remain owned.
    pub async fn release_target(&self, target: EntityId) -> Result<(), DispatchError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let core = self.core.clone();
        let failed = self.core.clone();
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.release_target(target).await;
                let _ = tx.send(result);
            },
            move || failed.fail_all(DispatchError::ResourceUnconfirmed),
        )?;
        rx.await.map_err(|_| DispatchError::ResourceUnconfirmed)?
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
    pub(super) async fn wait_resource_response(
        &self,
        source: &SourceSlot<P, L>,
        resource: &ResourceSlot<P>,
        operation: &LifecycleOperation<P>,
    ) -> Result<LifecycleResponse, DispatchError> {
        loop {
            let received = operation.changed.notified();
            tokio::pin!(received);
            received.as_mut().enable();
            let changed = resource.target.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let stopped = source.changed.notified();
            tokio::pin!(stopped);
            stopped.as_mut().enable();
            let response = operation
                .response
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if let Some(response) = response {
                return Ok(response);
            }
            if source.pre.authority.closed.load(Ordering::SeqCst)
                || resource.pre.authority.releasing.load(Ordering::SeqCst)
            {
                return Err(DispatchError::Stopped);
            }
            tokio::select! {()=&mut received=>{},()=&mut changed=>{},()=&mut stopped=>{}}
        }
    }
    async fn release_target(self: &Arc<Self>, target: EntityId) -> Result<(), DispatchError> {
        self.running()?;
        let sources = self
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        let mut found = None;
        for source in sources {
            let resource = source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .find(|resource| resource.pre.metadata.target == target)
                .cloned();
            if let Some(resource) = resource {
                found = Some((source, resource));
                break;
            }
        }
        let (source, resource) = found.ok_or(DispatchError::InvalidResource)?;
        let core = self.clone();
        let slot = source.clone();
        self.run_source_job(source, async move {
            core.release_installed_target(&slot, &resource).await
        })
        .await
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The original resource close operation retains its scope, protocol response and release evidence across all stages."
    )]
    async fn release_installed_target(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        let deadline = self.effective_close_deadline(source)?;
        let _close = tokio::time::timeout_at(deadline, resource.close.lock())
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        *resource
            .close_deadline
            .lock()
            .map_err(|_| DispatchError::Stopped)? = Some(deadline);
        if resource.pre.authority.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let saved = resource
            .release
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(proof) = saved {
            self.repository
                .record_resource_closed(&resource.pre, &proof, &self.context(), &self.deadline()?)
                .await?;
            resource.pre.authority.closed.store(true, Ordering::SeqCst);
            source.view_attachment.seal_target(resource.pre.native());
            self.environment
                .retire_closed_target(resource.pre.native(), &resource.target);
            return Ok(());
        }
        {
            let _facts = resource
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
            resource
                .pre
                .authority
                .releasing
                .store(true, Ordering::SeqCst);
            source.view_attachment.seal_target(resource.pre.native());
        }
        let releasing = {
            let mut proof = resource
                .releasing_record
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            proof
                .get_or_insert_with(|| {
                    Arc::new(ResourceStateProof {
                        resource: resource.pre.clone(),
                        state: ResourceLifecycle::Releasing,
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        // A necessary SQL gap blocks ordinary input; explicit safety cleanup retains its owner.
        let _ = self
            .repository
            .record_resource_state(&releasing, &self.deadline()?)
            .await;
        resource.target.wake_writer();
        resource.target.changed.notify_waiters();
        let operation = resource
            .load
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(operation) = &operation {
            let ticket = operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if let Some(ticket) = ticket {
                ticket.request_cancel();
            }
        }
        let _install = tokio::time::timeout_at(deadline, resource.install.lock())
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if let Some(operation) = operation {
            let ticket = operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            let fact = if let Some(ticket) = ticket {
                tokio::time::timeout_at(deadline, ticket.wait())
                    .await
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?
            } else {
                let original = operation
                    .resume
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .take();
                if let Some((reservation, _frame)) = original {
                    let cancelled = self.protocol.cancel_resume(reservation);
                    if cancelled.rpc != operation.rpc
                        || cancelled.kind != LifecycleKind::Resume
                        || source
                            .memory
                            .lock()
                            .map_err(|_| DispatchError::Stopped)?
                            .scope
                            .as_ref()
                            != Some(&cancelled.scope)
                    {
                        return Err(DispatchError::InvalidScope);
                    }
                    let TransportProgress::Finished(fact @ TransportFact::Zero { .. }) =
                        self.protocol.probe_progress(&cancelled.probe)
                    else {
                        return Err(DispatchError::ResourceUnconfirmed);
                    };
                    *operation
                        .cancelled_resume
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? = Some(cancelled);
                    fact
                } else {
                    let receipt = operation
                        .cancelled_resume
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    match receipt
                        .as_ref()
                        .map(|receipt| self.protocol.probe_progress(&receipt.probe))
                    {
                        Some(TransportProgress::Finished(fact @ TransportFact::Zero { .. })) => {
                            fact
                        }
                        _ => return Err(DispatchError::ResourceUnconfirmed),
                    }
                }
            };
            let row = self
                .repository
                .inspect_resource(&resource.pre, &self.deadline()?)
                .await?
                .ok_or(DispatchError::InvalidResource)?;
            if row.load_rpc.as_ref() == Some(&operation.rpc) {
                let proof = {
                    let mut current = operation
                        .transport_record
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    current
                        .get_or_insert_with(|| {
                            Arc::new(LoadTransportProof {
                                resource: resource.pre.clone(),
                                rpc: operation.rpc.clone(),
                                frame_length: operation.frame_length,
                                fact,
                                at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                self.repository
                    .record_load_transport(&proof, &self.deadline()?)
                    .await?;
            }
            match fact {
                TransportFact::Zero { .. } => {
                    return self.abandon_unloaded_resource(source, resource).await;
                }
                TransportFact::Written => {
                    let response =
                        tokio::time::timeout_at(deadline, self.wait_response(source, &operation))
                            .await
                            .map_err(|_| DispatchError::ResourceUnconfirmed)??;
                    if !matches!(&response.value,LifecycleResponseValue::Resumed(session) if session.as_ref()==resource.pre.native.native_id())
                    {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                }
                TransportFact::Unknown { .. } => return Err(DispatchError::ResourceUnconfirmed),
            }
        } else if resource.loaded.get().is_none() {
            return self.abandon_unloaded_resource(source, resource).await;
        }
        tokio::time::timeout_at(deadline, self.close_resource_controls(source, resource))
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)??;
        let run = self
            .runs
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .get(resource.pre.native())
            .cloned();
        if let Some(run) = &run {
            run.cancel_for_resource_close()?;
        }
        let run_guard = if let Some(run) = &run {
            Some(
                tokio::time::timeout_at(deadline, run.action_for_resource_close())
                    .await
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?,
            )
        } else {
            None
        };
        let connection = source.source.get().ok_or(DispatchError::InvalidScope)?;
        let scope = self.protocol.source_scope(connection);
        let existing = resource
            .close_operation
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
                    native: resource.pre.native.clone(),
                    resource: resource.pre.clone(),
                },
                ResponseWait::At(deadline),
            )?;
            let rpc = reservation.rpc().clone();
            let frame = reservation.encode()?;
            let operation = Arc::new(LifecycleOperation::<P>::new(
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
            *resource
                .close_operation
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
                .close_objects
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .take()
                .ok_or(DispatchError::InvalidPromptReservation)?;
            let boundary = Arc::new(ResourceCloseBoundary {
                scope: scope.clone(),
                rpc: operation.rpc.clone(),
                source: source.pre.authority.clone(),
                resource: resource.pre.authority.clone(),
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
        if !matches!(&response.value,LifecycleResponseValue::Closed(session) if session.as_ref()==resource.pre.native.native_id())
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        self.reconcile_resource_observations(source, resource)
            .await?;
        self.reconcile_resource_controls(source, resource).await?;
        self.repository
            .record_resource_state(&releasing, &self.deadline()?)
            .await?;
        {
            let facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if !facts.requirements.is_empty()
                || !facts.failed_requirements.is_empty()
                || !facts.control_holds.is_empty()
                || !facts.permissions.is_empty()
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        // All ordinary input was sealed before close. The original close reply, not an idle flag, is evidence.
        let epoch = resource
            .target
            .facts
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .activity
            .epoch();
        let mut order = 0;
        resource.queue.apply(epoch, &mut |value| {
            order = value;
            Ok(NativeQueuePolicy::None)
        })?;
        let convergence = {
            let mut value = resource
                .convergence
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if value.is_none() {
                let id = Self::new_id()?;
                *value = Some(Arc::new(ConvergenceProbe {
                    metadata: ConvergenceMetadata {
                        id,
                        requirement: Self::new_id()?,
                        resource: resource.pre.metadata.clone(),
                        owner_order: order,
                        kind: ConvergenceKind::NativeTargetClosedVerified,
                        fingerprint: super::source::metadata_stamp(
                            b"native-target-closed",
                            id,
                            response.sequence,
                            response.receipt,
                        ),
                        at: self.clock.unix_millis(),
                    },
                    authority: resource.pre.authority.clone(),
                }));
            }
            value.clone().ok_or(DispatchError::ResourceUnconfirmed)?
        };
        self.repository
            .record_convergence(&convergence, &self.context(), &self.deadline()?)
            .await?;
        let proof = {
            let current = resource
                .release
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if let Some(proof) = current {
                proof
            } else {
                let expected = self
                    .repository
                    .inspect_resource(&resource.pre, &self.deadline()?)
                    .await?
                    .ok_or(DispatchError::InvalidResource)?;
                let proof = Arc::new(ResourceClosureProof {
                    resource: resource.pre.clone(),
                    expected,
                    basis: ResourceCloseBasis::TargetNativeConverged,
                    stage: ResourceCloseStage::Loaded,
                    fingerprint: convergence.metadata.fingerprint,
                    at: self.clock.unix_millis(),
                });
                *resource
                    .release
                    .lock()
                    .map_err(|_| DispatchError::Stopped)? = Some(proof.clone());
                proof
            }
        };
        self.repository
            .record_resource_closed(&resource.pre, &proof, &self.context(), &self.deadline()?)
            .await?;
        resource.pre.authority.closed.store(true, Ordering::SeqCst);
        source.view_attachment.seal_target(resource.pre.native());
        drop(run_guard);
        self.retire_converged_run(source, resource, None).await?;
        self.environment
            .retire_closed_target(resource.pre.native(), &resource.target);
        Ok(())
    }
    async fn abandon_unloaded_resource(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<ResourceSlot<P>>,
    ) -> Result<(), DispatchError> {
        {
            let facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            if facts.pending_input.is_some()
                || !facts.permissions.is_empty()
                || facts.interval_open
                || !facts.requirements.is_empty()
                || !facts.failed_requirements.is_empty()
                || !facts.control_holds.is_empty()
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        let releasing = resource
            .releasing_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(proof) = releasing {
            self.repository
                .record_resource_state(&proof, &self.deadline()?)
                .await?;
        }
        let expected = self
            .repository
            .inspect_resource(&resource.pre, &self.deadline()?)
            .await?
            .ok_or(DispatchError::InvalidResource)?;
        let proof = {
            let mut current = resource
                .release
                .lock()
                .map_err(|_| DispatchError::Stopped)?;
            current
                .get_or_insert_with(|| {
                    Arc::new(ResourceClosureProof {
                        resource: resource.pre.clone(),
                        stage: if expected.scope_binding.is_some() {
                            ResourceCloseStage::BoundNoTargetIo
                        } else {
                            ResourceCloseStage::ReservedNoTargetIo
                        },
                        expected,
                        basis: ResourceCloseBasis::UnloadedReservationAbandoned,
                        fingerprint: super::source::metadata_stamp(
                            b"native-unloaded-abandoned",
                            resource.pre.metadata.load_epoch,
                            0,
                            0,
                        ),
                        at: self.clock.unix_millis(),
                    })
                })
                .clone()
        };
        self.repository
            .record_resource_closed(&resource.pre, &proof, &self.context(), &self.deadline()?)
            .await?;
        resource.pre.authority.closed.store(true, Ordering::SeqCst);
        source.view_attachment.seal_target(resource.pre.native());
        self.environment
            .retire_closed_target(resource.pre.native(), &resource.target);
        Ok(())
    }
}
struct ResourceCloseBoundary<S> {
    scope: S,
    rpc: RpcScalar,
    source: Arc<SourceAuthority>,
    resource: Arc<ResourceAuthority>,
}
impl<S: Clone + Eq + Send + Sync + 'static> LifecycleWriteBoundary<S> for ResourceCloseBoundary<S> {
    fn check_and_poll(
        &self,
        identity: &LifecycleWriteIdentity<S>,
        _: DispatchWriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn DispatchPollIo,
    ) -> Result<(), DispatchFenceRejection> {
        let _gate = self
            .source
            .gate
            .lock()
            .map_err(|_| DispatchFenceRejection::RequiredStateUnavailable)?;
        if identity.kind != LifecycleKind::Close
            || identity.scope != self.scope
            || identity.rpc != self.rpc
            || self.source.closed.load(Ordering::SeqCst)
            || !self.resource.releasing.load(Ordering::SeqCst)
            || self.resource.closed.load(Ordering::SeqCst)
        {
            return Err(DispatchFenceRejection::Stopped);
        }
        let _ = io.poll(cx);
        Ok(())
    }
}
