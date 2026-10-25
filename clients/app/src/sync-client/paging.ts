/** Messages and structured history share an authority sequence domain. Unknown legacy rows form a stable prefix. */
import type { DecimalString, SessionReadResult, SnapshotItemHistory, SnapshotItemMessages, Uuid } from "../protocol";
import { isDecimalString } from "./wire";

export interface ReadCursor {
  readonly sequence?: DecimalString;
  readonly createdAt: string;
  readonly messageId: Uuid;
}

export class IncompleteReadCursorError extends Error {
  constructor(detail: string) {
    super(`session.read 游标分量缺失或非法：${detail}`);
    this.name = "IncompleteReadCursorError";
  }
}

/** Canonical decimals stay strings; BigInt comparison preserves positions above 2^53. */
export function compareMessagePosition(a: SnapshotItemMessages, b: SnapshotItemMessages): number {
  if (a.sequence !== undefined && !isDecimalString(a.sequence)) throw new IncompleteReadCursorError("sequence");
  if (b.sequence !== undefined && !isDecimalString(b.sequence)) throw new IncompleteReadCursorError("sequence");
  if (a.sequence === undefined && b.sequence !== undefined) return -1;
  if (b.sequence === undefined && a.sequence !== undefined) return 1;
  if (a.sequence !== undefined && b.sequence !== undefined) {
    return BigInt(a.sequence) < BigInt(b.sequence) ? -1 : BigInt(a.sequence) > BigInt(b.sequence) ? 1 : 0;
  }
  return a.createdAt < b.createdAt ? -1 : a.createdAt > b.createdAt ? 1 :
    a.messageId < b.messageId ? -1 : a.messageId > b.messageId ? 1 : 0;
}

export function earliestCursor(messages: readonly SnapshotItemMessages[]): ReadCursor | null {
  let earliest: SnapshotItemMessages | undefined;
  for (const candidate of messages) {
    cursorOf(candidate);
    if (earliest === undefined || compareMessagePosition(candidate, earliest) < 0) earliest = candidate;
  }
  return earliest === undefined ? null : cursorOf(earliest);
}

export function cursorOf(message: SnapshotItemMessages): ReadCursor {
  if (typeof message.createdAt !== "string" || message.createdAt.length === 0) throw new IncompleteReadCursorError("createdAt");
  if (typeof message.messageId !== "string" || message.messageId.length === 0) throw new IncompleteReadCursorError("messageId");
  if (message.sequence !== undefined && !isDecimalString(message.sequence)) throw new IncompleteReadCursorError("sequence");
  return { createdAt: message.createdAt, messageId: message.messageId,
    ...(message.sequence === undefined ? {} : { sequence: message.sequence }) };
}

export interface PageRequest {
  readonly sessionId: Uuid;
  readonly include: readonly string[];
  /** 省略即取最新一页（§9.2 / spec 场景「省略游标时返回最新一页」）。 */
  readonly before?: ReadCursor;
  /** 历史条目的续取游标（会话内事件序号）；与 `before` 互不影响（D2）。 */
  readonly beforeHistory?: HistoryCursor;
  /** 省略时由服务端用配置默认页（spec 场景「同步面未提供参数时使用默认页」）。 */
  readonly limit?: number;
}

/** 构造 `session.read` 的 payload；三个游标键都只在提供时出现。 */
export function buildReadPayload(request: PageRequest): {
  readonly include: readonly string[];
  readonly before?: ReadCursor;
  readonly beforeHistory?: HistoryCursor;
  readonly limit?: number;
} {
  const base = { include: request.include };
  if (request.before === undefined && request.beforeHistory === undefined && request.limit === undefined) {
    return base;
  }
  return {
    ...base,
    ...(request.before === undefined ? {} : { before: request.before }),
    ...(request.beforeHistory === undefined ? {} : { beforeHistory: request.beforeHistory }),
    ...(request.limit === undefined ? {} : { limit: request.limit }),
  };
}

/** 一页的读取结果与「是否还有更早内容」。 */
export interface LoadedPage {
  readonly result: SessionReadResult;
  readonly messages: readonly SnapshotItemMessages[];
  /** 是否仍有更早内容；spec 的 `hasEarlier` 是必填布尔，缺失即视为协议违规。 */
  readonly hasEarlier: boolean;
  /** 下一页的游标；`hasEarlier` 为假或本页无消息时为 `null`。 */
  readonly nextBefore: ReadCursor | null;
}

/**
 * 把 `command.result` 的 `SessionReadResult` 投影成一页。
 *
 * `hasEarlier` 为真但本页没有消息是一个**自相矛盾**的结果：此时无法构造续取游标，
 * 直接抛错而不是静默停在第一页（否则用户会以为已到最早一条）。
 */
export function toLoadedPage(result: SessionReadResult): LoadedPage {
  const messages = result.resources.messages ?? [];
  for (const message of messages) cursorOf(message);
  if (typeof result.hasEarlier !== "boolean") {
    throw new Error("session.read 结果缺必填的 hasEarlier");
  }
  if (result.hasEarlier && messages.length === 0) {
    throw new Error("session.read 结果称仍有更早内容但未返回任何消息");
  }
  return {
    result,
    messages,
    hasEarlier: result.hasEarlier,
    nextBefore: result.hasEarlier ? earliestCursor(messages) : null,
  };
}

/**
 * 合并连续两页前的两项校验。
 *
 * 1. **不重**：两页出现同一 `messageId` 即判为重叠——说明续取游标算错或服务端页边界漂移，
 *    继续拼接会让同一条消息显示两次，调用方必须停止拼接。
 * 2. **不漏（页边界）**：`older` 里不得出现比 `newer` 最早一条**更新**的消息。正常的服务端
 *    升序页不可能出现这种结果，它只会来自「续取游标越过了一部分消息」——即漏读。
 *
 * 「不漏」的完整判定需要服务端提供该页的边界声明，wire 上没有这个字段；这里能验证的只是
 * 上述可观测的边界违例，不是页级完整性的证明。
 */
export function assertPagesDoNotOverlap(
  newer: readonly SnapshotItemMessages[],
  older: readonly SnapshotItemMessages[],
): void {
  const olderIds = new Set(older.map((message) => message.messageId));
  for (const message of newer) {
    if (olderIds.has(message.messageId)) {
      throw new Error(`相邻两页重叠：messageId ${message.messageId} 同时出现`);
    }
  }
  const newestOlder = older.reduce<SnapshotItemMessages | undefined>((latest, message) =>
    latest === undefined || compareMessagePosition(message, latest) > 0 ? message : latest, undefined);
  const oldestNewer = newer.reduce<SnapshotItemMessages | undefined>((earliest, message) =>
    earliest === undefined || compareMessagePosition(message, earliest) < 0 ? message : earliest, undefined);
  if (newestOlder !== undefined && oldestNewer !== undefined && compareMessagePosition(newestOlder, oldestNewer) > 0) {
    throw new Error("相邻两页漏读：较早页含有更新位置的消息");
  }
}
// ── 结构化历史的分页（D2：会话内序号游标，与消息分页完全分离）────────────────────

/**
 * 历史条目的续取游标（`$defs.sessionReadBeforeHistory`）：**会话内事件序号**。
 *
 * 它与 {@link ReadCursor} 是两种东西，刻意不共用一个类型：序号在提交时分配、天然单调唯一，
 * 因此**不需要**并列次序规则，也不能被 `(createdAt, eventId)` 之类的复合游标替代
 * （服务端 MUST NOT 用其它标识代替它）。
 */
export type HistoryCursor = string;

/** 从一条历史条目取会话内序号；缺失或形状非法即抛错（不得猜测或补全）。 */
export function historyCursorOf(item: SnapshotItemHistory): HistoryCursor {
  const sequence = item.sessionSequence;
  if (typeof sequence !== "string" || !isDecimalString(sequence)) {
    throw new IncompleteReadCursorError("sessionSequence");
  }
  return sequence;
}

/**
 * 取一页历史条目里**最早**一条的会话内序号；无条目返回 `null`。
 *
 * 按数值（`BigInt`）比较而不是字典序：序号是十进制字符串，位数不同时字典序会给出错答案
 * （`"10" < "9"`）。
 */
export function earliestHistoryCursor(items: readonly SnapshotItemHistory[]): HistoryCursor | null {
  let earliest: HistoryCursor | null = null;
  for (const item of items) {
    const sequence = historyCursorOf(item);
    if (earliest === null || BigInt(sequence) < BigInt(earliest)) earliest = sequence;
  }
  return earliest;
}

/** 一页历史条目与其页标记。 */
export interface LoadedHistoryPage {
  readonly result: SessionReadResult;
  readonly items: readonly SnapshotItemHistory[];
  /** `historyHasEarlier` 为真才说明还有更早条目；该键缺席时**不**推断（只说明本次无关）。 */
  readonly hasEarlier: boolean;
  /** 下一页的续取游标；`hasEarlier` 为假或本页无条目时为 `null`。 */
  readonly nextBeforeHistory: HistoryCursor | null;
}

/**
 * 把 `SessionReadResult` 的 `resources.history` 投影成一页。
 *
 * 与 {@link toLoadedPage} 的关键差别：这里的页标记是**可选**的 `historyHasEarlier`
 * （缺席表示这次请求没要历史，不能当成「没有更早条目」的断言之外的东西）。`historyHasEarlier`
 * 为真但一条都没返回同样是**自相矛盾**的结果——无法构造续取游标，必须抛错。
 */
export function toLoadedHistoryPage(result: SessionReadResult): LoadedHistoryPage {
  const items = result.resources.history ?? [];
  const marker = result.historyHasEarlier;
  if (marker !== undefined && typeof marker !== "boolean") {
    throw new Error("session.read 结果的 historyHasEarlier 既非布尔也非缺席");
  }
  const hasEarlier = marker === true;
  if (hasEarlier && items.length === 0) {
    throw new Error("session.read 结果称历史仍有更早条目但未返回任何条目");
  }
  return {
    result,
    items,
    hasEarlier,
    nextBeforeHistory: hasEarlier ? earliestHistoryCursor(items) : null,
  };
}

/**
 * 相邻两页历史条目的唯一校验：**不重**（同一 `eventId` 出现两次）。
 *
 * 消息分页还需要一条「不漏」的时间边界检查，历史这里不需要——序号单调唯一，
 * 页边界漂移在序号上直接可见（较早页的序号必然更小），不存在「同一时刻并列」的边界。
 */
export function assertHistoryPagesDoNotOverlap(
  newer: readonly { readonly eventId: string }[],
  older: readonly { readonly eventId: string }[],
): void {
  const olderIds = new Set(older.map((item) => item.eventId));
  for (const item of newer) {
    if (olderIds.has(item.eventId)) {
      throw new Error(`相邻两页历史条目重叠：eventId ${item.eventId} 同时出现`);
    }
  }
}
