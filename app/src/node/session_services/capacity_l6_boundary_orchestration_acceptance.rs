// The receipt is a standard ACP notification from an owned inert peer process.
// This test exercises the real-entry client's ordering and requalification,
// while preserving the separate requirement for real-Harness branch execution.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_orchestration_received_work_withdraws_and_requalifies_original() {
    Box::pin(capacity_l6_controlled_case(|f, mut config| {
        Box::pin(async move {
            use crate::application::native_dispatch::runtime_child_source_clock_probe::l6_automatic_waits;
            struct ReceiptTask(Option<tokio::task::JoinHandle<Result<(), &'static str>>>);
            impl Drop for ReceiptTask {
                fn drop(&mut self) {
                    if let Some(task) = &self.0 {
                        task.abort();
                    }
                }
            }
            config.allow_model_requests = true; // Only the inert peer can receive this text.
            std::fs::write(f.area.directory.join("runtime-next-created-idle"), b"owned idle").unwrap();
            // 后续 49 个根会话使用受控开发夹具，需显式开启创建根会话的前台保护标记以保证仅原根会话为候选。
            std::fs::write(
                f.area.directory.join("capacity-protect-created-roots"),
                b"controlled active roots",
            )
            .unwrap();
            let native_id = capacity_l6_new(f).await.unwrap();
            let root = capacity_l6_clock_root(f, native_id.clone()).await.unwrap();
            for _ in 0..49 {
                capacity_l6_new(f).await.unwrap();
            }
            capacity_l6_clock_eligible(f, &root).await.unwrap();
            let runtime = f.node.as_ref().unwrap().session_services.as_ref().unwrap().native.clone();
            let source = f.selection.as_ref().unwrap().source;
            let trigger = f.area.directory.join("l6-deferred-current-ready");
            let mut task = ReceiptTask(Some(tokio::spawn(async move {
                timeout(Duration::from_secs(15), async {
                    loop {
                        if l6_automatic_waits(&runtime).map_err(|_| "selection_probe_failed")?
                            .iter().any(|(s, n, _)| *s == source && n.as_ref() == native_id) {
                            std::fs::write(&trigger, b"actual original selected before SQL commit")
                                .map_err(|_| "owned_peer_trigger_failed")?;
                            return Ok::<(), &'static str>(());
                        }
                        tokio::task::yield_now().await;
                    }
                }).await.map_err(|_| "actual_selection_not_observed")?
            })));
            let scenario = CapacityL6Scenario {
                phases: [
                    ("received_before_send_arm".into(), vec![CapacityL6Command {
                        action: "session.capacity.inspect".into(),
                        payload: serde_json::json!({}),
                    }]),
                    ("received_before_send_finish".into(), vec![CapacityL6Command {
                        action: "l6.submit_text".into(),
                        payload: serde_json::json!({"text":"controlled end of work"}),
                    }]),
                ].into_iter().collect(),
                ..CapacityL6Scenario::default()
            };
            let mut steps = Vec::new();
            let result = capacity_l6_pre_send_recheck(f, &config, &scenario, "cp032", &mut steps).await;
            let receipt = task.0.take().unwrap().await.unwrap();
            assert!(
                result.is_ok(),
                "actual boundary client result={result:?}, receipt={receipt:?}"
            );
            assert!(
                receipt.is_ok(),
                "actual standard receipt trigger result={receipt:?}, client={result:?}"
            );
            let state: (String, String) = sqlx::query_as(
                "SELECT phase,outcome FROM node_session_automatic_releases WHERE anchor_reservation=?")
                .bind(&root.reservation).fetch_one(&capacity_l6_clock_pool(f).unwrap()).await.unwrap();
            assert_eq!(state, ("withdrawn".into(), "withdrawn".into()));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 50);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            let prompts = capacity_method_count(f, "session/prompt").max(
                std::fs::read_to_string(f.area.directory.join("source-prompt-count"))
                    .ok()
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0),
            );
            assert_eq!(prompts, 1);
            assert!(steps.iter().any(|step| step["branch"] == "received_before_send_recheck" && step["status"] == "passed"));
            assert!(capacity_l6_clock_sample(f, &root).unwrap().1.is_some());
        })
    })).await;
}
