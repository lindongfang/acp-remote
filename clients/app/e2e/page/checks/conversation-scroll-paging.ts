import { readMessage, readPage, SESSION_ALPHA } from "../fixtures";
import { expectEqual, expectThat, query, queryAll, waitFor } from "../support";
import type { Check } from "../support";
import { commandBodies, sessionScenario, withSessionPage } from "./conversation-timeline";

function messages(first: number): Record<string, unknown>[] {
  return Array.from({ length: 30 }, (_, offset) => {
    const index = first * 3 + offset;
    const part = offset % 3;
    return {
      ...readMessage({
        messageId: `b0000000-0000-4000-8000-${String(index).padStart(12, "0")}`,
        sessionId: SESSION_ALPHA,
        role: part === 0 ? "user" : "agent",
        text: `${part === 0 ? "输入" : part === 1 ? "思考" : "回复"} ${String(index)}\n${"对话内容\n".repeat(8)}`,
        createdAt: new Date(Date.UTC(2026, 9, 1, 0, 0, index)).toISOString(),
      }),
      channel: part === 1 ? "thought" : "reply",
    };
  });
}

export const CONVERSATION_SCROLL_PAGING_CHECKS: readonly Check[] = [{
  name: "SCROLL-PAGING 每页十条用户输入、回复与思考；滚到顶部续取并保持阅读位置",
  async run() {
    let reads = 0;
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => {
        reads += 1;
        return readPage({ sessionId, messages: messages(reads === 1 ? 10 : 0), hasEarlier: reads === 1 });
      },
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "首屏十组消息", async (mounted) => {
      const host = mounted.host;
      expectThat(host !== null, "假主机已连接");
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(timeline !== null, "时间线存在");
      const firstPayload = commandBodies(host.sent, "session.read")[0]?.["payload"] as Record<string, unknown>;
      expectEqual(firstPayload["limit"], 10, "首屏显式请求十条用户输入");
      expectThat(query(mounted.container, '[data-action="load-earlier"]') === null, "没有顶部加载按钮");
      await waitFor(() => timeline.scrollTop > 96, "首屏定位到底部");
      timeline.scrollTop = 40;
      timeline.dispatchEvent(new Event("scroll"));
      await waitFor(() => queryAll(mounted.container, "[data-message-id]").length === 60, "追加前十组消息");
      const requests = commandBodies(host.sent, "session.read");
      expectEqual(requests.length, 2, "滚动只续取一次");
      const payload = requests[1]?.["payload"] as Record<string, unknown>;
      expectEqual(payload["limit"], 10, "续取仍按十条用户输入计数");
      expectThat(payload["before"] !== undefined, "续取携带前一页边界");
      await waitFor(() => timeline.scrollTop > 96, "前置消息后保持原阅读位置");
      timeline.scrollTop = 0;
      timeline.dispatchEvent(new Event("scroll"));
      await new Promise((resolve) => setTimeout(resolve, 150));
      expectEqual(commandBodies(host.sent, "session.read").length, 2, "到达最早记录后不再请求");
    });
  },
}];
