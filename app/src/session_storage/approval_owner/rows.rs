//! Exact codecs for schema8 necessary approval facts. No raw native identifiers or content.
use super::{
    ActualReplyKind, ApprovalAssociationRecord, ApprovalAttemptRecord, ApprovalCommandRecord,
    ApprovalDecisionRecord, ApprovalId, ApprovalInvalidationReason, ApprovalInvalidationRecord,
    ApprovalRegistrationId, ApprovalRejection, ApprovalReplyId, ApprovalRequestRecord,
    ApprovalTerminalRecord, BoxFuture, KnownOptionKind, NecessaryWriteError, ReplyOrigin,
    SqliteConnection, StoredApprovalCommandState, StoredApprovalRejection, StoredAssociationKind,
    StoredRpcKind, TerminalReplyObservation, unavailable,
};
use crate::domain::{
    EntityId,
    native_dispatch::{ProtocolVersion, TransportReason},
};
use sqlx::{QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
fn corrupt() -> NecessaryWriteError {
    ApprovalRejection::Conflict.into()
}
fn value<T: for<'r> sqlx::Decode<'r, Sqlite> + sqlx::Type<Sqlite>>(
    row: &SqliteRow,
    column: &str,
) -> Result<T, NecessaryWriteError> {
    row.try_get(column).map_err(|_| corrupt())
}
fn entity(row: &SqliteRow, column: &str) -> Result<EntityId, NecessaryWriteError> {
    EntityId::decode(&value::<String>(row, column)?).map_err(|_| corrupt())
}
fn optional_entity(row: &SqliteRow, column: &str) -> Result<Option<EntityId>, NecessaryWriteError> {
    value::<Option<String>>(row, column)?
        .as_deref()
        .map(|s| EntityId::decode(s).map_err(|_| corrupt()))
        .transpose()
}
fn hash(row: &SqliteRow, column: &str) -> Result<[u8; 32], NecessaryWriteError> {
    value::<Vec<u8>>(row, column)?
        .try_into()
        .map_err(|_| corrupt())
}
fn optional_hash(row: &SqliteRow, column: &str) -> Result<Option<[u8; 32]>, NecessaryWriteError> {
    value::<Option<Vec<u8>>>(row, column)?
        .map(|v| v.try_into().map_err(|_| corrupt()))
        .transpose()
}
fn unsigned(row: &SqliteRow, column: &str) -> Result<u64, NecessaryWriteError> {
    let v: [u8; 8] = value::<Vec<u8>>(row, column)?
        .try_into()
        .map_err(|_| corrupt())?;
    Ok(u64::from_be_bytes(v))
}
fn optional_unsigned(row: &SqliteRow, column: &str) -> Result<Option<u64>, NecessaryWriteError> {
    value::<Option<Vec<u8>>>(row, column)?
        .map(|v| {
            let v: [u8; 8] = v.try_into().map_err(|_| corrupt())?;
            Ok(u64::from_be_bytes(v))
        })
        .transpose()
}
pub(super) trait ApprovalRow: Clone + PartialEq + Send + Sync {
    const TABLE: &'static str;
    const PRIMARY: &'static str;
    fn key(&self) -> String;
    fn insert(&self) -> QueryBuilder<'static, Sqlite>;
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError>;
    fn validate_relationships<'a>(
        &'a self,
        _: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), NecessaryWriteError>> {
        Box::pin(async { Ok(()) })
    }
}
const fn rpc_tag(v: StoredRpcKind) -> &'static str {
    match v {
        StoredRpcKind::Integer => "integer",
        StoredRpcKind::String => "string",
    }
}
fn rpc_decode(v: &str) -> Result<StoredRpcKind, NecessaryWriteError> {
    match v {
        "integer" => Ok(StoredRpcKind::Integer),
        "string" => Ok(StoredRpcKind::String),
        _ => Err(corrupt()),
    }
}
const fn assoc_tag(v: StoredAssociationKind) -> &'static str {
    match v {
        StoredAssociationKind::KnownPre => "known_pre",
        StoredAssociationKind::RegisteredSession => "registered_session",
    }
}
fn assoc_decode(v: &str) -> Result<StoredAssociationKind, NecessaryWriteError> {
    match v {
        "known_pre" => Ok(StoredAssociationKind::KnownPre),
        "registered_session" => Ok(StoredAssociationKind::RegisteredSession),
        _ => Err(corrupt()),
    }
}
const fn command_tag(v: StoredApprovalCommandState) -> &'static str {
    match v {
        StoredApprovalCommandState::Registered => "registered",
        StoredApprovalCommandState::Accepted => "accepted",
        StoredApprovalCommandState::Rejected => "rejected",
    }
}
fn command_decode(v: &str) -> Result<StoredApprovalCommandState, NecessaryWriteError> {
    match v {
        "registered" => Ok(StoredApprovalCommandState::Registered),
        "accepted" => Ok(StoredApprovalCommandState::Accepted),
        "rejected" => Ok(StoredApprovalCommandState::Rejected),
        _ => Err(corrupt()),
    }
}
const fn reply_tag(v: ActualReplyKind) -> &'static str {
    match v {
        ActualReplyKind::Selected => "selected",
        ActualReplyKind::Cancelled => "cancelled",
    }
}
fn reply_decode(v: &str) -> Result<ActualReplyKind, NecessaryWriteError> {
    match v {
        "selected" => Ok(ActualReplyKind::Selected),
        "cancelled" => Ok(ActualReplyKind::Cancelled),
        _ => Err(corrupt()),
    }
}
const fn origin_tag(v: ReplyOrigin) -> &'static str {
    match v {
        ReplyOrigin::UserDecision => "user_decision",
        ReplyOrigin::ProtocolCancel => "protocol_cancel",
        ReplyOrigin::ManagementFailure => "management_failure",
        ReplyOrigin::SourceClosing => "source_closing",
        ReplyOrigin::TargetClosing => "target_closing",
    }
}
fn origin_decode(v: &str) -> Result<ReplyOrigin, NecessaryWriteError> {
    match v {
        "user_decision" => Ok(ReplyOrigin::UserDecision),
        "protocol_cancel" => Ok(ReplyOrigin::ProtocolCancel),
        "management_failure" => Ok(ReplyOrigin::ManagementFailure),
        "source_closing" => Ok(ReplyOrigin::SourceClosing),
        "target_closing" => Ok(ReplyOrigin::TargetClosing),
        _ => Err(corrupt()),
    }
}
const fn rejection_tag(v: StoredApprovalRejection) -> &'static str {
    match v {
        StoredApprovalRejection::AlreadyDecided => "already_decided",
        StoredApprovalRejection::RequestInvalid => "request_invalid",
        StoredApprovalRejection::OptionInvalid => "option_invalid",
        StoredApprovalRejection::SourceInvalid => "source_invalid",
        StoredApprovalRejection::ActorInvalid => "actor_invalid",
    }
}
fn rejection_decode(v: &str) -> Result<StoredApprovalRejection, NecessaryWriteError> {
    match v {
        "already_decided" => Ok(StoredApprovalRejection::AlreadyDecided),
        "request_invalid" => Ok(StoredApprovalRejection::RequestInvalid),
        "option_invalid" => Ok(StoredApprovalRejection::OptionInvalid),
        "source_invalid" => Ok(StoredApprovalRejection::SourceInvalid),
        "actor_invalid" => Ok(StoredApprovalRejection::ActorInvalid),
        _ => Err(corrupt()),
    }
}
const fn invalidation_tag(v: ApprovalInvalidationReason) -> &'static str {
    match v {
        ApprovalInvalidationReason::RequestCancelled => "request_cancelled",
        ApprovalInvalidationReason::ResponseSpent => "response_spent",
        ApprovalInvalidationReason::ConnectionLost => "connection_lost",
        ApprovalInvalidationReason::SourceClosed => "source_closed",
        ApprovalInvalidationReason::TargetClosed => "target_closed",
        ApprovalInvalidationReason::VerifiedNativeRequestEnded => "verified_native_request_ended",
        ApprovalInvalidationReason::NodeRestart => "node_restart",
    }
}
fn invalidation_decode(v: &str) -> Result<ApprovalInvalidationReason, NecessaryWriteError> {
    match v {
        "request_cancelled" => Ok(ApprovalInvalidationReason::RequestCancelled),
        "response_spent" => Ok(ApprovalInvalidationReason::ResponseSpent),
        "connection_lost" => Ok(ApprovalInvalidationReason::ConnectionLost),
        "source_closed" => Ok(ApprovalInvalidationReason::SourceClosed),
        "target_closed" => Ok(ApprovalInvalidationReason::TargetClosed),
        "verified_native_request_ended" => {
            Ok(ApprovalInvalidationReason::VerifiedNativeRequestEnded)
        }
        "node_restart" => Ok(ApprovalInvalidationReason::NodeRestart),
        _ => Err(corrupt()),
    }
}
const fn treason_tag(v: TransportReason) -> &'static str {
    match v {
        TransportReason::Cancelled => "cancelled",
        TransportReason::Deadline => "deadline",
        TransportReason::FenceRejected => "fence_rejected",
        TransportReason::ConnectionClosed => "connection_closed",
        TransportReason::Abandoned => "abandoned",
        TransportReason::Io => "io",
        TransportReason::InvalidFence => "invalid_fence",
        TransportReason::ConnectionLost => "connection_lost",
        TransportReason::Restart => "restart",
    }
}
fn treason_decode(v: &str) -> Result<TransportReason, NecessaryWriteError> {
    match v {
        "cancelled" => Ok(TransportReason::Cancelled),
        "deadline" => Ok(TransportReason::Deadline),
        "fence_rejected" => Ok(TransportReason::FenceRejected),
        "connection_closed" => Ok(TransportReason::ConnectionClosed),
        "abandoned" => Ok(TransportReason::Abandoned),
        "io" => Ok(TransportReason::Io),
        "invalid_fence" => Ok(TransportReason::InvalidFence),
        "connection_lost" => Ok(TransportReason::ConnectionLost),
        "restart" => Ok(TransportReason::Restart),
        _ => Err(corrupt()),
    }
}
const fn terminal_state(v: TerminalReplyObservation) -> &'static str {
    match v {
        TerminalReplyObservation::Zero { .. } => "zero",
        TerminalReplyObservation::Written => "written",
        TerminalReplyObservation::Unknown { .. } => "unknown",
    }
}
const fn terminal_reason(v: TerminalReplyObservation) -> Option<&'static str> {
    match v {
        TerminalReplyObservation::Zero { reason } => Some(treason_tag(reason)),
        TerminalReplyObservation::Written => None,
        TerminalReplyObservation::Unknown { cause, .. } => Some(treason_tag(cause)),
    }
}
fn terminal_decode(row: &SqliteRow) -> Result<TerminalReplyObservation, NecessaryWriteError> {
    let state = value::<String>(row, "transport_state")?;
    let reason = value::<Option<String>>(row, "transport_reason")?;
    let accepted = unsigned(row, "accepted_length")?;
    match (state.as_str(), reason) {
        ("zero", Some(r)) if accepted == 0 => Ok(TerminalReplyObservation::Zero {
            reason: treason_decode(&r)?,
        }),
        ("written", None) => Ok(TerminalReplyObservation::Written),
        ("unknown", Some(r)) => Ok(TerminalReplyObservation::Unknown {
            accepted,
            cause: treason_decode(&r)?,
        }),
        _ => Err(corrupt()),
    }
}
impl ApprovalRow for ApprovalRequestRecord {
    const TABLE: &'static str = "node_approval_requests";
    const PRIMARY: &'static str = "approval_id";
    fn key(&self) -> String {
        self.approval.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_requests (approval_id,obligation_id,source_id,runtime_epoch,instance_id,connection_epoch,scope_binding_id,rpc_kind,rpc_digest,native_session_digest,request_digest,protocol_version,observed_sequence,observed_receipt,observed_at_ms,dispatch_id,context_kind) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.approval.encode());
            v.push_bind(self.obligation.encode());
            v.push_bind(self.source_id.encode());
            v.push_bind(self.runtime.encode());
            v.push_bind(self.instance.encode());
            v.push_bind(self.connection_epoch.encode());
            v.push_bind(self.scope_binding.encode());
            v.push_bind(rpc_tag(self.rpc_kind));
            v.push_bind(self.rpc_digest.to_vec());
            v.push_bind(self.native_session_digest.to_vec());
            v.push_bind(self.request_digest.to_vec());
            v.push_bind(match self.protocol {
                ProtocolVersion::V1 => 1_i64,
                ProtocolVersion::V2 => 2_i64,
            });
            v.push_bind(self.observed_sequence.to_be_bytes().to_vec());
            v.push_bind(self.observed_receipt.to_be_bytes().to_vec());
            v.push_bind(self.observed_at_ms);
            v.push_bind(self.verified_dispatch.map(EntityId::encode));
            v.push_bind(if self.verified_dispatch.is_some() {
                "verified_input"
            } else {
                "session_request"
            });
        }
        q.push(") ON CONFLICT(approval_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        let context = value::<String>(row, "context_kind")?;
        let verified = optional_entity(row, "dispatch_id")?;
        if !matches!(
            (context.as_str(), verified),
            ("session_request", None) | ("verified_input", Some(_))
        ) {
            return Err(corrupt());
        }
        Ok(Self {
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            obligation: entity(row, "obligation_id")?,
            source_id: entity(row, "source_id")?,
            runtime: entity(row, "runtime_epoch")?,
            instance: entity(row, "instance_id")?,
            connection_epoch: entity(row, "connection_epoch")?,
            scope_binding: entity(row, "scope_binding_id")?,
            rpc_kind: rpc_decode(&value::<String>(row, "rpc_kind")?)?,
            rpc_digest: hash(row, "rpc_digest")?,
            native_session_digest: hash(row, "native_session_digest")?,
            request_digest: hash(row, "request_digest")?,
            protocol: match value::<i64>(row, "protocol_version")? {
                1 => ProtocolVersion::V1,
                2 => ProtocolVersion::V2,
                _ => return Err(corrupt()),
            },
            observed_sequence: unsigned(row, "observed_sequence")?,
            observed_receipt: unsigned(row, "observed_receipt")?,
            observed_at_ms: value(row, "observed_at_ms")?,
            verified_dispatch: optional_entity(row, "dispatch_id")?,
        })
    }
}
impl ApprovalRow for ApprovalAssociationRecord {
    const TABLE: &'static str = "node_approval_associations";
    const PRIMARY: &'static str = "approval_id";
    fn key(&self) -> String {
        self.approval.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_associations (approval_id,target_id,association_kind,load_epoch,owner_id,association_fingerprint,associated_at_ms) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.approval.encode());
            v.push_bind(self.target.encode());
            v.push_bind(assoc_tag(self.kind));
            v.push_bind(self.load_epoch.map(EntityId::encode));
            v.push_bind(self.owner.map(EntityId::encode));
            v.push_bind(self.fingerprint.to_vec());
            v.push_bind(self.associated_at_ms);
        }
        q.push(") ON CONFLICT(approval_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            target: entity(row, "target_id")?,
            kind: assoc_decode(&value::<String>(row, "association_kind")?)?,
            load_epoch: optional_entity(row, "load_epoch")?,
            owner: optional_entity(row, "owner_id")?,
            fingerprint: hash(row, "association_fingerprint")?,
            associated_at_ms: value(row, "associated_at_ms")?,
        })
    }
}
impl ApprovalRow for ApprovalCommandRecord {
    const TABLE: &'static str = "node_approval_commands";
    const PRIMARY: &'static str = "registration_id";
    fn key(&self) -> String {
        self.registration.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_commands (registration_id,approval_id,target_id,actor_id,registration_auth_generation,option_digest,intent_fingerprint,runtime_epoch,registered_at_ms,state,operation_id,admission_auth_generation,admitted_at_ms,rejection_reason) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.registration.encode());
            v.push_bind(self.approval.encode());
            v.push_bind(self.target.encode());
            v.push_bind(self.actor.encode());
            v.push_bind(self.registration_auth_generation);
            v.push_bind(self.option_digest.to_vec());
            v.push_bind(self.intent_fingerprint.to_vec());
            v.push_bind(self.runtime.encode());
            v.push_bind(self.registered_at_ms);
            v.push_bind(command_tag(self.state));
            v.push_bind(self.operation.map(EntityId::encode));
            v.push_bind(self.admission_auth_generation);
            v.push_bind(self.admitted_at_ms);
            v.push_bind(self.rejection.map(rejection_tag));
        }
        q.push(") ON CONFLICT(registration_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            registration: ApprovalRegistrationId::from_entity(entity(row, "registration_id")?),
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            target: entity(row, "target_id")?,
            actor: entity(row, "actor_id")?,
            registration_auth_generation: value(row, "registration_auth_generation")?,
            option_digest: hash(row, "option_digest")?,
            intent_fingerprint: hash(row, "intent_fingerprint")?,
            runtime: entity(row, "runtime_epoch")?,
            registered_at_ms: value(row, "registered_at_ms")?,
            state: command_decode(&value::<String>(row, "state")?)?,
            operation: optional_entity(row, "operation_id")?,
            admission_auth_generation: value(row, "admission_auth_generation")?,
            admitted_at_ms: value(row, "admitted_at_ms")?,
            rejection: value::<Option<String>>(row, "rejection_reason")?
                .as_deref()
                .map(rejection_decode)
                .transpose()?,
        })
    }
}
impl ApprovalRow for ApprovalDecisionRecord {
    const TABLE: &'static str = "node_approval_decisions";
    const PRIMARY: &'static str = "approval_id";
    fn key(&self) -> String {
        self.approval.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_decisions (approval_id,registration_id,operation_id,actor_id,admission_auth_generation,option_digest,known_option_kind,authorization_fingerprint,decision_fingerprint,accepted_at_ms) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.approval.encode());
            v.push_bind(self.registration.encode());
            v.push_bind(self.operation.encode());
            v.push_bind(self.actor.encode());
            v.push_bind(self.admission_auth_generation);
            v.push_bind(self.option_digest.to_vec());
            v.push_bind(self.known_kind.map(KnownOptionKind::label));
            v.push_bind(self.authorization_fingerprint.to_vec());
            v.push_bind(self.fingerprint.to_vec());
            v.push_bind(self.accepted_at_ms);
        }
        q.push(") ON CONFLICT(approval_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            registration: ApprovalRegistrationId::from_entity(entity(row, "registration_id")?),
            operation: entity(row, "operation_id")?,
            actor: entity(row, "actor_id")?,
            admission_auth_generation: value(row, "admission_auth_generation")?,
            option_digest: hash(row, "option_digest")?,
            known_kind: value::<Option<String>>(row, "known_option_kind")?
                .map(|s| KnownOptionKind::classify(&s).ok_or_else(corrupt))
                .transpose()?,
            authorization_fingerprint: hash(row, "authorization_fingerprint")?,
            fingerprint: hash(row, "decision_fingerprint")?,
            accepted_at_ms: value(row, "accepted_at_ms")?,
        })
    }
}
impl ApprovalRow for ApprovalAttemptRecord {
    fn validate_relationships<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), NecessaryWriteError>> {
        Box::pin(async move {
            let invalid: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM main.node_approval_invalidations WHERE approval_id=?",
            )
            .bind(self.approval.encode())
            .fetch_one(&mut *c)
            .await
            .map_err(|_| unavailable())?;
            if invalid != 0 {
                return Err(ApprovalRejection::InvalidRequest.into());
            }
            let previous = super::list::<Self>(c, self.approval).await?;
            if let Some(latest) = previous.iter().max_by_key(|row| row.serial) {
                if latest.serial.checked_add(1) != Some(self.serial) {
                    return Err(corrupt());
                }
                let facts = super::list::<ApprovalTerminalRecord>(c, self.approval).await?;
                if !facts.iter().any(|row| {
                    row.response == latest.response
                        && matches!(row.observation, TerminalReplyObservation::Zero { .. })
                        && row.accepted_length == 0
                }) {
                    return Err(corrupt());
                }
            } else if self.serial != 0 {
                return Err(corrupt());
            }
            match self.kind {
                ActualReplyKind::Selected => {
                    let decisions = super::list::<ApprovalDecisionRecord>(c, self.approval).await?;
                    if !decisions.iter().any(|row| {
                        Some(row.operation) == self.decision_operation
                            && Some(row.option_digest) == self.option_digest
                    }) {
                        return Err(corrupt());
                    }
                }
                ActualReplyKind::Cancelled => {
                    if self.decision_operation.is_some() || self.option_digest.is_some() {
                        return Err(corrupt());
                    }
                }
            }
            Ok(())
        })
    }

    const TABLE: &'static str = "node_approval_reply_attempts";
    const PRIMARY: &'static str = "response_id";
    fn key(&self) -> String {
        self.response.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_reply_attempts (response_id,approval_id,decision_operation_id,attempt_serial,response_kind,origin,option_digest,frame_length,reserved_fingerprint,observed_at_ms,recorded_at_ms) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.response.encode());
            v.push_bind(self.approval.encode());
            v.push_bind(self.decision_operation.map(EntityId::encode));
            v.push_bind(self.serial.to_be_bytes().to_vec());
            v.push_bind(reply_tag(self.kind));
            v.push_bind(origin_tag(self.origin));
            v.push_bind(self.option_digest.map(|v| v.to_vec()));
            v.push_bind(self.frame_length.to_be_bytes().to_vec());
            v.push_bind(self.reserved_fingerprint.to_vec());
            v.push_bind(self.observed_at_ms);
            v.push_bind(self.recorded_at_ms);
        }
        q.push(") ON CONFLICT(response_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            response: ApprovalReplyId::from_entity(entity(row, "response_id")?),
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            decision_operation: optional_entity(row, "decision_operation_id")?,
            serial: unsigned(row, "attempt_serial")?,
            kind: reply_decode(&value::<String>(row, "response_kind")?)?,
            origin: origin_decode(&value::<String>(row, "origin")?)?,
            option_digest: optional_hash(row, "option_digest")?,
            frame_length: unsigned(row, "frame_length")?,
            reserved_fingerprint: hash(row, "reserved_fingerprint")?,
            observed_at_ms: value(row, "observed_at_ms")?,
            recorded_at_ms: value(row, "recorded_at_ms")?,
        })
    }
}
impl ApprovalRow for ApprovalTerminalRecord {
    fn validate_relationships<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
    ) -> BoxFuture<'a, Result<(), NecessaryWriteError>> {
        Box::pin(async move {
            let attempts = super::list::<ApprovalAttemptRecord>(c, self.approval).await?;
            let arm = attempts
                .iter()
                .find(|row| row.response == self.response)
                .ok_or_else(corrupt)?;
            if self.accepted_length > arm.frame_length {
                return Err(corrupt());
            }
            match self.observation {
                TerminalReplyObservation::Written if self.accepted_length != arm.frame_length => {
                    return Err(corrupt());
                }
                TerminalReplyObservation::Zero { .. } if self.accepted_length != 0 => {
                    return Err(corrupt());
                }
                _ => {}
            }
            Ok(())
        })
    }

    const TABLE: &'static str = "node_approval_reply_facts";
    const PRIMARY: &'static str = "response_id";
    fn key(&self) -> String {
        self.response.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_reply_facts (response_id,approval_id,transport_state,accepted_length,transport_reason,terminal_fingerprint,observed_at_ms,recorded_at_ms) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.response.encode());
            v.push_bind(self.approval.encode());
            v.push_bind(terminal_state(self.observation));
            v.push_bind(self.accepted_length.to_be_bytes().to_vec());
            v.push_bind(terminal_reason(self.observation));
            v.push_bind(self.fingerprint.to_vec());
            v.push_bind(self.observed_at_ms);
            v.push_bind(self.recorded_at_ms);
        }
        q.push(") ON CONFLICT(response_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            response: ApprovalReplyId::from_entity(entity(row, "response_id")?),
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            accepted_length: unsigned(row, "accepted_length")?,
            fingerprint: hash(row, "terminal_fingerprint")?,
            observed_at_ms: value(row, "observed_at_ms")?,
            recorded_at_ms: value(row, "recorded_at_ms")?,
            observation: terminal_decode(row)?,
        })
    }
}
impl ApprovalRow for ApprovalInvalidationRecord {
    const TABLE: &'static str = "node_approval_invalidations";
    const PRIMARY: &'static str = "invalidation_id";
    fn key(&self) -> String {
        self.id.encode()
    }
    fn insert(&self) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(
            "INSERT INTO main.node_approval_invalidations (invalidation_id,approval_id,reason,evidence_fingerprint,observed_sequence,observed_at_ms,recorded_at_ms) VALUES (",
        );
        {
            let mut v = q.separated(",");
            v.push_bind(self.id.encode());
            v.push_bind(self.approval.encode());
            v.push_bind(invalidation_tag(self.reason));
            v.push_bind(self.fingerprint.to_vec());
            v.push_bind(self.observed_sequence.map(|v| v.to_be_bytes().to_vec()));
            v.push_bind(self.observed_at_ms);
            v.push_bind(self.recorded_at_ms);
        }
        q.push(") ON CONFLICT(invalidation_id) DO NOTHING");
        q
    }
    fn decode(row: &SqliteRow) -> Result<Self, NecessaryWriteError> {
        Ok(Self {
            id: entity(row, "invalidation_id")?,
            approval: ApprovalId::from_entity(entity(row, "approval_id")?),
            reason: invalidation_decode(&value::<String>(row, "reason")?)?,
            fingerprint: hash(row, "evidence_fingerprint")?,
            observed_sequence: optional_unsigned(row, "observed_sequence")?,
            observed_at_ms: value(row, "observed_at_ms")?,
            recorded_at_ms: value(row, "recorded_at_ms")?,
        })
    }
}
