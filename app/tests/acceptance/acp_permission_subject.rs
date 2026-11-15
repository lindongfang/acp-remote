//! Independent §8A public API wire cases; synthetic peers are not product approval authority.
use super::acp_runtime_legacy29_support as runtime;
use acp_remote::adapters::acp::{
    AcpError, Incoming, OptionId, PermissionRequest, PermissionSelection, Response, RpcId,
    ToolCallId, Version, WriteFailure, WriteReport, ZeroReason,
};
use runtime::{RpcJson, TestClient as Client, WriterControl, raw_field, raw_value};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Duration, Instant, timeout},
};
struct Peer {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
fn option(id: &str) -> OptionId {
    OptionId::parse(raw_value(&json!(id)).as_ref()).unwrap()
}
fn frame(version: Version, id: Value) -> Value {
    let mut params = json!({"sessionId":"session-A","options":[
        {"optionId":"allow","name":"合成允许","kind":"allow_once","_meta":{"opaque":[null,1]}},
        {"optionId":"deny","name":"合成拒绝","kind":"reject_once"}],"future":{"raw":[1,null,"é"]}});
    if version == Version::V1 {
        params["toolCall"] = json!({"toolCallId":"tool-A","future":true});
    } else {
        params["title"] = json!("合成权限");
    }
    let mut message = json!({"jsonrpc":"2.0","method":"session/request_permission","params":params,"_meta":{"preserve":true}});
    message.as_object_mut().unwrap().insert("id".into(), id);
    message
}
async fn feed_bytes(peer: &mut Peer, bytes: &[u8]) {
    peer.write.write_all(bytes).await.unwrap();
    peer.write.write_all(b"\n").await.unwrap();
}
async fn feed(peer: &mut Peer, value: &Value) {
    feed_bytes(peer, &serde_json::to_vec(value).unwrap()).await;
}
async fn sent(peer: &mut Peer) -> Value {
    let mut line = String::new();
    let count = timeout(Duration::from_secs(2), peer.read.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(count > 0);
    serde_json::from_str(&line).unwrap()
}
async fn no_wire(peer: &mut Peer) {
    let mut line = String::new();
    if let Ok(result) = timeout(Duration::from_millis(3), peer.read.read_line(&mut line)).await {
        assert_eq!(result.unwrap(), 0, "unexpected extra protocol frame");
    }
}
async fn ready(version: Version) -> (Client, Peer, WriterControl) {
    let (local, remote) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(local);
    let (pr, pw) = tokio::io::split(remote);
    let mut client = Client::new(read, write);
    let mode = client.control.clone();
    let mut peer = Peer {
        read: BufReader::new(pr),
        write: pw,
    };
    let id = client.initialize(version, deadline()).await.unwrap();
    assert_eq!(
        sent(&mut peer).await["params"]["protocolVersion"],
        version.number()
    );
    let result = if version == Version::V1 {
        json!({"protocolVersion":1,"agentInfo":{"name":"synthetic","version":"fixed"},"agentCapabilities":{}})
    } else {
        json!({"protocolVersion":2,"info":{"name":"synthetic","version":"fixed"},"capabilities":{"session":{}}})
    };
    feed(
        &mut peer,
        &json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    (client, peer, mode)
}
async fn permission(client: &mut Client, peer: &mut Peer, value: &Value) -> PermissionRequest {
    feed(peer, value).await;
    match client.next(deadline()).await.unwrap() {
        Incoming::Permission(request) => request,
        _ => panic!("permission event"),
    }
}
async fn malformed(version: Version, value: &Value) {
    let (mut client, mut peer, _) = ready(version).await;
    feed(&mut peer, value).await;
    assert!(matches!(
        client.next(deadline()).await,
        Err(AcpError::InvalidMessage)
    ));
    assert!(client.is_closed());
    no_wire(&mut peer).await;
}
async fn selected(client: &Client, peer: &mut Peer, request: &PermissionRequest) {
    client
        .permission_response(request, Some(&option("allow")), deadline())
        .await
        .unwrap();
    let response = sent(peer).await;
    assert_eq!(response["id"], request.id().json());
    assert_eq!(
        response["result"]["outcome"],
        json!({"outcome":"selected","optionId":"allow"})
    );
}
async fn cancelled_once(client: &Client, peer: &mut Peer, request: &PermissionRequest) {
    client
        .permission_response(request, None, deadline())
        .await
        .unwrap();
    let response = sent(peer).await;
    assert_eq!(response["id"], request.id().json());
    assert_eq!(
        response["result"]["outcome"],
        json!({"outcome":"cancelled"})
    );
    assert!(
        client
            .permission_response(request, None, deadline())
            .await
            .is_err()
    );
    assert!(
        client
            .permission_response(request, Some(&option("allow")), deadline())
            .await
            .is_err()
    );
    no_wire(peer).await;
}

#[tokio::test]
async fn ap_v2_subject_optional_is_supported_without_tool_or_run() {
    for subject in [None, Some(Value::Null)] {
        for cancel in [false, true] {
            let (mut client, mut peer, _) = ready(Version::V2).await;
            let mut value = frame(Version::V2, json!("optional"));
            if let Some(subject) = &subject {
                value["params"]["subject"] = subject.clone();
            }
            let request = permission(&mut client, &mut peer, &value).await;
            assert!(request.supported_subject());
            assert!(request.tool().is_none());
            assert_eq!(request.session().as_str(), "session-A");
            assert_eq!(
                raw_field(request.raw(), "params").as_ref(),
                value.get("params")
            );
            if cancel {
                cancelled_once(&client, &mut peer, &request).await;
            } else {
                selected(&client, &mut peer, &request).await;
                no_wire(&mut peer).await;
            }
        }
    }
}

#[tokio::test]
async fn ap_v2_command_tool_absent_null_and_string_are_supported() {
    for tool in [None, Some(Value::Null), Some(json!("tool-A"))] {
        let (mut client, mut peer, _) = ready(Version::V2).await;
        let mut value = frame(Version::V2, json!("command"));
        value["params"]["subject"] = json!({"type":"command","command":"synthetic --flag","cwd":"C:/fixture","terminalId":null,"_meta":{"x":[1,null]},"future":false});
        if let Some(tool) = &tool {
            value["params"]["subject"]["toolCallId"] = tool.clone();
        }
        let request = permission(&mut client, &mut peer, &value).await;
        assert!(request.supported_subject());
        assert_eq!(
            request.tool().map(ToolCallId::as_str),
            tool.as_ref().and_then(Value::as_str)
        );
        assert_eq!(
            raw_field(request.raw(), "params").as_ref(),
            value.get("params")
        );
        selected(&client, &mut peer, &request).await;
        no_wire(&mut peer).await;
    }
}

#[tokio::test]
async fn ap_subject_shape_errors_are_not_absence_or_extensions() {
    for subject in [
        json!(true),
        json!(false),
        json!([]),
        json!([null]),
        json!(1),
        json!("command"),
        json!({}),
        json!({"type":null}),
        json!({"type":true}),
        json!({"type":[]}),
        json!({"type":1}),
    ] {
        let mut value = frame(Version::V2, json!("bad"));
        value["params"]["subject"] = subject;
        malformed(Version::V2, &value).await;
    }
}

#[tokio::test]
async fn ap_command_known_field_types_cannot_silently_disappear() {
    for field in ["command", "cwd", "toolCallId"] {
        for bad in [json!(true), json!([]), json!({}), json!(1)] {
            let mut value = frame(Version::V2, json!("bad"));
            value["params"]["subject"] =
                json!({"type":"command","command":"synthetic","cwd":"C:/fixture"});
            value["params"]["subject"][field] = bad;
            malformed(Version::V2, &value).await;
        }
        if field != "toolCallId" {
            for absent in [true, false] {
                let mut value = frame(Version::V2, json!("bad"));
                value["params"]["subject"] =
                    json!({"type":"command","command":"synthetic","cwd":"C:/fixture"});
                if absent {
                    value["params"]["subject"]
                        .as_object_mut()
                        .unwrap()
                        .remove(field);
                } else {
                    value["params"]["subject"][field] = Value::Null;
                }
                malformed(Version::V2, &value).await;
            }
        }
    }
}

#[tokio::test]
async fn ap_v2_tool_call_retains_required_tool_association() {
    let (mut client, mut peer, _) = ready(Version::V2).await;
    let mut value = frame(Version::V2, json!(1));
    value["params"]["subject"] = json!({"type":"tool_call","toolCall":{"toolCallId":"tool-A","rawInput":{"opaque":[null,2]}}});
    let request = permission(&mut client, &mut peer, &value).await;
    assert!(request.supported_subject());
    assert_eq!(request.tool().unwrap().as_str(), "tool-A");
    assert_eq!(
        raw_field(request.raw(), "params").as_ref(),
        value.get("params")
    );
    selected(&client, &mut peer, &request).await;
    for bad in [
        None,
        Some(Value::Null),
        Some(json!(true)),
        Some(json!([])),
        Some(json!({})),
        Some(json!({"toolCallId":null})),
        Some(json!({"toolCallId":1})),
        Some(json!({"toolCallId":[]})),
    ] {
        let mut value = frame(Version::V2, json!("bad"));
        value["params"]["subject"] = json!({"type":"tool_call"});
        if let Some(bad) = bad {
            value["params"]["subject"]["toolCall"] = bad;
        }
        malformed(Version::V2, &value).await;
    }
}

#[tokio::test]
async fn ap_v1_required_tool_is_not_relaxed_by_v2_projection() {
    let (mut client, mut peer, _) = ready(Version::V1).await;
    let value = frame(Version::V1, json!(1));
    let request = permission(&mut client, &mut peer, &value).await;
    assert!(request.supported_subject());
    assert_eq!(request.tool().unwrap().as_str(), "tool-A");
    selected(&client, &mut peer, &request).await;
    for bad in [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!([])),
        Some(json!({})),
        Some(json!({"toolCallId":null})),
        Some(json!({"toolCallId":true})),
        Some(json!({"toolCallId":[]})),
    ] {
        let mut value = frame(Version::V1, json!("bad"));
        value["params"].as_object_mut().unwrap().remove("toolCall");
        value["params"]["subject"] = json!({"type":"tool_call","toolCall":{"toolCallId":"v2"}});
        if let Some(bad) = bad {
            value["params"]["toolCall"] = bad;
        }
        malformed(Version::V1, &value).await;
    }
}

#[tokio::test]
async fn ap_required_envelope_fields_and_versioned_option_cardinality() {
    for bad in [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!([])),
        Some(json!(1)),
    ] {
        let mut value = frame(Version::V2, json!("bad"));
        value["params"].as_object_mut().unwrap().remove("title");
        if let Some(bad) = bad {
            value["params"]["title"] = bad;
        }
        malformed(Version::V2, &value).await;
    }
    for version in [Version::V1, Version::V2] {
        for field in ["sessionId", "options"] {
            for bad in [Value::Null, json!(true), json!({}), json!(2)] {
                let mut value = frame(version, json!("bad"));
                value["params"][field] = bad;
                malformed(version, &value).await;
            }
        }
        for field in ["optionId", "kind", "name"] {
            for bad in [None, Some(Value::Null), Some(json!([])), Some(json!(2))] {
                let mut value = frame(version, json!("bad"));
                value["params"]["options"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                if let Some(bad) = bad {
                    value["params"]["options"][0][field] = bad;
                }
                malformed(version, &value).await;
            }
        }
        let mut value = frame(version, json!("empty"));
        value["params"]["options"] = json!([]);
        if version == Version::V2 {
            malformed(version, &value).await;
        } else {
            let (mut client, mut peer, _) = ready(version).await;
            let request = permission(&mut client, &mut peer, &value).await;
            assert!(
                client
                    .permission_response(&request, Some(&option("allow")), deadline())
                    .await
                    .is_err()
            );
            no_wire(&mut peer).await;
            cancelled_once(&client, &mut peer, &request).await;
        }
    }
}

#[tokio::test]
async fn ap_original_advertisement_and_raw_override_mutable_projection() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer, _) = ready(version).await;
        let value = frame(version, json!("raw"));
        let bytes = format!("  {}  ", serde_json::to_string(&value).unwrap()).into_bytes();
        feed_bytes(&mut peer, &bytes).await;
        let Incoming::Permission(request) = client.next(deadline()).await.unwrap() else {
            panic!("permission")
        };
        assert_eq!(request.raw().bytes(), bytes);
        assert_eq!(
            raw_field(request.raw(), "params").as_ref(),
            value.get("params")
        );
        assert!(
            client
                .permission_response(&request, Some(&option("forged")), deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer).await;
        let forged = client
            .strategy()
            .unwrap()
            .permission(
                request.id().clone(),
                request.raw().clone(),
                request.raw().field("params").unwrap().unwrap(),
            )
            .unwrap();
        assert!(
            client
                .permission_response(&forged, Some(&option("allow")), deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer).await;
        let mut other_value = frame(version, json!("other"));
        other_value["params"]["options"][0]["optionId"] = json!("changed");
        let other = permission(&mut client, &mut peer, &other_value).await;
        assert!(
            client
                .permission_response(&request, Some(&option("changed")), deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer).await;
        selected(&client, &mut peer, &request).await;
        cancelled_once(&client, &mut peer, &other).await;
        assert_eq!(request.raw().bytes(), bytes);
    }
}

#[tokio::test]
async fn ap_unknown_subject_options_duplicate_ids_allow_only_safe_cancel() {
    for version in [Version::V1, Version::V2] {
        let mut variants = Vec::new();
        for kind in ["future_allow", "_custom"] {
            let mut value = frame(version, json!("unknown"));
            value["params"]["options"][0]["kind"] = json!(kind);
            variants.push(value);
        }
        for same_kind in [true, false] {
            let mut value = frame(version, json!("duplicate"));
            value["params"]["options"][1]["optionId"] = json!("allow");
            if same_kind {
                value["params"]["options"][1]["kind"] = json!("allow_once");
            }
            variants.push(value);
        }
        if version == Version::V2 {
            for tag in ["future", "_custom"] {
                let mut value = frame(version, json!("subject"));
                value["params"]["subject"] = json!({"type":tag,"opaque":[null,{"x":true}]});
                variants.push(value);
            }
        }
        for value in variants {
            let (mut client, mut peer, _) = ready(version).await;
            let request = permission(&mut client, &mut peer, &value).await;
            assert_eq!(
                raw_field(request.raw(), "params").as_ref(),
                value.get("params")
            );
            let duplicate = value["params"]["options"][0]["optionId"]
                == value["params"]["options"][1]["optionId"];
            if duplicate {
                for id in ["allow", "deny", "unadvertised"] {
                    assert!(
                        client
                            .permission_response(&request, Some(&option(id)), deadline())
                            .await
                            .is_err()
                    );
                    no_wire(&mut peer).await;
                }
            } else {
                no_wire(&mut peer).await;
                assert!(
                    client
                        .permission_response(&request, Some(&option("unadvertised")), deadline())
                        .await
                        .is_err()
                );
                no_wire(&mut peer).await;
                for choice in ["allow", "deny"] {
                    let (mut manual, mut manual_peer, _) = ready(version).await;
                    let original = permission(&mut manual, &mut manual_peer, &value).await;
                    assert_eq!(
                        raw_field(original.raw(), "params").as_ref(),
                        value.get("params")
                    );
                    no_wire(&mut manual_peer).await;
                    assert!(
                        manual
                            .permission_response(
                                &original,
                                Some(&option("unadvertised")),
                                deadline()
                            )
                            .await
                            .is_err()
                    );
                    no_wire(&mut manual_peer).await;
                    manual
                        .permission_response(&original, Some(&option(choice)), deadline())
                        .await
                        .unwrap();
                    let sent = sent(&mut manual_peer).await;
                    assert_eq!(sent["id"], original.id().json());
                    assert_eq!(
                        sent["result"]["outcome"],
                        json!({"outcome":"selected","optionId":choice})
                    );
                    assert!(
                        manual
                            .permission_response(&original, None, deadline())
                            .await
                            .is_err()
                    );
                    assert!(
                        manual
                            .permission_response(&original, Some(&option(choice)), deadline())
                            .await
                            .is_err()
                    );
                    no_wire(&mut manual_peer).await;
                    assert!(manual.owner.shutdown(deadline()).await.joined);
                }
            }
            cancelled_once(&client, &mut peer, &request).await;
        }
    }
}

#[tokio::test]
async fn ap_rpc_scalar_identity_is_not_stringified_or_aliased() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer, _) = ready(version).await;
        let mut pending = Vec::new();
        for id in [json!(7), json!("7"), json!("007")] {
            pending.push(permission(&mut client, &mut peer, &frame(version, id)).await);
        }
        for request in pending {
            selected(&client, &mut peer, &request).await;
        }
        no_wire(&mut peer).await;
        for id in [
            Value::Null,
            json!(true),
            json!([]),
            json!({}),
            json!(1.5),
            json!(1.0),
            json!(9_007_199_254_740_992_u64),
        ] {
            malformed(version, &frame(version, id)).await;
        }
    }
}

#[tokio::test]
async fn ap_connection_scope_duplicate_request_ids_and_spent_requests() {
    for version in [Version::V1, Version::V2] {
        let (mut first, mut peer_a, _) = ready(version).await;
        let (mut second, mut peer_b, _) = ready(version).await;
        let value = frame(version, json!("same"));
        let a = permission(&mut first, &mut peer_a, &value).await;
        let b = permission(&mut second, &mut peer_b, &value).await;
        assert!(
            second
                .permission_response(&a, Some(&option("allow")), deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer_b).await;
        let unrequested = first
            .strategy()
            .unwrap()
            .permission(
                RpcId::parse(raw_value(&json!("unrequested")).as_ref()).unwrap(),
                a.raw().clone(),
                a.raw().field("params").unwrap().unwrap(),
            )
            .unwrap();
        assert!(
            first
                .permission_response(&unrequested, None, deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer_a).await;
        cancelled_once(&second, &mut peer_b, &b).await;
        selected(&first, &mut peer_a, &a).await;
        assert!(
            first
                .permission_response(&a, None, deadline())
                .await
                .is_err()
        );
        no_wire(&mut peer_a).await;
        for answered in [false, true] {
            let (mut client, mut peer, _) = ready(version).await;
            let request = permission(&mut client, &mut peer, &value).await;
            if answered {
                cancelled_once(&client, &mut peer, &request).await;
            }
            feed(&mut peer, &value).await;
            assert!(client.next(deadline()).await.is_err());
            assert!(client.is_closed());
            no_wire(&mut peer).await;
        }
    }
}

#[tokio::test]
async fn ap_cancelled_once_and_unknown_delivery_never_automatically_resend() {
    for subject in [
        None,
        Some(json!({"type":"command","command":"synthetic","cwd":"C:/fixture"})),
    ] {
        for failure in [1, 2, 3] {
            let (mut client, mut peer, mode) = ready(Version::V2).await;
            let mut value = frame(Version::V2, json!("write"));
            if let Some(subject) = &subject {
                value["params"]["subject"] = subject.clone();
            }
            let request = permission(&mut client, &mut peer, &value).await;
            mode.mode(if failure == 1 {
                1
            } else if failure == 2 {
                2
            } else {
                3
            });
            let reservation = client
                .connection
                .reserve_permission_response(&request, PermissionSelection::Cancelled)
                .unwrap();
            let ticket = client
                .enqueue(
                    reservation,
                    if failure == 2 {
                        Instant::now() + Duration::from_millis(50)
                    } else {
                        deadline()
                    },
                )
                .unwrap();
            let report = if failure == 3 {
                mode.wait_pending().await;
                assert!(mode.accepted() > 0);
                runtime::drop_pending_waiter(&ticket).await;
                assert!(!client.is_closed());
                ticket.cancel_and_wait().await
            } else {
                ticket.wait().await
            };
            if failure == 2 {
                assert_eq!(
                    report,
                    WriteReport::Zero {
                        reason: ZeroReason::Deadline
                    }
                );
                assert!(!client.is_closed());
                no_wire(&mut peer).await;
                mode.mode(0);
                cancelled_once(&client, &mut peer, &request).await;
            } else {
                assert!(
                    matches!(
                        report,
                        WriteReport::Unknown {
                            cause: WriteFailure::Io,
                            ..
                        }
                    ) && failure == 1
                        || matches!(
                            report,
                            WriteReport::Unknown {
                                accepted: 8,
                                cause: WriteFailure::Cancelled
                            }
                        ) && failure == 3
                );
                assert!(client.is_closed());
                mode.mode(0);
                assert!(
                    client
                        .permission_response(&request, None, deadline())
                        .await
                        .is_err()
                );
                assert!(
                    client
                        .permission_response(&request, Some(&option("allow")), deadline())
                        .await
                        .is_err()
                );
                if failure == 3 {
                    let mut partial = [0; 8];
                    tokio::io::AsyncReadExt::read_exact(&mut peer.read, &mut partial)
                        .await
                        .unwrap();
                    assert!(!partial.contains(&b'\n'));
                }
                no_wire(&mut peer).await;
            }
            assert!(client.owner.shutdown(deadline()).await.joined);
        }
    }
}
