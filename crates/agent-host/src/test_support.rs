//! Deterministic Agent startup faults for this crate's tests.
//!
//! Faults travel in the child's whitelisted environment so parallel tests cannot affect one
//! another. They are recognized only in test builds by the library's process supervisor.

use std::path::PathBuf;

use crate::error::HostError;

pub(crate) const STARTUP_FAULT_ENV: &str = "ACPR_TEST_AGENT_STARTUP_FAULT";
pub(crate) const STARTUP_BARRIER_ENV: &str = "ACPR_TEST_AGENT_STARTUP_BARRIER";

pub(crate) fn startup_fault(env: &[(String, String)]) -> Result<Option<String>, HostError> {
    env_value(env, STARTUP_FAULT_ENV)
}

pub(crate) fn startup_barrier(env: &[(String, String)]) -> Result<Option<PathBuf>, HostError> {
    Ok(env_value(env, STARTUP_BARRIER_ENV)?.map(PathBuf::from))
}

fn env_value(env: &[(String, String)], name: &str) -> Result<Option<String>, HostError> {
    let mut matches = env
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone());
    let Some(value) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(HostError::SpawnFailed {
            detail: format!("duplicate {name}"),
        });
    }
    Ok(Some(value))
}
