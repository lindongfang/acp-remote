/**
 * 输入与提交、取消、模型/模式切换，以及 `session.origin.online_changed` 的归并口径
 * （spec「输入与提交只支持文本且以终态为准」「取消当前轮次以终态为准」「模型与模式的呈现与切换」；
 * design D5 的顺带口径）。
 *
 * ## 判别力
 *
 * - **提交不等于完成**：提交后必须是 `pending_confirmation`；`accepted`/`completed` 只能由
 *   `command.result` 的终态产生。若实现本地乐观置态，断言会看到 `accepted` → 变红。
 * - **取消不等于空闲**：取消后是 `requested`，只有终态才变 `confirmed`；失败后回到
 *   `available` 并带上失败说明（不是假装成功）。
 * - **来源离线的草稿不落盘**：imported 会话不接受提交、草稿标注只在内存。
 * - **模型/模式**：未提供可选项时呈现 `unavailable` 且不展示可点选项；切换中为 `switching`，
 *   确认后标注「下一轮生效」。当前模式与提交用的 `expectedVersion` 以最新的
 *   `session.mode.changed` 为准（回落清单、再回落摘要）——只读清单会与时间线上的同一事件矛盾，
 *   并让版本停在旧值上。
 * - **online_changed 只改标注**：逐字段比对摘要其余字段（同一个值），并且不可达时正好出现
 *   离线标注、可达时消失。
 */

import { describe, expect, it } from "vitest";

import type { CommandResultMessage, SessionSummary } from "../protocol";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  interactionStore,
  lastDispatch,
  messageItem,
  onlineStore,
  readResult,
  resources,
  session,
  storeWithNoConnection,
} from "./testing/fixtures";

/** 一条 `turn.started`（让会话有进行中的轮次，取消才有对象）。 */
function turnStarted(eventId: string, globalSequence: string, turnId = "turn-1"): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: "turn.started",
      eventId,
      sessionId: ONLINE_SESSION_ID,
      globalSequence,
      view: { turnId, state: "running" },
    }),
  };
}

/** 一条 `session.config.changed`：带一个 `category=model` 的配置项。 */
function configChanged(eventId: string, globalSequence: string, version: string): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: "session.config.changed",
      eventId,
      sessionId: ONLINE_SESSION_ID,
      globalSequence,
      view: {
        configOptions: [
          {
            id: "model",
            name: "模型",
            description: null,
            category: "model",
            type: "select",
            currentValue: "fast",
            options: [
              { value: "fast", name: "快", description: null },
              { value: "smart", name: "聪明", description: null },
            ],
          },
        ],
        version,
      },
    }),
  };
}

/** 一条 `session.prompt` 的命令结果（受理给出 `{turnId}`，失败/不确定只给错误）。 */
function promptResult(input: {
  readonly requestId: string;
  readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
  readonly turnId?: string | null;
}): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-0000000007aa",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.prompt",
      status: input.status,
      acceptedAt: input.status === "rejected" ? null : "2026-10-01T00:00:02.000Z",
      terminalEventId: null,
      result:
        input.status === "accepted" && input.turnId != null ? { turnId: input.turnId } : null,
      error:
        input.status === "failed" || input.status === "rejected" || input.status === "uncertain"
          ? { code: "session.busy", message: "执行失败", retryable: false, details: {} }
          : null,
    },
  };
}

/** 一条 `user.message.delta`（权威用户消息的实时投影）。 */
function userDelta(input: {
  readonly messageId: string;
  readonly turnId: string | null;
  readonly text: string;
  readonly eventId?: string;
}): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: "user.message.delta",
      eventId: input.eventId ?? "dddddddd-0000-4000-8000-000000000701",
      sessionId: ONLINE_SESSION_ID,
      view: {
        messageId: input.messageId,
        ...(input.turnId === null ? {} : { turnId: input.turnId }),
        deltaIndex: "0",
        text: input.text,
      },
    }),
  };
}

describe("用户消息在时间线上立即可见", () => {
  it("提交后、服务端回显/读取之前就呈现自己发的正文", () => {
    const { store } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "这条消息现在就要出现");
    expect(outcome.ok).toBe(true);

    const page = store.conversationPage(ONLINE_SESSION_ID);
    const userMessages = page?.timeline.entries.filter(
      (entry) => entry.kind === "message" && entry.channel === "user",
    ) ?? [];
    expect(userMessages).toHaveLength(1);
    expect(userMessages[0]?.kind === "message" ? userMessages[0].message.text : null).toBe("这条消息现在就要出现");
  });

  it("权威回显带同一 turnId 时原位接替本地条目（只呈现一处）", () => {
    const { store } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "只出现一次");
    if (!outcome.ok) throw new Error("提交应被接受");
    store.apply({ kind: "command_result", result: promptResult({ requestId: outcome.requestId, status: "accepted", turnId: "turn-1" }) });
    store.apply(userDelta({ messageId: "cccccccc-0000-4000-8000-000000000701", turnId: "turn-1", text: "只出现一次" }));

    const page = store.conversationPage(ONLINE_SESSION_ID);
    const matching = (page?.timeline.entries ?? []).filter(
      (entry) => entry.kind === "message" && entry.channel === "user" &&
        entry.message.text === "只出现一次",
    );
    expect(matching).toHaveLength(1);
    // 留下的必须是正式消息，而不是本地的 `sent:` 条目。
    expect(matching[0]?.key).toBe("message:cccccccc-0000-4000-8000-000000000701");
  });

  it("旧“继续”不抵消新“继续”：文字相同也各自保留，直到它们各自的身份到位", () => {
    const { store, gateway } = interactionStore();
    // 历史里已有一条“继续”（权威消息，turnId 不同）。
    store.loadSession(ONLINE_SESSION_ID);
    const readId = lastDispatch(gateway).body.requestId;
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: readId,
        messages: [messageItem({ id: "m-old", createdAt: "2026-10-01T00:00:01.000Z", text: "继续" })],
      }),
    });

    const first = store.submitPrompt(ONLINE_SESSION_ID, "继续");
    if (!first.ok) throw new Error("提交应被接受");
    store.apply({ kind: "command_result", result: promptResult({ requestId: first.requestId, status: "accepted", turnId: "turn-new" }) });

    const userTexts = (store.conversationPage(ONLINE_SESSION_ID)?.timeline.entries ?? [])
      .filter((entry) => entry.kind === "message" && entry.channel === "user")
      .map((entry) => (entry.kind === "message" ? entry.message.text : ""));
    // 历史“继续” + 本次“继续”都必须可见：按文字计数抵消会只剩 1 条。
    expect(userTexts.filter((text) => text === "继续")).toHaveLength(2);

    // 本次的正式消息按同一 turnId 接替后，历史那条仍然在。
    store.apply(userDelta({ messageId: "cccccccc-0000-4000-8000-000000000702", turnId: "turn-new", text: "继续" }));
    const after = (store.conversationPage(ONLINE_SESSION_ID)?.timeline.entries ?? [])
      .filter((entry) => entry.kind === "message" && entry.channel === "user");
    expect(after).toHaveLength(2);
    expect(after.map((entry) => entry.key)).toEqual([
      "message:m-old",
      "message:cccccccc-0000-4000-8000-000000000702",
    ]);
  });

  it("没有身份关联的相同文字不被当作重复：正式消息先到也不吞掉本地输入", () => {
    const { store } = interactionStore();
    store.submitPrompt(ONLINE_SESSION_ID, "同样的字");
    // 回显带的是另一个轮次，且本地提交还没拿到 turnId：不能凭文字判定为同一条。
    store.apply(userDelta({ messageId: "cccccccc-0000-4000-8000-000000000703", turnId: "turn-other", text: "同样的字" }));
    const userEntries = (store.conversationPage(ONLINE_SESSION_ID)?.timeline.entries ?? [])
      .filter((entry) => entry.kind === "message" && entry.channel === "user");
    expect(userEntries).toHaveLength(2);
    expect(userEntries.some((entry) => entry.key.startsWith("sent:"))).toBe(true);
  });

  it("明确被拒绝的提交不再冒充已发送", () => {
    const { store, gateway } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "不会发出去");
    if (!outcome.ok) throw new Error("提交应被接受");
    const requestId = outcome.requestId;
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000703",
        connectionId: lastDispatch(gateway).connectionId,
        connectionSequence: "1",
        body: {
          requestId,
          command: "session.prompt",
          status: "rejected",
          acceptedAt: null,
          terminalEventId: "dddddddd-0000-4000-8000-000000000703",
          result: null,
          error: { code: "session.not_found", message: "会话不存在", retryable: false, details: {} },
        },
      },
    });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect((page?.timeline.entries ?? []).some(
      (entry) => entry.kind === "message" && entry.channel === "user" && entry.message.text === "不会发出去",
    )).toBe(false);
    // 从未受理的输入回到草稿区，用户可以修正后再发（不是凭空消失）。
    expect(page?.input.draft).toBe("不会发出去");
  });

  it("受理后执行失败：已显示的输入保留，且不会把草稿回填成可重发的新消息", () => {
    const { store } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "已经保存过的输入");
    if (!outcome.ok) throw new Error("提交应被接受");
    store.apply({ kind: "command_result", result: promptResult({ requestId: outcome.requestId, status: "accepted", turnId: "turn-1" }) });
    store.apply({ kind: "command_result", result: promptResult({ requestId: outcome.requestId, status: "failed" }) });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    const entry = (page?.timeline.entries ?? []).find(
      (item) => item.kind === "message" && item.channel === "user" && item.message.text === "已经保存过的输入",
    );
    expect(entry).toBeDefined();
    expect(entry?.kind === "message" ? entry.local?.phase : null).toBe("accepted");
    // 受理后失败不把正文退回输入框：已保存的消息不该诱导用户再发一条。
    expect(page?.input.draft).toBe("");
  });

  it("只回终态的快速失败：acceptedAt 证明已受理，输入保留且不回填草稿", () => {
    const { store } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "快速失败但已保存");
    if (!outcome.ok) throw new Error("提交应被接受");
    // 没有先前的 accepted：服务端在受理返回前同步派发失败，只能回一条带 acceptedAt 的终态。
    store.apply({ kind: "command_result", result: promptResult({ requestId: outcome.requestId, status: "failed" }) });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    const entry = (page?.timeline.entries ?? []).find(
      (item) => item.kind === "message" && item.channel === "user" && item.message.text === "快速失败但已保存",
    );
    expect(entry).toBeDefined();
    expect(entry?.kind === "message" ? entry.local?.phase : null).toBe("accepted");
    expect(page?.input.draft).toBe("");
  });

  it("受理结果不确定：输入保留并标注不确定，草稿不回填", () => {
    const { store } = interactionStore();
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "不知道有没有保存");
    if (!outcome.ok) throw new Error("提交应被接受");
    store.apply({ kind: "command_result", result: promptResult({ requestId: outcome.requestId, status: "uncertain" }) });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    const entry = (page?.timeline.entries ?? []).find(
      (item) => item.kind === "message" && item.channel === "user" && item.message.text === "不知道有没有保存",
    );
    expect(entry?.kind === "message" ? entry.local?.phase : null).toBe("uncertain");
    expect(entry?.kind === "message" ? entry.local?.notice : null).toContain("不确定");
    expect(page?.input.draft).toBe("");
    expect(page?.input.submission.kind).toBe("uncertain");
  });
});

describe("输入与提交以终态为准", () => {
  it("提交只发文本、先呈现待确认，服务端确认后才变已接受", () => {
    const { store, gateway } = interactionStore();
    store.setDraft(ONLINE_SESSION_ID, "  帮我改一下  ");
    const outcome = store.submitPrompt(ONLINE_SESSION_ID, "  帮我改一下  ");
    expect(outcome.ok).toBe(true);

    const body = lastDispatch(gateway).body;
    expect(body.command).toBe("session.prompt");
    if (body.command !== "session.prompt") throw new Error("命令不是 session.prompt");
    // 载荷只有文本块：附件在 v1 没有表达方式（因此不可能被「静默丢弃后提交剩下的」）。
    expect(body.payload).toEqual({ content: [{ type: "text", text: "帮我改一下" }] });

    const pending = store.conversationPage(ONLINE_SESSION_ID);
    expect(pending?.input.submission.kind).toBe("pending_confirmation");
    // 反例：本地乐观置态会在这里给出 accepted/completed → 断言变红。
    expect(pending?.input.state).toBe("in_progress");
    // 提交后草稿清空（它已经不是「未提交的文本」了，而是一次待确认的命令）。
    expect(pending?.input.draft).toBe("");
    // 附件不受支持是显式说明，不是「没提」。
    expect(pending?.input.attachmentsSupported).toBe(false);
    expect(pending?.input.attachmentNotice).toContain("不支持");

    if (!outcome.ok) throw new Error("提交应被接受");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f2",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.prompt",
          status: "accepted",
          acceptedAt: "2026-10-01T00:00:02.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      },
    });

    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.submission).toEqual({
      kind: "accepted",
      acceptedAt: "2026-10-01T00:00:02.000Z",
    });
  });

  it("轮次进行中：canSubmit=false、给出原因，提交（含回车派发路径）不产生命令", () => {
    const { store, gateway } = interactionStore();
    store.apply(turnStarted("dddddddd-0000-4000-8000-0000000000d0", "10"));

    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.input.state).toBe("in_progress");
    expect(page?.input.blockedBy).toBe("turn_in_progress");
    // 反例：进行中仍可提交 → 下面两条会红。
    expect(page?.input.canSubmit).toBe(false);
    expect(page?.input.disabledReason).toContain("进行中");

    const before = gateway.dispatches.length;
    // 回车提交在组件里先看 `canSubmit`；仓库侧闸门是同一份模型值，因此这条路径不派发命令。
    expect(store.submitPrompt(ONLINE_SESSION_ID, "这条不该发出去").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(before);
  });

  it("空文本与未连接时不派发命令；设备离线不再重复输入区原因文案", () => {
    const { store, gateway } = interactionStore();
    const before = gateway.dispatches.length;
    expect(store.submitPrompt(ONLINE_SESSION_ID, "   ").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(before);

    const { store: offline, gateway: offlineGateway } = storeWithNoConnection();
    const page = offline.conversationPage(ONLINE_SESSION_ID);
    expect(page?.input.state).toBe("unavailable");
    expect(page?.input.blockedBy).toBe("connection_offline");
    // 原因由连接问题弹层完整说明；输入区只关闭提交闸门，不重复一句「连接不可用」。
    expect(page?.input.disabledReason).toBeNull();
    // 不可用仍可编辑草稿（三态都不只读）。
    offline.setDraft(ONLINE_SESSION_ID, "离线也能写");
    expect(offline.conversationPage(ONLINE_SESSION_ID)?.input.draft).toBe("离线也能写");
    expect(offline.submitPrompt(ONLINE_SESSION_ID, "你好").ok).toBe(false);
    expect(offlineGateway.dispatches).toEqual([]);
  });

  it("来源为其它节点的会话：不可提交、草稿只在内存并标注未提交", () => {
    const remote = session({
      id: ONLINE_SESSION_ID,
      state: "running",
      origin: {
        kind: "remote",
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        exportId: "export-1",
        originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
        online: false,
      },
    });
    const { store, gateway } = onlineStore({ resources: resources({ sessions: [remote] }) });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.input.canSubmit).toBe(false);
    expect(page?.input.blockedBy).toBe("source_offline");
    expect(page?.input.disabledReason).toBe("来源节点离线，不能提交");
    expect(page?.input.draftInMemoryOnly).toBe(true);
    expect(page?.input.draftNotice).toContain("只在内存");

    store.setDraft(ONLINE_SESSION_ID, "这段草稿不该落盘");
    expect(store.draftOf(ONLINE_SESSION_ID)).toBe("这段草稿不该落盘");
    // 草稿经模型暴露给输入区（受控输入），并且标注仍然是「未提交」。
    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.draft).toBe("这段草稿不该落盘");

    const before = gateway.dispatches.length;
    expect(store.submitPrompt(ONLINE_SESSION_ID, "这段草稿不该落盘").ok).toBe(false);
    // 反例：把 imported 会话的输入标记为已发送 → 这里会多一条派发记录 → 断言变红。
    expect(gateway.dispatches).toHaveLength(before);
  });
});

describe("取消当前轮次以终态为准", () => {
  it("有进行中的轮次时取消 → 先呈现已提交，终态后呈现已确认", () => {
    const { store, gateway } = interactionStore();
    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.cancel.kind).toBe("unavailable");
    store.apply(turnStarted("dddddddd-0000-4000-8000-0000000000d1", "11"));
    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.cancel).toEqual({ kind: "available", failure: null });

    const outcome = store.cancelTurn(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.cancel") throw new Error("命令不是 session.cancel");
    expect(body.payload).toEqual({ turnId: "turn-1" });

    // 反例：立即把会话呈现为空闲 → 这里会是 available/confirmed → 断言变红。
    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.cancel).toEqual({
      kind: "requested",
      requestId: outcome.ok ? outcome.requestId : "",
    });

    if (!outcome.ok) throw new Error("取消应被接受");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f3",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.cancel",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:03.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      },
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.input.cancel).toEqual({ kind: "confirmed" });
  });

  it("取消失败后回到可取消并带上失败说明（不假装成功）", () => {
    const { store } = interactionStore();
    store.apply(turnStarted("dddddddd-0000-4000-8000-0000000000d2", "12"));
    const outcome = store.cancelTurn(ONLINE_SESSION_ID);
    if (!outcome.ok) throw new Error("取消应被接受");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f4",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.cancel",
          status: "failed",
          acceptedAt: "2026-10-01T00:00:04.000Z",
          terminalEventId: null,
          result: null,
          error: { code: "session.busy", message: "轮次已结束", retryable: false, details: {} },
        },
      },
    });

    const cancel = store.conversationPage(ONLINE_SESSION_ID)?.input.cancel;
    expect(cancel?.kind).toBe("available");
    expect(cancel?.kind === "available" ? cancel.failure : null).toContain("session.busy");
  });

  it("没有进行中的轮次时不派发取消命令", () => {
    const { store, gateway } = interactionStore();
    const before = gateway.dispatches.length;
    expect(store.cancelTurn(ONLINE_SESSION_ID).ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(before);
  });
});

describe("模型与模式的呈现与切换", () => {
  it("未提供模型可选项时呈现 unavailable 且没有可点选项", () => {
    const { store } = interactionStore();
    const model = store.conversationPage(ONLINE_SESSION_ID)?.settings.model;
    expect(model?.state).toBe("unavailable");
    expect(model?.choices).toEqual([]);
    expect(model?.notice).toContain("未提供模型可选项");
  });

  it("切换中 → 服务端确认后呈现新值与「下一轮生效」", () => {
    const { store, gateway } = interactionStore();
    store.apply(configChanged("dddddddd-0000-4000-8000-0000000000d3", "13", "1"));

    const before = store.conversationPage(ONLINE_SESSION_ID)?.settings.model;
    expect(before?.state).toBe("available");
    expect(before?.currentId).toBe("fast");
    expect(before?.choices.map((choice) => choice.id)).toEqual(["fast", "smart"]);

    const outcome = store.setConfigValue(ONLINE_SESSION_ID, "model", "smart");
    expect(outcome.ok).toBe(true);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.config.set") throw new Error("命令不是 session.config.set");
    expect(body.expectedVersion).toBe("1");
    expect(body.payload).toEqual({ configId: "model", value: "smart" });

    const switching = store.conversationPage(ONLINE_SESSION_ID)?.settings.model;
    expect(switching?.state).toBe("switching");
    // 切换中**不**预先改本地当前值（新值只由服务端确认）。
    expect(switching?.currentId).toBe("fast");
    expect(switching?.notice).toContain("下一轮生效");

    if (!outcome.ok) throw new Error("切换应被接受");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f5",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.config.set",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:05.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      },
    });
    const confirmed = store.conversationPage(ONLINE_SESSION_ID)?.settings.model;
    expect(confirmed?.state).toBe("available");
    expect(confirmed?.notice).toContain("下一轮生效");
  });

  it("模式清单未加载时不冒充「没有模式」，加载后才给出可选项", () => {
    const { store, gateway } = interactionStore();
    const before = store.conversationPage(ONLINE_SESSION_ID)?.settings.mode;
    expect(before?.state).toBe("not_loaded");
    expect(before?.choices).toEqual([]);

    const outcome = store.loadModes(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const body = lastDispatch(gateway).body;
    expect(body.command).toBe("session.mode.list");
    if (!outcome.ok) throw new Error("加载模式清单应被接受");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f6",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.mode.list",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:06.000Z",
          terminalEventId: null,
          result: {
            currentModeId: "code",
            availableModes: [
              { modeId: "code", displayName: "写代码" },
              { modeId: "ask", displayName: "只回答" },
            ],
            version: "4",
          },
          error: null,
        },
      },
    });

    const mode = store.conversationPage(ONLINE_SESSION_ID)?.settings.mode;
    expect(mode?.state).toBe("available");
    expect(mode?.currentId).toBe("code");
    expect(mode?.choices.map((choice) => [choice.id, choice.current])).toEqual([
      ["code", true],
      ["ask", false],
    ]);

    const switched = store.setMode(ONLINE_SESSION_ID, "ask");
    expect(switched.ok).toBe(true);
    const setCommand = lastDispatch(gateway).body;
    if (setCommand.command !== "session.mode.set") throw new Error("命令不是 session.mode.set");
    expect(setCommand.expectedVersion).toBe("4");
    expect(setCommand.payload).toEqual({ modeId: "ask" });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.settings.mode.state).toBe("switching");

    // 服务端确认了模式变更：core **不**产出 `session.updated`，因此 `session.mode.changed`
    // 是当前模式与并发版本唯一的活动信号。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.mode.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000c1",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "11",
        view: { currentModeId: "ask", version: "7" },
      }),
    });
    const changed = store.conversationPage(ONLINE_SESSION_ID)?.settings.mode;
    // 反例：只读清单的旧实现会停在 "code"（同一事件在时间线上却已是 "ask"）→ 断言变红。
    expect(changed?.currentId).toBe("ask");
    expect(changed?.currentName).toBe("只回答");
    expect(changed?.choices.map((choice) => [choice.id, choice.current])).toEqual([
      ["code", false],
      ["ask", true],
    ]);

    // 并发版本同源：仍拿清单里的 "4" 去提交会被服务端以版本冲突直接拒掉。
    const switchedBack = store.setMode(ONLINE_SESSION_ID, "code");
    expect(switchedBack.ok).toBe(true);
    const backCommand = lastDispatch(gateway).body;
    if (backCommand.command !== "session.mode.set") throw new Error("命令不是 session.mode.set");
    expect(backCommand.expectedVersion).toBe("7");
  });

  it("模式变更缺席时回落清单、再回落快照摘要（摘要名字只在它就是当前项时使用）", () => {
    // 快照摘要带 `currentMode`：清单还没加载时它是当前值唯一的来源。
    const { store } = onlineStore({
      resources: resources({
        sessions: [
          session({
            id: ONLINE_SESSION_ID,
            state: "running",
            currentMode: { modeId: "ask", displayName: "只回答" },
          }),
        ],
      }),
    });
    const fromSummary = store.conversationPage(ONLINE_SESSION_ID)?.settings.mode;
    expect(fromSummary?.state).toBe("not_loaded");
    expect(fromSummary?.currentId).toBe("ask");
    expect(fromSummary?.currentName).toBe("只回答");

    // 事件比摘要新：当前值随事件走，且 MUST NOT 把摘要里**另一个**模式的名字安到它头上。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.mode.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000c2",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "9",
        view: { currentModeId: "code", version: "9" },
      }),
    });
    const fromChange = store.conversationPage(ONLINE_SESSION_ID)?.settings.mode;
    expect(fromChange?.currentId).toBe("code");
    expect(fromChange?.currentName).toBeNull();
  });
});

describe("session.origin.online_changed 只改来源可达性", () => {
  it("逐字段保持摘要其余字段（只翻 origin.online）", () => {
    const remote: SessionSummary = session({
      id: ONLINE_SESSION_ID,
      state: "running",
      title: "远程任务",
      origin: {
        kind: "remote",
        ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
        exportId: "export-1",
        originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
        online: true,
      },
    });
    const { store } = onlineStore({ resources: resources({ sessions: [remote] }) });
    const before = store.conversationPage(ONLINE_SESSION_ID);

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.origin.online_changed",
        eventId: "dddddddd-0000-4000-8000-0000000000e1",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "21",
        view: {
          ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
          exportId: "export-1",
          originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
          online: false,
        },
      }),
    });

    const after = store.state.resources?.sessions.find((item) => item.sessionId === ONLINE_SESSION_ID);
    if (after === undefined || after.origin.kind !== "remote" || remote.origin.kind !== "remote") {
      throw new Error("夹具前提不成立");
    }
    // 只有 reachability 变了：其余每一个字段都与事件到达前**逐字段相同**。
    expect(after.origin.online).toBe(false);
    expect({ ...after, origin: remote.origin }).toEqual(remote);
    expect(after.origin.ownerNodeId).toBe(remote.origin.ownerNodeId);
    expect(after.origin.exportId).toBe(remote.origin.exportId);
    expect(after.origin.originEpoch).toBe(remote.origin.originEpoch);
    expect(after.title).toBe(remote.title);
    expect(after.state).toBe(remote.state);
    expect(after.version).toBe(remote.version);
    expect(after.workspace).toEqual(remote.workspace);

    // 呈现上不再有来源离线标注：只有输入可用性随可达性变化。
    const offline = store.conversationPage(ONLINE_SESSION_ID);
    expect(offline?.header).not.toHaveProperty("originOfflineLabel");
    expect(offline?.header.title).toBe(before?.header.title);
    expect(offline?.header.state).toBe(before?.header.state);
    // 来源离线后不得再把输入标为可发送；在线时远程来源可发送（stateless origin 的远程命令）。
    expect(before?.capabilities.canSend).toBe(true);
    expect(offline?.capabilities.canSend).toBe(false);
    // 该事件走摘要通道，因此不进时间线、也不产生降级卡片。
    expect(offline?.timeline.entries).toEqual([]);
    expect(offline?.degraded).toEqual([]);

    // 回到可达：标注消失，摘要其余字段仍逐字段相同。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.origin.online_changed",
        eventId: "dddddddd-0000-4000-8000-0000000000e2",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "22",
        view: {
          ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
          exportId: "export-1",
          originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
          online: true,
        },
      }),
    });
    const back = store.state.resources?.sessions.find((item) => item.sessionId === ONLINE_SESSION_ID);
    expect(back).toEqual(remote);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.capabilities.canSend).toBe(true);
  });

  it("本节点自有的会话不会被这条事件安上来源节点", () => {
    const { store } = interactionStore();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.origin.online_changed",
        eventId: "dddddddd-0000-4000-8000-0000000000e3",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "23",
        view: {
          ownerNodeId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
          exportId: "export-1",
          originEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
          online: false,
        },
      }),
    });

    const summary = store.state.resources?.sessions.find((item) => item.sessionId === ONLINE_SESSION_ID);
    expect(summary?.origin).toEqual({ kind: "local" });
  });
});
