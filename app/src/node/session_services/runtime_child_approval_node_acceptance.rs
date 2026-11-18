// Private overlay after current R3/Node capacity helpers.
// Real authenticated actor, original approval service, ACP response ticket and SQLite.
struct RtBarrierDrop<F: FnMut()>(F);
impl<F: FnMut()> Drop for RtBarrierDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_008_actual_child_selected_written_touches_all_ancestors_not_other_root() {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_activity_clocks;
    use crate::domain::approval_owner::{CommitState, RegisterDecision, SubmitDecision};
    use crate::session_storage::approval_owner::test_control::{ApprovalCut, ApprovalCutSelector};
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let root = capacity_create(f, 151).await;
            let other = capacity_create(f, 153).await;
            capacity_route_target(f, "runtime-child-leaf", r3_id(40)).await;
            std::fs::write(
                f.area.directory.join("runtime-emit-approval-tree"),
                root.as_bytes(),
            )
            .unwrap();
            let discovered = capacity_source_operation(
                f,
                155,
                serde_json::json!({"kind":"discover","cursor":null}),
            )
            .await;
            assert_eq!(discovered["ok"], true, "{discovered}");
            assert_eq!(discovered["result"]["outcome"], "succeeded", "{discovered}");
            let node = f.node.as_ref().unwrap();
            let services = node.session_services.as_ref().unwrap();
            let native = services.native.clone();
            let approvals = services.approval.clone();
            let policy = node.application_policy().unwrap();
            let actor = policy_actor(
                &policy,
                &f.peers[0].key,
                &f.peers[0].client_id,
                Id128::new([156; 16]),
            )
            .await;
            let display = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let mut all = approvals.list(&actor).await.unwrap();
                    if let Some(value) = all.pop()
                        && value.status().request_save.commit == CommitState::Confirmed
                    {
                        break value;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(display.choices().len(), 1);
            assert_eq!(display.choices()[0].native_option_id(), "");
            let read_clock = |name: &str| {
                rt_source_activity_clocks(&native)
                    .unwrap()
                    .into_iter()
                    .find(|(source, session, _)| *source == r3_id(1) && session.as_ref() == name)
                    .unwrap_or_else(|| panic!("actual current session clock absent: {name}"))
                    .2
            };
            let before = [
                read_clock(&root),
                read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),
                read_clock(&other),
            ];
            let controller = approvals.approval_test_controller();
            let cut = controller
                .pause_next(
                    ApprovalCut::BeforeSelectedEnqueue,
                    ApprovalCutSelector::Approval(display.id()),
                )
                .unwrap();
            let release = RtBarrierDrop(|| cut.release());
            let registration = approvals
                .register_decision(
                    &actor,
                    RegisterDecision::new(display.id(), display.choices()[0].id()),
                )
                .await
                .unwrap();
            let decision = approvals
                .decide(&actor, SubmitDecision::new(registration))
                .await;
            assert!(decision.is_ok(), "{decision:?}");
            let arrival = cut
                .wait_until_arrived(tokio::time::Instant::now() + Duration::from_secs(5))
                .await
                .unwrap();
            assert!(arrival.source_job_registered);
            assert_eq!(
                [
                    read_clock(&root),
                    read_clock("runtime-child-parent"),
                    read_clock("runtime-child-leaf"),
                    read_clock(&other),
                ],
                before,
                "registering/deciding without an enqueued response is not Written activity"
            );
            assert!(!f.area.directory.join("runtime-approval-selected").exists());
            let allowed_at = std::time::Instant::now();
            drop(release);
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let written: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM node_approval_reply_facts WHERE transport_state='written'",
                    )
                    .fetch_one(node.storage.as_ref().unwrap().pool_for_test())
                    .await
                    .unwrap();
                    if written == 1
                        && f.area.directory.join("runtime-approval-selected").is_file()
                        && read_clock("runtime-child-leaf") >= allowed_at
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let after = [
                read_clock(&root),
                read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),
                read_clock(&other),
            ];
            assert!(after[0] > before[0] && after[1] > before[1] && after[2] > before[2]);
            assert_eq!(after[0], after[2], "same actual Written touches root");
            assert_eq!(after[1], after[2], "same actual Written touches immediate ancestor");
            assert_eq!(after[3], before[3], "unrelated tree receives no approval activity");
            assert_eq!(capacity_method_count(f, "permission.response"), 1);
            assert_eq!(capacity_method_count(f, "session/prompt"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
        })
    })
    .await;
}
