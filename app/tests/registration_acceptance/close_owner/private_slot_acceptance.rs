//! Independent infrastructure checks; these do not replace actual Storage/owner integration.
use super::{
    ConnectionCloseStatus, ConnectionOwner,
    test_hooks::{CloseBarrier, ClosePoint},
};
use sqlx::{Connection, sqlite::SqliteConnectOptions};
use std::{
    future::poll_fn,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::Poll,
};
use tokio::time::{Duration, timeout};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn area() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/q3-private-registry-close");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let child = root.join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&child).unwrap();
    let child = child.canonicalize().unwrap();
    assert_eq!(child.parent(), Some(root.as_path()));
    child
}
fn closed(path: &std::path::Path) {
    use std::os::windows::fs::OpenOptionsExt;
    drop(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)
            .unwrap(),
    );
}
fn cleanup(path: &std::path::Path) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../.tmp/q3-private-registry-close")
        .canonicalize()
        .unwrap();
    let actual = path.canonicalize().unwrap();
    assert_eq!(actual.parent(), Some(root.as_path()));
    assert_eq!(actual, root.join(path.file_name().unwrap()));
    std::fs::remove_dir_all(actual).unwrap();
}
#[tokio::test]
async fn reg_close_private_01_real_finished_slots_are_reaped_on_subsequent_lease() {
    let directory = area();
    let db = directory.join("slots.sqlite3");
    let owner = ConnectionOwner::new();
    for _ in 0..32 {
        let lease = owner.lease().unwrap();
        {
            let state = owner.state.lock().unwrap();
            assert_eq!(state.leases, 1);
            assert!(state.slots.is_empty());
            drop(state);
        }
        let c = sqlx::SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&db)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        let mut connection = lease.attach(c);
        sqlx::query("CREATE TABLE IF NOT EXISTS actual_slot_data(value INTEGER)")
            .execute(connection.connection().unwrap())
            .await
            .unwrap();
        connection.close().await;
        let next = owner.lease().unwrap();
        {
            let state = owner.state.lock().unwrap();
            assert_eq!(state.leases, 1);
            assert!(
                state.slots.is_empty(),
                "joined close slots accumulated across normal lease/reap"
            );
            drop(state);
        }
        drop(next);
    }
    owner.shutdown().await;
    assert!(matches!(owner.status(), ConnectionCloseStatus::Complete));
    {
        let state = owner.state.lock().unwrap();
        assert_eq!(state.leases, 0);
        assert!(state.slots.is_empty());
        drop(state);
    }
    closed(&db);
    let witness = Arc::downgrade(&owner);
    drop(owner);
    assert!(
        witness.upgrade().is_none(),
        "close bookkeeping retained an owner Arc cycle"
    );
    cleanup(&directory);
}
#[tokio::test]
async fn reg_close_private_02_pending_slot_survives_reap_seal_and_cancelled_shutdown_waiter() {
    let directory = area();
    let db = directory.join("pending.sqlite3");
    let owner = ConnectionOwner::new();
    let gate = CloseBarrier::new();
    owner
        .hooks()
        .pause_next(ClosePoint::BeforeClose, gate.clone())
        .unwrap();
    let lease = owner.lease().unwrap();
    let c = sqlx::SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&db)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    drop(lease.attach(c));
    timeout(Duration::from_secs(20), gate.arrived())
        .await
        .unwrap();
    {
        let state = owner.state.lock().unwrap();
        assert_eq!(state.leases, 0);
        assert_eq!(state.slots.len(), 1);
    }
    let next = owner.lease().unwrap();
    {
        let state = owner.state.lock().unwrap();
        assert_eq!(state.leases, 1);
        assert_eq!(
            state.slots.len(),
            1,
            "a pending close was reaped before actual join"
        );
    }
    drop(next);
    let mut first = Box::pin(owner.shutdown());
    assert!(matches!(
        poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await,
        Poll::Pending
    ));
    assert!(matches!(owner.status(), ConnectionCloseStatus::Pending));
    drop(first);
    assert!(owner.lease().is_err(), "sealed owner minted a new lease");
    {
        let state = owner.state.lock().unwrap();
        assert_eq!(state.slots.len(), 1);
    }
    gate.release();
    timeout(Duration::from_secs(20), owner.shutdown())
        .await
        .unwrap();
    {
        let state = owner.state.lock().unwrap();
        assert_eq!(state.leases, 0);
        assert!(state.slots.is_empty());
        drop(state);
    }
    assert!(matches!(owner.status(), ConnectionCloseStatus::Complete));
    closed(&db);
    cleanup(&directory);
}
#[tokio::test]
async fn reg_close_private_03_guard_drop_from_non_runtime_thread_still_has_captured_owner() {
    let directory = area();
    let db = directory.join("thread.sqlite3");
    let owner = ConnectionOwner::new();
    let c = sqlx::SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&db)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let connection = owner.lease().unwrap().attach(c);
    std::thread::spawn(move || drop(connection)).join().unwrap();
    timeout(Duration::from_secs(20), owner.shutdown())
        .await
        .unwrap();
    assert!(matches!(owner.status(), ConnectionCloseStatus::Complete));
    closed(&db);
    cleanup(&directory);
}
