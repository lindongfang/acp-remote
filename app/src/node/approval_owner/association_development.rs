// Real original Node/SQLite/ACP Source. No synthetic approval or association constructors.
use crate::domain::approval_owner::{ApprovalRejection, AssociationDisplay, NecessaryWriteError};
use crate::session_storage::approval_owner::test_control::{ApprovalCut, ApprovalCutSelector};
#[tokio::test]
async fn approval_association_same_original_no_run_trusted_mapping_and_read_is_readonly() {
    association_case(0).await;
}
#[tokio::test]
async fn approval_association_caller_drop_keeps_original_owned_association_save() {
    association_case(1).await;
}
#[tokio::test]
async fn approval_association_second_connection_remap_before_begin_rejects_then_reparses() {
    association_case(2).await;
}
#[tokio::test]
async fn approval_association_source_close_cannot_revive_original_unassociated() {
    association_case(3).await;
}
#[tokio::test]
async fn approval_association_same_slot_concurrent_registers_share_one_association() {
    association_case(4).await;
}
#[tokio::test]
async fn approval_association_committed_close_ack_loss_fixes_original_candidate_and_gap() {
    association_case(5).await;
}
#[tokio::test]
async fn approval_association_commit_unknown_does_not_reparse_until_original_exact_repair() {
    association_case(6).await;
}
#[tokio::test]
async fn approval_association_source_close_joins_original_save_paused_before_begin() {
    association_case(7).await;
}
#[tokio::test]
async fn approval_association_quarantined_source_only_actual_zero_retries_original_saved_once() {
    association_case(8).await;
}
#[tokio::test]
async fn approval_resolution_pre_candidate_actual_ack_loss_can_recover_via_register() {
    association_case(9).await;
}
#[tokio::test]
async fn approval_resolution_pre_candidate_actual_ack_loss_explicit_repair_keeps_original() {
    association_case(10).await;
}
async fn association_case(mode: u8) {
    let area = TestArea::new();
    let runtime = start_approval_runtime(&area).await;
    let key = SigningKey::from_slice(&[81; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "association development").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([82; 16])).await;
    let storage = runtime.storage.as_ref().unwrap();
    let pool = storage.pool_for_test();
    seed_association_storage(pool, &area, mode).await;
    let (queue, native) = start_association_native(&runtime, storage, mode).await;
    wait_association_native(&native).await;
    let approvals = runtime.approval_owner(&native).unwrap();
    let controller = approvals.approval_test_controller();
    let proof = queue
        .prepare_native_read(&actor, id(4), ReadReason::FirstOpen)
        .await
        .unwrap();
    let acquisition = native
        .acquire_read_source(proof, ReadReason::FirstOpen)
        .unwrap();
    let (_loaded,result)=timeout(Duration::from_secs(20),async{
      let loaded=async move{acquisition.source_capability().await};
      tokio::join!(loaded,async{
 let result:Result<(),String>=async{
  let initial=loop{if let Some(d)=approvals.list(&actor).await.map_err(|e|format!("list {e:?}"))?.pop() && d.status().request_save.commit==CommitState::Confirmed{break d;}tokio::task::yield_now().await;};
  let approval=initial.id();let choice=initial.choices()[0].id();let require=|ok:bool,msg:&str|if ok{Ok(())}else{Err(msg.to_owned())};
  require(matches!(initial.association(),AssociationDisplay::Unassociated),"initial must be unassociated")?;
  let before:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_associations").fetch_one(pool).await.unwrap();require(before==0,"initial SQL association")?;
  let unmapped=approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await;
  require(matches!(unmapped,Err(NecessaryWriteError::Rejected(ApprovalRejection::UnassociatedTarget))),&format!("unmapped register rejected: {unmapped:?}; initial {:?}",initial.status().validity))?;
  sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,?)").bind(id(8).encode()).bind(id(7).encode()).bind("synthetic-native-B").bind(id(2).encode()).execute(pool).await.unwrap();
  require(matches!(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await,Err(NecessaryWriteError::Rejected(ApprovalRejection::UnassociatedTarget))),"wrong Source mapping cannot associate")?;
  if mode==3{let close=native.shutdown(Instant::now()+Duration::from_secs(8)).await;require(close.joined&&close.metadata_confirmed,"first close unassociated")?;}
  sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,?)").bind(id(5).encode()).bind(id(1).encode()).bind("synthetic-native-B").bind(id(2).encode()).execute(pool).await.unwrap();
  for _ in 0..3{let d=approvals.read(&actor,approval).await.map_err(|e|format!("read {e:?}"))?;require(matches!(d.association(),AssociationDisplay::Unassociated),"read starts no association write")?;}
  let rows:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_associations").fetch_one(pool).await.unwrap();require(rows==0,"GET-like read wrote association")?;
  if mode==3{require(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await.is_err(),"closed original must not revive")?;return Ok(());}
  if mode==1 || mode==2 || mode==7 {
   let cut=controller.pause_next(ApprovalCut::RequestBeforeBegin,ApprovalCutSelector::Approval(approval)).unwrap();
   let first={let fut=approvals.register_decision(&actor,RegisterDecision::new(approval,choice));tokio::pin!(fut);let arrived=tokio::select!{r=&mut fut=>return Err(format!("register returned before association SQL cut: {r:?}")),a=cut.wait_until_arrived(Instant::now()+Duration::from_secs(5))=>a.map_err(|e|format!("cut {e:?}"))?};require(arrived.source_job_registered&&!arrived.lease_attached&&!arrived.begin_started,"association cut owned before BEGIN")?;
    if mode==7{let original=crate::application::native_dispatch::observe_approval_original(&native,approval).unwrap();let(close,())=tokio::join!(native.shutdown(Instant::now()+Duration::from_secs(8)),async{timeout(Duration::from_secs(5),async{while !original.snapshot().unwrap().source_jobs_sealed{tokio::task::yield_now().await;}}).await.unwrap();cut.release();});let end=original.snapshot().unwrap();require(close.joined&&close.metadata_confirmed,&format!("SourceClose joins paused original save: {close:?}; jobs={}/{}/{} sealed={} cleanup={} metadata={} physical={} manifest={} required={}",end.source_jobs_registered,end.source_jobs_joined,end.source_jobs_failed,end.source_jobs_sealed,end.cleanup_joined,end.metadata_closer_joined,end.physical_proof_cached,end.approval_manifest_cached,end.required_metadata_count))?;require(fut.await.is_err(),"closing does not admit decision after association")?;return Ok(());}
    if mode==2{sqlx::query("UPDATE main.product_targets SET native_id='moved-after-resolve' WHERE target_id=?").bind(id(5).encode()).execute(pool).await.unwrap();cut.release();Some(fut.await)}else{None}
   };
   cut.release();
   if mode==2 {require(matches!(first,Some(Err(NecessaryWriteError::Rejected(ApprovalRejection::UnassociatedTarget)))),"atomic recheck must reject stale resolver target")?;let n:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_associations").fetch_one(pool).await.unwrap();require(n==0,"stale candidate persisted")?;sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,?)").bind(id(6).encode()).bind(id(1).encode()).bind("synthetic-native-B").bind(id(2).encode()).execute(pool).await.unwrap();}
  }
  if mode==9 || mode==10 {
   let original=crate::application::native_dispatch::observe_approval_original(&native,approval).unwrap();
   let cut=controller.pause_next(ApprovalCut::LeaseAfterCloseAck,ApprovalCutSelector::ApprovalOperation(approval, crate::session_storage::approval_owner::test_control::ApprovalSqlOperation::Resolve)).unwrap();
    let first={let(first,_)=tokio::join!(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)),async{let arrival=cut.wait_until_arrived(Instant::now()+Duration::from_secs(5)).await.unwrap();require(arrival.actual_close_ack_observed&&arrival.source_job_registered,"resolver close cut must follow actual ack in its Source job")?;require(matches!(arrival.commit,CommitState::NotStarted),"read-only resolver must not report a commit")?;controller.fail_result_after_ack(&cut).unwrap();cut.release();Ok::<(),String>(())});first};
    if !matches!(first,Err(NecessaryWriteError::SqlCloseUnconfirmed{commit_confirmed:false})){let inspection=approvals.inspect_decision(&actor,approval).await.ok();let read=approvals.read(&actor,approval).await.ok().map(|display|format!("{:?}",display.association()));let diagnostic=original.snapshot().ok();return Err(format!("real resolver first report loss: {first:?}; inspection={inspection:?}; read_association={read:?}; original={diagnostic:?}"));}
   let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_associations").fetch_one(pool).await.unwrap();require(count==0,"resolver unknown did not save candidate")?;
   require(!area.directory.join("approval-selected").exists(),"resolver unknown zero Selected")?;
   let before=original.snapshot().unwrap().source_jobs_registered;
   for _ in 0..2{let d=approvals.read(&actor,approval).await.unwrap();require(matches!(d.association(),AssociationDisplay::Unassociated),"read must not publish association")?;let _=approvals.list(&actor).await.unwrap();let _=approvals.inspect_decision(&actor,approval).await.unwrap();let mut watch=approvals.watch_status(&actor,approval).await.unwrap();let _=watch.next(&actor,0,Instant::now()+Duration::from_secs(1)).await.unwrap();}
   require(original.snapshot().unwrap().source_jobs_registered==before,"read creates no recovery job")?;
   if mode==10{let owner=approvals.original_reconciler();let mut repair=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(approval),crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts,Instant::now()+Duration::from_secs(5)).unwrap();let report=repair.wait(Instant::now()+Duration::from_secs(6)).await.unwrap();require(matches!(report.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired),&format!("original resolver repair {:?}",report.outcome))?;}
  }
  if mode==5 || mode==6 {
   let original=crate::application::native_dispatch::observe_approval_original(&native,approval).unwrap();
   let first=if mode==5{let cut=controller.pause_next(ApprovalCut::LeaseAfterCloseAck,ApprovalCutSelector::ApprovalOperation(approval,crate::session_storage::approval_owner::test_control::ApprovalSqlOperation::Register)).unwrap();let(r,())=tokio::join!(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)),async{let arrival=cut.wait_until_arrived(Instant::now()+Duration::from_secs(5)).await.unwrap();assert!(arrival.actual_close_ack_observed&&arrival.source_job_registered);assert_eq!(arrival.commit,CommitState::Confirmed);controller.fail_result_after_ack(&cut).unwrap();cut.release();});r}else{let fault=controller.deferred_foreign_key_at_commit(ApprovalCutSelector::ApprovalOperation(approval,crate::session_storage::approval_owner::test_control::ApprovalSqlOperation::Register)).unwrap();let r=approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await;let actual=fault.wait_until_fired(Instant::now()+Duration::from_secs(5)).await.unwrap();assert_eq!(actual.sqlite_extended_code,Some(787));assert!(actual.actual_sql_error&&actual.commit_attempted);r};
   require(if mode==5{matches!(first,Err(NecessaryWriteError::SqlCloseUnconfirmed{commit_confirmed:true}))}else{matches!(first,Err(NecessaryWriteError::CommitUnconfirmed))},&format!("real unknown classification {first:?}"))?;
   let debt=original.snapshot().unwrap();require(debt.required_metadata_count>0,"unknown association keeps original gap")?;
   let associations:Vec<String>=sqlx::query_scalar("SELECT target_id FROM main.node_approval_associations").fetch_all(pool).await.unwrap();require(associations==vec![id(5).encode()],&format!("association COMMIT precedes the later register COMMIT: {associations:?}"))?;
   sqlx::query("UPDATE main.product_targets SET native_id='changed-during-unknown' WHERE target_id=?").bind(id(5).encode()).execute(pool).await.unwrap();
   sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,?)").bind(id(6).encode()).bind(id(1).encode()).bind("synthetic-native-B").bind(id(2).encode()).execute(pool).await.unwrap();
   let repeated=approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await;
   require(matches!(repeated,Err(NecessaryWriteError::Rejected(ApprovalRejection::InvalidActor))),"unknown attempt must reject when its original target changed instead of mapping the new target")?;
   for _ in 0..2{let read=approvals.read(&actor,approval).await.unwrap();require(matches!(read.association(),AssociationDisplay::RegisteredSession{target} if *target==id(5)),"the association COMMIT publishes its original target independently of register")?;let _=approvals.list(&actor).await.unwrap();let _=approvals.inspect_decision(&actor,approval).await.unwrap();let mut watch=approvals.watch_status(&actor,approval).await.unwrap();let _=watch.next(&actor,0,Instant::now()+Duration::from_secs(1)).await.unwrap();}
   require(original.snapshot().unwrap().source_jobs_registered==debt.source_jobs_registered,"read/list/watch creates no Source job")?;require(!area.directory.join("approval-selected").exists(),"unknown has zero Selected")?;
   let owner=approvals.original_reconciler();let mut repair=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(approval),crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts,Instant::now()+Duration::from_secs(5)).unwrap();let recovered=repair.wait(Instant::now()+Duration::from_secs(6)).await.unwrap();
   if mode==5{require(matches!(recovered.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired),&format!("same original fixed candidate repair {:?}",recovered.outcome))?;let old=approvals.read(&actor,approval).await.unwrap();require(matches!(old.association(),AssociationDisplay::RegisteredSession{target} if *target==id(5)),"known COMMIT must publish original target")?;require(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await.is_err(),"remapped current actor target rejected")?;sqlx::query("DELETE FROM main.product_targets WHERE target_id=?").bind(id(6).encode()).execute(pool).await.unwrap();sqlx::query("UPDATE main.product_targets SET native_id='synthetic-native-B' WHERE target_id=?").bind(id(5).encode()).execute(pool).await.unwrap();}else{require(matches!(recovered.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::StillBlocked{..}),"old candidate cannot save remapped target")?;let commands:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_commands WHERE approval_id=?").bind(approval.encode()).fetch_one(pool).await.unwrap();require(commands==0,"unknown candidate must not commit a remapped command")?;let association:Vec<String>=sqlx::query_scalar("SELECT target_id FROM main.node_approval_associations WHERE approval_id=?").bind(approval.encode()).fetch_all(pool).await.unwrap();require(association==vec![id(5).encode()],"unknown candidate must retain only its original association")?;require(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await.is_err(),"unknown candidate must not reparse to the later mapping")?;require(!area.directory.join("approval-selected").exists(),"blocked unknown has zero Selected")?;return Ok(());}
  }
  let registration=if mode==4{let(a,b)=tokio::join!(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)),approvals.register_decision(&actor,RegisterDecision::new(approval,choice)));let first=a.map_err(|e|format!("concurrent a {e:?}"))?;b.map_err(|e|format!("concurrent b {e:?}"))?;first}else{approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await.map_err(|e|format!("upgrade register {e:?}"))?};
  let associated=approvals.read(&actor,approval).await.map_err(|e|format!("associated read {e:?}"))?;require(associated.id()==approval&&associated.choices()[0].id()==choice,"original ID and choice preserved")?;let expected=if mode==2{id(6)}else{id(5)};require(matches!(associated.association(),AssociationDisplay::RegisteredSession{target} if *target==expected),"same original must upgrade to exact registered target")?;
  let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM main.node_approval_requests),(SELECT count(*) FROM main.node_approval_associations),(SELECT count(*) FROM main.node_native_dispatches)").fetch_one(pool).await.unwrap();require(counts==(1,1,0),"one request/association and no Run")?;
  approvals.decide(&actor,SubmitDecision::new(registration)).await.map_err(|e|format!("decide {e:?}"))?;
  if mode==8{let owner=approvals.original_reconciler();let permit=timeout(Duration::from_secs(5),async{loop{if let Ok(permit)=owner.prepare_zero_retry(approval){break permit;}tokio::task::yield_now().await;}}).await.map_err(|_|"quarantined original Zero permit absent".to_owned())?;require(owner.prepare_zero_retry(approval).is_err(),"same Zero single nonce")?;let mut retry=owner.start_zero_retry(permit,Instant::now()+Duration::from_secs(5)).map_err(|_|"original Zero retry adoption".to_owned())?;let report=retry.wait(Instant::now()+Duration::from_secs(6)).await.map_err(|e|format!("Zero retry {e:?}"))?;require(matches!(report.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::ReplyStarted{..}),"quarantined Source-only actual retry")?;let serials:Vec<Vec<u8>>=sqlx::query_scalar("SELECT attempt_serial FROM main.node_approval_reply_attempts ORDER BY attempt_serial").fetch_all(pool).await.unwrap();require(serials==vec![0_u64.to_be_bytes().to_vec(),1_u64.to_be_bytes().to_vec()],"exact original two attempts")?;let facts:Vec<String>=sqlx::query_scalar("SELECT transport_state FROM main.node_approval_reply_facts ORDER BY transport_state").fetch_all(pool).await.unwrap();require(facts==vec!["written".to_owned(),"zero".to_owned()],"real Written and real Zero")?;require(owner.prepare_zero_retry(approval).is_err(),"Written cannot retry")?;}
  timeout(Duration::from_secs(5),async{while !area.directory.join("approval-selected").exists(){tokio::task::yield_now().await;}}).await.map_err(|_|"original Selected absent".to_owned())?;
  sqlx::query("UPDATE main.product_targets SET native_id='changed-after-saved' WHERE target_id=?").bind(expected.encode()).execute(pool).await.unwrap();
  let saved=approvals.read(&actor,approval).await.map_err(|e|format!("Saved read {e:?}"))?;require(matches!(saved.association(),AssociationDisplay::RegisteredSession{target} if *target==expected),"Saved association must not replace")?;require(approvals.register_decision(&actor,RegisterDecision::new(approval,choice)).await.is_err(),"Spent/remapped request cannot revive")?;
  Ok(())
 }.await;
 if result.is_err(){let _=native.shutdown(Instant::now()+Duration::from_secs(8)).await;}
 result
 })}).await.unwrap();
    let report = native
        .shutdown(Instant::now() + Duration::from_secs(10))
        .await;
    drop(approvals);
    drop(queue);
    drop(native);
    drop(policy);
    let close = runtime.shutdown(None).await;
    assert_association_close_tail(&area, mode, report, close, &result);
}

async fn seed_association_storage(pool: &sqlx::SqlitePool, area: &TestArea, mode: u8) {
    let child = crate::test_support::approval_owner_dev_child();
    sqlx::query(
        "INSERT INTO main.product_sources VALUES(?,'association development'),(?,'wrong source')",
    )
    .bind(id(1).encode())
    .bind(id(7).encode())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO main.workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(area.directory.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.harnesses VALUES(?,'association development',?,?,NULL)")
        .bind(id(3).encode())
        .bind(child.to_str().unwrap())
        .bind(if mode == 3 || mode == 7 {
            "[\"--unassociated-request\",\"--expect-cancelled\"]"
        } else {
            "[\"--unassociated-request\"]"
        })
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.product_harness_sources VALUES(?,?)")
        .bind(id(3).encode())
        .bind(id(1).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,?)")
        .bind(id(4).encode())
        .bind(id(1).encode())
        .bind("synthetic-native-A")
        .bind(id(2).encode())
        .execute(pool)
        .await
        .unwrap();
}

async fn start_association_native(
    runtime: &NodeRuntime,
    storage: &crate::storage::Storage,
    mode: u8,
) -> (ApprovalQueue, ApprovalNative) {
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let mut runtime_options = options();
    if mode == 8 {
        runtime_options.prompt_write_wait = Duration::ZERO;
    }
    storage
        .start_native_service(
            runtime.request_authority().unwrap(),
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            configuration,
            Arc::new(Retain),
            runtime_options,
        )
        .await
        .unwrap()
}

fn assert_association_close_tail(
    area: &TestArea,
    mode: u8,
    report: crate::application::native_dispatch::NativeShutdownReport,
    close: Result<(), crate::error::AppError>,
    result: &Result<(), String>,
) {
    use std::os::windows::fs::OpenOptionsExt;
    let mut exclusive_failures = Vec::new();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let file = area.data_dir().join(format!("node.sqlite3{suffix}"));
        if !suffix.is_empty() && !file.exists() {
            eprintln!("approval-association first-exclusive {suffix}=absent");
            continue;
        }
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&file)
        {
            Ok(handle) => {
                drop(handle);
                eprintln!("approval-association first-exclusive {suffix}=ok");
            }
            Err(error) => {
                exclusive_failures.push(format!("{suffix}: {error:?}"));
            }
        }
    }
    let marker = area.directory.join(if mode == 3 || mode == 7 {
        "approval-cancelled"
    } else {
        "approval-selected"
    });
    let marker_cleanup = if marker.exists() {
        std::fs::remove_file(marker).err()
    } else {
        None
    };
    let directory_cleanup = std::fs::remove_dir_all(&area.directory).err();
    let diagnostics = format!(
        "report={report:?}; close={close:?}; result={result:?}; exclusive_failures={exclusive_failures:?}; marker_cleanup={marker_cleanup:?}; directory_cleanup={directory_cleanup:?}"
    );
    assert!(
        exclusive_failures.is_empty(),
        "approval SQLite files remain exclusively locked: {diagnostics}"
    );
    assert!(
        marker_cleanup.is_none(),
        "approval marker cleanup failed: {diagnostics}"
    );
    assert!(
        directory_cleanup.is_none(),
        "approval directory cleanup failed: {diagnostics}"
    );
    assert!(
        report.joined && report.metadata_confirmed,
        "first/final native close: {diagnostics}"
    );
    let report_loss = matches!(mode, 5 | 9 | 10);
    match close {
        Ok(()) if !report_loss => {}
        Err(crate::error::AppError::Shutdown(ref message))
            if report_loss
                && message == "request registry SQLite connection closure could not be confirmed" => {}
        other => panic!(
            "approval association close result does not match injected close-report loss \
             (mode={mode}, report_loss={report_loss}): {other:?}"
        ),
    }
    assert!(!area.directory.exists());
    assert!(result.is_ok(), "original association chain: {diagnostics}");
}

async fn wait_association_native(native: &ApprovalNative) {
    while native.status() == NativeRuntimeStatus::Starting {
        tokio::task::yield_now().await;
    }
}
