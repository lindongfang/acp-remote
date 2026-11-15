// Included as a child of the existing node::tests module; uses only its synthetic fixture.
use super::*;
use crate::{
    application::request_registry::{
        CurrentRequestActor, OperationContext, QueueAction, RegisteredIntent, RegistrationId,
        RegistryDeadline, RequestRegistryRepository,
    },
    domain::{
        ActorContext, EntityId,
        native::{NativeSessionKey, RuntimeEpoch},
    },
    session_storage::request_registry::connection_close_test_hooks::{CloseBarrier, ClosePoint},
};
use sqlx::{Connection, sqlite::SqliteConnectOptions};
fn entity(n: u8) -> EntityId {
    EntityId::from_bytes([n; 16])
}
async fn seed(path: &Path) {
    let mut c = sqlx::SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .foreign_keys(true),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO clients(client_id,public_key,display_name,status,auth_generation,created_at) VALUES(?,X'09080706','synthetic-close-client','trusted',12,0)").bind(entity(1).encode()).execute(&mut c).await.unwrap();
    sqlx::query("INSERT INTO product_sources VALUES(?,'synthetic-close-source')")
        .bind(entity(80).encode())
        .execute(&mut c)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,'node-close-synthetic',NULL)")
        .bind(entity(81).encode())
        .bind(entity(80).encode())
        .execute(&mut c)
        .await
        .unwrap();
    c.close().await.unwrap();
}
fn current() -> CurrentRequestActor {
    CurrentRequestActor::new(
        ActorContext {
            client_id: entity(1),
            auth_generation: 12,
        },
        entity(81),
        NativeSessionKey::from_config(Some(entity(80)), "node-close-synthetic").unwrap(),
        Some(QueueAction::Submit),
    )
}
fn intent() -> RegisteredIntent {
    RegisteredIntent::new(
        entity(1),
        QueueAction::Submit,
        entity(81),
        None,
        None,
        Some([5; 32]),
    )
    .unwrap()
}
fn at() -> OperationContext {
    OperationContext {
        runtime: RuntimeEpoch(entity(90)),
        observed_at_ms: 0,
    }
}
fn limit() -> RegistryDeadline {
    RegistryDeadline::new(TokioInstant::now() + Duration::from_secs(20))
}
fn physical(path: &Path) {
    use std::os::windows::fs::OpenOptionsExt;
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .unwrap();
    drop(handle);
}
#[tokio::test]
async fn reg_close_node_01_real_shutdown_reports_unconfirmed_and_preserves_complete_control() {
    for lose_report in [false, true] {
        let area = TestArea::new();
        let protector = Arc::new(FakeProtector::default());
        let runtime = NodeRuntime::start(area.config(free_address()), protector.clone())
            .await
            .unwrap();
        let path = runtime.data_dir.as_ref().unwrap().database_path();
        seed(&path).await;
        let registry = runtime.storage.as_ref().unwrap().request_registry();
        if lose_report {
            registry
                .connection_closer_for_test()
                .fail_next_result_after_ack()
                .unwrap();
        }
        let fact = registry
            .register(
                &current(),
                &intent(),
                RegistrationId::new(entity(100)),
                &at(),
                &limit(),
            )
            .await
            .unwrap();
        assert_eq!(fact.id(), RegistrationId::new(entity(100)));
        let mut report = runtime.control.shutdown_receiver();
        let result = runtime.shutdown(None).await;
        physical(&path);
        if lose_report {
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("request registry SQLite connection closure could not be confirmed")
            );
            assert!(!error.contains("synthetic-close-client"));
            assert!(!error.contains("node-close-synthetic"));
            assert!(
                matches!(&*report.borrow_and_update(),ShutdownResult::Failed(message) if message.contains("request registry SQLite connection closure could not be confirmed"))
            );
        } else {
            result.unwrap();
            assert!(matches!(
                &*report.borrow_and_update(),
                ShutdownResult::Completed
            ));
        }
        drop(registry);
    }
}
#[tokio::test]
async fn reg_close_node_02_real_pending_sql_close_cannot_publish_a_successful_stop() {
    let area = TestArea::new();
    let protector = Arc::new(FakeProtector::default());
    let runtime = NodeRuntime::start(area.config(free_address()), protector.clone())
        .await
        .unwrap();
    let path = runtime.data_dir.as_ref().unwrap().database_path();
    seed(&path).await;
    let registry = runtime.storage.as_ref().unwrap().request_registry();
    let controller = registry.connection_closer_for_test();
    let gate = CloseBarrier::new();
    controller
        .pause_next(ClosePoint::BeforeClose, gate.clone())
        .unwrap();
    let owned = registry.clone();
    let io = tokio::spawn(async move {
        owned
            .register(
                &current(),
                &intent(),
                RegistrationId::new(entity(100)),
                &at(),
                &limit(),
            )
            .await
    });
    timeout(Duration::from_secs(20), gate.arrived())
        .await
        .unwrap();
    let after = CloseBarrier::new();
    controller
        .pause_next(ClosePoint::AfterCloseAck, after.clone())
        .unwrap();
    let mut report = runtime.control.shutdown_receiver();
    let result = runtime
        .shutdown_with_deadline(None, Duration::from_millis(250))
        .await;
    assert!(result.unwrap_err().to_string().contains("closing storage"));
    assert!(
        matches!(&*report.borrow_and_update(),ShutdownResult::Failed(message) if message.contains("closing storage"))
    );
    gate.release();
    timeout(Duration::from_secs(20), after.arrived())
        .await
        .unwrap();
    after.release();
    assert!(
        timeout(Duration::from_secs(20), io)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    physical(&path);
    drop(registry);
}
