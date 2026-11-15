//! Disposable node-owned windows. These handles never grant network or native authority.
use super::native::{NativeSessionKey, ResourceEpoch};
use std::{
    collections::LinkedList,
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

pub const ROOT_BYTES: usize = 8192;
pub const IDENTITY_BYTES: usize = 1024;
pub const WINDOW_BYTES: usize = 1024;
pub const ITEM_BYTES: usize = 512;
pub const SNAPSHOT_BYTES: usize = 256;
pub const REFERENCE_BYTES: usize = 64;
pub const MAX_IDENTITIES: usize = 64;

mod staging;
pub use staging::{
    BoundedWriter, ItemLease, PublishFact, StageParts, StageWorkspace, StagedItem, WindowStatus,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewLimits {
    pub target_bytes: usize,
    pub node_bytes: usize,
    pub target_items: usize,
    pub node_items: usize,
}
impl Default for ViewLimits {
    fn default() -> Self {
        Self {
            target_bytes: 16 * 1024 * 1024,
            node_bytes: 128 * 1024 * 1024,
            target_items: 4096,
            node_items: 32768,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ViewUsage {
    pub bytes: usize,
    /// Immutable objects plus every snapshot reference, including evicted leased objects.
    pub items: usize,
}
struct Ledger {
    usage: ViewUsage,
    broken: bool,
}
struct NodeBudget {
    limits: ViewLimits,
    ledger: Mutex<Ledger>,
}
struct TargetBudget {
    node: Arc<NodeBudget>,
    ledger: Mutex<Ledger>,
    identity_bytes: usize,
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn addition(
    ledger: &Ledger,
    bytes: usize,
    items: usize,
    max_bytes: usize,
    max_items: usize,
) -> Result<ViewUsage, ViewError> {
    let next = ViewUsage {
        bytes: ledger
            .usage
            .bytes
            .checked_add(bytes)
            .ok_or(ViewError::Capacity)?,
        items: ledger
            .usage
            .items
            .checked_add(items)
            .ok_or(ViewError::Capacity)?,
    };
    if ledger.broken || next.bytes > max_bytes || next.items > max_items {
        return Err(ViewError::Capacity);
    }
    Ok(next)
}
const fn subtract(ledger: &mut Ledger, bytes: usize, items: usize) {
    if let (Some(bytes), Some(items)) = (
        ledger.usage.bytes.checked_sub(bytes),
        ledger.usage.items.checked_sub(items),
    ) {
        ledger.usage = ViewUsage { bytes, items };
    } else {
        // An accounting invariant violation permanently blocks allocations; never wrap or mint room.
        ledger.broken = true;
    }
}
impl TargetBudget {
    fn create(node: &Arc<NodeBudget>, native_bytes: usize) -> Result<Arc<Self>, ViewError> {
        let bytes = IDENTITY_BYTES
            .checked_add(native_bytes)
            .ok_or(ViewError::Capacity)?;
        let mut global = lock(&node.ledger);
        let next = addition(
            &global,
            bytes,
            0,
            node.limits.node_bytes,
            node.limits.node_items,
        )?;
        if bytes > node.limits.target_bytes {
            return Err(ViewError::Capacity);
        }
        global.usage = next;
        drop(global);
        Ok(Arc::new(Self {
            node: node.clone(),
            ledger: Mutex::new(Ledger {
                usage: ViewUsage { bytes, items: 0 },
                broken: false,
            }),
            identity_bytes: bytes,
        }))
    }
    fn reserve(self: &Arc<Self>, bytes: usize, items: usize) -> Result<Charge, ViewError> {
        // Every reserve/release follows this order, including snapshot Drop on other threads.
        let mut global = lock(&self.node.ledger);
        let mut local = lock(&self.ledger);
        let limits = self.node.limits;
        let next_global = addition(&global, bytes, items, limits.node_bytes, limits.node_items)?;
        let next_local = addition(
            &local,
            bytes,
            items,
            limits.target_bytes,
            limits.target_items,
        )?;
        global.usage = next_global;
        local.usage = next_local;
        drop(local);
        drop(global);
        Ok(Charge {
            target: self.clone(),
            bytes,
            items,
        })
    }
}
impl Drop for TargetBudget {
    fn drop(&mut self) {
        subtract(&mut lock(&self.node.ledger), self.identity_bytes, 0);
    }
}
struct Charge {
    target: Arc<TargetBudget>,
    bytes: usize,
    items: usize,
}
impl Drop for Charge {
    fn drop(&mut self) {
        let mut global = lock(&self.target.node.ledger);
        let mut local = lock(&self.target.ledger);
        subtract(&mut global, self.bytes, self.items);
        subtract(&mut local, self.bytes, self.items);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyNamespace {
    UserMessage,
    AgentMessage,
    Thought,
    Tool,
    Terminal,
    Plan,
    Extension,
}
/// Borrowed, adapter-provided reliable identity; never inferred from the displayed bytes.
#[derive(Clone, Copy)]
pub struct SourceKey<'a> {
    pub namespace: KeyNamespace,
    pub id: &'a str,
}
struct StoredKey {
    namespace: KeyNamespace,
    id: Box<str>,
}
impl StoredKey {
    fn matches(&self, other: SourceKey<'_>) -> bool {
        self.namespace == other.namespace && self.id.as_ref() == other.id
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewFailure {
    Capacity,
    SourceRead,
    ConnectionLost,
    Structure,
    SequenceExhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewState {
    Reading,
    Finished,
    Incomplete(ViewFailure),
}
#[derive(Clone)]
pub struct ViewHandle {
    scope: Arc<()>,
    target: NativeSessionKey,
    resource: ResourceEpoch,
    generation: u64,
}
impl ViewHandle {
    #[must_use]
    pub const fn target(&self) -> &NativeSessionKey {
        &self.target
    }
    #[must_use]
    pub const fn resource(&self) -> ResourceEpoch {
        self.resource
    }
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}
impl fmt::Debug for ViewHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewHandle")
            .field("target", &self.target)
            .field("resource", &self.resource)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
struct Item {
    position: u64,
    key: Option<StoredKey>,
    bytes: Box<[u8]>,
    _charge: Charge,
}
struct ReadStatusCell {
    meta: Mutex<ReadMeta>,
    _charge: Charge,
}
struct ReadMeta {
    state: ViewState,
    revision: u64,
    sealed: bool,
}
impl ReadMeta {
    const fn ensure_open(&self) -> Result<(), ViewError> {
        if self.sealed {
            Err(ViewError::SequenceExhausted)
        } else {
            Ok(())
        }
    }
    fn advance(&mut self) -> Result<(), ViewError> {
        self.ensure_open()?;
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
            Ok(())
        } else {
            self.sealed = true;
            if !matches!(self.state, ViewState::Incomplete(_)) {
                self.state = ViewState::Incomplete(ViewFailure::SequenceExhausted);
            }
            Err(ViewError::SequenceExhausted)
        }
    }
}
impl ReadStatusCell {
    fn state(&self) -> ViewState {
        lock(&self.meta).state
    }
    fn ensure_open(&self) -> Result<(), ViewError> {
        lock(&self.meta).ensure_open()
    }
    fn fail(&self, failure: ViewFailure) -> Result<(), ViewError> {
        let mut meta = lock(&self.meta);
        meta.ensure_open()?;
        if !matches!(meta.state, ViewState::Incomplete(_)) {
            meta.advance()?;
            meta.state = ViewState::Incomplete(failure);
        }
        drop(meta);
        Ok(())
    }
}
struct Window {
    handle: ViewHandle,
    next_position: u64,
    evicted_through: Option<u64>,
    evicted_count: u64,
    // No retained capacity or index survives removal of one linked node.
    #[allow(
        clippy::linkedlist,
        reason = "Each evicted node must release capacity; bounded linear identity lookup needs no retained index"
    )]
    items: LinkedList<Arc<Item>>,
    // Keep the charged window header alive until all other fields have been destroyed.
    status: Arc<ReadStatusCell>,
}
impl Window {
    fn evict_front(&mut self) -> Result<bool, ViewError> {
        let Some(position) = self.items.front().map(|item| item.position) else {
            return Ok(false);
        };
        let Some(count) = self.evicted_count.checked_add(1) else {
            self.status.fail(ViewFailure::SequenceExhausted)?;
            return Err(ViewError::SequenceExhausted);
        };
        let old = {
            let mut meta = lock(&self.status.meta);
            meta.advance()?;
            let old = self.items.pop_front();
            self.evicted_through = Some(position);
            self.evicted_count = count;
            drop(meta);
            old
        };
        // LinkedList frees its node during pop; the charged Item only drops after Meta unlocks.
        drop(old);
        Ok(true)
    }
}

/// Fixed control identity borrowed from the caller's handle; holds no body or authority.
#[derive(Clone, Copy)]
pub struct WindowReadStamp<'h> {
    handle: &'h ViewHandle,
    revision: u64,
}
impl WindowReadStamp<'_> {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}
/// Synchronous body borrow with a frozen header. It cannot outlive its callback.
pub struct BorrowedWindow<'w, 'h> {
    window: &'w Window,
    status: WindowStatus,
    stamp: WindowReadStamp<'h>,
}
impl<'h> BorrowedWindow<'_, 'h> {
    #[must_use]
    pub const fn status(&self) -> WindowStatus {
        self.status
    }
    #[must_use]
    pub const fn stamp(&self) -> WindowReadStamp<'h> {
        self.stamp
    }
    #[must_use]
    pub fn items(&self) -> impl ExactSizeIterator<Item = ViewItem<'_>> {
        self.window.items.iter().map(|item| ViewItem {
            position: item.position,
            key: item.key.as_ref().map(|key| SourceKey {
                namespace: key.namespace,
                id: &key.id,
            }),
            bytes: &item.bytes,
        })
    }
}
struct Slot {
    target: NativeSessionKey,
    budget: Arc<TargetBudget>,
    window: Option<Window>,
}
/// One node instance. Disconnecting a reader does not mutate this registry.
pub struct ViewWindows {
    scope: Arc<()>,
    node: Arc<NodeBudget>,
    slots: Box<[Option<Box<Slot>>; MAX_IDENTITIES]>,
    next_generation: u64,
}
impl ViewWindows {
    /// Creates the fixed, charged registry. Explicit smaller limits support bounded deployments/tests.
    /// # Errors
    /// Rejects limits above the verified production ceilings or insufficient root room.
    pub fn new(limits: ViewLimits) -> Result<Self, ViewError> {
        let defaults = ViewLimits::default();
        if limits.target_bytes > defaults.target_bytes
            || limits.node_bytes > defaults.node_bytes
            || limits.target_items > defaults.target_items
            || limits.node_items > defaults.node_items
            || limits.node_bytes < ROOT_BYTES
            || limits.target_items == 0
            || limits.node_items == 0
        {
            return Err(ViewError::InvalidLimits);
        }
        Ok(Self {
            scope: Arc::new(()),
            node: Arc::new(NodeBudget {
                limits,
                ledger: Mutex::new(Ledger {
                    usage: ViewUsage {
                        bytes: ROOT_BYTES,
                        items: 0,
                    },
                    broken: false,
                }),
            }),
            slots: Box::new(std::array::from_fn(|_| None)),
            next_generation: 0,
        })
    }
    #[must_use]
    pub fn usage(&self) -> ViewUsage {
        lock(&self.node.ledger).usage
    }
    #[must_use]
    pub fn target_usage(&self, target: &NativeSessionKey) -> Option<ViewUsage> {
        self.slots
            .iter()
            .flatten()
            .find(|s| &s.target == target)
            .map(|s| lock(&s.budget.ledger).usage)
    }
    #[must_use]
    pub fn identity_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    /// Reclaims identities after their final old lease has gone. Begin/release also call this.
    pub fn collect_released(&mut self) {
        for slot in &mut *self.slots {
            if slot
                .as_ref()
                .is_some_and(|s| s.window.is_none() && Arc::strong_count(&s.budget) == 1)
            {
                *slot = None;
            }
        }
    }
    /// Starts a distinct disposable read; active reads require explicit invalidation first.
    /// # Errors
    /// Rejects concurrent reads, exhausted identities/generations or charged memory capacity.
    pub fn begin(
        &mut self,
        target: NativeSessionKey,
        resource: ResourceEpoch,
    ) -> Result<ViewHandle, ViewError> {
        self.collect_released();
        let existing = self
            .slots
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.target == target));
        if existing.is_some_and(|i| {
            self.slots[i]
                .as_ref()
                .and_then(|s| s.window.as_ref())
                .is_some_and(|w| w.status.state() == ViewState::Reading)
        }) {
            return Err(ViewError::Busy);
        }
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or(ViewError::SequenceExhausted)?;
        self.next_generation = generation;
        let index = if let Some(index) = existing {
            index
        } else {
            let index = self
                .slots
                .iter()
                .position(Option::is_none)
                .ok_or(ViewError::Capacity)?;
            let budget = TargetBudget::create(&self.node, target.native_id().len())?;
            self.slots[index] = Some(Box::new(Slot {
                target: target.clone(),
                budget,
                window: None,
            }));
            index
        };
        let slot = self.slots[index].as_mut().ok_or(ViewError::Stale)?;
        // Explicit refresh invalidates the prior window even if the new allocation fails.
        slot.window = None;
        let charge = slot.budget.reserve(WINDOW_BYTES, 0)?;
        let handle = ViewHandle {
            scope: self.scope.clone(),
            target,
            resource,
            generation,
        };
        slot.window = Some(Window {
            handle: handle.clone(),
            status: Arc::new(ReadStatusCell {
                meta: Mutex::new(ReadMeta {
                    state: ViewState::Reading,
                    revision: 0,
                    sealed: false,
                }),
                _charge: charge,
            }),
            next_position: 0,
            evicted_through: None,
            evicted_count: 0,
            items: LinkedList::new(),
        });
        Ok(handle)
    }
    fn slot(&self, handle: &ViewHandle) -> Result<&Slot, ViewError> {
        if !Arc::ptr_eq(&self.scope, &handle.scope) {
            return Err(ViewError::Stale);
        }
        self.slots
            .iter()
            .flatten()
            .find(|slot| {
                slot.target == handle.target
                    && slot.window.as_ref().is_some_and(|w| {
                        w.handle.generation == handle.generation
                            && w.handle.resource == handle.resource
                    })
            })
            .map(Box::as_ref)
            .ok_or(ViewError::Stale)
    }
    fn slot_mut(&mut self, handle: &ViewHandle) -> Result<&mut Slot, ViewError> {
        if !Arc::ptr_eq(&self.scope, &handle.scope) {
            return Err(ViewError::Stale);
        }
        self.slots
            .iter_mut()
            .flatten()
            .find(|slot| {
                slot.target == handle.target
                    && slot.window.as_ref().is_some_and(|w| {
                        w.handle.generation == handle.generation
                            && w.handle.resource == handle.resource
                    })
            })
            .map(Box::as_mut)
            .ok_or(ViewError::Stale)
    }
    /// Releases display ownership only. Old immutable leases remain charged until Drop.
    /// # Errors
    /// Rejects stale or foreign handles without changing the current window.
    pub fn release(&mut self, handle: &ViewHandle) -> Result<(), ViewError> {
        self.slot_mut(handle)?.window = None;
        self.collect_released();
        Ok(())
    }
    /// Supplies an owner-confirmed read completion, never an execution completion.
    /// # Errors
    /// Rejects old handles or a read that already failed.
    pub fn finish(&mut self, handle: &ViewHandle) -> Result<(), ViewError> {
        let window = self
            .slot_mut(handle)?
            .window
            .as_mut()
            .ok_or(ViewError::Stale)?;
        let mut meta = lock(&window.status.meta);
        meta.ensure_open()?;
        if matches!(meta.state, ViewState::Incomplete(_)) {
            return Err(ViewError::Incomplete);
        }
        if meta.state == ViewState::Reading {
            meta.advance()?;
            meta.state = ViewState::Finished;
        }
        drop(meta);
        Ok(())
    }
    /// Records the first read failure; later completion cannot erase its reason.
    /// # Errors
    /// Rejects a foreign or superseded read.
    pub fn fail(&mut self, handle: &ViewHandle, reason: ViewFailure) -> Result<(), ViewError> {
        let window = self
            .slot_mut(handle)?
            .window
            .as_mut()
            .ok_or(ViewError::Stale)?;
        window.status.fail(reason)
    }
    /// Inserts a new source entity, evicting this window's FIFO prefix when necessary.
    /// # Errors
    /// Rejects duplicate reliable keys, invalid keys, old handles and bounded capacity failures.
    pub fn insert(
        &mut self,
        handle: &ViewHandle,
        key: Option<SourceKey<'_>>,
        bytes: &[u8],
    ) -> Result<u64, ViewError> {
        let cost = item_cost(key, bytes.len())?;
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_mut().ok_or(ViewError::Stale)?;
        window.status.ensure_open()?;
        if key.is_some_and(|key| {
            window
                .items
                .iter()
                .any(|item| item.key.as_ref().is_some_and(|k| k.matches(key)))
        }) {
            return Err(ViewError::Conflict);
        }
        let Some(next) = window.next_position.checked_add(1) else {
            mark_failure(window, ViewFailure::SequenceExhausted)?;
            return Err(ViewError::SequenceExhausted);
        };
        if cost > slot.budget.node.limits.target_bytes {
            mark_failure(window, ViewFailure::Capacity)?;
            return Err(ViewError::Capacity);
        }
        let charge = loop {
            match slot.budget.reserve(cost, 1) {
                Ok(charge) => break charge,
                Err(error) => {
                    if !window.evict_front()? {
                        mark_failure(window, ViewFailure::Capacity)?;
                        return Err(error);
                    }
                }
            }
        };
        let item = match allocate_item(next, key, bytes, charge) {
            Ok(item) => item,
            Err(error) => {
                mark_failure(window, ViewFailure::Capacity)?;
                return Err(error);
            }
        };
        let mut prepared = LinkedList::new();
        prepared.push_back(Arc::new(item));
        {
            let mut meta = lock(&window.status.meta);
            meta.advance()?;
            window.items.append(&mut prepared);
            window.next_position = next;
            drop(meta);
        }
        Ok(next)
    }
    /// Replaces an already retained entity in place. Candidate and old value are charged together.
    /// # Errors
    /// Missing keys never become inserts; capacity failure preserves every existing item.
    pub fn replace(
        &mut self,
        handle: &ViewHandle,
        key: SourceKey<'_>,
        bytes: &[u8],
    ) -> Result<u64, ViewError> {
        let cost = item_cost(Some(key), bytes.len())?;
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_mut().ok_or(ViewError::Stale)?;
        window.status.ensure_open()?;
        let item = window
            .items
            .iter_mut()
            .find(|item| item.key.as_ref().is_some_and(|k| k.matches(key)))
            .ok_or(ViewError::MissingItem)?;
        let position = item.position;
        let replacement = slot
            .budget
            .reserve(cost, 1)
            .and_then(|charge| allocate_item(position, Some(key), bytes, charge));
        match replacement {
            Ok(replacement) => {
                let replacement = Arc::new(replacement);
                let old = {
                    let mut meta = lock(&window.status.meta);
                    meta.advance()?;
                    let old = std::mem::replace(item, replacement);
                    drop(meta);
                    old
                };
                drop(old);
                Ok(position)
            }
            Err(error) => {
                mark_failure(window, ViewFailure::Capacity)?;
                Err(error)
            }
        }
    }
    /// Freezes the retained window immediately. It is not an authorization check or output queue.
    /// # Errors
    /// Rejects old handles and leases that would exceed shared byte/reference budgets.
    pub fn snapshot(&mut self, handle: &ViewHandle) -> Result<ViewSnapshot, ViewError> {
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_ref().ok_or(ViewError::Stale)?;
        let count = window.items.len();
        let cost = count
            .checked_mul(REFERENCE_BYTES)
            .and_then(|n| n.checked_add(SNAPSHOT_BYTES))
            .ok_or(ViewError::Capacity)?;
        let charge = slot.budget.reserve(cost, count)?;
        let mut items = Vec::new();
        items
            .try_reserve_exact(count)
            .map_err(|_| ViewError::Capacity)?;
        items.extend(window.items.iter().cloned());
        Ok(ViewSnapshot {
            handle: window.handle.clone(),
            state: window.status.state(),
            evicted_through: window.evicted_through,
            evicted_count: window.evicted_count,
            items: items.into_boxed_slice(),
            _charge: charge,
        })
    }
    /// Reads without adding immutable references or allocating a snapshot.
    /// Only publish caller-owned output after this returns Ok; callbacks cannot be rolled back.
    /// # Errors
    /// Rejects stale handles, a sealed read or a state change during the callback.
    pub fn with_window<'h, R>(
        &self,
        handle: &'h ViewHandle,
        f: impl for<'w> FnOnce(BorrowedWindow<'w, 'h>) -> R,
    ) -> Result<R, ViewError> {
        let window = self.slot(handle)?.window.as_ref().ok_or(ViewError::Stale)?;
        let (state, revision) = {
            let meta = lock(&window.status.meta);
            meta.ensure_open()?;
            (meta.state, meta.revision)
        };
        let result = f(BorrowedWindow {
            window,
            status: WindowStatus::with_state(window, state),
            stamp: WindowReadStamp { handle, revision },
        });
        // This is the successful read's linearization point. Drop a rejected R outside the lock.
        let checked = {
            let meta = lock(&window.status.meta);
            meta.ensure_open().and_then(|()| {
                if meta.revision == revision {
                    Ok(())
                } else {
                    Err(ViewError::ReadChanged)
                }
            })
        };
        checked?;
        Ok(result)
    }
    /// Checks the complete read identity and its revision at one short comparison point.
    /// # Errors
    /// Rejects foreign/stale identities, sealed reads or a changed revision.
    pub fn validate_read_stamp(
        &self,
        handle: &ViewHandle,
        stamp: &WindowReadStamp<'_>,
    ) -> Result<(), ViewError> {
        let window = self.slot(handle)?.window.as_ref().ok_or(ViewError::Stale)?;
        if !staging::same_read(handle, stamp.handle) {
            return Err(ViewError::Stale);
        }
        let meta = lock(&window.status.meta);
        meta.ensure_open()?;
        if meta.revision == stamp.revision {
            Ok(())
        } else {
            Err(ViewError::ReadChanged)
        }
    }
}
impl Drop for ViewWindows {
    fn drop(&mut self) {
        // Root storage goes away now; old snapshot target/data charges independently outlive it.
        subtract(&mut lock(&self.node.ledger), ROOT_BYTES, 0);
    }
}
fn mark_failure(window: &Window, failure: ViewFailure) -> Result<(), ViewError> {
    window.status.fail(failure)
}
fn item_cost(key: Option<SourceKey<'_>>, size: usize) -> Result<usize, ViewError> {
    let key_size = key.map_or(0, |key| key.id.len());
    if key_size > 4096 {
        return Err(ViewError::InvalidKey);
    }
    ITEM_BYTES
        .checked_add(key_size)
        .and_then(|n| n.checked_add(size))
        .ok_or(ViewError::Capacity)
}
fn allocate_item(
    position: u64,
    key: Option<SourceKey<'_>>,
    bytes: &[u8],
    charge: Charge,
) -> Result<Item, ViewError> {
    let mut data = Vec::new();
    data.try_reserve_exact(bytes.len())
        .map_err(|_| ViewError::Capacity)?;
    data.extend_from_slice(bytes);
    let key = if let Some(key) = key {
        let mut id = String::new();
        id.try_reserve_exact(key.id.len())
            .map_err(|_| ViewError::Capacity)?;
        id.push_str(key.id);
        Some(StoredKey {
            namespace: key.namespace,
            id: id.into_boxed_str(),
        })
    } else {
        None
    };
    Ok(Item {
        position,
        key,
        bytes: data.into_boxed_slice(),
        _charge: charge,
    })
}

/// Non-Clone immutable lease. Borrowing bytes never exposes a clonable raw Arc.
pub struct ViewSnapshot {
    handle: ViewHandle,
    state: ViewState,
    evicted_through: Option<u64>,
    evicted_count: u64,
    items: Box<[Arc<Item>]>,
    _charge: Charge,
}
impl ViewSnapshot {
    #[must_use]
    pub const fn handle(&self) -> &ViewHandle {
        &self.handle
    }
    #[must_use]
    pub const fn state(&self) -> ViewState {
        self.state
    }
    #[must_use]
    pub const fn evicted_through(&self) -> Option<u64> {
        self.evicted_through
    }
    #[must_use]
    pub const fn evicted_count(&self) -> u64 {
        self.evicted_count
    }
    #[must_use]
    pub fn first_position(&self) -> Option<u64> {
        self.items.first().map(|item| item.position)
    }
    #[must_use]
    pub const fn history_complete(&self) -> bool {
        matches!(self.state, ViewState::Finished) && self.evicted_count == 0
    }
    #[must_use]
    pub fn items(&self) -> impl ExactSizeIterator<Item = ViewItem<'_>> {
        self.items.iter().map(|item| ViewItem {
            position: item.position,
            key: item.key.as_ref().map(|k| SourceKey {
                namespace: k.namespace,
                id: &k.id,
            }),
            bytes: &item.bytes,
        })
    }
}
pub struct ViewItem<'a> {
    pub position: u64,
    pub key: Option<SourceKey<'a>>,
    pub bytes: &'a [u8],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ViewError {
    #[error("view limits are outside the implemented budgets")]
    InvalidLimits,
    #[error("view handle is foreign or no longer current")]
    Stale,
    #[error("view changed while it was being read")]
    ReadChanged,
    #[error("view already has an active read")]
    Busy,
    #[error("view memory or reference capacity is exhausted")]
    Capacity,
    #[error("view source key is invalid")]
    InvalidKey,
    #[error("view source key already exists")]
    Conflict,
    #[error("view source item is no longer retained")]
    MissingItem,
    #[error("view read is incomplete")]
    Incomplete,
    #[error("view sequence is exhausted")]
    SequenceExhausted,
    #[error("view candidate write exceeds its reserved length")]
    WriteTooLong,
    #[error("view candidate has not been completely written")]
    ShortWrite,
    #[error("view candidate failed and cannot be published")]
    PoisonedStage,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[derive(Clone, Copy)]
enum RevisionBoundary {
    Penultimate,
    Last,
}
#[cfg(test)]
impl ViewWindows {
    fn set_revision_boundary_for_test(
        &mut self,
        handle: &ViewHandle,
        boundary: RevisionBoundary,
    ) -> Result<(), ViewError> {
        let window = self
            .slot_mut(handle)?
            .window
            .as_mut()
            .ok_or(ViewError::Stale)?;
        let mut meta = lock(&window.status.meta);
        meta.ensure_open()?;
        let revision = match boundary {
            RevisionBoundary::Penultimate => u64::MAX - 1,
            RevisionBoundary::Last => u64::MAX,
        };
        if revision < meta.revision {
            return Err(ViewError::Conflict);
        }
        meta.revision = revision;
        drop(meta);
        Ok(())
    }
}

#[cfg(test)]
mod borrow_tests;
