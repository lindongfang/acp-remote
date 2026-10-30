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
pub mod storage;
pub(crate) mod strict_json;
pub mod tls;
pub mod trust;

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
