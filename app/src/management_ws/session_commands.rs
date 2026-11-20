//! Authenticated control metadata only; original queue tasks own all query work.
#![allow(
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, AuthenticatedClient, CLOSE_AUTHENTICATION, CLOSE_INTERNAL, CLOSE_REVOKED, CommandFrame,
    CommandOutcome, Id128, ManagementState, Outbound, SocketAttachment, Value, close,
    management_error, management_success, session_protocol, still_valid,
};
#[cfg(all(test, windows))]
use super::{UploadCommandAction, UploadCommandBoundary, pause_upload_command};
use crate::{
    application::queue_owner::{QueueReadFailure, QueueReadReceipt},
    domain::request_registry::{
        QueueOutcome, RegistrationFact, RegistrationId, RegistryError, RequestFact,
    },
};
#[cfg(windows)]
async fn queue_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::node::queue_management::NodeQueueError,
) -> CommandOutcome {
    match error {
        crate::node::queue_management::NodeQueueError::Registry(error) => {
            failure(state, actor, request, QueueReadFailure::Registry(error)).await
        }
        crate::node::queue_management::NodeQueueError::Upload(_) => {
            CommandOutcome::Result(management_error(request, "invalid_request"))
        }
        crate::node::queue_management::NodeQueueError::SocketClosed => {
            CommandOutcome::Result(management_error(request, "read_failed"))
        }
        crate::node::queue_management::NodeQueueError::NodeStopping => {
            CommandOutcome::Result(management_error(request, "node_stopping"))
        }
    }
}
use std::future::Future;
use tokio::task::JoinSet;

pub(super) struct PendingReads {
    tasks: JoinSet<CommandOutcome>,
}

impl PendingReads {
    pub(super) fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub(super) fn push<F>(&mut self, future: F)
    where
        F: Future<Output = CommandOutcome> + Send + 'static,
    {
        self.tasks.spawn(future);
    }

    pub(super) async fn next(&mut self) -> Option<CommandOutcome> {
        loop {
            match self.tasks.join_next().await {
                Some(Ok(outcome)) => return Some(outcome),
                Some(Err(error)) => {
                    if !error.is_cancelled() {
                        return Some(CommandOutcome::Close(close(
                            CLOSE_INTERNAL,
                            "internal error",
                        )));
                    }
                }
                None => return None,
            }
        }
    }

    pub(super) async fn shutdown(&mut self) {
        self.tasks.shutdown().await;
    }
}

const fn allocation_code(error: QueueReadFailure) -> Option<&'static str> {
    match error {
        QueueReadFailure::AllocationFailed => Some("allocation_failed"),
        QueueReadFailure::Registry(_) | QueueReadFailure::WorkerStoppedUnconfirmed => None,
    }
}

const fn id(value: Id128) -> crate::domain::EntityId {
    crate::domain::EntityId::from_bytes(*value.as_bytes())
}
#[cfg(windows)]
const fn approval_error_code(
    error: crate::domain::approval_owner::ApprovalRejection,
) -> &'static str {
    use crate::domain::approval_owner::ApprovalRejection;
    match error {
        ApprovalRejection::InvalidActor => "revoked",
        ApprovalRejection::SessionNotControlled => "session_not_controlled",
        ApprovalRejection::SessionTreeUnverified => "session_tree_unverified",
        ApprovalRejection::UnknownRegistration => "invalid_registration",
        ApprovalRejection::Conflict | ApprovalRejection::AlreadyDecided => "request_conflict",
        ApprovalRejection::InvalidRequest
        | ApprovalRejection::UnassociatedTarget
        | ApprovalRejection::InvalidChoice => "invalid_approval",
        ApprovalRejection::LengthUnrepresentable => "invalid_request",
        ApprovalRejection::Stopped => "node_stopping",
        ApprovalRejection::NecessaryStorageUnavailable => "necessary_storage_unavailable",
    }
}
#[cfg(windows)]
const fn approval_write_error_code(
    error: crate::domain::approval_owner::NecessaryWriteError,
) -> &'static str {
    use crate::domain::approval_owner::NecessaryWriteError;
    match error {
        NecessaryWriteError::Rejected(error) => approval_error_code(error),
        NecessaryWriteError::CommitUnconfirmed
        | NecessaryWriteError::SqlCloseUnconfirmed {
            commit_confirmed: _,
        } => "commit_unconfirmed",
    }
}
#[cfg(windows)]
async fn approval_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::domain::approval_owner::ApprovalRejection,
) -> CommandOutcome {
    if error == crate::domain::approval_owner::ApprovalRejection::InvalidActor {
        return failure(
            state,
            actor,
            request,
            QueueReadFailure::Registry(RegistryError::InvalidActor),
        )
        .await;
    }
    CommandOutcome::Result(management_error(request, approval_error_code(error)))
}
#[cfg(windows)]
async fn approval_write_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::domain::approval_owner::NecessaryWriteError,
) -> CommandOutcome {
    if let crate::domain::approval_owner::NecessaryWriteError::Rejected(
        rejection @ crate::domain::approval_owner::ApprovalRejection::InvalidActor,
    ) = error
    {
        return approval_failure(state, actor, request, rejection).await;
    }
    CommandOutcome::Result(management_error(request, approval_write_error_code(error)))
}
#[cfg(windows)]
const fn release_error_code(error: crate::node::NativeReleaseError) -> &'static str {
    use crate::node::NativeReleaseError;
    match error {
        NativeReleaseError::Registry(error) => match error {
            RegistryError::InvalidActor => "revoked",
            RegistryError::InvalidTarget => "invalid_target",
            RegistryError::SessionNotControlled => "session_not_controlled",
            RegistryError::SessionTreeUnverified => "session_tree_unverified",
            RegistryError::Conflict | RegistryError::RuntimeAlreadyStarted => "request_conflict",
            RegistryError::UnknownRegistration => "invalid_release",
            RegistryError::StorageUnavailable => "necessary_storage_unavailable",
            RegistryError::CommitUnconfirmed => "commit_unconfirmed",
            RegistryError::NodeStopping => "node_stopping",
            RegistryError::NodeStarting => "product_not_ready",
            _ => "release_failed",
        },
        NativeReleaseError::Conflict => "request_conflict",
        NativeReleaseError::Native(error) => match error {
            crate::domain::native_dispatch::DispatchError::SessionCapacityFull => {
                "session_capacity_full"
            }
            crate::domain::native_dispatch::DispatchError::SessionTreeUnverified => {
                "session_tree_unverified"
            }
            crate::domain::native_dispatch::DispatchError::NodeStopping
            | crate::domain::native_dispatch::DispatchError::Stopped => "node_stopping",
            crate::domain::native_dispatch::DispatchError::InvalidTarget
            | crate::domain::native_dispatch::DispatchError::InvalidResource
            | crate::domain::native_dispatch::DispatchError::InvalidSource
            | crate::domain::native_dispatch::DispatchError::InvalidScope => "invalid_target",
            crate::domain::native_dispatch::DispatchError::Conflict
            | crate::domain::native_dispatch::DispatchError::RevisionChanged
            | crate::domain::native_dispatch::DispatchError::StaleSelection => "request_conflict",
            crate::domain::native_dispatch::DispatchError::StorageUnavailable
            | crate::domain::native_dispatch::DispatchError::CommitUnconfirmed
            | crate::domain::native_dispatch::DispatchError::SqlCloseUnconfirmed { .. } => {
                "necessary_storage_unavailable"
            }
            crate::domain::native_dispatch::DispatchError::ResourceUnconfirmed => {
                "release_unconfirmed"
            }
            crate::domain::native_dispatch::DispatchError::ProtocolFault(_)
            | crate::domain::native_dispatch::DispatchError::ProtocolUnavailable
            | crate::domain::native_dispatch::DispatchError::ProtocolRejected => "protocol_failure",
            _ => "release_failed",
        },
    }
}
#[cfg(windows)]
async fn release_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::node::NativeReleaseError,
) -> CommandOutcome {
    if matches!(
        error,
        crate::node::NativeReleaseError::Registry(RegistryError::InvalidActor)
    ) {
        return failure(
            state,
            actor,
            request,
            QueueReadFailure::Registry(RegistryError::InvalidActor),
        )
        .await;
    }
    CommandOutcome::Result(management_error(request, release_error_code(error)))
}
#[cfg(windows)]
fn approval_status_result(value: &crate::domain::approval_owner::ApprovalStatus) -> Value {
    use crate::domain::approval_owner::{
        AssociationDisplay, RequestValidityDisplay, SourceCloseDisplay,
    };
    let validity = match value.validity {
        RequestValidityDisplay::CurrentPending => "current_pending",
        RequestValidityDisplay::ResponseReserved => "response_reserved",
        RequestValidityDisplay::Spent => "spent",
        RequestValidityDisplay::Invalid => "invalid",
        RequestValidityDisplay::Unknown => "unknown",
    };
    let association = match &value.association {
        AssociationDisplay::Unassociated => serde_json::json!({"kind":"unassociated"}),
        AssociationDisplay::KnownPre {
            target,
            load_epoch,
            owner,
        } => serde_json::json!({
            "kind":"known_pre",
            "targetId":target.encode(),
            "loadEpoch":load_epoch.encode(),
            "ownerId":owner.encode(),
        }),
        AssociationDisplay::RegisteredSession { target } => {
            serde_json::json!({"kind":"registered_session","targetId":target.encode()})
        }
    };
    let decision = value.decision.as_ref().map(|decision| {
        serde_json::json!({
            "registrationId":decision.registration.encode(),
            "operationId":decision.operation.encode(),
            "actorId":decision.actor.encode(),
            "admissionAuthGeneration":decision.admission_auth_generation.to_string(),
            "choiceId":decision.selected_choice.map(crate::domain::approval_owner::ChoiceId::encode),
            "knownKind":decision.known_kind.map(crate::domain::approval_owner::KnownOptionKind::label),
            "acceptedAtMs":decision.accepted_at_ms.to_string(),
        })
    });
    let delivery = match &value.delivery {
        crate::domain::approval_owner::DeliverySummary::NotAttempted => "not_attempted",
        crate::domain::approval_owner::DeliverySummary::InProgress { .. } => "in_progress",
        crate::domain::approval_owner::DeliverySummary::Terminal { .. } => "terminal",
        crate::domain::approval_owner::DeliverySummary::Suppressed { .. } => "suppressed",
    };
    let source_close = match value.source_close {
        SourceCloseDisplay::Open => "open",
        SourceCloseDisplay::Closing => "closing",
        SourceCloseDisplay::PhysicalUnconfirmed => "physical_unconfirmed",
        SourceCloseDisplay::MetadataUnconfirmed => "metadata_unconfirmed",
        SourceCloseDisplay::Confirmed => "confirmed",
    };
    serde_json::json!({
        "approvalId":value.approval.encode(),
        "changeSequence":value.change_sequence.to_string(),
        "validity":validity,
        "association":association,
        "decision":decision,
        "delivery":delivery,
        "sourceClose":source_close,
        "necessaryFactsMissing":value.necessary_facts_missing,
        "reconciliationRunning":value.owned_reconciliation_running,
    })
}
#[cfg(windows)]
fn approval_view_result(
    value: &crate::application::native_dispatch::ApprovalDisplay<crate::adapters::acp::RawMessage>,
) -> Value {
    use crate::{
        application::native_dispatch::ApprovalDisplayContent,
        domain::approval_owner::{ApprovalContextDisplay, DisplayLimitation},
    };
    let source = value.source();
    let context = match value.context() {
        ApprovalContextDisplay::SessionRequest => {
            serde_json::json!({"kind":"session_request"})
        }
        ApprovalContextDisplay::VerifiedInput { dispatch } => {
            serde_json::json!({"kind":"verified_input","dispatchId":dispatch.encode()})
        }
    };
    let choices = value
        .choices()
        .iter()
        .map(|choice| {
            serde_json::json!({
                "choiceId":choice.id().encode(),
                "ordinal":choice.ordinal().to_string(),
                "originalOptionId":choice.native_option_id(),
                "originalKind":choice.kind_text(),
                "selectable":choice.selectable(),
            })
        })
        .collect::<Vec<_>>();
    let content = match value.content() {
        ApprovalDisplayContent::LiveOriginal(message) => {
            let raw = std::str::from_utf8(message.bytes()).ok().map(str::to_owned);
            match raw {
                Some(raw) => serde_json::json!({"availability":"available","rawJson":raw}),
                None => serde_json::json!({"availability":"unavailable"}),
            }
        }
        ApprovalDisplayContent::Unavailable => {
            serde_json::json!({"availability":"unavailable"})
        }
    };
    let limitations = value
        .limitations()
        .iter()
        .map(|limitation| match limitation {
            DisplayLimitation::UnknownSubject => "unknown_subject",
            DisplayLimitation::UnknownOptionKind => "unknown_option_kind",
            DisplayLimitation::TargetUnassociated => "target_unassociated",
            DisplayLimitation::MalformedAdvertisement => "malformed_advertisement",
            DisplayLimitation::OriginalUnavailable => "original_unavailable",
            DisplayLimitation::RequestInvalid => "request_invalid",
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "approvalId":value.id().encode(),
        "source":{
            "sourceId":source.source_id.encode(),
            "runtimeId":source.runtime.encode(),
            "instanceId":source.instance.encode(),
            "connectionEpoch":source.connection_epoch.encode(),
            "protocolVersion":source.protocol_version.number().to_string(),
        },
        "context":context,
        "choices":choices,
        "content":content,
        "limitations":limitations,
        "status":approval_status_result(value.status()),
    })
}
#[cfg(windows)]
fn release_registered_result(
    operation: crate::domain::EntityId,
    target: crate::domain::EntityId,
) -> Value {
    serde_json::json!({
        "releaseOperationId":operation.encode(),
        "targetId":target.encode(),
        "state":"registered",
    })
}
#[cfg(windows)]
fn release_snapshot_result(value: &crate::node::NativeReleaseSnapshot) -> Value {
    use crate::node::NativeReleasePhase;
    let state = match value.phase {
        NativeReleasePhase::Registered => "registered",
        NativeReleasePhase::Releasing => "releasing",
        NativeReleasePhase::Released => "released",
        NativeReleasePhase::Failed => "failed",
    };
    serde_json::json!({
        "releaseOperationId":value.operation.encode(),
        "targetId":value.target.encode(),
        "state":state,
        "error":value.error,
    })
}
#[cfg(windows)]
fn upload_failure(request: Id128, error: crate::node::UploadRoutingError) -> CommandOutcome {
    match error {
        crate::node::UploadRoutingError::Business(error) => {
            CommandOutcome::Result(management_error(request, error.code()))
        }
        crate::node::UploadRoutingError::Revoked => {
            CommandOutcome::Close(close(CLOSE_REVOKED, "revoked"))
        }
        crate::node::UploadRoutingError::StorageUnavailable => {
            CommandOutcome::Close(close(CLOSE_INTERNAL, "storage unavailable"))
        }
    }
}
#[cfg(windows)]
fn upload_error(
    request: Id128,
    error: crate::management_ws::input_upload::UploadError,
) -> CommandOutcome {
    CommandOutcome::Result(management_error(request, error.code()))
}
#[cfg(windows)]
const fn catalog_error_code(error: crate::domain::native_catalog::CatalogError) -> &'static str {
    use crate::domain::native_catalog::CatalogError;
    match error {
        CatalogError::InvalidActor => "revoked",
        CatalogError::InvalidSource => "invalid_source",
        CatalogError::InvalidTarget => "invalid_target",
        CatalogError::UnknownRequest => "unknown_request",
        CatalogError::InvalidConfiguration => "invalid_configuration",
        CatalogError::Conflict => "request_conflict",
        CatalogError::CorruptState | CatalogError::Cancelled => "read_failed",
        CatalogError::RandomUnavailable => "random_unavailable",
        CatalogError::Allocation => "allocation_failed",
        CatalogError::ProtectionUnavailable => "protection_unavailable",
        CatalogError::StorageUnavailable => "necessary_storage_unavailable",
        CatalogError::NodeStopping => "node_stopping",
        CatalogError::CommitUnconfirmed | CatalogError::SqlCloseUnconfirmed { .. } => {
            "commit_unconfirmed"
        }
    }
}
#[cfg(windows)]
const fn catalog_preparation_error_code(
    error: &crate::application::catalog_preparation::CatalogPreparationFailure,
) -> &'static str {
    use crate::application::catalog_preparation::{CatalogBufferError, CatalogPreparationFailure};
    match error {
        CatalogPreparationFailure::Upload(CatalogBufferError::InvalidState) => {
            "invalid_upload_state"
        }
        CatalogPreparationFailure::Upload(CatalogBufferError::Conflict) => "upload_conflict",
        CatalogPreparationFailure::Upload(CatalogBufferError::BodyUnavailable) => {
            "body_unavailable"
        }
        CatalogPreparationFailure::Upload(CatalogBufferError::Allocation) => "allocation_failed",
        CatalogPreparationFailure::Upload(CatalogBufferError::NodeStopping) => "node_stopping",
        CatalogPreparationFailure::Upload(CatalogBufferError::StorageUnavailable) => {
            "necessary_storage_unavailable"
        }
        CatalogPreparationFailure::Upload(CatalogBufferError::InvalidDocument) => "invalid_request",
        CatalogPreparationFailure::Catalog(error) => catalog_error_code(error.kind()),
        CatalogPreparationFailure::WorkerUnconfirmed(_) => "commit_unconfirmed",
    }
}
#[cfg(windows)]
async fn catalog_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::domain::native_catalog::CatalogFailure,
) -> CommandOutcome {
    if error.kind() == crate::domain::native_catalog::CatalogError::InvalidActor {
        return CommandOutcome::Close(match still_valid(state, actor).await {
            Ok(false) => close(CLOSE_REVOKED, "revoked"),
            Ok(true) => close(CLOSE_AUTHENTICATION, "authentication failed"),
            Err(()) => close(CLOSE_INTERNAL, "storage unavailable"),
        });
    }
    CommandOutcome::Result(management_error(request, catalog_error_code(error.kind())))
}
#[cfg(windows)]
async fn catalog_command_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::node::CatalogCommandFailure,
) -> CommandOutcome {
    match error {
        crate::node::CatalogCommandFailure::Catalog(error) => {
            catalog_failure(state, actor, request, error).await
        }
        crate::node::CatalogCommandFailure::WorkerUnconfirmed { .. } => {
            CommandOutcome::Result(management_error(request, "commit_unconfirmed"))
        }
    }
}
#[cfg(windows)]
const fn upload_phase(phase: crate::node::UploadPhase) -> &'static str {
    match phase {
        crate::node::UploadPhase::Receiving => "receiving",
        crate::node::UploadPhase::Preparing => "preparing",
        crate::node::UploadPhase::Ready => "ready",
        crate::node::UploadPhase::Failed => "failed",
    }
}
#[cfg(windows)]
const fn read_reason(
    reason: session_protocol::ViewOpenReason,
) -> crate::application::shared_views::budget::ReadReason {
    use crate::{
        application::shared_views::budget::ReadReason,
        management_ws::session_protocol::ViewOpenReason,
    };
    match reason {
        ViewOpenReason::FirstOpen => ReadReason::FirstOpen,
        ViewOpenReason::ExplicitRefresh => ReadReason::ExplicitRefresh,
        ViewOpenReason::Reopen => ReadReason::Reopen,
        ViewOpenReason::Reconnect => ReadReason::Reconnect,
        ViewOpenReason::Reacquire => ReadReason::Reacquire,
    }
}
#[cfg(windows)]
const fn view_error_code(error: crate::domain::shared_view::ViewError) -> &'static str {
    use crate::domain::shared_view::ViewError;
    match error {
        ViewError::Unauthenticated | ViewError::Revoked => "revoked",
        ViewError::WrongSocket | ViewError::InvalidClient => "invalid_client",
        ViewError::InvalidSource => "invalid_source",
        ViewError::InvalidConfiguration => "invalid_configuration",
        ViewError::Unsupported => "platform_unimplemented",
        ViewError::UnsupportedCapability => "unsupported_capability",
        ViewError::AlreadyAttached => "already_attached",
        ViewError::CaptureUnavailable => "view_unavailable",
        ViewError::InvalidStage => "invalid_stage",
        ViewError::SourceReplaced => "source_replaced",
        ViewError::StreamSealed => "stream_sealed",
        ViewError::ProtocolFailure => "protocol_failure",
        ViewError::NativeFailure(error) => native_view_error_code(error),
        ViewError::InvalidCredit => "invalid_credit",
        ViewError::QueueRejected => "queue_rejected",
        ViewError::TransportFailure => "transport_failure",
        ViewError::ShutdownUnconfirmed => "shutdown_unconfirmed",
        ViewError::InvalidTarget => "invalid_target",
        ViewError::NodeStopping => "node_stopping",
        ViewError::InvalidScope => "invalid_scope",
        ViewError::CaptureGap => "capture_gap",
        ViewError::AllocationFailure => "allocation_failed",
        ViewError::RandomUnavailable => "random_unavailable",
        ViewError::SequenceExhausted => "sequence_exhausted",
        ViewError::StaleCursor => "stale_cursor",
        ViewError::InvalidCursor => "invalid_cursor",
        ViewError::SourceReleased => "source_released",
        ViewError::ProjectionFailure => "projection_failure",
        ViewError::ReadFailure => "read_failed",
        ViewError::LengthUnrepresentable => "length_unrepresentable",
    }
}
#[cfg(windows)]
const fn native_view_error_code(
    error: crate::domain::native_dispatch::DispatchError,
) -> &'static str {
    use crate::domain::native_dispatch::DispatchError;
    match error {
        DispatchError::SessionCapacityFull => "session_capacity_full",
        DispatchError::SessionTreeUnverified => "session_tree_unverified",
        DispatchError::ProtocolFault(_)
        | DispatchError::ProtocolUnavailable
        | DispatchError::ProtocolRejected => "protocol_failure",
        DispatchError::InvalidConfiguration => "invalid_configuration",
        DispatchError::InvalidTarget => "invalid_target",
        DispatchError::InvalidSource => "invalid_source",
        DispatchError::InvalidScope => "invalid_scope",
        DispatchError::NodeStopping => "node_stopping",
        DispatchError::Stopped => "source_released",
        DispatchError::LengthUnrepresentable => "length_unrepresentable",
        DispatchError::RuntimeAlreadyStarted
        | DispatchError::StorageUnavailable
        | DispatchError::CommitUnconfirmed
        | DispatchError::SqlCloseUnconfirmed { .. }
        | DispatchError::CorruptState
        | DispatchError::Conflict
        | DispatchError::InvalidResource
        | DispatchError::InvalidPromptReservation
        | DispatchError::StaleSelection
        | DispatchError::RevisionChanged
        | DispatchError::NotReady
        | DispatchError::RequiredStateUnavailable
        | DispatchError::SequenceExhausted
        | DispatchError::EnqueueRejected
        | DispatchError::ObservationOrder
        | DispatchError::ObservationUnapplied
        | DispatchError::ObservationDropped
        | DispatchError::ResourceUnconfirmed
        | DispatchError::SourcePlatform { .. } => "read_failed",
    }
}
#[cfg(windows)]
async fn view_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::domain::shared_view::ViewError,
) -> CommandOutcome {
    if matches!(
        error,
        crate::domain::shared_view::ViewError::Unauthenticated
            | crate::domain::shared_view::ViewError::Revoked
            | crate::domain::shared_view::ViewError::InvalidClient
    ) {
        return CommandOutcome::Close(match still_valid(state, actor).await {
            Ok(false) => close(CLOSE_REVOKED, "revoked"),
            Ok(true) => close(CLOSE_AUTHENTICATION, "authentication failed"),
            Err(()) => close(CLOSE_INTERNAL, "storage unavailable"),
        });
    }
    CommandOutcome::Result(management_error(request, view_error_code(error)))
}
#[cfg(windows)]
async fn document_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::application::document_source::DocumentError,
) -> CommandOutcome {
    use crate::application::document_source::DocumentError;
    use crate::domain::input_queue::QueueError;

    match error {
        DocumentError::Registry(error) => {
            failure(state, actor, request, QueueReadFailure::Registry(error)).await
        }
        DocumentError::View(error) => view_failure(state, actor, request, error).await,
        DocumentError::Queue(
            QueueError::NotFound | QueueError::DuplicateId | QueueError::RevisionExhausted,
        ) => CommandOutcome::Result(management_error(request, "invalid_item")),
        DocumentError::Queue(QueueError::RevisionChanged) => {
            CommandOutcome::Result(management_error(request, "invalid_revision"))
        }
        DocumentError::Queue(
            QueueError::Full
            | QueueError::DispatchStarted
            | QueueError::Busy
            | QueueError::Blocked
            | QueueError::ResourceChanged,
        ) => CommandOutcome::Result(management_error(request, "request_conflict")),
        DocumentError::Queue(QueueError::Suspended) => {
            CommandOutcome::Result(management_error(request, "input_suspended"))
        }
        DocumentError::Queue(QueueError::StaleToken | QueueError::SequenceExhausted) => {
            CommandOutcome::Result(management_error(request, "read_stale"))
        }
        DocumentError::Closed => {
            CommandOutcome::Result(management_error(request, "body_unavailable"))
        }
        DocumentError::Allocation => {
            CommandOutcome::Result(management_error(request, "allocation_failed"))
        }
        DocumentError::DisposalFailed | DocumentError::StartUnconfirmed => {
            CommandOutcome::Result(management_error(request, "read_failed"))
        }
    }
}
#[cfg(windows)]
fn view_opened(request: Id128, read: &crate::node::NativeSocketViewRead) -> Value {
    use crate::domain::shared_view::{Freshness, JoinState, ProjectionState, ReadMode};
    let descriptor = read.descriptor();
    let mode = match descriptor.read_mode() {
        ReadMode::CurrentOnly => "current_only",
        ReadMode::ReplayRequested => "replay_requested",
        ReadMode::Replaying => "replaying",
        ReadMode::ReplayComplete => "replay_complete",
        ReadMode::ReplayIncomplete => "replay_incomplete",
    };
    let freshness = match descriptor.freshness() {
        Freshness::CurrentObservation => "current_observation",
        Freshness::Revalidating => "revalidating",
        Freshness::Stale => "stale",
    };
    let projection = match descriptor.projection() {
        ProjectionState::Available => "available",
        ProjectionState::PartialKnownStructure => "partial_known_structure",
        ProjectionState::ProjectionUnavailable => "unavailable",
    };
    let join = match descriptor.join() {
        JoinState::ProvenNativeIds => "proven_native_ids",
        JoinState::RebuiltNoStableIds => "rebuilt_no_stable_ids",
        JoinState::UnassociatedSegments => "unassociated_segments",
    };
    let mut status = serde_json::json!({
        "mode": mode,
        "freshness": freshness,
        "projection": projection,
        "join": join,
        "historyIncomplete": descriptor.history_incomplete(),
        "cacheRecoverable": descriptor.cache_recoverable()
    });
    if let Some(tc) = read.transcript_cache_status()
        && !(tc.transcript_source == crate::domain::shared_view::TranscriptSource::Live
            && tc.sync_state == crate::domain::shared_view::TranscriptSyncState::Idle
            && tc.capability_boundary
                == crate::domain::shared_view::TranscriptCapabilityBoundary::Unknown
            && tc.continuable
            && !tc.cache_warning)
        && let Some(obj) = status.as_object_mut()
    {
        obj.insert(
            "transcriptSource".to_owned(),
            Value::String(tc.transcript_source.as_wire_str().to_owned()),
        );
        obj.insert(
            "syncState".to_owned(),
            Value::String(tc.sync_state.as_wire_str().to_owned()),
        );
        obj.insert(
            "capabilityBoundary".to_owned(),
            Value::String(tc.capability_boundary.as_wire_str().to_owned()),
        );
        obj.insert("continuable".to_owned(), Value::Bool(tc.continuable));
        obj.insert("cacheWarning".to_owned(), Value::Bool(tc.cache_warning));
    }
    let mut response = serde_json::json!({
        "v": 1,
        "type": "view.opened",
        "requestId": request,
        "streamId": read.stream_id().encode(),
        "viewEpoch": descriptor.view_epoch().as_entity_id().encode(),
        "snapshotRevision": descriptor.revision().as_u64().to_string(),
        "capabilities": {"opaqueChunks": 1},
        "status": status
    });
    if let Some(page) = read.page_metadata() {
        response["page"] = page.clone();
    }
    response
}
#[cfg(windows)]
fn view_close_result(
    receipt: &crate::application::shared_views::reader::ReadCloseReceipt,
) -> Value {
    use crate::application::shared_views::reader::ReadStreamState;
    let state = match receipt.state() {
        ReadStreamState::Closed => "closed",
        ReadStreamState::ClosingAfterStarted => "closing_after_started",
        ReadStreamState::AwaitInitialCredit
        | ReadStreamState::Ready
        | ReadStreamState::Prepared
        | ReadStreamState::Started
        | ReadStreamState::Flushing
        | ReadStreamState::Sent => "closed",
    };
    serde_json::json!({
        "streamId": receipt.stream_id().encode(),
        "alreadyClosed": receipt.was_already_closed(),
        "lastCommittedPacket": receipt.last_committed_packet().to_string(),
        "startedPacket": receipt.started_packet().map(|packet| packet.to_string()),
        "state": state,
    })
}
#[cfg(windows)]
fn source_session_operation_id(
    actor: &AuthenticatedClient,
    request_id: Id128,
) -> crate::domain::source_sessions::SourceSessionOperationId {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"acp-remote:source-session-operation:v1\0");
    hasher.update(actor.client_id().as_bytes());
    hasher.update(b"\0");
    hasher.update(request_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    crate::domain::source_sessions::SourceSessionOperationId::new(
        crate::domain::EntityId::from_bytes(bytes),
    )
}
#[cfg(windows)]
fn build_source_selection(
    input: &session_protocol::SourceLaunchSelectionInput,
) -> Result<crate::domain::source_sessions::SourceLaunchSelection, &'static str> {
    let profile = crate::domain::native_catalog::ProfileKey::try_new(
        id(input.profile_id),
        input.profile_revision.as_u64(),
    )
    .map_err(|_| "invalid_configuration")?;
    Ok(crate::domain::source_sessions::SourceLaunchSelection {
        source: id(input.source_id),
        harness: id(input.harness_id),
        profile,
        workspace: id(input.workspace_id),
        launch_path: input.launch_path.clone(),
    })
}
#[cfg(windows)]
const fn source_session_error_code(
    error: crate::domain::source_sessions::SourceSessionError,
) -> &'static str {
    use crate::domain::source_sessions::SourceSessionError;
    match error {
        SourceSessionError::InvalidActor => "invalid_client",
        SourceSessionError::InvalidScope => "invalid_scope",
        SourceSessionError::InvalidSelection => "invalid_request",
        SourceSessionError::UnknownOperation => "unknown_request",
        SourceSessionError::Conflict => "request_conflict",
        SourceSessionError::CorruptState => "read_failed",
        SourceSessionError::StorageUnavailable => "necessary_storage_unavailable",
        SourceSessionError::CommitUnconfirmed
        | SourceSessionError::SqlCloseUnconfirmed
        | SourceSessionError::WorkerUnconfirmed => "commit_unconfirmed",
        SourceSessionError::Protocol => "protocol_failure",
        SourceSessionError::Configuration => "invalid_configuration",
        SourceSessionError::OwnershipUnconfirmed => "release_unconfirmed",
        SourceSessionError::ResultUnavailable => "body_unavailable",
        SourceSessionError::Allocation => "allocation_failed",
        SourceSessionError::RandomUnavailable => "random_unavailable",
        SourceSessionError::NodeStopping => "node_stopping",
    }
}
#[cfg(windows)]
const fn source_failure_code(
    error: &crate::domain::source_sessions::SourceSessionFailure,
) -> &'static str {
    use crate::domain::native_dispatch::{DispatchError, NativeProtocolFailure};
    use crate::domain::source_sessions::SourceSessionFailure;
    match error {
        SourceSessionFailure::Configuration(failure) => catalog_error_code(failure.kind()),
        SourceSessionFailure::Native(dispatch) => match dispatch {
            DispatchError::SessionCapacityFull => "session_capacity_full",
            DispatchError::SessionTreeUnverified => "session_tree_unverified",
            DispatchError::ProtocolFault(NativeProtocolFailure::UnsupportedCapability) => {
                "unsupported_capability"
            }
            DispatchError::ProtocolFault(_)
            | DispatchError::ProtocolUnavailable
            | DispatchError::ProtocolRejected => "protocol_failure",
            DispatchError::NodeStopping => "node_stopping",
            DispatchError::Stopped => "source_released",
            DispatchError::InvalidConfiguration => "invalid_configuration",
            DispatchError::InvalidSource => "invalid_source",
            DispatchError::InvalidScope => "invalid_scope",
            DispatchError::InvalidTarget => "invalid_target",
            DispatchError::Conflict
            | DispatchError::StaleSelection
            | DispatchError::RevisionChanged
            | DispatchError::RuntimeAlreadyStarted => "request_conflict",
            DispatchError::CommitUnconfirmed | DispatchError::SqlCloseUnconfirmed { .. } => {
                "commit_unconfirmed"
            }
            DispatchError::StorageUnavailable => "necessary_storage_unavailable",
            DispatchError::LengthUnrepresentable => "length_unrepresentable",
            DispatchError::SequenceExhausted => "sequence_exhausted",
            DispatchError::CorruptState
            | DispatchError::InvalidResource
            | DispatchError::InvalidPromptReservation
            | DispatchError::NotReady
            | DispatchError::RequiredStateUnavailable
            | DispatchError::EnqueueRejected
            | DispatchError::ObservationOrder
            | DispatchError::ObservationUnapplied
            | DispatchError::ObservationDropped
            | DispatchError::ResourceUnconfirmed
            | DispatchError::SourcePlatform { .. } => "read_failed",
        },
        SourceSessionFailure::NecessarySql(sql) => source_session_error_code(sql.kind),
        SourceSessionFailure::WorkerUnconfirmed { .. } => "commit_unconfirmed",
        SourceSessionFailure::Rejected(rejected) => source_session_error_code(*rejected),
    }
}
#[cfg(windows)]
async fn source_session_failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: crate::domain::source_sessions::SourceSessionFailure,
) -> CommandOutcome {
    use crate::domain::native_catalog::CatalogError;
    use crate::domain::source_sessions::{SourceSessionError, SourceSessionFailure};
    let is_invalid_actor = match &error {
        SourceSessionFailure::Rejected(SourceSessionError::InvalidActor) => true,
        SourceSessionFailure::NecessarySql(sql) if sql.kind == SourceSessionError::InvalidActor => {
            true
        }
        SourceSessionFailure::Configuration(failure)
            if failure.kind() == CatalogError::InvalidActor =>
        {
            true
        }
        _ => false,
    };
    if is_invalid_actor {
        return match still_valid(state, actor).await {
            Ok(false) => CommandOutcome::Close(close(CLOSE_REVOKED, "revoked")),
            Err(()) => CommandOutcome::Close(close(CLOSE_INTERNAL, "storage unavailable")),
            Ok(true) => CommandOutcome::Result(management_error(request, "invalid_client")),
        };
    }
    CommandOutcome::Result(management_error(request, source_failure_code(&error)))
}
#[cfg(windows)]
fn source_session_registration_json(
    value: &crate::domain::source_sessions::SourceSessionRegistrationMetadata,
) -> Value {
    serde_json::json!({
        "sourceOperationId": value.id.encode(),
        "actorId": value.actor.encode(),
        "action": value.action,
        "sourceId": value.source.encode(),
        "harnessId": value.harness.encode(),
        "profileId": value.profile.id().encode(),
        "profileRevision": value.profile.revision().to_string(),
        "workspaceId": value.workspace.encode(),
        "registeredAtMs": value.registered_at_ms.to_string(),
    })
}
#[cfg(windows)]
fn workspace_record_json(value: &crate::domain::native_catalog::WorkspaceRecord) -> Value {
    serde_json::json!({
        "workspaceId": value.workspace_id.encode(),
        "canonicalPath": value.canonical_path,
    })
}
#[cfg(windows)]
fn product_target_record_json(value: &crate::domain::native_catalog::ProductTargetRecord) -> Value {
    let binding = value.binding.as_ref().map(|b| {
        serde_json::json!({
            "profileId": b.profile_id.encode(),
            "profileRevision": b.profile_revision.to_string(),
            "workspaceId": b.workspace_id.encode(),
            "launchPath": b.launch_path,
            "observedCanonicalPath": b.observed_canonical_path,
        })
    });
    serde_json::json!({
        "targetId": value.target_id.encode(),
        "sourceId": value.source_id.encode(),
        "nativeSessionId": value.native_session_id,
        "workspaceId": value.workspace_id.map(super::super::domain::EntityId::encode),
        "binding": binding,
    })
}
#[cfg(windows)]
fn catalog_inventory_json(value: &crate::domain::native_catalog::CatalogInventory) -> Value {
    let workspaces = value
        .workspaces
        .iter()
        .map(workspace_record_json)
        .collect::<Vec<_>>();
    let sources = value
        .sources
        .iter()
        .map(|s| {
            serde_json::json!({
                "sourceId": s.source_id.encode(),
                "name": s.name,
            })
        })
        .collect::<Vec<_>>();
    let harnesses = value
        .harnesses
        .iter()
        .map(|h| {
            serde_json::json!({
                "harnessId": h.harness_id.encode(),
                "name": h.name,
                "sourceId": h.source_id.encode(),
            })
        })
        .collect::<Vec<_>>();
    let profiles = value
        .profiles
        .iter()
        .map(|p| {
            serde_json::json!({
                "profileId": p.profile_id.encode(),
                "revision": p.revision.to_string(),
                "harnessId": p.harness_id.encode(),
                "sourceId": p.source_id.encode(),
            })
        })
        .collect::<Vec<_>>();
    let targets = value
        .targets
        .iter()
        .map(product_target_record_json)
        .collect::<Vec<_>>();
    serde_json::json!({
        "workspaces": workspaces,
        "sources": sources,
        "harnesses": harnesses,
        "profiles": profiles,
        "targets": targets,
    })
}
#[cfg(windows)]
fn source_session_fact_result(value: &crate::domain::source_sessions::SourceSessionFact) -> Value {
    use crate::domain::native_dispatch::{RpcScalar, TransportFact};
    use crate::domain::source_sessions::{
        SourceAckState, SourceFailureClass, SourceSessionOutcome, SourceSessionState,
    };
    let state = match value.state {
        SourceSessionState::Registered => "registered",
        SourceSessionState::Owned => "owned",
        SourceSessionState::Terminal => "terminal",
        SourceSessionState::Interrupted => "interrupted",
    };
    let outcome = match value.outcome {
        SourceSessionOutcome::Pending => "pending",
        SourceSessionOutcome::Succeeded => "succeeded",
        SourceSessionOutcome::Refused => "refused",
        SourceSessionOutcome::Unknown => "unknown",
    };
    let rpc = value.rpc.as_ref().map(|scalar| match scalar {
        RpcScalar::Integer(v) => serde_json::json!({"kind": "integer", "value": v.to_string()}),
        RpcScalar::String(v) => serde_json::json!({"kind": "string", "value": v.as_ref()}),
    });
    let transport = value.transport.map(|t| match t {
        TransportFact::Zero { reason } => serde_json::json!({
            "kind": "zero",
            "reason": reason.label(),
        }),
        TransportFact::Unknown { accepted, cause } => serde_json::json!({
            "kind": "unknown",
            "accepted": accepted.to_string(),
            "cause": cause.label(),
        }),
        TransportFact::Written => serde_json::json!({
            "kind": "written",
        }),
    });
    let response = value.response.as_ref().map(|r| {
        let ack = match r.ack {
            SourceAckState::Pending => "pending",
            SourceAckState::Confirmed => "confirmed",
            SourceAckState::Rejected => "rejected",
            SourceAckState::Unknown => "unknown",
        };
        serde_json::json!({
            "kind": r.kind.label(),
            "nativeSessionId": r.native_session_id.as_deref(),
            "nativeKeyAvailable": r.native_key_available,
            "remoteErrorCode": r.remote_error_code.map(|v| v.to_string()),
            "sequence": r.sequence.to_string(),
            "receipt": r.receipt.to_string(),
            "resultBytes": r.result_bytes.to_string(),
            "ack": ack,
        })
    });
    serde_json::json!({
        "registration": source_session_registration_json(&value.registration),
        "resultAvailable": value.result_available,
        "state": state,
        "outcome": outcome,
        "runtimeId": value.runtime.map(|r| r.0.encode()),
        "admissionAuthGeneration": value.admission_auth_generation.map(|v| v.to_string()),
        "sourceInstanceId": value.source_instance.map(super::super::domain::EntityId::encode),
        "connectionEpoch": value.connection_epoch.map(super::super::domain::EntityId::encode),
        "protocolVersion": value.wire.map(|w| w.number().to_string()),
        "rpcId": rpc,
        "frameLength": value.frame_length.map(|v| v.to_string()),
        "transport": transport,
        "response": response,
        "failure": value.failure.map(SourceFailureClass::label),
    })
}
fn outcome(value: QueueOutcome) -> Value {
    let (kind, item, revision) = match value {
        QueueOutcome::Queued { item, revision } => ("queued", item, revision),
        QueueOutcome::DirectSelected { item, revision } => ("direct_selected", item, revision),
        QueueOutcome::Edited { item, revision } => ("edited", item, revision),
        QueueOutcome::Cancelled { item, revision } => ("cancelled", item, revision),
        QueueOutcome::Prioritized { item, revision } => ("prioritized", item, revision),
        QueueOutcome::Resumed { restored } => {
            return serde_json::json!({"kind":"resumed","restored":restored.to_string()});
        }
        QueueOutcome::Rejected(reason) => {
            return serde_json::json!({"kind":"rejected","reason":reason.label()});
        }
    };
    serde_json::json!({"kind":kind,"itemId":item.encode(),"revision":revision.to_string()})
}
fn fact(id: RegistrationId, value: RequestFact) -> Value {
    let mut result = serde_json::json!({"registrationId":id.encode(),"state":"retired"});
    match value {
        RequestFact::Registered(_) => result["state"] = "registered".into(),
        RequestFact::Applying(applied) => {
            result["state"] = "applying".into();
            result["operationId"] = applied.operation.encode().into();
            result["runtimeId"] = applied.runtime.0.encode().into();
            result["admittedAtMs"] = applied.begun_at_ms.to_string().into();
        }
        RequestFact::Settled {
            outcome: settled, ..
        } => {
            result["state"] = "settled".into();
            result["outcome"] = outcome(settled);
        }
        RequestFact::Interrupted { .. } => result["state"] = "interrupted".into(),
        RequestFact::Retired => {}
    }
    result
}
#[cfg(windows)]
fn control_snapshot_json(
    snapshot: &crate::application::session_control::SessionControlSnapshot,
) -> Value {
    serde_json::json!({
        "targetId": snapshot.target_id.map(crate::domain::EntityId::encode),
        "rootTargetId": snapshot.root_target_id.map(crate::domain::EntityId::encode),
        "sourceId": snapshot.source_id.encode(),
        "rootNativeSessionId": snapshot.root_native_session_id,
        "controllerClientId": snapshot.controller_client_id.map(crate::domain::EntityId::encode),
        "controllerDisplayName": snapshot.controller_display_name,
        "controlEpoch": snapshot.control_epoch.to_string(),
    })
}
#[cfg(windows)]
fn control_audit_timestamp_seconds() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}
fn registration(value: RegistrationFact) -> Value {
    serde_json::json!({
        "registrationId": value.id().encode(),
        "state": "registered",
        "action": value.intent().action().label(),
        "targetId": value.intent().target().encode(),
        "registerAuthGeneration": value.register_auth_generation().to_string(),
        "registeredAtMs": value.registered_at_ms().to_string()
    })
}
async fn failure(
    state: &ManagementState,
    actor: &AuthenticatedClient,
    request: Id128,
    error: QueueReadFailure,
) -> CommandOutcome {
    if let Some(code) = allocation_code(error) {
        return CommandOutcome::Result(management_error(request, code));
    }
    let error = match error {
        QueueReadFailure::AllocationFailed => {
            return CommandOutcome::Result(management_error(request, "allocation_failed"));
        }
        QueueReadFailure::WorkerStoppedUnconfirmed => {
            return CommandOutcome::Result(management_error(
                request,
                if state.control.is_ready() {
                    "read_failed"
                } else {
                    "node_stopping"
                },
            ));
        }
        QueueReadFailure::Registry(error) => error,
    };
    let code = match error {
        RegistryError::InvalidActor => {
            return CommandOutcome::Close(match still_valid(state, actor).await {
                Ok(false) => close(CLOSE_REVOKED, "revoked"),
                Ok(true) => close(CLOSE_AUTHENTICATION, "authentication failed"),
                Err(()) => close(CLOSE_INTERNAL, "storage unavailable"),
            });
        }
        RegistryError::RandomUnavailable => {
            return CommandOutcome::Close(close(CLOSE_INTERNAL, "internal error"));
        }
        RegistryError::InvalidIntent => "invalid_request",
        RegistryError::InvalidTarget => "invalid_target",
        RegistryError::SessionNotControlled => "session_not_controlled",
        RegistryError::SessionTreeUnverified => "session_tree_unverified",
        RegistryError::UnknownRegistration => "invalid_registration",
        RegistryError::Conflict | RegistryError::RuntimeAlreadyStarted => "request_conflict",
        RegistryError::Retired => "request_retired",
        RegistryError::StorageUnavailable => "necessary_storage_unavailable",
        RegistryError::CommitUnconfirmed => "commit_unconfirmed",
        RegistryError::CorruptState | RegistryError::Cancelled => "read_failed",
        RegistryError::NoOwnedOperation | RegistryError::StaleObservation => "read_stale",
        RegistryError::NodeStopping => "node_stopping",
        RegistryError::NodeStarting => "product_not_ready",
    };
    CommandOutcome::Result(management_error(request, code))
}
fn push<T: Clone + Send + 'static>(
    pending: &mut PendingReads,
    state: &Arc<ManagementState>,
    actor: &AuthenticatedClient,
    request: Id128,
    receipt: Result<QueueReadReceipt<T>, QueueReadFailure>,
    project: impl FnOnce(T) -> Result<Value, serde_json::Error> + Send + 'static,
) {
    let state = state.clone();
    let actor = actor.clone();
    pending.push(Box::pin(async move {
        let result = match receipt {
            Ok(receipt) => receipt.wait().await,
            Err(error) => Err(error),
        };
        match result {
            Ok(value) => project(value).map_or_else(
                |_| CommandOutcome::Close(close(CLOSE_INTERNAL, "internal error")),
                |value| CommandOutcome::Result(management_success(request, &value)),
            ),
            Err(error) => failure(&state, &actor, request, error).await,
        }
    }));
}
pub(super) fn start(
    state: &Arc<ManagementState>,
    actor: &AuthenticatedClient,
    attachment: &Arc<SocketAttachment>,
    outbound: &Outbound,
    command: &CommandFrame,
    pending: &mut PendingReads,
) -> Option<CommandOutcome> {
    let request = command.request_id;
    if !state.control.is_ready() {
        return Some(CommandOutcome::Result(management_error(
            request,
            "node_stopping",
        )));
    }
    let bytes = serde_json::to_vec(
        &serde_json::json!({"v":1,"type":"command","requestId":request,"action":command.action,"payload":command.payload}),
    );
    let parsed = bytes
        .ok()
        .and_then(|bytes| session_protocol::parse_session_command(&bytes).ok());
    let Some(command) = parsed else {
        return Some(CommandOutcome::Result(management_error(
            request,
            "invalid_request",
        )));
    };
    #[cfg(windows)]
    {
        use session_protocol::SessionAction;
        match command.action() {
            SessionAction::InputUploadBegin { target_id, purpose } => {
                let Some(owner) = attachment.upload.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let target = id(*target_id);
                let purpose = match purpose {
                    session_protocol::UploadPurpose::Submit => crate::node::UploadPurpose::Submit,
                    session_protocol::UploadPurpose::Edit => crate::node::UploadPurpose::Edit,
                };
                #[cfg(test)]
                let state = state.clone();
                pending.push(Box::pin(async move {
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Begin,
                        UploadCommandBoundary::BeforeCall,
                    )
                    .await;
                    let result = owner.begin_typed(&client, target, purpose).await;
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Begin,
                        UploadCommandBoundary::AfterCall,
                    )
                    .await;
                    match result {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"uploadId":value.upload().to_base64url()}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::InputUploadFinish {
                upload_id,
                total_bytes,
            } => {
                let Some(owner) = attachment.upload.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                let total = total_bytes.as_u64();
                #[cfg(test)]
                let state = state.clone();
                pending.push(Box::pin(async move {
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Finish,
                        UploadCommandBoundary::BeforeCall,
                    )
                    .await;
                    let result = owner.finish_typed(&client, upload, total).await;
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Finish,
                        UploadCommandBoundary::AfterCall,
                    )
                    .await;
                    match result {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"uploadId":value.upload().to_base64url()}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::InputUploadStatus { upload_id } => {
                let Some(owner) = attachment.upload.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                #[cfg(test)]
                let state = state.clone();
                pending.push(Box::pin(async move {
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Status,
                        UploadCommandBoundary::BeforeCall,
                    )
                    .await;
                    let result = owner.status_typed(&client, upload).await;
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Status,
                        UploadCommandBoundary::AfterCall,
                    )
                    .await;
                    match result {
                        Ok(status) => {
                            let mut value = serde_json::json!({
                                "uploadId":upload.to_base64url(),
                                "phase":upload_phase(status.phase()),
                                "nextOffset":status.next_offset().to_string(),
                            });
                            if let Some(body) = status.body() {
                                value["bodyId"] = body.to_base64url().into();
                            }
                            if let Some(error) = status.error() {
                                value["error"] = error.code().into();
                            }
                            CommandOutcome::Result(management_success(request, &value))
                        }
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::InputUploadRelease { upload_id } => {
                let Some(owner) = attachment.upload.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                #[cfg(test)]
                let state = state.clone();
                pending.push(Box::pin(async move {
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Release,
                        UploadCommandBoundary::BeforeCall,
                    )
                    .await;
                    let result = owner.release_typed(&client, upload).await;
                    #[cfg(test)]
                    pause_upload_command(
                        &state,
                        UploadCommandAction::Release,
                        UploadCommandBoundary::AfterCall,
                    )
                    .await;
                    match result {
                        Ok(()) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"released":true}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::CatalogUploadBegin {} => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                pending.push(Box::pin(async move {
                    let owner = match management.open_upload(&client) {
                        Ok(owner) => owner,
                        Err(error) => return upload_error(request, error),
                    };
                    match owner.begin_typed(&client).await {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"uploadId":value.upload().to_base64url()}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::CatalogUploadFinish {
                upload_id,
                total_bytes,
            } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                let total = total_bytes.as_u64();
                pending.push(Box::pin(async move {
                    let owner = match management.open_upload(&client) {
                        Ok(owner) => owner,
                        Err(error) => return upload_error(request, error),
                    };
                    match owner.finish_typed(&client, upload, total).await {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"uploadId":value.upload().to_base64url()}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::CatalogUploadStatus { upload_id } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                pending.push(Box::pin(async move {
                    let owner = match management.open_upload(&client) {
                        Ok(owner) => owner,
                        Err(error) => return upload_error(request, error),
                    };
                    match owner.status_typed(&client, upload).await {
                        Ok(status) => {
                            let mut value = serde_json::json!({
                                "uploadId":upload.to_base64url(),
                                "phase":upload_phase(status.phase()),
                                "nextOffset":status.next_offset().to_string(),
                            });
                            if let Some(error) = status.error() {
                                value["error"] = catalog_preparation_error_code(error).into();
                            }
                            CommandOutcome::Result(management_success(request, &value))
                        }
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::CatalogUploadRelease { upload_id } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *upload_id;
                pending.push(Box::pin(async move {
                    let owner = match management.open_upload(&client) {
                        Ok(owner) => owner,
                        Err(error) => return upload_error(request, error),
                    };
                    match owner.release_typed(&client, upload).await {
                        Ok(()) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"released":true}),
                        )),
                        Err(error) => upload_failure(request, error),
                    }
                }));
                return None;
            }
            SessionAction::CatalogRegister { document_upload_id } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let upload = *document_upload_id;
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let receipt = match management.register(&client, upload).await {
                        Ok(receipt) => receipt,
                        Err(error) => return catalog_failure(&state, &actor, request, error).await,
                    };
                    match receipt.wait().await {
                        Ok(catalog_request) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({
                                "catalogRequestId":catalog_request.encode(),
                                "state":"registered",
                            }),
                        )),
                        Err(error) => catalog_command_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::CatalogApply { catalog_request_id } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let catalog_request =
                    crate::domain::native_catalog::CatalogRequestId::new(id(*catalog_request_id));
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let receipt = match management.apply(&client, catalog_request).await {
                        Ok(receipt) => receipt,
                        Err(error) => return catalog_failure(&state, &actor, request, error).await,
                    };
                    match receipt.wait().await {
                        Ok(receipt) => CommandOutcome::Result(management_success(
                            request,
                            &crate::node::catalog_receipt_metadata(&receipt),
                        )),
                        Err(error) => catalog_command_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::CatalogRequest { catalog_request_id } => {
                let Some(management) = attachment.catalog_management.clone() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let catalog_request =
                    crate::domain::native_catalog::CatalogRequestId::new(id(*catalog_request_id));
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let receipt = match management.query(&client, catalog_request).await {
                        Ok(receipt) => receipt,
                        Err(error) => return catalog_failure(&state, &actor, request, error).await,
                    };
                    match receipt.wait().await {
                        Ok(fact) => CommandOutcome::Result(management_success(
                            request,
                            &crate::node::catalog_fact_metadata(&fact),
                        )),
                        Err(error) => catalog_command_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::WorkspaceRegister { path } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let path = path.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services.register_workspace(&actor, &path).await {
                        Ok(record) => CommandOutcome::Result(management_success(
                            request,
                            &workspace_record_json(&record),
                        )),
                        Err(error) => catalog_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::TargetRegister {
                source_id,
                native_session_id,
                workspace_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let source = id(*source_id);
                let native_session_id = native_session_id.clone();
                let workspace = id(*workspace_id);
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services
                        .register_target(&actor, source, &native_session_id, workspace)
                        .await
                    {
                        Ok(record) => CommandOutcome::Result(management_success(
                            request,
                            &product_target_record_json(&record),
                        )),
                        Err(error) => catalog_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SessionCapacityInspect {} => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services.session_capacity(&actor).await {
                        Ok(capacity) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({
                                "limit": capacity.resources.limit,
                                "occupied": capacity.resources.occupied,
                                "reserved": capacity.resources.reserved,
                                "closedPending": capacity.resources.closed_pending,
                                "automaticRecycling": capacity.automatic_recycling,
                            }),
                        )),
                        Err(error) => release_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::CatalogInventory {} => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services.catalog_inventory(&actor).await {
                        Ok(inventory) => CommandOutcome::Result(management_success(
                            request,
                            &catalog_inventory_json(&inventory),
                        )),
                        Err(error) => catalog_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::InputRegister {
                target_id,
                operation,
                expected_control_epoch,
            } => {
                let Some(services) = state.session_services.get() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let service = services.queue_management();
                let mutation = match operation {
                    session_protocol::QueueMutation::Submit { body_id } => {
                        crate::node::queue_management::NodeQueueMutation::Submit { body: *body_id }
                    }
                    session_protocol::QueueMutation::Edit {
                        item_id,
                        expected_revision,
                        body_id,
                    } => crate::node::queue_management::NodeQueueMutation::Edit {
                        item: id(*item_id),
                        revision: expected_revision.as_u64(),
                        body: *body_id,
                    },
                    session_protocol::QueueMutation::Cancel {
                        item_id,
                        expected_revision,
                    } => crate::node::queue_management::NodeQueueMutation::Cancel {
                        item: id(*item_id),
                        revision: expected_revision.as_u64(),
                    },
                    session_protocol::QueueMutation::Prioritize {
                        item_id,
                        expected_revision,
                    } => crate::node::queue_management::NodeQueueMutation::Prioritize {
                        item: id(*item_id),
                        revision: expected_revision.as_u64(),
                    },
                    session_protocol::QueueMutation::Resume {} => {
                        crate::node::queue_management::NodeQueueMutation::Resume
                    }
                };
                let client = attachment.client.context.client();
                let target = id(*target_id);
                let expected_epoch =
                    expected_control_epoch.map(session_protocol::DecimalU64::as_u64);
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match service
                        .register_with_id_and_epoch(
                            &client,
                            target,
                            mutation,
                            Some(RegistrationId::new(id(request))),
                            expected_epoch,
                        )
                        .await
                    {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &registration(value),
                        )),
                        Err(error) => queue_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::InputConsume {
                target_id,
                registration_id,
                body_id,
                expected_control_epoch,
            } => {
                let Some(services) = state.session_services.get() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let service = services.queue_management();
                let client = attachment.client.context.client();
                let target = id(*target_id);
                let registration = RegistrationId::new(id(*registration_id));
                let body = *body_id;
                let expected_epoch =
                    expected_control_epoch.map(session_protocol::DecimalU64::as_u64);
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let result = async {
                        let receipt = service
                            .begin_consume_with_epoch(
                                &client,
                                target,
                                registration,
                                body,
                                expected_epoch,
                            )
                            .await?;
                        receipt.wait().await
                    }.await;
                    match result {
                        Ok(crate::node::queue_management::NodeConsumeResult::Refused {
                            error:
                                error @ (RegistryError::SessionNotControlled
                                | RegistryError::SessionTreeUnverified
                                | RegistryError::InvalidActor),
                            returned: _,
                        }) => {
                            failure(&state, &actor, request, QueueReadFailure::Registry(error)).await
                        }
                        Ok(value) => {
                            let value = match value {
                                crate::node::queue_management::NodeConsumeResult::Observed { fact: value } => fact(registration, value),
                                crate::node::queue_management::NodeConsumeResult::Resolved { fact: value, returned } => serde_json::json!({"state":"settled","returned":returned,"request":fact(registration, value)}),
                                crate::node::queue_management::NodeConsumeResult::Pending { request: id } => serde_json::json!({"state":"pending","registrationId":id.encode()}),
                                crate::node::queue_management::NodeConsumeResult::Refused { error, returned } => serde_json::json!({"state":"refused","reason":format!("{error:?}"),"returned":returned}),
                            };
                            CommandOutcome::Result(management_success(request, &value))
                        }
                        Err(error) => queue_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::InputQueue { target_id } => {
                let Some(services) = state.session_services.get() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let service = services.queue_reads();
                push(
                    pending,
                    state,
                    actor,
                    request,
                    service.inspect(&attachment.client.context.client(), id(*target_id)),
                    serde_json::to_value,
                );
                return None;
            }
            SessionAction::InputRequest {
                target_id,
                registration_id,
            } => {
                let Some(services) = state.session_services.get() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let service = services.queue_reads();
                let registration = RegistrationId::new(id(*registration_id));
                push(
                    pending,
                    state,
                    actor,
                    request,
                    service.query(
                        &attachment.client.context.client(),
                        id(*target_id),
                        registration,
                    ),
                    move |value| Ok(fact(registration, value)),
                );
                return None;
            }
            SessionAction::InputBodyOpen {
                target_id,
                item_id,
                expected_revision,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let target = id(*target_id);
                let item = id(*item_id);
                let revision = expected_revision.as_u64();
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let transfer = match services
                        .open_queued_body(&client, target, item, revision)
                        .await
                    {
                        Ok(transfer) => transfer,
                        Err(error) => {
                            return document_failure(&state, &actor, request, error).await;
                        }
                    };
                    let descriptor = match serde_json::to_value(transfer.descriptor()) {
                        Ok(descriptor) => descriptor,
                        Err(_) => {
                            return CommandOutcome::Close(close(CLOSE_INTERNAL, "internal error"));
                        }
                    };
                    if let Err(error) = attachment.attach_document(transfer, &outbound) {
                        return view_failure(&state, &actor, request, error).await;
                    }
                    CommandOutcome::Result(management_success(request, &descriptor))
                }));
                return None;
            }
            SessionAction::ApprovalList {} => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services.approval_list(&actor).await {
                        Ok(items) => {
                            let items = items.iter().map(approval_view_result).collect::<Vec<_>>();
                            CommandOutcome::Result(management_success(
                                request,
                                &serde_json::json!({"items":items}),
                            ))
                        }
                        Err(error) => approval_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ApprovalRead { approval_id } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let approval =
                    crate::domain::approval_owner::ApprovalId::from_entity(id(*approval_id));
                pending.push(Box::pin(async move {
                    match services.approval_read(&actor, approval).await {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &approval_view_result(&value),
                        )),
                        Err(error) => approval_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ApprovalRegister {
                approval_id,
                choice_id,
                expected_control_epoch,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let approval =
                    crate::domain::approval_owner::ApprovalId::from_entity(id(*approval_id));
                let choice = crate::domain::approval_owner::ChoiceId::from_entity(id(*choice_id));
                let input = crate::domain::approval_owner::RegisterDecision::new(approval, choice)
                    .with_expected_control_epoch(
                        expected_control_epoch.map(session_protocol::DecimalU64::as_u64),
                    );
                pending.push(Box::pin(async move {
                    match services.approval_register(&actor, input).await {
                        Ok(registration) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({
                                "approvalId":approval.encode(),
                                "choiceId":choice.encode(),
                                "approvalRegistrationId":registration.encode(),
                                "state":"registered",
                            }),
                        )),
                        Err(error) => approval_write_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ApprovalDecide {
                approval_registration_id,
                expected_control_epoch,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let registration =
                    crate::domain::approval_owner::ApprovalRegistrationId::from_entity(id(
                        *approval_registration_id,
                    ));
                let input = crate::domain::approval_owner::SubmitDecision::new(registration)
                    .with_expected_control_epoch(
                        expected_control_epoch.map(session_protocol::DecimalU64::as_u64),
                    );
                pending.push(Box::pin(async move {
                    use crate::domain::approval_owner::DecisionAcceptance;
                    match services.approval_decide(&actor, input).await {
                        Ok(DecisionAcceptance::Accepted(decision)) => {
                            CommandOutcome::Result(management_success(
                                request,
                                &serde_json::json!({
                                    "approvalId":decision.approval.encode(),
                                    "approvalRegistrationId":decision.registration.encode(),
                                    "state":"accepted",
                                }),
                            ))
                        }
                        Ok(DecisionAcceptance::Existing(decision)) => {
                            CommandOutcome::Result(management_success(
                                request,
                                &serde_json::json!({
                                    "approvalId":decision.approval.encode(),
                                    "approvalRegistrationId":decision.registration.encode(),
                                    "state":"existing",
                                }),
                            ))
                        }
                        Ok(DecisionAcceptance::Rejected(error)) => {
                            approval_failure(&state, &actor, request, error).await
                        }
                        Ok(DecisionAcceptance::ConfirmationUnknown { .. }) => {
                            CommandOutcome::Result(management_error(request, "commit_unconfirmed"))
                        }
                        Err(error) => approval_write_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SessionControlInspect { target_id } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let target = id(*target_id);
                pending.push(Box::pin(async move {
                    if let Err(error) = services.control_actor(&actor).await {
                        return failure(&state, &actor, request, QueueReadFailure::Registry(error))
                            .await;
                    }
                    let resolved = match services.resolve_control_root(target).await {
                        Ok(resolved) => resolved,
                        Err(error) => {
                            return failure(
                                &state,
                                &actor,
                                request,
                                QueueReadFailure::Registry(error),
                            )
                            .await;
                        }
                    };
                    let snapshot = services.session_control().inspect(
                        &resolved.root,
                        target,
                        resolved.root_target_id,
                    );
                    CommandOutcome::Result(management_success(
                        request,
                        &control_snapshot_json(&snapshot),
                    ))
                }));
                return None;
            }
            SessionAction::SessionControlTakeover { target_id } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let target = id(*target_id);
                let request_entity = id(request);
                pending.push(Box::pin(async move {
                    let actor_ctx = match services.control_actor(&actor).await {
                        Ok(ctx) => ctx,
                        Err(error) => {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    };
                    let resolved = match services.resolve_control_root(target).await {
                        Ok(resolved) => resolved,
                        Err(error) => {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    };
                    let display_name = match services
                        .trusted_client_display_name(actor_ctx.client_id)
                        .await
                    {
                        Ok(Some(name)) => name,
                        Ok(None) => {
                            return failure(
                                &state,
                                &actor,
                                request,
                                QueueReadFailure::Registry(RegistryError::InvalidActor),
                            )
                            .await;
                        }
                        Err(error) => {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    };
                    let outcome = match services.session_control().takeover(
                        actor_ctx.client_id,
                        &display_name,
                        request_entity,
                        target,
                        &resolved.root,
                        resolved.root_target_id,
                    ) {
                        Ok(outcome) => outcome,
                        Err(crate::application::session_control::ControlError::RequestConflict) => {
                            return CommandOutcome::Result(management_error(
                                request,
                                "request_conflict",
                            ));
                        }
                        Err(crate::application::session_control::ControlError::NotControlled) => {
                            return CommandOutcome::Result(management_error(
                                request,
                                "session_not_controlled",
                            ));
                        }
                    };
                    if outcome.state_changed {
                        let audit_target = format!(
                            "{}:{}",
                            outcome.snapshot.source_id.encode(),
                            outcome.snapshot.root_native_session_id
                        );
                        let outcome_json = serde_json::json!({
                            "controllerClientId": actor_ctx.client_id.encode(),
                            "controllerDisplayName": display_name,
                            "controlEpoch": outcome.snapshot.control_epoch.to_string(),
                            "previousController": outcome.previous_controller.map(crate::domain::EntityId::encode),
                            "previousEpoch": outcome.previous_epoch.to_string(),
                            "targetId": target.encode(),
                            "rootTargetId": outcome.snapshot.root_target_id.map(crate::domain::EntityId::encode),
                        })
                        .to_string();
                        if let Err(error) = services
                            .append_control_security_audit(
                                Some(&actor_ctx.client_id.encode()),
                                "session.control.takeover",
                                &audit_target,
                                &outcome_json,
                                control_audit_timestamp_seconds(),
                            )
                            .await
                        {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    }
                    CommandOutcome::Result(management_success(
                        request,
                        &control_snapshot_json(&outcome.snapshot),
                    ))
                }));
                return None;
            }
            SessionAction::SessionControlRelease {
                target_id,
                expected_control_epoch,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let target = id(*target_id);
                let request_entity = id(request);
                let expected_epoch =
                    expected_control_epoch.map(session_protocol::DecimalU64::as_u64);
                pending.push(Box::pin(async move {
                    let actor_ctx = match services.control_actor(&actor).await {
                        Ok(ctx) => ctx,
                        Err(error) => {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    };
                    let resolved = match services.resolve_control_root(target).await {
                        Ok(resolved) => resolved,
                        Err(error) => {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    };
                    let outcome = match services.session_control().release(
                        actor_ctx.client_id,
                        request_entity,
                        target,
                        &resolved.root,
                        resolved.root_target_id,
                        expected_epoch,
                    ) {
                        Ok(outcome) => outcome,
                        Err(crate::application::session_control::ControlError::RequestConflict) => {
                            return CommandOutcome::Result(management_error(
                                request,
                                "request_conflict",
                            ));
                        }
                        Err(crate::application::session_control::ControlError::NotControlled) => {
                            return CommandOutcome::Result(management_error(
                                request,
                                "session_not_controlled",
                            ));
                        }
                    };
                    if outcome.state_changed {
                        let audit_target = format!(
                            "{}:{}",
                            outcome.snapshot.source_id.encode(),
                            outcome.snapshot.root_native_session_id
                        );
                        let outcome_json = serde_json::json!({
                            "previousController": actor_ctx.client_id.encode(),
                            "previousEpoch": outcome.previous_epoch.to_string(),
                            "controlEpoch": outcome.snapshot.control_epoch.to_string(),
                            "targetId": target.encode(),
                            "rootTargetId": outcome.snapshot.root_target_id.map(crate::domain::EntityId::encode),
                        })
                        .to_string();
                        if let Err(error) = services
                            .append_control_security_audit(
                                Some(&actor_ctx.client_id.encode()),
                                "session.control.release",
                                &audit_target,
                                &outcome_json,
                                control_audit_timestamp_seconds(),
                            )
                            .await
                        {
                            return failure(&state, &actor, request, QueueReadFailure::Registry(error)).await;
                        }
                    }
                    CommandOutcome::Result(management_success(
                        request,
                        &control_snapshot_json(&outcome.snapshot),
                    ))
                }));
                return None;
            }
            SessionAction::ApprovalStatus { approval_id } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let approval =
                    crate::domain::approval_owner::ApprovalId::from_entity(id(*approval_id));
                pending.push(Box::pin(async move {
                    match services.approval_status(&actor, approval).await {
                        Ok(status) => CommandOutcome::Result(management_success(
                            request,
                            &approval_status_result(&status),
                        )),
                        Err(error) => approval_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::NativeReleaseRegister { target_id } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let request_id = id(command.request_id());
                let target = id(*target_id);
                pending.push(Box::pin(async move {
                    match services
                        .register_native_release(&actor, request_id, target)
                        .await
                    {
                        Ok(operation) => CommandOutcome::Result(management_success(
                            request,
                            &release_registered_result(operation, target),
                        )),
                        Err(error) => release_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::NativeRelease {
                release_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let operation = id(*release_operation_id);
                pending.push(Box::pin(async move {
                    match services.native_release(&actor, operation).await {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &release_snapshot_result(&value),
                        )),
                        Err(error) => release_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::NativeReleaseStatus {
                release_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let state = state.clone();
                let actor = actor.clone();
                let operation = id(*release_operation_id);
                pending.push(Box::pin(async move {
                    match services.native_release_status(&actor, operation).await {
                        Ok(value) => CommandOutcome::Result(management_success(
                            request,
                            &release_snapshot_result(&value),
                        )),
                        Err(error) => release_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SourceSessionRegister {
                selection,
                operation,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let selection = match build_source_selection(selection) {
                    Ok(selection) => selection,
                    Err(code) => {
                        return Some(CommandOutcome::Result(management_error(request, code)));
                    }
                };
                let action = match operation {
                    session_protocol::SourceOperationInput::Discover { cursor } => {
                        crate::domain::source_sessions::SourceSessionAction::Discover {
                            cursor: cursor.clone(),
                        }
                    }
                    session_protocol::SourceOperationInput::Create {} => {
                        crate::domain::source_sessions::SourceSessionAction::Create
                    }
                };
                let operation_id = source_session_operation_id(actor, request);
                let client = attachment.client.context.client();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services
                        .source_sessions()
                        .register_with_id(&client, operation_id, selection, action)
                        .await
                    {
                        Ok(metadata) => {
                            let mut body = source_session_registration_json(&metadata);
                            body["state"] = "registered".into();
                            CommandOutcome::Result(management_success(request, &body))
                        }
                        Err(error) => source_session_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SourceSessionExecute {
                source_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let operation_id = crate::domain::source_sessions::SourceSessionOperationId::new(
                    id(*source_operation_id),
                );
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let receipt = match services
                        .source_sessions()
                        .execute(&client, operation_id)
                        .await
                    {
                        Ok(receipt) => receipt,
                        Err(error) => {
                            return source_session_failure(&state, &actor, request, error).await;
                        }
                    };
                    match receipt.wait().await {
                        Ok(fact) => CommandOutcome::Result(management_success(
                            request,
                            &source_session_fact_result(&fact),
                        )),
                        Err(error) => source_session_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SourceSessionStatus {
                source_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let operation_id = crate::domain::source_sessions::SourceSessionOperationId::new(
                    id(*source_operation_id),
                );
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services
                        .source_sessions()
                        .query(&client, operation_id)
                        .await
                    {
                        Ok(fact) => CommandOutcome::Result(management_success(
                            request,
                            &source_session_fact_result(&fact),
                        )),
                        Err(error) => source_session_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SourceSessionReconcile {
                source_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let operation_id = crate::domain::source_sessions::SourceSessionOperationId::new(
                    id(*source_operation_id),
                );
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services
                        .source_sessions()
                        .reconcile(&client, operation_id)
                        .await
                    {
                        Ok(fact) => CommandOutcome::Result(management_success(
                            request,
                            &source_session_fact_result(&fact),
                        )),
                        Err(error) => source_session_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::SourceSessionResultOpen {
                source_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let operation_id = crate::domain::source_sessions::SourceSessionOperationId::new(
                    id(*source_operation_id),
                );
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let transfer = match services
                        .source_sessions()
                        .open_result(&client, operation_id)
                        .await
                    {
                        Ok(transfer) => transfer,
                        Err(error) => {
                            return source_session_failure(&state, &actor, request, error).await;
                        }
                    };
                    let descriptor = match serde_json::to_value(transfer.descriptor()) {
                        Ok(descriptor) => descriptor,
                        Err(_) => {
                            return CommandOutcome::Close(close(CLOSE_INTERNAL, "internal error"));
                        }
                    };
                    if let Err(error) = attachment.attach_document(transfer, &outbound) {
                        return view_failure(&state, &actor, request, error).await;
                    }
                    CommandOutcome::Result(management_success(request, &descriptor))
                }));
                return None;
            }
            SessionAction::SourceSessionResultRelease {
                source_operation_id,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let operation_id = crate::domain::source_sessions::SourceSessionOperationId::new(
                    id(*source_operation_id),
                );
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    match services
                        .source_sessions()
                        .release_result(&client, operation_id)
                        .await
                    {
                        Ok(()) => CommandOutcome::Result(management_success(
                            request,
                            &serde_json::json!({"released": true}),
                        )),
                        Err(error) => source_session_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            action @ (SessionAction::HistoryPage(_)
            | SessionAction::ViewOpen { page: Some(_), .. }
            | SessionAction::ViewOpenSource { page: Some(_), .. }) => {
                use crate::node::native_views::{
                    SocketRebindTarget,
                    history::{HistoryError, HistoryRequest},
                };
                use session_protocol::{HistoryMode, HistoryPageInput, HistoryTarget};
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let (input, open) = match action {
                    SessionAction::HistoryPage(input) => (input.clone(), None),
                    SessionAction::ViewOpen {
                        target_id,
                        reason,
                        page: Some(page),
                    } => (
                        HistoryPageInput {
                            target: HistoryTarget::Target {
                                target_id: *target_id,
                            },
                            mode: HistoryMode::First {},
                            turns: page.turns,
                            child: None,
                        },
                        Some(read_reason(*reason)),
                    ),
                    SessionAction::ViewOpenSource {
                        selection,
                        native_session_id,
                        reason,
                        page: Some(page),
                    } => (
                        HistoryPageInput {
                            target: HistoryTarget::Source {
                                selection: selection.clone(),
                                native_session_id: native_session_id.clone(),
                            },
                            mode: HistoryMode::First {},
                            turns: page.turns,
                            child: None,
                        },
                        Some(read_reason(*reason)),
                    ),
                    _ => {
                        return Some(CommandOutcome::Result(management_error(
                            request,
                            "invalid_request",
                        )));
                    }
                };
                let target = match &input.target {
                    HistoryTarget::Target { target_id } => {
                        SocketRebindTarget::Target(id(*target_id))
                    }
                    HistoryTarget::Source {
                        selection,
                        native_session_id,
                    } => match build_source_selection(selection) {
                        Ok(selection) => SocketRebindTarget::Source {
                            selection,
                            native_session_id: native_session_id.clone(),
                        },
                        Err(code) => {
                            return Some(CommandOutcome::Result(management_error(request, code)));
                        }
                    },
                };
                let client = attachment.client.context.client();
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let req = HistoryRequest {
                        input,
                        open,
                        identity: state.identity.clone(),
                    };
                    let read = match services.views.history(&client, target, req).await {
                        Ok(value) => value,
                        Err(HistoryError::View(error)) => {
                            return view_failure(&state, &actor, request, error).await;
                        }
                        Err(HistoryError::Page(error)) => {
                            use crate::domain::history_page::PageError;
                            let code = match error {
                                PageError::InvalidRequest => "invalid_request",
                                PageError::InvalidCursor => "invalid_cursor",
                                PageError::StaleCursor => "stale_cursor",
                                PageError::NotFound => "not_found",
                                PageError::UnsupportedCapability => "unsupported_capability",
                            };
                            return CommandOutcome::Result(management_error(request, code));
                        }
                    };
                    let response = view_opened(request, &read);
                    match attachment.attach_view_read(read, &outbound) {
                        Ok(()) => CommandOutcome::Result(management_success(request, &response)),
                        Err(error) => view_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ViewOpen {
                target_id,
                reason,
                page: None,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let client = attachment.client.context.client();
                let target = id(*target_id);
                let reason = read_reason(*reason);
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let read = match services.open_view(&client, target, reason).await {
                        Ok(read) => read,
                        Err(error) => return view_failure(&state, &actor, request, error).await,
                    };
                    let response = view_opened(request, &read);
                    match attachment.attach_view_read(read, &outbound) {
                        Ok(()) => CommandOutcome::Result(management_success(request, &response)),
                        Err(error) => view_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ViewOpenSource {
                selection,
                native_session_id,
                reason,
                page: None,
            } => {
                let Some(services) = state.session_services.get().cloned() else {
                    return Some(CommandOutcome::Result(management_error(
                        request,
                        "product_not_ready",
                    )));
                };
                let selection = match build_source_selection(selection) {
                    Ok(selection) => selection,
                    Err(code) => {
                        return Some(CommandOutcome::Result(management_error(request, code)));
                    }
                };
                let client = attachment.client.context.client();
                let native_session_id = native_session_id.clone();
                let reason = read_reason(*reason);
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                pending.push(Box::pin(async move {
                    let read = match services
                        .open_source_view(&client, selection, native_session_id, reason)
                        .await
                    {
                        Ok(read) => read,
                        Err(error) => return view_failure(&state, &actor, request, error).await,
                    };
                    let response = view_opened(request, &read);
                    match attachment.attach_view_read(read, &outbound) {
                        Ok(()) => CommandOutcome::Result(management_success(request, &response)),
                        Err(error) => view_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            SessionAction::ViewClose { stream_id } => {
                let attachment = attachment.clone();
                let stream_id = *stream_id;
                pending.push(Box::pin(async move {
                    match attachment.close_view_read(stream_id).await {
                        Ok(receipt) => CommandOutcome::Result(management_success(
                            request,
                            &view_close_result(&receipt),
                        )),
                        Err(error) => CommandOutcome::Result(management_error(
                            request,
                            view_error_code(error),
                        )),
                    }
                }));
                return None;
            }
            SessionAction::ViewRebind { stream_id } => {
                let attachment = attachment.clone();
                let outbound = outbound.clone();
                let state = state.clone();
                let actor = actor.clone();
                let stream_id = *stream_id;
                pending.push(Box::pin(async move {
                    let read = match attachment.rebind_view_read(stream_id).await {
                        Ok(read) => read,
                        Err(error) => return view_failure(&state, &actor, request, error).await,
                    };
                    let response = view_opened(request, &read);
                    match attachment.attach_view_read(read, &outbound) {
                        Ok(()) => CommandOutcome::Result(management_success(request, &response)),
                        Err(error) => view_failure(&state, &actor, request, error).await,
                    }
                }));
                return None;
            }
            _ => {}
        }
    }
    #[cfg(not(windows))]
    let _ = (command, attachment, outbound, pending, actor);
    Some(CommandOutcome::Result(management_error(
        request,
        "platform_unimplemented",
    )))
}

#[cfg(all(test, windows))]
mod view_error_development;

#[cfg(test)]
mod projection_development {
    use super::*;
    use crate::domain::{
        EntityId,
        native::RuntimeEpoch,
        request_registry::{QueueAction, RegisteredIntent, RegistrationFact, StoredApplying},
    };

    #[test]
    fn queue_read_projection_preserves_large_values_and_typed_retirement() {
        let entity = EntityId::from_bytes([42; 16]);
        let registration = RegistrationId::new(entity);
        let original = RegistrationFact::new(
            registration,
            RegisteredIntent::new(entity, QueueAction::Resume, entity, None, None, None).unwrap(),
            0,
            0,
        )
        .unwrap();
        let applied = fact(
            registration,
            RequestFact::Applying(StoredApplying {
                registration: original.clone(),
                operation: entity,
                runtime: RuntimeEpoch(entity),
                admission_auth_generation: 0,
                begun_at_ms: i64::MIN,
            }),
        );
        assert_eq!(applied["admittedAtMs"], "-9223372036854775808");
        assert_eq!(applied["state"], "applying");
        let settled = fact(
            registration,
            RequestFact::Settled {
                registration: original.clone(),
                outcome: QueueOutcome::Edited {
                    item: entity,
                    revision: u64::MAX,
                },
            },
        );
        assert_eq!(settled["outcome"]["revision"], "18446744073709551615");
        assert_eq!(
            fact(registration, RequestFact::Registered(original.clone()))["state"],
            "registered"
        );
        assert_eq!(
            fact(
                registration,
                RequestFact::Interrupted {
                    registration: original
                }
            )["state"],
            "interrupted"
        );
        assert_eq!(
            fact(registration, RequestFact::Retired),
            serde_json::json!({"registrationId":registration.encode(),"state":"retired"})
        );
        assert_eq!(
            management_error(Id128::new([42; 16]), "request_retired")["ok"],
            false
        );
    }

    #[test]
    fn queue_read_task_slot_allocation_uses_allocation_failed() {
        assert_eq!(
            allocation_code(QueueReadFailure::AllocationFailed),
            Some("allocation_failed")
        );
    }
}
