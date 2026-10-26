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

/** 等一帧（采样窗口用：`requestAnimationFrame` 解析成 Promise）。 */
function nextFrame(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  requestAnimationFrame(() => {
    resolve();
  });
  return promise;
}

/** 等若干毫秒（让若干帧与 React 提交落地）。 */
function afterMs(ms: number): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  return promise;
}

/**
 * **绘制之后**逐帧采样。
 *
 * `requestAnimationFrame` 回调跑在样式与布局之前，而 `ResizeObserver` 的补偿发生在同一帧的布局
 * 之后、绘制之前；直接在 rAF 里读几何会读到「DOM 已变、补偿还没写回」的中间态——那一帧用户根本
 * 看不到，用它判红等于断言一个不存在的帧。这里在 rAF 里再排一个 `setTimeout(0)`，把读取推到本帧
 * 绘制之后，于是断言读到的都是用户实际看到的位置。
 */
function sampleAfterPaint<T>(read: () => T, samples: T[], isRunning: () => boolean): void {
  requestAnimationFrame(() => {
    setTimeout(() => {
      if (!isRunning()) return;
      samples.push(read());
      sampleAfterPaint(read, samples, isRunning);
    }, 0);
  });
}

/**
 * 把中栏改窄，并在宽度真正生效后把控制权交给 `body`，退出时还原。
 *
 * 只改外层 `[data-conversation-column="timeline"]` 不够：内层滚动容器按 `width:100%` 撑满外层
 * flex 列，外层写死宽度未必让它跟着变窄——那样用例会在「什么都没发生」时也绿。所以两层同时写死，
 * 并在 `body` 之前用 `clientWidth` **自证**内层宽度确实变过。
 */
async function withNarrowTimeline(timeline: Element, width: number, body: () => Promise<void>): Promise<void> {
  const sheet = new CSSStyleSheet();
  const rule = (selector: string): string =>
    `${selector} { width:${String(width)}px !important; max-width:${String(width)}px !important; min-width:${String(width)}px !important; }`;
  sheet.replaceSync([
    rule('[data-conversation-column="timeline"]'),
    rule('[data-conversation-column="timeline"] [data-conversation-scroll="timeline"]'),
  ].join("\n"));
  const before = timeline.clientWidth;
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  try {
    await nextFrame();
    expectThat(timeline.clientWidth !== before,
      `宽度触发必须真的改变内层滚动容器：before=${String(before)}px，after=${String(timeline.clientWidth)}px`);
    await body();
  } finally {
    document.adoptedStyleSheets = document.adoptedStyleSheets.filter((item) => item !== sheet);
  }
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
      sampleAfterPaint(() => timeline.scrollTop, samples, () => sampling);
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
      sampleAfterPaint(() => timeline.scrollTop, samples, () => sampling);
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
        sampleAfterPaint(
          () => (reading ? timeline.scrollTop : timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight),
          samples,
          () => sampling,
        );
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
}, {
  name: "SCROLL-SAME-LINE 正文同行追加不改变高度与滚动位置，片段立即可见",
  async run() {
    // 旧实现：每次渲染都无条件测量并（跟随底部时）写 `scrollToY(scrollHeight)`，ScrollView 的
    // 滚动事件会把位置挪一像素；把「正文变化」当成尺寸变化的重构也会在这里红——同行追加不写滚动。
    // 采样与其他逐帧用例一致走 `sampleAfterPaint`：读的是绘制后的位置（用户实际看到的那一帧）。
    const messageId = "b0000000-0000-4000-8000-0000000000a1";
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(timeline !== null, "时间线存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      let sequence = 11;
      let deltaIndex = 0;
      const append = (text: string): void => {
        mounted.host?.deliverEvent(wireEvent({
          globalSequence: sequence++, eventId: `e0000000-0000-4000-8000-${String(sequence).padStart(12, "0")}`,
          sessionId: SESSION_ALPHA, eventType: "agent.message.delta",
          createdAt: "2026-10-01T01:00:00.000Z",
          view: { messageId, turnId: "same-line-turn", deltaIndex: String(deltaIndex++), text },
        }));
      };
      append("同一行");
      await waitFor(() => query(timeline, `[data-message-id="${messageId}"]`) !== null, "新回复的第一段立即呈现");
      await afterMs(80);
      const beforeTop = timeline.scrollTop;
      const beforeHeight = timeline.scrollHeight;
      expectThat(beforeTop > 0, "内容必须真的超过可视高度，位置断言才有意义");
      const samples: number[] = [];
      let sampling = true;
      sampleAfterPaint(() => timeline.scrollTop, samples, () => sampling);
      try {
        // 同一行继续追加：没有换行，实际高度不变。
        append(" ok");
        await waitFor(() => (timeline.textContent ?? "").includes(" ok"), "同行片段立即进入呈现");
        await afterMs(150);
        expectThat(
          Math.abs(timeline.scrollHeight - beforeHeight) < 2,
          `同行追加后实际高度应当不变：before=${String(beforeHeight)}，after=${String(timeline.scrollHeight)}`,
        );
        expectThat(samples.length > 2, "实际采样多帧");
        expectThat(
          samples.every((top) => Math.abs(top - beforeTop) < 1),
          `同行追加不得写滚动：before=${String(beforeTop)}，samples=${samples.join(",")}`,
        );
      } finally { sampling = false; }
    });
  },
}, {
  name: "SCROLL-WRAP 真正换行时底部逐帧跟随，不按固定行数猜高度",
  async run() {
    // 守的是「用户实际看到的每一帧都在底部」：采样本用例走 `sampleAfterPaint`，读的是绘制后的
    // 位置（见该辅助函数的说明）。这里不宣称某种实现必然红，只钉住可见结果——任何一帧被画成
    // 「先落后再纠正」的大位移就会红。
    const messageId = "b0000000-0000-4000-8000-0000000000a2";
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(timeline !== null, "时间线存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      const samples: number[] = [];
      let sampling = true;
      sampleAfterPaint(() => timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight, samples, () => sampling);
      try {
        let sequence = 11;
        for (let index = 0; index < 6; index += 1) {
          mounted.host?.deliverEvent(wireEvent({
            globalSequence: sequence++, eventId: `e0000000-0000-4000-8000-${String(sequence).padStart(12, "0")}`,
            sessionId: SESSION_ALPHA, eventType: "agent.message.delta",
            createdAt: "2026-10-01T01:02:00.000Z",
            view: { messageId, turnId: "wrap-turn", deltaIndex: String(index), text: `第 ${String(index)} 行：换行后的新内容会改变实际高度。\n` },
          }));
          await afterMs(60);
        }
        await waitFor(() => query(timeline, `[data-message-id="${messageId}"]`) !== null, "换行后的正文呈现");
        const height = timeline.scrollHeight;
        await afterMs(150);
        expectThat(height > 0 && samples.length > 4, "实际采样多帧");
        expectThat(
          samples.every((distance) => distance < 2),
          `换行增长时底部必须逐帧跟随：${samples.join(",")}`,
        );
      } finally { sampling = false; }
    });
  },
}, {
  name: "SCROLL-REWRAP 上读时宽度变化与历史前置都逐帧保持阅读位置",
  async run() {
    // 守的是「绘制后的每一帧阅读位置都不动」：两个触发点（宽度变化重排、历史前置）都会在一帧内
    // 改变上方内容的几何，补偿必须在绘制前完成（见 `sampleAfterPaint` 的说明）。
    // 位置读数同时带上条目键：宽度变窄后必须仍是**同一条**条目、同一个偏移。只比位置无法区分
    // 「补偿没生效」与「锚点被重排后的读数覆盖」，而这两者的修法完全不同。
    let reads = 0;
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => {
        reads += 1;
        return readPage({ sessionId, messages: messages(reads === 1 ? 10 : 0), hasEarlier: reads === 1 });
      },
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(timeline !== null, "时间线存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      timeline.scrollTop = 420;
      timeline.dispatchEvent(new Event("scroll"));
      /**
       * 阅读位置读数：**同一条**锚点条目的键 + 它相对容器顶边的偏移。
       *
       * 只量位置会把「锚点身份被换掉」读成「位置抖了 13px」：两者原因完全不同——前者是重排后
       * 的位置覆盖了阅读锚点（补偿从此差那一次重排的量），后者是补偿压根没生效。只报位置无法
       * 区分，因此把条目键一起采下来，断言既要求逐帧位置不动，也要求始终是同一条条目在提供这个
       * 位置。键取自应用自己用的同一个属性（`data-timeline-key`），不另立测量口径。
       */
      interface ReadingAnchor { readonly key: string; readonly offset: number }

      const readingAnchor = (expectedKey?: string): ReadingAnchor | null => {
        const top = timeline.getBoundingClientRect().top;
        const node = queryAll(timeline, "[data-timeline-key]")
          .find((candidate) => expectedKey === undefined
            ? candidate.getBoundingClientRect().bottom > top + 1
            : candidate.getAttribute("data-timeline-key") === expectedKey);
        if (node === undefined) return null;
        return {
          key: node.getAttribute("data-timeline-key") ?? "",
          offset: node.getBoundingClientRect().top - top,
        };
      };
      const describe = (anchor: ReadingAnchor | null): string =>
        anchor === null ? "无可见条目" : `${anchor.key}@${String(anchor.offset)}px`;
      /**
       * 动手**之前**取一次读数作为基线，再逐帧（绘制后）核对每一帧都与它一致。
       *
       * 基线不能取「采样的第一帧」：那样第一帧若已经因重排挪了位，后面整串都会被当成对照。
       */
      const sampleFrames = async (action: () => Promise<void>, label: string): Promise<void> => {
        const baseline = readingAnchor();
        expectThat(baseline !== null, `${label}：动手前的位置上必须有可见条目`);
        const samples: (ReadingAnchor | null)[] = [];
        let sampling = true;
        // 后续每帧都量同一条基线条目。分页前置后，旧条目的位置只会上移；若每帧重新找
        // “当前第一条可见条目”，会换成另一条而把真实的位置保持误报成跳动。
        sampleAfterPaint(() => readingAnchor(baseline.key), samples, () => sampling);
        try {
          await action();
          // 动作结束后继续采几帧：还原宽度等二次重排同样是一次真实补偿。
          await afterMs(160);
          expectThat(samples.length > 2, `${label}：实际采样多帧`);
          expectThat(
            samples.every((value) => value !== null && value.key === baseline.key &&
              Math.abs(value.offset - baseline.offset) < 2),
            `${label}：绘制后的每一帧都必须由同一条可见条目保持同一阅读位置：baseline=${describe(baseline)}，samples=${samples.map(describe).join(",")}`,
          );
        } finally { sampling = false; }
      };

      // ① 宽度变化让所有长段落重新换行：`withNarrowTimeline` 先用 `clientWidth` 自证内层滚动
      //    容器真的变窄了（只改外层样式可能什么都没变），再核对阅读位置。
      await sampleFrames(() => withNarrowTimeline(timeline, 320, async () => undefined), "宽度变化");

      // ② 分页前置：滚到顶部触继续读取，前置的历史不得把当前阅读内容推走。
      timeline.scrollTop = 40;
      timeline.dispatchEvent(new Event("scroll"));
      await sampleFrames(async () => {
        await waitFor(() => queryAll(mounted.container, "[data-message-id]").length === 60, "前置历史到达");
      }, "历史前置");
    });
  },
}, {
  name: "SCROLL-INTENT 程序滚动补偿不改变跟随状态与入口，真正上读才出现回到底部",
  async run() {
    // D4：回到底部入口不得因补偿写入的滚动事件而切换。旧实现没有「程序意图」这一层，
    // 一旦补偿落点跨进跟随阈值，滚动事件就会把入口切掉；本用例钉住「补偿不改写跟随状态」。
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "历史就绪", async (mounted) => {
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      const region = query(mounted.container, '[data-conversation-region="timeline"]');
      expectThat(timeline !== null && region !== null, "对话区存在");
      await waitFor(() => timeline.scrollTop > 400, "首次贴底");
      // 贴底时入口不出现（跟随状态由真实几何决定）。
      expectEqual(query(region, '[data-action="scroll-to-bottom"]'), null, "贴底时不呈现回到底部入口");

      // 上读：入口出现，且跟随标记为 false。
      timeline.scrollTop = Math.max(0, timeline.scrollHeight - timeline.clientHeight - 120);
      timeline.dispatchEvent(new Event("scroll"));
      await waitFor(() => query(region, '[data-action="scroll-to-bottom"]') !== null, "上读后出现回到底部入口");
      expectEqual(timeline.getAttribute("data-scroll-follow"), "false", "上读后不再跟随底部");

      // 驱动一次真实的程序补偿（宽度变化导致重排）：入口与跟随标记都不得变化。
      // `withNarrowTimeline` 先自证内层滚动容器真的变窄；采样在绘制之后读，因此断言的是
      // 「用户看到的每一帧都没有闪动」，而不是补偿写入前的那一帧。
      const followed: string[] = [];
      let sampling = true;
      sampleAfterPaint(
        () => `${timeline.getAttribute("data-scroll-follow") ?? "?"}/${String(query(region, '[data-action="scroll-to-bottom"]') !== null)}`,
        followed,
        () => sampling,
      );
      try {
        await withNarrowTimeline(timeline, 320, async () => {
          await afterMs(150);
        });
        // 还原宽度同样是重排：再多采几帧。
        await afterMs(80);
      } finally {
        sampling = false;
      }
      expectThat(followed.length > 2, "实际采样多帧");
      expectThat(
        followed.every((value) => value === "false/true"),
        `补偿期间跟随状态与入口不得切换：${followed.join(",")}`,
      );

      // 真正回到用户手上：滚到底部后入口消失。
      timeline.scrollTop = timeline.scrollHeight;
      timeline.dispatchEvent(new Event("scroll"));
      await waitFor(() => query(region, '[data-action="scroll-to-bottom"]') === null, "真正回到底部后入口移除");
      expectEqual(timeline.getAttribute("data-scroll-follow"), "true", "回到底部后恢复跟随");
    });
  },
}, {
  name: "SCROLL-COMPACT keeps streamed rows mounted until the authoritative read replaces them",
  async run() {
    const userId = "b0000000-0000-4000-8000-000000000710";
    const agentId = "b0000000-0000-4000-8000-000000000711";
    const turnId = "d0000000-0000-4000-8000-000000000732";
    await withSessionPage(sessionScenario({
      readResult: (sessionId) => readPage({ sessionId, messages: messages(10) }),
    }), (container) => queryAll(container, "[data-message-id]").length === 30, "history ready", async (mounted) => {
      const host = mounted.host;
      const timeline = query(mounted.container, '[data-conversation-scroll="timeline"]');
      expectThat(host !== null && timeline !== null, "timeline and fake host are present");
      await waitFor(() => timeline.scrollTop > 400, "initial bottom follow");

      host.holdCommand("session.read");
      let sequence = 11;
      const deliver = (eventType: string, view: Record<string, unknown>): void => {
        sequence += 1;
        host.deliverEvent(wireEvent({
          globalSequence: sequence,
          eventId: `e0000000-0000-4000-8000-${String(sequence + 700).padStart(12, "0")}`,
          sessionId: SESSION_ALPHA,
          eventType,
          view,
        }));
      };
      deliver("user.message.delta", {
        messageId: userId, turnId, deltaIndex: "0", text: "short prompt", source: "owned_prompt",
      });
      deliver("agent.message.delta", {
        messageId: agentId, turnId, deltaIndex: "0",
        text: "streamed reply\n".repeat(18),
      });
      await waitFor(
        () => query(mounted.container, `[data-message-id="${userId}"]`) !== null &&
          query(mounted.container, `[data-message-id="${agentId}"]`) !== null,
        "streamed rows visible",
      );
      const userNode = query(timeline, `[data-message-id="${userId}"]`);
      const agentNode = query(timeline, `[data-message-id="${agentId}"]`);
      expectThat(userNode !== null && agentNode !== null, "stream node identities captured");

      deliver("turn.completed", { turnId, state: "completed" });
      deliver("turn.delta_compacted", { turnId, deltaCount: "1" });
      await waitFor(
        () => commandBodies(host.sent, "session.read").length >= 2,
        "compaction requested an authoritative refresh",
      );
      await new Promise(resolve => setTimeout(resolve, 150));

      expectThat(userNode?.isConnected === true, "user row remains mounted while refresh is pending");
      expectThat(agentNode?.isConnected === true, "agent row remains mounted while refresh is pending");
      expectThat(
        timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight <= 2,
        "streamed content remains pinned while refresh is pending",
      );

      expectThat(host.completeHeldCommand("session.read", {
        result: readPage({
          sessionId: SESSION_ALPHA,
          messages: [
            ...messages(10),
            readMessage({
              messageId: userId, sessionId: SESSION_ALPHA, role: "user", text: "short prompt",
              createdAt: "2026-10-01T00:10:01.000Z", turnId,
            }),
            readMessage({
              messageId: agentId, sessionId: SESSION_ALPHA, role: "agent",
              text: "streamed reply\n".repeat(18),
              createdAt: "2026-10-01T00:10:02.000Z", turnId,
            }),
          ],
        }),
      }), "held authoritative read completed");
      await waitFor(
        () => query(mounted.container, `[data-message-id="${agentId}"][data-message-volatile="false"]`) !== null,
        "authoritative rows installed",
      );
      expectThat(
        query(mounted.container, `[data-message-id="${userId}"]`) === userNode &&
          query(mounted.container, `[data-message-id="${agentId}"]`) === agentNode,
        "authoritative read replaces content in place",
      );
      await waitFor(
        () => timeline.scrollHeight - timeline.scrollTop - timeline.clientHeight <= 2,
        "authoritative replacement remains at bottom",
      );
    });
  },
}];
