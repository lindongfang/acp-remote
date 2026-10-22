/**
 * 本机会话生命周期（`core.local-session-lifecycle.v1`）的 feature 层判定。
 *
 * 覆盖的不可让步行为：
 * - 未协商 feature 时不发任何查看/恢复帧，并明确显示「自动激活不可用」；
 * - 进入会话时**先**声明查看（版本单调）**再**派发 `session.resume`（payload `{}`）；
 * - 恢复中历史可读、发送禁用、草稿保留；完成不自动提交、失败保留原因；
 * - 隐藏/离页立即停续租并声明 `null`，返回可见立即重新声明与激活；
 * - 迟到的旧连接/旧版本响应不得切回旧页、不得续租旧 viewer；
 * - `uncertain` 的显式重试先查 `command.status`，不自动换 `requestId` 重派。
 */

import { describe, expect, it } from "vitest";

import type { CommandMessage, CommandResultMessage, ErrorCode } from "../protocol";
import { LOCAL_SESSION_LIFECYCLE_FEATURE } from "../sync-client";
import type { ClientStore } from "./client-store";
import {
  ONLINE_SESSION_ID,
  TEST_DEVICE_ID,
  TEST_SERVER_EPOCH,
  onlineStore,
  resources,
  session,
} from "./testing/fixtures";

const SESSION_B = "bbbbbbbb-0000-4000-8000-000000000002";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";

/** 协商了本机生命周期、两条本机会话的起点。 */
function lifecycleStore(input: { readonly schedule?: (delayMs: number, task: () => void) => () => void } = {}) {
  return onlineStore({
    resources: resources({
      sessions: [
        session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" }),
        session({ id: SESSION_B, state: "idle", localActivity: "active" }),
      ],
    }),
    features: [LOCAL_SESSION_LIFECYCLE_FEATURE],
    ...(input.schedule === undefined ? {} : { schedule: input.schedule }),
  });
}

/** 一条 `session.resume` 的命令结果。 */
function resumeResult(input: {
  readonly requestId: string;
  readonly sessionId: string;
  readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
  readonly code?: ErrorCode;
  readonly connectionId?: string;
}): CommandResultMessage {
  const error =
    input.code === undefined
      ? null
      : { code: input.code, message: "假主机的明确错误", retryable: true, details: {} };
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-0000000000ff",
    connectionId: input.connectionId ?? CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.resume",
      status: input.status,
      acceptedAt: input.status === "rejected" ? null : "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result:
        input.status === "completed"
          ? {
              sessionId: input.sessionId,
              session: session({ id: input.sessionId, state: "idle", localActivity: "active" }),
            }
          : null,
      error: input.status === "rejected" ? (error ?? {
        code: "session.not_found",
        message: "会话不存在",
        retryable: false,
        details: {},
      }) : error,
    },
  };
}

function resumeRequests(gateway: { readonly dispatches: readonly CommandMessage[] }): readonly CommandMessage[] {
  return gateway.dispatches.filter((command) => command.body.command === "session.resume");
}

/** 某一条会话的 `session.resume` 请求（A/B 快切与重连用例只关心这一条会话）。 */
function resumeRequestsOf(
  gateway: { readonly dispatches: readonly CommandMessage[] },
  sessionId: string,
): readonly CommandMessage[] {
  return resumeRequests(gateway).filter(
    (command) => command.body.command === "session.resume" && command.body.sessionId === sessionId,
  );
}

/** 走一次完整的重连（新连接代）：连接尝试 → 认证 → 重放 → 追平。 */
function reconnect(store: ClientStore): void {
  store.apply({ kind: "authenticating" });
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: TEST_DEVICE_ID,
      serverEpoch: TEST_SERVER_EPOCH,
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: [LOCAL_SESSION_LIFECYCLE_FEATURE],
  });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: TEST_SERVER_EPOCH, globalSequence: "10" } });
}

/** 一条 `command.status` 的 completed 结果（核实一次先前派发的 `session.resume`）。 */
function statusResult(input: {
  readonly requestId: string;
  readonly targetRequestId: string;
  readonly state: "accepted" | "completed" | "failed" | "uncertain";
  readonly result?: Record<string, unknown> | null;
}): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-2222-4222-8222-000000000002",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "command.status",
      status: "completed",
      acceptedAt: null,
      terminalEventId: null,
      result: {
        targetRequestId: input.targetRequestId,
        state: input.state,
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalAt: null,
        terminalEventId: null,
        result: input.result ?? null,
        error: null,
      },
      error: null,
    },
  };
}


describe("本机会话生命周期：声明、激活与恢复态", () => {
  it("进入会话先声明查看再派发 resume；恢复中不可发送，完成不自动提交草稿", () => {
    const { store, gateway } = lifecycleStore();
    store.setDraft(ONLINE_SESSION_ID, "进入前就在的草稿");
    store.setActiveConversationSession(ONLINE_SESSION_ID);

    expect(gateway.views).toEqual([{ sessionId: ONLINE_SESSION_ID, viewVersion: 1 }]);
    const requests = resumeRequests(gateway);
    expect(requests).toHaveLength(1);
    if (requests[0]?.body.command !== "session.resume") throw new Error("不是 session.resume");
    expect(requests[0].body.payload).toEqual({});
    expect(requests[0].body.sessionId).toBe(ONLINE_SESSION_ID);

    const activating = store.conversationPage(ONLINE_SESSION_ID);
    expect(activating?.header.activation?.kind).toBe("activating");
    expect(activating?.input.canSubmit).toBe(false);
    expect(activating?.input.draft).toBe("进入前就在的草稿");
    expect(activating?.capabilities.canSend).toBe(false);

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: requests[0].body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "completed",
      }),
    });
    const ready = store.conversationPage(ONLINE_SESSION_ID);
    expect(ready?.header.activation?.kind).toBe("ready");
    expect(ready?.input.canSubmit).toBe(true);
    expect(ready?.input.draft).toBe("进入前就在的草稿");
    expect(ready?.header.localActivity).toBe("active");
  });

  it("未协商 feature：不发查看/恢复帧，明确显示自动激活不可用，旧合同发送判定不变", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }),
      features: [],
    });
    store.setActiveConversationSession(ONLINE_SESSION_ID);

    expect(gateway.views).toEqual([]);
    expect(resumeRequests(gateway)).toEqual([]);
    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.header.activation?.kind).toBe("unavailable");
    expect(page?.header.localActivity).toBeNull();
    expect(page?.input.canSubmit).toBe(true);
  });

  it("隐藏立即声明 null 并停续租；返回可见重新声明并再次激活", () => {
    const timers: (() => void)[] = [];
    const { store, gateway } = lifecycleStore({
      schedule: (_delayMs, task) => {
        timers.push(task);
        return () => {
          const index = timers.indexOf(task);
          if (index >= 0) timers.splice(index, 1);
        };
      },
    });
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    expect(gateway.views.at(-1)).toEqual({ sessionId: ONLINE_SESSION_ID, viewVersion: 1 });
    expect(timers).toHaveLength(1);

    // 可见阅读续租：同一会话、递增版本。
    timers.shift()?.();
    expect(gateway.views.at(-1)).toEqual({ sessionId: ONLINE_SESSION_ID, viewVersion: 2 });

    store.applyLifecycle({ kind: "visibility", state: "hidden" });
    expect(gateway.views.at(-1)).toEqual({ sessionId: null, viewVersion: 3 });
    expect(timers).toHaveLength(0);

    store.applyLifecycle({ kind: "visibility", state: "visible" });
    expect(gateway.views.at(-1)).toEqual({ sessionId: ONLINE_SESSION_ID, viewVersion: 4 });
    expect(resumeRequests(gateway)).toHaveLength(2);
  });

  it("A/B 快切后，迟到的 A 恢复不改当前选择、不续租旧 viewer", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    store.setActiveConversationSession(SESSION_B);
    const requests = resumeRequests(gateway);
    expect(requests).toHaveLength(2);
    const first = requests[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "completed",
      }),
    });

    expect(store.state.lifecycle.selectedSessionId).toBe(SESSION_B);
    expect(gateway.views.at(-1)?.sessionId).toBe(SESSION_B);
    expect(store.conversationPage(SESSION_B)?.header.sessionId).toBe(SESSION_B);

    // 旧版本 accepted 也不得复活 A 的 viewer。
    const current = gateway.views.at(-1);
    if (current === undefined || current.sessionId === null) throw new Error("缺少 B 的声明");
    store.apply({
      kind: "session_view_result",
      result: {
        protocolVersion: 1,
        type: "session.view.result",
        messageId: "cccccccc-1111-4111-8111-000000000001",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          sessionId: ONLINE_SESSION_ID,
          viewVersion: current.viewVersion,
          status: "accepted",
          leaseExpiresAt: "2026-10-01T00:00:31.000Z",
        },
      },
    });
    expect(store.state.lifecycle.view?.sessionId).toBe(SESSION_B);
    expect(store.state.lifecycle.leaseExpiresAt).toBeNull();
  });

  it("失败可区分：容量已满保留草稿并给出原因；uncertain 的重试先查原请求", () => {
    const { store, gateway } = lifecycleStore();
    store.setDraft(ONLINE_SESSION_ID, "别丢");
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    const first = resumeRequests(gateway)[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "failed",
        code: "session.capacity_exceeded",
      }),
    });
    const failed = store.conversationPage(ONLINE_SESSION_ID);
    expect(failed?.header.activation?.kind).toBe("failed");
    expect(failed?.input.canSubmit).toBe(false);
    expect(failed?.input.draft).toBe("别丢");
    expect(failed?.input.disabledReason ?? "").toContain("名额");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "uncertain",
      }),
    });
    const uncertain = store.conversationPage(ONLINE_SESSION_ID);
    expect(uncertain?.header.activation).toMatchObject({ kind: "failed", code: null });
    expect(uncertain?.input.disabledReason ?? "").toContain("不确定");

    const outcome = store.retrySessionActivation(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const statusQuery = gateway.dispatches.filter((command) => command.body.command === "command.status").at(-1);
    if (statusQuery?.body.command !== "command.status") throw new Error("没有发出 command.status");
    expect(statusQuery.body.payload).toEqual({ targetRequestId: first.body.requestId });
  });

  it("旧连接上的 resume 终态被丢弃（不把新连接的状态改成 ready）", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    const first = resumeRequests(gateway)[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("activating");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "completed",
        connectionId: "cccccccc-0000-4000-8000-000000000999",
      }),
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("activating");
  });

  it("uncertain 不自动换号重派：返回前台、重新进入与重连都保留原请求；显式重试先核实", () => {
    const { store, gateway } = lifecycleStore();
    store.setDraft(ONLINE_SESSION_ID, "结果未知也保留草稿");
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    const first = resumeRequestsOf(gateway, ONLINE_SESSION_ID)[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "uncertain",
      }),
    });
    const uncertain = store.conversationPage(ONLINE_SESSION_ID);
    expect(uncertain?.header.activation).toMatchObject({ kind: "failed", code: null, retryable: true });
    expect(uncertain?.input.draft).toBe("结果未知也保留草稿");
    expect(uncertain?.input.canSubmit).toBe(false);

    // 隐藏 → 返回可见：只重新声明查看，不重派 resume。
    store.applyLifecycle({ kind: "visibility", state: "hidden" });
    store.applyLifecycle({ kind: "visibility", state: "visible" });
    // 重新进入（A → B → A）：B 有自己的尝试，A 仍是原请求。
    store.setActiveConversationSession(SESSION_B);
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    // 断线重连（新连接代）：同样不换号。
    store.apply({ kind: "closed", code: 1006, retryable: true });
    reconnect(store);

    expect(resumeRequestsOf(gateway, ONLINE_SESSION_ID)).toHaveLength(1);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation).toMatchObject({
      kind: "failed",
      retryable: true,
    });

    // 显式重试：只核实原请求（同一 targetRequestId），不派发新的 session.resume。
    const outcome = store.retrySessionActivation(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const statusQuery = gateway.dispatches.filter((command) => command.body.command === "command.status").at(-1);
    if (statusQuery?.body.command !== "command.status") throw new Error("没有发出 command.status");
    expect(statusQuery.body.payload).toEqual({ targetRequestId: first.body.requestId });
    expect(resumeRequestsOf(gateway, ONLINE_SESSION_ID)).toHaveLength(1);

    // 核实到原请求确实完成（形状合规）才打开发送门。
    store.apply({
      kind: "command_result",
      result: statusResult({
        requestId: statusQuery.body.requestId,
        targetRequestId: first.body.requestId,
        state: "completed",
        result: {
          sessionId: ONLINE_SESSION_ID,
          session: session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" }),
        },
      }),
    });
    const ready = store.conversationPage(ONLINE_SESSION_ID);
    expect(ready?.header.activation?.kind).toBe("ready");
    expect(ready?.input.canSubmit).toBe(true);
    expect(ready?.input.draft).toBe("结果未知也保留草稿");
  });

  it("连接断开终结在途激活；重连后按新连接代重新申请，旧终态不能把它改成 ready", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    const first = resumeRequestsOf(gateway, ONLINE_SESSION_ID)[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");
    expect(store.state.lifecycle.activations.get(ONLINE_SESSION_ID)?.phase).toBe("activating");

    // 连接消失后旧连接上的终态永远不会到达：不能停在「正在恢复」。
    store.apply({ kind: "closed", code: 1006, retryable: true });
    expect(store.state.lifecycle.activations.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("inactive");

    reconnect(store);
    const requests = resumeRequestsOf(gateway, ONLINE_SESSION_ID);
    expect(requests).toHaveLength(2);
    const second = requests[1];
    if (second?.body.command !== "session.resume") throw new Error("不是 session.resume");
    expect(second.body.requestId).not.toBe(first.body.requestId);
    expect(store.state.lifecycle.activations.get(ONLINE_SESSION_ID)).toMatchObject({
      phase: "activating",
      requestId: second.body.requestId,
      connectionGeneration: store.state.lifecycle.generation,
    });

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "completed",
      }),
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("activating");

    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: second.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "completed",
      }),
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("ready");
  });

  it("command.status 的 completed 结果形状不符不得标记 ready（与 command.result 同判据）", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    const first = resumeRequestsOf(gateway, ONLINE_SESSION_ID)[0];
    if (first?.body.command !== "session.resume") throw new Error("不是 session.resume");
    store.apply({
      kind: "command_result",
      result: resumeResult({
        requestId: first.body.requestId,
        sessionId: ONLINE_SESSION_ID,
        status: "uncertain",
      }),
    });

    const outcome = store.retrySessionActivation(ONLINE_SESSION_ID);
    expect(outcome.ok).toBe(true);
    const statusQuery = gateway.dispatches.filter((command) => command.body.command === "command.status").at(-1);
    if (statusQuery?.body.command !== "command.status") throw new Error("没有发出 command.status");

    // 完成结果指向别的会话：不得当成这次尝试成功。
    store.apply({
      kind: "command_result",
      result: statusResult({
        requestId: statusQuery.body.requestId,
        targetRequestId: first.body.requestId,
        state: "completed",
        result: {
          sessionId: SESSION_B,
          session: session({ id: SESSION_B, state: "idle", localActivity: "active" }),
        },
      }),
    });
    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.header.activation?.kind).toBe("failed");
    expect(page?.input.canSubmit).toBe(false);
    expect(store.state.lifecycle.activations.get(ONLINE_SESSION_ID)?.failure?.message ?? "").toContain("形状不符");
  });
});
