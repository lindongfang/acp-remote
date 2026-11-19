//! Typed visits from the same real schema migration body, without a registry or extra pool.
#![allow(
    dead_code,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "Test-only approval migration fault variants"
)]
use super::test_control::ApprovalControlError;
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalDdlOrdinal(u8);
impl ApprovalDdlOrdinal {
    pub const fn new(value: u8) -> Result<Self, ApprovalControlError> {
        if value >= 1 && value <= 12 {
            Ok(Self(value))
        } else {
            Err(ApprovalControlError::InvalidPoint)
        }
    }
    pub const fn get(self) -> u8 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalDdlEdge {
    Before,
    After,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApprovalMigrationFault {
    Ddl {
        ordinal: ApprovalDdlOrdinal,
        edge: ApprovalDdlEdge,
    },
    Metadata,
    Pragma,
    DeferredCommit,
    AfterCommitReportLost,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApprovalMigrationVisit {
    pub ordinal: ApprovalDdlOrdinal,
    pub edge: ApprovalDdlEdge,
    pub actual_ddl_succeeded: bool,
}
pub struct ApprovalMigrationTrace {
    entries: Mutex<Vec<ApprovalMigrationVisit>>,
}
impl ApprovalMigrationTrace {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(Vec::new()),
        })
    }
    pub fn entries(&self) -> Vec<ApprovalMigrationVisit> {
        self.entries
            .lock()
            .map_or_else(|_| Vec::new(), |value| value.clone())
    }
    pub(in crate::session_storage) fn record(
        &self,
        ordinal: u8,
        edge: ApprovalDdlEdge,
    ) -> Result<(), crate::error::StorageError> {
        let ordinal = ApprovalDdlOrdinal::new(ordinal)
            .map_err(|_| crate::error::StorageError::MigrationInterrupted)?;
        self.entries
            .lock()
            .map_err(|_| crate::error::StorageError::MigrationInterrupted)?
            .push(ApprovalMigrationVisit {
                ordinal,
                edge,
                actual_ddl_succeeded: edge == ApprovalDdlEdge::After,
            });
        Ok(())
    }
}
