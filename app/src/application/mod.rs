//! Input admission port. The execution owner dispatches only newly accepted work.
pub mod approvals;
pub mod history;
use crate::domain::{ActorContext, Admission, InputRequest, SessionError};
use std::future::Future;

pub trait InputRepository: Send + Sync {
    fn admit(
        &self,
        actor: ActorContext,
        request: InputRequest,
    ) -> impl Future<Output = Result<Admission, SessionError>> + Send;
}

pub struct InputService<R> {
    repository: R,
}
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
