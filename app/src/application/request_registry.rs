//! Inward-facing registration ports. Opaque wrappers are minted by the application only.
pub use crate::domain::request_registry::*;
use crate::domain::{
    ActorContext, EntityId,
    native::{NativeSessionKey, RuntimeEpoch},
};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{sync::Notify, time::Instant};

pub struct CurrentRequestActor {
    context: ActorContext,
    target: EntityId,
    native: NativeSessionKey,
    action: Option<QueueAction>,
    control_root: Option<NativeSessionKey>,
    control_epoch: Option<u64>,
    expected_control_epoch: Option<u64>,
}
impl CurrentRequestActor {
    pub(crate) const fn new(
        context: ActorContext,
        target: EntityId,
        native: NativeSessionKey,
        action: Option<QueueAction>,
    ) -> Self {
        Self {
            context,
            target,
            native,
            action,
            control_root: None,
            control_epoch: None,
            expected_control_epoch: None,
        }
    }
    pub(crate) const fn with_control(
        context: ActorContext,
        target: EntityId,
        native: NativeSessionKey,
        action: Option<QueueAction>,
        control_root: Option<NativeSessionKey>,
        control_epoch: Option<u64>,
        expected_control_epoch: Option<u64>,
    ) -> Self {
        Self {
            context,
            target,
            native,
            action,
            control_root,
            control_epoch,
            expected_control_epoch,
        }
    }
    #[must_use]
    pub const fn context(&self) -> ActorContext {
        self.context
    }
    #[must_use]
    pub const fn target(&self) -> EntityId {
        self.target
    }
    #[must_use]
    pub const fn native(&self) -> &NativeSessionKey {
        &self.native
    }
    #[must_use]
    pub const fn action(&self) -> Option<QueueAction> {
        self.action
    }
    #[must_use]
    pub const fn control_root(&self) -> Option<&NativeSessionKey> {
        self.control_root.as_ref()
    }
    #[must_use]
    pub const fn control_epoch(&self) -> Option<u64> {
        self.control_epoch
    }
    #[must_use]
    pub const fn expected_control_epoch(&self) -> Option<u64> {
        self.expected_control_epoch
    }
}

pub struct OperationProbe {
    id: RegistrationId,
    operation: EntityId,
    runtime: RuntimeEpoch,
    intent: RegisteredIntent,
    generation: i64,
    begun_at_ms: i64,
}
impl OperationProbe {
    pub(crate) const fn new(
        id: RegistrationId,
        operation: EntityId,
        at: &OperationContext,
        actor: &CurrentRequestActor,
        intent: RegisteredIntent,
    ) -> Self {
        Self {
            id,
            operation,
            runtime: at.runtime,
            intent,
            generation: actor.context.auth_generation,
            begun_at_ms: at.observed_at_ms,
        }
    }
    #[must_use]
    pub const fn registration_id(&self) -> RegistrationId {
        self.id
    }
    #[must_use]
    pub const fn operation(&self) -> EntityId {
        self.operation
    }
    #[must_use]
    pub const fn runtime(&self) -> RuntimeEpoch {
        self.runtime
    }
    #[must_use]
    pub const fn intent(&self) -> &RegisteredIntent {
        &self.intent
    }
    #[must_use]
    pub const fn admission_auth_generation(&self) -> i64 {
        self.generation
    }
    #[must_use]
    pub const fn begun_at_ms(&self) -> i64 {
        self.begun_at_ms
    }
    pub(crate) fn matches(&self, row: &StoredApplying) -> bool {
        row.registration.id() == self.id
            && row.registration.intent() == &self.intent
            && row.operation == self.operation
            && row.runtime == self.runtime
            && row.admission_auth_generation == self.generation
            && row.begun_at_ms == self.begun_at_ms
    }
}
impl fmt::Debug for OperationProbe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationProbe")
            .field("request", &self.id)
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}
pub struct ConsumeLease {
    record: StoredApplying,
}
impl ConsumeLease {
    pub(crate) fn mint(
        probe: &OperationProbe,
        record: StoredApplying,
    ) -> Result<Self, RegistryError> {
        if !probe.matches(&record) {
            return Err(RegistryError::CorruptState);
        }
        Ok(Self { record })
    }
    #[must_use]
    pub const fn registration_id(&self) -> RegistrationId {
        self.record.registration.id()
    }
    #[must_use]
    pub const fn operation(&self) -> EntityId {
        self.record.operation
    }
    #[must_use]
    pub const fn runtime(&self) -> RuntimeEpoch {
        self.record.runtime
    }
    #[must_use]
    pub const fn intent(&self) -> &RegisteredIntent {
        self.record.registration.intent()
    }
    #[must_use]
    pub const fn stored(&self) -> &StoredApplying {
        &self.record
    }
}
impl fmt::Debug for ConsumeLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsumeLease")
            .field("request", &self.registration_id())
            .field("operation", &self.operation())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct OperationContext {
    pub runtime: RuntimeEpoch,
    pub observed_at_ms: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BeginRecord {
    Started(StoredApplying),
    Existing(RequestFact),
}

struct DeadlineState {
    cancelled: AtomicBool,
    notify: Notify,
}
#[derive(Clone)]
pub struct RegistryDeadline {
    at: Instant,
    state: Arc<DeadlineState>,
}
impl RegistryDeadline {
    #[must_use]
    pub fn new(deadline: Instant) -> Self {
        Self {
            at: deadline,
            state: Arc::new(DeadlineState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::SeqCst);
        self.state.notify.notify_waiters();
    }
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.at
    }
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }
    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.state.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
    pub(crate) async fn run<T>(
        &self,
        future: impl Future<Output = Result<T, RegistryError>>,
    ) -> Result<T, RegistryError> {
        tokio::select! {
            biased;
            () = self.cancelled() => Err(RegistryError::Cancelled),
            result = tokio::time::timeout_at(self.at, future) => result.map_err(|_| RegistryError::Cancelled)?,
        }
    }
}

pub trait NodeClock: Send + Sync {
    fn unix_millis(&self) -> i64;
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemNodeClock;
impl NodeClock for SystemNodeClock {
    fn unix_millis(&self) -> i64 {
        // Signed event metadata only. It never determines input eligibility.
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(e) => i64::try_from(e.duration().as_millis()).map_or(i64::MIN, |n| -n),
        }
    }
}
pub(crate) trait RegistryOwnerPort: Send + Sync {
    fn is_sealed(&self) -> bool;
    fn wait_sealed(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Identity and read-only lifetime of the original physical registry connection owner.
#[derive(Clone)]
pub struct RegistryOwnerScope(Arc<dyn RegistryOwnerPort>);
impl RegistryOwnerScope {
    pub(crate) fn from_owner(owner: Arc<dyn RegistryOwnerPort>) -> Self {
        Self(owner)
    }
    pub(crate) fn require_open(&self) -> Result<(), RegistryError> {
        if self.0.is_sealed() {
            Err(RegistryError::NodeStopping)
        } else {
            Ok(())
        }
    }
    pub(crate) fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) async fn while_open<T>(
        &self,
        future: impl Future<Output = Result<T, RegistryError>>,
    ) -> Result<T, RegistryError> {
        self.require_open()?;
        tokio::select! {
            biased;
            () = self.0.wait_sealed() => Err(RegistryError::NodeStopping),
            result = future => {
                self.require_open()?;
                result
            }
        }
    }
}

pub trait RequestAuthority: Send + Sync {
    type Actor: Send + Sync;
    /// Trusted ports may additionally bind to the repository's actual owner.
    /// # Errors
    /// A production authority rejects a foreign or stopped registry owner.
    fn verify_owner_scope(&self, _owner: &RegistryOwnerScope) -> Result<(), RegistryError> {
        Ok(())
    }
    fn authorize(
        &self,
        actor: &Self::Actor,
        target: EntityId,
        action: QueueAction,
    ) -> impl Future<Output = Result<ActorContext, RegistryError>> + Send;
    fn authorize_with_epoch(
        &self,
        actor: &Self::Actor,
        target: EntityId,
        action: QueueAction,
        _expected_control_epoch: Option<u64>,
    ) -> impl Future<
        Output = Result<(ActorContext, Option<NativeSessionKey>, Option<u64>), RegistryError>,
    > + Send {
        async move {
            let context = self.authorize(actor, target, action).await?;
            Ok((context, None, None))
        }
    }
    fn authorize_input_consume(
        &self,
        actor: &Self::Actor,
        target: EntityId,
        action: QueueAction,
        _registration: RegistrationId,
        expected_control_epoch: Option<u64>,
    ) -> impl Future<
        Output = Result<(ActorContext, Option<NativeSessionKey>, Option<u64>), RegistryError>,
    > + Send {
        self.authorize_with_epoch(actor, target, action, expected_control_epoch)
    }
    fn record_input_registration(
        &self,
        _actor: &CurrentRequestActor,
        _registration: RegistrationId,
    ) {
    }
    /// # Errors
    /// Returns a registry error when session control changed before commit.
    fn reverify_consume(
        &self,
        _actor: &CurrentRequestActor,
        _registration: RegistrationId,
    ) -> Result<(), RegistryError> {
        Ok(())
    }
    fn authorize_read(
        &self,
        actor: &Self::Actor,
        target: EntityId,
    ) -> impl Future<Output = Result<ActorContext, RegistryError>> + Send;
    fn resolve_target(
        &self,
        target: EntityId,
    ) -> impl Future<Output = Result<NativeSessionKey, RegistryError>> + Send;
}
pub trait RequestRegistryRepository: Clone + Send + Sync {
    fn register(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<RegistrationFact, RegistryError>> + Send;
    /// Registers a node-generated ID and reports any existing row as a collision.
    ///
    /// The default preserves repositories that cannot distinguish insertion from an
    /// idempotent retry. A durable owner overrides this so generated IDs cannot alias
    /// an earlier request with the same intent.
    fn register_generated(
        &self,
        actor: &CurrentRequestActor,
        intent: &RegisteredIntent,
        id: RegistrationId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<RegistrationFact, RegistryError>> + Send {
        self.register(actor, intent, id, at, deadline)
    }
    fn begin_consume(
        &self,
        actor: &CurrentRequestActor,
        id: RegistrationId,
        intent: &RegisteredIntent,
        operation: EntityId,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<BeginRecord, RegistryError>> + Send;
    fn settle(
        &self,
        lease: &ConsumeLease,
        outcome: &QueueOutcome,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<RequestFact, RegistryError>> + Send;
    fn read(
        &self,
        reader: &CurrentRequestActor,
        target: EntityId,
        id: RegistrationId,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<RequestFact, RegistryError>> + Send;
    fn inspect_operation(
        &self,
        probe: &OperationProbe,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<RequestFact, RegistryError>> + Send;
    fn interrupt_previous_runtime(
        &self,
        current: RuntimeEpoch,
        at: &OperationContext,
        deadline: &RegistryDeadline,
    ) -> impl Future<Output = Result<u64, RegistryError>> + Send;
}

pub trait ValidatedBodyPort: Send + Sync {
    type Raw: ?Sized;
    type Body: Send + 'static;
    /// Validate and freeze the complete structure; return a binding for precisely that value.
    /// # Errors
    /// Rejects invalid input without logging its body.
    fn validate(&self, raw: &Self::Raw) -> Result<(Self::Body, [u8; 32]), RegistryError>;
}
pub struct ValidatedBody<T> {
    body: T,
    digest: [u8; 32],
}
impl<T> ValidatedBody<T> {
    pub(crate) const fn new(body: T, digest: [u8; 32]) -> Self {
        Self { body, digest }
    }
    #[must_use]
    pub const fn body(&self) -> &T {
        &self.body
    }
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}
impl<T> fmt::Debug for ValidatedBody<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ValidatedBody([redacted])")
    }
}
pub enum ValidatedMutation<T> {
    Submit(ValidatedBody<T>),
    Edit {
        item: EntityId,
        revision: u64,
        body: ValidatedBody<T>,
    },
    Cancel {
        item: EntityId,
        revision: u64,
    },
    Prioritize {
        item: EntityId,
        revision: u64,
    },
    Resume,
}
impl<T> ValidatedMutation<T> {
    #[must_use]
    pub const fn action(&self) -> QueueAction {
        match self {
            Self::Submit(_) => QueueAction::Submit,
            Self::Edit { .. } => QueueAction::Edit,
            Self::Cancel { .. } => QueueAction::Cancel,
            Self::Prioritize { .. } => QueueAction::Prioritize,
            Self::Resume => QueueAction::Resume,
        }
    }
    pub(crate) const fn intent(
        &self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<RegisteredIntent, RegistryError> {
        let (item, revision, digest) = match self {
            Self::Submit(body) => (None, None, Some(*body.digest())),
            Self::Edit {
                item,
                revision,
                body,
            } => (Some(*item), Some(*revision), Some(*body.digest())),
            Self::Cancel { item, revision } | Self::Prioritize { item, revision } => {
                (Some(*item), Some(*revision), None)
            }
            Self::Resume => (None, None, None),
        };
        RegisteredIntent::new(actor, self.action(), target, item, revision, digest)
    }
}
impl<T> fmt::Debug for ValidatedMutation<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ValidatedMutation")
            .field(&self.action())
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct AuditMetadata {
    pub request: RegistrationId,
    pub actor: EntityId,
    pub action: QueueAction,
    pub target: EntityId,
    pub outcome: QueueOutcome,
    pub observed_at_ms: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditFailure {
    Unavailable,
    Full,
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct AuditWarning {
    pub request: RegistrationId,
    pub failure: AuditFailure,
}
pub trait AuditPort: Send + Sync {
    fn append<'a>(
        &'a self,
        metadata: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>>;
}
#[derive(Default)]
pub struct NoAudit;
impl AuditPort for NoAudit {
    fn append<'a>(
        &'a self,
        _: &'a AuditMetadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), AuditFailure>> + Send + 'a>> {
        Box::pin(async { Err(AuditFailure::Unavailable) })
    }
}

pub(crate) fn random_id() -> Result<EntityId, RegistryError> {
    crate::random::bytes()
        .map(EntityId::from_bytes)
        .map_err(|_| RegistryError::RandomUnavailable)
}

#[cfg(test)]
pub(crate) mod random_hooks {
    use super::{EntityId, RegistryError};
    use std::sync::Mutex;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Point {
        Runtime,
        Registration,
    }
    pub enum Action {
        Unavailable,
        Value(EntityId),
    }
    #[derive(Default)]
    pub struct Controller {
        next: Mutex<Option<(Point, Action)>>,
    }
    impl Controller {
        pub fn arm(&self, point: Point, action: Action) -> Result<(), RegistryError> {
            let mut next = self
                .next
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            if next.is_some() {
                return Err(RegistryError::Conflict);
            }
            *next = Some((point, action));
            drop(next);
            Ok(())
        }
        pub(crate) fn take(
            &self,
            point: Point,
        ) -> Result<Option<Result<EntityId, RegistryError>>, RegistryError> {
            let mut next = self
                .next
                .lock()
                .map_err(|_| RegistryError::StorageUnavailable)?;
            let action = if next.as_ref().is_some_and(|(p, _)| *p == point) {
                next.take().map(|(_, a)| a)
            } else {
                None
            };
            Ok(action.map(|a| match a {
                Action::Unavailable => Err(RegistryError::RandomUnavailable),
                Action::Value(v) => Ok(v),
            }))
        }
    }
}
