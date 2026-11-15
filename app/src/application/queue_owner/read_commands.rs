//! Read results and actual task joins have distinct owners and observations.
use super::{
    Arc, EntityId, Future, Mutex, NodeClock, Notify, QueueRead, QueueService, RegistrationId,
    RegistryError, RequestAuthority, RequestFact, RequestRegistryRepository, TaskRecord,
    ValidatedBodyPort, lock, oneshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadTaskSettlement {
    Pending,
    Joined,
    Failed,
    Cancelled,
}
pub(in crate::application::queue_owner) struct ReadTaskCompletion {
    state: Mutex<ReadTaskSettlement>,
    changed: Notify,
}
impl ReadTaskCompletion {
    fn new() -> Self {
        Self {
            state: Mutex::new(ReadTaskSettlement::Pending),
            changed: Notify::new(),
        }
    }
    pub(in crate::application::queue_owner) fn complete(
        &self,
        result: &Result<(), tokio::task::JoinError>,
    ) {
        let outcome = match result {
            Ok(()) => ReadTaskSettlement::Joined,
            Err(error) if error.is_cancelled() => ReadTaskSettlement::Cancelled,
            Err(_) => ReadTaskSettlement::Failed,
        };
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = outcome;
        self.changed.notify_waiters();
    }
    #[cfg(test)]
    fn observe(&self) -> ReadTaskSettlement {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueReadFailure {
    Registry(RegistryError),
    AllocationFailed,
    WorkerStoppedUnconfirmed,
}
struct ReadResult<T> {
    value: Mutex<Option<Result<T, QueueReadFailure>>>,
    changed: Notify,
}
impl<T> ReadResult<T> {
    fn publish(&self, value: Result<T, QueueReadFailure>) {
        let mut state = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.is_none() {
            *state = Some(value);
        }
        drop(state);
        self.changed.notify_waiters();
    }
}
struct ReadPublication<T>(Arc<ReadResult<T>>);
impl<T> Drop for ReadPublication<T> {
    fn drop(&mut self) {
        self.0
            .publish(Err(QueueReadFailure::WorkerStoppedUnconfirmed));
    }
}
pub struct QueueReadReceipt<T> {
    result: Arc<ReadResult<T>>,
    #[cfg(test)]
    completion: Arc<ReadTaskCompletion>,
}
impl<T: Clone> QueueReadReceipt<T> {
    #[cfg(test)]
    pub(crate) fn task_observation(&self) -> ReadTaskSettlement {
        self.completion.observe()
    }
    pub(crate) async fn wait(&self) -> Result<T, QueueReadFailure> {
        loop {
            let changed = self.result.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let result = self
                .result
                .value
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(result) = result {
                return result;
            }
            changed.await;
        }
    }
}

impl<A, V, R, C> QueueService<A, V, R, C>
where
    A: RequestAuthority + 'static,
    A::Actor: 'static,
    V: ValidatedBodyPort + 'static,
    R: RequestRegistryRepository + 'static,
    C: NodeClock + 'static,
{
    fn begin_read<T: Clone + Send + 'static>(
        &self,
        work: impl Future<Output = Result<T, RegistryError>> + Send + 'static,
    ) -> Result<QueueReadReceipt<T>, QueueReadFailure> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| QueueReadFailure::Registry(RegistryError::NodeStopping))?;
        let result = Arc::new(ReadResult {
            value: Mutex::new(None),
            changed: Notify::new(),
        });
        // Construct outside the future so even cancellation before its first poll publishes failure.
        let publication = ReadPublication(result.clone());
        let completion = Arc::new(ReadTaskCompletion::new());
        let (start, started) = oneshot::channel();
        let mut tasks = lock(&self.inner.core.tasks).map_err(QueueReadFailure::Registry)?;
        self.inner
            .core
            .running()
            .map_err(QueueReadFailure::Registry)?;
        self.inner.core.reap_completed(&mut tasks);
        tasks
            .try_reserve(1)
            .map_err(|_| QueueReadFailure::AllocationFailed)?;
        let handle = runtime.spawn(async move {
            let publication = publication;
            if started.await.is_ok() {
                publication
                    .0
                    .publish(work.await.map_err(QueueReadFailure::Registry));
            }
        });
        #[cfg(test)]
        let observed_completion = completion.clone();
        tasks.push(TaskRecord::new_read(handle, completion));
        drop(tasks);
        // A concurrent stop can abort the registered job; the original slot and failure remain.
        let _ = start.send(());
        Ok(QueueReadReceipt {
            result,
            #[cfg(test)]
            completion: observed_completion,
        })
    }
    pub(crate) fn begin_inspect_queue<T: Clone + Send + 'static>(
        &self,
        actor: A::Actor,
        target: EntityId,
        project: impl for<'a> FnOnce(QueueRead<'a, V::Body>) -> T + Send + 'static,
    ) -> Result<QueueReadReceipt<T>, QueueReadFailure> {
        let service = self.clone();
        self.begin_read(async move { service.inspect_queue(&actor, target, project).await })
    }
    pub(crate) fn begin_query_request(
        &self,
        actor: A::Actor,
        target: EntityId,
        id: RegistrationId,
    ) -> Result<QueueReadReceipt<RequestFact>, QueueReadFailure> {
        let service = self.clone();
        self.begin_read(async move { service.query_request(&actor, target, id).await })
    }
}
