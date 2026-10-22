/**
 * 用例共享的快照夹具与路由条目。
 *
 * ## 目录身份：`directoryId` + `path`
 *
 * 快照的目录条目现在是协议里的 `{ directoryId, path }`（`WorkspaceCatalogEntry`，schema 明确
 * `additionalProperties: false`）：`directoryId` 是不透明标识，只用于路由、命令载荷与键；`path`
 * 是服务端下发的逐字路径，展示名与同名消歧段都由它派生（见 `src/domain/directory.ts`）。
 *
 * 因此夹具**只含协议字段**：旧形状（`alias`/`displayName`，或已删除的独立路径 feature）会被
 * 真实解码边界拒收，夹具不能再用它们。
 */

import type { ReactElement } from "react";

import type { SessionSummary, WorkspaceCatalogEntry } from "../../src/protocol";
import DirectoriesRoute from "../../app/index";
import DirectoryDetailRoute from "../../app/dir/[directoryId]";
import HostsRoute from "../../app/hosts";
import NodeLinkPairRoute from "../../app/node-link/pair";
import PairRoute from "../../app/pair";
import SessionRoute from "../../app/session/[id]";
import type { RouteEntry } from "./router-stub";
import type { StagedResources } from "../../src/sync-client";


/** 违规路径片段：出现在任何一处渲染结果里都判红。 */
export const SECRET_FRAGMENTS = ["D:\\", "secret", "vault", "\\work"] as const;

/**
 * 只代表**真实宿主路径**的片段（不含示例盘符）：创建弹层的目录提示会渲染 `如 D:\work\api` 这样的
 * **示例**，因此那一处只能按这一份子集判泄漏——目录页/详情页没有目录输入，仍用 {@link SECRET_FRAGMENTS} 全表。
 */
export const HOST_PATH_FRAGMENTS = ["secret", "vault", "\\work"] as const;

/**
 * 两条目录标识：32 位十六进制（与协议 `directoryId` 的形状一致）。
 *
 * 它们只作路由参数、命令载荷与 `data-directory-id` 锚点；任何界面文本里都不得出现。
 */
export const DIRECTORY_ID_ALPHA = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
export const DIRECTORY_ID_BETA = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

/**
 * 两条**同尾段**的目录路径：展示名都是 `api`，靠更多路径段 `alpha` / `beta` 消歧。
 *
 * 这是本轮保留的「同名目录」夹具：展示名与消歧段都逐字取自路径，因此断言可以逐字比对。
 */
export const DIRECTORY_PATH_ALPHA = "D:\\secret\\vault\\alpha\\api";
export const DIRECTORY_PATH_BETA = "D:\\secret\\vault\\beta\\api";

/**
 * 两条同名目录：身份字段就是协议里的 `{ directoryId, path }`。
 *
 * `WORKSPACES_WITH_PATHS` 是历史名字（路径现在始终随条目下发），与 {@link WORKSPACES} 是同一份
 * 协议形状；保留两个导出只为既有调用点不必为改名而改。
 */
export const WORKSPACES = [
  { directoryId: DIRECTORY_ID_ALPHA, path: DIRECTORY_PATH_ALPHA },
  { directoryId: DIRECTORY_ID_BETA, path: DIRECTORY_PATH_BETA },
] as const satisfies readonly WorkspaceCatalogEntry[];

export const WORKSPACES_WITH_PATHS: readonly WorkspaceCatalogEntry[] = WORKSPACES;


/** 两条同名 Agent（`R19` 的同名区分与 `R16` 的候选来源）。 */
export const AGENTS = [
  { agentId: "codex-a", displayName: "codex", default: true },
  { agentId: "codex-b", displayName: "codex", default: false },
] as const satisfies StagedResources["agents"];

/** 目录 alpha 下的会话（运行中）。 */
export const SESSION_ALPHA = "aaaaaaaa-0000-4000-8000-00000000aa01";

/** 目录 beta 下的会话（等待输入 → 计入待处理）。 */
export const SESSION_BETA = "aaaaaaaa-0000-4000-8000-00000000bb01";

/**
 * 构造一条会话摘要。
 *
 * `title` / `updatedAt` 是**可选**覆盖：缺省值与既有调用一字不差（`会话 <id 末两位>` +
 * 固定的 2026-10-01），因此既有用例的期望不受影响。搜索用例需要能逐字断言的标题
 * （目录页按会话标题匹配），分页用例需要**会话自身**的时间（倒序切片与相对时间文案都落在
 * 它上面），两者都不能靠 id 末两位或当前时间凑出来。
 */
export function sessionSummary(input: {
  readonly id: string;
  readonly directoryId: string;
  readonly state: SessionSummary["state"];
  /** 呈现标题；缺省 `会话 <id 末两位>`。 */
  readonly title?: string;
  /** 会话自身的时间（`createdAt`/`updatedAt` 同值）；缺省 2026-10-01T00:00:00.000Z。 */
  readonly updatedAt?: string;
  /**
   * 会话来源；缺省本机自有（`{kind:"local"}`，与既有调用一字不差）。
   *
   * 来源为其它节点的会话用来断言「结构化历史在客户端显式降级」「草稿只在内存、输入不被标记为已发送」
   * ——那两条判定都以 `origin.kind === "remote"` 为前提（`conversation-model.ts` 的
   * `structuredHistoryAvailability` 与 `conversationCapabilities`）。
   */
  readonly origin?: SessionSummary["origin"];
  /** 可选的本机活跃程度（只在协商 `core.local-session-lifecycle.v1` 的用例里给出）。 */
  readonly localActivity?: SessionSummary["localActivity"];
}): SessionSummary {
  const at = input.updatedAt ?? "2026-10-01T00:00:00.000Z";
  return {
    sessionId: input.id,
    title: input.title ?? `会话 ${input.id.slice(-2)}`,
    agent: { agentId: "codex-a", name: "codex" },
    state: input.state,
    ...(input.localActivity === undefined ? {} : { localActivity: input.localActivity }),
    origin: input.origin ?? { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: at,
    updatedAt: at,
    workspace: { directoryId: input.directoryId },
  };
}

/**
 * 一个目录下的 `titles.length` 条会话：标题、时间与状态逐条给出。
 *
 * `idPrefix` 是 8 位十六进制（如 `aaaaaaaa`），序号补成 12 位，因此 id 是**可断言的确定值**
 * （`seriesSessionId`），不必把 UUID 抄进用例。`at` 决定两件事：目录页内联会话行的顺序（它
 * 与相对时间文案同源）与详情页按页取片时的倒序——两条路径都按会话**自身**的时间排序。
 */
export function sessionSeries(input: {
  readonly directoryId: string;
  readonly idPrefix: string;
  readonly titles: readonly string[];
  /** 与 `titles` 等长的 ISO 时间；应逐条递减（最新在前），与实际快照的顺序一致。 */
  readonly at: readonly string[];
  /** 与 `titles` 等长的状态；缺省全部 `idle`（既不计运行中也不计待处理）。 */
  readonly states?: readonly SessionSummary["state"][];
}): readonly SessionSummary[] {
  if (input.titles.length !== input.at.length) throw new Error("sessionSeries：titles 与 at 必须等长");
  if (input.states !== undefined && input.states.length !== input.titles.length) {
    throw new Error("sessionSeries：states 与 titles 必须等长");
  }
  return input.titles.map((title, index) =>
    sessionSummary({
      id: seriesSessionId(input.idPrefix, index),
      directoryId: input.directoryId,
      title,
      updatedAt: input.at[index] ?? "",
      state: input.states?.[index] ?? "idle",
    }),
  );
}

/** `sessionSeries` 第 `index` 条的会话 id；用例据此逐字断言顺序与集合。 */
export function seriesSessionId(idPrefix: string, index: number): string {
  return `${idPrefix}-0000-4000-8000-${String(index + 1).padStart(12, "0")}`;
}

/** 两条目录各一条会话：alpha 运行中（运行中 +1），beta 等待输入（待处理 +1）。 */
export function sessionsForBothDirectories(): readonly SessionSummary[] {
  return [
    sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" }),
    sessionSummary({ id: SESSION_BETA, directoryId: DIRECTORY_ID_BETA, state: "pending" }),
  ];
}

/** 目录页路由。 */
export const DIRS_ROUTE: RouteEntry = { path: "/", component: DirectoriesRoute as RouteComponent, params: {} };

/**
 * 配对页路由（`app/pair.tsx`，`data-route="pair"`）。
 *
 * 与其它路由一样走**正常路径**（design D4）：配对内容留在片段里，不参与路由命中；
 * 页面挂载后由 `startPairingFromAddress` 读取片段并**立即清除**。
 * 「未配对」起点的用例（`checks/pairing-first-run.ts`）把二维码地址的片段挂到
 * `location.hash` 上再打开这一页，走真实配对链路。
 */
export const PAIR_ROUTE: RouteEntry = { path: "/pair", component: PairRoute as RouteComponent, params: {} };

/**
 * node-link 配对路由（`app/node-link/pair.tsx`）。
 *
 * 服务端为本机 node-link 配对生成的二维码指向这一条路径（`NODE_LINK_PROTOCOL.md` §13.1）：
 * 它复用 `app/pair.tsx` 的页面主体，只把期望的配对类型声明成 `node-link`。
 * `checks/pairing-kinds.ts` 用它验「打开该地址读到 node-link 内容」与「两类入口互不混淆」。
 */
export const NODE_LINK_PAIR_ROUTE: RouteEntry = {
  path: "/node-link/pair",
  component: NodeLinkPairRoute as RouteComponent,
  params: {},
};

/**
 * 主机与连接页路由（`app/hosts.tsx`）。
 *
 * 节点级降级事件（没有 `sessionId` 的 `device.revoked` 等）的唯一出口就在这一面的
 * `data-node-degraded-entry` 列表上：不把这一页挂起来，「协议已知但客户端没有专用视图」的
 * **节点级**事件有没有出口就无法被断言。
 */
export const HOSTS_ROUTE: RouteEntry = { path: "/hosts", component: HostsRoute as RouteComponent, params: {} };

/** 目录详情路由（`/dir/<directoryId>`）。 */
export function dirRoute(directoryId: string): RouteEntry {
  return {
    path: `/dir/${directoryId}`,
    component: DirectoryDetailRoute as RouteComponent,
    params: { directoryId },
  };
}

/** 会话详情路由。 */
export function sessionRoute(sessionId: string): RouteEntry {
  return {
    path: `/session/${sessionId}`,
    component: SessionRoute as RouteComponent,
    params: { id: sessionId },
  };
}

/** 路由组件的最小形状（`RouteEntry.component` 就是它）。 */
type RouteComponent = () => ReactElement;

/**
 * 两条夹具目录的详情路由：跨路由用例把它们放进 `ScenarioOptions.extraRoutes`。
 *
 * `navigate` 对**未注册**的路径静默忽略（真实 expo-router 会渲染未匹配路由，替身不需要），
 * 因此漏给 `extraRoutes` 的红法是一个 10 秒的等待超时，而不是「路由没注册」这种一眼能看懂的
 * 失败。`registerRoutes` 每条用例整表替换（不跨用例残留），所以这里必须显式给出。
 */
export const DIRECTORY_ROUTES: readonly RouteEntry[] = [
  dirRoute(DIRECTORY_ID_ALPHA),
  dirRoute(DIRECTORY_ID_BETA),
];

/**
 * 一条 `event` 消息原文。
 *
 * `globalSequence` 必须**严格递增且大于屏障**，否则会被去重账本判成回退/重复而丢弃——
 * 那会让降级卡片凭空消失，用例变成「断言了一个从未投递的事件」。
 *
 * `sessionId` 为 `null` 表示**节点级**事件（如 `device.revoked`）：它们没有会话时间线可挂，
 * 只能落在主机/连接面的节点级降级列表。`createdAt` 缺省与既有调用一字不差。
 *
 * `origin` 缺省与既有调用一字不差（`{ kind: "agent", deviceId: null }`）：它是**加性**入参，
 * 只为让用例把协议允许的其它来源种类送到真实连接上（例如 `kind: "node"` 的形状照
 * `fixtures/sync/v1/valid/event-node-origin.json`）。既有检查集的取值因此逐字不变。
 */
export function wireEvent(input: {
  readonly globalSequence: number;
  readonly eventId: string;
  readonly sessionId: string | null;
  readonly eventType: string;
  readonly view?: Record<string, unknown>;
  readonly acp?: Record<string, unknown>;
  readonly createdAt?: string;
  /** 事件来源（`event.body.origin`）；缺省 `{ kind: "agent", deviceId: null }`。 */
  readonly origin?: Record<string, unknown>;
}): Record<string, unknown> {
  const payload: { view: Record<string, unknown>; acp?: Record<string, unknown> } = {
    view: input.view ?? {},
  };
  if (input.acp !== undefined) payload.acp = input.acp;
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${String(input.globalSequence).padStart(12, "0")}`,
    connectionSequence: "1",
    body: {
      globalSequence: String(input.globalSequence),
      sessionSequence: null,
      eventId: input.eventId,
      sessionId: input.sessionId,
      eventType: input.eventType,
      causationRequestId: null,
      origin: input.origin ?? { kind: "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: input.createdAt ?? "2026-10-01T00:00:02.000Z",
      payload,
    },
  };
}

/** `session.read` 的空页应答（让会话页的线程有内容，避免把线程错误误当成降级问题）。 */
export function emptyReadResult(sessionId: string): Record<string, unknown> {
  return { sessionId, resources: { messages: [] }, hasEarlier: false };
}

/** `session.read` 里的一条消息（wire 形状：`snapshotItem.messages`）。 */
export function readMessage(input: {
  readonly messageId: string;
  readonly sessionId: string;
  readonly role: "user" | "agent";
  readonly text: string;
  readonly createdAt: string;
  readonly turnId?: string | null;
}): Record<string, unknown> {
  return {
    messageId: input.messageId,
    sessionId: input.sessionId,
    role: input.role,
    content: [{ type: "text", text: input.text }],
    status: "completed",
    createdAt: input.createdAt,
    turnId: input.turnId ?? null,
  };
}

/**
 * 一条结构化历史条目（wire 形状：`snapshotItem.history`）。
 *
 * 与实时事件的 `body.payload` 同形（`view` 走同一个解析入口）：因此时间线上的「同一条目」
 * 在实时与历史两条来源下真的只是同一份数据的两种来源。
 */
export function historyItem(input: {
  readonly eventId: string;
  readonly eventType: string;
  readonly sessionSequence: number;
  readonly createdAt: string;
  readonly view: Record<string, unknown>;
  readonly acp?: Record<string, unknown>;
}): Record<string, unknown> {
  const item: Record<string, unknown> = {
    eventId: input.eventId,
    eventType: input.eventType,
    sessionSequence: String(input.sessionSequence),
    createdAt: input.createdAt,
    view: input.view,
  };
  if (input.acp !== undefined) item["acp"] = input.acp;
  return item;
}

/** 一个待处理交互（wire 形状：`snapshotItem.pending_interactions`）；它**没有**可选项。 */
export function pendingInteraction(input: {
  readonly interactionId: string;
  readonly sessionId: string;
  readonly kind: "permission" | "elicitation";
  readonly createdAt: string;
  readonly schema?: Record<string, unknown>;
}): Record<string, unknown> {
  return {
    interactionId: input.interactionId,
    sessionId: input.sessionId,
    kind: input.kind,
    state: "pending",
    schema: input.schema ?? {},
    createdAt: input.createdAt,
  };
}

/** 一个配置项（wire 形状：`configOptionView`）；`category: "model"` 的那一项即模型分节的来源。 */
export function configOption(input: {
  readonly id: string;
  readonly name: string;
  readonly currentValue: string;
  readonly category?: string;
  readonly choices?: readonly { readonly value: string; readonly name: string }[];
}): Record<string, unknown> {
  return {
    id: input.id,
    name: input.name,
    description: null,
    category: input.category ?? "model",
    type: "select",
    currentValue: input.currentValue,
    ...(input.choices === undefined
      ? {}
      : { options: input.choices.map((choice) => ({ value: choice.value, name: choice.name, description: null })) }),
  };
}

/**
 * `session.read` 的一页应答（wire 形状：`sessionReadResult`）。
 *
 * 只写出用例用到的资源键：`resources.*` 的每个键都可缺席，缺席即「这次没要那一份」。
 * `config_options` 是**数组**（`snapshotItem.config_options`），元素里再带 `configOptions`。
 */
export function readPage(input: {
  readonly sessionId: string;
  readonly messages?: readonly Record<string, unknown>[];
  readonly history?: readonly Record<string, unknown>[];
  readonly pendingInteractions?: readonly Record<string, unknown>[];
  readonly configOptions?: readonly Record<string, unknown>[];
  readonly configVersion?: string;
  readonly hasEarlier?: boolean;
  readonly historyHasEarlier?: boolean;
}): Record<string, unknown> {
  const resources: Record<string, unknown> = { messages: input.messages ?? [] };
  if (input.history !== undefined) resources["history"] = input.history;
  if (input.pendingInteractions !== undefined) resources["pending_interactions"] = input.pendingInteractions;
  if (input.configOptions !== undefined) {
    resources["config_options"] = [
      {
        sessionId: input.sessionId,
        configOptions: input.configOptions,
        version: input.configVersion ?? "1",
      },
    ];
  }
  const page: Record<string, unknown> = {
    sessionId: input.sessionId,
    resources,
    hasEarlier: input.hasEarlier ?? false,
  };
  if (input.historyHasEarlier !== undefined) page["historyHasEarlier"] = input.historyHasEarlier;
  return page;
}