/**
 * 配对链路（WP2）：配对页状态机 + 仓库的配对用例。
 *
 * ## 覆盖什么、不覆盖什么
 *
 * 覆盖：`PairingPageModel` 的阶段迁移与「进行中」判定、`ClientStore` 的配对用例
 * （`submitPairingQrText` / `startPairingFromAddress`）在**注入替身接线**下的完整行为——
 * 结构校验不过时明确失败且不发请求、已批准时用响应的主机材料写已配对记录、连接机的状态迁移、
 * 载荷（含 `pairingSecret`）不进公开状态。
 *
 * **不覆盖**：真实的 `fetch`、真实的地址栏（`location`/`history.replaceState`）、真实的本地缓存。
 * 那些属于 `src/composition.ts` 的装配面，由 `src/composition.test.ts` 用同一个仓库跑真装配来验。
 *
 * ## 判别力来源
 *
 * - 主机公钥取自**已批准响应**：二维码里的那份与响应里的那份刻意不同，抄错来源（或只取
 *   `deviceId` 的旧口径）会让「记录三字段」这条断言变红。
 * - 「不尽力解析」：`readQrPayload` 抛错时断言 claim / poll / 定时器三处计数都还是 0。
 * - 「两类入口互不混淆」：载荷类型与入口声明的类型不一致时，同样断言 claim / poll / 定时器
 *   三处计数为 0，且失败文案说清是哪一类喂给了哪一类（去掉类型校验 → 另一类流程接手 → 变红）。
 * - 「端点过形状正则但解析不了」：`wss://h:999999/…`、`wss://%zz/…` 这类端点会被校验层照收，
 *   因此断言入口呈现落到 `failed`（码 `protocol.schema_invalid`，有可执行的下一步）而不是抛异常
 *   或停在 `idle`（把 `new URL` 的解析放回求值中途 → 用例红）。
 * - 「不停在等待扫码」：断言状态机落到 `disconnected`（已配对），而不是 `unpaired`。
 */

import { describe, expect, it } from "vitest";

import { ClientStore } from "./client-store";
import type {
  PairingKind,
  PairingPollResult,
  PairingQrRead,
  PairingTransport,
  PairingWiring,
} from "./pairing-model";
import {
  PAIRING_POLL_INTERVAL_MS,
  PairingQrInvalidError,
  applyPairingCloseoutFailure,
  applyPairingFailure,
  applyPollResult,
  applyScannedNodeLink,
  initialPairingPageModel,
  isPairingInFlight,
  pairedHostRecordFor,
  publicErrorForCloseoutFailure,
  publicErrorForQrInput,
  publicErrorForTransportFailure,
} from "./pairing-model";
import type { PairedHostRecord } from "../sync-client";
import type { NodeLinkQrPayload, PairingQrPayload, PublicError } from "../protocol";

const ORIGIN = "https://work-pc.example.ts.net";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/** 一次性配对秘密（base64url-32 字节）：断言它不出现在任何公开状态里。 */
const QR_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** 二维码里的主机公钥（65 字节 base64url）。 */
const QR_HOST_PUBLIC_KEY = "BAcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

/** 状态响应里的主机公钥：与二维码里的那份**故意不同**，用来钉住材料的来源。 */
const APPROVED_HOST_PUBLIC_KEY = "BAYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgYGBgY";

const QR: PairingQrPayload = {
  pairingProtocol: "acp-remote-pairing-v1",
  hostId: HOST_ID,
  hostPublicKey: QR_HOST_PUBLIC_KEY,
  canonicalOrigin: ORIGIN,
  pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
  pairingSecret: QR_SECRET,
  expiresAt: "2026-10-18T00:10:00.000Z",
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

/** 配对接线替身：每一次调用都记账，并按脚本返回 claim / poll 的结果。 */
class RecordingPairingWiring implements PairingWiring {
  readonly claims: PairingQrPayload[] = [];
  readonly polls: { readonly pairingId: string; readonly nonce: string }[] = [];
  readonly written: PairedHostRecord[] = [];
  readonly onPairedRecords: PairedHostRecord[] = [];
  /** 调用顺序证据：记录必须先落盘，才允许收口到「已配对」。 */
  readonly order: string[] = [];
  readonly scheduled: (() => void)[] = [];
  readonly delays: number[] = [];
  /** 地址片段里待读取的内容；读一次即清空（真实实现是读取后立即清除）。 */
  fragment: string | null = null;
  /** 交给 `readQrPayload` 的原文：证明片段内容真的被读到了。 */
  readText: string | null = null;
  /** `readQrPayload` 的返回值（缺省是 sync 的 `QR`）：用来验「载荷类型与入口声明的类型不一致」。 */
  readResult: PairingQrRead | null = null;
  readError: unknown = null;
  claimError: PublicError | null = null;
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
      return this.readResult ?? { kind: "sync", qr: QR };
    },
    claim: async (input: { readonly qr: PairingQrPayload; readonly canonicalOrigin: string }) => {
      this.order.push("claim");
      this.claims.push(input.qr);
      if (this.claimThrows !== null) throw this.claimThrows;
      return this.claimError;
    },
    poll: async (input: { readonly qr: PairingQrPayload; readonly requestNonce: string }) => {
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
    this.scheduled.push(task);
    return () => {
      const index = this.scheduled.indexOf(task);
      if (index >= 0) this.scheduled.splice(index, 1);
    };
  }

  nextRequestNonce(): string {
    this.#nonceCounter += 1;
    return `nonce-${this.#nonceCounter}`;
  }

  /**
   * 触发一次已安排的轮询，并等它的异步链走完。
   *
   * 不引入真实时钟、也不猜时长：只连续让出微任务队列——被触发的链上全是 `async` 函数
   * （`poll` → 落盘 → 收口），没有需要等待的宏任务或定时器。
   */
  async runScheduledPoll(): Promise<void> {
    const task = this.scheduled.shift();
    task?.();
    for (let round = 0; round < 16; round += 1) await Promise.resolve();
  }
}

/** 一个只装配了配对链路的仓库：同步网关是空的——这条用例不派发任何命令。 */
function storeWithPairing(pairing: PairingWiring | undefined): ClientStore {
  return new ClientStore({
    sync: {
      current: null,
      authenticatedInfo: null,
      dispatch: () => undefined,
    },
    ids: { uuid: () => "00000000-0000-4000-8000-000000000001" },
    now: () => 1_700_000_000_000,
    identity: { connectionId: null },
    pairing,
  });
}

describe("配对链路：结构校验 → 认领 → 轮询 → 已配对", () => {
  it("合法内容：claim → 轮询 → 已配对；记录三字段，状态机不停在等待扫码", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [{ kind: "pending_confirmation" }, APPROVED];
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    expect(store.pairingPage().phase).toBe("claiming");
    // 还没批准：状态机不动（仍是未配对），也就不该拿去连接。
    expect(store.state.connection.state).toBe("unpaired");
    // 轮询按约定节奏安排：给一个窗口，而不是立刻打服务端。
    expect(wiring.delays).toEqual([PAIRING_POLL_INTERVAL_MS]);

    await wiring.runScheduledPoll();
    expect(store.pairingPage().phase).toBe("pending_confirmation");
    expect(store.pairingPage().awaitingDesktopConfirmation).toBe(true);
    expect(wiring.polls).toHaveLength(1);
    // 每次轮询换新 nonce（§7.3 的 schema 要求）：复用同一个会变红。
    expect(wiring.polls.map((poll) => poll.nonce)).toEqual(["nonce-1"]);

    await wiring.runScheduledPoll();

    // 2.2 记录三字段：主机标识、主机公钥（取自**已批准响应**）、规范化来源。
    // 反例：抄二维码里的公钥（`QR.hostPublicKey` 与下面那条断言里的 `APPROVED_HOST_PUBLIC_KEY`
    // 刻意不同）、或只写 `deviceId` 的旧口径 → 这条断言变红。
    expect(wiring.written).toEqual([
      { hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN },
    ]);
    // 落盘必须先于收口到「已配对」，且收口交给组合根去装配连接。
    expect(wiring.order).toEqual(["claim", "poll", "poll", "rememberPairedHost", "onPaired"]);
    expect(wiring.onPairedRecords).toEqual(wiring.written);
    // 第二次轮询换了 nonce（复用会被服务端当成重放）。
    expect(wiring.polls.map((poll) => poll.nonce)).toEqual(["nonce-1", "nonce-2"]);

    // 2.5 状态迁移：已配对 = `disconnected`（不是 `unpaired`，也没停在等待确认）。
    expect(store.state.connection.state).toBe("disconnected");
    expect(store.pairingPage().phase).toBe("approved");
    expect(store.pairingPage().deviceId).toBe(DEVICE_ID);
    expect(store.state.host).toEqual({ address: ORIGIN, hostId: HOST_ID, deviceId: DEVICE_ID });

    // 载荷用完即弃：秘密不出现在可导出、可快照的公开状态里。
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    // 终态之后不再轮询（也不再有定时器在等）。
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

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    expect(page.phase).toBe("failed");
    expect(page.error?.code).toBe("protocol.schema_invalid");
    expect(page.error?.message).toContain("主机身份材料");
    // 2.2 的负向：没有公钥就没有已配对记录——不许「先标记已配对、等连接时再补公钥」。
    expect(wiring.written).toHaveLength(0);
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
    expect(store.state.host).toBeNull();
    expect(wiring.scheduled).toHaveLength(0);
  });

  it("claim 被拒：落到失败阶段，不再轮询、也不写记录", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.claimError = { code: "pairing.expired", message: "这次配对二维码已过期。", retryable: false, details: {} };
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("pairing.expired");
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
  });

  it("claim **抛出**（设备身份绑在别的来源上）：会话结束、落到可读的失败态，且能立刻重试", async () => {
    const wiring = new RecordingPairingWiring();
    // 平台层的来源不符：`ensureIdentity` 抛 `origin_mismatch`（R11 场景 3，身份不得跨来源迁移）。
    wiring.claimThrows = new OriginMismatchError();
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");

    const page = store.pairingPage();
    // 反例：若 `claim` 的拒绝直接冒泡（没有收口），阶段会停在 `claiming` → 断言变红。
    expect(page.phase).toBe("failed");
    // 反例：若把抛出也当通用故障，这里会拿到 `internal.unavailable` → 断言变红。
    expect(page.error?.code).toBe("auth.origin_mismatch");
    expect(page.error?.retryable).toBe(false);
    // 平台错误的原文（可能带请求上下文）不透传进页面。
    expect(page.error?.message).not.toContain("已绑定的规范化来源");
    // 「进行中」必须结束：输入框据此解除禁用，页面出现「重新扫码」入口。
    // 反例：若会话没结束（`#pairingSession` 仍非 null），两个都会是 false → 断言变红。
    expect(isPairingInFlight(page)).toBe(false);
    expect(page.canRestart).toBe(true);
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");

    // 会话真的结束了：同一次页面会话里可以立刻重试（否则这次提交会被提前 return）。
    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    expect(wiring.claims).toHaveLength(2);
  });

  it("poll **抛出**（网络中断）：同样结束会话并落到可重试的失败态", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.pollThrows = new Error("Failed to fetch");
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    await wiring.runScheduledPoll();

    const page = store.pairingPage();
    // 反例：若 `poll` 的拒绝冒泡，阶段会停在 `pending_confirmation`（「进行中」恒真、
    // 输入框一直禁用、也没有任何重排）→ 断言变红。
    expect(page.phase).toBe("failed");
    expect(page.error?.code).toBe("internal.unavailable");
    expect(page.error?.retryable).toBe(true);
    expect(page.error?.message).not.toContain("Failed to fetch");
    expect(isPairingInFlight(page)).toBe(false);
    expect(page.canRestart).toBe(true);
    // 没有残留的定时器等待，也没有落盘任何记录。
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    expect(wiring.claims).toHaveLength(2);
  });

  it("已配对记录写不进去：明确失败且不触发连接（绝不伪造「已配对」）", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [APPROVED];
    wiring.writeError = new Error("QuotaExceededError");
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    await wiring.runScheduledPoll();

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("internal.unavailable");
    expect(store.pairingPage().error?.message).toContain("QuotaExceededError");
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
  });

  it("组合根收口时抛出（连接没起来）：记录仍算已配对，原因落到页面而不是未处理拒绝", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [APPROVED];
    wiring.onPairedError = new Error("Failed to fetch");
    const store = storeWithPairing(wiring);

    // 反例：若 `onPaired` 的拒绝直接冒泡，这一句 await 会抛出（未处理拒绝）→ 用例变红。
    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    await wiring.runScheduledPoll();

    // 已配对记录已经落盘、连接机已经推进——收口失败不回退这些结果。
    expect(wiring.written).toEqual([
      { hostId: HOST_ID, publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY, canonicalOrigin: ORIGIN },
    ]);
    expect(store.state.connection.state).toBe("disconnected");
    const page = store.pairingPage();
    // 反例：若收口失败按 `applyPairingFailure` 处理，阶段会变成 `failed`（与已写入的记录、
    // 主机面板将显示的身份自相矛盾）→ 这条与下一条都会变红。
    expect(page.phase).toBe("approved");
    expect(page.error?.code).toBe("internal.unavailable");
    expect(page.error?.retryable).toBe(true);
    expect(page.error?.details).toEqual({});
    // 反例：若收口路径复用通用文案，阶段是 `approved` 的这一页会同时出现引导语的
    // 「配对已完成，正在建立同步连接。」与原因句的「配对请求未能完成」——两句互相否定。
    expect(page.error?.message).toContain("配对已完成");
    expect(page.error?.message).toContain("同步连接未能建立");
    expect(page.error?.message).not.toContain("配对请求未能完成");
    expect(page.guidance).toContain("配对已完成");
    // 平台错误原文（这里注入的 `Failed to fetch`）不进文案，也不进 `details`。
    expect(page.error?.message).not.toContain("Failed to fetch");
    expect(JSON.stringify(page.error)).not.toContain("Failed to fetch");
  });
});

describe("配对链路：输入入口（粘贴 / 地址片段）", () => {
  it("非法内容明确失败且不发任何请求（四种失败形态都带自己的码）", async () => {
    const cases = [
      {
        name: "JSON 非法",
        error: new PairingQrInvalidError("protocol.invalid_json", "二维码内容不是 JSON 对象"),
        code: "protocol.invalid_json",
      },
      {
        name: "协议版本不符",
        error: new PairingQrInvalidError("protocol.schema_invalid", "二维码协议版本不匹配"),
        code: "protocol.schema_invalid",
      },
      {
        name: "字段缺失",
        error: new PairingQrInvalidError("protocol.schema_invalid", "二维码缺少字段 hostId"),
        code: "protocol.schema_invalid",
      },
    ] as const;

    for (const scenario of cases) {
      const wiring = new RecordingPairingWiring();
      wiring.readError = scenario.error;
      const store = storeWithPairing(wiring);

      await store.submitPairingQrText("{", "sync");

      expect(store.pairingPage().phase, scenario.name).toBe("failed");
      expect(store.pairingPage().error?.code, scenario.name).toBe(scenario.code);
      expect(store.pairingPage().canRestart, scenario.name).toBe(true);
      // 反例：尽力解析（补字段、放过版本）会走到 claim → 这三处的计数会不是 0。
      expect(wiring.claims, scenario.name).toHaveLength(0);
      expect(wiring.polls, scenario.name).toHaveLength(0);
      expect(wiring.scheduled, scenario.name).toHaveLength(0);
    }
  });

  it("任意异常的文本不透传：失败文案不含不可控内容（可能带着载荷片段）", () => {
    const error = publicErrorForQrInput(new Error(`解析失败：${QR_SECRET}`));
    expect(error.code).toBe("protocol.schema_invalid");
    expect(error.message).not.toContain(QR_SECRET);
    expect(error.details).toEqual({});
  });

  it("从地址片段进入认领流程：片段内容原样交给校验，且只读一次", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.fragment = `data=${QR_SECRET}`;
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    // 片段内容真的被交给了校验（真实实现读它就等于清掉它：`composition.test.ts` 钉这一条）。
    expect(wiring.readText).toBe(`data=${QR_SECRET}`);
    expect(store.pairingPage().phase).toBe("claiming");
    expect(wiring.claims).toHaveLength(1);

    // 再进一次页面不会重放：片段已经被读走（没有可读内容时不动作）。
    await store.startPairingFromAddress("sync");
    expect(wiring.claims).toHaveLength(1);
    expect(wiring.readText).toBe(`data=${QR_SECRET}`);
  });

  it("没有片段时不动作：页面停在输入入口等粘贴", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress("sync");

    expect(store.pairingPage().phase).toBe("idle");
    expect(wiring.claims).toHaveLength(0);
  });

  it("配对进行中重复提交被忽略：不会出现两条并行的 claim 与轮询", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");
    await store.submitPairingQrText(JSON.stringify(QR), "sync");

    expect(wiring.claims).toHaveLength(1);
    expect(wiring.scheduled).toHaveLength(1);
  });

  it("未装配配对链路：提交明确失败，而不是静默无操作", async () => {
    const store = storeWithPairing(undefined);

    await store.submitPairingQrText(JSON.stringify(QR), "sync");

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("internal.unavailable");
    expect(store.pairingPage().error?.message).toContain("配对链路未装配");
  });
});

describe("两类配对入口互不混淆（design D2/D3）", () => {
  /**
   * 载荷类型与**路由声明的**入口类型不一致：读载荷之后、发任何请求之前停住。
   *
   * `read` 是组合根会标注出来的读取结果（真实实现里由 `readQrPayload` 按 `pairingProtocol` 判定），
   * 这里直接给出两类的组合，因此用例判的是**入口**的取舍，而不是解析器。
   */
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
      name: "node-link 入口收到 sync 载荷",
      entry: "node-link",
      read: { kind: "sync", qr: QR },
      foreign: "设备（sync）",
    },
  ];

  for (const scenario of CASES) {
    it(`${scenario.name}：明确失败，且一个请求都不发`, async () => {
      const wiring = new RecordingPairingWiring();
      wiring.readResult = scenario.read;
      wiring.results = [{ kind: "pending_confirmation" }];
      const store = storeWithPairing(wiring);

      await store.submitPairingQrText("任意内容", scenario.entry);

      const page = store.pairingPage();
      expect(page.phase).toBe("failed");
      expect(page.error?.code).toBe("protocol.schema_invalid");
      // 文案说清「哪一类喂给了哪一类」，但不回显载荷内容（它可能含一次性配对秘密）。
      expect(page.error?.message).toContain(scenario.foreign);
      expect(page.error?.message).not.toContain(QR_SECRET);
      // 反例：去掉类型校验 → 另一类流程接手，这三处的计数不再是 0。
      expect(wiring.claims).toHaveLength(0);
      expect(wiring.polls).toHaveLength(0);
      expect(wiring.scheduled).toHaveLength(0);
      // 没有会话残留：换一段内容可以立刻重试。
      expect(isPairingInFlight(page)).toBe(false);
      expect(page.canRestart).toBe(true);
    });
  }

  it("node-link 载荷在 node-link 入口：只呈现公开信息，不发请求、不留秘密", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.readResult = { kind: "node-link", qr: NODE_LINK_QR };
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(`data=${QR_SECRET}`, "node-link");

    const page = store.pairingPage();
    // 已读到内容（不是 `failed`、也不是停在 `idle` 什么都没发生）。
    expect(page.phase).toBe("node_link_scanned");
    // §13.1：HTTP 来源由 `endpoint` 的 host 派生；对端标识是 owner 节点。
    expect(page.host).toEqual({ address: "https://work-pc.example.ts.net", hostId: HOST_ID });
    expect(page.error).toBeNull();
    // 本客户端不是 node-link 配对的参与方：一个请求都不发，也不安排轮询。
    expect(wiring.claims).toHaveLength(0);
    expect(wiring.polls).toHaveLength(0);
    expect(wiring.scheduled).toHaveLength(0);
    // 入口保持可用（输入仍在可用态），且含秘密的载荷不进公开状态。
    expect(isPairingInFlight(page)).toBe(false);
    expect(JSON.stringify(store.state)).not.toContain(QR_SECRET);
    // 再提交一次仍是同一结果：没有第二条会话，也不补发任何请求。
    await store.submitPairingQrText(`data=${QR_SECRET}`, "node-link");
    expect(store.pairingPage().phase).toBe("node_link_scanned");
    expect(wiring.claims).toHaveLength(0);
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

  it("端点过形状正则但派生不出地址：落到明确失败态，不抛异常、也不停在 idle", async () => {
    const idle = initialPairingPageModel();
    for (const endpoint of UNPARSABLE_ENDPOINTS) {
      // 前提证据：这些端点确实过得了形状正则（否则本用例判的就不是「校验层放行」这一幕）。
      expect(NODE_LINK_ENDPOINT_PATTERN.test(endpoint), endpoint).toBe(true);

      // 纯函数层：不抛异常，也不停在上一状态。
      const direct = applyScannedNodeLink(idle, { ...NODE_LINK_QR, endpoint });
      expect(direct.phase, endpoint).toBe("failed");
      expect(direct.error?.code, endpoint).toBe("protocol.schema_invalid");
      expect(direct.host, endpoint).toBeNull();
      // 有可执行的下一步（`canRestart`），因而与「停在 idle」可分。
      expect(direct.canRestart, endpoint).toBe(true);

      // 用例层：同一条路径（读载荷成功 → 入口呈现）——明确失败，且一个请求都不发。
      const wiring = new RecordingPairingWiring();
      wiring.readResult = { kind: "node-link", qr: { ...NODE_LINK_QR, endpoint } };
      const store = storeWithPairing(wiring);

      // 反例：解析放到 `#commit(...)` 的求值里 → 异常穿出 `submitPairingQrText`（这里会变红）。
      await store.submitPairingQrText(`data=${QR_SECRET}`, "node-link");

      const page = store.pairingPage();
      expect(page.phase, endpoint).toBe("failed");
      expect(page.error?.code, endpoint).toBe("protocol.schema_invalid");
      expect(page.error?.message.length ?? 0, endpoint).toBeGreaterThan(0);
      expect(isPairingInFlight(page), endpoint).toBe(false);
      expect(wiring.claims, endpoint).toHaveLength(0);
      expect(wiring.polls, endpoint).toHaveLength(0);
      expect(wiring.scheduled, endpoint).toHaveLength(0);
      // 含秘密的载荷不进公开状态。
      expect(JSON.stringify(store.state), endpoint).not.toContain(QR_SECRET);
    }
  });
});

describe("配对页状态机（纯函数）", () => {
  it("「进行中」只覆盖已读到内容、还没到终态的三态", () => {
    const idle = initialPairingPageModel();
    expect(isPairingInFlight(idle)).toBe(false);
    expect(isPairingInFlight(applyPollResult(idle, { kind: "pending_confirmation" }))).toBe(true);
    // 已批准与四种非成功终态都不是「进行中」：这一刻载荷已经（或必须）被丢掉。
    expect(isPairingInFlight(applyPollResult(idle, APPROVED))).toBe(false);
    expect(
      isPairingInFlight(
        applyPollResult(idle, {
          kind: "terminal",
          status: "expired",
          error: { code: "pairing.expired", message: "这次配对二维码已过期。", retryable: false, details: {} },
        }),
      ),
    ).toBe(false);
    expect(
      isPairingInFlight(
        applyPairingFailure(idle, { code: "pairing.expired", message: "已过期", retryable: false, details: {} }),
      ),
    ).toBe(false);
    // node-link 入口读到内容后**不**算进行中：它不持有会话、也不发请求，输入因此保持可用。
    expect(isPairingInFlight(applyScannedNodeLink(idle, NODE_LINK_QR))).toBe(false);
  });

  it("失败与终态都允许重来，且已批准的结果带上设备标识", () => {
    const idle = initialPairingPageModel();
    const failed = applyPairingFailure(idle, {
      code: "protocol.schema_invalid",
      message: "内容不是合法的配对二维码载荷。",
      retryable: false,
      details: {},
    });
    expect(failed.phase).toBe("failed");
    expect(failed.canRestart).toBe(true);
    expect(failed.guidance).not.toBe(idle.guidance);

    const approved = applyPollResult(idle, APPROVED);
    expect(approved.phase).toBe("approved");
    expect(approved.deviceId).toBe(DEVICE_ID);
    expect(approved.canRestart).toBe(false);
  });

  it("已配对记录由已批准结果与二维码来源拼出（三字段缺一不可）", () => {
    expect(pairedHostRecordFor({ deviceId: DEVICE_ID, hostId: HOST_ID, hostPublicKey: APPROVED_HOST_PUBLIC_KEY }, ORIGIN)).toEqual({
      hostId: HOST_ID,
      publicKeyBase64Url: APPROVED_HOST_PUBLIC_KEY,
      canonicalOrigin: ORIGIN,
    });
  });

  it("传输层失败映射：来源不符是可读的专用码，其余落到可重试的通用码", () => {
    const mismatch = publicErrorForTransportFailure(new OriginMismatchError());
    // 反例：若把 `origin_mismatch` 也归到通用分支，这条与下一条都会变红。
    expect(mismatch.code).toBe("auth.origin_mismatch");
    expect(mismatch.retryable).toBe(false);
    // 文案与错误码词表的建议动作同源（不是平台错误原文）。
    expect(mismatch.message).toContain("另一个地址");
    expect(mismatch.message).not.toContain("已绑定的规范化来源");
    expect(mismatch.details).toEqual({});

    // 反例：若把任意异常都当来源不符，这两条会变红（重试一次可能就好了，必须可重试）。
    const generic = publicErrorForTransportFailure(new Error("Failed to fetch"));
    expect(generic.code).toBe("internal.unavailable");
    expect(generic.retryable).toBe(true);
    expect(generic.details).toEqual({});
    // 非对象、null 等形状同样落通用分支，而不是抛错。
    expect(publicErrorForTransportFailure(null).code).toBe("internal.unavailable");
    expect(publicErrorForTransportFailure("boom").code).toBe("internal.unavailable");
  });

  it("收口失败只补原因，不把已经成功的配对退回失败", () => {
    const approved = applyPollResult(initialPairingPageModel(), APPROVED);
    const withNotice = applyPairingCloseoutFailure(approved, publicErrorForCloseoutFailure(new Error("boom")));

    // 反例：若收口失败走 `applyPairingFailure`，阶段会变成 `failed`（与已落盘的记录矛盾）→ 变红。
    expect(withNotice.phase).toBe("approved");
    expect(withNotice.deviceId).toBe(DEVICE_ID);
    expect(withNotice.error?.code).toBe("internal.unavailable");
    // 码 / 可重试性 / `details` 与通用分支一致，变的是措辞。
    expect(withNotice.error?.retryable).toBe(true);
    expect(withNotice.error?.details).toEqual({});
    // 反例：若收口路径继续用 `publicErrorForTransportFailure`，这句会变成「配对请求未能完成」→ 变红。
    expect(withNotice.error?.message).toContain("配对已完成");
    expect(withNotice.error?.message).not.toContain("配对请求未能完成");
    // 同一页的引导语（`approved`）与原因句现在说的是同一件事，不再互相否定。
    expect(withNotice.guidance).toContain("配对已完成");
  });

  it("收口失败的文案与传输层通用文案分开，且不外溢平台错误原文", () => {
    const platformFailure = new Error("Failed to fetch");
    const closeout = publicErrorForCloseoutFailure(platformFailure);
    const generic = publicErrorForTransportFailure(platformFailure);

    // 反例：若收口映射直接返回通用分支的那条文案，前三条断言都会变红。
    expect(closeout.message).not.toBe(generic.message);
    expect(closeout.message).not.toContain("配对请求未能完成");
    expect(closeout.message).toContain("配对已完成");
    expect(closeout.message).toContain("同步连接未能建立");
    // 平台错误原文一律不进入文案（`details` 也留空）：底层文本可能带着请求上下文。
    expect(closeout.message).not.toContain("Failed to fetch");
    expect(closeout.details).toEqual({});
    // 反例：若照 `publicErrorForTransportFailure` 的形状分类（来源不符 → 不可重试的专用码），
    // 这两条会变红——收口失败与「这次请求没能完成」同类，重试是有效的下一步。
    expect(closeout.code).toBe("internal.unavailable");
    expect(closeout.retryable).toBe(true);
  });
});
