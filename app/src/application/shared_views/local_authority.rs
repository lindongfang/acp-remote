//! Local qualification uses the actual policy and its exact trust store. No socket identity is minted.
use super::{BoxReadFuture, CurrentViewAuthority, ReadClientBinding};
use crate::{
    access_policy::{AccessPolicyError, ApplicationPolicy, AuthenticatedActor},
    domain::{
        native::{NativeSessionKey, NativeSourceId},
        shared_view::ViewError,
    },
    trust::TrustStore,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;
pub(crate) struct CurrentLocalReadGuard {
    _gate: tokio::sync::OwnedMutexGuard<()>,
    binding: ReadClientBinding,
    target: NativeSessionKey,
}
impl CurrentLocalReadGuard {
    pub(crate) fn matches(&self, binding: &ReadClientBinding, target: &NativeSessionKey) -> bool {
        self.binding.same_client(binding) && self.target == *target
    }
}

struct Issuer {
    closed: AtomicBool,
    send_fence: std::sync::Mutex<()>,
    wake: Notify,
    #[cfg(test)]
    wait_ready: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    wait_ready_notify: Notify,
}
pub struct AuthenticatedLocalViewClient {
    actor: AuthenticatedActor,
    issuer: Arc<Issuer>,
    identity: Arc<()>,
}
impl AuthenticatedLocalViewClient {
    #[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
    pub(crate) fn owned_task_ref(&self) -> Self {
        Self {
            actor: self.actor.clone(),
            issuer: Arc::clone(&self.issuer),
            identity: Arc::clone(&self.identity),
        }
    }
    #[must_use]
    pub fn client_id(&self) -> &str {
        self.actor.client_id()
    }
}
pub struct LocalViewAuthority {
    policy: Arc<ApplicationPolicy>,
    trust: TrustStore,
    issuer: Arc<Issuer>,
    sources: std::sync::RwLock<Vec<NativeSourceId>>,
}
impl LocalViewAuthority {
    pub(crate) fn native_actor<'a>(
        &self,
        client: &'a AuthenticatedLocalViewClient,
    ) -> Result<&'a AuthenticatedActor, ViewError> {
        self.check_issuer(client)?;
        Ok(&client.actor)
    }
    pub(crate) fn register_native_source<
        P: crate::application::native_dispatch::NativeApprovalPort,
    >(
        &self,
        capability: &crate::application::native_dispatch::SourceReadCapability<P>,
    ) -> Result<(), ViewError> {
        capability.require_current()?;
        self.register_verified_source(capability.native().source())
    }
    pub(crate) fn register_verified_source(&self, source: NativeSourceId) -> Result<(), ViewError> {
        let mut sources = self.sources.write().map_err(|_| ViewError::ReadFailure)?;
        if !sources.contains(&source) {
            sources
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            sources.push(source);
        }
        drop(sources);
        Ok(())
    }
    pub(crate) fn from_current_policy(
        policy: Arc<ApplicationPolicy>,
        trust: TrustStore,
        sources: Vec<NativeSourceId>,
    ) -> Self {
        Self {
            policy,
            trust,
            issuer: Arc::new(Issuer {
                closed: AtomicBool::new(false),
                send_fence: std::sync::Mutex::new(()),
                wake: Notify::new(),
                #[cfg(test)]
                wait_ready: std::sync::atomic::AtomicU64::new(0),
                #[cfg(test)]
                wait_ready_notify: Notify::new(),
            }),
            sources: std::sync::RwLock::new(sources),
        }
    }
    /// Accepts only a policy-authenticated actor that is current on this exact node.
    /// # Errors
    /// Rejects an actor from another node, a stale/revoked actor, or unavailable policy state.
    pub async fn accept_actor(
        &self,
        actor: AuthenticatedActor,
    ) -> Result<AuthenticatedLocalViewClient, ViewError> {
        let gate = self.trust.client_gate(actor.client_id());
        let _guard = gate.lock().await;
        if self.issuer.closed.load(Ordering::Acquire) {
            return Err(ViewError::NodeStopping);
        }
        self.policy
            .require_current_actor(&actor)
            .await
            .map_err(policy_error)?;
        Ok(AuthenticatedLocalViewClient {
            actor,
            issuer: Arc::clone(&self.issuer),
            identity: Arc::new(()),
        })
    }
    pub fn close(&self) {
        let fence = self.issuer.send_fence.lock();
        self.issuer.closed.store(true, Ordering::Release);
        drop(fence);
        self.issuer.wake.notify_waiters();
    }
    pub(crate) fn with_live_issuer<T>(
        &self,
        client: &AuthenticatedLocalViewClient,
        f: impl FnOnce() -> Result<T, ViewError>,
    ) -> Result<T, ViewError> {
        let _fence = self
            .issuer
            .send_fence
            .lock()
            .map_err(|_| ViewError::ReadFailure)?;
        self.check_issuer(client)?;
        f()
    }
    pub(crate) async fn current_guard(
        &self,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
    ) -> Result<CurrentLocalReadGuard, ViewError> {
        self.check_issuer(client)?;
        let gate = self
            .trust
            .client_gate(client.actor.client_id())
            .lock_owned()
            .await;
        self.check_issuer(client)?;
        self.policy
            .require_current_actor(&client.actor)
            .await
            .map_err(policy_error)?;
        self.check_issuer(client)?;
        if !self
            .sources
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .contains(&target.source())
        {
            return Err(ViewError::InvalidTarget);
        }
        Ok(CurrentLocalReadGuard {
            _gate: gate,
            binding: self.client_binding(client)?,
            target: target.clone(),
        })
    }
    pub(crate) async fn try_current_guard(
        &self,
        client: &AuthenticatedLocalViewClient,
        target: &NativeSessionKey,
    ) -> Result<Option<CurrentLocalReadGuard>, ViewError> {
        self.check_issuer(client)?;
        let Ok(gate) = self
            .trust
            .client_gate(client.actor.client_id())
            .try_lock_owned()
        else {
            return Ok(None);
        };
        self.check_issuer(client)?;
        self.policy
            .require_current_actor(&client.actor)
            .await
            .map_err(policy_error)?;
        self.check_issuer(client)?;
        if !self
            .sources
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .contains(&target.source())
        {
            return Err(ViewError::InvalidTarget);
        }
        Ok(Some(CurrentLocalReadGuard {
            _gate: gate,
            binding: self.client_binding(client)?,
            target: target.clone(),
        }))
    }
    #[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
    pub(crate) fn register_module_source<S, R>(
        &self,
        views: &super::SharedViews<S, R>,
    ) -> Result<(), ViewError> {
        if views.core.probe.is_sealed() {
            return Err(ViewError::SourceReleased);
        }
        let mut sources = self.sources.write().map_err(|_| ViewError::ReadFailure)?;
        if !sources.contains(&views.core.source) {
            sources
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            sources.push(views.core.source);
        }
        drop(sources);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test_registered_waits(&self) -> u64 {
        self.issuer.wait_ready.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub(crate) async fn test_wait_until_registered(&self) {
        loop {
            let n = self.issuer.wait_ready_notify.notified();
            tokio::pin!(n);
            n.as_mut().enable();
            if self.test_registered_waits() > 0 {
                return;
            }
            n.await;
        }
    }
    fn check_issuer(&self, client: &AuthenticatedLocalViewClient) -> Result<(), ViewError> {
        if !Arc::ptr_eq(&self.issuer, &client.issuer) {
            return Err(ViewError::InvalidClient);
        }
        if self.issuer.closed.load(Ordering::Acquire) {
            return Err(ViewError::NodeStopping);
        }
        Ok(())
    }
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Consumes the actual policy error directly in Result::map_err and emits only a static view category"
)]
fn policy_error(error: AccessPolicyError) -> ViewError {
    match error {
        AccessPolicyError::InvalidActor => ViewError::Revoked,
        AccessPolicyError::NodeUnavailable => ViewError::NodeStopping,
        _ => ViewError::ReadFailure,
    }
}
impl CurrentViewAuthority<AuthenticatedLocalViewClient> for LocalViewAuthority {
    fn client_binding(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<ReadClientBinding, ViewError> {
        self.check_issuer(client)?;
        Ok(ReadClientBinding {
            identity: Arc::clone(&client.identity),
        })
    }
    fn with_current<'a, T, F>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: &'a NativeSessionKey,
        f: F,
    ) -> BoxReadFuture<'a, Result<T, ViewError>>
    where
        T: Send + 'a,
        F: FnOnce() -> Result<T, ViewError> + Send + 'a,
    {
        Box::pin(async move {
            let _guard = self.current_guard(client, target).await?;
            self.with_live_issuer(client, f)
        })
    }
    fn wait_client_change<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
    ) -> BoxReadFuture<'a, Result<(), ViewError>> {
        Box::pin(async move {
            let mut changes = self.trust.revision_receiver();
            let closed = self.issuer.wake.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            self.check_issuer(client)?;
            self.policy
                .require_current_actor(&client.actor)
                .await
                .map_err(policy_error)?;
            #[cfg(test)]
            {
                self.issuer.wait_ready.fetch_add(1, Ordering::Release);
                self.issuer.wait_ready_notify.notify_waiters();
            }
            tokio::select! { result=changes.changed()=>result.map_err(|_|ViewError::ReadFailure)?,()=closed=>{} }
            self.check_issuer(client)?;
            self.policy
                .require_current_actor(&client.actor)
                .await
                .map_err(policy_error)
        })
    }
}
