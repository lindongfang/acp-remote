#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_009_terminal_statement_failure_keeps_tree_protected_until_original_repair() {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::rt_source_activity_clocks;
    use crate::domain::approval_owner::{CommitState, RegisterDecision, SubmitDecision};
    use crate::session_storage::approval_owner::test_control::{
        ApprovalCut, ApprovalCutSelector, ApprovalStatement,
    };
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let root = capacity_create(f, 151).await;
            let other = capacity_create(f, 153).await;
            for i in 0..48_u8 {
                capacity_create(f, 1 + 2 * i).await;
            }
            capacity_expect(f, 154, 50, 0).await;
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
            let pool = node.storage.as_ref().unwrap().pool_for_test().clone();
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
            let fault = controller
                .abort_next_statement(
                    ApprovalStatement::InsertTerminal,
                    ApprovalCutSelector::Approval(display.id()),
                )
                .unwrap();
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
            let fired = fault
                .wait_until_fired(tokio::time::Instant::now() + Duration::from_secs(10))
                .await
                .unwrap();
            assert!(
                fired.actual_sql_error,
                "real original InsertTerminal SQL statement abort"
            );
            assert_eq!(fired.tag.approval, Some(display.id()));
            let response_id = fired.tag.response.unwrap();
            timeout(Duration::from_secs(10), async {
                while !f.area.directory.join("runtime-approval-selected").is_file() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let terminal: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM node_approval_reply_facts WHERE response_id=?",
            )
            .bind(response_id.encode())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                terminal, 0,
                "Written does not fabricate necessary terminal success"
            );
            let after = [
                read_clock(&root),
                read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),
                read_clock(&other),
            ];
            assert_eq!(
                after, before,
                "missing necessary terminal does not publish approval completion"
            );
            let attempts: Vec<(String, String)> =
                sqlx::query_as("SELECT response_id, approval_id FROM node_approval_reply_attempts")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                attempts,
                vec![(response_id.encode(), display.id().encode())]
            );
            let refused =
                capacity_source_operation(f, 170, serde_json::json!({"kind":"create"})).await;
            assert!(
                refused.to_string().contains("session_capacity_full"),
                "{refused}"
            );
            capacity_expect(f, 172, 50, 0).await;
            assert_eq!(
                capacity_method_count(f, "session/close"),
                0,
                "actual Written with missing necessary terminal still protects original root"
            );
            let owner = approvals.original_reconciler();
            let mut repair = owner
                .start_repair(
                    crate::application::native_dispatch::ApprovalRepairScope::Request(display.id()),
                    crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts,
                    tokio::time::Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
            let report = repair
                .wait(tokio::time::Instant::now() + Duration::from_secs(6))
                .await
                .unwrap();
            assert_eq!(
                report.outcome,
                crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired
            );
            let facts: Vec<(String, String)> =
                sqlx::query_as("SELECT response_id,transport_state FROM node_approval_reply_facts")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                facts,
                vec![(response_id.encode(), "written".into())],
                "repair records only the actual original ticket"
            );
            let attempts_after: Vec<(String, String)> =
                sqlx::query_as("SELECT response_id, approval_id FROM node_approval_reply_attempts")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                attempts_after, attempts,
                "repair must not create another attempt"
            );
            let repaired = [
                read_clock(&root),
                read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),
                read_clock(&other),
            ];
            assert!(repaired[0] >= allowed_at && repaired[0] > before[0]);
            assert_eq!(repaired[0], repaired[1]);
            assert_eq!(repaired[0], repaired[2]);
            assert_eq!(repaired[3], before[3], "unrelated tree remains unchanged");
            assert_eq!(capacity_method_count(f, "permission.response"), 1);
            assert_eq!(capacity_method_count(f, "session/prompt"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
        })
    })
    .await;
}

#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_009_actual_expired_selected_zero_protects_tree_then_retries_only_original() {
    use crate::application::native_dispatch::runtime_child_source_clock_probe::{
        rt_selected_deadline, rt_source_activity_clocks,
    };
    use crate::domain::approval_owner::{CommitState, RegisterDecision, SubmitDecision};
    use crate::session_storage::approval_owner::test_control::{ApprovalCut, ApprovalCutSelector};
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"idle").unwrap();
            let root = capacity_create(f, 151).await;
            let other = capacity_create(f, 153).await;
            for i in 0..48_u8 { capacity_create(f, 1 + 2 * i).await; }
            capacity_expect(f, 154, 50, 0).await;
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
            let pool = node.storage.as_ref().unwrap().pool_for_test().clone();
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
            let (original_reply, write_until) = rt_selected_deadline(&native, "runtime-child-leaf").unwrap();
            assert!(write_until > tokio::time::Instant::now(), "actual production write budget is still live at the cut");
            // Observe and wait for the real production deadline. No test changes
            // a ticket, activity, source option, or transport result.
            tokio::time::sleep_until(write_until + Duration::from_millis(1)).await;
            drop(release);
            timeout(Duration::from_secs(5), async {
                loop {
                    let zero: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM node_approval_reply_facts WHERE response_id=? AND transport_state='zero'",
                    ).bind(original_reply.encode()).fetch_one(&pool).await.unwrap();
                    if zero == 1 { break; }
                    tokio::task::yield_now().await;
                }
            }).await.expect("real original ticket must be reliably Zero, not hook metadata timeout");
            assert!(!f.area.directory.join("runtime-approval-selected").exists());
            assert_eq!(capacity_method_count(f, "permission.response"), 0);
            assert_eq!([
                read_clock(&root),read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),read_clock(&other),
            ], before, "actual Zero must not publish Written activity");
            let refused = capacity_source_operation(f, 174, serde_json::json!({"kind":"create"})).await;
            assert!(refused.to_string().contains("session_capacity_full"), "{refused}");
            capacity_expect(f, 176, 50, 0).await;
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let owner = approvals.original_reconciler();
            let permit = owner.prepare_zero_retry(display.id()).unwrap();
            assert!(owner.prepare_zero_retry(display.id()).is_err(), "single original retry nonce");
            let allowed_at = std::time::Instant::now();
            let mut retry = owner.start_zero_retry(
                permit, tokio::time::Instant::now() + Duration::from_secs(5),
            ).map_err(|rejected| rejected.error).unwrap();
            let report = retry.wait(tokio::time::Instant::now() + Duration::from_secs(6)).await.unwrap();
            let crate::application::native_dispatch::ApprovalReconcileOutcome::ReplyStarted { response } = report.outcome else {
                panic!("original Zero must adopt exactly its fresh-budget attempt: {:?}", report.outcome);
            };
            assert_ne!(response, original_reply);
            timeout(Duration::from_secs(6), async {
                loop {
                    let written: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM node_approval_reply_facts WHERE response_id=? AND transport_state='written'",
                    ).bind(response.encode()).fetch_one(&pool).await.unwrap();
                    if written == 1 && f.area.directory.join("runtime-approval-selected").is_file()
                        && read_clock("runtime-child-leaf") >= allowed_at { break; }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT response_id FROM node_approval_reply_attempts WHERE approval_id=? ORDER BY attempt_serial",
            ).bind(display.id().encode()).fetch_all(&pool).await.unwrap();
            assert_eq!(ids, vec![original_reply.encode(), response.encode()]);
            let clocks = [
                read_clock(&root),read_clock("runtime-child-parent"),
                read_clock("runtime-child-leaf"),read_clock(&other),
            ];
            assert!(clocks[0] >= allowed_at && clocks[0] > before[0]);
            assert_eq!(clocks[0], clocks[1]); assert_eq!(clocks[0], clocks[2]);
            assert_eq!(clocks[3], before[3]);
            assert!(owner.prepare_zero_retry(display.id()).is_err(), "Written cannot retry");
            assert_eq!(capacity_method_count(f, "permission.response"), 1);
            assert_eq!(capacity_method_count(f, "session/prompt"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
        })
    })
    .await;
}
