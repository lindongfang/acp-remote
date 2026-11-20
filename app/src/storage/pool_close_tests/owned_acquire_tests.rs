//! Development regressions only. Independent CLDB01閳?7 remain unexecuted.
use super::*;
use crate::{
    domain::request_registry::RegistryError,
    session_storage::{
        AcquisitionCloseProgress as CloseProgress, ConnectionCloseStatus,
        request_registry::connection_close_test_hooks::{CloseBarrier, ClosePoint},
    },
};
use std::sync::{Condvar, Mutex};
use tokio::time::timeout;

async fn pending_once<F: Future>(future: std::pin::Pin<&mut F>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

fn original_acquisition(storage: &Storage) -> Arc<crate::session_storage::AcquisitionObservation> {
    let slots = storage.request_registry().acquisitions_for_test();
    assert_eq!(
        slots.len(),
        1,
        "the actual original acquisition is registered"
    );
    slots.into_iter().next().expect("one acquisition")
}

#[tokio::test]
async fn cancelled_registry_acquire_retains_original_task_before_attach() {
    let area = Arc::new(OwnedDatabase::create());
    let (host, ticket) = prepare_storage(&area, SqlitePoolOptions::new().max_connections(1));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    let registry = storage.request_registry();
    let gate = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::AfterAcquire, gate.clone())
        .expect("install");
    let mut caller = Box::pin(registry.detached_owned_connection());
    pending_once(caller.as_mut()).await;
    timeout(Duration::from_secs(2), gate.arrived())
        .await
        .expect("real acquire succeeded before attach");
    let original = original_acquisition(&storage);
    assert!(!original.snapshot().transferred);
    drop(caller);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    assert!(!original.snapshot().acquisition_joined);
    drop(close);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert!(!original.snapshot().acquisition_joined);
    gate.release();
    timeout(Duration::from_secs(2), close)
        .await
        .expect("same acquire and close joined");
    let observed = original.snapshot();
    assert!(observed.transferred && observed.acquisition_joined);
    assert_eq!(observed.close, CloseProgress::ConfirmedJoined);
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Complete
    );
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn trust_scope_cancel_uses_same_storage_acquisition_and_original_close() {
    let area = Arc::new(OwnedDatabase::create());
    let (host, ticket) = prepare_storage(&area, SqlitePoolOptions::new().max_connections(1));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    let registry = storage.request_registry();
    let gate = CloseBarrier::new();
    registry
        .connection_closer_for_test()
        .pause_next(ClosePoint::AfterAcquire, gate.clone())
        .expect("install");
    let trust = storage.trust_store();
    let scope = registry.owner_scope();
    let caller = tokio::spawn(async move {
        scope
            .while_open(async {
                trust
                    .client_by_id("missing-client")
                    .await
                    .map_err(|_| RegistryError::StorageUnavailable)
            })
            .await
    });
    timeout(Duration::from_secs(2), gate.arrived())
        .await
        .expect("real Trust acquire before attach");
    let original = original_acquisition(&storage);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(
        caller.await.expect("caller joined"),
        Err(RegistryError::NodeStopping)
    );
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    gate.release();
    timeout(Duration::from_secs(2), close)
        .await
        .expect("same Storage closes Trust worker");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert!(original.snapshot().acquisition_joined);
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    area.cleanup();
}

#[tokio::test]
async fn cancelled_trust_transaction_keeps_original_connection_close_receipt() {
    let area = Arc::new(OwnedDatabase::create());
    let (host, ticket) = prepare_storage(&area, SqlitePoolOptions::new().max_connections(1));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    let trust = storage.trust_store();
    let transaction_gate = trust.pause_next_list_page();
    let caller = tokio::spawn(async move { trust.list_page("actor", None).await });
    timeout(
        Duration::from_secs(2),
        transaction_gate.wait_until_entered(),
    )
    .await
    .expect("transaction queried revision");
    let original = original_acquisition(&storage);
    let close_gate = CloseBarrier::new();
    storage
        .request_registry()
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close_gate.clone())
        .expect("install");
    caller.abort();
    assert!(
        caller
            .await
            .expect_err("only caller cancellation")
            .is_cancelled()
    );
    timeout(Duration::from_secs(2), close_gate.arrived())
        .await
        .expect("original connection close");
    assert_eq!(original.snapshot().close, CloseProgress::Pending);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    close_gate.release();
    timeout(Duration::from_secs(2), close)
        .await
        .expect("rollback and original close ACK");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    area.cleanup();
}

struct WorkerGate {
    armed: AtomicBool,
    entered: Notify,
    released: Mutex<bool>,
    changed: Condvar,
}
impl WorkerGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            armed: AtomicBool::new(false),
            entered: Notify::new(),
            released: Mutex::new(false),
            changed: Condvar::new(),
        })
    }
    fn block(&self) {
        self.entered.notify_one();
        let mut released = self.released.lock().expect("owned worker gate");
        while !*released {
            released = self.changed.wait(released).expect("owned worker gate");
        }
        drop(released);
    }
    fn release(&self) {
        *self.released.lock().expect("owned worker gate") = true;
        self.changed.notify_all();
    }
}
struct ReleaseWorker(Arc<WorkerGate>);
impl Drop for ReleaseWorker {
    fn drop(&mut self) {
        self.0.release();
    }
}
const WORKER_SQL: &str = "SELECT value FROM (SELECT 'a' AS value UNION ALL SELECT 'b') ORDER BY value COLLATE owned_acquire_gate";

fn worker_options(
    gate: &Arc<WorkerGate>,
    fresh: bool,
    acquire_timeout: Duration,
) -> SqlitePoolOptions {
    let connect = gate.clone();
    let before = gate.clone();
    SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .acquire_timeout(acquire_timeout)
        .after_connect(move |connection, _| {
            let gate = connect.clone();
            Box::pin(async move {
                let callback = gate.clone();
                connection.lock_handle().await?.create_collation(
                    "owned_acquire_gate",
                    move |a, b| {
                        callback.block();
                        a.cmp(b)
                    },
                )?;
                if fresh && gate.armed.load(Ordering::SeqCst) {
                    connection.execute(WORKER_SQL).await?;
                }
                Ok(())
            })
        })
        .before_acquire(move |connection, _| {
            let gate = before.clone();
            Box::pin(async move {
                if !fresh && gate.armed.load(Ordering::SeqCst) {
                    connection.execute(WORKER_SQL).await?;
                }
                Ok(true)
            })
        })
}

async fn real_worker_cancellation(fresh: bool) {
    let area = Arc::new(OwnedDatabase::create());
    let gate = WorkerGate::new();
    let release = ReleaseWorker(gate.clone());
    let (host, ticket) =
        prepare_storage(&area, worker_options(&gate, fresh, Duration::from_secs(5)));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    if fresh {
        drop(
            storage
                .request_registry()
                .detached_owned_connection()
                .await
                .expect("drain idle"),
        );
        assert_eq!(
            storage.pool_for_test().size(),
            0,
            "fresh path has no idle physical connection"
        );
    }
    gate.armed.store(true, Ordering::SeqCst);
    let registry = storage.request_registry();
    let mut caller = Box::pin(registry.detached_owned_connection());
    pending_once(caller.as_mut()).await;
    timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .expect("actual SQLite worker callback");
    let original = original_acquisition(&storage);
    drop(caller);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    assert_eq!(original.snapshot().close, CloseProgress::NotStarted);
    assert!(
        !area.snapshot(storage.pool_for_test()).is_closed(),
        "original worker still owns real files"
    );
    gate.release();
    timeout(Duration::from_secs(2), close)
        .await
        .expect("real worker close ACK and acquisition join");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert!(original.snapshot().acquisition_joined);
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    drop(release);
    area.cleanup();
}

#[tokio::test]
async fn real_idle_worker_acquire_survives_caller_cancel() {
    real_worker_cancellation(false).await;
}
#[tokio::test]
async fn real_new_worker_acquire_survives_caller_cancel() {
    real_worker_cancellation(true).await;
}

#[tokio::test]
async fn sqlx_internal_timeout_with_real_worker_remains_unconfirmed() {
    let area = Arc::new(OwnedDatabase::create());
    let gate = WorkerGate::new();
    let release = ReleaseWorker(gate.clone());
    let (host, ticket) = prepare_storage(
        &area,
        worker_options(&gate, false, Duration::from_millis(750)),
    );
    let storage = ticket.ready().await.expect("Storage").projection(host);
    gate.armed.store(true, Ordering::SeqCst);
    let registry = storage.request_registry();
    let caller = tokio::spawn(async move { registry.detached_owned_connection().await });
    timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .expect("actual SQLite worker callback");
    let original = original_acquisition(&storage);
    assert!(
        timeout(Duration::from_secs(3), caller)
            .await
            .expect("SQLx own acquire timeout")
            .expect("acquisition caller joined")
            .is_err()
    );
    timeout(Duration::from_secs(2), storage.close())
        .await
        .expect("unconfirmed shutdown settles");
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Unconfirmed
    );
    assert!(original.snapshot().acquisition_joined);
    assert_eq!(original.snapshot().close, CloseProgress::NotStarted);
    assert!(
        !area.snapshot(storage.pool_for_test()).is_closed(),
        "size zero cannot replace original worker ACK"
    );
    gate.release();
    storage.close().await;
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Unconfirmed,
        "sticky after repeat close"
    );
    drop(release);
    // No original SQLx receipt exists. Retain this owned diagnostic area; a later
    // successful physical probe must never wash this shutdown outcome green.
    eprintln!(
        "unconfirmed owned area retained: {}",
        area.directory.display()
    );
}

#[tokio::test]
async fn delivered_query_cancel_keeps_worker_and_original_close_receipt() {
    let area = Arc::new(OwnedDatabase::create());
    let gate = WorkerGate::new();
    let release = ReleaseWorker(gate.clone());
    let (host, ticket) =
        prepare_storage(&area, worker_options(&gate, false, Duration::from_secs(5)));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    let registry = storage.request_registry();
    let caller = tokio::spawn(async move {
        let mut connection = registry
            .detached_owned_connection()
            .await
            .expect("owned acquire delivered");
        connection
            .connection()
            .expect("original raw connection")
            .execute(WORKER_SQL)
            .await
    });
    timeout(Duration::from_secs(2), gate.entered.notified())
        .await
        .expect("query reached actual worker");
    let original = original_acquisition(&storage);
    assert!(original.snapshot().transferred);
    let close_gate = CloseBarrier::new();
    storage
        .request_registry()
        .connection_closer_for_test()
        .pause_next(ClosePoint::BeforeClose, close_gate.clone())
        .expect("install");
    caller.abort();
    assert!(
        caller
            .await
            .expect_err("caller cancellation")
            .is_cancelled()
    );
    timeout(Duration::from_secs(2), close_gate.arrived())
        .await
        .expect("original close registered");
    close_gate.release();
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
        .expect("query worker original close ACK and join");
    assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    drop(release);
    area.cleanup();
}

#[tokio::test]
async fn trust_and_registry_acquires_drain_one_permit_without_shutdown_cycle() {
    let area = Arc::new(OwnedDatabase::create());
    let (host, ticket) = prepare_storage(&area, SqlitePoolOptions::new().max_connections(1));
    let storage = ticket.ready().await.expect("Storage").projection(host);
    let blocker = storage
        .pool_for_test()
        .acquire()
        .await
        .expect("fixture owns only permit");
    let registry = storage.request_registry();
    let trust = storage.trust_store();
    let mut registry_caller = Box::pin(registry.detached_owned_connection());
    let mut trust_caller = Box::pin(trust.client_by_id("missing-client"));
    pending_once(registry_caller.as_mut()).await;
    pending_once(trust_caller.as_mut()).await;
    let originals = registry.acquisitions_for_test();
    assert_eq!(originals.len(), 2);
    drop(registry_caller);
    drop(trust_caller);
    let mut close = Box::pin(storage.close());
    pending_once(close.as_mut()).await;
    assert_eq!(
        storage.registry_close_status(),
        ConnectionCloseStatus::Pending
    );
    drop(blocker);
    timeout(Duration::from_secs(2), close)
        .await
        .expect("old acquisitions advance after original permit released");
    for original in originals {
        assert!(original.snapshot().acquisition_joined);
        assert_eq!(original.snapshot().close, CloseProgress::ConfirmedJoined);
    }
    assert!(area.snapshot(storage.pool_for_test()).is_closed());
    area.cleanup();
}

#[path = "trust_retention_tests.rs"]
mod trust_retention_tests;
