//! Opaque proofs and body-free stored records for the unique native dispatch slot.
use super::{
    Arc, AtomicBool, BoxFuture, DispatchError, DispatchPlan, EntityId, KnownStopReason, Ordering,
    PreResourceProbe, PreSourceProbe, ProtocolVersion, RpcScalar, StoredTransport, TransportFact,
};
use crate::application::request_registry::{OperationContext, RegistryDeadline};

pub struct DispatchProbe {
    pub(super) plan: DispatchPlan,
    pub(super) resource: Arc<PreResourceProbe>,
    pub(super) nonce: Arc<()>,
}
impl DispatchProbe {
    #[must_use]
    pub const fn plan(&self) -> &DispatchPlan {
        &self.plan
    }
}
impl DispatchProbe {
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.nonce, &other.nonce) && self.plan == other.plan
    }
}
impl DispatchProbe {
    #[must_use]
    pub fn resource(&self) -> &PreResourceProbe {
        &self.resource
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionStamp {
    pub selection_id: EntityId,
    pub owner_order: u64,
    pub at: i64,
}
pub struct SelectedProof {
    pub(super) dispatch: Arc<DispatchProbe>,
    pub(super) stamp: SelectionStamp,
}
impl SelectedProof {
    #[must_use]
    pub fn dispatch(&self) -> &DispatchProbe {
        &self.dispatch
    }
    #[must_use]
    pub const fn stamp(&self) -> SelectionStamp {
        self.stamp
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptMetadata {
    pub attempt_id: EntityId,
    pub dispatch_id: EntityId,
    pub selection: SelectionStamp,
    pub item_id: EntityId,
    pub revision: u64,
    pub connection_epoch: EntityId,
    pub wire: ProtocolVersion,
    pub rpc: RpcScalar,
    pub frame_length: u64,
    pub armed_at: i64,
}
pub struct AttemptProbe {
    pub(super) dispatch: Arc<DispatchProbe>,
    pub(super) metadata: AttemptMetadata,
}
impl AttemptProbe {
    #[must_use]
    pub fn dispatch(&self) -> &DispatchProbe {
        &self.dispatch
    }
    #[must_use]
    pub const fn metadata(&self) -> &AttemptMetadata {
        &self.metadata
    }
}
pub struct TerminalTransportProof {
    pub(super) attempt: Arc<AttemptProbe>,
    pub(super) fact: TransportFact,
    pub(super) recorded_at: i64,
}
impl TerminalTransportProof {
    #[must_use]
    pub fn attempt(&self) -> &AttemptProbe {
        &self.attempt
    }
    #[must_use]
    pub const fn fact(&self) -> TransportFact {
        self.fact
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.recorded_at
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchLifecycle {
    PreparedZero,
    Selected,
    Closed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchCloseRecord {
    pub reason: Arc<str>,
    pub fingerprint: [u8; 32],
    pub at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredDispatch {
    pub plan: DispatchPlan,
    pub lifecycle: DispatchLifecycle,
    pub selection: Option<SelectionStamp>,
    pub close: Option<DispatchCloseRecord>,
    pub has_slot: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAttempt {
    pub metadata: AttemptMetadata,
    pub transport: StoredTransport,
    pub recorded_at: Option<i64>,
}
pub struct ReleaseProof {
    pub(super) dispatch: Arc<DispatchProbe>,
    pub(super) reason: ReleaseKind,
    pub(super) fingerprint: [u8; 32],
    pub(super) at: i64,
    pub(super) requirements: Vec<EntityId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseKind {
    KnownZero,
    RestartZero,
    NativeSettled,
    ResourcesConverged,
}
impl ReleaseKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::KnownZero => "known_zero",
            Self::RestartZero => "restart_zero",
            Self::NativeSettled => "native_settled",
            Self::ResourcesConverged => "resources_converged",
        }
    }
}
impl ReleaseProof {
    #[must_use]
    pub fn dispatch(&self) -> &DispatchProbe {
        &self.dispatch
    }
    #[must_use]
    pub const fn reason(&self) -> ReleaseKind {
        self.reason
    }
    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    #[must_use]
    pub const fn at(&self) -> i64 {
        self.at
    }
}
impl ReleaseProof {
    #[must_use]
    pub fn requirements(&self) -> &[EntityId] {
        &self.requirements
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchInspection {
    pub dispatch: StoredDispatch,
    pub attempts: Vec<StoredAttempt>,
}
pub struct StartupProof {
    pub(super) runtime: crate::domain::native::RuntimeEpoch,
    pub(super) exclusive: Arc<AtomicBool>,
}
impl StartupProof {
    pub(crate) fn owns_runtime(&self, runtime: crate::domain::native::RuntimeEpoch) -> bool {
        self.runtime == runtime && self.exclusive.load(Ordering::SeqCst)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoverySummary {
    pub zero_closed: u64,
    pub unknown_marked: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationMetadata {
    pub observation_id: EntityId,
    pub source_id: EntityId,
    pub runtime: crate::domain::native::RuntimeEpoch,
    pub instance_id: EntityId,
    pub connection_epoch: EntityId,
    pub sequence: u64,
    pub receipt: u64,
    pub observed_at: i64,
    pub fingerprint: [u8; 32],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactKind {
    V1PromptResult,
    V2Inserted,
    PromptRejected,
    ForegroundRunning,
    ForegroundRequiresAction,
    ForegroundIdle,
    ForegroundUnknown,
    ForegroundTerminal,
    ApprovalPresent,
    ApprovalRetired,
}
impl FactKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::V1PromptResult => "v1_prompt_result",
            Self::V2Inserted => "v2_inserted",
            Self::PromptRejected => "prompt_rejected",
            Self::ForegroundRunning => "foreground_running",
            Self::ForegroundRequiresAction => "foreground_requires_action",
            Self::ForegroundIdle => "foreground_idle",
            Self::ForegroundUnknown => "foreground_unknown",
            Self::ForegroundTerminal => "foreground_terminal",
            Self::ApprovalPresent => "approval_present",
            Self::ApprovalRetired => "approval_retired",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultKind {
    Completed,
    Stopped,
    Failed,
    Rejected,
    Unknown,
}
impl ResultKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionKind {
    New,
    Snapshot,
    Repeated,
    Unknown,
}
impl TransitionKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Snapshot => "snapshot",
            Self::Repeated => "repeated",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct NativeFactMetadata {
    pub requirement: EntityId,
    pub target: EntityId,
    pub owner: EntityId,
    pub load_epoch: EntityId,
    pub owner_order: u64,
    pub attempt: Option<EntityId>,
    pub kind: FactKind,
    pub message: Option<Arc<str>>,
    pub result: Option<ResultKind>,
    pub reason: Option<KnownStopReason>,
    pub transition: TransitionKind,
    pub interval: Option<u64>,
    pub request: Option<RpcScalar>,
}
pub struct ObservationProbe {
    pub(super) header: ObservationMetadata,
    pub(super) source: Arc<PreSourceProbe>,
}
impl ObservationProbe {
    #[must_use]
    pub const fn header(&self) -> &ObservationMetadata {
        &self.header
    }
}
impl ObservationProbe {
    #[must_use]
    pub fn source(&self) -> &PreSourceProbe {
        &self.source
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct MetadataBatch {
    pub header: ObservationMetadata,
    pub facts: Vec<NativeFactMetadata>,
}
#[derive(Clone, Eq, PartialEq)]
pub struct StoredFacts {
    pub batch: MetadataBatch,
}
pub trait NativeDispatchRepository: Send + Sync + 'static {
    fn reserve<'a>(
        &'a self,
        p: &'a DispatchProbe,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredDispatch, DispatchError>>;
    fn arm<'a>(
        &'a self,
        p: &'a AttemptProbe,
        s: &'a SelectedProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredAttempt, DispatchError>>;
    fn record_transport<'a>(
        &'a self,
        p: &'a AttemptProbe,
        f: &'a TerminalTransportProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredAttempt, DispatchError>>;
    fn record_observation<'a>(
        &'a self,
        p: &'a ObservationProbe,
        b: &'a MetadataBatch,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredFacts, DispatchError>>;
    fn close<'a>(
        &'a self,
        p: &'a DispatchProbe,
        f: &'a ReleaseProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<StoredDispatch, DispatchError>>;
    fn inspect<'a>(
        &'a self,
        p: &'a DispatchProbe,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<DispatchInspection>, DispatchError>>;
    fn interrupt_previous_runtime<'a>(
        &'a self,
        p: &'a StartupProof,
        c: &'a OperationContext,
        d: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<RecoverySummary, DispatchError>>;
}

/// Fixed-order, body-free observation metadata digest, shared with the actual repository.
/// # Errors
/// Rejects unrepresentable byte lengths, not large protocol content.
pub fn observation_fingerprint(
    header: &ObservationMetadata,
    facts: &[NativeFactMetadata],
) -> Result<[u8; 32], DispatchError> {
    use sha2::{Digest, Sha256};
    fn text(h: &mut Sha256, value: &str) -> Result<(), DispatchError> {
        h.update(
            u64::try_from(value.len())
                .map_err(|_| DispatchError::LengthUnrepresentable)?
                .to_be_bytes(),
        );
        h.update(value.as_bytes());
        Ok(())
    }
    fn opt_text(h: &mut Sha256, value: Option<&str>) -> Result<(), DispatchError> {
        h.update([u8::from(value.is_some())]);
        if let Some(v) = value {
            text(h, v)?;
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    hash.update(b"native-observation-v1");
    for id in [
        header.observation_id,
        header.source_id,
        header.runtime.0,
        header.instance_id,
        header.connection_epoch,
    ] {
        text(&mut hash, &id.encode())?;
    }
    hash.update(header.sequence.to_be_bytes());
    hash.update(header.receipt.to_be_bytes());
    hash.update(header.observed_at.to_be_bytes());
    hash.update(
        u64::try_from(facts.len())
            .map_err(|_| DispatchError::LengthUnrepresentable)?
            .to_be_bytes(),
    );
    for (fact_index, f) in facts.iter().enumerate() {
        hash.update(
            u64::try_from(fact_index)
                .map_err(|_| DispatchError::LengthUnrepresentable)?
                .to_be_bytes(),
        );
        for id in [f.requirement, f.target, f.owner, f.load_epoch] {
            text(&mut hash, &id.encode())?;
        }
        hash.update(f.owner_order.to_be_bytes());
        opt_text(&mut hash, f.attempt.map(EntityId::encode).as_deref())?;
        text(&mut hash, f.kind.label())?;
        opt_text(&mut hash, f.message.as_deref())?;
        opt_text(&mut hash, f.result.map(ResultKind::label))?;
        opt_text(&mut hash, f.reason.map(KnownStopReason::label))?;
        text(&mut hash, f.transition.label())?;
        hash.update([u8::from(f.interval.is_some())]);
        if let Some(v) = f.interval {
            hash.update(v.to_be_bytes());
        }
        match &f.request {
            None => hash.update([0]),
            Some(RpcScalar::Integer(v)) => {
                hash.update([1]);
                hash.update(v.to_be_bytes());
            }
            Some(RpcScalar::String(v)) => {
                hash.update([2]);
                text(&mut hash, v)?;
            }
        }
    }
    Ok(hash.finalize().into())
}
