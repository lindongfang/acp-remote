//! Worker-owned reduction; readers use immutable roots and bounded metadata lock scopes.
use super::{RetainedRaw, journal::ContentIdentity};
use crate::domain::{
    shared_projection::{
        FieldEvidence, FieldKnowledge, ProjectedField, ProjectedItemVersion, RawFieldReference,
        StructuredItemKind, TOOL_FIELD_COUNT,
    },
    shared_view::{AnchorId, ProjectionState, RawRecordId, ViewEpoch, ViewError},
    shared_view_log::{AppendLog, LogSnapshot},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Clone, Copy)]
pub enum FieldChange {
    Keep,
    Set {
        knowledge: FieldKnowledge,
        span: Option<(usize, usize)>,
        code: Option<u8>,
        append: bool,
    },
}
#[derive(Clone)]
pub struct ProjectionEnvelope {
    pub session: Option<Arc<str>>,
    pub identity: Option<ContentIdentity>,
    pub kind: StructuredItemKind,
    pub fields: [FieldChange; TOOL_FIELD_COUNT],
    pub projection: ProjectionState,
    pub failure: Option<ViewError>,
    pub(crate) observation: super::read_observation::ReadObservation,
}
impl ProjectionEnvelope {
    pub(crate) const fn unknown(session: Option<Arc<str>>) -> Self {
        Self {
            session,
            identity: None,
            kind: StructuredItemKind::UnknownStructured,
            fields: [FieldChange::Keep; TOOL_FIELD_COUNT],
            projection: ProjectionState::PartialKnownStructure,
            failure: None,
            observation: super::read_observation::ReadObservation::unavailable(),
        }
    }
}
pub trait ViewProjector<R>: Send + Sync + 'static {
    fn inspect(&self, raw: &R) -> ProjectionEnvelope;
}
pub struct RawRecord<R> {
    pub segment: ViewEpoch,
    pub segment_index: usize,
    pub raw: R,
    pub id: RawRecordId,
    pub revision: u64,
    pub sequence: u64,
    pub receipt_order: u64,
}
pub struct AnchorPublication {
    pub segment: ViewEpoch,
    pub segment_index: usize,
    pub first_revision: u64,
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub namespace: Option<u8>,
    pub versions: RwLock<LogSnapshot<ProjectedItemVersion>>,
}
pub struct ProjectionVersion {
    pub anchor: AnchorId,
    pub first_position: u64,
    pub version: Arc<ProjectedItemVersion>,
}
struct AnchorWorker {
    publication: Arc<AnchorPublication>,
    versions: AppendLog<ProjectedItemVersion>,
    fields: [ProjectedField; TOOL_FIELD_COUNT],
    content: AppendLog<RawFieldReference>,
    content_start: usize,
}
pub struct IndexedJournal<R> {
    epoch: ViewEpoch,
    revision: u64,
    records: AppendLog<RawRecord<R>>,
    anchors: AppendLog<AnchorPublication>,
    workers: Vec<AnchorWorker>,
    identities: HashMap<(crate::domain::EntityId, ContentIdentity), AnchorId>,
    projection: ProjectionState,
    versions: AppendLog<ProjectionVersion>,
}
pub struct JournalSnapshot<R> {
    pub epoch: ViewEpoch,
    pub revision: u64,
    pub records: LogSnapshot<RawRecord<R>>,
    pub anchors: LogSnapshot<AnchorPublication>,
    pub projection: ProjectionState,
    pub versions: LogSnapshot<ProjectionVersion>,
}
impl<R> Clone for JournalSnapshot<R> {
    fn clone(&self) -> Self {
        Self {
            epoch: self.epoch,
            revision: self.revision,
            records: self.records.clone(),
            anchors: self.anchors.clone(),
            projection: self.projection,
            versions: self.versions.clone(),
        }
    }
}
impl<R: RetainedRaw> IndexedJournal<R> {
    pub(crate) fn new() -> Result<Self, ViewError> {
        Ok(Self {
            epoch: ViewEpoch::fresh()?,
            revision: 0,
            records: AppendLog::new()?,
            anchors: AppendLog::new()?,
            workers: Vec::new(),
            identities: HashMap::new(),
            projection: ProjectionState::Available,
            versions: AppendLog::new()?,
        })
    }
    pub(crate) fn snapshot(&self) -> Result<JournalSnapshot<R>, ViewError> {
        Ok(JournalSnapshot {
            epoch: self.epoch,
            revision: self.revision,
            records: self.records.snapshot()?,
            anchors: self.anchors.snapshot()?,
            projection: self.projection,
            versions: self.versions.snapshot()?,
        })
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "One immutable raw record carries original receipt, platform revision and independent segment address"
    )]
    pub(crate) fn append_segment_at(
        &mut self,
        segment: ViewEpoch,
        segment_index: usize,
        id: RawRecordId,
        revision: u64,
        raw: R,
        sequence: u64,
        receipt_order: u64,
        envelope: ProjectionEnvelope,
    ) -> Result<RawRecordId, ViewError> {
        if revision <= self.revision {
            return Err(ViewError::InvalidCursor);
        }
        let length = raw.len();
        let record_index = self.records.append(RawRecord {
            segment,
            segment_index,
            raw,
            id,
            revision,
            sequence,
            receipt_order,
        })?;
        self.revision = revision;
        if envelope.projection == ProjectionState::PartialKnownStructure
            && self.projection == ProjectionState::Available
        {
            self.projection = ProjectionState::PartialKnownStructure;
        }
        if let Some(error) = envelope.failure {
            self.projection = ProjectionState::ProjectionUnavailable;
            return Err(error);
        }
        let projected = self.project(
            segment,
            segment_index,
            id,
            revision,
            (record_index, length),
            envelope,
        );
        if projected.is_err() {
            self.projection = ProjectionState::ProjectionUnavailable;
        }
        projected.map(|()| id)
    }
    #[allow(
        clippy::too_many_lines,
        reason = "The fixed schema reduction publishes the anchor version only after all exact raw references have been validated"
    )]
    fn project(
        &mut self,
        segment: ViewEpoch,
        segment_index: usize,
        raw: RawRecordId,
        revision: u64,
        (record_index, length): (usize, usize),
        envelope: ProjectionEnvelope,
    ) -> Result<(), ViewError> {
        let anchor = if let Some(anchor) = envelope
            .identity
            .as_ref()
            .and_then(|id| self.identities.get(&(segment.as_entity_id(), id.clone())))
            .copied()
        {
            anchor
        } else {
            self.workers
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            if envelope.identity.is_some() {
                self.identities
                    .try_reserve(1)
                    .map_err(|_| ViewError::AllocationFailure)?;
            }
            let versions = AppendLog::new()?;
            let content = AppendLog::new()?;
            #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
            let namespace = envelope.identity.as_ref().map(|id| match id {
                ContentIdentity::Message(_) => 1,
                ContentIdentity::Tool(_) => 2,
            });
            let position = self.anchors.append(AnchorPublication {
                segment,
                segment_index,
                first_revision: revision,
                #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
                namespace,
                versions: RwLock::new(versions.snapshot()?),
            })?;
            let publication = self.anchors.snapshot()?.get(position)?;
            let anchor = AnchorId(position);
            self.workers.push(AnchorWorker {
                publication,
                versions,
                fields: std::array::from_fn(|_| ProjectedField::unknown()),
                content,
                content_start: 0,
            });
            if let Some(id) = envelope.identity {
                self.identities.insert((segment.as_entity_id(), id), anchor);
            }
            anchor
        };
        let worker = self
            .workers
            .get_mut(anchor.0)
            .ok_or(ViewError::ProjectionFailure)?;
        for (index, change) in envelope.fields.into_iter().enumerate() {
            let FieldChange::Set {
                knowledge,
                span,
                code,
                append,
            } = change
            else {
                continue;
            };
            let reference = if let Some((start, end)) = span {
                if start > end || end > length {
                    return Err(ViewError::ProjectionFailure);
                }
                Some(RawFieldReference {
                    record: raw,
                    record_index,
                    start,
                    end,
                    append,
                })
            } else {
                None
            };
            let evidence = if index == 0 {
                if !append {
                    worker.content_start = worker.content.snapshot()?.len();
                }
                if let Some(reference) = reference {
                    worker.content.append(reference)?;
                }
                let log = worker.content.snapshot()?;
                let end = log.len();
                Some(FieldEvidence {
                    log,
                    start: worker.content_start,
                    end,
                })
            } else {
                None
            };
            worker.fields[index] = ProjectedField {
                knowledge,
                raw: reference,
                code,
                evidence,
            };
        }
        let version_position = worker.versions.append(ProjectedItemVersion {
            segment,
            revision,
            raw,
            kind: envelope.kind,
            fields: worker.fields.clone(),
        })?;
        let published = worker.versions.snapshot()?;
        self.versions.append(ProjectionVersion {
            anchor,
            first_position: worker.publication.first_revision,
            version: published.get(version_position)?,
        })?;
        *worker
            .publication
            .versions
            .write()
            .map_err(|_| ViewError::ReadFailure)? = published;
        Ok(())
    }
}
impl<R: RetainedRaw> JournalSnapshot<R> {
    pub(crate) fn item(&self, index: usize) -> Result<Arc<ProjectedItemVersion>, ViewError> {
        let anchor = self.anchors.get(index)?;
        let versions = anchor
            .versions
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .clone();
        let mut low = 0;
        let mut high = versions.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if versions.get(middle)?.revision <= self.revision {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let position = low.checked_sub(1).ok_or(ViewError::InvalidCursor)?;
        versions.get(position)
    }
}
