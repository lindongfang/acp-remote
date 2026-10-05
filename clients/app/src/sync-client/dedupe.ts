/**
 * 事件去重与连续性账本（R14）。
 *
 * ## 去重键为什么是 `eventId` 而不是别的
 *
 * spec 与 `docs/SYNC_PROTOCOL.md` §9.4/§9.5 都要求按**稳定 `eventId`** 去重：§8.5 的
 * 连接切换窗口里同一条事件会被重复投递，而 `globalSequence` 在权限过滤下可以有不可见的
 * 空洞（「全局 sequence 对单个设备可以有不可见的空洞，不能据此推断隐藏事件」），
 * 因此**不能**用序号连续性判断「是否见过」。`messageId` 是 WSS 传输层信封标识，
 * 重连后服务端会重新分配，同样不能用。
 *
 * ## 连续性判定为什么仍要看序号
 *
 * 去重解决「重复不重复呈现」，但 spec 还要求「MUST NOT 出现丢失区间」。
 * 因此本模块同时做两件事：
 *
 * - `eventId` 命中 → 判为**重复**，丢弃且不产生任何副作用（不推进游标、不触发呈现）。
 * - `eventId` 未命中但序号**落后于**已确认游标 → 同样是重复投递的另一种表现，丢弃。
 * - `eventId` 未命中且序号超前于 `lastSequence + 1` → 判为**缺口**，要求从最后确认位置
 *   重新订阅（连续恢复），而不是继续推进——静默跳过会让丢失区间永久不可见。
 */

import type { Cursor, EventMessage, Uuid } from "../protocol";

/** 一次事件投递的判定结果。 */
export type EventVerdict =
  /** 首次见到：应呈现并推进游标。 */
  | { readonly kind: "new" }
  /** 已处理过：必须丢弃且不产生任何副作用（spec 场景「重复事件不重复呈现」）。 */
  | { readonly kind: "duplicate" }
  /** 序号出现缺口：需要重新订阅，不得继续推进。 */
  | { readonly kind: "gap"; readonly expected: string; readonly received: string };

/**
 * 事件去重账本。
 *
 * `seenEventIds` 是**有界**的：只保留游标窗口内的 ID，否则一个长连客户端会无限增长。
 * 淘汰按 `globalSequence` 从旧到新，因此绑定必须与游标一致。
 */
export class EventLedger {
  readonly #epoch: Uuid;
  readonly #seen = new Set<Uuid>();
  /** 已连续处理的最高 `globalSequence`；`null` 表示还没有处理过任何事件。 */
  #lastSequence: string | null;
  /** 允许缓存的去重键数量上限（`bound` 的取值依据）。 */
  readonly #retention: number;

  /**
   * @param resumeFrom 客户端最后一次确认的游标。其序号之前的事件**已经处理过**，
   *   因此账本从它起步而不是从 0——否则重连后的第一条事件会被判成缺口。
   */
  constructor(options: {
    readonly serverEpoch: Uuid;
    readonly resumeFrom?: Cursor | null;
    readonly retention?: number;
  }) {
    this.#epoch = options.serverEpoch;
    // 默认窗口取 replay 批量上限的若干倍：足以覆盖重连切换窗口，又不至于让
    // Set 随连接时长线性增长。
    this.#retention = options.retention ?? 2000;
    this.#lastSequence = options.resumeFrom?.globalSequence ?? null;
  }

  /** 本账本绑定的 `serverEpoch`（跨连接复用账本时必须一致）。 */
  get serverEpoch(): Uuid {
    return this.#epoch;
  }

  /** 已确认的游标；从未处理过事件时为 `null`（首次订阅发 `cursor: null`）。 */
  get cursor(): Cursor | null {
    if (this.#lastSequence === null) return null;
    return { serverEpoch: this.#epoch, globalSequence: this.#lastSequence };
  }

  /** 判定一条事件是否应当被处理。 */
  evaluate(event: EventMessage): EventVerdict {
    if (this.#seen.has(event.body.eventId)) {
      return { kind: "duplicate" };
    }
    const sequence = event.body.globalSequence;
    if (this.#lastSequence === null) {
      // 首次事件必须正好是订阅游标的下一条；订阅游标为 null 时必须是 1。
      const expected = BigInt(1);
      if (BigInt(sequence) !== expected) {
        return { kind: "gap", expected: expected.toString(), received: sequence };
      }
      return { kind: "new" };
    }
    const last = BigInt(this.#lastSequence);
    const received = BigInt(sequence);
    if (received <= last) {
      // 序号已处理过但 ID 不在窗口内（例如被淘汰后又重投）：语义上仍是重复。
      return { kind: "duplicate" };
    }
    const expected = last + 1n;
    if (received !== expected) {
      return { kind: "gap", expected: expected.toString(), received: sequence };
    }
    return { kind: "new" };
  }

  /** 确认一条 `new` 事件已处理：记录 ID 并推进游标。 */
  commit(event: EventMessage): Cursor {
    if (this.#lastSequence === null || BigInt(event.body.globalSequence) > BigInt(this.#lastSequence)) {
      this.#lastSequence = event.body.globalSequence;
    }
    this.#seen.add(event.body.eventId);
    this.#evictIfNeeded();
    return { serverEpoch: this.#epoch, globalSequence: event.body.globalSequence };
  }

  /** 当前已记住的去重键数量（诊断与测试断言用）。 */
  get seenCount(): number {
    return this.#seen.size;
  }

  /**
   * 换连接时复用账本：游标保留（§9.2：新连接恢复位置以客户端本地 cursor 为准），
   * 去重键也保留——§8.5 的切换窗口里重复投递必须被识别为重复。
   */
  carryOverToNextConnection(): void {
    // 账本本身与连接无关，无需重建；此方法存在的意义是把「重连不重置账本」
    // 变成一个显式、可被测试覆盖的契约而不是隐含假设。
  }

  /** 超出保留窗口时按插入顺序（FIFO = 序号升序）淘汰最旧的键。 */
  #evictIfNeeded(): void {
    while (this.#seen.size > this.#retention) {
      const oldest = this.#seen.values().next();
      if (oldest.done === true) return;
      this.#seen.delete(oldest.value);
    }
  }
}

/**
 * `eventId` 稳定性的判定：导入事件跨跳 `eventId` 必须等于 `remoteOrigin.originEventId`（§9.6）。
 *
 * 客户端据此可以把 Access 与 Owner 上报的同一条事件去重，因此本函数在 ingest 时校验，
 * 不一致立即拒绝——继续处理会让「同一事件被呈现两次」在跨节点场景复发。
 */
export function isImportedEventIdConsistent(event: EventMessage): boolean {
  const remote = event.body.remoteOrigin;
  if (remote === null) return true;
  return remote.originEventId === event.body.eventId;
}