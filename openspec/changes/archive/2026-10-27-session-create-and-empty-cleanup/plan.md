# Execution Plan

## Execution Model

- 主 Agent 使用 omp `task` 的 batch 形状派发工作包；每个工作包是一次独立 spawn。
- 每个会写文件的工作包都使用 `isolated: true`，隔离配置为 `merge: patch`、`apply: false`。
- 交付物是 `<id>.patch`、`agent://<id>` 与 `history://<id>`；作者不提交、不建分支、不直接改主检出。
- 主 Agent 按 `## Merge Order` 串行执行 `git apply --check`、`git apply` 和主检出 Project Verify。
- 每个工作包由另一次 `reviewer` spawn 检视 patch 绝对路径、`agent://<id>` 与 `history://<id>`。
- 隔离区没有 `target/`、根 `node_modules/` 或 `clients/app/node_modules/`；仓库级检查只在主检出执行。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | Freeze the v19 contract: remove preparation/session.start, add `session.delete` and `session.deleted`, define tombstone visibility, identify deletion by `(actor, requestId)`, require a terminal for every accepted delete, document direct create and empty-cleanup semantics. Covers `sync-command-handling`, `sync-event-delivery`, `sync-event-projection`, `web-session-preparation`. | none | wp-coder | `compatibility/`, `schemas/`, `fixtures/`, `crates/sync-protocol/src/`, `crates/identity-auth/`, `crates/identity-keystore/`, `scripts/check-contract-assets.mjs`, `scripts/probe-session-preparation.mjs`, `README.md`, `docs/SYNC_PROTOCOL.md`, `docs/INITIAL_DESIGN.md`, `docs/FRONTEND_DESIGN.md`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/NODE_LINK_PROTOCOL.md`, `docs/MODULE_ARCHITECTURE.md`, `docs/IDENTITY_AND_AUTH_CONTRACT.md`, `docs/SECURITY_DESIGN.md`, `docs/CONFIG_REFERENCE.md`, `docs/ACP_COMPATIBILITY_MATRIX.md`, `docs/SESSION_LIFECYCLE_DISCUSSION.md`, `docs/adr/`, `GLOSSARY.md` | Patch + `agent://WP1`: contract/fixture diff, removed preparation runtime and signing assets, authoritative doc updates, ADR-0016, schema-v19 notes | `npm run check` |
| WP2 | Implement core/storage semantics: direct-create active projection inputs, content facts, restart-seeded empty deadlines, tombstone-filtered store reads, `session.delete` transaction, v19 migration, preparation removal. Covers `local-session-lifecycle`, `sync-session-create`, `storage-schema-v2-migration`, `conversation-attachments`. | contract:WP1 | wp-coder | `crates/core/`, `crates/storage-sqlite/` | Patch + `agent://WP2`: core values/ports, focused tests, v19 migration and rollback tests, no preparation storage | `cargo test -p core -p storage-sqlite` |
| WP3 | Wire Owner runtime: delete admission/release/settle, linearized activation race handling, startup empty-session seeding, restart/deletion command recovery, created-active response; remove preparation from server, daemon, composition root and agent-host integration. Covers `local-session-lifecycle`, `sync-write-command-handling`, `sync-session-create`. | code:WP2 | wp-coder | `crates/core/src/model/session.rs`, `crates/core/src/broker.rs`, `crates/core/src/use_cases.rs`, `crates/core/src/lifecycle.rs`, `crates/core/src/lifecycle/tests.rs`, `crates/server/`, `crates/app/src/`, `crates/agent-host/` | Patch + `agent://WP3`: routes and daemon lifecycle, release classification, deterministic clock seam, focused integration tests | `cargo test -p server -p agent-host -p app --lib` |
| WP4 | Replace Web preparation with direct `session.create`, result-driven navigation, immediate view/resume activation, switch/reconnect activation, deletion projection/cache cleanup, and preparation draft removal. Covers `pwa-web-client`, `web-session-preparation`, `conversation-attachments`, `sync-event-projection`. | contract:WP1, code:WP3 | wp-coder | `clients/app/app/`, `clients/app/src/`, `clients/app/e2e/`, `clients/app/package.json`, `clients/app/package-lock.json`, `clients/app/tsconfig.json`, `clients/app/vitest.config.ts` | Patch + `agent://WP4`: direct-create flow, route/view sequencing, silent deletion handling, removed preparation stores, live E2E and tests | `npm run check` in `clients/app` |
| WP5 | Add cross-layer and real-browser acceptance: real Daemon + built PWA + controlled Agent, deterministic 30-minute advance, restart re-timing, race, empty delete, nonempty retain, navigation and switching. | code:WP1, code:WP2, code:WP3, code:WP4 | wp-tester | `crates/app/tests/`, `clients/app/e2e/`, `scripts/`, `docs/evidence/session-create-empty-cleanup.md` | Patch + `agent://WP5`: new/updated real-browser driver, cross-layer tests, evidence record with observable assertions | `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | Contract and authority-doc base for every code package; no write-scope overlap. | `npm run check` |
| 2 | WP2 | Core/storage implementation consumes WP1 contract without touching its files. | `cargo test -p core -p storage-sqlite` |
| 3 | WP3 | Server/daemon composes WP2 and WP1; no write-scope overlap. | `cargo test -p server -p agent-host -p app --lib` |
| 4 | WP4 | Web consumes WP1 and WP3; no write-scope overlap. | `npm run check` in `clients/app` |
| 5 | WP5 | Acceptance consumes all production patches; only E2E/evidence paths change. | `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run check` / repository root | OpenSpec strict validation, compatibility/schema/fixture consistency, boundary and ACP checks all pass. |
| PV2 | `npm run check:rust` / repository root | Workspace formatting, Clippy and Rust tests pass; no warnings promoted by the gate. |
| PV3 | `npm run check` / `clients/app` | TypeScript, unit tests and production Web export pass with preparation code removed. |
| PV4 | `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture` / repository root | Real browser observes direct create, navigation, activation, switching, exact 30-minute empty deletion, restart re-timing, and nonempty retention. |
| PV5 | `npm run verify` / repository root | Final aggregate gate passes after every patch is merged. |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | Closed vocabularies, removed preparation references, event/command schema consistency, no ACP `session/delete` claim, authority-doc drift. | No CRITICAL/MAJOR finding remains; all fixtures and references agree with the plan. |
| WP2 | reviewer | Empty/content classification, v19 atomicity, tombstone filters, replay/idempotency retention, configuration boundaries and transaction rollback. | No CRITICAL/MAJOR finding remains; store tests cover every visibility and event-family branch. |
| WP3 | reviewer | Admission/release/settle ordering, protection re-check, activation/delete races, restart seed, task cancellation and failure classification. | No CRITICAL/MAJOR finding remains; no deleted session can reopen or leak a live projection. |
| WP4 | reviewer | Direct mutation result handling, stale navigation/response rejection, viewer lease sequencing, cache and route cleanup, complete preparation removal. | No CRITICAL/MAJOR finding remains; UI has no preparation fallback or deletion notification. |
| WP5 | reviewer | Real components, deterministic clock, observable assertions, restart and race coverage, evidence that distinguishes executed from skipped checks. | No CRITICAL/MAJOR finding remains; E2E fails on each missing observable behavior and records actual execution. |

## Main E2E

```yaml
mode: required
reason: ""
basis: "The change modifies the Web create path, navigation/activation, session lists, and silent deletion; all are visible through a browser."
alternative_checks: []
```

The required run launches a real Daemon, controlled ACP Agent, built PWA, and real browser. It observes that a direct
create persists before the first message, navigates to the returned id, shows `active` without an idle flash, switches
between sessions, deletes an empty session after exactly 30 minutes of lifecycle time, retains a nonempty session, and
restarts the Daemon during the grace period. The 30-minute boundary uses the repository's deterministic clock seam;
default configuration and real-time arithmetic remain unit tested.

## Risks

- The change crosses every layer; strict write scopes prevent a cleanup-only patch from drifting into contract or
  production implementation files.
- Preparation identifiers are spread through schemas, fixtures, server composition, storage and Web state; WP1 and
  each implementation package must remove them together before the final contract gate.
- Tombstone filtering is easy to miss in a narrow read path; WP2 must exercise every `SessionStore` and `ReadView`
  entry, and WP5 must verify that browser and Node Link projections converge.
- Real E2E is slow and environment-sensitive; the harness must assert DOM and wire outcomes, not startup or screenshots,
  and must leave unexecuted gates explicit.
