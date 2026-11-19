// Third batch. Mount after the core and extended independent acceptance sources.

async fn capacity_wait_method(f: &R3WssCase, method: &str, count: usize) {
    timeout(Duration::from_secs(10), async {
        while capacity_method_count(f, method) < count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual owned ACP peer method observed");
}

async fn capacity_wait_completed_stream_rebound(socket: &mut ManagementSocket, stream: Id128) {
    timeout(Duration::from_secs(12), async {
        loop {
            let bytes = r3_next_binary(socket).await;
            let (header, payload) = TransferHeader::decode(&bytes).unwrap();
            assert_eq!(header.stream(), stream);
            r3_send_binary(
                socket,
                TransferHeader::credit(stream, header.packet())
                    .encode(&[])
                    .unwrap(),
            )
            .await;
            if header.record_kind() == Some(RecordKind::Status) {
                let status: serde_json::Value = serde_json::from_slice(payload).unwrap();
                if status["type"] == "stream_rebound" {
                    assert_eq!(status["previousStreamId"], stream.to_base64url());
                    break;
                }
            }
        }
    })
    .await
    .expect("real original-read Complete produces stream_rebound before reader close");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_restore_worker_failure_returns_real_node_error_and_keeps_slot() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            use crate::application::{
                native_dispatch::SourceReadSelectionProof,
                shared_views::{
                    budget::ReadReason,
                    controller::{HookSelector, ViewFalliblePoint},
                },
            };
            let native = capacity_create(f, 240).await;
            capacity_route_target(f, &native, r3_id(30)).await;
            let opened = f.call(0, 247, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            let first_stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
            let (_, needs_rebind) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, first_stream).await;
            // Created/cache snapshot is not completion of the original load.
            // Keep this actual reader alive through native IO and Complete.
            capacity_wait_method(f, "session/load", 1).await;
            if !needs_rebind {
                capacity_wait_completed_stream_rebound(&mut f.peers[0].socket, first_stream).await;
            }
            let active = {
                let rebound = f
                    .call(
                        0,
                        248,
                        "view.rebind",
                        serde_json::json!({
                            "streamId":first_stream.to_base64url()
                        }),
                    )
                    .await;
                assert_eq!(rebound["ok"], true, "{rebound}");
                let stream =
                    Id128::parse_base64url(rebound["result"]["streamId"].as_str().unwrap())
                        .unwrap();
                let (_, again) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream).await;
                assert!(!again);
                stream
            };
            let closed_reader = f
                .call(
                    0,
                    249,
                    "view.close",
                    serde_json::json!({
                        "streamId":active.to_base64url()
                    }),
                )
                .await;
            assert_eq!(closed_reader["ok"], true, "{closed_reader}");
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            // Obtain the real original attachment without manufacturing a source,
            // native root, capacity record, or a replacement worker.
            let node = f.node.as_ref().unwrap();
            let proof = SourceReadSelectionProof::mint(
                node.storage
                    .as_ref()
                    .unwrap()
                    .request_registry()
                    .owner_scope(),
                f.selection.as_ref().unwrap().clone(),
                &native,
            )
            .unwrap();
            let acquisition = node
                .session_services
                .as_ref()
                .unwrap()
                .native
                .acquire_source_read(proof, ReadReason::FirstOpen)
                .unwrap();
            let capability = acquisition.source_capability().await.unwrap();
            let attachment = capability.source().views().unwrap();
            let views = attachment.views().unwrap();
            let key = capability.native().clone();
            // Initial publication is produced by the actual FIFO worker.
            timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(current) = views.core.target(&key) {
                        break current;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual original native view publication");
            let original = views.core.target(&key).unwrap();
            let release = capacity_register_release(f, 242, r3_id(30)).await;
            let released = capacity_release(f, 243, &release).await;
            assert_eq!(released["result"]["state"], "released", "{released}");
            capacity_expect(f, 244, 0, 0).await;
            assert!(
                !original.send.current().unwrap(),
                "original native generation sealed"
            );
            let mut fault = attachment
                .test_controller()
                .unwrap()
                .fail_next(
                    ViewFalliblePoint::NewViewEpochRandom,
                    HookSelector::Target(key.clone()),
                )
                .unwrap();
            let selection = f.selection_json();
            let response = timeout(
                Duration::from_secs(5),
                f.call(
                    0,
                    245,
                    "view.open_source",
                    serde_json::json!({
                        "selection":selection,"nativeSessionId":native,"reason":"reopen"
                    }),
                ),
            )
            .await
            .expect("actual Node must report worker failure without hanging");
            timeout(Duration::from_secs(1), fault.triggered())
                .await
                .expect("real restore worker reached armed failure")
                .unwrap();
            assert_eq!(response["ok"], false, "{response}");
            assert_eq!(
                response["error"]["code"], "random_unavailable",
                "{response}"
            );
            assert!(
                !original.send.current().unwrap(),
                "old native generation stays sealed"
            );
            assert_eq!(
                views.core.probe.route_failure(),
                Some(crate::domain::shared_view::ViewError::RandomUnavailable),
            );
            capacity_expect(f, 246, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            assert_eq!(capacity_method_count(f, "session/close"), 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_create_and_v2_source_replay_compete_for_last_slot() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            let dormant = capacity_create(f, 130).await;
            capacity_route_target(f, &dormant, r3_id(26)).await;
            let release = capacity_register_release(f, 132, r3_id(26)).await;
            let closed = capacity_release(f, 133, &release).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            for i in 0..49_u8 {
                capacity_create(f, 1 + 2 * i).await;
            }
            capacity_expect(f, 134, 49, 0).await;
            // With production reclamation enabled the actual replay winner
            // must remain known-busy, otherwise the next contender may legally
            // close its fresh idle generation and both requests can succeed.
            std::fs::write(f.area.directory.join("runtime-replay-running"), b"busy").unwrap();
            let create = f
                .call(
                    0,
                    140,
                    "source.session.register",
                    serde_json::json!({
                        "selection":f.selection_json(),"operation":{"kind":"create"}
                    }),
                )
                .await;
            assert_eq!(create["ok"], true, "{create}");
            send_json(
                &mut f.peers[0].socket,
                &serde_json::json!({
                    "v":1,"type":"command","requestId":Id128::new([141;16]),
                    "action":"source.session.execute",
                    "payload":{"sourceOperationId":create["result"]["sourceOperationId"]}
                }),
            )
            .await;
            let selection = f.selection_json();
            send_json(&mut f.peers[1].socket, &serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([142;16]),
                "action":"view.open_source",
                "payload":{"selection":selection,"nativeSessionId":dormant,"reason":"first_open"}
            })).await;
            let (left, right) = f.peers.split_at_mut(1);
            let (created, replay) = tokio::join!(
                next_json(&mut left[0].socket, Duration::from_secs(15)),
                next_json(&mut right[0].socket, Duration::from_secs(15)),
            );
            let create_won = created["ok"] == true && created["result"]["outcome"] == "succeeded";
            let replay_won = replay["ok"] == true;
            assert_ne!(
                create_won, replay_won,
                "exactly one of actual new/replay can obtain final slot: {created} {replay}"
            );
            let refused = if create_won { &replay } else { &created };
            assert!(
                refused.to_string().contains("session_capacity_full"),
                "{refused}"
            );
            if replay_won {
                let stream =
                    Id128::parse_base64url(replay["result"]["streamId"].as_str().unwrap()).unwrap();
                let (_, rebound) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, stream).await;
                let active = if rebound {
                    let rebound = f
                        .call(
                            1,
                            143,
                            "view.rebind",
                            serde_json::json!({"streamId":stream.to_base64url()}),
                        )
                        .await;
                    assert_eq!(rebound["ok"], true, "{rebound}");
                    let next =
                        Id128::parse_base64url(rebound["result"]["streamId"].as_str().unwrap())
                            .unwrap();
                    let (_, repeated) =
                        r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, next).await;
                    assert!(!repeated);
                    next
                } else {
                    stream
                };
                let reader_closed = f
                    .call(
                        1,
                        144,
                        "view.close",
                        serde_json::json!({"streamId":active.to_base64url()}),
                    )
                    .await;
                assert_eq!(reader_closed["ok"], true, "{reader_closed}");
            }
            capacity_expect(f, 145, 50, 0).await;
            assert_eq!(
                capacity_method_count(f, "session/new"),
                50 + usize::from(create_won)
            );
            assert_eq!(
                capacity_method_count(f, "session/resume"),
                usize::from(replay_won)
            );
            assert_eq!(capacity_method_count(f, "session/close"), 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_closing_reader_during_actual_replay_does_not_return_root_slot() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 150).await;
            capacity_route_target(f, &native, r3_id(27)).await;
            let release = capacity_register_release(f, 152, r3_id(27)).await;
            let closed = capacity_release(f, 153, &release).await;
            assert_eq!(closed["result"]["state"], "released", "{closed}");
            capacity_expect(f, 154, 0, 0).await;
            let gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold").unwrap();
            let opened = f.call(0, 155, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("capacity-entered-replay").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual protocol replay entered peer before response");
            let view_closed = f
                .call(
                    0,
                    156,
                    "view.close",
                    serde_json::json!({"streamId":opened["result"]["streamId"]}),
                )
                .await;
            assert_eq!(view_closed["ok"], true, "{view_closed}");
            let held = capacity_inspect(f, 157).await;
            assert_eq!(
                held["occupied"], 1,
                "reader cancellation cannot prove native resource absence: {held}"
            );
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            drop(gate);
            capacity_expect(f, 158, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/load"), 1);
        })
    })
    .await;
}

async fn capacity_unknown_new_response(mode: &'static str) {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let flag = f.area.directory.join("capacity-new-unknown-root");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(flag, mode.as_bytes()).unwrap();
            let registered = f
                .call(
                    0,
                    160,
                    "source.session.register",
                    serde_json::json!({
                        "selection":f.selection_json(),"operation":{"kind":"create"}
                    }),
                )
                .await;
            assert_eq!(registered["ok"], true, "{registered}");
            let operation = registered["result"]["sourceOperationId"].clone();
            let response = f
                .call(
                    0,
                    161,
                    "source.session.execute",
                    serde_json::json!({
                        "sourceOperationId":operation
                    }),
                )
                .await;
            assert_eq!(response["ok"], false, "{response}");
            assert_eq!(
                response["error"]["code"], "session_tree_unverified",
                "{response}"
            );
            let status = f
                .call(
                    0,
                    166,
                    "source.session.status",
                    serde_json::json!({
                        "sourceOperationId":operation
                    }),
                )
                .await;
            assert_eq!(status["ok"], true, "{status}");
            assert_ne!(status["result"]["outcome"], "succeeded", "{status}");
            assert_eq!(status["result"]["response"]["ack"], "confirmed", "{status}");
            assert_eq!(status["result"]["resultAvailable"], true, "{status}");
            let native = status["result"]["response"]["nativeSessionId"]
                .as_str()
                .unwrap()
                .to_owned();
            let document = f
                .call(
                    1,
                    162,
                    "source.session.result.open",
                    serde_json::json!({
                        "sourceOperationId":operation
                    }),
                )
                .await;
            assert_eq!(document["ok"], true, "{document}");
            let stream =
                Id128::parse_base64url(document["result"]["streamId"].as_str().unwrap()).unwrap();
            let raw = r3_drain_document(&mut f.peers[1].socket, stream).await;
            let preserved: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            assert_eq!(preserved["sessionId"], native);
            if mode == "top" {
                assert_eq!(
                    preserved["x-capacity-relation"]["parent"],
                    "unverified-parent"
                );
            } else {
                assert_eq!(
                    preserved["_meta"]["x-capacity-relation"]["parent"],
                    "unverified-parent"
                );
            }
            capacity_expect(f, 163, 1, 1).await;
            let retry = f
                .call(
                    0,
                    164,
                    "source.session.execute",
                    serde_json::json!({
                        "sourceOperationId":operation
                    }),
                )
                .await;
            assert_eq!(retry["ok"], false, "{retry}");
            assert_eq!(retry["error"]["code"], "session_tree_unverified", "{retry}");
            capacity_route_target(f, &native, r3_id(28)).await;
            capacity_submit_execution_target(f, r3_id(28)).await;
            let load = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(28))
                .await;
            assert!(
                matches!(
                    load,
                    Err(crate::domain::native_dispatch::DispatchError::SessionTreeUnverified)
                ),
                "unknown result relationships cannot establish independent root: {load:?}"
            );
            capacity_expect(f, 165, 1, 1).await;
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_unknown_new_top_extension_preserves_raw_without_root() {
    capacity_unknown_new_response("top").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_unknown_new_meta_extension_preserves_raw_without_root() {
    capacity_unknown_new_response("meta").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_old_release_retry_cannot_refund_new_generation() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 170).await;
            capacity_route_target(f, &native, r3_id(29)).await;
            let release_a = capacity_register_release(f, 172, r3_id(29)).await;
            let closed_a = capacity_release(f, 173, &release_a).await;
            assert_eq!(closed_a["result"]["state"], "released", "{closed_a}");
            capacity_expect(f, 174, 0, 0).await;
            let opened = f.call(0, 175, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            let reader = f
                .call(
                    0,
                    176,
                    "view.close",
                    serde_json::json!({
                        "streamId":opened["result"]["streamId"]
                    }),
                )
                .await;
            assert_eq!(reader["ok"], true, "{reader}");
            capacity_expect(f, 177, 1, 0).await;
            let release_b = capacity_register_release(f, 178, r3_id(29)).await;
            assert_ne!(release_a, release_b);
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
                "CREATE TRIGGER capacity_old_release_fault \
                BEFORE UPDATE OF phase,error_code ON node_native_release_operations \
                WHEN OLD.phase='releasing' \
                BEGIN SELECT RAISE(ABORT,'generation fixture'); END",
            )
            .execute(&pool)
            .await
            .unwrap();
            let observed = AssertUnwindSafe(async {
                let failed_b = capacity_release(f, 179, &release_b).await;
                let before = capacity_inspect(f, 180).await;
                let old_retry = capacity_release(f, 181, &release_a).await;
                let after = capacity_inspect(f, 182).await;
                (failed_b, before, old_retry, after)
            })
            .catch_unwind()
            .await;
            sqlx::query("DROP TRIGGER capacity_old_release_fault")
                .execute(&pool)
                .await
                .expect("unconditional owned trigger cleanup");
            let (failed_b, before, old_retry, after) = match observed {
                Ok(value) => value,
                Err(error) => std::panic::resume_unwind(error),
            };
            assert_eq!(
                failed_b["error"]["code"], "necessary_storage_unavailable",
                "{failed_b}"
            );
            assert_eq!(before["occupied"], 1, "{before}");
            assert_eq!(before["closedPending"], 1, "{before}");
            assert_eq!(old_retry["result"]["state"], "released", "{old_retry}");
            assert_eq!(
                after["occupied"], 1,
                "old Released A only coordinates A's fixed reservations, never later B: {after}"
            );
            assert_eq!(after["closedPending"], 1, "{after}");
            let retry_b = capacity_release(f, 183, &release_b).await;
            assert_eq!(retry_b["result"]["state"], "released", "{retry_b}");
            capacity_expect(f, 184, 0, 0).await;
            capacity_release(f, 185, &release_a).await;
            capacity_expect(f, 186, 0, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 2);
            assert_eq!(capacity_method_count(f, "session/load"), 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_replay_error_after_actual_write_preserves_original_owner() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 190).await;
            capacity_route_target(f, &native, r3_id(30)).await;
            let release = capacity_register_release(f, 192, r3_id(30)).await;
            assert_eq!(
                capacity_release(f, 193, &release).await["result"]["state"],
                "released"
            );
            let flag = f.area.directory.join("capacity-replay-error");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(flag, b"owned peer error").unwrap();
            let opened = f.call(0, 194, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            // Wait for the actual original response failure to reach the view.
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
            let (frames, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream).await;
            assert!(
                !rebound,
                "failed replay must not be presented as a completed replacement"
            );
            let reports_failure = |kind: RecordKind, payload: &[u8]| {
                if kind != RecordKind::Status {
                    return false;
                }
                let status: serde_json::Value = serde_json::from_slice(payload).unwrap();
                status["limitation"] == "requested_replay_failed"
                    || (!status["failure"].is_null() && status["failure"].is_string())
            };
            if !frames
                .iter()
                .any(|(kind, payload)| reports_failure(*kind, payload))
            {
                timeout(Duration::from_secs(12), async {
                    loop {
                        let bytes = r3_next_binary(&mut f.peers[0].socket).await;
                        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
                        assert_eq!(header.stream(), stream);
                        let failed = reports_failure(header.record_kind().unwrap(), payload);
                        r3_send_binary(
                            &mut f.peers[0].socket,
                            TransferHeader::credit(stream, header.packet())
                                .encode(&[])
                                .unwrap(),
                        )
                        .await;
                        if failed {
                            break;
                        }
                    }
                })
                .await
                .expect("actual original replay error must reach stream status");
            }
            // The stream socket may still have legitimately queued binary status
            // records. Query through the other actual authenticated connection.
            let capacity = f
                .call(1, 195, "session.capacity.inspect", serde_json::json!({}))
                .await;
            assert_eq!(capacity["ok"], true, "{capacity}");
            assert_eq!(capacity["result"]["occupied"], 1, "{capacity}");
            assert_eq!(capacity["result"]["reserved"], 0, "{capacity}");
            let events = capacity_events(f);
            let new = events
                .iter()
                .find(|e| e["method"] == "session/new")
                .unwrap();
            let load = events
                .iter()
                .find(|e| e["method"] == "session/load")
                .unwrap();
            assert_eq!(new["pid"], load["pid"]);
            assert_eq!(load["native"], native);
            assert_eq!(
                capacity_method_count(f, "session/close"),
                1,
                "RPC failure does not establish physical close"
            );
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_reader_disconnect_after_replay_write_retains_slot() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 200).await;
            capacity_route_target(f, &native, r3_id(32)).await;
            let release = capacity_register_release(f, 202, r3_id(32)).await;
            assert_eq!(
                capacity_release(f, 203, &release).await["result"]["state"],
                "released"
            );
            let gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold").unwrap();
            let opened = f.call(1, 204, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            f.peers[1].socket.close(None).await.unwrap();
            capacity_expect(f, 205, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            drop(gate);
            capacity_expect(f, 206, 1, 0).await;
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_foreign_actor_cannot_release_or_refund_original_operation() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 210).await;
            capacity_route_target(f, &native, r3_id(33)).await;
            let operation = capacity_register_release(f, 212, r3_id(33)).await;
            let foreign_status = f
                .call(
                    1,
                    213,
                    "native.release.status",
                    serde_json::json!({
                        "releaseOperationId":operation
                    }),
                )
                .await;
            assert_eq!(
                foreign_status["error"]["code"], "invalid_release",
                "{foreign_status}"
            );
            let foreign_release = f
                .call(
                    1,
                    214,
                    "native.release",
                    serde_json::json!({
                        "releaseOperationId":operation
                    }),
                )
                .await;
            assert_eq!(foreign_release["ok"], false, "{foreign_release}");
            assert_eq!(
                foreign_release["error"]["code"], "invalid_release",
                "{foreign_release}"
            );
            capacity_expect(f, 215, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let original = capacity_release(f, 216, &operation).await;
            assert_eq!(original["result"]["state"], "released", "{original}");
            capacity_expect(f, 217, 0, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 1);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_source_disconnect_after_load_keeps_slot_until_necessary_convergence() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let registered = f.call(0, 220, "source.session.register", serde_json::json!({
                "selection":f.selection_json(),"operation":{"kind":"create"}
            })).await;
            assert_eq!(registered["ok"], true, "{registered}");
            let operation = registered["result"]["sourceOperationId"].clone();
            let created = f.call(0, 221, "source.session.execute", serde_json::json!({
                "sourceOperationId":operation
            })).await;
            assert_eq!(created["result"]["outcome"], "succeeded", "{created}");
            let native = created["result"]["response"]["nativeSessionId"].as_str().unwrap().to_owned();
            capacity_route_target(f, &native, r3_id(34)).await;
            let release = capacity_register_release(f, 222, r3_id(34)).await;
            assert_eq!(capacity_release(f, 223, &release).await["result"]["state"], "released");
            let flag = f.area.directory.join("capacity-replay-disconnect");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(flag, b"exit after actual replay write").unwrap();
            let gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold").unwrap();
            let opened = f.call(0, 224, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            capacity_expect(f, 225, 1, 0).await;
            let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test().clone();
            sqlx::query("CREATE TRIGGER capacity_source_close_fault \
                BEFORE UPDATE OF lifecycle ON node_native_source_instances \
                WHEN NEW.lifecycle='closed' \
                BEGIN SELECT RAISE(ABORT,'source convergence fixture'); END")
                .execute(&pool).await.unwrap();
            let observed = AssertUnwindSafe(async {
                drop(gate);
                timeout(Duration::from_secs(12), async {
                    loop {
                        let diagnostics = f.node.as_ref().unwrap().session_services.as_ref().unwrap()
                            .native.source_close_diagnostics();
                        if diagnostics.iter().any(|source| source.physical_present
                            && source.metadata_close.as_ref().is_some_and(Result::is_err)) {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                }).await.expect("actual owned source disconnect closes physically but necessary SQL fails");
                let snapshot = capacity_inspect(f, 226).await;
                let closed_count = capacity_method_count(f, "session/close");
                (snapshot, closed_count)
            }).catch_unwind().await;
            sqlx::query("DROP TRIGGER capacity_source_close_fault")
                .execute(&pool).await.expect("remove owned source fault before node cleanup");
            // Query the original operation requests the original source metadata retry.
            let status = f.call(0, 227, "source.session.status", serde_json::json!({
                "sourceOperationId":operation
            })).await;
            let (held, closed_count) = match observed {
                Ok(value) => value, Err(error) => std::panic::resume_unwind(error),
            };
            assert_eq!(held["occupied"], 1,
                "EOF/physical source closure alone cannot refund without necessary source fact: {held}");
            assert_eq!(closed_count, 1, "source disconnect is not an extra per-native release");
            assert_eq!(status["ok"], true, "{status}");
            timeout(Duration::from_secs(12), async {
                loop {
                    let snapshot = capacity_inspect(f, 228).await;
                    if snapshot["occupied"] == 0 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("original full-source necessary coordination refunds once");
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            assert_eq!(capacity_method_count(f, "session/close"), 1);
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_reopened_source_release_targets_new_owned_instance() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 180).await;
            capacity_route_target(f, &native, r3_id(35)).await;
            let first_release = capacity_register_release(f, 182, r3_id(35)).await;
            assert_eq!(
                capacity_release(f, 183, &first_release).await["result"]["state"],
                "released"
            );
            capacity_expect(f, 184, 0, 0).await;
            let flag = f.area.directory.join("capacity-replay-disconnect");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(&flag, b"exit after actual replay write").unwrap();
            let gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold").unwrap();
            let opened = f.call(0, 185, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            capacity_expect(f, 186, 1, 0).await;
            let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test().clone();
            let old_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            let first_load = capacity_events(f).into_iter()
                .find(|event| event["method"] == "session/load").unwrap();
            drop(gate);
            timeout(Duration::from_secs(12), async {
                loop {
                    let old_closed: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_native_source_instances \
                         WHERE instance_id=? AND lifecycle='closed'"
                    ).bind(&old_instance).fetch_one(&pool).await.unwrap();
                    let snapshot = capacity_inspect(f, 187).await;
                    if old_closed == 1 && snapshot["occupied"] == 0 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("original physical source and necessary metadata fully converge");
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            std::fs::remove_file(&flag).unwrap();
            let reopened = f.call(1, 188, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(reopened["ok"], true, "{reopened}");
            let stream = Id128::parse_base64url(
                reopened["result"]["streamId"].as_str().unwrap()
            ).unwrap();
            let (_, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, stream).await;
            let active = if rebound {
                let response = f.call(1, 189, "view.rebind", serde_json::json!({
                    "streamId":stream.to_base64url()
                })).await;
                assert_eq!(response["ok"], true, "{response}");
                let next = Id128::parse_base64url(
                    response["result"]["streamId"].as_str().unwrap()
                ).unwrap();
                let (_, again) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, next).await;
                assert!(!again);
                next
            } else { stream };
            let reader_closed = f.call(1, 190, "view.close", serde_json::json!({
                "streamId":active.to_base64url()
            })).await;
            assert_eq!(reader_closed["ok"], true, "{reader_closed}");
            capacity_expect(f, 191, 1, 0).await;
            let new_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            assert_ne!(new_instance, old_instance);
            let loads: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/load").collect();
            assert_eq!(loads.len(), 2);
            let new_loads: Vec<_> = loads.iter()
                .filter(|event| event["pid"] != first_load["pid"]).collect();
            assert_eq!(new_loads.len(), 1, "one actual load belongs to the new process");
            let new_pid = new_loads[0]["pid"].clone();
            assert_ne!(new_pid, first_load["pid"]);
            assert_eq!(new_loads[0]["native"], native);
            let closes_before: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(closes_before.len(), 1);
            assert_eq!(closes_before[0]["pid"], first_load["pid"]);
            assert_eq!(capacity_method_count(f, "initialize"), 2);
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            let second_release = capacity_register_release(f, 192, r3_id(35)).await;
            let released = capacity_release(f, 193, &second_release).await;
            assert_eq!(released["result"]["state"], "released",
                "release must close the current owned instance after original source convergence: {released}");
            capacity_expect(f, 194, 0, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 2);
            let closes_after: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(closes_after.iter().filter(|event| event["pid"] == first_load["pid"]).count(), 1,
                "the closed original process must not receive another close");
            assert_eq!(closes_after.iter().filter(|event| event["pid"] == new_pid).count(), 1,
                "the new operation must actually close the new process");
            assert_eq!(capacity_method_count(f, "session/load"), 2);
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_source_replay_after_execution_release_closes_current_generation() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 195).await;
            capacity_route_target(f, &native, r3_id(36)).await;
            let release_prompt = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-prompt"), b"hold").unwrap();
            capacity_submit_execution_target(f, r3_id(36)).await;
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(r3_id(36))
                .await
                .expect("actual execution installs its original ResourceSlot");
            capacity_wait_method(f, "session/prompt", 1).await;
            capacity_expect(f, 197, 1, 0).await;
            assert_eq!(capacity_method_count(f, "session/resume"), 1);
            let pool = f
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .pool_for_test()
                .clone();
            let original_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            drop(release_prompt);
            let release_a = capacity_register_release(f, 198, r3_id(36)).await;
            let closed_a = capacity_release(f, 199, &release_a).await;
            assert_eq!(closed_a["result"]["state"], "released", "{closed_a}");
            capacity_expect(f, 200, 0, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            std::fs::remove_file(f.area.directory.join("capacity-release-replay")).unwrap();
            let replay_gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(
                f.area.directory.join("capacity-hold-replay"),
                b"hold original replay B",
            )
            .unwrap();
            let opened = f.call(1, 201, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"explicit_refresh"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            let stream =
                Id128::parse_base64url(opened["result"]["streamId"].as_str().unwrap()).unwrap();
            let (_, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, stream).await;
            assert!(!rebound, "held original response cannot complete");
            drop(replay_gate);
            capacity_wait_completed_stream_rebound(&mut f.peers[1].socket, stream).await;
            let active = {
                let response = f
                    .call(
                        1,
                        202,
                        "view.rebind",
                        serde_json::json!({
                            "streamId":stream.to_base64url()
                        }),
                    )
                    .await;
                assert_eq!(response["ok"], true, "{response}");
                assert_eq!(
                    response["result"]["status"]["mode"], "replay_complete",
                    "{response}"
                );
                let next = Id128::parse_base64url(response["result"]["streamId"].as_str().unwrap())
                    .unwrap();
                let (_, again) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, next).await;
                assert!(!again);
                next
            };
            let reader = f
                .call(
                    1,
                    203,
                    "view.close",
                    serde_json::json!({
                        "streamId":active.to_base64url()
                    }),
                )
                .await;
            assert_eq!(reader["ok"], true, "{reader}");
            capacity_expect(f, 204, 1, 0).await;
            let reservation_b: String = sqlx::query_scalar(
                "SELECT reservation_id FROM node_session_capacity WHERE released=0",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let instance_b: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE reservation_id=?",
            )
            .bind(&reservation_b)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(instance_b, original_instance);
            assert_eq!(capacity_method_count(f, "initialize"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            let release_b = capacity_register_release(f, 205, r3_id(36)).await;
            assert_ne!(release_a, release_b);
            let closed_b = capacity_release(f, 206, &release_b).await;
            let snapshot = capacity_inspect(f, 207).await;
            let record: String = sqlx::query_scalar(
                "SELECT json_object('physical',physically_closed,'released',released) \
                 FROM node_session_capacity WHERE reservation_id=?",
            )
            .bind(&reservation_b)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                capacity_method_count(f, "session/close"),
                2,
                "new SourceOnly generation B needs its own actual close; result={closed_b}, \
                 capacity={snapshot}, reservationB={record}"
            );
            assert_eq!(closed_b["result"]["state"], "released", "{closed_b}");
            assert_eq!(snapshot["occupied"], 0, "{snapshot}");
            let record: serde_json::Value = serde_json::from_str(&record).unwrap();
            assert_eq!(record["physical"], 1);
            assert_eq!(record["released"], 1);
            let repeated = capacity_release(f, 208, &release_b).await;
            assert_eq!(repeated["result"]["state"], "released", "{repeated}");
            capacity_expect(f, 209, 0, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 2);
        })
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_original_pending_release_finishes_without_touching_reopened_owner() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 210).await;
            capacity_route_target(f, &native, r3_id(37)).await;
            let release_a = capacity_register_release(f, 212, r3_id(37)).await;
            let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test().clone();
            let old_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            sqlx::query(
                "CREATE TRIGGER capacity_cross_instance_finish_fault \
                 BEFORE UPDATE OF phase,error_code ON node_native_release_operations \
                 WHEN OLD.phase='releasing' \
                 BEGIN SELECT RAISE(ABORT,'original finish fixture'); END"
            ).execute(&pool).await.unwrap();
            let observed = AssertUnwindSafe(async {
                capacity_release(f, 213, &release_a).await
            }).catch_unwind().await;
            sqlx::query("DROP TRIGGER capacity_cross_instance_finish_fault")
                .execute(&pool).await.expect("remove owned necessary finish fault");
            let failed = match observed {
                Ok(value) => value, Err(error) => std::panic::resume_unwind(error),
            };
            assert_eq!(failed["error"]["code"], "necessary_storage_unavailable", "{failed}");
            let pending_a = capacity_inspect(f, 214).await;
            assert_eq!(pending_a["occupied"], 1, "{pending_a}");
            assert_eq!(pending_a["reserved"], 0, "{pending_a}");
            assert_eq!(pending_a["closedPending"], 1, "{pending_a}");
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            let pid_a = capacity_events(f).into_iter()
                .find(|event| event["method"] == "session/close").unwrap()["pid"].clone();
            let phase: String = sqlx::query_scalar(
                "SELECT phase FROM node_native_release_operations WHERE operation_id=?"
            ).bind(&release_a).fetch_one(&pool).await.unwrap();
            assert_eq!(phase, "releasing");
            let flag = f.area.directory.join("capacity-list-disconnect");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(&flag, b"owned original source EOF on real list").unwrap();
            let _disconnected = capacity_source_operation(
                f, 215, serde_json::json!({"kind":"discover","cursor":null})
            ).await;
            capacity_wait_method(f, "session/list", 1).await;
            timeout(Duration::from_secs(12), async {
                loop {
                    let old_closed: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_native_source_instances \
                         WHERE instance_id=? AND lifecycle='closed'"
                    ).bind(&old_instance).fetch_one(&pool).await.unwrap();
                    let held: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_session_capacity \
                         WHERE instance_id=? AND (released=0 OR physically_closed=0)"
                    ).bind(&old_instance).fetch_one(&pool).await.unwrap();
                    if old_closed == 1 && held == 0 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("original source closed and every original reservation confirmed");
            capacity_expect(f, 217, 0, 0).await;
            std::fs::remove_file(&flag).unwrap();
            let replay_gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold new instance B response").unwrap();
            let opened = f.call(1, 218, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            let stream = Id128::parse_base64url(
                opened["result"]["streamId"].as_str().unwrap()
            ).unwrap();
            let (_, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, stream).await;
            assert!(!rebound, "held original response cannot complete");
            drop(replay_gate);
            capacity_wait_completed_stream_rebound(&mut f.peers[1].socket, stream).await;
            let active = {
                let response = f.call(1, 219, "view.rebind", serde_json::json!({
                    "streamId":stream.to_base64url()
                })).await;
                assert_eq!(response["ok"], true, "{response}");
                assert_eq!(response["result"]["status"]["mode"], "replay_complete", "{response}");
                let next = Id128::parse_base64url(
                    response["result"]["streamId"].as_str().unwrap()
                ).unwrap();
                let (_, again) =
                    r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, next).await;
                assert!(!again);
                next
            };
            let reader = f.call(1, 220, "view.close", serde_json::json!({
                "streamId":active.to_base64url()
            })).await;
            assert_eq!(reader["ok"], true, "{reader}");
            capacity_expect(f, 221, 1, 0).await;
            let new_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            assert_ne!(new_instance, old_instance);
            let original_b = capacity_crash_original_snapshot(&pool, &new_instance).await;
            let pid_b = capacity_events(f).into_iter()
                .find(|event| event["method"] == "session/load").unwrap()["pid"].clone();
            assert_ne!(pid_b, pid_a);
            assert_eq!(capacity_method_count(f, "initialize"), 2);
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            let retried_a = capacity_release(f, 222, &release_a).await;
            let retained = capacity_inspect(f, 223).await;
            let after_b = capacity_crash_original_snapshot(&pool, &new_instance).await;
            assert_eq!(capacity_method_count(f, "session/close"), 1,
                "retry A must never close B; A={retried_a}, capacity={retained}, B={after_b}");
            assert_eq!(retained["occupied"], 1, "{retained}");
            assert_eq!(after_b, original_b, "retry A cannot rewrite the new owner or reservations");
            assert_eq!(retried_a["result"]["state"], "released",
                "confirmed SourceClosed original set is finish-only, never an operation on B: {retried_a}");
            let again_a = capacity_release(f, 224, &release_a).await;
            assert_eq!(again_a["result"]["state"], "released", "{again_a}");
            assert_eq!(capacity_method_count(f, "session/close"), 1);
            let before_b: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(before_b.iter().filter(|event| event["pid"] == pid_a).count(), 1);
            assert_eq!(before_b.iter().filter(|event| event["pid"] == pid_b).count(), 0,
                "neither retry of A may close B");
            let release_b = capacity_register_release(f, 225, r3_id(37)).await;
            let closed_b = capacity_release(f, 226, &release_b).await;
            assert_eq!(closed_b["result"]["state"], "released", "{closed_b}");
            capacity_expect(f, 227, 0, 0).await;
            let closes: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(closes.len(), 2);
            assert_eq!(closes.iter().filter(|event| event["pid"] == pid_a).count(), 1,
                "B's operation must not close the original process again");
            assert_eq!(closes.iter().filter(|event| event["pid"] == pid_b).count(), 1,
                "only B's own operation closes B");
        })
    }).await;
}

struct CapacityMetadataGateRelease(Arc<crate::test_support::AsyncBarrier>);

impl Drop for CapacityMetadataGateRelease {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_rb001_source_closes_after_owned_basis_keeps_original_release_retryable() {
    r3_wss_case("normal", false, |f| {
        Box::pin(async move {
            let native = capacity_create(f, 230).await;
            capacity_route_target(f, &native, r3_id(38)).await;
            let release_a = capacity_register_release(f, 232, r3_id(38)).await;
            let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test().clone();
            let old_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            let pid_a = capacity_events(f).into_iter()
                .find(|event| event["method"] == "session/new").unwrap()["pid"].clone();
            let barrier = f.node.as_ref().unwrap().storage.as_ref().unwrap()
                .request_registry().native_metadata_hooks_for_test()
                .pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::CapacityReleaseOwnedAfterCommit)
                .unwrap();
            let metadata_gate = CapacityMetadataGateRelease(barrier.clone());
            let pending_request = Id128::new([233; 16]);
            // Peer 2 is another authenticated connection of actor A. Keep its
            // pending response separate from normal management and B's stream.
            send_json(&mut f.peers[2].socket, &serde_json::json!({
                "v":1,"type":"command","requestId":pending_request,
                "action":"native.release","payload":{"releaseOperationId":release_a}
            })).await;
            timeout(Duration::from_secs(5), barrier.wait_until_entered()).await
                .expect("real release obtained committed Owned basis before native work");
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let initial_phase: String = sqlx::query_scalar(
                "SELECT phase FROM node_native_release_operations WHERE operation_id=?"
            ).bind(&release_a).fetch_one(&pool).await.unwrap();
            assert_eq!(initial_phase, "releasing");
            let flag = f.area.directory.join("capacity-list-disconnect");
            let _cleanup = CapacityRemoveFixtureFile(flag.clone());
            std::fs::write(&flag, b"original EOF after committed Owned basis").unwrap();
            let _disconnected = capacity_source_operation(
                f, 234, serde_json::json!({"kind":"discover","cursor":null})
            ).await;
            capacity_wait_method(f, "session/list", 1).await;
            timeout(Duration::from_secs(5), async {
                loop {
                    let old_closed: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_native_source_instances \
                         WHERE instance_id=? AND lifecycle='closed' \
                         AND close_evidence_fingerprint IS NOT NULL"
                    ).bind(&old_instance).fetch_one(&pool).await.unwrap();
                    let unsettled: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_session_capacity_releases r \
                         JOIN node_session_capacity c ON c.reservation_id=r.reservation_id \
                         WHERE r.operation_id=? AND (c.released=0 OR c.physically_closed=0)"
                    ).bind(&release_a).fetch_one(&pool).await.unwrap();
                    let original_count: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM node_session_capacity_releases WHERE operation_id=?"
                    ).bind(&release_a).fetch_one(&pool).await.unwrap();
                    if old_closed == 1 && original_count > 0 && unsettled == 0 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("original source and nonempty original release set fully settle");
            capacity_expect(f, 236, 0, 0).await;
            std::fs::remove_file(&flag).unwrap();
            let replay_gate = CapacityGateRelease(f.area.directory.clone());
            std::fs::write(f.area.directory.join("capacity-hold-replay"), b"hold new B response").unwrap();
            let opened = f.call(1, 237, "view.open_source", serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":native,"reason":"first_open"
            })).await;
            assert_eq!(opened["ok"], true, "{opened}");
            capacity_wait_method(f, "session/load", 1).await;
            let stream = Id128::parse_base64url(
                opened["result"]["streamId"].as_str().unwrap()
            ).unwrap();
            let (_, rebound) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, stream).await;
            assert!(!rebound, "held original response cannot complete");
            drop(replay_gate);
            capacity_wait_completed_stream_rebound(&mut f.peers[1].socket, stream).await;
            let rebound = f.call(1, 238, "view.rebind", serde_json::json!({
                "streamId":stream.to_base64url()
            })).await;
            assert_eq!(rebound["ok"], true, "{rebound}");
            assert_eq!(rebound["result"]["status"]["mode"], "replay_complete", "{rebound}");
            let active = Id128::parse_base64url(
                rebound["result"]["streamId"].as_str().unwrap()
            ).unwrap();
            let (_, again) =
                r3_drain_view_snapshot_or_rebound(&mut f.peers[1].socket, active).await;
            assert!(!again);
            let reader_closed = f.call(1, 239, "view.close", serde_json::json!({
                "streamId":active.to_base64url()
            })).await;
            assert_eq!(reader_closed["ok"], true, "{reader_closed}");
            capacity_expect(f, 240, 1, 0).await;
            let new_instance: String = sqlx::query_scalar(
                "SELECT instance_id FROM node_session_capacity WHERE released=0"
            ).fetch_one(&pool).await.unwrap();
            assert_ne!(new_instance, old_instance);
            let pid_b = capacity_events(f).into_iter()
                .find(|event| event["method"] == "session/load").unwrap()["pid"].clone();
            assert_ne!(pid_b, pid_a);
            assert_eq!(capacity_method_count(f, "initialize"), 2);
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 1);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let original_b = capacity_crash_original_snapshot(&pool, &new_instance).await;
            drop(metadata_gate);
            let failed_a = next_json(&mut f.peers[2].socket, Duration::from_secs(5)).await;
            assert_eq!(failed_a["requestId"], pending_request.to_base64url(), "{failed_a}");
            assert_eq!(failed_a["ok"], false, "{failed_a}");
            let retained = capacity_inspect(f, 241).await;
            assert_eq!(retained["occupied"], 1, "{retained}");
            assert_eq!(capacity_crash_original_snapshot(&pool, &new_instance).await, original_b);
            assert_eq!(capacity_method_count(f, "session/close"), 0,
                "stale Owned basis must not close either process");
            let phase: String = sqlx::query_scalar(
                "SELECT phase FROM node_native_release_operations WHERE operation_id=?"
            ).bind(&release_a).fetch_one(&pool).await.unwrap();
            assert_eq!(phase, "releasing",
                "owner convergence after committed Owned basis must remain retryable; A={failed_a}");
            assert_eq!(failed_a["error"]["code"], "release_unconfirmed",
                "the admitted old owner stopped; a metadata pause timeout is not this path");
            let retry_a = capacity_release(f, 242, &release_a).await;
            assert_eq!(retry_a["result"]["state"], "released", "{retry_a}");
            let repeated_a = capacity_release(f, 243, &release_a).await;
            assert_eq!(repeated_a["result"]["state"], "released", "{repeated_a}");
            capacity_expect(f, 244, 1, 0).await;
            assert_eq!(capacity_crash_original_snapshot(&pool, &new_instance).await, original_b);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let release_b = capacity_register_release(f, 245, r3_id(38)).await;
            let closed_b = capacity_release(f, 246, &release_b).await;
            assert_eq!(closed_b["result"]["state"], "released", "{closed_b}");
            capacity_expect(f, 247, 0, 0).await;
            let closes: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(closes.len(), 1);
            assert_eq!(closes[0]["pid"], pid_b, "only B's own operation closes B");
            assert_eq!(closes[0]["native"], native);
        })
    }).await;
}
