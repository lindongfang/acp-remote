// Included in node::tests::r3_source_wss_development after its existing fixture.
// Only old-HEAD Rust APIs are used: new behavior is exercised through real WSS.

fn capacity_events(f: &R3WssCase) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir(&f.area.directory).expect("owned fixture directory") {
        let entry = entry.expect("owned fixture entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("capacity-peer-")
            && std::path::Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        {
            let bytes = std::fs::read(entry.path()).expect("body-free peer events");
            // The peer may currently be appending its next event. Only newline
            // terminated records are committed observations; malformed complete
            // records still fail rather than being silently discarded.
            for line in bytes.split_inclusive(|byte| *byte == b'\n') {
                if line.last() == Some(&b'\n') {
                    result.push(serde_json::from_slice(line).expect("complete peer event JSON"));
                }
            }
        }
    }
    result
}

fn capacity_method_count(f: &R3WssCase, method: &str) -> usize {
    let observed = capacity_events(f)
        .iter()
        .filter(|event| event["method"] == method)
        .count();
    if observed > 0 {
        return observed;
    }
    let fixture_count = match method {
        "session/new" => Some("source-create-count"),
        "session/load" => Some("source-load-count"),
        "session/resume" => Some("source-resume-count"),
        "session/list" => Some("source-list-count"),
        _ => None,
    };
    fixture_count
        .and_then(|name| std::fs::read_to_string(f.area.directory.join(name)).ok())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

async fn capacity_inspect(f: &mut R3WssCase, marker: u8) -> serde_json::Value {
    let response = f
        .call(0, marker, "session.capacity.inspect", serde_json::json!({}))
        .await;
    assert_eq!(
        response["ok"], true,
        "capacity action must exist: {response}"
    );
    assert_eq!(response["result"]["limit"], 50, "{response}");
    assert_eq!(response["result"]["automaticRecycling"], true, "{response}");
    response["result"].clone()
}

async fn capacity_expect(f: &mut R3WssCase, marker: u8, occupied: u64, reserved: u64) {
    let actual = capacity_inspect(f, marker).await;
    assert_eq!(actual["occupied"], occupied, "{actual}");
    assert_eq!(actual["reserved"], reserved, "{actual}");
}

async fn capacity_source_operation(
    f: &mut R3WssCase,
    marker: u8,
    operation: serde_json::Value,
) -> serde_json::Value {
    let registered = f
        .call(
            0,
            marker,
            "source.session.register",
            serde_json::json!({"selection":f.selection_json(),"operation":operation}),
        )
        .await;
    assert_eq!(registered["ok"], true, "{registered}");
    f.call(
        0,
        marker + 1,
        "source.session.execute",
        serde_json::json!({"sourceOperationId":registered["result"]["sourceOperationId"]}),
    )
    .await
}

async fn capacity_create(f: &mut R3WssCase, marker: u8) -> String {
    let response = capacity_source_operation(f, marker, serde_json::json!({"kind":"create"})).await;
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"]["outcome"], "succeeded", "{response}");
    response["result"]["response"]["nativeSessionId"]
        .as_str()
        .expect("native identity from paired raw new response")
        .to_owned()
}

// Product target is only routing metadata. The root proof comes exclusively
// from capacity_create's actual standard new exchange, never this SQL row.
async fn capacity_route_target(f: &mut R3WssCase, native: &str, target: crate::domain::EntityId) {
    let node = f.node.as_ref().expect("live node");
    let pool = node.storage.as_ref().expect("real SQLite").pool_for_test();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
        .bind(target.encode())
        .bind(r3_id(1).encode())
        .bind(native)
        .bind(r3_id(9).encode())
        .execute(pool)
        .await
        .expect("route target for observed original native ID");
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
    let endpoint = observed
        .await
        .expect("original authenticated catalog actor");
    let client = endpoint.client();
    let catalog = &node.session_services.as_ref().unwrap().catalog;
    let actor = client.context.actor();
    let selection = f.selection.as_ref().unwrap();
    let observation = catalog
        .observe_workspace(
            actor,
            target,
            selection.source,
            selection.workspace,
            &r3_deadline(),
        )
        .await
        .expect("actual catalog observation");
    let registration = catalog
        .register(
            actor,
            observation
                .bind(
                    selection.harness,
                    selection.profile,
                    selection.launch_path.clone(),
                )
                .expect("bind original configured source"),
            &r3_deadline(),
        )
        .await
        .expect("catalog registration");
    catalog
        .apply(actor, registration, &r3_deadline())
        .await
        .expect("catalog apply");
    f.peers.push(R3WssPeer {
        client_id: f.peers[0].client_id.clone(),
        key: f.peers[0].key.clone(),
        socket,
    });
}

async fn capacity_register_release(
    f: &mut R3WssCase,
    marker: u8,
    target: crate::domain::EntityId,
) -> String {
    let response = f
        .call(
            0,
            marker,
            "native.release.register",
            serde_json::json!({"targetId":target.encode()}),
        )
        .await;
    assert_eq!(response["ok"], true, "{response}");
    response["result"]["releaseOperationId"]
        .as_str()
        .expect("original release operation")
        .to_owned()
}

async fn capacity_submit_execution_target(f: &mut R3WssCase, target: crate::domain::EntityId) {
    use crate::{
        management_ws::input_upload::InputData,
        node::{input_upload::UploadPurpose, queue_management::NodeQueueMutation},
    };
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
    let upload_owner = node.input_upload_owner(&client).unwrap();
    let upload = upload_owner
        .begin(&client, target, UploadPurpose::Submit)
        .await
        .unwrap()
        .upload();
    let raw = br#"[{"type":"text","text":"capacity owned synthetic execution"}]"#;
    let mut frame = vec![0; 32];
    frame[..4].copy_from_slice(b"ACPI");
    frame[4] = 1;
    frame[5] = 1;
    frame[8..24].copy_from_slice(upload.as_bytes());
    frame.extend_from_slice(raw);
    upload_owner
        .accept_data(&client, InputData::decode_client(&frame).unwrap())
        .await
        .unwrap();
    upload_owner
        .finish(&client, upload, raw.len() as u64)
        .await
        .unwrap();
    let body = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(body) = upload_owner.status(&client, upload).await.unwrap().body() {
                break body;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual upload body ready");
    let queue = node.queue_management().unwrap();
    let registered = queue
        .register(&client, target, NodeQueueMutation::Submit { body })
        .await
        .expect("real execution target registration");
    queue
        .begin_consume(&client, target, registered.id(), Some(body))
        .await
        .expect("real execution target consume")
        .wait()
        .await
        .expect("accepted target exists before ensure_target");
    f.peers.push(R3WssPeer {
        client_id: f.peers[0].client_id.clone(),
        key: f.peers[0].key.clone(),
        socket,
    });
}

async fn capacity_release(f: &mut R3WssCase, marker: u8, operation: &str) -> serde_json::Value {
    f.call(
        0,
        marker,
        "native.release",
        serde_json::json!({"releaseOperationId":operation}),
    )
    .await
}

struct CapacityGateRelease(std::path::PathBuf);
impl Drop for CapacityGateRelease {
    fn drop(&mut self) {
        for gate in ["new", "replay", "close", "prompt"] {
            let _ = std::fs::write(self.0.join(format!("capacity-release-{gate}")), b"release");
        }
        let _ = std::fs::remove_file(self.0.join("capacity-close-error"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_default_inspect_discover_has_no_root() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            capacity_expect(f, 150, 0, 0).await;
            let discover = capacity_source_operation(
                f,
                151,
                serde_json::json!({"kind":"discover","cursor":null}),
            )
            .await;
            assert_eq!(discover["ok"], true, "{discover}");
            assert_eq!(discover["result"]["outcome"], "succeeded", "{discover}");
            capacity_expect(f, 153, 0, 0).await;
            assert_eq!(capacity_method_count(f, "initialize"), 1);
            assert_eq!(capacity_method_count(f, "session/list"), 1);
            assert_eq!(capacity_method_count(f, "session/new"), 0);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_fifty_real_creates_reject_fifty_first_without_write() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let mut roots = std::collections::BTreeSet::new();
            for i in 0..50_u8 {
                let native = capacity_create(f, 1 + i * 2).await;
                assert!(
                    roots.insert(native),
                    "each actual new must be a distinct root"
                );
            }
            // This assertion independently fails on old HEAD even without inspect:
            // the old implementation really writes and succeeds on its 51st new.
            let rejected =
                capacity_source_operation(f, 110, serde_json::json!({"kind":"create"})).await;
            assert!(
                rejected["ok"] == false || rejected["result"]["outcome"] != "succeeded",
                "51st actual new must fail admission: {rejected}"
            );
            assert!(
                rejected.to_string().contains("session_capacity_full"),
                "stable full-capacity reason required: {rejected}"
            );
            assert_eq!(capacity_method_count(f, "session/new"), 50);
            for event in capacity_events(f)
                .iter()
                .filter(|e| e["method"] == "session/new")
            {
                assert!(
                    !event["keys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|k| k == "sessionId")
                );
            }
            capacity_expect(f, 112, 50, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_unknown_external_tree_rejected_before_replay() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            // Routing and an accepted input are not proof of an independent root.
            capacity_route_target(f, "unknown-external-root", r3_id(31)).await;
            capacity_submit_execution_target(f, r3_id(31)).await;
            let execution = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(31))
                .await;
            assert!(
                matches!(
                    execution,
                    Err(crate::domain::native_dispatch::DispatchError::SessionTreeUnverified)
                ),
                "actual execution loading must reject an unverified root: {execution:?}"
            );
            let response = f
                .call(
                    0,
                    160,
                    "view.open_source",
                    serde_json::json!({
                        "selection":f.selection_json(),
                        "nativeSessionId":"unknown-external-root",
                        "reason":"first_open"
                    }),
                )
                .await;
            if response["ok"] == true {
                assert_eq!(
                    response["result"]["status"]["mode"], "current_only",
                    "{response}"
                );
                assert_eq!(
                    response["result"]["status"]["historyIncomplete"], true,
                    "{response}"
                );
                let closed = f
                    .call(
                        0,
                        162,
                        "view.close",
                        serde_json::json!({"streamId":response["result"]["streamId"]}),
                    )
                    .await;
                assert_eq!(closed["ok"], true, "{closed}");
            } else {
                assert_eq!(
                    response["error"]["code"], "session_tree_unverified",
                    "{response}"
                );
            }
            assert_eq!(capacity_method_count(f, "session/load"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            capacity_expect(f, 161, 0, 0).await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_create_only_release_uses_original_source_without_resume() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 170).await;
            capacity_expect(f, 172, 1, 0).await;
            capacity_route_target(f, &native, r3_id(20)).await;
            let operation = capacity_register_release(f, 173, r3_id(20)).await;
            let released = capacity_release(f, 174, &operation).await;
            assert_eq!(released["ok"], true, "{released}");
            assert_eq!(released["result"]["state"], "released", "{released}");
            capacity_expect(f, 175, 0, 0).await;
            let repeated = capacity_release(f, 176, &operation).await;
            assert_eq!(repeated["result"], released["result"]);
            capacity_expect(f, 177, 0, 0).await;
            let events = capacity_events(f);
            let created = events
                .iter()
                .find(|e| e["method"] == "session/new")
                .unwrap();
            let closed = events
                .iter()
                .find(|e| e["method"] == "session/close")
                .unwrap();
            assert_eq!(closed["native"], native);
            assert_eq!(
                closed["pid"], created["pid"],
                "close must use original source"
            );
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_necessary_outer_release_failure_retains_slot_until_retry() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 180).await;
            capacity_route_target(f, &native, r3_id(21)).await;
            let operation = capacity_register_release(f, 182, r3_id(21)).await;
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test()
                .clone();
            sqlx::query(
                "CREATE TRIGGER capacity_release_result_fault \
                 BEFORE UPDATE OF phase,error_code ON node_native_release_operations \
                 WHEN OLD.phase='releasing' \
                 BEGIN SELECT RAISE(ABORT,'capacity release result fixture'); END",
            )
            .execute(&pool)
            .await
            .expect("real necessary result failure");
            let observed = AssertUnwindSafe(async {
                let failed = capacity_release(f, 183, &operation).await;
                let held = f
                    .call(0, 184, "session.capacity.inspect", serde_json::json!({}))
                    .await;
                (failed, held, capacity_method_count(f, "session/close"))
            })
            .catch_unwind()
            .await;
            // Remove the actual SQL fault before any expected old-version
            // assertion can unwind into Node shutdown.
            sqlx::query("DROP TRIGGER capacity_release_result_fault")
                .execute(&pool)
                .await
                .expect("remove owned trigger on success or assertion unwind");
            let (failed, snapshot, close_count) = match observed {
                Ok(value) => value,
                Err(error) => std::panic::resume_unwind(error),
            };
            assert_eq!(failed["ok"], false, "{failed}");
            assert_eq!(
                failed["error"]["code"], "necessary_storage_unavailable",
                "{failed}"
            );
            assert_eq!(close_count, 1);
            assert_eq!(snapshot["ok"], true, "{snapshot}");
            let held = &snapshot["result"];
            assert_eq!(
                held["occupied"], 1,
                "physical close alone must not return quota"
            );
            assert_eq!(held["closedPending"], 1, "{held}");
            let retried = capacity_release(f, 185, &operation).await;
            assert_eq!(retried["ok"], true, "{retried}");
            assert_eq!(retried["result"]["state"], "released", "{retried}");
            capacity_expect(f, 186, 0, 0).await;
            let repeated = capacity_release(f, 187, &operation).await;
            assert_eq!(repeated["result"], retried["result"]);
            assert_eq!(capacity_method_count(f, "session/close"), 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_create_reserves_before_raw_identity_arrives() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let release = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-new"), b"hold").unwrap();
            let registered = f.call(
                0, 190, "source.session.register",
                serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
            ).await;
            assert_eq!(registered["ok"], true, "{registered}");
            send_json(
                &mut f.peers[0].socket,
                &serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([191;16]),
                    "action":"source.session.execute",
                    "payload":{"sourceOperationId":registered["result"]["sourceOperationId"]}
                }),
            )
            .await;
            timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("capacity-entered-new").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("peer received original new but has not replied");
            let snapshot = f
                .call(1, 192, "session.capacity.inspect", serde_json::json!({}))
                .await;
            assert_eq!(snapshot["ok"], true, "{snapshot}");
            assert_eq!(snapshot["result"]["occupied"], 1, "{snapshot}");
            assert_eq!(snapshot["result"]["reserved"], 1, "{snapshot}");
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            drop(release);
            let completed = next_json(&mut f.peers[0].socket, Duration::from_secs(15)).await;
            assert_eq!(completed["requestId"], Id128::new([191; 16]).to_base64url());
            assert_eq!(completed["result"]["outcome"], "succeeded", "{completed}");
            capacity_expect(f, 193, 1, 0).await;
        })
    })
    .await;
}

async fn capacity_replay_reallocation(scenario: &str, expected_method: &'static str) {
    r3_wss_case(scenario, false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 200).await;
            capacity_route_target(f, &native, r3_id(22)).await;
            let original_release = capacity_register_release(f, 202, r3_id(22)).await;
            let closed = capacity_release(f, 203, &original_release).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            capacity_expect(f, 204, 0, 0).await;
            let opened = f
                .call(
                    0,
                    205,
                    "view.open_source",
                    serde_json::json!({
                        "selection":f.selection_json(),"nativeSessionId":native,
                        "reason":"first_open"
                    }),
                )
                .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
            let (_, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream).await;
            let active = if rebound {
                let rebound = f
                    .call(
                        0,
                        206,
                        "view.rebind",
                        serde_json::json!({"streamId":stream.to_base64url()}),
                    )
                    .await;
                assert_eq!(rebound["ok"], true, "{rebound}");
                let next = Id128::parse_base64url(rebound["result"]["streamId"].as_str().unwrap())
                    .unwrap();
                let (_, rebound_again) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, next).await;
                assert!(!rebound_again);
                next
            } else {
                stream
            };
            let closed_view = f
                .call(
                    0,
                    207,
                    "view.close",
                    serde_json::json!({"streamId":active.to_base64url()}),
                )
                .await;
            assert_eq!(closed_view["ok"], true, "{closed_view}");
            capacity_expect(f, 208, 1, 0).await;
            assert_eq!(capacity_method_count(f, expected_method), 1);
            // Closing a reader does not claim that the native tree was released.
            let release = capacity_register_release(f, 209, r3_id(22)).await;
            let final_close = capacity_release(f, 210, &release).await;
            assert_eq!(final_close["result"]["state"], "released", "{final_close}");
            capacity_expect(f, 211, 0, 0).await;
            assert_eq!(
                capacity_method_count(f, expected_method),
                1,
                "explicit release must not resume/load just to close"
            );
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_v1_source_load_reallocates_after_explicit_release() {
    capacity_replay_reallocation("normal", "session/load").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_v2_source_resume_reallocates_after_explicit_release() {
    capacity_replay_reallocation("source-create-replay-state", "session/resume").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_execution_reallocates_after_explicit_release() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 220).await;
            capacity_route_target(f, &native, r3_id(23)).await;
            let operation = capacity_register_release(f, 222, r3_id(23)).await;
            let closed = capacity_release(f, 223, &operation).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            capacity_expect(f, 224, 0, 0).await;
            let release_prompt = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-prompt"), b"hold").unwrap();
            capacity_submit_execution_target(f, r3_id(23)).await;
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(23))
                .await
                .expect("actual execution admission");
            capacity_expect(f, 225, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/resume"), 1);
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(23))
                .await
                .expect("same owner reuse");
            capacity_expect(f, 226, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/resume"), 1);
            drop(release_prompt);
            let operation = capacity_register_release(f, 227, r3_id(23)).await;
            let closed = capacity_release(f, 228, &operation).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            capacity_expect(f, 229, 0, 0).await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_two_authenticated_creates_compete_for_last_slot() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            for i in 0..49_u8 {
                capacity_create(f, 1 + 2*i).await;
            }
            let mut operations = Vec::new();
            for peer in 0..2 {
                let response = f.call(
                    peer, 120 + 2*u8::try_from(peer).expect("bounded test peer index"), "source.session.register",
                    serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
                ).await;
                assert_eq!(response["ok"], true, "{response}");
                operations.push(response["result"]["sourceOperationId"].clone());
            }
            for (peer, operation) in operations.iter().enumerate() {
                send_json(&mut f.peers[peer].socket, &serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([121+2*u8::try_from(peer).expect("bounded test peer index");16]),
                    "action":"source.session.execute","payload":{"sourceOperationId":operation}
                })).await;
            }
            let (left, right) = f.peers.split_at_mut(1);
            let (a,b) = tokio::join!(
                next_json(&mut left[0].socket, Duration::from_secs(15)),
                next_json(&mut right[0].socket, Duration::from_secs(15)),
            );
            let successful = [&a,&b].iter()
                .filter(|r| r["ok"] == true && r["result"]["outcome"] == "succeeded")
                .count();
            assert_eq!(successful, 1, "exactly one final admission: {a} {b}");
            let rejected = if a["result"]["outcome"] == "succeeded" { b } else { a };
            assert!(rejected.to_string().contains("session_capacity_full"), "{rejected}");
            assert_eq!(capacity_method_count(f, "session/new"), 50);
            capacity_expect(f, 124, 50, 0).await;
        })
    }).await;
}

async fn capacity_full_reopen_refusal(scenario: &str, wire_method: &'static str) {
    r3_wss_case(scenario, false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 130).await;
            capacity_route_target(f, &native, r3_id(24)).await;
            let operation = capacity_register_release(f, 132, r3_id(24)).await;
            let closed = capacity_release(f, 133, &operation).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            for i in 0..50_u8 {
                capacity_create(f, 1 + 2 * i).await;
            }
            capacity_expect(f, 134, 50, 0).await;
            capacity_submit_execution_target(f, r3_id(24)).await;
            let execution = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(24))
                .await;
            assert!(
                execution.is_err(),
                "full capacity must refuse execution reopen"
            );
            let replay = f
                .call(
                    0,
                    135,
                    "view.open_source",
                    serde_json::json!({
                        "selection":f.selection_json(),
                        "nativeSessionId":native,
                        "reason":"first_open"
                    }),
                )
                .await;
            assert_eq!(replay["ok"], false, "{replay}");
            assert_eq!(replay["error"]["code"], "session_capacity_full", "{replay}");
            assert_eq!(
                capacity_method_count(f, wire_method),
                0,
                "rejected source replay must not write protocol load/resume"
            );
            assert_eq!(
                capacity_method_count(f, "session/resume"),
                0,
                "rejected execution must not write protocol resume"
            );
            assert_eq!(
                capacity_method_count(f, "session/new"),
                51,
                "one previously closed root plus exactly50 current roots"
            );
            assert_eq!(
                capacity_method_count(f, "session/close"),
                1,
                "capacity pressure must not close unknown-activity roots"
            );
            capacity_expect(f, 136, 50, 0).await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_full_refuses_execution_and_v1_source_before_write() {
    capacity_full_reopen_refusal("normal", "session/load").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_full_refuses_execution_and_v2_source_before_write() {
    capacity_full_reopen_refusal("source-create-replay-state", "session/resume").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_native_close_error_keeps_both_roots_owned() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let cleanup = CapacityGateRelease(f.area.directory.clone());
            let native = capacity_create(f, 140).await;
            capacity_create(f, 142).await;
            capacity_route_target(f, &native, r3_id(25)).await;
            let operation = capacity_register_release(f, 144, r3_id(25)).await;
            std::fs::write(f.area.directory.join("capacity-close-error"), b"synthetic").unwrap();
            let failed = capacity_release(f, 145, &operation).await;
            assert_eq!(failed["ok"], false, "{failed}");
            capacity_expect(f, 146, 2, 0).await;
            let events = capacity_events(f);
            let close = events
                .iter()
                .filter(|e| e["method"] == "session/close")
                .collect::<Vec<_>>();
            assert_eq!(close.len(), 1);
            assert_eq!(close[0]["native"], native);
            drop(cleanup);
        })
    })
    .await;
}
