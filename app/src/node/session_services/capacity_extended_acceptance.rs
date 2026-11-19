// Second batch; include in the same R3 module after capacity_node_acceptance.rs.
// This source deliberately uses only baseline Rust interfaces.

async fn capacity_alternate_selection(
    f: &mut R3WssCase,
    source: crate::domain::EntityId,
) -> SourceLaunchSelection {
    let node = f.node.as_ref().unwrap();
    if source != r3_id(1) {
        sqlx::query("INSERT INTO product_sources VALUES(?,'capacity independent second source')")
            .bind(source.encode())
            .execute(node.storage.as_ref().unwrap().pool_for_test())
            .await
            .unwrap();
    }
    let observed = node
        .management
        .as_ref()
        .unwrap()
        .observe_next_document_socket_for_test()
        .unwrap();
    let socket = authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        &f.peers[0].key,
        &f.peers[0].client_id,
        node_public_key(node),
    )
    .await;
    let endpoint = observed.await.unwrap();
    let client = endpoint.client();
    let peer = crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);
    let raw = format!(
        r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}","{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
        r3_utf16(&peer.to_string_lossy()),
        r3_utf16("normal"),
        r3_utf16("independent-alternate-profile"),
    );
    let profile =
        crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes()).unwrap();
    let draft = CatalogDraft::install_harness(
        "Capacity independent alternate".into(),
        SourceSelection::Existing(source),
        profile,
    )
    .unwrap();
    let catalog = &node.session_services.as_ref().unwrap().catalog;
    let actor = client.context.actor();
    let registered = catalog
        .register(actor, draft, &r3_deadline())
        .await
        .unwrap();
    let applied = catalog
        .apply(actor, registered, &r3_deadline())
        .await
        .unwrap();
    let (harness, profile) = match applied.fact().result().unwrap() {
        CatalogResult::Harness {
            harness, profile, ..
        } => (*harness, *profile),
        _ => panic!("expected original catalog harness result"),
    };
    let mut selection = f.selection.as_ref().unwrap().clone();
    selection.source = source;
    selection.harness = harness;
    selection.profile = profile;
    f.peers.push(R3WssPeer {
        client_id: f.peers[0].client_id.clone(),
        key: f.peers[0].key.clone(),
        socket,
    });
    selection
}

async fn capacity_open_and_close_reader(f: &mut R3WssCase, marker: u8, native: &str) {
    let response = f
        .call(
            0,
            marker,
            "view.open_source",
            serde_json::json!({
                "selection": f.selection_json(), "nativeSessionId":native, "reason":"first_open",
            }),
        )
        .await;
    assert_eq!(response["ok"], true, "{response}");
    let stream = Id128::parse_base64url(response["result"]["streamId"].as_str().unwrap()).unwrap();
    let (_, rebound) = r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream).await;
    let active = if rebound {
        let response = f
            .call(
                0,
                marker + 1,
                "view.rebind",
                serde_json::json!({"streamId":stream.to_base64url()}),
            )
            .await;
        assert_eq!(response["ok"], true, "{response}");
        let next =
            Id128::parse_base64url(response["result"]["streamId"].as_str().unwrap()).unwrap();
        let (_, repeated) = r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, next).await;
        assert!(!repeated);
        next
    } else {
        stream
    };
    let response = f
        .call(
            0,
            marker + 2,
            "view.close",
            serde_json::json!({"streamId":active.to_base64url()}),
        )
        .await;
    assert_eq!(response["ok"], true, "{response}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_same_native_alternate_profile_reuses_original_source() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 10).await;
            let original = f.selection.as_ref().unwrap().clone();
            let alternate = capacity_alternate_selection(f, r3_id(1)).await;
            assert_ne!(alternate.profile, original.profile);
            f.selection = Some(alternate);
            capacity_open_and_close_reader(f, 20, &native).await;
            capacity_expect(f, 23, 1, 0).await;
            let events = capacity_events(f);
            let created = events
                .iter()
                .find(|e| e["method"] == "session/new")
                .unwrap();
            let replay = events
                .iter()
                .find(|e| e["method"] == "session/load")
                .unwrap();
            assert_eq!(
                created["pid"], replay["pid"],
                "original source owns this native"
            );
            assert_eq!(
                capacity_method_count(f, "initialize"),
                1,
                "alternate profile must not replace running native connection"
            );
            assert_eq!(capacity_method_count(f, "session/new"), 1);
        })
    })
    .await;
}

struct CapacityRemoveFixtureFile(std::path::PathBuf);
impl Drop for CapacityRemoveFixtureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_same_short_id_different_explicit_sources_do_not_merge() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let flag = f.area.directory.join("capacity-fixed-id");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(flag, b"synthetic native identity").unwrap();
            let first = capacity_create(f, 30).await;
            let second_source = capacity_alternate_selection(f, r3_id(30)).await;
            f.selection = Some(second_source);
            let second = capacity_create(f, 32).await;
            assert_eq!(
                first, second,
                "controlled peers produce the same short native ID"
            );
            capacity_expect(f, 34, 2, 0).await;
            let events = capacity_events(f);
            let created: Vec<_> = events
                .iter()
                .filter(|e| e["method"] == "session/new")
                .collect();
            assert_eq!(created.len(), 2);
            assert_ne!(created[0]["pid"], created[1]["pid"]);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_conflicting_new_identity_does_not_return_unknown_reservation() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let flag = f.area.directory.join("capacity-duplicate-id");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(flag, b"synthetic duplicate identity").unwrap();
            capacity_create(f, 40).await;
            let conflict =
                capacity_source_operation(f, 42, serde_json::json!({"kind":"create"})).await;
            assert!(
                conflict["ok"] == false || conflict["result"]["outcome"] != "succeeded",
                "duplicate raw new response cannot replace original owner: {conflict}"
            );
            let held = capacity_inspect(f, 44).await;
            assert_eq!(
                held["occupied"], 2,
                "potential new resource remains a separately owned reservation: {held}"
            );
            assert_eq!(capacity_method_count(f, "session/new"), 2);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_capacity_inspection_requires_current_authenticated_actor() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let node = f.node.as_ref().unwrap();
            let mut unauthenticated = valid_management_socket(&f.area, node.listen_addr()).await;
            send_json(
                &mut unauthenticated,
                &serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([50;16]),
                    "action":"session.capacity.inspect","payload":{}
                }),
            )
            .await;
            let (code, _) = next_close(&mut unauthenticated, Duration::from_secs(5)).await;
            assert_eq!(code, 4400, "pre-auth commands must not expose capacity");
            let inspected = f
                .call(1, 51, "session.capacity.inspect", serde_json::json!({}))
                .await;
            assert_eq!(inspected["ok"], true, "{inspected}");
            let revoked = f.peers[1].client_id.clone();
            ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &revoked)
                .await
                .unwrap();
            let (code, reason) = next_close(&mut f.peers[1].socket, Duration::from_secs(5)).await;
            assert_eq!(code, 4410);
            assert_eq!(reason, "revoked");
            // The remaining authenticated actor still receives the real snapshot.
            capacity_expect(f, 52, 0, 0).await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_confirmed_source_shutdown_reconciles_before_restart() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            capacity_create(f, 60).await;
            capacity_expect(f, 62, 1, 0).await;
            let client = f.peers[0].client_id.clone();
            let key = f.peers[0].key.clone();
            // A fully confirmed original source convergence is valid necessary
            // coordination. This is not an unconfirmed/crashed-runtime fixture.
            assert!(
                f.close().await,
                "original node and all owned peers must join"
            );
            f.owner.take();
            f.owner = Some(NodeProcessOwner::begin(
                &tokio::runtime::Handle::current(),
                f.area.config(free_address()),
                Arc::new(FakeProtector::default()),
            ));
            f.node = Some(f.owner.as_ref().unwrap().ready().await.unwrap());
            let node = f.node.as_ref().unwrap();
            let socket = authenticate_socket(
                valid_management_socket(&f.area, node.listen_addr()).await,
                &key,
                &client,
                node_public_key(node),
            )
            .await;
            f.peers.push(R3WssPeer {
                client_id: client,
                key,
                socket,
            });
            capacity_expect(f, 63, 0, 0).await;
            capacity_create(f, 64).await;
            capacity_expect(f, 66, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/new"), 2);
            assert_eq!(
                capacity_method_count(f, "session/close"),
                0,
                "whole-source convergence must not issue unrelated per-native closes"
            );
        })
    })
    .await;
}
