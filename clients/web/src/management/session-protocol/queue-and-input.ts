import type { JsonValue } from "../../protocol/jws";
import {
  closed,
  object,
  oneOf,
  requireArray,
  requireBoolean,
  requireNullable,
  requireSessionId,
  requireSessionI64,
  requireSessionU64,
  requireText,
  requireTrue,
  responseResult,
  SessionProtocolError,
} from "./primitives";

export type QueueMutationDto =
  | { readonly kind: "submit"; readonly bodyId: string }
  | {
      readonly kind: "edit";
      readonly itemId: string;
      readonly expectedRevision: string;
      readonly bodyId: string;
    }
  | {
      readonly kind: "cancel" | "prioritize";
      readonly itemId: string;
      readonly expectedRevision: string;
    }
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

export type InputUploadStatus =
  | {
      readonly uploadId: string;
      readonly phase: "receiving" | "preparing";
      readonly nextOffset: string;
    }
  | {
      readonly uploadId: string;
      readonly phase: "ready";
      readonly nextOffset: string;
      readonly bodyId: string;
    }
  | {
      readonly uploadId: string;
      readonly phase: "failed";
      readonly nextOffset: string;
      readonly error: string;
    };

export interface InputUploadFinishResult {
  readonly uploadId: string;
}

export interface InputUploadReleaseResult {
  readonly released: true;
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
      readonly kind: "pending";
      readonly registrationId: string;
    }
  | {
      readonly kind: "resolved";
      readonly returned: boolean;
      readonly request: SessionInputFact;
    }
  | {
      readonly kind: "refused";
      readonly reason: string;
      readonly returned: boolean;
    }
  | {
      readonly kind: "observed";
      readonly fact: SessionInputFact;
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
      request: (request) =>
        closed(request, {
          registrationId: requireSessionId,
          state: oneOf("settled"),
          outcome: (outcome) =>
            closed(outcome, {
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

function sessionInputOutcome(
  value: JsonValue | undefined,
): SessionInputOutcome {
  const source = object(value);
  switch (
    oneOf(
      "queued",
      "direct_selected",
      "edited",
      "cancelled",
      "prioritized",
      "resumed",
      "rejected",
    )(source.kind)
  ) {
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

function sessionInputFactValue(
  value: JsonValue | undefined,
): SessionInputFact {
  const source = object(value);
  switch (
    oneOf(
      "registered",
      "applying",
      "settled",
      "interrupted",
      "retired",
    )(source.state)
  ) {
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

export function parseSessionConsumeResult(
  value: JsonValue,
): SessionConsumeResult {
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

export function mutation(value: JsonValue | undefined): QueueMutationDto {
  const source = object(value);
  switch (source.kind) {
    case "submit":
      return closed(source, { kind: oneOf("submit"), bodyId: requireSessionId });
    case "edit":
      return closed(source, {
        kind: oneOf("edit"),
        itemId: requireSessionId,
        expectedRevision: requireSessionU64,
        bodyId: requireSessionId,
      });
    case "cancel":
    case "prioritize":
      return closed(source, {
        kind: oneOf("cancel", "prioritize"),
        itemId: requireSessionId,
        expectedRevision: requireSessionU64,
      });
    case "resume":
      return closed(source, { kind: oneOf("resume") });
    default:
      throw new SessionProtocolError("invalid_command");
  }
}
