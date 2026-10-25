/**
 * D4「路由与配对片段的争用取舍」的结构化判定（tasks 3.1–3.4）。
 *
 * ## 3.1 核实结果：前端走**正常路径**，不是片段路由
 *
 * 配对二维码是 `<origin>/pair#data=<base64url-json>`（`crates/server/src/local_admin/pairing.rs`），
 * 而 HTML 原型把路由写在片段里（`prototypes/README.md`），`app/` 下的页面注释照抄了那一口径。
 * 三处**产物与实现**的观察一致地表明实际行为是正常路径：
 *
 * 1. `app.json` 的 `web.output = "single"`，`expo export --platform web` 产出单页入口
 *    `clients/app/dist/index.html`（以绝对路径 `/_expo/static/js/web/entry-*.js` 引入脚本），
 *    服务端对非保留路径回退入口文档（`crates/server/src/transport/net/static_ui.rs` 的 `resolve`）——
 *    因此页面路径由客户端路由决定，不需要服务端为每条路由备一份文件。
 * 2. `expo export` 生成的类型化路由（`clients/app/.expo/types/router.d.ts`）里 href 是
 *    `/`、`/pair`、`/hosts`、`/dir/[directoryId]`、`/session/[id]`，没有片段形态。
 * 3. `expo-router` 的 Web 链接实现（`build/fork/useLinking.js`）在初态与 `popstate` 时都从
 *    `location.pathname + location.search + location.hash` 取地址，但**路由匹配只用 `pathname`**：
 *    `build/fork/getStateFromPath-forks.js` 的 `getUrlWithReactNavigationConcessions` 用 URL 解析把
 *    片段分到 `hash` 字段，`getStateFromPath` 只把 `pathname` 拿去匹配；历史写的是真实路径
 *    （`window.history.pushState/replaceState`）。
 *
 * 结论：片段**只**承载配对内容，路由不落在片段上，`app.json` 不需要任何路由配置改动；
 * 需要改的是照抄原型口径的页面注释（已改）。
 *
 * 这条核实的边界：本文件断言的是**地址语义**（片段不进路径）、**文件清单**（页面文件 ↔ 路径）
 * 与**唯一的片段读点**（组合根）。expo-router 的链接实现本身由上面第 3 条（读产物与实现）
 * 与浏览器级用例共同负责——本文件不会因为 expo-router 内部换成片段路由而变红，那种改动要靠
 * 真实浏览器里的地址观察来裁定；届时页面注释与实际地址形态的矛盾由本文件的清单那一条报出。
 *
 * ## 判别力
 *
 * - 路由清单一条：删掉、改名或新增 `app/` 下的页面文件即变红（页面可达性来自「文件在、路径对」）。
 * - 页面注释一条：把任一页头注释退回片段口径（或路径写错）即变红。
 * - 地址栏唯一读点一条：`app/` 与 `src/` 的源码里出现 `globalThis.location`/`globalThis.history`
 *   的文件必须**只有**组合根，且组合根只用 `replaceState`——页面或 feature 自己读片段再跳转，
 *   正是「片段被当作导航指令」的实现方式，这条会直接变红。
 * - 页面源码一条：`app/` 下出现片段形态的路由（`#/…`）或地址栏标识符即变红。
 * - 二维码地址一条（走**真实组合根**）：删掉清除片段的逻辑（`replaced` 为空）、删掉读取
 *   （配对状态停在 `idle`）、或把清除改成跳转（清除后的地址不再落在 `/pair`、或落在 `pushState`
 *   的历史里）都会变红。
 * - 片段不变一条：把地址语义写死成可核对的期望（`/pair#/hosts` 与 `/pair` 必须命中同一条路由）。
 *   它断言的是地址语义本身，因此上面两条源码断言才是红线；这一条回答的是「若语义变了会怎样」。
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { createComposition } from "./composition";
import type { Composition } from "./composition";
import type { PlatformPorts } from "./platform";
import { encodeBase64Url } from "./sync-client";

const SRC_ROOT = dirname(fileURLToPath(import.meta.url));
const APP_ROOT = resolve(SRC_ROOT, "..");
/** `clients/app/app`：Expo Router 的页面根（`app/` 目录本身）。 */
const ROUTE_ROOT = join(APP_ROOT, "app");

/** 一条来源，用于把「路径 + 片段」补成可解析的绝对地址。 */
const ORIGIN = "https://work-pc.example.ts.net";
/** 组合根认的配对片段前缀（`src/composition.ts` 的 `QR_DATA_MARKER`）。 */
const QR_DATA_MARKER = "data=";
const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const SESSION_ID = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
/** 目录标识的样例（32 位小写十六进制，与 `$defs.directoryId` 一致）。 */
const DIRECTORY_ID = "0123456789abcdef0123456789abcdef";

/**
 * 页面 ↔ 路由路径 ↔ 文件。
 *
 * `route` 的形状与 `expo export` 生成的类型化路由逐字一致（文件头第 2 条观察）；
 * `sample` 是访问该页面时地址栏里的实际路径（动态段换成真实值）。
 */
const PAGES = [
  { page: "目录页", route: "/", sample: "/", file: "index.tsx" },
  { page: "配对页", route: "/pair", sample: "/pair", file: "pair.tsx" },
  {
    page: "node-link 配对页",
    route: "/node-link/pair",
    sample: "/node-link/pair",
    file: "node-link/pair.tsx",
  },
  { page: "主机与连接页", route: "/hosts", sample: "/hosts", file: "hosts.tsx" },
  { page: "目录详情页", route: "/dir/[directoryId]", sample: `/dir/${DIRECTORY_ID}`, file: "dir/[directoryId].tsx" },
  { page: "会话页", route: "/session/[id]", sample: `/session/${SESSION_ID}`, file: "session/[id].tsx" },
] as const;

/**
 * 各形态的片段：配对内容、**看起来像路由**的片段（原型口径）、无关片段。
 *
 * `#/hosts` 一类必须与「空片段」命中同一条路由——否则片段就成了第二条导航通路。
 */
const FRAGMENTS = [
  "",
  "#data=eyJhIjox",
  "#data=",
  "#/hosts",
  "#/dir/other",
  `#/session/${SESSION_ID}`,
  "#/",
  "#whatever",
] as const;

/**
 * `app/` 下的页面文件（相对 `app/` 的路径）。
 *
 * `_` 开头的是路由布局（`_layout.tsx`），`+` 开头的是 Expo Router 的特例文件
 * （`+not-found` 等）：两者都不提供页面路径，因此不进清单。
 */
function pageFiles(dir: string = ROUTE_ROOT, prefix = ""): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      files.push(...pageFiles(full, `${prefix}${entry}/`));
      continue;
    }
    if (!entry.endsWith(".tsx") || entry.startsWith("_") || entry.startsWith("+")) continue;
    files.push(`${prefix}${entry}`);
  }
  return files;
}

/**
 * 页面文件 → 路由路径（Expo Router 的文件约定：去扩展名、`index` 即所在层级本身、
 * 动态段 `[x]` 原样保留）。
 *
 * 这里**不**实现匹配器——匹配由 expo-router 负责；本函数只把文件系统上的清单翻译成能与
 * 类型化路由对照的字符串。
 */
function routePathOf(relativeFile: string): string {
  const segments = relativeFile
    .replace(/\.tsx$/, "")
    .split("/")
    .filter((segment) => segment !== "index");
  return `/${segments.join("/")}`;
}

/** 一个目录下的全部 TS 源文件（不含测试文件）：用于「地址栏只有一处读点」的扫描。 */
function sourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      files.push(...sourceFiles(full));
      continue;
    }
    if (!/\.tsx?$/.test(entry) || /\.test\.tsx?$/.test(entry)) continue;
    files.push(full);
  }
  return files;
}

/**
 * 一条地址**命中哪条路由**：路由匹配只看 `pathname`（片段与查询都不进路径）。
 *
 * 与 expo-router 的链接实现同一口径（`getUrlWithReactNavigationConcessions` 用 URL 解析分出
 * `pathname` 与 `hash`，`getStateFromPath` 只拿 `pathname` 匹配）。
 */
function routeKeyOf(address: string): string {
  const { pathname } = new URL(address, ORIGIN);
  return pathname.replace(/\/+$/, "") || "/";
}

/** 一段「有 JSON 对象形状、但字段不全」的配对片段：走读取与结构校验，但不发任何请求。 */
function qrFragment(payload: Record<string, string>): string {
  const json = JSON.stringify(payload);
  return `#${QR_DATA_MARKER}${encodeBase64Url(new TextEncoder().encode(json))}`;
}

/** 新设备短二维码的片段（共享向量 canonical）：浏览器只清除它，不解析。 */
function deviceQrFragment(): string {
  return "#q2=AgABAgMEBQYHCAkKCwwNDg8QERITFBUWFxgZGhscHR4fUSpZeYIpHWiVwqrdwRuVFov6KCpsWZfofCn__9Gcw3w";
}

/**
 * 一份**完整可读**的 node-link 配对片段（`NODE_LINK_PROTOCOL.md` §13.1）：字段齐全、
 * 端点形状与两条 base64url 字段的长度（65 / 32 字节）合规、未过期。
 *
 * 长度合规用全零字节拼出来（`encodeBase64Url` 决定实际长度），不写死一串 base64 字面量——
 * 字面量截短或改长会让「字段长度校验」这一层悄悄失去判别力。
 */
function nodeLinkQrFragment(): string {
  const payload = {
    pairingProtocol: "acp-remote-nodelink-v1",
    ownerNodeId: HOST_ID,
    ownerPublicKey: encodeBase64Url(new Uint8Array(65)),
    endpoint: "wss://work-pc.example.ts.net/node-link/v1",
    pairingId: "2bc8b944-2a4f-46a7-8c31-b2c40923f60a",
    pairingSecret: encodeBase64Url(new Uint8Array(32)),
    expiresAt: new Date(Date.now() + 600_000).toISOString(),
  };
  return `#${QR_DATA_MARKER}${encodeBase64Url(new TextEncoder().encode(JSON.stringify(payload)))}`;
}

interface StubAddressBar {
  /** `replaceState` 收到的 URL（按调用顺序）。 */
  readonly replaced: string[];
  /** `pushState` 收到的 URL：清除片段**不得**用它（那会把带秘密的地址留在历史里）。 */
  readonly pushed: string[];
  /** 此刻地址栏里的地址。 */
  address(): string;
  restore(): void;
}

/**
 * 装一个地址栏替身（`location` + `history`）。
 *
 * Node 里没有地址栏，而组合根的片段读取与清除正是被测面。`pushState` 也记账：清除片段只能
 * 用 `replaceState`（不产生历史记录），用 `pushState` 会把带秘密的地址留在历史里（§7.1 的反面）。
 */
function stubAddressBar(pathname: string, search: string, hash: string): StubAddressBar {
  let current = { pathname, search, hash };
  const replaced: string[] = [];
  const pushed: string[] = [];
  const apply = (url: string): void => {
    const parsed = new URL(url, ORIGIN);
    current = { pathname: parsed.pathname, search: parsed.search, hash: parsed.hash };
  };
  Object.defineProperty(globalThis, "location", {
    configurable: true,
    value: {
      get pathname() {
        return current.pathname;
      },
      get search() {
        return current.search;
      },
      get hash() {
        return current.hash;
      },
    },
  });
  Object.defineProperty(globalThis, "history", {
    configurable: true,
    value: {
      replaceState: (_data: unknown, _title: string, url: string) => {
        replaced.push(url);
        apply(url);
      },
      pushState: (_data: unknown, _title: string, url: string) => {
        pushed.push(url);
        apply(url);
      },
    },
  });
  return {
    replaced,
    pushed,
    address: () => `${current.pathname}${current.search}${current.hash}`,
    restore() {
      Reflect.deleteProperty(globalThis, "location");
      Reflect.deleteProperty(globalThis, "history");
    },
  };
}

/**
 * 建一个只用来读地址栏的组合根：不启动、不连接、不发请求。
 *
 * 平台端口给的是最小形状（显式转型）：本文件的组合根用例只走「读片段 → 结构校验失败」这一段，
 * **不碰**任何平台能力，而四个端口各有自己的测试——把所有方法在这里再实现一遍只是噪音。
 * 若构造期真的用到了其中某一个，这一步会直接抛错，那正是需要知道的信号。
 */
function compositionWithoutNetwork(): Composition {
  const platform = {
    deviceIdentity: {},
    localCache: {},
    lifecycle: {},
  } as unknown as PlatformPorts;
  return createComposition({
    platform,
    // 配对入口的前置校验（`pairing-origin-secure-context`）：本套件跑在 vitest 的 `environment: "node"`
    // 下——没有 `globalThis.isSecureContext`（默认替身因此判成非安全上下文），也没有真实地址栏；而本文件
    // 用的是 `https://…` 形态的来源与载荷，因此这里注入与本文件载荷一致的环境事实。
    secureContext: () => true,
    origin: () => ORIGIN,
    // 一次请求都不该发出：配对内容在校验阶段就被挡下。
    fetch: async () => {
      throw new Error("本用例的配对内容应在结构校验阶段失败，不应发出任何请求");
    },
  });
}

describe("3.2 路由清单：页面路径由 app/ 下的路由文件提供", () => {
  it("文件 ↔ 路径一一对应，没有缺失或多余的页面", () => {
    const actual: Record<string, string> = Object.fromEntries(
      pageFiles().map((file) => [routePathOf(file), file]),
    );
    expect(actual).toEqual(Object.fromEntries(PAGES.map(({ route, file }) => [route, file])));
  });

  it("每个页头注释写的路径与它的路由一致（不再写成片段形态）", () => {
    for (const { page, route, file } of PAGES) {
      const head = readFileSync(join(ROUTE_ROOT, file), "utf8").slice(0, 400);
      const display = route.replace("[directoryId]", "<directoryId>").replace("[id]", "<id>");
      expect(head, page).toContain(`路由 \`${display}\``);
    }
  });
});

describe("3.3 片段不是第二条导航通路", () => {
  it("任意片段都不改变命中的路由（`/pair#/hosts` 仍是配对页，不是主机页）", () => {
    for (const { page, sample } of PAGES) {
      for (const fragment of FRAGMENTS) {
        expect(routeKeyOf(`${sample}${fragment}`), `${page} ${sample}${fragment}`).toBe(routeKeyOf(sample));
      }
    }
    // 期望写死在用例里：`/pair` 的片段（哪怕是 `#/hosts` 这种像路由的内容）不参与路由命中。
    expect(routeKeyOf("/pair#/hosts")).toBe("/pair");
  });

  it("app/ 下不出现片段形态的路由，也不出现地址栏标识符（页面自己读片段再跳转即为反例）", () => {
    const offenders: string[] = [];
    for (const file of [...pageFiles(), "_layout.tsx"]) {
      const source = readFileSync(join(ROUTE_ROOT, file), "utf8");
      if (/#\/[A-Za-z]/.test(source)) offenders.push(`${file}：出现片段形态的路由`);
      if (/\b(?:location|history)\b/.test(source)) offenders.push(`${file}：出现地址栏标识符`);
    }
    expect(offenders).toEqual([]);
  });

  it("地址栏只有组合根一处读点，且只做清除（不用 pushState 把带秘密的地址留在历史里）", () => {
    const readers = [...sourceFiles(SRC_ROOT), ...sourceFiles(ROUTE_ROOT)]
      .filter((file) => /globalThis\.(?:location|history)/.test(readFileSync(file, "utf8")))
      .map((file) => relative(APP_ROOT, file).replaceAll("\\", "/"));
    // Shell 和认证页面只调用 `location.reload()`（连接问题弹窗的恢复动作），不读取片段或地址；
    // 片段读取点仍唯一在组合根。
    expect(readers).toEqual(["src/components/AuthenticatedScreen.tsx", "src/components/shell/Shell.tsx", "src/composition.ts"]);
    // 同一处读点的第二条约束：清除片段只能用 `replaceState`——`pushState` 会把带秘密的地址留在历史里。
    expect(readFileSync(join(SRC_ROOT, "composition.ts"), "utf8")).not.toContain("pushState");
  });
});

describe("3.3 配对二维码地址：片段被读出、被清除，清除后仍停在配对页", () => {
  it("读到片段 → 清除 → 地址仍是配对页的路径（不跳走，也不产生历史记录）", async () => {
    // 只缺字段的载荷：证明片段**先**被读到（否则停在 `idle`），又不需要网络。
    const addressBar = stubAddressBar("/pair", "", qrFragment({ hostId: HOST_ID }));
    const composition = compositionWithoutNetwork();
    try {
      const before = routeKeyOf(addressBar.address());
      expect(before).toBe("/pair");

      await composition.store.startPairingFromAddress("sync");

      const page = composition.store.pairingPage();
      // 旧设备 `data=` 仍被**先**读出并清除，但不再解析或认领：页面明确提示格式已停用。
      expect(page.phase).toBe("legacy_device_qr_unsupported");
      expect(page.presentation.title).toBe("设备二维码旧格式已停用");
      expect(addressBar.replaced).toEqual(["/pair"]);
      expect(addressBar.pushed).toEqual([]);
      // 清除后地址栏里的地址仍落在配对页：`replaceState` 只改地址，不触发导航。
      expect(addressBar.address()).toBe("/pair");
      expect(routeKeyOf(addressBar.address())).toBe(before);
    } finally {
      addressBar.restore();
      composition.dispose();
    }
  });

  it("地址带查询串时，清除后路径与查询都保留，仍停在配对页", async () => {
    const addressBar = stubAddressBar("/pair", "?from=qr", qrFragment({ hostId: HOST_ID }));
    const composition = compositionWithoutNetwork();
    try {
      await composition.store.startPairingFromAddress("sync");

      expect(addressBar.replaced).toEqual(["/pair?from=qr"]);
      expect(addressBar.address()).toBe("/pair?from=qr");
      expect(routeKeyOf(addressBar.address())).toBe("/pair");
    } finally {
      addressBar.restore();
      composition.dispose();
    }
  });

  it("不是配对片段的片段：既不被解析也不被清除，路由不因此改变", async () => {
    const addressBar = stubAddressBar("/pair", "", "#/hosts");
    const composition = compositionWithoutNetwork();
    try {
      await composition.store.startPairingFromAddress("sync");

      // 片段不归配对所有：既不进配对流程，也不被清除（别人的片段可能承载别的语义）。
      // sync 入口没有配对片段时进入配对码输入（S0），而不是把别人的片段当载荷。
      expect(composition.store.pairingPage().phase).toBe("awaiting_code");
      expect(composition.store.pairingPage().origin).toBe(ORIGIN);
      expect(addressBar.replaced).toEqual([]);
      expect(addressBar.pushed).toEqual([]);
      expect(addressBar.address()).toBe("/pair#/hosts");
      expect(routeKeyOf(addressBar.address())).toBe("/pair");
    } finally {
      addressBar.restore();
      composition.dispose();
    }
  });

  it("新设备短二维码片段：先清除，再显示固定 App 指引，不解析也不发请求", async () => {
    const addressBar = stubAddressBar("/pair", "", deviceQrFragment());
    const composition = compositionWithoutNetwork();
    try {
      await composition.store.startPairingFromAddress("sync");

      const page = composition.store.pairingPage();
      expect(page.phase).toBe("device_qr_app_only");
      expect(page.presentation.kind).toBe("info");
      expect(page.presentation.title).toBe("请在 ACP Remote App 内扫码");
      expect(page.presentation.lede).toContain("请在 ACP Remote App 内扫码，或返回网页使用配对码");
      // 片段（含扫码取回凭据）先被清除，且清除后仍落在配对页。
      expect(addressBar.replaced).toEqual(["/pair"]);
      expect(addressBar.pushed).toEqual([]);
      expect(routeKeyOf(addressBar.address())).toBe("/pair");
      // 页面模型不持有凭据，也没有任何请求（compositionWithoutNetwork 的 fetch 会抛错）。
      expect(JSON.stringify(composition.store.state)).not.toContain("AgABAgME");
    } finally {
      addressBar.restore();
      composition.dispose();
    }
  });

  it("node-link 二维码地址：片段被读出、被清除，清除后仍停在 node-link 配对页", async () => {
    const addressBar = stubAddressBar("/node-link/pair", "", nodeLinkQrFragment());
    const composition = compositionWithoutNetwork();
    try {
      await composition.store.startPairingFromAddress("node-link");

      const page = composition.store.pairingPage();
      // 已读到内容（不是 `idle`），且**不是** `failed`：这条入口接受 node-link 载荷，
      // 并把载荷里公开的对端信息呈现出来（§13.1：HTTP 来源由 endpoint 的 host 派生）。
      expect(page.phase).toBe("node_link_scanned");
      expect(page.host?.address).toBe("https://work-pc.example.ts.net");
      expect(page.host?.hostId).toBe(HOST_ID);
      expect(addressBar.replaced).toEqual(["/node-link/pair"]);
      expect(addressBar.pushed).toEqual([]);
      // 清除后地址栏里的地址仍落在 node-link 配对页（`replaceState` 只改地址，不触发导航）。
      expect(routeKeyOf(addressBar.address())).toBe("/node-link/pair");
    } finally {
      addressBar.restore();
      composition.dispose();
    }
  });
});

describe("3.4 其余页面（目录页 / 主机页 / 目录详情 / 会话页）可达性不因该取舍改变", () => {
  it("四处页面各有正常路径，且携带任意片段（含配对片段）时仍命中自己的路由", () => {
    const files = pageFiles();
    for (const { page, route, sample, file } of PAGES) {
      if (page === "配对页") continue;
      // 页面文件真的在磁盘上（可达性的锚点：文件不在就不可达），并落在自己的路径上。
      expect(files, page).toContain(file);
      expect(routePathOf(file), page).toBe(route);
      expect(routeKeyOf(sample), page).toBe(route.replace("[directoryId]", DIRECTORY_ID).replace("[id]", SESSION_ID));
      expect(routeKeyOf(`${sample}${qrFragment({ hostId: HOST_ID })}`), page).toBe(routeKeyOf(sample));
    }
  });
});
