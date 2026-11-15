//! Complete prompt data without connection, admission or authorization authority.
use super::{
    AcpError,
    model::{field, object, text},
    native_dispatch::AcpNativeAdapter,
    raw_json::{OwnedRawValue, RawDocument, RawKind, RawValueRef},
};
use crate::application::{
    queue_owner::RetainPrompt,
    request_registry::{RegistryError, ValidatedBody, ValidatedBodyPort},
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
#[cfg(all(test, windows))]
pub mod test_hooks;

/// Immutable, structurally validated prompt data retaining the complete supplied bytes.
///
/// This value proves neither the upstream origin of those bytes nor authorization or
/// support by a negotiated connection. Its protocol fragment shares the original storage.
#[derive(Clone)]
pub struct PromptDocument {
    raw: RawDocument,
}

impl PromptDocument {
    /// Takes one complete UTF-8 JSON prompt array without re-encoding its contents.
    ///
    /// Unknown block types and fields are preserved. Actual connection capabilities and
    /// version restrictions remain the responsibility of `Strategy::prompt`.
    ///
    /// # Errors
    /// Returns `InvalidMessage` for malformed JSON, ambiguous interpreted fields or invalid
    /// required block fields, and preserves reported raw allocation failures.
    pub fn parse(uploaded: Vec<u8>) -> Result<Self, AcpError> {
        let raw = RawDocument::parse(uploaded)?;
        #[cfg(all(test, windows))]
        test_hooks::point(test_hooks::Point::RawParsedBeforeBlockChecks);
        raw.require_unique_fields().map_err(AcpError::from)?;
        for block in raw.root().array().ok_or(AcpError::InvalidMessage)?.iter() {
            let tag = block_tag(block)?;
            validate_block_fields(block, &tag)?;
        }
        Ok(Self { raw })
    }

    /// Returns all supplied bytes, including whitespace outside the root value.
    #[must_use]
    pub fn uploaded_bytes(&self) -> &[u8] {
        self.raw.bytes()
    }

    /// Returns the original root array fragment used by protocol prompt encoding.
    #[must_use]
    pub fn prompt_fragment(&self) -> &str {
        self.raw.root().fragment()
    }
}

pub(super) fn block_tag(block: RawValueRef<'_>) -> Result<Arc<str>, AcpError> {
    text(field(block, "type")?)
}

// The caller decides whether connection capabilities permit this block before entering
// its field checks. Keeping this separate preserves Strategy's first-error ordering.
pub(super) fn validate_block_fields(block: RawValueRef<'_>, tag: &str) -> Result<(), AcpError> {
    match tag {
        "text" => {
            text(field(block, "text")?)?;
        }
        "resource_link" => {
            text(field(block, "uri")?)?;
            text(field(block, "name")?)?;
        }
        "image" | "audio" => {
            text(field(block, "data")?)?;
            text(field(block, "mimeType")?)?;
        }
        "resource" => {
            let resource = object(field(block, "resource")?)?;
            let uri = resource
                .get("uri")?
                .is_some_and(|v| v.kind() == RawKind::String);
            let text = resource.get("text")?;
            let blob = resource.get("blob")?;
            let contents = text.is_some_and(|v| v.kind() == RawKind::String)
                || blob.is_some_and(|v| v.kind() == RawKind::String);
            if !uri || !contents {
                return Err(AcpError::InvalidMessage);
            }
        }
        _ => {}
    }
    Ok(())
}

impl ValidatedBodyPort for Arc<AcpNativeAdapter> {
    type Raw = PromptDocument;
    type Body = PromptDocument;

    fn validate(&self, raw: &PromptDocument) -> Result<(PromptDocument, [u8; 32]), RegistryError> {
        let digest = Sha256::digest(raw.uploaded_bytes()).into();
        #[cfg(all(test, windows))]
        test_hooks::point(test_hooks::Point::DigestComputed);
        Ok((raw.clone(), digest))
    }
}

#[cfg(windows)]
impl crate::application::shared_views::RetainedRaw for PromptDocument {
    fn len(&self) -> usize {
        self.uploaded_bytes().len()
    }
    fn bytes(&self) -> &[u8] {
        self.uploaded_bytes()
    }
}

impl RetainPrompt<PromptDocument> for AcpNativeAdapter {
    type Retained = OwnedRawValue;

    fn retain(&self, body: &ValidatedBody<PromptDocument>) -> OwnedRawValue {
        body.body().raw.root_owned()
    }
}

#[cfg(test)]
mod tests;
