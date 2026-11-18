// Exercise the unchanged L6 ticket-fault client with real Node/IO and a
// dedicated inert peer. The only scenario command is an inspect query.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_orchestration_zero_partial_and_flush_keep_original_ticket_responsibility() {
    Box::pin(capacity_l6_controlled_case(|f, mut config| {
        Box::pin(async move {
            config.allow_model_requests = true; // Required by the generic command parser; no input command is sent.
            // 健康源使用受控开发夹具，需显式开启创建根会话的前台保护标记。
            std::fs::write(
                f.area.directory.join("capacity-protect-created-roots"),
                b"controlled active roots",
            )
            .unwrap();
            let peer = std::env::var("L6_CONTROLLED_IDLE_PEER")
                .expect("explicit owned inert peer executable required");
            assert!(std::path::Path::new(&peer).is_absolute());
            assert!(std::path::Path::new(&peer).is_file());
            let profile = serde_json::json!({
                "format":1,
                "launch":{
                    "executable_utf16le_base64":r3_utf16(&peer),
                    "arguments_utf16le_base64":[r3_utf16("source-create-replay-state")],
                    "environment":[]
                },
                "recipes":[{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{}}]
            });
            config.launch_profile = f.area.directory.join("owned-inert-fault-profile.json");
            std::fs::write(
                &config.launch_profile,
                serde_json::to_vec(&profile).unwrap(),
            )
            .unwrap();
            let scenario = CapacityL6Scenario {
                protected_root_steps: vec![CapacityL6Command {
                    action: "session.capacity.inspect".into(),
                    payload: serde_json::json!({}),
                }],
                ..CapacityL6Scenario::default()
            };
            let mut steps = Vec::new();
            let result =
                capacity_l6_original_ticket_faults(f, &config, &scenario, "cp032", &mut steps)
                    .await;
            assert!(result.is_ok(), "actual ticket client result={result:?}");
            let faults = steps
                .iter()
                .filter(|step| step["step"] == "real_harness_original_close_node_ticket_fault")
                .collect::<Vec<_>>();
            assert_eq!(faults.len(), 3);
            for label in ["zero", "partial", "after_frame_unknown"] {
                assert!(faults.iter().any(|step| {
                    step["fault"] == label
                        && step["released"] == false
                        && step["occupied"] == 50
                        && step["healthyOriginalTrees"] == 49
                }));
            }
            assert!(steps.iter().any(|step| {
                step["branch"] == "node_zero_partial_unknown_ticket_boundaries"
                    && step["status"] == "passed"
            }));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 0);
            assert_eq!(capacity_method_count(f, "session/prompt"), 0);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
            assert_eq!(capacity_method_count(f, "session/resume"), 0);
        })
    }))
    .await;
}
