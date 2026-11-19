import type { JsonValue } from "../../protocol/jws";
import {
  closed,
  object,
  oneOf,
  requireBoolean,
  requireBoundedSourceText,
  requireNull,
  requireNullable,
  requireSessionId,
  requireSessionI64,
  requireSessionU64,
  requireText,
  requireTrue,
  responseResult,
  SessionProtocolError,
} from "./primitives";

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

export interface SourceSessionRegistration
  extends SourceSessionRegistrationMetadata {
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
      if (
        typeof input !== "number" ||
        !Number.isSafeInteger(input) ||
        input < 0 ||
        input > 0xffff_ffff
      ) {
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
        if (typeof input !== "boolean")
          throw new SessionProtocolError("invalid_frame");
        return input;
      },
    });
    // Upgrade may retain more old resources than the new limit.
    if (
      capacity.limit === 0 ||
      capacity.reserved + capacity.closedPending > capacity.occupied
    ) {
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

function sourceSessionRpcId(
  value: JsonValue | undefined,
): SourceSessionRpcId {
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

export function parseSourceSessionFact(
  value: JsonValue,
): SourceSessionFact {
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

export function sourceSelection(
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

export function sourceOperation(
  value: JsonValue | undefined,
): SourceOperationInput {
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
