//! Actual source jobs outlive receivers; a single closer joins the original producer sets.
use super::{
    Arc, ConfigurationPort, DispatchError, Future, Instant, NativeControlRepository,
    NativeCoordinator, NativeDispatchRepository, NativeResourceRepository, Ordering,
    OwnedNativeSource, PreSourceProbe, SourceLaunchPort, runtime::SourceSlot, tasks::TaskSlot,
};
use std::sync::{Mutex, atomic::AtomicU64};
use tokio::sync::Notify;

pub(super) struct SourceCloseOperation {
    generation: AtomicU64,
    changed: Notify,
    state: Mutex<SourceCloseState>,
}
struct SourceCloseState {
    completed: Option<Result<(), DispatchError>>,
    quiescence: Option<Arc<SourceQuiescence>>,
}
impl SourceCloseOperation {
    pub(super) const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            changed: Notify::const_new(),
            state: Mutex::new(SourceCloseState {
                completed: None,
                quiescence: None,
            }),
        }
    }
    fn complete(&self, result: Result<(), DispatchError>) {
        match self.state.lock() {
            Ok(mut state) => state.completed = Some(result),
            Err(error) => {
                error.into_inner().completed = Some(Err(DispatchError::ResourceUnconfirmed));
            }
        }
        self.changed.notify_waiters();
    }
    pub(super) fn result(&self) -> Option<Result<(), DispatchError>> {
        self.state
            .lock()
            .map_or(Some(Err(DispatchError::ResourceUnconfirmed)), |state| {
                state.completed
            })
    }
}
pub(super) struct SourceQuiescence {
    source: Arc<PreSourceProbe>,
    jobs: Vec<Arc<TaskSlot>>,
    children: Vec<Arc<TaskSlot>>,
    cleanup: Arc<TaskSlot>,
}
impl SourceQuiescence {
    pub(super) fn confirms(&self, source: &Arc<PreSourceProbe>) -> bool {
        Arc::ptr_eq(&self.source, source)
            && source.authority.closed.load(Ordering::Acquire)
            && self.cleanup.confirmed_joined()
            && self
                .jobs
                .iter()
                .chain(&self.children)
                .all(|slot| slot.confirmed_joined())
    }
    pub(super) fn flags<P, L>(&self, source: &SourceSlot<P, L>) -> Result<Vec<u8>, DispatchError>
    where
        P: crate::application::native_dispatch::NativeApprovalPort,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = P::Streams>,
    {
        if !Arc::ptr_eq(&self.source, &source.pre)
            || !source
                .cleanup
                .get()
                .is_some_and(|slot| Arc::ptr_eq(slot, &self.cleanup))
            || !self.cleanup.confirmed_joined()
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        let jobs = source.jobs.quiet_slots()?;
        let children = source.tasks.quiet_slots()?;
        for (current, original) in [(&jobs, &self.jobs), (&children, &self.children)] {
            if current.len() != original.len()
                || !current.iter().zip(original).all(|(a, b)| Arc::ptr_eq(a, b))
            {
                return Err(DispatchError::ResourceUnconfirmed);
            }
        }
        let mut flags = source.jobs.quiet_flags()?;
        flags.extend(source.tasks.quiet_flags()?);
        flags.push(u8::from(self.cleanup.failed()));
        Ok(flags)
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
    pub(super) async fn run_source_job<T: Send + 'static>(
        self: &Arc<Self>,
        source: Arc<SourceSlot<P, L>>,
        operation: impl Future<Output = Result<T, DispatchError>> + Send + 'static,
    ) -> Result<T, DispatchError> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let gate = source
                .pre
                .authority
                .gate
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?;
            if source.pre.authority.closed.load(Ordering::Acquire) {
                return Err(DispatchError::Stopped);
            }
            let failed = self.clone();
            let failed_source = source.clone();
            source.jobs.spawn_guarded(
                async move {
                    let _ = sender.send(operation.await);
                },
                move || failed.fail_source(&failed_source, DispatchError::RequiredStateUnavailable),
            )?;
            drop(gate);
        }
        receiver
            .await
            .map_err(|_| DispatchError::RequiredStateUnavailable)?
    }
    pub(super) fn source_quiescence(
        source: &SourceSlot<P, L>,
    ) -> Result<Arc<SourceQuiescence>, DispatchError> {
        source
            .metadata_close
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .quiescence
            .clone()
            .ok_or(DispatchError::ResourceUnconfirmed)
    }
    pub(super) fn start_source_metadata_closer(
        self: &Arc<Self>,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let core = self.clone();
        let original = source.clone();
        let failed = self.clone();
        let failed_source = source.clone();
        let task = self.tasks.spawn_guarded(
            async move {
                core.run_source_metadata_closer(original).await;
            },
            move || {
                failed.fail_source(&failed_source, DispatchError::ResourceUnconfirmed);
                failed_source
                    .metadata_close
                    .complete(Err(DispatchError::ResourceUnconfirmed));
            },
        )?;
        source
            .metadata_closer
            .set(task)
            .map_err(|_| DispatchError::InvalidSource)
    }
    async fn run_source_metadata_closer(self: &Arc<Self>, source: Arc<SourceSlot<P, L>>) {
        loop {
            let changed = source.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if source.pre.authority.closed.load(Ordering::Acquire) {
                break;
            }
            changed.await;
        }
        // No source.close/install/action/SQL lease is held while any producer is joined.
        let cleanup = loop {
            let changed = source.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(cleanup) = source.cleanup.get() {
                break cleanup.clone();
            }
            changed.await;
        };
        cleanup.join_unbounded().await;
        source.jobs.seal();
        source.jobs.join_unbounded().await;
        if let Some(pump) = source.pump.get() {
            pump.join_unbounded().await;
        }
        source.tasks.seal();
        source.tasks.join_unbounded().await;
        let quiescence = Self::cache_source_quiescence(&source, cleanup);
        if let Err(error) = quiescence {
            source.metadata_close.complete(Err(error));
            return;
        }
        loop {
            let requested = source.metadata_close.generation.load(Ordering::Acquire);
            let progress = source.progress.current();
            let result = async {
                let close = source.close.lock().await;
                let result = self.persist_reconciled_source_closed(&source).await;
                drop(close);
                result
            }
            .await;
            if result.is_err() {
                // The source SQL may already be closed while the original R3 transaction
                // still owns its pending slot. Wait on that same owner's real transition;
                // no source lock or SQL lease remains, and the token is checked again.
                let resources = source
                    .all_resources
                    .lock()
                    .map(|resources| resources.clone())
                    .map_err(|_| DispatchError::ResourceUnconfirmed);
                if let Ok(resources) = resources {
                    let mut waited = false;
                    for resource in resources {
                        if let Ok(Some(original_wait)) = resource.queue.pending_retirement()
                            && original_wait.await.is_ok()
                        {
                            waited = true;
                        }
                    }
                    if waited {
                        continue;
                    }
                }
            }
            source.metadata_close.complete(result);
            Self::notify_approval_source_change(&source);
            if result.is_ok() {
                return;
            }
            loop {
                let changed = source.metadata_close.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if source.metadata_close.generation.load(Ordering::Acquire) != requested
                    || source.progress.current() != progress
                {
                    break;
                }
                tokio::select! {()=&mut changed=>{},()=source.progress.wait_after(progress)=>{}}
            }
        }
    }
    async fn persist_reconciled_source_closed(
        &self,
        source: &Arc<SourceSlot<P, L>>,
    ) -> Result<(), DispatchError> {
        let proof = source
            .closure
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone();
        if let Some(proof) = proof {
            // The original proof already pins reconciled facts, including a possibly committed close.
            proof.quiescence.flags(source)?;
        } else {
            self.ensure_closed_approval_inventory(source)?;
            self.reconcile_source_observations(source).await?;
            self.reconcile_source_lifecycle(source).await?;
            self.reconcile_approval_requests(source).await?;
            self.reconcile_source_controls(source).await?;
            self.reconcile_source_dispatches(source).await?;
            #[cfg(windows)]
            if let Some(closer) = self.source_sql_closer.get() {
                closer
                    .reconcile_source(source.pre.metadata().instance_id)
                    .await?;
            }
        }
        self.persist_source_closed(source).await
    }
    fn cache_source_quiescence(
        source: &Arc<SourceSlot<P, L>>,
        cleanup: Arc<TaskSlot>,
    ) -> Result<(), DispatchError> {
        let quiet = Arc::new(SourceQuiescence {
            source: source.pre.clone(),
            jobs: source.jobs.quiet_slots()?,
            children: source.tasks.quiet_slots()?,
            cleanup,
        });
        quiet.flags(source)?;
        source
            .metadata_close
            .state
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .quiescence = Some(quiet);
        Ok::<(), DispatchError>(())
    }
    pub(super) fn request_source_metadata_retry(source: &SourceSlot<P, L>) {
        if let Ok(mut state) = source.metadata_close.state.lock()
            && state.completed.is_some_and(|result| result.is_err())
        {
            state.completed = None;
        }
        source
            .metadata_close
            .generation
            .fetch_add(1, Ordering::AcqRel);
        source.metadata_close.changed.notify_waiters();
    }
    pub(super) async fn wait_source_metadata_closed(
        &self,
        source: &SourceSlot<P, L>,
        deadline: Instant,
    ) -> Result<(), DispatchError> {
        tokio::time::timeout_at(deadline, async {
            loop {
                let changed = source.metadata_close.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if let Some(result) = source.metadata_close.result() {
                    return result;
                }
                changed.await;
            }
        })
        .await
        .map_err(|_| DispatchError::ResourceUnconfirmed)?
    }
}
