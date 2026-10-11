/**
 * R15「目录详情按月分档、目录内搜索与按宽度自适应」的浏览器级验证（详情页那一半）。
 *
 * 与 `r15-directory.ts` 同一套装配：真实路由组件 `app/dir/[alias].tsx` → 真实
 * `DirectoryDetail` → `src/features/directory-model.ts`，数据来自真实快照管线。
 * 三个共用机关（`withScenario` / `waitForDirectoryList` / `typeIntoInput`）从那里导入，
 * 不复制第二份——两页的搜索框、卸载纪律必须逐字一致。
 *
 * ## 判别力
 *
 * - 「三种空互不冒充」：别名不在快照里（`unknown_directory`）与「目录里没有会话」
 *   （`sessions="empty"`）是两个分支，而「搜索/月份没命中」是**第三支**
 *   （`data-directory-detail-no-match`）——任一支被合并进另一支，对应用例必红。
 * - 「按会话自身时间分档」：断言的是月份档的**取值与顺序**（`["all","2026-10","2026-09"]`）与
 *   切档前后 `shown`/`total` 的差；把「当前时间」当成所有会话的档、或把两个月的会话并档，
 *   在这里都会红。
 * - 「切换月份只改 shown」：断言切档后 `data-directory-shown` 变小而 `data-directory-total`
 *   不变——两个数字同源或口径互换都会在这里红。
 * - 「目录页的查询词不进入详情页」：同一次挂载里先在目录页敲查询词、再进入详情页，
 *   断言详情页的查询词为空且呈现该目录的**全部**会话。
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
 * R15-23/R15-24（零会话的两种情形都能进入详情页）四条用例可以真的走一遍跨路由。
 *
 * `key` 是必要的：同类型路由之间切换（`/dir/alpha` → `/dir/beta`）时若不重挂，上一屏的局部
 * 状态会被带进下一屏（实测：在 alpha 选过的 Agent 会在 beta 的弹层里仍处于选中态）。
 * 目录页 ↔ 详情页是类型切换，两种写法都能重挂——所以 `key` 不是那两条用例的前提，但它是
 * 「每个路由一屏」这条替身语义的一部分。
 *
 * 仍未覆盖：真实 expo-router 的 URL 同步与浏览器历史——替身只提供 `push`/`back` 的内存栈。
 */

import {
  AGENTS,
  DIRECTORY_ROUTES,
  DIRS_ROUTE,
  SECRET_FRAGMENTS,
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
  click,
  domHtml,
  expectEqual,
  expectNone,
  expectThat,
  mountScenario,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import { typeIntoInput, waitForDirectoryList, withScenario } from "./r15-directory";

/** 详情页场景：默认进入 alpha 的详情页，其余与目录页的场景同源。 */
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
    route: dirRoute("alpha"),
    ...overrides,
  };
}

/** 宽度两档都**显式**给值：不依赖跑用例时的真实窗口大小。 */
const WIDE_VIEWPORT_WIDTH = 1200;
const NARROW_VIEWPORT_WIDTH = 360;

/** 等到真正的详情页模型渲染出来（占位 `RuntimeUnavailable` 也发 `data-route`，不能拿它等）。 */
async function waitForDirectoryDetail(container: HTMLElement): Promise<void> {
  await waitFor(
    () => query(container, '[data-route="dir-detail"][data-directory-alias]') !== null,
    "目录详情页",
  );
}

/** 详情页的会话行 id，按 DOM 顺序（= 视图模型的顺序）。 */
function rowIdsOf(container: HTMLElement): readonly string[] {
  return queryAll(container, "[data-session-id]").map((node) => node.getAttribute("data-session-id") ?? "");
}

/** 详情的「当前显示 N / M」两个数字（经 `dataSet` 原样进 DOM）。 */
function shownTotalOf(container: HTMLElement): readonly (string | null)[] {
  const shown = query(container, "[data-directory-shown]");
  const total = query(container, "[data-directory-total]");
  return [
    shown?.getAttribute("data-directory-shown") ?? null,
    total?.getAttribute("data-directory-total") ?? null,
  ];
}

const checks: readonly Check[] = [
  {
    name: "R15-5 目录详情页同样不泄露路径，且列出该目录下的会话",
    async run() {
      const mounted = await mountScenario(baseScenario({ route: dirRoute("alpha") }));
      try {
        // 等待条件必须用只有真实 `DirectoryDetail` 才发的属性：`[data-route="dir-detail"]`
        // 同时被占位 `RuntimeUnavailable` 命中，用它等待会在首帧占位时就成立，
        // 随后的「展示名 / 别名」断言便与 `start()` 的 resolve 形成竞态。
        // `data-directory-alias` 只在真实详情页算出模型后出现（占位不发）。
        await waitForDirectoryDetail(mounted.container);
        expectEqual(
          textOf(query(mounted.container, "[data-directory-display-name]")),
          "api",
          "详情页的目录展示名",
        );
        expectEqual(textOf(query(mounted.container, "[data-directory-alias-label]")), "alpha", "详情页的别名");
        expectEqual(queryAll(mounted.container, "[data-session-id]").length, 1, "详情页应列出该目录下的会话");
        expectNone(domHtml(mounted.container), SECRET_FRAGMENTS, "详情页渲染结果泄露了本机路径");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R15-6 目录存在但没有会话：详情页必须呈现「还没有会话」，不得说成「链接失效」",
    async run() {
      await withScenario(baseScenario({ sessions: [], route: dirRoute("alpha") }), async (container) => {
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
      });
    },
  },
  {
    name: "R15-7 别名失效：详情页呈现「不在目录列表里」，不得说成「还没有会话」",
    async run() {
      await withScenario(baseScenario({ route: dirRoute("ghost") }), async (container) => {
        await waitFor(
          () => query(container, '[data-directory-detail-state="unknown_directory"]') !== null,
          "失效别名的详情页",
        );
        expectThat(
          query(container, '[data-directory-sessions="empty"]') === null,
          "别名失效时不得呈现「还没有会话」",
        );
      });
    },
  },
  {
    name: "R15-13 目录详情按月分档：档取值与顺序按会话自身时间，切档只改 shown 不改 total",
    async run() {
      // 两条十月、一条九月：档的取值与顺序都取自会话**自身**的时间，
      // 按「当前时间」归档只会剩一档，断言两个档的取值时必红。
      const sessions = sessionSeries({
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["十月甲", "十月乙", "九月丙"],
        at: [
          "2026-10-05T09:00:00.000Z",
          "2026-10-04T09:00:00.000Z",
          "2026-09-20T09:00:00.000Z",
        ],
        states: ["running", "idle", "waiting_input"],
      });
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryDetail(container);
        const monthOptions = queryAll(container, '[data-segment="月份"] [data-segment-option]');
        expectEqual(
          monthOptions.map((node) => node.getAttribute("data-segment-option")),
          ["all", "2026-10", "2026-09"],
          "月份档的取值与顺序（档之间按时间倒序）",
        );
        expectEqual(
          monthOptions.map((node) => node.textContent),
          ["全部", "2026-10", "2026-09"],
          "月份档的文案",
        );
        expectEqual(shownTotalOf(container), ["3", "3"], "默认呈现全部月份");
        expectEqual(rowIdsOf(container), [0, 1, 2].map((index) => seriesSessionId("aaaaaaaa", index)), "档内按时间倒序");

        click(query(container, '[data-segment="月份"] [data-segment-option="2026-09"]'), "「2026-09」月份档");
        await waitFor(
          () => shownTotalOf(container)[0] === "1",
          "切到九月档后 shown 变小",
        );
        // shown 随档变，total 是「查询命中后的会话总数」，与档无关。
        expectEqual(shownTotalOf(container), ["1", "3"], "切档后 当前显示 N / M");
        expectEqual(rowIdsOf(container), [seriesSessionId("aaaaaaaa", 2)], "九月档只包含九月的会话");
        expectEqual(
          query(container, '[data-segment="月份"] [data-segment-option="2026-09"]')
            ?.getAttribute("data-segment-selected"),
          "true",
          "被选中的月份档",
        );

        click(query(container, '[data-segment="月份"] [data-segment-option="all"]'), "「全部」月份档");
        await waitFor(() => shownTotalOf(container)[0] === "3", "切回全部月份");
        expectEqual(rowIdsOf(container), [0, 1, 2].map((index) => seriesSessionId("aaaaaaaa", index)), "切回后重新列出全部会话");
      });
    },
  },
  {
    name: "R15-14 目录内搜索只在本目录命中；无命中是第三种呈现，不占用 sessions=empty",
    async run() {
      const sessions = [
        ...sessionSeries({
          alias: "alpha",
          idPrefix: "aaaaaaaa",
          titles: ["部署检查", "回滚预案"],
          at: ["2026-10-05T09:00:00.000Z", "2026-10-04T09:00:00.000Z"],
          states: ["running", "idle"],
        }),
        ...sessionSeries({
          alias: "beta",
          idPrefix: "bbbbbbbb",
          titles: ["beta 专属会话"],
          at: ["2026-10-03T09:00:00.000Z"],
        }),
      ];
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryDetail(container);
        expectEqual(shownTotalOf(container), ["2", "2"], "进入详情页时呈现该目录的全部会话");

        // 命中：只按标题匹配，且 shown 与 total 一起跟着查询走。
        typeIntoInput(container, "[data-directory-detail-search]", "回滚", "详情页搜索框");
        await waitFor(() => shownTotalOf(container)[0] === "1", "按标题搜索后的 shown");
        expectEqual(shownTotalOf(container), ["1", "1"], "命中一条时 当前显示 N / M");
        expectEqual(rowIdsOf(container), [seriesSessionId("aaaaaaaa", 1)], "只剩标题命中的那一条");
        expectEqual(
          textOf(query(container, '[data-session-id] [data-session-title="true"]')),
          "回滚预案",
          "命中行的标题",
        );
        expectEqual(
          query(container, "[data-directory-detail-search]")
            ?.getAttribute("data-directory-detail-search-value"),
          "回滚",
          "受控搜索框回读到的查询词",
        );

        // 无命中：另一条会话属于别的目录，在本目录的搜索框里必须一条都不命中。
        typeIntoInput(container, "[data-directory-detail-search]", "beta 专属会话", "详情页搜索框");
        await waitFor(
          () => query(container, '[data-directory-detail-no-match="true"]') !== null,
          "目录内搜索无命中的呈现",
        );
        expectEqual(shownTotalOf(container), ["0", "0"], "无命中时的 当前显示 N / M");
        expectEqual(rowIdsOf(container).length, 0, "无命中时不得剩下任何会话行");
        expectThat(
          query(container, '[data-directory-sessions="empty"]') === null,
          "「没有符合条件的会话」不得被画成「这个目录里还没有会话」",
        );
        expectThat(
          textOf(query(container, '[data-directory-detail-no-match="true"]')).includes("没有符合条件的会话"),
          "无命中的文案必须说的是「没有符合条件的会话」",
        );
      });
    },
  },
  {
    name: "R15-15 两页的查询词彼此独立：目录页的查询词不进入详情页，详情页的查询词/月份也不改变目录页",
    async run() {
      // 本目录这两条会话的时间分属两个月，因此「月份档泄漏到目录页」也能被下面的内联行数抓住。
      const sessions = [
        ...sessionSeries({
          alias: "alpha",
          idPrefix: "aaaaaaaa",
          titles: ["部署检查", "九月归档"],
          at: ["2026-10-05T09:00:00.000Z", "2026-09-20T09:00:00.000Z"],
          states: ["running", "idle"],
        }),
        ...sessionSeries({
          alias: "beta",
          idPrefix: "bbbbbbbb",
          titles: ["文档整理"],
          at: ["2026-10-03T09:00:00.000Z"],
          states: ["waiting_input"],
        }),
      ];

      // (1) 目录页的查询词写的是 `directoryQuery`，详情页读的是 `directoryDetailQuery`：前者
      // 即使命中了这个目录里的会话，也不得改变详情页呈现的内容。
      // 这里直接写 store 而不是先在目录页打字再导航：要断的是**渲染读的是哪个字段**，
      // 直接写入才能把「同一个值」同时放进两个字段的对照里（跨路由的那条路径由 R15-21/R15-22 覆盖）。
      const detail = await mountScenario(baseScenario({ sessions }));
      try {
        await waitForDirectoryDetail(detail.container);
        // 「部署」只命中 alpha 的第一条会话：若目录页的字段泄漏进详情页，下面的 total 会从 2 变 1。
        detail.composition?.store.setDirectoryQuery("部署");
        // 切一次月份档：既是同步点（这次渲染一定发生在上面那次写入之后），
        // 也让详情页真的重绘了一遍。
        click(query(detail.container, '[data-segment="月份"] [data-segment-option="2026-09"]'), "九月档");
        await waitFor(
          () => shownTotalOf(detail.container)[0] === "1",
          "切到九月档",
        );
        expectEqual(
          query(detail.container, "[data-directory-detail-search]")
            ?.getAttribute("data-directory-detail-search-value"),
          "",
          "目录页的查询词不得进入详情页的搜索框",
        );
        expectEqual(shownTotalOf(detail.container), ["1", "2"], "详情页的 shown/total 只由它自己的字段决定");
        expectEqual(rowIdsOf(detail.container), [seriesSessionId("aaaaaaaa", 1)], "九月档只剩九月那一条");
      } finally {
        await detail.unmount();
      }

      // (2) 反向：详情页的查询词与月份档写的是另外两个字段，不得改变目录页的搜索框与内联会话行。
      const dirs = await mountScenario(baseScenario({ route: DIRS_ROUTE, sessions }));
      try {
        await waitForDirectoryList(dirs.container);
        dirs.composition?.store.setDirectoryDetailQuery("部署");
        dirs.composition?.store.setDirectoryDetailMonth("2026-10");
        // 用目录页自己的查询词做一次**可见**的改动，既是同步点也是对照：
        // 「归档」只命中 alpha 的九月会话，而详情页那两个值（查询「部署」、月份 2026-10）
        // 一旦泄漏就会被它挡掉。
        typeIntoInput(dirs.container, "[data-directory-search]", "归档", "目录页搜索框");
        await waitFor(
          () => query(dirs.container, '[data-directory-search-value="归档"]') !== null,
          "目录页自己的查询词生效",
        );
        expectEqual(
          queryAll(dirs.container, "[data-directory-alias]").map((node) =>
            node.getAttribute("data-directory-alias"),
          ),
          ["alpha"],
          "目录页的目录集合只由它自己的查询词决定",
        );
        expectEqual(
          queryAll(dirs.container, "[data-session-id]").map((node) =>
            node.getAttribute("data-session-id"),
          ),
          [0, 1].map((index) => seriesSessionId("aaaaaaaa", index)),
          "目录页的内联会话行不得被详情页的查询词或月份档挡住",
        );
      } finally {
        await dirs.unmount();
      }
    },
  },
  {
    name: "R15-16 窄视口：会话行、三项汇总与创建入口一个不少，data-* 集合与宽视口逐项相同",
    async run() {
      const sessions = sessionSeries({
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["部署检查", "回滚预案"],
        at: ["2026-10-05T09:00:00.000Z", "2026-10-04T09:00:00.000Z"],
        states: ["running", "waiting_input"],
      });
      const wide = await factsAtViewportWidth(WIDE_VIEWPORT_WIDTH, baseScenario({ sessions }));
      const narrow = await factsAtViewportWidth(NARROW_VIEWPORT_WIDTH, baseScenario({ sessions }));

      // 前置：两档真的走了不同的布局分支（`flex-direction` 由 row 变 column）。
      // 视口覆盖没生效时这里就会红，读作「这条用例没有在测窄屏」，而不是被测行为回退。
      expectEqual(wide.toolbarDirection, "row", "宽档必须走宽屏布局");
      expectEqual(narrow.toolbarDirection, "column", "窄档必须走单列布局");

      // 信息集合：两档的 `data-*` 锚点逐项相同，一个都不少、一个也不多。
      expectEqual(narrow.dataAttributes, wide.dataAttributes, "两档的 data-* 集合必须完全相同");
      expectEqual(
        narrow.rowIds,
        [0, 1].map((index) => seriesSessionId("aaaaaaaa", index)),
        "窄档仍然呈现目录里的会话行",
      );
      expectEqual(narrow.counts, wide.counts, "两档的三项汇总数字相同");
      expectEqual(narrow.counts, ["2", "1", "1"], "三项汇总（会话数/运行中/待处理）");
      expectThat(narrow.hasCreateEntry, "窄档仍然呈现创建会话的入口");
      expectThat(narrow.hasCreateSideEffect, "窄档仍然呈现创建会话的副作用说明");
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
          query(mounted.container, "[data-synced-at]") === null,
          "内容来自本次同步时不得标注「已同步 …」时刻",
        );

        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后详情页标注 last_sync",
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
          textOf(query(mounted.container, '[data-synced-at]')),
          `已同步 ${new Date(Number(epochMs)).toISOString().slice(0, 16).replace("T", " ")} UTC`,
          "同步时刻的展示文本（与目录页同一套文案）",
        );
      } finally {
        await mounted.unmount();
      }

      // 从未成功替换过快照：详情页停在等待同步，一个同步时刻都不得出现。
      await withScenario(baseScenario({ deliverSnapshot: false }), async (container) => {
        await waitFor(
          () => query(container, '[data-connection-state="replaying"]') !== null,
          "连接已认证并订阅（尚未追平）",
        );
        expectThat(
          query(container, '[data-directory-detail-state="loading"]') !== null,
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
    name: "R15-18 已选月份档被查询挤掉时，生效月份回落到「全部」而不是「无选中 + 空列表」",
    async run() {
      const sessions = sessionSeries({
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["十月甲", "十月乙", "九月丙"],
        at: [
          "2026-10-05T09:00:00.000Z",
          "2026-10-04T09:00:00.000Z",
          "2026-09-20T09:00:00.000Z",
        ],
        states: ["running", "idle", "waiting_input"],
      });
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryDetail(container);
        click(query(container, '[data-segment="月份"] [data-segment-option="2026-09"]'), "「2026-09」月份档");
        await waitFor(() => shownTotalOf(container)[0] === "1", "切到九月档");
        expectEqual(shownTotalOf(container), ["1", "3"], "切档后的 当前显示 N / M");

        // 「十月」把九月从月份档里挤掉（`months` 由查询命中的行算出），而已选的月份还是
        // 「2026-09」。生效的月份必须回落到「全部」：否则月份组会没有任何一项被选中，
        // 而列表又是空的——用户既看不懂也退不出来。
        typeIntoInput(container, "[data-directory-detail-search]", "十月", "详情页搜索框");
        await waitFor(() => shownTotalOf(container)[0] === "2", "查询后呈现十月命中的两条");
        expectEqual(shownTotalOf(container), ["2", "2"], "回落之后呈现的就是查询命中的两条");
        expectEqual(
          rowIdsOf(container),
          [0, 1].map((index) => seriesSessionId("aaaaaaaa", index)),
          "呈现的是十月那两条",
        );
        expectEqual(
          queryAll(container, '[data-segment="月份"] [data-segment-option="2026-09"]').length,
          0,
          "九月档已不在月份档列表里",
        );
        expectEqual(
          query(container, '[data-segment="月份"] [data-segment-option="all"]')
            ?.getAttribute("data-segment-selected"),
          "true",
          "月份档消失后，「全部」必须是生效且被选中的那一项",
        );
        expectThat(
          query(container, "[data-directory-detail-no-match]") === null,
          "回落之后不是「没有符合条件的会话」",
        );
      });
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
            query(container, '[data-directory-alias="alpha"] [data-action="open-directory"]'),
            "alpha 的卡片标题入口",
          );
          await waitFor(
            () => query(container, '[data-route="dir-detail"][data-directory-alias="alpha"]') !== null,
            "标题入口进入 alpha 的详情页",
          );
          // 进入的是**那一个**目录：别名与它名下的会话都要对得上（走错目录/没带参数都会红）。
          expectEqual(
            textOf(query(container, "[data-directory-alias-label]")),
            "alpha",
            "详情页的别名",
          );
          expectEqual(rowIdsOf(container), [SESSION_ALPHA], "详情页列出的是 alpha 名下的会话");

          // 返回目录页，再从另一个入口进另一个目录：两条入口不能串到同一目录上。
          click(query(container, '[data-action="back-to-dirs"]'), "返回目录");
          await waitForDirectoryList(container);
          click(
            query(container, '[data-directory-alias="beta"] [data-action="open-directory-all"]'),
            "beta 的「查看全部」入口",
          );
          await waitFor(
            () => query(container, '[data-route="dir-detail"][data-directory-alias="beta"]') !== null,
            "「查看全部」入口进入 beta 的详情页",
          );
          expectEqual(rowIdsOf(container), [SESSION_BETA], "详情页列出的是 beta 名下的会话");
        },
      );
    },
  },
  {
    name: "R15-22 跨路由：返回目录页后目录页的查询词仍在，命中集合也仍是那次搜索的结果",
    async run() {
      // 「部署」只出现在 alpha 的会话标题里（两条目录的展示名都是 api、别名也不含这两个字）。
      const sessions = [
        ...sessionSeries({
          alias: "alpha",
          idPrefix: "aaaaaaaa",
          titles: ["部署检查"],
          at: ["2026-10-05T09:00:00.000Z"],
          states: ["running"],
        }),
        ...sessionSeries({
          alias: "beta",
          idPrefix: "bbbbbbbb",
          titles: ["文档整理"],
          at: ["2026-10-03T09:00:00.000Z"],
          states: ["waiting_input"],
        }),
      ];
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions }),
        async (container) => {
          await waitForDirectoryList(container);
          expectEqual(queryAll(container, "[data-directory-alias]").length, 2, "初始两条目录项");

          typeIntoInput(container, "[data-directory-search]", "部署", "目录页搜索框");
          await waitFor(
            () => queryAll(container, "[data-directory-alias]").length === 1,
            "按会话标题搜索后只剩 alpha",
          );
          expectEqual(
            query(container, "[data-directory-alias]")?.getAttribute("data-directory-alias") ?? null,
            "alpha",
            "命中的是 alpha",
          );

          click(
            query(container, '[data-directory-alias="alpha"] [data-action="open-directory"]'),
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
            queryAll(container, "[data-directory-alias]").map((node) =>
              node.getAttribute("data-directory-alias"),
            ),
            ["alpha"],
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
          expectEqual(queryAll(container, "[data-directory-alias]").length, 2, "两条目录项都要在");
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
            query(container, '[data-directory-alias="alpha"] [data-action="open-directory"]'),
            "alpha 的卡片标题入口",
          );
          await waitFor(
            () => query(container, '[data-route="dir-detail"][data-directory-alias="alpha"]') !== null,
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
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["部署检查"],
        at: ["2026-10-05T09:00:00.000Z"],
        states: ["running"],
      });
      await withScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES, sessions }),
        async (container) => {
          await waitForDirectoryList(container);
          const beta = query(container, '[data-directory-alias="beta"]');
          expectThat(beta !== null, "0 会话的目录也要作为目录项呈现");
          expectEqual(
            ["total", "active", "pending"].map((kind) =>
              textOf(query(beta, `[data-directory-count="${kind}"]`)),
            ),
            ["0", "0", "0"],
            "beta 的三项汇总为零",
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
            () => query(container, '[data-route="dir-detail"][data-directory-alias="beta"]') !== null,
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
];

/** 一次挂载里读到的、与宽度无关的全部事实。 */
interface DetailFacts {
  /** 渲染结果里每条 `data-*` 的 `名字=值`（同步时刻只记名字：它是挂载那一刻的 epoch）。 */
  readonly dataAttributes: readonly string[];
  /** 工具条的 `flex-direction`：`row` = 宽屏分支，`column` = 窄屏分支。 */
  readonly toolbarDirection: string;
  readonly rowIds: readonly string[];
  readonly counts: readonly string[];
  readonly hasCreateEntry: boolean;
  readonly hasCreateSideEffect: boolean;
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
  const toolbar = query(container, "[data-directory-detail-search]")?.parentElement ?? null;
  return {
    dataAttributes: entries.sort(),
    toolbarDirection: toolbar === null ? "" : getComputedStyle(toolbar).flexDirection,
    rowIds: rowIdsOf(container),
    counts: ["total", "active", "pending"].map((kind) =>
      textOf(query(container, `[data-directory-count="${kind}"]`)),
    ),
    hasCreateEntry: query(container, '[data-action="open-create-sheet"]') !== null,
    hasCreateSideEffect: query(container, "[data-create-side-effect]") !== null,
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
function overrideViewportWidth(width: number): () => void {
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
