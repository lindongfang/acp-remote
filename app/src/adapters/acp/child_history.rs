//! Project child-history extension. A response chain is private until the final chunk validates.
use super::{
    AcpError, RawMessage, Request, RequestKind, SessionId, Strategy,
    model::{field, text},
    raw_model as raw,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    collections::HashSet,
    sync::{Arc, atomic::Ordering},
};

pub(super) const KEY: &str = "io.github.lindongfang.acp-remote/child-history";
const METHOD: &str = "_io.github.lindongfang.acp-remote/child-history";
const MAX_CHUNK: usize = 16 * 1024;

impl Strategy {
    /// Builds a request only while this connection's negotiated extension is available.
    /// # Errors
    /// Returns an error for missing negotiation, invalid targets or encoding failure.
    pub fn child_history(
        &self,
        root: &SessionId,
        child: &SessionId,
        cursor: Option<&str>,
    ) -> Result<Request, AcpError> {
        if !self.child_history_enabled.load(Ordering::Acquire) {
            return Err(AcpError::UnsupportedCapability);
        }
        if root == child {
            return Err(AcpError::InvalidMessage);
        }
        let cursor = cursor.map_or_else(|| raw::literal("null"), raw::string)?;
        let params = raw::object(
            [
                ("rootSessionId", &raw::string(root.as_str())?),
                ("sessionId", &raw::string(child.as_str())?),
                ("cursor", &cursor),
            ],
            None,
        )?;
        Ok(self.request(METHOD, RequestKind::ChildHistory, params, None))
    }
}

/// An in-progress snapshot. Consuming the assembler on each step discards all data on error.
pub struct ChildHistoryAssembly {
    strategy: Strategy,
    root: SessionId,
    child: SessionId,
    sequence: u64,
    snapshot: Option<Arc<str>>,
    cursors: HashSet<Arc<str>>,
    partial: Vec<u8>,
    frames: Vec<RawMessage>,
}

pub enum ChildHistoryProgress {
    More {
        cursor: Arc<str>,
        assembly: ChildHistoryAssembly,
    },
    Complete(Vec<RawMessage>),
}

impl ChildHistoryAssembly {
    #[must_use]
    pub fn new(strategy: Strategy, root: SessionId, child: SessionId) -> Self {
        Self {
            strategy,
            root,
            child,
            sequence: 0,
            snapshot: None,
            cursors: HashSet::new(),
            partial: Vec::new(),
            frames: Vec::new(),
        }
    }

    pub(crate) const fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// # Errors
    /// Rejects mismatched targets, framing, ordering, snapshot changes or invalid ACP notifications.
    pub fn accept(mut self, message: &RawMessage) -> Result<ChildHistoryProgress, AcpError> {
        let result = message.field("result")?.ok_or(AcpError::InvalidMessage)?;
        let string = |key| text(field(result, key)?);
        let snapshot = string("snapshotId")?;
        if SessionId::parse(field(result, "rootSessionId")?)? != self.root
            || SessionId::parse(field(result, "sessionId")?)? != self.child
            || snapshot.is_empty()
            || self
                .snapshot
                .as_ref()
                .is_some_and(|prior| *prior != snapshot)
            || *string("sequence")? != self.sequence.to_string()
        {
            return Err(AcpError::InvalidMessage);
        }
        let cursor = field(result, "nextCursor")?;
        let next = if cursor.is_null() {
            None
        } else {
            Some(text(cursor)?)
        };
        let encoded = string("chunk")?;
        if encoded.len() > (MAX_CHUNK * 4).div_ceil(3)
            || (encoded.is_empty() && (next.is_some() || self.sequence != 0))
        {
            return Err(AcpError::InvalidMessage);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded.as_bytes())
            .map_err(|_| AcpError::InvalidMessage)?;
        for piece in bytes.split_inclusive(|byte| *byte == b'\n') {
            self.partial
                .try_reserve(piece.len())
                .map_err(|_| AcpError::Allocation)?;
            if let Some(end) = piece.strip_suffix(b"\n") {
                self.partial.extend_from_slice(end);
                let frame = RawMessage::parse(std::mem::take(&mut self.partial))?;
                self.validate_frame(&frame)?;
                self.frames
                    .try_reserve(1)
                    .map_err(|_| AcpError::Allocation)?;
                self.frames.push(frame);
            } else {
                self.partial.extend_from_slice(piece);
            }
        }
        if let Some(cursor) = next {
            self.cursors
                .try_reserve(1)
                .map_err(|_| AcpError::Allocation)?;
            if !self.cursors.insert(cursor.clone()) {
                return Err(AcpError::InvalidMessage);
            }
            self.sequence = self.sequence.checked_add(1).ok_or(AcpError::Capacity)?;
            self.snapshot = Some(snapshot);
            Ok(ChildHistoryProgress::More {
                cursor,
                assembly: self,
            })
        } else if self.partial.is_empty() {
            Ok(ChildHistoryProgress::Complete(self.frames))
        } else {
            Err(AcpError::InvalidMessage)
        }
    }

    fn validate_frame(&self, frame: &RawMessage) -> Result<(), AcpError> {
        let required = |key| frame.field(key)?.ok_or(AcpError::InvalidMessage);
        if &*text(required("jsonrpc")?)? != "2.0"
            || &*text(required("method")?)? != "session/update"
            || frame.field("id")?.is_some()
            || frame.field("result")?.is_some()
            || frame.field("error")?.is_some()
            || self.strategy.update(required("params")?)?.session() != &self.child
        {
            return Err(AcpError::InvalidMessage);
        }
        Ok(())
    }
}
