//! Source-only operations use the original runtime, configuration provider and SQL owner.
#![allow(
    clippy::needless_pass_by_value,
    clippy::redundant_pub_crate,
    clippy::significant_drop_tightening,
    clippy::type_complexity,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{BoxFuture, ConfigurationPort, SourceInitializationPlan};
use crate::{
    application::request_registry::RegistryOwnerScope,
    domain::{
        ActorContext,
        native::RuntimeEpoch,
        source_sessions::{
            SourceAckState, SourceFailureClass, SourceLaunchSelection, SourceResponseFact,
            SourceResponseKind, SourceSessionAction, SourceSessionError, SourceSessionFact,
            SourceSessionFailure, SourceSessionOperationId, SourceSessionOutcome,
            SourceSessionRegistration, SourceSessionRegistrationMetadata, SourceSessionState,
            SourceSqlObservation, SourceSqlRole, SourceTaskObservation,
        },
    },
};

/// A single durably admitted operation, not cached authorization for a later command.
/// Only this module can construct it after its original necessary Owned row is confirmed.
pub struct SourceAccessProof {
    registration: SourceSessionRegistration,
    runtime: RuntimeEpoch,
    scope: RegistryOwnerScope,
    admission: ActorContext,
}
impl SourceAccessProof {
    pub(crate) const fn selection(&self) -> &SourceLaunchSelection {
        &self.registration.selection
    }
    pub(crate) const fn operation_id(&self) -> SourceSessionOperationId {
        self.registration.id
    }
    pub(super) fn require_source(
        &self,
        source: &super::PreSourceProbe,
    ) -> Result<(), super::DispatchError> {
        self.scope.require_open()?;
        let metadata = source.metadata();
        if metadata.runtime != self.runtime
            || metadata.source_id != self.registration.selection.source
            || metadata.config_identity != self.registration.selection.profile.id()
            || metadata.config_revision != self.registration.selection.profile.revision()
        {
            return Err(super::DispatchError::InvalidScope);
        }
        Ok(())
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        if !self.scope.matches(scope)
            || self.admission.client_id != self.registration.actor
            || self.admission.auth_generation < 0
        {
            return Err(SourceSessionError::InvalidScope);
        }
        self.scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)?;
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
}
/// No target, `NativeSessionKey` or read proof is manufactured by this projection.
pub trait SourceConfigurationPort: ConfigurationPort {
    fn source_configuration_scope(&self) -> RegistryOwnerScope;
    fn resolve_source<'a>(
        &'a self,
        proof: &'a SourceAccessProof,
    ) -> BoxFuture<
        'a,
        Result<SourceInitializationPlan<Self::LaunchPlan, Self::Setup>, SourceSessionFailure>,
    >;
}

use super::{Arc, PreSourceProbe, ProtocolVersion, RpcScalar, TransportFact};
use crate::application::request_registry::RegistryDeadline;
use crate::domain::{EntityId, native::NativeSessionKey};

/// The outer authority verifies its actual issued actor, never a value-only actor DTO.
pub trait SourceSessionAuthority<C>: Send + Sync + 'static {
    fn verify_owner_scope(&self, scope: &RegistryOwnerScope) -> Result<(), SourceSessionError>;
    fn current<'a>(
        &'a self,
        actor: &'a C,
    ) -> BoxFuture<'a, Result<ActorContext, SourceSessionError>>;
}
#[derive(Clone)]
pub struct SourceStoredOperation {
    pub(crate) registration: SourceSessionRegistration,
    pub(crate) fact: SourceSessionFact,
}
pub enum SourceAdmissionResult {
    Admitted(SourceStoredOperation),
    Existing(SourceStoredOperation),
}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceSqlTestFault {
    DeferredCommit,
    StatementError,
}
pub trait SourceSqlReceipt: Send + Sync + 'static {
    fn role(&self) -> SourceSqlRole;
    #[cfg(test)]
    fn original_cleanup_for_test(&self) -> Option<SourceSqlOriginalSnapshot>;
    #[cfg(test)]
    fn arm_fault_for_test(&self, _fault: SourceSqlTestFault) {}
    #[cfg(test)]
    fn sql_started_for_test(&self) -> usize {
        0
    }
    fn observe(&self) -> Option<SourceSqlObservation>;
    fn confirm_original<'a>(
        &'a self,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<SourceSqlObservation>, SourceSessionFailure>>;
}
#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceSqlOriginalSnapshot {
    pub(crate) owner: usize,
    pub(crate) close_slot: usize,
    pub(crate) actual_ack: u8,
    pub(crate) close_task_join: u8,
    pub(crate) worker_registered: bool,
    pub(crate) worker_joined: Option<bool>,
    pub(crate) app_join: Option<bool>,
}
/// A body-free operation proof is created only after the current actor/scope check.
pub struct SourceRegistrationProof {
    registration: SourceSessionRegistration,
    scope: RegistryOwnerScope,
    generated_id: bool,
}
impl SourceRegistrationProof {
    pub(crate) const fn registration(&self) -> &SourceSessionRegistration {
        &self.registration
    }
    pub(crate) const fn generated_id(&self) -> bool {
        self.generated_id
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        if !self.scope.matches(scope) {
            return Err(SourceSessionError::InvalidScope);
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
}
pub struct SourceReaderProof {
    scope: RegistryOwnerScope,
    actor: ActorContext,
}
impl SourceReaderProof {
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        if !self.scope.matches(scope) || self.actor.auth_generation < 0 {
            return Err(SourceSessionError::InvalidScope);
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
}
pub struct SourceAdmissionProof {
    original: Arc<SourceRegistrationProof>,
    runtime: RuntimeEpoch,
    actor: ActorContext,
    at: i64,
}
impl SourceAdmissionProof {
    pub(crate) fn registration(&self) -> &SourceSessionRegistration {
        self.original.registration()
    }
    pub(crate) const fn runtime(&self) -> RuntimeEpoch {
        self.runtime
    }
    pub(crate) const fn actor(&self) -> ActorContext {
        self.actor
    }
    pub(crate) const fn at(&self) -> i64 {
        self.at
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        self.original.verify_owner_scope(scope)?;
        if self.actor.client_id != self.original.registration.actor
            || self.actor.auth_generation < 0
        {
            return Err(SourceSessionError::InvalidActor);
        }
        Ok(())
    }
}
pub enum SourceUpdateView<'a> {
    Attach {
        source: &'a PreSourceProbe,
        wire: ProtocolVersion,
    },
    Reserve {
        source: &'a PreSourceProbe,
        wire: ProtocolVersion,
        rpc: &'a RpcScalar,
        frame_length: u64,
    },
    Arm {
        rpc: &'a RpcScalar,
    },
    Transport {
        rpc: &'a RpcScalar,
        frame_length: u64,
        fact: TransportFact,
    },
    Response {
        rpc: &'a RpcScalar,
        fact: &'a SourceResponseFact,
    },
    Finish {
        outcome: SourceSessionOutcome,
        failure: Option<SourceFailureClass>,
    },
}
#[derive(Clone)]
enum SourceUpdate {
    Attach {
        source: Arc<PreSourceProbe>,
        wire: ProtocolVersion,
    },
    Reserve {
        source: Arc<PreSourceProbe>,
        wire: ProtocolVersion,
        rpc: RpcScalar,
        frame_length: u64,
    },
    Arm {
        rpc: RpcScalar,
    },
    Transport {
        rpc: RpcScalar,
        frame_length: u64,
        fact: TransportFact,
    },
    Response {
        rpc: RpcScalar,
        fact: SourceResponseFact,
    },
    Finish {
        outcome: SourceSessionOutcome,
        failure: Option<SourceFailureClass>,
    },
}
#[derive(Clone)]
pub struct SourceWriteProof {
    access: Arc<SourceAccessProof>,
    update: SourceUpdate,
    at: i64,
}
impl SourceWriteProof {
    pub(crate) fn operation_id(&self) -> SourceSessionOperationId {
        self.access.operation_id()
    }
    pub(crate) fn runtime(&self) -> RuntimeEpoch {
        self.access.runtime
    }
    pub(crate) const fn at(&self) -> i64 {
        self.at
    }
    pub(crate) fn view(&self) -> SourceUpdateView<'_> {
        match &self.update {
            SourceUpdate::Attach { source, wire } => SourceUpdateView::Attach {
                source,
                wire: *wire,
            },
            SourceUpdate::Reserve {
                source,
                wire,
                rpc,
                frame_length,
            } => SourceUpdateView::Reserve {
                source,
                wire: *wire,
                rpc,
                frame_length: *frame_length,
            },
            SourceUpdate::Arm { rpc } => SourceUpdateView::Arm { rpc },
            SourceUpdate::Transport {
                rpc,
                frame_length,
                fact,
            } => SourceUpdateView::Transport {
                rpc,
                frame_length: *frame_length,
                fact: *fact,
            },
            SourceUpdate::Response { rpc, fact } => SourceUpdateView::Response { rpc, fact },
            SourceUpdate::Finish { outcome, failure } => SourceUpdateView::Finish {
                outcome: *outcome,
                failure: *failure,
            },
        }
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        self.access.verify_owner_scope(scope)?;
        match &self.update {
            SourceUpdate::Attach { source, .. } | SourceUpdate::Reserve { source, .. } => self
                .access
                .require_source(source)
                .map_err(|_| SourceSessionError::InvalidScope)?,
            _ => {}
        }
        Ok(())
    }
}
pub struct SourceRecoveryProof {
    scope: RegistryOwnerScope,
    runtime: RuntimeEpoch,
    at: i64,
}
impl SourceRecoveryProof {
    pub(crate) const fn runtime(&self) -> RuntimeEpoch {
        self.runtime
    }
    pub(crate) const fn at(&self) -> i64 {
        self.at
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        if !self.scope.matches(scope) {
            return Err(SourceSessionError::InvalidScope);
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
}
/// A restart deny barrier. It can never supply a live source, resource or load proof.
pub struct StoredNativeClaimBarrier {
    pub(super) native: NativeSessionKey,
    pub(super) instance: EntityId,
}
impl StoredNativeClaimBarrier {
    pub(crate) const fn from_necessary_row(native: NativeSessionKey, instance: EntityId) -> Self {
        Self { native, instance }
    }
}
#[derive(Clone)]
pub struct SourceReconcileProof {
    scope: RegistryOwnerScope,
    id: SourceSessionOperationId,
    original: Arc<SourceSqlGroup>,
}
impl SourceReconcileProof {
    pub(crate) const fn operation_id(&self) -> SourceSessionOperationId {
        self.id
    }
    pub(crate) fn verify_owner_scope(
        &self,
        scope: &RegistryOwnerScope,
    ) -> Result<(), SourceSessionError> {
        if !self.scope.matches(scope) {
            return Err(SourceSessionError::InvalidScope);
        }
        scope
            .require_open()
            .map_err(|_| SourceSessionError::NodeStopping)
    }
}
pub trait SourceSessionRepository: super::NativeResourceRepository {
    type SqlOperation: SourceSqlReceipt;
    fn source_owner_scope(&self) -> RegistryOwnerScope;
    fn source_sql_operation(
        &self,
        role: SourceSqlRole,
        id: Option<SourceSessionOperationId>,
    ) -> Result<Arc<Self::SqlOperation>, SourceSessionFailure>;
    fn register_source_session<'a>(
        &'a self,
        proof: &'a SourceRegistrationProof,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>>;
    fn admit_source_session<'a>(
        &'a self,
        proof: &'a SourceAdmissionProof,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceAdmissionResult, SourceSessionFailure>>;
    fn update_source_session<'a>(
        &'a self,
        proof: &'a SourceWriteProof,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>>;
    fn query_source_session<'a>(
        &'a self,
        proof: &'a SourceReaderProof,
        id: SourceSessionOperationId,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>>;
    fn reconcile_source_session<'a>(
        &'a self,
        proof: &'a SourceReconcileProof,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<SourceStoredOperation, SourceSessionFailure>>;
    fn recover_source_sessions<'a>(
        &'a self,
        proof: &'a SourceRecoveryProof,
        sql: Arc<Self::SqlOperation>,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Vec<StoredNativeClaimBarrier>, SourceSessionFailure>>;
}

/// Canonical framing of the complete registered intent. This digest grants no authority.
pub fn source_intent_digest(
    selection: &SourceLaunchSelection,
    action: &SourceSessionAction,
) -> Result<[u8; 32], SourceSessionError> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"acp-remote/source-session/intent/v1\0");
    for value in [
        selection.source.encode(),
        selection.harness.encode(),
        selection.profile.id().encode(),
        selection.workspace.encode(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(selection.profile.revision().to_be_bytes());
    for value in [selection.launch_path.as_str(), action.label()] {
        hash.update(
            u64::try_from(value.len())
                .map_err(|_| SourceSessionError::Allocation)?
                .to_be_bytes(),
        );
        hash.update(value.as_bytes());
    }
    if let Some(cursor) = action.cursor() {
        hash.update([1]);
        hash.update(
            u64::try_from(cursor.len())
                .map_err(|_| SourceSessionError::Allocation)?
                .to_be_bytes(),
        );
        hash.update(cursor.as_bytes());
    } else {
        hash.update([0]);
    }
    Ok(hash.finalize().into())
}

/// Only the original matched Created response path can construct this claim.
pub(super) struct OriginalCreatedClaim {
    source: Arc<PreSourceProbe>,
    native: NativeSessionKey,
}
impl OriginalCreatedClaim {
    pub(super) const fn source(&self) -> &Arc<PreSourceProbe> {
        &self.source
    }
    pub(super) const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceRequestKind {
    Discover,
    Create,
}
pub struct SourceReplyBinding<S> {
    scope: S,
    rpc: RpcScalar,
    kind: SourceRequestKind,
    sequence: u64,
    receipt: u64,
}
impl<S> SourceReplyBinding<S> {
    pub(crate) const fn scope(&self) -> &S {
        &self.scope
    }
    pub(crate) const fn rpc(&self) -> &RpcScalar {
        &self.rpc
    }
    pub(crate) const fn kind(&self) -> SourceRequestKind {
        self.kind
    }
    pub(crate) const fn sequence(&self) -> u64 {
        self.sequence
    }
    pub(crate) const fn receipt(&self) -> u64 {
        self.receipt
    }
}
pub enum SourceObservedResponse {
    Created(std::sync::Arc<str>),
    Sessions,
    RemoteError(i64),
}
pub struct OriginalSourceReply<R> {
    pub(crate) raw: R,
    pub(crate) response: SourceObservedResponse,
    pub(crate) deferred_decode:
        Option<Box<dyn FnOnce() -> Result<(), super::DispatchError> + Send>>,
}
pub struct SourceReplyEnd<S> {
    scope: S,
    original: Arc<super::SourceClosureProof>,
}
impl<S> SourceReplyEnd<S> {
    pub(crate) const fn scope(&self) -> &S {
        &self.scope
    }
}
pub trait NativeSourceSessionPort: super::NativeLifecyclePort {
    type SessionReservation: super::LifecycleReservation<Scope = Self::Scope, Frame = Self::Frame, Probe = Self::Probe>;
    type ResultRaw: crate::application::shared_views::RetainedRaw;
    type ReplyTracking: Clone + Send + Sync + 'static;
    fn reserve_discover(
        &self,
        source: &Self::Source,
        setup: &Self::BoundSetup,
        cursor: Option<&str>,
        wait: super::ResponseWait,
    ) -> Result<Self::SessionReservation, super::DispatchError>;
    fn reserve_create(
        &self,
        source: &Self::Source,
        setup: &Self::BoundSetup,
        wait: super::ResponseWait,
    ) -> Result<Self::SessionReservation, super::DispatchError>;
    fn enqueue_source_session(
        &self,
        source: &Self::Source,
        reservation: Self::SessionReservation,
        frame: Self::Frame,
        boundary: Arc<dyn super::LifecycleWriteBoundary<Self::Scope>>,
        deadline: super::Instant,
    ) -> Result<Self::Ticket, super::EnqueueRejected<Self::SessionReservation, Self::Frame>>;
    fn retain_source_reply(
        &self,
        lease: &Self::Lease,
        binding: &SourceReplyBinding<Self::Scope>,
    ) -> Result<OriginalSourceReply<Self::ResultRaw>, super::DispatchError>;
    fn source_reply_tracking(&self, reservation: &Self::SessionReservation) -> Self::ReplyTracking;
    fn acknowledge_source_reply(
        &self,
        tracking: &Self::ReplyTracking,
        binding: &SourceReplyBinding<Self::Scope>,
    );
    fn finish_source_reply_tracking(
        &self,
        tracking: &Self::ReplyTracking,
        end: &SourceReplyEnd<Self::Scope>,
    );
}

use crate::application::{
    document_disposal::DocumentPayloadSlot,
    document_source::{
        DocumentEpoch, DocumentError, DocumentSourceLease, SourceResultDocumentSource,
    },
    queue_owner::QueueRuntimeIdentity,
    shared_views::RetainedRaw,
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
struct ResultReadState {
    offer: bool,
    readers: usize,
    retired: bool,
}
pub struct OriginalSourceResult<R: RetainedRaw> {
    payload: DocumentPayloadSlot<R>,
    epoch: Arc<DocumentEpoch>,
    source: Arc<PreSourceProbe>,
    scope: RegistryOwnerScope,
    queue: QueueRuntimeIdentity,
    total_bytes: u64,
    state: Mutex<ResultReadState>,
    retired_offer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}
impl<R: RetainedRaw> OriginalSourceResult<R> {
    fn from_original(
        payload: DocumentPayloadSlot<R>,
        source: Arc<PreSourceProbe>,
        scope: RegistryOwnerScope,
        queue: QueueRuntimeIdentity,
        total_bytes: u64,
    ) -> Arc<Self> {
        let epoch = DocumentEpoch::new();
        let root = Arc::new(Self {
            payload,
            epoch: epoch.clone(),
            source: source.clone(),
            scope,
            queue,
            total_bytes,
            state: Mutex::new(ResultReadState {
                offer: true,
                readers: 0,
                retired: false,
            }),
            retired_offer: Mutex::new(None),
        });
        let participant: Arc<dyn crate::application::document_source::DocumentStreamSeal> =
            root.clone();
        let participant_registered = epoch.register(Arc::downgrade(&participant)).is_ok();
        let retained = {
            let _gate = source
                .authority
                .gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut epochs = source
                .authority
                .result_epochs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !participant_registered
                || source.authority.closed.load(Ordering::Acquire)
                || source.authority.closing.load(Ordering::Acquire)
            {
                false
            } else {
                epochs.retain(|epoch| epoch.strong_count() > 0);
                if epochs.try_reserve(1).is_ok() {
                    epochs.push(Arc::downgrade(&epoch));
                    true
                } else {
                    false
                }
            }
        };
        if !retained {
            root.withdraw_offer();
        }
        root
    }
    pub(crate) fn verify_destination(
        &self,
        scope: &RegistryOwnerScope,
        queue: &QueueRuntimeIdentity,
    ) -> Result<(), DocumentError> {
        self.payload.verify_brand(scope, queue)
    }
    fn require_current(&self) -> Result<(), DocumentError> {
        self.payload.verify_brand(&self.scope, &self.queue)?;
        if self.source.authority.closed.load(Ordering::Acquire)
            || self.source.authority.closing.load(Ordering::Acquire)
            || self
                .state
                .lock()
                .map_err(|_| DocumentError::Closed)?
                .retired
        {
            return Err(DocumentError::Closed);
        }
        self.payload.require_live()
    }
    pub(crate) fn available(&self) -> bool {
        self.require_current().is_ok()
    }
    pub(crate) fn on_offer_retired(&self, callback: Box<dyn FnOnce() + Send>) {
        let mut slot = self
            .retired_offer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retired = {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            !state.offer || state.retired
        };
        if retired {
            drop(slot);
            callback();
        } else {
            *slot = Some(callback);
        }
    }
    pub(crate) fn claimed(&self) {
        let callback = self
            .retired_offer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(callback);
    }
    fn notify_offer_retired(&self) {
        let callback = self
            .retired_offer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(callback) = callback {
            callback();
        }
    }
    pub(crate) fn withdraw_offer(&self) {
        let retire = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.offer = false;
            if state.readers == 0 && !state.retired {
                state.retired = true;
                true
            } else {
                false
            }
        };
        self.notify_offer_retired();
        if retire {
            self.payload.retire();
            self.epoch.seal_and_notify();
        }
    }
    pub(crate) fn open_read(self: &Arc<Self>) -> Result<DocumentSourceLease, DocumentError> {
        self.require_current()?;
        let snapshot = self.epoch.with_fence(|live| {
            if !live {
                return Err(crate::domain::shared_view::ViewError::StreamSealed);
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| crate::domain::shared_view::ViewError::ReadFailure)?;
            if state.retired {
                return Err(crate::domain::shared_view::ViewError::StreamSealed);
            }
            state.readers = state
                .readers
                .checked_add(1)
                .ok_or(crate::domain::shared_view::ViewError::AllocationFailure)?;
            state.offer = false;
            Ok(OriginalSourceResultSnapshot {
                read: Arc::new(SourceResultRead {
                    root: self.clone(),
                    retired: AtomicBool::new(false),
                }),
            })
        })?;
        DocumentSourceLease::source_result(Arc::new(snapshot)).bind_registry(self.scope.clone())
    }
    fn release_read(&self) {
        let retire = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.readers > 0 {
                state.readers -= 1;
            }
            if state.readers == 0 && !state.offer && !state.retired {
                state.retired = true;
                true
            } else {
                false
            }
        };
        if retire {
            self.notify_offer_retired();
            self.payload.retire();
            self.epoch.seal_and_notify();
        }
    }
}
impl<R: RetainedRaw> crate::application::document_source::DocumentStreamSeal
    for OriginalSourceResult<R>
{
    fn seal_from_epoch(&self) {
        self.withdraw_offer();
    }
}
impl<R: RetainedRaw> Drop for OriginalSourceResult<R> {
    fn drop(&mut self) {
        self.payload.retire();
    }
}
struct SourceResultRead<R: RetainedRaw> {
    root: Arc<OriginalSourceResult<R>>,
    retired: AtomicBool,
}
impl<R: RetainedRaw> SourceResultRead<R> {
    fn retire(&self) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            self.root.release_read();
        }
    }
}
impl<R: RetainedRaw> Drop for SourceResultRead<R> {
    fn drop(&mut self) {
        self.retire();
    }
}
#[derive(Clone)]
pub struct OriginalSourceResultSnapshot<R: RetainedRaw> {
    read: Arc<SourceResultRead<R>>,
}
impl<R: RetainedRaw> SourceResultDocumentSource for OriginalSourceResultSnapshot<R> {
    fn verify_scope(&self, scope: &RegistryOwnerScope) -> Result<(), DocumentError> {
        self.read
            .root
            .payload
            .verify_brand(scope, &self.read.root.queue)
    }
    fn total_bytes(&self) -> u64 {
        self.read.root.total_bytes
    }
    fn epoch(&self) -> &Arc<DocumentEpoch> {
        &self.read.root.epoch
    }
    fn require_current(&self) -> Result<(), DocumentError> {
        if self.read.retired.load(Ordering::Acquire) {
            return Err(DocumentError::Closed);
        }
        self.read.root.require_current()
    }
    fn copy_range(&self, offset: usize, length: usize) -> Result<Vec<u8>, DocumentError> {
        self.require_current()?;
        self.read.root.payload.copy_range(offset, length)
    }
    fn retire(&self) {
        self.read.retire();
    }
}

mod reply;
pub(super) use reply::{SourceReplyAckUse, SourceReplyOperation};
pub trait SourceResultPublisher<R: RetainedRaw>: Send + Sync + 'static {
    fn publish(&self, result: Arc<OriginalSourceResult<R>>);
    fn no_result(&self);
}

mod sql_inventory;
#[cfg(all(test, windows, feature = "native-dispatch-dev-fixtures"))]
pub(crate) use sql_inventory::SourceSqlScheduleState;
#[cfg(test)]
pub(crate) use sql_inventory::{BlockingTestBarrier, SourceSqlInvocation};
pub(super) use sql_inventory::{SourceSqlClosurePort, SourceSqlGroup, SourceSqlInventory};
mod operations;
pub use operations::{SourceSessionManager, SourceSessionReceipt};
