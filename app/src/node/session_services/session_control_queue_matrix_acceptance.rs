// Session control takeover & read-only isolation L5 queue/upload/audit matrix tests.
// Included under `crate::node::tests::queue_management_development`.

async fn sctm_open_client_seeded(
    f: &Case,
    seed: u8,
    display_name: &str,
) -> (String, SigningKey, ManagementSocket) {
    let node = f.node.as_ref().expect("live node");
    let key = SigningKey::from_slice(&[seed; 32]).expect("seeded signing key");
    let client_id = pair_browser(node, &key, display_name).await;
    let socket = authenticate_socket(
        valid_management_socket(&f.area, node.listen_addr()).await,
        &key,
        &client_id,
        node_public_key(node),
    )
    .await;
    (client_id, key, socket)
}

async fn sctm_register_and_consume_text(
    f: &mut Case,
    socket_idx: usize,
    marker: u8,
    target_entity: crate::domain::EntityId,
    text: &str,
) -> String {
    let raw = serde_json::to_vec(&serde_json::json!([{"type": "text", "text": text}])).unwrap();
    let body_id = sct_body_for_target(f, socket_idx, target_entity, UploadPurpose::Submit, &raw)
        .await
        .to_base64url();
    let target_id = target_entity.encode();
    let (reg, _) = sct_call_peer(
        &mut f.peers[socket_idx],
        marker,
        "input.register",
        serde_json::json!({
            "targetId": target_id,
            "operation": { "kind": "submit", "bodyId": body_id }
        }),
    )
    .await;
    assert_eq!(reg["ok"], true, "{reg}");
    let reg_id = reg["result"]["registrationId"]
        .as_str()
        .expect("registrationId")
        .to_owned();
    let (consumed, _) = sct_call_peer(
        &mut f.peers[socket_idx],
        marker.saturating_add(1),
        "input.consume",
        serde_json::json!({
            "targetId": target_id,
            "registrationId": reg_id,
            "bodyId": body_id
        }),
    )
    .await;
    assert_eq!(consumed["ok"], true, "{consumed}");
    assert_eq!(consumed["result"]["request"]["outcome"]["kind"], "queued");
    reg_id
}

async fn sctm_upload_via_wss(
    socket: &mut ManagementSocket,
    marker: u8,
    target_id: &str,
    raw: &[u8],
) -> (String, String) {
    let (begin, _) = sct_call_peer(
        socket,
        marker,
        "input.upload.begin",
        serde_json::json!({ "targetId": target_id, "purpose": "submit" }),
    )
    .await;
    assert_eq!(begin["ok"], true, "{begin}");
    let upload_str = begin["result"]["uploadId"]
        .as_str()
        .expect("uploadId")
        .to_owned();
    let upload = Id128::parse_base64url(&upload_str).expect("upload id128");
    send_binary(socket, frame(upload, 0, raw)).await;
    let ack = next_binary(socket, Duration::from_secs(5)).await;
    assert_eq!(&ack[..4], b"ACPI");
    let (finish, _) = sct_call_peer(
        socket,
        marker.saturating_add(1),
        "input.upload.finish",
        serde_json::json!({
            "uploadId": upload_str,
            "totalBytes": raw.len().to_string()
        }),
    )
    .await;
    assert_eq!(finish["ok"], true, "{finish}");
    let mut poll = 0_u8;
    let body_id = loop {
        let (status, _) = sct_call_peer(
            socket,
            marker.saturating_add(2).saturating_add(poll),
            "input.upload.status",
            serde_json::json!({ "uploadId": upload_str }),
        )
        .await;
        assert_eq!(status["ok"], true, "{status}");
        assert_ne!(status["result"]["phase"], "failed", "{status}");
        if status["result"]["phase"] == "ready" {
            break status["result"]["bodyId"]
                .as_str()
                .expect("ready bodyId")
                .to_owned();
        }
        poll = poll.saturating_add(1);
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    (upload_str, body_id)
}

#[tokio::test]
async fn sctm_004_wss_standalone_and_missing_targets() {
    case(|f| {
        Box::pin(async move {
            let standalone = sct_insert_uncontrolled_target(f, 10).await;
            let (inspected, _) = sct_call_peer(
                &mut f.peers[0],
                1,
                "session.control.inspect",
                serde_json::json!({ "targetId": standalone }),
            )
            .await;
            assert_eq!(inspected["ok"], true, "{inspected}");
            assert_eq!(inspected["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(inspected["result"]["controlEpoch"], "0");
            assert_eq!(inspected["result"]["rootTargetId"], standalone);

            let body_id = sct_body_for_target(f, 0, target(10), UploadPurpose::Submit, b"[]")
                .await
                .to_base64url();
            let (denied, _) = sct_call_peer(
                &mut f.peers[0],
                2,
                "input.register",
                serde_json::json!({
                    "targetId": standalone,
                    "operation": { "kind": "submit", "bodyId": body_id }
                }),
            )
            .await;
            assert_eq!(denied["ok"], false);
            assert_eq!(denied["error"]["code"], "session_not_controlled");

            let epoch1 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut p0[0], &mut others, 3, &standalone).await
            };
            assert_eq!(epoch1, "1");

            let (missing, _) = sct_call_peer(
                &mut f.peers[0],
                4,
                "session.control.inspect",
                serde_json::json!({ "targetId": target(99).encode() }),
            )
            .await;
            assert_eq!(missing["ok"], false);
            assert_eq!(missing["error"]["code"], "invalid_target");
        })
    })
    .await;
}

#[tokio::test]
async fn sctm_005_acpi_upload_preemption_survives_and_releases_without_4400() {
    case(|f| {
        Box::pin(async move {
            let target_id = sct_insert_uncontrolled_target(f, 11).await;
            let (_, _, mut peer_b) = sctm_open_client_seeded(f, 82, "Preempt B").await;
            let epoch1 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut peer_b, &mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut p0[0], &mut others, 10, &target_id).await
            };
            assert_eq!(epoch1, "1");

            let (upload_id, body_id) = sctm_upload_via_wss(
                &mut f.peers[0],
                11,
                &target_id,
                br#"[{"type":"text","text":"draft"}]"#,
            )
            .await;

            let epoch2 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut peer_b, &mut others, 15, &target_id).await
            };
            assert_eq!(epoch2, "2");

            // Preempted client socket stays open (no 4400) and releases staged upload cleanly.
            let (reg, _) = sct_call_peer(
                &mut f.peers[0],
                16,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_id }
                }),
            )
            .await;
            assert_eq!(reg["ok"], false, "{reg}");
            assert_eq!(reg["error"]["code"], "session_not_controlled", "{reg}");

            let (released, _) = sct_call_peer(
                &mut f.peers[0],
                17,
                "input.upload.release",
                serde_json::json!({ "uploadId": upload_id }),
            )
            .await;
            assert_eq!(released["ok"], true, "{released}");
            let _ = peer_b.close(None).await;
        })
    })
    .await;
}

#[tokio::test]
async fn sctm_007_readonly_observation_and_all_five_mutations_refused() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let raw = br#"[{"type":"text","text":"readonly inspectable item"}]"#;
            let item_id = sctm_register_and_consume_text(
                f,
                0,
                20,
                target(2),
                "readonly inspectable item",
            )
            .await;
            let _ = raw;
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue
                .suspend_waiting_for_test(
                    f.clients[0].context.actor(),
                    target(2),
                    crate::domain::native_dispatch::KnownStopReason::MaxTokens,
                )
                .await
                .unwrap();

            // Non-controller B (f.peers[2]) reads queue and queued item body.
            let (q, _) = sct_call_peer(
                &mut f.peers[2],
                22,
                "input.queue",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(q["ok"], true, "{q}");
            let entry = &q["result"]["waiting"][0];
            let (opened, _) = sct_call_peer(
                &mut f.peers[2],
                23,
                "input.body.open",
                serde_json::json!({
                    "targetId": target_id,
                    "itemId": entry["itemId"],
                    "expectedRevision": entry["revision"]
                }),
            )
            .await;
            assert_eq!(opened["ok"], true, "{opened}");
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
            send_binary(
                &mut f.peers[2],
                TransferHeader::credit(stream, 0).encode(&[]).unwrap(),
            )
            .await;
            let chunk = next_binary(&mut f.peers[2], Duration::from_secs(5)).await;
            let (_, payload) = TransferHeader::decode(&chunk).unwrap();
            assert!(std::str::from_utf8(payload).unwrap().contains("readonly inspectable item"));

            let before_rows: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
                    .fetch_one(f.pool.as_ref().unwrap())
                    .await
                    .unwrap();
            let submit_body = body_for(f, 2, UploadPurpose::Submit, b"[]").await.to_base64url();
            let edit_body = body_for(f, 2, UploadPurpose::Edit, b"[]").await.to_base64url();
            let ops = [
                serde_json::json!({"kind": "submit", "bodyId": submit_body}),
                serde_json::json!({"kind": "edit", "itemId": item_id, "expectedRevision": "1", "bodyId": edit_body}),
                serde_json::json!({"kind": "cancel", "itemId": item_id, "expectedRevision": "1"}),
                serde_json::json!({"kind": "prioritize", "itemId": item_id, "expectedRevision": "1"}),
                serde_json::json!({"kind": "resume"}),
            ];
            for (idx, op) in ops.into_iter().enumerate() {
                let (denied, _) = sct_call_peer(
                    &mut f.peers[2],
                    30 + u8::try_from(idx).unwrap(),
                    "input.register",
                    serde_json::json!({ "targetId": target_id, "operation": op }),
                )
                .await;
                assert_eq!(denied["ok"], false, "{denied}");
                assert_eq!(denied["error"]["code"], "session_not_controlled", "{denied}");
            }
            let after_rows: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM node_request_registrations")
                    .fetch_one(f.pool.as_ref().unwrap())
                    .await
                    .unwrap();
            assert_eq!(before_rows, after_rows, "uncontrolled writes leave zero SQL rows");
        })
    })
    .await;
}

#[tokio::test]
async fn sctm_009_reconnect_settled_replay_zero_slot_conflict_and_retired() {
    case(|f| {
        Box::pin(async move {
            let target_id = sct_insert_uncontrolled_target(f, 12).await;
            let (id_a, key_a, mut sock_a) = sctm_open_client_seeded(f, 83, "Reconnect A").await;
            {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut p2[0]];
                assert_eq!(sct_takeover_and_drain(&mut sock_a, &mut others, 40, &target_id).await, "1");
            }

            let (_, body_a) = sctm_upload_via_wss(&mut sock_a, 41, &target_id, b"[{\"type\":\"text\",\"text\":\"item-a\"}]").await;
            let (reg, _) = sct_call_peer(
                &mut sock_a,
                45,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_a }
                }),
            )
            .await;
            assert_eq!(reg["ok"], true, "{reg}");
            let reg_id = reg["result"]["registrationId"].as_str().unwrap().to_owned();
            let (consumed, _) = sct_call_peer(
                &mut sock_a,
                46,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg_id,
                    "bodyId": body_a
                }),
            )
            .await;
            assert_eq!(consumed["ok"], true, "{consumed}");

            // Disconnect A without revoke, reconnect A with same key & verify control + idempotent replay.
            let _ = sock_a.close(None).await;
            let node = f.node.as_ref().unwrap();
            let mut sock_a2 = authenticate_socket(
                valid_management_socket(&f.area, node.listen_addr()).await,
                &key_a,
                &id_a,
                node_public_key(node),
            )
            .await;
            let (inspected, _) = sct_call_peer(
                &mut sock_a2,
                47,
                "session.control.inspect",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(inspected["result"]["controllerClientId"], id_a);
            let (queried, _) = sct_call_peer(
                &mut sock_a2,
                50,
                "input.request",
                serde_json::json!({ "targetId": target_id, "registrationId": reg_id }),
            )
            .await;
            assert_eq!(queried["ok"], true, "{queried}");
            assert_eq!(queried["result"]["state"], "settled", "{queried}");
            assert_eq!(queried["result"]["outcome"]["kind"], "queued", "{queried}");

            let (_, body_a2) = sctm_upload_via_wss(
                &mut sock_a2,
                51,
                &target_id,
                b"[{\"type\":\"text\",\"text\":\"item-a\"}]",
            )
            .await;
            let (retry_reg, _) = sct_call_peer(
                &mut sock_a2,
                45,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_a2 }
                }),
            )
            .await;
            assert_eq!(retry_reg["result"]["registrationId"], reg_id);
            let (replay, _) = sct_call_peer(
                &mut sock_a2,
                55,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg_id,
                    "bodyId": body_a2
                }),
            )
            .await;
            assert_eq!(replay["ok"], true, "{replay}");
            assert_eq!(replay["result"]["request"]["outcome"]["kind"], "queued", "{replay}");
            let (q_after, _) = sct_call_peer(
                &mut sock_a2,
                56,
                "input.queue",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(q_after["result"]["waiting"].as_array().unwrap().len(), 1);

            // Conflict check on same requestId with different intent, plus retired check.
            let (conflict, _) = sct_call_peer(
                &mut sock_a2,
                45,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "resume" }
                }),
            )
            .await;
            assert_eq!(conflict["error"]["code"], "request_conflict");

            // Uncontrolled B (f.peers[2]) refused at marker 48 without occupying requestId 48 or queue slot.
            let body_b = sct_body_for_target(f, 2, target(12), UploadPurpose::Submit, b"[]")
                .await
                .to_base64url();
            let (refused_b, _) = sct_call_peer(
                &mut f.peers[2],
                48,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_b }
                }),
            )
            .await;
            assert_eq!(refused_b["error"]["code"], "session_not_controlled");
            let slot_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM node_request_registrations WHERE request_id=?",
            )
            .bind(Id128::new([48; 16]).to_base64url())
            .fetch_one(f.pool.as_ref().unwrap())
            .await
            .unwrap();
            assert_eq!(slot_count, 0, "uncontrolled refusal must not occupy request slot");

            sqlx::query("UPDATE main.node_request_registrations SET retired=1 WHERE request_id=?")
                .bind(&reg_id)
                .execute(f.pool.as_ref().unwrap())
                .await
                .unwrap();
            let (retired, _) = sct_call_peer(
                &mut sock_a2,
                49,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg_id,
                    "bodyId": body_a
                }),
            )
            .await;
            assert_eq!(retired["ok"], false);
            assert_eq!(retired["error"]["code"], "request_retired");
            let _ = sock_a2.close(None).await;
        })
    })
    .await;
}

#[tokio::test]
async fn sctm_010_and_011_three_item_queue_takeover_sync_and_late_edit_refused() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let mut items = Vec::new();
            for (idx, text) in ["Q1", "Q2", "Q3"].into_iter().enumerate() {
                let marker = 60 + u8::try_from(idx * 2).unwrap();
                items.push(
                    sctm_register_and_consume_text(f, 0, marker, target(2), text).await,
                );
            }
            let (_, _, mut observer_c) = sctm_open_client_seeded(f, 84, "Observer C").await;
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .queue
                .suspend_waiting_for_test(
                    f.clients[0].context.actor(),
                    target(2),
                    crate::domain::native_dispatch::KnownStopReason::MaxTokens,
                )
                .await
                .unwrap();

            // Pre-takeover read works for B while edit fails; takeover notifies A, B, C.
            let (q_before, _) = sct_call_peer(
                &mut f.peers[2],
                66,
                "input.queue",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(q_before["result"]["waiting"].as_array().unwrap().len(), 3);
            let edit_b_deny = body_for(f, 2, UploadPurpose::Edit, b"[]").await.to_base64url();
            let (denied, _) = sct_call_peer(
                &mut f.peers[2],
                67,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "edit", "itemId": items[0], "expectedRevision": "1", "bodyId": edit_b_deny }
                }),
            )
            .await;
            assert_eq!(denied["error"]["code"], "session_not_controlled");

            let epoch1 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut observer_c];
                sct_takeover_and_drain(&mut p2[0], &mut others, 68, &target_id).await
            };
            assert_eq!(epoch1, "2");

            // Pre-register an edit as B or test A's late edit after displacement -> rejected cleanly.
            let edit_a_late = body_for(f, 0, UploadPurpose::Edit, b"[{\"type\":\"text\",\"text\":\"late\"}]")
                .await
                .to_base64url();
            let (late_edit, _) = sct_call_peer(
                &mut f.peers[0],
                69,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "edit", "itemId": items[0], "expectedRevision": "1", "bodyId": edit_a_late }
                }),
            )
            .await;
            assert_eq!(late_edit["error"]["code"], "session_not_controlled");

            let edit_b = body_for(f, 2, UploadPurpose::Edit, b"[{\"type\":\"text\",\"text\":\"Q1-edited\"}]")
                .await
                .to_base64url();
            let (reg_e, _) = sct_call_peer(
                &mut f.peers[2],
                70,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "edit", "itemId": items[0], "expectedRevision": "1", "bodyId": edit_b }
                }),
            )
            .await;
            assert_eq!(reg_e["ok"], true, "{reg_e}");
            let (con_e, _) = sct_call_peer(
                &mut f.peers[2],
                71,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg_e["result"]["registrationId"],
                    "bodyId": edit_b
                }),
            )
            .await;
            assert_eq!(con_e["ok"], true, "{con_e}");
            assert_eq!(con_e["result"]["request"]["outcome"]["kind"], "edited");

            let (reg_c, _) = sct_call_peer(
                &mut f.peers[2],
                72,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "cancel", "itemId": items[1], "expectedRevision": "1" }
                }),
            )
            .await;
            assert_eq!(reg_c["ok"], true, "{reg_c}");
            let (con_c, _) = sct_call_peer(
                &mut f.peers[2],
                73,
                "input.consume",
                serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg_c["result"]["registrationId"],
                    "bodyId": serde_json::Value::Null
                }),
            )
            .await;
            assert_eq!(con_c["ok"], true, "{con_c}");
            assert_eq!(con_c["result"]["request"]["outcome"]["kind"], "cancelled");

            let (qa, _) = sct_call_peer(&mut f.peers[0], 74, "input.queue", serde_json::json!({ "targetId": target_id })).await;
            let (qb, _) = sct_call_peer(&mut f.peers[2], 75, "input.queue", serde_json::json!({ "targetId": target_id })).await;
            let (qc, _) = sct_call_peer(&mut observer_c, 76, "input.queue", serde_json::json!({ "targetId": target_id })).await;
            assert_eq!(qa["result"]["waiting"], qb["result"]["waiting"]);
            assert_eq!(qa["result"]["waiting"], qc["result"]["waiting"]);
            assert_eq!(qa["result"]["waiting"].as_array().unwrap().len(), 2);
            let _ = observer_c.close(None).await;
        })
    })
    .await;
}

#[tokio::test]
async fn sctm_014_release_wss_and_cli_revoke_audit_and_frozen_tables() {
    case(|f| {
        Box::pin(async move {
            let target_id = target(2).encode();
            let item_id = sctm_register_and_consume_text(f, 0, 80, target(2), "keep-me").await;
            let _ = item_id;
            let (initial, _) = sct_call_peer(
                &mut f.peers[0],
                82,
                "session.control.inspect",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(initial["result"]["controlEpoch"], "1");

            // Non-controller B release refused; controller A release increments epoch to 2.
            let (non_ctrl_rel, _) = sct_call_peer(
                &mut f.peers[2],
                83,
                "session.control.release",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(non_ctrl_rel["error"]["code"], "session_not_controlled");

            let (rel, _) = sct_call_peer(
                &mut f.peers[0],
                84,
                "session.control.release",
                serde_json::json!({ "targetId": target_id, "expectedControlEpoch": "1" }),
            )
            .await;
            assert_eq!(rel["ok"], true, "{rel}");
            assert_eq!(rel["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(rel["result"]["controlEpoch"], "2");
            assert_eq!(queued(f, 0).await.len(), 1, "release preserves queue items");

            // Client C takes over -> revoked via WSS `device.revoke` -> clears to uncontrolled epoch 4.
            let (id_c, _, mut sock_c) = sctm_open_client_seeded(f, 85, "Client C").await;
            {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut p2[0]];
                assert_eq!(sct_takeover_and_drain(&mut sock_c, &mut others, 85, &target_id).await, "3");
            }
            let (rev_wss, _) = sct_call_peer(
                &mut f.peers[0],
                86,
                "device.revoke",
                serde_json::json!({ "clientId": id_c }),
            )
            .await;
            assert_eq!(rev_wss["ok"], true, "{rev_wss}");
            drop(sock_c);

            // Client D takes over -> revoked via CLI named pipe (`ipc::device_revoke`) without deadlock.
            let (id_d, _, mut sock_d) = sctm_open_client_seeded(f, 86, "Client D").await;
            {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut p2[0]];
                assert_eq!(sct_takeover_and_drain(&mut sock_d, &mut others, 87, &target_id).await, "5");
            }
            let revoked_ipc = crate::ipc::device_revoke(f.node.as_ref().unwrap().ipc_address(), &id_d)
                .await
                .unwrap();
            assert_eq!(revoked_ipc.client_id, id_d);
            drop(sock_d);

            let (post_ipc, _) = sct_call_peer(
                &mut f.peers[0],
                88,
                "session.control.inspect",
                serde_json::json!({ "targetId": target_id }),
            )
            .await;
            assert_eq!(post_ipc["result"]["controllerClientId"], serde_json::Value::Null);
            assert_eq!(post_ipc["result"]["controlEpoch"], "6");

            let pool = f.pool.as_ref().unwrap();
            for action in ["session.control.takeover", "session.control.release", "session.control.revoke_clear"] {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM security_audit WHERE action=?")
                    .bind(action)
                    .fetch_one(pool)
                    .await
                    .unwrap();
                assert!(count >= 1, "missing security_audit row for {action}");
            }
            let forbidden_tbl: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='session_commands'",
            )
            .fetch_one(pool)
            .await
            .unwrap();
            assert_eq!(forbidden_tbl, 0);
            let mut conn = pool.acquire().await.unwrap();
            crate::session_storage::schema::validate_management_audit(&mut conn)
                .await
                .unwrap();
        })
    })
    .await;
}
