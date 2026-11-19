//! Stable task slots are registered before the task can perform any work.
#![allow(
    clippy::significant_drop_tightening,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{Arc, AtomicBool, DispatchError, Future, Instant, Ordering, Pin};
use futures_util::FutureExt;
use std::{
    panic::AssertUnwindSafe,
    sync::{Mutex, atomic::AtomicU64},
    task::Poll,
};
use tokio::{sync::Notify, task::JoinHandle};

#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
tokio::task_local! {static APPROVAL_OWNED_TASK:std::sync::Weak<TaskSlot>;}
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
#[allow(
    clippy::redundant_pub_crate,
    reason = "Re-exported as pub(crate) from native_dispatch"
)]
pub(crate) fn owned_job_registered_for_approval_test() -> bool {
    APPROVAL_OWNED_TASK
        .try_with(|original| original.upgrade().is_some())
        .unwrap_or(false)
}
pub(super) struct CloseProgress {
    generation: AtomicU64,
    parent: Option<Arc<Self>>,
    changed: Notify,
}

impl CloseProgress {
    pub(super) const fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            parent: None,
            changed: Notify::const_new(),
        }
    }
    pub(super) fn child(parent: Arc<Self>) -> Self {
        Self {
            generation: AtomicU64::new(0),
            parent: Some(parent),
            changed: Notify::new(),
        }
    }
    pub(super) fn current(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub(super) fn record(&self) {
        // A wrapping wake stamp grants nothing and can only conservatively miss a wake.
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_waiters();
        if let Some(parent) = &self.parent {
            parent.record();
        }
    }
    pub(super) async fn wait_after(&self, observed: u64) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.current() != observed {
                return;
            }
            changed.await;
        }
    }
}

pub(super) struct TaskSlot {
    handle: Mutex<Option<JoinHandle<()>>>,
    join_owner: tokio::sync::Mutex<()>,
    installed: AtomicBool,
    changed: Notify,
    joined: AtomicBool,
    failed: AtomicBool,
}
impl TaskSlot {
    fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            join_owner: tokio::sync::Mutex::new(()),
            installed: AtomicBool::new(false),
            changed: Notify::new(),
            joined: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        }
    }
    pub(super) async fn join(&self, deadline: Instant) -> bool {
        tokio::time::timeout_at(deadline, self.join_unbounded())
            .await
            .is_ok()
            && self.joined.load(Ordering::Acquire)
    }
    pub(super) async fn join_unbounded(&self) {
        let _join_owner = self.join_owner.lock().await;
        loop {
            let n = self.changed.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.installed.load(Ordering::Acquire) {
                break;
            }
            n.await;
        }
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.joined.load(Ordering::Acquire) {
                break;
            }
            let completion = std::future::poll_fn(|cx| {
                let mut slot = match self.handle.lock() {
                    Ok(v) => v,
                    Err(e) => {
                        self.failed.store(true, Ordering::SeqCst);
                        e.into_inner()
                    }
                };
                let Some(handle) = slot.as_mut() else {
                    return Poll::Ready(());
                };

                match Pin::new(handle).poll(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => {
                        if result.is_err() {
                            self.failed.store(true, Ordering::SeqCst);
                        }
                        slot.take();
                        drop(slot);
                        self.joined.store(true, Ordering::Release);
                        self.changed.notify_waiters();
                        Poll::Ready(())
                    }
                }
            });
            tokio::select! {()=&mut changed=>{},()=completion=>break}
        }
    }
    pub(super) fn confirmed_joined(&self) -> bool {
        self.joined.load(Ordering::Acquire)
    }
    pub(super) fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
    pub(super) fn mark_failed(&self) {
        self.failed.store(true, Ordering::Release);
        self.changed.notify_waiters();
    }
}
pub(super) struct TaskRegistry {
    slots: Mutex<Vec<Arc<TaskSlot>>>,
    sealed: AtomicBool,
    progress: Option<Arc<CloseProgress>>,
}
impl TaskRegistry {
    #[cfg(test)]
    pub(super) fn registered_for_test(&self) -> Result<usize, DispatchError> {
        Ok(self.slots.lock().map_err(|_| DispatchError::Stopped)?.len())
    }
    pub(super) fn is_sealed(&self) -> bool {
        self.sealed.load(Ordering::Acquire)
    }
    pub(super) const fn new() -> Self {
        Self {
            slots: Mutex::new(Vec::new()),
            sealed: AtomicBool::new(false),
            progress: None,
        }
    }
    pub(super) const fn with_progress(progress: Arc<CloseProgress>) -> Self {
        Self {
            slots: Mutex::new(Vec::new()),
            sealed: AtomicBool::new(false),
            progress: Some(progress),
        }
    }
    #[cfg(test)]
    pub(super) fn spawn(
        &self,
        f: impl Future<Output = ()> + Send + 'static,
    ) -> Result<Arc<TaskSlot>, DispatchError> {
        self.spawn_guarded(f, || {})
    }
    pub(super) fn spawn_guarded(
        &self,
        f: impl Future<Output = ()> + Send + 'static,
        on_failure: impl FnOnce() + Send + 'static,
    ) -> Result<Arc<TaskSlot>, DispatchError> {
        self.spawn_guarded_recover(f, on_failure)
            .map_err(|(error, _, _)| error)
    }
    pub(super) fn spawn_guarded_recover<F, X>(
        &self,
        f: F,
        on_failure: X,
    ) -> Result<Arc<TaskSlot>, (DispatchError, F, X)>
    where
        F: Future<Output = ()> + Send + 'static,
        X: FnOnce() + Send + 'static,
    {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return Err((DispatchError::ProtocolUnavailable, f, on_failure));
        };
        let Ok(mut records) = self.slots.lock() else {
            return Err((DispatchError::Stopped, f, on_failure));
        };
        if self.sealed.load(Ordering::SeqCst) {
            return Err((DispatchError::NodeStopping, f, on_failure));
        }
        if records.try_reserve(1).is_err() {
            return Err((DispatchError::StorageUnavailable, f, on_failure));
        }
        let slot = Arc::new(TaskSlot::new());
        records.push(slot.clone());
        let target = slot.clone();
        let progress = self.progress.clone();
        let handle = runtime.spawn(async move {
            #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
            let f = APPROVAL_OWNED_TASK.scope(Arc::downgrade(&target), f);
            if AssertUnwindSafe(f).catch_unwind().await.is_err() {
                target.failed.store(true, Ordering::SeqCst);
                on_failure();
            }
            if let Some(progress) = progress {
                progress.record();
            }
        });
        match slot.handle.lock() {
            Ok(mut s) => *s = Some(handle),
            Err(e) => {
                *e.into_inner() = Some(handle);
                slot.failed.store(true, Ordering::SeqCst);
            }
        }
        slot.installed.store(true, Ordering::Release);
        slot.changed.notify_waiters();
        drop(records);
        Ok(slot)
    }
    #[cfg(windows)]
    pub(super) fn register_existing(&self, slot: Arc<TaskSlot>) -> Result<(), DispatchError> {
        let mut records = self.slots.lock().map_err(|_| DispatchError::Stopped)?;
        if self.sealed.load(Ordering::Acquire) {
            return Err(DispatchError::NodeStopping);
        }
        if records.iter().any(|original| Arc::ptr_eq(original, &slot)) {
            return Ok(());
        }
        records
            .try_reserve(1)
            .map_err(|_| DispatchError::StorageUnavailable)?;
        records.push(slot);
        Ok(())
    }
    #[cfg(windows)]
    pub(super) fn spawn_blocking_guarded(
        &self,
        work: impl FnOnce() + Send + 'static,
        on_failure: impl FnOnce() + Send + 'static,
    ) -> Result<Arc<TaskSlot>, DispatchError> {
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| DispatchError::ProtocolUnavailable)?;
        let mut records = self.slots.lock().map_err(|_| DispatchError::Stopped)?;
        if self.sealed.load(Ordering::Acquire) {
            return Err(DispatchError::NodeStopping);
        }
        records
            .try_reserve(1)
            .map_err(|_| DispatchError::StorageUnavailable)?;
        let slot = Arc::new(TaskSlot::new());
        records.push(slot.clone());
        let weak = Arc::downgrade(&slot);
        let progress = self.progress.clone();
        let handle = runtime.spawn_blocking(move || {
            if std::panic::catch_unwind(AssertUnwindSafe(work)).is_err() {
                if let Some(slot) = weak.upgrade() {
                    slot.failed.store(true, Ordering::Release);
                }
                on_failure();
            }
            if let Some(progress) = progress {
                progress.record();
            }
        });
        match slot.handle.lock() {
            Ok(mut original) => *original = Some(handle),
            Err(error) => {
                *error.into_inner() = Some(handle);
                slot.failed.store(true, Ordering::SeqCst);
            }
        }
        slot.installed.store(true, Ordering::Release);
        slot.changed.notify_waiters();
        drop(records);
        Ok(slot)
    }
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    #[allow(
        dead_code,
        reason = "Used by approval-owner-acceptance diagnostic mount"
    )]
    pub(super) fn approval_diagnostic_counts(
        &self,
    ) -> Result<(usize, usize, usize, bool), DispatchError> {
        let originals = self.slots.lock().map_err(|_| DispatchError::Stopped)?;
        Ok((
            originals.len(),
            originals
                .iter()
                .filter(|slot| slot.confirmed_joined())
                .count(),
            originals.iter().filter(|slot| slot.failed()).count(),
            self.sealed.load(Ordering::Acquire),
        ))
    }
    pub(super) fn seal(&self) {
        let _guard = match self.slots.lock() {
            Ok(v) => v,
            Err(e) => e.into_inner(),
        };
        self.sealed.store(true, Ordering::SeqCst);
    }
    pub(super) fn quiet_flags(&self) -> Result<Vec<u8>, DispatchError> {
        let records = self
            .slots
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if !self.sealed.load(Ordering::SeqCst)
            || records.iter().any(|slot| {
                !slot.installed.load(Ordering::Acquire) || !slot.joined.load(Ordering::Acquire)
            })
        {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        Ok(records.iter().map(|slot| u8::from(slot.failed())).collect())
    }
    pub(super) fn quiet_slots(&self) -> Result<Vec<Arc<TaskSlot>>, DispatchError> {
        self.quiet_flags()?;
        Ok(self
            .slots
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            .clone())
    }
    pub(super) async fn join_unbounded(&self) {
        let slots = match self.slots.lock() {
            Ok(slots) => slots.clone(),
            Err(error) => error.into_inner().clone(),
        };
        for slot in slots {
            slot.join_unbounded().await;
        }
    }
    pub(super) async fn join(&self, deadline: Instant) -> bool {
        let slots = match self.slots.lock() {
            Ok(v) => v.clone(),
            Err(e) => e.into_inner().clone(),
        };
        let mut joined = true;
        for slot in slots {
            if !slot.join(deadline).await {
                joined = false;
            }
        }
        joined
    }
}

#[cfg(test)]
mod join_tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn original_join_handle_survives_poll_owner_cancellation_and_deadline() {
        for expire in [false, true] {
            let registry = TaskRegistry::new();
            let (send, receive) = tokio::sync::oneshot::channel();
            let slot = registry
                .spawn(async move {
                    let _ = receive.await;
                })
                .unwrap();
            let original = slot.clone();
            let first = tokio::spawn(async move {
                original
                    .join(Instant::now() + Duration::from_millis(if expire { 30 } else { 3000 }))
                    .await
            });
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    if slot.join_owner.try_lock().is_err() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let second = slot.join(Instant::now() + Duration::from_secs(2));
            tokio::pin!(second);
            std::future::poll_fn(|cx| {
                assert!(second.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
            if expire {
                assert!(!first.await.unwrap());
            } else {
                first.abort();
                assert!(matches!(first.await,Err(error) if error.is_cancelled()));
            }
            assert!(!slot.confirmed_joined());
            assert!(slot.handle.lock().unwrap().is_some());
            send.send(()).unwrap();
            assert!(second.await);
            assert!(slot.confirmed_joined());
            assert!(slot.handle.lock().unwrap().is_none());
            registry.seal();
            assert_eq!(registry.quiet_slots().unwrap().len(), 1);
            assert_eq!(registry.quiet_flags().unwrap(), vec![0]);
        }
    }
}
