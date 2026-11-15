//! Fixed document metadata and synchronous owner revision fences. No Native authority is minted.
#![allow(
    clippy::ref_option,
    clippy::significant_drop_tightening,
    clippy::trivially_copy_pass_by_ref,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use crate::{
    claims::Id128,
    domain::{
        EntityId, input_queue::QueueError, request_registry::RegistryError, shared_view::ViewError,
    },
};
use std::sync::{Arc, Mutex, Weak};
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DocumentError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("document is closed")]
    Closed,
    #[error("document allocation failed")]
    Allocation,
    #[error("document disposal failed")]
    DisposalFailed,
    #[error("document task start is unconfirmed")]
    StartUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub enum DocumentStreamKind {
    #[serde(rename = "queued_input_body")]
    QueuedInputBody,
    #[serde(rename = "native_session_result")]
    NativeSessionResult,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStreamDescriptor {
    pub(crate) stream_kind: DocumentStreamKind,
    pub(crate) stream_id: Id128,
    pub(crate) document_id: Id128,
    #[serde(serialize_with = "serialize_target")]
    pub(crate) target_id: Option<EntityId>,
    #[serde(serialize_with = "serialize_revision")]
    pub(crate) revision: Option<u64>,
    #[serde(serialize_with = "serialize_bytes")]
    pub(crate) total_bytes: u64,
    pub(crate) encoding: &'static str,
}
pub trait DocumentStreamSeal: Send + Sync {
    fn seal_from_epoch(&self);
}
struct EpochState {
    sealed: bool,
    streams: Vec<Weak<dyn DocumentStreamSeal>>,
}
pub struct DocumentEpoch {
    inner: Mutex<EpochState>,
}
pub struct DocumentWake {
    streams: Vec<Weak<dyn DocumentStreamSeal>>,
}
impl DocumentWake {
    pub(crate) fn notify(self) {
        for stream in self.streams.into_iter().filter_map(|s| s.upgrade()) {
            stream.seal_from_epoch();
        }
    }
}
impl DocumentEpoch {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(EpochState {
                sealed: false,
                streams: Vec::new(),
            }),
        })
    }
    pub(crate) fn with_fence<T>(
        &self,
        work: impl FnOnce(bool) -> Result<T, ViewError>,
    ) -> Result<T, ViewError> {
        let state = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        work(!state.sealed)
    }
    pub(crate) fn register(&self, stream: Weak<dyn DocumentStreamSeal>) -> Result<(), ViewError> {
        let mut state = self.inner.lock().map_err(|_| ViewError::ReadFailure)?;
        if state.sealed {
            return Err(ViewError::StreamSealed);
        }
        state.streams.retain(|s| s.strong_count() > 0);
        state
            .streams
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        state.streams.push(stream);
        Ok(())
    }
    pub(crate) fn invalidate(&self) -> DocumentWake {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.sealed = true;
        DocumentWake {
            streams: std::mem::take(&mut state.streams),
        }
    }
    pub(crate) fn seal_and_notify(&self) {
        self.invalidate().notify();
    }
}
mod sealed {
    pub trait Sealed {}
    impl<
        B: crate::application::shared_views::RetainedRaw,
        P: crate::application::shared_views::RetainedRaw,
    > Sealed for crate::application::queue_owner::QueuedBodySnapshot<B, P>
    {
    }
    impl<R: crate::application::shared_views::RetainedRaw> Sealed
        for crate::application::native_dispatch::source_sessions::OriginalSourceResultSnapshot<R>
    {
    }
}
// Only the original Queue snapshot can satisfy this private source boundary.
pub trait QueuedDocumentSource: sealed::Sealed + Send + Sync {
    fn verify_scope(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DocumentError>;
    fn target(&self) -> EntityId;
    fn revision(&self) -> u64;
    fn total_bytes(&self) -> u64;
    fn epoch(&self) -> &Arc<DocumentEpoch>;
    fn require_current(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DocumentError>> + Send + '_>>;
    fn copy_range(&self, offset: usize, length: usize) -> Result<Vec<u8>, DocumentError>;
    fn retire(&self);
}
pub trait SourceResultDocumentSource: sealed::Sealed + Send + Sync {
    fn verify_scope(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DocumentError>;
    fn total_bytes(&self) -> u64;
    fn epoch(&self) -> &Arc<DocumentEpoch>;
    fn require_current(&self) -> Result<(), DocumentError>;
    fn copy_range(&self, offset: usize, length: usize) -> Result<Vec<u8>, DocumentError>;
    fn retire(&self);
}
#[derive(Clone)]
enum OriginalDocumentSource {
    Queued(Arc<dyn QueuedDocumentSource>),
    SourceResult(Arc<dyn SourceResultDocumentSource>),
}
#[derive(Clone)]
pub struct DocumentSourceLease {
    original: OriginalDocumentSource,
    expected: Option<crate::application::request_registry::RegistryOwnerScope>,
}
impl DocumentSourceLease {
    pub(in crate::application) fn queued(original: Arc<dyn QueuedDocumentSource>) -> Self {
        Self {
            original: OriginalDocumentSource::Queued(original),
            expected: None,
        }
    }
    pub(in crate::application) fn source_result(
        original: Arc<dyn SourceResultDocumentSource>,
    ) -> Self {
        Self {
            original: OriginalDocumentSource::SourceResult(original),
            expected: None,
        }
    }
    pub(crate) const fn kind(&self) -> DocumentStreamKind {
        match &self.original {
            OriginalDocumentSource::Queued(_) => DocumentStreamKind::QueuedInputBody,
            OriginalDocumentSource::SourceResult(_) => DocumentStreamKind::NativeSessionResult,
        }
    }
    pub(crate) fn target(&self) -> Option<EntityId> {
        match &self.original {
            OriginalDocumentSource::Queued(source) => Some(source.target()),
            OriginalDocumentSource::SourceResult(_) => None,
        }
    }
    pub(crate) fn revision(&self) -> Option<u64> {
        match &self.original {
            OriginalDocumentSource::Queued(source) => Some(source.revision()),
            OriginalDocumentSource::SourceResult(_) => None,
        }
    }
    pub(crate) fn total_bytes(&self) -> u64 {
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.total_bytes(),
            OriginalDocumentSource::SourceResult(source) => source.total_bytes(),
        }
    }
    pub(crate) fn epoch(&self) -> &Arc<DocumentEpoch> {
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.epoch(),
            OriginalDocumentSource::SourceResult(source) => source.epoch(),
        }
    }
    fn verify_scope(
        &self,
        scope: &crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<(), DocumentError> {
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.verify_scope(scope),
            OriginalDocumentSource::SourceResult(source) => source.verify_scope(scope),
        }
    }
    pub(crate) fn bind_registry(
        mut self,
        scope: crate::application::request_registry::RegistryOwnerScope,
    ) -> Result<Self, DocumentError> {
        self.verify_scope(&scope)?;
        self.expected = Some(scope);
        Ok(self)
    }
    pub(crate) async fn require_current(&self) -> Result<(), ViewError> {
        let scope = self.expected.as_ref().ok_or(ViewError::InvalidClient)?;
        self.verify_scope(scope)
            .map_err(|_| ViewError::InvalidClient)?;
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.require_current().await,
            OriginalDocumentSource::SourceResult(source) => source.require_current(),
        }
        .map_err(|_| ViewError::StaleCursor)
    }
    pub(crate) fn copy_range(
        &self,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, DocumentError> {
        self.verify_scope(self.expected.as_ref().ok_or(DocumentError::Closed)?)?;
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.copy_range(offset, length),
            OriginalDocumentSource::SourceResult(source) => source.copy_range(offset, length),
        }
    }
    pub(crate) fn retire(&self) {
        match &self.original {
            OriginalDocumentSource::Queued(source) => source.retire(),
            OriginalDocumentSource::SourceResult(source) => source.retire(),
        }
    }
}

fn serialize_target<S: serde::Serializer>(
    value: &Option<EntityId>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(id) => serializer.serialize_some(&id.encode()),
        None => serializer.serialize_none(),
    }
}

pub struct DocumentEpochGuard<'a> {
    state: std::sync::MutexGuard<'a, EpochState>,
}
impl DocumentEpochGuard<'_> {
    pub(crate) fn current(&self) -> bool {
        !self.state.sealed
    }
}
impl DocumentEpoch {
    pub(crate) fn lock_for_send(&self) -> Result<DocumentEpochGuard<'_>, ViewError> {
        Ok(DocumentEpochGuard {
            state: self.inner.lock().map_err(|_| ViewError::ReadFailure)?,
        })
    }
}

pub trait DocumentCommitObserver: Send + Sync {
    fn socket_committed(&self);
}

fn serialize_revision<S: serde::Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_some(&value.to_string()),
        None => serializer.serialize_none(),
    }
}
fn serialize_bytes<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}
