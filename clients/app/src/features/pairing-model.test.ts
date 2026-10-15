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
 * - 「不停在等待扫码」：断言状态机落到 `disconnected`（已配对），而不是 `unpaired`。
 */

import { describe, expect, it } from "vitest";

import { ClientStore } from "./client-store";
import type { PairingPollResult, PairingTransport, PairingWiring } from "./pairing-model";
import {
  PAIRING_POLL_INTERVAL_MS,
  PairingQrInvalidError,
  applyPairingFailure,
  applyPollResult,
  initialPairingPageModel,
  isPairingInFlight,
  pairedHostRecordFor,
  publicErrorForQrInput,
} from "./pairing-model";
import type { PairedHostRecord } from "../sync-client";
import type { PairingQrPayload, PublicError } from "../protocol";

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

const APPROVED: PairingPollResult = {
  kind: "approved",
  deviceId: DEVICE_ID,
  hostId: HOST_ID,
  hostPublicKey: APPROVED_HOST_PUBLIC_KEY,
};

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
  readError: unknown = null;
  claimError: PublicError | null = null;
  results: PairingPollResult[] = [];
  writeError: unknown = null;
  #nonceCounter = 0;

  readonly transport: PairingTransport = {
    readQrPayload: (text: string): PairingQrPayload => {
      this.readText = text;
      if (this.readError !== null) throw this.readError;
      return QR;
    },
    claim: async (input: { readonly qr: PairingQrPayload; readonly canonicalOrigin: string }) => {
      this.order.push("claim");
      this.claims.push(input.qr);
      return this.claimError;
    },
    poll: async (input: { readonly qr: PairingQrPayload; readonly requestNonce: string }) => {
      this.order.push("poll");
      this.polls.push({ pairingId: input.qr.pairingId, nonce: input.requestNonce });
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

    await store.submitPairingQrText(JSON.stringify(QR));
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
    // 反例：抄二维码里的公钥、或只写 `deviceId` 的旧口径 → 这条断言变红。
    expect(QR.hostPublicKey).not.toBe(APPROVED_HOST_PUBLIC_KEY);
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

    await store.submitPairingQrText(JSON.stringify(QR));
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

    await store.submitPairingQrText(JSON.stringify(QR));

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("pairing.expired");
    expect(wiring.scheduled).toHaveLength(0);
    expect(wiring.written).toHaveLength(0);
  });

  it("已配对记录写不进去：明确失败且不触发连接（绝不伪造「已配对」）", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [APPROVED];
    wiring.writeError = new Error("QuotaExceededError");
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR));
    await wiring.runScheduledPoll();

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("internal.unavailable");
    expect(store.pairingPage().error?.message).toContain("QuotaExceededError");
    expect(wiring.onPairedRecords).toHaveLength(0);
    expect(store.state.connection.state).toBe("unpaired");
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

      await store.submitPairingQrText("{");

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

    await store.startPairingFromAddress();

    // 片段内容真的被交给了校验（真实实现读它就等于清掉它：`composition.test.ts` 钉这一条）。
    expect(wiring.readText).toBe(`data=${QR_SECRET}`);
    expect(store.pairingPage().phase).toBe("claiming");
    expect(wiring.claims).toHaveLength(1);

    // 再进一次页面不会重放：片段已经被读走（没有可读内容时不动作）。
    await store.startPairingFromAddress();
    expect(wiring.claims).toHaveLength(1);
    expect(wiring.readText).toBe(`data=${QR_SECRET}`);
  });

  it("没有片段时不动作：页面停在输入入口等粘贴", async () => {
    const wiring = new RecordingPairingWiring();
    const store = storeWithPairing(wiring);

    await store.startPairingFromAddress();

    expect(store.pairingPage().phase).toBe("idle");
    expect(wiring.claims).toHaveLength(0);
  });

  it("配对进行中重复提交被忽略：不会出现两条并行的 claim 与轮询", async () => {
    const wiring = new RecordingPairingWiring();
    wiring.results = [{ kind: "pending_confirmation" }];
    const store = storeWithPairing(wiring);

    await store.submitPairingQrText(JSON.stringify(QR));
    await store.submitPairingQrText(JSON.stringify(QR));

    expect(wiring.claims).toHaveLength(1);
    expect(wiring.scheduled).toHaveLength(1);
  });

  it("未装配配对链路：提交明确失败，而不是静默无操作", async () => {
    const store = storeWithPairing(undefined);

    await store.submitPairingQrText(JSON.stringify(QR));

    expect(store.pairingPage().phase).toBe("failed");
    expect(store.pairingPage().error?.code).toBe("internal.unavailable");
    expect(store.pairingPage().error?.message).toContain("配对链路未装配");
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
});
