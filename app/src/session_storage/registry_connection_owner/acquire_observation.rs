//! Test-only reads of the original acquisition task and its actual close slot.
use super::ConnectionOwner;
use crate::session_storage::{
    AcquisitionCloseProgress as CloseProgress, AcquisitionObservation,
    AcquisitionSnapshot as Snapshot,
};
use std::sync::{Arc, atomic::Ordering};
impl AcquisitionObservation {
    // Observe only the original miss future's actual poll result; do not change its wake path.
    pub(super) async fn observe_pool_acquire<F: std::future::Future>(
        &self,
        future: F,
    ) -> F::Output {
        tokio::pin!(future);
        std::future::poll_fn(|cx| {
            let result = future.as_mut().poll(cx);
            if result.is_pending() && !self.pool_first_pending.swap(true, Ordering::AcqRel) {
                self.pool_pending_changed.notify_waiters();
            }
            result
        })
        .await
    }
    pub(crate) async fn wait_pool_first_pending(&self) {
        loop {
            let notified = self.pool_pending_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.pool_first_pending.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
    pub(crate) async fn join_original_close(&self) -> bool {
        let physical = self
            .physical_close
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let slot = physical.and_then(|physical| {
            physical
                .close
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        });
        if let Some(slot) = slot {
            slot.wait().await;
            true
        } else {
            false
        }
    }
    pub(crate) fn same_physical(&self, other: &Self) -> bool {
        let own = self
            .physical_close
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let other = other
            .physical_close
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        own.zip(other)
            .is_some_and(|(own, other)| Arc::ptr_eq(&own, &other))
    }
    pub(crate) fn snapshot(&self) -> crate::session_storage::AcquisitionSnapshot {
        let physical = self
            .physical_close
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let close = physical.as_ref().and_then(|physical| {
            physical
                .close
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        });
        Snapshot {
            transferred: self.transferred.load(Ordering::Acquire),
            acquisition_joined: self.joined.load(Ordering::Acquire),
            close: close.as_ref().map_or(CloseProgress::NotStarted, |slot| {
                match (
                    slot.actual.load(Ordering::Acquire),
                    slot.joined.load(Ordering::Acquire),
                ) {
                    (1, 1) => CloseProgress::ConfirmedJoined,
                    (_, 1 | 2) => CloseProgress::UnconfirmedJoined,
                    (1, _) => CloseProgress::ActualAcknowledged,
                    _ => CloseProgress::Pending,
                }
            }),
        }
    }
}
impl ConnectionOwner {
    pub(in crate::session_storage) fn acquisitions_for_test(
        &self,
    ) -> Vec<Arc<AcquisitionObservation>> {
        super::cleanup_state(&self.state)
            .acquisitions
            .iter()
            .map(|slot| slot.observation.clone())
            .collect()
    }
}
