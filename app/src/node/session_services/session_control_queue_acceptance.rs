// Session control takeover & read-only isolation L5 tests (default Cargo features).
// Included under `crate::node::tests::queue_management_development`.

async fn sct_call_peer(
    socket: &mut ManagementSocket,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> (serde_json::Value, Vec<serde_json::Value>) {
    let request_id = Id128::new([marker; 16]);
    send_json(
        socket,
        &serde_json::json!({
            "v": 1,
            "type": "command",
            "requestId": request_id,
            "action": action,
            "payload": payload,
        }),
    )
    .await;
    let mut events = Vec::new();
    let expected = request_id.to_base64url();
    loop {
        let frame = next_json(socket, Duration::from_secs(15)).await;
        if frame["type"] != "result" {
            events.push(frame);
            continue;
        }
        if frame["requestId"] == expected {
            return (frame, events);
        }
        panic!("unexpected command result while awaiting {expected}: {frame}");
    }
}

async fn sct_takeover_and_drain(
    socket: &mut ManagementSocket,
    other_sockets: &mut [&mut ManagementSocket],
    marker: u8,
    target_id: &str,
) -> String {
    let (resp, mut events) = sct_call_peer(
        socket,
        marker,
        "session.control.takeover",
        serde_json::json!({ "targetId": target_id }),
    )
    .await;
    assert_eq!(resp["ok"], true, "{resp}");
    let epoch = resp["result"]["controlEpoch"]
        .as_str()
        .expect("controlEpoch string")
        .to_owned();
    let expected_event = |evt: &serde_json::Value| {
        evt["type"] == "session.control.changed" && evt["controlEpoch"] == epoch
    };
    if !events.iter().any(expected_event) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            let evt = next_json(socket, Duration::from_secs(5)).await;
            let matched = expected_event(&evt);
            events.push(evt);
            if matched {
                break;
            }
        }
    }
    assert!(events.iter().any(expected_event), "missing broadcast {events:?}");
    for other in other_sockets {
        loop {
            let evt = next_json(other, Duration::from_secs(5)).await;
            if expected_event(&evt) {
                break;
            }
        }
    }
    epoch
}

async fn sct_open_second_client(f: &Case, display_name: &str) -> (String, SigningKey, ManagementSocket) {
    let node = f.node.as_ref().expect("live node");
    let key = SigningKey::from_slice(&[77; 32]).expect("second client key");
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

async fn sct_insert_uncontrolled_target(f: &Case, n: u8) -> String {
    let target_id = target(n).encode();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(&target_id)
        .bind(target(1).encode())
        .bind(format!("sct-native-{n}"))
        .execute(f.pool.as_ref().unwrap())
        .await
        .unwrap();
    target_id
}

async fn sct_body_for_target(
    f: &Case,
    socket: usize,
    target_entity: crate::domain::EntityId,
    purpose: UploadPurpose,
    raw: &[u8],
) -> Id128 {
    let uploads = f.uploads(socket);
    let client = &f.clients[socket];
    let id = uploads.begin(client, target_entity, purpose).await.unwrap().upload();
    let wire = frame(id, 0, raw);
    uploads
        .accept_data(client, InputData::decode_client(&wire).unwrap())
        .await
        .unwrap();
    uploads.finish(client, id, raw.len() as u64).await.unwrap();
    ready(&uploads, client, id).await.body().unwrap()
}

#[tokio::test]
async fn sct_red_check_queue_uncontrolled_and_cross_epoch_two_step_rejected() {
    case(|f| {
        Box::pin(async move {
            let target_id = sct_insert_uncontrolled_target(f, 4).await;
            let body_id = sct_body_for_target(f, 0, target(4), UploadPurpose::Submit, b"[]")
                .await
                .to_base64url();

            // 1. Default uncontrolled session must reject input.register with session_not_controlled.
            let (uncontrolled, _) = sct_call_peer(
                &mut f.peers[0],
                201,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_id }
                }),
            )
            .await;
            assert_eq!(uncontrolled["ok"], false, "{uncontrolled}");
            assert_eq!(
                uncontrolled["error"]["code"], "session_not_controlled",
                "{uncontrolled}"
            );

            // 2. Pair a distinct second client B for A(1) -> B(2) -> A(3) wrap-around test.
            let (_, _, mut peer_b) = sct_open_second_client(f, "Tablet B").await;
            let epoch1 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut peer_b, &mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut p0[0], &mut others, 202, &target_id).await
            };
            assert_eq!(epoch1, "1");

            // A registers in epoch 1 (pauses before input.consume).
            let (reg1, _) = sct_call_peer(
                &mut f.peers[0],
                203,
                "input.register",
                serde_json::json!({
                    "targetId": target_id,
                    "operation": { "kind": "submit", "bodyId": body_id },
                    "expectedControlEpoch": "1"
                }),
            )
            .await;
            assert_eq!(reg1["ok"], true, "{reg1}");
            let reg1_id = reg1["result"]["registrationId"]
                .as_str()
                .expect("reg1 id")
                .to_owned();

            // B preempts -> epoch 2, then A retakes -> epoch 3.
            let epoch2 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut p0[0], &mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut peer_b, &mut others, 204, &target_id).await
            };
            assert_eq!(epoch2, "2");
            let epoch3 = {
                let (p0, rest) = f.peers.split_at_mut(1);
                let (p1, p2) = rest.split_at_mut(1);
                let mut others = [&mut peer_b, &mut p1[0], &mut p2[0]];
                sct_takeover_and_drain(&mut p0[0], &mut others, 205, &target_id).await
            };
            assert_eq!(epoch3, "3");

            // A retries input.consume using epoch-1 reg1_id (both omitted and expectedControlEpoch:"3"):
            // both must fail with session_not_controlled.
            for (marker, expected) in [(206, None), (207, Some("3"))] {
                let mut payload = serde_json::json!({
                    "targetId": target_id,
                    "registrationId": reg1_id,
                    "bodyId": body_id
                });
                if let Some(epoch) = expected {
                    payload["expectedControlEpoch"] = serde_json::Value::String(epoch.to_owned());
                }
                let (resp, _) =
                    sct_call_peer(&mut f.peers[0], marker, "input.consume", payload).await;
                assert_eq!(resp["ok"], false, "{resp}");
                assert_eq!(resp["error"]["code"], "session_not_controlled", "{resp}");
            }
            let _ = peer_b.close(None).await;
        })
    })
    .await;
}
