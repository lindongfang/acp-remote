/**
 * 目录页与目录详情的视图模型（R15「目录页以目录为主角且可离线渲染」）。
 *
 * ## 三条结构性约束如何被落到类型上
 *
 * 1. **目录项只有展示名、别名、汇总与最近活动时间**：`DirectoryRowModel` 的字段就这四样，
 *    **没有**任何可以反推路径的槽位（没有 id 派生自路径、没有父目录、没有末段、没有 cwd）。
 *    `WorkspaceRef` 在 wire 上本就只有 `alias`/`displayName`，本层再投影一次；
 *    `latestActivityAt` 是该目录下会话自身的 `updatedAt`（固定形状的 UTC 串），同样不含路径。
 *    于是「多带一个 path 字段」在类型与渲染两处都不成立。
 * 2. **同名目录以别名区分**：`displayName` 允许重复，`alias` 是唯一标识且**始终**渲染。
 *    组件没有任何「同名时才显示别名」的分支——那种分支就是路径消歧的入口。
 * 3. **两种空态可区分**：`no_directories` 与 `directories_without_sessions` 是两个取值，
 *    且各自的标题与操作指引来自不同词表，不可能被合并成一句话。
 *
 * ## 数据只来自快照
 *
 * 构造输入只有「上一次成功替换的快照资源」与「连接状态」两个，**没有任何命令出口**：
 * 目录页因此在结构上不可能依赖额外的在线查询命令，断连时也照常渲染（R15 MUST）。
 */

import type { DirectoryListState, DirectoryView, SessionSummaryView } from "../domain";
import { UNTITLED_SESSION_LABEL, buildDirectoryViews, buildSessionSummaryView } from "../domain";
import type { SessionSummary, WorkspaceRef } from "../protocol";
import type { StagedResources } from "../sync-client";
import type { ConnectionStateName } from "../state";

/** 目录的三项汇总（全部由会话摘要得出，因此断连时仍可呈现）。 */
export interface DirectoryCountsModel {
  readonly total: number;
  readonly active: number;
  readonly pending: number;
}

/**
 * 一个目录项：**只有**展示名、别名、三项汇总与最近活动时间。
 *
 * `latestActivityAt` 是该目录下会话 `updatedAt` 的**最大值**（`Timestamp` 是固定形状的 UTC 串，
 * 字典序即时间序）；该目录没有任何会话时为 `null`——**不**用当前时间或别的信号顶替。
 * 它与 `counts` 一样只由快照资源推出，因此断连时仍可呈现。
 */
export interface DirectoryRowModel {
  readonly alias: string;
  readonly displayName: string;
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

/** 一个月份档：`key` 为 `YYYY-MM`，`count` 是该月的行数。 */
export interface DirectoryMonthModel {
  readonly key: string;
  readonly count: number;
}

/**
 * 会话时间 → `YYYY-MM`。
 *
 * 直接取字符串前 7 位，不经过 `Date`：`Timestamp` 是固定形状的 UTC 串
 * （`src/protocol/common.ts` 的 `$defs.timestamp`），而 `Date` 会按本地时区换算，
 * 把月界附近的会话挪进相邻月份。分档必须落在会话**自身**的时间上。
 */
export function monthKeyOf(timestamp: string): string {
  return timestamp.slice(0, 7);
}

/** 会话行（目录详情里的条目）。 */
export interface SessionRowModel {
  readonly id: string;
  readonly title: string;
  readonly agentName: string;
  readonly state: SessionSummaryView["state"];
  readonly directoryAlias: string | null;
  /** imported 会话来源离线时的标注；本节点自有会话为 `null`。 */
  readonly originOfflineLabel: string | null;
  readonly updatedAt: string;
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

/** 目录详情模型。 */
export type DirectoryDetailModel =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly directory: DirectoryRowModel;
      /**
       * **生效**的月份档（对账后的值）：请求的档若已不在 `months` 的 key 集合里，这里回落为
       * `"all"`。组件必须拿它当分段控件的 `value`——否则控件（选项由 `months` 生成）无法表示
       * 该值，整组 `data-segment-selected` 会全为 false，而 `sessions` 又被一个已经不存在的档
       * 挡成空列表，用户看不到是哪个档在挡着。
       */
      readonly month: string | "all";
      /** 已按**生效**月份档过滤后的行；`month` 为 `"all"` 时即查询命中的全部行。 */
      readonly sessions: readonly SessionRowModel[];
      /** 全部月份档（受查询影响，但不受当前月份选择影响），按时间倒序。 */
      readonly months: readonly DirectoryMonthModel[];
      /** 当前呈现的行数（随月份选择变化）。 */
      readonly shown: number;
      /** 查询命中后的会话总数（与月份选择无关，即「当前显示 N / M」的分母 M）。 */
      readonly total: number;
      readonly contentOrigin: DirectoryContentOrigin;
    }
  | {
      /** 别名不在快照目录里（目录被删或链接过期）：与「目录存在但无会话」是不同呈现。 */
      readonly kind: "unknown_directory";
      readonly alias: string;
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
 * 目录项投影：目录名出现在标题上，会话数/运行中/待处理来自会话摘要，最近活动时间由调用方传入
 * （见 {@link latestActivityByAlias}）。
 *
 * 带会话聚合结果的目录项（目录页 `ready` 支的每一项与详情页的目录）都走这里，因此不会有第二份
 * 行构造漏掉新字段；「目录存在但没有任何会话」的零汇总项没有聚合结果可投影，另见 {@link zeroRow}。
 */
function toRow(directory: DirectoryView, latestActivityAt: string | null): DirectoryRowModel {
  return {
    alias: directory.alias,
    displayName: directory.displayName,
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
 * ——**不**用当前时间或别的信号顶替。列表页在空态下列出它，详情页也由它按别名取到该目录。
 */
function zeroRow(workspace: WorkspaceRef): DirectoryRowModel {
  return {
    alias: workspace.alias,
    displayName: workspace.displayName,
    counts: { total: 0, active: 0, pending: 0 },
    latestActivityAt: null,
  };
}

/**
 * 每个目录的最近活动时间：该目录下会话 `updatedAt` 的**最大值**。
 *
 * 用字符串比较取最大值，不经过 `Date`：`Timestamp` 是固定形状的 UTC 串
 * （`src/protocol/common.ts` 的 `$defs.timestamp`），字典序即时间序，而 `Date` 会把时间换算到
 * 本地时区；也**不**取「第一条」或「当前时间」——活动时间必须是会话自身携带的事实。
 * 会话不按目录分组前的时间序排列，因此这里与列表顺序无关（构造点见 `toRow` 的调用方）。
 *
 * 没有会话的目录不出现在返回值里，调用方回落为 `null`；键只在已登记别名上被查询
 * （`ready` 的目录项来自快照 `workspaces`，详情页的别名也必先在这份目录里定位到），
 * 因此未登记目录的会话不进入任何目录项，与 `buildSessionRowsByAlias` 的收录口径一致。
 */
function latestActivityByAlias(sessions: readonly SessionSummary[]): ReadonlyMap<string, string> {
  const map = new Map<string, string>();
  for (const summary of sessions) {
    const alias = summary.workspace?.alias;
    if (alias === undefined) continue;
    const current = map.get(alias);
    if (current === undefined || summary.updatedAt > current) map.set(alias, summary.updatedAt);
  }
  return map;
}

/** 断连（含未配对与全部阻断态）时内容来源是「上次同步」。 */
function contentOriginOf(connection: ConnectionStateName): DirectoryContentOrigin {
  return connection === "online" ? "live" : "last_sync";
}

/** 目录页匹配所需的「别名 → 该目录下会话的呈现标题」。 */
function titlesByAlias(sessions: readonly SessionSummary[]): ReadonlyMap<string, readonly string[]> {
  const map = new Map<string, string[]>();
  for (const summary of sessions) {
    const alias = summary.workspace?.alias;
    if (alias === undefined) continue;
    // 空标题在行上呈现为「未命名会话」，匹配就用同一段文字——搜索的就是看得见的内容。
    const text = summary.title === null || summary.title.length === 0 ? UNTITLED_SESSION_LABEL : summary.title;
    const bucket = map.get(alias);
    if (bucket === undefined) map.set(alias, [text]);
    else bucket.push(text);
  }
  return map;
}

/** trim + 大小写不敏感的包含匹配：命中展示名、别名，或该目录下任一标题。 */
function matchesQuery(
  query: string,
  directory: DirectoryView,
  titles: ReadonlyMap<string, readonly string[]>,
): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return true;
  if (directory.displayName.toLowerCase().includes(needle)) return true;
  if (directory.alias.toLowerCase().includes(needle)) return true;
  return (titles.get(directory.alias) ?? []).some((title) => title.toLowerCase().includes(needle));
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
 * @param query 查询词（缺省 `""`）；匹配展示名、别名与名下会话标题，trim + 大小写不敏感。
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
  if (list.kind === "empty") {
    return {
      kind: "empty",
      empty: list.empty,
      // 空态也列出已登记的目录：`directories_without_sessions` 的指引写着「在下面选一个目录」，
      // 那一支必须真有目录项（每一项都是零汇总行），否则指引指向不存在的入口；
      // `no_directories` 时确实是空数组。本支的 kind/empty 取值与判定条件都不因此改变。
      directories:
        list.empty === "directories_without_sessions"
          ? input.resources.workspaces.map((workspace) => zeroRow(workspace))
          : [],
      contentOrigin,
      lastSyncedAtMs: input.lastSyncedAtMs,
    };
  }

  const query = input.query ?? "";
  const filter = input.filter ?? "all";
  const titles = titlesByAlias(input.resources.sessions);
  const directories = list.directories.filter(
    (directory) => matchesQuery(query, directory, titles) && matchesFilter(filter, directory),
  );

  // 目录列表非空，且缺省查询/筛选下命中的就是全部目录，因此这一支只在显式提供了非空查询
  // 或非默认筛选时才可达——`empty` 于是仍然只表达「没有目录 / 目录里从来没有会话」两种空态。
  if (directories.length === 0) {
    return { kind: "no_match", contentOrigin, lastSyncedAtMs: input.lastSyncedAtMs };
  }

  const latestActivity = latestActivityByAlias(input.resources.sessions);
  return {
    kind: "ready",
    directories: directories.map((directory) => toRow(directory, latestActivity.get(directory.alias) ?? null)),
    contentOrigin,
    lastSyncedAtMs: input.lastSyncedAtMs,
  };
}

/** 会话摘要 → 会话行。imported 会话来源离线时标注来源（R20：只显示元数据）。 */
function toSessionRow(summary: SessionSummaryView): SessionRowModel {
  const offline =
    summary.origin.kind === "remote" && !summary.origin.online
      ? `来源「${summary.origin.ownerLabel}」当前离线`
      : null;
  return {
    id: summary.id,
    title: summary.title.kind === "titled" ? summary.title.text : UNTITLED_SESSION_LABEL,
    agentName: summary.agentName,
    state: summary.state,
    directoryAlias: summary.directoryAlias,
    originOfflineLabel: offline,
    updatedAt: summary.updatedAt,
  };
}

/**
 * 按目录别名分组、组内按 `updatedAt` 倒序的会话行（`design.md` D3 的「会话行投影」）。
 *
 * 目录页的内联会话行用它——**不经过**详情页的查询词/月份状态：那两个字段只属于详情页
 * （spec「与目录页的查询词互不影响」，反向同理：详情页里敲的关键词不得改变目录页看到的内容）。
 * 目录详情的行也走这里（再叠加目录内查询与月份档），因此两页的行口径只有一份。
 *
 * 只收录别名在快照 `workspaces` 里登记过的会话：未登记目录的会话不归入任何目录项，
 * 与 `buildDirectoryViews` 的汇总口径一致。`resources` 为 `null`（从未同步）时返回空 Map。
 */
export function buildSessionRowsByAlias(input: {
  readonly resources: StagedResources | null;
  readonly ownerLabelOf: (ownerNodeId: string) => string;
}): ReadonlyMap<string, readonly SessionRowModel[]> {
  const byAlias = new Map<string, SessionRowModel[]>();
  if (input.resources === null) return byAlias;

  const registered = new Set(input.resources.workspaces.map((workspace) => workspace.alias));
  for (const summary of input.resources.sessions) {
    const alias = summary.workspace?.alias;
    if (alias === undefined || !registered.has(alias)) continue;
    const origin = summary.origin;
    const ownerLabel = origin.kind === "remote" ? input.ownerLabelOf(origin.ownerNodeId) : "";
    const row = toSessionRow(buildSessionSummaryView(summary, ownerLabel));
    const bucket = byAlias.get(alias);
    if (bucket === undefined) byAlias.set(alias, [row]);
    else bucket.push(row);
  }

  // 组内倒序：详情页的月份档由这段已排序的行派生，于是档间也自动倒序。
  for (const bucket of byAlias.values()) {
    bucket.sort((left, right) => (left.updatedAt < right.updatedAt ? 1 : left.updatedAt > right.updatedAt ? -1 : 0));
  }
  return byAlias;
}

/**
 * 在快照目录里按别名定位目录项。
 *
 * 列表页与详情页对「有目录、但每个目录内都没有会话」的处置**不同**（R15：别名失效与
 * 「目录存在但没有会话」MUST 是两种可区分的呈现）：列表页要呈现
 * `directories_without_sessions` 空态，于是 `buildDirectoryViews` 在这一态**不携带目录项**；
 * 而详情页仍必须能按别名取到该目录——否则别名明明在快照里也会被说成「链接已失效」，
 * 把用户引向「目录被删了」的错误结论。
 *
 * 两条支路各自给出一个目录项：`ready` 支的目录经 {@link toRow} 投影（活动时间由调用方算好传入，
 * 与列表页的目录项同一份口径）；空态支的目录由快照目录直接投影成 {@link zeroRow}——该状态下
 * 每个目录的会话汇总恒为零（`directories_without_sessions` 的定义就是「没有任何会话归属任何
 * 已登记目录」），活动时间也固定为 `null`（该别名下没有任何会话）。
 */
function findDirectoryByAlias(
  list: DirectoryListState,
  workspaces: readonly WorkspaceRef[],
  alias: string,
  latestActivityAt: string | null,
): DirectoryRowModel | undefined {
  if (list.kind === "ready") {
    const found = list.directories.find((candidate) => candidate.alias === alias);
    return found === undefined ? undefined : toRow(found, latestActivityAt);
  }
  if (list.empty !== "directories_without_sessions") return undefined;
  const workspace = workspaces.find((candidate) => candidate.alias === alias);
  return workspace === undefined ? undefined : zeroRow(workspace);
}

/** 月份档：入参已按时间倒序，因此首次出现即倒序；`count` 是该月的行数。 */
function monthBuckets(rows: readonly SessionRowModel[]): readonly DirectoryMonthModel[] {
  const counts = new Map<string, number>();
  for (const row of rows) {
    const key = monthKeyOf(row.updatedAt);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts].map(([key, count]) => ({ key, count }));
}

/**
 * 目录详情模型：某个别名下的会话列表。
 *
 * `alias` 不在快照目录里时返回 `unknown_directory`——它与「目录存在但没有会话」不是同一件事，
 * 前者说明链接已失效，后者说明目录是好的。
 *
 * @param query 目录内查询词（缺省 `""`）；只匹配**本目录**会话的呈现标题。
 * @param month 请求的月份档（缺省 `"all"`）；只决定 `sessions`/`shown`，不影响 `months`/`total`。
 *   不在派生出的 `months` 里时按 `"all"` 计算，并把回落后的值放进 {@link DirectoryDetailModel}
 *   的 `month` 字段（生效值）。
 */
export function buildDirectoryDetailModel(input: {
  readonly resources: StagedResources | null;
  readonly connection: ConnectionStateName;
  readonly alias: string;
  /** imported 会话的来源展示名（按 `ownerNodeId` 解析）。 */
  readonly ownerLabelOf: (ownerNodeId: string) => string;
  readonly query?: string;
  readonly month?: string | "all";
}): DirectoryDetailModel {
  if (input.resources === null) return { kind: "loading" };

  const list = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
  // 最近活动时间与列表页同一份口径：该目录下会话 `updatedAt` 的最大值，没有会话即 `null`
  // （「目录存在但没有任何会话」这一支也算出 `null`，与空态下列出的零汇总项一致）。
  const latestActivityAt = latestActivityByAlias(input.resources.sessions).get(input.alias) ?? null;
  const directory = findDirectoryByAlias(list, input.resources.workspaces, input.alias, latestActivityAt);
  if (directory === undefined) {
    return { kind: "unknown_directory", alias: input.alias };
  }

  const needle = (input.query ?? "").trim().toLowerCase();
  // 行投影与目录页共用同一个出口（`buildSessionRowsByAlias`）：两页的标题、来源标注与排序
  // 口径只有一份；`input.query`/`input.month` 只作用于**详情页**这两个局部量，不回流到目录页。
  const rows = (
    buildSessionRowsByAlias({ resources: input.resources, ownerLabelOf: input.ownerLabelOf }).get(input.alias) ?? []
  ).filter((row) => needle.length === 0 || row.title.toLowerCase().includes(needle));

  const months = monthBuckets(rows);
  const requestedMonth = input.month ?? "all";
  // 对账：请求的档若不在派生出的 `months` 里（目录内搜索把只含别的月份的档滤没了、或跨目录
  // 残留下上一个目录的档），必须回落 `"all"`。否则会同时坏两处：控件的选项由 `months` 生成，
  // 表示不了这个值，于是整组选中态为空；而 `sessions` 又被这个不存在的档滤成空——用户看到
  // 「没有符合条件的会话」，却看不到是哪个（已经消失的）档在挡着。
  const month =
    requestedMonth === "all" || months.some((bucket) => bucket.key === requestedMonth) ? requestedMonth : "all";
  const sessions = month === "all" ? rows : rows.filter((row) => monthKeyOf(row.updatedAt) === month);

  return {
    kind: "ready",
    // 目录项与列表页走同一条投影（`toRow`）：汇总来自快照聚合，活动时间即上面算出的最大值。
    directory,
    month,
    sessions,
    months,
    shown: sessions.length,
    total: rows.length,
    contentOrigin: contentOriginOf(input.connection),
  };
}
