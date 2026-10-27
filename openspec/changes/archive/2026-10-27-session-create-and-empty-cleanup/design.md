# Design

## Context

See `proposal.md` for the product motivation and `specs/**/spec.md` for the required behavior.

The current `session.create` path already has the right create-side split: persist the command admission, call the
Agent, then commit the formal session row, ACP session identifier, directory assignment, and terminal event in one
store transaction. The Web client still uses the separate preparation path, and the local lifecycle scanner currently
releases resources without deciding whether an owned session should be deleted.

Two existing constraints shape the deletion design:

- `owned_session` is the foreign-key anchor for session events and commands. A literal `DELETE` would cascade the
  `session.deleted` fact and `session.delete` terminal away, making replay and idempotent retry impossible.
- Session events and commands are part of the global replay and idempotency contracts. Their row presence must not
  depend on whether the session can still be read or activated.

The storage format and owned schema are currently v18. This change also removes the preparation model and
`owned_first_submission`, so the new schema is v19.

## Goals / Non-Goals

**Goals:**

- Reuse the existing atomic create path and make the created projection immediately active.
- Make activation, retention, release, and deletion one ordered Owner-side lifecycle for each owned session.
- Keep `session.deleted` replayable and `session.delete` idempotent after the active session is removed from every
  product-readable surface.
- Give restart, migration, failure, and race paths deterministic behavior.

**Non-Goals:**

- No ACP `session/delete` support. Deletion removes ACP Remote state only.
- No persisted access-node copy of an Owner deletion, command, content, or cursor fact.
- No new user-facing deletion action or notification.
- No deletion of the Agent's own internal storage, the host workspace, or attachment bytes still referenced by another
  live message.

## Decisions

### D1: Web creation uses the existing atomic `session.create` flow

The Web entry will dispatch `session.create` directly. The existing admission/execute/settle sequence remains the
authority:

1. Persist only the `(actor, requestId)` admission.
2. Ask the backend to establish the Agent session.
3. On success, commit the formal session row, Agent session identity, workspace assignment, and `command.completed`
   terminal in one transaction.
4. On a provably failed creation, commit a terminal without a session row. On an unprovable failure, commit
   `uncertain`; never synthesize a session row.

The lifecycle reservation is settled as ready in the same orchestration boundary before the completed result is
published. When the connection negotiated `core.local-session-lifecycle.v1`, the completed create result and the
following snapshot project `localActivity: active` directly. There is no intermediate durable `idle` projection for
the just-created endpoint.

The client accepts navigation only from the matching create terminal and only while that create operation is still
the current navigation intent. Once it reaches `/session/<id>`, it declares itself as the viewer and dispatches
`session.resume`; the already-ready endpoint is reused.

**Alternatives considered:** keeping preparation only as a UI prelude would preserve temporary identities and duplicate
the create transaction; creating the row before the backend call would persist failed sessions; publishing `idle`
first would produce a false transition for a resource that is already ready.

### D2: Retention is a per-session protection window

The same protection conditions apply to every local owned session:

- no live view lease;
- no unfinished or queued turn;
- no unresolved interaction;
- no accepted mutation whose execution or terminal is still unresolved; and
- no unconfirmed stop or release.

The first observed moment after all protections disappear starts a complete
`sessions.idle_timeout_ms` window. The default remains 30 minutes; explicit `0` disables time-based reclaim but not
capacity reclaim. Re-entering, activating, queueing work, or creating a pending interaction cancels the current
window. Mere page visibility heartbeats do not extend it after the user leaves.

At startup, the lifecycle runtime seeds a fresh deadline for every persisted empty session, regardless of whether its
old Agent process or runtime entry survived. Therefore a restart after 20 minutes grants another complete 30 minutes
and never triggers deletion from a persisted old deadline.

When that new deadline expires, a session whose managed Agent process is known to have exited counts as released even
if the restart left no runtime entry to close. A missing in-memory descriptor alone is not proof when process liveness
is unknown; that case remains `Uncertain`, keeps its capacity reservation and retries instead of deleting on an
unverified assumption.

Capacity pressure may release a session's runtime resource before the retention window ends, but it must not delete an
empty session early. Empty-session deletion occurs only through the time-based due path.

**Alternatives considered:** using a database timestamp would immediately expire sessions after restart and contradict
the selected behavior; using process uptime alone would miss sessions that became empty after startup.

### D3: Activation and deletion are serialized by the Owner lifecycle gate

View declaration, resume, release, and delete all enter the existing per-session serialization boundary. A delete
candidate must re-check the session existence and every protection condition immediately before it is released and
again inside the final store transaction.

The race outcomes are fixed:

- if activation or a new view declaration commits first, deletion abandons that attempt and waits for a new complete
  retention window;
- if deletion commits first, the session is a tombstone on every read path and the late activation returns
  `session.not_found` without opening an Agent;
- a late result from an older resource generation cannot become active or extend a newer generation's lease.

The update loop only observes a runtime generation that is still current. It must not hold a view lease or run an
Agent operation while waiting for database work.

### D4: Empty means no persisted conversation content

The store exposes one session-content fact to core, backed by a single query rather than UI heuristics. A session is
nonempty if it has any persisted message, user-input association, turn, interaction, terminal record, tool or other
structured-history event, or content-bearing Agent event.

The following metadata-only facts do not by themselves make a session nonempty:

- the session row and directory assignment;
- title, mode, and configuration;
- `session.created`;
- `session.info.changed`;
- `session.mode.changed`;
- `session.config.changed`;
- `session.activity.changed`;
- `session.updated`;
- `session.commands.changed`;
- `session.usage.changed`;
- `session.origin.online_changed`;
- `session.deleted`; and
- command acceptance/terminal events.

Persisted plan changes, terminal output, tool events, and other structured-history events are content.

The exclusion list belongs in one core/store contract and is covered by a table-driven test for every event family.
Unsubmitted uploads and browser drafts are not content. A saved user message or turn makes the session nonempty even if
the Agent later fails, disconnects, or becomes unavailable.

**Alternatives considered:** checking only `owned_message` would miss tool/terminal-only histories; treating every
event as content would make metadata updates prevent cleanup forever.

### D5: Deletion retains a minimal invisible tombstone

A successful deletion converts the active `owned_session` row into a tombstone instead of physically deleting that
single foreign-key anchor. The transition clears user/Agent identity data that is not needed for idempotency:

- title and custom title;
- Agent session identifier and canonical workspace path;
- workspace-directory assignment; and
- current mode and other display metadata.

The row retains only its stable `sessionId`, the minimum Agent identity needed to keep existing columns valid, the
command/event foreign-key anchor, version/timestamps, and a non-null `deleted_at`. A tombstone is not a session:
`load`, `list`, recovery, origin-epoch lookup, session-scoped history reads, snapshots, local-activity projection,
activation, and all session commands except idempotent `session.delete` treat it as absent.

The deletion transaction removes the session's messages, user-input associations, turns, interactions, timeline
anchors, media references, attachment bindings and submissions, content-bearing events, and other session history.
It preserves `owned_command` rows because `(actor, requestId)` idempotency must survive; command terminal events and
the new `session.deleted` event remain replayable. This keeps `terminalEventId` resolvable and avoids rewriting event
history merely because the session is gone.

**Alternatives considered:** hard-deleting `owned_session` would cascade the deletion fact and retry result; changing
every event/command foreign key to `SET NULL` would either require nulling session identity or rebuilding broad
schemas for one terminal case; a separate tombstone table would add a second event/command identity model. A minimal
logical tombstone keeps the current authority boundaries while removing every product-visible session record and all
conversation content.

### D6: `session.delete` is an ordered Owner-side lifecycle mutation

`session.delete` is registered as a Sync-only mutation gated by the existing
`core.local-session-lifecycle.v1`; no new feature ID is introduced. The target session comes from the command
envelope and the payload is an empty object. It is not exposed in Node Link and is not an ACP `session/delete`.

The lifecycle scanner submits a stable request as `LocalCli`. Admission is persisted before calling the real release
port. A `Released` outcome permits the final deletion transaction. `Failed` or `Uncertain` retains the live row and
capacity reservation and records the accepted command's `failed` or `uncertain` terminal; a later scan uses a new
request. If the daemon dies after admission, normal startup recovery marks the command `uncertain` and the session
remains.

After release, the database transaction performs the final linearized operation:

1. re-check that the row is non-tombstoned and empty and that no protection appeared;
2. if the re-check fails, append a durable `command.failed` terminal and leave the row/history intact; or
3. append the durable `session.deleted` event;
4. delete conversation/history rows and scrub the session row into a tombstone;
5. append `command.completed`; and
6. update the delete command's terminal fields and `result = { sessionId }`.

The event precedes the terminal within the global sequence. Failure at any step rolls back the tombstone, content
deletion, event sequence, and command terminal together. The resource is already idle if release succeeded, so a
transaction failure leaves a retryable row rather than losing history.

Repeating `session.delete` with the same request and fingerprint returns its stored result. Other delayed commands for
the tombstoned session return `session.not_found`, including retries of requests created before deletion; no command
may reopen or mutate a tombstone.

Every deletion port write carries the complete `Actor`, `RequestId`, and request fingerprint so storage can use the
single `(actor, requestId)` idempotency key and attribute the retained deletion events to the requester. An accepted
deletion can never disappear without a terminal: release failure becomes `failed` or `uncertain`, and a protection
that appears after release becomes `failed` while the session remains live. A new eligible scan uses a new request.

### D7: Deletion is a persistent event and a silent projection

Add the `session.deleted` event type with a typed view containing at least `sessionId`. The event keeps its ordinary
top-level `sessionId` and `sessionSequence`, durable policy, global sequence, origin, causation, and ACP-compatible
payload shape. It participates in normal replay and is filtered only by the existing feature gate; it is not encoded
as `session.updated` with a closed or hidden flag.

Clients apply deletion by stable `eventId`. Reapplying it is idempotent. The current session page leaves for the
directory without a toast or dialog; lists, directory rows, summaries, detail caches, activation state, draft state,
and navigation entries for that id are removed. A new snapshot omits tombstones, so cursor-expiry recovery converges
even when the deletion event is no longer replayable.

For Node Link imported sessions, the Access node removes the runtime catalog entry when the Owner's catalog no longer
contains it and returns the normal not-found result for later commands. It does not persist a deleted-session index;
reconnect rebuilds the authority view from the Owner.

### D8: Retire preparation end to end

The implementation removes the preparation capability rather than adapting it:

- remove `core.session-preparation.v1` and `session.start` from compatibility/docs/schemas/fixtures;
- remove preparation frames, credentials, runtimes, endpoints, attachment targets, ports, and browser draft storage;
- remove the `acp-remote/session-preparation/v1` signing domain and its authority entry points; and
- remove the `owned_first_submission` schema and code paths; and
- remove preparation-only UI state and tests.

Existing formal sessions, messages, events, commands, audit records, and ordinary attachments remain in place. The v18
to v19 migration drops `owned_first_submission`, removes preparation artifacts linked only to that schema, and deletes
`.preparation` staging files. Formal data is never converted into or inferred from preparation state.

## Risks / Trade-offs

- **[A tombstone can be mistaken for a live session]** -> Centralize the `deleted_at IS NULL` visibility predicate in
  every store read, add tombstone-read tests for every API, and make the core never expose a deleted row.
- **[Retaining command terminal events leaves operational metadata after cleanup]** -> Keep only the bounded metadata
  required by replay/idempotency contracts; all conversation content and user-visible history are removed.
- **[Release succeeds but the database transaction fails]** -> Keep the session row and retry deletion later; never
  report completion before commit.
- **[Restart loses retention timing]** -> Seed a new complete window from startup discovery rather than persisting and
  reusing the old deadline.
- **[Metadata events are misclassified as content]** -> Maintain one explicit exclusion table and test every known
  event family against it.
- **[Deletion races with navigation or resume]** -> Use the per-session gate for all three operations and re-check
  protections inside the final transaction.
- **[A real 30-minute E2E is slow and flaky]** -> Verify the default and boundaries at unit level, then drive a real
  Daemon and browser with the existing deterministic clock seam to advance exactly 30 minutes.

## Migration Plan

1. Stop the v18 daemon and back up the database plus attachment directory.
2. Start v19 in one write transaction: add `owned_session.deleted_at`, drop `owned_first_submission` and its indexes,
   remove `/attachments/.preparation`, and update file/owned schema versions to 19.
3. Preserve all formal session, message, event, command, audit, directory, and ordinary attachment rows and their
   existing sequences.
4. Make migration retry-safe: failure rolls back to the complete v18 shape, a successful repeated open performs no
   writes, and a v19 binary rejects a future version without writing.
5. Deploy the matching daemon and Web bundle together. The preparation capability and its browser state are removed
   without compatibility fallback.
6. Verify create/list/read/replay, a deleted tombstone, restart re-timing, and the preparation-upgrade fixture before
   accepting traffic.

Rollback requires restoring the pre-upgrade backup and deploying the matching v18 binaries; there is no in-place down
migration after v19 has accepted writes.

## Open Questions

None. The remaining work is implementation and acceptance, not a product or architecture choice.
