# Independent static review: strict4R v11 dynamic gate

Review target: `C:\Users\jack\.codex\visualizations\2026\10\30\01a18dc1-45b2-7f82-981e-964264221cef\source-sql-v4-acceptance-v2\strict4R-dynamic-gate-v11-scopebound-20261108`
Revision: `strict4R-v11-scopebound-20261108-r1`
Review type: read-only inspection only. No verifier, Cargo, list, or test command was run. No gate, product, or source-candidate file was changed.

## Decision

**Do not allow compile or list under v11.** The package is not internally executable as frozen: its manifest records an incorrect verifier byte count, and its static verifier requires a runner diagnostic string that the frozen runner does not contain. The latter makes the verifier fail even if the byte-count row is corrected. Keep `SSF-SQL-RECONCILIATION001` **Open**. This review does not authorize tests, product integration, or closure of that finding.

## Findings

1. **Blocking freeze metadata mismatch.** `PROGRAM-MANIFEST-v11.json` records `verify-static.mjs` as 11,377 bytes with SHA-256 `233da287207b22d2e011014f99291d0cd18b82e4ca5a22d316df55baf0d632ca`. The actual file is 11,563 bytes. Its SHA does match the recorded SHA, but its byte count does not. Both `run-gate.ps1` and `verify-static.mjs` iterate the program member table and require both byte count and digest equality, so the verifier and runner will reject this freeze.

2. **Blocking verifier/runner source-contract mismatch.** `verify-static.mjs` requires the runner to contain the literal `review approval scope mismatch`. `run-gate.ps1` contains `review approval scope/package binding mismatch` and `review scope differs from runner scope`, but not the verifier's required literal. Therefore the verifier's `owned runner/mode scope enforcement` assertion fails. The v11 program-manifest digest correctly binds these exact current files; this is a content incompatibility, not a hash drift.

3. **Frozen identity rows otherwise match for the checked program members.** The other seven entries in `programFiles` match actual bytes and SHA-256. The program manifest's digest matches `PROGRAM-FREEZE-v11.json`; manifest/freeze report, runner and verifier bindings match their current files; the config revision and scope are consistently `compile`/`list`, the two selectors, and test-disabled. The overlay, acceptance source leaves, Cargo manifest and lock match the hashes recorded in the manifest/config. These matches do not cure findings 1–2.

4. **Selector and command scope are narrow as declared.** The two exact Rust selectors resolve through the Windows-only `node::tests::source_session_development` include to functions present in the candidate development test file. `run-gate.ps1` accepts only `compile` and `list`, fixes Cargo compile arguments to `test --lib --all-features --locked --offline --no-run --message-format=json`, and for list runs the supplied compiled binary with exactly one approved selector plus `--exact --list`. It checks exact selector/label pairing, one matching test line, and a `1 test` summary. Test mode is unavailable. These scope controls are statically narrow but cannot be used until the freeze and verifier defects are repaired and a fresh review approves the resulting exact package.

## Limits

This is a static package review, not execution evidence. I did not run the verifier, Rust tooling, selector listing, or tests. I did not validate the behavioral adequacy of the SQL tests, the external Harness, or the unresolved reconciliation finding. No approval document was created or written into the reviewed package.
