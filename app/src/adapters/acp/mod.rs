//! Shared ACP transport and negotiated version strategies; no process or authorization authority.
mod configuration;
mod model;
pub mod native_dispatch;
#[cfg(windows)]
pub(crate) mod native_views;
pub mod node_configuration_profile;
mod prompt_input;
pub mod raw_json;
mod raw_model;
pub mod shared_view_capture;
#[cfg(all(windows, any(test, feature = "shared-view-acceptance-fixtures")))]
pub mod shared_view_module;
pub(crate) mod shared_view_projection;
pub mod shared_view_read;
mod strategy;
pub mod tool_projection;
mod transport;
pub mod turn_classify;
pub mod view_observations;
pub use configuration::*;
pub use model::*;
pub use prompt_input::PromptDocument;
#[cfg(all(test, windows))]
pub(crate) use prompt_input::test_hooks as prompt_preparation_test_hooks;
pub use raw_model::{OrderedRawObject, RawMetadata};
pub use strategy::*;
pub use transport::*;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod runtime_tests;

#[cfg(test)]
pub(crate) mod raw_preservation_independent;

#[cfg(windows)]
pub(crate) mod catalog_draft_document;
