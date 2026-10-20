/**
 * R17：未上报的上下文不得以零冒充（渲染层 + 仓库层）。
 *
 * 三条 MUST 的负向用例：
 *
 * - **未收到用量事件 → 未上报，且不以零冒充**：断言 HTML 里没有任何数字槽位
 *   （`data-context-used`/`data-context-size`/百分比），也没有 `0%`；
 * - **窗口为零 → 同样是未上报，且不算出无意义的占比**：断言与上一种可区分，
 *   且同样没有占比；
 * - **未上报不影响其它交互**：断言发消息/取消/看历史仍然可用（未上报不是禁用理由）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ContextUsageBar } from "./ContextUsageBar";
import { contextWarningOf } from "./ConversationComposer";
import { buildContextUsageView, contextUsageLevel } from "../domain";
import { buildConversationModel } from "../features/conversation-model";
import { resources, session, storeWith } from "../features/testing/fixtures";

/** 渲染上下文条。 */
function renderBar(usage: Parameters<typeof ContextUsageBar>[0]["usage"]): string {
  return renderToStaticMarkup(createElement(ContextUsageBar, { usage }));
}

const HEADER = {
  sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
  title: "修复登录超时",
  agentName: "Claude",
  state: "running" as const,
  directoryId: "work-api",
  originOfflineLabel: null,
};

/** 会话明细：两条消息，只用来让模型有内容可渲染。 */
const PAGE = {
  sessionId: HEADER.sessionId,
  hasEarlier: true,
  earliestCursor: { createdAt: "2026-10-01T00:00:00.000Z", messageId: "m-1" },
  messages: [
    { id: "m-1", role: "user" as const, channel: "reply" as const, text: "开始", content: { text: "开始", media: [], hasStructuredContent: false }, partial: null, hasStructuredContent: false, createdAt: "2026-10-01T00:00:00.000Z", turnId: null },
    { id: "m-2", role: "agent" as const, channel: "reply" as const, text: "好的", content: { text: "好的", media: [], hasStructuredContent: false }, partial: null, hasStructuredContent: false, createdAt: "2026-10-01T00:00:01.000Z", turnId: null },
  ],
};

const CAPABILITIES = { canSend: true, canCancel: true, canViewHistory: true };

describe("R17 三种上下文状态可区分", () => {
  it("从未收到用量事件：呈现未上报，且渲染结果里没有任何数字", () => {
    const html = renderBar(buildContextUsageView(null));

    expect(html).toContain('data-context-state="unreported"');
    // 反例：若把未上报渲染成「0 / 0」或 0%，这里会出现数字槽位或 0%，断言变红。
    expect(html).not.toContain("data-context-used");
    expect(html).not.toContain("data-context-size");
    expect(html).not.toContain("0%");
    expect(html).not.toMatch(/>\s*0\s*</);
  });

  it("窗口为零：呈现未上报（窗口为零），与「从未上报」可区分，且不算占比", () => {
    const html = renderBar(buildContextUsageView({ used: "0", size: "0" }));

    expect(html).toContain('data-context-state="zero_window"');
    expect(html).toContain("窗口为零");
    // 反例：若把 size=0 算成分母为零的除法，这里会出现 NaN 或 0%，断言变红。
    expect(html).not.toContain("NaN");
    expect(html).not.toContain("Infinity");
    expect(html).not.toContain("0%");
    expect(html).not.toContain("data-context-used");
  });

  it("已知用量：呈现已用、上限与占比（对照，证明前两条不是因为组件永远不出数字）", () => {
    const html = renderBar(buildContextUsageView({ used: "30000", size: "120000" }));

    expect(html).toContain('data-context-state="known"');
    expect(html).toContain('data-context-used="30000"');
    expect(html).toContain('data-context-size="120000"');
    expect(html).toContain("25.0%");
  });

  it("三种状态的 DOM 标记互不相同（不可被合并成一个分支）", () => {
    const states = [null, { used: "0", size: "0" }, { used: "1", size: "2" }].map((usage) =>
      buildContextUsageView(usage),
    );
    const marks = states.map((usage) => {
      const html = renderBar(usage);
      const match = /data-context-state="([a-z_]+)"/.exec(html);
      return match?.[1] ?? "";
    });
    expect(new Set(marks).size).toBe(3);
  });
});

describe("R17 未上报不影响其它交互", () => {
  it("未上报时仍可查看历史、发送消息与取消当前轮次", () => {
    const model = buildConversationModel({
      header: HEADER,
      page: PAGE,
      usage: null,
      degraded: [],
      loadingEarlier: false,
      pagingError: null,
      capabilities: CAPABILITIES,
    });

    expect(model.usage.kind).toBe("unreported");
    // 反例：若「未上报」被当成阻断理由，这里会出现 false，断言变红。
    expect(model.capabilities.canViewHistory).toBe(true);
    expect(model.capabilities.canSend).toBe(true);
    expect(model.capabilities.canCancel).toBe(true);
  });

  it("窗口为零时同样不禁用任何交互", () => {
    const model = buildConversationModel({
      header: HEADER,
      page: PAGE,
      usage: { used: "0", size: "0" },
      degraded: [],
      loadingEarlier: false,
      pagingError: null,
      capabilities: CAPABILITIES,
    });

    expect(model.usage.kind).toBe("zero_window");
    expect(model.capabilities).toMatchObject({ canViewHistory: true, canSend: true, canCancel: true });
  });

  it("仓库层：未收到用量事件的会话，能力由连接与来源决定，与用量无关", () => {
    const { store } = storeWith({
      resources: resources({
        sessions: [session({ id: HEADER.sessionId, state: "running" })],
        agents: [],
      }),
    });
    store.apply({ kind: "authenticated", info: { deviceId: "d", serverEpoch: "e", headGlobalSequence: "1", heartbeatIntervalMs: 30000, limits: { maxMessageBytes: 1, maxPromptBytes: 1, maxReplayEventsPerBatch: 1 } } });
    // 让连接走到在线：配对完成 → 开始连接 → socket 打开 → 已订阅 → 追平完成。
    store.pairSucceeded();
    store.apply({ kind: "authenticating" });
    store.apply({ kind: "replaying", cursor: null });
    store.apply({ kind: "online", cursor: { serverEpoch: "e", globalSequence: "1" } });

    const model = store.conversationPage(HEADER.sessionId);
    expect(model).not.toBeNull();
    expect(model?.usage.kind).toBe("unreported");
    expect(model?.capabilities.canSend).toBe(true);
  });

  it("对照：imported 会话来源离线时不得把输入标记为已发送（R20）", () => {
    const { store } = storeWith({
      resources: resources({
        sessions: [
          session({
            id: HEADER.sessionId,
            state: "running",
            origin: { kind: "remote", ownerNodeId: "node-1", exportId: "e", originEpoch: "o", online: false },
          }),
        ],
        agents: [],
      }),
    });

    const model = store.conversationPage(HEADER.sessionId);
    expect(model?.header.originOfflineLabel).not.toBeNull();
    // 反例：若离线也允许标记已发送，这里会变成 true，断言变红。
    expect(model?.capabilities.canSend).toBe(false);
  });
});

/**
 * `design.md` D7 的形态判据：胶囊是「进度条 + 占比 + 已用 / 上限」，点它展开明细。
 *
 * 明细内容**始终在 DOM**（收起只是不显示），因此这里断言的是静置的渲染结果，不需要造交互；
 * 「点得动」由胶囊本身是 `<button>`（`role="button"` 的 RNW 映射）与展开入口锚点表达。
 */
describe("R17 上下文胶囊与明细", () => {
  it("已上报：进度条与占比在胶囊里，明细在 DOM 里给出三个数字与数字来源", () => {
    const html = renderBar(buildContextUsageView({ used: "30000", size: "120000" }));

    // 胶囊是可点控件：RNW 把 `role="button"` 渲染成 `<button>`，展开入口挂在它上面。
    expect(html).toMatch(/<button[^>]*data-action="toggle-context-detail"/);
    expect(html).toContain('data-context-chip-open="false"');
    // 进度条按 `used / size` 算出（轨道 + 填充两段），占比与两个数值在胶囊里。
    expect(html).toContain('data-context-track="true"');
    expect(html).toContain('data-context-fill="true"');
    expect(html).toContain("25.0%");
    // 明细：收起时也在 DOM 里，给出已用、上限、占比，并说明数字来源与「不做推算」。
    expect(html).toContain('data-context-detail="true"');
    expect(html).toContain('data-context-detail-used="30000"');
    expect(html).toContain('data-context-detail-size="120000"');
    expect(html).toContain('data-context-detail-percent="25.0%"');
    expect(html).toContain("数字来自 Agent 上报");
    expect(html).toContain("网页端不做推算");
    // 反例：进度条的宽度若写成 `style={{ width: … }}`，RNW 会把它渲染成内联 `style=` 属性
    // （CSP 基线下会被内核拒掉）→ 这条断言变红。
    expect(html).not.toContain("style=");
  });

  it("未上报：没有进度条（不退化为零占位），明细说明它是未上报而不是零", () => {
    const html = renderBar(buildContextUsageView(null));

    // 反例：把未上报画成一条 0% 的进度条 → 下面两条变红。
    expect(html).not.toContain('data-context-track="true"');
    expect(html).not.toContain('data-context-fill="true"');
    // 明细仍给出三个位置，但一格数字都没有：说「未上报」「未知」「无法计算」。
    expect(html).toContain('data-context-detail-used="未上报"');
    expect(html).toContain('data-context-detail-size="未知"');
    expect(html).toContain('data-context-detail-percent="无法计算"');
    expect(html).not.toContain("style=");
  });
});

/**
 * 原型的两档状态色：`pct>=85 → crit`、`pct>=60 → warn`（`.ctx.warn` / `.ctx.crit` 换掉胶囊的
 * 边框色与填充色，crit 另换底色）。
 *
 * 阈值现在是**一处定义**（`src/domain/context-usage.ts`）：胶囊与输入区告警条共用，因此这里的
 * 边界用例同时覆盖两处——同一个数字（85）决定胶囊进 crit 档与告警条出现。
 */
describe("R17 上下文胶囊的 warn / crit 两档", () => {
  /** 胶囊（`data-context-chip`）的原子类名：RNW 的样式只以 class 落地，档位差异必须体现为 class 差异。 */
  function chipClassOf(html: string): string {
    const tag = /<[a-z]+[^>]*\sdata-context-chip="true"[^>]*>/.exec(html)?.[0] ?? "";
    return /class="([^"]*)"/.exec(tag)?.[1] ?? "";
  }

  it("恰好 60% 进 warn 档、恰好 85% 进 crit 档，两档的边界两侧各自成立", () => {
    // 每一行是「已用 / 上限 → 档位」；占比由 domain 从这两个数算出（`used/size*100`）。
    const cases: readonly (readonly [string, string, string])[] = [
      ["599", "1000", "ok"], // 59.9%：正常档（原型 `pct>=60` 的左侧）
      ["600", "1000", "warn"], // 恰好 60%：warn 档的闭区间下界
      ["849", "1000", "warn"], // 84.9%：warn 档的右侧
      ["850", "1000", "crit"], // 恰好 85%：crit 档的闭区间下界
      ["1000", "1000", "crit"], // 100%：上界
    ];
    for (const [used, size, level] of cases) {
      const html = renderBar(buildContextUsageView({ used, size }));
      // 胶囊与进度条填充各带自己的档位锚点（原型 `.ctx.warn` 与 `.ctx.warn .ctxbar i` 两处选择器）。
      expect(html).toContain(`data-context-chip-level="${level}"`);
      expect(html).toContain(`data-context-fill-level="${level}"`);
    }
    // 反例：档位若只看「已上报」而不看占比，上表会全部落在同一档，`ok`/`warn`/`crit` 里必有一行变红。
  });

  it("未上报与窗口为零没有档位锚点：不参与上色，也不因此变成已上报", () => {
    for (const usage of [buildContextUsageView(null), buildContextUsageView({ used: "0", size: "0" })]) {
      const html = renderBar(usage);
      // 反例：让这两态也去算档位（0% → ok）→ 它们会带上档位锚点与状态色，下面两条变红。
      expect(html).not.toContain("data-context-chip-level");
      expect(html).not.toContain("data-context-fill-level");
      // 上色与否不改变状态锚点：两态仍各自可区分，且仍未破 CSP（没有内联 `style=`）。
      expect(html).toMatch(/data-context-state="(unreported|zero_window)"/);
      expect(html).not.toContain("style=");
    }
  });

  it("阈值只有一处定义：胶囊与告警条在同一个数字上切换", () => {
    // 常量的边界行为（胶囊与告警条共用的判据）。
    expect(contextUsageLevel(59.9)).toBe("ok");
    expect(contextUsageLevel(60)).toBe("warn");
    expect(contextUsageLevel(84.9)).toBe("warn");
    expect(contextUsageLevel(85)).toBe("crit");

    // 告警条取自同一处定义的 85：84.9% 不出现，恰好 85% 出现。
    expect(contextWarningOf(buildContextUsageView({ used: "849", size: "1000" }))).toBeNull();
    expect(contextWarningOf(buildContextUsageView({ used: "850", size: "1000" }))).toContain("上下文已用 85%");
    // 未上报 / 窗口为零没有可比的比例，两处都不参与判定。
    expect(contextWarningOf(buildContextUsageView(null))).toBeNull();
    expect(contextWarningOf(buildContextUsageView({ used: "0", size: "0" }))).toBeNull();
  });

  it("三档在胶囊上真的换了色：档位不同的胶囊用不同的原子类，且仍走样式表", () => {
    const okHtml = renderBar(buildContextUsageView({ used: "100", size: "1000" }));
    const warnHtml = renderBar(buildContextUsageView({ used: "600", size: "1000" }));
    const critHtml = renderBar(buildContextUsageView({ used: "850", size: "1000" }));
    const chipClasses = [okHtml, warnHtml, critHtml].map(chipClassOf);

    expect(chipClasses.every((value) => value.length > 0)).toBe(true);
    // 反例：三档共用同一个 class（即「占比高低外观完全相同」这个偏离）→ 集合大小退回 1。
    // 胶囊自身没有随占比变化的其它样式（宽度类只落在进度条填充上），因此这里的差异只能来自档位。
    expect(new Set(chipClasses).size).toBe(3);
    // CSP：状态色同样只能来自原子类——内联 `style=` 属性会被内核拒掉。
    expect(critHtml).not.toContain("style=");
  });
});
