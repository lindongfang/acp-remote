//! Actual standard Replay receipts and source raw frontier. Transport is an independent fact.
//!
//! This component owns neither a Ticket nor a Native resource; its enclosing operation owner
//! must retain its original Reservation/Ticket and join it. No dispatch/Loaded proof is minted.
use super::{
    Connection, ConnectionScope, Incoming, OutboundKind, Received, RequestKind, Reservation,
    Response, RpcId, SessionId, WriteProbe, WriteProgress, WriteReport,
    shared_view_capture::RetainedAcpRaw,
};
use crate::{
    application::shared_views::ingress::IngressProbe,
    domain::shared_view::{ReadEpoch, ViewError},
};

pub(super) const fn request_error(error: super::AcpError) -> ViewError {
    use super::AcpError;
    match error {
        AcpError::UnsupportedCapability => ViewError::UnsupportedCapability,
        AcpError::ConfigurationIncomplete
        | AcpError::ConfigurationVersionMismatch
        | AcpError::UnencodableConfiguration => ViewError::InvalidConfiguration,
        AcpError::Allocation => ViewError::AllocationFailure,
        AcpError::InvalidRequestScope => ViewError::InvalidScope,
        AcpError::Closed => ViewError::SourceReleased,
        _ => ViewError::ProtocolFailure,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayTransportState {
    Reserved,
    Queued,
    Writing,
    Written,
    Unknown,
    Zero,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayCompletionState {
    AwaitingResponse,
    AwaitingCapture,
    Complete,
    Incomplete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayFreshness {
    CurrentObservation,
    Stale,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayReadObservation {
    epoch: ReadEpoch,
    transport: ReplayTransportState,
    completion: ReplayCompletionState,
    freshness: ReplayFreshness,
    response_sequence: Option<u64>,
    response_receipt_order: Option<u64>,
    required_cut: Option<u64>,
    failure: Option<ViewError>,
}

#[cfg(test)]
mod tests;
impl ReplayReadObservation {
    #[must_use]
    pub const fn read_epoch(self) -> ReadEpoch {
        self.epoch
    }
    #[must_use]
    pub const fn transport(self) -> ReplayTransportState {
        self.transport
    }
    #[must_use]
    pub const fn completion(self) -> ReplayCompletionState {
        self.completion
    }
    #[must_use]
    pub const fn freshness(self) -> ReplayFreshness {
        self.freshness
    }
    #[must_use]
    pub const fn response_sequence(self) -> Option<u64> {
        self.response_sequence
    }
    #[must_use]
    pub const fn response_receipt_order(self) -> Option<u64> {
        self.response_receipt_order
    }
    #[must_use]
    pub const fn required_cut(self) -> Option<u64> {
        self.required_cut
    }
    #[must_use]
    pub const fn failure(self) -> Option<ViewError> {
        self.failure
    }
}
struct CompletionBoundary {
    sequence: u64,
    receipt_order: u64,
}
pub(super) struct ReplayResponseBoundary {
    pub scope: ConnectionScope,
    pub session: SessionId,
    pub epoch: ReadEpoch,
    pub sequence: u64,
    pub receipt_order: u64,
}
pub struct AcpReplayRead {
    connection: Connection,
    scope: ConnectionScope,
    rpc: RpcId,
    session: SessionId,
    epoch: ReadEpoch,
    probe: WriteProbe,
    ingress: IngressProbe<RetainedAcpRaw>,
    response: Option<CompletionBoundary>,
    complete: bool,
    remote_failed: bool,
    prefix: std::sync::Arc<crate::application::shared_views::read_prefix::ReadPrefix>,
}
impl AcpReplayRead {
    pub(super) fn capture_prefix(
        &self,
    ) -> std::sync::Arc<crate::application::shared_views::read_prefix::ReadPrefix> {
        std::sync::Arc::clone(&self.prefix)
    }
    pub(super) const fn original_rpc(&self) -> &RpcId {
        &self.rpc
    }
    pub(super) const fn original_epoch(&self) -> ReadEpoch {
        self.epoch
    }
    pub(super) const fn original_session(&self) -> &SessionId {
        &self.session
    }
    pub(super) fn original_scope(&self) -> ConnectionScope {
        self.scope.clone()
    }
    pub(super) fn original_write_probe(&self) -> WriteProbe {
        self.probe.clone()
    }
    pub(super) fn response_boundary(&self) -> Option<ReplayResponseBoundary> {
        self.response.as_ref().map(|value| ReplayResponseBoundary {
            scope: self.scope.clone(),
            session: self.session.clone(),
            epoch: self.epoch,
            sequence: value.sequence,
            receipt_order: value.receipt_order,
        })
    }
    pub(super) fn from_reservation(
        reservation: &Reservation,
        scope: &ConnectionScope,
        ingress: IngressProbe<RetainedAcpRaw>,
        connection: Connection,
        epoch: ReadEpoch,
    ) -> Result<Self, ViewError> {
        if &reservation.scope() != scope || connection.scope() != *scope {
            return Err(ViewError::InvalidScope);
        }
        let OutboundKind::Request(RequestKind::Replay(session)) = reservation.kind() else {
            return Err(ViewError::InvalidTarget);
        };
        let rpc = reservation.id().ok_or(ViewError::InvalidTarget)?.clone();
        let prefix = ingress.track_prefix(
            crate::application::shared_views::read_prefix::ReadPrefixIdentity {
                rpc: match &rpc {
                    RpcId::Integer(id) => {
                        crate::application::shared_views::read_control::RpcScalar::Integer(*id)
                    }
                    RpcId::String(id) => {
                        crate::application::shared_views::read_control::RpcScalar::String(
                            std::sync::Arc::clone(id),
                        )
                    }
                },
                session: session.shared_text(),
                cell: reservation.view_cell_key(),
            },
        )?;
        Ok(Self {
            connection,
            scope: scope.clone(),
            rpc,
            session: session.clone(),
            epoch,
            probe: reservation.probe(),
            ingress,
            response: None,
            complete: false,
            remote_failed: false,
            prefix,
        })
    }
    /// Borrows the same original event only after that event has entered its source capture.
    /// # Errors
    /// Rejects a foreign real connection or an event that has not been captured on this source.
    pub fn observe_received(&mut self, received: &Received) -> Result<bool, ViewError> {
        if received.scope() != self.scope {
            return Err(ViewError::InvalidScope);
        }
        let Incoming::Response { id, response, .. } = received.incoming() else {
            return Ok(false);
        };
        if *id != self.rpc {
            return Ok(false);
        }
        if self.ingress.observed_frontier() < received.sequence() {
            return Err(ViewError::CaptureGap);
        }
        match response {
            Response::Replayed { session } if *session == self.session => {
                if self.response.is_some() {
                    return Err(ViewError::CaptureGap);
                }
                self.response = Some(CompletionBoundary {
                    sequence: received.sequence(),
                    receipt_order: received.receipt_order(),
                });
                self.prefix.response(received.sequence());
                Ok(true)
            }
            Response::RemoteError { .. } => {
                self.remote_failed = true;
                self.prefix.finish();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    /// The source's actual processed cut gates completion. Written/Unknown do not.
    /// A later source/transport failure cannot erase a completion already observed.
    #[must_use]
    pub fn observation(&mut self) -> ReplayReadObservation {
        let transport = match self.probe.progress() {
            WriteProgress::Reserved => ReplayTransportState::Reserved,
            WriteProgress::Queued => ReplayTransportState::Queued,
            WriteProgress::Writing { .. } | WriteProgress::Flushing { .. } => {
                ReplayTransportState::Writing
            }
            WriteProgress::Finished(WriteReport::Written) => ReplayTransportState::Written,
            WriteProgress::Finished(WriteReport::Unknown { .. }) => ReplayTransportState::Unknown,
            WriteProgress::Finished(WriteReport::Zero { .. }) => ReplayTransportState::Zero,
        };
        let mut failure = None;
        if let Some(boundary) = &self.response {
            let gap = self.prefix.failure();
            if !self.complete
                && gap.is_none()
                && self.ingress.processed_frontier() >= boundary.sequence
            {
                self.complete = true;
            }
            if !self.complete {
                failure = gap;
                if failure.is_none()
                    && (self.connection.is_closed() || self.ingress.is_sealed())
                    && self.ingress.processed_frontier() < boundary.sequence
                {
                    failure = Some(ViewError::SourceReleased);
                }
            }
        }
        let completion = if self.complete {
            ReplayCompletionState::Complete
        } else if failure.is_some() || self.remote_failed {
            ReplayCompletionState::Incomplete
        } else if self.response.is_some() {
            ReplayCompletionState::AwaitingCapture
        } else if self.connection.is_closed()
            || matches!(
                transport,
                ReplayTransportState::Unknown | ReplayTransportState::Zero
            )
        {
            ReplayCompletionState::Incomplete
        } else {
            ReplayCompletionState::AwaitingResponse
        };
        if self.remote_failed && !self.complete {
            failure = Some(ViewError::ReadFailure);
        }
        if matches!(
            completion,
            ReplayCompletionState::Complete | ReplayCompletionState::Incomplete
        ) {
            self.prefix.finish();
        }
        ReplayReadObservation {
            epoch: self.epoch,
            transport,
            completion,
            freshness: if self.connection.is_closed() || self.ingress.is_sealed() {
                ReplayFreshness::Stale
            } else {
                ReplayFreshness::CurrentObservation
            },
            response_sequence: self.response.as_ref().map(|v| v.sequence),
            response_receipt_order: self.response.as_ref().map(|v| v.receipt_order),
            required_cut: self.response.as_ref().map(|v| v.sequence),
            failure,
        }
    }
}
