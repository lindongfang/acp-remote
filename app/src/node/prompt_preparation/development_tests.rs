use super::{
    FakeProtector, Id128, NodeRuntime, SigningKey, TestArea, free_address, ipc, pair_browser,
    policy_actor,
};
use crate::{
    adapters::acp::native_dispatch::AcpNativeAdapter,
    application::{
        prompt_preparation::*,
        queue_owner::{BlockedEnvironment, RetainPrompt, RuntimeSupervisor, validation_test_hooks},
        request_registry::{RegistryDeadline, RequestAuthority, SystemNodeClock},
    },
    node::prompt_preparation::{
        test_hooks::{Hooks, Point},
        *,
    },
};
use sha2::{Digest, Sha256};
use std::os::windows::fs::OpenOptionsExt;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Notify, time::Instant};
async fn poll_once<F: std::future::Future>(
    mut f: std::pin::Pin<&mut F>,
) -> std::task::Poll<F::Output> {
    std::future::poll_fn(|cx| std::task::Poll::Ready(f.as_mut().poll(cx))).await
}
const UPLOAD: &[u8] = b" \n[{\"type\":\"text\",\"text\":\"owner-canary\"}] ";
struct Pause {
    entered: AtomicBool,
    changed: Notify,
    released: Mutex<bool>,
    cv: Condvar,
}
impl Pause {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: AtomicBool::new(false),
            changed: Notify::new(),
            released: Mutex::new(false),
            cv: Condvar::new(),
        })
    }
    fn stop(&self) {
        self.entered.store(true, Ordering::Release);
        self.changed.notify_one();
        let mut g = self.released.lock().unwrap();
        while !*g {
            g = self.cv.wait(g).unwrap();
        }
        drop(g);
    }
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.cv.notify_all();
    }
    async fn reached(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let n = self.changed.notified();
                tokio::pin!(n);
                n.as_mut().enable();
                if self.entered.load(Ordering::Acquire) {
                    break;
                }
                n.await;
            }
        })
        .await
        .unwrap();
    }
}
struct Release(Arc<Pause>);
impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}
fn paused(point: Point) -> (Hooks, Arc<Pause>, Release) {
    let pause = Pause::new();
    let p = pause.clone();
    let stopped = AtomicBool::new(false);
    (
        Hooks {
            observer: Some(Arc::new(move |v| {
                if v == point && !stopped.swap(true, Ordering::AcqRel) {
                    p.stop();
                }
            })),
            queue: None,
        },
        pause.clone(),
        Release(pause),
    )
}
async fn fixture() -> (
    TestArea,
    NodeRuntime,
    NodePromptQueue,
    Arc<AcpNativeAdapter>,
) {
    let area = TestArea::new();
    let node = NodeRuntime::start_control_only_for_test(
        area.config(free_address()),
        std::sync::Arc::new(FakeProtector::default()),
    )
    .await
    .unwrap();
    let pool = node.storage.as_ref().unwrap().pool_for_test();
    sqlx::query("INSERT INTO product_sources VALUES(?,'body component development')")
        .bind(crate::domain::EntityId::from_bytes([1; 16]).encode())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO product_targets VALUES(?,?,?,NULL)")
        .bind(crate::domain::EntityId::from_bytes([2; 16]).encode())
        .bind(crate::domain::EntityId::from_bytes([1; 16]).encode())
        .bind("body-control")
        .execute(pool)
        .await
        .unwrap();
    let adapter = Arc::new(AcpNativeAdapter::default());
    let (queue, driver) = node
        .storage
        .as_ref()
        .unwrap()
        .start_queue_service(
            node.request_authority().unwrap(),
            adapter.clone(),
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await
        .unwrap();
    drop(driver);
    (area, node, queue, adapter)
}
async fn finish(area: TestArea, node: NodeRuntime) {
    let db = area.data_dir().join("node.sqlite3");
    node.shutdown(None).await.unwrap();
    drop(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(db)
            .unwrap(),
    );
    drop(area);
}
fn receipt(node: &NodeRuntime) -> Arc<dyn PromptPreparationShutdownPort> {
    node.storage
        .as_ref()
        .unwrap()
        .prompt_preparation_receipt()
        .unwrap()
}
async fn joined(r: &Arc<dyn PromptPreparationShutdownPort>) -> PreparationShutdownReport {
    let report = tokio::time::timeout(Duration::from_secs(10), r.join())
        .await
        .unwrap();
    assert!(report.observation().all_joined(), "{report:?}");
    assert!(
        report
            .observation()
            .cpu()
            .iter()
            .all(|t| !matches!(t.join(), TaskJoinState::Pending))
    );
    report
}
async fn sql_count(node: &NodeRuntime) -> (i64, i64) {
    let pool = node.storage.as_ref().unwrap().pool_for_test();
    let registrations = sqlx::query_scalar("SELECT count(*) FROM node_request_registrations")
        .fetch_one(pool)
        .await
        .unwrap();
    let consumes = sqlx::query_scalar("SELECT count(*) FROM node_request_operations")
        .fetch_one(pool)
        .await
        .unwrap();
    (registrations, consumes)
}
#[tokio::test]
async fn body_dev_registered_full_upload_uses_original_queue() {
    let (area, node, queue, adapter) = fixture().await;
    let original = queue.queue_runtime_identity();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let h = attach_with_hooks(
        node.storage.as_ref().unwrap(),
        queue,
        Hooks {
            observer: None,
            queue: Some(Arc::new(move |point, actual| {
                assert!(original.same(&actual));
                if point == validation_test_hooks::Point::Entered {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            })),
        },
    )
    .unwrap();
    let r = receipt(&node);
    assert!(r.observe().cpu().is_empty());
    let bytes = b" \n[{\"type\":\"future\",\"ext\":90071992547409931234567890}] \t".to_vec();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let mut rx = h.begin(bytes.clone()).unwrap();
    let prepared = rx.wait().await.unwrap();
    assert!(matches!(
        rx.observe().join(),
        TaskJoinState::Joined(TaskExit::Completed)
    ));
    let lease = prepared.acquire_use().unwrap();
    assert_eq!(lease.validated().body().uploaded_bytes(), bytes);
    assert_eq!(*lease.validated().digest(), digest);
    let retain = adapter.retain(lease.validated());
    assert_eq!(
        retain.as_ref().fragment(),
        lease.validated().body().prompt_fragment()
    );
    assert_eq!(
        retain.as_ref().fragment().as_ptr(),
        lease.validated().body().prompt_fragment().as_ptr()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        rx.wait().await.unwrap_err(),
        PreparationError::ResultAlreadyClaimed
    );
    drop(retain);
    drop(lease);
    drop(prepared);
    drop(rx);
    drop(h);
    let report = joined(&r).await;
    assert_eq!(report.observation().disposal().len(), 1);
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_cpu_pause_keeps_control_progress() {
    let (area, node, queue, _) = fixture().await;
    let key = SigningKey::from_slice(&[29; 32]).unwrap();
    let client = pair_browser(&node, &key, "body control").await;
    let policy = node.application_policy().unwrap();
    let actor = policy_actor(&policy, &key, &client, Id128::new([25; 16])).await;
    let authority = node.request_authority().unwrap();
    let client_entity = crate::domain::EntityId::decode(&client).unwrap();
    let root_key = crate::domain::native::NativeSessionKey::from_config(
        Some(crate::domain::EntityId::from_bytes([1; 16])),
        "body-control",
    )
    .unwrap();
    authority
        .session_control()
        .claim_initial(&root_key, client_entity, "body control", true);
    let (hooks, pause, _release) = paused(Point::RawParsedBeforeBlockChecks);
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue.clone(), hooks).unwrap();
    let rx = h.begin(UPLOAD.to_vec()).unwrap();
    pause.reached().await;
    ipc::status(node.ipc_address()).await.unwrap();
    let target = crate::domain::EntityId::from_bytes([2; 16]);
    authority.authorize_read(&actor, target).await.unwrap();
    queue.inspect_queue(&actor, target, |_| ()).await.unwrap();
    let mutation = crate::application::request_registry::ValidatedMutation::Cancel {
        item: crate::domain::EntityId::from_bytes([3; 16]),
        revision: 1,
    };
    let registration = queue.register(&actor, target, &mutation).await.unwrap();
    assert!(matches!(
        queue
            .consume(&actor, target, registration.id(), mutation)
            .await,
        crate::application::queue_owner::ConsumeReply::Resolved { .. }
    ));
    let mut later = h
        .begin(br#"[{"type":"text","text":"later independently joins"}]"#.to_vec())
        .unwrap();
    let later_prompt = tokio::time::timeout(Duration::from_secs(5), later.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        later.observe().join(),
        TaskJoinState::Joined(TaskExit::Completed)
    ));
    drop(later_prompt);
    drop(later);
    h.seal();
    assert!(receipt(&node).observe().sealed());
    pause.release();
    drop(rx);
    drop(h);
    joined(&receipt(&node)).await;
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_wait_drop_and_receipt_drop_keep_original_slot() {
    for point in [
        Point::CpuEntered,
        Point::RawParsedBeforeBlockChecks,
        Point::DigestComputed,
    ] {
        let (area, node, queue, _) = fixture().await;
        let (hooks, pause, _release) = paused(point);
        let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
        let mut rx = h.begin(UPLOAD.to_vec()).unwrap();
        pause.reached().await;
        let before = rx.observe();
        {
            let waiter = rx.wait();
            tokio::pin!(waiter);
            assert!(poll_once(waiter.as_mut()).await.is_pending());
        }
        let after = rx.observe();
        assert!(before.identity().same(after.identity()));
        assert_eq!(after.join(), TaskJoinState::Pending);
        drop(rx);
        assert_eq!(receipt(&node).observe().cpu().len(), 1);
        pause.release();
        drop(h);
        joined(&receipt(&node)).await;
        finish(area, node).await;
    }
}
#[tokio::test]
async fn body_dev_malformed_and_unknown_are_private() {
    let (area, node, queue, _) = fixture().await;
    let h = attach_prompt_preparation(node.storage.as_ref().unwrap(), queue).unwrap();
    let before = sql_count(&node).await;
    for input in [
        vec![255],
        br#"[{"type":"text","te\u0078t":"a","text":"b"}]"#.to_vec(),
        b"{}".to_vec(),
        br#"[{"type":"text"}]"#.to_vec(),
    ] {
        let mut rx = h.begin(input).unwrap();
        let e = rx.wait().await.unwrap_err();
        assert_eq!(e, PreparationError::MalformedInput);
        assert!(!format!("{e:?}").contains("owner-canary"));
    }
    let mut rx = h
        .begin(br#"[{"type":"future","private":"owner-canary"}]"#.to_vec())
        .unwrap();
    drop(rx.wait().await.unwrap());
    drop(rx);
    assert_eq!(before, sql_count(&node).await);
    drop(h);
    joined(&receipt(&node)).await;
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_seal_wins_returns_original_upload() {
    let (area, node, queue, _) = fixture().await;
    let (hooks, pause, _release) = paused(Point::BeforeAdmissionLock);
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
    let second = h.clone();
    let upload = UPLOAD.to_vec();
    let pointer = upload.as_ptr() as usize;
    let task = tokio::task::spawn_blocking(move || second.begin(upload));
    pause.reached().await;
    h.seal();
    pause.release();
    let Err(rejected) = task.await.unwrap() else {
        panic!("seal lost")
    };
    assert_eq!(rejected.kind(), PreparationError::Stopping);
    let returned = rejected.into_upload();
    assert_eq!(returned.as_ptr() as usize, pointer);
    assert_eq!(returned, UPLOAD);
    assert!(receipt(&node).observe().cpu().is_empty());
    drop(returned);
    drop(h);
    joined(&receipt(&node)).await;
    finish(area, node).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn body_dev_registration_wins_seal_keeps_task() {
    let (area, node, queue, _) = fixture().await;
    let runtime = tokio::runtime::Handle::current();
    let pause = Pause::new();
    let _release = Release(pause.clone());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let original = queue.queue_runtime_identity();
    let (h, shutdown) = std::thread::scope(|scope| {
        let storage = node.storage.as_ref().unwrap();
        let q = queue.clone();
        let p = pause.clone();
        let original_reg = original.clone();
        let handle = scope.spawn(move || {
            let _runtime = runtime.enter();
            let _observer = validation_test_hooks::install(Arc::new(move |point, identity| {
                assert!(original_reg.same(&identity));
                if point == validation_test_hooks::Point::PreparationInstalledUnderTaskFence {
                    entered_tx.send(()).unwrap();
                    p.stop();
                }
            }));
            attach_prompt_preparation(storage, q).unwrap()
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let r = receipt(&node);
        assert_eq!(r.observe().supervisor(), TaskJoinState::NeverStarted);
        r.seal();
        let q = queue.clone();
        let original_shutdown = original.clone();
        let runtime = tokio::runtime::Handle::current();
        let shutdown = scope.spawn(move || {
            let _observer = validation_test_hooks::install(Arc::new(move |point, identity| {
                assert!(original_shutdown.same(&identity));
                if point == validation_test_hooks::Point::ShutdownBeforeTaskFence {
                    shutdown_tx.send(()).unwrap();
                }
            }));
            runtime.block_on(q.shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(5),
            )))
        });
        shutdown_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(r.observe().supervisor(), TaskJoinState::NeverStarted);
        pause.release();
        (handle.join().unwrap(), shutdown.join().unwrap())
    });
    assert!(shutdown.joined);
    assert!(receipt(&node).observe().sealed());
    let replacement = node
        .storage
        .as_ref()
        .unwrap()
        .start_queue_service(
            node.request_authority().unwrap(),
            Arc::new(AcpNativeAdapter::default()),
            SystemNodeClock,
            Arc::new(BlockedEnvironment),
        )
        .await;
    assert!(matches!(
        replacement,
        Err(crate::domain::request_registry::RegistryError::RuntimeAlreadyStarted)
    ));
    drop(h);
    joined(&receipt(&node)).await;
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_deadline_and_cancelled_close_waiter_keep_join() {
    let (area, node, queue, _) = fixture().await;
    let (hooks, pause, _release) = paused(Point::RawParsedBeforeBlockChecks);
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
    let rx = h.begin(UPLOAD.to_vec()).unwrap();
    pause.reached().await;
    let r = receipt(&node);
    let old = r.wait_until(Instant::now()).await;
    let PreparationCloseWait::WaitExpired(snapshot) = &old else {
        panic!("CPU was paused")
    };
    let identity = snapshot.cpu()[0].identity().clone();
    {
        let close = r.join();
        tokio::pin!(close);
        assert!(poll_once(close.as_mut()).await.is_pending());
    }
    {
        let storage = node.storage.as_ref().unwrap();
        let mut first_close = Box::pin(storage.close());
        assert!(poll_once(first_close.as_mut()).await.is_pending());
        drop(first_close);
        assert!(Arc::ptr_eq(&r, &receipt(&node)));
    }
    assert!(identity.same(r.observe().cpu()[0].identity()));
    pause.release();
    drop(rx);
    drop(h);
    let done = joined(&r).await;
    assert!(identity.same(done.observation().cpu()[0].identity()));
    assert_eq!(snapshot.cpu()[0].join(), TaskJoinState::Pending);
    match r.wait_until(Instant::now() + Duration::from_secs(1)).await {
        PreparationCloseWait::Joined(report) => assert!(report.observation().all_joined()),
        PreparationCloseWait::WaitExpired(_) => panic!("joined receipt regressed"),
    }
    finish(area, node).await;
}
async fn abandoned_published_result(send_failure: bool) {
    let (area, node, queue, _) = fixture().await;
    let receiver = Arc::new(Mutex::new(None::<PreparationReceipt>));
    let observer_receiver = receiver.clone();
    let published = Arc::new(Notify::new());
    let observer_published = published.clone();
    let hooks = Hooks {
        observer: Some(Arc::new(move |p| {
            if p == Point::BeforeResultPublish && send_failure {
                drop(observer_receiver.lock().unwrap().take());
            }
            if p == Point::ResultPublishAttempted {
                observer_published.notify_one();
            }
        })),
        queue: None,
    };
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
    // This current-thread host cannot poll its supervisor until this synchronous move ends.
    *receiver.lock().unwrap() = Some(h.begin(UPLOAD.to_vec()).unwrap());
    tokio::time::timeout(Duration::from_secs(5), published.notified())
        .await
        .unwrap();
    let r = receipt(&node);
    assert_eq!(
        r.observe().cpu()[0].join(),
        TaskJoinState::Joined(TaskExit::Completed)
    );
    if send_failure {
        assert!(receiver.lock().unwrap().is_none());
    } else {
        assert!(receiver.lock().unwrap().is_some());
        drop(receiver.lock().unwrap().take());
    }
    drop(h);
    let report = joined(&r).await;
    assert_eq!(report.observation().disposal().len(), 1);
    assert_eq!(
        report.observation().disposal()[0].join(),
        TaskJoinState::Joined(TaskExit::Completed)
    );
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_disposal_and_custody_are_owned() {
    let (area, node, queue, _) = fixture().await;
    let (hooks, pause, _release) = paused(Point::DisposalEntered);
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
    let mut rx = h.begin(UPLOAD.to_vec()).unwrap();
    let prompt = rx.wait().await.unwrap();
    drop(rx);
    let lease = prompt.acquire_use().unwrap();
    let r = receipt(&node);
    r.seal();
    assert_eq!(r.observe().active_use(), 1);
    assert_eq!(prompt.acquire_use().err(), Some(PreparationError::Stopping));
    assert!(matches!(
        r.wait_until(Instant::now()).await,
        PreparationCloseWait::WaitExpired(_)
    ));
    drop(prompt);
    drop(lease);
    pause.reached().await;
    let pending = r.observe();
    assert_eq!(pending.disposal().len(), 1);
    assert_eq!(pending.disposal()[0].join(), TaskJoinState::Pending);
    pause.release();
    drop(h);
    let report = joined(&r).await;
    assert_eq!(
        report.observation().disposal()[0].join(),
        TaskJoinState::Joined(TaskExit::Completed)
    );
    finish(area, node).await;
    abandoned_published_result(false).await;
    abandoned_published_result(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn body_dev_registration_failure_has_no_unowned_spawn() {
    let (area, node, queue, _) = fixture().await;
    let (other_area, other, foreign, _) = fixture().await;
    assert!(attach_prompt_preparation(node.storage.as_ref().unwrap(), foreign).is_err());
    assert!(
        node.storage
            .as_ref()
            .unwrap()
            .prompt_preparation_receipt()
            .is_none()
    );
    let pause = Pause::new();
    let _release = Release(pause.clone());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (attempt_tx, attempt_rx) = std::sync::mpsc::channel();
    let runtime = tokio::runtime::Handle::current();
    let runtime2 = runtime.clone();
    let original = queue.queue_runtime_identity();
    let (closed, rejected) = std::thread::scope(|scope| {
        let q = queue.clone();
        let p = pause.clone();
        let stopping = scope.spawn(move || {
            let _observer = validation_test_hooks::install(Arc::new(move |point, identity| {
                assert!(original.same(&identity));
                if point == validation_test_hooks::Point::StoppingUnderTaskFence {
                    entered_tx.send(()).unwrap();
                    p.stop();
                }
            }));
            runtime.block_on(q.shutdown(RegistryDeadline::new(
                Instant::now() + Duration::from_secs(5),
            )))
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            queue.startup_status(),
            crate::application::queue_owner::StartupStatus::Stopping
        );
        let storage = node.storage.as_ref().unwrap();
        let q = queue.clone();
        let registration = scope.spawn(move || {
            let _runtime = runtime2.enter();
            attempt_tx.send(()).unwrap();
            attach_prompt_preparation(storage, q)
        });
        attempt_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            node.storage
                .as_ref()
                .unwrap()
                .prompt_preparation_receipt()
                .is_none()
        );
        pause.release();
        (stopping.join().unwrap(), registration.join().unwrap())
    });
    assert!(closed.joined);
    assert!(rejected.is_err());
    assert!(
        node.storage
            .as_ref()
            .unwrap()
            .prompt_preparation_receipt()
            .is_none()
    );
    finish(other_area, other).await;
    finish(area, node).await;
    let (area, node, queue, _) = fixture().await;
    let h = attach_prompt_preparation(node.storage.as_ref().unwrap(), queue.clone()).unwrap();
    let old = receipt(&node);
    assert!(attach_prompt_preparation(node.storage.as_ref().unwrap(), queue).is_err());
    assert!(Arc::ptr_eq(&old, &receipt(&node)));
    assert!(old.observe().cpu().is_empty());
    drop(h);
    joined(&old).await;
    finish(area, node).await;
    let (area, node, queue, _) = fixture().await;
    node.storage.as_ref().unwrap().close().await;
    assert!(attach_prompt_preparation(node.storage.as_ref().unwrap(), queue).is_err());
    assert!(
        node.storage
            .as_ref()
            .unwrap()
            .prompt_preparation_receipt()
            .is_none()
    );
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_factory_wait_cancel_keeps_registered_receipt() {
    let (area, node, queue, _) = fixture().await;
    let storage = node.storage.as_ref().unwrap();
    let factory = async {
        let h = attach_prompt_preparation(storage, queue).unwrap();
        std::future::pending::<()>().await;
        drop(h);
    };
    let mut factory = Box::pin(factory);
    assert!(poll_once(factory.as_mut()).await.is_pending());
    let r = receipt(&node);
    drop(factory);
    assert!(r.observe().sealed());
    joined(&r).await;
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_no_runtime_and_no_ambient_drop() {
    let (area, node, queue, _) = fixture().await;
    std::thread::scope(|scope| {
        let q = queue.clone();
        let storage = node.storage.as_ref().unwrap();
        scope
            .spawn(move || {
                assert!(matches!(
                    attach_prompt_preparation(storage, q),
                    Err(PreparationError::Unavailable)
                ));
            })
            .join()
            .unwrap();
    });
    assert!(
        node.storage
            .as_ref()
            .unwrap()
            .prompt_preparation_receipt()
            .is_none()
    );
    let h = attach_prompt_preparation(node.storage.as_ref().unwrap(), queue).unwrap();
    let mut rx = h.begin(UPLOAD.to_vec()).unwrap();
    drop(rx.wait().await.unwrap());
    drop(rx);
    std::thread::spawn(move || drop(h)).join().unwrap();
    assert!(receipt(&node).observe().sealed());
    joined(&receipt(&node)).await;
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_concurrent_close_is_same_operation() {
    let (area, node, queue, _) = fixture().await;
    let (hooks, pause, _release) = paused(Point::RawParsedBeforeBlockChecks);
    let h = attach_with_hooks(node.storage.as_ref().unwrap(), queue, hooks).unwrap();
    let rx = h.begin(UPLOAD.to_vec()).unwrap();
    pause.reached().await;
    let r = receipt(&node);
    let identity = r.observe().cpu()[0].identity().clone();
    let (a, b) = tokio::join!(r.wait_until(Instant::now()), r.wait_until(Instant::now()));
    assert!(matches!(a, PreparationCloseWait::WaitExpired(_)));
    assert!(matches!(b, PreparationCloseWait::WaitExpired(_)));
    pause.release();
    drop(rx);
    drop(h);
    let (a, b) = tokio::join!(joined(&r), joined(&r));
    assert!(identity.same(a.observation().cpu()[0].identity()));
    assert!(identity.same(b.observation().cpu()[0].identity()));
    let storage = node.storage.as_ref().unwrap();
    tokio::join!(storage.close(), storage.close());
    assert!(r.observe().all_joined());
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_real_worker_panic_is_safe_failure() {
    let (area, node, queue, _) = fixture().await;
    let h = attach_with_hooks(
        node.storage.as_ref().unwrap(),
        queue,
        Hooks {
            observer: Some(Arc::new(|p| {
                assert!(
                    p != Point::DigestComputed,
                    "body worker static safety probe"
                );
            })),
            queue: None,
        },
    )
    .unwrap();
    let mut rx = h.begin(UPLOAD.to_vec()).unwrap();
    assert_eq!(rx.wait().await.unwrap_err(), PreparationError::WorkerFailed);
    drop(rx);
    drop(h);
    let report = joined(&receipt(&node)).await;
    assert!(
        report
            .observation()
            .failures()
            .contains(&PreparationFailure::WorkerFailed)
    );
    assert!(!format!("{report:?}").contains("owner-canary"));
    finish(area, node).await;
}
#[tokio::test]
async fn body_dev_large_repeated_inputs_add_no_quota() {
    let (area, node, queue, _) = fixture().await;
    let h = attach_prompt_preparation(node.storage.as_ref().unwrap(), queue).unwrap();
    for size in [1, 65536, 1_048_576, 65536, 2_097_152] {
        let bytes =
            format!("[{{\"type\":\"text\",\"text\":\"{}\"}}]", "x".repeat(size)).into_bytes();
        let mut rx = h.begin(bytes).unwrap();
        let prompt = rx.wait().await.unwrap();
        let lease = prompt.acquire_use().unwrap();
        assert_eq!(lease.validated().body().uploaded_bytes().len(), size + 27);
        drop(lease);
        drop(prompt);
        drop(rx);
    }
    drop(h);
    let report = joined(&receipt(&node)).await;
    assert_eq!(report.observation().cpu().len(), 5);
    assert_eq!(report.observation().disposal().len(), 5);
    finish(area, node).await;
}
