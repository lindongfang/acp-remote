/**
 * 目录面视图模型（目录页、目录详情页与对话页左栏）。
 *
 * ## 结构性约束如何落到类型上
 *
 * 1. **路径只有一处来源**：`DirectoryRowModel.path` 逐字等于快照目录条目上的 `path` 键
 *    （必填）。本层不做拼接、不做规范化，也**不**用展示名或目录标识拼一段看起来像路径的文本。
 * 2. **展示名由路径派生**：取 `path` 的最后一段；同一份快照里尾段相同的目录用更多路径段消歧
 *    （`pathTail` 只在同名时非 `null`）。判定按**整份快照**算，不随查询/筛选变化。
 * 3. **两种空态可区分**：`no_directories` 与 `directories_without_sessions` 是两个取值，
 *    且各自的标题与操作指引来自不同词表，不可能被合并成一句话。
 * 4. **会话时间只有一处口径**：三个页面共用的 {@link SessionRowModel.updatedAtLabel} 由
 *    {@link formatRelativeTime} 按**注入的**时钟算出，组件不再自行格式化时刻（D3）。
 * 5. **分页判定全在本层的纯函数里**：每页条数、总页数、当前页切片与页码纠正都由
 *    {@link buildDirectoryDetailModel} / {@link correctPageNumber} 给出，组件只渲染并上抛。
 *
 * ## 数据只来自快照
 *
 * 构造输入只有「上一次成功替换的快照资源」「连接状态」与「时钟」三个，**没有任何命令出口**：
 * 目录面因此在结构上不可能依赖额外的在线查询命令，断连时也照常渲染（R15 MUST）。
 */

import type { DirectoryListState, DirectoryView, SessionSummaryView } from "../domain";
import { UNTITLED_SESSION_LABEL, buildDirectoryViews, buildSessionSummaryView, directoryNamesOf } from "../domain";
import type { SessionSummary, WorkspaceCatalogEntry } from "../protocol";
import type { StagedResources } from "../sync-client";
import type { ConnectionStateName } from "../state";

/** 一分钟的毫秒数（相对时间的分档线）。 */
const MINUTE_MS = 60_000;

/** 一小时的毫秒数（相对时间的分档线）。 */
const HOUR_MS = 3_600_000;

/**
 * 会话时间 → 相对文案（规格「会话时间按相对口径呈现」，D3）。
 *
 * 口径：距 `nowMs` 不足一分钟给「刚刚」，不足一小时给「N 分钟前」（`N` 四舍五入到分钟），
 * 否则给该时刻的 UTC 年月日。三个页面（目录页内联行、目录详情行、对话页左栏）共用这一段。
 *
 * **时钟是输入**：`nowMs` 由调用方（页面从 store 的时钟端口读一次、测试注入固定值）传入，
 * 函数本身不读任何全局时钟。因此「同一份输入渲染出同一段文本」这条既有性质被保留：同一份
 * `(timestamp, nowMs)` 永远得到同一段文字，`renderToStaticMarkup` 与浏览器断言都不依赖运行时刻。
 *
 * 时间串形状不合规时**原样返回**那个串：不假装算出了相对时间，也不抛出（渲染路径不该崩）。
 */
export function formatRelativeTime(timestamp: string, nowMs: number): string {
  const at = Date.parse(timestamp);
  if (Number.isNaN(at)) return timestamp;
  const diff = nowMs - at;
  if (diff < MINUTE_MS) return "刚刚";
  if (diff < HOUR_MS) return `${Math.round(diff / MINUTE_MS)} 分钟前`;
  // 与原型同口径：UTC 的年月日，不按本地时区换算（会话时刻本身就是 UTC 串）。
  return new Date(at).toISOString().slice(0, 10);
}

/** 目录的三项汇总（全部由会话摘要得出，因此断连时仍可呈现）。 */
export interface DirectoryCountsModel {
  readonly total: number;
  readonly active: number;
  readonly pending: number;
}

/**
 * 一个目录项。
 *
 * `path` 是服务端下发的**逐字**路径（快照目录条目的必填 `path` 键），本层原样透传。
 *
 * `displayName` 取自该路径的最后一段；`pathTail` 是**列表项消歧用的**更多路径段，只在「同一份
 * 快照里有另一个目录的展示名与它相同」时才有值（展示名唯一时为 `null`）。它逐字来自 `path`，
 * 不经任何规范化。
 *
 * `latestActivityAt` 是该目录下会话 `updatedAt` 的**最大值**（`Timestamp` 是固定形状的 UTC 串，
 * 字典序即时间序）；该目录没有任何会话时为 `null`——**不**用当前时间或别的信号顶替。
 * 它与 `counts` 一样只由快照资源推出，因此断连时仍可呈现。
 */
export interface DirectoryRowModel {
  /** 目录标识：只用于路由与键，MUST NOT 呈现为界面文本。 */
  readonly directoryId: string;
  readonly displayName: string;
  /** 逐字路径（必填）。 */
  readonly path: string;
  /** 同名目录的消歧段（更多路径段）；展示名唯一时为 `null`。 */
  readonly pathTail: string | null;
  readonly counts: DirectoryCountsModel;
  readonly latestActivityAt: string | null;
}

/** 目录内容的来源：实时同步 vs 上一次成功同步的快照（断连时必须明确标注）。 */
export type DirectoryContentOrigin = "live" | "last_sync";

/**
 * 目录页的状态筛选。
 *
 * `all` 不过滤；`running` 命中存在排队中或运行中会话的目录；`pending` 命中存在等待输入
 * 或等待授权会话的目录。判定复用会话汇总里的 `active`/`pending`（与 `buildDirectoryViews`
 * 同源），因此不会出现第二套状态口径。
 */
export type DirectoryFilter = "all" | "running" | "pending";

/**
 * 会话行（目录页内联行、目录详情行与对话页左栏共用）。
 *
 * `updatedAt` 是会话自身的时刻（固定形状的 UTC 串，排序与 `data-*` 精确值都用它）；
 * `updatedAtLabel` 是**呈现用**的相对文案，由 {@link formatRelativeTime} 按注入的时钟算出。
 * 组件 MUST 渲染 `updatedAtLabel`：自己拿 `updatedAt` 格式化就等于各写一套时区/相对口径。
 */
export interface SessionRowModel {
  readonly id: string;
  readonly title: string;
  readonly agentName: string;
  readonly state: SessionSummaryView["state"];
  readonly directoryId: string | null;
  /** imported 会话来源离线时的标注；本节点自有会话为 `null`。 */
  readonly originOfflineLabel: string | null;
  readonly updatedAt: string;
  /** 相对文案（刚刚 / N 分钟前 / 年月日），时钟经输入注入。 */
  readonly updatedAtLabel: string;
}

/** 目录页模型。 */
export type DirectoryPageModel =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly directories: readonly DirectoryRowModel[];
      readonly contentOrigin: DirectoryContentOrigin;
      /** 快照替换时刻（epoch 毫秒）；从未同步过为 `null`。 */
      readonly lastSyncedAtMs: number | null;
    }
  | {
      readonly kind: "empty";
      readonly empty: "no_directories" | "directories_without_sessions";
      /**
       * 已登记的目录项：`no_directories` 时为 `[]`；`directories_without_sessions` 时为各目录的
       * 零汇总行（汇总全 0、无活动时间），于是空态指引里的「在下面选一个目录」有真实入口。
       *
       * 它**不**改变本支的语义：`empty` 仍然只表达「没有目录 / 目录里从来没有会话」两种情形，
       * `data-empty-state` 仍只在 `empty` 出现，`ready` / `no_match` / `loading` 的判定条件不变。
       */
      readonly directories: readonly DirectoryRowModel[];
      readonly contentOrigin: DirectoryContentOrigin;
      readonly lastSyncedAtMs: number | null;
    }
  | {
      /**
       * 有快照、目录列表非空，但当前查询或筛选没有任何命中。
       *
       * 它与 `empty` 是两回事：`empty` 说的是「本来就没有目录 / 目录里从来没有会话」，
       * 本支说的是「数据在，只是被查询/筛选挡住了」。二者在文案与标记上互不冒充，
       * 本支**不占用** `data-empty-state`（否则会污染 R15 既有的空态负向断言）。
       */
      readonly kind: "no_match";
      readonly contentOrigin: DirectoryContentOrigin;
      readonly lastSyncedAtMs: number | null;
    };

/**
 * 目录详情页每页的会话条数（规格「每页固定条数」的唯一出处，与原型 `PAGE_SIZE` 一致）。
 */
export const DIRECTORY_DETAIL_PAGE_SIZE = 10;

/**
 * 页码输入 → 合法页码（纯函数，D4 的纠正规则与原型 `commitPage` 同口径）。
 *
 * - 能解析成正整数：夹紧到 `[1, pageCount]`（超出上界取下界/上界）；
 * - 解析不出（空串、`"abc"`、`"2页"` 这类带尾巴的、纯符号）：回落 `fallback`——调用方给的
 *   **当前生效页码**。这正是规格里「列表内容不因一次非法输入而重画」的落点：纠正后的值等于
 *   原来的值，切片自然是同一段。
 *
 * 严格解析（而不是 `parseInt` 的宽松解析）是有意的：`parseInt("2abc") === 2` 会把一段明显
 * 不合法的输入当成合法页码跳过去，而规格要求的是「不是合法页码就就地纠正」。
 */
export function correctPageNumber(input: {
  readonly requested: string;
  readonly pageCount: number;
  readonly fallback: number;
}): number {
  const pageCount = Math.max(1, Math.trunc(input.pageCount));
  const fallback = Math.min(Math.max(1, Math.trunc(input.fallback)), pageCount);
  const requested = input.requested.trim();
  if (!/^[0-9]+$/.test(requested)) return fallback;
  const parsed = Number.parseInt(requested, 10);
  if (parsed < 1) return 1;
  return Math.min(parsed, pageCount);
}

/** 目录详情模型。 */
export type DirectoryDetailModel =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly directory: DirectoryRowModel;
      /**
       * **当前页**的会话行：按时间倒序平铺后的第 `page` 页切片。
       *
       * 详情页没有查询词与月份档（规格「目录详情按时间倒序分页」替换了旧的月份分档与目录内
       * 搜索），因此切片前的行就是该目录的全部会话行。
       */
      readonly sessions: readonly SessionRowModel[];
      /**
       * **生效**（纠正后）的页码：请求值不是合法页码或超出范围时，这里给出纠正后的值。
       * 组件必须拿它回填页码输入框——因此「非法输入就地纠正」是可断言的行为而不是实现细节。
       */
      readonly page: number;
      /** 总页数（`ceil(total / pageSize)`，至少 1，因此 `page` 恒有合法解）。 */
      readonly pageCount: number;
      /** 每页条数（{@link DIRECTORY_DETAIL_PAGE_SIZE}）。 */
      readonly pageSize: number;
      /** 当前呈现的行数（即本页切片长度）。 */
      readonly shown: number;
      /**
       * 该目录的全部会话数（分页前）：页面头部的「N 个会话」与总页数都从它算出。
       */
      readonly total: number;
      readonly contentOrigin: DirectoryContentOrigin;
    }
  | {
      /** 目录标识不在快照目录里（目录被删或链接过期）：与「目录存在但无会话」是不同呈现。 */
      readonly kind: "unknown_directory";
      readonly directoryId: string;
    };

/** 两种空态的标题与操作指引：不同词、不同指引，合并即违反 R15。 */
export const DIRECTORY_EMPTY_COPY: Readonly<
  Record<"no_directories" | "directories_without_sessions", { readonly title: string; readonly guidance: string }>
> = {
  no_directories: {
    title: "本机没有任何目录",
    guidance: "目录在电脑上登记后才出现在这里。请先在电脑上添加工作区目录，再回到本页面。",
  },
  directories_without_sessions: {
    title: "目录已登记，但还没有会话",
    guidance: "在下面选一个目录进入详情页，然后在目录内创建一个会话。",
  },
};

/** 内容来源标注：断连时必须让人看出这些内容来自上次同步，而不是刚刚推送的。 */
export const CONTENT_ORIGIN_LABELS: Readonly<Record<DirectoryContentOrigin, string>> = {
  live: "内容来自本次同步",
  last_sync: "内容来自上次同步，当前未连接",
};

/**
 * 目录项投影：目录名出现在标题上，会话数/运行中/待处理来自会话摘要，路径、展示名与同名消歧段
 * 由领域层按整份快照派生（{@link directoryNamesOf}），最近活动时间由调用方传入
 * （见 {@link latestActivityByDirectoryId}）。
 *
 * 带会话聚合结果的目录项（目录页 `ready` 支的每一项与详情页的目录）都走这里，因此不会有第二份
 * 行构造漏掉新字段；「目录存在但没有任何会话」的零汇总项没有聚合结果可投影，另见 {@link zeroRow}。
 */
function toRow(directory: DirectoryView, latestActivityAt: string | null): DirectoryRowModel {
  return {
    directoryId: directory.id,
    displayName: directory.displayName,
    path: directory.path,
    pathTail: directory.pathTail,
    counts: {
      total: directory.counts.total,
      active: directory.counts.active,
      pending: directory.counts.pending,
    },
    latestActivityAt,
  };
}

/**
 * 「目录存在但没有任何会话」时的零汇总目录项。
 *
 * 该状态下每个目录的会话汇总恒为零：`directories_without_sessions` 的定义就是
 * 「没有任何会话归属任何已登记目录」（`buildDirectoryViews` 里的 `hasAnySession` 为假），
 * 因此由快照目录直接投影，不必再走一遍会话聚合。这一支也没有任何会话，活动时间固定为 `null`
 * ——**不**用当前时间或别的信号顶替。列表页在空态下列出它，详情页也由它按目录标识取到该目录。
 *
 * 投影仍走 {@link toRow}：路径、展示名与「同名才给消歧段」因此与列表页其余项只有一处实现。
 */
function zeroRow(
  workspace: WorkspaceCatalogEntry,
  names: ReadonlyMap<string, { readonly displayName: string; readonly pathTail: string | null }>,
): DirectoryRowModel {
  const derived = names.get(workspace.directoryId);
  return toRow(
    {
      id: workspace.directoryId,
      displayName: derived?.displayName ?? workspace.path,
      pathTail: derived?.pathTail ?? null,
      path: workspace.path,
      counts: { total: 0, active: 0, pending: 0 },
    },
    null,
  );
}

/**
 * 每个目录的最近活动时间：该目录下会话 `updatedAt` 的**最大值**。
 *
 * 用字符串比较取最大值，不经过 `Date`：`Timestamp` 是固定形状的 UTC 串
 * （`src/protocol/common.ts` 的 `$defs.timestamp`），字典序即时间序，而 `Date` 会把时间换算到
 * 本地时区；也**不**取「第一条」或「当前时间」——活动时间必须是会话自身携带的事实。
 * 会话不按目录分组前的时间序排列，因此这里与列表顺序无关（构造点见 `toRow` 的调用方）。
 *
 * 没有会话的目录不出现在返回值里，调用方回落为 `null`；键只在已登记 `directoryId` 上被查询
 * （`ready` 的目录项来自快照 `workspaces`，详情页的标识也必先在这份目录里定位到），
 * 因此未登记目录的会话不进入任何目录项，与 `buildSessionRowsByDirectoryId` 的收录口径一致。
 */
function latestActivityByDirectoryId(sessions: readonly SessionSummary[]): ReadonlyMap<string, string> {
  const map = new Map<string, string>();
  for (const summary of sessions) {
    const directoryId = summary.workspace?.directoryId;
    if (directoryId === undefined) continue;
    const current = map.get(directoryId);
    if (current === undefined || summary.updatedAt > current) map.set(directoryId, summary.updatedAt);
  }
  return map;
}

/** 断连（含未配对与全部阻断态）时内容来源是「上次同步」。 */
function contentOriginOf(connection: ConnectionStateName): DirectoryContentOrigin {
  return connection === "online" ? "live" : "last_sync";
}

/** 目录页匹配所需的「目录标识 → 该目录下会话的呈现标题」。 */
function titlesByDirectoryId(sessions: readonly SessionSummary[]): ReadonlyMap<string, readonly string[]> {
  const map = new Map<string, string[]>();
  for (const summary of sessions) {
    const directoryId = summary.workspace?.directoryId;
    if (directoryId === undefined) continue;
    // 空标题在行上呈现为「未命名会话」，匹配就用同一段文字——搜索的就是看得见的内容。
    const text = summary.title === null || summary.title.length === 0 ? UNTITLED_SESSION_LABEL : summary.title;
    const bucket = map.get(directoryId);
    if (bucket === undefined) map.set(directoryId, [text]);
    else bucket.push(text);
  }
  return map;
}

/**
 * trim + 大小写不敏感的包含匹配：只命中展示名或该目录下任一标题。
 *
 * 目录标识（`directoryId`）MUST NOT 参与搜索：它只是路由与命令引用的内部标识，命中它会让
 * 「标识不出现在界面文本」这条验收无法成立。
 */
function matchesQuery(
  query: string,
  directory: DirectoryView,
  titles: ReadonlyMap<string, readonly string[]>,
): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  if (directory.displayName.toLowerCase().includes(needle)) return true;
  return (titles.get(directory.id) ?? []).some((title) => title.toLowerCase().includes(needle));
}

/** 状态筛选：复用汇总里的 `active`/`pending`，与目录页三项汇总同源。 */
function matchesFilter(filter: DirectoryFilter, directory: DirectoryView): boolean {
  if (filter === "all") return true;
  return filter === "running" ? directory.counts.active > 0 : directory.counts.pending > 0;
}

/**
 * 目录页模型。
 *
 * @param resources 上一次成功替换的快照资源；`null` 表示从未同步过。
 * @param lastSyncedAtMs 快照替换时刻；`null` 表示没有可标注的时间。
 * @param query 查询词（缺省 `""`）；只匹配展示名与名下会话标题，trim + 大小写不敏感。
 * @param filter 状态筛选（缺省 `"all"`）；只决定哪些目录项留下，不动各项汇总数字。
 */
export function buildDirectoryPageModel(input: {
  readonly resources: StagedResources | null;
  readonly connection: ConnectionStateName;
  readonly lastSyncedAtMs: number | null;
  readonly query?: string;
  readonly filter?: DirectoryFilter;
}): DirectoryPageModel {
  if (input.resources === null) return { kind: "loading" };

  const list: DirectoryListState = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
  const contentOrigin = contentOriginOf(input.connection);
  // 同名判定与展示名都按**整份快照**算（不是按筛选后的子集）：它们都是数据的性质，
  // 不因为用户敲了一个查询词就变——否则消歧段会时有时无。
  const names = directoryNamesOf(input.resources.workspaces);
  if (list.kind === "empty") {
    return {
      kind: "empty",
      empty: list.empty,
      // 空态也列出已登记的目录：`directories_without_sessions` 的指引写着「在下面选一个目录」，
      // 那一支必须真有目录项（每一项都是零汇总行），否则指引指向不存在的入口；
      // `no_directories` 时确实是空数组。本支的 kind/empty 取值与判定条件都不因此改变。
      directories:
        list.empty === "directories_without_sessions"
          ? input.resources.workspaces.map((workspace) => zeroRow(workspace, names))
          : [],
      contentOrigin,
      lastSyncedAtMs: input.lastSyncedAtMs,
    };
  }

  const query = input.query ?? "";
  const filter = input.filter ?? "all";
  const titles = titlesByDirectoryId(input.resources.sessions);
  const directories = list.directories.filter(
    (directory) => matchesQuery(query, directory, titles) && matchesFilter(filter, directory),
  );

  // 目录列表非空，且缺省查询/筛选下命中的就是全部目录，因此这一支只在显式提供了非空查询
  // 或非默认筛选时才可达——`empty` 于是仍然只表达「没有目录 / 目录里从来没有会话」两种空态。
  if (directories.length === 0) {
    return { kind: "no_match", contentOrigin, lastSyncedAtMs: input.lastSyncedAtMs };
  }

  const latestActivity = latestActivityByDirectoryId(input.resources.sessions);
  return {
    kind: "ready",
    directories: directories.map((directory) => toRow(directory, latestActivity.get(directory.id) ?? null)),
    contentOrigin,
    lastSyncedAtMs: input.lastSyncedAtMs,
  };
}

/** 会话摘要 → 会话行。imported 会话来源离线时标注来源（R20：只显示元数据）。 */
function toSessionRow(summary: SessionSummaryView, nowMs: number): SessionRowModel {
  const offline =
    summary.origin.kind === "remote" && !summary.origin.online
      ? `来源「${summary.origin.ownerLabel}」当前离线`
      : null;
  return {
    id: summary.id,
    title: summary.title.kind === "titled" ? summary.title.text : UNTITLED_SESSION_LABEL,
    agentName: summary.agentName,
    state: summary.state,
    directoryId: summary.directoryId,
    originOfflineLabel: offline,
    updatedAt: summary.updatedAt,
    updatedAtLabel: formatRelativeTime(summary.updatedAt, nowMs),
  };
}

/**
 * 按目录标识分组、组内按 `updatedAt` 倒序的会话行（`design.md` D3 的「会话行投影」）。
 *
 * 目录页的内联会话行用它——**不经过**目录页自己的查询词与筛选：那两个字段只属于目录页的
 * 呈现。目录详情的平铺列表与对话页左栏的列表也走这里（详情页再叠加分页、左栏再叠加它自己的
 * 目录内搜索），因此三处的行口径只有一份。
 *
 * 只收录目录标识在快照 `workspaces` 里登记过的会话：未登记目录的会话不归入任何目录项，
 * 与 `buildDirectoryViews` 的汇总口径一致。`resources` 为 `null`（从未同步）时返回空 Map。
 *
 * @param nowMs 相对时间的基准（D3：时钟是**输入**，由调用方注入，本层不读全局时钟）。
 */
export function buildSessionRowsByDirectoryId(input: {
  readonly resources: StagedResources | null;
  readonly ownerLabelOf: (ownerNodeId: string) => string;
  readonly nowMs: number;
}): ReadonlyMap<string, readonly SessionRowModel[]> {
  const byDirectoryId = new Map<string, SessionRowModel[]>();
  if (input.resources === null) return byDirectoryId;

  const registered = new Set(input.resources.workspaces.map((workspace) => workspace.directoryId));
  for (const summary of input.resources.sessions) {
    const directoryId = summary.workspace?.directoryId;
    if (directoryId === undefined || !registered.has(directoryId)) continue;
    const origin = summary.origin;
    const ownerLabel = origin.kind === "remote" ? input.ownerLabelOf(origin.ownerNodeId) : "";
    const row = toSessionRow(buildSessionSummaryView(summary, ownerLabel), input.nowMs);
    const bucket = byDirectoryId.get(directoryId);
    if (bucket === undefined) byDirectoryId.set(directoryId, [row]);
    else bucket.push(row);
  }

  // 组内倒序：详情页的平铺列表直接按这个顺序切片，因此第 1 页就是最新的那一页。
  for (const bucket of byDirectoryId.values()) {
    bucket.sort((left, right) => (left.updatedAt < right.updatedAt ? 1 : left.updatedAt > right.updatedAt ? -1 : 0));
  }
  return byDirectoryId;
}

/**
 * 在快照目录里按 `directoryId` 定位目录项。
 *
 * 列表页与详情页对「有目录、但每个目录内都没有会话」的处置**不同**（R15：目录标识失效与
 * 「目录存在但没有会话」MUST 是两种可区分的呈现）：列表页要呈现
 * `directories_without_sessions` 空态，于是 `buildDirectoryViews` 在这一态**不携带目录项**；
 * 而详情页仍必须能按标识取到该目录——否则标识明明在快照里也会被说成「链接已失效」，
 * 把用户引向「目录被删了」的错误结论。
 *
 * 两条支路各自给出一个目录项：`ready` 支的目录经 {@link toRow} 投影（活动时间由调用方算好传入，
 * 与列表页的目录项同一份口径）；空态支的目录由快照目录直接投影成 {@link zeroRow}——该状态下
 * 每个目录的会话汇总恒为零（`directories_without_sessions` 的定义就是「没有任何会话归属任何
 * 已登记目录」），活动时间也固定为 `null`（该标识下没有任何会话）。
 */
function findDirectoryByDirectoryId(
  list: DirectoryListState,
  workspaces: readonly WorkspaceCatalogEntry[],
  directoryId: string,
  latestActivityAt: string | null,
): DirectoryRowModel | undefined {
  if (list.kind === "ready") {
    const found = list.directories.find((candidate) => candidate.id === directoryId);
    return found === undefined ? undefined : toRow(found, latestActivityAt);
  }
  if (list.empty !== "directories_without_sessions") return undefined;
  const workspace = workspaces.find((candidate) => candidate.directoryId === directoryId);
  return workspace === undefined ? undefined : zeroRow(workspace, directoryNamesOf(workspaces));
}

/**
 * 目录详情模型：某个 `directoryId` 下的会话列表（倒序平铺 + 分页）。
 *
 * `directoryId` 不在快照目录里时返回 `unknown_directory`——它与「目录存在但没有会话」不是
 * 同一件事，前者说明链接已失效，后者说明目录是好的。返回的标识只供路由/键使用，组件 MUST NOT
 * 把它渲染成界面文本。
 *
 * @param pageInput 页码输入框里的**原文**（缺省 `""`，等价于第一页）。它按
 *   {@link correctPageNumber} 纠正：不是合法页码时回落到第一页，超出范围时夹紧到边界。
 *   「回落」在这里是**第一页**而不是「上一页」——从原文里读不出「上一页」是第几页；
 *   store 的 `setDirectoryDetailPage` 在提交时用当前生效页做回落，因此用户敲一段垃圾
 *   不会把列表换掉（规格「列表内容不因一次非法输入而重画」）。
 * @param nowMs 相对时间的基准（D3：时钟是**输入**）。
 */
export function buildDirectoryDetailModel(input: {
  readonly resources: StagedResources | null;
  readonly connection: ConnectionStateName;
  readonly directoryId: string;
  /** imported 会话的来源展示名（按 `ownerNodeId` 解析）。 */
  readonly ownerLabelOf: (ownerNodeId: string) => string;
  readonly nowMs: number;
  readonly pageInput?: string;
}): DirectoryDetailModel {
  if (input.resources === null) return { kind: "loading" };

  const list = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
  // 最近活动时间与列表页同一份口径：该目录下会话 `updatedAt` 的最大值，没有会话即 `null`
  // （「目录存在但没有任何会话」这一支也算出 `null`，与空态下列出的零汇总项一致）。
  const latestActivityAt = latestActivityByDirectoryId(input.resources.sessions).get(input.directoryId) ?? null;
  const directory = findDirectoryByDirectoryId(
    list,
    input.resources.workspaces,
    input.directoryId,
    latestActivityAt,
  );
  if (directory === undefined) {
    return { kind: "unknown_directory", directoryId: input.directoryId };
  }

  // 行投影与目录页共用同一个出口（`buildSessionRowsByDirectoryId`）：两页的标题、来源标注与排序
  // 口径只有一份。详情页没有任何查询词或月份档（规格把两者都移除了），因此切片前的行
  // 就是该目录的全部会话行——「空列表」于是只可能是「该目录一条会话都没有」。
  const rows =
    buildSessionRowsByDirectoryId({
      resources: input.resources,
      ownerLabelOf: input.ownerLabelOf,
      nowMs: input.nowMs,
    }).get(input.directoryId) ?? [];
  const total = rows.length;

  // 分页：条数、总页数、纠正后的页码与切片都在这里算完（D4），组件只渲染。
  const pageCount = Math.max(1, Math.ceil(total / DIRECTORY_DETAIL_PAGE_SIZE));
  const page = correctPageNumber({ requested: input.pageInput ?? "", pageCount, fallback: 1 });
  const sessions = rows.slice((page - 1) * DIRECTORY_DETAIL_PAGE_SIZE, page * DIRECTORY_DETAIL_PAGE_SIZE);

  return {
    kind: "ready",
    // 目录项与列表页走同一条投影（`toRow`）：汇总来自快照聚合，活动时间即上面算出的最大值。
    directory,
    // 本页切片；`total` 是**分页前**的全部会话数，总页数与页面头部的「N 个会话」都从它算。
    sessions,
    page,
    pageCount,
    pageSize: DIRECTORY_DETAIL_PAGE_SIZE,
    shown: sessions.length,
    total,
    contentOrigin: contentOriginOf(input.connection),
  };
}
