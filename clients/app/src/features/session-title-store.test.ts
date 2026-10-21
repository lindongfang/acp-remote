/**
 * 会话标题的仓库接线（`session.title.set`，`design.md` D3/D4）。
 *
 * ## 判别力
 *
 * - **只双击进入**：未调用 `beginTitleEdit` 时 `titleEditorView` 永远是 `null`，调用前不派发任何命令；
 * - **一次提交一个 requestId**：Enter 紧随失焦只有一条 `session.title.set`，且 `payload` 是
 *   trim 后的 `{ title }`、body 上没有 `expectedVersion`；
 * - **accepted 不是完成**：只有 `completed` + 权威摘要才退出编辑；
 * - **归属原会话**：迟到的失败不改新会话编辑器，离页只留可归因提示；
 * - **不重发**：重连后用原 `requestId` 发 `command.status`，不产生第二条标题命令。
 */

import { describe, expect, it } from "vitest";

import type { CommandResultStatus, PublicError, SessionSummary } from "../protocol";
import type { SyncClientEvent } from "../sync-client";
import { SESSION_TITLE_FEATURE } from "../sync-client";
import { ClientStore } from "./client-store";
import type { StoreFixture } from "./testing/fixtures";
import {
  TEST_DEVICE_ID,
  TEST_SERVER_EPOCH,
  eventMessage,
  lastDispatch,
  onlineStore,
  resources,
  session,
} from "./testing/fixtures";

const TITLE_SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";
const OTHER_SESSION_ID = "bbbbbbbb-0000-4000-8000-000000000002";
const OWNER_TERMINAL_EVENT_ID = "99999999-0000-4000-8000-000000000099";

function titleStore(input: {
  readonly title?: string | null;
  readonly features?: readonly string[];
} = {}): StoreFixture {
  return onlineStore({
    resources: resources({
      sessions: [
        session({ id: TITLE_SESSION_ID, title: input.title === undefined ? "Agent 标题" : input.title, state: "idle" }),
        session({ id: OTHER_SESSION_ID, title: "另一个会话", state: "idle" }),
      ],
    }),
    features: input.features ?? [SESSION_TITLE_FEATURE],
  });
}

function titleResult(input: {
  readonly requestId: string;
  readonly status: CommandResultStatus;
  readonly error?: PublicError | null;
}): SyncClientEvent {
  const accepted = input.status === "accepted" || input.status === "completed";
  return {
    kind: "command_result",
    result: {
      protocolVersion: 1,
      type: "command.result",
      messageId: "bbbbbbbb-2222-4222-8222-000000000001",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
      connectionSequence: "1",
      body: {
        requestId: input.requestId,
        command: "session.title.set",
        status: input.status,
        acceptedAt: accepted ? "2026-10-01T00:00:01.000Z" : null,
        // Owner 的事件标识（远程会话尤其如此）；客户端不得拿它去本地事件表里找。
        terminalEventId: input.status === "completed" ? OWNER_TERMINAL_EVENT_ID : null,
        result: null,
        error: input.error ?? null,
      },
    },
  };
}

function statusResult(input: {
  readonly requestId: string;
  readonly targetRequestId: string;
  readonly state: "completed" | "failed" | "uncertain";
  readonly error?: PublicError | null;
}): SyncClientEvent {
  return {
    kind: "command_result",
    result: {
      protocolVersion: 1,
      type: "command.result",
      messageId: "bbbbbbbb-2222-4222-8222-000000000002",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
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
          terminalAt: input.state === "uncertain" ? null : "2026-10-01T00:00:02.000Z",
          terminalEventId: input.state === "completed" ? OWNER_TERMINAL_EVENT_ID : null,
          result: null,
          error: input.error ?? null,
        },
        error: null,
      },
    },
  };
}

function sessionUpdated(input: {
  readonly title: string | null;
  readonly version?: string;
  readonly eventId?: string;
}): SyncClientEvent {
  const summary = session({
    id: TITLE_SESSION_ID,
    title: input.title,
    state: "idle",
    updatedAt: "2026-10-01T00:00:03.000Z",
  }) satisfies SessionSummary;
  return {
    kind: "event",
    event: eventMessage({
      eventType: "session.updated",
      eventId: input.eventId ?? "dddddddd-0000-4000-8000-0000000000aa",
      sessionId: TITLE_SESSION_ID,
      globalSequence: "20",
      view: { session: { ...summary, version: input.version ?? "2" } },
    }),
  };
}

function titleRequestId(store: ClientStore): string {
  const requestId = store.state.titleEditor.requestId;
  if (requestId === null) throw new Error("没有在途的标题请求");
  return requestId;
}

describe("双击进入与预填", () => {
  it("未双击前不呈现编辑器、不派发命令；双击后旧标题作为灰色占位、输入框为空", () => {
    const { store, gateway } = titleStore();
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();

    store.beginTitleEdit(TITLE_SESSION_ID);
    const view = store.titleEditorView(TITLE_SESSION_ID);
    expect(view?.phase).toBe("editing");
    expect(view?.draft).toBe("");
    expect(view?.placeholder).toBe("Agent 标题");
    expect(gateway.dispatches).toHaveLength(0);
  });

  it("未命名会话：编辑框为空，不输入直接确认只保留未命名、不派发命令", () => {
    const { store, gateway } = titleStore({ title: null });
    expect(store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("未命名会话");

    store.beginTitleEdit(TITLE_SESSION_ID);
    expect(store.titleEditorView(TITLE_SESSION_ID)?.draft).toBe("");

    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
    expect(gateway.dispatches).toHaveLength(0);
  });
});

describe("Enter 与失焦共用一次提交", () => {
  it("只派发一条 session.title.set，payload 是 trim 后的 title 且没有 expectedVersion", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "  排查登录失败  ");
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    store.commitTitleEdit(TITLE_SESSION_ID, "blur");

    expect(gateway.dispatches).toHaveLength(1);
    const body = lastDispatch(gateway).body;
    expect(body.command).toBe("session.title.set");
    if (body.command !== "session.title.set") throw new Error("命令不是 session.title.set");
    expect(body.sessionId).toBe(TITLE_SESSION_ID);
    expect(body.payload).toEqual({ title: "排查登录失败" });
    expect(Object.keys(body).sort()).toEqual(["command", "payload", "requestId", "sessionId"]);
    expect(store.state.titleEditor.phase).toBe("submitting");
    expect(store.state.titleEditor.requestId).toBe(body.requestId);
    expect(store.titleEditorView(TITLE_SESSION_ID)?.submitting).toBe(true);
  });

  it("Escape 取消后紧随的失焦不提交（取消后的 blur 不保存）", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "取消的标题");
    store.cancelTitleEdit(TITLE_SESSION_ID);
    store.commitTitleEdit(TITLE_SESSION_ID, "blur");
    expect(gateway.dispatches).toHaveLength(0);
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
    expect(store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("Agent 标题");
  });

  it("非法输入（超长）不派发命令并保留输入与原因", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "x".repeat(513));
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    expect(gateway.dispatches).toHaveLength(0);
    const view = store.titleEditorView(TITLE_SESSION_ID);
    expect(view?.phase).toBe("editing-error");
    expect(view?.draft).toHaveLength(513);
    expect(view?.error).toContain("512");
  });

  it("同值错误后的重复失焦不再派发；改了文字才允许新的提交", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "失败标题");
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    const firstRequestId = titleRequestId(store);
    store.apply(titleResult({
      requestId: firstRequestId,
      status: "failed",
      error: { code: "internal.unavailable", message: "写入失败", retryable: false, details: {} },
    }));

    const view = store.titleEditorView(TITLE_SESSION_ID);
    expect(view?.phase).toBe("editing-error");
    expect(view?.draft).toBe("失败标题");
    expect(view?.error).toContain("写入失败");

    store.commitTitleEdit(TITLE_SESSION_ID, "blur");
    store.commitTitleEdit(TITLE_SESSION_ID, "navigation");
    // 反例：重复失焦又发一条命令 → 这里会变成 2。
    expect(gateway.dispatches).toHaveLength(1);

    store.setTitleDraft(TITLE_SESSION_ID, "重试标题");
    store.commitTitleEdit(TITLE_SESSION_ID, "blur");
    expect(gateway.dispatches).toHaveLength(2);
    expect(store.state.titleEditor.requestId).not.toBe(firstRequestId);
  });
});

describe("终态与权威摘要", () => {
  it("accepted 不退出编辑；completed 且摘要已反映保存值后才退出", () => {
    const { store } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "新标题");
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    const requestId = titleRequestId(store);

    store.apply(titleResult({ requestId, status: "accepted" }));
    expect(store.titleEditorView(TITLE_SESSION_ID)?.phase).toBe("submitting");

    store.apply(titleResult({ requestId, status: "completed" }));
    // 摘要仍是旧标题：终态到了也不能显示完成，必须等权威摘要。
    expect(store.titleEditorView(TITLE_SESSION_ID)?.phase).toBe("submitting");

    store.apply(sessionUpdated({ title: "新标题" }));
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
    expect(store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("新标题");
  });

  it("摘要先到、终态后到：终态一确认就退出编辑", () => {
    const { store } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "先到标题");
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    const requestId = titleRequestId(store);

    store.apply(sessionUpdated({ title: "先到标题" }));
    expect(store.titleEditorView(TITLE_SESSION_ID)?.phase).toBe("submitting");

    store.apply(titleResult({ requestId, status: "completed" }));
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
  });

  it("其他设备的 session.updated 不覆盖编辑中的文字；非编辑态由有效标题驱动三处文本", () => {
    const { store } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "我的草稿");
    store.apply(sessionUpdated({ title: "别的设备标题" }));

    expect(store.titleEditorView(TITLE_SESSION_ID)?.draft).toBe("我的草稿");
    // 非编辑状态（头部/列表/搜索）都只读同一个有效标题。
    expect(store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("别的设备标题");
    const sidebarRow = store.conversationPage(TITLE_SESSION_ID)?.sidebar.rows.find((row) => row.id === TITLE_SESSION_ID);
    expect(sidebarRow?.title).toBe("别的设备标题");
    const directoryRows = store.directoryPageSessions().get("work-api") ?? [];
    expect(directoryRows.find((row) => row.id === TITLE_SESSION_ID)?.title).toBe("别的设备标题");
    store.setDirectoryQuery("别的设备标题");
    expect(store.directoryPage().kind).toBe("ready");

    store.cancelTitleEdit(TITLE_SESSION_ID);
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
    expect(store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("别的设备标题");
  });
});

describe("离页、迟到结果与刷新", () => {
  it("切换会话：请求绑定原会话；失败只留可归因提示，不覆盖新会话编辑器", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "离页标题");
    store.leaveConversationSession(TITLE_SESSION_ID);

    // 先尝试提交、再放弃输入，命令仍绑定原会话。
    expect(gateway.dispatches).toHaveLength(1);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.title.set") throw new Error("命令不是 session.title.set");
    expect(body.sessionId).toBe(TITLE_SESSION_ID);
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();

    store.setActiveConversationSession(OTHER_SESSION_ID);
    store.beginTitleEdit(OTHER_SESSION_ID);
    store.setTitleDraft(OTHER_SESSION_ID, "B 会话草稿");

    store.apply(titleResult({
      requestId: body.requestId,
      status: "failed",
      error: { code: "internal.unavailable", message: "原会话写入失败", retryable: false, details: {} },
    }));

    // 迟到失败不覆盖当前编辑器。
    const current = store.titleEditorView(OTHER_SESSION_ID);
    expect(current?.draft).toBe("B 会话草稿");
    expect(current?.error).toBeNull();
    // 原会话只留下可归因的错误提示；没有草稿、没有强制跳回。
    const original = store.titleEditorView(TITLE_SESSION_ID);
    expect(original?.phase).toBe("viewing");
    expect(original?.notice).toContain("原会话写入失败");

    // 切回原会话：从当前有效标题重新开始，不恢复旧草稿。
    store.cancelTitleEdit(OTHER_SESSION_ID);
    store.beginTitleEdit(TITLE_SESSION_ID);
    expect(store.titleEditorView(TITLE_SESSION_ID)?.draft).toBe("");
    expect(store.titleEditorView(TITLE_SESSION_ID)?.placeholder).toBe("Agent 标题");
  });

  it("离线/未协商：双击只说明不可用，不进入可保存状态、不显示本地成功", () => {
    const unsupported = titleStore({ features: [] });
    unsupported.store.beginTitleEdit(TITLE_SESSION_ID);
    const unsupportedView = unsupported.store.titleEditorView(TITLE_SESSION_ID);
    expect(unsupportedView?.phase).toBe("viewing");
    expect(unsupportedView?.notice).toContain("core.session-title.v1");
    expect(unsupportedView?.submitting).toBe(false);
    expect(unsupported.gateway.dispatches).toHaveLength(0);

    const offline = titleStore();
    offline.store.apply({ kind: "closed", code: 1006, retryable: true });
    offline.store.beginTitleEdit(TITLE_SESSION_ID);
    const offlineView = offline.store.titleEditorView(TITLE_SESSION_ID);
    expect(offlineView?.phase).toBe("viewing");
    expect(offlineView?.notice).toContain("离线");
    expect(offline.gateway.dispatches).toHaveLength(0);
  });

  it("刷新（重建仓库）只读当前标题，不恢复编辑进度、不自动补发", () => {
    const before = titleStore();
    before.store.beginTitleEdit(TITLE_SESSION_ID);
    before.store.setTitleDraft(TITLE_SESSION_ID, "未提交草稿");

    const after = titleStore();
    expect(after.store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
    expect(after.store.conversationPage(TITLE_SESSION_ID)?.header.title).toBe("Agent 标题");
    expect(after.gateway.dispatches).toHaveLength(0);
  });

  it("已提交但结果未知：重连后用原 requestId 走 command.status，不换号重发", () => {
    const { store, gateway } = titleStore();
    store.beginTitleEdit(TITLE_SESSION_ID);
    store.setTitleDraft(TITLE_SESSION_ID, "重连标题");
    store.commitTitleEdit(TITLE_SESSION_ID, "enter");
    const first = lastDispatch(gateway).body;
    if (first.command !== "session.title.set") throw new Error("命令不是 session.title.set");

    store.apply({ kind: "closed", code: 1006, retryable: true });
    // 提交中断线：仍保留输入并明确「结果未知」，不冒充失败或成功。
    const interrupted = store.titleEditorView(TITLE_SESSION_ID);
    expect(interrupted?.phase).toBe("editing-error");
    expect(interrupted?.draft).toBe("重连标题");
    expect(interrupted?.error).toContain("结果未知");

    store.apply({
      kind: "authenticated",
      info: {
        deviceId: TEST_DEVICE_ID,
        serverEpoch: TEST_SERVER_EPOCH,
        headGlobalSequence: "10",
        heartbeatIntervalMs: 30000,
        limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
      },
      selectedFeatures: [SESSION_TITLE_FEATURE],
    });

    const status = lastDispatch(gateway).body;
    expect(status.command).toBe("command.status");
    if (status.command !== "command.status") throw new Error("命令不是 command.status");
    expect(status.payload.targetRequestId).toBe(first.requestId);
    // 没有第二条标题命令（不换号重发）。
    expect(gateway.dispatches.filter((command) => command.body.command === "session.title.set")).toHaveLength(1);

    store.apply(statusResult({
      requestId: status.requestId,
      targetRequestId: first.requestId,
      state: "completed",
    }));
    // 终态已确认、摘要未到：仍不退出编辑（也不显示完成）。
    expect(store.titleEditorView(TITLE_SESSION_ID)?.phase).toBe("editing-error");
    store.apply(sessionUpdated({ title: "重连标题" }));
    expect(store.titleEditorView(TITLE_SESSION_ID)).toBeNull();
  });
});
