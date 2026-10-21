/**
 * 目录详情页渲染层的判定（R15：页头、平铺分页、两种空、响应式、副作用提示）。
 *
 * ## 判别力从哪来
 *
 * 断言的是**渲染后的 HTML 字符串**而不是模型字段：「目录标识不呈现」「路径逐字呈现且不派生」
 * 这类要求只有落到「哪一段文字出现在同一份渲染结果里」才是真的。为此本文件把渲染结果
 * 里**所有标签剥掉**再断言文本（见 `textContentOf`）：`data-directory-id` 之类的机器锚点
 * 本来就带着目录标识，只有文本层才能回答「用户看得见目录标识吗」。
 *
 * `renderToStaticMarkup` 没有事件、也没有布局引擎，因此这里**不**断言交互与滚动：
 * 「点上一页/下一页」「输入期间列表不重画、回车与失焦才提交（非法页码就地纠正）」
 * 「翻页后回到页首」由 e2e 用例（`e2e/page/checks/r15-directory-detail.ts`）与 feature 层单测
 * （`features/directory-detail-paging.test.ts`）覆盖。这里断言的是：页码、总页数、输入框上
 * **真实的 `value` 属性**（= 纠正后的生效页码）、翻页控件的**有无**，以及「禁用」落在按钮真实的
 * `disabled` 属性而不是某个 `data-*` 锚点的名字上。
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
import { buildDirectoryDetailModel } from "../features/directory-model";
import type { DirectoryDetailModel } from "../features/directory-model";
import type { SessionSummary } from "../protocol";
import { WORKSPACES, WORKSPACES_WITH_PATHS, resources, session, storeWith } from "../features/testing/fixtures";

/** 宽屏那一档的注入宽度（组件内部断点取 `breakpoints.wide`）。 */
const WIDE_WIDTH = 1024;
/** 窄屏那一档的注入宽度。 */
const NARROW_WIDTH = 375;

/** 渲染目录详情。所有可选 props 都给显式缺省值：调用点不会传 `undefined`（`exactOptionalPropertyTypes`）。 */
function renderDetail(
  model: DirectoryDetailModel,
  options: {
    readonly availableWidth?: number;
  } = {},
): string {
  return renderToStaticMarkup(
    createElement(DirectoryDetail, {
      model,
      onBack: () => undefined,
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      onPageChange: () => undefined,
      availableWidth: options.availableWidth ?? WIDE_WIDTH,
    }),
  );
}

/** 渲染结果里全部的 `data-*` 属性（键与取值都在内），排序后便于按集合比较。 */
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

/**
 * 标签上是否带**真**的 `disabled` 属性。
 *
 * RNW 把 `disabled={true}` 渲染成 `disabled=""`（另附 `aria-disabled="true"`，见标签原文）。
 * 判禁用**必须**按这个属性判，不能靠子串：属性**名**里带 `disabled` 的锚点（本组件曾输出过
 * `data-page-disabled="true|false"`）会让 `toContain("disabled")` 两个方向都假通过——把
 * `disabled={…}` 整句删掉、只留那个锚点，断言照样绿（那个锚点因此也被删了）。
 *
 * 模式要求 `disabled` 是**独立属性**（前面是空白，后面紧跟 `=`、空白、`>` 或 `/`），
 * 因此 `aria-disabled="true"` 与 `data-page-disabled="true"` 里的那两次出现都不算数。
 */
function isDisabled(tag: string): boolean {
  return /(^|\s)disabled(=""|\s|>|\/)/.test(tag);
}

/**
 * 渲染结果的**可见文本**：剥掉全部标签与实体后剩下的文字。
 *
 * 目录标识、路径这类「该不该让用户看见」的要求必须在文本层判定——机器锚点（`data-directory-id`）
 * 本来就带目录标识，拿整串 HTML 断言只会得到自相矛盾的结论。
 */
function textContentOf(html: string): string {
  return html
    .replace(/<[^>]*>/g, " ")
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

/** 会话行的标识，按 DOM 顺序（= 视图模型的顺序）。 */
function rowIdsOf(html: string): readonly string[] {
  return [...html.matchAll(/data-session-id="([^"]*)"/g)].map((match) => match[1] ?? "");
}

/**
 * `count` 条 `work-api` 下的会话，每条比上一条早一分钟：倒序即编号顺序，
 * 因此「第几页是哪几条」在断言里是可预期的（`会话 N` 的编号与页号对得上）。
 */
function manySessions(count: number): readonly SessionSummary[] {
  const base = Date.parse("2026-10-02T12:00:00.000Z");
  return Array.from({ length: count }, (_, index) =>
    session({
      id: `dddddddd-0000-4000-8000-${(index + 1).toString().padStart(12, "0")}`,
      title: `会话 ${index + 1}`,
      state: index % 3 === 0 ? "running" : "idle",
      updatedAt: new Date(base - index * 60_000).toISOString(),
    }),
  );
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

describe("R15 目录详情：两种「空」互不冒充", () => {
  it("目录存在但没有任何会话：呈现「还没有会话」，不呈现「链接失效」，翻页固定渲染但禁用", () => {
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="empty"');
    expect(html).toContain("这个目录还没有会话");
    // 反例：把「还没有会话」当成目录标识失效，下面两条会各自变红。
    expect(html).not.toContain("可能已被删除");
    expect(html).not.toContain("data-directory-detail-state");
    // 一条会话都没有时总页数是 1：翻页固定渲染，两个按钮禁用。
    expect(html).toContain('data-directory-detail-pager="true"');
    expect(html).toContain('data-directory-page-count="1"');
    expect(isDisabled(tagContaining(html, 'data-action="page-prev"'))).toBe(true);
    expect(isDisabled(tagContaining(html, 'data-action="page-next"'))).toBe(true);
  });

  it("目录里有会话：既不呈现空态也不呈现链接失效（详情页没有「被筛掉」的第三种空）", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="list"');
    expect(rowIdsOf(html)).toHaveLength(2);
    // 反例：把「详情页有查询词/月份档」的旧第三支留下时，这里会出现空态标记或那句文案。
    expect(html).not.toContain("这个目录还没有会话");
    expect(html).not.toContain("没有符合条件的会话");
    expect(html).not.toContain("data-directory-detail-state");
  });

  it("对照：目录标识不在快照里才是「链接已失效」——与上面两种在文案与标记上都不同", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("not-there"));

    expect(html).toContain('data-directory-detail-state="unknown_directory"');
    expect(html).toContain("可能已被删除");
    // 提示里**不得**出现目录标识（它只作机器锚点）：反例是把文案写回「目录「not-there」不在…」。
    expect(textContentOf(html)).not.toContain("not-there");
    expect(textContentOf(html)).toContain("这个目录不在最近一次同步的目录列表里");
    expect(html).not.toContain("这个目录还没有会话");
    expect(html).not.toContain("data-directory-sessions");
    // 失效标识下也要有离开本页的入口：自建导航删掉后，返回按钮只在这里（见组件内的页头）。
    expect(html).toContain('data-action="back-to-dirs"');
  });

  it("从未同步过：呈现等待同步，不冒充任何一种空态，也不呈现同步时间", () => {
    const { store } = storeWith({ resources: null });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-detail-state="loading"');
    expect(html).not.toContain("data-directory-sessions");
    expect(html).not.toContain("已同步");
    expect(html).toContain('data-action="back-to-dirs"');
  });
});

describe("R15 目录详情：页头（返回 + 标题 + 会话数芯片 + 路径）", () => {
  it("四项都在：返回入口、展示名标题、会话数芯片、路径标签与逐字路径值", () => {
    const { store } = storeWith({
      resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
    });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-action="back-to-dirs"');
    expect(html).toContain('data-directory-display-name="api"');
    // 芯片：数字与文案都来自模型（`total` = 该目录的全部会话数，不是本页条数）。
    expect(html).toContain('data-directory-session-count="2"');
    expect(textContentOf(html)).toContain("2 个会话");
    // 路径行是「淡标签 + 值」两段（原型的 `.drow .dk` + `.dpath`）：行上有逐字的路径属性，
    // 行内的文字先出现标签「路径」、再出现那一串路径——组件不做任何拼接、规范化或截断
    // （超宽交给 CSS 省略号，DOM 文本仍是逐字的那一串）。
    const rowAt = html.indexOf('data-directory-path="D:\\work\\api"');
    expect(rowAt, "路径行没带上逐字路径").toBeGreaterThanOrEqual(0);
    const labelAt = html.indexOf("路径", rowAt);
    expect(labelAt, "路径行缺少「路径」标签").toBeGreaterThanOrEqual(0);
    // 从标签之后开始找：属性里那次出现在标签之前，因此这里命中的是渲染出的值。
    expect(html.indexOf("D:\\work\\api", labelAt), "路径值必须出现在标签之后").toBeGreaterThanOrEqual(0);
  });

  it("目录标识不呈现：它只作为路由与命令引用的内部标识留在机器锚点上", () => {
    const { store } = storeWith({
      resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
    });
    const html = renderDetail(store.directoryDetail("work-api"));

    // 反例：把目录标识再当标题/副标题渲染出来时，下面两条会红（`data-directory-id-label` 是旧锚点）。
    expect(textContentOf(html)).not.toContain("work-api");
    expect(html).not.toContain("data-directory-id-label");
    // 目录标识仍在机器锚点上：路由与命令引用需要它，R15 的既有断言也靠它选节点。
    expect(html).toContain('data-directory-id="work-api"');
  });

  it("路径始终呈现：逐字来自快照条目的 `path`，不拼接也不替代", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-path="D:\\work\\api"');
    expect(textContentOf(html)).toContain("路径");
    // DOM 里的文本逐字是那一段路径。
    expect(textContentOf(html)).toContain("D:\\work\\api");
  });
});

describe("R15 目录详情：平铺 + 每页 10 条 + 翻页", () => {
  it("第一页只给最新的 10 条，页码与总页数上到 DOM", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(25), workspaces: WORKSPACES }) });
    const model = store.directoryDetail("work-api");
    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;

    const html = renderDetail(model);
    // 平铺倒序：第一页是最新的十条（夹具里编号越小越新），第 11 条只在第二页。
    expect(rowIdsOf(html)).toEqual(manySessions(25).slice(0, 10).map((item) => item.sessionId));
    expect(textContentOf(html)).toContain("会话 1");
    expect(textContentOf(html)).not.toContain("会话 11");
    // 芯片是**全部**会话数，翻页区里的页码是当前页 / 总页数：两个数字各有出处。
    expect(html).toContain('data-directory-session-count="25"');
    expect(html).toContain('data-directory-detail-pager="true"');
    expect(html).toContain('data-directory-page="1"');
    expect(html).toContain('data-directory-page-count="3"');
    expect(html).toContain('data-directory-detail-page-value="1"');
    // 真属性：`disabled=""`（不是 `data-*` 锚点里的子串）。
    expect(isDisabled(tagContaining(html, 'data-action="page-prev"'))).toBe(true);
    expect(isDisabled(tagContaining(html, 'data-action="page-next"'))).toBe(false);
  });

  it("请求最后一页：只给那 5 条，且「下一页」在边界上不可用", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(25), workspaces: WORKSPACES }) });
    store.setDirectoryDetailPage("work-api", "3");
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(rowIdsOf(html)).toHaveLength(5);
    expect(textContentOf(html)).toContain("会话 25");
    expect(textContentOf(html)).not.toContain("会话 20");
    expect(html).toContain('data-directory-page="3"');
    expect(html).toContain('data-directory-detail-page-value="3"');
    expect(isDisabled(tagContaining(html, 'data-action="page-next"'))).toBe(true);
    expect(isDisabled(tagContaining(html, 'data-action="page-prev"'))).toBe(false);
  });

  it("只有一页时翻页控件仍然渲染，上一页/下一页均为禁用", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(9), workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(rowIdsOf(html)).toHaveLength(9);
    expect(html).toContain('data-directory-session-count="9"');
    expect(html).toContain('data-directory-detail-pager="true"');
    expect(html).toContain('data-directory-page="1"');
    expect(html).toContain('data-directory-page-count="1"');
    expect(html).toContain('data-directory-detail-page-value="1"');
    expect(isDisabled(tagContaining(html, 'data-action="page-prev"'))).toBe(true);
    expect(isDisabled(tagContaining(html, 'data-action="page-next"'))).toBe(true);
    expect(textContentOf(html)).toContain("/ 1 页");
  });

  it("页码输入框回填的是**纠正后**的值：非数字回落当前页、越界夹到边界", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(25), workspaces: WORKSPACES }) });
    store.setDirectoryDetailPage("work-api", "2");
    // 非数字：回落到当前生效页（2），列表因此还是第二页那十条。
    store.setDirectoryDetailPage("work-api", "abc");
    const corrected = renderDetail(store.directoryDetail("work-api"));
    expect(corrected).toContain('data-directory-detail-page-value="2"');
    // 回填落在输入框**真实的 `value` 属性**上（草稿为空时输入框显示的恒是生效页码）：
    // 反例：`value` 挂到别处（或写死不回填）时这里会红。
    expect(tagContaining(corrected, "data-directory-detail-page-input")).toContain('value="2"');
    expect(textContentOf(corrected)).toContain("会话 11");

    // 越界：夹到上界（3），输入框与列表同时落到最后一页。
    store.setDirectoryDetailPage("work-api", "99");
    const clamped = renderDetail(store.directoryDetail("work-api"));
    expect(clamped).toContain('data-directory-detail-page-value="3"');
    expect(tagContaining(clamped, "data-directory-detail-page-input")).toContain('value="3"');
    expect(textContentOf(clamped)).toContain("会话 21");

    // 下界：0 落到第 1 页。
    store.setDirectoryDetailPage("work-api", "0");
    const first = renderDetail(store.directoryDetail("work-api"));
    expect(first).toContain('data-directory-detail-page-value="1"');
    expect(tagContaining(first, "data-directory-detail-page-input")).toContain('value="1"');
    expect(textContentOf(first)).toContain("会话 1");
  });
});

describe("R15 目录详情：不呈现内容来源与同步时间", () => {
  /** 与连接状态无关的详情模型：断连时页面仍照常渲染快照内容。 */
  function detailModel(): DirectoryDetailModel {
    return buildDirectoryDetailModel({
      resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }),
      directoryId: "work-api",
      ownerLabelOf: () => "另一台电脑",
      nowMs: 1_700_000_000_000,
    });
  }

  it("断连时也没有来源标注条、同步时刻或来源机器属性", () => {
    const model = detailModel();
    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;
    expect("contentOrigin" in model).toBe(false);

    const html = renderDetail(model);
    expect(html).not.toContain("data-content-origin");
    expect(html).not.toContain("data-origin-notice");
    expect(html).not.toContain("data-synced-at");
    expect(html).not.toContain("内容来自上次同步");
    expect(html).not.toContain("已同步");
    // 页面内容本身照常呈现。
    expect(html).toContain('data-directory-sessions="list"');
    expect(textContentOf(html)).toContain("2 个会话");
  });
});

describe("R15 目录详情：按设备宽度自适应，且信息集合相同", () => {
  it("窄屏与宽屏渲染出完全相同的 data-* 集合（差异只在样式）", () => {
    const { store } = storeWith({
      resources: resources({ sessions: manySessions(25), workspaces: WORKSPACES_WITH_PATHS }),
    });
    const model = store.directoryDetail("work-api");
    const narrow = renderDetail(model, {
      availableWidth: NARROW_WIDTH,
    });
    const wide = renderDetail(model, {
      availableWidth: WIDE_WIDTH,
    });

    expect(dataAttributes(narrow)).toEqual(dataAttributes(wide));
    // 判别力：宽度真的换过一档（否则上面那条「集合相同」在宽度被忽略时也会绿）。
    expect(narrow).not.toBe(wide);
  });

  it("窄屏下会话行、会话数芯片、创建入口与翻页都还在", () => {
    const { store } = storeWith({
      resources: resources({ sessions: manySessions(25), workspaces: WORKSPACES_WITH_PATHS }),
    });
    const narrow = renderDetail(store.directoryDetail("work-api"), { availableWidth: NARROW_WIDTH });

    expect(narrow).toContain('data-directory-sessions="list"');
    expect(narrow).toContain("data-session-id");
    expect(narrow).toContain('data-directory-session-count="25"');
    expect(narrow).toContain('data-action="open-create-sheet"');
    expect(narrow).not.toContain("将在你的电脑上启动一个 Agent 进程");
    // 反例：窄屏把翻页藏起来（MUST NOT 因宽度不足改变判定结果）时这里的锚点会消失。
    expect(narrow).toContain('data-directory-detail-pager="true"');
    expect(narrow).toContain("data-directory-detail-page-input");
  });
});

describe("R15 目录详情：创建入口", () => {
  it("入口恒可用，且不再附带副作用说明或权限提示", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    const entry = tagContaining(html, 'data-action="open-create-sheet"');
    expect(html).not.toContain('data-create-side-effect="true"');
    expect(html).not.toContain("将在你的电脑上启动一个 Agent 进程");
    // 反例：入口若因权限不可用，这里会带 `disabled=""` → 断言变红。
    expect(isDisabled(entry)).toBe(false);
    // 反例：权限门控若残留，这里会出现 `create-entry-unavailable` 或那句「需要电脑端授予」。
    expect(html).not.toContain("data-create-entry-unavailable");
    expect(html).not.toContain("需要电脑端授予");
    expect(html).not.toContain("没有创建会话的权限");
  });
});

describe("R15 目录详情：既有 data-* 与列表语义逐一保留", () => {
  it("一条带标题的会话 + 一条来源离线的会话渲染出其余既有标记", () => {
    const { store } = storeWith({ resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-route="dir-detail"');
    expect(html).toContain('data-directory-id="work-api"');
    expect(html).toContain('data-directory-display-name="api"');
    expect(html).not.toContain('data-content-origin');
    expect(html).toContain('data-directory-sessions="list"');
    expect(html).toContain('data-session-id="dddddddd-0000-4000-8000-000000000001"');
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain('data-session-agent="true"');
    // 来源离线不再有行内标注（功能原因由输入区/文件栏各自承担）。
    expect(html).not.toContain('data-origin-offline="true"');
    expect(html).not.toContain("当前离线");
    expect(html).toContain('data-action="open-session"');
    expect(html).toContain('data-action="open-create-sheet"');
    // 创建入口不再按权限门控：ready 态既不呈现「不可用」标记，也不呈现权限文案。
    expect(html).not.toContain('data-create-entry-unavailable');
    expect(html).not.toContain("需要电脑端授予");
    // 三项汇总徽标已随本变更移除（它是旧详情页的呈现，原型上只有「N 个会话」芯片）。
    expect(html).not.toContain("data-directory-count");
    // ready 分支不呈现「目录标识失效 / 等待同步」这两个状态标记。
    expect(html).not.toContain("data-directory-detail-state");

    // 列表语义仍在：容器 role="list" 渲染成 `<ul>`，会话行 role="listitem" 渲染成 `<li>`。
    expect(tagContaining(html, 'data-directory-sessions="list"')).toContain('role="list"');
    expect(html).toContain("<ul");
    expect(html).toContain("<li");
  });

  it("只呈现快照里的逐字路径：不派生父目录，也不漏出快照之外的键", () => {
    const { store } = storeWith({
      resources: resources({ sessions: DETAIL_SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
    });
    const html = renderDetail(store.directoryDetail("work-api"));

    // `path` 必填且逐字呈现：那一整串只来自快照条目。
    expect(html).toContain('data-directory-path="D:\\work\\api"');
    expect(textContentOf(html)).toContain("D:\\work\\api");
    // 反例：组件若从目录标识派生「父目录」，或把快照之外的多余键（夹具里的 `root`、
    // 将来可能出现的 `cwd`）漏出来，去掉那条逐字路径后仍会留下反斜杠片段/盘符路径。
    const withoutPath = textContentOf(html).split("D:\\work\\api").join("");
    expect(withoutPath).not.toContain("\\");
    expect(withoutPath).not.toMatch(/[A-Za-z]:[\\/]/);
  });
});
