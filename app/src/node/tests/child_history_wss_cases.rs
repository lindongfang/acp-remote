use super::*;
use serde_json::json;

async fn bind_child(node: &NodeRuntime, selection: &SourceLaunchSelection, native: &str) {
    let pool = node.storage.as_ref().unwrap().pool_for_test();
    let inserted = sqlx::query(
        "INSERT INTO node_session_parents
         SELECT ?, 'child-history', ?, source_instance_id, runtime_epoch, connection_epoch,
                X'0000000000000001', X'0000000000000001', zeroblob(32), 0
         FROM node_source_session_operations op
         JOIN node_source_session_registrations reg ON reg.operation_id=op.operation_id
         WHERE op.native_session_id=? AND reg.source_id=?",
    )
    .bind(selection.source.encode())
    .bind(native)
    .bind(native)
    .bind(selection.source.encode())
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(inserted.rows_affected(), 1);
}

#[tokio::test]
async fn independent_child_history_over_wss() {
    for (version, wrong_child) in [(1, false), (2, false), (1, true)] {
        let fixture = HarnessFixture::new("independent-child");
        let mut cfg: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&fixture.cfg_path).unwrap()).unwrap();
        cfg["protocolVersion"] = json!(version);
        cfg["childHistory"] = json!(true);
        cfg["wrongChild"] = json!(wrong_child);
        let release = fixture.area.directory.join("child-release");
        if !wrong_child {
            cfg["childBarrierIndex"] = json!(i32::from(version != 1));
            cfg["childBarrierFile"] = json!(release);
        }
        std::fs::write(&fixture.cfg_path, cfg.to_string()).unwrap();
        let owner = NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            fixture.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        );
        let node = owner.ready().await.unwrap();
        let (id, key, selection) = fixture.install_profile(&node, version, 74).await;
        let mut sock = authenticate_socket(
            valid_management_socket(&fixture.area, node.listen_addr()).await,
            &key,
            &id,
            node_public_key(&node),
        )
        .await;
        let (root_target, native) = fixture
            .create_and_bind_target(&node, &mut sock, &key, &id, &selection)
            .await;
        bind_child(&node, &selection, &native).await;
        let mut payload = json!({"target":{"kind":"source","selection":selection_json(&selection),"nativeSessionId":native},"child":"child-history","turns":"10","mode":{"kind":"first"}});
        let first = if wrong_child {
            send_command_wait(&mut sock, "history.page", payload.clone()).await
        } else {
            let reader_key = SigningKey::from_slice(&[75; 32]).unwrap();
            let reader_id = pair_browser(&node, &reader_key, "second reader").await;
            let mut second = authenticate_socket(
                valid_management_socket(&fixture.area, node.listen_addr()).await,
                &reader_key,
                &reader_id,
                node_public_key(&node),
            )
            .await;
            let request_id = Id128::new([85; 16]).to_base64url();
            send_json(&mut sock,&json!({"v":1,"type":"command","requestId":request_id,"action":"history.page","payload":payload})).await;
            let child =
                NativeSessionKey::from_config(Some(selection.source), "child-history").unwrap();
            let native = &node.session_services.as_ref().unwrap().native;
            let wait_for = |n| {
                let child = child.clone();
                async move {
                    timeout(Duration::from_secs(10), async {
                        loop {
                            if native
                                .all_active_protocol_sources()
                                .iter()
                                .filter_map(|s| s.views().ok())
                                .any(|v| v.child_observers(&child) == n)
                            {
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .expect("shared query reader count");
                }
            };
            wait_for(1).await;
            let control = async {
                wait_for(2).await;
                assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
                let mut control_sock = authenticate_socket(
                    valid_management_socket(&fixture.area, node.listen_addr()).await,
                    &key,
                    &id,
                    node_public_key(&node),
                )
                .await;
                let blocked =
                    submit_prompt_turn(&mut control_sock, root_target, "while reading").await;
                assert_eq!(blocked["result"]["state"], "refused", "{blocked}");
                assert_eq!(blocked["result"]["reason"], "Conflict", "{blocked}");
                assert!(
                    !fixture
                        .peer_logs()
                        .iter()
                        .any(|r| r["method"] == "session/prompt")
                );
                crate::ipc::device_revoke(node.ipc_address(), &id)
                    .await
                    .unwrap();
                assert_eq!(next_close(&mut sock, Duration::from_secs(5)).await.0, 4410);
                std::fs::write(&release, b"continue").unwrap();
            };
            let (first, ()) = tokio::join!(
                send_command_wait(&mut second, "history.page", payload.clone()),
                control
            );
            sock = second;
            first
        };
        if wrong_child {
            assert_eq!(first["ok"], false);
            let child =
                NativeSessionKey::from_config(Some(selection.source), "child-history").unwrap();
            assert!(
                node.session_services
                    .as_ref()
                    .unwrap()
                    .transcript_cache
                    .store()
                    .load_snapshot(&child)
                    .await
                    .unwrap()
                    .is_none()
            );
        } else {
            assert_eq!(
                first["result"]["page"]["source"],
                "replay",
                "protocol={version} ok={} error={} emitted_batches={:?}",
                first["ok"],
                first["error"]["code"],
                fixture
                    .peer_logs()
                    .iter()
                    .filter(|e| e["event"] == "childBatchSent")
                    .filter_map(|e| e["batch"].as_u64())
                    .collect::<Vec<_>>()
            );
            assert_eq!(first["result"]["page"]["lastTurn"], "10");
            let child =
                NativeSessionKey::from_config(Some(selection.source), "child-history").unwrap();
            let stored = node
                .session_services
                .as_ref()
                .unwrap()
                .transcript_cache
                .store()
                .load_snapshot(&child)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stored.records.len(), 13);
            assert!(stored.records[0].raw.bytes().len() > 16 * 1024);
            assert!(
                std::str::from_utf8(stored.records[0].raw.bytes())
                    .unwrap()
                    .contains(r#""unknown" : 1.00"#)
            );
            payload["mode"] = json!({"kind":"continue","direction":"newer","cursor":first["result"]["page"]["newerCursor"]});
            let next = send_command_wait(&mut sock, "history.page", payload).await;
            assert_eq!(next["result"]["page"]["lastTurn"], "12", "{next}");
            takeover_target(&mut sock, root_target).await;
            let next_work = submit_prompt_turn(&mut sock, root_target, "after query").await;
            assert_eq!(next_work["ok"], true, "{next_work}");
            assert_ne!(next_work["result"]["state"], "refused", "{next_work}");
            wait_for_peer_method(&fixture, "session/prompt", 1).await;
        }
        let calls = fixture
            .peer_logs()
            .into_iter()
            .filter(|row| row["method"] == "_io.github.lindongfang.acp-remote/child-history")
            .count();
        // The first frame spans both chunks, including when it belongs to the sibling.
        assert_eq!(calls, 2);
        sock.close(None).await.unwrap();
        node.shutdown(None).await.unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Interrupt {
    Shutdown,
    Eof,
    RemoteError,
    LastReader,
    LastReaderClosed,
    Busy,
    Relation,
    NewSnapshot,
    Absent,
    Refused,
}

#[tokio::test]
async fn independent_child_history_aborts_before_publication_over_wss() {
    for case in [
        Interrupt::Shutdown,
        Interrupt::Eof,
        Interrupt::RemoteError,
        Interrupt::LastReader,
        Interrupt::LastReaderClosed,
        Interrupt::Busy,
        Interrupt::Relation,
        Interrupt::NewSnapshot,
        Interrupt::Absent,
        Interrupt::Refused,
    ] {
        Box::pin(interrupted(case)).await;
    }
}
async fn interrupted(case: Interrupt) {
    let fixture = HarnessFixture::new("interrupted-child");
    let gate = fixture.area.directory.join("child-release");
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.cfg_path).unwrap()).unwrap();
    cfg["childHistory"] = json!(!matches!(case, Interrupt::Absent));
    cfg["childEof"] = json!(matches!(case, Interrupt::Eof));
    cfg["childError"] = json!(matches!(case, Interrupt::RemoteError));
    if matches!(case, Interrupt::Busy) {
        cfg["promptBarrierFile"] = json!(gate);
    }
    cfg["childRefused"] = json!(matches!(case, Interrupt::Refused));
    if !matches!(case, Interrupt::Absent | Interrupt::Refused) {
        cfg["childBarrierFile"] = json!(gate);
        cfg["childBarrierIndex"] = json!(0);
    }
    std::fs::write(&fixture.cfg_path, cfg.to_string()).unwrap();
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    let (id, key, selection) = fixture.install_profile(&node, 1, 76).await;
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &id,
        node_public_key(&node),
    )
    .await;
    let (root, native) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &id, &selection)
        .await;
    bind_child(&node, &selection, &native).await;
    let child = NativeSessionKey::from_config(Some(selection.source), "child-history").unwrap();
    let store = node
        .session_services
        .as_ref()
        .unwrap()
        .transcript_cache
        .store();
    let payload = json!({"target":{"kind":"target","targetId":root.encode()},"child":"child-history","turns":"10","mode":{"kind":"first"}});
    let method = "_io.github.lindongfang.acp-remote/child-history";
    match case {
        Interrupt::Shutdown => {
            send_json(&mut sock,&json!({"v":1,"type":"command","requestId":Id128::new([87;16]).to_base64url(),"action":"history.page","payload":payload})).await;
            wait_for_peer_method(&fixture, method, 1).await;
            node.shutdown(None).await.unwrap();
            drop(owner);
            drop(sock);
            std::fs::remove_dir_all(&fixture.area.directory)
                .expect("closed node releases its isolated temporary directory");
            return;
        }
        Interrupt::RemoteError | Interrupt::Eof => {
            let reply = send_command_wait(&mut sock, "history.page", payload).await;
            assert_eq!(reply["ok"], false, "{case:?} {reply}");
        }
        Interrupt::Absent | Interrupt::Refused => {
            for _ in 0..2 {
                let reply = send_command_wait(&mut sock, "history.page", payload.clone()).await;
                assert_eq!(
                    reply["error"]["code"], "unsupported_capability",
                    "{case:?} {reply}"
                );
            }
        }
        Interrupt::Busy => {
            let mut reader = authenticate_socket(
                valid_management_socket(&fixture.area, node.listen_addr()).await,
                &key,
                &id,
                node_public_key(&node),
            )
            .await;
            let (_, ()) = tokio::join!(submit_prompt_turn(&mut sock, root, "active root"), async {
                wait_for_peer_method(&fixture, "session/prompt", 1).await;
                let reply = send_command_wait(&mut reader, "history.page", payload.clone()).await;
                assert_eq!(reply["error"]["code"], "invalid_stage", "{reply}");
                assert!(!fixture.peer_logs().iter().any(|r| r["method"] == method));
                std::fs::write(&gate, b"release root prompt").unwrap();
            });
            timeout(Duration::from_secs(10), async {
                loop {
                    let reply =
                        send_command_wait(&mut reader, "history.page", payload.clone()).await;
                    if reply["error"]["code"] != "invalid_stage" {
                        assert_eq!(reply["result"]["page"]["lastTurn"], "10", "{reply}");
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("child query after root work finished");
        }
        Interrupt::LastReader | Interrupt::LastReaderClosed => {
            send_json(&mut sock,&json!({"v":1,"type":"command","requestId":Id128::new([86;16]).to_base64url(),"action":"history.page","payload":payload})).await;
            wait_for_peer_method(&fixture, method, 1).await;
            if matches!(case, Interrupt::LastReader) {
                crate::ipc::device_revoke(node.ipc_address(), &id)
                    .await
                    .unwrap();
                assert_eq!(next_close(&mut sock, Duration::from_secs(5)).await.0, 4410);
            } else {
                sock.close(None).await.unwrap();
                drop(sock);
            }
            timeout(Duration::from_secs(8), async {
                while node
                    .session_services
                    .as_ref()
                    .unwrap()
                    .native
                    .all_active_protocol_sources()
                    .iter()
                    .filter_map(|source| source.views().ok())
                    .any(|views| views.child_observers(&child) != 0)
                {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
            })
            .await
            .expect("the last reader left before the native response");
            std::fs::write(&gate, b"late reply").unwrap();
        }
        Interrupt::NewSnapshot | Interrupt::Relation => {
            let change = async {
                wait_for_peer_method(&fixture, method, 1).await;
                if matches!(case, Interrupt::Relation) {
                    sqlx::query(
                        "DELETE FROM node_session_parents WHERE source_id=? AND child_native_id=?",
                    )
                    .bind(selection.source.encode())
                    .bind(child.native_id())
                    .execute(node.storage.as_ref().unwrap().pool_for_test())
                    .await
                    .unwrap();
                } else {
                    let frame = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":child.native_id(),"update":{"sessionUpdate":"user_message_chunk","messageId":"newer","content":{"type":"text","text":"newer"}}}});
                    store
                        .replace_with_replay(
                            &child,
                            crate::adapters::acp::Version::V1,
                            crate::transcript_cache::CapabilityProfile::ReplayCapable,
                            vec![(
                                crate::adapters::acp::RawMessage::parse(
                                    frame.to_string().into_bytes(),
                                )
                                .unwrap(),
                                1,
                            )],
                        )
                        .await
                        .unwrap();
                }
                std::fs::write(&gate, b"late reply").unwrap();
            };
            let (reply, ()) = tokio::join!(
                send_command_wait(&mut sock, "history.page", payload),
                change
            );
            assert_eq!(reply["ok"], false, "{case:?}");
        }
    }
    let cached = store.load_snapshot(&child).await.unwrap();
    if matches!(case, Interrupt::Busy) {
        assert_eq!(cached.unwrap().records.len(), 13);
    } else if matches!(case, Interrupt::NewSnapshot) {
        assert_eq!(cached.unwrap().records.len(), 1);
    } else {
        assert!(cached.is_none(), "{case:?}");
    }
    let count = fixture
        .peer_logs()
        .iter()
        .filter(|r| r["method"] == method)
        .count();
    assert_eq!(
        count,
        match case {
            Interrupt::Absent => 0,
            Interrupt::Busy | Interrupt::NewSnapshot => 2,
            _ => 1,
        },
        "{case:?}"
    );
    node.shutdown(None).await.unwrap();
}

#[tokio::test]
async fn child_history_thousand_anonymous_turns_relocate_after_eviction() {
    let fixture = HarnessFixture::new("thousand-child");
    let mut cfg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&fixture.cfg_path).unwrap()).unwrap();
    cfg["childHistory"] = json!(true);
    cfg["childTurns"] = json!(1000);
    cfg["randomChildIds"] = json!(true);
    std::fs::write(&fixture.cfg_path, cfg.to_string()).unwrap();
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        fixture.area.config(free_address()),
        Arc::new(FakeProtector::default()),
    );
    let node = owner.ready().await.unwrap();
    let (id, key, selection) = fixture.install_profile(&node, 1, 77).await;
    let mut sock = authenticate_socket(
        valid_management_socket(&fixture.area, node.listen_addr()).await,
        &key,
        &id,
        node_public_key(&node),
    )
    .await;
    let (root, native) = fixture
        .create_and_bind_target(&node, &mut sock, &key, &id, &selection)
        .await;
    bind_child(&node, &selection, &native).await;
    let mut payload = json!({"target":{"kind":"target","targetId":root.encode()},"child":"child-history","turns":"10","mode":{"kind":"first"}});
    for first_turn in (1..=1000).step_by(10) {
        if first_turn == 501 {
            // Resume paging as the same authenticated reader after the first 50 streams.
            sock.close(None).await.unwrap();
            sock = authenticate_socket(
                valid_management_socket(&fixture.area, node.listen_addr()).await,
                &key,
                &id,
                node_public_key(&node),
            )
            .await;
            let pool = node
                .session_services
                .as_ref()
                .unwrap()
                .transcript_cache
                .store()
                .pool_for_test()
                .await
                .unwrap();
            let count = sqlx::query(
                "DELETE FROM transcript_sessions WHERE native_session_id='child-history'",
            )
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected();
            assert_eq!(count, 1);
            let stale = send_command_wait(&mut sock, "history.page", payload.clone()).await;
            assert_eq!(stale["error"]["code"], "stale_cursor", "{stale}");
            payload["mode"] = json!({"kind":"locate","direction":"newer","fromTurn":"501"});
        }
        let response = send_command_wait(&mut sock, "history.page", payload.clone()).await;
        let page = &response["result"]["page"];
        assert_eq!(
            page["firstTurn"],
            json!(first_turn.to_string()),
            "{response}"
        );
        assert_eq!(page["lastTurn"], json!((first_turn + 9).to_string()));
        assert_eq!(page["relocated"], json!(first_turn == 501));
        assert_eq!(
            page["continuity"],
            if first_turn == 501 {
                "unproven"
            } else {
                "same_generation"
            }
        );
        let marks = page["turns"].as_array().unwrap();
        assert_eq!(marks.len(), 10);
        let stream =
            Id128::parse_base64url(response["result"]["streamId"].as_str().unwrap()).unwrap();
        let frames = read_until_snapshot_end(&mut sock, stream).await;
        assert_eq!(frames.snapshot_ends, 1);
        assert_eq!(frames.raw.len(), 20);
        for (i, pair) in frames.raw.as_chunks::<2>().0.iter().enumerate() {
            let n = first_turn + i;
            assert_eq!(marks[i]["turn"], n.to_string());
            assert!(
                pair.iter()
                    .all(|r| r["params"]["sessionId"] == "child-history")
            );
            let user = &pair[0]["params"]["update"];
            let agent = &pair[1]["params"]["update"];
            assert!(user.get("messageId").is_none());
            assert_eq!(user["sessionUpdate"], "user_message_chunk");
            assert!(
                user["content"]["text"].eq(&format!("child-turn-{n}")),
                "unexpected child turn {n}"
            );
            assert_eq!(agent["sessionUpdate"], "agent_message_chunk");
            assert_eq!(agent["messageId"].as_str().unwrap().len(), 36);
            assert_eq!(agent["content"]["text"], "reply");
        }
        if first_turn == 991 {
            assert!(page["newerCursor"].is_null());
        }
        payload["mode"] =
            json!({"kind":"continue","direction":"newer","cursor":page["newerCursor"]});
        let closed = send_command_wait(
            &mut sock,
            "view.close",
            json!({"streamId":response["result"]["streamId"]}),
        )
        .await;
        assert_eq!(closed["ok"], true, "{closed}");
    }
    let logs = fixture.peer_logs();
    assert_eq!(logs.iter().filter(|r| r["first"] == true).count(), 2);
    assert!(!logs.iter().any(|r| r["method"] == "session/load"));
    sock.close(None).await.unwrap();
    node.shutdown(None).await.unwrap();
}

#[path = "child_history_wss_cases/independent.rs"]
mod independent;
