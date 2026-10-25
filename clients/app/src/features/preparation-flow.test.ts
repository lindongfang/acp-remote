/**
 * WP7 准备流程的 feature 级用例（`clients/app/src/features/`）。
 *
 * 覆盖：allocate 只带 Agent/目录/标签页归属、成功后进入同 ID、首条只派发一次、
 * 双击不产生第二次执行、结果不确定不自动重试、`store_rebuilt` 明确不可用、换 Agent 释放旧资源并换新 ID、
 * 旧代应答不覆盖新页面、刷新沿草稿 restore、未协商明确降级、未知 URL 不产生准备。
 *
 * 网络与存储都被替身接管：网关只记账并允许用例投递服务端应答；草稿端口用内存替身。
 */

import { describe, expect, it } from "vitest";

import type { CommandMessage, PreparationReceipt, SessionPreparationBody } from "../protocol";
import type { CompletedSnapshot, StagedResources, SyncClientEvent } from "../sync-client";
import { AttachmentTransferError } from "../sync-client";
import { ClientStore } from "./client-store";
import type {
  AttachmentMediaPort,
  SyncGateway,
  AttachmentTransferGateway,
  PreparationDraftAttachmentInput,
  PreparationDraftContent,
  PreparationDraftGateway,
  PreparationDraftLoadResult,
  PreparationTabLocatorSnapshot,
} from "./ports";
import { SESSION_PREPARATION_FEATURE, PREPARATION_RENEW_INTERVAL_MS } from "./preparation-model";
import { eventMessage, resources, session } from "./testing/fixtures";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
const HOST_ID = "0f9c3b9a-2d31-4fd0-8e5d-4a4f7a5f1b10";
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";
const SESSION_ID = "6f1c2d3e-4a5b-4c6d-8e7f-901122334455";
const TAB_NONCE = "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8";
const ORIGIN = "https://127.0.0.1:1";
const TAB_SLOT_ID = "11111111-1111-4111-8111-111111111111";

/** 本标签页定位：刷新恢复只认它（不含正文/凭据）。 */
function tabLocatorFor(sessionId: string): PreparationTabLocatorSnapshot {
  return {
    canonicalOrigin: ORIGIN,
    ownerNodeId: HOST_ID,
    deviceId: DEVICE_ID,
    tabSlotId: TAB_SLOT_ID,
    sessionId,
  };
}

function receiptFor(sessionId: string, agentId = "codex-a"): PreparationReceipt {
  return {
    claims: {
      version: 1,
      ownerNodeId: HOST_ID,
      storeEpoch: SERVER_EPOCH,
      origin: ORIGIN,
      deviceId: DEVICE_ID,
      tabNonce: TAB_NONCE,
      sessionId,
      agentId,
      normalizedWorkspacePath: "D:\\work\\api",
      workspaceSource: "directoryId",
    },
    signature: "fQajLSzq22JwoKUsx9CuNXg6t1wPQpr2CbpLKOhnNqCBl33DMmr8uIlPXDDBdkzdCehwkW503hBvUjO2ywZQsw",
  };
}

/**
 * 构造一条 `session.preparation.result` 事件。
 *
 * 协议类型镜像没有把 `sessionId` 回显字段展开到每个分支（wire 上它恒在），因此这里做一次
 * **有理由的**断言：测试只构造 wire 形状，实际解码由 `wire.ts` 的门禁覆盖。
 */
function preparationEvent(body: Record<string, unknown>): SyncClientEvent {
  const result = {
    protocolVersion: 1,
    type: "session.preparation.result",
    messageId: "9f1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f",
    connectionId: CONNECTION_ID,
    connectionSequence: "12",
    body,
  } as unknown as Extract<SyncClientEvent, { kind: "session_preparation_result" }>["result"];
  return { kind: "session_preparation_result", result };
}

function readyResult(input: {
  readonly operationId: string;
  readonly sessionId: string;
  readonly action: "allocate" | "restore" | "configure" | "renew";
}): SyncClientEvent {
  return preparationEvent({
    operationId: input.operationId,
    action: input.action,
    sessionId: input.sessionId,
    status: "ready",
    receipt: receiptFor(input.sessionId),
    runtimeEpoch: "3f2a1b0c-9d8e-4f7a-8b6c-5d4e3f2a1b0c",
    generation: "1",
    modes: [
      { modeId: "default", displayName: "Default" },
      { modeId: "compact", displayName: "Compact" },
    ],
    configOptions: [
      { id: "sandbox", name: "Sandbox", description: null, category: null, type: "boolean", currentValue: true },
    ],
    leaseExpiresAt: "2026-10-22T08:00:30.000Z",
  });
}

function failedResult(input: {
  readonly operationId: string;
  readonly sessionId: string;
  readonly action: "allocate" | "restore" | "configure" | "release";
  readonly code: string;
  readonly retryable: boolean;
}): SyncClientEvent {
  return preparationEvent({
    operationId: input.operationId,
    action: input.action,
    sessionId: input.sessionId,
    status: "failed",
    error: { code: input.code, message: "rejected", retryable: input.retryable, details: {} },
  });
}

interface Harness {
  readonly store: ClientStore;
  readonly frames: SessionPreparationBody[];
  readonly commands: CommandMessage[];
}

/** 一个已在线、协商了准备能力的 store；网关记录准备帧与命令，用例可直接投递服务端应答。 */
function preparationStore(input: {
  readonly features?: readonly string[];
  readonly drafts?: PreparationDraftGateway;
  readonly transfers?: AttachmentTransferGateway;
  readonly media?: AttachmentMediaPort;
  readonly resources?: StagedResources;
  readonly schedule?: (delayMs: number, task: () => void) => () => void;
  readonly tabNonce?: boolean;
}): Harness {
  const frames: SessionPreparationBody[] = [];
  const commands: CommandMessage[] = [];
  let counter = 0;
  const snapshot: CompletedSnapshot = {
    snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
    cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "10" },
    resources:
      input.resources ??
      resources({
        sessions: [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "idle" })],
      }),
    verifiedAtMs: 1_700_000_000_000,
  };
  const gateway: SyncGateway = {
    current: snapshot,
    refreshSnapshot: () => undefined,
    authenticatedInfo: null,
    dispatch(command) {
      commands.push(command);
    },
    sendPreparation(body) {
      frames.push(body);
      return true;
    },
    setSessionView: () => true,
    subscribeRemote: () => undefined,
    unsubscribeRemote: () => undefined,
  };
  const store = new ClientStore({
    sync: gateway,
    ids: {
      uuid(): string {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    now: () => 1_700_000_000_000,
    identity: { connectionId: CONNECTION_ID },
    ...(input.schedule === undefined ? {} : { schedule: input.schedule }),
    ...(input.drafts === undefined ? {} : { preparationDrafts: input.drafts }),
    ...(input.transfers === undefined
      ? {}
      : {
          attachments: {
            media: input.media ?? fakeMedia(),
            transfer: input.transfers,
          },
        }),
    ...(input.tabNonce === false ? {} : { preparationTabNonce: () => TAB_NONCE }),
  });
  store.setHostIdentity({ address: ORIGIN, hostId: HOST_ID, deviceId: DEVICE_ID });
  store.apply({
    kind: "snapshot_verified",
    snapshot: { ...snapshot, chunkCount: 3, receivedChunks: 3, snapshotDigest: "kQ8x2mB7yV1cJ0nP5" },
  });
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: DEVICE_ID,
      serverEpoch: SERVER_EPOCH,
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: input.features ?? [SESSION_PREPARATION_FEATURE, "core.attachment-transfer.v1"],
  });
  store.pairSucceeded();
  store.apply({ kind: "authenticating" });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "10" } });
  return { store, frames, commands };
}

function fakeMedia(): AttachmentMediaPort {
  return {
    pick: async () => [],
    fromPaste: async () => [],
    readChunk: async (_handle, _offset, length) => new Uint8Array(length),
    previewUrl: () => null,
    createObjectUrl: () => null,
    releaseUrl: () => undefined,
  };
}

/** 内存草稿端口：实现本用例用到的草稿/定位路径。 */
function fakeDrafts(input: {
  readonly load?: PreparationDraftLoadResult;
  readonly onSave?: (content: PreparationDraftContent) => void;
  /** 模拟复制/opener 标签页：归属已被另一个文档实例持有（可按会话区分）。 */
  readonly duplicate?: boolean | ((sessionId: string) => boolean);
  /** 让 `beginTab` 停在闸门上，用来构造「慢恢复晚于新分配落地」的竞态。 */
  readonly beginTabGate?: Promise<void>;
  /** 让 `invalidateDraft` 停在闸门上，用来构造「放弃期间 restore 结果落地」的竞态。 */
  readonly invalidateGate?: Promise<void>;
  readonly locator?: PreparationTabLocatorSnapshot | null;
}): PreparationDraftGateway & { readonly invalidated: number } {
  let revision = 0;
  let invalidated = 0;
  let locator: PreparationTabLocatorSnapshot | null = input.locator ?? null;
  let saved:
    | {
        readonly revision: number;
        readonly content: PreparationDraftContent;
        readonly attachments: readonly PreparationDraftAttachmentInput[];
      }
    | null = null;
  return {
    get invalidated() {
      return invalidated;
    },
    async readTabLocator() {
      return locator;
    },
    async clearTabLocator() {
      locator = null;
    },
    async beginTab({ scope }) {
      if (input.beginTabGate !== undefined) await input.beginTabGate;
      const duplicate =
        typeof input.duplicate === "function" ? input.duplicate(scope.sessionId) : input.duplicate === true;
      if (duplicate) {
        return { status: "duplicate", reason: "live_tab_owner" };
      }
      // 与 Web 端口同一规则：定位与来源/节点/设备/会话一致时沿用它的 tabSlotId，
      // 否则刷新会被误判成换分区。调用方首次挂载时只带随机占位值。
      const effectiveScope =
        locator !== null &&
        locator.canonicalOrigin === scope.canonicalOrigin &&
        locator.ownerNodeId === scope.ownerNodeId &&
        locator.deviceId === scope.deviceId &&
        locator.sessionId === scope.sessionId
          ? { ...scope, tabSlotId: locator.tabSlotId }
          : scope;
      locator = {
        canonicalOrigin: effectiveScope.canonicalOrigin,
        ownerNodeId: effectiveScope.ownerNodeId,
        deviceId: effectiveScope.deviceId,
        tabSlotId: effectiveScope.tabSlotId,
        sessionId: effectiveScope.sessionId,
      };
      return {
        status: "owner",
        access: { scope: effectiveScope, instanceId: "11111111-1111-4111-8111-111111111111" },
        restorable: true,
        restored: input.load?.status === "active",
      };
    },
    async loadDraft() {
      if (input.load !== undefined) return input.load;
      if (saved === null) return { status: "absent" };
      return {
        status: "active",
        record: {
          revision: saved.revision,
          payload: {
            status: "active",
            content: saved.content,
            attachments: saved.attachments.map((attachment) => ({
              localId: attachment.localId,
              kind: attachment.kind,
              fileName: attachment.fileName,
              mediaType: attachment.mediaType,
              byteLength: attachment.byteLength,
              chunkSize: 128 * 1024,
              chunkCount: Math.max(1, Math.ceil(Number(attachment.byteLength) / (128 * 1024))),
            })),
          },
        },
      };
    },
    async saveDraft(request) {
      revision += 1;
      saved = {
        revision,
        content: request.content,
        attachments: request.attachments ?? [],
      };
      input.onSave?.(request.content);
      return { revision, restorable: true };
    },
    async readAttachment({ localId }) {
      return {
        attachment: {
          localId,
          kind: "file",
          fileName: "a.txt",
          mediaType: "text/plain",
          byteLength: "0",
          chunkSize: 128 * 1024,
          chunkCount: 1,
        },
        bytes: new Uint8Array(0),
      };
    },
    async markAccepted() {
      revision += 1;
      return { revision };
    },
    async completeRequest() {
      /* 恢复记录删除由平台端口落盘；测试只需要它不抛错。 */
    },
    async invalidateDraft() {
      if (input.invalidateGate !== undefined) await input.invalidateGate;
      invalidated += 1;
      saved = null;
      revision += 1;
      return { revision };
    },
    async releaseTab() {
      /* 本标签页归属清理由平台端口完成。 */
    },
  };
}

/** 让已经排队的 Promise 链落地。 */
async function flush(): Promise<void> {
  for (let round = 0; round < 20; round += 1) await Promise.resolve();
}

async function allocateReady(harness: Harness, sessionId = SESSION_ID): Promise<void> {
  const begin = harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
  const frame = harness.frames[0];
  if (frame === undefined || frame.action !== "allocate") throw new Error("没有发出 allocate");
  harness.store.apply(readyResult({ operationId: frame.operationId, sessionId, action: "allocate" }));
  const outcome = await begin;
  expect(outcome.ok).toBe(true);
}

describe("会话准备：分配与同页", () => {
  it("新目录左栏不混入已有目录，文件与 Git 按目录读取，不等待首条或正式会话", async () => {
    const harness = preparationStore({ features: [SESSION_PREPARATION_FEATURE, "core.session-files.v1", "core.workspace-files.v1"] });
    const begin = harness.store.beginPreparation({ directoryId: null, agentId: "codex-a", directoryPath: "D:\\fresh\\project" });
    const allocation = harness.frames[0];
    if (allocation === undefined) throw new Error("缺少分配请求");
    const ready = readyResult({ operationId: allocation.operationId, sessionId: SESSION_ID, action: "allocate" });
    if (ready.kind !== "session_preparation_result") throw new Error("错误的准备应答");
    const receipt = { ...receiptFor(SESSION_ID), claims: { ...receiptFor(SESSION_ID).claims,
      normalizedWorkspacePath: "D:\\fresh\\project", workspaceSource: "workspacePath" as const } };
    harness.store.apply(preparationEvent({ ...ready.result.body, receipt }));
    await begin;
    expect(harness.store.conversationPage(SESSION_ID)?.sidebar).toMatchObject({
      directory: { directoryId: null, displayName: "project", path: "D:\\fresh\\project" },
      notice: null, rows: [{ id: SESSION_ID, title: "新会话", updatedAtLabel: "未发送消息" }], total: 1,
    });
    harness.store.setActiveConversationSession(SESSION_ID);
    harness.store.loadMoreAllFiles(SESSION_ID, "");
    harness.store.refreshFiles(SESSION_ID);
    harness.store.selectFileSource(SESSION_ID, "git");
    expect(harness.commands.length).toBeGreaterThan(0);
    for (const command of harness.commands) {
      expect(command.body).not.toHaveProperty("sessionId");
      expect(command.body.payload).toMatchObject({ workspace: { workspacePath: "D:\\fresh\\project" } });
    }
    expect(harness.store.sessionFilesPage(SESSION_ID).gitNotice).toBeNull();

    harness.store.apply({ kind: "snapshot_verified", snapshot: {
      snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
      cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "11" },
      resources: resources({ workspaces: [{ directoryId: "fresh", path: "D:\\fresh\\project" }],
        sessions: [session({ id: SESSION_ID, directoryId: "fresh" })] }),
      chunkCount: 3, receivedChunks: 3, snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
    } });
    expect(harness.commands.some((command) => command.body.command === "session.git.status")).toBe(true);
    expect(harness.store.sessionFilesPage(SESSION_ID).gitNotice).toBeNull();
  });

  it("普通新建只复用唯一槽位：切换后保留标题与草稿，其他目录/Agent 入口不另分配", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "尚未发送");
    const count = harness.frames.length;
    expect(harness.store.activePreparationSessionId()).toBe(SESSION_ID);
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: SESSION_ID });
    harness.store.leaveConversationSession(SESSION_ID);
    expect(harness.store.preparationPage(SESSION_ID)?.text).toBe("尚未发送");
    expect(harness.store.conversationPage(SESSION_ID)?.sidebar.rows).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: SESSION_ID, title: "新会话" }),
    ]));
    // 其他目录、不同 Agent 的普通入口也只导航回现有槽位：不发 allocate、不改目录。
    expect(await harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" }))
      .toEqual({ ok: true, sessionId: SESSION_ID });
    expect(await harness.store.beginPreparation({ directoryId: "work-other", agentId: "codex-b" }))
      .toEqual({ ok: true, sessionId: SESSION_ID });
    expect(harness.frames).toHaveLength(count);
    expect(harness.store.preparationPage(SESSION_ID)?.workspace.path).toBe("D:\\work\\api");
    expect(harness.store.preparationPage(SESSION_ID)?.agentId).toBe("codex-a");
  });

  it("首条前提供正常对话页、目录上下文和输入模型，且不伪造正式会话", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "第一条消息");
    const page = harness.store.conversationPage(SESSION_ID);
    expect(page?.header.title).toBe("新会话");
    expect(page?.header.directoryId).toBe("work-api");
    expect(page?.sidebar.directory?.path).toBe("D:\\work\\api");
    expect(page?.settings.mode.choices.map((mode) => mode.id)).toEqual(["default", "compact"]);
    expect(page?.input.draft).toBe("第一条消息");
    expect(page?.input.canSubmit).toBe(true);
    expect(page?.timeline.entries).toEqual([]);
    expect(harness.commands).toEqual([]);
    expect(harness.store.state.resources?.sessions.some((item) => item.sessionId === SESSION_ID)).toBe(false);
  });

  it("首条提交后立即在时间线呈现用户正文", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "首条立即可见");
    const outcome = harness.store.submitPreparation(SESSION_ID, "首条立即可见");
    expect(outcome.ok).toBe(true);
    await flush();

    const page = harness.store.conversationPage(SESSION_ID);
    expect(page?.timeline.entries.some(
      (entry) => entry.kind === "message" && entry.channel === "user" && entry.message.text === "首条立即可见",
    )).toBe(true);
  });

  it("首条派发后的思考与工具调用在正式读取前也呈现", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.thought.delta",
        eventId: "dddddddd-0000-4000-8000-000000000501",
        sessionId: SESSION_ID,
        view: { messageId: "th-live", turnId: "turn-live", deltaIndex: "0", text: "先看一下仓库" },
      }),
    });
    harness.store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "tool.call.started",
        eventId: "dddddddd-0000-4000-8000-000000000502",
        sessionId: SESSION_ID,
        view: { toolCallId: "tool-live", turnId: "turn-live", title: "读取 package.json", state: "in_progress" },
      }),
    });

    const page = harness.store.conversationPage(SESSION_ID);
    // 正式 `session.read` 还没回来：时间线仍必须呈现已经到达的思考与工具卡。
    expect(page?.timeline.entries.some((entry) => entry.kind === "message" && entry.channel === "thought")).toBe(true);
    expect(page?.timeline.entries.some((entry) => entry.kind === "tool_call")).toBe(true);
  });

  it("目录输入与服务端规范化路径不同，保存的草稿目录仍与服务端凭据一致", async () => {
    let saved: PreparationDraftContent | undefined;
    const harness = preparationStore({ drafts: fakeDrafts({ onSave: (content) => {
      expect(content.workspace.path).toBe(content.receipt.claims.normalizedWorkspacePath);
      expect(content.workspace.source).toBe(content.receipt.claims.workspaceSource);
      saved = content;
    } }) });
    const begin = harness.store.beginPreparation({ directoryId: null, agentId: "codex-a", directoryPath: "d:/work/api/" });
    const allocation = harness.frames[0];
    if (allocation === undefined) throw new Error("缺少分配请求");
    const receipt = { ...receiptFor(SESSION_ID), claims: { ...receiptFor(SESSION_ID).claims, workspaceSource: "workspacePath" as const } };
    harness.store.apply(preparationEvent({
      ...((readyResult({ operationId: allocation.operationId, sessionId: SESSION_ID, action: "allocate" }) as Extract<SyncClientEvent, { kind: "session_preparation_result" }>).result.body),
      receipt,
    }));
    await begin;
    await flush();
    expect(saved?.workspace.path).toBe(receipt.claims.normalizedWorkspacePath);
    expect(saved?.workspace.source).toBe("workspacePath");
    expect(harness.store.preparationPage(SESSION_ID)?.notice).toBeNull();
  });

  it("allocate 只带 Agent、目录与标签页归属，成功取得后端 ID，且不产生 session.create", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    const frame = harness.frames[0];
    expect(frame).toBeDefined();
    expect(Object.keys(frame!).sort()).toEqual(["action", "agentId", "operationId", "tabNonce", "workspace"]);
    expect(frame).not.toHaveProperty("sessionId");
    expect(frame).toMatchObject({ action: "allocate", agentId: "codex-a", workspace: { directoryId: "work-api" } });
    // 首条之前：没有命令、目录里没有新会话。
    expect(harness.commands).toEqual([]);
    expect(harness.store.state.resources?.sessions.map((item) => item.sessionId)).not.toContain(SESSION_ID);
    // 同一页面进入准备模式，设置来自服务端真实回报。
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page).not.toBeNull();
    expect(page?.phase).toBe("ready");
    expect(page?.settings.modes.map((mode) => mode.modeId)).toEqual(["default", "compact"]);
    expect(page?.settings.selectedMode).toBe("default");
    expect(page?.settings.configOptions[0]?.value).toBe(true);
    expect(page?.input.canSend).toBe(false);
  });

  it("会话进入权威快照后退出准备角色：准备页不再遮蔽正式会话", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    expect(harness.store.preparationPage(SESSION_ID)).not.toBeNull();
    harness.store.apply({
      kind: "snapshot_verified",
      snapshot: {
        snapshotId: "8194de43-e213-423d-acf4-2e3549304567",
        cursor: { serverEpoch: SERVER_EPOCH, globalSequence: "11" },
        resources: resources({ sessions: [session({ id: SESSION_ID, directoryId: "work-api" })] }),
        chunkCount: 3,
        receivedChunks: 3,
        snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
      },
    });
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "none" });
    expect(harness.store.conversationPage(SESSION_ID)).not.toBeNull();
  });

  it("未协商准备能力：入口明确不可用，且一条准备帧/命令都不发", async () => {
    const harness = preparationStore({ features: ["core.attachment-transfer.v1"] });
    expect(harness.store.preparationEntryAvailability().available).toBe(false);
    const outcome = await harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    expect(outcome.ok).toBe(false);
    expect(harness.frames).toEqual([]);
    expect(harness.store.createEntry("work-api").preparation.available).toBe(false);
  });

  it("陌生 URL 没有本标签页草稿时不会凭空产生准备", async () => {
    const harness = preparationStore({ drafts: fakeDrafts({}) });
    harness.store.openPreparationSession(SESSION_ID);
    await flush();
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.frames).toEqual([]);
  });

  it("分配在途时两个入口共用一个闸门：只发一帧，两个 Promise 得到同一 ID", async () => {
    const harness = preparationStore({});
    const first = harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    // 第二个入口可以是另一个目录、另一个 Agent：都不得再发 allocate。
    const second = harness.store.beginPreparation({ directoryId: "work-other", agentId: "codex-b" });
    expect(harness.store.preparationSlotRoute().kind).toBe("allocating");
    expect(harness.frames).toHaveLength(1);
    const frame = harness.frames[0];
    harness.store.apply(readyResult({ operationId: frame!.operationId, sessionId: SESSION_ID, action: "allocate" }));
    await expect(first).resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    await expect(second).resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    expect(harness.frames).toHaveLength(1);
    expect(harness.store.activePreparationSessionId()).toBe(SESSION_ID);
  });
});

describe("会话准备：首条提交", () => {
  it("文本首条只派发一次 session.start（initial），双击被拒", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "帮我看看这个仓库");
    const first = harness.store.submitPreparation(SESSION_ID, "帮我看看这个仓库");
    expect(first.ok).toBe(true);
    const second = harness.store.submitPreparation(SESSION_ID, "帮我看看这个仓库");
    expect(second.ok).toBe(false);
    await flush();
    const starts = harness.commands.filter((command) => command.body.command === "session.start");
    expect(starts).toHaveLength(1);
    const body = starts[0]!.body;
    if (body.command !== "session.start") throw new Error("命令形状不符");
    expect(body.payload.kind).toBe("initial");
    if (body.payload.kind !== "initial") return;
    expect(body.payload.content).toEqual([{ type: "text", text: "帮我看看这个仓库" }]);
    expect(body.payload.receipt.claims.sessionId).toBe(SESSION_ID);
    // 保存失败/受理前的状态仍停在同一页
    expect(harness.store.preparationPage(SESSION_ID)?.phase).toBe("submitting");
  });

  it("保存结果不确定：显眼保留，且不会自动重发", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "hi");
    const outcome = harness.store.submitPreparation(SESSION_ID, "hi");
    expect(outcome.ok).toBe(true);
    await flush();
    const requestId = outcome.ok ? outcome.requestId : "";
    harness.store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bf1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f",
        connectionId: CONNECTION_ID,
        connectionSequence: "14",
        body: {
          requestId,
          command: "session.start",
          status: "uncertain",
          acceptedAt: "2026-10-22T08:00:00.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      },
    });
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.phase).toBe("uncertain");
    expect(page?.input.canSend).toBe(false);
    // 不自动重发：仍然是那一条 start。
    await flush();
    expect(harness.commands.filter((command) => command.body.command === "session.start")).toHaveLength(1);
    // 显式核实：查询原请求（不是新 start）。
    const verify = harness.store.retryPreparation(SESSION_ID);
    expect(verify.ok).toBe(true);
    const statusQuery = harness.commands.at(-1);
    expect(statusQuery?.body.command).toBe("command.status");
    if (statusQuery?.body.command !== "command.status") return;
    expect(statusQuery.body.payload.targetRequestId).toBe(requestId);
  });

  it("store_rebuilt：丢弃旧材料并明确要求回目录重新开始", async () => {
    const drafts = fakeDrafts({});
    const harness = preparationStore({ drafts });
    await allocateReady(harness);
    // 一个 configure 失败（存储整体重建）——按固定动作不可重试、不回退。
    const operationId = "12121212-1212-4121-8121-121212121212";
    harness.store.apply(failedResult({ operationId, sessionId: SESSION_ID, action: "configure", code: "preparation.store_rebuilt", retryable: false }));
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.phase).toBe("unavailable");
    expect(page?.restartRequired).toBe(true);
    expect(page?.notice).toContain("存储已重建");
    expect(harness.store.state.preparations.get(SESSION_ID)?.receipt).toBeNull();
  });
});

describe("会话准备：换 Agent/目录与旧应答", () => {
  it("换 Agent：释放旧资源、清空草稿，分配新 ID；旧应答不覆盖新页面", async () => {
    const drafts = fakeDrafts({});
    const harness = preparationStore({ drafts });
    await allocateReady(harness);
    await flush();
    const oldAllocate = harness.frames[0]!;
    harness.store.setPreparationDraft(SESSION_ID, "旧输入");
    const changing = harness.store.changePreparationAgent(SESSION_ID, "codex-b");
    await flush();
    // 释放旧临时资源
    expect(harness.frames.some((frame) => frame.action === "release" && frame.sessionId === SESSION_ID)).toBe(true);
    expect(drafts.invalidated).toBe(1);
    const replacement = harness.frames.find(
      (frame) => frame.action === "allocate" && frame.operationId !== oldAllocate.operationId,
    );
    expect(replacement).toBeDefined();
    const newSessionId = "6f1c2d3e-4a5b-4c6d-8e7f-901122334466";
    harness.store.apply(readyResult({ operationId: replacement!.operationId, sessionId: newSessionId, action: "allocate" }));
    const outcome = await changing;
    if (!outcome.ok) throw new Error(outcome.reason);
    expect(outcome.sessionId).toBe(newSessionId);
    // 旧会话状态被清掉，新页面从空白开始；槽位只剩新的那一个。
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.store.preparationPage(newSessionId)?.agentId).toBe("codex-b");
    expect(harness.store.preparationPage(newSessionId)?.text).toBe("");
    expect(harness.store.activePreparationSessionId()).toBe(newSessionId);
    // 迟到的旧代应答不改变新页面，也不能复活旧槽位
    harness.store.apply(readyResult({ operationId: oldAllocate.operationId, sessionId: SESSION_ID, action: "allocate" }));
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.store.preparationPage(newSessionId)?.phase).toBe("ready");
    expect(harness.store.activePreparationSessionId()).toBe(newSessionId);
  });
});

describe("会话准备：刷新恢复", () => {
  it("同标签页草稿沿同一 ID restore，并用真实设置重新渲染", async () => {
    const receipt = receiptFor(SESSION_ID);
    const drafts = fakeDrafts({
      locator: tabLocatorFor(SESSION_ID),
      load: {
        status: "active",
        record: {
          revision: 3,
          payload: {
            status: "active",
            content: {
              text: "刷新前的输入",
              mode: "compact",
              configOptions: { sandbox: false },
              agentId: "codex-a",
              workspace: { directoryId: "work-api", path: "D:\\work\\api", source: "directoryId" },
              receipt,
              pendingRequestId: null,
            },
            attachments: [],
          },
        },
      },
    });
    const harness = preparationStore({ drafts });
    harness.store.openPreparationSession(SESSION_ID);
    await flush();
    const restore = harness.frames.find((frame) => frame.action === "restore");
    expect(restore).toBeDefined();
    expect(restore?.sessionId).toBe(SESSION_ID);
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.text).toBe("刷新前的输入");
    expect(page?.phase).toBe("restoring");
    // restore 返回 ready：设置以 Agent 确认为准，用户的非默认选择保留。
    harness.store.apply(readyResult({ operationId: restore!.operationId, sessionId: SESSION_ID, action: "restore" }));
    const restored = harness.store.preparationPage(SESSION_ID);
    expect(restored?.settings.selectedMode).toBe("compact");
    expect(restored?.settings.configOptions[0]?.value).toBe(false);
  });

  it("restore 发现已正式受理：转 committed 并读正式会话，不重新 start", async () => {
    const receipt = receiptFor(SESSION_ID);
    const drafts = fakeDrafts({
      locator: tabLocatorFor(SESSION_ID),
      load: {
        status: "active",
        record: {
          revision: 1,
          payload: {
            status: "active",
            content: {
              text: "已经保存过",
              mode: null,
              configOptions: {},
              agentId: "codex-a",
              workspace: { directoryId: "work-api", path: "D:\\work\\api", source: "directoryId" },
              receipt,
              pendingRequestId: null,
            },
            attachments: [],
          },
        },
      },
    });
    const harness = preparationStore({ drafts });
    harness.store.openPreparationSession(SESSION_ID);
    await flush();
    const restore = harness.frames.find((frame) => frame.action === "restore");
    harness.store.apply(preparationEvent({
      operationId: restore!.operationId,
      action: "restore",
      sessionId: SESSION_ID,
      status: "committed",
      firstSubmissionId: "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
      firstRequestId: "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e",
      currentRequestId: "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f",
      firstMessageId: "3d4e5f6a-7b8c-4d9e-8f0a-1b2c3d4e5f6a",
    }));
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.phase).toBe("committed");
    expect(page?.association?.currentRequestId).toBe("2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f");
    // 不再次 start；只读取正式会话。
    await flush();
    expect(harness.commands.filter((command) => command.body.command === "session.start")).toHaveLength(0);
  });
});

// ── WP3：单一准备槽位、刷新恢复、显式放弃与身份隔离 ───────────────────────────

const FORMAL_B_ID = "1f1c2d3e-4a5b-4c6d-8e7f-901122334455";
const REPLACEMENT_ID = "7f1c2d3e-4a5b-4c6d-8e7f-901122334455";

describe("会话准备：单一槽位跨切换与刷新", () => {
  it("A→B→刷新 B→新建回 A，草稿与附件一起恢复且不分配第二个 ID", async () => {
    const drafts = fakeDrafts({});
    const first = preparationStore({
      drafts,
      media: singleAttachmentMedia(),
      transfers: failingTransfer(new AttachmentTransferError("protocol", "本用例不上传", true)),
    });
    await allocateReady(first);
    first.store.setPreparationDraft(SESSION_ID, "刷新前的草稿");
    const picked = await first.store.pickAttachments(SESSION_ID, "file");
    expect(picked.ok).toBe(true);
    await flush();
    await expect(drafts.readTabLocator()).resolves.toMatchObject({ sessionId: SESSION_ID });

    // “刷新”：同一份草稿/定位存储换一个 store（内存状态全丢，只剩平台端口）。
    const refreshed = preparationStore({
      drafts,
      media: singleAttachmentMedia(),
      transfers: failingTransfer(new AttachmentTransferError("protocol", "本用例不上传", true)),
    });
    await expect(refreshed.store.restoreTabPreparation()).resolves.toBe(SESSION_ID);
    expect(refreshed.store.preparationPage(SESSION_ID)?.text).toBe("刷新前的草稿");
    expect(refreshed.store.preparationPage(SESSION_ID)?.attachments.map((item) => item.fileName))
      .toEqual(["a.txt"]);
    expect(refreshed.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: SESSION_ID });

    // 刷新在正式会话 B 上：恢复槽位不能把 B 变成准备，也不得改绑到 B。
    const b = FORMAL_B_ID;
    await expect(refreshed.store.openPreparationSession(b)).resolves.toBe(false);
    expect(refreshed.store.state.preparations.has(b)).toBe(false);
    expect(refreshed.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: SESSION_ID });

    // 从别的目录点普通新建：导航回 A，不再发 allocate。
    const restoreFrames = refreshed.frames.length;
    await expect(refreshed.store.beginPreparation({ directoryId: "work-other", agentId: "codex-b" }))
      .resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    expect(refreshed.frames.filter((frame) => frame.action === "allocate")).toHaveLength(0);
    expect(refreshed.frames.length).toBe(restoreFrames);
    expect(refreshed.store.preparationPage(SESSION_ID)?.workspace.path).toBe("D:\\work\\api");
    expect(refreshed.store.preparationPage(SESSION_ID)?.agentId).toBe("codex-a");
  });

  it("复制标签页的定位不接管原槽位：只得到重复占用说明，普通新建仍可用", async () => {
    const drafts = fakeDrafts({
      duplicate: true,
      locator: {
        canonicalOrigin: ORIGIN,
        ownerNodeId: HOST_ID,
        deviceId: DEVICE_ID,
        tabSlotId: TAB_SLOT_ID,
        sessionId: SESSION_ID,
      },
    });
    const copied = preparationStore({ drafts });
    // 复制标签页从未持有槽位：恢复不报告会话，普通新建入口仍按「没有槽位」放行。
    await expect(copied.store.restoreTabPreparation()).resolves.toBeNull();
    expect(copied.store.preparationSlotRoute()).toEqual({ kind: "none" });
    expect(copied.store.activePreparationSessionId()).toBeNull();
    // 提示照常渲染，且不声称恢复出任何内容。
    expect(copied.store.preparationPage(SESSION_ID)?.notice).toContain("另一个标签页");
    expect(copied.store.preparationPage(SESSION_ID)?.text).toBe("");
    expect(copied.frames.filter((frame) => frame.action === "restore")).toHaveLength(0);
    expect(copied.frames.filter((frame) => frame.action === "allocate")).toHaveLength(0);

    // 点普通新建只发自己的 allocate，不被占用提示劫持到原会话。
    const begin = copied.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    const allocate = copied.frames.find((frame) => frame.action === "allocate");
    if (allocate === undefined) throw new Error("没有发出 allocate");
    copied.store.apply(readyResult({ operationId: allocate.operationId, sessionId: REPLACEMENT_ID, action: "allocate" }));
    await expect(begin).resolves.toEqual({ ok: true, sessionId: REPLACEMENT_ID });
    expect(copied.store.activePreparationSessionId()).toBe(REPLACEMENT_ID);
    expect(copied.store.preparationPage(SESSION_ID)?.notice).toContain("另一个标签页");
  });

  it("复制标签页的占用说明条目不提供/不执行放弃，也不清本标签页自己槽位的定位", async () => {
    const drafts = fakeDrafts({
      // 只有被复制的原会话被另一个文档实例占用；本标签页为自己新分配的槽位仍能持有归属。
      duplicate: (sessionId) => sessionId === SESSION_ID,
      locator: tabLocatorFor(SESSION_ID),
    });
    const copied = preparationStore({ drafts });
    // 打开原会话 URL：只得到说明条目（无凭据、无 Agent，本页从未持有它的资源）。
    await expect(copied.store.openPreparationSession(SESSION_ID)).resolves.toBe(true);
    expect(copied.store.preparationPage(SESSION_ID)?.notice).toContain("另一个标签页");

    // 本标签页照常新建自己的槽位 R；草稿保存把定位改写成 R。
    const begin = copied.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    const allocate = copied.frames.find((frame) => frame.action === "allocate");
    if (allocate === undefined) throw new Error("没有发出 allocate");
    copied.store.apply(readyResult({ operationId: allocate.operationId, sessionId: REPLACEMENT_ID, action: "allocate" }));
    await expect(begin).resolves.toEqual({ ok: true, sessionId: REPLACEMENT_ID });
    copied.store.setPreparationDraft(REPLACEMENT_ID, "本标签页的草稿");
    await flush();
    await expect(drafts.readTabLocator()).resolves.toMatchObject({ sessionId: REPLACEMENT_ID });

    // 说明条目不是槽位：不提供放弃；即使调用也既不作废草稿，也不清掉 R 的唯一定位。
    expect(copied.store.canAbandonPreparation(SESSION_ID)).toBe(false);
    await copied.store.abandonPreparation(SESSION_ID);
    expect(drafts.invalidated).toBe(0);
    await expect(drafts.readTabLocator()).resolves.toMatchObject({ sessionId: REPLACEMENT_ID });
    expect(copied.store.preparationPage(SESSION_ID)?.notice).toContain("另一个标签页");
    expect(copied.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: REPLACEMENT_ID });
    // 真正占槽的临时会话仍然可以放弃。
    expect(copied.store.canAbandonPreparation(REPLACEMENT_ID)).toBe(true);
  });

  it("启动恢复在途时两个新建入口只发一帧 allocate", async () => {
    let openBeginTab!: () => void;
    const beginTabGate = new Promise<void>((resolve) => {
      openBeginTab = resolve;
    });
    // 定位指向一个已经没有草稿的记录：恢复最终不产生槽位，两个入口都会在恢复上等待。
    const drafts = fakeDrafts({ locator: tabLocatorFor(SESSION_ID), beginTabGate });
    const harness = preparationStore({ drafts });
    const restoring = harness.store.restoreTabPreparation();
    await flush();
    const first = harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    const second = harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    await flush();
    expect(harness.frames.filter((frame) => frame.action === "allocate")).toHaveLength(0);

    openBeginTab();
    await expect(restoring).resolves.toBeNull();
    await flush();
    const allocations = harness.frames.filter((frame) => frame.action === "allocate");
    expect(allocations).toHaveLength(1);
    harness.store.apply(readyResult({ operationId: allocations[0]!.operationId, sessionId: SESSION_ID, action: "allocate" }));
    await expect(first).resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    await expect(second).resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    expect(harness.frames.filter((frame) => frame.action === "allocate")).toHaveLength(1);
  });

  it("慢恢复晚于新分配落地时被丢弃：槽位只留新分配", async () => {
    let openBeginTab!: () => void;
    const beginTabGate = new Promise<void>((resolve) => {
      openBeginTab = resolve;
    });
    const drafts = fakeDrafts({
      locator: tabLocatorFor(SESSION_ID),
      beginTabGate,
      load: {
        status: "active",
        record: {
          revision: 2,
          payload: {
            status: "active",
            content: {
              text: "刷新前的输入",
              mode: null,
              configOptions: {},
              agentId: "codex-a",
              workspace: { directoryId: "work-api", path: "D:\\work\\api", source: "directoryId" },
              receipt: receiptFor(SESSION_ID),
              pendingRequestId: null,
            },
            attachments: [],
          },
        },
      },
    });
    const harness = preparationStore({ drafts });
    // 启动恢复卡在 `beginTab`：这一刻槽位还是空的。
    const restoring = harness.store.openPreparationSession(SESSION_ID);
    await flush();
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "none" });

    // 用户在这个窗口里点普通新建：分配先落地。
    const begin = harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" });
    const allocate = harness.frames.find((frame) => frame.action === "allocate");
    if (allocate === undefined) throw new Error("没有发出 allocate");
    harness.store.apply(readyResult({ operationId: allocate.operationId, sessionId: REPLACEMENT_ID, action: "allocate" }));
    await expect(begin).resolves.toEqual({ ok: true, sessionId: REPLACEMENT_ID });

    // 慢恢复随后完成：不得插入第二个槽位条目，也不得再发 restore。
    openBeginTab();
    await expect(restoring).resolves.toBe(false);
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.store.activePreparationSessionId()).toBe(REPLACEMENT_ID);
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: REPLACEMENT_ID });
    expect(harness.frames.filter((frame) => frame.action === "restore")).toHaveLength(0);
  });

  it("显式放弃：清草稿与定位、release 旧资源；随后换目录新建得到新 ID，迟到旧应答不复活", async () => {
    const drafts = fakeDrafts({});
    const harness = preparationStore({ drafts });
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "要放弃的草稿");
    await flush();
    const oldAllocate = harness.frames.find((frame) => frame.action === "allocate")!;

    await harness.store.abandonPreparation(SESSION_ID);
    expect(drafts.invalidated).toBe(1);
    await expect(drafts.readTabLocator()).resolves.toBeNull();
    expect(harness.frames.some((frame) => frame.action === "release" && frame.sessionId === SESSION_ID)).toBe(true);
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "none" });
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();

    // 换目录后普通新建：新 ID（不把旧证明用于新目录）。
    const begin = harness.store.beginPreparation({ directoryId: "personal-api", agentId: "codex-b" });
    const replacement = harness.frames.at(-1);
    if (replacement === undefined || replacement.action !== "allocate") throw new Error("缺少新目录的分配");
    expect(replacement.workspace).toEqual({ directoryId: "personal-api" });
    harness.store.apply(readyResult({ operationId: replacement.operationId, sessionId: REPLACEMENT_ID, action: "allocate" }));
    await expect(begin).resolves.toEqual({ ok: true, sessionId: REPLACEMENT_ID });
    expect(harness.store.activePreparationSessionId()).toBe(REPLACEMENT_ID);

    // 迟到的旧分配/准备应答不能复活旧槽位。
    harness.store.apply(readyResult({ operationId: oldAllocate.operationId, sessionId: SESSION_ID, action: "allocate" }));
    await flush();
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: REPLACEMENT_ID });
  });

  it("未确认放弃时返回目录/普通新建保留原槽位", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "还没放弃");
    harness.store.leaveConversationSession(SESSION_ID);
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "session", sessionId: SESSION_ID });
    await expect(harness.store.beginPreparation({ directoryId: "work-other", agentId: "codex-b" }))
      .resolves.toEqual({ ok: true, sessionId: SESSION_ID });
    expect(harness.store.preparationPage(SESSION_ID)?.text).toBe("还没放弃");
    expect(harness.store.canAbandonPreparation(SESSION_ID)).toBe(true);
  });

  it("restoring 阶段确认放弃：restore 结果在等待期间落地后，仍用最新资源代发 release", async () => {
    let openInvalidate!: () => void;
    const invalidateGate = new Promise<void>((resolve) => {
      openInvalidate = resolve;
    });
    const drafts = fakeDrafts({
      locator: tabLocatorFor(SESSION_ID),
      invalidateGate,
      load: {
        status: "active",
        record: {
          revision: 1,
          payload: {
            status: "active",
            content: {
              text: "恢复中的草稿",
              mode: null,
              configOptions: {},
              agentId: "codex-a",
              workspace: { directoryId: "work-api", path: "D:\\work\\api", source: "directoryId" },
              receipt: receiptFor(SESSION_ID),
              pendingRequestId: null,
            },
            attachments: [],
          },
        },
      },
    });
    const harness = preparationStore({ drafts });
    void harness.store.openPreparationSession(SESSION_ID);
    await flush();
    const restore = harness.frames.find((frame) => frame.action === "restore");
    if (restore === undefined) throw new Error("没有发出 restore");
    expect(harness.store.preparationPage(SESSION_ID)?.phase).toBe("restoring");

    // 放弃在草稿作废处停住；这期间 restore 结果到达，把 runtimeEpoch/generation 写进状态。
    const abandoning = harness.store.abandonPreparation(SESSION_ID);
    await flush();
    harness.store.apply(readyResult({ operationId: restore.operationId, sessionId: SESSION_ID, action: "restore" }));
    await flush();
    openInvalidate();
    await expect(abandoning).resolves.toBeUndefined();

    expect(drafts.invalidated).toBe(1);
    // 旧快照此刻仍是 `null`/`null`，只有最新快照能发出释放帧。
    const release = harness.frames.find((frame) => frame.action === "release");
    expect(release?.sessionId).toBe(SESSION_ID);
    expect(harness.store.preparationPage(SESSION_ID)).toBeNull();
  });

  it("资源过期（stale）：先核实原请求，再同 ID restore，不分配第二个", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    const allocateCount = harness.frames.filter((frame) => frame.action === "allocate").length;
    const configured = harness.store.selectPreparationMode(SESSION_ID, "compact");
    expect(configured.ok).toBe(true);
    const configure = harness.frames.find((frame) => frame.action === "configure");
    if (configure === undefined) throw new Error("缺少 configure 帧");
    harness.store.apply(failedResult({
      operationId: configure.operationId,
      sessionId: SESSION_ID,
      action: "configure",
      code: "preparation.stale",
      retryable: false,
    }));
    await flush();
    expect(harness.frames.some((frame) => frame.action === "restore" && frame.sessionId === SESSION_ID)).toBe(true);
    expect(harness.frames.filter((frame) => frame.action === "allocate")).toHaveLength(allocateCount);
  });

  it("接受转正后槽位退出：再次显式新建才分配新 ID", async () => {
    const drafts = fakeDrafts({});
    const harness = preparationStore({ drafts });
    await allocateReady(harness);
    await flush();
    const firstAllocate = harness.frames.find((frame) => frame.action === "allocate")!;
    // 服务端确认同 ID 已正式受理：槽位退出临时角色。
    harness.store.apply(preparationEvent({
      operationId: firstAllocate.operationId,
      action: "restore",
      sessionId: SESSION_ID,
      status: "committed",
      firstSubmissionId: "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
      firstRequestId: "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e",
      currentRequestId: "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f",
      firstMessageId: "3d4e5f6a-7b8c-4d9e-8f0a-1b2c3d4e5f6a",
    }));
    expect(harness.store.preparationSlotRoute()).toEqual({ kind: "none" });
    const begin = harness.store.beginPreparation({ directoryId: "personal-api", agentId: "codex-b" });
    const replacement = harness.frames.at(-1);
    if (replacement === undefined || replacement.action !== "allocate") throw new Error("缺少转正后的新分配");
    harness.store.apply(readyResult({ operationId: replacement.operationId, sessionId: REPLACEMENT_ID, action: "allocate" }));
    await expect(begin).resolves.toEqual({ ok: true, sessionId: REPLACEMENT_ID });
  });
});

// ── 复审修复：续租、重连恢复、显式继续与 D6 动作 ──────────────────────────────

/** 手控定时器：每条排期都可见，用例按间隔推进（准备续租 10s、草稿合并 350ms）。 */
function manualSchedule(): {
  readonly schedule: (delayMs: number, task: () => void) => () => void;
  readonly pending: (delayMs: number) => number;
  readonly runDelay: (delayMs: number) => number;
} {
  const tasks: { readonly delayMs: number; readonly task: () => void; readonly entry: object }[] = [];
  return {
    schedule(delayMs, task) {
      const entry = {};
      tasks.push({ delayMs, task, entry });
      return () => {
        const index = tasks.findIndex((candidate) => candidate.entry === entry);
        if (index >= 0) tasks.splice(index, 1);
      };
    },
    pending(delayMs) {
      return tasks.filter((candidate) => candidate.delayMs === delayMs).length;
    },
    runDelay(delayMs) {
      const due = tasks.filter((candidate) => candidate.delayMs === delayMs);
      for (const candidate of due) {
        const index = tasks.indexOf(candidate);
        if (index >= 0) tasks.splice(index, 1);
        candidate.task();
      }
      return due.length;
    },
  };
}

/** 一帧 `command.result`（用例直接投递，绕过 socket；归并与动作分支仍是生产代码）。 */
function commandResult(input: {
  readonly requestId: string;
  readonly command: "session.start" | "command.status";
  readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
  readonly result?: Record<string, unknown> | null;
  readonly error?: Record<string, unknown> | null;
}): SyncClientEvent {
  return {
    kind: "command_result",
    result: {
      protocolVersion: 1,
      type: "command.result",
      messageId: "cf1c2d3e-4a5b-4c6d-8e9f-0a1b2c3d4e5f",
      connectionId: CONNECTION_ID,
      connectionSequence: "15",
      body: {
        requestId: input.requestId,
        command: input.command,
        status: input.status,
        acceptedAt: "2026-10-22T08:00:00.000Z",
        terminalEventId: null,
        result: input.result ?? null,
        error: input.error ?? null,
      },
    },
  } as SyncClientEvent;
}

const FIRST_SUBMISSION_ID = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

function projection(input: { readonly currentRequestId: string; readonly canContinue: boolean }): Record<string, unknown> {
  return {
    stage: input.canContinue ? "saved_not_dispatched" : "dispatch_started",
    firstSubmissionId: FIRST_SUBMISSION_ID,
    firstRequestId: input.currentRequestId,
    currentRequestId: input.currentRequestId,
    everDispatched: !input.canContinue,
    canContinue: input.canContinue,
  };
}

/** 只选择得到一个附件的媒体端口（附件上传路径的入口）。 */
function singleAttachmentMedia(): AttachmentMediaPort {
  const handle = {
    localId: "local-1",
    kind: "file" as const,
    fileName: "a.txt",
    mediaType: "text/plain",
    byteLength: "4",
  };
  return {
    pick: async () => [handle],
    fromPaste: async () => [handle],
    readChunk: async (_handle, _offset, length) => new Uint8Array(length),
    previewUrl: () => null,
    createObjectUrl: () => null,
    releaseUrl: () => undefined,
  };
}

/** 上传必定失败（把服务端码原样抛出）的传输端口。 */
function failingTransfer(error: AttachmentTransferError): AttachmentTransferGateway {
  return {
    upload: async () => {
      throw error;
    },
    sealManifest: async () => ({ attachmentCount: "1", manifestDigest: "digest" }),
    read: async () => ({
      offset: "0",
      length: "0",
      totalLength: "0",
      data: new Uint8Array(0),
      eof: true,
    }),
    abort: () => undefined,
  };
}

describe("会话准备：续租与可见性", () => {
  it("ready 准备每 10 秒续租当前凭据/资源代；隐藏即停，返回可见重新 restore", async () => {
    const scheduler = manualSchedule();
    const harness = preparationStore({ schedule: scheduler.schedule });
    await allocateReady(harness);
    expect(scheduler.pending(PREPARATION_RENEW_INTERVAL_MS)).toBe(1);

    expect(scheduler.runDelay(PREPARATION_RENEW_INTERVAL_MS)).toBe(1);
    const renew = harness.frames.find((frame) => frame.action === "renew");
    expect(renew).toBeDefined();
    expect(renew).toMatchObject({
      action: "renew",
      sessionId: SESSION_ID,
      generation: "1",
      runtimeEpoch: "3f2a1b0c-9d8e-4f7a-8b6c-5d4e3f2a1b0c",
    });
    if (renew?.action !== "renew") return;
    expect(renew.receipt.claims.sessionId).toBe(SESSION_ID);

    // 续租应答（ready）刷新租约并继续排下一次。
    harness.store.apply(readyResult({ operationId: renew.operationId, sessionId: SESSION_ID, action: "renew" }));
    expect(scheduler.pending(PREPARATION_RENEW_INTERVAL_MS)).toBe(1);

    // 隐藏：计时器立即取消，不会在后台保活。
    harness.store.applyLifecycle({ kind: "visibility", state: "hidden" });
    expect(scheduler.pending(PREPARATION_RENEW_INTERVAL_MS)).toBe(0);

    // 返回可见：重连恢复路径重新 restore 同一 ID，而不是重发首条。
    harness.store.applyLifecycle({ kind: "visibility", state: "visible" });
    expect(harness.frames.filter((frame) => frame.action === "restore").length).toBe(1);
    expect(harness.commands).toHaveLength(0);
  });

  it("租约过期在页面上显式呈现并阻止发送", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    const page = harness.store.preparationPage(SESSION_ID, Date.parse("2026-10-22T08:01:00.000Z"));
    expect(page?.leaseExpired).toBe(true);
    expect(page?.input.canSend).toBe(false);
    expect(page?.input.reason).toContain("租约已过期");
  });
});

describe("会话准备：服务端失败后的显式继续", () => {
  it("failed 先核实原请求，再以新 requestId + kind:continue 显式继续（旧 requestId 不重发）", async () => {
    const harness = preparationStore({});
    await allocateReady(harness);
    harness.store.setPreparationDraft(SESSION_ID, "hi");
    const first = harness.store.submitPreparation(SESSION_ID, "hi");
    expect(first.ok).toBe(true);
    await flush();
    const initial = harness.commands.find((command) => command.body.command === "session.start");
    if (initial?.body.command !== "session.start") throw new Error("没有发 initial");
    const originalRequestId = initial.body.requestId;

    harness.store.apply(commandResult({
      requestId: originalRequestId,
      command: "session.start",
      status: "failed",
      error: { code: "internal.unavailable", message: "保存后派发失败", retryable: true, details: {} },
    }));
    const failed = harness.store.preparationPage(SESSION_ID);
    expect(failed?.phase).toBe("failed");
    expect(harness.store.state.preparations.get(SESSION_ID)?.failureOrigin).toBe("server_start");
    expect(failed?.input.canSend).toBe(true);

    // 首次提交身份未知 / 不可继续：只查原请求，绝不重发旧 requestId。
    const verify = harness.store.submitPreparation(SESSION_ID, "hi");
    expect(verify.ok).toBe(true);
    await flush();
    const status = harness.commands.at(-1);
    expect(status?.body.command).toBe("command.status");
    if (status?.body.command !== "command.status") return;
    expect(status.body.payload.targetRequestId).toBe(originalRequestId);
    expect(harness.commands.filter((command) => command.body.command === "session.start")).toHaveLength(1);

    // 状态查询带回投影：可证明未派发 → 下一次显式发送走 continue。
    harness.store.apply(commandResult({
      requestId: status.body.requestId,
      command: "command.status",
      status: "completed",
      result: {
        targetRequestId: originalRequestId,
        state: "failed",
        acceptedAt: "2026-10-22T08:00:00.000Z",
        terminalAt: "2026-10-22T08:00:05.000Z",
        terminalEventId: null,
        result: null,
        error: null,
        preparation: projection({ currentRequestId: originalRequestId, canContinue: true }),
      },
    }));
    expect(harness.store.state.preparations.get(SESSION_ID)?.canContinue).toBe(true);

    const continued = harness.store.submitPreparation(SESSION_ID, "hi");
    expect(continued.ok).toBe(true);
    await flush();
    const starts = harness.commands.filter((command) => command.body.command === "session.start");
    expect(starts).toHaveLength(2);
    const body = starts[1]!.body;
    if (body.command !== "session.start") return;
    expect(body.requestId).not.toBe(originalRequestId);
    if (body.payload.kind !== "continue") throw new Error("继续必须走 kind:continue");
    expect(body.payload.firstSubmissionId).toBe(FIRST_SUBMISSION_ID);
    expect(body.payload.retryOfRequestId).toBe(originalRequestId);
    expect(body.payload).not.toHaveProperty("content");
    expect(body.payload).not.toHaveProperty("receipt");
  });
});

describe("会话准备：D6 动作与端口缺失", () => {
  it("附件已经正式受理（preparation.already_committed）：停止上传并 restore，不重传", async () => {
    const media = singleAttachmentMedia();
    const harness = preparationStore({
      media,
      transfers: failingTransfer(
        new AttachmentTransferError("already_committed", "已受理", false, {}, "preparation.already_committed"),
      ),
    });
    await allocateReady(harness);
    await harness.store.pickAttachments(SESSION_ID, "file");
    harness.store.setPreparationDraft(SESSION_ID, "带附件");
    expect(harness.store.submitPreparation(SESSION_ID, "带附件").ok).toBe(true);
    await flush();
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.phase).toBe("restoring");
    expect(page?.notice).toContain("不会重传附件");
    expect(harness.frames.some((frame) => frame.action === "restore" && frame.sessionId === SESSION_ID)).toBe(true);
    expect(harness.commands.filter((command) => command.body.command === "session.start")).toHaveLength(0);
  });

  it("存储重建（preparation.store_rebuilt）：丢弃凭据并要求回目录", async () => {
    const harness = preparationStore({
      media: singleAttachmentMedia(),
      transfers: failingTransfer(
        new AttachmentTransferError("store_rebuilt", "存储已重建", false, {}, "preparation.store_rebuilt"),
      ),
    });
    await allocateReady(harness);
    await harness.store.pickAttachments(SESSION_ID, "file");
    harness.store.setPreparationDraft(SESSION_ID, "带附件");
    harness.store.submitPreparation(SESSION_ID, "带附件");
    await flush();
    const page = harness.store.preparationPage(SESSION_ID);
    expect(page?.phase).toBe("unavailable");
    expect(page?.restartRequired).toBe(true);
    expect(harness.store.state.preparations.get(SESSION_ID)?.receipt).toBeNull();
  });

  it("缺少标签页 nonce 端口：入口明确不可用，beginPreparation 不抛错", async () => {
    const harness = preparationStore({ tabNonce: false });
    expect(harness.store.preparationEntryAvailability()).toMatchObject({ available: false });
    await expect(harness.store.beginPreparation({ directoryId: "work-api", agentId: "codex-a" }))
      .resolves.toMatchObject({ ok: false });
    expect(harness.frames).toEqual([]);
  });

  it("选择/移除附件都会排一次准备草稿保存（刷新恢复包含附件）", async () => {
    let saves = 0;
    const drafts = fakeDrafts({ onSave: () => { saves += 1; } });
    const harness = preparationStore({
      drafts,
      media: singleAttachmentMedia(),
      transfers: failingTransfer(new AttachmentTransferError("protocol", "本用例不上传", true)),
    });
    await allocateReady(harness);
    await flush();
    const baseline = saves;
    await harness.store.pickAttachments(SESSION_ID, "file");
    await flush();
    expect(saves).toBeGreaterThan(baseline);
    const afterAdd = saves;
    const attachment = harness.store.state.attachments.get(SESSION_ID)?.[0];
    expect(attachment).toBeDefined();
    harness.store.removeAttachment(SESSION_ID, attachment!.localId);
    await flush();
    expect(saves).toBeGreaterThan(afterAdd);
  });
});
