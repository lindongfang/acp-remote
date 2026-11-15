// Explicit recovery confirms original jobs and SQL cleanup before reading necessary facts.
struct OriginalSourceSqlCloser<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    manager: Weak<ManagementCore<P, L, G, N>>,
}
impl<P, L, G, N> SourceSqlClosurePort for OriginalSourceSqlCloser<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    fn reconcile_source(
        &self,
        source: EntityId,
    ) -> BoxFuture<'_, Result<(), DispatchError>> {
        Box::pin(async move {
            let core = self
                .manager
                .upgrade()
                .ok_or(DispatchError::ResourceUnconfirmed)?;
            let manager = SourceSessionManager { core };
            let groups = manager
                .core
                .native
                .core
                .source_sql
                .source_groups(source)
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            for group in &groups {
                group
                    .seal()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            }
            for group in groups {
                if group
                    .metadata_closed()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?
                {
                    continue;
                }
                if let Some(id) = group.id {
                    let stored = manager
                        .reconcile_original(id, true)
                        .await
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                    let operation = manager
                        .core
                        .operations
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?
                        .get(&id)
                        .cloned();
                    if let Some(operation) = operation
                        && let Some(actual) =
                            operation.reply.get().and_then(|reply| reply.response())
                            && stored.fact.response.as_ref() != Some(&actual) {
                                return Err(DispatchError::ResourceUnconfirmed);
                            }
                    group
                        .freeze_metadata()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                }
            }
            manager.core.native.core.source_sql.source_stamp(source)?;
            Ok(())
        })
    }
}
impl<P, L, G, N> SourceSessionManager<P, L, G, N>
where
    P: NativeApprovalPort + NativeSourceSessionPort,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = P::Streams>,
    G: SourceConfigurationPort<LaunchPlan = L::Plan, Setup = P::Setup>,
    N: SourceRepository,
{
    pub(crate) async fn reconcile<C, A: SourceSessionAuthority<C>>(
        &self,
        authority: &A,
        actor: &C,
        id: SourceSessionOperationId,
    ) -> Result<SourceSessionFact, SourceSessionFailure> {
        authority.verify_owner_scope(&self.core.scope)?;
        authority.current(actor).await?;
        let mut stored = self.reconcile_original(id, false).await?;
        stored.fact.result_available = self.result(id).is_some_and(|root| root.available());
        if let Some(instance) = self.core.native.core.source_sql.bound_source(id)? {
            let sources = self
                .core
                .native
                .core
                .sources
                .lock()
                .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
                .clone();
            for source in sources {
                if source.pre.metadata().instance_id == instance {
                    NativeCoordinator::<P, L, G, N>::request_source_metadata_retry(&source);
                }
            }
        }
        Ok(stored.fact)
    }
    async fn reconcile_original(
        &self,
        id: SourceSessionOperationId,
        source_only: bool,
    ) -> Result<SourceStoredOperation, SourceSessionFailure> {
        let group = self.core.native.core.source_sql.group(Some(id))?;
        let _serial = group.reconcile.lock().await;
        let originals = if source_only {
            group.source_entries()?
        } else {
            group.entries()?
        };
        for entry in &originals {
            entry.confirm(&self.deadline()?).await?;
        }
        let permit = SourceReconcileProof {
            scope: self.core.scope.clone(),
            id,
            original: group.clone(),
        };
        let sql = self
            .core
            .native
            .core
            .repository
            .source_sql_operation(SourceSqlRole::Reconcile, Some(id))?;
        let observed = sql.clone();
        let original = permit.clone();
        let native = self.core.native.clone();
        let d = self.deadline()?;
        let read = async move {
            native
                .core
                .repository
                .reconcile_source_session(&original, observed, &d)
                .await
        };
        let mut stored = if source_only {
            self.call_recorded(Some(id), sql, None, Some(permit.clone()), true, read)
                .await?
        } else {
            self.call(Some(id), sql, read).await?
        };
        let operation = self
            .core
            .operations
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .get(&id)
            .cloned();
        let mut updates = originals
            .iter()
            .filter_map(|entry| entry.update.clone())
            .collect::<Vec<_>>();
        if let Some(operation) = &operation
            && let (Some(access), Some(reply)) = (operation.access.get(), operation.reply.get()) {
                access.require_source(&reply.source)?;
                if let Some(ticket) = operation
                    .ticket
                    .lock()
                    .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
                    .clone()
                    && let super::super::TransportProgress::Finished(fact) = ticket.progress() {
                        updates.push(Arc::new(SourceWriteProof {
                            access: access.clone(),
                            update: SourceUpdate::Transport {
                                rpc: reply.rpc.clone(),
                                frame_length: reply.frame_length,
                                fact,
                            },
                            at: self.core.native.core.clock.unix_millis(),
                        }));
                    }
                if let Some(fact) = reply.response()
                    && fact.ack != SourceAckState::Pending {
                        updates.push(Arc::new(SourceWriteProof {
                            access: access.clone(),
                            update: SourceUpdate::Response {
                                rpc: reply.rpc.clone(),
                                fact,
                            },
                            at: self.core.native.core.clock.unix_millis(),
                        }));
                    }
            }
        for original in updates {
            if stored.registration != original.access.registration {
                return Err(SourceSessionError::Conflict.into());
            }
            let role = match original.view() {
                SourceUpdateView::Transport { rpc, fact, .. }
                    if stored.fact.rpc.as_ref() == Some(rpc)
                        && stored.fact.transport != Some(fact) =>
                {
                    Some(SourceSqlRole::Transport)
                }
                SourceUpdateView::Response { rpc, fact }
                    if stored.fact.rpc.as_ref() == Some(rpc)
                        && stored.fact.response.as_ref() != Some(fact) =>
                {
                    Some(SourceSqlRole::Response)
                }
                SourceUpdateView::Finish { outcome, .. }
                    if stored.fact.state == SourceSessionState::Owned
                        && stored.fact.outcome == SourceSessionOutcome::Pending
                        && outcome != SourceSessionOutcome::Succeeded =>
                {
                    Some(SourceSqlRole::Finish)
                }
                _ => None,
            };
            let Some(role) = role else {
                continue;
            };
            let sql = self
                .core
                .native
                .core
                .repository
                .source_sql_operation(role, Some(id))?;
            let observed = sql.clone();
            let proof = original.clone();
            let native = self.core.native.clone();
            let d = self.deadline()?;
            stored = self
                .call_recorded(
                    Some(id),
                    sql,
                    Some(original),
                    Some(permit.clone()),
                    source_only,
                    async move {
                        native
                            .core
                            .repository
                            .update_source_session(&proof, observed, &d)
                            .await
                    },
                )
                .await?;
        }
        // The stable result/error belongs to the original business task, not this metadata read.
        Ok(stored)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_sql_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<crate::test_support::SharedAsyncBarrier, SourceSessionFailure> {
        self.core.native.core.source_sql.pause_next_for_test(role)
    }
    #[cfg(test)]
    pub(crate) fn observe_next_invocation_for_test(
        &self,
        role: SourceSqlRole,
    ) -> Result<tokio::sync::oneshot::Receiver<Arc<SourceSqlInvocation>>, SourceSessionFailure> {
        self.core
            .native
            .core
            .source_sql
            .observe_next_invocation_for_test(role)
    }
    #[cfg(test)]
    pub(crate) fn reject_next_dispatch_for_test(
        &self,
        role: SourceSqlRole,
        error: SourceSessionError,
    ) -> Result<(), SourceSessionFailure> {
        self.core
            .native
            .core
            .source_sql
            .reject_next_dispatch_for_test(role, error)
    }
    #[cfg(test)]
    pub(crate) fn arm_next_sql_fault_for_test(
        &self,
        role: SourceSqlRole,
        fault: SourceSqlTestFault,
    ) -> Result<(), SourceSessionFailure> {
        self.core
            .native
            .core
            .source_sql
            .arm_next_sql_fault_for_test(role, fault)
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_decode_for_test(
        &self,
    ) -> Result<Arc<BlockingTestBarrier>, SourceSessionFailure> {
        self.core
            .native
            .core
            .source_sql
            .pause_next_cpu_decode_for_test()
    }
    #[cfg(test)]
    pub(crate) fn pause_next_cpu_retire_for_test(
        &self,
    ) -> Result<Arc<BlockingTestBarrier>, SourceSessionFailure> {
        self.core
            .native
            .core
            .source_sql
            .pause_next_cpu_retire_for_test()
    }
    #[cfg(test)]
    pub(crate) fn cpu_tasks_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<(usize, SourceTaskObservation)>, SourceSessionFailure> {
        let operations = self
            .core
            .operations
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        let Some(operation) = operations.get(&id) else {
            return Ok(Vec::new());
        };
        let cpu = operation
            .cpu
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?;
        Ok(cpu
            .iter()
            .map(|slot| {
                (
                    Weak::as_ptr(slot) as usize,
                    task_observation(slot.upgrade()),
                )
            })
            .collect())
    }
    #[cfg(test)]
    pub(crate) fn registered_native_tasks_for_test(&self) -> Result<usize, SourceSessionFailure> {
        self.core
            .native
            .core
            .tasks
            .registered_for_test()
            .map_err(Into::into)
    }
    #[cfg(test)]
    pub(crate) fn original_sql_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<(Option<SourceSqlObservation>, bool)>, SourceSessionFailure> {
        self.core.native.core.source_sql.observe_for_test(id)
    }
    #[cfg(test)]
    pub(crate) fn original_sql_cleanup_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Vec<Option<SourceSqlOriginalSnapshot>>, SourceSessionFailure> {
        self.core.native.core.source_sql.cleanup_for_test(id)
    }
    #[cfg(test)]
    pub(crate) fn source_close_for_test(
        &self,
        id: SourceSessionOperationId,
    ) -> Result<Option<(bool, Option<Result<(), DispatchError>>, usize)>, SourceSessionFailure>
    {
        let Some(instance) = self.core.native.core.source_sql.bound_source(id)? else {
            return Ok(None);
        };
        let sources = self
            .core
            .native
            .core
            .sources
            .lock()
            .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
            .clone();
        for source in sources {
            if source.pre.metadata().instance_id == instance {
                let physical_and_jobs = source
                    .cleanup
                    .get()
                    .is_some_and(|task| task.confirmed_joined())
                    && source.jobs.quiet_slots().is_ok()
                    && source.tasks.quiet_slots().is_ok()
                    && source
                        .io_closed
                        .lock()
                        .map_err(|_| SourceSessionError::WorkerUnconfirmed)?
                        .as_ref()
                        .is_some_and(|io| io.joined);
                let claims = self
                    .core
                    .native
                    .core
                    .environment
                    .ownership
                    .claims_for_source(instance)?;
                return Ok(Some((
                    physical_and_jobs,
                    source.metadata_close.result(),
                    claims,
                )));
            }
        }
        Ok(None)
    }
}
