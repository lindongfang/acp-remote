/**
 * 事件去重与**权威水位**账本（R14）。
 *
 * ## 去重键为什么是 `eventId` 而不是别的
 *
 * spec 与 `docs/SYNC_PROTOCOL.md` §9.4/§9.5 都要求按**稳定 `eventId`** 去重：§8.5 的
 * 连接切换窗口里同一条事件会被重复投递，而 `messageId` 是 WSS 传输层信封标识，重连后服务端
 * 会重新分配，两者都不能用。
 *
 * ## 为什么**不再**用序号连续性判断丢失（本轮纠正的核心假设）
 *
 * §9.2 明文：
 *
 * > v1 只定义 `scope: machine`，服务端仍按当前设备 scopes 过滤。**全局 sequence 对单个设备
 * > 可以有不可见的空洞，不能据此推断隐藏事件。**
 *
 * 因此对 scope 过滤后的 feed，「序号不连续」是**协议允许的常态**：设备看不到 seq 2/3/4，
 * 下一条可见事件就是 seq 5。任何 `received !== last + 1` ⇒「丢失」的判定都会把这三段空洞
 * 误判成丢失，进而丢弃本该呈现的事件并触发一轮无意义的重新订阅——这正是前三轮反复翻车的
 * 同一片区域。
 *
 * ## 水位只有一个来源：服务端屏障
 *
 * §9.3 规定增量重放的第 4 步是「发送 `sync.caught_up`，之后继续实时事件」，§9.4 的
 * `sync.snapshot_end.cursor` 是同一语义的快照版本。它们的含义是**服务端对本设备作出的声明**：
 * 「这条序号之前**对本设备可见**的事件都已交付」（§9.5 还明确「即使某段 global sequence
 * 全部因权限过滤而不可见，客户端也可以 ACK `sync.caught_up.cursor`」）。
 *
 * 因此水位（`#watermark`）只由两个来源抬高：
 *
 * 1. 服务端屏障（`advanceTo`，§9.3/§9.4）；
 * 2. 客户端**确实呈现过**的事件序号（`commit`）。
 *
 * 客户端**不靠数序号**推进水位：数序号等于重新引入上面那条错误假设。
 *
 * ## 真正的丢失如何仍然可被检出
 *
 * 屏障之下未被投递的空洞**在协议层不可判定**——服务端已经声明交付，没送就是不可见，
 * 客户端无权也无力区分「不可见」与「漏投」。能判定的只有一种：**服务端已经声明过的水位之下，
 * 又送来一条客户端从未呈现过的事件**。这必然意味着某一侧出了问题：
 *
 * - 服务端违反了自己的 barrier 声明（先声明交付、后补投），或
 * - 客户端此前把这条事件误丢了。
 *
 * 两种情况下这条事件都**必须补齐呈现**（R14「MUST NOT 出现丢失区间」优先于「不重复呈现」：
 * 重复呈现是可见且可去重的，丢失是静默且不可逆的），同时从最后确认位置**有界**地重新订阅，
 * 看服务端是否还压着别的未交付事件。判定为 {@link EventVerdict} 的 `late`。
 *
 * ## 有界记忆带来的唯一取舍
 *
 * 去重键窗口是有限的（默认 2000 条，淘汰最旧）。淘汰掉的条目之上，「重复」与「丢失」在信息上
 * 已不可区分——这就是 `#accountedFloor` 的职责：它记录「这一段及以下的序号都已处理过、且记忆
 * 已释放」，只增不减（初值是续接游标，每次淘汰抬到被淘汰条目的序号）。落在这段里的未见事件按
 * 重复丢弃，落在它之上、水位之下的判 `late` 并补齐。
 *
 * 取舍是明确的一边倒：§9.3 的重放严格从游标**之后**开始、§8.5 的切换窗口重复投递紧贴当前水位，
 * 正常服务端不会重投已释放的那一段；而把一段看不见的事件按「丢失」补齐，会重新打开静默丢失
 * 之外的另一条错误路径（把重复当成丢失反复触发重订阅）。
 *
 * ## 为什么不存在「已投递但未呈现」的中间态
 *
 * 这是 P1-A（屏障吞掉已投递事件）能被结构性消除的原因，也是 `advanceTo` 无需再接收
 * 「已丢弃序号」参数的原因：
 *
 * - 每条非重复 verdict 都在**同一个同步调用栈**内被 `commit` 并呈现，没有跨消息的暂存态；
 * - 屏障、快照这类消息各自是一次独立的入站调用，不会在某条事件的呈现过程中插入。
 *
 * 因此 `advanceTo` 抬高的水位之下，不存在任何「服务端已投递、客户端尚未呈现」的事件。
 */

import type { Cursor, EventMessage, Uuid } from "../protocol";
import { isDecimalString } from "./wire";

/**
 * 序号上界（`Number.MAX_SAFE_INTEGER`，2^53-1）。
 *
 * 取值依据：序列号在本层只用于 `BigInt` 比较与诊断，任何超过 2^53 的「事件数」都不可能真实
 * 发生；而 §9.2 规定服务端对「sequence 大于当前 head」的 cursor 必须回 `sync.cursor_invalid`，
 * 合规服务端不会发出越界屏障。这个上界的作用是把**故障或恶意**服务端发来的荒谬屏障挡在
 * 水位之外——否则一次 `advanceTo("999999999999999999999")` 就会让此后每条真实事件都落到水位
 * 以下，事件流静默停摆且无法自愈。
 */
export const SEQUENCE_UPPER_BOUND = BigInt(Number.MAX_SAFE_INTEGER);

/** 序号不可用的原因（形状非法 / 越界）。 */
export type SequenceRejection = "malformed_sequence" | "sequence_out_of_range";

/** 一次事件投递的判定结果。 */
export type EventVerdict =
  /** 首次见到且高于水位：应呈现并抬高水位。 */
  | { readonly kind: "new" }
  /** 已处理过：必须丢弃且不产生任何副作用（spec 场景「重复事件不重复呈现」）。 */
  | { readonly kind: "duplicate" }
  /**
   * **低于权威水位却从未呈现过**：一条真实的丢失区间证据（服务端违反 barrier 声明，
   * 或客户端此前误丢）。必须补齐呈现，并从最后确认位置重新订阅去看是否还有别的未交付事件。
   *
   * `watermark` 是收到本条事件时的水位（即「本应已处理到的位置」），`received` 是迟到事件的序号。
   */
  | { readonly kind: "late"; readonly watermark: string; readonly received: string }
  /** 序号本身不可用（形状非法或越界）：不可判定，必须拒绝而不是猜。 */
  | { readonly kind: "rejected"; readonly reason: SequenceRejection };

/** 一次屏障采纳的结果。 */
export type BarrierVerdict =
  /** 屏障抬高了水位。 */
  | { readonly kind: "advanced" }
  /** 水位已经覆盖该屏障（重复或更旧的 `caught_up`）：合法，但水位不动。 */
  | { readonly kind: "already_covered" }
  /** 屏障不可用：跨 epoch 游标不可比（§9.4 的 epoch 已变更意味着事件库重建），或序号非法。 */
  | { readonly kind: "rejected"; readonly reason: "epoch_mismatch" | SequenceRejection };

/** 序号解析结果（`BigInt` 或拒绝原因）。 */
type ParsedSequence = { readonly ok: true; readonly value: bigint } | { readonly ok: false; readonly reason: SequenceRejection };

/**
 * 解析 `globalSequence`。
 *
 * 形状判定复用 `wire.ts` 的 `isDecimalString`（`$defs.decimalString`），账本与解码器因此对
 * 「什么算十进制序号」只有一份定义；`wire` 的 `decodeEvent` 已经会拒掉形状非法的 `event`，
 * 这里是对账本作为独立单元的第二道防线（游标侧的 `decodeCaughtUp` 只做形状检查，不查上界）。
 */
function parseSequence(raw: string): ParsedSequence {
  if (!isDecimalString(raw)) return { ok: false, reason: "malformed_sequence" };
  const value = BigInt(raw);
  if (value > SEQUENCE_UPPER_BOUND) return { ok: false, reason: "sequence_out_of_range" };
  return { ok: true, value };
}

/**
 * 事件去重账本。
 *
 * `#seen` 是**有界**的记忆窗口：键为已呈现事件的 `eventId`，值为它的 `globalSequence`
 * （记住序号是为了判断「重投的事件是不是早就被淘汰出窗口的旧事件」）。否则一个长连客户端
 * 会无限增长。
 */
export class EventLedger {
  readonly #epoch: Uuid;
  readonly #seen = new Map<Uuid, bigint>();
  /**
   * 权威水位：服务端屏障与已呈现事件序号的**最大值**。
   *
   * `null` 表示还没有任何屏障或事件；它**不是**「下一个必须是 1」的起点假设。
   */
  #watermark: bigint | null;
  /**
   * 「这一段及以下的序号都已处理过、且记忆已被释放」的界，只增不减。
   *
   * 初值是续接游标（其序号之前的事件在**上一次**会话就已处理）；每次淘汰记忆条目时抬到被淘汰
   * 条目的序号。它把「水位之下未见过」分成两段：界以上判 `late`（真实丢失，必须补齐），
   * 界以下按重复丢弃（记忆已释放，重复与丢失不可区分，而该区间的完整性由服务端屏障保证）。
   */
  #accountedFloor: bigint | null;
  /** 允许缓存的去重键数量上限（`bound` 的取值依据）。 */
  readonly #retention: number;

  /**
   * @param resumeFrom 客户端最后一次确认的游标。其序号之前对本设备可见的事件**已经处理过**，
   *   因此水位从它起步（重连后的第一条可见事件序号通常远大于它，§9.2 的合法空洞）。
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
    const resumeFrom = options.resumeFrom;
    this.#accountedFloor = null;
    if (resumeFrom === null || resumeFrom === undefined) {
      this.#watermark = null;
      return;
    }
    const parsed = parseSequence(resumeFrom.globalSequence);
    // 构造期输入来自客户端自己的持久游标，形状非法属于编程错误，必须响亮地失败：
    // 静默退化成「无游标」会让恢复点被无声丢弃。
    if (!parsed.ok) throw new Error(`resumeFrom 的 globalSequence 不可用：${resumeFrom.globalSequence}（${parsed.reason}）`);
    this.#watermark = parsed.value;
    this.#accountedFloor = parsed.value;
  }

  /** 本账本绑定的 `serverEpoch`（跨连接复用账本时必须一致）。 */
  get serverEpoch(): Uuid {
    return this.#epoch;
  }

  /** 权威水位；从未处理过任何事件时为 `null`（首次订阅发 `cursor: null`）。 */
  get cursor(): Cursor | null {
    const watermark = this.#watermark;
    if (watermark === null) return null;
    return { serverEpoch: this.#epoch, globalSequence: String(watermark) };
  }

  /**
   * 判定一条事件是否应当被处理。
   *
   * 注意这里**没有**「序号必须连续」的分支：scope 过滤让空洞成为常态（见文件头）。
   * 判定只有三个问题：见过没有？序号可用吗？落在水位的哪一侧？
   */
  evaluate(event: EventMessage): EventVerdict {
    if (this.#seen.has(event.body.eventId)) {
      return { kind: "duplicate" };
    }
    const parsed = parseSequence(event.body.globalSequence);
    if (!parsed.ok) {
      return { kind: "rejected", reason: parsed.reason };
    }
    const watermark = this.#watermark;
    if (watermark !== null && parsed.value <= watermark) {
      // 水位之下却没见过：服务端声明过「这条之前都已交付」，因此它要么是漏投、要么是客户端
      // 此前误丢。两种都必须补齐呈现，绝不能按重复丢弃——丢弃就是永久丢失（P1-A 的教训）。
      const floor = this.#accountedFloor;
      if (floor !== null && parsed.value <= floor) {
        // 已释放的记忆段：这条事件的呈现与否无从回忆，按重复丢弃。取舍是明确的——这一段
        // 的完整性由服务端屏障保证（§9.3 的 `caught_up` 覆盖整段），而 §9.3 的重放严格从游标
        // **之后**开始，正常服务端不会重投这一段。
        return { kind: "duplicate" };
      }
      return { kind: "late", watermark: String(watermark), received: event.body.globalSequence };
    }
    return { kind: "new" };
  }

  /**
   * 确认一条事件已处理：记录 ID 并抬高水位。
   *
   * 水位取「已呈现事件序号的最大值」而不是本事件的序号：`late` 事件本来就低于水位，
   * 抬高会把水位推到一个未呈现过的区间（§9.5 的 ACK 语义要求 ACK 是**最高已连续处理的
   * 可见事件 cursor**，未呈现的序号不满足「已处理」）。
   */
  commit(event: EventMessage): Cursor {
    const parsed = parseSequence(event.body.globalSequence);
    if (!parsed.ok) {
      throw new Error(`事件序号不可用：${event.body.globalSequence}（${parsed.reason}）`);
    }
    if (this.#watermark === null || parsed.value > this.#watermark) {
      this.#watermark = parsed.value;
    }
    this.#seen.set(event.body.eventId, parsed.value);
    this.#evictIfNeeded();
    return { serverEpoch: this.#epoch, globalSequence: String(this.#watermark) };
  }

  /**
   * 用**服务端给出的屏障游标**推进账本（`sync.caught_up.cursor`、已验证快照的 cursor）。
   *
   * 语义（§9.3 第 4 步、§9.4、§9.5）：这条序号之前**对本设备可见**的事件都已交付。
   * 于是屏障之下的空洞已被服务端消解，客户端**必须**采纳它作为水位——否则「已确认游标」
   * 与账本水位会分成两份会漂移的拷贝：重连时账本按旧值起步，紧接着的可见事件（屏障之后，
   * 中间有不可见段）会被误判，恢复通道原地打转。
   *
   * 屏障**不关闭**检测能力：它之上到达的事件仍按 {@link evaluate} 判定，而它之下到达的
   * 未呈现事件会被判成 `late` 并补齐（P1-A：屏障不得越过任何「已投递但未呈现」的事件；
   * 由于本模块不存在未呈现的中间态，见文件头，这条保证是结构性的）。
   *
   * `advanced` 与 `already_covered` 必须分开：只有 `advanced` 说明「屏障真的把水位抬高了」，
   * 缺口恢复预算据此清零——收到过 `caught_up` 本身并不说明任何事（§9.3 要求每次重订阅后都
   * 有一条 `caught_up`，按「收到过就清零」会让预算永远满格、退避上限不可达）。
   */
  advanceTo(cursor: Cursor): BarrierVerdict {
    if (cursor.serverEpoch !== this.#epoch) return { kind: "rejected", reason: "epoch_mismatch" };
    const parsed = parseSequence(cursor.globalSequence);
    if (!parsed.ok) return { kind: "rejected", reason: parsed.reason };
    const watermark = this.#watermark;
    if (watermark !== null && parsed.value <= watermark) return { kind: "already_covered" };
    this.#watermark = parsed.value;
    return { kind: "advanced" };
  }

  /** 当前已记住的去重键数量（诊断与测试断言用）。 */
  get seenCount(): number {
    return this.#seen.size;
  }

  /**
   * 换连接时复用账本：水位保留（§9.2：新连接恢复位置以客户端本地 cursor 为准），
   * 去重键也保留——§8.5 的切换窗口里重复投递必须被识别为重复。
   */
  carryOverToNextConnection(): void {
    // 账本本身与连接无关，无需重建；此方法存在的意义是把「重连不重置账本」
    // 变成一个显式、可被测试覆盖的契约而不是隐含假设。
  }

  /**
   * 超出保留窗口时按插入顺序（FIFO）淘汰最旧的键，并把「已处理界」抬到被淘汰条目的序号。
   *
   * 迟到事件的插入顺序可能打乱升序，因此抬界用的是「被淘汰条目本身的序号」而不是「剩余条目里
   * 最旧的那个」——后者会把从未交付的 scope 空洞一并划进已处理段，把真正的漏投判成重复。
   */
  #evictIfNeeded(): void {
    while (this.#seen.size > this.#retention) {
      const oldest = this.#seen.entries().next();
      if (oldest.done === true) return;
      const [eventId, sequence] = oldest.value;
      this.#seen.delete(eventId);
      if (this.#accountedFloor === null || sequence > this.#accountedFloor) {
        this.#accountedFloor = sequence;
      }
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