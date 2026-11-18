//! Read-only original-operation progress and one linear handoff to its actual candidate.
use super::{
    AcceptedIntent, Arc, CurrentViewAuthority, OpenedView, ReadHandle, ReadRebind,
    ReadRebindMetadata, RebindClaim, RebindStage, RetainedRaw, TargetGeneration, TargetSnapshot,
    ViewError, ViewReadService,
};
use crate::{
    application::shared_views::{
        local_authority::{AuthenticatedLocalViewClient, LocalViewAuthority},
        send_epoch::OwnedDisplaySendGroup,
        source::TargetPublication,
    },
    domain::{
        EntityId,
        shared_view::{ReadEpoch, ReadMode, ViewEpoch},
    },
};
use std::sync::Weak;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalReadProgress {
    Pending,
    CandidateReady { read: ReadEpoch, epoch: ViewEpoch },
    Complete { read: ReadEpoch, epoch: ViewEpoch },
    Failed(ViewError),
}
#[derive(Clone)]
pub struct OriginalReadObserver {
    handle: Weak<ReadHandle>,
}
// References sampled under authentication remain here until that guard has ended.
// In particular a concurrent source retirement cannot make a large destructor run in it.
struct ObservedReferences<R> {
    publication: Option<Arc<TargetPublication<R>>>,
    generation: Option<Arc<TargetGeneration<R>>>,
    snapshot: Option<Arc<TargetSnapshot<R>>>,
}
impl<R> ObservedReferences<R> {
    const fn new() -> Self {
        Self {
            publication: None,
            generation: None,
            snapshot: None,
        }
    }
}
impl<S, R, A, C> ViewReadService<S, R, A, C>
where
    S: Clone + Eq + Send + Sync + 'static,
    R: RetainedRaw,
    A: CurrentViewAuthority<C>,
    C: Send + Sync + 'static,
{
    fn check_original_handle(&self, client: &C, handle: &ReadHandle) -> Result<(), ViewError> {
        if !Arc::ptr_eq(&self.identity, &handle.service)
            || !handle
                .binding
                .same_client(&self.authority.client_binding(client)?)
        {
            return Err(ViewError::InvalidClient);
        }
        if *handle.close.borrow() || handle.stream.terminally_sealed()? {
            return Err(ViewError::StreamSealed);
        }
        if self.views.core.probe.is_sealed() {
            return Err(ViewError::SourceReleased);
        }
        let source = self
            .views
            .core
            .send
            .inner
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if !handle.stream.target_generation_current_in(&source)
            || !OwnedDisplaySendGroup::target_live_in(&source, &handle.target)
        {
            return Err(ViewError::SourceReleased);
        }
        drop(source);
        Ok(())
    }
    pub(crate) fn original_observer(
        &self,
        read: &OpenedView,
    ) -> Result<OriginalReadObserver, ViewError> {
        if !Arc::ptr_eq(&self.identity, &read.handle.service) {
            return Err(ViewError::InvalidClient);
        }
        Ok(OriginalReadObserver {
            handle: Arc::downgrade(&read.handle),
        })
    }
    fn observe_original_handle(
        &self,
        client: &C,
        handle: &ReadHandle,
        retained: &mut ObservedReferences<R>,
    ) -> Result<OriginalReadProgress, ViewError> {
        self.check_original_handle(client, handle)?;
        let request = handle
            .intent
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .as_ref()
            .map(|intent| Arc::clone(&intent.request))
            .ok_or(ViewError::InvalidStage)?;
        let status = request.subscribe().borrow().clone();
        if status.operation_id() != request.pinned_operation() {
            return Err(ViewError::InvalidStage);
        }
        if status.read_mode() == ReadMode::ReplayIncomplete {
            return Ok(OriginalReadProgress::Failed(
                status.failure().unwrap_or(ViewError::ReadFailure),
            ));
        }
        let Some(read) = status.read_epoch() else {
            return Ok(OriginalReadProgress::Pending);
        };
        retained.publication = self
            .views
            .core
            .target_publication(&handle.target)?
            .upgrade();
        let publication = retained
            .publication
            .as_ref()
            .ok_or(ViewError::SourceReleased)?;
        let failure = *publication
            .candidate_failure
            .read()
            .map_err(|_| ViewError::ReadFailure)?;
        if let Some((failed_read, error)) = failure
            && failed_read == read
        {
            return Ok(OriginalReadProgress::Failed(error));
        }
        retained.generation = match status.read_mode() {
            ReadMode::Replaying => publication
                .candidate
                .read()
                .map_err(|_| ViewError::ReadFailure)?
                .upgrade(),
            ReadMode::ReplayComplete => Some(Arc::clone(
                &*publication
                    .current
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?,
            )),
            _ => return Ok(OriginalReadProgress::Pending),
        };
        let Some(generation) = retained.generation.as_ref() else {
            return Ok(OriginalReadProgress::Pending);
        };
        retained.snapshot = Some(Arc::clone(
            &*generation
                .snapshot
                .read()
                .map_err(|_| ViewError::ReadFailure)?,
        ));
        let descriptor = retained
            .snapshot
            .as_ref()
            .ok_or(ViewError::ReadFailure)?
            .descriptor;
        if descriptor.source_stamp() != handle.descriptor.source_stamp()
            || descriptor.read_epoch() != Some(read)
        {
            return Ok(OriginalReadProgress::Pending);
        }
        if status.read_mode() == ReadMode::ReplayComplete
            && descriptor.read_mode() != ReadMode::ReplayComplete
        {
            return Ok(OriginalReadProgress::Pending);
        }
        if !generation.send.current()? {
            return Err(ViewError::StaleCursor);
        }
        self.check_original_handle(client, handle)?;
        Ok(match (status.read_mode(), descriptor.read_mode()) {
            (ReadMode::Replaying, ReadMode::Replaying) => OriginalReadProgress::CandidateReady {
                read,
                epoch: descriptor.view_epoch(),
            },
            (ReadMode::ReplayComplete, ReadMode::ReplayComplete) => {
                OriginalReadProgress::Complete {
                    read,
                    epoch: descriptor.view_epoch(),
                }
            }
            _ => OriginalReadProgress::Pending,
        })
    }
    pub(crate) async fn original_progress(
        &self,
        client: &C,
        previous: &OpenedView,
    ) -> Result<OriginalReadProgress, ViewError> {
        let mut retained = ObservedReferences::new();
        let result = self
            .authority
            .with_current(client, &previous.handle.target, || {
                self.observe_original_handle(client, &previous.handle, &mut retained)
            })
            .await;
        drop(retained);
        result
    }
    pub(crate) async fn wait_original_progress(
        &self,
        client: &C,
        previous: &OpenedView,
    ) -> Result<OriginalReadProgress, ViewError> {
        let mut native = previous
            .handle
            .intent
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .as_ref()
            .ok_or(ViewError::InvalidStage)?
            .request
            .subscribe();
        loop {
            let changed = self.views.core.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let progress = self.original_progress(client, previous).await?;
            if progress != OriginalReadProgress::Pending {
                return Ok(progress);
            }
            tokio::select! {
                ()=changed=>{},
                result=native.changed()=>result.map_err(|_|ViewError::SourceReleased)?,
                result=self.authority.wait_client_change(client)=>result?,
            }
        }
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the original nonce claim, cancellation guard and final same-generation publication fence visible together; all waits are outside short locks"
    )]
    pub(crate) async fn rebind_inflight_same_read(
        &self,
        client: &C,
        previous: &OpenedView,
    ) -> Result<ReadRebind, ViewError> {
        let nonce =
            EntityId::from_bytes(crate::random::bytes().map_err(|_| ViewError::RandomUnavailable)?);
        let mut candidate = None;
        let (request, read, epoch) = self
            .authority
            .with_current(client, &previous.handle.target, || {
                self.check_original_handle(client, &previous.handle)?;
                let mut intent = previous
                    .handle
                    .intent
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                let intent = intent.as_mut().ok_or(ViewError::InvalidStage)?;
                if !matches!(intent.stage, RebindStage::Available) {
                    return Err(ViewError::InvalidStage);
                }
                let request = Arc::clone(&intent.request);
                let status = request.subscribe().borrow().clone();
                if status.operation_id() != request.pinned_operation()
                    || status.read_mode() != ReadMode::Replaying
                {
                    return Err(ViewError::InvalidStage);
                }
                let read = status.read_epoch().ok_or(ViewError::InvalidStage)?;
                candidate = Some(
                    self.views
                        .core
                        .candidate_for_read(&previous.handle.target, read)?,
                );
                let descriptor = candidate
                    .as_ref()
                    .ok_or(ViewError::ReadFailure)?
                    .snapshot
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?
                    .descriptor;
                if descriptor.source_stamp() != previous.handle.descriptor.source_stamp()
                    || descriptor.view_epoch() == previous.handle.descriptor.view_epoch()
                {
                    return Err(ViewError::StaleCursor);
                }
                intent.stage = RebindStage::Preparing(nonce);
                Ok((request, read, descriptor.view_epoch()))
            })
            .await?;
        let mut claim = RebindClaim {
            handle: Arc::clone(&previous.handle),
            nonce,
            finished: false,
        };
        let candidate = candidate.ok_or(ViewError::ReadFailure)?;
        let new = self
            .open_selected(
                client,
                &previous.handle.target,
                Some(Arc::clone(&candidate)),
            )
            .await?;
        let publication = self
            .views
            .core
            .target_publication(&previous.handle.target)?
            .upgrade()
            .ok_or(ViewError::SourceReleased)?;
        let mut actual = None;
        self.authority
            .with_current(client, &previous.handle.target, || {
                let source = self
                    .views
                    .core
                    .send
                    .inner
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                if self.views.core.probe.is_sealed()
                    || !previous.handle.stream.target_generation_current_in(&source)
                    || !OwnedDisplaySendGroup::target_live_in(&source, &previous.handle.target)
                {
                    return Err(ViewError::SourceReleased);
                }
                if *previous.handle.close.borrow() || previous.handle.stream.terminally_sealed()? {
                    return Err(ViewError::StreamSealed);
                }
                let mut intent = previous
                    .handle
                    .intent
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                let intent = intent.as_mut().ok_or(ViewError::InvalidStage)?;
                if !matches!(intent.stage,RebindStage::Preparing(value) if value==nonce)
                    || !Arc::ptr_eq(&request, &intent.request)
                {
                    return Err(ViewError::InvalidStage);
                }
                let status = request.subscribe().borrow().clone();
                actual = publication
                    .candidate
                    .read()
                    .map_err(|_| ViewError::ReadFailure)?
                    .upgrade();
                if status.operation_id() != request.pinned_operation()
                    || status.read_epoch() != Some(read)
                    || status.read_mode() != ReadMode::Replaying
                    || !actual
                        .as_ref()
                        .is_some_and(|actual| Arc::ptr_eq(actual, &candidate))
                    || !candidate.send.current_in(&source)
                    || new.handle.descriptor.view_epoch() != epoch
                {
                    return Err(ViewError::StaleCursor);
                }
                let mut next_intent = new
                    .handle
                    .intent
                    .lock()
                    .map_err(|_| ViewError::ReadFailure)?;
                *next_intent = Some(AcceptedIntent {
                    request: Arc::clone(&request),
                    stage: RebindStage::Consumed,
                });
                intent.stage = RebindStage::Consumed;
                Ok(())
            })
            .await?;
        claim.finished = true;
        let metadata = ReadRebindMetadata {
            previous_stream: previous.stream_id(),
            stream: new.stream_id(),
            epoch,
            read,
            revision: new.handle.descriptor.revision(),
            source: new.handle.descriptor.source_stamp(),
        };
        Ok(ReadRebind {
            read: new,
            metadata,
        })
    }
}
impl<S, R> ViewReadService<S, R, LocalViewAuthority, AuthenticatedLocalViewClient>
where
    S: Clone + Eq + Send + Sync + 'static,
    R: RetainedRaw,
{
    pub(crate) async fn try_observe_original(
        &self,
        client: &AuthenticatedLocalViewClient,
        observer: &OriginalReadObserver,
    ) -> Result<Option<OriginalReadProgress>, ViewError> {
        let handle = observer.handle.upgrade().ok_or(ViewError::StreamSealed)?;
        if !Arc::ptr_eq(&self.identity, &handle.service) {
            return Err(ViewError::InvalidClient);
        }
        let binding = self.authority.client_binding(client)?;
        if !handle.binding.same_client(&binding) {
            return Err(ViewError::InvalidClient);
        }
        let Some(guard) = self
            .authority
            .try_current_guard(client, &handle.target)
            .await?
        else {
            return Ok(None);
        };
        if !guard.matches(&binding, &handle.target) {
            return Err(ViewError::InvalidClient);
        }
        let mut retained = ObservedReferences::new();
        let result = self.authority.with_live_issuer(client, || {
            self.observe_original_handle(client, &handle, &mut retained)
        });
        drop(guard);
        drop(retained);
        result.map(Some)
    }
}
