//! Structured view knowledge and original-byte references; no native execution authority.
use super::{
    shared_view::{RawRecordId, ViewError},
    shared_view_log::LogSnapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldKnowledge {
    Unknown,
    Known,
    KnownAbsent,
    KnownEmpty,
    Partial,
    Uninterpreted,
    Deferred,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum ToolField {
    Content = 0,
    Metadata = 1,
    Name = 2,
    Title = 3,
    Kind = 4,
    Status = 5,
    Locations = 6,
    RawInput = 7,
    RawOutput = 8,
}
pub(crate) const TOOL_FIELD_COUNT: usize = 9;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructuredItemKind {
    Tool,
    Message,
    State,
    UnknownStructured,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawFieldReference {
    pub(crate) record: RawRecordId,
    pub(crate) record_index: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) append: bool,
}
impl RawFieldReference {
    #[must_use]
    pub const fn record(self) -> RawRecordId {
        self.record
    }
    #[must_use]
    pub const fn byte_start(self) -> usize {
        self.start
    }
    #[must_use]
    pub const fn byte_end(self) -> usize {
        self.end
    }
    #[must_use]
    pub const fn is_append_evidence(self) -> bool {
        self.append
    }
}
pub(crate) struct FieldEvidence {
    pub log: LogSnapshot<RawFieldReference>,
    pub start: usize,
    pub end: usize,
}
impl Clone for FieldEvidence {
    fn clone(&self) -> Self {
        Self {
            log: self.log.clone(),
            start: self.start,
            end: self.end,
        }
    }
}
#[derive(Clone)]
pub struct ProjectedField {
    pub(crate) knowledge: FieldKnowledge,
    pub(crate) raw: Option<RawFieldReference>,
    pub(crate) code: Option<u8>,
    pub(crate) evidence: Option<FieldEvidence>,
}
impl ProjectedField {
    pub(crate) const fn unknown() -> Self {
        Self {
            knowledge: FieldKnowledge::Unknown,
            raw: None,
            code: None,
            evidence: None,
        }
    }
    #[must_use]
    pub const fn knowledge(&self) -> FieldKnowledge {
        self.knowledge
    }
    #[must_use]
    pub const fn raw_reference(&self) -> Option<RawFieldReference> {
        self.raw
    }
    #[must_use]
    pub const fn enum_code(&self) -> Option<u8> {
        self.code
    }
    #[must_use]
    pub fn evidence_count(&self) -> usize {
        self.evidence.as_ref().map_or(0, |v| v.end - v.start)
    }
    /// # Errors
    /// Rejects an index outside this immutable evidence version.
    pub fn evidence_at(&self, index: usize) -> Result<RawFieldReference, ViewError> {
        let evidence = self.evidence.as_ref().ok_or(ViewError::InvalidCursor)?;
        let position = evidence
            .start
            .checked_add(index)
            .filter(|i| *i < evidence.end)
            .ok_or(ViewError::InvalidCursor)?;
        Ok(*evidence.log.get(position)?)
    }
}
pub struct ProjectedItemVersion {
    pub(crate) segment: crate::domain::shared_view::ViewEpoch,
    pub(crate) revision: u64,
    pub(crate) raw: RawRecordId,
    pub(crate) kind: StructuredItemKind,
    pub(crate) fields: [ProjectedField; TOOL_FIELD_COUNT],
}
impl ProjectedItemVersion {
    #[must_use]
    pub const fn segment_view_epoch(&self) -> crate::domain::shared_view::ViewEpoch {
        self.segment
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn raw_record(&self) -> RawRecordId {
        self.raw
    }
    #[must_use]
    pub const fn kind(&self) -> StructuredItemKind {
        self.kind
    }
    #[must_use]
    pub const fn field(&self, field: ToolField) -> &ProjectedField {
        &self.fields[field as usize]
    }
}
