//! A real close-ack report loss belongs to this transaction, not the aggregate shutdown phase.
use super::*;
use crate::{
    application::request_registry::RegistryDeadline,
    session_storage::registry_connection_owner::{
        ConnectionCloseStatus,
        test_hooks::{CloseBarrier, ClosePoint},
    },
};
use std::time::Duration;
use tokio::time::{Instant, timeout};

#[tokio::test]
async fn native_transaction_close_ack_loss_is_unconfirmed_while_registry_unsealed() {
    let nonce = crate::application::request_registry::random_id()
        .unwrap()
        .encode();
    let area = std::env::temp_dir().join(format!(
        "acpr-native-close-dev-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&area).unwrap();
    let roots = std::sync::Arc::new((nonce, area));
    let area = &roots.1;
    let storage_scope = crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(
        &area.join("node.sqlite3"),
        roots.clone(),
    );
    let storage = storage_scope.open().await.unwrap();
    let registry = storage.request_registry();
    let repository = registry.native_dispatch_repository();
    let deadline = RegistryDeadline::new(Instant::now() + Duration::from_secs(5));
    let mut connection = repository.begin(&deadline).await.unwrap();
    sqlx::query("INSERT INTO product_sources(source_id,name) VALUES(?,'synthetic close outcome')")
        .bind(EntityId::from_bytes([37; 16]).encode())
        .execute(connection.connection().unwrap())
        .await
        .unwrap();
    let controller = registry.connection_closer_for_test();
    let barrier = CloseBarrier::new();
    controller
        .pause_next(ClosePoint::AfterCloseAck, barrier.clone())
        .unwrap();
    let worker = repository.clone();
    let operation =
        tokio::spawn(async move { worker.finish(connection, Ok(37_i64), &deadline).await });
    timeout(Duration::from_secs(3), barrier.arrived())
        .await
        .unwrap();
    assert_eq!(
        registry.close_status(),
        ConnectionCloseStatus::Pending,
        "unsealed aggregate is not a transaction outcome"
    );
    controller.fail_next_result_after_ack().unwrap();
    barrier.release();
    assert!(matches!(
        timeout(Duration::from_secs(3), operation)
            .await
            .unwrap()
            .unwrap(),
        Err(DispatchError::SqlCloseUnconfirmed {
            commit_confirmed: true
        })
    ));
    let deadline = RegistryDeadline::new(Instant::now() + Duration::from_secs(4));
    let mut connection = repository.begin(&deadline).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM product_sources WHERE source_id=?")
        .bind(EntityId::from_bytes([37; 16]).encode())
        .fetch_one(connection.connection().unwrap())
        .await
        .unwrap();
    assert_eq!(
        repository
            .finish(connection, Ok(count), &deadline)
            .await
            .unwrap(),
        1,
        "the prior COMMIT was durable and this new physical close has its own result"
    );
    timeout(Duration::from_secs(5), storage.close())
        .await
        .unwrap();
    storage_scope.stop_and_observe().await;
    assert_eq!(
        registry.close_status(),
        ConnectionCloseStatus::Unconfirmed,
        "a lost close report remains globally unconfirmed even after later SQL succeeds"
    );
    std::fs::remove_dir_all(area).unwrap();
}
