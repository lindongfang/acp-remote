//! One handoff owns the display attachment; it never owns its native source or runtime.
use super::{Arc, BoxFuture, DispatchError, Ordering, PreSourceProbe};
use crate::domain::shared_view::ViewError;
use std::sync::{Mutex, MutexGuard};
use tokio::sync::Notify;

pub struct ViewAttachmentClose {
    pub(crate) workers: usize,
    pub(crate) readers: usize,
    pub(crate) cleanup: usize,
    pub(crate) native_reads: usize,
    pub(crate) failure: Option<ViewError>,
}
impl ViewAttachmentClose {
    const fn all_joined(&self) -> bool {
        self.workers == 0 && self.readers == 0 && self.cleanup == 0 && self.native_reads == 0
    }
}
pub trait NativeSourceViewOwner: Send + Sync + 'static {
    fn seal(&self) -> Result<(), ViewError>;
    fn seal_target(&self, target: &super::NativeSessionKey) -> Result<(), ViewError>;
    fn join_stopped(&self) -> BoxFuture<'_, ViewAttachmentClose>;
}
pub trait NativeSourceViewFactory<P: super::NativeApprovalPort>: Send + Sync + 'static {
    fn attach(
        &self,
        context: super::read_access::ViewSourceContext<P>,
        guard: AttachmentBuildGuard,
    ) -> Result<(), ViewError>;
}
enum AttachmentState {
    Vacant,
    Disabled(Option<ViewError>),
    Attaching(Option<Arc<dyn NativeSourceViewOwner>>),
    Ready(Arc<dyn NativeSourceViewOwner>),
    UnavailablePartial(Arc<dyn NativeSourceViewOwner>, ViewError),
    Taken,
    Closed(ViewAttachmentClose),
    NotInstalled,
}
pub(super) struct ViewAttachmentHandoff {
    state: Mutex<AttachmentState>,
    changed: Arc<Notify>,
}
pub struct AttachmentBuildGuard {
    handoff: Arc<ViewAttachmentHandoff>,
    source: Arc<PreSourceProbe>,
    finished: bool,
}
impl ViewAttachmentHandoff {
    pub(super) const fn new(changed: Arc<Notify>) -> Self {
        Self {
            state: Mutex::new(AttachmentState::Vacant),
            changed,
        }
    }
    // Teardown recovers ownership, never a read or native-write grant.
    fn teardown(&self) -> MutexGuard<'_, AttachmentState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        }
    }
    pub(super) fn disable(&self, error: Option<ViewError>) {
        let mut state = self.teardown();
        if matches!(*state, AttachmentState::Vacant) {
            *state = AttachmentState::Disabled(error);
        }
        drop(state);
        self.changed.notify_waiters();
    }
    pub(super) fn note_failure(&self, error: ViewError) {
        match &mut *self.teardown() {
            AttachmentState::Disabled(reason) => *reason = Some(error),
            AttachmentState::UnavailablePartial(_, reason) => *reason = error,
            _ => {}
        }
        self.changed.notify_waiters();
    }
    pub(super) fn unavailable_reason(&self) -> Option<ViewError> {
        match &*self.teardown() {
            AttachmentState::Disabled(reason) => {
                Some(reason.unwrap_or(ViewError::CaptureUnavailable))
            }
            AttachmentState::UnavailablePartial(_, reason) => Some(*reason),
            AttachmentState::Closed(report) => report.failure,
            AttachmentState::NotInstalled => Some(ViewError::SourceReleased),
            _ => None,
        }
    }
    pub(super) fn begin(
        self: &Arc<Self>,
        source: Arc<PreSourceProbe>,
    ) -> Result<AttachmentBuildGuard, ViewError> {
        {
            let _gate = source
                .authority
                .gate
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            if source.authority.closed.load(Ordering::Acquire)
                || source.authority.closing.load(Ordering::Acquire)
            {
                return Err(ViewError::SourceReleased);
            }
            let mut state = self.state.lock().map_err(|_| ViewError::ReadFailure)?;
            if !matches!(*state, AttachmentState::Vacant) {
                return Err(ViewError::CaptureUnavailable);
            }
            *state = AttachmentState::Attaching(None);
        }
        Ok(AttachmentBuildGuard {
            handoff: Arc::clone(self),
            source,
            finished: false,
        })
    }
    pub(super) fn seal(&self) {
        let owner = {
            let state = self.teardown();
            match &*state {
                AttachmentState::Attaching(owner) => owner.clone(),
                AttachmentState::Ready(owner) | AttachmentState::UnavailablePartial(owner, _) => {
                    Some(Arc::clone(owner))
                }
                _ => None,
            }
        };
        if let Some(owner) = owner {
            let _ = owner.seal();
        }
        self.changed.notify_waiters();
    }
    pub(super) fn seal_target(&self, target: &super::NativeSessionKey) {
        let owner = {
            let state = self.teardown();
            match &*state {
                AttachmentState::Attaching(owner) => owner.clone(),
                AttachmentState::Ready(owner) | AttachmentState::UnavailablePartial(owner, _) => {
                    Some(Arc::clone(owner))
                }
                _ => None,
            }
        };
        if let Some(owner) = owner {
            let _ = owner.seal_target(target);
        }
        self.changed.notify_waiters();
    }
    pub(super) fn joined_or_not_installed(&self) -> bool {
        match &*self.teardown() {
            AttachmentState::Closed(report) => report.all_joined(),
            AttachmentState::NotInstalled | AttachmentState::Disabled(_) => true,
            _ => false,
        }
    }
    pub(super) async fn close_after_source(self: Arc<Self>, source: Arc<PreSourceProbe>) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let owner = if source.authority.closed.load(Ordering::Acquire) {
                let mut state = self.teardown();
                match &*state {
                    AttachmentState::Vacant | AttachmentState::Disabled(_) => {
                        *state = AttachmentState::NotInstalled;
                        return;
                    }
                    AttachmentState::Ready(owner)
                    | AttachmentState::UnavailablePartial(owner, _) => {
                        let owner = Arc::clone(owner);
                        *state = AttachmentState::Taken;
                        drop(state);
                        Some(owner)
                    }
                    AttachmentState::Closed(_) | AttachmentState::NotInstalled => return,
                    AttachmentState::Attaching(_) | AttachmentState::Taken => None,
                }
            } else {
                None
            };
            if let Some(owner) = owner {
                // The guard hands the original owner back on panic; a deadline cancels
                // only a caller's wait on the separately registered closer task.
                let mut taken = TakenAttachment {
                    handoff: Arc::clone(&self),
                    owner: Some(owner),
                };
                if let Some(owner) = taken.owner.as_ref() {
                    let _ = owner.seal();
                    let report = owner.join_stopped().await;
                    if report.all_joined() {
                        *self.teardown() = AttachmentState::Closed(report);
                        taken.owner.take();
                    } else {
                        // Retain the actual unjoined owner in the source inventory.
                        // This task ending does not establish attachment quiescence.
                        let error = report.failure.unwrap_or(ViewError::ShutdownUnconfirmed);
                        if let Some(owner) = taken.owner.take() {
                            *self.teardown() = AttachmentState::UnavailablePartial(owner, error);
                        }
                    }
                }
                self.changed.notify_waiters();
                return;
            }
            changed.await;
        }
    }
}
impl AttachmentBuildGuard {
    /// Called before the first attachment child is started.
    pub(crate) fn adopt_partial(
        &self,
        owner: Arc<dyn NativeSourceViewOwner>,
    ) -> Result<(), ViewError> {
        let mut state = self
            .handoff
            .state
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        match &mut *state {
            AttachmentState::Attaching(partial) if partial.is_none() => *partial = Some(owner),
            _ => return Err(ViewError::InvalidStage),
        }
        drop(state);
        Ok(())
    }
    pub(crate) fn publish(mut self) -> Result<(), ViewError> {
        let owner = {
            let mut state = self
                .handoff
                .state
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            let AttachmentState::Attaching(Some(owner)) = &*state else {
                return Err(ViewError::InvalidStage);
            };
            let owner = Arc::clone(owner);
            *state = AttachmentState::Ready(Arc::clone(&owner));
            owner
        };
        self.finished = true;
        if self.source.authority.closed.load(Ordering::Acquire)
            || self.source.authority.closing.load(Ordering::Acquire)
        {
            let _ = owner.seal();
        }
        self.handoff.changed.notify_waiters();
        Ok(())
    }
}
impl Drop for AttachmentBuildGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let owner = {
            let mut state = self.handoff.teardown();
            let AttachmentState::Attaching(partial) = &mut *state else {
                return;
            };
            let owner = partial.take();
            *state = owner.as_ref().map_or_else(
                || AttachmentState::Disabled(Some(ViewError::CaptureUnavailable)),
                |owner| {
                    AttachmentState::UnavailablePartial(
                        Arc::clone(owner),
                        ViewError::CaptureUnavailable,
                    )
                },
            );
            owner
        };
        if (self.source.authority.closed.load(Ordering::Acquire)
            || self.source.authority.closing.load(Ordering::Acquire))
            && let Some(owner) = owner
        {
            let _ = owner.seal();
        }
        self.handoff.changed.notify_waiters();
    }
}
struct TakenAttachment {
    handoff: Arc<ViewAttachmentHandoff>,
    owner: Option<Arc<dyn NativeSourceViewOwner>>,
}
impl Drop for TakenAttachment {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.take() {
            *self.handoff.teardown() =
                AttachmentState::UnavailablePartial(owner, ViewError::ShutdownUnconfirmed);
            self.handoff.changed.notify_waiters();
        }
    }
}

pub(super) fn register_closer(
    tasks: &super::tasks::TaskRegistry,
    handoff: &Arc<ViewAttachmentHandoff>,
    source: &Arc<PreSourceProbe>,
) -> Result<(), DispatchError> {
    let closing = Arc::clone(handoff);
    let original = Arc::clone(source);
    tasks.spawn_guarded(
        async move {
            closing.close_after_source(original).await;
        },
        || {},
    )?;
    Ok(())
}
