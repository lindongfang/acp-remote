/**
 * 测试替身：假 socket、假端口与消息构造器。
 *
 * 全部端口都是结构化的（见 `../ports.ts`），因此这里不需要任何平台实现就能驱动整条连接流程。
 * 断言的判别力来自「替身记录了什么」：出站消息逐条留痕，端口调用可回放。
 */

import type {
  AuthAuthenticated,
  CommandResultMessage,
  Cursor,
  EventMessage,
  RemoteOrigin,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  Uuid,
} from "../../protocol";
import { encodeBase64Url } from "../base64url";
import type { SyncClientEvent } from "../connection";
import { SyncConnection } from "../connection";
import { EventLedger } from "../dedupe";
import type {
  ByteArray,
  DeviceIdentityLike,
  HostChallengeInput,
  HostIdentityPort,
  RandomPort,
  SocketHandlers,
  SocketPort,
  TranscriptCodec,
  TranscriptField,
} from "../ports";

/** 一条出站消息与它被解析后的形状。 */
export interface SentMessage {
  readonly text: string;
  readonly parsed: Record<string, unknown>;
}

/** 记录所有出站文本、并可模拟入站消息的假 socket。 */
export class FakeSocket implements SocketPort {
  readonly sent: SentMessage[] = [];
  closedWith: { readonly code: number; readonly reason: string } | null = null;
  readonly #handlers: SocketHandlers;

  constructor(handlers: SocketHandlers) {
    this.#handlers = handlers;
  }

  send(text: string): void {
    this.sent.push({ text, parsed: JSON.parse(text) as Record<string, unknown> });
  }

  close(code: number, reason: string): void {
    this.closedWith = { code, reason };
  }

  /** socket 打开：触发被测代码的 `onOpen`。 */
  open(): void {
    this.#handlers.onOpen();
  }

  /** 模拟入站消息（对象会被序列化，与真实 socket 的文本交付一致）。 */
  deliver(message: unknown): void {
    this.#handlers.onMessage(typeof message === "string" ? message : JSON.stringify(message));
  }

  /** 模拟连接关闭。 */
  serverClose(code: number, reason = ""): void {
    this.#handlers.onClose(code, reason);
  }

  /** 出站消息的 `type` 序列。 */
  types(): string[] {
    return this.sent.map((message) => String(message.parsed["type"]));
  }

  /** 最后一条指定类型的出站消息。 */
  last(type: string): Record<string, unknown> | undefined {
    return [...this.sent].reverse().find((message) => message.parsed["type"] === type)?.parsed;
  }
}

/** 记录每次 `openSocket` 调用的工厂；每次返回一个独立假 socket。 */
export class FakeSocketFactory {
  readonly sockets: FakeSocket[] = [];
  readonly urls: string[] = [];

  readonly open = (input: { readonly url: string; readonly handlers: SocketHandlers }): SocketPort => {
    this.urls.push(input.url);
    const socket = new FakeSocket(input.handlers);
    this.sockets.push(socket);
    return socket;
  };

  /** 最近建立的连接。 */
  get latest(): FakeSocket {
    const socket = this.sockets[this.sockets.length - 1];
    if (socket === undefined) throw new Error("尚未建立任何连接");
    return socket;
  }
}

/** 确定性随机源：字节按计数器递增，UUID 按计数器生成合法 v4 形状。 */
export class FakeRandom implements RandomPort {
  #counter = 0;
  readonly byteCalls: number[] = [];

  bytes(length: number): ByteArray {
    this.byteCalls.push(length);
    this.#counter += 1;
    const out = new Uint8Array(length);
    for (let index = 0; index < length; index += 1) {
      out[index] = (this.#counter + index) & 0xff;
    }
    return out;
  }

  uuid(): Uuid {
    this.#counter += 1;
    const hex = this.#counter.toString(16).padStart(12, "0");
    return `00000000-0000-4000-8000-${hex}`;
  }
}

/**
 * 确定性摘要端口：FNV-1a 派生的 32 字节值，**不是** SHA-256。
 *
 * 为什么不用真实 SHA-256：快照 digest 的不变量是「两级哈希、按 index 连接、覆盖已到达 chunk」，
 * 而不是 SHA-256 本身（真实算法的对拍属于平台层与 TP3）。用确定性替身的好处是测试可以
 * **独立复算**期望值：若实现少做一级、换序或漏掉某个 chunk，独立复算就会得出不同结果。
 */
export function fakeDigest(): RecordingDigest {
  const calls: Uint8Array[] = [];
  return {
    calls,
    async sha256(data: Uint8Array): Promise<Uint8Array> {
      calls.push(data);
      return deterministicHash(data);
    },
  };
}

/** 与 `fakeDigest` 同算法的独立实现，供测试独立复算期望 digest。 */
export function deterministicHash(data: Uint8Array): Uint8Array {
  const out = new Uint8Array(32);
  let hash = 0x811c9dc5;
  for (const byte of data) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  for (let index = 0; index < 4; index += 1) {
    out[index] = (hash >>> (index * 8)) & 0xff;
  }
  return out;
}

/**
 * 独立复算快照 digest：每 chunk 一次摘要，按 index 连接后再摘要一次（§9.4）。
 *
 * @param rawTexts 按 chunkIndex 升序的**原始文本**。
 */
export function expectedSnapshotDigest(rawTexts: readonly string[]): string {
  const joined = new Uint8Array(rawTexts.length * 32);
  rawTexts.forEach((text, index) => {
    joined.set(deterministicHash(new TextEncoder().encode(text)), index * 32);
  });
  return encodeBase64Url(deterministicHash(joined));
}

/** 记录 transcript 编码调用的假 codec。 */
export class FakeTranscript implements TranscriptCodec {
  readonly encoded: { readonly domain: string; readonly fields: readonly TranscriptField[] }[] = [];
  readonly fieldValues: { readonly tag: number; readonly bytes: Uint8Array }[] = [];

  encode(domain: string, fields: readonly TranscriptField[]): ByteArray {
    this.encoded.push({ domain, fields });
    return new Uint8Array(fields.length + 1);
  }

  u16be(value: number): ByteArray {
    const bytes = new Uint8Array(2);
    bytes[0] = value >> 8;
    bytes[1] = value & 0xff;
    return this.#record(1, bytes);
  }

  uuid16(value: string): ByteArray {
    return this.#record(value === "" ? 0 : 1, new Uint8Array(16).fill(value.length & 0xff));
  }

  utf8(value: string): ByteArray {
    return this.#record(value.length, new TextEncoder().encode(value));
  }

  nulJoinedUtf8(parts: readonly string[]): ByteArray {
    return this.#record(parts.length, new TextEncoder().encode(parts.join("\u0000")));
  }

  #record(tag: number, bytes: Uint8Array<ArrayBuffer>): ByteArray {
    this.fieldValues.push({ tag, bytes });
    return bytes as ByteArray;
  }
}

/** 假设备身份：记录签过的 transcript。 */
export class FakeIdentity implements DeviceIdentityLike {
  readonly proofs: Uint8Array[] = [];

  constructor(
    private readonly material: {
      readonly deviceId: Uuid;
      readonly canonicalOrigin: string;
      readonly publicKeyBase64Url: string;
    } | null = {
      deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
      canonicalOrigin: "https://host.example:8443",
      publicKeyBase64Url: "AQID",
    },
  ) {}

  async loadIdentity() {
    return this.material;
  }

  async signDeviceProof(transcript: ByteArray): Promise<ByteArray> {
    this.proofs.push(transcript);
    return new Uint8Array(64).fill(this.proofs.length) as ByteArray;
  }
}

/** 假主机身份端口：记录验签输入，可强制返回校验失败。 */
export class FakeHostIdentity implements HostIdentityPort {
  readonly challenges: HostChallengeInput[] = [];
  verificationResult = true;
  paired: { readonly hostId: Uuid; readonly publicKeyBase64Url: string; readonly canonicalOrigin: string } | null = {
    hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
    publicKeyBase64Url: "BQYH",
    canonicalOrigin: "https://host.example:8443",
  };

  async loadPairedHost(): Promise<typeof this.paired> {
    return this.paired;
  }

  async verifyHostChallenge(input: HostChallengeInput): Promise<boolean> {
    this.challenges.push(input);
    return this.verificationResult;
  }
}

/** 统一的事件构造器。 */
export function makeEvent(input: {
  readonly globalSequence: string;
  readonly eventId: Uuid;
  readonly eventType?: string;
  readonly sessionId?: Uuid | null;
  readonly view?: Record<string, unknown>;
  readonly remoteOrigin?: RemoteOrigin | null;
}): EventMessage {
  const payloadView = input.view ?? {};
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${input.globalSequence.padStart(12, "0")}`,
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      globalSequence: input.globalSequence,
      sessionSequence: null,
      eventId: input.eventId,
      sessionId: input.sessionId ?? null,
      eventType: input.eventType ?? "session.updated",
      causationRequestId: null,
      origin: { kind: "daemon", deviceId: null },
      remoteOrigin: input.remoteOrigin ?? null,
      createdAt: "2026-10-01T00:00:00.000Z",
      payload: { view: payloadView },
    },
  };
}

/** `auth.server_challenge`。 */
export function makeChallenge(input: {
  readonly connectionId: Uuid;
  readonly hostId?: Uuid;
  readonly selectedProtocolVersion?: number;
  readonly serverNonce?: string;
}): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "auth.server_challenge",
    messageId: "cbb91891-8f84-4b47-bdf3-2c902c338367",
    body: {
      selectedProtocolVersion: input.selectedProtocolVersion ?? 1,
      hostId: input.hostId ?? "bdb2ec20-f98c-4d87-b789-e540d527ef87",
      connectionId: input.connectionId,
      serverNonce: input.serverNonce ?? "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
      selectedFeatures: ["core.event-ack.v1"],
      hostProof: "AQID",
    },
  };
}

/** `auth.authenticated`。 */
export function makeAuthenticated(input: {
  readonly connectionId: Uuid;
  readonly deviceId?: Uuid;
  /** 服务端方向的计数器值；用于断言客户端**不**抄服务端的序号。 */
  readonly serverConnectionSequence?: string;
  readonly serverEpoch?: Uuid;
}): AuthAuthenticated {
  return {
    protocolVersion: 1,
    type: "auth.authenticated",
    messageId: "8a536862-9a18-4766-b4ce-a2e478734980",
    connectionId: input.connectionId,
    connectionSequence: input.serverConnectionSequence ?? "1",
    body: {
      deviceId: input.deviceId ?? "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
      scopes: ["session.list", "session.read"],
      serverEpoch: input.serverEpoch ?? "00384a03-bc90-4095-b65d-82fb8cc47e13",
      headGlobalSequence: "2318",
      heartbeatIntervalMs: 30000,
      limits: {
        maxMessageBytes: 1048576,
        maxPromptBytes: 262144,
        maxReplayEventsPerBatch: 500,
      },
    },
  };
}

/** `sync.snapshot_begin`。 */
export function makeSnapshotBegin(input: {
  readonly snapshotId: Uuid;
  readonly chunkCount: 1 | 2 | 3;
  readonly cursor?: Cursor;
}): SyncSnapshotBegin {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_begin",
    messageId: "aaaaaaaa-1111-4111-8111-000000000001",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      snapshotId: input.snapshotId,
      cursor: input.cursor ?? { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
      schemaVersion: 1,
      chunkCount: input.chunkCount,
    },
  };
}

/** 一个 `sync.snapshot_chunk` 及其**原始文本**（digest 按原始字节计算）。 */
export function makeSnapshotChunk(input: {
  readonly snapshotId: Uuid;
  readonly chunkIndex: string;
  readonly resource: "sessions" | "workspaces" | "agents";
  readonly items: readonly unknown[];
}): { readonly message: SyncSnapshotChunk; readonly rawText: string } {
  const message = {
    protocolVersion: 1,
    type: "sync.snapshot_chunk",
    messageId: "aaaaaaaa-1111-4111-8111-000000000002",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      snapshotId: input.snapshotId,
      chunkIndex: input.chunkIndex,
      resource: input.resource,
      items: input.items,
    },
  } as SyncSnapshotChunk;
  return { message, rawText: JSON.stringify(message) };
}

/** `sync.snapshot_end`。 */
export function makeSnapshotEnd(input: {
  readonly snapshotId: Uuid;
  readonly chunkCount: 1 | 2 | 3;
  readonly digest: string;
  readonly cursor?: Cursor;
}): SyncSnapshotEnd {
  return {
    protocolVersion: 1,
    type: "sync.snapshot_end",
    messageId: "aaaaaaaa-1111-4111-8111-000000000003",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      snapshotId: input.snapshotId,
      cursor: input.cursor ?? { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
      chunkCount: input.chunkCount,
      snapshotDigest: input.digest,
    },
  };
}

/** `command.result`。 */
export function makeCommandResult(input: {
  readonly requestId: Uuid;
  readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
}): CommandResultMessage {
  const rejected = input.status === "rejected";
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-000000000001",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.prompt",
      status: input.status,
      acceptedAt: rejected ? null : "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result: null,
      error: rejected ? { code: "session.busy", message: "会话忙", retryable: true, details: {} } : null,
    },
  };
}

/** 摘要端口替身：记录调用，便于断言两级哈希各发生一次且顺序正确。 */
export interface RecordingDigest {
  readonly calls: Uint8Array[];
  sha256(data: Uint8Array): Promise<Uint8Array>;
}

/**
 * 等待一个可观察条件成立。
 *
 * 用**微任务轮转**而不是定时器：握手链上的每一步都是对已决 promise 的 `await`
 * （假端口的方法立即返回），因此每轮 `await Promise.resolve()` 恰好推进一格，
 * 不引入任何真实时间依赖；轮数上限只用于把「条件永不成立」变成一条明确的失败信息。
 *
 * @param label 失败时报告的条件描述。
 */
export async function waitUntil(predicate: () => boolean, label: string): Promise<void> {
  const maxRounds = 200;
  for (let round = 0; round < maxRounds; round += 1) {
    if (predicate()) return;
    await Promise.resolve();
  }
  throw new Error(`等待条件未成立：${label}`);
}

/** 一条被测连接连同它的全部替身（测试以命名类型引用，不用 `ReturnType`）。 */
export interface Harness {
  readonly connection: SyncConnection;
  readonly sockets: FakeSocketFactory;
  readonly random: FakeRandom;
  readonly transcript: FakeTranscript;
  readonly identity: FakeIdentity;
  readonly host: FakeHostIdentity;
  readonly digest: RecordingDigest;
  readonly events: SyncClientEvent[];
  readonly ledger: EventLedger;
}
