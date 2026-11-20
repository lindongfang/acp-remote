use super::*;

#[tokio::test]
async fn hn_011_verified_cold_replay_retention_and_capacity() {
    use serde_json::json;
    let f = HarnessFixture::new("hn011");
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.cfg_path).unwrap()).unwrap();
    cfg["replayTurns"] = json!(25);
    std::fs::write(&f.cfg_path, cfg.to_string()).unwrap();
    let start = || {
        NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            f.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        )
    };
    let owner = start();
    let node = owner.ready().await.unwrap();
    let (id, key, sel) = f.install_profile(&node, 1, 77).await;
    let mut sock = authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        &key,
        &id,
        node_public_key(&node),
    )
    .await;
    let unverified = f
        .register_and_bind_target(&node, &key, &id, &sel, "external")
        .await;
    let before = f.peer_logs().len();
    let denied = send_command_wait(
        &mut sock,
        "view.open",
        json!({"targetId":unverified.encode(),"reason":"first_open","page":{"turns":"10"}}),
    )
    .await;
    assert_eq!(denied["error"], json!({"code":"session_tree_unverified"}));
    assert!(denied["result"].is_null());
    assert_eq!(f.peer_logs().len(), before);
    assert_eq!(
        send_command_wait(&mut sock, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        0
    );
    let (target, native) = f
        .create_and_bind_target(&node, &mut sock, &key, &id, &sel)
        .await;
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    drop(owner);
    let owner = start();
    let node = owner.ready().await.unwrap();
    let mut sock = authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        &key,
        &id,
        node_public_key(&node),
    )
    .await;
    let first = send_command_wait(
        &mut sock,
        "view.open",
        json!({"targetId":target.encode(),"reason":"first_open","page":{"turns":"10"}}),
    )
    .await;
    let page = &first["result"]["page"];
    assert_eq!(
        (&page["firstTurn"], &page["lastTurn"], &page["source"]),
        (&json!("16"), &json!("25"), &json!("replay"))
    );
    let scope = json!({"kind":"target","targetId":target.encode()});
    let older = json!({"target":scope,"turns":"10","mode":{"kind":"continue","direction":"older","cursor":page["olderCursor"]}});
    let locate = json!({"target":scope,"turns":"10","mode":{"kind":"locate","direction":"older","fromTurn":"15"}});
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let pool = store.pool_for_test().await.unwrap();
    let indexed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM transcript_turns WHERE native_session_id=?")
            .bind(&native)
            .fetch_one(&pool)
            .await
            .unwrap();

    let close_page =
        |response: &serde_json::Value| json!({"streamId":response["result"]["streamId"]});
    assert_eq!(
        send_command_wait(&mut sock, "view.close", close_page(&first)).await["ok"],
        true
    );
    release_target(&mut sock, target).await;
    let loads = || {
        f.peer_logs()
            .iter()
            .filter(|e| e["method"] == "session/load")
            .count()
    };
    let processes = || {
        f.peer_logs()
            .iter()
            .filter(|e| e["method"] == "initialize")
            .count()
    };
    let count = processes();
    let cached = send_command_wait(&mut sock, "history.page", older.clone()).await;
    let p = &cached["result"]["page"];
    assert_eq!(
        (&p["firstTurn"], &p["lastTurn"], &p["source"]),
        (&json!("6"), &json!("15"), &json!("transcript_cache"))
    );
    assert_eq!(loads(), 1);
    assert_eq!(processes(), count);
    assert_eq!(
        send_command_wait(&mut sock, "view.close", close_page(&cached)).await["ok"],
        true
    );
    let expire = || async {
        sqlx::query("UPDATE transcript_sessions SET last_updated_at_ms=last_updated_at_ms-2678400000 WHERE native_session_id=?").bind(&native).execute(&pool).await.unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                if store
                    .enforce_retention_and_budget(None)
                    .await
                    .unwrap()
                    .expired_sessions
                    == 1
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("expired unprotected session is evicted");
    };
    expire().await;
    let stale = send_command_wait(&mut sock, "history.page", older).await;
    assert_eq!(stale["error"], json!({"code":"stale_cursor"}));
    assert!(stale["result"].is_null());
    let relocated = send_command_wait(&mut sock, "history.page", locate.clone()).await;
    let p = &relocated["result"]["page"];
    assert_eq!(
        (&p["firstTurn"], &p["lastTurn"]),
        (&json!("6"), &json!("15"))
    );
    assert_eq!(p["relocated"], true);
    assert_eq!(p["continuity"], "unproven");
    assert_ne!(p["generationId"], page["generationId"]);
    assert_eq!(loads(), 2);
    assert_eq!(
        send_command_wait(&mut sock, "view.close", close_page(&relocated)).await["ok"],
        true
    );
    release_target(&mut sock, target).await;
    expire().await;
    let second = SigningKey::from_slice(&[78; 32]).unwrap();
    let second_id = pair_browser(&node, &second, "HN capacity").await;
    let mut fill = authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        &second,
        &second_id,
        node_public_key(&node),
    )
    .await;
    let (holder, _) = f
        .create_and_bind_target(&node, &mut fill, &second, &second_id, &sel)
        .await;
    for _ in 1..50 {
        let reg = send_command_wait(
            &mut fill,
            "source.session.register",
            json!({"selection":selection_json(&sel),"operation":{"kind":"create"}}),
        )
        .await;
        assert_eq!(reg["ok"], true);
        let exec = send_command_wait(
            &mut fill,
            "source.session.execute",
            json!({"sourceOperationId":reg["result"]["sourceOperationId"]}),
        )
        .await;
        assert_eq!(exec["ok"], true);
    }
    let count = processes();
    assert_eq!(
        send_command_wait(&mut sock, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        50
    );
    let rejected = send_command_wait(&mut sock, "history.page", locate.clone()).await;
    assert_eq!(rejected["error"], json!({"code":"session_capacity_full"}));
    assert!(rejected["result"].is_null());
    assert_eq!(processes(), count);
    release_target(&mut sock, holder).await;
    assert_eq!(
        send_command_wait(&mut sock, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        49
    );
    let retry = send_command_wait(&mut sock, "history.page", locate).await;
    assert_eq!(retry["result"]["page"]["firstTurn"], "6");
    assert_eq!(retry["result"]["page"]["lastTurn"], "15");
    assert_eq!(loads(), 3);
    fill.close(None).await.unwrap();
    sock.close(None).await.unwrap();
    drop(pool);
    node.shutdown(None).await.unwrap();
    assert_eq!(indexed, 25, "turn rows immediately after first replay");
}

async fn connected(
    f: &HarnessFixture,
    node: &NodeRuntime,
    key: &SigningKey,
    id: &str,
) -> ManagementSocket {
    authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        key,
        id,
        node_public_key(node),
    )
    .await
}

fn configure(f: &HarnessFixture, additions: &serde_json::Value) {
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&f.cfg_path).unwrap()).unwrap();
    for (key, value) in additions.as_object().unwrap() {
        cfg[key] = value.clone();
    }
    std::fs::write(&f.cfg_path, cfg.to_string()).unwrap();
}

async fn record_child(node: &NodeRuntime, sel: &SourceLaunchSelection, parent: &str, child: &str) {
    let inserted = sqlx::query("INSERT INTO node_session_parents SELECT ?, ?, ?,source_instance_id,runtime_epoch,connection_epoch,X'0000000000000001',X'0000000000000001',zeroblob(32),0 FROM node_source_session_operations WHERE native_session_id=?")
        .bind(sel.source.encode()).bind(child).bind(parent).bind(parent).execute(node.storage.as_ref().unwrap().pool_for_test()).await.unwrap();
    assert_eq!(inserted.rows_affected(), 1);
}

#[tokio::test]
async fn hn_012_running_parent_without_auto_delivery() {
    Box::pin(check_running_parent(false)).await;
}

#[tokio::test]
async fn hn_012_running_parent_with_auto_delivery() {
    Box::pin(check_running_parent(true)).await;
}

async fn check_running_parent(delivers: bool) {
    use serde_json::json;
    let f = HarnessFixture::new("hn012");
    let release = f.area.directory.join("prompt-release");
    configure(
        &f,
        &json!({"replayTurns":25,"promptBarrierReleaseFile":release}),
    );
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        f.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    if delivers {
        configure(&f, &json!({"autoChildSession":"child"}));
    }
    let (id, key, sel) = f.install_profile(&node, 1, 80).await;
    let mut a = connected(&f, &node, &key, &id).await;
    let (root, native) = f
        .create_and_bind_target(&node, &mut a, &key, &id, &sel)
        .await;
    record_child(&node, &sel, &native, "child").await;
    let other = SigningKey::from_slice(&[81; 32]).unwrap();
    let other_id = pair_browser(&node, &other, "Child reader").await;
    let mut b = connected(&f, &node, &other, &other_id).await;
    assert_eq!(
        submit_prompt_turn(&mut a, root, "running").await["ok"],
        true
    );
    wait_for_peer_method(&f, "session/prompt", 1).await;
    let payload = json!({"target":{"kind":"target","targetId":root.encode()},"child":"child","mode":{"kind":"first"}});
    let count = f.peer_logs().len();
    let busy = send_command_wait(&mut b, "history.page", payload.clone()).await;
    std::fs::write(&release, b"done").unwrap();
    assert_eq!(busy["error"], json!({"code":"invalid_stage"}));
    assert!(busy["result"].is_null());
    assert_eq!(f.peer_logs().len(), count);
    let idle = timeout(Duration::from_secs(10), async {
        loop {
            let page = send_command_wait(&mut b, "history.page", payload.clone()).await;
            if page["error"]["code"] != "invalid_stage" {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        send_command_wait(&mut b, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        1
    );
    assert!(
        f.peer_logs()
            .iter()
            .all(|e| e["method"] != "session/cancel")
    );
    b.close(None).await.unwrap();
    a.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    if delivers {
        assert_eq!(
            idle["result"]["page"]["firstTurn"], "1",
            "after auto-delivery: {:?}",
            idle["error"]
        );
        assert_eq!(idle["result"]["page"]["lastTurn"], "10");
    } else {
        assert_eq!(idle["error"], json!({"code":"unsupported_capability"}));
    }
}

async fn seed_turns(
    node: &NodeRuntime,
    sel: &SourceLaunchSelection,
    native: &str,
    turns: usize,
    prefix: &str,
    kind: &str,
) {
    use crate::{
        adapters::acp::{RawMessage, Version},
        transcript_cache::CapabilityProfile,
    };
    let key = NativeSessionKey::from_config(Some(sel.source), native).unwrap();
    let frames=(1..=turns).map(|i| {
        let update=serde_json::json!({"sessionUpdate":kind,"messageId":format!("{prefix}{i}"),"content":{"type":"text","text":format!("{prefix}{i:04}")}});
        let raw=serde_json::json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":native,"update":update}});
        (RawMessage::parse(raw.to_string().into_bytes()).unwrap(),i as u64)
    }).collect();
    node.session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store()
        .replace_with_replay(&key, Version::V1, CapabilityProfile::ResumeOnly, frames)
        .await
        .unwrap();
}

async fn snapshot(sock: &mut ManagementSocket, reply: &serde_json::Value) -> DrainedStreamFrames {
    let stream =
        Id128::parse_base64url(reply["result"]["streamId"].as_str().expect("page opened")).unwrap();
    let frames = read_until_snapshot_end(sock, stream).await;
    assert_eq!(
        send_command_wait(sock, "view.close", serde_json::json!({"streamId":stream})).await["ok"],
        true
    );
    frames
}

#[tokio::test]
async fn hn_019_cursors_reject_authorized_targets_across_sources_and_sessions() {
    use serde_json::json;
    let mut f = HarnessFixture::new("hn019");
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        f.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    let (id, key, one) = f.install_profile(&node, 1, 82).await;
    f.source_id = tc_id(111);
    let (_, _, two) = f.install_profile(&node, 1, 83).await;
    let mut sock = connected(&f, &node, &key, &id).await;
    let mut targets = Vec::new();
    for (sel, native) in [(&one, "same"), (&one, "other"), (&two, "same")] {
        f.register_and_bind_target(&node, &key, &id, sel, native)
            .await;
        seed_turns(&node, sel, native, 25, "U", "user_message_chunk").await;
        targets.push(
            json!({"kind":"source","selection":selection_json(sel),"nativeSessionId":native}),
        );
    }
    let mut pages = Vec::new();
    for target in &targets {
        let page = send_command_wait(
            &mut sock,
            "history.page",
            json!({"target":target,"mode":{"kind":"first"}}),
        )
        .await;
        assert_eq!(page["result"]["page"]["firstTurn"], "16");
        snapshot(&mut sock, &page).await;
        pages.push(page);
    }
    for target in &targets[1..] {
        let reply=send_command_wait(&mut sock,"history.page",json!({"target":target,"mode":{"kind":"continue","direction":"older","cursor":pages[0]["result"]["page"]["olderCursor"]}})).await;
        assert_eq!(reply["error"], json!({"code":"invalid_cursor"}));
        assert!(reply["result"].is_null());
    }
    assert!(f.peer_logs().is_empty());
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_017_cold_page_rewrite_and_hn_024_unprovable_view() {
    use serde_json::json;
    let f = HarnessFixture::new("hn017");
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        f.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    let (id, key, sel) = f.install_profile(&node, 1, 84).await;
    let target = f
        .register_and_bind_target(&node, &key, &id, &sel, "s")
        .await;
    let mut sock = connected(&f, &node, &key, &id).await;
    let scope = json!({"kind":"target","targetId":target.encode()});
    let locate = |turn: usize| json!({"target":scope,"mode":{"kind":"locate","direction":"older","fromTurn":turn.to_string()}});
    seed_turns(&node, &sel, "s", 1000, "U", "user_message_chunk").await;
    let first = send_command_wait(&mut sock, "history.page", locate(820)).await;
    assert_eq!(first["result"]["page"]["firstTurn"], "811");
    snapshot(&mut sock, &first).await;
    for prefix in ["C", "W"] {
        seed_turns(&node, &sel, "s", 600, prefix, "user_message_chunk").await;
        let stale=send_command_wait(&mut sock,"history.page",json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":first["result"]["page"]["olderCursor"]}})).await;
        assert_eq!(stale["error"], json!({"code":"stale_cursor"}));
        let missing = send_command_wait(&mut sock, "history.page", locate(810)).await;
        assert_eq!(missing["error"], json!({"code":"not_found"}));
        assert!(missing["result"].is_null());
        let new = send_command_wait(&mut sock, "history.page", locate(500)).await;
        let p = &new["result"]["page"];
        assert_eq!(p["totalTurns"], "600");
        assert_eq!(p["firstTurn"], "491");
        assert_eq!(p["lastTurn"], "500");
        assert_eq!(p["continuity"], "unproven");
        let content = snapshot(&mut sock, &new).await;
        assert_eq!(content.raw.len(), 10);
        for (i, frame) in content.raw.iter().enumerate() {
            assert!(
                frame["params"]["update"]["content"]["text"] == format!("{prefix}{:04}", 491 + i),
                "page body differs at offset {i}"
            );
        }
    }
    seed_turns(&node, &sel, "s", 25, "A", "agent_message_chunk").await;
    let whole = send_command_wait(
        &mut sock,
        "view.open",
        json!({"targetId":target.encode(),"reason":"first_open","page":{"turns":"10"}}),
    )
    .await;
    let p = &whole["result"]["page"];
    assert_eq!(p["turnStatus"], "unprovable");
    for key in ["firstTurn", "lastTurn", "olderCursor", "newerCursor"] {
        assert!(p[key].is_null());
    }
    assert_eq!(snapshot(&mut sock, &whole).await.raw.len(), 25);
    let denied = send_command_wait(&mut sock, "history.page", locate(1)).await;
    assert_eq!(denied["error"], json!({"code":"unsupported_capability"}));
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

fn start_node(f: &HarnessFixture) -> NodeProcessOwner {
    NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        f.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    )
}

struct ColdRoot {
    id: String,
    key: SigningKey,
    selection: SourceLaunchSelection,
    target: EntityId,
    native: String,
}

async fn cold_root(f: &HarnessFixture) -> ColdRoot {
    let owner = start_node(f);
    let node = owner.ready().await.unwrap();
    let (id, key, selection) = f.install_profile(&node, 1, 85).await;
    let mut sock = connected(f, &node, &key, &id).await;
    let (target, native) = f
        .create_and_bind_target(&node, &mut sock, &key, &id, &selection)
        .await;
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    ColdRoot {
        id,
        key,
        selection,
        target,
        native,
    }
}

#[tokio::test]
async fn hn_013_five_mib_output_survives_reopen_and_reassembly() {
    use serde_json::json;
    use sha2::Digest;
    let f = HarnessFixture::new("hn013");
    let size = 5 * 1024 * 1024;
    configure(&f, &json!({"replayTurns":25,"largeToolBytes":size}));
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let request = json!({"target":{"kind":"target","targetId":root.target.encode()},"mode":{"kind":"locate","fromTurn":"15","direction":"older"}});
    let first = send_command_wait(&mut sock, "history.page", request.clone()).await;
    let stream = Id128::parse_base64url(
        first["result"]["streamId"]
            .as_str()
            .expect("large turn page opened"),
    )
    .unwrap();
    sock.send(Message::Binary(
        TransferHeader::credit(stream, 0)
            .encode(&[])
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    let mut partial = 0;
    while partial < size / 2 {
        let frame = timeout(Duration::from_secs(5), sock.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Binary(bytes) = frame {
            let (hdr, payload) = TransferHeader::decode(&bytes).unwrap();
            assert!(payload.len() <= 16 * 1024);
            if hdr.record_kind() == Some(RecordKind::OriginalRaw) && hdr.total() > size as u64 {
                partial += payload.len();
            }
            if hdr.record_kind() == Some(RecordKind::OriginalRaw)
                && hdr.total() > size as u64
                && hdr.offset() == 0
            {
                let waiting = timeout(Duration::from_millis(120), async {
                    loop {
                        let frame = sock.next().await.unwrap().unwrap();
                        if matches!(frame, Message::Binary(_)) {
                            break;
                        }
                    }
                })
                .await;
                assert!(waiting.is_err(), "sender advanced without the next credit");
            }
            sock.send(Message::Binary(
                TransferHeader::credit(stream, hdr.packet())
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
        }
    }
    sock.close(None).await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let reopened = send_command_wait(&mut sock, "history.page", request).await;
    let page = &reopened["result"]["page"];
    assert_eq!(page["firstTurn"], "6");
    assert_eq!(page["lastTurn"], "15");
    let frames = snapshot(&mut sock, &reopened).await;
    let outputs: Vec<_> = frames
        .raw
        .iter()
        .filter_map(|v| v["params"]["update"]["rawOutput"].as_str())
        .collect();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].len(), size);
    assert!(outputs[0].bytes().all(|b| b == b'x'));
    let large_hash = |raw: &DrainedStreamFrames| {
        use sha2::{Digest, Sha256};
        let records = raw
            .raw_bytes
            .values()
            .filter(|bytes| bytes.len() > size)
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        Sha256::digest(records[0])
    };
    let cold_hash = large_hash(&frames);
    let live = send_command_wait(
        &mut sock,
        "view.open",
        json!({"targetId":root.target.encode(),"reason":"first_open","page":{"turns":"25"}}),
    )
    .await;
    assert_eq!(live["result"]["page"]["source"], "live");
    let live_frames = snapshot(&mut sock, &live).await;
    assert_eq!(large_hash(&live_frames), cold_hash);
    let original = format!(
        r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"{}","update":{{"sessionUpdate":"tool_call_update","toolCallId":"big","rawOutput":"{}","status":"completed"}}}}}}"#,
        root.native,
        "x".repeat(size)
    );
    assert_eq!(sha2::Sha256::digest(original.as_bytes()), cold_hash);
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_016_parent_thousand_anonymous_turns_relocate_then_finish() {
    use serde_json::json;
    let f = HarnessFixture::new("hn016");
    configure(&f, &json!({"replayTurns":1000,"anonymousTurns":true}));
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let mut mode = json!({"kind":"first"});
    for index in 0..100 {
        if index > 0 && index % 25 == 0 {
            sock.close(None).await.unwrap();
            let other =
                SigningKey::from_slice(&[90 + u8::try_from(index / 25).unwrap(); 32]).unwrap();
            let id = pair_browser(&node, &other, "Independent page reader").await;
            sock = connected(&f, &node, &other, &id).await;
        }
        if index == 50 {
            release_target(&mut sock, root.target).await;
            let store = node
                .session_services
                .as_ref()
                .unwrap()
                .transcript_cache
                .store();
            let pool = store.pool_for_test().await.unwrap();
            sqlx::query(
                "UPDATE transcript_sessions SET last_updated_at_ms=0 WHERE native_session_id=?",
            )
            .bind(&root.native)
            .execute(&pool)
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
            let stale = send_command_wait(
                &mut sock,
                "history.page",
                json!({"target":scope,"mode":mode}),
            )
            .await;
            assert_eq!(stale["error"], json!({"code":"stale_cursor"}));
            mode = json!({"kind":"locate","fromTurn":"500","direction":"older"});
        }
        let page = send_command_wait(
            &mut sock,
            "history.page",
            json!({"target":scope,"mode":mode}),
        )
        .await;
        let p = &page["result"]["page"];
        let low = 991 - 10 * index;
        assert_eq!(
            p["firstTurn"],
            low.to_string(),
            "page {index} error {:?}",
            page["error"]
        );
        assert_eq!(p["lastTurn"], (low + 9).to_string());
        assert_eq!(p["relocated"], index == 50);
        let frames = snapshot(&mut sock, &page).await;
        assert_eq!(frames.raw.len(), 20);
        for (i, pair) in frames.raw.as_chunks::<2>().0.iter().enumerate() {
            for (frame, prefix) in pair.iter().zip(["U", "A"]) {
                assert!(
                    frame["params"]["update"]["content"]["text"]
                        == format!("{prefix}{:04}", low + i),
                    "page {index}, offset {i}"
                );
            }
        }
        assert_eq!(p["olderCursor"].is_null(), index == 99);
        mode = json!({"kind":"continue","direction":"older","cursor":p["olderCursor"]});
    }
    assert_eq!(
        f.peer_logs()
            .iter()
            .filter(|e| e["method"] == "session/load")
            .count(),
        2
    );
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_023_interleaved_clients_share_cursors_preserve_control_and_queue() {
    use serde_json::json;
    let f = HarnessFixture::new("hn023");
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let (id, key, sel) = f.install_profile(&node, 1, 89).await;
    let mut a = connected(&f, &node, &key, &id).await;
    let (target, native) = f
        .create_and_bind_target(&node, &mut a, &key, &id, &sel)
        .await;
    takeover_target(&mut a, target).await;
    seed_turns(&node, &sel, &native, 25, "U", "user_message_chunk").await;
    let bkey = SigningKey::from_slice(&[90; 32]).unwrap();
    let bid = pair_browser(&node, &bkey, "Observer").await;
    let mut b = connected(&f, &node, &bkey, &bid).await;
    let bound = json!({"targetId":target.encode()});
    let initial = send_command_wait(&mut a, "session.control.inspect", bound.clone()).await;
    assert_eq!(initial["result"]["controlEpoch"], "1");
    let queue = send_command_wait(&mut a, "input.queue", bound.clone()).await;
    let scope = json!({"kind":"target","targetId":target.encode()});
    let mut cursor = serde_json::Value::Null;
    for i in 0..50 {
        let shared = i % 4 == 3;
        let (sock, size) = if i % 2 == 0 {
            (&mut a, 10)
        } else {
            (&mut b, if shared { 10 } else { 5 })
        };
        let mode = if shared {
            json!({"kind":"continue","direction":"older","cursor":cursor})
        } else {
            json!({"kind":"first"})
        };
        let response = send_command_wait(
            sock,
            "history.page",
            json!({"target":scope,"turns":size.to_string(),"mode":mode}),
        )
        .await;
        let p = &response["result"]["page"];
        let first = if shared { 6 } else { 26 - size };
        assert_eq!(p["firstTurn"], first.to_string());
        assert_eq!(p["lastTurn"], (first + size - 1).to_string());
        if i % 2 == 0 {
            cursor = p["olderCursor"].clone();
        }
        snapshot(sock, &response).await;
    }
    assert_eq!(
        send_command_wait(&mut a, "session.control.inspect", bound.clone()).await["result"],
        initial["result"]
    );
    assert_eq!(
        send_command_wait(&mut a, "input.queue", bound).await["result"],
        queue["result"]
    );
    assert!(
        f.peer_logs()
            .iter()
            .all(|e| e["method"] != "session/prompt" && e["method"] != "session/cancel")
    );
    crate::ipc::device_revoke(node.ipc_address(), &bid)
        .await
        .unwrap();
    assert_eq!(next_close(&mut b, Duration::from_secs(5)).await.0, 4410);
    a.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

async fn expire_cache(node: &NodeRuntime, native: &str) {
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let db = store.pool_for_test().await.unwrap();
    assert_eq!(
        sqlx::query(
            "UPDATE transcript_sessions SET last_updated_at_ms=0 WHERE native_session_id=?"
        )
        .bind(native)
        .execute(&db)
        .await
        .unwrap()
        .rows_affected(),
        1
    );
    timeout(Duration::from_secs(5), async {
        loop {
            if store
                .enforce_retention_and_budget(None)
                .await
                .unwrap()
                .expired_sessions
                > 0
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("evict expired cache");
}

#[tokio::test]
async fn hn_021_resume_only_cold_views_and_retention() {
    Box::pin(check_nonreplayable("resume_only")).await;
}

#[tokio::test]
async fn hn_021_pure_memory_cold_views_and_retention() {
    Box::pin(check_nonreplayable("pure_memory")).await;
}
async fn check_nonreplayable(mode: &str) {
    use serde_json::json;
    let f = HarnessFixture::new("hn021");
    configure(&f, &json!({"capabilityMode":mode,"echoUserChunk":true}));
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let native_gate = f.area.directory.join("native-turns");
    if mode == "pure_memory" {
        configure(&f, &json!({"nativeTurnGate":native_gate}));
    }
    let (id, key, sel) = f.install_profile(&node, 1, 86).await;
    let mut sock = connected(&f, &node, &key, &id).await;
    let (target, native) = f
        .create_and_bind_target(&node, &mut sock, &key, &id, &sel)
        .await;
    if mode == "pure_memory" {
        std::fs::write(native_gate, b"emit").unwrap();
        let store = node
            .session_services
            .as_ref()
            .unwrap()
            .transcript_cache
            .store();
        let session = NativeSessionKey::from_config(Some(sel.source), &native).unwrap();
        timeout(Duration::from_secs(10), async {
            loop {
                if store
                    .load_snapshot(&session)
                    .await
                    .unwrap()
                    .is_some_and(|s| s.session.record_count == 50)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("pure memory peer's original 25 native turns reached the cache");
    } else {
        for turn in 1..=25 {
            if turn % 10 == 0 {
                sock.close(None).await.unwrap();
                sock = connected(&f, &node, &key, &id).await;
            }
            assert_eq!(
                submit_prompt_turn(&mut sock, target, &format!("U{turn:04}")).await["ok"],
                true
            );
            wait_for_peer_method(&f, "session/prompt", turn).await;
        }
    }
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    drop(owner);
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &key, &id).await;
    let initial = f.peer_logs().len();
    let open =
        |reason: &str| json!({"targetId":target.encode(),"reason":reason,"page":{"turns":"10"}});
    let first = send_command_wait(&mut sock, "view.open", open("first_open")).await;
    assert_eq!(first["result"]["status"]["capabilityBoundary"], mode);
    let p = &first["result"]["page"];
    assert_eq!(p["source"], "transcript_cache");
    assert_eq!(p["firstTurn"], "16");
    snapshot(&mut sock, &first).await;
    let scope = json!({"kind":"target","targetId":target.encode()});
    let previous = |cursor: &serde_json::Value| json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":cursor}});
    let middle = send_command_wait(&mut sock, "history.page", previous(&p["olderCursor"])).await;
    assert_eq!(middle["result"]["page"]["firstTurn"], "6");
    snapshot(&mut sock, &middle).await;
    let oldest = send_command_wait(
        &mut sock,
        "history.page",
        previous(&middle["result"]["page"]["olderCursor"]),
    )
    .await;
    assert_eq!(oldest["result"]["page"]["firstTurn"], "1");
    snapshot(&mut sock, &oldest).await;
    configure(&f, &json!({"replayTurns":26}));
    let refreshed = send_command_wait(&mut sock, "view.open", open("explicit_refresh")).await;
    assert_eq!(
        refreshed["result"]["page"]["generationId"],
        p["generationId"]
    );
    snapshot(&mut sock, &refreshed).await;
    let old_again = send_command_wait(&mut sock, "history.page", previous(&p["olderCursor"])).await;
    assert_eq!(old_again["result"]["page"]["firstTurn"], "6");
    snapshot(&mut sock, &old_again).await;
    assert_eq!(f.peer_logs().len(), initial);
    expire_cache(&node, &native).await;
    let missing = send_command_wait(
        &mut sock,
        "history.page",
        json!({"target":scope,"mode":{"kind":"first"}}),
    )
    .await;
    takeover_target(&mut sock, target).await;
    let input = submit_prompt_turn(&mut sock, target, "after cache retention").await;
    if mode == "resume_only" {
        wait_for_peer_method(&f, "session/prompt", 26).await;
        assert_eq!(input["ok"], true);
        assert!(
            f.peer_logs()
                .iter()
                .any(|e| e["method"] == "session/resume")
        );
    } else {
        assert_eq!(first["result"]["status"]["continuable"], false);
        // The queue acknowledgement is an admission fact. Check native execution separately after shutdown.
        assert_eq!(input["result"]["request"]["state"], "settled");
        assert_eq!(input["result"]["request"]["outcome"]["kind"], "queued");
    }
    let inventory = send_command_wait(&mut sock, "catalog.inventory", json!({})).await;
    let capacity = send_command_wait(&mut sock, "session.capacity.inspect", json!({})).await;
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    if mode == "pure_memory" {
        assert!(
            f.peer_logs()
                .iter()
                .all(|e| e["method"] != "session/prompt" && e["method"] != "session/resume")
        );
    }
    assert!(
        inventory["result"]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["targetId"] == target.encode() && row["nativeSessionId"] == native)
    );
    assert_eq!(capacity["ok"], true);
    assert_eq!(capacity["result"]["limit"], 50);
    assert!(
        capacity["result"]["occupied"]
            .as_u64()
            .is_some_and(|n| n <= 1)
    );
    assert_eq!(
        missing["error"],
        json!({"code":"unsupported_capability"}),
        "capability {mode}"
    );
}

#[tokio::test]
async fn hn_015_refresh_then_relocate_after_native_rewrite() {
    use serde_json::json;
    let f = HarnessFixture::new("hn015");
    configure(&f, &json!({"replayTurns":25}));
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let open = |reason: &str| json!({"targetId":root.target.encode(),"reason":reason,"page":{"turns":"10"}});
    let first = send_command_wait(&mut sock, "view.open", open("first_open")).await;
    let p = &first["result"]["page"];
    assert_eq!(p["lastTurn"], "25");
    snapshot(&mut sock, &first).await;
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let older = |cursor: &serde_json::Value| json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":cursor}});
    let mid = send_command_wait(&mut sock, "history.page", older(&p["olderCursor"])).await;
    let cursor = mid["result"]["page"]["olderCursor"].clone();
    snapshot(&mut sock, &mid).await;
    let same = send_command_wait(&mut sock, "view.open", open("explicit_refresh")).await;
    assert_eq!(same["result"]["page"]["generationId"], p["generationId"]);
    snapshot(&mut sock, &same).await;
    let oldest = send_command_wait(&mut sock, "history.page", older(&cursor)).await;
    assert_eq!(oldest["result"]["page"]["lastTurn"], "5");
    snapshot(&mut sock, &oldest).await;
    configure(&f, &json!({"replayTurns":26,"turnPrefix":"C"}));
    let changed = send_command_wait(&mut sock, "view.open", open("explicit_refresh")).await;
    assert_eq!(changed["result"]["page"]["lastTurn"], "26");
    assert_ne!(changed["result"]["page"]["generationId"], p["generationId"]);
    snapshot(&mut sock, &changed).await;
    let stale = send_command_wait(&mut sock, "history.page", older(&cursor)).await;
    assert_eq!(stale["error"], json!({"code":"stale_cursor"}));
    let located = send_command_wait(
        &mut sock,
        "history.page",
        json!({"target":scope,"mode":{"kind":"locate","fromTurn":"5","direction":"older"}}),
    )
    .await;
    assert_eq!(located["result"]["page"]["continuity"], "unproven");
    let frames = snapshot(&mut sock, &located).await;
    assert_eq!(frames.raw.len(), 5);
    for (i, raw) in frames.raw.iter().enumerate() {
        assert!(
            raw["params"]["update"]["content"]["text"].eq(&format!("C{:04}", i + 1)),
            "rewritten body at offset {i}"
        );
    }
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_014_live_append_preserves_middle_page_cursors() {
    use crate::adapters::acp::{RawMessage, Version};
    use serde_json::json;
    let f = HarnessFixture::new("hn014");
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let (id, key, sel) = f.install_profile(&node, 1, 88).await;
    let target = f
        .register_and_bind_target(&node, &key, &id, &sel, "s")
        .await;
    let mut sock = connected(&f, &node, &key, &id).await;
    seed_turns(&node, &sel, "s", 25, "U", "user_message_chunk").await;
    let native = NativeSessionKey::from_config(Some(sel.source), "s").unwrap();
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let scope = json!({"kind":"target","targetId":target.encode()});
    let request = |mode| json!({"target":scope,"mode":mode});
    let mid = send_command_wait(
        &mut sock,
        "history.page",
        request(json!({"kind":"locate","fromTurn":"15","direction":"older"})),
    )
    .await;
    let p = &mid["result"]["page"];
    snapshot(&mut sock, &mid).await;
    for turn in 26..=27 {
        let raw = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","messageId":format!("U{turn}"),"content":{"type":"text","text":format!("U{turn:04}")}}}});
        assert!(store.writer().try_enqueue_frame(
            &native,
            Version::V1,
            turn,
            RawMessage::parse(raw.to_string().into_bytes()).unwrap()
        ));
        store.writer().flush().await.unwrap();
    }
    for (dir, cursor, from, to) in [
        ("older", &p["olderCursor"], 1, 5),
        ("newer", &p["newerCursor"], 16, 25),
    ] {
        let r = send_command_wait(
            &mut sock,
            "history.page",
            request(json!({"kind":"continue","direction":dir,"cursor":cursor})),
        )
        .await;
        let observed = &r["result"]["page"];
        assert_eq!(observed["generationId"], p["generationId"]);
        assert_eq!(observed["firstTurn"], from.to_string());
        assert_eq!(observed["lastTurn"], to.to_string());
        let frames = snapshot(&mut sock, &r).await;
        assert_eq!(frames.raw.len(), to - from + 1);
    }
    let latest =
        send_command_wait(&mut sock, "history.page", request(json!({"kind":"first"}))).await;
    assert_eq!(latest["result"]["page"]["firstTurn"], "18");
    assert_eq!(latest["result"]["page"]["lastTurn"], "27");
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_010_restart_keeps_cursors_and_protects_partial_read() {
    use serde_json::json;
    let f = HarnessFixture::new("hn010");
    let prelude =
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"prelude"}});
    configure(
        &f,
        &json!({"replayTurns":25,"preludeFrames":[prelude,prelude]}),
    );
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let first = send_command_wait(
        &mut sock,
        "view.open",
        json!({"targetId":root.target.encode(),"reason":"first_open","page":{"turns":"10"}}),
    )
    .await;
    let cursor = first["result"]["page"]["olderCursor"].clone();
    snapshot(&mut sock, &first).await;
    release_target(&mut sock, root.target).await;
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    drop(owner);
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let peer_count = f.peer_logs().len();
    let mut a = connected(&f, &node, &root.key, &root.id).await;
    let key = SigningKey::from_slice(&[92; 32]).unwrap();
    let id = pair_browser(&node, &key, "Reader").await;
    let mut b = connected(&f, &node, &key, &id).await;
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let older = |cursor: &serde_json::Value| json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":cursor}});
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let db = store.pool_for_test().await.unwrap();
    let timestamp = || {
        sqlx::query_scalar::<_, i64>(
            "SELECT last_updated_at_ms FROM transcript_sessions WHERE native_session_id=?",
        )
        .bind(&root.native)
        .fetch_one(&db)
    };
    let before = timestamp().await.unwrap();
    let middle = send_command_wait(&mut b, "history.page", older(&cursor)).await;
    assert_eq!(middle["result"]["page"]["firstTurn"], "6");
    let expected = snapshot(&mut b, &middle).await;
    let second = send_command_wait(&mut a, "history.page", older(&cursor)).await;
    assert!(
        snapshot(&mut a, &second).await.raw.eq(&expected.raw),
        "different page bodies across clients"
    );
    let oldest = send_command_wait(
        &mut b,
        "history.page",
        older(&middle["result"]["page"]["olderCursor"]),
    )
    .await;
    assert_eq!(oldest["result"]["page"]["prelude"], true);
    assert!(oldest["result"]["page"]["olderCursor"].is_null());
    assert_eq!(snapshot(&mut b, &oldest).await.raw.len(), 7);
    let bad = send_command_wait(
        &mut b,
        "history.page",
        json!({"target":scope,"mode":{"kind":"first"},"turns":"0"}),
    )
    .await;
    assert_eq!(bad["error"], json!({"code":"invalid_request"}));
    let newest = send_command_wait(
        &mut b,
        "history.page",
        json!({"target":scope,"mode":{"kind":"first"},"turns":"5"}),
    )
    .await;
    assert_eq!(newest["result"]["page"]["firstTurn"], "21");
    assert_eq!(timestamp().await.unwrap(), before);
    assert_eq!(f.peer_logs().len(), peer_count);
    assert_eq!(
        send_command_wait(&mut a, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        0
    );
    sqlx::query("UPDATE transcript_sessions SET last_updated_at_ms=0 WHERE native_session_id=?")
        .bind(&root.native)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        store
            .enforce_retention_and_budget(None)
            .await
            .unwrap()
            .expired_sessions,
        0
    );
    snapshot(&mut b, &newest).await;
    expire_cache(&node, &root.native).await;
    a.close(None).await.unwrap();
    b.close(None).await.unwrap();
    drop(db);
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_018_two_simultaneous_locations_and_failed_replay() {
    use serde_json::json;
    let f = HarnessFixture::new("hn018");
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut a = connected(&f, &node, &root.key, &root.id).await;
    let other = SigningKey::from_slice(&[93; 32]).unwrap();
    let id = pair_browser(&node, &other, "Other page reader").await;
    let mut b = connected(&f, &node, &other, &id).await;
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let locate = |n: u32| json!({"target":scope,"mode":{"kind":"locate","direction":"older","fromTurn":n.to_string()}});
    let key = NativeSessionKey::from_config(Some(root.selection.source), &root.native).unwrap();
    configure(
        &f,
        &json!({"failLoadMessage":"controlled failure","replayTurns":600}),
    );
    let failure = send_command_wait(&mut a, "history.page", locate(500)).await;
    assert!(!failure["ok"].as_bool().unwrap());
    assert_eq!(failure["error"].as_object().unwrap().len(), 1);
    assert!(failure["result"].is_null());
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    assert!(store.load_snapshot(&key).await.unwrap().is_none());
    let gate = f.area.directory.join("load-release");
    let entered = f.area.directory.join("load-entered");
    configure(
        &f,
        &json!({"failLoadMessage":null,"loadBarrierReleaseFile":gate,"loadBarrierEnteredFile":entered}),
    );
    let unblock = async {
        timeout(Duration::from_secs(10), async {
            while !entered.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        std::fs::write(gate, b"release").unwrap();
    };
    let (first, second, ()) = tokio::join!(
        send_command_wait(&mut a, "history.page", locate(500)),
        send_command_wait(&mut b, "history.page", locate(300)),
        unblock
    );
    for (reply, expected, socket) in [(&first, 500, &mut a), (&second, 300, &mut b)] {
        let page = &reply["result"]["page"];
        assert_eq!(
            page["lastTurn"],
            expected.to_string(),
            "concurrent error {:?}",
            reply["error"]
        );
        let frames = snapshot(socket, reply).await;
        assert_eq!(frames.raw.len(), 10);
        for (i, frame) in frames.raw.iter().enumerate() {
            assert!(
                frame["params"]["update"]["content"]["text"]
                    .eq(&format!("U{:04}", expected - 9 + i))
            );
        }
    }
    assert_eq!(
        first["result"]["page"]["generationId"],
        second["result"]["page"]["generationId"]
    );
    a.close(None).await.unwrap();
    b.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_020_cached_children_read_independently_while_root_owns_prompt() {
    use serde_json::json;
    let f = HarnessFixture::new("hn020");
    let gate = f.area.directory.join("prompt-release");
    configure(&f, &json!({"promptBarrierReleaseFile":gate}));
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let (id, key, sel) = f.install_profile(&node, 1, 94).await;
    let mut a = connected(&f, &node, &key, &id).await;
    let (root, native) = f
        .create_and_bind_target(&node, &mut a, &key, &id, &sel)
        .await;
    let (unrelated, other_native) = f
        .create_and_bind_target(&node, &mut a, &key, &id, &sel)
        .await;
    release_target(&mut a, unrelated).await;
    record_child(&node, &sel, &native, "child").await;
    let pool = node.storage.as_ref().unwrap().pool_for_test();
    sqlx::query("INSERT INTO node_session_parents SELECT ?, 'grandchild', 'child',source_instance_id,runtime_epoch,connection_epoch,X'0000000000000001',X'0000000000000001',zeroblob(32),0 FROM node_source_session_operations WHERE native_session_id=?").bind(sel.source.encode()).bind(&native).execute(pool).await.unwrap();
    seed_turns(&node, &sel, &native, 25, "R", "user_message_chunk").await;
    seed_turns(&node, &sel, "child", 10, "C", "user_message_chunk").await;
    seed_turns(&node, &sel, "grandchild", 5, "G", "user_message_chunk").await;
    let read_key = SigningKey::from_slice(&[95; 32]).unwrap();
    let reader = pair_browser(&node, &read_key, "Child observer").await;
    let mut b = connected(&f, &node, &read_key, &reader).await;
    let scope = json!({"kind":"target","targetId":root.encode()});
    let parent = json!({"target":scope,"turns":"5","mode":{"kind":"first"}});
    let first = send_command_wait(&mut b, "history.page", parent.clone()).await;
    assert_eq!(first["result"]["page"]["firstTurn"], "21");
    snapshot(&mut b, &first).await;
    let child =
        |name: &str| json!({"target":scope,"child":name,"turns":"3","mode":{"kind":"first"}});
    for name in ["missing", &other_native] {
        let bad = send_command_wait(&mut b, "history.page", child(name)).await;
        assert_eq!(bad["error"], json!({"code":"invalid_target"}));
        assert!(bad["result"].is_null());
    }
    let grand = send_command_wait(&mut b, "history.page", child("grandchild")).await;
    assert_eq!(grand["result"]["page"]["firstTurn"], "1");
    let frames = snapshot(&mut b, &grand).await;
    assert_eq!(frames.raw.len(), 3);
    assert!(
        frames
            .raw
            .iter()
            .all(|r| r["params"]["sessionId"] == "grandchild")
    );
    let bound = json!({"targetId":root.encode()});
    let ctrl = send_command_wait(&mut a, "session.control.inspect", bound.clone()).await;
    assert_eq!(
        submit_prompt_turn(&mut a, root, "keep running").await["ok"],
        true
    );
    wait_for_peer_method(&f, "session/prompt", 1).await;
    let queue = send_command_wait(&mut a, "input.queue", bound.clone()).await;
    let count = f.peer_logs().len();
    for _ in 0..20 {
        let page = send_command_wait(&mut b, "history.page", child("child")).await;
        let meta = &page["result"]["page"];
        assert_eq!(meta["firstTurn"], "1");
        assert_eq!(meta["lastTurn"], "3");
        assert_eq!(meta["relocated"], false);
        assert_eq!(snapshot(&mut b, &page).await.raw.len(), 3);
    }
    assert_eq!(f.peer_logs().len(), count);
    assert_eq!(
        send_command_wait(&mut a, "input.queue", bound.clone()).await["result"],
        queue["result"]
    );
    assert_eq!(
        send_command_wait(&mut a, "session.control.inspect", bound).await["result"],
        ctrl["result"]
    );
    assert_eq!(
        send_command_wait(&mut a, "session.capacity.inspect", json!({})).await["result"]["occupied"],
        1
    );
    seed_turns(&node, &sel, "child", 12, "C", "user_message_chunk").await;
    let req = json!({"target":scope,"child":"child","mode":{"kind":"first"}});
    let reopened = send_command_wait(&mut b, "history.page", req).await;
    assert_eq!(reopened["result"]["page"]["lastTurn"], "10");
    let cursor = reopened["result"]["page"]["newerCursor"].clone();
    snapshot(&mut b, &reopened).await;
    let tail=send_command_wait(&mut b,"history.page",json!({"target":scope,"child":"child","mode":{"kind":"continue","cursor":cursor,"direction":"newer"}})).await;
    assert_eq!(tail["result"]["page"]["firstTurn"], "11");
    assert_eq!(tail["result"]["page"]["lastTurn"], "12");
    crate::ipc::device_revoke(node.ipc_address(), &reader)
        .await
        .unwrap();
    assert_eq!(next_close(&mut b, Duration::from_secs(5)).await.0, 4410);
    let stable = send_command_wait(&mut a, "history.page", parent).await;
    assert_eq!(
        stable["result"]["page"]["turns"],
        first["result"]["page"]["turns"]
    );
    std::fs::write(gate, b"done").unwrap();
    a.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_014_append_over_the_original_socket_view() {
    use serde_json::json;
    let f = HarnessFixture::new("hn014-tail");
    configure(
        &f,
        &json!({"capabilityMode":"replay_and_resume","echoUserChunk":true,"replayTurns":25}),
    );
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut a = connected(&f, &node, &root.key, &root.id).await;
    let reader_key = SigningKey::from_slice(&[96; 32]).unwrap();
    let reader_id = pair_browser(&node, &reader_key, "Live tail reader").await;
    let mut reader = connected(&f, &node, &reader_key, &reader_id).await;
    let open = json!({"targetId":root.target.encode(),"reason":"first_open","page":{"turns":"10"}});
    let initial = send_command_wait(&mut reader, "view.open", open.clone()).await;
    assert_eq!(initial["result"]["page"]["firstTurn"], "16");
    let stream = Id128::parse_base64url(initial["result"]["streamId"].as_str().unwrap()).unwrap();
    let initial_frames = read_until_snapshot_end(&mut reader, stream).await;
    assert!(initial_frames.turns.is_empty());
    let initial_turns: Vec<_> = initial["result"]["page"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["turn"].as_str().unwrap())
        .collect();
    assert_eq!(
        initial_turns,
        ["16", "17", "18", "19", "20", "21", "22", "23", "24", "25"]
    );
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let mid=send_command_wait(&mut a,"history.page",json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":initial["result"]["page"]["olderCursor"]}})).await;
    let p = &mid["result"]["page"];
    snapshot(&mut a, &mid).await;
    takeover_target(&mut a, root.target).await;
    for i in 26..=27 {
        let reply = submit_prompt_turn(&mut a, root.target, &format!("U{i:04}")).await;
        assert_eq!(reply["ok"], true);
        wait_for_peer_method(&f, "session/prompt", i - 25).await;
    }
    let receipt = timeout(Duration::from_secs(8), async {
        let mut starts = Vec::new();
        let mut content = Vec::new();
        loop {
            let msg = reader.next().await.unwrap().unwrap();
            let Message::Binary(bytes) = msg else {
                continue;
            };
            let (h, payload) = TransferHeader::decode(&bytes).unwrap();
            if h.stream() != stream {
                continue;
            }
            reader
                .send(Message::Binary(
                    TransferHeader::credit(stream, h.packet())
                        .encode(&[])
                        .unwrap()
                        .into(),
                ))
                .await
                .unwrap();
            if h.record_kind() == Some(RecordKind::TurnStart) {
                let val: serde_json::Value = serde_json::from_slice(payload).unwrap();
                starts.push(val["turn"].as_str().unwrap().to_owned());
            } else if h.record_kind() == Some(RecordKind::OriginalRaw) {
                let val: serde_json::Value = serde_json::from_slice(payload).unwrap();
                content.push((
                    starts.len(),
                    val["params"]["update"]["content"]["text"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ));
                if content.len() == 4 {
                    break (starts, content);
                }
            }
        }
    })
    .await;
    let latest = send_command_wait(&mut a, "view.open", open).await;
    let older=send_command_wait(&mut a,"history.page",json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":p["olderCursor"]}})).await;
    let newer=send_command_wait(&mut a,"history.page",json!({"target":scope,"mode":{"kind":"continue","direction":"newer","cursor":p["newerCursor"]}})).await;
    a.close(None).await.unwrap();
    reader.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
    assert_eq!(latest["result"]["page"]["lastTurn"], "27");
    assert_eq!(older["result"]["page"]["firstTurn"], "1");
    assert_eq!(newer["result"]["page"]["lastTurn"], "25");
    let (starts, content) = receipt.expect("live view receives both complete appended turns");
    assert_eq!(starts, ["26", "27"]);
    let correct = content.iter().map(|(n, s)| (*n, s.as_str())).eq([
        (1, "U0026"),
        (1, "reply:U0026"),
        (2, "U0027"),
        (2, "reply:U0027"),
    ]);
    assert!(correct, "turn marker order and message ownership");
}

#[tokio::test]
async fn hn_017_replay_relocates_against_compressed_and_rewritten_native_history() {
    use serde_json::json;
    let f = HarnessFixture::new("hn017-replay");
    configure(&f, &json!({"replayTurns":1000}));
    let root = Box::pin(cold_root(&f)).await;
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    let mut sock = connected(&f, &node, &root.key, &root.id).await;
    let scope = json!({"kind":"target","targetId":root.target.encode()});
    let locate = |n: u64| json!({"target":scope,"mode":{"kind":"locate","direction":"older","fromTurn":n.to_string()}});
    let previous = send_command_wait(&mut sock, "history.page", locate(820)).await;
    assert_eq!(previous["result"]["page"]["firstTurn"], "811");
    snapshot(&mut sock, &previous).await;
    for marker in ["C", "W"] {
        release_target(&mut sock, root.target).await;
        expire_cache(&node, &root.native).await;
        configure(&f, &json!({"replayTurns":600,"turnPrefix":marker}));
        let stale=send_command_wait(&mut sock,"history.page",json!({"target":scope,"mode":{"kind":"continue","direction":"older","cursor":previous["result"]["page"]["olderCursor"]}})).await;
        assert_eq!(stale["error"], json!({"code":"stale_cursor"}));
        let missing = send_command_wait(&mut sock, "history.page", locate(810)).await;
        assert_eq!(missing["error"], json!({"code":"not_found"}));
        assert!(missing["result"].is_null());
        let next = send_command_wait(&mut sock, "history.page", locate(500)).await;
        assert_eq!(next["result"]["page"]["totalTurns"], "600");
        assert_eq!(next["result"]["page"]["firstTurn"], "491");
        assert_eq!(next["result"]["page"]["continuity"], "unproven");
        let frames = snapshot(&mut sock, &next).await;
        assert_eq!(frames.raw.len(), 10);
        for (i, frame) in frames.raw.iter().enumerate() {
            assert!(
                frame["params"]["update"]["content"]["text"].eq(&format!("{marker}{:04}", 491 + i)),
                "unexpected native content at offset {i}"
            );
        }
    }
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn hn_008_future_schema_preserves_interactive_operations() {
    Box::pin(check_schema_degradation(false)).await;
}

#[tokio::test]
async fn hn_008_failed_migration_preserves_interactive_operations() {
    Box::pin(check_schema_degradation(true)).await;
}

async fn check_schema_degradation(migration_fault: bool) {
    use sha2::{Digest, Sha256};
    let f = HarnessFixture::new("hn008");
    let owner = start_node(&f);
    let node = owner.ready().await.unwrap();
    node.shutdown(None).await.unwrap();
    drop(owner);
    let path = f.area.data_dir().join("transcript-cache.sqlite3");
    let db = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    if migration_fault {
        sqlx::raw_sql(
            "DROP TABLE transcript_turns;
            ALTER TABLE transcript_sessions DROP COLUMN generation_token;
            ALTER TABLE transcript_sessions DROP COLUMN turn_state;
            ALTER TABLE transcript_sessions DROP COLUMN turn_count;
            UPDATE transcript_cache_meta SET schema_version=1;
            CREATE TABLE transcript_turns(migration_conflict INTEGER);",
        )
        .execute(&db)
        .await
        .unwrap();
    } else {
        sqlx::query("UPDATE transcript_cache_meta SET schema_version=3")
            .execute(&db)
            .await
            .unwrap();
    }
    db.close().await;
    let before = Sha256::digest(std::fs::read(&path).unwrap());
    Box::pin(check_degraded_prompt_and_approval(&f)).await;
    assert_eq!(Sha256::digest(std::fs::read(path).unwrap()), before);
}
