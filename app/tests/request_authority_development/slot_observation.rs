//! Test-only read view of the actual registry owner's existing slots; no slot construction.
use super::{CloseOutcome, CloseSlot, ConnectionOwner};
use std::sync::{Arc, atomic::Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotOutcome {
    Pending,
    Confirmed,
    Unconfirmed,
}

pub struct SlotObservation(Arc<CloseSlot>);
impl SlotObservation {
    pub(crate) fn same_slot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn stored(&self) -> SlotOutcome {
        match self.0.outcome.load(Ordering::Acquire) {
            1 => SlotOutcome::Confirmed,
            2 => SlotOutcome::Unconfirmed,
            _ => SlotOutcome::Pending,
        }
    }
    pub(crate) async fn wait(&self) -> SlotOutcome {
        match self.0.wait().await {
            CloseOutcome::Confirmed => SlotOutcome::Confirmed,
            CloseOutcome::Unconfirmed => SlotOutcome::Unconfirmed,
        }
    }
}
pub struct OwnerObservation {
    pub(crate) sealed: bool,
    pub(crate) leases: usize,
    pub(crate) slots: Vec<SlotObservation>,
}
pub(in crate::session_storage) fn capture(owner: &Arc<ConnectionOwner>) -> OwnerObservation {
    let state = owner.state.lock().expect("self-owned owner state");
    OwnerObservation {
        sealed: state.sealed,
        leases: state.leases,
        slots: state
            .slots
            .iter()
            .map(|slot| SlotObservation(Arc::clone(slot)))
            .collect(),
    }
}
