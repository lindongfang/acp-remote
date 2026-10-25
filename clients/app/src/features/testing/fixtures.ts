/**
 * 组件层测试的共享夹具。
 *
 * 这里的每一份数据都**刻意**带上违规输入可能携带的字段（目录项上的 `path`、
 * 事件上的未知 `eventType`、原文的 `rawUnavailable`），这样负向断言才有意义：
 * 组件必须把它们原样丢掉或明确说明，而不是渲染成「看起来完整」的内容。
 *
 * 夹具只提供数据与构造器，不复制任何被测判定逻辑。
 */

import type {
  CommandMessage,
  CommandResultMessage,
  EffectiveInteraction,
  EventMessage,
  RawAcp,
  SessionSummary,
  SnapshotItemConfigOptions,
  SnapshotItemHistory,
  SnapshotItemMessages,
  WorkspaceCatalogEntry,
} from "../../protocol";
import type { CompletedSnapshot, StagedResources } from "../../sync-client";
import { SESSION_HISTORY_FEATURE } from "../../sync-client";
import type { InteractionView } from "../../domain/interaction";
import { ClientStore } from "../client-store";
import type { SyncGateway } from "../ports";

/**
 * 目录条目夹具：`{ directoryId, path }`（当前合同）。
 *
 * 这里仍按 open object（双断言）声明，是因为条目还刻意带了一个协议之外的 `root` 键（多带的键是
 * 文件头说的那类负向断言的原料）：`as const satisfies readonly WorkspaceCatalogEntry[]` 会把这个
 * 多余属性判成错（TS2353），装不下这一形状。消费方只读 `directoryId` 与 `path`。
 */
export const WORKSPACES_WITH_PATHS = [
  { directoryId: "work-api", path: "D:\\work\\api", root: "D:\\work" },
  { directoryId: "personal-api", path: "D:\\personal\\api", root: "D:\\personal" },
] as unknown as readonly WorkspaceCatalogEntry[];

/** 两条尾段相同的目录（展示名都由路径尾段派生为 `api`）。 */
export const WORKSPACES = [
  { directoryId: "work-api", path: "D:\\work\\api" },
  { directoryId: "personal-api", path: "D:\\personal\\api" },
] as const satisfies readonly WorkspaceCatalogEntry[];

/** 会话摘要构造器。 */
export function session(input: {
  readonly id: string;
  readonly state?: SessionSummary["state"];
  readonly directoryId?: string | null;
  readonly title?: string | null;
  readonly origin?: SessionSummary["origin"];
  /** 摘要里的当前模式；只在清单与 `session.mode.changed` 都缺席时才轮到它（回落链最末）。 */
  readonly currentMode?: SessionSummary["currentMode"];
  readonly createdAt?: string;
  readonly updatedAt?: string;
  readonly localActivity?: SessionSummary["localActivity"];
}): SessionSummary {
  const directoryId = input.directoryId === undefined ? "work-api" : input.directoryId;
  return {
    sessionId: input.id,
    title: input.title === undefined ? `会话 ${input.id.slice(-4)}` : input.title,
    agent: { agentId: "claude", name: "Claude" },
    state: input.state ?? "idle",
    ...(input.localActivity === undefined ? {} : { localActivity: input.localActivity }),
    origin: input.origin ?? { kind: "local" },
    currentMode: input.currentMode ?? null,
    version: "1",
    createdAt: input.createdAt ?? "2026-10-01T00:00:00.000Z",
    updatedAt: input.updatedAt ?? "2026-10-01T00:00:00.000Z",
    workspace: directoryId === null ? null : { directoryId },
  };
}

/**
 * 跨月夹具：两条 `work-api` 下的会话分属 2026-09 与 2026-10。
 *
 * 详情页是一条**按时间倒序的平铺列表**（规格「目录详情按时间倒序分页」替换了旧的月份分档），
 * 因此这两个月份的会话必须落在同一条列表里、十月那条排在前面；若实现还按月份分档或按档过滤，
 * `directory-filter.test.ts` 的「跨月的会话落在同一条列表里」与最近活动时间断言会随之变红。
 */
export const CROSS_MONTH_SESSIONS: readonly SessionSummary[] = [
  session({
    id: "eeeeeeee-0000-4000-8000-000000000001",
    title: "九月的重构",
    state: "idle",
    updatedAt: "2026-09-30T23:00:00.000Z",
  }),
  session({
    id: "eeeeeeee-0000-4000-8000-000000000002",
    title: "十月的发布",
    state: "running",
    updatedAt: "2026-10-02T08:00:00.000Z",
  }),
];

/**
 * 最近活动时间夹具：同一个目录下的会话**故意不按时间排列**，且**全局最晚的一条在别的目录**。
 *
 * `work-api` 的三条依次为 `09-30`（首）/ `10-02`（中，最晚）/ `08-15`（末），`personal-api`
 * 有一条 `12-31` 的会话。于是四种错法各自得到不同的值：
 *
 * - 取数组第一条 → `09-30`；取最后一条 → `08-15`；取全局最大值 → `12-31`（别的目录的会话）；
 *   取当前时间 → 运行当天。只有「该目录下 `updatedAt` 的最大值」等于 `10-02`。
 *
 * 用法见 `directory-filter.test.ts` 的「目录页最近活动时间」。
 */
export const ACTIVITY_SESSIONS: readonly SessionSummary[] = [
  session({
    id: "ffffffff-0000-4000-8000-000000000001",
    title: "早先的一次提交",
    state: "idle",
    directoryId: "work-api",
    updatedAt: "2026-09-30T23:00:00.000Z",
  }),
  session({
    id: "ffffffff-0000-4000-8000-000000000002",
    title: "最晚的一次提交",
    state: "running",
    directoryId: "work-api",
    updatedAt: "2026-10-02T08:00:00.000Z",
  }),
  session({
    id: "ffffffff-0000-4000-8000-000000000003",
    title: "最早的一次提交",
    state: "idle",
    directoryId: "work-api",
    updatedAt: "2026-08-15T01:00:00.000Z",
  }),
  session({
    id: "ffffffff-0000-4000-8000-000000000004",
    title: "别的目录的跨年会话",
    state: "idle",
    directoryId: "personal-api",
    updatedAt: "2026-12-31T23:59:59.000Z",
  }),
];

/** 默认快照资源：两个同名目录、两条会话、两个同名 Agent。 */
export function resources(input: {
  readonly sessions?: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceCatalogEntry[];
  readonly agents?: StagedResources["agents"];
}): StagedResources {
  return {
    sessions: input.sessions ?? [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" })],
    workspaces: input.workspaces ?? WORKSPACES,
    agents: input.agents ?? [
      { agentId: "codex-a", displayName: "codex", default: true },
      { agentId: "codex-b", displayName: "codex", default: false },
    ],
  };
}

/** 记账式同步网关。 */
export interface RecordingGateway extends SyncGateway {
  readonly dispatches: CommandMessage[];
  /** `session.view` 声明（按发出顺序）。 */
  readonly views: { readonly sessionId: string | null; readonly viewVersion: number }[];
  snapshotRefreshes: number;
}

export function recordingGateway(input: {
  readonly resources: StagedResources | null;
  readonly connectionId?: string | null;
}): RecordingGateway {
  const dispatches: CommandMessage[] = [];
  const views: { readonly sessionId: string | null; readonly viewVersion: number }[] = [];
  const current: CompletedSnapshot | null =
    input.resources === null
      ? null
      : {
          snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
          cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
          resources: input.resources,
          verifiedAtMs: 1_700_000_000_000,
        };
  return {
    dispatches,
    views,
    snapshotRefreshes: 0,
    refreshSnapshot() { this.snapshotRefreshes += 1; },
    current,
    // 认证结果（`deviceId` / serverEpoch / 限额）由 `onlineStore` 自己发一条 `authenticated` 事件
    // 交付；wire 上没有授权集合，因此这里没有任何 scope 可给，也不编造一份。
    authenticatedInfo: null,
    dispatch(command: CommandMessage) {
      dispatches.push(command);
    },
    sendPreparation() {
      // 记账网关默认不接准备控制面：用例需要它时自己替换这个方法（见准备流程用例）。
      return false;
    },
    setSessionView(input) {
      views.push({ sessionId: input.sessionId, viewVersion: input.viewVersion });
      return true;
    },
    subscribeRemote: () => undefined,
    unsubscribeRemote: () => undefined,
  };
}

/** 一个已喂入数据的仓库与它的记账网关。 */
export interface StoreFixture {
  readonly store: ClientStore;
  readonly gateway: RecordingGateway;
}

/** 建一个已喂入快照的仓库。 */
export function storeWith(input: {
  readonly resources: StagedResources | null;
  readonly connectionId?: string | null;
  /**
   * 注入的时钟；缺省是一个固定时刻（`1_700_000_000_000`）。
   *
   * 相对时间由视图模型按注入的 `nowMs` 算出（D3），因此要断言「几分钟前 / 年月日」的用例传入
   * 一份与会话时刻相配的时钟，而不是去打桩全局 `Date`。
   */
  readonly now?: () => number;
  /** 文件栏刷新退避的定时端口；缺省不排期（测试直接调用 `refreshFiles`）。 */
  readonly schedule?: (delayMs: number, task: () => void) => () => void;
}): StoreFixture {
  const gateway = recordingGateway(input);
  let counter = 0;
  const store = new ClientStore({
    sync: gateway,
    ids: {
      uuid(): string {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    now: input.now ?? (() => 1_700_000_000_000),
    // 生产组合根总是注入这个端口；测试仓库缺省给同一形状，避免「入口不可用」把无关用例带红。
    preparationTabNonce: () => TEST_TAB_NONCE,
    // `??` 会把「显式传 null（未连接）」当成缺省，因此这里区分 undefined 与 null。
    identity: { connectionId: input.connectionId === undefined ? "2de54db7-74ae-4c21-88e3-05df0dc2e637" : input.connectionId },
    ...(input.schedule === undefined ? {} : { schedule: input.schedule }),
  });

  if (input.resources !== null) {
    store.apply({
      kind: "snapshot_verified",
      snapshot: {
        ...(gateway.current as CompletedSnapshot),
        chunkCount: 3,
        receivedChunks: 3,
        snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
      },
    });
  }
  return { store, gateway };
}

/** 一条事件消息（降级用例的原料）。 */
export function eventMessage(input: {
  readonly eventType: string;
  readonly eventId?: string;
  readonly sessionId?: string | null;
  readonly view?: Record<string, unknown>;
  readonly acp?: RawAcp;
  readonly globalSequence?: string;
  readonly createdAt?: string;
  readonly causationRequestId?: string | null;
}): EventMessage {
  const payload: { view: Record<string, unknown>; acp?: RawAcp } = { view: input.view ?? {} };
  if (input.acp !== undefined) payload.acp = input.acp;
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${(input.globalSequence ?? "1").padStart(12, "0")}`,
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      globalSequence: input.globalSequence ?? "1",
      sessionSequence: null,
      eventId: input.eventId ?? "dddddddd-0000-4000-8000-000000000001",
      sessionId: input.sessionId ?? null,
      eventType: input.eventType,
      causationRequestId: input.causationRequestId ?? null,
      origin: { kind: "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
      payload,
    },
  };
}

/** `session.read` 的一条消息项。 */
export function messageItem(input: {
  readonly id: string;
  readonly createdAt: string;
  readonly role?: "user" | "agent";
  readonly text?: string;
  readonly blocks?: SnapshotItemMessages["content"];
}): SnapshotItemMessages {
  return {
    messageId: input.id,
    sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
    role: input.role ?? "user",
    content: input.blocks ?? [{ type: "text", text: input.text ?? "你好" }],
    status: "done",
    createdAt: input.createdAt,
    turnId: null,
  };
}

/** 测试用的设备标识（`auth.authenticated.deviceId`）：决定「本设备处理」的判定。 */
export const TEST_DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/** 测试用的服务端 epoch。 */
export const TEST_SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";

/** 测试用的准备标签页 nonce（32 字节 base64url；与生产组合根注入的端口同形）。 */
export const TEST_TAB_NONCE = "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8";

/**
 * 已认证、已在线、并（可选）协商到若干 feature 的仓库。
 *
 * 结构化历史的 `history` include 只在**服务端选中**对应 feature 时才带，因此要测历史读取的
 * 用例必须走这一条路；`storeWith` 不带 `selectedFeatures`（等价于什么都没协商出来）。
 */
export function onlineStore(input: {
  readonly resources?: StagedResources;
  readonly features?: readonly string[];
  /** 注入的时钟；见 {@link storeWith}。 */
  readonly now?: () => number;
  /** 续租计时器端口（用例据推进 10 秒续租）；缺省不排期。 */
  readonly schedule?: (delayMs: number, task: () => void) => () => void;
}): StoreFixture {
  const { store, gateway } = storeWith({
    resources: input.resources ?? resources({}),
    ...(input.now === undefined ? {} : { now: input.now }),
    ...(input.schedule === undefined ? {} : { schedule: input.schedule }),
  });
  // 发一次认证结果，带上服务端在 `auth.server_challenge` 里选中的 feature。
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: TEST_DEVICE_ID,
      serverEpoch: TEST_SERVER_EPOCH,
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: [...(input.features ?? [])],
  });
  // 走完整条连接路径（单独发 `online` 会被迁移表拒绝，能力判定随之失真）。
  store.pairSucceeded();
  store.apply({ kind: "authenticating" });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: TEST_SERVER_EPOCH, globalSequence: "10" } });
  return { store, gateway };
}

/** `session.read` 的一条结构化历史条目（`snapshotItem.history`）。 */
export function historyItem(input: {
  readonly eventId: string;
  readonly eventType: string;
  readonly sessionSequence?: string;
  readonly createdAt?: string;
  readonly view?: Record<string, unknown>;
  readonly acp?: RawAcp;
}): SnapshotItemHistory {
  return {
    eventId: input.eventId,
    eventType: input.eventType,
    sessionSequence: input.sessionSequence ?? "1",
    createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
    view: input.view ?? {},
    ...(input.acp === undefined ? {} : { acp: input.acp }),
  };
}

/** `session.read` 的 completed 结果。 */
export function readResult(input: {
  readonly requestId: string;
  readonly sessionId?: string;
  readonly messages?: readonly SnapshotItemMessages[];
  readonly history?: readonly SnapshotItemHistory[];
  readonly pending?: readonly Record<string, unknown>[];
  readonly configOptions?: readonly SnapshotItemConfigOptions[];
  readonly hasEarlier?: boolean;
  readonly historyHasEarlier?: boolean;
}): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-0000000000ff",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.read",
      status: "completed",
      acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result: {
        sessionId: input.sessionId ?? "aaaaaaaa-0000-4000-8000-000000000001",
        resources: {
          ...(input.messages === undefined ? {} : { messages: input.messages }),
          ...(input.history === undefined ? {} : { history: input.history }),
          ...(input.pending === undefined
            ? {}
            : { pending_interactions: input.pending as unknown as readonly EffectiveInteraction[] }),
          ...(input.configOptions === undefined ? {} : { config_options: input.configOptions }),
        },
        hasEarlier: input.hasEarlier ?? false,
        ...(input.historyHasEarlier === undefined ? {} : { historyHasEarlier: input.historyHasEarlier }),
      },
      error: null,
    },
  };
}

/** 取最后一条派发的命令（便于断言出站载荷）。 */
export function lastDispatch(gateway: RecordingGateway): CommandMessage {
  const command = gateway.dispatches[gateway.dispatches.length - 1];
  if (command === undefined) throw new Error("没有任何派发记录");
  return command;
}

/** 交互与输入用例共用的会话标识：快照里它处于 `running`，因此有进行中的轮次。 */
export const ONLINE_SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";

/**
 * 没有已认证连接的仓库（`connectionId === null`）。
 *
 * 命令信封需要 `connectionId`，因此任何「要发命令」的意图都必须在这里闸住——用例据此断言
 * 「未连接时**没有**任何派发记录」。
 */
export function storeWithNoConnection(): StoreFixture {
  return storeWith({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }), connectionId: null });
}

/** 一个已在线、协商到结构化历史、快照里有 `running` 会话的仓库（交互与输入用例的起点）。 */
export function interactionStore(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
    features: [SESSION_HISTORY_FEATURE],
  });
}

/** 取一条交互的呈现模型（不在本会话的交互里即 `null`）。 */
export function interactionViewOf(store: ClientStore, interactionId: string): InteractionView | null {
  return (
    store.conversationPage(ONLINE_SESSION_ID)?.interactions.find((item) => item.interactionId === interactionId) ?? null
  );
}
