// Independent short regressions of the real-entry client orchestration.
// ACP facts come from the controlled peer. No real Harness, model or identity is used.
#[test]
fn capacity_l6_command_pacing_keeps_rolling_minute_below_product_limit() {
    let start = tokio::time::Instant::now();
    let mut previous = None;
    let mut scheduled = Vec::new();
    for _ in 0..250 {
        let next = capacity_l6_next_command_at(previous, start);
        scheduled.push(next);
        previous = Some(next);
    }
    for (index, sent) in scheduled.iter().enumerate() {
        assert!(
            scheduled[index..]
                .iter()
                .filter(|next| next.saturating_duration_since(*sent) < Duration::from_secs(60))
                .count()
                <= 116
        );
    }
    let idle = scheduled[249] + Duration::from_secs(10);
    assert_eq!(capacity_l6_next_command_at(previous, idle), idle);
}

fn capacity_l6_orchestration_config(f: &R3WssCase) -> CapacityL6Config {
    CapacityL6Config {
        format: 1,
        run_root: f.area.directory.clone(),
        launch_profile: f.area.directory.join("not-used-by-finish-or-restart.json"),
        dedicated_home: f.area.directory.join("unused-owned-home"),
        dedicated_codex_home: f.area.directory.join("unused-owned-codex"),
        allow_model_requests: false,
        default_limit: 50,
        idle_seconds: 1800,
        scenario_files: std::collections::BTreeMap::new(),
        web_control: None,
    }
}

async fn capacity_l6_controlled_case(
    run: impl for<'a> FnOnce(
        &'a mut R3WssCase,
        CapacityL6Config,
    ) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
) {
    let mut f = R3WssCase {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        selection: None,
    };
    let result = AssertUnwindSafe(async {
        let mut c = capacity_l6_orchestration_config(&f);
        let peer = crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);
        let profile = serde_json::json!({
            "format":1,
            "launch":{
                "executable_utf16le_base64":r3_utf16(&peer.to_string_lossy()),
                // The verified scenario returns a standard Created reply with a
                // unique native identity. The older scenario keeps unknown
                // extensions, which deliberately cannot prove an independent root.
                "arguments_utf16le_base64":[r3_utf16("source-create-replay-state-verified")],
                "environment":[]
            },
            "recipes":[{"wire":2,"mcp_servers":[],"additional_directories":[],"extra":{}}]
        });
        c.launch_profile = f.area.directory.join("owned-controlled-entry-profile.json");
        std::fs::write(&c.launch_profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        capacity_l6_start(&mut f, &c).await.unwrap();
        run(&mut f, c).await;
    })
    .catch_unwind()
    .await;
    if let Some(pool) = f
        .node
        .as_ref()
        .and_then(|node| node.storage.as_deref())
        .map(crate::storage::Storage::pool_for_test)
    {
        for name in [
            "l6_ticket_source_fault",
            "l6_ticket_finish_fault",
            "l6_owned_finish_fault",
            "l6_clock_finish_fault",
        ] {
            sqlx::query(&format!("DROP TRIGGER IF EXISTS {name}"))
                .execute(pool)
                .await
                .expect("remove only the owned regression's fault before shutdown");
        }
    }
    let address = f.node.as_ref().map(NodeRuntime::listen_addr);
    let retained = f
        .node
        .as_ref()
        .and_then(|node| node.session_services.as_ref())
        .map(|services| services.native.clone());
    let closed = f.close().await;
    let source_cleanup = retained.is_some_and(|native| {
        native
            .last_shutdown_report()
            .is_some_and(|report| report.joined && report.metadata_confirmed)
            && {
                let sources = native.source_close_diagnostics();
                !sources.is_empty() && sources.iter().all(|source| {
                    source.physical_present
                        && source.metadata_close.as_ref().is_some_and(Result::is_ok)
                        && matches!(
                            source.physical_owner,
                            Some(
                                crate::application::native_dispatch::OwnerCloseKind::WindowsJob {
                                    active_processes: 0
                                }
                            )
                        )
                })
            }
    });
    let port_closed = if let Some(address) = address {
        matches!(
            timeout(Duration::from_secs(10), tokio::net::TcpStream::connect(address)).await,
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused
        )
    } else {
        false
    };
    eprintln!(
        "L6_CONTROLLED_CLEANUP joined={closed} source_jobs_and_metadata={source_cleanup} port_closed={port_closed}"
    );
    if result.is_err() || !closed {
        eprintln!("L6_CONTROLLED_FIXTURE_PRESERVED {:?}", f.area.directory);
        std::mem::forget(f.area);
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(closed, "actual L6 Node owner must join");
    assert!(
        source_cleanup,
        "actual source Jobs and metadata must converge"
    );
    assert!(
        port_closed,
        "original listener must refuse a new connection"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_orchestration_last_slot_client_uses_three_actual_ingresses() {
    Box::pin(capacity_l6_controlled_case(|f, mut config| {
        Box::pin(async move {
            // This flag only allows the generic scenario command parser. The
            // installed executable is the inert ACP peer and cannot bill a model.
            config.allow_model_requests = true;
            let scenario = CapacityL6Scenario {
                protected_root_steps: vec![CapacityL6Command {
                    action: "session.capacity.inspect".into(),
                    payload: serde_json::json!({}),
                }],
                phases: std::iter::once((
                    "last_slot_execution".into(),
                    vec![CapacityL6Command {
                        action: "l6.submit_text".into(),
                        payload: serde_json::json!({"text":"controlled entry upload only"}),
                    }],
                ))
                .collect(),
                ..CapacityL6Scenario::default()
            };
            // The controlled peer emits current foreground activity for each
            // created root only when this owned marker exists. The real Harness
            // run never writes it and must supply activity from actual work.
            std::fs::write(
                f.area.directory.join("capacity-protect-created-roots"),
                b"controlled active roots",
            )
            .unwrap();
            let mut steps = Vec::new();
            let result = capacity_l6_last_slot(f, &config, &scenario, &mut steps).await;
            assert!(result.is_ok(), "actual admission client result={result:?}");
            assert!(steps.iter().any(|step| {
                step["branch"] == "real_fifty_and_three_ingress_last_slot"
                    && step["status"] == "passed"
            }));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 0);
            let prompts = std::fs::read_to_string(f.area.directory.join("source-prompt-count"))
                .ok()
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            assert_eq!(
                prompts, 1,
                "one refused load attempt followed by one recovery load must send exactly one execution prompt"
            );
            assert!(capacity_method_count(f, "session/resume") > 0);
            assert!(
                steps
                    .iter()
                    .all(|step| step["branch"] != "same_node_real_harness_web")
            );
        })
    }))
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_orchestration_finish_then_restart_preserves_original_operation() {
    Box::pin(capacity_l6_controlled_case(|f, config| {
        Box::pin(async move {
            std::fs::write(
                f.area.directory.join("runtime-next-created-idle"),
                b"controlled standard peer current Idle",
            )
            .unwrap();
            let original_native = capacity_l6_new(f).await.unwrap();
            let original = capacity_l6_clock_root(f, original_native).await.unwrap();
            for _ in 0..49 {
                capacity_l6_new(f).await.unwrap();
            }
            capacity_l6_clock_eligible(f, &original).await.unwrap();
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 50);
            let mut steps = Vec::new();
            let repaired = capacity_l6_cp032_finish(f, &config, &mut steps).await;
            assert!(repaired.is_ok(), "actual finish client result={repaired:?}");
            let operation = repaired.unwrap();
            assert!(steps.iter().any(|step| {
                step["step"] == "same_original_repair"
                    && step["operationId"] == operation
                    && step["releasedExactlyOnce"] == true
            }));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 50);
            let result = capacity_l6_cp032_restart(f, &config, &operation, &mut steps).await;
            assert!(result.is_ok(), "actual restart client result={result:?}");
            assert!(steps.iter().any(|step| {
                step["step"] == "same_owned_node_restart_original_operation_isolated"
                    && step["operationId"] == operation
                    && step["originalOperationAndMembersUnchanged"] == true
                    && step["executionUrlUnchanged"] == true
                    && step["oldOwnerJoined"] == true
                    && step["oldNecessaryMetadataConfirmed"] == true
                    && step["oldJobActiveProcesses"] == 0
            }));
            assert_eq!(capacity_l6_inspect(f).await.unwrap()["occupied"], 1);
            assert!(
                steps
                    .iter()
                    .all(|step| step["branch"] != "same_node_real_harness_web")
            );
        })
    }))
    .await;
}
