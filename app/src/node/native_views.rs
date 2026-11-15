//! Thin local-read assembly over this node's original registry, policy and native owner.
use super::{NodeRequestAuthority, NodeRuntime};
use crate::{
    access_policy::AuthenticatedActor,
    adapters::acp::{
        SetupSnapshot,
        native_dispatch::{AcpNativeAdapter, AcpStreams},
        native_views::{AcpNativeViewFactory, AcpReadService, AcpSocketReadService},
        raw_json::OwnedRawValue,
    },
    application::{
        native_dispatch::{
            NativeRuntimeHandle, NativeRuntimeOptions, OwnedNativeSource, ReadConfigurationPort,
            RetainPrompt, SourceLaunchPort, SourceReadSelectionProof,
        },
        queue_owner::QueueService,
        request_registry::{NodeClock, RegistryOwnerScope, ValidatedBodyPort},
        shared_views::{
            BoxReadFuture, CurrentViewAuthority,
            budget::{NativeReadOptionsProvider, ReadReason},
            local_authority::{AuthenticatedLocalViewClient, LocalViewAuthority},
            reader::{
                OpenedView, OriginalReadProgress, PrepareNext, ReadCloseReceipt, ReadRebindMetadata,
            },
            socket_authority::{SocketViewAuthority, SocketViewClient},
        },
    },
    domain::{
        EntityId,
        native::NativeSessionKey,
        native_dispatch::DispatchError,
        request_registry::RegistryError,
        shared_view::{
            ReadEpoch, ViewDescriptor, ViewEpoch, ViewError, ViewRevision, ViewSourceStamp,
        },
        source_sessions::SourceLaunchSelection,
    },
    session_storage::{
        native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry,
    },
};
use std::{num::NonZeroUsize, sync::Arc};

pub struct StartedNativeViews<V, C, L, G>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    pub queue: QueueService<NodeRequestAuthority, V, SqliteRequestRegistry, C>,
    pub native: NativeRuntimeHandle<AcpNativeAdapter, L, G, SqliteNativeDispatch>,
    pub views: NodeNativeViewsService,
}
struct AdmittedNativeView {
    target: NativeSessionKey,
    reader: Arc<AcpReadService>,
}
struct AdmittedSocketNativeView {
    target: NativeSessionKey,
    reader: Arc<AcpSocketReadService>,
}
trait NativeViewAdmission: Send + Sync {
    fn admit<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedNativeView, ViewError>>;
}
trait SocketNativeViewAdmission: Send + Sync {
    fn admit_socket<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>>;
    fn admit_socket_source<'a>(
        &'a self,
        client: &'a SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>>;
}
#[derive(Clone)]
pub struct NodeNativeViewsService {
    authority: Arc<LocalViewAuthority>,
    admission: Arc<dyn NativeViewAdmission>,
    socket_authority: Option<Arc<SocketViewAuthority>>,
    socket_admission: Option<Arc<dyn SocketNativeViewAdmission>>,
}
pub struct NativeLocalViewRead {
    read: OpenedView,
    reader: Arc<AcpReadService>,
}
pub struct NativeSocketViewRead {
    read: OpenedView,
    reader: Arc<AcpSocketReadService>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLocalReadProgress {
    Busy,
    Pending,
    CandidateReady,
    Complete,
    Failed(ViewError),
}
pub struct NativeLocalStreamMetadata {
    actual: ReadRebindMetadata,
}
pub struct NativeLocalViewContinuation {
    read: NativeLocalViewRead,
    metadata: NativeLocalStreamMetadata,
}
impl NativeLocalViewContinuation {
    #[must_use]
    pub fn into_parts(self) -> (NativeLocalViewRead, NativeLocalStreamMetadata) {
        (self.read, self.metadata)
    }
}
impl NativeLocalStreamMetadata {
    #[must_use]
    pub const fn previous_stream(&self) -> EntityId {
        self.actual.previous_stream
    }
    #[must_use]
    pub const fn stream(&self) -> EntityId {
        self.actual.stream
    }
    #[must_use]
    pub const fn view_epoch(&self) -> ViewEpoch {
        self.actual.epoch
    }
    #[must_use]
    pub const fn read_epoch(&self) -> ReadEpoch {
        self.actual.read
    }
    #[must_use]
    pub const fn source(&self) -> ViewSourceStamp {
        self.actual.source
    }
    #[must_use]
    pub const fn snapshot_revision(&self) -> ViewRevision {
        self.actual.revision
    }
}
struct TypedNativeViewAdmission<V, C, L, G>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    queue: QueueService<NodeRequestAuthority, V, SqliteRequestRegistry, C>,
    native: NativeRuntimeHandle<AcpNativeAdapter, L, G, SqliteNativeDispatch>,
    authority: Arc<LocalViewAuthority>,
}
struct TypedSocketNativeViewAdmission<V, C, L, G>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    queue: QueueService<NodeRequestAuthority, V, SqliteRequestRegistry, C>,
    native: NativeRuntimeHandle<AcpNativeAdapter, L, G, SqliteNativeDispatch>,
    authority: Arc<SocketViewAuthority>,
    scope: RegistryOwnerScope,
}
const fn registry_error(error: RegistryError) -> ViewError {
    match error {
        RegistryError::InvalidActor => ViewError::Revoked,
        RegistryError::InvalidTarget => ViewError::InvalidTarget,
        RegistryError::NodeStopping | RegistryError::NodeStarting => ViewError::NodeStopping,
        _ => ViewError::ReadFailure,
    }
}
const fn dispatch_error(error: DispatchError) -> ViewError {
    match error {
        DispatchError::InvalidTarget => ViewError::InvalidTarget,
        DispatchError::InvalidSource => ViewError::InvalidSource,
        DispatchError::InvalidScope => ViewError::InvalidScope,
        DispatchError::NodeStopping => ViewError::NodeStopping,
        DispatchError::Stopped => ViewError::SourceReleased,
        DispatchError::InvalidConfiguration => ViewError::InvalidConfiguration,
        DispatchError::ProtocolRejected => ViewError::ProtocolFailure,
        DispatchError::LengthUnrepresentable => ViewError::LengthUnrepresentable,
        _ => ViewError::ReadFailure,
    }
}
impl<V, C, L, G> NativeViewAdmission for TypedNativeViewAdmission<V, C, L, G>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    fn admit<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedNativeView, ViewError>> {
        Box::pin(async move {
            self.authority.client_binding(client)?;
            let actor = self.authority.native_actor(client)?;
            let proof = self
                .queue
                .prepare_native_read(actor, target, reason)
                .await
                .map_err(registry_error)?;
            let acquisition = self
                .native
                .acquire_read_source(proof, reason)
                .map_err(dispatch_error)?;
            let capability = acquisition.source_capability().await?;
            self.authority.register_native_source(&capability)?;
            let attachment = capability.source().views().map_err(|_| {
                capability
                    .view_failure()
                    .unwrap_or(ViewError::CaptureUnavailable)
            })?;
            let views = attachment.views()?;
            let reader = attachment.reader(&capability)?;
            drop(attachment);
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                capability.require_current()?;
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(_) => {
                        capability.require_current()?;
                        break;
                    }
                    Err(ViewError::InvalidTarget) => {}
                    Err(error) => return Err(error),
                }
                tokio::select! {
                    () = changed => {},
                    result = self.authority.wait_client_change(client) => { result?; },
                }
            }
            // No attachment/core/native handle is stored in the returned data reader.
            Ok(AdmittedNativeView {
                target: capability.native().clone(),
                reader,
            })
        })
    }
}
impl<V, C, L, G> SocketNativeViewAdmission for TypedSocketNativeViewAdmission<V, C, L, G>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    fn admit_socket<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>> {
        Box::pin(async move {
            let (actor, gate) = self.authority.current_actor_guard(client).await?;
            drop(gate);
            let proof = self
                .queue
                .prepare_native_read(&actor, target, reason)
                .await
                .map_err(registry_error)?;
            let acquisition = self
                .native
                .acquire_read_source(proof, reason)
                .map_err(dispatch_error)?;
            let capability = acquisition.source_capability().await?;
            self.authority.register_native_source(&capability)?;
            let attachment = capability.source().views().map_err(|_| {
                capability
                    .view_failure()
                    .unwrap_or(ViewError::CaptureUnavailable)
            })?;
            let views = attachment.views()?;
            let reader = attachment.socket_reader(&capability, &self.authority)?;
            drop(attachment);
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                capability.require_current()?;
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(_) => {
                        capability.require_current()?;
                        break;
                    }
                    Err(ViewError::InvalidTarget) => {}
                    Err(error) => return Err(error),
                }
                tokio::select! {
                    () = changed => {},
                    result = self.authority.wait_client_change(client) => { result?; },
                }
            }
            Ok(AdmittedSocketNativeView {
                target: capability.native().clone(),
                reader,
            })
        })
    }
    fn admit_socket_source<'a>(
        &'a self,
        client: &'a SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>> {
        Box::pin(async move {
            let (_actor, gate) = self.authority.current_actor_guard(client).await?;
            drop(gate);
            let proof =
                SourceReadSelectionProof::mint(self.scope.clone(), selection, &native_session_id)
                    .map_err(dispatch_error)?;
            let acquisition = self
                .native
                .acquire_source_read(proof, reason)
                .map_err(dispatch_error)?;
            let capability = acquisition.source_capability().await?;
            self.authority.register_native_source(&capability)?;
            let attachment = capability.source().views().map_err(|_| {
                capability
                    .view_failure()
                    .unwrap_or(ViewError::CaptureUnavailable)
            })?;
            let views = attachment.views()?;
            let reader = attachment.socket_reader(&capability, &self.authority)?;
            let mut external_bound = false;
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                capability.require_current()?;
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(_) => {
                        capability.require_current()?;
                        break;
                    }
                    Err(ViewError::InvalidTarget) => {
                        if !external_bound {
                            attachment.ensure_external_target(capability.native().clone())?;
                            external_bound = true;
                        }
                    }
                    Err(error) => return Err(error),
                }
                tokio::select! {
                    () = changed => {},
                    result = self.authority.wait_client_change(client) => { result?; },
                }
            }
            drop(attachment);
            Ok(AdmittedSocketNativeView {
                target: capability.native().clone(),
                reader,
            })
        })
    }
}
impl NodeRuntime {
    /// Uses this node's private actor issuer and its original Storage runtime claim.
    /// No default harness/provider, extra protocol admission or socket identity is supplied.
    /// # Errors
    /// Reports the original composition, configuration or owner failure.
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep independent trusted protocol/configuration/read-budget/owner inputs explicit at the node assembly boundary"
    )]
    pub async fn start_native_views<V, C, L, G>(
        &self,
        validator: V,
        clock: C,
        protocol: Arc<AcpNativeAdapter>,
        launcher: Arc<L>,
        configuration: Arc<G>,
        retain: Arc<dyn RetainPrompt<V::Body, Retained = OwnedRawValue>>,
        options: NativeRuntimeOptions,
        read_options: Arc<dyn NativeReadOptionsProvider>,
        yield_every: NonZeroUsize,
    ) -> Result<StartedNativeViews<V, C, L, G>, DispatchError>
    where
        V: ValidatedBodyPort + 'static,
        C: NodeClock + 'static,
        L: SourceLaunchPort,
        L::Owned: OwnedNativeSource<Streams = AcpStreams>,
        G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
    {
        let storage = self.storage.as_ref().ok_or(DispatchError::NodeStopping)?;
        let policy = self
            .application_policy
            .as_ref()
            .ok_or(DispatchError::NodeStopping)?;
        let request_authority = self.request_authority().map_err(DispatchError::from)?;
        start_from_node_parts(
            storage,
            policy,
            request_authority,
            validator,
            clock,
            protocol,
            launcher,
            configuration,
            retain,
            options,
            read_options,
            yield_every,
            None,
            #[cfg(test)]
            None,
        )
        .await
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Keep original independent production ports explicit"
)]
pub(super) async fn start_from_node_parts<V, C, L, G>(
    storage: &crate::storage::Storage,
    policy: &Arc<crate::access_policy::ApplicationPolicy>,
    request_authority: NodeRequestAuthority,
    validator: V,
    clock: C,
    protocol: Arc<AcpNativeAdapter>,
    launcher: Arc<L>,
    configuration: Arc<G>,
    retain: Arc<dyn RetainPrompt<V::Body, Retained = OwnedRawValue>>,
    options: NativeRuntimeOptions,
    read_options: Arc<dyn NativeReadOptionsProvider>,
    yield_every: NonZeroUsize,
    socket_authority: Option<
        Arc<crate::application::shared_views::socket_authority::SocketViewAuthority>,
    >,
    #[cfg(test)] factory_hooks: Option<&super::session_services::FactoryTestHooks>,
) -> Result<StartedNativeViews<V, C, L, G>, DispatchError>
where
    V: ValidatedBodyPort + 'static,
    C: NodeClock + 'static,
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    let authority = Arc::new(policy.local_view_authority(Vec::new()));
    let factory = AcpNativeViewFactory::new(yield_every, read_options, authority.clone());
    let factory = Arc::new(match socket_authority.as_ref() {
        Some(socket) => factory.with_socket_authority(socket.clone()),
        None => factory,
    });
    let (queue, native) = storage
        .start_native_service(
            request_authority,
            validator,
            clock,
            protocol,
            launcher,
            configuration,
            retain,
            options,
        )
        .await?;
    #[cfg(test)]
    if let Some(hooks) = factory_hooks {
        hooks.mark(super::session_services::FactoryStage::BeforeViewInstall);
        if let Some(barrier) = &hooks.before_view_install {
            barrier.pause().await;
        }
    }
    native
        .install_view_factory(factory)
        .map_err(|_| DispatchError::RequiredStateUnavailable)?;
    let admission = Arc::new(TypedNativeViewAdmission {
        queue: queue.clone(),
        native: native.clone(),
        authority: authority.clone(),
    });
    let scope = storage.request_registry().owner_scope();
    let socket_admission: Option<Arc<dyn SocketNativeViewAdmission>> =
        socket_authority.as_ref().map(|socket_authority| {
            Arc::new(TypedSocketNativeViewAdmission {
                queue: queue.clone(),
                native: native.clone(),
                authority: socket_authority.clone(),
                scope: scope.clone(),
            }) as Arc<dyn SocketNativeViewAdmission>
        });
    Ok(StartedNativeViews {
        queue,
        native,
        views: NodeNativeViewsService {
            authority,
            admission,
            socket_authority,
            socket_admission,
        },
    })
}
impl NodeNativeViewsService {
    /// # Errors
    /// Rejects a foreign, stopped or revoked node actor through the actual policy.
    pub async fn accept_actor(
        &self,
        actor: AuthenticatedActor,
    ) -> Result<AuthenticatedLocalViewClient, ViewError> {
        self.authority.accept_actor(actor).await
    }
    /// # Errors
    /// Keeps current authentication, original native admission and view availability separate.
    pub async fn open(
        &self,
        client: &AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> Result<NativeLocalViewRead, ViewError> {
        let admitted = self.admission.admit(client, target, reason).await?;
        let read = admitted
            .reader
            .open(client, &admitted.target, reason)
            .await?;
        Ok(NativeLocalViewRead {
            read,
            reader: admitted.reader,
        })
    }
    /// Opens the same original native read through this authenticated socket.
    /// # Errors
    /// Rejects missing socket assembly, current authentication/revocation, invalid
    /// targets and source or capture failures.
    pub async fn open_socket(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> Result<NativeSocketViewRead, ViewError> {
        let Some(authority) = self.socket_authority.as_ref() else {
            return Err(ViewError::CaptureUnavailable);
        };
        let Some(admission) = self.socket_admission.as_ref() else {
            return Err(ViewError::CaptureUnavailable);
        };
        if !std::ptr::eq(authority.as_ref(), client.context.authority().as_ref()) {
            return Err(ViewError::InvalidClient);
        }
        let admitted = admission.admit_socket(client, target, reason).await?;
        let read = admitted
            .reader
            .open(client, &admitted.target, reason)
            .await?;
        Ok(NativeSocketViewRead {
            read,
            reader: admitted.reader,
        })
    }
    /// Opens an authenticated read-only view for an external native session without a target.
    /// # Errors
    /// Rejects missing socket assembly, current authentication/revocation, invalid
    /// selection/native identifier and source or capture failures.
    pub async fn open_socket_source(
        &self,
        client: &SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        reason: ReadReason,
    ) -> Result<NativeSocketViewRead, ViewError> {
        let Some(authority) = self.socket_authority.as_ref() else {
            return Err(ViewError::CaptureUnavailable);
        };
        let Some(admission) = self.socket_admission.as_ref() else {
            return Err(ViewError::CaptureUnavailable);
        };
        if !std::ptr::eq(authority.as_ref(), client.context.authority().as_ref()) {
            return Err(ViewError::InvalidClient);
        }
        let admitted = admission
            .admit_socket_source(client, selection, native_session_id, reason)
            .await?;
        let read = admitted
            .reader
            .open(client, &admitted.target, reason)
            .await?;
        Ok(NativeSocketViewRead {
            read,
            reader: admitted.reader,
        })
    }
}
impl NativeLocalViewRead {
    /// # Errors
    /// Reads only this original operation under current authentication; Busy grants nothing.
    pub async fn original_progress(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<NativeLocalReadProgress, ViewError> {
        let observer = self.reader.original_observer(&self.read)?;
        Ok(
            match self.reader.try_observe_original(client, &observer).await? {
                None => NativeLocalReadProgress::Busy,
                Some(OriginalReadProgress::Pending) => NativeLocalReadProgress::Pending,
                Some(OriginalReadProgress::CandidateReady { .. }) => {
                    NativeLocalReadProgress::CandidateReady
                }
                Some(OriginalReadProgress::Complete { .. }) => NativeLocalReadProgress::Complete,
                Some(OriginalReadProgress::Failed(error)) => NativeLocalReadProgress::Failed(error),
            },
        )
    }
    /// # Errors
    /// Claims this original in-flight request once and pins its existing candidate generation.
    pub async fn inflight_original(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<NativeLocalViewContinuation, ViewError> {
        let rebound = self
            .reader
            .rebind_inflight_same_read(client, &self.read)
            .await?;
        let (read, metadata) = rebound.into_parts();
        Ok(NativeLocalViewContinuation {
            read: Self {
                read,
                reader: Arc::clone(&self.reader),
            },
            metadata: NativeLocalStreamMetadata { actual: metadata },
        })
    }
    #[must_use]
    pub fn descriptor(&self) -> ViewDescriptor {
        self.read.descriptor()
    }
    /// # Errors
    /// Reauthorizes this actual chunk; preparation grants no delivered cursor commit.
    pub async fn next_snapshot(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<PrepareNext, ViewError> {
        self.reader.next_snapshot(client, &self.read).await
    }
    /// # Errors
    /// Reauthorizes this actual chunk and preserves the original fixed-H/tail boundary.
    pub async fn next_delta(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<PrepareNext, ViewError> {
        self.reader.next_delta(client, &self.read).await
    }
    /// # Errors
    /// Returns source/stream/epoch invalidation or actual authentication failure.
    pub async fn wait_change(
        &self,
        client: &AuthenticatedLocalViewClient,
    ) -> Result<ViewDescriptor, ViewError> {
        self.reader.wait_change(client, &self.read).await
    }
    #[must_use]
    pub fn release(self) -> ReadCloseReceipt {
        self.reader.release(self.read)
    }
}
impl NativeSocketViewRead {
    /// # Errors
    /// Reads only this original operation's progress under current socket authentication.
    pub async fn original_progress(
        &self,
        client: &SocketViewClient,
    ) -> Result<OriginalReadProgress, ViewError> {
        self.reader.original_progress(client, &self.read).await
    }
    /// # Errors
    /// Waits for this original read operation to finish replay or publish its replacement generation.
    pub async fn wait_original_progress(
        &self,
        client: &SocketViewClient,
    ) -> Result<OriginalReadProgress, ViewError> {
        self.reader.wait_original_progress(client, &self.read).await
    }
    /// # Errors
    /// Waits for this original read operation to finish replay or fail while still valid.
    pub async fn wait_original_completion(
        &self,
        client: &SocketViewClient,
    ) -> Result<(), ViewError> {
        self.reader
            .wait_original_completion(client, &self.read)
            .await
    }
    /// # Errors
    /// Rebinds this completed original read onto its published replay-complete generation.
    pub async fn rebind_same_read(&self, client: &SocketViewClient) -> Result<Self, ViewError> {
        let rebound = self.reader.rebind_same_read(client, &self.read).await?;
        let (read, _) = rebound.into_parts();
        Ok(Self {
            read,
            reader: Arc::clone(&self.reader),
        })
    }
    /// # Errors
    /// Reauthorizes this actual socket and prepares at most one bounded snapshot chunk.
    pub async fn next_snapshot(&self, client: &SocketViewClient) -> Result<PrepareNext, ViewError> {
        self.reader.next_snapshot(client, &self.read).await
    }
    /// # Errors
    /// Reauthorizes this actual socket and prepares one bounded tail chunk.
    pub async fn next_delta(&self, client: &SocketViewClient) -> Result<PrepareNext, ViewError> {
        self.reader.next_delta(client, &self.read).await
    }
    /// # Errors
    /// Returns current authentication, source loss or generation invalidation.
    pub async fn wait_change(
        &self,
        client: &SocketViewClient,
    ) -> Result<ViewDescriptor, ViewError> {
        self.reader.wait_change(client, &self.read).await
    }
    #[must_use]
    pub fn descriptor(&self) -> ViewDescriptor {
        self.read.descriptor()
    }
    #[must_use]
    pub fn stream_id(&self) -> EntityId {
        self.read.stream_id()
    }
    /// Opens the node-owned transfer stream for this native view.
    ///
    /// # Errors
    ///
    /// Returns an error when the owned view or its transfer stream is no
    /// longer available.
    pub fn stream(
        &self,
    ) -> Result<crate::application::shared_views::socket_transfer::SocketViewStream, ViewError>
    {
        self.read.socket_stream()
    }
    #[must_use]
    pub fn release(self) -> ReadCloseReceipt {
        self.reader.release(self.read)
    }
}
