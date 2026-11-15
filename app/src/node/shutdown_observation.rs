//! Default-off, per-Node static observations. These facts confer no control or ownership.
#![allow(
    dead_code,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeShutdownStage {
    Pairing,
    Network,
    Management,
    Storage,
    DataDirectory,
    OwnerIpcTask,
    StopResponses,
}
impl NodeShutdownStage {
    const fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeStageOutcome {
    NotReached,
    Pending,
    Absent,
    Completed,
    ReturnedError,
    JoinError,
    Deadline,
    RegistryUnconfirmed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeShutdownPhase {
    NotEntered,
    Entered,
    Stage(NodeShutdownStage),
    DependencySlotsDropped,
    IdentitySlotDropped,
    ReportPublished,
    FinalResultSelected,
    ObservationUnavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeShutdownDisposition {
    Ok,
    Error,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeShutdownFailure {
    InitialFailure,
    InitialControlFailure,
    PairingDeadline,
    NetworkReturnedError,
    NetworkJoinError,
    NetworkDeadline,
    ManagementDeadline,
    StorageDeadline,
    RegistryUnconfirmed,
    DataDirectoryUnlockError,
    LateControlFailureAdded,
    OwnerIpcReturnedError,
    OwnerIpcJoinError,
    OwnerIpcDeadline,
    StopResponsesReturnedError,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeShutdownFailureFlags(u16);
impl NodeShutdownFailureFlags {
    #[must_use]
    pub const fn contains(self, kind: NodeShutdownFailure) -> bool {
        self.0 & (1_u16 << kind as u8) != 0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeShutdownSnapshot {
    revision: u64,
    phase: NodeShutdownPhase,
    outcomes: [NodeStageOutcome; 7],
    failures: NodeShutdownFailureFlags,
    disposition: Option<NodeShutdownDisposition>,
}
impl NodeShutdownSnapshot {
    const fn initial() -> Self {
        Self {
            revision: 0,
            phase: NodeShutdownPhase::NotEntered,
            outcomes: [NodeStageOutcome::NotReached; 7],
            failures: NodeShutdownFailureFlags(0),
            disposition: None,
        }
    }
    const fn unavailable() -> Self {
        Self {
            phase: NodeShutdownPhase::ObservationUnavailable,
            ..Self::initial()
        }
    }
    #[must_use]
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    #[must_use]
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    pub const fn phase(self) -> NodeShutdownPhase {
        self.phase
    }
    #[must_use]
    pub const fn outcome(self, stage: NodeShutdownStage) -> NodeStageOutcome {
        self.outcomes[stage.index()]
    }
    #[must_use]
    pub const fn failures(self) -> NodeShutdownFailureFlags {
        self.failures
    }
    #[must_use]
    #[cfg(feature = "shared-view-acceptance-fixtures")]
    pub const fn disposition(self) -> Option<NodeShutdownDisposition> {
        self.disposition
    }
}

pub(super) struct NodeShutdownObserver {
    state: Arc<Mutex<NodeShutdownSnapshot>>,
}
pub(super) struct NodeShutdownProbe {
    state: Arc<Mutex<NodeShutdownSnapshot>>,
}
impl NodeShutdownObserver {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NodeShutdownSnapshot::initial())),
        }
    }
    pub(super) fn probe(&self) -> NodeShutdownProbe {
        NodeShutdownProbe {
            state: Arc::clone(&self.state),
        }
    }
    fn update(&self, apply: impl FnOnce(&mut NodeShutdownSnapshot)) {
        if let Ok(mut state) = self.state.lock() {
            apply(&mut state);
            state.revision = state.revision.saturating_add(1);
        }
    }
    pub(super) fn phase(&self, phase: NodeShutdownPhase) {
        self.update(|s| s.phase = phase);
    }
    pub(super) fn enter(&self, stage: NodeShutdownStage, present: bool) {
        self.update(|s| {
            s.phase = NodeShutdownPhase::Stage(stage);
            s.outcomes[stage.index()] = if present {
                NodeStageOutcome::Pending
            } else {
                NodeStageOutcome::Absent
            };
        });
    }
    pub(super) fn finish_if_pending(&self, stage: NodeShutdownStage) {
        self.update(|s| {
            if s.outcomes[stage.index()] == NodeStageOutcome::Pending {
                s.outcomes[stage.index()] = NodeStageOutcome::Completed;
            }
        });
    }
    pub(super) fn flag(&self, kind: NodeShutdownFailure) {
        self.update(|s| s.failures.0 |= 1_u16 << kind as u8);
    }
    pub(super) fn fail(
        &self,
        stage: NodeShutdownStage,
        outcome: NodeStageOutcome,
        kind: NodeShutdownFailure,
    ) {
        self.update(|s| {
            s.outcomes[stage.index()] = outcome;
            s.failures.0 |= 1_u16 << kind as u8;
        });
    }
    pub(super) fn select_result(&self, disposition: NodeShutdownDisposition) {
        self.update(|s| {
            s.phase = NodeShutdownPhase::FinalResultSelected;
            s.disposition = Some(disposition);
        });
    }
}
impl NodeShutdownProbe {
    pub(super) fn snapshot(&self) -> NodeShutdownSnapshot {
        self.state
            .lock()
            .map_or_else(|_| NodeShutdownSnapshot::unavailable(), |s| *s)
    }
}
