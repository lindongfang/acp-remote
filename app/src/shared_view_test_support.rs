//! Default-off acceptance support. These constructors create owned test data, never native grants.
#[cfg(windows)]
pub use crate::adapters::acp::shared_view_module::{
    AcpViewModuleFixture, CollectedModuleRecord, FixtureShutdownReport, FixtureSourceConfig,
    FixtureTarget, FixtureTargetOrigin, ModuleViewTransfer,
};
#[cfg(all(windows, feature = "shared-view-acceptance-fixtures"))]
pub use crate::node::shared_view_test_support::{
    AreaObservation, ClosePhase, ErasureSite, FirstCloseObservation, OwnerObservation,
};
#[cfg(windows)]
pub use crate::node::shared_view_test_support::{LocalFixtureViewAuthority, LocalViewAuthFixture};
#[cfg(all(windows, feature = "shared-view-acceptance-fixtures"))]
pub use crate::node::shutdown_observation::{
    NodeShutdownDisposition, NodeShutdownFailure, NodeShutdownFailureFlags, NodeShutdownPhase,
    NodeShutdownSnapshot, NodeShutdownStage, NodeStageOutcome,
};
use crate::{claims::PublicKeySec1, domain::EntityId};
use p256::ecdsa::SigningKey;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    runtime::Handle,
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{Instant, timeout_at},
};
mod wire;
#[cfg(windows)]
pub use wire::ChildWireAdoptError;
pub use wire::{
    AdoptRejected, FixtureIoBoundary, FixtureIoController, FixtureIoFault, FixtureIoGate,
    FixtureIoObservation, FixtureIoPlan, FixtureIoSnapshot, FixturePeer, FixturePeerWire,
    FixtureResourceLease,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FixtureError {
    #[error("fixture area is invalid")]
    InvalidArea,
    #[error("fixture allocation failed")]
    Allocation,
    #[error("fixture random source is unavailable")]
    RandomUnavailable,
    #[error("fixture IO failed")]
    Io,
    #[error("fixture deadline elapsed")]
    Timeout,
    #[error("fixture was cancelled")]
    Cancelled,
    #[error("fixture resource was already adopted")]
    AlreadyAdopted,
    #[error("fixture resource was already taken")]
    AlreadyTaken,
    #[error("fixture controller is busy")]
    ControlBusy,
    #[error("fixture operation is unsupported")]
    Unsupported,
    #[error("fixture resource join is unconfirmed")]
    JoinUnconfirmed,
}

pub struct OwnedTestArea {
    parent: PathBuf,
    path: PathBuf,
    id: EntityId,
    marker: String,
    registry: Arc<AreaRegistry>,
    removed: bool,
}
#[derive(Default)]
pub(crate) struct AreaRegistry {
    resources: AtomicUsize,
    tasks: AtomicUsize,
}
pub struct AreaCleanupReport {
    removed: bool,
    pending: usize,
}
impl AreaCleanupReport {
    #[must_use]
    pub const fn removed(&self) -> bool {
        self.removed
    }
    #[must_use]
    pub const fn pending_resources(&self) -> usize {
        self.pending
    }
}
impl OwnedTestArea {
    /// # Errors
    /// Requires a canonical existing parent and a safe label; creates a unique new child only.
    pub fn create(parent: &Path, label: &str) -> Result<Self, FixtureError> {
        if label.is_empty()
            || label.len() > 48
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(FixtureError::InvalidArea);
        }
        let parent = parent
            .canonicalize()
            .map_err(|_| FixtureError::InvalidArea)?;
        if !parent.is_dir() {
            return Err(FixtureError::InvalidArea);
        }
        loop {
            let bytes =
                crate::random::bytes::<16>().map_err(|_| FixtureError::RandomUnavailable)?;
            let marker = hex::encode(bytes);
            let path = parent.join(format!("{label}-{marker}"));
            match fs::create_dir(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(FixtureError::Io),
            }
            if path.canonicalize().map_err(|_| FixtureError::InvalidArea)? != path {
                return Err(FixtureError::InvalidArea);
            }
            let result = write_new(&path.join(".fixture-owner"), marker.as_bytes());
            if result.is_err() {
                let _ = fs::remove_dir(&path);
                return Err(FixtureError::Io);
            }
            return Ok(Self {
                parent,
                path,
                id: EntityId::from_bytes(bytes),
                marker,
                registry: Arc::default(),
                removed: false,
            });
        }
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    #[must_use]
    pub const fn area_id(&self) -> EntityId {
        self.id
    }
    /// # Errors
    /// Writes fixed new paths only. The real Node TLS loader validates the material later.
    pub fn install_tls(
        &self,
        certificate_pem: &[u8],
        private_key_pem: &[u8],
    ) -> Result<(), FixtureError> {
        self.validate()?;
        write_new(&self.path.join("server.pem"), certificate_pem)?;
        write_new(&self.path.join("server-key.pem"), private_key_pem)
    }
    /// # Errors
    /// Rejects replaced paths and unjoined resources. Drop never removes this directory.
    #[allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "Frozen A10 async cleanup signature; actual resource waiting occurs in the registered owner before this filesystem step"
    )]
    pub async fn cleanup(&mut self) -> Result<AreaCleanupReport, FixtureError> {
        if self.removed {
            return Ok(AreaCleanupReport {
                removed: true,
                pending: 0,
            });
        }
        let pending = self
            .registry
            .resources
            .load(Ordering::Acquire)
            .saturating_add(self.registry.tasks.load(Ordering::Acquire));
        if pending != 0 {
            return Ok(AreaCleanupReport {
                removed: false,
                pending,
            });
        }
        self.validate()?;
        reject_links(&self.path)?;
        for attempt in 0..20_u8 {
            match fs::remove_dir_all(&self.path) {
                Ok(()) => break,
                Err(_) if attempt < 19 => {
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                Err(_) => return Err(FixtureError::Io),
            }
        }
        self.removed = true;
        Ok(AreaCleanupReport {
            removed: true,
            pending: 0,
        })
    }
    pub(crate) fn validate(&self) -> Result<(), FixtureError> {
        if self.removed
            || self
                .path
                .canonicalize()
                .map_err(|_| FixtureError::InvalidArea)?
                != self.path
            || self.path.parent() != Some(self.parent.as_path())
            || fs::read(self.path.join(".fixture-owner")).map_err(|_| FixtureError::InvalidArea)?
                != self.marker.as_bytes()
        {
            return Err(FixtureError::InvalidArea);
        }
        reject_link(&self.path)
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), FixtureError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| FixtureError::Io)?;
    file.write_all(bytes).map_err(|_| FixtureError::Io)
}
fn reject_link(path: &Path) -> Result<(), FixtureError> {
    let meta = fs::symlink_metadata(path).map_err(|_| FixtureError::InvalidArea)?;
    if meta.file_type().is_symlink() {
        return Err(FixtureError::InvalidArea);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(FixtureError::InvalidArea);
        }
    }
    Ok(())
}
fn reject_links(path: &Path) -> Result<(), FixtureError> {
    reject_link(path)?;
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| FixtureError::Io)? {
            reject_links(&entry.map_err(|_| FixtureError::Io)?.path())?;
        }
    }
    Ok(())
}

/// Holds one newly generated test signing key. No Debug, Serialize or trusted client ID.
pub struct FixtureSigningClient {
    pub(crate) key: SigningKey,
    public: PublicKeySec1,
}
impl FixtureSigningClient {
    /// # Errors
    /// Returns a static random-source error. Invalid random scalars are sampled again.
    pub fn generate() -> Result<Self, FixtureError> {
        loop {
            let seed = zeroize::Zeroizing::new(
                crate::random::bytes::<32>().map_err(|_| FixtureError::RandomUnavailable)?,
            );
            let Ok(key) = SigningKey::from_slice(seed.as_ref()) else {
                continue;
            };
            let bytes = key
                .verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .map_err(|_| FixtureError::Io)?;
            let public = PublicKeySec1::new(bytes).map_err(|_| FixtureError::Io)?;
            return Ok(Self { key, public });
        }
    }
    #[must_use]
    pub const fn public_key(&self) -> PublicKeySec1 {
        self.public
    }
}

pub struct FixtureTaskOwner {
    inner: Arc<OwnerCore>,
}
#[derive(Clone)]
pub struct FixtureOwnerHandle {
    inner: Arc<OwnerCore>,
}
struct OwnerCore {
    runtime: Handle,
    area: Arc<AreaRegistry>,
    stop: watch::Sender<bool>,
    tasks: Mutex<Vec<Arc<TaskSlot>>>,
    resources: Mutex<Vec<Arc<dyn wire::WireResource>>>,
    released: AtomicBool,
}
struct TaskSlot {
    join: tokio::sync::Mutex<Option<JoinHandle<Result<(), FixtureError>>>>,
    joined: AtomicBool,
    result: Mutex<Option<Result<(), FixtureError>>>,
}
pub struct FixtureOwnerReport {
    tasks: usize,
    resources: usize,
    failure: Option<FixtureError>,
}
impl FixtureOwnerReport {
    #[must_use]
    pub const fn pending_tasks(&self) -> usize {
        self.tasks
    }
    #[must_use]
    pub const fn pending_resources(&self) -> usize {
        self.resources
    }
    #[must_use]
    pub const fn all_joined(&self) -> bool {
        self.tasks == 0 && self.resources == 0 && self.failure.is_none()
    }
    #[must_use]
    pub const fn failure(&self) -> Option<FixtureError> {
        self.failure
    }
}
impl FixtureTaskOwner {
    /// # Errors
    /// Rejects a replaced/removed area. The supplied runtime is retained for Drop cleanup.
    pub fn new(area: &OwnedTestArea, runtime: Handle) -> Result<Self, FixtureError> {
        area.validate()?;
        let (stop, _) = watch::channel(false);
        let inner = Arc::new(OwnerCore {
            runtime,
            area: Arc::clone(&area.registry),
            stop,
            tasks: Mutex::new(Vec::new()),
            resources: Mutex::new(Vec::new()),
            released: AtomicBool::new(false),
        });
        inner.area.resources.fetch_add(1, Ordering::AcqRel);
        Ok(Self { inner })
    }
    #[must_use]
    pub fn handle(&self) -> FixtureOwnerHandle {
        FixtureOwnerHandle {
            inner: Arc::clone(&self.inner),
        }
    }
    pub async fn shutdown(&self, deadline: Instant) -> FixtureOwnerReport {
        self.inner.shutdown(deadline).await
    }
}
impl Drop for FixtureTaskOwner {
    fn drop(&mut self) {
        self.inner.stop.send_replace(true);
        let inner = Arc::clone(&self.inner);
        // The captured Handle is safe without an ambient runtime. Failure to run cleanup
        // leaves the area resource registered; it never manufactures a successful join.
        self.inner.runtime.spawn(async move {
            let deadline = Instant::now() + std::time::Duration::from_secs(15);
            let _ = inner.shutdown(deadline).await;
        });
    }
}
impl OwnerCore {
    async fn shutdown(&self, deadline: Instant) -> FixtureOwnerReport {
        self.stop.send_replace(true);
        let resources = match self.resources.lock() {
            Ok(resources) => resources.clone(),
            Err(_) => {
                return FixtureOwnerReport {
                    tasks: 1,
                    resources: 1,
                    failure: Some(FixtureError::JoinUnconfirmed),
                };
            }
        };
        let mut failure = None;
        for resource in &resources {
            if !resource.shutdown(deadline).await {
                failure = Some(FixtureError::JoinUnconfirmed);
            }
        }
        let slots = match self.tasks.lock() {
            Ok(slots) => slots.clone(),
            Err(_) => {
                return FixtureOwnerReport {
                    tasks: 1,
                    resources: 1,
                    failure: Some(FixtureError::JoinUnconfirmed),
                };
            }
        };
        for slot in &slots {
            let joined = join_registered_slot(slot, &self.area, deadline).await;
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(error)) => failure = Some(error),
                Err(_) => failure = Some(FixtureError::JoinUnconfirmed),
            }
        }
        let tasks = slots
            .iter()
            .filter(|slot| !slot.joined.load(Ordering::Acquire))
            .count();
        let pending = resources
            .iter()
            .filter(|resource| !resource.closed())
            .count();
        if tasks == 0 && pending == 0 && !self.released.swap(true, Ordering::AcqRel) {
            self.area.resources.fetch_sub(1, Ordering::AcqRel);
        }
        FixtureOwnerReport {
            tasks,
            resources: pending,
            failure,
        }
    }
}

impl FixtureOwnerHandle {
    pub(crate) fn subscribe_stop(&self) -> watch::Receiver<bool> {
        self.inner.stop.subscribe()
    }
    pub(crate) fn spawn<F>(&self, work: F) -> Result<(), FixtureError>
    where
        F: std::future::Future<Output = Result<(), FixtureError>> + Send + 'static,
    {
        self.spawn_registered(work).map(|_| ())
    }
    fn spawn_registered<F>(&self, work: F) -> Result<Arc<TaskSlot>, FixtureError>
    where
        F: std::future::Future<Output = Result<(), FixtureError>> + Send + 'static,
    {
        let mut slots = self
            .inner
            .tasks
            .lock()
            .map_err(|_| FixtureError::JoinUnconfirmed)?;
        if self.inner.released.load(Ordering::Acquire) || *self.inner.stop.borrow() {
            return Err(FixtureError::Cancelled);
        }
        slots.try_reserve(1).map_err(|_| FixtureError::Allocation)?;
        let (start, ready) = oneshot::channel();
        let task = self.inner.runtime.spawn(async move {
            ready.await.map_err(|_| FixtureError::Cancelled)?;
            work.await
        });
        self.inner.area.tasks.fetch_add(1, Ordering::AcqRel);
        let slot = Arc::new(TaskSlot {
            join: tokio::sync::Mutex::new(Some(task)),
            joined: AtomicBool::new(false),
            result: Mutex::new(None),
        });
        slots.push(Arc::clone(&slot));
        drop(slots);
        let _ = start.send(());
        Ok(slot)
    }
}
async fn join_registered_slot(
    slot: &TaskSlot,
    area: &AreaRegistry,
    deadline: Instant,
) -> Result<Result<(), FixtureError>, tokio::time::error::Elapsed> {
    timeout_at(deadline, async {
        let mut handle = slot.join.lock().await;
        if slot.joined.load(Ordering::Acquire) {
            return slot
                .result
                .lock()
                .map_err(|_| FixtureError::JoinUnconfirmed)?
                .unwrap_or(Err(FixtureError::JoinUnconfirmed));
        }
        let Some(task) = handle.as_mut() else {
            return Err(FixtureError::JoinUnconfirmed);
        };
        let result = task
            .await
            .map_err(|_| FixtureError::JoinUnconfirmed)
            .and_then(|result| result);
        handle.take();
        *slot
            .result
            .lock()
            .map_err(|_| FixtureError::JoinUnconfirmed)? = Some(result);
        slot.joined.store(true, Ordering::Release);
        area.tasks.fetch_sub(1, Ordering::AcqRel);
        drop(handle);
        result
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shared_view_fixture_area_owner_real_join_and_cancelled_wait_keep_resource() {
        let mut area = OwnedTestArea::create(&std::env::temp_dir(), "shared-view-owner")
            .expect("owned unique child");
        area.install_tls(b"synthetic cert", b"synthetic key")
            .expect("create_new inputs");
        assert_eq!(area.install_tls(b"other", b"other"), Err(FixtureError::Io));
        let owner = FixtureTaskOwner::new(&area, Handle::current()).expect("registered owner");
        let (release, wait) = oneshot::channel();
        let (arrived, ready) = oneshot::channel();
        owner
            .handle()
            .spawn(async move {
                let _ = arrived.send(());
                wait.await.map_err(|_| FixtureError::Cancelled)
            })
            .expect("registered task starts only after handle stored");
        ready.await.expect("real task started");
        assert!(
            !area
                .cleanup()
                .await
                .expect("registered resource prevents removal")
                .removed()
        );
        let first = owner
            .shutdown(Instant::now() + std::time::Duration::from_millis(1))
            .await;
        assert_eq!(first.pending_tasks(), 1);
        assert!(!first.all_joined());
        release.send(()).expect("same running task");
        let second = owner
            .shutdown(Instant::now() + std::time::Duration::from_secs(1))
            .await;
        assert!(second.all_joined());
        assert!(
            area.cleanup()
                .await
                .expect("only after actual join")
                .removed()
        );
    }
    #[test]
    fn shared_view_fixture_signing_key_is_real_and_area_labels_cannot_escape() {
        assert!(matches!(
            OwnedTestArea::create(&std::env::temp_dir(), "../escape"),
            Err(FixtureError::InvalidArea)
        ));
        let client = FixtureSigningClient::generate().expect("actual random signing key");
        assert_eq!(
            client.key.verifying_key().to_sec1_point(false).as_bytes(),
            client.public_key().as_bytes()
        );
    }
}
