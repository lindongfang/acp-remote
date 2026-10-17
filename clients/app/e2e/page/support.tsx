/**
 * 用例运行骨架：断言、场景装配与身份预置。
 *
 * ## 起点：多数用例预置「已配对」，配对链路本身另有专门用例
 *
 * 真实配对**在运行时可达**（本登记此前写作「不可达」，已按当前事实改写）：
 *
 * - 配对页只从**地址片段**取内容（`/pair#data=…` 或 `/node-link/pair#data=…`）：
 *   `app/pair.tsx` → `src/components/PairingPanel.tsx` 的固定状态页，页面挂载后由
 *   `ClientStore.startPairingFromAddress` 读取并立即清除片段，随后自动认领——页面没有输入框、
 *   粘贴框、提交按钮或页内返回；
 * - 认领与轮询由 `ClientStore.startPairingFromAddress` 驱动，
 *   网络面是 `src/composition.ts` 的 `HttpPairingTransport`（`POST /sync/v1/pairing/claim`
 *   与 `/sync/v1/pairing/status`）——两者都有生产调用方；
 * - 已批准时 `poll` 的 `approved` 分支返回主机标识与主机公钥，`rememberPairedHost` 把它写进
 *   `paired-host/<origin>`（`src/composition.ts` 的 `PAIRED_HOST_KEY_PREFIX`），随后
 *   `onPaired` 装配连接并触发一次连接；页面约 1.6 秒后自动 `replace` 到目录页根路由 `/`。
 *
 * 这条链路的浏览器级证据是 `checks/pairing-first-run.ts` 的 PW-1：它从**未配对**出发，
 * 把二维码地址的片段挂到 `location.hash` 上再打开配对页，走完自动认领 → 等待本机确认 →
 * 已批准 → 落盘 → 自动进入目录页 → 连接在线。
 *
 * ## 那为什么多数用例仍然预置「已配对」
 *
 * 因为它们的被测面是**已配对之后**的行为（握手、验签、快照、命令、各页面的呈现），
 * 配对只是一次性的前置动作：让每条用例都从零配对会把「等待本机确认」的 2 秒轮询与一次
 * IndexedDB 写入乘上用例数，并把「页面用法」与「配对链路」两件事混进同一条断言——
 * 配对链路坏掉时本该只红 PW-1，却会让整个检查集变红而丢掉定位。
 *
 * 预置写的是**真实**平台端口（`openPlatform()` 的真实 IndexedDB）里的同一份记录，
 * 读取它的也是**真实**的 `createComposition().start()`：被跳过的只有配对页与 HTTPS 轮询面，
 * 其后的握手、验签、快照、命令全部走生产代码。需要「未配对」起点的用例用
 * `ScenarioOptions.seedPaired: false`（`pairing-first-run.ts` 的 PW-1…PW-5 与
 * `pairing-kinds.ts` 的 NL-1…NL-3 就是）。
 */

import { createElement, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { createRoot } from "react-dom/client";

import { createComposition } from "../../src/composition";
import type { Composition, FetchLike } from "../../src/composition";
import { openPlatform } from "../../src/platform";
import type { DeviceIdentity } from "../../src/platform";
import { ClientStoreProvider } from "../../src/features/runtime";
import RootLayout from "../../app/_layout";
import { Stack, mountRoute, registerRoutes } from "./router-stub";
import type { RouteEntry } from "./router-stub";
import { createFakeHostIdentity, FakeHost } from "./fake-host";
import type { FakeHostIdentity, FakeHostOptions } from "./fake-host";

/** 已配对主机记录在本地缓存里的键前缀（`src/composition.ts` 的 `PAIRED_HOST_KEY_PREFIX`）。 */
const PAIRED_HOST_KEY_PREFIX = "paired-host/";

/** 用例统一的规范化来源；`https` 形状满足 `$defs.canonicalOrigin`。 */
export const CANONICAL_ORIGIN = "https://127.0.0.1:1";

// 路由替身里的 `mountRoute` / `navigate` 由本模块转出：用例只需要一个入口去驱动页面。
// `navigate` 与页面里的 `useRouter().push` 走同一条路径查找，因此「离开某一页再回来」
// 不需要用例自己重挂组件树（那会换掉 store，验的不是同一件事）。
export { mountRoute, navigate } from "./router-stub";

/** 固定主机标识：断言「连接用的是预置的那把钥匙」时需要它。 */
export const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";

// ── 断言 ────────────────────────────────────────────────────────────────────

declare global {
  interface Window {
    /**
     * 页面级错误收集器：由 `run-e2e.mjs` 的入口文档以外部脚本（`/page-errors.js`）建立的
     * bootstrap 收集；未注入时为 `undefined`。
     */
    __pageErrors?: readonly unknown[];
  }
}

/**
 * 页面快照：DOM 片段 + 页面级错误。失败定位靠它。
 *
 * ## 为什么不能直接切 `document.body.innerHTML`
 *
 * 页面 body 的开头是启动占位 `<div id="root">` 与那段收集页面错误的 bootstrap `<script>`；
 * 二者加起来就吃掉了 600 字符的窗口，真正被挂载的组件树（容器排在它们**之后**）一个字都看不到。
 * 这里先摘掉脚本节点再截，让片段落在组件树上。
 *
 * ## 为什么要在**抛错当时**取，而不是在 `runChecks` 捕获时取
 *
 * 每条用例都在 `finally` 里卸载容器（`dispose()` 后的组合根不可复活）。等错误冒泡到
 * `runChecks` 时页面已经被拆空，快照只剩 `<div id="root"></div>`——这正是此前失败明细
 * 无法定位偶发红的原因。因此 `CheckFailure` 在构造时就把它固定下来。
 *
 * ## 为什么要带页面级错误
 *
 * 页面错误（未捕获异常 / 未处理的 promise 拒绝）不会让用例直接变红，只会让它**超时**：
 * 握手里某一跳抛错时页面停在原状态，用例报的是「等待 X 超时」，真正的原因却只躺在
 * `window.__pageErrors` 里没人看。收集器由 `run-e2e.mjs` 的入口文档引入——形态是**同源外部脚本**
 * （`/page-errors.js`），因为本页按生产口径带 `script-src 'self'`，内联脚本会被内核拒掉。
 */
function pageSnapshot(): string {
  const errors = (window.__pageErrors ?? []).map((entry) => String(entry));
  // `cloneNode` 的返回类型是 `Node`；文档 body 是元素，它的深克隆同样是元素。
  const clone = document.body.cloneNode(true) as HTMLElement;
  for (const script of [...clone.querySelectorAll("script")]) script.remove();
  const errorText = errors.length === 0 ? "（无）" : errors.join(" | ");
  return `页面：${clone.innerHTML.slice(0, 600)}｜页面错误：${errorText}`;
}

/** 断言失败：整条检查判红，错误消息进入结果明细。 */
export class CheckFailure extends Error {
  /** 抛错**当时**的页面快照（用例的 `finally` 之后页面已被拆空，所以必须现在取）。 */
  readonly snapshot: string;

  constructor(message: string) {
    super(message);
    this.name = "CheckFailure";
    this.snapshot = pageSnapshot();
  }
}

/**
 * 条件断言。
 *
 * 声明为**断言函数**（`asserts condition`）而不是返回 `void`：用例普遍写成
 * `const entry = query(...); expectThat(entry !== null, "…");` 之后直接用 `entry`——
 * 那正是「断言不过就抛 `CheckFailure`、过了就一定有值」的语义。少了 `asserts`，TS 的控制流分析
 * 看不见这层保证，`domText(entry)` 这类**只接受非 null** 的助手就会报类型错误（或者迫使每个调用点
 * 写一遍局部变量 + 非空断言）。行为不变：条件不成立时仍然抛同一个 `CheckFailure`。
 */
export function expectThat(condition: boolean, detail: string): asserts condition {
  if (!condition) throw new CheckFailure(detail);
}

/** 相等断言：`actual` / `expected` 序列化后逐字比较（对象按键序无关的深比较）。 */
export function expectEqual(actual: unknown, expected: unknown, detail: string): void {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new CheckFailure(`${detail}；实际 ${JSON.stringify(actual)}，期望 ${JSON.stringify(expected)}`);
  }
}

/**
 * 负向断言优先：一段文本里**不得**出现某个片段。
 *
 * R15/R16 的多数 MUST NOT 只能用它证伪——「模型对象里没有这个键」证明不了
 * 渲染结果里没有泄露出来的路径片段。
 */
export function expectAbsent(haystack: string, needle: string, detail: string): void {
  if (haystack.includes(needle)) throw new CheckFailure(`${detail}：文本里出现了 ${JSON.stringify(needle)}`);
}

/** 负向断言的复数形式：任一片段出现即判红。 */
export function expectNone(haystack: string, needles: readonly string[], detail: string): void {
  for (const needle of needles) expectAbsent(haystack, needle, detail);
}

/**
 * 创建入口的权限门控类文案。
 *
 * 设备方向不再有「创建会话的权限」这一维度：这些说法（连同 `data-create-permission-note` 之类的
 * 标记）一个都不该再出现。词表放在这里而不是各用例文件各写一份，是为了让目录页、目录详情页与
 * 创建弹层三处的负向断言查的是**同一份**词表——各写一份的结果必然是慢慢只剩一处还在查。
 */
export const PERMISSION_GATE_VOCABULARY = ["创建权限", "创建会话的权限", "电脑端授予"] as const;

/** 一条检查的结果。 */
export interface CheckResult {
  readonly name: string;
  readonly passed: boolean;
  readonly detail: string;
}

/** 一条检查：名字 + 异步主体。 */
export interface Check {
  readonly name: string;
  readonly run: () => Promise<void>;
}

/** 顺序跑完全部检查，逐条记录通过与否（一条抛错不影响其余）。 */
export async function runChecks(checks: readonly Check[]): Promise<CheckResult[]> {
  const results: CheckResult[] = [];
  for (const check of checks) {
    try {
      await check.run();
      results.push({ name: check.name, passed: true, detail: "通过" });
    } catch (error) {
      results.push({
        name: check.name,
        passed: false,
        // 失败时附上页面快照：没有它，「等不到某个选择器」这类失败根本无法定位。
        // `CheckFailure` 自带抛错当时的快照（那时容器还在）；其它异常只能现取（页面已空）。
        detail: `${error instanceof Error ? error.message : String(error)}｜${
          error instanceof CheckFailure ? error.snapshot : pageSnapshot()
        }`,
      });
    }
  }
  return results;
}

// ── DOM 读取 ────────────────────────────────────────────────────────────────

/** 一个已挂载的场景。 */
export interface Mounted {
  readonly container: HTMLElement;
  readonly host: FakeHost | null;
  readonly composition: Composition | null;
  /** 卸载：解除 React 树与组合根，避免 `dispose()` 后的单例复活问题干扰下一条用例。 */
  readonly unmount: () => Promise<void>;
}

/** 容器的序列化 DOM（R15/R18 的负向断言就查它）。 */
export function domHtml(container: Element): string {
  return container.innerHTML;
}

/** 容器的可见文本。 */
export function domText(container: Element): string {
  return container.textContent ?? "";
}

/** 第一个匹配元素；根节点缺席时返回 `null`（存在性由调用点的断言把关）。 */
export function query(root: ParentNode | null, selector: string): Element | null {
  return root?.querySelector(selector) ?? null;
}

/** 全部匹配元素；根节点缺席时返回空数组。 */
export function queryAll(root: ParentNode | null, selector: string): readonly Element[] {
  return root === null ? [] : [...root.querySelectorAll(selector)];
}

/** 真实点击：`HTMLElement.click()` 触发 React 的事件委托，等价于用户点一次。 */
export function click(element: Element | null, detail: string): void {
  if (element === null) throw new CheckFailure(`${detail}：找不到可点击元素`);
  (element as HTMLElement).click();
}

/** 元素的 `textContent`；缺席时返回 `""`（缺席本身由调用点的存在性断言把关）。 */
export function textOf(element: Element | null | undefined): string {
  return element?.textContent ?? "";
}

/** 轮询直到条件成立；超时抛错并带上标签，便于定位是哪一步没等到。 */
export async function waitFor(predicate: () => boolean, label: string, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    const { promise, resolve } = Promise.withResolvers<void>();
    setTimeout(resolve, 25);
    await promise;
  }
  throw new CheckFailure(`等待「${label}」超时（${timeoutMs}ms）`);
}

// ── 身份预置 ────────────────────────────────────────────────────────────────

/**
 * 预置的设备标识（`src/platform/` 的 `DeviceIdentity.deviceId`）。
 *
 * 固定值而不是随机：配对用例要逐字断言「认领正文里的 `deviceId` 是平台身份的那一个」，
 * 随机值会让这条断言退化成「两边都从同一个变量取」的复述。同一来源下 `ensureIdentity`
 * 是幂等的（返回既有身份），因此 `seedDeviceIdentity` 与 `seedPairedHost` 谁先谁后都拿到同一份。
 */
export const DEVICE_ID = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/**
 * 只预置设备身份（不写已配对主机记录）——「未配对」这个起点的前置。
 *
 * 与 `seedPairedHost` 分开：**设备身份**与**已配对记录**是两件事，真实运行时前者在首次
 * 配对时就存在（`claim` 调 `ensureIdentity`），后者要等主机批准才写入。同来源下幂等。
 */
export async function seedDeviceIdentity(): Promise<DeviceIdentity> {
  return openPlatform().deviceIdentity.ensureIdentity({
    deviceId: DEVICE_ID,
    canonicalOrigin: CANONICAL_ORIGIN,
  });
}

/** 预置设备身份 + 已配对主机记录。返回主机身份（用例可核对公钥真的被用上了）。 */
export async function seedPairedHost(): Promise<FakeHostIdentity> {
  const identity = await createFakeHostIdentity(HOST_ID);
  await seedDeviceIdentity();
  await openPlatform().localCache.putLocalSummary({
    key: `${PAIRED_HOST_KEY_PREFIX}${CANONICAL_ORIGIN}`,
    summary: JSON.stringify({ hostId: identity.hostId, publicKeyBase64Url: identity.publicKeyBase64Url }),
    nowMs: Date.now(),
  });
  return identity;
}

// ── 场景装配 ────────────────────────────────────────────────────────────────

/** 场景输入：`FakeHostOptions` 加一个初始路由。 */
export type ScenarioOptions = FakeHostOptions & {
  readonly route: RouteEntry;
  /** 把预置的 `paired-host/<origin>` 写成非法摘要（模拟记录被外部改坏）。 */
  readonly corruptPairedHostRecord?: boolean;
  /** 额外注册可导航到的路由（点击目录行进入详情页时用得上）。 */
  readonly extraRoutes?: readonly RouteEntry[];
  /**
   * 是否预置「已配对」记录；缺省 `true`（多数用例的起点，见文件头）。
   *
   * `false` = 从**未配对**出发：只预置设备身份，`start()` 因此停在 `unpaired`，
   * 后面的「已配对」只能由真实配对链路（打开携带片段的配对地址 → 认领 → 批准 → 落盘）挣来
   * ——`pairing-first-run.ts` 与 `pairing-kinds.ts` 用它。
   */
  readonly seedPaired?: boolean;
  /** 预置的主机身份；缺省新建一把。未配对起点用它把「已批准」响应的公钥与宿主钥对上。 */
  readonly identity?: FakeHostIdentity;
  /**
   * 配对 HTTPS 面的替身（`claim` / `status`）；缺省用生产 `fetch`。
   *
   * 真实网络面在 e2e 环境里不可达（`canonicalOrigin` 是 `https://` 而页面由 http 提供），
   * 因此配对用例必须注入它；注入的替身只替换**网络**，配对状态机、落盘与之后的连接
   * 仍是生产代码。
   */
  readonly fetch?: FetchLike;
  /**
   * 环境事实：本页是否处于安全上下文（`CompositionInputs.secureContext`）；缺省不注入、用生产实现。
   *
   * 与 `fetch` 同一理由注入：e2e 页面由 http 提供、二维码夹具的 `canonicalOrigin` 是 `https://…`，
   * 要验「安全上下文不成立时明确失败」必须能把这条事实造出来。
   */
  readonly secureContext?: () => boolean;
  /** 环境事实：本页实际来源（`CompositionInputs.origin`）；缺省不注入、用生产实现。 */
  readonly origin?: () => string;
};

/**
 * 与 `app/_layout.tsx` **同构**的挂载件，唯一差别是组合根由用例注入
 * （`openSocket` 换成假主机，其余端口全部是生产实现）。
 *
 * 为什么不直接用真实的 `_layout.tsx`：它以**无参** `createComposition()` 建立
 * 模块作用域单例，`CompositionInputs` 无处可注入。要验的接线（`useEffect` → `start()`
 * → store → 页面）在这里一字不差地重演一遍；真实 `_layout.tsx` 本身另有一条用例
 * （`composition-root.tsx`）在浏览器里直接挂它。
 */
function HarnessLayout({ composition }: { readonly composition: Composition }): ReactElement {
  const [runtime, setRuntime] = useState<Composition | null>(null);

  useEffect(() => {
    let cancelled = false;
    void composition
      .start()
      .catch(() => undefined)
      .then(() => {
        if (!cancelled) setRuntime(composition);
      });
    return () => {
      cancelled = true;
      composition.dispose();
    };
  }, [composition]);

  return (
    <ClientStoreProvider store={runtime?.store ?? null}>
      <Stack screenOptions={{ headerShown: false }} />
    </ClientStoreProvider>
  );
}

/**
 * 挂一个走真实 `createComposition` 的场景（socket 换成假主机）。
 *
 * 返回的 `composition.store` 让用例能直接核对连接状态机与快照是否落地——
 * 这些是「接线通了」的正面证据，而不是「页面没崩」。
 */
export async function mountScenario(options: ScenarioOptions): Promise<Mounted> {
  // 未配对的起点：设备身份要有（`claim` 拿它出证），已配对记录**不**写——`start()` 因此
  // 停在 `unpaired`，页面呈现的配对页才有事可做。
  //
  // 还要先**清掉本地缓存**：同一个浏览器 profile 在同一次 e2e 运行里被所有用例共用，
  // 上一条用例（`seedPairedHost`）留下的 `paired-host/<origin>` 会让本起点一挂起来就是
  // 「已配对」，后面的断言会在一条预置记录上走捷径。设备身份另有存储面，不受这次清除影响。
  const unpairedStart = options.seedPaired === false;
  if (unpairedStart) await openPlatform().localCache.clear();
  const identity = unpairedStart
    ? (options.identity ?? (await createFakeHostIdentity(HOST_ID)))
    : await seedPairedHost();
  if (unpairedStart) await seedDeviceIdentity();
  // 「预置记录被改坏」的场景：写入一条形状非法的摘要。`parseStoredPairedHost`
  // 把它判成「从未配对」而不是抛错——验签因此绝不会被放行。
  if (options.corruptPairedHostRecord) {
    await openPlatform().localCache.putLocalSummary({
      key: `${PAIRED_HOST_KEY_PREFIX}${CANONICAL_ORIGIN}`,
      summary: "{不是合法 JSON",
      nowMs: Date.now(),
    });
  }
  const host = new FakeHost(options, identity);
  const container = document.createElement("div");
  document.body.append(container);
  registerRoutes([...(options.extraRoutes ?? []), options.route]);
  mountRoute(options.route);

  // 配对 HTTPS 面：用例给了替身就用它，否则回到生产 `fetch`（组合根的缺省）。环境事实
  // （安全上下文 / 当前来源）同属可注入的一次性能力：给了就注入，没给就让组合根读浏览器全局
  // ——用条件展开而不是 `options.fetch`，因为 `exactOptionalPropertyTypes` 下显式传
  // `undefined` 与「不传」不是一回事。
  const composition = createComposition({
    openSocket: host.openSocket,
    ...(options.fetch === undefined ? {} : { fetch: options.fetch }),
    ...(options.secureContext === undefined ? {} : { secureContext: options.secureContext }),
    ...(options.origin === undefined ? {} : { origin: options.origin }),
  });
  const root = createRoot(container);
  root.render(createElement(HarnessLayout, { composition }));

  return {
    container,
    host,
    composition,
    async unmount() {
      root.unmount();
      container.remove();
    },
  };
}

/**
 * 直接挂真实的 `app/_layout.tsx`。
 *
 * 这里的组合根是**完全无参**的：`openPlatform()` 取真实 IndexedDB 与真实 WebCrypto，
 * `openWebSocket` 取真实 `WebSocket`。预置的 `paired-host/<origin>` 指向一个**不存在的**
 * 端口，因此连接必然失败——这条用例要证明的恰恰是：失败之后页面**仍然**离开
 * `RuntimeUnavailable`，即 `start()` 真的在浏览器里跑完了。
 */
export async function mountRootLayout(): Promise<Mounted> {
  await seedPairedHost();
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  root.render(createElement(RootLayout));

  return {
    container,
    host: null,
    composition: null,
    async unmount() {
      root.unmount();
      container.remove();
    },
  };
}