# Tasks

## 1. WP1: Freeze the v19 contract and authority documents

- [x] 1.1 Update Sync command/feature vocabularies and `sync-protocol` types to register the Sync-only `session.delete` mutation with an empty payload, remove `session.start`, and remove preparation feature IDs; verify the machine definitions and Rust vocabulary agree with `npm run check`.
- [x] 1.2 Add the typed `session.deleted` event and minimal `payload.view { sessionId }`, update Sync event schemas and fixtures for replay/idempotent application, and verify schema and fixture checks pass.
- [x] 1.3 Update `SYNC_PROTOCOL.md`, `INITIAL_DESIGN.md`, `FRONTEND_DESIGN.md`, `CORE_PORTS_AND_STORAGE.md`, `NODE_LINK_PROTOCOL.md`, `MODULE_ARCHITECTURE.md`, `IDENTITY_AND_AUTH_CONTRACT.md`, `SECURITY_DESIGN.md`, `CONFIG_REFERENCE.md`, `ACP_COMPATIBILITY_MATRIX.md`, and `README.md` with direct create, active projection, empty-session cleanup, tombstone visibility, v19 migration, Access non-persistence, and the prohibition on claiming ACP `session/delete`; verify every changed behavior is stated once as authority with consistent links and current contract counts.
- [x] 1.4 Remove preparation capability, control-frame, credential, signing domain, target, error, and attachment artifacts from `crates/sync-protocol`, `crates/identity-auth`, `crates/identity-keystore`, compatibility assets, schemas, fixtures, and preparation-only scripts; verify repository searches find no live preparation contract outside explicitly historical evidence and superseded ADRs.
- [x] 1.5 Write `docs/adr/0016-session-create-and-empty-cleanup.md` as an accepted decision that supersedes ADR-0014, update `SESSION_LIFECYCLE_DISCUSSION.md` and `GLOSSARY.md` to distinguish historical preparation from the current plan, and verify all new links resolve and no current-behavior section still presents preparation as the target.
- [x] 1.6 Have an independent reviewer inspect the contract delta for closed-vocabulary drift, accidental ACP capability claims, stale preparation references, and replay/idempotency gaps; verify no CRITICAL or MAJOR finding remains.

## 2. WP2: Implement core and storage semantics

- [x] 2.1 Add the core domain fact and port for persisted session content, the deletion write carrying complete actor/request identity, deletion outcomes, and restart-seeded empty-session deadlines, using concrete types rather than opaque JSON; verify focused core unit tests compile and cover each outcome.
- [x] 2.2 Centralize the live-session predicate and apply tombstone filtering to every `SessionStore` read, list, snapshot, recovery, origin lookup, summary, and session-scoped history path; verify table-driven tests prove a tombstone is invisible through every public read.
- [x] 2.3 Implement the single content-classification contract and table-driven coverage for user/Agent messages, turns, interactions, tool/terminal/structured events, and every metadata-only event family; verify metadata changes remain empty while each content family prevents deletion.
- [x] 2.4 Implement the delete transaction in the required order: on a failed final eligibility re-check persist `command.failed` without deleting; otherwise append `session.deleted`, remove conversation/history and scoped attachment bindings, scrub the row to a minimal tombstone, append `command.completed`, and persist the terminal/result; verify every accepted outcome is terminal and rollback tests restore all rows and sequences.
- [x] 2.5 Add the v18-to-v19 migration with `owned_session.deleted_at`, removal of `owned_first_submission`, `.preparation` staging cleanup, and version bumps; verify upgrade, repeated-open, rollback, future-version rejection, and preservation of all formal data in `storage-sqlite` tests.
- [x] 2.6 Remove preparation ports, records, stores, and migration branches from core and storage while retaining formal histories and ordinary attachments; verify no preparation symbols remain in those crates and the v18 fixture upgrades without conversion.
- [x] 2.7 Run `cargo test -p core -p storage-sqlite` and any focused migration tests; verify all pass, with no tombstone path, empty classification, transaction-order, or migration case left ignored.
- [x] 2.8 Have an independent reviewer inspect data visibility, content classification, transaction atomicity, sequence retention, migration rollback, and preparation removal; verify no CRITICAL or MAJOR finding remains.

## 3. WP3: Wire the Owner runtime and Daemon

- [x] 3.1 Publish a completed `session.create` result and matching snapshot with `localActivity: active` directly, settling the lifecycle reservation before broadcast; verify a server integration test observes no intermediate durable `idle` projection.
- [x] 3.2 Implement `session.delete` admission, real release, and final settlement for scanner-initiated cleanup, including `Released`, `Failed`, and `Uncertain` handling and startup recovery of admitted deletes; verify focused tests prove only confirmed release reaches the database deletion transaction.
- [x] 3.3 Route view declaration, resume, release, and delete through one per-session Owner lifecycle gate with protections re-checked immediately before release and inside the final transaction; verify activation-first and deletion-first race tests produce the specified outcomes and stale resource generations cannot revive a tombstone.
- [x] 3.4 Seed a fresh complete retention deadline from startup discovery for every persisted empty session, including sessions whose runtime entry or process did not survive, and treat an already-exited managed process as confirmed release when no other session uses it; verify restart, absent-runtime, and shared-process cases with deterministic clock tests.
- [x] 3.5 Remove preparation routing, runtime ownership, activation, attachment targets, recovery, and composition-root wiring from server, application, and agent-host while preserving existing resume and release behavior; verify obsolete preparation references are gone and affected modules compile.
- [x] 3.6 Add integration tests for silent deletion projection, `session.not_found` after deletion, nonempty retention, metadata-only empty cleanup, and Access/Node Link convergence without persisted remote deletion state; verify the tests fail if any of those observable contracts regress.
- [x] 3.7 Run `cargo test -p server -p agent-host -p app --lib`; verify all focused runtime, lifecycle, restart, race, and preparation-removal tests pass.
- [x] 3.8 Have an independent reviewer inspect admission/release/settle ordering, cancellation ownership, protection checks, restart timing, stale generations, and Node Link persistence boundaries; verify no CRITICAL or MAJOR finding remains.

## 4. WP4: Replace the Web preparation flow

- [x] 4.1 Replace Web new-session preparation with a single stable-id `session.create` request containing only directory source and configured Agent ID, with duplicate-click suppression and honest accepted/failed/uncertain states; verify client unit tests assert the exact payload and request reuse rules.
- [x] 4.2 Drive navigation from the matching completed create result, reject stale results after another navigation intent, declare the created session visible, and call `session.resume` while rendering `active` without an idle flash; verify focused state-machine tests cover create success, delayed result, and resume reuse.
- [x] 4.3 Declare leaving session A and viewing session B before activating B, ignore stale A responses, and reactivate the current visible session on refresh, visibility, and reconnect; verify switching and reconnect tests keep the route and view lease on B.
- [x] 4.4 Apply `session.deleted` idempotently by removing list, directory row, summary, detail, draft, activation, and navigation cache state; navigate from the deleted current page to `/` without a notice; verify projection tests cover current, non-current, and replayed deletion.
- [x] 4.5 Remove preparation pages, controls, browser persistence, attachment pre-upload state, the old `clients/app/e2e/session-preparation-live.mjs` flow, tests, and dependencies while retaining ordinary in-memory drafts on formal sessions; verify no preparation fallback or persistent draft remains in the client.
- [x] 4.6 Run `npm run check` in `clients/app`; verify TypeScript, unit tests, and the production Web export pass with the replacement flow.
- [x] 4.7 Have an independent reviewer inspect request identity, navigation ordering, view-lease handoff, stale response handling, complete cache cleanup, and preparation removal; verify no CRITICAL or MAJOR finding remains.

## 5. WP5: Prove the behavior through a real browser and Daemon

- [x] 5.1 Build a real-browser acceptance harness that launches a built PWA, real Daemon, controlled ACP Agent, and the repository clock seam; verify each component has an explicit readiness, shutdown, and failure path and no screenshot or fixed sleep substitutes for a contract assertion.
- [x] 5.2 Drive direct creation through the real Web entry, observe the formal session before any message, navigate to its returned ID, verify immediate active projection and resume, then switch among sessions and verify the target is activated while stale work cannot move the route.
- [x] 5.3 Advance lifecycle time through the deterministic seam, verify an empty session remains usable before 30 minutes, is released then silently removed at the boundary, and a nonempty or newly messaged session is retained; verify no user notification appears.
- [x] 5.4 Restart the Daemon during an empty session's grace period and verify it receives a fresh complete window, remains listed and activatable, and is deleted only after the new boundary; add activation/delete race coverage with stable observable results.
- [x] 5.5 Record actual command, environment, assertions, pass/fail/skip results, and residual limitations in `docs/evidence/session-create-empty-cleanup.md`; verify unexecuted checks are explicitly unfinished rather than reported as passes.
- [x] 5.6 Run `cargo test -p app --test session_create_empty_cleanup_browser -- --ignored --nocapture`; verify the real-browser suite passes with every required observable behavior asserted.
- [x] 5.7 Have an independent reviewer inspect real-component use, deterministic boundary control, restart and race coverage, observable assertions, and evidence accuracy; verify no CRITICAL or MAJOR finding remains.

## 6. Integration and delivery

- [x] 6.1 Apply WP1 through WP5 patches serially in the documented merge order, running `git apply --check`, `git apply`, and each work package's re-verification command before proceeding; verify every patch applies and every re-check passes before the next patch.
- [x] 6.2 Run `npm run verify` in the main checkout after all patches are merged; verify the repository's full check and Rust gates pass.
- [x] 6.3 Inspect the final diff for unintended changes to `docs/evidence/omp-acp-tool-order.md`, generated directories, or user-owned work; verify only planned files are included and `git diff --check` reports no whitespace errors.

## Workflow follow-up

- Run the required real-browser E2E and preserve its actual evidence before archive.
- Archive the OpenSpec change only after implementation, reviews, and all merge gates are complete.
