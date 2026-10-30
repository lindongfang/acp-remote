use serde_json::{Map, Value};
use std::{fmt, sync::Arc};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    V1,
    V2,
}
impl Version {
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
    /// Rejects null, fractions and numbers outside interoperable safe integers.
    pub fn parse(value: &Value) -> Result<Self, AcpError> {
        match value {
            Value::String(s) if s.len() <= 1024 => Ok(Self::String(Arc::from(s.as_str()))),
            Value::Number(n) => n
                .as_i64()
                .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991)
                .map(Self::Integer)
                .ok_or(AcpError::InvalidMessage),
            _ => Err(AcpError::InvalidMessage),
        }
    }
    #[must_use]
    pub fn json(&self) -> Value {
        match self {
            Self::Integer(n) => Value::from(*n),
            Self::String(s) => Value::String(s.to_string()),
        }
    }
}
impl fmt::Debug for RpcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(_) => f.write_str("RpcId::Integer(..)"),
            Self::String(s) => f.debug_tuple("RpcId::StringBytes").field(&s.len()).finish(),
        }
    }
}
macro_rules! native_id {
    ($name:ident) => {
        #[derive(Clone, Eq, PartialEq, Hash)]
        pub struct $name(Arc<str>);
        impl $name {
            /// # Errors
            /// Rejects invalid native identifier types. Native IDs are opaque protocol strings.
            pub fn parse(value: &Value) -> Result<Self, AcpError> {
                value
                    .as_str()
                    .filter(|s| s.len() <= 4096)
                    .map(|s| Self(Arc::from(s)))
                    .ok_or(AcpError::InvalidMessage)
            }
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.0.len())
                    .finish()
            }
        }
    };
}
native_id!(SessionId);
native_id!(ToolCallId);
native_id!(MessageId);
native_id!(OptionId);
/// Keeps exact original bytes and parsed fields; Debug never emits message contents.
#[derive(Clone)]
pub struct RawMessage {
    bytes: Arc<[u8]>,
    fields: Arc<Map<String, Value>>,
}
impl RawMessage {
    pub(super) fn parse(bytes: Vec<u8>) -> Result<Self, AcpError> {
        // Count structural tokens before allocating parsed collections; strings/escapes are excluded.
        let mut string = false;
        let mut escape = false;
        let mut tokens = 0usize;
        for byte in &bytes {
            if string {
                if escape {
                    escape = false;
                } else if *byte == b'\\' {
                    escape = true;
                } else if *byte == b'"' {
                    string = false;
                }
            } else if *byte == b'"' {
                string = true;
                tokens += 1;
            } else if matches!(*byte, b'{' | b'[' | b',' | b':') {
                tokens += 1;
            }
            if tokens > 131_072 {
                return Err(AcpError::Capacity);
            }
        }
        let fields =
            crate::strict_json::parse_object(&bytes, 96).map_err(|_| AcpError::InvalidMessage)?;
        Ok(Self {
            bytes: bytes.into(),
            fields: Arc::new(fields),
        })
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    #[must_use]
    pub fn fields(&self) -> &Map<String, Value> {
        &self.fields
    }
}
impl fmt::Debug for RawMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawMessage")
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AcpError {
    #[error("ACP request does not belong to this negotiated connection")]
    InvalidRequestScope,
    #[error("ACP connection is not initialized")]
    NotInitialized,
    #[error("ACP initialization was already attempted")]
    AlreadyInitialized,
    #[error("ACP protocol message is invalid")]
    InvalidMessage,
    #[error("ACP selected protocol is incompatible")]
    IncompatibleVersion,
    #[error("ACP capability is not supported")]
    UnsupportedCapability,
    #[error("ACP request or buffer capacity is exhausted")]
    Capacity,
    #[error("ACP response has no matching live request")]
    UnmatchedResponse,
    #[error("ACP delivery or connection outcome is unknown")]
    DeliveryUnknown,
    #[error("ACP connection is closed")]
    Closed,
    #[error("ACP operation deadline elapsed")]
    Deadline,
    #[error("ACP I/O failed")]
    Io,
    #[error("ACP permission option is not advertised or supported")]
    InvalidOption,
    #[error("ACP current-state snapshot is missing")]
    MissingSnapshot,
}
pub(super) fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, AcpError> {
    value.get(key).ok_or(AcpError::InvalidMessage)
}
pub(super) fn object(value: &Value) -> Result<&Map<String, Value>, AcpError> {
    value.as_object().ok_or(AcpError::InvalidMessage)
}
#[derive(Clone, Debug)]
pub struct Negotiated {
    pub version: Version,
    pub raw: RawMessage,
    pub capabilities: Capabilities,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent advertised protocol features are not lifecycle states"
)]
pub struct Capabilities {
    pub sessions: bool,
    pub resume: bool,
    pub close: bool,
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
}
#[derive(Clone, Debug)]
pub enum PromptResult {
    Completed { stop_reason: Arc<str> },
    Inserted { message: MessageId },
}
#[derive(Clone, Debug)]
pub enum SessionEvent {
    /// Current state is a snapshot; caller classifies resume boundaries before producing Run facts.
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
#[derive(Clone)]
pub struct PermissionRequest {
    pub(super) connection: Option<Arc<()>>,
    pub id: RpcId,
    pub session: SessionId,
    pub tool: Option<ToolCallId>,
    pub supported_subject: bool,
    pub options: Vec<PermissionChoice>,
    pub raw: RawMessage,
}
#[derive(Clone)]
pub struct PermissionChoice {
    pub id: OptionId,
    pub kind: Arc<str>,
}
impl PermissionChoice {
    #[must_use]
    pub fn supported(&self) -> bool {
        matches!(
            self.kind.as_ref(),
            "allow_once" | "allow_always" | "reject_once" | "reject_always"
        )
    }
}
impl fmt::Debug for PermissionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PermissionRequest")
            .field("options", &self.options.len())
            .finish_non_exhaustive()
    }
}
