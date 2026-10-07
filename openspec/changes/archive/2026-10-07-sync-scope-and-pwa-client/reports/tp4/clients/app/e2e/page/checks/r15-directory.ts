// 【交接副本】来源 worktree .worktrees/tp4 的 clients/app/e2e/page/checks/r15-directory.ts
// 交付提交 0a19d39b4c63b723c7e7d7edbaaf64e60cd82e83（分支 feat/tp4），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R15「目录页以目录为主角且可离线渲染」的浏览器级验证。
 *
 * 每条用例都跑在**真实页面**上：`app/index.tsx` / `app/dir/[alias].tsx` → 展示组件 →
 * `src/features/` 的模型，数据来自**真实快照管线**（经真实 SHA-256 两级摘要校验过的
 * `sync.snapshot_*` 帧，由真实 `SyncClient` 校验后交给 `ClientStore`）。
 *
 * ## 判别力
 *
 * - 「不泄露本机路径」用**负向断言**：快照里真的塞进了协议中不存在的 `path` / `root`，
 *   断言渲染后的 DOM 里连片段都没有。只断言「模型对象没有 path 键」证明不了这一点。
 * - 「断连仍可呈现」先断言在线时来源是 `live`，再断开连接断言变 `last_sync` 且列表仍在——
 *   单看 `last_sync` 那一帧区分不出「断连后仍渲染」与「从来没有过内容」。
 * - 「两种空态可区分」分别挂两个场景，比对 `data-empty-state` 与标题、指引文案。
 */

import {
  AGENTS,
  DIRS_ROUTE,
  SECRET_FRAGMENTS,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
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

/** 等到目录列表出现（快照已验证并替换了仓库）。 */
async function waitForDirectoryList(container: HTMLElement): Promise<void> {
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

/** 一条场景跑完就卸载：`dispose()` 之后的组合根不可复活，留着会污染下一条用例。 */
async function withScenario<T>(
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

        const rows = queryAll(container, "li[data-directory-alias]");
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

        const alphaRow = query(mounted.container, 'li[data-directory-alias="alpha"]');
        const betaRow = query(mounted.container, 'li[data-directory-alias="beta"]');
        expectEqual(textOf(alphaRow?.querySelector('[data-directory-count="total"]')), "1", "alpha 的会话数");
        expectEqual(textOf(alphaRow?.querySelector('[data-directory-count="active"]')), "1", "alpha 的运行中数");
        expectEqual(textOf(alphaRow?.querySelector('[data-directory-count="pending"]')), "0", "alpha 的待处理数");
        expectEqual(textOf(betaRow?.querySelector('[data-directory-count="total"]')), "1", "beta 的会话数");
        expectEqual(textOf(betaRow?.querySelector('[data-directory-count="active"]')), "0", "beta 的运行中数");
        expectEqual(textOf(betaRow?.querySelector('[data-directory-count="pending"]')), "1", "beta 的待处理数");

        // 断开连接：内容必须仍在，且来源标注切换成「上次同步」。
        mounted.host?.drop();
        await waitFor(
          () => query(mounted.container, '[data-content-origin="last_sync"]') !== null,
          "断连后目录页标注 last_sync",
        );
        expectEqual(queryAll(mounted.container, "li[data-directory-alias]").length, 2, "断连后的目录行数");
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
    name: "R15-5 目录详情页同样不泄露路径，且列出该目录下的会话",
    async run() {
      const mounted = await mountScenario(baseScenario({ route: dirRoute("alpha") }));
      try {
        // 等待条件必须用只有真实 `DirectoryDetail` 才发的属性：`[data-route="dir-detail"]`
        // 同时被占位 `RuntimeUnavailable` 命中，用它等待会在首帧占位时就成立，
        // 随后的「展示名 / 别名」断言便与 `start()` 的 resolve 形成竞态。
        // `data-directory-alias` 只在真实详情页算出模型后出现（占位不发）。
        await waitFor(
          () => query(mounted.container, '[data-route="dir-detail"][data-directory-alias]') !== null,
          "目录详情页",
        );
        expectEqual(
          textOf(query(mounted.container, "[data-directory-display-name]")),
          "api",
          "详情页的目录展示名",
        );
        expectEqual(textOf(query(mounted.container, "[data-directory-alias-label]")), "alpha", "详情页的别名");
        expectEqual(queryAll(mounted.container, "li[data-session-id]").length, 1, "详情页应列出该目录下的会话");
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
        await waitFor(
          () => query(container, '[data-route="dir-detail"][data-directory-alias]') !== null,
          "空目录的详情页",
        );
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
];

export const R15_CHECKS = checks;