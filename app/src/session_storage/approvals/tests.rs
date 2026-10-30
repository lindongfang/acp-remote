//! Real SQLite development tests; no Harness or issuer authentication is simulated as a product claim.
use super::*;
use crate::{
    domain::{
        InputRequest,
        history::{ContentVersion, HistoryItem, ItemIdentity, NativeToolCallId, SourceOrigin},
    },
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Area {
    directory: PathBuf,
}
impl Area {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acp-wp2-approval-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).expect("owned fixture");
        Self { directory }
    }
    fn database(&self) -> PathBuf {
        self.directory.join("node.sqlite3")
    }
}
impl Drop for Area {
    fn drop(&mut self) {
        assert_eq!(
            self.directory.parent(),
            Some(std::env::temp_dir().as_path())
        );
        assert!(
            self.directory
                .file_name()
                .expect("name")
                .to_string_lossy()
                .starts_with("acp-wp2-approval-")
        );
        if self.directory.exists() {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
}
fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}
async fn seed(storage: &Storage) -> OutputScope {
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "fixture".into(),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("meta");
    sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,X'01','fixture','trusted',1,0)").bind(id(1).encode()).execute(storage.pool_for_test()).await.expect("client");
    sqlx::query("INSERT INTO harnesses VALUES('h','fixture','not-launched','[]',NULL)")
        .execute(storage.pool_for_test())
        .await
        .expect("harness");
    sqlx::query("INSERT INTO workspaces VALUES('w','fixture')")
        .execute(storage.pool_for_test())
        .await
        .expect("workspace");
    sqlx::query(
        "INSERT INTO sessions(session_id,harness_id,workspace_id,state) VALUES(?,'h','w','loaded')",
    )
    .bind(id(3).encode())
    .execute(storage.pool_for_test())
    .await
    .expect("session");
    sqlx::query("INSERT INTO session_state VALUES(?,0,0,0)")
        .bind(id(3).encode())
        .execute(storage.pool_for_test())
        .await
        .expect("sequence");
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    let reference = storage
        .sessions()
        .persist_input(actor, id(3), b"synthetic input")
        .await
        .expect("input content");
    let accepted = storage
        .sessions()
        .admit_input(
            actor,
            InputRequest {
                request_id: id(5),
                session_id: id(3),
                content_ref: reference,
            },
        )
        .await
        .expect("accepted");
    OutputScope {
        session: SessionId(id(3)),
        run: RunId(EntityId::decode(&accepted.command.input.run_id).expect("run")),
        turn: TurnId(EntityId::decode(&accepted.command.input.turn_id).expect("turn")),
    }
}

fn actor() -> ActorContext {
    ActorContext {
        client_id: id(1),
        auth_generation: 1,
    }
}
async fn fixture() -> (Area, Storage, ApprovalObservation) {
    let area = Area::new();
    let storage = Storage::open(&area.database()).await.expect("open");
    let scope = seed(&storage).await;
    let native_session =
        NativeSessionId::new(Arc::from("native-session-sensitive")).expect("native session");
    sqlx::query("UPDATE sessions SET native_session_id=? WHERE session_id=?")
        .bind(native_session.as_str())
        .bind(scope.session.encode())
        .execute(storage.pool_for_test())
        .await
        .expect("native");
    let connection = ConnectionEpoch {
        instance: InstanceId(id(30)),
        generation: 1,
    };
    let store = storage.approvals();
    store.bind_connection(connection).await.expect("connection");
    store
        .bind_run(scope, connection, &native_session)
        .await
        .expect("run");
    let native = NativeToolCallId::new(Arc::from("tool-sensitive")).expect("tool");
    let positions = storage
        .histories()
        .reserve_positions(scope, 1)
        .await
        .expect("positions");
    let item = HistoryItem {
        identity: Arc::new(ItemIdentity {
            scope,
            item: ItemId(id(31)),
            source: SourceId(id(32)),
            position: crate::domain::history::Position(positions.next),
            kind: Arc::from("tool"),
            protocol: Arc::from("acp"),
            protocol_version: Arc::from("1"),
            tool_call_id: Some(native.clone()),
            tool_origin: Some(SourceOrigin {
                connection,
                native_session: native_session.clone(),
            }),
        }),
        version: ContentVersion::new(1).expect("version"),
        content: RawContent(Arc::from(b"sensitive-tool-body".as_slice())),
    };
    storage
        .histories()
        .commit_items(
            scope,
            &[item],
            crate::domain::history::IntegrityChange::Keep,
        )
        .await
        .expect("tool commit");
    let observation = ApprovalObservation {
        binding: ApprovalBinding {
            scope,
            connection,
            native_session,
            native_request: NativeRequestId::integer(1).expect("native request"),
        },
        kind: PermissionKind::ToolPermission,
        tool_call_id: Some(native),
        options: vec![
            NativePermissionOption {
                native_id: Arc::from("native-allow-sensitive"),
                name: Arc::from("allow sensitive"),
                kind: PermissionOptionKind::AllowOnce,
            },
            NativePermissionOption {
                native_id: Arc::from("native-deny-sensitive"),
                name: Arc::from("deny sensitive"),
                kind: PermissionOptionKind::RejectOnce,
            },
        ],
        scope_understood: true,
        scope_raw: RawContent(Arc::from(b"{\"scope\":\"sensitive\"}".as_slice())),
        raw_request: RawContent(Arc::from(
            b"{\"unknown\":[1,true,{\"secret\":\"sensitive\"}]}".as_slice(),
        )),
        protocol: Arc::from("acp"),
        protocol_version: Arc::from("1"),
    };
    (area, storage, observation)
}
async fn count(storage: &Storage, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(storage.pool_for_test())
        .await
        .expect("count")
}
fn input(view: &ApprovalView, request: u8, choice: usize) -> DecisionInput {
    DecisionInput {
        session: view.binding.scope.session,
        approval: view.id,
        request_id: id(request),
        choice: view.choices[choice].choice_id,
    }
}
#[tokio::test]
async fn approval_structure_reference_and_typed_namespaces() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (_area, storage, mut observation) = fixture().await;
        let store = storage.approvals();
        let view = store.observe(&observation).await.expect("observe");
        assert_eq!(view.state, ApprovalState::Pending);
        assert_eq!(view.raw_request.0, observation.raw_request.0);
        assert_eq!(view.scope_raw.0, observation.scope_raw.0);
        assert!(view.tool.is_some());
        assert_eq!(
            store.observe(&observation).await.expect("repeat").id,
            view.id
        );
        observation.binding.native_request = NativeRequestId::string("1").expect("string id");
        let other = store.observe(&observation).await.expect("string distinct");
        assert_ne!(other.id, view.id);
        observation.tool_call_id =
            Some(NativeToolCallId::new(Arc::from("unknown tool")).expect("id"));
        assert!(matches!(
            store.observe(&observation).await,
            Err(ApprovalError::SourceConflict)
        ));
        observation.binding.native_request = NativeRequestId::integer(3).expect("id");
        let unsupported = store
            .observe(&observation)
            .await
            .expect("unsupported preserved");
        assert_eq!(
            unsupported.restriction,
            Some(ApprovalRestriction::UnresolvedTool)
        );
        assert!(matches!(
            store.decide(actor(), input(&unsupported, 41, 0)).await,
            Err(ApprovalError::UnsupportedRequest)
        ));
    })
    .await
    .expect("bounded");
}
#[tokio::test]
async fn approval_once_decision_replay_claim_and_transport_fact() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (_area, storage, observation) = fixture().await;
        let store = storage.approvals();
        let view = store.observe(&observation).await.expect("observe");
        let request = input(&view, 41, 0);
        let decision = store.decide(actor(), request).await.expect("decision");
        let audit = count(&storage, "security_audit").await;
        let replay = store.decide(actor(), request).await.expect("replay");
        assert!(replay.replayed);
        assert_eq!(replay.command, decision.command);
        assert_eq!(count(&storage, "security_audit").await, audit);
        assert!(matches!(
            store.decide(actor(), input(&view, 41, 1)).await,
            Err(ApprovalError::RequestConflict)
        ));
        let claim = store.claim_delivery(actor(), view.id).await.expect("claim");
        assert!(store.claim_current(&claim).await.expect("current"));
        assert!(matches!(
            store.claim_delivery(actor(), view.id).await,
            Err(ApprovalError::DeliveryAlreadyClaimed)
        ));
        let report = store
            .report_transport(&claim, TransportReport::WriteCompleted)
            .await
            .expect("transport report");
        assert_eq!(report.state, DeliveryState::TransportWritten);
        assert_eq!(
            store
                .report_transport(&claim, TransportReport::WriteCompleted)
                .await
                .expect("repeat"),
            report
        );
        assert!(matches!(
            store
                .report_transport(&claim, TransportReport::Unknown)
                .await,
            Err(ApprovalError::ReportConflict)
        ));
        let rows = sqlx::query("SELECT action,target_id,outcome FROM security_audit")
            .fetch_all(storage.pool_for_test())
            .await
            .expect("audit");
        for row in rows {
            for name in ["action", "target_id", "outcome"] {
                assert!(
                    !row.try_get::<String, _>(name)
                        .expect("metadata")
                        .contains("sensitive")
                );
            }
        }
    })
    .await
    .expect("bounded");
}
#[tokio::test]
async fn approval_concurrent_opposite_actors_one_winner() {
    tokio::time::timeout(Duration::from_secs(15),async{
 let (area,storage,observation)=fixture().await;sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,X'02','second','trusted',1,0)").bind(id(2).encode()).execute(storage.pool_for_test()).await.expect("second actor");let store=storage.approvals();let view=store.observe(&observation).await.expect("observe");let independent=Storage::open(&area.database()).await.expect("independent pool");let other=independent.approvals();let (a,b)=tokio::join!(store.decide(actor(),input(&view,41,0)),other.decide(ActorContext{client_id:id(2),auth_generation:1},input(&view,42,1)));assert_ne!(a.is_ok(),b.is_ok());assert_eq!(count(&storage,"approval_deliveries").await,1);assert_eq!(count(&storage,"session_commands").await,2);
}).await.expect("bounded");
}

#[tokio::test]
async fn approval_lifecycle_old_epoch_restart_and_unknown_never_retry() {
    tokio::time::timeout(Duration::from_secs(20), async {
        for event in [
            RunLifecycle::CancelRequested,
            RunLifecycle::Completed,
            RunLifecycle::Cancelled,
            RunLifecycle::Failed,
        ] {
            let (_area, storage, observation) = fixture().await;
            let store = storage.approvals();
            let view = store.observe(&observation).await.expect("observe");
            store
                .decide(actor(), input(&view, 41, 0))
                .await
                .expect("decision");
            let claim = store.claim_delivery(actor(), view.id).await.expect("claim");
            store
                .invalidate_run(
                    observation.binding.scope,
                    observation.binding.connection,
                    event,
                )
                .await
                .expect("invalidate");
            assert!(!store.claim_current(&claim).await.expect("old claim"));
            let report = store
                .report_transport(&claim, TransportReport::WriteCompleted)
                .await
                .expect("late transport fact");
            assert_eq!(report.state, DeliveryState::Unknown);
            assert!(!report.source_current_at_report);
            assert!(store.claim_delivery(actor(), view.id).await.is_err());
            if event == RunLifecycle::CancelRequested {
                let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE run_id=?")
                    .bind(observation.binding.scope.run.encode())
                    .fetch_one(storage.pool_for_test())
                    .await
                    .expect("run");
                assert_eq!(status, "cancelling");
            }
        }
        let (_area, storage, observation) = fixture().await;
        let store = storage.approvals();
        let view = store.observe(&observation).await.expect("observe");
        let request = input(&view, 41, 0);
        let command = store
            .decide(actor(), request)
            .await
            .expect("decision")
            .command;
        let claim = store.claim_delivery(actor(), view.id).await.expect("claim");
        storage
            .sessions()
            .recover_interrupted()
            .await
            .expect("recovery");
        assert!(!store.claim_current(&claim).await.expect("old claim"));
        assert!(store.observe(&observation).await.is_err());
        assert!(
            store
                .decide(actor(), request)
                .await
                .expect("stable accepted decision")
                .replayed
        );
        let status: String =
            sqlx::query_scalar("SELECT status FROM session_commands WHERE command_id=?")
                .bind(command.encode())
                .fetch_one(storage.pool_for_test())
                .await
                .expect("command");
        assert_eq!(status, "accepted");
        assert!(
            store
                .bind_connection(observation.binding.connection)
                .await
                .is_err()
        );
        let (_area, storage, observation) = fixture().await;
        let store = storage.approvals();
        let view = store.observe(&observation).await.expect("observe");
        let newer = ConnectionEpoch {
            generation: 2,
            ..observation.binding.connection
        };
        store.bind_connection(newer).await.expect("new epoch");
        assert!(store.decide(actor(), input(&view, 41, 0)).await.is_err());
        assert!(store.observe(&observation).await.is_err());
    })
    .await
    .expect("bounded");
}
#[tokio::test]
async fn approval_transaction_faults_rollback_decision_and_claim() {
    tokio::time::timeout(Duration::from_secs(25),async{
 for trigger in ["CREATE TRIGGER fail BEFORE INSERT ON session_commands WHEN NEW.action='approval.decide' BEGIN SELECT RAISE(ABORT,'synthetic');END", "CREATE TRIGGER fail BEFORE UPDATE OF status ON approvals WHEN NEW.status='decided' BEGIN SELECT RAISE(ABORT,'synthetic');END", "CREATE TRIGGER fail BEFORE INSERT ON security_audit WHEN NEW.action='approval.decide' BEGIN SELECT RAISE(ABORT,'synthetic');END", "CREATE TRIGGER fail BEFORE INSERT ON approval_audit_links BEGIN SELECT RAISE(ABORT,'synthetic');END"]{
 let (_area,storage,observation)=fixture().await;let store=storage.approvals();let view=store.observe(&observation).await.expect("observe");let audits=count(&storage,"security_audit").await;sqlx::query(trigger).execute(storage.pool_for_test()).await.expect("fault trigger");assert!(matches!(store.decide(actor(),input(&view,41,0)).await,Err(ApprovalError::StorageUnavailable)));assert_eq!(count(&storage,"session_commands").await,1);assert_eq!(count(&storage,"security_audit").await,audits);assert_eq!(store.view(actor(),view.id).await.expect("view").state,ApprovalState::Pending);sqlx::query("DROP TRIGGER fail").execute(storage.pool_for_test()).await.expect("remove fault");store.decide(actor(),input(&view,41,0)).await.expect("retry after rollback");}
 for trigger in ["CREATE TRIGGER fail BEFORE UPDATE ON approval_deliveries WHEN NEW.state='claimed' BEGIN SELECT RAISE(ABORT,'synthetic');END", "CREATE TRIGGER fail BEFORE INSERT ON security_audit WHEN NEW.action='approval.claim' BEGIN SELECT RAISE(ABORT,'synthetic');END"]{
 let (_area,storage,observation)=fixture().await;let store=storage.approvals();let view=store.observe(&observation).await.expect("observe");store.decide(actor(),input(&view,41,0)).await.expect("decision");let audits=count(&storage,"security_audit").await;sqlx::query(trigger).execute(storage.pool_for_test()).await.expect("fault");assert!(store.claim_delivery(actor(),view.id).await.is_err());assert_eq!(store.view(actor(),view.id).await.expect("view").delivery,DeliveryState::NotSent);assert_eq!(count(&storage,"security_audit").await,audits);sqlx::query("DROP TRIGGER fail").execute(storage.pool_for_test()).await.expect("remove fault");let claim=store.claim_delivery(actor(),view.id).await.expect("claim after rollback");sqlx::query("CREATE TRIGGER fail BEFORE INSERT ON security_audit WHEN NEW.action='approval.transport' BEGIN SELECT RAISE(ABORT,'synthetic');END").execute(storage.pool_for_test()).await.expect("report fault");assert!(store.report_transport(&claim,TransportReport::Unknown).await.is_err());assert_eq!(store.view(actor(),view.id).await.expect("view").delivery,DeliveryState::Claimed);assert!(store.claim_delivery(actor(),view.id).await.is_err());}
}).await.expect("bounded");
}
#[tokio::test]
async fn approval_unsupported_option_wrong_generation_and_revoked_actor() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (_area, storage, mut observation) = fixture().await;
        let store = storage.approvals();
        observation.options[0].kind = PermissionOptionKind::Unsupported(Arc::from("future-kind"));
        let view = store.observe(&observation).await.expect("retain unknown");
        assert_eq!(view.restriction, Some(ApprovalRestriction::UnknownOption));
        assert!(store.decide(actor(), input(&view, 41, 0)).await.is_err());
        observation.binding.native_request = NativeRequestId::integer(2).expect("id");
        observation.binding.native_session =
            NativeSessionId::new(Arc::from("other native session")).expect("id");
        assert!(store.observe(&observation).await.is_err());
        sqlx::query("UPDATE clients SET status='revoked',auth_generation=2 WHERE client_id=?")
            .bind(actor().client_id.encode())
            .execute(storage.pool_for_test())
            .await
            .expect("revoke");
        assert!(matches!(
            store.view(actor(), view.id).await,
            Err(ApprovalError::InvalidActor)
        ));
    })
    .await
    .expect("bounded");
}

#[tokio::test]
async fn approval_real_deferred_commit_faults_keep_all_associations() {
    tokio::time::timeout(Duration::from_secs(20),async{
 for phase in ["decision","claim","report"]{let (_area,storage,observation)=fixture().await;let store=storage.approvals();let view=store.observe(&observation).await.expect("observe");if phase!="decision"{store.decide(actor(),input(&view,41,0)).await.expect("decision");}let claim=if phase=="report"{Some(store.claim_delivery(actor(),view.id).await.expect("claim"))}else{None};let commands=count(&storage,"session_commands").await;let audits=count(&storage,"security_audit").await;
 sqlx::query("CREATE TABLE commit_fault(id TEXT REFERENCES sessions(session_id) DEFERRABLE INITIALLY DEFERRED)").execute(storage.pool_for_test()).await.expect("deferred fixture");let action=match phase{"decision"=>"approval.decide","claim"=>"approval.claim",_=>"approval.transport"};sqlx::query(&format!("CREATE TRIGGER commit_fail AFTER INSERT ON security_audit WHEN NEW.action='{action}' BEGIN INSERT INTO commit_fault VALUES('missing'); END")).execute(storage.pool_for_test()).await.expect("commit fault");
 let failed=match phase{"decision"=>store.decide(actor(),input(&view,41,0)).await.is_err(),"claim"=>store.claim_delivery(actor(),view.id).await.is_err(),_=>store.report_transport(claim.as_ref().expect("claim"),TransportReport::Unknown).await.is_err()};assert!(failed);assert_eq!(count(&storage,"session_commands").await,commands);assert_eq!(count(&storage,"security_audit").await,audits);assert_eq!(count(&storage,"commit_fault").await,0);let actual=store.view(actor(),view.id).await.expect("view");assert_eq!(actual.delivery,if phase=="report"{DeliveryState::Claimed}else{DeliveryState::NotSent});}
}).await.expect("bounded");
}
#[tokio::test]
async fn approval_input_shares_actor_request_namespace() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (_area, storage, observation) = fixture().await;
        let store = storage.approvals();
        let view = store.observe(&observation).await.expect("observe");
        assert!(matches!(
            store.decide(actor(), input(&view, 5, 0)).await,
            Err(ApprovalError::RequestConflict)
        ));
        store
            .decide(actor(), input(&view, 41, 0))
            .await
            .expect("decision");
        let content = storage
            .sessions()
            .persist_input(actor(), view.binding.scope.session.0, b"future input")
            .await
            .expect("content");
        let request = InputRequest {
            request_id: id(41),
            session_id: view.binding.scope.session.0,
            content_ref: content,
        };
        assert!(matches!(
            storage.sessions().admit_input(actor(), request).await,
            Err(crate::domain::SessionError::RequestConflict)
        ));
    })
    .await
    .expect("bounded");
}

struct Clock;
impl crate::application::history::HistoryClock for Clock {
    fn millis(&self) -> u64 {
        0
    }
}
struct OutputFixture;
impl crate::application::history::OutputControl for OutputFixture {
    fn request_stop(
        &self,
        _: OutputScope,
        _: crate::domain::history::IntegrityFault,
    ) -> crate::domain::history::StopReceipt {
        crate::domain::history::StopReceipt::Requested
    }
    fn finish_stop(
        &self,
        _: OutputScope,
    ) -> impl std::future::Future<Output = Result<(), crate::domain::history::HistoryError>> {
        std::future::ready(Ok(()))
    }
}
#[tokio::test]
async fn approval_forced_flush_failure_and_changed_checkpoint_accept_nothing() {
    tokio::time::timeout(Duration::from_secs(20),async{
 use crate::application::{history::HistoryCoordinator,approvals::ApprovalService};
 let (_area,storage,observation)=fixture().await;let history=HistoryCoordinator::new(storage.histories(),Clock,OutputFixture);let scope=observation.binding.scope;history.attach(scope).await.expect("attach");let service=ApprovalService::new(storage.approvals(),history.clone());let prepared=service.prepare_observation(observation.clone()).await.expect("prepare");let tail=history.begin_item(scope,SourceId(id(60)),Arc::from("text"),Arc::from("acp"),Arc::from("1"),b"new dirty tail".to_vec()).await.expect("tail");assert!(matches!(service.commit_observation(prepared).await,Err(ApprovalError::HistoryUnavailable)));assert_eq!(count(&storage,"approvals").await,0);
 sqlx::query("CREATE TRIGGER history_fault BEFORE INSERT ON content_objects WHEN NEW.purpose='output' BEGIN SELECT RAISE(ABORT,'synthetic');END").execute(storage.pool_for_test()).await.expect("history fault");assert!(matches!(service.prepare_observation(observation.clone()).await,Err(ApprovalError::HistoryUnavailable)));assert_eq!(count(&storage,"approvals").await,0);assert_eq!(count(&storage,"session_commands").await,1);sqlx::query("DROP TRIGGER history_fault").execute(storage.pool_for_test()).await.expect("remove fault");
 // The controlled clock remains at zero after failure: use a fresh owner to preserve minimum retry delay.
 assert!(history.close(Duration::from_millis(50)).await.is_err());drop(service);drop(history);
 let history=HistoryCoordinator::new(storage.histories(),Clock,OutputFixture);history.attach(scope).await.expect("new fixture owner");let service=ApprovalService::new(storage.approvals(),history.clone());let view=service.commit_observation(service.prepare_observation(observation).await.expect("prepare")).await.expect("observe");let decision=service.prepare_decision(actor(),input(&view,41,0)).await.expect("prepare decision");history.begin_item(scope,SourceId(id(61)),Arc::from("text"),Arc::from("acp"),Arc::from("1"),b"changed".to_vec()).await.expect("dirty");assert!(matches!(service.commit_decision(decision).await,Err(ApprovalError::HistoryUnavailable)));assert_eq!(count(&storage,"session_commands").await,1);service.commit_decision(service.prepare_decision(actor(),input(&view,41,0)).await.expect("reflush")).await.expect("decision");let delivery=service.prepare_delivery(actor(),view.id).await.expect("prepare delivery");history.begin_item(scope,SourceId(id(62)),Arc::from("text"),Arc::from("acp"),Arc::from("1"),b"changed again".to_vec()).await.expect("dirty");assert!(matches!(service.claim_delivery(delivery).await,Err(ApprovalError::HistoryUnavailable)));assert_eq!(storage.approvals().view(actor(),view.id).await.expect("view").delivery,DeliveryState::NotSent);history.close(Duration::from_secs(1)).await.expect("owned close");drop(tail);
}).await.expect("bounded");
}

#[tokio::test]
async fn approval_tool_reference_never_crosses_source_scope() {
    tokio::time::timeout(Duration::from_secs(20),async{
 for mutation in ["UPDATE conversation_items SET tool_generation=0 WHERE kind='tool'", "UPDATE conversation_items SET tool_instance_id='another-instance' WHERE kind='tool'", "UPDATE conversation_items SET tool_native_session_id='other-native',tool_native_session_digest='other-digest' WHERE kind='tool'", "UPDATE conversation_items SET native_tool_id='other-tool',native_tool_digest='other-digest' WHERE kind='tool'"]{let (_area,storage,observation)=fixture().await;sqlx::query(mutation).execute(storage.pool_for_test()).await.expect("different historical source fixture");let view=storage.approvals().observe(&observation).await.expect("fail closed request");assert_eq!(view.restriction,Some(ApprovalRestriction::UnresolvedTool));assert!(view.tool.is_none());assert!(storage.approvals().decide(actor(),input(&view,41,0)).await.is_err());}
 let (_area,storage,observation)=fixture().await;let mut wrong=observation.clone();wrong.binding.scope.session=SessionId(id(99));assert!(storage.approvals().observe(&wrong).await.is_err());
}).await.expect("bounded");
}
