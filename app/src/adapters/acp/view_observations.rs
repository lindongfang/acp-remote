//! Bounded raw display observations. Connection ownership and authorization belong to the caller.
use super::{
    ConnectionScope, Incoming, RawMessage, Received, SessionEvent, Strategy, UpdateOrigin, Version,
};
use crate::domain::views::{
    PublishFact, StageWorkspace, ViewError, ViewFailure, ViewHandle, ViewWindows,
};

use std::fmt;

const HEADER_BYTES: usize = 32;
const READ_SCRATCH: usize = 1024;
const FRAME_SCRATCH: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationFailure {
    Capacity,
    Structure,
    CounterExhausted,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObservationStats {
    pub last_sequence: u64,
    pub dropped_count: u64,
    pub first_dropped: Option<u64>,
    pub last_dropped: Option<u64>,
    pub first_failure: Option<ObservationFailure>,
    pub closed: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationOutcome {
    Ignored,
    Recorded {
        sequence: u64,
        publication: PublishFact,
    },
    Dropped {
        sequence: u64,
        reason: ObservationFailure,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ObservationError {
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("input is not a consistent ACP update notification")]
    InvalidObservation,
    #[error("display observation counter is exhausted")]
    CounterExhausted,
    #[error("raw observation header is invalid or unsupported")]
    InvalidHeader,
}

/// Borrowed data only. Header decoding does not validate JSON, authority or execution facts.
pub struct RawObservation<'a> {
    pub version: Version,
    pub origin: UpdateOrigin,
    pub sequence: u64,
    pub raw: &'a [u8],
}
impl fmt::Debug for RawObservation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawObservation")
            .field("version", &self.version)
            .field("origin", &self.origin)
            .field("sequence", &self.sequence)
            .field("raw_bytes", &self.raw.len())
            .finish()
    }
}
/// Decodes the fixed format and borrows the payload without copying or parsing it.
/// # Errors
/// Rejects unsupported tags, invalid counters, reserved bits or inexact payload lengths.
pub fn parse_header(bytes: &[u8]) -> Result<RawObservation<'_>, ObservationError> {
    let header = bytes
        .get(..HEADER_BYTES)
        .ok_or(ObservationError::InvalidHeader)?;
    if &header[..4] != b"ACPO" || header[4] != 1 || header[7] != 1 || header[24..32] != [0; 8] {
        return Err(ObservationError::InvalidHeader);
    }
    let version = match header[5] {
        1 => Version::V1,
        2 => Version::V2,
        _ => return Err(ObservationError::InvalidHeader),
    };
    let origin = match header[6] {
        1 => UpdateOrigin::Live,
        2 => UpdateOrigin::DuringResume,
        3 => UpdateOrigin::DuringReplay,
        _ => return Err(ObservationError::InvalidHeader),
    };
    let sequence = u64::from_le_bytes(
        header[8..16]
            .try_into()
            .map_err(|_| ObservationError::InvalidHeader)?,
    );
    let length = u64::from_le_bytes(
        header[16..24]
            .try_into()
            .map_err(|_| ObservationError::InvalidHeader)?,
    );
    let length = usize::try_from(length).map_err(|_| ObservationError::InvalidHeader)?;
    if sequence == 0 || length == 0 || HEADER_BYTES.checked_add(length) != Some(bytes.len()) {
        return Err(ObservationError::InvalidHeader);
    }
    Ok(RawObservation {
        version,
        origin,
        sequence,
        raw: &bytes[HEADER_BYTES..],
    })
}

/// One owner composes one instance per read. This wrapper is not a connection or authority token.
pub struct ObservationRead {
    scope: ConnectionScope,
    handle: ViewHandle,
    version: Version,
    stats: ObservationStats,
    // Last field: all read metadata retires before its fixed header/scratch charge.
    _workspace: StageWorkspace,
}
impl ObservationRead {
    /// Binds an existing read and copies only the selected version, not initialization payloads.
    /// # Errors
    /// Rejects foreign/stale handles or unavailable shared workspace capacity.
    pub fn begin(
        windows: &mut ViewWindows,
        handle: &ViewHandle,
        strategy: &Strategy,
    ) -> Result<Self, ObservationError> {
        let workspace = windows.stage_workspace(handle, READ_SCRATCH)?;
        Ok(Self {
            handle: handle.clone(),
            version: strategy.version(),
            scope: strategy.scope.clone(),
            stats: ObservationStats::default(),
            _workspace: workspace,
        })
    }
    #[must_use]
    pub const fn stats(&self) -> ObservationStats {
        self.stats
    }

    /// Stores matching display data. The caller routes control events and verifies the real source.
    /// # Errors
    /// Rejects inconsistent inputs, stale display reads or exhausted counters without a wire action.
    pub fn observe(
        &mut self,
        windows: &mut ViewWindows,
        received: &Received,
    ) -> Result<ObservationOutcome, ObservationError> {
        if received.scope() != self.scope {
            return Err(ObservationError::InvalidObservation);
        }
        self.observe_inner(windows, received.incoming())
    }
    pub(super) fn observe_inner(
        &mut self,
        windows: &mut ViewWindows,
        incoming: &Incoming,
    ) -> Result<ObservationOutcome, ObservationError> {
        let Incoming::Update { event, origin, raw } = incoming else {
            return Ok(ObservationOutcome::Ignored);
        };
        let session = match event {
            SessionEvent::State { session, .. }
            | SessionEvent::Tool { session, .. }
            | SessionEvent::Message { session, .. }
            | SessionEvent::Other { session, .. } => session.as_str(),
        };
        if session != self.handle.target().native_id()
            || (self.version == Version::V2 && matches!(event, SessionEvent::State { .. }))
        {
            return Ok(ObservationOutcome::Ignored);
        }
        windows.window_status(&self.handle)?;
        let (_, tag) = context(event, raw)?;
        if self.version == Version::V2 && tag == "state_update" {
            return Ok(ObservationOutcome::Ignored);
        }
        if self.stats.closed {
            return Err(ObservationError::CounterExhausted);
        }
        let Some(sequence) = self.stats.last_sequence.checked_add(1) else {
            return Err(self.close_counter(windows));
        };
        self.stats.last_sequence = sequence;
        let operation = self.store(windows, raw.bytes(), *origin, sequence);
        match operation {
            Ok(publication) => Ok(ObservationOutcome::Recorded {
                sequence,
                publication,
            }),
            Err(ViewError::SequenceExhausted) => {
                self.dropped(windows, sequence, ObservationFailure::CounterExhausted)?;
                Err(self.close_counter(windows))
            }
            Err(ViewError::Stale) => Err(ObservationError::View(ViewError::Stale)),
            Err(error) => {
                let reason = if error == ViewError::Capacity {
                    ObservationFailure::Capacity
                } else {
                    ObservationFailure::Structure
                };
                self.dropped(windows, sequence, reason)
            }
        }
    }
    fn dropped(
        &mut self,
        windows: &mut ViewWindows,
        sequence: u64,
        reason: ObservationFailure,
    ) -> Result<ObservationOutcome, ObservationError> {
        self.stats.first_dropped.get_or_insert(sequence);
        self.stats.last_dropped = Some(sequence);
        self.stats.first_failure.get_or_insert(reason);
        let Some(count) = self.stats.dropped_count.checked_add(1) else {
            return Err(self.close_counter(windows));
        };
        self.stats.dropped_count = count;
        let failure = match reason {
            ObservationFailure::Capacity => ViewFailure::Capacity,
            ObservationFailure::Structure => ViewFailure::Structure,
            ObservationFailure::CounterExhausted => ViewFailure::SequenceExhausted,
        };
        windows.fail(&self.handle, failure)?;
        Ok(ObservationOutcome::Dropped { sequence, reason })
    }
    fn close_counter(&mut self, windows: &mut ViewWindows) -> ObservationError {
        self.stats.closed = true;
        self.stats
            .first_failure
            .get_or_insert(ObservationFailure::CounterExhausted);
        // A stale read must never mark the replacement window.
        let _ = windows.fail(&self.handle, ViewFailure::SequenceExhausted);
        ObservationError::CounterExhausted
    }
    fn store(
        &self,
        windows: &mut ViewWindows,
        raw: &[u8],
        origin: UpdateOrigin,
        sequence: u64,
    ) -> Result<PublishFact, ViewError> {
        let workspace = windows.stage_workspace(&self.handle, FRAME_SCRATCH)?;
        let length = HEADER_BYTES
            .checked_add(raw.len())
            .ok_or(ViewError::Capacity)?;
        let raw_length = u64::try_from(raw.len()).map_err(|_| ViewError::Capacity)?;
        let mut staged = windows.stage_insert(&self.handle, workspace, None, length)?;
        {
            let mut parts = staged.parts();
            let header = parts
                .scratch
                .get_mut(..HEADER_BYTES)
                .ok_or(ViewError::Capacity)?;
            header.fill(0);
            header[..4].copy_from_slice(b"ACPO");
            header[4] = 1;
            header[5] = match self.version {
                Version::V1 => 1,
                Version::V2 => 2,
            };
            header[6] = match origin {
                UpdateOrigin::Live => 1,
                UpdateOrigin::DuringResume => 2,
                UpdateOrigin::DuringReplay => 3,
            };
            header[7] = 1;
            header[8..16].copy_from_slice(&sequence.to_le_bytes());
            header[16..24].copy_from_slice(&raw_length.to_le_bytes());
            parts.writer.write_all(header)?;
            parts.writer.write_all(raw)?;
        }
        windows.publish(&self.handle, staged)
    }
}

fn context(event: &SessionEvent, raw: &RawMessage) -> Result<(String, String), ObservationError> {
    let bad = || ObservationError::InvalidObservation;
    let fields = raw.fields().map_err(|_| bad())?;
    let string = |v: Option<super::raw_json::RawValueRef<'_>>| v.and_then(|v| v.string().ok());
    if string(fields.get("jsonrpc").map_err(|_| bad())?).as_deref() != Some("2.0")
        || string(fields.get("method").map_err(|_| bad())?).as_deref() != Some("session/update")
        || fields.get("id").map_err(|_| bad())?.is_some()
    {
        return Err(bad());
    }
    let params = fields
        .get("params")
        .map_err(|_| bad())?
        .and_then(super::raw_json::RawValueRef::object)
        .ok_or_else(bad)?;
    let session = string(params.get("sessionId").map_err(|_| bad())?).ok_or_else(bad)?;
    let update = params
        .get("update")
        .map_err(|_| bad())?
        .and_then(super::raw_json::RawValueRef::object)
        .ok_or_else(bad)?;
    let tag = string(update.get("sessionUpdate").map_err(|_| bad())?).ok_or_else(bad)?;
    let consistent = match event {
        SessionEvent::Message {
            session: actual,
            kind,
            ..
        }
        | SessionEvent::Other {
            session: actual,
            kind,
        } => actual.as_str() == session && kind.as_ref() == tag,
        SessionEvent::Tool {
            session: actual,
            tool,
        } => {
            actual.as_str() == session
                && matches!(tag.as_str(), "tool_call" | "tool_call_update")
                && string(update.get("toolCallId").map_err(|_| bad())?).as_deref()
                    == Some(tool.as_str())
        }
        SessionEvent::State {
            session: actual,
            state,
            ..
        } => {
            actual.as_str() == session
                && tag == "state_update"
                && string(update.get("state").map_err(|_| bad())?).as_deref()
                    == Some(state.as_ref())
        }
    };
    if !consistent {
        return Err(bad());
    }
    Ok((session, tag))
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod observation_independent;
