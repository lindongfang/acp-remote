import {
  decodeBase64urlStrict,
  encodeBase64url,
  type JsonObject,
  type JsonValue,
  parseStrictJsonObject,
} from "../protocol/jws";
import { decodeViewFrame, type ViewDataFrame } from "../views/binary";
import { ViewDecodeError } from "../views/errors";
import {
  decodeInputAcknowledgement,
  InputTransferError,
  type InputAcknowledgement,
} from "./input-transfer";
import { MAX_MANAGEMENT_FRAME_BYTES } from "./protocol";

const ENCODER = new TextEncoder();
const MAX_SOURCE_TEXT_BYTES = 4096;
const U64_MAX = "18446744073709551615";
const I64_MAX = "9223372036854775807";
const I64_MIN_MAGNITUDE = "9223372036854775808";

type SessionProtocolErrorCode =
  | "invalid_command"
  | "invalid_id"
  | "invalid_decimal"
  | "invalid_frame"
  | "invalid_direction"
  | "binary_before_auth"
  | "allocation_failed";

export class SessionProtocolError extends Error {
  readonly code: SessionProtocolErrorCode;

  constructor(code: SessionProtocolErrorCode) {
    super(code);
    this.name = "SessionProtocolError";
    this.code = code;
  }
}

/** A syntactically valid platform ID grants no authority or resource ownership. */
export function requireSessionId(value: unknown): string {
  if (typeof value !== "string" || value.length !== 22) {
    throw new SessionProtocolError("invalid_id");
  }
  try {
    const bytes = decodeBase64urlStrict(value);
    if (bytes.length !== 16 || encodeBase64url(bytes) !== value) {
      throw new SessionProtocolError("invalid_id");
    }
    return value;
  } catch (error) {
    if (error instanceof RangeError) {
      throw new SessionProtocolError("allocation_failed");
    }
    throw new SessionProtocolError("invalid_id");
  }
}

function magnitudeWithin(value: string, maximum: string): boolean {
  return value.length < maximum.length ||
    (value.length === maximum.length && value <= maximum);
}

/** Keep the original canonical decimal string, including values above 2^53. */
export function requireSessionU64(value: unknown): string {
  if (
    typeof value !== "string" || value.length > U64_MAX.length ||
    !/^(?:0|[1-9][0-9]*)$/u.test(value) || !magnitudeWithin(value, U64_MAX)
  ) {
    throw new SessionProtocolError("invalid_decimal");
  }
  return value;
}

export function requireSessionI64(value: unknown): string {
  if (typeof value !== "string" || value.length > 20) {
    throw new SessionProtocolError("invalid_decimal");
  }
  if (value.startsWith("-")) {
    const magnitude = value.slice(1);
    if (/^[1-9][0-9]*$/u.test(magnitude) && magnitudeWithin(magnitude, I64_MIN_MAGNITUDE)) {
      return value;
    }
  } else if (/^(?:0|[1-9][0-9]*)$/u.test(value) && magnitudeWithin(value, I64_MAX)) {
    return value;
  }
  throw new SessionProtocolError("invalid_decimal");
}

export type QueueMutationDto =
  | { readonly kind: "submit"; readonly bodyId: string }
  | { readonly kind: "edit"; readonly itemId: string; readonly expectedRevision: string; readonly bodyId: string }
  | { readonly kind: "cancel" | "prioritize"; readonly itemId: string; readonly expectedRevision: string }
  | { readonly kind: "resume" };

export type SessionStopReason =
  | "end_turn"
  | "max_tokens"
  | "max_turn_requests"
  | "refusal"
  | "cancelled"
  | "error"
  | "unknown";

export type SessionPriorityFailure =
  | "selection_changed"
  | "interrupt_unconfirmed";

export interface SessionQueueEntry {
  readonly itemId: string;
  readonly revision: string;
  readonly submitterId: string;
  readonly editorId: string;
  readonly suspended: boolean;
}

export type SessionQueueProcessingPhase =
  | "await_begin"
  | "begin_unconfirmed"
  | "ready_to_apply"
  | "await_settle"
  | "settle_unconfirmed"
  | "faulted";

export interface SessionQueueProcessing {
  readonly phase: SessionQueueProcessingPhase;
  readonly applied: boolean;
  readonly registrationId: string;
}

export interface SessionQueueSnapshot {
  readonly targetId: string;
  readonly failureSuspended: boolean;
  readonly stopReason: SessionStopReason | null;
  readonly priorityFailure: SessionPriorityFailure | null;
  readonly waiting: readonly SessionQueueEntry[];
  readonly active: SessionQueueEntry | null;
  readonly priority: string | null;
  readonly processing: SessionQueueProcessing | null;
}

export type SessionQueueAction =
  | "input.submit"
  | "input.edit"
  | "input.cancel"
  | "input.prioritize"
  | "input.resume";

export interface SessionRegistration {
  readonly registrationId: string;
  readonly state: "registered";
  readonly action: SessionQueueAction;
  readonly targetId: string;
  readonly registerAuthGeneration: string;
  readonly registeredAtMs: string;
}

export interface SessionResumeResult {
  readonly registrationId: string;
  readonly restored: string;
  readonly returned: boolean;
}

export interface InputUploadBeginResult {
  readonly uploadId: string;
}

interface InputUploadStatusBase {
  readonly uploadId: string;
  readonly nextOffset: string;
}

export type InputUploadStatus =
  | InputUploadStatusBase & {
      readonly phase: "receiving" | "preparing";
    }
  | InputUploadStatusBase & {
      readonly phase: "ready";
      readonly bodyId: string;
    }
  | InputUploadStatusBase & {
      readonly phase: "failed";
      readonly error: string;
    };

export interface InputUploadFinishResult {
  readonly uploadId: string;
}

export interface InputUploadReleaseResult {
  readonly released: true;
}

export interface CatalogUploadBeginResult {
  readonly uploadId: string;
}

export type CatalogUploadStatus =
  | InputUploadStatusBase & {
      readonly phase: "receiving" | "preparing" | "ready";
    }
  | InputUploadStatusBase & {
      readonly phase: "failed";
      readonly error: string;
    };

export interface CatalogUploadFinishResult {
  readonly uploadId: string;
}

export interface CatalogUploadReleaseResult {
  readonly released: true;
}

export interface CatalogRegistration {
  readonly catalogRequestId: string;
  readonly state: "registered";
}

export type CatalogFactResult =
  | {
      readonly kind: "harness" | "profile";
      readonly harnessId: string;
      readonly sourceId: string;
      readonly profileId: string;
      readonly profileRevision: string;
    }
  | {
      readonly kind: "target";
      readonly targetId: string;
      readonly workspaceId: string;
      readonly harnessId: string;
      readonly sourceId: string;
      readonly profileId: string;
      readonly profileRevision: string;
    };

export interface CatalogFactMetadata {
  readonly catalogRequestId: string;
  readonly actorId: string;
  readonly action: "install_harness" | "publish_profile" | "bind_target_initial";
  readonly state: "registered" | "committed" | "rejected";
  readonly registeredAtMs: string;
  readonly registerAuthGeneration: string;
  readonly admissionAuthGeneration: string | null;
  readonly resultAtMs: string | null;
  readonly result: CatalogFactResult | null;
}

export interface CatalogSqlSlotObservation {
  readonly role:
    | "identity_read"
    | "registered_snapshot"
    | "register_write"
    | "apply_write"
    | "query_read"
    | "configuration_read"
    | "recovery_read";
  readonly transaction:
    | "no_transaction"
    | "begun_read"
    | "begun_write"
    | "commit_polling"
    | "commit_confirmed"
    | "rollback_confirmed"
    | "effect_unknown";
  readonly cleanup:
    | "no_detached_connection"
    | "pending_original"
    | "confirmed_original"
    | "unconfirmed_original";
}

export interface CatalogSqlObservationSet {
  readonly operation:
    | "register"
    | "apply"
    | "query"
    | "resolve"
    | "resolve_read"
    | "reconcile";
  readonly slots: readonly [
    CatalogSqlSlotObservation | null,
    CatalogSqlSlotObservation | null,
  ];
}

export interface CatalogReceiptMetadata {
  readonly fact: CatalogFactMetadata;
  readonly auditWarning: "none" | "unavailable" | "full";
  readonly sqlObservation: CatalogSqlObservationSet;
}

export interface WorkspaceRecord {
  readonly workspaceId: string;
  readonly canonicalPath: string;
}

export interface ProductSourceSummary {
  readonly sourceId: string;
  readonly name: string;
}

export interface HarnessSummary {
  readonly harnessId: string;
  readonly name: string;
  readonly sourceId: string;
}

export interface ProfileSummary {
  readonly profileId: string;
  readonly revision: string;
  readonly harnessId: string;
  readonly sourceId: string;
}

export interface TargetBindingSummary {
  readonly profileId: string;
  readonly profileRevision: string;
  readonly workspaceId: string;
  readonly launchPath: string;
  readonly observedCanonicalPath: string;
}

export interface ProductTargetRecord {
  readonly targetId: string;
  readonly sourceId: string;
  readonly nativeSessionId: string;
  readonly workspaceId: string | null;
  readonly binding: TargetBindingSummary | null;
}

export interface CatalogInventory {
  readonly workspaces: readonly WorkspaceRecord[];
  readonly sources: readonly ProductSourceSummary[];
  readonly harnesses: readonly HarnessSummary[];
  readonly profiles: readonly ProfileSummary[];
  readonly targets: readonly ProductTargetRecord[];
}

export type SessionInputOutcome =
  | {
      readonly kind:
        | "queued"
        | "direct_selected"
        | "edited"
        | "cancelled"
        | "prioritized";
      readonly itemId: string;
      readonly revision: string;
    }
  | {
      readonly kind: "resumed";
      readonly restored: string;
    }
  | {
      readonly kind: "rejected";
      readonly reason: string;
    };

export type SessionConsumeResult =
  | {
      readonly kind: "observed";
      readonly fact: SessionInputFact;
    }
  | {
      readonly kind: "resolved";
      readonly returned: boolean;
      readonly request: SessionInputFact;
    }
  | {
      readonly kind: "pending";
      readonly registrationId: string;
    }
  | {
      readonly kind: "refused";
      readonly reason: string;
      readonly returned: boolean;
    };

export type SessionInputFact =
  | {
      readonly registrationId: string;
      readonly state: "registered";
    }
  | {
      readonly registrationId: string;
      readonly state: "applying";
      readonly operationId: string;
      readonly runtimeId: string;
      readonly admittedAtMs: string;
    }
  | {
      readonly registrationId: string;
      readonly state: "settled";
      readonly outcome: SessionInputOutcome;
    }
  | {
      readonly registrationId: string;
      readonly state: "interrupted";
    }
  | {
      readonly registrationId: string;
      readonly state: "retired";
    };

export interface SessionQueuedBodyDescriptor {
  readonly streamKind: "queued_input_body";
  readonly streamId: string;
  readonly documentId: string;
  readonly targetId: string;
  readonly revision: string;
  readonly totalBytes: string;
  readonly encoding: "utf8_json";
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
  readonly status: {
    readonly mode: SessionViewMode;
    readonly freshness: SessionViewFreshness;
    readonly projection: SessionViewProjection;
    readonly join: SessionViewJoin;
    readonly historyIncomplete: boolean;
    readonly cacheRecoverable: boolean;
  };
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
  | { readonly kind: "registered_session"; readonly targetId: string };

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

export interface NativeReleaseRegistered {
  readonly releaseOperationId: string;
  readonly targetId: string;
  readonly state: "registered";
}
export interface SessionCapacity {
  readonly limit: number;
  readonly occupied: number;
  readonly reserved: number;
  readonly closedPending: number;
  readonly automaticRecycling: boolean;
}

export interface SourceLaunchSelectionInput {
  readonly sourceId: string;
  readonly harnessId: string;
  readonly profileId: string;
  readonly profileRevision: string;
  readonly workspaceId: string;
  readonly launchPath: string;
}

export type SourceOperationInput =
  | { readonly kind: "discover"; readonly cursor: string | null }
  | { readonly kind: "create" };

export interface SourceSessionRegistrationMetadata {
  readonly sourceOperationId: string;
  readonly actorId: string;
  readonly action: "discover" | "create";
  readonly sourceId: string;
  readonly harnessId: string;
  readonly profileId: string;
  readonly profileRevision: string;
  readonly workspaceId: string;
  readonly registeredAtMs: string;
}

export interface SourceSessionRegistration extends SourceSessionRegistrationMetadata {
  readonly state: "registered";
}

export type SourceSessionRpcId =
  | { readonly kind: "integer"; readonly value: string }
  | { readonly kind: "string"; readonly value: string };

export type SourceSessionTransportReason =
  | "cancelled"
  | "deadline"
  | "fence_rejected"
  | "connection_closed"
  | "abandoned"
  | "io"
  | "invalid_fence"
  | "connection_lost"
  | "restart";

export type SourceSessionTransportFact =
  | { readonly kind: "zero"; readonly reason: SourceSessionTransportReason }
  | {
      readonly kind: "unknown";
      readonly accepted: string;
      readonly cause: SourceSessionTransportReason;
    }
  | { readonly kind: "written" };

export interface SourceSessionResponseFact {
  readonly kind: "created" | "sessions" | "remote_error";
  readonly nativeSessionId: string | null;
  readonly nativeKeyAvailable: boolean;
  readonly remoteErrorCode: string | null;
  readonly sequence: string;
  readonly receipt: string;
  readonly resultBytes: string;
  readonly ack: "pending" | "confirmed" | "rejected" | "unknown";
}

export type SourceSessionFailureClass =
  | "configuration"
  | "protocol"
  | "source"
  | "metadata"
  | "ownership"
  | "allocation"
  | "interrupted";

export interface SourceSessionFact {
  readonly registration: SourceSessionRegistrationMetadata;
  readonly resultAvailable: boolean;
  readonly state: "registered" | "owned" | "terminal" | "interrupted";
  readonly outcome: "pending" | "succeeded" | "refused" | "unknown";
  readonly runtimeId: string | null;
  readonly admissionAuthGeneration: string | null;
  readonly sourceInstanceId: string | null;
  readonly connectionEpoch: string | null;
  readonly protocolVersion: "1" | "2" | null;
  readonly rpcId: SourceSessionRpcId | null;
  readonly frameLength: string | null;
  readonly transport: SourceSessionTransportFact | null;
  readonly response: SourceSessionResponseFact | null;
  readonly failure: SourceSessionFailureClass | null;
}

export interface SourceSessionResultDescriptor {
  readonly streamKind: "native_session_result";
  readonly streamId: string;
  readonly documentId: string;
  readonly targetId: null;
  readonly revision: null;
  readonly totalBytes: string;
  readonly encoding: "utf8_json";
}

export interface SourceSessionResultReleaseResult {
  readonly released: true;
}

export type NativeReleaseStatus =
  | {
      readonly releaseOperationId: string;
      readonly targetId: string;
      readonly state: "registered" | "releasing" | "released";
      readonly error: null;
    }
  | {
      readonly releaseOperationId: string;
      readonly targetId: string;
      readonly state: "failed";
      readonly error: string;
    };

function requireBoolean(value: JsonValue | undefined): boolean {
  if (typeof value !== "boolean") {
    throw new SessionProtocolError("invalid_frame");
  }
  return value;
}

function requireNullable<T>(
  parse: (value: JsonValue | undefined) => T,
): (value: JsonValue | undefined) => T | null {
  return (value) => value === null ? null : parse(value);
}

function requireArray<T>(
  parse: (value: JsonValue | undefined, index: number) => T,
): (value: JsonValue | undefined) => readonly T[] {
  return (value) => {
    if (!Array.isArray(value) || value.length > 3) {
      throw new SessionProtocolError("invalid_frame");
    }
    return Object.freeze(value.map(parse));
  };
}

const stopReason = oneOf(
  "end_turn",
  "max_tokens",
  "max_turn_requests",
  "refusal",
  "cancelled",
  "error",
  "unknown",
);
const priorityFailure = oneOf("selection_changed", "interrupt_unconfirmed");

function queueEntry(value: JsonValue | undefined): SessionQueueEntry {
  return closed(value, {
    itemId: requireSessionId,
    revision: requireSessionU64,
    submitterId: requireSessionId,
    editorId: requireSessionId,
    suspended: requireBoolean,
  });
}

function queueProcessing(value: JsonValue | undefined): SessionQueueProcessing {
  return closed(value, {
    phase: oneOf(
      "await_begin",
      "begin_unconfirmed",
      "ready_to_apply",
      "await_settle",
      "settle_unconfirmed",
      "faulted",
    ),
    applied: requireBoolean,
    registrationId: requireSessionId,
  });
}

export function parseSessionQueue(value: JsonValue): SessionQueueSnapshot {
  return responseResult(() =>
    closed(value, {
      targetId: requireSessionId,
      failureSuspended: requireBoolean,
      stopReason: requireNullable(stopReason),
      priorityFailure: requireNullable(priorityFailure),
      waiting: requireArray(queueEntry),
      active: requireNullable(queueEntry),
      priority: requireNullable(requireSessionId),
      processing: requireNullable(queueProcessing),
    }),
  );
}

export function parseSessionRegistration(value: JsonValue): SessionRegistration {
  return responseResult(() =>
    closed(value, {
      registrationId: requireSessionId,
      state: oneOf("registered"),
      action: oneOf(
        "input.submit",
        "input.edit",
        "input.cancel",
        "input.prioritize",
        "input.resume",
      ),
      targetId: requireSessionId,
      registerAuthGeneration: requireSessionU64,
      registeredAtMs: requireSessionI64,
    }),
  );
}

export function parseSessionResumeResult(value: JsonValue): SessionResumeResult {
  return responseResult(() => {
    const result = closed(value, {
      state: oneOf("settled"),
      returned: requireBoolean,
      request: (request) => closed(request, {
        registrationId: requireSessionId,
        state: oneOf("settled"),
        outcome: (outcome) => closed(outcome, {
          kind: oneOf("resumed"),
          restored: requireSessionU64,
        }),
      }),
    });
    return Object.freeze({
      registrationId: result.request.registrationId,
      restored: result.request.outcome.restored,
      returned: result.returned,
    });
  });
}

export function parseInputUploadBeginResult(
  value: JsonValue,
): InputUploadBeginResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseInputUploadStatus(value: JsonValue): InputUploadStatus {
  return responseResult(() => {
    const source = object(value);
    const phase = oneOf("receiving", "preparing", "ready", "failed")(
      source.phase,
    );
    switch (phase) {
      case "receiving":
      case "preparing":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf(phase),
          nextOffset: requireSessionU64,
        });
      case "ready":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf("ready"),
          nextOffset: requireSessionU64,
          bodyId: requireSessionId,
        });
      case "failed":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf("failed"),
          nextOffset: requireSessionU64,
          error: requireText,
        });
    }
  });
}

export function parseInputUploadFinishResult(
  value: JsonValue,
): InputUploadFinishResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseInputUploadReleaseResult(
  value: JsonValue,
): InputUploadReleaseResult {
  return responseResult(() =>
    closed(value, {
      released: requireTrue,
    }),
  );
}

export function parseCatalogUploadBeginResult(
  value: JsonValue,
): CatalogUploadBeginResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseCatalogUploadStatus(
  value: JsonValue,
): CatalogUploadStatus {
  return responseResult(() => {
    const source = object(value);
    const phase = oneOf("receiving", "preparing", "ready", "failed")(
      source.phase,
    );
    switch (phase) {
      case "receiving":
      case "preparing":
      case "ready":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf(phase),
          nextOffset: requireSessionU64,
        });
      case "failed":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf("failed"),
          nextOffset: requireSessionU64,
          error: requireText,
        });
    }
  });
}

export function parseCatalogUploadFinishResult(
  value: JsonValue,
): CatalogUploadFinishResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseCatalogUploadReleaseResult(
  value: JsonValue,
): CatalogUploadReleaseResult {
  return responseResult(() =>
    closed(value, {
      released: requireTrue,
    }),
  );
}

export function parseCatalogRegistration(
  value: JsonValue,
): CatalogRegistration {
  return responseResult(() =>
    closed(value, {
      catalogRequestId: requireSessionId,
      state: oneOf("registered"),
    }),
  );
}

function catalogFactResult(
  value: JsonValue | undefined,
): CatalogFactResult | null {
  if (value === null) return null;
  const source = object(value);
  switch (oneOf("harness", "profile", "target")(source.kind)) {
    case "harness":
    case "profile":
      return closed(value, {
        kind: oneOf("harness", "profile"),
        harnessId: requireSessionId,
        sourceId: requireSessionId,
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
      });
    case "target":
      return closed(value, {
        kind: oneOf("target"),
        targetId: requireSessionId,
        workspaceId: requireSessionId,
        harnessId: requireSessionId,
        sourceId: requireSessionId,
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
      });
  }
}

function catalogFactMetadataValue(
  value: JsonValue | undefined,
): CatalogFactMetadata {
  return closed(value, {
    catalogRequestId: requireSessionId,
    actorId: requireSessionId,
    action: oneOf("install_harness", "publish_profile", "bind_target_initial"),
    state: oneOf("registered", "committed", "rejected"),
    registeredAtMs: requireSessionI64,
    registerAuthGeneration: requireSessionI64,
    admissionAuthGeneration: requireNullable(requireSessionI64),
    resultAtMs: requireNullable(requireSessionI64),
    result: catalogFactResult,
  });
}

export function parseCatalogFactMetadata(
  value: JsonValue,
): CatalogFactMetadata {
  return responseResult(() => catalogFactMetadataValue(value));
}

function catalogSqlSlot(
  value: JsonValue | undefined,
): CatalogSqlSlotObservation | null {
  if (value === null) return null;
  return closed(value, {
    role: oneOf(
      "identity_read",
      "registered_snapshot",
      "register_write",
      "apply_write",
      "query_read",
      "configuration_read",
      "recovery_read",
    ),
    transaction: oneOf(
      "no_transaction",
      "begun_read",
      "begun_write",
      "commit_polling",
      "commit_confirmed",
      "rollback_confirmed",
      "effect_unknown",
    ),
    cleanup: oneOf(
      "no_detached_connection",
      "pending_original",
      "confirmed_original",
      "unconfirmed_original",
    ),
  });
}

function catalogSqlObservation(
  value: JsonValue | undefined,
): CatalogSqlObservationSet {
  return closed(value, {
    operation: oneOf(
      "register",
      "apply",
      "query",
      "resolve",
      "resolve_read",
      "reconcile",
    ),
    slots: (slots) => {
      if (!Array.isArray(slots) || slots.length !== 2) {
        throw new SessionProtocolError("invalid_frame");
      }
      return Object.freeze([
        catalogSqlSlot(slots[0]),
        catalogSqlSlot(slots[1]),
      ] as const);
    },
  });
}

export function parseCatalogReceiptMetadata(
  value: JsonValue,
): CatalogReceiptMetadata {
  return responseResult(() =>
    closed(value, {
      fact: catalogFactMetadataValue,
      auditWarning: oneOf("none", "unavailable", "full"),
      sqlObservation: catalogSqlObservation,
    }),
  );
}

function workspaceRecordValue(
  value: JsonValue | undefined,
): WorkspaceRecord {
  return closed(value, {
    workspaceId: requireSessionId,
    canonicalPath: requireText,
  });
}

export function parseWorkspaceRecord(value: JsonValue): WorkspaceRecord {
  return responseResult(() => workspaceRecordValue(value));
}

function productTargetRecordValue(
  value: JsonValue | undefined,
): ProductTargetRecord {
  return closed(value, {
    targetId: requireSessionId,
    sourceId: requireSessionId,
    nativeSessionId: requireText,
    workspaceId: requireNullable(requireSessionId),
    binding: requireNullable((binding) =>
      closed(binding, {
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
        workspaceId: requireSessionId,
        launchPath: requireText,
        observedCanonicalPath: requireText,
      }),
    ),
  });
}

export function parseProductTargetRecord(
  value: JsonValue,
): ProductTargetRecord {
  return responseResult(() => productTargetRecordValue(value));
}

export function parseCatalogInventory(value: JsonValue): CatalogInventory {
  return responseResult(() =>
    closed(value, {
      workspaces: (items) => approvalQuantity(items, workspaceRecordValue),
      sources: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            sourceId: requireSessionId,
            name: requireText,
          }),
        ),
      harnesses: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            harnessId: requireSessionId,
            name: requireText,
            sourceId: requireSessionId,
          }),
        ),
      profiles: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            profileId: requireSessionId,
            revision: requireSessionU64,
            harnessId: requireSessionId,
            sourceId: requireSessionId,
          }),
        ),
      targets: (items) => approvalQuantity(items, productTargetRecordValue),
    }),
  );
}

function sessionInputFactValue(value: JsonValue | undefined): SessionInputFact {
  const source = object(value);
  switch (oneOf(
    "registered",
    "applying",
    "settled",
    "interrupted",
    "retired",
  )(source.state)) {
    case "registered":
      return closed(value, {
        registrationId: requireSessionId,
        state: oneOf("registered"),
      });
    case "applying":
      return closed(value, {
        registrationId: requireSessionId,
        state: oneOf("applying"),
        operationId: requireSessionId,
        runtimeId: requireSessionId,
        admittedAtMs: requireSessionI64,
      });
    case "settled":
      return closed(value, {
        registrationId: requireSessionId,
        state: oneOf("settled"),
        outcome: sessionInputOutcome,
      });
    case "interrupted":
      return closed(value, {
        registrationId: requireSessionId,
        state: oneOf("interrupted"),
      });
    case "retired":
      return closed(value, {
        registrationId: requireSessionId,
        state: oneOf("retired"),
      });
  }
}

export function parseSessionInputFact(value: JsonValue): SessionInputFact {
  return responseResult(() => sessionInputFactValue(value));
}

export function parseSessionConsumeResult(value: JsonValue): SessionConsumeResult {
  return responseResult(() => {
    const source = object(value);
    if (source.state === "pending") {
      const parsed = closed(value, {
        state: oneOf("pending"),
        registrationId: requireSessionId,
      });
      return Object.freeze({
        kind: "pending",
        registrationId: parsed.registrationId,
      });
    }
    if (source.state === "refused") {
      const parsed = closed(value, {
        state: oneOf("refused"),
        reason: requireText,
        returned: requireBoolean,
      });
      return Object.freeze({
        kind: "refused",
        reason: parsed.reason,
        returned: parsed.returned,
      });
    }
    if (source.state === "settled" && Object.hasOwn(source, "request")) {
      const parsed = closed(value, {
        state: oneOf("settled"),
        returned: requireBoolean,
        request: sessionInputFactValue,
      });
      return Object.freeze({
        kind: "resolved",
        returned: parsed.returned,
        request: parsed.request,
      });
    }
    return Object.freeze({
      kind: "observed",
      fact: sessionInputFactValue(value),
    });
  });
}

export function parseSessionQueuedBodyDescriptor(
  value: JsonValue,
): SessionQueuedBodyDescriptor {
  return responseResult(() =>
    closed(value, {
      streamKind: oneOf("queued_input_body"),
      streamId: requireSessionId,
      documentId: requireSessionId,
      targetId: requireSessionId,
      revision: requireSessionU64,
      totalBytes: requireSessionU64,
      encoding: oneOf("utf8_json"),
    }),
  );
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
        closed(status, {
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
        }),
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
  switch (oneOf(
    "unassociated",
    "known_pre",
    "registered_session",
  )(source.kind)) {
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
    knownKind: requireNullable(oneOf(
      "allow_once",
      "allow_always",
      "reject_once",
      "reject_always",
    )),
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

function approvalQuantity<T>(
  value: JsonValue | undefined,
  parse: (entry: JsonValue | undefined) => T,
): readonly T[] {
  if (!Array.isArray(value)) {
    throw new SessionProtocolError("invalid_frame");
  }
  return Object.freeze(value.map(parse));
}

function approvalView(value: JsonValue | undefined): ApprovalView {
  const source = object(value);
  const contentShape = object(source.content);
  const availability = oneOf("available", "unavailable")(
    contentShape.availability,
  );
  const content = availability === "available"
    ? closed(source.content, {
        availability: oneOf("available"),
        rawJson: requireText,
      })
    : closed(source.content, { availability: oneOf("unavailable") });
  return closed(value, {
    approvalId: requireSessionId,
    source: (entry) => closed(entry, {
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
    choices: (entry) => approvalQuantity(entry, (choice) =>
      closed(choice, {
        choiceId: requireSessionId,
        ordinal: requireSessionU64,
        originalOptionId: requireText,
        originalKind: requireText,
        selectable: requireBoolean,
      })),
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

export function parseNativeReleaseRegisterResult(
  value: JsonValue,
): NativeReleaseRegistered {
  return responseResult(() =>
    closed(value, {
      releaseOperationId: requireSessionId,
      targetId: requireSessionId,
      state: oneOf("registered"),
    }),
  );
}
export function parseSessionCapacity(value: JsonValue): SessionCapacity {
  return responseResult(() => {
    const count = (input: JsonValue | undefined): number => {
      if (typeof input !== "number" || !Number.isSafeInteger(input) ||
        input < 0 || input > 0xffff_ffff) {
        throw new SessionProtocolError("invalid_frame");
      }
      return input;
    };
    const capacity = closed(value, {
      limit: count,
      occupied: count,
      reserved: count,
      closedPending: count,
      automaticRecycling: (input) => {
        if (typeof input !== "boolean") throw new SessionProtocolError("invalid_frame");
        return input;
      },
    });
    // Upgrade may retain more old resources than the new limit.
    if (capacity.limit === 0 ||
      capacity.reserved + capacity.closedPending > capacity.occupied) {
      throw new SessionProtocolError("invalid_frame");
    }
    return capacity;
  });
}

export function parseNativeReleaseStatusResult(
  value: JsonValue,
): NativeReleaseStatus {
  return responseResult(() => {
    const source = object(value);
    const state = oneOf(
      "registered",
      "releasing",
      "released",
      "failed",
    )(source.state);
    if (state === "failed") {
      return closed(value, {
        releaseOperationId: requireSessionId,
        targetId: requireSessionId,
        state: oneOf("failed"),
        error: requireText,
      });
    }
    return closed(value, {
      releaseOperationId: requireSessionId,
      targetId: requireSessionId,
      state: oneOf(state),
      error: (error) => {
        if (error !== null) {
          throw new SessionProtocolError("invalid_frame");
        }
        return null;
      },
    });
  });
}

function requireNull(value: JsonValue | undefined): null {
  if (value !== null) {
    throw new SessionProtocolError("invalid_frame");
  }
  return null;
}

function sourceSessionRegistrationFields(
  value: JsonValue | undefined,
): SourceSessionRegistrationMetadata {
  return closed(value, {
    sourceOperationId: requireSessionId,
    actorId: requireSessionId,
    action: oneOf("discover", "create"),
    sourceId: requireSessionId,
    harnessId: requireSessionId,
    profileId: requireSessionId,
    profileRevision: requireSessionU64,
    workspaceId: requireSessionId,
    registeredAtMs: requireSessionI64,
  });
}

export function parseSourceSessionRegistration(
  value: JsonValue,
): SourceSessionRegistration {
  return responseResult(() =>
    closed(value, {
      sourceOperationId: requireSessionId,
      actorId: requireSessionId,
      action: oneOf("discover", "create"),
      sourceId: requireSessionId,
      harnessId: requireSessionId,
      profileId: requireSessionId,
      profileRevision: requireSessionU64,
      workspaceId: requireSessionId,
      registeredAtMs: requireSessionI64,
      state: oneOf("registered"),
    }),
  );
}

const sourceTransportReason = oneOf(
  "cancelled",
  "deadline",
  "fence_rejected",
  "connection_closed",
  "abandoned",
  "io",
  "invalid_fence",
  "connection_lost",
  "restart",
);

function sourceSessionRpcId(value: JsonValue | undefined): SourceSessionRpcId {
  const source = object(value);
  switch (oneOf("integer", "string")(source.kind)) {
    case "integer":
      return closed(value, {
        kind: oneOf("integer"),
        value: requireSessionI64,
      });
    case "string":
      return closed(value, {
        kind: oneOf("string"),
        value: requireText,
      });
  }
}

function sourceSessionTransport(
  value: JsonValue | undefined,
): SourceSessionTransportFact {
  const source = object(value);
  switch (oneOf("zero", "unknown", "written")(source.kind)) {
    case "zero":
      return closed(value, {
        kind: oneOf("zero"),
        reason: sourceTransportReason,
      });
    case "unknown":
      return closed(value, {
        kind: oneOf("unknown"),
        accepted: requireSessionU64,
        cause: sourceTransportReason,
      });
    case "written":
      return closed(value, {
        kind: oneOf("written"),
      });
  }
}

function sourceSessionResponse(
  value: JsonValue | undefined,
): SourceSessionResponseFact {
  return closed(value, {
    kind: oneOf("created", "sessions", "remote_error"),
    nativeSessionId: requireNullable(requireText),
    nativeKeyAvailable: requireBoolean,
    remoteErrorCode: requireNullable(requireSessionI64),
    sequence: requireSessionU64,
    receipt: requireSessionU64,
    resultBytes: requireSessionU64,
    ack: oneOf("pending", "confirmed", "rejected", "unknown"),
  });
}

export function parseSourceSessionFact(value: JsonValue): SourceSessionFact {
  return responseResult(() =>
    closed(value, {
      registration: sourceSessionRegistrationFields,
      resultAvailable: requireBoolean,
      state: oneOf("registered", "owned", "terminal", "interrupted"),
      outcome: oneOf("pending", "succeeded", "refused", "unknown"),
      runtimeId: requireNullable(requireSessionId),
      admissionAuthGeneration: requireNullable(requireSessionI64),
      sourceInstanceId: requireNullable(requireSessionId),
      connectionEpoch: requireNullable(requireSessionId),
      protocolVersion: requireNullable(oneOf("1", "2")),
      rpcId: requireNullable(sourceSessionRpcId),
      frameLength: requireNullable(requireSessionU64),
      transport: requireNullable(sourceSessionTransport),
      response: requireNullable(sourceSessionResponse),
      failure: requireNullable(
        oneOf(
          "configuration",
          "protocol",
          "source",
          "metadata",
          "ownership",
          "allocation",
          "interrupted",
        ),
      ),
    }),
  );
}

export function parseSourceSessionResultDescriptor(
  value: JsonValue,
): SourceSessionResultDescriptor {
  return responseResult(() =>
    closed(value, {
      streamKind: oneOf("native_session_result"),
      streamId: requireSessionId,
      documentId: requireSessionId,
      targetId: requireNull,
      revision: requireNull,
      totalBytes: requireSessionU64,
      encoding: oneOf("utf8_json"),
    }),
  );
}

export function parseSourceSessionResultReleaseResult(
  value: JsonValue,
): SourceSessionResultReleaseResult {
  return responseResult(() =>
    closed(value, {
      released: requireTrue,
    }),
  );
}

function sessionInputOutcome(
  value: JsonValue | undefined,
): SessionInputOutcome {
  const source = object(value);
  switch (oneOf(
    "queued",
    "direct_selected",
    "edited",
    "cancelled",
    "prioritized",
    "resumed",
    "rejected",
  )(source.kind)) {
    case "queued":
    case "direct_selected":
    case "edited":
    case "cancelled":
    case "prioritized":
      return closed(value, {
        kind: oneOf(
          "queued",
          "direct_selected",
          "edited",
          "cancelled",
          "prioritized",
        ),
        itemId: requireSessionId,
        revision: requireSessionU64,
      }) as Extract<SessionInputOutcome, { readonly itemId: string }>;
    case "resumed":
      return closed(value, {
        kind: oneOf("resumed"),
        restored: requireSessionU64,
      });
    case "rejected":
      return closed(value, {
        kind: oneOf("rejected"),
        reason: requireText,
      });
  }
}

function responseResult<T>(parse: () => T): T {
  try {
    return parse();
  } catch (error) {
    if (error instanceof SessionProtocolError && error.code === "allocation_failed") {
      throw error;
    }
    throw new SessionProtocolError("invalid_frame");
  }
}

type Validator = (value: JsonValue | undefined) => unknown;
type Shape = Readonly<Record<string, Validator>>;
type ShapeValue<S extends Shape> = { readonly [K in keyof S]: ReturnType<S[K]> };

function object(value: JsonValue | undefined): JsonObject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new SessionProtocolError("invalid_command");
  }
  return value;
}

function closed<S extends Shape>(value: JsonValue | undefined, shape: S): ShapeValue<S> {
  const source = object(value);
  const actual = Object.keys(source);
  const keys = Object.keys(shape);
  if (actual.length !== keys.length || keys.some((key) => !Object.hasOwn(source, key))) {
    throw new SessionProtocolError("invalid_command");
  }
  // Every returned key is constructed by its validator; never return unvalidated input.
  return Object.freeze(Object.fromEntries(keys.map((key) => [key, shape[key]!(source[key])]))) as ShapeValue<S>;
}

function oneOf<const T extends readonly string[]>(...values: T) {
  return (value: JsonValue | undefined): T[number] => {
    if (typeof value !== "string" || !values.includes(value)) {
      throw new SessionProtocolError("invalid_command");
    }
    return value;
  };
}

function literal<const T extends string | number | boolean>(
  expected: T,
): (value: JsonValue | undefined) => T {
  return (value) => {
    if (value !== expected) {
      throw new SessionProtocolError("invalid_frame");
    }
    return expected;
  };
}

function requireText(value: JsonValue | undefined): string {
  if (typeof value !== "string") {
    throw new SessionProtocolError("invalid_frame");
  }
  return value;
}

function requireTrue(value: JsonValue | undefined): true {
  if (value !== true) {
    throw new SessionProtocolError("invalid_frame");
  }
  return true;
}

function mutation(value: JsonValue | undefined): QueueMutationDto {
  const source = object(value);
  switch (source.kind) {
    case "submit":
      return closed(source, { kind: oneOf("submit"), bodyId: requireSessionId });
    case "edit":
      return closed(source, {
        kind: oneOf("edit"), itemId: requireSessionId,
        expectedRevision: requireSessionU64, bodyId: requireSessionId,
      });
    case "cancel":
    case "prioritize":
      return closed(source, {
        kind: oneOf("cancel", "prioritize"), itemId: requireSessionId,
        expectedRevision: requireSessionU64,
      });
    case "resume":
      return closed(source, { kind: oneOf("resume") });
    default:
      throw new SessionProtocolError("invalid_command");
  }
}

function requireBoundedSourceText(
  rejectNul: boolean,
): (value: JsonValue | undefined) => string {
  return (value: JsonValue | undefined): string => {
    if (typeof value !== "string" || (rejectNul && value.includes("\0"))) {
      throw new SessionProtocolError("invalid_command");
    }
    const bytes = ENCODER.encode(value);
    if (bytes.length === 0 || bytes.length > MAX_SOURCE_TEXT_BYTES) {
      throw new SessionProtocolError("invalid_command");
    }
    return value;
  };
}

function sourceSelection(
  value: JsonValue | undefined,
): SourceLaunchSelectionInput {
  return closed(value, {
    sourceId: requireSessionId,
    harnessId: requireSessionId,
    profileId: requireSessionId,
    profileRevision: requireSessionU64,
    workspaceId: requireSessionId,
    launchPath: requireBoundedSourceText(true),
  });
}

function sourceOperation(value: JsonValue | undefined): SourceOperationInput {
  const source = object(value);
  switch (source.kind) {
    case "discover":
      return closed(source, {
        kind: oneOf("discover"),
        cursor: (cursor) =>
          cursor === null ? null : requireBoundedSourceText(false)(cursor),
      });
    case "create":
      return closed(source, { kind: oneOf("create") });
    default:
      throw new SessionProtocolError("invalid_command");
  }
}

const SHAPES = {
  "product.status": {},
  "input.upload.begin": { targetId: requireSessionId, purpose: oneOf("submit", "edit") },
  "input.upload.finish": { uploadId: requireSessionId, totalBytes: requireSessionU64 },
  "input.upload.status": { uploadId: requireSessionId },
  "input.upload.release": { uploadId: requireSessionId },
  "input.register": { targetId: requireSessionId, operation: mutation },
  "input.consume": {
    targetId: requireSessionId, registrationId: requireSessionId,
    bodyId: (value: JsonValue | undefined): string | null => value === null ? null : requireSessionId(value),
  },
  "input.request": { targetId: requireSessionId, registrationId: requireSessionId },
  "input.queue": { targetId: requireSessionId },
  "input.body.open": { targetId: requireSessionId, itemId: requireSessionId, expectedRevision: requireSessionU64 },
  "catalog.upload.begin": {},
  "catalog.upload.finish": { uploadId: requireSessionId, totalBytes: requireSessionU64 },
  "catalog.upload.status": { uploadId: requireSessionId },
  "catalog.upload.release": { uploadId: requireSessionId },
  "catalog.register": { documentUploadId: requireSessionId },
  "catalog.apply": { catalogRequestId: requireSessionId },
  "catalog.request": { catalogRequestId: requireSessionId },
  "workspace.register": { path: requireBoundedSourceText(true) },
  "target.register": {
    sourceId: requireSessionId,
    nativeSessionId: requireBoundedSourceText(true),
    workspaceId: requireSessionId,
  },
  "catalog.inventory": {},
  "session.capacity.inspect": {},
  "approval.list": {},
  "approval.read": { approvalId: requireSessionId },
  "approval.register": { approvalId: requireSessionId, choiceId: requireSessionId },
  "approval.decide": { approvalRegistrationId: requireSessionId },
  "approval.status": { approvalId: requireSessionId },
  "native.release.register": { targetId: requireSessionId },
  "native.release": { releaseOperationId: requireSessionId },
  "native.release.status": { releaseOperationId: requireSessionId },
  "source.session.register": {
    selection: sourceSelection,
    operation: sourceOperation,
  },
  "source.session.execute": { sourceOperationId: requireSessionId },
  "source.session.status": { sourceOperationId: requireSessionId },
  "source.session.reconcile": { sourceOperationId: requireSessionId },
  "source.session.result.open": { sourceOperationId: requireSessionId },
  "source.session.result.release": { sourceOperationId: requireSessionId },
  "view.open": {
    targetId: requireSessionId,
    reason: oneOf("first_open", "explicit_refresh", "reopen", "reconnect", "reacquire"),
  },
  "view.open_source": {
    selection: sourceSelection,
    nativeSessionId: requireBoundedSourceText(true),
    reason: oneOf("first_open", "explicit_refresh", "reopen", "reconnect", "reacquire"),
  },
  "view.close": { streamId: requireSessionId },
  "view.original": { streamId: requireSessionId },
  "view.rebind": { streamId: requireSessionId },
} as const satisfies Readonly<Record<string, Shape>>;

export type SessionAction = keyof typeof SHAPES;
export type SessionCommand = {
  [A in SessionAction]: {
    readonly v: 1;
    readonly type: "command";
    readonly requestId: string;
    readonly action: A;
    readonly payload: ShapeValue<(typeof SHAPES)[A]>;
  }
}[SessionAction];

/** Syntax only: neither service availability, admission nor delivery is implied. */
export function parseSessionCommand(input: string): SessionCommand {
  try {
    if (typeof input !== "string" || input.length > MAX_MANAGEMENT_FRAME_BYTES) {
      throw new SessionProtocolError("invalid_command");
    }
    const bytes = ENCODER.encode(input);
    if (bytes.length > MAX_MANAGEMENT_FRAME_BYTES) {
      throw new SessionProtocolError("invalid_command");
    }
    const source = parseStrictJsonObject(bytes);
    const envelope = closed(source, {
      v: (value) => { if (value !== 1) throw new SessionProtocolError("invalid_command"); return 1 as const; },
      type: oneOf("command"),
      requestId: requireSessionId,
      action: (value): SessionAction => {
        if (typeof value !== "string" || !Object.hasOwn(SHAPES, value)) {
          throw new SessionProtocolError("invalid_command");
        }
        return value as SessionAction;
      },
      payload: object,
    });
    const payload = closed(envelope.payload, SHAPES[envelope.action]);
    // The action selects the exact closed shape before this discriminated union is built.
    return Object.freeze({ ...envelope, payload }) as SessionCommand;
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (error instanceof RangeError) throw new SessionProtocolError("allocation_failed");
    throw new SessionProtocolError("invalid_command");
  }
}

export function encodeSessionCommand(command: SessionCommand): string {
  try {
    return JSON.stringify(parseSessionCommand(JSON.stringify(command)));
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (error instanceof RangeError) throw new SessionProtocolError("allocation_failed");
    throw new SessionProtocolError("invalid_command");
  }
}

export type SessionServerBinary =
  | { readonly kind: "input_ack"; readonly acknowledgement: InputAcknowledgement }
  | { readonly kind: "view_data"; readonly frame: ViewDataFrame };

/** The caller supplies its connection phase; this function never authenticates a socket. */
export function decodeSessionServerBinary(
  wire: Uint8Array,
  phase: "handshake" | "authenticated",
): SessionServerBinary {
  if (phase !== "authenticated") throw new SessionProtocolError("binary_before_auth");
  if (!(wire instanceof Uint8Array) || wire.length < 4) {
    throw new SessionProtocolError("invalid_frame");
  }
  try {
    if (wire[0] !== 65 || wire[1] !== 67 || wire[2] !== 80) {
      throw new SessionProtocolError("invalid_frame");
    }
    if (wire[3] === 73) {
      return Object.freeze({ kind: "input_ack", acknowledgement: decodeInputAcknowledgement(wire) });
    }
    if (wire[3] === 86) {
      const frame = decodeViewFrame(wire);
      if (frame.type !== "data") throw new SessionProtocolError("invalid_direction");
      return Object.freeze({ kind: "view_data", frame });
    }
    throw new SessionProtocolError("invalid_frame");
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (error instanceof RangeError ||
      ((error instanceof ViewDecodeError || error instanceof InputTransferError) && error.code === "allocation_failed")) {
      throw new SessionProtocolError("allocation_failed");
    }
    if (error instanceof InputTransferError && error.code === "invalid_direction") {
      throw new SessionProtocolError("invalid_direction");
    }
    throw new SessionProtocolError("invalid_frame");
  }
}
