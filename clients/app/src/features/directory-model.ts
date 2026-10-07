/**
 * 目录页与目录详情的视图模型（R15「目录页以目录为主角且可离线渲染」）。
 *
 * ## 三条结构性约束如何被落到类型上
 *
 * 1. **目录项只有展示名与别名**：`DirectoryRowModel` 的字段就是这三样加三项汇总，
 *    **没有**任何可以反推路径的槽位（没有 id 派生自路径、没有父目录、没有末段、没有 cwd）。
 *    `WorkspaceRef` 在 wire 上本就只有 `alias`/`displayName`，本层再投影一次，
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
import type { WorkspaceRef } from "../protocol";
import type { StagedResources } from "../sync-client";
import type { ConnectionStateName } from "../state";

/** 目录的三项汇总（全部由会话摘要得出，因此断连时仍可呈现）。 */
export interface DirectoryCountsModel {
  readonly total: number;
  readonly active: number;
  readonly pending: number;
}

/** 一个目录项：**只有**展示名、别名与汇总。 */
export interface DirectoryRowModel {
  readonly alias: string;
  readonly displayName: string;
  readonly counts: DirectoryCountsModel;
}

/** 目录内容的来源：实时同步 vs 上一次成功同步的快照（断连时必须明确标注）。 */
export type DirectoryContentOrigin = "live" | "last_sync";

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
      readonly contentOrigin: DirectoryContentOrigin;
      readonly lastSyncedAtMs: number | null;
    };

/** 目录详情模型。 */
export type DirectoryDetailModel =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly directory: DirectoryRowModel;
      readonly sessions: readonly SessionRowModel[];
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

/** 目录是列表的主要组织单位：目录名出现在标题上，会话数/运行中/待处理来自会话摘要。 */
function toRow(directory: DirectoryView): DirectoryRowModel {
  return {
    alias: directory.alias,
    displayName: directory.displayName,
    counts: {
      total: directory.counts.total,
      active: directory.counts.active,
      pending: directory.counts.pending,
    },
  };
}

/** 断连（含未配对与全部阻断态）时内容来源是「上次同步」。 */
function contentOriginOf(connection: ConnectionStateName): DirectoryContentOrigin {
  return connection === "online" ? "live" : "last_sync";
}

/**
 * 目录页模型。
 *
 * @param resources 上一次成功替换的快照资源；`null` 表示从未同步过。
 * @param lastSyncedAtMs 快照替换时刻；`null` 表示没有可标注的时间。
 */
export function buildDirectoryPageModel(input: {
  readonly resources: StagedResources | null;
  readonly connection: ConnectionStateName;
  readonly lastSyncedAtMs: number | null;
}): DirectoryPageModel {
  if (input.resources === null) return { kind: "loading" };

  const list: DirectoryListState = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
  const contentOrigin = contentOriginOf(input.connection);
  if (list.kind === "empty") {
    return {
      kind: "empty",
      empty: list.empty,
      contentOrigin,
      lastSyncedAtMs: input.lastSyncedAtMs,
    };
  }
  return {
    kind: "ready",
    directories: list.directories.map(toRow),
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
 * 在快照目录里按别名定位目录项。
 *
 * 列表页与详情页对「有目录、但每个目录内都没有会话」的处置**不同**（R15：别名失效与
 * 「目录存在但没有会话」MUST 是两种可区分的呈现）：列表页要呈现
 * `directories_without_sessions` 空态，于是 `buildDirectoryViews` 在这一态**不携带目录项**；
 * 而详情页仍必须能按别名取到该目录——否则别名明明在快照里也会被说成「链接已失效」，
 * 把用户引向「目录被删了」的错误结论。
 *
 * 该状态下每个目录的会话汇总恒为零：`directories_without_sessions` 的定义就是
 * 「没有任何会话归属任何已登记目录」（`buildDirectoryViews` 里的 `hasAnySession` 为假），
 * 因此这里由快照目录直接投影出零汇总行，无需再走一遍会话聚合。
 */
function findDirectoryByAlias(
  list: DirectoryListState,
  workspaces: readonly WorkspaceRef[],
  alias: string,
): DirectoryView | undefined {
  if (list.kind === "ready") {
    return list.directories.find((candidate) => candidate.alias === alias);
  }
  if (list.empty !== "directories_without_sessions") return undefined;
  const workspace = workspaces.find((candidate) => candidate.alias === alias);
  if (workspace === undefined) return undefined;
  return {
    id: workspace.alias,
    displayName: workspace.displayName,
    alias: workspace.alias,
    counts: { total: 0, active: 0, pending: 0 },
  };
}

/**
 * 目录详情模型：某个别名下的会话列表。
 *
 * `alias` 不在快照目录里时返回 `unknown_directory`——它与「目录存在但没有会话」不是同一件事，
 * 前者说明链接已失效，后者说明目录是好的。
 */
export function buildDirectoryDetailModel(input: {
  readonly resources: StagedResources | null;
  readonly connection: ConnectionStateName;
  readonly alias: string;
  /** imported 会话的来源展示名（按 `ownerNodeId` 解析）。 */
  readonly ownerLabelOf: (ownerNodeId: string) => string;
}): DirectoryDetailModel {
  if (input.resources === null) return { kind: "loading" };

  const list = buildDirectoryViews(input.resources.workspaces, input.resources.sessions);
  const directory = findDirectoryByAlias(list, input.resources.workspaces, input.alias);
  if (directory === undefined) {
    return { kind: "unknown_directory", alias: input.alias };
  }

  const sessions = input.resources.sessions
    .filter((summary) => summary.workspace?.alias === input.alias)
    .map((summary) => {
      const origin = summary.origin;
      const ownerLabel = origin.kind === "remote" ? input.ownerLabelOf(origin.ownerNodeId) : "";
      return toSessionRow(buildSessionSummaryView(summary, ownerLabel));
    });

  return {
    kind: "ready",
    directory: toRow(directory),
    sessions,
    contentOrigin: contentOriginOf(input.connection),
  };
}
