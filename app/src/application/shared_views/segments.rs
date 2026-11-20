//! Immutable segment metadata and temporal status events, owned by the source worker.
use super::read_observation::ReadContextKind;
use crate::domain::{
    shared_view::{
        Freshness, JoinState, ProjectionState, ReadEpoch, ReadMode, ViewDescriptor, ViewEpoch,
        ViewError,
    },
    shared_view_log::{AppendLog, LogSnapshot},
};
use std::sync::{Arc, RwLock};
#[derive(Clone, Copy)]
pub enum SegmentOrigin {
    RetainedCurrent,
    FailedReplay,
    LiveObservation,
}
#[derive(Clone, Copy)]
pub enum SegmentPhase {
    Start,
    Update,
    End,
}
#[derive(Clone, Copy)]
pub struct SegmentMetadata {
    pub epoch: ViewEpoch,
    pub read: Option<ReadEpoch>,
    pub origin: SegmentOrigin,
    pub descriptor: ViewDescriptor,
    pub begin: Option<u64>,
    pub through: Option<u64>,
    pub completion: Option<u64>,
    pub open: bool,
    pub context: ReadContextKind,
    pub revision: u64,
    pub limitation: Option<&'static str>,
}
pub struct SegmentPublication {
    pub epoch: ViewEpoch,
    pub versions: RwLock<LogSnapshot<SegmentMetadata>>,
}
#[derive(Clone, Copy)]
pub struct SegmentEvent {
    pub index: usize,
    pub phase: SegmentPhase,
    pub metadata: SegmentMetadata,
}
#[derive(Clone)]
pub struct CustomStatusEvent {
    pub bytes: Vec<u8>,
    pub segment: ViewEpoch,
}
pub struct SegmentLog {
    publications: AppendLog<SegmentPublication>,
    versions: Vec<AppendLog<SegmentMetadata>>,
    events: AppendLog<SegmentEvent>,
}
pub struct SegmentSnapshot {
    pub publications: LogSnapshot<SegmentPublication>,
    pub events: LogSnapshot<SegmentEvent>,
}
impl Clone for SegmentSnapshot {
    fn clone(&self) -> Self {
        Self {
            publications: self.publications.clone(),
            events: self.events.clone(),
        }
    }
}
impl SegmentLog {
    pub fn new() -> Result<Self, ViewError> {
        Ok(Self {
            publications: AppendLog::new()?,
            versions: Vec::new(),
            events: AppendLog::new()?,
        })
    }
    pub fn append(&mut self, metadata: SegmentMetadata) -> Result<usize, ViewError> {
        self.versions
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        let mut versions = AppendLog::new()?;
        versions.append(metadata)?;
        let index = self.publications.append(SegmentPublication {
            epoch: metadata.epoch,
            versions: RwLock::new(versions.snapshot()?),
        })?;
        self.versions.push(versions);
        self.events.append(SegmentEvent {
            index,
            phase: SegmentPhase::Start,
            metadata,
        })?;
        Ok(index)
    }
    pub fn update(
        &mut self,
        index: usize,
        metadata: SegmentMetadata,
        phase: SegmentPhase,
    ) -> Result<(), ViewError> {
        let versions = self
            .versions
            .get_mut(index)
            .ok_or(ViewError::InvalidCursor)?;
        let publication = self.publications.snapshot()?.get(index)?;
        if publication.epoch != metadata.epoch {
            return Err(ViewError::InvalidCursor);
        }
        versions.append(metadata)?;
        self.events.append(SegmentEvent {
            index,
            phase,
            metadata,
        })?;
        *publication
            .versions
            .write()
            .map_err(|_| ViewError::ReadFailure)? = versions.snapshot()?;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<SegmentSnapshot, ViewError> {
        Ok(SegmentSnapshot {
            publications: self.publications.snapshot()?,
            events: self.events.snapshot()?,
        })
    }
}
impl SegmentSnapshot {
    pub fn metadata(
        &self,
        index: usize,
        highwater: u64,
    ) -> Result<Arc<SegmentMetadata>, ViewError> {
        let publication = self.publications.get(index)?;
        let versions = publication
            .versions
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .clone();
        let mut low = 0;
        let mut high = versions.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if versions.get(middle)?.revision <= highwater {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        versions.get(low.checked_sub(1).ok_or(ViewError::InvalidCursor)?)
    }
}
pub fn status(metadata: SegmentMetadata, phase: SegmentPhase) -> Result<Vec<u8>, ViewError> {
    use serde::Serialize;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Status {
        r#type: &'static str,
        phase: &'static str,
        segment_id: String,
        segment_view_epoch: String,
        read_epoch: Option<String>,
        origin: &'static str,
        read_mode: &'static str,
        freshness: &'static str,
        join: &'static str,
        source_binding_id: String,
        source_id: String,
        begin_sequence: Option<String>,
        through_sequence: Option<String>,
        failure: Option<String>,
        projection: &'static str,
        completion_response_sequence: Option<String>,
        open: bool,
        read_context: &'static str,
        relation: &'static str,
        limitation: Option<&'static str>,
    }
    let descriptor = metadata.descriptor;
    let value = Status {
        r#type: "view_segment",
        phase: match phase {
            SegmentPhase::Start => "start",
            SegmentPhase::Update => "update",
            SegmentPhase::End => "end",
        },
        segment_id: metadata.epoch.as_entity_id().encode(),
        segment_view_epoch: metadata.epoch.as_entity_id().encode(),
        read_epoch: metadata.read.map(|read| read.as_entity_id().encode()),
        origin: match metadata.origin {
            SegmentOrigin::RetainedCurrent => "retained_current",
            SegmentOrigin::FailedReplay => "failed_replay",
            SegmentOrigin::LiveObservation => "live_observation",
        },
        read_mode: match descriptor.mode {
            ReadMode::CurrentOnly => "CurrentOnly",
            ReadMode::ReplayRequested => "ReplayRequested",
            ReadMode::Replaying => "Replaying",
            ReadMode::ReplayComplete => "ReplayComplete",
            ReadMode::ReplayIncomplete => "ReplayIncomplete",
        },
        freshness: match descriptor.freshness {
            Freshness::CurrentObservation => "CurrentObservation",
            Freshness::Revalidating => "Revalidating",
            Freshness::Stale => "Stale",
        },
        join: match descriptor.join {
            JoinState::ProvenNativeIds => "ProvenNativeIds",
            JoinState::RebuiltNoStableIds => "RebuiltNoStableIds",
            JoinState::UnassociatedSegments => "UnassociatedSegments",
        },
        source_binding_id: descriptor.source.binding_id().encode(),
        source_id: descriptor.source.source().0.encode(),
        begin_sequence: metadata.begin.map(|seq| seq.to_string()),
        through_sequence: metadata.through.map(|seq| seq.to_string()),
        failure: descriptor.failure.map(|error| error.to_string()),
        projection: match descriptor.projection {
            ProjectionState::Available => "Available",
            ProjectionState::PartialKnownStructure => "PartialKnownStructure",
            ProjectionState::ProjectionUnavailable => "ProjectionUnavailable",
        },
        completion_response_sequence: metadata.completion.map(|seq| seq.to_string()),
        open: metadata.open,
        read_context: match metadata.context {
            ReadContextKind::UniqueReceiptWindow => "unique_receipt_window",
            ReadContextKind::AmbiguousReceiptWindow => "ambiguous_receipt_window",
            ReadContextKind::OutsideReadWindow | ReadContextKind::Unavailable => {
                "outside_read_window"
            }
        },
        relation: "connection_observation_only",
        limitation: metadata.limitation,
    };
    super::projection_wire::encode(&value)
}
