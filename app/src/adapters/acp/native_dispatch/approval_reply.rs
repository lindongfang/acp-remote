use super::{
    AcpControlReservation, AcpError, AcpNativeAdapter, AcpSource, Arc, ConnectionScope, Context,
    EncodedFrame, FenceRejection, Instant, OutboundKind, PermissionRequest, PermissionSelection,
    PollBridge, PollIo, RawMessage, Reservation, WriteFence, WriteIdentity, WriteProbe, WriteStep,
    WriteTicket, d, error, n, reject, scalar, step,
};
use crate::domain::approval_owner::choice_fingerprint;
use sha2::{Digest, Sha256};

pub struct AcpPermissionReplyReservation {
    actual: Reservation,
    identity: n::PermissionReplyIdentity<ConnectionScope>,
}
impl n::ReservedPermissionReply for AcpPermissionReplyReservation {
    type Scope = ConnectionScope;
    type Frame = EncodedFrame;
    type Probe = WriteProbe;
    fn identity(&self) -> &n::PermissionReplyIdentity<ConnectionScope> {
        &self.identity
    }
    fn probe(&self) -> WriteProbe {
        self.actual.probe()
    }
    fn encode(&self) -> Result<EncodedFrame, d::DispatchError> {
        self.actual.encode().map_err(error)
    }
}

struct ApprovalReplyFence {
    identity: n::PermissionReplyIdentity<ConnectionScope>,
    boundary: Arc<dyn n::PermissionReplyBoundary<ConnectionScope>>,
}
impl WriteFence for ApprovalReplyFence {
    fn check_and_poll(
        &self,
        write: &WriteIdentity,
        write_step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        if write.scope() != *self.identity.scope()
            || write.id().map(scalar).as_ref() != Some(self.identity.rpc())
            || !matches!(write.kind(), OutboundKind::PermissionReply { session, id }
                if session.as_str() == self.identity.native_session() && scalar(id) == *self.identity.rpc())
        {
            return Err(FenceRejection::StaleTarget);
        }
        self.boundary
            .check_and_poll(&self.identity, step(write_step), cx, &mut PollBridge(io))
            .map_err(reject)
    }
}

impl n::NativeApprovalPort for AcpNativeAdapter {
    fn reserve_permission_failure_cancelled(
        &self,
        source: &AcpSource,
        original: &PermissionRequest,
        binding: &n::PermissionFailureCancelBinding<ConnectionScope>,
    ) -> Result<AcpControlReservation, d::DispatchError> {
        if *binding.scope() != source.0.scope
            || original.scope().as_ref() != Some(binding.scope())
            || scalar(original.id()) != *binding.rpc()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = source
            .connection()?
            .reserve_permission_response(original, PermissionSelection::Cancelled)
            .map_err(error)?;
        Ok(AcpControlReservation {
            identity: n::ControlIdentity::PermissionCancelled {
                scope: actual.scope(),
                native_session: Arc::from(original.session().as_str()),
                rpc: scalar(original.id()),
            },
            actual,
        })
    }
    type PermissionChoice = super::super::PermissionChoice;
    type PermissionReplyReservation = AcpPermissionReplyReservation;

    fn permission_choices(
        &self,
        original: &PermissionRequest,
    ) -> Result<Vec<Self::PermissionChoice>, d::DispatchError> {
        if original.scope().is_none() {
            return Err(d::DispatchError::InvalidScope);
        }
        Ok(original.options().to_vec())
    }
    fn permission_choice_id<'a>(&self, choice: &'a Self::PermissionChoice) -> &'a str {
        choice.id().as_str()
    }
    fn permission_choice_kind<'a>(&self, choice: &'a Self::PermissionChoice) -> &'a str {
        choice.kind()
    }
    fn permission_request_digest(&self, original: &PermissionRequest) -> [u8; 32] {
        Sha256::digest(original.raw().bytes()).into()
    }
    fn permission_display(
        &self,
        original: &PermissionRequest,
    ) -> Result<n::NativePermissionDisplay<RawMessage>, d::DispatchError> {
        if original.scope().is_none() {
            return Err(d::DispatchError::InvalidScope);
        }
        Ok(n::NativePermissionDisplay {
            raw_request: original.raw().clone(),
            supported_subject: original.supported_subject(),
        })
    }
    fn inspect_permission(
        &self,
        source: &AcpSource,
        original: &PermissionRequest,
    ) -> Result<n::PermissionPendingSnapshot, d::DispatchError> {
        Ok(
            match source
                .connection()?
                .inspect_permission_request(original)
                .map_err(error)?
            {
                super::super::PermissionPendingSnapshot::Waiting => {
                    n::PermissionPendingSnapshot::Waiting
                }
                super::super::PermissionPendingSnapshot::Reserved => {
                    n::PermissionPendingSnapshot::Reserved
                }
                super::super::PermissionPendingSnapshot::SpentOrInvalid => {
                    n::PermissionPendingSnapshot::SpentOrInvalid
                }
            },
        )
    }
    fn reserve_permission_selected(
        &self,
        source: &AcpSource,
        original: &PermissionRequest,
        choice: &Self::PermissionChoice,
        binding: &n::PermissionSelectedBinding<ConnectionScope>,
    ) -> Result<AcpPermissionReplyReservation, d::DispatchError> {
        if *binding.scope() != source.0.scope
            || original.scope().as_ref() != Some(binding.scope())
            || scalar(original.id()) != *binding.rpc()
        {
            return Err(d::DispatchError::InvalidScope);
        }
        let ordinal = original
            .options()
            .iter()
            .position(|advertised| {
                advertised.id() == choice.id()
                    && advertised.kind() == choice.kind()
                    && advertised.raw().fragment() == choice.raw().fragment()
            })
            .ok_or_else(|| error(AcpError::InvalidOption))?;
        let option_digest = choice_fingerprint(
            self.permission_request_digest(original),
            ordinal,
            choice.id().as_str(),
        )
        .map_err(|_| d::DispatchError::LengthUnrepresentable)?;
        if &option_digest != binding.option_digest() {
            return Err(d::DispatchError::InvalidScope);
        }
        let actual = source
            .connection()?
            .reserve_permission_response(
                original,
                PermissionSelection::Selected(choice.id().clone()),
            )
            .map_err(error)?;
        let identity = n::PermissionReplyIdentity::from_actual(
            actual.scope(),
            scalar(original.id()),
            Arc::from(original.session().as_str()),
            option_digest,
        );
        Ok(AcpPermissionReplyReservation { actual, identity })
    }
    fn enqueue_permission_reply(
        &self,
        source: &AcpSource,
        reservation: AcpPermissionReplyReservation,
        frame: EncodedFrame,
        boundary: Arc<dyn n::PermissionReplyBoundary<ConnectionScope>>,
        deadline: Instant,
    ) -> Result<WriteTicket, n::EnqueueRejected<AcpPermissionReplyReservation, EncodedFrame>> {
        let connection = match source.connection() {
            Ok(connection) => connection,
            Err(error) => {
                return Err(n::EnqueueRejected {
                    error,
                    reservation,
                    frame,
                });
            }
        };
        let AcpPermissionReplyReservation { actual, identity } = reservation;
        let fence = Arc::new(ApprovalReplyFence {
            identity: identity.clone(),
            boundary,
        });
        connection
            .enqueue(actual, frame, fence, deadline)
            .map_err(|rejected| {
                let (rejected, actual, frame) = rejected.into_parts();
                n::EnqueueRejected {
                    error: error(rejected),
                    reservation: AcpPermissionReplyReservation { actual, identity },
                    frame,
                }
            })
    }
    fn cancel_reserved_permission_reply(
        &self,
        reservation: AcpPermissionReplyReservation,
    ) -> n::CancelledPermissionReplyReservation<ConnectionScope, WriteProbe> {
        let probe = reservation.actual.cancel();
        n::CancelledPermissionReplyReservation::from_actual(reservation.identity, probe)
    }
}
