//! ACP Remote node access service.

#[cfg(windows)]
pub mod access_policy;
pub mod adapters;
pub mod application;
pub mod challenge;
pub mod claims;
pub mod config;
pub mod data_dir;
pub mod domain;
pub mod error;
pub mod identity;
#[cfg(windows)]
pub mod ipc;
pub mod jws;
#[cfg(windows)]
pub mod management_ws;
#[cfg(windows)]
pub mod node;
pub mod pairing;
#[cfg(windows)]
pub mod pairing_http;
pub mod platform;
pub mod random;
pub mod session_storage;
#[cfg(any(test, feature = "shared-view-acceptance-fixtures"))]
pub mod shared_view_test_support;
pub mod storage;
pub(crate) mod strict_json;
pub mod tls;
pub mod transcript_cache;
pub mod trust;

#[cfg(test)]
#[path = "../tests/registration_acceptance/c/schema3_migration.rs"]
mod schema3_migration_acceptance;
#[cfg(test)]
#[path = "../tests/registration_acceptance/c/schema4_migration.rs"]
mod schema4_migration_acceptance;
#[cfg(test)]
mod test_support;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the product version embedded at build time.
#[must_use]
pub const fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}

#[cfg(test)]
#[path = "../tests/acceptance/prompt_document_private.rs"]
mod prompt_document_acceptance;

#[cfg(test)]
mod sqlite_fixture_scope;

#[cfg(all(test, windows))]
#[path = "storage/pool_close_wait_development.rs"]
mod pool_close_wait_development;
