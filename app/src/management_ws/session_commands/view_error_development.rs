use super::*;
use crate::domain::{
    native_dispatch::{DispatchError, NativeProtocolFailure},
    shared_view::ViewError,
};

#[test]
fn native_view_failure_preserves_dispatch_category() {
    let cases = [
        (DispatchError::InvalidConfiguration, "invalid_configuration"),
        (DispatchError::NodeStopping, "node_stopping"),
        (DispatchError::RuntimeAlreadyStarted, "read_failed"),
        (DispatchError::StorageUnavailable, "read_failed"),
        (DispatchError::CommitUnconfirmed, "read_failed"),
        (
            DispatchError::SqlCloseUnconfirmed {
                commit_confirmed: false,
            },
            "read_failed",
        ),
        (
            DispatchError::SqlCloseUnconfirmed {
                commit_confirmed: true,
            },
            "read_failed",
        ),
        (DispatchError::CorruptState, "read_failed"),
        (DispatchError::Conflict, "read_failed"),
        (DispatchError::InvalidTarget, "invalid_target"),
        (DispatchError::InvalidSource, "invalid_source"),
        (DispatchError::InvalidScope, "invalid_scope"),
        (DispatchError::InvalidResource, "read_failed"),
        (DispatchError::InvalidPromptReservation, "read_failed"),
        (
            DispatchError::LengthUnrepresentable,
            "length_unrepresentable",
        ),
        (DispatchError::StaleSelection, "read_failed"),
        (DispatchError::RevisionChanged, "read_failed"),
        (DispatchError::NotReady, "read_failed"),
        (DispatchError::RequiredStateUnavailable, "read_failed"),
        (DispatchError::SequenceExhausted, "read_failed"),
        (DispatchError::ProtocolUnavailable, "protocol_failure"),
        (DispatchError::ProtocolRejected, "protocol_failure"),
        (DispatchError::EnqueueRejected, "read_failed"),
        (DispatchError::ObservationOrder, "read_failed"),
        (DispatchError::ObservationUnapplied, "read_failed"),
        (DispatchError::ObservationDropped, "read_failed"),
        (DispatchError::ResourceUnconfirmed, "read_failed"),
        (
            DispatchError::SourcePlatform {
                operation: "fixture-source-open",
                code: 5,
            },
            "read_failed",
        ),
        (
            DispatchError::SourcePlatform {
                operation: "fixture-source-close",
                code: -17,
            },
            "read_failed",
        ),
        (DispatchError::Stopped, "source_released"),
    ];
    for (cause, expected) in cases {
        assert_eq!(
            view_error_code(ViewError::NativeFailure(cause)),
            expected,
            "cause={cause:?}"
        );
    }
}

#[test]
fn native_view_protocol_causes_remain_protocol_failures() {
    let causes = [
        NativeProtocolFailure::NotInitialized,
        NativeProtocolFailure::AlreadyInitialized,
        NativeProtocolFailure::InvalidMessage,
        NativeProtocolFailure::IncompatibleVersion,
        NativeProtocolFailure::UnsupportedCapability,
        NativeProtocolFailure::Capacity,
        NativeProtocolFailure::UnmatchedResponse,
        NativeProtocolFailure::DeliveryUnknown,
        NativeProtocolFailure::Deadline,
        NativeProtocolFailure::Io,
        NativeProtocolFailure::InvalidOption,
        NativeProtocolFailure::MissingSnapshot,
        NativeProtocolFailure::SessionReadBusy,
        NativeProtocolFailure::RuntimeUnavailable,
        NativeProtocolFailure::Allocation,
        NativeProtocolFailure::InvalidFence,
        NativeProtocolFailure::InvalidFrame,
    ];
    for cause in causes {
        assert_eq!(
            view_error_code(ViewError::NativeFailure(DispatchError::ProtocolFault(
                cause
            ))),
            "protocol_failure"
        );
    }
}

#[test]
fn direct_view_failure_codes_are_preserved() {
    let cases = [
        (ViewError::Unauthenticated, "revoked"),
        (ViewError::Revoked, "revoked"),
        (ViewError::WrongSocket, "invalid_client"),
        (ViewError::InvalidClient, "invalid_client"),
        (ViewError::InvalidSource, "invalid_source"),
        (ViewError::InvalidConfiguration, "invalid_configuration"),
        (ViewError::Unsupported, "platform_unimplemented"),
        (ViewError::UnsupportedCapability, "unsupported_capability"),
        (ViewError::AlreadyAttached, "already_attached"),
        (ViewError::CaptureUnavailable, "view_unavailable"),
        (ViewError::InvalidStage, "invalid_stage"),
        (ViewError::SourceReplaced, "source_replaced"),
        (ViewError::StreamSealed, "stream_sealed"),
        (ViewError::ProtocolFailure, "protocol_failure"),
        (ViewError::InvalidCredit, "invalid_credit"),
        (ViewError::QueueRejected, "queue_rejected"),
        (ViewError::TransportFailure, "transport_failure"),
        (ViewError::ShutdownUnconfirmed, "shutdown_unconfirmed"),
        (ViewError::InvalidTarget, "invalid_target"),
        (ViewError::NodeStopping, "node_stopping"),
        (ViewError::InvalidScope, "invalid_scope"),
        (ViewError::CaptureGap, "capture_gap"),
        (ViewError::AllocationFailure, "allocation_failed"),
        (ViewError::RandomUnavailable, "random_unavailable"),
        (ViewError::SequenceExhausted, "sequence_exhausted"),
        (ViewError::StaleCursor, "stale_cursor"),
        (ViewError::InvalidCursor, "invalid_cursor"),
        (ViewError::SourceReleased, "source_released"),
        (ViewError::ProjectionFailure, "projection_failure"),
        (ViewError::ReadFailure, "read_failed"),
        (ViewError::LengthUnrepresentable, "length_unrepresentable"),
    ];
    for (error, expected) in cases {
        assert_eq!(view_error_code(error), expected, "error={error:?}");
    }
}

#[test]
fn native_view_error_envelope_exposes_only_fixed_code() {
    let request = Id128::new([73; 16]);
    let cases = [
        (DispatchError::InvalidConfiguration, "invalid_configuration"),
        (
            DispatchError::SourcePlatform {
                operation: "fixture-sensitive-operation",
                code: -1_234_567,
            },
            "read_failed",
        ),
        (
            DispatchError::SourcePlatform {
                operation: "fixture-other-sensitive-operation",
                code: 987_654,
            },
            "read_failed",
        ),
        (
            DispatchError::SqlCloseUnconfirmed {
                commit_confirmed: false,
            },
            "read_failed",
        ),
        (
            DispatchError::SqlCloseUnconfirmed {
                commit_confirmed: true,
            },
            "read_failed",
        ),
    ];
    for (cause, code) in cases {
        let value = management_error(request, view_error_code(ViewError::NativeFailure(cause)));
        let serialized = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serialized).unwrap(),
            serde_json::json!({
                "v": 1,
                "type": "result",
                "requestId": request,
                "ok": false,
                "result": null,
                "error": { "code": code }
            })
        );
    }
}

#[test]
fn r3_source_failure_codes_separate_unsupported_capability_from_platform_unimplemented() {
    use crate::domain::source_sessions::{SourceSessionError, SourceSessionFailure};
    let request = Id128::new([88; 16]);
    let unsupported_cap = SourceSessionFailure::Native(DispatchError::ProtocolFault(
        NativeProtocolFailure::UnsupportedCapability,
    ));
    assert_eq!(
        source_failure_code(&unsupported_cap),
        "unsupported_capability"
    );
    assert_eq!(
        management_error(request, source_failure_code(&unsupported_cap))["error"]["code"],
        "unsupported_capability"
    );
    assert_eq!(
        view_error_code(ViewError::UnsupportedCapability),
        "unsupported_capability"
    );
    assert_eq!(
        view_error_code(ViewError::Unsupported),
        "platform_unimplemented"
    );
    assert_eq!(
        source_failure_code(&SourceSessionFailure::Native(DispatchError::ProtocolFault(
            NativeProtocolFailure::InvalidMessage
        ))),
        "protocol_failure"
    );
    assert_eq!(
        source_failure_code(&SourceSessionFailure::Rejected(
            SourceSessionError::ResultUnavailable
        )),
        "body_unavailable"
    );
}
