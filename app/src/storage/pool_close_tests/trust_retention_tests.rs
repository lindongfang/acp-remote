//! Development coverage for the finite Trust retention ABI, not independent CLDB.
use super::*;
use sqlx::Connection;

#[tokio::test]
async fn repeated_trust_scopes_share_one_original_physical_close() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let registry = storage.request_registry();
    let mut connection = registry
        .trust_owned_connection()
        .await
        .expect("Trust scope");
    connection.connection().expect("raw").execute("CREATE TEMP TABLE retained_identity(value INTEGER); INSERT INTO retained_identity VALUES(137)").await.expect("original worker identity");
    let first = original_acquisition(&storage);
    assert!(connection.retain_for_trust().is_ok());
    let trust = storage.trust_store();
    assert!(
        trust
            .client_by_id("missing")
            .await
            .expect("reliable None")
            .is_none()
    );
    let second = original_acquisition(&storage);
    assert!(
        trust
            .client_by_public_key(&[0; 65])
            .await
            .expect("reliable None")
            .is_none()
    );
    assert!(first.snapshot().acquisition_joined && second.snapshot().acquisition_joined);
    assert!(first.same_physical(&second));
    assert_eq!(first.snapshot().close, CloseProgress::NotStarted);
    assert_eq!(second.snapshot().close, CloseProgress::NotStarted);
    let mut connection = registry
        .trust_owned_connection()
        .await
        .expect("same physical take");
    let value: i64 = sqlx::query_scalar("SELECT value FROM retained_identity")
        .fetch_one(connection.connection().expect("raw"))
        .await
        .expect("same worker's TEMP state");
    assert_eq!(value, 137);
    assert!(connection.retain_for_trust().is_ok());
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("one original close");
    assert_eq!(first.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(second.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Complete
    );
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn registry_cannot_take_trust_idle_and_full_idle_returns_original() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let registry = storage.request_registry();
    let first = registry.trust_owned_connection().await.expect("first");
    let original = original_acquisition(&storage);
    let second = registry
        .trust_owned_connection()
        .await
        .expect("concurrent second");
    let other = original_acquisition(&storage);
    assert!(!original.same_physical(&other));
    assert!(first.retain_for_trust().is_ok());
    let gate = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, gate.clone())
        .expect("close gate");
    let returned = second
        .retain_for_trust()
        .expect_err("full idle returns complete original");
    drop(returned);
    timeout(Duration::from_secs(2), gate.arrived())
        .await
        .expect("second original closes");
    assert_eq!(original.snapshot().close, CloseProgress::NotStarted);
    assert_eq!(other.snapshot().close, CloseProgress::Pending);
    let registry_connection = registry
        .detached_owned_connection()
        .await
        .expect("registry gets its own worker");
    let registry_original = original_acquisition(&storage);
    assert!(!original.same_physical(&registry_original));
    let returned = registry_connection
        .retain_for_trust()
        .expect_err("registry scope cannot retain");
    drop(returned);
    gate.release();
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("all originals drain");
    for observation in [&original, &other, &registry_original] {
        assert_eq!(observation.snapshot().close, CloseProgress::ConfirmedJoined);
    }
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn seal_before_retain_returns_full_original_outside_fence() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let registry = storage.request_registry();
    let connection = registry
        .trust_owned_connection()
        .await
        .expect("Trust scope");
    let original = original_acquisition(&storage);
    let gate = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, gate.clone())
        .expect("gate");
    registry.seal_connections();
    let connection = connection
        .retain_for_trust()
        .expect_err("sealed returns original");
    assert_eq!(original.snapshot().close, CloseProgress::NotStarted);
    drop(connection);
    timeout(Duration::from_secs(2), gate.arrived())
        .await
        .expect("original close registered");
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    gate.release();
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("close");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn seal_before_idle_take_closes_cache_without_waiting_for_new_business() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let trust = storage.trust_store();
    trust.client_by_id("missing").await.expect("warm idle");
    let original = original_acquisition(&storage);
    let take = CloseBarrier::new();
    let registry = storage.request_registry();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeTrustTake, take.clone())
        .expect("take gate");
    let caller = tokio::spawn(async move { trust.client_by_id("missing").await });
    timeout(Duration::from_secs(2), take.arrived())
        .await
        .expect("registered before take");
    let next = original_acquisition(&storage);
    let close = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close.clone())
        .expect("close gate");
    registry.seal_connections();
    timeout(Duration::from_secs(2), close.arrived())
        .await
        .expect("idle independently closed");
    take.release();
    assert!(
        timeout(Duration::from_secs(2), caller)
            .await
            .expect("caller ends")
            .expect("join")
            .is_err()
    );
    close.release();
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("drain");
    assert!(next.snapshot().acquisition_joined);
    assert!(next.snapshot().transferred);
    assert!(
        !original.same_physical(&next),
        "sealed cache is never lent to a new task"
    );
    assert_eq!(next.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn idle_take_cancel_and_close_waiter_cancel_keep_original_link() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let trust = storage.trust_store();
    trust.client_by_id("missing").await.expect("warm");
    let original = original_acquisition(&storage);
    let take = CloseBarrier::new();
    let registry = storage.request_registry();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::AfterAcquire, take.clone())
        .expect("after take");
    let caller = tokio::spawn(async move { trust.client_by_id("missing").await });
    timeout(Duration::from_secs(2), take.arrived())
        .await
        .expect("original idle taken");
    let next = original_acquisition(&storage);
    let close = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close.clone())
        .expect("close gate");
    let mut first_close = Box::pin(storage.close());
    pending_once(first_close.as_mut()).await;
    drop(first_close);
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    caller.abort();
    assert!(caller.await.expect_err("caller cancelled").is_cancelled());
    take.release();
    timeout(Duration::from_secs(2), close.arrived())
        .await
        .expect("same original handed to close");
    assert!(original.same_physical(&next));
    assert_eq!(original.snapshot().close, CloseProgress::Pending);
    close.release();
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("reentered original owner");
    for observation in [&original, &next] {
        assert!(observation.snapshot().acquisition_joined);
        assert_eq!(observation.snapshot().close, CloseProgress::ConfirmedJoined);
    }
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

async fn cancelled_cached_worker(commit: bool) {
    let area = Arc::new(OwnedDatabase::create());
    let gate = WorkerGate::new();
    let release = ReleaseWorker(gate.clone());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope
        .open_with(worker_options(&gate, false, Duration::from_secs(5)))
        .await
        .expect("Storage");
    let registry = storage.request_registry();
    let connection = registry.trust_owned_connection().await.expect("warm");
    let original = original_acquisition(&storage);
    assert!(connection.retain_for_trust().is_ok());
    let (pending_send, pending_receive) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn(async move {
        let mut connection = registry
            .trust_owned_connection()
            .await
            .expect("same cached worker");
        if commit {
            let mut transaction = connection
                .connection()
                .expect("raw")
                .begin()
                .await
                .expect("transaction");
            let mut query = Box::pin(transaction.execute(WORKER_SQL));
            pending_once(query.as_mut()).await;
            drop(query);
            let mut commit = Box::pin(transaction.commit());
            pending_once(commit.as_mut()).await;
            let _ = pending_send.send(());
            commit.await
        } else {
            let _ = pending_send.send(());
            connection
                .connection()
                .expect("raw")
                .execute(WORKER_SQL)
                .await
                .map(|_| ())
        }
    });
    timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .expect("original worker SQL blocked");
    timeout(Duration::from_secs(2), pending_receive)
        .await
        .expect("caller reached SQL or queued COMMIT")
        .expect("pending signal");
    let next = original_acquisition(&storage);
    assert!(original.same_physical(&next));
    let close = CloseBarrier::new();
    storage
        .request_registry()
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close.clone())
        .expect("close gate");
    caller.abort();
    assert!(caller.await.expect_err("caller cancelled").is_cancelled());
    timeout(Duration::from_secs(2), close.arrived())
        .await
        .expect("original connection close handoff");
    close.release();
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(original.snapshot().close, CloseProgress::Pending);
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    gate.release();
    timeout(Duration::from_secs(2), close)
        .await
        .expect("original worker ACK and join");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(next.snapshot().close, CloseProgress::ConfirmedJoined);
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    drop(release);
    area.cleanup();
}

#[tokio::test]
async fn cached_original_query_cancel_keeps_actual_ack() {
    cancelled_cached_worker(false).await;
}
#[tokio::test]
async fn cached_original_commit_cancel_keeps_actual_ack() {
    cancelled_cached_worker(true).await;
}

#[tokio::test]
async fn storage_drop_seals_and_drains_retained_owner_cycle() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    storage
        .trust_store()
        .client_by_id("missing")
        .await
        .expect("retain");
    let original = original_acquisition(&storage);
    let registry = storage.request_registry();
    let pool = storage.pool.clone();
    let gate = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, gate.clone())
        .expect("gate");
    drop(storage);
    timeout(Duration::from_secs(2), gate.arrived())
        .await
        .expect("Drop drains idle into original close");
    assert_eq!(registry.close_status(), ConnectionCloseStatus::Pending);
    gate.release();
    timeout(Duration::from_secs(2), registry.close_connections())
        .await
        .expect("actual task join");
    pool.close().await;
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(registry.close_status(), ConnectionCloseStatus::Complete);
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn revoke_replay_ok_with_active_transaction_does_not_retain() {
    use crate::claims::Id128;
    use p256::ecdsa::SigningKey;
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let trust = storage.trust_store();
    let actor = Id128::new([0x71; 16]).to_base64url();
    let target = Id128::new([0x72; 16]).to_base64url();
    for (client, seed) in [(&actor, 0x71), (&target, 0x72)] {
        let key = SigningKey::from_slice(&[seed; 32]).expect("key");
        trust
            .register_client(
                client,
                key.verifying_key().to_sec1_point(false).as_bytes(),
                "fixture",
            )
            .await
            .expect("register");
    }
    let request = Id128::new([0x73; 16]).to_base64url();
    let expected = trust
        .revoke(&actor, &request, &target)
        .await
        .expect("committed revoke");
    let first = original_acquisition(&storage);
    let replay = trust
        .revoke(&actor, &request, &target)
        .await
        .expect("same replay result");
    let second = original_acquisition(&storage);
    assert_eq!(expected, replay);
    assert!(first.same_physical(&second));
    trust.current_revision().await.expect("new reliable scope");
    let next = original_acquisition(&storage);
    assert!(
        !first.same_physical(&next),
        "replay's transaction Drop requires original close"
    );
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("close");
    assert_eq!(first.snapshot().close, CloseProgress::ConfirmedJoined);
    assert_eq!(second.snapshot().close, CloseProgress::ConfirmedJoined);
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn unknown_close_report_survives_later_successful_trust_reuse() {
    let area = Arc::new(OwnedDatabase::create());
    let storage_scope =
        crate::sqlite_fixture_scope::SqliteFixtureScope::prepare(&area.database(), area.clone());
    let storage = storage_scope.open().await.expect("Storage");
    let registry = storage.request_registry();
    let cached = registry
        .trust_owned_connection()
        .await
        .expect("cached original");
    let original = original_acquisition(&storage);
    let other = registry
        .trust_owned_connection()
        .await
        .expect("other original");
    let other_original = original_acquisition(&storage);
    assert!(cached.retain_for_trust().is_ok());
    registry
        .connection_closer_for_test()
        .fail_next_result_after_ack()
        .expect("report failure");
    drop(
        other
            .retain_for_trust()
            .expect_err("full idle returns other original"),
    );
    assert!(
        timeout(Duration::from_secs(2), other_original.join_original_close())
            .await
            .expect("original unknown report joined before later success")
    );
    let trust = storage.trust_store();
    trust
        .client_by_id("missing")
        .await
        .expect("later reliable read succeeds");
    let reused = original_acquisition(&storage);
    assert!(original.same_physical(&reused));
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("all original tasks join");
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Unconfirmed
    );
    storage_scope.stop_and_observe().await;
    assert!(area.snapshot(&storage.pool).is_closed());
    area.cleanup();
}
