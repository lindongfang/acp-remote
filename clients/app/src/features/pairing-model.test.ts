/**
 * 配对页状态机与仓库用例（WP1）。
 *
 * ## 覆盖什么、不覆盖什么
 *
 * 覆盖：`PairingPageModel` 的固定呈现与失败分类、`ClientStore` 在**注入替身接线**下的
 * 完整行为——配对码入口经 lookup 进入 claim/轮询、误扫设备二维码只显示固定指引且不发请求、
 * 旧设备 `data=` 明确停用、node-link 只读展示、到期定时器停止轮询并进入固定失败、
 * 已批准时用响应的主机材料写已配对记录、载荷（含 `pairingSecret`）不进公开状态。
 *
 * **不覆盖**：真实的 `fetch`、真实的地址栏（`location`/`history.replaceState`）、真实的本地缓存。
 * 那些属于 `src/composition.ts` 的装配面，由 `src/composition.test.ts` 用同一个仓库跑真装配来验。
 *
 * ## 判别力来源
 *
 * - 主机公钥取自**已批准响应**：lookup 里的那份与响应里的那份刻意不同，抄错来源会让
 *   「记录三字段」这条断言变红。
 * - 「误扫不发请求」：`q2=` / `data=` 两种片段都断言 lookup / claim / poll / 定时器计数为 0，
 *   且页面模型里没有载荷内容。
 * - 「类型互不混淆」：node-link 入口读到设备载荷时断言三类计数为 0，且固定文案说清是哪一类。
 * - 「到期即停」：让按 `expiresAt` 安排的定时器触发，断言轮询与到期定时器都被取消、没有任何
 *   记录落盘，且呈现固定为「配对码不对或已过期，请重新生成配对码」。
 * - 「环境前置条件」：注入非安全上下文 / 跨来源两条环境事实，断言各自落到**可区分**的
 *   固定呈现，且文案与错误对象都不含一次性配对秘密。
 */

import { describe, expect, it } from "vitest";

import { ClientStore } from "./client-store";
import type {
  PairingKind,
  PairingLookupOutcome,
  PairingPageModel,
  PairingPollResult,
  PairingQrRead,
  PairingTransport,
  PairingWiring,
} from "./pairing-model";
import {
  PAIRING_POLL_INTERVAL_MS,
  PairingQrInvalidError,
  applyAwaitingCode,
  applyLookingUp,
  applyNoPayload,
  applyPairingCloseoutFailure,
  applyPairingFailure,
  applyPollResult,
  applyScannedNodeLink,
  canonicalOriginOf,
  codeInvalidFailure,
  initialPairingPageModel,
  insecureContextFailure,
  kindMismatchFailure,
  normalizePairingCode,
  originMismatchFailure,
  pairedHostRecordFor,
  publicErrorForCloseoutFailure,
  publicErrorForQrInput,
  publicErrorForTransportFailure,
} from "./pairing-model";
import type { PairedHostRecord } from "../sync-client";
import type { NodeLinkQrPayload, PairingLookupResponse, PublicError } from "../protocol";

const ORIGIN = "https://work-pc.example.ts.net";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/** 一次性配对秘密（base64url-32 字节）：断言它不出现在任何公开状态里。 */
const QR_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** 二维码里的主机公钥（65 字节 base64url）。 */
const QR_HOST_PUBLIC_KEY = "BAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

/** 状态响应里的主机公钥：与二维码里的那份**故意不同**，用来钉住材料的来源。 */
const APPROVED_HOST_PUBLIC_KEY = "BAYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgY";

/** 用例时钟：二维码在这个时刻之后到期（倒计时仍有余量）。 */
const NOW_MS = Date.parse("2026-10-17T23:00:00.000Z");

/** 一次配对码 lookup 的成功响应（`PairingLookupResponse`）：与 claim/轮询使用的绑定同形。 */
const QR: PairingLookupResponse = {
  protocolVersion: 1,
  hostId: HOST_ID,
  hostPublicKey: QR_HOST_PUBLIC_KEY,
  canonicalOrigin: ORIGIN,
  pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  pairingSecret: QR_SECRET,
  expiresAt: "2026-10-18T00:10:00.000Z",
};

/** 一个很快就会到期的 sync 载荷：用来驱动仓库的到期定时器。 */
const SOON_EXPIRING_QR: PairingLookupResponse = {
  ...QR,
  expiresAt: new Date(NOW_MS + 60_000).toISOString(),
};

/**
 * 一份 node-link 配对载荷（`NODE_LINK_PROTOCOL.md` §13.1）：字段齐全、端点形状与字段长度合规。
 *
 * 与 {@link QR} 用同一份 `pairingSecret`：两条「互不混淆」用例都断言它不出现在公开状态里。
 */
const NODE_LINK_QR: NodeLinkQrPayload = {
  pairingProtocol: "acp-remote-nodelink-v1",
  ownerNodeId: HOST_ID,
  ownerPublicKey: QR_HOST_PUBLIC_KEY,
  endpoint: "wss://work-pc.example.ts.net/node-link/v1",
  pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  pairingSecret: QR_SECRET,
  expiresAt: "2026-10-18T00:10:00.000Z",
};

const APPROVED: PairingPollResult = {
  kind: "approved",
  deviceId: DEVICE_ID,
  hostId: HOST_ID,
  hostPublicKey: APPROVED_HOST_PUBLIC_KEY,
};

/**
 * 端点的形状正则（`src/composition.ts` 的 `NODE_LINK_ENDPOINT_PATTERN`，与
 * `schemas/node-link/v1/pairing.schema.json` 的 `$defs.endpoint` 逐字相同）：组合根据此判端点是否合法。
 *
 * 本文件里只用来**证明**下面那组端点确实「过得了校验层」，否则用例会退化成「随便一个非法端点」——
 * 换成正则也拦得住的形状就失去判别力。
 */
const NODE_LINK_ENDPOINT_PATTERN = /^wss:\/\/[^/?#]+\/node-link\/v1$/;

/**
 * 平台层的「来源不符」错误（`SecureStorageError` 的结构化形状，`kind: "origin_mismatch"`）。
 *
 * 这里不 import `src/platform/`：feature 层依赖平台层是分层测试判死的反向依赖
 * （`src/layers.ts` / `src/layers.test.ts`）。被测的只是「按 `kind` 识别来源不符」这一条形状约定，
 * 而**真实**的 `SecureStorageError` 走的是 `composition.test.ts` 里那条端到端用例。
 */
class OriginMismatchError extends Error {
  readonly kind = "origin_mismatch";

  constructor() {
    super("已绑定的规范化来源与本次来源不符");
    this.name = "SecureStorageError";
  }
}

/** 一条已安排的定时任务：轮询与到期回调共用同一个 `schedule` 出口。 */
interface ScheduledTask {
  readonly delayMs: number;
  readonly task: () => void;
}

/** 配对接线替身：每一次调用都记账，并按脚本返回 claim / poll 的结果。 */
class RecordingPairingWiring implements PairingWiring {
  readonly claims: PairingLookupResponse[] = [];
  readonly polls: { readonly pairingId: string; readonly nonce: string }[] = [];
  readonly written: PairedHostRecord[] = [];
  readonly onPairedRecords: PairedHostRecord[] = [];
  /** 调用顺序证据：记录必须先落盘，才允许收口到「已配对」。 */
  readonly order: string[] = [];
  readonly scheduled: ScheduledTask[] = [];
  /** 每次 `schedule` 的时长（含到期与轮询）。 */
  readonly delays: number[] = [];
  /** 地址片段里待读取的原始内容（含 `q2=` / `data=` 标记）；读一次即清空（真实实现是读取后立即清除）。 */
  fragment: string | null = null;
  /** 交给 `readQrPayload` 的原文：证明片段内容真的被读到了。 */
  readText: string | null = null;
  /** `readQrPayload` 的返回值（缺省是设备载荷的分类结果）：用来验「载荷类型与入口声明的类型不一致」。 */
  readResult: PairingQrRead | null = null;
  readError: unknown = null;
  /** 配对码 lookup 的请求留痕与脚本化结果。 */
  readonly lookups: { readonly canonicalOrigin: string; readonly pairingCode: string }[] = [];
  lookupQr: PairingLookupResponse | null = null;
  lookupResult: PairingLookupOutcome | null = null;
  lookupThrows: unknown = null;
  claimError: PublicError | null = null;
  /** `claim` **挂起**：一个受控的 deferred，让到期定时器有机会先跑完（`claim` 在途时到期）。 */
  claimGate: Promise<unknown> | null = null;
  /** `claim` **抛出**（而不是返回 `PublicError`）：网络不可达、来源不符等都走这一支。 */
  claimThrows: unknown = null;
  results: PairingPollResult[] = [];
  /** `poll` **抛出**：同上，用来覆盖「请求本身没能完成」的收口。 */
  pollThrows: unknown = null;
  writeError: unknown = null;
  /** 组合根的收口（装配连接并触发一次连接）失败：`onPaired` 抛出。 */
  onPairedError: unknown = null;
  #nonceCounter = 0;

  readonly transport: PairingTransport = {
    readQrPayload: (text: string): PairingQrRead => {
      this.readText = text;
      if (this.readError !== null) throw this.readError;
      return this.readResult ?? { kind: "sync" };
    },
    lookup: async (input: { readonly canonicalOrigin: string; readonly pairingCode: string }) => {
      this.lookups.push({ canonicalOrigin: input.canonicalOrigin, pairingCode: input.pairingCode });
      if (this.lookupThrows !== null) throw this.lookupThrows;
      return this.lookupResult ?? { kind: "ok", qr: this.lookupQr ?? QR };
    },
    claim: async (input: { readonly qr: PairingLookupResponse; readonly canonicalOrigin: string }) => {
      this.order.push("claim");
      this.claims.push(input.qr);
      if (this.claimGate !== null) await this.claimGate;
      if (this.claimThrows !== null) throw this.claimThrows;
      return this.claimError;
    },
    poll: async (input: { readonly qr: PairingLookupResponse; readonly requestNonce: string }) => {
      this.order.push("poll");
      this.polls.push({ pairingId: input.qr.pairingId, nonce: input.requestNonce });
      if (this.pollThrows !== null) throw this.pollThrows;
      const index = Math.min(this.polls.length - 1, this.results.length - 1);
      return (
        this.results[index] ?? {
          kind: "error",
          error: { code: "internal.unavailable", message: "替身没有更多轮询结果", retryable: false, details: {} },
        }
      );
    },
  };

  takeQrTextFromFragment(): string | null {
    const text = this.fragment;
    this.fragment = null;
    this.order.push("fragment");
    return text;
  }

  async rememberPairedHost(record: PairedHostRecord): Promise<void> {
    this.order.push("rememberPairedHost");
    if (this.writeError !== null) throw this.writeError;
    this.written.push(record);
  }

  async onPaired(record: PairedHostRecord): Promise<void> {
    this.order.push("onPaired");
    if (this.onPairedError !== null) throw this.onPairedError;
    this.onPairedRecords.push(record);
  }

  schedule(delayMs: number, task: () => void): () => void {
    this.delays.push(delayMs);
    const entry: ScheduledTask = { delayMs, task };
    this.scheduled.push(entry);
    return () => {
      const index = this.scheduled.indexOf(entry);
      if (index >= 0) this.scheduled.splice(index, 1);
    };
  }

  nextRequestNonce(): string {
    this.#nonceCounter += 1;
    return `nonce-${this.#nonceCounter}`;
  }

  /**
   * 环境事实（design D2）：缺省「安全上下文成立、本页来源等于二维码声明的来源」，让既有用例
   * 判的是配对链路本身；新增的两条前置条件用例各自把其中一项破掉来造出失败形态。
   */
  secure = true;
  origin = ORIGIN;

  isSecureContext(): boolean {
    return this.secure;
  }

  currentOrigin(): string {
    return this.origin;
  }

  /** 是否有已安排的轮询（`PAIRING_POLL_INTERVAL_MS` 那一条）。 */
  get hasScheduledPoll(): boolean {
    return this.scheduled.some((entry) => entry.delayMs === PAIRING_POLL_INTERVAL_MS);
  }

  /**
   * 触发一次已安排的轮询，并等它的异步链走完。
   *
   * 不引入真实时钟、也不猜时长：只连续让出微任务队列——被触发的链上全是 `async` 函数
   * （`poll` → 落盘 → 收口），没有需要等待的宏任务或定时器。
   */
  async runScheduledPoll(): Promise<void> {
    const index = this.scheduled.findIndex((entry) => entry.delayMs === PAIRING_POLL_INTERVAL_MS);
    if (index < 0) throw new Error("没有已安排的轮询");
    const [entry] = this.scheduled.splice(index, 1);
    entry?.task();
    await this.flush();
  }

  /** 触发到期回调（不是轮询的那一条定时任务）。 */
  async runExpiry(): Promise<void> {
    const index = this.scheduled.findIndex((entry) => entry.delayMs !== PAIRING_POLL_INTERVAL_MS);
    if (index < 0) throw new Error("没有已安排的到期回调");
    const [entry] = this.scheduled.splice(index, 1);
    entry?.task();
    await this.flush();
  }

  /** 连续让出微任务队列，让 `async` 链路走完。 */
  async flush(): Promise<void> {
    for (let round = 0; round < 16; round += 1) await Promise.resolve();
  }
}

/** 一个只装配了配对链路的仓库：同步网关是空的——这条用例不派发任何命令。 */
function storeWithPairing(pairing: PairingWiring | undefined): ClientStore {
  return new ClientStore({
    sync: {
      current: null,
      refreshSnapshot: () => undefined,
      authenticatedInfo: null,
      dispatch: () => undefined,
      setSessionView: () => true,
      subscribeRemote: () => undefined,
      unsubscribeRemote: () => undefined,
    },
    ids: { uuid: () => "00000000-0000-4000-8000-000000000001" },
    now: () => NOW_MS,
    identity: { connectionId: null },
    pairing,
  });
}

/** 让仓库从「地址片段」进入配对流程：本仓库只把它用于 node-link 入口。 */
async function enterFromAddress(
  store: ClientStore,
  wiring: RecordingPairingWiring,
  kind: PairingKind = "sync",
): Promise<void> {
  wiring.fragment = "data=fixture";
  await store.startPairingFromAddress(kind);
}

/** 浏览器唯一的配对入口：裸 `/pair` → 提交 8 位码 → lookup → claim。 */
async function enterByCode(store: ClientStore, code = "ABCD-EFGH"): Promise<void> {
  await store.startPairingFromAddress("sync");
  await store.submitPairingCode(code);
}

describe("配对码链路：lookup → 认领 → 轮询 → 已配对", () => {
  it("合法配对码：claim → 轮询 → 已配对；记录三字段，写入绝对截止时间", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [{ kind: "pending_confirmation" }, APPROVED];
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    const claiming = store.pairingPage();
    expect(claiming.phase).toBe("claiming");
    expect(claiming.presentation.kind).toBe("progress");
    expect(claiming.presentation.countdown).toBe(true);
    expect(claiming.expiresAtMs).toBe(Date.parse(QR.expiresAt));
    // 码只发往同源 lookup，且只发一次。
    expect(wiring.lookups).toEqual([{ canonicalOrigin: ORIGIN, pairingCode: "ABCD-EFGH" }]);
    // 还没批准：状态机不动（仍是未配对），也就不该拿去连接。
    expect(store.state.connection.state).toBe("unpaired");
    // 到期回调 + 轮询按约定节奏安排：给一个窗口，而不是立刻打服务端。
    expect(wiring.delays).toEqual([
      Math.max(0, Date.parse(QR.expiresAt) - NOW_MS),
      PAIRING_POLL_INTERVAL_MS,
    ]);

    await wiring.runScheduledPoll();
    const pending = store.pairingPage();
    expect(pending.phase).toBe("pending_confirmation");
    expect(pending.presentation.title).toBe("配对中");
    expect(pending.presentation.countdown).toBe(true);
    expect(wiring.polls).toHaveLength(1);
    // 每次轮询换新 nonce（§7.3 的 schema 要求）：复用同一个会变红。
    expect(wiring.polls.map((poll) => poll.nonce)).toEqual(["nonce-1"]);

    await wiring.runScheduledPoll();

    // 记录三字段：主机标识、主机公钥（取自**已批准响应**）、规范化来源。
    // 反例：抄 lookup 里的公钥（`QR.hostPublicKey` 与下面那条断言里的
    // `APPROVED_HOST_PUBLIC_KEY` 刻意不同）、或只写 `deviceId` 的旧口径 → 这条断言变红。
    expect(wiring.written).toEqual([
      { hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN },
    ]);
    // 落盘必须先于收口到「已配对」，且收口交给组合根去装配连接。
    expect(wiring.order).toEqual(["fragment", "claim", "poll", "poll", "rememberPairedHost", "onPaired"]);
    expect(wiring.onPairedRecords).toEqual(wiring.written);
    expect(wiring.polls.map((poll) => poll.nonce)).toEqual(["nonce-1", "nonce-2"]);

    expect(store.state.connection.state).toBe("disconnected");
    const approved = store.pairingPage();
    expect(approved.phase).toBe("approved");
    expect(approved.deviceId).toBe(DEVICE_ID);
    expect(approved.presentation.title).toBe("已通过审批，正在进入目录");
    expect(approved.presentation.countdown).toBe(false);
    expect(store.state.host).toEqual({ address: ORIGIN, hostId: HOST_ID, deviceId: DEVICE_ID });

    // 载荷用完即弃：秘密不出现在可导出、可快照的公开状态里。
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    // 终态之后不再轮询、也不再有到期定时器在等。
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("到期定时器触发：停止轮询、固定 S6、不落盘任何记录", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupQr = SOON_EXPIRING_QR;
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    expect(wiring.hasScheduledPoll).toBe(true);

    await wiring.runExpiry();

    const page = store.pairingPage();
    // 配对码入口只有一种过期呈现：固定 S6，不显示服务端细节。
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    // 反例：到期只改页面状态却没取消轮询 → 这里会留下一条 poll 定时器。
    expect(wiring.hasScheduledPoll).toBe(false);
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
  });

  it("claim 在途时到期：迟到的 claim 失败不覆盖固定的 S6，也不落盘、不建立连接", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupQr = SOON_EXPIRING_QR;
    // claim 挂起：先不返回，让到期定时器先收口，再让这次 claim 失败。
    let rejectClaim!: (reason: unknown) => void;
    wiring.claimGate = new Promise<unknown>((_resolve, reject) => {
      rejectClaim = reject;
    });
    const store = storeWithPairing(wiring);

    const entering = enterByCode(store);
    await wiring.flush();
    expect(store.pairingPage().phase).toBe("claiming");
    expect(wiring.claims).toHaveLength(1);

    // 到期定时器先跑：会话结束、页面落到固定的「配对码不对或已过期」。
    await wiring.runExpiry();
    expect(store.pairingPage().phase).toBe("failed");

    // 迟到的 claim 失败（通用传输失败的码）不得把页面从 S6 改回「请求未能完成」。
    rejectClaim(new Error("Failed to fetch"));
    await entering;

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    // 反例：没有会话空值保护 → 页面会被覆盖成 `request_failed`，上面三条断言变红。
    expect(wiring.written).toHaveLength(0);
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
    expect(wiring.hasScheduledPoll).toBe(false);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("已批准但主机材料不可用：不写记录、不进已配对、不触发连接", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [
      {
        kind: "error",
        error: {
          code: "protocol.schema_invalid",
          message: "已批准响应缺少可用的主机身份材料（host.hostPublicKey）：该设备不会被当作已配对。",
          retryable: false,
          details: {},
        },
      },
    ];
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("unrecognized");
    expect(page.error?.code).toBe("protocol.schema_invalid");
    // 固定文案不渲染后端 message，也不含其内容。
    expect(page.presentation.lede).not.toContain("主机身份材料");
    expect(wiring.written).toHaveLength(0);
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
    expect(store.state.host).toBeNull();
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("claim 被拒为已过期：落到固定 S6 呈现，不再轮询、也不写记录", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.claimError = { code: "pairing.expired", message: "这次配对已过期。", retryable: false, details: {} };
    const store = storeWithPairing(wiring);

    await enterByCode(store);

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
  });

  it("claim 被拒为已被使用：落到 consumed 呈现", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.claimError = {
      code: "pairing.already_claimed",
      message: "这台电脑已经登记过本设备。",
      retryable: false,
      details: {},
    };
    const store = storeWithPairing(wiring);

    await enterByCode(store);

    const page = store.pairingPage();
    expect(page.phase).toBe("consumed");
    expect(page.presentation.title).toBe("这个配对地址已经被使用");
    expect(wiring.written).toHaveLength(0);
  });

  it("claim **抛出**来源不符：会话结束，落到 origin_mismatch 固定呈现", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.claimThrows = new OriginMismatchError();
    const store = storeWithPairing(wiring);

    await enterByCode(store);

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("origin_mismatch");
    expect(page.error?.code).toBe("auth.origin_mismatch");
    expect(page.presentation.title).toBe("这个地址属于另一台电脑");
    // 平台错误的原文（可能带请求上下文）不透传进页面。
    expect(page.error?.message).not.toContain("已绑定的规范化来源");
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
  });

  it("poll **抛出**（网络中断）：结束会话并落到请求失败的固定呈现", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.pollThrows = new Error("Failed to fetch");
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("request_failed");
    expect(page.presentation.title).toBe("配对请求未能完成");
    expect(page.error?.code).toBe("internal.unavailable");
    expect(page.error?.message).not.toContain("Failed to fetch");
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
  });

  it("已配对记录写不进去：明确失败且不触发连接", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [APPROVED];
    wiring.writeError = new Error("QuotaExceededError");
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("request_failed");
    expect(page.error?.code).toBe("internal.unavailable");
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
  });

  it("组合根收口时抛出（连接没起来）：配对仍算完成，页面仍呈现 approved", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [APPROVED];
    wiring.onPairedError = new Error("Failed to fetch");
    const store = storeWithPairing(wiring);

    await enterByCode(store);
    await wiring.runScheduledPoll();

    expect(wiring.written).toEqual([
      { hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN },
    ]);
    expect(store.state.connection.state).toBe("disconnected");
    const page = store.pairingPage();
    // 反例：收口失败若走 `applyPairingFailure`，阶段会变成 `failed`（与已写入的记录矛盾）。
    expect(page.phase).toBe("approved");
    expect(page.presentation.kind).toBe("progress");
    expect(page.presentation.title).toBe("已通过审批，正在进入目录");
    expect(page.presentation.countdown).toBe(false);
    expect(page.error?.code).toBe("internal.unavailable");
    expect(page.error?.details).toEqual({});
    expect(page.error?.message).not.toContain("Failed to fetch");
  });
});

describe("配对链路：入口与固定呈现", () => {
  it("/pair 没有片段：进入配对码输入（S0），显示来源与静态提示，且一个请求、一个定时器都不安排", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("awaiting_code");
    expect(page.failure).toBeNull();
    expect(page.presentation.kind).toBe("code_entry");
    expect(page.presentation.title).toBe("输入配对码");
    // lookup 之前没有服务端 `expiresAt`：只有静态提示，没有数字倒计时。
    expect(page.presentation.note).toBe("配对码 5 分钟内有效。");
    expect(page.presentation.countdown).toBe(false);
    expect(page.origin).toBe(ORIGIN);
    expect(page.secureContext).toBe(true);
    expect(page.host).toBeNull();
    expect(page.expiresAtMs).toBeNull();
    // 反例：输入态也去 lookup / claim / 轮询 / 安排定时器 → 计数非 0。
    expect(wiring.readText).toBeNull();
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("node-link 入口没有片段：节点专用无载荷文案", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("node-link");

    const page = store.pairingPage();
    expect(page.phase).toBe("idle");
    expect(page.kind).toBe("node-link");
    expect(page.failure).toBe("no_payload");
    expect(page.presentation.title).toBe("这个地址里没有节点配对信息");
    expect(wiring.claims).toHaveLength(0);
  });

  it("未装配配对链路：明确失败，而不是静默无操作", async () => {
    const store = storeWithPairing(undefined);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("request_failed");
    expect(page.error?.code).toBe("internal.unavailable");
    expect(page.error?.message).toContain("配对链路未装配");
  });

  it("误扫新设备短二维码：清除后只显示固定 App 指引，不读载荷、不发请求", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.fragment = "q2=fixture-payload";
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("device_qr_app_only");
    expect(page.presentation.kind).toBe("info");
    expect(page.presentation.title).toBe("请在 ACP Remote App 内扫码");
    expect(page.presentation.lede).toContain("请在 ACP Remote App 内扫码，或返回网页使用配对码");
    // 不读载荷：组合根只做了清除与类型前缀判断。
    expect(wiring.readText).toBeNull();
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    // 片段内容不进公开状态。
    expect(JSON.stringify(store.state)).not.toContain("fixture-payload");
  });

  it("node-link 入口误扫设备 q2：同样只显示固定 App 指引，不发请求", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.fragment = "q2=fixture-payload";
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("node-link");

    const page = store.pairingPage();
    expect(page.phase).toBe("device_qr_app_only");
    expect(page.kind).toBe("node-link");
    expect(wiring.readText).toBeNull();
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("旧设备 data=：清除后提示已停用；不解析设备字段、不发请求", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.readResult = { kind: "sync" };
    wiring.fragment = `data=${QR_SECRET}`;
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("legacy_device_qr_unsupported");
    expect(page.presentation.title).toBe("设备二维码旧格式已停用");
    expect(page.error?.code).toBe("protocol.schema_invalid");
    // 只做类型分类，不解析字段；没有 claim / poll / 定时器。
    expect(wiring.readText).toBe(`data=${QR_SECRET}`);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    // 片段里的秘密（这里借用 QR_SECRET 作为载荷文本）不进页面或公开状态。
    expect(JSON.stringify(page)).not.toContain(QR_SECRET);
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
  });

  it("旧设备 data= 分类失败：仍按「已停用」处理，不发请求", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.readError = new PairingQrInvalidError("protocol.invalid_json", "二维码内容不是 JSON 对象");
    wiring.fragment = "data=not-json";
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("legacy_device_qr_unsupported");
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("/pair 的 data= 是 node-link 载荷：专用入口不匹配文案，不发请求", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.readResult = { kind: "node-link", qr: NODE_LINK_QR };
    wiring.fragment = "data=fixture";
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("kind_mismatch");
    expect(page.presentation.title).toBe("这不是本入口的配对地址");
    expect(page.presentation.lede).toContain("node-link");
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
  });

  it("任意异常文本不透传：失败文案不含不可控内容（可能带着载荷片段）", () => {
    const error = publicErrorForQrInput(new Error(`解析失败：${QR_SECRET}`));
    expect(error.code).toBe("protocol.schema_invalid");
    expect(error.message).not.toContain(QR_SECRET);
    expect(error.details).toEqual({});
    // 过期码原样保留，由 `applyPairingFailure` 落到固定的 S6 呈现。
    const expired = publicErrorForQrInput(new PairingQrInvalidError("pairing.expired", "二维码已过期"));
    expect(expired.code).toBe("pairing.expired");
  });
});

describe("配对码入口：S0 → lookup → 同一套 claim/轮询（design D6）", () => {
  it("格式归一化：大小写不敏感、I/L→1、O→0、允许一个 -；其余格式在发请求前拒绝", () => {
    expect(normalizePairingCode("abcd-efgh")).toBe("ABCD-EFGH");
    expect(normalizePairingCode("abcd1234")).toBe("ABCD-1234");
    expect(normalizePairingCode("IL0O1234")).toBe("1100-1234");
    expect(normalizePairingCode("  ABCD-EFGH  ")).toBe("ABCD-EFGH");
    expect(normalizePairingCode("ABCD-EFG")).toBeNull();
    expect(normalizePairingCode("ABCD-EFGHJ")).toBeNull();
    expect(normalizePairingCode("ABCD_EFGH")).toBeNull();
    expect(normalizePairingCode("ABCD-EFG1-")).toBeNull();
    expect(normalizePairingCode("UUUU-UUUU")).toBeNull();
    expect(normalizePairingCode("")).toBeNull();
  });

  it("格式不符：lookup 之前明确失败，不发任何请求", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("UUUU-UUUU");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("合法码：lookup → claim → 轮询 → 已配对；倒计时用 lookup 响应的 expiresAt，码不进公开状态", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [{ kind: "pending_confirmation" }, APPROVED];
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    const submitted = store.submitPairingCode("abcd-efgh");
    // S1：lookup 在途，仍没有服务端 `expiresAt`，因此不显示数字倒计时。
    expect(store.pairingPage().phase).toBe("looking_up");
    expect(store.pairingPage().presentation.countdown).toBe(false);
    expect(store.pairingPage().expiresAtMs).toBeNull();
    expect(wiring.lookups).toEqual([{ canonicalOrigin: ORIGIN, pairingCode: "ABCD-EFGH" }]);

    await submitted;
    await wiring.flush();

    // lookup 之后进入**既有**的 claim 链路：没有第二套状态机。
    const claiming = store.pairingPage();
    expect(claiming.phase).toBe("claiming");
    expect(claiming.presentation.countdown).toBe(true);
    expect(claiming.expiresAtMs).toBe(Date.parse(QR.expiresAt));
    expect(claiming.host).toEqual({ address: ORIGIN, hostId: HOST_ID });
    expect(wiring.claims).toEqual([QR]);
    expect(wiring.delays[0]).toBe(Math.max(0, Date.parse(QR.expiresAt) - NOW_MS));

    await wiring.runScheduledPoll();
    expect(store.pairingPage().phase).toBe("pending_confirmation");
    await wiring.runScheduledPoll();

    expect(store.pairingPage().phase).toBe("approved");
    expect(wiring.written).toEqual([
      { hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN },
    ]);
    expect(wiring.order).toEqual([
      "fragment",
      "claim",
      "poll",
      "poll",
      "rememberPairedHost",
      "onPaired",
    ]);
    // 码只活在组件内存与这次请求里：公开状态中既没有码，也没有配对秘密。
    expect(JSON.stringify(store.state)).not.toContain("ABCD-EFGH");
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("lookup 返回失败：固定文案、不 claim、不轮询、不落盘，且不暴露原因", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupResult = { kind: "error" };
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("ABCD-EFGH");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(page.presentation.lede).not.toContain(QR_SECRET);
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(page.error?.details).toEqual({});
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
  });

  it("lookup 抛出（响应丢失）：同一固定失败呈现，不自动重试", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupThrows = new Error("Failed to fetch");
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("ABCD-EFGH");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(page.error?.message).not.toContain("Failed to fetch");
    expect(wiring.lookups).toHaveLength(1);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("lookup 返回的码已过期：不 claim，进入同一固定失败呈现", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupQr = { ...QR, expiresAt: new Date(NOW_MS - 1).toISOString() };
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("ABCD-EFGH");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("配对码入口倒计时归零：固定呈现「配对码不对或已过期」（S6）", async () => {
    const wiring = new RecordingPairingWiring();
    // lookup 返回一个很快到期的绑定：到期只能由页面按绝对时间推进。
    wiring.lookupQr = SOON_EXPIRING_QR;
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("ABCD-EFGH");
    await wiring.flush();
    expect(store.pairingPage().phase).toBe("claiming");
    expect(wiring.hasScheduledPoll).toBe(true);

    await wiring.runExpiry();

    const page = store.pairingPage();
    // 反例：若过期仍保留「配对地址已过期」的分流 → 标题与阶段都会变红。
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(page.error?.code).toBe("pairing.code_invalid");
    expect(wiring.hasScheduledPoll).toBe(false);
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
  });

  it("配对码入口的服务端过期（轮询终态）：同一固定呈现，停止轮询、不落盘", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [
      {
        kind: "terminal",
        status: "expired",
        // 真实 HTTPS 传输层对 HTTP 200 的 `expired` 正文只能合成 `pairing.failed`（§7.3），
        // 这里用同一个码，避免替身比生产实现更「友好」而掩盖入口分流。
        error: { code: "pairing.failed" as PublicError["code"], message: "服务端说过期", retryable: false, details: {} },
      },
    ];
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");
    await store.submitPairingCode("ABCD-EFGH");
    await wiring.flush();
    expect(wiring.hasScheduledPoll).toBe(true);

    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("code_invalid");
    expect(page.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(wiring.hasScheduledPoll).toBe(false);
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
    expect(wiring.polls).toHaveLength(1);
  });

  it("非安全上下文：输入态可见，提交时明确失败且不发 lookup", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.secure = false;
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    expect(store.pairingPage().phase).toBe("awaiting_code");
    expect(store.pairingPage().secureContext).toBe(false);

    await store.submitPairingCode("ABCD-EFGH");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("insecure_context");
    expect(page.presentation.title).toBe("需要 HTTPS 才能配对");
    expect(page.error?.code).toBe("auth.insecure_context");
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
  });

  it("lookup 在途的重复提交被忽略（只发一次请求）", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    const first = store.submitPairingCode("ABCD-EFGH");
    const second = store.submitPairingCode("ABCD-EFGH");
    await Promise.all([first, second]);

    expect(wiring.lookups).toHaveLength(1);
  });
});

describe("两类配对入口互不混淆（design D2/D3）", () => {
  const CASES: readonly {
    readonly name: string;
    readonly entry: PairingKind;
    readonly read: PairingQrRead;
    readonly foreign: string;
  }[] = [
    {
      name: "sync 入口收到 node-link 载荷",
      entry: "sync",
      read: { kind: "node-link", qr: NODE_LINK_QR },
      foreign: "node-link",
    },
    {
      name: "node-link 入口收到设备载荷",
      entry: "node-link",
      read: { kind: "sync" },
      foreign: "设备（sync）",
    },
  ];

  for (const scenario of CASES) {
    it(`${scenario.name}：明确失败，且一个请求都不发`, async () => {
      const wiring = new RecordingPairingWiring();
      wiring.readResult = scenario.read;
      wiring.results = [{ kind: "pending_confirmation" }];
      const store = storeWithPairing(wiring);

      await enterFromAddress(store, wiring, scenario.entry);

      const page = store.pairingPage();
      expect(page.phase).toBe("failed");
      expect(page.failure).toBe("kind_mismatch");
      expect(page.error?.code).toBe("protocol.schema_invalid");
      expect(page.presentation.title).toBe("这不是本入口的配对地址");
      // 文案说清「哪一类喂给了哪一类」，但不回显载荷内容（它可能含一次性配对秘密）。
      expect(page.presentation.lede).toContain(scenario.foreign);
      expect(page.presentation.lede).not.toContain(QR_SECRET);
      expect(wiring.claims).toHaveLength(0);
      expect(wiring.polls).toHaveLength(0);
      expect(wiring.scheduled).toHaveLength(0);
    });
  }

  it("node-link 载荷在 node-link 入口：只呈现公开信息，不发请求、不留秘密、不显示倒计时", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.readResult = { kind: "node-link", qr: NODE_LINK_QR };
    const store = storeWithPairing(wiring);

    await enterFromAddress(store, wiring, "node-link");

    const page = store.pairingPage();
    expect(page.phase).toBe("node_link_scanned");
    // 入口类型来自路由声明：成功路径同样写着 node-link（不能留在默认的 sync）。
    expect(page.kind).toBe("node-link");
    // 呈现是 info，不是失败也不是停在 reading。
    expect(page.presentation.kind).toBe("info");
    expect(page.presentation.title).toBe("已读到 node-link 配对内容");
    expect(page.presentation.countdown).toBe(false);
    expect(page.expiresAtMs).toBeNull();
    // §13.1：HTTP 来源由 `endpoint` 的 host 派生；对端标识是 owner 节点。
    expect(page.host).toEqual({ address: "https://work-pc.example.ts.net", hostId: HOST_ID });
    expect(page.error).toBeNull();
    // 本客户端不是 node-link 配对的参与方：一个请求都不发，也不安排轮询。
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
  });

  /**
   * 打开一条**已过期**的 node-link `#data=`：组合根在读取时抛 `pairing.expired`，入口收口。
   *
   * 这个页面没有配对码输入（也没有配对码可重新生成）， поэтому不能落到 S6 的
   * 「配对码不对或已过期」+「生成新的配对码后再输入」：那是一条在本页无法执行的恢复路径，
   * 并且会把真实的 `pairing.expired` 诊断码改写成 `pairing.code_invalid`。
   */
  it("node-link 地址已过期：节点专用的过期呈现，不发请求、不出现配对码指引", async () => {
    const wiring = new RecordingPairingWiring();
    // 组合根对过期 node-link 载荷抛出的正是这个错误（`composition.ts` 的 `readNodeLinkQrPayload`）。
    wiring.readError = new PairingQrInvalidError("pairing.expired", "二维码已过期");
    const store = storeWithPairing(wiring);

    await enterFromAddress(store, wiring, "node-link");

    const page = store.pairingPage();
    expect(page.phase).toBe("expired");
    expect(page.kind).toBe("node-link");
    // 过期是入口给出的结论，不是「本侧失败」。
    expect(page.failure).toBeNull();
    expect(page.presentation.kind).toBe("failure");
    expect(page.presentation.title).toBe("配对地址已过期");
    expect(page.presentation.warning).toBe("请在电脑上重新发起配对，并用新的节点地址打开本页。");
    // 反例：若过期仍按配对码口径收口 → 标题变成「配对码不对或已过期」、阶段变成 `failed`。
    expect(page.presentation.title).not.toBe("配对码不对或已过期，请重新生成配对码");
    expect(`${page.presentation.lede}${page.presentation.warning}`).not.toContain("配对码");
    // 诊断码保留真实原因，不被改写成 `pairing.code_invalid`。
    expect(page.error?.code).toBe("pairing.expired");
    // 一个配对请求都不发，也不安排任何定时任务。
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("同一过期错误在配对码链路（sync）仍落到 S6", () => {
    const expired: PublicError = {
      code: "pairing.expired",
      message: "这次配对已过期。",
      retryable: false,
      details: {},
    };
    for (const current of [
      initialPairingPageModel("sync"),
      applyAwaitingCode(initialPairingPageModel("sync"), "sync", ORIGIN, true),
    ]) {
      const next = applyPairingFailure(current, expired);
      expect(next.phase).toBe("failed");
      expect(next.failure).toBe("code_invalid");
      expect(next.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
      expect(next.error?.code).toBe("pairing.code_invalid");
    }
  });

  /**
   * 「过形状正则、但 `new URL` 解析不了」的端点：端口越界、主机里的非法百分号转义。
   *
   * 组合根按 {@link NODE_LINK_ENDPOINT_PATTERN} 判端点合法性，那条正则比 `new URL` 宽松，
   * 因此这几个端点会被**照收**，失败点只在派生 https 来源那一步。
   */
  const UNPARSABLE_ENDPOINTS: readonly string[] = [
    "wss://work-pc.example.ts.net:999999/node-link/v1",
    "wss://work-pc.example.ts.net:70000/node-link/v1",
    "wss://%zz/node-link/v1",
  ];

  it("端点过形状正则但派生不出地址：落到明确失败态，不抛异常、也不停在 reading", async () => {
    const idle = initialPairingPageModel("node-link");
    for (const endpoint of UNPARSABLE_ENDPOINTS) {
      // 前提证据：这些端点确实过得了形状正则（否则本用例判的就不是「校验层放行」这一幕）。
      expect(NODE_LINK_ENDPOINT_PATTERN.test(endpoint), endpoint).toBe(true);

      // 纯函数层：不抛异常，也不停在上一状态。
      const direct = applyScannedNodeLink(idle, { ...NODE_LINK_QR, endpoint });
      expect(direct.phase, endpoint).toBe("failed");
      expect(direct.failure, endpoint).toBe("unrecognized");
      expect(direct.presentation.title, endpoint).toBe("无法识别这个地址");
      expect(direct.error?.code, endpoint).toBe("protocol.schema_invalid");
      expect(direct.host, endpoint).toBeNull();

      // 用例层：同一条路径（读载荷成功 → 入口呈现）——明确失败，且一个请求都不发。
      const wiring = new RecordingPairingWiring();
      wiring.readResult = { kind: "node-link", qr: { ...NODE_LINK_QR, endpoint } };
      const store = storeWithPairing(wiring);

      // 反例：解析放到 `#commit(...)` 的求值里 → 异常穿出 `startPairingFromAddress`（这里会变红）。
      await enterFromAddress(store, wiring, "node-link");

      const page = store.pairingPage();
      expect(page.phase, endpoint).toBe("failed");
      expect(page.error?.code, endpoint).toBe("protocol.schema_invalid");
      expect(wiring.claims, endpoint).toHaveLength(0);
      expect(wiring.polls, endpoint).toHaveLength(0);
      expect(wiring.scheduled, endpoint).toHaveLength(0);
      expect(JSON.stringify(store.state), endpoint).not.toContain(QR_SECRET);
    }
  });
});

describe("配对入口的环境前置条件（design D1–D5）", () => {
  it("来源不符：可区分的固定失败 + claim/status 计数为 0", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.lookupQr = { ...QR, canonicalOrigin: "https://other-host.example.ts.net" };
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await enterByCode(store);

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("origin_mismatch");
    expect(page.presentation.title).toBe("这个地址属于另一台电脑");
    expect(page.error?.code).toBe("auth.origin_mismatch");
    expect(page.error?.retryable).toBe(false);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    expect(page.presentation.lede).not.toContain(QR_SECRET);
    expect(JSON.stringify(page.error)).not.toContain(QR_SECRET);
  });

  it("来源比较走同一归一化口径：默认端口与尾部斜杠不产生误判", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.origin = "https://work-pc.example.ts.net:443/";
    wiring.lookupQr = { ...QR, canonicalOrigin: "https://work-pc.example.ts.net:443/" };
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await enterByCode(store);

    expect(store.pairingPage().phase).toBe("claiming");
    expect(wiring.claims).toHaveLength(1);
  });

  it("非安全上下文：可区分的固定失败 + 一个请求都不发", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.secure = false;
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);
    await store.startPairingFromAddress("sync");

    await store.submitPairingCode("ABCD-EFGH");

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.failure).toBe("insecure_context");
    expect(page.presentation.title).toBe("需要 HTTPS 才能配对");
    expect(page.error?.code).toBe("auth.insecure_context");
    expect(page.error?.retryable).toBe(false);
    expect(wiring.lookups).toHaveLength(0);
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("顺序（design D1）：安全上下文 → 载荷类型", async () => {
    // 非安全上下文 + 类型不符：报安全上下文（环境根本不满足，比「内容对不上」更根本）。
    const insecure = new RecordingPairingWiring();
    insecure.secure = false;
    insecure.readResult = { kind: "node-link", qr: NODE_LINK_QR };
    const insecureStore = storeWithPairing(insecure);
    await enterFromAddress(insecureStore, insecure, "node-link");
    expect(insecureStore.pairingPage().failure).toBe("insecure_context");

    // 设备载荷喂给 node-link 入口：安全上下文满足后按类型失败。
    const mismatched = new RecordingPairingWiring();
    const mismatchStore = storeWithPairing(mismatched);
    await enterFromAddress(mismatchStore, mismatched, "node-link");
    expect(mismatchStore.pairingPage().failure).toBe("kind_mismatch");
  });

  it("秘密不透传：失败文案与错误对象都不含配对秘密", async () => {
    const leakPaths: readonly ((wiring: RecordingPairingWiring) => void)[] = [
      (wiring) => {
        wiring.lookupQr = { ...QR, canonicalOrigin: "https://other-host.example.ts.net" };
      },
      (wiring) => {
        wiring.secure = false;
      },
    ];
    for (const configure of leakPaths) {
      const wiring = new RecordingPairingWiring();
      configure(wiring);
      const store = storeWithPairing(wiring);
      await enterByCode(store);
      const page = store.pairingPage();
      expect(page.error).not.toBeNull();
      expect(JSON.stringify(page)).not.toContain(QR_SECRET);
      expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    }

    // 片段路径同样不透传：q2 与旧 data= 的载荷都不进页面或公开状态。
    for (const fragment of [`q2=${QR_SECRET}`, `data=${QR_SECRET}`]) {
      const wiring = new RecordingPairingWiring();
      wiring.fragment = fragment;
      const store = storeWithPairing(wiring);
      await store.startPairingFromAddress("sync");
      expect(JSON.stringify(store.pairingPage())).not.toContain(QR_SECRET);
      expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    }

    // 异常文本同样不透传（既有的 `publicErrorForQrInput` 口径）。
    const failure = publicErrorForQrInput(new Error(`解析失败：${QR_SECRET}`));
    expect(failure.message).not.toContain(QR_SECRET);
    expect(JSON.stringify(failure)).not.toContain(QR_SECRET);
  });

  it("失败构造器取自封闭词表，归一化口径就是 §3.2 的 canonical origin", () => {
    expect(insecureContextFailure()).toMatchObject({
      code: "auth.insecure_context",
      retryable: false,
      details: {},
    });
    expect(originMismatchFailure()).toMatchObject({
      code: "auth.origin_mismatch",
      retryable: false,
      details: {},
    });
    expect(canonicalOriginOf("https://Host.Example:443/path?x#y")).toBe("https://host.example");
    expect(canonicalOriginOf("https://host.example:8443")).toBe("https://host.example:8443");
    expect(canonicalOriginOf("不是来源")).toBeNull();
  });
});

describe("配对页固定呈现（纯函数）", () => {
  it("初始状态是 reading 的进度，不显示倒计时", () => {
    for (const kind of ["sync", "node-link"] as const) {
      const model = initialPairingPageModel(kind);
      expect(model.phase).toBe("reading");
      expect(model.kind).toBe(kind);
      expect(model.presentation.kind).toBe("progress");
      expect(model.presentation.title).toBe("配对中");
      expect(model.presentation.countdown).toBe(false);
      expect(model.expiresAtMs).toBeNull();
      expect(model.host).toBeNull();
    }
  });

  it("无载荷按入口给固定文案", () => {
    const sync = applyNoPayload(initialPairingPageModel("sync"), "sync");
    expect(sync.phase).toBe("idle");
    expect(sync.failure).toBe("no_payload");
    expect(sync.presentation.kind).toBe("failure");
    expect(sync.presentation.title).toBe("这个地址里没有配对信息");
    expect(sync.presentation.tone).toBe("warning");

    const nodeLink = applyNoPayload(initialPairingPageModel("node-link"), "node-link");
    expect(nodeLink.presentation.title).toBe("这个地址里没有节点配对信息");
  });

  it("配对码输入态（S0）：显示来源与 HTTPS、只有静态提示、没有数字倒计时", () => {
    const model = applyAwaitingCode(initialPairingPageModel("sync"), "sync", ORIGIN, true);
    expect(model.phase).toBe("awaiting_code");
    expect(model.presentation.kind).toBe("code_entry");
    expect(model.presentation.title).toBe("输入配对码");
    expect(model.presentation.note).toBe("配对码 5 分钟内有效。");
    expect(model.presentation.countdown).toBe(false);
    expect(model.expiresAtMs).toBeNull();
    expect(model.origin).toBe(ORIGIN);
    expect(model.secureContext).toBe(true);
    expect(model.host).toBeNull();
  });

  it("lookup 在途（S1）：进度态，但仍不显示倒计时", () => {
    const awaiting = applyAwaitingCode(initialPairingPageModel(), "sync", ORIGIN, true);
    const lookingUp = applyLookingUp(awaiting);
    expect(lookingUp.phase).toBe("looking_up");
    expect(lookingUp.presentation.kind).toBe("progress");
    expect(lookingUp.presentation.countdown).toBe(false);
    expect(lookingUp.expiresAtMs).toBeNull();
    expect(lookingUp.origin).toBe(ORIGIN);
  });

  it("十类失败各有固定标题与说明，且不含载荷秘密", () => {
    const idle = initialPairingPageModel();
    const errorOf = (code: PublicError["code"]): PublicError => ({
      code,
      message: `后端消息 ${QR_SECRET}`,
      retryable: false,
      details: {},
    });
    const cases = [
      {
        name: "无法识别",
        model: applyPairingFailure(idle, errorOf("protocol.schema_invalid")),
        title: "无法识别这个地址",
        tone: "warning",
      },
      {
        name: "类型不匹配",
        model: applyPairingFailure(
          idle,
          kindMismatchFailure("sync", "node-link"),
          "kind_mismatch",
        ),
        title: "这不是本入口的配对地址",
        tone: "warning",
      },
      {
        name: "来源不符",
        model: applyPairingFailure(idle, errorOf("auth.host_mismatch"), "origin_mismatch"),
        title: "这个地址属于另一台电脑",
        tone: "warning",
      },
      {
        name: "主机身份变化",
        model: applyPairingFailure(idle, errorOf("auth.host_mismatch"), "host_identity_changed"),
        title: "这台电脑的主机身份已变化",
        tone: "warning",
      },
      {
        name: "非安全上下文",
        model: applyPairingFailure(idle, errorOf("auth.insecure_context"), "insecure_context"),
        title: "需要 HTTPS 才能配对",
        tone: "warning",
      },
      {
        name: "请求失败",
        model: applyPairingFailure(idle, errorOf("internal.unavailable")),
        title: "配对请求未能完成",
        tone: "warning",
      },
      {
        name: "配对码无效",
        model: applyPairingFailure(idle, codeInvalidFailure()),
        title: "配对码不对或已过期，请重新生成配对码",
        tone: "warning",
      },
    ];
    for (const scenario of cases) {
      expect(scenario.model.phase, scenario.name).toBe("failed");
      expect(scenario.model.failure, scenario.name).not.toBeNull();
      expect(scenario.model.presentation.title, scenario.name).toBe(scenario.title);
      expect(scenario.model.presentation.tone, scenario.name).toBe(scenario.tone);
      expect(scenario.model.presentation.lede, scenario.name).not.toContain(QR_SECRET);
      expect(scenario.model.presentation.warning, scenario.name).not.toBeNull();
    }

    // 终态：被拒绝用 danger；过期统一落到固定的 S6；已被使用保留自己的固定呈现。
    const terminal = (status: "rejected" | "expired" | "consumed"): PairingPageModel =>
      applyPollResult(idle, { kind: "terminal", status, error: errorOf("pairing.expired") });
    expect(terminal("rejected").presentation.title).toBe("审批未通过");
    expect(terminal("rejected").presentation.tone).toBe("danger");
    expect(terminal("expired").phase).toBe("failed");
    expect(terminal("expired").failure).toBe("code_invalid");
    expect(terminal("expired").presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(terminal("consumed").presentation.title).toBe("这个配对地址已经被使用");
    for (const status of ["rejected", "expired", "consumed"] as const) {
      expect(terminal(status).presentation.lede).not.toContain(QR_SECRET);
    }
  });

  it("错误码分类：显式 reason 优先，其余按词表归类", () => {
    const idle = initialPairingPageModel();
    const error = (code: PublicError["code"]): PublicError => ({
      code,
      message: "原因",
      retryable: false,
      details: {},
    });

    expect(applyPairingFailure(idle, error("auth.origin_mismatch")).failure).toBe("origin_mismatch");
    expect(applyPairingFailure(idle, error("auth.host_mismatch")).failure).toBe("origin_mismatch");
    expect(applyPairingFailure(idle, error("auth.insecure_context")).failure).toBe("insecure_context");
    expect(applyPairingFailure(idle, error("protocol.invalid_json")).failure).toBe("unrecognized");
    expect(applyPairingFailure(idle, error("internal.unavailable")).failure).toBe("request_failed");
    // 显式 reason 覆盖错误码分类（类型不匹配不会被混成 unrecognized）。
    expect(
      applyPairingFailure(idle, error("protocol.schema_invalid"), "kind_mismatch").failure,
    ).toBe("kind_mismatch");

    // `pairing.consumed` / `pairing.already_claimed` 走终态；`pairing.expired` 在配对码链路落到 S6
    // （node-link 入口的过期呈现另有一条用例，见「两类配对入口互不混淆」）。
    expect(applyPairingFailure(idle, error("pairing.expired")).phase).toBe("failed");
    expect(applyPairingFailure(idle, error("pairing.expired")).failure).toBe("code_invalid");
    expect(applyPairingFailure(idle, error("pairing.consumed")).phase).toBe("consumed");
    expect(applyPairingFailure(idle, error("pairing.already_claimed")).phase).toBe("consumed");
  });

  it("服务端过期统一到 S6：不把合成的 pairing.failed 呈现成「请求未能完成」", () => {
    const error: PublicError = {
      // §7.3：`expired` 也是 HTTP 200，HTTPS 传输层因此只能把码合成为 `pairing.failed`。
      // 用真实的合成码而不是 `pairing.expired`，这条用例才有判别力：按 `error.code` 分类的
      // 实现会落到「配对请求未能完成」（S12），而不是 S6。
      code: "pairing.failed" as PublicError["code"],
      message: "服务端说过期",
      retryable: false,
      details: {},
    };
    const terminalExpired: PairingPollResult = { kind: "terminal", status: "expired", error };

    const code = applyPollResult(
      applyLookingUp(applyAwaitingCode(initialPairingPageModel(), "sync", ORIGIN, true)),
      terminalExpired,
    );
    expect(code.phase).toBe("failed");
    expect(code.failure).toBe("code_invalid");
    expect(code.presentation.title).toBe("配对码不对或已过期，请重新生成配对码");
    expect(code.error?.code).toBe("pairing.code_invalid");
    // 服务端原因文本不进入固定文案。
    expect(code.presentation.lede).not.toContain("服务端说过期");
  });

  it("node-link 只呈现对端信息（info），没有倒计时", () => {
    const model = applyScannedNodeLink(initialPairingPageModel("node-link"), NODE_LINK_QR);
    expect(model.phase).toBe("node_link_scanned");
    expect(model.presentation.kind).toBe("info");
    expect(model.presentation.countdown).toBe(false);
    expect(model.expiresAtMs).toBeNull();
    expect(model.host).toEqual({ address: "https://work-pc.example.ts.net", hostId: HOST_ID });
  });

  it("已配对记录由已批准结果与二维码来源拼出（三字段缺一不可）", () => {
    expect(
      pairedHostRecordFor(
        { deviceId: DEVICE_ID, hostId: HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY },
        ORIGIN,
      ),
    ).toEqual({
      hostId: HOST_ID,
      publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY,
      canonicalOrigin: ORIGIN,
    });
  });

  it("传输层失败映射：来源不符是可读的专用码，其余落到可重试的通用码", () => {
    const mismatch = publicErrorForTransportFailure(new OriginMismatchError());
    expect(mismatch.code).toBe("auth.origin_mismatch");
    expect(mismatch.retryable).toBe(false);
    expect(mismatch.message).toContain("另一个来源");
    expect(mismatch.message).not.toContain("已绑定的规范化来源");
    expect(mismatch.details).toEqual({});

    const generic = publicErrorForTransportFailure(new Error("Failed to fetch"));
    expect(generic.code).toBe("internal.unavailable");
    expect(generic.retryable).toBe(true);
    expect(generic.details).toEqual({});
    expect(publicErrorForTransportFailure(null).code).toBe("internal.unavailable");
    expect(publicErrorForTransportFailure("boom").code).toBe("internal.unavailable");
  });

  it("收口失败只补原因，不把已经成功的配对退回失败（呈现仍是 approved）", () => {
    const approved = applyPollResult(initialPairingPageModel(), APPROVED);
    const withNotice = applyPairingCloseoutFailure(approved, publicErrorForCloseoutFailure(new Error("boom")));

    expect(withNotice.phase).toBe("approved");
    expect(withNotice.deviceId).toBe(DEVICE_ID);
    expect(withNotice.presentation.kind).toBe("progress");
    expect(withNotice.presentation.title).toBe("已通过审批，正在进入目录");
    expect(withNotice.error?.code).toBe("internal.unavailable");
    expect(withNotice.error?.retryable).toBe(true);
    expect(withNotice.error?.details).toEqual({});
    expect(withNotice.error?.message).toContain("配对已完成");
    expect(withNotice.error?.message).not.toContain("配对请求未能完成");
  });

  it("收口失败的文案与传输层通用文案分开，且不外溢平台错误原文", () => {
    const platformFailure = new Error("Failed to fetch");
    const closeout = publicErrorForCloseoutFailure(platformFailure);
    const generic = publicErrorForTransportFailure(platformFailure);

    expect(closeout.message).not.toBe(generic.message);
    expect(closeout.message).not.toContain("配对请求未能完成");
    expect(closeout.message).toContain("配对已完成");
    expect(closeout.message).toContain("同步连接未能建立");
    expect(closeout.message).not.toContain("Failed to fetch");
    expect(closeout.details).toEqual({});
    expect(closeout.code).toBe("internal.unavailable");
    expect(closeout.retryable).toBe(true);
  });
});
