use super::*;
use crate::storage::{NodeMeta, SCHEMA_VERSION, Storage};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn id(value: u8) -> EntityId {
    EntityId::from_bytes([value; 16])
}

fn actor(value: u8, generation: i64) -> ActorContext {
    ActorContext {
        client_id: id(value),
        auth_generation: generation,
    }
}

fn deadline() -> RegistryDeadline {
    RegistryDeadline::new(tokio::time::Instant::now() + Duration::from_secs(5))
}

fn database() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("fixture clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "acp-native-release-{}-{}-{nonce}.sqlite3",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

async fn seed(storage: &Storage) {
    storage
        .write_node_meta(&NodeMeta {
            schema_version: SCHEMA_VERSION,
            node_id: "native-release-fixture".into(),
            public_key: vec![4; 65],
            key_file_ref: "identity.key".into(),
        })
        .await
        .expect("node metadata fixture");
    for (client, key, name) in [(id(1), 1_u8, "first"), (id(2), 2_u8, "second")] {
        sqlx::query(
            "INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) \
             VALUES(?,? ,?,'trusted',3,0)",
        )
        .bind(client.encode())
        .bind(vec![key])
        .bind(name)
        .execute(storage.pool_for_test())
        .await
        .expect("client fixture");
    }
    sqlx::query("INSERT INTO product_sources VALUES(?,'release source')")
        .bind(id(10).encode())
        .execute(storage.pool_for_test())
        .await
        .expect("source fixture");
    for (target, name) in [(id(11), "release native target"), (id(12), "other target")] {
        sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
            .bind(target.encode())
            .bind(id(10).encode())
            .bind(name)
            .execute(storage.pool_for_test())
            .await
            .expect("target fixture");
    }
}

fn clean(path: &Path) {
    for name in ["", "-wal", "-shm"] {
        let candidate = PathBuf::from(format!("{}{name}", path.display()));
        if candidate.exists() {
            std::fs::remove_file(candidate).expect("owned fixture cleanup");
        }
    }
}

#[tokio::test]
async fn durable_release_fact_survives_reopen_and_is_bound_to_actor_generation() {
    let path = database();
    let alice = actor(1, 3);
    let bob = actor(2, 3);
    let request = id(30);
    let operation = id(20);

    let storage = Storage::open(&path).await.expect("first owned database");
    seed(&storage).await;
    let operations = storage.native_release_operations();
    let registered = operations
        .register(alice, id(11), request, operation, 100, &deadline())
        .await
        .expect("registration");
    assert_eq!(registered.operation, operation);
    assert_eq!(
        operations
            .register(alice, id(11), request, id(22), 101, &deadline())
            .await
            .expect("same request and target must return the original fact")
            .operation,
        operation
    );
    assert!(matches!(
        operations
            .register(alice, id(12), request, id(23), 102, &deadline())
            .await,
        Err(RegistryError::Conflict)
    ));
    assert!(matches!(
        operations
            .register(alice, id(11), id(31), operation, 103, &deadline())
            .await,
        Err(RegistryError::Conflict)
    ));
    storage.close().await;

    let storage = Storage::open(&path).await.expect("reopened owned database");
    let operations = storage.native_release_operations();
    assert_eq!(
        operations
            .inspect(alice, operation, &deadline())
            .await
            .expect("durable owner inspection")
            .phase,
        NativeReleaseStoredPhase::Registered
    );
    assert!(matches!(
        operations.inspect(bob, operation, &deadline()).await,
        Err(RegistryError::UnknownRegistration)
    ));
    assert!(matches!(
        operations
            .inspect(actor(1, 2), operation, &deadline())
            .await,
        Err(RegistryError::UnknownRegistration)
    ));
    assert!(matches!(
        operations
            .register(alice, id(11), request, operation, 104, &deadline())
            .await,
        Ok(NativeReleaseRecord { .. })
    ));

    let releasing = operations
        .begin(alice, operation, 102, &deadline())
        .await
        .expect("begin");
    assert_eq!(releasing.phase, NativeReleaseStoredPhase::Releasing);
    assert_eq!(
        operations
            .begin(alice, operation, 103, &deadline())
            .await
            .expect("idempotent begin")
            .phase,
        NativeReleaseStoredPhase::Releasing
    );
    let released = operations
        .finish(alice, operation, None, 104, &deadline())
        .await
        .expect("successful finish");
    assert_eq!(released.phase, NativeReleaseStoredPhase::Released);
    assert_eq!(released.error_code, None);
    assert_eq!(
        operations
            .finish(alice, operation, None, 105, &deadline())
            .await
            .expect("idempotent successful finish"),
        released
    );
    assert!(matches!(
        operations
            .finish(alice, operation, Some("protocol_failure"), 106, &deadline())
            .await,
        Err(RegistryError::Conflict)
    ));

    storage.close().await;
    clean(&path);
}

#[tokio::test]
async fn finish_storage_failure_leaves_releasing_and_can_retry_with_terminal_failure() {
    let path = database();
    let alice = actor(1, 3);
    let operation = id(21);
    let storage = Storage::open(&path).await.expect("owned database");
    seed(&storage).await;
    let operations = storage.native_release_operations();
    operations
        .register(alice, id(11), id(30), operation, 100, &deadline())
        .await
        .expect("registration");
    operations
        .begin(alice, operation, 101, &deadline())
        .await
        .expect("begin");

    sqlx::query(
        "CREATE TRIGGER release_finish_fault \
         BEFORE UPDATE OF phase,error_code ON node_native_release_operations \
         WHEN OLD.phase='releasing' \
         BEGIN SELECT RAISE(ABORT,'release finish fixture'); END",
    )
    .execute(storage.pool_for_test())
    .await
    .expect("finish fault");
    assert!(matches!(
        operations
            .finish(alice, operation, Some("protocol_failure"), 102, &deadline())
            .await,
        Err(RegistryError::StorageUnavailable)
    ));
    assert_eq!(
        operations
            .inspect(alice, operation, &deadline())
            .await
            .expect("releasing fact after failed finish")
            .phase,
        NativeReleaseStoredPhase::Releasing
    );

    sqlx::query("DROP TRIGGER release_finish_fault")
        .execute(storage.pool_for_test())
        .await
        .expect("remove finish fault");
    let failed = operations
        .finish(alice, operation, Some("protocol_failure"), 103, &deadline())
        .await
        .expect("retry terminal failure");
    assert_eq!(failed.phase, NativeReleaseStoredPhase::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("protocol_failure"));
    assert_eq!(
        operations
            .finish(alice, operation, Some("protocol_failure"), 104, &deadline(),)
            .await
            .expect("idempotent failed finish"),
        failed
    );

    storage.close().await;
    clean(&path);
}
