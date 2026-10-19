/**
 * 文件栏的 feature 状态与 view model（WP4）。
 *
 * ## 三来源分开、互斥内容态
 *
 * `SessionFilesState` 里三个来源各有自己的形状：`all`（逐层目录 + 忽略开关）、`git`（未提交
 * 状态）、`session`（由 `domain/session-files.ts` 从已持久化记录归并）。它们不共享一个
 * 「文件数组」，因此一个来源刷新失败或不可用不会把另一个来源的结果冲掉。
 *
 * 中间查看区只有 `conversation | file` **一个**判别值（`FileContentState`）。`file` 内部再分
 * `loading | ready | error`，并携带返回上下文 `{ sessionId, source, path }` 与请求代次
 * （`SessionFilesState.generation`）：响应只有在代次与上下文都匹配时才会被采纳，所以
 * 「A 的响应迟到」不会覆盖 B，「关闭/切换会话后的迟到响应」也不会重新打开文件。
 *
 * ## 刷新协调
 *
 * 进入来源视图、用户手动刷新、当前会话 `file.changed` 三类触发都在 `ClientStore` 里收敛：
 * 突发事件先合并到一个短退避定时器，限流错误按 `retryAfterMs` 再退避；失败只记 `error`，
 * 不清空已成功加载的条目。这里不装任何磁盘 watcher，也不把正文写进持久缓存。
 *
 * ## 不拥有上层状态
 *
 * 草稿、时间线滚动锚点、条目展开状态都属于对话页的上层状态；本模块的状态里**没有**这些字段，
 * 因此文件视图的开关在结构上不可能清理它们（测试用「只改 files 切片」的方式钉住这一点）。
 */

import type { PublicError, Uuid } from "../protocol";
import type { StructuredRecord } from "../domain/timeline";
import type {
  AllFilesDirectoryView,
  FileSource,
  FileSourceUnsupportedReason,
  GitChangesView,
  GitDiffView,
  SessionChangedFile,
  SessionChangesModel,
  SessionChangeRecord,
  HistoryCoverage,
} from "../domain/session-files";
import {
  DEFAULT_FILE_SOURCE,
  FILE_SOURCE_UNSUPPORTED_NOTICES,
  buildSessionChangesModel,
} from "../domain/session-files";

// UI 与测试从 feature 层消费同一份来源词表；不要在组件里再抄一份。
export type { FileSource } from "../domain/session-files";
export { FILE_SOURCES } from "../domain/session-files";

/** 文本/二进制预览的客户端预算（服务端还会按自己的预算下发 `previewTruncated`）。 */
export const FILE_PREVIEW_BUDGET_BYTES = 256 * 1024;

/**
 * 图片预览的客户端预算。
 *
 * 服务端对图片按 4 MiB 分块下发，因此客户端不能沿用文本的 256 KiB，否则合法图片会被误判为
 * `resource.result_too_large`。预算、完成判定与 `previewTruncated` 阈值统一按内容 kind 选择。
 */
export const FILE_IMAGE_PREVIEW_BUDGET_BYTES = 4 * 1024 * 1024;

/** 突发事件合并窗口：窗口内的多次 `file.changed` 只触发一次来源刷新。 */
export const FILE_REFRESH_DEBOUNCE_MS = 150;

/** 限流退避上限：服务端给出的 `retryAfterMs` 超过它时按上限等待，避免无限期沉默。 */
export const FILE_REFRESH_MAX_RETRY_MS = 30_000;

/** 文件来源的展示名（UI 与测试共用同一份词表）。 */
export const FILE_SOURCE_LABELS: Readonly<Record<FileSource, string>> = {
  all: "全部文件",
  git: "Git 变更",
  session: "会话变更",
};

/** 全部文件来源状态：忽略开关 + 逐层缓存（按根内相对路径，空串为根）。 */
export interface AllFilesSourceState {
  readonly hideIgnored: boolean;
  readonly directories: ReadonlyMap<string, AllFilesDirectoryView>;
  /** `feature_unavailable` / `remote_imported`；非 `null` 时**不派发**任何本机查询。 */
  readonly unsupported: FileSourceUnsupportedReason | null;
}

/** Git 变更来源状态：成功后保留条目，失败只补 `error`。 */
export interface GitChangesSourceState extends GitChangesView {
  readonly unsupported: FileSourceUnsupportedReason | null;
}

/** 一个会话的三来源状态（会话变更本身由记录派生，不在这里存事件）。 */
export interface SessionFilesSourcesState {
  readonly all: AllFilesSourceState;
  readonly git: GitChangesSourceState;
}

/** 文件内容的返回上下文：响应必须与会话、来源、路径与代次同时匹配才被采纳。 */
export interface FileViewContext {
  readonly sessionId: Uuid;
  readonly source: FileSource;
  readonly path: string;
}

/** 文本/图片/二进制预览（同一版本的有界字节块，已按到达顺序拼接）。 */
export interface FilePreviewContent {
  readonly kind: "preview";
  readonly path: string;
  readonly fileKind: "text" | "image" | "binary";
  readonly sizeBytes: string;
  readonly version: string;
  /** 已拼接到的偏移（等于下一块的起点）。 */
  readonly offset: string;
  readonly bytes: Uint8Array;
  readonly eof: boolean;
  readonly previewTruncated: boolean;
}

/** Git 差异内容。 */
export interface FileGitDiffContent {
  readonly kind: "git_diff";
  readonly diff: GitDiffView;
}

/** 会话变更内容：该文件全部已加载的历次记录（不合成累计净差异）。 */
export interface FileSessionChangesContent {
  readonly kind: "session_changes";
  readonly path: string;
  readonly records: readonly SessionChangeRecord[];
}

export type FileContentView = FilePreviewContent | FileGitDiffContent | FileSessionChangesContent;

/** 互斥的中间内容态。查看期间文件改变只置 `stale`，不擅自替换正在阅读的内容。 */
export type FileContentState =
  | { readonly kind: "conversation" }
  | {
      readonly kind: "file";
      readonly context: FileViewContext;
      readonly generation: number;
      readonly stale: boolean;
      readonly status: "loading";
    }
  | {
      readonly kind: "file";
      readonly context: FileViewContext;
      readonly generation: number;
      readonly stale: boolean;
      readonly status: "ready";
      readonly content: FileContentView;
    }
  | {
      readonly kind: "file";
      readonly context: FileViewContext;
      readonly generation: number;
      readonly stale: boolean;
      readonly status: "error";
      readonly error: PublicError;
    };

/** 文件栏状态。 */
export interface SessionFilesState {
  readonly selectedSources: ReadonlyMap<Uuid, FileSource>;
  readonly sources: ReadonlyMap<Uuid, SessionFilesSourcesState>;
  readonly content: FileContentState;
  /** 内容请求代次：每次打开/关闭都递增，迟到的旧代次响应被丢弃。 */
  readonly generation: number;
}

/** 空目录页（尚未加载）。 */
export function emptyAllFilesDirectory(path: string): AllFilesDirectoryView {
  return { path, entries: [], next: null, gitState: "available", loading: false, error: null };
}

/** 空的 Git 来源。 */
export function emptyGitChanges(): GitChangesSourceState {
  return { gitState: null, entries: [], next: null, loading: false, error: null, unsupported: null };
}

/** 空的来源集合。 */
export function emptySources(): SessionFilesSourcesState {
  return {
    all: { hideIgnored: false, directories: new Map(), unsupported: null },
    git: emptyGitChanges(),
  };
}

/** 初始状态：没有会话、没有内容视图。 */
export function initialSessionFilesState(): SessionFilesState {
  return { selectedSources: new Map(), sources: new Map(), content: { kind: "conversation" }, generation: 0 };
}

/** 某会话当前选中的来源（默认「会话变更」）。 */
export function selectedSourceOf(state: SessionFilesState, sessionId: Uuid): FileSource {
  return state.selectedSources.get(sessionId) ?? DEFAULT_FILE_SOURCE;
}

/** 某会话的三来源状态（未创建过时返回空状态，不写入 map）。 */
export function sourcesOf(state: SessionFilesState, sessionId: Uuid): SessionFilesSourcesState {
  return state.sources.get(sessionId) ?? emptySources();
}

/** 内容是否属于该会话；不属于时中间区按对话呈现（切换会话后旧内容不显示）。 */
export function contentForSession(state: SessionFilesState, sessionId: Uuid): FileContentState {
  if (state.content.kind === "conversation") return state.content;
  return state.content.context.sessionId === sessionId ? state.content : { kind: "conversation" };
}

/** 三条来源的不可用文案（可用时为 `null`）。 */
export function sourceUnsupportedNotice(reason: FileSourceUnsupportedReason | null): string | null {
  return reason === null ? null : FILE_SOURCE_UNSUPPORTED_NOTICES[reason];
}

/** 文件栏 view model（WP5 消费的唯一形状）。 */
export interface SessionFilesPanelModel {
  readonly refreshState: { readonly kind: "available" | "loading" } | { readonly kind: "unavailable"; readonly reason: string };
  readonly refreshError: string | null;
  readonly sessionId: Uuid;
  readonly selectedSource: FileSource;
  readonly all: AllFilesSourceState;
  readonly git: GitChangesSourceState;
  readonly sessionChanges: SessionChangesModel;
  readonly content: FileContentState;
  readonly connectionOnline: boolean;
  readonly allNotice: string | null;
  readonly gitNotice: string | null;
  /** 会话变更历史范围说明；完整加载时为 `null`。 */
  readonly historyNotice: string | null;
  readonly historyCoverage: HistoryCoverage;
}

/** 组装文件栏 view model。 */
export function buildSessionFilesPanelModel(input: {
  readonly sessionId: Uuid;
  readonly state: SessionFilesState;
  /** 本会话已去重的结构化记录（实时 + 历史）。 */
  readonly records: readonly StructuredRecord[];
  readonly historyHasEarlier: boolean | null;
  readonly connectionOnline: boolean;
  readonly historyRefreshSupported?: boolean;
  readonly historyReadPending?: boolean;
  readonly historyRefreshError?: string | null;
}): SessionFilesPanelModel {
  const sources = sourcesOf(input.state, input.sessionId);
  const sessionChanges = buildSessionChangesModel(input.records, input.historyHasEarlier);
  const selectedSource = selectedSourceOf(input.state, input.sessionId);
  const unavailable = !input.connectionOnline ? "当前未连接，连接恢复后可刷新。"
    : selectedSource === "session" ? input.historyRefreshSupported !== true ? "此主机不支持读取会话历史，无法手动刷新会话变更；已收到的改动仍会自动显示。" : null
    : sourceUnsupportedNotice(selectedSource === "all" ? sources.all.unsupported : sources.git.unsupported);
  const loading = selectedSource === "session" ? input.historyReadPending === true
    : selectedSource === "git" ? sources.git.loading : [...sources.all.directories.values()].some(directory => directory.loading);
  return {
    refreshState: unavailable !== null ? { kind: "unavailable", reason: unavailable } : { kind: loading ? "loading" : "available" },
    refreshError: selectedSource === "session" ? input.historyRefreshError ?? null : null,
    sessionId: input.sessionId,
    selectedSource,
    all: sources.all,
    git: sources.git,
    sessionChanges,
    content: contentForSession(input.state, input.sessionId),
    connectionOnline: input.connectionOnline,
    allNotice: sourceUnsupportedNotice(sources.all.unsupported),
    gitNotice: sourceUnsupportedNotice(sources.git.unsupported),
    historyNotice: sessionChanges.historyNotice,
    historyCoverage: sessionChanges.historyCoverage,
  };
}

/** 一个会话变更文件的记录列表（按可靠路径/事件键取）；不存在返回 `null`。 */
export function sessionChangesForPath(
  model: SessionChangesModel,
  path: string,
): { readonly file: SessionChangedFile; readonly records: readonly SessionChangeRecord[] } | null {
  const file = model.files.find((candidate) => candidate.path === path || candidate.key === `path:${path}`);
  return file === undefined ? null : { file, records: file.records };
}
