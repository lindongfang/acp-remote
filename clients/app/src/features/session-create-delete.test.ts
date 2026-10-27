import { describe, expect, it } from "vitest";

import type { CommandResultMessage } from "../protocol";
import { ATTACHMENT_FEATURE, LOCAL_SESSION_LIFECYCLE_FEATURE } from "../sync-client";
import type {
  AttachmentFileHandle,
  AttachmentMediaPort,
  AttachmentTransferGateway,
} from "./ports";
import {
  ONLINE_SESSION_ID,
  TEST_DEVICE_ID,
  TEST_SERVER_EPOCH,
  eventMessage,
  lastDispatch,
  onlineStore,
  readResult,
  resources,
  session,
} from "./testing/fixtures";

const SESSION_B = "bbbbbbbb-0000-4000-8000-000000000002";
const CREATED_SESSION_ID = "cccccccc-0000-4000-8000-000000000003";
const DIRECTORY_ID = "work-api";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";

function createResult(
  requestId: string,
  status: CommandResultMessage["body"]["status"],
  sessionId = CREATED_SESSION_ID,
  error: CommandResultMessage["body"]["error"] = null,
): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      requestId,
      command: "session.create",
      status,
      acceptedAt: status === "rejected" ? null : "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result:
        status === "completed"
          ? {
              sessionId,
              session: session({
                id: sessionId,
                directoryId: DIRECTORY_ID,
                state: "idle",
                localActivity: "active",
                title: "新建会话",
              }),
            }
          : null,
      error,
    },
  };
}

function deletedEvent(eventId: string, sessionId: string, sequence: string) {
  return eventMessage({
    eventType: "session.deleted",
    eventId,
    sessionId,
    globalSequence: sequence,
    view: { sessionId },
  });
}

function reconnect(store: ReturnType<typeof onlineStore>["store"]): void {
  store.apply({ kind: "closed", code: 1006, retryable: true });
  store.apply({ kind: "authenticating" });
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: TEST_DEVICE_ID,
      serverEpoch: TEST_SERVER_EPOCH,
      headGlobalSequence: "20",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: [LOCAL_SESSION_LIFECYCLE_FEATURE],
  });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: TEST_SERVER_EPOCH, globalSequence: "20" } });
}

function lifecycleStore(
  schedule?: (delayMs: number, task: () => void) => () => void,
) {
  return onlineStore({
    resources: resources({
      sessions: [
        session({ id: ONLINE_SESSION_ID, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
        session({ id: SESSION_B, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
      ],
    }),
    features: [LOCAL_SESSION_LIFECYCLE_FEATURE],
    ...(schedule === undefined ? {} : { schedule }),
  });
}

describe("WP4 直接创建与导航归属", () => {
  it.each([
    {
      label: "已登记目录",
      input: { directoryId: DIRECTORY_ID, agentId: "codex-a" },
      payload: { directoryId: DIRECTORY_ID, agentId: "codex-a" },
    },
    {
      label: "已有路径",
      input: { directoryId: null, agentId: "codex-a", directoryPath: "D:\\work\\api" },
      payload: { workspacePath: "D:\\work\\api", agentId: "codex-a" },
    },
  ] as const)("$label 只发送一次稳定 requestId 的规范载荷", ({ input, payload }) => {
    const { store, gateway } = onlineStore({});
    const first = store.submitCreateSession(input);
    expect(first.ok).toBe(true);
    if (!first.ok) throw new Error(first.reason);

    const duplicate = store.submitCreateSession(input);
    expect(duplicate.ok).toBe(false);
    const command = lastDispatch(gateway);
    if (command.body.command !== "session.create") throw new Error("应派发 session.create");
    expect(command.body.payload).toEqual(payload);
    expect(Object.keys(command.body.payload).sort()).toEqual(Object.keys(payload).sort());
    expect(Object.keys(command.body.payload)).not.toContain("createIfMissing");
    expect(command.body.requestId).toBe(first.requestId);
    expect(gateway.dispatches.filter((item) => item.body.command === "session.create")).toHaveLength(1);
  });

  it("accepted 只表示已接受；匹配的 completed 才产生一次一次性导航", () => {
    const { store } = onlineStore({});
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "codex-a" });
    if (!outcome.ok) throw new Error(outcome.reason);

    store.apply({ kind: "command_result", result: createResult(outcome.requestId, "accepted") });
    expect(store.state.navigationTarget).toBeNull();
    expect(store.takeNavigationTarget()).toBeNull();

    store.apply({ kind: "command_result", result: createResult(outcome.requestId, "completed") });
    expect(store.state.resources?.sessions.some((item) => item.sessionId === CREATED_SESSION_ID)).toBe(true);
    expect(store.takeNavigationTarget()).toEqual({
      kind: "session",
      sessionId: CREATED_SESSION_ID,
      reason: "created",
    });
    expect(store.takeNavigationTarget()).toBeNull();
  });

  it("浏览器返回或外部路由变化后，迟到的 completed 只入库不抢路由", () => {
    const { store } = onlineStore({});
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "codex-a" });
    if (!outcome.ok) throw new Error(outcome.reason);

    store.noteRouteChanged();
    store.apply({ kind: "command_result", result: createResult(outcome.requestId, "completed") });

    expect(store.takeNavigationTarget()).toBeNull();
    expect(store.state.resources?.sessions.some((item) => item.sessionId === CREATED_SESSION_ID)).toBe(true);
  });

  it("创建会话已进入墓碑后，迟到 completed 不恢复列表、详情或命令记录", () => {
    const { store } = onlineStore({});
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "codex-a" });
    if (!outcome.ok) throw new Error(outcome.reason);

    store.apply({
      kind: "event",
      event: deletedEvent("dddddddd-0000-4000-8000-000000000003", CREATED_SESSION_ID, "11"),
    });
    store.apply({ kind: "command_result", result: createResult(outcome.requestId, "completed") });

    expect(store.state.resources?.sessions.some((item) => item.sessionId === CREATED_SESSION_ID)).toBe(false);
    expect(store.conversationPage(CREATED_SESSION_ID)).toBeNull();
    expect(store.state.commands.has(outcome.requestId)).toBe(false);
    expect(store.state.createRequestIds.get(DIRECTORY_ID)).toBeUndefined();
    expect(store.state.createNavigationIntent).toBeNull();
  });

  it("uncertain 保留原 requestId；再次点击、返回页面和重连都不自动重发或换号", () => {
    const { store, gateway } = onlineStore({ features: [LOCAL_SESSION_LIFECYCLE_FEATURE] });
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "codex-a" });
    if (!outcome.ok) throw new Error(outcome.reason);
    store.apply({ kind: "command_result", result: createResult(outcome.requestId, "uncertain") });

    const duplicate = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "codex-a" });
    expect(duplicate.ok).toBe(false);
    expect(store.state.createRequestIds.get(DIRECTORY_ID)).toBe(outcome.requestId);

    store.setActiveConversationSession(ONLINE_SESSION_ID);
    reconnect(store);
    expect(store.state.createRequestIds.get(DIRECTORY_ID)).toBe(outcome.requestId);
    expect(gateway.dispatches.filter((item) => item.body.command === "session.create")).toHaveLength(1);
  });
});

describe("WP4 查看声明、激活与切换顺序", () => {
  it("active 摘要首帧 ready；同一连接先声明 view 再派发 resume，重入复用 ready 资源", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);

    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("ready");
    expect(gateway.lifecycleOrder.slice(0, 2)).toEqual([
      `view:${ONLINE_SESSION_ID}`,
      "command:session.resume",
    ]);

    store.setActiveConversationSession(SESSION_B);
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    expect(gateway.lifecycleOrder.slice(-5)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
      "view:null",
      `view:${ONLINE_SESSION_ID}`,
      "command:session.resume",
    ]);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.header.activation?.kind).toBe("ready");
  });

  it("A→B 明确经过 A→null→B；A 迟到结果不改变 B 的查看选择", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    store.setActiveConversationSession(SESSION_B);

    expect(gateway.views.map((view) => view.sessionId)).toEqual([
      ONLINE_SESSION_ID,
      null,
      SESSION_B,
    ]);
    expect(gateway.lifecycleOrder.slice(-2)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
    ]);

    const stale = gateway.dispatches.find(
      (item) => item.body.command === "session.resume" && item.body.sessionId === ONLINE_SESSION_ID,
    );
    if (stale?.body.command !== "session.resume") throw new Error("缺少 A 的 resume");
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000002",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          requestId: stale.body.requestId,
          command: "session.resume",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: {
            sessionId: ONLINE_SESSION_ID,
            session: session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" }),
          },
          error: null,
        },
      },
    });

    expect(store.state.lifecycle.selectedSessionId).toBe(SESSION_B);
    expect(gateway.views.at(-1)?.sessionId).toBe(SESSION_B);
    expect(store.conversationPage(SESSION_B)?.header.sessionId).toBe(SESSION_B);
  });

  it("刷新对应的重新进入、页面可见与重连都重新激活当前 B，保持选择与 viewer lease", () => {
    const first = lifecycleStore();
    first.store.setActiveConversationSession(SESSION_B);
    expect(first.gateway.lifecycleOrder.slice(-2)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
    ]);

    first.store.applyLifecycle({ kind: "visibility", state: "hidden" });
    expect(first.gateway.views.at(-1)?.sessionId).toBeNull();
    first.store.applyLifecycle({ kind: "visibility", state: "visible" });
    expect(first.gateway.lifecycleOrder.slice(-2)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
    ]);

    reconnect(first.store);
    expect(first.store.state.lifecycle.selectedSessionId).toBe(SESSION_B);
    expect(first.gateway.lifecycleOrder.slice(-2)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
    ]);

    const refreshed = lifecycleStore();
    refreshed.store.setActiveConversationSession(SESSION_B);
    expect(refreshed.gateway.lifecycleOrder.slice(-2)).toEqual([
      `view:${SESSION_B}`,
      "command:session.resume",
    ]);
    expect(refreshed.store.conversationPage(SESSION_B)?.header.activation?.kind).toBe("ready");
  });
});

describe("WP4 session.deleted 投影", () => {
  it("当前会话删除清除全部会话缓存、声明离开并只导航一次；重放和快照不能复活", async () => {
    const released: string[] = [];
    const handle: AttachmentFileHandle = {
      localId: "attachment-local",
      kind: "file",
      fileName: "notes.txt",
      mediaType: "text/plain",
      byteLength: "5",
    };
    const media: AttachmentMediaPort = {
      pick: async () => [handle],
      fromPaste: async () => [],
      readChunk: async () => new Uint8Array([1]),
      previewUrl: () => "blob:notes",
      createObjectUrl: () => null,
      releaseUrl: (url) => released.push(url),
    };
    const transfer: AttachmentTransferGateway = {
      upload: async () => ({ attachmentId: "attachment-1", sha256: "digest", sealedAt: "2026-10-01T00:00:01.000Z" }),
      sealManifest: async () => ({ attachmentCount: "1", manifestDigest: "digest" }),
      read: async () => ({
        offset: "0",
        length: "1",
        totalLength: "5",
        data: new Uint8Array([1]),
        eof: true,
      }),
      abort: () => undefined,
    };
    const { store, gateway } = onlineStore({
      resources: resources({
        sessions: [
          session({ id: ONLINE_SESSION_ID, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
          session({ id: SESSION_B, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
        ],
      }),
      features: [LOCAL_SESSION_LIFECYCLE_FEATURE, ATTACHMENT_FEATURE],
      attachments: { media, transfer },
    });

    store.setActiveConversationSession(ONLINE_SESSION_ID);
    store.setDraft(ONLINE_SESSION_ID, "删除我");
    store.setDraft(SESSION_B, "保留我");
    store.beginTitleEdit(ONLINE_SESSION_ID);
    store.setTitleDraft(ONLINE_SESSION_ID, "新标题");
    expect(store.addPathRef(ONLINE_SESSION_ID, "src/main.ts").ok).toBe(true);
    expect((await store.pickAttachments(ONLINE_SESSION_ID, "file")).ok).toBe(true);
    store.setConversationSecondaryView("files");
    store.setConversationSidebarQuery("main");

    store.loadSession(ONLINE_SESSION_ID);
    const read = lastDispatch(gateway);
    if (read.body.command !== "session.read") throw new Error("应派发 session.read");
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: read.body.requestId,
        sessionId: ONLINE_SESSION_ID,
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "turn.started",
        eventId: "dddddddd-0000-4000-8000-000000000001",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "11",
        view: { turnId: "turn-1", state: "running" },
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.usage.changed",
        eventId: "dddddddd-0000-4000-8000-000000000002",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "12",
        view: { used: "1", size: "10" },
      }),
    });
    expect(store.state.threads.has(ONLINE_SESSION_ID)).toBe(true);
    expect(store.state.readResources.has(ONLINE_SESSION_ID)).toBe(true);
    expect(store.state.usage.has(ONLINE_SESSION_ID)).toBe(true);
    expect(store.state.attachments.has(ONLINE_SESSION_ID)).toBe(true);

    const deletion = deletedEvent("dddddddd-0000-4000-8000-000000000010", ONLINE_SESSION_ID, "13");
    store.apply({ kind: "event", event: deletion });

    expect(store.state.resources?.sessions.map((item) => item.sessionId)).toEqual([SESSION_B]);
    expect(store.directoryPageSessions().get(DIRECTORY_ID)?.map((row) => row.id)).toEqual([SESSION_B]);
    expect(store.directoryDetail(DIRECTORY_ID)).toMatchObject({
      kind: "ready",
      sessions: [expect.objectContaining({ id: SESSION_B })],
    });
    expect(store.conversationPage(ONLINE_SESSION_ID)).toBeNull();
    expect(store.state.threads.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.readResources.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.drafts.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.drafts.get(SESSION_B)).toBe("保留我");
    expect(store.state.pathRefs.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.attachments.has(ONLINE_SESSION_ID)).toBe(false);
    expect(released).toContain("blob:notes");
    expect(store.state.usage.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.lifecycle.activations.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.lifecycle.selectedSessionId).toBeNull();
    expect(store.state.lifecycle.view?.sessionId).toBeNull();
    expect(store.state.titleEditor.sessionId).toBeNull();
    expect(store.state.conversationSecondaryView).not.toBe("files");
    expect(store.state.conversationSidebarQuery).toBe("");
    expect(store.state.commands.size).toBe(0);
    expect(store.state.deletedSessionIds.has(ONLINE_SESSION_ID)).toBe(true);
    expect(store.takeNavigationTarget()).toEqual({
      kind: "home",
      sessionId: ONLINE_SESSION_ID,
      reason: "deleted",
    });
    expect(store.takeNavigationTarget()).toBeNull();

    store.apply({ kind: "event", event: deletion });
    expect(store.takeNavigationTarget()).toBeNull();
    store.apply({
      kind: "event",
      event: deletedEvent("dddddddd-0000-4000-8000-000000000011", ONLINE_SESSION_ID, "14"),
    });
    expect(store.takeNavigationTarget()).toBeNull();

    store.apply({
      kind: "snapshot_verified",
      snapshot: {
        snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
        cursor: { serverEpoch: TEST_SERVER_EPOCH, globalSequence: "15" },
        resources: resources({
          sessions: [
            session({ id: ONLINE_SESSION_ID, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
            session({ id: SESSION_B, directoryId: DIRECTORY_ID, state: "idle", localActivity: "active" }),
          ],
        }),
        chunkCount: 1,
        receivedChunks: 1,
        snapshotDigest: "digest",
      },
    });
    expect(store.state.resources?.sessions.map((item) => item.sessionId)).toEqual([SESSION_B]);
  });

  it("非当前会话删除不改变当前 B 的页面、选择或查看租约", () => {
    const cancellations: number[] = [];
    let timerId = 0;
    const { store, gateway } = lifecycleStore(() => {
      const current = ++timerId;
      return () => cancellations.push(current);
    });
    store.setActiveConversationSession(SESSION_B);
    const before = gateway.views.at(-1);
    expect(cancellations).toEqual([]);

    store.apply({
      kind: "event",
      event: deletedEvent("dddddddd-0000-4000-8000-000000000020", ONLINE_SESSION_ID, "13"),
    });

    expect(store.takeNavigationTarget()).toBeNull();
    expect(store.state.lifecycle.selectedSessionId).toBe(SESSION_B);
    expect(store.state.lifecycle.view?.sessionId).toBe(SESSION_B);
    expect(gateway.views.at(-1)).toEqual(before);
    expect(cancellations).toEqual([]);
    expect(store.conversationPage(SESSION_B)?.header.sessionId).toBe(SESSION_B);
  });

  it("删除后迟到的创建、恢复、读取与 session.created 都不能复活列表或详情", () => {
    const { store, gateway } = lifecycleStore();
    store.setActiveConversationSession(ONLINE_SESSION_ID);
    store.loadSession(ONLINE_SESSION_ID);

    const resume = gateway.dispatches.find(
      (item) => item.body.command === "session.resume" && item.body.sessionId === ONLINE_SESSION_ID,
    );
    const read = gateway.dispatches.find(
      (item) => item.body.command === "session.read" && item.body.sessionId === ONLINE_SESSION_ID,
    );
    if (resume?.body.command !== "session.resume") throw new Error("缺少 session.resume");
    if (read?.body.command !== "session.read") throw new Error("缺少 session.read");

    store.apply({
      kind: "event",
      event: deletedEvent("dddddddd-0000-4000-8000-000000000030", ONLINE_SESSION_ID, "13"),
    });
    store.takeNavigationTarget();

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000030",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          requestId: resume.body.requestId,
          command: "session.resume",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: {
            sessionId: ONLINE_SESSION_ID,
            session: session({ id: ONLINE_SESSION_ID, directoryId: DIRECTORY_ID }),
          },
          error: null,
        },
      },
    });
    store.apply({
      kind: "command_result",
      result: readResult({ requestId: read.body.requestId, sessionId: ONLINE_SESSION_ID }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.created",
        eventId: "dddddddd-0000-4000-8000-000000000031",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "14",
        view: { session: session({ id: ONLINE_SESSION_ID, directoryId: DIRECTORY_ID }) },
      }),
    });

    expect(store.state.resources?.sessions.map((item) => item.sessionId)).toEqual([SESSION_B]);
    expect(store.conversationPage(ONLINE_SESSION_ID)).toBeNull();
    expect(store.state.threads.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.readResources.has(ONLINE_SESSION_ID)).toBe(false);
    expect(store.state.lifecycle.activations.has(ONLINE_SESSION_ID)).toBe(false);
  });
});
