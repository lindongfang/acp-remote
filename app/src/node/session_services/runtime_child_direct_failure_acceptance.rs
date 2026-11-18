async fn rt_child_failed_root_read(flag: &'static str) {
    r3_wss_case("source-create-replay-state", false, move |f| {
        Box::pin(async move {
            let root = rt_direct_tree(f).await;
            let flag_path = f.area.directory.join(flag);
            std::fs::write(&flag_path, b"inert controlled root response").unwrap();
            let opened = f.call(
                0, 175, "view.open_source",
                serde_json::json!({
                    "selection": f.selection_json(),
                    "nativeSessionId": "rt-direct-child-A",
                    "reason": "first_open"
                }),
            ).await;
            assert_eq!(opened["ok"], false, "failed root cannot mint a successful child view: {opened}");
            assert!(opened["result"].is_null(), "{opened}");
            let resumes: Vec<_> = capacity_events(f).into_iter()
                .filter(|event| event["method"] == "session/resume").collect();
            assert_eq!(resumes.len(), 1, "must actually reach the original root native request");
            assert_eq!(resumes[0]["native"], root);
            assert_eq!(capacity_method_count(f, "session/new"), 1);
            assert_eq!(capacity_method_count(f, "session/load"), 0);
            assert_eq!(capacity_method_count(f, "session/close"), 0);
            capacity_expect(f, 177, 1, 0).await;
            let pool = f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test();
            let rows: Vec<(String, i64, i64)> = sqlx::query_as(
                "SELECT native_id,physically_closed,released FROM node_session_capacity WHERE native_id=?",
            ).bind(&root).fetch_all(pool).await.unwrap();
            assert_eq!(rows, vec![(root, 0, 0)], "a failed/uncertain original root read keeps exactly its original responsibility");
            std::fs::remove_file(flag_path).unwrap();
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_012_actual_root_protocol_error_never_publishes_child_and_keeps_capacity() {
    rt_child_failed_root_read("capacity-replay-error").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_012_actual_root_completion_without_current_snapshot_keeps_original_capacity() {
    rt_child_failed_root_read("capacity-replay-unknown").await;
}
