//! Charged, move-only construction; no protocol interpretation or execution authority.
use super::{
    Arc, Charge, Item, ReadStatusCell, SourceKey, StoredKey, ViewError, ViewFailure, ViewHandle,
    ViewItem, ViewState, ViewWindows, WINDOW_BYTES, Window, item_cost,
};

const LEASE_BYTES: usize = 320;
const WORKSPACE_HEADER_BYTES: usize = 512;

/// Allocation-free observation, available even when another snapshot cannot be reserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowStatus {
    pub state: ViewState,
    pub generation: u64,
    pub first_position: Option<u64>,
    pub last_position: Option<u64>,
    /// Position the next successful insertion would receive; None means exhausted.
    pub next_position: Option<u64>,
    pub evicted_through: Option<u64>,
    pub evicted_count: u64,
    pub retained_items: usize,
}
impl WindowStatus {
    fn read(window: &Window) -> Self {
        Self::with_state(window, window.status.state())
    }
    pub(super) fn with_state(window: &Window, state: ViewState) -> Self {
        Self {
            state,
            generation: window.handle.generation,
            first_position: window.items.front().map(|item| item.position),
            last_position: window.items.back().map(|item| item.position),
            next_position: window.next_position.checked_add(1),
            evicted_through: window.evicted_through,
            evicted_count: window.evicted_count,
            retained_items: window.items.len(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishFact {
    pub position: u64,
    pub replaced: bool,
    pub status: WindowStatus,
}

/// One charged immutable reference. An obsolete lease remains readable, but cannot publish.
pub struct ItemLease {
    handle: ViewHandle,
    item: Arc<Item>,
    _charge: Charge,
}
impl ItemLease {
    #[must_use]
    pub fn item(&self) -> ViewItem<'_> {
        ViewItem {
            position: self.item.position,
            key: self.item.key.as_ref().map(|key| SourceKey {
                namespace: key.namespace,
                id: &key.id,
            }),
            bytes: &self.item.bytes,
        }
    }
}

/// The header and exact scratch capacity are charged before either allocation is owned.
pub struct StageWorkspace {
    handle: ViewHandle,
    scratch: Box<[u8]>,
    status: Arc<ReadStatusCell>,
    charge: Charge,
}
impl StageWorkspace {
    #[must_use]
    pub fn scratch(&mut self) -> &mut [u8] {
        &mut self.scratch
    }
}

/// Full-or-zero writes into already allocated space. A failed write permanently poisons the stage.
pub struct BoundedWriter<'a> {
    bytes: &'a mut [u8],
    written: &'a mut usize,
    poisoned: &'a mut bool,
    status: &'a ReadStatusCell,
}
impl BoundedWriter<'_> {
    /// # Errors
    /// Rejects overlong writes or a previously failed candidate without truncating the input.
    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), ViewError> {
        self.status.ensure_open()?;
        if *self.poisoned {
            return Err(ViewError::PoisonedStage);
        }
        let Some(end) = self.written.checked_add(bytes.len()) else {
            return self.overrun();
        };
        let Some(destination) = self.bytes.get_mut(*self.written..end) else {
            return self.overrun();
        };
        destination.copy_from_slice(bytes);
        *self.written = end;
        Ok(())
    }
    fn overrun(&mut self) -> Result<(), ViewError> {
        *self.poisoned = true;
        self.status.fail(ViewFailure::Structure)?;
        Err(ViewError::WriteTooLong)
    }
    #[must_use]
    pub const fn written(&self) -> usize {
        *self.written
    }
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(*self.written)
    }
}
/// Disjoint borrowed fields permit scanning the old value while writing its replacement.
pub struct StageParts<'a> {
    pub scratch: &'a mut [u8],
    pub old: Option<ViewItem<'a>>,
    pub writer: BoundedWriter<'a>,
}
/// Not visible until a complete, current candidate is consumed by publish.
pub struct StagedItem {
    old: Option<ItemLease>,
    key: Option<StoredKey>,
    data: Vec<u8>,
    charge: Option<Charge>,
    written: usize,
    poisoned: bool,
    published: bool,
    // Its header charge also covers this builder; retire it after the candidate and old lease.
    work: StageWorkspace,
}
impl StagedItem {
    #[must_use]
    pub fn parts(&mut self) -> StageParts<'_> {
        StageParts {
            scratch: &mut self.work.scratch,
            old: self.old.as_ref().map(ItemLease::item),
            writer: BoundedWriter {
                bytes: &mut self.data,
                written: &mut self.written,
                poisoned: &mut self.poisoned,
                status: &self.work.status,
            },
        }
    }
}
impl Drop for StagedItem {
    fn drop(&mut self) {
        if !self.published {
            let _ = self.work.status.fail(ViewFailure::Structure);
        }
    }
}

pub(super) fn same_read(a: &ViewHandle, b: &ViewHandle) -> bool {
    Arc::ptr_eq(&a.scope, &b.scope)
        && a.target == b.target
        && a.resource == b.resource
        && a.generation == b.generation
}
fn contains_key(window: &Window, key: SourceKey<'_>) -> bool {
    window
        .items
        .iter()
        .any(|item| item.key.as_ref().is_some_and(|k| k.matches(key)))
}
fn contains_lease(window: &Window, old: &ItemLease) -> bool {
    window.items.iter().any(|item| Arc::ptr_eq(item, &old.item))
}

// The pinned std's grow_exact stores the requested capacity. Filling that exact capacity before
// into_boxed_slice avoids shrink allocation. Layout/capacity evidence lives with the stage tests.
fn zeroed_exact(size: usize) -> Result<Vec<u8>, ViewError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| ViewError::Capacity)?;
    if bytes.capacity() != size {
        return Err(ViewError::Capacity);
    }
    bytes.resize(size, 0);
    Ok(bytes)
}
fn copy_key(key: Option<SourceKey<'_>>) -> Result<Option<StoredKey>, ViewError> {
    key.map(|key| {
        let mut id = String::new();
        id.try_reserve_exact(key.id.len())
            .map_err(|_| ViewError::Capacity)?;
        if id.capacity() != key.id.len() {
            return Err(ViewError::Capacity);
        }
        id.push_str(key.id);
        Ok(StoredKey {
            namespace: key.namespace,
            id: id.into_boxed_str(),
        })
    })
    .transpose()
}
fn build(
    work: StageWorkspace,
    old: Option<ItemLease>,
    key: Option<SourceKey<'_>>,
    size: usize,
    charge: Charge,
) -> Result<StagedItem, ViewError> {
    let allocation = zeroed_exact(size).and_then(|data| copy_key(key).map(|key| (data, key)));
    match allocation {
        Ok((data, key)) => Ok(StagedItem {
            work,
            old,
            key,
            data,
            charge: Some(charge),
            written: 0,
            poisoned: false,
            published: false,
        }),
        Err(error) => {
            work.status.fail(ViewFailure::Capacity)?;
            Err(error)
        }
    }
}

impl ViewWindows {
    /// Returns only fixed-size facts and allocates no snapshot or new reference.
    /// # Errors
    /// Rejects foreign or superseded read handles.
    pub fn window_status(&self, handle: &ViewHandle) -> Result<WindowStatus, ViewError> {
        self.slot(handle)?
            .window
            .as_ref()
            .map(WindowStatus::read)
            .ok_or(ViewError::Stale)
    }
    /// Borrows exactly one retained item, reserving its 320-byte/one-reference lease first.
    /// # Errors
    /// Rejects stale/invalid/missing keys and shared capacity exhaustion.
    pub fn item_lease(
        &mut self,
        handle: &ViewHandle,
        key: SourceKey<'_>,
    ) -> Result<ItemLease, ViewError> {
        item_cost(Some(key), 0)?;
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_ref().ok_or(ViewError::Stale)?;
        let item = window
            .items
            .iter()
            .find(|item| item.key.as_ref().is_some_and(|k| k.matches(key)))
            .ok_or(ViewError::MissingItem)?;
        let charge = slot.budget.reserve(LEASE_BYTES, 1)?;
        Ok(ItemLease {
            handle: handle.clone(),
            item: item.clone(),
            _charge: charge,
        })
    }
    /// Reserves a 512-byte header plus an explicit exact scratch capacity.
    /// # Errors
    /// Rejects stale handles, overflow or unavailable memory; capacity errors mark this read.
    pub fn stage_workspace(
        &mut self,
        handle: &ViewHandle,
        scratch_bytes: usize,
    ) -> Result<StageWorkspace, ViewError> {
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_ref().ok_or(ViewError::Stale)?;
        window.status.ensure_open()?;
        let allocation = WORKSPACE_HEADER_BYTES
            .checked_add(scratch_bytes)
            .ok_or(ViewError::Capacity)
            .and_then(|cost| slot.budget.reserve(cost, 0))
            .and_then(|charge| zeroed_exact(scratch_bytes).map(|scratch| (charge, scratch)));
        match allocation {
            Ok((charge, scratch)) => {
                window.status.ensure_open()?;
                Ok(StageWorkspace {
                    handle: handle.clone(),
                    scratch: scratch.into_boxed_slice(),
                    status: window.status.clone(),
                    charge,
                })
            }
            Err(error) => {
                window.status.fail(ViewFailure::Capacity)?;
                Err(error)
            }
        }
    }
    /// Reserves a new candidate, evicting only this read's FIFO prefix if necessary.
    /// # Errors
    /// Rejects stale work, invalid/duplicate keys and capacity. Status preserves all actual evictions.
    pub fn stage_insert(
        &mut self,
        handle: &ViewHandle,
        work: StageWorkspace,
        key: Option<SourceKey<'_>>,
        payload_bytes: usize,
    ) -> Result<StagedItem, ViewError> {
        if !same_read(handle, &work.handle) {
            return Err(ViewError::Stale);
        }
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_mut().ok_or(ViewError::Stale)?;
        if key.is_some_and(|key| key.id.len() > 4096) {
            // Preserve parameter priority without changing a read that was already sealed.
            if work.status.ensure_open().is_ok() {
                work.status.fail(ViewFailure::Structure)?;
            }
            return Err(ViewError::InvalidKey);
        }
        window.status.ensure_open()?;
        let cost = match item_cost(key, payload_bytes) {
            Ok(cost) => cost,
            Err(error) => {
                work.status.fail(if error == ViewError::Capacity {
                    ViewFailure::Capacity
                } else {
                    ViewFailure::Structure
                })?;
                return Err(error);
            }
        };
        if key.is_some_and(|key| contains_key(window, key)) {
            work.status.fail(ViewFailure::Structure)?;
            return Err(ViewError::Conflict);
        }
        // This irreducible baseline cannot be reclaimed by evicting a prefix.
        let minimum = slot
            .budget
            .identity_bytes
            .checked_add(WINDOW_BYTES)
            .and_then(|n| n.checked_add(work.charge.bytes))
            .and_then(|n| n.checked_add(cost));
        if minimum.is_none_or(|n| n > slot.budget.node.limits.target_bytes) {
            work.status.fail(ViewFailure::Capacity)?;
            return Err(ViewError::Capacity);
        }
        let charge = loop {
            match slot.budget.reserve(cost, 1) {
                Ok(charge) => break charge,
                Err(error) => {
                    if !window.evict_front()? {
                        work.status.fail(ViewFailure::Capacity)?;
                        return Err(error);
                    }
                }
            }
        };
        build(work, None, key, payload_bytes, charge)
    }
    /// Consumes the paid old lease; replacement never evicts or releases the old object early.
    /// # Errors
    /// Rejects stale work/leases, a changed item or insufficient candidate peak capacity.
    pub fn stage_replace(
        &mut self,
        handle: &ViewHandle,
        work: StageWorkspace,
        old: ItemLease,
        payload_bytes: usize,
    ) -> Result<StagedItem, ViewError> {
        if !same_read(handle, &work.handle) || !same_read(handle, &old.handle) {
            return Err(ViewError::Stale);
        }
        let slot = self.slot_mut(handle)?;
        let window = slot.window.as_ref().ok_or(ViewError::Stale)?;
        window.status.ensure_open()?;
        if !contains_lease(window, &old) {
            work.status.fail(ViewFailure::Structure)?;
            return Err(ViewError::Conflict);
        }
        let key = old.item.key.as_ref().map(|key| SourceKey {
            namespace: key.namespace,
            id: key.id.as_ref(),
        });
        let candidate = item_cost(key, payload_bytes).and_then(|cost| slot.budget.reserve(cost, 1));
        let charge = match candidate {
            Ok(charge) => charge,
            Err(error) => {
                work.status.fail(ViewFailure::Capacity)?;
                return Err(error);
            }
        };
        // Copy the borrowed key before moving the single old lease; no second Arc reference.
        let allocation =
            zeroed_exact(payload_bytes).and_then(|data| copy_key(key).map(|key| (data, key)));
        match allocation {
            Ok((data, key)) => Ok(StagedItem {
                work,
                old: Some(old),
                key,
                data,
                charge: Some(charge),
                written: 0,
                poisoned: false,
                published: false,
            }),
            Err(error) => {
                work.status.fail(ViewFailure::Capacity)?;
                Err(error)
            }
        }
    }
    /// Publishes by moving the candidate bytes and charge, checking the original item again.
    /// # Errors
    /// Rejects old handles, poison/short writes, duplicate keys, changed items or sequence exhaustion.
    pub fn publish(
        &mut self,
        handle: &ViewHandle,
        mut staged: StagedItem,
    ) -> Result<PublishFact, ViewError> {
        if !same_read(handle, &staged.work.handle) {
            return Err(ViewError::Stale);
        }
        let window = self
            .slot_mut(handle)?
            .window
            .as_mut()
            .ok_or(ViewError::Stale)?;
        window.status.ensure_open()?;
        if staged.poisoned {
            return Err(ViewError::PoisonedStage);
        }
        if staged.written != staged.data.len() {
            return Err(ViewError::ShortWrite);
        }
        let replaced = staged.old.is_some();
        let position = if let Some(old) = &staged.old {
            if !contains_lease(window, old) {
                return Err(ViewError::Conflict);
            }
            old.item.position
        } else {
            if staged.key.as_ref().is_some_and(|key| {
                contains_key(
                    window,
                    SourceKey {
                        namespace: key.namespace,
                        id: &key.id,
                    },
                )
            }) {
                return Err(ViewError::Conflict);
            }
            let Some(position) = window.next_position.checked_add(1) else {
                staged.work.status.fail(ViewFailure::SequenceExhausted)?;
                return Err(ViewError::SequenceExhausted);
            };
            position
        };
        let charge = staged.charge.take().ok_or(ViewError::PoisonedStage)?;
        let replacement = Arc::new(Item {
            position,
            key: staged.key.take(),
            bytes: std::mem::take(&mut staged.data).into_boxed_slice(),
            _charge: charge,
        });
        let status = if let Some(old) = &staged.old {
            let current = window
                .items
                .iter_mut()
                .find(|item| Arc::ptr_eq(item, &old.item))
                .ok_or(ViewError::Conflict)?;
            let previous = {
                let mut meta = super::lock(&window.status.meta);
                meta.advance()?;
                // The replacement Arc was prepared outside the critical section.
                let previous = std::mem::replace(current, replacement);
                drop(meta);
                previous
            };
            drop(previous);
            WindowStatus::read(window)
        } else {
            let mut prepared = std::collections::LinkedList::new();
            prepared.push_back(replacement);
            let mut meta = super::lock(&window.status.meta);
            meta.advance()?;
            window.items.append(&mut prepared);
            window.next_position = position;
            WindowStatus::with_state(window, meta.state)
        };
        staged.published = true;
        Ok(PublishFact {
            position,
            replaced,
            status,
        })
    }
    /// Explicitly abandons a candidate, reporting only to its original read even after invalidation.
    /// # Errors
    /// Returns Stale after consuming foreign/obsolete work; it cannot affect the newer read.
    pub fn abort_stage(
        &mut self,
        handle: &ViewHandle,
        staged: StagedItem,
        failure: ViewFailure,
    ) -> Result<(), ViewError> {
        let current_scope = same_read(handle, &staged.work.handle);
        let failed = staged.work.status.fail(failure);
        drop(staged);
        if !current_scope {
            return Err(ViewError::Stale);
        }
        self.slot(handle)?;
        failed
    }
}

#[cfg(test)]
mod tests;
