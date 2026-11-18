#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_012_actual_child_root_eof_retains_until_original_source_metadata_converges() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            let _root = rt_direct_tree(f).await;
            let pool=f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test().clone();
            let disconnect=f.area.directory.join("capacity-replay-disconnect");
            let _cleanup=CapacityRemoveFixtureFile(disconnect.clone());
            std::fs::write(disconnect,b"actual root EOF after Written").unwrap();
            std::fs::write(f.area.directory.join("capacity-hold-replay"),b"hold actual request").unwrap();
            let gate=CapacityGateRelease(f.area.directory.clone());
            let selection=f.selection_json();
            send_json(&mut f.peers[0].socket,&serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([181;16]),
                "action":"view.open_source","payload":{
                    "selection":selection,"nativeSessionId":"rt-direct-child-A","reason":"first_open"
                }
            })).await;
            capacity_wait_method(f,"session/resume",1).await;
            sqlx::query("CREATE TRIGGER rt_child_eof_source_fault BEFORE UPDATE OF lifecycle ON node_native_source_instances WHEN NEW.lifecycle='closed' BEGIN SELECT RAISE(ABORT,'owned child EOF necessary fact'); END")
                .execute(&pool).await.unwrap();
            let observed=AssertUnwindSafe(async {
                drop(gate);
                let opened=next_json(&mut f.peers[0].socket,Duration::from_secs(15)).await;
                assert_eq!(opened["ok"],false,"EOF cannot publish a successful child stream");
                assert!(opened["result"].is_null());
                timeout(Duration::from_secs(12),async {
                    loop {
                        let diagnostics=f.node.as_ref().unwrap().session_services.as_ref().unwrap()
                            .native.source_close_diagnostics();
                        if diagnostics.iter().any(|s|s.physical_present&&s.metadata_close.as_ref().is_some_and(Result::is_err)) {break;}
                        tokio::task::yield_now().await;
                    }
                }).await.expect("original source physically converged but necessary close failed");
                let held=capacity_inspect(f,182).await;
                assert_eq!(held["occupied"],1,"EOF/Job0 alone cannot refund original root");
                let released:i64=sqlx::query_scalar("SELECT sum(released) FROM node_session_capacity")
                    .fetch_one(&pool).await.unwrap();
                assert_eq!(released,0);
                assert_eq!(capacity_method_count(f,"session/resume"),1);
                assert_eq!(capacity_method_count(f,"session/close"),0);
            }).catch_unwind().await;
            sqlx::query("DROP TRIGGER rt_child_eof_source_fault").execute(&pool).await.unwrap();
            // Status of the real original Created operation asks the original
            // retained source owner to retry only its own necessary close.
            let operation:String=sqlx::query_scalar("SELECT operation_id FROM node_source_session_operations WHERE action='create' LIMIT 1")
                .fetch_one(&pool).await.unwrap();
            let status=f.call(0,183,"source.session.status",serde_json::json!({"sourceOperationId":operation})).await;
            assert_eq!(status["ok"],true);
            if let Err(error)=observed {std::panic::resume_unwind(error);}
            timeout(Duration::from_secs(12),async {
                loop {
                    if capacity_inspect(f,184).await["occupied"]==0 {break;}
                    tokio::task::yield_now().await;
                }
            }).await.expect("original full-source necessary close returns original slot once");
            assert_eq!(capacity_method_count(f,"session/new"),1);
            assert_eq!(capacity_method_count(f,"session/resume"),1);
            assert_eq!(capacity_method_count(f,"session/close"),0);
        })
    }).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rt_012_child_requester_disconnect_after_root_written_never_refunds_root() {
    r3_wss_case("source-create-replay-state", false, |f| {
        Box::pin(async move {
            let _root=rt_direct_tree(f).await;
            std::fs::write(f.area.directory.join("capacity-hold-replay"),b"hold actual root read").unwrap();
            let gate=CapacityGateRelease(f.area.directory.clone());
            let selection=f.selection_json();
            send_json(&mut f.peers[0].socket,&serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([185;16]),
                "action":"view.open_source","payload":{
                    "selection":selection,"nativeSessionId":"rt-direct-child-A","reason":"first_open"
                }
            })).await;
            capacity_wait_method(f,"session/resume",1).await;
            // The requesting authenticated connection is the actual cancelled
            // consumer; neither native response nor capacity is fabricated.
            f.peers[0].socket.close(None).await.unwrap();
            let held=f.call(1,186,"session.capacity.inspect",serde_json::json!({})).await;
            assert_eq!(held["result"]["occupied"],1);
            assert_eq!(capacity_method_count(f,"session/close"),0);
            drop(gate);
            // A following real ACP list on the same source is processed after
            // the held original resume response. It cannot stand in for a read.
            let registered=f.call(1,187,"source.session.register",serde_json::json!({
                "selection":f.selection_json(),"operation":{"kind":"discover","cursor":null}
            })).await;
            assert_eq!(registered["ok"],true);
            let done=f.call(1,188,"source.session.execute",serde_json::json!({
                "sourceOperationId":registered["result"]["sourceOperationId"]
            })).await;
            assert_eq!(done["result"]["outcome"],"succeeded");
            let after=f.call(1,189,"session.capacity.inspect",serde_json::json!({})).await;
            assert_eq!(after["result"]["occupied"],1);
            let pool=f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test();
            let original:Vec<(i64,i64)>=sqlx::query_as("SELECT physically_closed,released FROM node_session_capacity")
                .fetch_all(pool).await.unwrap();
            assert_eq!(original,vec![(0,0)],"consumer cancellation retains original live root responsibility");
            assert_eq!(capacity_method_count(f,"session/new"),1);
            assert_eq!(capacity_method_count(f,"session/resume"),1);
            assert_eq!(capacity_method_count(f,"session/load"),0);
            assert_eq!(capacity_method_count(f,"session/close"),0);
        })
    }).await;
}
