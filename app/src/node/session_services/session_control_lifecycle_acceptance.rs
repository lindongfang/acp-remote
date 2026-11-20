// Session control lifecycle, source create, readonly replay, approvals & capacity reset L5 tests.
// Included under `crate::node::tests::r3_source_wss_development`.

fn sctl_selection_json(f: &R3WssCase) -> serde_json::Value {
    let sel = f.selection.as_ref().expect("installed selection");
    serde_json::json!({
        "sourceId": sel.source.encode(),
        "harnessId": sel.harness.encode(),
        "profileId": sel.profile.id().encode(),
        "profileRevision": sel.profile.revision().to_string(),
        "workspaceId": sel.workspace.encode(),
        "launchPath": sel.launch_path,
    })
}

fn sctl_pool(f: &R3WssCase) -> sqlx::SqlitePool {
    f.node
        .as_ref()
        .unwrap()
        .storage
        .as_ref()
        .unwrap()
        .pool_for_test()
        .clone()
}

async fn sctl_call_socket(
    peer: &mut R3WssPeer,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let request_id = Id128::new([marker; 16]);
    send_json(
        &mut peer.socket,
        &serde_json::json!({
            "v": 1,
            "type": "command",
            "requestId": request_id,
            "action": action,
            "payload": payload,
        }),
    )
    .await;
    let expected = request_id.to_base64url();
    loop {
        let frame = next_json(&mut peer.socket, Duration::from_secs(15)).await;
        if frame["type"] == "result" {
            assert_eq!(frame["requestId"], expected, "{frame}");
            return frame;
        }
    }
}

async fn sctl_upload(
    f: &mut R3WssCase,
    peer_idx: usize,
    base: u8,
    target_id: &str,
    purpose: &str,
    text: &str,
) -> (String, String) {
    let begin = sct_r3_call_peer(
        f,
        peer_idx,
        base,
        "input.upload.begin",
        serde_json::json!({ "targetId": target_id, "purpose": purpose }),
    )
    .await;
    assert_eq!(begin["ok"], true, "{begin}");
    let upload_str = begin["result"]["uploadId"].as_str().unwrap().to_owned();
    let upload = Id128::parse_base64url(&upload_str).unwrap();
    let body = serde_json::to_vec(&serde_json::json!([{"type": "text", "text": text}])).unwrap();
    let mut wire = vec![0; 32];
    wire[..4].copy_from_slice(b"ACPI");
    wire[4] = 1;
    wire[5] = 1;
    wire[8..24].copy_from_slice(upload.as_bytes());
    wire.extend_from_slice(&body);
    f.peers[peer_idx]
        .socket
        .send(tokio_tungstenite::tungstenite::Message::Binary(wire.into()))
        .await
        .unwrap();
    let _ = f.peers[peer_idx].socket.next().await.unwrap();
    let fin = sct_r3_call_peer(
        f,
        peer_idx,
        base + 1,
        "input.upload.finish",
        serde_json::json!({ "uploadId": upload_str, "totalBytes": body.len().to_string() }),
    )
    .await;
    assert_eq!(fin["ok"], true, "{fin}");
    let body_id = loop {
        let st = sct_r3_call_peer(
            f,
            peer_idx,
            base + 2,
            "input.upload.status",
            serde_json::json!({ "uploadId": upload_str }),
        )
        .await;
        if st["result"]["phase"] == "ready" {
            break st["result"]["bodyId"].as_str().unwrap().to_owned();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    (upload_str, body_id)
}

async fn sctl_upload_and_submit(
    f: &mut R3WssCase,
    peer_idx: usize,
    base: u8,
    target_id: &str,
    text: &str,
) -> String {
    let (_, body_id) = sctl_upload(f, peer_idx, base, target_id, "submit", text).await;
    let reg = sct_r3_call_peer(
        f,
        peer_idx,
        base + 3,
        "input.register",
        serde_json::json!({
            "targetId": target_id,
            "operation": { "kind": "submit", "bodyId": body_id }
        }),
    )
    .await;
    assert_eq!(reg["ok"], true, "{reg}");
    let reg_id = reg["result"]["registrationId"].as_str().unwrap().to_owned();
    let con = sct_r3_call_peer(
        f,
        peer_idx,
        base + 4,
        "input.consume",
        serde_json::json!({
            "targetId": target_id,
            "registrationId": reg_id,
            "bodyId": body_id
        }),
    )
    .await;
    assert_eq!(con["ok"], true, "{con}");
    assert_eq!(con["result"]["request"]["outcome"]["kind"], "queued");
    reg_id
}

async fn sctl_release_target(f: &mut R3WssCase, peer_idx: usize, base: u8, target_id: &str) {
    let reg = sct_r3_call_peer(
        f,
        peer_idx,
        base,
        "native.release.register",
        serde_json::json!({ "targetId": target_id }),
    )
    .await;
    assert_eq!(reg["ok"], true, "{reg}");
    let op = reg["result"]["releaseOperationId"].as_str().unwrap().to_owned();
    let rel = sct_r3_call_peer(
        f,
        peer_idx,
        base + 1,
        "native.release",
        serde_json::json!({ "releaseOperationId": op }),
    )
    .await;
    assert_eq!(rel["ok"], true, "{rel}");
    assert_eq!(rel["result"]["state"], "released", "{rel}");
}

async fn sctl_create_session(
    f: &mut R3WssCase,
    peer_idx: usize,
    base: u8,
    target: crate::domain::EntityId,
) -> String {
    let sel = sctl_selection_json(f);
    let reg = sct_r3_call_peer(
        f,
        peer_idx,
        base,
        "source.session.register",
        serde_json::json!({ "selection": sel, "operation": { "kind": "create" } }),
    )
    .await;
    assert_eq!(reg["ok"], true, "{reg}");
    let op = reg["result"]["sourceOperationId"].as_str().unwrap().to_owned();
    let exec = sct_r3_call_peer(
        f,
        peer_idx,
        base + 1,
        "source.session.execute",
        serde_json::json!({ "sourceOperationId": op }),
    )
    .await;
    assert_eq!(exec["ok"], true, "{exec}");
    assert_eq!(exec["result"]["outcome"], "succeeded", "{exec}");
    let native = exec["result"]["response"]["nativeSessionId"]
        .as_str()
        .unwrap()
        .to_owned();
    capacity_route_target(f, &native, target).await;
    native
}

async fn sctl_wait_confirmed_approval(f: &R3WssCase) -> (String, String) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let node = f.node.as_ref().unwrap();
        let approvals = node.session_services.as_ref().unwrap().approval.clone();
        let policy = node.application_policy().unwrap();
        let actor =
            policy_actor(&policy, &f.peers[1].key, &f.peers[1].client_id, Id128::new([29; 16]))
                .await;
        loop {
            let mut all = approvals.list(&actor).await.unwrap();
            if let Some(value) = all.pop()
                && value.status().request_save.commit
                    == crate::domain::approval_owner::CommitState::Confirmed
            {
                break (value.id().encode(), value.choices()[0].id().encode());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("confirmed approval")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_003_initial_control_and_cross_session_node_orthogonality() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            let t1 = r3_id(40);
            capacity_route_target(f, &s1, t1).await;

            let inspect_b = sct_r3_call_peer(
                f,
                1,
                21,
                "session.control.inspect",
                serde_json::json!({ "targetId": t1.encode() }),
            )
            .await;
            assert_eq!(inspect_b["result"]["controllerClientId"], f.peers[0].client_id);
            assert_eq!(inspect_b["result"]["controlEpoch"], "1");
            let init_audit: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM security_audit WHERE action='session.control.initial'",
            )
            .fetch_one(&sctl_pool(f))
            .await
            .unwrap();
            assert_eq!(init_audit, 1);

            for (m, action, payload) in [
                (22, "node.info", serde_json::json!({})),
                (23, "device.list", serde_json::json!({ "cursor": null })),
                (24, "catalog.inventory", serde_json::json!({})),
                (25, "session.capacity.inspect", serde_json::json!({})),
            ] {
                let res = sct_r3_call_peer(f, 1, m, action, payload).await;
                assert_eq!(res["ok"], true, "{action}: {res}");
            }
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let t2 = r3_id(41);
            let _s2 = sctl_create_session(f, 1, 26, t2).await;
            let inspect_s2 = sct_r3_call_peer(
                f,
                1,
                28,
                "session.control.inspect",
                serde_json::json!({ "targetId": t2.encode() }),
            )
            .await;
            assert_eq!(inspect_s2["result"]["controllerClientId"], f.peers[1].client_id);
            assert_eq!(inspect_s2["result"]["controlEpoch"], "1");
            let _ = sctl_upload_and_submit(f, 1, 30, &t2.encode(), "s2 prompt").await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_004_tree_inheritance_root_target_presence_and_unverified_rejected() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            std::fs::write(f.area.directory.join("runtime-emit-approval-tree"), s1.as_bytes()).unwrap();
            let disc = capacity_source_operation(f, 12, serde_json::json!({ "kind": "discover", "cursor": null })).await;
            assert_eq!(disc["result"]["outcome"], "succeeded");

            // Child inspected BEFORE root target exists -> rootTargetId == null; after root routed -> non-null.
            let child_t = r3_id(42);
            capacity_route_target(f, "runtime-child-leaf", child_t).await;
            tokio::time::timeout(Duration::from_secs(10), async {
                let registry = f
                    .node
                    .as_ref()
                    .unwrap()
                    .storage
                    .as_ref()
                    .unwrap()
                    .request_registry();
                loop {
                    if let Ok(resolved) = registry.resolve_control_root(child_t).await
                        && resolved.root.native_id() == s1.as_str()
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("child_t parent link persisted");
            let child_no_root = sct_r3_call_peer(f, 1, 13, "session.control.inspect", serde_json::json!({ "targetId": child_t.encode() })).await;
            assert_eq!(child_no_root["result"]["controllerClientId"], f.peers[0].client_id);
            assert_eq!(child_no_root["result"]["rootTargetId"], serde_json::Value::Null);

            let root_t = r3_id(40);
            capacity_route_target(f, &s1, root_t).await;
            let child_with_root = sct_r3_call_peer(f, 1, 14, "session.control.inspect", serde_json::json!({ "targetId": child_t.encode() })).await;
            assert_eq!(child_with_root["result"]["rootTargetId"], root_t.encode());

            // Seed unverified root_verified=0 target & broken child pointing to unverified native root.
            let pool = sctl_pool(f);
            let (source_id, inst_id, rt_ep): (String, String, String) =
                sqlx::query_as("SELECT source_id,instance_id,runtime_epoch FROM node_session_capacity LIMIT 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let unverified_t = r3_id(43);
            sqlx::query("INSERT INTO product_targets VALUES(?,?,'unverified-native',NULL)")
                .bind(unverified_t.encode())
                .bind(&source_id)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO node_session_capacity VALUES(?,?,'unverified-native',?,?,NULL,0,0,0)")
                .bind(r3_id(91).encode())
                .bind(&source_id)
                .bind(&inst_id)
                .bind(&rt_ep)
                .execute(&pool)
                .await
                .unwrap();
            let broken_child_t = r3_id(44);
            sqlx::query("INSERT INTO product_targets VALUES(?,?,'broken-child-native',NULL)")
                .bind(broken_child_t.encode())
                .bind(&source_id)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO node_session_parents(source_id,child_native_id,parent_native_id,observed_instance,runtime_epoch,connection_epoch,sequence,receipt,fingerprint,observed_at_ms) SELECT source_id,'broken-child-native','unverified-native',observed_instance,runtime_epoch,connection_epoch,X'0000000000000002',X'0000000000000002',zeroblob(32),observed_at_ms FROM node_session_parents LIMIT 1")
                .execute(&pool)
                .await
                .unwrap();
            for (m, tid) in [(15, unverified_t), (16, broken_child_t)] {
                let res = sct_r3_call_peer(f, 0, m, "session.control.inspect", serde_json::json!({ "targetId": tid.encode() })).await;
                assert_eq!(res["ok"], false, "{res}");
                assert_eq!(res["error"]["code"], "session_tree_unverified", "{res}");
            }
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_008_and_011_readonly_replay_no_auto_claim_and_dispatch_vs_edit_race() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            let t1 = r3_id(40);
            capacity_route_target(f, &s1, t1).await;

            std::fs::write(f.area.directory.join("capacity-hold-prompt"), b"hold").unwrap();
            let _q1 = sctl_upload_and_submit(f, 0, 15, &t1.encode(), "prompt-1").await;
            tokio::time::timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("capacity-entered-prompt").is_file() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let _q2 = sctl_upload_and_submit(f, 0, 22, &t1.encode(), "prompt-2-orig").await;
            let queue = sct_r3_call_peer(f, 0, 28, "input.queue", serde_json::json!({ "targetId": t1.encode() })).await;
            let q2_id = queue["result"]["waiting"][0]["itemId"].as_str().unwrap().to_owned();
            let (_, body_id) = sctl_upload(f, 0, 30, &t1.encode(), "edit", "prompt-2-edited").await;
            let edit_reg = sct_r3_call_peer(f, 0, 34, "input.register", serde_json::json!({
                "targetId": t1.encode(),
                "operation": { "kind": "edit", "itemId": q2_id, "expectedRevision": "1", "bodyId": body_id }
            })).await;
            let edit_con = sct_r3_call_peer(f, 0, 35, "input.consume", serde_json::json!({
                "targetId": t1.encode(), "registrationId": edit_reg["result"]["registrationId"], "bodyId": body_id
            })).await;
            assert_eq!(edit_con["result"]["request"]["outcome"]["kind"], "edited");
            if let Some(active) = queue["result"]["active"].as_object() {
                let act_reg = sct_r3_call_peer(f, 0, 36, "input.register", serde_json::json!({
                    "targetId": t1.encode(),
                    "operation": { "kind": "edit", "itemId": active["itemId"], "expectedRevision": active["revision"], "bodyId": body_id }
                })).await;
                let act_con = sct_r3_call_peer(f, 0, 37, "input.consume", serde_json::json!({
                    "targetId": t1.encode(), "registrationId": act_reg["result"]["registrationId"], "bodyId": body_id
                })).await;
                assert_eq!(act_con["result"]["request"]["outcome"]["reason"], "dispatch_started");
            }
            std::fs::remove_file(f.area.directory.join("capacity-hold-prompt")).unwrap();
            std::fs::write(f.area.directory.join("capacity-release-prompt"), b"go").unwrap();
            tokio::time::sleep(Duration::from_millis(150)).await;

            sctl_release_target(f, 0, 40, &t1.encode()).await;
            let before = sct_r3_call_peer(f, 1, 42, "session.control.inspect", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(before["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(before["result"]["controlEpoch"], "2");
            let replayed = sct_r3_call_peer(f, 1, 43, "view.open_source", serde_json::json!({
                "selection": sctl_selection_json(f),
                "nativeSessionId": s1,
                "reason": "explicit_refresh"
            })).await;
            assert_eq!(replayed["ok"], true, "{replayed}");
            let after = sct_r3_call_peer(f, 1, 44, "session.control.inspect", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(after["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(after["result"]["controlEpoch"], "2");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_012_and_013_approval_readonly_reads_takeover_and_managed_delivery() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            let t1 = r3_id(40);
            capacity_route_target(f, &s1, t1).await;
            capacity_route_target(f, "runtime-child-leaf", r3_id(41)).await;
            std::fs::write(f.area.directory.join("runtime-emit-approval-tree"), s1.as_bytes()).unwrap();
            let _ = capacity_source_operation(f, 15, serde_json::json!({ "kind": "discover", "cursor": null })).await;
            let (approval_id, choice_id) = sctl_wait_confirmed_approval(f).await;

            assert_eq!(sct_r3_call_peer(f, 1, 16, "approval.list", serde_json::json!({})).await["ok"], true);
            assert_eq!(sct_r3_call_peer(f, 1, 17, "approval.read", serde_json::json!({ "approvalId": approval_id })).await["ok"], true);
            assert_eq!(sct_r3_call_peer(f, 1, 18, "approval.status", serde_json::json!({ "approvalId": approval_id })).await["ok"], true);

            let deny_b = sct_r3_call_peer(f, 1, 19, "approval.register", serde_json::json!({ "approvalId": approval_id, "choiceId": choice_id })).await;
            assert_eq!(deny_b["error"]["code"], "session_not_controlled");
            assert_eq!(
                sct_r3_call_peer(f, 1, 20, "session.control.takeover", serde_json::json!({ "targetId": t1.encode() })).await["result"]["controlEpoch"],
                "2"
            );
            let reg_b = sct_r3_call_peer(f, 1, 21, "approval.register", serde_json::json!({
                "approvalId": approval_id, "choiceId": choice_id, "expectedControlEpoch": "2"
            })).await;
            let dec_b = sct_r3_call_peer(f, 1, 22, "approval.decide", serde_json::json!({
                "approvalRegistrationId": reg_b["result"]["approvalRegistrationId"], "expectedControlEpoch": "2"
            })).await;
            assert_eq!(dec_b["ok"], true, "{dec_b}");
            assert_eq!(dec_b["result"]["state"], "accepted");

            let rel = sct_r3_call_peer(f, 1, 23, "session.control.release", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(rel["result"]["controlEpoch"], "3");
            tokio::time::timeout(Duration::from_secs(8), async {
                while !f.area.directory.join("runtime-approval-selected").is_file() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("managed approval delivery after control release");

            let st_a = sct_r3_call_peer(f, 0, 24, "approval.status", serde_json::json!({ "approvalId": approval_id })).await;
            let st_b = sct_r3_call_peer(f, 1, 25, "approval.status", serde_json::json!({ "approvalId": approval_id })).await;
            assert_eq!(st_a["result"]["status"]["decision"], st_b["result"]["status"]["decision"]);
            let late_a = sct_r3_call_peer(f, 0, 26, "approval.register", serde_json::json!({ "approvalId": approval_id, "choiceId": choice_id })).await;
            assert_eq!(late_a["ok"], false, "{late_a}");
            let post_rel_reg = sct_r3_call_peer(f, 1, 27, "input.register", serde_json::json!({ "targetId": t1.encode(), "operation": { "kind": "resume" } })).await;
            assert_eq!(post_rel_reg["error"]["code"], "session_not_controlled", "{post_rel_reg}");
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_012_concurrent_approval_race() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            let t1 = r3_id(40);
            capacity_route_target(f, &s1, t1).await;
            capacity_route_target(f, "runtime-child-leaf", r3_id(41)).await;
            std::fs::write(f.area.directory.join("runtime-emit-approval-tree"), s1.as_bytes()).unwrap();
            let _ = capacity_source_operation(f, 15, serde_json::json!({ "kind": "discover", "cursor": null })).await;
            let (approval_id, choice_id) = sctl_wait_confirmed_approval(f).await;
            let reg_a = sct_r3_call_peer(f, 0, 50, "approval.register", serde_json::json!({
                "approvalId": approval_id, "choiceId": choice_id, "expectedControlEpoch": "1"
            })).await;
            assert_eq!(reg_a["ok"], true, "{reg_a}");
            let reg_a_id = reg_a["result"]["approvalRegistrationId"].as_str().unwrap().to_owned();

            let (p0, rest) = f.peers.split_at_mut(1);
            let p1 = &mut rest[0];
            let (outcome_a, outcome_b) = tokio::join!(
                sctl_call_socket(&mut p0[0], 51, "approval.decide", serde_json::json!({ "approvalRegistrationId": reg_a_id })),
                async {
                    let take = sctl_call_socket(p1, 52, "session.control.takeover", serde_json::json!({ "targetId": t1.encode() })).await;
                    assert_eq!(take["ok"], true);
                    let reg_b = sctl_call_socket(p1, 53, "approval.register", serde_json::json!({ "approvalId": approval_id, "choiceId": choice_id })).await;
                    if reg_b["ok"] == true {
                        Some(sctl_call_socket(p1, 54, "approval.decide", serde_json::json!({ "approvalRegistrationId": reg_b["result"]["approvalRegistrationId"] })).await)
                    } else {
                        None
                    }
                }
            );
            let a_won = outcome_a["ok"] == true && outcome_a["result"]["state"] == "accepted";
            let b_won = outcome_b.as_ref().is_some_and(|v| v["ok"] == true && v["result"]["state"] == "accepted");
            assert!(!(a_won && b_won), "at most one decision may accept: a={outcome_a} b={outcome_b:?}");
            let st_a = sct_r3_call_peer(f, 0, 55, "approval.status", serde_json::json!({ "approvalId": approval_id })).await;
            let st_b = sct_r3_call_peer(f, 1, 56, "approval.status", serde_json::json!({ "approvalId": approval_id })).await;
            assert_eq!(st_a["result"]["status"]["decision"], st_b["result"]["status"]["decision"]);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sctl_015_capacity_release_aba_idle_expiry_monotonic_epoch_and_restart_reset() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            sct_install_capacity_peer(f, "source-create-replay-state").await;
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let s1 = capacity_create(f, 10).await;
            let t1 = r3_id(40);
            capacity_route_target(f, &s1, t1).await;

            // A registers step 1 at epoch 1 then pauses; B executes global `native.release` -> epoch 2.
            let (_, pre_body) = sctl_upload(f, 0, 12, &t1.encode(), "submit", "aba-pre").await;
            let pre_reg = sct_r3_call_peer(f, 0, 15, "input.register", serde_json::json!({
                "targetId": t1.encode(), "operation": { "kind": "submit", "bodyId": pre_body }, "expectedControlEpoch": "1"
            })).await;
            let pre_reg_id = pre_reg["result"]["registrationId"].as_str().unwrap().to_owned();

            sctl_release_target(f, 1, 16, &t1.encode()).await;
            let after_rel = sct_r3_call_peer(f, 1, 18, "session.control.inspect", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(after_rel["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(after_rel["result"]["controlEpoch"], "2");

            // A retakes -> epoch 3; pre-release reg & expectedControlEpoch="1" both blocked (no ABA).
            let retake = sct_r3_call_peer(f, 0, 19, "session.control.takeover", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(retake["result"]["controlEpoch"], "3");
            let aba_con = sct_r3_call_peer(f, 0, 20, "input.consume", serde_json::json!({
                "targetId": t1.encode(), "registrationId": pre_reg_id, "bodyId": pre_body
            })).await;
            assert_eq!(aba_con["error"]["code"], "session_not_controlled");
            let aba_exp = sct_r3_call_peer(f, 0, 21, "input.register", serde_json::json!({
                "targetId": t1.encode(), "operation": { "kind": "resume" }, "expectedControlEpoch": "1"
            })).await;
            assert_eq!(aba_exp["error"]["code"], "session_not_controlled");

            // Coordinator 30m clock expiry on unloaded pre-takeover clears -> epoch 2.
            let t_unloaded = r3_id(44);
            sqlx::query("INSERT INTO product_targets VALUES(?,?,'unloaded-native',NULL)")
                .bind(t_unloaded.encode())
                .bind(f.selection.as_ref().unwrap().source.encode())
                .execute(&sctl_pool(f))
                .await
                .unwrap();
            let take_u = sct_r3_call_peer(f, 0, 22, "session.control.takeover", serde_json::json!({ "targetId": t_unloaded.encode() })).await;
            assert_eq!(take_u["result"]["controlEpoch"], "1");
            let coord = f.node.as_ref().unwrap().storage.as_ref().unwrap().session_control();
            coord.advance_clock_for_test(Duration::from_secs(1801));
            assert!(coord.sweep_expired() >= 1);
            let expired = sct_r3_call_peer(f, 0, 23, "session.control.inspect", serde_json::json!({ "targetId": t_unloaded.encode() })).await;
            assert_eq!(expired["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(expired["result"]["controlEpoch"], "2");

            // Node restart resets controllerClientId=null and controlEpoch="0".
            let key_a = f.peers[0].key.clone();
            let id_a = f.peers[0].client_id.clone();
            f.peers.clear();
            let old_node = f.node.take().unwrap();
            old_node.shutdown(None).await.unwrap();
            if let Some(mut owner) = f.owner.take() {
                owner.disarm();
            }
            f.owner = Some(NodeProcessOwner::begin(
                &tokio::runtime::Handle::current(),
                f.area.config(free_address()),
                Arc::new(FakeProtector::default()),
            ));
            f.node = Some(f.owner.as_ref().unwrap().ready().await.unwrap());
            let node = f.node.as_ref().unwrap();
            let sock = authenticate_socket(
                valid_management_socket(&f.area, node.listen_addr()).await,
                &key_a,
                &id_a,
                node_public_key(node),
            )
            .await;
            f.peers.push(R3WssPeer { client_id: id_a, key: key_a, socket: sock });
            let post_restart = sct_r3_call_peer(f, 0, 24, "session.control.inspect", serde_json::json!({ "targetId": t1.encode() })).await;
            assert_eq!(post_restart["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(post_restart["result"]["controlEpoch"], "0");
        })
    })
    .await;
}
