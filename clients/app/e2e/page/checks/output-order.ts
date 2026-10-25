import { historyItem, readMessage, readPage } from "../fixtures";
import { expectEqual, expectThat, query } from "../support";
import type { Check } from "../support";
import { sessionScenario, withSessionPage } from "./conversation-timeline";

const THOUGHT = "f0000000-0000-4000-8000-000000000009";
const REPLY = "10000000-0000-4000-8000-000000000001";
const NEXT = "20000000-0000-4000-8000-000000000002";
const TOOL = "30000000-0000-4000-8000-000000000003";
const AT = "2026-10-24T09:56:12.904Z";

export const OUTPUT_ORDER_CHECKS: readonly Check[] = [{
  name: "ORDER-1 同毫秒及工具跨页起点：页面重建后仍保持思考、工具、回复、后续思考的输出顺序",
  async run() {
    const options = sessionScenario({
      events: [],
      readResult: (sessionId) => readPage({
        sessionId,
        messages: [
          { ...readMessage({ messageId: REPLY, sessionId, role: "agent", text: "后回复", createdAt: "2026-10-23T00:00:00.000Z" }), channel: "reply", sequence: "21" },
          { ...readMessage({ messageId: THOUGHT, sessionId, role: "agent", text: "先思考", createdAt: AT }), channel: "thought", sequence: "6" },
          { ...readMessage({ messageId: NEXT, sessionId, role: "agent", text: "回复后再思考", createdAt: AT }), channel: "thought", sequence: "25" },
        ],
        history: [historyItem({
          eventId: "40000000-0000-4000-8000-000000000004",
          eventType: "tool.call.completed", sessionSequence: 100, createdAt: AT,
          view: { toolCallId: TOOL, title: "读取文件", state: "completed", _timelineSequence: "12", _timelineOriginSequence: "112" },
        })],
        hasEarlier: false,
        historyHasEarlier: false,
      }),
    });
    // 两次独立挂载会重建 store，第二次只能依赖持久读取结果。
    for (let mount = 0; mount < 2; mount += 1) {
      await withSessionPage(options,
        container => query(container, `[data-message-id="${NEXT}"]`) !== null,
        "完整输出时间线读取完成",
        async ({ container }) => {
          const selectors = [
            `[data-message-id="${THOUGHT}"]`, `[data-tool-call-id="${TOOL}"]`,
            `[data-message-id="${REPLY}"]`, `[data-message-id="${NEXT}"]`,
          ];
          const nodes = selectors.map(selector => query(container, selector));
          expectThat(nodes.every(node => node !== null), "四个真实条目全部呈现");
          for (let index = 1; index < nodes.length; index += 1) {
            const left = nodes[index - 1]; const right = nodes[index];
            expectThat(left !== null && left !== undefined && right !== null && right !== undefined &&
              (left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0,
            `条目 ${index} 必须在条目 ${index + 1} 前，不能按时间或消息标识重排`);
          }
          expectEqual(query(container, '[data-timeline="entries"]')?.getAttribute("data-timeline-entry-count"), "4", "片段不能跨工具合并");
        });
    }
  },
}];
