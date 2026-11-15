//! Independent §6A public-API wire tests. Synthetic peers are not real Harness evidence.
use acp_remote::adapters::acp::{
    AcpError, ExtensionFields, Incoming, Request, Response, SessionEvent, SessionId, ToolNamePatch,
    UpdateOrigin, Version,
};
use serde_json::{Value, json};
use tokio::{
    io::{
        AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf,
    },
    time::{Duration, Instant, timeout},
};

use super::acp_runtime_legacy29_support as runtime;
use runtime::{LegacyId as RpcId, LegacySetup, RpcJson, TestClient as Client, raw_value};
struct Peer {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
}
fn pair() -> (Client, Peer) {
    let (application, agent) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(application);
    let (peer_reader, peer_writer) = tokio::io::split(agent);
    (
        Client::new(reader, writer),
        Peer {
            read: BufReader::new(peer_reader),
            write: peer_writer,
        },
    )
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn session(name: &str) -> SessionId {
    SessionId::parse(raw_value(&json!(name)).as_ref()).unwrap()
}
async fn wire(peer: &mut Peer) -> Value {
    let mut line = String::new();
    assert!(
        timeout(Duration::from_secs(2), peer.read.read_line(&mut line))
            .await
            .unwrap()
            .unwrap()
            > 0
    );
    serde_json::from_str(&line).unwrap()
}
async fn no_wire(peer: &mut Peer) {
    let mut byte = [0];
    match timeout(Duration::from_millis(15), peer.read.read(&mut byte)).await {
        Err(_) | Ok(Ok(0)) => {}
        other => panic!("unexpected additional wire byte: {other:?}"),
    }
}
async fn receive_bytes(
    client: &mut Client,
    peer: &mut Peer,
    bytes: &[u8],
) -> Result<Incoming, AcpError> {
    let (written, event) = tokio::join!(
        async {
            peer.write.write_all(bytes).await?;
            peer.write.write_all(b"\n").await
        },
        client.next(deadline())
    );
    written.unwrap();
    event
}
async fn receive(client: &mut Client, peer: &mut Peer, frame: Value) -> Result<Incoming, AcpError> {
    receive_bytes(client, peer, &serde_json::to_vec(&frame).unwrap()).await
}
fn initialized(version: Version, caps: &Value) -> Value {
    match version {
        Version::V1 => {
            json!({"protocolVersion":1,"agentInfo":{"name":"independent-peer","version":"synthetic"},"agentCapabilities":caps})
        }
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"independent-peer","version":"synthetic"},"capabilities":caps})
        }
    }
}
async fn ready_with(version: Version, caps: Value) -> (Client, Peer) {
    let (mut client, mut peer) = pair();
    let rpc = client.initialize_preferred(deadline()).await.unwrap();
    let offer = wire(&mut peer).await;
    assert_eq!(offer["method"], "initialize");
    assert_eq!(offer["params"]["protocolVersion"], 2);
    assert!(offer["params"]["info"].is_object());
    receive(
        &mut client,
        &mut peer,
        json!({"jsonrpc":"2.0","id":rpc.json(),"result":initialized(version,&caps)}),
    )
    .await
    .unwrap();
    assert_eq!(client.strategy().unwrap().negotiated().offered, Version::V2);
    assert_eq!(client.strategy().unwrap().version(), version);
    (client, peer)
}
async fn ready(version: Version) -> (Client, Peer) {
    let caps = match version {
        Version::V1 => json!({"loadSession":true,"sessionCapabilities":{"list":{},"resume":{}}}),
        Version::V2 => json!({"session":{}}),
    };
    ready_with(version, caps).await
}
fn read_request(client: &Client, replay: bool, target: &str) -> Request {
    let strategy = client.strategy().unwrap();
    if replay {
        strategy
            .legacy_replay(session(target), "C:/independent")
            .unwrap()
    } else {
        strategy
            .legacy_resume(session(target), "C:/independent")
            .unwrap()
    }
}
fn state(target: &str, state: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":target,"update":{"sessionUpdate":"state_update","state":state,"stopReason":"end_turn"}}})
}
async fn finish(client: &mut Client, peer: &mut Peer, rpc: &RpcId, target: &str) -> Incoming {
    if client.strategy().unwrap().version() == Version::V2 {
        receive(client, peer, state(target, "idle")).await.unwrap();
    }
    receive(
        client,
        peer,
        json!({"jsonrpc":"2.0","id":rpc.json(),"result":{"_meta":{"source":"synthetic"}}}),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn cl032_047_048_v1_capabilities_are_independent_and_v2_baseline_needs_no_extra_flags() {
    for mask in 0..8 {
        let mut caps = json!({"loadSession":mask&1!=0,"sessionCapabilities":{}});
        if mask & 2 != 0 {
            caps["sessionCapabilities"]["list"] = json!({});
        }
        if mask & 4 != 0 {
            caps["sessionCapabilities"]["resume"] = json!({});
        }
        let (client, mut peer) = ready_with(Version::V1, caps).await;
        let s = client.strategy().unwrap();
        assert_eq!(s.negotiated().capabilities.standard_replay, mask & 1 != 0);
        assert_eq!(
            s.list_sessions(None, None, ExtensionFields::empty().unwrap())
                .is_ok(),
            mask & 2 != 0
        );
        assert_eq!(
            s.legacy_replay(session("opaque"), "C:/independent").is_ok(),
            mask & 1 != 0
        );
        assert_eq!(
            s.legacy_resume(session("opaque"), "C:/independent").is_ok(),
            mask & 4 != 0
        );
        assert_eq!(client.pending_count(), 0);
        no_wire(&mut peer).await;
    }
    for caps in [json!({}), json!({"session":null}), json!({"session":{}})] {
        let enabled = caps["session"].is_object();
        let (client, mut peer) = ready_with(Version::V2, caps).await;
        let s = client.strategy().unwrap();
        assert_eq!(s.negotiated().capabilities.list, enabled);
        assert_eq!(s.negotiated().capabilities.standard_replay, enabled);
        assert_eq!(
            s.legacy_replay(session("opaque"), "C:/independent").is_ok(),
            enabled
        );
        no_wire(&mut peer).await;
    }
}

#[tokio::test]
async fn cl035_036_058_preferred_is_once_and_invalid_or_failed_negotiation_never_downgrades() {
    for response in [
        json!({"protocolVersion":3}),
        json!({"protocolVersion":2,"info":null}),
        json!({"protocolVersion":2,"info":{"name":"missing-version"}}),
    ] {
        let (mut client, mut peer) = pair();
        let rpc = client.initialize_preferred(deadline()).await.unwrap();
        wire(&mut peer).await;
        assert!(
            receive(
                &mut client,
                &mut peer,
                json!({"jsonrpc":"2.0","id":rpc.json(),"result":response})
            )
            .await
            .is_err()
        );
        assert!(client.is_closed());
        assert_eq!(client.pending_count(), 0);
        assert_eq!(
            client.initialize(Version::V1, deadline()).await,
            Err(AcpError::AlreadyInitialized)
        );
        no_wire(&mut peer).await;
    }
    for error_code in [-32_022, -32_000] {
        let (mut client, mut peer) = pair();
        let rpc = client.initialize_preferred(deadline()).await.unwrap();
        wire(&mut peer).await;
        assert!(
            matches!(receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":rpc.json(),"error":{"code":error_code,"message":"synthetic-auth-or-no-common"}})).await.unwrap(),Incoming::Response{response:Response::RemoteError{code},..} if code==error_code)
        );
        assert!(client.is_closed());
        assert!(client.initialize_preferred(deadline()).await.is_err());
        no_wire(&mut peer).await;
    }
    let (mut client, mut peer) = pair();
    let rpc = client.initialize(Version::V1, deadline()).await.unwrap();
    assert_eq!(wire(&mut peer).await["params"]["protocolVersion"], 1);
    assert_eq!(receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":rpc.json(),"result":initialized(Version::V2,&json!({"session":{}}))})).await.unwrap_err(),AcpError::IncompatibleVersion);
    no_wire(&mut peer).await;
}

#[tokio::test]
async fn cl058_initialization_deadline_eof_and_wrong_rpc_do_not_reoffer() {
    for branch in 0..3 {
        let (mut client, mut peer) = pair();
        let until = if branch == 0 {
            Instant::now() + Duration::from_millis(25)
        } else {
            deadline()
        };
        let rpc = client.initialize_preferred(until).await.unwrap();
        wire(&mut peer).await;
        let failure = match branch {
            0 => client.next(deadline()).await.unwrap_err(),
            1 => { peer.write.shutdown().await.unwrap(); client.next(deadline()).await.unwrap_err() }
            _ => receive(&mut client, &mut peer, json!({"jsonrpc":"2.0","id":rpc.json().to_string(),"result":initialized(Version::V1,&json!({}))})).await.unwrap_err(),
        };
        assert!(matches!(
            failure,
            AcpError::Deadline | AcpError::DeliveryUnknown | AcpError::UnmatchedResponse
        ));
        assert!(client.is_closed());
        assert_eq!(
            client.initialize(Version::V1, deadline()).await,
            Err(AcpError::AlreadyInitialized)
        );
        no_wire(&mut peer).await;
    }
}

#[tokio::test]
async fn cl032_048_peer_information_does_not_change_the_standard_replay_wire() {
    for version in [Version::V1, Version::V2] {
        let mut observed = Vec::new();
        for name in ["synthetic-program-A", "synthetic-program-B"] {
            let (mut client, mut peer) = pair();
            let rpc = client.initialize_preferred(deadline()).await.unwrap();
            wire(&mut peer).await;
            let caps = if version == Version::V1 {
                json!({"loadSession":true})
            } else {
                json!({"session":{}})
            };
            let mut result = initialized(version, &caps);
            let info = if version == Version::V1 {
                "agentInfo"
            } else {
                "info"
            };
            result[info]["name"] = json!(name);
            result[info]["version"] = json!(format!("unreported-{name}"));
            receive(
                &mut client,
                &mut peer,
                json!({"jsonrpc":"2.0","id":rpc.json(),"result":result}),
            )
            .await
            .unwrap();
            let request = read_request(&client, true, "same-native");
            client.request(request, deadline()).await.unwrap();
            let frame = wire(&mut peer).await;
            observed.push((frame["method"].clone(), frame["params"].clone()));
        }
        assert_eq!(observed[0], observed[1]);
    }
}

#[tokio::test]
async fn cl034_list_wire_omits_options_and_preserves_exact_raw_order_and_cursor() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(version).await;
        for (cwd, cursor) in [
            (None, None),
            (Some("C:/independent"), None),
            (None, Some("opaque-cursor")),
            (Some("C:/independent"), Some("opaque-cursor")),
        ] {
            let r = client
                .strategy()
                .unwrap()
                .list_sessions(cwd, cursor, ExtensionFields::empty().unwrap())
                .unwrap();
            let rpc = client.request(r, deadline()).await.unwrap();
            let sent = wire(&mut peer).await;
            assert_eq!(sent["method"], "session/list");
            assert_eq!(sent["params"].get("cwd"), cwd.map(Value::from).as_ref());
            assert_eq!(
                sent["params"].get("cursor"),
                cursor.map(Value::from).as_ref()
            );
            let next = cursor.map_or(Value::Null, |_| json!("next-uninterpreted"));
            let result = json!({"sessions":[{"sessionId":"later-source","cwd":"C:/second","title":"Alpha","_meta":{"arbitrary":[{"body":"synthetic-sensitive"}]}},{"sessionId":"earlier-source","cwd":"C:/first","title":"Zulu","additionalDirectories":["C:/additional"]}],"nextCursor":next,"future":{"keep":[1,true]}});
            let frame = json!({"jsonrpc":"2.0","id":rpc.json(),"result":result});
            let bytes = serde_json::to_vec(&frame).unwrap();
            let Incoming::Response {
                response: Response::Sessions(listing),
                raw,
                ..
            } = receive_bytes(&mut client, &mut peer, &bytes).await.unwrap()
            else {
                panic!("expected standard listing")
            };
            assert_eq!(raw.bytes(), bytes);
            assert_eq!(
                listing
                    .sessions()
                    .iter()
                    .map(|x| x.session().as_str())
                    .collect::<Vec<_>>(),
                ["later-source", "earlier-source"]
            );
            assert_eq!(listing.next_cursor(), next.as_str());
            assert!(!format!("{listing:?} {raw:?}").contains("synthetic-sensitive"));
            // Request a new page explicitly; the client must not follow nextCursor itself.
            no_wire(&mut peer).await;
        }
        let r = client
            .strategy()
            .unwrap()
            .list_sessions(None, None, ExtensionFields::empty().unwrap())
            .unwrap();
        let rpc = client.request(r, deadline()).await.unwrap();
        wire(&mut peer).await;
        assert!(
            matches!(receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":rpc.json(),"result":{"sessions":[]}})).await.unwrap(),Incoming::Response{response:Response::Sessions(s),..} if s.next_cursor().is_none()&&s.sessions().is_empty())
        );
    }
}

#[tokio::test]
async fn cl049_list_bounds_are_utf8_bytes_and_never_silently_drop_items() {
    for version in [Version::V1, Version::V2] {
        let (client, mut peer) = ready(version).await;
        let boundary = format!("{}x", "界".repeat(1365));
        assert_eq!(boundary.len(), 4096);
        let s = client.strategy().unwrap();
        assert!(
            s.list_sessions(
                Some(&boundary),
                Some(&boundary),
                ExtensionFields::empty().unwrap()
            )
            .is_ok()
        );
        assert!(matches!(
            s.list_sessions(
                Some(&(boundary.clone() + "x")),
                None,
                ExtensionFields::empty().unwrap()
            ),
            Err(AcpError::Capacity)
        ));
        assert!(matches!(
            s.list_sessions(
                None,
                Some(&(boundary.clone() + "x")),
                ExtensionFields::empty().unwrap()
            ),
            Err(AcpError::Capacity)
        ));
        assert!(matches!(
            s.legacy_replay(session("opaque"), &(boundary + "x")),
            Err(AcpError::Capacity)
        ));
        no_wire(&mut peer).await;
        for count in [4096, 4097] {
            let (mut client, mut peer) = ready(version).await;
            let r = client
                .strategy()
                .unwrap()
                .list_sessions(None, None, ExtensionFields::empty().unwrap())
                .unwrap();
            let rpc = client.request(r, deadline()).await.unwrap();
            wire(&mut peer).await;
            let event=receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":rpc.json(),"result":{"sessions":vec![json!({"sessionId":"repeated-source-id","cwd":"/"});count],"nextCursor":"bounded-next"}})).await;
            let Incoming::Response {
                response: Response::Sessions(listing),
                raw,
                ..
            } = event.unwrap()
            else {
                panic!("complete listing")
            };
            assert_eq!(listing.sessions().len(), count);
            assert_eq!(listing.next_cursor(), Some("bounded-next"));
            assert!(
                listing
                    .sessions()
                    .iter()
                    .all(|item| item.session().as_str() == "repeated-source-id")
            );
            assert_eq!(
                serde_json::from_slice::<Value>(raw.bytes()).unwrap()["result"]["sessions"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
            assert!(!client.is_closed());
        }
    }
}

#[tokio::test]
async fn cl034_050_invalid_list_shapes_fail_without_publishing_a_partial_list() {
    for version in [Version::V1, Version::V2] {
        for result in [
            json!({}),
            json!({"sessions":null}),
            json!({"sessions":[null]}),
            json!({"sessions":[{"sessionId":"valid","cwd":"/"},{"sessionId":"missing-cwd"}]}),
            json!({"sessions":[{"sessionId":9,"cwd":"/"}]}),
            json!({"sessions":[{"sessionId":"s","cwd":false}]}),
            json!({"sessions":[],"nextCursor":{}}),
        ] {
            let (mut client, mut peer) = ready(version).await;
            let r = client
                .strategy()
                .unwrap()
                .list_sessions(None, None, ExtensionFields::empty().unwrap())
                .unwrap();
            let rpc = client.request(r, deadline()).await.unwrap();
            wire(&mut peer).await;
            assert_eq!(
                receive(
                    &mut client,
                    &mut peer,
                    json!({"jsonrpc":"2.0","id":rpc.json(),"result":result})
                )
                .await
                .unwrap_err(),
                AcpError::InvalidMessage
            );
            assert!(client.is_closed());
            assert_eq!(client.pending_count(), 0);
            no_wire(&mut peer).await;
        }
    }
}

#[tokio::test]
async fn cl033_052_replay_and_no_replay_resume_use_their_own_standard_wire_and_completion() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(version).await;
        for replay in [false, true] {
            let request = read_request(&client, replay, "native-opaque");
            let rpc = client.request(request, deadline()).await.unwrap();
            let sent = wire(&mut peer).await;
            assert_eq!(
                sent["method"],
                if version == Version::V1 && replay {
                    "session/load"
                } else {
                    "session/resume"
                }
            );
            assert_eq!(sent["params"]["sessionId"], "native-opaque");
            assert_eq!(sent["params"]["mcpServers"], json!([]));
            assert_eq!(
                sent["params"].get("replayFrom"),
                (replay && version == Version::V2)
                    .then(|| json!({"type":"start"}))
                    .as_ref()
            );
            let event = finish(&mut client, &mut peer, &rpc, "native-opaque").await;
            assert_eq!(
                matches!(
                    event,
                    Incoming::Response {
                        response: Response::Replayed { .. },
                        ..
                    }
                ),
                replay
            );
            assert_eq!(client.pending_count(), 0);
            no_wire(&mut peer).await;
        }
    }
}

#[tokio::test]
async fn cl051_052_one_read_window_per_session_has_zero_write_conflicts_and_error_release() {
    for version in [Version::V1, Version::V2] {
        for first in [false, true] {
            for second in [false, true] {
                let (mut client, mut peer) = ready(version).await;
                let r = read_request(&client, first, "A");
                let a = client.request(r, deadline()).await.unwrap();
                wire(&mut peer).await;
                for _ in 0..2 {
                    let r = read_request(&client, second, "A");
                    assert_eq!(
                        client.request(r, deadline()).await,
                        Err(AcpError::SessionReadBusy)
                    );
                    assert_eq!(client.pending_count(), 1);
                    no_wire(&mut peer).await;
                }
                let b = client
                    .request(read_request(&client, true, "B"), deadline())
                    .await
                    .unwrap();
                wire(&mut peer).await;
                finish(&mut client, &mut peer, &b, "B").await;
                assert_eq!(client.pending_count(), 1);
                client.cancel(&session("A"), deadline()).await.unwrap();
                assert_eq!(wire(&mut peer).await["method"], "session/cancel");
                assert_eq!(
                    client
                        .request(read_request(&client, second, "A"), deadline())
                        .await,
                    Err(AcpError::SessionReadBusy)
                );
                no_wire(&mut peer).await;
                assert!(matches!(receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":a.json(),"error":{"code":-32602,"message":"synthetic"}})).await.unwrap(),Incoming::Response{response:Response::RemoteError{..},..}));
                assert_eq!(client.pending_count(), 0);
                no_wire(&mut peer).await;
                let explicit = client
                    .request(read_request(&client, second, "A"), deadline())
                    .await
                    .unwrap();
                wire(&mut peer).await;
                finish(&mut client, &mut peer, &explicit, "A").await;
            }
        }
    }
}

fn permission(version: Version, rpc: &Value) -> Value {
    let options = json!([{"optionId":"deny-one","kind":"reject_once","name":"Synthetic deny"}]);
    let params = match version {
        Version::V1 => {
            json!({"sessionId":"A","toolCall":{"toolCallId":"synthetic-tool"},"options":options})
        }
        Version::V2 => {
            json!({"sessionId":"A","title":"synthetic","subject":{"type":"tool_call","toolCall":{"toolCallId":"synthetic-tool"}},"options":options})
        }
    };
    json!({"jsonrpc":"2.0","id":rpc,"method":"session/request_permission","params":params})
}
fn update(target: &str) -> Value {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":target,"update":{"sessionUpdate":"future","body":"same synthetic text","_meta":{"nested":[true,{"keep":1}]}}}})
}
#[tokio::test]
async fn cl034_051_replay_origin_is_only_a_window_and_permissions_remain_independent() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(version).await;
        assert!(matches!(
            receive(&mut client, &mut peer, update("A")).await.unwrap(),
            Incoming::Update {
                origin: UpdateOrigin::Live,
                ..
            }
        ));
        let rpc = client
            .request(read_request(&client, true, "A"), deadline())
            .await
            .unwrap();
        wire(&mut peer).await;
        for (target, expected) in [
            ("A", UpdateOrigin::DuringReplay),
            ("B", UpdateOrigin::Live),
            ("A", UpdateOrigin::DuringReplay),
        ] {
            let frame = update(target);
            let bytes = serde_json::to_vec(&frame).unwrap();
            let Incoming::Update { origin, raw, .. } =
                receive_bytes(&mut client, &mut peer, &bytes).await.unwrap()
            else {
                panic!("update")
            };
            assert_eq!(origin, expected);
            assert_eq!(raw.bytes(), bytes);
        }
        for identity in [json!(17), json!("17")] {
            let Incoming::Permission(request) =
                receive(&mut client, &mut peer, permission(version, &identity))
                    .await
                    .unwrap()
            else {
                panic!("independent permission")
            };
            assert_eq!(request.id().json(), identity);
            no_wire(&mut peer).await;
            client
                .permission_response(&request, None, deadline())
                .await
                .unwrap();
            let response = wire(&mut peer).await;
            assert_eq!(response["id"], identity);
            assert_eq!(response["result"]["outcome"]["outcome"], "cancelled");
            assert!(
                client
                    .permission_response(&request, None, deadline())
                    .await
                    .is_err()
            );
            no_wire(&mut peer).await;
        }
        finish(&mut client, &mut peer, &rpc, "A").await;
        assert!(matches!(
            receive(&mut client, &mut peer, update("A")).await.unwrap(),
            Incoming::Update {
                origin: UpdateOrigin::Live,
                ..
            }
        ));
    }
}

#[tokio::test]
async fn cl033_052_v2_requires_a_same_window_snapshot_and_preserves_unknown_state() {
    for replay in [false, true] {
        for case in [0, 1, 2] {
            let (mut client, mut peer) = ready(Version::V2).await;
            if case == 1 {
                receive(&mut client, &mut peer, state("A", "idle"))
                    .await
                    .unwrap();
            }
            let rpc = client
                .request(read_request(&client, replay, "A"), deadline())
                .await
                .unwrap();
            wire(&mut peer).await;
            if case == 2 {
                receive(&mut client, &mut peer, state("B", "idle"))
                    .await
                    .unwrap();
            }
            assert_eq!(
                receive(
                    &mut client,
                    &mut peer,
                    json!({"jsonrpc":"2.0","id":rpc.json(),"result":{}})
                )
                .await
                .unwrap_err(),
                AcpError::MissingSnapshot
            );
            assert!(client.is_closed());
            assert_eq!(client.pending_count(), 0);
            no_wire(&mut peer).await;
        }
        for current in ["running", "idle", "requires_action", "future_state"] {
            let (mut client, mut peer) = ready(Version::V2).await;
            let rpc = client
                .request(read_request(&client, replay, "A"), deadline())
                .await
                .unwrap();
            wire(&mut peer).await;
            receive(&mut client, &mut peer, update("A")).await.unwrap();
            for _ in 0..2 {
                let Incoming::Update {
                    event: SessionEvent::State { state, .. },
                    origin,
                    ..
                } = receive(&mut client, &mut peer, state("A", current))
                    .await
                    .unwrap()
                else {
                    panic!("state fact, not prompt completion")
                };
                assert_eq!(state.as_ref(), current);
                assert_eq!(
                    origin,
                    if replay {
                        UpdateOrigin::DuringReplay
                    } else {
                        UpdateOrigin::DuringResume
                    }
                );
            }
            let event = receive(
                &mut client,
                &mut peer,
                json!({"jsonrpc":"2.0","id":rpc.json(),"result":{}}),
            )
            .await
            .unwrap();
            assert_eq!(
                matches!(
                    event,
                    Incoming::Response {
                        response: Response::Replayed { .. },
                        ..
                    }
                ),
                replay
            );
        }
    }
}

#[tokio::test]
async fn cl038_039_040_053_059_new_connections_recompute_capabilities_and_reject_foreign_requests()
{
    let (low, mut low_peer) = ready(Version::V1).await;
    let old_strategy = low.strategy().unwrap().clone();
    let (high, mut high_peer) = ready(Version::V2).await;
    assert_eq!(low.strategy().unwrap().version(), Version::V1);
    assert_eq!(
        old_strategy
            .tool_name_patch(raw_value(&json!({"name":null})).as_ref())
            .unwrap(),
        ToolNamePatch::Unchanged
    );
    assert_eq!(
        high.strategy()
            .unwrap()
            .tool_name_patch(raw_value(&json!({"name":null})).as_ref())
            .unwrap(),
        ToolNamePatch::Clear
    );
    for request in [
        old_strategy
            .list_sessions(None, None, ExtensionFields::empty().unwrap())
            .unwrap(),
        old_strategy
            .legacy_replay(session("A"), "C:/independent")
            .unwrap(),
        old_strategy
            .legacy_resume(session("A"), "C:/independent")
            .unwrap(),
    ] {
        assert_eq!(
            high.request(request, deadline()).await,
            Err(AcpError::InvalidRequestScope)
        );
        assert_eq!(high.pending_count(), 0);
        no_wire(&mut high_peer).await;
    }
    low.shutdown(deadline()).await.unwrap();
    assert_eq!(
        high.request(
            old_strategy
                .legacy_replay(session("A"), "C:/independent")
                .unwrap(),
            deadline()
        )
        .await,
        Err(AcpError::InvalidRequestScope)
    );
    no_wire(&mut high_peer).await;
    no_wire(&mut low_peer).await;
    let (absent, mut absent_peer) = ready_with(Version::V2, json!({})).await;
    assert!(
        !absent
            .strategy()
            .unwrap()
            .negotiated()
            .capabilities
            .standard_replay
    );
    assert!(
        absent
            .strategy()
            .unwrap()
            .legacy_replay(session("A"), "C:/independent")
            .is_err()
    );
    no_wire(&mut absent_peer).await;
}

#[tokio::test]
async fn cl050_059_business_errors_are_facts_without_initialize_or_side_effect_retry() {
    for operation in 0..3 {
        let (mut client, mut peer) = ready(Version::V2).await;
        let request = if operation == 0 {
            client
                .strategy()
                .unwrap()
                .list_sessions(None, None, ExtensionFields::empty().unwrap())
                .unwrap()
        } else {
            read_request(&client, operation == 1, "A")
        };
        let rpc = client.request(request, deadline()).await.unwrap();
        wire(&mut peer).await;
        let event=receive(&mut client,&mut peer,json!({"jsonrpc":"2.0","id":rpc.json(),"error":{"code":-32000,"message":"synthetic-sensitive-peer-message","data":{"keep":true}}})).await.unwrap();
        assert!(matches!(
            event,
            Incoming::Response {
                response: Response::RemoteError { code: -32000 },
                ..
            }
        ));
        assert!(!format!("{event:?}").contains("synthetic-sensitive-peer-message"));
        assert_eq!(client.pending_count(), 0);
        assert_eq!(client.strategy().unwrap().version(), Version::V2);
        assert_eq!(
            client.initialize(Version::V1, deadline()).await,
            Err(AcpError::AlreadyInitialized)
        );
        no_wire(&mut peer).await;
    }
}

#[tokio::test]
async fn cl050_deadline_eof_duplicate_and_typed_mismatch_poison_without_replay() {
    for case in 0..5 {
        let (mut client, mut peer) = ready(Version::V2).await;
        let r = client
            .strategy()
            .unwrap()
            .list_sessions(None, None, ExtensionFields::empty().unwrap())
            .unwrap();
        let until = if case == 0 {
            Instant::now() + Duration::from_millis(25)
        } else {
            deadline()
        };
        let rpc = client.request(r, until).await.unwrap();
        wire(&mut peer).await;
        let error = match case {
            0 => client.next(deadline()).await.unwrap_err(),
            1 => {
                peer.write.shutdown().await.unwrap();
                client.next(deadline()).await.unwrap_err()
            }
            2 => receive(
                &mut client,
                &mut peer,
                json!({"jsonrpc":"2.0","id":rpc.json().to_string(),"result":{"sessions":[]}}),
            )
            .await
            .unwrap_err(),
            3 => {
                let frame = json!({"jsonrpc":"2.0","id":rpc.json(),"result":{"sessions":[]}});
                receive(&mut client, &mut peer, frame.clone())
                    .await
                    .unwrap();
                receive(&mut client, &mut peer, frame).await.unwrap_err()
            }
            _ => receive_bytes(&mut client, &mut peer, b"{not-json}")
                .await
                .unwrap_err(),
        };
        assert!(matches!(
            error,
            AcpError::Deadline
                | AcpError::DeliveryUnknown
                | AcpError::UnmatchedResponse
                | AcpError::InvalidMessage
        ));
        assert!(client.is_closed());
        assert_eq!(client.pending_count(), 0);
        no_wire(&mut peer).await;
    }
}

#[tokio::test]
async fn cl049_pending_limit_and_structural_limits_remain_bounded() {
    let (client, mut peer) = ready(Version::V2).await;
    for _ in 0..64 {
        let r = client
            .strategy()
            .unwrap()
            .list_sessions(None, None, ExtensionFields::empty().unwrap())
            .unwrap();
        client.request(r, deadline()).await.unwrap();
        wire(&mut peer).await;
    }
    assert_eq!(client.pending_count(), 64);
    let r = client
        .strategy()
        .unwrap()
        .list_sessions(None, None, ExtensionFields::empty().unwrap())
        .unwrap();
    assert_eq!(client.request(r, deadline()).await, Err(AcpError::Capacity));
    no_wire(&mut peer).await;
    client.shutdown(deadline()).await.unwrap();
    assert_eq!(client.pending_count(), 0);
    for depth in [96, 97] {
        let (mut client, mut peer) = ready(Version::V2).await;
        let text = format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"_nested\",\"params\":{}0{}}}",
            "[".repeat(depth - 1),
            "]".repeat(depth - 1)
        );
        let result = receive_bytes(&mut client, &mut peer, text.as_bytes()).await;
        let Incoming::UnknownNotification { raw } = result.unwrap() else {
            panic!("full nested notification")
        };
        assert_eq!(raw.bytes(), text.as_bytes());
        assert!(!client.is_closed());
    }
    for items in [65_500, 65_540] {
        let (mut client, mut peer) = ready(Version::V2).await;
        let frame = json!({"jsonrpc":"2.0","method":"_token_budget","params":vec!["";items]});
        let result = receive(&mut client, &mut peer, frame.clone()).await;
        let Incoming::UnknownNotification { raw } = result.unwrap() else {
            panic!("complete token notification")
        };
        assert_eq!(serde_json::from_slice::<Value>(raw.bytes()).unwrap(), frame);
        assert!(!client.is_closed());
    }
}

#[tokio::test]
async fn cl049_frame_byte_boundary_is_enforced_through_public_connection() {
    const MAX: usize = 32 * 1024 * 1024;
    for extra in [0, 1] {
        let (mut client, mut peer) = ready(Version::V2).await;
        let prefix = br#"{"jsonrpc":"2.0","method":"_opaque","params":{"body":""#;
        let suffix = br#""}}"#;
        let mut bytes = prefix.to_vec();
        bytes.resize(MAX + extra - suffix.len(), b'x');
        bytes.extend_from_slice(suffix);
        assert_eq!(bytes.len(), MAX + extra);
        let event = receive_bytes(&mut client, &mut peer, &bytes).await;
        let Incoming::UnknownNotification { raw } = event.unwrap() else {
            panic!("opaque frame")
        };
        assert_eq!(raw.bytes().len(), MAX + extra);
        assert_eq!(raw.bytes(), bytes);
        assert!(!client.is_closed());
    }
}

#[tokio::test]
async fn cl050_cancelling_a_read_preserves_partial_bytes_but_cancelling_a_write_closes() {
    let (mut client, mut peer) = ready(Version::V2).await;
    let r = client
        .strategy()
        .unwrap()
        .list_sessions(None, None, ExtensionFields::empty().unwrap())
        .unwrap();
    let rpc = client.request(r, deadline()).await.unwrap();
    wire(&mut peer).await;
    let bytes = serde_json::to_vec(
        &json!({"jsonrpc":"2.0","id":rpc.json(),"result":{"sessions":[],"nextCursor":"retained"}}),
    )
    .unwrap();
    let split = bytes.len() / 2;
    peer.write.write_all(&bytes[..split]).await.unwrap();
    assert!(
        timeout(Duration::from_millis(20), client.next(deadline()))
            .await
            .is_err()
    );
    assert!(!client.is_closed());
    let event = receive_bytes(&mut client, &mut peer, &bytes[split..])
        .await
        .unwrap();
    assert!(
        matches!(event,Incoming::Response{response:Response::Sessions(s),..} if s.next_cursor()==Some("retained"))
    );
    assert_eq!(client.pending_count(), 0);
    no_wire(&mut peer).await;
    let (client, mut peer) = ready(Version::V2).await;
    let request = client
        .strategy()
        .unwrap()
        .prompt(
            session("A"),
            raw_value(&json!([{ "type":"text", "text":"x".repeat(2*1024*1024) }])),
            ExtensionFields::empty().unwrap(),
        )
        .unwrap();
    client.control.mode(3);
    let reservation = client
        .connection
        .reserve_request(
            request,
            acp_remote::adapters::acp::ResponseDeadline::Unbounded,
        )
        .unwrap();
    let ticket = client.enqueue(reservation, deadline()).unwrap();
    client.control.wait_pending().await;
    assert!(client.control.accepted() > 0);
    runtime::drop_pending_waiter(&ticket).await;
    assert!(!client.is_closed());
    assert!(matches!(
        ticket.cancel_and_wait().await,
        acp_remote::adapters::acp::WriteReport::Unknown { accepted: 8, .. }
    ));
    assert!(client.is_closed());
    assert_eq!(
        client
            .request(
                client
                    .strategy()
                    .unwrap()
                    .list_sessions(None, None, ExtensionFields::empty().unwrap())
                    .unwrap(),
                deadline()
            )
            .await,
        Err(AcpError::Closed)
    );
    assert!(client.owner.shutdown(deadline()).await.joined);
    drop(client);
    let mut partial = Vec::new();
    timeout(Duration::from_secs(2), peer.read.read_to_end(&mut partial))
        .await
        .unwrap()
        .unwrap();
    assert!(!partial.is_empty());
    assert!(!partial.ends_with(b"\n"));
}
