#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;

const ID: &str = "AAECAwQFBgcICQoLDA0ODw";
const OTHER: &str = "AAAAAAAAAAAAAAAAAAAAAA";

fn envelope(action: &str, payload: &str) -> String {
    format!(
        r#"{{"v":1,"type":"command","requestId":"{ID}","action":"{action}","payload":{payload}}}"#
    )
}

fn valid_selection_json() -> String {
    format!(
        r#"{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":"C:/work/repo"}}"#
    )
}

#[test]
fn session_codec_development_exact_unsigned_metadata_survives_both_directions() {
    let wire = envelope(
        "input.register",
        &format!(
            r#"{{"targetId":"{ID}","operation":{{"kind":"edit","itemId":"{OTHER}","expectedRevision":"18446744073709551615","bodyId":"{ID}"}}}}"#
        ),
    );
    let parsed = parse_session_command(wire.as_bytes()).unwrap();
    let SessionAction::InputRegister {
        operation: QueueMutation::Edit {
            expected_revision, ..
        },
        ..
    } = parsed.action()
    else {
        panic!("original action and mutation must be retained")
    };
    assert_eq!(expected_revision.as_u64(), u64::MAX);
    let encoded = encode_session_command(&parsed).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&wire).unwrap(),
        serde_json::from_str::<Value>(&encoded).unwrap()
    );
    assert_eq!(parse_session_command(encoded.as_bytes()).unwrap(), parsed);
    for value in ["0", "1", "9007199254740993", "18446744073709551615"] {
        let exact = DecimalU64::parse(value).unwrap();
        assert_eq!(
            serde_json::to_string(&exact).unwrap(),
            format!("\"{value}\"")
        );
    }
    for invalid in [
        "",
        "00",
        "01",
        "+1",
        "-0",
        "-1",
        "1.0",
        "1e3",
        " 1",
        "1\n",
        "18446744073709551616",
    ] {
        assert_eq!(
            DecimalU64::parse(invalid),
            Err(SessionProtocolError::InvalidDecimal)
        );
    }
}

#[test]
fn session_codec_development_constructed_empty_and_nullable_payloads_remain_explicit() {
    let request = Id128::parse_base64url(ID).unwrap();
    for action in [
        SessionAction::ProductStatus {},
        SessionAction::CatalogUploadBegin {},
        SessionAction::CatalogInventory {},
        SessionAction::ApprovalList {},
        SessionAction::InputConsume {
            target_id: request,
            registration_id: request,
            body_id: None,
            expected_control_epoch: None,
        },
        SessionAction::InputRegister {
            target_id: request,
            operation: QueueMutation::Resume {},
            expected_control_epoch: None,
        },
    ] {
        let command = SessionCommand::new(request, action.clone());
        let encoded = encode_session_command(&command).unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert!(value["payload"].is_object());
        assert_eq!(parse_session_command(encoded.as_bytes()).unwrap(), command);
        if matches!(action, SessionAction::InputConsume { .. }) {
            assert_eq!(
                value["payload"].as_object().unwrap().get("bodyId"),
                Some(&Value::Null)
            );
        }
    }
}

#[test]
fn session_codec_development_existing_control_boundary_and_static_errors_remain() {
    let wire = envelope("input.queue", &format!(r#"{{"targetId":"{ID}"}}"#));
    let mut padded = wire.as_bytes().to_vec();
    padded.resize(super::super::MAX_MANAGEMENT_FRAME_BYTES, b' ');
    assert!(parse_session_command(&padded).is_ok());
    padded.push(b' ');
    assert_eq!(
        parse_session_command(&padded),
        Err(SessionProtocolError::InvalidCommand)
    );
    for invalid in [
        wire.replace("\"v\":1", "\"v\":1.0"),
        wire.replace("\"v\":1", "\"v\":1e0"),
        wire.replace("\"v\":1", "\"v\":1,\"\\u0076\":1"),
        envelope(
            "input.queue",
            &format!(r#"{{"targetId":"{ID}","actor":"PRIVATE_CANARY"}}"#),
        ),
        envelope(
            "input.consume",
            &format!(r#"{{"targetId":"{ID}","registrationId":"{OTHER}"}}"#),
        ),
        envelope("product.subscribe", "{}"),
        format!("{wire} {{}}"),
    ] {
        let error = parse_session_command(invalid.as_bytes()).unwrap_err();
        assert_eq!(error, SessionProtocolError::InvalidCommand);
        assert_eq!(error.to_string(), "invalid session command");
        assert!(!format!("{error:?}").contains("PRIVATE_CANARY"));
    }
    assert_eq!(
        parse_session_command(&[0xff]),
        Err(SessionProtocolError::InvalidCommand)
    );
}

#[test]
fn session_codec_development_view_close_requires_exactly_one_stream_id() {
    let wire = envelope("view.close", &format!(r#"{{"streamId":"{ID}"}}"#));
    let command = parse_session_command(wire.as_bytes()).expect("view.close is a session command");
    assert_eq!(
        serde_json::from_str::<Value>(&encode_session_command(&command).unwrap()).unwrap(),
        serde_json::from_str::<Value>(&wire).unwrap()
    );

    for invalid in [
        envelope("view.close", "{}"),
        envelope(
            "view.close",
            &format!(r#"{{"streamId":"{ID}","targetId":"{OTHER}"}}"#),
        ),
        envelope("view.close", r#"{"streamId":42}"#),
        envelope("view.close", r#"{"streamId":"not-an-id"}"#),
    ] {
        assert!(parse_session_command(invalid.as_bytes()).is_err());
    }
}

#[test]
fn session_codec_r3_source_session_and_view_open_source_round_trip_and_reject_invalid() {
    let selection = valid_selection_json();
    let max_cursor = "c".repeat(MAX_SOURCE_TEXT_BYTES);
    let max_session = "s".repeat(MAX_SOURCE_TEXT_BYTES);
    let max_path = "p".repeat(MAX_SOURCE_TEXT_BYTES);
    let max_selection = format!(
        r#"{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":"{max_path}"}}"#
    );

    let valid_wires = [
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{selection},"operation":{{"kind":"discover","cursor":null}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{max_selection},"operation":{{"kind":"discover","cursor":"{max_cursor}"}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(r#"{{"selection":{selection},"operation":{{"kind":"create"}}}}"#),
        ),
        envelope(
            "source.session.execute",
            &format!(r#"{{"sourceOperationId":"{ID}"}}"#),
        ),
        envelope(
            "source.session.status",
            &format!(r#"{{"sourceOperationId":"{ID}"}}"#),
        ),
        envelope(
            "source.session.reconcile",
            &format!(r#"{{"sourceOperationId":"{ID}"}}"#),
        ),
        envelope(
            "source.session.result.open",
            &format!(r#"{{"sourceOperationId":"{ID}"}}"#),
        ),
        envelope(
            "source.session.result.release",
            &format!(r#"{{"sourceOperationId":"{ID}"}}"#),
        ),
        envelope(
            "view.open_source",
            &format!(
                r#"{{"selection":{selection},"nativeSessionId":"{max_session}","reason":"first_open"}}"#
            ),
        ),
    ];

    for wire in valid_wires {
        let parsed = parse_session_command(wire.as_bytes())
            .unwrap_or_else(|err| panic!("expected valid R3 command {wire}: {err}"));
        let encoded = encode_session_command(&parsed).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&wire).unwrap(),
            serde_json::from_str::<Value>(&encoded).unwrap()
        );
        assert_eq!(parse_session_command(encoded.as_bytes()).unwrap(), parsed);
    }

    let over_text = "x".repeat(MAX_SOURCE_TEXT_BYTES + 1);
    let over_utf8 = "中".repeat((MAX_SOURCE_TEXT_BYTES / 3) + 1);
    assert!(over_utf8.len() > MAX_SOURCE_TEXT_BYTES);

    let invalid_wires = [
        // Missing fields
        envelope(
            "source.session.register",
            &format!(r#"{{"selection":{selection}}}"#),
        ),
        envelope(
            "source.session.register",
            &format!(r#"{{"selection":{selection},"operation":{{"kind":"discover"}}}}"#),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}"}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope("source.session.execute", "{}"),
        envelope("source.session.status", "{}"),
        envelope("source.session.reconcile", "{}"),
        envelope("source.session.result.open", "{}"),
        envelope("source.session.result.release", "{}"),
        envelope(
            "view.open_source",
            &format!(r#"{{"selection":{selection},"nativeSessionId":"sess-1"}}"#),
        ),
        // Extra unknown fields
        envelope(
            "source.session.register",
            &format!(r#"{{"selection":{selection},"operation":{{"kind":"create","extra":1}}}}"#),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{selection},"operation":{{"kind":"create"}},"extra":"PRIVATE_CANARY"}}"#
            ),
        ),
        envelope(
            "source.session.execute",
            &format!(r#"{{"sourceOperationId":"{ID}","extra":true}}"#),
        ),
        envelope(
            "view.open_source",
            &format!(
                r#"{{"selection":{selection},"nativeSessionId":"sess-1","reason":"first_open","extra":1}}"#
            ),
        ),
        // Invalid Id128 / DecimalU64 / reason
        envelope(
            "source.session.execute",
            r#"{"sourceOperationId":"not-base64url"}"#,
        ),
        envelope(
            "source.session.status",
            r#"{"sourceOperationId":"not-base64url"}"#,
        ),
        envelope(
            "source.session.reconcile",
            r#"{"sourceOperationId":"not-base64url"}"#,
        ),
        envelope(
            "source.session.result.open",
            r#"{"sourceOperationId":"not-base64url"}"#,
        ),
        envelope(
            "source.session.result.release",
            r#"{"sourceOperationId":"not-base64url"}"#,
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"bad","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":"C:/repo"}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"01","workspaceId":"{OTHER}","launchPath":"C:/repo"}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope(
            "view.open_source",
            &format!(r#"{{"selection":{selection},"nativeSessionId":"sess-1","reason":"auto"}}"#),
        ),
        // Boundary checks for launchPath, discover.cursor, nativeSessionId
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":""}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":"C:/repo\u0000x"}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{{"sourceId":"{ID}","harnessId":"{OTHER}","profileId":"{ID}","profileRevision":"7","workspaceId":"{OTHER}","launchPath":"{over_utf8}"}},"operation":{{"kind":"create"}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{selection},"operation":{{"kind":"discover","cursor":""}}}}"#
            ),
        ),
        envelope(
            "source.session.register",
            &format!(
                r#"{{"selection":{selection},"operation":{{"kind":"discover","cursor":"{over_text}"}}}}"#
            ),
        ),
        envelope(
            "view.open_source",
            &format!(r#"{{"selection":{selection},"nativeSessionId":"","reason":"first_open"}}"#),
        ),
        envelope(
            "view.open_source",
            &format!(
                r#"{{"selection":{selection},"nativeSessionId":"sess\u00001","reason":"first_open"}}"#
            ),
        ),
        envelope(
            "view.open_source",
            &format!(
                r#"{{"selection":{selection},"nativeSessionId":"{over_text}","reason":"first_open"}}"#
            ),
        ),
    ];

    for wire in invalid_wires {
        let error = parse_session_command(wire.as_bytes())
            .expect_err(&format!("expected R3 command rejection for {wire}"));
        assert!(!format!("{error:?}").contains("PRIVATE_CANARY"));
    }
}

#[test]
fn session_codec_catalog_registration_and_inventory_round_trip_and_reject_invalid() {
    let max_text = "w".repeat(MAX_SOURCE_TEXT_BYTES);
    let valid_wires = [
        envelope("workspace.register", r#"{"path":"C:/work/repo"}"#),
        envelope("workspace.register", &format!(r#"{{"path":"{max_text}"}}"#)),
        envelope(
            "target.register",
            &format!(
                r#"{{"sourceId":"{ID}","nativeSessionId":"codex-sess-1","workspaceId":"{OTHER}"}}"#
            ),
        ),
        envelope(
            "target.register",
            &format!(
                r#"{{"sourceId":"{ID}","nativeSessionId":"{max_text}","workspaceId":"{OTHER}"}}"#
            ),
        ),
        envelope("catalog.inventory", "{}"),
    ];

    for wire in valid_wires {
        let parsed = parse_session_command(wire.as_bytes())
            .unwrap_or_else(|err| panic!("expected valid catalog command {wire}: {err}"));
        let encoded = encode_session_command(&parsed).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&wire).unwrap(),
            serde_json::from_str::<Value>(&encoded).unwrap()
        );
        assert_eq!(parse_session_command(encoded.as_bytes()).unwrap(), parsed);
    }

    let over_text = "x".repeat(MAX_SOURCE_TEXT_BYTES + 1);
    let over_utf8 = "中".repeat((MAX_SOURCE_TEXT_BYTES / 3) + 1);
    let invalid_cases = [
        // Missing fields
        (
            envelope("workspace.register", "{}"),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(r#"{{"sourceId":"{ID}","nativeSessionId":"sess"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(r#"{{"sourceId":"{ID}","workspaceId":"{OTHER}"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(r#"{{"nativeSessionId":"sess","workspaceId":"{OTHER}"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        // Extra unknown fields
        (
            envelope(
                "workspace.register",
                r#"{"path":"C:/work","extra":"PRIVATE_CANARY"}"#,
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(
                    r#"{{"sourceId":"{ID}","nativeSessionId":"sess","workspaceId":"{OTHER}","extra":"PRIVATE_CANARY"}}"#
                ),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope("catalog.inventory", r#"{"extra":"PRIVATE_CANARY"}"#),
            SessionProtocolError::InvalidCommand,
        ),
        // Empty, NUL, over-length, invalid IDs
        (
            envelope("workspace.register", r#"{"path":""}"#),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope("workspace.register", r#"{"path":"C:/work\u0000sub"}"#),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "workspace.register",
                &format!(r#"{{"path":"{over_text}"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "workspace.register",
                &format!(r#"{{"path":"{over_utf8}"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope("workspace.register", r#"{"path":123}"#),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(
                    r#"{{"sourceId":"bad","nativeSessionId":"sess","workspaceId":"{OTHER}"}}"#
                ),
            ),
            SessionProtocolError::InvalidId,
        ),
        (
            envelope(
                "target.register",
                &format!(r#"{{"sourceId":"{ID}","nativeSessionId":"sess","workspaceId":"bad"}}"#),
            ),
            SessionProtocolError::InvalidId,
        ),
        (
            envelope(
                "target.register",
                &format!(r#"{{"sourceId":"{ID}","nativeSessionId":"","workspaceId":"{OTHER}"}}"#),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(
                    r#"{{"sourceId":"{ID}","nativeSessionId":"sess\u00001","workspaceId":"{OTHER}"}}"#
                ),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(
                    r#"{{"sourceId":"{ID}","nativeSessionId":"{over_text}","workspaceId":"{OTHER}"}}"#
                ),
            ),
            SessionProtocolError::InvalidCommand,
        ),
        (
            envelope(
                "target.register",
                &format!(
                    r#"{{"sourceId":"{ID}","nativeSessionId":"{over_utf8}","workspaceId":"{OTHER}"}}"#
                ),
            ),
            SessionProtocolError::InvalidCommand,
        ),
    ];

    for (wire, expected) in invalid_cases {
        let error = parse_session_command(wire.as_bytes())
            .expect_err(&format!("expected catalog command rejection for {wire}"));
        assert_eq!(error, expected);
        assert!(!format!("{error:?}").contains("PRIVATE_CANARY"));
    }
}

#[test]
fn sct_001_session_control_commands_and_expected_control_epoch_strict_contract() {
    let valid_wires = [
        envelope(
            "session.control.inspect",
            &format!(r#"{{"targetId":"{ID}"}}"#),
        ),
        envelope(
            "session.control.takeover",
            &format!(r#"{{"targetId":"{ID}"}}"#),
        ),
        envelope(
            "session.control.release",
            &format!(r#"{{"targetId":"{ID}"}}"#),
        ),
        envelope(
            "session.control.release",
            &format!(r#"{{"targetId":"{ID}","expectedControlEpoch":"42"}}"#),
        ),
        envelope(
            "input.register",
            &format!(
                r#"{{"targetId":"{ID}","operation":{{"kind":"resume"}},"expectedControlEpoch":"7"}}"#
            ),
        ),
        envelope(
            "input.consume",
            &format!(
                r#"{{"targetId":"{ID}","registrationId":"{OTHER}","bodyId":null,"expectedControlEpoch":"0"}}"#
            ),
        ),
        envelope(
            "approval.register",
            &format!(
                r#"{{"approvalId":"{ID}","choiceId":"{OTHER}","expectedControlEpoch":"9007199254740993"}}"#
            ),
        ),
        envelope(
            "approval.decide",
            &format!(
                r#"{{"approvalRegistrationId":"{ID}","expectedControlEpoch":"18446744073709551615"}}"#
            ),
        ),
    ];

    for wire in valid_wires {
        let parsed = parse_session_command(wire.as_bytes())
            .unwrap_or_else(|err| panic!("expected valid SCT-001 command {wire}: {err}"));
        let encoded = encode_session_command(&parsed).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&wire).unwrap(),
            serde_json::from_str::<Value>(&encoded).unwrap()
        );
        assert_eq!(parse_session_command(encoded.as_bytes()).unwrap(), parsed);
    }

    let invalid_cases = [
        envelope(
            "session.control.inspect",
            &format!(r#"{{"targetId":"{ID}","extra":1}}"#),
        ),
        envelope(
            "session.control.takeover",
            &format!(r#"{{"targetId":"{ID}","expectedControlEpoch":"1"}}"#),
        ),
        envelope(
            "session.control.release",
            &format!(r#"{{"targetId":"{ID}","expectedControlEpoch":null}}"#),
        ),
        envelope(
            "input.register",
            &format!(
                r#"{{"targetId":"{ID}","operation":{{"kind":"resume"}},"expectedControlEpoch":null}}"#
            ),
        ),
        envelope(
            "input.consume",
            &format!(
                r#"{{"targetId":"{ID}","registrationId":"{OTHER}","bodyId":null,"expectedControlEpoch":1}}"#
            ),
        ),
        envelope(
            "approval.register",
            &format!(r#"{{"approvalId":"{ID}","choiceId":"{OTHER}","expectedControlEpoch":"01"}}"#),
        ),
        envelope(
            "approval.decide",
            &format!(
                r#"{{"approvalRegistrationId":"{ID}","expectedControlEpoch":"18446744073709551616"}}"#
            ),
        ),
        envelope(
            "input.register",
            &format!(
                r#"{{"targetId":"{ID}","operation":{{"kind":"resume"}},"expectedControlEpoch":"-1"}}"#
            ),
        ),
    ];

    for wire in invalid_cases {
        assert!(
            parse_session_command(wire.as_bytes()).is_err(),
            "expected strict rejection for {wire}"
        );
    }
}
