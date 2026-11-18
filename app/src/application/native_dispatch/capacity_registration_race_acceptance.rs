//! Development RT016: actual ACP supervisor, original native registrar and start signal.
use super::*;
use crate::adapters::acp::native_views::AcpNativeViewFactory;
use crate::application::native_dispatch::AutomaticReleaseReason;
use crate::application::shared_views::tasks::{
    ViewTaskKind,
    capacity_registration_race_hook::{
        RegistrationCut, RegistrationRaceGate, ReleaseRegistrationGate,
    },
};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

#[tokio::test]
async fn rt016_root_freeze_wins_before_original_adoption() {
    super::super::run_factory_scenario("child-auto-read-rt-race-before-root").await;
}

#[tokio::test]
async fn rt016_original_adoption_wins_before_root_freeze() {
    super::super::run_factory_scenario("child-auto-read-rt-race-after-root").await;
}

#[tokio::test]
async fn rt016_source_shutdown_wins_before_original_adoption() {
    super::super::run_factory_scenario("child-auto-read-rt-race-before-source").await;
}

#[tokio::test]
async fn rt016_original_adoption_wins_before_source_shutdown() {
    super::super::run_factory_scenario("child-auto-read-rt-race-after-source").await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Preserve real source setup and both sides of the registered-handle/start-signal interleaving in one owned fixture"
)]
pub(super) async fn run<L>(
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
        .install_view_factory(Arc::new(AcpNativeViewFactory::replay_only_for_test(
            std::num::NonZeroUsize::new(64).unwrap(),
            Arc::new(BridgeReadOptions::new()),
        )))
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
    sqlx::query("INSERT INTO product_targets(target_id,source_id,native_id,workspace_id) VALUES(?,?,?,?) ON CONFLICT(target_id) DO UPDATE SET native_id=excluded.native_id")
        .bind(id(8).encode()).bind(id(1).encode()).bind("synthetic-child-A")
        .bind(id(2).encode()).execute(storage.pool_for_test()).await.unwrap();
    let proof = service
        .prepare_native_read(&id(10), id(8), ReadReason::FirstOpen)
        .await
        .unwrap();
    let requested = native
        .acquire_read_source(proof, ReadReason::FirstOpen)
        .unwrap();
    let root_resume_release = ChildReadRelease(area.join("release-child-root-resume"));
    drop(root_resume_release);
    let capability = timeout(Duration::from_secs(8), requested.source_capability())
        .await
        .unwrap()
        .unwrap();
    let capacity = capability.capacity.clone();
    let attachment = capability.source().views().unwrap();
    timeout(Duration::from_secs(8), async {
        while native
            .core
            .automatic_candidate(&source, &capacity, std::time::Instant::now())
            .unwrap()
            .is_none()
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual peer supplied root and both child current Idle");
    let after_adopt = scenario.contains("-after-");
    let close_source = scenario.ends_with("-source");
    let cut = if after_adopt {
        RegistrationCut::AfterAdoptBeforeStart
    } else {
        RegistrationCut::BeforeAdopt
    };
    let gate = RegistrationRaceGate::new(cut, capability.native().clone());
    let release = ReleaseRegistrationGate(gate.clone());
    attachment.capacity_install_registration_gate(gate.clone());
    let previous: Vec<_> = capacity
        .reads
        .lock()
        .unwrap()
        .iter()
        .filter_map(std::sync::Weak::upgrade)
        .map(|slot| Arc::as_ptr(&slot) as usize)
        .collect();
    let started = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let user_started = started.clone();
    let child = capability.native().clone();
    let mut registration = tokio::task::spawn_blocking(move || {
        attachment.rt_spawn_target_kind_for_test(
            &child,
            ViewTaskKind::Reader,
            Box::pin(async move {
                user_started.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
    });
    let mut registration_joined = false;
    let result = AssertUnwindSafe(async {
        timeout(Duration::from_secs(5), async {
            while !gate.reached() {
                tokio::task::yield_now().await;
            }
        }).await.expect("actual supervisor reached owned cut before start.send");
        assert_eq!(started.load(Ordering::SeqCst), 0);
        let registered: Vec<_> = capacity.reads.lock().unwrap().iter()
            .filter_map(std::sync::Weak::upgrade)
            .filter(|slot| !previous.contains(&(Arc::as_ptr(slot) as usize)))
            .collect();
        assert_eq!(registered.len(), usize::from(after_adopt),
            "only successful original registrar adoption adds the actual handle");
        if close_source {
            let (report, ()) = tokio::join!(
                native.shutdown(Instant::now() + Duration::from_secs(8)),
                async {
                    timeout(Duration::from_secs(5), async {
                        while !source.pre.authority.closing.load(Ordering::Acquire)
                            && !source.pre.authority.closed.load(Ordering::Acquire)
                        {
                            tokio::task::yield_now().await;
                        }
                    }).await.unwrap();
                    assert_eq!(started.load(Ordering::SeqCst), 0);
                    assert!(registered.iter().all(|slot| !slot.confirmed_joined()));
                    gate.release();
                }
            );
            assert!(report.joined && report.metadata_confirmed, "{report:?}");
        } else {
            let (closed, ()) = tokio::join!(
                native.core.automatic_release(
                    &source, &capacity, AutomaticReleaseReason::CapacityPressure,
                ),
                async {
                    timeout(Duration::from_secs(5), async {
                        while !capacity.pre.authority.closed.load(Ordering::Acquire) {
                            tokio::task::yield_now().await;
                        }
                    }).await.expect("actual original native Close response");
                    assert_eq!(started.load(Ordering::SeqCst), 0);
                    assert!(registered.iter().all(|slot| !slot.confirmed_joined()));
                    if after_adopt {
                        assert_eq!(native.inspect_capacity().await.unwrap().occupied, 1);
                    }
                    gate.release();
                }
            );
            assert_eq!(closed, Ok(true));
            assert_eq!(std::fs::read_to_string(area.join("rt-child-close-count")).unwrap(), "1");
        }
        let registration = timeout(Duration::from_secs(5), &mut registration).await.unwrap().unwrap();
        registration_joined = true;
        assert_eq!(registration.is_ok(), after_adopt, "original registrar wins or rejects");
        assert_eq!(started.load(Ordering::SeqCst), usize::from(after_adopt),
            "rejected work must never start; accepted original work starts exactly once");
        assert!(registered.iter().all(|slot| slot.confirmed_joined() && !slot.failed()));
        eprintln!("RT016 after_adopt={after_adopt} source_shutdown={close_source} registered={} started={} confirmed_joined={}",
            registered.len(), started.load(Ordering::SeqCst),
            registered.iter().all(|slot| slot.confirmed_joined()));
    }).catch_unwind().await;
    drop(release);
    if !registration_joined {
        let _ = timeout(Duration::from_secs(5), &mut registration)
            .await
            .expect(
                "the owned blocked registrar is released and joined even after assertion failure",
            );
    }
    // Preserve cleanup even if any interleaving assertion failed.
    let shutdown = native
        .shutdown(Instant::now() + Duration::from_secs(8))
        .await;
    eprintln!(
        "RT016 cleanup={shutdown:?} sources={:?}",
        native.source_close_diagnostics()
    );
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
