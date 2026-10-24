import { expect, it } from "vitest";
import { appendDelta, buildTimeline } from "./conversation-timeline";
import type { InteractionView } from "../domain/interaction";

it("没有历史事件的审批按请求序号插入思考和工具之后，补回事件后不重复", () => {
  const interaction: InteractionView = {
    interactionId: "approval", sessionId: "session", kind: "permission",
    sequence: "3",
    title: null, description: null, schema: null,
    requestedAt: "2026-10-24T00:00:03.000Z", turnId: null,
    witnessed: false, effective: false, requestVersion: null, expiresAt: null,
    options: null, availability: { kind: "not_answerable", reason: "options_missing" },
  };
  const input = { messages: [], volatile: [appendDelta(undefined, {
    messageId: "thought", sessionId: "session", channel: "thought", turnId: null,
    sequence: "1",
    deltaIndex: "0", text: "先检查环境", createdAt: "2026-10-24T00:00:01.000Z",
  })], interactions: [interaction], records: [
    { source: "history", ownerNodeId: null, eventId: "tool", eventType: "tool.call.started",
      sessionId: "session", createdAt: "2026-10-24T00:00:02.000Z", sessionSequence: "2",
      globalSequence: null, originKind: null,
      view: { toolCallId: "tool", title: "检查环境", state: "pending" } },
  ] } as const;
  const entries = buildTimeline(input);
  expect(entries.map(entry => entry.kind)).toEqual(["message", "tool_call", "pending_interaction"]);
  expect(entries[2]).toMatchObject({ createdAt: interaction.requestedAt });
  const recovered = buildTimeline({ ...input, records: [...input.records, {
    ...input.records[0], eventId: "request", eventType: "permission.requested",
    createdAt: interaction.requestedAt, sessionSequence: "3",
    view: { interactionId: "approval", title: "执行命令" },
  }] });
  expect(recovered.map(entry => entry.kind)).toEqual(["message", "tool_call", "permission_requested"]);
});
