async fn rt_socket_command(
    socket: &mut ManagementSocket,
    marker: u8,
    action: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    send_json(
        socket,
        &serde_json::json!({
            "v":1,"type":"command","requestId":Id128::new([marker;16]),
            "action":action,"payload":payload
        }),
    )
    .await;
    next_json(socket, Duration::from_secs(15)).await
}

fn rt_method_count_at(directory: &std::path::Path, method: &str) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("capacity-peer-") && name.ends_with(".jsonl")
        })
        .flat_map(|entry| {
            let bytes = std::fs::read(entry.path()).unwrap();
            bytes
                .split_inclusive(|byte| *byte == b'\n')
                .filter(|line| line.last() == Some(&b'\n'))
                .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
                .collect::<Vec<_>>()
        })
        .filter(|event| event["method"] == method)
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_019_node_current_idle_with_necessary_pending_then_waiting_queue_is_protected() {
    use crate::session_storage::request_registry::test_hooks::{Action, Barrier, Operation, Point};
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let idle_root = capacity_create(f, 151).await;
            capacity_route_target(f, &idle_root, r3_id(46)).await;
            for i in 0..49_u8 {
                capacity_create(f, 1 + 2 * i).await;
            }
            capacity_expect(f, 154, 50, 0).await;
            let node = f.node.as_ref().unwrap();
            let mut socket = authenticate_socket(
                valid_management_socket(&f.area, node.listen_addr()).await,
                &f.peers[0].key,
                &f.peers[0].client_id,
                node_public_key(node),
            )
            .await;
            let registered = rt_socket_command(
                &mut socket,
                155,
                "source.session.register",
                serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
            )
            .await;
            assert_eq!(registered["ok"], true, "{registered}");
            let source_operation = registered["result"]["sourceOperationId"].clone();
            let directory = f.area.directory.clone();
            let barrier = Barrier::new();
            let release = RtBarrierDrop(|| barrier.release());
            node.storage
                .as_ref()
                .unwrap()
                .request_registry()
                .faults_for_test()
                .arm(
                    Point::AfterCommit(Operation::Begin),
                    Action::Pause(barrier.clone()),
                )
                .unwrap();
            tokio::join!(capacity_submit_execution_target(f, r3_id(46)), async {
                timeout(Duration::from_secs(5), barrier.arrived())
                    .await
                    .unwrap();
                let queue = rt_socket_command(
                    &mut socket,
                    156,
                    "input.queue",
                    serde_json::json!({"targetId":r3_id(46).encode()}),
                )
                .await;
                assert_eq!(queue["ok"], true, "{queue}");
                assert!(
                    queue["result"]["processing"].is_object(),
                    "actual necessary operation is pending: {queue}"
                );
                assert_eq!(
                    queue["result"]["waiting"].as_array().unwrap().len(),
                    0,
                    "{queue}"
                );
                let result = rt_socket_command(
                    &mut socket,
                    157,
                    "source.session.execute",
                    serde_json::json!({"sourceOperationId":source_operation}),
                )
                .await;
                assert!(
                    result.to_string().contains("session_capacity_full"),
                    "{result}"
                );
                assert!(
                    !queue["result"]["processing"].is_null(),
                    "the observed pending queue protects the idle root"
                );
                for method in ["session/prompt", "session/load", "session/resume"] {
                    assert_eq!(
                        rt_method_count_at(&directory, method),
                        0,
                        "necessary Begin is blocked before an executable item exists: {method}"
                    );
                }
                drop(release);
            });
            let queue = rt_socket_command(
                &mut socket,
                158,
                "input.queue",
                serde_json::json!({"targetId":r3_id(46).encode()}),
            )
            .await;
            assert_eq!(queue["ok"], true, "{queue}");
            assert_eq!(
                queue["result"]["waiting"].as_array().unwrap().len(),
                1,
                "{queue}"
            );
            assert!(queue["result"]["active"].is_null(), "{queue}");
            let refused =
                capacity_source_operation(f, 159, serde_json::json!({"kind":"create"})).await;
            assert!(
                refused.to_string().contains("session_capacity_full"),
                "{refused}"
            );
            assert_eq!(capacity_method_count(f, "session/new"), 50);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            // After the pending barrier opens, the ordinary dispatcher may
            // legally promote the observed waiting item. Protection was tested
            // while pending and again immediately after the real waiting view.
            capacity_expect(f, 162, 50, 0).await;
        })
    })
    .await;
}
