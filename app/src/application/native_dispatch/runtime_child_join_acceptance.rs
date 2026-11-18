use super::*;
use crate::application::native_dispatch::AutomaticReleaseReason;
use crate::application::shared_views::tasks::ViewTaskKind;
use crate::application::{queue_owner::QueueService, shared_views::budget::ReadReason};
use crate::session_storage::{
    native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry,
};

struct ChildReadRelease(PathBuf);
impl Drop for ChildReadRelease {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"release");
    }
}

struct ReleaseOnDrop(Option<tokio::sync::oneshot::Sender<()>>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Track both child views and their original root task ownership across read, seal and join"
)]
pub(super) async fn rt_case<L>(
    storage: &crate::sqlite_fixture_scope::FixtureStorage,
    service: &QueueService<Authority, Validator, SqliteRequestRegistry, SystemNodeClock>,
    native: &NativeRuntimeHandle<
        AcpNativeAdapter,
        L,
        SqliteNativeConfigurationProvider<Configuration>,
        SqliteNativeDispatch,
    >,
    area: &NativeArea,
    scenario: &str,
) where
    L: SourceLaunchPort<Plan = WindowsLaunchPlan>,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
{
    native
        .install_view_factory(Arc::new(
            crate::adapters::acp::native_views::AcpNativeViewFactory::replay_only_for_test(
                std::num::NonZeroUsize::new(64).unwrap(),
                Arc::new(BridgeReadOptions::new()),
            ),
        ))
        .unwrap();
    let proof = service
        .prepare_native_read(&id(10), id(4), ReadReason::FirstOpen)
        .await
        .unwrap();
    let plan = native
        .core
        .configuration
        .resolve_read(&proof)
        .await
        .unwrap();
    let source = native
        .core
        .capacity_source(proof.native(), Arc::new(plan.into_source()))
        .await
        .unwrap();
    native
        .core
        .repository
        .reserve_source(&source.pre, &native.core.deadline().unwrap())
        .await
        .unwrap();
    source.recorded.store(true, Ordering::Release);
    seed_child_read_tree(storage.pool_for_test(), &source.pre).await;
    assert!(
        source.source.get().is_none(),
        "root fixture has no native connection or body"
    );
    for (target, child) in [(id(8), "synthetic-child-A"), (id(5), "synthetic-child-B")] {
        sqlx::query("INSERT INTO product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,?) ON CONFLICT(target_id) DO UPDATE SET native_id=excluded.native_id")
            .bind(target.encode()).bind(id(1).encode()).bind(child).bind(id(2).encode())
            .execute(storage.pool_for_test()).await.unwrap();
    }
    let release = ChildReadRelease(area.join("release-child-root-resume"));
    let first_proof = service
        .prepare_native_read(&id(10), id(8), ReadReason::FirstOpen)
        .await
        .unwrap();
    let second_proof = service
        .prepare_native_read(&id(10), id(5), ReadReason::FirstOpen)
        .await
        .unwrap();
    let first = native
        .acquire_read_source(first_proof, ReadReason::FirstOpen)
        .unwrap();
    let second = native
        .acquire_read_source(second_proof, ReadReason::FirstOpen)
        .unwrap();
    let held = timeout(Duration::from_secs(8), async {
        loop {
            let jobs = source
                .capacity
                .lock()
                .unwrap()
                .values()
                .map(|capacity| capacity.reads.lock().unwrap().len())
                .sum::<usize>();
            if area.join("child-root-resume-held").is_file() && jobs >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if held.is_err() {
        let first_result = timeout(Duration::from_millis(100), first.source_capability())
            .await
            .map(|result| result.map(|_| ()));
        let second_result = timeout(Duration::from_millis(100), second.source_capability())
            .await
            .map(|result| result.map(|_| ()));
        panic!(
            "root resume not reached: first={first_result:?}, second={second_result:?}, requests={:?}, sources={}",
            std::fs::read_to_string(area.join("child-read-requests")),
            native.core.sources.lock().unwrap().len()
        );
    }
    assert_eq!(
        std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
        "synthetic-native-A\n"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_session_capacity WHERE released=0")
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap();
    assert_eq!(
        count, 1,
        "both child installations own one root reservation before response"
    );
    drop(release);
    let (first, second) = tokio::join!(first.source_capability(), second.source_capability());
    if scenario == "child-auto-read-failure" {
        assert!(matches!(
            first,
            Err(crate::domain::shared_view::ViewError::NativeFailure(
                DispatchError::ProtocolRejected
            ))
        ));
        assert!(matches!(
            second,
            Err(crate::domain::shared_view::ViewError::NativeFailure(
                DispatchError::ProtocolRejected
            ))
        ));
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM node_session_capacity WHERE released=0")
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap();
        assert_eq!(
            count, 1,
            "failed resume keeps original capacity responsibility"
        );
        return;
    }
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.native().native_id(), "synthetic-child-A");
    assert_eq!(second.native().native_id(), "synthetic-child-B");
    assert!(Arc::ptr_eq(&first.capacity, &second.capacity));
    assert_eq!(
        first.configuration_binding(),
        second.configuration_binding()
    );
    assert_eq!(first.pre_source.metadata.config_identity, id(6));
    assert_eq!(
        native.core.sources.lock().unwrap().len(),
        1,
        "second child's different launch configuration follows the original root owner"
    );
    assert_eq!(first.capacity.pre.native.native_id(), "synthetic-native-A");
    assert!(
        source.targets.lock().unwrap().is_empty(),
        "child read does not invent root or child queue resource"
    );
    let attachment = first.source().views().unwrap();
    let views = attachment.views().unwrap();
    for capability in [&first, &second] {
        let snapshot = timeout(Duration::from_secs(8), async {
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if let Ok(target) = views.core.target(capability.native()) {
                    let snapshot = target.snapshot.read().unwrap().clone();
                    if snapshot.projection.records.len() > 0 {
                        break snapshot;
                    }
                }
                changed.await;
            }
        })
        .await
        .unwrap();
        let body = snapshot.projection.records.get(0).unwrap();
        assert!(
            std::str::from_utf8(body.raw.bytes())
                .unwrap()
                .contains(&format!(
                    "automatically delivered {}",
                    capability.native().native_id()
                ))
        );
        let request = attachment
            .read_control()
            .unwrap()
            .request(capability.native(), ReadReason::FirstOpen)
            .unwrap();
        timeout(Duration::from_secs(8), async {
            let mut changes = request.subscribe();
            loop {
                let current = changes.borrow_and_update().clone();
                if current.read_mode() == crate::domain::shared_view::ReadMode::CurrentOnly {
                    break;
                }
                changes.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
        "synthetic-native-A\n"
    );
    // Independent RT-013/014 assertions start here. Setup above reuses the
    // existing real-source child-read fixture, not its controlled closed cut.
    let kind = if scenario.ends_with("-native-read") {
        ViewTaskKind::NativeRead
    } else if scenario.ends_with("-cleanup") {
        ViewTaskKind::Cleanup
    } else {
        ViewTaskKind::Reader
    };
    let fails = scenario.ends_with("-join-failure");
    let original_reads: Vec<_> = first
        .capacity
        .reads
        .lock()
        .unwrap()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .map(|slot| Arc::as_ptr(&slot) as usize)
        .collect();
    let (entered, entered_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = tokio::sync::oneshot::channel();
    let release = ReleaseOnDrop(Some(release));
    attachment
        .rt_spawn_target_kind_for_test(
            first.native(),
            kind,
            Box::pin(async move {
                let _ = entered.send(());
                let _ = release_rx.await;
                assert!(!fails, "controlled RT-014 actual child JoinError");
                Ok(())
            }),
        )
        .unwrap();
    timeout(Duration::from_secs(5), entered_rx)
        .await
        .unwrap()
        .unwrap();
    let new_reads: Vec<_> = first
        .capacity
        .reads
        .lock()
        .unwrap()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .filter(|slot| !original_reads.contains(&(Arc::as_ptr(slot) as usize)))
        .collect();
    assert_eq!(
        new_reads.len(),
        1,
        "one actual target task registered with root"
    );
    let last_read = new_reads[0].clone();
    let core = &native.core;
    if scenario.ends_with("-shutdown") {
        assert!(
            native.automatic_recycling_enabled(),
            "normal source runtime scan registered"
        );
        let scan = core.capacity_scan.get().unwrap().clone();
        let (report, ()) = tokio::join!(
            native.shutdown(tokio::time::Instant::now() + Duration::from_secs(8)),
            async {
                timeout(Duration::from_secs(5), async {
                    while !source.pre.authority.closing.load(Ordering::Acquire) {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(
                    !last_read.confirmed_joined(),
                    "shutdown cannot skip held original child handle"
                );
                assert_eq!(
                    native.inspect_capacity().await.unwrap().occupied,
                    1,
                    "shutdown intent is not original source convergence"
                );
                let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let observed = started.clone();
                let registration = attachment.rt_spawn_target_kind_for_test(
                    second.native(),
                    ViewTaskKind::NativeRead,
                    Box::pin(async move {
                        observed.store(true, Ordering::Release);
                        Ok(())
                    }),
                );
                assert!(
                    registration.is_err(),
                    "source closing rejects a new target task"
                );
                assert!(
                    !started.load(Ordering::Acquire),
                    "rejected registration never starts native work"
                );
                drop(release);
            }
        );
        assert!(report.joined && report.metadata_confirmed, "{report:?}");
        assert!(last_read.confirmed_joined() && !last_read.failed());
        assert!(
            scan.confirmed_joined(),
            "actual original scan JoinHandle was reaped"
        );
        assert!(!native.automatic_recycling_enabled());
        assert_eq!(native.inspect_capacity().await.unwrap().occupied, 0);
        assert_eq!(
            std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
            "synthetic-native-A\n"
        );
        return;
    }
    timeout(Duration::from_secs(8), async {
        while core
            .automatic_candidate(&source, &first.capacity, std::time::Instant::now())
            .unwrap()
            .is_none()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual current idle root and both children, no synthetic activity injection");
    let (closed, ()) = tokio::join!(
        core.automatic_release(
            &source,
            &first.capacity,
            AutomaticReleaseReason::CapacityPressure
        ),
        async {
            timeout(Duration::from_secs(4), async {
                while !first.capacity.pre.authority.closed.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual peer Closed, not a test authority cut");
            assert_eq!(
                std::fs::read_to_string(area.join("rt-child-close-count")).unwrap(),
                "1"
            );
            assert!(
                !last_read.confirmed_joined(),
                "seal/Closed cannot stand for actual join"
            );
            assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
            let operation = first.capacity.automatic.lock().unwrap().clone().unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT phase FROM node_session_automatic_releases WHERE operation_id=?"
                )
                .bind(operation.id.encode())
                .fetch_one(storage.pool_for_test())
                .await
                .unwrap(),
                "pending"
            );
            assert!(
                !views
                    .core
                    .target(second.native())
                    .unwrap()
                    .send
                    .current()
                    .unwrap()
            );
            assert!(matches!(
                attachment.rt_spawn_target_kind_for_test(
                    first.native(),
                    kind,
                    Box::pin(async { Ok(()) })
                ),
                Err(crate::domain::shared_view::ViewError::SourceReleased)
            ));
            drop(release);
        }
    );
    assert!(
        last_read.confirmed_joined(),
        "original actual handle was reaped"
    );
    let operation = first.capacity.automatic.lock().unwrap().clone().unwrap();
    if fails {
        assert_eq!(closed, Err(DispatchError::ResourceUnconfirmed));
        assert!(
            last_read.failed(),
            "JoinError must propagate to original native slot"
        );
        assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT phase FROM node_session_automatic_releases WHERE operation_id=?"
            )
            .bind(operation.id.encode())
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap(),
            "pending"
        );
    } else {
        assert_eq!(closed, Ok(true));
        assert!(!last_read.failed());
        assert_eq!(native.inspect_capacity().await.unwrap().occupied, 0);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT phase FROM node_session_automatic_releases WHERE operation_id=?"
            )
            .bind(operation.id.encode())
            .fetch_one(storage.pool_for_test())
            .await
            .unwrap(),
            "finished"
        );
        assert_eq!(
            core.automatic_release(
                &source,
                &first.capacity,
                AutomaticReleaseReason::CapacityPressure
            )
            .await,
            Ok(true)
        );
    }
    assert_eq!(
        std::fs::read_to_string(area.join("rt-child-close-count")).unwrap(),
        "1",
        "only original root native close; no child close and no retry duplicate"
    );
    assert_eq!(
        std::fs::read_to_string(area.join("child-read-requests")).unwrap(),
        "synthetic-native-A\n"
    );
}

#[tokio::test]
async fn rt_013_reader_actual_closed_waits_last_child_join_then_finishes() {
    super::super::run_factory_scenario("child-auto-read-rt-reader").await;
}
#[tokio::test]
async fn rt_013_native_read_actual_closed_waits_last_child_join_then_finishes() {
    super::super::run_factory_scenario("child-auto-read-rt-native-read").await;
}
#[tokio::test]
async fn rt_013_cleanup_actual_closed_waits_last_child_join_then_finishes() {
    super::super::run_factory_scenario("child-auto-read-rt-cleanup").await;
}
#[tokio::test]
async fn rt_014_actual_closed_child_join_error_retains_original_capacity() {
    super::super::run_factory_scenario("child-auto-read-rt-join-failure").await;
}

#[tokio::test]
async fn rt_017_shutdown_joins_scan_and_last_actual_child_task_before_returning_capacity() {
    super::super::run_factory_scenario("child-auto-read-rt-shutdown").await;
}
