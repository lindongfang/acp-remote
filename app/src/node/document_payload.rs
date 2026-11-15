//! Payload variants share the original immutable bytes under the single Node Doc owner.
use crate::{adapters::acp::PromptDocument, application::shared_views::RetainedRaw};

#[derive(Clone)]
pub(super) enum NodeDocumentPayload {
    Queued(PromptDocument),
}

impl From<PromptDocument> for NodeDocumentPayload {
    fn from(document: PromptDocument) -> Self {
        Self::Queued(document)
    }
}

impl RetainedRaw for NodeDocumentPayload {
    fn len(&self) -> usize {
        self.bytes().len()
    }
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Queued(document) => document.uploaded_bytes(),
        }
    }
}
