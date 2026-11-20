//! In-memory single-instance root session control coordinator.
//! All `Mutex` critical sections are strictly synchronous and never cross `.await` or SQLite I/O.
use crate::domain::{EntityId, native::NativeSessionKey, session_capacity::DEFAULT_IDLE_TIMEOUT};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

pub const DEFAULT_CONTROL_IDLE_TIMEOUT: Duration = DEFAULT_IDLE_TIMEOUT;
pub const CONTROL_CHANGED_EVENT_TYPE: &str = "session.control.changed";
const DEFAULT_MAX_ROOTS: usize = 4096;
const DEFAULT_MAX_RECEIPTS: usize = 4096;
const BROADCAST_CAPACITY: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionControlSnapshot {
    pub target_id: Option<EntityId>,
    pub root_target_id: Option<EntityId>,
    pub source_id: EntityId,
    pub root_native_session_id: String,
    pub controller_client_id: Option<EntityId>,
    pub controller_display_name: Option<String>,
    pub control_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionControlChangedEvent {
    pub root_target_id: Option<EntityId>,
    pub source_id: EntityId,
    pub root_native_session_id: String,
    pub controller_client_id: Option<EntityId>,
    pub controller_display_name: Option<String>,
    pub control_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlMutationOutcome {
    pub snapshot: SessionControlSnapshot,
    pub state_changed: bool,
    pub previous_controller: Option<EntityId>,
    pub previous_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlClearedRecord {
    pub root: NativeSessionKey,
    pub root_target_id: Option<EntityId>,
    pub previous_controller: EntityId,
    pub previous_epoch: u64,
    pub control_epoch: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ControlError {
    #[error("session is not controlled by this client or control epoch changed")]
    NotControlled,
    #[error("control request identifier conflicts with a different action or target")]
    RequestConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlReceiptAction {
    Takeover,
    Release,
}

#[derive(Clone, Debug)]
struct CachedControlReceipt {
    action: ControlReceiptAction,
    target_id: EntityId,
    expected_epoch: Option<u64>,
    snapshot: SessionControlSnapshot,
}

#[derive(Clone, Debug)]
struct RootControlRecord {
    controller_client_id: Option<EntityId>,
    controller_display_name: Option<String>,
    control_epoch: u64,
    root_target_id: Option<EntityId>,
    loaded: bool,
    unloaded_claimed_at: Option<Duration>,
    last_touched_seq: u64,
}

impl RootControlRecord {
    fn snapshot(
        &self,
        root: &NativeSessionKey,
        target_id: Option<EntityId>,
    ) -> SessionControlSnapshot {
        SessionControlSnapshot {
            target_id,
            root_target_id: self.root_target_id,
            source_id: root.source().0,
            root_native_session_id: root.native_id().to_owned(),
            controller_client_id: self.controller_client_id,
            controller_display_name: self.controller_display_name.clone(),
            control_epoch: self.control_epoch,
        }
    }

    fn changed_event(&self, root: &NativeSessionKey) -> SessionControlChangedEvent {
        SessionControlChangedEvent {
            root_target_id: self.root_target_id,
            source_id: root.source().0,
            root_native_session_id: root.native_id().to_owned(),
            controller_client_id: self.controller_client_id,
            controller_display_name: self.controller_display_name.clone(),
            control_epoch: self.control_epoch,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RegisteredControlEpoch {
    root: NativeSessionKey,
    actor_id: EntityId,
    control_epoch: u64,
}

struct ControlState {
    roots: HashMap<NativeSessionKey, RootControlRecord>,
    evicted_epoch_floor: u64,
    touch_seq: u64,
    receipts: HashMap<(EntityId, EntityId), CachedControlReceipt>,
    receipt_order: VecDeque<(EntityId, EntityId)>,
    input_registrations: HashMap<(EntityId, EntityId), RegisteredControlEpoch>,
    input_registration_order: VecDeque<(EntityId, EntityId)>,
    approval_registrations: HashMap<EntityId, RegisteredControlEpoch>,
    approval_registration_order: VecDeque<EntityId>,
}

impl ControlState {
    fn new() -> Self {
        Self {
            roots: HashMap::new(),
            evicted_epoch_floor: 0,
            touch_seq: 0,
            receipts: HashMap::new(),
            receipt_order: VecDeque::new(),
            input_registrations: HashMap::new(),
            input_registration_order: VecDeque::new(),
            approval_registrations: HashMap::new(),
            approval_registration_order: VecDeque::new(),
        }
    }

    const fn next_touch_seq(&mut self) -> u64 {
        self.touch_seq = self.touch_seq.saturating_add(1);
        self.touch_seq
    }

    fn expire_unloaded(
        &mut self,
        now: Duration,
        idle_timeout: Duration,
    ) -> Vec<SessionControlChangedEvent> {
        let mut expired = Vec::new();
        for (root, record) in &mut self.roots {
            if record.controller_client_id.is_some()
                && !record.loaded
                && record
                    .unloaded_claimed_at
                    .is_some_and(|claimed_at| now.saturating_sub(claimed_at) >= idle_timeout)
            {
                record.controller_client_id = None;
                record.controller_display_name = None;
                record.unloaded_claimed_at = None;
                record.control_epoch = record
                    .control_epoch
                    .max(self.evicted_epoch_floor)
                    .saturating_add(1);
                expired.push(record.changed_event(root));
            }
        }
        expired
    }

    fn evict_root_if_needed(&mut self, key: &NativeSessionKey, max_roots: usize) {
        if self.roots.contains_key(key) || self.roots.len() < max_roots || max_roots == 0 {
            return;
        }
        // Prefer oldest uncontrolled and unloaded entry; fall back to oldest uncontrolled entry.
        let victim = self
            .roots
            .iter()
            .filter(|(_, record)| record.controller_client_id.is_none() && !record.loaded)
            .min_by_key(|(_, record)| record.last_touched_seq)
            .map(|(root, _)| root.clone())
            .or_else(|| {
                self.roots
                    .iter()
                    .filter(|(_, record)| record.controller_client_id.is_none())
                    .min_by_key(|(_, record)| record.last_touched_seq)
                    .map(|(root, _)| root.clone())
            });
        if let Some(victim) = victim
            && let Some(removed) = self.roots.remove(&victim)
        {
            self.evicted_epoch_floor = self.evicted_epoch_floor.max(removed.control_epoch);
        }
    }

    fn insert_receipt(
        &mut self,
        key: (EntityId, EntityId),
        receipt: CachedControlReceipt,
        max_receipts: usize,
    ) {
        if max_receipts == 0 {
            return;
        }
        if !self.receipts.contains_key(&key) {
            while self.receipts.len() >= max_receipts {
                let Some(oldest) = self.receipt_order.pop_front() else {
                    break;
                };
                self.receipts.remove(&oldest);
            }
            self.receipt_order.push_back(key);
        }
        self.receipts.insert(key, receipt);
    }

    fn insert_registration_if_absent<K: Copy + Eq + std::hash::Hash>(
        map: &mut HashMap<K, RegisteredControlEpoch>,
        order: &mut VecDeque<K>,
        key: K,
        entry: RegisteredControlEpoch,
        max_entries: usize,
    ) {
        if max_entries == 0 || map.contains_key(&key) {
            return;
        }
        while map.len() >= max_entries {
            let Some(oldest) = order.pop_front() else {
                break;
            };
            map.remove(&oldest);
        }
        order.push_back(key);
        map.insert(key, entry);
    }
}

pub struct SessionControlCoordinator {
    inner: Mutex<ControlState>,
    events: broadcast::Sender<SessionControlChangedEvent>,
    epoch_origin: Instant,
    idle_timeout: Duration,
    max_roots: usize,
    max_receipts: usize,
    #[cfg(test)]
    test_clock_advance_ms: AtomicU64,
}

impl Default for SessionControlCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionControlCoordinator {
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(
            DEFAULT_CONTROL_IDLE_TIMEOUT,
            DEFAULT_MAX_ROOTS,
            DEFAULT_MAX_RECEIPTS,
        )
    }

    #[must_use]
    fn with_limits(idle_timeout: Duration, max_roots: usize, max_receipts: usize) -> Self {
        let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            inner: Mutex::new(ControlState::new()),
            events,
            epoch_origin: Instant::now(),
            idle_timeout,
            max_roots,
            max_receipts,
            #[cfg(test)]
            test_clock_advance_ms: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_limits_for_test(
        idle_timeout: Duration,
        max_roots: usize,
        max_receipts: usize,
    ) -> Self {
        Self::with_limits(idle_timeout, max_roots, max_receipts)
    }

    #[cfg(test)]
    pub(crate) fn advance_clock_for_test(&self, delta: Duration) {
        let millis = u64::try_from(delta.as_millis()).unwrap_or(u64::MAX);
        self.test_clock_advance_ms
            .fetch_add(millis, Ordering::SeqCst);
    }

    fn now_offset(&self) -> Duration {
        let elapsed = self.epoch_origin.elapsed();
        #[cfg(test)]
        let elapsed = elapsed.saturating_add(Duration::from_millis(
            self.test_clock_advance_ms.load(Ordering::SeqCst),
        ));
        elapsed
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<SessionControlChangedEvent> {
        self.events.subscribe()
    }

    fn emit_events(&self, events: Vec<SessionControlChangedEvent>) {
        for event in events {
            let _ = self.events.send(event);
        }
    }

    /// Initializes or claims initial control for a newly created root session.
    pub fn claim_initial(
        &self,
        root: &NativeSessionKey,
        controller_client_id: EntityId,
        controller_display_name: &str,
        loaded: bool,
    ) -> ControlMutationOutcome {
        let now = self.now_offset();
        let (outcome, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            state.evict_root_if_needed(root, self.max_roots);
            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let entry = state
                .roots
                .entry(root.clone())
                .or_insert_with(|| RootControlRecord {
                    controller_client_id: None,
                    controller_display_name: None,
                    control_epoch: floor,
                    root_target_id: None,
                    loaded,
                    unloaded_claimed_at: None,
                    last_touched_seq: seq,
                });
            let previous_controller = entry.controller_client_id;
            let previous_epoch = entry.control_epoch;
            entry.controller_client_id = Some(controller_client_id);
            entry.controller_display_name = Some(controller_display_name.to_owned());
            entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
            entry.loaded = loaded;
            entry.unloaded_claimed_at = if loaded { None } else { Some(now) };
            entry.last_touched_seq = seq;
            let snapshot = entry.snapshot(root, entry.root_target_id);
            events.push(entry.changed_event(root));
            drop(state);
            (
                ControlMutationOutcome {
                    snapshot,
                    state_changed: true,
                    previous_controller,
                    previous_epoch,
                },
                events,
            )
        };
        self.emit_events(events);
        outcome
    }

    /// Updates cached `root_target_id` when a top-level target binding is registered.
    pub fn update_root_target_id(&self, root: &NativeSessionKey, root_target_id: EntityId) {
        let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = state.roots.get_mut(root) {
            entry.root_target_id = Some(root_target_id);
        }
    }

    /// Marks the capacity resource for `root` as loaded (clearing any cold-session claim timer).
    pub fn mark_loaded(&self, root: &NativeSessionKey) {
        let now = self.now_offset();
        let events = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let events = state.expire_unloaded(now, self.idle_timeout);
            let seq = state.next_touch_seq();
            if let Some(entry) = state.roots.get_mut(root) {
                entry.loaded = true;
                entry.unloaded_claimed_at = None;
                entry.last_touched_seq = seq;
            }
            drop(state);
            events
        };
        self.emit_events(events);
    }

    /// Inspects current control state for a resolved root session without acquiring control.
    #[must_use]
    pub fn inspect(
        &self,
        root: &NativeSessionKey,
        target_id: EntityId,
        root_target_id: Option<EntityId>,
    ) -> SessionControlSnapshot {
        let now = self.now_offset();
        let (snapshot, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let events = state.expire_unloaded(now, self.idle_timeout);
            let floor = state.evicted_epoch_floor;
            let seq = state.next_touch_seq();
            let snapshot = if let Some(entry) = state.roots.get_mut(root) {
                if let Some(resolved_root_target) = root_target_id {
                    entry.root_target_id = Some(resolved_root_target);
                }
                entry.last_touched_seq = seq;
                entry.snapshot(root, Some(target_id))
            } else {
                SessionControlSnapshot {
                    target_id: Some(target_id),
                    root_target_id,
                    source_id: root.source().0,
                    root_native_session_id: root.native_id().to_owned(),
                    controller_client_id: None,
                    controller_display_name: None,
                    control_epoch: floor,
                }
            };
            drop(state);
            (snapshot, events)
        };
        self.emit_events(events);
        snapshot
    }

    /// Explicitly takes or preempts control of a root session with idempotency tracking.
    ///
    /// # Errors
    /// Returns [`ControlError::RequestConflict`] when `request_id` was already used by the same
    /// actor for a different action or target.
    pub fn takeover(
        &self,
        actor_id: EntityId,
        display_name: &str,
        request_id: EntityId,
        target_id: EntityId,
        root: &NativeSessionKey,
        root_target_id: Option<EntityId>,
    ) -> Result<ControlMutationOutcome, ControlError> {
        let now = self.now_offset();
        let result = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            let receipt_key = (actor_id, request_id);
            if let Some(cached) = state.receipts.get(&receipt_key) {
                if cached.action == ControlReceiptAction::Takeover && cached.target_id == target_id
                {
                    let outcome = ControlMutationOutcome {
                        snapshot: cached.snapshot.clone(),
                        state_changed: false,
                        previous_controller: None,
                        previous_epoch: cached.snapshot.control_epoch,
                    };
                    drop(state);
                    self.emit_events(events);
                    return Ok(outcome);
                }
                drop(state);
                self.emit_events(events);
                return Err(ControlError::RequestConflict);
            }

            state.evict_root_if_needed(root, self.max_roots);
            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let entry = state
                .roots
                .entry(root.clone())
                .or_insert_with(|| RootControlRecord {
                    controller_client_id: None,
                    controller_display_name: None,
                    control_epoch: floor,
                    root_target_id,
                    loaded: false,
                    unloaded_claimed_at: None,
                    last_touched_seq: seq,
                });
            if let Some(resolved_root_target) = root_target_id {
                entry.root_target_id = Some(resolved_root_target);
            }
            entry.last_touched_seq = seq;

            let outcome = if entry.controller_client_id == Some(actor_id) {
                let previous_epoch = entry.control_epoch;
                entry.controller_display_name = Some(display_name.to_owned());
                if !entry.loaded && entry.unloaded_claimed_at.is_none() {
                    entry.unloaded_claimed_at = Some(now);
                }
                let snapshot = entry.snapshot(root, Some(target_id));
                ControlMutationOutcome {
                    snapshot,
                    state_changed: false,
                    previous_controller: Some(actor_id),
                    previous_epoch,
                }
            } else {
                let previous_controller = entry.controller_client_id.take();
                let previous_epoch = entry.control_epoch;
                entry.controller_client_id = Some(actor_id);
                entry.controller_display_name = Some(display_name.to_owned());
                entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
                entry.unloaded_claimed_at = if entry.loaded { None } else { Some(now) };
                events.push(entry.changed_event(root));
                let snapshot = entry.snapshot(root, Some(target_id));
                ControlMutationOutcome {
                    snapshot,
                    state_changed: true,
                    previous_controller,
                    previous_epoch,
                }
            };

            state.insert_receipt(
                receipt_key,
                CachedControlReceipt {
                    action: ControlReceiptAction::Takeover,
                    target_id,
                    expected_epoch: None,
                    snapshot: outcome.snapshot.clone(),
                },
                self.max_receipts,
            );
            drop(state);
            (outcome, events)
        };
        self.emit_events(result.1);
        Ok(result.0)
    }

    /// Explicitly releases control held by `actor_id` for a root session.
    ///
    /// # Errors
    /// - [`ControlError::RequestConflict`] when `request_id` was already used with a different
    ///   action, target, or expected epoch.
    /// - [`ControlError::NotControlled`] when the root session is not controlled by `actor_id` or
    ///   its `control_epoch` differs from `expected_epoch`.
    pub fn release(
        &self,
        actor_id: EntityId,
        request_id: EntityId,
        target_id: EntityId,
        root: &NativeSessionKey,
        root_target_id: Option<EntityId>,
        expected_epoch: Option<u64>,
    ) -> Result<ControlMutationOutcome, ControlError> {
        let now = self.now_offset();
        let (outcome, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            let receipt_key = (actor_id, request_id);
            if let Some(cached) = state.receipts.get(&receipt_key) {
                if cached.action == ControlReceiptAction::Release
                    && cached.target_id == target_id
                    && cached.expected_epoch == expected_epoch
                {
                    let outcome = ControlMutationOutcome {
                        snapshot: cached.snapshot.clone(),
                        state_changed: false,
                        previous_controller: None,
                        previous_epoch: cached.snapshot.control_epoch,
                    };
                    drop(state);
                    self.emit_events(events);
                    return Ok(outcome);
                }
                drop(state);
                self.emit_events(events);
                return Err(ControlError::RequestConflict);
            }

            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let Some(entry) = state.roots.get_mut(root) else {
                drop(state);
                self.emit_events(events);
                return Err(ControlError::NotControlled);
            };
            if entry.controller_client_id != Some(actor_id)
                || expected_epoch.is_some_and(|expected| expected != entry.control_epoch)
            {
                drop(state);
                self.emit_events(events);
                return Err(ControlError::NotControlled);
            }

            if let Some(resolved_root_target) = root_target_id {
                entry.root_target_id = Some(resolved_root_target);
            }
            let previous_epoch = entry.control_epoch;
            entry.controller_client_id = None;
            entry.controller_display_name = None;
            entry.unloaded_claimed_at = None;
            entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
            entry.last_touched_seq = seq;
            let snapshot = entry.snapshot(root, Some(target_id));
            events.push(entry.changed_event(root));

            let outcome = ControlMutationOutcome {
                snapshot: snapshot.clone(),
                state_changed: true,
                previous_controller: Some(actor_id),
                previous_epoch,
            };
            state.insert_receipt(
                receipt_key,
                CachedControlReceipt {
                    action: ControlReceiptAction::Release,
                    target_id,
                    expected_epoch,
                    snapshot,
                },
                self.max_receipts,
            );
            drop(state);
            (outcome, events)
        };
        self.emit_events(events);
        Ok(outcome)
    }

    /// Authorizes a controlled write operation for `root` by `actor_id` and returns the current
    /// `control_epoch`.
    ///
    /// # Errors
    /// Returns [`ControlError::NotControlled`] when the root is uncontrolled, held by another
    /// client, or `expected_epoch` mismatches.
    pub fn authorize_controlled(
        &self,
        root: &NativeSessionKey,
        actor_id: EntityId,
        expected_epoch: Option<u64>,
    ) -> Result<u64, ControlError> {
        let now = self.now_offset();
        let (result, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let events = state.expire_unloaded(now, self.idle_timeout);
            let result = state
                .roots
                .get(root)
                .map_or(Err(ControlError::NotControlled), |entry| {
                    if entry.controller_client_id == Some(actor_id)
                        && expected_epoch.is_none_or(|expected| expected == entry.control_epoch)
                    {
                        Ok(entry.control_epoch)
                    } else {
                        Err(ControlError::NotControlled)
                    }
                });
            drop(state);
            (result, events)
        };
        self.emit_events(events);
        result
    }

    /// Clears control when a session capacity resource is confirmed closed, preserving monotonic
    /// `control_epoch` growth if controlled.
    pub fn clear_on_capacity_closed(&self, root: &NativeSessionKey) -> bool {
        let now = self.now_offset();
        let (cleared, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let mut cleared = false;
            if let Some(entry) = state.roots.get_mut(root) {
                entry.loaded = false;
                entry.unloaded_claimed_at = None;
                entry.last_touched_seq = seq;
                if entry.controller_client_id.take().is_some() {
                    entry.controller_display_name = None;
                    entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
                    events.push(entry.changed_event(root));
                    cleared = true;
                }
            }
            drop(state);
            (cleared, events)
        };
        self.emit_events(events);
        cleared
    }

    /// Clears control for all root sessions belonging to a physically closed source.
    pub fn clear_on_source_closed(&self, roots: &[NativeSessionKey]) -> usize {
        if roots.is_empty() {
            return 0;
        }
        let now = self.now_offset();
        let (count, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let mut count = 0_usize;
            for root in roots {
                if let Some(entry) = state.roots.get_mut(root) {
                    entry.loaded = false;
                    entry.unloaded_claimed_at = None;
                    entry.last_touched_seq = seq;
                    if entry.controller_client_id.take().is_some() {
                        entry.controller_display_name = None;
                        entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
                        events.push(entry.changed_event(root));
                        count += 1;
                    }
                }
            }
            (count, events)
        };
        self.emit_events(events);
        count
    }

    /// Clears control on all root sessions currently controlled by a revoked client.
    pub fn clear_revoked_client(&self, revoked_client: EntityId) -> Vec<ControlClearedRecord> {
        let now = self.now_offset();
        let (cleared, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let mut events = state.expire_unloaded(now, self.idle_timeout);
            let seq = state.next_touch_seq();
            let floor = state.evicted_epoch_floor;
            let mut cleared = Vec::new();
            for (root, entry) in &mut state.roots {
                if entry.controller_client_id == Some(revoked_client) {
                    let previous_epoch = entry.control_epoch;
                    entry.controller_client_id = None;
                    entry.controller_display_name = None;
                    entry.unloaded_claimed_at = None;
                    entry.control_epoch = entry.control_epoch.max(floor).saturating_add(1);
                    entry.last_touched_seq = seq;
                    cleared.push(ControlClearedRecord {
                        root: root.clone(),
                        root_target_id: entry.root_target_id,
                        previous_controller: revoked_client,
                        previous_epoch,
                        control_epoch: entry.control_epoch,
                    });
                    events.push(entry.changed_event(root));
                }
            }
            drop(state);
            (cleared, events)
        };
        self.emit_events(events);
        cleared
    }

    /// Expires any unloaded pre-takeover claims that exceeded the idle timeout.
    pub fn sweep_expired(&self) -> usize {
        let now = self.now_offset();
        let events = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            state.expire_unloaded(now, self.idle_timeout)
        };
        let count = events.len();
        self.emit_events(events);
        count
    }

    /// Binds an accepted `input.register` identity to the `control_epoch` held at registration.
    pub fn bind_input_registration(
        &self,
        registration_id: EntityId,
        root: &NativeSessionKey,
        actor_id: EntityId,
        control_epoch: u64,
    ) {
        let max_entries = self.max_receipts;
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let state = &mut *guard;
        ControlState::insert_registration_if_absent(
            &mut state.input_registrations,
            &mut state.input_registration_order,
            (actor_id, registration_id),
            RegisteredControlEpoch {
                root: root.clone(),
                actor_id,
                control_epoch,
            },
            max_entries,
        );
        drop(guard);
    }

    /// Verifies `input.consume` against current control and its registration-time epoch.
    ///
    /// # Errors
    /// Returns [`ControlError::NotControlled`] if uncontrolled, `expected_epoch` mismatches,
    /// the registration binding is missing, or the `control_epoch` changed since registration
    /// (including `A(1) -> B(2) -> A(3)` wrap-around).
    pub fn verify_input_consume(
        &self,
        registration_id: EntityId,
        root: &NativeSessionKey,
        actor_id: EntityId,
        expected_epoch: Option<u64>,
    ) -> Result<u64, ControlError> {
        let key = (actor_id, registration_id);
        self.verify_registered_step(root, actor_id, expected_epoch, |state| {
            state.input_registrations.get(&key)
        })
    }

    /// Binds an accepted `approval.register` identity to the `control_epoch` held at registration.
    pub fn bind_approval_registration(
        &self,
        registration_id: EntityId,
        root: &NativeSessionKey,
        actor_id: EntityId,
        control_epoch: u64,
    ) {
        let max_entries = self.max_receipts;
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let state = &mut *guard;
        ControlState::insert_registration_if_absent(
            &mut state.approval_registrations,
            &mut state.approval_registration_order,
            registration_id,
            RegisteredControlEpoch {
                root: root.clone(),
                actor_id,
                control_epoch,
            },
            max_entries,
        );
        drop(guard);
    }

    /// Verifies `approval.decide` against current control and its registration-time epoch.
    ///
    /// # Errors
    /// Returns [`ControlError::NotControlled`] if uncontrolled, `expected_epoch` mismatches,
    /// the registration binding is missing, or the `control_epoch` changed since registration
    /// (including `A(1) -> B(2) -> A(3)` wrap-around).
    pub fn verify_approval_decide(
        &self,
        registration_id: EntityId,
        root: &NativeSessionKey,
        actor_id: EntityId,
        expected_epoch: Option<u64>,
    ) -> Result<u64, ControlError> {
        self.verify_registered_step(root, actor_id, expected_epoch, |state| {
            state.approval_registrations.get(&registration_id)
        })
    }

    fn verify_registered_step<F>(
        &self,
        root: &NativeSessionKey,
        actor_id: EntityId,
        expected_epoch: Option<u64>,
        lookup: F,
    ) -> Result<u64, ControlError>
    where
        F: FnOnce(&ControlState) -> Option<&RegisteredControlEpoch>,
    {
        let now = self.now_offset();
        let (result, events) = {
            let mut state = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            let events = state.expire_unloaded(now, self.idle_timeout);
            let result = match state.roots.get(root) {
                Some(entry)
                    if entry.controller_client_id == Some(actor_id)
                        && expected_epoch
                            .is_none_or(|expected| expected == entry.control_epoch) =>
                {
                    let current_epoch = entry.control_epoch;
                    match lookup(&state) {
                        Some(bound)
                            if bound.root == *root
                                && bound.actor_id == actor_id
                                && bound.control_epoch == current_epoch =>
                        {
                            Ok(current_epoch)
                        }
                        _ => Err(ControlError::NotControlled),
                    }
                }
                _ => Err(ControlError::NotControlled),
            };
            drop(state);
            (result, events)
        };
        self.emit_events(events);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::error::TryRecvError;

    fn id(byte: u8) -> EntityId {
        EntityId::from_bytes([byte; 16])
    }

    fn root(source: u8, session: &str) -> NativeSessionKey {
        NativeSessionKey::from_config(Some(id(source)), session).unwrap()
    }

    #[test]
    fn sct_006_takeover_release_idempotency_conflict_and_retakeover() {
        let coordinator = SessionControlCoordinator::new();
        let mut rx = coordinator.subscribe();
        let r1 = root(1, "s1");
        let t1 = id(10);
        let t1_child = id(11);
        let t2 = id(12);
        let client_a = id(20);
        let client_b = id(21);
        let req_1 = id(30);
        let req_2 = id(31);
        let req_3 = id(32);
        let req_4 = id(33);

        let initial = coordinator.inspect(&r1, t1, Some(t1));
        assert_eq!(initial.controller_client_id, None);
        assert_eq!(initial.control_epoch, 0);
        assert_eq!(initial.root_target_id, Some(t1));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        // 1. Client B takes over S1 -> epoch 1, broadcast emitted.
        let taken = coordinator
            .takeover(client_b, "Phone", req_1, t1, &r1, Some(t1))
            .unwrap();
        assert!(taken.state_changed);
        assert_eq!(taken.snapshot.controller_client_id, Some(client_b));
        assert_eq!(
            taken.snapshot.controller_display_name.as_deref(),
            Some("Phone")
        );
        assert_eq!(taken.snapshot.control_epoch, 1);
        assert_eq!(
            rx.try_recv().unwrap(),
            SessionControlChangedEvent {
                root_target_id: Some(t1),
                source_id: id(1),
                root_native_session_id: "s1".to_owned(),
                controller_client_id: Some(client_b),
                controller_display_name: Some("Phone".to_owned()),
                control_epoch: 1,
            }
        );

        // Retry with same (client_b, req_1, t1) is idempotent without epoch bump or broadcast.
        let retry = coordinator
            .takeover(client_b, "Phone", req_1, t1, &r1, Some(t1))
            .unwrap();
        assert!(!retry.state_changed);
        assert_eq!(retry.snapshot, taken.snapshot);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        // 2. Same req_1 with different target (even under same root or different target) or release -> RequestConflict.
        assert_eq!(
            coordinator.takeover(client_b, "Phone", req_1, t1_child, &r1, Some(t1)),
            Err(ControlError::RequestConflict)
        );
        assert_eq!(
            coordinator.takeover(client_b, "Phone", req_1, t2, &root(1, "s2"), Some(t2)),
            Err(ControlError::RequestConflict)
        );
        assert_eq!(
            coordinator.release(client_b, req_1, t1, &r1, Some(t1), None),
            Err(ControlError::RequestConflict)
        );

        // 3. Same controller B with new req_2 refreshes display name while keeping control_epoch = 1.
        let refreshed = coordinator
            .takeover(client_b, "Phone v2", req_2, t1, &r1, Some(t1))
            .unwrap();
        assert!(!refreshed.state_changed);
        assert_eq!(refreshed.snapshot.control_epoch, 1);
        assert_eq!(
            refreshed.snapshot.controller_display_name.as_deref(),
            Some("Phone v2")
        );
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        // 4a. Non-controller release or mismatched expectedControlEpoch fails with NotControlled.
        assert_eq!(
            coordinator.release(client_a, req_3, t1, &r1, Some(t1), None),
            Err(ControlError::NotControlled)
        );
        assert_eq!(
            coordinator.release(client_b, req_3, t1, &r1, Some(t1), Some(99)),
            Err(ControlError::NotControlled)
        );
        assert_eq!(coordinator.authorize_controlled(&r1, client_b, None), Ok(1));

        // 4b. Controller B releases with matching expected epoch -> epoch rises to 2, uncontrolled.
        let released = coordinator
            .release(client_b, req_3, t1, &r1, Some(t1), Some(1))
            .unwrap();
        assert!(released.state_changed);
        assert_eq!(released.snapshot.controller_client_id, None);
        assert_eq!(released.snapshot.controller_display_name, None);
        assert_eq!(released.snapshot.control_epoch, 2);
        assert_eq!(rx.try_recv().unwrap().control_epoch, 2);

        // Retry of release returns same snapshot without epoch bump.
        let release_retry = coordinator
            .release(client_b, req_3, t1, &r1, Some(t1), Some(1))
            .unwrap();
        assert!(!release_retry.state_changed);
        assert_eq!(release_retry.snapshot, released.snapshot);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

        // Conflict on release retry with different expected epoch.
        assert_eq!(
            coordinator.release(client_b, req_3, t1, &r1, Some(t1), None),
            Err(ControlError::RequestConflict)
        );

        // Releasing again with new request_id after already released returns NotControlled.
        assert_eq!(
            coordinator.release(client_b, req_4, t1, &r1, Some(t1), None),
            Err(ControlError::NotControlled)
        );
    }

    #[test]
    fn sct_006_and_sct_015_evicted_epoch_floor_and_unloaded_timeout() {
        let coordinator =
            SessionControlCoordinator::with_limits_for_test(Duration::from_mins(30), 2, 16);
        let mut rx = coordinator.subscribe();
        let r1 = root(1, "r1");
        let r2 = root(1, "r2");
        let r3 = root(1, "r3");
        let client_a = id(20);

        // Take over + release r1 twice -> r1.control_epoch = 4
        coordinator
            .takeover(client_a, "A", id(1), id(10), &r1, Some(id(10)))
            .unwrap();
        coordinator
            .release(client_a, id(2), id(10), &r1, Some(id(10)), None)
            .unwrap();
        coordinator
            .takeover(client_a, "A", id(3), id(10), &r1, Some(id(10)))
            .unwrap();
        coordinator
            .release(client_a, id(4), id(10), &r1, Some(id(10)), None)
            .unwrap();
        assert_eq!(
            coordinator.inspect(&r1, id(10), Some(id(10))).control_epoch,
            4
        );

        // Occupy r2 uncontrolled slot -> capacity full (2)
        coordinator
            .takeover(client_a, "A", id(5), id(11), &r2, Some(id(11)))
            .unwrap();
        coordinator
            .release(client_a, id(6), id(11), &r2, Some(id(11)), None)
            .unwrap();

        // Adding r3 evicts r1 (oldest uncontrolled) and raises evicted_epoch_floor to 4.
        let r3_taken = coordinator
            .takeover(client_a, "A", id(7), id(12), &r3, Some(id(12)))
            .unwrap();
        assert!(r3_taken.snapshot.control_epoch >= 5);
        assert!(coordinator.inspect(&r1, id(10), Some(id(10))).control_epoch >= 4);

        // Re-taking over previously evicted r1 strictly exceeds evicted floor (never resets to 0 or 1).
        let r1_reclaimed = coordinator
            .takeover(client_a, "A", id(8), id(10), &r1, Some(id(10)))
            .unwrap();
        assert!(r1_reclaimed.snapshot.control_epoch >= 5);

        // Advance 29m59s: unloaded r3 + r1 still controlled.
        while rx.try_recv().is_ok() {}
        coordinator.mark_loaded(&r3);
        coordinator.advance_clock_for_test(Duration::from_secs(1799));
        assert_eq!(coordinator.sweep_expired(), 0);
        assert!(
            coordinator
                .authorize_controlled(&r1, client_a, None)
                .is_ok()
        );

        // Advance +2s (total > 30m): unloaded r1 expires & increments epoch; loaded r3 stays controlled!
        coordinator.advance_clock_for_test(Duration::from_secs(2));
        assert_eq!(coordinator.sweep_expired(), 1);
        assert_eq!(
            coordinator.authorize_controlled(&r1, client_a, None),
            Err(ControlError::NotControlled)
        );
        assert!(
            coordinator
                .authorize_controlled(&r3, client_a, None)
                .is_ok()
        );
        let evt = rx.try_recv().unwrap();
        assert_eq!(evt.root_native_session_id, "r1");
        assert_eq!(evt.controller_client_id, None);
        assert_eq!(evt.control_epoch, r1_reclaimed.snapshot.control_epoch + 1);

        // Loaded r3 clears cleanly on capacity_closed (epoch increments) and stays unchanged on repeat.
        assert!(coordinator.clear_on_capacity_closed(&r3));
        assert!(!coordinator.clear_on_capacity_closed(&r3));
        let r3_after = coordinator.inspect(&r3, id(12), Some(id(12)));
        assert_eq!(r3_after.controller_client_id, None);
        assert_eq!(r3_after.control_epoch, r3_taken.snapshot.control_epoch + 1);
    }

    #[test]
    fn sct_006_two_step_epoch_wrap_around_and_release_aba_rejected() {
        let coordinator = SessionControlCoordinator::new();
        let r1 = root(1, "s-two-step");
        let t1 = id(10);
        let client_a = id(20);
        let client_b = id(21);
        let in_reg_1 = id(40);
        let in_reg_missing = id(41);
        let ap_reg_1 = id(50);
        let ap_reg_2 = id(51);

        // Epoch 1: Client A claims initial control and registers step-1 input & approval.
        let init = coordinator.claim_initial(&r1, client_a, "A", true);
        assert_eq!(init.snapshot.control_epoch, 1);
        coordinator.bind_input_registration(in_reg_1, &r1, client_a, init.snapshot.control_epoch);
        coordinator.bind_approval_registration(
            ap_reg_1,
            &r1,
            client_a,
            init.snapshot.control_epoch,
        );

        // Within same epoch 1 (and no later takeover), both step-2 checks succeed.
        assert_eq!(
            coordinator.verify_input_consume(in_reg_1, &r1, client_a, None),
            Ok(1)
        );
        assert_eq!(
            coordinator.verify_approval_decide(ap_reg_1, &r1, client_a, Some(1)),
            Ok(1)
        );
        // Unregistered/missing binding cannot pass consume/decide.
        assert_eq!(
            coordinator.verify_input_consume(in_reg_missing, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );

        // Epoch 2: Client B preempts; A's step-2 fails.
        let taken_b = coordinator
            .takeover(client_b, "B", id(60), t1, &r1, Some(t1))
            .unwrap();
        assert_eq!(taken_b.snapshot.control_epoch, 2);
        assert_eq!(
            coordinator.verify_input_consume(in_reg_1, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );
        assert_eq!(
            coordinator.verify_approval_decide(ap_reg_1, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );

        // Epoch 3: Client A retakes over; A now controls at epoch 3, yet epoch-1 registrations MUST still fail!
        let retake_a = coordinator
            .takeover(client_a, "A", id(61), t1, &r1, Some(t1))
            .unwrap();
        assert_eq!(retake_a.snapshot.control_epoch, 3);
        // Even if client_a retries bind_input_registration for in_reg_1 or omits expected_epoch or passes Some(3), epoch-1 binding stays immutable!
        coordinator.bind_input_registration(
            in_reg_1,
            &r1,
            client_a,
            retake_a.snapshot.control_epoch,
        );
        assert_eq!(
            coordinator.verify_input_consume(in_reg_1, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );
        assert_eq!(
            coordinator.verify_input_consume(in_reg_1, &r1, client_a, Some(3)),
            Err(ControlError::NotControlled)
        );
        assert_eq!(
            coordinator.verify_approval_decide(ap_reg_1, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );

        // Fresh registration in epoch 3 -> capacity release (epoch 4) -> retake (epoch 5) also blocks ABA!
        coordinator.bind_approval_registration(ap_reg_2, &r1, client_a, 3);
        assert!(coordinator.clear_on_capacity_closed(&r1));
        let post_release_a = coordinator
            .takeover(client_a, "A", id(62), t1, &r1, Some(t1))
            .unwrap();
        assert_eq!(post_release_a.snapshot.control_epoch, 5);
        assert_eq!(
            coordinator.verify_approval_decide(ap_reg_2, &r1, client_a, None),
            Err(ControlError::NotControlled)
        );
    }
}
