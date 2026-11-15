//! Per-original-read raw coverage. No view cache, actor, IO or reconstructed RPC identity.
use super::{
    read_control::RpcScalar,
    read_observation::{ReadContextKind, ReadObservation, ReadWindowKind},
};
use crate::domain::shared_view::ViewError;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
pub struct ReadPrefixIdentity {
    pub rpc: RpcScalar,
    pub session: Arc<str>,
    pub cell: u64,
}
pub struct ReadPrefix {
    identity: ReadPrefixIdentity,
    start: u64,
    end: AtomicU64,
    gap: Mutex<Option<(u64, ViewError)>>,
    unassociated: AtomicU64,
    finished: AtomicBool,
    registry: Weak<Mutex<Registry>>,
}
impl ReadPrefix {
    pub fn response(&self, sequence: u64) {
        self.end.store(sequence, Ordering::Release);
    }
    pub fn failure(&self) -> Option<ViewError> {
        let end = self.end.load(Ordering::Acquire);
        let gap = match self.gap.lock() {
            Ok(gap) => *gap,
            Err(poison) => *poison.into_inner(),
        };
        gap.filter(|(sequence, _)| end == 0 || *sequence <= end)
            .map(|(_, error)| error)
    }
    pub fn association_limited(&self) -> bool {
        let sequence = self.unassociated.load(Ordering::Acquire);
        let end = self.end.load(Ordering::Acquire);
        sequence != 0 && (end == 0 || sequence <= end)
    }
    pub fn finish(&self) {
        self.finished.store(true, Ordering::Release);
        if let Some(registry) = self.registry.upgrade()
            && let Ok(mut registry) = registry.lock()
            && registry
                .rpcs
                .get(&self.identity.rpc)
                .is_some_and(|weak| std::ptr::eq(weak.as_ptr(), self))
        {
            registry.rpcs.remove(&self.identity.rpc);
            registry.sessions.remove(&self.identity.session);
        }
    }
    fn record_gap(&self, sequence: u64, error: ViewError) {
        let end = self.end.load(Ordering::Acquire);
        if end != 0 && sequence > end {
            return;
        }
        let mut gap = match self.gap.lock() {
            Ok(gap) => gap,
            Err(poison) => poison.into_inner(),
        };
        if gap.is_none_or(|(old, _)| sequence < old) {
            *gap = Some((sequence, error));
        }
    }
    fn record_unassociated(&self, sequence: u64) {
        let end = self.end.load(Ordering::Acquire);
        if sequence <= self.start || (end != 0 && sequence > end) {
            return;
        }
        let _ = self
            .unassociated
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                Some(if old == 0 {
                    sequence
                } else {
                    old.min(sequence)
                })
            });
    }
}
struct Registry {
    sessions: HashMap<Arc<str>, Weak<ReadPrefix>>,
    rpcs: HashMap<RpcScalar, Weak<ReadPrefix>>,
}
pub struct ReadPrefixRegistry(Arc<Mutex<Registry>>);
impl ReadPrefixRegistry {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Registry {
            sessions: HashMap::new(),
            rpcs: HashMap::new(),
        })))
    }
    pub fn track(
        &self,
        identity: ReadPrefixIdentity,
        start: u64,
    ) -> Result<Arc<ReadPrefix>, ViewError> {
        let prefix = Arc::new(ReadPrefix {
            identity,
            start,
            end: AtomicU64::new(0),
            gap: Mutex::new(None),
            unassociated: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            registry: Arc::downgrade(&self.0),
        });
        let mut registry = self.0.lock().map_err(|_| ViewError::ReadFailure)?;
        if let Some(previous) = registry
            .sessions
            .get(&prefix.identity.session)
            .and_then(Weak::upgrade)
        {
            if !previous.finished.load(Ordering::Acquire) {
                return Err(ViewError::AlreadyAttached);
            }
            registry.rpcs.remove(&previous.identity.rpc);
        }
        registry
            .sessions
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        registry
            .rpcs
            .try_reserve(1)
            .map_err(|_| ViewError::AllocationFailure)?;
        registry.sessions.insert(
            Arc::clone(&prefix.identity.session),
            Arc::downgrade(&prefix),
        );
        registry
            .rpcs
            .insert(prefix.identity.rpc.clone(), Arc::downgrade(&prefix));
        drop(registry);
        Ok(prefix)
    }
    pub fn record_gap(&self, sequence: u64, error: ViewError, observation: &ReadObservation) {
        let registry = match self.0.lock() {
            Ok(registry) => registry,
            Err(poison) => poison.into_inner(),
        };
        if let Some(rpc) = observation.response.as_ref() {
            if let Some(prefix) = registry.rpcs.get(rpc).and_then(Weak::upgrade) {
                prefix.record_gap(sequence, error);
            }
            return;
        }
        if observation.kind == ReadContextKind::UniqueReceiptWindow
            && let Some(window) = &observation.window
        {
            if let Some(prefix) = registry.rpcs.get(&window.rpc).and_then(Weak::upgrade)
                && prefix.identity.session == window.session
                && prefix.identity.cell == window.receipt_cell
                && window.kind == ReadWindowKind::Replay
            {
                prefix.record_gap(sequence, error);
            }
            return;
        }
        if let Some(session) = &observation.session
            && let Some(prefix) = registry.sessions.get(session).and_then(Weak::upgrade)
        {
            prefix.record_unassociated(sequence);
        }
    }
}
