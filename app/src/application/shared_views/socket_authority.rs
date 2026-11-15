#![allow(
    clippy::significant_drop_tightening,
    reason = "The socket lifetime lock must cover each short synchronous qualification and stream registration fence"
)]
//! Socket read qualification minted only after the original management handshake.
use super::{BoxReadFuture, CurrentViewAuthority, ReadClientBinding};
use crate::{
    access_policy::{AccessPolicyError, ApplicationPolicy, AuthenticatedActor},
    claims::Id128,
    domain::{
        native::{NativeSessionKey, NativeSourceId},
        shared_view::ViewError,
    },
    trust::TrustStore,
};
use std::sync::{Arc, Mutex, RwLock, Weak};
use tokio::sync::Notify;

pub(crate) trait SocketLifetimeParticipant: Send + Sync {
    fn seal(&self);
}
pub(crate) struct SocketLifetime {
    live: Mutex<bool>,
    changed: Notify,
    streams: Mutex<Vec<Weak<super::socket_transfer::Stream>>>,
    participants: Mutex<Vec<Weak<dyn SocketLifetimeParticipant>>>,
}
impl SocketLifetime {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            live: Mutex::new(true),
            changed: Notify::new(),
            streams: Mutex::new(Vec::new()),
            participants: Mutex::new(Vec::new()),
        })
    }
    pub(crate) fn with_live<T>(
        &self,
        f: impl FnOnce() -> Result<T, ViewError>,
    ) -> Result<T, ViewError> {
        let live = self.live.lock().map_err(|_| ViewError::ReadFailure)?;
        if !*live {
            return Err(ViewError::StreamSealed);
        }
        f()
    }
    pub(crate) fn register(
        &self,
        stream: &Arc<super::socket_transfer::Stream>,
    ) -> Result<(), ViewError> {
        self.with_live(|| {
            let mut streams = self.streams.lock().map_err(|_| ViewError::ReadFailure)?;
            streams.retain(|s| s.strong_count() > 0);
            streams
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            streams.push(Arc::downgrade(stream));
            Ok(())
        })
    }
    pub(crate) fn register_participant(
        &self,
        participant: Weak<dyn SocketLifetimeParticipant>,
    ) -> Result<(), ViewError> {
        self.with_live(|| {
            let mut participants = self
                .participants
                .lock()
                .map_err(|_| ViewError::ReadFailure)?;
            participants.retain(|entry| entry.strong_count() > 0);
            participants
                .try_reserve(1)
                .map_err(|_| ViewError::AllocationFailure)?;
            participants.push(participant);
            Ok(())
        })
    }
    pub(crate) fn seal(&self) {
        if let Ok(mut live) = self.live.lock() {
            *live = false;
        }
        let streams = match self.streams.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        for stream in streams.iter().filter_map(Weak::upgrade) {
            let _ = stream.seal();
        }
        drop(streams);
        let participants = self
            .participants
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for participant in participants.iter().filter_map(Weak::upgrade) {
            participant.seal();
        }
        drop(participants);
        self.changed.notify_waiters();
    }
}
/// This value cannot be constructed from an actor or a client identifier.
pub struct SocketViewClient {
    pub(crate) context: Arc<LiveSocketContext>,
}
pub struct LiveSocketContext {
    actor: AuthenticatedActor,
    authority: Arc<SocketViewAuthority>,
    pub(crate) lifetime: Arc<SocketLifetime>,
    identity: Arc<()>,
    connection_id: Id128,
}
impl LiveSocketContext {
    pub(crate) const fn actor(&self) -> &AuthenticatedActor {
        &self.actor
    }
    pub(crate) fn binding(&self) -> ReadClientBinding {
        ReadClientBinding {
            identity: self.identity.clone(),
        }
    }
    pub(crate) const fn authority(&self) -> &Arc<SocketViewAuthority> {
        &self.authority
    }
    pub(crate) fn client(self: &Arc<Self>) -> SocketViewClient {
        SocketViewClient {
            context: self.clone(),
        }
    }
    pub(crate) const fn connection_id(&self) -> Id128 {
        self.connection_id
    }
}
pub struct SocketViewAuthority {
    policy: Arc<ApplicationPolicy>,
    trust: TrustStore,
    sources: RwLock<Vec<NativeSourceId>>,
}
impl SocketViewAuthority {
    pub(crate) fn new(policy: Arc<ApplicationPolicy>, trust: TrustStore) -> Arc<Self> {
        Arc::new(Self {
            policy,
            trust,
            sources: RwLock::new(Vec::new()),
        })
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "The original authentication receipt is a linear one-use handoff and must be consumed"
    )]
    pub(crate) fn accept(
        self: &Arc<Self>,
        receipt: crate::management_ws::AuthenticatedSocketReceipt,
    ) -> Result<SocketViewClient, ViewError> {
        if !Arc::ptr_eq(&self.policy, receipt.policy()) {
            return Err(ViewError::InvalidClient);
        }
        receipt.lifetime().with_live(|| Ok(()))?;
        Ok(SocketViewClient {
            context: Arc::new(LiveSocketContext {
                actor: receipt.actor().clone(),
                authority: self.clone(),
                lifetime: receipt.lifetime().clone(),
                identity: Arc::new(()),
                connection_id: receipt.connection_id(),
            }),
        })
    }
    pub(crate) fn register_native_source<
        P: crate::application::native_dispatch::NativeApprovalPort,
    >(
        &self,
        cap: &crate::application::native_dispatch::SourceReadCapability<P>,
    ) -> Result<(), ViewError> {
        cap.require_current()?;
        self.register_source(cap.native().source())
    }
    fn register_source(&self, source: NativeSourceId) -> Result<(), ViewError> {
        let mut s = self.sources.write().map_err(|_| ViewError::ReadFailure)?;
        if !s.contains(&source) {
            s.try_reserve(1).map_err(|_| ViewError::AllocationFailure)?;
            s.push(source);
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn register_module_source<S, R>(
        &self,
        views: &super::SharedViews<S, R>,
    ) -> Result<(), ViewError> {
        if views.core.probe.is_sealed() {
            return Err(ViewError::SourceReleased);
        }
        self.register_source(views.core.source)
    }
    fn check(&self, client: &SocketViewClient) -> Result<(), ViewError> {
        if !std::ptr::eq(self, Arc::as_ptr(&client.context.authority)) {
            return Err(ViewError::InvalidClient);
        }
        client.context.lifetime.with_live(|| Ok(()))
    }
    pub(crate) async fn current_actor_guard(
        &self,
        client: &SocketViewClient,
    ) -> Result<(AuthenticatedActor, tokio::sync::OwnedMutexGuard<()>), ViewError> {
        self.check(client)?;
        let gate = self
            .trust
            .client_gate(client.context.actor.client_id())
            .lock_owned()
            .await;
        self.policy
            .require_current_actor(&client.context.actor)
            .await
            .map_err(policy_error)?;
        self.check(client)?;
        Ok((client.context.actor.clone(), gate))
    }
    pub(crate) async fn current_guard(
        &self,
        client: &SocketViewClient,
        target: &NativeSessionKey,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, ViewError> {
        let (_, gate) = self.current_actor_guard(client).await?;
        if !self
            .sources
            .read()
            .map_err(|_| ViewError::ReadFailure)?
            .contains(&target.source())
        {
            return Err(ViewError::InvalidTarget);
        }
        Ok(gate)
    }
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Owned policy errors enter this semantic map directly through Result::map_err"
)]
fn policy_error(e: AccessPolicyError) -> ViewError {
    match e {
        AccessPolicyError::InvalidActor => ViewError::Revoked,
        AccessPolicyError::NodeUnavailable => ViewError::NodeStopping,
        _ => ViewError::ReadFailure,
    }
}
impl CurrentViewAuthority<SocketViewClient> for SocketViewAuthority {
    fn client_binding(&self, client: &SocketViewClient) -> Result<ReadClientBinding, ViewError> {
        self.check(client)?;
        Ok(client.context.binding())
    }
    fn socket_context(
        &self,
        client: &SocketViewClient,
    ) -> Result<Option<Arc<LiveSocketContext>>, ViewError> {
        self.check(client)?;
        Ok(Some(client.context.clone()))
    }
    fn with_current<'a, T, F>(
        &'a self,
        client: &'a SocketViewClient,
        target: &'a NativeSessionKey,
        f: F,
    ) -> BoxReadFuture<'a, Result<T, ViewError>>
    where
        T: Send + 'a,
        F: FnOnce() -> Result<T, ViewError> + Send + 'a,
    {
        Box::pin(async move {
            let _gate = self.current_guard(client, target).await?;
            f()
        })
    }
    fn wait_client_change<'a>(
        &'a self,
        client: &'a SocketViewClient,
    ) -> BoxReadFuture<'a, Result<(), ViewError>> {
        Box::pin(async move {
            let mut revisions = self.trust.revision_receiver();
            let closed = client.context.lifetime.changed.notified();
            tokio::pin!(closed);
            closed.as_mut().enable();
            self.check(client)?;
            self.policy
                .require_current_actor(&client.context.actor)
                .await
                .map_err(policy_error)?;
            tokio::select! {()=closed=>{},_=revisions.changed()=>{}}
            self.check(client)?;
            self.policy
                .require_current_actor(&client.context.actor)
                .await
                .map_err(policy_error)
        })
    }
}
