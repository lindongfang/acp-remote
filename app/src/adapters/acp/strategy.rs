#![allow(
    clippy::needless_pass_by_value,
    reason = "Frozen public API consumes owned snapshots/fragments rather than borrowing caller state"
)]
//! Version-selected protocol construction and shallow validation, with no IO or authorization.
use super::{
    AcpError, Capabilities, ConfigSourceStamp, ConnectionScope, ExtensionFields, McpTransport,
    MessageId, Negotiated, OptionId, OrderedRawObject, PermissionChoice, PermissionRequest,
    PromptResult, RawMessage, RawMetadata, ReportedDirectories, RpcId, SessionEvent, SessionId,
    SessionListItem, SessionListing, SessionSetup, ToolCallId, Version,
    model::{field, object, text},
    raw_json::{OwnedRawValue, RawKind, RawValueRef},
    raw_model as raw,
};
use std::{fmt, sync::Arc};
#[derive(Clone, Debug)]
pub enum RequestKind {
    Initialize(Version),
    NewSession,
    Prompt(SessionId),
    Resume(SessionId),
    Replay(SessionId),
    ListSessions,
    Close(SessionId),
}
pub struct Request {
    pub(super) scope: ConnectionScope,
    pub(super) method: &'static str,
    pub(super) params: OwnedRawValue,
    pub(super) kind: RequestKind,
    pub(super) stamp: Option<ConfigSourceStamp>,
}
impl Request {
    #[must_use]
    pub const fn kind(&self) -> &RequestKind {
        &self.kind
    }
    #[must_use]
    pub const fn params(&self) -> RawValueRef<'_> {
        self.params.as_ref()
    }
    #[must_use]
    pub const fn setup_stamp(&self) -> Option<&ConfigSourceStamp> {
        self.stamp.as_ref()
    }
}
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Request(..)")
    }
}
#[derive(Clone, Debug)]
pub struct Strategy {
    pub(super) scope: ConnectionScope,
    negotiated: Negotiated,
}
pub(super) fn at<'a>(
    mut v: RawValueRef<'a>,
    keys: &[&str],
) -> Result<Option<RawValueRef<'a>>, AcpError> {
    for key in keys {
        let Some(object) = v.object() else {
            return Ok(None);
        };
        let Some(value) = object.get(key)? else {
            return Ok(None);
        };
        v = value;
    }
    Ok(Some(v))
}
impl Strategy {
    pub(super) fn select(
        offer: Version,
        scope: ConnectionScope,
        raw: RawMessage,
        result: RawValueRef<'_>,
    ) -> Result<Self, AcpError> {
        object(result)?;
        let selected = field(result, "protocolVersion")?
            .unsigned()
            .ok_or(AcpError::InvalidMessage)?;
        let version = match selected {
            1 => Version::V1,
            2 if offer == Version::V2 => Version::V2,
            _ => return Err(AcpError::IncompatibleVersion),
        };
        let (info_key, cap_key) = if version == Version::V1 {
            ("agentInfo", "agentCapabilities")
        } else {
            ("info", "capabilities")
        };
        if version == Version::V2 || at(result, &[info_key])?.is_some_and(|v| !v.is_null()) {
            let info = field(result, info_key)?;
            for key in ["name", "version"] {
                text(field(info, key)?)?;
            }
        }
        let empty = raw::literal("{}")?;
        let caps = at(result, &[cap_key])?.unwrap_or(empty.as_ref());
        object(caps)?;
        let session_key = if version == Version::V1 {
            "sessionCapabilities"
        } else {
            "session"
        };
        let session = at(caps, &[session_key])?;
        let sessions = version == Version::V1 || session.is_some_and(|v| v.object().is_some());
        let lifecycle = |key: &str| -> Result<bool, AcpError> {
            if !sessions {
                return Ok(false);
            }
            if version == Version::V2 {
                return Ok(true);
            }
            Ok(session
                .map(|v| at(v, &[key]))
                .transpose()?
                .flatten()
                .is_some_and(|v| v.object().is_some()))
        };
        let prompt = if version == Version::V1 {
            at(caps, &["promptCapabilities"])?
        } else {
            session.map(|v| at(v, &["prompt"])).transpose()?.flatten()
        };
        let advertised = |key: &str| -> Result<bool, AcpError> {
            Ok(prompt
                .map(|v| at(v, &[key]))
                .transpose()?
                .flatten()
                .is_some_and(|v| {
                    if version == Version::V1 {
                        v.boolean() == Some(true)
                    } else {
                        v.object().is_some()
                    }
                }))
        };
        let capabilities = Capabilities {
            sessions,
            list: lifecycle("list")?,
            standard_replay: if version == Version::V1 {
                at(caps, &["loadSession"])?.and_then(RawValueRef::boolean) == Some(true)
            } else {
                sessions
            },
            resume: lifecycle("resume")?,
            close: lifecycle("close")?,
            image: advertised("image")?,
            audio: advertised("audio")?,
            embedded_context: advertised("embeddedContext")?,
        };
        Ok(Self {
            scope,
            negotiated: Negotiated {
                offered: offer,
                version,
                raw,
                capabilities,
            },
        })
    }
    #[must_use]
    pub const fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }
    #[must_use]
    pub const fn version(&self) -> Version {
        self.negotiated.version
    }
    fn request(
        &self,
        method: &'static str,
        kind: RequestKind,
        params: OwnedRawValue,
        stamp: Option<ConfigSourceStamp>,
    ) -> Request {
        Request {
            scope: self.scope.clone(),
            method,
            kind,
            params,
            stamp,
        }
    }
    const fn require(b: bool) -> Result<(), AcpError> {
        if b {
            Ok(())
        } else {
            Err(AcpError::UnsupportedCapability)
        }
    }
    pub(super) const fn require_list(&self) -> Result<(), AcpError> {
        Self::require(self.negotiated.capabilities.list)
    }
    fn setup_request(&self, kind: RequestKind, setup: &SessionSetup) -> Result<Request, AcpError> {
        if setup.wire() != self.version() {
            return Err(AcpError::ConfigurationVersionMismatch);
        }
        let session = match &kind {
            RequestKind::Resume(s) | RequestKind::Replay(s) => Some(s),
            _ => None,
        };
        let replay = matches!(kind, RequestKind::Replay(_));
        let method = match &kind {
            RequestKind::NewSession => {
                Self::require(self.negotiated.capabilities.sessions)?;
                "session/new"
            }
            RequestKind::Resume(_) => {
                Self::require(self.negotiated.capabilities.resume)?;
                "session/resume"
            }
            RequestKind::Replay(_) => {
                Self::require(self.negotiated.capabilities.standard_replay)?;
                if setup.cwd().len() > 4096 {
                    return Err(AcpError::Capacity);
                }
                if self.version() == Version::V1 {
                    "session/load"
                } else {
                    "session/resume"
                }
            }
            _ => return Err(AcpError::InvalidMessage),
        };
        self.validate_setup_caps(setup)?;
        let mcp = setup
            .mcp_servers()
            .iter()
            .map(|m| m.as_raw().to_owned())
            .collect::<Vec<_>>();
        let mut fields = vec![
            ("cwd", raw::string(setup.cwd())?),
            ("mcpServers", raw::array(&mcp)?),
        ];
        if let Some(s) = session {
            fields.insert(0, ("sessionId", raw::string(s.as_str())?));
        }
        if !setup.additional_directories().is_empty() {
            let dirs = setup
                .additional_directories()
                .iter()
                .map(|s| raw::string(s))
                .collect::<Result<Vec<_>, _>>()?;
            fields.push(("additionalDirectories", raw::array(&dirs)?));
        }
        if replay && self.version() == Version::V2 {
            fields.push(("replayFrom", raw::literal(r#"{"type":"start"}"#)?));
        }
        let params = raw::object(
            fields.iter().map(|(k, v)| (*k, v)),
            Some(setup.extra().as_raw()),
        )?;
        Ok(self.request(method, kind, params, Some(setup.source().clone())))
    }
    fn validate_setup_caps(&self, setup: &SessionSetup) -> Result<(), AcpError> {
        let result = self
            .negotiated
            .raw
            .field("result")?
            .ok_or(AcpError::InvalidMessage)?;
        let caps = at(
            result,
            &[if self.version() == Version::V1 {
                "agentCapabilities"
            } else {
                "capabilities"
            }],
        )?;
        let advertised = |path: &[&str], boolean: bool| -> Result<bool, AcpError> {
            Ok(caps
                .map(|v| at(v, path))
                .transpose()?
                .flatten()
                .is_some_and(|v| {
                    if boolean {
                        v.boolean() == Some(true)
                    } else {
                        v.object().is_some()
                    }
                }))
        };
        if !setup.additional_directories().is_empty() {
            Self::require(advertised(
                if self.version() == Version::V1 {
                    &["sessionCapabilities", "additionalDirectories"]
                } else {
                    &["session", "additionalDirectories"]
                },
                false,
            )?)?;
        }
        for server in setup.mcp_servers() {
            if server.wire() != self.version() {
                return Err(AcpError::ConfigurationVersionMismatch);
            }
            let supported = match (self.version(), server.transport()) {
                (Version::V1, McpTransport::Stdio) | (Version::V2, McpTransport::Other(_)) => true,
                (Version::V1, McpTransport::Http) => {
                    advertised(&["mcpCapabilities", "http"], true)?
                }
                (Version::V1, McpTransport::Sse) => advertised(&["mcpCapabilities", "sse"], true)?,
                (Version::V2, McpTransport::Stdio) => {
                    advertised(&["session", "mcp", "stdio"], false)?
                }
                (Version::V2, McpTransport::Http) => {
                    advertised(&["session", "mcp", "http"], false)?
                }
                _ => false,
            };
            Self::require(supported)?;
        }
        Ok(())
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn new_session(&self, setup: &SessionSetup) -> Result<Request, AcpError> {
        self.setup_request(RequestKind::NewSession, setup)
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn resume(&self, session: SessionId, setup: &SessionSetup) -> Result<Request, AcpError> {
        self.setup_request(RequestKind::Resume(session), setup)
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn replay(&self, session: SessionId, setup: &SessionSetup) -> Result<Request, AcpError> {
        self.setup_request(RequestKind::Replay(session), setup)
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn prompt(
        &self,
        session: SessionId,
        prompt: OwnedRawValue,
        extra: ExtensionFields,
    ) -> Result<Request, AcpError> {
        Self::require(self.negotiated.capabilities.sessions)?;
        extra.reject(&["sessionId", "prompt"])?;
        for block in prompt
            .as_ref()
            .array()
            .ok_or(AcpError::InvalidMessage)?
            .iter()
        {
            let kind = super::prompt_input::block_tag(block)?;
            match &*kind {
                "text" | "resource_link" => {
                    super::prompt_input::validate_block_fields(block, &kind)?;
                }
                "image" | "audio" => {
                    Self::require(if &*kind == "image" {
                        self.negotiated.capabilities.image
                    } else {
                        self.negotiated.capabilities.audio
                    })?;
                    super::prompt_input::validate_block_fields(block, &kind)?;
                }
                "resource" => {
                    Self::require(self.negotiated.capabilities.embedded_context)?;
                    super::prompt_input::validate_block_fields(block, &kind)?;
                }
                _ if self.version() == Version::V2 => {}
                _ => return Err(AcpError::UnsupportedCapability),
            }
        }
        let params = raw::object(
            [
                ("sessionId", &raw::string(session.as_str())?),
                ("prompt", &prompt),
            ],
            Some(extra.as_raw()),
        )?;
        Ok(self.request("session/prompt", RequestKind::Prompt(session), params, None))
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn list_sessions(
        &self,
        cwd: Option<&str>,
        cursor: Option<&str>,
        extra: ExtensionFields,
    ) -> Result<Request, AcpError> {
        Self::require(self.negotiated.capabilities.list)?;
        extra.reject(&["cwd", "cursor"])?;
        let mut fields = Vec::new();
        for (key, v) in [("cwd", cwd), ("cursor", cursor)] {
            if let Some(v) = v {
                if v.len() > 4096 {
                    return Err(AcpError::Capacity);
                }
                fields.push((key, raw::string(v)?));
            }
        }
        Ok(self.request(
            "session/list",
            RequestKind::ListSessions,
            raw::object(fields.iter().map(|(k, v)| (*k, v)), Some(extra.as_raw()))?,
            None,
        ))
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn close(&self, session: SessionId, extra: ExtensionFields) -> Result<Request, AcpError> {
        Self::require(self.negotiated.capabilities.close)?;
        extra.reject(&["sessionId"])?;
        let params = raw::object(
            [("sessionId", &raw::string(session.as_str())?)],
            Some(extra.as_raw()),
        )?;
        Ok(self.request("session/close", RequestKind::Close(session), params, None))
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    #[allow(
        clippy::option_if_let_else,
        reason = "Reported omission, empty, null and uninterpreted states stay explicit"
    )]
    pub fn session_listing(&self, result: RawValueRef<'_>) -> Result<SessionListing, AcpError> {
        Self::require(self.negotiated.capabilities.list)?;
        let entries = field(result, "sessions")?
            .array()
            .ok_or(AcpError::InvalidMessage)?;
        let mut sessions = Vec::new();
        sessions
            .try_reserve_exact(entries.len())
            .map_err(|_| AcpError::Allocation)?;
        for entry in entries.iter() {
            let session = SessionId::parse(field(entry, "sessionId")?)?;
            let cwd = text(field(entry, "cwd")?)?;
            if cwd.len() > 4096 {
                return Err(AcpError::Capacity);
            }
            let directories = match at(entry, &["additionalDirectories"])? {
                None => ReportedDirectories::Omitted,
                Some(v) if v.is_null() => ReportedDirectories::Null,
                Some(v) => match v.array() {
                    Some(a) => {
                        let dirs = a.iter().map(text).collect::<Result<Vec<_>, _>>();
                        match dirs {
                            Ok(d) if d.is_empty() => ReportedDirectories::Empty,
                            Ok(d) => ReportedDirectories::Known(d),
                            Err(_) => ReportedDirectories::Uninterpreted(v.to_owned()),
                        }
                    }
                    None => ReportedDirectories::Uninterpreted(v.to_owned()),
                },
            };
            sessions.push(SessionListItem {
                session,
                cwd,
                directories,
                raw: entry.to_owned(),
            });
        }
        let next_cursor = at(result, &["nextCursor"])?
            .filter(|v| !v.is_null())
            .map(text)
            .transpose()?;
        if next_cursor.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err(AcpError::Capacity);
        }
        Ok(SessionListing {
            sessions,
            next_cursor,
            raw: result.to_owned(),
        })
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn prompt_result(&self, result: RawValueRef<'_>) -> Result<PromptResult, AcpError> {
        object(result)?;
        if self.version() == Version::V1 {
            let stop_reason = text(field(result, "stopReason")?)?;
            if !matches!(
                &*stop_reason,
                "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled"
            ) {
                return Err(AcpError::InvalidMessage);
            }
            Ok(PromptResult::Completed { stop_reason })
        } else {
            Ok(PromptResult::Inserted {
                message: MessageId::parse(field(result, "messageId")?)?,
            })
        }
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn update(&self, params: RawValueRef<'_>) -> Result<SessionEvent, AcpError> {
        let session = SessionId::parse(field(params, "sessionId")?)?;
        let update = field(params, "update")?;
        let kind = text(field(update, "sessionUpdate")?)?;
        match &*kind {
            "state_update" if self.version() == Version::V2 => Ok(SessionEvent::State {
                session,
                state: text(field(update, "state")?)?,
                stop_reason: at(update, &["stopReason"])?
                    .filter(|v| v.kind() == RawKind::String)
                    .map(text)
                    .transpose()?,
            }),
            "tool_call" | "tool_call_update" => Ok(SessionEvent::Tool {
                session,
                tool: ToolCallId::parse(field(update, "toolCallId")?)?,
            }),
            "user_message"
            | "user_message_chunk"
            | "agent_message"
            | "agent_message_chunk"
            | "agent_thought"
            | "agent_thought_chunk" => Ok(SessionEvent::Message {
                session,
                message: at(update, &["messageId"])?
                    .filter(|v| self.version() != Version::V1 || !v.is_null())
                    .map(MessageId::parse)
                    .transpose()?,
                kind,
            }),
            _ => Ok(SessionEvent::Other { session, kind }),
        }
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn permission(
        &self,
        id: RpcId,
        raw: RawMessage,
        params: RawValueRef<'_>,
    ) -> Result<PermissionRequest, AcpError> {
        let session = SessionId::parse(field(params, "sessionId")?)?;
        let (supported_subject, tool) = if self.version() == Version::V1 {
            (
                true,
                Some(ToolCallId::parse(field(
                    field(params, "toolCall")?,
                    "toolCallId",
                )?)?),
            )
        } else {
            text(field(params, "title")?)?;
            match at(params, &["subject"])? {
                None => (true, None),
                Some(s) if s.is_null() => (true, None),
                Some(s) => {
                    let kind = text(field(s, "type")?)?;
                    match &*kind {
                        "tool_call" => (
                            true,
                            Some(ToolCallId::parse(field(
                                field(s, "toolCall")?,
                                "toolCallId",
                            )?)?),
                        ),
                        "command" => {
                            text(field(s, "command")?)?;
                            text(field(s, "cwd")?)?;
                            (
                                true,
                                at(s, &["toolCallId"])?
                                    .filter(|v| !v.is_null())
                                    .map(ToolCallId::parse)
                                    .transpose()?,
                            )
                        }
                        _ => (false, None),
                    }
                }
            }
        };
        let options = field(params, "options")?
            .array()
            .ok_or(AcpError::InvalidMessage)?;
        if self.version() == Version::V2 && options.is_empty() {
            return Err(AcpError::InvalidMessage);
        }
        let mut parsed = Vec::new();
        parsed
            .try_reserve_exact(options.len())
            .map_err(|_| AcpError::Allocation)?;
        for option in options.iter() {
            text(field(option, "name")?)?;
            parsed.push(PermissionChoice {
                id: OptionId::parse(field(option, "optionId")?)?,
                kind: text(field(option, "kind")?)?,
                raw: option.to_owned(),
            });
        }
        Ok(PermissionRequest {
            connection: None,
            id,
            session,
            tool,
            supported_subject,
            options: parsed,
            raw,
        })
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn tool_name_patch(&self, update: RawValueRef<'_>) -> Result<ToolNamePatch, AcpError> {
        match at(update, &["name"])? {
            None => Ok(ToolNamePatch::Unchanged),
            Some(v) if v.is_null() => Ok(if self.version() == Version::V1 {
                ToolNamePatch::Unchanged
            } else {
                ToolNamePatch::Clear
            }),
            Some(v) => Ok(ToolNamePatch::Set(text(v)?)),
        }
    }
    /// # Errors
    /// Rejects invalid shapes, unsupported capabilities, incompatible configuration or allocation failure.
    pub fn tool_metadata_patch(
        &self,
        current: &RawMetadata,
        update: RawValueRef<'_>,
    ) -> Result<RawMetadata, AcpError> {
        Self::require(self.version() == Version::V2)?;
        let Some(patch) = at(update, &["_meta"])? else {
            return Ok(current.clone());
        };
        if patch.is_null() {
            return Ok(RawMetadata::Null);
        }
        let patch = object(patch)?;
        let mut merged = current.as_object().cloned().unwrap_or(OrderedRawObject {
            members: Vec::new(),
        });
        for (key, value) in patch.iter() {
            patch.get(key)?;
            merged.get(key)?;
            if let Some(i) = merged.members.iter().position(|(k, _)| &**k == key) {
                if value.is_null() {
                    merged.members.remove(i);
                } else {
                    merged.members[i].1 = value.to_owned();
                }
            } else if !value.is_null() {
                merged
                    .members
                    .try_reserve(1)
                    .map_err(|_| AcpError::Allocation)?;
                merged.members.push((key.into(), value.to_owned()));
            }
        }
        Ok(RawMetadata::Object(merged))
    }
}
pub(super) fn initialize(scope: ConnectionScope, offer: Version) -> Result<Request, AcpError> {
    let info = raw::object(
        [
            ("name", &raw::string("acp-remote")?),
            ("version", &raw::string(crate::VERSION)?),
        ],
        None,
    )?;
    let fields = [
        (
            "protocolVersion",
            raw::literal(&offer.number().to_string())?,
        ),
        (
            if offer == Version::V1 {
                "clientCapabilities"
            } else {
                "capabilities"
            },
            raw::literal("{}")?,
        ),
        (
            if offer == Version::V1 {
                "clientInfo"
            } else {
                "info"
            },
            info,
        ),
    ];
    Ok(Request {
        scope,
        method: "initialize",
        kind: RequestKind::Initialize(offer),
        params: raw::object(fields.iter().map(|(k, v)| (*k, v)), None)?,
        stamp: None,
    })
}
#[derive(Clone, Eq, PartialEq)]
pub enum ToolNamePatch {
    Unchanged,
    Clear,
    Set(Arc<str>),
}

impl fmt::Debug for ToolNamePatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unchanged => "ToolNamePatch::Unchanged",
            Self::Clear => "ToolNamePatch::Clear",
            Self::Set(_) => "ToolNamePatch::Set(..)",
        })
    }
}
