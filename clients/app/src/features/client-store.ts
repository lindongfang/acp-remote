/**
 * 客户端状态仓库：把 `src/sync-client/` 的事件流与命令结果收敛成页面可渲染的模型。
 *
 * ## 这一层做什么、不做什么
 *
 * **做**：把 `SyncClientEvent` 归并进一份不可变状态，并从状态投影出各页面的视图模型。
 * **不做**：不持有 socket、不碰 IndexedDB/WebCrypto、不自己判定该不该重连——重连与阻断
 * 由 `src/state/` 的连接状态机决定（`shouldAutoReconnect`），本页只读它的结论。
 *
 * ## 明细一律现取（`design.md` D8）
 *
 * 会话正文**只**来自 `session.read`：事件侧不拼接正文、不做增量合并。因此事件面只消费四类
 * 与正文无关的信息——会话摘要（目录汇总）、用量（R17 上下文条）、Agent 连接覆盖层（R19）、
 * 降级事件（R18）。这样就不存在「快照里的明细」与「读回来的明细」两套数据需要合并。
 *
 * ## 命令出口只有一个
 *
 * 用户意图（创建会话、读取明细、加载更早）都经 `#dispatchCommand`，它用**稳定 `requestId`**
 * 并把记录写进 `commands`；终态由 `command_result` 回填（R13）。
 */

import type {
  CommandMessage,
  CommandResultMessage,
  CommandResultValue,
  EventMessage,
  SessionReadInclude,
  SessionReadResult,
  SessionSummary,
  SnapshotItemMessages,
  Uuid,
} from "../protocol";
import type { StagedResources, SyncClientEvent } from "../sync-client";
import { AgentConnectionOverlayStore, assertPagesDoNotOverlap, toLoadedPage } from "../sync-client";
import type { CommandRecord, ConnectionEvent, ConnectionMachineState } from "../state";
import {
  IllegalConnectionTransition,
  applyCommandResult,
  createCommand,
  initialConnectionState,
  isBlockingState,
  transition,
} from "../state";
import type { AgentCatalogEntryView, SessionReadPageView } from "../domain";
import { buildAgentCatalogView, buildSessionReadPageView } from "../domain";
import type { AgentConnectionOverlay } from "../domain/agent-catalog";
import type { ConnectionIdentity, IdentifierSource, SyncGateway } from "./ports";
import type { ConversationHeaderModel, ConversationPageModel, UsageReading } from "./conversation-model";
import { buildConversationModel, conversationCapabilities } from "./conversation-model";
import type { DegradedEventModel } from "./degradation-model";
import { buildDegradedEventModel, isDegradedEvent } from "./degradation-model";
import type { CreateSessionEntryModel, CreateSessionRefs } from "./create-session-model";
import {
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  buildCreateSessionEntryModel,
} from "./create-session-model";
import type { DirectoryDetailModel, DirectoryPageModel } from "./directory-model";
import { buildDirectoryDetailModel, buildDirectoryPageModel } from "./directory-model";
import type { HostIdentityModel, HostPanelModel } from "./host-panel-model";
import { buildHostPanelModel } from "./host-panel-model";
import type { PairingPageModel } from "./pairing-model";
import { initialPairingPageModel } from "./pairing-model";

/** v1 对话页只请求 `messages`：明细一律现取，不预取能力/配置/交互等资源（D8）。 */
const READ_MESSAGES: readonly SessionReadInclude[] = ["messages"];

/** 一个会话的明细线程（分页现取的结果）。 */
export interface ConversationThreadState {
  readonly page: SessionReadPageView | null;
  /** 已读到的原始消息项（`session.read` 的元素形状）：向上翻页的不重不漏校验以它为准。 */
  readonly items: readonly SnapshotItemMessages[];
  readonly loadingEarlier: boolean;
  readonly pagingError: string | null;
}

/** 一次 `session.read` 的用途：首屏还是向上翻页。二者的结果合并方式不同。 */
type ReadIntent = "first" | "earlier";

/** 待完成的读取请求。 */
interface PendingRead {
  readonly sessionId: Uuid;
  readonly intent: ReadIntent;
}

/** 客户端状态（不可变快照）。 */
export interface ClientState {
  readonly connection: ConnectionMachineState;
  /** 本次认证授予的 scopes；未认证为 `null`（创建入口据此判定不可用）。 */
  readonly scopes: readonly string[] | null;
  /** 最后一次**验证通过并完成替换**的快照资源；断连时仍是目录页的唯一数据来源。 */
  readonly resources: StagedResources | null;
  readonly lastSyncedAtMs: number | null;
  readonly overlay: readonly AgentConnectionOverlay[];
  readonly degraded: readonly DegradedEventModel[];
  /** 每个会话最近一次用量事件。键**不存在**即「从未收到用量事件」（R17 第一态）。 */
  readonly usage: ReadonlyMap<Uuid, UsageReading>;
  readonly threads: ReadonlyMap<Uuid, ConversationThreadState>;
  readonly commands: ReadonlyMap<Uuid, CommandRecord>;
  /** 每个目录当前那次创建会话的 `requestId`；未提交过的目录没有键。 */
  readonly createRequestIds: ReadonlyMap<string, Uuid>;
  /** 结果不确定的 `requestId`：保留到用户主动处理，不随时间或重连清除。 */
  readonly uncertainRequestIds: readonly Uuid[];
  readonly createSheetAlias: string | null;
  readonly diagnosticsEventId: string | null;
  readonly pairing: PairingPageModel;
  readonly host: HostIdentityModel | null;
}

/** 初始状态：未配对、没有快照。 */
export function initialClientState(): ClientState {
  return {
    connection: initialConnectionState(),
    scopes: null,
    resources: null,
    lastSyncedAtMs: null,
    overlay: [],
    degraded: [],
    usage: new Map(),
    threads: new Map(),
    commands: new Map(),
    createRequestIds: new Map(),
    uncertainRequestIds: [],
    createSheetAlias: null,
    diagnosticsEventId: null,
    pairing: initialPairingPageModel(),
    host: null,
  };
}

/**
 * 应用一个连接机事件；迁移表不接受时保持原状态。
 *
 * 「保持原状态」而不是抛错，是因为关闭之后仍可能到达迟到的 `online`/`closed`
 * （旧 socket 的回调）。迁移表仍是唯一判定源——只是不把这类竞态升级成崩溃。
 */
function applyConnectionEvent(current: ConnectionMachineState, event: ConnectionEvent): ConnectionMachineState {
  try {
    return transition(current, event);
  } catch (error) {
    if (error instanceof IllegalConnectionTransition) return current;
    throw error;
  }
}

/** 会话摘要 → 替换 `resources.sessions` 里的同 id 条目（目录汇总随之更新）。 */
function withSessionSummary(resources: StagedResources | null, summary: SessionSummary): StagedResources | null {
  if (resources === null) return resources;
  const rest = resources.sessions.filter((item) => item.sessionId !== summary.sessionId);
  return { ...resources, sessions: [summary, ...rest] };
}

/** `session.created`/`session.updated` 视图里的会话摘要。 */
function sessionSummaryIn(event: EventMessage): SessionSummary | null {
  if (event.body.eventType !== "session.created" && event.body.eventType !== "session.updated") return null;
  const summary = event.body.payload.view["session"];
  if (typeof summary !== "object" || summary === null || !("sessionId" in summary)) return null;
  const candidate = summary.sessionId;
  if (typeof candidate !== "string" || candidate.length === 0) return null;
  return summary as SessionSummary;
}

/** `session.usage.changed` 视图里的两项计数。 */
function usageIn(event: EventMessage): UsageReading | null {
  if (event.body.eventType !== "session.usage.changed") return null;
  const view = event.body.payload.view;
  const used = view["used"];
  const size = view["size"];
  if (typeof used !== "string" || typeof size !== "string") return null;
  return { used, size };
}

/**
 * 事件是否需要降级卡片（R18）：未识别、原文未下发、显式不支持、引用未下发的二进制内容。
 *
 * 判定经 {@link isDegradedEvent}，它按 `payload.acp` 得出原文可用性。早先这里硬编码
 * `raw: {kind:"absent"}`，使「事件类型已知 + 原文未下发」这一组合永远判不出降级，
 * 生产路径上 `raw_not_synced` 分支不可达——用户看到的是静默丢失（R18 MUST 禁止）。
 */
function degradedModelFor(event: EventMessage): DegradedEventModel | null {
  if (!isDegradedEvent(event)) return null;
  return buildDegradedEventModel({ event });
}

/** `CommandResultValue` → 可展开成键值对象的结果；非对象结果（如 `modeList`）取 `null`。 */
function toResultRecord(value: CommandResultValue): Readonly<Record<string, unknown>> | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Readonly<Record<string, unknown>>;
}

/** 会话标题（空标题呈现为未命名，`design.md` D7：标题单向由 Agent 通知写入）。 */
function titleOf(summary: SessionSummary | undefined): string {
  const title = summary?.title;
  return title === undefined || title === null || title.length === 0 ? "未命名会话" : title;
}

/** imported 会话来源离线时的标注（R20：只显示元数据）。 */
function originOfflineLabelOf(
  summary: SessionSummary | undefined,
  ownerLabelOf: (ownerNodeId: string) => string,
): string | null {
  if (summary?.origin.kind !== "remote" || summary.origin.online) return null;
  return `来源「${ownerLabelOf(summary.origin.ownerNodeId)}」当前离线`;
}

/** 命令失败的说明（结构化：码 + 消息 + 可重试）。 */
function describeCommandFailure(result: CommandResultMessage): string {
  const error = result.body.error;
  if (error === null) return `${result.body.command} 未成功（${result.body.status}）`;
  return error.retryable ? `${error.code}：${error.message}（可重试）` : `${error.code}：${error.message}`;
}

/** `ClientStore` 的构造输入。 */
export interface ClientStoreOptions {
  readonly sync: SyncGateway;
  readonly ids: IdentifierSource;
  readonly now: () => number;
  /** 已建立连接的标识；未连接为 `null`（此时不派发任何命令）。 */
  readonly identity: ConnectionIdentity;
  /** imported 会话来源节点的展示名。 */
  readonly ownerLabelOf?: (ownerNodeId: string) => string;
}

type Listener = () => void;

/** 提交结果：一个 `requestId`，或拒绝原因（此时**没有**派发任何命令）。 */
export type CreateOutcome =
  | { readonly ok: true; readonly requestId: Uuid }
  | { readonly ok: false; readonly reason: string };

/**
 * 客户端状态仓库。
 *
 * 一个实例覆盖整个应用生命周期：组合根把 `SyncClient` 作为 `sync` 传进来，
 * 并把 `onEvent` 接到 `apply`。页面只读它的选择器结果与用户意图方法。
 */
export class ClientStore {
  readonly #sync: SyncGateway;
  readonly #ids: IdentifierSource;
  readonly #now: () => number;
  readonly #identity: ConnectionIdentity;
  readonly #ownerLabelOf: (ownerNodeId: string) => string;
  readonly #overlayStore = new AgentConnectionOverlayStore();
  readonly #pendingReads = new Map<Uuid, PendingRead>();
  readonly #listeners = new Set<Listener>();
  #state: ClientState = initialClientState();

  constructor(options: ClientStoreOptions) {
    this.#sync = options.sync;
    this.#ids = options.ids;
    this.#now = options.now;
    this.#identity = options.identity;
    this.#ownerLabelOf = options.ownerLabelOf ?? ((ownerNodeId: string) => ownerNodeId);
  }

  /** 当前状态。 */
  get state(): ClientState {
    return this.#state;
  }

  /** 订阅状态变化（React 的 `useSyncExternalStore` 与页面共用这一入口）。 */
  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  /**
   * 摄入一条 `SyncClientEvent`。
   *
   * 连接、快照、事件、命令结果四类都在这里归并；`reset_required` 刻意**不**清空
   * `resources`——§9.4 要求验证失败时保留上一次已完成的状态。
   */
  apply(event: SyncClientEvent): void {
    const next = this.#reduce(event);
    if (next === this.#state) return;
    this.#commit(next);
  }

  #reduce(event: SyncClientEvent): ClientState {
    switch (event.kind) {
      case "authenticated":
        return { ...this.#state, scopes: [...event.info.scopes] };
      case "authenticating": {
        // 一条事件对应连接机的两步：开始一次连接尝试（connecting），随后 socket 打开进入认证（authenticating）。
        const started = applyConnectionEvent(this.#state.connection, {
          kind: "connect_started",
          fromReconnect: this.#state.connection.state === "reconnecting",
        });
        return { ...this.#state, connection: applyConnectionEvent(started, { kind: "socket_opened" }) };
      }
      case "replaying":
        return this.#withConnection({ kind: "subscribed" });
      case "online":
        return this.#withConnection({ kind: "caught_up" });
      case "closed":
        return this.#withConnection({ kind: "connection_closed", retryable: event.retryable });
      case "blocked":
        return this.#withConnection({ kind: "blocked", cause: event.cause });
      case "snapshot_verified":
        return { ...this.#state, resources: event.snapshot.resources, lastSyncedAtMs: this.#now() };
      case "event":
        return this.#withEvent(event.event);
      case "command_result":
        return this.#withCommandResult(event.result);
      default:
        return this.#state;
    }
  }

  #withConnection(event: ConnectionEvent): ClientState {
    return { ...this.#state, connection: applyConnectionEvent(this.#state.connection, event) };
  }

  /** 一条事件的状态归并：会话摘要、用量、Agent 覆盖层、降级卡片。 */
  #withEvent(event: EventMessage): ClientState {
    let next = this.#state;

    const summary = sessionSummaryIn(event);
    if (summary !== null) next = { ...next, resources: withSessionSummary(next.resources, summary) };

    const usage = usageIn(event);
    const sessionId = event.body.sessionId;
    if (usage !== null && sessionId !== null) {
      const usageMap = new Map(next.usage);
      usageMap.set(sessionId, usage);
      next = { ...next, usage: usageMap };
    }

    // R19：连接事件只进**覆盖层**；列表本体始终来自快照。
    if (this.#overlayStore.ingest(event) !== null) {
      next = { ...next, overlay: this.#overlayStore.entries() };
    }

    const degraded = degradedModelFor(event);
    if (degraded !== null) next = { ...next, degraded: [...next.degraded, degraded] };

    return next;
  }

  /** 命令结果归并：终态回填 + `session.read` 结果并入线程 + 结果不确定登记。 */
  #withCommandResult(result: CommandResultMessage): ClientState {
    const record =
      this.#state.commands.get(result.body.requestId) ??
      createCommand({ requestId: result.body.requestId, command: result.body.command });
    const updated = applyCommandResult(record, {
      requestId: result.body.requestId,
      status: result.body.status,
      acceptedAt: result.body.acceptedAt,
      terminalEventId: result.body.terminalEventId,
      // `CommandResultValue` 是开放联合；命令记录只保存可展开成键值对象的那部分结果。
      result: toResultRecord(result.body.result),
      error: result.body.error,
    });
    const commands = new Map(this.#state.commands);
    commands.set(updated.requestId, updated);

    let next: ClientState = { ...this.#state, commands };
    if (updated.state === "uncertain" && !next.uncertainRequestIds.includes(updated.requestId)) {
      // 结果不确定**只**在服务端确认后登记，且没有任何自动清除路径（R16 场景 4）。
      next = { ...next, uncertainRequestIds: [...next.uncertainRequestIds, updated.requestId] };
    }
    return this.#applyReadResult(next, result);
  }

  /** `session.read` 结果并入线程：首屏替换，向上翻页前置并校验不重不漏。 */
  #applyReadResult(state: ClientState, result: CommandResultMessage): ClientState {
    const pending = this.#pendingReads.get(result.body.requestId);
    if (pending === undefined) return state;
    this.#pendingReads.delete(result.body.requestId);
    const threads = new Map(state.threads);
    const existing = threads.get(pending.sessionId) ?? {
      page: null,
      items: [],
      loadingEarlier: false,
      pagingError: null,
    };

    if (result.body.command !== "session.read" || result.body.status !== "completed") {
      threads.set(pending.sessionId, { ...existing, loadingEarlier: false, pagingError: describeCommandFailure(result) });
      return { ...state, threads };
    }

    const loaded = toLoadedPage(result.body.result as SessionReadResult);
    const sessionId = pending.sessionId;
    if (pending.intent === "first" || existing.page === null) {
      threads.set(sessionId, {
        page: buildSessionReadPageView(loaded.result),
        items: loaded.messages,
        loadingEarlier: false,
        pagingError: null,
      });
      return { ...state, threads };
    }

    // 向上翻页：不重不漏由 `assertPagesDoNotOverlap` 把关，违反即抛而不是静默拼接。
    assertPagesDoNotOverlap(existing.items, loaded.messages);
    threads.set(sessionId, {
      page: buildSessionReadPageView({
        sessionId,
        resources: { messages: [...loaded.messages, ...existing.items] },
        hasEarlier: loaded.hasEarlier,
      }),
      items: [...loaded.messages, ...existing.items],
      loadingEarlier: false,
      pagingError: null,
    });
    return { ...state, threads };
  }

  // ── 选择器 ────────────────────────────────────────────────────────────────

  /** 目录页模型（R15）。数据只来自快照，不发任何命令。 */
  directoryPage(): DirectoryPageModel {
    return buildDirectoryPageModel({
      resources: this.#state.resources,
      connection: this.#state.connection.state,
      lastSyncedAtMs: this.#state.lastSyncedAtMs,
    });
  }

  /** 目录详情模型（R15）。 */
  directoryDetail(alias: string): DirectoryDetailModel {
    return buildDirectoryDetailModel({
      resources: this.#state.resources,
      connection: this.#state.connection.state,
      alias,
      ownerLabelOf: this.#ownerLabelOf,
    });
  }

  /** 本机已配置的 Agent 目录（快照 `agents` + 连接覆盖层，R19）。 */
  agentCatalog(): readonly AgentCatalogEntryView[] {
    return buildAgentCatalogView(this.#state.resources?.agents ?? [], this.#state.overlay).entries;
  }

  /** 创建弹层模型（R16）。 */
  createEntry(alias: string): CreateSessionEntryModel {
    const requestId = this.#state.createRequestIds.get(alias);
    return buildCreateSessionEntryModel({
      directoryAlias: alias,
      scopes: this.#state.scopes,
      agents: this.agentCatalog(),
      record: requestId === undefined ? null : (this.#state.commands.get(requestId) ?? null),
      connectionOnline: this.#state.connection.state === "online",
    });
  }

  /** 对话页模型（R17 + R18 + 翻页）。会话既不在快照里也没有线程时为 `null`。 */
  conversationPage(sessionId: Uuid): ConversationPageModel | null {
    const thread = this.#state.threads.get(sessionId);
    const summary = this.#findSession(sessionId);
    if (thread === undefined && summary === undefined) return null;

    const header: ConversationHeaderModel = {
      sessionId,
      title: titleOf(summary),
      agentName: summary?.agent.name ?? "",
      state: summary?.state ?? "idle",
      directoryAlias: summary?.workspace?.alias ?? null,
      originOfflineLabel: originOfflineLabelOf(summary, this.#ownerLabelOf),
    };
    const origin: "local" | { readonly kind: "remote"; readonly online: boolean } =
      summary?.origin.kind === "remote" ? { kind: "remote", online: summary.origin.online } : "local";

    return buildConversationModel({
      header,
      page: thread?.page ?? null,
      usage: this.#state.usage.get(sessionId) ?? null,
      degraded: this.#state.degraded.filter((item) => item.sessionId === sessionId),
      loadingEarlier: thread?.loadingEarlier ?? false,
      pagingError: thread?.pagingError ?? null,
      capabilities: conversationCapabilities({
        connectionOnline: this.#state.connection.state === "online",
        sessionState: summary?.state ?? "idle",
        origin,
      }),
    });
  }

  /** 主机与连接面板模型（R19）。 */
  hostPanel(): HostPanelModel {
    return buildHostPanelModel({
      resources: this.#state.resources,
      overlay: this.#state.overlay,
      connection: this.#state.connection.state,
      blocking: this.#state.connection.blocking,
      host: this.#state.host,
    });
  }

  /** 配对页模型。 */
  pairingPage(): PairingPageModel {
    return this.#state.pairing;
  }

  /** 诊断抽屉模型：选中降级事件的原文与元数据（R18）。 */
  diagnostics(): DegradedEventModel | null {
    const id = this.#state.diagnosticsEventId;
    if (id === null) return null;
    return this.#state.degraded.find((item) => item.eventId === id) ?? null;
  }

  /** 是否处于阻断态（页面据此停止一切主动动作）。 */
  get blocked(): boolean {
    return isBlockingState(this.#state.connection.state);
  }

  // ── 用户意图 ──────────────────────────────────────────────────────────────

  /** 打开创建弹层。 */
  openCreateSheet(alias: string): void {
    this.#commit({ ...this.#state, createSheetAlias: alias });
  }

  /** 关闭创建弹层。 */
  closeCreateSheet(): void {
    this.#commit({ ...this.#state, createSheetAlias: null });
  }

  /** 打开诊断抽屉。 */
  openDiagnostics(eventId: string): void {
    this.#commit({ ...this.#state, diagnosticsEventId: eventId });
  }

  /** 关闭诊断抽屉。 */
  closeDiagnostics(): void {
    this.#commit({ ...this.#state, diagnosticsEventId: null });
  }

  /** 更新配对页状态。 */
  setPairing(pairing: PairingPageModel): void {
    this.#commit({ ...this.#state, pairing });
  }

  /**
   * 记录已配对的主机身份。
   *
   * 它是**状态**而不是构造参数：组合根先装配 store、再异步读 IndexedDB 里的设备身份与配对
   * 记录，主机身份因此在 store 构造之后才变得可用；构造期定格会让主机面板在真实运行时永远
   * 显示「未配对」。未配对时保持 `null`——不编造地址与指纹（R19）。
   */
  setHostIdentity(host: HostIdentityModel): void {
    this.#commit({ ...this.#state, host });
  }

  /**
   * 配对完成（由配对页在收到 `approved` 后调用）。
   *
   * 连接机要求 `unpaired --pair_started--> pairing --pair_succeeded--> disconnected`，
   * 之后才允许发起连接；跳过这两步的话，后续的 `connect_started` 会被迁移表拒绝，
   * 页面就会一直停在「未配对」。
   */
  pairSucceeded(): void {
    const pairing = applyConnectionEvent(this.#state.connection, { kind: "pair_started" });
    this.#commit({ ...this.#state, connection: applyConnectionEvent(pairing, { kind: "pair_succeeded" }) });
  }

  /** 用户主动处理结果不确定——**唯一**能把它移出列表的动作。 */
  dismissUncertain(requestId: Uuid): void {
    this.#commit({
      ...this.#state,
      uncertainRequestIds: this.#state.uncertainRequestIds.filter((id) => id !== requestId),
    });
  }

  /**
   * 提交创建会话。
   *
   * 四道闸门，任一不过即**不发命令**：
   * 1. 已建立连接（命令信封需要 `connectionId`）；
   * 2. 权限（scopes）——R16 要求无权限时入口不可用；
   * 3. Agent 必须在本机已配置的目录里（快照 `agents`），不是「用户输入什么就发什么」；
   * 4. 载荷精确两键（`assertCreateSessionRefs`）。
   */
  submitCreateSession(refs: CreateSessionRefs): CreateOutcome {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return { ok: false, reason: "尚未建立已认证连接，无法提交命令" };

    const entry = this.createEntry(refs.workspaceAlias);
    if (entry.permission.kind === "denied") return { ok: false, reason: entry.permission.reason };
    if (!entry.agents.some((agent) => agent.id === refs.agentId)) {
      return { ok: false, reason: "所选 Agent 不在本机已配置的 Agent 目录里" };
    }
    assertCreateSessionRefs({ workspaceAlias: refs.workspaceAlias, agentId: refs.agentId });

    const requestId = this.#ids.uuid();
    const commands = new Map(this.#state.commands);
    commands.set(requestId, createCommand({ requestId, command: "session.create" }));
    const createRequestIds = new Map(this.#state.createRequestIds);
    createRequestIds.set(refs.workspaceAlias, requestId);
    this.#commit({ ...this.#state, commands, createRequestIds });

    this.#sync.dispatch(
      buildCreateSessionCommand({
        refs,
        requestId,
        messageId: this.#ids.uuid(),
        connectionId,
      }),
    );
    return { ok: true, requestId };
  }

  /** 首次读取会话明细（明细分页，默认只取 messages）。 */
  loadSession(sessionId: Uuid): void {
    if (this.#pendingReads.size > 0) return;
    this.#requestPage(sessionId, "first");
  }

  /** 向上翻页：续取游标取上一页最早一条的 `(createdAt, messageId)`。 */
  loadEarlier(sessionId: Uuid): void {
    const before = this.#state.threads.get(sessionId)?.page?.earliestCursor ?? null;
    if (before === null) return;
    this.#requestPage(sessionId, "earlier", before);
  }

  #requestPage(sessionId: Uuid, intent: ReadIntent, before?: { createdAt: string; messageId: Uuid }): void {
    const connectionId = this.#identity.connectionId;
    if (connectionId === null) return;

    const requestId = this.#ids.uuid();
    this.#pendingReads.set(requestId, { sessionId, intent });
    const threads = new Map(this.#state.threads);
    const existing = threads.get(sessionId) ?? {
      page: null,
      items: [] as readonly SnapshotItemMessages[],
      loadingEarlier: false,
      pagingError: null,
    };
    threads.set(sessionId, { ...existing, loadingEarlier: intent === "earlier", pagingError: null });
    this.#commit({ ...this.#state, threads });

    const command: CommandMessage = {
      protocolVersion: 1,
      type: "command",
      messageId: this.#ids.uuid(),
      connectionId,
      // 占位：真正的出站序号由 `SyncConnection.sendCommand` 覆盖（§4.1）。
      connectionSequence: "0",
      body: {
        requestId,
        command: "session.read",
        sessionId,
        // 与 `paging.ts` 的 `buildReadPayload` 同一形状；这里就地构造是为了让
        // `include` 保持 `SessionReadInclude[]`（协议类型），而 `PageRequest` 声明的是
        // 更宽的 `string[]`。`before` 是复合游标，两键都由上一页最早一条给出。
        payload: before === undefined ? { include: READ_MESSAGES } : { include: READ_MESSAGES, before },
      },
    };
    this.#sync.dispatch(command);
  }

  #findSession(sessionId: Uuid): SessionSummary | undefined {
    return this.#state.resources?.sessions.find((summary) => summary.sessionId === sessionId);
  }

  #commit(next: ClientState): void {
    if (next === this.#state) return;
    this.#state = next;
    for (const listener of [...this.#listeners]) listener();
  }
}
