//! Input admission port. The execution owner dispatches only newly accepted work.
#[cfg(windows)]
pub(crate) mod document_disposal;
#[cfg(windows)]
pub(crate) mod document_source;
#[cfg(windows)]
pub(crate) mod document_transfer;

pub mod approval_owner;
#[cfg(test)]
pub mod approvals;
#[cfg(test)]
pub mod history;
pub mod native_catalog;
pub mod native_dispatch;
#[cfg(test)]
pub mod product_commands;
#[cfg(windows)]
pub(crate) mod prompt_preparation;
pub mod queue_owner;
pub mod request_registry;
pub mod shared_views;
#[cfg(test)]
use crate::domain::{ActorContext, Admission, InputRequest, SessionError};
#[cfg(test)]
use std::future::Future;

#[cfg(test)]
pub trait InputRepository: Send + Sync {
    fn admit(
        &self,
        actor: ActorContext,
        request: InputRequest,
    ) -> impl Future<Output = Result<Admission, SessionError>> + Send;
}

#[cfg(test)]
pub struct InputService<R> {
    repository: R,
}
#[cfg(test)]
impl<R: InputRepository> InputService<R> {
    #[must_use]
    pub const fn new(repository: R) -> Self {
        Self { repository }
    }
    /// Commits acceptance; it neither starts a process nor retries a dispatch.
    /// # Errors
    /// Rejects an invalid actor, conflicting request, busy session or failed transaction.
    pub async fn submit(
        &self,
        actor: ActorContext,
        request: InputRequest,
    ) -> Result<Admission, SessionError> {
        self.repository.admit(actor, request).await
    }
}

#[cfg(windows)]
pub(crate) mod catalog_preparation;
