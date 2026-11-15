use super::ConnectionScope;
use super::raw_json::{OwnedRawValue, RawDocument, RawJsonError, RawKind, RawObject, RawValueRef};
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    V1,
    V2,
}
impl Version {
    #[must_use]
    pub const fn preferred() -> Self {
        Self::V2
    }
    #[must_use]
    pub const fn number(self) -> u64 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }
}
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum RpcId {
    Integer(i64),
    String(Arc<str>),
}
impl RpcId {
    /// # Errors
    /// Rejects invalid protocol shapes or identity controls, or a raw allocation failure.
    pub fn parse(value: RawValueRef<'_>) -> Result<Self, AcpError> {
        if value.kind() == RawKind::String {
            let s = value.string()?;
            return if s.len() <= 1024 {
                Ok(Self::String(s.into()))
            } else {
                Err(AcpError::InvalidMessage)
            };
        }
        if value.fragment() == "-0" {
            return Err(AcpError::InvalidMessage);
        }
        value
            .integer()
            .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991)
            .map(Self::Integer)
            .ok_or(AcpError::InvalidMessage)
    }
    /// # Errors
    /// Reports scalar JSON encoding/allocation failure; this method performs no RPC routing or authorization.
    pub fn write_json(&self, out: &mut Vec<u8>) -> Result<(), AcpError> {
        match self {
            Self::Integer(n) => out.extend_from_slice(n.to_string().as_bytes()),
            Self::String(s) => out.extend_from_slice(
                &serde_json::to_vec(s.as_ref()).map_err(|_| AcpError::Allocation)?,
            ),
        }
        Ok(())
    }
}
impl fmt::Debug for RpcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RpcId(..)")
    }
}
macro_rules! native_id {
    ($name:ident,$limit:expr) => {
        #[derive(Clone, Eq, PartialEq, Hash)]
        pub struct $name(Arc<str>);
        impl $name {
            /// # Errors
            /// Rejects invalid protocol shapes or identity controls, or a raw allocation failure.
            pub fn parse(value: RawValueRef<'_>) -> Result<Self, AcpError> {
                let s = value.string()?;
                if $limit.is_some_and(|limit| s.len() > limit) {
                    return Err(AcpError::InvalidMessage);
                }
                Ok(Self(s.into()))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    };
}
native_id!(SessionId, Some(4096usize));
impl SessionId {
    pub(super) fn shared_text(&self) -> Arc<str> {
        Arc::clone(&self.0)
    }
}
native_id!(OptionId, Some(4096usize));
native_id!(ToolCallId, None::<usize>);
native_id!(MessageId, None::<usize>);
#[derive(Clone)]
pub struct RawMessage {
    document: RawDocument,
}
impl RawMessage {
    /// # Errors
    /// Rejects invalid syntax, Unicode, shape, ambiguous envelope fields or allocation failure.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, AcpError> {
        let document = RawDocument::parse(bytes)?;
        let fields = object(document.root())?;
        for key in ["jsonrpc", "id", "method", "params", "result", "error"] {
            fields.get(key)?;
        }
        Ok(Self { document })
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.document.bytes()
    }
    /// # Errors
    /// Returns `InvalidMessage` if the validated document root cannot be borrowed as an object.
    pub fn fields(&self) -> Result<RawObject<'_>, AcpError> {
        object(self.document.root())
    }
    /// # Errors
    /// Rejects ambiguous decoded names, preserving missing fields as None.
    pub fn field(&self, key: &str) -> Result<Option<RawValueRef<'_>>, AcpError> {
        Ok(object(self.document.root())?.get(key)?)
    }
}
impl fmt::Debug for RawMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawMessage")
            .field("bytes", &self.bytes().len())
            .finish()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AcpError {
    #[error("ACP foreign request")]
    InvalidRequestScope,
    #[error("ACP not initialized")]
    NotInitialized,
    #[error("ACP initialization already attempted")]
    AlreadyInitialized,
    #[error("ACP invalid message")]
    InvalidMessage,
    #[error("ACP incompatible version")]
    IncompatibleVersion,
    #[error("ACP unsupported capability")]
    UnsupportedCapability,
    #[error("ACP control capacity exhausted")]
    Capacity,
    #[error("ACP unmatched response")]
    UnmatchedResponse,
    #[error("ACP delivery unknown")]
    DeliveryUnknown,
    #[error("ACP connection closed")]
    Closed,
    #[error("ACP deadline elapsed")]
    Deadline,
    #[error("ACP I/O failed")]
    Io,
    #[error("ACP invalid advertised option")]
    InvalidOption,
    #[error("ACP missing snapshot")]
    MissingSnapshot,
    #[error("ACP session read busy")]
    SessionReadBusy,
    #[error("ACP runtime unavailable")]
    RuntimeUnavailable,
    #[error("ACP allocation failed")]
    Allocation,
    #[error("ACP cancelled")]
    Cancelled,
    #[error("ACP invalid write fence")]
    InvalidFence,
    #[error("ACP mismatched encoded frame")]
    InvalidFrame,
    #[error("ACP configuration incomplete")]
    ConfigurationIncomplete,
    #[error("ACP configuration version mismatch")]
    ConfigurationVersionMismatch,
    #[error("ACP configuration cannot be encoded")]
    UnencodableConfiguration,
}
impl From<RawJsonError> for AcpError {
    fn from(e: RawJsonError) -> Self {
        if e == RawJsonError::Allocation {
            Self::Allocation
        } else {
            Self::InvalidMessage
        }
    }
}
pub(super) fn field<'a>(v: RawValueRef<'a>, key: &str) -> Result<RawValueRef<'a>, AcpError> {
    object(v)?.get(key)?.ok_or(AcpError::InvalidMessage)
}
pub(super) fn object(v: RawValueRef<'_>) -> Result<RawObject<'_>, AcpError> {
    v.object().ok_or(AcpError::InvalidMessage)
}
pub(super) fn text(v: RawValueRef<'_>) -> Result<Arc<str>, AcpError> {
    Ok(v.string()?.into())
}
#[derive(Clone, Debug)]
pub struct Negotiated {
    pub offered: Version,
    pub version: Version,
    pub raw: RawMessage,
    pub capabilities: Capabilities,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent advertised protocol capabilities"
)]
pub struct Capabilities {
    pub sessions: bool,
    pub list: bool,
    pub standard_replay: bool,
    pub resume: bool,
    pub close: bool,
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
}
#[derive(Clone)]
pub enum ReportedDirectories {
    Omitted,
    Empty,
    Known(Vec<Arc<str>>),
    Null,
    Uninterpreted(OwnedRawValue),
}
impl ReportedDirectories {
    #[must_use]
    pub fn known(&self) -> Option<&[Arc<str>]> {
        match self {
            Self::Empty => Some(&[]),
            Self::Known(v) => Some(v),
            _ => None,
        }
    }
}
#[derive(Clone)]
pub struct SessionListItem {
    pub(super) session: SessionId,
    pub(super) cwd: Arc<str>,
    pub(super) directories: ReportedDirectories,
    pub(super) raw: OwnedRawValue,
}
impl SessionListItem {
    #[must_use]
    pub const fn session(&self) -> &SessionId {
        &self.session
    }
    #[must_use]
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
    #[must_use]
    pub const fn additional_directories(&self) -> &ReportedDirectories {
        &self.directories
    }
    #[must_use]
    pub const fn raw(&self) -> RawValueRef<'_> {
        self.raw.as_ref()
    }
}
#[derive(Clone)]
pub struct SessionListing {
    pub(super) sessions: Vec<SessionListItem>,
    pub(super) next_cursor: Option<Arc<str>>,
    pub(super) raw: OwnedRawValue,
}
impl SessionListing {
    #[must_use]
    pub fn sessions(&self) -> &[SessionListItem] {
        &self.sessions
    }
    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
    #[must_use]
    pub const fn raw(&self) -> RawValueRef<'_> {
        self.raw.as_ref()
    }
}
#[derive(Clone)]
pub enum PromptResult {
    Completed { stop_reason: Arc<str> },
    Inserted { message: MessageId },
}
#[derive(Clone)]
pub enum SessionEvent {
    State {
        session: SessionId,
        state: Arc<str>,
        stop_reason: Option<Arc<str>>,
    },
    Tool {
        session: SessionId,
        tool: ToolCallId,
    },
    Message {
        session: SessionId,
        message: Option<MessageId>,
        kind: Arc<str>,
    },
    Other {
        session: SessionId,
        kind: Arc<str>,
    },
}
impl SessionEvent {
    pub(super) const fn session(&self) -> &SessionId {
        match self {
            Self::State { session, .. }
            | Self::Tool { session, .. }
            | Self::Message { session, .. }
            | Self::Other { session, .. } => session,
        }
    }
}
#[derive(Clone)]
pub struct PermissionRequest {
    pub(super) connection: Option<ConnectionScope>,
    pub(super) id: RpcId,
    pub(super) session: SessionId,
    pub(super) tool: Option<ToolCallId>,
    pub(super) supported_subject: bool,
    pub(super) options: Vec<PermissionChoice>,
    pub(super) raw: RawMessage,
}
impl PermissionRequest {
    #[must_use]
    pub const fn id(&self) -> &RpcId {
        &self.id
    }
    #[must_use]
    pub const fn session(&self) -> &SessionId {
        &self.session
    }
    #[must_use]
    pub const fn tool(&self) -> Option<&ToolCallId> {
        self.tool.as_ref()
    }
    #[must_use]
    pub const fn supported_subject(&self) -> bool {
        self.supported_subject
    }
    #[must_use]
    pub fn options(&self) -> &[PermissionChoice] {
        &self.options
    }
    #[must_use]
    pub const fn raw(&self) -> &RawMessage {
        &self.raw
    }
    #[must_use]
    pub fn scope(&self) -> Option<ConnectionScope> {
        self.connection.clone()
    }
}
#[derive(Clone)]
pub struct PermissionChoice {
    pub(super) id: OptionId,
    pub(super) kind: Arc<str>,
    pub(super) raw: OwnedRawValue,
}
impl PermissionChoice {
    #[must_use]
    pub const fn id(&self) -> &OptionId {
        &self.id
    }
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    #[must_use]
    pub const fn raw(&self) -> RawValueRef<'_> {
        self.raw.as_ref()
    }
    #[must_use]
    pub fn supported(&self) -> bool {
        matches!(
            &*self.kind,
            "allow_once" | "allow_always" | "reject_once" | "reject_always"
        )
    }
}
#[derive(Clone)]
pub struct UnsupportedRequest {
    pub(super) scope: ConnectionScope,
    pub(super) id: RpcId,
    pub(super) method: Arc<str>,
    pub(super) raw: RawMessage,
}
impl UnsupportedRequest {
    #[must_use]
    pub const fn id(&self) -> &RpcId {
        &self.id
    }
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }
    #[must_use]
    pub const fn raw(&self) -> &RawMessage {
        &self.raw
    }
    #[must_use]
    pub fn scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
}
macro_rules! redacted {($($t:ty),*)=>{$(impl fmt::Debug for $t {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(concat!(stringify!($t),"(..)"))}})*}}
redacted!(
    ReportedDirectories,
    SessionListItem,
    SessionListing,
    PromptResult,
    SessionEvent,
    PermissionRequest,
    PermissionChoice,
    UnsupportedRequest
);
