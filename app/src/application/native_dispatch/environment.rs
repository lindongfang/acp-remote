//! Current native coordination facts. Only real installation and observation paths mutate them.
#![allow(
    clippy::significant_drop_tightening,
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::{
    Arc, AtomicBool, EntityId, KnownStopReason, NativeSessionKey, Ordering, PreResourceProbe,
    RpcScalar, SessionBinding, TransportFact,
};
use crate::{
    application::queue_owner::{QueueEnvironmentPort, QueueOwnerEvent},
    domain::{activity::WorkActivity, input_queue::DispatchReadiness, native::ResourceEpoch},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Mutex,
};
use tokio::sync::{Notify, mpsc};

pub(super) struct TargetFacts<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) pre: Arc<PreResourceProbe>,
    pub(super) binding: Option<Arc<SessionBinding<P::Scope>>>,
    pub(super) foreground: WorkActivity,
    pub(super) interval: u64,
    pub(super) interval_open: bool,
    pub(super) interval_live: bool,
    pub(super) terminal_sequence: u64,
    pub(super) live_observed: bool,
    pub(super) foreground_evidence: ForegroundEvidence,
    pub(super) activity: crate::domain::activity::SessionActivity,
    pub(super) last_idle_sequence: u64,
    pub(super) writer_parked: Option<std::task::Waker>,
    pub(super) terminal: Option<KnownStopReason>,
    pub(super) pending_input: Option<PendingInput>,
    pub(super) retired_prompts: HashMap<RpcScalar, RetiredPrompt>,
    pub(super) requirements: HashSet<EntityId>,
    pub(super) failed_requirements: HashSet<EntityId>,
    pub(super) control_holds: HashSet<EntityId>,
    pub(super) permissions: HashMap<RpcScalar, Arc<SourcePermissionSlot<P>>>,
    pub(super) events: VecDeque<QueueOwnerEvent>,
    pub(super) raw: Vec<P::RawEvent>,
}
pub(super) struct PendingInput {
    pub(super) dispatch: EntityId,
    pub(super) attempt: EntityId,
    pub(super) rpc: RpcScalar,
    pub(super) inserted: bool,
    pub(super) transport: Option<TransportFact>,
    pub(super) native_reason: Option<KnownStopReason>,
    pub(super) work: crate::domain::activity::WorkToken,
    pub(super) admitted_sequence: u64,
    pub(super) requirements: Vec<EntityId>,
}
pub(super) struct RetiredPrompt {
    pub(super) attempt: EntityId,
}
pub(super) enum ForegroundEvidence {
    Unknown,
    Active,
    IdleSnapshot,
    OwnedV1Idle,
    ReliableTerminal,
    UnreportedStop,
}
pub(super) struct SourcePermissionSlot<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) approval: std::sync::OnceLock<Arc<super::approval_inventory::ApprovalSlot<P>>>,
    pub(super) response_owner: Mutex<Option<PermissionResponseOwner<P>>>,
    pub(super) request: P::PermissionRequest,
    pub(super) scope: P::Scope,
    pub(super) session: Arc<str>,
    pub(super) rpc: RpcScalar,
    pub(super) sequence: u64,
    pub(super) receipt: u64,
    pub(super) association: Option<Arc<PreResourceProbe>>,
    pub(super) action: Mutex<()>,
    pub(super) state: Mutex<PermissionState>,
}
pub(super) enum PermissionResponseOwner<P: super::NativeApprovalPort> {
    Selected(Arc<super::approval_response::ApprovalResponseOperation<P>>),
    ManagementFailure(Arc<super::approval_failure::ApprovalFailureResponse<P>>),
    Cancelled {
        id: EntityId,
        original: Arc<super::control::ControlOperation<P>>,
    },
}
#[derive(Clone, Copy)]
pub(super) struct PermissionState {
    pub(super) obligation: Option<EntityId>,
    pub(super) claimed: Option<EntityId>,
    pub(super) written: bool,
    pub(super) retired: bool,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> SourcePermissionSlot<P> {
    pub(super) fn snapshot(&self) -> Result<PermissionState, super::DispatchError> {
        self.state
            .lock()
            .map(|state| *state)
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)
    }
}
pub(super) struct NativeTarget<P: crate::application::native_dispatch::NativeApprovalPort> {
    source_authority: Arc<super::SourceAuthority>,
    source_instance: EntityId,
    pub(super) facts: Mutex<TargetFacts<P>>,
    pub(super) changed: Notify,
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> NativeTarget<P> {
    pub(super) fn wake_writer(&self) {
        let waker = match self.facts.lock() {
            Ok(mut facts) => facts.writer_parked.take(),
            Err(error) => error.into_inner().writer_parked.take(),
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
    pub(super) fn new(pre: Arc<PreResourceProbe>) -> Self {
        let epoch = ResourceEpoch {
            runtime: pre.metadata.source.runtime,
            connection: crate::domain::native::HarnessConnectionEpoch(
                pre.metadata.source.connection_epoch,
            ),
            load: crate::domain::native::LoadEpoch(pre.metadata.load_epoch),
        };
        let activity = crate::domain::activity::SessionActivity::new(pre.native.clone(), epoch);
        Self {
            source_authority: pre.authority.source.clone(),
            source_instance: pre.metadata.source.instance_id,
            facts: Mutex::new(TargetFacts {
                pre,
                binding: None,
                foreground: WorkActivity::Unknown,
                interval: 0,
                interval_open: false,
                interval_live: false,
                terminal_sequence: 0,
                live_observed: false,
                foreground_evidence: ForegroundEvidence::Unknown,
                activity,
                last_idle_sequence: 0,
                writer_parked: None,
                terminal: None,
                pending_input: None,
                retired_prompts: HashMap::new(),
                requirements: HashSet::new(),
                failed_requirements: HashSet::new(),
                control_holds: HashSet::new(),
                permissions: HashMap::new(),
                events: VecDeque::new(),
                raw: Vec::new(),
            }),
            changed: Notify::new(),
        }
    }
    pub(super) fn retire_closed_target(&self) {
        let mut facts = match self.facts.lock() {
            Ok(facts) => facts,
            Err(poisoned) => poisoned.into_inner(),
        };
        // The resource authority is the lifetime boundary for attempts whose v2 insertion
        // response has not arrived. Once the resource is physically closed and its reader has
        // converged, no later frame can belong to that target; retaining the association would
        // only accumulate unbounded dead metadata for the lifetime of the source.
        facts.retired_prompts.clear();
    }
}
enum NativeClaimOwner {
    Live(Arc<super::PreSourceProbe>),
    Stored(EntityId),
}
pub(super) struct NativeOwnershipGate {
    claims: Mutex<HashMap<NativeSessionKey, Vec<NativeClaimOwner>>>,
}
impl NativeOwnershipGate {
    fn new() -> Self {
        Self {
            claims: Mutex::new(HashMap::new()),
        }
    }
    fn register_live(
        &self,
        key: &NativeSessionKey,
        source: &Arc<super::PreSourceProbe>,
        created: bool,
    ) -> Result<(), super::DispatchError> {
        if key.source().0 != source.metadata().source_id {
            return Err(super::DispatchError::InvalidSource);
        }
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        let owners = claims.entry(key.clone()).or_default();
        if !created
            && owners
                .iter()
                .any(|owner| !matches!(owner, NativeClaimOwner::Live(live) if live.same(source)))
        {
            return Err(super::DispatchError::RequiredStateUnavailable);
        }
        if !owners
            .iter()
            .any(|owner| matches!(owner, NativeClaimOwner::Live(live) if live.same(source)))
        {
            owners
                .try_reserve(1)
                .map_err(|_| super::DispatchError::StorageUnavailable)?;
            owners.push(NativeClaimOwner::Live(source.clone()));
        }
        if owners
            .iter()
            .any(|owner| !matches!(owner, NativeClaimOwner::Live(live) if live.same(source)))
        {
            return Err(super::DispatchError::RequiredStateUnavailable);
        }
        Ok(())
    }
    fn guard_for<'a>(
        &'a self,
        key: &NativeSessionKey,
        instance: EntityId,
        authority: &Arc<super::SourceAuthority>,
    ) -> Result<
        std::sync::MutexGuard<'a, HashMap<NativeSessionKey, Vec<NativeClaimOwner>>>,
        super::DispatchError,
    > {
        let claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        if claims.get(key).is_some_and(|owners| {
            owners.is_empty()
                || owners.iter().any(|owner| match owner {
                    NativeClaimOwner::Live(source) => {
                        source.metadata().instance_id != instance
                            || !Arc::ptr_eq(&source.authority, authority)
                    }
                    NativeClaimOwner::Stored(_) => true,
                })
        }) {
            return Err(super::DispatchError::RequiredStateUnavailable);
        }
        Ok(claims)
    }
    pub(super) fn owns_live(
        &self,
        key: &NativeSessionKey,
        source: &Arc<super::PreSourceProbe>,
    ) -> Result<bool, super::DispatchError> {
        let claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        Ok(claims.get(key).is_some_and(|owners| {
            !owners.is_empty()
                && owners
                    .iter()
                    .all(|owner| matches!(owner, NativeClaimOwner::Live(live) if live.same(source)))
        }))
    }
    pub(super) fn restore(
        &self,
        barriers: Vec<super::source_sessions::StoredNativeClaimBarrier>,
    ) -> Result<(), super::DispatchError> {
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        claims
            .try_reserve(barriers.len())
            .map_err(|_| super::DispatchError::StorageUnavailable)?;
        for barrier in barriers {
            let owners = claims.entry(barrier.native).or_default();
            if !owners.iter().any(
                |owner| matches!(owner, NativeClaimOwner::Stored(id) if *id == barrier.instance),
            ) {
                owners
                    .try_reserve(1)
                    .map_err(|_| super::DispatchError::StorageUnavailable)?;
                owners.push(NativeClaimOwner::Stored(barrier.instance));
            }
        }
        Ok(())
    }
    fn release_confirmed(
        &self,
        proof: &super::SourceClosureProof,
    ) -> Result<Vec<NativeSessionKey>, super::DispatchError> {
        let mut changed = Vec::new();
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        for (key, owners) in claims.iter_mut() {
            let before = owners.len();
            owners.retain(|owner| match owner {
                NativeClaimOwner::Live(source) => !source.same(&proof.source),
                NativeClaimOwner::Stored(id) => *id != proof.source.metadata().instance_id,
            });
            if owners.len() != before {
                changed.push(key.clone());
            }
        }
        claims.retain(|_, owners| !owners.is_empty());
        Ok(changed)
    }
    #[cfg(windows)]
    pub(super) fn claims_for_source(
        &self,
        instance: EntityId,
    ) -> Result<usize, super::DispatchError> {
        let claims = self
            .claims
            .lock()
            .map_err(|_| super::DispatchError::RequiredStateUnavailable)?;
        Ok(claims
            .values()
            .flatten()
            .filter(|owner| match owner {
                NativeClaimOwner::Live(source) => source.metadata().instance_id == instance,
                NativeClaimOwner::Stored(id) => *id == instance,
            })
            .count())
    }
}
pub struct NativeEnvironment<P: crate::application::native_dispatch::NativeApprovalPort> {
    pub(super) targets: Mutex<HashMap<NativeSessionKey, Arc<NativeTarget<P>>>>,
    pub(super) ownership: Arc<NativeOwnershipGate>,
    pub(super) changes: mpsc::UnboundedSender<NativeWake>,
    pub(super) stopping: AtomicBool,
}
pub enum NativeWake {
    Worker(NativeSessionKey),
    Priority(NativeSessionKey),
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> NativeEnvironment<P> {
    pub(crate) fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<NativeWake>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                targets: Mutex::new(HashMap::new()),
                ownership: Arc::new(NativeOwnershipGate::new()),
                changes: tx,
                stopping: AtomicBool::new(false),
            }),
            rx,
        )
    }
    pub(super) fn claim_created(
        &self,
        proof: &super::source_sessions::OriginalCreatedClaim,
    ) -> Result<(), super::DispatchError> {
        let result = self
            .ownership
            .register_live(proof.native(), proof.source(), true);
        self.wake(proof.native());
        result
    }
    pub(super) fn release_source_claims(
        &self,
        proof: &super::SourceClosureProof,
    ) -> Result<(), super::DispatchError> {
        for key in self.ownership.release_confirmed(proof)? {
            self.wake(&key);
            if let Some(target) = self.target(&key) {
                target.wake_writer();
                target.changed.notify_waiters();
            }
        }
        Ok(())
    }
    pub(super) fn target(&self, key: &NativeSessionKey) -> Option<Arc<NativeTarget<P>>> {
        self.targets.lock().ok()?.get(key).cloned()
    }
    pub(super) fn wake(&self, key: &NativeSessionKey) {
        let _ = self.changes.send(NativeWake::Worker(key.clone()));
    }
    pub(super) fn retire_closed_target(
        &self,
        key: &NativeSessionKey,
        target: &Arc<NativeTarget<P>>,
    ) {
        target.retire_closed_target();
        let mut targets = match self.targets.lock() {
            Ok(targets) => targets,
            Err(poisoned) => poisoned.into_inner(),
        };
        if targets
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, target))
        {
            targets.remove(key);
        }
    }
}
impl<P: crate::application::native_dispatch::NativeApprovalPort> QueueEnvironmentPort
    for NativeEnvironment<P>
{
    fn readiness(&self, key: &NativeSessionKey) -> DispatchReadiness {
        if self.stopping.load(Ordering::SeqCst) {
            return DispatchReadiness::Blocked;
        }
        let Some(target) = self.target(key) else {
            return DispatchReadiness::Blocked;
        };
        let Ok(f) = target.facts.lock() else {
            return DispatchReadiness::Blocked;
        };
        let Some(b) = &f.binding else {
            return DispatchReadiness::Blocked;
        };
        if !Arc::ptr_eq(&f.pre.authority, &b.authority) {
            return DispatchReadiness::Blocked;
        }
        if !b.authority.source.closing.load(Ordering::SeqCst)
            && !b.authority.source.quarantined.load(Ordering::SeqCst)
            && b.authority.source.required.load(Ordering::SeqCst) == 0
            && !b.authority.releasing.load(Ordering::SeqCst)
            && !b.authority.closed.load(Ordering::SeqCst)
            && !b.authority.source.closed.load(Ordering::SeqCst)
            && f.foreground == WorkActivity::Idle
            && matches!(
                f.foreground_evidence,
                ForegroundEvidence::IdleSnapshot
                    | ForegroundEvidence::OwnedV1Idle
                    | ForegroundEvidence::ReliableTerminal
            )
            && f.pending_input.is_none()
            && f.requirements.is_empty()
            && f.failed_requirements.is_empty()
            && f.control_holds.is_empty()
        {
            DispatchReadiness::Ready(b.resource)
        } else {
            DispatchReadiness::Blocked
        }
    }
    fn resource_epoch(&self, key: &NativeSessionKey) -> Option<ResourceEpoch> {
        let target = self.target(key)?;
        let f = target.facts.lock().ok()?;
        f.binding.as_ref().map(|b| b.resource)
    }
    fn poll_event(&self, key: &NativeSessionKey) -> Option<QueueOwnerEvent> {
        self.target(key)?.facts.lock().ok()?.events.pop_front()
    }
    fn queue_changed(&self, key: &NativeSessionKey) {
        self.wake(key);
    }
    fn priority_accepted(&self, key: &NativeSessionKey, intent: EntityId) {
        if let Some(target) = self.target(key)
            && let Ok(mut f) = target.facts.lock()
        {
            f.control_holds.insert(intent);
        }
        let _ = self.changes.send(NativeWake::Priority(key.clone()));
    }
    fn priority_failed(&self, key: &NativeSessionKey, intent: EntityId) {
        if let Some(target) = self.target(key)
            && let Ok(mut facts) = target.facts.lock()
            && facts.control_holds.remove(&intent)
        {
            target.wake_writer();
            target.changed.notify_waiters();
        }
    }
}
