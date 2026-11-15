//! Controlled original `TrustStore` transactions; hooks never replace SQLite work.
#![allow(
    clippy::option_if_let_else,
    clippy::ref_option,
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;
use futures_util::FutureExt;
use std::os::windows::fs::OpenOptionsExt;
use std::{
    future::{Future, poll_fn},
    panic::AssertUnwindSafe,
    pin::pin,
    task::Poll,
};
use tokio::{sync::Notify, task::JoinHandle, time::timeout};

struct Plan {
    first: String,
    second: String,
    pause: crate::test_support::SharedAsyncBarrier,
    pause_used: AtomicBool,
    second_pending: AtomicBool,
    pending: Notify,
}
static PLAN: Mutex<Option<Arc<Plan>>> = Mutex::new(None);
struct Installed(Arc<Plan>);
impl Drop for Installed {
    fn drop(&mut self) {
        self.0.pause.release();
        let mut slot = PLAN.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.as_ref().is_some_and(|p| Arc::ptr_eq(p, &self.0)) {
            *slot = None;
        }
    }
}
fn current(id: &str) -> Option<Arc<Plan>> {
    PLAN.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .filter(|p| p.first == id || p.second == id)
        .cloned()
}
pub(in crate::trust) async fn observe_begin<F: Future>(id: &str, original: F) -> F::Output {
    let mut original = pin!(original);
    poll_fn(|cx| {
        let actual = original.as_mut().poll(cx);
        if matches!(&actual, Poll::Pending)
            && let Some(p) = current(id).filter(|p| p.second == id)
        {
            p.second_pending.store(true, Ordering::SeqCst);
            p.pending.notify_one();
        }
        actual
    })
    .await
}
pub(in crate::trust) async fn after_snapshot(id: &str) {
    if let Some(p) = current(id).filter(|p| p.first == id)
        && !p.pause_used.swap(true, Ordering::SeqCst)
    {
        p.pause.pause().await;
    }
}
fn install(first: String, second: String) -> Installed {
    let p = Arc::new(Plan {
        first,
        second,
        pause: Arc::new(crate::test_support::AsyncBarrier::default()),
        pause_used: AtomicBool::new(false),
        second_pending: AtomicBool::new(false),
        pending: Notify::new(),
    });
    let mut slot = PLAN.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(slot.is_none(), "one owned diagnostic plan");
    *slot = Some(p.clone());
    Installed(p)
}
fn new_client() -> String {
    Id128::new(crate::random::bytes().expect("fixture random id")).to_base64url()
}
fn public(byte: u8) -> Vec<u8> {
    p256::ecdsa::SigningKey::from_slice(&[byte; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false)
        .as_bytes()
        .to_vec()
}
async fn initialize(storage: &Storage) {
    let key = public(0x39);
    storage
        .write_node_meta(&crate::storage::NodeMeta {
            schema_version: crate::storage::SCHEMA_VERSION,
            node_id: hex::encode(Sha256::digest(&key)),
            public_key: key,
            key_file_ref: "pair-concurrency-fixture-only.key".into(),
        })
        .await
        .expect("original node metadata");
}
type Registration = Result<TrustedClientRecord, TrustError>;
fn launch(trust: TrustStore, id: String, key: Vec<u8>) -> JoinHandle<Registration> {
    tokio::spawn(async move {
        trust
            .register_client(&id, &key, "controlled concurrent client")
            .await
    })
}
fn database_code(result: &Option<Registration>) -> Option<String> {
    match result.as_ref() {
        Some(Err(TrustError::Database(sqlx::Error::Database(error)))) => {
            error.code().map(std::borrow::Cow::into_owned)
        }
        _ => None,
    }
}
async fn settle(task: &mut Option<JoinHandle<Registration>>) -> Option<Registration> {
    let handle = task.as_mut()?;
    match timeout(Duration::from_secs(30), handle).await {
        Ok(result) => {
            task.take();
            result.ok()
        }
        Err(_) => None,
    }
}
async fn drain(task: &mut Option<JoinHandle<Registration>>) -> Option<Registration> {
    let result = task.as_mut()?.await;
    task.take();
    result.ok()
}
async fn close(path: &TestDatabase, storage: crate::sqlite_fixture_scope::FixtureStorage) -> bool {
    let registry = storage.request_registry();
    storage.close().await;
    let scopes = std::mem::take(&mut *path.scopes.lock().expect("own fixture scopes"));
    let mut joined = true;
    for scope in scopes {
        joined &= scope.stop_and_observe().await;
    }
    let physical = registry.close_status().is_complete() && storage.pool_for_test().size() == 0;
    let exclusive = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path.as_ref());
    let file_closed = exclusive.is_ok();
    drop(exclusive);
    joined && physical && file_closed
}

#[tokio::test]
async fn pairing_registration_serial_control_uses_original_commits() {
    let path = test_database();
    let storage = path.open().await.expect("original fixture storage");
    initialize(&storage).await;
    let trust = TrustStore::new(storage.request_registry());
    let first = trust
        .register_client(&new_client(), &public(0x41), "control a")
        .await;
    let second = trust
        .register_client(&new_client(), &public(0x42), "control b")
        .await;
    let count: Result<i64, sqlx::Error> = sqlx::query_scalar("SELECT count(*) FROM clients")
        .fetch_one(storage.pool_for_test())
        .await;
    let audits: Result<i64, sqlx::Error> = sqlx::query_scalar(
        "SELECT count(*) FROM security_audit WHERE action='device.pair.approve'",
    )
    .fetch_one(storage.pool_for_test())
    .await;
    let revision = trust.current_revision().await;
    drop(trust);
    let clean = close(&path, storage).await;
    if clean
        && first.is_ok()
        && second.is_ok()
        && count.as_ref().ok() == Some(&2)
        && audits.as_ref().ok() == Some(&2)
        && revision.as_ref().ok() == Some(&2)
    {
        path.roots
            .remove_owned()
            .expect("successful fixture cleanup");
    }

    assert!(
        clean,
        "original scope ACK/join, pool0 and first exclusive file open"
    );
    assert!(first.is_ok() && second.is_ok());
    assert_eq!(
        (count.unwrap(), audits.unwrap(), revision.unwrap()),
        (2, 2, 2)
    );
}

#[tokio::test]
async fn pairing_registration_concurrent_writer_preserves_both_clients() {
    let path = test_database();
    let storage = path.open().await.expect("original fixture storage");
    initialize(&storage).await;
    let trust = TrustStore::new(storage.request_registry());
    let first = new_client();
    let second = new_client();
    let plan = install(first.clone(), second.clone());
    let mut a = None;
    let mut b = None;
    let mut a_result = None;
    let mut b_result = None;
    let overlap_commit = std::env::var("PAIR_REPRO_MODE").as_deref() == Ok("overlap_commit");
    let outcome = AssertUnwindSafe(async {
        a = Some(launch(trust.clone(), first.clone(), public(0x43)));
        timeout(Duration::from_secs(10), plan.0.pause.wait_until_entered())
            .await
            .expect("first original SELECT reached");
        b = Some(launch(trust.clone(), second.clone(), public(0x44)));
        if overlap_commit {
            // Explicit causal diagnostic on the old deferred transaction: B truly commits
            // while A's original read snapshot remains held. Never use this schedule with IMMEDIATE.
            let task = b.as_mut().unwrap();
            let joined = timeout(Duration::from_secs(10), task)
                .await
                .expect("second original commit under old read snapshot")
                .expect("second task");
            b.take();
            b_result = Some(joined);
            assert!(
                b_result.as_ref().unwrap().is_ok(),
                "second real transaction commits"
            );
        } else {
            // The fixed writer schedule waits only for the actual original BEGIN future
            // to have been polled Pending; it never requires B to commit while A owns the writer lock.
            timeout(Duration::from_secs(10), plan.0.pending.notified())
                .await
                .expect("second original begin actually Pending");
            assert!(plan.0.second_pending.load(Ordering::SeqCst));
        }
        plan.0.pause.release();
        a_result = settle(&mut a).await;
        if b_result.is_none() {
            b_result = settle(&mut b).await;
        }
    })
    .catch_unwind()
    .await;
    // Original operation ownership survives all assertions, timeouts and panics above.
    plan.0.pause.release();
    if a_result.is_none() {
        a_result = drain(&mut a).await;
    }
    if b_result.is_none() {
        b_result = drain(&mut b).await;
    }
    let code_a = database_code(&a_result);
    let code_b = database_code(&b_result);
    let count_a: Result<i64, sqlx::Error> =
        sqlx::query_scalar("SELECT count(*) FROM clients WHERE client_id=?")
            .bind(&first)
            .fetch_one(storage.pool_for_test())
            .await;
    let count_b: Result<i64, sqlx::Error> =
        sqlx::query_scalar("SELECT count(*) FROM clients WHERE client_id=?")
            .bind(&second)
            .fetch_one(storage.pool_for_test())
            .await;
    let audits: Result<i64, sqlx::Error> = sqlx::query_scalar(
        "SELECT count(*) FROM security_audit WHERE action='device.pair.approve'",
    )
    .fetch_one(storage.pool_for_test())
    .await;
    let revision = trust.current_revision().await;
    let a_ok = a_result.as_ref().is_some_and(Result::is_ok);
    let b_ok = b_result.as_ref().is_some_and(Result::is_ok);
    println!(
        "PAIR_REPRO phase=settled mode={} a_ok={} b_ok={} sqlite_a={:?} sqlite_b={:?} a_rows={:?} b_rows={:?} audits={:?} revision={:?}",
        if overlap_commit {
            "overlap_commit"
        } else {
            "serialize"
        },
        a_ok,
        b_ok,
        code_a,
        code_b,
        count_a.as_ref().ok(),
        count_b.as_ref().ok(),
        audits.as_ref().ok(),
        revision.as_ref().ok()
    );
    drop(plan);
    drop(trust);
    let clean = close(&path, storage).await;
    if clean
        && outcome.is_ok()
        && a_ok
        && b_ok
        && count_a.as_ref().ok() == Some(&1)
        && count_b.as_ref().ok() == Some(&1)
        && audits.as_ref().ok() == Some(&2)
        && revision.as_ref().ok() == Some(&2)
    {
        path.roots
            .remove_owned()
            .expect("successful fixture cleanup");
    } else {
        println!("PAIR_REPRO retained_fixture=true");
    }

    assert!(
        clean,
        "original scope ACK/join, pool0 and first exclusive file open"
    );
    assert!(
        outcome.is_ok(),
        "controlled schedule failed; original tasks already drained"
    );
    assert!(
        a_ok && b_ok,
        "both legitimate original registrations must commit"
    );
    assert_eq!(
        (
            count_a.unwrap(),
            count_b.unwrap(),
            audits.unwrap(),
            revision.unwrap()
        ),
        (1, 1, 2, 2)
    );
}
