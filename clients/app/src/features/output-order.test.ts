import { describe, expect, it } from "vitest";
import { buildSessionReadPageView } from "../domain/conversation";
import { recordFromHistoryItem, recordFromRemoteEvent } from "../domain/timeline";
import { assertPagesDoNotOverlap, cursorOf, earliestCursor, toLoadedPage } from "../sync-client/paging";
import { appendDelta, buildTimeline } from "./conversation-timeline";
import { historyItem, messageItem, onlineStore, readResult, lastDispatch, resources, session } from "./testing/fixtures";
import { SESSION_HISTORY_FEATURE } from "../sync-client";
import type { SnapshotItemMessages } from "../protocol";

const ID = "aaaaaaaa-0000-4000-8000-000000000001";
const AT = "2026-10-01T00:00:02.000Z";
const msg = (id: string, sequence?: string, createdAt = AT): SnapshotItemMessages => ({
  ...messageItem({ id, role: "agent", text: id, createdAt }),
  ...(sequence === undefined ? {} : { sequence }),
});
const views = (messages: readonly SnapshotItemMessages[]) => buildSessionReadPageView({ sessionId: ID, hasEarlier: false, resources: { messages } }).messages;
const tool = (sequence: string, completed = false) => recordFromHistoryItem(historyItem({
  eventId: "tool-" + sequence, sessionSequence: sequence,
  eventType: completed ? "tool.call.completed" : "tool.call.started",
  createdAt: completed ? "2026-09-01T00:00:00.000Z" : AT,
  view: { toolCallId: "tool", turnId: "turn", title: "search", state: completed ? "completed" : "in_progress", kind: "search" },
}), ID);

describe("authority output positions", () => {
  it("keeps same-millisecond reversed IDs and backward clocks in event order, including tools", () => {
    const messages = [{ ...msg("z-thought", "6"), channel: "thought" as const }, msg("a-reply", "21", "2026-09-01T00:00:00.000Z")];
    const entries = buildTimeline({ messages: views(messages), records: [tool("12"), tool("30", true)], volatile: [], interactions: [] });
    expect(entries.map(entry => entry.sequence)).toEqual(["6", "12", "21"]);
    const card = entries[1];
    expect(card?.kind).toBe("tool_call");
    expect(card?.kind === "tool_call" ? card.state : null).toBe("completed");
  });

  it("preserves a volatile message position when authoritative content replaces it", () => {
    const tail = appendDelta(undefined, { messageId: "reply", sessionId: ID, channel: "agent", turnId: "turn", deltaIndex: "0", text: "reply", createdAt: AT, sequence: "21" });
    const input = { records: [tool("12")], volatile: [tail], interactions: [] };
    const before = buildTimeline({ ...input, messages: [] });
    const after = buildTimeline({ ...input, messages: views([msg("reply", "21")]) });
    expect(before.map(entry => [entry.key, entry.sequence])).toEqual(after.map(entry => [entry.key, entry.sequence]));
  });

  it("uses remote origin positions in the same domain as remote read messages", () => {
    const record = recordFromRemoteEvent({ sessionRef: { ownerNodeId: "owner", sessionId: ID }, originEventId: "tool", originEpoch: "epoch", payloadDigest: "digest", originSequence: "9007199254740993", eventType: "tool.call.started", createdAt: AT,
      payload: { view: { toolCallId: "tool", turnId: "turn", title: "search", state: "in_progress", kind: "search" } } });
    const entries = buildTimeline({ messages: views([msg("reply", "9007199254740994", "2026-09-01T00:00:00.000Z")]), records: [record], volatile: [], interactions: [] });
    expect(entries.map(entry => entry.sequence)).toEqual(["9007199254740993", "9007199254740994"]);
  });

  it("keeps legacy rows in a stable prefix and carries the correct cross-partition cursor", () => {
    const legacy = msg("legacy", undefined, "2027-01-01T00:00:00.000Z");
    const known = msg("known", "1", "2026-01-01T00:00:00.000Z");
    expect(earliestCursor([known, legacy])).toEqual({ createdAt: legacy.createdAt, messageId: "legacy" });
    expect(cursorOf(known).sequence).toBe("1");
    expect(() => assertPagesDoNotOverlap([known], [legacy])).not.toThrow();
    expect(() => assertPagesDoNotOverlap([legacy], [known])).toThrow(/漏读/);
    expect(buildTimeline({ messages: views([known, legacy]), records: [], volatile: [], interactions: [] }).map(entry => entry.key)).toEqual(["message:legacy", "message:known"]);
  });

  it("paginates by exact sequence despite backward clocks and rejects noncanonical decimals", () => {
    const older = msg("z", "9007199254740993", "2027-01-01T00:00:00.000Z");
    const newer = msg("a", "9007199254740994", "2026-01-01T00:00:00.000Z");
    expect(earliestCursor([newer, older])?.sequence).toBe(older.sequence);
    expect(() => assertPagesDoNotOverlap([newer], [older])).not.toThrow();
    for (const sequence of ["01", "-1", "1.0", "1e3", ""]) {
      expect(() => toLoadedPage({ sessionId: ID, hasEarlier: false, resources: { messages: [msg("bad", sequence)] } })).toThrow(/sequence/);
    }
  });

  it("loads the lagging history stream and keeps loading after messages end", () => {
    const { store, gateway } = onlineStore({ resources: resources({ sessions: [session({ id: ID, state: "running" })] }), features: [SESSION_HISTORY_FEATURE] });
    store.loadSession(ID);
    store.apply({ kind: "command_result", result: readResult({ requestId: lastDispatch(gateway).body.requestId, messages: [msg("reply", "21")], hasEarlier: false,
      history: [historyItem({ eventId: "tool-30", eventType: "tool.call.started", sessionSequence: "30", view: { toolCallId: "late", turnId: "turn", title: "search", state: "in_progress", kind: "search" } })], historyHasEarlier: true }) });
    expect(store.conversationPage(ID)?.timeline.windowLoading).toBe(true);
    expect(store.conversationPage(ID)?.paging.loadingHistory).toBe(true);
    store.loadEarlier(ID);
    const command = lastDispatch(gateway).body;
    expect(command.command === "session.read" ? command.payload.beforeHistory : null).toBe("30");
    store.apply({ kind: "command_result", result: readResult({ requestId: command.requestId,
      history: [historyItem({ eventId: "tool-12", eventType: "tool.call.started", sessionSequence: "12", view: { toolCallId: "early", title: "first", state: "completed" } })],
      historyHasEarlier: false }) });
    expect(store.conversationPage(ID)?.timeline.windowLoading).toBe(false);
  });
});
