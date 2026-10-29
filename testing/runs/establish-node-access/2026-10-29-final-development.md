# establish-node-access final development run

Status: development evidence only. This run does not replace independent tester/reviewer acceptance and does not satisfy the human-review cases NA-033/NA-034.

## Version and environment

- Date: 2026-10-29 (Asia/Shanghai)
- Baseline HEAD: `7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`
- Version: current uncommitted workspace snapshot
- Operating system: Windows x64, interactive desktop user session
- Node: current workspace Rust binary
- Node origin: `https://localhost:28444`
- Node ID: `a348d753f1d18311ca7875492cb66ed1b70a64d2a80af1e4db0d6f761f3bc858`
- TLS: generated self-signed localhost certificate; Chromium certificate errors were bypassed with test-only automation
- Browser A: ego Chromium task space `establish-node-access-archive-20261029-1945`, client `E2E Chromium A`
- Browser B: isolated Edge profile `.tmp/establish-node-access-final-20261029/edge-b-recovery-20261029`, client `Archive Edge B`

## Automated checks

| Check | Result | Notes |
| --- | --- | --- |
| `cargo fmt --manifest-path app/Cargo.toml --all --check` | pass | Current workspace |
| `cargo check --manifest-path app/Cargo.toml --all-targets --locked` | pass | Current workspace |
| `cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings` | pass | Current workspace |
| `cargo test --manifest-path app/Cargo.toml --lib --locked` | pass | 113 passed, no filtering |
| `cargo test --manifest-path app/Cargo.toml --all-targets --locked` | pass | 113 library tests and 4 JWS vector tests |
| Targeted shutdown ordering test | pass | `node::tests::shutdown_does_not_publish_a_report_before_the_ipc_task_finishes` |
| `npm.cmd --prefix clients/web run check` | pass | TypeScript check |
| `npm.cmd --prefix clients/web run test` | pass | 6 files, 30 tests |
| `npm.cmd --prefix clients/web run build` | pass | Vite production build |
| `npm.cmd exec -- openspec validate establish-node-access --strict` | pass | Change structure and strict validation |

### Race found and fixed during final verification

The first complete Rust run exposed four startup/shutdown tests timing out while stopping the owner control channel. The cause was that `watch::Sender::send` does not preserve a new state when no receiver exists yet; an immediate shutdown could therefore be lost before the IPC task subscribed. `IpcControl` now uses `send_replace` for stop and shutdown state and the IPC server checks the current stop state before entering `select!`.

After the fix, the previously failing restart, duplicate-instance, invalid-TLS/occupied-port, and corrupt-identity tests passed in the complete suite. This is development regression evidence; the independent tester must still rerun it.

## Real browser observations

### Client identities

- A: client ID `aRVsX63pBiWQlK9wEW-Vsg`, fingerprint `04c6edb99c7df5f55c66b734132c983e6443097fd9a6bdd94e8e4e1edfca1b21`.
- B recovery: client ID `HS9HnKKjRAlxw0cRrfn_7A`, fingerprint `53368438a50003360d8e2c4ff00677d66b946e29150be447cb1bc9a889abe282`.
- Both browsers stored non-extractable P-256 private keys in IndexedDB.

### Pairing and lost approval response

- B completed owner-started lookup and claim. The owner terminal showed the requested client name and fingerprint and accepted only an explicit `y`.
- The node was stopped immediately after owner approval so the browser did not receive the approval response.
- After restarting the same data directory, B authenticated using its saved public key, changed from `pending_confirmation` to `consumed`, and retained the client ID recorded by the owner approval. It did not require a second `device pair` command.

### Browser process persistence

- The isolated B profile's Edge processes were closed and the same profile was opened again.
- IndexedDB still contained status `consumed`, client ID `HS9HnKKjRAlxw0cRrfn_7A`, display name `Archive Edge B`, and `privateKeyExtractable === false`.
- WSS authentication succeeded again from the saved identity.

### Revocation and node restart

- A revoked B through the real management page. The device-list revision became 5 and showed the B revocation timestamp.
- B's existing management socket closed with code `4410`, reason `revoked`, and the page displayed the revoked state.
- Every subsequent B reload opened a new WSS connection, sent its saved public key proof, and closed with code `4401`, reason `authentication failed`.
- The node was stopped and restarted in the same data directory. A authenticated again with the same client ID and a non-extractable private key. B remained rejected with `4401`; revocation was not undone by restart.

### Graceful stop and resource release

- `acp-remote daemon stop` returned `state=stopped` with the expected node ID, origin, and PID, and the foreground process printed `ACP Remote node stopped`.
- A's management page changed to the stopped state and reported that the management connection had closed while the node was stopping.
- After shutdown, the node process was absent, TCP port `28444` had no listener, the `acp-remote.*` named pipe was absent, and a same-directory restart succeeded. The successful restart demonstrates that the data-directory lock was released.

Evidence screenshots from the development run are under `.tmp/establish-node-access-final-20261029/evidence/`; `.tmp` is intentionally excluded from delivery hashes because it contains browser identity material.

## Not executed or blocked

- Windows/Chromium trust of a real certificate without ignore-certificate overrides was not exercised.
- No real Tailscale route was exercised.
- No second real Windows user was available for cross-user data-directory, named-pipe, DACL, and DPAPI denial tests.
- Concurrent approval/revocation, forced storage-commit failure, queue invalidation, full-budget and shutdown-deadline exhaustion matrices were not completed at L4/L6.
- NA-010 and NA-030 depend on a later Harness approval boundary and are not implemented by this change.
- NA-033 and NA-034 require independent human review and were not signed.
- Independent tester execution of NA-001–034 and independent reviewer inspection of this fixed snapshot were still pending when this development record was written.

## Cleanup ownership

Only the node started from `.tmp/establish-node-access-final-20261029/acp-remote.toml` and browser processes using the four test profiles named in the run belong to this exercise. No user database, identity, certificate, normal browser profile, or unrelated process was modified.
