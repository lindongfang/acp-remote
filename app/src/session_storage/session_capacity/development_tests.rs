//! Developer fixtures execute real SQLite/registry transactions, not a real Harness.
use super::*;
use crate::{
    application::native_dispatch::capacity_source_development_fixture,
    domain::{
        ActorContext,
        native_dispatch::{NativeOwnerKind, SourceMetadata},
    },
    session_storage::{native_release::NativeReleaseStoredPhase, schema},
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use std::{path::PathBuf, sync::Arc, time::Duration};

#[path = "automatic_release_development_tests.rs"]
mod automatic_release;

fn id(n: u16) -> EntityId {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&n.to_be_bytes());
    EntityId::from_bytes(bytes)
}
fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(10))
}
struct Fixture {
    storage: Storage,
    path: PathBuf,
    source: Arc<PreSourceProbe>,
}
impl Fixture {
    async fn new() -> Self {
        static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("fixture time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "capacity-development-{}-{nonce}-{}.sqlite3",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let storage = Storage::open(&path).await.expect("owned SQLite");
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "capacity-development".into(),
                public_key: vec![4; 65],
                key_file_ref: "fixture.key".into(),
            })
            .await
            .expect("identity");
        let statements = [
            format!(
                "INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES('{}',X'01','fixture','trusted',1,0)",
                id(1).encode()
            ),
            format!(
                "INSERT INTO product_sources VALUES('{}','fixture source')",
                id(2).encode()
            ),
            format!(
                "INSERT INTO harnesses VALUES('{}','fixture harness','fixture','[]',NULL)",
                id(3).encode()
            ),
            format!(
                "INSERT INTO workspaces VALUES('{}','fixture')",
                id(4).encode()
            ),
            format!(
                "INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES('{}','{}',1,'harness.install','registered',0,X'01',zeroblob(32))",
                id(5).encode(),
                id(1).encode()
            ),
            format!(
                "INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES('{}',X'0000000000000001','{}','{}',1,X'01',zeroblob(32),'{}',0)",
                id(6).encode(),
                id(3).encode(),
                id(2).encode(),
                id(5).encode()
            ),
            format!(
                "INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) VALUES('{}','{}','{}','{}','{}',X'0000000000000001','test_owned_stream','reserved',0,0)",
                id(7).encode(),
                id(2).encode(),
                id(8).encode(),
                id(9).encode(),
                id(6).encode()
            ),
        ];
        for statement in statements {
            sqlx::query(&statement)
                .execute(storage.pool_for_test())
                .await
                .expect("development parents");
        }
        let source = capacity_source_development_fixture(SourceMetadata {
            source_id: id(2),
            instance_id: id(7),
            runtime: RuntimeEpoch(id(8)),
            connection_epoch: id(9),
            config_identity: id(6),
            config_revision: 1,
            owner_kind: NativeOwnerKind::TestOwnedStream,
            created_at_ms: 0,
        });
        Self {
            storage,
            path,
            source,
        }
    }
    fn repository(&self) -> SqliteNativeDispatch {
        self.storage.request_registry().native_dispatch_repository()
    }
    async fn operation(&self, n: u16) -> CapacityReservation {
        let operation = id(n);
        sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
            .bind(operation.encode()).bind(id(1).encode()).bind(id(2).encode()).bind(id(3).encode()).bind(id(6).encode()).bind(id(4).encode()).execute(self.storage.pool_for_test()).await.expect("create registration");
        sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
            .bind(operation.encode()).bind(id(8).encode()).bind(id(7).encode()).bind(id(9).encode()).execute(self.storage.pool_for_test()).await.expect("create operation");
        CapacityReservation {
            id: operation,
            source: self.source.clone(),
            native: None,
            create_operation: Some(operation),
        }
    }
    async fn created(&self, n: u16, native: &str, terminal: bool) {
        sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=?,frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id=?,response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed' WHERE operation_id=?")
            .bind(id(n).encode().into_bytes()).bind(native).bind(id(n).encode()).execute(self.storage.pool_for_test()).await.expect("original Created ACK fixture");
        if terminal {
            sqlx::query("UPDATE node_source_session_operations SET operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?").bind(id(n).encode()).execute(self.storage.pool_for_test()).await.expect("necessary Finish");
        }
    }
    async fn close(self) {
        self.storage.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let file = PathBuf::from(format!("{}{suffix}", self.path.display()));
            if file.exists() {
                std::fs::remove_file(file).expect("owned development file cleanup");
            }
        }
    }
}

#[tokio::test]
async fn fifty_reservations_survive_reopen_and_last_slot_is_atomic() {
    let f = Fixture::new().await;
    let repo = f.repository();
    for n in 100..149 {
        let p = f.operation(n).await;
        repo.reserve_capacity(&p, &deadline())
            .await
            .expect("first 49");
    }
    let a = f.operation(149).await;
    let b = f.operation(150).await;
    let d1 = deadline();
    let d2 = deadline();
    let (left, right) = tokio::join!(
        repo.reserve_capacity(&a, &d1),
        repo.reserve_capacity(&b, &d2)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(
        matches!(left, Err(DispatchError::SessionCapacityFull))
            || matches!(right, Err(DispatchError::SessionCapacityFull))
    );
    let state = repo.inspect_capacity(&deadline()).await.expect("capacity");
    assert_eq!(
        state,
        CapacitySnapshot {
            limit: 50,
            occupied: 50,
            reserved: 50,
            closed_pending: 0
        }
    );
    drop(repo);
    f.storage.close().await;
    let reopened = Storage::open(&f.path)
        .await
        .expect("reopen exact necessary ledger");
    let state = reopened
        .request_registry()
        .native_dispatch_repository()
        .inspect_capacity(&deadline())
        .await
        .expect("restored");
    assert_eq!(state.occupied, 50);
    reopened.close().await;
    f.close().await;
}

#[tokio::test]
async fn created_conflict_retains_second_reservation_and_root_is_not_inferred() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let unverified = NativeSessionKey::from_config(Some(id(2)), "external").expect("identity");
    let p = CapacityReservation {
        id: id(90),
        source: f.source.clone(),
        native: Some(unverified),
        create_operation: None,
    };
    assert_eq!(
        repo.reserve_capacity(&p, &deadline()).await,
        Err(DispatchError::SessionTreeUnverified)
    );
    let first = f.operation(100).await;
    let second = f.operation(101).await;
    repo.reserve_capacity(&first, &deadline())
        .await
        .expect("first reservation");
    repo.reserve_capacity(&second, &deadline())
        .await
        .expect("second reservation");
    f.created(100, "same-native", true).await;
    f.created(101, "same-native", true).await;
    let native = NativeSessionKey::from_config(Some(id(2)), "same-native").expect("identity");
    let owner = repo
        .bind_created_capacity(id(100), &native, &f.source, &deadline())
        .await
        .expect("original root");
    assert_eq!(
        repo.bind_created_capacity(id(101), &native, &f.source, &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    assert_eq!(
        repo.reserve_capacity(&first, &deadline())
            .await
            .expect("original retry"),
        owner
    );
    let reuse = CapacityReservation {
        id: id(102),
        source: f.source.clone(),
        native: Some(native.clone()),
        create_operation: None,
    };
    assert_eq!(
        repo.reserve_capacity(&reuse, &deadline())
            .await
            .expect("same owner reuse"),
        owner
    );
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("ledger")
            .occupied,
        2
    );
    assert_eq!(
        repo.abandon_capacity(id(101), &f.source, &deadline()).await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    f.close().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "The original Create, close, outer finish fault and retry assertions form one atomicity scenario."
)]
async fn closed_pending_needs_original_finish_and_outer_atomic_result_before_refund() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let p = f.operation(100).await;
    repo.reserve_capacity(&p, &deadline())
        .await
        .expect("reserved");
    f.created(100, "created", false).await;
    let native = NativeSessionKey::from_config(Some(id(2)), "created").expect("identity");
    let record = repo
        .bind_created_capacity(id(100), &native, &f.source, &deadline())
        .await
        .expect("bound before Finish");
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("runtime physical close");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("closed pending")
            .closed_pending,
        1
    );
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind("created")
        .execute(f.storage.pool_for_test())
        .await
        .expect("target");
    let releases = f.storage.native_release_operations();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("registered");
    releases
        .begin(actor, id(22), 2, &deadline())
        .await
        .expect("begin");
    assert!(
        releases
            .finish(actor, id(22), None, 3, &deadline())
            .await
            .is_err()
    );
    assert_eq!(
        releases
            .inspect(actor, id(22), &deadline())
            .await
            .expect("atomic rollback")
            .phase,
        NativeReleaseStoredPhase::Releasing
    );
    f.created(100, "created", true).await;
    sqlx::query("CREATE TRIGGER fail_capacity_release BEFORE UPDATE OF released ON node_session_capacity WHEN NEW.released=1 BEGIN SELECT RAISE(ABORT,'development failure'); END").execute(f.storage.pool_for_test()).await.expect("necessary failure fixture");
    assert!(
        releases
            .finish(actor, id(22), None, 4, &deadline())
            .await
            .is_err()
    );
    assert_eq!(
        releases
            .inspect(actor, id(22), &deadline())
            .await
            .expect("outer rolled back")
            .phase,
        NativeReleaseStoredPhase::Releasing
    );
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("still occupied")
            .occupied,
        1
    );
    sqlx::query("DROP TRIGGER fail_capacity_release")
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore store");
    releases
        .finish(actor, id(22), None, 5, &deadline())
        .await
        .expect("atomic outer result and refund");
    releases
        .finish(actor, id(22), None, 6, &deadline())
        .await
        .expect("Released retry coordinates once");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("refunded once")
            .occupied,
        0
    );
    assert_eq!(
        repo.capacity_owner(&native, &deadline())
            .await
            .expect("released is not live"),
        None
    );
    f.close().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keeps both capacity generations and the original failing release replay in one observable sequence."
)]
async fn old_released_retry_cannot_refund_resumed_capacity_generation() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let reservation = f.operation(100).await;
    repo.reserve_capacity(&reservation, &deadline())
        .await
        .expect("original capacity");
    f.created(100, "generation-native", true).await;
    let native = NativeSessionKey::from_config(Some(id(2)), "generation-native").expect("identity");
    let original = repo
        .bind_created_capacity(id(100), &native, &f.source, &deadline())
        .await
        .expect("original root");
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind(native.native_id())
        .execute(f.storage.pool_for_test())
        .await
        .expect("target");
    let releases = f.storage.native_release_operations();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("register original release");
    releases
        .begin(actor, id(22), 2, &deadline())
        .await
        .expect("begin original release");
    repo.confirm_capacity_closed(&original, &deadline())
        .await
        .expect("original runtime closed");
    releases
        .finish(actor, id(22), None, 3, &deadline())
        .await
        .expect("original release and refund");
    let resumed = repo
        .reserve_capacity(
            &CapacityReservation {
                id: id(101),
                source: f.source.clone(),
                native: Some(native.clone()),
                create_operation: None,
            },
            &deadline(),
        )
        .await
        .expect("explicit resume creates a distinct capacity responsibility");
    assert_ne!(original.id, resumed.id);
    releases
        .register(actor, id(20), id(23), id(24), 4, &deadline())
        .await
        .expect("register resumed release");
    releases
        .begin(actor, id(24), 5, &deadline())
        .await
        .expect("begin resumed release");
    releases
        .begin(actor, id(22), 5, &deadline())
        .await
        .expect("old begin retry cannot capture resumed capacity");
    repo.confirm_capacity_closed(&resumed, &deadline())
        .await
        .expect("resumed native physically closed");
    let trigger = format!(
        "CREATE TRIGGER fail_generation_finish BEFORE UPDATE OF phase ON node_native_release_operations WHEN NEW.phase='released' AND OLD.operation_id='{}' BEGIN SELECT RAISE(ABORT,'generation finish failure'); END",
        id(24).encode()
    );
    sqlx::query(&trigger)
        .execute(f.storage.pool_for_test())
        .await
        .expect("necessary outer finish failure fixture");
    assert!(
        releases
            .finish(actor, id(24), None, 6, &deadline())
            .await
            .is_err()
    );
    sqlx::query("DROP TRIGGER fail_generation_finish")
        .execute(f.storage.pool_for_test())
        .await
        .expect("restore store");
    releases
        .finish(actor, id(22), None, 7, &deadline())
        .await
        .expect("old Released retry remains idempotent");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("resumed generation remains occupied")
            .occupied,
        1,
        "old release proof must not refund a newer capacity generation"
    );
    assert_eq!(
        releases
            .inspect(actor, id(24), &deadline())
            .await
            .expect("resumed result is still unconfirmed")
            .phase,
        NativeReleaseStoredPhase::Releasing
    );
    releases
        .finish(actor, id(24), None, 8, &deadline())
        .await
        .expect("only resumed release proof refunds resumed capacity");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("resumed refund")
            .occupied,
        0
    );
    f.close().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "The legacy operation, three new proofs and restart assertions share one durable responsibility."
)]
async fn legacy_release_without_generation_proof_keeps_capacity_until_new_explicit_release() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let p = f.operation(100).await;
    repo.reserve_capacity(&p, &deadline())
        .await
        .expect("capacity");
    f.created(100, "legacy-release-native", true).await;
    let native =
        NativeSessionKey::from_config(Some(id(2)), "legacy-release-native").expect("identity");
    let record = repo
        .bind_created_capacity(p.id, &native, &f.source, &deadline())
        .await
        .expect("root");
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind(native.native_id())
        .execute(f.storage.pool_for_test())
        .await
        .expect("target");
    let releases = f.storage.native_release_operations();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("legacy operation");
    // A schema11 in-flight operation has no begin-time capacity association.
    sqlx::query("UPDATE node_native_release_operations SET phase='releasing' WHERE operation_id=?")
        .bind(id(22).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("legacy Releasing fixture");
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("original native closed");
    releases
        .finish(actor, id(22), None, 2, &deadline())
        .await
        .expect("legacy result preserves compatibility without a refund");
    releases
        .finish(actor, id(22), None, 3, &deadline())
        .await
        .expect("legacy Released repeat does not adopt current capacity");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("conservative legacy responsibility")
            .occupied,
        1
    );
    for (request, operation) in [(23, 24), (25, 26), (27, 28)] {
        releases
            .register(actor, id(20), id(request), id(operation), 4, &deadline())
            .await
            .expect("new explicit release");
        releases
            .begin(actor, id(operation), 5, &deadline())
            .await
            .expect("multiple release operations may bind the same original responsibility");
    }
    let associations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_session_capacity_releases")
            .fetch_one(f.storage.pool_for_test())
            .await
            .expect("fixed associations");
    assert_eq!(associations, 3);
    releases
        .finish(actor, id(24), Some("release_failed"), 6, &deadline())
        .await
        .expect("failed release never refunds");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("failure keeps capacity")
            .occupied,
        1
    );
    drop(repo);
    f.storage.close().await;
    let reopened = Storage::open(&f.path)
        .await
        .expect("association survives restart and exact schema validation");
    let releases = reopened.native_release_operations();
    releases
        .finish(actor, id(26), None, 7, &deadline())
        .await
        .expect("new proof reconciles its original responsibility");
    releases
        .finish(actor, id(28), None, 8, &deadline())
        .await
        .expect("second associated operation coordinates once");
    assert_eq!(
        reopened
            .request_registry()
            .native_dispatch_repository()
            .inspect_capacity(&deadline())
            .await
            .expect("one refund")
            .occupied,
        0
    );
    reopened.close().await;
    f.close().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Preserves the empty-file historical DDL setup, original resources and first migration assertions together."
)]
async fn exact_historical_schema11_preserves_fifty_one_resources_and_rejects_new_capacity() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("fixture clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "capacity-exact-schema11-{}-{nonce}.sqlite3",
        std::process::id()
    ));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .expect("empty owned legacy database");
    // This pinned Git-baseline DDL creates schema11 directly from an empty file.
    // No schema12 object or current migration is used to manufacture the old layout.
    sqlx::raw_sql(include_str!("schema11_fixture.sql"))
        .execute(&pool)
        .await
        .expect("exact historical schema0..11 DDL");
    let parents = format!(
        "INSERT INTO node_meta VALUES(1,11,'capacity-legacy',X'01','legacy.key');
         INSERT INTO node_state VALUES(1,0);
         INSERT INTO product_control VALUES(1,'{epoch}',0);
         INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES('{actor}',X'01','fixture','trusted',1,0);
         INSERT INTO product_sources VALUES('{source}','legacy source');
         INSERT INTO harnesses VALUES('{harness}','legacy harness','fixture','[]',NULL);
         INSERT INTO workspaces VALUES('{workspace}','fixture');
         INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES('{request}','{actor}',1,'harness.install','registered',0,X'01',zeroblob(32));
         INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES('{profile}',X'0000000000000001','{harness}','{source}',1,X'01',zeroblob(32),'{request}',0);
         INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) VALUES('{instance}','{source}','{runtime}','{connection}','{profile}',X'0000000000000001','test_owned_stream','reserved',0,0);
         PRAGMA user_version=11;",
        epoch = id(10).encode(),
        actor = id(1).encode(),
        source = id(2).encode(),
        harness = id(3).encode(),
        workspace = id(4).encode(),
        request = id(5).encode(),
        profile = id(6).encode(),
        instance = id(7).encode(),
        runtime = id(8).encode(),
        connection = id(9).encode(),
    );
    sqlx::raw_sql(&parents)
        .execute(&pool)
        .await
        .expect("original schema11 identity and source");
    for n in 100..151 {
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(id(n).encode())
            .bind(id(2).encode())
            .bind(format!("legacy-native-{n}"))
            .execute(&pool)
            .await
            .expect("original target");
        sqlx::query("INSERT INTO node_native_resource_bindings(load_epoch,target_id,owner_id,instance_id,runtime_epoch,connection_epoch,config_identity,config_revision,lifecycle,reserved_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?,X'0000000000000001','unknown',0,0)")
            .bind(id(n + 1000).encode()).bind(id(n).encode()).bind(id(10).encode())
            .bind(id(7).encode()).bind(id(8).encode()).bind(id(9).encode()).bind(id(6).encode())
            .execute(&pool).await.expect("original unreleased resource responsibility");
    }
    for (request, operation, phase) in [(300, 301, "releasing"), (302, 303, "released")] {
        sqlx::query("INSERT INTO node_native_release_operations(operation_id,request_id,actor_id,actor_auth_generation,target_id,phase,registered_at_ms,updated_at_ms) VALUES(?,?,?,1,?,?,0,0)")
            .bind(id(operation).encode()).bind(id(request).encode()).bind(id(1).encode())
            .bind(id(100).encode()).bind(phase).execute(&pool).await.expect("original legacy operation without generation proof");
    }
    let original: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.load_epoch,b.instance_id,b.runtime_epoch,b.lifecycle,t.native_id FROM node_native_resource_bindings b JOIN product_targets t ON t.target_id=b.target_id ORDER BY b.load_epoch",
    ).fetch_all(&pool).await.expect("pin all original responsibilities");
    assert_eq!(original.len(), 51);
    let old_layout: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT name,sql FROM sqlite_schema ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("pin exact historical DDL");
    assert!(
        !old_layout
            .iter()
            .any(|(name, _)| name.starts_with("node_session_capacity"))
    );
    pool.close().await;
    let storage = Storage::open(&path)
        .await
        .expect("first actual current migration of historical schema11");
    let after: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.load_epoch,b.instance_id,b.runtime_epoch,b.lifecycle,t.native_id FROM node_native_resource_bindings b JOIN product_targets t ON t.target_id=b.target_id ORDER BY b.load_epoch",
    ).fetch_all(storage.pool_for_test()).await.expect("preserved original resources");
    assert_eq!(after, original);
    let retired_remaining: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema")
        .fetch_all(storage.pool_for_test())
        .await
        .expect("schema objects");
    assert!(
        !retired_remaining
            .iter()
            .any(|name| crate::session_storage::schema::is_retired_legacy_object(name)),
        "retired legacy objects must be absent at v15"
    );
    let expected_old_layout: Vec<(String, Option<String>)> = old_layout
        .into_iter()
        .filter(|(name, _)| !crate::session_storage::schema::is_retired_legacy_object(name))
        .collect();
    let retained_layout: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name,sql FROM sqlite_schema WHERE name NOT LIKE 'node_session_capacity%' AND name NOT LIKE 'sqlite_autoindex_node_session_capacity%' AND name NOT LIKE 'node_session_automatic_%' AND name NOT LIKE 'sqlite_autoindex_node_session_automatic_%' AND name NOT IN ('node_session_parents','sqlite_autoindex_node_session_parents_1') ORDER BY name",
    ).fetch_all(storage.pool_for_test()).await.expect("old DDL unchanged");
    assert_eq!(retained_layout, expected_old_layout);
    let recovery: (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),SUM(root_verified),(SELECT COUNT(*) FROM node_session_capacity_releases) FROM node_session_capacity",
    ).fetch_one(storage.pool_for_test()).await.expect("recovered conservative capacities");
    assert_eq!(recovery, (51, 0, 0));
    let f = Fixture {
        storage,
        path,
        source: capacity_source_development_fixture(SourceMetadata {
            source_id: id(2),
            instance_id: id(7),
            runtime: RuntimeEpoch(id(8)),
            connection_epoch: id(9),
            config_identity: id(6),
            config_revision: 1,
            owner_kind: NativeOwnerKind::TestOwnedStream,
            created_at_ms: 0,
        }),
    };
    assert_eq!(
        f.repository()
            .inspect_capacity(&deadline())
            .await
            .expect("over-limit state preserved")
            .occupied,
        51
    );
    let new = f.operation(200).await;
    assert_eq!(
        f.repository().reserve_capacity(&new, &deadline()).await,
        Err(DispatchError::SessionCapacityFull)
    );
    f.storage.close().await;
    let reopened = Storage::open(&f.path)
        .await
        .expect("reopen recovered ledger");
    assert_eq!(
        reopened
            .request_registry()
            .native_dispatch_repository()
            .inspect_capacity(&deadline())
            .await
            .expect("recovered occupancy persists")
            .occupied,
        51
    );
    reopened.close().await;
    f.close().await;
}

#[tokio::test]
async fn only_zero_create_abandons_and_migration_recovers_more_than_fifty_without_roots() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let p = f.operation(100).await;
    repo.reserve_capacity(&p, &deadline())
        .await
        .expect("reserved zero");
    assert_eq!(
        repo.abandon_capacity(p.id, &f.source, &deadline()).await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    sqlx::query("UPDATE node_source_session_operations SET operation_state='terminal',outcome='refused',failure_class='metadata',terminal_at_ms=1 WHERE operation_id=?")
        .bind(p.id.encode()).execute(f.storage.pool_for_test()).await.expect("original operation durably refused before arm");
    repo.abandon_capacity(p.id, &f.source, &deadline())
        .await
        .expect("SQL proves no arm");
    repo.abandon_capacity(p.id, &f.source, &deadline())
        .await
        .expect("idempotent original owner");
    for n in 101..153 {
        f.operation(n).await;
        f.created(n, &format!("legacy-{n}"), true).await;
    }
    // Only this test's owned database is downgraded to reproduce an exact schema11 upgrade.
    crate::session_storage::automatic_capacity_release_schema::remove_owned_fixture_for_test(
        f.storage.pool_for_test(),
    )
    .await;
    sqlx::query("DROP TABLE node_session_capacity_releases")
        .execute(f.storage.pool_for_test())
        .await
        .expect("remove test schema12 associations");
    sqlx::query("DROP TABLE node_session_capacity")
        .execute(f.storage.pool_for_test())
        .await
        .expect("remove test schema12");
    sqlx::query("PRAGMA user_version=11")
        .execute(f.storage.pool_for_test())
        .await
        .expect("old pragma");
    sqlx::query("UPDATE node_meta SET schema_version=11")
        .execute(f.storage.pool_for_test())
        .await
        .expect("old metadata");
    schema::install_legacy_session_for_test(f.storage.pool_for_test())
        .await
        .expect("legacy session tables for v11");
    schema::initialize(f.storage.pool_for_test())
        .await
        .expect("recover exact old operations");
    let state = repo
        .inspect_capacity(&deadline())
        .await
        .expect("legacy responsibility");
    // Operation 100 has a durable terminal refusal before arming and is not a resource.
    assert_eq!(state.occupied, 52);
    let extra = f.operation(160).await;
    assert_eq!(
        repo.reserve_capacity(&extra, &deadline()).await,
        Err(DispatchError::SessionCapacityFull)
    );
    let native = NativeSessionKey::from_config(Some(id(2)), "legacy-101").expect("identity");
    let p = CapacityReservation {
        id: id(161),
        source: f.source.clone(),
        native: Some(native),
        create_operation: None,
    };
    assert_eq!(
        repo.reserve_capacity(&p, &deadline()).await,
        Err(DispatchError::SessionTreeUnverified)
    );
    f.close().await;
}

#[tokio::test]
async fn complete_original_source_closure_reconciles_unknown_reservation_and_missing_commit() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let p = f.operation(100).await;
    repo.reserve_capacity(&p, &deadline())
        .await
        .expect("original responsibility");
    f.created(100, "source-close", false).await;
    f.operation(101).await;
    sqlx::query("UPDATE node_source_session_operations SET source_instance_id=NULL,connection_epoch=NULL WHERE operation_id=?")
        .bind(id(101).encode()).execute(f.storage.pool_for_test()).await.expect("original operation not yet attached durably");
    assert_eq!(
        repo.abandon_capacity(id(200), &f.source, &deadline()).await,
        Err(DispatchError::Conflict)
    );
    sqlx::query("UPDATE node_native_source_instances SET lifecycle='closed',close_basis='owned_converged',close_stage='started',close_io_evidence='started_joined',close_owner_evidence='test_owned_stream',close_evidence_fingerprint=zeroblob(32) WHERE instance_id=?")
        .bind(id(7).encode()).execute(f.storage.pool_for_test()).await.expect("development fixture original source closure metadata");
    assert_eq!(
        repo.abandon_capacity(id(200), &f.source, &deadline()).await,
        Err(DispatchError::Conflict)
    );
    assert_eq!(
        repo.abandon_capacity(p.id, &f.source, &deadline()).await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    sqlx::query("UPDATE node_source_session_operations SET operation_state='interrupted',outcome='unknown',terminal_at_ms=3 WHERE operation_id=?")
        .bind(p.id.encode()).execute(f.storage.pool_for_test()).await.expect("necessary original operation interrupted");
    repo.abandon_capacity(p.id, &f.source, &deadline())
        .await
        .expect("whole source converged");
    assert_eq!(
        repo.abandon_capacity(id(101), &f.source, &deadline()).await,
        Err(DispatchError::Conflict)
    );
    sqlx::query("UPDATE node_source_session_operations SET operation_state='interrupted',outcome='unknown',terminal_at_ms=3 WHERE operation_id=?")
        .bind(id(101).encode()).execute(f.storage.pool_for_test()).await.expect("unattached original Create necessarily settled");
    repo.abandon_capacity(id(101), &f.source, &deadline())
        .await
        .expect("missing capacity row with settled original Create");
    repo.abandon_capacity(id(200), &f.source, &deadline())
        .await
        .expect("missing commit has no residual responsibility after full source convergence");
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("ledger")
            .occupied,
        0
    );
    f.close().await;
}

#[tokio::test]
async fn opaque_native_identity_keeps_nul_and_unicode_bytes() {
    let f = Fixture::new().await;
    let repo = f.repository();
    for (n, value) in [(100, "\0native"), (101, "会话-原生")] {
        let p = f.operation(n).await;
        repo.reserve_capacity(&p, &deadline())
            .await
            .expect("original reservation");
        f.created(n, value, true).await;
        let native =
            NativeSessionKey::from_config(Some(id(2)), value).expect("opaque protocol identity");
        let record = repo
            .bind_created_capacity(p.id, &native, &f.source, &deadline())
            .await
            .expect("bind exact bytes");
        assert_eq!(record.native.as_ref(), Some(&native));
        assert_eq!(
            repo.capacity_owner(&native, &deadline())
                .await
                .expect("exact lookup"),
            Some(record)
        );
    }
    assert_eq!(
        repo.inspect_capacity(&deadline())
            .await
            .expect("distinct identities")
            .occupied,
        2
    );
    f.close().await;
}

#[tokio::test]
async fn release_basis_requires_original_set_and_necessary_source_closure() {
    let f = Fixture::new().await;
    let repo = f.repository();
    let reservation = f.operation(100).await;
    repo.reserve_capacity(&reservation, &deadline())
        .await
        .expect("original reservation");
    f.created(100, "release-basis", true).await;
    let native = NativeSessionKey::from_config(Some(id(2)), "release-basis").expect("identity");
    let record = repo
        .bind_created_capacity(id(100), &native, &f.source, &deadline())
        .await
        .expect("original root");
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(20).encode())
        .bind(id(2).encode())
        .bind(native.native_id())
        .execute(f.storage.pool_for_test())
        .await
        .expect("target");
    let releases = f.storage.native_release_operations();
    let actor = ActorContext {
        client_id: id(1),
        auth_generation: 1,
    };
    releases
        .register(actor, id(20), id(21), id(22), 1, &deadline())
        .await
        .expect("legacy release");
    sqlx::query("UPDATE node_native_release_operations SET phase='releasing' WHERE operation_id=?")
        .bind(id(22).encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("legacy fixture has no generation association");
    assert_eq!(
        repo.capacity_release_basis(id(22), &native, &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    releases
        .register(actor, id(20), id(23), id(24), 2, &deadline())
        .await
        .expect("current explicit release");
    releases
        .begin(actor, id(24), 3, &deadline())
        .await
        .expect("freeze original generation");
    assert_eq!(
        repo.capacity_release_basis(id(24), &native, &deadline())
            .await,
        Ok(CapacityReleaseBasis::Owned(record.clone()))
    );
    let wrong_native = NativeSessionKey::from_config(Some(id(2)), "other").expect("other identity");
    assert_eq!(
        repo.capacity_release_basis(id(24), &wrong_native, &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed)
    );
    repo.confirm_capacity_closed(&record, &deadline())
        .await
        .expect("original close metadata");
    let mut closed = record.clone();
    closed.physically_closed = true;
    assert_eq!(
        repo.capacity_release_basis(id(24), &native, &deadline())
            .await,
        Ok(CapacityReleaseBasis::Owned(closed))
    );
    sqlx::query("UPDATE node_session_capacity SET released=1 WHERE reservation_id=?")
        .bind(record.id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("developer fixture isolates released from source closure");
    assert_eq!(
        repo.capacity_release_basis(id(24), &native, &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed),
        "released rows without necessary source closure do not authorize finish-only"
    );
    sqlx::query("UPDATE node_native_source_instances SET lifecycle='closed',close_basis='owned_converged',close_stage='started',close_io_evidence='started_joined',close_owner_evidence='test_owned_stream',close_evidence_fingerprint=zeroblob(32) WHERE instance_id=?")
        .bind(f.source.metadata().instance_id.encode())
        .execute(f.storage.pool_for_test())
        .await
        .expect("developer fixture original source closure");
    assert_eq!(
        repo.capacity_release_basis(id(24), &native, &deadline())
            .await,
        Ok(CapacityReleaseBasis::SourceClosed)
    );
    assert_eq!(
        repo.capacity_release_basis(id(22), &native, &deadline())
            .await,
        Err(DispatchError::ResourceUnconfirmed),
        "even a closed source cannot authorize an empty legacy set"
    );
    f.close().await;
}
