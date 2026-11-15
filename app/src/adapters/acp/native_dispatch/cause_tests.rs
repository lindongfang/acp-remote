//! Pure classification vectors; these do not simulate Native IO or mint capabilities.
use super::*;

#[test]
fn native_cause_all_23_acp_categories_keep_the_independent_contract() {
    fn copy_eq<T: Copy + Eq>() {}
    copy_eq::<d::NativeProtocolFailure>();
    copy_eq::<d::DispatchError>();
    let cases = [
        (
            AcpError::InvalidRequestScope,
            d::DispatchError::InvalidScope,
        ),
        (
            AcpError::NotInitialized,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::NotInitialized),
        ),
        (
            AcpError::AlreadyInitialized,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::AlreadyInitialized),
        ),
        (
            AcpError::InvalidMessage,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidMessage),
        ),
        (
            AcpError::IncompatibleVersion,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::IncompatibleVersion),
        ),
        (
            AcpError::UnsupportedCapability,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::UnsupportedCapability),
        ),
        (
            AcpError::Capacity,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Capacity),
        ),
        (
            AcpError::UnmatchedResponse,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::UnmatchedResponse),
        ),
        (
            AcpError::DeliveryUnknown,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::DeliveryUnknown),
        ),
        (AcpError::Closed, d::DispatchError::Stopped),
        (
            AcpError::Deadline,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Deadline),
        ),
        (
            AcpError::Io,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Io),
        ),
        (
            AcpError::InvalidOption,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidOption),
        ),
        (
            AcpError::MissingSnapshot,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::MissingSnapshot),
        ),
        (
            AcpError::SessionReadBusy,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::SessionReadBusy),
        ),
        (
            AcpError::RuntimeUnavailable,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::RuntimeUnavailable),
        ),
        (
            AcpError::Allocation,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Allocation),
        ),
        (AcpError::Cancelled, d::DispatchError::Stopped),
        (
            AcpError::InvalidFence,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidFence),
        ),
        (
            AcpError::InvalidFrame,
            d::DispatchError::ProtocolFault(d::NativeProtocolFailure::InvalidFrame),
        ),
        (
            AcpError::ConfigurationIncomplete,
            d::DispatchError::InvalidConfiguration,
        ),
        (
            AcpError::ConfigurationVersionMismatch,
            d::DispatchError::InvalidConfiguration,
        ),
        (
            AcpError::UnencodableConfiguration,
            d::DispatchError::InvalidConfiguration,
        ),
    ];
    assert_eq!(cases.len(), 23);
    for (original, expected) in cases {
        assert_eq!(error(original), expected, "{original:?}");
        assert!(!expected.to_string().is_empty());
    }
}

#[test]
fn native_cause_protocol_and_platform_failures_stay_distinct() {
    let platform = d::DispatchError::SourcePlatform {
        operation: "synthetic-static-operation",
        code: 5,
    };
    let errors = [
        error(AcpError::Io),
        error(AcpError::Capacity),
        error(AcpError::Allocation),
        error(AcpError::Deadline),
        error(AcpError::RuntimeUnavailable),
        platform,
    ];
    for actual in errors {
        assert_ne!(actual, error(AcpError::UnsupportedCapability));
        let view = crate::domain::shared_view::ViewError::NativeFailure(actual);
        assert!(
            matches!(view, crate::domain::shared_view::ViewError::NativeFailure(original) if original == actual)
        );
    }
    assert_ne!(
        d::DispatchError::ProtocolRejected,
        error(AcpError::UnsupportedCapability)
    );
}

// Independent ACP producers + real mapper seam. These are not Native/R3/SQL grants.
#[test]
fn ndt_ncae004_actual_no_runtime_preserves_its_local_cause() {
    assert!(tokio::runtime::Handle::try_current().is_err());
    let original = Connection::spawn(tokio::io::empty(), tokio::io::sink())
        .expect_err("no runtime must reject before any IO task is adopted");
    assert_eq!(original, AcpError::RuntimeUnavailable);
    assert_eq!(
        error(original),
        d::DispatchError::ProtocolFault(d::NativeProtocolFailure::RuntimeUnavailable)
    );
    assert_ne!(error(original), error(AcpError::UnsupportedCapability));
    eprintln!("NCAE004 actual_runtime_absent=true owner_adopted=false mapper=RuntimeUnavailable");
}

#[tokio::test]
async fn ndt_ncae004_actual_deadline_and_reinitialize_keep_original_connection() {
    use std::time::Duration;
    let (local, peer) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(local);
    let (connection, receiver, owner) = Connection::spawn(reader, writer).unwrap();
    let scope = connection.scope();
    let original_deadline = connection
        .reserve_preferred_initialize(Instant::now())
        .expect_err("actually elapsed first reservation");
    let pending_after_deadline = connection.pending_count();
    let reservation = connection
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let reservation_scope = reservation.scope();
    let probe = reservation.probe();
    let original_repeat = connection
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(5))
        .expect_err("same original connection initializes only once");
    let pending_after_repeat = connection.pending_count();
    drop(reservation);
    let zero = probe.progress();
    let pending_after_drop = connection.pending_count();
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(5))
        .await;
    drop((receiver, peer));
    assert!(report.joined, "{report:?}");
    assert!(report.unresolved_requests.is_empty());
    assert_eq!(scope, reservation_scope);
    assert_eq!(original_deadline, AcpError::Deadline);
    assert_eq!(
        error(original_deadline),
        d::DispatchError::ProtocolFault(d::NativeProtocolFailure::Deadline)
    );
    assert_eq!(original_repeat, AcpError::AlreadyInitialized);
    assert_eq!(
        error(original_repeat),
        d::DispatchError::ProtocolFault(d::NativeProtocolFailure::AlreadyInitialized)
    );
    assert_eq!(pending_after_deadline, 0);
    assert_eq!(pending_after_repeat, 1);
    assert_eq!(pending_after_drop, 0);
    assert_eq!(
        zero,
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Abandoned
        })
    );
    eprintln!(
        "NCAE004 actual_same_connection=true expired_reserve=Deadline repeated_reserve=AlreadyInitialized pending=0,1,0 actual_original_reservation=AbandonedZero original_io_joined=true"
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "This test preserves the same actual connection, original reservations, peer responses and normal IO shutdown in one ordered sequence; no Native grants are constructed."
)]
async fn ndt_ncae003_actual_no_image_still_writes_text_on_same_negotiated_scope() {
    use crate::adapters::acp::test_support::{Allow, parse_session, raw_value};
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::time::timeout;
    let deadline = || Instant::now() + Duration::from_secs(5);
    let (local, mut peer) = tokio::io::duplex(65536);
    let (reader, writer) = tokio::io::split(local);
    let (connection, mut receiver, owner) = Connection::spawn(reader, writer).unwrap();
    let reservation = connection
        .reserve_initialize(Version::V2, deadline())
        .unwrap();
    let frame = reservation.encode().unwrap();
    let written = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), written.wait())
            .await
            .unwrap(),
        WriteReport::Written
    );
    let mut init_line = String::new();
    timeout(
        Duration::from_secs(5),
        BufReader::new(&mut peer).read_line(&mut init_line),
    )
    .await
    .unwrap()
    .unwrap();
    let init: serde_json::Value = serde_json::from_str(&init_line).unwrap();
    let mut response = serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":2,"info":{"name":"independent-seam","version":"fixed"},"capabilities":{"session":{}}}})).unwrap();
    response.push(b'\n');
    peer.write_all(&response).await.unwrap();
    peer.flush().await.unwrap();
    let initialized_event = timeout(Duration::from_secs(5), receiver.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let init_same_scope = initialized_event.scope() == connection.scope();
    assert!(matches!(
        initialized_event.incoming(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    let strategy = connection.strategy().unwrap();
    let image_advertised = strategy.negotiated().capabilities.image;
    let session = parse_session(&serde_json::json!("independent-seam-session")).unwrap();
    let image =
        raw_value(&serde_json::json!([{"type":"image","data":"AA==","mimeType":"image/png"}]));
    let original = strategy
        .prompt(session.clone(), image, ExtensionFields::empty().unwrap())
        .expect_err("image is truly not advertised by the initialized_event initialize response");
    let pending_after_unsupported = connection.pending_count();
    let prompt = strategy.prompt(session, raw_value(&serde_json::json!([{"type":"text","text":"independent text survives optional image refusal"}])), ExtensionFields::empty().unwrap()).unwrap();
    let reservation = connection
        .reserve_request(prompt, ResponseDeadline::Unbounded)
        .unwrap();
    let frame = reservation.encode().unwrap();
    let written = connection
        .enqueue(reservation, frame, Arc::new(Allow), deadline())
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), written.wait())
            .await
            .unwrap(),
        WriteReport::Written
    );
    let mut prompt_line = String::new();
    timeout(
        Duration::from_secs(5),
        BufReader::new(&mut peer).read_line(&mut prompt_line),
    )
    .await
    .unwrap()
    .unwrap();
    let prompt: serde_json::Value = serde_json::from_str(&prompt_line).unwrap();
    let mut response = serde_json::to_vec(
        &serde_json::json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"messageId":"independent-text-response"}}),
    )
    .unwrap();
    response.push(b'\n');
    peer.write_all(&response).await.unwrap();
    peer.flush().await.unwrap();
    let terminal = timeout(Duration::from_secs(5), receiver.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let terminal_same_scope = terminal.scope() == connection.scope();
    let prompt_response = matches!(
        terminal.incoming(),
        Incoming::Response {
            response: Response::Prompt(_),
            ..
        }
    );
    let pending_after_response = connection.pending_count();
    let report = owner.shutdown(deadline()).await;
    drop((initialized_event, terminal, receiver, peer));
    assert!(report.joined, "{report:?}");
    assert!(report.unresolved_requests.is_empty());
    assert!(init_same_scope && terminal_same_scope);
    assert!(!image_advertised);
    assert_eq!(original, AcpError::UnsupportedCapability);
    assert_eq!(
        error(original),
        d::DispatchError::ProtocolFault(d::NativeProtocolFailure::UnsupportedCapability)
    );
    assert_eq!(pending_after_unsupported, 0);
    assert_eq!(init["method"], "initialize");
    assert_eq!(prompt["method"], "session/prompt");
    assert_eq!(prompt["params"]["prompt"][0]["type"], "text");
    assert!(prompt_response);
    assert_eq!(pending_after_response, 0);
    eprintln!(
        "NCAE003 actual_initialize_frames=1 actual_text_frames=1 optional_image_request_not_reserved=true original_received_scope=true prompt_response_received=true pending=0 original_io_joined=true; ACP+mapper seam, no Native/R3/SQL claim"
    );
}
