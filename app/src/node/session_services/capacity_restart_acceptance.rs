// A real abrupt exit of our exclusively owned fixture node; never a user node.

async fn capacity_crash_original_snapshot(
    pool: &sqlx::SqlitePool,
    instance: &str,
) -> serde_json::Value {
    let source: String = sqlx::query_scalar(
        "SELECT json_object('instance',instance_id,'source',source_id,'runtime',runtime_epoch,\
        'connection',connection_epoch,'config',config_identity,'revision',hex(config_revision),\
        'owner',owner_kind,'lifecycle',lifecycle,'closeBasis',close_basis,'closeStage',close_stage) \
        FROM node_native_source_instances WHERE instance_id=?",
    ).bind(instance).fetch_one(pool).await.unwrap();
    let capacity: String = sqlx::query_scalar(
        "SELECT json_group_array(json_object('reservation',reservation_id,'source',source_id,\
        'native',native_id,'instance',instance_id,'runtime',runtime_epoch,'create',create_operation,\
        'root',root_verified,'physical',physically_closed,'released',released)) \
        FROM (SELECT * FROM node_session_capacity WHERE instance_id=? ORDER BY reservation_id)",
    ).bind(instance).fetch_one(pool).await.unwrap();
    serde_json::json!({
        "source":serde_json::from_str::<serde_json::Value>(&source).unwrap(),
        "capacity":serde_json::from_str::<serde_json::Value>(&capacity).unwrap(),
    })
}

async fn capacity_restart_fresh_explicit_source(f: &mut R3WssCase) -> SourceLaunchSelection {
    let node = f.node.as_ref().unwrap();
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
        r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
        r3_utf16(&peer.to_string_lossy()),
        r3_utf16("normal"),
    );
    let profile =
        crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes()).unwrap();
    let draft = CatalogDraft::install_harness(
        "Capacity independent post-crash configuration".into(),
        SourceSelection::New {
            name: "Capacity independent post-crash source".into(),
        },
        profile,
    )
    .unwrap();
    let catalog = &node.session_services.as_ref().unwrap().catalog;
    let registration = catalog
        .register(client.context.actor(), draft, &r3_deadline())
        .await
        .unwrap();
    let applied = catalog
        .apply(client.context.actor(), registration, &r3_deadline())
        .await
        .unwrap();
    let (source, harness, profile) = match applied.fact().result().unwrap() {
        CatalogResult::Harness {
            source,
            harness,
            profile,
        } => (*source, *harness, *profile),
        _ => panic!("actual newly installed explicit source"),
    };
    let mut selected = f.selection.as_ref().unwrap().clone();
    assert_ne!(source, selected.source);
    selected.source = source;
    selected.harness = harness;
    selected.profile = profile;
    f.peers.push(R3WssPeer {
        client_id: f.peers[0].client_id.clone(),
        key: f.peers[0].key.clone(),
        socket,
    });
    selected
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "only invoked by the owned capacity crash/restart acceptance parent"]
async fn capacity_crash_seed_owned_node() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CAPACITY_CRASH_OWNED_DIRECTORY").expect("explicit owned crash directory"),
    );
    assert!(
        directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("acp-remote-node-")
    );
    let mut f = R3WssCase {
        area: TestArea {
            certificate: directory.join("server.pem"),
            private_key: directory.join("server-key.pem"),
            directory,
        },
        owner: None,
        node: None,
        peers: Vec::new(),
        selection: None,
    };
    // The outer test owns this process and its complete Windows Job tree.
    f.start("normal", false).await;
    let pending = std::env::var("CAPACITY_CRASH_MODE").as_deref() == Ok("waiting-new");
    let (native, operation, reserved) = if pending {
        std::fs::write(
            f.area.directory.join("capacity-hold-new"),
            b"owned crash gate",
        )
        .unwrap();
        let registered = f
            .call(
                0,
                230,
                "source.session.register",
                serde_json::json!({
                    "selection":f.selection_json(),"operation":{"kind":"create"}
                }),
            )
            .await;
        assert_eq!(registered["ok"], true, "{registered}");
        let operation = registered["result"]["sourceOperationId"].clone();
        send_json(
            &mut f.peers[0].socket,
            &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([231;16]),
                "action":"source.session.execute","payload":{"sourceOperationId":operation}
            }),
        )
        .await;
        timeout(Duration::from_secs(10), async {
            while !f.area.directory.join("capacity-entered-new").is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("peer received actual new; original node has no identity response");
        let snapshot = f
            .call(1, 232, "session.capacity.inspect", serde_json::json!({}))
            .await;
        assert_eq!(snapshot["result"]["occupied"], 1, "{snapshot}");
        assert_eq!(snapshot["result"]["reserved"], 1, "{snapshot}");
        (None, operation, 1_u64)
    } else {
        let native = capacity_create(&mut f, 230).await;
        capacity_expect(&mut f, 232, 1, 0).await;
        (Some(native), serde_json::Value::Null, 0)
    };
    let fact = serde_json::json!({
        "selection":f.selection_json(),"clientId":f.peers[0].client_id,
        "nativeId":native,"sourceOperationId":operation,"occupied":1,"reserved":reserved,
        "boundary":"real committed SQLite and original source; no graceful shutdown",
    });
    std::fs::write(
        f.area.directory.join("capacity-crash-ready.json"),
        serde_json::to_vec(&fact).unwrap(),
    )
    .unwrap();
    println!("CAPACITY_OWNED_CRASH_READY occupied=1; deliberate fixture exit=42");
    // Deliberately skips Rust Drop. OS Job handles close, and the outer actual
    // owner independently proves that the entire descendant tree has exited.
    std::process::exit(42);
}

async fn capacity_crash_restart_case(mode: &'static str, reserved: u64) {
    use crate::platform::process_owner::{OwnedProcess, ProcessSpec, ShutdownDisposition};
    use tokio::io::AsyncReadExt as _;
    let mut f = R3WssCase {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        selection: None,
    };
    let result = AssertUnwindSafe(async {
        let mut process = OwnedProcess::spawn(
            ProcessSpec {
                executable: std::env::current_exe().unwrap(),
                args: vec![
                    "--exact".into(),
                    "node::tests::r3_source_wss_development::capacity_crash_seed_owned_node".into(),
                    "--ignored".into(),
                    "--nocapture".into(),
                    "--test-threads=1".into(),
                ],
                cwd: f.area.directory.clone(),
                env: vec![
                    (
                        "CAPACITY_CRASH_OWNED_DIRECTORY".into(),
                        f.area.directory.as_os_str().to_owned(),
                    ),
                    ("CAPACITY_CRASH_MODE".into(), mode.into()),
                ],
            },
            tokio::time::Instant::now() + Duration::from_secs(10),
        )
        .await
        .unwrap();
        let mut io = process.take_io().unwrap();
        drop(io.stdin);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let captured = timeout(Duration::from_secs(40), async {
            tokio::try_join!(
                io.stdout.read_to_end(&mut stdout),
                io.stderr.read_to_end(&mut stderr),
            )
        })
        .await;
        // Always converge the actual outer Job before any assertion or reopening.
        let first = process
            .shutdown(tokio::time::Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        let closed = if first.disposition == ShutdownDisposition::Incomplete {
            process
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(8))
                .await
                .unwrap()
        } else {
            first
        };
        assert_eq!(
            closed.active_processes, 0,
            "owned fixture crash descendants must all exit"
        );
        assert_eq!(
            closed.root_exit_code,
            Some(42),
            "actual child intentionally exited without node shutdown"
        );
        captured
            .expect("bounded actual crash fixture run")
            .expect("read owned fixture pipes");
        assert!(String::from_utf8_lossy(&stdout).contains("CAPACITY_OWNED_CRASH_READY"));
        // These are exclusively our synthetic fixture diagnostics, with no body or key.
        std::fs::write(
            f.area.directory.join("capacity-crash-seed.stdout.log"),
            stdout,
        )
        .unwrap();
        std::fs::write(
            f.area.directory.join("capacity-crash-seed.stderr.log"),
            stderr,
        )
        .unwrap();
        let fact: serde_json::Value = serde_json::from_slice(
            &std::fs::read(f.area.directory.join("capacity-crash-ready.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(fact["reserved"], reserved);
        // Test gate cannot apply to the new original source after restart.
        if mode == "waiting-new" {
            std::fs::remove_file(f.area.directory.join("capacity-hold-new")).unwrap();
        }
        f.owner = Some(NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            f.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        ));
        f.node = Some(f.owner.as_ref().unwrap().ready().await.unwrap());
        let node = f.node.as_ref().unwrap();
        let key = SigningKey::from_slice(&[71; 32]).unwrap();
        let client_id = fact["clientId"].as_str().unwrap().to_owned();
        let socket = authenticate_socket(
            valid_management_socket(&f.area, node.listen_addr()).await,
            &key,
            &client_id,
            node_public_key(node),
        )
        .await;
        f.peers.push(R3WssPeer {
            client_id,
            key,
            socket,
        });
        let selection = &fact["selection"];
        f.selection = Some(SourceLaunchSelection {
            source: crate::domain::EntityId::decode(selection["sourceId"].as_str().unwrap())
                .unwrap(),
            harness: crate::domain::EntityId::decode(selection["harnessId"].as_str().unwrap())
                .unwrap(),
            profile: crate::domain::native_catalog::ProfileKey::try_new(
                crate::domain::EntityId::decode(selection["profileId"].as_str().unwrap()).unwrap(),
                selection["profileRevision"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap(),
            )
            .unwrap(),
            workspace: crate::domain::EntityId::decode(selection["workspaceId"].as_str().unwrap())
                .unwrap(),
            launch_path: selection["launchPath"].as_str().unwrap().to_owned(),
        });
        capacity_expect(&mut f, 233, 1, reserved).await;
        let pool = f
            .node
            .as_ref()
            .unwrap()
            .storage
            .as_ref()
            .unwrap()
            .pool_for_test()
            .clone();
        let old_open: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM node_native_source_instances WHERE lifecycle!='closed'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            old_open, 1,
            "crashed original source has no reliable necessary closed fact"
        );
        let original_instance: String =
            sqlx::query_scalar("SELECT instance_id FROM node_session_capacity WHERE released=0")
                .fetch_one(&pool)
                .await
                .unwrap();
        let original = capacity_crash_original_snapshot(&pool, &original_instance).await;
        assert_ne!(original["source"]["lifecycle"], "closed");
        if mode == "waiting-new" {
            let status = f
                .call(
                    0,
                    237,
                    "source.session.status",
                    serde_json::json!({
                        "sourceOperationId":fact["sourceOperationId"]
                    }),
                )
                .await;
            assert_eq!(status["ok"], true, "{status}");
            assert_ne!(status["result"]["outcome"], "succeeded", "{status}");
            assert!(status["result"]["response"].is_null(), "{status}");
        }
        let denied =
            capacity_source_operation(&mut f, 234, serde_json::json!({"kind":"create"})).await;
        assert_eq!(
            denied["ok"], false,
            "old source owner must not be replaced: {denied}"
        );
        assert_eq!(denied["error"]["code"], "read_failed", "{denied}");
        capacity_expect(&mut f, 238, 1, reserved).await;
        assert_eq!(
            capacity_method_count(&f, "session/new"),
            1,
            "unknown original source blocks actual new IO before owner replacement"
        );
        assert_eq!(capacity_method_count(&f, "session/load"), 0);
        assert_eq!(capacity_method_count(&f, "session/resume"), 0);
        assert_eq!(capacity_method_count(&f, "session/close"), 0);
        assert_eq!(
            capacity_crash_original_snapshot(&pool, &original_instance).await,
            original,
            "refusal cannot change original instance, runtime, owner or reservation"
        );
        let fresh = capacity_restart_fresh_explicit_source(&mut f).await;
        f.selection = Some(fresh);
        capacity_create(&mut f, 240).await;
        capacity_expect(&mut f, 236, 2, reserved).await;
        assert_eq!(
            capacity_crash_original_snapshot(&pool, &original_instance).await,
            original,
            "fresh explicit source cannot rewrite or release the unknown original source"
        );
        assert_eq!(capacity_method_count(&f, "session/load"), 0);
        assert_eq!(capacity_method_count(&f, "session/resume"), 0);
        assert_eq!(capacity_method_count(&f, "session/close"), 0);
        assert_eq!(
            capacity_method_count(&f, "session/new"),
            2,
            "restart must neither resend old Create nor reset capacity"
        );
    })
    .catch_unwind()
    .await;
    let closed = f.close().await;
    if result.is_err() || !closed {
        let preserved = f.area.directory.clone();
        std::mem::forget(f.area);
        eprintln!("CAPACITY_CRASH_FIXTURE_PRESERVED {preserved:?}");
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(
        closed,
        "restarted node and all current owned sources must join"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_abrupt_owned_node_restart_preserves_unconfirmed_original_slot() {
    capacity_crash_restart_case("created", 0).await;
    capacity_crash_restart_case("waiting-new", 1).await;
}
