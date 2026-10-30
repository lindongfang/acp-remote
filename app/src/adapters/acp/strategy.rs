use super::model::{field, object};
use super::{
    AcpError, Capabilities, MessageId, Negotiated, OptionId, PermissionChoice, PermissionRequest,
    PromptResult, RawMessage, RpcId, SessionEvent, SessionId, ToolCallId, Version,
};
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone, Debug)]
pub enum RequestKind {
    Initialize(Version),
    NewSession,
    Prompt(SessionId),
    Resume(SessionId),
    Close(SessionId),
}
pub struct Request {
    scope: Option<Arc<()>>,
    pub(super) method: &'static str,
    pub(super) params: Value,
    pub(super) kind: RequestKind,
}
/// Frozen strategy selected by one initialize response, independent of program identity.
#[derive(Clone, Debug)]
pub struct Strategy {
    scope: Arc<()>,
    negotiated: Negotiated,
}
impl Strategy {
    pub(super) fn select(
        offer: Version,
        raw: RawMessage,
        result: &Value,
    ) -> Result<Self, AcpError> {
        object(result)?;
        let selected = field(result, "protocolVersion")?
            .as_u64()
            .ok_or(AcpError::InvalidMessage)?;
        let version = match selected {
            1 => Version::V1,
            2 if offer == Version::V2 => Version::V2,
            _ => return Err(AcpError::IncompatibleVersion),
        };
        let (info_key, cap_key) = match version {
            Version::V1 => ("agentInfo", "agentCapabilities"),
            Version::V2 => ("info", "capabilities"),
        };
        let info = result.get(info_key);
        if version == Version::V2 || info.is_some_and(|v| !v.is_null()) {
            let info = info.ok_or(AcpError::InvalidMessage)?;
            object(info)?;
            for k in ["name", "version"] {
                field(info, k)?.as_str().ok_or(AcpError::InvalidMessage)?;
            }
        }
        // A selected-v1 response may omit its optional v1 fields; v2 aliases are never read as v1 capabilities.
        let caps = result.get(cap_key).cloned().unwrap_or_else(|| json!({}));
        object(&caps)?;
        let session = match version {
            Version::V1 => caps.get("sessionCapabilities"),
            Version::V2 => caps.get("session"),
        };
        let supports_session = version == Version::V1 || session.is_some_and(Value::is_object);
        let prompt = match version {
            Version::V1 => caps.get("promptCapabilities"),
            Version::V2 => session.and_then(|v| v.get("prompt")),
        };
        let advertised = |name: &str| {
            prompt
                .and_then(|v| v.get(name))
                .is_some_and(|v| match version {
                    Version::V1 => v == true,
                    Version::V2 => v.is_object(),
                })
        };
        let lifecycle = |name: &str| {
            supports_session
                && (version == Version::V2
                    || session
                        .and_then(|s| s.get(name))
                        .is_some_and(Value::is_object))
        };
        let capabilities = Capabilities {
            sessions: supports_session,
            resume: lifecycle("resume"),
            close: lifecycle("close"),
            image: advertised("image"),
            audio: advertised("audio"),
            embedded_context: advertised("embeddedContext"),
        };
        Ok(Self {
            scope: Arc::new(()),
            negotiated: Negotiated {
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
    pub(super) fn owns_request(&self, request: &Request) -> bool {
        request
            .scope
            .as_ref()
            .is_some_and(|scope| Arc::ptr_eq(scope, &self.scope))
    }
    /// # Errors
    /// Rejects absent session support; workspace validation belongs to the application owner.
    pub fn new_session(&self, cwd: &str) -> Result<Request, AcpError> {
        self.require_sessions()?;
        Ok(Request {
            scope: Some(Arc::clone(&self.scope)),
            method: "session/new",
            params: json!({"cwd":cwd,"mcpServers":[]}),
            kind: RequestKind::NewSession,
        })
    }
    /// No replay cursor is sent. The v2 owner must observe current state before the response.
    /// # Errors
    /// Rejects unadvertised resume support; it never substitutes session/new or session/load.
    pub fn resume(&self, session: SessionId, cwd: &str) -> Result<Request, AcpError> {
        if !self.negotiated.capabilities.resume {
            return Err(AcpError::UnsupportedCapability);
        }
        Ok(Request {
            scope: Some(Arc::clone(&self.scope)),
            method: "session/resume",
            params: json!({"sessionId":session.as_str(),"cwd":cwd,"mcpServers":[]}),
            kind: RequestKind::Resume(session),
        })
    }
    /// # Errors
    /// Rejects unadvertised close support.
    pub fn close(&self, session: SessionId) -> Result<Request, AcpError> {
        if !self.negotiated.capabilities.close {
            return Err(AcpError::UnsupportedCapability);
        }
        Ok(Request {
            scope: Some(Arc::clone(&self.scope)),
            method: "session/close",
            params: json!({"sessionId":session.as_str()}),
            kind: RequestKind::Close(session),
        })
    }
    /// # Errors
    /// Rejects empty, malformed and unsupported content instead of converting it to text.
    pub fn prompt(&self, session: SessionId, prompt: Value) -> Result<Request, AcpError> {
        self.require_sessions()?;
        let blocks = prompt
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or(AcpError::InvalidMessage)?;
        for block in blocks {
            let kind = field(block, "type")?
                .as_str()
                .ok_or(AcpError::InvalidMessage)?;
            match kind {
                "text" => {
                    field(block, "text")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?;
                }
                "resource_link" => {
                    field(block, "uri")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?;
                    field(block, "name")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?;
                }
                "image" | "audio" => {
                    let supported = if kind == "image" {
                        self.negotiated.capabilities.image
                    } else {
                        self.negotiated.capabilities.audio
                    };
                    if !supported {
                        return Err(AcpError::UnsupportedCapability);
                    }
                    field(block, "data")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?;
                    field(block, "mimeType")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?;
                }
                "resource" => {
                    if !self.negotiated.capabilities.embedded_context {
                        return Err(AcpError::UnsupportedCapability);
                    }
                    object(field(block, "resource")?)?;
                }
                _ => return Err(AcpError::UnsupportedCapability),
            }
        }
        Ok(Request {
            scope: Some(Arc::clone(&self.scope)),
            method: "session/prompt",
            params: {
                let mut fields = serde_json::Map::new();
                fields.insert("sessionId".into(), Value::String(session.as_str().into()));
                fields.insert("prompt".into(), prompt);
                Value::Object(fields)
            },
            kind: RequestKind::Prompt(session),
        })
    }
    /// # Errors
    /// Rejects an invalid version-specific response; insertion is never completion.
    pub fn prompt_result(&self, result: &Value) -> Result<PromptResult, AcpError> {
        object(result)?;
        match self.version() {
            Version::V1 => {
                let stop_reason = field(result, "stopReason")?
                    .as_str()
                    .ok_or(AcpError::InvalidMessage)?;
                if !matches!(
                    stop_reason,
                    "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled"
                ) {
                    return Err(AcpError::InvalidMessage);
                }
                Ok(PromptResult::Completed {
                    stop_reason: Arc::from(stop_reason),
                })
            }
            Version::V2 => Ok(PromptResult::Inserted {
                message: MessageId::parse(field(result, "messageId")?)?,
            }),
        }
    }
    /// Unknown update kinds retain the complete raw message in the transport event.
    /// # Errors
    /// Rejects malformed required session/update identifiers.
    pub fn update(&self, params: &Value) -> Result<SessionEvent, AcpError> {
        let session = SessionId::parse(field(params, "sessionId")?)?;
        let update = field(params, "update")?;
        let kind = field(update, "sessionUpdate")?
            .as_str()
            .ok_or(AcpError::InvalidMessage)?;
        match kind {
            "state_update" if self.version() == Version::V2 => Ok(SessionEvent::State {
                session,
                state: Arc::from(
                    field(update, "state")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?,
                ),
                stop_reason: update
                    .get("stopReason")
                    .and_then(Value::as_str)
                    .map(Arc::from),
            }),
            "tool_call_update" | "tool_call" => Ok(SessionEvent::Tool {
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
                message: update.get("messageId").map(MessageId::parse).transpose()?,
                kind: Arc::from(kind),
            }),
            _ => Ok(SessionEvent::Other {
                session,
                kind: Arc::from(kind),
            }),
        }
    }
    /// Only parses transport facts; source/Run and structured tool association are owner duties.
    /// # Errors
    /// Rejects malformed required fields; unknown subjects/options remain unsupported raw facts.
    pub fn permission(
        &self,
        id: RpcId,
        raw: RawMessage,
        params: &Value,
    ) -> Result<PermissionRequest, AcpError> {
        let session = SessionId::parse(field(params, "sessionId")?)?;
        let (supported_subject, tool) = match self.version() {
            Version::V1 => (
                true,
                Some(ToolCallId::parse(field(
                    field(params, "toolCall")?,
                    "toolCallId",
                )?)?),
            ),
            Version::V2 => {
                field(params, "title")?
                    .as_str()
                    .ok_or(AcpError::InvalidMessage)?;
                match params.get("subject") {
                    Some(s) if s.get("type").and_then(Value::as_str) == Some("tool_call") => (
                        true,
                        Some(ToolCallId::parse(field(
                            field(s, "toolCall")?,
                            "toolCallId",
                        )?)?),
                    ),
                    Some(subject)
                        if subject.get("type").and_then(Value::as_str) == Some("command") =>
                    {
                        field(subject, "command")?
                            .as_str()
                            .ok_or(AcpError::InvalidMessage)?;
                        field(subject, "cwd")?
                            .as_str()
                            .ok_or(AcpError::InvalidMessage)?;
                        let tool = subject
                            .get("toolCallId")
                            .filter(|v| !v.is_null())
                            .map(ToolCallId::parse)
                            .transpose()?;
                        (tool.is_some(), tool)
                    }
                    _ => (false, None),
                }
            }
        };
        let options = field(params, "options")?
            .as_array()
            .ok_or(AcpError::InvalidMessage)?;
        if self.version() == Version::V2 && options.is_empty() {
            return Err(AcpError::InvalidMessage);
        }
        let mut parsed = Vec::with_capacity(options.len());
        for option in options {
            field(option, "name")?
                .as_str()
                .ok_or(AcpError::InvalidMessage)?;
            parsed.push(PermissionChoice {
                id: OptionId::parse(field(option, "optionId")?)?,
                kind: Arc::from(
                    field(option, "kind")?
                        .as_str()
                        .ok_or(AcpError::InvalidMessage)?,
                ),
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
    /// Only an advertised supported option can be encoded; source authority must be checked separately.
    pub fn permission_response(
        &self,
        request: &PermissionRequest,
        selection: Option<&OptionId>,
    ) -> Result<Value, AcpError> {
        let outcome = if let Some(id) = selection {
            if !request.supported_subject
                || request.options.iter().any(|option| !option.supported())
                || request.options.iter().enumerate().any(|(index, option)| {
                    request.options[..index]
                        .iter()
                        .any(|prior| prior.id == option.id)
                })
                || request.options.iter().filter(|o| &o.id == id).count() != 1
                || !request.options.iter().any(|o| &o.id == id && o.supported())
            {
                return Err(AcpError::InvalidOption);
            }
            json!({"outcome":"selected","optionId":id.as_str()})
        } else {
            json!({"outcome":"cancelled"})
        };
        Ok(json!({"jsonrpc":"2.0","id":request.id.json(),"result":{"outcome":outcome}}))
    }
    /// Interprets only the name patch whose version distinction is normative in both schemas.
    /// # Errors
    /// Rejects malformed name values; unknown/raw fields remain in the original message.
    pub fn tool_name_patch<'a>(&self, update: &'a Value) -> Result<ToolNamePatch<'a>, AcpError> {
        match update.get("name") {
            None => Ok(ToolNamePatch::Unchanged),
            Some(Value::Null) => Ok(if self.version() == Version::V1 {
                ToolNamePatch::Unchanged
            } else {
                ToolNamePatch::Clear
            }),
            Some(Value::String(name)) => Ok(ToolNamePatch::Set(name)),
            _ => Err(AcpError::InvalidMessage),
        }
    }
    /// Applies v2 tool metadata patches by top-level key; nested values replace, never deep merge.
    /// # Errors
    /// v1 metadata stays uninterpreted; non-object/non-null v2 patches are invalid.
    pub fn tool_metadata_patch(&self, current: &Value, update: &Value) -> Result<Value, AcpError> {
        if self.version() != Version::V2 {
            return Err(AcpError::UnsupportedCapability);
        }
        match update.get("_meta") {
            None => Ok(current.clone()),
            Some(Value::Null) => Ok(Value::Null),
            Some(Value::Object(patch)) => {
                let mut merged = current.as_object().cloned().unwrap_or_default();
                for (key, value) in patch {
                    if value.is_null() {
                        merged.remove(key);
                    } else {
                        merged.insert(key.clone(), value.clone());
                    }
                }
                Ok(Value::Object(merged))
            }
            _ => Err(AcpError::InvalidMessage),
        }
    }
    const fn require_sessions(&self) -> Result<(), AcpError> {
        if self.negotiated.capabilities.sessions {
            Ok(())
        } else {
            Err(AcpError::UnsupportedCapability)
        }
    }
}
pub(super) fn initialize(offer: Version) -> Request {
    let info = json!({"name":"acp-remote","version":crate::VERSION});
    let params = match offer {
        Version::V1 => json!({"protocolVersion":1,"clientCapabilities":{},"clientInfo":info}),
        Version::V2 => json!({"protocolVersion":2,"capabilities":{},"info":info}),
    };
    Request {
        scope: None,
        method: "initialize",
        params,
        kind: RequestKind::Initialize(offer),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolNamePatch<'a> {
    Unchanged,
    Clear,
    Set(&'a str),
}
