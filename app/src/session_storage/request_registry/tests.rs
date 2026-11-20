use super::*;
use crate::{
    domain::{ActorContext, native::NativeSessionKey},
    session_storage::{
        product_schema,
        schema::{self, MigrationFault},
    },
};
use sqlx::{Connection, sqlite::SqlitePoolOptions};
use std::{path::PathBuf, sync::Arc};
use tokio::time::{Duration, Instant};
fn id(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
struct AreaRoots(PathBuf);
struct Area {
    roots: Arc<AreaRoots>,
    scopes: std::sync::Mutex<Vec<Arc<crate::sqlite_fixture_scope::SqliteFixtureScope>>>,
}
impl std::ops::Deref for Area {
    type Target = AreaRoots;
    fn deref(&self) -> &AreaRoots {
        &self.roots
    }
}
impl Area {
    async fn open(
        &self,
    ) -> Result<
        crate::sqlite_fixture_scope::FixtureStorage,
        std::sync::Arc<crate::error::StorageError>,
    > {
        let scope = std::sync::Arc::new(crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &self.db(),
            self.roots.clone(),
        ));
        self.scopes
            .lock()
            .expect("fixture scopes")
            .push(scope.clone());
        scope.open().await
    }
    async fn raw_scope(&self) -> std::sync::Arc<crate::sqlite_fixture_scope::SqliteFixtureScope> {
        let scope = std::sync::Arc::new(crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &self.db(),
            self.roots.clone(),
        ));
        self.scopes
            .lock()
            .expect("fixture scopes")
            .push(scope.clone());
        scope.start_raw().await;
        scope
    }
    async fn observe(&self) {
        let scopes = std::mem::take(&mut *self.scopes.lock().expect("fixture scopes"));
        for scope in scopes {
            scope.stop_and_observe().await;
        }
    }

    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "acpr-reg-sql-dev-{}",
            crate::random::hex(12).unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        Self {
            roots: Arc::new(AreaRoots(root)),
            scopes: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn db(&self) -> PathBuf {
        self.0.join("node.sqlite3")
    }
}
impl Drop for AreaRoots {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("acpr-reg-sql-dev-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn connect(area: &Area) -> SqliteConnection {
    let scope = area.raw_scope().await;
    SqliteConnection::connect_with(&scope.options().create_if_missing(true).foreign_keys(true))
        .await
        .unwrap()
}
async fn schema4(area: &Area) {
    let mut c = connect(area).await;
    // Build the existing four layouts directly; never create schema5 and forge an old header.
    for definitions in [
        schema::CORE,
        schema::CATALOG_TABLES,
        schema::LEGACY_SESSION_DEFINITIONS,
        product_schema::DEFINITIONS,
        product_schema::DELIVERY_DEFINITIONS,
    ] {
        for ddl in definitions {
            sqlx::query(ddl).execute(&mut c).await.unwrap();
        }
    }
    sqlx::query(
        "INSERT INTO node_meta VALUES(1,4,'synthetic-original',X'00ff7b7d','synthetic.key')",
    )
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query("INSERT INTO node_state VALUES(1,19)")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO clients VALUES(?,X'01','original','trusted',7,1,NULL),(?,X'02','revoked','revoked',9,1,2)").bind(id(1).encode()).bind(id(2).encode()).execute(&mut c).await.unwrap();
    sqlx::query("INSERT INTO product_control VALUES(1,?,888)")
        .bind(id(3).encode())
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO harnesses VALUES('old-h','synthetic','not-executed.exe','[]',NULL)")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces VALUES('old-w','D:/synthetic-unused')")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions VALUES('old-s','old-h','old-w','interrupted',NULL,NULL,NULL)",
    )
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO content_objects VALUES('old-body','old-s',?,7,'output',X'00ff7b7d',?,4)",
    )
    .bind(id(1).encode())
    .bind("a".repeat(64))
    .execute(&mut c)
    .await
    .unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'original-source')")
        .bind(id(5).encode())
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(id(4).encode())
        .bind(id(5).encode())
        .bind("old-native")
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("PRAGMA user_version=4")
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
    area.observe().await;
}
async fn preserved(c: &mut SqliteConnection, version: i64) {
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        version
    );
    let meta: (i64, String, Vec<u8>, String) =
        sqlx::query_as("SELECT schema_version,node_id,public_key,key_file_ref FROM node_meta")
            .fetch_one(&mut *c)
            .await
            .unwrap();
    assert_eq!(
        meta,
        (
            version,
            "synthetic-original".into(),
            vec![0, 255, 123, 125],
            "synthetic.key".into()
        )
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM harnesses WHERE harness_id='old-h'")
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        "synthetic"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT canonical_path FROM workspaces WHERE workspace_id='old-w'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap(),
        "D:/synthetic-unused"
    );
    if version < 15 {
        assert_eq!(
            sqlx::query_scalar::<_, Vec<u8>>(
                "SELECT body FROM content_objects WHERE content_ref='old-body'"
            )
            .fetch_one(&mut *c)
            .await
            .unwrap(),
            [0, 255, 123, 125]
        );
    } else {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('sessions','content_objects')",
            )
            .fetch_one(&mut *c)
            .await
            .unwrap(),
            0
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT time_floor_ms FROM product_control")
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        888
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM clients WHERE client_id=?")
            .bind(id(2).encode())
            .fetch_one(&mut *c)
            .await
            .unwrap(),
        "revoked"
    );
}
#[tokio::test]
async fn registry_schema5_addition_is_atomic_at_every_new_migration_checkpoint() {
    for fault in [
        MigrationFault::RegistrationTable,
        MigrationFault::OperationTable,
        MigrationFault::RegistrationIndex,
        MigrationFault::RegistrationMetadata,
        MigrationFault::RegistrationPragma,
        MigrationFault::Validation,
        MigrationFault::Commit,
        MigrationFault::DeferredCommit,
    ] {
        let area = Area::new();
        schema4(&area).await;
        let raw_scope = area.raw_scope().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(raw_scope.options().foreign_keys(true))
            .await
            .unwrap();
        assert!(schema::initialize_for_test(&pool, fault).await.is_err());
        pool.close().await;
        area.observe().await;
        let mut c = connect(&area).await;
        preserved(&mut c, 4).await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name LIKE 'node_request_%'"
            )
            .fetch_one(&mut c)
            .await
            .unwrap(),
            0
        );
        c.close().await.unwrap();
        area.observe().await;
        let storage = area.open().await.unwrap();
        storage.close().await;
        area.observe().await;
        let mut c = connect(&area).await;
        preserved(&mut c, crate::storage::SCHEMA_VERSION).await;
        c.close().await.unwrap();
        area.observe().await;
    }
}
#[tokio::test]
async fn registry_full_u64_binding_and_wrong_outcome_never_alias() {
    let area = Area::new();
    schema4(&area).await;
    let storage = area.open().await.unwrap();
    let repository = storage.request_registry();
    let actor = CurrentRequestActor::new(
        ActorContext {
            client_id: id(1),
            auth_generation: 7,
        },
        id(4),
        NativeSessionKey::from_config(Some(id(5)), "old-native").unwrap(),
        Some(QueueAction::Edit),
    );
    let intent = RegisteredIntent::new(
        id(1),
        QueueAction::Edit,
        id(4),
        Some(id(9)),
        Some(u64::MAX),
        Some([42; 32]),
    )
    .unwrap();
    let request = RegistrationId::new(id(10));
    let at = OperationContext {
        runtime: RuntimeEpoch(id(11)),
        observed_at_ms: -10,
    };
    let deadline = RegistryDeadline::new(Instant::now() + Duration::from_secs(3));
    repository
        .register(&actor, &intent, request, &at, &deadline)
        .await
        .unwrap();
    let BeginRecord::Started(record) = repository
        .begin_consume(&actor, request, &intent, id(12), &at, &deadline)
        .await
        .unwrap()
    else {
        panic!("first consume must own its applying fact")
    };
    let probe = OperationProbe::new(request, id(12), &at, &actor, intent);
    let lease = ConsumeLease::mint(&probe, record).unwrap();
    assert_eq!(
        repository
            .settle(
                &lease,
                &QueueOutcome::Edited {
                    item: id(9),
                    revision: 0
                },
                &at,
                &deadline
            )
            .await,
        Err(RegistryError::InvalidIntent)
    );
    assert_eq!(
        match repository
            .inspect_operation(&probe, &deadline)
            .await
            .unwrap()
        {
            RequestFact::Applying(a) => a.registration.intent().expected_revision(),
            _ => None,
        },
        Some(u64::MAX)
    );
    repository
        .settle(
            &lease,
            &QueueOutcome::Rejected(QueueRejection::RevisionExhausted),
            &at,
            &deadline,
        )
        .await
        .unwrap();
    storage.close().await;
    area.observe().await;
}

async fn seed_sct_004_catalog_and_targets(
    pool: &sqlx::SqlitePool,
    source: EntityId,
    actor_id: EntityId,
    instance_id: EntityId,
    runtime_epoch: EntityId,
    conn_epoch: EntityId,
    profile_id: EntityId,
) {
    let catalog_req_id = id(44);
    let profile_rev = 1_u64.to_be_bytes().to_vec();
    sqlx::query(
        "INSERT INTO node_catalog_commands(request_id,actor_id,register_auth_generation,action,state,registered_at_ms,intent_cipher,intent_digest) VALUES(?,?,7,'harness.install','registered',0,X'01',zeroblob(32))",
    )
    .bind(catalog_req_id.encode())
    .bind(actor_id.encode())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_launch_profiles(profile_id,revision,harness_id,source_id,payload_format,protected_payload,payload_digest,created_by_request_id,created_at_ms) VALUES(?,?,'old-h',?,1,X'01',zeroblob(32),?,0)",
    )
    .bind(profile_id.encode())
    .bind(&profile_rev)
    .bind(source.encode())
    .bind(catalog_req_id.encode())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_native_source_instances(instance_id,source_id,runtime_epoch,connection_epoch,config_identity,config_revision,owner_kind,lifecycle,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,'test_owned_stream','reserved',0,0)",
    )
    .bind(instance_id.encode())
    .bind(source.encode())
    .bind(runtime_epoch.encode())
    .bind(conn_epoch.encode())
    .bind(profile_id.encode())
    .bind(&profile_rev)
    .execute(pool)
    .await
    .unwrap();

    for (tid, nid) in [
        (id(20), "unverified-top"),
        (id(21), "child-of-old"),
        (id(22), "child-of-unreg-root"),
        (id(23), "child-of-unverified"),
        (id(24), "standalone-top"),
        (id(25), "cycle-a"),
        (id(26), "verified-child"),
    ] {
        sqlx::query("INSERT INTO product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,NULL)")
            .bind(tid.encode())
            .bind(source.encode())
            .bind(nid)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn seed_sct_004_operations_and_capacity(
    pool: &sqlx::SqlitePool,
    source: EntityId,
    actor_id: EntityId,
    instance_id: EntityId,
    runtime_epoch: EntityId,
    conn_epoch: EntityId,
    profile_id: EntityId,
) {
    let profile_rev = 1_u64.to_be_bytes().to_vec();
    for (op_id, nid) in [
        (id(60), "old-native"),
        (id(61), "unreg-root"),
        (id(62), "verified-child"),
    ] {
        sqlx::query(
            "INSERT INTO node_source_session_registrations(operation_id,action,actor_id,register_auth_generation,source_id,harness_id,profile_id,profile_revision,workspace_id,launch_path,intent_digest,registered_at_ms) VALUES(?,'create',?,7,?,'old-h',?,?,'old-w','D:/synthetic-unused',zeroblob(32),0)",
        )
        .bind(op_id.encode())
        .bind(actor_id.encode())
        .bind(source.encode())
        .bind(profile_id.encode())
        .bind(&profile_rev)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_source_session_operations(operation_id,action,runtime_epoch,admission_auth_generation,admitted_at_ms,operation_state,outcome,source_instance_id,connection_epoch,protocol_version,rpc_id_kind,rpc_id_bytes,frame_length,accepted_length,transport_state,transport_recorded_at_ms,response_kind,native_session_id,response_sequence,response_receipt,response_evidence,result_bytes,ack_state,updated_at_ms,terminal_at_ms) VALUES(?,'create',?,7,0,'terminal','succeeded',?,?,2,'string',?,X'0000000000000001',X'0000000000000001','written',1,'created',?,X'0000000000000001',X'0000000000000001',zeroblob(32),X'0000000000000001','confirmed',2,2)",
        )
        .bind(op_id.encode())
        .bind(runtime_epoch.encode())
        .bind(instance_id.encode())
        .bind(conn_epoch.encode())
        .bind(op_id.encode().into_bytes())
        .bind(nid)
        .execute(pool)
        .await
        .unwrap();
    }

    for (res_id, nid, create_op, verified) in [
        (id(50), "old-native", Some(id(60)), 1_i64),
        (id(51), "unreg-root", Some(id(61)), 1_i64),
        (id(52), "unverified-top", None, 0_i64),
        (id(53), "verified-child", Some(id(62)), 1_i64),
    ] {
        sqlx::query("INSERT INTO node_session_capacity(reservation_id,source_id,instance_id,runtime_epoch,native_id,create_operation,root_verified,physically_closed,released) VALUES(?,?,?,?,?,?,?,0,0)")
            .bind(res_id.encode())
            .bind(source.encode())
            .bind(instance_id.encode())
            .bind(runtime_epoch.encode())
            .bind(nid)
            .bind(create_op.map(EntityId::encode))
            .bind(verified)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn seed_sct_004_tree(pool: &sqlx::SqlitePool, source: EntityId) {
    let actor_id = id(1);
    let instance_id = id(40);
    let runtime_epoch = id(41);
    let conn_epoch = id(42);
    let profile_id = id(43);

    seed_sct_004_catalog_and_targets(
        pool,
        source,
        actor_id,
        instance_id,
        runtime_epoch,
        conn_epoch,
        profile_id,
    )
    .await;
    seed_sct_004_operations_and_capacity(
        pool,
        source,
        actor_id,
        instance_id,
        runtime_epoch,
        conn_epoch,
        profile_id,
    )
    .await;
    seed_sct_004_parents(pool, source, instance_id, runtime_epoch, conn_epoch).await;
}

async fn seed_sct_004_parents(
    pool: &sqlx::SqlitePool,
    source: EntityId,
    instance_id: EntityId,
    runtime_epoch: EntityId,
    conn_epoch: EntityId,
) {
    for (idx, (child, parent)) in [
        ("child-of-old", "old-native"),
        ("child-of-unreg-root", "unreg-root"),
        ("child-of-unverified", "unverified-top"),
        ("cycle-a", "cycle-b"),
        ("cycle-b", "cycle-a"),
        ("verified-child", "old-native"),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query("INSERT INTO node_session_parents(source_id,child_native_id,parent_native_id,observed_instance,runtime_epoch,connection_epoch,sequence,receipt,fingerprint,observed_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(source.encode())
            .bind(child)
            .bind(parent)
            .bind(instance_id.encode())
            .bind(runtime_epoch.encode())
            .bind(conn_epoch.encode())
            .bind((u64::try_from(idx + 1).unwrap()).to_be_bytes().to_vec())
            .bind(1_u64.to_be_bytes().to_vec())
            .bind(vec![7_u8; 32])
            .bind(100_i64)
            .execute(pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sct_004_resolve_control_root_boundaries() {
    let area = Area::new();
    schema4(&area).await;
    {
        let c = connect(&area).await;
        let storage_tmp = area.open().await.unwrap();
        storage_tmp.close().await;
        c.close().await.unwrap();
    }
    area.observe().await;

    let storage = area.open().await.unwrap();
    let pool = storage.pool_for_test().clone();
    let source = id(5);
    seed_sct_004_tree(&pool, source).await;

    let repository = storage.request_registry();
    assert_eq!(
        repository.resolve_control_root(id(21)).await,
        Ok(ResolvedControlRoot {
            root: NativeSessionKey::from_config(Some(source), "old-native").unwrap(),
            root_target_id: Some(id(4)),
        })
    );
    assert_eq!(
        repository.resolve_control_root(id(22)).await,
        Ok(ResolvedControlRoot {
            root: NativeSessionKey::from_config(Some(source), "unreg-root").unwrap(),
            root_target_id: None,
        })
    );
    assert_eq!(
        repository.resolve_control_root(id(24)).await,
        Ok(ResolvedControlRoot {
            root: NativeSessionKey::from_config(Some(source), "standalone-top").unwrap(),
            root_target_id: Some(id(24)),
        })
    );
    assert_eq!(
        repository.resolve_control_root(id(20)).await,
        Err(RegistryError::SessionTreeUnverified)
    );
    assert_eq!(
        repository.resolve_control_root(id(23)).await,
        Err(RegistryError::SessionTreeUnverified)
    );
    assert_eq!(
        repository.resolve_control_root(id(99)).await,
        Err(RegistryError::InvalidTarget)
    );
    assert_eq!(
        repository.resolve_control_root(id(25)).await,
        Err(RegistryError::CorruptState)
    );
    assert_eq!(
        repository.resolve_control_root(id(26)).await,
        Err(RegistryError::CorruptState)
    );

    storage.close().await;
    area.observe().await;
}

#[tokio::test]
async fn sct_004_security_audit_boundaries() {
    let area = Area::new();
    schema4(&area).await;
    {
        let c = connect(&area).await;
        let storage_tmp = area.open().await.unwrap();
        storage_tmp.close().await;
        c.close().await.unwrap();
    }
    area.observe().await;

    let storage = area.open().await.unwrap();
    let pool = storage.pool_for_test().clone();
    let source = id(5);
    let repository = storage.request_registry();
    let actor_str = id(1).encode();
    repository
        .append_control_security_audit(
            Some(&actor_str),
            "session.control.takeover",
            &format!("{}:old-native", source.encode()),
            "{\"controlEpoch\":\"1\"}",
            1234,
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM security_audit WHERE action='session.control.takeover'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let legacy_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_schema WHERE name='session_commands')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(legacy_exists, 0);
    let mut audit_conn = pool.acquire().await.unwrap();
    assert!(
        crate::session_storage::schema::validate_management_audit(&mut audit_conn)
            .await
            .is_ok()
    );
    drop(audit_conn);

    storage.close().await;
    area.observe().await;
}
