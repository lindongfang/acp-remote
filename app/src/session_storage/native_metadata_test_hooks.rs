//! Test-only pauses on the real metadata path, before it owns a connection or starts BEGIN.
use crate::{domain::native_dispatch::DispatchError, test_support::AsyncBarrier};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum NativeMetadataPoint {
    ObservationBeforeBegin,
    ControlReservedBeforeBegin,
    ControlTerminalBeforeBegin,
    DispatchReserveBeforeBegin,
    SourceClosedBeforeBegin,
    SourceStateBeforeBegin,
}
#[derive(Default)]
pub struct NativeMetadataHooks {
    pending: Mutex<HashMap<NativeMetadataPoint, Arc<AsyncBarrier>>>,
}
impl NativeMetadataHooks {
    /// Arms one pause; an occupied cut cannot be overwritten.
    /// # Errors
    /// A poisoned controller or an already armed cut is rejected.
    pub fn pause_next(
        &self,
        point: NativeMetadataPoint,
    ) -> Result<Arc<AsyncBarrier>, DispatchError> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| DispatchError::StorageUnavailable)?;
        if pending.contains_key(&point) {
            return Err(DispatchError::Conflict);
        }
        let barrier = Arc::new(AsyncBarrier::default());
        pending.insert(point, barrier.clone());
        drop(pending);
        Ok(barrier)
    }
    /// # Errors
    /// Refuses overwriting an existing one-shot observation pause.
    pub fn pause_next_observation_before_begin(&self) -> Result<Arc<AsyncBarrier>, DispatchError> {
        self.pause_next(NativeMetadataPoint::ObservationBeforeBegin)
    }
    pub(crate) async fn hit(&self, point: NativeMetadataPoint) -> Result<(), DispatchError> {
        let barrier = self
            .pending
            .lock()
            .map_err(|_| DispatchError::StorageUnavailable)?
            .remove(&point);
        if let Some(barrier) = barrier {
            barrier.pause().await;
        }
        Ok(())
    }
}
