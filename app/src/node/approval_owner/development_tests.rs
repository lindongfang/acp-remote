// Developer test: real private Node actor, original SQLite/ACP Source and Owned Windows child.
use super::{Id128, NodeRuntime, SigningKey, TestArea, free_address, pair_browser, policy_actor};
use crate::{
    adapters::acp::native_dispatch::AcpNativeAdapter,
    application::{
        native_dispatch::{
            NativeRuntimeStatus,
            dev_tests::{Configuration, Retain, Validator, options},
        },
        request_registry::SystemNodeClock,
        shared_views::budget::ReadReason,
    },
    domain::{
        EntityId,
        approval_owner::{CommitState, DecisionAcceptance, RegisterDecision, SubmitDecision},
    },
    platform::{DpapiProtector, native_source::WindowsSourceLauncher},
};
use std::{sync::Arc, time::Duration};
use tokio::time::{Instant, timeout};
const fn id(v: u8) -> EntityId {
    EntityId::from_bytes([v; 16])
}
type ApprovalQueue = crate::application::queue_owner::QueueService<
    crate::node::NodeRequestAuthority,
    Validator,
    crate::session_storage::request_registry::SqliteRequestRegistry,
    SystemNodeClock,
>;
type ApprovalConfiguration =
    crate::session_storage::native_configuration::SqliteNativeConfigurationProvider<Configuration>;
type ApprovalNative = crate::application::native_dispatch::NativeRuntimeHandle<
    AcpNativeAdapter,
    WindowsSourceLauncher,
    ApprovalConfiguration,
    crate::session_storage::native_dispatch::SqliteNativeDispatch,
>;
type ApprovalService = crate::node::approval_owner::NodeApprovalService<
    AcpNativeAdapter,
    WindowsSourceLauncher,
    ApprovalConfiguration,
    crate::session_storage::native_dispatch::SqliteNativeDispatch,
>;

#[tokio::test]
async fn approval_node_real_no_run_empty_unknown_option_is_saved_before_actual_selected() {
    run_approval(false, false, false, false, 0).await;
}
#[tokio::test]
async fn approval_node_saved_waits_original_observation_sql_before_reserving_selected() {
    run_approval(true, false, false, false, 0).await;
}
#[tokio::test]
async fn approval_node_full_global_audit_warns_without_rolling_back_decision_or_selected() {
    run_approval(false, true, false, false, 0).await;
}
#[tokio::test]
async fn approval_node_actual_deadline_zero_retries_only_same_saved_original_with_new_budget() {
    run_approval(false, false, true, false, 0).await;
}
#[tokio::test]
async fn approval_node_necessary_schema_guard_failure_cancels_original_without_loaded_or_run_then_repairs_only_facts()
 {
    run_approval(false, false, false, true, 0).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_before_begin_has_original_job_and_no_lease() {
    run_approval(false, false, false, false, 1).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_actual_insert_abort_zero_selected_original_cancelled() {
    run_approval(false, false, false, false, 2).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_actual_deferred_commit_787_same_probe_resume() {
    run_approval(false, false, false, false, 3).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_known_commit_lost_report_same_probe_resume() {
    run_approval(false, false, false, false, 4).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_actual_close_ack_loss_keeps_first_unknown() {
    run_approval(false, false, false, false, 5).await;
}
#[cfg(feature = "approval-owner-acceptance-fixtures")]
#[tokio::test]
async fn approval_controller_node_before_enqueue_holds_original_encoded_frame() {
    run_approval(false, false, false, false, 6).await;
}
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "Internal test matrix helper"
)]
async fn run_approval(
    control_gap: bool,
    full_audit: bool,
    zero_retry: bool,
    save_guard: bool,
    controller_mode: u8,
) {
    let area = TestArea::new();
    let runtime = start_approval_runtime(&area).await;
    let key = SigningKey::from_slice(&[91; 32]).unwrap();
    let client = pair_browser(&runtime, &key, "approval owner development").await;
    let policy = runtime.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([92; 16])).await;
    let observer = approval_observer(&runtime, &policy, control_gap).await;
    let storage = runtime.storage.as_ref().unwrap();
    let pool = storage.pool_for_test();
    seed_approval_storage(pool, &area, save_guard, controller_mode).await;
    let (queue, native) = start_approval_native(&runtime, storage, zero_retry).await;
    wait_approval_native(&native).await;
    let approvals = runtime.approval_owner(&native).unwrap();
    let observation_cut=control_gap.then(||storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::ObservationBeforeBegin).unwrap());
    #[cfg(feature = "approval-owner-acceptance-fixtures")]
    let recovery_observation_cut=(controller_mode==4).then(||storage.request_registry().native_metadata_hooks_for_test().pause_next(crate::session_storage::native_metadata_test_hooks::NativeMetadataPoint::ObservationBeforeBegin).unwrap());
    let proof = queue
        .prepare_native_read(&actor, id(4), ReadReason::FirstOpen)
        .await
        .unwrap();
    let acquisition = native
        .acquire_read_source(proof, ReadReason::FirstOpen)
        .unwrap();
    let (loaded,decision)=timeout(Duration::from_secs(12),async{tokio::join!(acquisition.source_capability(),async{
  let display=loop{let mut all=approvals.list(&actor).await.unwrap();if let Some(value)=all.pop() && value.status().request_save.commit==CommitState::Confirmed{break value;}tokio::task::yield_now().await;};
  assert_eq!(display.choices().len(),1);assert_eq!(display.choices()[0].native_option_id(),"");assert!(display.choices()[0].known_kind().is_none());
  let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM main.node_native_dispatches),(SELECT count(*) FROM main.node_native_resource_bindings WHERE lifecycle='loaded')").fetch_one(pool).await.unwrap();assert_eq!(counts,(0,0));
  assert!(!area.directory.join("approval-selected").exists());
  if full_audit {sqlx::query("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO main.node_approval_audit(record_id,approval_id,basis_fingerprint,event_kind,actor_id,target_id,outcome,occurred_at_ms,recorded_at_ms,retain_until_ms,retention_hold) SELECT 'developer-full-'||x,?,randomblob(32),'approval.delivery',NULL,NULL,'unknown',0,0,9000000000000000000,1 FROM n").bind(display.id().encode()).execute(pool).await.unwrap();}
  let before_sequence=display.status().change_sequence;let mut watch=approvals.watch_status(&actor,display.id()).await.unwrap();
  let registration=approvals.register_decision(&actor,RegisterDecision::new(display.id(),display.choices()[0].id())).await.unwrap();
  if save_guard{sqlx::query("CREATE TRIGGER main.approval_developer_guard BEFORE INSERT ON node_approval_decisions BEGIN SELECT RAISE(ABORT,'synthetic guard'); END").execute(pool).await.unwrap();}
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  let controller=approvals.approval_test_controller();
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  assert!(Arc::ptr_eq(&controller,&storage.request_registry().approval_test_controller()));
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  let cut=match controller_mode{1=>Some(controller.pause_next(crate::session_storage::approval_owner::test_control::ApprovalCut::DecideBeforeBegin,crate::session_storage::approval_owner::test_control::ApprovalCutSelector::Approval(display.id())).unwrap()),4=>Some(controller.pause_next(crate::session_storage::approval_owner::test_control::ApprovalCut::DecideAfterCommit,crate::session_storage::approval_owner::test_control::ApprovalCutSelector::Approval(display.id())).unwrap()),5=>Some(controller.pause_next(crate::session_storage::approval_owner::test_control::ApprovalCut::LeaseAfterCloseAck,crate::session_storage::approval_owner::test_control::ApprovalCutSelector::ApprovalOperation(display.id(),crate::session_storage::approval_owner::test_control::ApprovalSqlOperation::Decide)).unwrap()),6=>Some(controller.pause_next(crate::session_storage::approval_owner::test_control::ApprovalCut::BeforeSelectedEnqueue,crate::session_storage::approval_owner::test_control::ApprovalCutSelector::Approval(display.id())).unwrap()),_=>None};
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  let fault=match controller_mode{2=>Some(controller.abort_next_statement(crate::session_storage::approval_owner::test_control::ApprovalStatement::InsertDecision,crate::session_storage::approval_owner::test_control::ApprovalCutSelector::Approval(display.id())).unwrap()),3=>Some(controller.deferred_foreign_key_at_commit(crate::session_storage::approval_owner::test_control::ApprovalCutSelector::Approval(display.id())).unwrap()),_=>None};
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  let accepted=if matches!(controller_mode,1|4|5){let cut=cut.as_ref().unwrap();let (result,())=tokio::join!(approvals.decide(&actor,SubmitDecision::new(registration)),async{let arrival=cut.wait_until_arrived(Instant::now()+Duration::from_secs(5)).await.unwrap();assert!(arrival.source_job_registered);assert_eq!(arrival.tag.approval,Some(display.id()));assert!(arrival.tag.instance.is_some());if controller_mode==1{use sqlx::Connection;assert!(!arrival.lease_attached&&!arrival.begin_started);let mut second=sqlx::SqliteConnection::connect_with(&pool.connect_options()).await.unwrap();sqlx::query("BEGIN IMMEDIATE").execute(&mut second).await.unwrap();sqlx::query("ROLLBACK").execute(&mut second).await.unwrap();second.close().await.unwrap();}else{assert_eq!(arrival.commit,CommitState::Confirmed);if controller_mode==4{controller.lose_commit_report(cut).unwrap();}else{assert!(arrival.actual_close_ack_observed);controller.fail_result_after_ack(cut).unwrap();}}cut.release();});result}else{approvals.decide(&actor,SubmitDecision::new(registration)).await};
  #[cfg(not(feature="approval-owner-acceptance-fixtures"))]
  let accepted=approvals.decide(&actor,SubmitDecision::new(registration)).await;
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  if let Some(fault) = &fault {
        let fired_fault = fault
            .wait_until_fired(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert!(fired_fault.actual_sql_error);
        assert_eq!(
            fired_fault.sqlite_extended_code,
            Some(if controller_mode == 3 { 787 } else { 1811 })
        );
        assert_eq!(fired_fault.commit_attempted, controller_mode == 3);
    }
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  if controller_mode==6{let cut=cut.as_ref().unwrap();let arrival=cut.wait_until_arrived(Instant::now()+Duration::from_secs(5)).await.unwrap();assert!(arrival.source_job_registered);let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_reply_attempts").fetch_one(pool).await.unwrap();assert_eq!(count,1);assert!(!area.directory.join("approval-selected").exists());cut.release();}
  #[cfg(feature="approval-owner-acceptance-fixtures")]
  if matches!(controller_mode,3..=5){assert!(matches!(accepted,Err(crate::domain::approval_owner::NecessaryWriteError::CommitUnconfirmed|crate::domain::approval_owner::NecessaryWriteError::SqlCloseUnconfirmed{commit_confirmed:true})));let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_reply_attempts").fetch_one(pool).await.unwrap();assert_eq!(count,0);assert!(!area.directory.join("approval-selected").exists());let owner=approvals.original_reconciler();let mode=if controller_mode==3{crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts}else{crate::application::native_dispatch::ApprovalRepairMode::RecheckOnly};let mut reconcile=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(display.id()),mode,Instant::now()+Duration::from_secs(5)).unwrap();if controller_mode==4{let cut=recovery_observation_cut.as_ref().unwrap();timeout(Duration::from_secs(5),cut.wait_until_entered()).await.unwrap();let early=timeout(Duration::from_millis(250),reconcile.wait(Instant::now()+Duration::from_secs(1))).await;cut.release();assert!(early.is_err(),"approval reconcile returned before the required Source observation was durable");}let report=reconcile.wait(Instant::now()+Duration::from_secs(6)).await.unwrap();assert!(matches!(report.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired|crate::application::native_dispatch::ApprovalReconcileOutcome::FactsConfirmed));let diagnostic=crate::application::native_dispatch::observe_approval_original(&native,display.id()).unwrap().snapshot().unwrap();let resume=owner.start_resume_saved_decision(display.id(),Instant::now()+Duration::from_secs(5));assert!(resume.is_ok(),"resume rejected after reconcile: required={} ids={:?} allow_disabled={} retry_nonce={} invalidations={} controls={} rejected_admissions={} jobs={}/{}/{}/{} pending={:?}",diagnostic.required_metadata_count,diagnostic.required_metadata,diagnostic.allow_disabled,diagnostic.retry_nonce,diagnostic.invalidations,diagnostic.controls,diagnostic.rejected_admissions,diagnostic.source_jobs_registered,diagnostic.source_jobs_joined,diagnostic.source_jobs_failed,diagnostic.source_jobs_sealed,diagnostic.pending_response);let mut resume=resume.unwrap();let resume_report=resume.wait(Instant::now()+Duration::from_secs(6)).await.unwrap();assert!(matches!(resume_report.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::ReplyStarted{..}),"resume outcome after reconcile: {:?}; required={} ids={:?} allow_disabled={} retry_nonce={} invalidations={} controls={} rejected_admissions={} jobs={}/{}/{}/{} pending={:?}",resume_report.outcome,diagnostic.required_metadata_count,diagnostic.required_metadata,diagnostic.allow_disabled,diagnostic.retry_nonce,diagnostic.invalidations,diagnostic.controls,diagnostic.rejected_admissions,diagnostic.source_jobs_registered,diagnostic.source_jobs_joined,diagnostic.source_jobs_failed,diagnostic.source_jobs_sealed,diagnostic.pending_response);if controller_mode==5{assert_eq!(approvals.inspect_decision(&actor,display.id()).await.unwrap().decision_confirmation.close_report,crate::domain::approval_owner::CloseReportState::Unconfirmed);}}

  if save_guard||controller_mode==2{assert!(matches!(accepted,Err(crate::domain::approval_owner::NecessaryWriteError::Rejected(crate::domain::approval_owner::ApprovalRejection::NecessaryStorageUnavailable))));}else if !matches!(controller_mode,3..=5){assert!(matches!(accepted.unwrap(),DecisionAcceptance::Accepted(_)));}
  let changed=watch.next(&actor,before_sequence,Instant::now()+Duration::from_secs(5)).await.unwrap();assert!(changed.change_sequence>before_sequence);
  if control_gap {sqlx::query("UPDATE main.clients SET status='revoked',auth_generation=auth_generation+1 WHERE client_id=?").bind(&client).execute(pool).await.unwrap();assert!(approvals.read(&actor,display.id()).await.is_err());assert!(matches!(watch.next(&actor,changed.change_sequence,Instant::now()+Duration::from_secs(2)).await,Err(crate::application::native_dispatch::ApprovalWatchError::Rejected(_))));}
  if let Some(cut)=&observation_cut {timeout(Duration::from_secs(5),cut.wait_until_entered()).await.unwrap();let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_reply_attempts").fetch_one(pool).await.unwrap();assert_eq!(count,0);assert!(!area.directory.join("approval-selected").is_file());assert!(native.park_snapshot_for_test(id(4)).unwrap().unwrap().source_required>0);cut.release();}
  if zero_retry{
   let owner=approvals.original_reconciler();
   let permit=timeout(Duration::from_secs(5),async{loop{match owner.prepare_zero_retry(display.id()){Ok(permit)=>break permit,Err(_)=>tokio::task::yield_now().await,}}}).await.unwrap();
   assert!(owner.prepare_zero_retry(display.id()).is_err());
   let mut waiter=owner.start_zero_retry(permit,Instant::now()+Duration::from_secs(5)).unwrap_or_else(|_|panic!("original zero retry adoption failed"));
   let report=waiter.wait(Instant::now()+Duration::from_secs(8)).await.unwrap();assert!(matches!(report.outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::ReplyStarted{..}));
   let serials:Vec<Vec<u8>>=sqlx::query_scalar("SELECT attempt_serial FROM main.node_approval_reply_attempts ORDER BY attempt_serial").fetch_all(pool).await.unwrap();assert_eq!(serials,vec![0_u64.to_be_bytes().to_vec(),1_u64.to_be_bytes().to_vec()]);
   let facts:Vec<(String,Option<String>)>=sqlx::query_as("SELECT transport_state,transport_reason FROM main.node_approval_reply_facts ORDER BY CASE transport_state WHEN 'zero' THEN 0 ELSE 1 END").fetch_all(pool).await.unwrap();assert_eq!(facts,vec![("zero".into(),Some("deadline".into())),("written".into(),None)]);
   let mut checked=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(display.id()),crate::application::native_dispatch::ApprovalRepairMode::RecheckOnly,Instant::now()+Duration::from_secs(5)).unwrap();assert_eq!(checked.wait(Instant::now()+Duration::from_secs(6)).await.unwrap().outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsConfirmed);
   let mut repaired=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(display.id()),crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts,Instant::now()+Duration::from_secs(5)).unwrap();assert_eq!(repaired.wait(Instant::now()+Duration::from_secs(6)).await.unwrap().outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired);
   assert!(owner.prepare_zero_retry(display.id()).is_err());
  }
  let marker=if save_guard||controller_mode==2{"approval-cancelled"}else{"approval-selected"};
  let peer=timeout(Duration::from_secs(5),async{while !area.directory.join(marker).is_file(){tokio::task::yield_now().await;}}).await;

  peer.unwrap();
  if save_guard||controller_mode==2{let decision_count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_decisions").fetch_one(pool).await.unwrap();assert_eq!(decision_count,0);assert!(!area.directory.join("approval-selected").exists());if save_guard{sqlx::query("DROP TRIGGER main.approval_developer_guard").execute(pool).await.unwrap();}let owner=approvals.original_reconciler();let mut repair=owner.start_repair(crate::application::native_dispatch::ApprovalRepairScope::Request(display.id()),crate::application::native_dispatch::ApprovalRepairMode::RepairMissingFacts,Instant::now()+Duration::from_secs(5)).unwrap();assert_eq!(repair.wait(Instant::now()+Duration::from_secs(6)).await.unwrap().outcome,crate::application::native_dispatch::ApprovalReconcileOutcome::FactsRepaired);assert!(owner.start_resume_saved_decision(display.id(),Instant::now()+Duration::from_secs(5)).is_err());}
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM main.node_approval_decisions WHERE approval_id=?").bind(display.id().encode()).fetch_one(pool).await.unwrap();assert_eq!(count,1);
  display.id()
 })}).await.unwrap();
    assert!(loaded.is_ok());
    let _ = decision;
    let report = native
        .shutdown(Instant::now() + Duration::from_secs(10))
        .await;
    let current_observer = observer.as_ref().unwrap_or(&actor);
    assert_closed_approval(
        &approvals,
        pool,
        current_observer,
        decision,
        full_audit,
        zero_retry,
    )
    .await;
    drop(approvals);
    drop(queue);
    drop(native);
    drop(policy);
    let node_closed = runtime.shutdown(None).await;
    assert_approval_close_tail(&area, report, node_closed, save_guard, controller_mode);
}

async fn start_approval_runtime(area: &TestArea) -> NodeRuntime {
    NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(DpapiProtector),
    )
    .await
    .unwrap()
}

async fn approval_observer(
    runtime: &NodeRuntime,
    policy: &crate::access_policy::ApplicationPolicy,
    control_gap: bool,
) -> Option<crate::access_policy::AuthenticatedActor> {
    if control_gap {
        let key = SigningKey::from_slice(&[93; 32]).unwrap();
        let client = pair_browser(runtime, &key, "approval current observer").await;
        Some(policy_actor(policy, &key, &client, Id128::new([94; 16])).await)
    } else {
        None
    }
}

async fn wait_approval_native(native: &ApprovalNative) {
    timeout(Duration::from_secs(5), async {
        while native.status() == NativeRuntimeStatus::Starting {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn seed_approval_storage(
    pool: &sqlx::SqlitePool,
    area: &TestArea,
    save_guard: bool,
    controller_mode: u8,
) {
    let child = crate::test_support::approval_owner_dev_child();
    assert!(child.is_file());
    sqlx::query("INSERT INTO main.product_sources VALUES(?,'approval developer')")
        .bind(id(1).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.workspaces VALUES(?,?)")
        .bind(id(2).encode())
        .bind(area.directory.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.harnesses VALUES(?,'approval developer',?,?,NULL)")
        .bind(id(3).encode())
        .bind(child.to_str().unwrap())
        .bind(if save_guard || controller_mode == 2 {
            "[\"--expect-cancelled\"]"
        } else {
            "[]"
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

async fn start_approval_native(
    runtime: &NodeRuntime,
    storage: &crate::storage::Storage,
    zero_retry: bool,
) -> (ApprovalQueue, ApprovalNative) {
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let mut native_options = options();
    if zero_retry {
        native_options.prompt_write_wait = Duration::ZERO;
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
            native_options,
        )
        .await
        .unwrap()
}

async fn assert_closed_approval(
    approvals: &ApprovalService,
    pool: &sqlx::SqlitePool,
    current_observer: &crate::access_policy::AuthenticatedActor,
    decision: crate::domain::approval_owner::ApprovalId,
    full_audit: bool,
    zero_retry: bool,
) {
    let closed_status = approvals
        .inspect_decision(current_observer, decision)
        .await
        .unwrap();
    assert_eq!(closed_status.invalidations.len(), 1);
    assert!(matches!(
        closed_status.source_close,
        crate::domain::approval_owner::SourceCloseDisplay::Confirmed
    ));
    let warnings = approvals
        .audit_warnings(current_observer, decision)
        .await
        .unwrap();
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM main.node_approval_audit")
        .fetch_one(pool)
        .await
        .unwrap();
    if full_audit {
        assert_eq!(audit_count, 100_000);
        assert!(warnings.contains(&crate::domain::approval_audit::ApprovalAuditWarning::Full));
    } else {
        assert_eq!(audit_count, if zero_retry { 5 } else { 4 });
        assert!(
            warnings.is_empty(),
            "healthy default audit warnings {warnings:?}"
        );
        let kinds: Vec<String> = sqlx::query_scalar(
            "SELECT event_kind FROM main.node_approval_audit ORDER BY event_kind",
        )
        .fetch_all(pool)
        .await
        .unwrap();
        assert_eq!(
            kinds,
            if zero_retry {
                vec![
                    "approval.decided",
                    "approval.delivery",
                    "approval.delivery",
                    "approval.invalidated",
                    "approval.registered",
                ]
            } else {
                vec![
                    "approval.decided",
                    "approval.delivery",
                    "approval.invalidated",
                    "approval.registered",
                ]
            }
        );
    }
}

fn assert_approval_close_tail(
    area: &TestArea,
    report: crate::application::native_dispatch::NativeShutdownReport,
    node_closed: Result<(), crate::error::AppError>,
    save_guard: bool,
    controller_mode: u8,
) {
    let report_loss = controller_mode == 5;
    match node_closed {
        Ok(()) if !report_loss => {}
        Err(crate::error::AppError::Shutdown(ref message))
            if report_loss
                && message
                    == "request registry SQLite connection closure could not be confirmed" => {}
        other => panic!(
            "approval close result does not match injected close-report loss \
             (report_loss={report_loss}): {other:?}"
        ),
    }
    assert!(
        report.joined && report.metadata_confirmed,
        "first actual native shutdown {report:?}"
    );
    let marker = area.directory.join(if save_guard || controller_mode == 2 {
        "approval-cancelled"
    } else {
        "approval-selected"
    });
    assert!(marker.is_file());
    std::fs::remove_file(marker).unwrap();
    std::fs::remove_dir_all(&area.directory).unwrap();
    assert!(!area.directory.exists());
}

#[cfg(feature = "approval-owner-acceptance-fixtures")]
mod association {
    use super::*;
    include!("association_development.rs");
}
