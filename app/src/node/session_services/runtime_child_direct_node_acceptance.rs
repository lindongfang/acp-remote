async fn rt_direct_owner_snapshot(f: &R3WssCase) -> (Vec<String>, Vec<String>) {
    let pool = f
        .node
        .as_ref()
        .unwrap()
        .storage
        .as_ref()
        .unwrap()
        .pool_for_test();
    let owners: Vec<String> = sqlx::query_scalar(
        "SELECT json_array(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,hex(config_revision),owner_kind,lifecycle,scope_binding_id) FROM node_native_source_instances ORDER BY instance_id",
    ).fetch_all(pool).await.unwrap();
    let capacity: Vec<String> = sqlx::query_scalar(
        "SELECT json_array(reservation_id,source_id,native_id,instance_id,runtime_epoch,create_operation,root_verified,physically_closed,released) FROM node_session_capacity ORDER BY reservation_id",
    ).fetch_all(pool).await.unwrap();
    (owners, capacity)
}
async fn rt_direct_tree(f: &mut R3WssCase) -> String {
    let root = capacity_create(f, 141).await;
    std::fs::write(
        f.area.directory.join("runtime-direct-tree"),
        root.as_bytes(),
    )
    .unwrap();
    let discovered =
        capacity_source_operation(f, 143, serde_json::json!({"kind":"discover","cursor":null}))
            .await;
    assert_eq!(discovered["ok"], true, "{discovered}");
    let pool = f
        .node
        .as_ref()
        .unwrap()
        .storage
        .as_ref()
        .unwrap()
        .pool_for_test()
        .clone();
    timeout(Duration::from_secs(10), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_session_parents WHERE source_id=? AND parent_native_id=?",
            ).bind(r3_id(1).encode()).bind(&root).fetch_one(&pool).await.unwrap();
            if count == 2 { break; }
            tokio::task::yield_now().await;
        }
    }).await.unwrap();
    assert_eq!(
        capacity_method_count(f, "session/resume"),
        0,
        "no root UI read has occurred"
    );
    root
}

async fn rt_direct_child_snapshot(
    f: &mut R3WssCase,
    peer: usize,
    opened: &serde_json::Value,
    marker: u8,
    wanted: &str,
    forbidden: &str,
) {
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(
        opened["result"]["status"]["mode"], "current_only",
        "{opened}"
    );
    assert_eq!(
        opened["result"]["status"]["historyIncomplete"], true,
        "{opened}"
    );
    let mut stream =
        Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
    let (mut records, rebound) =
        r3_drain_view_snapshot_or_rebound(&mut f.peers[peer].socket, stream).await;
    if rebound {
        let next = f
            .call(
                peer,
                marker,
                "view.rebind",
                serde_json::json!({"streamId":stream.to_base64url()}),
            )
            .await;
        assert_eq!(next["ok"], true, "{next}");
        stream = Id128::parse_base64url(next["result"]["streamId"].as_str().unwrap()).unwrap();
        let (actual, again) =
            r3_drain_view_snapshot_or_rebound(&mut f.peers[peer].socket, stream).await;
        assert!(!again);
        records = actual;
    }
    let text = records
        .iter()
        .map(|(_, body)| String::from_utf8_lossy(body))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains(wanted),
        "the correct actual child body is required: {text}"
    );
    assert!(
        !text.contains(forbidden),
        "another child body must not leak into this view: {text}"
    );
    r3_send_binary(
        &mut f.peers[peer].socket,
        TransferHeader::detach(stream).encode(&[]).unwrap(),
    )
    .await;
    let closed = f
        .call(
            peer,
            marker + 1,
            "view.close",
            serde_json::json!({"streamId":stream.to_base64url()}),
        )
        .await;
    assert_eq!(closed["ok"], true, "{closed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_010_011_actual_created_relations_direct_children_share_original_root_and_distinct_bodies()
 {
    r3_wss_case("source-create-replay-state", true, |f| {
        Box::pin(async move {
            let root = rt_direct_tree(f).await;
            let before_owner = rt_direct_owner_snapshot(f).await;
            assert_eq!(before_owner.0.len(), 1);
            assert_eq!(before_owner.1.len(), 1);
            let before_pid = capacity_events(f).into_iter().find(|e| e["method"]=="initialize").unwrap()["pid"].clone();
            let alternate = capacity_alternate_selection(f, r3_id(1)).await;
            let first_payload = serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":"rt-direct-child-A","reason":"first_open"
            });
            let original = f.selection.replace(alternate.clone()).unwrap();
            let second_payload = serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":"rt-direct-child-B","reason":"first_open"
            });
            f.selection = Some(original);
            send_json(&mut f.peers[0].socket, &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([145;16]),"action":"view.open_source","payload":first_payload
            })).await;
            send_json(&mut f.peers[1].socket, &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([146;16]),"action":"view.open_source","payload":second_payload
            })).await;
            let (left,right) = f.peers.split_at_mut(1);
            let (first,second) = tokio::join!(
                next_json(&mut left[0].socket, Duration::from_secs(15)),
                next_json(&mut right[0].socket, Duration::from_secs(15)),
            );
            rt_direct_child_snapshot(f,0,&first,147,"RT actual alpha child A","RT actual beta child B").await;
            rt_direct_child_snapshot(f,1,&second,149,"RT actual beta child B","RT actual alpha child A").await;

            assert_eq!(rt_direct_owner_snapshot(f).await, before_owner,
                "closing readers must preserve original instance/runtime/config/reservation");
            let selection = f.selection.replace(alternate).unwrap();
            let reopened = f.call(0, 179, "view.open_source", serde_json::json!({
                "selection": f.selection_json(), "nativeSessionId":"rt-direct-child-A", "reason":"first_open"
            })).await;
            f.selection = Some(selection);
            rt_direct_child_snapshot(f,0,&reopened,180,"RT actual alpha child A","RT actual beta child B").await;
            assert_eq!(rt_direct_owner_snapshot(f).await, before_owner,
                "cross-profile reopen retains exact source instance/runtime/config and original capacity");
            assert!(capacity_events(f).iter().all(|e|e["pid"]==before_pid), "all actual native work stays on the captured original process");
            capacity_expect(f,151,1,0).await;
            assert_eq!(capacity_method_count(f,"session/new"),1);
            assert_eq!(capacity_method_count(f,"initialize"),1, "alternate profile must use the original running connection");
            let resumes: Vec<_> = capacity_events(f).into_iter().filter(|event|event["method"]=="session/resume").collect();
            assert_eq!(resumes.len(),1);
            assert_eq!(resumes[0]["native"],root);
            assert_eq!(capacity_method_count(f,"session/load"),0);
            assert_eq!(capacity_method_count(f,"session/close"),0, "closing both readers does not close root or child");
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_012_missing_child_relation_never_guesses_root_or_sends_child_request() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            let _root = capacity_create(f,141).await;
            let opened = f.call(0,143,"view.open_source",serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":"rt-never-observed-child","reason":"first_open"
            })).await;
            assert_eq!(opened["ok"],false,"{opened}");
            assert_eq!(opened["error"]["code"],"session_tree_unverified","{opened}");
            assert_eq!(capacity_method_count(f,"session/resume"),0);
            assert_eq!(capacity_method_count(f,"session/load"),0);
            assert_eq!(capacity_method_count(f,"session/close"),0);
            capacity_expect(f,145,1,0).await;
        })
    }).await;
}
