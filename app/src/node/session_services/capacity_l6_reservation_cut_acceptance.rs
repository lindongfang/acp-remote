// Controlled-peer regression of the Node-only scheduling cut used by the real
// Harness entry. Its success is not a real-Harness CP011 result.
struct CapacityL6ReserveGate(std::sync::Arc<crate::test_support::AsyncBarrier>);
impl Drop for CapacityL6ReserveGate {
    fn drop(&mut self) {
        self.0.release();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capacity_l6_reserved_last_slot_blocks_execution_and_replay_before_real_new() {
    use crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint;
    r3_wss_case("source-create-replay-state-verified",false,|f| {
        Box::pin(async move {
            std::fs::write(f.area.directory.join("capacity-protect-created-roots"),b"owned fixture").unwrap();
            let execution = capacity_create(f,151).await;
            capacity_route_target(f,&execution,r3_id(45)).await;
            let release = capacity_register_release(f,153,r3_id(45)).await;
            assert_eq!(capacity_release(f,154,&release).await["result"]["state"],"released");
            let replay = capacity_create(f,155).await;
            capacity_route_target(f,&replay,r3_id(46)).await;
            let release = capacity_register_release(f,157,r3_id(46)).await;
            assert_eq!(capacity_release(f,158,&release).await["result"]["state"],"released");
            for index in 0..49_u8 { capacity_create(f,1+index*2).await; }
            capacity_expect(f,160,49,0).await;
            let registered=f.call(0,161,"source.session.register",serde_json::json!({
                "selection":f.selection_json(),"operation":{"kind":"create"}
            })).await;
            assert_eq!(registered["ok"],true);
            let operation=registered["result"]["sourceOperationId"].clone();
            let node=f.node.as_ref().unwrap();
            let pool=node.storage.as_ref().unwrap().pool_for_test().clone();
            let native=node.session_services.as_ref().unwrap().native.clone();
            let barrier=node.storage.as_ref().unwrap().request_registry()
                .native_metadata_hooks_for_test().pause_next(NativeMetadataPoint::CapacityReservedAfterCommit).unwrap();
            let gate=CapacityL6ReserveGate(barrier.clone());
            let before_new=capacity_method_count(f,"session/new");
            let before_resume=capacity_method_count(f,"session/resume");
            let before_load=capacity_method_count(f,"session/load");
            send_json(&mut f.peers[0].socket,&serde_json::json!({
                "v":1,"type":"command","requestId":Id128::new([162;16]),
                "action":"source.session.execute","payload":{"sourceOperationId":operation}
            })).await;
            timeout(Duration::from_secs(5),barrier.wait_until_entered()).await.unwrap();
            let reserved:Vec<String>=sqlx::query_scalar(
                "SELECT reservation_id FROM node_session_capacity WHERE native_id IS NULL AND physically_closed=0 AND released=0",
            ).fetch_all(&pool).await.unwrap();
            assert_eq!(reserved.len(),1,"one real committed reservation, not a fabricated count");
            let snapshot=native.inspect_capacity().await.unwrap();
            assert_eq!(snapshot.occupied,50);
            assert_eq!(snapshot.reserved,1);
            assert_eq!(capacity_method_count(f,"session/new"),before_new);

            // Independent authenticated connection: the first command is still
            // awaiting its real source operation response on peer zero.
            let reopened=f.call(1,163,"view.open_source",serde_json::json!({
                "selection":f.selection_json(),"nativeSessionId":replay,"reason":"explicit_refresh"
            })).await;
            assert_eq!(reopened["ok"],false,"{reopened}");
            assert_eq!(reopened["error"]["code"],"session_capacity_full");
            capacity_submit_execution_target(f,r3_id(45)).await;
            assert!(matches!(native.ensure_target(r3_id(45)).await,
                Err(crate::domain::native_dispatch::DispatchError::SessionCapacityFull)));
            assert_eq!(capacity_method_count(f,"session/new"),before_new);
            assert_eq!(capacity_method_count(f,"session/resume"),before_resume);
            assert_eq!(capacity_method_count(f,"session/load"),before_load);
            assert_eq!(native.inspect_capacity().await.unwrap().occupied,50);
            drop(gate);
            let actual=next_json(&mut f.peers[0].socket,Duration::from_secs(20)).await;
            assert_eq!(actual["requestId"],Id128::new([162;16]).to_base64url());
            assert_eq!(actual["result"]["outcome"],"succeeded");
            let created=actual["result"]["response"]["nativeSessionId"].as_str().unwrap();
            let bound:Vec<(String,i64,i64)>=sqlx::query_as(
                "SELECT native_id,root_verified,released FROM node_session_capacity WHERE reservation_id=?",
            ).bind(&reserved[0]).fetch_all(&pool).await.unwrap();
            assert_eq!(bound,vec![(created.to_owned(),1,0)]);
            assert_eq!(capacity_method_count(f,"session/new"),before_new+1);
            assert_eq!(capacity_method_count(f,"session/resume"),before_resume);
            assert_eq!(capacity_method_count(f,"session/load"),before_load);
            capacity_expect(f,164,50,0).await;
        })
    }).await;
}
