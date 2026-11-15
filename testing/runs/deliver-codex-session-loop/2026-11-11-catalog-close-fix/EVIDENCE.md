# Catalog close fix regression evidence (2026-11-11)

All runs use the current source set after removing the early registry seal from `app/src/storage.rs::begin_runtime_shutdown`. The authoritative same-source run environment uses Windows owner, independent TEMP, dedicated target, and the synthetic C1 child. No real Harness identity or native session was touched. `cli-close-after-fix` was an accidental filter with 0 tests and is not evidence.

| Run | Exit | Result | SHA-256 |
|---|---:|---:|---|
| `catalog-close-after-fix-owner2.stdout.txt` | 0 | `1 passed; 0 failed` | `FB519507123CBF4D0AB94040D7868D346591D8D34514517CDE5A1250166FB1F1` |
| `acpi-group-after-fix-owner.stdout.txt` | 0 | `13 passed; 0 failed` | `7ED7C809B31E1CE039565EFA1BC627CC22F3B8FF585B157D665934A0B08B41CE` |
| `production-factory-after-fix-owner.stdout.txt` | 0 | `6 passed; 0 failed` | `8B4ABB5FB4126F514127D5813CD609EA36C0EAA7D24A474A7A9E481902ECA31F` |

Catalog key observations:
- `metadata_close: Some(Ok(()))`
- `NativeShutdownReport { joined: true, metadata_confirmed: true }`
- `shutdown_ok=true`
- `[CATALOG-C1-CLOSE] complete true true true true`
- `test result: ok. 1 passed; 0 failed; ...`

ACPI scope: 13 tests include codec and R01-R09. Production factory scope is 6 tests and includes `production_factory_opens_listener_only_after_the_bundle_is_complete` and `production_factory_service_rejection_opens_no_listener_and_joins_workers`. The same run also includes `queue_read_control_only_does_not_report_an_empty_product`; verified by test-name search in the source at `app/src/node/session_services/queue_read_development_tests.rs:2835`.

Environment-restricted failures retained as diagnostics, not acceptance:
- `catalog-close-after-fix2.stdout.txt`: pairing IPC error 5 in sandbox.
- `catalog-close-after-fix-owner.stdout.txt`: `C1_NATIVE_CHILD` not present.
- `acpi-group-after-fix.stdout.txt`: 12 pairing IPC error 5 failures in sandbox; the codec test passed.

# Prompt nested duplicate-key fix (2026-11-11)

Confirmed failure: `complete_structure_rejects_late_damage_and_preserves_extensions` accepted a decoded duplicate key inside an unknown nested object. The old `require_unique_members` only checked the immediate object of each prompt block.

Fix: `PromptDocument::parse` now calls `RawDocument::require_unique_fields()` before per-block validation. This recursively checks every object in the raw document, including unknown nested objects and arrays. The local immediate-block-only helper was removed. `RawJsonError` still maps to the existing `AcpError::InvalidMessage` / `Allocation` boundary.

Test hardening added:
- direct duplicate key inside an unknown nested object remains invalid;
- two sibling objects with the same key remain valid (negative control), using an outer empty object to exercise the per-object scope;
- literal duplicate keys in a nested object are invalid;
- a valid escaped equivalent (`"\u0061"`) in a separate nested object is invalid as a decoded duplicate.

Evidence: Windows owner serial run of the four prompt_input tests passed, `4 passed; 0 failed`, exit 0.
SHA-256:
- `prompt-input-duplicate-key4.stdout.txt`: `D3C88218669AA1D5A5E5F65C5021CBD002CCE4996C0E604FCA4C4BE97C45C9CB`
- `app/src/adapters/acp/prompt_input.rs`: `7B51A2B6BEB217AE36192514CAC29991652F6D6B44F202666A6D791EB5C990A0`
- `app/src/adapters/acp/prompt_input/tests.rs`: `2A06DCCED5332840F753B6580EB895CCDECB3FAFAECB7C74D0A5AA898446AD58`
- `AGENTS.md`: `16C00B0DF7DA3C58CDFEECB8C5E823F662DAB7F00A0827B737C5AF9A673F716C`

Earlier failed diagnostics retained: `prompt-input-duplicate-key.stdout.txt`, `prompt-input-duplicate-key2.stdout.txt`, `prompt-input-duplicate-key3.stdout.txt`.
