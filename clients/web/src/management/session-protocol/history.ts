import type { JsonValue } from "../../protocol/jws";
import { closed, closedWithOptional, object, oneOf, requireBoolean, requireBoundedSourceText,
  requireNullable, requireSessionId, requireSessionU64, requireText, SessionProtocolError } from "./primitives";
import { sourceSelection } from "./source-and-capacity";

function positive(value: unknown): string {
  const number = requireSessionU64(value);
  if (number === "0") throw new SessionProtocolError("invalid_decimal");
  return number;
}
export function historyTurns(value: unknown): string {
  const count = positive(value);
  if (BigInt(count) > 50n) throw new SessionProtocolError("invalid_decimal");
  return count;
}
const nativeId = requireBoundedSourceText(true);
const direction = oneOf("older", "newer");
function target(value: JsonValue | undefined) {
  return object(value).kind === "target"
    ? closed(value, { kind: oneOf("target"), targetId: requireSessionId })
    : closed(value, { kind: oneOf("source"), selection: sourceSelection, nativeSessionId: nativeId });
}
function mode(value: JsonValue | undefined) {
  switch (object(value).kind) {
    case "first": return closed(value, { kind: oneOf("first") });
    case "continue": return closed(value, { kind: oneOf("continue"), cursor: requireText, direction });
    default: return closed(value, { kind: oneOf("locate"), fromTurn: positive, direction });
  }
}
export const historyShape = { required: { target, mode }, optional: { turns: historyTurns, child: nativeId } };
export type HistoryRequest = { target: ReturnType<typeof target>; mode: ReturnType<typeof mode>; child?: string; turns?: string };
export const historyOpenPage = (value: JsonValue | undefined) => closedWithOptional(value, {}, { turns: historyTurns });
export const turnBoundary = (value: JsonValue | undefined) => closed(value, { turn: positive, firstPosition: requireSessionU64 });
export function parseHistoryPage(value: JsonValue | undefined) {
  return closed(value, {
    generationId: requireText, source: oneOf("transcript_cache", "live", "replay"),
    turnStatus: oneOf("provable", "unprovable"), order: oneOf("ascending"), totalTurns: requireSessionU64,
    firstTurn: requireNullable(positive), lastTurn: requireNullable(positive),
    turns: entries => {
      if (!Array.isArray(entries)) throw new SessionProtocolError("invalid_frame");
      const turns = entries.map(turnBoundary);
      for (let i = 1; i < turns.length; i++) {
        if (BigInt(turns[i]!.turn) <= BigInt(turns[i-1]!.turn) ||
            BigInt(turns[i]!.firstPosition) <= BigInt(turns[i-1]!.firstPosition)) throw new SessionProtocolError("invalid_frame");
      }
      return turns;
    },
    prelude: requireBoolean, olderCursor: requireNullable(requireText), newerCursor: requireNullable(requireText),
    relocated: requireBoolean, continuity: oneOf("same_generation", "unproven"), liveTail: requireBoolean,
  });
}
export type HistoryPage = ReturnType<typeof parseHistoryPage>;
