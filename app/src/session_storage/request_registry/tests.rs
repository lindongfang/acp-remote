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
