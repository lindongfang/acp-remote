//! Registered payload roots. Only an owned blocking worker may take the last document Arc.
#![allow(
    clippy::collapsible_if,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::redundant_pub_crate,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    document_source::DocumentError,
    queue_owner::QueueRuntimeIdentity,
    request_registry::{RegistryError, RegistryOwnerScope},
    shared_views::RetainedRaw,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::Notify;
mod task_slot;
use task_slot::TaskSlot;
fn lifecycle_lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentTaskExit {
    Completed,
    Failed(DocumentError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentTaskJoinState {
    NeverStarted,
    Pending,
    Joined(DocumentTaskExit),
    StartUnconfirmed,
}
#[derive(Clone, Copy, Debug)]
pub enum DocumentTaskPhase {
    Disposing,
}
#[derive(Clone, Debug)]
pub struct DocumentTaskIdentity(Arc<()>);
impl DocumentTaskIdentity {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn new() -> Self {
        Self(Arc::new(()))
    }
}
#[derive(Clone, Debug)]
pub struct DocumentTaskObservation {
    pub(crate) identity: DocumentTaskIdentity,
    pub(crate) phase: DocumentTaskPhase,
    pub(crate) join: DocumentTaskJoinState,
}
impl DocumentTaskObservation {
    const fn from_owner_state(
        identity: DocumentTaskIdentity,
        phase: DocumentTaskPhase,
        join: DocumentTaskJoinState,
    ) -> Self {
        Self {
            identity,
            phase,
            join,
        }
    }
}
#[derive(Clone, Debug)]
pub struct DocumentShutdownObservation {
    pub(crate) sealed: bool,
    pub(crate) active_use: usize,
    pub(crate) payload_roots: usize,
    pub(crate) registered_jobs: usize,
    pub(crate) disposal: Vec<DocumentTaskObservation>,
    pub(crate) supervisor: DocumentTaskJoinState,
}
#[derive(Clone, Debug)]
pub struct DocumentShutdownReport {
    pub(crate) observation: DocumentShutdownObservation,
}
pub trait DocumentShutdownPort: Send + Sync {
    fn seal(&self);
    fn observe(&self) -> DocumentShutdownObservation;
    fn join(&self) -> Pin<Box<dyn Future<Output = DocumentShutdownReport> + Send + '_>>;
}
/// Outer owner supplies the original registry scope and registration fence.
/// The disposal state retains neither this port nor its concrete owner.
pub trait DocumentDisposalRegistration {
    fn owner_scope(&self) -> RegistryOwnerScope;
    fn register(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn DocumentShutdownPort>,
    ) -> Result<(), RegistryError>;
}
#[derive(Default)]
pub struct DocumentShutdownSlot(Mutex<Option<Arc<dyn DocumentShutdownPort>>>);
impl DocumentShutdownSlot {
    pub(crate) fn get(&self) -> Option<Arc<dyn DocumentShutdownPort>> {
        lifecycle_lock(&self.0).clone()
    }
    pub(crate) fn install(
        &self,
        r: Arc<dyn DocumentShutdownPort>,
    ) -> Result<(), crate::domain::request_registry::RegistryError> {
        let mut slot = lifecycle_lock(&self.0);
        if slot.is_some() {
            return Err(crate::domain::request_registry::RegistryError::RuntimeAlreadyStarted);
        }
        *slot = Some(r);
        Ok(())
    }
    pub(crate) fn seal(&self) {
        if let Some(r) = self.get() {
            r.seal();
        }
    }
    pub(crate) async fn join(&self) {
        if let Some(r) = self.get() {
            let _ = r.join().await;
        }
    }
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisposalPoint {
    BlockingEntered,
    BlockingJoined,
}
#[cfg(test)]
pub(crate) type DisposalObserver = Arc<dyn Fn(DisposalPoint, DocumentTaskIdentity) + Send + Sync>;
struct PayloadCell<R> {
    value: Mutex<Option<Arc<R>>>,
}
struct JobData {
    retired: bool,
    ever_installed: bool,
    uses: usize,
    disposal: Option<Arc<TaskSlot>>,
    #[cfg(test)]
    joined_observed: bool,
}
struct Job<R> {
    payload: Arc<PayloadCell<R>>,
    data: Mutex<JobData>,
    handles: AtomicUsize,
}
trait DisposalJob: Send + Sync {
    fn data(&self) -> &Mutex<JobData>;
    fn has_payload(&self) -> bool;
    fn disposal_action(&self) -> Box<dyn FnOnce() + Send>;
}
impl<R: RetainedRaw> DisposalJob for Job<R> {
    fn data(&self) -> &Mutex<JobData> {
        &self.data
    }
    fn has_payload(&self) -> bool {
        lifecycle_lock(&self.payload.value).is_some()
    }
    fn disposal_action(&self) -> Box<dyn FnOnce() + Send> {
        let cell = self.payload.clone();
        Box::new(move || {
            let value = { lifecycle_lock(&cell.value).take() };
            drop(value);
        })
    }
}
struct Inner {
    sealed: bool,
    jobs: Vec<Arc<dyn DisposalJob>>,
}
struct State {
    inner: Mutex<Inner>,
    runtime: tokio::runtime::Handle,
    changed: Arc<Notify>,
    scope: RegistryOwnerScope,
    queue: QueueRuntimeIdentity,
    #[cfg(test)]
    observer: Mutex<Option<DisposalObserver>>,
}
struct Shutdown {
    state: Arc<State>,
    supervisor: Arc<TaskSlot>,
}
pub struct DocumentDisposalOwner<R: RetainedRaw> {
    state: Arc<State>,
    shutdown: Arc<Shutdown>,
    marker: std::marker::PhantomData<fn() -> R>,
}
impl<R: RetainedRaw> Clone for DocumentDisposalOwner<R> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            shutdown: self.shutdown.clone(),
            marker: std::marker::PhantomData,
        }
    }
}
pub struct DocumentPayloadSlot<R: RetainedRaw> {
    state: Arc<State>,
    job: Arc<Job<R>>,
}
impl<R: RetainedRaw> Clone for DocumentPayloadSlot<R> {
    fn clone(&self) -> Self {
        self.job.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            state: self.state.clone(),
            job: self.job.clone(),
        }
    }
}
impl<R: RetainedRaw> Drop for DocumentPayloadSlot<R> {
    fn drop(&mut self) {
        if self.job.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.retire();
        }
    }
}
struct CopyLease<R: RetainedRaw> {
    payload: Option<Arc<R>>,
    job: Arc<Job<R>>,
    changed: Arc<Notify>,
}
impl<R: RetainedRaw> Drop for CopyLease<R> {
    fn drop(&mut self) {
        drop(self.payload.take());
        lifecycle_lock(&self.job.data).uses -= 1;
        self.changed.notify_one();
    }
}
impl<R: RetainedRaw> DocumentDisposalOwner<R> {
    pub(crate) fn attach(
        registration: &impl DocumentDisposalRegistration,
        expected: QueueRuntimeIdentity,
    ) -> Result<Self, DocumentError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| DocumentError::StartUnconfirmed)?;
        let state = Arc::new(State {
            inner: Mutex::new(Inner {
                sealed: false,
                jobs: Vec::new(),
            }),
            runtime: runtime.clone(),
            changed: Arc::new(Notify::new()),
            scope: registration.owner_scope(),
            queue: expected.clone(),
            #[cfg(test)]
            observer: Mutex::new(None),
        });
        let supervisor = TaskSlot::new();
        let shutdown = Arc::new(Shutdown {
            state: state.clone(),
            supervisor: supervisor.clone(),
        });
        registration.register(&expected, shutdown.clone())?;
        supervisor.spawn_supervisor(&runtime, {
            let state = state.clone();
            async move {
                state.supervise().await;
            }
        });
        if !matches!(supervisor.state(), DocumentTaskJoinState::Pending) {
            state.seal();
            return Err(DocumentError::StartUnconfirmed);
        }
        Ok(Self {
            state,
            shutdown,
            marker: std::marker::PhantomData,
        })
    }
    /// Typed views share this one original registry, supervisor and shutdown receipt.
    pub(crate) fn retype<T: RetainedRaw>(&self) -> DocumentDisposalOwner<T> {
        DocumentDisposalOwner {
            state: self.state.clone(),
            shutdown: self.shutdown.clone(),
            marker: std::marker::PhantomData,
        }
    }
    pub(in crate::application) fn reserve_handoff(
        &self,
    ) -> Result<PendingDocumentHandoff<R>, DocumentError> {
        let slot = self.reserve_payload_with_uses(1)?;
        Ok(PendingDocumentHandoff {
            original: DocumentPayloadUse { slot },
        })
    }
    pub(crate) fn reserve_payload(&self) -> Result<DocumentPayloadSlot<R>, DocumentError> {
        self.reserve_payload_with_uses(0)
    }
    fn reserve_payload_with_uses(
        &self,
        uses: usize,
    ) -> Result<DocumentPayloadSlot<R>, DocumentError> {
        let mut inner = lifecycle_lock(&self.state.inner);
        if inner.sealed {
            return Err(DocumentError::Closed);
        }
        inner
            .jobs
            .try_reserve(1)
            .map_err(|_| DocumentError::Allocation)?;
        let job = Arc::new(Job {
            payload: Arc::new(PayloadCell {
                value: Mutex::new(None),
            }),
            data: Mutex::new(JobData {
                retired: false,
                ever_installed: false,
                uses,
                disposal: None,
                #[cfg(test)]
                joined_observed: false,
            }),
            handles: AtomicUsize::new(1),
        });
        inner.jobs.push(job.clone());
        drop(inner);
        self.state.changed.notify_one();
        Ok(DocumentPayloadSlot {
            state: self.state.clone(),
            job,
        })
    }
    #[cfg(test)]
    pub(crate) fn install_observer_for_test(
        &self,
        observer: DisposalObserver,
    ) -> Result<(), DocumentError> {
        let mut slot = lifecycle_lock(&self.state.observer);
        if slot.is_some() {
            return Err(crate::domain::request_registry::RegistryError::Conflict.into());
        }
        *slot = Some(observer);
        Ok(())
    }
    pub(crate) fn seal(&self) {
        self.state.seal();
    }
    pub(crate) fn receipt(&self) -> Arc<dyn DocumentShutdownPort> {
        self.shutdown.clone()
    }
}
impl<R: RetainedRaw> DocumentPayloadSlot<R> {
    pub(crate) fn verify_brand(
        &self,
        scope: &RegistryOwnerScope,
        queue: &QueueRuntimeIdentity,
    ) -> Result<(), DocumentError> {
        if !self.state.scope.matches(scope) || !self.state.queue.same(queue) {
            return Err(crate::domain::request_registry::RegistryError::InvalidActor.into());
        }
        self.state.scope.require_open()?;
        scope.require_open()?;
        Ok(())
    }
    pub(in crate::application) fn install_from_queue(
        &self,
        scope: &RegistryOwnerScope,
        queue: &QueueRuntimeIdentity,
        copy: impl FnOnce() -> R,
    ) -> Result<(), DocumentError> {
        self.verify_brand(scope, queue)?;
        let mut data = lifecycle_lock(&self.job.data);
        if data.retired {
            return Err(DocumentError::Closed);
        }
        let mut payload = lifecycle_lock(&self.job.payload.value);
        if payload.is_some() {
            return Err(crate::domain::request_registry::RegistryError::Conflict.into());
        }
        *payload = Some(Arc::new(copy()));
        data.ever_installed = true;
        drop(payload);
        drop(data);
        self.state.changed.notify_one();
        Ok(())
    }
    pub(crate) fn copy_range(
        &self,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, DocumentError> {
        if length > crate::application::shared_views::wire::MAX_PAYLOAD_BYTES {
            return Err(DocumentError::View(
                crate::domain::shared_view::ViewError::InvalidCursor,
            ));
        }
        let lease = {
            let mut data = lifecycle_lock(&self.job.data);
            if data.retired {
                return Err(DocumentError::Closed);
            }
            let payload = lifecycle_lock(&self.job.payload.value)
                .clone()
                .ok_or(DocumentError::Closed)?;
            data.uses = data.uses.checked_add(1).ok_or(DocumentError::Allocation)?;
            CopyLease {
                payload: Some(payload),
                job: self.job.clone(),
                changed: self.state.changed.clone(),
            }
        };
        let value = lease.payload.as_ref().ok_or(DocumentError::Closed)?;
        let end = offset
            .checked_add(length)
            .ok_or(DocumentError::Allocation)?;
        let bytes = value
            .bytes()
            .get(offset..end)
            .ok_or(DocumentError::Closed)?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(length)
            .map_err(|_| DocumentError::Allocation)?;
        result.extend_from_slice(bytes);
        drop(lease);
        Ok(result)
    }
    pub(crate) fn retire(&self) {
        lifecycle_lock(&self.job.data).retired = true;
        self.state.changed.notify_one();
    }
    pub(crate) fn require_live(&self) -> Result<(), DocumentError> {
        self.state.scope.require_open()?;
        if lifecycle_lock(&self.job.data).retired {
            return Err(DocumentError::Closed);
        }
        Ok(())
    }
}
impl State {
    fn seal(&self) {
        let mut inner = lifecycle_lock(&self.inner);
        inner.sealed = true;
        for job in &inner.jobs {
            lifecycle_lock(job.data()).retired = true;
        }
        drop(inner);
        self.changed.notify_one();
    }
    async fn supervise(self: Arc<Self>) {
        use futures_util::{StreamExt, stream::FuturesUnordered};
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let jobs = lifecycle_lock(&self.inner).jobs.clone();
            let mut joins = FuturesUnordered::new();
            for job in &jobs {
                let pending = {
                    let mut data = lifecycle_lock(job.data());
                    if data.retired
                        && data.uses == 0
                        && job.has_payload()
                        && data.disposal.is_none()
                    {
                        let slot = TaskSlot::new();
                        data.disposal = Some(slot.clone());
                        Some((slot, true))
                    } else {
                        data.disposal.clone().map(|slot| (slot, false))
                    }
                };
                if let Some((slot, start)) = pending {
                    if start {
                        let action = job.disposal_action();
                        #[cfg(test)]
                        let observer = lifecycle_lock(&self.observer).clone();
                        #[cfg(test)]
                        let identity = slot.observation(DocumentTaskPhase::Disposing).identity;
                        slot.spawn_blocking(&self.runtime, move || {
                            #[cfg(test)]
                            if let Some(observer) = observer {
                                observer(DisposalPoint::BlockingEntered, identity);
                            }
                            action();
                        });
                    }
                    #[cfg(test)]
                    if matches!(slot.state(), DocumentTaskJoinState::Joined(_)) {
                        let publish = {
                            let mut data = lifecycle_lock(job.data());
                            let publish = !data.joined_observed;
                            data.joined_observed = true;
                            publish
                        };
                        let observer = lifecycle_lock(&self.observer).clone();
                        if publish {
                            if let Some(observer) = observer {
                                observer(
                                    DisposalPoint::BlockingJoined,
                                    slot.observation(DocumentTaskPhase::Disposing).identity,
                                );
                            }
                        }
                    }
                    if !matches!(
                        slot.state(),
                        DocumentTaskJoinState::Joined(_) | DocumentTaskJoinState::StartUnconfirmed
                    ) {
                        joins.push(async move {
                            let _ = slot.join().await;
                        });
                    }
                }
            }
            // Only actual completed disposal or a never-installed empty reservation may leave
            // the live registry. Failed/unknown workers keep their original roots and handles.
            let removed = {
                let mut inner = lifecycle_lock(&self.inner);
                let mut removed = Vec::new();
                let mut index = 0;
                while index < inner.jobs.len() {
                    let reap = {
                        let job = &inner.jobs[index];
                        let data = lifecycle_lock(job.data());
                        data.retired
                            && data.uses == 0
                            && !job.has_payload()
                            && match &data.disposal {
                                Some(slot) => {
                                    slot.state()
                                        == DocumentTaskJoinState::Joined(
                                            DocumentTaskExit::Completed,
                                        )
                                }
                                None => !data.ever_installed,
                            }
                    };
                    if reap {
                        removed.push(inner.jobs.swap_remove(index));
                    } else {
                        index += 1;
                    }
                }
                removed
            };
            // Even the empty job/handle bookkeeping is released outside all owner mutexes.
            drop(removed);
            let done = {
                let inner = lifecycle_lock(&self.inner);
                inner.sealed
                    && inner.jobs.iter().all(|job| {
                        let data = lifecycle_lock(job.data());
                        data.retired
                            && data.uses == 0
                            && !job.has_payload()
                            && data.disposal.as_ref().is_none_or(|s| {
                                matches!(
                                    s.state(),
                                    DocumentTaskJoinState::Joined(DocumentTaskExit::Completed)
                                )
                            })
                    })
            };
            if done {
                return;
            }
            tokio::select! {()=&mut notified=>{},_ = joins.next(),if !joins.is_empty()=>{}}
        }
    }
}
impl DocumentShutdownPort for Shutdown {
    fn seal(&self) {
        self.state.seal();
    }
    fn observe(&self) -> DocumentShutdownObservation {
        let inner = lifecycle_lock(&self.state.inner);
        let mut active_use = 0;
        let mut payload_roots = 0;
        let mut disposal = Vec::new();
        for job in &inner.jobs {
            let data = lifecycle_lock(job.data());
            active_use += data.uses;
            payload_roots += usize::from(job.has_payload());
            if let Some(slot) = &data.disposal {
                disposal.push(slot.observation(DocumentTaskPhase::Disposing));
            }
        }
        DocumentShutdownObservation {
            sealed: inner.sealed,
            registered_jobs: inner.jobs.len(),
            active_use,
            payload_roots,
            disposal,
            supervisor: self.supervisor.state(),
        }
    }
    fn join(&self) -> Pin<Box<dyn Future<Output = DocumentShutdownReport> + Send + '_>> {
        Box::pin(async move {
            self.seal();
            let joined = self.supervisor.join().await;
            if joined != DocumentTaskJoinState::Joined(DocumentTaskExit::Completed) {
                return std::future::pending().await;
            }
            DocumentShutdownReport {
                observation: self.observe(),
            }
        })
    }
}

/// A request's pre-registered right to adopt its one actual response, even after seal.
/// Neither pending nor installed handoffs can be cloned or installed twice.
pub(in crate::application) struct PendingDocumentHandoff<R: RetainedRaw> {
    original: DocumentPayloadUse<R>,
}
pub(in crate::application) struct InstalledDocumentHandoff<R: RetainedRaw> {
    original: DocumentPayloadUse<R>,
}
struct DocumentPayloadUse<R: RetainedRaw> {
    slot: DocumentPayloadSlot<R>,
}
impl<R: RetainedRaw> Drop for DocumentPayloadUse<R> {
    fn drop(&mut self) {
        {
            let mut data = lifecycle_lock(&self.slot.job.data);
            data.uses -= 1;
        }
        self.slot.state.changed.notify_one();
    }
}
impl<R: RetainedRaw> PendingDocumentHandoff<R> {
    pub(in crate::application) fn install(self, value: R) -> InstalledDocumentHandoff<R> {
        let slot = &self.original.slot;
        // This empty slot has not been exposed: the linear pending value is its only installer.
        let mut data = lifecycle_lock(&slot.job.data);
        let mut payload = lifecycle_lock(&slot.job.payload.value);
        *payload = Some(Arc::new(value));
        data.ever_installed = true;
        drop(payload);
        drop(data);
        slot.state.changed.notify_one();
        InstalledDocumentHandoff {
            original: self.original,
        }
    }
}
impl<R: RetainedRaw> InstalledDocumentHandoff<R> {
    pub(in crate::application) fn payload_slot(&self) -> DocumentPayloadSlot<R> {
        self.original.slot.clone()
    }
}
