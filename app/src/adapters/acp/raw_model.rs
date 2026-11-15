#![allow(
    clippy::needless_pass_by_value,
    reason = "Frozen public API consumes owned snapshots/fragments rather than borrowing caller state"
)]
//! Shallow protocol objects and encoding; unknown values remain shared raw fragments.
use super::{
    AcpError,
    raw_json::{OwnedRawValue, RawDocument, RawValueRef},
};
use std::{fmt, sync::Arc};

#[derive(Clone)]
pub struct OrderedRawObject {
    pub(super) members: Vec<(Arc<str>, OwnedRawValue)>,
}
impl OrderedRawObject {
    /// # Errors
    /// Rejects a mismatched shallow shape or a raw encoding/allocation failure.
    pub fn from_raw(value: OwnedRawValue) -> Result<Self, AcpError> {
        let object = value.as_ref().object().ok_or(AcpError::InvalidMessage)?;
        let mut members = Vec::new();
        members
            .try_reserve_exact(object.len())
            .map_err(|_| AcpError::Allocation)?;
        members.extend(object.iter().map(|(k, v)| (Arc::from(k), v.to_owned())));
        Ok(Self { members })
    }
    #[must_use]
    pub const fn len(&self) -> usize {
        self.members.len()
    }
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    /// # Errors
    /// Rejects multiple occurrences of the queried decoded name.
    pub fn get(&self, key: &str) -> Result<Option<RawValueRef<'_>>, AcpError> {
        let mut found = None;
        for (_, value) in self.members.iter().filter(|(name, _)| &**name == key) {
            if found.is_some() {
                return Err(AcpError::InvalidMessage);
            }
            found = Some(value.as_ref());
        }
        Ok(found)
    }
    #[must_use]
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, RawValueRef<'_>)> + DoubleEndedIterator + '_ {
        self.members.iter().map(|(k, v)| (&**k, v.as_ref()))
    }
    /// # Errors
    /// Rejects a mismatched shallow shape or a raw encoding/allocation failure.
    pub fn to_raw(&self) -> Result<OwnedRawValue, AcpError> {
        object(self.members.iter().map(|(k, v)| (&**k, v)), None)
    }
}
#[derive(Clone, Debug)]
pub enum RawMetadata {
    Null,
    Object(OrderedRawObject),
}
impl RawMetadata {
    /// # Errors
    /// Rejects a mismatched shallow shape or a raw encoding/allocation failure.
    pub fn from_raw(v: OwnedRawValue) -> Result<Self, AcpError> {
        if v.as_ref().is_null() {
            Ok(Self::Null)
        } else {
            Ok(Self::Object(OrderedRawObject::from_raw(v)?))
        }
    }
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    #[must_use]
    pub const fn as_object(&self) -> Option<&OrderedRawObject> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    /// # Errors
    /// Rejects a mismatched shallow shape or a raw encoding/allocation failure.
    pub fn to_raw(&self) -> Result<OwnedRawValue, AcpError> {
        match self {
            Self::Null => literal("null"),
            Self::Object(v) => v.to_raw(),
        }
    }
}
impl fmt::Debug for OrderedRawObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrderedRawObject")
            .field("members", &self.len())
            .finish()
    }
}
pub(super) fn literal(s: &str) -> Result<OwnedRawValue, AcpError> {
    Ok(RawDocument::parse(s.as_bytes().to_vec())?.root_owned())
}
pub(super) fn string(s: &str) -> Result<OwnedRawValue, AcpError> {
    Ok(RawDocument::parse(serde_json::to_vec(s).map_err(|_| AcpError::Allocation)?)?.root_owned())
}
pub(super) fn array<'a>(
    values: impl IntoIterator<Item = &'a OwnedRawValue>,
) -> Result<OwnedRawValue, AcpError> {
    let mut bytes = vec![b'['];
    let mut first = true;
    for v in values {
        if !first {
            bytes.push(b',');
        }
        first = false;
        append(&mut bytes, v.as_ref().compact()?.as_bytes())?;
    }
    bytes.push(b']');
    Ok(RawDocument::parse(bytes)?.root_owned())
}
pub(super) fn append(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), AcpError> {
    out.try_reserve(bytes.len())
        .map_err(|_| AcpError::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(())
}
pub(super) fn object<'a>(
    fields: impl IntoIterator<Item = (&'a str, &'a OwnedRawValue)>,
    extra: Option<RawValueRef<'_>>,
) -> Result<OwnedRawValue, AcpError> {
    let mut bytes = vec![b'{'];
    let mut first = true;
    for (key, value) in fields {
        if !first {
            bytes.push(b',');
        }
        first = false;
        append(
            &mut bytes,
            &serde_json::to_vec(key).map_err(|_| AcpError::Allocation)?,
        )?;
        bytes.push(b':');
        append(&mut bytes, value.as_ref().compact()?.as_bytes())?;
    }
    if let Some(extra) = extra {
        let compact = extra.compact()?;
        let inner = compact
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .ok_or(AcpError::InvalidMessage)?;
        if !inner.is_empty() {
            if !first {
                bytes.push(b',');
            }
            append(&mut bytes, inner.as_bytes())?;
        }
    }
    bytes.push(b'}');
    Ok(RawDocument::parse(bytes)?.root_owned())
}
