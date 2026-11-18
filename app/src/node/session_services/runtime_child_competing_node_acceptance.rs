#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_007_two_real_create_contenders_share_one_reclaim_and_one_refund() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let candidate = capacity_create(f, 151).await;
            capacity_route_target(f, &candidate, r3_id(47)).await;
            for i in 0..49_u8 {
                capacity_create(f, 1 + i * 2).await;
            }
            capacity_expect(f, 154, 50, 0).await;
            let release = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-close"), b"hold").unwrap();
            let one = f.call(
                0, 155, "source.session.register",
                serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
            ).await;
            let two = f.call(
                1, 156, "source.session.register",
                serde_json::json!({"selection":f.selection_json(),"operation":{"kind":"create"}}),
            ).await;
            assert_eq!(one["ok"], true, "{one}");
            assert_eq!(two["ok"], true, "{two}");
            for (peer, marker, operation) in [
                (0, 157, &one["result"]["sourceOperationId"]),
                (1, 158, &two["result"]["sourceOperationId"]),
            ] {
                send_json(
                    &mut f.peers[peer].socket,
                    &serde_json::json!({
                        "v":1,"type":"command","requestId":Id128::new([marker;16]),
                        "action":"source.session.execute",
                        "payload":{"sourceOperationId":operation}
                    }),
                )
                .await;
            }
            capacity_wait_method(f, "session/close", 1).await;
            assert_eq!(capacity_method_count(f, "session/new"), 50);
            assert_eq!(
                f.node
                    .as_ref()
                    .unwrap()
                    .session_services
                    .as_ref()
                    .unwrap()
                    .native
                    .inspect_capacity()
                    .await
                    .unwrap()
                    .occupied,
                50,
                "a peer close request held before its result has not returned quota",
            );
            drop(release);
            let (left, right) = f.peers.split_at_mut(1);
            let (one, two) = tokio::join!(
                next_json(&mut left[0].socket, Duration::from_secs(15)),
                next_json(&mut right[0].socket, Duration::from_secs(15)),
            );
            let succeeded = |result: &serde_json::Value| {
                result["ok"] == true && result["result"]["outcome"] == "succeeded"
            };
            assert_ne!(succeeded(&one), succeeded(&two), "{one} {two}");
            let loser = if succeeded(&one) { &two } else { &one };
            assert!(
                loser.to_string().contains("session_capacity_full"),
                "{loser}"
            );
            assert_eq!(capacity_method_count(f, "session/new"), 51);
            let closes: Vec<_> = capacity_events(f)
                .into_iter()
                .filter(|event| event["method"] == "session/close")
                .collect();
            assert_eq!(closes.len(), 1);
            assert_eq!(closes[0]["native"], candidate);
            let finished: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_session_automatic_releases WHERE phase='finished'",
            )
            .fetch_one(
                f.node
                    .as_ref()
                    .unwrap()
                    .storage
                    .as_ref()
                    .unwrap()
                    .pool_for_test(),
            )
            .await
            .unwrap();
            assert_eq!(finished, 1);
            capacity_expect(f, 160, 50, 0).await;
        })
    })
    .await;
}
