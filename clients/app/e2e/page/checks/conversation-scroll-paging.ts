import { DIRECTORY_ID_ALPHA, readMessage, readPage, SESSION_ALPHA, sessionSummary, wireEvent } from "../fixtures";
import { click, expectEqual, expectThat, query, queryAll, waitFor } from "../support";
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
  name: "SCROLL-SETTLE 输入与轮次结束不在高度未变时重新定位",
  async run() {
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
      events: [wireEvent({ globalSequence: 11,
        eventId: "e0000000-0000-4000-8000-000000000451", sessionId: SESSION_ALPHA,
        eventType: "turn.started", view: { turnId: "settle-turn", state: "running" } })],
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      const input = query(mounted.container, "[data-composer-input]");
      expectThat(timeline !== null && input !== null, "时间线与输入框存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      // 仍在跟随阈值内，但没有新内容时不能偷偷移到绝对底部。
      timeline.scrollTop -= 18;
      timeline.dispatchEvent(new Event("scroll"));
      const before = timeline.scrollTop;
      const samples: number[] = [];
      let sampling = true;
      const sample = (): void => {
        samples.push(timeline.scrollTop);
        if (sampling) requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
      try {
        Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(input, "下一条草稿");
        input.dispatchEvent(new Event("input", { bubbles: true }));
        await new Promise(resolve => setTimeout(resolve, 120));
        mounted.host?.deliverEvent(wireEvent({ globalSequence: 12,
          eventId: "e0000000-0000-4000-8000-000000000452", sessionId: SESSION_ALPHA,
          eventType: "turn.completed", view: { turnId: "settle-turn", state: "completed" } }));
        await new Promise(resolve => setTimeout(resolve, 200));
        expectThat(samples.length > 2, "实际采样多帧");
        expectThat(samples.every(top => Math.abs(top - before) < 2),
          `高度未变时保持位置：before=${before}, samples=${samples.join(",")}`);
      } finally { sampling = false; }
    });
  },
}, {
  name: "SCROLL-CONFIRM 输入受理确认不在内容未变时拉动页面",
  async run() {
    await withSessionPage(sessionScenario({
      sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "idle" })],
      holdCommands: ["session.prompt"],
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      const input = query(mounted.container, "[data-composer-input]");
      expectThat(timeline !== null && input !== null, "时间线与输入存在");
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set?.call(input, "确认输入的滚动测试");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await waitFor(() => mounted.composition?.store.draftOf(SESSION_ALPHA) === "确认输入的滚动测试", "草稿写入");
      click(query(mounted.container, '[data-action="submit-prompt"]'), "发送输入");
      await waitFor(() => query(timeline, '[data-message-local="submitted"]') !== null, "输入已提交");
      await new Promise(resolve => setTimeout(resolve, 80));
      timeline.scrollTop -= 18;
      timeline.dispatchEvent(new Event("scroll"));
      const before = timeline.scrollTop;
      const samples: number[] = [];
      let sampling = true;
      const sample = (): void => {
        samples.push(timeline.scrollTop);
        if (sampling) requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
      try {
        mounted.host?.completeHeldCommand("session.prompt", { status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: "d0000000-0000-4000-8000-000000000731" } });
        await waitFor(() => query(timeline, '[data-message-local="accepted"]') !== null, "服务端确认已接受");
        await new Promise(resolve => setTimeout(resolve, 150));
        expectThat(samples.length > 2 && samples.every(top => Math.abs(top - before) < 2),
          `确认逐帧保持位置 before=${before}: ${samples.join(",")}`);
      } finally { sampling = false; }
    });
  },
}, {
  name: "SCROLL-FINISH 末尾长思考完成折叠时逐帧贴底，上读时不拉走",
  async run() {
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(timeline !== null, "时间线存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      let sequence = 11;
      const deliver = (eventType: string, view: Record<string, unknown>): void => {
        const current = sequence++;
        mounted.host?.deliverEvent(wireEvent({ globalSequence: current,
          eventId: `e0000000-0000-4000-8000-${String(current + 600).padStart(12, "0")}`,
          sessionId: SESSION_ALPHA, eventType, view }));
      };
      for (const reading of [false, true]) {
        const turnId = `finish-turn-${String(reading)}`;
        const messageId = reading ? "b0000000-0000-4000-8000-000000000702" : "b0000000-0000-4000-8000-000000000701";
        deliver("turn.started", { turnId, state: "running" });
        deliver("agent.thought.delta", { messageId, turnId, deltaIndex: "0", text: "思考内容逐行增长。\n\n".repeat(80) });
        await waitFor(() => query(timeline, `[data-message-id="${messageId}"]`) !== null, "长思考呈现");
        await new Promise(resolve => setTimeout(resolve, 80));
        if (reading) {
          timeline.scrollTop = 420;
          timeline.dispatchEvent(new Event("scroll"));
        }
        const samples: number[] = [];
        let sampling = true;
        const sample = (): void => {
          samples.push(reading ? timeline.scrollTop : timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight);
          if (sampling) requestAnimationFrame(sample);
        };
        requestAnimationFrame(sample);
        try {
          deliver("turn.completed", { turnId, state: "completed" });
          await new Promise(resolve => setTimeout(resolve, 180));
          expectThat(samples.length > 2 && samples.every(value => Math.abs(value - (reading ? 420 : 0)) < 2),
            `结束折叠逐帧稳定 reading=${reading}: ${samples.join(",")}`);
        } finally { sampling = false; }
      }
    });
  },
}, {
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
}, {
  name: "SCROLL-STABILITY 连续上读固定新位置，流式底部跟随，长块快捷收起和窄输入栏",
  async run() {
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史消息加载", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      const region = query(mounted.container, '[data-conversation-region="timeline"]');
      expectThat(timeline !== null && region !== null, "独立对话区存在");
      await waitFor(() => timeline.scrollTop > 400, "首次位于底部");
      timeline.scrollTop = 180;
      timeline.dispatchEvent(new Event("scroll"));
      timeline.scrollTop = 420;
      timeline.dispatchEvent(new Event("scroll"));
      const messageId = "b0000000-0000-4000-8000-000000000099";
      let sequence = 11;
      let deltaIndex = 0;
      const append = async (): Promise<void> => {
        mounted.host?.deliverEvent(wireEvent({
          globalSequence: sequence++, eventId: `e0000000-0000-4000-8000-${String(sequence).padStart(12, "0")}`,
          sessionId: SESSION_ALPHA, eventType: "agent.message.delta",
          createdAt: "2026-10-01T01:00:00.000Z",
          view: { messageId, deltaIndex: String(deltaIndex++), text: "流式输出逐行增长。\n".repeat(8) },
        }));
        await new Promise((resolve) => setTimeout(resolve, 70));
      };
      await append();
      expectThat(Math.abs(timeline.scrollTop - 420) < 2, "流式输出不得拉回第一次上读的旧位置");
      // 模拟后台期间浏览器调整滚动值；该事件不得覆盖用户的阅读位置。
      Object.defineProperty(document, "hidden", { configurable: true, value: true });
      try {
        timeline.scrollTop = 0;
        timeline.dispatchEvent(new Event("scroll"));
        await append();
      } finally {
        Reflect.deleteProperty(document, "hidden");
        document.dispatchEvent(new Event("visibilitychange"));
      }
      await waitFor(() => Math.abs(timeline.scrollTop - 420) < 2, "恢复前台后仍在原阅读位置");
      const bottom = query(region, '[data-action="scroll-to-bottom"]');
      expectThat(bottom !== null, "回到底部位于对话区内部");
      expectThat(bottom.getBoundingClientRect().bottom <= region.getBoundingClientRect().bottom, "回底按钮不越入输入区");
      click(bottom, "回到底部");
      await append();
      expectThat(timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight < 2, "新增多行后仍贴底");

      const fold = query(timeline, '[data-fold-entry]');
      expectThat(fold !== null, "历史思考可折叠");
      click(query(fold, '[data-action="toggle-timeline-entry"]'), "展开历史思考");
      await waitFor(() => fold.getAttribute("data-entry-expanded") === "true", "思考已展开");
      timeline.scrollTop += fold.getBoundingClientRect().top - timeline.getBoundingClientRect().top + 80;
      timeline.dispatchEvent(new Event("scroll"));
      await waitFor(() => query(region, '[data-action="collapse-crossing-fold"]') !== null, "顶部快捷折叠出现");
      click(query(region, '[data-action="collapse-crossing-fold"]'), "快捷收起");
      await waitFor(() => fold.getAttribute("data-entry-expanded") === "false", "对应思考已收起");

      // 将真实中栏限制为最小宽度，验证工具栏不会换行或越出输入边框。
      const column = query(mounted.container, '[data-conversation-column="timeline"]');
      expectThat(column !== null, "中栏存在");
      const sheet = new CSSStyleSheet();
      sheet.replaceSync('[data-conversation-column="timeline"] { width:300px !important; max-width:300px !important; min-width:300px !important; }');
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      try {
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const composer = query(column, '[data-composer]');
        const bar = query(column, '[data-composer-bar]');
        const send = query(bar, '[data-action="submit-prompt"], [data-action="cancel-turn"]');
        const add = query(bar, '[data-action="pick-attachment"]');
        expectThat(composer !== null && bar !== null && send !== null && add !== null, "窄栏控件齐全");
        expectThat(Math.abs(send.getBoundingClientRect().top - add.getBoundingClientRect().top) < 3, "发送与添加不换行");
        expectThat(send.getBoundingClientRect().right <= composer.getBoundingClientRect().right, "发送不超出输入区");
      } finally {
        document.adoptedStyleSheets = document.adoptedStyleSheets.filter((item) => item !== sheet);
      }
    });
  },
}];
