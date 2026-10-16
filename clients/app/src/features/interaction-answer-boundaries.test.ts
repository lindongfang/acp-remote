/**
 * D7 授权与提问应答的**四条边界**（spec「授权与提问应答的可用边界」）。
 *
 * 四条边界各有一条实测：
 *
 * 1. **亲历的请求可应答**：本设备连接期间收到 `permission.requested` → 可选项来自该请求自身，
 *    提交后派发 `permission.resolve`，呈现先变 `submitting`（待确认）再变 `handled_here`。
 * 2. **仅历史读到的待处理项不可操作**：`session.read` 读回的 `pending_interactions` 没有可选项
 *    → `not_answerable(read_from_history)`，且 `resolvePermission` **不派发任何命令**。
 * 3. **已被其它设备处理**：`permission.resolved` 的 `resolvedByDeviceId` 不是本设备
 *    → `handled_elsewhere`，重复提交被拒绝（`interactions` 不可重复提交，spec MUST NOT）。
 * 4. **会话已关闭**：即便请求是亲历的，也不能再应答。
 *
 * 另有一条对照证明「本设备提交并获确认」呈现为 `handled_here`（否则上面第 1 条只是「永远
 * 不确认」的平凡通过）。
 */

import { describe, expect, it } from "vitest";

import {
  ONLINE_SESSION_ID,
  eventMessage,
  interactionStore,
  interactionViewOf,
  lastDispatch,
  readResult,
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
    if (!outcome.ok) expect(outcome.reason).toContain("没有收到过它的请求内容");
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

describe("D7 边界四：会话已关闭", () => {
  it("会话关闭后亲历的请求也不能再应答", () => {
    const { store, gateway } = interactionStore();
    store.apply(permissionRequested({ eventId: "dddddddd-0000-4000-8000-0000000000c8", globalSequence: "18" }));
    expect(interactionViewOf(store, PERMISSION_ID)?.availability.kind).toBe("answerable");

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        eventId: "dddddddd-0000-4000-8000-0000000000c9",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "19",
        view: {
          session: {
            sessionId: ONLINE_SESSION_ID,
            title: null,
            agent: { agentId: "claude", name: "Claude" },
            state: "closed",
            origin: { kind: "local" },
            currentMode: null,
            version: "2",
            createdAt: "2026-10-01T00:00:00.000Z",
            updatedAt: "2026-10-01T00:00:05.000Z",
          },
        },
      }),
    });

    expect(interactionViewOf(store, PERMISSION_ID)?.availability).toEqual({
      kind: "not_answerable",
      reason: "session_closed",
    });
    const dispatches = gateway.dispatches.length;
    const outcome = store.resolvePermission(ONLINE_SESSION_ID, PERMISSION_ID, "allow-once");
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.reason).toContain("会话已关闭");
    expect(gateway.dispatches).toHaveLength(dispatches);
  });

  it("会话关闭后也不接受文本提交（输入区的第三种状态）", () => {
    const { store, gateway } = interactionStore();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        eventId: "dddddddd-0000-4000-8000-0000000000ca",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "20",
        view: {
          session: {
            sessionId: ONLINE_SESSION_ID,
            title: null,
            agent: { agentId: "claude", name: "Claude" },
            state: "closed",
            origin: { kind: "local" },
            currentMode: null,
            version: "3",
            createdAt: "2026-10-01T00:00:00.000Z",
            updatedAt: "2026-10-01T00:00:06.000Z",
          },
        },
      }),
    });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.input.state).toBe("closed");
    expect(page?.input.canSubmit).toBe(false);
    const dispatches = gateway.dispatches.length;
    expect(store.submitPrompt(ONLINE_SESSION_ID, "还能发吗").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(dispatches);
  });
});
