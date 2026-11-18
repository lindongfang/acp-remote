//! Typed observations used for automatic request decisions; raw protocol data stays retained.
use super::{Arc, ObservationOrigin};
use crate::domain::session_capacity::Coverage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapacityState {
    pub foreground: Coverage,
    /// None means not reported. It does not erase a previous active snapshot.
    pub background: Option<Coverage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapacityUpdate {
    State(CapacityState),
    Tool {
        id: Arc<str>,
        /// Missing or malformed status cannot clear an earlier active tool.
        /// Unknown status strings are still active; a new omitted tool starts pending.
        active: Option<bool>,
    },
    Child {
        session: Arc<str>,
        /// Omitted, explicit null, or a concrete current state snapshot.
        state: Option<Option<CapacityState>>,
    },
    Activity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapacityFreshness {
    Current,
    History,
    Unconfirmed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapacityObservation {
    pub session: Arc<str>,
    pub origin: ObservationOrigin,
    pub freshness: CapacityFreshness,
    pub update: CapacityUpdate,
}
