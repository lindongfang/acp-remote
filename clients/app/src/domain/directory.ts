/**
 * 目录视图模型（R15「目录页以目录为主角且可离线渲染」的基础层）。
 *
 * 关键约束（`pwa-web-client` 的目录页需求）：
 *
 * - 目录项**只有**展示名与别名；`WorkspaceRef` 本身就不含路径，本层的输出类型里也**没有**
 *   任何可以反推路径的字段（没有 id 派生自路径、没有父目录、没有末段）。
 * - 同名目录之间**不**通过路径片段消歧，只能用别名区分。因此 `DirectoryId` 用别名，
 *   `displayName` 可以重复。
 * - 目录内的会话数、运行中数量与待处理数量由会话摘要得出（`SessionSummary.state`），
 *   因此断连时仍可呈现。
 * - 「目录存在但无会话」与「一个目录都没有」是两种可区分的状态。
 */

import type { SessionSummary, WorkspaceRef } from "../protocol/common";
import type { SessionStateView } from "./session-summary";

/** 目录的稳定标识：用别名（wire 上唯一的目录标识，且不含路径信息）。 */
export type DirectoryId = string;

/** 目录的会话汇总，全部由会话摘要得出。 */
export interface DirectorySessionCounts {
  /** 该目录全部会话数（含已关闭）。 */
  readonly total: number;
  /** `state` 为 `queued` 或 `running` 的会话数。 */
  readonly active: number;
  /** `state` 为 `waiting_input` 或 `waiting_permission` 的会话数。 */
  readonly pending: number;
}

/** 一个目录项。 */
export interface DirectoryView {
  readonly id: DirectoryId;
  /** 展示名；同名允许，由别名（`id`）区分。 */
  readonly displayName: string;
  /** 别名，wire 上的唯一标识；`session.create` 的 `workspaceAlias` 即取此值。 */
  readonly alias: string;
  readonly counts: DirectorySessionCounts;
}

/** 空态：本机没有任何目录，或目录存在但都没有会话。 */
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
  queued: true,
  running: true,
  idle: undefined,
  waiting_input: undefined,
  waiting_permission: undefined,
  failed: undefined,
  closed: undefined,
};

/** 会话状态 → 是否计入「待处理」（静态词表，用 `Record` 而非 `Set`）。 */
const IS_PENDING: Record<SessionStateView, true | undefined> = {
  waiting_input: true,
  waiting_permission: true,
  idle: undefined,
  queued: undefined,
  running: undefined,
  failed: undefined,
  closed: undefined,
};

/**
 * 从快照资源构造目录视图列表。
 *
 * `workspaces` 是快照的目录资源（`WorkspaceRef`），`sessions` 是快照的会话摘要。
 * 会话通过 `summary.workspace.alias` 归属到目录；未指定目录的会话（`workspace` 缺席或为 `null`）
 * 不计入任何目录项，也不凭空造一个目录。
 */
export function buildDirectoryViews(
  workspaces: readonly WorkspaceRef[],
  sessions: readonly SessionSummary[],
): DirectoryListState {
  if (workspaces.length === 0) {
    return { kind: "empty", empty: "no_directories" };
  }

  const byAlias = new Map<string, { total: number; active: number; pending: number }>();
  for (const workspace of workspaces) {
    byAlias.set(workspace.alias, { total: 0, active: 0, pending: 0 });
  }

  for (const session of sessions) {
    const alias = session.workspace?.alias;
    if (alias === undefined) continue;
    const counts = byAlias.get(alias);
    if (counts === undefined) continue;
    counts.total += 1;
    const state = session.state as SessionStateView;
    if (IS_ACTIVE[state] === true) counts.active += 1;
    if (IS_PENDING[state] === true) counts.pending += 1;
  }

  const directories: DirectoryView[] = workspaces.map((workspace) => {
    const counts = byAlias.get(workspace.alias) ?? { total: 0, active: 0, pending: 0 };
    return {
      id: workspace.alias,
      displayName: workspace.displayName,
      alias: workspace.alias,
      counts,
    };
  });

  const hasAnySession = directories.some((directory) => directory.counts.total > 0);
  if (!hasAnySession) {
    return { kind: "empty", empty: "directories_without_sessions" };
  }
  return { kind: "ready", directories };
}
