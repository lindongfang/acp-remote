//! Read-only diagnostics from the actual registered owner; no setter or capability constructor.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct NativeParkSnapshot {
    pub source_required: usize,
    pub target_requirements: usize,
    pub failed_requirements: usize,
    pub control_holds: usize,
    pub writer_registered: bool,
    pub attempt: Option<EntityId>,
    pub ticket: Option<TransportProgress>,
    pub source_tasks: usize,
    pub sequence: u64,
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
    /// # Errors
    /// A poisoned owner is reported unavailable, never Ready.
    pub fn park_snapshot_for_test(
        &self,
        target: EntityId,
    ) -> Result<Option<NativeParkSnapshot>, DispatchError> {
        let sources = self
            .core
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        for source in sources {
            let resource = source
                .targets
                .lock()
                .map_err(|_| DispatchError::Stopped)?
                .values()
                .find(|resource| resource.pre.metadata.target == target)
                .cloned();
            if let Some(resource) = resource {
                let facts = resource
                    .target
                    .facts
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?;
                let snapshot = NativeParkSnapshot {
                    source_required: source.pre.authority.required.load(Ordering::SeqCst),
                    target_requirements: facts.requirements.len(),
                    failed_requirements: facts.failed_requirements.len(),
                    control_holds: facts.control_holds.len(),
                    writer_registered: facts.writer_parked.is_some(),
                    attempt: facts.pending_input.as_ref().map(|input| input.attempt),
                    ticket: None,
                    source_tasks: source.tasks.registered_for_test()?,
                    sequence: 0,
                };
                drop(facts);
                let run = self
                    .core
                    .runs
                    .lock()
                    .map_err(|_| DispatchError::Stopped)?
                    .get(resource.pre.native())
                    .cloned();
                return Ok(Some(NativeParkSnapshot {
                    ticket: run
                        .map(|run| run.transport_for_test())
                        .transpose()?
                        .flatten(),
                    sequence: source
                        .memory
                        .lock()
                        .map_err(|_| DispatchError::Stopped)?
                        .observed_sequence,
                    ..snapshot
                }));
            }
        }
        Ok(None)
    }

    /// # Errors
    /// A poisoned owner is reported unavailable.
    pub fn any_source_quarantined_for_test(&self) -> Result<bool, DispatchError> {
        let sources = self
            .core
            .sources
            .lock()
            .map_err(|_| DispatchError::Stopped)?
            .clone();
        Ok(sources
            .iter()
            .any(|source| source.pre.authority.quarantined.load(Ordering::SeqCst)))
    }
}
