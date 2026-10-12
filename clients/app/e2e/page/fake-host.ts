/**
 * 页内假主机：在真实浏览器里扮演同步服务端，让**真实的** `SyncClient` 走完握手、
 * 订阅、快照与事件投递。
 *
 * ## 它为什么必须在浏览器里而不是在 Node 里
 *
 * 握手的两个签名都不是摆设：客户端要用**预置的已配对主机公钥**验 `hostProof`
 * （`src/composition.ts` 的 `PairedHostIdentity`），再用 IndexedDB 里那把
 * **不可导出**的设备私钥签 `auth.client_proof`。这两步只有真实 WebCrypto + 真实
 * IndexedDB 才跑得起来——WP5b 的 runner 已经在核对平台层的事实，本文件把它延伸到
 * 组合根与页面这一层。
 *
 * ## 判别力：假主机不是橡皮图章
 *
 * - transcript 的 domain 与字段 tag 由 `src/composition.ts:231-240` 逐字复制。**抄错就会验签失败**、
 *   连接被阻断、全部用例变红，因此这里不存在「怎么签都能过」的可能。
 * - `tamperHostProof` 开关故意签错一条，检查据此断言阻断态——证明校验真的在跑。
 * - 快照 digest 用**真实 SHA-256** 两级哈希（每 chunk 一次再对连接结果一次，§9.4），
 *   由 `src/sync-client/snapshot.ts:232-248` 独立校验。digest 算错 → `digest_mismatch` → 不提交。
 *
 * ## 假的是哪一层
 *
 * 只有传输：`SocketPort` 的 `send` / `close` 与服务端的消息时序。
 * 帧**原文**（JSON 文本）逐字记录——`R16` 要断言的「实际发出的字节」就是这些原文。
 */

import type { SocketHandlers, SocketPort } from "../../src/sync-client";
import { encodeBase64Url, sortedFeatures } from "../../src/sync-client";
import {
  encodeNulJoinedUtf8,
  encodeTranscript,
  encodeU16be,
  encodeUuid16,
  encodeUtf8,
} from "../../src/platform/transcript";

/** transcript domain（`src/composition.ts:92` 的 `DOMAIN.hostChallenge`）。 */
const HOST_CHALLENGE_DOMAIN = "acp-remote/host-challenge/v1";

/**
 * Host challenge transcript 的字段 tag（`src/composition.ts:71-86` 的 `TAG`）。
 *
 * 第 12 号 tag 是 features：`composition.ts:239` 把它写成**字面量 12**，
 * 因此这里同样是字面量而不是 `TAG.negotiatedFeatures`（那个键根本不存在）。
 */
const HOST_CHALLENGE_TAG = {
  protocolVersion: 1,
  hostId: 2,
  deviceId: 3,
  canonicalOrigin: 6,
  clientNonce: 9,
  serverNonce: 10,
  connectionId: 11,
  negotiatedFeatures: 12,
} as const;

/** 客户端看到的连接标识；固定一条，便于断言出站命令的 `connectionId`。 */
const CONNECTION_ID = "3c9f1b6e-6a41-4a2e-9a1c-2f0a5b7d9e13";

/**
 * 服务端在 `auth.server_challenge` 里**选中**的 feature。
 *
 * `core.event-ack.v1` 是既有取值（ACK 面的用例依赖它）；`core.session-history.v1` 是本次
 * 变更新登记的非必需 feature，**必须**在这里选中才有意义：客户端只在服务端选中它时
 * 才给 `session.read` 带上 `history` include（`client-store.ts` 的 `#includesFor`）。
 * 只选 `core.event-ack.v1` 时每次读取都只取消息，结构化历史条目永远来不到页面，
 * 「重开页面仍见历史条目」因此**不可能**被断言——那是 feature 门控的直接后果，不是用例写法问题。
 *
 * 选中它是安全的：transcript 与报文里的 `selectedFeatures` 都取自本常量，
 * 客户端用**报文里**的那份重建 transcript 验签（`connection.ts` 的 `#onServerChallenge`）。
 */
const SERVER_SELECTED_FEATURES: readonly string[] = ["core.event-ack.v1", "core.session-history.v1"];

/** 非 `session.read` 命令的缺省应答结果（与本次变更前的取值逐字一致）。 */
const DEFAULT_COMMAND_RESULT: Record<string, unknown> = { sessionId: "dddddddd-0000-4000-8000-0000000000ff" };

/** 假主机给每条命令的「服务端接受时刻」（`command.result.acceptedAt`，与本次变更前逐字一致）。 */
const ACCEPTED_AT = "2026-10-01T00:00:01.000Z";

/** 主机快照的固定 epoch：与 `src/sync-client/testing/harness.ts` 的夹具同口径。 */
const SERVER_EPOCH = "00384a03-bc90-4095-b65d-82fb8cc47e13";

/** 一帧 wire 消息里本文件真正常读的字段。 */
interface CommandFrame {
  readonly requestId: string;
  readonly command: string;
  readonly sessionId: string;
}

/** 从一帧里取出 `body` 并按 `type` 过滤；形状不符返回 `null`。 */
function bodyOf(message: Readonly<Record<string, unknown>>, type: string): Record<string, unknown> | null {
  if (message["type"] !== type) return null;
  const body = message["body"];
  return typeof body === "object" && body !== null ? (body as Record<string, unknown>) : null;
}

/** 必填字符串字段；缺失即抛（假主机自己造的帧，不该缺字段）。 */
function requiredString(body: Readonly<Record<string, unknown>>, key: string): string {
  const value = body[key];
  if (typeof value !== "string") throw new Error(`wire 帧缺字符串字段 ${key}`);
  return value;
}

/** base64url 文本 → 字节；假主机只需要客户端 nonce 的原文字节。 */
function decodeBase64UrlLoose(text: string): Uint8Array<ArrayBuffer> {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded.padEnd(padded.length + ((4 - (padded.length % 4)) % 4), "="));
  const out = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) out[index] = binary.charCodeAt(index);
  return out as Uint8Array<ArrayBuffer>;
}

/**
 * 真正改坏一条签名：解码 → 翻转首字节的最高位 → 按 base64url 重新编码。
 *
 * ## 为什么不能只改**最后一个字符**（本轮修掉的偶发红根因）
 *
 * 64 字节的签名编码成 86 个 base64url 字符：86 × 6 = 516 位，而数据只有 512 位，
 * 于是**末字符只有高 2 位有效**，低 4 位是补零。把末字符在 `A` / `B`（索引 0 / 1，
 * 高 2 位都是 `00`）之间翻转时，若原末字符恰好是 `A`——编码器补零后这正是「末 2 位为 00」
 * 的唯一写法，概率 1/4——解码回来的字节序列**与原来逐字节相同**：
 * 客户端（`src/sync-client/base64url.ts:60` 的 `decodeBase64Url`，只取前 8 位、丢弃补位）
 * 拿到的是同一条合法签名，验签通过、连接照常走到 `online`，CR-3 因此报
 * 「连接未进入 identity_changed 阻断态」。实测 4/10 次运行如此。
 *
 * 翻转解码后的**首字节**则必然改变签名：置起 r 的最高位后 r ≥ 2^255 > 曲线阶 n，
 * 验签不可能通过（ECDSA 要求 1 ≤ r < n）。
 */
function flipSignature(signature: string): string {
  const bytes = decodeBase64UrlLoose(signature);
  bytes[0] = (bytes[0] ?? 0) ^ 0x80;
  return encodeBase64Url(bytes);
}

export interface FakeHostOptions {
  readonly canonicalOrigin: string;
  readonly workspaces: readonly unknown[];
  readonly sessions: readonly unknown[];
  readonly agents: readonly unknown[];
  /** `null` = 不下发 `auth.authenticated`：客户端停在未认证，`scopes` 恒为 `null`。 */
  readonly scopes: readonly string[] | null;
  /** 故意签一条错的 hostProof：用于「验签不是橡皮图章」的负向对照。 */
  readonly tamperHostProof: boolean;
  /** 是否下发快照与屏障（`false` 用于「已配对但从未同步过」）。 */
  readonly deliverSnapshot: boolean;
  /** 快照屏障的全局序号；后续事件的序号必须大于它。 */
  readonly barrierGlobalSequence: string;
  /** 快照就绪后按序下发的 `event` 消息（原文对象）。 */
  readonly events: readonly Record<string, unknown>[];
  /** `session.read` 的应答结果；缺省返回空页。 */
  readonly readResult: (sessionId: string) => Record<string, unknown>;
  /**
   * 要**扣住**不应答的命令名（用例据此观察「未收到终态」的呈现）。
   *
   * 缺省（未给或空数组）与既有行为完全一致：任何命令都立即回一条 `command.result`。
   * 指定后这些命令的帧只登记不回答，由用例调 {@link FakeHost.completeHeldCommand} 决定终态——
   * 「提交不等于完成」「取消以终态为准」这两条只有在能**不下发**终态时才谈得上被证伪
   * （立即回 `completed` 的假主机让所有提交一发出就变「已完成」，两条 MUST NOT 都不可观测）。
   */
  readonly holdCommands?: readonly string[] | undefined;
}

/** 已配对主机记录（写进本地缓存的那一份，只含公开材料）。 */
export interface HostKeyRecord {
  readonly hostId: string;
  readonly publicKeyBase64Url: string;
}

/** 主机 P-256 密钥对：私钥只留在页内，公开材料进「已配对主机记录」。 */
export interface FakeHostIdentity extends HostKeyRecord {
  readonly signHostChallenge: (transcript: Uint8Array) => Promise<string>;
}

/** 生成主机密钥对。这是**主机**的钥匙，与设备私钥无关（设备私钥由 IndexedDB 持有）。 */
export async function createFakeHostIdentity(hostId: string): Promise<FakeHostIdentity> {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  return {
    hostId,
    publicKeyBase64Url: encodeBase64Url(raw as Uint8Array<ArrayBuffer>),
    async signHostChallenge(transcript: Uint8Array) {
      const signature = await crypto.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        pair.privateKey,
        transcript as BufferSource,
      );
      return encodeBase64Url(new Uint8Array(signature) as Uint8Array<ArrayBuffer>);
    },
  };
}

/** 真实 SHA-256（快照两级摘要的第一级）。 */
async function sha256(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes as BufferSource)) as Uint8Array<ArrayBuffer>;
}

/** 把若干字节块拼成一段（第二级摘要的输入）。 */
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

/**
 * 假主机。
 *
 * 一条连接对应一组 `handlers`；重连会新建 socket 并覆盖它——本目录的用例全部只连一次
 * （断连由 `drop()` 触发），因此不处理并存。
 */
export class FakeHost {
  readonly options: FakeHostOptions;
  readonly identity: FakeHostIdentity;
  /** 客户端发出的每一帧**原文**（`R16` 的字节级断言就查这里）。 */
  readonly sent: string[] = [];
  /** 客户端发出的每条消息解析后的形状，便于按 `type` 检索。 */
  readonly received: Readonly<Record<string, unknown>>[] = [];
  readonly socketUrls: string[] = [];
  readonly deviceProofs: string[] = [];

  #handlers: SocketHandlers | null = null;
  #closed = false;
  /** 被扣住、尚未应答的命令帧（按发出次序；见 {@link FakeHostOptions.holdCommands}）。 */
  #resultCounter = 0;
  #held: CommandFrame[] = [];

  constructor(options: FakeHostOptions, identity: FakeHostIdentity) {
    this.options = options;
    this.identity = identity;
  }

  /** 注入给 `createComposition({ openSocket })` 的连接工厂。 */
  readonly openSocket = (input: { readonly url: string; readonly handlers: SocketHandlers }): SocketPort => {
    this.socketUrls.push(input.url);
    this.#handlers = input.handlers;
    this.#closed = false;
    // 真实 socket 的 `open` 事件是异步的；同步调用会让客户端在构造完成前就收到帧。
    queueMicrotask(() => {
      if (!this.#closed) input.handlers.onOpen();
    });
    return {
      send: (text: string): void => {
        this.sent.push(text);
        this.#onClientFrame(text);
      },
      close: (code: number, reason: string): void => {
        this.drop(code, reason);
      },
    };
  };

  /** 模拟服务端断开：浏览器侧看到的效果与 socket 被动关闭一致。 */
  drop(code = 1006, reason = "connection reset by peer"): void {
    if (this.#handlers === null || this.#closed) return;
    this.#closed = true;
    this.#handlers.onClose(code, reason);
  }

  /** 客户端某一帧的原文（按消息类型取第一条）；没有则 `null`。 */
  frameOfType(type: string): string | null {
    for (const text of this.sent) {
      if (this.#typeOf(text) === type) return text;
    }
    return null;
  }

  #typeOf(text: string): string | null {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null) return null;
    const type = (parsed as { type?: unknown }).type;
    return typeof type === "string" ? type : null;
  }

  /** 向客户端投递一条消息原文。 */
  #deliver(text: string): void {
    if (this.#handlers === null || this.#closed) return;
    const handlers = this.#handlers;
    queueMicrotask(() => {
      if (!this.#closed) handlers.onMessage(text);
    });
  }

  #onClientFrame(text: string): void {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null) return;
    const message = parsed as Record<string, unknown>;
    this.received.push(message);

    const hello = bodyOf(message, "auth.client_hello");
    if (hello !== null) {
      void this.#onClientHello({
        deviceId: requiredString(hello, "deviceId"),
        clientNonce: requiredString(hello, "clientNonce"),
      });
      return;
    }

    const proof = bodyOf(message, "auth.client_proof");
    if (proof !== null) {
      this.deviceProofs.push(requiredString(proof, "deviceProof"));
      this.#sendAuthenticated();
      return;
    }

    if (bodyOf(message, "sync.subscribe") !== null) {
      this.#sendSnapshot();
      return;
    }

    const command = bodyOf(message, "command");
    if (command !== null) {
      const frame: CommandFrame = {
        requestId: requiredString(command, "requestId"),
        command: requiredString(command, "command"),
        sessionId: typeof command["sessionId"] === "string" ? command["sessionId"] : "",
      };
      // 扣住用例点名的命令：只登记不应答，终态交给 `completeHeldCommand`。
      // `session.read` 永不扣——页面首帧就靠它，扣住只会让每条用例都超时，而不是断言到什么。
      if (frame.command !== "session.read" && this.options.holdCommands?.includes(frame.command) === true) {
        this.#held.push(frame);
        return;
      }
      this.#sendCommandResult(frame);
      return;
    }

    const ping = bodyOf(message, "control.ping");
    if (ping !== null) {
      this.#deliver(
        JSON.stringify({
          protocolVersion: 1,
          type: "control.pong",
          messageId: "00000000-0000-4000-8000-00000000f00d",
          connectionId: CONNECTION_ID,
          connectionSequence: "1",
          body: { nonce: requiredString(ping, "nonce") },
        }),
      );
    }
  }

  async #onClientHello(hello: { readonly deviceId: string; readonly clientNonce: string }): Promise<void> {
    const serverNonce = crypto.getRandomValues(new Uint8Array(32)) as Uint8Array<ArrayBuffer>;
    const selectedFeatures = SERVER_SELECTED_FEATURES;
    const transcript = encodeTranscript(HOST_CHALLENGE_DOMAIN, [
      { tag: HOST_CHALLENGE_TAG.protocolVersion, bytes: encodeU16be(1) },
      { tag: HOST_CHALLENGE_TAG.hostId, bytes: encodeUuid16(this.identity.hostId) },
      { tag: HOST_CHALLENGE_TAG.deviceId, bytes: encodeUuid16(hello.deviceId) },
      { tag: HOST_CHALLENGE_TAG.canonicalOrigin, bytes: encodeUtf8(this.options.canonicalOrigin) },
      { tag: HOST_CHALLENGE_TAG.clientNonce, bytes: decodeBase64UrlLoose(hello.clientNonce) },
      { tag: HOST_CHALLENGE_TAG.serverNonce, bytes: serverNonce },
      { tag: HOST_CHALLENGE_TAG.connectionId, bytes: encodeUuid16(CONNECTION_ID) },
      {
        tag: HOST_CHALLENGE_TAG.negotiatedFeatures,
        bytes: encodeNulJoinedUtf8(sortedFeatures(selectedFeatures)),
      },
    ]);
    const hostProof = await this.identity.signHostChallenge(transcript as Uint8Array<ArrayBuffer>);

    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "auth.server_challenge",
        messageId: "00000000-0000-4000-8000-0000000000c1",
        body: {
          selectedProtocolVersion: 1,
          hostId: this.identity.hostId,
          connectionId: CONNECTION_ID,
          serverNonce: encodeBase64Url(serverNonce),
          selectedFeatures,
          hostProof: this.options.tamperHostProof ? flipSignature(hostProof) : hostProof,
        },
      }),
    );
  }

  #sendAuthenticated(): void {
    if (this.options.scopes === null) return;
    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "auth.authenticated",
        messageId: "00000000-0000-4000-8000-0000000000a1",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          deviceId: this.#deviceIdFromHello(),
          scopes: this.options.scopes,
          serverEpoch: SERVER_EPOCH,
          headGlobalSequence: this.options.barrierGlobalSequence,
          heartbeatIntervalMs: 30_000,
          limits: {
            maxMessageBytes: 1_048_576,
            maxPromptBytes: 262_144,
            maxReplayEventsPerBatch: 500,
          },
        },
      }),
    );
  }

  #deviceIdFromHello(): string {
    for (const message of this.received) {
      const hello = bodyOf(message, "auth.client_hello");
      if (hello !== null) return requiredString(hello, "deviceId");
    }
    throw new Error("尚未收到 auth.client_hello");
  }

  #sendSnapshot(): void {
    if (!this.options.deliverSnapshot) return;
    const snapshotId = "8194de43-e213-423d-acf4-2e3549304566";
    const cursor = { serverEpoch: SERVER_EPOCH, globalSequence: this.options.barrierGlobalSequence };
    const chunks = [
      { resource: "sessions", items: this.options.sessions },
      { resource: "workspaces", items: this.options.workspaces },
      { resource: "agents", items: this.options.agents },
    ].map((chunk, index) =>
      JSON.stringify({
        protocolVersion: 1,
        type: "sync.snapshot_chunk",
        messageId: `aaaaaaaa-1111-4111-8111-00000000000${index}`,
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          snapshotId,
          chunkIndex: String(index),
          resource: chunk.resource,
          items: chunk.items,
        },
      }),
    );

    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "sync.snapshot_begin",
        messageId: "aaaaaaaa-1111-4111-8111-0000000000b1",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { snapshotId, cursor, schemaVersion: 1, chunkCount: chunks.length },
      }),
    );
    for (const chunk of chunks) this.#deliver(chunk);
    void this.#sendSnapshotEnd(snapshotId, cursor, chunks);
  }

  async #sendSnapshotEnd(
    snapshotId: string,
    cursor: { readonly serverEpoch: string; readonly globalSequence: string },
    chunks: readonly string[],
  ): Promise<void> {
    const encoder = new TextEncoder();
    const perChunk: Uint8Array<ArrayBuffer>[] = [];
    for (const chunk of chunks) perChunk.push(await sha256(encoder.encode(chunk)));
    const digest = encodeBase64Url(await sha256(concatBytes(perChunk)));

    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "sync.snapshot_end",
        messageId: "aaaaaaaa-1111-4111-8111-0000000000e1",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { snapshotId, cursor, chunkCount: chunks.length, snapshotDigest: digest },
      }),
    );

    for (const event of this.options.events) {
      this.#deliver(JSON.stringify({ ...event, connectionId: CONNECTION_ID, connectionSequence: "1" }));
    }

    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "sync.caught_up",
        messageId: "aaaaaaaa-1111-4111-8111-0000000000c2",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { cursor },
      }),
    );
  }

  #sendCommandResult(command: CommandFrame): void {
    this.#deliverResult(command, {
      status: "completed",
      result: command.command === "session.read" ? this.options.readResult(command.sessionId) : DEFAULT_COMMAND_RESULT,
      acceptedAt: ACCEPTED_AT,
      error: null,
    });
  }

  /**
   * 对最早一条被扣住的 `command` 下发 `command.result`（终态由用例决定）。
   *
   * 返回 `false` 表示没有该命令的待答帧：那时用例的断言应当已经失败，不是静默通过。
   * 同一条命令可以先回 `accepted` 再回 `completed`——`accepted` 不是终态，
   * 「未确认接受前不得显示已接受」因此可以用**同一 requestId** 的前后两步来断言。
   */
  completeHeldCommand(
    command: string,
    outcome: {
      readonly status?: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
      readonly result?: Record<string, unknown>;
      readonly error?: Record<string, unknown> | null;
    } = {},
  ): boolean {
    const index = this.#held.findIndex((frame) => frame.command === command);
    if (index < 0) return false;
    const frame = this.#held[index];
    if (frame === undefined) return false;
    // `accepted` **不是**终态：同一 requestId 还会再收到回落。此时保留被扣的帧，
    // 否则第二次 `completeHeldCommand` 找不到它、终态永远回不来（CP-4 的观测窗口就是这么塌的）。
    const terminal = outcome.status !== undefined && outcome.status !== "accepted";
    if (terminal) this.#held.splice(index, 1);
    const status = outcome.status ?? "completed";
    this.#deliverResult(frame, {
      status,
      result: outcome.result ?? DEFAULT_COMMAND_RESULT,
      // `wire.ts` 要求非 `rejected` 的 `command.result` 必带 `acceptedAt` 时间戳；
      // `rejected` 必须为 `null` 且带结构化 error。假主机不在这里制造协议违规。
      acceptedAt: status === "rejected" ? null : ACCEPTED_AT,
      error:
        status === "rejected"
          ? (outcome.error ?? { code: "capability.unsupported_by_agent", message: "假主机拒绝", retryable: false, details: {} })
          : (outcome.error ?? null),
    });
    return true;
  }

  /** 投递一条 `command.result`（终态字段由调用方给出）。 */
  #deliverResult(
    command: CommandFrame,
    fields: {
      readonly status: string;
      readonly result: Record<string, unknown>;
      readonly acceptedAt: string | null;
      readonly error: Record<string, unknown> | null;
    },
  ): void {
    // 每条 `command.result` 是**独立**的一帧：`messageId` 必须唯一。
    // 同一命令先回 `accepted` 再回 `completed` 时若复用同一个 `messageId`，
    // 第二帧会被当作重复信封丢掉，「只有终态才算完成」就无从断言。
    this.#resultCounter += 1;
    this.#deliver(
      JSON.stringify({
        protocolVersion: 1,
        type: "command.result",
        messageId: `bbbbbbbb-1111-4111-8111-${this.#resultCounter.toString().padStart(12, "0")}`,
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: {
          requestId: command.requestId,
          command: command.command,
          status: fields.status,
          acceptedAt: fields.acceptedAt,
          terminalEventId: null,
          result: fields.result,
          error: fields.error,
        },
      }),
    );
  }
}