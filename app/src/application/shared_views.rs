//! In-memory view components. The node read service must authorize every actual chunk.
//!
//! These owned storage objects are not gateway handles, native proofs or a second reader.
pub mod budget;
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
pub mod controller;
pub(crate) mod indexed_journal;
pub mod ingress;
pub mod journal;
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
pub mod probe;
mod projection_wire;
pub mod read_control;
pub(crate) mod read_observation;
pub(crate) mod read_prefix;
pub(crate) mod segments;
pub mod send_epoch;
pub mod source;
pub(crate) mod tasks;
pub use source::{SharedViewOwner, SharedViews, ViewShutdownReport};
#[cfg(windows)]
pub mod local_authority;
#[cfg(windows)]
pub mod local_transfer;
#[cfg(windows)]
pub mod reader;
#[cfg(windows)]
pub mod socket_authority;
#[cfg(windows)]
pub(crate) mod socket_transfer;
#[cfg(windows)]
mod transfer_state;
pub mod wire;

pub type BoxReadFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
pub trait CurrentViewAuthority<C>: Send + Sync + 'static {
    /// # Errors
    /// Rejects a client issued by another read authority.
    fn client_binding(
        &self,
        client: &C,
    ) -> Result<ReadClientBinding, crate::domain::shared_view::ViewError>;
    /// # Errors
    /// Rejects a client that does not belong to this actual read authority.
    #[cfg(windows)]
    fn socket_context(
        &self,
        _client: &C,
    ) -> Result<
        Option<std::sync::Arc<socket_authority::LiveSocketContext>>,
        crate::domain::shared_view::ViewError,
    > {
        Ok(None)
    }
    fn wait_client_change<'a>(
        &'a self,
        client: &'a C,
    ) -> BoxReadFuture<'a, Result<(), crate::domain::shared_view::ViewError>>;
    fn with_current<'a, T, F>(
        &'a self,
        client: &'a C,
        target: &'a crate::domain::native::NativeSessionKey,
        f: F,
    ) -> BoxReadFuture<'a, Result<T, crate::domain::shared_view::ViewError>>
    where
        T: Send + 'a,
        F: FnOnce() -> Result<T, crate::domain::shared_view::ViewError> + Send + 'a;
}
/// Opaque per-reader client binding. It does not grant actor, source or body-read qualification.
#[derive(Clone)]
pub struct ReadClientBinding {
    identity: std::sync::Arc<()>,
}
impl ReadClientBinding {
    #[must_use]
    pub fn same_client(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.identity, &other.identity)
    }
}

/// Sharing must retain the original immutable allocation rather than reserialize its contents.
pub trait RetainedRaw: Clone + Send + Sync + 'static {
    fn len(&self) -> usize;
    fn bytes(&self) -> &[u8];
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
