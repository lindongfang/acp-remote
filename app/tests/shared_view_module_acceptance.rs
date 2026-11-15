#![cfg(all(windows, feature = "shared-view-acceptance-fixtures"))]

#[path = "shared_view_module_acceptance/public_cases.rs"]
mod public_cases;
#[path = "shared_view_module_acceptance/support.rs"]
mod support;
