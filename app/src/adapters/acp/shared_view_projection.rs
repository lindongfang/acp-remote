//! Version-aware structural projection using original raw ranges, independent of legacy budgets.
use super::{
    Connection, RawMessage, SessionId, Version,
    raw_json::{RawKind, RawObject, RawValueRef},
    shared_view_capture::RetainedAcpRaw,
};
use crate::{
    application::shared_views::{
        indexed_journal::{FieldChange, ProjectionEnvelope, ViewProjector},
        journal::ContentIdentity,
    },
    domain::{
        shared_projection::{FieldKnowledge, StructuredItemKind, ToolField},
        shared_view::{ProjectionState, ViewError},
    },
};
use std::sync::Arc;

pub struct AcpViewProjector {
    connection: Connection,
}
impl AcpViewProjector {
    pub(crate) const fn new(connection: Connection) -> Self {
        Self { connection }
    }
}
impl ViewProjector<RetainedAcpRaw> for AcpViewProjector {
    fn inspect(&self, raw: &RetainedAcpRaw) -> ProjectionEnvelope {
        let message = raw.message();
        let Some(version) = self.connection.strategy().map(|s| s.version()) else {
            return ProjectionEnvelope::unknown(None);
        };
        let parsed = envelope(message, version);
        let mut projected = match parsed {
            Ok(value) => value,
            Err((session, error)) => {
                let mut value = ProjectionEnvelope::unknown(session);
                value.failure = Some(error);
                value
            }
        };
        projected.observation = raw.observation();
        projected
    }
}
#[allow(
    clippy::too_many_lines,
    reason = "One version-aware envelope projection preserves field omission, raw spans, and unsupported structure together"
)]
fn envelope(
    raw: &RawMessage,
    version: Version,
) -> Result<ProjectionEnvelope, (Option<Arc<str>>, ViewError)> {
    let Some(method) = raw
        .field("method")
        .map_err(|_| (None, ViewError::ProjectionFailure))?
        .and_then(|v| v.string().ok())
    else {
        return Ok(ProjectionEnvelope::unknown(None));
    };
    if method != "session/update" {
        return Ok(ProjectionEnvelope::unknown(None));
    }
    let Some(params) = raw
        .field("params")
        .map_err(|_| (None, ViewError::ProjectionFailure))?
        .and_then(RawValueRef::object)
    else {
        return Ok(ProjectionEnvelope::unknown(None));
    };
    let Some(session) = params
        .get("sessionId")
        .map_err(|_| (None, ViewError::ProjectionFailure))?
        .and_then(|v| SessionId::parse(v).ok())
    else {
        return Ok(ProjectionEnvelope::unknown(None));
    };
    let session: Arc<str> = Arc::from(session.as_str());
    let mut result = ProjectionEnvelope::unknown(Some(Arc::clone(&session)));
    let Some(update) = params
        .get("update")
        .map_err(|_| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?
        .and_then(RawValueRef::object)
    else {
        return Ok(result);
    };
    let Some(tag) = update
        .get("sessionUpdate")
        .map_err(|_| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?
        .and_then(|v| v.string().ok())
    else {
        return Ok(result);
    };
    let initial = version == Version::V1 && tag == "tool_call";
    let tool_chunk = version == Version::V2 && tag == "tool_call_content_chunk";
    if initial || tool_chunk || tag == "tool_call_update" {
        let id = update
            .get("toolCallId")
            .map_err(|_| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?
            .and_then(|v| v.string().ok())
            .ok_or_else(|| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?;
        result.identity = Some(ContentIdentity::Tool(Arc::from(id)));
        result.kind = StructuredItemKind::Tool;
        result.projection = ProjectionState::Available;
        match tool_fields(raw, update, version, initial, tool_chunk) {
            Ok(fields) => {
                result.fields = fields;
                if fields.iter().any(|v| {
                    matches!(
                        v,
                        FieldChange::Set {
                            knowledge: FieldKnowledge::Uninterpreted,
                            ..
                        }
                    ) | matches!(
                        v,
                        FieldChange::Set {
                            code: Some(255),
                            ..
                        }
                    )
                }) {
                    result.projection = ProjectionState::PartialKnownStructure;
                }
            }
            Err(error) => {
                result.failure = Some(error);
                result.projection = ProjectionState::ProjectionUnavailable;
            }
        }
    } else if matches!(
        tag.as_str(),
        "user_message"
            | "user_message_chunk"
            | "agent_message"
            | "agent_message_chunk"
            | "agent_thought"
            | "agent_thought_chunk"
    ) {
        result.kind = StructuredItemKind::Message;
        result.projection = ProjectionState::Available;
        if let Some(id) = update
            .get("messageId")
            .map_err(|_| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?
            .filter(|v| version != Version::V1 || !v.is_null())
        {
            let Ok(id) = id.string() else {
                result.failure = Some(ViewError::ProjectionFailure);
                return Ok(result);
            };
            result.identity = Some(ContentIdentity::Message(Arc::from(id)));
        }
        if let Some(content) = update
            .get("content")
            .map_err(|_| (Some(Arc::clone(&session)), ViewError::ProjectionFailure))?
        {
            let structured = content.array().is_some()
                || content_has_type(content).map_err(|e| (Some(Arc::clone(&session)), e))?;
            let knowledge = if !structured || !tag.ends_with("_chunk") {
                FieldKnowledge::Uninterpreted
            } else {
                FieldKnowledge::Deferred
            };
            if knowledge == FieldKnowledge::Uninterpreted {
                result.projection = ProjectionState::PartialKnownStructure;
            }
            result.fields[ToolField::Content as usize] =
                set(raw, content, knowledge, None, result.identity.is_some())
                    .map_err(|e| (Some(Arc::clone(&session)), e))?;
        } else {
            result.projection = ProjectionState::PartialKnownStructure;
        }
    } else if version == Version::V2 && tag == "state_update" {
        result.kind = StructuredItemKind::State;
    }
    Ok(result)
}
fn content_has_type(content: RawValueRef<'_>) -> Result<bool, ViewError> {
    let Some(object) = content.object() else {
        return Ok(false);
    };
    Ok(object
        .get("type")
        .map_err(|_| ViewError::ProjectionFailure)?
        .is_some_and(|v| v.kind() == RawKind::String))
}
fn span(raw: &RawMessage, value: RawValueRef<'_>) -> Result<(usize, usize), ViewError> {
    let fragment = value.fragment().as_bytes();
    let start = fragment
        .as_ptr()
        .addr()
        .checked_sub(raw.bytes().as_ptr().addr())
        .ok_or(ViewError::ProjectionFailure)?;
    let end = start
        .checked_add(fragment.len())
        .filter(|end| *end <= raw.bytes().len())
        .ok_or(ViewError::ProjectionFailure)?;
    Ok((start, end))
}
fn set(
    raw: &RawMessage,
    value: RawValueRef<'_>,
    knowledge: FieldKnowledge,
    code: Option<u8>,
    append: bool,
) -> Result<FieldChange, ViewError> {
    Ok(FieldChange::Set {
        knowledge,
        span: Some(span(raw, value)?),
        code,
        append,
    })
}
fn tool_fields(
    raw: &RawMessage,
    update: RawObject<'_>,
    version: Version,
    initial: bool,
    chunk: bool,
) -> Result<[FieldChange; 9], ViewError> {
    let get = |key| update.get(key).map_err(|_| ViewError::ProjectionFailure);
    let mut fields = [FieldChange::Keep; 9];
    if chunk {
        let content = get("content")?.ok_or(ViewError::ProjectionFailure)?;
        if !content_has_type(content)? {
            return Err(ViewError::ProjectionFailure);
        }
        fields[0] = set(raw, content, FieldKnowledge::Deferred, None, true)?;
        validate_meta(get("_meta")?)?;
        return Ok(fields);
    }
    fields[2] = text_field(raw, get("name")?, version, initial, true)?;
    fields[3] = text_field(raw, get("title")?, version, initial, false)?;
    fields[4] = enum_field(raw, get("kind")?, version, initial, true)?;
    fields[5] = enum_field(raw, get("status")?, version, initial, false)?;
    fields[0] = array_field(raw, get("content")?, version, initial)?;
    fields[6] = array_field(raw, get("locations")?, version, initial)?;
    validate_meta(get("_meta")?)?;
    fields[1] = match get("_meta")? {
        None => FieldChange::Keep,
        Some(v) => set(
            raw,
            v,
            if v.is_null() {
                if version == Version::V1 {
                    FieldKnowledge::Uninterpreted
                } else {
                    FieldKnowledge::KnownEmpty
                }
            } else {
                FieldKnowledge::Deferred
            },
            None,
            false,
        )?,
    };
    for (index, name) in [(7, "rawInput"), (8, "rawOutput")] {
        fields[index] = match get(name)? {
            None => FieldChange::Keep,
            Some(v) if v.is_null() && version == Version::V1 && !initial => FieldChange::Keep,
            Some(v) => set(
                raw,
                v,
                if v.is_null() && version == Version::V2 {
                    FieldKnowledge::KnownAbsent
                } else {
                    FieldKnowledge::Deferred
                },
                None,
                false,
            )?,
        };
    }
    Ok(fields)
}
fn text_field(
    raw: &RawMessage,
    value: Option<RawValueRef<'_>>,
    version: Version,
    initial: bool,
    name: bool,
) -> Result<FieldChange, ViewError> {
    match value {
        None if initial && !name => Err(ViewError::ProjectionFailure),
        None if initial => Ok(FieldChange::Set {
            knowledge: FieldKnowledge::KnownAbsent,
            span: None,
            code: None,
            append: false,
        }),
        None => Ok(FieldChange::Keep),
        Some(v) if v.kind() == RawKind::String => set(raw, v, FieldKnowledge::Known, None, false),
        Some(v) if v.is_null() && name && version == Version::V1 && !initial => {
            Ok(FieldChange::Keep)
        }
        Some(v) if v.is_null() && !name && initial => Err(ViewError::ProjectionFailure),
        Some(v) if v.is_null() => set(
            raw,
            v,
            if version == Version::V2 || name {
                FieldKnowledge::KnownAbsent
            } else {
                FieldKnowledge::Uninterpreted
            },
            None,
            false,
        ),
        Some(_) => Err(ViewError::ProjectionFailure),
    }
}
fn enum_field(
    raw: &RawMessage,
    value: Option<RawValueRef<'_>>,
    version: Version,
    initial: bool,
    kind: bool,
) -> Result<FieldChange, ViewError> {
    let Some(value) = value else {
        return Ok(FieldChange::Keep);
    };
    if value.is_null() && initial {
        return Err(ViewError::ProjectionFailure);
    }
    if value.is_null() && version == Version::V1 {
        return set(raw, value, FieldKnowledge::Uninterpreted, None, false);
    }
    let code = if value.is_null() {
        0
    } else {
        let value = value.string().map_err(|_| ViewError::ProjectionFailure)?;
        if kind {
            match value.as_str() {
                "other" => 0,
                "read" => 1,
                "edit" => 2,
                "delete" => 3,
                "move" => 4,
                "search" => 5,
                "execute" => 6,
                "think" => 7,
                "fetch" => 8,
                "switch_mode" => 9,
                _ => 255,
            }
        } else {
            match value.as_str() {
                "pending" => 0,
                "in_progress" => 1,
                "completed" => 2,
                "failed" => 3,
                "cancelled" if version == Version::V2 => 4,
                _ => 255,
            }
        }
    };
    if code == 255 && version == Version::V1 {
        return Err(ViewError::ProjectionFailure);
    }
    set(raw, value, FieldKnowledge::Known, Some(code), false)
}
fn array_field(
    raw: &RawMessage,
    value: Option<RawValueRef<'_>>,
    version: Version,
    initial: bool,
) -> Result<FieldChange, ViewError> {
    match value {
        None => Ok(FieldChange::Keep),
        Some(v) if v.array().is_some() => set(raw, v, FieldKnowledge::Deferred, None, false),
        Some(v) if v.is_null() && initial => Err(ViewError::ProjectionFailure),
        Some(v) if v.is_null() => set(
            raw,
            v,
            if version == Version::V1 {
                FieldKnowledge::Uninterpreted
            } else {
                FieldKnowledge::KnownEmpty
            },
            None,
            false,
        ),
        Some(_) => Err(ViewError::ProjectionFailure),
    }
}
fn validate_meta(value: Option<RawValueRef<'_>>) -> Result<(), ViewError> {
    if value.is_none_or(|v| v.is_null() || v.object().is_some()) {
        Ok(())
    } else {
        Err(ViewError::ProjectionFailure)
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod projection_independent;
