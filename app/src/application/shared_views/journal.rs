//! Owned disposable storage; no byte, entry, identity, snapshot, or reference quota.
use super::RetainedRaw;
use crate::domain::shared_view::{
    AnchorId, ProjectionState, RawRecordId, Snapshot, ViewEpoch, ViewError,
};
use std::{collections::HashMap, fmt, sync::Arc};

pub const TRANSFER_CHUNK_BYTES: usize = 16 * 1024;

/// Protocol identity data only. Namespace and exact decoded UTF-8 bytes are significant.
#[derive(Clone, Eq, Hash, PartialEq)]
pub enum ContentIdentity {
    Message(Arc<str>),
    Tool(Arc<str>),
}
impl fmt::Debug for ContentIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Message(_) => "Message(..)",
            Self::Tool(_) => "Tool(..)",
        })
    }
}

struct RawRecord<R> {
    raw: R,
    revision: u64,
    sequence: u64,
    receipt_order: u64,
}
struct Anchor {
    first_revision: u64,
    versions: Vec<(u64, RawRecordId)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemMetadata {
    pub anchor: AnchorId,
    pub first_revision: u64,
    pub revision: u64,
    pub raw: RawRecordId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawMetadata {
    pub id: RawRecordId,
    pub revision: u64,
    pub sequence: u64,
    pub receipt_order: u64,
    pub byte_length: usize,
}

/// The caller owns this store; production gateways must never expose it instead of a read service.
pub struct MemoryJournal<R> {
    epoch: ViewEpoch,
    records: Vec<RawRecord<R>>,
    anchors: Vec<Anchor>,
    identities: HashMap<ContentIdentity, AnchorId>,
    projection: ProjectionState,
    sealed: bool,
}
impl<R: RetainedRaw> MemoryJournal<R> {
    /// # Errors
    /// Returns `RandomUnavailable` if no new disposable generation can be created.
    pub fn new() -> Result<Self, ViewError> {
        Ok(Self {
            epoch: ViewEpoch::fresh()?,
            records: Vec::new(),
            anchors: Vec::new(),
            identities: HashMap::new(),
            projection: ProjectionState::Available,
            sealed: false,
        })
    }
    #[must_use]
    pub const fn epoch(&self) -> ViewEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn projection(&self) -> ProjectionState {
        self.projection
    }
    #[must_use]
    pub const fn raw_count(&self) -> usize {
        self.records.len()
    }
    #[must_use]
    pub const fn anchor_count(&self) -> usize {
        self.anchors.len()
    }
    pub const fn seal(&mut self) {
        self.sealed = true;
    }

    /// Retains original data before any optional projection work.
    /// # Errors
    /// Reports actual allocation/representation failure or a sealed generation.
    pub fn append_raw(
        &mut self,
        raw: R,
        sequence: u64,
        receipt_order: u64,
    ) -> Result<RawRecordId, ViewError> {
        if self.sealed {
            return Err(ViewError::SourceReleased);
        }
        let revision = u64::try_from(self.records.len())
            .map_err(|_| ViewError::SequenceExhausted)?
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        self.records
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        self.records.push(RawRecord {
            raw,
            revision,
            sequence,
            receipt_order,
        });
        Ok(RawRecordId(revision))
    }

    /// Adds a version at the first observed anchor. None always creates a distinct chunk.
    /// # Errors
    /// Projection failure leaves the original raw record retained and readable.
    pub fn project(
        &mut self,
        raw: RawRecordId,
        identity: Option<ContentIdentity>,
    ) -> Result<AnchorId, ViewError> {
        if self.sealed {
            return Err(ViewError::SourceReleased);
        }
        let revision = self.record(raw)?.revision;
        let outcome = self.project_inner(raw, revision, identity);
        if outcome.is_err() {
            self.projection = ProjectionState::ProjectionUnavailable;
        }
        outcome
    }
    fn project_inner(
        &mut self,
        raw: RawRecordId,
        revision: u64,
        identity: Option<ContentIdentity>,
    ) -> Result<AnchorId, ViewError> {
        if let Some(anchor) = identity
            .as_ref()
            .and_then(|key| self.identities.get(key))
            .copied()
        {
            let item = self
                .anchors
                .get_mut(anchor.0)
                .ok_or(ViewError::ProjectionFailure)?;
            if item
                .versions
                .last()
                .is_some_and(|(old, _)| *old >= revision)
            {
                return Err(ViewError::ProjectionFailure);
            }
            item.versions
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            item.versions.push((revision, raw));
            return Ok(anchor);
        }
        let mut versions = Vec::new();
        versions
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        self.anchors
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        if identity.is_some() {
            self.identities
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
        }
        versions.push((revision, raw));
        let anchor = AnchorId(self.anchors.len());
        self.anchors.push(Anchor {
            first_revision: revision,
            versions,
        });
        if let Some(key) = identity {
            self.identities.insert(key, anchor);
        }
        Ok(anchor)
    }
    /// # Errors
    /// A released source cannot open a fresh snapshot of the old generation.
    pub fn snapshot(&self) -> Result<Snapshot, ViewError> {
        if self.sealed {
            return Err(ViewError::SourceReleased);
        }
        Ok(Snapshot {
            epoch: self.epoch,
            highwater: u64::try_from(self.records.len())
                .map_err(|_| ViewError::SequenceExhausted)?,
            anchors: self.anchors.len(),
        })
    }
    /// # Errors
    /// Rejects a stale snapshot and positions outside its fixed boundary.
    pub fn item_at(&self, snapshot: Snapshot, index: usize) -> Result<ItemMetadata, ViewError> {
        self.check(snapshot)?;
        if index >= snapshot.anchors {
            return Err(ViewError::InvalidCursor);
        }
        let item = self.anchors.get(index).ok_or(ViewError::InvalidCursor)?;
        let (revision, raw) = item
            .versions
            .iter()
            .rev()
            .find(|(rev, _)| *rev <= snapshot.highwater)
            .copied()
            .ok_or(ViewError::InvalidCursor)?;
        Ok(ItemMetadata {
            anchor: AnchorId(index),
            first_revision: item.first_revision,
            revision,
            raw,
        })
    }
    /// Copies only one network processing block. This is not an authorization boundary.
    /// # Errors
    /// Rejects stale generations/offsets and actual buffer allocation failures.
    pub fn copy_raw_chunk(
        &self,
        snapshot: Snapshot,
        raw: RawRecordId,
        offset: usize,
    ) -> Result<Vec<u8>, ViewError> {
        self.check(snapshot)?;
        let record = self.record(raw)?;
        if record.revision > snapshot.highwater {
            return Err(ViewError::InvalidCursor);
        }
        let remaining = record
            .raw
            .bytes()
            .get(offset..)
            .ok_or(ViewError::InvalidCursor)?;
        let count = remaining.len().min(TRANSFER_CHUNK_BYTES);
        let mut out = Vec::new();
        out.try_reserve_exact(count)
            .map_err(|_| ViewError::AllocationFailure)?;
        out.extend_from_slice(&remaining[..count]);
        Ok(out)
    }
    /// # Errors
    /// Rejects a missing platform record identifier.
    pub fn raw_metadata(&self, id: RawRecordId) -> Result<RawMetadata, ViewError> {
        let record = self.record(id)?;
        Ok(RawMetadata {
            id,
            revision: record.revision,
            sequence: record.sequence,
            receipt_order: record.receipt_order,
            byte_length: record.raw.len(),
        })
    }
    /// # Errors
    /// Rejects an index outside the retained original records.
    pub fn raw_metadata_at(&self, index: usize) -> Result<RawMetadata, ViewError> {
        let record = self.records.get(index).ok_or(ViewError::InvalidCursor)?;
        self.raw_metadata(RawRecordId(record.revision))
    }
    fn record(&self, id: RawRecordId) -> Result<&RawRecord<R>, ViewError> {
        let index =
            id.0.checked_sub(1)
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(ViewError::InvalidCursor)?;
        self.records.get(index).ok_or(ViewError::InvalidCursor)
    }
    fn check(&self, snapshot: Snapshot) -> Result<(), ViewError> {
        if self.sealed || snapshot.epoch != self.epoch {
            return Err(ViewError::StaleCursor);
        }
        Ok(())
    }
}
