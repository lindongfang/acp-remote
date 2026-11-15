//! Default-off read-only observations of the same actual source and registered consumers.
use super::{
    RetainedRaw,
    source::SharedViews,
    tasks::{ViewTaskKind, ViewTaskSupervisor},
};
use crate::domain::{
    EntityId,
    native::NativeSessionKey,
    shared_view::{
        AnchorId, RawRecordId, ReadEpoch, ViewDescriptor, ViewEpoch, ViewError, ViewRevision,
    },
};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewTaskState {
    Registered,
    Running,
    Sealing,
    Joined,
    JoinUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawNamespace {
    Message,
    Tool,
    Unknown,
}
pub struct SourceInspection<S> {
    scope: S,
    retained: u64,
    processed: u64,
    missing: Option<u64>,
    route: u64,
    processed_route: u64,
    state: ViewTaskState,
}
impl<S> SourceInspection<S> {
    #[must_use]
    pub const fn scope(&self) -> &S {
        &self.scope
    }
    #[must_use]
    pub const fn retained_frontier(&self) -> u64 {
        self.retained
    }
    #[must_use]
    pub const fn processed_frontier(&self) -> u64 {
        self.processed
    }
    #[must_use]
    pub const fn first_missing_sequence(&self) -> Option<u64> {
        self.missing
    }
    #[must_use]
    pub const fn route_version(&self) -> u64 {
        self.route
    }
    #[must_use]
    pub const fn processed_route_version(&self) -> u64 {
        self.processed_route
    }
    #[must_use]
    pub const fn task_state(&self) -> ViewTaskState {
        self.state
    }
}
#[derive(Clone)]
pub struct AnchorMetadata {
    segment: ViewEpoch,
    id: AnchorId,
    namespace: RawNamespace,
    first: u64,
    last: ViewRevision,
    raw: Vec<RawRecordId>,
}
impl AnchorMetadata {
    #[must_use]
    pub const fn segment_id(&self) -> EntityId {
        self.segment.as_entity_id()
    }
    #[must_use]
    pub const fn segment_view_epoch(&self) -> ViewEpoch {
        self.segment
    }
    #[must_use]
    pub const fn namespace(&self) -> RawNamespace {
        self.namespace
    }
    #[must_use]
    pub const fn first_position(&self) -> u64 {
        self.first
    }
    #[must_use]
    pub const fn last_revision(&self) -> ViewRevision {
        self.last
    }
    #[must_use]
    pub fn raw_records(&self) -> &[RawRecordId] {
        &self.raw
    }
    #[must_use]
    pub const fn anchor_id(&self) -> AnchorId {
        self.id
    }
}
pub struct TargetInspection {
    descriptor: ViewDescriptor,
    anchors: Vec<AnchorMetadata>,
}
impl TargetInspection {
    #[must_use]
    pub const fn descriptor(&self) -> ViewDescriptor {
        self.descriptor
    }
    #[must_use]
    pub const fn anchor_count(&self) -> usize {
        self.anchors.len()
    }
    #[must_use]
    pub fn anchor(&self, id: AnchorId) -> Option<AnchorMetadata> {
        self.anchors
            .get(id.as_usize())
            .filter(|anchor| anchor.id == id)
            .cloned()
    }
    #[must_use]
    pub fn anchors(&self) -> &[AnchorMetadata] {
        &self.anchors
    }
}
#[derive(Clone, Copy)]
pub struct StreamInspection {
    pub(crate) stream: EntityId,
    pub(crate) epoch: ViewEpoch,
    pub(crate) read: Option<ReadEpoch>,
    pub(crate) observed: super::local_transfer::LocalTransferObservation,
    pub(crate) snapshot: super::reader::SnapshotCursor,
    pub(crate) delta: super::reader::DeltaCursor,
}
impl StreamInspection {
    #[must_use]
    pub const fn stream_id(self) -> EntityId {
        self.stream
    }
    #[must_use]
    pub const fn view_epoch(self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn read_epoch(self) -> Option<ReadEpoch> {
        self.read
    }
    #[must_use]
    pub const fn highest_started(self) -> u64 {
        self.observed.highest_started
    }
    #[must_use]
    pub const fn highest_flushed(self) -> u64 {
        self.observed.highest_flushed
    }
    #[must_use]
    pub const fn highest_committed(self) -> u64 {
        self.observed.highest_committed
    }
    #[must_use]
    pub const fn pending_early_credit(self) -> bool {
        self.observed.pending_early_credit
    }
    #[must_use]
    pub const fn snapshot_cursor(self) -> super::reader::SnapshotCursor {
        self.snapshot
    }
    #[must_use]
    pub const fn delta_cursor(self) -> super::reader::DeltaCursor {
        self.delta
    }
}
pub(crate) trait StreamInspectionPort: Send + Sync {
    fn inspect(&self) -> Option<StreamInspection>;
}
trait ProbePort<S>: Send + Sync {
    fn source(&self, scope: &S) -> Result<SourceInspection<S>, ViewError>;
    fn target(&self, key: &NativeSessionKey) -> Result<TargetInspection, ViewError>;
    fn stream(&self, id: EntityId) -> Option<StreamInspection>;
}
pub struct SharedViewProbe<S> {
    port: Arc<dyn ProbePort<S>>,
}
impl<S> SharedViewProbe<S> {
    /// # Errors
    /// Rejects a scope other than the original slot's actual scope.
    pub fn source(&self, scope: &S) -> Result<SourceInspection<S>, ViewError> {
        self.port.source(scope)
    }
    /// # Errors
    /// Rejects released/unbound targets and actual metadata allocation failure.
    pub fn target(&self, key: &NativeSessionKey) -> Result<TargetInspection, ViewError> {
        self.port.target(key)
    }
    #[must_use]
    pub fn stream(&self, id: EntityId) -> Option<StreamInspection> {
        self.port.stream(id)
    }
}
struct SourceProbe<S, R> {
    views: SharedViews<S, R>,
    supervisor: ViewTaskSupervisor,
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> super::SharedViewOwner<S, R> {
    #[must_use]
    pub fn test_probe(&self) -> SharedViewProbe<S> {
        SharedViewProbe {
            port: Arc::new(SourceProbe {
                views: self.handle(),
                supervisor: self.supervisor.clone(),
            }),
        }
    }
}
impl<S: Clone + Eq + Send + Sync + 'static, R: RetainedRaw> ProbePort<S> for SourceProbe<S, R> {
    fn source(&self, scope: &S) -> Result<SourceInspection<S>, ViewError> {
        if *scope != self.views.core.scope {
            return Err(ViewError::InvalidScope);
        }
        let probe = &self.views.core.probe;
        Ok(SourceInspection {
            scope: self.views.core.scope.clone(),
            retained: probe.retained_frontier(),
            processed: probe.processed_frontier(),
            missing: probe.first_missing_sequence(),
            route: probe.route_version(),
            processed_route: probe.processed_route_version(),
            state: self.supervisor.task_state(ViewTaskKind::Worker),
        })
    }
    fn target(&self, key: &NativeSessionKey) -> Result<TargetInspection, ViewError> {
        let generation = self.views.core.target(key)?;
        let snapshot = Arc::clone(
            &*generation
                .snapshot
                .read()
                .map_err(|_| ViewError::ReadFailure)?,
        );
        let mut anchors = Vec::new();
        anchors
            .try_reserve(snapshot.projection.anchors.len())
            .map_err(|_| ViewError::AllocationFailure)?;
        for index in 0..snapshot.projection.anchors.len() {
            let anchor = snapshot.projection.anchors.get(index)?;
            let versions = anchor
                .versions
                .read()
                .map_err(|_| ViewError::ReadFailure)?
                .clone();
            let mut raw = Vec::new();
            let mut last = anchor.first_revision;
            for position in 0..versions.len() {
                let version = versions.get(position)?;
                if version.revision() > snapshot.projection.revision {
                    break;
                }
                raw.try_reserve(1)
                    .map_err(|_| ViewError::AllocationFailure)?;
                raw.push(version.raw_record());
                last = version.revision();
            }
            anchors.push(AnchorMetadata {
                segment: anchor.segment,
                id: AnchorId(index),
                namespace: match anchor.namespace {
                    Some(1) => RawNamespace::Message,
                    Some(2) => RawNamespace::Tool,
                    _ => RawNamespace::Unknown,
                },
                first: anchor.first_revision,
                last: ViewRevision(last),
                raw,
            });
        }
        Ok(TargetInspection {
            descriptor: snapshot.descriptor,
            anchors,
        })
    }
    fn stream(&self, id: EntityId) -> Option<StreamInspection> {
        let streams = self.views.core.streams.lock().ok()?;
        let stream = streams
            .iter()
            .find(|(stream, _)| *stream == id)?
            .1
            .upgrade()?;
        drop(streams);
        stream.inspect()
    }
}
