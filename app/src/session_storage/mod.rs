//! Management-only storage. Historical body writers are compiled only in tests.
#[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
pub(crate) mod approval_owner;
#[cfg(not(all(test, feature = "approval-owner-acceptance-fixtures")))]
mod approval_owner;
mod approval_owner_schema;
mod catalog_sql;
#[cfg(test)]
mod legacy;
pub(crate) mod management_audit;
mod management_audit_budget;
mod management_audit_schema;
#[cfg(windows)]
pub mod native_configuration;
pub mod native_dispatch;
mod native_dispatch_schema;
#[cfg(test)]
pub mod native_metadata_test_hooks;
pub(crate) mod native_release;
mod native_release_schema;
mod node_catalog_schema;
#[cfg(windows)]
mod node_configuration;
#[cfg(windows)]
pub(crate) use node_configuration::SqliteNodeConfiguration;
#[cfg(test)]
pub mod product_commands;
mod product_schema;
mod registration_schema;
mod registry_connection_owner;
#[allow(
    clippy::needless_raw_string_hashes,
    reason = "Generated schema SQL literals remain untouched outside this wrapper module"
)]
mod source_sessions_schema;
pub(crate) use registry_connection_owner::ConnectionCloseStatus;
pub mod request_registry;
pub(crate) mod schema;
pub(crate) mod sqlite_lifecycle;
#[cfg(test)]
pub use legacy::{SessionStore, approvals, history};

#[cfg(windows)]
pub(crate) fn node_catalog_repository(
    registry: request_registry::SqliteRequestRegistry,
    protector: std::sync::Arc<dyn crate::platform::SecretProtector>,
    node_id: String,
    public_key: Vec<u8>,
    metadata_wait: std::time::Duration,
) -> Result<SqliteNodeConfiguration, crate::domain::native_catalog::CatalogError> {
    node_configuration::from_node(registry, protector, node_id, public_key, metadata_wait)
}

// Shared only inside this crate: both Trust and registry retain the same physical owner.
pub(crate) struct RegistryConnection {
    connection: Option<sqlx::SqliteConnection>,
    lease: registry_connection_owner::ConnectionLease,
    trust_scope: bool,
    #[cfg(test)]
    physical_close: Option<std::sync::Arc<PhysicalCloseLink>>,
    #[cfg(all(test, feature = "approval-owner-acceptance-fixtures"))]
    approval_context: Option<Box<approval_owner::test_control::ApprovalConnectionContext>>,
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct AcquisitionObservation {
    pool_first_pending: std::sync::atomic::AtomicBool,
    pool_pending_changed: tokio::sync::Notify,
    transferred: std::sync::atomic::AtomicBool,
    joined: std::sync::atomic::AtomicBool,
    physical_close: std::sync::Mutex<Option<std::sync::Arc<PhysicalCloseLink>>>,
}
#[cfg(test)]
#[derive(Default)]
struct PhysicalCloseLink {
    close: std::sync::Mutex<Option<std::sync::Arc<registry_connection_owner::CloseSlot>>>,
}
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AcquisitionSnapshot {
    pub(crate) transferred: bool,
    pub(crate) acquisition_joined: bool,
    pub(crate) close: AcquisitionCloseProgress,
}
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AcquisitionCloseProgress {
    NotStarted,
    Pending,
    ActualAcknowledged,
    ConfirmedJoined,
    UnconfirmedJoined,
}

#[cfg(test)]
pub(crate) mod schema9_namespace_test_hook;
