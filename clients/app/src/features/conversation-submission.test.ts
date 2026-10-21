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

import type { SessionSummary } from "../protocol";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  interactionStore,
  lastDispatch,
  onlineStore,
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

  it("空文本与未连接时不派发命令；未连接的原因说明是「来源离线」", () => {
    const { store, gateway } = interactionStore();
    const before = gateway.dispatches.length;
    expect(store.submitPrompt(ONLINE_SESSION_ID, "   ").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(before);

    const { store: offline, gateway: offlineGateway } = storeWithNoConnection();
    const page = offline.conversationPage(ONLINE_SESSION_ID);
    expect(page?.input.state).toBe("unavailable");
    expect(page?.input.blockedBy).toBe("source_offline");
    expect(page?.input.disabledReason).toContain("连接不可用");
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

describe("session.origin.online_changed 只改来源可达性标注", () => {
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
    expect(before?.header.originOfflineLabel).toBeNull();

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

    // 呈现上只多了那一句离线标注，其余模型字段不变。
    const offline = store.conversationPage(ONLINE_SESSION_ID);
    expect(offline?.header.originOfflineLabel).toContain("当前离线");
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
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.originOfflineLabel).toBeNull();
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
