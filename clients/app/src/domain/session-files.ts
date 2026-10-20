/**
 * 会话文件栏的**领域模型**：两个来源（全部文件 / Git 变更）的数据形状。
 *
 * 本层只做纯计算：不派发命令、不读平台 API、不持有连接状态。两个来源的判定必须在类型上分开，
 * 不能用一个「文件列表」把它们混在一起：
 *
 * - 全部文件：磁盘枚举 + 分页 + Git 忽略开关（`AllFilesDirectoryView`）；
 * - Git 变更：未提交状态 + 分页（`GitChangesView`）。
 *
 * 会话改动的呈现只属于对话时间线：文件栏不再按会话历史归并改动清单，也不提供第三种来源。
 * 旧页面状态里可能还带着已被移除的 `session` 取值，因此来源只在**输入边界**用
 * {@link normalizeFileSource} 归一：除 `git` 之外的一切取值（含旧的 `session`）都回到 `all`。
 */

import type {
  GitDiffBase,
  GitDiffKind,
  GitDiffUnavailableReason,
  GitState,
  GitStatusEntry,
  SessionFileEntry,
} from "../protocol/session-files";
import type { PublicError } from "../protocol/common";

/** 文件栏的两个来源。名称与 spec 的「全部文件 / Git 变更」一一对应。 */
export type FileSource = "all" | "git";

/** 两个来源的固定顺序（UI 渲染与测试都用它，避免各处再抄一份）。 */
export const FILE_SOURCES: readonly FileSource[] = ["all", "git"];

/** 默认来源：首次进入会话是「全部文件」。 */
export const DEFAULT_FILE_SOURCE: FileSource = "all";

/**
 * 来源的**输入边界**归一：只有 `git` 保持原样，其余取值（旧的 `session`、`null`、未知字符串）
 * 一律回到 `all`。状态 setter、组件回调与任何恢复的旧选择都必须经过它。
 */
export function normalizeFileSource(value: unknown): FileSource {
  return value === "git" ? "git" : "all";
}

/** 全部文件：某一层目录的已加载页。 */
export interface AllFilesDirectoryView {
  /** 根内相对路径；空串表示会话根。 */
  readonly path: string;
  readonly entries: readonly SessionFileEntry[];
  /** 下一层的翻页位置；`null` 表示这一层已经列完。 */
  readonly next: string | null;
  readonly gitState: GitState;
  readonly loading: boolean;
  /** 本次失败原因；保留已成功加载的 `entries`，不用空列表覆盖成功结果。 */
  readonly error: PublicError | null;
}

/** 全部文件来源：忽略开关与逐层缓存。 */
export interface AllFilesSourceView {
  readonly hideIgnored: boolean;
  readonly directories: ReadonlyMap<string, AllFilesDirectoryView>;
}

/** Git 变更来源：一页合并状态。 */
export interface GitChangesView {
  readonly gitState: GitState | null;
  readonly entries: readonly GitStatusEntry[];
  /** 下一页位置；`null` 表示已列完。 */
  readonly next: string | null;
  readonly loading: boolean;
  /** 失败原因；失败时保留已有 `entries`。 */
  readonly error: PublicError | null;
}

/** Git 差异的只读视图（`session.git.diff` 的 completed 结果）。 */
export interface GitDiffView {
  readonly path: string;
  readonly base: GitDiffBase;
  readonly kind: GitDiffKind;
  readonly diffLines: readonly string[];
  readonly unavailable: GitDiffUnavailableReason | null;
  readonly truncated: boolean;
}

/** 一个来源暂不可用的原因（旧主机或远程导入会话）。 */
export type FileSourceUnsupportedReason = "feature_unavailable" | "remote_imported";

/** 不可用原因的显式文案；`null` 表示该来源可用。 */
export const FILE_SOURCE_UNSUPPORTED_NOTICES: Readonly<Record<FileSourceUnsupportedReason, string>> = {
  feature_unavailable: "此主机未协商会话文件能力（core.session-files.v1），全部文件与 Git 变更暂不可用。",
  remote_imported: "远程导入会话不读取本机目录；全部文件与 Git 变更暂不支持，已收到的对话差异仍可在对话中查看。",
};
