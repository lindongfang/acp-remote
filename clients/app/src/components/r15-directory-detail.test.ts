/**
 * 目录详情页渲染层的判定（R15 + 本次新增的目录内搜索 / 月份分档 / 响应式 / 副作用提示）。
 *
 * ## 判别力从哪来
 *
 * 断言的是**渲染后的 HTML 字符串**而不是模型字段：模型里没有路径字段，并不能保证组件不会
 * 去渲染别的东西；同理，「三种空互不冒充」只有落到「哪一段文案出现在同一份渲染结果里」才是
 * 真的。
 *
 * `renderToStaticMarkup` 没有事件、也没有时钟，因此这里**不**断言交互（「输入查询词 → 列表
 * 变化」由 `features/directory-filter.test.ts` 的纯函数单测与 e2e 覆盖）。这里断言的是：
 * 页面把仓库里的查询词/月份档原样交给控件（`data-directory-detail-search-value`、
 * `data-segment-selected`），以及视图模型里的 N/M 真的被渲染出来。
 *
 * 宽度用 `availableWidth` 注入：Node 下 `Dimensions.get('window')` 恒为 0，只有注入了两种宽度，
 * 「窄屏与宽屏的信息集合相同」才是一条能红的断言。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { DirectoryDetail } from "./DirectoryDetail";
import { CREATE_DENIED_NOTE, buildCreateSessionEntryModel } from "../features/create-session-model";
import { buildDirectoryDetailModel } from "../features/directory-model";
import type { DirectoryDetailModel } from "../features/directory-model";
import { CROSS_MONTH_SESSIONS, WORKSPACES, resources, session, storeWith } from "../features/testing/fixtures";

/** 宽屏那一档的注入宽度（组件内部断点是 720）。 */
const WIDE_WIDTH = 1024;
/** 窄屏那一档的注入宽度。 */
const NARROW_WIDTH = 375;

/** 渲染目录详情。所有可选 props 都给显式缺省值：调用点不会传 `undefined`（`exactOptionalPropertyTypes`）。 */
function renderDetail(
  model: DirectoryDetailModel,
  options: {
    readonly query?: string;
    readonly canCreate?: boolean;
    readonly lastSyncedAtMs?: number | null;
    readonly availableWidth?: number;
  } = {},
): string {
  const canCreate = options.canCreate ?? true;
  return renderToStaticMarkup(
    createElement(DirectoryDetail, {
      model,
      createEntry: buildCreateSessionEntryModel({
        directoryAlias: "work-api",
        scopes: canCreate ? ["session.create"] : [],
        agents: [],
        record: null,
        connectionOnline: true,
      }),
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      canCreate,
      query: options.query ?? "",
      lastSyncedAtMs: options.lastSyncedAtMs ?? null,
      availableWidth: options.availableWidth ?? WIDE_WIDTH,
      onQueryChange: () => undefined,
      onMonthChange: () => undefined,
    }),
  );
}

/** 渲染结果里全部 `data-*` 属性（键与取值都在内），排序后便于按集合比较。 */
function dataAttributes(html: string): readonly string[] {
  return (html.match(/data-[a-zA-Z0-9-]+="[^"]*"/g) ?? []).slice().sort();
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`），用于断言标签自身的属性（例如 `disabled`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  const start = html.lastIndexOf("<", at);
  const end = html.indexOf(">", at);
  return html.slice(start, end + 1);
}

/** 一条 `work-api` 下的会话 + 一条 imported 且来源离线的会话。 */
const DETAIL_SESSIONS = [
  session({ id: "dddddddd-0000-4000-8000-000000000001", title: "检索词命中", state: "running" }),
  session({
    id: "dddddddd-0000-4000-8000-000000000002",
    title: "远端会话",
    state: "idle",
    origin: {
      kind: "remote",
      ownerNodeId: "77777777-7777-4777-8777-777777777777",
      exportId: "export-1",
      originEpoch: "88888888-8888-4888-8888-888888888888",
      online: false,
    },
  }),
];

describe("R15 目录详情：三种「空」互不冒充", () => {
  it("目录存在但没有任何会话：呈现「还没有会话」，不呈现「没有符合条件的会话」，也不说成链接失效", () => {
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="empty"');
    expect(html).toContain("这个目录里还没有会话");
    // 反例：把「还没有会话」当成「搜索无结果」，或当成别名失效，下面三条会各自变红。
    expect(html).not.toContain("没有符合条件的会话");
    expect(html).not.toContain("data-directory-detail-no-match");
    expect(html).not.toContain("可能已被删除");
    expect(html).not.toContain("data-directory-detail-state");
  });

  it("目录里有会话、但目录内搜索无命中：呈现「没有符合条件的会话」，且不占用 data-directory-sessions", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    store.setDirectoryDetailQuery("查无此项");
    const html = renderDetail(store.directoryDetail("work-api"), {
      query: store.state.directoryDetailQuery,
    });

    expect(html).toContain('data-directory-detail-no-match="true"');
    expect(html).toContain("没有符合条件的会话");
    // 反例：这一支若复用「还没有会话」或占用 data-directory-sessions，R15 的两种空态就被合并了。
    expect(html).not.toContain("这个目录里还没有会话");
    expect(html).not.toContain("data-directory-sessions");
    expect(html).not.toContain("可能已被删除");
  });

  it("对照：别名不在快照里才是「链接已失效」——与上面两种在文案与标记上都不同", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("not-there"));

    expect(html).toContain('data-directory-detail-state="unknown_directory"');
    expect(html).toContain("可能已被删除");
    expect(html).not.toContain("这个目录里还没有会话");
    expect(html).not.toContain("没有符合条件的会话");
    expect(html).not.toContain("data-directory-sessions");
    expect(html).not.toContain("data-directory-detail-no-match");
  });

  it("从未同步过：呈现等待同步，不冒充任何一种空态，也不呈现同步时间", () => {
    const { store } = storeWith({ resources: null });
    const html = renderDetail(store.directoryDetail("work-api"), { lastSyncedAtMs: null });

    expect(html).toContain('data-directory-detail-state="loading"');
    expect(html).not.toContain("data-directory-sessions");
    expect(html).not.toContain("data-directory-detail-no-match");
    expect(html).not.toContain("已同步");
  });
});

describe("R15 目录详情：月份分档与「当前显示 N / M」", () => {
  it("两个月份档各自只含本月会话，且档按时间倒序", () => {
    const { store } = storeWith({ resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }) });
    const model = store.directoryDetail("work-api");
    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;

    expect(model.months.map((bucket) => bucket.key)).toEqual(["2026-10", "2026-09"]);
    expect(model.shown).toBe(2);
    expect(model.total).toBe(2);

    const html = renderDetail(model);
    expect(html).toContain("十月的发布");
    expect(html).toContain("九月的重构");
    expect(html).toContain("当前显示 2 / 2");
    expect(html).toContain('data-directory-shown="2"');
    expect(html).toContain('data-directory-total="2"');
  });

  it("切到某个月份档后 N 变小而 M 不变，且只渲染该月的会话", () => {
    const { store } = storeWith({ resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }) });
    const before = store.directoryDetail("work-api");
    store.setDirectoryDetailMonth("2026-10");
    const after = store.directoryDetail("work-api");
    expect(before.kind).toBe("ready");
    expect(after.kind).toBe("ready");
    if (before.kind !== "ready" || after.kind !== "ready") return;

    // 分子随月份档变小，分母（查询命中后的总数）不变。
    expect(after.shown).toBeLessThan(before.shown);
    expect(after.total).toBe(before.total);

    const html = renderDetail(after);
    expect(html).toContain("当前显示 1 / 2");
    expect(html).toContain('data-directory-shown="1"');
    expect(html).toContain('data-directory-total="2"');
    expect(html).toContain("十月的发布");
    // 反例：月份档没接上（或不按会话自身时间分档）时，九月的会话仍会出现。
    expect(html).not.toContain("九月的重构");
  });

  it("月份分段控件呈现「全部」+ 各月份档 key，并标出当前档", () => {
    const { store } = storeWith({ resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }) });
    const all = renderDetail(store.directoryDetail("work-api"));

    expect(all).toContain('data-segment="月份"');
    expect(all).toContain('data-segment-option="all"');
    expect(all).toContain('data-segment-option="2026-09"');
    expect(all).toContain('data-segment-option="2026-10"');
    expect(tagContaining(all, 'data-segment-option="all"')).toContain('data-segment-selected="true"');

    store.setDirectoryDetailMonth("2026-09");
    const september = renderDetail(store.directoryDetail("work-api"));
    expect(tagContaining(september, 'data-segment-option="2026-09"')).toContain('data-segment-selected="true"');
    expect(tagContaining(september, 'data-segment-option="all"')).toContain('data-segment-selected="false"');
    expect(september).toContain("九月的重构");
    expect(september).not.toContain("十月的发布");
  });

  it("已选月份档在查询命中后消失：控件选中态落到「全部」，而不是整组全 false", () => {
    const { store } = storeWith({ resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }) });
    // 用户先选了 2026-09，然后输入的查询词只命中 2026-10 的会话——`months` 随之收缩，
    // 那个被选中的档已不在选项集合里。
    store.setDirectoryDetailMonth("2026-09");
    store.setDirectoryDetailQuery("十月的发布");

    const model = store.directoryDetail("work-api");
    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;
    // 模型层对账：请求的档算出的生效值回落 "all"（目录页/详情页的档集合只有 2026-10）。
    expect(model.month).toBe("all");
    expect(model.months.map((bucket) => bucket.key)).toEqual(["2026-10"]);

    const html = renderDetail(model, { query: store.state.directoryDetailQuery });
    // 反例：若控件仍直取仓库里的原始值（这里是 "2026-09"，选项里已经没有了），
    // 整组 data-segment-selected 会全为 false；若模型不对账，列表还会被滤成空白。
    expect(tagContaining(html, 'data-segment-option="all"')).toContain('data-segment-selected="true"');
    expect(html).not.toContain('data-segment-option="2026-09"');
    expect(html).toContain('data-directory-shown="1"');
    expect(html).toContain('data-directory-total="1"');
    expect(html).toContain("十月的发布");
    // 搜索框里确实有查询词，也就是说列表不是被「查询无命中」挡住的（它本来就该命中）。
    expect(html).toContain('data-directory-detail-search-value="十月的发布"');
    expect(html).not.toContain("没有符合条件的会话");
  });
});

describe("R15 目录详情：目录内搜索只在本目录内生效", () => {
  const TWO_DIRECTORIES = [
    session({ id: "cccccccc-0000-4000-8000-000000000001", title: "本目录检索词", alias: "work-api" }),
    session({ id: "cccccccc-0000-4000-8000-000000000002", title: "别的目录检索词", alias: "personal-api" }),
  ];

  it("查询词命中别的目录的会话时不呈现任何会话（其它目录不参与匹配）", () => {
    const { store } = storeWith({ resources: resources({ sessions: TWO_DIRECTORIES, workspaces: WORKSPACES }) });
    store.setDirectoryDetailQuery("别的目录检索词");
    const html = renderDetail(store.directoryDetail("work-api"), {
      query: store.state.directoryDetailQuery,
    });

    expect(html).toContain("没有符合条件的会话");
    // 反例：匹配跨目录时，这里会列出那条属于 personal-api 的会话。
    // 断言按**会话标识**而不是查询词：搜索框是受控的，会把查询词回显进它的 value，
    // 因此「HTML 里没有查询词」这条断言本身就与实现相矛盾（同文件的对照用例也依赖这处回显）。
    expect(html).not.toContain("cccccccc-0000-4000-8000-000000000002");
    expect(html).not.toContain("personal-api");
  });

  it("对照：查询词命中本目录的会话时正常列出，且不列出其它目录的会话", () => {
    const { store } = storeWith({ resources: resources({ sessions: TWO_DIRECTORIES, workspaces: WORKSPACES }) });
    store.setDirectoryDetailQuery("本目录检索词");
    const html = renderDetail(store.directoryDetail("work-api"), {
      query: store.state.directoryDetailQuery,
    });

    expect(html).toContain("本目录检索词");
    expect(html).toContain('data-directory-shown="1"');
    expect(html).toContain('data-directory-total="1"');
    expect(html).not.toContain("别的目录检索词");
    // 查询词原样交给搜索框（否则用户看到的框是空的，输入就被吞掉了）。
    expect(html).toContain('data-directory-detail-search-value="本目录检索词"');
  });

  it("详情页的目录内搜索与目录页的查询词是两个独立字段", () => {
    const { store } = storeWith({ resources: resources({ sessions: TWO_DIRECTORIES, workspaces: WORKSPACES }) });
    // 目录页的查询词只命中 personal-api：若它漏进详情页，work-api 会被整页过滤成「无命中」。
    store.setDirectoryQuery("别的目录检索词");

    const html = renderDetail(store.directoryDetail("work-api"), { query: store.state.directoryDetailQuery });
    expect(store.state.directoryDetailQuery).toBe("");
    expect(html).toContain("本目录检索词");
    expect(html).not.toContain("没有符合条件的会话");

    // 在详情页输入查询词（页面对搜索框做的事）：目录页的查询词必须原样还在。
    store.setDirectoryDetailQuery("本目录检索词");
    expect(store.state.directoryQuery).toBe("别的目录检索词");
    expect(store.directoryPage().kind).toBe("ready");
  });
});

describe("R15 目录详情：断连时标注上次同步时刻", () => {
  /** 断连（内容来自上次同步）的详情模型。 */
  function offlineModel(connection: "disconnected" | "online"): DirectoryDetailModel {
    return buildDirectoryDetailModel({
      resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }),
      connection,
      alias: "work-api",
      ownerLabelOf: () => "另一台电脑",
      query: "",
      month: "all",
    });
  }

  it("内容来自上次同步时，在来源标注旁给出同步时刻", () => {
    const model = offlineModel("disconnected");
    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;
    // 断连（含未配对）时内容来源必须是「上次同步」。
    expect(model.contentOrigin).toBe("last_sync");

    const html = renderDetail(model, { lastSyncedAtMs: 1_700_000_000_000 });
    expect(html).toContain('data-content-origin="last_sync"');
    expect(html).toContain("已同步 2023-11-14 22:13 UTC");
    expect(html).toContain('data-synced-at="1700000000000"');
  });

  it("对照：在线（本次同步）时不呈现同步时刻", () => {
    const html = renderDetail(offlineModel("online"), { lastSyncedAtMs: 1_700_000_000_000 });

    expect(html).toContain('data-content-origin="live"');
    expect(html).not.toContain("已同步");
    expect(html).not.toContain("data-synced-at");
  });

  it("从未成功替换过快照：即使来源标记是 last_sync，也不呈现任何同步时间", () => {
    const html = renderDetail(offlineModel("disconnected"), { lastSyncedAtMs: null });

    expect(html).toContain('data-content-origin="last_sync"');
    expect(html).not.toContain("已同步");
    expect(html).not.toContain("data-synced-at");
  });
});

describe("R15 目录详情：按设备宽度自适应，且信息集合相同", () => {
  it("窄屏与宽屏渲染出完全相同的 data-* 集合（差异只在样式）", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const model = store.directoryDetail("work-api");
    const narrow = renderDetail(model, {
      query: "检索词",
      lastSyncedAtMs: store.state.lastSyncedAtMs,
      availableWidth: NARROW_WIDTH,
    });
    const wide = renderDetail(model, {
      query: "检索词",
      lastSyncedAtMs: store.state.lastSyncedAtMs,
      availableWidth: WIDE_WIDTH,
    });

    expect(dataAttributes(narrow)).toEqual(dataAttributes(wide));
    // 判别力：宽度真的换过一档（否则上面那条「集合相同」在宽度被忽略时也会绿）。
    expect(narrow).not.toBe(wide);
  });

  it("窄屏下会话行、三项汇总与创建入口都还在", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const narrow = renderDetail(store.directoryDetail("work-api"), { availableWidth: NARROW_WIDTH });

    expect(narrow).toContain('data-directory-sessions="list"');
    expect(narrow).toContain("data-session-id");
    expect(narrow).toContain('data-directory-count="total"');
    expect(narrow).toContain('data-directory-count="active"');
    expect(narrow).toContain('data-directory-count="pending"');
    expect(narrow).toContain('data-action="open-create-sheet"');
    expect(narrow).toContain("将在你的电脑上启动一个 Agent 进程");
  });
});

describe("R15 目录详情：创建入口的副作用与权限说明", () => {
  it("有权限：入口可用，旁边给出「将在你的电脑上启动一个 Agent 进程」，不出现拒绝说明", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"), { canCreate: true });

    const entry = tagContaining(html, 'data-action="open-create-sheet"');
    expect(html).toContain('data-create-side-effect="true"');
    expect(html).toContain("将在你的电脑上启动一个 Agent 进程");
    expect(entry).not.toContain("disabled");
    expect(html).not.toContain("data-create-entry-unavailable");
    expect(html).not.toContain(CREATE_DENIED_NOTE);
  });

  it("无权限：入口仍呈现（不隐藏）但不可用，并同时给出拒绝说明与副作用说明", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"), { canCreate: false });

    const entry = tagContaining(html, 'data-action="open-create-sheet"');
    expect(entry).toContain("disabled");
    expect(html).toContain('data-create-entry-unavailable="true"');
    expect(html).toContain(CREATE_DENIED_NOTE);
    expect(html).toContain("将在你的电脑上启动一个 Agent 进程");
  });
});

describe("R15 目录详情：既有 data-* 与列表语义逐一保留", () => {
  it("一条带标题的会话 + 一条来源离线的会话渲染出全部既有标记", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"), { canCreate: false });

    expect(html).toContain('data-route="dir-detail"');
    expect(html).toContain('data-directory-alias="work-api"');
    expect(html).toContain('data-directory-display-name="api"');
    expect(html).toContain('data-directory-alias-label="true"');
    expect(html).toContain('data-directory-count="total"');
    expect(html).toContain('data-directory-count="active"');
    expect(html).toContain('data-directory-count="pending"');
    expect(html).toContain('data-content-origin="last_sync"');
    expect(html).toContain('data-directory-sessions="list"');
    expect(html).toContain('data-session-id="dddddddd-0000-4000-8000-000000000001"');
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain('data-session-agent="true"');
    expect(html).toContain('data-origin-offline="true"');
    expect(html).toContain('data-action="open-session"');
    expect(html).toContain('data-action="open-create-sheet"');
    expect(html).toContain('data-create-entry-unavailable="true"');
    // ready 分支不呈现「别名失效 / 等待同步」这两个状态标记。
    expect(html).not.toContain("data-directory-detail-state");

    // 列表语义仍在：容器 role="list" 渲染成 `<ul>`，会话行 role="listitem" 渲染成 `<li>`。
    expect(tagContaining(html, 'data-directory-sessions="list"')).toContain('role="list"');
    expect(html).toContain("<ul");
    expect(html).toContain("<li");
  });

  it("详情页渲染结果里不出现任何路径片段", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    // 反例：组件若从别名派生「父目录」或把 cwd 之类的字段漏出来，下面两条会变红。
    expect(html).not.toContain("\\");
    expect(html).not.toMatch(/[A-Za-z]:[\\/]/);
  });
});
