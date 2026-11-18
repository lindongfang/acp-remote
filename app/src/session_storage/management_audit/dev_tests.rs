use super::super::request_registry::connection_close_test_hooks::{CloseBarrier, ClosePoint};
use super::*;
use crate::application::{
    queue_owner::{BlockedEnvironment, ConsumeReply, QueueService},
    request_registry::{RegistryDeadline, RequestAuthority, ValidatedBodyPort, ValidatedMutation},
};
use crate::domain::{ActorContext, native::NativeSessionKey, request_registry::RequestFact};
use crate::{
    domain::{
        EntityId,
        management_audit::DEFAULT_RETENTION_MS,
        request_registry::{QueueAction, RegistrationId},
    },
    storage::{NodeMeta, SCHEMA_VERSION},
};
use sha2::{Digest, Sha256};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicI64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{Duration, Instant};

const NOW: i64 = 10_000_000_000;
fn id(n: u64) -> EntityId {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&n.to_be_bytes());
    EntityId::from_bytes(bytes)
}
struct Clock(AtomicI64);
impl NodeClock for Clock {
    fn unix_millis(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct Authority(sqlx::SqlitePool);
impl RequestAuthority for Authority {
    type Actor = EntityId;
    async fn authorize(
        &self,
        actor: &EntityId,
        _: EntityId,
        _: QueueAction,
    ) -> Result<ActorContext, RegistryError> {
        self.authorize_read(actor, id(2)).await
    }
    async fn authorize_read(
        &self,
        actor: &EntityId,
        _: EntityId,
    ) -> Result<ActorContext, RegistryError> {
        let record: Option<(String, i64)> =
            sqlx::query_as("SELECT status,auth_generation FROM main.clients WHERE client_id=?")
                .bind(actor.encode())
                .fetch_optional(&self.0)
                .await?;
        match record {
            Some((status, generation)) if status == "trusted" => Ok(ActorContext {
                client_id: *actor,
                auth_generation: generation,
            }),
            _ => Err(RegistryError::InvalidActor),
        }
    }
    async fn resolve_target(&self, target: EntityId) -> Result<NativeSessionKey, RegistryError> {
        let record: Option<(String, String)> = sqlx::query_as(
            "SELECT source_id,native_id FROM main.product_targets WHERE target_id=?",
        )
        .bind(target.encode())
        .fetch_optional(&self.0)
        .await?;
        let (source, native) = record.ok_or(RegistryError::InvalidTarget)?;
        NativeSessionKey::from_config(
            Some(EntityId::decode(&source).map_err(|_| RegistryError::CorruptState)?),
            &native,
        )
        .map_err(|_| RegistryError::InvalidTarget)
    }
}
struct Validator;
impl ValidatedBodyPort for Validator {
    type Raw = str;
    type Body = String;
    fn validate(&self, text: &str) -> Result<(String, [u8; 32]), RegistryError> {
        Ok((text.to_owned(), Sha256::digest(text.as_bytes()).into()))
    }
}
type Service = QueueService<Authority, Validator, SqliteRequestRegistry, SystemNodeClock>;
async fn resolve(service: &Service, mutation: ValidatedMutation<String>) -> RegistrationId {
    let fact = service.register(&id(1), id(2), &mutation).await.unwrap();
    assert!(matches!(
        service.consume(&id(1), id(2), fact.id(), mutation).await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled { .. },
            ..
        }
    ));
    fact.id()
}
struct FixtureRoots {
    directory: PathBuf,
    clock: Arc<Clock>,
}
struct Fixture {
    storage: crate::sqlite_fixture_scope::FixtureStorage,
    roots: Arc<FixtureRoots>,
    scope: crate::sqlite_fixture_scope::SqliteFixtureScope,
}
impl std::ops::Deref for Fixture {
    type Target = FixtureRoots;
    fn deref(&self) -> &FixtureRoots {
        &self.roots
    }
}
impl Fixture {
    async fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acp-remote-audit-dev-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let roots = Arc::new(FixtureRoots {
            directory,
            clock: Arc::new(Clock(AtomicI64::new(NOW))),
        });
        let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &roots.directory.join("node.sqlite3"),
            roots.clone(),
        );
        let storage = scope
            .open_with(sqlx::sqlite::SqlitePoolOptions::new().max_connections(2))
            .await
            .unwrap();
        storage
            .write_node_meta(&NodeMeta {
                schema_version: SCHEMA_VERSION,
                node_id: "audit-development".into(),
                public_key: vec![1, 2, 3],
                key_file_ref: "synthetic-unused.key".into(),
            })
            .await
            .unwrap();
        sqlx::query("INSERT INTO main.clients VALUES(?,?,'audit dev','trusted',0,0,NULL)")
            .bind(id(1).encode())
            .bind(vec![1u8])
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO main.product_sources VALUES(?,'audit dev')")
            .bind(id(3).encode())
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        sqlx::query("INSERT INTO main.product_targets VALUES(?,?,?,NULL)")
            .bind(id(2).encode())
            .bind(id(3).encode())
            .bind("audit-dev-native")
            .execute(storage.pool_for_test())
            .await
            .unwrap();
        Self {
            storage,
            roots,
            scope,
        }
    }
    async fn reopen(&mut self) {
        self.scope.stop_and_observe().await;
        let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
            &self.directory.join("node.sqlite3"),
            self.roots.clone(),
        );
        self.scope = scope;
        self.storage = self.scope.open().await.unwrap();
    }
    fn audit(&self) -> SqliteManagementAudit {
        self.storage
            .request_registry()
            .management_audit()
            .test_port()
            .with_clock(self.clock.clone())
    }
    async fn source(&self, n: u64) -> AuditMetadata {
        let request = RegistrationId::new(id(n));
        sqlx::query("INSERT INTO main.node_request_registrations VALUES(?,1,?,0,'input.submit',?,NULL,NULL,?,10,0)").bind(request.encode()).bind(id(1).encode()).bind(id(2).encode()).bind(vec![0u8;32]).execute(self.storage.pool_for_test()).await.unwrap();
        sqlx::query("INSERT INTO main.node_request_operations VALUES(?,?,?,0,'settled',11,12,'queued',?,?,NULL,NULL)").bind(request.encode()).bind(id(n).encode()).bind(id(9).encode()).bind(id(n).encode()).bind(1u64.to_be_bytes().to_vec()).execute(self.storage.pool_for_test()).await.unwrap();
        AuditMetadata {
            request,
            actor: id(1),
            action: QueueAction::Submit,
            target: id(2),
            outcome: QueueOutcome::Queued {
                item: id(n),
                revision: 1,
            },
            observed_at_ms: 13,
        }
    }
    async fn count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM main.node_management_audit")
            .fetch_one(self.storage.pool_for_test())
            .await
            .unwrap()
    }
    async fn seed(&self, count: i64) {
        const CTE: &str = "WITH RECURSIVE c(n) AS (SELECT 100 UNION ALL SELECT n+1 FROM c WHERE n+1<100+?), ids(id) AS (SELECT 'AAAAAAAAAAAAAAAAAA'||substr('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_',n/16384%64+1,1)||substr('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_',n/256%64+1,1)||substr('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_',n/4%64+1,1)||substr('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_',n%4*16+1,1) FROM c) ";
        let mut tx = self.storage.pool_for_test().begin().await.unwrap();
        sqlx::query(&(CTE.to_owned()+"INSERT INTO main.node_request_registrations SELECT id,1,?,0,'input.submit',?,NULL,NULL,zeroblob(32),10,0 FROM ids")).bind(count).bind(id(1).encode()).bind(id(2).encode()).execute(&mut *tx).await.unwrap();
        sqlx::query(&(CTE.to_owned()+"INSERT INTO main.node_request_operations SELECT id,id,?,0,'settled',11,12,'queued',id,X'0000000000000001',NULL,NULL FROM ids")).bind(count).bind(id(9).encode()).execute(&mut *tx).await.unwrap();
        sqlx::query(&(CTE.to_owned()+"INSERT INTO main.node_management_audit SELECT 'queue.settled:'||id,1,'queue.settled',id,id,?,'input.submit',?,'registered_operation',0,0,?,'queued',id,X'0000000000000001',NULL,NULL,12,13,?,?,0 FROM ids")).bind(count).bind(id(1).encode()).bind(id(2).encode()).bind(id(9).encode()).bind(NOW).bind(NOW+DEFAULT_RETENTION_MS).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    async fn finish(self, unconfirmed: bool) {
        self.storage.close().await;
        self.scope.stop_and_observe().await;
        let status = self.storage.registry_close_status();
        assert_eq!(
            status,
            if unconfirmed {
                super::super::ConnectionCloseStatus::Unconfirmed
            } else {
                super::super::ConnectionCloseStatus::Complete
            }
        );
        drop(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(self.directory.join("node.sqlite3"))
                .unwrap(),
        );
        drop(self.storage);
        let directory = &self.roots.directory;
        assert!(directory.starts_with(std::env::temp_dir()));
        assert!(
            directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("acp-remote-audit-dev-")
        );
        std::fs::remove_dir_all(directory).unwrap();
        assert!(!directory.exists());
    }
}

#[tokio::test]
async fn audit_dev_default_retention_duplicate_cleanup_and_source() {
    let f = Fixture::new().await;
    let audit = f.audit();
    let metadata = f.source(10).await;
    let first = audit.test_port().append_detailed(&metadata).await.unwrap();
    assert_eq!(
        (first.recorded_at_ms, first.retain_until_ms),
        (NOW, NOW + DEFAULT_RETENTION_MS)
    );
    assert_eq!(
        first.id.encode(),
        format!("queue.settled:{}", metadata.request.encode())
    );
    f.clock.0.store(first.retain_until_ms - 1, Ordering::SeqCst);
    audit.append(&f.source(11).await).await.unwrap();
    assert_eq!(f.count().await, 2);
    f.clock.0.store(first.retain_until_ms, Ordering::SeqCst);
    let repeat = audit.test_port().append_detailed(&metadata).await.unwrap();
    assert!(repeat.duplicate);
    assert_eq!(repeat.retain_until_ms, first.retain_until_ms);
    assert_eq!(f.count().await, 2);
    audit.append(&f.source(12).await).await.unwrap();
    assert_eq!(f.count().await, 2);
    let mut wrong = f.source(13).await;
    wrong.actor = id(99);
    assert_eq!(
        audit.test_port().append_detailed(&wrong).await,
        Err(Error::InvalidSource)
    );
    assert_eq!(f.count().await, 2);
    f.finish(false).await;
}

#[tokio::test]
async fn audit_dev_clock_protection_and_historical_actor() {
    let f = Fixture::new().await;
    let audit = f.audit();
    let old = f.source(10).await;
    audit.append(&old).await.unwrap();
    sqlx::query("UPDATE main.node_management_audit SET retention_hold=1")
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("UPDATE main.clients SET status='revoked',auth_generation=1")
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    sqlx::query("UPDATE main.node_request_registrations SET retired=1 WHERE request_id=?")
        .bind(old.request.encode())
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    f.clock
        .0
        .store(NOW + DEFAULT_RETENTION_MS, Ordering::SeqCst);
    assert!(
        audit
            .test_port()
            .append_detailed(&old)
            .await
            .unwrap()
            .duplicate
    );
    audit.append(&f.source(11).await).await.unwrap();
    assert_eq!(f.count().await, 2);
    for (n, time) in [(12, -1), (13, i64::MAX - DEFAULT_RETENTION_MS + 1)] {
        let m = f.source(n).await;
        f.clock.0.store(time, Ordering::SeqCst);
        assert_eq!(
            audit.test_port().append_detailed(&m).await,
            Err(Error::Clock)
        );
    }
    f.clock
        .0
        .store(i64::MAX - DEFAULT_RETENTION_MS, Ordering::SeqCst);
    assert_eq!(
        audit
            .test_port()
            .append_detailed(&f.source(14).await)
            .await
            .unwrap()
            .retain_until_ms,
        i64::MAX
    );
    assert_eq!(f.count().await, 2);
    f.finish(false).await;
}

#[tokio::test]
async fn audit_dev_real_100000_and_two_original_connections() {
    let f = Fixture::new().await;
    f.seed(99_999).await;
    assert_eq!(f.count().await, 99_999);
    let registry = f.storage.request_registry();
    let audit = f.audit();
    let a = f.source(1_000_001).await;
    let b = f.source(1_000_002).await;
    let ap = audit
        .test_port()
        .pause_next(AuditPoint::AfterAcquire)
        .unwrap();
    let aa = audit.clone();
    let at = tokio::spawn(async move { aa.test_port().append_detailed(&a).await });
    ap.wait_until_entered().await;
    let bp = audit
        .test_port()
        .pause_next(AuditPoint::AfterAcquire)
        .unwrap();
    let ba = audit.clone();
    let bt = tokio::spawn(async move { ba.test_port().append_detailed(&b).await });
    bp.wait_until_entered().await;
    assert_eq!(registry.observe_authority_slots_for_test().leases, 2);
    let close = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close.clone())
        .unwrap();
    ap.release();
    bp.release();
    close.arrived().await;
    let slots = loop {
        let view = registry.observe_authority_slots_for_test();
        if view.leases == 0 && view.slots.len() == 2 {
            break view.slots;
        }
        tokio::task::yield_now().await;
    };
    assert!(!slots[0].same_slot(&slots[1]));
    close.release();
    let results = [at.await.unwrap(), bt.await.unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.iter().filter(|r| **r == Err(Error::Full)).count(),
        1
    );
    for slot in slots {
        assert_eq!(
            slot.wait().await,
            super::super::request_registry::authority_slot_observation::SlotOutcome::Confirmed
        );
    }
    assert_eq!(f.count().await, 100_000);
    assert_eq!(
        audit.append(&f.source(1_000_003).await).await,
        Err(AuditFailure::Full)
    );
    assert_eq!(f.count().await, 100_000);
    f.finish(false).await;
}

#[tokio::test]
async fn audit_dev_rollback_true_commit_veto_and_lost_reply() {
    for mode in 0..3 {
        let f = Fixture::new().await;
        let audit = f.audit();
        audit.append(&f.source(10).await).await.unwrap();
        f.clock
            .0
            .store(NOW + DEFAULT_RETENTION_MS, Ordering::SeqCst);
        let metadata = f.source(11).await;
        match mode {
            0 => audit
                .test_port()
                .fault_next(AuditFault::FailBeforeCommit)
                .unwrap(),
            1 => audit.test_port().veto_commit_once().unwrap(),
            _ => audit
                .test_port()
                .fault_next(AuditFault::LoseReplyAfterCommitAck)
                .unwrap(),
        }
        assert!(audit.test_port().append_detailed(&metadata).await.is_err());
        let requests: Vec<String> =
            sqlx::query_scalar("SELECT request_id FROM main.node_management_audit")
                .fetch_all(f.storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(
            requests,
            vec![RegistrationId::new(id(if mode == 2 { 11 } else { 10 })).encode()]
        );
        if mode == 2 {
            assert!(
                audit
                    .test_port()
                    .append_detailed(&metadata)
                    .await
                    .unwrap()
                    .duplicate
            );
        }
        f.finish(false).await;
    }
}

#[tokio::test]
async fn audit_dev_four_actual_vm_cancellation_and_original_slot() {
    for point in [
        AuditSqlPoint::Begin,
        AuditSqlPoint::Prune,
        AuditSqlPoint::Insert,
        AuditSqlPoint::Commit,
    ] {
        let f = Fixture::new().await;
        let audit = f.audit();
        let metadata = f.source(10).await;
        let gate: VmGate = audit.test_port().pause_vm_next(point).unwrap();
        let mut future = Box::pin(async move { audit.append(&metadata).await });
        tokio::select! {()=gate.entered()=>{},result=&mut future=>panic!("audit completed before VM gate: {result:?}")}
        drop(future);
        let slots = f
            .storage
            .request_registry()
            .observe_authority_slots_for_test()
            .slots;
        assert_eq!(slots.len(), 1);
        assert_eq!(
            slots[0].stored(),
            super::super::request_registry::authority_slot_observation::SlotOutcome::Pending
        );
        gate.release_continue();
        assert_eq!(
            slots[0].wait().await,
            super::super::request_registry::authority_slot_observation::SlotOutcome::Confirmed
        );
        assert!(f.count().await <= 1);
        f.finish(false).await;
    }
}

#[tokio::test]
async fn audit_dev_audit_layout_degrades_but_required_trigger_refuses() {
    let mut f = Fixture::new().await;
    let metadata = f.source(10).await;
    sqlx::query("DROP INDEX main.node_management_audit_expiry")
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    f.storage.close().await;
    f.reopen().await;
    assert_eq!(
        f.audit().test_port().append_detailed(&metadata).await,
        Err(Error::Layout)
    );
    sqlx::query("CREATE TRIGGER main.audit_dev_required_trigger AFTER INSERT ON clients BEGIN SELECT 1; END").execute(f.storage.pool_for_test()).await.unwrap();
    f.storage.close().await;
    f.scope.stop_and_observe().await;
    let failed_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &f.directory.join("node.sqlite3"),
        f.roots.clone(),
    );
    let failed = failed_scope.open().await;
    assert!(
        matches!(failed, Err(ref error) if matches!(error.as_ref(), crate::error::StorageError::UnrecognizedSchema))
    );
    failed_scope.stop_and_observe().await;
    f.finish(false).await;
}

#[derive(PartialEq, Eq)]
struct Historical6Table {
    columns: Vec<Vec<u8>>,
    indexes: Vec<Vec<u8>>,
    index_columns: std::collections::BTreeMap<String, Vec<Vec<u8>>>,
    foreign_keys: Vec<Vec<u8>>,
    rows: Vec<Vec<u8>>,
}
#[derive(PartialEq, Eq)]
struct Historical6Snapshot {
    definitions: Vec<Vec<u8>>,
    tables: std::collections::BTreeMap<String, Historical6Table>,
    user_version: i64,
    meta_version: i64,
    key_digest: [u8; 32],
}
fn historical6_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn historical6_encode_row(row: &sqlx::sqlite::SqliteRow) -> Vec<u8> {
    use sqlx::{Row, TypeInfo, ValueRef};
    let mut encoded = Vec::new();
    encoded.extend_from_slice(&(row.len() as u64).to_be_bytes());
    for index in 0..row.len() {
        let value = row.try_get_raw(index).unwrap();
        let (tag, bytes) = if value.is_null() {
            (0, Vec::new())
        } else {
            match value.type_info().name() {
                "INTEGER" => (1, row.get::<i64, _>(index).to_be_bytes().to_vec()),
                "REAL" => (2, row.get::<f64, _>(index).to_bits().to_be_bytes().to_vec()),
                "TEXT" => (3, row.get::<Vec<u8>, _>(index)),
                "BLOB" => (4, row.get::<Vec<u8>, _>(index)),
                other => panic!("unexpected SQLite storage class {other}"),
            }
        };
        encoded.push(tag);
        encoded.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        encoded.extend_from_slice(&bytes);
    }
    encoded
}
async fn historical6_rows(pool: &sqlx::SqlitePool, query: &str) -> Vec<Vec<u8>> {
    let mut rows: Vec<_> = sqlx::query(query)
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(historical6_encode_row)
        .collect();
    rows.sort();
    rows
}
async fn historical6_snapshot(
    pool: &sqlx::SqlitePool,
    directory: &std::path::Path,
    normalize_success_version: bool,
) -> Historical6Snapshot {
    use sqlx::Row;
    let mut definitions = Vec::new();
    let mut tables = std::collections::BTreeMap::new();
    for schema in ["main", "temp"] {
        definitions.extend(
            historical6_rows(
                pool,
                &format!("SELECT '{schema}',type,name,tbl_name,sql FROM {schema}.sqlite_schema"),
            )
            .await,
        );
        let names: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT name FROM {schema}.sqlite_schema WHERE type='table' ORDER BY name"
        ))
        .fetch_all(pool)
        .await
        .unwrap();
        for name in names {
            let quoted = historical6_identifier(&name);
            let columns =
                historical6_rows(pool, &format!("PRAGMA {schema}.table_xinfo({quoted})")).await;
            let indexes =
                historical6_rows(pool, &format!("PRAGMA {schema}.index_list({quoted})")).await;
            let mut index_columns = std::collections::BTreeMap::new();
            for index in sqlx::query(&format!("PRAGMA {schema}.index_list({quoted})"))
                .fetch_all(pool)
                .await
                .unwrap()
            {
                let index_name: String = index.get("name");
                index_columns.insert(
                    index_name.clone(),
                    historical6_rows(
                        pool,
                        &format!(
                            "PRAGMA {schema}.index_xinfo({})",
                            historical6_identifier(&index_name)
                        ),
                    )
                    .await,
                );
            }
            let foreign_keys =
                historical6_rows(pool, &format!("PRAGMA {schema}.foreign_key_list({quoted})"))
                    .await;
            let query = if schema == "main" && name == "node_meta" && normalize_success_version {
                "SELECT singleton,6 AS schema_version,node_id,public_key,key_file_ref FROM main.node_meta".to_owned()
            } else {
                format!("SELECT * FROM {schema}.{quoted}")
            };
            tables.insert(
                format!("{schema}.{name}"),
                Historical6Table {
                    columns,
                    indexes,
                    index_columns,
                    foreign_keys,
                    rows: historical6_rows(pool, &query).await,
                },
            );
        }
    }
    definitions.sort();
    Historical6Snapshot {
        definitions,
        tables,
        user_version: sqlx::query_scalar("PRAGMA main.user_version")
            .fetch_one(pool)
            .await
            .unwrap(),
        meta_version: sqlx::query_scalar(
            "SELECT schema_version FROM main.node_meta WHERE singleton=1",
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        key_digest: Sha256::digest(std::fs::read(directory.join("identity.key")).unwrap()).into(),
    }
}
async fn historical6_integrity(pool: &sqlx::SqlitePool) {
    let checks: Vec<String> = sqlx::query_scalar("PRAGMA main.integrity_check")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(checks, ["ok"]);
    assert!(
        sqlx::query("PRAGMA main.foreign_key_check")
            .fetch_all(pool)
            .await
            .unwrap()
            .is_empty()
    );
}
async fn historical6_seed(pool: &sqlx::SqlitePool) -> AuditMetadata {
    for (table, column, value) in [
        ("clients", "client_id", id(1)),
        ("product_sources", "source_id", id(3)),
        ("product_targets", "target_id", id(2)),
        ("node_request_registrations", "request_id", id(10)),
        ("node_request_operations", "operation_id", id(10)),
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM main.{table} WHERE {column}=?"
        ))
        .bind(value.encode())
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "historical audit seed collision in {table}");
    }
    let public_key_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM main.clients WHERE public_key=?")
            .bind(vec![1u8])
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(public_key_count, 0);
    sqlx::query("INSERT INTO main.clients(client_id,public_key,display_name,status,auth_generation,created_at,revoked_at) VALUES(?,?,'audit dev','trusted',0,0,NULL)")
        .bind(id(1).encode()).bind(vec![1u8]).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO main.product_sources(source_id,name) VALUES(?,'audit dev')")
        .bind(id(3).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO main.product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,NULL)")
        .bind(id(2).encode()).bind(id(3).encode()).bind("audit-dev-native").execute(pool).await.unwrap();
    let request = RegistrationId::new(id(10));
    sqlx::query("INSERT INTO main.node_request_registrations(request_id,intent_version,actor_id,register_auth_generation,action,target_id,item_id,expected_revision,body_digest,registered_at_ms,retired) VALUES(?,1,?,0,'input.submit',?,NULL,NULL,?,10,0)")
        .bind(request.encode()).bind(id(1).encode()).bind(id(2).encode()).bind(vec![0u8;32]).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO main.node_request_operations(request_id,operation_id,runtime_epoch,admission_auth_generation,phase,begun_at_ms,resolved_at_ms,result_code,result_item_id,result_revision,restored_count,rejection_code) VALUES(?,?,?,0,'settled',11,12,'queued',?,?,NULL,NULL)")
        .bind(request.encode()).bind(id(10).encode()).bind(id(9).encode()).bind(id(10).encode()).bind(1u64.to_be_bytes().to_vec()).execute(pool).await.unwrap();
    AuditMetadata {
        request,
        actor: id(1),
        action: QueueAction::Submit,
        target: id(2),
        outcome: QueueOutcome::Queued {
            item: id(10),
            revision: 1,
        },
        observed_at_ms: 13,
    }
}
fn historical6_assert_seed_preserved(before: &Historical6Snapshot, seeded: &Historical6Snapshot) {
    assert!(
        before.definitions == seeded.definitions,
        "seed must preserve all schema definitions"
    );
    assert_eq!(before.user_version, 6);
    assert_eq!(before.meta_version, 6);
    assert_eq!(seeded.user_version, 6);
    assert_eq!(seeded.meta_version, 6);
    assert_eq!(before.key_digest, seeded.key_digest);
    assert_eq!(before.tables.len(), seeded.tables.len());
    for (name, old) in &before.tables {
        let new = &seeded.tables[name];
        assert!(
            old.columns == new.columns,
            "historical columns changed in {name}"
        );
        assert!(
            old.indexes == new.indexes,
            "historical indexes changed in {name}"
        );
        assert!(
            old.index_columns == new.index_columns,
            "historical index_columns changed in {name}"
        );
        assert!(
            old.foreign_keys == new.foreign_keys,
            "historical foreign_keys changed in {name}"
        );
        let seeded_table = [
            "main.clients",
            "main.product_sources",
            "main.product_targets",
            "main.node_request_registrations",
            "main.node_request_operations",
        ]
        .contains(&name.as_str());
        if seeded_table {
            let mut remaining = new.rows.clone();
            for row in &old.rows {
                let position = remaining
                    .iter()
                    .position(|candidate| candidate == row)
                    .unwrap();
                remaining.remove(position);
            }
            assert_eq!(
                remaining.len(),
                1,
                "only one declared seed row may be added to {name}"
            );
        } else {
            assert!(old.rows == new.rows, "historical rows changed in {name}");
        }
    }
}
async fn historical6_read_original(roots: &Arc<FixtureRoots>) -> Historical6Snapshot {
    let read_only_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &roots.directory.join("node.sqlite3"),
        roots.clone(),
    );
    read_only_scope.start_raw().await;
    let read_only_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            read_only_scope
                .options()
                .read_only(true)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let read_only_task_pool = read_only_pool.clone();
    let read_only_roots = roots.clone();
    let original = tokio::spawn(async move {
        let snapshot =
            historical6_snapshot(&read_only_task_pool, &read_only_roots.directory, false).await;
        assert_eq!(snapshot.user_version, 6);
        assert_eq!(snapshot.meta_version, 6);
        assert_eq!(snapshot.tables.len(), 47);
        assert_eq!(snapshot.definitions.len(), 139);
        historical6_integrity(&read_only_task_pool).await;
        snapshot
    })
    .await;
    crate::sqlite_fixture_scope::SqliteFixtureScope::close_raw_pool(&read_only_pool).await;
    assert!(read_only_scope.stop_and_observe().await);
    original.unwrap()
}
async fn historical6_test_migration_rollbacks(
    roots: &Arc<FixtureRoots>,
    original: Historical6Snapshot,
) -> (Historical6Snapshot, AuditMetadata) {
    use super::super::schema::{self, MigrationFault};
    let raw_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &roots.directory.join("node.sqlite3"),
        roots.clone(),
    );
    raw_scope.start_raw().await;
    let options = raw_scope
        .options()
        .create_if_missing(false)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let task_pool = pool.clone();
    let task_roots = roots.clone();
    // The outer test retains the pool and closes it even if a raw migration assertion panics.
    let raw_checks = tokio::spawn(async move {
        let before = historical6_snapshot(&task_pool, &task_roots.directory, false).await;
        assert!(before == original, "raw pool preparation changed original typed historical facts");
        assert_eq!(before.user_version, 6);
        assert_eq!(before.meta_version, 6);
        assert_eq!(before.tables.len(), 47);
        assert_eq!(before.definitions.len(), 139);
        historical6_integrity(&task_pool).await;
        let metadata = historical6_seed(&task_pool).await;
        let seeded = historical6_snapshot(&task_pool, &task_roots.directory, false).await;
        historical6_assert_seed_preserved(&before, &seeded);
        for fault in [
            MigrationFault::AuditTable,
            MigrationFault::AuditIndex,
            MigrationFault::AuditMetadata,
            MigrationFault::AuditPragma,
            MigrationFault::Validation,
            MigrationFault::Commit,
            MigrationFault::DeferredCommit,
        ] {
            let error = schema::initialize_for_test(&task_pool, fault).await.unwrap_err();
            if fault == MigrationFault::DeferredCommit {
                let crate::error::StorageError::Database(sqlx::Error::Database(database)) = error else {
                    panic!("DeferredCommit must reach a real engine COMMIT");
                };
                assert_eq!(database.code().as_deref(), Some("787"));
                assert!(database.message().contains("FOREIGN KEY"));
            } else {
                assert!(matches!(error, crate::error::StorageError::MigrationInterrupted));
            }
            assert!(historical6_snapshot(&task_pool, &task_roots.directory, false).await == seeded, "migration rollback changed typed historical facts");
            let unexpected: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM main.sqlite_schema WHERE name='node_management_audit' OR name LIKE 'node_approval_%' OR name IN ('node_catalog_commands','node_launch_profiles','node_target_launch_selection','migration_test_parent','migration_test_child')")
                .fetch_one(&task_pool).await.unwrap();
            assert_eq!(unexpected, 0);
            historical6_integrity(&task_pool).await;
        }
        (seeded, metadata)
    }).await;
    crate::sqlite_fixture_scope::SqliteFixtureScope::close_raw_pool(&pool).await;
    assert!(raw_scope.stop_and_observe().await);
    raw_checks.unwrap()
}
async fn historical6_validate_upgraded_storage(
    roots: Arc<FixtureRoots>,
    seeded: Historical6Snapshot,
    metadata: AuditMetadata,
) {
    let scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &roots.directory.join("node.sqlite3"),
        roots.clone(),
    );
    let storage = scope.open().await.unwrap();
    let f = Fixture {
        storage,
        roots,
        scope,
    };
    assert_eq!(
        f.storage
            .load_node_meta()
            .await
            .unwrap()
            .unwrap()
            .schema_version,
        crate::storage::SCHEMA_VERSION
    );
    let migrated = historical6_snapshot(f.storage.pool_for_test(), &f.directory, true).await;
    assert_eq!(migrated.user_version, crate::storage::SCHEMA_VERSION);
    assert_eq!(migrated.meta_version, crate::storage::SCHEMA_VERSION);
    assert_eq!(migrated.key_digest, seeded.key_digest);
    for definition in &seeded.definitions {
        assert!(migrated.definitions.contains(definition));
    }
    for (name, table) in &seeded.tables {
        assert!(
            &migrated.tables[name] == table,
            "historical table changed: {name}"
        );
    }
    let actual_new: std::collections::BTreeSet<_> = migrated
        .tables
        .keys()
        .filter(|name| !seeded.tables.contains_key(*name))
        .map(String::as_str)
        .collect();
    let expected_new: std::collections::BTreeSet<_> = [
        "main.node_management_audit",
        "main.node_approval_requests",
        "main.node_approval_associations",
        "main.node_approval_commands",
        "main.node_approval_decisions",
        "main.node_approval_reply_attempts",
        "main.node_approval_reply_facts",
        "main.node_approval_invalidations",
        "main.node_approval_audit",
        "main.node_catalog_commands",
        "main.node_launch_profiles",
        "main.node_target_launch_selection",
        "main.node_native_release_operations",
        "main.node_source_session_registrations",
        "main.node_source_session_operations",
        "main.node_session_capacity_releases",
        "main.node_session_capacity",
        "main.node_session_automatic_releases",
        "main.node_session_automatic_release_members",
        "main.node_session_parents",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual_new, expected_new);
    for name in actual_new {
        assert!(
            migrated.tables[name].rows.is_empty(),
            "new table should initially be empty: {name}"
        );
    }
    historical6_integrity(f.storage.pool_for_test()).await;
    f.audit().append(&metadata).await.unwrap();
    assert_eq!(f.count().await, 1);
    f.finish(false).await;
}
#[tokio::test]
async fn audit_dev_real_storage_schema6_upgrade_and_fault_rollback() {
    const DATABASE: &[u8] = include_bytes!("../../../dev-fixtures/schema6/node.sqlite3");
    const KEY: &[u8] = include_bytes!("../../../dev-fixtures/schema6/identity.key");
    assert_eq!(DATABASE.len(), 634_880);
    assert_eq!(
        hex::encode(Sha256::digest(DATABASE)),
        "890151e9ee0ad0ff6c29e590a863c5b79d2f59d1b2440d653c71f6169c99505d"
    );
    assert_eq!(KEY.len(), 39);
    assert_eq!(
        hex::encode(Sha256::digest(KEY)),
        "d8785244405352be72e07ed8ac609558ec732e589641290b98ae991e4461cf33"
    );
    let directory = std::env::temp_dir().join(format!(
        "acp-remote-audit-dev-{}-{}-historical6",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("node.sqlite3"), DATABASE).unwrap();
    std::fs::write(directory.join("identity.key"), KEY).unwrap();
    assert!(
        std::fs::read(directory.join("node.sqlite3")).unwrap() == DATABASE,
        "database fixture copy differs"
    );
    assert!(
        std::fs::read(directory.join("identity.key")).unwrap() == KEY,
        "key fixture copy differs"
    );
    let roots = Arc::new(FixtureRoots {
        directory,
        clock: Arc::new(Clock(AtomicI64::new(NOW))),
    });
    let preflight_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &roots.directory.join("node.sqlite3"),
        roots.clone(),
    );
    let preflight = preflight_scope.preflight().await;
    assert!(preflight_scope.stop_and_observe().await);
    preflight.unwrap();
    let original = historical6_read_original(&roots).await;
    let (seeded, metadata) = historical6_test_migration_rollbacks(&roots, original).await;
    historical6_validate_upgraded_storage(roots, seeded, metadata).await;
}

#[tokio::test]
async fn audit_dev_close_report_keeps_commit_and_global_history_separate() {
    let f = Fixture::new().await;
    let audit = f.audit();
    let registry = f.storage.request_registry();
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .unwrap();
    assert_eq!(
        audit.test_port().append_detailed(&f.source(10).await).await,
        Err(Error::CloseUnconfirmed { committed: true })
    );
    assert_eq!(f.count().await, 1);
    audit.append(&f.source(11).await).await.unwrap();
    assert_eq!(f.count().await, 2);
    f.finish(true).await;
}

#[tokio::test]
async fn audit_dev_five_real_r3_actions_use_sql_audit_port() {
    let f = Fixture::new().await;
    let audit = f.audit();
    let (service, driver) = f
        .storage
        .start_queue_service_with_audit_for_fixture(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
            Arc::new(audit),
        )
        .await
        .unwrap();
    let a = resolve(
        &service,
        ValidatedMutation::Submit(
            service
                .validate_body("development body is never audited")
                .unwrap(),
        ),
    )
    .await;
    resolve(
        &service,
        ValidatedMutation::Edit {
            item: a.entity(),
            revision: 1,
            body: service
                .validate_body("replacement body is never audited")
                .unwrap(),
        },
    )
    .await;
    let b = resolve(
        &service,
        ValidatedMutation::Submit(service.validate_body("another private body").unwrap()),
    )
    .await;
    resolve(
        &service,
        ValidatedMutation::Prioritize {
            item: b.entity(),
            revision: 1,
        },
    )
    .await;
    resolve(
        &service,
        ValidatedMutation::Cancel {
            item: a.entity(),
            revision: 2,
        },
    )
    .await;
    resolve(&service, ValidatedMutation::Resume).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.count().await != 6 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT action FROM main.node_management_audit ORDER BY action",
    )
    .fetch_all(f.storage.pool_for_test())
    .await
    .unwrap();
    assert_eq!(
        actions,
        vec![
            "input.cancel",
            "input.edit",
            "input.prioritize",
            "input.resume",
            "input.submit"
        ]
    );
    assert!(service.take_audit_warnings().is_empty());
    assert_no_body_columns(&f).await;
    assert!(
        service
            .shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(5)
            ))
            .await
            .joined
    );
    drop(driver);
    drop(service);
    f.finish(false).await;
}

async fn assert_no_body_columns(f: &Fixture) {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM main.pragma_table_info('node_management_audit','main')",
    )
    .fetch_all(f.storage.pool_for_test())
    .await
    .unwrap();
    assert!(
        !columns
            .iter()
            .any(|c| c.contains("body") || c.contains("digest"))
    );
}

// These tests call the real no-audit-argument factories, separately from the preserved A1 fixtures.
type A2Service<C> = QueueService<Authority, Validator, SqliteRequestRegistry, C>;
type A2Check = Result<(), String>;

async fn a2_resolve<C: NodeClock + 'static>(
    service: &A2Service<C>,
    mutation: ValidatedMutation<String>,
) -> Result<RegistrationId, String> {
    let fact = service
        .register(&id(1), id(2), &mutation)
        .await
        .map_err(|_| "default register failed")?;
    if !matches!(
        service.consume(&id(1), id(2), fact.id(), mutation).await,
        ConsumeReply::Resolved {
            fact: RequestFact::Settled { .. },
            ..
        }
    ) {
        return Err("default operation did not settle".into());
    }
    Ok(fact.id())
}

async fn a2_five(service: &Service) -> A2Check {
    let first = a2_resolve(
        service,
        ValidatedMutation::Submit(service.validate_body("A2 synthetic first body").unwrap()),
    )
    .await?;
    a2_resolve(
        service,
        ValidatedMutation::Edit {
            item: first.entity(),
            revision: 1,
            body: service
                .validate_body("A2 synthetic replacement body")
                .unwrap(),
        },
    )
    .await?;
    let second = a2_resolve(
        service,
        ValidatedMutation::Submit(service.validate_body("A2 synthetic second body").unwrap()),
    )
    .await?;
    a2_resolve(
        service,
        ValidatedMutation::Prioritize {
            item: second.entity(),
            revision: 1,
        },
    )
    .await?;
    a2_resolve(
        service,
        ValidatedMutation::Cancel {
            item: first.entity(),
            revision: 2,
        },
    )
    .await?;
    a2_resolve(service, ValidatedMutation::Resume).await?;
    Ok(())
}

async fn a2_wait_count(f: &Fixture, expected: i64) -> A2Check {
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.count().await != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "default SQL audit count not reached".into())
}

async fn a2_stop<C: NodeClock + 'static>(
    service: A2Service<C>,
    driver: crate::application::queue_owner::NativeQueueDriver<String>,
    result: A2Check,
) -> A2Check {
    let report = service
        .shutdown(RegistryDeadline::new(
            Instant::now() + Duration::from_secs(5),
        ))
        .await;
    drop(driver);
    drop(service);
    if !report.joined {
        return Err("default queue did not actually join".into());
    }
    result
}

async fn a2_finish(f: Fixture, result: A2Check) {
    f.storage.close().await;
    f.scope.stop_and_observe().await;
    let mut errors: Vec<String> = result.err().into_iter().collect();
    if !f.storage.registry_close_status().is_complete() {
        errors.push("original registry close not Complete".into());
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let path = f.directory.join(format!("node.sqlite3{suffix}"));
        if !suffix.is_empty() && !path.exists() {
            eprintln!("a2 first-exclusive {suffix}=absent");
            continue;
        }
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
        {
            Ok(handle) => {
                drop(handle);
                eprintln!("a2 first-exclusive {suffix}=ok");
            }
            Err(error) => errors.push(format!("first-exclusive {suffix}: {error:?}")),
        }
    }
    drop(f.storage);
    assert!(
        errors.is_empty(),
        "a2 preserved={} errors={errors:?}",
        f.roots.directory.display()
    );
    let actual = f.roots.directory.canonicalize().unwrap();
    assert_eq!(
        actual.parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    assert!(
        actual
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("acp-remote-audit-dev-{}-", std::process::id()))
    );
    assert_eq!(
        std::fs::symlink_metadata(&actual)
            .unwrap()
            .file_attributes()
            & 0x400,
        0
    );
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let file = actual.join(format!("node.sqlite3{suffix}"));
        if file.exists() {
            let metadata = std::fs::symlink_metadata(&file).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.file_attributes() & 0x400, 0);
            std::fs::remove_file(file).unwrap();
        }
    }
    std::fs::remove_dir(actual).unwrap();
}

#[tokio::test]
async fn a2_default_queue_five_actions_are_real_sql() {
    let f = Fixture::new().await;
    let before = SystemNodeClock.unix_millis();
    let (service, driver) = f
        .storage
        .start_queue_service(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await
        .unwrap();
    let result: A2Check = async {
        a2_five(&service).await?;
        a2_wait_count(&f, 6).await?;
        let matching: i64 = sqlx::query_scalar("SELECT count(*) FROM main.node_management_audit a JOIN main.node_request_registrations r ON r.request_id=a.request_id JOIN main.node_request_operations o ON o.request_id=a.request_id WHERE a.actor_id=r.actor_id AND a.target_id=r.target_id AND a.action=r.action AND a.operation_id=o.operation_id AND a.runtime_epoch=o.runtime_epoch AND a.outcome=o.result_code")
            .fetch_one(f.storage.pool_for_test()).await.map_err(|_| "default basis query failed")?;
        if matching != 6 { return Err("default SQL basis did not match all six settled operations".into()); }
        let range: (i64, i64, i64, i64) = sqlx::query_as("SELECT min(recorded_at_ms),max(recorded_at_ms),min(retain_until_ms-recorded_at_ms),max(retain_until_ms-recorded_at_ms) FROM main.node_management_audit")
            .fetch_one(f.storage.pool_for_test()).await.map_err(|_| "default timestamp query failed")?;
        if range.0 < before || range.1 > SystemNodeClock.unix_millis() || range.2 != DEFAULT_RETENTION_MS || range.3 != DEFAULT_RETENTION_MS {
            return Err("default system retention interval did not match".into());
        }
        if !service.take_audit_warnings().is_empty() { return Err("healthy default generated an audit warning".into()); }
        eprintln!("a2 default queue six SQL rows/five actions, real basis and system30d verified");
        Ok(())
    }.await;
    let result = a2_stop(service, driver, result).await;
    a2_finish(f, result).await;
}

#[tokio::test]
async fn a2_default_wrapped_owner_and_clock_are_separate() {
    let f = Fixture::new().await;
    let foreign = Fixture::new().await;
    let expected = f.storage.request_registry().owner_scope();
    let before = SystemNodeClock.unix_millis();
    let (service, driver) = f
        .storage
        .start_queue_service_with_repository(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            Clock(AtomicI64::new(NOW)),
            Arc::new(BlockedEnvironment),
            move |registry| {
                if !expected.matches(&registry.owner_scope()) {
                    return Err(RegistryError::InvalidActor);
                }
                Ok(registry)
            },
        )
        .await
        .unwrap();
    let result: A2Check = async {
        a2_resolve(&service, ValidatedMutation::Resume).await?;
        a2_wait_count(&f, 1).await?;
        let row: (i64, i64, i64) = sqlx::query_as("SELECT observed_at_ms,recorded_at_ms,retain_until_ms FROM main.node_management_audit")
            .fetch_one(f.storage.pool_for_test()).await.map_err(|_| "wrapped row failed")?;
        if row.0 != NOW || row.1 < before || row.1 > SystemNodeClock.unix_millis() || row.2-row.1 != DEFAULT_RETENTION_MS || foreign.count().await != 0 {
            return Err("default audit clock or original-store ownership was replaced".into());
        }
        eprintln!("a2 wrapped same RegistryOwnerScope; foreign DB untouched; observed sentinel separate from system recorded time");
        Ok(())
    }.await;
    let result = a2_stop(service, driver, result).await;
    a2_finish(foreign, Ok(())).await;
    a2_finish(f, result).await;
}

async fn a2_one_warning(
    service: &Service,
) -> Result<Vec<crate::application::request_registry::AuditWarning>, String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let warnings = service.take_audit_warnings();
            if !warnings.is_empty() {
                return warnings;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "default audit warning not observed".into())
}

#[tokio::test]
async fn a2_default_unavailable_keeps_necessary_settlement() {
    let f = Fixture::new().await;
    sqlx::query("DROP INDEX main.node_management_audit_expiry")
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    let (service, driver) = f
        .storage
        .start_queue_service(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await
        .unwrap();
    let result: A2Check = async {
        let request = a2_resolve(&service, ValidatedMutation::Resume).await?;
        let warnings = a2_one_warning(&service).await?;
        if warnings.len()!=1 || warnings[0].request!=request || warnings[0].failure!=crate::application::request_registry::AuditFailure::Unavailable || f.count().await!=0 {
            return Err("audit-only layout failure did not preserve exact Unavailable warning/zero rows".into());
        }
        let settled: i64 = sqlx::query_scalar("SELECT count(*) FROM main.node_request_operations WHERE request_id=? AND phase='settled'")
            .bind(request.encode()).fetch_one(f.storage.pool_for_test()).await.map_err(|_| "necessary settlement query failed")?;
        if settled != 1 { return Err("optional Unavailable lost necessary settlement".into()); }
        eprintln!("a2 audit-only Unavailable, necessary operation settled once, no fake audit save");
        Ok(())
    }.await;
    let result = a2_stop(service, driver, result).await;
    a2_finish(f, result).await;
}

#[tokio::test]
async fn a2_default_full_is_real_100000_and_does_not_stop_action() {
    let f = Fixture::new().await;
    f.seed(100_000).await;
    let now = SystemNodeClock.unix_millis();
    sqlx::query("UPDATE main.node_management_audit SET recorded_at_ms=?,retain_until_ms=?")
        .bind(now)
        .bind(now + DEFAULT_RETENTION_MS)
        .execute(f.storage.pool_for_test())
        .await
        .unwrap();
    let before: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),sum(recorded_at_ms),sum(retain_until_ms) FROM main.node_management_audit",
    )
    .fetch_one(f.storage.pool_for_test())
    .await
    .unwrap();
    let (service, driver) = f
        .storage
        .start_queue_service(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await
        .unwrap();
    let result: A2Check = async {
        let request = a2_resolve(&service, ValidatedMutation::Resume).await?;
        let warnings = a2_one_warning(&service).await?;
        let after: (i64, i64, i64) = sqlx::query_as("SELECT count(*),sum(recorded_at_ms),sum(retain_until_ms) FROM main.node_management_audit")
            .fetch_one(f.storage.pool_for_test()).await.map_err(|_| "full-table preservation query failed")?;
        if before.0!=100_000 || before!=after || warnings.len()!=1 || warnings[0].request!=request || warnings[0].failure!=crate::application::request_registry::AuditFailure::Full {
            return Err("default Full failed preservation or exact warning".into());
        }
        eprintln!("a2 true100000 young rows unchanged by default Full; necessary Resume settled");
        Ok(())
    }.await;
    let result = a2_stop(service, driver, result).await;
    a2_finish(f, result).await;
}

#[tokio::test]
async fn a2_default_cannot_bypass_original_necessary_fault() {
    use super::super::request_registry::test_hooks::{Action, Point};
    let f = Fixture::new().await;
    let (service, driver) = f
        .storage
        .start_queue_service(
            Authority(f.storage.pool_for_test().clone()),
            Validator,
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await
        .unwrap();
    f.storage
        .request_registry()
        .faults_for_test()
        .arm(Point::RegisterInsert, Action::Fail)
        .unwrap();
    let result: A2Check = async {
        let mutation = ValidatedMutation::Resume;
        if !matches!(service.register(&id(1),id(2),&mutation).await, Err(RegistryError::StorageUnavailable)) {
            return Err("necessary register SQL fault was bypassed".into());
        }
        let count: i64=sqlx::query_scalar("SELECT count(*) FROM main.node_request_registrations").fetch_one(f.storage.pool_for_test()).await.map_err(|_| "register rollback query failed")?;
        if count!=0 || f.count().await!=0 || !service.take_audit_warnings().is_empty() { return Err("failed necessary register fabricated facts/audit".into()); }
        a2_resolve(&service, ValidatedMutation::Resume).await?;
        a2_wait_count(&f,1).await?;
        eprintln!("a2 actual RegisterInsert fault refused/rolled back; separate subsequent normal default request saved SQL");
        Ok(())
    }.await;
    let result = a2_stop(service, driver, result).await;
    a2_finish(f, result).await;
}
