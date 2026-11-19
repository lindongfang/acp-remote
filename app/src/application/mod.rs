//! Input admission port. The execution owner dispatches only newly accepted work.
#[cfg(windows)]
pub(crate) mod document_disposal;
#[cfg(windows)]
pub(crate) mod document_source;
#[cfg(windows)]
pub(crate) mod document_transfer;

pub mod approval_owner;
pub mod native_catalog;
pub mod native_dispatch;
#[cfg(test)]
pub mod product_commands;
#[cfg(windows)]
pub(crate) mod prompt_preparation;
pub mod queue_owner;
pub mod request_registry;
pub mod shared_views;

#[cfg(windows)]
pub(crate) mod catalog_preparation;
