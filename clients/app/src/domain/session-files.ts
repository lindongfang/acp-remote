/**
 * 会话文件栏的**领域模型**：三来源（全部文件 / Git 变更 / 会话变更）的数据形状与会话改动归并。
 *
 * 本层只做纯计算：不派发命令、不读平台 API、不持有连接状态。三个来源的判定必须在类型上分开，
 * 不能用一个「文件列表」把它们混在一起：
 *
 * - 全部文件：磁盘枚举 + 分页 + Git 忽略开关（`AllFilesDirectoryView`）；
 * - Git 变更：未提交状态 + 分页（`GitChangesView`）；
 * - 会话变更：已持久化 `file.changed` 按**可靠的根内完整相对路径**归并后的历次记录
 *   （`SessionChangesModel`）。
 *
 * ## 会话变更的归并规则
 *
 * - 只接受调用方已按会话筛过的结构化记录；同 `eventId` 重放只保留一条。
 * - 设备来源（`device`）与本机 CLI（`local_cli`）的 `file.changed` 属于手动修改，不并入
 *   会话变更；它们仍可在时间线上查看。
 * - 只有 `outsideWorkspace !== true` 且 `displayPath` 通过 `$defs.sessionRelativePath` 校验时才
 *   作为「同一文件」归并。文件名（basename）或越界路径**永不**作为归并键：同名不同路径不会
 *   被合并，根外事件按事件各自保留并标注不可读取。
 * - 每条记录保留自己的统计（`addedLines`/`deletedLines`）与差异区；模型**不提供**
 *   「累计新增/删除」——历次统计不能相加当作净差异。
 * - 只展示调用方已加载的记录；`historyHasEarlier` 为真时标注「部分历史」，
 *   为 `null`（尚未读取过历史页）时标注「范围未知」，不声称覆盖整个会话。
 */

import type {
  GitDiffBase,
  GitDiffKind,
  GitDiffUnavailableReason,
  GitState,
  GitStatusEntry,
  SessionFileEntry,
} from "../protocol/session-files";
import { isSessionRelativePath } from "../protocol/session-files";
import type { PublicError } from "../protocol/common";
import type { FileDiffView, StructuredRecord } from "./timeline";
import { buildTimelineCard, compareTimelinePosition, recordGroupOf } from "./timeline";

/** 文件栏的三个来源。名称与 spec 的「全部文件 / Git 变更 / 会话变更」一一对应。 */
export type FileSource = "all" | "git" | "session";

/** 三个来源的固定顺序（UI 渲染与测试都用它，避免各处再抄一份）。 */
export const FILE_SOURCES: readonly FileSource[] = ["all", "git", "session"];

/** 默认来源：首次进入会话是「会话变更」。 */
export const DEFAULT_FILE_SOURCE: FileSource = "session";

/** 会话变更的一条诊断入口数据（原文按不可信文本呈现，不在这里解析成差异）。 */
export interface SessionChangeDiagnostics {
  readonly eventId: string;
  readonly eventType: string;
  readonly createdAt: string;
  readonly sessionSequence: string | null;
  readonly globalSequence: string | null;
  /** 事件视图原样带出（诊断按 JSON 呈现；MUST NOT 解析成结构化差异）。 */
  readonly view: Readonly<Record<string, unknown>>;
}

/** 一次会话改动（单条 `file.changed`），保留自己的统计与差异区。 */
export interface SessionChangeRecord {
  readonly eventId: string;
  readonly changeId: string;
  readonly kind: string;
  readonly displayPath: string;
  readonly summary: string;
  /** 判定不出时为 `null`（省略而不是填零）。 */
  readonly addedLines: string | null;
  readonly deletedLines: string | null;
  readonly diff: FileDiffView | null;
  readonly sessionSequence: string | null;
  readonly createdAt: string;
  readonly diagnostics: SessionChangeDiagnostics;
}

/** 一个会话改动文件：由可靠路径归并后的全部已加载记录。 */
export interface SessionChangedFile {
  /** 稳定归并键：可靠路径用 `path:<相对路径>`，不可读取的事件用 `event:<eventId>`。 */
  readonly key: string;
  /** 可靠的根内完整相对路径；不可读取/越界时为 `null`。 */
  readonly path: string | null;
  readonly displayPath: string;
  /** 是否在工作目录外（或路径不可证明在根内）。 */
  readonly outsideWorkspace: boolean;
  /** 是否可打开（只有可靠根内路径才可打开当前内容/差异）。 */
  readonly readable: boolean;
  /** 不可读取时的显式说明；可读取时为 `null`。 */
  readonly unreadableReason: string | null;
  /** 该文件的全部已加载记录，按会话内 sequence 升序；单次统计各随其记录保留。 */
  readonly records: readonly SessionChangeRecord[];
}

/** 历史覆盖范围：只描述**已加载**的部分，不声称完整。 */
export type HistoryCoverage = "complete" | "partial" | "unknown";

/** 会话变更模型。 */
export interface SessionChangesModel {
  readonly files: readonly SessionChangedFile[];
  /** 文件数（不是改动次数）：清单按文件计数。 */
  readonly fileCount: number;
  readonly historyCoverage: HistoryCoverage;
  /** 需要显式呈现的范围说明；完整加载时为 `null`。 */
  readonly historyNotice: string | null;
}

/** 部分历史/范围未知时的说明文案（不冒充完整）。 */
export const HISTORY_COVERAGE_NOTICES: Readonly<Record<Exclude<HistoryCoverage, "complete">, string>> = {
  partial: "只展示已加载的会话历史；更早的改动仍可用既有的历史加载入口继续读取。",
  unknown: "只展示本设备已收到的会话改动；更早的历史可能尚未加载。",
};

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
  remote_imported: "远程导入会话只保留已收到的会话变更；全部文件与 Git 变更暂不支持。",
};

/** 会话内 sequence 的先后比较；两条都缺时退回时间位置。 */
function compareSessionChangeRecords(left: SessionChangeRecord, right: SessionChangeRecord): number {
  if (left.sessionSequence !== null && right.sessionSequence !== null && left.sessionSequence !== right.sessionSequence) {
    return BigInt(left.sessionSequence) < BigInt(right.sessionSequence) ? -1 : 1;
  }
  return compareTimelinePosition(
    { createdAt: left.createdAt, sequence: left.sessionSequence },
    { createdAt: right.createdAt, sequence: right.sessionSequence },
  );
}

/** 历史标记 → 范围模型。 */
function historyOf(historyHasEarlier: boolean | null): {
  readonly historyCoverage: HistoryCoverage;
  readonly historyNotice: string | null;
} {
  if (historyHasEarlier === true) {
    return { historyCoverage: "partial", historyNotice: HISTORY_COVERAGE_NOTICES.partial };
  }
  if (historyHasEarlier === false) return { historyCoverage: "complete", historyNotice: null };
  return { historyCoverage: "unknown", historyNotice: HISTORY_COVERAGE_NOTICES.unknown };
}

/**
 * 把已去重、已按会话筛过的结构化记录归并成文件清单。
 *
 * @param records 实时事件与历史条目合并、去重后的记录（调用方负责只传本会话）。
 * @param historyHasEarlier 历史页是否仍有更早条目；`null` = 尚未读过历史页。
 */
export function buildSessionChangesModel(
  records: readonly StructuredRecord[],
  historyHasEarlier: boolean | null,
): SessionChangesModel {
  const byEvent = new Map<string, StructuredRecord>();
  for (const record of records) {
    if (record.eventType !== "file.changed") continue;
    // 只把 Agent/节点产生的改动并入「会话变更」；设备或本机 CLI 的手动修改属于其他来源，
    // 它们仍会出现在时间线上，但不得因为同名/同路径被算成这个会话 Agent 的改动。
    if (record.originKind === "device" || record.originKind === "local_cli") continue;
    // 重放同一 `eventId` 只保留先到的一条；调用方通常已经去重，这里是第二道防线。
    if (!byEvent.has(record.eventId)) byEvent.set(record.eventId, record);
  }

  const files = new Map<string, { path: string | null; displayPath: string; outside: boolean; records: SessionChangeRecord[] }>();
  for (const record of byEvent.values()) {
    const card = buildTimelineCard(recordGroupOf(record), () => null);
    if (card === null || card.kind !== "file_changed") continue;

    // `outsideWorkspace` 是 core 的显式判定；即使展示路径看起来像相对路径，也不能据它去归并。
    const reliable = card.outsideWorkspace !== true && isSessionRelativePath(card.displayPath);
    const key = reliable ? `path:${card.displayPath}` : `event:${record.eventId}`;
    const entry = files.get(key) ?? {
      path: reliable ? card.displayPath : null,
      displayPath: card.displayPath,
      outside: !reliable,
      records: [],
    };
    entry.records.push({
      eventId: record.eventId,
      changeId: card.changeId,
      kind: card.changeKind,
      displayPath: card.displayPath,
      summary: card.summary,
      addedLines: card.addedLines,
      deletedLines: card.deletedLines,
      diff: card.diff,
      sessionSequence: record.sessionSequence,
      createdAt: record.createdAt,
      diagnostics: {
        eventId: record.eventId,
        eventType: record.eventType,
        createdAt: record.createdAt,
        sessionSequence: record.sessionSequence,
        globalSequence: record.globalSequence,
        view: record.view,
      },
    });
    files.set(key, entry);
  }

  const merged: SessionChangedFile[] = [...files.entries()]
    .map(([key, value]) => ({
      key,
      path: value.path,
      displayPath: value.displayPath,
      outsideWorkspace: value.outside,
      readable: value.path !== null,
      unreadableReason:
        value.path === null
          ? "这条改动只提供了文件名（或路径不在会话工作目录内），无法读取其内容。"
          : null,
      records: [...value.records].sort(compareSessionChangeRecords),
    }))
    .sort((left, right) => {
      const leftName = left.path ?? left.displayPath;
      const rightName = right.path ?? right.displayPath;
      if (leftName !== rightName) return leftName < rightName ? -1 : 1;
      return left.key < right.key ? -1 : left.key > right.key ? 1 : 0;
    });

  return { files: merged, fileCount: merged.length, ...historyOf(historyHasEarlier) };
}
