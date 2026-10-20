/**
 * R15「目录详情按时间倒序分页」的浏览器级验证（详情页那一半）。
 *
 * 与 `r15-directory.ts` 同一套装配：真实路由组件 `app/dir/[directoryId].tsx` → 真实
 * `DirectoryDetail` → `src/features/directory-model.ts`，数据来自真实快照管线。
 * 三个共用机关（`withScenario` / `waitForDirectoryList` / `typeIntoInput`）从那里导入，
 * 不复制第二份——两页的输入框、卸载纪律必须逐字一致。
 *
 * ## 判别力
 *
 * - 「两种空互不冒充」：目录标识不在快照里（`unknown_directory`）与「目录里没有会话」
 *   （`sessions="empty"`）是两个分支——任一支被合并进另一支，对应用例必红；详情页没有查询词
 *   与月份档，因此**不存在**「数据在、只是被筛掉了」的第三支（旧的 `data-directory-detail-no-match`
 *   随旧规格一并移除）。
 * - 「页头」：展示名由路径尾段派生、路径逐字等于快照 `workspaces` 条目上的 `path`；目录标识只在
 *   `data-directory-id` 上，不得进入可见文本（拿标识当标题/路径的实现必红）。
 * - 「分页」：页码、总页数、每页固定条数与切片集合都取 `data-*`；各页的会话集合互不相同，
 *   改错 offset、改错方向或按月份/查询再滤一遍都会撞上别的页。
 * - 「非法页码在提交时就地纠正」：在**第 2 页**敲非数字，先断言**输入期间**输入框里就是原文
 *   （`abc`）、页码与列表一个都没动，再按回车，断言输入框被回填成 2、列表逐字不变且会话行
 *   没有重挂——把非数字一律当第一页的实现会在这里红；每个 `input` 事件都提交并回写纠正值的
 *   实现（输入框永远清不空）在第一段就红。
 * - 「清空重输」：把输入框清空后重新敲 `1`，断言清空期间它真的是空的（每键都提交的实现会立刻
 *   把纠正值写回），回车后落到第 1 页而不是被残留数字拼成 `21` 再夹到最后一页。
 * - 「失焦也提交」：敲 `99` 后不失焦整段不动，派发 `focusout` 后越界值夹到边界。
 * - 「只有一页」：翻页区与页码输入**仍然在场**，上一页与下一页均为禁用态（页码 1 / 1）。
 * - 「翻页后回到页首」：把挂载容器变成真的可滚动区，断言翻页前滚动位置确实 > 0（否则用例假绿），
 *   翻页后归零。
 * - 「窄视口」：先断言两档的布局方向真的换了（`flex-direction` 由 `row` 变 `column`），
 *   再比对两档的 `data-*` 集合逐项相同——宽度覆盖没生效时前置断言就会红，不会变成
 *   「拿同一条渲染路径自证」。
 *
 * ## 跨路由进入详情页（替身修好之后才可测）
 *
 * `router-stub.tsx` 的 `Stack` 早先写成 `return current.component()`：那是在 `Stack` 自己的
 * fiber 上**调用**路由组件，路由组件的 hooks 与 `Stack` 的 `useSyncExternalStore` 共用同一批
 * 槽位。切到 hooks 序列不同的路由组件（目录页 ↔ 详情页）时，React 的更新分派器拿上一轮的
 * `deps` 比对新一轮的 effect，抛
 * `TypeError: Cannot read properties of undefined (reading 'length')`（栈顶是
 * `areHookInputsEqual ← updateEffectImpl ← updateSyncExternalStore ← useLocalSearchParams`）
 * 并把整棵树卸载——实测点击目录项后容器 `innerHTML` 变空、`[data-route]` 一个都不剩。
 *
 * 因此替身改成把路由组件当**子元素**渲染（`createElement(current.component, { key: current.path })`），
 * 于是 R15-21（两条入口都进入**该目录**的详情页）、R15-22（返回目录页后查询词仍在）、
 * R15-23/R15-24（零会话的两种情形都能进入详情页）、R15-25（再进入同一个目录时页码复位）
 * 四条用例可以真的走一遍跨路由。
 *
 * `key` 是必要的：同类型路由之间切换（`/dir/<id-A>` → `/dir/<id-B>`）时若不重挂，上一屏的局部
 * 状态会被带进下一屏（实测：在 alpha 目录选过的 Agent 会在 beta 目录的弹层里仍处于选中态）。
 *
 * 仍未覆盖：真实 expo-router 的 URL 同步与浏览器历史——替身只提供 `push`/`back` 的内存栈。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  DIRECTORY_ID_BETA,
  DIRECTORY_ROUTES,
  DIRECTORY_PATH_BETA,
  DIRS_ROUTE,
  SESSION_ALPHA,
  SESSION_BETA,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  seriesSessionId,
  sessionSeries,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import { typeIntoInput, waitForDirectoryList, withScenario } from "./r15-directory";

/**
 * 夹具里 alpha 目录的服务端下发路径（`WORKSPACES_WITH_PATHS[0].path`，逐字）。
 *
 * 夹具的目录条目只含协议字段，但这里仍按运行时收窄读出那一个字符串：宁可抛错说
 * 「夹具没有下发路径」，也不要在断言里读到一个未校验的形状。
 */
function fixturePathOf(entry: unknown): string {
  if (typeof entry !== "object" || entry === null || !("path" in entry) || typeof entry.path !== "string") {
    throw new Error("夹具 WORKSPACES_WITH_PATHS[0] 没有下发 path，无法断言逐字透传");
  }
  return entry.path;
}

const ALPHA_PATH = fixturePathOf(WORKSPACES_WITH_PATHS[0]);

/** 详情页场景：默认进入 alpha 的详情页，其余与目录页的场景同源。 */
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
    route: dirRoute(DIRECTORY_ID_ALPHA),
    ...overrides,
  };
}

/** 宽度两档都**显式**给值：不依赖跑用例时的真实窗口大小。 */
const WIDE_VIEWPORT_WIDTH = 1200;
const NARROW_VIEWPORT_WIDTH = 360;

/**
 * `count` 条 `alpha` 下的会话，每条比上一条早一分钟（最新在前）。
 *
 * 倒序即编号顺序，于是「第 N 页是哪几条」在断言里是可预期的：第 1 页是 `会话 1`…`会话 10`，
 * 第 3 页是 `会话 21`…`会话 25`。
 */
function alphaSeries(count: number) {
  const base = Date.parse("2026-10-05T09:00:00.000Z");
  return sessionSeries({
    directoryId: DIRECTORY_ID_ALPHA,
    idPrefix: "aaaaaaaa",
    titles: Array.from({ length: count }, (_, index) => `会话 ${String(index + 1)}`),
    at: Array.from({ length: count }, (_, index) => new Date(base - index * 60_000).toISOString()),
    states: Array.from({ length: count }, (_, index) => (index % 3 === 0 ? "running" : "idle")),
  });
}

/** 等到真正的详情页模型渲染出来（占位 `RuntimeUnavailable` 也发 `data-route`，不能拿它等）。 */
async function waitForDirectoryDetail(container: HTMLElement): Promise<void> {
  await waitFor(
    () => query(container, '[data-route="dir-detail"][data-directory-id]') !== null,
    "目录详情页",
  );
}

/** 详情页的会话行 id，按 DOM 顺序（= 视图模型的顺序）。 */
function rowIdsOf(container: HTMLElement): readonly string[] {
  return queryAll(container, "[data-session-id]").map((node) => node.getAttribute("data-session-id") ?? "");
}

/** 会话数芯片上的数字（「N 个会话」的那个 N，取自视图模型的 `total`）。 */
function chipOf(container: HTMLElement): string | null {
  return query(container, "[data-directory-session-count]")?.getAttribute("data-directory-session-count") ?? null;
}

/** 页码输入框的**机器锚点**：恒是模型给出的生效页码（正在编辑时它与 DOM 里的当前值不同）。 */
function pageValueOf(container: HTMLElement): string | null {
  return (
    query(container, "[data-directory-detail-page-input]")?.getAttribute("data-directory-detail-page-value") ??
    null
  );
}

/**
 * 页码输入框在 DOM 里的当前值。
 *
 * 输入期间是用户敲进去的**原文**（这一轮输入真的进了草稿），提交后被 React 写回成生效页码
 * （这一轮提交真的被纠正过）——两段都能拿它当同步点。
 */
function pageInputDomValueOf(container: HTMLElement): string {
  const node = query(container, "[data-directory-detail-page-input]");
  return node instanceof HTMLInputElement ? node.value : "";
}

/** 翻页区的三个机器事实：当前页、总页数、输入框回填值。 */
function pagerFactsOf(container: HTMLElement): readonly (string | null)[] {
  const pager = query(container, "[data-directory-detail-pager]");
  return [
    pager?.getAttribute("data-directory-page") ?? null,
    pager?.getAttribute("data-directory-page-count") ?? null,
    pageValueOf(container),
  ];
}

/** 翻到某一页：点「下一页」直到当前页等于 `page`（用例只关心最终落在哪一页）。 */
async function turnToPage(container: HTMLElement, page: number, label: string): Promise<void> {
  click(query(container, '[data-action="page-next"]'), label);
  await waitFor(
    () => query(container, "[data-directory-detail-pager]")?.getAttribute("data-directory-page") === String(page),
    label,
  );
}

/**
 * 在页码输入框上按一次回车（`onSubmitEditing` 是第一条提交路径）。
 *
 * `TextInput`（RNW）把 `onSubmitEditing` 挂在**原生 `keydown`** 上：`key === "Enter"`、未按住
 * shift、非多行时调用它，随后（`blurOnSubmit` 缺省 = 非多行）再排一次
 * `setTimeout(hostNode.blur, 0)`——所以「回车」实际会走提交回调两次，第二次是那次迟到的失焦。
 * 用例因此在回车之后立刻收尾，不让它落进下一次输入中间（见 R15-14 的分段说明）。
 */
function pressEnter(container: HTMLElement, selector: string, detail: string): void {
  const node = query(container, selector);
  if (node === null) throw new CheckFailure(`${detail}：找不到输入框 ${selector}`);
  node.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
}

/**
 * 让输入框失焦（`onBlur` 是第二条提交路径）。
 *
 * React 的 `onBlur` 监听的是原生 **`focusout`**，这里直接派发它：真实失焦与它走的是同一条
 * 路径，但不依赖用例先 `focus()`、也不依赖容器窗口拿到系统焦点（headless 下不保证）。
 */
function blurInput(container: HTMLElement, selector: string, detail: string): void {
  const node = query(container, selector);
  if (node === null) throw new CheckFailure(`${detail}：找不到输入框 ${selector}`);
  node.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
}

const checks: readonly Check[] = [
  {
    name: "R15-5 目录详情页头：展示名 + 会话数芯片 + 逐字路径；目录标识不出现在界面上",
    async run() {
      const mounted = await mountScenario(baseScenario({ route: dirRoute(DIRECTORY_ID_ALPHA) }));
      try {
        // 等待条件必须用只有真实 `DirectoryDetail` 才发的属性：`[data-route="dir-detail"]`
        // 同时被占位 `RuntimeUnavailable` 命中，用它等待会在首帧占位时就成立，
        // 随后的断言便与 `start()` 的 resolve 形成竞态。
        // `data-directory-id` 只在真实详情页算出模型后出现（占位不发）。
        await waitForDirectoryDetail(mounted.container);
        expectEqual(
          textOf(query(mounted.container, "[data-directory-display-name]")),
          "api",
          "详情页的目录展示名（路径尾段）",
        );
        // 芯片说的是**全部**会话数（不是本页条数）。
        expectEqual(chipOf(mounted.container), "1", "会话数芯片的数字");
        expectEqual(
          textOf(query(mounted.container, "[data-directory-session-count]")),
          "1 个会话",
          "会话数芯片的文案",
        );
        // 路径逐字等于快照 `workspaces` 条目上的 `path` 键：不拼接、不规范化的证据就在这里。
        expectEqual(
          query(mounted.container, "[data-directory-path]")?.getAttribute("data-directory-path") ?? null,
          ALPHA_PATH,
          "路径逐字来自服务端下发",
        );
        expectThat(domText(mounted.container).includes(ALPHA_PATH), "下发的路径必须真的呈现出来");
        // 目录标识只留在机器锚点上：可见文本里不得出现。
        expectAbsent(domText(mounted.container), DIRECTORY_ID_ALPHA, "详情页渲染出的文字里不得出现目录标识");
        expectEqual(queryAll(mounted.container, "[data-session-id]").length, 1, "详情页应列出该目录下的会话");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R15-6 目录存在但没有会话：详情页必须呈现「还没有会话」，不得说成「链接失效」，翻页固定渲染但禁用",
    async run() {
      await withScenario(baseScenario({ sessions: [], route: dirRoute(DIRECTORY_ID_ALPHA) }), async (container) => {
        // 同 R15-5：等待条件不能是 `[data-route="dir-detail"]`（占位也发它），
        // 否则断言可能在页面还停在占位时执行，红法就变成时序问题而不是本用例要抓的缺陷。
        await waitForDirectoryDetail(container);
        expectThat(
          textOf(query(container, '[data-directory-sessions="empty"]')).length > 0,
          "目录存在但无会话时必须给出「还没有会话」",
        );
        expectThat(
          query(container, '[data-directory-detail-state="unknown_directory"]') === null,
          "目录存在时不得呈现「链接失效」",
        );
        // 一条会话都没有时总页数是 1：翻页区固定呈现，两个按钮禁用。
        expectEqual(pagerFactsOf(container), ["1", "1", "1"], "空目录 页码/总页数/输入框回填值");
        expectThat(
          query(container, '[data-action="page-prev"]')?.hasAttribute("disabled") === true,
          "空目录时「上一页」必须禁用",
        );
        expectThat(
          query(container, '[data-action="page-next"]')?.hasAttribute("disabled") === true,
          "空目录时「下一页」必须禁用",
        );
      });
    },
  },
  {
    name: "R15-7 目录标识失效：详情页呈现「不在目录列表里」，不得说成「还没有会话」，且仍能返回",
    async run() {
      const ghostId = "ffffffffffffffffffffffffffffffff";
      await withScenario(baseScenario({ route: dirRoute(ghostId) }), async (container) => {
        await waitFor(
          () => query(container, '[data-directory-detail-state="unknown_directory"]') !== null,
          "失效目录标识的详情页",
        );
        expectThat(
          query(container, '[data-directory-sessions="empty"]') === null,
          "目录标识失效时不得呈现「还没有会话」",
        );
        // 目录标识是路由与命令引用的内部标识，界面上不得露出来：提示文案只说「这个目录」。
        // 反例：把提示写回「目录「ffff…」不在…」时这条会红。
        expectAbsent(domText(container), ghostId, "失效目录标识的详情页不得把标识写进界面文字");
        // 自建导航删掉之后，返回入口只在这个页头里：失效标识下它也必须可用，否则用户走进死路。
        expectThat(
          query(container, '[data-action="back-to-dirs"]') !== null,
          "失效目录标识的详情页必须仍有返回入口",
        );
      });
    },
  },
  {
    name: "R15-13 目录详情平铺分页：每页固定条数、最新的一页在最前，并呈现当前页码与总页数",
    async run() {
      // 25 条会话：三页（10/10/5），各页的会话集合互不相同。
      await withScenario(baseScenario({ sessions: alphaSeries(25) }), async (container) => {
        await waitForDirectoryDetail(container);
        // 第 1 页是最新的十条（夹具里编号越小越新）。
        expectEqual(
          rowIdsOf(container),
          [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((index) => seriesSessionId("aaaaaaaa", index)),
          "第 1 页的十条会话",
        );
        expectEqual(chipOf(container), "25", "会话数芯片是全部会话数，不是本页条数");
        expectEqual(pagerFactsOf(container), ["1", "3", "1"], "第 1 页的 页码/总页数/输入框回填值");
        // 边界：第 1 页的「上一页」不可用，「下一页」可用。
        expectEqual(
          query(container, '[data-action="page-prev"]')?.hasAttribute("disabled") ?? null,
          true,
          "第 1 页的「上一页」必须不可用",
        );
        expectEqual(
          query(container, '[data-action="page-next"]')?.hasAttribute("disabled") ?? null,
          false,
          "第 1 页的「下一页」必须可用",
        );

        // 翻到第 2 页：换一批会话、页码跟着走，芯片（全部会话数）不变。
        await turnToPage(container, 2, "翻到第 2 页");
        expectEqual(
          rowIdsOf(container),
          [10, 11, 12, 13, 14, 15, 16, 17, 18, 19].map((index) => seriesSessionId("aaaaaaaa", index)),
          "第 2 页的十条会话",
        );
        expectEqual(pagerFactsOf(container), ["2", "3", "2"], "第 2 页的 页码/总页数/输入框回填值");
        expectEqual(chipOf(container), "25", "翻页不得改变会话数芯片");

        // 最后一页不满也照常呈现，且「下一页」在边界上不可用。
        await turnToPage(container, 3, "翻到第 3 页");
        expectEqual(
          rowIdsOf(container),
          [20, 21, 22, 23, 24].map((index) => seriesSessionId("aaaaaaaa", index)),
          "第 3 页（最后一页不满）的会话",
        );
        expectEqual(
          query(container, '[data-action="page-next"]')?.hasAttribute("disabled") ?? null,
          true,
          "最后一页的「下一页」必须不可用",
        );

        // 回到上一页：内容回到该页的十条。
        click(query(container, '[data-action="page-prev"]'), "「上一页」");
        await waitFor(
          () => query(container, "[data-directory-detail-pager]")?.getAttribute("data-directory-page") === "2",
          "回到第 2 页",
        );
        expectEqual(
          rowIdsOf(container),
          [10, 11, 12, 13, 14, 15, 16, 17, 18, 19].map((index) => seriesSessionId("aaaaaaaa", index)),
          "回到第 2 页后重新列出该页的十条",
        );
      });
    },
  },
  {
    name: "R15-14 非法页码只在提交时就地纠正：输入期间不重画列表，清空重输也走得通",
    async run() {
      // 三段各自「输入 → 提交」后立刻收尾，中途不再敲键：RNW 的 `TextInput` 在回车提交后会
      // 自己排一次 `setTimeout(() => hostNode.blur(), 0)`（见 `pressEnter` 的说明），紧挨着敲键
      // 会让那次失焦带着新草稿提交，把用例想观测的「输入期间」提前结束。

      // 第一段：草稿只在输入里长，回车才提交——「清空重输」是这条用例的落点。
      await withScenario(baseScenario({ sessions: alphaSeries(25) }), async (container) => {
        await waitForDirectoryDetail(container);
        await turnToPage(container, 2, "翻到第 2 页");
        const secondPage = rowIdsOf(container);
        const firstRowBefore = query(container, "[data-session-id]");
        expectEqual(secondPage.length, 10, "第 2 页的条数");

        // 输入期间：输入框里就是敲进去的原文，页码与列表一个都没动。
        // 反例（本轮修复前 `onChangeText` 直连提交）：第一个字符就把纠正值写回 DOM，输入框
        // 里根本看不到 `abc`（它就是 2），下面这条等待会超时。
        typeIntoInput(container, "[data-directory-detail-page-input]", "abc", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "abc", "输入期间输入框里应是原文");
        expectEqual(pagerFactsOf(container), ["2", "3", "2"], "输入期间不得提交页码");
        expectEqual(rowIdsOf(container), secondPage, "输入期间列表内容不得变");
        // 比「id 集合相同」更强：会话行**没有重挂**（同一个 DOM 节点还在原位），也就是没有重画。
        expectEqual(
          query(container, "[data-session-id]") === firstRowBefore,
          true,
          "输入期间会话行不得重挂",
        );

        // 清空重输：输入框必须真的能空着（每键都提交的实现会立刻把纠正值写回，永远清不空），
        // 接着敲的 `1` 也不能被残留数字拼成 `21`。
        typeIntoInput(container, "[data-directory-detail-page-input]", "", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "", "输入框必须能被清空");
        typeIntoInput(container, "[data-directory-detail-page-input]", "1", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "1", "清空后重新敲入的原文");
        expectEqual(pagerFactsOf(container), ["2", "3", "2"], "清空重输期间页码仍是第 2 页");
        expectEqual(rowIdsOf(container), secondPage, "清空重输期间列表内容不得变");

        // 回车提交「1」：落到第 1 页（反例：清不空时这里提交的是 `21`，会被夹到最后一页）。
        pressEnter(container, "[data-directory-detail-page-input]", "页码输入框");
        await waitFor(
          () => query(container, "[data-directory-detail-pager]")?.getAttribute("data-directory-page") === "1",
          "回车后落到第 1 页",
        );
        expectEqual(pagerFactsOf(container), ["1", "3", "1"], "回车后 页码/总页数/输入框回填值");
        expectEqual(
          rowIdsOf(container),
          [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((index) => seriesSessionId("aaaaaaaa", index)),
          "回车后呈现第 1 页的十条",
        );
      });

      // 第二段：非数字在回车后就地纠正回**当前生效页**，且列表逐字不变、会话行不重挂。
      await withScenario(baseScenario({ sessions: alphaSeries(25) }), async (container) => {
        await waitForDirectoryDetail(container);
        await turnToPage(container, 2, "翻到第 2 页");
        const secondPage = rowIdsOf(container);
        const firstRowBefore = query(container, "[data-session-id]");

        typeIntoInput(container, "[data-directory-detail-page-input]", "abc", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "abc", "输入期间输入框里应是原文");
        pressEnter(container, "[data-directory-detail-page-input]", "页码输入框");
        // 等待条件取 DOM 里的**输入值**：它被回填成 2 才说明这次提交真的被处理过
        // （否则下面「列表没变」只是「事件没生效」的假象）。
        await waitFor(() => pageInputDomValueOf(container) === "2", "非数字被就地纠正回当前页");
        expectEqual(pageValueOf(container), "2", "非数字提交后输入框回填的值");
        expectEqual(pagerFactsOf(container), ["2", "3", "2"], "非数字不得翻页");
        expectEqual(rowIdsOf(container), secondPage, "非数字不得重画列表");
        expectEqual(
          query(container, "[data-session-id]") === firstRowBefore,
          true,
          "非数字不得重挂会话行",
        );
      });

      // 第三段：**失焦**是第二条提交路径（原型把 `commitPage` 也挂在 `onchange` 上）：不提交
      // 就整段不动，提交后越界值夹到范围边界，输入框与列表同时落到那里。
      await withScenario(baseScenario({ sessions: alphaSeries(25) }), async (container) => {
        await waitForDirectoryDetail(container);
        await turnToPage(container, 2, "翻到第 2 页");
        const secondPage = rowIdsOf(container);

        typeIntoInput(container, "[data-directory-detail-page-input]", "99", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "99", "失焦前输入框里应是原文");
        expectEqual(pagerFactsOf(container), ["2", "3", "2"], "失焦前不得提交页码");
        expectEqual(rowIdsOf(container), secondPage, "失焦前列表内容不得变");
        blurInput(container, "[data-directory-detail-page-input]", "页码输入框");
        await waitFor(() => pageInputDomValueOf(container) === "3", "越界数字被夹到边界");
        expectEqual(pagerFactsOf(container), ["3", "3", "3"], "越界后 页码/总页数/输入框回填值");
        expectEqual(
          rowIdsOf(container),
          [20, 21, 22, 23, 24].map((index) => seriesSessionId("aaaaaaaa", index)),
          "越界后呈现最后一页",
        );
      });
    },
  },
  {
    name: "R15-15 只有一页时翻页固定呈现且禁用；目录页的查询词与筛选也不进入详情页",
    async run() {
      // 3 条会话：一页即可放下 → 翻页仍渲染，但上/下一页都禁用。
      await withScenario(baseScenario({ sessions: alphaSeries(3) }), async (container) => {
        await waitForDirectoryDetail(container);
        expectEqual(rowIdsOf(container).length, 3, "一页放得下时全部会话都在");
        expectEqual(chipOf(container), "3", "会话数芯片");
        expectEqual(pagerFactsOf(container), ["1", "1", "1"], "单页 页码/总页数/输入框回填值");
        expectThat(
          query(container, '[data-action="page-prev"]')?.hasAttribute("disabled") === true,
          "只有一页时「上一页」必须禁用",
        );
        expectThat(
          query(container, '[data-action="page-next"]')?.hasAttribute("disabled") === true,
          "只有一页时「下一页」必须禁用",
        );
        // 详情页没有目录内搜索、也没有月份分段控件（两个需求都已从规格里移除）。
        expectThat(
          query(container, "[data-directory-detail-search]") === null,
          "详情页不得再呈现目录内搜索框",
        );
        expectThat(query(container, '[data-segment="月份"]') === null, "详情页不得再呈现月份分段控件");
      });

      // 目录页的查询词只属于目录页：写进去之后详情页的**模型与渲染**都一个样。
      const sessions = alphaSeries(3);
      const detail = await mountScenario(baseScenario({ sessions }));
      try {
        await waitForDirectoryDetail(detail.container);
        const before = rowIdsOf(detail.container);
        // 「会话 1」只命中 alpha 的第一条：若目录页的查询词漏进详情页，这里会从 3 变 1。
        detail.composition?.store.setDirectoryQuery("会话 1");
        // 模型层直接读（时序无关）：详情模型只吃「快照 + 目录标识 + 页码」。
        const model = detail.composition?.store.directoryDetail(DIRECTORY_ID_ALPHA);
        expectThat(model?.kind === "ready", "详情模型已就绪");
        expectEqual(
          model?.kind === "ready" ? model.sessions.length : -1,
          3,
          "目录页的查询词不得改变详情模型的行数",
        );
        // 渲染层：仓库写入会触发页面重渲染，重渲染后的行集合仍与之前逐字相同。
        expectEqual(rowIdsOf(detail.container), before, "目录页的查询词不得改变详情页的会话行");
        expectEqual(chipOf(detail.container), "3", "详情页的芯片只由它自己的模型决定");
      } finally {
        await detail.unmount();
      }

      // 反向：详情页翻到的页码不进入目录页，目录页的内联会话行一条不少。
      // alpha 有 25 条（三页）：页码一旦泄漏过来，内联行会变成「第 2 页那一段」而不是同一批。
      const dirs = await mountScenario(baseScenario({ route: DIRS_ROUTE, sessions: alphaSeries(25) }));
      try {
        await waitForDirectoryList(dirs.container);
        const before = queryAll(dirs.container, "[data-session-id]").map((node) =>
          node.getAttribute("data-session-id"),
        );
        expectThat(before.length > 0, "前置：目录页一开始列出 alpha 的内联会话行");
        dirs.composition?.store.setDirectoryDetailPage(DIRECTORY_ID_ALPHA, "2");
        // 用目录页自己的查询词做一次**可见**的改动，既是同步点也是对照：「会话」只命中
        // alpha（它的会话标题命中），0 会话的 beta 被筛掉。
        typeIntoInput(dirs.container, "[data-directory-search]", "会话", "目录页搜索框");
        await waitFor(
          () => query(dirs.container, '[data-directory-search-value="会话"]') !== null,
          "目录页自己的查询词生效",
        );
        expectEqual(
          queryAll(dirs.container, "[data-directory-id]").map((node) =>
            node.getAttribute("data-directory-id"),
          ),
          [DIRECTORY_ID_ALPHA],
          "目录页的目录集合只由它自己的查询词决定",
        );
        expectEqual(
          queryAll(dirs.container, "[data-session-id]").map((node) => node.getAttribute("data-session-id")),
          before,
          "目录页的内联会话行不得被详情页的页码（以及它带出的切片）改变",
        );
      } finally {
        await dirs.unmount();
      }
    },
  },
  {
    name: "R15-16 窄视口：会话行、会话数芯片、创建入口与翻页一个不少，data-* 集合与宽视口逐项相同",
    async run() {
      const sessions = alphaSeries(25);
      const wide = await factsAtViewportWidth(WIDE_VIEWPORT_WIDTH, baseScenario({ sessions }));
      const narrow = await factsAtViewportWidth(NARROW_VIEWPORT_WIDTH, baseScenario({ sessions }));

      // 前置：两档真的走了不同的布局分支（底栏的 `flex-direction` 由 row 变 column）。
      // 视口覆盖没生效时这里就会红，读作「这条用例没有在测窄屏」，而不是被测行为回退。
      expectEqual(wide.footbarDirection, "row", "宽档必须走宽屏布局");
      expectEqual(narrow.footbarDirection, "column", "窄档必须走单列布局");

      // 信息集合：两档的 `data-*` 锚点逐项相同，一个都不少、一个也不多。
      expectEqual(narrow.dataAttributes, wide.dataAttributes, "两档的 data-* 集合必须完全相同");
      expectEqual(narrow.rowIds.length, 10, "窄档仍然呈现当前页的会话行");
      expectEqual(narrow.rowIds, wide.rowIds, "两档呈现的是同一页的同一批会话");
      expectEqual(narrow.chip, "25", "窄档仍然呈现会话数芯片");
      expectThat(narrow.hasCreateEntry, "窄档仍然呈现创建会话的入口");
      expectThat(!narrow.hasCreateSideEffect, "详情页不再呈现创建会话的副作用说明");
      expectThat(narrow.hasPager, "窄档仍然呈现翻页（MUST NOT 因宽度不足改变判定结果）");
      expectEqual(narrow.pagerFacts, ["1", "3", "1"], "窄档的 页码/总页数/输入框回填值");
    },
  },
  {
    name: "R15-17 详情页的同步时刻：在线时不标注，断连后标注那次快照的时刻，从未同步时一个都不标",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForDirectoryDetail(mounted.container);
        expectThat(
          query(mounted.container, '[data-content-origin="live"]') !== null,
          "在线时来源标注必须是「本次同步」",
        );
        expectThat(
          query(mounted.container, '[data-origin-notice="last_sync"]') === null,
          "在线时不得呈现断连提示条",
        );
        expectAbsent(domText(mounted.container), "内容来自本次同步", "在线时不得呈现来源提示行");
        expectThat(
          query(mounted.container, "[data-synced-at]") === null,
          "内容来自本次同步时不得标注「已同步 …」时刻",
        );

        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后详情页标注 last_sync",
        );
        await waitFor(
          () => query(mounted.container, '[data-origin-notice="last_sync"]') !== null,
          "断连后详情页呈现来源提示条",
        );
        expectThat(
          domText(mounted.container).includes("内容来自上次同步"),
          "断连提示条要说明内容来自上次同步",
        );
        await waitFor(
          () => query(mounted.container, "[data-synced-at]") !== null,
          "断连后标注上次同步时刻",
        );
        const epochMs = query(mounted.container, "[data-synced-at]")?.getAttribute("data-synced-at") ?? "";
        expectThat(
          /^\d{13}$/.test(epochMs) && Number(epochMs) > 0,
          `同步时刻应是快照替换时的 epoch 毫秒，实际 ${JSON.stringify(epochMs)}`,
        );
        expectEqual(
          textOf(query(mounted.container, "[data-synced-at]")),
          `已同步 ${new Date(Number(epochMs)).toISOString().slice(0, 16).replace("T", " ")} UTC`,
          "同步时刻的展示文本（与目录页同一套文案）",
        );
      } finally {
        await mounted.unmount();
      }

      // 从未成功替换过快照：详情页停在等待同步，一个同步时刻都不得出现。
      // 等待条件用详情页自己的 loading 锚点：占位 `RuntimeUnavailable` 不发它，因此它出现即
      // 「真实组件已渲染」——自建导航与连接状态徽标移除后，这一页不再有别的同步点可用。
      await withScenario(baseScenario({ deliverSnapshot: false }), async (container) => {
        await waitFor(
          () => query(container, '[data-directory-detail-state="loading"]') !== null,
          "从未同步过时详情页呈现等待同步的状态",
        );
        expectThat(
          query(container, "[data-synced-at]") === null,
          "从未成功替换过快照时不得呈现任何同步时刻",
        );
      });
    },
  },
  {
    name: "R15-18 翻页后回到页首：内容滚动位置回到该页顶部",
    async run() {
      const mounted = await mountScenario(baseScenario({ sessions: alphaSeries(25) }));
      try {
        await waitForDirectoryDetail(mounted.container);
        // 把挂载容器变成唯一可滚动的祖先（外壳不在本替身里）：滚动位置于是有一个可断言的落点。
        const scroller = mounted.container;
        scroller.style.height = "220px";
        scroller.style.overflowY = "auto";
        await waitFor(() => scroller.scrollHeight > scroller.clientHeight, "列表比容器高（可滚动）");
        expectEqual(pagerFactsOf(scroller)[0], "1", "前置：停在第一页");

        // 先真的翻一次页（第 1 → 2 页），再模拟「用户往下滚了一段」。
        await turnToPage(scroller, 2, "翻到第 2 页");
        scroller.scrollTop = 120;
        expectThat(scroller.scrollTop > 0, "前置：滚动位置必须真的离开顶部，否则本用例假绿");

        // 再翻一页：内容回到该页顶部。
        await turnToPage(scroller, 3, "翻到第 3 页");
        await waitFor(() => scroller.scrollTop === 0, "翻页后内容回到该页顶部");
        expectEqual(scroller.scrollTop, 0, "翻页后的滚动位置");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R15-21 跨路由：卡片标题与「查看全部 N 个」两条入口都进入**该目录**的详情页",
    async run() {
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES }),
        async (container) => {
          await waitForDirectoryList(container);

          // 入口一：卡片**标题**（与会话数无关的那个）。
          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"] [data-action="open-directory"]`),
            "alpha 的卡片标题入口",
          );
          await waitFor(
            () => query(container, `[data-route="dir-detail"][data-directory-id="${DIRECTORY_ID_ALPHA}"]`) !== null,
            "标题入口进入 alpha 的详情页",
          );
          // 进入的是**那一个**目录：展示名、路径与它名下的会话都要对得上（走错目录/没带参数都会红）。
          expectEqual(
            textOf(query(container, "[data-directory-display-name]")),
            "api",
            "详情页的目录展示名",
          );
          expectEqual(
            query(container, "[data-directory-path]")?.getAttribute("data-directory-path") ?? null,
            ALPHA_PATH,
            "进入的是 alpha 的路径",
          );
          expectEqual(rowIdsOf(container), [SESSION_ALPHA], "详情页列出的是 alpha 名下的会话");

          // 返回目录页，再从另一个入口进另一个目录：两条入口不能串到同一目录上。
          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitForDirectoryList(container);
          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_BETA}"] [data-action="open-directory-all"]`),
            "beta 的「查看全部」入口",
          );
          await waitFor(
            () => query(container, `[data-route="dir-detail"][data-directory-id="${DIRECTORY_ID_BETA}"]`) !== null,
            "「查看全部」入口进入 beta 的详情页",
          );
          expectEqual(
            query(container, "[data-directory-path]")?.getAttribute("data-directory-path") ?? null,
            DIRECTORY_PATH_BETA,
            "进入的是 beta 的路径",
          );
          expectEqual(rowIdsOf(container), [SESSION_BETA], "详情页列出的是 beta 名下的会话");
        },
      );
    },
  },
  {
    name: "R15-22 跨路由：返回目录页后目录页的查询词仍在，命中集合也仍是那次搜索的结果",
    async run() {
      // 「部署」只出现在 alpha 的会话标题里（两条目录的展示名都是 api、目录标识也不含这两个字）。
      const sessions = [
        ...sessionSeries({
          directoryId: DIRECTORY_ID_ALPHA,
          idPrefix: "aaaaaaaa",
          titles: ["部署检查"],
          at: ["2026-10-05T09:00:00.000Z"],
          states: ["running"],
        }),
        ...sessionSeries({
          directoryId: DIRECTORY_ID_BETA,
          idPrefix: "bbbbbbbb",
          titles: ["文档整理"],
          at: ["2026-10-03T09:00:00.000Z"],
          states: ["pending"],
        }),
      ];
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions }),
        async (container) => {
          await waitForDirectoryList(container);
          expectEqual(queryAll(container, "[data-directory-id]").length, 2, "初始两条目录项");

          typeIntoInput(container, "[data-directory-search]", "部署", "目录页搜索框");
          await waitFor(
            () => queryAll(container, "[data-directory-id]").length === 1,
            "按会话标题搜索后只剩 alpha",
          );
          expectEqual(
            query(container, "[data-directory-id]")?.getAttribute("data-directory-id") ?? null,
            DIRECTORY_ID_ALPHA,
            "命中的是 alpha",
          );

          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"] [data-action="open-directory"]`),
            "进入 alpha 的详情页",
          );
          await waitForDirectoryDetail(container);

          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitForDirectoryList(container);

          // 查询词住在 store 里，因此返回时（页面是重挂的）它仍在——写成组件局部状态就会在这里红。
          expectEqual(
            query(container, "[data-directory-search]")?.getAttribute("data-directory-search-value"),
            "部署",
            "返回目录页后受控搜索框里的查询词",
          );
          // 而且列表**确实**仍被它筛过：只留文字、列表已复原的写法在这里红。
          expectEqual(
            queryAll(container, "[data-directory-id]").map((node) =>
              node.getAttribute("data-directory-id"),
            ),
            [DIRECTORY_ID_ALPHA],
            "返回目录页后命中集合仍是那次搜索的结果",
          );
        },
      );
    },
  },
  {
    name: "R15-23 零会话（全部目录都没有会话）：空态与目录项同时呈现，且每一项都能进入详情页并看到创建入口",
    async run() {
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions: [] }),
        async (container) => {
          await waitFor(() => query(container, "[data-empty-state]") !== null, "空态渲染");
          expectEqual(
            query(container, "[data-empty-state]")?.getAttribute("data-empty-state") ?? null,
            "directories_without_sessions",
            "空态标识",
          );
          // 空态与目录列表**同时**存在：空态指引写着「在下面选一个目录」，列表就必须真的在。
          expectThat(
            query(container, '[data-directory-list="directories"]') !== null,
            "空态下必须仍然列出目录项（否则指引指向一个不存在的列表）",
          );
          expectEqual(queryAll(container, "[data-directory-id]").length, 2, "两条目录项都要在");
          expectEqual(
            queryAll(container, '[data-action="open-directory"]').length,
            2,
            "每条目录项都要有进入详情页的标题入口",
          );
          expectEqual(
            queryAll(container, '[data-action="open-directory-all"]').length,
            0,
            "一条会话都没有时不得呈现「查看全部 N 个」",
          );

          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"] [data-action="open-directory"]`),
            "alpha 的卡片标题入口",
          );
          await waitFor(
            () => query(container, `[data-route="dir-detail"][data-directory-id="${DIRECTORY_ID_ALPHA}"]`) !== null,
            "零会话的目录也能进入详情页",
          );
          expectThat(
            query(container, '[data-directory-sessions="empty"]') !== null,
            "详情页必须说明该目录里还没有会话",
          );
          expectThat(
            query(container, '[data-directory-detail-state="unknown_directory"]') === null,
            "目录存在时不得呈现「链接失效」",
          );
          // 死路的终点就是这个入口：一台新机器上的第一个会话只能从这里创建。
          expectThat(
            query(container, '[data-action="open-create-sheet"]') !== null,
            "零会话的目录详情页必须有创建会话入口",
          );
        },
      );
    },
  },
  {
    name: "R15-24 零会话（部分目录没有会话）：0 会话的目录项仍有标题入口、没有「查看全部」，且能进入它的详情页",
    async run() {
      const sessions = sessionSeries({
        directoryId: DIRECTORY_ID_ALPHA,
        idPrefix: "aaaaaaaa",
        titles: ["部署检查"],
        at: ["2026-10-05T09:00:00.000Z"],
        states: ["running"],
      });
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions }),
        async (container) => {
          await waitForDirectoryList(container);
          const beta = query(container, `[data-directory-id="${DIRECTORY_ID_BETA}"]`);
          expectThat(beta !== null, "0 会话的目录也要作为目录项呈现");
          // 0 会话时只有「会话 0」那一个徽标在（原型的运行中/待处理徽标只在非零时渲染）。
          expectEqual(
            textOf(query(beta, '[data-directory-count="total"]')),
            "0",
            "beta 的会话数为零",
          );
          expectEqual(
            ["active", "pending"].map((kind) => textOf(query(beta, `[data-directory-count="${kind}"]`))),
            ["", ""],
            "零值时不呈现运行中/待处理的徽标",
          );
          expectThat(
            query(beta, '[data-action="open-directory"]') !== null,
            "0 会话的目录项必须仍有标题入口",
          );
          expectThat(
            query(beta, '[data-action="open-directory-all"]') === null,
            "0 会话的目录项不得呈现「查看全部 N 个」",
          );
          expectThat(
            query(beta, "[data-directory-latest-at]") === null,
            "0 会话的目录项不得呈现活动时间",
          );

          click(query(beta, '[data-action="open-directory"]'), "beta 的卡片标题入口");
          await waitFor(
            () => query(container, `[data-route="dir-detail"][data-directory-id="${DIRECTORY_ID_BETA}"]`) !== null,
            "进入 beta 的详情页",
          );
          expectThat(
            query(container, '[data-directory-sessions="empty"]') !== null,
            "beta 的详情页必须说明该目录里还没有会话",
          );
          expectThat(
            query(container, '[data-directory-detail-state="unknown_directory"]') === null,
            "目录存在时不得呈现「链接失效」",
          );
          expectThat(
            query(container, '[data-action="open-create-sheet"]') !== null,
            "beta 的详情页必须有创建会话入口",
          );
        },
      );
    },
  },
  {
    name: "R15-25 跨路由：从第 2 页返回、再进入同一个目录时页码复位到第一页",
    async run() {
      // alpha 有 25 条（三页）、beta 一条：页码是**单份**的 store 状态，不复位就会带进下一屏。
      const sessions = [
        ...alphaSeries(25),
        ...sessionSeries({
          directoryId: DIRECTORY_ID_BETA,
          idPrefix: "bbbbbbbb",
          titles: ["文档整理"],
          at: ["2026-10-03T09:00:00.000Z"],
          states: ["pending"],
        }),
      ];
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions }),
        async (container) => {
          await waitForDirectoryList(container);

          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"] [data-action="open-directory"]`),
            "进入 alpha 的详情页",
          );
          await waitForDirectoryDetail(container);
          expectEqual(pagerFactsOf(container), ["1", "3", "1"], "首次进入 alpha 的页码");
          await turnToPage(container, 2, "翻到第 2 页");
          const secondPage = rowIdsOf(container);

          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitForDirectoryList(container);

          // 先从另一个目录进一次（页码同样必须复位），再回到 alpha：不能是「上一次那页」。
          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_BETA}"] [data-action="open-directory"]`),
            "进入 beta 的详情页",
          );
          await waitFor(
            () => query(container, `[data-route="dir-detail"][data-directory-id="${DIRECTORY_ID_BETA}"]`) !== null,
            "beta 的详情页",
          );
          expectEqual(chipOf(container), "1", "beta 只有一条会话");
          expectEqual(pagerFactsOf(container), ["1", "1", "1"], "beta 单页也固定呈现翻页");
          expectThat(
            query(container, '[data-action="page-next"]')?.hasAttribute("disabled") === true,
            "beta 只有一页时「下一页」禁用",
          );

          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitForDirectoryList(container);
          click(
            query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"] [data-action="open-directory"]`),
            "再次进入 alpha 的详情页",
          );
          await waitForDirectoryDetail(container);

          // 反例：不复位页码时会停在上一次的第 2 页（`secondPage`），第 1 页的十条一条都不在。
          await waitFor(
            () =>
              query(container, "[data-directory-detail-pager]")?.getAttribute("data-directory-page") === "1",
            "再次进入 alpha 时回到第 1 页",
          );
          expectEqual(
            rowIdsOf(container),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map((index) => seriesSessionId("aaaaaaaa", index)),
            "再次进入时呈现第 1 页",
          );
          expectThat(
            rowIdsOf(container).join(",") !== secondPage.join(","),
            "前置：第 1 页与第 2 页必须是不同的会话集合",
          );
        },
      );
    },
  },
  {
    name: "R15-26 直接打开详情页：没有应用内历史时「返回目录」仍导航到目录页",
    async run() {
      await withScenario(
        baseScenario({ route: dirRoute(DIRECTORY_ID_ALPHA), extraRoutes: [DIRS_ROUTE] }),
        async (container) => {
          await waitForDirectoryDetail(container);
          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitFor(
            () => query(container, '[data-route="dirs"]') !== null,
            "直接打开详情页时返回目录应落到目录页",
          );
          expectEqual(
            queryAll(container, '[data-route="dir-detail"]').length,
            0,
            "返回后不得仍停留在详情页",
          );
        },
      );
    },
  },
];

/** 一次挂载里读到的、与宽度无关的全部事实。 */
interface DetailFacts {
  /** 渲染结果里每条 `data-*` 的 `名字=值`（同步时刻只记名字：它是挂载那一刻的 epoch）。 */
  readonly dataAttributes: readonly string[];
  /** 底栏的 `flex-direction`：`row` = 宽屏分支，`column` = 窄屏分支。 */
  readonly footbarDirection: string;
  readonly rowIds: readonly string[];
  readonly chip: string | null;
  readonly pagerFacts: readonly (string | null)[];
  readonly hasCreateEntry: boolean;
  readonly hasCreateSideEffect: boolean;
  readonly hasPager: boolean;
}

function detailFactsOf(container: HTMLElement): DetailFacts {
  const entries: string[] = [];
  for (const node of [container, ...queryAll(container, "*")]) {
    for (const name of node.getAttributeNames()) {
      if (!name.startsWith("data-")) continue;
      const value = name === "data-synced-at" ? "<epoch>" : (node.getAttribute(name) ?? "");
      entries.push(`${name}=${value}`);
    }
  }
  const pager = query(container, "[data-directory-detail-pager]");
  return {
    dataAttributes: entries.sort(),
    footbarDirection: pager === null ? "" : getComputedStyle(pager.parentElement ?? pager).flexDirection,
    rowIds: rowIdsOf(container),
    chip: chipOf(container),
    pagerFacts: pagerFactsOf(container),
    hasCreateEntry: query(container, '[data-action="open-create-sheet"]') !== null,
    hasCreateSideEffect: query(container, "[data-create-side-effect]") !== null,
    hasPager: pager !== null,
  };
}

/**
 * 把 RNW 读到的视口宽度改成 `width`，并触发一次 `resize` 让它重新取值；返回恢复函数。
 *
 * RNW 的 `Dimensions` 在浏览器里**只**从 `window.visualViewport`（存在时）或
 * `document.documentElement.clientWidth` 取值，且拿首次取值缓存住、只在 `resize` 时刷新；
 * `Dimensions.set()` 在浏览器里直接 `invariant` 抛错。因此在页内收窄视口的唯一办法就是
 * 覆盖这两个读数并派发 `resize`——`visualViewport` 存在时 RNW 把监听挂**在它身上**，
 * 所以两个目标都要派发一次。宽度按 `devicePixelRatio` 折算，避免缩放比把宽度又乘回去。
 *
 * 恢复必须放在 `finally`：宽度是模块级缓存，漏恢复会让后面的用例全部跑在窄屏上。
 */
export function overrideViewportWidth(width: number): () => void {
  const reported = Math.max(1, Math.round(width / (window.devicePixelRatio || 1)));
  const viewport = window.visualViewport;
  const documentElement = document.documentElement;
  const ownViewportWidth = viewport === null ? undefined : Object.getOwnPropertyDescriptor(viewport, "width");
  const ownClientWidth = Object.getOwnPropertyDescriptor(documentElement, "clientWidth");
  if (viewport !== null) {
    Object.defineProperty(viewport, "width", { configurable: true, get: () => reported });
  }
  Object.defineProperty(documentElement, "clientWidth", { configurable: true, get: () => reported });
  const notify = (): void => {
    viewport?.dispatchEvent(new Event("resize"));
    window.dispatchEvent(new Event("resize"));
  };
  notify();
  return () => {
    if (viewport !== null) {
      if (ownViewportWidth === undefined) Reflect.deleteProperty(viewport, "width");
      else Object.defineProperty(viewport, "width", ownViewportWidth);
    }
    if (ownClientWidth === undefined) Reflect.deleteProperty(documentElement, "clientWidth");
    else Object.defineProperty(documentElement, "clientWidth", ownClientWidth);
    notify();
  };
}

/** 在指定视口宽度下挂一条详情页场景，读完事实后卸载并恢复宽度。 */
async function factsAtViewportWidth(width: number, options: ScenarioOptions): Promise<DetailFacts> {
  const restore = overrideViewportWidth(width);
  const mounted = await mountScenario(options);
  try {
    await waitForDirectoryDetail(mounted.container);
    return detailFactsOf(mounted.container);
  } finally {
    await mounted.unmount();
    restore();
  }
}

export const R15_DETAIL_CHECKS = checks;
