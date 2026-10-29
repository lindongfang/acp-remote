# establish-node-access historical development checkpoint

Status: historical development checkpoint, superseded by `2026-10-29-final-development.md`. Statements below about work not yet executed describe this earlier checkpoint only and must not be read as the current result.

## Version and environment

- Date: 2026-10-29 (Asia/Shanghai)
- Baseline HEAD: `7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`
- Version: current uncommitted workspace snapshot
- Operating system: Windows x64, desktop user session
- Node: node environment used by the repository
- Browser A: ego Chromium task space `establish-node-access-final-20261029`
- Browser B: isolated Microsoft Edge profile under `.tmp/establish-node-access-final-20261029/edge-b-profile`
- Node origin: `https://localhost:28444`
- TLS: test-generated self-signed certificate; Chromium certificate errors were ignored through test-only CDP settings

## Automated checks observed

| Check | Result | Notes |
| --- | --- | --- |
| Non-sandbox repository `npm run check` | pass | Earlier complete run: Rust lib 109 passed, JWS vectors 4 passed, Web build and tests passed |
| `cargo fmt --all --check` | pass | Re-run against the current workspace |
| `cargo check --all-targets --locked` | pass | Re-run against the current workspace |
| `cargo clippy --all-targets --locked -- -D warnings` | pass | Re-run against the current workspace |
| `npm --prefix clients/web run check` | pass | Re-run against the current workspace |
| `npm --prefix clients/web run test` | pass | 6 files, 26 tests |
| `npm --prefix clients/web run build` | pass | Vite production build completed |

The sandboxed Rust unit-test rerun was stopped because Windows filesystem, named-pipe and network cases require the real desktop user environment. It is not recorded as a product failure or a pass. The complete non-sandbox run above remains the authoritative automated result for the previous integrated snapshot.

### Current-session recheck

- Re-ran `cargo fmt --all --check`, `cargo check --all-targets --locked`, and `cargo clippy --all-targets --locked -- -D warnings`: pass.
- Re-ran `npm --prefix clients/web run check`: pass; `npm --prefix clients/web run test`: 6 files, 26 tests passed; `npm --prefix clients/web run build`: pass.
- Re-ran `cargo test --test jws_vectors --locked`: 4 tests passed.
- Re-ran the non-node/non-platform Rust library subset in the sandbox: 73 passed, 6 identity-file tests failed with `PermissionDenied`, 30 tests filtered. These failures are sandbox environment evidence, not a claim that the Windows product path passed.
- `npm exec -- openspec validate establish-node-access --strict`: pass.
- Non-sandbox Rust test execution was not available because the command approval service was at its usage limit; no approval was bypassed.

## Real browser observations

### Browser A: initial trust and management

- Client name: `E2E Chromium A`
- Client ID: `aRVsX63pBiWQlK9wEW-Vsg`
- Public-key fingerprint: `04c6edb99c7df5f55c66b734132c983e6443097fd9a6bdd94e8e4e1edfca1b21`
- IndexedDB state after pairing: `consumed`
- Private key read back with `extractable === false`
- WSS authentication succeeded.
- `node.info`, `device.list` and `devices` subscription succeeded.

### Browser B: second-client pairing

- Client name: `E2E Edge B`
- Client ID: `xdSyC-ViTYAkB9r1AR3pcg`
- Public-key fingerprint: `8c59477723596009d3b7100dadca854dfaeb3e12a79d4f9f61a09a031735347a`
- The owner terminal displayed the client name and fingerprint and accepted only an explicit `y`.
- The browser stored a non-extractable P-256 key in IndexedDB.
- WSS hello, challenge verification, client proof and `authenticated` succeeded.
- The device list observed revisions 2 and then 3 with both A and B present.

### Revocation and reconnect rejection

- Browser A revoked B through the real management UI.
- A's existing WSS connection remained usable and refreshed the list to revision 3.
- B's existing WSS connection closed with code `4410`, reason `revoked`.
- B then showed `管理权限已被撤销`.
- Reloading B created a new WSS connection, which closed with code `4401`, reason `authentication failed`.
- No new device operation succeeded from B after revocation.

### Anomaly observed during setup

One B claim attempt made after an earlier pairing challenge had become stale ended in the generic closed contract-error UI. A fresh active pairing and the same browser later completed lookup, claim, challenge and status successfully with all response bodies matching the contract. The anomaly was not reproduced, but an independent tester should repeat stale/interrupted-claim retries before declaring the case closed.

## Not executed or blocked

- Graceful node stop had to be paused because the command-review service exhausted its quota. Chromium A's 1001 close code, final resource release and CLI stop output were not captured.
- Restart of the same node and post-restart authentication of A and rejection of B were not executed.
- Complete browser-process exit and reopen with the same profile was not executed.
- A certificate trusted by Windows/Chromium without ignore-certificate overrides was not available.
- No real Tailscale route was exercised.
- Cross-Windows-user data-directory, named-pipe and DPAPI denial were not executed.
- NA-010 and NA-030 require a later Harness approval boundary and are not implemented by this change.
- NA-033 and NA-034 require independent human review and were not signed.

## Cleanup ownership

Only resources under `.tmp/establish-node-access-final-20261029/` and the node process started from that configuration belong to this run. No user database, identity, certificate, browser profile or unrelated process was modified.
