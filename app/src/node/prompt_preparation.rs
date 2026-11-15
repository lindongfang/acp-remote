//! A component-owned full-upload parser/hasher. Admission authority remains in Queue.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::NodeRequestAuthority;
use crate::{
    adapters::acp::{AcpError, PromptDocument, native_dispatch::AcpNativeAdapter},
    application::{
        prompt_preparation::{
            PreparationFailure, PreparationShutdownObservation, PreparationShutdownReport,
            PreparationTaskIdentity, PreparationTaskObservation, PreparationTaskPhase,
            PromptPreparationShutdownPort, TaskExit, TaskJoinState, lifecycle_lock,
        },
        queue_owner::{QueueService, RuntimeSupervisor, ValidatedBody},
        request_registry::SystemNodeClock,
    },
    session_storage::request_registry::SqliteRequestRegistry,
    storage::Storage,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::{Notify, oneshot};
mod shutdown;
mod task_slot;
#[cfg(test)]
pub(super) mod test_hooks;
#[cfg(not(test))]
pub(super) mod test_hooks {
    #[derive(Clone, Copy)]
    pub(in crate::node) enum Point {
        BeforeAdmissionLock,
        RegisteredBeforeStart,
        CpuEntered,
        CpuJoined,
        BeforeResultPublish,
        ResultPublishAttempted,
        DisposalEntered,
        DisposalJoined,
    }
    #[derive(Clone, Default)]
    pub(in crate::node) struct Hooks;
    impl Hooks {
        #[allow(
            clippy::unused_self,
            reason = "Matches test_hooks::Hooks::point method signature."
        )]
        pub(in crate::node) const fn point(&self, _: Point) {}
    }
}
#[cfg(test)]
use crate::{
    adapters::acp::prompt_preparation_test_hooks as acp_hooks,
    application::queue_owner::validation_test_hooks as queue_hooks,
};
use task_slot::TaskSlot;
use test_hooks::{Hooks, Point};
pub(super) type NodePromptQueue = QueueService<
    NodeRequestAuthority,
    Arc<AcpNativeAdapter>,
    SqliteRequestRegistry,
    SystemNodeClock,
>;
pub(super) type PreparationError = PreparationFailure;
struct Payload {
    validated: ValidatedBody<PromptDocument>,
}
struct WorkerData {
    upload: Option<Vec<u8>>,
    result: Option<Result<Arc<Payload>, PreparationError>>,
}
struct ReceiptSignal {
    claimed: AtomicBool,
    abandoned: AtomicBool,
    observation: Mutex<PreparationTaskObservation>,
    changed: Arc<Notify>,
}
impl ReceiptSignal {
    fn record_phase(&self, phase: PreparationTaskPhase) {
        let mut observation = lifecycle_lock(&self.observation);
        *observation = PreparationTaskObservation::from_owner_state(
            observation.identity().clone(),
            phase,
            observation.join(),
        );
    }
    fn record_join(&self, join: TaskJoinState) {
        let mut observation = lifecycle_lock(&self.observation);
        *observation = PreparationTaskObservation::from_owner_state(
            observation.identity().clone(),
            observation.phase(),
            join,
        );
    }
}
struct Disposal {
    slot: Arc<TaskSlot>,
    payload: Arc<Mutex<Option<Arc<Payload>>>>,
}
struct JobData {
    processed: bool,
    payload: Option<Arc<Payload>>,
    sender: Option<oneshot::Sender<Result<PreparedPrompt, PreparationError>>>,
    uses: usize,
    claimed: bool,
    disposal: Option<Arc<Disposal>>,
}
struct Job {
    slot: Arc<TaskSlot>,
    worker: Arc<Mutex<WorkerData>>,
    signal: Arc<ReceiptSignal>,
    data: Mutex<JobData>,
}
struct Inner {
    sealed: bool,
    jobs: Vec<Arc<Job>>,
}
struct State {
    inner: Mutex<Inner>,
    queue: NodePromptQueue,
    runtime: tokio::runtime::Handle,
    changed: Arc<Notify>,
    handles: AtomicUsize,
    hooks: Hooks,
}
pub(super) struct PreparedBodyHandle {
    state: Arc<State>,
}
impl Clone for PreparedBodyHandle {
    fn clone(&self) -> Self {
        self.state.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            state: self.state.clone(),
        }
    }
}
impl Drop for PreparedBodyHandle {
    fn drop(&mut self) {
        if self.state.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.state.seal();
        }
    }
}
pub(super) struct PreparationBeginRejected {
    kind: PreparationError,
    upload: Vec<u8>,
}
impl PreparationBeginRejected {
    pub(super) const fn kind(&self) -> PreparationError {
        self.kind
    }
    pub(super) fn into_upload(self) -> Vec<u8> {
        self.upload
    }
}
impl std::fmt::Debug for PreparationBeginRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PreparationBeginRejected")
            .field(&self.kind)
            .finish()
    }
}
pub(super) struct PreparationReceipt {
    received: bool,
    receiver: oneshot::Receiver<Result<PreparedPrompt, PreparationError>>,
    signal: Arc<ReceiptSignal>,
}
impl PreparationReceipt {
    pub(super) fn try_ready(&mut self) -> Result<Option<PreparedPrompt>, PreparationError> {
        if self.received {
            return Err(PreparationError::ResultAlreadyClaimed);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.received = true;
                if result.is_ok() {
                    self.signal.claimed.store(true, Ordering::Release);
                }
                result.map(Some)
            }
            Err(oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(oneshot::error::TryRecvError::Closed) => {
                self.received = true;
                Err(PreparationError::OwnerUnavailable)
            }
        }
    }
    pub(super) fn observe(&self) -> PreparationTaskObservation {
        lifecycle_lock(&self.signal.observation).clone()
    }
    pub(super) async fn wait(&mut self) -> Result<PreparedPrompt, PreparationError> {
        if self.received {
            return Err(PreparationError::ResultAlreadyClaimed);
        }
        let result = (&mut self.receiver).await;
        self.received = true;
        let result = result.map_err(|_| PreparationError::OwnerUnavailable)?;
        if result.is_ok() {
            self.signal.claimed.store(true, Ordering::Release);
        }
        result
    }
}
impl Drop for PreparationReceipt {
    fn drop(&mut self) {
        if !self.signal.claimed.load(Ordering::Acquire) {
            self.signal.abandoned.store(true, Ordering::Release);
            self.signal.changed.notify_one();
        }
    }
}
pub(super) struct PreparedPrompt {
    state: Arc<State>,
    job: Arc<Job>,
}
impl std::fmt::Debug for PreparedPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedPrompt([custody])")
    }
}
impl PreparedPrompt {
    pub(super) fn acquire_use(&self) -> Result<PreparedPromptLease, PreparationError> {
        let inner = lifecycle_lock(&self.state.inner);
        if inner.sealed {
            return Err(PreparationError::Stopping);
        }
        let mut data = lifecycle_lock(&self.job.data);
        let payload = data
            .payload
            .clone()
            .ok_or(PreparationError::OwnerUnavailable)?;
        data.uses += 1;
        drop(data);
        drop(inner);
        Ok(PreparedPromptLease {
            state: self.state.clone(),
            job: self.job.clone(),
            payload: Some(payload),
        })
    }
}
impl Drop for PreparedPrompt {
    fn drop(&mut self) {
        self.job.signal.abandoned.store(true, Ordering::Release);
        self.state.changed.notify_one();
    }
}
pub(super) struct PreparedPromptLease {
    state: Arc<State>,
    job: Arc<Job>,
    payload: Option<Arc<Payload>>,
}
impl PreparedPromptLease {
    pub(super) fn validated(&self) -> &ValidatedBody<PromptDocument> {
        // A live lease always has its payload; only this type's Drop removes it.
        self.payload.as_ref().map_or_else(
            || unreachable!("lease payload only removed during Drop"),
            |p| &p.validated,
        )
    }
}
impl Drop for PreparedPromptLease {
    fn drop(&mut self) {
        drop(self.payload.take());
        let mut d = lifecycle_lock(&self.job.data);
        d.uses -= 1;
        drop(d);
        self.state.changed.notify_one();
    }
}
impl PreparedBodyHandle {
    pub(super) fn seal(&self) {
        self.state.seal();
    }
    pub(super) fn begin(
        &self,
        uploaded: Vec<u8>,
    ) -> Result<PreparationReceipt, PreparationBeginRejected> {
        self.state.hooks.point(Point::BeforeAdmissionLock);
        let mut inner = lifecycle_lock(&self.state.inner);
        if inner.sealed {
            return Err(PreparationBeginRejected {
                kind: PreparationError::Stopping,
                upload: uploaded,
            });
        }
        if inner.jobs.try_reserve(1).is_err() {
            return Err(PreparationBeginRejected {
                kind: PreparationError::Allocation,
                upload: uploaded,
            });
        }
        let (sender, receiver) = oneshot::channel();
        let slot = TaskSlot::new();
        let signal = Arc::new(ReceiptSignal {
            claimed: AtomicBool::new(false),
            abandoned: AtomicBool::new(false),
            observation: Mutex::new(slot.observation(PreparationTaskPhase::Registered)),
            changed: self.state.changed.clone(),
        });
        let worker = Arc::new(Mutex::new(WorkerData {
            upload: Some(uploaded),
            result: None,
        }));
        let job = Arc::new(Job {
            slot: slot.clone(),
            worker: worker.clone(),
            signal: signal.clone(),
            data: Mutex::new(JobData {
                processed: false,
                payload: None,
                sender: Some(sender),
                uses: 0,
                claimed: false,
                disposal: None,
            }),
        });
        inner.jobs.push(job);
        drop(inner);
        let queue = self.state.queue.clone();
        let hooks = self.state.hooks.clone();
        let worker_signal = signal.clone();
        slot.spawn_blocking(&self.state.runtime, move || {
            let upload = lifecycle_lock(&worker).upload.take();
            hooks.point(Point::CpuEntered);
            worker_signal.record_phase(PreparationTaskPhase::CpuEntered);
            #[cfg(test)]
            let hook_signal = worker_signal.clone();
            #[cfg(test)]
            let adapter_hooks = hooks.clone();
            #[cfg(test)]
            let _adapter = acp_hooks::install(Arc::new(move |p| {
                let (phase, point) = match p {
                    acp_hooks::Point::RawParsedBeforeBlockChecks => (
                        PreparationTaskPhase::ParseReturned,
                        Point::RawParsedBeforeBlockChecks,
                    ),
                    acp_hooks::Point::DigestComputed => {
                        (PreparationTaskPhase::HashComputed, Point::DigestComputed)
                    }
                };
                hook_signal.record_phase(phase);
                adapter_hooks.point(point);
            }));
            #[cfg(test)]
            let _queue = hooks.queue.map(queue_hooks::install);
            let result = upload
                .ok_or(PreparationError::OwnerUnavailable)
                .and_then(|u| {
                    PromptDocument::parse(u).map_err(|e| match e {
                        AcpError::Allocation => PreparationError::Allocation,
                        _ => PreparationError::MalformedInput,
                    })
                })
                .and_then(|parsed| {
                    worker_signal.record_phase(PreparationTaskPhase::ParseReturned);
                    if worker_signal.abandoned.load(Ordering::Acquire) {
                        return Err(PreparationError::OwnerUnavailable);
                    }
                    queue
                        .validate_body(&parsed)
                        .map(|validated| {
                            worker_signal.record_phase(PreparationTaskPhase::HashComputed);
                            Arc::new(Payload { validated })
                        })
                        .map_err(PreparationError::Validation)
                });
            lifecycle_lock(&worker).result = Some(result);
        });
        signal.record_join(slot.state());
        self.state.changed.notify_one();
        Ok(PreparationReceipt {
            received: false,
            receiver,
            signal,
        })
    }
}
impl State {
    fn seal(&self) {
        lifecycle_lock(&self.inner).sealed = true;
        self.changed.notify_one();
    }
    fn finish_cpu(self: &Arc<Self>, job: &Arc<Job>) {
        let join = job.slot.state();
        job.signal.record_join(join);
        self.hooks.point(Point::CpuJoined);
        let result = lifecycle_lock(&job.worker)
            .result
            .take()
            .unwrap_or(Err(PreparationError::WorkerFailed));
        let sender = {
            let mut data = lifecycle_lock(&job.data);
            data.processed = true;
            match result {
                Ok(payload) => data.payload = Some(payload),
                Err(error) => {
                    if let Some(sender) = data.sender.take() {
                        let _ = sender.send(Err(error));
                    }
                }
            }
            data.sender.take()
        };
        if lifecycle_lock(&job.data).payload.is_none() {
            job.signal.record_phase(PreparationTaskPhase::Terminal);
        }
        if let Some(sender) = sender {
            let sealed = lifecycle_lock(&self.inner).sealed;
            if !sealed && !job.signal.abandoned.load(Ordering::Acquire) {
                lifecycle_lock(&job.data).claimed = true;
                self.hooks.point(Point::BeforeResultPublish);
                let _ = sender.send(Ok(PreparedPrompt {
                    state: self.clone(),
                    job: job.clone(),
                }));
                self.hooks.point(Point::ResultPublishAttempted);
            } else {
                let _ = sender.send(Err(PreparationError::Stopping));
            }
        }
    }
    fn join_waiter(slot: Arc<TaskSlot>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let _ = slot.join().await;
        })
    }
    async fn supervise(self: Arc<Self>) {
        use futures_util::{StreamExt, stream::FuturesUnordered};
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let jobs = lifecycle_lock(&self.inner).jobs.clone();
            let mut joins = FuturesUnordered::new();
            for job in &jobs {
                if !lifecycle_lock(&job.data).processed {
                    if matches!(job.slot.state(), TaskJoinState::Joined(_)) {
                        self.finish_cpu(job);
                    } else {
                        joins.push(Self::join_waiter(job.slot.clone()));
                        continue;
                    }
                }
                let sealed = lifecycle_lock(&self.inner).sealed;
                let disposal = {
                    let mut data = lifecycle_lock(&job.data);
                    if (sealed || job.signal.abandoned.load(Ordering::Acquire))
                        && data.uses == 0
                        && data.payload.is_some()
                        && data.disposal.is_none()
                    {
                        let disposal = Arc::new(Disposal {
                            slot: TaskSlot::new(),
                            payload: Arc::new(Mutex::new(data.payload.take())),
                        });
                        data.disposal = Some(disposal.clone());
                        Some((disposal, true))
                    } else {
                        data.disposal.clone().map(|d| (d, false))
                    }
                };
                if let Some((disposal, start)) = disposal {
                    if start {
                        let payload = disposal.payload.clone();
                        let hooks = self.hooks.clone();
                        disposal.slot.spawn_blocking(&self.runtime, move || {
                            hooks.point(Point::DisposalEntered);
                            let payload = lifecycle_lock(&payload).take();
                            drop(payload);
                        });
                    }
                    if matches!(disposal.slot.state(), TaskJoinState::Joined(_)) {
                        if lifecycle_lock(&job.signal.observation).phase()
                            != PreparationTaskPhase::Terminal
                        {
                            self.hooks.point(Point::DisposalJoined);
                            job.signal.record_phase(PreparationTaskPhase::Terminal);
                        }
                    } else {
                        joins.push(Self::join_waiter(disposal.slot.clone()));
                    }
                }
            }
            let done = {
                let inner = lifecycle_lock(&self.inner);
                inner.sealed
                    && inner.jobs.iter().all(|job| {
                        let data = lifecycle_lock(&job.data);
                        data.processed
                            && data.uses == 0
                            && data.payload.is_none()
                            && data
                                .disposal
                                .as_ref()
                                .is_none_or(|d| matches!(d.slot.state(), TaskJoinState::Joined(_)))
                    })
            };
            if done {
                return;
            }
            tokio::select! {()=&mut changed=>{},_ = joins.next(),if !joins.is_empty()=>{}}
        }
    }
}
pub(super) fn attach_prompt_preparation(
    storage: &Storage,
    queue: NodePromptQueue,
) -> Result<PreparedBodyHandle, PreparationError> {
    #[cfg(test)]
    {
        attach_with_hooks(storage, queue, Hooks::default())
    }
    #[cfg(not(test))]
    {
        attach_with_hooks(storage, queue, Hooks)
    }
}
pub(super) fn attach_with_hooks(
    storage: &Storage,
    queue: NodePromptQueue,
    hooks: Hooks,
) -> Result<PreparedBodyHandle, PreparationError> {
    let runtime =
        tokio::runtime::Handle::try_current().map_err(|_| PreparationError::Unavailable)?;
    let identity = queue.queue_runtime_identity();
    let state = Arc::new(State {
        inner: Mutex::new(Inner {
            sealed: false,
            jobs: Vec::new(),
        }),
        queue,
        runtime: runtime.clone(),
        changed: Arc::new(Notify::new()),
        handles: AtomicUsize::new(1),
        hooks,
    });
    let supervisor = TaskSlot::new();
    let shutdown = Arc::new(shutdown::PreparationShutdown {
        state: state.clone(),
        supervisor: supervisor.clone(),
    });
    storage
        .register_prompt_preparation(&identity, shutdown)
        .map_err(PreparationError::Validation)?;
    state.hooks.point(Point::RegisteredBeforeStart);
    supervisor.spawn_supervisor(&runtime, state.clone());
    if !matches!(supervisor.state(), TaskJoinState::Pending) {
        state.seal();
        return Err(PreparationError::OwnerUnavailable);
    }
    Ok(PreparedBodyHandle { state })
}
