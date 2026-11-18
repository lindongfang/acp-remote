// Independent real Node/WSS admissions. Every verified root comes from actual
// standard new; only routing metadata is written by the existing R3 helper.
#[derive(Clone, Copy)]
enum RtAdmission {
    Create,
    Execution,
    Replay,
}

async fn rt_full_admission_case(route: RtAdmission, eligible: bool) {
    r3_wss_case("source-create-replay-state", false, move |f| {
        Box::pin(async move {
            let reopening = if matches!(route, RtAdmission::Create) {
                None
            } else {
                let native = capacity_create(f, 151).await;
                capacity_route_target(f, &native, r3_id(45)).await;
                let operation = capacity_register_release(f, 153, r3_id(45)).await;
                let result = capacity_release(f, 154, &operation).await;
                assert_eq!(result["result"]["state"], "released", "{result}");
                Some(native)
            };
            let mut candidate = None;
            for i in 0..50_u8 {
                if i == 0 && eligible {
                    std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle")
                        .unwrap();
                }
                let native = capacity_create(f, 1 + 2 * i).await;
                if i == 0 {
                    candidate = Some(native);
                }
            }
            capacity_expect(f, 156, 50, 0).await;
            let before_close = capacity_method_count(f, "session/close");
            let before_new = capacity_method_count(f, "session/new");
            let before_load = capacity_method_count(f, "session/load");
            let before_resume = capacity_method_count(f, "session/resume");
            match route {
                RtAdmission::Create => {
                    let result = capacity_source_operation(
                        f, 157, serde_json::json!({"kind":"create"}),
                    ).await;
                    if eligible {
                        assert_eq!(result["ok"], true, "{result}");
                        assert_eq!(result["result"]["outcome"], "succeeded", "{result}");
                        assert_eq!(capacity_method_count(f, "session/new"), before_new + 1);
                    } else {
                        assert!(result.to_string().contains("session_capacity_full"), "{result}");
                        assert!(result["ok"] == false || result["result"]["outcome"] != "succeeded");
                        assert_eq!(capacity_method_count(f, "session/new"), before_new);
                    }
                }
                RtAdmission::Execution => {
                    capacity_submit_execution_target(f, r3_id(45)).await;
                    let native = f.node.as_ref().unwrap().session_services.as_ref().unwrap().native.clone();
                    if eligible {
                        // Consume already started the original queue dispatcher. Observe
                        // that same admitted input, rather than start a second pressure
                        // attempt while it has not yet reserved its target.
                        capacity_wait_method(f, "session/resume", before_resume + 1).await;
                        let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test();
                        timeout(Duration::from_secs(10), async {
                            loop {
                                let loaded: i64 = sqlx::query_scalar(
                                    "SELECT count(*) FROM node_native_resource_bindings WHERE target_id=? AND lifecycle='loaded' AND load_transport_state='written'",
                                ).bind(r3_id(45).encode()).fetch_one(pool).await.unwrap();
                                if loaded == 1 { break; }
                                tokio::task::yield_now().await;
                            }
                        }).await.expect("the original accepted execution must durably load its actual target");
                        capacity_wait_method(f, "session/prompt", 1).await;
                        // Once the original target is reserved and loaded, another
                        // caller must reuse it even though global capacity is full.
                        let reused = native.ensure_target(r3_id(45)).await;
                        assert!(reused.is_ok(), "{reused:?}");
                        assert_eq!(capacity_method_count(f, "session/resume"), before_resume + 1);
                    } else {
                        let result = native.ensure_target(r3_id(45)).await;
                        assert!(
                            matches!(result, Err(crate::domain::native_dispatch::DispatchError::SessionCapacityFull)),
                            "{result:?}"
                        );
                        assert_eq!(capacity_method_count(f, "session/resume"), before_resume);
                    }
                }
                RtAdmission::Replay => {
                    let result = f.call(
                        0, 157, "view.open_source",
                        serde_json::json!({"selection":f.selection_json(),"nativeSessionId":reopening.as_ref().unwrap(),"reason":"first_open"}),
                    ).await;
                    if eligible {
                        assert_eq!(result["ok"], true, "{result}");
                        capacity_wait_method(f, "session/resume", before_resume + 1).await;
                        let stream = Id128::parse_base64url(result["result"]["streamId"].as_str().unwrap()).unwrap();
                        let (records, rebound) = r3_drain_view_snapshot_or_rebound(&mut f.peers[0].socket, stream).await;
                        assert!(!records.is_empty() || rebound, "actual replay must deliver a nonempty body or its completed-read rebound");
                        let closed = f.call(0, 158, "view.close", serde_json::json!({"streamId":stream.to_base64url()})).await;
                        assert_eq!(closed["ok"], true, "{closed}");
                    } else {
                        assert_eq!(result["ok"], false, "{result}");
                        assert_eq!(result["error"]["code"], "session_capacity_full", "{result}");
                        assert_eq!(capacity_method_count(f, "session/resume"), before_resume);
                    }
                }
            }
            let closes: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/close").collect();
            assert_eq!(closes.len(), before_close + usize::from(eligible));
            if eligible {
                assert_eq!(closes.last().unwrap()["native"], candidate.unwrap());
                let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test();
                let finished: i64 = sqlx::query_scalar("SELECT count(*) FROM node_session_automatic_releases WHERE phase='finished'")
                    .fetch_one(pool).await.unwrap();
                assert_eq!(finished, 1, "actual close and necessary finish precede admission");
            }
            assert_eq!(capacity_method_count(f, "session/load"), before_load);
            assert_eq!(capacity_method_count(f, "session/new"), before_new + usize::from(eligible && matches!(route, RtAdmission::Create)));
            capacity_expect(f, 160, 50, 0).await;
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_002_create_full_known_busy_has_zero_close_and_zero_new() {
    rt_full_admission_case(RtAdmission::Create, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_002_execution_full_known_busy_has_zero_close_and_zero_resume() {
    rt_full_admission_case(RtAdmission::Execution, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_002_replay_full_known_busy_has_zero_close_and_zero_resume() {
    rt_full_admission_case(RtAdmission::Replay, false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_003_create_reclaims_only_idle_root_then_actual_new() {
    rt_full_admission_case(RtAdmission::Create, true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_003_execution_reclaims_only_idle_root_then_actual_resume() {
    rt_full_admission_case(RtAdmission::Execution, true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_003_replay_reclaims_only_idle_root_then_actual_resume() {
    rt_full_admission_case(RtAdmission::Replay, true).await;
}
