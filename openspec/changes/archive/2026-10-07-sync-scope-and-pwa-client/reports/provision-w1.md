# Provisioner Handoff — Wave 1 (WP1, WP2)

- **task_id**: `provision-w1`
- **role**: `provisioner`
- **phase**: `runtime`
- **agent_id**: `ProvisionerW1` (`fork_turns=none`)
- **target_revision**: `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`
- **result**: **PASS** — 约定资源与就绪条件满足；不表示任何产品检查通过。

## Baseline Verification (pre-provision)

```
$ git rev-parse refs/heads/main        # cwd D:\Project\acp-remote
353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f
```

Matches the expected baseline `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`. Proceeding was therefore not blocked.

## Work Packages

| Field | WP1 | WP2 |
| --- | --- | --- |
| WP id | WP1 | WP2 |
| Attempt | 1 | 1 |
| Worktree (absolute) | `D:\Project\acp-remote\.worktrees\wp1` | `D:\Project\acp-remote\.worktrees\wp2` |
| Branch | `feat/wp1-sync-snapshot-contract` | `feat/wp2-derived-event-fields` |
| Baseline (verified in-worktree) | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| Isolation config | `node_modules` at repo root — **shared, read-only** (executor must not write) | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2` (exclusive); `node_modules` at repo root — shared, read-only |
| Claimed executor | `coding-1` (role `coder`) | `coding-2` (role `coder`) |
| Pre-start receipt time (Received At) | 2026-10-03T17:21:58+08:00 | 2026-10-03T17:21:58+08:00 |

## Commands Executed

| # | Command | cwd | Exit | Result |
| --- | --- | --- | --- | --- |
| PV-BASE | `git rev-parse refs/heads/main` | `D:\Project\acp-remote` | 0 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| PV-WT1 | `git worktree add .worktrees/wp1 -b feat/wp1-sync-snapshot-contract 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` | `D:\Project\acp-remote` | 0 | worktree prepared at `353ba6e` |
| PV-WT2 | `git worktree add .worktrees/wp2 -b feat/wp2-derived-event-fields 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` | `D:\Project\acp-remote` | 0 | worktree prepared at `353ba6e` |
| PV-H1 | `git -C .worktrees/wp1 rev-parse HEAD` | `D:\Project\acp-remote` | 0 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| PV-H2 | `git -C .worktrees/wp2 rev-parse HEAD` | `D:\Project\acp-remote` | 0 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| PV-TGT | `mkdir -p .target-wt/wp2` | `D:\Project\acp-remote` | 0 | `D:\Project\acp-remote\.target-wt\wp2` |
| PV-NM | `ls -d node_modules` | `D:\Project\acp-remote` | 0 | present |
| PV-READY | `npm run check` | `D:\Project\acp-remote` | 0 | full log below |

## Readiness Check Output (relied on by both WP1 and WP2)

Command: `npm run check` from `D:\Project\acp-remote`, exit code **0**.
Full capture: `openspec/changes/sync-scope-and-pwa-client/reports/provision-w1-check.log`.

```
> npm run check:schemas && npm run check:commands && npm run check:errors && npm run check:features && npm run check:assets && npm run check:acp && npm run check:docs && npm run check:boundaries && npm run check:drift && npm run check:agentic

schema fixtures OK: 127 valid, 30 invalid (ajv Draft 2020-12), 39 event views bound
command catalog OK: 13 commands
error registry OK: 58 codes across 2 protocols
feature registry OK: 13 feature ids across 2 protocols
contract assets OK: 17 schemas, 170 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed
ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)
… (check:docs / check:boundaries / check:drift all PASS)
✓ change/sync-scope-and-pwa-client
Totals: 22 passed, 0 failed (22 items)
agentic 宿主入口检查完成：17 个文件
```

## Observations

- Both `git worktree add` runs emitted an identical advisory warning, reproduced verbatim:
  `warning: worktree clone fell back to plain checkout: git worktree add: Windows block clone unsupported for \\?\D:\Project\acp-remote\.agents\skills\.openspec-target -> D:\Project\acp-remote\.worktrees\wp<N>\.agents\skills\.openspec-target: 函数不正确。 (os error 1)`
  It affects only block cloning of the `.agents/skills/.openspec-target` subdirectory, fell back to plain checkout, and did not affect repository source files or HEAD (both worktrees verified at the baseline commit). Exit code 0.
- No `git add` was run; `.gitignore` was not modified. `.worktrees/` and `.target-wt/` remain untracked and were not staged.
- Only WP1 and WP2 resources were created; nothing was provisioned for any other package.
- This PASS covers resource readiness only; it asserts nothing about any product check.

## Resource Cleanup

| Owned resource | Cleanup trigger |
| --- | --- |
| `D:\Project\acp-remote\.worktrees\wp1` (branch `feat/wp1-sync-snapshot-contract`) | reclaim after WP1 delivery unit completes |
| `D:\Project\acp-remote\.worktrees\wp2` (branch `feat/wp2-derived-event-fields`) | reclaim after WP2 delivery unit completes |
| `D:\Project\acp-remote\.target-wt\wp2` (WP2 exclusive `CARGO_TARGET_DIR`) | reclaim after WP2 delivery unit completes |

Nothing released this round; no other executor's worktree or instance was touched.

## Handoff Index

```agentic-handoff
version: 1
agent_context:
  agent_id: "ProvisionerW1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "provision-w1"
    work_package: WP1
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/provision-w1.json"
    result: PASS
    evidence_status: NEW
    applicability_basis: "Worktree .worktrees/wp1 on feat/wp1-sync-snapshot-contract created from refs/heads/main 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f, baseline re-verified in-worktree; root node_modules shared read-only confirmed; repo-root 'npm run check' exit 0."
    source_evidence: NOT_APPLICABLE
  - task_id: "provision-w1"
    work_package: WP2
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/provision-w1.json"
    result: PASS
    evidence_status: NEW
    applicability_basis: "Worktree .worktrees/wp2 on feat/wp2-derived-event-fields created from refs/heads/main 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f, baseline re-verified in-worktree; exclusive CARGO_TARGET_DIR .target-wt/wp2 created; repo-root 'npm run check' exit 0."
    source_evidence: NOT_APPLICABLE
```
