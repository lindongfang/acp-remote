//! Owned catalog buffers and workers, registered before launch. No outer concrete owner is retained.
#![allow(
    clippy::needless_pass_by_value,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    queue_owner::QueueRuntimeIdentity,
    request_registry::{RegistryError, RegistryOwnerScope},
};
use crate::domain::native_catalog::{
    CatalogDraft, CatalogError, CatalogFact, CatalogFailure, CatalogReceipt, CatalogRequestId,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::Notify;
mod task_slot;
use task_slot::TaskSlot;
fn lifecycle_lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogBufferError {
    InvalidState,
    Conflict,
    BodyUnavailable,
    Allocation,
    NodeStopping,
    StorageUnavailable,
    InvalidDocument,
}
#[derive(Clone, Debug)]
pub enum CatalogPreparationFailure {
    Upload(CatalogBufferError),
    Catalog(CatalogFailure),
    WorkerUnconfirmed(CatalogTaskObservation),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogTaskExit {
    Completed,
    Failed(CatalogError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogTaskJoinState {
    NeverStarted,
    Pending,
    Joined(CatalogTaskExit),
    StartUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogTaskPhase {
    Append,
    Prepare,
    Register,
    Apply,
    Query,
    Dispose,
    Supervise,
}
#[derive(Clone, Debug)]
pub struct CatalogTaskIdentity(Arc<()>);
impl CatalogTaskIdentity {
    fn new() -> Self {
        Self(Arc::new(()))
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone, Debug)]
pub struct CatalogTaskObservation {
    pub(crate) identity: CatalogTaskIdentity,
    pub(crate) phase: CatalogTaskPhase,
    pub(crate) join: CatalogTaskJoinState,
}
impl CatalogTaskObservation {
    const fn from_owner_state(
        identity: CatalogTaskIdentity,
        phase: CatalogTaskPhase,
        join: CatalogTaskJoinState,
    ) -> Self {
        Self {
            identity,
            phase,
            join,
        }
    }
}
#[derive(Clone, Debug)]
pub struct CatalogShutdownObservation {
    pub(crate) sealed: bool,
    pub(crate) payload_roots: usize,
    pub(crate) active_use: usize,
    pub(crate) registered_jobs: usize,
    pub(crate) workers: Vec<CatalogTaskObservation>,
    pub(crate) supervisor: CatalogTaskJoinState,
}
#[derive(Clone, Debug)]
pub struct CatalogShutdownReport {
    pub(crate) observation: CatalogShutdownObservation,
}
pub trait CatalogUploadShutdownPort: Send + Sync {
    fn seal(&self);
    fn observe(&self) -> CatalogShutdownObservation;
    fn join(&self) -> Pin<Box<dyn Future<Output = CatalogShutdownReport> + Send + '_>>;
}
pub trait CatalogPreparationRegistration {
    fn owner_scope(&self) -> RegistryOwnerScope;
    fn register(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn CatalogUploadShutdownPort>,
    ) -> Result<(), RegistryError>;
}
#[derive(Default)]
pub struct CatalogShutdownSlot(Mutex<Option<Arc<dyn CatalogUploadShutdownPort>>>);
impl CatalogShutdownSlot {
    pub(crate) fn get(&self) -> Option<Arc<dyn CatalogUploadShutdownPort>> {
        lifecycle_lock(&self.0).clone()
    }
    pub(crate) fn install(
        &self,
        r: Arc<dyn CatalogUploadShutdownPort>,
    ) -> Result<(), RegistryError> {
        let mut slot = lifecycle_lock(&self.0);
        if slot.is_some() {
            return Err(RegistryError::RuntimeAlreadyStarted);
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
#[derive(Clone)]
pub enum CatalogJobResult {
    Append(Result<u64, CatalogBufferError>),
    Prepared(Result<(), CatalogPreparationFailure>),
    Registered(Result<CatalogRequestId, CatalogFailure>),
    Applied(Result<CatalogReceipt, CatalogFailure>),
    Queried(Result<CatalogFact, CatalogFailure>),
    Disposed,
    WorkerUnconfirmed(CatalogTaskObservation),
}
type WorkCell =
    Arc<Mutex<Option<Box<dyn FnOnce(tokio::runtime::Handle) -> CatalogJobResult + Send>>>>;
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogWorkerPoint {
    Entered,
    Joined,
}
#[cfg(test)]
pub type CatalogWorkerObserver =
    Arc<dyn Fn(CatalogWorkerPoint, CatalogTaskPhase, CatalogTaskIdentity) + Send + Sync>;
struct Job {
    slot: Arc<TaskSlot>,
    phase: CatalogTaskPhase,
    result: Arc<Mutex<Option<CatalogJobResult>>>,
    _work: Option<WorkCell>,
    #[cfg(test)]
    joined_observed: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct CatalogWorkReceipt {
    job: Arc<Job>,
    changed: Arc<Notify>,
}
impl CatalogWorkReceipt {
    pub(crate) fn observation(&self) -> CatalogTaskObservation {
        self.job.slot.observation(self.job.phase)
    }
    pub(crate) fn original_result(&self) -> Option<CatalogJobResult> {
        lifecycle_lock(&self.job.result).clone()
    }
    pub(crate) fn try_result(&self) -> Option<CatalogJobResult> {
        match self.job.slot.state() {
            CatalogTaskJoinState::Joined(CatalogTaskExit::Completed) => self.original_result(),
            CatalogTaskJoinState::Joined(CatalogTaskExit::Failed(_))
            | CatalogTaskJoinState::StartUnconfirmed => {
                Some(CatalogJobResult::WorkerUnconfirmed(self.observation()))
            }
            _ => None,
        }
    }
    pub(crate) async fn wait(&self) -> CatalogJobResult {
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            let _ = self.job.slot.join().await;
            self.changed.notify_one();
            if let Some(r) = self.try_result() {
                return r;
            }
            n.await;
        }
    }
}
/// The owner already holds the original slot even if this launch guard is cancelled.
pub struct CatalogWork {
    pub(crate) receipt: CatalogWorkReceipt,
    launch: Option<Box<dyn FnOnce() + Send>>,
}
impl CatalogWork {
    pub(crate) fn start(mut self) -> CatalogWorkReceipt {
        if let Some(f) = self.launch.take() {
            f();
        }
        self.receipt.clone()
    }
}
impl Drop for CatalogWork {
    fn drop(&mut self) {
        if let Some(f) = self.launch.take() {
            f();
        }
    }
}
enum Payload {
    Bytes(Vec<u8>),
    Draft(CatalogDraft),
}
struct PayloadCell {
    value: Mutex<Option<Payload>>,
    retired: AtomicBool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogBufferPhase {
    Receiving,
    Preparing,
    Ready,
    Failed,
    Claimed,
    Released,
}
#[derive(Clone)]
pub struct CatalogBufferStatus {
    pub(crate) phase: CatalogBufferPhase,
    pub(crate) next_offset: u64,
    pub(crate) error: Option<CatalogPreparationFailure>,
}
struct BufferData {
    phase: CatalogBufferPhase,
    next: u64,
    previous: Option<(u64, usize)>,
    inflight: Option<(u64, Vec<u8>)>,
    job: Option<CatalogWorkReceipt>,
    registered: Option<CatalogWorkReceipt>,
    error: Option<CatalogPreparationFailure>,
}
struct Buffer {
    payload: Arc<PayloadCell>,
    data: Mutex<BufferData>,
    handles: AtomicUsize,
}
struct Inner {
    sealed: bool,
    jobs: Vec<Arc<Job>>,
    buffers: Vec<Arc<Buffer>>,
}
struct State {
    inner: Mutex<Inner>,
    runtime: tokio::runtime::Handle,
    scope: RegistryOwnerScope,
    changed: Arc<Notify>,
    #[cfg(test)]
    observer: Mutex<Option<CatalogWorkerObserver>>,
}
struct Shutdown {
    state: Arc<State>,
    supervisor: Arc<TaskSlot>,
}
#[derive(Clone)]
pub struct CatalogPreparationOwner {
    state: Arc<State>,
    shutdown: Arc<Shutdown>,
}
pub struct CatalogBufferSlot {
    state: Arc<State>,
    buffer: Arc<Buffer>,
}
impl Clone for CatalogBufferSlot {
    fn clone(&self) -> Self {
        self.buffer.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            state: self.state.clone(),
            buffer: self.buffer.clone(),
        }
    }
}
impl Drop for CatalogBufferSlot {
    fn drop(&mut self) {
        if self.buffer.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.retire();
        }
    }
}
impl CatalogPreparationOwner {
    pub(crate) fn attach(
        port: &impl CatalogPreparationRegistration,
        expected: QueueRuntimeIdentity,
    ) -> Result<Self, RegistryError> {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| RegistryError::StorageUnavailable)?;
        let state = Arc::new(State {
            inner: Mutex::new(Inner {
                sealed: false,
                jobs: Vec::new(),
                buffers: Vec::new(),
            }),
            runtime: runtime.clone(),
            scope: port.owner_scope(),
            changed: Arc::new(Notify::new()),
            #[cfg(test)]
            observer: Mutex::new(None),
        });
        let supervisor = TaskSlot::new();
        let shutdown = Arc::new(Shutdown {
            state: state.clone(),
            supervisor: supervisor.clone(),
        });
        port.register(&expected, shutdown.clone())?;
        supervisor.spawn_supervisor(&runtime, {
            let state = state.clone();
            async move { state.supervise().await }
        });
        if supervisor.state() != CatalogTaskJoinState::Pending {
            state.seal();
            return Err(RegistryError::StorageUnavailable);
        }
        Ok(Self { state, shutdown })
    }
    #[cfg(test)]
    pub(crate) fn install_observer_for_test(
        &self,
        observer: CatalogWorkerObserver,
    ) -> Result<(), CatalogBufferError> {
        let mut slot = lifecycle_lock(&self.state.observer);
        if slot.is_some() {
            return Err(CatalogBufferError::Conflict);
        }
        *slot = Some(observer);
        Ok(())
    }
    pub(crate) fn reserve_buffer(&self) -> Result<CatalogBufferSlot, CatalogBufferError> {
        self.state
            .scope
            .require_open()
            .map_err(|_| CatalogBufferError::NodeStopping)?;
        let mut inner = lifecycle_lock(&self.state.inner);
        if inner.sealed {
            return Err(CatalogBufferError::NodeStopping);
        }
        inner
            .buffers
            .try_reserve(1)
            .map_err(|_| CatalogBufferError::Allocation)?;
        let buffer = Arc::new(Buffer {
            payload: Arc::new(PayloadCell {
                value: Mutex::new(Some(Payload::Bytes(Vec::new()))),
                retired: AtomicBool::new(false),
            }),
            data: Mutex::new(BufferData {
                phase: CatalogBufferPhase::Receiving,
                next: 0,
                previous: None,
                inflight: None,
                job: None,
                registered: None,
                error: None,
            }),
            handles: AtomicUsize::new(1),
        });
        inner.buffers.push(buffer.clone());
        Ok(CatalogBufferSlot {
            state: self.state.clone(),
            buffer,
        })
    }
    pub(crate) fn job(
        &self,
        phase: CatalogTaskPhase,
        work: impl FnOnce(tokio::runtime::Handle) -> CatalogJobResult + Send + 'static,
    ) -> Result<CatalogWork, CatalogBufferError> {
        self.state.job(phase, work)
    }
    pub(crate) fn seal(&self) {
        self.state.seal();
    }
    pub(crate) fn receipt(&self) -> Arc<dyn CatalogUploadShutdownPort> {
        self.shutdown.clone()
    }
}
impl Buffer {
    fn refresh(&self) {
        let mut d = lifecycle_lock(&self.data);
        let result = d.job.as_ref().and_then(CatalogWorkReceipt::try_result);
        let Some(result) = result else { return };
        match result {
            CatalogJobResult::Append(r) => {
                if let Ok(next) = r {
                    d.next = next;
                    if let Some((offset, bytes)) = d.inflight.take() {
                        d.previous = Some((offset, bytes.len()));
                    }
                } else {
                    d.inflight = None;
                }
                d.job = None;
            }
            CatalogJobResult::Prepared(r) => {
                match r {
                    Ok(()) => d.phase = CatalogBufferPhase::Ready,
                    Err(e) => {
                        d.phase = CatalogBufferPhase::Failed;
                        d.error = Some(e);
                    }
                }
                d.job = None;
            }
            CatalogJobResult::Disposed => {
                d.phase = CatalogBufferPhase::Released;
                d.job = None;
            }
            CatalogJobResult::WorkerUnconfirmed(observation) => {
                d.phase = CatalogBufferPhase::Failed;
                d.error = Some(CatalogPreparationFailure::WorkerUnconfirmed(observation));
            }
            _ => {}
        }
    }
}
impl CatalogBufferSlot {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer)
    }
    pub(crate) fn status(&self) -> CatalogBufferStatus {
        self.buffer.refresh();
        let d = lifecycle_lock(&self.buffer.data);
        let phase = if self.buffer.payload.retired.load(Ordering::Acquire)
            && d.phase != CatalogBufferPhase::Claimed
        {
            CatalogBufferPhase::Released
        } else {
            d.phase
        };
        CatalogBufferStatus {
            phase,
            next_offset: d.next,
            error: d.error.clone(),
        }
    }
    pub(crate) fn retire(&self) {
        self.buffer.payload.retired.store(true, Ordering::Release);
        self.state.changed.notify_one();
    }
    pub(crate) fn append(
        &self,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<(CatalogWorkReceipt, Option<CatalogWork>), CatalogBufferError> {
        self.buffer.refresh();
        let mut d = lifecycle_lock(&self.buffer.data);
        if self.buffer.payload.retired.load(Ordering::Acquire) {
            return Err(CatalogBufferError::BodyUnavailable);
        }
        if d.phase != CatalogBufferPhase::Receiving {
            return Err(CatalogBufferError::InvalidState);
        }
        if let Some((prior, part)) = &d.inflight {
            if *prior == offset && *part == bytes {
                return Ok((
                    d.job
                        .as_ref()
                        .ok_or(CatalogBufferError::StorageUnavailable)?
                        .clone(),
                    None,
                ));
            }
            return Err(CatalogBufferError::Conflict);
        }
        let next = d.next;
        let previous = d.previous;
        let cell = self.buffer.payload.clone();
        let mut remembered = Vec::new();
        remembered
            .try_reserve_exact(bytes.len())
            .map_err(|_| CatalogBufferError::Allocation)?;
        remembered.extend_from_slice(&bytes);
        let work = self.state.job(CatalogTaskPhase::Append, move |_| {
            let value = { lifecycle_lock(&cell.value).take() };
            let Some(Payload::Bytes(mut buffer)) = value else {
                return CatalogJobResult::Append(Err(CatalogBufferError::InvalidState));
            };
            let result = (|| {
                if let Some((prior, len)) = previous
                    && prior == offset
                {
                    let start = usize::try_from(prior).map_err(|_| CatalogBufferError::Conflict)?;
                    let end = start.checked_add(len).ok_or(CatalogBufferError::Conflict)?;
                    if len == bytes.len() && buffer.get(start..end) == Some(bytes.as_slice()) {
                        return Ok(next);
                    }
                    return Err(CatalogBufferError::Conflict);
                }
                if offset != next {
                    return Err(CatalogBufferError::Conflict);
                }
                let end = offset
                    .checked_add(bytes.len() as u64)
                    .ok_or(CatalogBufferError::Conflict)?;
                usize::try_from(end).map_err(|_| CatalogBufferError::Allocation)?;
                buffer
                    .try_reserve(bytes.len())
                    .map_err(|_| CatalogBufferError::Allocation)?;
                buffer.extend_from_slice(&bytes);
                Ok(end)
            })();
            if cell.retired.load(Ordering::Acquire) {
                drop(buffer);
            } else {
                *lifecycle_lock(&cell.value) = Some(Payload::Bytes(buffer));
            }
            CatalogJobResult::Append(result)
        })?;
        let receipt = work.receipt.clone();
        d.inflight = Some((offset, remembered));
        d.job = Some(receipt.clone());
        Ok((receipt, Some(work)))
    }
    pub(crate) fn prepare(
        &self,
        total: u64,
        work: impl FnOnce(
            Vec<u8>,
            tokio::runtime::Handle,
        ) -> Result<CatalogDraft, CatalogPreparationFailure>
        + Send
        + 'static,
    ) -> Result<CatalogWork, CatalogBufferError> {
        self.buffer.refresh();
        let mut d = lifecycle_lock(&self.buffer.data);
        if self.buffer.payload.retired.load(Ordering::Acquire) {
            return Err(CatalogBufferError::BodyUnavailable);
        }
        if d.phase != CatalogBufferPhase::Receiving || d.job.is_some() {
            return Err(CatalogBufferError::InvalidState);
        }
        if d.next != total {
            return Err(CatalogBufferError::Conflict);
        }
        let cell = self.buffer.payload.clone();
        let pending = self.state.job(CatalogTaskPhase::Prepare, move |runtime| {
            let value = { lifecycle_lock(&cell.value).take() };
            let Some(Payload::Bytes(bytes)) = value else {
                return CatalogJobResult::Prepared(Err(CatalogPreparationFailure::Upload(
                    CatalogBufferError::InvalidState,
                )));
            };
            match work(bytes, runtime) {
                Ok(draft) => {
                    if cell.retired.load(Ordering::Acquire) {
                        drop(draft);
                    } else {
                        *lifecycle_lock(&cell.value) = Some(Payload::Draft(draft));
                    }
                    CatalogJobResult::Prepared(Ok(()))
                }
                Err(e) => CatalogJobResult::Prepared(Err(e)),
            }
        })?;
        d.phase = CatalogBufferPhase::Preparing;
        d.job = Some(pending.receipt.clone());
        Ok(pending)
    }
    pub(crate) fn registered(&self) -> Option<CatalogWorkReceipt> {
        lifecycle_lock(&self.buffer.data).registered.clone()
    }
    pub(crate) fn register(
        &self,
        work: impl FnOnce(
            CatalogDraft,
            tokio::runtime::Handle,
        ) -> Result<CatalogRequestId, CatalogFailure>
        + Send
        + 'static,
    ) -> Result<CatalogWork, CatalogBufferError> {
        self.buffer.refresh();
        let mut d = lifecycle_lock(&self.buffer.data);
        if d.phase != CatalogBufferPhase::Ready
            || self.buffer.payload.retired.load(Ordering::Acquire)
        {
            return Err(CatalogBufferError::BodyUnavailable);
        }
        let cell = self.buffer.payload.clone();
        let pending = self.state.job(CatalogTaskPhase::Register, move |runtime| {
            let value = { lifecycle_lock(&cell.value).take() };
            let Some(Payload::Draft(draft)) = value else {
                return CatalogJobResult::Registered(Err(super::native_catalog::failure(
                    crate::domain::native_catalog::CatalogOperationKind::Register,
                    CatalogError::InvalidConfiguration,
                )));
            };
            CatalogJobResult::Registered(work(draft, runtime))
        })?;
        d.phase = CatalogBufferPhase::Claimed;
        d.registered = Some(pending.receipt.clone());
        d.job = Some(pending.receipt.clone());
        self.buffer.payload.retired.store(true, Ordering::Release);
        Ok(pending)
    }
}
impl State {
    fn job(
        &self,
        phase: CatalogTaskPhase,
        work: impl FnOnce(tokio::runtime::Handle) -> CatalogJobResult + Send + 'static,
    ) -> Result<CatalogWork, CatalogBufferError> {
        self.scope
            .require_open()
            .map_err(|_| CatalogBufferError::NodeStopping)?;
        let mut inner = lifecycle_lock(&self.inner);
        if inner.sealed {
            return Err(CatalogBufferError::NodeStopping);
        }
        inner
            .jobs
            .try_reserve(1)
            .map_err(|_| CatalogBufferError::Allocation)?;
        let result = Arc::new(Mutex::new(None));
        let slot = TaskSlot::new();
        let work_cell: WorkCell = Arc::new(Mutex::new(Some(Box::new(work))));
        let job = Arc::new(Job {
            slot: slot.clone(),
            phase,
            result: result.clone(),
            _work: Some(work_cell.clone()),
            #[cfg(test)]
            joined_observed: Arc::new(AtomicBool::new(false)),
        });
        inner.jobs.push(job.clone());
        drop(inner);
        self.changed.notify_one();
        // On launch failure the work cell remains registered; no large capture is dropped by the reader.
        let retained = work_cell.clone();
        let runtime = self.runtime.clone();
        let changed = self.changed.clone();
        let receipt = CatalogWorkReceipt {
            job,
            changed: changed.clone(),
        };
        #[cfg(test)]
        let observer = lifecycle_lock(&self.observer).clone();
        #[cfg(test)]
        let identity = slot.observation(phase).identity;
        let launch = Box::new(move || {
            let rt = runtime.clone();
            slot.spawn_blocking(&runtime, move || {
                #[cfg(test)]
                if let Some(observer) = observer {
                    observer(CatalogWorkerPoint::Entered, phase, identity);
                }
                let work = { lifecycle_lock(&retained).take() };
                if let Some(work) = work {
                    let output = work(rt);
                    *lifecycle_lock(&result) = Some(output);
                    changed.notify_one();
                }
            });
        });
        // The closure captures only lightweight cells; payload stays in the separately registered buffer.
        Ok(CatalogWork {
            receipt,
            launch: Some(launch),
        })
    }
    fn seal(&self) {
        let mut inner = lifecycle_lock(&self.inner);
        inner.sealed = true;
        for b in &inner.buffers {
            b.payload.retired.store(true, Ordering::Release);
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
            let (jobs, buffers) = {
                let inner = lifecycle_lock(&self.inner);
                (inner.jobs.clone(), inner.buffers.clone())
            };
            let mut joins = FuturesUnordered::new();
            for job in jobs {
                #[cfg(test)]
                let observer = lifecycle_lock(&self.observer).clone();
                #[cfg(test)]
                let observation = job.slot.observation(job.phase);
                #[cfg(test)]
                if matches!(job.slot.state(), CatalogTaskJoinState::Joined(_))
                    && !job.joined_observed.swap(true, Ordering::AcqRel)
                    && let Some(observer) = &observer
                {
                    observer(
                        CatalogWorkerPoint::Joined,
                        job.phase,
                        observation.identity.clone(),
                    );
                }
                if !matches!(
                    job.slot.state(),
                    CatalogTaskJoinState::Joined(_) | CatalogTaskJoinState::StartUnconfirmed
                ) {
                    let changed = self.changed.clone();
                    joins.push(async move {
                        let joined = job.slot.join().await;
                        #[cfg(test)]
                        if matches!(joined, CatalogTaskJoinState::Joined(_))
                            && !job.joined_observed.swap(true, Ordering::AcqRel)
                            && let Some(observer) = observer
                        {
                            observer(CatalogWorkerPoint::Joined, job.phase, observation.identity);
                        }
                        #[cfg(not(test))]
                        let _ = joined;
                        changed.notify_one();
                    });
                }
            }
            for b in &buffers {
                b.refresh();
                if !b.payload.retired.load(Ordering::Acquire) {
                    continue;
                }
                let mut data = lifecycle_lock(&b.data);
                let busy = data.job.as_ref().is_some_and(|r| {
                    r.job.slot.state() != CatalogTaskJoinState::Joined(CatalogTaskExit::Completed)
                });
                if busy {
                    continue;
                }
                if lifecycle_lock(&b.payload.value).is_some() {
                    let cell = b.payload.clone();
                    let result = Arc::new(Mutex::new(None));
                    let slot = TaskSlot::new();
                    let job = Arc::new(Job {
                        slot: slot.clone(),
                        phase: CatalogTaskPhase::Dispose,
                        result: result.clone(),
                        _work: None,
                        #[cfg(test)]
                        joined_observed: Arc::new(AtomicBool::new(false)),
                    });
                    {
                        let mut inner = lifecycle_lock(&self.inner);
                        if inner.jobs.try_reserve(1).is_err() {
                            continue;
                        }
                        inner.jobs.push(job.clone());
                    }
                    let receipt = CatalogWorkReceipt {
                        job,
                        changed: self.changed.clone(),
                    };
                    data.job = Some(receipt);
                    drop(data);
                    let changed = self.changed.clone();
                    #[cfg(test)]
                    let observer = lifecycle_lock(&self.observer).clone();
                    #[cfg(test)]
                    let identity = slot.observation(CatalogTaskPhase::Dispose).identity;
                    slot.spawn_blocking(&self.runtime, move || {
                        #[cfg(test)]
                        if let Some(observer) = observer {
                            observer(
                                CatalogWorkerPoint::Entered,
                                CatalogTaskPhase::Dispose,
                                identity,
                            );
                        }
                        let value = { lifecycle_lock(&cell.value).take() };
                        drop(value);
                        *lifecycle_lock(&result) = Some(CatalogJobResult::Disposed);
                        changed.notify_one();
                    });
                }
            }
            let removed = {
                let mut inner = lifecycle_lock(&self.inner);
                let mut removed = Vec::new();
                let mut i = 0;
                while i < inner.jobs.len() {
                    if inner.jobs[i].slot.state()
                        == CatalogTaskJoinState::Joined(CatalogTaskExit::Completed)
                    {
                        removed.push(inner.jobs.swap_remove(i));
                    } else {
                        i += 1;
                    }
                }
                removed
            };
            drop(removed);
            // Inspect metadata without holding the registry lock: callers register jobs under buffer -> registry.
            let reap = buffers
                .iter()
                .filter(|b| {
                    let d = lifecycle_lock(&b.data);
                    b.payload.retired.load(Ordering::Acquire)
                        && lifecycle_lock(&b.payload.value).is_none()
                        && d.job.as_ref().is_none_or(|r| {
                            r.job.slot.state()
                                == CatalogTaskJoinState::Joined(CatalogTaskExit::Completed)
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            let removed = {
                let mut inner = lifecycle_lock(&self.inner);
                let mut removed = Vec::new();
                let mut i = 0;
                while i < inner.buffers.len() {
                    if reap.iter().any(|b| Arc::ptr_eq(b, &inner.buffers[i])) {
                        removed.push(inner.buffers.swap_remove(i));
                    } else {
                        i += 1;
                    }
                }
                removed
            };
            drop(removed);
            let done = {
                let inner = lifecycle_lock(&self.inner);
                inner.sealed && inner.jobs.is_empty() && inner.buffers.is_empty()
            };
            if done {
                return;
            }
            tokio::select! {()=&mut notified=>{},_=joins.next(),if !joins.is_empty()=>{}}
        }
    }
}
impl CatalogUploadShutdownPort for Shutdown {
    fn seal(&self) {
        self.state.seal();
    }
    fn observe(&self) -> CatalogShutdownObservation {
        let inner = lifecycle_lock(&self.state.inner);
        CatalogShutdownObservation {
            sealed: inner.sealed,
            payload_roots: inner
                .buffers
                .iter()
                .filter(|b| lifecycle_lock(&b.payload.value).is_some())
                .count(),
            active_use: inner
                .jobs
                .iter()
                .filter(|j| {
                    j.slot.state() != CatalogTaskJoinState::Joined(CatalogTaskExit::Completed)
                })
                .count(),
            registered_jobs: inner.jobs.len(),
            workers: inner
                .jobs
                .iter()
                .map(|j| j.slot.observation(j.phase))
                .collect(),
            supervisor: self.supervisor.state(),
        }
    }
    fn join(&self) -> Pin<Box<dyn Future<Output = CatalogShutdownReport> + Send + '_>> {
        Box::pin(async move {
            self.seal();
            let joined = self.supervisor.join().await;
            if joined != CatalogTaskJoinState::Joined(CatalogTaskExit::Completed) {
                return std::future::pending().await;
            }
            CatalogShutdownReport {
                observation: self.observe(),
            }
        })
    }
}

#[cfg(test)]
mod worker_failure_tests {
    use super::*;
    fn receipt(slot: Arc<TaskSlot>) -> CatalogWorkReceipt {
        CatalogWorkReceipt {
            job: Arc::new(Job {
                slot,
                phase: CatalogTaskPhase::Register,
                result: Arc::new(Mutex::new(None)),
                _work: None,
                joined_observed: Arc::new(AtomicBool::new(false)),
            }),
            changed: Arc::new(Notify::new()),
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn catalog_c1_real_worker_failure_is_explicit_unknown_not_pending_or_nowrite() {
        let slot = TaskSlot::new();
        slot.spawn_blocking(&tokio::runtime::Handle::current(), || {
            panic!("controlled catalog worker failure")
        });
        let receipt = receipt(slot);
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), receipt.wait())
            .await
            .unwrap();
        assert!(matches!(
            result,
            CatalogJobResult::WorkerUnconfirmed(CatalogTaskObservation {
                join: CatalogTaskJoinState::Joined(CatalogTaskExit::Failed(_)),
                ..
            })
        ));
        assert!(receipt.original_result().is_none());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn catalog_c1_real_launch_guard_failure_is_explicit_unknown_not_pending_or_nowrite() {
        let slot = TaskSlot::new();
        slot.fail_launch_for_test();
        let receipt = receipt(slot);
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), receipt.wait())
            .await
            .unwrap();
        assert!(matches!(
            result,
            CatalogJobResult::WorkerUnconfirmed(CatalogTaskObservation {
                join: CatalogTaskJoinState::StartUnconfirmed,
                ..
            })
        ));
        assert!(receipt.original_result().is_none());
    }
}
