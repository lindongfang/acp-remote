//! Development RT015: real ACP Received/ACK and necessary parent SQL, controlled Created seed.
use super::*;
use crate::adapters::acp::native_views::AcpNativeViewFactory;
use crate::application::native_dispatch::SessionTreeRepository;
use crate::application::shared_views::budget::{MonotonicReadOptionsProvider, ReadResponseBudget};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;

struct ReleaseClosed(PathBuf);
impl Drop for ReleaseClosed {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, b"release original Closed");
    }
}

#[tokio::test]
async fn rt015_received_child_before_first_byte_withdraws_original_close() {
    late_child(false).await;
}

#[tokio::test]
async fn rt015_received_child_after_first_byte_is_persisted_and_sealed() {
    late_child(true).await;
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep real SQL writer, Received/ACK evidence, original close and owned cleanup in one deterministic interleaving"
)]
async fn late_child(after_first_byte: bool) {
    let (storage, scope, directory) = storage_fixture().await;
    let configuration = Arc::new(storage.native_configuration_provider(Arc::new(Configuration)));
    let (service, runtime) = storage
        .start_native_service(
            Authority,
            Validator,
            SystemNodeClock,
            Arc::new(AcpNativeAdapter::default()),
            Arc::new(WindowsSourceLauncher),
            configuration,
            Arc::new(Retain),
            options(),
        )
        .await
        .unwrap();
    let result = AssertUnwindSafe(async {
        runtime.install_view_factory(Arc::new(AcpNativeViewFactory::replay_only_for_test(
            std::num::NonZeroUsize::new(64).unwrap(),
            Arc::new(MonotonicReadOptionsProvider::new(Duration::from_secs(5), ReadResponseBudget::Unbounded)),
        ))).unwrap();
        let core = &runtime.core;
        let source = core.ensure_source_slot(plan(&directory)).unwrap();
        core.repository.reserve_source(&source.pre, &core.deadline().unwrap()).await.unwrap();
        source.recorded.store(true, Ordering::Release);
        core.initialize_source_checked(&source, || Ok(())).await.unwrap();
        let root = NativeSessionKey::from_config(Some(id(1)), "synthetic-native-A").unwrap();
        let child = NativeSessionKey::from_config(Some(id(1)), "capacity-race-late-child").unwrap();
        let record = seed_synthetic_root(&storage, &source.pre).await;
        let capacity = core.capacity_resource(&source, &root).unwrap();
        *capacity.record.lock().unwrap() = Some(record.clone());
        core.reserve_native_capacity(&source, &capacity).await.unwrap();
        // The Created seed is explicit setup only. New child activity below is wire-derived.
        source.capacity_activity.lock().unwrap().created(
            root.native_id(), ProtocolVersion::V2, Instant::now(),
        );
        assert!(core.automatic_candidate(&source, &capacity, Instant::now()).unwrap().is_some());
        std::fs::write(directory.join("runtime-auto-count-close"), b"count").unwrap();
        let connection = source.source.get().unwrap();
        let views = connection.views().unwrap().views().unwrap();
        let reads_before = capacity.reads.lock().unwrap().len();
        if after_first_byte {
            std::fs::write(directory.join("capacity-race-after-close-child"), b"emit").unwrap();
            let release = ReleaseClosed(directory.join("capacity-race-release-closed"));
            let sequence = source.memory.lock().unwrap().observed_sequence;
            let (first_close, ()) = tokio::join!(
                core.automatic_release(
                    &source, &capacity, AutomaticReleaseReason::CapacityPressure,
                ),
                async {
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while source.memory.lock().unwrap().observed_sequence < sequence + 2 {
                            tokio::task::yield_now().await;
                        }
                    }).await.expect("actual late child relation ACKed before following child State");
                    assert!(capacity.pre.authority.releasing.load(Ordering::Acquire));
                    assert!(!capacity.pre.authority.closed.load(Ordering::Acquire),
                        "peer deliberately retains original Closed response, not a product flag cut");
                    let denied = views.core.send.view_for_target(
                        crate::domain::shared_view::ViewEpoch::fresh().unwrap(), &child,
                    ).unwrap();
                    assert!(!denied.current().unwrap(),
                        "late child must be sealed at receipt, before later Closed/join can mask a missing fence");
                    assert_eq!(capacity.reads.lock().unwrap().len(), reads_before);
                    assert_eq!(runtime.inspect_capacity().await.unwrap().occupied, 1);
                    drop(release);
                }
            );
            let original = capacity.automatic.lock().unwrap().clone().unwrap();
            let lifecycle = original.lifecycle.lock().unwrap().clone().unwrap();
            let ticket = lifecycle.ticket.lock().unwrap().clone().unwrap();
            if first_close == Err(DispatchError::ResourceUnconfirmed) {
                assert!(capacity.pre.authority.closed.load(Ordering::Acquire));
                assert_eq!(runtime.inspect_capacity().await.unwrap().occupied, 1,
                    "late required proof may outlive Closed, so the original reservation remains");
                tokio::time::timeout(Duration::from_secs(5), async {
                    while source.pre.authority.required.load(Ordering::Acquire) != 0 {
                        tokio::task::yield_now().await;
                    }
                }).await.unwrap();
                assert_eq!(core.automatic_release(
                    &source, &capacity, AutomaticReleaseReason::CapacityPressure,
                ).await, Ok(true));
            } else {
                assert_eq!(first_close, Ok(true));
            }
            let retained = capacity.automatic.lock().unwrap().clone().unwrap();
            assert!(Arc::ptr_eq(&original, &retained));
            assert!(Arc::ptr_eq(&ticket, retained.lifecycle.lock().unwrap().as_ref().unwrap()
                .ticket.lock().unwrap().as_ref().unwrap()));
            assert_eq!(std::fs::read_to_string(directory.join("rt-auto-close-count")).unwrap(), "1");
            assert!(capacity.pre.authority.closed.load(Ordering::Acquire));
            let old_generation = views.core.send.view_for_target(
                crate::domain::shared_view::ViewEpoch::fresh().unwrap(), &child,
            ).unwrap();
            assert!(!old_generation.current().unwrap(), "late target inherits old root seal");
            assert_eq!(capacity.reads.lock().unwrap().len(), reads_before,
                "a received late relation must not adopt a new task into the old reservation");
            assert_eq!(runtime.inspect_capacity().await.unwrap().occupied, 0);
        } else {
            let mut writer = storage.pool_for_test().begin().await.unwrap();
            sqlx::query("UPDATE product_sources SET name=name WHERE source_id=?")
                .bind(id(1).encode()).execute(&mut *writer).await.unwrap();
            let (released, ()) = tokio::join!(
                core.automatic_release(&source, &capacity, AutomaticReleaseReason::CapacityPressure),
                async {
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while capacity.automatic.lock().unwrap().is_none() {
                            tokio::task::yield_now().await;
                        }
                    }).await.unwrap();
                    let original = capacity.automatic.lock().unwrap().clone().unwrap();
                    assert!(original.record.lock().unwrap().is_none(), "real begin waits SQL writer");
                    std::fs::write(directory.join("capacity-race-before-close-child"), b"emit").unwrap();
                    let setup = core.protocol.bind_setup(
                        connection, ProtocolVersion::V2, &source.plan.configuration,
                        source.plan.setups[0].1.clone(),
                    ).unwrap();
                    let sequence = source.memory.lock().unwrap().observed_sequence;
                    let ticket = connection.replay_for_test(
                        root.native_id(), &setup,
                        super::super::super::Instant::now() + Duration::from_secs(5),
                    ).unwrap();
                    assert_eq!(ticket.wait().await, WriteReport::Written);
                    tokio::time::timeout(Duration::from_secs(5), async {
                        loop {
                            let received = source.memory.lock().unwrap().observed_sequence;
                            let child_seen = source.capacity_activity.lock().unwrap()
                                .members(root.native_id()).is_ok_and(|members|
                                    members.iter().any(|member| member.as_ref() == child.native_id()));
                            if child_seen && received >= sequence + 7 {
                                break;
                            }
                            tokio::task::yield_now().await;
                        }
                    }).await.expect("subagent, child State and following response consumed");
                    // pump_source ACKs each Received before reading its next frame. The final
                    // response above proves the earlier relation crossed its original ACK.
                    assert!(source.pre.authority.required.load(Ordering::Acquire) > 0);
                    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM node_session_parents")
                        .fetch_one(&mut *writer).await.unwrap(), 0, "proof SQL is still blocked");
                    assert!(!directory.join("rt-auto-close-count").exists());
                    writer.commit().await.unwrap();
                }
            );
            assert_eq!(released, Ok(false));
            assert!(!directory.join("rt-auto-close-count").exists(), "zero actual Close bytes");
            let operation = capacity.automatic.lock().unwrap().clone().unwrap();
            assert_eq!(operation.original, record);
            assert_eq!(operation.record.lock().unwrap().as_ref().unwrap().phase,
                AutomaticReleasePhase::Withdrawn);
            let lifecycle = operation.lifecycle.lock().unwrap().clone().unwrap();
            let ticket = lifecycle.ticket.lock().unwrap().clone().unwrap();
            assert!(matches!(
                crate::application::native_dispatch::PromptTicket::wait(ticket.as_ref()).await,
                crate::application::native_dispatch::TransportFact::Zero { .. }
            ), "the original writer ticket confirms zero bytes, not merely a peer log absence");
            assert!(!capacity.pre.authority.releasing.load(Ordering::Acquire));
            assert_eq!(runtime.inspect_capacity().await.unwrap().occupied, 1);
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while source.pre.authority.required.load(Ordering::Acquire) != 0 {
                tokio::task::yield_now().await;
            }
        }).await.unwrap();
        assert_eq!(core.repository.resolve_session_tree(&child, &core.deadline().unwrap())
            .await.unwrap().root, Some(root));
        let proof: (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT sequence,receipt FROM node_session_parents WHERE child_native_id=?")
                .bind(child.native_id()).fetch_one(storage.pool_for_test()).await.unwrap();
        assert!(proof.0.iter().any(|byte| *byte != 0) && proof.1.iter().any(|byte| *byte != 0));
        eprintln!("RT015 after_first_byte={after_first_byte} actual relation proof sequence={:?} receipt={:?}", proof.0, proof.1);
    }).catch_unwind().await;
    let shutdown = runtime
        .shutdown(super::super::super::Instant::now() + Duration::from_secs(8))
        .await;
    eprintln!(
        "RT015 cleanup={shutdown:?} sources={:?} area={}",
        runtime.source_close_diagnostics(),
        directory.display()
    );
    drop(service);
    drop(runtime);
    storage.close().await;
    scope.stop_and_observe().await;
    assert!(
        shutdown.joined && shutdown.metadata_confirmed,
        "{shutdown:?}"
    );
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
