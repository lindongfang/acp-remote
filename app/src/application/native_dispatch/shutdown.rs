//! Physical process/IO convergence precedes SQL release and the original registry closer.
#![allow(
    clippy::items_after_statements,
    clippy::option_if_let_else,
    clippy::redundant_pub_crate,
    clippy::significant_drop_tightening,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, ConfigurationPort, ConvergenceKind, ConvergenceMetadata, ConvergenceProbe, DispatchError,
    EntityId, InitializeMetadata, Instant, IoCloseKind, LifecycleKind, LoadTransportProof,
    LoadedResourceProof, NativeControlRepository, NativeCoordinator, NativeDispatchRepository,
    NativeIoOwner, NativeOwnerKind, NativeResourceRepository, NativeRuntimeHandle, Ordering,
    OwnedNativeSource, OwnedSourceShutdown, OwnerCloseKind, PromptTicket, ReservedPrompt,
    ResourceLifecycle, RpcScalar, SourceCloseBasis, SourceCloseStage, SourceClosureProof,
    SourceInspection, SourceLaunchPort, SourceLifecycle, SourceMetadata, SourceStateProof,
    StoredTransport, TransportFact, TransportProgress,
    runtime::{
        LifecycleResponse, LifecycleResponseValue, NativeRuntimeStatus, SourcePhase, SourceSlot,
    },
};
use crate::domain::native::{HarnessConnectionEpoch, LoadEpoch, ResourceEpoch};
use std::sync::{Mutex, OnceLock};
use tokio::sync::Notify;

pub(super) struct ShutdownOperation {
    deadline: Instant,
    report: Mutex<Option<NativeShutdownReport>>,
    changed: Notify,
    task: OnceLock<Arc<super::tasks::TaskSlot>>,
}
impl ShutdownOperation {
    fn finish(&self, report: NativeShutdownReport) {
        match self.report.lock() {
            Ok(mut value) => *value = Some(report),
            Err(error) => *error.into_inner() = Some(report),
        }
        self.changed.notify_waiters();
    }
    fn report(&self) -> Option<NativeShutdownReport> {
        match self.report.lock() {
            Ok(value) => *value,
            Err(error) => *error.into_inner(),
        }
    }
}
#[derive(Clone)]
pub(super) struct SourcePhysicalClosure {
    basis: SourceCloseBasis,
    stage: SourceCloseStage,
    owner: OwnerCloseKind,
    io: IoCloseKind,
    owner_witness: Option<super::source_ports::OwnerClosureWitness>,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct NativeSourceCloseDiagnostic {
    pub(crate) physical_present: bool,
    pub(crate) physical_stage: Option<SourceCloseStage>,
    pub(crate) physical_owner: Option<OwnerCloseKind>,
    pub(crate) metadata_close: Option<Result<(), DispatchError>>,
    pub(crate) task_slots: usize,
    pub(crate) job_slots: usize,
    pub(crate) view_failure: Option<crate::domain::shared_view::ViewError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeShutdownReport {
    pub joined: bool,
    pub metadata_confirmed: bool,
}
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
    #[must_use]
    pub fn status(&self) -> NativeRuntimeStatus {
        self.core.status()
    }
    /// # Errors
    /// Returns source/configuration/transport or necessary-state errors while retaining original cleanup ownership.
    pub async fn ensure_target(&self, target: EntityId) -> Result<(), DispatchError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let core = self.core.clone();
        let failed = self.core.clone();
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.ensure_resource(target).await.map(|_| ());
                let _ = tx.send(result);
            },
            move || failed.fail_all(DispatchError::RequiredStateUnavailable),
        )?;
        rx.await
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
    }
    /// Retries only this owner's current selected input after the previous actual ticket and SQL prove Zero.
    /// # Errors
    /// Refuses unknown writes, stale selections, unresolved necessary facts and replaced resources.
    pub async fn retry_zero(&self, target: EntityId) -> Result<(), DispatchError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let core = self.core.clone();
        let failed = self.core.clone();
        self.core.tasks.spawn_guarded(
            async move {
                let result = core.retry_zero(target).await;
                let _ = tx.send(result);
            },
            move || failed.fail_all(DispatchError::RequiredStateUnavailable),
        )?;
        rx.await
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
    }
    pub async fn shutdown(&self, deadline: Instant) -> NativeShutdownReport {
        self.core.close_runtime(deadline).await
    }
    #[cfg(test)]
    pub(crate) fn source_close_diagnostics(&self) -> Vec<NativeSourceCloseDiagnostic> {
        let sources = match self.core.sources.lock() {
            Ok(sources) => sources.clone(),
            Err(error) => error.into_inner().clone(),
        };
        sources
            .iter()
            .map(|source| {
                let (physical_present, physical_stage, physical_owner) =
                    match source.physical_closed.lock() {
                        Ok(physical) => (
                            physical.is_some(),
                            physical.as_ref().map(|physical| physical.stage),
                            physical.as_ref().map(|physical| physical.owner),
                        ),
                        Err(error) => {
                            let physical = error.into_inner();
                            (
                                physical.is_some(),
                                physical.as_ref().map(|physical| physical.stage),
                                physical.as_ref().map(|physical| physical.owner),
                            )
                        }
                    };
                NativeSourceCloseDiagnostic {
                    physical_present,
                    physical_stage,
                    physical_owner,
                    metadata_close: source.metadata_close.result(),
                    task_slots: source.tasks.registered_for_test().unwrap_or(usize::MAX),
                    job_slots: source.jobs.registered_for_test().unwrap_or(usize::MAX),
                    view_failure: source.view_attachment.unavailable_reason(),
                }
            })
            .collect()
    }
    #[must_use]
    pub fn last_shutdown_report(&self) -> Option<NativeShutdownReport> {
        match self.core.last_shutdown.lock() {
            Ok(value) => *value,
            Err(error) => *error.into_inner(),
        }
    }
    pub(crate) fn close_progress(&self) -> u64 {
        self.core.close_progress.current()
    }
    pub(crate) async fn wait_close_progress(&self, observed: u64) {
        self.core.close_progress.wait_after(observed).await;
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
    #[expect(
        clippy::significant_drop_tightening,
        reason = "Serialize publication until the original shutdown task slot is installed, so no concurrent waiter skips its join."
    )]
    pub(crate) async fn close_runtime(self: &Arc<Self>, deadline: Instant) -> NativeShutdownReport {
        loop {
            let (operation, created) = {
                let mut installed = match self.shutdown_operation.lock() {
                    Ok(value) => value,
                    Err(error) => error.into_inner(),
                };
                let reusable = installed
                    .as_ref()
                    .filter(|operation| {
                        operation
                            .report()
                            .is_none_or(|report| report.joined && report.metadata_confirmed)
                    })
                    .cloned();
                if let Some(operation) = reusable {
                    (operation, false)
                } else {
                    let operation = Arc::new(ShutdownOperation {
                        deadline,
                        report: Mutex::new(None),
                        changed: Notify::new(),
                        task: OnceLock::new(),
                    });
                    *installed = Some(operation.clone());
                    let core = self.clone();
                    let target = operation.clone();
                    let failed = self.clone();
                    let failure = operation.clone();
                    match self.shutdown_tasks.spawn_guarded(
                        async move {
                            let report = core.perform_close(deadline).await;
                            core.note_shutdown_report(report);
                            target.finish(report);
                        },
                        move || {
                            failed.fail_all(DispatchError::ResourceUnconfirmed);
                            let report = NativeShutdownReport {
                                joined: false,
                                metadata_confirmed: false,
                            };
                            failed.note_shutdown_report(report);
                            failure.finish(report);
                        },
                    ) {
                        Ok(task) => {
                            let _ = operation.task.set(task);
                        }
                        Err(_) => operation.finish(NativeShutdownReport {
                            joined: false,
                            metadata_confirmed: false,
                        }),
                    }
                    (operation, true)
                }
            };
            let waited = tokio::time::timeout_at(deadline, async {
                loop {
                    let changed = operation.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if let Some(report) = operation.report() {
                        if let Some(task) = operation.task.get()
                            && !task.join(deadline).await
                        {
                            return NativeShutdownReport {
                                joined: false,
                                metadata_confirmed: report.metadata_confirmed,
                            };
                        }
                        return report;
                    }
                    changed.await;
                }
            })
            .await;
            let report = waited.unwrap_or(NativeShutdownReport {
                joined: false,
                metadata_confirmed: false,
            });
            if created
                || (report.joined && report.metadata_confirmed)
                || operation.deadline >= deadline
                || Instant::now() >= deadline
                || operation.report().is_none()
            {
                return report;
            }
        }
    }
    fn note_shutdown_report(&self, report: NativeShutdownReport) {
        let mut previous = match self.last_shutdown.lock() {
            Ok(value) => value,
            Err(error) => error.into_inner(),
        };
        let changed = previous.is_some_and(|previous| previous != report);
        *previous = Some(report);
        drop(previous);
        if changed {
            self.close_progress.record();
        }
    }
    async fn perform_close(self: &Arc<Self>, deadline: Instant) -> NativeShutdownReport {
        self.request_shutdown();
        let Ok(_closing) = tokio::time::timeout_at(deadline, self.shutdown.lock()).await else {
            return NativeShutdownReport {
                joined: false,
                metadata_confirmed: false,
            };
        };
        if let Ok(sources) = self.sources.lock() {
            for source in &*sources {
                let _gate = match source.pre.authority.gate.lock() {
                    Ok(v) => v,
                    Err(e) => e.into_inner(),
                };
                source.pre.authority.closing.store(true, Ordering::SeqCst);
                #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
                self.repository
                    .stop_approval_test_source(source.pre.metadata().instance_id);
                Self::notify_approval_source_change(source);
                source.changed.notify_waiters();
                source.view_attachment.seal();
                if let Ok(mut budget) = source.close_deadline.lock() {
                    *budget = Some(deadline);
                }
                if let Ok(mut memory) = source.memory.lock()
                    && memory.phase != SourcePhase::Closed
                {
                    memory.phase = SourcePhase::Closing;
                }
            }
        }
        let _ = tokio::time::timeout_at(deadline, self.shutdown_controls()).await;
        self.cancel();
        let sources = match self.sources.lock() {
            Ok(s) => s.clone(),
            Err(e) => e.into_inner().clone(),
        };
        for source in &sources {
            Self::request_source_metadata_retry(source);
        }
        let mut metadata = true;
        for source in &sources {
            metadata &= self
                .wait_source_metadata_closed(source, deadline)
                .await
                .is_ok();
        }
        self.tasks.seal();
        let mut joined = self.tasks.join(deadline).await
            && sources
                .iter()
                .all(|source| source.view_attachment.joined_or_not_installed());
        #[cfg(windows)]
        if joined && metadata {
            self.source_close_tasks.seal();
            joined &= self.source_close_tasks.join(deadline).await;
        }
        if joined {
            self.status.store(4, Ordering::SeqCst);
        }
        NativeShutdownReport {
            joined,
            metadata_confirmed: metadata,
        }
    }
    pub(super) fn effective_close_deadline(
        &self,
        source: &SourceSlot<P, L>,
    ) -> Result<Instant, DispatchError> {
        let deadline = super::NativeRuntimeOptions::at(self.options.close_wait)?;
        let shutdown_deadline = source
            .close_deadline
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        Ok(shutdown_deadline.map_or(deadline, |closing| deadline.min(closing)))
    }
    pub(super) fn start_source_cleanup(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let core = self.clone();
        let slot = source.clone();
        let failed = self.clone();
        let failed_slot = source.clone();
        let task = self.tasks.spawn_guarded(
            async move {
                loop {
                    let changed = slot.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    if !slot.pre.authority.closed.load(Ordering::SeqCst) {
                        changed.await;
                        continue;
                    }
                    let Ok(deadline) = core.effective_close_deadline(&slot) else {
                        return;
                    };
                    if core
                        .collect_source_physical_once(&slot, deadline)
                        .await
                        .is_ok()
                    {
                        slot.changed.notify_waiters();
                        return;
                    }
                    let has_original_owner = slot.owned.lock().is_ok_and(|owned| owned.is_some());
                    if has_original_owner {
                        // Retry actual probes of the SAME retained owner/IO; the timer grants nothing.
                        tokio::select! {
                            ()=&mut changed=>{},
                            ()=tokio::time::sleep(std::time::Duration::from_millis(10))=>{},
                        }
                    } else {
                        // Entered with no owner has no object to probe. Never manufacture Job0 or busy-loop.
                        changed.await;
                    }
                }
            },
            move || failed.fail_source(&failed_slot, DispatchError::ResourceUnconfirmed),
        )?;
        source
            .cleanup
            .set(task)
            .map_err(|_| DispatchError::Conflict)
    }
    pub(super) async fn collect_source_physical(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        deadline: Instant,
    ) -> Result<(), DispatchError> {
        let cleanup = source
            .cleanup
            .get()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        if !cleanup.join(deadline).await || cleanup.failed() {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        if source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .is_none()
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the original owner and IO shutdown inventory together under the same source close/install serialization."
    )]
    async fn collect_source_physical_once(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        deadline: Instant,
    ) -> Result<(), DispatchError> {
        let _close = tokio::time::timeout_at(deadline, source.close.lock())
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .is_some()
        {
            return Ok(());
        }
        let _install = tokio::time::timeout_at(deadline, source.install.lock())
            .await
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        // Cleanup may use a retained original owner even if a bookkeeping lock was poisoned.
        // The later metadata proof still rejects poisoned coordination state.
        let process_owner = match source.owned.lock() {
            Ok(value) => value.clone(),
            Err(error) => error.into_inner().clone(),
        };
        let protocol_io = match source.io.lock() {
            Ok(value) => value.clone(),
            Err(error) => error.into_inner().clone(),
        };
        let (basis, stage, owner, io_kind) = if let Some(process_owner) = process_owner {
            if let Some(protocol_io) = &protocol_io {
                protocol_io.request_stop();
            }
            let existing = source
                .owner_closed
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .as_ref()
                .copied();
            let owner = if let Some(owner) = existing {
                owner
            } else {
                let observation = process_owner.lock().await.shutdown(deadline).await?;
                let owner = match observation {
                    OwnedSourceShutdown::WindowsJob {
                        active_processes: 0,
                        stderr_joined: true,
                        ..
                    } => OwnerCloseKind::WindowsJob {
                        active_processes: 0,
                    },
                    OwnedSourceShutdown::TestOwnedStream { tasks_joined: true } => {
                        OwnerCloseKind::TestOwnedStream
                    }
                    OwnedSourceShutdown::NeverCreated(proof) => {
                        proof.require_original(&source.controller)?;
                        *source
                            .owner_witness
                            .lock()
                            .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(
                            super::source_ports::OwnerClosureWitness::NeverCreated(proof),
                        );
                        OwnerCloseKind::NoProcessCreated
                    }
                    OwnedSourceShutdown::SuspendedUnassigned(proof) => {
                        proof.require_original(&source.controller)?;
                        *source
                            .owner_witness
                            .lock()
                            .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(
                            super::source_ports::OwnerClosureWitness::SuspendedUnassigned(proof),
                        );
                        OwnerCloseKind::SuspendedRootExited
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                };
                *source
                    .owner_closed
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(owner);
                source.progress.record();
                owner
            };
            let (io_kind, stage) = if let Some(protocol_io) = protocol_io {
                let result = protocol_io.shutdown(deadline).await;
                if !result.joined {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                *source
                    .io_closed
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(result);
                source.progress.record();
                if source.source.get().is_some() {
                    (IoCloseKind::StartedJoined, SourceCloseStage::Started)
                } else {
                    source
                        .prepared
                        .lock()
                        .map_err(|_| DispatchError::ResourceUnconfirmed)?
                        .take();
                    (
                        IoCloseKind::AdoptedUnstartedJoined,
                        SourceCloseStage::IoUnstarted,
                    )
                }
            } else {
                let rejected = source
                    .prepare_failure
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?
                    .is_some_and(|phase| matches!(phase, super::PrepareFailurePhase::BeforeAdopt));
                (
                    if rejected {
                        IoCloseKind::BeforeAdoptRejected
                    } else {
                        IoCloseKind::NotPrepared
                    },
                    SourceCloseStage::OwnedBeforeIo,
                )
            };
            if matches!(
                owner,
                OwnerCloseKind::NoProcessCreated | OwnerCloseKind::SuspendedRootExited
            ) {
                let memory = source
                    .memory
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                if io_kind != IoCloseKind::NotPrepared
                    || source.source.get().is_some()
                    || memory.scope.is_some()
                    || memory.binding.is_some()
                    || memory.initialized.is_some()
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                drop(memory);
                let witness = source
                    .owner_witness
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                match (owner, witness.as_ref()) {
                    (
                        OwnerCloseKind::NoProcessCreated,
                        Some(super::source_ports::OwnerClosureWitness::NeverCreated(proof)),
                    ) => {
                        let stage = match proof.require_original(&source.controller)? {
                            super::SourceCreationCut::BeforeCreateCall => {
                                SourceCloseStage::EnteredBeforeCreate
                            }
                            super::SourceCreationCut::CreateCallRejected => {
                                SourceCloseStage::CreateRejected
                            }
                        };
                        (SourceCloseBasis::EnteredNeverCreated, stage, owner, io_kind)
                    }
                    (
                        OwnerCloseKind::SuspendedRootExited,
                        Some(super::source_ports::OwnerClosureWitness::SuspendedUnassigned(proof)),
                    ) => {
                        proof.require_original(&source.controller)?;
                        (
                            SourceCloseBasis::OwnedConverged,
                            SourceCloseStage::OwnedBeforeIo,
                            owner,
                            io_kind,
                        )
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            } else {
                (SourceCloseBasis::OwnedConverged, stage, owner, io_kind)
            }
        } else {
            let revoked = source.controller.revoke_unentered()?;
            revoked.require_original(&source.controller)?;
            source
                .permit
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .take();
            (
                SourceCloseBasis::NeverLaunched,
                SourceCloseStage::NeverEntered,
                OwnerCloseKind::NotApplicable,
                IoCloseKind::NotApplicable,
            )
        };
        *source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(SourcePhysicalClosure {
            basis,
            stage,
            owner,
            io: io_kind,
            owner_witness: source
                .owner_witness
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .clone(),
        });
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Only original cached lifecycle objects can close these stages; keep the no-reinitialize reconciliation paths explicit."
    )]
    pub(super) async fn reconcile_source_lifecycle(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        // The original producers are joined. Reconfirm only facts that their retained objects prove.
        // This path never starts IO, synthesizes a response or installs a Loaded capability.
        let Some(mut row) = self
            .repository
            .inspect_source(&source.pre, &self.deadline()?)
            .await?
        else {
            return if source.recorded.load(Ordering::SeqCst) {
                Err(DispatchError::ResourceUnconfirmed)
            } else {
                Ok(())
            };
        };
        if row.lifecycle == SourceLifecycle::Closed {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let operations = source
            .operations
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for operation in operations {
            if operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .is_none()
            {
                let init = operation
                    .initialize
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .take();
                let close = operation
                    .close_objects
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .take();
                let probe = if let Some((reservation, _frame)) = init {
                    let probe = reservation.probe();
                    drop(reservation);
                    Some(probe)
                } else if let Some((reservation, _frame)) = close {
                    let probe = reservation.probe();
                    drop(reservation);
                    Some(probe)
                } else {
                    None
                };
                if let Some(probe) = probe {
                    *operation
                        .retired_probe
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? = Some(probe);
                }
                if let Some(probe) = operation
                    .retired_probe
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .as_ref()
                    && !matches!(
                        self.protocol.probe_progress(probe),
                        TransportProgress::Finished(TransportFact::Zero { .. })
                    )
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
            }
        }
        let spawned = source
            .spawned_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if row.lifecycle == SourceLifecycle::Reserved
            && let Some(proof) = spawned
        {
            row = self
                .repository
                .record_source_state(&proof, &self.deadline()?)
                .await?;
        }
        let bound = source
            .bound_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(binding) = row.scope_binding {
            let proof = bound.as_ref().ok_or(DispatchError::InvalidScope)?;
            if proof.binding != binding || row.scope_bound_at != Some(proof.at) {
                return Err(DispatchError::InvalidScope);
            }
        }
        // A failed Bound write must not be replayed merely to close an unstarted Pre stage.
        // Once actual IO started the original binding had already been confirmed.
        if source.source.get().is_some() && row.scope_binding.is_none() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let initialize = source
            .initialize
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(operation) = initialize {
            let terminal = operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .as_ref()
                .map(|ticket| ticket.progress());
            let response = operation
                .response
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if terminal == Some(TransportProgress::Finished(TransportFact::Written))
                && let Some(LifecycleResponse {
                    sequence,
                    receipt,
                    value: LifecycleResponseValue::Initialized(wire),
                }) = response
            {
                let proof = {
                    let mut saved = operation
                        .initialize_record
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    saved
                        .get_or_insert_with(|| {
                            Arc::new(SourceStateProof {
                                source: source.pre.clone(),
                                state: SourceLifecycle::Initialized,
                                initialized: Some(InitializeMetadata {
                                    rpc: operation.rpc.clone(),
                                    wire,
                                    sequence,
                                    receipt,
                                    fingerprint: super::source::metadata_stamp(
                                        b"native-initialize",
                                        source.pre.metadata.instance_id,
                                        sequence,
                                        receipt,
                                    ),
                                }),
                                at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                self.repository
                    .record_source_state(&proof, &self.deadline()?)
                    .await?;
            }
        }
        let resources = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for resource in resources {
            let Some(row) = self
                .repository
                .inspect_resource(&resource.pre, &self.deadline()?)
                .await?
            else {
                if resource.recorded.load(Ordering::SeqCst) {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                continue;
            };
            if row.lifecycle == ResourceLifecycle::Released {
                continue;
            }
            let operation = resource
                .load
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            let Some(operation) = operation else {
                continue;
            };
            // A failed/unknown BEGIN may have left no load columns. Preserve that real Pre stage.
            if row.load_rpc.as_ref() != Some(&operation.rpc) {
                if row.load_rpc.is_some() {
                    return Err(DispatchError::Conflict);
                }
                continue;
            }
            let progress = operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .as_ref()
                .map(|ticket| ticket.progress());
            let terminal = if let Some(TransportProgress::Finished(fact)) = progress {
                fact
            } else if progress.is_some() {
                return Err(DispatchError::ResourceUnconfirmed);
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
                    *operation
                        .cancelled_resume
                        .lock()
                        .map_err(|_| DispatchError::Stopped)? = Some(cancelled);
                }
                let saved = operation
                    .cancelled_resume
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                match saved
                    .as_ref()
                    .map(|receipt| self.protocol.probe_progress(&receipt.probe))
                {
                    Some(TransportProgress::Finished(fact @ TransportFact::Zero { .. })) => fact,
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            };
            let proof = {
                let mut saved = operation
                    .transport_record
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                saved
                    .get_or_insert_with(|| {
                        Arc::new(LoadTransportProof {
                            resource: resource.pre.clone(),
                            rpc: operation.rpc.clone(),
                            frame_length: operation.frame_length,
                            fact: terminal,
                            at: self.clock.unix_millis(),
                        })
                    })
                    .clone()
            };
            self.repository
                .record_load_transport(&proof, &self.deadline()?)
                .await?;
            let response = operation
                .response
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .clone();
            if terminal == TransportFact::Written
                && let Some(LifecycleResponse {
                    sequence,
                    receipt,
                    value: LifecycleResponseValue::Resumed(session),
                }) = response
                && session.as_ref() == resource.pre.native.native_id()
            {
                let proof = {
                    let mut saved = operation
                        .loaded_record
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?;
                    saved
                        .get_or_insert_with(|| {
                            Arc::new(LoadedResourceProof {
                                resource: resource.pre.clone(),
                                rpc: operation.rpc.clone(),
                                sequence,
                                receipt,
                                fingerprint: super::source::metadata_stamp(
                                    b"native-resumed",
                                    resource.pre.metadata.load_epoch,
                                    sequence,
                                    receipt,
                                ),
                                at: self.clock.unix_millis(),
                            })
                        })
                        .clone()
                };
                self.repository
                    .record_loaded(&proof, &self.deadline()?)
                    .await?;
            }
        }
        let failure = source
            .failure_record
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        if let Some(proof) = failure {
            self.repository
                .record_source_state(&proof, &self.deadline()?)
                .await?;
        }
        Ok(())
    }
    #[expect(
        clippy::too_many_lines,
        reason = "The exact reserved set and original closure proof remain one idempotent SQL and coordination retirement operation."
    )]
    pub(super) async fn persist_source_closed(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let physical = source
            .physical_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        let resources: Vec<_> = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone();
        let existing = source
            .closure
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone();
        let proof = if let Some(proof) = existing {
            proof
        } else {
            let pres: Vec<_> = resources.iter().map(|r| r.pre.clone()).collect();
            let expected = self
                .repository
                .inspect_source_resources(&source.pre, &pres, &self.deadline()?)
                .await?;
            if expected.source.is_none() {
                if source.recorded.load(Ordering::SeqCst)
                    || !expected.resources.is_empty()
                    || physical.basis != SourceCloseBasis::NeverLaunched
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                for resource in &resources {
                    if resource.recorded.load(Ordering::SeqCst) {
                        return Err(DispatchError::ResourceUnconfirmed);
                    }
                    resource.pre.authority.closed.store(true, Ordering::SeqCst);
                    source.view_attachment.seal_target(resource.pre.native());
                    self.environment
                        .retire_closed_target(resource.pre.native(), &resource.target);
                }
                source
                    .memory
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .phase = SourcePhase::Closed;
                return Ok(());
            }
            if resources.iter().any(|resource| {
                resource.recorded.load(Ordering::SeqCst)
                    && !expected
                        .resources
                        .iter()
                        .any(|row| row.metadata == resource.pre.metadata)
            }) {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            let approvals = self.collect_approval_closure(source).await?;
            let fingerprint =
                self.source_closure_fingerprint(source, &expected, &physical, &approvals)?;
            let proof = Arc::new(SourceClosureProof {
                approvals,
                source: source.pre.clone(),
                expected,
                basis: physical.basis,
                stage: physical.stage,
                owner: physical.owner,
                owner_witness: physical.owner_witness.clone(),
                quiescence: Self::source_quiescence(source)?,
                io: physical.io,
                fingerprint,
                at: self.clock.unix_millis(),
            });
            *source
                .closure
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(proof.clone());
            proof
        };
        let closed = self
            .repository
            .record_source_closed(&source.pre, &proof, &self.context(), &self.deadline()?)
            .await?;
        #[cfg(windows)]
        {
            let replies = source
                .source_replies
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for reply in replies {
                reply.source_closed(&proof);
            }
            self.environment.release_source_claims(&proof)?;
        }
        for permission in source
            .permissions
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .values()
        {
            permission
                .state
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .retired = true;
        }
        for resource in &resources {
            if !closed.resources.iter().any(|r| {
                r.metadata == resource.pre.metadata && r.lifecycle == ResourceLifecycle::Released
            }) {
                if !resource.recorded.load(Ordering::SeqCst)
                    && !proof
                        .expected
                        .resources
                        .iter()
                        .any(|row| row.metadata == resource.pre.metadata)
                {
                    resource.pre.authority.closed.store(true, Ordering::SeqCst);
                    source.view_attachment.seal_target(resource.pre.native());
                    self.environment
                        .retire_closed_target(resource.pre.native(), &resource.target);
                    continue;
                }
                return Err(DispatchError::ResourceUnconfirmed);
            }
            if matches!(
                proof.basis,
                SourceCloseBasis::NeverLaunched | SourceCloseBasis::EnteredNeverCreated
            ) || proof.owner == OwnerCloseKind::SuspendedRootExited
            {
                if !source
                    .dispatches
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?
                    .is_empty()
                {
                    return Err(DispatchError::ResourceUnconfirmed);
                }
                // The original reserved set is closed by its real source proof. No target IO,
                // Convergence observation, Job0 or IO-join fact is invented for an unstarted resource.
                resource.pre.authority.closed.store(true, Ordering::SeqCst);
                source.view_attachment.seal_target(resource.pre.native());
                self.environment
                    .retire_closed_target(resource.pre.native(), &resource.target);
                continue;
            }
            if resource.pre.authority.closed.load(Ordering::SeqCst) {
                if resource
                    .convergence
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?
                    .is_some()
                {
                    let retirement = Self::source_resource_retirement(source, resource)?;
                    self.retire_converged_run(source, resource, retirement.as_ref())
                        .await?;
                }
                self.environment
                    .retire_closed_target(resource.pre.native(), &resource.target);
                continue;
            }
            let epoch = ResourceEpoch {
                runtime: self.runtime,
                connection: HarnessConnectionEpoch(source.pre.metadata.connection_epoch),
                load: LoadEpoch(resource.pre.metadata.load_epoch),
            };
            let existing = resource
                .convergence
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .clone();
            let convergence = if let Some(probe) = existing {
                probe
            } else {
                let requirement = Self::new_id()?;
                let mut order = 0;
                resource.queue.apply(epoch, &mut |v| {
                    order = v;
                    Ok(crate::application::queue_owner::NativeQueuePolicy::None)
                })?;
                let probe = Arc::new(ConvergenceProbe {
                    metadata: ConvergenceMetadata {
                        id: Self::new_id()?,
                        requirement,
                        resource: resource.pre.metadata.clone(),
                        owner_order: order,
                        kind: ConvergenceKind::JoinedOwnedSource,
                        fingerprint: *proof.fingerprint(),
                        at: self.clock.unix_millis(),
                    },
                    authority: resource.pre.authority.clone(),
                });
                *resource
                    .convergence
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)? = Some(probe.clone());
                probe
            };
            self.repository
                .record_convergence(&convergence, &self.context(), &self.deadline()?)
                .await?;
            resource.pre.authority.closed.store(true, Ordering::SeqCst);
            source.view_attachment.seal_target(resource.pre.native());
            let retirement = Self::source_resource_retirement(source, resource)?;
            self.retire_converged_run(source, resource, retirement.as_ref())
                .await?;
            self.environment
                .retire_closed_target(resource.pre.native(), &resource.target);
        }
        let mut closed_natives = Vec::new();
        for resource in &resources {
            if !closed_natives.contains(&resource.pre.native) {
                closed_natives.push(resource.pre.native.clone());
            }
        }
        if let Ok(capacities) = source.capacity.lock() {
            for key in capacities.keys() {
                if !closed_natives.contains(key) {
                    closed_natives.push(key.clone());
                }
            }
        }
        source
            .memory
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .phase = SourcePhase::Closed;
        let _ = self.session_control.clear_on_source_closed(&closed_natives);
        Ok(())
    }
    fn source_resource_retirement(
        source: &Arc<SourceSlot<P, L>>,
        resource: &Arc<super::runtime::ResourceSlot<P>>,
    ) -> Result<Option<super::RetiredResourceProof>, DispatchError> {
        let Some(loaded) = resource.loaded.get() else {
            return Ok(None);
        };
        let proof = source
            .closure
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        proof.quiescence.flags(source)?;
        let convergence = resource
            .convergence
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        Ok(Some(super::RetiredResourceProof {
            resource: resource.pre.clone(),
            source: proof,
            convergence,
            epoch: loaded.binding().resource,
        }))
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Every original task, IO, permission, control and dispatch witness must be included in this single ordered digest."
    )]
    fn source_closure_fingerprint(
        &self,
        source: &Arc<SourceSlot<P, L>>,
        expected: &SourceInspection,
        physical: &SourcePhysicalClosure,
        approvals: &super::ApprovalClosureManifest,
    ) -> Result<[u8; 32], DispatchError> {
        let mut digest = ClosureDigest::new();
        digest.raw(approvals.fingerprint());
        digest.text(physical.basis.label())?;
        digest.text(physical.stage.label())?;
        digest.text(physical.owner.label())?;
        digest.optional_u64(physical.owner.active_processes().map(u64::from));
        digest.text(physical.io.label())?;
        digest.flag(physical.owner_witness.is_some());
        if let Some(witness) = &physical.owner_witness {
            match witness {
                super::source_ports::OwnerClosureWitness::NeverCreated(proof) => {
                    digest.raw(&[0]);
                    digest.raw(&[match proof.require_original(&source.controller)? {
                        super::SourceCreationCut::BeforeCreateCall => 0,
                        super::SourceCreationCut::CreateCallRejected => 1,
                    }]);
                }
                super::source_ports::OwnerClosureWitness::SuspendedUnassigned(proof) => {
                    proof.require_original(&source.controller)?;
                    digest.raw(&[1]);
                    digest.u64(u64::from(proof.root_exit_code()));
                }
            }
        }
        digest.bytes(&Self::source_quiescence(source)?.flags(source)?)?;
        let dispatch_guard = source
            .dispatch_closures
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        let dispatches = dispatch_guard
            .as_ref()
            .ok_or(DispatchError::ResourceUnconfirmed)?;
        digest.length(dispatches.len())?;
        for fingerprint in dispatches {
            digest.raw(fingerprint);
        }
        drop(dispatch_guard);
        let io = source
            .io_closed
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        let prepare_failure = source
            .prepare_failure
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        digest.raw(&[match *prepare_failure {
            None => 0,
            Some(super::PrepareFailurePhase::BeforeAdopt) => 1,
            Some(super::PrepareFailurePhase::AfterAdopt) => 2,
        }]);
        drop(prepare_failure);
        digest.flag(io.is_some());
        if let Some(io) = &*io {
            if !io.joined {
                return Err(DispatchError::ResourceUnconfirmed);
            }
            digest.flag(io.failed);
            digest.length(io.outstanding.len())?;
            for progress in &io.outstanding {
                match progress {
                    TransportProgress::Finished(fact) => {
                        digest.transport(Some(StoredTransport::Terminal(*fact)));
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            }
            digest.length(io.unresolved.len())?;
            for rpc in &io.unresolved {
                digest.rpc(Some(rpc))?;
            }
        }
        drop(io);
        let operations = source
            .operations
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        digest.length(operations.len())?;
        for operation in operations {
            digest.rpc(Some(&operation.rpc))?;
            digest.raw(&[match operation.kind {
                LifecycleKind::Create => 4,
                LifecycleKind::Discover => 5,
                LifecycleKind::Initialize => 1,
                LifecycleKind::Resume => 2,
                LifecycleKind::Close => 3,
            }]);
            digest.u64(operation.frame_length);
            digest.i64(operation.at);
            let ticket = operation
                .ticket
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            digest.flag(ticket.is_some());
            if let Some(ticket) = &*ticket {
                match ticket.progress() {
                    TransportProgress::Finished(fact) => {
                        digest.transport(Some(StoredTransport::Terminal(fact)));
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            }
            drop(ticket);
            let response = operation
                .response
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            digest.flag(response.is_some());
            if let Some(response) = &*response {
                digest.u64(response.sequence);
                digest.u64(response.receipt);
                match &response.value {
                    LifecycleResponseValue::Initialized(wire) => {
                        digest.raw(&[1]);
                        digest.u64(wire.number().cast_unsigned());
                    }
                    LifecycleResponseValue::Resumed(session) => {
                        digest.raw(&[2]);
                        digest.text(session)?;
                    }
                    LifecycleResponseValue::Closed(session) => {
                        digest.raw(&[3]);
                        digest.text(session)?;
                    }
                    LifecycleResponseValue::Error => digest.raw(&[4]),
                }
            }
            drop(response);
            let cancelled = operation
                .cancelled_resume
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            digest.flag(cancelled.is_some());
            if let Some(cancelled) = &*cancelled {
                digest.rpc(Some(&cancelled.rpc))?;
                match self.protocol.probe_progress(&cancelled.probe) {
                    TransportProgress::Finished(fact @ TransportFact::Zero { .. }) => {
                        digest.transport(Some(StoredTransport::Terminal(fact)));
                    }
                    _ => return Err(DispatchError::ResourceUnconfirmed),
                }
            }
        }
        #[cfg(windows)]
        {
            let replies = {
                let guard = source
                    .source_replies
                    .lock()
                    .map_err(|_| DispatchError::ResourceUnconfirmed)?;
                let mut entries = guard
                    .iter()
                    .map(|(rpc, reply)| (rpc.clone(), reply.clone()))
                    .collect::<Vec<_>>();
                fn rpc_sort_key(rpc: &RpcScalar) -> (u8, i64, &str) {
                    match rpc {
                        RpcScalar::Integer(value) => (0, *value, ""),
                        RpcScalar::String(value) => (1, 0, value.as_ref()),
                    }
                }
                entries
                    .sort_by(|(left, _), (right, _)| rpc_sort_key(left).cmp(&rpc_sort_key(right)));
                entries
            };
            digest.length(replies.len())?;
            for (rpc, reply) in replies {
                digest.rpc(Some(&rpc))?;
                digest.raw(&reply.close_stamp()?);
            }
            digest.raw(
                &self
                    .source_sql
                    .source_stamp(source.pre.metadata().instance_id)?,
            );
        }
        let memory = source
            .memory
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if !memory.requirements.is_empty() {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        digest.u64(memory.observed_sequence);
        digest.optional_id(memory.binding);
        digest.flag(memory.scope.is_some());
        drop(memory);
        if source.pre.authority.required.load(Ordering::SeqCst) != 0 {
            return Err(DispatchError::RequiredStateUnavailable);
        }
        let resources = source
            .all_resources
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone();
        digest.length(resources.len())?;
        for resource in resources {
            let facts = resource
                .target
                .facts
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            if !facts.requirements.is_empty() || !facts.failed_requirements.is_empty() {
                return Err(DispatchError::RequiredStateUnavailable);
            }
            digest.id(resource.pre.metadata.load_epoch);
            digest.u64(facts.interval);
            digest.flag(facts.interval_open);
            digest.flag(facts.interval_live);
            digest.u64(facts.terminal_sequence);
            digest.flag(facts.pending_input.is_some());
            if let Some(input) = &facts.pending_input {
                digest.id(input.dispatch);
                digest.id(input.attempt);
                digest.rpc(Some(&input.rpc))?;
                digest.flag(input.inserted);
                digest.optional_u64(input.native_reason.map(|reason| reason as u64));
            }
            let mut holds = facts.control_holds.iter().copied().collect::<Vec<_>>();
            holds.sort_by_key(|id| id.encode());
            digest.length(holds.len())?;
            for id in holds {
                digest.id(id);
            }
            digest.length(facts.permissions.len())?;
            for (rpc, permission) in &facts.permissions {
                let permission = permission.snapshot()?;
                digest.rpc(Some(rpc))?;
                digest.optional_id(permission.obligation);
                digest.optional_id(permission.claimed);
                digest.flag(permission.written);
                digest.flag(permission.retired);
            }
        }
        let permissions = source
            .permissions
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        digest.length(permissions.len())?;
        for permission in permissions.values() {
            let state = permission.snapshot()?;
            digest.rpc(Some(&permission.rpc))?;
            digest.u64(permission.sequence);
            digest.u64(permission.receipt);
            digest.flag(permission.association.is_some());
            if let Some(resource) = &permission.association {
                if resource.metadata.source != source.pre.metadata {
                    return Err(DispatchError::InvalidSource);
                }
                digest.id(resource.metadata.target);
                digest.id(resource.metadata.owner);
                digest.id(resource.metadata.load_epoch);
            }
            digest.optional_id(state.obligation);
            digest.optional_id(state.claimed);
            digest.flag(state.written);
            digest.flag(state.retired);
        }
        drop(permissions);
        let row = expected
            .source
            .as_ref()
            .ok_or(DispatchError::InvalidSource)?;
        digest.source_metadata(&row.metadata);
        digest.text(row.lifecycle.label())?;
        digest.optional_id(row.scope_binding);
        digest.optional_i64(row.scope_bound_at);
        digest.optional_u64(row.wire.map(|wire| wire.number().cast_unsigned()));
        digest.rpc(row.initialize_rpc.as_ref())?;
        digest.optional_u64(row.initialize_sequence);
        digest.optional_u64(row.initialize_receipt);
        digest.optional_hash(row.evidence);
        digest.i64(row.updated_at);
        digest.flag(row.close.is_some());
        if let Some(close) = &row.close {
            for value in [&close.basis, &close.stage, &close.io, &close.owner] {
                digest.text(value)?;
            }
            digest.optional_u64(close.active_processes.map(u64::from));
            digest.raw(&close.fingerprint);
        }
        digest.length(expected.resources.len())?;
        for row in &expected.resources {
            digest.source_metadata(&row.metadata.source);
            for id in [
                row.metadata.target,
                row.metadata.owner,
                row.metadata.load_epoch,
            ] {
                digest.id(id);
            }
            digest.i64(row.metadata.reserved_at_ms);
            digest.text(row.lifecycle.label())?;
            digest.optional_id(row.scope_binding);
            digest.optional_u64(row.wire.map(|wire| wire.number().cast_unsigned()));
            digest.rpc(row.load_rpc.as_ref())?;
            digest.optional_u64(row.frame_length);
            digest.transport(row.transport);
            digest.optional_u64(row.loaded_sequence);
            digest.optional_u64(row.loaded_receipt);
            digest.optional_hash(row.evidence);
            digest.i64(row.updated_at);
            digest.flag(row.release.is_some());
            if let Some(release) = &row.release {
                digest.text(&release.basis)?;
                digest.text(&release.stage)?;
                digest.raw(&release.fingerprint);
            }
        }
        let observations = source
            .observations
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        digest.length(observations.len())?;
        for observation in &*observations {
            digest.raw(&observation.persisted_fingerprint()?);
        }
        drop(observations);
        let tree_observations = source
            .tree_observations
            .lock()
            .map_err(|_| DispatchError::Stopped)?;
        digest.length(tree_observations.len())?;
        for observation in &*tree_observations {
            digest.raw(&observation.persisted_fingerprint()?);
        }
        drop(tree_observations);
        let controls = source.controls.lock().map_err(|_| DispatchError::Stopped)?;
        digest.length(controls.len())?;
        for control in &*controls {
            let (reserved, terminal) = control.persisted_fingerprints()?;
            digest.raw(&reserved);
            digest.raw(&terminal);
        }
        drop(controls);
        let staged = source
            .control_staging
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        digest.length(staged.len())?;
        for stage in &*staged {
            let scope = source
                .memory
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                .scope
                .clone()
                .ok_or(DispatchError::InvalidScope)?;
            let evidence = stage.closure_evidence(&scope, &*self.protocol)?;
            digest.flag(evidence.is_some());
            if let Some(evidence) = evidence {
                digest.text(evidence.kind.label())?;
                digest.text(evidence.intent.label())?;
                digest.optional_id(evidence.reference);
                digest.rpc(evidence.rpc.as_ref())?;
                digest.text(&evidence.error.to_string())?;
                digest.transport(Some(StoredTransport::Terminal(evidence.zero)));
                digest.flag(evidence.resource.is_some());
                if let Some(resource) = evidence.resource {
                    if resource.source != source.pre.metadata {
                        return Err(DispatchError::InvalidResource);
                    }
                    digest.id(resource.target);
                    digest.id(resource.owner);
                    digest.id(resource.load_epoch);
                }
            }
        }
        drop(staged);
        let loss = source.loss.lock().map_err(|_| DispatchError::Stopped)?;
        digest.flag(loss.is_some());
        if let Some(loss) = &*loss {
            digest.id(loss.loss_id);
            digest.id(loss.requirement);
            digest.id(loss.binding);
            digest.raw(&loss.fingerprint);
            digest.i64(loss.at);
            digest.length(loss.targets.len())?;
            for target in &loss.targets {
                for id in [
                    target.target,
                    target.load_epoch,
                    target.owner,
                    target.requirement,
                ] {
                    digest.id(id);
                }
                digest.u64(target.owner_order);
                digest.raw(&target.fingerprint);
            }
        }
        drop(loss);
        Ok(digest.finish())
    }
}

pub(super) struct ClosureDigest(sha2::Sha256);
impl ClosureDigest {
    pub(super) fn new() -> Self {
        use sha2::Digest;
        let mut result = Self(sha2::Sha256::new());
        result.raw(b"native-source-closure-exact-v1");
        result
    }
    pub(super) fn raw(&mut self, bytes: &[u8]) {
        use sha2::Digest;
        self.0.update(bytes);
    }
    pub(super) fn flag(&mut self, value: bool) {
        self.raw(&[u8::from(value)]);
    }
    pub(super) fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    pub(super) fn i64(&mut self, value: i64) {
        self.raw(&value.to_be_bytes());
    }
    pub(super) fn length(&mut self, value: usize) -> Result<(), DispatchError> {
        self.u64(u64::try_from(value).map_err(|_| DispatchError::LengthUnrepresentable)?);
        Ok(())
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), DispatchError> {
        self.length(bytes.len())?;
        self.raw(bytes);
        Ok(())
    }
    pub(super) fn text(&mut self, value: &str) -> Result<(), DispatchError> {
        self.bytes(value.as_bytes())
    }
    pub(super) fn id(&mut self, value: EntityId) {
        self.raw(value.encode().as_bytes());
    }
    pub(super) fn optional_id(&mut self, value: Option<EntityId>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.id(value);
        }
    }
    fn optional_u64(&mut self, value: Option<u64>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.u64(value);
        }
    }
    pub(super) fn optional_i64(&mut self, value: Option<i64>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.i64(value);
        }
    }
    fn optional_hash(&mut self, value: Option<[u8; 32]>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.raw(&value);
        }
    }
    pub(super) fn rpc(&mut self, value: Option<&RpcScalar>) -> Result<(), DispatchError> {
        match value {
            None => self.raw(&[0]),
            Some(RpcScalar::Integer(value)) => {
                self.raw(&[1]);
                self.i64(*value);
            }
            Some(RpcScalar::String(value)) => {
                self.raw(&[2]);
                self.text(value)?;
            }
        }
        Ok(())
    }
    fn source_metadata(&mut self, value: &SourceMetadata) {
        for id in [
            value.source_id,
            value.instance_id,
            value.runtime.0,
            value.connection_epoch,
            value.config_identity,
        ] {
            self.id(id);
        }
        self.u64(value.config_revision);
        self.raw(&[match value.owner_kind {
            NativeOwnerKind::WindowsJob => 1,
            NativeOwnerKind::TestOwnedStream => 2,
        }]);
        self.i64(value.created_at_ms);
    }
    pub(super) fn transport(&mut self, value: Option<StoredTransport>) {
        match value {
            None => self.raw(&[0]),
            Some(StoredTransport::Armed) => self.raw(&[1]),
            Some(StoredTransport::Terminal(TransportFact::Written)) => self.raw(&[2]),
            Some(StoredTransport::Terminal(TransportFact::Zero { reason })) => {
                self.raw(&[3]);
                self.raw(reason.label().as_bytes());
                self.raw(&[0]);
            }
            Some(StoredTransport::Terminal(TransportFact::Unknown { accepted, cause })) => {
                self.raw(&[4]);
                self.u64(accepted);
                self.raw(cause.label().as_bytes());
                self.raw(&[0]);
            }
        }
    }
    pub(super) fn finish(self) -> [u8; 32] {
        use sha2::Digest;
        self.0.finalize().into()
    }
}
