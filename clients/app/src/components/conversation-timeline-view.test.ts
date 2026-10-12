/**
 * WP5 · 对话时间线的呈现（spec「对话页按时间线呈现消息与结构化条目」「Agent 产生的富文本按
 * 不可信输入渲染」；design D1/D5/D6/D8）。
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
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream } from "./index";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import { SESSION_HISTORY_FEATURE } from "../sync-client";
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

/** 渲染对话页正文。 */
function renderPage(model: ConversationPageModel): string {
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model,
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
      onAnswerInteraction: () => undefined,
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
    expect(html.indexOf('data-message-id="m-1"')).toBeLessThan(html.indexOf("</ul>"));
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

  it("提问请求：三条动作来自模型，表单结构按不可信文本原样呈现", () => {
    const { store: client } = interactionStore();
    applyEvent(client, {
      eventType: "elicitation.requested",
      eventId: "dddddddd-0000-4000-8000-00000000020a",
      globalSequence: "40",
      view: { interactionId: "ask-1", title: "选一个分支", schema: { type: "object", properties: { branch: { type: "string" } } } },
    });
    const html = renderPage(pageOf(client));

    expect(html).toContain('data-answer-action="submit"');
    expect(html).toContain('data-answer-action="decline"');
    expect(html).toContain('data-answer-action="cancel"');
    expect(html).toContain('data-elicitation-schema="true"');
    expect(html).toContain('data-elicitation-values="not-collected"');
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
