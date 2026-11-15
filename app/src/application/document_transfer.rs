//! Fixed Queue documents on the original linear socket transport. No Native view or replay state.
#![allow(
    clippy::significant_drop_tightening,
    clippy::unnecessary_wraps,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
#[cfg(test)]
use super::shared_views::local_transfer::LocalTransferObservation;
use super::{
    document_source::{DocumentError, DocumentSourceLease, DocumentStreamDescriptor},
    shared_views::{
        local_transfer::LocalCreditResult,
        socket_authority::SocketViewClient,
        socket_transfer::{PreparedSocketChunk, SocketTransferStream},
    },
};
use crate::claims::Id128;
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentPrepare {
    AwaitCredit,
    Prepared,
    Complete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentEnqueue {
    Queued,
    Backpressured,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentCredit {
    Initial,
    HeldEarly,
    Duplicate,
    Committed,
    Sealed,
}
pub struct PreparedDocumentChunk {
    socket: PreparedSocketChunk,
}
pub enum DocumentEnqueueFailure {
    Rejected(PreparedDocumentChunk),
    StateLost,
}
impl PreparedDocumentChunk {
    pub(crate) fn into_socket(self) -> PreparedSocketChunk {
        self.socket
    }
    pub(crate) fn rejected(socket: PreparedSocketChunk) -> Result<Self, DocumentError> {
        if !socket.is_document() {
            return Err(DocumentError::Closed);
        }
        Ok(Self { socket })
    }
}
#[derive(Clone, Copy)]
enum Position {
    Body(u64),
    End,
    Complete,
}
struct Pending {
    packet: u64,
    after: Position,
}
struct ReadData {
    closed: bool,
    position: Position,
    pending: Option<Pending>,
    prepared: Option<PreparedDocumentChunk>,
}
pub struct DocumentReadState {
    source: DocumentSourceLease,
    stream: SocketTransferStream,
    descriptor: DocumentStreamDescriptor,
    data: Mutex<ReadData>,
    preparing: tokio::sync::Mutex<()>,
    handles: AtomicUsize,
}
pub struct DocumentTransfer {
    state: Arc<DocumentReadState>,
}
impl Clone for DocumentTransfer {
    fn clone(&self) -> Self {
        self.state.handles.fetch_add(1, Ordering::AcqRel);
        Self {
            state: self.state.clone(),
        }
    }
}
impl Drop for DocumentTransfer {
    fn drop(&mut self) {
        if self.state.handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.state.close();
        }
    }
}
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, DocumentError> {
    m.lock().map_err(|_| DocumentError::Closed)
}
impl DocumentReadState {
    fn synchronize(&self) -> Result<bool, DocumentError> {
        let observation = self.stream.observation()?;
        let complete = {
            let mut data = lock(&self.data)?;
            if let Some(pending) = &data.pending
                && observation.highest_committed == pending.packet
            {
                data.position = pending.after;
                data.pending = None;
            }
            matches!(data.position, Position::Complete)
        };
        if complete || self.stream.explicitly_sealed()? {
            self.close();
        }
        Ok(complete)
    }

    pub(crate) fn close(&self) {
        let prepared = {
            let mut data = self
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if data.closed {
                return;
            }
            data.closed = true;
            data.pending = None;
            data.prepared.take()
        };
        drop(prepared);
        let _ = self.stream.seal();
        self.source.retire();
    }
}
impl DocumentTransfer {
    pub(crate) fn open(
        source: DocumentSourceLease,
        client: &SocketViewClient,
    ) -> Result<Self, DocumentError> {
        let document_id = Id128::new(crate::random::bytes().map_err(|_| {
            DocumentError::View(crate::domain::shared_view::ViewError::RandomUnavailable)
        })?);
        let stream = SocketTransferStream::open_document(source.clone(), client.context.clone())?;
        let descriptor = DocumentStreamDescriptor {
            stream_kind: source.kind(),
            stream_id: stream.id(),
            document_id,
            target_id: source.target(),
            revision: source.revision(),
            total_bytes: source.total_bytes(),
            encoding: "utf8_json",
        };
        let state = Arc::new(DocumentReadState {
            source,
            stream,
            descriptor,
            data: Mutex::new(ReadData {
                closed: false,
                position: Position::Body(0),
                pending: None,
                prepared: None,
            }),
            preparing: tokio::sync::Mutex::new(()),
            handles: AtomicUsize::new(1),
        });
        let observer: Arc<dyn crate::application::document_source::DocumentCommitObserver> =
            state.clone();
        state
            .stream
            .install_document_commit(Arc::downgrade(&observer))?;
        let participant: Arc<dyn crate::application::document_source::DocumentStreamSeal> =
            state.clone();
        state
            .source
            .epoch()
            .register(Arc::downgrade(&participant))?;
        let participant: Arc<
            dyn crate::application::shared_views::socket_authority::SocketLifetimeParticipant,
        > = state.clone();
        client
            .context
            .lifetime
            .register_participant(Arc::downgrade(&participant))?;
        Ok(Self { state })
    }
    pub(crate) fn descriptor(&self) -> &DocumentStreamDescriptor {
        &self.state.descriptor
    }
    pub(crate) fn weak_state(&self) -> Weak<DocumentReadState> {
        Arc::downgrade(&self.state)
    }
    pub(crate) fn socket_stream(&self) -> SocketTransferStream {
        self.state.stream.clone()
    }
    fn check(&self, client: &SocketViewClient) -> Result<(), DocumentError> {
        if !self.state.stream.belongs_to(client) {
            return Err(crate::domain::shared_view::ViewError::InvalidClient.into());
        }
        Ok(())
    }
    pub(crate) async fn prepare_next(
        &self,
        client: &SocketViewClient,
    ) -> Result<DocumentPrepare, DocumentError> {
        self.check(client)?;
        let _preparing = self.state.preparing.lock().await;
        // Current actor is rechecked even when observing a completed document. No std lock is held.
        let (_, guard) = client
            .context
            .authority()
            .current_actor_guard(client)
            .await?;
        if self.state.synchronize()? {
            return Ok(DocumentPrepare::Complete);
        }
        let position = {
            let data = lock(&self.state.data)?;
            if data.closed {
                return Err(DocumentError::Closed);
            }
            if data.prepared.is_some() {
                return Ok(DocumentPrepare::Prepared);
            }
            if data.pending.is_some() {
                return Ok(DocumentPrepare::AwaitCredit);
            }
            data.position
        };
        drop(guard);
        if !self.state.stream.ready()? {
            return Ok(DocumentPrepare::AwaitCredit);
        }
        let (offset, end) = match position {
            Position::Body(offset) => (offset, false),
            Position::End => (0, true),
            Position::Complete => return Ok(DocumentPrepare::Complete),
        };
        let socket = self
            .state
            .stream
            .prepare_document(client, offset, end)
            .await?;
        let header = socket.header();
        let after = if end {
            Position::Complete
        } else if header.is_last() {
            Position::End
        } else {
            Position::Body(
                header
                    .offset()
                    .checked_add(u64::from(header.payload_length()))
                    .ok_or(DocumentError::Allocation)?,
            )
        };
        let prepared = PreparedDocumentChunk { socket };
        let mut data = lock(&self.state.data)?;
        if data.closed {
            drop(data);
            drop(prepared);
            return Err(DocumentError::Closed);
        }
        data.pending = Some(Pending {
            packet: header.packet(),
            after,
        });
        data.prepared = Some(prepared);
        Ok(DocumentPrepare::Prepared)
    }
    pub(crate) fn enqueue_with(
        &self,
        client: &SocketViewClient,
        enqueue: impl FnOnce(PreparedDocumentChunk) -> Result<(), DocumentEnqueueFailure>,
    ) -> Result<DocumentEnqueue, DocumentError> {
        self.check(client)?;
        let prepared = {
            let mut data = lock(&self.state.data)?;
            if data.closed {
                return Err(DocumentError::Closed);
            }
            data.prepared.take().ok_or(DocumentError::View(
                crate::domain::shared_view::ViewError::InvalidStage,
            ))?
        };
        match enqueue(prepared) {
            Ok(()) => Ok(DocumentEnqueue::Queued),
            Err(DocumentEnqueueFailure::StateLost) => {
                self.state.close();
                Err(DocumentError::Closed)
            }
            Err(DocumentEnqueueFailure::Rejected(prepared)) => {
                let mut data = lock(&self.state.data)?;
                if data.closed {
                    drop(data);
                    drop(prepared);
                    return Err(DocumentError::Closed);
                }
                data.prepared = Some(prepared);
                Ok(DocumentEnqueue::Backpressured)
            }
        }
    }
    pub(crate) async fn accept_control(
        &self,
        client: &SocketViewClient,
        frame: &[u8],
    ) -> Result<DocumentCredit, DocumentError> {
        self.check(client)?;
        let result = self.state.stream.accept_control(client, frame).await?;
        let receipt = match result {
            LocalCreditResult::Initial => DocumentCredit::Initial,
            LocalCreditResult::HeldEarly => DocumentCredit::HeldEarly,
            LocalCreditResult::Duplicate => DocumentCredit::Duplicate,
            LocalCreditResult::Committed => DocumentCredit::Committed,
            LocalCreditResult::Sealed => DocumentCredit::Sealed,
        };
        if matches!(receipt, DocumentCredit::Sealed) {
            self.state.close();
        } else {
            let _ = self.state.synchronize()?;
        }
        Ok(receipt)
    }
    pub(crate) fn detach(&self) -> Result<(), DocumentError> {
        self.state.close();
        Ok(())
    }
    pub(crate) async fn wait_closed(&self) {
        self.state.stream.wait_closed().await;
    }
    #[cfg(test)]
    pub(crate) fn observation(&self) -> Result<LocalTransferObservation, DocumentError> {
        Ok(self.state.stream.observation()?)
    }
}

impl crate::application::document_source::DocumentStreamSeal for DocumentReadState {
    fn seal_from_epoch(&self) {
        self.close();
    }
}
impl crate::application::shared_views::socket_authority::SocketLifetimeParticipant
    for DocumentReadState
{
    fn seal(&self) {
        self.close();
    }
}

impl crate::application::document_source::DocumentCommitObserver for DocumentReadState {
    fn socket_committed(&self) {
        if self.synchronize().is_err() {
            self.close();
        }
    }
}
