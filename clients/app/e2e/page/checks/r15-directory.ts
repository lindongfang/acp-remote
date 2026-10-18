/**
 * R15「目录页」的浏览器级验证。
 *
 * 每条用例都跑在**真实页面**上：`app/index.tsx` → 展示组件 → `src/features/` 的模型，数据来自
 * **真实快照管线**（经真实 SHA-256 两级摘要校验过的 `sync.snapshot_*` 帧，由真实 `SyncClient`
 * 校验后交给 `ClientStore`）。
 *
 * 目录详情页的用例在 `r15-directory-detail.ts`（同一批选择器与夹具，见那里的说明）。
 *
 * ## 判别力
 *
 * - 「只呈现服务端下发的路径」用**负向断言**：快照里的路径是完整本机路径，断言渲染后的 DOM 里
 *   没有整条路径、没有它的父段、也没有任何分隔符——只有同名消歧段（逐字搬运的那一段）允许出现。
 *   只断言「模型对象没有某个键」证明不了这一点。
 * - 「同名目录以路径尾段区分」正面断言那两段尾段逐字等于快照 `path` 的末两段，并断言**目录标识
 *   不是**界面文字（拿标识承担消歧，这条会与可见文本断言一起红）。
 * - 「展示名唯一时不呈现路径」换一份展示名唯一的目录，断言连尾段锚点都不出现。
 * - 「断连仍可呈现」先断言在线时来源是 `live`，再断开连接断言变 `last_sync` 且列表仍在——
 *   单看 `last_sync` 那一帧区分不出「断连后仍渲染」与「从来没有过内容」。
 * - 「两种空态可区分」分别挂两个场景，比对 `data-empty-state` 与标题、指引文案。
 * - 「搜索/筛选」的查询词**故意不命中任何展示名与目录标识**（两条目录都叫 `api`），只能由
 *   「该目录下会话的标题」命中：把匹配退化成只查目录名时该用例必红；另有一条只用目录标识
 *   前缀搜索的负向用例，命中即红。
 * - 「无命中」断言的是**第三种呈现**（`data-directory-no-match`）且两种空态**都不出现**——
 *   把无命中画成「本机没有目录」在这里必红。
 * - 选择器一律**属性型**（`[data-directory-id]` 等 `data-*` 锚点），不依赖 RNW 把 `role`
 *   映射成哪个标签：`role="listitem"` 目前渲染成 `li`，但那是渲染器的实现细节，不是契约。
 * - 「创建入口不再有权限维度」既查**呈现**也查**痕迹**：每个目录项恒有一个入口且都不是禁用态，
 *   同时目录页全文不得出现权限提示条/不可用说明的标记或「电脑端授予」类说法。
 * - 「连接状态只有一个表达」量的是**路由自己那一层**（`data-route="dirs"` 的父节点）：自建导航
 *   (`data-action="open-pairing"`) 与第二个状态徽标 (`data-action="open-hosts"`，`ConnectionBadge`
 *   的指纹) 在这一层里一个都不许再出现；这一层里表达连接状态的只剩主机面板。共享外壳的顶栏
 *   （含设备胶囊）在**外面**一层，因此这条断言不受它影响。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  DIRECTORY_ID_BETA,
  DIRECTORY_PATH_ALPHA,
  DIRECTORY_PATH_BETA,
  DIRS_ROUTE,
  WORKSPACES_WITH_PATHS,
  seriesSessionId,
  sessionSeries,
  sessionsForBothDirectories,
} from "../fixtures";
// 深度与同目录其它 check 一致（`checks/` → `e2e/page` → `e2e` → `src`）：`../../src/*`
// 会落到 `e2e/src/*` 这个不存在的目录，只有 `../../../src/*` 才指向 `src/`。
import type { WorkspaceCatalogEntry } from "../../../src/protocol";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  PERMISSION_GATE_VOCABULARY,
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

/**
 * 目录页允许呈现的**唯一**路径片段是同名消歧段（逐字取自服务端下发路径的父段）。整条路径、
 * 它的父段与任何分隔符都 MUST NOT 出现。
 *
 * 这里不用 `SECRET_FRAGMENTS` 整表：那张表里的 `secret` / `vault` 恰好是本用例快照
 * （`D:\secret\vault\alpha\api`）的前缀，而消歧段 `alpha` / `beta` 是本轮规格明确要求呈现的。
 */
const PATH_PREFIX_FRAGMENTS = ["D:\\", "secret", "vault"] as const;

/** 两条同名目录 + 各一条会话的默认场景。 */
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

/** 一个目录行：呈现展示名与同名消歧段，**不**呈现目录标识或整条路径。 */
function assertRowShowsName(row: Element | null, directoryId: string, pathTail: string): void {
  expectEqual(
    row?.getAttribute("data-directory-id") ?? null,
    directoryId,
    `目录行 ${directoryId} 的 data-directory-id`,
  );
  expectEqual(
    textOf(row?.querySelector("[data-directory-display-name]")),
    "api",
    `目录行 ${directoryId} 的展示名`,
  );
  // 同名消歧只呈现路径里的更多段（逐字）；目录标识既不是标签、也不是可见文本。
  expectEqual(
    textOf(row?.querySelector("[data-directory-path-tail]")),
    pathTail,
    `目录行 ${directoryId} 的消歧段`,
  );
  expectAbsent(textOf(row), directoryId, `目录行 ${directoryId} 的可见文本里出现了目录标识`);
  // 整条路径只允许以消歧段的形式出现：目录页没有逐字路径锚点。
  expectThat(
    row?.querySelector("[data-directory-path]") === null,
    `目录行 ${directoryId} 不得呈现整条路径`,
  );
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

/** 目录项的标识集合：按**集合**比较，不依赖 DOM 顺序。 */
function directoryIdsOf(container: HTMLElement): readonly string[] {
  return queryAll(container, "[data-directory-id]")
    .map((node) => node.getAttribute("data-directory-id") ?? "")
    .sort();
}

/**
 * 一个目录项上的三项汇总（数字以文本呈现）。
 *
 * 会话总数胶囊恒在；运行中/待处理徽标只在**非零**时追加（原型同此），缺席即空串。
 */
function countsOf(row: Element | null): readonly string[] {
  return ["total", "active", "pending"].map((kind) =>
    textOf(query(row, `[data-directory-count="${kind}"]`)),
  );
}

/** 会话的时间文案允许的三种形态（相对口径）：刚刚 / N 分钟前 / 年月日。 */
const RELATIVE_TIME_PATTERN = /^(刚刚|\d+ 分钟前|\d{4}-\d{2}-\d{2})$/;

/**
 * 搜索/筛选用例的会话：两条目录的展示名都叫 `api`（标识是 32 位十六进制），
 * 而查询词只出现在 alpha 的会话标题里——命中因此**只能**来自标题这条匹配路径。
 * alpha：2 条（1 运行中）；beta：1 条（待处理）。
 */
function searchableSessions() {
  return [
    ...sessionSeries({
      directoryId: DIRECTORY_ID_ALPHA,
      idPrefix: "aaaaaaaa",
      titles: ["部署检查", "回滚预案"],
      at: ["2026-10-05T09:00:00.000Z", "2026-10-04T09:00:00.000Z"],
      states: ["running", "idle"],
    }),
    ...sessionSeries({
      directoryId: DIRECTORY_ID_BETA,
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
    name: "R15-1 目录页只呈现展示名与同名消歧段：快照里的整条路径、父段与分隔符都不许出现在 DOM，目录标识也不外显",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);
        const html = domHtml(container);

        const rows = queryAll(container, "[data-directory-id]");
        expectEqual(rows.length, 2, "目录行数");
        assertRowShowsName(rows[0] ?? null, DIRECTORY_ID_ALPHA, "alpha");
        assertRowShowsName(rows[1] ?? null, DIRECTORY_ID_BETA, "beta");

        // MUST NOT：渲染结果里不得出现整条本机路径、它的父段或盘符。
        expectNone(html, PATH_PREFIX_FRAGMENTS, "目录页渲染结果泄露了本机路径");
        // 分隔符一个都不许出现：客户端**没有**拼接任何路径（消歧段是不带分隔符的一段）。
        expectAbsent(html, "\\", "目录页出现了路径分隔符（说明有一段路径是本地拼出来的）");
        expectAbsent(html, "path=", "目录行把 path 当成了 DOM 属性");
        // 目录标识只作机器锚点：可见文本里一个都不许出现。
        expectNone(
          domText(container),
          [DIRECTORY_ID_ALPHA, DIRECTORY_ID_BETA],
          "目录页可见文本里出现了目录标识",
        );
      });
    },
  },
  {
    name: "R15-2 同名目录以更多路径段区分：消歧段逐字取自服务端下发的路径，目录标识不承担消歧",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);
        const names = queryAll(container, "[data-directory-display-name]").map((node) => node.textContent);
        const tails = queryAll(container, "[data-directory-path-tail]");

        expectEqual(names, ["api", "api"], "两条目录的展示名必须相同，否则这条用例没在测同名");
        // 快照里是 `D:\secret\vault\alpha\api` / `D:\secret\vault\beta\api`：
        // 展示名都是 `api`，消歧段因此是各自路径里的父段 `alpha` / `beta`（逐字搬运）。
        expectEqual(
          tails.map((node) => node.getAttribute("data-directory-path-tail")),
          ["alpha", "beta"],
          "两条同名目录的消歧段",
        );
        expectEqual(tails.map((node) => node.textContent), ["alpha", "beta"], "消歧段的可见文本");
        // 消歧段确实是路径里的一段：alpha 的完整路径以 `\alpha\api` 结尾。
        expectThat(
          DIRECTORY_PATH_ALPHA.endsWith("\\alpha\\api") && DIRECTORY_PATH_BETA.endsWith("\\beta\\api"),
          "前置：夹具路径的末两段必须与消歧段一致",
        );
        // 反例：改用目录标识承担消歧（把标识画出来）会在标识的可见文本断言上红。
        expectNone(
          domText(container),
          [DIRECTORY_ID_ALPHA, DIRECTORY_ID_BETA],
          "同名消歧不得改用目录标识",
        );
        // 整条路径与父段仍然不许出现（消歧段是允许的那一段，前缀不是）。
        expectNone(domHtml(container), ["D:\\", "secret", "vault"], "同名消歧用了整条路径");
      });
    },
  },
  {
    name: "R15-2b 展示名唯一时不呈现任何路径片段",
    async run() {
      // 展示名唯一的目录：feature 层据「同名才给尾段」把 `pathTail` 判成 `null`。
      const soloId = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";
      const solo = [
        { directoryId: soloId, path: "D:\\secret\\vault\\solo" },
      ] as unknown as readonly WorkspaceCatalogEntry[];
      const soloIds = sessionSeries({
        directoryId: soloId,
        idPrefix: "aaaaaaaa",
        titles: ["独自一条"],
        at: ["2026-10-05T09:00:00.000Z"],
        states: ["running"],
      });

      await withScenario(baseScenario({ workspaces: solo, sessions: soloIds }), async (container) => {
        await waitForDirectoryList(container);
        expectEqual(queryAll(container, "[data-directory-path-tail]").length, 0, "唯一展示名不得呈现路径");
        expectNone(domHtml(container), PATH_PREFIX_FRAGMENTS, "唯一展示名泄露了本机路径");
        // 对照：那一项确实渲染出来了——否则上一条可能只是「什么都没渲染」。
        expectEqual(
          queryAll(container, "[data-directory-display-name]").map((node) => node.textContent),
          ["solo"],
          "唯一展示名的目录项",
        );
        expectAbsent(domText(container), soloId, "唯一展示名的目录项泄露了目录标识");
      });
    },
  },
  {
    name: "R15-3 会话数由会话摘要得出（总数胶囊恒在、运行中/待处理徽标只在非零时追加），且断连后仍列出内容",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitForDirectoryList(mounted.container);
        // 在线时来源标注必须是「本次同步」，否则下面的 last_sync 断言没有对照价值。
        await waitFor(
          () => query(mounted.container, '[data-content-origin="live"]') !== null,
          "连接在线后目录页标注 live",
        );

        const alphaRow = query(mounted.container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
        const betaRow = query(mounted.container, `[data-directory-id="${DIRECTORY_ID_BETA}"]`);
        // alpha 一条运行中会话；beta 一条等待输入会话（计入待处理）。零值那一档由徽标缺席表达。
        expectEqual(countsOf(alphaRow), ["1", "1", ""], "alpha 的会话总数/运行中/待处理");
        expectEqual(countsOf(betaRow), ["1", "", "1"], "beta 的会话总数/运行中/待处理");
        const alphaText = textOf(alphaRow);
        const betaText = textOf(betaRow);
        expectThat(alphaText.includes("运行中") && !alphaText.includes("待处理"), "alpha 只应带运行中徽标");
        expectThat(betaText.includes("待处理") && !betaText.includes("运行中"), "beta 只应带待处理徽标");

        // 断开连接：内容必须仍在，且来源标注切换成「上次同步」。
        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后目录页标注 last_sync",
        );
        expectEqual(queryAll(mounted.container, "[data-directory-id]").length, 2, "断连后的目录行数");
        const text = domText(mounted.container);
        expectNone(
          text,
          [DIRECTORY_ID_ALPHA, DIRECTORY_ID_BETA],
          "目录标识不得出现在界面上",
        );
        expectNone(domHtml(mounted.container), PATH_PREFIX_FRAGMENTS, "断连后的渲染结果泄露了本机路径");
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
            // 标题与指引按**显式锚点**取，不依赖 RNW 把 `role` 映射成 `h2`/`p`。
            textOf(node?.querySelector("[data-empty-state-title]")),
            textOf(node?.querySelector("[data-empty-state-guidance]")),
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
        expectEqual(directoryIdsOf(container), [DIRECTORY_ID_ALPHA, DIRECTORY_ID_BETA], "初始的两个目录项");

        // 「部署检查」只出现在 alpha 的会话标题里（展示名都是 api、目录标识也不含这三个字）。
        typeIntoInput(container, "[data-directory-search]", "部署", "目录页搜索框");
        await waitFor(() => directoryIdsOf(container).join(",") === DIRECTORY_ID_ALPHA, "按会话标题搜索后只剩 alpha");
        expectEqual(
          query(container, "[data-directory-search]")?.getAttribute("data-directory-search-value"),
          "部署",
          "受控搜索框回读到的查询词",
        );
        // 命中集合变了，但存活目录的三项汇总仍是它自己的全量数字，不是「命中的会话数」。
        expectEqual(
          countsOf(query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`)),
          ["2", "1", ""],
          "alpha 的会话总数/运行中",
        );
        // 未命中的目录连同它的会话行一起不呈现；命中目录里，只有标题命中的那条会话行留下
        // （原型 `passQuery`：展示名不命中时按会话标题逐行过滤）。
        expectEqual(
          queryAll(container, "[data-session-id]")
            .map((node) => node.getAttribute("data-session-id") ?? "")
            .sort(),
          [seriesSessionId("aaaaaaaa", 0)],
          "内联会话行只应剩标题命中的那一条",
        );

        // 目录标识不参与搜索：用 alpha 标识的前 8 位搜索，一个目录都不该命中。
        typeIntoInput(
          container,
          "[data-directory-search]",
          DIRECTORY_ID_ALPHA.slice(0, 8),
          "目录页搜索框",
        );
        await waitFor(
          () => query(container, '[data-directory-no-match="true"]') !== null,
          "按目录标识搜索不得有命中",
        );
        expectEqual(queryAll(container, "[data-directory-id]").length, 0, "目录标识命中时不得保留目录项");
        expectAbsent(domText(container), DIRECTORY_ID_ALPHA.slice(0, 8), "搜索结果里出现了目录标识");

        // 无命中：与两种 `data-empty-state` 互不冒充的第三种呈现。
        typeIntoInput(container, "[data-directory-search]", "没有这个目录", "目录页搜索框");
        await waitFor(
          () =>
            query(container, '[data-directory-search-value="没有这个目录"]') !== null &&
            query(container, '[data-directory-no-match="true"]') !== null,
          "搜索无结果的呈现",
        );
        expectEqual(queryAll(container, "[data-directory-id]").length, 0, "无命中时不得剩下目录项");
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
        await waitFor(
          () => directoryIdsOf(container).join(",") === DIRECTORY_ID_ALPHA,
          "筛选「运行中」后只剩 alpha",
        );
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
        expectEqual(
          countsOf(query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`)),
          ["2", "1", ""],
          "筛选不改 alpha 的会话总数与运行中数",
        );

        click(query(container, '[data-segment="筛选"] [data-segment-option="pending"]'), "「待处理」筛选");
        await waitFor(
          () => directoryIdsOf(container).join(",") === DIRECTORY_ID_BETA,
          "筛选「待处理」后只剩 beta",
        );
        expectEqual(
          countsOf(query(container, `[data-directory-id="${DIRECTORY_ID_BETA}"]`)),
          ["1", "", "1"],
          "筛选不改 beta 的会话总数与待处理数",
        );

        // 叠加：查询词「api」对两条目录都命中，决定性的因此仍然是「待处理」这一筛选。
        typeIntoInput(container, "[data-directory-search]", "api", "目录页搜索框");
        await waitFor(
          () => query(container, '[data-directory-search-value="api"]') !== null,
          "搜索词进入受控输入框",
        );
        expectEqual(directoryIdsOf(container), [DIRECTORY_ID_BETA], "搜索与筛选叠加后仍只剩 beta");
      });
    },
  },
  {
    name: "R15-10 超过内联上限的目录：只列 5 条最近的会话，并给出「查看全部 ›」入口",
    async run() {
      const sessions = sessionSeries({
        directoryId: DIRECTORY_ID_ALPHA,
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
        const card = query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
        expectThat(card !== null, "应有 alpha 的目录项");
        // 内联行只列最近的 5 条，且按会话自身的时间倒序；第 6 条不出现在目录页上。
        expectEqual(
          queryAll(card, "[data-session-id]").map((node) => node.getAttribute("data-session-id") ?? ""),
          [0, 1, 2, 3, 4].map((index) => seriesSessionId("aaaaaaaa", index)),
          "内联会话行必须是最近的 5 条、按时间倒序",
        );
        expectEqual(countsOf(card), ["6", "", ""], "目录项的会话总数按全量口径");

        // 入口文案里**不带 N**（原型这一轮去掉了 N）：写成「查看全部 6 个」会在下面两条里红。
        // 动作取值是 `open-directory-all`（与展示名入口的 `open-directory` 区分开）。
        const openAll = query(card, '[data-action="open-directory-all"]');
        expectThat(openAll !== null, "会话数超过内联上限时必须给出进入详情页的入口");
        expectEqual(textOf(openAll), "查看全部 ›", "「查看全部」入口的文案");
        // 原型 `.footline`：卡体底部再给一次入口（独立动作取值，避免与卡片头数成同一个）。
        const footAll = query(card, '[data-action="open-directory-all-footer"]');
        expectThat(footAll !== null, "超过内联上限时卡体底部应有「查看全部」");
        expectEqual(textOf(footAll), "查看全部 ›", "卡体底部入口的文案");
      });
    },
  },
  {
    name: "R15-11 0 会话的目录项仍有进入详情页的展示名入口，但不呈现「查看全部」（对照：有会话的目录项两个入口都在）",
    async run() {
      const onlyAlpha = sessionSeries({
        directoryId: DIRECTORY_ID_ALPHA,
        idPrefix: "aaaaaaaa",
        titles: ["部署检查"],
        at: ["2026-10-05T09:00:00.000Z"],
        states: ["running"],
      });
      await withScenario(baseScenario({ sessions: onlyAlpha }), async (container) => {
        await waitForDirectoryList(container);
        expectEqual(directoryIdsOf(container), [DIRECTORY_ID_ALPHA, DIRECTORY_ID_BETA], "两条目录都要呈现（一条没有会话）");
        const beta = query(container, `[data-directory-id="${DIRECTORY_ID_BETA}"]`);
        expectEqual(countsOf(beta), ["0", "", ""], "beta 的会话总数是零（另两档由徽标缺席表达）");
        // 0 条会话的卡体是那一行说明（原型 `.emptyline`），不是空白。
        expectThat(textOf(beta).includes("这个目录还没有会话"), "0 会话的卡体应说明「这个目录还没有会话」");
        // 0 条会话：没有「查看全部」（那里也没有会话可看），但**必须**留下展示名入口——
        // 否则这台机器上没有任何路径进入该目录的详情页，详情页的创建入口也随之不可达。
        expectThat(
          query(beta, '[data-action="open-directory-all"]') === null,
          "0 条会话的目录项不得给出「查看全部」入口（那里也没有会话可看）",
        );
        expectThat(
          query(beta, '[data-action="open-directory"]') !== null,
          "0 条会话的目录项必须仍有进入详情页的展示名入口",
        );
        // 对照：有会话的目录项两个入口都在。
        const alpha = query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
        expectThat(
          query(alpha, '[data-action="open-directory"]') !== null,
          "对照：有会话的目录项必须给出展示名入口",
        );
        const openAll = query(alpha, '[data-action="open-directory-all"]');
        expectThat(openAll !== null, "对照：有会话的目录项必须给出「查看全部」入口");
        expectEqual(textOf(openAll), "查看全部 ›", "「查看全部」入口的文案");
        // 两个入口各自每条目录项恰好一处：把新属性挂到卡片根节点或重复渲染都会在这里红。
        expectEqual(
          queryAll(container, '[data-action="open-directory"]').length,
          2,
          "每条目录项恰好一个展示名入口",
        );
        expectEqual(
          queryAll(container, '[data-action="open-directory-all"]').length,
          1,
          "只有有会话的那一项带「查看全部」",
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
        expectAbsent(domText(mounted.container), "已同步", "No visible sync timestamp");
        // 重新渲染（敲一次查询）后时刻不得变：它不是「渲染时的当前时间」。
        typeIntoInput(mounted.container, "[data-directory-search]", "api", "目录页搜索框");
        await waitFor(
          () => query(mounted.container, '[data-directory-search-value="api"]') !== null,
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
      // 等待条件用真实目录页的加载态文案（占位 `RuntimeUnavailable` 不发它）。
      await withScenario(baseScenario({ deliverSnapshot: false }), async (container) => {
        await waitFor(
          () => query(container, '[data-directory-state="loading"]') !== null,
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
  {
    name: "R15-19 目录项不再呈现最近活动时间（该呈现已随原型撤销）",
    async run() {
      const sessions = sessionSeries({
        directoryId: DIRECTORY_ID_ALPHA,
        idPrefix: "aaaaaaaa",
        titles: ["十月四日", "十月六日", "九月二十日"],
        at: [
          "2026-10-04T09:00:00.000Z",
          "2026-10-06T09:00:00.000Z",
          "2026-09-20T09:00:00.000Z",
        ],
      });
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryList(container);
        // 反例：把目录项的最近活动时间加回来（锚点或「最近 …」文案）都会在这里红。
        expectThat(
          query(container, "[data-directory-latest-at]") === null,
          "目录项不得再呈现最近活动时间",
        );
        expectAbsent(domText(container), "最近 ", "目录项不得再呈现最近活动时间");
        // 对照：那一项仍然渲染出会话行与会话总数——不是「谁都没渲染」。
        const card = query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
        expectThat(card !== null, "对照：alpha 的目录项必须在");
        expectEqual(countsOf(card), ["3", "", ""], "对照：会话总数仍是全量口径");
        expectEqual(queryAll(card, "[data-session-id]").length, 3, "对照：三条会话行都在");
      });
    },
  },
  {
    name: "R15-21 内联会话行五字段：状态圆点、标题、等待状态词、Agent、相对时间",
    async run() {
      const sessions = sessionSeries({
        directoryId: DIRECTORY_ID_ALPHA,
        idPrefix: "aaaaaaaa",
        titles: ["授权中的一单", "等回信的一单", "尚未开始的一单"],
        at: ["2026-10-05T09:00:00.000Z", "2026-10-04T09:00:00.000Z", "2026-10-03T09:00:00.000Z"],
        states: ["waiting_permission", "waiting_input", "idle"],
      });
      await withScenario(baseScenario({ sessions }), async (container) => {
        await waitForDirectoryList(container);
        const card = query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
        // 目录项找不到时给一条**可读的**失败，而不是让下面几条断言各自在空值上假红。
        expectThat(card !== null, "alpha 的目录项必须在，才能看到它的内联会话行");
        const rows = queryAll(card, "[data-session-id]");
        expectEqual(rows.length, 3, "alpha 的三条内联会话行");

        // 圆点上的状态标识（机器证据）+ 与状态对应的状态词。
        expectEqual(
          rows.map((row) => query(row, "[data-session-state]")?.getAttribute("data-session-state")),
          ["waiting_permission", "waiting_input", "idle"],
          "三条行圆点上的状态标识",
        );
        expectEqual(
          rows.map((row) => textOf(query(row, "[data-session-title]"))),
          // 逐字取自夹具里的标题（行序由 `updatedAt` 倒序钉死，见上一条状态断言）。
          // 第三行是 `idle` 那一单：它的标题必须与夹具一致，而不是被写成状态词「空闲的一单」——
          // 后者会让下面的 `!rows[2].includes("空闲")` 自相矛盾。
          ["授权中的一单", "等回信的一单", "尚未开始的一单"],
          "三条行的标题",
        );
        expectThat(textOf(rows[0]).includes("待授权"), "等待授权必须在行内说明原因");
        expectThat(textOf(rows[1]).includes("等你回复"), "等待输入必须在行内说明原因");
        expectThat(!textOf(rows[2]).includes("空闲"), "非等待状态不追加状态词（只有圆点标识状态）");
        expectEqual(textOf(query(rows[0] ?? null, "[data-session-agent]")), "codex", "Agent 名称");

        // 时间按**相对口径**呈现（刚刚 / N 分钟前 / 年月日），绝不是 `MM-DD HH:MM` 那种绝对切片。
        for (const row of rows) {
          const label = textOf(query(row, "[data-session-updated-at]"));
          expectThat(RELATIVE_TIME_PATTERN.test(label), `会话时间应是相对口径，实际 ${JSON.stringify(label)}`);
        }

        // 会话行不得带任何路径信息：整条路径与父段一个片段都不许出现。
        expectNone(domHtml(card), PATH_PREFIX_FRAGMENTS, "会话行泄露了本机路径");
      });
    },
  },
  {
    name: "R15-22 连接状态只有一个表达：路由自建的状态徽标与导航条都已从目录页移除",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);

        // **目录页自己那一层**：`data-route="dirs"` 的父节点（路由组件的根 `View role="main"`）。
        // 自建导航与第二个状态徽标都曾是这一层里的兄弟节点；顶栏（含设备胶囊）由根布局装配的
        // 共享外壳承担，在**外面**一层，因此这里量到的正是「路由自己拼出来的东西」。
        const pageLayer = query(container, '[data-route="dirs"]')?.parentElement ?? null;
        expectThat(pageLayer !== null, '目录页应渲染出带 `data-route="dirs"` 的容器');

        // 导航：路由不再自建「配对」「主机与连接」两个入口（那些由外壳的设备胶囊承担）。
        expectThat(
          query(pageLayer, '[data-action="open-pairing"]') === null,
          "目录页不得再自建「配对」导航入口",
        );
        // 第二个状态徽标：`data-action="open-hosts"` 只有 `ConnectionBadge` 会发，是它的指纹。
        expectThat(
          query(pageLayer, '[data-action="open-hosts"]') === null,
          "目录页不得再出现自建的状态徽标",
        );
        // 主机与连接面板不再内联在目录页：它由设备胶囊进入 `/hosts`。这一层里因此不应
        // 出现任何连接状态控件——状态只由外壳那一个设备胶囊表达。
        expectThat(
          query(pageLayer, '[data-connection-state]') === null,
          "目录页层里不得再出现连接状态控件",
        );
        expectThat(
          query(pageLayer, '[data-route="hosts"]') === null,
          "主机与连接面板不得再内联在目录页",
        );
      });
    },
  },
  {
    name: "R15-20 目录页的创建入口不再有权限维度：每个目录项恒有一个入口、没有权限提示条，且点得开弹层",
    async run() {
      await withScenario(baseScenario(), async (container) => {
        await waitForDirectoryList(container);

        // 入口是**呈现出来的**（不是藏起来），且不是禁用态：设备不再有创建会话的权限维度，
        // 因此没有任何判定能让它不可用。把门控加回来（禁用、或按权限不呈现）→ 这两条变红。
        expectEqual(
          queryAll(container, '[data-action="create-session"]').length,
          2,
          "每个目录项各有一个「⊕ 新建」入口（入口恒可用）",
        );
        for (const entry of queryAll(container, '[data-action="create-session"]')) {
          expectEqual(
            entry.hasAttribute("disabled"),
            false,
            `目录项创建入口不得是禁用态（${entry.getAttribute("data-directory-id") ?? "?"}）`,
          );
        }

        // 权限痕迹必须**一处都没有**：提示条、说明条与「电脑端授予」类说法都随权限维度一起消失。
        expectThat(
          query(container, "[data-create-permission-note]") === null,
          "目录页不得再呈现创建权限提示条",
        );
        expectThat(
          query(container, "[data-create-entry-unavailable]") === null,
          "目录页不得再把创建入口呈现成「不可用并给出说明」",
        );
        expectNone(domText(container), PERMISSION_GATE_VOCABULARY, "目录页出现了权限门控类文案");

        // 点得开：入口真的接到弹层（不是画了一个按钮）。
        click(query(container, '[data-action="create-session"]'), "第一个目录项的创建入口");
        await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层打开");
        expectEqual(queryAll(container, "[data-create-sheet]").length, 1, "同一时刻只有一个创建弹层");
        // 副作用说明仍在（spec「创建入口的副作用说明」）：它曾与权限态成对出现，现在恒在。
        expectThat(
          textOf(query(container, "[data-create-note]")).includes("Agent 进程"),
          "创建弹层必须说明会在电脑上启动一个 Agent 进程",
        );
      });
    },
  },
];

export const R15_CHECKS = checks;
