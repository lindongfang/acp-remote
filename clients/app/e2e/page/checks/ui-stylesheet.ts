/**
 * 样式形态与样式生效：`ui-csp-inline-styles` 的浏览器级判据（task 2.1 的判据，落在 2.2 要求的
 * **既有检查集**里，而不是一次性脚本）。
 *
 * ## 为什么这条判据只能在这一层取
 *
 * 静态面的 CSP 里**没有 `style-src`**，因此 `style-src` 与 `style-src-attr` 都回落到
 * `default-src 'self'`：内联 `<style>` 元素与 `style=` 属性一律被浏览器内核拒掉。node/jsdom 里
 * 没有这条拦截（`element.sheet` 不为 `null`，样式照样生效），所以组件级测试与 vitest 复现不了它；
 * 本目录的页面现在按生产口径带同一组响应头（`e2e/run-e2e.mjs` 的 `STATIC_HEADERS`）。
 *
 * ## 被拦下时页面发生什么
 *
 * `react-native-web` 0.21 的 `StyleSheet` 在**模块求值期**执行 `createSheet()` →
 * `createCSSStyleSheet('react-native-stylesheet')`：先 `document.getElementById(id)`，**命中就复用
 * 该元素的 `.sheet`**，查不到才 `document.createElement('style')`。后者在既定 CSP 下
 * `element.sheet === null`，而 `createOrderedCSSStyleSheet` 的每一次 CSSOM 写入都以
 * `if (sheet != null)` 守门——一条原子类规则都进不了 DOM，计算样式整体退回浏览器默认值
 * （设计期实测：同一个 `View` 的 `display` 从 `flex` 变成 `block`）。
 *
 * ## 两条检查各自证伪什么
 *
 * - **UI-1** 证伪「页面仍在依赖被拦下的内联样式形态」：DOM 里出现任何 `<style>` 元素即判红
 *   （RNW 的兜底分支会自己造一个），并正面断言 RNW 的规则真的落在**那份同源表**里；
 * - **UI-2** 证伪「表被拦下但页面看起来没坏」：读的是**计算样式**，期望值分别来自 RNW 自己的
 *   `View` 默认样式与应用的样式 token。表失效时同一个元素算出来的是浏览器默认值
 *   （`block` / `row` / `0px`），因此这两条断言必然变红。
 *
 * 断言一律不看 class 名（RNW 开发态用可读名、生产态用哈希名，两者都不稳定），也不看源码文本。
 */

import { AGENTS, DIRS_ROUTE, WORKSPACES_WITH_PATHS, sessionsForBothDirectories } from "../fixtures";
import { spacing } from "../../../src/components/theme";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";

/**
 * 本检查集的场景：真实 `start()` + 真实握手，但**不下发快照**。
 *
 * 落点选目录页（`DIRS_ROUTE`）而不是专门造一个只渲染样式的最小页面：被测的必须是**真实页面**
 * 在真实托管面上的样式形态，因此取的是 `app/index.tsx` → `DirectoryList` → `HostConnectionPanel`
 * 这一串真实组件。不下发快照只是让路径最短（省掉快照的两级摘要校验）：本检查与数据无关，
 * `DirectoryList` 的 `loading` 分支与 `ready` 分支渲染的是同一张 `styles.page`。
 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: false,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: DIRS_ROUTE,
    ...overrides,
  };
}

/**
 * 等到**真实页面**渲染出来。
 *
 * 等的是目录页的加载态文案（`data-directory-state="loading"`，只有真实 `DirectoryList` 会发）：
 * store 为 `null` 时页面呈现的是占位 `RuntimeUnavailable`，它不发这个属性，因此等到它就等价于
 * 「组合根装配完成、真实路由组件已渲染」——与 CR-1 同一条防呆口径（用 `[data-route="dirs"]`
 * 等待会在首帧占位时就成立）。
 */
async function waitForRenderedPage(container: HTMLElement): Promise<void> {
  await waitFor(
    () => query(container, '[data-directory-state="loading"]') !== null,
    "真实页面渲染出目录页",
  );
}

/** 某个锚点的计算样式；锚点缺席即判红（本检查的断言全挂在它上面）。 */
function computedStyleOf(container: HTMLElement, selector: string, label: string): CSSStyleDeclaration {
  const element = query(container, selector);
  if (element === null) throw new CheckFailure(`${label}：容器里找不到 ${selector}`);
  return getComputedStyle(element);
}

const checks: readonly Check[] = [
  {
    name: "UI-1 页面不依赖被拦下的内联样式形态：DOM 里没有 <style> 元素，且 RNW 的规则落在同源样式表里",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForRenderedPage(mounted.container);

        // ① 内联 `<style>` 元素：一条都不许有。
        //
        // RNW 命中不了 `#react-native-stylesheet` 时会自己 `document.createElement('style')` 并插进
        // `<head>`，因此这里的条数就是「它复用了入口文档那张同源表，还是退回了被 CSP 拦下的形态」。
        // 页面已经渲染过一轮才取，顺带覆盖「某个组件在渲染期注入样式元素」这种形态。
        const inlineStyles = [...document.querySelectorAll("style")];
        expectEqual(
          inlineStyles.length,
          0,
          `页面里不得出现内联 <style> 元素（实际：${
            inlineStyles.map((element) => element.outerHTML.slice(0, 60)).join(" | ") || "无"
          }）`,
        );

        // ② 正面证据一：RNW 复用到的挂点就是入口文档声明的那个 `<link>`，且同源。
        //    这里刻意用 RNW 自己的查找方式（`document.getElementById`），断言的就是「它当时会撞上的
        //    那个元素」；换成别的选择器会在挂点被改名时给出一个偏题的结论。
        const sheetElement = document.getElementById("react-native-stylesheet");
        expectThat(
          sheetElement !== null,
          "入口文档必须以 id=react-native-stylesheet 声明样式表挂点（RNW 只按这个 id 复用）",
        );
        if (!(sheetElement instanceof HTMLLinkElement)) {
          throw new CheckFailure(
            "样式表挂点必须由入口文档以 <link rel=stylesheet> 声明，而不是由脚本自己造元素",
          );
        }
        expectEqual(sheetElement.rel, "stylesheet", "挂点的 rel 取值");
        expectEqual(
          new URL(sheetElement.href).origin,
          location.origin,
          "样式表必须与页面同源（CSP 的 default-src 只放行同源的 <link rel=stylesheet>）",
        );

        // ③ 正面证据二：RNW 的规则真的写进了**这张**表。
        //
        // `createSheet()` 会先插入 4 条顶层 reset，随后每个 `StyleSheet.create` 都在**求值期**把编译出的
        // 原子类规则 `insertRule` 进来（`compileAndInsertAtomic`）。表被判成 `null` 时这些写入被
        // `if (sheet != null)` 整段挡掉：规则数为 0 只可能意味着「RNW 拿到的不是一张可写的表」。
        const sheet = sheetElement.sheet;
        expectThat(sheet !== null, "同源样式表的 .sheet 不得为 null（为 null 时 RNW 的规则写入被静默丢弃）");
        const ruleCount = sheet?.cssRules.length ?? 0;
        expectThat(ruleCount > 0, `RNW 的规则必须落在同源样式表里（实际规则数 ${ruleCount}）`);
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "UI-2 RNW 生成的样式真的生效：View 的计算 display/flex-direction 与面板的 padding 是 RNW 与 token 的取值",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForRenderedPage(mounted.container);

        // 目录页顶层 View（`DirectoryList` 的 `styles.page`，只声明了 `gap`）。
        //
        // `display: flex` / `flex-direction: column` **不是**应用声明的，而是 RNW 的 `View` 默认样式
        // （`react-native-web` 的 `View/index.js` 里 `view$raw` 那几条）。它们只可能来自那张原子类样式表：
        // 表被拦下时同一个 `<div>` 算出来的是浏览器默认值 `block` / `row`（设计期实测是 `display: block`）。
        const page = computedStyleOf(mounted.container, '[data-route="dirs"]', "目录页顶层 View");
        expectEqual(page.display, "flex", "目录页 View 的计算 display（RNW 的 View 默认样式）");
        expectEqual(page.flexDirection, "column", "目录页 View 的计算 flex-direction（RNW 的 View 默认样式）");

        // 目录页工具条：RNW 的默认样式之外，它自己还声明了 `paddingVertical: spacing.md`
        // 与一条下边框。这一条证明的**不是**「RNW 的默认值还在」，而是**应用的原子类**也落在
        // 同一张表里：RNW 把数值型长度补成 `12px` 写进类规则，表被拦下时那里的计算值是 `0px`。
        //
        // 期望值取自 `spacing.md` 这个 token 而不是字面量 `12px`：本断言要证伪的是「表失效」，而表失效的
        // 失败形态是**浏览器默认值**（0px），不是另一个 token 取值；写死字面量只会在合法改 token 时
        // 制造一条与样式机制无关的红。
        const toolbar = computedStyleOf(mounted.container, '[data-directory-toolbar="true"]', "目录页工具条");
        expectEqual(toolbar.display, "flex", "目录页工具条的计算 display（RNW 的 View 默认样式）");
        expectEqual(
          toolbar.paddingTop,
          `${spacing.md}px`,
          "目录页工具条的计算 padding-top（应用声明的 token 值）",
        );
        expectEqual(toolbar.borderBottomWidth, "1px", "目录页工具条的下边框（应用声明的样式）");
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const UI_STYLESHEET_CHECKS = checks;
