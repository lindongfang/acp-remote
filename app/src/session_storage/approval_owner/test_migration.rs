//! Typed visits from the same real schema migration body, without a registry or extra pool.
use super::test_control::ApprovalControlError;
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ApprovalDdlOrdinal(u8);
impl ApprovalDdlOrdinal {
    pub(crate) const fn new(value: u8) -> Result<Self, ApprovalControlError> {
        if value >= 1 && value <= 12 {
            Ok(Self(value))
        } else {
            Err(ApprovalControlError::InvalidPoint)
        }
    }
    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApprovalDdlEdge {
    Before,
    After,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApprovalMigrationFault {
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
pub(crate) struct ApprovalMigrationVisit {
    pub ordinal: ApprovalDdlOrdinal,
    pub edge: ApprovalDdlEdge,
    pub actual_ddl_succeeded: bool,
}
pub(crate) struct ApprovalMigrationTrace {
    entries: Mutex<Vec<ApprovalMigrationVisit>>,
}
impl ApprovalMigrationTrace {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(Vec::new()),
        })
    }
    pub(crate) fn entries(&self) -> Vec<ApprovalMigrationVisit> {
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
