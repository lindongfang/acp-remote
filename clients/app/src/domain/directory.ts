/**
 * 目录视图模型（目录页、目录详情页与对话页左栏的基础层）。
 *
 * 关键约束（`pwa-web-client` 的目录页需求）：
 *
 * - 目录项的展示名与别名是两个固定键；路径**只**在服务端下发时才有（快照条目的可选 `path`
 *   键，`core.workspace-path.v1` 门控）。本层**逐字**透传那个值，MUST NOT 拼接、规范化或
 *   反查本机路径：`path` 要么等于服务端下发的那个字符串，要么是「没有下发」的 `null`。
 * - 「要不要把路径显示出来」不归本层：规格只允许在同名目录、目录详情页这两处呈现它，
 *   那条判定由 `features/` 按同一份快照算出（见 `DirectoryRowModel.pathTail`）。
 * - 同名目录之间**不**合并：`DirectoryId` 用别名，`displayName` 可以重复。
 * - 目录内的会话数、运行中数量与待处理数量由会话摘要得出（`SessionSummary.state`），
 *   因此断连时仍可呈现。
 * - 「目录存在但无会话」与「一个目录都没有」是两种可区分的状态。
 */

import type { SessionSummary, WorkspaceCatalogEntry, WorkspaceRef } from "../protocol/common";
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
  /**
   * 服务端下发的逐字路径；未协商 `core.workspace-path.v1` 时该键**缺席**，这里为 `null`。
   *
   * 它是同一个字符串的搬运，不是推导结果：调用方 MUST NOT 拿展示名或别名拼补它，
   * 也 MUST NOT 在它为 `null` 时用别的字段顶替。
   */
  readonly path: string | null;
  readonly counts: DirectorySessionCounts;
}

/**
 * 快照目录条目上的逐字路径（`WorkspaceCatalogEntry` 的可选键 `path`）。
 *
 * 参数就取镜像里的那个值对象（快照 `workspaces` 的元素类型），因此直接读 `workspace.path`：
 * 不再需要按开放对象做双断言，键的在场与否由类型表达。
 *
 * 那个键在场时 `wire` 已校验过它是**非空字符串**（`sync-client/wire.ts` 对空串与非串一律拒收
 * 整条 chunk），本层仍逐字判一次形状：`DirectoryView.path` 的契约是「缺席、非字符串或空串一律为
 * `null`」——三种都是「没有可用路径」，调用方据此走「不呈现路径」的分支，MUST NOT 退回展示名或
 * 别名去拼一段像路径的文本；不经 `wire` 的调用方（组件测试直接喂开放对象）也走同一条判定。
 */
export function workspacePathOf(workspace: WorkspaceCatalogEntry): string | null {
  const value = workspace.path;
  return typeof value === "string" && value.length > 0 ? value : null;
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
 * `workspaces` 是快照的目录资源：快照里的元素类型是 `WorkspaceCatalogEntry`（比 `WorkspaceRef` 多
 * 一个可选的逐字 `path`），本函数只用别名与展示名两个键，因此按 `WorkspaceRef` 收——路径那一路由
 * `workspacePathOf` 负责。`sessions` 是快照的会话摘要。
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
      // 逐字透传（缺席即 `null`）：见 `workspacePathOf`，这里不做任何拼接或规范化。
      path: workspacePathOf(workspace),
      counts,
    };
  });

  const hasAnySession = directories.some((directory) => directory.counts.total > 0);
  if (!hasAnySession) {
    return { kind: "empty", empty: "directories_without_sessions" };
  }
  return { kind: "ready", directories };
}
