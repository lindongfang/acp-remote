import type { JsonValue } from "../../protocol/jws";
import {
  approvalQuantity,
  closed,
  closedWithOptional,
  literal,
  object,
  oneOf,
  requireBoolean,
  requireNullable,
  requireSessionId,
  requireSessionI64,
  requireSessionU64,
  requireText,
  responseResult,
} from "./primitives";

export type TranscriptSource = "live" | "transcript_cache" | "hybrid_resumed";
export type TranscriptSyncState =
  | "idle"
  | "revalidating"
  | "synced"
  | "degraded_capacity_full"
  | "degraded_unreachable"
  | "native_unrecoverable";
export type TranscriptCapabilityBoundary =
  | "unknown"
  | "replay_capable"
  | "resume_only"
  | "pure_memory";

export interface TranscriptCacheState {
  readonly transcriptSource: TranscriptSource;
  readonly syncState: TranscriptSyncState;
  readonly capabilityBoundary: TranscriptCapabilityBoundary;
  readonly continuable: boolean;
  readonly cacheWarning: boolean;
}

export interface SessionViewStatus {
  readonly mode: SessionViewMode;
  readonly freshness: SessionViewFreshness;
  readonly projection: SessionViewProjection;
  readonly join: SessionViewJoin;
  readonly historyIncomplete: boolean;
  readonly cacheRecoverable: boolean;
  readonly transcriptSource?: TranscriptSource;
  readonly syncState?: TranscriptSyncState;
  readonly capabilityBoundary?: TranscriptCapabilityBoundary;
  readonly continuable?: boolean;
  readonly cacheWarning?: boolean;
}

export type SessionViewMode =
  | "current_only"
  | "replay_requested"
  | "replaying"
  | "replay_complete"
  | "replay_incomplete";

export type SessionViewFreshness =
  | "current_observation"
  | "revalidating"
  | "stale";

export type SessionViewProjection =
  | "available"
  | "partial_known_structure"
  | "unavailable";

export type SessionViewJoin =
  | "proven_native_ids"
  | "rebuilt_no_stable_ids"
  | "unassociated_segments";

export interface SessionViewOpened {
  readonly v: 1;
  readonly type: "view.opened";
  readonly requestId: string;
  readonly streamId: string;
  readonly viewEpoch: string;
  readonly snapshotRevision: string;
  readonly capabilities: {
    readonly opaqueChunks: 1;
  };
  readonly status: SessionViewStatus;
}

export interface SessionViewClosed {
  readonly streamId: string;
  readonly alreadyClosed: boolean;
  readonly lastCommittedPacket: string;
  readonly startedPacket: string | null;
  readonly state: "closed" | "closing_after_started";
}

export type ApprovalValidity =
  | "current_pending"
  | "response_reserved"
  | "spent"
  | "invalid"
  | "unknown";

export type ApprovalAssociation =
  | { readonly kind: "unassociated" }
  | {
      readonly kind: "known_pre";
      readonly targetId: string;
      readonly loadEpoch: string;
      readonly ownerId: string;
    }
  | {
      readonly kind: "registered_session";
      readonly targetId: string;
    };

export interface ApprovalDecision {
  readonly registrationId: string;
  readonly operationId: string;
  readonly actorId: string;
  readonly admissionAuthGeneration: string;
  readonly choiceId: string | null;
  readonly knownKind:
    | "allow_once"
    | "allow_always"
    | "reject_once"
    | "reject_always"
    | null;
  readonly acceptedAtMs: string;
}

export interface ApprovalStatus {
  readonly approvalId: string;
  readonly changeSequence: string;
  readonly validity: ApprovalValidity;
  readonly association: ApprovalAssociation;
  readonly decision: ApprovalDecision | null;
  readonly delivery:
    | "not_attempted"
    | "in_progress"
    | "terminal"
    | "suppressed";
  readonly sourceClose:
    | "open"
    | "closing"
    | "physical_unconfirmed"
    | "metadata_unconfirmed"
    | "confirmed";
  readonly necessaryFactsMissing: boolean;
  readonly reconciliationRunning: boolean;
}

export interface ApprovalView {
  readonly approvalId: string;
  readonly source: {
    readonly sourceId: string;
    readonly runtimeId: string;
    readonly instanceId: string;
    readonly connectionEpoch: string;
    readonly protocolVersion: "1" | "2";
  };
  readonly context:
    | { readonly kind: "session_request" }
    | { readonly kind: "verified_input"; readonly dispatchId: string };
  readonly choices: readonly {
    readonly choiceId: string;
    readonly ordinal: string;
    readonly originalOptionId: string;
    readonly originalKind: string;
    readonly selectable: boolean;
  }[];
  readonly content:
    | { readonly availability: "available"; readonly rawJson: string }
    | { readonly availability: "unavailable" };
  readonly limitations: readonly string[];
  readonly status: ApprovalStatus;
}

export interface ApprovalListResult {
  readonly items: readonly ApprovalView[];
}

export interface ApprovalRegistration {
  readonly approvalId: string;
  readonly choiceId: string;
  readonly approvalRegistrationId: string;
  readonly state: "registered";
}

export interface ApprovalDecisionResult {
  readonly approvalId: string;
  readonly approvalRegistrationId: string;
  readonly state: "accepted" | "existing";
}

export function parseSessionViewOpened(value: JsonValue): SessionViewOpened {
  return responseResult(() =>
    closed(value, {
      v: literal(1),
      type: oneOf("view.opened"),
      requestId: requireSessionId,
      streamId: requireSessionId,
      viewEpoch: requireSessionId,
      snapshotRevision: requireSessionU64,
      capabilities: (capabilities) =>
        closed(capabilities, { opaqueChunks: literal(1) }),
      status: (status) =>
        closedWithOptional(
          status,
          {
            mode: oneOf(
              "current_only",
              "replay_requested",
              "replaying",
              "replay_complete",
              "replay_incomplete",
            ),
            freshness: oneOf(
              "current_observation",
              "revalidating",
              "stale",
            ),
            projection: oneOf(
              "available",
              "partial_known_structure",
              "unavailable",
            ),
            join: oneOf(
              "proven_native_ids",
              "rebuilt_no_stable_ids",
              "unassociated_segments",
            ),
            historyIncomplete: requireBoolean,
            cacheRecoverable: requireBoolean,
          },
          {
            transcriptSource: oneOf("live", "transcript_cache", "hybrid_resumed"),
            syncState: oneOf(
              "idle",
              "revalidating",
              "synced",
              "degraded_capacity_full",
              "degraded_unreachable",
              "native_unrecoverable",
            ),
            capabilityBoundary: oneOf(
              "unknown",
              "replay_capable",
              "resume_only",
              "pure_memory",
            ),
            continuable: requireBoolean,
            cacheWarning: requireBoolean,
          },
        ),
    }),
  );
}

export function parseSessionViewClosed(value: JsonValue): SessionViewClosed {
  return responseResult(() =>
    closed(value, {
      streamId: requireSessionId,
      alreadyClosed: requireBoolean,
      lastCommittedPacket: requireSessionU64,
      startedPacket: requireNullable(requireSessionU64),
      state: oneOf("closed", "closing_after_started"),
    }),
  );
}

function approvalAssociation(
  value: JsonValue | undefined,
): ApprovalAssociation {
  const source = object(value);
  switch (
    oneOf(
      "unassociated",
      "known_pre",
      "registered_session",
    )(source.kind)
  ) {
    case "unassociated":
      return closed(value, { kind: oneOf("unassociated") });
    case "known_pre":
      return closed(value, {
        kind: oneOf("known_pre"),
        targetId: requireSessionId,
        loadEpoch: requireSessionId,
        ownerId: requireSessionId,
      });
    case "registered_session":
      return closed(value, {
        kind: oneOf("registered_session"),
        targetId: requireSessionId,
      });
  }
}

function approvalDecision(
  value: JsonValue | undefined,
): ApprovalDecision | null {
  if (value === null) return null;
  return closed(value, {
    registrationId: requireSessionId,
    operationId: requireSessionId,
    actorId: requireSessionId,
    admissionAuthGeneration: requireSessionI64,
    choiceId: requireNullable(requireSessionId),
    knownKind: requireNullable(
      oneOf(
        "allow_once",
        "allow_always",
        "reject_once",
        "reject_always",
      ),
    ),
    acceptedAtMs: requireSessionI64,
  });
}

function approvalStatus(value: JsonValue | undefined): ApprovalStatus {
  return closed(value, {
    approvalId: requireSessionId,
    changeSequence: requireSessionU64,
    validity: oneOf(
      "current_pending",
      "response_reserved",
      "spent",
      "invalid",
      "unknown",
    ),
    association: approvalAssociation,
    decision: approvalDecision,
    delivery: oneOf(
      "not_attempted",
      "in_progress",
      "terminal",
      "suppressed",
    ),
    sourceClose: oneOf(
      "open",
      "closing",
      "physical_unconfirmed",
      "metadata_unconfirmed",
      "confirmed",
    ),
    necessaryFactsMissing: requireBoolean,
    reconciliationRunning: requireBoolean,
  });
}

export function parseApprovalStatusResult(
  value: JsonValue,
): ApprovalStatus {
  return responseResult(() => approvalStatus(value));
}

function approvalView(value: JsonValue | undefined): ApprovalView {
  const source = object(value);
  const contentShape = object(source.content);
  const availability = oneOf("available", "unavailable")(
    contentShape.availability,
  );
  const content =
    availability === "available"
      ? closed(source.content, {
          availability: oneOf("available"),
          rawJson: requireText,
        })
      : closed(source.content, { availability: oneOf("unavailable") });
  return closed(value, {
    approvalId: requireSessionId,
    source: (entry) =>
      closed(entry, {
        sourceId: requireSessionId,
        runtimeId: requireSessionId,
        instanceId: requireSessionId,
        connectionEpoch: requireSessionId,
        protocolVersion: oneOf("1", "2"),
      }),
    context: (entry) => {
      const context = object(entry);
      switch (oneOf("session_request", "verified_input")(context.kind)) {
        case "session_request":
          return closed(entry, { kind: oneOf("session_request") });
        case "verified_input":
          return closed(entry, {
            kind: oneOf("verified_input"),
            dispatchId: requireSessionId,
          });
      }
    },
    choices: (entry) =>
      approvalQuantity(entry, (choice) =>
        closed(choice, {
          choiceId: requireSessionId,
          ordinal: requireSessionU64,
          originalOptionId: requireText,
          originalKind: requireText,
          selectable: requireBoolean,
        }),
      ),
    content: () => content,
    limitations: (entry) => approvalQuantity(entry, requireText),
    status: approvalStatus,
  });
}

export function parseApprovalListResult(
  value: JsonValue,
): ApprovalListResult {
  return responseResult(() =>
    closed(value, {
      items: (items) => approvalQuantity(items, approvalView),
    }),
  );
}

export function parseApprovalResult(value: JsonValue): ApprovalView {
  return responseResult(() => approvalView(value));
}

export function parseApprovalRegistration(
  value: JsonValue,
): ApprovalRegistration {
  return responseResult(() =>
    closed(value, {
      approvalId: requireSessionId,
      choiceId: requireSessionId,
      approvalRegistrationId: requireSessionId,
      state: oneOf("registered"),
    }),
  );
}

export function parseApprovalDecisionResult(
  value: JsonValue,
): ApprovalDecisionResult {
  return responseResult(() =>
    closed(value, {
      approvalId: requireSessionId,
      approvalRegistrationId: requireSessionId,
      state: oneOf("accepted", "existing"),
    }),
  );
}
