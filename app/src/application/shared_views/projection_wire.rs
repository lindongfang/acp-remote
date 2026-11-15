//! Platform projection records contain knowledge and exact raw references, never reencoded native values.
use crate::domain::{
    shared_projection::{
        FieldKnowledge, ProjectedItemVersion, RawFieldReference, StructuredItemKind,
        TOOL_FIELD_COUNT,
    },
    shared_view::{AnchorId, ViewError},
};
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    raw_record_id: String,
    byte_start: String,
    byte_end: String,
    append: bool,
}
fn reference(value: RawFieldReference) -> Reference {
    Reference {
        raw_record_id: value.record().as_u64().to_string(),
        byte_start: value.byte_start().to_string(),
        byte_end: value.byte_end().to_string(),
        append: value.is_append_evidence(),
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeMetadata {
    field: &'static str,
    knowledge: &'static str,
    raw: Option<Reference>,
    enum_code: Option<u8>,
    evidence_count: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    segment_id: String,
    r#type: &'static str,
    anchor_id: String,
    first_position: String,
    revision: String,
    raw_record_id: String,
    kind: &'static str,
    fields: [KnowledgeMetadata; TOOL_FIELD_COUNT],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    segment_id: String,
    r#type: &'static str,
    anchor_id: String,
    revision: String,
    field: &'static str,
    index: String,
    raw: Reference,
}
const FIELDS: [&str; TOOL_FIELD_COUNT] = [
    "content",
    "metadata",
    "name",
    "title",
    "kind",
    "status",
    "locations",
    "rawInput",
    "rawOutput",
];
const fn knowledge(value: FieldKnowledge) -> &'static str {
    match value {
        FieldKnowledge::Unknown => "unknown",
        FieldKnowledge::Known => "known",
        FieldKnowledge::KnownAbsent => "knownAbsent",
        FieldKnowledge::KnownEmpty => "knownEmpty",
        FieldKnowledge::Partial => "partial",
        FieldKnowledge::Uninterpreted => "uninterpreted",
        FieldKnowledge::Deferred => "deferred",
    }
}
const fn kind(value: StructuredItemKind) -> &'static str {
    match value {
        StructuredItemKind::Tool => "tool",
        StructuredItemKind::Message => "message",
        StructuredItemKind::State => "state",
        StructuredItemKind::UnknownStructured => "unknownStructured",
    }
}
struct Output(Vec<u8>);
impl std::io::Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .try_reserve(bytes.len())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn encode(value: &impl Serialize) -> Result<Vec<u8>, ViewError> {
    let mut output = Output(Vec::new());
    serde_json::to_writer(&mut output, value).map_err(|_| ViewError::AllocationFailure)?;
    Ok(output.0)
}
pub fn item(
    anchor: AnchorId,
    first_position: u64,
    version: &ProjectedItemVersion,
) -> Result<Vec<u8>, ViewError> {
    let fields = std::array::from_fn(|index| {
        let field = &version.fields[index];
        KnowledgeMetadata {
            field: FIELDS[index],
            knowledge: knowledge(field.knowledge()),
            raw: field.raw_reference().map(reference),
            enum_code: field.enum_code(),
            evidence_count: field.evidence_count().to_string(),
        }
    });
    encode(&Item {
        segment_id: version.segment_view_epoch().as_entity_id().encode(),
        r#type: "item_projection",
        anchor_id: anchor.as_usize().to_string(),
        first_position: first_position.to_string(),
        revision: version.revision().to_string(),
        raw_record_id: version.raw_record().as_u64().to_string(),
        kind: kind(version.kind()),
        fields,
    })
}
pub fn evidence(
    anchor: AnchorId,
    version: &ProjectedItemVersion,
    field: usize,
    index: usize,
) -> Result<Vec<u8>, ViewError> {
    let value = version.fields.get(field).ok_or(ViewError::InvalidCursor)?;
    encode(&Evidence {
        segment_id: version.segment_view_epoch().as_entity_id().encode(),
        r#type: "projection_evidence",
        anchor_id: anchor.as_usize().to_string(),
        revision: version.revision().to_string(),
        field: FIELDS[field],
        index: index.to_string(),
        raw: reference(value.evidence_at(index)?),
    })
}
pub fn first_evidence(version: &ProjectedItemVersion, from: usize) -> Option<(usize, usize)> {
    (from..TOOL_FIELD_COUNT)
        .find(|field| version.fields[*field].evidence_count() > 0)
        .map(|field| (field, 0))
}
