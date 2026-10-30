//! Shared ACP transport and negotiated version strategies; no process or authorization authority.
mod model;
mod strategy;
mod transport;
pub use model::*;
pub use strategy::*;
pub use transport::*;
#[cfg(test)]
mod tests;
