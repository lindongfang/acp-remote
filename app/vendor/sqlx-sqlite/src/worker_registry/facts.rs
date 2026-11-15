//! Facts from the original worker; no database contents or panic payloads.
use std::{fmt, sync::Arc};

#[derive(Clone)]
pub struct WorkerScopeId(pub(super) Arc<()>);
impl PartialEq for WorkerScopeId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for WorkerScopeId {}
impl fmt::Debug for WorkerScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkerScopeId(opaque)")
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerId {
    pub(super) scope: WorkerScopeId,
    pub(super) ordinal: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerSpawn {
    Reserved,
    Installed,
    SpawnFailedNoThread,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerHandleState {
    NotInstalled,
    VendorOwned,
    Joining,
    Joined,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerJoinReport {
    Ok,
    WorkerPanicked,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerExitPath {
    Unknown,
    EstablishFailed,
    EstablishUndelivered,
    CommandChannelClosed,
    CancelledBeginRollbackFailed,
    ShutdownBranch,
    StartGateClosed,
    WorkerUnwinding,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerShutdownFacts {
    pub requested: bool,
    pub command_send_failed: bool,
    pub ack_send_attempted: bool,
    pub ack_enqueued: bool,
    pub ack_receiver_gone: bool,
    pub ack_observed: bool,
    pub ack_receive_failed: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerCloseFacts {
    pub graceful_started: bool,
    pub optimize_failed: bool,
    pub shutdown_error: bool,
    pub graceful_returned_ok: bool,
    pub hard_drop_requested: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerSnapshot {
    pub id: WorkerId,
    pub reservation_generation: u64,
    pub spawn: WorkerSpawn,
    pub spawn_error_kind: Option<std::io::ErrorKind>,
    pub handle: WorkerHandleState,
    pub join_report: Option<WorkerJoinReport>,
    /// An actual join occurred but its terminal writeback could not be confirmed.
    pub unconfirmed_join_report: Option<WorkerJoinReport>,
    pub exit_path: WorkerExitPath,
    pub shutdown: WorkerShutdownFacts,
    pub close: WorkerCloseFacts,
    pub unknown: bool,
    /// A scheduling hint; this is never a join receipt.
    pub thread_is_finished: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrySnapshot {
    pub scope: WorkerScopeId,
    pub generation: u64,
    pub sealed: bool,
    pub finalized: bool,
    pub unknown: bool,
    pub workers: Vec<WorkerSnapshot>,
}
impl RegistrySnapshot {
    pub fn all_threads_terminated(&self) -> bool {
        self.sealed
            && !self.unknown
            && self.workers.iter().all(|w| {
                !w.unknown
                    && (w.spawn == WorkerSpawn::SpawnFailedNoThread
                        || (w.spawn == WorkerSpawn::Installed
                            && w.handle == WorkerHandleState::Joined
                            && w.join_report.is_some()))
            })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerRegistryError {
    Sealed,
    WrongScope,
    UnknownWorker,
    SpawnPending,
    AlreadyJoining,
    StateUnavailable,
    WouldJoinSelf,
    NotSealed,
    Incomplete,
    InvariantViolation,
}
impl fmt::Display for WorkerRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sealed => "SQLite worker scope is sealed",
            Self::WrongScope => "SQLite worker belongs to another scope",
            Self::UnknownWorker => "SQLite worker is not registered",
            Self::SpawnPending => "SQLite worker spawn has not settled",
            Self::AlreadyJoining => "SQLite worker join is already owned",
            Self::StateUnavailable => "SQLite worker state is unconfirmed",
            Self::WouldJoinSelf => "SQLite worker cannot join itself",
            Self::NotSealed => "SQLite worker scope has not been sealed",
            Self::Incomplete => "SQLite worker scope is not physically terminated",
            Self::InvariantViolation => "SQLite worker ownership invariant failed",
        })
    }
}
impl std::error::Error for WorkerRegistryError {}

pub(crate) enum Fact {
    Exit(WorkerExitPath),
    ShutdownRequested,
    ShutdownSendFailed,
    AckAttempted,
    AckEnqueued,
    AckReceiverGone,
    AckObserved,
    AckReceiveFailed,
    GracefulStarted,
    OptimizeFailed,
    ShutdownError,
    GracefulReturnedOk,
    HardDropRequested,
}
