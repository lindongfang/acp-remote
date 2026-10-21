/**
 * 远程来源流与**原子读取重建**的纯判定（`design.md` D4；`docs/SYNC_PROTOCOL.md` §9.7/§11.5）。
 *
 * ## 为什么这一层只做纯函数
 *
 * 远程数据的生命周期全部是**运行期**的（不落盘、刷新即失去），而「一次读取覆盖了哪些增量」
 * 「旧页能不能覆盖新内容」这些判定与 React、socket、IndexedDB 都无关。把它们收敛到这里，
 * 测试可以直接构造 `(覆盖页, 增量事件)` 的先后次序，不必驱动整条连接。
 *
 * ## 覆盖语义（D4）
 *
 * 同一页 `session.read` 的 `messageCoverage` 是**同一次只读事务**的原子结果：每条消息的
 * `through` 都等于该页的 `readCursor` R，`lastDeltaIndex` 是这条消息在所返回完整正文里已覆盖的
 * 最后一个连续增量。于是：
 *
 * - `deltaIndex <= lastDeltaIndex` 的增量已经包含在正文里，**不得再拼接**；
 * - `lastDeltaIndex` 之后（或消息未出现在本次覆盖里）的增量仍然有效；
 * - 覆盖只适用于**本次返回的消息**，未返回消息的事件一律照常应用（不能因为一页没读到它而丢弃）。
 *
 * ## 旧页不回滚
 *
 * 每个消息独立保存「当前已应用到的覆盖位置」，只有新页的 `through` 不早于它时才允许替换该消息。
 * 分页没有跨页快照 token，因此两页可以各有自己的 R；迟到的一页只能合并它自己的范围。
 */

import type { MessageCoverage, OriginCursor } from "../protocol";

/** 单条消息已应用的覆盖位置（`through` + 最后一个连续增量索引）。 */
export interface RemoteMessageCoverage {
  readonly through: OriginCursor;
  readonly lastDeltaIndex: string | null;
}

/** `messageId → 覆盖位置`。 */
export type RemoteCoverageMap = ReadonlyMap<string, RemoteMessageCoverage>;

/** 空覆盖表（只读，调用方不需要每次新建）。 */
export const EMPTY_REMOTE_COVERAGE: RemoteCoverageMap = new Map();

/**
 * 比较两个来源游标。
 *
 * @returns 同 epoch 时返回 `-1 | 0 | 1`；**跨 epoch 返回 `null`**——两个来源序号域不可比，
 *   任何「谁更新」的推断都会把重置后的位置当成回退。
 */
export function compareOriginCursors(left: OriginCursor, right: OriginCursor): number | null {
  if (left.originEpoch !== right.originEpoch) return null;
  const l = BigInt(left.originSequence);
  const r = BigInt(right.originSequence);
  if (l < r) return -1;
  if (l > r) return 1;
  return 0;
}

/**
 * 已有连续位置与一条新位置取较新者。
 *
 * 跨 epoch 时**替换**：来源重置后旧序号不再代表任何可恢复位置，保留它会让下一次订阅
 * 带着已经作废的 epoch 去请求重放。
 */
export function advanceOriginCursor(current: OriginCursor | null, candidate: OriginCursor): OriginCursor {
  if (current === null) return candidate;
  const comparison = compareOriginCursors(candidate, current);
  if (comparison === null) return candidate;
  return comparison > 0 ? candidate : current;
}

/** `session.read` 的 `messageCoverage` → 覆盖表（不是覆盖字段时返回空表）。 */
export function coverageFromRead(items: readonly MessageCoverage[] | undefined): RemoteCoverageMap {
  if (items === undefined || items.length === 0) return EMPTY_REMOTE_COVERAGE;
  const coverage = new Map<string, RemoteMessageCoverage>();
  for (const item of items) {
    // 同一消息在合同里只出现一次；重复项按「较新者胜」合并，避免后出现的旧页覆盖新页。
    const existing = coverage.get(item.messageId);
    if (existing === undefined || (compareOriginCursors(item.through, existing.through) ?? 1) >= 0) {
      coverage.set(item.messageId, { through: item.through, lastDeltaIndex: item.lastDeltaIndex });
    }
  }
  return coverage;
}

/**
 * 合并两批覆盖位置：同一条消息**只允许前进**。
 *
 * 跨 epoch 的位置不可比，此时取新的那一份（来源已重置，旧位置没有意义）。
 */
export function mergeRemoteCoverage(
  current: RemoteCoverageMap,
  incoming: RemoteCoverageMap,
): RemoteCoverageMap {
  if (incoming.size === 0) return current;
  const merged = new Map(current);
  for (const [messageId, entry] of incoming) {
    const existing = merged.get(messageId);
    if (existing === undefined) {
      merged.set(messageId, entry);
      continue;
    }
    const comparison = compareOriginCursors(entry.through, existing.through);
    if (comparison === null || comparison >= 0) merged.set(messageId, entry);
  }
  return merged;
}

/** 页面的覆盖是否不早于该消息已应用的覆盖（旧页不得回滚新内容）。 */
export function coverageAllowsReplace(
  current: RemoteMessageCoverage | undefined,
  incoming: RemoteMessageCoverage | undefined,
): boolean {
  if (current === undefined) return true;
  if (incoming === undefined) return false;
  const comparison = compareOriginCursors(incoming.through, current.through);
  // 跨 epoch：来源已重置，新页是唯一有效的一份。
  return comparison === null || comparison >= 0;
}

/** `deltaIndex` 是否已被该消息的覆盖页包含。 */
export function isDeltaCovered(
  coverage: RemoteCoverageMap,
  messageId: string,
  deltaIndex: string,
): boolean {
  const entry = coverage.get(messageId);
  if (entry === undefined || entry.lastDeltaIndex === null) return false;
  // 两端都是 `decimalString`；用 BigInt 比较，避免 10 vs 9 的字符串误判。
  return BigInt(deltaIndex) <= BigInt(entry.lastDeltaIndex);
}

/** 创建/其它非增量事件是否已被该消息的覆盖页包含（按来源位置比较）. */
export function isEventCovered(
  coverage: RemoteCoverageMap,
  messageId: string,
  originSequence: string,
): boolean {
  const entry = coverage.get(messageId);
  if (entry === undefined) return false;
  const comparison = compareOriginCursors(
    { originEpoch: entry.through.originEpoch, originSequence },
    entry.through,
  );
  // 跨 epoch 的序号不可比：不能据此声称「已覆盖」，事件必须保留。
  return comparison !== null && comparison <= 0;
}

/**
 * 丢弃已被读取页覆盖的增量。
 *
 * @param deltas 该消息当前的易失增量（`deltaIndex → 文本`）。
 * @param entry 该消息的覆盖位置；缺席时不丢弃任何增量。
 */
export function pruneCoveredDeltas(
  deltas: ReadonlyMap<string, string>,
  entry: RemoteMessageCoverage | undefined,
): ReadonlyMap<string, string> {
  if (entry === undefined || entry.lastDeltaIndex === null || deltas.size === 0) return deltas;
  const last = BigInt(entry.lastDeltaIndex);
  let changed = false;
  const kept = new Map<string, string>();
  for (const [deltaIndex, text] of deltas) {
    if (BigInt(deltaIndex) <= last) {
      changed = true;
      continue;
    }
    kept.set(deltaIndex, text);
  }
  return changed ? kept : deltas;
}
