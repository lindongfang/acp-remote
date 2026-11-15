//! Default-off, instance-owned observations and single-use controlled fault paths.
//! These data selectors neither create native facts nor change authorization.
use crate::{
    domain::{
        EntityId,
        native::NativeSessionKey,
        shared_view::{ReadEpoch, ViewEpoch, ViewError, ViewRevision},
    },
    shared_view_test_support::FixtureError,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewHookPoint {
    AfterCaptureEnqueue,
    BeforeWorkerDetach,
    AfterWorkerCutProcessed,
    AfterSnapshotH,
    BeforeSnapshotEnd,
    BeforeChunkCopy,
    AfterChunkQueuedBeforeWriterGate,
    WriterGateHeld,
    BeforeSinkReady,
    AfterStartSendBeforeFlush,
    AfterFlushFactBeforePublish,
    BeforeCommitCursor,
    BeforeViewWorkerJoin,
    AfterModuleCreditReadBeforePublish,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewFalliblePoint {
    CaptureAppendReserve,
    RouteMarkerReserve,
    ProjectionMetadataReserve,
    ReadChunkReserve,
    NewViewEpochRandom,
    NewReadEpochRandom,
    NewStreamIdRandom,
}
#[derive(Clone, Eq, PartialEq)]
pub enum HookSelector<S> {
    Any,
    Source(S),
    Target(NativeSessionKey),
    Stream(EntityId),
    Read(ReadEpoch),
}
#[derive(Clone)]
pub(crate) struct HookContext<S> {
    pub scope: Option<S>,
    pub target: Option<NativeSessionKey>,
    pub sequence: Option<u64>,
    pub epoch: Option<ViewEpoch>,
    pub stream: Option<EntityId>,
    pub packet: Option<u64>,
    pub revision: Option<ViewRevision>,
    pub read: Option<ReadEpoch>,
}
impl<S> Default for HookContext<S> {
    fn default() -> Self {
        Self {
            scope: None,
            target: None,
            sequence: None,
            epoch: None,
            stream: None,
            packet: None,
            revision: None,
            read: None,
        }
    }
}
#[derive(Clone)]
pub struct HookObservation<S> {
    point: ViewHookPoint,
    context: HookContext<S>,
}
#[derive(Clone)]
pub struct ViewFaultObservation<S> {
    point: ViewFalliblePoint,
    context: HookContext<S>,
}
impl<S> HookObservation<S> {
    #[must_use]
    pub const fn point(&self) -> ViewHookPoint {
        self.point
    }
    #[must_use]
    pub const fn scope(&self) -> Option<&S> {
        self.context.scope.as_ref()
    }
    #[must_use]
    pub const fn sequence(&self) -> Option<u64> {
        self.context.sequence
    }
    #[must_use]
    pub const fn view_epoch(&self) -> Option<ViewEpoch> {
        self.context.epoch
    }
    #[must_use]
    pub const fn stream_id(&self) -> Option<EntityId> {
        self.context.stream
    }
    #[must_use]
    pub const fn packet_sequence(&self) -> Option<u64> {
        self.context.packet
    }
    #[must_use]
    pub const fn snapshot_revision(&self) -> Option<ViewRevision> {
        self.context.revision
    }
}
impl<S> ViewFaultObservation<S> {
    #[must_use]
    pub const fn point(&self) -> ViewFalliblePoint {
        self.point
    }
    #[must_use]
    pub const fn scope(&self) -> Option<&S> {
        self.context.scope.as_ref()
    }
    #[must_use]
    pub const fn sequence(&self) -> Option<u64> {
        self.context.sequence
    }
    #[must_use]
    pub const fn stream_id(&self) -> Option<EntityId> {
        self.context.stream
    }
    #[must_use]
    pub const fn read_epoch(&self) -> Option<ReadEpoch> {
        self.context.read
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Point {
    Hook(ViewHookPoint),
    Fault(ViewFalliblePoint),
}
#[derive(Clone)]
enum Arrived<S> {
    Hook(HookObservation<S>),
    Fault(ViewFaultObservation<S>),
}
struct Arm<S> {
    id: u64,
    point: Point,
    selector: HookSelector<S>,
    pause: bool,
    claimed: AtomicBool,
    released: watch::Sender<bool>,
    arrived: watch::Sender<Option<Arrived<S>>>,
}
struct Controller<S> {
    closed: AtomicBool,
    nonce: AtomicU64,
    arms: Mutex<Vec<Arc<Arm<S>>>>,
}
pub struct ViewTestController<S> {
    inner: Arc<Controller<S>>,
}
impl<S> Clone for ViewTestController<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
pub struct ViewPauseGate<S> {
    inner: Arc<Controller<S>>,
    arm: Arc<Arm<S>>,
}
pub struct ViewObservationWatch<S> {
    inner: Arc<Controller<S>>,
    arm: Arc<Arm<S>>,
}
pub struct ViewFaultArm<S> {
    inner: Arc<Controller<S>>,
    arm: Arc<Arm<S>>,
}
fn release<S>(inner: &Controller<S>, arm: &Arm<S>) {
    arm.released.send_replace(true);
    if let Ok(mut arms) = inner.arms.lock() {
        arms.retain(|current| current.id != arm.id);
    }
}
impl<S> Drop for ViewPauseGate<S> {
    fn drop(&mut self) {
        release(&self.inner, &self.arm);
    }
}
impl<S> Drop for ViewObservationWatch<S> {
    fn drop(&mut self) {
        release(&self.inner, &self.arm);
    }
}
impl<S> Drop for ViewFaultArm<S> {
    fn drop(&mut self) {
        release(&self.inner, &self.arm);
    }
}
async fn arrived<S: Clone + Send + Sync>(arm: &Arm<S>) -> Result<Arrived<S>, FixtureError> {
    let mut result = arm.arrived.subscribe();
    let mut released = arm.released.subscribe();
    loop {
        let current = result.borrow().clone();
        if let Some(value) = current {
            return Ok(value);
        }
        if *released.borrow() {
            return Err(FixtureError::Cancelled);
        }
        tokio::select! {
            change=result.changed()=>change.map_err(|_|FixtureError::Cancelled)?,
            change=released.changed()=>change.map_err(|_|FixtureError::Cancelled)?,
        }
    }
}
impl<S: Clone + Send + Sync> ViewPauseGate<S> {
    /// # Errors
    /// Reports cancellation of this original arm or an incompatible observation.
    pub async fn arrived(&mut self) -> Result<HookObservation<S>, FixtureError> {
        match arrived(&self.arm).await? {
            Arrived::Hook(value) => Ok(value),
            Arrived::Fault(_) => Err(FixtureError::Unsupported),
        }
    }
    pub fn release(self) {}
}
impl<S: Clone + Send + Sync> ViewObservationWatch<S> {
    /// # Errors
    /// Reports owner cancellation; cancelling this waiter does not remove its arm.
    pub async fn arrived(&mut self) -> Result<HookObservation<S>, FixtureError> {
        match arrived(&self.arm).await? {
            Arrived::Hook(value) => Ok(value),
            Arrived::Fault(_) => Err(FixtureError::Unsupported),
        }
    }
}
impl<S: Clone + Send + Sync> ViewFaultArm<S> {
    /// # Errors
    /// Reports owner cancellation before this real named fault path is reached.
    pub async fn triggered(&mut self) -> Result<ViewFaultObservation<S>, FixtureError> {
        match arrived(&self.arm).await? {
            Arrived::Fault(value) => Ok(value),
            Arrived::Hook(_) => Err(FixtureError::Unsupported),
        }
    }
    pub fn disarm(self) {}
}
impl<S: Clone + Eq + Send + Sync + 'static> ViewTestController<S> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Controller {
                closed: AtomicBool::new(false),
                nonce: AtomicU64::new(0),
                arms: Mutex::new(Vec::new()),
            }),
        }
    }
    fn arm(
        &self,
        point: Point,
        selector: HookSelector<S>,
        pause: bool,
    ) -> Result<Arc<Arm<S>>, FixtureError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(FixtureError::Cancelled);
        }
        let mut arms = self
            .inner
            .arms
            .lock()
            .map_err(|_| FixtureError::JoinUnconfirmed)?;
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(FixtureError::Cancelled);
        }
        if arms
            .iter()
            .any(|arm| arm.point == point && arm.selector == selector && !*arm.released.borrow())
        {
            return Err(FixtureError::ControlBusy);
        }
        arms.try_reserve(1).map_err(|_| FixtureError::Allocation)?;
        let id = self
            .inner
            .nonce
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| FixtureError::Allocation)?;
        let (released, _) = watch::channel(false);
        let (arrived, _) = watch::channel(None);
        let arm = Arc::new(Arm {
            id,
            point,
            selector,
            pause,
            claimed: AtomicBool::new(false),
            released,
            arrived,
        });
        arms.push(Arc::clone(&arm));
        drop(arms);
        Ok(arm)
    }
    /// # Errors
    /// Rejects an observe-only point, an occupied arm or a stopped owner.
    pub fn arm_pause(
        &self,
        point: ViewHookPoint,
        selector: HookSelector<S>,
    ) -> Result<ViewPauseGate<S>, FixtureError> {
        if matches!(
            point,
            ViewHookPoint::WriterGateHeld | ViewHookPoint::AfterStartSendBeforeFlush
        ) {
            return Err(FixtureError::Unsupported);
        }
        Ok(ViewPauseGate {
            inner: Arc::clone(&self.inner),
            arm: self.arm(Point::Hook(point), selector, true)?,
        })
    }
    /// # Errors
    /// Rejects an occupied arm or a stopped owner without changing a running task.
    pub fn observe_once(
        &self,
        point: ViewHookPoint,
        selector: HookSelector<S>,
    ) -> Result<ViewObservationWatch<S>, FixtureError> {
        Ok(ViewObservationWatch {
            inner: Arc::clone(&self.inner),
            arm: self.arm(Point::Hook(point), selector, false)?,
        })
    }
    /// # Errors
    /// Rejects an occupied arm, an exhausted nonce or a stopped owner.
    pub fn fail_next(
        &self,
        point: ViewFalliblePoint,
        selector: HookSelector<S>,
    ) -> Result<ViewFaultArm<S>, FixtureError> {
        Ok(ViewFaultArm {
            inner: Arc::clone(&self.inner),
            arm: self.arm(Point::Fault(point), selector, false)?,
        })
    }
    fn matches(selector: &HookSelector<S>, context: &HookContext<S>) -> bool {
        match selector {
            HookSelector::Any => true,
            HookSelector::Source(source) => context.scope.as_ref() == Some(source),
            HookSelector::Target(target) => context.target.as_ref() == Some(target),
            HookSelector::Stream(stream) => context.stream == Some(*stream),
            HookSelector::Read(read) => context.read == Some(*read),
        }
    }
    fn take(&self, point: Point, context: &HookContext<S>) -> Result<Vec<Arc<Arm<S>>>, ViewError> {
        let mut arms = self.inner.arms.lock().map_err(|_| ViewError::ReadFailure)?;
        let mut selected = Vec::new();
        for arm in arms.iter() {
            if arm.point == point
                && !*arm.released.borrow()
                && Self::matches(&arm.selector, context)
                && !arm.claimed.load(Ordering::Acquire)
            {
                selected
                    .try_reserve(1)
                    .map_err(|_| ViewError::AllocationFailure)?;
                if arm
                    .claimed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    selected.push(Arc::clone(arm));
                }
            }
        }
        arms.retain(|arm| arm.pause || !arm.claimed.load(Ordering::Acquire));
        drop(arms);
        Ok(selected)
    }
    pub(crate) async fn hit(
        &self,
        point: ViewHookPoint,
        context: HookContext<S>,
    ) -> Result<(), ViewError> {
        let selected = self.take(Point::Hook(point), &context)?;
        for arm in &selected {
            arm.arrived
                .send_replace(Some(Arrived::Hook(HookObservation {
                    point,
                    context: context.clone(),
                })));
        }
        for arm in &selected {
            if arm.pause {
                let mut released = arm.released.subscribe();
                while !*released.borrow_and_update() {
                    if released.changed().await.is_err() {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Each hook owns its immutable arrival context; the frozen facade shares the same owned shape for pause and observation"
    )]
    pub(crate) fn observe(
        &self,
        point: ViewHookPoint,
        context: HookContext<S>,
    ) -> Result<(), ViewError> {
        for arm in self.take(Point::Hook(point), &context)? {
            arm.arrived
                .send_replace(Some(Arrived::Hook(HookObservation {
                    point,
                    context: context.clone(),
                })));
        }
        Ok(())
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Each real fallible point owns its immutable arrival context consistently with pause hooks"
    )]
    pub(crate) fn fault(
        &self,
        point: ViewFalliblePoint,
        context: HookContext<S>,
    ) -> Result<(), ViewError> {
        let selected = self.take(Point::Fault(point), &context)?;
        if selected.is_empty() {
            return Ok(());
        }
        for arm in selected {
            arm.arrived
                .send_replace(Some(Arrived::Fault(ViewFaultObservation {
                    point,
                    context: context.clone(),
                })));
        }
        Err(match point {
            ViewFalliblePoint::CaptureAppendReserve
            | ViewFalliblePoint::RouteMarkerReserve
            | ViewFalliblePoint::ProjectionMetadataReserve
            | ViewFalliblePoint::ReadChunkReserve => ViewError::AllocationFailure,
            ViewFalliblePoint::NewViewEpochRandom
            | ViewFalliblePoint::NewReadEpochRandom
            | ViewFalliblePoint::NewStreamIdRandom => ViewError::RandomUnavailable,
        })
    }
}
impl<S> ViewTestController<S> {
    pub(crate) fn close(&self) {
        self.inner.closed.store(true, Ordering::Release);
        if let Ok(mut arms) = self.inner.arms.lock() {
            for arm in arms.drain(..) {
                arm.released.send_replace(true);
            }
        }
    }
}
