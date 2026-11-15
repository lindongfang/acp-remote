# Regression evidence for new v12 source set

## Scope

All files under `testing/runs/deliver-codex-session-loop/2026-11-11-*` are evidence for the current working tree after the source changes recorded in the v11 freeze. The v11 `SOURCE.json` is stale: a file-by-file comparison found 45 controlled files changed after the original v11 freeze, so the previous `SOURCE.json` cannot bind these later runs to the old source set.

## Valid passing runs

| Run | Exit | Result | Raw output | SHA-256 |
|---|---:|---|---|---|
| ACPI R01-R09 | 0 | `13 passed; 0 failed; 899 filtered out` | `2026-11-11-r10-narrow/acpi-r01-r09-owner.stdout.txt` | `3AFD62312F484D88DDFFFD882F0394729665BBFBD2989B39EA6FAEA1A5C72759` |
| R10 production factory | 0 | `6 passed; 0 failed; 906 filtered out` | `2026-11-11-r10-narrow/production-factory-owner.stdout.txt` | `B26EA1C0D913F61AEB16B2BADF6C348225DBE6FC437898CEE820C027CF0689A9` |
| R10 queue control-only | 0 | `1 passed; 0 failed; 911 filtered out` | `2026-11-11-r10-narrow/queue-read-control-only-owner.stdout.txt` | `D7BB467E37AD1B4DF9A4AE64BFE1B0EF7C1B884419543F0A647A6AAFFEC5460C` |
| CL-009 device revoked | 0 | `1 passed; 0 failed; 912 filtered out` | `2026-11-11-cl009-device-revoked/queue_read_device_revoked.stdout.txt` | `256169E57BDB0AC58D841C01F4DE6ECEDF9A633F32475CF23A63FA1D984635C7` |
| CL-017 feature fixture | 0 | `3 passed; 0 failed; 48 filtered out` | `2026-11-11-wp4-regression/cl017-registration-owner-feature.stdout.txt` | `564F8FB67D377FFB729CB2FA60C65D362A0DEBF0795359DD9D3362AFA47C5FD3` |
| CL-017 SQL fault | 0 | `1 passed; 0 failed; 911 filtered out` | `2026-11-11-wp4-regression/cl017-f11-sql-fault.stdout.txt` | `65B5C0568317565B1C703058A89DF7108D80FF018054D7030AC486D3FBB962E9` |
| CL-024 view window | 0 | `2 passed; 0 failed; 55 filtered out` | `2026-11-11-wp4-regression/cl024-view-window.stdout.txt` | `EA3F14F4FE8970621B4DE3CFE7B6B7D499C894F79FF55F19545F93FAFC9E8F31` |
| Registration owner acceptance | 0 | `51 passed; 0 failed; 0 filtered out` | `2026-11-11-wp4-regression/registration-owner-acceptance.stdout.txt` | `15CA1EEFD1D7DCF42C1F680EC8779E2C7C2601945F35567D27AC695AA1F1415E` |
| Session loop acceptance | 0 | `57 passed; 0 failed; 0 filtered out` | `2026-11-11-wp4-regression/session-loop-acceptance.stdout.txt` | `5A2711FDAE4B77AC356CCD55780AD649A07050FAFE47945BC22EDF6C05C13CC2` |
| Shared input queue | 0 | `39 passed; 0 failed; 0 filtered out` | `2026-11-11-wp4-regression/shared-input-queue.stdout.txt` | `BA52140E639C26FDD1C5C00364E7390FB6BEBDC5DD2A671C36E65514CA07C0C2` |
| Socket independent | 0 | `15 passed; 0 failed; 897 filtered out` | `2026-11-11-wp4-regression/socket-independent.stdout.txt` | `CB0897360D220E56024969468441D9E386C2B37EF01B67C8B9D4DC464AC372E9` |

The CL-017 run `2026-11-11-wp4-regression/cl017-registration-owner.*` exited 101 only because it omitted `--features registration-acceptance-fixtures`; Cargo reported the missing feature. This is a command error, not a product failure.

## Known blocking failures

| Run | Exit | Result | Cause |
|---|---:|---|---|
| Full Rust all-targets | 101 | `890 passed; 22 failed; 0 ignored; 0 measured; 0 filtered out` | 21 tests failed on missing explicit fixture-root environment variables; 1 was the prompt duplicate-key defect later fixed and revalidated in `prompt-input-duplicate-key4`. |
| Strict Clippy | 101 | 414 diagnostics in v11 | Strict Clippy remains a blocking gate. |

Full-rust source output: `2026-11-11-full-rust-catalog-fix/full-rust-catalog-fix.stdout.txt`, SHA-256 `10CBC35BB6B3D94087261059062A487AFDC26A3A8B89F8A57763235B4AD9E0CA`. The exit marker records `CARGO_EXIT=101`.

## Not covered here

- Complete full-rust rerun after supplying the missing fixture-root variables.
- Current Web check/test/build, fmt, OpenSpec strict, and `git diff --check` for the latest source set.
- Real harness/session validation and independent reviewer sign-off.
