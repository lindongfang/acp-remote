use super::*;
use crate::SqliteConnectOptions;
use futures_executor::block_on;
use sqlx_core::connection::{ConnectOptions, Connection};
use std::{future::Future, path::PathBuf, sync::atomic::AtomicUsize, time::Duration};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);
struct FileFixture {
    root: PathBuf,
}
impl FileFixture {
    fn new() -> Self {
        let ordinal = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("acp-worker-dev-{}-{ordinal}", std::process::id()));
        std::fs::create_dir(&root).expect("fresh test-owned directory");
        Self { root }
    }
    fn options(&self, registry: &SqliteWorkerRegistry) -> SqliteConnectOptions {
        SqliteConnectOptions::new()
            .filename(self.root.join("test.sqlite3"))
            .create_if_missing(true)
            .worker_registry(registry.clone())
    }
    fn finish(self) {
        std::fs::remove_dir_all(self.root).expect("remove only test-owned files after actual join");
    }
}
fn join_and_finalize(registry: &SqliteWorkerRegistry) -> RegistrySnapshot {
    let snapshot = registry.seal().expect("seal original scope");
    for worker in snapshot.workers {
        if worker.spawn == WorkerSpawn::Installed {
            assert_eq!(
                registry.join_worker_blocking(worker.id),
                Ok(WorkerJoinReport::Ok)
            );
        }
    }
    registry.finalize().expect("complete actual terminal set")
}
#[test]
fn actual_worker_close_and_join_are_separate() {
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    let options = file.options(&registry);
    let mut connection = block_on(options.connect()).expect("actual original file-backed worker");
    block_on(
        sqlx_core::query::query("CREATE TABLE sentinel(value INTEGER NOT NULL)")
            .execute(&mut connection),
    )
    .expect("actual SQLite SQL");
    block_on(sqlx_core::query::query("INSERT INTO sentinel VALUES(41)").execute(&mut connection))
        .expect("actual insert");
    let registration = connection
        .worker_registration()
        .expect("same original registration");
    block_on(connection.close()).expect("original SQL close");
    let before = registration.snapshot().expect("snapshot");
    assert!(before.shutdown.ack_observed && before.close.graceful_returned_ok);
    assert_eq!(before.handle, WorkerHandleState::VendorOwned);
    assert!(registry.finalize().is_err());
    registry.seal().expect("seal");
    assert_eq!(
        registry.join_worker_blocking(registration.id()),
        Ok(WorkerJoinReport::Ok)
    );
    assert_eq!(
        registry.join_worker_blocking(registration.id()),
        Ok(WorkerJoinReport::Ok)
    );
    let final_snapshot = registry.finalize().expect("real terminal set");
    assert!(final_snapshot.finalized && final_snapshot.all_threads_terminated());
    assert!(registry.finalize().expect("idempotent").finalized);
    file.finish();
}
#[test]
fn seal_first_has_no_worker() {
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    registry.seal().expect("seal");
    assert!(block_on(file.options(&registry).connect()).is_err());
    let snapshot = registry.finalize().expect("empty sealed domain");
    assert!(snapshot.workers.is_empty());
    assert!(!file.root.join("test.sqlite3").exists());
    file.finish();
}
#[test]
fn dropped_connection_still_requires_original_join() {
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    let connection = block_on(file.options(&registry).connect()).expect("real worker");
    let id = connection.worker_registration().expect("registration").id();
    drop(connection);
    registry.seal().expect("seal");
    assert_eq!(registry.join_worker_blocking(id), Ok(WorkerJoinReport::Ok));
    let snapshot = registry.finalize().expect("actual join");
    assert!(!snapshot.workers[0].shutdown.ack_observed);
    assert_eq!(
        snapshot.workers[0].exit_path,
        WorkerExitPath::CommandChannelClosed
    );
    file.finish();
}
#[test]
fn ack_observed_while_original_thread_is_still_blocked() {
    use test_control::{Action, Point, Selection, TestController, TestPlan};
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    let point = Selection {
        worker: 1,
        point: Point::AfterShutdownAckBeforeThreadReturn,
        hit: 1,
    };
    let controller = TestController::install(
        &registry,
        TestPlan {
            rules: vec![(point, Action::Pause)],
        },
    );
    let connection = block_on(file.options(&registry).connect()).expect("actual worker");
    let registration = connection
        .worker_registration()
        .expect("actual registration");
    block_on(connection.close()).expect("ACK observed before original thread return");
    assert!(controller.wait_reached(point, Duration::from_secs(5)));
    let snapshot = registration.snapshot().expect("original slot");
    assert!(snapshot.shutdown.ack_observed);
    assert_eq!(snapshot.handle, WorkerHandleState::VendorOwned);
    assert!(!snapshot.thread_is_finished);
    controller.release(point);
    assert!(join_and_finalize(&registry).all_threads_terminated());
    file.finish();
}
#[test]
fn original_spawn_boundary_error_records_no_thread() {
    use test_control::{Action, Point, Selection, TestController, TestPlan};
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    let point = Selection {
        worker: 1,
        point: Point::AfterReserveBeforeSpawn,
        hit: 1,
    };
    let _controller = TestController::install(
        &registry,
        TestPlan {
            rules: vec![(point, Action::SpawnIoErrorOnce)],
        },
    );
    let error = block_on(file.options(&registry).connect())
        .expect_err("explicit OS-boundary injected failure");
    assert!(matches!(error, sqlx_core::error::Error::Io(_)));
    let snapshot = registry.seal().expect("seal");
    assert_eq!(snapshot.workers.len(), 1);
    assert_eq!(snapshot.workers[0].spawn, WorkerSpawn::SpawnFailedNoThread);
    assert_eq!(snapshot.workers[0].handle, WorkerHandleState::NotInstalled);
    assert!(snapshot.workers[0].spawn_error_kind.is_some());
    assert!(snapshot.workers[0].join_report.is_none());
    assert!(registry
        .finalize()
        .expect("no thread to join")
        .all_threads_terminated());
    assert!(!file.root.join("test.sqlite3").exists());
    file.finish();
}
#[test]
fn cancelled_connect_retains_the_original_worker() {
    use futures_util::{future::poll_fn, pin_mut};
    use std::task::Poll;
    use test_control::{Action, Point, Selection, TestController, TestPlan};
    let file = FileFixture::new();
    let registry = SqliteWorkerRegistry::new();
    let point = Selection {
        worker: 1,
        point: Point::WorkerBeforeEstablish,
        hit: 1,
    };
    let controller = TestController::install(
        &registry,
        TestPlan {
            rules: vec![(point, Action::Pause)],
        },
    );
    let options = file.options(&registry);
    {
        let connection = options.connect();
        pin_mut!(connection);
        block_on(poll_fn(|cx| {
            assert!(connection.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        }));
    }
    assert!(controller.wait_reached(point, Duration::from_secs(5)));
    let before = registry
        .snapshot()
        .expect("scope survives local future cancellation");
    assert_eq!(before.workers[0].handle, WorkerHandleState::VendorOwned);
    controller.release(point);
    let after = join_and_finalize(&registry);
    assert_eq!(
        after.workers[0].exit_path,
        WorkerExitPath::EstablishUndelivered
    );
    assert!(!after.workers[0].shutdown.ack_observed);
    file.finish();
}
