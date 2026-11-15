# Q-004 6.8 final development evidence

- Captured: 2026-11-10T08:37:30+08:00
- Branch / HEAD: `codex/session-foundations-replay-planning` / `65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- Environment: Windows `10.0.26300.0`, Rust/Cargo `1.98.1`
- Workspace state: dirty by design; user and staged-work modifications were preserved.
- Scope: CL-117–122 development and protocol-contract evidence. The protocol peer is a
  synthetic fixture, while the child process, pipe ownership, Windows Job cleanup, queue
  owner and SQLite are real. This is not CL-123 real-Harness/Web acceptance.

## Executed checks

All Cargo commands used:

```powershell
$env:CARGO_TARGET_DIR = (Join-Path (Get-Location) '.tmp\q004-final-6-8-target')
$env:NATIVE_DISPATCH_DEV_CHILD = (
  Resolve-Path '.tmp\q004-final-6-8-target\debug\native_dispatch_dev_child.exe'
).Path
```

1. Fixture build:
   `cargo build --manifest-path app/Cargo.toml --features native-dispatch-dev-fixtures --bin native_dispatch_dev_child --locked --offline`
   passed.
2. `native_factory_real_v1_v2_stopped_keeps_three_paused_until_explicit_resume`
   passed 1/1 and exercised v1/v2 × (`max_tokens`, `max_turn_requests`, `refusal`,
   `cancelled`) = 8 real Windows child scenarios.
3. `native_factory_v2_early_idle_waits_for_late_insertion_without_second_idle`
   passed 1/1. Early terminal and late insertion were observed separately and no second
   idle was required.
4. `native_factory_v2_unknown_insertion_never_settles_or_retries`
   passed 1/1. The insertion remained unknown, no retry occurred, and source closure
   discarded the dead association.
5. `native_factory_real_priority_only_advances_original_selected_item`
   passed 1/1 and exercised v1/v2 × (`end_turn`, `error`, `cancelled`) = 6 real Windows
   child scenarios. Only the originally selected Y advanced; X and Z remained paused.
6. `native_factory_release_clears_retired_v2_prompt_association`
   passed 1/1. Resource release removed the old X association, retired the environment
   target, and did not let the late response settle Y.
7. `native_repeated_control_release_does_not_wake_unchanged_target`
   passed 1/1.
8. `registry_live_terminal_pauses_before_external_marker_without_creating_run`
   passed 1/1. The terminal fact paused the queue without creating a fabricated Run.
9. The `registry_selected_priority_` group passed 4/4 against the queue owner and SQLite:
   edit, cancel, resource replacement and interrupt failure all paused every retained item
   without falling back.
10. `shared_input_queue_acceptance` filter `cl090_api` passed 4/4. The two stale tests
    that previously required fallback were corrected to the current CL-122 contract:
    selection invalidation pauses all retained items, produces no dispatch, and explicit
    resume restores the original order.
11. `cargo fmt --manifest-path app/Cargo.toml -- --check`, `git diff --check`, and
    `openspec validate deliver-codex-session-loop --strict --json` passed. The OpenSpec
    strict summary was 1/1.

## Case mapping

| Case | Current evidence | Status and limit |
| --- | --- | --- |
| CL-117 | Stopped matrix for v1/v2 max-token, max-turn and refusal; registry external terminal | L3/L4 supported; no real-Harness L5 |
| CL-118 | Stopped matrix for v1/v2 ordinary cancellation; early-terminal separation | L3/L4 supported; no real-Harness L5 |
| CL-119 | Early idle then late insertion; insertion never returned | L3/L4 supported; no real-Harness L5 |
| CL-120 | Stopped matrix requires explicit resume; registry and shared queue keep items suspended | L3/L4 supported; final WSS/Web behavior remains in 6.9 |
| CL-121 | `end_turn`, `error`, stopped reasons, unknown insertion and external terminal remain distinct | L3/L4 supported; no real-Harness L5 |
| CL-122 | Six priority scenarios plus edit, cancel, resource replacement, interrupt failure and repeated release | L3/L4 supported; no real-Harness L5 |
| CL-123 | Not executed by this evidence | Remains open for 6.10 real L6 acceptance |

## Frozen source hashes

| Path | SHA-256 |
| --- | --- |
| `app/src/application/native_dispatch/environment.rs` | `C3D170FCD53CDBEE6F5ADC0E668291C1179A588078AF42368FFA8B080BF62B00` |
| `app/src/application/native_dispatch/resource_close.rs` | `F06A217EF2D9A61AD48BD646F04D9FEA82D3443CFCD0AE9C6A6354B74D8AECEB` |
| `app/src/application/native_dispatch/shutdown.rs` | `482FD59DB8AB48982178EBE260DCBCAD135ACAB6A64DD2A42139F23974C05A82` |
| `app/src/application/native_dispatch/dispatch.rs` | `A505499BD3BDA8C6F3716A1CA69FA96A8F0D326FF5BBA24221BE0834F1E07775` |
| `app/src/application/native_dispatch/observation.rs` | `8EC3FA5943F54B25AEAC96CD27C4C1AF585FD9D786E835E62D0106820A0E7C28` |
| `app/src/application/native_dispatch/dev_tests.rs` | `B8B1EB9365DA9DB43A98B9807B3A2B5D239C49926B4390630AB0A078F8D17E81` |
| `app/src/domain/input_queue.rs` | `ED98455D448C4A45B3906BF57D047EB8CE861239C7F1AA71E10F93BB08B04307` |
| `app/src/application/queue_owner/tests.rs` | `619872E62F65C041199723B5962C950532B8C6F1525C59100DBED5DACB7D42C9` |
| `app/dev-fixtures/native_dispatch_dev_child.rs` | `0E7EBAA7FAE705C9D93D0AB49A71CC7A01C59A3F6223423F3BF463683BC793C7` |
| `app/tests/shared_input_queue_acceptance.rs` | `4190F7BF53B507D353A78140351F84BE7F8D75E0423EF229A86221572BE1D500` |
| `native_dispatch_dev_child.exe` | `BF1E71AA6965B835B3464927AB3BED3C193413F69CBA58BE2B373D892A4949FF` |
| `specs/session-execution/spec.md` | `45CB72683BFDCCD91444190DA27D076028969043B0A74FEA499CC9C927BC4287` |
| `tests.md` | `1B9086F633785D5A0DAC1BB39AC9A8C3E17397F5A0228F63E79D6B2BB9CE1F53` |

## Raw output hashes

| Evidence | SHA-256 |
| --- | --- |
| `2026-11-10-q004-final-native_factory_real_v1_v2_stopped_keeps_three_paused_until_explicit_resume.stdout.txt` | `7A498683C48BEC194CAD14A4A945C7C4280E91C8558197DBC8DF5FE11F351793` |
| `2026-11-10-q004-final-native_factory_release_clears_retired_v2_prompt_association.stdout.txt` | `352F749A63B353E7FC76CFF6EA9EC3B4C286ED6667FA74CBB7AF3B75AFFDF6ED` |
| `2026-11-10-q004-final-native_factory_v2_early_idle_waits_for_late_insertion_without_second_idle.stdout.txt` | `B8369F6E5B50A3C477DC9601857ED387EDEA119C453E5178ED725C38948C4F56` |
| `2026-11-10-q004-final-native_factory_v2_unknown_insertion_never_settles_or_retries.stdout.txt` | `15DAB566347700504AF2BD5797B59C45994FBE56B46711FC64746B0C92F19164` |
| `2026-11-10-q004-final-native_repeated_control_release_does_not_wake_unchanged_target.stdout.txt` | `4974EE4F9E9645CAF848324FF3C63F4A1032B106864883D98EE635F2C68D43B3` |
| `2026-11-10-q004-final-priority.stdout.txt` | `AA38BFB93F288F90959563F8F0B68051C61883C7B75729050262FB0EDFAF4A40` |
| `2026-11-10-q004-final-registry-live-terminal.stdout.txt` | `300D57487728A91FEBBF31B68C0A3BCAC492D645F6EEE257F73118EDBDF7CFAE` |
| `2026-11-10-q004-final-registry-selected-priority.stdout.txt` | `41D050A41AF36123770F943EF7E5400C400B33294A22B5884581DED53E183EF3` |
| `2026-11-10-q004-final-shared-input-priority.stdout.txt` | `42B82F9E9769574645A6A9282A9456E7ECDB605BF015FDD1223C2443E8B45B51` |

## Remaining boundary

The attempted batch wrapper stalled after three successfully saved groups. Its fourth
test was rerun alone and passed 1/1, followed by the fifth test, both with saved output.
There is no successful evidence that the batch wrapper itself completed, and this record
does not claim it did.

6.9 still needs final-source WSS/Web/native integration and complete resource-tree
evidence. 6.10 still needs independent CL-117–123 results, including CL-123 against a
real Harness and Web client. No real Harness result is claimed here.
