//! Cohesive queue/native runtime claim and shutdown supervision for [`super::Storage`].
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicBool, Ordering},
};

use crate::application::queue_owner::RuntimeSupervisor;

#[cfg(windows)]
use crate::{
    application::{
        catalog_preparation::{CatalogShutdownSlot, CatalogUploadShutdownPort},
        document_disposal::{DocumentShutdownPort, DocumentShutdownSlot},
        prompt_preparation::{PreparationShutdownSlot, PromptPreparationShutdownPort},
        queue_owner::QueueRuntimeIdentity,
    },
    domain::request_registry::RegistryError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ClaimFailure {
    Unavailable,
    Stopping,
    AlreadyStarted,
}

pub(super) struct StorageRuntimeCoordinator {
    queue_runtime_claim: Arc<AtomicBool>,
    queue_supervisor: Mutex<Option<Arc<dyn RuntimeSupervisor>>>,
    queue_close: tokio::sync::Mutex<()>,
    queue_lifecycle: Mutex<()>,
    queue_closed: AtomicBool,
    #[cfg(windows)]
    preparation_shutdown: Arc<PreparationShutdownSlot>,
    #[cfg(windows)]
    document_shutdown: Arc<DocumentShutdownSlot>,
    #[cfg(windows)]
    catalog_shutdown: Arc<CatalogShutdownSlot>,
}

pub(super) struct RuntimeClaimContext<'a> {
    coordinator: &'a StorageRuntimeCoordinator,
    installed: bool,
}

impl RuntimeClaimContext<'_> {
    pub(super) fn claim(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.coordinator.queue_runtime_claim)
    }

    #[cfg(windows)]
    pub(super) fn preparation(&self) -> Arc<PreparationShutdownSlot> {
        Arc::clone(&self.coordinator.preparation_shutdown)
    }

    #[cfg(windows)]
    pub(super) fn document(&self) -> Arc<DocumentShutdownSlot> {
        Arc::clone(&self.coordinator.document_shutdown)
    }

    #[cfg(windows)]
    pub(super) fn catalog(&self) -> Arc<CatalogShutdownSlot> {
        Arc::clone(&self.coordinator.catalog_shutdown)
    }

    pub(super) fn install_supervisor(
        &mut self,
        supervisor: Arc<dyn RuntimeSupervisor>,
    ) -> Result<(), ClaimFailure> {
        if let Ok(mut slot) = self.coordinator.queue_supervisor.lock() {
            *slot = Some(supervisor);
            self.installed = true;
            Ok(())
        } else {
            self.coordinator
                .queue_runtime_claim
                .store(false, Ordering::SeqCst);
            Err(ClaimFailure::Unavailable)
        }
    }
}

impl StorageRuntimeCoordinator {
    pub(super) fn new() -> Self {
        Self {
            queue_runtime_claim: Arc::new(AtomicBool::new(false)),
            queue_supervisor: Mutex::new(None),
            queue_close: tokio::sync::Mutex::new(()),
            queue_lifecycle: Mutex::new(()),
            queue_closed: AtomicBool::new(false),
            #[cfg(windows)]
            preparation_shutdown: Arc::new(PreparationShutdownSlot::default()),
            #[cfg(windows)]
            document_shutdown: Arc::new(DocumentShutdownSlot::default()),
            #[cfg(windows)]
            catalog_shutdown: Arc::new(CatalogShutdownSlot::default()),
        }
    }

    pub(super) fn claim_runtime<T, E>(
        &self,
        map_claim_err: impl FnOnce(ClaimFailure) -> E,
        run: impl FnOnce(&mut RuntimeClaimContext<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let Ok(_lifecycle) = self.queue_lifecycle.lock() else {
            return Err(map_claim_err(ClaimFailure::Unavailable));
        };
        if self.queue_closed.load(Ordering::SeqCst) {
            return Err(map_claim_err(ClaimFailure::Stopping));
        }
        if self
            .queue_runtime_claim
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(map_claim_err(ClaimFailure::AlreadyStarted));
        }
        let mut context = RuntimeClaimContext {
            coordinator: self,
            installed: false,
        };
        let result = run(&mut context);
        if result.is_err() && !context.installed {
            self.queue_runtime_claim.store(false, Ordering::SeqCst);
        }
        result
    }

    #[cfg(windows)]
    fn with_supervisor<T>(
        &self,
        action: impl FnOnce(&dyn RuntimeSupervisor) -> Result<T, RegistryError>,
    ) -> Result<T, RegistryError> {
        let _lifecycle = self
            .queue_lifecycle
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?;
        if self.queue_closed.load(Ordering::SeqCst) {
            return Err(RegistryError::NodeStopping);
        }
        let supervisor = self
            .queue_supervisor
            .lock()
            .map_err(|_| RegistryError::StorageUnavailable)?
            .clone()
            .ok_or(RegistryError::NodeStarting)?;
        action(supervisor.as_ref())
    }

    #[cfg(windows)]
    pub(super) fn register_preparation(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn PromptPreparationShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.with_supervisor(|supervisor| {
            let mut receipt = Some(receipt);
            supervisor.register_preparation(expected, &mut || {
                self.preparation_shutdown
                    .install(receipt.take().ok_or(RegistryError::StorageUnavailable)?)
            })
        })
    }

    #[cfg(windows)]
    pub(super) fn preparation_receipt(&self) -> Option<Arc<dyn PromptPreparationShutdownPort>> {
        self.preparation_shutdown.get()
    }

    #[cfg(windows)]
    pub(super) fn register_document(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn DocumentShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.with_supervisor(|supervisor| {
            let mut receipt = Some(receipt);
            supervisor.register_document(expected, &mut || {
                self.document_shutdown
                    .install(receipt.take().ok_or(RegistryError::StorageUnavailable)?)
            })
        })
    }

    #[cfg(windows)]
    pub(super) fn document_receipt(&self) -> Option<Arc<dyn DocumentShutdownPort>> {
        self.document_shutdown.get()
    }

    #[cfg(windows)]
    pub(super) fn register_catalog(
        &self,
        expected: &QueueRuntimeIdentity,
        receipt: Arc<dyn CatalogUploadShutdownPort>,
    ) -> Result<(), RegistryError> {
        self.with_supervisor(|supervisor| {
            let mut receipt = Some(receipt);
            supervisor.register_catalog(expected, &mut || {
                self.catalog_shutdown
                    .install(receipt.take().ok_or(RegistryError::StorageUnavailable)?)
            })
        })
    }

    #[cfg(windows)]
    pub(super) fn catalog_receipt(&self) -> Option<Arc<dyn CatalogUploadShutdownPort>> {
        self.catalog_shutdown.get()
    }

    /// Synchronously seals installation on the original runtime lifecycle fence.
    pub(super) fn begin_shutdown(&self) {
        let supervisor = {
            let _lifecycle = self
                .queue_lifecycle
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if self.queue_closed.swap(true, Ordering::SeqCst) {
                None
            } else {
                self.queue_supervisor
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone()
            }
        };
        if let Some(supervisor) = supervisor {
            supervisor.cancel();
        }
    }

    pub(super) async fn close_supervisor_and_join(&self) {
        self.begin_shutdown();
        let _closing = self.queue_close.lock().await;
        let supervisor = self
            .queue_supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(supervisor) = supervisor {
            supervisor.stop_and_join().await;
            // Keep ownership across the await (including close cancellation), then release
            // the same supervisor only after its actual task join has completed.
            let mut slot = self
                .queue_supervisor
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &supervisor))
            {
                *slot = None;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn runtime_registration_for_test(&self) -> (bool, Option<usize>) {
        let _lifecycle = self
            .queue_lifecycle
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let slot = self
            .queue_supervisor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (
            self.queue_runtime_claim.load(Ordering::SeqCst),
            slot.as_ref()
                .map(|owner| Arc::as_ptr(owner).cast::<()>() as usize),
        )
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::{future::Future, pin::Pin, time::Duration};
    use tokio::sync::Notify;

    pub(in super::super) struct RecordingSupervisor {
        events: Mutex<Vec<&'static str>>,
        pub(in super::super) join_entered: Notify,
        pub(in super::super) release_join: Notify,
        claim: Arc<AtomicBool>,
    }

    impl RecordingSupervisor {
        pub(in super::super) fn new(claim: Arc<AtomicBool>) -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(Vec::new()),
                join_entered: Notify::new(),
                release_join: Notify::new(),
                claim,
            })
        }

        pub(in super::super) fn events(&self) -> Vec<&'static str> {
            self.events.lock().unwrap().clone()
        }
    }

    impl RuntimeSupervisor for RecordingSupervisor {
        #[cfg(windows)]
        fn queue_runtime_identity(&self) -> QueueRuntimeIdentity {
            unreachable!("queue_runtime_identity unused in coordinator unit tests")
        }

        #[cfg(windows)]
        fn register_preparation(
            &self,
            _expected: &QueueRuntimeIdentity,
            _install: &mut dyn FnMut() -> Result<(), RegistryError>,
        ) -> Result<(), RegistryError> {
            Ok(())
        }

        fn cancel(&self) {
            self.events.lock().unwrap().push("cancel");
        }

        fn stop_and_join(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(async move {
                self.events.lock().unwrap().push("stop_and_join_start");
                self.join_entered.notify_one();
                self.release_join.notified().await;
                self.claim.store(false, Ordering::SeqCst);
                self.events.lock().unwrap().push("stop_and_join_finish");
            })
        }
    }

    #[test]
    fn duplicate_claim_is_rejected_and_setup_error_releases_claim() {
        let coordinator = StorageRuntimeCoordinator::new();

        let failed: Result<(), ClaimFailure> =
            coordinator.claim_runtime(|err| err, |_| Err(ClaimFailure::Unavailable));
        assert_eq!(failed, Err(ClaimFailure::Unavailable));
        assert_eq!(coordinator.runtime_registration_for_test(), (false, None));

        let first = coordinator.claim_runtime(
            |err| err,
            |ctx| {
                let supervisor = RecordingSupervisor::new(ctx.claim());
                ctx.install_supervisor(supervisor)?;
                Ok(())
            },
        );
        assert!(first.is_ok());
        let (claimed, slot) = coordinator.runtime_registration_for_test();
        assert!(claimed);
        assert!(slot.is_some());

        let duplicate = coordinator.claim_runtime(|err| err, |_| Ok(()));
        assert_eq!(duplicate, Err(ClaimFailure::AlreadyStarted));
    }

    #[tokio::test]
    async fn shutdown_cancels_joins_before_clearing_and_blocks_new_claims() {
        let coordinator = Arc::new(StorageRuntimeCoordinator::new());
        let supervisor = coordinator
            .claim_runtime(
                |err| err,
                |ctx| {
                    let supervisor = RecordingSupervisor::new(ctx.claim());
                    ctx.install_supervisor(Arc::clone(&supervisor) as Arc<dyn RuntimeSupervisor>)?;
                    Ok(supervisor)
                },
            )
            .expect("initial claim succeeds");

        let close_task = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move {
                coordinator.close_supervisor_and_join().await;
            })
        };

        tokio::time::timeout(Duration::from_secs(2), supervisor.join_entered.notified())
            .await
            .expect("cancel triggered stop_and_join");

        assert_eq!(supervisor.events(), vec!["cancel", "stop_and_join_start"]);
        assert!(coordinator.runtime_registration_for_test().1.is_some());
        assert_eq!(
            coordinator.claim_runtime(|err| err, |_| Ok(())),
            Err(ClaimFailure::Stopping)
        );

        supervisor.release_join.notify_one();
        tokio::time::timeout(Duration::from_secs(2), close_task)
            .await
            .expect("close finished")
            .expect("close task did not panic");

        assert_eq!(
            supervisor.events(),
            vec!["cancel", "stop_and_join_start", "stop_and_join_finish"]
        );
        assert_eq!(coordinator.runtime_registration_for_test(), (false, None));
    }
}
