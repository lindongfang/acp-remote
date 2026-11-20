//! Thin local-read assembly over this node's original registry, policy and native owner.
use super::{NodeRequestAuthority, NodeRuntime};
use crate::{
    access_policy::AuthenticatedActor,
    adapters::acp::{
        SetupSnapshot,
        native_dispatch::{AcpNativeAdapter, AcpStreams},
        native_views::{
            AcpNativeViewFactory, AcpReadService, AcpSocketReadService, AcpTranscriptTap,
        },
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
            ReadEpoch, ReadMode, TranscriptCacheViewStatus, TranscriptCapabilityBoundary,
            TranscriptSource, TranscriptSyncState, ViewDescriptor, ViewEpoch, ViewError,
            ViewRevision, ViewSourceStamp,
        },
        source_sessions::SourceLaunchSelection,
    },
    session_storage::{
        native_dispatch::SqliteNativeDispatch, request_registry::SqliteRequestRegistry,
    },
    transcript_cache::{
        CachedViewHandle, CapabilityProfile, RevalidationOutcome, TranscriptCacheCoordinator,
        TranscriptCacheError,
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
struct LiveViewCandidate<R> {
    reader: Arc<R>,
    is_loaded: bool,
    replay_complete: bool,
    hybrid_preloaded: bool,
}
trait NativeViewAdmission: Send + Sync {
    fn admit<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedNativeView, ViewError>>;
    fn verify_cold_local<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>>;
    fn try_open_live_local<'a>(
        &'a self,
        key: &'a NativeSessionKey,
    ) -> BoxReadFuture<'a, Result<Option<LiveViewCandidate<AcpReadService>>, ViewError>>;
    fn probe_and_revalidate_target(
        &self,
        actor: AuthenticatedActor,
        target: EntityId,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()>;
}
enum SocketReadPurpose {
    Open,
    History(Option<NativeSessionKey>),
}
trait SocketNativeViewAdmission: Send + Sync {
    fn history_replay_allowed(&self, key: &NativeSessionKey) -> Result<bool, ViewError>;
    fn history_child<'a>(
        &'a self,
        parent: &'a NativeSessionKey,
        child: &'a str,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>>;
    fn admit_socket<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
        purpose: SocketReadPurpose,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>>;
    fn admit_socket_source<'a>(
        &'a self,
        client: &'a SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>>;
    fn verify_cold_target<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>>;
    fn verify_cold_source<'a>(
        &'a self,
        client: &'a SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>>;
    fn try_open_live_socket<'a>(
        &'a self,
        key: &'a NativeSessionKey,
    ) -> BoxReadFuture<'a, Result<Option<LiveViewCandidate<AcpSocketReadService>>, ViewError>>;
    fn probe_and_revalidate_socket_target(
        &self,
        client: SocketViewClient,
        target: EntityId,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()>;
    fn probe_and_revalidate_socket_source(
        &self,
        client: SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()>;
}
#[derive(Clone)]
pub struct NodeNativeViewsService {
    authority: Arc<LocalViewAuthority>,
    admission: Arc<dyn NativeViewAdmission>,
    socket_authority: Option<Arc<SocketViewAuthority>>,
    socket_admission: Option<Arc<dyn SocketNativeViewAdmission>>,
    transcript_cache: Option<Arc<TranscriptCacheCoordinator>>,
}
pub struct NativeLocalViewRead {
    read: OpenedView,
    reader: Arc<AcpReadService>,
    cached: Option<CachedViewHandle>,
}
pub(crate) mod history;
#[derive(Clone)]
pub(crate) enum SocketRebindTarget {
    Target(EntityId),
    Source {
        selection: SourceLaunchSelection,
        native_session_id: String,
    },
}
enum SocketReadBacking {
    Live(Option<TranscriptCacheViewStatus>),
    Cached {
        handle: CachedViewHandle,
        authority: Arc<SocketViewAuthority>,
        admission: Arc<dyn SocketNativeViewAdmission>,
        target: SocketRebindTarget,
        coordinator: Arc<TranscriptCacheCoordinator>,
    },
}
pub struct NativeSocketViewRead {
    history: Option<history::BoundHistory>,
    read: OpenedView,
    reader: Arc<AcpSocketReadService>,
    backing: SocketReadBacking,
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
        DispatchError::SessionCapacityFull | DispatchError::SessionTreeUnverified => {
            ViewError::NativeFailure(error)
        }
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
fn push_non_rebound_degraded_status(
    coordinator: &TranscriptCacheCoordinator,
    key: &NativeSessionKey,
    profile: CapabilityProfile,
    error: ViewError,
) {
    if coordinator.is_remote_missing(key) {
        return;
    }
    let sync_state = if matches!(
        error,
        ViewError::NativeFailure(DispatchError::SessionCapacityFull)
    ) {
        TranscriptSyncState::DegradedCapacityFull
    } else {
        TranscriptSyncState::DegradedUnreachable
    };
    coordinator.push_status_for_key(
        key,
        TranscriptCacheViewStatus {
            transcript_source: TranscriptSource::TranscriptCache,
            sync_state,
            capability_boundary: TranscriptCacheCoordinator::capability_boundary_for(profile),
            continuable: true,
            cache_warning: coordinator.has_write_warning(),
        },
    );
}
fn push_non_replay_boundary_status(
    coordinator: &TranscriptCacheCoordinator,
    key: &NativeSessionKey,
    profile: CapabilityProfile,
) {
    coordinator.push_status_for_key(
        key,
        TranscriptCacheViewStatus {
            transcript_source: TranscriptSource::TranscriptCache,
            sync_state: TranscriptSyncState::Idle,
            capability_boundary: TranscriptCacheCoordinator::capability_boundary_for(profile),
            continuable: profile == CapabilityProfile::ResumeOnly,
            cache_warning: coordinator.has_write_warning(),
        },
    );
}
fn discovered_profile(
    proto: &crate::adapters::acp::native_dispatch::AcpSource,
    coordinator: &TranscriptCacheCoordinator,
    key: &NativeSessionKey,
) -> CapabilityProfile {
    proto
        .capability_profile()
        .or_else(|| {
            coordinator
                .known_source_capabilities(key.source())
                .map(|(_, profile)| profile)
        })
        .unwrap_or(CapabilityProfile::ReplayCapable)
}
fn lookup_live_candidate<R, L, G>(
    native: &NativeRuntimeHandle<AcpNativeAdapter, L, G, SqliteNativeDispatch>,
    key: &NativeSessionKey,
    get_reader: impl FnOnce(
        &crate::adapters::acp::native_views::AcpSourceViews,
    ) -> Result<Arc<R>, ViewError>,
) -> Option<LiveViewCandidate<R>>
where
    L: SourceLaunchPort,
    L::Owned: OwnedNativeSource<Streams = AcpStreams>,
    G: ReadConfigurationPort<LaunchPlan = L::Plan, Setup = SetupSnapshot>,
{
    let proto = native.active_live_source(key)?;
    let attachment = proto.views().ok()?;
    let views = attachment.views().ok()?;
    let pub_target = views.core.target(key).ok()?;
    if !pub_target.send.current().unwrap_or(false) {
        return None;
    }
    let reader = get_reader(&attachment).ok()?;
    let replay_complete = pub_target
        .snapshot
        .read()
        .is_ok_and(|snap| snap.descriptor.read_mode() == ReadMode::ReplayComplete);
    let is_loaded = native.is_session_slot_active(key);
    let hybrid_preloaded = attachment.is_hybrid_preloaded(key);
    Some(LiveViewCandidate {
        reader,
        is_loaded,
        replay_complete,
        hybrid_preloaded,
    })
}
async fn execute_replay_revalidation<F>(
    key: &NativeSessionKey,
    profile: CapabilityProfile,
    coordinator: &TranscriptCacheCoordinator,
    register_source: F,
    acquisition: Result<
        crate::application::native_dispatch::read_source::NativeReadAcquisition<AcpNativeAdapter>,
        DispatchError,
    >,
) where
    F: FnOnce(
        &crate::application::native_dispatch::read_source::SourceReadCapability<AcpNativeAdapter>,
    ) -> Result<(), ViewError>,
{
    let acquisition = match acquisition {
        Ok(acq) => acq,
        Err(error) => {
            push_non_rebound_degraded_status(coordinator, key, profile, dispatch_error(error));
            return;
        }
    };
    let capability = match acquisition.source_capability().await {
        Ok(cap) => cap,
        Err(error) => {
            push_non_rebound_degraded_status(coordinator, key, profile, error);
            return;
        }
    };
    if let Err(error) = crate::adapters::acp::native_views::admit_replay_capacity(&capability).await
    {
        push_non_rebound_degraded_status(coordinator, key, profile, error);
        return;
    }
    if let Err(error) = register_source(&capability) {
        push_non_rebound_degraded_status(coordinator, key, profile, error);
        return;
    }
    let Ok(attachment) = capability.source().views() else {
        let error = capability
            .view_failure()
            .unwrap_or(ViewError::CaptureUnavailable);
        push_non_rebound_degraded_status(coordinator, key, profile, error);
        return;
    };
    if let Err(error) = capability.require_current() {
        push_non_rebound_degraded_status(coordinator, key, profile, error);
        return;
    }
    if let Ok(views) = attachment.views()
        && matches!(views.core.target(key), Err(ViewError::InvalidTarget))
    {
        if let Err(error) =
            crate::application::native_dispatch::view_attachment::NativeSourceViewOwner::bind_target(
                attachment.as_ref(),
                key,
            )
        {
            push_non_rebound_degraded_status(coordinator, key, profile, error);
            return;
        }
        for _ in 0..50 {
            let notified = views.core.changed.notified();
            if views.core.target(key).is_ok() {
                break;
            }
            if tokio::time::timeout(std::time::Duration::from_millis(50), notified)
                .await
                .is_err()
            {
                break;
            }
        }
    }
    match attachment.trigger_background_replay(key).await {
        Ok(()) => {
            coordinator.wait_pending_replay_persist(key).await;
            coordinator.notify_synced_rebound(key);
        }
        Err(error) => {
            push_non_rebound_degraded_status(coordinator, key, profile, error);
        }
    }
}
async fn try_open_live_socket_read(
    tc: &TranscriptCacheCoordinator,
    admission: &dyn SocketNativeViewAdmission,
    client: &SocketViewClient,
    key: &NativeSessionKey,
    reason: ReadReason,
) -> Result<(Option<NativeSocketViewRead>, bool), ViewError> {
    let live = admission.try_open_live_socket(key).await?;
    let is_loaded = live.as_ref().is_some_and(|candidate| candidate.is_loaded);
    if reason != ReadReason::ExplicitRefresh
        && let Some(live) = live
        && (live.is_loaded || live.replay_complete)
        && !tc.is_revalidating(key).await
    {
        let hybrid = live.hybrid_preloaded || tc.is_hybrid_resumed(key);
        let read = if hybrid && !live.replay_complete {
            live.reader.open_current(client, key).await?
        } else {
            live.reader.open(client, key, reason).await?
        };
        let replay_complete = read.descriptor().read_mode() == ReadMode::ReplayComplete;
        let status = tc.live_status(key, hybrid, replay_complete);
        return Ok((
            Some(NativeSocketViewRead {
                read,
                reader: Arc::clone(&live.reader),
                history: None,
                backing: SocketReadBacking::Live(Some(status)),
            }),
            is_loaded,
        ));
    }
    Ok((None, is_loaded))
}
fn prepare_cold_status(
    tc: &TranscriptCacheCoordinator,
    session: &crate::transcript_cache::PersistedTranscriptSession,
    reason: ReadReason,
) -> (TranscriptCacheViewStatus, bool) {
    let (mut status, mut needs_probe) = tc.initial_cold_status(session);
    if matches!(reason, ReadReason::ExplicitRefresh)
        && status.continuable
        && matches!(
            session.capability_profile,
            CapabilityProfile::ReplayCapable | CapabilityProfile::Unknown
        )
    {
        status.sync_state = TranscriptSyncState::Revalidating;
        needs_probe = true;
    }
    (status, needs_probe)
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
            crate::adapters::acp::native_views::admit_replay_capacity(&capability).await?;
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
                if let Some(error) = views.core.probe.route_failure() {
                    return Err(error);
                }
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(target) if target.send.current()? => {
                        capability.require_current()?;
                        break;
                    }
                    Ok(_) | Err(ViewError::InvalidTarget) => {}
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
    fn verify_cold_local<'a>(
        &'a self,
        client: &'a AuthenticatedLocalViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>> {
        Box::pin(async move {
            self.authority.client_binding(client)?;
            let actor = self.authority.native_actor(client)?;
            let proof = self
                .queue
                .prepare_native_read(actor, target, reason)
                .await
                .map_err(registry_error)?;
            let metadata = self
                .native
                .verify_cold_target_read(&proof)
                .await
                .map_err(dispatch_error)?;
            self.authority
                .register_verified_source(metadata.key.source())?;
            Ok(metadata.key)
        })
    }
    fn try_open_live_local<'a>(
        &'a self,
        key: &'a NativeSessionKey,
    ) -> BoxReadFuture<'a, Result<Option<LiveViewCandidate<AcpReadService>>, ViewError>> {
        Box::pin(async move {
            Ok(lookup_live_candidate(&self.native, key, |attachment| {
                attachment.live_local_reader(&self.authority)
            }))
        })
    }
    fn probe_and_revalidate_target(
        &self,
        actor: AuthenticatedActor,
        target: EntityId,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()> {
        Box::pin(async move {
            let mut profile = known_profile;
            if profile == CapabilityProfile::Unknown {
                let proof = match self
                    .queue
                    .prepare_native_read(&actor, target, ReadReason::ExplicitRefresh)
                    .await
                {
                    Ok(proof) => proof,
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            registry_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                };
                match self.native.probe_target_read(proof).await {
                    Ok((proto, probed_key)) => {
                        profile = discovered_profile(&proto, &coordinator, &probed_key);
                        let _ = coordinator
                            .store()
                            .update_session_status(&probed_key, Some(profile), None)
                            .await;
                    }
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            dispatch_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                }
            }
            if matches!(
                profile,
                CapabilityProfile::ResumeOnly | CapabilityProfile::PureMemory
            ) {
                push_non_replay_boundary_status(&coordinator, &key, profile);
                coordinator.clear_inflight(&key).await;
                return;
            }
            let replay_acquisition = match self
                .queue
                .prepare_native_read(&actor, target, ReadReason::ExplicitRefresh)
                .await
            {
                Ok(proof) => self.native.acquire_target_replay_read(proof),
                Err(error) => Err(match registry_error(error) {
                    ViewError::NativeFailure(dispatch) => dispatch,
                    ViewError::Revoked => DispatchError::Stopped,
                    _ => DispatchError::InvalidTarget,
                }),
            };
            execute_replay_revalidation(
                &key,
                profile,
                &coordinator,
                |capability| self.authority.register_native_source(capability),
                replay_acquisition,
            )
            .await;
            coordinator.clear_inflight(&key).await;
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
    fn history_replay_allowed(&self, key: &NativeSessionKey) -> Result<bool, ViewError> {
        self.native
            .history_replay_allowed(key)
            .map_err(dispatch_error)
    }
    fn admit_socket<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
        purpose: SocketReadPurpose,
    ) -> BoxReadFuture<'a, Result<AdmittedSocketNativeView, ViewError>> {
        Box::pin(async move {
            let (actor, gate) = self.authority.current_actor_guard(client).await?;
            drop(gate);
            let proof = self
                .queue
                .prepare_native_read(&actor, target, reason)
                .await
                .map_err(registry_error)?;
            let acquisition = match purpose {
                SocketReadPurpose::History(child) => {
                    self.native.acquire_history_replay(proof, child)
                }
                SocketReadPurpose::Open => self.native.acquire_read_source(proof, reason),
            }
            .map_err(dispatch_error)?;
            let capability = acquisition.source_capability().await?;
            crate::adapters::acp::native_views::admit_replay_capacity(&capability).await?;
            self.authority.register_native_source(&capability)?;
            let attachment = capability.source().views().map_err(|_| {
                capability
                    .view_failure()
                    .unwrap_or(ViewError::CaptureUnavailable)
            })?;
            let views = attachment.views()?;
            let reader = attachment.socket_reader(&capability, &self.authority)?;
            if capability.is_source_only()
                && matches!(
                    views.core.target(capability.native()),
                    Err(ViewError::InvalidTarget)
                )
            {
                attachment.ensure_external_target(capability.native().clone())?;
            }
            drop(attachment);
            loop {
                let changed = views.core.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                capability.require_current()?;
                if let Some(error) = views.core.probe.route_failure() {
                    return Err(error);
                }
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(target) if target.send.current()? => {
                        capability.require_current()?;
                        break;
                    }
                    Ok(_) | Err(ViewError::InvalidTarget) => {}
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
            crate::adapters::acp::native_views::admit_replay_capacity(&capability).await?;
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
                if let Some(error) = views.core.probe.route_failure() {
                    return Err(error);
                }
                if views.core.probe.is_sealed() {
                    return Err(ViewError::SourceReleased);
                }
                match views.core.target(capability.native()) {
                    Ok(target) if target.send.current()? => {
                        capability.require_current()?;
                        break;
                    }
                    Ok(_) => {}
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
    fn history_child<'a>(
        &'a self,
        parent: &'a NativeSessionKey,
        child: &'a str,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>> {
        Box::pin(async move {
            self.native
                .verify_history_child(parent, child)
                .await
                .map_err(|error| match error {
                    DispatchError::SessionTreeUnverified => ViewError::InvalidTarget,
                    other => dispatch_error(other),
                })
        })
    }
    fn verify_cold_target<'a>(
        &'a self,
        client: &'a SocketViewClient,
        target: EntityId,
        reason: ReadReason,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>> {
        Box::pin(async move {
            let (actor, gate) = self.authority.current_actor_guard(client).await?;
            drop(gate);
            let proof = self
                .queue
                .prepare_native_read(&actor, target, reason)
                .await
                .map_err(registry_error)?;
            let metadata = self
                .native
                .verify_cold_target_read(&proof)
                .await
                .map_err(dispatch_error)?;
            self.authority
                .register_verified_source(metadata.key.source())?;
            Ok(metadata.key)
        })
    }
    fn verify_cold_source<'a>(
        &'a self,
        client: &'a SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
    ) -> BoxReadFuture<'a, Result<NativeSessionKey, ViewError>> {
        Box::pin(async move {
            let (_actor, gate) = self.authority.current_actor_guard(client).await?;
            drop(gate);
            let proof =
                SourceReadSelectionProof::mint(self.scope.clone(), selection, &native_session_id)
                    .map_err(dispatch_error)?;
            let metadata = self
                .native
                .verify_cold_source_read(&proof)
                .await
                .map_err(dispatch_error)?;
            self.authority
                .register_verified_source(metadata.key.source())?;
            Ok(metadata.key)
        })
    }
    fn try_open_live_socket<'a>(
        &'a self,
        key: &'a NativeSessionKey,
    ) -> BoxReadFuture<'a, Result<Option<LiveViewCandidate<AcpSocketReadService>>, ViewError>> {
        Box::pin(async move {
            Ok(lookup_live_candidate(&self.native, key, |attachment| {
                attachment.live_socket_reader(&self.authority)
            }))
        })
    }
    fn probe_and_revalidate_socket_target(
        &self,
        client: SocketViewClient,
        target: EntityId,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()> {
        Box::pin(async move {
            let actor = match self.authority.current_actor_guard(&client).await {
                Ok((actor, gate)) => {
                    drop(gate);
                    actor
                }
                Err(error) => {
                    push_non_rebound_degraded_status(&coordinator, &key, known_profile, error);
                    coordinator.clear_inflight(&key).await;
                    return;
                }
            };
            let mut profile = known_profile;
            if profile == CapabilityProfile::Unknown {
                let proof = match self
                    .queue
                    .prepare_native_read(&actor, target, ReadReason::ExplicitRefresh)
                    .await
                {
                    Ok(proof) => proof,
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            registry_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                };
                match self.native.probe_target_read(proof).await {
                    Ok((proto, probed_key)) => {
                        profile = discovered_profile(&proto, &coordinator, &probed_key);
                        let _ = coordinator
                            .store()
                            .update_session_status(&probed_key, Some(profile), None)
                            .await;
                    }
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            dispatch_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                }
            }
            if matches!(
                profile,
                CapabilityProfile::ResumeOnly | CapabilityProfile::PureMemory
            ) {
                push_non_replay_boundary_status(&coordinator, &key, profile);
                coordinator.clear_inflight(&key).await;
                return;
            }
            let replay_acquisition = match self
                .queue
                .prepare_native_read(&actor, target, ReadReason::ExplicitRefresh)
                .await
            {
                Ok(proof) => self.native.acquire_target_replay_read(proof),
                Err(error) => Err(match registry_error(error) {
                    ViewError::NativeFailure(dispatch) => dispatch,
                    ViewError::Revoked => DispatchError::Stopped,
                    _ => DispatchError::InvalidTarget,
                }),
            };
            execute_replay_revalidation(
                &key,
                profile,
                &coordinator,
                |capability| self.authority.register_native_source(capability),
                replay_acquisition,
            )
            .await;
            coordinator.clear_inflight(&key).await;
        })
    }
    fn probe_and_revalidate_socket_source(
        &self,
        client: SocketViewClient,
        selection: SourceLaunchSelection,
        native_session_id: String,
        key: NativeSessionKey,
        known_profile: CapabilityProfile,
        coordinator: Arc<TranscriptCacheCoordinator>,
    ) -> BoxReadFuture<'_, ()> {
        Box::pin(async move {
            match self.authority.current_actor_guard(&client).await {
                Ok((_, gate)) => drop(gate),
                Err(error) => {
                    push_non_rebound_degraded_status(&coordinator, &key, known_profile, error);
                    coordinator.clear_inflight(&key).await;
                    return;
                }
            }
            let mut profile = known_profile;
            if profile == CapabilityProfile::Unknown {
                let proof = match SourceReadSelectionProof::mint(
                    self.scope.clone(),
                    selection.clone(),
                    &native_session_id,
                ) {
                    Ok(proof) => proof,
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            dispatch_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                };
                match self.native.probe_source_read(proof).await {
                    Ok((proto, probed_key)) => {
                        profile = discovered_profile(&proto, &coordinator, &probed_key);
                        let _ = coordinator
                            .store()
                            .update_session_status(&probed_key, Some(profile), None)
                            .await;
                    }
                    Err(error) => {
                        push_non_rebound_degraded_status(
                            &coordinator,
                            &key,
                            profile,
                            dispatch_error(error),
                        );
                        coordinator.clear_inflight(&key).await;
                        return;
                    }
                }
            }
            if matches!(
                profile,
                CapabilityProfile::ResumeOnly | CapabilityProfile::PureMemory
            ) {
                push_non_replay_boundary_status(&coordinator, &key, profile);
                coordinator.clear_inflight(&key).await;
                return;
            }
            let replay_acquisition = match SourceReadSelectionProof::mint(
                self.scope.clone(),
                selection,
                &native_session_id,
            ) {
                Ok(proof) => self
                    .native
                    .acquire_source_read(proof, ReadReason::ExplicitRefresh),
                Err(error) => Err(error),
            };
            execute_replay_revalidation(
                &key,
                profile,
                &coordinator,
                |capability| self.authority.register_native_source(capability),
                replay_acquisition,
            )
            .await;
            coordinator.clear_inflight(&key).await;
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
    transcript_cache: Option<Arc<TranscriptCacheCoordinator>>,
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
    let factory = match socket_authority.as_ref() {
        Some(socket) => factory.with_socket_authority(socket.clone()),
        None => factory,
    };
    let factory = Arc::new(match transcript_cache.as_ref() {
        Some(cache) => factory.with_transcript_tap(Arc::clone(cache) as Arc<dyn AcpTranscriptTap>),
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
            transcript_cache,
        },
    })
}
impl NodeNativeViewsService {
    /// Synchronously seals cached views and deletes persisted cache rows for a confirmed-deleted
    /// native session.
    ///
    /// # Errors
    /// Returns `TranscriptCacheError` if cleanup fails.
    pub async fn purge_deleted_native_session(
        &self,
        key: &NativeSessionKey,
    ) -> Result<(), TranscriptCacheError> {
        if let Some(tc) = &self.transcript_cache {
            tc.purge_deleted_native_session(key).await?;
        }
        Ok(())
    }

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
        if let Some(tc) = &self.transcript_cache {
            let actor = self.authority.native_actor(client)?.clone();
            let key = self
                .admission
                .verify_cold_local(client, target, reason)
                .await?;
            let live = self.admission.try_open_live_local(&key).await?;
            if reason != ReadReason::ExplicitRefresh
                && let Some(live) = &live
                && (live.is_loaded || live.replay_complete)
                && !tc.is_revalidating(&key).await
            {
                let hybrid = live.hybrid_preloaded || tc.is_hybrid_resumed(&key);
                let read = if hybrid && !live.replay_complete {
                    live.reader.open_current(client, &key).await?
                } else {
                    live.reader.open(client, &key, reason).await?
                };
                return Ok(NativeLocalViewRead {
                    read,
                    reader: Arc::clone(&live.reader),
                    cached: None,
                });
            }
            if !live.as_ref().is_some_and(|candidate| candidate.is_loaded)
                && let Ok(Some(snapshot)) = tc.store().load_snapshot(&key).await
            {
                if tc.store().is_snapshot_expired(&snapshot.session) {
                    let _ = tc.store().enforce_retention_and_budget(None).await;
                } else {
                    let (mut status, mut needs_probe) = tc.initial_cold_status(&snapshot.session);
                    let profile = snapshot.session.capability_profile;
                    if reason == ReadReason::ExplicitRefresh
                        && status.continuable
                        && matches!(
                            profile,
                            CapabilityProfile::ReplayCapable | CapabilityProfile::Unknown
                        )
                    {
                        status.sync_state = TranscriptSyncState::Revalidating;
                        needs_probe = true;
                    }
                    let (read, reader, handle) = tc
                        .open_cached_local_view(&key, client, &self.authority, snapshot, status)
                        .await?;
                    if needs_probe {
                        let admission = Arc::clone(&self.admission);
                        let coordinator = Arc::clone(tc);
                        let key_owned = key.clone();
                        tc.spawn_singleflight_revalidation(key.clone(), async move {
                            admission
                                .probe_and_revalidate_target(
                                    actor,
                                    target,
                                    key_owned,
                                    profile,
                                    coordinator,
                                )
                                .await;
                        })
                        .await;
                    }
                    return Ok(NativeLocalViewRead {
                        read,
                        reader,
                        cached: Some(handle),
                    });
                }
            }
        }
        let admitted = self.admission.admit(client, target, reason).await?;
        let read = admitted
            .reader
            .open(client, &admitted.target, reason)
            .await?;
        Ok(NativeLocalViewRead {
            read,
            reader: admitted.reader,
            cached: None,
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
        if let Some(tc) = &self.transcript_cache {
            let key = admission.verify_cold_target(client, target, reason).await?;
            let (live_read, live_loaded) =
                try_open_live_socket_read(tc, admission.as_ref(), client, &key, reason).await?;
            if let Some(read) = live_read {
                return Ok(read);
            }
            if !live_loaded && let Ok(Some(snapshot)) = tc.store().load_snapshot(&key).await {
                if tc.store().is_snapshot_expired(&snapshot.session) {
                    let _ = tc.store().enforce_retention_and_budget(None).await;
                } else {
                    let profile = snapshot.session.capability_profile;
                    let (status, needs_probe) = prepare_cold_status(tc, &snapshot.session, reason);
                    let (read, reader, handle) = tc
                        .open_cached_socket_view(&key, client, authority, snapshot, status, false)
                        .await?;
                    if needs_probe {
                        let probe_admission = Arc::clone(admission);
                        let probe_client = client.context.client();
                        let coordinator = Arc::clone(tc);
                        let key_owned = key.clone();
                        tc.spawn_singleflight_revalidation(key, async move {
                            probe_admission
                                .probe_and_revalidate_socket_target(
                                    probe_client,
                                    target,
                                    key_owned,
                                    profile,
                                    coordinator,
                                )
                                .await;
                        })
                        .await;
                    }
                    return Ok(NativeSocketViewRead {
                        read,
                        reader,
                        history: None,
                        backing: SocketReadBacking::Cached {
                            handle,
                            authority: Arc::clone(authority),
                            admission: Arc::clone(admission),
                            target: SocketRebindTarget::Target(target),
                            coordinator: Arc::clone(tc),
                        },
                    });
                }
            }
        }
        let admitted = admission
            .admit_socket(client, target, reason, SocketReadPurpose::Open)
            .await?;
        self.finish_live_socket_read(client, admitted, reason).await
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
        if let Some(tc) = &self.transcript_cache {
            let key = admission
                .verify_cold_source(client, selection.clone(), native_session_id.clone())
                .await?;
            let (live_read, live_loaded) =
                try_open_live_socket_read(tc, admission.as_ref(), client, &key, reason).await?;
            if let Some(read) = live_read {
                return Ok(read);
            }
            if !live_loaded && let Ok(Some(snapshot)) = tc.store().load_snapshot(&key).await {
                if tc.store().is_snapshot_expired(&snapshot.session) {
                    let _ = tc.store().enforce_retention_and_budget(None).await;
                } else {
                    let profile = snapshot.session.capability_profile;
                    let (status, needs_probe) = prepare_cold_status(tc, &snapshot.session, reason);
                    let (read, reader, handle) = tc
                        .open_cached_socket_view(&key, client, authority, snapshot, status, false)
                        .await?;
                    let rebind_target = SocketRebindTarget::Source {
                        selection: selection.clone(),
                        native_session_id: native_session_id.clone(),
                    };
                    if needs_probe {
                        let probe_admission = Arc::clone(admission);
                        let probe_client = client.context.client();
                        let coordinator = Arc::clone(tc);
                        let key_owned = key.clone();
                        tc.spawn_singleflight_revalidation(key, async move {
                            probe_admission
                                .probe_and_revalidate_socket_source(
                                    probe_client,
                                    selection,
                                    native_session_id,
                                    key_owned,
                                    profile,
                                    coordinator,
                                )
                                .await;
                        })
                        .await;
                    }
                    return Ok(NativeSocketViewRead {
                        read,
                        reader,
                        history: None,
                        backing: SocketReadBacking::Cached {
                            handle,
                            authority: Arc::clone(authority),
                            admission: Arc::clone(admission),
                            target: rebind_target,
                            coordinator: Arc::clone(tc),
                        },
                    });
                }
            }
        }
        let admitted = admission
            .admit_socket_source(client, selection, native_session_id, reason)
            .await?;
        self.finish_live_socket_read(client, admitted, reason).await
    }
    async fn finish_live_socket_read(
        &self,
        client: &SocketViewClient,
        admitted: AdmittedSocketNativeView,
        reason: ReadReason,
    ) -> Result<NativeSocketViewRead, ViewError> {
        let read = admitted
            .reader
            .open(client, &admitted.target, reason)
            .await?;
        let live_status = self.transcript_cache.as_ref().map(|tc| {
            let replay_complete = read.descriptor().read_mode() == ReadMode::ReplayComplete;
            let hybrid = tc.is_hybrid_resumed(&admitted.target);
            tc.live_status(&admitted.target, hybrid, replay_complete)
        });
        Ok(NativeSocketViewRead {
            read,
            reader: admitted.reader,
            history: None,
            backing: SocketReadBacking::Live(live_status),
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
        if let Some(cached) = &self.cached {
            self.reader.check_current(client, cached.key()).await?;
            return Ok(match *cached.revalidation_receiver().borrow() {
                RevalidationOutcome::Pending => NativeLocalReadProgress::Pending,
                RevalidationOutcome::SyncedReady | RevalidationOutcome::StayCached => {
                    NativeLocalReadProgress::Complete
                }
            });
        }
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
        if self.cached.is_some() {
            return Err(ViewError::InvalidStage);
        }
        let rebound = self
            .reader
            .rebind_inflight_same_read(client, &self.read)
            .await?;
        let (read, metadata) = rebound.into_parts();
        Ok(NativeLocalViewContinuation {
            read: Self {
                read,
                reader: Arc::clone(&self.reader),
                cached: None,
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
        if let Some(cached) = &self.cached {
            let mut rx = cached.revalidation_receiver();
            if *rx.borrow_and_update() == RevalidationOutcome::SyncedReady {
                return Err(ViewError::StaleCursor);
            }
            tokio::select! {
                res = self.reader.wait_change(client, &self.read) => res,
                changed = rx.changed() => {
                    if changed.is_ok() && *rx.borrow() == RevalidationOutcome::SyncedReady {
                        Err(ViewError::StaleCursor)
                    } else {
                        self.reader.wait_change(client, &self.read).await
                    }
                }
            }
        } else {
            self.reader.wait_change(client, &self.read).await
        }
    }
    #[must_use]
    pub fn release(self) -> ReadCloseReceipt {
        if let Some(cached) = &self.cached {
            cached.seal_on_release();
        }
        self.reader.release(self.read)
    }
}
impl NativeSocketViewRead {
    /// Returns current transcript cache presentation metadata for `view.opened`, if enabled.
    #[must_use]
    pub fn transcript_cache_status(&self) -> Option<TranscriptCacheViewStatus> {
        match &self.backing {
            SocketReadBacking::Live(status) => *status,
            SocketReadBacking::Cached { handle, .. } => Some(handle.status()),
        }
    }
    /// # Errors
    /// Reads only this original operation's progress under current socket authentication.
    pub async fn original_progress(
        &self,
        client: &SocketViewClient,
    ) -> Result<OriginalReadProgress, ViewError> {
        match &self.backing {
            SocketReadBacking::Live(_) => self.reader.original_progress(client, &self.read).await,
            SocketReadBacking::Cached {
                handle, authority, ..
            } => {
                let _gate = authority.current_guard(client, handle.key()).await?;
                match *handle.revalidation_receiver().borrow() {
                    RevalidationOutcome::SyncedReady => Ok(OriginalReadProgress::Complete {
                        read: ReadEpoch::fresh()?,
                        epoch: self.read.descriptor().view_epoch(),
                    }),
                    RevalidationOutcome::Pending | RevalidationOutcome::StayCached => {
                        Ok(OriginalReadProgress::Pending)
                    }
                }
            }
        }
    }
    /// # Errors
    /// Waits for this original read operation to finish replay or publish its replacement generation.
    pub async fn wait_original_progress(
        &self,
        client: &SocketViewClient,
    ) -> Result<OriginalReadProgress, ViewError> {
        match &self.backing {
            _ if self
                .history
                .as_ref()
                .is_some_and(|h| h.request.open.is_none()) =>
            {
                Err(ViewError::InvalidStage)
            }
            SocketReadBacking::Live(_) => {
                self.reader.wait_original_progress(client, &self.read).await
            }
            SocketReadBacking::Cached {
                handle, authority, ..
            } => {
                let mut rx = handle.revalidation_receiver();
                loop {
                    let gate = authority.current_guard(client, handle.key()).await?;
                    if *rx.borrow_and_update() == RevalidationOutcome::SyncedReady {
                        return Ok(OriginalReadProgress::Complete {
                            read: ReadEpoch::fresh()?,
                            epoch: self.read.descriptor().view_epoch(),
                        });
                    }
                    drop(gate);
                    tokio::select! {
                        changed = rx.changed() => {
                            if changed.is_err() {
                                return Err(ViewError::StreamSealed);
                            }
                        }
                        revoked = self.reader.wait_client_change(client) => {
                            revoked?;
                        }
                    }
                }
            }
        }
    }
    /// # Errors
    /// Waits for this original read operation to finish replay or fail while still valid.
    pub async fn wait_original_completion(
        &self,
        client: &SocketViewClient,
    ) -> Result<(), ViewError> {
        match &self.backing {
            SocketReadBacking::Live(_) => {
                self.reader
                    .wait_original_completion(client, &self.read)
                    .await
            }
            SocketReadBacking::Cached {
                handle, authority, ..
            } => {
                let mut rx = handle.revalidation_receiver();
                loop {
                    let gate = authority.current_guard(client, handle.key()).await?;
                    match *rx.borrow_and_update() {
                        RevalidationOutcome::SyncedReady => return Ok(()),
                        RevalidationOutcome::StayCached => return Err(ViewError::ReadFailure),
                        RevalidationOutcome::Pending => {}
                    }
                    drop(gate);
                    tokio::select! {
                        changed = rx.changed() => {
                            if changed.is_err() {
                                return Err(ViewError::StreamSealed);
                            }
                        }
                        revoked = self.reader.wait_client_change(client) => {
                            revoked?;
                        }
                    }
                }
            }
        }
    }
    /// # Errors
    /// Rebinds this completed original read onto its published replay-complete generation.
    pub async fn rebind_same_read(&self, client: &SocketViewClient) -> Result<Self, ViewError> {
        if self
            .history
            .as_ref()
            .is_some_and(|h| h.request.open.is_none())
        {
            return Err(ViewError::InvalidStage);
        }
        let mut next = self.rebind_original(client).await?;
        if let Some(bound) = &self.history {
            next = bound
                .request
                .clone()
                .select_live(client, next, &bound.key, &bound.tc, "live")
                .await
                .map_err(|e| match e {
                    history::HistoryError::View(e) => e,
                    history::HistoryError::Page(_) => ViewError::StaleCursor,
                })?;
        }
        Ok(next)
    }
    pub(crate) fn page_metadata(&self) -> Option<&serde_json::Value> {
        self.history.as_ref().map(|h| &h.metadata)
    }
    async fn rebind_original(&self, client: &SocketViewClient) -> Result<Self, ViewError> {
        match &self.backing {
            SocketReadBacking::Live(status) => {
                let rebound = self.reader.rebind_same_read(client, &self.read).await?;
                let (read, _) = rebound.into_parts();
                let next_status = status.map(|mut prev| {
                    if read.descriptor().read_mode() == ReadMode::ReplayComplete {
                        prev.transcript_source = TranscriptSource::Live;
                        prev.sync_state = TranscriptSyncState::Synced;
                        prev.capability_boundary = TranscriptCapabilityBoundary::ReplayCapable;
                        prev.continuable = true;
                    }
                    prev
                });
                Ok(Self {
                    read,
                    reader: Arc::clone(&self.reader),
                    history: None,
                    backing: SocketReadBacking::Live(next_status),
                })
            }
            SocketReadBacking::Cached {
                handle,
                authority,
                admission,
                target,
                coordinator,
            } => {
                let gate = authority.current_guard(client, handle.key()).await?;
                if *handle.revalidation_receiver().borrow() != RevalidationOutcome::SyncedReady {
                    return Err(ViewError::InvalidStage);
                }
                drop(gate);
                if let Some(live) = admission.try_open_live_socket(handle.key()).await? {
                    let read = live.reader.open_current(client, handle.key()).await?;
                    let hybrid = (live.hybrid_preloaded
                        || coordinator.is_hybrid_resumed(handle.key()))
                        && !live.replay_complete;
                    let replay_complete = read.descriptor().read_mode() == ReadMode::ReplayComplete;
                    let status = coordinator.live_status(handle.key(), hybrid, replay_complete);
                    return Ok(Self {
                        read,
                        reader: live.reader,
                        history: None,
                        backing: SocketReadBacking::Live(Some(status)),
                    });
                }
                let (reader, live_target) = match target {
                    SocketRebindTarget::Target(target_id) => {
                        let admitted = admission
                            .admit_socket(
                                client,
                                *target_id,
                                ReadReason::ExplicitRefresh,
                                SocketReadPurpose::Open,
                            )
                            .await?;
                        (admitted.reader, admitted.target)
                    }
                    SocketRebindTarget::Source {
                        selection,
                        native_session_id,
                    } => {
                        let admitted = admission
                            .admit_socket_source(
                                client,
                                selection.clone(),
                                native_session_id.clone(),
                                ReadReason::ExplicitRefresh,
                            )
                            .await?;
                        (admitted.reader, admitted.target)
                    }
                };
                let read = reader.open_current(client, &live_target).await?;
                let replay_complete = read.descriptor().read_mode() == ReadMode::ReplayComplete;
                let hybrid = coordinator.is_hybrid_resumed(&live_target) && !replay_complete;
                let status = coordinator.live_status(&live_target, hybrid, replay_complete);
                Ok(Self {
                    read,
                    reader,
                    history: None,
                    backing: SocketReadBacking::Live(Some(status)),
                })
            }
        }
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
        if let SocketReadBacking::Cached { handle, .. } = &self.backing {
            let mut rx = handle.revalidation_receiver();
            if *rx.borrow_and_update() == RevalidationOutcome::SyncedReady {
                return Err(ViewError::StaleCursor);
            }
            tokio::select! {
                res = self.reader.wait_change(client, &self.read) => res,
                changed = rx.changed() => {
                    if changed.is_ok() && *rx.borrow() == RevalidationOutcome::SyncedReady {
                        Err(ViewError::StaleCursor)
                    } else {
                        self.reader.wait_change(client, &self.read).await
                    }
                }
            }
        } else {
            self.reader.wait_change(client, &self.read).await
        }
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
        if let SocketReadBacking::Cached { handle, .. } = &self.backing {
            handle.seal_on_release();
        }
        self.reader.release(self.read)
    }
}
