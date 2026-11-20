//! Pure syntax for session control metadata. These values grant no authority.
use crate::{
    claims::Id128,
    strict_json::{MAX_PROTOCOL_JSON_DEPTH, parse_object},
};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

mod history;
pub use history::{HistoryMode, HistoryPageInput, HistoryTarget, PageOptions};

const MAX_SOURCE_TEXT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SessionProtocolError {
    #[error("invalid session command")]
    InvalidCommand,
    #[error("invalid session identifier")]
    InvalidId,
    #[error("invalid session decimal")]
    InvalidDecimal,
}

/// An exact unsigned value whose wire representation is a decimal string.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DecimalU64(u64);
impl DecimalU64 {
    /// # Errors
    /// Rejects noncanonical strings and values outside the unsigned 64-bit range.
    pub fn parse(value: &str) -> Result<Self, SessionProtocolError> {
        let bytes = value.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 20
            || (bytes[0] == b'0' && bytes.len() != 1)
            || !bytes.iter().all(u8::is_ascii_digit)
        {
            return Err(SessionProtocolError::InvalidDecimal);
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| SessionProtocolError::InvalidDecimal)
    }
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}
impl Serialize for DecimalU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadPurpose {
    Submit,
    Edit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewOpenReason {
    FirstOpen,
    ExplicitRefresh,
    Reopen,
    Reconnect,
    Reacquire,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum QueueMutation {
    Submit {
        body_id: Id128,
    },
    Edit {
        item_id: Id128,
        expected_revision: DecimalU64,
        body_id: Id128,
    },
    Cancel {
        item_id: Id128,
        expected_revision: DecimalU64,
    },
    Prioritize {
        item_id: Id128,
        expected_revision: DecimalU64,
    },
    Resume {},
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLaunchSelectionInput {
    pub source_id: Id128,
    pub harness_id: Id128,
    pub profile_id: Id128,
    pub profile_revision: DecimalU64,
    pub workspace_id: Id128,
    pub launch_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SourceOperationInput {
    Discover { cursor: Option<String> },
    Create {},
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "action", content = "payload", rename_all_fields = "camelCase")]
pub enum SessionAction {
    #[serde(rename = "product.status")]
    ProductStatus {},
    #[serde(rename = "input.upload.begin")]
    InputUploadBegin {
        target_id: Id128,
        purpose: UploadPurpose,
    },
    #[serde(rename = "input.upload.finish")]
    InputUploadFinish {
        upload_id: Id128,
        total_bytes: DecimalU64,
    },
    #[serde(rename = "input.upload.status")]
    InputUploadStatus { upload_id: Id128 },
    #[serde(rename = "input.upload.release")]
    InputUploadRelease { upload_id: Id128 },
    #[serde(rename = "input.register")]
    InputRegister {
        target_id: Id128,
        operation: QueueMutation,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_control_epoch: Option<DecimalU64>,
    },
    #[serde(rename = "input.consume")]
    InputConsume {
        target_id: Id128,
        registration_id: Id128,
        body_id: Option<Id128>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_control_epoch: Option<DecimalU64>,
    },
    #[serde(rename = "input.request")]
    InputRequest {
        target_id: Id128,
        registration_id: Id128,
    },
    #[serde(rename = "input.queue")]
    InputQueue { target_id: Id128 },
    #[serde(rename = "input.body.open")]
    InputBodyOpen {
        target_id: Id128,
        item_id: Id128,
        expected_revision: DecimalU64,
    },
    #[serde(rename = "catalog.upload.begin")]
    CatalogUploadBegin {},
    #[serde(rename = "catalog.upload.finish")]
    CatalogUploadFinish {
        upload_id: Id128,
        total_bytes: DecimalU64,
    },
    #[serde(rename = "catalog.upload.status")]
    CatalogUploadStatus { upload_id: Id128 },
    #[serde(rename = "catalog.upload.release")]
    CatalogUploadRelease { upload_id: Id128 },
    #[serde(rename = "catalog.register")]
    CatalogRegister { document_upload_id: Id128 },
    #[serde(rename = "catalog.apply")]
    CatalogApply { catalog_request_id: Id128 },
    #[serde(rename = "catalog.request")]
    CatalogRequest { catalog_request_id: Id128 },
    #[serde(rename = "workspace.register")]
    WorkspaceRegister { path: String },
    #[serde(rename = "target.register")]
    TargetRegister {
        source_id: Id128,
        native_session_id: String,
        workspace_id: Id128,
    },
    #[serde(rename = "catalog.inventory")]
    CatalogInventory {},
    #[serde(rename = "session.capacity.inspect")]
    SessionCapacityInspect {},
    #[serde(rename = "approval.list")]
    ApprovalList {},
    #[serde(rename = "approval.read")]
    ApprovalRead { approval_id: Id128 },
    #[serde(rename = "approval.register")]
    ApprovalRegister {
        approval_id: Id128,
        choice_id: Id128,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_control_epoch: Option<DecimalU64>,
    },
    #[serde(rename = "approval.decide")]
    ApprovalDecide {
        approval_registration_id: Id128,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_control_epoch: Option<DecimalU64>,
    },
    #[serde(rename = "session.control.inspect")]
    SessionControlInspect { target_id: Id128 },
    #[serde(rename = "session.control.takeover")]
    SessionControlTakeover { target_id: Id128 },
    #[serde(rename = "session.control.release")]
    SessionControlRelease {
        target_id: Id128,
        #[serde(skip_serializing_if = "Option::is_none")]
        expected_control_epoch: Option<DecimalU64>,
    },
    #[serde(rename = "approval.status")]
    ApprovalStatus { approval_id: Id128 },
    #[serde(rename = "native.release.register")]
    NativeReleaseRegister { target_id: Id128 },
    #[serde(rename = "native.release")]
    NativeRelease { release_operation_id: Id128 },
    #[serde(rename = "native.release.status")]
    NativeReleaseStatus { release_operation_id: Id128 },
    #[serde(rename = "source.session.register")]
    SourceSessionRegister {
        selection: SourceLaunchSelectionInput,
        operation: SourceOperationInput,
    },
    #[serde(rename = "source.session.execute")]
    SourceSessionExecute { source_operation_id: Id128 },
    #[serde(rename = "source.session.status")]
    SourceSessionStatus { source_operation_id: Id128 },
    #[serde(rename = "source.session.reconcile")]
    SourceSessionReconcile { source_operation_id: Id128 },
    #[serde(rename = "source.session.result.open")]
    SourceSessionResultOpen { source_operation_id: Id128 },
    #[serde(rename = "source.session.result.release")]
    SourceSessionResultRelease { source_operation_id: Id128 },
    #[serde(rename = "history.page")]
    HistoryPage(HistoryPageInput),
    #[serde(rename = "view.open")]
    ViewOpen {
        target_id: Id128,
        reason: ViewOpenReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        page: Option<PageOptions>,
    },
    #[serde(rename = "view.open_source")]
    ViewOpenSource {
        selection: SourceLaunchSelectionInput,
        native_session_id: String,
        reason: ViewOpenReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        page: Option<PageOptions>,
    },
    #[serde(rename = "view.close")]
    ViewClose { stream_id: Id128 },
    #[serde(rename = "view.original")]
    ViewOriginal { stream_id: Id128 },
    #[serde(rename = "view.rebind")]
    ViewRebind { stream_id: Id128 },
}

/// A syntactically valid request, without authentication or operation ownership.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionCommand {
    v: u8,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(rename = "requestId")]
    request_id: Id128,
    #[serde(flatten)]
    action: SessionAction,
}
impl SessionCommand {
    #[must_use]
    pub const fn new(request_id: Id128, action: SessionAction) -> Self {
        Self {
            v: 1,
            kind: "command",
            request_id,
            action,
        }
    }
    #[must_use]
    pub const fn request_id(&self) -> Id128 {
        self.request_id
    }
    #[must_use]
    pub const fn action(&self) -> &SessionAction {
        &self.action
    }
}

/// Parses metadata using the existing control frame and strict JSON boundaries.
/// # Errors
/// Returns a static syntax error without retaining or exposing input text.
pub fn parse_session_command(input: &[u8]) -> Result<SessionCommand, SessionProtocolError> {
    if input.len() > super::MAX_MANAGEMENT_FRAME_BYTES {
        return Err(SessionProtocolError::InvalidCommand);
    }
    let envelope = parse_object(input, MAX_PROTOCOL_JSON_DEPTH)
        .map_err(|_| SessionProtocolError::InvalidCommand)?;
    closed(&envelope, &["v", "type", "requestId", "action", "payload"])?;
    // Preserve the original Rust control parser's integer version semantics.
    if envelope.get("v").and_then(Value::as_u64) != Some(1)
        || envelope.get("type").and_then(Value::as_str) != Some("command")
    {
        return Err(SessionProtocolError::InvalidCommand);
    }
    let request_id = id(envelope.get("requestId"))?;
    let action = text(envelope.get("action"))?;
    let payload = object(envelope.get("payload"))?;
    Ok(SessionCommand::new(
        request_id,
        parse_action(action, payload)?,
    ))
}

/// Encodes metadata with exact string representations for identifiers and u64s.
/// # Errors
/// Returns a static error if serialization or the control syntax check fails.
pub fn encode_session_command(command: &SessionCommand) -> Result<String, SessionProtocolError> {
    let wire = serde_json::to_string(command).map_err(|_| SessionProtocolError::InvalidCommand)?;
    parse_session_command(wire.as_bytes())?;
    Ok(wire)
}

fn closed(value: &Map<String, Value>, fields: &[&str]) -> Result<(), SessionProtocolError> {
    if value.len() != fields.len() || fields.iter().any(|field| !value.contains_key(*field)) {
        return Err(SessionProtocolError::InvalidCommand);
    }
    Ok(())
}
fn closed_with_optional(
    value: &Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<(), SessionProtocolError> {
    if required.iter().any(|field| !value.contains_key(*field))
        || value
            .keys()
            .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        return Err(SessionProtocolError::InvalidCommand);
    }
    Ok(())
}
fn optional_decimal(
    value: &Map<String, Value>,
    key: &str,
) -> Result<Option<DecimalU64>, SessionProtocolError> {
    if !value.contains_key(key) {
        return Ok(None);
    }
    decimal(value.get(key)).map(Some)
}
fn object(value: Option<&Value>) -> Result<&Map<String, Value>, SessionProtocolError> {
    value
        .and_then(Value::as_object)
        .ok_or(SessionProtocolError::InvalidCommand)
}
fn text(value: Option<&Value>) -> Result<&str, SessionProtocolError> {
    value
        .and_then(Value::as_str)
        .ok_or(SessionProtocolError::InvalidCommand)
}
fn bounded_non_empty_text(
    value: Option<&Value>,
    reject_nul: bool,
) -> Result<String, SessionProtocolError> {
    let raw = text(value)?;
    if raw.is_empty() || raw.len() > MAX_SOURCE_TEXT_BYTES || (reject_nul && raw.contains('\0')) {
        return Err(SessionProtocolError::InvalidCommand);
    }
    Ok(raw.to_owned())
}
fn id(value: Option<&Value>) -> Result<Id128, SessionProtocolError> {
    value
        .and_then(Value::as_str)
        .ok_or(SessionProtocolError::InvalidId)
        .and_then(|value| {
            Id128::parse_base64url(value).map_err(|_| SessionProtocolError::InvalidId)
        })
}
fn decimal(value: Option<&Value>) -> Result<DecimalU64, SessionProtocolError> {
    value
        .and_then(Value::as_str)
        .ok_or(SessionProtocolError::InvalidDecimal)
        .and_then(DecimalU64::parse)
}

fn mutation(value: Option<&Value>) -> Result<QueueMutation, SessionProtocolError> {
    let value = object(value)?;
    match text(value.get("kind"))? {
        "submit" => {
            closed(value, &["kind", "bodyId"])?;
            Ok(QueueMutation::Submit {
                body_id: id(value.get("bodyId"))?,
            })
        }
        "edit" => {
            closed(value, &["kind", "itemId", "expectedRevision", "bodyId"])?;
            Ok(QueueMutation::Edit {
                item_id: id(value.get("itemId"))?,
                expected_revision: decimal(value.get("expectedRevision"))?,
                body_id: id(value.get("bodyId"))?,
            })
        }
        "cancel" | "prioritize" => {
            closed(value, &["kind", "itemId", "expectedRevision"])?;
            let item_id = id(value.get("itemId"))?;
            let expected_revision = decimal(value.get("expectedRevision"))?;
            Ok(if text(value.get("kind"))? == "cancel" {
                QueueMutation::Cancel {
                    item_id,
                    expected_revision,
                }
            } else {
                QueueMutation::Prioritize {
                    item_id,
                    expected_revision,
                }
            })
        }
        "resume" => {
            closed(value, &["kind"])?;
            Ok(QueueMutation::Resume {})
        }
        _ => Err(SessionProtocolError::InvalidCommand),
    }
}

fn source_selection(
    value: Option<&Value>,
) -> Result<SourceLaunchSelectionInput, SessionProtocolError> {
    let value = object(value)?;
    closed(
        value,
        &[
            "sourceId",
            "harnessId",
            "profileId",
            "profileRevision",
            "workspaceId",
            "launchPath",
        ],
    )?;
    Ok(SourceLaunchSelectionInput {
        source_id: id(value.get("sourceId"))?,
        harness_id: id(value.get("harnessId"))?,
        profile_id: id(value.get("profileId"))?,
        profile_revision: decimal(value.get("profileRevision"))?,
        workspace_id: id(value.get("workspaceId"))?,
        launch_path: bounded_non_empty_text(value.get("launchPath"), true)?,
    })
}

fn source_operation(value: Option<&Value>) -> Result<SourceOperationInput, SessionProtocolError> {
    let value = object(value)?;
    match text(value.get("kind"))? {
        "discover" => {
            closed(value, &["kind", "cursor"])?;
            let cursor = match value.get("cursor") {
                Some(Value::Null) => None,
                other => Some(bounded_non_empty_text(other, false)?),
            };
            Ok(SourceOperationInput::Discover { cursor })
        }
        "create" => {
            closed(value, &["kind"])?;
            Ok(SourceOperationInput::Create {})
        }
        _ => Err(SessionProtocolError::InvalidCommand),
    }
}

fn view_open_reason(value: Option<&Value>) -> Result<ViewOpenReason, SessionProtocolError> {
    match text(value)? {
        "first_open" => Ok(ViewOpenReason::FirstOpen),
        "explicit_refresh" => Ok(ViewOpenReason::ExplicitRefresh),
        "reopen" => Ok(ViewOpenReason::Reopen),
        "reconnect" => Ok(ViewOpenReason::Reconnect),
        "reacquire" => Ok(ViewOpenReason::Reacquire),
        _ => Err(SessionProtocolError::InvalidCommand),
    }
}

fn parse_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    match action {
        "session.capacity.inspect" => {
            closed(p, &[])?;
            Ok(SessionAction::SessionCapacityInspect {})
        }
        "product.status" => {
            closed(p, &[])?;
            Ok(SessionAction::ProductStatus {})
        }
        "input.upload.begin"
        | "input.upload.finish"
        | "input.upload.status"
        | "input.upload.release"
        | "input.register"
        | "input.consume"
        | "input.request"
        | "input.queue"
        | "input.body.open" => parse_input_action(action, p),
        "catalog.upload.begin"
        | "catalog.upload.finish"
        | "catalog.upload.status"
        | "catalog.upload.release"
        | "catalog.register"
        | "catalog.apply"
        | "catalog.request"
        | "workspace.register"
        | "target.register"
        | "catalog.inventory" => parse_catalog_action(action, p),
        "approval.list" | "approval.read" | "approval.register" | "approval.decide"
        | "approval.status" => parse_approval_action(action, p),
        "native.release.register" | "native.release" | "native.release.status" => {
            parse_release_action(action, p)
        }
        "source.session.register"
        | "source.session.execute"
        | "source.session.status"
        | "source.session.reconcile"
        | "source.session.result.open"
        | "source.session.result.release" => parse_source_session_action(action, p),
        "session.control.inspect" | "session.control.takeover" | "session.control.release" => {
            parse_session_control_action(action, p)
        }
        "history.page" => history::parse_page(p).map(SessionAction::HistoryPage),
        "view.open" | "view.open_source" | "view.close" | "view.original" | "view.rebind" => {
            parse_view_action(action, p)
        }
        _ => Err(SessionProtocolError::InvalidCommand),
    }
}

fn parse_input_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "input.upload.begin" => {
            closed(p, &["targetId", "purpose"])?;
            let purpose = match text(p.get("purpose"))? {
                "submit" => UploadPurpose::Submit,
                "edit" => UploadPurpose::Edit,
                _ => return Err(SessionProtocolError::InvalidCommand),
            };
            SessionAction::InputUploadBegin {
                target_id: id(p.get("targetId"))?,
                purpose,
            }
        }
        "input.upload.finish" => {
            closed(p, &["uploadId", "totalBytes"])?;
            SessionAction::InputUploadFinish {
                upload_id: id(p.get("uploadId"))?,
                total_bytes: decimal(p.get("totalBytes"))?,
            }
        }
        "input.upload.status" => {
            closed(p, &["uploadId"])?;
            SessionAction::InputUploadStatus {
                upload_id: id(p.get("uploadId"))?,
            }
        }
        "input.upload.release" => {
            closed(p, &["uploadId"])?;
            SessionAction::InputUploadRelease {
                upload_id: id(p.get("uploadId"))?,
            }
        }
        "input.register" => {
            closed_with_optional(p, &["targetId", "operation"], &["expectedControlEpoch"])?;
            SessionAction::InputRegister {
                target_id: id(p.get("targetId"))?,
                operation: mutation(p.get("operation"))?,
                expected_control_epoch: optional_decimal(p, "expectedControlEpoch")?,
            }
        }
        "input.consume" => {
            closed_with_optional(
                p,
                &["targetId", "registrationId", "bodyId"],
                &["expectedControlEpoch"],
            )?;
            let body_id = match p.get("bodyId") {
                Some(Value::Null) => None,
                value => Some(id(value)?),
            };
            SessionAction::InputConsume {
                target_id: id(p.get("targetId"))?,
                registration_id: id(p.get("registrationId"))?,
                body_id,
                expected_control_epoch: optional_decimal(p, "expectedControlEpoch")?,
            }
        }
        "input.request" => {
            closed(p, &["targetId", "registrationId"])?;
            SessionAction::InputRequest {
                target_id: id(p.get("targetId"))?,
                registration_id: id(p.get("registrationId"))?,
            }
        }
        "input.queue" => {
            closed(p, &["targetId"])?;
            SessionAction::InputQueue {
                target_id: id(p.get("targetId"))?,
            }
        }
        "input.body.open" => {
            closed(p, &["targetId", "itemId", "expectedRevision"])?;
            SessionAction::InputBodyOpen {
                target_id: id(p.get("targetId"))?,
                item_id: id(p.get("itemId"))?,
                expected_revision: decimal(p.get("expectedRevision"))?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_catalog_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "catalog.upload.begin" => {
            closed(p, &[])?;
            SessionAction::CatalogUploadBegin {}
        }
        "catalog.upload.finish" => {
            closed(p, &["uploadId", "totalBytes"])?;
            SessionAction::CatalogUploadFinish {
                upload_id: id(p.get("uploadId"))?,
                total_bytes: decimal(p.get("totalBytes"))?,
            }
        }
        "catalog.upload.status" => {
            closed(p, &["uploadId"])?;
            SessionAction::CatalogUploadStatus {
                upload_id: id(p.get("uploadId"))?,
            }
        }
        "catalog.upload.release" => {
            closed(p, &["uploadId"])?;
            SessionAction::CatalogUploadRelease {
                upload_id: id(p.get("uploadId"))?,
            }
        }
        "catalog.register" => {
            closed(p, &["documentUploadId"])?;
            SessionAction::CatalogRegister {
                document_upload_id: id(p.get("documentUploadId"))?,
            }
        }
        "catalog.apply" => {
            closed(p, &["catalogRequestId"])?;
            SessionAction::CatalogApply {
                catalog_request_id: id(p.get("catalogRequestId"))?,
            }
        }
        "catalog.request" => {
            closed(p, &["catalogRequestId"])?;
            SessionAction::CatalogRequest {
                catalog_request_id: id(p.get("catalogRequestId"))?,
            }
        }
        "workspace.register" => {
            closed(p, &["path"])?;
            SessionAction::WorkspaceRegister {
                path: bounded_non_empty_text(p.get("path"), true)?,
            }
        }
        "target.register" => {
            closed(p, &["sourceId", "nativeSessionId", "workspaceId"])?;
            SessionAction::TargetRegister {
                source_id: id(p.get("sourceId"))?,
                native_session_id: bounded_non_empty_text(p.get("nativeSessionId"), true)?,
                workspace_id: id(p.get("workspaceId"))?,
            }
        }
        "catalog.inventory" => {
            closed(p, &[])?;
            SessionAction::CatalogInventory {}
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_approval_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "approval.list" => {
            closed(p, &[])?;
            SessionAction::ApprovalList {}
        }
        "approval.read" => {
            closed(p, &["approvalId"])?;
            SessionAction::ApprovalRead {
                approval_id: id(p.get("approvalId"))?,
            }
        }
        "approval.register" => {
            closed_with_optional(p, &["approvalId", "choiceId"], &["expectedControlEpoch"])?;
            SessionAction::ApprovalRegister {
                approval_id: id(p.get("approvalId"))?,
                choice_id: id(p.get("choiceId"))?,
                expected_control_epoch: optional_decimal(p, "expectedControlEpoch")?,
            }
        }
        "approval.decide" => {
            closed_with_optional(p, &["approvalRegistrationId"], &["expectedControlEpoch"])?;
            SessionAction::ApprovalDecide {
                approval_registration_id: id(p.get("approvalRegistrationId"))?,
                expected_control_epoch: optional_decimal(p, "expectedControlEpoch")?,
            }
        }
        "approval.status" => {
            closed(p, &["approvalId"])?;
            SessionAction::ApprovalStatus {
                approval_id: id(p.get("approvalId"))?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_release_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "native.release.register" => {
            closed(p, &["targetId"])?;
            SessionAction::NativeReleaseRegister {
                target_id: id(p.get("targetId"))?,
            }
        }
        "native.release" => {
            closed(p, &["releaseOperationId"])?;
            SessionAction::NativeRelease {
                release_operation_id: id(p.get("releaseOperationId"))?,
            }
        }
        "native.release.status" => {
            closed(p, &["releaseOperationId"])?;
            SessionAction::NativeReleaseStatus {
                release_operation_id: id(p.get("releaseOperationId"))?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_source_session_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "source.session.register" => {
            closed(p, &["selection", "operation"])?;
            SessionAction::SourceSessionRegister {
                selection: source_selection(p.get("selection"))?,
                operation: source_operation(p.get("operation"))?,
            }
        }
        "source.session.execute" => {
            closed(p, &["sourceOperationId"])?;
            SessionAction::SourceSessionExecute {
                source_operation_id: id(p.get("sourceOperationId"))?,
            }
        }
        "source.session.status" => {
            closed(p, &["sourceOperationId"])?;
            SessionAction::SourceSessionStatus {
                source_operation_id: id(p.get("sourceOperationId"))?,
            }
        }
        "source.session.reconcile" => {
            closed(p, &["sourceOperationId"])?;
            SessionAction::SourceSessionReconcile {
                source_operation_id: id(p.get("sourceOperationId"))?,
            }
        }
        "source.session.result.open" => {
            closed(p, &["sourceOperationId"])?;
            SessionAction::SourceSessionResultOpen {
                source_operation_id: id(p.get("sourceOperationId"))?,
            }
        }
        "source.session.result.release" => {
            closed(p, &["sourceOperationId"])?;
            SessionAction::SourceSessionResultRelease {
                source_operation_id: id(p.get("sourceOperationId"))?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_session_control_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "session.control.inspect" => {
            closed(p, &["targetId"])?;
            SessionAction::SessionControlInspect {
                target_id: id(p.get("targetId"))?,
            }
        }
        "session.control.takeover" => {
            closed(p, &["targetId"])?;
            SessionAction::SessionControlTakeover {
                target_id: id(p.get("targetId"))?,
            }
        }
        "session.control.release" => {
            closed_with_optional(p, &["targetId"], &["expectedControlEpoch"])?;
            SessionAction::SessionControlRelease {
                target_id: id(p.get("targetId"))?,
                expected_control_epoch: optional_decimal(p, "expectedControlEpoch")?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

fn parse_view_action(
    action: &str,
    p: &Map<String, Value>,
) -> Result<SessionAction, SessionProtocolError> {
    Ok(match action {
        "view.open" => {
            closed_with_optional(p, &["targetId", "reason"], &["page"])?;
            SessionAction::ViewOpen {
                target_id: id(p.get("targetId"))?,
                reason: view_open_reason(p.get("reason"))?,
                page: p.get("page").map(history::parse_options).transpose()?,
            }
        }
        "view.open_source" => {
            closed_with_optional(p, &["selection", "nativeSessionId", "reason"], &["page"])?;
            SessionAction::ViewOpenSource {
                selection: source_selection(p.get("selection"))?,
                native_session_id: bounded_non_empty_text(p.get("nativeSessionId"), true)?,
                reason: view_open_reason(p.get("reason"))?,
                page: p.get("page").map(history::parse_options).transpose()?,
            }
        }
        "view.close" => {
            closed(p, &["streamId"])?;
            SessionAction::ViewClose {
                stream_id: id(p.get("streamId"))?,
            }
        }
        "view.original" => {
            closed(p, &["streamId"])?;
            SessionAction::ViewOriginal {
                stream_id: id(p.get("streamId"))?,
            }
        }
        "view.rebind" => {
            closed(p, &["streamId"])?;
            SessionAction::ViewRebind {
                stream_id: id(p.get("streamId"))?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    })
}

#[cfg(test)]
mod tests;
