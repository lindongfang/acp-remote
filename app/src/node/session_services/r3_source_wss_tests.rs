use super::*;
use crate::application::shared_views::wire::{RecordKind, TransferHeader};
use crate::domain::native_catalog::{CatalogDraft, CatalogResult, SourceSelection};
use crate::domain::source_sessions::SourceLaunchSelection;
use futures_util::FutureExt;
use std::{future::Future, panic::AssertUnwindSafe, pin::Pin};

const fn r3_id(n: u8) -> crate::domain::EntityId {
    crate::domain::EntityId::from_bytes([n; 16])
}

fn r3_deadline() -> crate::application::request_registry::RegistryDeadline {
    crate::application::request_registry::RegistryDeadline::new(
        tokio::time::Instant::now() + Duration::from_secs(10),
    )
}

fn r3_utf16(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

struct R3WssPeer {
    client_id: String,
    key: SigningKey,
    socket: ManagementSocket,
}

struct R3WssCase {
    area: TestArea,
    owner: Option<NodeProcessOwner>,
    node: Option<NodeRuntime>,
    peers: Vec<R3WssPeer>,
    selection: Option<SourceLaunchSelection>,
}

impl R3WssCase {
    async fn start(&mut self, scenario: &str, seed_target: bool) {
        self.owner = Some(NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            self.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        ));
        self.node = Some(self.owner.as_ref().unwrap().ready().await.unwrap());
        let node = self.node.as_ref().unwrap();
        let pool = node.storage.as_ref().unwrap().pool_for_test().clone();
        let launch = self
            .area
            .directory
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        sqlx::query("INSERT INTO product_sources VALUES(?,'r3 wss source-session fixture')")
            .bind(r3_id(1).encode())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces VALUES(?,?)")
            .bind(r3_id(9).encode())
            .bind(&launch)
            .execute(&pool)
            .await
            .unwrap();
        if seed_target {
            sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
                .bind(r3_id(2).encode())
                .bind(r3_id(1).encode())
                .bind("source-native-existing")
                .bind(r3_id(9).encode())
                .execute(&pool)
                .await
                .unwrap();
        }

        let key_a = SigningKey::from_slice(&[71; 32]).unwrap();
        let id_a = pair_browser(node, &key_a, "R3 actor A").await;
        let sock_a = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            &key_a,
            &id_a,
            node_public_key(node),
        )
        .await;
        self.peers.push(R3WssPeer {
            client_id: id_a.clone(),
            key: key_a,
            socket: sock_a,
        });

        let key_b = SigningKey::from_slice(&[72; 32]).unwrap();
        let id_b = pair_browser(node, &key_b, "R3 actor B").await;
        let sock_b = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            &key_b,
            &id_b,
            node_public_key(node),
        )
        .await;
        self.peers.push(R3WssPeer {
            client_id: id_b,
            key: key_b,
            socket: sock_b,
        });

        let observed = node
            .management
            .as_ref()
            .unwrap()
            .observe_next_document_socket_for_test()
            .unwrap();
        let sock_a2 = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            &self.peers[0].key,
            &id_a,
            node_public_key(node),
        )
        .await;
        let endpoint_a2 = observed.await.unwrap();
        let client_a2 = endpoint_a2.client();
        self.peers.push(R3WssPeer {
            client_id: id_a,
            key: self.peers[0].key.clone(),
            socket: sock_a2,
        });

        let child = crate::test_support::native_dispatch_dev_child(&[
            "SOURCE_SESSION_CHILD",
            "NATIVE_DISPATCH_DEV_CHILD",
        ]);
        let child = child.to_string_lossy();
        assert!(std::path::Path::new(child.as_ref()).is_file());
        let wire = if matches!(
            scenario,
            "source-create-replay-state" | "external-reads-quarantine"
        ) {
            2
        } else {
            1
        };
        let raw = format!(
            r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":{wire},"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
            r3_utf16(&child),
            r3_utf16(scenario)
        );
        let profile =
            crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes())
                .unwrap();
        let draft = CatalogDraft::install_harness(
            "R3 WSS child".into(),
            SourceSelection::Existing(r3_id(1)),
            profile,
        )
        .unwrap();
        let catalog = &node.session_services.as_ref().unwrap().catalog;
        let actor = client_a2.context.actor();
        let registered = catalog
            .register(actor, draft, &r3_deadline())
            .await
            .unwrap();
        let result = catalog
            .apply(actor, registered, &r3_deadline())
            .await
            .unwrap();
        let (harness, profile) = match result.fact().result().unwrap() {
            CatalogResult::Harness {
                harness, profile, ..
            } => (*harness, *profile),
            _ => panic!("expected installed profile"),
        };
        let selection = SourceLaunchSelection {
            source: r3_id(1),
            harness,
            profile,
            workspace: r3_id(9),
            launch_path: launch.clone(),
        };
        if seed_target {
            let observation = catalog
                .observe_workspace(actor, r3_id(2), r3_id(1), r3_id(9), &r3_deadline())
                .await
                .unwrap();
            let reg = catalog
                .register(
                    actor,
                    observation.bind(harness, profile, launch).unwrap(),
                    &r3_deadline(),
                )
                .await
                .unwrap();
            catalog.apply(actor, reg, &r3_deadline()).await.unwrap();
        }
        self.selection = Some(selection);
    }

    fn selection_json(&self) -> serde_json::Value {
        let s = self.selection.as_ref().unwrap();
        serde_json::json!({
            "sourceId": s.source.encode(),
            "harnessId": s.harness.encode(),
            "profileId": s.profile.id().encode(),
            "profileRevision": s.profile.revision().to_string(),
            "workspaceId": s.workspace.encode(),
            "launchPath": s.launch_path,
        })
    }

    async fn call(
        &mut self,
        peer_idx: usize,
        request_marker: u8,
        action: &str,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        let request_id = Id128::new([request_marker; 16]);
        send_json(
            &mut self.peers[peer_idx].socket,
            &serde_json::json!({
                "v": 1,
                "type": "command",
                "requestId": request_id,
                "action": action,
                "payload": payload,
            }),
        )
        .await;
        let response = next_json(&mut self.peers[peer_idx].socket, Duration::from_secs(15)).await;
        assert_eq!(
            response["requestId"],
            request_id.to_base64url(),
            "{response}"
        );
        response
    }

    async fn reconnect_peer(&mut self, peer_idx: usize) -> usize {
        let node = self.node.as_ref().unwrap();
        let client_id = self.peers[peer_idx].client_id.clone();
        let key = self.peers[peer_idx].key.clone();
        let sock = authenticate_socket(
            valid_management_socket(&self.area, node.listen_addr()).await,
            &key,
            &client_id,
            node_public_key(node),
        )
        .await;
        self.peers.push(R3WssPeer {
            client_id,
            key,
            socket: sock,
        });
        self.peers.len() - 1
    }

    async fn close(&mut self) -> bool {
        for peer in &mut self.peers {
            let _ = peer.socket.close(None).await;
        }
        self.peers.clear();
        let closed = if let Some(node) = self.node.take() {
            node.shutdown(None).await.is_ok()
        } else {
            true
        };
        let joined = if let Some(owner) = &self.owner {
            owner.join_actual().await.is_ok()
        } else {
            true
        };
        closed && joined
    }
}

async fn r3_wss_case(
    scenario: &str,
    seed_target: bool,
    run: impl for<'a> FnOnce(&'a mut R3WssCase) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
) {
    let mut fixture = R3WssCase {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        selection: None,
    };
    let result = AssertUnwindSafe(async {
        fixture.start(scenario, seed_target).await;
        run(&mut fixture).await;
    })
    .catch_unwind()
    .await;
    let _ = std::fs::write(
        fixture.area.directory.join("source-release-create"),
        b"owned fixture close",
    );
    let _ = std::fs::write(
        fixture.area.directory.join("release-read-first-held"),
        b"owned fixture close",
    );
    let closed = fixture.close().await;
    if result.is_err() || !closed {
        let retained = fixture.area.directory.clone();
        std::mem::forget(fixture.area);
        eprintln!("R3_WSS_FIXTURE_PRESERVED {retained:?}");
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(closed, "R3 WSS fixture shutdown must join cleanly");
}

async fn r3_send_binary(socket: &mut ManagementSocket, bytes: Vec<u8>) {
    socket
        .send(Message::Binary(bytes.into()))
        .await
        .expect("binary view/document control send");
}

async fn r3_next_binary(socket: &mut ManagementSocket) -> Vec<u8> {
    timeout(Duration::from_secs(10), async {
        loop {
            match socket.next().await.expect("frame").expect("ws") {
                Message::Binary(bytes) => return bytes.to_vec(),
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("unexpected frame kind {:?}", std::mem::discriminant(&other)),
            }
        }
    })
    .await
    .expect("timed out waiting for binary frame")
}

async fn r3_drain_document(socket: &mut ManagementSocket, stream_id: Id128) -> Vec<u8> {
    r3_send_binary(
        socket,
        TransferHeader::credit(stream_id, 0)
            .encode(&[])
            .expect("initial document credit"),
    )
    .await;
    let mut raw = Vec::new();
    loop {
        let bytes = r3_next_binary(socket).await;
        let (header, payload) = TransferHeader::decode(&bytes).expect("document frame");
        assert_eq!(header.stream(), stream_id);
        match header.record_kind() {
            Some(RecordKind::OriginalRaw) => {
                raw.extend_from_slice(payload);
                r3_send_binary(
                    socket,
                    TransferHeader::credit(stream_id, header.packet())
                        .encode(&[])
                        .expect("chunk credit"),
                )
                .await;
            }
            Some(RecordKind::SnapshotEnd) => {
                assert!(payload.is_empty());
                r3_send_binary(
                    socket,
                    TransferHeader::credit(stream_id, header.packet())
                        .encode(&[])
                        .expect("end credit"),
                )
                .await;
                break;
            }
            other => panic!("unexpected document record kind: {other:?}"),
        }
    }
    raw
}

async fn r3_drain_view_snapshot_or_rebound(
    socket: &mut ManagementSocket,
    stream_id: Id128,
) -> (Vec<(RecordKind, Vec<u8>)>, bool) {
    r3_send_binary(
        socket,
        TransferHeader::credit(stream_id, 0)
            .encode(&[])
            .expect("initial view credit"),
    )
    .await;
    let mut frames = Vec::new();
    let mut rebound = false;
    loop {
        let bytes = r3_next_binary(socket).await;
        let (header, payload) = TransferHeader::decode(&bytes).expect("view frame");
        assert_eq!(header.stream(), stream_id);
        let kind = header.record_kind().expect("view data record kind");
        frames.push((kind, payload.to_vec()));
        r3_send_binary(
            socket,
            TransferHeader::credit(stream_id, header.packet())
                .encode(&[])
                .expect("view credit"),
        )
        .await;
        if kind == RecordKind::Status {
            let status: serde_json::Value =
                serde_json::from_slice(payload).expect("status record JSON");
            if status["type"] == "stream_rebound" {
                assert_eq!(status["previousStreamId"], stream_id.to_base64url());
                rebound = true;
                break;
            }
        }
        if kind == RecordKind::SnapshotEnd {
            break;
        }
    }
    (frames, rebound)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_register_idempotency_conflict_and_cross_actor_isolation() {
    r3_wss_case("r2-v01-discover", false, |f| {
        Box::pin(async move {
            let selection = f.selection_json();
            let first = f
                .call(
                    0,
                    11,
                    "source.session.register",
                    serde_json::json!({
                        "selection": selection,
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(first["ok"], true, "{first}");
            assert_eq!(first["result"]["state"], "registered");
            assert_eq!(first["result"]["action"], "create");
            assert_eq!(first["result"]["actorId"], f.peers[0].client_id);
            let op_a = first["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(!f.area.directory.join("source-child-pid").exists());
            assert!(!f.area.directory.join("source-create-count").exists());

            // Same actor on second authenticated connection retries exact requestId + payload -> idempotent
            let repeat = f
                .call(
                    2,
                    11,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(repeat, first, "same-actor retry must be idempotent");
            assert!(!f.area.directory.join("source-child-pid").exists());

            // Same actor reuses requestId with different operation -> request_conflict
            let conflict = f
                .call(
                    0,
                    11,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(conflict["ok"], false, "{conflict}");
            assert_eq!(conflict["error"]["code"], "request_conflict", "{conflict}");

            // Different valid actor sends same requestId + same payload -> distinct operationId
            let actor_b = f
                .call(
                    1,
                    11,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(actor_b["ok"], true, "{actor_b}");
            assert_eq!(actor_b["result"]["actorId"], f.peers[1].client_id);
            assert_ne!(actor_b["result"]["sourceOperationId"], op_a);

            // Invalid profileRevision = 0 rejected before registration
            let mut bad_selection = f.selection_json();
            bad_selection["profileRevision"] = serde_json::json!("0");
            let invalid_rev = f
                .call(
                    0,
                    12,
                    "source.session.register",
                    serde_json::json!({
                        "selection": bad_selection,
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(invalid_rev["ok"], false, "{invalid_rev}");
            assert_eq!(invalid_rev["error"]["code"], "invalid_configuration");

            // Unknown sourceId rejected pre-registration
            let mut unknown_source = f.selection_json();
            unknown_source["sourceId"] = serde_json::json!(r3_id(99).encode());
            let bad_source = f
                .call(
                    0,
                    13,
                    "source.session.register",
                    serde_json::json!({
                        "selection": unknown_source,
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(bad_source["ok"], false, "{bad_source}");
            assert_ne!(bad_source["error"]["code"], "");

            // Revoked actor closes cleanly with 4410 without creating a registration
            let actor_b_id = f.peers[1].client_id.clone();
            ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &actor_b_id)
                .await
                .unwrap();
            let (code, reason) = next_close(&mut f.peers[1].socket, Duration::from_secs(5)).await;
            assert_eq!(code, 4410);
            assert_eq!(reason, "revoked");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_execute_foreign_actor_rejection_and_disconnect_reconnect_recovery() {
    r3_wss_case("delayed-create", false, |f| {
        Box::pin(async move {
            let reg = f
                .call(
                    0,
                    21,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(reg["ok"], true, "{reg}");
            let op_id = reg["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();

            // Foreign valid paired actor cannot execute actor A's registered operation
            let foreign = f
                .call(
                    1,
                    22,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(foreign["ok"], false, "{foreign}");
            assert_eq!(foreign["error"]["code"], "invalid_client", "{foreign}");

            // Foreign socket remains usable for permitted queries
            let peer_status = f
                .call(
                    1,
                    23,
                    "source.session.status",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(peer_status["ok"], true, "{peer_status}");
            assert_eq!(peer_status["result"]["state"], "registered");

            // Actor A begins execute on peer 0 while delayed-create holds in child
            send_json(
                &mut f.peers[0].socket,
                &serde_json::json!({
                    "v": 1,
                    "type": "command",
                    "requestId": Id128::new([24; 16]),
                    "action": "source.session.execute",
                    "payload": { "sourceOperationId": op_id }
                }),
            )
            .await;
            timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("source-create-waiting").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("child reached delayed-create gate");

            // Caller disconnects while child RPC is still in-flight
            let _ = f.peers[0].socket.close(None).await;
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Release child gate after caller socket dropped
            std::fs::write(f.area.directory.join("source-release-create"), b"release").unwrap();

            let reconnected_idx = f.reconnect_peer(0).await;
            let fact = timeout(Duration::from_secs(15), async {
                loop {
                    let status = f
                        .call(
                            reconnected_idx,
                            25,
                            "source.session.status",
                            serde_json::json!({ "sourceOperationId": op_id }),
                        )
                        .await;
                    assert_eq!(status["ok"], true, "{status}");
                    if status["result"]["state"] == "terminal" {
                        break status["result"].clone();
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("operation completed in background after caller disconnect");
            assert_eq!(fact["outcome"], "succeeded", "{fact}");
            assert_eq!(fact["resultAvailable"], false, "{fact}");
            assert_eq!(fact["response"]["kind"], "created", "{fact}");
            assert_eq!(
                fact["response"]["nativeSessionId"], "source-native-created",
                "{fact}"
            );
            assert_eq!(fact["response"]["ack"], "confirmed", "{fact}");

            let reconciled = f
                .call(
                    reconnected_idx,
                    26,
                    "source.session.reconcile",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(reconciled["ok"], true, "{reconciled}");
            assert_eq!(reconciled["result"]["outcome"], "succeeded");
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_result_open_shared_chunked_json_and_release() {
    r3_wss_case("r2-v01-discover", false, |f| {
        Box::pin(async move {
            let reg = f
                .call(
                    0,
                    31,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(reg["ok"], true, "{reg}");
            let op_id = reg["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();

            let exec = f
                .call(
                    0,
                    32,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(exec["ok"], true, "{exec}");
            assert_eq!(exec["result"]["outcome"], "succeeded");
            assert_eq!(exec["result"]["resultAvailable"], true);

            // Paired peer (actor B) opens the shared result stream BEFORE actor A drains it
            let open_b = f
                .call(
                    1,
                    33,
                    "source.session.result.open",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(open_b["ok"], true, "{open_b}");
            assert_eq!(open_b["result"]["streamKind"], "native_session_result");
            assert!(open_b["result"]["targetId"].is_null());
            assert!(open_b["result"]["revision"].is_null());
            assert_eq!(open_b["result"]["encoding"], "utf8_json");
            let stream_b =
                Id128::parse_base64url(open_b["result"]["streamId"].as_str().unwrap()).unwrap();

            // Revoked third connection (peer 2 shares actor A; revoke a freshly paired actor C instead)
            let key_c = SigningKey::from_slice(&[73; 32]).unwrap();
            let id_c = pair_browser(f.node.as_ref().unwrap(), &key_c, "R3 revoked peer").await;
            let sock_c = authenticate_socket(
                valid_management_socket(&f.area, f.node.as_ref().unwrap().listen_addr()).await,
                &key_c,
                &id_c,
                node_public_key(f.node.as_ref().unwrap()),
            )
            .await;
            f.peers.push(R3WssPeer {
                client_id: id_c.clone(),
                key: key_c,
                socket: sock_c,
            });
            let c_idx = f.peers.len() - 1;
            ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &id_c)
                .await
                .unwrap();
            let (code, reason) =
                next_close(&mut f.peers[c_idx].socket, Duration::from_secs(5)).await;
            assert_eq!(code, 4410);
            assert_eq!(reason, "revoked");

            // Peer B still drains the bare ACP "result" JSON object without interference
            let raw = r3_drain_document(&mut f.peers[1].socket, stream_b).await;
            let text = std::str::from_utf8(&raw).expect("valid utf-8 native_session_result");
            let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
            assert!(
                parsed.get("jsonrpc").is_none(),
                "bare result object only: {text}"
            );
            assert!(parsed["sessions"].is_array(), "{text}");
            assert_eq!(parsed["sessions"][0]["sessionId"], "external-session-A");
            assert_eq!(parsed["nextCursor"], "opaque-next-cursor");
            assert!(text.contains("1234567890123456789012345678901234567890"));
            assert!(text.contains("1.2300e+09"));

            // Register + execute remote-error discover, then release result before opening
            let err_reg = f
                .call(
                    0,
                    35,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "discover", "cursor": "remote-error" }
                    }),
                )
                .await;
            let err_op = err_reg["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();
            let err_exec = f
                .call(
                    0,
                    36,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": err_op }),
                )
                .await;
            assert_eq!(err_exec["ok"], true, "{err_exec}");
            assert_eq!(err_exec["result"]["outcome"], "refused");
            assert_eq!(err_exec["result"]["resultAvailable"], true);

            let released = f
                .call(
                    1,
                    37,
                    "source.session.result.release",
                    serde_json::json!({ "sourceOperationId": err_op }),
                )
                .await;
            assert_eq!(released["ok"], true, "{released}");
            assert_eq!(released["result"]["released"], true);

            let after_release_open = f
                .call(
                    0,
                    38,
                    "source.session.result.open",
                    serde_json::json!({ "sourceOperationId": err_op }),
                )
                .await;
            assert_eq!(after_release_open["ok"], false, "{after_release_open}");
            assert_eq!(after_release_open["error"]["code"], "body_unavailable");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_unsupported_capability_for_no_list_child() {
    r3_wss_case("no-list-with-new", false, |f| {
        Box::pin(async move {
            let reg_discover = f
                .call(
                    0,
                    41,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(reg_discover["ok"], true, "{reg_discover}");
            let op_discover = reg_discover["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();
            let exec_discover = f
                .call(
                    0,
                    42,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": op_discover }),
                )
                .await;
            assert_eq!(exec_discover["ok"], false, "{exec_discover}");
            assert_eq!(
                exec_discover["error"]["code"], "unsupported_capability",
                "{exec_discover}"
            );

            // session/new IS supported on no-list-with-new -> succeeds
            let reg_create = f
                .call(
                    0,
                    43,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            let op_create = reg_create["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();
            let exec_create = f
                .call(
                    0,
                    44,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": op_create }),
                )
                .await;
            assert_eq!(exec_create["ok"], true, "{exec_create}");
            assert_eq!(exec_create["result"]["outcome"], "succeeded");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_view_open_source_without_target_streams_via_pump_and_closes() {
    r3_wss_case("read-first", false, |f| {
        Box::pin(async move {
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test()
                .clone();
            let before_targets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_targets")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                before_targets, 0,
                "external source test starts with 0 product_targets"
            );

            let opened = f
                .call(
                    0,
                    51,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": "external-source-session-A",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let view = &opened["result"];
            assert_eq!(view["type"], "view.opened", "{opened}");
            let stream_id = Id128::parse_base64url(view["streamId"].as_str().unwrap()).unwrap();
            assert!(
                matches!(
                    view["status"]["mode"].as_str(),
                    Some("current_only" | "replaying" | "replay_requested" | "replay_complete")
                ),
                "{view}"
            );

            let (first_frames, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream_id).await;
            let (active_stream, frames) = if rebound {
                r3_send_binary(
                    &mut f.peers[0].socket,
                    TransferHeader::detach(stream_id)
                        .encode(&[])
                        .expect("view detach"),
                )
                .await;
                let rebound_resp = f
                    .call(
                        0,
                        52,
                        "view.rebind",
                        serde_json::json!({ "streamId": stream_id.to_base64url() }),
                    )
                    .await;
                assert_eq!(rebound_resp["ok"], true, "{rebound_resp}");
                let rebound_view = &rebound_resp["result"];
                assert_eq!(rebound_view["type"], "view.opened", "{rebound_resp}");
                let next_stream =
                    Id128::parse_base64url(rebound_view["streamId"].as_str().unwrap()).unwrap();
                assert_ne!(
                    next_stream, stream_id,
                    "view.rebind must allocate new streamId"
                );
                assert_eq!(
                    rebound_view["status"]["mode"], "replay_complete",
                    "{rebound_resp}"
                );
                let (next_frames, next_rebound) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, next_stream).await;
                assert!(
                    !next_rebound,
                    "rebound replay_complete stream must not rebind again"
                );
                (next_stream, next_frames)
            } else {
                (stream_id, first_frames)
            };
            let replay_wait = timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("read-first-replay-count").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await;
            assert!(
                replay_wait.is_ok(),
                "read-first replay must execute via session/load"
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("read-first-replay-count")).unwrap(),
                "1"
            );
            assert!(
                frames
                    .iter()
                    .any(|(kind, _)| *kind == RecordKind::ItemProjection
                        || *kind == RecordKind::OriginalRaw),
                "run_view_pump must stream data records before SnapshotEnd"
            );
            assert_eq!(
                frames.last().map(|(k, _)| *k),
                Some(RecordKind::SnapshotEnd)
            );

            let closed = f
                .call(
                    0,
                    53,
                    "view.close",
                    serde_json::json!({ "streamId": active_stream.to_base64url() }),
                )
                .await;
            assert_eq!(closed["ok"], true, "{closed}");
            assert_eq!(closed["result"]["state"], "closed", "{closed}");
            assert_eq!(closed["result"]["alreadyClosed"], false, "{closed}");

            let retry_close = f
                .call(
                    0,
                    54,
                    "view.close",
                    serde_json::json!({ "streamId": active_stream.to_base64url() }),
                )
                .await;
            assert_eq!(retry_close["ok"], true, "{retry_close}");
            assert_eq!(retry_close["result"], closed["result"], "{retry_close}");

            let after_targets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_targets")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                after_targets, 0,
                "view.open_source must NOT forge product_targets rows"
            );
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_view_open_source_no_replay_current_only() {
    r3_wss_case("no-replay-source", false, |f| {
        Box::pin(async move {
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test()
                .clone();
            let before_targets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_targets")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(before_targets, 0);

            let opened = f
                .call(
                    0,
                    61,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": "external-no-replay-session",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let view = &opened["result"];
            assert_eq!(view["type"], "view.opened", "{opened}");
            assert_eq!(view["status"]["mode"], "current_only", "{view}");
            assert_eq!(view["status"]["historyIncomplete"], true, "{view}");
            assert_eq!(view["status"]["cacheRecoverable"], false, "{view}");
            let stream_id = Id128::parse_base64url(view["streamId"].as_str().unwrap()).unwrap();

            let closed = f
                .call(
                    0,
                    62,
                    "view.close",
                    serde_json::json!({ "streamId": stream_id.to_base64url() }),
                )
                .await;
            assert_eq!(closed["ok"], true, "{closed}");
            assert_eq!(closed["result"]["state"], "closed", "{closed}");

            let after_targets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_targets")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(after_targets, 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_launch_path_outside_workspace_rejected_and_inside_accepted() {
    r3_wss_case("r2-v01-discover", false, |f| {
        Box::pin(async move {
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test()
                .clone();

            let outside = f
                .area
                .directory
                .parent()
                .expect("test directory parent")
                .to_str()
                .unwrap()
                .to_owned();
            let mut outside_sel = f.selection_json();
            outside_sel["launchPath"] = serde_json::json!(outside);

            let reg_outside = f
                .call(
                    0,
                    71,
                    "source.session.register",
                    serde_json::json!({
                        "selection": outside_sel.clone(),
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(reg_outside["ok"], false, "{reg_outside}");
            assert_eq!(
                reg_outside["error"]["code"], "invalid_request",
                "{reg_outside}"
            );
            assert!(!f.area.directory.join("source-child-pid").exists());

            let view_outside = f
                .call(
                    0,
                    72,
                    "view.open_source",
                    serde_json::json!({
                        "selection": outside_sel,
                        "nativeSessionId": "external-outside-workspace",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(view_outside["ok"], false, "{view_outside}");
            assert_eq!(
                view_outside["error"]["code"], "invalid_configuration",
                "{view_outside}"
            );
            assert!(!f.area.directory.join("source-child-pid").exists());
            let targets_after_outside: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM product_targets")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(targets_after_outside, 0);

            let missing_dir = f.area.directory.join("does-not-exist");
            let mut missing_sel = f.selection_json();
            missing_sel["launchPath"] = serde_json::json!(missing_dir.to_str().unwrap());
            let reg_missing = f
                .call(
                    0,
                    73,
                    "source.session.register",
                    serde_json::json!({
                        "selection": missing_sel,
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(reg_missing["ok"], false, "{reg_missing}");
            assert_eq!(
                reg_missing["error"]["code"], "invalid_request",
                "{reg_missing}"
            );

            let file_inside = f.area.directory.join("not-a-dir.txt");
            std::fs::write(&file_inside, b"x").unwrap();
            let mut file_sel = f.selection_json();
            file_sel["launchPath"] = serde_json::json!(file_inside.to_str().unwrap());
            let reg_file = f
                .call(
                    0,
                    74,
                    "source.session.register",
                    serde_json::json!({
                        "selection": file_sel,
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(reg_file["ok"], false, "{reg_file}");
            assert_eq!(reg_file["error"]["code"], "invalid_request", "{reg_file}");

            let inside_subdir = f.area.directory.join("nested-cwd");
            std::fs::create_dir(&inside_subdir).unwrap();
            let mut inside_sel = f.selection_json();
            inside_sel["launchPath"] = serde_json::json!(inside_subdir.to_str().unwrap());
            let reg_inside = f
                .call(
                    0,
                    75,
                    "source.session.register",
                    serde_json::json!({
                        "selection": inside_sel,
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(reg_inside["ok"], true, "{reg_inside}");
            assert_eq!(reg_inside["result"]["state"], "registered");
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_source_create_then_open_source_reaches_replay_complete_without_quarantine() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            let reg = f
                .call(
                    0,
                    81,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "create" }
                    }),
                )
                .await;
            assert_eq!(reg["ok"], true, "{reg}");
            let op_id = reg["result"]["sourceOperationId"]
                .as_str()
                .unwrap()
                .to_owned();
            let exec = f
                .call(
                    0,
                    82,
                    "source.session.execute",
                    serde_json::json!({ "sourceOperationId": op_id }),
                )
                .await;
            assert_eq!(exec["ok"], true, "{exec}");
            assert_eq!(exec["result"]["outcome"], "succeeded", "{exec}");
            let created_id = exec["result"]["response"]["nativeSessionId"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(created_id, "source-native-created");

            let opened = f
                .call(
                    0,
                    83,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": created_id,
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let stream_id =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();

            let (first_frames, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream_id).await;
            let (active_stream, frames) = if rebound {
                r3_send_binary(
                    &mut f.peers[0].socket,
                    TransferHeader::detach(stream_id)
                        .encode(&[])
                        .expect("view detach"),
                )
                .await;
                let rebound_resp = f
                    .call(
                        0,
                        84,
                        "view.rebind",
                        serde_json::json!({ "streamId": stream_id.to_base64url() }),
                    )
                    .await;
                assert_eq!(rebound_resp["ok"], true, "{rebound_resp}");
                assert_eq!(
                    rebound_resp["result"]["status"]["mode"], "replay_complete",
                    "{rebound_resp}"
                );
                let next_stream = Id128::parse_base64url(
                    rebound_resp["result"]["streamId"].as_str().unwrap(),
                )
                .unwrap();
                // Trailing Detach for the already-rebound streamId must NOT close WebSocket with 4400
                r3_send_binary(
                    &mut f.peers[0].socket,
                    TransferHeader::detach(stream_id)
                        .encode(&[])
                        .expect("trailing detach"),
                )
                .await;
                let (next_frames, next_rebound) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, next_stream).await;
                assert!(!next_rebound);
                (next_stream, next_frames)
            } else {
                (stream_id, first_frames)
            };
            assert!(
                frames
                    .iter()
                    .any(|(kind, _)| *kind == RecordKind::ItemProjection
                        || *kind == RecordKind::OriginalRaw),
                "replayed records must arrive after source.session.create"
            );

            let quarantined = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .any_source_quarantined_for_test()
                .unwrap();
            assert!(
                !quarantined,
                "State update for external_reads session must not quarantine source"
            );

            let reg_discover = f
                .call(
                    0,
                    85,
                    "source.session.register",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "operation": { "kind": "discover", "cursor": null }
                    }),
                )
                .await;
            assert_eq!(reg_discover["ok"], true, "{reg_discover}");
            let exec_discover = f
                .call(
                    0,
                    86,
                    "source.session.execute",
                    serde_json::json!({
                        "sourceOperationId": reg_discover["result"]["sourceOperationId"]
                    }),
                )
                .await;
            assert_eq!(exec_discover["ok"], true, "{exec_discover}");
            assert_eq!(exec_discover["result"]["outcome"], "succeeded");

            let closed = f
                .call(
                    0,
                    87,
                    "view.close",
                    serde_json::json!({ "streamId": active_stream.to_base64url() }),
                )
                .await;
            assert_eq!(closed["ok"], true, "{closed}");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r3_wss_external_read_quarantine_boundaries() {
    r3_wss_case("external-reads-quarantine", false, |f| {
        Box::pin(async move {
            // 1. State update for known external_reads session does NOT quarantine
            let open_state = f
                .call(
                    0,
                    91,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": "external-session-state",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(open_state["ok"], true, "{open_state}");
            let state_stream =
                Id128::parse_base64url(open_state["result"]["streamId"].as_str().unwrap())
                    .unwrap();
            let (_frames, _rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, state_stream).await;
            assert!(
                !f.node
                    .as_ref()
                    .unwrap()
                    .session_services
                    .as_ref()
                    .unwrap()
                    .native
                    .any_source_quarantined_for_test()
                    .unwrap(),
                "known external_reads State must not quarantine source"
            );

            // 2. Unassociated Permission MUST still quarantine even if session is in external_reads
            let open_perm = f
                .call(
                    0,
                    92,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": "external-session-permission",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(open_perm["ok"], true, "{open_perm}");
            let native = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native;
            timeout(Duration::from_secs(5), async {
                while !native.any_source_quarantined_for_test().unwrap() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("unassociated Permission on external_reads session must quarantine source");
        })
    })
    .await;

    r3_wss_case("external-reads-quarantine", false, |f| {
        Box::pin(async move {
            // 3. Truly unknown State (session not in targets or external_reads) MUST still quarantine
            let open_unknown = f
                .call(
                    0,
                    93,
                    "view.open_source",
                    serde_json::json!({
                        "selection": f.selection_json(),
                        "nativeSessionId": "external-session-trigger-unknown",
                        "reason": "first_open"
                    }),
                )
                .await;
            assert_eq!(open_unknown["ok"], true, "{open_unknown}");
            let native = &f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native;
            timeout(Duration::from_secs(5), async {
                while !native.any_source_quarantined_for_test().unwrap() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("truly unknown State must still quarantine source");
        })
    })
    .await;
}