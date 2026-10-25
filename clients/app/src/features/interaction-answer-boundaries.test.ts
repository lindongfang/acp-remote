/**
 * D7 授权与提问应答的三条边界（spec「授权与提问应答的可用边界」）。
 *
 * 三条边界各有一条实测：
 *
 * 1. **亲历的请求可应答**：本设备连接期间收到 `permission.requested` → 可选项来自该请求自身，
 *    提交后派发 `permission.resolve`，呈现先变 `submitting`（待确认）再变 `handled_here`。
 * 2. **仅历史读到的待处理项不可操作**：`session.read` 读回的 `pending_interactions` 没有可选项
 *    → `not_answerable(read_from_history)`，且 `resolvePermission` **不派发任何命令**。
 * 3. **已被其它设备处理**：`permission.resolved` 的 `resolvedByDeviceId` 不是本设备
 *    → `handled_elsewhere`，重复提交被拒绝（`interactions` 不可重复提交，spec MUST NOT）。
 *
 * 会话主状态不再是应答的边界：`idle`/`running`/`pending` 都不构成「不能再应答」，也**不**
 * 存在 `session_closed` 这一不可应答原因。历史事件视图里的旧七态标签（例如 `closed`）不会被
 * 当成当前摘要应用——见下面「旧标签不覆盖当前状态」。
 *
 * 另有一条对照证明「本设备提交并获确认」呈现为 `handled_here`（否则上面第 1 条只是「永远
 * 不确认」的平凡通过）。
 */

import { describe, expect, it } from "vitest";

import type { EventMessage } from "../protocol";
import { PENDING_INTERACTIONS_FEATURE, SESSION_HISTORY_FEATURE } from "../sync-client";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  historyItem,
  interactionStore,
  interactionViewOf,
  lastDispatch,
  onlineStore,
  readResult,
  resources,
  session,
} from "./testing/fixtures";

const PERMISSION_ID = "99999999-0000-4000-8000-000000000001";
const ELICITATION_ID = "99999999-0000-4000-8000-000000000002";

/** 一条亲历的 `permission.requested`。 */
function permissionRequested(input: {
  readonly eventId: string;
  readonly globalSequence: string;
  readonly interactionId?: string;
  readonly options?: readonly { readonly optionId: string; readonly label: string; readonly kind: string }[];
}): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: "permission.requested",
      eventId: input.eventId,
      sessionId: ONLINE_SESSION_ID,
      globalSequence: input.globalSequence,
      view: {
        interactionId: input.interactionId ?? PERMISSION_ID,
        turnId: "turn-1",
        title: "允许写入 src/ 吗？",
        description: null,
        options: input.options ?? [
          { optionId: "allow-once", label: "允许一次", kind: "allow_once" },
          { optionId: "deny", label: "拒绝", kind: "deny" },
        ],
      },
    }),
  };
}

describe("D7 边界一：亲历的请求可应答", () => {
  it("可选项来自请求自身；提交后先呈现待确认，终态到达后呈现已由本设备处理", () => {
    const { store, gateway } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c1", globalSequence: "11" }));

    const before = interactionViewOf(store, PERMISSION_ID);
    expect(before?.witnessed).toBe(true);
    expect(before?.availability).toEqual({
      kind: "answerable",
      answers: [
        { via: "option", optionId: "allow-once", label: "允许一次", kind: "allow_once" },
        { via: "option", optionId: "deny", label: "拒绝", kind: "deny" },
      ],
    });
    // 时间线上的那张授权卡片带同一份判定（卡片与应答区不各自为政）。
    const card = store
      .conversationPage(ONLINE_SESSION_ID)
      ?.timeline.entries.find((entry) => entry.kind === "permission_requested");
    expect(card?.kind === "permission_requested" ? card.availability.kind : null).toBe("answerable");

    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(true);
    const dispatched = lastDispatch(gateway).body;
    expect(dispatched.command).toBe("permission.resolve");
    if (dispatched.command !== "permission.resolve") throw new Error("命令不是 permission.resolve");
    // 载荷精确两键：可选项**只能**来自请求本身（没有名字、没有自由文本）。
    expect(dispatched.payload).toEqual({ interactionId: PERMISSION_ID, optionId: "allow-once" });
    expect(Object.keys(dispatched.payload).sort()).toEqual(["interactionId", "optionId"]);

    // 终态之前：待确认（**不**呈现为已处理）。
    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({
      kind: "submitting",
      requestId: outcome.ok ? outcome.requestId : "",
    });
    // 重复提交被拒，且不再派发命令。
    const dispatches = gateway.dispatches.length;
    expect(store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "deny").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(dispatches);
  });

  it("本设备提交的命令完成 → 呈现已由本设备处理（对照：不是「永远待确认」）", () => {
    const { store, gateway } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c2", globalSequence: "12" }));
    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "deny");
    if (!outcome.ok) throw new Error("应答应被接受");

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f1",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "permission.resolve",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: { interactionId: PERMISSION_ID, resolution: "deny" },
          error: null,
        },
      },
    });

    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({ kind: "handled_here", resolution: "deny" });
    expect(gateway.dispatches).toHaveLength(1);
  });

  it("提问请求的可选项是固定三支动作（提交/拒绝/取消），不来自服务端可选项", () => {
    const { store } = interactionStore();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "elicitation.requested",
        eventId: "dddddddd-0000-4000-8000-0000000000c3",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "13",
        view: {
          interactionId: ELICITATION_ID,
          turnId: "turn-1",
          title: "请补一个分支名",
          schema: { type: "object", properties: { branch: { type: "string" } } },
          initialValues: null,
        },
      }),
    });

    const view = interactionViewOf(store, ELICITATION_ID);
    expect(view?.availability).toEqual({
      kind: "answerable",
      answers: [
        { via: "action", action: "submit", label: "提交" },
        { via: "action", action: "decline", label: "拒绝" },
        { via: "action", action: "cancel", label: "取消" },
      ],
    });
    expect(view?.schema).toMatchObject({ type: "object" });
  });
});

describe("D7 边界二：仅从历史读到的待处理项不可操作", () => {
  it("读回的 pending_interactions 没有可选项 → 不可应答，且提交不派发命令", () => {
    const { store, gateway } = interactionStore();
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        pending: [
          {
            interactionId: PERMISSION_ID,
            sessionId: ONLINE_SESSION_ID,
            kind: "permission",
            state: "pending",
            schema: {},
            createdAt: "2026-10-01T00:00:02.000Z",
          },
        ],
      }),
    });

    const view = interactionViewOf(store, PERMISSION_ID);
    expect(view?.witnessed).toBe(false);
    expect(view?.availability).toEqual({ kind: "not_answerable", reason: "read_from_history" });
    expect(view?.title).toBeNull();

    const dispatches = gateway.dispatches.length;
    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.reason).toContain("只从历史读到");
    // 反例：画出可点的应答按钮 → 这里会多一条派发记录 → 断言变红。
    expect(gateway.dispatches).toHaveLength(dispatches);

    // 读回的待处理项**不是**降级事件（它是一条正常的记录），所以降级列表为空；
    // 它的呈现就是交互模型里的「不可操作 + 原因」，而不是一张卡上的可点按钮。
    const card = store
      .conversationPage(ONLINE_SESSION_ID)
      ?.timeline.entries.find((entry) => entry.kind === "permission_requested");
    expect(card).toBeUndefined();
    expect(store.conversationPage(ONLINE_SESSION_ID)?.degraded).toEqual([]);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.interactions.map((item) => item.interactionId)).toEqual([
      PERMISSION_ID,
    ]);
  });
});

describe("D7 边界三：已被其它设备处理", () => {
  it("resolvedByDeviceId 不是本设备 → 呈现已处理并拒绝重复提交", () => {
    const { store, gateway } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c4", globalSequence: "14" }));
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.resolved",
        eventId: "dddddddd-0000-4000-8000-0000000000c5",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "15",
        view: { interactionId: PERMISSION_ID, resolution: "allow", resolvedByDeviceId: "11111111-1111-4111-8111-111111111111" },
      }),
    });

    const view = interactionViewOf(store, PERMISSION_ID);
    expect(view?.availability).toEqual({
      kind: "handled_elsewhere",
      resolution: "allow",
      resolvedByDeviceId: "11111111-1111-4111-8111-111111111111",
    });

    const dispatches = gateway.dispatches.length;
    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.reason).toContain("已被其它设备处理");
    // 反例：仍允许再提交 → 这里会多一条派发记录 → 断言变红。
    expect(gateway.dispatches).toHaveLength(dispatches);
  });

  it("本设备提交过、且服务端未给出 resolvedByDeviceId 时呈现为已由本设备处理", () => {
    const { store } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c6", globalSequence: "16" }));
    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(true);
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "permission.resolved",
        eventId: "dddddddd-0000-4000-8000-0000000000c7",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "17",
        view: { interactionId: PERMISSION_ID, resolution: "allow", resolvedByDeviceId: null },
      }),
    });

    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({ kind: "handled_here", resolution: "allow" });
  });
});

describe("旧七态标签不覆盖当前三态", () => {
  /** 一条携带旧七态摘要的 `session.updated`（历史事件视图可能仍带这种载荷）。 */
  function legacySummaryEvent(eventId: string, globalSequence: string, state: string): {
    readonly kind: "event";
    readonly event: EventMessage;
  } {
    return {
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        eventId,
        sessionId: ONLINE_SESSION_ID,
        globalSequence,
        view: {
          session: {
            sessionId: ONLINE_SESSION_ID,
            title: "历史标题",
            agent: { agentId: "claude", name: "Claude" },
            state,
            origin: { kind: "local" },
            currentMode: null,
            version: "2",
            createdAt: "2026-10-01T00:00:00.000Z",
            updatedAt: "2026-10-01T00:00:05.000Z",
          },
        },
      }),
    };
  }

  it("旧 `closed` 标签既不写进当前状态，也不让交互或提交变得不可用", () => {
    const { store, gateway } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c8", globalSequence: "18" }));
    const before = store.conversationPage(ONLINE_SESSION_ID);
    expect(before?.header.state).toBe("running");

    // 反例：把历史旧标签当实时摘要应用 → 状态变成 closed、交互变为不可应答、提交被拦。
    store.apply(legacySummaryEvent("dddddddd-0000-4000-8000-0000000000c9", "19", "closed"));

    const summary = store.state.resources?.sessions.find((item) => item.sessionId === ONLINE_SESSION_ID);
    expect(summary?.state).toBe("running");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.state).toBe("running");
    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");

    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(true);
    expect(lastDispatch(gateway).body.command).toBe("permission.resolve");
  });

  it("旧 `closed` 标签不再限制文本提交（三态里没有第三种只读）", () => {
    const { store, gateway } = interactionStore();
    store.apply(legacySummaryEvent("dddddddd-0000-4000-8000-0000000000ca", "20", "waiting_permission"));

    const page = store.conversationPage(ONLINE_SESSION_ID);
    // 反例：旧标签把输入区判成 closed → 状态与原因都会以「已关闭」出现。
    expect(page?.input.state).toBe("editable");
    expect(page?.input.blockedBy).toBeNull();

    const dispatches = gateway.dispatches.length;
    expect(store.submitPrompt(ONLINE_SESSION_ID, "还能发吗").ok).toBe(true);
    expect(gateway.dispatches.length).toBe(dispatches + 1);
    expect(lastDispatch(gateway).body.command).toBe("session.prompt");
  });
});

/**
 * D6 刷新重新取得有效请求（tasks 6.5「刷新重新取得有效交互」）。
 *
 * 判别力：`session.read` 的 include 一旦漏掉 `pending_interactions`（旧的 `refresh_history`
 * 只带 `history`），下面第一条断言直接变红；把刷新结果当成首屏资源整份替换，配置项断言变红；
 * 结果为空时仍停在 `read_from_history`，按钮点击在界面上就「什么都没有发生」，最后一条变红。
 */
describe("D6 刷新重新取得有效请求", () => {
  /** 一个协商到结构化历史 + 有效交互的仓库（首屏能读配置项，也能刷新 pending）。 */
  function refreshableStore() {
    return onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
      features: [SESSION_HISTORY_FEATURE, PENDING_INTERACTIONS_FEATURE],
    });
  }

  const CONFIG_OPTION = {
    id: "model",
    name: "模型",
    description: null,
    category: "model",
    type: "select",
    currentValue: "fast",
    options: [{ value: "fast", name: "快", description: null }],
  } as const;

  it("刷新请求 pending_interactions，结果只合并待处理交互（配置项不被清空）", () => {
    const { store, gateway } = refreshableStore();
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        pending: [
          // 旧 peer 的定位形状：只从历史读到，不可操作。
          {
            interactionId: PERMISSION_ID,
            sessionId: ONLINE_SESSION_ID,
            kind: "permission",
            state: "pending",
            schema: {},
            createdAt: "2026-10-01T00:00:02.000Z",
          },
        ],
        configOptions: [{ sessionId: ONLINE_SESSION_ID, configOptions: [CONFIG_OPTION], version: "1" }],
      }),
    });
    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({
      kind: "not_answerable",
      reason: "read_from_history",
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.settings.model.currentId).toBe("fast");

    const outcome = store.refreshInteractions(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const dispatched = lastDispatch(gateway).body;
    expect(dispatched.command).toBe("session.read");
    if (dispatched.command !== "session.read") throw new Error("命令不是 session.read");
    // 只请求当前有效请求这一份资源（刷新历史是另一条 intent）。
    expect(dispatched.payload.include).toEqual(["pending_interactions"]);

    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: outcome.ok ? outcome.requestId : "",
        sessionId: ONLINE_SESSION_ID,
        pending: [
          {
            interactionId: PERMISSION_ID,
            sessionId: ONLINE_SESSION_ID,
            kind: "permission",
            requestVersion: "7",
            expiresAt: null,
            options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
            schema: null,
          },
        ],
      }),
    });

    // 服务端确认后，卡片从「只从历史读到」变为可应答。
    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({
      kind: "answerable",
      answers: [{ via: "option", optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
    });
    // 刷新只替换 pending：首屏读到的模型可选项仍在。
    expect(store.conversationPage(ONLINE_SESSION_ID)?.settings.model.currentId).toBe("fast");
  });

  it("刷新结果不再包含该请求时标「服务端不再确认」，按钮点击有可见结果", () => {
    const { store, gateway } = refreshableStore();
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: ONLINE_SESSION_ID,
        // 历史里仍有这条请求的定位记录：它是卡片继续存在的来源（刷新只换「当前有效」这一半）。
        history: [
          historyItem({
            eventId: "dddddddd-0000-4000-8000-0000000000d1",
            eventType: "permission.requested",
            view: {
              interactionId: PERMISSION_ID,
              turnId: "turn-1",
              title: "允许写入 src/ 吗？",
              description: null,
              options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
            },
          }),
        ],
        pending: [
          {
            interactionId: PERMISSION_ID,
            sessionId: ONLINE_SESSION_ID,
            kind: "permission",
            state: "pending",
            schema: {},
            createdAt: "2026-10-01T00:00:02.000Z",
          },
        ],
      }),
    });

    const outcome = store.refreshInteractions(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: outcome.ok ? outcome.requestId : "",
        sessionId: ONLINE_SESSION_ID,
        pending: [],
      }),
    });

    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({
      kind: "not_answerable",
      reason: "not_effective",
    });
    // 不在有效集合里就不能应答（不画一个点了必然失败的按钮）。
    const dispatches = gateway.dispatches.length;
    expect(store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(dispatches);
  });

  it("未协商 core.pending-interactions.v1 时不派发命令并给出原因", () => {
    const { store, gateway } = interactionStore();
    const dispatches = gateway.dispatches.length;
    const outcome = store.refreshInteractions(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.reason).toContain("core.pending-interactions.v1");
    expect(gateway.dispatches).toHaveLength(dispatches);
  });
});

/**
 * 实时请求的 `requestVersion`（`docs/SYNC_PROTOCOL.md` §11.5/§11.7）。
 *
 * 协商 `core.pending-interactions.v1` 后应答 MUST 携带 `requestVersion`，服务端不得因缺席
 * 放宽校验。实时事件有两种形状：视图自带版本（原样用）与只有定位信息（必须先经
 * `session.read` 取得，未取得前不得画出必然失败的按钮）。
 */
describe("D7 实时交互的请求版本", () => {
  /** 协商到结构化历史 + 有效交互的仓库（与刷新用例同一前提）。 */
  function negotiatedStore() {
    return onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
      features: [SESSION_HISTORY_FEATURE, PENDING_INTERACTIONS_FEATURE],
    });
  }

  /** 一条带版本（或不带）的实时 `permission.requested`。 */
  function versionedPermission(input: {
    readonly eventId: string;
    readonly globalSequence: string;
    readonly requestVersion?: string;
  }): { readonly kind: "event"; readonly event: EventMessage } {
    return {
      kind: "event",
      event: eventMessage({
        eventType: "permission.requested",
        eventId: input.eventId,
        sessionId: ONLINE_SESSION_ID,
        globalSequence: input.globalSequence,
        view: {
          interactionId: PERMISSION_ID,
          turnId: "turn-1",
          title: "允许写入 src/ 吗？",
          description: null,
          options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
          ...(input.requestVersion === undefined ? {} : { requestVersion: input.requestVersion }),
        },
      }),
    };
  }

  it("事件视图自带请求版本：直接可应答，应答载荷原样回传该版本", () => {
    const { store, gateway } = negotiatedStore();
    store.apply(
      versionedPermission({
        eventId: "dddddddd-0000-4000-8000-0000000000e1",
        globalSequence: "41",
        requestVersion: "7",
      }),
    );

    const view = interactionViewOf(store, PERMISSION_ID);
    expect(view?.requestVersion).toBe("7");
    expect(view?.availability.kind).toBe("answerable");

    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(true);
    const dispatched = lastDispatch(gateway).body;
    expect(dispatched.command).toBe("permission.resolve");
    if (dispatched.command !== "permission.resolve") throw new Error("命令不是 permission.resolve");
    expect(dispatched.payload).toEqual({
      interactionId: PERMISSION_ID,
      optionId: "allow-once",
      requestVersion: "7",
    });
  });

  it("核实结果排除旧实时请求后撤销其审批能力", () => {
    const { store, gateway } = negotiatedStore();
    store.apply(versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000f9", globalSequence: "59", requestVersion: "7" }));
    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");
    store.refreshInteractions(ONLINE_SESSION_ID);
    store.apply({ kind: "command_result", result: readResult({ requestId: lastDispatch(gateway).body.requestId,
      sessionId: ONLINE_SESSION_ID, pending: [] }) });
    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({ kind: "not_answerable", reason: "not_effective" });
    const count = gateway.dispatches.length;
    expect(store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(count);
  });

  it.each(["turn.cancelled", "turn.completed", "turn.failed"])("%s 后即使缓存仍确认旧请求也不能继续审批", (eventType) => {
    const { store, gateway } = negotiatedStore();
    store.apply(versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000a1", globalSequence: "61", requestVersion: "7" }));
    store.refreshInteractions(ONLINE_SESSION_ID);
    store.apply({ kind: "command_result", result: readResult({ requestId: lastDispatch(gateway).body.requestId,
      sessionId: ONLINE_SESSION_ID, pending: [{ interactionId: PERMISSION_ID, sessionId: ONLINE_SESSION_ID, kind: "permission",
        requestVersion: "7", expiresAt: null, options: [{ optionId: "allow-once", label: "Allow once", kind: "allow_once" }], schema: null }] }) });
    store.apply({ kind: "event", event: eventMessage({ eventType, eventId: "dddddddd-0000-4000-8000-0000000000a2",
      sessionId: ONLINE_SESSION_ID, globalSequence: "62", view: { turnId: "turn-1", state: eventType.slice(5) } }) });
    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({ kind: "not_answerable", reason: "turn_ended" });
    expect(store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once").ok).toBe(false);
  });

  it("旧读取的空结果不使读取期间刚收到的新请求失效", () => {
    const { store, gateway } = negotiatedStore();
    store.apply(versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000f7", globalSequence: "57", requestVersion: "7" }));
    store.refreshInteractions(ONLINE_SESSION_ID);
    const requestId = lastDispatch(gateway).body.requestId;
    store.apply(versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000f8", globalSequence: "58", requestVersion: "8" }));
    store.apply({ kind: "command_result", result: readResult({ requestId, sessionId: ONLINE_SESSION_ID, pending: [] }) });
    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");
    expect(interactionViewOf(store, PERMISSION_ID)?.requestVersion).toBe("8");
  });

  it("其他轮次的终态不会使当前审批失效", () => {
    const { store } = negotiatedStore();
    store.apply(versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000b1", globalSequence: "71", requestVersion: "7" }));
    store.apply({ kind: "event", event: eventMessage({ eventType: "turn.cancelled",
      eventId: "dddddddd-0000-4000-8000-0000000000b2", sessionId: ONLINE_SESSION_ID, globalSequence: "72",
      view: { turnId: "another-turn", state: "cancelled" } }) });
    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");
  });

  it("初次读取期间收到缺版本授权：等读取完成后再核实，不丢失刷新", () => {
    const { store, gateway } = negotiatedStore();
    store.loadSession(ONLINE_SESSION_ID);
    const firstRead = lastDispatch(gateway).body.requestId;
    const count = gateway.dispatches.length;
    const request = versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000e9", globalSequence: "49" });
    store.apply(request);
    expect(gateway.dispatches).toHaveLength(count);
    store.apply({ kind: "command_result", result: readResult({ requestId: firstRead, sessionId: ONLINE_SESSION_ID, pending: [] }) });
    expect(gateway.dispatches).toHaveLength(count + 1);
    const refresh = lastDispatch(gateway).body;
    if (refresh.command !== "session.read") throw new Error("没有自动核实审批请求");
    expect(refresh.payload.include).toEqual(["pending_interactions"]);
    store.apply(request);
    expect(gateway.dispatches).toHaveLength(count + 1);
  });

  it("事件视图没有请求版本：自动读取有效请求，确认前禁止审批，取得版本后可提交", () => {
    const { store, gateway } = negotiatedStore();
    store.apply(
      versionedPermission({ eventId: "dddddddd-0000-4000-8000-0000000000e2", globalSequence: "42" }),
    );

    const view = interactionViewOf(store, PERMISSION_ID);
    expect(view?.availability).toEqual({ kind: "not_answerable", reason: "request_version_unknown" });

    const dispatches = gateway.dispatches.length;
    const rejected = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(rejected.ok).toBe(false);
    if (!rejected.ok) expect(rejected.reason).toContain("请求版本");
    // 反例：缺版本仍派发 → 这里会多一条记录（服务端必然拒绝它）。
    expect(gateway.dispatches).toHaveLength(dispatches);

    const refresh = lastDispatch(gateway).body;
    expect(refresh.command).toBe("session.read");
    if (refresh.command !== "session.read") throw new Error("没有自动读取有效请求");
    expect(refresh.payload.include).toEqual(["pending_interactions"]);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: refresh.requestId,
        sessionId: ONLINE_SESSION_ID,
        pending: [
          {
            interactionId: PERMISSION_ID,
            sessionId: ONLINE_SESSION_ID,
            kind: "permission",
            requestVersion: "8",
            expiresAt: null,
            options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
            schema: null,
          },
        ],
      }),
    });

    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");
    expect(store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once").ok).toBe(true);
    const dispatched = lastDispatch(gateway).body;
    if (dispatched.command !== "permission.resolve") throw new Error("命令不是 permission.resolve");
    expect(dispatched.payload).toEqual({
      interactionId: PERMISSION_ID,
      optionId: "allow-once",
      requestVersion: "8",
    });
  });

  it("提问请求的两种形状：带版本可应答并回传，缺版本拒绝且不发命令", () => {
    const { store, gateway } = negotiatedStore();
    const schema = { type: "object", properties: { branch: { type: "string" } } };
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "elicitation.requested",
        eventId: "dddddddd-0000-4000-8000-0000000000e3",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "43",
        view: { interactionId: ELICITATION_ID, turnId: "turn-1", title: "请补一个分支名", schema, initialValues: null },
      }),
    });

    expect(interactionViewOf(store, ELICITATION_ID)?.availability).toEqual({
      kind: "not_answerable",
      reason: "request_version_unknown",
    });
    const dispatches = gateway.dispatches.length;
    const rejected = store.respondToElicitation(ONLINE_SESSION_ID, ELICITATION_ID, "decline", null);
    expect(rejected.ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(dispatches);

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "elicitation.requested",
        eventId: "dddddddd-0000-4000-8000-0000000000e4",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "44",
        view: {
          interactionId: ELICITATION_ID,
          turnId: "turn-1",
          title: "请补一个分支名",
          schema,
          initialValues: null,
          requestVersion: "9",
        },
      }),
    });

    expect(interactionViewOf(store, ELICITATION_ID)?.availability.kind).toBe("answerable");
    expect(store.respondToElicitation(ONLINE_SESSION_ID, ELICITATION_ID, "decline", null).ok).toBe(true);
    const dispatched = lastDispatch(gateway).body;
    if (dispatched.command !== "elicitation.respond") throw new Error("命令不是 elicitation.respond");
    expect(dispatched.payload).toEqual({
      interactionId: ELICITATION_ID,
      action: "decline",
      values: null,
      requestVersion: "9",
    });
  });
});
