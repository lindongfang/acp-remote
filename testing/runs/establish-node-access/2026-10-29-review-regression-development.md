# Second review regression development verification

Status: development evidence only. This revision supersedes the implementation behavior described in the earlier shutdown regression report; historical reports remain unchanged.

## Changes

- NA-REV-001/002 were independently closed for the preceding revision. The pre-subscription stop checks and response write error propagation remain in place.
- NA-REV-003: the node publishes one immutable resource-cleanup result. Response delivery failures are collected independently and returned as the final shutdown error. A failed response does not abort other stop clients; only the shared response deadline cancels unfinished deliveries. The node process does not print normal shutdown success when delivery fails, while healthy clients can still observe the completed resource cleanup.
- NA-REV-004: typed WebSocket capacity errors map to 4429; protocol and UTF-8 errors map to 4400. Added an explicit dependency on the already locked tungstenite 0.29.0 error type with default features disabled; no new resolved package or parser was introduced. Cargo updated the root dependency entry in its lockfile offline.
- NA-REV-D01: removed outdated statements that the contract had no implementation, build entry, or dependencies. Actual verification still belongs to versioned run evidence.

## Verification

- `cargo fmt --manifest-path app/Cargo.toml --all --check`: passed.
- `cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings`: passed.
- `cargo test --manifest-path app/Cargo.toml --all-targets --locked`: passed in the real Windows desktop user session, 118 library tests and 4 JWS vector tests, no filtering.
- `stop_response_failure_does_not_cancel_other_clients`: actual named pipes, one client disconnected and one healthy; healthy client received stopped, failure remained visible to the owner, and published cleanup state did not change.
- `management_websocket_closes_oversized_transport_frames_with_4429`: actual TLS/WebSocket transport with 32769, 32770, and 65536 byte frames; all closed with 4429 and fixed limit reason.
- OpenSpec strict validation: passed.

Web sources did not change in this revision; earlier 31-test Web results are not represented as a new browser run. Independent retest, reviewer closure, current-version trusted Tailscale browser acceptance, cross-user and fault matrices, Rust advisory scanning, and human NA-033/034 signatures remain pending. No tasks are marked complete by this development report.
