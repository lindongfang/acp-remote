//! Necessary capacity ownership; a closed record still occupies capacity until reconciliation.
use super::{Arc, BoxFuture, DispatchError, EntityId, NativeSessionKey, PreSourceProbe};
use crate::{application::request_registry::RegistryDeadline, domain::native::RuntimeEpoch};

pub struct CapacityReservation {
    pub(crate) id: EntityId,
    pub(crate) source: Arc<PreSourceProbe>,
    pub(crate) native: Option<NativeSessionKey>,
    pub(crate) create_operation: Option<EntityId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapacityRecord {
    pub(crate) id: EntityId,
    pub(crate) native: Option<NativeSessionKey>,
    pub(crate) source: EntityId,
    pub(crate) instance: EntityId,
    pub(crate) runtime: RuntimeEpoch,
    pub(crate) physically_closed: bool,
}

impl CapacityRecord {
    pub(crate) fn same_reservation(&self, other: &Self) -> bool {
        self.id == other.id
            && self.native == other.native
            && self.source == other.source
            && self.instance == other.instance
            && self.runtime == other.runtime
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapacityReleaseBasis {
    Owned(CapacityRecord),
    /// The nonempty original set was reconciled by its original sources.
    SourceClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleaseReason {
    IdleTimeout,
    CapacityPressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleaseOutcome {
    Requested,
    Failed,
    Unknown,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomaticReleasePhase {
    Pending,
    Finished,
    Withdrawn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomaticReleaseRecord {
    pub(crate) operation: EntityId,
    pub(crate) original: CapacityRecord,
    pub(crate) reason: AutomaticReleaseReason,
    pub(crate) phase: AutomaticReleasePhase,
    pub(crate) outcome: Option<AutomaticReleaseOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacitySnapshot {
    pub limit: u32,
    /// All unreconciled slots, including both subsets below.
    pub occupied: u32,
    pub reserved: u32,
    pub closed_pending: u32,
}

pub trait CapacityRepository: super::SessionTreeRepository {
    /// Internal single-native mechanism; production tree capture requires its own gate.
    fn begin_automatic_release<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        reason: AutomaticReleaseReason,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;
    /// The caller owns the original ticket's Written/Zero evidence.
    fn record_automatic_release_outcome<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        outcome: AutomaticReleaseOutcome,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;
    fn finish_automatic_release<'a>(
        &'a self,
        record: &'a CapacityRecord,
        operation: EntityId,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<AutomaticReleaseRecord, DispatchError>>;
    fn reserve_capacity<'a>(
        &'a self,
        reservation: &'a CapacityReservation,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityRecord, DispatchError>>;
    fn capacity_owner<'a>(
        &'a self,
        native: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<CapacityRecord>, DispatchError>>;
    fn capacity_release_basis<'a>(
        &'a self,
        operation: EntityId,
        native: &'a NativeSessionKey,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityReleaseBasis, DispatchError>>;
    fn inspect_capacity<'a>(
        &'a self,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacitySnapshot, DispatchError>>;
    fn bind_created_capacity<'a>(
        &'a self,
        id: EntityId,
        native: &'a NativeSessionKey,
        source: &'a PreSourceProbe,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<CapacityRecord, DispatchError>>;
    /// Only records original runtime closure. This never returns a slot.
    fn confirm_capacity_closed<'a>(
        &'a self,
        record: &'a CapacityRecord,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>>;
    fn abandon_capacity<'a>(
        &'a self,
        id: EntityId,
        source: &'a PreSourceProbe,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>>;
    fn trusted_client_display_name<'a>(
        &'a self,
        client: EntityId,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<Option<String>, DispatchError>>;
    fn record_control_security_audit<'a>(
        &'a self,
        actor: Option<EntityId>,
        action: &'static str,
        root: &'a NativeSessionKey,
        outcome_json: String,
        occurred_at: i64,
        deadline: &'a RegistryDeadline,
    ) -> BoxFuture<'a, Result<(), DispatchError>>;
}

#[cfg(test)]
#[must_use]
pub fn capacity_source_development_fixture(
    metadata: crate::domain::native_dispatch::SourceMetadata,
) -> Arc<PreSourceProbe> {
    Arc::new(PreSourceProbe {
        metadata,
        authority: Arc::new(super::SourceAuthority {
            closed: super::AtomicBool::new(false),
            closing: super::AtomicBool::new(false),
            quarantined: super::AtomicBool::new(false),
            required: std::sync::atomic::AtomicUsize::new(0),
            gate: std::sync::Mutex::new(()),
            #[cfg(windows)]
            result_epochs: std::sync::Mutex::new(Vec::new()),
        }),
    })
}
