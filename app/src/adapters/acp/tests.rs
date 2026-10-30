//! Parameterized standard protocol fixtures. These are not real Harness compatibility evidence.
use super::*;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout},
};
type Client = Connection<ReadHalf<DuplexStream>, WriteHalf<DuplexStream>>;
type Peer = (BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>);
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
fn pair() -> (Client, Peer) {
    let (a, b) = tokio::io::duplex(128 * 1024);
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    (Connection::new(ar, aw), (BufReader::new(br), bw))
}
async fn sent(peer: &mut Peer) -> Value {
    let mut line = String::new();
    timeout(Duration::from_secs(2), peer.0.read_line(&mut line))
        .await
        .expect("bounded")
        .expect("read");
    serde_json::from_str(&line).expect("JSON")
}
async fn feed(peer: &mut Peer, value: Value) {
    let mut b = serde_json::to_vec(&value).expect("encode");
    b.push(b'\n');
    timeout(Duration::from_secs(2), peer.1.write_all(&b))
        .await
        .expect("bounded")
        .expect("write");
}
fn init_result(v: Version) -> Value {
    match v {
        Version::V1 => {
            json!({"protocolVersion":1,"agentInfo":{"name":"fixture","version":"fixed"},"agentCapabilities":{"sessionCapabilities":{"resume":{},"close":{}},"promptCapabilities":{"image":true}}})
        }
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{"prompt":{"image":{}}}}})
        }
    }
}
async fn ready(offer: Version, selected: Version) -> (Client, Peer) {
    let (mut c, mut p) = pair();
    let id = c.initialize(offer, deadline()).await.expect("initialize");
    let request = sent(&mut p).await;
    assert_eq!(request["params"]["protocolVersion"], offer.number());
    assert_eq!(
        request["params"].get("clientInfo").is_some(),
        offer == Version::V1
    );
    assert_eq!(
        request["params"].get("info").is_some(),
        offer == Version::V2
    );
    feed(
        &mut p,
        json!({"jsonrpc":"2.0","id":id.json(),"result":init_result(selected)}),
    )
    .await;
    assert!(matches!(
        c.next(deadline()).await.expect("init response"),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    assert_eq!(c.strategy().expect("strategy").version(), selected);
    (c, p)
}
#[tokio::test]
async fn selected_version_bootstrap_once_and_capability_gates() {
    for (offer, selected) in [
        (Version::V1, Version::V1),
        (Version::V2, Version::V2),
        (Version::V2, Version::V1),
    ] {
        let (mut c, mut p) = ready(offer, selected).await;
        assert_eq!(
            c.initialize(offer, deadline()).await.expect_err("once"),
            AcpError::AlreadyInitialized
        );
        let strategy = c.strategy().expect("strategy");
        let session = SessionId::parse(&json!("s")).expect("id");
        assert!(strategy.prompt(session.clone(), json!([])).is_err());
        assert!(
            strategy
                .prompt(
                    session.clone(),
                    json!([{"type":"future","data":"do not flatten"}])
                )
                .is_err()
        );
        assert!(
            strategy
                .prompt(
                    session,
                    json!([{"type":"audio","data":"AA","mimeType":"audio/wav"}])
                )
                .is_err()
        );
        c.shutdown(deadline()).await.expect("close");
        assert!(c.is_closed());
        let mut eof = String::new();
        assert_eq!(
            timeout(Duration::from_secs(1), p.0.read_line(&mut eof))
                .await
                .expect("bounded EOF")
                .expect("EOF"),
            0
        );
    }
}
#[tokio::test]
async fn bootstrap_rejects_unknown_above_offer_bad_id_shape_and_no_common_version() {
    for result in [
        json!({"protocolVersion":3}),
        json!({"protocolVersion":2,"info":null}),
        json!({"protocolVersion":2,"info":{"name":"missing version"}}),
    ] {
        let (mut c, mut p) = pair();
        let id = c.initialize(Version::V2, deadline()).await.expect("init");
        sent(&mut p).await;
        feed(
            &mut p,
            json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
        )
        .await;
        assert!(c.next(deadline()).await.is_err());
        assert!(c.is_closed());
        assert_eq!(
            c.initialize(Version::V1, deadline())
                .await
                .expect_err("never reinit"),
            AcpError::AlreadyInitialized
        );
    }
    let (mut c, mut p) = pair();
    let id = c.initialize(Version::V1, deadline()).await.expect("init");
    sent(&mut p).await;
    feed(
        &mut p,
        json!({"jsonrpc":"2.0","id":id.json(),"result":init_result(Version::V2)}),
    )
    .await;
    assert_eq!(
        c.next(deadline()).await.expect_err("above offer"),
        AcpError::IncompatibleVersion
    );
    let (mut c, mut p) = pair();
    c.initialize(Version::V2, deadline()).await.expect("init");
    sent(&mut p).await;
    feed(
        &mut p,
        json!({"jsonrpc":"2.0","id":99,"result":init_result(Version::V1)}),
    )
    .await;
    assert_eq!(
        c.next(deadline()).await.expect_err("mismatch"),
        AcpError::UnmatchedResponse
    );
    let (mut c, mut p) = pair();
    let id = c.initialize(Version::V2, deadline()).await.expect("init");
    sent(&mut p).await;
    feed(&mut p,json!({"jsonrpc":"2.0","id":id.json(),"error":{"code":-32022,"message":"private peer diagnostic"}})).await;
    assert!(matches!(
        c.next(deadline()).await.expect("remote fact"),
        Incoming::Response {
            response: Response::RemoteError { code: -32022 },
            ..
        }
    ));
    assert!(c.is_closed());
}
#[tokio::test]
async fn prompt_insertion_and_cancel_are_not_completion() {
    for v in [Version::V1, Version::V2] {
        let (mut c, mut p) = ready(v, v).await;
        let session = SessionId::parse(&json!("s")).expect("id");
        let request = c
            .strategy()
            .expect("strategy")
            .prompt(
                session.clone(),
                json!([{"type":"text","text":"synthetic","_meta":{"future":[1,true]}}]),
            )
            .expect("prompt");
        let id = c.request(request, deadline()).await.expect("write");
        let sent = sent(&mut p).await;
        assert_eq!(
            sent["params"]["prompt"][0]["_meta"]["future"],
            json!([1, true])
        );
        c.cancel(&session, deadline()).await.expect("cancel");
        let cancel = super::tests::sent(&mut p).await;
        assert!(cancel.get("id").is_none());
        assert_eq!(c.pending_count(), 1);
        let result = match v {
            Version::V1 => json!({"stopReason":"cancelled","future":{"x":1}}),
            Version::V2 => json!({"messageId":"msg1","future":{"x":1}}),
        };
        feed(
            &mut p,
            json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
        )
        .await;
        match c.next(deadline()).await.expect("response") {
            Incoming::Response {
                response: Response::Prompt(result),
                ..
            } => assert!(matches!(
                (v, result),
                (Version::V1, PromptResult::Completed { .. })
                    | (Version::V2, PromptResult::Inserted { .. })
            )),
            _ => panic!("prompt response"),
        }
        if v == Version::V2 {
            feed(&mut p,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"state_update","state":"idle","stopReason":"cancelled"}}})).await;
            assert!(matches!(
                c.next(deadline()).await.expect("idle"),
                Incoming::Update {
                    event: SessionEvent::State { .. },
                    origin: UpdateOrigin::Live,
                    ..
                }
            ));
        }
    }
}
#[tokio::test]
async fn resume_without_replay_requires_v2_snapshot_and_marks_boundary() {
    for v in [Version::V1, Version::V2] {
        let (mut c, mut p) = ready(v, v).await;
        let s = SessionId::parse(&json!("original")).expect("id");
        let r = c
            .strategy()
            .expect("strategy")
            .resume(s, "C:/fixture")
            .expect("resume");
        let id = c.request(r, deadline()).await.expect("send");
        let msg = sent(&mut p).await;
        assert!(msg["params"].get("replayFrom").is_none());
        assert_eq!(msg["method"], "session/resume");
        if v == Version::V2 {
            feed(&mut p,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"original","update":{"sessionUpdate":"state_update","state":"idle"}}})).await;
            assert!(matches!(
                c.next(deadline()).await.expect("snapshot"),
                Incoming::Update {
                    origin: UpdateOrigin::DuringResume,
                    ..
                }
            ));
        }
        feed(&mut p, json!({"jsonrpc":"2.0","id":id.json(),"result":{}})).await;
        assert!(matches!(
            c.next(deadline()).await.expect("resumed"),
            Incoming::Response {
                response: Response::Resumed { .. },
                ..
            }
        ));
    }
    let (mut c, mut p) = ready(Version::V2, Version::V2).await;
    let r = c
        .strategy()
        .expect("strategy")
        .resume(SessionId::parse(&json!("s")).expect("id"), "C:/fixture")
        .expect("request");
    let id = c.request(r, deadline()).await.expect("send");
    sent(&mut p).await;
    feed(&mut p, json!({"jsonrpc":"2.0","id":id.json(),"result":{}})).await;
    assert_eq!(
        c.next(deadline()).await.expect_err("missing snapshot"),
        AcpError::MissingSnapshot
    );
}

fn permission(v: Version, id: &Value) -> Value {
    let mut params = json!({"sessionId":"s","options":[{"optionId":"allow","name":"synthetic allow","kind":"allow_once"},{"optionId":"reject","name":"synthetic reject","kind":"reject_once"}],"future":{"scope":[{"x":true}]}});
    match v {
        Version::V1 => params["toolCall"] = json!({"toolCallId":"tool1"}),
        Version::V2 => {
            params["title"] = json!("synthetic");
            params["subject"] = json!({"type":"tool_call","toolCall":{"toolCallId":"tool1"}});
        }
    }
    json!({"jsonrpc":"2.0","id":id,"method":"session/request_permission","params":params})
}
#[tokio::test]
async fn permission_original_advertisement_typed_ids_once_and_fail_closed() {
    for v in [Version::V1, Version::V2] {
        let (mut c, mut p) = ready(v, v).await;
        for id in [json!(1), json!("1")] {
            feed(&mut p, permission(v, &id)).await;
            let Incoming::Permission(mut request) = c.next(deadline()).await.expect("permission")
            else {
                panic!("permission")
            };
            assert_eq!(
                request
                    .tool
                    .as_ref()
                    .expect("reference without rawInput")
                    .as_str(),
                "tool1"
            );
            let raw = request.raw.bytes().to_vec();
            assert!(request.raw.fields()["params"]["future"].is_object());
            let option = OptionId::parse(&json!("forged")).expect("id");
            request.options.push(PermissionChoice {
                id: option.clone(),
                kind: "allow_once".into(),
            });
            assert_eq!(
                c.permission_response(&request, Some(&option), deadline())
                    .await
                    .expect_err("must reparse original"),
                AcpError::InvalidOption
            );
            let allow = OptionId::parse(&json!("allow")).expect("id");
            c.permission_response(&request, Some(&allow), deadline())
                .await
                .expect("response");
            let response = sent(&mut p).await;
            assert_eq!(response["id"], id);
            assert_eq!(response["result"]["outcome"]["optionId"], "allow");
            assert_eq!(request.raw.bytes(), raw);
            assert_eq!(
                c.permission_response(&request, Some(&allow), deadline())
                    .await
                    .expect_err("spent"),
                AcpError::UnmatchedResponse
            );
        }
        feed(&mut p, permission(v, &json!(1))).await;
        assert!(c.next(deadline()).await.is_err());
        assert!(c.is_closed());
    }
}
#[tokio::test]
async fn unknown_permission_options_and_subjects_are_preserved_not_allowed() {
    for v in [Version::V1, Version::V2] {
        let (mut c, mut p) = ready(v, v).await;
        let mut frame = permission(v, &json!("unknown"));
        frame["params"]["options"][0]["kind"] = json!("future_allow");
        if v == Version::V2 {
            frame["params"]["subject"] = json!({"type":"future","payload":{"x":[1,2]}});
        }
        feed(&mut p, frame).await;
        let Incoming::Permission(request) = c.next(deadline()).await.expect("request") else {
            panic!("permission")
        };
        assert!(!request.options[0].supported());
        assert!(
            c.permission_response(&request, Some(&request.options[0].id), deadline())
                .await
                .is_err()
        );
        c.permission_response(&request, None, deadline())
            .await
            .expect("cancel");
        assert_eq!(
            sent(&mut p).await["result"]["outcome"]["outcome"],
            "cancelled"
        );
    }
}
#[tokio::test]
async fn raw_unknown_deep_structure_and_extensions_survive() {
    for v in [Version::V1, Version::V2] {
        let (mut c, mut p) = ready(v, v).await;
        let mut nested = json!({"unknown":"exact structure"});
        for _ in 0..12 {
            nested = json!({"a":[nested]});
        }
        let frame = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"future_update","data":nested}},"_meta":{"x":1}});
        let expected = serde_json::to_vec(&frame).expect("bytes");
        feed(&mut p, frame.clone()).await;
        let Incoming::Update {
            raw,
            event: SessionEvent::Other { .. },
            ..
        } = c.next(deadline()).await.expect("unknown preserved")
        else {
            panic!("unknown")
        };
        assert_eq!(raw.bytes(), expected);
        assert_eq!(raw.fields()["params"], frame["params"]);
        assert!(!format!("{raw:?}").contains("exact structure"));
        feed(
            &mut p,
            json!({"jsonrpc":"2.0","id":"unsupported","method":"future/method"}),
        )
        .await;
        let Incoming::UnsupportedRequest { id, .. } =
            c.next(deadline()).await.expect("unsupported")
        else {
            panic!("unsupported")
        };
        c.reject_unsupported(&id, deadline()).await.expect("reject");
        assert_eq!(sent(&mut p).await["error"]["code"], -32601);
    }
}
#[tokio::test]
async fn pending_out_of_order_no_retransmission_duplicate_response_and_deadline() {
    let (mut c, mut p) = ready(Version::V2, Version::V2).await;
    let first = c
        .request(
            c.strategy()
                .expect("strategy")
                .new_session("C:/a")
                .expect("new"),
            deadline(),
        )
        .await
        .expect("first");
    let second = c
        .request(
            c.strategy()
                .expect("strategy")
                .new_session("C:/b")
                .expect("new"),
            deadline(),
        )
        .await
        .expect("second");
    sent(&mut p).await;
    sent(&mut p).await;
    for id in [&second, &first] {
        feed(
            &mut p,
            json!({"jsonrpc":"2.0","id":id.json(),"result":{"sessionId":"s"}}),
        )
        .await;
        let Incoming::Response { id: actual, .. } = c.next(deadline()).await.expect("response")
        else {
            panic!("response")
        };
        assert_eq!(&actual, id);
    }
    feed(
        &mut p,
        json!({"jsonrpc":"2.0","id":first.json(),"result":{"sessionId":"s"}}),
    )
    .await;
    assert_eq!(
        c.next(deadline()).await.expect_err("duplicate"),
        AcpError::UnmatchedResponse
    );
    let (mut c, mut p) = ready(Version::V1, Version::V1).await;
    let req = c
        .strategy()
        .expect("strategy")
        .new_session("C:/a")
        .expect("request");
    c.request(req, Instant::now() + Duration::from_millis(20))
        .await
        .expect("write");
    sent(&mut p).await;
    assert_eq!(
        c.next(deadline()).await.expect_err("no automatic retry"),
        AcpError::Deadline
    );
    assert!(c.is_closed());
    assert_eq!(c.pending_count(), 0);
    assert_eq!(
        c.initialize(Version::V1, deadline())
            .await
            .expect_err("no retry"),
        AcpError::AlreadyInitialized
    );
}
#[tokio::test]
async fn cancellation_safe_partial_read_and_poisoned_partial_write() {
    let (mut c, mut p) = pair();
    let id = c.initialize(Version::V1, deadline()).await.expect("init");
    sent(&mut p).await;
    let bytes = serde_json::to_vec(
        &json!({"jsonrpc":"2.0","id":id.json(),"result":init_result(Version::V1)}),
    )
    .expect("encode");
    p.1.write_all(&bytes[..8]).await.expect("partial");
    assert!(
        timeout(Duration::from_millis(5), c.next(deadline()))
            .await
            .is_err()
    );
    p.1.write_all(&bytes[8..]).await.expect("rest");
    p.1.write_all(b"\n").await.expect("newline");
    assert!(matches!(
        c.next(deadline())
            .await
            .expect("complete after cancelled read"),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    let (stream, _peer) = tokio::io::duplex(1);
    let (reader, writer) = tokio::io::split(stream);
    let mut c = Connection::new(reader, writer);
    assert!(
        timeout(
            Duration::from_millis(5),
            c.initialize(Version::V1, deadline())
        )
        .await
        .is_err()
    );
    assert!(c.is_closed());
    assert_eq!(
        c.initialize(Version::V1, deadline())
            .await
            .expect_err("unknown write never retry"),
        AcpError::AlreadyInitialized
    );
    assert!(
        c.shutdown(Instant::now() + Duration::from_millis(10))
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn strict_envelope_depth_duplicates_fractional_ids_and_capacities() {
    for bytes in [
        br#"{"jsonrpc":"2.0","id":1,"id":1,"result":{}}"#.as_slice(),
        br#"{"jsonrpc":"2.0","id":1.5,"result":{}}"#.as_slice(),
        br#"{"jsonrpc":"2.0","id":1,"result":{},"error":{}}"#.as_slice(),
    ] {
        let (mut c, mut p) = pair();
        c.initialize(Version::V1, deadline()).await.expect("init");
        sent(&mut p).await;
        p.1.write_all(bytes).await.expect("malformed");
        p.1.write_all(b"\n").await.expect("newline");
        assert!(c.next(deadline()).await.is_err());
        assert!(c.is_closed());
    }
    assert_ne!(
        RpcId::parse(&json!(1)).expect("numeric"),
        RpcId::parse(&json!("1")).expect("string")
    );
    assert!(RpcId::parse(&json!("x".repeat(1025))).is_err());
    let (mut c, mut p) = ready(Version::V1, Version::V1).await;
    for _ in 0..64 {
        let r = c
            .strategy()
            .expect("strategy")
            .new_session("C:/fixture")
            .expect("request");
        c.request(r, deadline()).await.expect("capacity slot");
        sent(&mut p).await;
    }
    let r = c
        .strategy()
        .expect("strategy")
        .new_session("C:/fixture")
        .expect("request");
    assert_eq!(
        c.request(r, deadline()).await.expect_err("bounded pending"),
        AcpError::Capacity
    );
    c.shutdown(deadline()).await.expect("close");
}

#[tokio::test]
async fn version_specific_tool_patch_semantics_do_not_collapse_null_and_omission() {
    for v in [Version::V1, Version::V2] {
        let (c, _p) = ready(v, v).await;
        let strategy = c.strategy().expect("strategy");
        assert_eq!(
            strategy.tool_name_patch(&json!({})).expect("omitted"),
            ToolNamePatch::Unchanged
        );
        assert_eq!(
            strategy
                .tool_name_patch(&json!({"name":null}))
                .expect("null"),
            if v == Version::V1 {
                ToolNamePatch::Unchanged
            } else {
                ToolNamePatch::Clear
            }
        );
        assert_eq!(
            strategy
                .tool_name_patch(&json!({"name":"updated"}))
                .expect("set"),
            ToolNamePatch::Set("updated")
        );
        let current = json!({"keep":1,"remove":2,"nested":{"old":true}});
        let update = json!({"_meta":{"remove":null,"nested":{"new":true}}});
        if v == Version::V2 {
            assert_eq!(
                strategy
                    .tool_metadata_patch(&current, &update)
                    .expect("patch"),
                json!({"keep":1,"nested":{"new":true}})
            );
            assert_eq!(
                strategy
                    .tool_metadata_patch(&current, &json!({"_meta":null}))
                    .expect("clear"),
                Value::Null
            );
        } else {
            assert_eq!(
                strategy
                    .tool_metadata_patch(&current, &update)
                    .expect_err("no v2 rule for v1"),
                AcpError::UnsupportedCapability
            );
        }
    }
}
#[tokio::test]
async fn command_subject_can_reference_standard_observed_tool_but_missing_reference_is_restricted()
{
    let (mut c, mut p) = ready(Version::V2, Version::V2).await;
    let mut frame = permission(Version::V2, &json!("cmd"));
    frame["params"]["subject"] = json!({"type":"command","command":"synthetic command","cwd":"C:/fixture","toolCallId":"tool1"});
    feed(&mut p, frame).await;
    let Incoming::Permission(request) = c.next(deadline()).await.expect("command subject") else {
        panic!("permission")
    };
    assert!(request.supported_subject);
    assert_eq!(request.tool.expect("tool").as_str(), "tool1");
    let mut frame = permission(Version::V2, &json!("unassociated"));
    frame["params"]["subject"] =
        json!({"type":"command","command":"synthetic command","cwd":"C:/fixture"});
    feed(&mut p, frame).await;
    let Incoming::Permission(request) = c.next(deadline()).await.expect("unassociated") else {
        panic!("permission")
    };
    assert!(!request.supported_subject);
    assert!(
        c.permission_response(&request, Some(&request.options[0].id), deadline())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn acp_frame_byte_ceiling_exact_and_over_with_owned_writer_tasks() {
    timeout(Duration::from_secs(20), async {
        const LIMIT: usize = 32 * 1024 * 1024;
        for extra in [0usize, 1] {
            let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
            let prefix = br#"{"jsonrpc":"2.0","method":"future","params":{"data":""#;
            let suffix = br#""}}"#;
            let body_len = LIMIT + extra - prefix.len() - suffix.len();
            let mut bytes = Vec::with_capacity(LIMIT + extra + 2);
            bytes.extend_from_slice(prefix);
            bytes.resize(bytes.len() + body_len, b'x');
            bytes.extend_from_slice(suffix);
            assert_eq!(bytes.len(), LIMIT + extra);
            bytes.extend_from_slice(b"\r\n");
            let mut tasks = tokio::task::JoinSet::new();
            tasks.spawn(async move { peer.1.write_all(&bytes).await });
            let result = client.next(Instant::now() + Duration::from_secs(10)).await;
            if extra == 0 {
                assert!(matches!(result, Ok(Incoming::UnknownNotification { .. })));
                let result = tasks
                    .join_next()
                    .await
                    .expect("one owned task")
                    .expect("join");
                result.expect("write complete");
            } else {
                assert_eq!(result.expect_err("over byte ceiling"), AcpError::Capacity);
                assert!(client.is_closed());
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
            }
        }
    })
    .await
    .expect("bounded frame fixture");
}
#[tokio::test]
async fn acp_structure_and_native_id_limits_are_explicit() {
    let mut nested = json!({"x":1});
    for _ in 0..97 {
        nested = json!({"x":nested});
    }
    let bytes = serde_json::to_vec(&nested).expect("deep");
    assert_eq!(
        RawMessage::parse(bytes).expect_err("depth budget"),
        AcpError::InvalidMessage
    );
    let many = json!({"items":vec![Value::Null;131_073]});
    assert_eq!(
        RawMessage::parse(serde_json::to_vec(&many).expect("many"))
            .expect_err("metadata collection budget"),
        AcpError::Capacity
    );
    assert!(ToolCallId::parse(&json!("x".repeat(4097))).is_err());
}

#[tokio::test]
async fn permission_source_cannot_cross_connections_even_for_identical_wire_ids() {
    for v in [Version::V1, Version::V2] {
        let (mut first, mut first_peer) = ready(v, v).await;
        let (mut second, mut second_peer) = ready(v, v).await;
        for peer in [&mut first_peer, &mut second_peer] {
            feed(peer, permission(v, &json!("same-id"))).await;
        }
        let Incoming::Permission(request) = first.next(deadline()).await.expect("first source")
        else {
            panic!("permission")
        };
        let Incoming::Permission(second_request) =
            second.next(deadline()).await.expect("second source")
        else {
            panic!("permission")
        };
        let allow = &request.options[0].id;
        assert_eq!(
            second
                .permission_response(&request, Some(allow), deadline())
                .await
                .expect_err("foreign source"),
            AcpError::InvalidMessage
        );
        second
            .permission_response(&second_request, None, deadline())
            .await
            .expect("own source cancellation");
        assert_eq!(
            sent(&mut second_peer).await["result"]["outcome"]["outcome"],
            "cancelled"
        );
        first
            .permission_response(&request, None, deadline())
            .await
            .expect("own source cancellation");
    }
}

#[tokio::test]
async fn already_elapsed_deadline_never_writes_even_when_stream_is_ready() {
    let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
    let request = client
        .strategy()
        .expect("strategy")
        .new_session("C:/fixture")
        .expect("request");
    assert_eq!(
        client
            .request(request, Instant::now() - Duration::from_millis(1))
            .await
            .expect_err("expired before write"),
        AcpError::Deadline
    );
    assert!(client.is_closed());
    assert_eq!(client.pending_count(), 0);
    let mut line = String::new();
    assert!(
        timeout(Duration::from_millis(10), peer.0.read_line(&mut line))
            .await
            .is_err()
    );
    assert!(line.is_empty());
    client.shutdown(deadline()).await.expect("cleanup");
}

#[tokio::test]
async fn oversized_outbound_serialization_is_bounded_and_writes_no_prefix() {
    timeout(Duration::from_secs(10), async {
        let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
        let request = client
            .strategy()
            .expect("strategy")
            .new_session(&"x".repeat(32 * 1024 * 1024))
            .expect("owner normally validates cwd first");
        assert_eq!(
            client
                .request(request, deadline())
                .await
                .expect_err("serialized envelope exceeds capacity"),
            AcpError::Capacity
        );
        assert!(client.is_closed());
        assert_eq!(client.pending_count(), 0);
        let mut line = String::new();
        assert!(
            timeout(Duration::from_millis(10), peer.0.read_line(&mut line))
                .await
                .is_err()
        );
        assert!(line.is_empty());
        client.shutdown(deadline()).await.expect("close");
    })
    .await
    .expect("bounded");
}

#[tokio::test]
async fn acp_r001_v1_only_five_official_stop_reasons_complete_unknown_raw_is_retained() {
    timeout(Duration::from_secs(10),async{
 for reason in ["end_turn","max_tokens","max_turn_requests","refusal","cancelled"]{let(mut client,mut peer)=ready(Version::V1,Version::V1).await;let request=client.strategy().expect("strategy").prompt(SessionId::parse(&json!("s")).expect("id"),json!([{"type":"text","text":"fixture"}])).expect("request");let id=client.request(request,deadline()).await.expect("send");sent(&mut peer).await;feed(&mut peer,json!({"jsonrpc":"2.0","id":id.json(),"result":{"stopReason":reason}})).await;let Incoming::Response{response:Response::Prompt(PromptResult::Completed{stop_reason}),..}=client.next(deadline()).await.expect("known completion")else{panic!("known completion")};assert_eq!(stop_reason.as_ref(),reason);assert!(!client.is_closed());}
 for reason in [json!("future_or_invalid"),json!(""),Value::Null,json!(5),json!({"future":true})]{let(mut client,mut peer)=ready(Version::V1,Version::V1).await;let request=client.strategy().expect("strategy").prompt(SessionId::parse(&json!("s")).expect("id"),json!([{"type":"text","text":"fixture"}])).expect("request");let id=client.request(request,deadline()).await.expect("send");sent(&mut peer).await;let frame=json!({"jsonrpc":"2.0","id":id.json(),"result":{"stopReason":reason,"unknown":{"nested":[1,true,{"source":"retained"}]}}});let expected=serde_json::to_vec(&frame).expect("raw");feed(&mut peer,frame).await;assert_eq!(client.next(deadline()).await.expect_err("unknown never Completed"),AcpError::InvalidMessage);assert!(client.is_closed());assert_eq!(client.pending_count(),0);let raw=client.take_rejected_message().expect("bounded raw retained");assert_eq!(raw.bytes(),expected);assert!(client.take_rejected_message().is_none());assert_eq!(client.next(deadline()).await.expect_err("no replay"),AcpError::Closed);let mut line=String::new();assert!(timeout(Duration::from_millis(10),peer.0.read_line(&mut line)).await.is_err());assert!(line.is_empty());}
 let(client,_peer)=ready(Version::V1,Version::V1).await;assert_eq!(client.strategy().expect("strategy").prompt_result(&json!({})).expect_err("missing reason"),AcpError::InvalidMessage);
 let(mut client,mut peer)=ready(Version::V2,Version::V2).await;feed(&mut peer,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"state_update","state":"future_state","stopReason":"future_reason"}}})).await;assert!(matches!(client.next(deadline()).await.expect("raw control fact"),Incoming::Update{event:SessionEvent::State{..},..})); // No Completed variant is produced for raw v2 control state.
}).await.expect("bounded R001 regression");
}
async fn ready_with_result(version: Version, result: Value) -> (Client, Peer) {
    let (mut client, mut peer) = pair();
    let id = client.initialize(version, deadline()).await.expect("init");
    sent(&mut peer).await;
    feed(
        &mut peer,
        json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
    )
    .await;
    client.next(deadline()).await.expect("selected");
    (client, peer)
}
fn without_image(version: Version) -> Value {
    match version {
        Version::V1 => json!({"protocolVersion":1,"agentCapabilities":{}}),
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"target","version":"fixture"},"capabilities":{"session":{}}})
        }
    }
}
#[tokio::test]
async fn acp_r002_foreign_and_stale_strategy_requests_rejected_before_registration_or_write() {
    timeout(Duration::from_secs(10), async {
        for source_version in [Version::V1, Version::V2] {
            for target_version in [Version::V1, Version::V2] {
                let (mut source, _source_peer) = ready(source_version, source_version).await;
                let stale = source.strategy().expect("source strategy").clone();
                let (mut target, mut target_peer) =
                    ready_with_result(target_version, without_image(target_version)).await;
                assert!(
                    !target
                        .strategy()
                        .expect("target strategy")
                        .negotiated()
                        .capabilities
                        .image
                );
                let session = SessionId::parse(&json!("s")).expect("id");
                let request = stale
                    .prompt(
                        session.clone(),
                        json!([{"type":"image","data":"AA","mimeType":"image/png"}]),
                    )
                    .expect("source supports image");
                assert_eq!(
                    target
                        .request(request, deadline())
                        .await
                        .expect_err("cross strategy capability bypass rejected"),
                    AcpError::InvalidRequestScope
                );
                assert_eq!(target.pending_count(), 0);
                assert!(!target.is_closed());
                let mut line = String::new();
                assert!(
                    timeout(
                        Duration::from_millis(10),
                        target_peer.0.read_line(&mut line)
                    )
                    .await
                    .is_err()
                );
                assert!(line.is_empty());
                source
                    .shutdown(deadline())
                    .await
                    .expect("old source shutdown");
                let stale_request = stale
                    .resume(session, "C:/fixture")
                    .expect("old source supported resume");
                assert_eq!(
                    target
                        .request(stale_request, deadline())
                        .await
                        .expect_err("old connection strategy cannot revive"),
                    AcpError::InvalidRequestScope
                );
                assert_eq!(target.pending_count(), 0);
                assert!(!target.is_closed());
                let legitimate = target
                    .strategy()
                    .expect("target own strategy")
                    .new_session("C:/fixture")
                    .expect("target still usable");
                let id = target
                    .request(legitimate, deadline())
                    .await
                    .expect("own request");
                assert_eq!(id, RpcId::Integer(2));
                let sent_frame = sent(&mut target_peer).await;
                assert_eq!(sent_frame["method"], "session/new");
                feed(
                    &mut target_peer,
                    json!({"jsonrpc":"2.0","id":id.json(),"result":{"sessionId":"new"}}),
                )
                .await;
                assert!(matches!(
                    target.next(deadline()).await.expect("own response"),
                    Incoming::Response {
                        response: Response::NewSession(_),
                        ..
                    }
                ));
            }
        }
    })
    .await
    .expect("bounded R002 regression");
}
