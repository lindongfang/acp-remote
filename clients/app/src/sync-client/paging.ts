/**
 * `session.read` 分页（R2「会话明细经分页读取」+ R3「游标由时间与标识复合构成」+ D2）。
 *
 * ## 复合游标为什么必须是 `(createdAt, messageId)`
 *
 * D2 的理由在本层直接成立：`snapshotItem.messages` **没有序号字段**，而 §3 明令
 * 「排序不得依赖 UUID」，因此：
 *
 * - 单用 `before: messageId` —— 没有排序依据；
 * - 单用 `before: createdAt` —— 同一毫秒的多条会漏或重（spec 场景「同一时刻的多条消息不重不漏」）。
 *
 * 所以本模块的游标构造强制**两个分量都在**：`earliestCursor` 从一页的最早一条消息取
 * `(createdAt, messageId)`，并**断言**两键齐备；缺任一分量直接抛错而不是补默认值
 * （spec 场景「游标不可由标识前缀构造」要求服务端拒绝，客户端同样不得猜测）。
 *
 * ## 比较为什么用字符串比较
 *
 * `createdAt` 是固定形状 `YYYY-MM-DDTHH:MM:SS.sssZ`（`$defs.timestamp`），字典序与时间序一致；
 * `messageId` **只作为 tie-breaker 参与相等判定之外的次序**，绝不用作主排序键
 * （spec 场景「不按标识排序」）。同一 `createdAt` 内消息在服务端已有稳定次序，
 * 客户端把该页最早一条的 `messageId` 原样带上即可续取。
 */

import type { SessionReadResult, SnapshotItemMessages, Uuid } from "../protocol";

/** 复合游标（`$defs.sessionReadBefore`）。 */
export interface ReadCursor {
  readonly createdAt: string;
  readonly messageId: Uuid;
}

/** 游标分量缺失：客户端不得猜测或补全（spec 场景「游标不可由标识前缀构造」）。 */
export class IncompleteReadCursorError extends Error {
  constructor(detail: string) {
    super(`session.read 游标分量缺失：${detail}`);
    this.name = "IncompleteReadCursorError";
  }
}

/**
 * 取一页里最早一条消息的复合游标；无消息返回 `null`。
 *
 * 「最早」只按 `createdAt` 判定：**不**用 `messageId` 参与排序（spec 场景「不按标识排序」）。
 * 同一 `createdAt` 的多条按页内出现顺序取第一条——服务端已给出该时刻内的稳定次序，
 * 客户端保留它而不是自行重排。
 *
 * 实现上按时间取最小而不是直接取 `messages[0]`：这样即便某一页的顺序不合预期，
 * 续取游标仍指向真正最早的那一条，而不是跳过一整段历史。
 */
export function earliestCursor(
  messages: readonly SnapshotItemMessages[],
): ReadCursor | null {
  let earliest: SnapshotItemMessages | undefined;
  for (const candidate of messages) {
    if (earliest === undefined || candidate.createdAt < earliest.createdAt) {
      earliest = candidate;
    }
  }
  if (earliest === undefined) return null;
  return cursorOf(earliest);
}

/**
 * 从一条消息取复合游标；两键缺一即抛错。
 *
 * 不补全缺失分量（spec 场景「游标不可由标识前缀构造」：服务端拒绝，客户端同样不得猜测）。
 */
export function cursorOf(message: SnapshotItemMessages): ReadCursor {
  if (typeof message.createdAt !== "string" || message.createdAt.length === 0) {
    throw new IncompleteReadCursorError("createdAt");
  }
  if (typeof message.messageId !== "string" || message.messageId.length === 0) {
    throw new IncompleteReadCursorError("messageId");
  }
  return { createdAt: message.createdAt, messageId: message.messageId };
}
/** 一次向上翻页的读取输入。 */
export interface PageRequest {
  readonly sessionId: Uuid;
  readonly include: readonly string[];
  /** 省略即取最新一页（§9.2 / spec 场景「省略游标时返回最新一页」）。 */
  readonly before?: ReadCursor;
  /** 省略时由服务端用配置默认页（spec 场景「同步面未提供参数时使用默认页」）。 */
  readonly limit?: number;
}

/** 构造 `session.read` 的 payload；`before`/`limit` 只在提供时出现。 */
export function buildReadPayload(request: PageRequest): {
  readonly include: readonly string[];
  readonly before?: ReadCursor;
  readonly limit?: number;
} {
  const base = { include: request.include };
  if (request.before === undefined && request.limit === undefined) return base;
  return {
    ...base,
    ...(request.before === undefined ? {} : { before: request.before }),
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
 * 合并连续两页，验证「不重不漏」。
 *
 * 用于「向上滚动加载更早的用户输入」的滚动拼接：若拼接后出现重复 `messageId`，
 * 说明两页重叠，调用方必须停止拼接而不是产生重复呈现。
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
}