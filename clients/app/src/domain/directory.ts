/**
 * 目录视图模型（目录页、目录详情页与对话页左栏的基础层）。
 *
 * 关键约束（`archive/2026-10-17-directory-auto-registration` 的目录页需求）：
 *
 * - 目录的稳定标识是服务端下发的 `directoryId`：它只用于路由参数、命令载荷与视图模型的键，
 *   MUST NOT 呈现为界面文本，也 MUST NOT 被解析或由它反推路径。
 * - 目录项只显示由 `path` 派生的展示名：取路径最后一段。两个目录的尾段相同时，用**更多路径段**
 *   （从末尾往前）消歧；消歧是按**整份快照目录集**算出的数据性质，不随查询/筛选变化。
 * - 路径逐字透传服务端下发的 `path`：本层 MUST NOT 拼接、规范化或反查本机路径。展示名与消歧段
 *   都是同一条路径里的**逐字片段**，不是另拼一段「看起来像路径」的文本。
 * - 同名目录之间**不**合并：`id`（`directoryId`）区分它们，展示名可以重复。
 * - 目录内的会话数、运行中数量与待处理数量由会话摘要得出（`SessionSummary.state`），
 *   因此断连时仍可呈现。
 * - 「目录存在但无会话」与「一个目录都没有」是两种可区分的状态。
 */

import type { SessionSummary, WorkspaceCatalogEntry } from "../protocol/common";
import type { LocalActivityView, SessionStateView } from "./session-summary";
import { localActivityOf } from "./session-summary";

/** 目录的稳定标识：服务端下发的 `directoryId`（不含路径信息）。 */
export type DirectoryId = string;

/**
 * 目录的会话汇总，全部由会话摘要得出。
 *
 * `active` / `pending` 与主状态同源：协商后的 owned 会话按 `localActivity` 计入，其余按任务
 * 三态计入（`running` → `active`、`pending` → `pending`）。**每个会话只计一次**——同一会话的
 * 多个有效交互不会让待处理计数增加。
 */
export interface DirectorySessionCounts {
  /** 该目录全部会话数。 */
  readonly total: number;
  /** 主状态为「活跃 / 运行中」的会话数。 */
  readonly active: number;
  /** 主状态为「待处理」的会话数。 */
  readonly pending: number;
}

/** 一个目录项。 */
export interface DirectoryView {
  readonly id: DirectoryId;
  /** 展示名：`path` 的最后一段（同名允许，由 `id` 区分）。 */
  readonly displayName: string;
  /**
   * 同名消歧用的**更多路径段**：只有同一份快照里存在另一目录的展示名与它相同时才有值，
   * 展示名唯一时为 `null`。取值逐字来自 `path`，按路径自身的分隔符连接，不含展示名那一段。
   */
  readonly pathTail: string | null;
  /** 服务端下发的逐字路径（必填，不规范化、不拼接）。 */
  readonly path: string;
  readonly counts: DirectorySessionCounts;
}

/** 一个目录的派生展示名与同名消歧段。 */
export interface DirectoryNames {
  readonly displayName: string;
  readonly pathTail: string | null;
}

/** 路径的非空段（按 `/` 与 `\` 切分；不规范化、不合并大小写）。 */
function pathSegments(path: string): readonly string[] {
  return path.split(/[\\/]+/).filter((part) => part.length > 0);
}

/** 路径自身使用的分隔符（取最后出现的那个；没有分隔符时给 `/`）。 */
function separatorOf(path: string): string {
  return path.lastIndexOf("\\") > path.lastIndexOf("/") ? "\\" : "/";
}

/**
 * 从**整份快照目录集**派生每个目录的展示名与同名消歧段。
 *
 * - 展示名 = `path` 的最后一段；
 * - 尾段相同的目录：从父目录开始逐步追加更多段，直到该组内唯一；`pathTail` 就是追加的那几段
 *   （不含展示名那一段），按该路径自己的分隔符连接。只有一段路径可用时无可追加，`pathTail`
 *   保持 `null`（列表上两项看起来仍相同，但绝不拿目录标识去顶替）。
 *
 * 返回按 `directoryId` 索引的 Map；调用方遍历目录集时不必重复扫描。重复的标识（不合规输入）
 * 以第一次出现为准。
 */
export function directoryNamesOf(
  workspaces: readonly WorkspaceCatalogEntry[],
): ReadonlyMap<DirectoryId, DirectoryNames> {
  const segmentsById = new Map<DirectoryId, readonly string[]>();
  const separatorById = new Map<DirectoryId, string>();
  for (const workspace of workspaces) {
    if (segmentsById.has(workspace.directoryId)) continue;
    segmentsById.set(workspace.directoryId, pathSegments(workspace.path));
    separatorById.set(workspace.directoryId, separatorOf(workspace.path));
  }

  const byDisplayName = new Map<string, DirectoryId[]>();
  for (const [directoryId, segments] of segmentsById) {
    const displayName = segments.at(-1) ?? "";
    const group = byDisplayName.get(displayName);
    if (group === undefined) byDisplayName.set(displayName, [directoryId]);
    else group.push(directoryId);
  }

  const names = new Map<DirectoryId, DirectoryNames>();
  for (const group of byDisplayName.values()) {
    for (const directoryId of group) {
      const segments = segmentsById.get(directoryId) ?? [];
      const displayName = segments.at(-1) ?? "";
      names.set(directoryId, {
        displayName,
        pathTail:
          group.length < 2 || segments.length < 2 ? null : uniqueSuffixOf(directoryId, group, segments, segmentsById, separatorById),
      });
    }
  }
  return names;
}

/**
 * 最短的唯一后缀（含展示名那一段）之外、需要追加的父段。
 *
 * 从 1 段父目录开始试：`[`, `...]` 与组内其它目录的同深度后缀比较，全部不同即停。
 * 追加到只剩首段仍不唯一时，给出除展示名外的全部段（此时两份路径的后缀确实相同，
 * 调用方仍不拿目录标识顶上）。
 */
function uniqueSuffixOf(
  directoryId: DirectoryId,
  group: readonly DirectoryId[],
  segments: readonly string[],
  segmentsById: ReadonlyMap<DirectoryId, readonly string[]>,
  separatorById: ReadonlyMap<DirectoryId, string>,
): string | null {
  const maxExtra = segments.length - 1;
  for (let extra = 1; extra <= maxExtra; extra += 1) {
    const suffix = segments.slice(segments.length - extra - 1);
    const key = suffix.join("\u0000");
    const unique = group.every((otherId) => {
      if (otherId === directoryId) return true;
      const other = segmentsById.get(otherId) ?? [];
      if (other.length < suffix.length) return true;
      return other.slice(other.length - suffix.length).join("\u0000") !== key;
    });
    if (unique) return segments.slice(segments.length - extra - 1, -1).join(separatorById.get(directoryId) ?? "/");
  }
  return null;
}

/** 空态：还没有可用的目录，或目录存在但都没有会话。 */
export type DirectoryEmptyState = "no_directories" | "directories_without_sessions";

/** 目录列表的整体状态：可直接渲染，两种空态可区分。 */
export type DirectoryListState =
  | { readonly kind: "ready"; readonly directories: readonly DirectoryView[] }
  | { readonly kind: "empty"; readonly empty: DirectoryEmptyState };

/** 两种空态的取值（导出给组件做穷尽 switch，避免散落字符串字面量）。 */
export const DIRECTION_EMPTY_STATES: readonly DirectoryEmptyState[] = [
  "no_directories",
  "directories_without_sessions",
];

/** 会话状态 → 是否计入「运行中」（静态词表，用 `Record` 而非 `Set`）。 */
const IS_ACTIVE: Record<SessionStateView, true | undefined> = {
  running: true,
  idle: undefined,
  pending: undefined,
};

/** 会话状态 → 是否计入「待处理」（静态词表，用 `Record` 而非 `Set`）。 */
const IS_PENDING: Record<SessionStateView, true | undefined> = {
  pending: true,
  idle: undefined,
  running: undefined,
};

/**
 * 从快照资源构造目录视图列表。
 *
 * `workspaces` 是快照的目录资源（`{ directoryId, path }`），展示名与消歧段由
 * {@link directoryNamesOf} 按整份目录集派生。`sessions` 是快照的会话摘要。
 * 会话通过 `summary.workspace.directoryId` 归属到目录；未指定目录的会话（`workspace` 缺席或为
 * `null`）不计入任何目录项，也不凭空造一个目录。
 */
export function buildDirectoryViews(
  workspaces: readonly WorkspaceCatalogEntry[],
  sessions: readonly SessionSummary[],
  options: { readonly localLifecycleNegotiated?: boolean } = {},
): DirectoryListState {
  const lifecycleNegotiated = options.localLifecycleNegotiated === true;
  if (workspaces.length === 0) {
    return { kind: "empty", empty: "no_directories" };
  }

  const countsById = new Map<DirectoryId, { total: number; active: number; pending: number }>();
  for (const workspace of workspaces) {
    if (!countsById.has(workspace.directoryId)) {
      countsById.set(workspace.directoryId, { total: 0, active: 0, pending: 0 });
    }
  }

  for (const session of sessions) {
    const directoryId = session.workspace?.directoryId;
    if (directoryId === undefined) continue;
    const counts = countsById.get(directoryId);
    if (counts === undefined) continue;
    counts.total += 1;
    // 每个会话只按**一个**主状态计数：本机活跃程度可用时以它为准，否则回落任务三态。
    // 同一会话的多条有效交互不会让 `pending` 增加第二次（这里按摘要而不是按交互遍历）。
    const localActivity: LocalActivityView | null = localActivityOf(session, lifecycleNegotiated);
    if (localActivity === "pending") {
      counts.pending += 1;
    } else if (localActivity === "active") {
      counts.active += 1;
    } else if (localActivity === "idle") {
      // 本机资源已确认释放且无任务/交互：既不计活跃也不计待处理。
    } else {
      const state = session.state as SessionStateView;
      if (IS_ACTIVE[state] === true) counts.active += 1;
      if (IS_PENDING[state] === true) counts.pending += 1;
    }
  }

  const names = directoryNamesOf(workspaces);
  const directories: DirectoryView[] = [];
  const seen = new Set<DirectoryId>();
  for (const workspace of workspaces) {
    if (seen.has(workspace.directoryId)) continue;
    seen.add(workspace.directoryId);
    const derived = names.get(workspace.directoryId) ?? { displayName: workspace.path, pathTail: null };
    directories.push({
      id: workspace.directoryId,
      displayName: derived.displayName,
      pathTail: derived.pathTail,
      path: workspace.path,
      counts: countsById.get(workspace.directoryId) ?? { total: 0, active: 0, pending: 0 },
    });
  }

  const hasAnySession = directories.some((directory) => directory.counts.total > 0);
  if (!hasAnySession) {
    return { kind: "empty", empty: "directories_without_sessions" };
  }
  return { kind: "ready", directories };
}
