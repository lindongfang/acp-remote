/**
 * 时间线：消息与结构化条目同一条序列、实时与历史同一呈现、同一标识只呈现一处（D1/D2）。
 *
 * ## 判别力
 *
 * - 「同一标识只呈现一处」用**同一 `eventId`** 走两条来源（先实时、后历史读取）来钉：
 *   若实现按来源各存一份，时间线上会出现两张 `file_changed` 卡片 → 断言变红。
 * - 「历史与实时同一呈现」用同一事件类型的两条**不同** `eventId`（一条实时、一条只在历史里）
 *   比对卡片形状：历史条目若走另一套解析，两者会在字段上分叉。
 * - 两套分页用载荷形状与页标记分别断言：消息翻页只带 `before`，历史翻页只带 `beforeHistory`，
 *   且互不改变对方的页边界。
 * - 来源为其它节点的会话必须给出「结构化历史不可用」的说明（spec 场景）。
 */

import { describe, expect, it, vi } from "vitest";

import { SESSION_HISTORY_FEATURE } from "../sync-client";
import { FILE_DIFF_UNAVAILABLE_LABELS } from "../domain";
import type {
  FileChangeView,
  MessageTimelineEntry,
  SentPromptView,
  TimelineEntry,
  VolatileMessageState,
} from "./conversation-timeline";
import {
  buildTimeline,
  formatTimelineTime,
  groupTimelineEntries,
  stabilizeTimelineEntries,
} from "./conversation-timeline";
import type { StoreFixture } from "./testing/fixtures";
import {
  eventMessage,
  historyItem,
  lastDispatch,
  messageItem,
  onlineStore,
  readResult,
  resources,
  session,
} from "./testing/fixtures";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";

/** 一条 `file.changed` 的视图（统计齐备，与 D8 的字段表一致）。 */
function fileChangedView(changeId: string, displayPath: string): Record<string, unknown> {
  return {
    changeId,
    kind: "update",
    displayPath,
    summary: "改了 3 行",
    addedLines: "2",
    deletedLines: "1",
    outsideWorkspace: false,
  };
}

/** 一个已在线、协商到结构化历史、快照里有目标会话的仓库。 */
function storeWithHistoryFeature(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: SESSION_ID, state: "running" })] }),
    features: [SESSION_HISTORY_FEATURE],
  });
}

describe("D1/D2 时间线：同一标识只呈现一处", () => {
  it("同一 eventId 先从实时到达、后出现在历史里：时间线上只有一条", () => {
    const { store, gateway } = storeWithHistoryFeature();
    const sharedEventId = "dddddddd-0000-4000-8000-0000000000a1";

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: sharedEventId,
        sessionId: SESSION_ID,
        globalSequence: "11",
        view: fileChangedView("11111111-0000-4000-8000-0000000000c1", "src/a.ts"),
      }),
    });
    // 首屏读取：服务端把同一条事件也作为历史条目返回（保留期内它确实在历史里）。
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:02.000Z" })],
        history: [
          historyItem({
            eventId: sharedEventId,
            eventType: "file.changed",
            sessionSequence: "7",
            view: fileChangedView("11111111-0000-4000-8000-0000000000c1", "src/a.ts"),
          }),
        ],
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    // 反例：两条来源各留一份时，这里会是 2 → 断言变红。
    expect(entries.filter((entry) => entry.kind === "file_changed")).toHaveLength(1);
    const card = entries.find((entry) => entry.kind === "file_changed");
    expect(card?.kind === "file_changed" ? card.displayPath : null).toBe("src/a.ts");
  });

  it("同一 messageId 先累加增量、后被读取结果覆盖：只呈现一处且以读取结果为准", () => {
    const { store, gateway } = storeWithHistoryFeature();
    const messageId = "cccccccc-0000-4000-8000-0000000000b1";

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.message.delta",
        eventId: "dddddddd-0000-4000-8000-0000000000a2",
        sessionId: SESSION_ID,
        globalSequence: "12",
        view: { messageId, turnId: "turn-1", deltaIndex: "0", text: "我先看到半句" },
      }),
    });
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [
          messageItem({ id: messageId, createdAt: "2026-10-01T00:00:02.000Z", role: "agent", text: "权威正文" }),
        ],
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    const messages = entries.filter((entry): entry is MessageTimelineEntry => entry.kind === "message");
    expect(messages).toHaveLength(1);
    expect(messages[0]?.volatile).toBe(false);
    expect(messages[0]?.message.text).toBe("权威正文");
  });

  it("读取结果之后到达的纯媒体增量：并入该消息、与读取结果按附件/URI 去重", () => {
    const { store, gateway } = storeWithHistoryFeature();
    const messageId = "cccccccc-0000-4000-8000-0000000000b2";
    const attachmentId = "11111111-0000-4000-8000-0000000000c2";
    const laterAttachmentId = "11111111-0000-4000-8000-0000000000c3";
    const resourceUri = "https://example.test/report.pdf";

    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [
          messageItem({
            id: messageId,
            createdAt: "2026-10-01T00:00:02.000Z",
            role: "agent",
            blocks: [
              { type: "text", text: "这是一张图" },
              { type: "image_ref", mimeType: "image/png", byteLength: "128", displayState: "available", attachmentId },
              { type: "resource_ref", uri: resourceUri, name: "报告", displayState: "available" },
            ],
          }),
        ],
      }),
    });

    // 读取屏障之后到达的三条**纯媒体**增量（`text` 为空、只有 `block`）：两条重投读取结果里
    // 已有的块，一条是新的。若第二条循环按 `readIds` 整条跳过，这三块都会消失。
    const deltas = [
      {
        eventId: "dddddddd-0000-4000-8000-0000000000a5",
        deltaIndex: "5",
        block: { type: "image_ref", mimeType: "image/png", byteLength: "128", displayState: "available", attachmentId },
      },
      {
        eventId: "dddddddd-0000-4000-8000-0000000000a6",
        deltaIndex: "6",
        block: { type: "resource_ref", uri: resourceUri, name: "报告", displayState: "available" },
      },
      {
        eventId: "dddddddd-0000-4000-8000-0000000000a7",
        deltaIndex: "7",
        block: {
          type: "image_ref",
          mimeType: "image/jpeg",
          byteLength: "256",
          displayState: "available",
          attachmentId: laterAttachmentId,
        },
      },
    ];
    for (const delta of deltas) {
      store.apply({
        kind: "event",
        event: eventMessage({
          eventType: "agent.message.delta",
          eventId: delta.eventId,
          sessionId: SESSION_ID,
          globalSequence: delta.deltaIndex,
          view: { messageId, turnId: "turn-1", deltaIndex: delta.deltaIndex, text: "", block: delta.block },
        }),
      });
    }

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    const messages = entries.filter((entry): entry is MessageTimelineEntry => entry.kind === "message");
    // 同一 messageId 不能因为易失层里还有一条 tail 而重复呈现。
    expect(messages).toHaveLength(1);
    expect(messages[0]?.message.text).toBe("这是一张图");
    expect(messages[0]?.message.content.media).toHaveLength(3);
    expect(
      messages[0]?.message.content.media.filter(
        (block) => block.kind === "image" && block.attachmentId === attachmentId,
      ),
    ).toHaveLength(1);
    expect(
      messages[0]?.message.content.media.filter(
        (block) => block.kind === "image" && block.attachmentId === laterAttachmentId,
      ),
    ).toHaveLength(1);
    expect(
      messages[0]?.message.content.media.filter((block) => block.kind === "resource" && block.uri === resourceUri),
    ).toHaveLength(1);
    // 媒体可见即结构化内容可见：组件据此渲染媒体区，而不是把纯媒体消息画成空正文。
    expect(messages[0]?.message.hasStructuredContent).toBe(true);
    expect(messages[0]?.message.content.hasStructuredContent).toBe(true);
    // 正文没有残留增量，因此不标记为易失。
    expect(messages[0]?.volatile).toBe(false);
  });
});

describe("D1 时间线：历史条目与实时事件同一套呈现", () => {
  it("同一类型的实时卡片与历史卡片字段形状一致", () => {
    const { store, gateway } = storeWithHistoryFeature();

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000a3",
        sessionId: SESSION_ID,
        globalSequence: "13",
        view: fileChangedView("11111111-0000-4000-8000-0000000000c2", "src/live.ts"),
      }),
    });
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        history: [
          historyItem({
            eventId: "dddddddd-0000-4000-8000-0000000000a4",
            eventType: "file.changed",
            sessionSequence: "3",
            view: fileChangedView("11111111-0000-4000-8000-0000000000c3", "src/old.ts"),
          }),
        ],
      }),
    });

    const cards = (store.conversationPage(SESSION_ID)?.timeline.entries ?? []).filter(
      (entry) => entry.kind === "file_changed",
    );
    // 两条不同事件：一条来自实时、一条只在历史里——两者都必须在，且形状同构。
    expect(cards).toHaveLength(2);
    const [live, fromHistory] = cards;
    if (live?.kind !== "file_changed" || fromHistory?.kind !== "file_changed") {
      throw new Error("卡片类型不是 file_changed");
    }
    expect(Object.keys(live).sort()).toEqual(Object.keys(fromHistory).sort());
    expect(live.addedLines).toBe("2");
    expect(fromHistory.addedLines).toBe("2");
  });

  it("时间线按时间排序：消息与结构化条目交错在同一条序列里", () => {
    const { store, gateway } = storeWithHistoryFeature();

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "terminal.output",
        eventId: "dddddddd-0000-4000-8000-0000000000a5",
        sessionId: SESSION_ID,
        globalSequence: "14",
        view: {
          terminalId: "t-1",
          chunkIndex: "0",
          stream: "stdout",
          text: "构建中\n",
          truncated: false,
        },
      }),
    });
    // 覆盖时间戳：事件在 00:00:02，两条消息分别在它之前与之后。
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [
          messageItem({ id: "m-early", createdAt: "2026-10-01T00:00:01.000Z" }),
          messageItem({ id: "m-late", createdAt: "2026-10-01T00:00:09.000Z" }),
        ],
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    // 反例：把消息与卡片分成两段渲染时，这里会是 [message, message, terminal_output] → 红。
    expect(entries.map((entry) => entry.kind)).toEqual(["message", "terminal_output", "message"]);
  });
});

describe("D9 轮次分组：同 turnId 被无标识条目切开", () => {
  it("思考先到则排在授权前，工具后续完成不把卡片移到授权后", () => {
    const { store } = storeWithHistoryFeature();
    const items = [
      { eventType: "agent.thought.delta", view: { messageId: "order-thought", turnId: "order-turn", deltaIndex: "0", text: "先检查环境" } },
      { eventType: "tool.call.started", view: { toolCallId: "order-tool", turnId: "order-turn", title: "搜索代码", state: "in_progress" } },
      { eventType: "permission.requested", view: { interactionId: "order-permission", turnId: "order-turn", title: "pwd", options: [{ optionId: "once", label: "允许一次", kind: "allow_once" }] } },
      { eventType: "tool.call.completed", view: { toolCallId: "order-tool", turnId: "order-turn", title: "搜索代码", state: "completed" } },
    ];
    items.forEach((item, index) => store.apply({ kind: "event", event: eventMessage({
      ...item, eventId: `dddddddd-0000-4000-8000-${String(901 + index).padStart(12, "0")}`,
      sessionId: SESSION_ID, globalSequence: String(20 + index), createdAt: "2026-10-01T00:00:02.000Z",
    }) }));
    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    expect(entries.map((entry) => entry.kind)).toEqual(["message", "tool_call", "permission_requested"]);
    expect(entries[1]).toMatchObject({ state: "completed" });
  });

  it("permission_requested → permission_resolved → agent.message 的组 key 不重复", () => {
    const { store } = storeWithHistoryFeature();
    const interactionId = "99999999-0000-4000-8000-000000000031";
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.requested",
        eventId: "dddddddd-0000-4000-8000-000000000031",
        sessionId: SESSION_ID,
        globalSequence: "31",
        createdAt: "2026-10-01T00:00:03.000Z",
        view: {
          interactionId,
          turnId: "turn-7",
          title: "允许写入 src/ 吗？",
          description: null,
          options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
        },
      }),
    });
    // 授权结果不携带 turnId：它把同一轮次切成「有标识 / 无标识 / 有标识」三段。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.resolved",
        eventId: "dddddddd-0000-4000-8000-000000000032",
        sessionId: SESSION_ID,
        globalSequence: "32",
        createdAt: "2026-10-01T00:00:04.000Z",
        view: { interactionId, resolution: "allow-once" },
      }),
    });
    // 同一轮次的后续消息（真实增量通道携带 turnId）。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.message.delta",
        eventId: "dddddddd-0000-4000-8000-000000000033",
        sessionId: SESSION_ID,
        globalSequence: "33",
        createdAt: "2026-10-01T00:00:05.000Z",
        view: {
          messageId: "cccccccc-0000-4000-8000-000000000031",
          turnId: "turn-7",
          deltaIndex: "0",
          text: "已经写入。",
        },
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    const groups = groupTimelineEntries(entries);
    // 组键只由轮次标识与条目自身的稳定身份决定：同一 turnId 的连续条目仍是一个组，且组键与
    // 「谁是组内首条」无关。分组只是呈现资料，条目本身平级渲染，因此组键变化不会重挂已有正文。
    const turnGroups = groups.filter((group) => group.turnId === "turn-7");
    expect(turnGroups).toHaveLength(1);
    const keys = groups.map((group) => group.key);
    expect(new Set(keys).size).toBe(keys.length);
    // 结果原位合并，不再切开同轮输出。
    expect(turnGroups[0]?.key).toBe("turn:turn-7");
    expect(entries.filter(entry => entry.kind === "permission_requested")).toHaveLength(1);
    expect(entries.some(entry => entry.kind === "permission_resolved")).toBe(false);
  });
});

describe("D2 两套独立分页", () => {
  it("消息翻页只带 before（复合游标），历史翻页只带 beforeHistory（会话内序号）", () => {
    const { store, gateway } = storeWithHistoryFeature();
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [
          messageItem({ id: "m-2", createdAt: "2026-10-01T00:00:02.000Z" }),
          messageItem({ id: "m-3", createdAt: "2026-10-01T00:00:03.000Z" }),
        ],
        history: [
          historyItem({ eventId: "e-1", eventType: "turn.started", sessionSequence: "4", view: { turnId: "t", state: "running" } }),
          historyItem({ eventId: "e-2", eventType: "turn.completed", sessionSequence: "9", view: { turnId: "t", state: "completed" } }),
        ],
        hasEarlier: true,
        historyHasEarlier: true,
      }),
    });

    store.loadEarlier(SESSION_ID);
    const messagePage = lastDispatch(gateway).body;
    if (messagePage.command !== "session.read") throw new Error("命令不是 session.read");
    expect(messagePage.payload.include).toEqual(["messages"]);
    expect(Object.keys(messagePage.payload).sort()).toEqual(["before", "include", "limit"]);
    expect(messagePage.payload.before?.messageId).toBe("m-2");

    store.apply({ kind: "command_result", result: readResult({ requestId: messagePage.requestId, sessionId: SESSION_ID, hasEarlier: false }) });

    store.loadEarlierHistory(SESSION_ID);
    const historyPage = lastDispatch(gateway).body;
    if (historyPage.command !== "session.read") throw new Error("命令不是 session.read");
    // 反例：历史翻页带上 before（或消息翻页带上 beforeHistory）→ 断言变红。
    expect(historyPage.payload.include).toEqual(["history"]);
    expect(Object.keys(historyPage.payload).sort()).toEqual(["beforeHistory", "include"]);
    expect(historyPage.payload.beforeHistory).toBe("4");
  });

  it("历史页标记只由 historyHasEarlier 改变，消息页标记不受影响", () => {
    const { store, gateway } = storeWithHistoryFeature();
    store.loadSession(SESSION_ID);
    const first = lastDispatch(gateway).body.requestId;
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: first,
        sessionId: SESSION_ID,
        messages: [messageItem({ id: "m-9", createdAt: "2026-10-01T00:00:09.000Z" })],
        history: [historyItem({ eventId: "e-9", eventType: "turn.started", sessionSequence: "9", view: { turnId: "t", state: "running" } })],
        hasEarlier: true,
        historyHasEarlier: true,
      }),
    });

    store.loadEarlierHistory(SESSION_ID);
    const requestId = lastDispatch(gateway).body.requestId;
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId,
        sessionId: SESSION_ID,
        history: [historyItem({ eventId: "e-4", eventType: "turn.started", sessionSequence: "4", view: { turnId: "t", state: "running" } })],
        historyHasEarlier: false,
      }),
    });

    const paging = store.conversationPage(SESSION_ID)?.paging;
    expect(paging?.hasEarlierHistory).toBe(false);
    expect(paging?.nextBeforeHistory).toBeNull();
    // 消息那一页的页边界一字未动（两套游标互不影响）。
    expect(paging?.hasEarlier).toBe(true);
    expect(paging?.nextBefore?.messageId).toBe("m-9");
  });

  it("相邻两页历史重复（同一 eventId 出现两次）时抛错而不是静默拼接", () => {
    const { store, gateway } = storeWithHistoryFeature();
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        history: [historyItem({ eventId: "e-dup", eventType: "turn.started", sessionSequence: "5", view: { turnId: "t", state: "running" } })],
        historyHasEarlier: true,
      }),
    });

    store.loadEarlierHistory(SESSION_ID);
    const requestId = lastDispatch(gateway).body.requestId;
    expect(() =>
      store.apply({
        kind: "command_result",
        result: readResult({
          requestId,
          sessionId: SESSION_ID,
          history: [historyItem({ eventId: "e-dup", eventType: "turn.started", sessionSequence: "5", view: { turnId: "t", state: "running" } })],
          historyHasEarlier: false,
        }),
      }),
    ).toThrow(/重叠/);
  });

  it("未协商到结构化历史 feature 时不带 history include（也不误报「没有历史」）", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: SESSION_ID })] }),
      features: [],
    });
    store.loadSession(SESSION_ID);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.read") throw new Error("命令不是 session.read");
    // 反例：无条件带 history 会得到明确的不支持类错误，而旧服务端连取值都不认识。
    expect(body.payload.include).toEqual(["messages", "config_options"]);

    const notice = store.conversationPage(SESSION_ID)?.timeline.structuredHistory;
    expect(notice?.state).toBe("unavailable");
    expect(notice?.notice).toContain("core.session-history.v1");
  });
});

describe("结构化历史的显式降级", () => {
  it("来源为其它节点的会话给出「不可用」说明，而不是让人以为没有结构化内容", () => {
    const remote = session({
      id: SESSION_ID,
      origin: {
        kind: "remote",
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        exportId: "export-1",
        originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
        online: false,
      },
    });
    const { store } = onlineStore({ resources: resources({ sessions: [remote] }), features: [SESSION_HISTORY_FEATURE] });

    const history = store.conversationPage(SESSION_ID)?.timeline.structuredHistory;
    expect(history?.state).toBe("unavailable");
    expect(history?.notice).toContain("不可达");
    expect(history?.notice).toContain("不代表该会话没有结构化内容");
    // 本节点自有的会话在同一连接下必须可用——否则上一条是平凡通过。
    const local = onlineStore({ resources: resources({ sessions: [session({ id: SESSION_ID })] }), features: [SESSION_HISTORY_FEATURE] });
    expect(local.store.conversationPage(SESSION_ID)?.timeline.structuredHistory).toEqual({ state: "available", notice: null });
  });
});

describe("D8 变更文件：结构化统计与相对化路径 + 诊断入口", () => {
  it("统计与路径来自视图，原文只在诊断入口里给，缺统计时是 null 而不是 0", () => {
    const { store } = storeWithHistoryFeature();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000a6",
        sessionId: SESSION_ID,
        globalSequence: "15",
        view: {
          changeId: "11111111-0000-4000-8000-0000000000c4",
          kind: "update",
          displayPath: "src/inner/file.ts",
          summary: "改了一处",
          outsideWorkspace: false,
        },
        acp: {
          mediaType: "application/json",
          rawJson: '{"diff":"@@ -1 +1 @@"}',
          byteLength: "22",
          sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p",
        },
      }),
    });

    const change = store.conversationPage(SESSION_ID)?.fileChanges[0];
    expect(change?.displayPath).toBe("src/inner/file.ts");
    // 反例：把「判定不出」填成 0 时这里会是 "0" → 断言变红。
    expect(change?.addedLines).toBeNull();
    expect(change?.deletedLines).toBeNull();
    expect(change?.diagnostics.eventId).toBe("dddddddd-0000-4000-8000-0000000000a6");
    expect(change?.diagnostics.raw.kind).toBe("available");
    // 诊断入口拿到的是**原文**，不是被解析出来的差异结构。
    expect(change?.diagnostics.raw.kind === "available" ? change.diagnostics.raw.rawJson : null).toContain("@@ -1 +1 @@");
  });

  it("客户端不自己相对化路径：越界改动只按 core 给的展示名呈现", () => {
    const { store } = storeWithHistoryFeature();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000a7",
        sessionId: SESSION_ID,
        globalSequence: "16",
        view: {
          changeId: "11111111-0000-4000-8000-0000000000c5",
          kind: "create",
          displayPath: "outside.ts",
          summary: "越界写入",
          outsideWorkspace: true,
        },
      }),
    });
    const change = store.conversationPage(SESSION_ID)?.fileChanges[0];
    expect(change?.displayPath).toBe("outside.ts");
    expect(change?.outsideWorkspace).toBe(true);
  });

  it("诊断入口不带差异正文，也不缺元数据（原文未下发时只给原因）", () => {
    const { store } = storeWithHistoryFeature();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000a8",
        sessionId: SESSION_ID,
        globalSequence: "17",
        view: fileChangedView("11111111-0000-4000-8000-0000000000c6", "src/b.ts"),
        acp: {
          mediaType: "application/json",
          rawUnavailable: { reason: "size_limit", byteLength: "281344", sha256: null },
        },
      }),
    });

    const change = store.conversationPage(SESSION_ID)?.fileChanges[0];
    expect(change?.diagnostics.raw).toEqual({ kind: "not_synced", reason: "size_limit", byteLength: "281344", sha256: null });
    // 统计仍来自视图（缺的是原文，不是统计）。
    expect(change?.addedLines).toBe("2");
    expect(change?.diagnostics.view).toMatchObject({ displayPath: "src/b.ts" });
  });
});

describe("D6 右栏差异区的三种形态", () => {
  /** 把一条 `file.changed` 事件喂进仓库，返回右栏里的那一处改动。 */
  function changeOf(store: StoreFixture["store"], view: Record<string, unknown>, sequence: string): FileChangeView {
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: `dddddddd-0000-4000-8000-0000000000${sequence}`,
        sessionId: SESSION_ID,
        globalSequence: sequence,
        view,
      }),
    });
    const change = store.conversationPage(SESSION_ID)?.fileChanges.at(-1);
    if (change === undefined) throw new Error("右栏应有这一处改动");
    return change;
  }

  it("随事件下发差异行时逐行呈现，并区分新增 / 删除 / 上下文", () => {
    const { store } = storeWithHistoryFeature();
    const change = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d1", "src/a.ts"),
        diffLines: [
          { kind: "context", text: "  const opts = resolveOptions(input);" },
          { kind: "deleted", text: "- const idleTimeout = 30_000;" },
          { kind: "added", text: "+ const idleTimeout = opts.idleTimeoutMs ?? 10_000;" },
        ],
      },
      "21",
    );

    expect(change.diff?.kind).toBe("lines");
    if (change.diff?.kind !== "lines") throw new Error("期望 lines");
    // 反例：把三档合并成一种颜色/一种标记时，这里会全等 → 断言变红。
    expect(change.diff.lines.map((line) => line.kind)).toEqual(["context", "deleted", "added"]);
    expect(change.diff.lines.map((line) => line.text)).toEqual([
      "  const opts = resolveOptions(input);",
      "- const idleTimeout = 30_000;",
      "+ const idleTimeout = opts.idleTimeoutMs ?? 10_000;",
    ]);
  });

  it("差异行内容按不可信文本逐字呈现（不被解析、不被改写）", () => {
    const { store } = storeWithHistoryFeature();
    const odd = "<script>alert(1)</script> \\u0000 @@ -1 +1 @@";
    const change = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d2", "src/a.ts"),
        diffLines: [{ kind: "context", text: odd }],
      },
      "22",
    );

    expect(change.diff?.kind === "lines" ? change.diff.lines[0]?.text : null).toBe(odd);
  });

  it("有不可用标记时给出原因，且**没有**任何差异行可渲染", () => {
    const { store } = storeWithHistoryFeature();
    const change = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d3", "src/a.ts"),
        diffUnavailable: { reason: "size_limit" },
      },
      "23",
    );

    expect(change.diff).toEqual({ kind: "unavailable", reason: "size_limit" });
    // 文案表跟着取值走（词表封闭）：未知取值不会有文案。
    expect(FILE_DIFF_UNAVAILABLE_LABELS.size_limit.length).toBeGreaterThan(0);
  });

  it("两者都没有时保持「只有统计」的形态（`null`，不冒充成任何一种）", () => {
    const { store } = storeWithHistoryFeature();
    const change = changeOf(
      store,
      fileChangedView("11111111-0000-4000-8000-0000000000d4", "src/a.ts"),
      "24",
    );

    expect(change.diff).toBeNull();
    // 对照：统计照常在，因此这一条不是「整条改动都没了」。
    expect(change.addedLines).toBe("2");
    expect(change.displayPath).toBe("src/a.ts");
  });

  it("不解析 ACP 原文补差异：只有 `rawJson` 带 diff 文本时差异区仍是「两者都没有」", () => {
    const { store } = storeWithHistoryFeature();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000d5",
        sessionId: SESSION_ID,
        globalSequence: "25",
        view: fileChangedView("11111111-0000-4000-8000-0000000000d5", "src/a.ts"),
        acp: {
          mediaType: "application/json",
          rawJson: '{"diff":[{"kind":"added","text":"+ 不该被解析出来"}]}',
          byteLength: "48",
          sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p",
        },
      }),
    });

    const change = store.conversationPage(SESSION_ID)?.fileChanges[0];
    // 反例：从 `rawJson` 里解析出差异行时，这里会是 lines → 断言变红。
    expect(change?.diff).toBeNull();
    expect(change?.diagnostics.raw.kind).toBe("available");
  });

  it("形状不合规的差异行不冒充成任何一种形态（不当上下文行吞下、也不假装完整）", () => {
    const { store } = storeWithHistoryFeature();
    const empty = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d6", "src/a.ts"),
        diffLines: [],
      },
      "26",
    );
    expect(empty.diff).toBeNull();

    const unknownKind = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d7", "src/b.ts"),
        diffLines: [{ kind: "moved", text: "?" }],
      },
      "27",
    );
    expect(unknownKind.diff).toBeNull();

    const unknownReason = changeOf(
      store,
      {
        ...fileChangedView("11111111-0000-4000-8000-0000000000d8", "src/c.ts"),
        diffUnavailable: { reason: "too_many" },
      },
      "28",
    );
    expect(unknownReason.diff).toBeNull();
  });
});

describe("WP2 工具调用：跨事件详情归并与诊断入口", () => {
  it("started 与 updated 合并成一张卡，输入与输出都保留，eventId 透传最新记录", () => {
    const { store } = storeWithHistoryFeature();
    const startedEventId = "dddddddd-0000-4000-8000-0000000000e1";
    const updatedEventId = "dddddddd-0000-4000-8000-0000000000e2";

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "tool.call.started",
        eventId: startedEventId,
        sessionId: SESSION_ID,
        globalSequence: "21",
        view: {
          toolCallId: "tc-1",
          turnId: "11111111-0000-4000-8000-000000000001",
          title: "读取文件",
          state: "running",
          toolDetail: { input: { path: "src/a.ts" } },
        },
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "tool.call.updated",
        eventId: updatedEventId,
        sessionId: SESSION_ID,
        globalSequence: "22",
        view: {
          toolCallId: "tc-1",
          turnId: "11111111-0000-4000-8000-000000000001",
          title: "读取文件",
          state: "completed",
          toolDetail: { output: { bytes: 3 } },
        },
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    const cards = entries.filter((entry) => entry.kind === "tool_call");
    // 反例：按记录逐条出卡时这里是 2 → 断言变红。
    expect(cards).toHaveLength(1);
    const card = cards[0];
    if (card?.kind !== "tool_call") throw new Error("条目不是工具调用卡");
    expect(card.state).toBe("completed");
    expect(card.eventId).toBe(updatedEventId);
    expect(card.detail).toEqual({
      input: { present: true, value: { path: "src/a.ts" } },
      output: { present: true, value: { bytes: 3 } },
      text: null,
      truncatedParts: [],
    });
  });

  it("同一 eventId 的 tool_call 实时与历史只呈现一张，eventId 与详情原样透传", () => {
    const { store, gateway } = storeWithHistoryFeature();
    const sharedEventId = "dddddddd-0000-4000-8000-0000000000e3";
    const view = {
      toolCallId: "tc-2",
      turnId: "11111111-0000-4000-8000-000000000002",
      title: "执行命令",
      state: "completed",
      toolDetail: { output: { ok: true }, text: ["完成"] },
    };

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "tool.call.completed",
        eventId: sharedEventId,
        sessionId: SESSION_ID,
        globalSequence: "23",
        view,
      }),
    });
    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        history: [
          historyItem({
            eventId: sharedEventId,
            eventType: "tool.call.completed",
            sessionSequence: "9",
            view,
          }),
        ],
      }),
    });

    const entries = store.conversationPage(SESSION_ID)?.timeline.entries ?? [];
    const cards = entries.filter((entry) => entry.kind === "tool_call");
    expect(cards).toHaveLength(1);
    const card = cards[0];
    if (card?.kind !== "tool_call") throw new Error("条目不是工具调用卡");
    expect(card.eventId).toBe(sharedEventId);
    expect(card.detail?.output).toEqual({ present: true, value: { ok: true } });
    expect(card.detail?.text).toEqual(["完成"]);
  });
});

/** 一条本设备刚提交的输入（身份字段由用例给出）。 */
function sentPromptView(overrides: Partial<SentPromptView> = {}): SentPromptView {
  return {
    requestId: "req-1",
    text: "继续",
    attachmentNames: [],
    createdAt: "2026-10-01T00:00:01.000Z",
    phase: "submitted",
    turnId: null,
    messageId: null,
    notice: null,
    ...overrides,
  };
}

/** 一条权威消息视图（`session.read` 的形状）：默认是本设备用户的正文，可指定序号/通道/时刻。 */
function messageView(input: {
  readonly id: string;
  readonly turnId: string | null;
  readonly sequence?: string;
  readonly role?: "user" | "agent";
  readonly channel?: "reply" | "thought";
  readonly text?: string;
  readonly createdAt?: string;
}): MessageTimelineEntry["message"] {
  const text = input.text ?? "继续";
  return {
    id: input.id,
    role: input.role ?? "user",
    channel: input.channel ?? "reply",
    text,
    content: { text, media: [], hasStructuredContent: false },
    hasStructuredContent: false,
    partial: null,
    ...(input.sequence === undefined ? {} : { sequence: input.sequence }),
    createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
    turnId: input.turnId,
  };
}

/** 一条来自实时增量的 Agent 正文（`turnId` 由增量事件给出）。 */
function volatileAgentMessage(turnId: string): VolatileMessageState {
  return {
    messageId: "agent-1",
    sessionId: null,
    channel: "agent",
    turnId,
    createdAt: "2026-10-01T00:00:04.000Z",
    deltas: new Map([["0", "部署脚本检查完成"]]),
    completed: false,
    mediaBlocks: [],
    partial: null,
  };
}

describe("D2 稳定内容身份：轮次资料补齐与正文接替都不换节点", () => {
  it("本地回显拿到 turnId：条目键不变，变化的只是可更新的轮次资料", () => {
    const before = buildTimeline({
      messages: [], records: [], volatile: [], interactions: [],
      sentPrompts: [sentPromptView()],
    });
    const after = buildTimeline({
      messages: [], records: [], volatile: [], interactions: [],
      sentPrompts: [sentPromptView({ phase: "accepted", turnId: "turn-1" })],
    });

    // 节点身份只由条目键决定；接替/补齐前后必须是同一个键。
    expect(before.map((entry) => entry.key)).toEqual(after.map((entry) => entry.key));
    expect(before[0]?.key).toBe("sent:req-1");
    // 分组只是呈现资料：轮次补齐会改「属于哪一段」，但不会让条目换父级（视图里条目是平级的）。
    const groupsBefore = groupTimelineEntries(before);
    const groupsAfter = groupTimelineEntries(after);
    expect(groupsBefore[0]?.turnId).toBe(null);
    expect(groupsAfter[0]?.turnId).toBe("turn-1");
    expect(groupsAfter[0]?.entries.map((entry) => entry.key)).toEqual(["sent:req-1"]);
  });

  it("正式用户消息按已确认锚点沿用本地稳定键；没有锚点时仍是自己的 message 身份", () => {
    const message = messageView({ id: "m-1", turnId: "turn-1", sequence: "11" });
    const anchored = buildTimeline({
      messages: [message], records: [], volatile: [], interactions: [],
      promptAnchors: new Map([["m-1", "req-1"]]),
    });
    // 反例（旧实现）：这里会是 `message:m-1`，接替时 `sent:req-1` 节点被移除再新建。
    expect(anchored[0]?.key).toBe("sent:req-1");
    const entry = anchored[0];
    if (entry?.kind !== "message") throw new Error("条目不是消息");
    expect(entry.message.id).toBe("m-1");
    expect(entry.localAnchor).toBe("req-1");
    expect(entry.local).toBe(null);

    // 没有可靠关联时不猜：同一个 messageId 不会被套上别人的本地身份。
    const plain = buildTimeline({ messages: [message], records: [], volatile: [], interactions: [] });
    expect(plain[0]?.key).toBe("message:m-1");
    expect(plain[0]?.kind === "message" ? plain[0].localAnchor : "missing").toBe(null);
  });

  it("被接替的正式消息与同轮 Agent 输出按权威序号相邻，条目键不变", () => {
    const entries = buildTimeline({
      messages: [
        messageView({ id: "m-1", turnId: "turn-1", sequence: "11" }),
        messageView({
          id: "m-2", turnId: "turn-1", sequence: "12", role: "agent", text: "收到",
          createdAt: "2026-10-01T00:00:03.000Z",
        }),
      ],
      records: [], volatile: [], interactions: [],
      promptAnchors: new Map([["m-1", "req-1"]]),
    });
    // 顺序只来自权威序号：本设备的正文是 11，Agent 输出是 12。
    expect(entries.map((entry) => entry.key)).toEqual(["sent:req-1", "message:m-2"]);
    // 同一轮次的连续条目进同一段呈现资料；分组不决定渲染父级（条目在视图里是平级的）。
    const groups = groupTimelineEntries(entries);
    expect(groups.map((group) => group.key)).toEqual(["turn:turn-1"]);
    expect(groups[0]?.entries.map((entry) => entry.key)).toEqual(["sent:req-1", "message:m-2"]);
  });

  it("正式条目序号 10/11/12：带已确认锚点的用户消息不得被前置到思考之前（turnId 有无都一样）", () => {
    // 反例（旧实现）：带 `localAnchor` 的**正式**消息被从权威排序里剔除，再按 `turnId` 插到整轮
    // 第一条之前 → 思考 10 被挤到用户消息之后，两条权威记录的相对顺序被改写。
    for (const turnId of ["turn-1", null]) {
      const entries = buildTimeline({
        messages: [
          messageView({
            id: "m-thought", turnId: "turn-1", sequence: "10", role: "agent", channel: "thought",
            text: "先想一下", createdAt: "2026-10-01T00:00:01.000Z",
          }),
          messageView({ id: "m-user", turnId, sequence: "11" }),
          messageView({
            id: "m-reply", turnId: "turn-1", sequence: "12", role: "agent", text: "做完了",
            createdAt: "2026-10-01T00:00:03.000Z",
          }),
        ],
        records: [], volatile: [], interactions: [],
        promptAnchors: new Map([["m-user", "req-1"]]),
      });
      expect(entries.map((entry) => entry.key))
        .toEqual(["message:m-thought", "sent:req-1", "message:m-reply"]);
      const user = entries[1];
      if (user?.kind !== "message") throw new Error("序号 11 的条目不是消息");
      expect(user.message.id).toBe("m-user");
      expect(user.localAnchor).toBe("req-1");
      expect(user.sequence).toBe("11");
      expect(user.local).toBe(null);
      // 分组只是呈现资料：缺 `turnId` 时那条用户消息自成一段，但顺序一个位置都不动。
      expect(groupTimelineEntries(entries).flatMap((group) => group.entries.map((entry) => entry.key)))
        .toEqual(entries.map((entry) => entry.key));
    }

    // 没有序号时按创建时间稳定排序：接替来的正式消息留在它自己的时间位置，不因为带 `turnId`
    // 被前移到整轮第一条之前。
    const timed = buildTimeline({
      messages: [
        messageView({
          id: "m-a", turnId: "turn-1", role: "agent", text: "第一段",
          createdAt: "2026-10-01T00:00:01.000Z",
        }),
        messageView({ id: "m-1", turnId: "turn-1", createdAt: "2026-10-01T00:00:02.000Z" }),
        messageView({
          id: "m-b", turnId: "turn-1", role: "agent", text: "第二段",
          createdAt: "2026-10-01T00:00:03.000Z",
        }),
      ],
      records: [], volatile: [], interactions: [],
      promptAnchors: new Map([["m-1", "req-1"]]),
    });
    // 反例（旧实现）：这里会被前插成 `sent:req-1, message:m-a, message:m-b`。
    expect(timed.map((entry) => entry.key)).toEqual(["message:m-a", "sent:req-1", "message:m-b"]);
  });

  it("回显尚无权威位置时只占临时位置：取得可靠轮次关联后前插，正式条目相对顺序不变", () => {
    const agentOutput = volatileAgentMessage("turn-1");
    // 提交已发出（尚无 `turnId`），且发起时刻晚于已有输出：临时回显排在这里（末尾）。
    const before = buildTimeline({
      messages: [], records: [], interactions: [], volatile: [agentOutput],
      sentPrompts: [sentPromptView({ createdAt: "2026-10-01T00:00:05.000Z" })],
    });
    expect(before.map((entry) => entry.key)).toEqual(["message:agent-1", "sent:req-1"]);
    const groupsBefore = groupTimelineEntries(before);
    expect(groupsBefore.map((group) => group.key)).toEqual(["turn:turn-1", "group:sent:req-1"]);

    // 受理应答后到：回显仍是**没有权威位置**的临时条目，因此可以移到该轮输出之前——这条移动
    // 不改变任何两条正式条目的相对顺序（Agent 正文只有它自己一条正式记录）。
    const after = buildTimeline({
      messages: [], records: [], interactions: [], volatile: [agentOutput],
      sentPrompts: [sentPromptView({ phase: "accepted", turnId: "turn-1" })],
    });
    expect(after.map((entry) => entry.key)).toEqual(["sent:req-1", "message:agent-1"]);
    expect([...after.map((entry) => entry.key)].sort()).toEqual([...before.map((entry) => entry.key)].sort());
    // Agent 正文的条目键在补齐前后都不变：位置变化只是键控重排，不换节点。
    expect(before[0]?.key).toBe("message:agent-1");
    expect(after[1]?.key).toBe("message:agent-1");
    // 分组只是资料：补齐后回显与同轮输出连续，因此归入同一段。
    const groupsAfter = groupTimelineEntries(after);
    expect(groupsAfter.map((group) => group.key)).toEqual(["turn:turn-1"]);
    expect(groupsAfter[0]?.entries.map((entry) => entry.key)).toEqual(["sent:req-1", "message:agent-1"]);
  });

  it("没有轮次关联的临时回显仍落在创建时刻之后：无 turnId 的流式回复不会排到输入之前", () => {
    // 提交回显（00:00:01）尚无 `turnId`，随后到达的流式回复（00:00:04）也没有 `turnId`：
    // 临时占位按创建时刻落位，回复在输入之后——过程模型据此才看得到「已回复」，不重新显示等待。
    const entries = buildTimeline({
      messages: [], records: [], interactions: [],
      volatile: [{ ...volatileAgentMessage("turn-1"), turnId: null }],
      sentPrompts: [sentPromptView()],
    });
    expect(entries.map((entry) => entry.key)).toEqual(["sent:req-1", "message:agent-1"]);

    // 回退只决定临时位置：夹在两条正式条目之间也不会改变它们的相对顺序。
    const between = buildTimeline({
      messages: [
        messageView({
          id: "m-a", turnId: null, sequence: "10", role: "agent", channel: "thought",
          text: "先想一下", createdAt: "2026-10-01T00:00:00.500Z",
        }),
        messageView({
          id: "m-b", turnId: null, sequence: "12", role: "agent", text: "做完了",
          createdAt: "2026-10-01T00:00:03.000Z",
        }),
      ],
      records: [], interactions: [], volatile: [],
      sentPrompts: [sentPromptView({ createdAt: "2026-10-01T00:00:02.000Z" })],
    });
    expect(between.map((entry) => entry.key))
      .toEqual(["message:m-a", "sent:req-1", "message:m-b"]);
  });
});

describe("D1 条目复用：内建值按内容比较，不把不同内容判成没变", () => {
  /** 一条带任意附加字段的消息条目（把内建值塞进结构比较的路径里）。 */
  const entryWith = (extra: Record<string, unknown>): TimelineEntry => ({
    kind: "message",
    key: "message:m-1",
    createdAt: "2026-10-01T00:00:01.000Z",
    sequence: null,
    channel: "agent",
    message: {
      id: "m-1",
      role: "agent",
      channel: "reply",
      text: "收到",
      content: { text: "收到", media: [], hasStructuredContent: false },
      hasStructuredContent: false,
      partial: null,
      createdAt: "2026-10-01T00:00:01.000Z",
      turnId: "turn-1",
      ...extra,
    } as MessageTimelineEntry["message"],
    volatile: false,
    streamComplete: true,
    partial: null,
  });

  it("Date 的时刻不同必须算作变化，相同时刻仍复用旧对象", () => {
    const at = (iso: string): TimelineEntry => entryWith({ createdAt: new Date(iso) });
    const before = at("2026-10-01T00:00:01.000Z");
    // 旧实现按自有字段比较：`Date` 没有自有字段，两个不同时刻会被判成相等。
    expect(stabilizeTimelineEntries([before], [at("2026-10-01T00:00:02.000Z")])[0]).not.toBe(before);
    expect(stabilizeTimelineEntries([before], [at("2026-10-01T00:00:01.000Z")])[0]).toBe(before);
  });

  it("Map 与 Set 的内容不同必须算作变化，内容相同仍复用旧对象", () => {
    const mapBefore = entryWith({ attachments: new Map([["k", 1]]) });
    // 旧实现按自有字段比较：`Map` 的条目不在自有字段里，两份不同内容会被判成相等。
    expect(stabilizeTimelineEntries([mapBefore], [entryWith({ attachments: new Map([["k", 2]]) })])[0])
      .not.toBe(mapBefore);
    expect(stabilizeTimelineEntries([mapBefore], [entryWith({ attachments: new Map([["k", 1]]) })])[0])
      .toBe(mapBefore);

    const setBefore = entryWith({ tags: new Set(["a"]) });
    expect(stabilizeTimelineEntries([setBefore], [entryWith({ tags: new Set(["b"]) })])[0]).not.toBe(setBefore);
    expect(stabilizeTimelineEntries([setBefore], [entryWith({ tags: new Set(["a"]) })])[0]).toBe(setBefore);
  });
});

describe("D1 时间格式化：共享实例并复用同一时刻的结果", () => {
  it("同一时刻重复显示不新建格式化器；非法时刻固定为占位符", () => {
    const spy = vi.spyOn(Intl, "DateTimeFormat");
    try {
      // 先预热一次（格式化器或结果缓存可能已在别的用例里建立）。
      formatTimelineTime("2026-10-01T00:00:02.000Z");
      const constructedBefore = spy.mock.calls.length;
      const first = formatTimelineTime("2026-10-01T00:00:02.000Z");
      const second = formatTimelineTime("2026-10-01T00:00:02.000Z");
      // 反例（旧实现）：每次调用都 `new Intl.DateTimeFormat(...)`，这里会多 2 次。
      expect(spy.mock.calls.length - constructedBefore).toBe(0);
      expect(second).toBe(first);
      expect(first).toMatch(/^\d{2}:\d{2}$/);
      expect(formatTimelineTime("不是时间")).toBe("—");
      expect(formatTimelineTime("不是时间")).toBe("—");
    } finally {
      spy.mockRestore();
    }
  });
});
