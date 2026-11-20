import type { JsonValue } from "../../protocol/jws";
import {
  closed,
  MAX_SOURCE_TEXT_BYTES,
  requireNullable,
  requireSessionId,
  requireSessionU64,
  requireText,
  responseResult,
  SessionProtocolError,
} from "./primitives";

const ENCODER = new TextEncoder();

export interface SessionControlSnapshot {
  readonly targetId: string | null;
  readonly rootTargetId: string | null;
  readonly sourceId: string;
  readonly rootNativeSessionId: string;
  readonly controllerClientId: string | null;
  readonly controllerDisplayName: string | null;
  readonly controlEpoch: string;
}

function requireNonEmptyControlString(value: JsonValue | undefined): string {
  const text = requireText(value);
  if (text.length === 0 || text.includes("\0") || ENCODER.encode(text).length > MAX_SOURCE_TEXT_BYTES) {
    throw new SessionProtocolError("invalid_frame");
  }
  return text;
}

export function parseSessionControlSnapshot(value: JsonValue): SessionControlSnapshot {
  return responseResult(() => {
    const parsed = closed(value, {
      targetId: requireNullable(requireSessionId),
      rootTargetId: requireNullable(requireSessionId),
      sourceId: requireSessionId,
      rootNativeSessionId: requireNonEmptyControlString,
      controllerClientId: requireNullable(requireSessionId),
      controllerDisplayName: requireNullable(requireNonEmptyControlString),
      controlEpoch: requireSessionU64,
    });
    if ((parsed.controllerClientId === null) !== (parsed.controllerDisplayName === null)) {
      throw new SessionProtocolError("invalid_frame");
    }
    return parsed;
  });
}
