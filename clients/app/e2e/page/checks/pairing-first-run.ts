/**
 * WP4：从「未配对」出发的**真实配对链路**（浏览器级）。
 *
 * 本目录其余的用例都从「已配对」起步（理由见 `../support.tsx` 的文件头）。这里的两条从
 * **未配对**起步，走的是生产代码：`app/pair.tsx` → `PairingPanel` 的粘贴入口 → 真实 DOM
 * 事件 → `ClientStore.submitPairingQrText` → 组合根的 `HttpPairingTransport`
 * （`/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status`）→ 已批准的主机材料写进真实
 * IndexedDB（`paired-host/<origin>`）→ 装配连接并触发连接 → 真实握手进入 `online`。
 *
 * 被替身顶掉的只有**传输**两头：配对 HTTPS 面（`fetch`，e2e 环境里 `https://` 来源不可达）
 * 与 socket（假主机）。配对状态机、结构校验、落盘、连接装配、握手验签、快照管线全部是生产代码。
 *
 * ## 判别力：每条断言在实现被回退时都会红
 *
 * - **PW-1**「落盘的主机公钥」取自**已批准响应**，而二维码里那一份是**另一把**密钥：
 *   把来源写错（抄二维码）→ 后续握手验签失败 → 连接进 `identity_changed` → 「连接进入
 *   online」红。整条链路的每一跳都有独立锚点：端点路径、认领正文里的设备材料、
 *   状态查询正文里的 `pairingRequestId` 与每次都换的 `requestNonce`、落盘摘要、出站 URL 路径。
 * - **PW-2**「配对入口」用**真实输入与真实点击**：删掉输入框（`[data-pairing-entry]`）→
 *   找不到元素红；把提交按钮的启用条件写死为可用（或与输入脱钩）→ 「输入非空后才可用」红；
 *   入口没有接到 store（点了什么都不发生）→ 「非法内容被明确拒绝」超时红。
 */

import {
  AGENTS,
  PAIR_ROUTE,
  WORKSPACES_WITH_PATHS,
  sessionsForBothDirectories,
} from "../fixtures";
import { createFakeHostIdentity } from "../fake-host";
import type { FakeHostIdentity } from "../fake-host";
import { encodeBase64Url } from "../../../src/sync-client";
import { openPlatform } from "../../../src/platform";
import {
  CANONICAL_ORIGIN,
  HOST_ID,
  click,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  seedDeviceIdentity,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import type { FetchLike, PairingHttpResponse } from "../../../src/composition";
import { typeIntoInput } from "./r15-directory";

/**
 * 配对页的三个锚点选择器。
 *
 * 每次现查而不是缓存元素：这些节点的可用性（禁用与否）本身就被断言。选择器只写一处，
 * 免得各调用点各写一份、慢慢走样。
 */
const PAIRING_SELECTOR = {
  entry: '[data-pairing-entry="true"]',
  input: "[data-pairing-input]",
  submit: '[data-action="submit-pairing-qr"]',
} as const;

/** 一次配对会话的固定标识（固定值才能逐字断言它进了请求正文）。 */
const PAIRING_ID = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";

/** 认领响应给出的配对请求标识（`poll` 的请求正文必须原样带上它，否则状态查询无法索引）。 */
const PAIRING_REQUEST_ID = "9f5b0d51-7c3a-4a2e-8b1d-5e6f708192a3";

/** 一次性配对秘密（32 字节 base64url，与 `src/composition.test.ts` 同口径）。 */
const PAIRING_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** 等待状态轮询的余量：配对节奏是 2 秒一档，两次查询之后再等重连与握手。 */
const PAIRING_WAIT_MS = 20_000;

/** 场景缺省：与 `composition-root.ts` 的 `baseScenario` 同口径，起点换成配对页。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    scopes: ["session.list", "session.read", "session.create"],
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: PAIR_ROUTE,
    ...overrides,
  };
}

/** 一次配对里 `claim` / `status` 的替身及其请求留痕。 */
interface PairingHttpFake {
  readonly fetch: FetchLike;
  /** 按次序记录的请求：端点 URL 与解析后的请求正文（断言「载荷取自哪里」）。 */
  readonly requests: { readonly url: string; readonly body: Record<string, unknown> }[];
}

/**
 * 配对 HTTPS 面的替身：认领回一个配对请求标识；状态**第一次**回「等待本机确认」、
 * **第二次**回「已批准」并带上主机身份材料。
 *
 * 两次状态而不是一次就批准：「请到电脑上确认」是用户真会看到的阶段（也是页面必须能渲染的
 * 一帧），一次就批准会让它在 e2e 里永远观测不到。
 *
 * 正文里的 `hostPublicKey` 由调用方给出，且**故意**与二维码里那一份不同——落盘必须取这里的
 * 那一份（§7.3：已批准响应携带 Host identity），取错来源会被 PW-1 的验签后果抓住。
 */
function pairingHttpFake(input: {
  readonly deviceId: string;
  readonly hostPublicKey: string;
}): PairingHttpFake {
  const requests: { url: string; body: Record<string, unknown> }[] = [];
  let statusPolls = 0;
  const json = (value: unknown, status: number): PairingHttpResponse => ({
    ok: true,
    status,
    json: () => Promise.resolve(value),
  });
  const fetchImpl: FetchLike = async (url, init) => {
    requests.push({ url, body: JSON.parse(init.body) as Record<string, unknown> });
    if (url.endsWith("/pairing/claim")) return json({ pairingRequestId: PAIRING_REQUEST_ID }, 201);
    statusPolls += 1;
    if (statusPolls === 1) return json({ status: "pending_confirmation" }, 200);
    return json(
      {
        status: "approved",
        device: { deviceId: input.deviceId },
        host: { hostId: HOST_ID, hostPublicKey: input.hostPublicKey },
      },
      200,
    );
  };
  return { fetch: fetchImpl, requests };
}

/** 只记账、不应答的配对面：用于断言「结构校验不过时一个请求都不发」。 */
function recordingFetch(): { readonly fetch: FetchLike; readonly urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: async (url) => {
      urls.push(url);
      return { ok: false, status: 500, json: () => Promise.resolve({}) };
    },
  };
}

/**
 * 用户粘贴的那段二维码内容：形式与协议 §7.1 的二维码一字不差
 * （`https://<origin>/pair#data=<base64url-json>`，秘密在**片段**里）。
 */
function pairingQrText(hostPublicKey: string): string {
  const payload = {
    pairingProtocol: "acp-remote-pairing-v1",
    hostId: HOST_ID,
    hostPublicKey,
    canonicalOrigin: CANONICAL_ORIGIN,
    pairingId: PAIRING_ID,
    pairingSecret: PAIRING_SECRET,
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
  };
  return `${CANONICAL_ORIGIN}/pair#data=${encodeBase64Url(new TextEncoder().encode(JSON.stringify(payload)))}`;
}

const checks: readonly Check[] = [
  {
    name: "PW-1 从「未配对」走真实配对链路：输入内容 → 认领 → 已批准 → 落盘主机材料 → 建立连接并在线",
    async run() {
      const device = await seedDeviceIdentity();
      // 二维码里那份主机公钥与「已批准」响应里那份**故意不同**：
      // 落盘必须取状态响应给出的主机材料，抄二维码的那一份会让后续握手验签失败。
      const qrIdentity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const hostIdentity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      expectThat(
        qrIdentity.publicKeyBase64Url !== hostIdentity.publicKeyBase64Url,
        "两把测试密钥的公钥必须不同，否则「落盘取了哪一份」无从区分",
      );
      const http = pairingHttpFake({
        deviceId: device.deviceId,
        hostPublicKey: hostIdentity.publicKeyBase64Url,
      });

      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity: hostIdentity, fetch: http.fetch }),
      );
      try {
        await waitFor(() => query(mounted.container, PAIRING_SELECTOR.entry) !== null, "配对页渲染出配对入口");

        // 起点真的是「未配对」：没有预置记录（否则后面每一步都可能在它上面走捷径）。
        expectEqual(
          mounted.composition?.store.state.connection.state ?? null,
          "unpaired",
          "未配对起点的连接状态",
        );
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "未配对时不得建立任何连接");
        expectEqual(
          query(mounted.container, "[data-pairing-phase]")?.getAttribute("data-pairing-phase") ?? null,
          "idle",
          "配对页的初始阶段",
        );

        // ① 入口可用：打字 → 提交（真实 DOM 事件，与用户操作同路径）。
        typeIntoInput(mounted.container, PAIRING_SELECTOR.input, pairingQrText(qrIdentity.publicKeyBase64Url), "配对入口");
        click(query(mounted.container, PAIRING_SELECTOR.submit), "配对提交按钮");

        // 认领：端点与载荷都取自**真实请求**。
        await waitFor(() => http.requests.length > 0, "发出认领请求");
        const claim = http.requests[0];
        expectEqual(claim?.url ?? null, `${CANONICAL_ORIGIN}/sync/v1/pairing/claim`, "认领请求的端点");
        expectEqual(claim?.body["pairingId"] ?? null, PAIRING_ID, "认领正文的配对标识（取自二维码）");
        expectEqual(claim?.body["hostId"] ?? null, HOST_ID, "认领正文的主机标识（取自二维码）");
        expectEqual(claim?.body["canonicalOrigin"] ?? null, CANONICAL_ORIGIN, "认领正文的规范化来源");
        // 设备材料必须来自平台身份：换成编造的 deviceId / 公钥，主机侧验签不可能通过。
        expectEqual(claim?.body["deviceId"] ?? null, device.deviceId, "认领正文的 deviceId（来自平台身份）");
        expectEqual(
          claim?.body["devicePublicKey"] ?? null,
          device.publicKeyBase64Url,
          "认领正文的设备公钥（来自平台身份）",
        );
        expectThat(
          typeof claim?.body["proof"] === "string" && (claim.body["proof"] as string).length >= 43,
          "认领正文必须带一段真实的 HMAC 证明",
        );

        // 进行中：输入被禁用（这一刻内存里持有一份含配对秘密的载荷，不再接受第二次提交）。
        await waitFor(
          () => (query(mounted.container, PAIRING_SELECTOR.input) as HTMLInputElement | null)?.disabled === true,
          "配对进行中禁用输入",
        );

        // ② 状态轮询：先「等待本机确认」，再「已批准」——两个阶段都必须由真实轮询推出来。
        await waitFor(
          () =>
            query(mounted.container, "[data-pairing-phase]")?.getAttribute("data-pairing-phase") ===
            "pending_confirmation",
          "状态查询进入「等待本机确认」",
          PAIRING_WAIT_MS,
        );
        // 「等待本机确认」这一刻页面上必须有两件事：二维码解析出来的主机地址，以及去电脑确认的提示。
        expectEqual(
          query(mounted.container, "[data-host-address]")?.getAttribute("data-host-address") ?? null,
          CANONICAL_ORIGIN,
          "等待确认阶段呈现的主机地址（取自二维码）",
        );
        expectThat(
          query(mounted.container, '[data-pairing-awaiting-desktop="true"]') !== null,
          "等待确认阶段必须提示用户到电脑上确认",
        );
        await waitFor(
          () =>
            query(mounted.container, "[data-pairing-phase]")?.getAttribute("data-pairing-phase") === "approved",
          "状态查询进入「已批准」",
          PAIRING_WAIT_MS,
        );

        const polls = http.requests.filter((request) => request.url.endsWith("/pairing/status"));
        expectEqual(polls.length, 2, "状态查询次数（一次待确认、一次已批准）");
        expectEqual(polls[0]?.url ?? null, `${CANONICAL_ORIGIN}/sync/v1/pairing/status`, "状态查询的端点");
        // 配对请求标识只在认领响应里出现过：正文里没有它，状态查询无从索引这次配对。
        expectEqual(
          polls[0]?.body["pairingRequestId"] ?? null,
          PAIRING_REQUEST_ID,
          "状态查询正文的配对请求标识（取自认领响应）",
        );
        // §7.3：每次轮询换新 nonce。写死或复用同一个 → 红。
        expectThat(
          typeof polls[0]?.body["requestNonce"] === "string" &&
            typeof polls[1]?.body["requestNonce"] === "string" &&
            polls[0].body["requestNonce"] !== polls[1].body["requestNonce"],
          "每次状态查询必须带一个新的 requestNonce",
        );
        // 已批准阶段：页面显示的就是状态响应给出的设备标识（不是本地编的）。
        expectEqual(
          query(mounted.container, "[data-pairing-device-id]")?.getAttribute("data-pairing-device-id") ?? null,
          device.deviceId,
          "已批准阶段呈现的设备标识",
        );

        // ③ 落盘的主机材料：键、主机标识与公钥都逐字来自「已批准」响应 + 规范化来源。
        const hit = await openPlatform().localCache.get(`paired-host/${CANONICAL_ORIGIN}`, Date.now());
        const summary = hit !== null && hit.entry.kind === "local" ? hit.entry.summary : null;
        expectThat(summary !== null, "已批准后必须写入 paired-host/<规范化来源> 记录");
        const stored = JSON.parse(summary ?? "{}") as Record<string, unknown>;
        expectEqual(stored["hostId"] ?? null, HOST_ID, "落盘记录的主机标识");
        expectEqual(
          stored["publicKeyBase64Url"] ?? null,
          hostIdentity.publicKeyBase64Url,
          "落盘记录的主机公钥（必须取自已批准响应，而不是二维码）",
        );

        // ④ 配对完成即触发连接：不是「停在等待扫码」，也不止一次装配。
        // `onPaired` 是异步收口（读设备身份后才装配），因此先等连接真的被建起来。
        await waitFor(
          () => (mounted.host?.socketUrls.length ?? 0) >= 1,
          "配对完成后建立连接",
          PAIRING_WAIT_MS,
        );
        // 在线意味着握手用**落盘的那把公钥**验签通过（取错来源或没落盘都会进阻断态）。
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "online",
          "配对后进入在线（握手用落盘的公钥验签通过）",
          PAIRING_WAIT_MS,
        );
        const socketUrls = mounted.host?.socketUrls ?? [];
        expectEqual(socketUrls.length, 1, "配对完成后恰好一条连接（无并行装配、无反复重连）");
        // 出站 URL 的路径：权威端点是**字面量** `/sync/v1`（不引用被测方的常量，
        // 否则常量被改回 `/sync` 时两边一起变、断言不会红）。
        const first = socketUrls[0];
        expectEqual(first === undefined ? null : new URL(first).pathname, "/sync/v1", "出站连接 URL 的路径");
        // 配对链路结束后输入回到可用（页面不在「进行中」）。
        await waitFor(
          () => (query(mounted.container, PAIRING_SELECTOR.input) as HTMLInputElement | null)?.disabled === false,
          "配对结束后输入恢复可用",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-2 配对入口的存在与可用性：空输入禁用提交、非空即启用、提交真的进 store 且非法内容不发请求",
    async run() {
      const http = recordingFetch();
      const mounted = await mountScenario(baseScenario({ seedPaired: false, fetch: http.fetch }));
      try {
        await waitFor(() => query(mounted.container, PAIRING_SELECTOR.entry) !== null, "配对页渲染出配对入口");

        // 存在性：入口、输入框、提交按钮三者都要在（删掉输入框即红）。
        const input = query(mounted.container, PAIRING_SELECTOR.input) as HTMLInputElement | null;
        expectThat(input !== null, "配对入口必须有二维码内容输入框");
        expectThat(query(mounted.container, PAIRING_SELECTOR.submit) !== null, "配对入口必须有提交按钮");
        // 可用性：空输入时输入框可输入、提交按钮禁用（把启用条件写死为「总是可用」即红）。
        expectEqual(input?.disabled ?? true, false, "空输入时输入框仍应可输入");
        expectEqual(
          (query(mounted.container, PAIRING_SELECTOR.submit) as HTMLButtonElement | null)?.disabled ?? false,
          true,
          "空输入时提交按钮必须禁用",
        );

        // 打字之后才可用：证明启用条件接的是**真实输入**，不是恒真/恒假。
        typeIntoInput(mounted.container, PAIRING_SELECTOR.input, "把这一段贴进来", "配对入口");
        await waitFor(
          () => (query(mounted.container, PAIRING_SELECTOR.submit) as HTMLButtonElement | null)?.disabled === false,
          "输入非空后提交按钮可用",
        );

        // 提交真的进了 store 的校验路径：非法内容被明确呈现为失败，而不是点了没反应。
        click(query(mounted.container, PAIRING_SELECTOR.submit), "配对提交按钮");
        await waitFor(
          () => query(mounted.container, '[data-pairing-error="protocol.schema_invalid"]') !== null,
          "非法内容被明确拒绝",
        );
        expectEqual(
          query(mounted.container, "[data-pairing-phase]")?.getAttribute("data-pairing-phase") ?? null,
          "failed",
          "非法内容的配对阶段",
        );
        // 不尽力解析：结构校验不过时一个配对请求都不发。
        expectEqual(http.urls.length, 0, "结构校验不过时不得发出任何配对请求");
        // 粘进来的一次性的秘密不留在这个组件里。
        expectEqual(
          (query(mounted.container, PAIRING_SELECTOR.input) as HTMLInputElement | null)?.value ?? null,
          "",
          "提交后输入框必须被清空",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const PAIRING_FIRST_RUN_CHECKS = checks;
