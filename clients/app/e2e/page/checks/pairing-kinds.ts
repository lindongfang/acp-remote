/**
 * 两类配对入口（`/pair` 与 `/node-link/pair`）的浏览器级断言。
 *
 * 跑的是**真实页面**：`app/node-link/pair.tsx` / `app/pair.tsx` → 共用的配对页主体 →
 * 真实 `ClientStore` → 生产组合根（片段读取与清除、结构校验、环境前置条件、类型校验）。
 * 起点一律是「二维码地址的片段挂到 `location.hash` 上再挂载页面」——本文件验的是**二维码**
 * 入口的类型区分；`/pair` 无片段时的配对码输入由 `pairing-first-run.ts` 的 PW-2/PW-6 覆盖。
 * 被替身顶掉的只有配对 HTTPS 面（`fetch`）：e2e 环境里 `https://` 来源不可达，因此它同时
 * 兼作「发过哪些请求」的账本。
 *
 * ## 这几条覆盖什么
 *
 * - **NL-1**：打开 node-link 二维码的地址（`/node-link/pair#data=…`）落到**只呈现对端信息**的
 *   info 状态——阶段是「已读到 node-link 内容」（不是 `failed`、也不是没动过），对端地址可读，
 *   不显示倒计时，且本客户端不为它发任何请求（claim/status 由节点发起，浏览器不是参与方）；
 *   片段读取后被清除。
 * - **NL-2**：两类入口**互不混淆**——把 node-link 内容喂给 sync 入口、把 sync 内容喂给 node-link
 *   入口，两处都明确失败（`kind_mismatch` + 固定标题「这不是本入口的配对地址」），文案指出被喂入
 *   的类别名，且一个请求都不发。
 * - **NL-3**：环境前置条件——非安全上下文与来源不符各自落到 `insecure_context` /
 *   `origin_mismatch` 的固定呈现，一个请求都不发。
 *
 * ## 判别力：每条断言在实现被回退时都会红
 *
 * - 删掉 `app/node-link/pair.tsx`（或把路径写错，例如放到 `node-link/pairing.tsx`）：
 *   `../fixtures` 对它的 import 解析不到 → 页面包构建失败 → 本文件全部红。
 * - 去掉入口的类型校验（`#beginPairing` 不再比对 `read.kind` 与路由声明的类型）：sync 入口会把
 *   node-link 载荷交给认领流程（账本出现请求 → NL-2 红），node-link 入口会把 sync 载荷当己方内容
 *   呈现（阶段不是 `failed` → NL-2 红）。
 * - 只在路由里加一个空页面（不接 store）：`[data-pairing-entry]` 与对端地址都渲染不出来 → NL-1 红。
 * - 把「读到 node-link 内容」实现成失败或输入入口：NL-1 的阶段/锚点断言立刻红。
 */

import {
  AGENTS,
  NODE_LINK_PAIR_ROUTE,
  PAIR_ROUTE,
  WORKSPACES_WITH_PATHS,
  sessionsForBothDirectories,
} from "../fixtures";
import { createFakeHostIdentity } from "../fake-host";
import { encodeBase64Url } from "../../../src/sync-client";
import {
  CANONICAL_ORIGIN,
  HOST_ID,
  domText,
  expectAbsent,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import type { RouteEntry } from "../router-stub";
import type { FetchLike } from "../../../src/composition";

/** 配对页的锚点选择器（与 `pairing-first-run.ts` 同一口径）。 */
const PAIRING_SELECTOR = {
  root: '[data-route="pair"]',
  entry: '[data-pairing-entry="true"]',
  phase: "[data-pairing-phase]",
  countdown: '[data-pairing-countdown="true"]',
} as const;

/** 一次配对会话的固定标识（固定值才能逐字断言它没进任何请求正文）。 */
const PAIRING_ID = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";

/** 一次性配对秘密（32 字节 base64url）。 */
const PAIRING_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** §13.1 的 node-link 端点：它决定呈现出来的 HTTP 来源（host + https）。 */
const NODE_LINK_ENDPOINT = "wss://work-pc.example.ts.net/node-link/v1";

/** 场景缺省：与 `pairing-first-run.ts` 的 `baseScenario` 同口径。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: PAIR_ROUTE,
    // 环境事实（design D2）：e2e 页面由 http 提供、二维码夹具的 `canonicalOrigin` 是 `https://…`；
    // 注入「安全上下文 + 来源等于夹具来源」让两条前置校验成立，NL-1/NL-2 才判得到它们各自要判的
    // 那一幕（读到 node-link / 类型互不混淆），而不是先被环境前置条件拦下。
    secureContext: () => true,
    origin: () => CANONICAL_ORIGIN,
    ...overrides,
  };
}

/** 只记账、不应答的配对面：用来断言「一个配对请求都没发」。 */
function recordingFetch(): { readonly fetch: FetchLike; readonly urls: string[] } {
  const urls: string[] = [];
  const fetch: FetchLike = async (url) => {
    urls.push(url);
    return { ok: false, status: 500, json: () => Promise.resolve({}) };
  };
  return { urls, fetch };
}

/** 当前配对阶段（`data-pairing-phase`）；没渲染出来时返回 `null`。 */
function phaseOf(container: HTMLElement): string | null {
  return query(container, PAIRING_SELECTOR.phase)?.getAttribute("data-pairing-phase") ?? null;
}

/**
 * 打开配对地址：把二维码地址的 `#…` 部分挂到 `location.hash` 上，再挂载页面。
 *
 * 二维码路径的入口是地址片段（design D1），与扫码打开同一条路径（组合根读片段并立即清除）。
 */
function openPairingAddress(text: string): void {
  globalThis.location.hash = text.slice(text.indexOf("#"));
}

/**
 * 一段 node-link 配对二维码内容（`NODE_LINK_PROTOCOL.md` §13.1 的地址形态，秘密在片段里）。
 *
 * 地址里的来源刻意用 `CANONICAL_ORIGIN`（与 endpoint 的 host 不同）：NL-1 因此能证明入口呈现的
 * 对端地址取自**载荷的 `endpoint`**，而不是从二维码地址本身抄来的。
 */
function nodeLinkQrText(ownerPublicKey: string): string {
  const payload = {
    pairingProtocol: "acp-remote-nodelink-v1",
    ownerNodeId: HOST_ID,
    ownerPublicKey,
    endpoint: NODE_LINK_ENDPOINT,
    pairingId: PAIRING_ID,
    pairingSecret: PAIRING_SECRET,
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
  };
  return `${CANONICAL_ORIGIN}/node-link/pair#data=${encodeBase64Url(
    new TextEncoder().encode(JSON.stringify(payload)),
  )}`;
}

/** 一段 sync 设备配对二维码内容（`SYNC_PROTOCOL.md` §7.1 的地址形态）。 */
function syncQrText(hostPublicKey: string): string {
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

/**
 * 两类「喂错内容」的组合：入口的期望类型 + 喂进去的**另一类**二维码内容。
 *
 * `foreign` 是失败文案里应当出现的那一类名字（文案说清「哪一类喂给了哪一类」，但不回显载荷）。
 */
const CONFUSION_CASES: readonly {
  readonly name: string;
  readonly route: RouteEntry;
  readonly text: (key: string) => string;
  readonly foreign: string;
}[] = [
  {
    name: "sync 入口收到 node-link 载荷",
    route: PAIR_ROUTE,
    text: nodeLinkQrText,
    foreign: "node-link",
  },
  {
    name: "node-link 入口收到 sync 载荷",
    route: NODE_LINK_PAIR_ROUTE,
    text: syncQrText,
    foreign: "设备（sync）",
  },
];

const checks: readonly Check[] = [
  {
    name: "NL-1 node-link 二维码打开的是只读的对端信息页：地址可读、不显示倒计时、一个请求都不发",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      const text = nodeLinkQrText(identity.publicKeyBase64Url);
      const http = recordingFetch();
      openPairingAddress(text);

      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, fetch: http.fetch, route: NODE_LINK_PAIR_ROUTE }),
      );
      try {
        await waitFor(
          () => query(mounted.container, PAIRING_SELECTOR.entry) !== null,
          "node-link 配对页渲染出来（data-pairing-entry 锚点）",
        );
        await waitFor(
          () => phaseOf(mounted.container) === "node_link_scanned",
          "读到 node-link 内容（不是 failed、也不是没动过）",
        );

        // 地址/主机信息可读：§13.1 的 HTTP 来源（endpoint 的 host 加 https）与 owner 节点标识。
        expectEqual(
          query(mounted.container, "[data-host-address]")?.getAttribute("data-host-address") ?? null,
          "https://work-pc.example.ts.net",
          "node-link 入口呈现的对端地址（取自 endpoint）",
        );
        // 浏览器不是 node-link 配对的参与方：不显示 sync 的审批窗口倒计时。
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.countdown),
          null,
          "node-link 入口不得显示倒计时",
        );
        expectThat(
          query(mounted.container, "[data-pairing-error]") === null,
          "node-link 内容不该在它自己的入口上失败",
        );

        // 片段被读出并被清除（§7.1），且本客户端不为 node-link 载荷发任何请求。
        expectEqual(globalThis.location.hash, "", "node-link 二维码的片段读取后被清除");
        expectEqual(http.urls.length, 0, "node-link 载荷不得发出任何配对请求");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "NL-2 两类入口互不混淆：交叉喂错内容都明确失败（kind_mismatch），且一个请求都不发",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      for (const scenario of CONFUSION_CASES) {
        const http = recordingFetch();
        // 片段是该入口收到的**另一类**载荷：页面读它、校验类型，然后在发请求之前停住。
        openPairingAddress(scenario.text(identity.publicKeyBase64Url));
        const mounted = await mountScenario(
          baseScenario({ seedPaired: false, fetch: http.fetch, route: scenario.route }),
        );
        try {
          await waitFor(
            () => query(mounted.container, '[data-pairing-failure="kind_mismatch"]') !== null,
            `${scenario.name}：明确失败（而不是误入另一类流程）`,
          );
          expectEqual(phaseOf(mounted.container), "failed", `${scenario.name}：失败阶段`);

          const root = query(mounted.container, PAIRING_SELECTOR.root);
          expectThat(root !== null, `${scenario.name}：失败页根节点必须存在`);
          // 固定标题 + 类别名：文案说清「哪一类喂给了哪一类」，但不回显载荷内容。
          const panelText = domText(root);
          expectThat(
            panelText.includes("这不是本入口的配对地址"),
            `${scenario.name}：必须呈现固定标题「这不是本入口的配对地址」`,
          );
          expectThat(
            panelText.includes(scenario.foreign),
            `${scenario.name}：文案要指出喂入的是哪一类（${scenario.foreign}）`,
          );
          expectAbsent(panelText, PAIRING_SECRET, `${scenario.name}：失败文案不得回显配对秘密`);

          // 反例：去掉类型校验 → 一类内容会被另一类流程接手，这里会出现认领或状态查询请求。
          expectEqual(http.urls.length, 0, `${scenario.name}：一个配对请求都不发`);
          expectEqual(globalThis.location.hash, "", `${scenario.name}：片段读取后被清除`);
        } finally {
          await mounted.unmount();
        }
      }
    },
  },
  /**
   * 环境前置条件（`SYNC_PROTOCOL.md` §7.1 的「当前 origin 与 secure context」）：
   * 只有把环境事实造出来才验得到——页面由 http 提供、二维码夹具是 https 来源，生产默认两条
   * 都成立，因此这里注入替身把两条各破一次。起点同样是地址片段。
   */
  {
    name: "NL-3 环境前置条件：非安全上下文与来源不符都明确失败，且一个请求都不发",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      const cases: readonly {
        readonly name: string;
        readonly env: Partial<ScenarioOptions>;
        readonly code: string;
        readonly failure: string;
        readonly title: string;
      }[] = [
        {
          name: "非安全上下文",
          env: { secureContext: () => false },
          code: "auth.insecure_context",
          failure: "insecure_context",
          title: "需要 HTTPS 才能配对",
        },
        {
          name: "来源不符",
          env: { origin: () => "https://other-host.example.ts.net" },
          code: "auth.origin_mismatch",
          failure: "origin_mismatch",
          title: "这个地址属于另一台电脑",
        },
      ];

      for (const scenario of cases) {
        const http = recordingFetch();
        // 走**地址片段**入口：与扫码打开同一条路径，页面读完载荷后先判环境事实。
        openPairingAddress(syncQrText(identity.publicKeyBase64Url));
        const mounted = await mountScenario(
          baseScenario({ seedPaired: false, fetch: http.fetch, route: PAIR_ROUTE, ...scenario.env }),
        );
        try {
          // 明确失败、且失败态带自己的码与固定呈现（两条前置条件各自可区分）。
          await waitFor(
            () => query(mounted.container, `[data-pairing-failure="${scenario.failure}"]`) !== null,
            `${scenario.name}：明确失败（而不是误入认领流程）`,
          );
          expectEqual(phaseOf(mounted.container), "failed", `${scenario.name}：失败阶段`);
          expectThat(
            query(mounted.container, `[data-pairing-error="${scenario.code}"]`) !== null,
            `${scenario.name}：失败态带自己的码`,
          );

          const root = query(mounted.container, PAIRING_SELECTOR.root);
          expectThat(root !== null, `${scenario.name}：失败页根节点必须存在`);
          // 固定文案在标题/说明里；`data-pairing-error` 只是一个空的诊断码节点。
          const panelText = domText(root);
          expectThat(
            panelText.includes(scenario.title),
            `${scenario.name}：必须呈现固定标题「${scenario.title}」`,
          );
          // 文案不回显载荷内容（一次性配对秘密不得落呈现）。
          expectAbsent(panelText, PAIRING_SECRET, `${scenario.name}：失败文案不得回显配对秘密`);
          // 反例：去掉任一条环境前置判定 → 会走到认领，这里就会出现请求。
          expectEqual(http.urls.length, 0, `${scenario.name}：一个配对请求都不发`);
          expectEqual(globalThis.location.hash, "", `${scenario.name}：片段读取后被清除`);
        } finally {
          await mounted.unmount();
        }
      }
    },
  },
];

export const PAIRING_KIND_CHECKS = checks;
