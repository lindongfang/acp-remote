//! Bounded tool display anchors. Raw observation and execution ownership remain separate.
use super::raw_json::{RawKind, RawValueRef};
use super::{
    Incoming, Received, Strategy, UpdateOrigin, Version,
    view_observations::{ObservationError, ObservationOutcome, ObservationRead, ObservationStats},
};
use crate::domain::views::{
    KeyNamespace, SourceKey, StageWorkspace, ViewError, ViewFailure, ViewHandle, ViewItem,
    ViewWindows,
};

const ANCHOR_BYTES: usize = 512;
const FIELD_COUNT: usize = 16;
const TOOL_FIELDS: usize = 9;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AnchorField {
    Content = 0,
    Metadata = 1,
    Name = 2,
    Title = 3,
    Kind = 4,
    Status = 5,
    Locations = 6,
    RawInput = 7,
    RawOutput = 8,
    Command = 9,
    Cwd = 10,
    ExitStatus = 11,
    Size = 12,
    Plan = 13,
    Reserved14 = 14,
    Reserved15 = 15,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Knowledge {
    Unknown = 0,
    Known = 1,
    KnownAbsent = 2,
    KnownEmpty = 3,
    Partial = 4,
    Uninterpreted = 5,
    MissingRaw = 6,
    Deferred = 7,
}
impl Knowledge {
    const fn parse(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::Known),
            2 => Some(Self::KnownAbsent),
            3 => Some(Self::KnownEmpty),
            4 => Some(Self::Partial),
            5 => Some(Self::Uninterpreted),
            6 => Some(Self::MissingRaw),
            7 => Some(Self::Deferred),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FieldOrigin {
    Unknown = 0,
    Live = 1,
    DuringResume = 2,
    DuringReplay = 3,
}
impl FieldOrigin {
    const fn parse(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Unknown),
            1 => Some(Self::Live),
            2 => Some(Self::DuringResume),
            3 => Some(Self::DuringReplay),
            _ => None,
        }
    }
    const fn mask(self) -> u8 {
        match self {
            Self::Unknown => 8,
            Self::Live => 1,
            Self::DuringResume => 2,
            Self::DuringReplay => 4,
        }
    }
}
impl From<UpdateOrigin> for FieldOrigin {
    fn from(value: UpdateOrigin) -> Self {
        match value {
            UpdateOrigin::Live => Self::Live,
            UpdateOrigin::DuringResume => Self::DuringResume,
            UpdateOrigin::DuringReplay => Self::DuringReplay,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldFact {
    pub knowledge: Knowledge,
    pub sequence: u64,
    pub origin: FieldOrigin,
    pub last_raw_loss: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawEvidence {
    Unverified,
    NotRecorded,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatusFact {
    Unknown,
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
    UnrecognizedInProgress { sequence: u64, raw: RawEvidence },
    Uninterpreted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKindFact {
    Unknown,
    Other,
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    UnrecognizedOther { sequence: u64, raw: RawEvidence },
    Uninterpreted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProjectionError {
    #[error(transparent)]
    Observation(#[from] ObservationError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("tool anchor format or identity is invalid")]
    InvalidAnchor,
    #[error("tool display update has an invalid required field")]
    MalformedUpdate,
    #[error("tool display tag is not supported by the negotiated version")]
    UnsupportedAnchorKind,
    #[error("display scan budget is exhausted")]
    ScanBudget,
    #[error("complete current display data is unavailable")]
    MissingCurrent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorOutcome {
    Ignored,
    Updated { position: u64 },
    CreatedPartial { position: u64 },
    Failed(ProjectionError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionStep {
    pub raw: ObservationOutcome,
    pub anchor: AnchorOutcome,
}

/// Immutable format view only; this handle neither owns body data nor grants authority.
pub struct AnchorView<'a> {
    item: ViewItem<'a>,
}
impl AnchorView<'_> {
    #[must_use]
    pub const fn position(&self) -> u64 {
        self.item.position
    }
    #[must_use]
    pub const fn field(&self, field: AnchorField) -> FieldFact {
        let index = field as usize;
        // Only decode_anchor can construct this type, after checking every code and range.
        let knowledge = match Knowledge::parse(self.item.bytes[32 + index]) {
            Some(value) => value,
            None => Knowledge::Unknown,
        };
        let origin = match FieldOrigin::parse(self.item.bytes[176 + index]) {
            Some(value) => value,
            None => FieldOrigin::Unknown,
        };
        FieldFact {
            knowledge,
            sequence: read_u64(self.item.bytes, 48 + index * 8),
            origin,
            last_raw_loss: read_u64(self.item.bytes, 192 + index * 8),
        }
    }
    #[must_use]
    pub const fn tool_status(&self) -> ToolStatusFact {
        let fact = self.field(AnchorField::Status);
        match fact.knowledge {
            Knowledge::Known => match self.item.bytes[25] {
                0 => ToolStatusFact::Pending,
                1 => ToolStatusFact::InProgress,
                2 => ToolStatusFact::Completed,
                3 => ToolStatusFact::Failed,
                4 => ToolStatusFact::Cancelled,
                _ => ToolStatusFact::UnrecognizedInProgress {
                    sequence: fact.sequence,
                    raw: RawEvidence::Unverified,
                },
            },
            Knowledge::MissingRaw => ToolStatusFact::UnrecognizedInProgress {
                sequence: fact.sequence,
                raw: RawEvidence::NotRecorded,
            },
            Knowledge::Uninterpreted => ToolStatusFact::Uninterpreted,
            _ => ToolStatusFact::Unknown,
        }
    }
    #[must_use]
    pub const fn tool_kind(&self) -> ToolKindFact {
        let fact = self.field(AnchorField::Kind);
        match fact.knowledge {
            Knowledge::Known => match self.item.bytes[26] {
                0 => ToolKindFact::Other,
                1 => ToolKindFact::Read,
                2 => ToolKindFact::Edit,
                3 => ToolKindFact::Delete,
                4 => ToolKindFact::Move,
                5 => ToolKindFact::Search,
                6 => ToolKindFact::Execute,
                7 => ToolKindFact::Think,
                8 => ToolKindFact::Fetch,
                9 => ToolKindFact::SwitchMode,
                _ => ToolKindFact::UnrecognizedOther {
                    sequence: fact.sequence,
                    raw: RawEvidence::Unverified,
                },
            },
            Knowledge::MissingRaw => ToolKindFact::UnrecognizedOther {
                sequence: fact.sequence,
                raw: RawEvidence::NotRecorded,
            },
            Knowledge::Uninterpreted => ToolKindFact::Uninterpreted,
            _ => ToolKindFact::Unknown,
        }
    }
}
const fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}
fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// Validates the exact tool format. Known variable metadata is not proof its raw still exists.
/// # Errors
/// Rejects foreign namespaces, unsupported wire formats, invalid reserved bits and field codes.
pub fn decode_anchor(item: ViewItem<'_>, wire: Version) -> Result<AnchorView<'_>, ProjectionError> {
    let bytes = item.bytes;
    let valid_key = item
        .key
        .is_some_and(|key| key.namespace == KeyNamespace::Tool && key.id.len() <= 4096);
    if !valid_key
        || bytes.len() != ANCHOR_BYTES
        || &bytes[..4] != b"ACPA"
        || bytes[4] != 1
        || u64::from(bytes[5]) != wire.number()
        || bytes[6] != 4
        || bytes[7] & !7 != 0
        || bytes[24] == 0
        || bytes[24] & !15 != 0
        || bytes[27..32] != [0; 5]
        || bytes[320..].iter().any(|byte| *byte != 0)
    {
        return Err(ProjectionError::InvalidAnchor);
    }
    let first = read_u64(bytes, 8);
    let last = read_u64(bytes, 16);
    if first == 0 || last < first {
        return Err(ProjectionError::InvalidAnchor);
    }
    let mut loss = false;
    let mut uninterpreted = false;
    for index in 0..FIELD_COUNT {
        let knowledge =
            Knowledge::parse(bytes[32 + index]).ok_or(ProjectionError::InvalidAnchor)?;
        let origin =
            FieldOrigin::parse(bytes[176 + index]).ok_or(ProjectionError::InvalidAnchor)?;
        let sequence = read_u64(bytes, 48 + index * 8);
        let last_loss = read_u64(bytes, 192 + index * 8);
        if knowledge == Knowledge::Unknown {
            if sequence != 0 || origin != FieldOrigin::Unknown || last_loss != 0 {
                return Err(ProjectionError::InvalidAnchor);
            }
        } else if !(first..=last).contains(&sequence) || bytes[24] & origin.mask() == 0 {
            return Err(ProjectionError::InvalidAnchor);
        }
        if last_loss != 0 && !(first..=last).contains(&last_loss) {
            return Err(ProjectionError::InvalidAnchor);
        }
        if knowledge == Knowledge::MissingRaw && last_loss != sequence {
            return Err(ProjectionError::InvalidAnchor);
        }
        if index >= TOOL_FIELDS && knowledge != Knowledge::Unknown {
            return Err(ProjectionError::InvalidAnchor);
        }
        loss |= last_loss != 0;
        uninterpreted |= knowledge == Knowledge::Uninterpreted;
        if matches!(index, 4 | 5) {
            let code = bytes[if index == 4 { 26 } else { 25 }];
            let maximum = if index == 4 {
                9
            } else if wire == Version::V1 {
                3
            } else {
                4
            };
            let valid = match knowledge {
                Knowledge::Unknown | Knowledge::Uninterpreted => code == 255,
                Knowledge::Known => code <= maximum || (wire == Version::V2 && code == 255),
                Knowledge::MissingRaw => wire == Version::V2 && code == 255,
                _ => false,
            };
            if !valid {
                return Err(ProjectionError::InvalidAnchor);
            }
        }
    }
    if (bytes[7] & 2 != 0) != loss || (bytes[7] & 4 != 0) != uninterpreted {
        return Err(ProjectionError::InvalidAnchor);
    }
    Ok(AnchorView { item })
}

#[derive(Clone, Copy)]
enum Assignment {
    Keep,
    Set { knowledge: Knowledge, code: u8 },
}
impl Assignment {
    const fn set(knowledge: Knowledge) -> Self {
        Self::Set {
            knowledge,
            code: 255,
        }
    }
}
struct ToolPatch {
    id: String,
    assignments: [Assignment; TOOL_FIELDS],
}

/// One owner composes this wrapper with its real connection and current display read.
pub struct ToolProjectionRead {
    raw: ObservationRead,
    handle: ViewHandle,
    wire: Version,
    // Retires after all metadata whose storage it conservatively covers.
    _workspace: StageWorkspace,
}
impl ToolProjectionRead {
    /// Creates the existing raw reader and a separately charged fixed tool wrapper.
    /// # Errors
    /// Rejects invalid read scopes or unavailable shared metadata capacity.
    pub fn begin(
        windows: &mut ViewWindows,
        handle: &ViewHandle,
        strategy: &Strategy,
    ) -> Result<Self, ProjectionError> {
        let raw = ObservationRead::begin(windows, handle, strategy)?;
        let workspace = windows.stage_workspace(handle, ANCHOR_BYTES)?;
        Ok(Self {
            raw,
            handle: handle.clone(),
            wire: strategy.version(),
            _workspace: workspace,
        })
    }
    #[must_use]
    pub const fn raw_stats(&self) -> ObservationStats {
        self.raw.stats()
    }
    /// Observes raw exactly once, then attempts one atomic update of the retained tool anchor.
    /// # Errors
    /// Propagates raw observation lifecycle/errors; an anchor failure remains paired with raw facts.
    pub fn apply(
        &mut self,
        windows: &mut ViewWindows,
        input: &Received,
    ) -> Result<ProjectionStep, ProjectionError> {
        let raw = self.raw.observe(windows, input)?;
        Ok(self.project(windows, input.incoming(), raw))
    }
    #[cfg(test)]
    fn apply_inner(
        &mut self,
        windows: &mut ViewWindows,
        input: &Incoming,
    ) -> Result<ProjectionStep, ProjectionError> {
        let raw = self.raw.observe_inner(windows, input)?;
        Ok(self.project(windows, input, raw))
    }
    fn project(
        &self,
        windows: &mut ViewWindows,
        input: &Incoming,
        raw: ObservationOutcome,
    ) -> ProjectionStep {
        let (sequence, recorded) = match raw {
            ObservationOutcome::Ignored => {
                return ProjectionStep {
                    raw,
                    anchor: AnchorOutcome::Ignored,
                };
            }
            ObservationOutcome::Recorded { sequence, .. } => (sequence, true),
            ObservationOutcome::Dropped { sequence, .. } => (sequence, false),
        };
        let operation = tool_patch(self.wire, input, recorded).and_then(|patch| {
            patch.map_or(Ok(AnchorOutcome::Ignored), |patch| {
                self.publish(windows, input, &patch, sequence, recorded)
            })
        });
        let anchor = match operation {
            Ok(outcome) => outcome,
            Err(error) => {
                let failure = match error {
                    ProjectionError::View(ViewError::Capacity) => ViewFailure::Capacity,
                    ProjectionError::View(ViewError::SequenceExhausted) => {
                        ViewFailure::SequenceExhausted
                    }
                    _ => ViewFailure::Structure,
                };
                // An old handle cannot mark the replacement read.
                let _ = windows.fail(&self.handle, failure);
                AnchorOutcome::Failed(error)
            }
        };
        ProjectionStep { raw, anchor }
    }
    fn publish(
        &self,
        windows: &mut ViewWindows,
        input: &Incoming,
        patch: &ToolPatch,
        sequence: u64,
        recorded: bool,
    ) -> Result<AnchorOutcome, ProjectionError> {
        let key = SourceKey {
            namespace: KeyNamespace::Tool,
            id: &patch.id,
        };
        let old = match windows.item_lease(&self.handle, key) {
            Ok(lease) => {
                let view = decode_anchor(lease.item(), self.wire)?;
                if sequence <= read_u64(view.item.bytes, 16) {
                    return Err(ProjectionError::MalformedUpdate);
                }
                Some(lease)
            }
            Err(ViewError::MissingItem) => None,
            Err(error) => return Err(error.into()),
        };
        let replaced = old.is_some();
        let workspace = windows.stage_workspace(&self.handle, ANCHOR_BYTES)?;
        let mut staged = if let Some(old) = old {
            windows.stage_replace(&self.handle, workspace, old, ANCHOR_BYTES)?
        } else {
            windows.stage_insert(&self.handle, workspace, Some(key), ANCHOR_BYTES)?
        };
        {
            let parts = staged.parts();
            let bytes = parts
                .scratch
                .get_mut(..ANCHOR_BYTES)
                .ok_or(ViewError::Capacity)?;
            if let Some(old) = parts.old {
                bytes.copy_from_slice(old.bytes);
            } else {
                bytes.fill(0);
                bytes[..4].copy_from_slice(b"ACPA");
                bytes[4] = 1;
                bytes[5] = match self.wire {
                    Version::V1 => 1,
                    Version::V2 => 2,
                };
                bytes[6] = 4;
                bytes[7] = 1;
                bytes[25] = 255;
                bytes[26] = 255;
                write_u64(bytes, 8, sequence);
            }
            let Incoming::Update { origin, .. } = input else {
                return Err(ProjectionError::MalformedUpdate);
            };
            apply_assignments(bytes, patch, sequence, FieldOrigin::from(*origin), recorded);
            decode_anchor(
                ViewItem {
                    position: 0,
                    key: Some(key),
                    bytes,
                },
                self.wire,
            )?;
            let mut writer = parts.writer;
            writer.write_all(bytes)?;
        }
        let fact = windows.publish(&self.handle, staged)?;
        Ok(if replaced {
            AnchorOutcome::Updated {
                position: fact.position,
            }
        } else {
            AnchorOutcome::CreatedPartial {
                position: fact.position,
            }
        })
    }
}
fn apply_assignments(
    bytes: &mut [u8],
    patch: &ToolPatch,
    sequence: u64,
    origin: FieldOrigin,
    recorded: bool,
) {
    write_u64(bytes, 16, sequence);
    bytes[24] |= origin.mask();
    for (index, assignment) in patch.assignments.iter().enumerate() {
        if let Assignment::Set { knowledge, code } = assignment {
            bytes[32 + index] = *knowledge as u8;
            write_u64(bytes, 48 + index * 8, sequence);
            bytes[176 + index] = origin as u8;
            if !recorded {
                write_u64(bytes, 192 + index * 8, sequence);
            }
            if index == 4 {
                bytes[26] = *code;
            }
            if index == 5 {
                bytes[25] = *code;
            }
        }
    }
    let loss = (0..FIELD_COUNT).any(|index| read_u64(bytes, 192 + index * 8) != 0);
    let uninterpreted = bytes[32..48].contains(&(Knowledge::Uninterpreted as u8));
    bytes[7] = (bytes[7] & 1) | (u8::from(loss) << 1) | (u8::from(uninterpreted) << 2);
}
fn tool_patch(
    wire: Version,
    input: &Incoming,
    recorded: bool,
) -> Result<Option<ToolPatch>, ProjectionError> {
    let Incoming::Update { raw, .. } = input else {
        return Ok(None);
    };
    let update = raw
        .fields()
        .map_err(|_| ProjectionError::MalformedUpdate)?
        .get("params")
        .map_err(|_| ProjectionError::MalformedUpdate)?
        .and_then(RawValueRef::object)
        .ok_or(ProjectionError::MalformedUpdate)?
        .get("update")
        .map_err(|_| ProjectionError::MalformedUpdate)?
        .and_then(RawValueRef::object)
        .ok_or(ProjectionError::MalformedUpdate)?;
    let get = |key| {
        update
            .get(key)
            .map_err(|_| ProjectionError::MalformedUpdate)
    };
    let tag = get("sessionUpdate")?
        .and_then(|v| v.string().ok())
        .ok_or(ProjectionError::MalformedUpdate)?;
    let initial = wire == Version::V1 && tag == "tool_call";
    let chunk = wire == Version::V2 && tag == "tool_call_content_chunk";
    let supported = initial || chunk || tag == "tool_call_update";
    if !supported {
        return if matches!(tag.as_str(), "tool_call" | "tool_call_content_chunk") {
            Err(ProjectionError::UnsupportedAnchorKind)
        } else {
            Ok(None)
        };
    }
    let id = get("toolCallId")?
        .and_then(|v| v.string().ok())
        .ok_or(ProjectionError::MalformedUpdate)?;
    let mut assignments = [Assignment::Keep; TOOL_FIELDS];
    if chunk {
        let content = get("content")?
            .and_then(RawValueRef::object)
            .ok_or(ProjectionError::MalformedUpdate)?;
        if content
            .get("type")
            .map_err(|_| ProjectionError::MalformedUpdate)?
            .and_then(|v| v.string().ok())
            .is_none()
        {
            return Err(ProjectionError::MalformedUpdate);
        }
        validate_meta(get("_meta")?)?;
        assignments[0] = Assignment::set(raw_knowledge(recorded, Knowledge::Deferred));
    } else {
        assignments[2] = text_assignment(get("name")?, wire, initial, true, recorded)?;
        assignments[3] = text_assignment(get("title")?, wire, initial, false, recorded)?;
        assignments[4] = small_assignment(get("kind")?, wire, initial, true, recorded)?;
        assignments[5] = small_assignment(get("status")?, wire, initial, false, recorded)?;
        assignments[0] = array_assignment(get("content")?, wire, initial, recorded)?;
        assignments[6] = array_assignment(get("locations")?, wire, initial, recorded)?;
        validate_meta(get("_meta")?)?;
        assignments[1] = match get("_meta")? {
            None => Assignment::Keep,
            Some(v) if v.is_null() => Assignment::set(if wire == Version::V1 {
                Knowledge::Uninterpreted
            } else {
                Knowledge::KnownEmpty
            }),
            Some(_) => Assignment::set(raw_knowledge(recorded, Knowledge::Deferred)),
        };
        assignments[7] = raw_assignment(get("rawInput")?, wire, initial, recorded);
        assignments[8] = raw_assignment(get("rawOutput")?, wire, initial, recorded);
    }
    Ok(Some(ToolPatch { id, assignments }))
}
const fn raw_knowledge(recorded: bool, known: Knowledge) -> Knowledge {
    if recorded {
        known
    } else {
        Knowledge::MissingRaw
    }
}
fn validate_meta(value: Option<RawValueRef<'_>>) -> Result<(), ProjectionError> {
    if value.is_none_or(|value| value.is_null() || value.object().is_some()) {
        Ok(())
    } else {
        Err(ProjectionError::MalformedUpdate)
    }
}
fn text_assignment(
    value: Option<RawValueRef<'_>>,
    wire: Version,
    initial: bool,
    name: bool,
    recorded: bool,
) -> Result<Assignment, ProjectionError> {
    match value {
        None if initial && !name => Err(ProjectionError::MalformedUpdate),
        None if initial => Ok(Assignment::set(Knowledge::KnownAbsent)),
        None => Ok(Assignment::Keep),
        Some(v) if v.kind() == RawKind::String => {
            Ok(Assignment::set(raw_knowledge(recorded, Knowledge::Known)))
        }
        Some(v) if v.is_null() && name && wire == Version::V1 && !initial => Ok(Assignment::Keep),
        Some(v) if v.is_null() && !name && initial => Err(ProjectionError::MalformedUpdate),
        Some(v) if v.is_null() => Ok(Assignment::set(if wire == Version::V2 || name {
            Knowledge::KnownAbsent
        } else {
            Knowledge::Uninterpreted
        })),
        Some(_) => Err(ProjectionError::MalformedUpdate),
    }
}
fn small_assignment(
    value: Option<RawValueRef<'_>>,
    wire: Version,
    initial: bool,
    kind: bool,
    recorded: bool,
) -> Result<Assignment, ProjectionError> {
    let code = match value {
        None => return Ok(Assignment::Keep),
        Some(v) if v.is_null() && initial => return Err(ProjectionError::MalformedUpdate),
        Some(v) if v.is_null() && wire == Version::V1 => {
            return Ok(Assignment::set(Knowledge::Uninterpreted));
        }
        Some(v) if v.is_null() => 0,
        Some(v) if v.kind() == RawKind::String => {
            let value = v.string().map_err(|_| ProjectionError::MalformedUpdate)?;
            let code = if kind {
                kind_code(&value)
            } else {
                status_code(&value, wire)
            };
            if code == 255 && wire == Version::V1 {
                return Err(ProjectionError::MalformedUpdate);
            }
            code
        }
        Some(_) => return Err(ProjectionError::MalformedUpdate),
    };
    Ok(Assignment::Set {
        knowledge: if code == 255 {
            raw_knowledge(recorded, Knowledge::Known)
        } else {
            Knowledge::Known
        },
        code,
    })
}
fn status_code(value: &str, wire: Version) -> u8 {
    match value {
        "pending" => 0,
        "in_progress" => 1,
        "completed" => 2,
        "failed" => 3,
        "cancelled" if wire == Version::V2 => 4,
        _ => 255,
    }
}
fn kind_code(value: &str) -> u8 {
    match value {
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
}
fn array_assignment(
    value: Option<RawValueRef<'_>>,
    wire: Version,
    initial: bool,
    recorded: bool,
) -> Result<Assignment, ProjectionError> {
    match value {
        None => Ok(Assignment::Keep),
        Some(v) if v.kind() == RawKind::Array => Ok(Assignment::set(raw_knowledge(
            recorded,
            Knowledge::Deferred,
        ))),
        Some(v) if v.is_null() && initial => Err(ProjectionError::MalformedUpdate),
        Some(v) if v.is_null() => Ok(Assignment::set(if wire == Version::V1 {
            Knowledge::Uninterpreted
        } else {
            Knowledge::KnownEmpty
        })),
        Some(_) => Err(ProjectionError::MalformedUpdate),
    }
}
fn raw_assignment(
    value: Option<RawValueRef<'_>>,
    wire: Version,
    initial: bool,
    recorded: bool,
) -> Assignment {
    match value {
        None => Assignment::Keep,
        Some(v) if v.is_null() && wire == Version::V1 && !initial => Assignment::Keep,
        Some(v) if v.is_null() && wire == Version::V2 => Assignment::set(Knowledge::KnownAbsent),
        Some(_) => Assignment::set(raw_knowledge(recorded, Knowledge::Deferred)),
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod tool_projection_independent;
