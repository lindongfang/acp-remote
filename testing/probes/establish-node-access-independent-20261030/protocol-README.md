# Independent protocol acceptance

These probes use Node built-ins only. Runtime materials belong in
`D:/Project/acp-remote/.tmp/protocol-acceptance-20261030/`, protected for the real
Windows node owner before starting. Private client keys remain in memory.

Run `protocol-timeouts.mjs` against the owned trusted TLS node on port 30451.
It records actual elapsed time for no hello and no proof, without mock clocks.

For each reject/enter/ctrlc/disconnect case:

1. In a real owner PTY, run `protocol-tty-regression.ps1` with the fixed binary
   and isolated config. Read the newly printed pairing code.
2. Write owner-only runtime `pair-input.json` containing that code and the mode.
3. Run `protocol-pair-case.mjs` under the same real owner. Wait for CLAIM_READY
   and the actual terminal name/fingerprint/expiry prompt.
4. Type `n`, Return, or Ctrl-C into that PTY. For disconnect, inspect the exact
   `device pair` child command line and terminate only that owned child process.
5. The protocol peer verifies signed terminal state and rejection of a valid
   possession proof for the unapproved key. It exits 0 only on those assertions.

NA-PROTO-001 red reproduction on the r3 binary:

- Actual TTY invocation was `acp-remote.exe --config
  .tmp/protocol-acceptance-20261030/node.toml device pair`.
- After pending_confirmation at the approval prompt, send Ctrl-C (U+0003), not
  a string containing the characters `^C`.
- The CLI printed `Pairing cancelled before approval.` but process PID 39852
  remained alive. Native process observation at 2026-10-30T00:44:48.977+08:00
  recorded it still running; process start was 00:42:47.207+08:00.
- A later Return released the stuck PTY. The tool session ended with exit 1;
  the raw child exit code was not separately captured in that original shell.
- `protocol-tty-regression.ps1` is the repeatable real-PTY launcher for the
  repaired revision and emits PAIR_EXIT_CODE after the CLI returns. The tester
  drives the same claim and Ctrl-C sequence through the terminal tool, then
  checks prompt return and native child-process disappearance without Return.

`protocol-network-approval.mjs` tests rejected network approval routes.
`protocol-auth-matrix.mjs` requires a new owner-approved key and exercises wrong
signatures, connection-bound replay, 8+1 connection capacity, survival of the
eight existing connections, proof reuse and management before authentication.

`protocol-startup-retry.mjs` temporarily owns port 30452. It runs four startup
failures, repairs each fixture, then verifies repeated clean stop/start with the
same node identity and successful port rebind. Do not run it when that port is
owned by another task. All output and databases stay in the runtime directory.

These scripts are acceptance assets, not product code. Their TLS validation
is enabled; the raw WebSocket framing adapter does not simulate the server,
state machine, database, clock, CLI decision or cryptographic verification.
