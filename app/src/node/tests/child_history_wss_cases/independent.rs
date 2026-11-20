use super::super::history_acceptance::{configure, connected, seed_turns, snapshot, start_node};
use super::*;

const QUERY: &str = "_io.github.lindongfang.acp-remote/child-history";
const EXT: &str = "io.github.lindongfang.acp-remote/child-history";
const CHILD: &str = "child-history";

struct Scenario {
    f: HarnessFixture,
    _owner: NodeProcessOwner,
    node: NodeRuntime,
    selection: SourceLaunchSelection,
    root: EntityId,
    native: String,
    id: String,
    key: SigningKey,
    control: ManagementSocket,
}
impl Scenario {
    async fn new(version: u8, options: serde_json::Value) -> Self {
        let f = HarnessFixture::new("hn029");
        configure(&f, &options);
        configure(&f, &json!({"protocolVersion":version}));
        let owner = start_node(&f);
        let node = owner.ready().await.unwrap();
        let (id, key, selection) = f.install_profile(&node, version, 112).await;
        let mut control = connected(&f, &node, &key, &id).await;
        let (root, native) = f
            .create_and_bind_target(&node, &mut control, &key, &id, &selection)
            .await;
        bind_child(&node, &selection, &native).await;
        configure(&f, &json!({"expectedChildRoot":native}));
        seed_turns(&node, &selection, &native, 25, "root", "user_message_chunk").await;
        Self {
            f,
            _owner: owner,
            node,
            selection,
            root,
            native,
            id,
            key,
            control,
        }
    }
    async fn observer(&self, byte: u8) -> ManagementSocket {
        let key = SigningKey::from_slice(&[byte; 32]).unwrap();
        let id = pair_browser(&self.node, &key, "Independent observer").await;
        connected(&self.f, &self.node, &key, &id).await
    }
    fn payload(&self) -> serde_json::Value {
        json!({"target":{"kind":"target","targetId":self.root.encode()},"child":CHILD,"mode":{"kind":"first"}})
    }
    fn cached_key(&self) -> NativeSessionKey {
        NativeSessionKey::from_config(Some(self.selection.source), CHILD).unwrap()
    }
    async fn assert_root_readable(&mut self) {
        let r=send_command_wait(&mut self.control,"history.page",json!({"target":{"kind":"target","targetId":self.root.encode()},"mode":{"kind":"first"}})).await;
        assert_eq!(r["result"]["page"]["lastTurn"], "25");
        let frames = snapshot(&mut self.control, &r).await;
        assert_eq!(frames.raw.len(), 10);
        assert!(
            frames
                .raw
                .iter()
                .all(|r| r["params"]["sessionId"] == self.native)
        );
    }
    async fn shutdown(mut self) {
        self.control.close(None).await.unwrap();
        self.node.shutdown(None).await.unwrap();
    }
}
fn child_frame(update: serde_json::Value) -> String {
    json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":CHILD,"update":update}})
        .to_string()
}

fn original_frames() -> Vec<String> {
    let mut frames = Vec::new();
    for i in 1..=12 {
        let text = if i == 1 {
            String::from("SPLIT_HERE")
        } else {
            format!("child-{i}")
        };
        let mut raw = child_frame(
            json!({"sessionUpdate":"user_message_chunk","messageId":i.to_string(),"content":{"type":"text","text":text},"unknown":1,"fixtureTag":"HN029-NATIVE-CONTENT-MARKER"}),
        );
        raw = raw.replace("\"unknown\":1", "\"unknown\" : 1.00");
        if i == 1 {
            let index = raw.find("SPLIT_HERE").unwrap();
            raw = raw.replace("SPLIT_HERE", &format!("{}字", "x".repeat(16383 - index)));
            assert!(std::str::from_utf8(&raw.as_bytes()[..16384]).is_err());
        }
        frames.push(raw);
        if i == 1 {
            frames.push(child_frame(json!({"sessionUpdate":"tool_call","toolCallId":"tool","title":"test tool","status":"completed","unclassified":{"value":7}})));
        }
    }
    frames
}
fn compare_page(frames: &DrainedStreamFrames, expected: &[String]) {
    assert_eq!(frames.snapshot_ends, 1);
    let mut actual: Vec<_> = frames.raw_bytes.iter().collect();
    actual.sort_by_key(|(id, _)| **id);
    assert!(
        actual
            .iter()
            .map(|(_, bytes)| bytes.as_slice())
            .eq(expected.iter().map(String::as_bytes)),
        "raw page records differ from the native byte fixture"
    );
}

#[tokio::test]
async fn hn_029_two_observers_receive_exact_native_bytes_in_both_protocols() {
    for version in [1, 2] {
        Box::pin(two_observers(version)).await;
    }
}
async fn two_observers(version: u8) {
    let mut s = Scenario::new(version, json!({"childHistory":true})).await;
    let expected = original_frames();
    let path = s.f.area.directory.join("peer-native.jsonl");
    let release = s.f.area.directory.join("grant-native-batch");
    std::fs::write(&path, format!("{}\n", expected.join("\n"))).unwrap();
    configure(
        &s.f,
        &json!({"childFramesFile":path,"childBarrierIndex":0,"childBarrierFile":release,"opaqueChildCursors":true}),
    );
    let mut b = s.observer(113).await;
    let mut c = s.observer(114).await;
    let child = s.cached_key();
    let scope = s.payload();
    let gate = async {
        let runtime = &s.node.session_services.as_ref().unwrap().native;
        timeout(Duration::from_secs(10), async {
            while !runtime
                .all_active_protocol_sources()
                .iter()
                .filter_map(|source| source.views().ok())
                .any(|view| view.child_observers(&child) == 2)
            {
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        })
        .await
        .expect("both authorized observers registered");
        assert_eq!(runtime.inspect_capacity().await.unwrap().occupied, 1);
        std::fs::write(release, b"complete").unwrap();
    };
    let (first, other, ()) = tokio::join!(
        send_command_wait(&mut b, "history.page", scope.clone()),
        send_command_wait(&mut c, "history.page", scope.clone()),
        gate
    );
    for r in [&first, &other] {
        let p = &r["result"]["page"];
        assert_eq!(p["firstTurn"], "1", "error {:?}", r["error"]);
        assert_eq!(p["lastTurn"], "10");
        assert_eq!(p["order"], "ascending");
        assert_eq!(p["newerCursor"].as_str().unwrap().len(), 144);
    }
    assert_eq!(
        first["result"]["page"]["generationId"],
        other["result"]["page"]["generationId"]
    );
    compare_page(&snapshot(&mut b, &first).await, &expected[..11]);
    compare_page(&snapshot(&mut c, &other).await, &expected[..11]);
    let mut next = scope;
    next["mode"] = json!({"kind":"continue","direction":"newer","cursor":first["result"]["page"]["newerCursor"]});
    let last = send_command_wait(&mut b, "history.page", next).await;
    assert_eq!(last["result"]["page"]["lastTurn"], "12");
    compare_page(&snapshot(&mut b, &last).await, &expected[11..]);
    let stored = s
        .node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store()
        .load_snapshot(&child)
        .await
        .unwrap()
        .unwrap();
    assert!(
        stored
            .records
            .iter()
            .map(|r| r.raw.bytes())
            .eq(expected.iter().map(String::as_bytes))
    );
    let logs = s.f.peer_logs();
    let calls: Vec<_> = logs.iter().filter(|e| e["method"] == QUERY).collect();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().enumerate().all(|(i, e)| e["batch"] == i
        && e["first"] == (i == 0)
        && e["rootMatches"] == true
        && e["childMatches"] == true));
    let init = logs.iter().find(|e| e["method"] == "initialize").unwrap();
    let caps = if init["params"]["protocolVersion"] == 1 {
        "clientCapabilities"
    } else {
        "capabilities"
    };
    assert_eq!(init["params"][caps]["_meta"][EXT]["version"], 1);
    let declaration = logs
        .iter()
        .find(|e| e["event"] == "childDeclaration")
        .unwrap();
    assert_eq!(declaration["selectedVersion"], version);
    assert_eq!(
        declaration["field"],
        if version == 1 {
            "agentCapabilities"
        } else {
            "capabilities"
        }
    );
    assert_eq!(declaration["matching"], true);
    assert!(!logs.iter().any(|e| e["params"]["sessionId"] == CHILD));
    s.assert_root_readable().await;
    b.close(None).await.unwrap();
    c.close(None).await.unwrap();
    s.shutdown().await;
}

#[tokio::test]
async fn hn_029_negotiation_is_connection_local_and_no_implicit_probes() {
    for version in [1, 2] {
        for declaration in [
            "absent",
            "wrongLocation",
            "wrongVersion",
            "methodOnly",
            "cancelOnly",
            "refused",
        ] {
            let s=Scenario::new(version,json!({"childHistory":declaration!="absent","childDeclaration":declaration,"childRefused":declaration=="refused"})).await;
            let mut b = s.observer(116).await;
            for _ in 0..2 {
                let r = send_command_wait(&mut b, "history.page", s.payload()).await;
                assert_eq!(
                    r["error"],
                    json!({"code":"unsupported_capability"}),
                    "version {version}, declaration {declaration}"
                );
                assert!(r["result"].is_null());
            }
            let count =
                s.f.peer_logs()
                    .iter()
                    .filter(|e| e["method"] == QUERY)
                    .count();
            assert_eq!(count, usize::from(declaration == "refused"));
            seed_turns(
                &s.node,
                &s.selection,
                CHILD,
                12,
                "known",
                "user_message_chunk",
            )
            .await;
            let cached = send_command_wait(&mut b, "history.page", s.payload()).await;
            assert_eq!(cached["result"]["page"]["lastTurn"], "10");
            assert_eq!(snapshot(&mut b, &cached).await.raw.len(), 10);
            assert_eq!(
                s.f.peer_logs()
                    .iter()
                    .filter(|e| e["method"] == QUERY)
                    .count(),
                count
            );
            b.close(None).await.unwrap();
            s.shutdown().await;
        }
    }
}

#[tokio::test]
async fn hn_020_hn_029_rejected_native_batches_leave_parent_and_other_source_usable() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let mut s = Scenario::new(2, json!({"childHistory":true})).await;
    let child = s.cached_key();
    let original_root = s.native.clone();
    s.f.source_id = tc_id(118);
    let (other_id, other_key, other_sel) = s.f.install_profile(&s.node, 1, 119).await;
    configure(
        &s.f,
        &json!({"fixedSessionId":original_root,"protocolVersion":1}),
    );
    let mut other = s.observer(120).await;
    let mut controller = connected(&s.f, &s.node, &other_key, &other_id).await;
    let (other_target, other_root) =
        s.f.create_and_bind_target(&s.node, &mut controller, &other_key, &other_id, &other_sel)
            .await;
    assert_eq!(other_root, original_root);
    bind_child(&s.node, &other_sel, &other_root).await;
    seed_turns(
        &s.node,
        &other_sel,
        CHILD,
        12,
        "sibling",
        "user_message_chunk",
    )
    .await;
    let other_payload = json!({"target":{"kind":"target","targetId":other_target.encode()},"child":CHILD,"mode":{"kind":"first"}});
    let path = s.f.area.directory.join("bad-chain.jsonl");
    std::fs::write(&path, format!("{}\n", original_frames().join("\n"))).unwrap();
    configure(&s.f, &json!({"childFramesFile":path,"protocolVersion":2}));
    let v1_only = child_frame(
        json!({"sessionUpdate":"user_message_chunk","messageId":null,"content":{"type":"text","text":"version"}}),
    );
    let bad_method =
        child_frame(json!({"sessionUpdate":"future"})).replace("session/update", "session/prompt");
    let wrong_child = child_frame(json!({"sessionUpdate":"future"})).replace(CHILD, "sibling");
    let text = child_frame(json!({"sessionUpdate":"future"}));
    let cases = [
        ("root", 1, json!({"rootSessionId":"other"})),
        ("child", 1, json!({"sessionId":"sibling"})),
        ("empty snapshot", 0, json!({"snapshotId":""})),
        ("changed snapshot", 1, json!({"snapshotId":"changed"})),
        ("start sequence", 0, json!({"sequence":"1"})),
        ("non-canonical sequence", 1, json!({"sequence":"01"})),
        ("skipped sequence", 1, json!({"sequence":"2"})),
        ("repeated sequence", 1, json!({"sequence":"0"})),
        ("repeated cursor", 1, json!({"nextCursor":"1"})),
        (
            "nonterminal no bytes",
            1,
            json!({"chunk":"","nextCursor":"2"}),
        ),
        ("base64url", 0, json!({"chunk":"=="})),
        (
            "oversized chunk",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode(vec![b'a';16385])}),
        ),
        (
            "invalid UTF8",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode([0xff,10]),"nextCursor":null}),
        ),
        (
            "open line",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode(text.as_bytes()),"nextCursor":null}),
        ),
        (
            "method",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode(format!("{bad_method}\n")),"nextCursor":null}),
        ),
        (
            "sibling frame",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode(format!("{wrong_child}\n")),"nextCursor":null}),
        ),
        (
            "negotiated version",
            0,
            json!({"chunk":URL_SAFE_NO_PAD.encode(format!("{v1_only}\n")),"nextCursor":null}),
        ),
    ];
    let mut bad = s.observer(121).await;
    for (i, (label, index, patch)) in cases.into_iter().enumerate() {
        if i == 8 {
            bad.close(None).await.unwrap();
            bad = s.observer(122).await;
            other.close(None).await.unwrap();
            other = s.observer(123).await;
        }
        configure(&s.f, &json!({"faultAtBatch":index,"childFault":patch}));
        let payload = s.payload();
        let (r, sibling, ()) = tokio::join!(
            send_command_wait(&mut bad, "history.page", payload),
            send_command_wait(&mut other, "history.page", other_payload.clone()),
            s.assert_root_readable()
        );
        assert_eq!(r["ok"], false, "case {label}");
        assert!(r["result"].is_null(), "case {label} returned a stream");
        assert_eq!(r["error"].as_object().unwrap().len(), 1, "case {label}");
        assert_eq!(
            sibling["result"]["page"]["lastTurn"], "10",
            "other source failed in {label}"
        );
        let good = snapshot(&mut other, &sibling).await;
        assert!(good.raw.iter().all(|r| {
            r["params"]["update"]["content"]["text"]
                .as_str()
                .unwrap()
                .starts_with("sibling")
        }));
        assert!(
            s.node
                .session_services
                .as_ref()
                .unwrap()
                .transcript_cache
                .store()
                .load_snapshot(&child)
                .await
                .unwrap()
                .is_none(),
            "partial publication in {label}"
        );
    }
    configure(
        &s.f,
        &json!({"childFault":{"chunk":"","nextCursor":null},"faultAtBatch":0}),
    );
    let empty = send_command_wait(&mut bad, "history.page", s.payload()).await;
    assert_eq!(empty["result"]["page"]["turnStatus"], "unprovable");
    assert_eq!(empty["result"]["page"]["totalTurns"], "0");
    assert!(
        empty["result"]["page"]["olderCursor"].is_null()
            && empty["result"]["page"]["newerCursor"].is_null()
    );
    assert!(snapshot(&mut bad, &empty).await.raw.is_empty());
    let mut locate = s.payload();
    locate["mode"] = json!({"kind":"locate","fromTurn":"1","direction":"newer"});
    assert_eq!(
        send_command_wait(&mut bad, "history.page", locate).await["error"],
        json!({"code":"unsupported_capability"})
    );
    bad.close(None).await.unwrap();
    other.close(None).await.unwrap();
    controller.close(None).await.unwrap();
    s.shutdown().await;
}

#[tokio::test]
async fn hn_020_native_child_and_grandchild_events_preserve_root_turns() {
    let mut s = Scenario::new(1, json!({"childHistory":false,"replayTurns":25})).await;
    let db = s.node.storage.as_ref().unwrap().pool_for_test();
    for (name, parent) in [
        ("grandchild", CHILD),
        ("no-native-frames", &s.native),
        ("agent-only", &s.native),
    ] {
        sqlx::query("INSERT INTO node_session_parents SELECT ?,?,?,source_instance_id,runtime_epoch,connection_epoch,X'0000000000000001',X'0000000000000001',zeroblob(32),0 FROM node_source_session_operations WHERE native_session_id=?")
        .bind(s.selection.source.encode()).bind(name).bind(parent).bind(&s.native).execute(db).await.unwrap();
    }
    let gate = s.f.area.directory.join("root-work");
    configure(
        &s.f,
        &json!({"replayChildEvents":[{"id":CHILD,"last":10},{"id":"grandchild","last":3},{"id":"agent-only","last":4,"agentOnly":true}],"promptChildEvents":[{"id":CHILD,"first":11,"last":12}],"promptBarrierReleaseFile":gate}),
    );
    release_target(&mut s.control, s.root).await;
    let snapshot_pool = s
        .node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store()
        .pool_for_test()
        .await
        .unwrap();
    let removed = sqlx::query("DELETE FROM transcript_sessions WHERE native_session_id=?")
        .bind(&s.native)
        .execute(&snapshot_pool)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(removed, 1);
    drop(snapshot_pool);
    let root_page = send_command_wait(
        &mut s.control,
        "view.open",
        json!({"targetId":s.root.encode(),"reason":"explicit_refresh","page":{"turns":"5"}}),
    )
    .await;
    let saved = root_page["result"]["page"]["turns"].clone();
    assert_eq!(root_page["result"]["page"]["firstTurn"], "21");
    snapshot(&mut s.control, &root_page).await;
    wait_for_peer_method(&s.f, "session/load", 1).await;
    assert!(
        s.f.peer_logs().iter().any(|e| e["event"] == "autoDelivery"
            && e["sessionId"] == "grandchild"
            && e["last"] == 3)
    );
    let mut b = s.observer(124).await;
    let mut req = s.payload();
    req["child"] = json!("grandchild");
    req["turns"] = json!("3");
    let grand = send_command_wait(&mut b, "history.page", req.clone()).await;
    assert_eq!(
        grand["result"]["page"]["lastTurn"], "3",
        "error {:?}",
        grand["error"]
    );
    assert_eq!(snapshot(&mut b, &grand).await.raw.len(), 6);
    for id in ["no-native-frames", "agent-only"] {
        req["child"] = json!(id);
        let unavailable = send_command_wait(&mut b, "history.page", req.clone()).await;
        assert_eq!(
            unavailable["error"],
            json!({"code":"unsupported_capability"}),
            "child {id}"
        );
    }
    req["child"] = json!(CHILD);
    let before = send_command_wait(
        &mut s.control,
        "session.control.inspect",
        json!({"targetId":s.root.encode()}),
    )
    .await;
    assert_eq!(
        submit_prompt_turn(&mut s.control, s.root, "holding root").await["ok"],
        true
    );
    wait_for_peer_method(&s.f, "session/prompt", 1).await;
    let queued = send_command_wait(
        &mut s.control,
        "input.queue",
        json!({"targetId":s.root.encode()}),
    )
    .await;
    let previous = s.f.peer_logs().len();
    for _ in 0..20 {
        let first = send_command_wait(&mut b, "history.page", req.clone()).await;
        assert_eq!(first["result"]["page"]["firstTurn"], "1");
        assert_eq!(first["result"]["page"]["lastTurn"], "3");
        let frames = snapshot(&mut b, &first).await;
        assert_eq!(frames.raw.len(), 6);
        assert!(frames.raw.iter().all(|r| r["params"]["sessionId"] == CHILD));
    }
    assert_eq!(previous, s.f.peer_logs().len());
    assert_eq!(
        send_command_wait(
            &mut s.control,
            "input.queue",
            json!({"targetId":s.root.encode()})
        )
        .await["result"],
        queued["result"]
    );
    assert_eq!(
        send_command_wait(
            &mut s.control,
            "session.control.inspect",
            json!({"targetId":s.root.encode()})
        )
        .await["result"],
        before["result"]
    );
    assert_eq!(
        send_command_wait(&mut s.control, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        1
    );
    std::fs::write(gate, b"finish root work").unwrap();
    let store = s
        .node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    timeout(Duration::from_secs(10), async {
        loop {
            if store
                .load_snapshot(&s.cached_key())
                .await
                .unwrap()
                .is_some_and(|v| v.session.record_count == 24)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native live delivery appended the child's last two turns");
    let first = send_command_wait(&mut b, "history.page", s.payload()).await;
    assert_eq!(first["result"]["page"]["lastTurn"], "10");
    let mut newer = s.payload();
    newer["mode"] = json!({"kind":"continue","direction":"newer","cursor":first["result"]["page"]["newerCursor"]});
    let last = send_command_wait(&mut b, "history.page", newer).await;
    assert_eq!(last["result"]["page"]["firstTurn"], "11");
    assert_eq!(last["result"]["page"]["lastTurn"], "12");
    assert_eq!(snapshot(&mut b, &last).await.raw.len(), 4);
    let root=send_command_wait(&mut s.control,"history.page",json!({"target":{"kind":"target","targetId":s.root.encode()},"mode":{"kind":"first"},"turns":"5"})).await;
    assert_eq!(root["result"]["page"]["turns"], saved);
    assert!(!s.f.peer_logs().iter().any(|e| e["method"] == QUERY));
    b.close(None).await.unwrap();
    s.shutdown().await;
}

#[derive(Clone, Copy, Debug)]
enum Changed {
    Relation,
    Device,
    Configuration,
    SourceDisconnected,
}

#[tokio::test]
async fn hn_029_current_authority_at_next_batch_and_final_publication() {
    for change in [
        Changed::Relation,
        Changed::Device,
        Changed::Configuration,
        Changed::SourceDisconnected,
    ] {
        for terminal in [false, true] {
            Box::pin(change_before_release(change, terminal)).await;
        }
    }
}

async fn change_before_release(change: Changed, terminal: bool) {
    use crate::{
        adapters::acp::{RawMessage, Version},
        transcript_cache::CapabilityProfile,
    };
    let mut s = Scenario::new(
        1,
        json!({"childHistory":true,"watchDisconnect":matches!(change,Changed::SourceDisconnected)}),
    )
    .await;
    let (_, _, new_sel) = s.f.install_profile(&s.node, 1, 130).await;
    let reader_key = SigningKey::from_slice(&[131; 32]).unwrap();
    let reader_id = pair_browser(&s.node, &reader_key, "Authority change").await;
    let mut b = connected(&s.f, &s.node, &reader_key, &reader_id).await;
    let store = s
        .node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store()
        .clone();
    let gate = s.f.area.directory.join("native-release");
    if !terminal {
        configure(
            &s.f,
            &json!({"childBarrierIndex":0,"childBarrierFile":gate}),
        );
    }
    let guard =
        NativeSessionKey::from_config(Some(s.selection.source), "test-publish-barrier").unwrap();
    let expectation = store.history_generation(&guard).await.unwrap();
    let raw =
        RawMessage::parse(child_frame(json!({"sessionUpdate":"future"})).into_bytes()).unwrap();
    let (ready, locked) = tokio::sync::oneshot::channel();
    let (release, unblock) = tokio::sync::oneshot::channel();
    let publication = store.replace_checked(
        &guard,
        Version::V1,
        CapabilityProfile::ReplayCapable,
        vec![(raw, 1)],
        Some(expectation),
        async {
            ready.send(()).unwrap();
            unblock.await.unwrap();
            Ok(())
        },
    );
    let work = async {
        locked.await.unwrap();
        let request_id = Id128::new([132; 16]).to_base64url();
        send_json(&mut b,&json!({"v":1,"type":"command","requestId":request_id,"action":"history.page","payload":s.payload()})).await;
        if terminal {
            timeout(Duration::from_secs(8), async {
                while !s
                    .f
                    .peer_logs()
                    .iter()
                    .any(|e| e["event"] == "childBatchSent" && e["last"] == true)
                {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("native final bytes arrived while publication is held");
        } else {
            wait_for_peer_method(&s.f, QUERY, 1).await;
        }
        match change {
            Changed::SourceDisconnected => {
                let signal = s.f.cfg_path.with_extension("json.disconnect");
                std::fs::write(signal, b"disconnect this peer").unwrap();
                timeout(Duration::from_secs(8), async {
                    while !s
                        .node
                        .session_services
                        .as_ref()
                        .unwrap()
                        .native
                        .all_active_protocol_sources()
                        .is_empty()
                    {
                        tokio::time::sleep(Duration::from_millis(15)).await;
                    }
                })
                .await
                .expect("the original native source owner closed");
            }
            Changed::Device => {
                crate::ipc::device_revoke(s.node.ipc_address(), &reader_id)
                    .await
                    .unwrap();
            }
            Changed::Relation => {
                let n = sqlx::query(
                    "DELETE FROM node_session_parents WHERE source_id=? AND child_native_id=?",
                )
                .bind(s.selection.source.encode())
                .bind(CHILD)
                .execute(s.node.storage.as_ref().unwrap().pool_for_test())
                .await
                .unwrap()
                .rows_affected();
                assert_eq!(n, 1);
            }
            Changed::Configuration => {
                set_fixture_profile(&s, &new_sel).await;
            }
        }
        if !terminal {
            std::fs::write(&gate, b"finish native response").unwrap();
        }
        release.send(()).unwrap();
        if matches!(change, Changed::Device) {
            assert_eq!(next_close(&mut b, Duration::from_secs(5)).await.0, 4410);
        } else {
            let r = wait_for_history_reply(&mut b, &request_id, Duration::from_secs(10)).await;
            assert_eq!(r["ok"], false, "{change:?} terminal={terminal}");
            assert!(r["result"].is_null());
        }
        let c = s.cached_key();
        timeout(Duration::from_secs(5), async {
            loop {
                if s.node
                    .session_services
                    .as_ref()
                    .unwrap()
                    .native
                    .all_active_protocol_sources()
                    .iter()
                    .filter_map(|src| src.views().ok())
                    .all(|v| v.child_observers(&c) == 0)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        // A subsequent authorized query has finished reading the parent before we inspect storage.
        if matches!(change, Changed::Configuration) {
            set_fixture_profile(&s, &s.selection).await;
        }
        s.assert_root_readable().await;
        assert!(
            store.load_snapshot(&c).await.unwrap().is_none(),
            "{change:?} terminal={terminal} published a late child history"
        );
        let events = s.f.peer_logs();
        assert_eq!(
            events.iter().filter(|e| e["method"] == QUERY).count(),
            if terminal { 2 } else { 1 }
        );
        assert!(!events.iter().any(|e| e["method"] == "session/cancel"));
    };
    let (result, ()) = tokio::join!(publication, work);
    result.unwrap();
    s.shutdown().await;
}

async fn set_fixture_profile(s: &Scenario, selected: &SourceLaunchSelection) {
    let n = sqlx::query(
        "UPDATE node_target_launch_selection SET profile_id=?,profile_revision=? WHERE target_id=?",
    )
    .bind(selected.profile.id().encode())
    .bind(selected.profile.revision().to_be_bytes().to_vec())
    .bind(s.root.encode())
    .execute(s.node.storage.as_ref().unwrap().pool_for_test())
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(n, 1);
}

async fn wait_for_history_reply(
    sock: &mut ManagementSocket,
    req: &str,
    wait: Duration,
) -> serde_json::Value {
    timeout(wait, async {
        loop {
            match sock.next().await.unwrap().unwrap() {
                Message::Text(text) => {
                    let response: serde_json::Value = serde_json::from_str(text.as_str()).unwrap();
                    if response["requestId"] == req {
                        return response;
                    }
                }
                Message::Ping(_) | Message::Pong(_) => {}
                _ => panic!("unexpected frame before failed history response"),
            }
        }
    })
    .await
    .expect("history request completed within the native progress deadline")
}

#[tokio::test]
async fn hn_029_missing_or_mismatched_response_id_times_out_without_publication() {
    for mismatch in [false, true] {
        let mut s = Scenario::new(1, json!({"childHistory":true})).await;
        let mut b = s.observer(133).await;
        let gate = s.f.area.directory.join("no-reply");
        if mismatch {
            configure(&s.f, &json!({"unmatchedChildReply":true}));
        } else {
            configure(
                &s.f,
                &json!({"childBarrierIndex":0,"childBarrierFile":gate}),
            );
        }
        let id = Id128::new([134; 16]).to_base64url();
        send_json(&mut b,&json!({"v":1,"type":"command","requestId":id,"action":"history.page","payload":s.payload()})).await;
        wait_for_peer_method(&s.f, QUERY, 1).await;
        let result = wait_for_history_reply(&mut b, &id, Duration::from_secs(38)).await;
        assert_eq!(result["ok"], false);
        assert!(result["result"].is_null());
        if !mismatch {
            std::fs::write(gate, b"late original response").unwrap();
        }
        assert!(
            s.node
                .session_services
                .as_ref()
                .unwrap()
                .transcript_cache
                .store()
                .load_snapshot(&s.cached_key())
                .await
                .unwrap()
                .is_none()
        );
        s.assert_root_readable().await;
        assert_eq!(
            s.f.peer_logs()
                .iter()
                .filter(|e| e["method"] == QUERY)
                .count(),
            1
        );
        assert_eq!(
            submit_prompt_turn(&mut s.control, s.root, "work after child query timeout").await["ok"],
            true
        );
        wait_for_peer_method(&s.f, "session/prompt", 1).await;
        b.close(None).await.unwrap();
        s.shutdown().await;
    }
}

#[tokio::test]
async fn hn_029_reconnection_requires_fresh_declaration() {
    let mut s = Scenario::new(1, json!({"childHistory":true,"replayTurns":25})).await;
    let payload = s.payload();
    let first = send_command_wait(&mut s.control, "history.page", payload.clone()).await;
    assert_eq!(first["result"]["page"]["lastTurn"], "10");
    snapshot(&mut s.control, &first).await;
    release_target(&mut s.control, s.root).await;
    let store = s
        .node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let db = store.pool_for_test().await.unwrap();
    sqlx::query("UPDATE transcript_sessions SET last_updated_at_ms=0 WHERE native_session_id=?")
        .bind(CHILD)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        store
            .enforce_retention_and_budget(None)
            .await
            .unwrap()
            .expired_sessions,
        1
    );
    drop(db);
    s.control.close(None).await.unwrap();
    s.node.shutdown(None).await.unwrap();
    drop(s._owner);
    configure(&s.f, &json!({"childHistory":false}));
    let owner = start_node(&s.f);
    let node = owner.ready().await.unwrap();
    let mut control = connected(&s.f, &node, &s.key, &s.id).await;
    let restored = send_command_wait(
        &mut control,
        "view.open",
        json!({"targetId":s.root.encode(),"reason":"explicit_refresh","page":{"turns":"10"}}),
    )
    .await;
    assert_eq!(restored["result"]["page"]["lastTurn"], "25");
    snapshot(&mut control, &restored).await;
    let calls =
        s.f.peer_logs()
            .iter()
            .filter(|e| e["method"] == QUERY)
            .count();
    assert_eq!(calls, 2);
    let no_extension = send_command_wait(&mut control, "history.page", payload).await;
    assert_eq!(
        no_extension["error"],
        json!({"code":"unsupported_capability"})
    );
    assert_eq!(
        s.f.peer_logs()
            .iter()
            .filter(|e| e["method"] == QUERY)
            .count(),
        calls
    );
    control.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}
