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
import { buildContextUsageView } from "../domain";
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
  directoryAlias: "work-api",
  originOfflineLabel: null,
};

/** 会话明细：两条消息，只用来让模型有内容可渲染。 */
const PAGE = {
  sessionId: HEADER.sessionId,
  hasEarlier: true,
  earliestCursor: { createdAt: "2026-10-01T00:00:00.000Z", messageId: "m-1" },
  messages: [
    { id: "m-1", role: "user" as const, text: "开始", hasStructuredContent: false, createdAt: "2026-10-01T00:00:00.000Z", turnId: null },
    { id: "m-2", role: "agent" as const, text: "好的", hasStructuredContent: false, createdAt: "2026-10-01T00:00:01.000Z", turnId: null },
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
      scopes: ["session.read"],
    });
    store.apply({ kind: "authenticated", info: { deviceId: "d", scopes: ["session.read"], serverEpoch: "e", headGlobalSequence: "1", heartbeatIntervalMs: 30000, limits: { maxMessageBytes: 1, maxPromptBytes: 1, maxReplayEventsPerBatch: 1 } } });
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
      scopes: ["session.read"],
    });

    const model = store.conversationPage(HEADER.sessionId);
    expect(model?.header.originOfflineLabel).not.toBeNull();
    // 反例：若离线也允许标记已发送，这里会变成 true，断言变红。
    expect(model?.capabilities.canSend).toBe(false);
  });
});
