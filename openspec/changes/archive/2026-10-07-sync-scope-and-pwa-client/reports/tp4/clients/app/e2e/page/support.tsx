// 【交接副本】来源 worktree .worktrees/tp4 的 clients/app/e2e/page/support.tsx
// 交付提交 0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83（分支 feat/tp4），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * 用例运行骨架：断言、场景装配与身份预置。
 *
 * ## 预置而不是端到端配对（必须在报告里如实登记的取舍）
 *
 * 真实配对在运行时**不可达**：`app/pair.tsx` 没有任何二维码扫描入口，
 * `src/composition.ts` 的 `rememberPairedHost` 与 `HttpPairingTransport` 没有生产调用方，
 * 连接机的 `poll` 在 `approved` 分支也不返回主机公钥。因此本目录**不走配对流程**：
 * 它直接用真实平台端口（`openPlatform()` 的真实 IndexedDB 写入口）写入
 * `paired-host/<origin>` 记录，再让**真实**的 `createComposition().start()` 读到它。
 *
 * 预置的是「已配对」这个**起点**，其后的握手、验签、快照、命令全部走生产代码。
 * 被绕过的是配对页与 HTTPS 轮询面——那部分登记为未验证项。
 */

import { createElement, useEffect, useState } from "react";
import type { ReactElement } from "react";
import { createRoot } from "react-dom/client";

import { createComposition } from "../../src/composition";
import type { Composition } from "../../src/composition";
import { openPlatform } from "../../src/platform";
import { ClientStoreProvider } from "../../src/features/runtime";
import RootLayout from "../../app/_layout";
import { Stack, mountRoute, registerRoutes } from "./router-stub";
import type { RouteEntry } from "./router-stub";
import { createFakeHostIdentity, FakeHost } from "./fake-host";
import type { FakeHostIdentity, FakeHostOptions } from "./fake-host";

/** 已配对主机记录在本地缓存里的键前缀（`src/composition.ts:112`）。 */
const PAIRED_HOST_KEY_PREFIX = "paired-host/";

/** 用例统一的规范化来源；`https` 形状满足 `$defs.canonicalOrigin`。 */
export const CANONICAL_ORIGIN = "https://127.0.0.1:1";

// 路由替身里的 `mountRoute` 由本模块转出：用例只需要一个入口去驱动页面。
export { mountRoute } from "./router-stub";

/** 固定主机标识：断言「连接用的是预置的那把钥匙」时需要它。 */
export const HOST_ID = "bdb2ec20-f98c-4d87-b789-e540d527ef87";

// ── 断言 ────────────────────────────────────────────────────────────────────

declare global {
  interface Window {
    /** 页面级错误收集器：由 `run-e2e.mjs` 注入的 bootstrap 建立；未注入时为 `undefined`。 */
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
 * `window.__pageErrors` 里没人看。收集器由 `run-e2e.mjs` 的 HTML 注入。
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

/** 条件断言。 */
export function expectThat(condition: boolean, detail: string): void {
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

/** 预置设备身份 + 已配对主机记录。返回主机身份（用例可核对公钥真的被用上了）。 */
export async function seedPairedHost(): Promise<FakeHostIdentity> {
  const identity = await createFakeHostIdentity(HOST_ID);
  const platform = openPlatform();
  await platform.deviceIdentity.ensureIdentity({
    deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
    canonicalOrigin: CANONICAL_ORIGIN,
  });
  await platform.localCache.putLocalSummary({
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
  const identity = await seedPairedHost();
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

  const composition = createComposition({ openSocket: host.openSocket });
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