#![allow(
    clippy::needless_pass_by_value,
    reason = "Frozen public API consumes owned snapshots/fragments rather than borrowing caller state"
)]
//! Explicit complete, version-bound protocol configuration; this module grants no authority.
use super::{
    AcpError, Version,
    raw_json::{OwnedRawValue, RawValueRef},
    raw_model as raw,
};
use std::{fmt, sync::Arc};
#[derive(Clone)]
pub enum Known<T> {
    Unknown,
    Known(T),
}
#[derive(Clone)]
pub struct ConfigSourceStamp {
    pub reference: Arc<str>,
    pub revision: u64,
}
#[derive(Clone, Debug)]
pub struct SetupSnapshot {
    pub wire: Version,
    pub source: ConfigSourceStamp,
    pub cwd: Known<Arc<str>>,
    pub mcp_servers: Known<Vec<McpServer>>,
    pub additional_directories: Known<Vec<Arc<str>>>,
    pub extra: ExtensionFields,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupField {
    Cwd,
    Mcp,
    AdditionalDirectories,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SetupError {
    #[error("ACP configuration incomplete")]
    Incomplete(SetupField),
    #[error("ACP configuration version mismatch")]
    VersionMismatch,
    #[error("ACP configuration invalid shape")]
    InvalidShape,
    #[error("ACP reserved configuration field")]
    ReservedField,
    #[error("ACP unsupported configuration encoding")]
    UnsupportedEncoding,
    #[error("ACP configuration allocation failed")]
    Allocation,
}
impl From<AcpError> for SetupError {
    fn from(e: AcpError) -> Self {
        if e == AcpError::Allocation {
            Self::Allocation
        } else {
            Self::InvalidShape
        }
    }
}
impl From<SetupError> for AcpError {
    fn from(e: SetupError) -> Self {
        match e {
            SetupError::Incomplete(_) => Self::ConfigurationIncomplete,
            SetupError::VersionMismatch => Self::ConfigurationVersionMismatch,
            SetupError::Allocation => Self::Allocation,
            _ => Self::UnencodableConfiguration,
        }
    }
}
#[derive(Clone)]
pub struct SessionSetup {
    wire: Version,
    source: ConfigSourceStamp,
    cwd: Arc<str>,
    mcp: Vec<McpServer>,
    dirs: Vec<Arc<str>>,
    extra: ExtensionFields,
}
impl SessionSetup {
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn try_from_snapshot(v: SetupSnapshot) -> Result<Self, SetupError> {
        let Known::Known(cwd) = v.cwd else {
            return Err(SetupError::Incomplete(SetupField::Cwd));
        };
        let Known::Known(mcp) = v.mcp_servers else {
            return Err(SetupError::Incomplete(SetupField::Mcp));
        };
        let Known::Known(dirs) = v.additional_directories else {
            return Err(SetupError::Incomplete(SetupField::AdditionalDirectories));
        };
        if mcp.iter().any(|m| m.wire != v.wire) {
            return Err(SetupError::VersionMismatch);
        }
        v.extra.reject(&[
            "sessionId",
            "cwd",
            "mcpServers",
            "additionalDirectories",
            "replayFrom",
        ])?;
        Ok(Self {
            wire: v.wire,
            source: v.source,
            cwd,
            mcp,
            dirs,
            extra: v.extra,
        })
    }
    #[must_use]
    pub const fn wire(&self) -> Version {
        self.wire
    }
    #[must_use]
    pub const fn source(&self) -> &ConfigSourceStamp {
        &self.source
    }
    #[must_use]
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
    #[must_use]
    pub fn mcp_servers(&self) -> &[McpServer] {
        &self.mcp
    }
    #[must_use]
    pub fn additional_directories(&self) -> &[Arc<str>] {
        &self.dirs
    }
    #[must_use]
    pub const fn extra(&self) -> &ExtensionFields {
        &self.extra
    }
}
#[derive(Clone)]
pub struct ExtensionFields(OwnedRawValue);
impl ExtensionFields {
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn empty() -> Result<Self, SetupError> {
        let v = super::raw_json::RawDocument::parse(b"{}".to_vec()).map_err(AcpError::from)?;
        Ok(Self(v.root_owned()))
    }
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn from_raw(v: OwnedRawValue) -> Result<Self, SetupError> {
        if v.as_ref().object().is_none() {
            return Err(SetupError::InvalidShape);
        }
        if v.as_ref()
            .object()
            .ok_or(SetupError::InvalidShape)?
            .get("_meta")
            .map_err(AcpError::from)?
            .is_some_and(|m| !m.is_null() && m.object().is_none())
        {
            return Err(SetupError::InvalidShape);
        }
        Ok(Self(v))
    }
    #[must_use]
    pub const fn as_raw(&self) -> RawValueRef<'_> {
        self.0.as_ref()
    }
    pub(super) fn reject(&self, keys: &[&str]) -> Result<(), SetupError> {
        let obj = self.as_raw().object().ok_or(SetupError::InvalidShape)?;
        for key in keys {
            if obj.get(key).map_err(AcpError::from)?.is_some() {
                return Err(SetupError::ReservedField);
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
    Other(Arc<str>),
}
#[derive(Clone)]
pub struct NamedValue {
    name: Arc<str>,
    value: Arc<str>,
    extra: ExtensionFields,
}
impl NamedValue {
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn new(
        name: Arc<str>,
        value: Arc<str>,
        extra: ExtensionFields,
    ) -> Result<Self, SetupError> {
        extra.reject(&["name", "value"])?;
        Ok(Self { name, value, extra })
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
    #[must_use]
    pub const fn extra(&self) -> &ExtensionFields {
        &self.extra
    }
    fn encode(&self) -> Result<OwnedRawValue, SetupError> {
        Ok(raw::object(
            [
                ("name", &raw::string(&self.name)?),
                ("value", &raw::string(&self.value)?),
            ],
            Some(self.extra.as_raw()),
        )?)
    }
}
#[derive(Clone)]
pub struct McpServer {
    wire: Version,
    transport: McpTransport,
    raw: OwnedRawValue,
}
impl McpServer {
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn from_raw(wire: Version, v: OwnedRawValue) -> Result<Self, SetupError> {
        let obj = v.as_ref().object().ok_or(SetupError::InvalidShape)?;
        let kind = obj
            .get("type")
            .map_err(AcpError::from)?
            .map(super::raw_json::RawValueRef::string)
            .transpose()
            .map_err(|_| SetupError::InvalidShape)?;
        let transport = match (wire, kind.as_deref()) {
            (Version::V1, None | Some("stdio")) | (Version::V2, Some("stdio")) => {
                McpTransport::Stdio
            }
            (_, Some("http")) => McpTransport::Http,
            (Version::V1, Some("sse")) => McpTransport::Sse,
            (Version::V2, Some(t)) => McpTransport::Other(t.into()),
            _ => return Err(SetupError::UnsupportedEncoding),
        };
        let required = match transport {
            McpTransport::Stdio => &["name", "command"][..],
            McpTransport::Http | McpTransport::Sse => &["name", "url"][..],
            McpTransport::Other(_) => &[][..],
        };
        for key in required {
            obj.get(key)
                .map_err(AcpError::from)?
                .ok_or(SetupError::InvalidShape)?
                .string()
                .map_err(|_| SetupError::InvalidShape)?;
        }
        if matches!(transport, McpTransport::Stdio)
            && (wire == Version::V1 || obj.get("args").map_err(AcpError::from)?.is_some())
        {
            let args = obj
                .get("args")
                .map_err(AcpError::from)?
                .and_then(super::raw_json::RawValueRef::array)
                .ok_or(SetupError::InvalidShape)?;
            for v in args.iter() {
                v.string().map_err(|_| SetupError::InvalidShape)?;
            }
        }
        let pairs = if matches!(transport, McpTransport::Stdio) {
            "env"
        } else {
            "headers"
        };
        if !matches!(transport, McpTransport::Other(_))
            && (wire == Version::V1 || obj.get(pairs).map_err(AcpError::from)?.is_some())
        {
            let a = obj
                .get(pairs)
                .map_err(AcpError::from)?
                .and_then(super::raw_json::RawValueRef::array)
                .ok_or(SetupError::InvalidShape)?;
            for v in a.iter() {
                for key in ["name", "value"] {
                    v.object()
                        .ok_or(SetupError::InvalidShape)?
                        .get(key)
                        .map_err(AcpError::from)?
                        .ok_or(SetupError::InvalidShape)?
                        .string()
                        .map_err(|_| SetupError::InvalidShape)?;
                }
            }
        }
        Ok(Self {
            wire,
            transport,
            raw: v,
        })
    }
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn stdio(
        wire: Version,
        name: Arc<str>,
        command: Arc<str>,
        args: Vec<Arc<str>>,
        env: Vec<NamedValue>,
        extra: ExtensionFields,
    ) -> Result<Self, SetupError> {
        extra.reject(&["type", "name", "command", "args", "env"])?;
        let args = args
            .iter()
            .map(|v| raw::string(v))
            .collect::<Result<Vec<_>, _>>()?;
        let env = env
            .iter()
            .map(NamedValue::encode)
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields = vec![
            ("name", raw::string(&name)?),
            ("command", raw::string(&command)?),
            ("args", raw::array(&args)?),
            ("env", raw::array(&env)?),
        ];
        if wire == Version::V2 {
            fields.insert(0, ("type", raw::string("stdio")?));
        }
        Self::from_raw(
            wire,
            raw::object(fields.iter().map(|(k, v)| (*k, v)), Some(extra.as_raw()))?,
        )
    }
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn http(
        wire: Version,
        name: Arc<str>,
        url: Arc<str>,
        headers: Vec<NamedValue>,
        extra: ExtensionFields,
    ) -> Result<Self, SetupError> {
        Self::remote(wire, "http", name, url, headers, extra)
    }
    /// # Errors
    /// Rejects incomplete, incompatible, malformed or reserved configuration, or allocation failure.
    pub fn sse_v1(
        name: Arc<str>,
        url: Arc<str>,
        headers: Vec<NamedValue>,
        extra: ExtensionFields,
    ) -> Result<Self, SetupError> {
        Self::remote(Version::V1, "sse", name, url, headers, extra)
    }
    fn remote(
        wire: Version,
        kind: &str,
        name: Arc<str>,
        url: Arc<str>,
        headers: Vec<NamedValue>,
        extra: ExtensionFields,
    ) -> Result<Self, SetupError> {
        extra.reject(&["type", "name", "url", "headers"])?;
        let headers = headers
            .iter()
            .map(NamedValue::encode)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_raw(
            wire,
            raw::object(
                [
                    ("type", &raw::string(kind)?),
                    ("name", &raw::string(&name)?),
                    ("url", &raw::string(&url)?),
                    ("headers", &raw::array(&headers)?),
                ],
                Some(extra.as_raw()),
            )?,
        )
    }
    #[must_use]
    pub const fn wire(&self) -> Version {
        self.wire
    }
    #[must_use]
    pub const fn transport(&self) -> &McpTransport {
        &self.transport
    }
    #[must_use]
    pub const fn as_raw(&self) -> RawValueRef<'_> {
        self.raw.as_ref()
    }
}
macro_rules! redacted{($($t:ty),*)=>{$(impl fmt::Debug for $t{fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{f.write_str(concat!(stringify!($t),"(..)"))}})*}}
redacted!(
    ConfigSourceStamp,
    SessionSetup,
    ExtensionFields,
    NamedValue,
    McpTransport,
    McpServer
);
impl<T> fmt::Debug for Known<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if matches!(self, Self::Unknown) {
            "Unknown"
        } else {
            "Known(..)"
        })
    }
}
