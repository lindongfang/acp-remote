/**
 * WP1：两类配对入口（`/pair` 与 `/node-link/pair`）的浏览器级断言。
 *
 * 跑的是**真实页面**：`app/node-link/pair.tsx` / `app/pair.tsx` → 共用的配对入口主体 →
 * 真实 `ClientStore` → 生产组合根（片段读取与清除、结构校验、类型校验）。被替身顶掉的只有
 * 配对 HTTPS 面（`fetch`）：e2e 环境里 `https://` 来源不可达，因此它同时兼作「发过哪些请求」的账本。
 *
 * ## 这两条覆盖什么
 *
 * - **NL-1**：打开 node-link 二维码的地址（`/node-link/pair#data=…`）落到**可用的**配对入口
 *   ——阶段是「已读到 node-link 内容」（不是 `failed`、也不是没动过），对端地址/主机信息可读，
 *   输入可提交，且本客户端不为它发任何请求（claim/status 由节点发起，浏览器不是参与方）。
 * - **NL-2**：两类入口**互不混淆**——把 node-link 内容喂给 sync 入口、把 sync 内容喂给 node-link
 *   入口，两处都明确失败，且一个请求都不发。
 *
 * ## 判别力：每条断言在实现被回退时都会红
 *
 * - 删掉 `app/node-link/pair.tsx`（或把路径写错，例如放到 `node-link/pairing.tsx`）：
 *   `../fixtures` 对它的 import 解析不到 → 页面包构建失败 → 本文件两条都红。
 * - 去掉入口的类型校验（`#beginPairing` 不再比对 `read.kind` 与路由声明的类型）：sync 入口会把
 *   node-link 载荷交给认领流程（账本出现请求 → NL-2 红），node-link 入口会把 sync 载荷当己方内容
 *   呈现（阶段不是 `failed` → NL-2 红）。
 * - 只在路由里加一个空页面（不接 store）：`[data-pairing-entry]` 与对端地址都渲染不出来 → NL-1 红。
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
  click,
  expectAbsent,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import type { RouteEntry } from "../router-stub";
import type { FetchLike } from "../../../src/composition";
import { typeIntoInput } from "./r15-directory";

/** 配对页的三个锚点选择器（与 `pairing-first-run.ts` 同一口径）。 */
const PAIRING_SELECTOR = {
  entry: '[data-pairing-entry="true"]',
  input: "[data-pairing-input]",
  submit: '[data-action="submit-pairing-qr"]',
  phase: "[data-pairing-phase]",
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
    name: "NL-1 node-link 二维码打开的是可用配对入口：地址可读、输入可提交、一个请求都不发",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      const text = nodeLinkQrText(identity.publicKeyBase64Url);
      const http = recordingFetch();
      // 真实地址栏形态：二维码地址里的片段先挂到 `location.hash` 上，页面按它读到内容
      // （与扫码打开同一条路径：组合根读片段并立即清除）。
      globalThis.location.hash = text.slice(text.indexOf("#"));

      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, fetch: http.fetch, route: NODE_LINK_PAIR_ROUTE }),
      );
      try {
        await waitFor(
          () => query(mounted.container, PAIRING_SELECTOR.entry) !== null,
          "node-link 配对页渲染出配对入口",
        );

        // 落在**可用**的入口：已读到 node-link 内容（不是 `failed`，也不是停在 `idle` 什么都没发生）。
        expectEqual(phaseOf(mounted.container), "node_link_scanned", "node-link 二维码打开后的配对阶段");

        // 地址/主机信息可读：§13.1 的 HTTP 来源（endpoint 的 host 加 https）与 owner 节点标识。
        expectEqual(
          query(mounted.container, "[data-host-address]")?.getAttribute("data-host-address") ?? null,
          "https://work-pc.example.ts.net",
          "node-link 入口呈现的对端地址（取自 endpoint）",
        );

        // 输入可提交：输入框未被禁用，且输入非空后提交按钮可用（与 sync 入口同一套判定）。
        const input = query(mounted.container, PAIRING_SELECTOR.input) as HTMLInputElement | null;
        expectThat(input !== null, "node-link 配对入口必须有二维码内容输入框");
        expectEqual(input?.disabled ?? true, false, "读到 node-link 内容后输入框仍应可输入");
        expectEqual(
          (query(mounted.container, PAIRING_SELECTOR.submit) as HTMLButtonElement | null)?.disabled ?? false,
          true,
          "空输入时提交按钮必须禁用",
        );
        typeIntoInput(mounted.container, PAIRING_SELECTOR.input, text, "node-link 配对入口");
        await waitFor(
          () => (query(mounted.container, PAIRING_SELECTOR.submit) as HTMLButtonElement | null)?.disabled === false,
          "输入非空后提交按钮可用",
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
    name: "NL-2 两类入口互不混淆：交叉喂错内容都明确失败，且一个请求都不发",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      for (const scenario of CONFUSION_CASES) {
        const http = recordingFetch();
        // 这一条走**粘贴**入口：起点不能带着上一轮的片段（否则页面会先按片段读一次内容）。
        globalThis.location.hash = "";
        const mounted = await mountScenario(
          baseScenario({ seedPaired: false, fetch: http.fetch, route: scenario.route }),
        );
        try {
          await waitFor(
            () => query(mounted.container, PAIRING_SELECTOR.entry) !== null,
            `${scenario.name}：渲染出配对入口`,
          );
          // 起点：这一页还没有读到任何内容（地址栏里没有片段）。
          expectEqual(phaseOf(mounted.container), "idle", `${scenario.name}：入口的初始阶段`);

          typeIntoInput(
            mounted.container,
            PAIRING_SELECTOR.input,
            scenario.text(identity.publicKeyBase64Url),
            scenario.name,
          );
          click(query(mounted.container, PAIRING_SELECTOR.submit), `${scenario.name}：提交按钮`);

          await waitFor(
            () => phaseOf(mounted.container) === "failed",
            `${scenario.name}：明确失败（而不是误入另一类流程）`,
          );
          expectEqual(
            query(mounted.container, '[data-pairing-error="protocol.schema_invalid"]') !== null,
            true,
            `${scenario.name}：失败态带自己的码`,
          );
          // 文案说清「哪一类喂给了哪一类」；它不含载荷内容（一次性配对秘密不落呈现）。
          const message = textOf(query(mounted.container, "[data-pairing-error]"));
          expectThat(message.includes(scenario.foreign), `${scenario.name}：失败文案要指出喂错的是哪一类`);
          expectAbsent(message, PAIRING_SECRET, `${scenario.name}：失败文案不得回显配对秘密`);
          // 反例：去掉类型校验 → 一类内容会被另一类流程接手，这里会出现认领或状态查询请求。
          expectEqual(http.urls.length, 0, `${scenario.name}：一个配对请求都不发`);
        } finally {
          await mounted.unmount();
        }
      }
    },
  },
];

export const PAIRING_KIND_CHECKS = checks;
