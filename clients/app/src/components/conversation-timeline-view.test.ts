/**
 * WP7 · 对话时间线的呈现（spec「对话页按时间线呈现消息与结构化条目」「Agent 产生的富文本按
 * 不可信输入渲染」「结构化条目的展开收起与右栏联动」；design D1/D5/D6/D8/D10）。
 *
 * ## 判别力从哪来
 *
 * - **同一容器**：断言消息条目与结构化卡片都是**同一个** `data-timeline="entries"` 列表的直接
 *   子项（`<li>`），而不是「两块列表各自渲染」——后者会让消息与卡片的分时顺序无从表达。
 * - **每类卡片唯一锚点、恰好出现一次**：`data-timeline-card` 的取值按种类区分，抽样夹具里每类
 *   各一条事件，因此每个取值在渲染结果里必须**恰好**出现一次（多一处即重复呈现）。
 * - **同一标识只呈现一处**：同一 `eventId` 先实时到达、后随 `session.read` 的历史条目再来一次，
 *   时间线上仍然只有一张卡片（按来源各存一份时会变成两张）。
 * - **正文按不可信输入渲染**：消息正文里的原始 HTML 只作为字面文本出现（`data-markdown-omitted`），
 *   渲染结果里没有 `<script`、没有 `<img`、也没有链接的 `href`。
 * - **授权边界**：只有亲历且拿到可选项的请求才渲染应答按钮；历史读到的待处理项、
 *   已被其它设备处理的、本设备已提交待确认的，一律**没有**可点按钮。
 * - **条目形态**：用户气泡与 Agent 气泡是两档（`class` 集合不同）；终端输出是一张代码卡
 *   （语言条 + 行号 + 复制）；进行中的轮次带运行指示、失败的轮次是一条失败条。
 * - **折叠默认值由状态决定**：进行中的工具调用 `data-entry-expanded="true"`、已完成的
 *   `"false"`；同一份条目换成另一个状态就换默认值（不看它在列表里的位置）。
 * - **工具详情只来自结构化视图**：参数/输出/文本内容与截断说明逐部分呈现，没有详情时不画空
 *   占位；原文入口是诊断（`acp.rawJson` 不参与详情渲染）。
 * - **显式覆盖只影响本条**：给同状态的两条里的一条写覆盖值，另一条的有效值不变。
 * - **收起不丢内容**：收起时正文仍在 DOM 里（`display: none`），因此「内容消失」与「收起」是
 *   两件事。
 * - **联动到右栏**：时间线上带文件标识的条目与右栏改动清单共用同一个选中值。
 */

import { createElement, useState } from "react";
import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { RefreshInteractionsAction } from "./ConversationTimeline";
import { ConversationStream } from "./index";
import type { ConversationStreamProps } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import type { InteractionAvailability } from "../domain/interaction";
import { PENDING_INTERACTIONS_FEATURE, SESSION_HISTORY_FEATURE } from "../sync-client";
import type { StoreFixture } from "../features/testing/fixtures";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  historyItem,
  interactionStore,
  lastDispatch,
  messageItem,
  onlineStore,
  readResult,
  resources,
  session,
} from "../features/testing/fixtures";

/** 渲染对话页正文（可注入折叠覆盖值、选中项与回调）。 */
function renderPage(model: ConversationPageModel, extra: Partial<ConversationStreamProps> = {}): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model,
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
      onAnswerInteraction: () => undefined,
      ...extra,
    }),
  );
}

/** 取模型（夹具会话一定在快照里）。 */
function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null：快照里有该会话");
  return model;
}

/** 一个已在线、协商到结构化历史的仓库。 */
function store(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
    features: [SESSION_HISTORY_FEATURE],
  });
}

/** 注入一条实时事件。 */
function applyEvent(store: ClientStore, input: Parameters<typeof eventMessage>[0]): void {
  store.apply({ kind: "event", event: eventMessage({ ...input, sessionId: input.sessionId ?? ONLINE_SESSION_ID }) });
}

/** 渲染结果里全部 `data-*` 属性（键与取值都在内）。 */
function dataAttributes(html: string): readonly string[] {
  return html.match(/data-[a-zA-Z0-9-]+="[^"]*"/g) ?? [];
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

/** 标签原文里某个属性的值（读判别值用；缺席即判红）。 */
function attributeOf(tag: string, name: string): string {
  const match = new RegExp(`${name}="([^"]*)"`).exec(tag);
  expect(match, `${name} 不在 ${tag} 里`).not.toBeNull();
  return match?.[1] ?? "";
}

/** 标签原文里的 `class` 属性（两档样式的机器证据：同一档就是同一个样式集合）。 */
function classOf(tag: string): string {
  return attributeOf(tag, "class");
}

/** 卡片条目的键：它最早那条记录的 `eventId`（`features/conversation-timeline.ts` 的 `cardEntry`）。 */
function foldKey(eventId: string): string {
  return `event:${eventId}`;
}

/** 某个折叠块的有效展开值（`data-fold-entry` → `data-entry-expanded`）。 */
function foldExpanded(html: string, entryKey: string): string {
  return attributeOf(tagContaining(html, `data-fold-entry="${entryKey}"`), "data-entry-expanded");
}

/** 每类卡片各注入一条事件（`eventId` 各不相同）。 */
function applyOneOfEachCard(store: ClientStore): void {
  let sequence = 20;
  const next = (): string => {
    sequence += 1;
    return String(sequence);
  };
  applyEvent(store, {
    eventType: "turn.started",
    eventId: "dddddddd-0000-4000-8000-000000000101",
    globalSequence: next(),
    view: { turnId: "turn-1", state: "running" },
  });
  applyEvent(store, {
    eventType: "tool.call.started",
    eventId: "dddddddd-0000-4000-8000-000000000102",
    globalSequence: next(),
    view: { toolCallId: "tool-1", turnId: "turn-1", title: "读取 src/a.ts", state: "running" },
  });
  applyEvent(store, {
    eventType: "terminal.output",
    eventId: "dddddddd-0000-4000-8000-000000000103",
    globalSequence: next(),
    view: { terminalId: "term-1", chunkIndex: "0", stream: "stderr", text: "boom", truncated: true },
  });
  applyEvent(store, {
    eventType: "session.plan.changed",
    eventId: "dddddddd-0000-4000-8000-000000000104",
    globalSequence: next(),
    view: { entries: [{ content: "改超时", priority: "high", status: "in_progress" }] },
  });
  applyEvent(store, {
    eventType: "file.changed",
    eventId: "dddddddd-0000-4000-8000-000000000105",
    globalSequence: next(),
    view: {
      changeId: "11111111-0000-4000-8000-0000000000c1",
      kind: "update",
      displayPath: "src/a.ts",
      summary: "改了 3 行",
      addedLines: "2",
      deletedLines: "1",
      outsideWorkspace: false,
    },
  });
  applyEvent(store, {
    eventType: "session.mode.changed",
    eventId: "dddddddd-0000-4000-8000-000000000106",
    globalSequence: next(),
    view: { currentModeId: "code", version: "3" },
  });
  applyEvent(store, {
    eventType: "session.config.changed",
    eventId: "dddddddd-0000-4000-8000-000000000107",
    globalSequence: next(),
    view: {
      version: "4",
      configOptions: [
        { id: "model", name: "模型", description: null, category: "model", type: "select", currentValue: "fast", options: [{ value: "fast", name: "快", description: null }] },
      ],
    },
  });
  applyEvent(store, {
    eventType: "permission.requested",
    eventId: "dddddddd-0000-4000-8000-000000000108",
    globalSequence: next(),
    view: {
      interactionId: "perm-1",
      turnId: "turn-1",
      title: "运行 npm test",
      description: "会改动 tests 下的快照",
      options: [
        { optionId: "allow-once", label: "允许一次", kind: "allow_once" },
        { optionId: "reject", label: "拒绝", kind: "reject_once" },
      ],
    },
  });
  applyEvent(store, {
    eventType: "elicitation.requested",
    eventId: "dddddddd-0000-4000-8000-000000000109",
    globalSequence: next(),
    view: { interactionId: "ask-1", title: "选一个分支", schema: { type: "object" } },
  });
  applyEvent(store, {
    eventType: "permission.resolved",
    eventId: "dddddddd-0000-4000-8000-00000000010a",
    globalSequence: next(),
    view: { interactionId: "perm-1", resolution: "allow-once" },
  });
  applyEvent(store, {
    eventType: "elicitation.resolved",
    eventId: "dddddddd-0000-4000-8000-00000000010b",
    globalSequence: next(),
    view: { interactionId: "ask-1", action: "decline" },
  });
}

describe("对话时间线：消息与结构化条目同一条时间线", () => {
  it("每类卡片各有唯一锚点，且每个锚点在渲染结果里恰好出现一次", () => {
    const { store: client } = store();
    applyOneOfEachCard(client);
    const html = renderPage(pageOf(client));

    const kinds = [
      "turn",
      "tool-call",
      "terminal-output",
      "plan",
      "file-changed",
      "mode-changed",
      "config-changed",
      "permission-requested",
      "elicitation-requested",
      "permission-resolved",
      "elicitation-resolved",
    ];
    for (const kind of kinds) {
      expect(html.match(new RegExp(`data-timeline-card="${kind}"`, "g")), kind).toHaveLength(1);
    }
    // 每类卡片自己的标识锚点也在（唯一）。
    expect(html.match(/data-turn-id="turn-1"/g)).toHaveLength(1);
    expect(html.match(/data-tool-call-id="tool-1"/g)).toHaveLength(1);
    expect(html.match(/data-terminal-id="term-1"/g)).toHaveLength(1);
    expect(html.match(/data-change-id="11111111-0000-4000-8000-0000000000c1"/g)).toHaveLength(1);
  });

  it("消息条目与结构化卡片是同一个列表容器的直接子项（`<ul>`/`<li>`）", () => {
    const { store: client, gateway } = store();
    applyEvent(client, {
      eventType: "turn.started",
      eventId: "dddddddd-0000-4000-8000-000000000201",
      globalSequence: "31",
      view: { turnId: "turn-9", state: "running" },
    });
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:02.000Z" })],
      }),
    });
    const html = renderPage(pageOf(client));

    // 唯一容器：role="list" → `<ul>`，并带时间线锚点与既有消费方的消息列表锚点。
    const container = tagContaining(html, 'data-timeline="entries"');
    expect(container.startsWith("<ul")).toBe(true);
    expect(container).toContain('role="list"');
    expect(container).toContain('data-message-list="messages"');
    expect(html.match(/data-timeline="entries"/g)).toHaveLength(1);

    // 两条条目都是这个 `<ul>` 的直接子项（`<li>`），因此「同一条时间线」在结构上成立。
    expect(tagContaining(html, 'data-message-id="m-1"').startsWith("<li")).toBe(true);
    expect(tagContaining(html, 'data-timeline-card="turn"').startsWith("<li")).toBe(true);
    // 反例：若消息与卡片各画一块列表，下面这条「同容器」断言会红。
    // `</ul>` 从时间线容器往后找：页面上还有别的列表（对话页左栏的同目录会话列表也是一个
    // `<ul>`），从文档开头找会撞上它。
    expect(html.indexOf('data-message-id="m-1"')).toBeLessThan(
      html.indexOf("</ul>", html.indexOf('data-timeline="entries"')),
    );
  });

  it("按时间排序：早于消息的结构化条目排在消息之前", () => {
    const { store: client, gateway } = store();
    applyEvent(client, {
      eventType: "turn.started",
      eventId: "dddddddd-0000-4000-8000-000000000202",
      globalSequence: "32",
      view: { turnId: "turn-8", state: "running" },
    });
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [messageItem({ id: "m-late", createdAt: "2026-10-01T00:00:30.000Z" })],
      }),
    });
    const html = renderPage(pageOf(client));

    // 事件时间是 00:00:02，消息时间是 00:00:30（同一 `createdAt` 的次级排序见 domain 单测）。
    expect(html.indexOf('data-timeline-card="turn"')).toBeLessThan(html.indexOf('data-message-id="m-late"'));
  });

  it("同一 eventId 实时与历史两条来源并存：仍然只有一处", () => {
    const { store: client, gateway } = store();
    const sharedEventId = "dddddddd-0000-4000-8000-000000000203";
    const view = {
      changeId: "11111111-0000-4000-8000-0000000000c2",
      kind: "update",
      displayPath: "src/b.ts",
      summary: "改了 1 行",
      addedLines: "1",
      deletedLines: "0",
      outsideWorkspace: false,
    };
    applyEvent(client, {
      eventType: "file.changed",
      eventId: sharedEventId,
      globalSequence: "33",
      view,
    });
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        history: [
          historyItem({ eventId: sharedEventId, eventType: "file.changed", sessionSequence: "5", view }),
        ],
      }),
    });
    const html = renderPage(pageOf(client));

    // 反例：按来源各留一份时会变成 2 → 断言变红。
    expect(html.match(/data-timeline-card="file-changed"/g)).toHaveLength(1);
    expect(html.match(/data-change-id="11111111-0000-4000-8000-0000000000c2"/g)).toHaveLength(1);
  });
});

describe("对话时间线：正文按不可信输入渲染", () => {
  it("消息正文走 Markdown，原始 HTML 只作为字面文本出现", () => {
    const { store: client, gateway } = store();
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [
          messageItem({
            id: "m-x",
            createdAt: "2026-10-01T00:00:03.000Z",
            role: "agent",
            text: "# 标题\n\n<script>alert(1)</script>\n\n![远端](https://example.com/a.png)\n\n[外链](https://example.com)",
          }),
        ],
      }),
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-message-text="true"');
    expect(html).toContain('data-markdown="true"');
    expect(html).toContain('data-markdown-block="heading"');
    // 反例：若原始 HTML 被渲染，下面三条会各自变红（`<script` 会在渲染结果里出现）。
    expect(html).toContain('data-markdown-omitted="html"');
    expect(html).not.toContain("<script");
    expect(html).not.toContain("<img");
    expect(html).not.toContain('href="https://example.com"');
  });

  it("终端输出走代码块（等宽 + 语言标记 + 复制入口），并原样带出 stream", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "terminal.output",
      eventId: "dddddddd-0000-4000-8000-000000000204",
      globalSequence: "34",
      view: { terminalId: "term-2", chunkIndex: "1", stream: "stdout", text: "ok\n" },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-terminal-stream="stdout"');
    expect(html).toContain('data-terminal-chunk="1"');
    expect(html).toContain('data-code-block="true"');
    expect(html).toContain('data-code-language="stdout"');
    expect(html).toContain('data-action="copy-code"');
    // 终端块默认收起（它不是「进行中」），但正文仍在 DOM 里：收起不等于内容消失。
    expect(foldExpanded(html, foldKey("dddddddd-0000-4000-8000-000000000204"))).toBe("false");
  });

  it("流式增量是易失呈现：读取结果到达后同一 messageId 只留一处", () => {
    const { store: client, gateway } = store();
    const messageId = "cccccccc-0000-4000-8000-0000000000b1";
    applyEvent(client, {
      eventType: "agent.message.delta",
      eventId: "dddddddd-0000-4000-8000-000000000205",
      globalSequence: "35",
      view: { messageId, turnId: "turn-1", deltaIndex: "0", text: "半句" },
    });
    const volatileHtml = renderPage(pageOf(client));
    expect(volatileHtml).toContain('data-message-volatile="true"');
    expect(volatileHtml).toContain('data-message-volatile-note="true"');

    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [
          messageItem({ id: messageId, createdAt: "2026-10-01T00:00:02.000Z", role: "agent", text: "权威正文" }),
        ],
      }),
    });
    const readHtml = renderPage(pageOf(client));
    expect(readHtml.match(new RegExp(`data-message-id="${messageId}"`, "g"))).toHaveLength(1);
    expect(readHtml).toContain('data-message-volatile="false"');
    expect(readHtml).toContain("权威正文");
  });
});

describe("对话时间线：条目形态与原型对齐", () => {
  it("用户消息与 Agent 气泡是两档样式（靠右的用户气泡 vs 另一个气泡档）", () => {
    const { store: client, gateway } = store();
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [
          messageItem({ id: "m-user", createdAt: "2026-10-01T00:00:01.000Z", role: "user", text: "先修那条" }),
          messageItem({ id: "m-agent", createdAt: "2026-10-01T00:00:02.000Z", role: "agent", text: "好，从 `session.ts` 开始看。" }),
        ],
      }),
    });
    const html = renderPage(pageOf(client));

    const userEntry = tagContaining(html, 'data-message-id="m-user"');
    const agentEntry = tagContaining(html, 'data-message-id="m-agent"');
    expect(userEntry.startsWith("<li")).toBe(true);
    expect(agentEntry.startsWith("<li")).toBe(true);
    // 反例：两条气泡共用一档样式（用户那条不再靠右）→ 这里相等。
    expect(classOf(userEntry)).not.toBe(classOf(agentEntry));

    // 气泡本体也是两档：用户气泡（右对齐 + 强调底色）与 Agent 气泡（面板底色）样式集合不同。
    const userBubble = tagContaining(html.slice(html.indexOf('data-message-id="m-user"')), 'data-message-text="true"');
    const agentBubble = tagContaining(
      html.slice(html.indexOf('data-message-id="m-agent"')),
      'data-message-text="true"',
    );
    expect(classOf(userBubble)).not.toBe(classOf(agentBubble));

    expect(userEntry).toContain('data-message-channel="user"');
    expect(agentEntry).toContain('data-message-channel="agent"');
    expect(html).toContain('data-message-who="user"');
    expect(html).toContain('data-message-who="agent"');
    expect(html).toContain("你");
    expect(html).toContain("Agent");
  });

  it("进行中的轮次带运行指示，失败的轮次是一条失败条", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "turn.started",
      eventId: "dddddddd-0000-4000-8000-000000000206",
      globalSequence: "36",
      view: { turnId: "turn-run", state: "running" },
    });
    applyEvent(client, {
      eventType: "turn.failed",
      eventId: "dddddddd-0000-4000-8000-000000000207",
      globalSequence: "37",
      view: {
        turnId: "turn-fail",
        state: "failed",
        error: { code: "session.busy", message: "Agent 进程异常退出", retryable: true, details: {} },
      },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-turn-running="true"');
    expect(html).toContain("Agent 正在输出");
    expect(html).toContain('data-turn-state="failed"');
    expect(html).toContain('data-turn-failure="true"');
    expect(html).toContain('data-turn-error="session.busy"');
    expect(html).toContain('data-turn-error-retryable="true"');
    expect(html).toContain("轮次失败：Agent 进程异常退出");
  });
});

describe("对话时间线：结构化条目的展开收起（D10）", () => {
  it("默认展开值由条目状态决定：进行中的展开，已落地的收起", () => {
    const { store: client } = store();
    let sequence = 40;
    const apply = (eventId: string, eventType: string, view: Record<string, unknown>, toolCallId: string): void => {
      sequence += 1;
      applyEvent(client, {
        eventType,
        eventId,
        globalSequence: String(sequence),
        view: { toolCallId, turnId: "turn-1", title: "读取 src/a.ts", ...view },
      });
    };
    apply("dddddddd-0000-4000-8000-000000000301", "tool.call.started", { state: "in_progress" }, "tool-open");
    apply("dddddddd-0000-4000-8000-000000000302", "tool.call.completed", { state: "completed" }, "tool-done");
    const html = renderPage(pageOf(client), { onToggleTimelineEntry: () => undefined });

    // 同一条时间线、同一个回调：默认值只跟着条目自己的状态走（进行中 → 展开）。
    expect(foldExpanded(html, foldKey("dddddddd-0000-4000-8000-000000000301"))).toBe("true");
    expect(foldExpanded(html, foldKey("dddddddd-0000-4000-8000-000000000302"))).toBe("false");
    // 表头是显式操作入口，并带上该条目的键与当前有效值（页面据此写覆盖值）。
    expect(html).toContain('data-action="toggle-timeline-entry"');
    expect(html).toContain(`data-entry-key="${foldKey("dddddddd-0000-4000-8000-000000000301")}"`);
  });

  it("思考在流出时展开、落地后收起；计划有进行中的一步时展开", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "agent.thought.delta",
      eventId: "dddddddd-0000-4000-8000-000000000303",
      globalSequence: "45",
      view: { messageId: "th-1", turnId: "turn-1", deltaIndex: "0", text: "先看关闭路径" },
    });
    applyEvent(client, {
      eventType: "session.plan.changed",
      eventId: "dddddddd-0000-4000-8000-000000000304",
      globalSequence: "46",
      view: { entries: [{ content: "改超时", priority: "high", status: "in_progress" }] },
    });
    applyEvent(client, {
      eventType: "session.plan.changed",
      eventId: "dddddddd-0000-4000-8000-000000000305",
      globalSequence: "47",
      view: { entries: [{ content: "改超时", priority: "high", status: "completed" }] },
    });
    const streaming = renderPage(pageOf(client));

    expect(foldExpanded(streaming, "message:th-1")).toBe("true");
    expect(foldExpanded(streaming, foldKey("dddddddd-0000-4000-8000-000000000304"))).toBe("true");
    expect(foldExpanded(streaming, foldKey("dddddddd-0000-4000-8000-000000000305"))).toBe("false");

    // 增量流结束（`agent.message.completed`）后同一条思考的默认值变成「收起」。
    applyEvent(client, {
      eventType: "agent.message.completed",
      eventId: "dddddddd-0000-4000-8000-000000000306",
      globalSequence: "48",
      view: { messageId: "th-1", turnId: "turn-1", content: [] },
    });
    expect(foldExpanded(renderPage(pageOf(client)), "message:th-1")).toBe("false");
  });

  it("显式覆盖只影响本条：同状态的两条里被指定的那条收起", () => {
    const { store: client } = store();
    let sequence = 50;
    const apply = (eventId: string, toolCallId: string): void => {
      sequence += 1;
      applyEvent(client, {
        eventType: "tool.call.started",
        eventId,
        globalSequence: String(sequence),
        view: { toolCallId, turnId: "turn-1", title: "读取 src/a.ts", state: "in_progress" },
      });
    };
    const first = "dddddddd-0000-4000-8000-000000000401";
    const second = "dddddddd-0000-4000-8000-000000000402";
    apply(first, "tool-a");
    apply(second, "tool-b");
    const model = pageOf(client);

    // 两条都是「进行中」，默认都展开。
    const defaults = renderPage(model);
    expect(foldExpanded(defaults, foldKey(first))).toBe("true");
    expect(foldExpanded(defaults, foldKey(second))).toBe("true");

    // 只对第一条写显式覆盖 → 只有它收起，同状态的另一条不变。
    const overridden = renderPage(model, { expandedTimelineEntries: { [foldKey(first)]: false } });
    expect(foldExpanded(overridden, foldKey(first))).toBe("false");
    expect(foldExpanded(overridden, foldKey(second))).toBe("true");

    // 显式展开也能覆盖默认收起（例如终端块）。
    applyEvent(client, {
      eventType: "terminal.output",
      eventId: "dddddddd-0000-4000-8000-000000000403",
      globalSequence: "60",
      view: { terminalId: "term-open", chunkIndex: "0", stream: "stdout", text: "ok", truncated: false },
    });
    const opened = renderPage(pageOf(client), { expandedTimelineEntries: { [foldKey("dddddddd-0000-4000-8000-000000000403")]: true } });
    expect(foldExpanded(opened, foldKey("dddddddd-0000-4000-8000-000000000403"))).toBe("true");
  });

  it("回调缺省时表头不是可点控件（不是点了没反应的按钮）", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "tool.call.started",
      eventId: "dddddddd-0000-4000-8000-000000000404",
      globalSequence: "61",
      view: { toolCallId: "tool-x", turnId: "turn-1", title: "读取 src/a.ts", state: "in_progress" },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-fold-entry="event:dddddddd-0000-4000-8000-000000000404"');
    expect(html).not.toContain('data-action="toggle-timeline-entry"');
  });
});

describe("对话时间线：工具调用的详情与原文入口", () => {
  it("渲染参数/输出/文本内容与截断说明，并提供查看 ACP 原文的入口", () => {
    const { store: client } = store();
    const eventId = "dddddddd-0000-4000-8000-000000000601";
    applyEvent(client, {
      eventType: "tool.call.started",
      eventId,
      globalSequence: "80",
      view: {
        toolCallId: "tool-detail",
        turnId: "turn-1",
        title: "读取 src/a.ts",
        state: "in_progress",
        toolDetail: {
          input: { path: "src/a.ts", limit: 10 },
          output: { bytes: 42 },
          text: ["第一段", "第二段"],
          truncatedParts: ["output"],
        },
      },
    });
    const html = renderPage(pageOf(client));

    // 进行中的工具调用默认展开，详情就在可观察的渲染结果里。
    expect(html).toContain('data-tool-detail="present"');
    expect(html).toContain('data-tool-detail-part="input"');
    expect(html).toContain('data-tool-detail-part="output"');
    expect(html).toContain('data-tool-detail-part="text"');
    expect(html).toContain("参数");
    expect(html).toContain("输出");
    expect(html).toContain("文本内容");
    // JSON 值按稳定 pretty-print 呈现（缩进与键值都在；引号在 HTML 文本里被转义）。
    expect(html).toContain("  &quot;path&quot;: &quot;src/a.ts&quot;");
    expect(html).toContain("bytes");
    expect(html).toContain("第一段");
    expect(html).toContain("第二段");
    // 截断登记逐部分说明，而不是悄悄缺失。
    expect(html).toContain('data-tool-detail-truncated="output"');
    expect(html).toContain("输出因超过上限未下发");
    // 原文入口按最新记录的 eventId 打开诊断。
    expect(html).toContain('data-action="open-tool-call-diagnostics"');
    expect(tagContaining(html, 'data-action="open-tool-call-diagnostics"')).toContain(`data-event-id="${eventId}"`);
    expect(html).toContain("查看 ACP 原文");
  });

  it("没有 detail 时不渲染空占位；acp.rawJson 里的伪详情不参与渲染", () => {
    const { store: client } = store();
    const eventId = "dddddddd-0000-4000-8000-000000000602";
    applyEvent(client, {
      eventType: "tool.call.completed",
      eventId,
      globalSequence: "81",
      view: { toolCallId: "tool-no-detail", title: "读取 src/b.ts", state: "completed" },
      acp: {
        mediaType: "application/json",
        rawJson: '{"toolDetail":{"input":{"secret":"raw-only"},"text":["raw-only-text"]}}',
        byteLength: "96",
        sha256: "c".repeat(43),
      },
    });
    const html = renderPage(pageOf(client), {
      expandedTimelineEntries: { [foldKey(eventId)]: true },
    });

    // 反例：从原文补造详情 → 下面三条会红。
    expect(html).not.toContain('data-tool-detail="present"');
    expect(html).not.toContain("raw-only");
    expect(html).not.toContain("因超过上限未下发");
    // 没有详情不妨碍原文入口：它是诊断抽屉，不是详情来源。
    expect(html).toContain('data-action="open-tool-call-diagnostics"');
    expect(tagContaining(html, 'data-action="open-tool-call-diagnostics"')).toContain(`data-event-id="${eventId}"`);
  });
});

describe("对话时间线：从条目联动到右栏", () => {
  const changeId = "11111111-0000-4000-8000-0000000000d1";

  /** 注入一条 `file.changed`（带相对化展示路径）。 */
  function applyFileChanged(client: ClientStore): void {
    applyEvent(client, {
      eventType: "file.changed",
      eventId: "dddddddd-0000-4000-8000-000000000501",
      globalSequence: "70",
      view: {
        changeId,
        kind: "update",
        displayPath: "src/auth/session.ts",
        summary: "替换 1 行",
        addedLines: "2",
        deletedLines: "1",
        outsideWorkspace: false,
      },
    });
  }

  it("带文件标识的条目被选中时，时间线与右栏指向同一处改动", () => {
    const { store: client } = store();
    applyFileChanged(client);
    const html = renderPage(pageOf(client), {
      selectedChangeId: changeId,
      onSelectChange: () => undefined,
      onOpenChangeDiagnostics: () => undefined,
    });

    // 时间线条目与右栏行都标出「同一处改动被选中」。
    expect(attributeOf(tagContaining(html, `data-change-id="${changeId}"`), "data-change-selected")).toBe("true");
    expect(
      attributeOf(tagContaining(html, `data-file-change="${changeId}"`), "data-file-change-selected"),
    ).toBe("true");
    // 两个入口（时间线条目 + 右栏行）都上抛同一个 `changeId`。
    expect(html.match(/data-action="select-file-change"/g) ?? []).toHaveLength(2);
  });

  it("没有选中时两处都不标出选中；回调缺省时不渲染可点入口", () => {
    const { store: client } = store();
    applyFileChanged(client);
    const html = renderPage(pageOf(client));

    expect(attributeOf(tagContaining(html, `data-change-id="${changeId}"`), "data-change-selected")).toBe("false");
    expect(
      attributeOf(tagContaining(html, `data-file-change="${changeId}"`), "data-file-change-selected"),
    ).toBe("false");
    // 反例：回调缺省却仍画出可点路径 → 这里不为 0。
    expect(html.match(/data-action="select-file-change"/g) ?? []).toHaveLength(0);
  });
});

describe("对话时间线：授权与提问的可用边界", () => {
  it("亲历的请求：按请求下发的可选项渲染应答按钮", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-000000000206",
      globalSequence: "36",
      view: {
        interactionId: "perm-hi",
        title: "运行 npm test",
        options: [
          { optionId: "allow-once", label: "允许一次", kind: "allow_once" },
          { optionId: "reject", label: "拒绝", kind: "reject_once" },
        ],
      },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-interaction-availability="answerable"');
    expect(html).toContain('data-answer-option="allow-once"');
    expect(html).toContain('data-answer-option="reject"');
    expect(html).toContain("允许一次");
  });

  it("授权按钮按 kind 分三档底色，未知取值退到中性档（不冒充拒绝）", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-00000000020e",
      globalSequence: "44",
      view: {
        interactionId: "perm-tiers",
        title: "运行 npm test",
        options: [
          { optionId: "once", label: "允许一次", kind: "allow_once" },
          { optionId: "always", label: "始终允许", kind: "allow_always" },
          { optionId: "reject", label: "拒绝", kind: "reject_once" },
          { optionId: "reject-always", label: "始终拒绝", kind: "reject_always" },
          { optionId: "future", label: "未来的选项", kind: "some_future_kind" },
        ],
      },
    });
    const html = renderPage(pageOf(client));

    // 底色就是 RNW 生成的原子 class（`r-backgroundColor-*`）：颜色相同即同一个 class，退化成
    // 同一档就会退化成同一个 class，因此这条断言能拦住「三个按钮一个样」。
    const backgroundClassOf = (optionId: string): string => {
      const match = /r-backgroundColor-[a-z0-9]+/.exec(classOf(tagContaining(html, `data-answer-option="${optionId}"`)));
      expect(match, `${optionId} 的按钮没有背景色 class`).not.toBeNull();
      return match?.[0] ?? "";
    };

    const once = backgroundClassOf("once");
    const always = backgroundClassOf("always");
    const reject = backgroundClassOf("reject");
    // 三档互不相同：accent / warn / danger。三者合成一档时其中一条必然变红。
    expect(once).not.toBe(always);
    expect(always).not.toBe(reject);
    expect(once).not.toBe(reject);
    // 两条拒绝取值合到同一档（原型只有 `.p-reject` 一个拒绝档）。
    expect(backgroundClassOf("reject-always")).toBe(reject);
    // 未知取值确定性降级：不与拒绝档同色，也不冒充允许的两档。
    const future = backgroundClassOf("future");
    expect([once, always, reject]).not.toContain(future);
  });

  it("协商有效交互后实时请求缺版本：不可应答，授权与提问卡片都给出重新取得入口", () => {
    const { store: client } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
      features: [SESSION_HISTORY_FEATURE, PENDING_INTERACTIONS_FEATURE],
    });
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-00000000020f",
      globalSequence: "45",
      view: {
        interactionId: "perm-noversion",
        title: "运行 npm test",
        options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
      },
    });
    applyEvent(client, {
      eventType: "elicitation.requested",
      eventId: "dddddddd-0000-4000-8000-000000000210",
      globalSequence: "46",
      view: { interactionId: "ask-noversion", title: "选一个分支", schema: { type: "object", properties: {} } },
    });

    const html = renderPage(pageOf(client), { onRefreshInteractions: () => undefined });

    expect(html).toContain('data-interaction-unavailable="request_version_unknown"');
    expect(html).toContain('data-refresh-interactions-reason="request_version_unknown"');
    // 两张卡片各有一个刷新入口：版本未知时它是唯一能让卡片变得可应答的动作。
    expect(html.match(/data-action="refresh-interactions"/g) ?? []).toHaveLength(2);
    expect(html).toContain("重新取得当前有效请求");
    // 反例：画出必然被服务端拒绝的应答按钮 → 下面两条变红。
    expect(html).not.toContain("data-answer-option");
    expect(html).not.toContain("data-answer-action");
  });

  it("协商有效交互且实时请求自带版本：直接可应答，不出现版本未知的刷新入口", () => {
    const { store: client } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
      features: [SESSION_HISTORY_FEATURE, PENDING_INTERACTIONS_FEATURE],
    });
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-000000000211",
      globalSequence: "47",
      view: {
        interactionId: "perm-versioned",
        title: "运行 npm test",
        options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
        requestVersion: "7",
      },
    });
    const html = renderPage(pageOf(client), { onRefreshInteractions: () => undefined });

    expect(html).toContain('data-interaction-availability="answerable"');
    expect(html).toContain('data-answer-option="allow-once"');
    expect(html).not.toContain('data-interaction-unavailable="request_version_unknown"');
    expect(html).not.toContain('data-refresh-interactions-reason="request_version_unknown"');
  });

  it("仅从历史读到的待处理项：不可操作、有说明、没有应答按钮", () => {
    const { store: client, gateway } = store();
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        pending: [
          { sessionId: ONLINE_SESSION_ID, interactionId: "perm-old", kind: "permission", state: "pending", schema: {}, createdAt: "2026-10-01T00:00:01.000Z" },
        ],
      }),
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-pending-interactions="list"');
    expect(html).toContain('data-pending-interaction="perm-old"');
    expect(html).toContain('data-pending-interaction-availability="not_answerable"');
    expect(html).toContain('data-interaction-unavailable="read_from_history"');
    expect(html).toContain("本设备没有收到过它的请求内容");
    // 反例：若历史待处理项也画出按钮，下面两条会红（点了必然失败）。
    expect(html).not.toContain("data-answer-option");
    expect(html).not.toContain("data-answer-action");
  });

  it("亲历的请求在时间线上有卡片时，不再进「没有请求内容」的待处理清单（同一标识只呈现一处）", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-00000000020d",
      globalSequence: "43",
      view: { interactionId: "perm-live", title: "运行 npm test", options: [{ optionId: "reject", label: "拒绝", kind: "reject_once" }] },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-timeline-card="permission-requested"');
    expect(html).toContain('data-interaction-id="perm-live"');
    // 反例：清单不排除已有卡片的交互时，这里会出现两处 `perm-live`。
    expect(html).not.toContain('data-pending-interaction="perm-live"');
    expect(html.match(/data-interaction-id="perm-live"/g) ?? []).toHaveLength(1);
    // 应答按钮带上它属于哪条交互（与卡片自己的锚点不同名，因此卡片锚点仍然唯一）。
    expect(html.match(/data-answer-interaction-id="perm-live"/g) ?? []).toHaveLength(1);
  });

  it("已被其它设备处理：标出「不是本设备所为」且没有应答按钮", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-000000000207",
      globalSequence: "37",
      view: { interactionId: "perm-other", title: "运行 npm test", options: [{ optionId: "reject", label: "拒绝", kind: "reject_once" }] },
    });
    applyEvent(client, {
      eventType: "permission.resolved",
      eventId: "dddddddd-0000-4000-8000-000000000208",
      globalSequence: "38",
      view: { interactionId: "perm-other", resolution: "reject", resolvedByDeviceId: "another-device" },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-interaction-availability="handled_elsewhere"');
    expect(html).toContain('data-interaction-handled="other-device"');
    expect(html).toContain('data-resolved-by-device-id="another-device"');
    expect(html).toContain("不是本设备所为");
    expect(html).not.toContain('data-answer-option="reject"');
  });

  it("本设备已提交、尚未收到终态：呈现在待确认，且没有重复提交的按钮", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-000000000209",
      globalSequence: "39",
      view: { interactionId: "perm-mine", title: "运行 npm test", options: [{ optionId: "reject", label: "拒绝", kind: "reject_once" }] },
    });
    client.resolvePermission(ONLINE_SESSION_ID, "perm-mine", "reject");
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-interaction-availability="submitting"');
    expect(html).toContain("等待服务端确认");
    // 反例：待确认时仍画出按钮 → 用户会重复提交。
    expect(html).not.toContain('data-answer-option="reject"');
  });

  it("提问请求：三条动作来自模型，受支持字段渲染成可填写控件", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "elicitation.requested",
      eventId: "dddddddd-0000-4000-8000-00000000020a",
      globalSequence: "40",
      view: { interactionId: "ask-1", title: "选一个分支", schema: { type: "object", properties: { branch: { type: "string" } } } },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-elicitation-form="supported"');
    expect(html).toContain('data-form-field="branch"');
    expect(html).toContain('data-form-input="branch"');
    expect(html).toContain('data-action="submit-elicitation-form"');
    expect(html).toContain('data-action="decline-elicitation"');
    expect(html).toContain('data-action="cancel-elicitation"');
  });
});

describe("对话时间线：轮次分组", () => {
  it("同一 turnId 的连续条目进一个组，未归属轮次的条目独立成组", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "turn.started",
      eventId: "dddddddd-0000-4000-8000-000000000301",
      globalSequence: "51",
      view: { turnId: "turn-9", state: "running" },
    });
    applyEvent(client, {
      eventType: "tool.call.started",
      eventId: "dddddddd-0000-4000-8000-000000000302",
      globalSequence: "52",
      view: { toolCallId: "tool-9", turnId: "turn-9", title: "读取 src/b.ts", state: "running" },
    });
    applyEvent(client, {
      eventType: "session.plan.changed",
      eventId: "dddddddd-0000-4000-8000-000000000303",
      globalSequence: "53",
      view: { entries: [{ content: "补充测试", priority: "medium", status: "pending" }] },
    });
    const html = renderPage(pageOf(client));

    // 组只来自可靠标识：两条 `turn-9` 同组（计数 2），计划没有标识因此单独成组。
    const turnGroup = tagContaining(html, 'data-turn-group-id="turn-9"');
    expect(attributeOf(turnGroup, "data-turn-group-entry-count")).toBe("2");
    expect(html.match(/data-turn-group-id="turn-9"/g) ?? []).toHaveLength(1);
    const unassignedGroup = tagContaining(html, 'data-turn-group-id="unassigned"');
    expect(attributeOf(unassignedGroup, "data-turn-group-entry-count")).toBe("1");
    // 组内保持发生顺序：turn 卡在工具卡之前。
    expect(html.indexOf('data-timeline-card="turn"')).toBeLessThan(
      html.indexOf('data-timeline-card="tool-call"'),
    );
  });
});

describe("对话时间线：降级条目", () => {
  it("客户端没有专用视图的事件：产出降级卡片并保留诊断入口", () => {
    const { store: client } = store();
    applyEvent(client, {
      eventType: "session.info.changed",
      eventId: "dddddddd-0000-4000-8000-00000000020b",
      globalSequence: "41",
      view: { title: "改了标题" },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-timeline-card="degraded"');
    expect(html).toContain('data-degraded-entry="dddddddd-0000-4000-8000-00000000020b"');
    expect(html).toContain('data-degraded-view="no-dedicated-view"');
    expect(html).toContain('data-action="open-diagnostics"');
    expect(html).toContain('data-event-id="dddddddd-0000-4000-8000-00000000020b"');
  });

  it("降级条目与消息同容器（未知事件不得让会话渲染失败）", () => {
    const { store: client, gateway } = store();
    applyEvent(client, {
      eventType: "acp.agent.vendor_status",
      eventId: "dddddddd-0000-4000-8000-00000000020c",
      globalSequence: "42",
      view: { phase: "review" },
    });
    client.loadSession(ONLINE_SESSION_ID);
    client.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        messages: [messageItem({ id: "m-2", createdAt: "2026-10-01T00:00:09.000Z" })],
      }),
    });
    const html = renderPage(pageOf(client));

    expect(tagContaining(html, 'data-message-id="m-2"').startsWith("<li")).toBe(true);
    expect(tagContaining(html, 'data-degraded-entry=').startsWith("<li")).toBe(true);
    // 锚点互不冲突：降级条目用自己的锚点，不占用消息的 `data-message-id`。
    expect(dataAttributes(html).some((attribute) => attribute.startsWith('data-degraded-entry='))).toBe(true);
  });
});

/**
 * 同一卡片跨两次渲染的 hook 顺序。
 *
 * `renderToStaticMarkup` 单次渲染不保留 hook 链，测不到「提前 return 之后才调用 hook」。这里让
 * 外层探针在渲染期间用 `useState` 触发 render-phase update：服务端渲染器会在**同一次渲染内**
 * 复用同一条 hook 链重跑组件，hook 数量一旦变化就抛
 * "Rendered more hooks than during the previous render"——与客户端复用 fiber 时的约束一致。
 */
function rerenderRefreshAction(
  first: InteractionAvailability,
  second: InteractionAvailability,
): string {
  function Harness(): ReactElement {
    const [step, setStep] = useState<0 | 1>(0);
    if (step === 0) setStep(1);
    const availability = step === 0 ? first : second;
    return (
      RefreshInteractionsAction({ availability, onRefreshInteractions: () => undefined }) ?? createElement("span", {
        "data-refresh-action": "absent",
      })
    );
  }
  return renderToStaticMarkup(createElement(Harness));
}

const ANSWERABLE: InteractionAvailability = { kind: "answerable", answers: [] };
const VERSION_UNKNOWN: InteractionAvailability = { kind: "not_answerable", reason: "request_version_unknown" };

describe("刷新交互入口的 hook 顺序（同一卡片重渲染）", () => {
  it("刷新成功：版本未知 → 可应答，同一卡片重渲染不抛错且入口消失", () => {
    const html = rerenderRefreshAction(VERSION_UNKNOWN, ANSWERABLE);

    expect(html).toContain('data-refresh-action="absent"');
    // 变成可应答后入口必须消失；若 hook 顺序被改回提前 return 之后，这里先抛错。
    expect(html).not.toContain('data-action="refresh-interactions"');
  });

  it("可应答 → 版本未知：hook 数量变化由重渲染直接抛错（提前 return 后调用 hook 的反例）", () => {
    const html = rerenderRefreshAction(ANSWERABLE, VERSION_UNKNOWN);

    expect(html).toContain('data-action="refresh-interactions"');
    expect(html).toContain('data-refresh-interactions-reason="request_version_unknown"');
  });
});
