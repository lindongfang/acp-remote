/**
 * R15「目录页以目录为主角且可离线渲染」的浏览器级验证（目录页那一半）。
 *
 * 每条用例都跑在**真实页面**上：`app/index.tsx` / `app/dir/[alias].tsx` → 展示组件 →
 * `src/features/` 的模型，数据来自**真实快照管线**（经真实 SHA-256 两级摘要校验过的
 * `sync.snapshot_*` 帧，由真实 `SyncClient` 校验后交给 `ClientStore`）。
 *
 * 目录详情页的用例在 `r15-directory-detail.ts`（同一批选择器与夹具，见那里的说明）。
 *
 * ## 判别力
 *
 * - 「不泄露本机路径」用**负向断言**：快照里真的塞进了协议中不存在的 `path` / `root`，
 *   断言渲染后的 DOM 里连片段都没有。只断言「模型对象没有 path 键」证明不了这一点。
 * - 「断连仍可呈现」先断言在线时来源是 `live`，再断开连接断言变 `last_sync` 且列表仍在——
 *   单看 `last_sync` 那一帧区分不出「断连后仍渲染」与「从来没有过内容」。
 * - 「两种空态可区分」分别挂两个场景，比对 `data-empty-state` 与标题、指引文案。
 * - 「搜索/筛选」的查询词**故意不命中任何展示名与别名**（两条目录都叫 `api`），只能由
 *   「该目录下会话的标题」命中：把匹配退化成只查目录名时该用例必红。
 * - 「无命中」断言的是**第三种呈现**（`data-directory-no-match`）且两种空态**都不出现**——
 *   把无命中画成「本机没有目录」在这里必红。
 * - 选择器一律**属性型**（`[data-directory-alias]` 等 `data-*` 锚点），不依赖 RNW 把 `role`
 *   映射成哪个标签：`role="listitem"` 目前渲染成 `li`，但那是渲染器的实现细节，不是契约。
 */

import {
  AGENTS,
  DIRS_ROUTE,
  SECRET_FRAGMENTS,
  WORKSPACES_WITH_PATHS,
  seriesSessionId,
  sessionSeries,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  click,
  domHtml,
  domText,
  expectAbsent,
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

/** 两条同名目录 + 各一条会话的默认场景。 */
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
    route: DIRS_ROUTE,
    ...overrides,
  };
}

/**
 * 等到目录列表出现（快照已验证并替换了仓库）。
 *
 * 与 `typeIntoInput` / `withScenario` 一样供 `r15-directory-detail.ts` 复用（那条用例要在
 * 目录页上验「详情页的查询词/月份档不影响目录页」）：目录页的等待条件只有一处，
 * 两个文件才不会各写一份、各自走样。
 */
export async function waitForDirectoryList(container: HTMLElement): Promise<void> {
  await waitFor(
    () => query(container, '[data-directory-list="directories"]') !== null,
    "目录列表渲染",
  );
}

/** 一个目录行：只有展示名与别名两项文字。 */
function assertRowShowsNameAndAlias(row: Element | null, alias: string): void {
  expectEqual(row?.getAttribute("data-directory-alias") ?? null, alias, `目录行 ${alias} 的 data-directory-alias`);
  expectEqual(
    textOf(row?.querySelector("[data-directory-display-name]")),
    "api",
    `目录行 ${alias} 的展示名`,
  );
  expectEqual(textOf(row?.querySelector("[data-directory-alias-label]")), alias, `目录行 ${alias} 的别名标签`);
}

/**
 * 往受控输入框里「打字」。
 *
 * 必须走**原型上的原生 setter**：React 会在输入节点上装自己的 `value` 存取器（记着上一次的
 * 值），直接 `node.value = next` 会让 React 认为「值没变」而**不**调用 `onChangeText`，
 * 用例就会在「输入没有效果」上假红。事件路径与用户敲键一致（真实的 `input` 事件 + 冒泡），
 * 输入框由 `TextInput`（RNW）渲染成真实的 `<input>`。
 *
 * 同时供 `r15-directory-detail.ts` 用：两页各有一个受控搜索框，打字的机关只有一份。
 */
export function typeIntoInput(container: HTMLElement, selector: string, value: string, detail: string): void {
  const node = query(container, selector) as HTMLInputElement | null;
  if (node === null) throw new CheckFailure(`${detail}：找不到输入框 ${selector}`);
  const own = Object.getOwnPropertyDescriptor(node, "value");
  const native = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
  if (native?.set !== undefined && native.set !== own?.set) native.set.call(node, value);
  else if (own?.set !== undefined) own.set.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}

/** 目录项的别名集合：按**集合**比较，不依赖 DOM 顺序。 */
function aliasesOf(container: HTMLElement): readonly string[] {
  return queryAll(container, "[data-directory-alias]")
    .map((node) => node.getAttribute("data-directory-alias") ?? "")
    .sort();
}

/** 一个目录项上的三项汇总（数字以文本呈现）。 */
function countsOf(row: Element | null): readonly string[] {
  return ["total", "active", "pending"].map((kind) =>
    textOf(row?.querySelector(`[data-directory-count="${kind}"]`)),
  );
}

/**
 * 搜索/筛选用例的会话：两条目录都叫 `api`，别名是 `alpha`/`beta`，
 * 而查询词只出现在 alpha 的会话标题里——命中因此**只能**来自标题这条匹配路径。
 * alpha：2 条（1 运行中）；beta：1 条（待处理）。
 */
function searchableSessions() {
  return [
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
      titles: ["文档整理"],
      at: ["2026-10-03T09:00:00.000Z"],
      states: ["waiting_input"],
    }),
  ];
}

/** 一条场景跑完就卸载：`dispose()` 之后的组合根不可复活，留着会污染下一条用例。 */
export async function withScenario<T>(
  options: ScenarioOptions,
  body: (container: HTMLElement) => Promise<T>,
): Promise<T> {
  const mounted = await mountScenario(options);
  try {
    return await body(mounted.container);
  } finally {
    await mounted.unmount();
  }
}

const checks: readonly Check[] = [
  {
    name: "R15-1 目录页只显示展示名与别名：快照里的 path/root 一个片段都不许出现在 DOM",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);
        const html = domHtml(container);

        const rows = queryAll(container, "[data-directory-alias]");
        expectEqual(rows.length, 2, "目录行数");
        assertRowShowsNameAndAlias(rows[0] ?? null, "alpha");
        assertRowShowsNameAndAlias(rows[1] ?? null, "beta");

        // MUST NOT：渲染结果里不得出现本机规范化路径的任何片段。
        expectNone(html, SECRET_FRAGMENTS, "目录页渲染结果泄露了本机路径");
        expectAbsent(html, "path=", "目录行把 path 当成了 DOM 属性");
      });
    },
  },
  {
    name: "R15-2 同名目录以别名区分：两条同名目录各自渲染出自己的别名，且没有第二种消歧依据",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);
        const names = queryAll(container, "[data-directory-display-name]").map((node) => node.textContent);
        const aliases = queryAll(container, "[data-directory-alias-label]").map((node) => node.textContent);

        expectEqual(names, ["api", "api"], "两条目录的展示名必须相同，否则这条用例没在测同名");
        expectEqual(aliases, ["alpha", "beta"], "两条目录的别名标签");
        // 区分同名目录的**唯一**依据是别名；渲染结果里不得再有路径之类的第二种依据。
        expectNone(domHtml(container), SECRET_FRAGMENTS, "同名消歧用了路径片段");
      });
    },
  },
  {
    name: "R15-3 会话数/运行中/待处理由会话摘要得出，且断开连接后目录页仍列出内容",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForDirectoryList(mounted.container);
        // 在线时来源标注必须是「本次同步」，否则下面的 last_sync 断言没有对照价值。
        await waitFor(
          () => query(mounted.container, '[data-content-origin="live"]') !== null,
          "连接在线后目录页标注 live",
        );

        const alphaRow = query(mounted.container, '[data-directory-alias="alpha"]');
        const betaRow = query(mounted.container, '[data-directory-alias="beta"]');
        expectEqual(countsOf(alphaRow), ["1", "1", "0"], "alpha 的会话数/运行中/待处理");
        expectEqual(countsOf(betaRow), ["1", "0", "1"], "beta 的会话数/运行中/待处理");

        // 断开连接：内容必须仍在，且来源标注切换成「上次同步」。
        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后目录页标注 last_sync",
        );
        expectEqual(queryAll(mounted.container, "[data-directory-alias]").length, 2, "断连后的目录行数");
        const text = domText(mounted.container);
        expectThat(text.includes("alpha") && text.includes("beta"), "断连后目录项文本应仍在");
        expectNone(domHtml(mounted.container), SECRET_FRAGMENTS, "断连后的渲染结果泄露了本机路径");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R15-4 「一个目录都没有」与「目录存在但无会话」是两种可区分的呈现",
    async run() {
      const read = async (options: ScenarioOptions): Promise<[string, string, string]> => {
        const mounted = await mountScenario(options);
        try {
          await waitFor(
            () => query(mounted.container, "[data-empty-state]") !== null,
            "空态渲染",
          );
          const node = query(mounted.container, "[data-empty-state]");
          return [
            node?.getAttribute("data-empty-state") ?? "",
            textOf(node?.querySelector("h2")),
            textOf(node?.querySelector("p")),
          ];
        } finally {
          await mounted.unmount();
        }
      };

      const [noDirectoryState, noDirectoryTitle, noDirectoryGuidance] = await read(
        baseScenario({ workspaces: [], sessions: [] }),
      );
      const [emptyState, emptyTitle, emptyGuidance] = await read(
        baseScenario({ workspaces: WORKSPACES_WITH_PATHS, sessions: [] }),
      );

      expectEqual(noDirectoryState, "no_directories", "无目录时的空态标识");
      expectEqual(emptyState, "directories_without_sessions", "有目录无会话时的空态标识");
      // 两种空态合并成一句话即违反 R15：标题与指引都必须不同。
      expectThat(noDirectoryTitle !== emptyTitle, `两种空态的标题必须不同（都是 ${noDirectoryTitle}）`);
      expectThat(
        noDirectoryGuidance !== emptyGuidance,
        `两种空态的操作指引必须不同（都是 ${noDirectoryGuidance}）`,
      );
    },
  },
  {
    name: "R15-8 目录页搜索：命中集合随查询词变化而汇总不变，无命中是第三种呈现",
    async run() {
      await withScenario(baseScenario({ sessions: searchableSessions() }), async (container) => {
        await waitForDirectoryList(container);
        expectEqual(aliasesOf(container), ["alpha", "beta"], "初始的两个目录项");

        // 「部署检查」只出现在 alpha 的会话标题里（展示名都是 api、别名也不含这三个字）。
        typeIntoInput(container, "[data-directory-search]", "部署", "目录页搜索框");
        await waitFor(() => aliasesOf(container).join(",") === "alpha", "按会话标题搜索后只剩 alpha");
        expectEqual(
          query(container, "[data-directory-search]")?.getAttribute("data-directory-search-value"),
          "部署",
          "受控搜索框回读到的查询词",
        );
        // 命中集合变了，但存活目录的三项汇总仍是它自己的全量数字，不是「命中的会话数」。
        expectEqual(countsOf(query(container, '[data-directory-alias="alpha"]')), ["2", "1", "0"], "alpha 的三项汇总");
        // 未命中的目录连同它的会话行一起不呈现。
        expectEqual(
          queryAll(container, "[data-session-id]")
            .map((node) => node.getAttribute("data-session-id") ?? "")
            .sort(),
          [seriesSessionId("aaaaaaaa", 0), seriesSessionId("aaaaaaaa", 1)],
          "内联会话行只应剩 alpha 的两条",
        );

        // 无命中：与两种 `data-empty-state` 互不冒充的第三种呈现。
        typeIntoInput(container, "[data-directory-search]", "没有这个目录", "目录页搜索框");
        await waitFor(
          () => query(container, '[data-directory-no-match="true"]') !== null,
          "搜索无结果的呈现",
        );
        expectEqual(queryAll(container, "[data-directory-alias]").length, 0, "无命中时不得剩下目录项");
        expectThat(
          query(container, "[data-empty-state]") === null,
          "无命中不是「本机没有目录」也不是「目录存在但无会话」",
        );
        expectThat(
          query(container, "[data-directory-list]") === null,
          "无命中时不得继续呈现目录列表",
        );
        expectThat(
          textOf(query(container, '[data-directory-no-match="true"]')).includes("没有匹配的结果"),
          `无命中的文案必须说明是「没有匹配的结果」，实际 ${JSON.stringify(
            textOf(query(container, '[data-directory-no-match="true"]')),
          )}`,
        );
      });
    },
  },
  {
    name: "R15-9 目录页状态筛选：只留下有相应状态的目录，汇总不因筛选改变，且可与搜索叠加",
    async run() {
      await withScenario(baseScenario({ sessions: searchableSessions() }), async (container) => {
        await waitForDirectoryList(container);

        click(query(container, '[data-segment="筛选"] [data-segment-option="running"]'), "「运行中」筛选");
        await waitFor(() => aliasesOf(container).join(",") === "alpha", "筛选「运行中」后只剩 alpha");
        expectEqual(
          query(container, '[data-segment="筛选"] [data-segment-option="running"]')
            ?.getAttribute("data-segment-selected"),
          "true",
          "「运行中」应处于选中态",
        );
        expectEqual(
          query(container, '[data-segment="筛选"] [data-segment-option="all"]')
            ?.getAttribute("data-segment-selected"),
          "false",
          "「全部」应不再是选中态",
        );
        expectEqual(countsOf(query(container, '[data-directory-alias="alpha"]')), ["2", "1", "0"], "筛选不改 alpha 的三项汇总");

        click(query(container, '[data-segment="筛选"] [data-segment-option="pending"]'), "「待处理」筛选");
        await waitFor(() => aliasesOf(container).join(",") === "beta", "筛选「待处理」后只剩 beta");
        expectEqual(countsOf(query(container, '[data-directory-alias="beta"]')), ["1", "0", "1"], "筛选不改 beta 的三项汇总");

        // 叠加：查询词「api」对两条目录都命中，决定性的因此仍然是「待处理」这一筛选。
        typeIntoInput(container, "[data-directory-search]", "api", "目录页搜索框");
        await waitFor(
          () => query(container, '[data-directory-search-value="api"]') !== null,
          "搜索词进入受控输入框",
        );
        expectEqual(aliasesOf(container), ["beta"], "搜索与筛选叠加后仍只剩 beta");
      });
    },
  },
  {
    name: "R15-10 超过内联上限的目录：只列 5 条最近的会话，并给出「查看全部 N 个」入口",
    async run() {
      const sessions = sessionSeries({
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["会话 一", "会话 二", "会话 三", "会话 四", "会话 五", "会话 六"],
        at: [
          "2026-10-06T09:00:00.000Z",
          "2026-10-05T09:00:00.000Z",
          "2026-10-04T09:00:00.000Z",
          "2026-10-03T09:00:00.000Z",
          "2026-10-02T09:00:00.000Z",
          "2026-10-01T09:00:00.000Z",
        ],
      });
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryList(container);
        const card = query(container, '[data-directory-alias="alpha"]');
        expectThat(card !== null, "应有 alpha 的目录项");
        // 内联行只列最近的 5 条，且按会话自身的时间倒序；第 6 条不出现在目录页上。
        expectEqual(
          queryAll(card, "[data-session-id]").map((node) => node.getAttribute("data-session-id") ?? ""),
          [0, 1, 2, 3, 4].map((index) => seriesSessionId("aaaaaaaa", index)),
          "内联会话行必须是最近的 5 条、按时间倒序",
        );
        expectEqual(countsOf(card), ["6", "0", "0"], "目录项的三项汇总按全量口径");

        // 入口文案里的 N 是目录的**会话总数**（不是内联行数）：写成 5 或写成内联行数都会在这里红。
        const openAll = query(card, '[data-action="open-directory"]');
        expectThat(openAll !== null, "会话数超过内联上限时必须给出进入详情页的入口");
        expectThat(
          textOf(openAll).includes("查看全部 6 个"),
          `入口文案里的 N 必须是目录的会话总数，实际 ${JSON.stringify(textOf(openAll))}`,
        );
      });
    },
  },
  {
    name: "R15-11 没有会话的目录项不呈现「查看全部」入口（对照：有会话的目录项必须有）",
    async run() {
      const onlyAlpha = sessionSeries({
        alias: "alpha",
        idPrefix: "aaaaaaaa",
        titles: ["部署检查"],
        at: ["2026-10-05T09:00:00.000Z"],
        states: ["running"],
      });
      await withScenario(baseScenario({ sessions: onlyAlpha }), async (container) => {
        await waitForDirectoryList(container);
        expectEqual(aliasesOf(container), ["alpha", "beta"], "两条目录都要呈现（一条没有会话）");
        const beta = query(container, '[data-directory-alias="beta"]');
        expectEqual(countsOf(beta), ["0", "0", "0"], "beta 的三项汇总为零");
        expectThat(
          query(beta, '[data-action="open-directory"]') === null,
          "0 条会话的目录项不得给出「查看全部 N 个」入口（那里也没有会话可看）",
        );
        expectThat(
          query(container, '[data-directory-alias="alpha"] [data-action="open-directory"]') !== null,
          "对照：有会话的目录项必须给出入口",
        );
      });
    },
  },
  {
    name: "R15-12 断连时标注上次同步时刻（且不随重新渲染改变）；从未成功同步时不呈现任何时刻",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForDirectoryList(mounted.container);
        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后目录页标注 last_sync",
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
        // 展示文本是 UTC 固定切片（`YYYY-MM-DD HH:MM UTC`），因此可以逐字断言：走本地时区
        // 或改成「N 分钟前」都会在这里红。
        expectEqual(
          textOf(query(mounted.container, '[data-synced-at]')),
          `已同步 ${new Date(Number(epochMs)).toISOString().slice(0, 16).replace("T", " ")} UTC`,
          "同步时刻的展示文本（UTC 固定切片）",
        );
        // 重新渲染（敲一次查询）后时刻不得变：它不是「渲染时的当前时间」。
        typeIntoInput(mounted.container, "[data-directory-search]", "alpha", "目录页搜索框");
        await waitFor(
          () => query(mounted.container, '[data-directory-search-value="alpha"]') !== null,
          "查询词进入受控输入框",
        );
        expectEqual(
          query(mounted.container, "[data-synced-at]")?.getAttribute("data-synced-at") ?? null,
          epochMs,
          "重新渲染后标注的仍是那次快照的时刻",
        );
      } finally {
        await mounted.unmount();
      }

      // 从未成功替换过快照：连接已经建起来（认证通过、已订阅、停在追平），但仓库里没有快照。
      await withScenario(baseScenario({ deliverSnapshot: false }), async (container) => {
        await waitFor(
          () => query(container, '[data-connection-state="replaying"]') !== null,
          "连接已认证并订阅（尚未追平）",
        );
        expectThat(
          query(container, '[data-directory-state="loading"]') !== null,
          "从未同步过时必须呈现等待同步的状态",
        );
        expectThat(
          query(container, "[data-synced-at]") === null,
          "从未成功替换过快照时不得呈现任何同步时刻",
        );
        expectAbsent(domText(container), "已同步", "从未同步过的页面不该出现「已同步」文案");
      });
    },
  },
];

export const R15_CHECKS = checks;
