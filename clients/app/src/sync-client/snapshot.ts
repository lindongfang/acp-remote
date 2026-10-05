/**
 * 快照暂存与**原子替换**（R14「事件按稳定标识去重且不跳过」+ D1）。
 *
 * ## 核心不变量
 *
 * 1. **未完成的暂存内容永不进入已完成状态。** 只有 chunk 连续、数量吻合、cursor 一致且
 *    digest 验证全部通过，`complete()` 才返回可提交的结果；否则调用方保持上一次已完成状态
 *    （spec 场景「重建快照时不破坏已完成状态」）。
 * 2. **`sync.reset_required` 丢弃暂存区。** 收到新的 `snapshot_begin` 或 reset 时，未完成的
 *    暂存区整体丢弃并重新请求（§9.4：v1 不支持 chunk 断点续传）。
 * 3. **digest 只覆盖已到达的 chunk 的原始字节。** 客户端按「每 chunk 的 UTF-8 字节各做一次
 *    SHA-256，按 index 连接后再做一次」计算（§9.4），因此必须拿到 socket 交付的**原始文本**，
 *    不能重新序列化 JSON。
 *
 * ## D1 的体现
 *
 * 快照只承载 `sessions`/`workspaces`/`agents` 三类清单资源。明细资源在 chunk 解码处就被拒绝，
 * 因此本文件的 `StagedSnapshot` 类型里根本没有正文字段——「正文不入快照」是类型层面的事实，
 * 而不是运行期的一条 `if`。
 */

import type {
  AgentCatalogEntry,
  Cursor,
  SessionSummary,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  Uuid,
  WorkspaceRef,
} from "../protocol";
import { bytesEqual, decodeBase64Url } from "./base64url";
import type { DigestPort } from "./ports";

/** 暂存区里一份已验证的清单资源。 */
export interface StagedResources {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces: readonly WorkspaceRef[];
  readonly agents: readonly AgentCatalogEntry[];
}

/** 空资源集（`sessions` chunk 恒发但可以是空数组，因此初值必须存在）。 */
function emptyResources(): StagedResources {
  return { sessions: [], workspaces: [], agents: [] };
}

/** 一次快照的暂存状态。 */
export interface StagedSnapshot {
  readonly snapshotId: Uuid;
  readonly cursor: Cursor;
  readonly chunkCount: 1 | 2 | 3;
  readonly resources: StagedResources;
  /** 已到达的 chunk 数（必须最终等于 `chunkCount`）。 */
  readonly receivedChunks: number;
}

/** 验证通过、可提交替换的快照。 */
export interface VerifiedSnapshot extends StagedSnapshot {
  readonly snapshotDigest: string;
}

/** 丢弃暂存区的结构化原因（测试与诊断据此断言「为什么没提交」）。 */
export type SnapshotDiscardReason =
  /** 服务端要求重建（`sync.reset_required`）。 */
  | "reset_required"
  /** 收到另一个 `snapshot_begin`（§9.4 要求丢弃旧暂存区）。 */
  | "superseded_by_new_snapshot"
  /** chunk 序号不连续。 */
  | "chunk_out_of_order"
  /** chunk 数与 `chunkCount` 不符。 */
  | "chunk_count_mismatch"
  /** begin/end 的 cursor 不一致。 */
  | "cursor_mismatch"
  /** digest 不匹配。 */
  | "digest_mismatch"
  /** chunk 的 `snapshotId` 与 begin 不一致。 */
  | "snapshot_id_mismatch"
  /** 连接断开：未完成快照按 §9.4 丢弃。 */
  | "connection_closed"
  /** 进入阻断态（撤销/不兼容/身份变化/被顶替）：不得留下任何未完成内容。 */
  | "connection_blocked";

/** 快照验证失败。 */
export class SnapshotValidationError extends Error {
  readonly reason: SnapshotDiscardReason;

  constructor(reason: SnapshotDiscardReason, message: string) {
    super(message);
    this.name = "SnapshotValidationError";
    this.reason = reason;
  }
}

/**
 * 快照暂存器：一次只持有一个未完成的暂存区。
 *
 * 不持有「已完成状态」——替换由调用方在拿到 {@link VerifiedSnapshot} 后**原子**完成，
 * 这样「验证未通过时保持旧状态」是结构性的（没有中间可写的槽位），而不是靠调用顺序保证。
 */
export class SnapshotStaging {
  #staged: StagedSnapshot | null = null;
  /**
   * 每个已接受 chunk 的摘要计算，按 `chunkIndex` **占位**存放。
   *
   * 占位而不是「算完再追加」是必须的：服务端在同一条 WSS 上背靠背下发 chunk，
   * 摘要的 `await` 完成顺序与到达顺序无关；按完成顺序追加会把摘要链接错，
   * `complete()` 的两级哈希随之失配。
   */
  #chunkDigests: Promise<Uint8Array>[] = [];
  /** 最近一次丢弃的原因：`SnapshotDiscardReason` 存在的意义就是让「为什么没提交」可观测。 */
  #lastDiscardReason: SnapshotDiscardReason | null = null;
  readonly #digest: DigestPort;

  constructor(digest: DigestPort) {
    this.#digest = digest;
  }

  /** 当前暂存区；`null` 表示没有未完成的快照。 */
  get staged(): StagedSnapshot | null {
    return this.#staged;
  }

  /** 是否持有未完成的暂存区（`reset_required` 的处理要看它）。 */
  get hasPending(): boolean {
    return this.#staged !== null;
  }
  /** 最近一次丢弃的原因；从未丢弃过时为 `null`。 */
  get lastDiscardReason(): SnapshotDiscardReason | null {
    return this.#lastDiscardReason;
  }

  /** 新的 `snapshot_begin`：无条件丢弃旧的未完成暂存区后开始新的（§9.4）。 */
  begin(begin: SyncSnapshotBegin): StagedSnapshot {
    this.discard("superseded_by_new_snapshot");
    this.#staged = {
      snapshotId: begin.body.snapshotId,
      cursor: begin.body.cursor,
      chunkCount: begin.body.chunkCount,
      resources: emptyResources(),
      receivedChunks: 0,
    };
    this.#chunkDigests = [];
    return this.#staged;
  }

  /**
   * 收下一个 chunk。
   *
   * **连续性判定、序号推进与资源合并之间不得有 `await`**：服务端在同一条 WSS 上连发
   * chunk 0/1/2 时，本方法会在摘要的 `await` 处让出执行权，下一个 chunk 随即读到**尚未推进**的
   * `receivedChunks`，被判成 `chunk_out_of_order` 并把整份暂存区 `discard()` 掉——
   * 于是任何 `chunkCount >= 2` 的快照（正常情况）永远无法完成。
   * 因此这里把状态提交放在摘要计算的**发起**之前；摘要按 index 占位，完成顺序不影响摘要链。
   *
   * @param rawText 该 chunk 的**原始** wire 文本：digest 按原始 UTF-8 bytes 计算，
   *   重新序列化 JSON 会改变字节从而改变 digest（§9.4 明确禁止）。
   */
  async acceptChunk(rawText: string, chunk: SyncSnapshotChunk): Promise<StagedSnapshot> {
    const staged = this.#staged;
    if (staged === null) {
      throw new SnapshotValidationError("snapshot_id_mismatch", "收到 chunk 但没有进行中的快照");
    }
    if (chunk.body.snapshotId !== staged.snapshotId) {
      this.discard("snapshot_id_mismatch");
      throw new SnapshotValidationError("snapshot_id_mismatch", "chunk 的 snapshotId 与 begin 不一致");
    }
    // chunkIndex 是从 0 开始的十进制字符串，必须严格等于已到达数量（连续递增）。
    if (chunk.body.chunkIndex !== String(staged.receivedChunks)) {
      this.discard("chunk_out_of_order");
      throw new SnapshotValidationError("chunk_out_of_order", "chunkIndex 不连续");
    }
    if (staged.receivedChunks + 1 > staged.chunkCount) {
      this.discard("chunk_count_mismatch");
      throw new SnapshotValidationError("chunk_count_mismatch", "chunk 数超过 chunkCount");
    }

    // 同步段：校验 → 推进序号 → 合并资源，中间没有任何 `await`。
    const index = staged.receivedChunks;
    this.#staged = {
      ...staged,
      receivedChunks: index + 1,
      resources: mergeResources(staged.resources, chunk),
    };
    // 摘要只发起、不等待：校验失败路径一律 discard，序号与资源不会留下半份状态。
    this.#chunkDigests.push(this.#digest.sha256(new TextEncoder().encode(rawText)));
    return this.#staged;
  }

  /**
   * 验证并产出可提交的快照。
   *
   * @throws {SnapshotValidationError} 任一校验不通过；此时暂存区已被丢弃。
   */
  async complete(end: SyncSnapshotEnd): Promise<VerifiedSnapshot> {
    const staged = this.#staged;
    if (staged === null) {
      throw new SnapshotValidationError("snapshot_id_mismatch", "没有进行中的快照");
    }
    if (end.body.snapshotId !== staged.snapshotId) {
      this.discard("snapshot_id_mismatch");
      throw new SnapshotValidationError("snapshot_id_mismatch", "end 的 snapshotId 与 begin 不一致");
    }
    if (end.body.chunkCount !== staged.chunkCount) {
      this.discard("chunk_count_mismatch");
      throw new SnapshotValidationError("chunk_count_mismatch", "end 的 chunkCount 与 begin 不一致");
    }
    if (staged.receivedChunks !== staged.chunkCount) {
      this.discard("chunk_count_mismatch");
      throw new SnapshotValidationError("chunk_count_mismatch", "chunk 数不足");
    }
    if (end.body.cursor.serverEpoch !== staged.cursor.serverEpoch || end.body.cursor.globalSequence !== staged.cursor.globalSequence) {
      this.discard("cursor_mismatch");
      throw new SnapshotValidationError("cursor_mismatch", "end 的 cursor 与 begin 不一致");
    }

    // 两级哈希：每 chunk 一次，再对连接结果做一次（§9.4）。按 index 等待各 chunk 的摘要。
    const joined = concatBytes(await Promise.all(this.#chunkDigests));
    const computed = await this.#digest.sha256(joined);
    const expected = decodeDigestText(end.body.snapshotDigest);
    if (!bytesEqual(computed, expected)) {
      this.discard("digest_mismatch");
      throw new SnapshotValidationError("digest_mismatch", "snapshotDigest 校验失败");
    }

    const verified: VerifiedSnapshot = { ...staged, snapshotDigest: end.body.snapshotDigest };
    this.#staged = null;
    this.#chunkDigests = [];
    this.#lastDiscardReason = null;
    return verified;
  }

  /** 丢弃未完成的暂存区（`sync.reset_required`、连接断开或进入阻断态）。 */
  discard(reason: SnapshotDiscardReason): void {
    this.#staged = null;
    this.#chunkDigests = [];
    // 原因必须留下来：调用方与测试据此断言「为什么这次快照没提交」。
    this.#lastDiscardReason = reason;
  }
}

/** 合并一个 chunk 进资源集（D1：只有三类清单资源）。 */
function mergeResources(current: StagedResources, chunk: SyncSnapshotChunk): StagedResources {
  const body = chunk.body;
  if (body.resource === "sessions") {
    return { ...current, sessions: [...current.sessions, ...body.items] };
  }
  if (body.resource === "workspaces") {
    return { ...current, workspaces: [...current.workspaces, ...body.items] };
  }
  return { ...current, agents: [...current.agents, ...body.items] };
}

/** 拼接若干字节块。 */
function concatBytes(parts: readonly Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

/** 把 base64url 文本解成 32 字节；非 32 字节直接判 digest 不匹配。 */
function decodeDigestText(text: string): Uint8Array {
  try {
    return decodeBase64Url(text, 32);
  } catch {
    // 长度或字母表非法：返回空数组，`bytesEqual` 必然判否——digest 不匹配。
    return new Uint8Array(0);
  }
}
