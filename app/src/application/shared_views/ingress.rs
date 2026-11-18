//! The source producer never waits for projection, network clients, SQL, or a bounded channel.
use super::{RetainedRaw, journal::MemoryJournal};
use crate::domain::{
    native::NativeSessionKey,
    shared_view::{ReadEpoch, ViewError},
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};
use tokio::sync::Notify;

pub(crate) struct Frame<R> {
    pub raw: R,
    pub sequence: u64,
    pub receipt_order: u64,
    pub context: super::read_observation::ReadObservation,
}
pub(crate) struct RouteMarker {
    pub key: NativeSessionKey,
    pub read: Option<ReadEpoch>,
    pub version: u64,
    pub cut: u64,
    pub rpc: Option<super::read_control::RpcScalar>,
    pub restore: Option<super::send_epoch::RestoredDisplayTarget>,
}
pub(crate) struct ReadCompleteMarker {
    pub key: NativeSessionKey,
    pub read: ReadEpoch,
    pub sequence: u64,
    pub receipt_order: u64,
    pub version: u64,
    pub prefix: Arc<super::read_prefix::ReadPrefix>,
}
pub(crate) enum ReadFailureEvidence<R> {
    Response {
        raw: R,
        sequence: u64,
        order: u64,
    },
    Transport(Arc<dyn std::any::Any + Send + Sync>),
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    Lifecycle(Arc<dyn std::any::Any + Send + Sync>),
}
pub(crate) struct ReadFailureMarker<R> {
    pub key: NativeSessionKey,
    pub read: ReadEpoch,
    pub rpc: super::read_control::RpcScalar,
    pub route: u64,
    pub cut: u64,
    pub version: u64,
    pub error: ViewError,
    pub evidence: ReadFailureEvidence<R>,
    pub received_cut: u64,
}
struct ReadRoute {
    key: NativeSessionKey,
    read: ReadEpoch,
    rpc: super::read_control::RpcScalar,
    version: u64,
    cut: u64,
}
pub(crate) enum SourceEntry<R> {
    Raw(Frame<R>),
    Bind(RouteMarker),
    ReadComplete(ReadCompleteMarker),
    ReadFailure(ReadFailureMarker<R>),
}

pub(crate) struct SourceBatch<R> {
    queue: VecDeque<SourceEntry<R>>,
    fallback: Option<ReadFailureMarker<R>>,
}
impl<R> SourceBatch<R> {
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.fallback.is_none()
    }
}
pub(crate) struct SourceBatchIter<R>(SourceBatch<R>);
impl<R> Iterator for SourceBatchIter<R> {
    type Item = SourceEntry<R>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(marker) = self.0.fallback.as_ref() {
            let before = self.0.queue.front().is_none_or(|entry| match entry {
                SourceEntry::Raw(frame) => frame.sequence > marker.received_cut,
                SourceEntry::Bind(next) => next.version > marker.version,
                SourceEntry::ReadComplete(next) => next.version > marker.version,
                SourceEntry::ReadFailure(next) => next.version > marker.version,
            });
            if before {
                return self.0.fallback.take().map(SourceEntry::ReadFailure);
            }
        }
        self.0.queue.pop_front()
    }
}
impl<R> IntoIterator for SourceBatch<R> {
    type Item = SourceEntry<R>;
    type IntoIter = SourceBatchIter<R>;
    fn into_iter(self) -> Self::IntoIter {
        SourceBatchIter(self)
    }
}

struct State<R> {
    queue: Mutex<VecDeque<SourceEntry<R>>>,
    failure_fallback: Mutex<Option<ReadFailureMarker<R>>>,
    retained: AtomicU64,
    observed: AtomicU64,
    prefixes: super::read_prefix::ReadPrefixRegistry,
    processed: AtomicU64,
    missing: AtomicU64,
    failure: AtomicU8,
    routing_failure: AtomicU8,
    display_fault_pending: AtomicBool,
    sealed: Arc<AtomicBool>,
    route: AtomicU64,
    processed_route: AtomicU64,
    wake: Arc<Notify>,
}
impl<R> State<R> {
    fn fail(&self, error: ViewError, sequence: Option<u64>) {
        let code = match error {
            ViewError::AllocationFailure => 1,
            ViewError::InvalidScope => 2,
            ViewError::SequenceExhausted => 3,
            _ => 4,
        };
        if self
            .failure
            .compare_exchange(0, 255, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if let Some(seq) = sequence {
                self.missing.store(seq, Ordering::Relaxed);
            }
            self.failure.store(code, Ordering::Release);
        }
        self.wake.notify_waiters();
        self.display_fault_pending.store(true, Ordering::Release);
        self.wake.notify_waiters();
    }
    fn route_fail(&self, error: ViewError) {
        let code = match error {
            ViewError::AllocationFailure => 1,
            ViewError::RandomUnavailable => 2,
            ViewError::SequenceExhausted => 3,
            _ => 4,
        };
        let _ = self
            .routing_failure
            .compare_exchange(0, code, Ordering::AcqRel, Ordering::Acquire);
        self.display_fault_pending.store(true, Ordering::Release);
        self.wake.notify_waiters();
    }
    fn route_error(&self) -> Option<ViewError> {
        match self.routing_failure.load(Ordering::Acquire) {
            0 => None,
            1 => Some(ViewError::AllocationFailure),
            2 => Some(ViewError::RandomUnavailable),
            3 => Some(ViewError::SequenceExhausted),
            _ => Some(ViewError::ReadFailure),
        }
    }
    fn error(&self) -> Option<ViewError> {
        match self.failure.load(Ordering::Acquire) {
            0 | 255 => None,
            1 => Some(ViewError::AllocationFailure),
            2 => Some(ViewError::InvalidScope),
            3 => Some(ViewError::SequenceExhausted),
            _ => Some(ViewError::CaptureGap),
        }
    }
}
pub struct SourceProducer<S, R> {
    scope: S,
    state: Arc<State<R>>,
    last_sequence: u64,
    reads: Vec<ReadRoute>,
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    controller: Option<super::controller::ViewTestController<S>>,
}
pub struct SourceConsumer<R> {
    state: Arc<State<R>>,
}
pub struct IngressProbe<R> {
    state: Arc<State<R>>,
}
impl<R> Clone for IngressProbe<R> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrainReport {
    pub processed: usize,
    pub last_sequence: u64,
    pub first_failure: Option<ViewError>,
}

/// Creates storage ports only; the protocol adapter supplies the real scope and received data.
#[must_use]
pub fn source_ingress<S, R>(
    scope: S,
) -> (SourceProducer<S, R>, SourceConsumer<R>, IngressProbe<R>) {
    let state = Arc::new(State {
        queue: Mutex::new(VecDeque::new()),
        failure_fallback: Mutex::new(None),
        retained: AtomicU64::new(0),
        observed: AtomicU64::new(0),
        prefixes: super::read_prefix::ReadPrefixRegistry::new(),
        processed: AtomicU64::new(0),
        missing: AtomicU64::new(0),
        failure: AtomicU8::new(0),
        routing_failure: AtomicU8::new(0),
        display_fault_pending: AtomicBool::new(false),
        sealed: Arc::new(AtomicBool::new(false)),
        route: AtomicU64::new(0),
        processed_route: AtomicU64::new(0),
        wake: Arc::new(Notify::new()),
    });
    (
        SourceProducer {
            scope,
            state: Arc::clone(&state),
            last_sequence: 0,
            reads: Vec::new(),
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            controller: None,
        },
        SourceConsumer {
            state: Arc::clone(&state),
        },
        IngressProbe { state },
    )
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> SourceProducer<S, R> {
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn install_controller(
        &mut self,
        controller: super::controller::ViewTestController<S>,
    ) -> Result<(), ViewError> {
        if self.controller.is_some() {
            return Err(ViewError::AlreadyAttached);
        }
        self.controller = Some(controller);
        Ok(())
    }
    pub(crate) const fn scope(&self) -> &S {
        &self.scope
    }
    pub(crate) fn probe(&self) -> IngressProbe<R> {
        IngressProbe {
            state: Arc::clone(&self.state),
        }
    }
    pub(crate) fn retain(
        &mut self,
        scope: &S,
        sequence: u64,
        receipt_order: u64,
        raw: R,
        context: super::read_observation::ReadObservation,
    ) -> Result<(), ViewError> {
        if *scope != self.scope {
            self.state
                .fail(ViewError::InvalidScope, self.last_sequence.checked_add(1));
            return Err(ViewError::InvalidScope);
        }
        if self.state.sealed.load(Ordering::Acquire) {
            self.state.observed.fetch_max(sequence, Ordering::AcqRel);
            self.state
                .prefixes
                .record_gap(sequence, ViewError::SourceReleased, &context);
            return Err(ViewError::SourceReleased);
        }
        let Some(expected) = self.last_sequence.checked_add(1) else {
            self.state.fail(ViewError::SequenceExhausted, None);
            return Err(ViewError::SequenceExhausted);
        };
        if sequence != expected {
            self.state.fail(ViewError::CaptureGap, Some(expected));
            return Err(ViewError::CaptureGap);
        }
        self.last_sequence = sequence;
        self.state.observed.store(sequence, Ordering::Release);
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        if let Some(controller) = &self.controller
            && let Err(error) = controller.fault(
                super::controller::ViewFalliblePoint::CaptureAppendReserve,
                super::controller::HookContext {
                    scope: Some(scope.clone()),
                    sequence: Some(sequence),
                    ..Default::default()
                },
            )
        {
            self.state.prefixes.record_gap(sequence, error, &context);
            self.state.fail(error, Some(sequence));
            return Err(error);
        }
        let Ok(mut queue) = self.state.queue.lock() else {
            self.state
                .prefixes
                .record_gap(sequence, ViewError::ReadFailure, &context);
            self.state.fail(ViewError::CaptureGap, Some(sequence));
            return Err(ViewError::ReadFailure);
        };
        if queue.try_reserve(1).is_err() {
            drop(queue);
            self.state
                .prefixes
                .record_gap(sequence, ViewError::AllocationFailure, &context);
            self.state
                .fail(ViewError::AllocationFailure, Some(sequence));
            return Err(ViewError::AllocationFailure);
        }
        queue.push_back(SourceEntry::Raw(Frame {
            raw,
            sequence,
            receipt_order,
            context,
        }));
        self.state.retained.store(sequence, Ordering::Release);
        drop(queue);
        self.state.wake.notify_waiters();
        Ok(())
    }
    pub fn seal(&self) {
        self.state.sealed.store(true, Ordering::Release);
        self.state.wake.notify_waiters();
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "The exclusive producer serializes route markers with captured frames"
    )]
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn bind_route(
        &mut self,
        key: NativeSessionKey,
        read: Option<ReadEpoch>,
    ) -> Result<(u64, u64), ViewError> {
        self.bind_read_route(key, read, None)
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "Only the exclusive producer publishes binding markers into its capture FIFO"
    )]
    pub(crate) fn bind_read_route(
        &mut self,
        key: NativeSessionKey,
        read: Option<ReadEpoch>,
        rpc: Option<super::read_control::RpcScalar>,
    ) -> Result<(u64, u64), ViewError> {
        self.publish_route(key, read, rpc, None)
    }
    pub(crate) fn restore_route(
        &mut self,
        key: NativeSessionKey,
        restored: super::send_epoch::RestoredDisplayTarget,
    ) -> Result<(u64, u64), ViewError> {
        self.publish_route(key, None, None, Some(restored))
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "Only the exclusive producer publishes ordered capture markers"
    )]
    fn publish_route(
        &mut self,
        key: NativeSessionKey,
        read: Option<ReadEpoch>,
        rpc: Option<super::read_control::RpcScalar>,
        restore: Option<super::send_epoch::RestoredDisplayTarget>,
    ) -> Result<(u64, u64), ViewError> {
        if self.state.sealed.load(Ordering::Acquire) {
            return Err(ViewError::SourceReleased);
        }
        let version = self
            .state
            .route
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        let cut = self.state.retained.load(Ordering::Acquire);
        if read.is_some() && rpc.is_some() {
            self.reads
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
        }
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        if let Some(controller) = &self.controller
            && let Err(error) = controller.fault(
                super::controller::ViewFalliblePoint::RouteMarkerReserve,
                super::controller::HookContext {
                    scope: Some(self.scope.clone()),
                    target: Some(key.clone()),
                    read,
                    ..Default::default()
                },
            )
        {
            self.state.route_fail(error);
            return Err(error);
        }
        let mut queue = self
            .state
            .queue
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if queue.try_reserve(1).is_err() {
            drop(queue);
            self.state.route_fail(ViewError::AllocationFailure);
            return Err(ViewError::AllocationFailure);
        }
        queue.push_back(SourceEntry::Bind(RouteMarker {
            key: key.clone(),
            read,
            version,
            cut,
            rpc: rpc.clone(),
            restore,
        }));
        if let (Some(read), Some(rpc)) = (read, rpc) {
            self.reads.push(ReadRoute {
                key,
                read,
                rpc,
                version,
                cut,
            });
        }
        self.state.route.store(version, Ordering::Release);
        drop(queue);
        self.state.wake.notify_waiters();
        Ok((version, cut))
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "The exclusive producer linearizes failure evidence with its original route and raw FIFO"
    )]
    pub(crate) fn fail_read(
        &mut self,
        key: &NativeSessionKey,
        read: ReadEpoch,
        rpc: &super::read_control::RpcScalar,
        error: ViewError,
        evidence: ReadFailureEvidence<R>,
    ) -> Result<u64, ViewError> {
        let original = self
            .reads
            .iter()
            .find(|original| original.key == *key && original.read == read && original.rpc == *rpc)
            .ok_or(ViewError::InvalidStage)?;
        let version = self
            .state
            .route
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        let marker = ReadFailureMarker {
            key: original.key.clone(),
            read: original.read,
            rpc: original.rpc.clone(),
            route: original.version,
            cut: original.cut,
            version,
            error,
            evidence,
            received_cut: self.state.observed.load(Ordering::Acquire),
        };
        #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
        if let Some(controller) = &self.controller
            && let Err(fault) = controller.fault(
                super::controller::ViewFalliblePoint::RouteMarkerReserve,
                super::controller::HookContext {
                    scope: Some(self.scope.clone()),
                    target: Some(key.clone()),
                    read: Some(read),
                    ..Default::default()
                },
            )
        {
            return self.fallback_failure(marker, fault);
        }
        let mut queue = self
            .state
            .queue
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if queue.try_reserve(1).is_err() {
            drop(queue);
            return self.fallback_failure(marker, ViewError::AllocationFailure);
        }
        queue.push_back(SourceEntry::ReadFailure(marker));
        self.state.route.store(version, Ordering::Release);
        drop(queue);
        self.state.wake.notify_waiters();
        Ok(version)
    }
    fn fallback_failure(
        &self,
        marker: ReadFailureMarker<R>,
        error: ViewError,
    ) -> Result<u64, ViewError> {
        let version = marker.version;
        let mut fallback = self
            .state
            .failure_fallback
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if fallback.is_some() {
            drop(fallback);
            self.state.route_fail(error);
            return Err(error);
        }
        *fallback = Some(marker);
        self.state.route.store(version, Ordering::Release);
        drop(fallback);
        self.state.route_fail(error);
        Ok(version)
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "The exclusive producer serializes the original completion marker with captured frames"
    )]
    pub(crate) fn complete_read(
        &mut self,
        key: NativeSessionKey,
        read: ReadEpoch,
        sequence: u64,
        receipt_order: u64,
        prefix: Arc<super::read_prefix::ReadPrefix>,
    ) -> Result<u64, ViewError> {
        if self.state.sealed.load(Ordering::Acquire)
            || self.state.retained.load(Ordering::Acquire) < sequence
        {
            return Err(ViewError::CaptureGap);
        }
        let version = self
            .state
            .route
            .load(Ordering::Acquire)
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        let mut queue = self
            .state
            .queue
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        if queue.try_reserve(1).is_err() {
            drop(queue);
            self.state.route_fail(ViewError::AllocationFailure);
            return Err(ViewError::AllocationFailure);
        }
        queue.push_back(SourceEntry::ReadComplete(ReadCompleteMarker {
            key,
            read,
            sequence,
            receipt_order,
            version,
            prefix,
        }));
        self.state.route.store(version, Ordering::Release);
        drop(queue);
        self.state.wake.notify_waiters();
        Ok(version)
    }
}
impl<S, R> Drop for SourceProducer<S, R> {
    fn drop(&mut self) {
        self.state.sealed.store(true, Ordering::Release);
        self.state.wake.notify_waiters();
    }
}
impl<R: RetainedRaw> SourceConsumer<R> {
    /// Detaches the whole queue before any journal allocation or optional projection.
    /// # Errors
    /// Reports an unusable ingress mutex; original control ownership remains elsewhere.
    pub fn drain_into(&mut self, journal: &mut MemoryJournal<R>) -> Result<DrainReport, ViewError> {
        let batch = {
            let mut q = self
                .state
                .queue
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            std::mem::take(&mut *q)
        };
        let mut report = DrainReport {
            processed: 0,
            last_sequence: self.state.processed.load(Ordering::Acquire),
            first_failure: None,
        };
        for frame in batch {
            let SourceEntry::Raw(frame) = frame else {
                self.state.fail(ViewError::CaptureGap, None);
                return Err(ViewError::CaptureGap);
            };
            if let Err(error) = journal.append_raw(frame.raw, frame.sequence, frame.receipt_order) {
                self.state
                    .prefixes
                    .record_gap(frame.sequence, error, &frame.context);
                self.state.fail(error, Some(frame.sequence));
                report.first_failure.get_or_insert(error);
            }
            report.processed += 1;
            report.last_sequence = frame.sequence;
            self.state
                .processed
                .store(frame.sequence, Ordering::Release);
        }
        self.state.wake.notify_waiters();
        Ok(report)
    }
    #[allow(
        clippy::needless_pass_by_ref_mut,
        reason = "Only the exclusive consumer can detach its remaining queue for owned cleanup"
    )]
    pub(crate) fn detach(&mut self) -> Result<SourceBatch<R>, ViewError> {
        let mut queued = self
            .state
            .queue
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        let queue = std::mem::take(&mut *queued);
        drop(queued);
        let fallback = self
            .state
            .failure_fallback
            .lock()
            .map_err(|_| ViewError::ReadFailure)?
            .take();
        Ok(SourceBatch { queue, fallback })
    }
    pub(crate) fn processed(
        &self,
        sequence: u64,
        result: Result<(), ViewError>,
        context: &super::read_observation::ReadObservation,
    ) {
        if let Err(error) = result {
            self.state.prefixes.record_gap(sequence, error, context);
            self.state.fail(error, Some(sequence));
        }
        self.state.processed.store(sequence, Ordering::Release);
        self.state.wake.notify_waiters();
    }
    pub(crate) fn routed(&self, version: u64) {
        self.state.processed_route.store(version, Ordering::Release);
        self.state.wake.notify_waiters();
    }
    pub(crate) fn route_failed(&self, error: ViewError) {
        self.state.route_fail(error);
    }
    pub(crate) fn take_display_fault(&self) -> Option<ViewError> {
        if self
            .state
            .display_fault_pending
            .swap(false, Ordering::AcqRel)
        {
            self.state.error().or_else(|| self.state.route_error())
        } else {
            None
        }
    }
    pub(crate) async fn wait_work(&self) {
        loop {
            let changed = self.state.wake.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let queued = self.state.queue.lock().map_or(true, |q| !q.is_empty());
            let fallback = self
                .state
                .failure_fallback
                .lock()
                .map_or(true, |slot| slot.is_some());
            if queued
                || fallback
                || self.state.sealed.load(Ordering::Acquire)
                || self.state.display_fault_pending.load(Ordering::Acquire)
            {
                return;
            }
            changed.await;
        }
    }
}
impl<R> IngressProbe<R> {
    pub(crate) fn observed_frontier(&self) -> u64 {
        self.state.observed.load(Ordering::Acquire)
    }
    pub(crate) fn track_prefix(
        &self,
        identity: super::read_prefix::ReadPrefixIdentity,
    ) -> Result<Arc<super::read_prefix::ReadPrefix>, ViewError> {
        self.state
            .prefixes
            .track(identity, self.observed_frontier())
    }
    pub(crate) fn sealed_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.state.sealed)
    }
    pub(crate) fn seal_signal(&self) -> (Arc<AtomicBool>, Arc<Notify>) {
        (Arc::clone(&self.state.sealed), Arc::clone(&self.state.wake))
    }
    #[must_use]
    pub fn route_version(&self) -> u64 {
        self.state.route.load(Ordering::Acquire)
    }
    #[must_use]
    pub fn processed_route_version(&self) -> u64 {
        self.state.processed_route.load(Ordering::Acquire)
    }
    pub(crate) fn seal(&self) {
        self.state.sealed.store(true, Ordering::Release);
        self.state.wake.notify_waiters();
    }
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        self.state.sealed.load(Ordering::Acquire)
    }
    #[must_use]
    pub fn retained_frontier(&self) -> u64 {
        self.state.retained.load(Ordering::Acquire)
    }
    #[must_use]
    pub fn processed_frontier(&self) -> u64 {
        self.state.processed.load(Ordering::Acquire)
    }
    #[must_use]
    pub fn first_missing_sequence(&self) -> Option<u64> {
        let n = self.state.missing.load(Ordering::Acquire);
        if n == 0 { None } else { Some(n) }
    }
    #[must_use]
    pub fn failure(&self) -> Option<ViewError> {
        self.state.error()
    }
    pub(crate) fn route_failure(&self) -> Option<ViewError> {
        self.state.route_error()
    }
    /// Registers the notification before checking the actual processed frontier.
    /// # Errors
    /// Returns an actual sticky capture fault or a source sealed before reaching this cut.
    pub async fn wait_processed(&self, cut: u64) -> Result<(), ViewError> {
        loop {
            let notified = self.state.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(error) = self.state.error()
                && self
                    .first_missing_sequence()
                    .is_none_or(|missing| missing <= cut)
            {
                return Err(error);
            }
            if self.processed_frontier() >= cut {
                return Ok(());
            }
            if self.state.sealed.load(Ordering::Acquire) {
                return Err(ViewError::SourceReleased);
            }
            notified.await;
        }
    }
    pub(crate) async fn wait_barrier(&self, cut: u64, route: u64) -> Result<(), ViewError> {
        loop {
            let changed = self.state.wake.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.processed_frontier() >= cut && self.processed_route_version() >= route {
                return Ok(());
            }
            if self.is_sealed() {
                return Err(ViewError::SourceReleased);
            }
            changed.await;
        }
    }
}
