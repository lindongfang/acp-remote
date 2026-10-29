# Shutdown regression development verification

Status: development verification only; independent retest and reviewer closure pending.

## Findings and fixes

- NA-REV-001: `NodeRuntime::run` and `wait_for_stop` now inspect the current watch value with `borrow_and_update` immediately after subscribing. A stop or failure published before subscription therefore enters shutdown without waiting for another update.
- NA-REV-002: stop-response tasks now return their write result. The owner joins these results, cancels and drains remaining response tasks on failure, returns a shutdown error, and updates the final control state to failed. A disconnected client is treated conservatively as a response-delivery failure; the node does not print normal shutdown success in this case.

## Checks

- Rust formatting and Clippy with warnings denied: passed.
- New deterministic tests: `run_observes_a_stop_requested_before_subscription`, `wait_for_stop_observes_a_failure_before_subscription`, and `stop_response_write_failure_is_reported`: passed.
- Complete `cargo test --manifest-path app/Cargo.toml --all-targets --locked` in the real Windows desktop user session: 116 library tests and 4 JWS vector tests passed; no filtering.
- The preceding sandbox run had 113 passes and 3 DPAPI failures (`CryptProtectData`, Windows error 2). It is not recorded as a complete pass. The real-user rerun above covers those same tests successfully.
- `cargo-audit` installation into `.tmp/cargo-audit-tool` failed while downloading `abscissa_core` from crates.io after repeated network timeouts. No Rust advisory scan was completed.

The build used `.tmp/establish-node-access-tailscale-20261029/build-target`. No dependency or lockfile changes were made. Earlier trusted TLS/Tailscale and browser reports describe their earlier source snapshot; they must not be presented as a fresh browser run of these shutdown fixes.

## Remaining delivery work

Independent retest and reviewer closure of NA-REV-001/002, cross-user Windows isolation, remaining fault and concurrency scenarios, Rust advisory scanning, and human NA-033/034 acceptance remain pending. This report does not mark any acceptance case or tracked task complete.
