use super::*;
use crate::{
    application::native_dispatch::{
        CapacityRepository, CapacityReservation, PreSourceProbe,
        capacity_source_development_fixture, session_parent_development_fixture,
    },
    domain::{
        EntityId,
        native::RuntimeEpoch,
        native_dispatch::{NativeOwnerKind, SourceMetadata},
    },
    session_storage::schema,
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn id(number: u16) -> EntityId {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&number.to_be_bytes());
    EntityId::from_bytes(bytes)
}

fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(15))
}

fn native(value: &str) -> NativeSessionKey {
    NativeSessionKey::from_config(Some(id(2)), value).expect("fixture native")
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
            "session-tree-{}-{nonce}-{}.sqlite3",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let storage = Storage::open(&path).await.expect("owned database");
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "tree-development".into(),
                public_key: vec![4; 65],
                key_file_ref: "fixture.key".into(),
            })
            .await
            .expect("node identity");
        for statement in [
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
        ] {
            sqlx::query(&statement)
                .execute(storage.pool_for_test())
                .await
                .expect("managed source fixture");
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

    fn proof(&self, parent: &str, child: &str) -> SessionParentProof {
        session_parent_development_fixture(self.source.clone(), native(parent), native(child), 1)
    }

    async fn prepare_created(&self, operation: u16, session: &str) {
        sqlx::query("INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,1,?,?,?,X'0000000000000001',?,'fixture',zeroblob(32),0)")
            .bind(id(operation).encode()).bind(id(1).encode()).bind(id(2).encode()).bind(id(3).encode()).bind(id(6).encode()).bind(id(4).encode())
            .execute(self.storage.pool_for_test()).await.expect("registered standard new");
        sqlx::query("INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,updated_at_ms) VALUES(?,'create',?,1,0,'owned','pending',?,?,0)")
            .bind(id(operation).encode()).bind(id(8).encode()).bind(id(7).encode()).bind(id(9).encode())
            .execute(self.storage.pool_for_test()).await.expect("original new owner");
        self.repository()
            .reserve_capacity(
                &CapacityReservation {
                    id: id(operation),
                    source: self.source.clone(),
                    native: None,
                    create_operation: Some(id(operation)),
                },
                &deadline(),
            )
            .await
            .expect("new reservation");
        sqlx::query("UPDATE node_source_session_operations SET protocol_version=2,rpc_id_kind='string',rpc_id_bytes=?,frame_length=X'0000000000000001',accepted_length=X'0000000000000001',transport_state='written',transport_recorded_at_ms=1,response_kind='created',native_session_id=?,response_sequence=X'0000000000000001',response_receipt=X'0000000000000001',response_evidence=zeroblob(32),result_bytes=X'0000000000000001',ack_state='confirmed',operation_state='terminal',outcome='succeeded',terminal_at_ms=2 WHERE operation_id=?")
            .bind(id(operation).encode().into_bytes()).bind(session).bind(id(operation).encode())
            .execute(self.storage.pool_for_test()).await.expect("necessary original standard-new response fixture");
    }

    async fn root(&self, operation: u16, session: &str) {
        self.prepare_created(operation, session).await;
        self.repository()
            .bind_created_capacity(id(operation), &native(session), &self.source, &deadline())
            .await
            .expect("verified standard-new root");
    }

    async fn close(self) {
        self.storage.close().await;
        for suffix in ["", "-wal", "-shm"] {
            let file = PathBuf::from(format!("{}{suffix}", self.path.display()));
            if file.exists() {
                std::fs::remove_file(file).expect("own fixture cleanup");
            }
        }
    }
}

#[tokio::test]
async fn session_tree_partial_nested_relations_require_verified_new_root() {
    let fixture = Fixture::new().await;
    let repository = fixture.repository();
    let result = repository
        .record_session_parent(&fixture.proof("middle", "child"), &deadline())
        .await
        .expect("partial relationship saved");
    assert_eq!(result.root, None);
    assert_eq!(result.ancestors, vec![native("middle")]);
    for subject in ["child", "middle", "random-no-parent"] {
        assert_eq!(
            repository
                .resolve_session_tree(&native(subject), &deadline())
                .await,
            Err(DispatchError::SessionTreeUnverified)
        );
    }
    fixture.root(100, "root").await;
    repository
        .record_session_parent(&fixture.proof("root", "middle"), &deadline())
        .await
        .expect("complete chain");
    let result = repository
        .resolve_session_tree(&native("child"), &deadline())
        .await
        .expect("rooted chain");
    assert_eq!(result.subject, native("child"));
    assert_eq!(result.root, Some(native("root")));
    assert_eq!(result.ancestors, vec![native("middle"), native("root")]);
    let root = repository
        .resolve_session_tree(&native("root"), &deadline())
        .await
        .expect("root itself");
    assert!(root.ancestors.is_empty());
    assert_eq!(root.root, Some(native("root")));
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .expect("capacity")
            .occupied,
        1
    );
    assert!(
        repository
            .capacity_owner(&native("child"), &deadline())
            .await
            .expect("no child reservation")
            .is_none()
    );
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_duplicate_keeps_original_evidence_and_conflicting_parent_rejects() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    let repository = fixture.repository();
    let first = repository
        .record_session_parent(&fixture.proof("root", "child"), &deadline())
        .await
        .expect("first");
    let repeated = session_parent_development_fixture(
        fixture.source.clone(),
        native("root"),
        native("child"),
        42,
    );
    assert_eq!(
        repository
            .record_session_parent(&repeated, &deadline())
            .await
            .expect("same parent"),
        first
    );
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("different", "child"), &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    let proof: (Vec<u8>, Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT sequence,receipt,fingerprint FROM node_session_parents")
            .fetch_one(fixture.storage.pool_for_test())
            .await
            .expect("original evidence");
    assert_eq!(
        proof,
        (
            1_u64.to_be_bytes().to_vec(),
            1_u64.to_be_bytes().to_vec(),
            vec![3; 32]
        )
    );
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_rejects_self_long_cycle_cross_source_and_both_root_conflicts() {
    let fixture = Fixture::new().await;
    let repository = fixture.repository();
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("same", "same"), &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    for (parent, child) in [("top", "middle"), ("middle", "bottom")] {
        repository
            .record_session_parent(&fixture.proof(parent, child), &deadline())
            .await
            .expect("partial edge");
    }
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("bottom", "top"), &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    let foreign = NativeSessionKey::from_config(Some(id(200)), "foreign").expect("other source");
    for proof in [
        session_parent_development_fixture(
            fixture.source.clone(),
            foreign.clone(),
            native("child"),
            1,
        ),
        session_parent_development_fixture(fixture.source.clone(), native("parent"), foreign, 1),
    ] {
        assert_eq!(
            repository.record_session_parent(&proof, &deadline()).await,
            Err(DispatchError::Conflict)
        );
    }
    fixture.root(100, "root").await;
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("outside", "root"), &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    fixture.prepare_created(101, "bottom").await;
    assert_eq!(
        repository
            .bind_created_capacity(id(101), &native("bottom"), &fixture.source, &deadline())
            .await,
        Err(DispatchError::Conflict)
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM node_session_capacity WHERE native_id='bottom' AND root_verified=1",
    )
    .fetch_one(fixture.storage.pool_for_test())
    .await
    .expect("no child promotion");
    assert_eq!(count, 0);
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_original_source_identity_is_checked_before_idempotence() {
    let fixture = Fixture::new().await;
    let repository = fixture.repository();
    repository
        .record_session_parent(&fixture.proof("parent", "child"), &deadline())
        .await
        .expect("original");
    let original = fixture.source.metadata();
    for field in [
        "source",
        "instance",
        "runtime",
        "connection",
        "config",
        "revision",
    ] {
        let mut metadata = original.clone();
        match field {
            "source" => metadata.source_id = id(500),
            "instance" => metadata.instance_id = id(500),
            "runtime" => metadata.runtime = RuntimeEpoch(id(500)),
            "connection" => metadata.connection_epoch = id(500),
            "config" => metadata.config_identity = id(500),
            "revision" => metadata.config_revision = 500,
            _ => unreachable!(),
        }
        let proof = session_parent_development_fixture(
            capacity_source_development_fixture(metadata),
            native("parent"),
            native("child"),
            2,
        );
        assert_eq!(
            repository.record_session_parent(&proof, &deadline()).await,
            Err(DispatchError::Conflict),
            "{field}"
        );
    }
    let zero = session_parent_development_fixture(
        fixture.source.clone(),
        native("parent"),
        native("child"),
        0,
    );
    assert_eq!(
        repository.record_session_parent(&zero, &deadline()).await,
        Err(DispatchError::Conflict)
    );
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_concurrent_same_parent_idempotent_and_opposite_edges_serialize() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    let repository = fixture.repository();
    let proof = fixture.proof("root", "child");
    let concurrent_deadline = deadline();
    let (first, second) = tokio::join!(
        repository.record_session_parent(&proof, &concurrent_deadline),
        repository.record_session_parent(&proof, &concurrent_deadline),
    );
    assert_eq!(first.expect("first"), second.expect("second"));
    let forward = fixture.proof("left", "right");
    let backward = fixture.proof("right", "left");
    let (first, second) = tokio::join!(
        repository.record_session_parent(&forward, &concurrent_deadline),
        repository.record_session_parent(&backward, &concurrent_deadline),
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(first == Err(DispatchError::Conflict) || second == Err(DispatchError::Conflict));
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .expect("one root")
            .occupied,
        1
    );
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_reopen_preserves_identity_without_requiring_current_source() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    fixture
        .repository()
        .record_session_parent(&fixture.proof("root", "child"), &deadline())
        .await
        .expect("original relationship");
    fixture
        .repository()
        .record_session_parent(&fixture.proof("child", "grandchild"), &deadline())
        .await
        .expect("nested relationship");
    fixture.storage.close().await;
    let reopened = Storage::open(&fixture.path)
        .await
        .expect("reopen existing database");
    let repository = reopened.request_registry().native_dispatch_repository();
    assert_eq!(
        repository
            .resolve_session_tree(&native("child"), &deadline())
            .await
            .expect("historical identity")
            .root,
        Some(native("root"))
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_session_parents")
        .fetch_one(reopened.pool_for_test())
        .await
        .expect("relationship");
    assert_eq!(count, 2);
    let members = repository
        .session_tree_members(&native("root"), &deadline())
        .await
        .expect("all historical identities");
    assert_eq!(
        members
            .iter()
            .map(|path| path.subject.clone())
            .collect::<Vec<_>>(),
        vec![native("child"), native("grandchild"), native("root")]
    );
    assert_eq!(members[1].ancestors, vec![native("child"), native("root")]);
    for unverified in ["child", "unknown"] {
        assert_eq!(
            repository
                .session_tree_members(&native(unverified), &deadline())
                .await,
            Err(DispatchError::SessionTreeUnverified)
        );
    }
    reopened.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_insert_and_commit_failures_retain_no_partial_relationship() {
    let fixture = Fixture::new().await;
    let repository = fixture.repository();
    sqlx::query("CREATE TRIGGER tree_insert_fail BEFORE INSERT ON node_session_parents BEGIN SELECT RAISE(ABORT,'owned failure'); END")
        .execute(fixture.storage.pool_for_test()).await.expect("owned insert fault");
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("parent", "child"), &deadline())
            .await,
        Err(DispatchError::StorageUnavailable)
    );
    sqlx::query("DROP TRIGGER tree_insert_fail")
        .execute(fixture.storage.pool_for_test())
        .await
        .expect("remove own fault");
    sqlx::raw_sql("CREATE TABLE tree_test_parent(id INTEGER PRIMARY KEY); CREATE TABLE tree_test_child(parent INTEGER REFERENCES tree_test_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER tree_commit_fail AFTER INSERT ON node_session_parents BEGIN INSERT INTO tree_test_child VALUES(1); END;")
        .execute(fixture.storage.pool_for_test()).await.expect("real deferred FK commit fault");
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("parent", "child"), &deadline())
            .await,
        Err(DispatchError::CommitUnconfirmed)
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_session_parents")
        .fetch_one(fixture.storage.pool_for_test())
        .await
        .expect("rollback");
    assert_eq!(count, 0);
    sqlx::raw_sql(
        "DROP TRIGGER tree_commit_fail; DROP TABLE tree_test_child; DROP TABLE tree_test_parent;",
    )
    .execute(fixture.storage.pool_for_test())
    .await
    .expect("remove own fault");
    repository
        .record_session_parent(&fixture.proof("parent", "child"), &deadline())
        .await
        .expect("same owner retry");
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_sql_close_unknown_keeps_committed_fact_for_retry() {
    let fixture = Fixture::new().await;
    let repository = fixture.repository();
    repository
        .registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .expect("owned close-report loss");
    assert_eq!(
        repository
            .record_session_parent(&fixture.proof("parent", "child"), &deadline())
            .await,
        Err(DispatchError::SqlCloseUnconfirmed {
            commit_confirmed: true
        })
    );
    repository
        .record_session_parent(&fixture.proof("parent", "child"), &deadline())
        .await
        .expect("retry original");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_session_parents")
        .fetch_one(fixture.storage.pool_for_test())
        .await
        .expect("one original");
    assert_eq!(count, 1);
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_schema_rejects_corrupt_identity_cycle_and_root_child_overlap() {
    for statement in [
        "UPDATE node_session_parents SET connection_epoch='AAAAAAAAAAAAAAAAAAAAAA'",
        "UPDATE node_session_parents SET parent_native_id='child' WHERE child_native_id='middle'",
        "UPDATE node_session_parents SET child_native_id='root' WHERE child_native_id='child'",
        "PRAGMA ignore_check_constraints=ON; UPDATE node_session_parents SET sequence=zeroblob(8); PRAGMA ignore_check_constraints=OFF;",
        "PRAGMA ignore_check_constraints=ON; UPDATE node_session_parents SET fingerprint=X'01'; PRAGMA ignore_check_constraints=OFF;",
    ] {
        let fixture = Fixture::new().await;
        fixture.root(100, "root").await;
        fixture
            .repository()
            .record_session_parent(&fixture.proof("root", "middle"), &deadline())
            .await
            .expect("middle");
        fixture
            .repository()
            .record_session_parent(&fixture.proof("middle", "child"), &deadline())
            .await
            .expect("child");
        sqlx::raw_sql(statement)
            .execute(fixture.storage.pool_for_test())
            .await
            .expect("owned corrupted fixture");
        assert!(
            schema::initialize(fixture.storage.pool_for_test())
                .await
                .is_err(),
            "{statement}"
        );
        fixture.close().await;
    }
}

#[tokio::test]
async fn session_tree_closed_source_only_coordinates_exact_original_evidence() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    let repository = fixture.repository();
    let proof = fixture.proof("root", "child");
    let original = repository
        .record_session_parent(&proof, &deadline())
        .await
        .expect("before closure");
    sqlx::query("UPDATE node_native_source_instances SET lifecycle='closed',close_basis='owned_converged',close_stage='started',close_io_evidence='started_joined',close_owner_evidence='test_owned_stream',close_evidence_fingerprint=zeroblob(32) WHERE instance_id=?")
        .bind(id(7).encode()).execute(fixture.storage.pool_for_test()).await.expect("source necessary closure fixture");
    assert_eq!(
        repository
            .record_session_parent(&proof, &deadline())
            .await
            .expect("original retry"),
        original
    );
    assert_eq!(
        repository
            .resolve_session_tree(&native("child"), &deadline())
            .await
            .expect("historical identity"),
        original
    );
    let changed = session_parent_development_fixture(
        fixture.source.clone(),
        native("root"),
        native("child"),
        2,
    );
    for proof in [
        changed,
        fixture.proof("root", "new-child"),
        fixture.proof("different", "child"),
    ] {
        assert_eq!(
            repository.record_session_parent(&proof, &deadline()).await,
            Err(DispatchError::Conflict)
        );
    }
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_same_source_other_configuration_reuses_persistent_identity() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    let repository = fixture.repository();
    repository
        .record_session_parent(&fixture.proof("root", "child"), &deadline())
        .await
        .expect("first observation");
    sqlx::query("INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?,?,1,'harness.install','registered',0,X'01',zeroblob(32))")
        .bind(id(205).encode()).bind(id(1).encode())
        .execute(fixture.storage.pool_for_test()).await.expect("second configuration command");
    sqlx::query("INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,X'0000000000000002',?,?,1,X'01',zeroblob(32),?,0)")
        .bind(id(206).encode()).bind(id(3).encode()).bind(id(2).encode()).bind(id(205).encode())
        .execute(fixture.storage.pool_for_test()).await.expect("second configuration");
    sqlx::query("INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,X'0000000000000002','test_owned_stream','reserved',0,0)")
        .bind(id(207).encode()).bind(id(2).encode()).bind(id(208).encode()).bind(id(209).encode()).bind(id(206).encode())
        .execute(fixture.storage.pool_for_test()).await.expect("second actual registered source");
    let source = capacity_source_development_fixture(SourceMetadata {
        source_id: id(2),
        instance_id: id(207),
        runtime: RuntimeEpoch(id(208)),
        connection_epoch: id(209),
        config_identity: id(206),
        config_revision: 2,
        owner_kind: NativeOwnerKind::TestOwnedStream,
        created_at_ms: 0,
    });
    let proof = session_parent_development_fixture(source, native("root"), native("child"), 2);
    assert_eq!(
        repository
            .record_session_parent(&proof, &deadline())
            .await
            .expect("same persistent relationship")
            .root,
        Some(native("root"))
    );
    let instance: String = sqlx::query_scalar("SELECT observed_instance FROM node_session_parents")
        .fetch_one(fixture.storage.pool_for_test())
        .await
        .expect("original evidence remains");
    assert_eq!(instance, id(7).encode());
    assert_eq!(
        repository
            .inspect_capacity(&deadline())
            .await
            .expect("no second slot")
            .occupied,
        1
    );
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_cancelled_request_does_not_install_relationship() {
    let fixture = Fixture::new().await;
    let cancelled = deadline();
    cancelled.cancel();
    assert!(
        fixture
            .repository()
            .record_session_parent(&fixture.proof("parent", "child"), &cancelled)
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_session_parents")
        .fetch_one(fixture.storage.pool_for_test())
        .await
        .expect("not installed");
    assert_eq!(count, 0);
    fixture.close().await;
}

#[tokio::test]
async fn session_tree_upgrade_preserves_existing_capacity_history() {
    let fixture = Fixture::new().await;
    fixture.root(100, "root").await;
    fixture.prepare_created(101, "unbound").await;
    let statement = "SELECT reservation_id,source_id,native_id,root_verified,physically_closed,released FROM node_session_capacity ORDER BY reservation_id";
    let before: Vec<(String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(statement)
        .fetch_all(fixture.storage.pool_for_test())
        .await
        .expect("capacity history");
    crate::session_storage::session_tree_schema::remove_owned_fixture_for_test(
        fixture.storage.pool_for_test(),
    )
    .await;
    sqlx::raw_sql("UPDATE node_meta SET schema_version=13; PRAGMA user_version=13;")
        .execute(fixture.storage.pool_for_test())
        .await
        .expect("owned populated predecessor fixture");
    schema::install_legacy_session_for_test(fixture.storage.pool_for_test())
        .await
        .expect("legacy session tables for v13");
    schema::initialize(fixture.storage.pool_for_test())
        .await
        .expect("additive populated migration");
    let after: Vec<(String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(statement)
        .fetch_all(fixture.storage.pool_for_test())
        .await
        .expect("preserved capacity history");
    assert_eq!(before, after);
    assert_eq!(
        fixture
            .repository()
            .session_tree_members(&native("root"), &deadline())
            .await
            .expect("only proven root")
            .len(),
        1
    );
    assert_eq!(
        fixture
            .repository()
            .inspect_capacity(&deadline())
            .await
            .expect("no returned slots")
            .occupied,
        2
    );
    fixture.close().await;
}

#[path = "tree_sql_acceptance.rs"]
mod independent_acceptance;
