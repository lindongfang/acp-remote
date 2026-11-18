// Candidate overlay only: include after R3WssCase in the private frozen snapshot.
// RT-001 requires the production scan integration, not a DTO that accepts either bool.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_001_node_inspect_reports_actual_enabled_scan_then_shutdown_false() {
    use crate::application::native_dispatch::NativeRuntimeStatus;

    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let retained = captured.clone();
    r3_wss_case("source-create-replay-state", false, move |f| {
        Box::pin(async move {
            let native = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .clone();
            assert_eq!(native.status(), NativeRuntimeStatus::Running);
            assert!(native.automatic_recycling_enabled());
            let response = f
                .call(0, 181, "session.capacity.inspect", serde_json::json!({}))
                .await;
            assert_eq!(response["ok"], true, "{response}");
            assert_eq!(response["result"]["limit"], 50, "{response}");
            assert_eq!(response["result"]["occupied"], 0, "{response}");
            assert_eq!(response["result"]["reserved"], 0, "{response}");
            assert_eq!(response["result"]["closedPending"], 0, "{response}");
            assert_eq!(
                response["result"]["automaticRecycling"], true,
                "Running production policy must actually start its owned scan: {response}"
            );
            assert!(
                !f.area.directory.join("source-child-pid").exists(),
                "capacity inspection must not bootstrap an ACP peer"
            );
            *retained.lock().unwrap() = Some(native);
        })
    })
    .await;
    // R3's owner has now completed actual Node shutdown and its join check.
    let native = captured.lock().unwrap().take().unwrap();
    assert_eq!(native.status(), NativeRuntimeStatus::Stopped);
    assert!(!native.automatic_recycling_enabled());
}
