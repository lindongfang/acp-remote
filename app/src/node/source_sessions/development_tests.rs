// Real SQLite migration checks; the in-memory fixture does not claim Native IO or Node close.
use crate::session_storage::schema;
use sqlx::SqlitePool;

async fn version10_fixture() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    schema::initialize(&pool).await.unwrap();
    sqlx::query("INSERT INTO node_meta VALUES(1,10,'fixture-node',X'01','fixture-key')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO clients VALUES('fixture-client',X'02','retained-client','trusted',3,11,NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DROP TABLE node_source_session_operations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE node_source_session_registrations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE node_meta SET schema_version=10")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA user_version=10")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

#[tokio::test]
async fn source_session_schema10_migrates9_and_reopens_without_replacing_identity() {
    let pool = version10_fixture().await;
    let result=async {
        schema::initialize(&pool).await.unwrap();
        schema::initialize(&pool).await.unwrap();
        let version: i64=sqlx::query_scalar("PRAGMA user_version").fetch_one(&pool).await.unwrap();
        let identity: (i64,String,Vec<u8>,String)=sqlx::query_as("SELECT schema_version,node_id,public_key,key_file_ref FROM node_meta").fetch_one(&pool).await.unwrap();
        let client: (String,i64)=sqlx::query_as("SELECT display_name,auth_generation FROM clients WHERE client_id='fixture-client'").fetch_one(&pool).await.unwrap();
        let objects: i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('node_source_session_registrations','node_source_session_operations','node_source_session_runtime_operations','node_source_session_created_owners')").fetch_one(&pool).await.unwrap();
        (version,identity,client,objects)
    }.await;
    pool.close().await;
    assert_eq!(
        result,
        (
            11,
            (11, "fixture-node".into(), vec![1], "fixture-key".into()),
            ("retained-client".into(), 3),
            4
        )
    );
}

#[tokio::test]
async fn source_session_schema10_interruption_rolls_back_and_partial_namespace_is_rejected() {
    let pool = version10_fixture().await;
    let interrupted = schema::initialize_for_test(&pool, schema::MigrationFault::Commit)
        .await
        .is_err();
    let head: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .unwrap();
    let objects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'node_source_session_%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE VIEW NODE_SOURCE_SESSION_CREATED_OWNERS AS SELECT 1 AS retained")
        .execute(&pool)
        .await
        .unwrap();
    let rejected = matches!(
        schema::initialize(&pool).await,
        Err(crate::error::StorageError::UnrecognizedSchema)
    );
    let retained: i64 =
        sqlx::query_scalar("SELECT retained FROM NODE_SOURCE_SESSION_CREATED_OWNERS")
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    assert!(interrupted, "original commit checkpoint must fail");
    assert_eq!((head, objects), (10, 0));
    assert!(
        rejected,
        "partial case-insensitive namespace must be rejected"
    );
    assert_eq!(
        retained, 1,
        "rejected migration preserves the conflicting object"
    );
}

use super::*;
use crate::{
    application::{
        document_transfer::{DocumentEnqueue, DocumentPrepare, DocumentTransfer},
        request_registry::RegistryDeadline,
        shared_views::{
            socket_authority::SocketViewClient,
            wire::{RecordKind, TransferHeader},
        },
    },
    domain::{
        native_catalog::{CatalogDraft, CatalogResult, SourceSelection},
        source_sessions::*,
    },
};
use futures_util::FutureExt;
use std::{future::Future, panic::AssertUnwindSafe, pin::Pin};
const fn source_id(n: u8) -> crate::domain::EntityId {
    crate::domain::EntityId::from_bytes([n; 16])
}
fn source_deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(10))
}
fn source_utf16(value: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
struct SourceCase {
    area: TestArea,
    owner: Option<NodeProcessOwner>,
    node: Option<NodeRuntime>,
    peers: Vec<ManagementSocket>,
    clients: Vec<SocketViewClient>,
    endpoints: Vec<crate::management_ws::DocumentSocketEndpoint>,
    selection: Option<SourceLaunchSelection>,
}
impl SourceCase {
    async fn start(&mut self, scenario: &str) {
        self.owner = Some(NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            self.area.config(free_address()),
            Arc::new(FakeProtector::default()),
        ));
        self.node = Some(self.owner.as_ref().unwrap().ready().await.unwrap());
        let node = self.node.as_ref().unwrap();
        let pool = node.storage.as_ref().unwrap().pool_for_test();
        let launch = self
            .area
            .directory
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        sqlx::query("INSERT INTO product_sources VALUES(?,'owned source-session fixture')")
            .bind(source_id(1).encode())
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces VALUES(?,?)")
            .bind(source_id(9).encode())
            .bind(&launch)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,?)")
            .bind(source_id(2).encode())
            .bind(source_id(1).encode())
            .bind("source-native-existing")
            .bind(source_id(9).encode())
            .execute(pool)
            .await
            .unwrap();
        let key = SigningKey::from_slice(&[71; 32]).unwrap();
        let id = pair_browser(node, &key, "Source session actor").await;
        for _ in 0..2 {
            let observed = node
                .management
                .as_ref()
                .unwrap()
                .observe_next_document_socket_for_test()
                .unwrap();
            let peer = valid_management_socket(&self.area, node.listen_addr()).await;
            self.peers
                .push(authenticate_socket(peer, &key, &id, node_public_key(node)).await);
            let endpoint = observed.await.unwrap();
            self.clients.push(endpoint.client());
            self.endpoints.push(endpoint);
        }
        let child = crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);
        let child = child.to_string_lossy();
        assert!(std::path::Path::new(child.as_ref()).is_file());
        let raw = format!(
            r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,
            source_utf16(&child),
            source_utf16(scenario)
        );
        let profile =
            crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes())
                .unwrap();
        let draft = CatalogDraft::install_harness(
            "Owned source session child".into(),
            SourceSelection::Existing(source_id(1)),
            profile,
        )
        .unwrap();
        let catalog = &node.session_services.as_ref().unwrap().catalog;
        let actor = self.clients[0].context.actor();
        let registered = catalog
            .register(actor, draft, &source_deadline())
            .await
            .unwrap();
        let result = catalog
            .apply(actor, registered, &source_deadline())
            .await
            .unwrap();
        let (harness, profile) = match result.fact().result().unwrap() {
            CatalogResult::Harness {
                harness, profile, ..
            } => (*harness, *profile),
            _ => panic!("expected installed profile"),
        };
        self.selection = Some(SourceLaunchSelection {
            source: source_id(1),
            harness,
            profile,
            workspace: source_id(9),
            launch_path: launch.clone(),
        });
        let observation = catalog
            .observe_workspace(
                actor,
                source_id(2),
                source_id(1),
                source_id(9),
                &source_deadline(),
            )
            .await
            .unwrap();
        let registered = catalog
            .register(
                actor,
                observation.bind(harness, profile, launch).unwrap(),
                &source_deadline(),
            )
            .await
            .unwrap();
        catalog
            .apply(actor, registered, &source_deadline())
            .await
            .unwrap();
    }
    fn service(&self) -> crate::node::source_sessions::NodeSourceSessionManagement {
        self.node
            .as_ref()
            .unwrap()
            .source_session_management()
            .unwrap()
    }
    async fn register(&self, action: SourceSessionAction) -> SourceSessionOperationId {
        self.service()
            .register(
                &self.clients[0],
                self.selection.as_ref().unwrap().clone(),
                action,
            )
            .await
            .unwrap()
            .id
    }
    async fn close(&mut self) -> bool {
        for mut peer in self.peers.drain(..) {
            let _ = peer.close(None).await;
        }
        self.endpoints.clear();
        let closed = if let Some(node) = self.node.take() {
            node.shutdown(None).await.is_ok()
        } else {
            true
        };
        let joined = if let Some(owner) = &self.owner {
            owner.join_actual().await.is_ok()
        } else {
            true
        };
        closed && joined
    }
}
async fn source_case(
    scenario: &str,
    run: impl for<'a> FnOnce(&'a mut SourceCase) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
) {
    let mut fixture = SourceCase {
        area: TestArea::new(),
        owner: None,
        node: None,
        peers: Vec::new(),
        clients: Vec::new(),
        endpoints: Vec::new(),
        selection: None,
    };
    let result = AssertUnwindSafe(async {
        fixture.start(scenario).await;
        run(&mut fixture).await;
    })
    .catch_unwind()
    .await;
    // Release only this fixture's child gate before the original Node and Job cleanup.
    let _ = std::fs::write(
        fixture.area.directory.join("source-release-create"),
        b"owned fixture close",
    );
    let closed = fixture.close().await;
    if result.is_err() || !closed {
        let retained = fixture.area.directory.clone();
        std::mem::forget(fixture.area);
        eprintln!("SOURCE_FIXTURE_PRESERVED {:?}", retained);
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(
        closed,
        "ordinary Source/Doc/Queue/SQL shutdown and original process owner must join"
    );
}
async fn source_wait_file(f: &SourceCase, name: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !f.area.directory.join(name).is_file() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
async fn source_drain(f: &mut SourceCase, transfer: &DocumentTransfer) -> Vec<u8> {
    f.endpoints[0].attach(transfer).unwrap();
    let mut raw = Vec::new();
    f.peers[0]
        .send(Message::Binary(
            TransferHeader::credit(transfer.descriptor().stream_id, 0)
                .encode(&[])
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    loop {
        let state = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let value = transfer.prepare_next(&f.clients[0]).await.unwrap();
                if value != DocumentPrepare::AwaitCredit {
                    break value;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if state == DocumentPrepare::Complete {
            return raw;
        }
        assert_eq!(
            f.endpoints[0].send(transfer).unwrap(),
            DocumentEnqueue::Queued
        );
        let bytes = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match f.peers[0].next().await.unwrap().unwrap() {
                    Message::Binary(bytes) => break bytes,
                    Message::Ping(_) | Message::Pong(_) => {}
                    other => panic!("unexpected frame kind {:?}", std::mem::discriminant(&other)),
                }
            }
        })
        .await
        .unwrap();
        let (header, payload) = TransferHeader::decode(&bytes).unwrap();
        if header.record_kind() == Some(RecordKind::OriginalRaw) {
            raw.extend_from_slice(payload);
        } else {
            assert_eq!(header.record_kind(), Some(RecordKind::SnapshotEnd));
            assert!(payload.is_empty());
        }
        f.peers[0]
            .send(Message::Binary(
                TransferHeader::credit(header.stream(), header.packet())
                    .encode(&[])
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_discover_create_repeat_doc_and_original_target_load() {
    source_case("r2-v01-discover", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            assert!(
                !f.area.directory.join("source-child-pid").exists(),
                "registration is metadata only"
            );
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);
            assert_eq!(
                fact.response.as_ref().unwrap().ack,
                SourceAckState::Confirmed
            );
            assert!(
                !f.area.directory.join("source-load-session").exists(),
                "source discovery must not mint or load a target"
            );
            let transfer = service.open_result(&f.clients[0], id).await.unwrap();
            let descriptor = serde_json::to_value(transfer.descriptor()).unwrap();
            assert_eq!(descriptor["streamKind"], "native_session_result");
            assert!(descriptor["targetId"].is_null() && descriptor["revision"].is_null());
            let raw = source_drain(f, &transfer).await;
            let text = std::str::from_utf8(&raw).unwrap();
            assert!(
                text.contains("1234567890123456789012345678901234567890")
                    && text.contains("1.2300e+09")
                    && text.contains("\"b\":1.2300e+09,\"a\":null")
            );
            assert!(
                service.open_result(&f.clients[0], id).await.is_err(),
                "final Credit retires the last raw result"
            );
            let same = service.execute(&f.clients[1], id).await.unwrap();
            assert_eq!(same.identity(), receipt.identity());
            assert_eq!(
                same.wait().await.unwrap().outcome,
                SourceSessionOutcome::Succeeded
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-list-count")).unwrap(),
                "1"
            );
            let requests =
                std::fs::read_to_string(f.area.directory.join("source-list-request.jsonl"))
                    .unwrap();
            let first_request: serde_json::Value =
                serde_json::from_str(requests.lines().next().unwrap()).unwrap();
            assert!(first_request["cwd"].is_string());
            assert!(first_request.get("cursor").is_none());
            let next_page = f
                .register(SourceSessionAction::Discover {
                    cursor: Some("opaque-next-cursor".into()),
                })
                .await;
            let next_page = service.execute(&f.clients[0], next_page).await.unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(15), next_page.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .outcome,
                SourceSessionOutcome::Succeeded
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-list-count")).unwrap(),
                "2",
                "opaque cursor is forwarded only for an explicit second operation"
            );
            let requests =
                std::fs::read_to_string(f.area.directory.join("source-list-request.jsonl"))
                    .unwrap();
            let second_request: serde_json::Value =
                serde_json::from_str(requests.lines().nth(1).unwrap()).unwrap();
            assert_eq!(second_request["cursor"], "opaque-next-cursor");
            let created = f.register(SourceSessionAction::Create).await;
            let receipt = service.execute(&f.clients[0], created).await.unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);
            assert_eq!(
                fact.response.as_ref().unwrap().native_session_id.as_deref(),
                Some("source-native-created")
            );
            let same = service.execute(&f.clients[1], created).await.unwrap();
            assert_eq!(same.identity(), receipt.identity());
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
            service
                .release_result(&f.clients[0], created)
                .await
                .unwrap();
            source_submit(f).await;
            f.node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .ensure_target(source_id(2))
                .await
                .unwrap();
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-load-session")).unwrap(),
                "source-native-existing"
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-list-count")).unwrap(),
                "2"
            );
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_remote_error_retains_safe_fact_and_original_error_fragment() {
    source_case("normal", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f
                .register(SourceSessionAction::Discover {
                    cursor: Some("remote-error".into()),
                })
                .await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Refused);
            assert_eq!(
                fact.response.as_ref().unwrap().remote_error_code,
                Some(-32602)
            );
            assert!(!format!("{fact:?}").contains("SOURCE_REPLY_CANARY"));
            let transfer = service.open_result(&f.clients[0], id).await.unwrap();
            let raw = source_drain(f, &transfer).await;
            assert!(
                std::str::from_utf8(&raw)
                    .unwrap()
                    .contains("SOURCE_REPLY_CANARY")
            );
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_written_unknown_is_not_retried_or_replaced() {
    source_case("exit-create", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f.register(SourceSessionAction::Create).await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(
                fact.transport,
                Some(crate::domain::native_dispatch::TransportFact::Written)
            );
            assert!(fact.response.is_none());
            let queried = service.query(&f.clients[1], id).await.unwrap();
            assert_eq!(queried.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(
                queried.transport,
                Some(crate::domain::native_dispatch::TransportFact::Written)
            );
            assert!(queried.response.is_none());
            assert!(!queried.result_available);
            let same = service.execute(&f.clients[1], id).await.unwrap();
            assert_eq!(same.identity(), receipt.identity());
            assert_eq!(
                same.wait().await.unwrap().outcome,
                SourceSessionOutcome::Unknown
            );
            let reconciled = service.reconcile(&f.clients[1], id).await.unwrap();
            assert_eq!(reconciled.outcome, SourceSessionOutcome::Unknown);
            assert!(reconciled.response.is_none());
            assert!(service.open_result(&f.clients[1], id).await.is_err());
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
            assert!(!f.area.directory.join("source-list-count").exists());
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_socket_seal_keeps_original_worker_and_late_reply() {
    source_case("delayed-create", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f.register(SourceSessionAction::Create).await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            source_wait_file(f, "source-create-waiting").await;
            f.peers[0].close(None).await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while f.clients[0].context.lifetime.with_live(|| Ok(())).is_ok() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            std::fs::write(
                f.area.directory.join("source-release-create"),
                b"original late reply",
            )
            .unwrap();
            let fact = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fact.outcome, SourceSessionOutcome::Succeeded);
            assert_eq!(
                service
                    .query(&f.clients[1], id)
                    .await
                    .unwrap()
                    .response
                    .as_ref()
                    .unwrap()
                    .native_session_id
                    .as_deref(),
                Some("source-native-created")
            );
            assert!(
                service.open_result(&f.clients[1], id).await.is_err(),
                "sealed original socket cannot leave an unclaimed strong raw offer"
            );
            assert_eq!(
                std::fs::read_to_string(f.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_source_close_seals_original_result_epoch_without_credit() {
    source_case("normal", |f| {
        Box::pin(async move {
            let service = f.service();
            let id = f
                .register(SourceSessionAction::Discover { cursor: None })
                .await;
            let receipt = service.execute(&f.clients[0], id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap();
            let transfer = service.open_result(&f.clients[0], id).await.unwrap();
            f.endpoints[0].attach(&transfer).unwrap();
            assert_eq!(
                transfer.prepare_next(&f.clients[0]).await.unwrap(),
                DocumentPrepare::AwaitCredit
            );
            let native = f
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .clone();
            let report = native
                .shutdown(tokio::time::Instant::now() + Duration::from_secs(15))
                .await;
            assert!(
                report.joined && report.metadata_confirmed,
                "original source close: {report:?}"
            );
            tokio::time::timeout(Duration::from_secs(5), transfer.wait_closed())
                .await
                .unwrap();
            assert!(transfer.prepare_next(&f.clients[0]).await.is_err());
            assert!(service.open_result(&f.clients[1], id).await.is_err());
            assert_eq!(
                receipt.wait().await.unwrap().outcome,
                SourceSessionOutcome::Succeeded
            );
        })
    })
    .await;
}
async fn source_submit(f: &SourceCase) {
    use crate::{
        management_ws::input_upload::InputData, node::input_upload::UploadPurpose,
        node::queue_management::NodeQueueMutation,
    };
    let node = f.node.as_ref().unwrap();
    let client = &f.clients[0];
    let owner = node.input_upload_owner(client).unwrap();
    let upload = owner
        .begin(client, source_id(2), UploadPurpose::Submit)
        .await
        .unwrap()
        .upload();
    let raw = b"[{\"type\":\"text\",\"text\":\"owned source load regression\"}]";
    let mut frame = vec![0; 32];
    frame[..4].copy_from_slice(b"ACPI");
    frame[4] = 1;
    frame[5] = 1;
    frame[8..24].copy_from_slice(upload.as_bytes());
    frame.extend_from_slice(raw);
    owner
        .accept_data(client, InputData::decode_client(&frame).unwrap())
        .await
        .unwrap();
    owner
        .finish(client, upload, raw.len() as u64)
        .await
        .unwrap();
    let body = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(body) = owner.status(client, upload).await.unwrap().body() {
                break body;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let service = node.queue_management().unwrap();
    let registered = service
        .register(client, source_id(2), NodeQueueMutation::Submit { body })
        .await
        .unwrap();
    service
        .begin_consume(client, source_id(2), registered.id(), Some(body))
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_real_created_claim_conflict_across_profiles_keeps_both_responses() {
    source_case("normal",|f|Box::pin(async move{
    use crate::domain::native_catalog::ProfilePublication;
    let service=f.service();let first=f.register(SourceSessionAction::Create).await;let receipt=service.execute(&f.clients[0],first).await.unwrap();
    let fact=tokio::time::timeout(Duration::from_secs(15),receipt.wait()).await.unwrap().unwrap();assert_eq!(fact.outcome,SourceSessionOutcome::Succeeded);
    let selection=f.selection.as_ref().unwrap();let child=crate::test_support::native_dispatch_dev_child(&["SOURCE_SESSION_CHILD"]);let child=child.to_string_lossy();
    let raw=format!(r#"{{"format":1,"launch":{{"executable_utf16le_base64":"{}","arguments_utf16le_base64":["{}"],"environment":[]}},"recipes":[{{"wire":1,"mcp_servers":[],"additional_directories":[],"extra":{{}}}}]}}"#,source_utf16(&child),source_utf16("normal"));
    let profile=crate::adapters::acp::node_configuration_profile::parse_profile(raw.into_bytes()).unwrap();
    let draft=CatalogDraft::publish_profile(selection.harness,selection.source,ProfilePublication::New,profile).unwrap();
    let catalog=&f.node.as_ref().unwrap().session_services.as_ref().unwrap().catalog;let actor=f.clients[0].context.actor();
    let id=catalog.register(actor,draft,&source_deadline()).await.unwrap();let published=catalog.apply(actor,id,&source_deadline()).await.unwrap();
    let profile=match published.fact().result().unwrap(){CatalogResult::Profile{profile,..}=>*profile,_=>panic!("expected new profile")};
    let mut second_selection=selection.clone();second_selection.profile=profile;
    let second=service.register(&f.clients[0],second_selection,SourceSessionAction::Create).await.unwrap().id;
    let receipt2=service.execute(&f.clients[0],second).await.unwrap();let fact2=tokio::time::timeout(Duration::from_secs(15),receipt2.wait()).await.unwrap().unwrap();
    assert_eq!(fact2.outcome,SourceSessionOutcome::Unknown);assert_eq!(fact2.failure,Some(SourceFailureClass::Ownership));
    assert_ne!(fact.source_instance,fact2.source_instance);assert_eq!(fact2.response.as_ref().unwrap().native_session_id.as_deref(),Some("source-native-created"));
    assert_eq!(fact2.response.as_ref().unwrap().ack,SourceAckState::Confirmed);
    let pool=f.node.as_ref().unwrap().storage.as_ref().unwrap().pool_for_test();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM node_source_session_operations WHERE response_kind='created' AND native_session_id='source-native-created'").fetch_one(pool).await.unwrap();assert_eq!(count,2,"nonunique query index must preserve both actual created facts");
    service.release_result(&f.clients[0],first).await.unwrap();service.release_result(&f.clients[0],second).await.unwrap();
})).await;
}

// Source SQL failures deliberately preserve the Node root and sticky global report.
struct SourceSqlFaultCase {
    case: SourceCase,
    phases: Vec<crate::test_support::SharedAsyncBarrier>,
    closes: Vec<
        Arc<crate::session_storage::request_registry::connection_close_test_hooks::CloseBarrier>,
    >,
}
async fn source_sql_fault_case(
    run: impl for<'a> FnOnce(&'a mut SourceSqlFaultCase) -> Pin<Box<dyn Future<Output = ()> + 'a>>,
) {
    let mut f = SourceSqlFaultCase {
        case: SourceCase {
            area: TestArea::new(),
            owner: None,
            node: None,
            peers: Vec::new(),
            clients: Vec::new(),
            endpoints: Vec::new(),
            selection: None,
        },
        phases: Vec::new(),
        closes: Vec::new(),
    };
    let result = AssertUnwindSafe(async {
        f.case.start("normal").await;
        run(&mut f).await;
    })
    .catch_unwind()
    .await;
    for phase in &f.phases {
        phase.release();
    }
    for close in &f.closes {
        close.release();
    }
    let _ = std::fs::write(
        f.case.area.directory.join("source-release-create"),
        b"owned close",
    );
    for mut peer in f.case.peers.drain(..) {
        let _ = tokio::time::timeout(Duration::from_secs(2), peer.close(None)).await;
    }
    f.case.endpoints.clear();
    let shutdown = if let Some(node) = f.case.node.take() {
        matches!(
            tokio::time::timeout(Duration::from_secs(2), node.shutdown(None)).await,
            Ok(Ok(()))
        )
    } else {
        true
    };
    let joined = if let Some(owner) = &f.case.owner {
        matches!(
            tokio::time::timeout(Duration::from_secs(2), owner.join_actual()).await,
            Ok(Ok(()))
        )
    } else {
        true
    };
    let closed = shutdown && joined;
    let retained = f.case.area.directory.clone();
    std::mem::forget(f.case.area);
    eprintln!(
        "SOURCE_SQL_FAULT_ROOT_PRESERVED {:?}; normalNodeClose={closed}",
        retained
    );
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(
        !closed,
        "global original SQL failure must remain sticky after actual cleanup"
    );
}
fn source_sql_role_index(
    service: &crate::node::source_sessions::NodeSourceSessionManagement,
    id: SourceSessionOperationId,
    role: SourceSqlRole,
) -> usize {
    service
        .original_sql_for_test(id)
        .unwrap()
        .iter()
        .position(|(slot, _)| slot.is_some_and(|slot| slot.role == role))
        .expect("actual original SQL role")
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_sql_report_loss_query_is_read_only_and_reconcile_keeps_original_error() {
    use crate::domain::native_catalog::{CleanupKnowledge, TransactionKnowledge};
    source_sql_fault_case(|f| {
        Box::pin(async move {
            let service = f.case.service();
            let id = f.case.register(SourceSessionAction::Create).await;
            let phase = service
                .pause_next_sql_for_test(SourceSqlRole::Response)
                .unwrap();
            f.phases.push(phase.clone());
            let receipt = service.execute(&f.case.clients[0], id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), phase.wait_until_entered())
                .await
                .unwrap();
            let registry = f
                .case
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .request_registry();
            registry
                .connection_closer_for_test()
                .fail_next_result_after_ack()
                .unwrap();
            phase.release();
            let error = tokio::time::timeout(Duration::from_secs(15), receipt.wait())
                .await
                .unwrap()
                .unwrap_err();
            let index = source_sql_role_index(&service, id, SourceSqlRole::Response);
            let before = service.original_sql_for_test(id).unwrap()[index].0.unwrap();
            assert_eq!(before.transaction, TransactionKnowledge::CommitConfirmed);
            assert_eq!(before.cleanup, CleanupKnowledge::UnconfirmedOriginal);
            let physical = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!(physical.actual_ack, 1);
            let queried = service.query(&f.case.clients[1], id).await.unwrap();
            assert_eq!(queried.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(
                service.original_sql_for_test(id).unwrap()[index].0,
                Some(before),
                "query cannot confirm an old close receipt"
            );
            assert_eq!(receipt.try_result(), Some(Err(error.clone())));
            let reconciled = service.reconcile(&f.case.clients[1], id).await.unwrap();
            assert_eq!(reconciled.outcome, SourceSessionOutcome::Unknown);
            assert_eq!(
                service.original_sql_for_test(id).unwrap()[index]
                    .0
                    .unwrap()
                    .cleanup,
                CleanupKnowledge::ConfirmedOriginal
            );
            let actual = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!(
                (physical.owner, physical.close_slot),
                (actual.owner, actual.close_slot)
            );
            assert_eq!(actual.close_task_join, 1);
            assert_eq!(actual.worker_joined, Some(true));
            assert_eq!(actual.app_join, Some(true));
            assert_eq!(receipt.try_result(), Some(Err(error)));
            assert!(
                service
                    .execute(&f.case.clients[1], id)
                    .await
                    .unwrap()
                    .wait()
                    .await
                    .is_err()
            );
            assert_eq!(
                std::fs::read_to_string(f.case.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_sql_reconcile_failure_is_retained_until_next_explicit_original_cleanup() {
    use crate::domain::native_catalog::CleanupKnowledge;
    source_sql_fault_case(|f| {
        Box::pin(async move {
            let service = f.case.service();
            let id = f.case.register(SourceSessionAction::Create).await;
            let receipt = service.execute(&f.case.clients[0], id).await.unwrap();
            let original = receipt.wait().await.unwrap();
            let phase = service
                .pause_next_sql_for_test(SourceSqlRole::Reconcile)
                .unwrap();
            f.phases.push(phase.clone());
            let command_service = service.clone();
            let client = f.case.endpoints[0].client();
            let command = tokio::spawn(async move { command_service.reconcile(&client, id).await });
            tokio::time::timeout(Duration::from_secs(10), phase.wait_until_entered())
                .await
                .unwrap();
            let registry = f
                .case
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .request_registry();
            registry
                .connection_closer_for_test()
                .fail_next_result_after_ack()
                .unwrap();
            phase.release();
            assert!(
                tokio::time::timeout(Duration::from_secs(10), command)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_err()
            );
            let index = source_sql_role_index(&service, id, SourceSqlRole::Reconcile);
            let old = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!(
                service.original_sql_for_test(id).unwrap()[index]
                    .0
                    .unwrap()
                    .cleanup,
                CleanupKnowledge::UnconfirmedOriginal
            );
            assert_eq!(
                service.query(&f.case.clients[1], id).await.unwrap().outcome,
                original.outcome
            );
            assert_eq!(
                service.original_sql_for_test(id).unwrap()[index]
                    .0
                    .unwrap()
                    .cleanup,
                CleanupKnowledge::UnconfirmedOriginal
            );
            assert_eq!(
                service
                    .reconcile(&f.case.clients[1], id)
                    .await
                    .unwrap()
                    .outcome,
                original.outcome
            );
            let now = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!((old.owner, old.close_slot), (now.owner, now.close_slot));
            assert_eq!(now.worker_joined, Some(true));
            assert_eq!(
                service.original_sql_for_test(id).unwrap()[index]
                    .0
                    .unwrap()
                    .cleanup,
                CleanupKnowledge::ConfirmedOriginal
            );
            assert_eq!(receipt.wait().await.unwrap(), original);
            assert_eq!(
                std::fs::read_to_string(f.case.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
        })
    })
    .await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_session_sql_pending_original_close_blocks_source_claim_release_after_producers_join()
 {
    use crate::domain::native_catalog::CleanupKnowledge;
    use crate::session_storage::request_registry::connection_close_test_hooks::{
        CloseBarrier, ClosePoint,
    };
    source_sql_fault_case(|f| {
        Box::pin(async move {
            let service = f.case.service();
            let id = f.case.register(SourceSessionAction::Create).await;
            let phase = service
                .pause_next_sql_for_test(SourceSqlRole::Response)
                .unwrap();
            f.phases.push(phase.clone());
            let receipt = service.execute(&f.case.clients[0], id).await.unwrap();
            tokio::time::timeout(Duration::from_secs(10), phase.wait_until_entered())
                .await
                .unwrap();
            let registry = f
                .case
                .node
                .as_ref()
                .unwrap()
                .storage
                .as_ref()
                .unwrap()
                .request_registry();
            let closer = registry.connection_closer_for_test();
            let close = CloseBarrier::new();
            f.closes.push(close.clone());
            closer
                .pause_next(ClosePoint::AfterCloseAck, close.clone())
                .unwrap();
            closer.fail_next_result_after_ack().unwrap();
            phase.release();
            tokio::time::timeout(Duration::from_secs(10), close.arrived())
                .await
                .unwrap();
            let index = source_sql_role_index(&service, id, SourceSqlRole::Response);
            let physical = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!((physical.actual_ack, physical.close_task_join), (1, 0));
            let error = tokio::time::timeout(Duration::from_secs(40), receipt.wait())
                .await
                .unwrap()
                .unwrap_err();
            let native = f
                .case
                .node
                .as_ref()
                .unwrap()
                .session_services
                .as_ref()
                .unwrap()
                .native
                .clone();
            native.core.cancel();
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if service
                        .source_close_for_test(id)
                        .unwrap()
                        .is_some_and(|(physical, _, _)| physical)
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let pending = service.source_close_for_test(id).unwrap().unwrap();
            assert_ne!(
                pending.1,
                Some(Ok(())),
                "outer driver/IO joined cannot substitute original SQL cleanup"
            );
            assert_eq!(
                pending.2, 1,
                "original Created claim remains until the SQL receipt actually joins"
            );
            assert_ne!(
                service.original_sql_for_test(id).unwrap()[index]
                    .0
                    .unwrap()
                    .cleanup,
                CleanupKnowledge::ConfirmedOriginal
            );
            close.release();
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let status = service.source_close_for_test(id).unwrap().unwrap();
                    if status.1 == Some(Ok(())) && status.2 == 0 {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let actual = service.original_sql_cleanup_for_test(id).unwrap()[index]
                .clone()
                .unwrap();
            assert_eq!(
                (physical.owner, physical.close_slot),
                (actual.owner, actual.close_slot)
            );
            assert_eq!(actual.close_task_join, 1);
            assert_eq!(actual.worker_joined, Some(true));
            assert_eq!(
                service
                    .reconcile(&f.case.clients[1], id)
                    .await
                    .unwrap()
                    .outcome,
                SourceSessionOutcome::Unknown
            );
            assert_eq!(
                service.source_close_for_test(id).unwrap().unwrap().2,
                0,
                "post-close pure reads cannot change the frozen proof or revive a claim"
            );
            assert_eq!(receipt.try_result(), Some(Err(error)));
            assert_eq!(
                std::fs::read_to_string(f.case.area.directory.join("source-create-count")).unwrap(),
                "1"
            );
        })
    })
    .await;
}

include!("r2_tests.rs");
