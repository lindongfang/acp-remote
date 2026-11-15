//! Linear launch authority, complete configuration and owned source cleanup.
#![allow(
    clippy::type_complexity,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    AcceptedTargetProof, Arc, BoxFuture, ConfigurationKey, DispatchError, EntityId, Instant,
    NativeOwnerKind, NativeSessionKey, ProtocolVersion,
};
use crate::{
    application::request_registry::RegistryOwnerScope,
    domain::source_sessions::SourceLaunchSelection,
};
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPhase {
    Reserved,
    Entered,
    RevokedNeverEntered,
}
pub struct SourceLaunchController {
    phase: Mutex<LaunchPhase>,
}
pub struct LaunchPermit {
    controller: Arc<SourceLaunchController>,
}
pub struct EnteredLaunch {
    controller: Arc<SourceLaunchController>,
}
pub struct RevokedLaunchAuthority {
    controller: Arc<SourceLaunchController>,
}
impl EnteredLaunch {
    pub(crate) fn require_original(
        &self,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        if !Arc::ptr_eq(&self.controller, controller) {
            return Err(DispatchError::InvalidSource);
        }
        self.require_entered()
    }
    pub(crate) fn require_entered(&self) -> Result<(), DispatchError> {
        if *self
            .controller
            .phase
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?
            == LaunchPhase::Entered
        {
            Ok(())
        } else {
            Err(DispatchError::Stopped)
        }
    }
}
impl RevokedLaunchAuthority {
    pub(super) fn require_original(
        &self,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        if Arc::ptr_eq(&self.controller, controller)
            && *self
                .controller
                .phase
                .lock()
                .map_err(|_| DispatchError::ResourceUnconfirmed)?
                == LaunchPhase::RevokedNeverEntered
        {
            Ok(())
        } else {
            Err(DispatchError::ResourceUnconfirmed)
        }
    }
}
impl SourceLaunchController {
    pub(super) fn current_phase(&self) -> Result<LaunchPhase, DispatchError> {
        self.phase
            .lock()
            .map(|phase| *phase)
            .map_err(|_| DispatchError::ResourceUnconfirmed)
    }
    pub(super) fn create() -> (Arc<Self>, LaunchPermit) {
        let c = Arc::new(Self {
            phase: Mutex::new(LaunchPhase::Reserved),
        });
        (c.clone(), LaunchPermit { controller: c })
    }
    pub(super) fn revoke_unentered(
        self: &Arc<Self>,
    ) -> Result<RevokedLaunchAuthority, DispatchError> {
        let mut p = self
            .phase
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if *p != LaunchPhase::Reserved {
            return Err(DispatchError::ResourceUnconfirmed);
        }
        *p = LaunchPhase::RevokedNeverEntered;
        drop(p);
        Ok(RevokedLaunchAuthority {
            controller: self.clone(),
        })
    }
}
impl LaunchPermit {
    /// Consumes the only future launch permission immediately before invoking the real provider.
    /// # Errors
    /// A prior irreversible revoke prevents provider entry.
    pub fn enter(self) -> Result<EnteredLaunch, DispatchError> {
        let mut p = self
            .controller
            .phase
            .lock()
            .map_err(|_| DispatchError::ResourceUnconfirmed)?;
        if *p != LaunchPhase::Reserved {
            return Err(DispatchError::Stopped);
        }
        *p = LaunchPhase::Entered;
        drop(p);
        Ok(EnteredLaunch {
            controller: self.controller,
        })
    }
}
pub struct LaunchAdopter<O> {
    pub(super) adopt: Box<dyn FnOnce(O) + Send>,
}
impl<O> LaunchAdopter<O> {
    pub fn adopt(self, owner: O) {
        (self.adopt)(owner);
    }
}
pub enum LaunchCompletion<R> {
    OwnedRunning(R),
    OwnedFailed(DispatchError),
    BeforeEntryFailed {
        error: DispatchError,
        permit: LaunchPermit,
    },
    Revoked,
    EnteredUnconfirmed(DispatchError),
}
pub(super) enum LaunchOutcome<R> {
    Adopted,
    Running(Arc<R>),
    Failed,
}
#[derive(Clone, Copy)]
pub enum SourceCreationCut {
    BeforeCreateCall,
    CreateCallRejected,
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep the explicit crate-only owner witness boundary across the parent glob re-export."
)]
pub(crate) mod owner_witness_sealed {
    pub trait Sealed {}
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep the explicit crate-only owner witness boundary across the parent glob re-export."
)]
pub(crate) trait NeverCreatedWitness: owner_witness_sealed::Sealed + Send + Sync {
    fn cut(&self) -> SourceCreationCut;
}
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep the explicit crate-only owner witness boundary across the parent glob re-export."
)]
pub(crate) trait SuspendedUnassignedWitness:
    owner_witness_sealed::Sealed + Send + Sync
{
    fn root_exit_code(&self) -> u32;
}
pub struct OwnedNeverCreatedClosure {
    entered: Arc<EnteredLaunch>,
    witness: Arc<dyn NeverCreatedWitness>,
}
pub struct OwnedSuspendedUnassignedClosure {
    entered: Arc<EnteredLaunch>,
    witness: Arc<dyn SuspendedUnassignedWitness>,
}
impl OwnedNeverCreatedClosure {
    pub(crate) fn from_actual(
        entered: Arc<EnteredLaunch>,
        witness: Arc<dyn NeverCreatedWitness>,
    ) -> Self {
        Self { entered, witness }
    }
    pub(super) fn require_original(
        &self,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<SourceCreationCut, DispatchError> {
        self.entered.require_original(controller)?;
        Ok(self.witness.cut())
    }
    pub(crate) fn creation_cut(&self) -> SourceCreationCut {
        self.witness.cut()
    }
}
impl OwnedSuspendedUnassignedClosure {
    pub(crate) fn from_actual(
        entered: Arc<EnteredLaunch>,
        witness: Arc<dyn SuspendedUnassignedWitness>,
    ) -> Self {
        Self { entered, witness }
    }
    pub(super) fn require_original(
        &self,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError> {
        self.entered.require_original(controller)
    }
    pub(super) fn root_exit_code(&self) -> u32 {
        self.witness.root_exit_code()
    }
}
#[derive(Clone)]
#[expect(
    clippy::redundant_pub_crate,
    reason = "Keep the explicit crate-only owner witness boundary across the parent glob re-export."
)]
pub(crate) enum OwnerClosureWitness {
    NeverCreated(Arc<OwnedNeverCreatedClosure>),
    SuspendedUnassigned(Arc<OwnedSuspendedUnassignedClosure>),
}
pub enum OwnedSourceShutdown {
    WindowsJob {
        active_processes: u32,
        root_exit_code: Option<u32>,
        stderr_joined: bool,
    },
    TestOwnedStream {
        tasks_joined: bool,
    },
    NeverCreated(Arc<OwnedNeverCreatedClosure>),
    SuspendedUnassigned(Arc<OwnedSuspendedUnassignedClosure>),
}
pub trait OwnedNativeSource: Send + 'static {
    type Streams: Send + 'static;
    /// # Errors
    /// Rejects nonrunning, stopped or already-consumed streams; it does not create replacement IO.
    fn take_streams(&mut self) -> Result<Self::Streams, DispatchError>;
    fn owner_kind(&self) -> NativeOwnerKind;
    fn request_stop(&self);
    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> BoxFuture<'_, Result<OwnedSourceShutdown, DispatchError>>;
}
pub trait SourceLaunchPort: Send + Sync + 'static {
    type Plan: Send + Sync + 'static;
    type Owned: OwnedNativeSource;
    type Running: Send + Sync + 'static;
    /// # Errors
    /// Requires the original completed running proof and this same adopted owner/entered controller.
    fn verify_running(
        &self,
        owner: &Self::Owned,
        proof: &Self::Running,
        controller: &Arc<SourceLaunchController>,
    ) -> Result<(), DispatchError>;
    fn same_plan(&self, current: &Self::Plan, candidate: &Self::Plan) -> bool;
    fn owner_kind(&self) -> NativeOwnerKind;
    fn launch(
        &self,
        plan: Arc<Self::Plan>,
        permit: LaunchPermit,
        adopter: LaunchAdopter<Self::Owned>,
        deadline: Instant,
    ) -> BoxFuture<'_, LaunchCompletion<Self::Running>>;
}
pub struct ConfiguredSourcePlan<LP, S> {
    pub source_id: EntityId,
    pub target: EntityId,
    pub native: NativeSessionKey,
    pub workspace: EntityId,
    pub harness: EntityId,
    pub configuration: ConfigurationKey,
    pub launch: Arc<LP>,
    pub setups: Vec<(ProtocolVersion, S)>,
}
/// Source initialization has no resource target or invented native session identity.
pub struct SourceInitializationPlan<LP, S> {
    pub source_id: EntityId,
    pub workspace: EntityId,
    pub harness: EntityId,
    pub configuration: ConfigurationKey,
    pub launch: Arc<LP>,
    pub setups: Vec<(ProtocolVersion, S)>,
}
impl<LP, S> ConfiguredSourcePlan<LP, S> {
    pub(super) fn into_source(self) -> SourceInitializationPlan<LP, S> {
        SourceInitializationPlan {
            source_id: self.source_id,
            workspace: self.workspace,
            harness: self.harness,
            configuration: self.configuration,
            launch: self.launch,
            setups: self.setups,
        }
    }
}

/// Authenticated read-only view proof for an external native session without a queue target.
pub struct SourceReadSelectionProof {
    scope: RegistryOwnerScope,
    selection: SourceLaunchSelection,
    native: NativeSessionKey,
}
impl SourceReadSelectionProof {
    pub(crate) fn mint(
        scope: RegistryOwnerScope,
        selection: SourceLaunchSelection,
        native_session_id: &str,
    ) -> Result<Self, DispatchError> {
        scope
            .require_open()
            .map_err(|_| DispatchError::NodeStopping)?;
        if selection.launch_path.is_empty() || selection.launch_path.contains('\0') {
            return Err(DispatchError::InvalidConfiguration);
        }
        if native_session_id.contains('\0') {
            return Err(DispatchError::InvalidTarget);
        }
        let native = NativeSessionKey::from_config(Some(selection.source), native_session_id)
            .map_err(|_| DispatchError::InvalidTarget)?;
        Ok(Self {
            scope,
            selection,
            native,
        })
    }
    #[must_use]
    pub(crate) const fn selection(&self) -> &SourceLaunchSelection {
        &self.selection
    }
    #[must_use]
    pub(crate) const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    pub(crate) fn require_current(&self) -> Result<(), DispatchError> {
        self.scope
            .require_open()
            .map_err(|_| DispatchError::NodeStopping)
    }
    pub(crate) fn verify_registry_owner(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), DispatchError> {
        if !self.scope.matches(scope) {
            return Err(DispatchError::InvalidScope);
        }
        self.scope
            .require_open()
            .map_err(|_| DispatchError::NodeStopping)
    }
}

pub type ConfigurationResolution<'a, L, S> =
    BoxFuture<'a, Result<ConfiguredSourcePlan<L, S>, DispatchError>>;
pub trait ConfigurationPort: Send + Sync + 'static {
    type LaunchPlan: Send + Sync + 'static;
    type Setup: Clone + Send + Sync + 'static;
    fn resolve<'a>(
        &'a self,
        proof: &'a AcceptedTargetProof,
    ) -> ConfigurationResolution<'a, Self::LaunchPlan, Self::Setup>;
}

/// Resolves the complete configuration for authenticated viewing without a queue input.
pub trait ReadConfigurationPort: ConfigurationPort {
    fn resolve_read<'a>(
        &'a self,
        proof: &'a crate::application::queue_owner::ReadTargetProof,
    ) -> ConfigurationResolution<'a, Self::LaunchPlan, Self::Setup>;
    fn resolve_source_read<'a>(
        &'a self,
        proof: &'a SourceReadSelectionProof,
    ) -> BoxFuture<'a, Result<SourceInitializationPlan<Self::LaunchPlan, Self::Setup>, DispatchError>>;
}
