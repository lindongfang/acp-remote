/**
 * R15：目录页渲染层的负向断言。
 *
 * 每条 MUST NOT 都写成「构造违规输入 → 断言渲染结果不含违规内容」：
 *
 * - 目录项只显示展示名与别名 → 快照资源里的 `path`/`root` 不得出现在 HTML 里；
 * - 同名目录不以路径消歧 → 两个同名目录必须各自带别名，且没有任何路径片段；
 * - 两种空态可区分 → 两段文案与 `data-empty-state` 都不同；
 * - 断连仍可渲染并标注来源 → 断连时目录行仍在，且标注「来自上次同步」；
 * - 目录详情里「别名失效」与「目录存在但无会话」是两种呈现 → 两种文案互不冒充。
 *
 * 判别力：断言的是**渲染后的 HTML 字符串**而不是模型字段——模型里没有路径字段并不能
 * 保证组件不会去渲染别的东西（例如从 alias 派生一个「父目录」标签）。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`，
 * 组件本身仍是 `.tsx`（在 `src/components/` 下照常被类型检查与 web 打包）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { DirectoryDetail } from "./DirectoryDetail";
import { DirectoryList } from "./DirectoryList";
import { DIRECTORY_EMPTY_COPY, buildDirectoryPageModel } from "../features/directory-model";
import type { DirectoryDetailModel, DirectoryPageModel, SessionRowModel } from "../features/directory-model";
import { CREATE_DENIED_NOTE, buildCreateSessionEntryModel } from "../features/create-session-model";
import { WORKSPACES, WORKSPACES_WITH_PATHS, resources, session, storeWith } from "../features/testing/fixtures";

const SESSIONS = [
  session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000002", state: "waiting_permission" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000003", state: "idle", alias: "personal-api" }),
];

/**
 * 渲染目录页。点击回调在渲染测试里不会被触发。
 *
 * 新 props 都带上：搜索框与筛选段只有传了回调才呈现，缺省（不传）就是既有调用点的样子。
 * `sessionsByAlias` 缺省不带这一键（`exactOptionalPropertyTypes` 不允许显式传 `undefined`），
 * 等价于「不列内联会话行」。
 */
function renderDirectoryList(
  model: DirectoryPageModel,
  options: {
    readonly sessionsByAlias?: ReadonlyMap<string, readonly SessionRowModel[]>;
    readonly query?: string;
    readonly createPermissionNote?: string | null;
  } = {},
): string {
  return renderToStaticMarkup(
    createElement(DirectoryList, {
      model,
      onOpenDirectory: () => undefined,
      query: options.query ?? "",
      onQueryChange: () => undefined,
      filter: "all",
      onFilterChange: () => undefined,
      onOpenSession: () => undefined,
      createPermissionNote: options.createPermissionNote ?? null,
      ...(options.sessionsByAlias === undefined ? {} : { sessionsByAlias: options.sessionsByAlias }),
    }),
  );
}

/**
 * 渲染目录详情。模型由 `ClientStore.directoryDetail` 取（与页面同一条取数路径），
 * 因此这里断言的是用户真正看到的那句话，而不是模型字段。
 */
function renderDirectoryDetail(model: DirectoryDetailModel): string {
  return renderToStaticMarkup(
    createElement(DirectoryDetail, {
      model,
      createEntry: buildCreateSessionEntryModel({
        directoryAlias: "work-api",
        scopes: ["session.create"],
        agents: [],
        record: null,
        connectionOnline: true,
      }),
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      canCreate: true,
    }),
  );
}

describe("R15 目录项不暴露路径", () => {
  it("带路径字段的目录项渲染后不含任何路径信息", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
        connection: "online",
        lastSyncedAtMs: 1_700_000_000_000,
      }),
    );

    // 反例：若组件把 path/root 当作附加信息渲染，这里会出现 `D:\work`，断言变红。
    expect(html).not.toContain("D:\\work");
    expect(html).not.toContain("D:\\personal");
    expect(html).not.toContain("path=");
    expect(html).not.toContain("root=");
    // 展示名与别名仍然必须在（否则「只显示展示名与别名」会被实现成「什么都不显示」）。
    expect(html).toContain("work-api");
    expect(html).toContain("personal-api");
  });

  it("同名目录各自带别名，且没有任何路径片段用于消歧", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    // 两个同名目录是两项，别名各自出现。
    expect(html.match(/data-directory-display-name="api"/g)).toHaveLength(2);
    expect(html).toContain(">work-api<");
    expect(html).toContain(">personal-api<");
    // 反例：若用「父目录/末段」消歧，这里会出现反斜杠或盘符，断言变红。
    expect(html).not.toContain("\\");
    expect(html).not.toMatch(/[A-Za-z]:[\\/]/);
  });

  it("目录项渲染出三项汇总（会话数/运行中/待处理）", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    expect(html).toContain('data-directory-count="total"');
    expect(html).toContain('data-directory-count="active"');
    expect(html).toContain('data-directory-count="pending"');
  });
});

describe("R15 断连时目录仍可渲染", () => {
  it("断连时目录行照常渲染，并明确标注内容来自上次同步", () => {
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
    });
    const html = renderDirectoryList(model);

    expect(model.kind === "loading" ? null : model.contentOrigin).toBe("last_sync");
    expect(html).toContain('data-content-origin="last_sync"');
    expect(html).toContain("内容来自上次同步");
    // 反例：断连时不渲染目录项，这里会是空列表，断言变红。
    expect(html).toContain('data-directory-alias="work-api"');
  });

  it("对照：在线时标注为本次同步（证明上一条不是因为标注写死）", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );
    expect(html).toContain('data-content-origin="live"');
  });
});

describe("R15 两种空态可区分", () => {
  it("目录存在但没有会话：呈现该空态的文案与指引", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: [], workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );
    expect(html).toContain('data-empty-state="directories_without_sessions"');
    expect(html).toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    expect(html).toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
  });

  it("一个目录都没有：呈现另一种空态，且与上一种在标题/指引上都不同", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: [], workspaces: [] }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    expect(html).toContain('data-empty-state="no_directories"');
    expect(html).toContain(DIRECTORY_EMPTY_COPY.no_directories.title);
    // 反例：若两种空态共用一句话，这里会同时出现上一种的指引，断言变红。
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
    expect(DIRECTORY_EMPTY_COPY.no_directories.title).not.toBe(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    expect(DIRECTORY_EMPTY_COPY.no_directories.guidance).not.toBe(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
  });

  it("从未同步过：呈现 loading，不冒充任何一种空态", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({ resources: null, connection: "online", lastSyncedAtMs: null }),
    );
    // 反例：若无快照时渲染「本机没有任何目录」，这里会出现 data-empty-state，断言变红。
    expect(html).toContain('data-directory-state="loading"');
    expect(html).not.toContain("data-empty-state");
  });
});

describe("R15 目录页：内联会话行 / 搜索无结果 / 查看全部", () => {
  it("目录项内联该目录的会话行（标题 / Agent / 状态 / 时间），且不额外引入消歧依据", () => {
    // 生产路径：模型与会话行都经 ClientStore 取（与页面同一条取数路径）。
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDirectoryList(store.directoryPage(), {
      sessionsByAlias: store.directoryPageSessions(),
    });

    // 空标题呈现为「未命名会话」由 `SessionRow` 的口径保证；这里断言四个可见字段都在。
    expect(html).toContain('data-session-id="aaaaaaaa-0000-4000-8000-000000000001"');
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain('data-session-agent="true"');
    expect(html).toContain('data-session-state="running"');
    expect(html).toContain('data-session-updated-at=');
    // 反例：若内联行顺手带上了目录的展示名/别名（当作消歧依据），计数会大于目录项数。
    expect(html.match(/data-directory-display-name="/g)).toHaveLength(2);
    expect(html.match(/data-directory-alias-label/g)).toHaveLength(2);
  });

  it("内联行与详情页的查询词/月份互不影响，且搜索/筛选不发任何命令", () => {
    const { store, gateway } = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      scopes: ["session.list"],
    });
    const before = [...(store.directoryPageSessions().get("work-api") ?? [])].map((row) => row.id);

    // 目录页的查询/筛选与详情页的查询/月份一起改（它们都是 store 的 UI 状态）。
    store.setDirectoryQuery("work-api");
    store.setDirectoryFilter("running");
    store.setDirectoryDetailQuery("一条都不命中");
    store.setDirectoryDetailMonth("2026-10");

    // 反例：若目录页的内联行借道 `directoryDetail`，这里会被详情页的查询词/月份过滤掉。
    expect([...(store.directoryPageSessions().get("work-api") ?? [])].map((row) => row.id)).toEqual(before);

    // 对照：详情页确实吃到了那两个字段——否则上一条断言可能只是「什么都没发生」。
    const detail = store.directoryDetail("work-api");
    expect(detail.kind === "ready" ? detail.sessions.length : -1).toBe(0);

    // spec「搜索与筛选不依赖在线查询」：这两个 UI 状态不派发任何命令。
    expect(gateway.dispatches).toHaveLength(0);
  });

  it("搜索或筛选没有命中：呈现「没有匹配的结果」，且**不占用** data-empty-state", () => {
    // 有快照、有目录，只是查询词一个都不命中 → `no_match`，不是任何一种空态。
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "online",
      lastSyncedAtMs: null,
      query: "不存在的目录或会话",
    });
    expect(model.kind).toBe("no_match");

    const html = renderDirectoryList(model, { query: "不存在的目录或会话" });

    expect(html).toContain('data-directory-no-match="true"');
    expect(html).toContain("没有匹配的结果");
    // 反例：若把 no_match 当成空态渲染（例如复用 `EmptyState`），这里会出现 `data-empty-state`，
    // 与 R15-4 的两种空态断言互相污染——本条就是防这个的。
    expect(html).not.toContain("data-empty-state");
    // 反例：若两种空态的词表被顺手复用，这里会出现它们的标题。
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.no_directories.title);
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    // 「搜索无结果」时必须留下工具条：否则用户没有把关键词/筛选改回去的入口。
    expect(html).toContain('data-directory-search="true"');
  });

  it("会话数超过内联展示上限：出现「查看全部 N 个」入口，N 取全量会话数", () => {
    const many = Array.from({ length: 7 }, (_, index) =>
      session({
        id: `bbbbbbbb-0000-4000-8000-00000000000${String(index)}`,
        title: `会话 ${String(index)}`,
      }),
    );
    const withMany = { resources: resources({ sessions: many, workspaces: WORKSPACES }) };
    const model = buildDirectoryPageModel({ ...withMany, connection: "online", lastSyncedAtMs: null });
    const html = renderDirectoryList(model, {
      sessionsByAlias: storeWith(withMany).store.directoryPageSessions(),
    });

    // N 是全量口径（7 条），不是内联行数（上限 5）：反例是把 N 写成内联行数。
    expect(html).toContain("查看全部 7 个");
    // 反例：若内联不限条数地全列出来，这里会出现 7 条会话行。
    expect(html.match(/data-session-id="/g)).toHaveLength(5);
    // 超过上限时必须有进入详情页的入口，否则「更早的会话」无处可看。
    expect(html).toContain('data-action="open-directory"');
  });

  it("对照：一个会话都没有的目录项不呈现「查看全部」入口", () => {
    // 只给 `work-api` 一条会话：`personal-api` 的会话数是 0（列表仍非空 → 仍是 ready）。
    const onlyWorkApi = [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" })];
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: onlyWorkApi, workspaces: WORKSPACES }),
      connection: "online",
      lastSyncedAtMs: null,
    });

    const html = renderDirectoryList(model);

    // 反例：若无条件渲染入口，两个目录项会各出一个「查看全部 0 个」。
    expect(html.match(/data-action="open-directory"/g)).toHaveLength(1);
    expect(html).not.toContain("查看全部 0 个");
  });

  it("无创建会话权限：呈现提示条；有权限时不呈现（缺省不留痕）", () => {
    // 权限只由 scopes 决定（与目录别名无关）——这里就按页面那条取数路径拿权限模型。
    const deniedStore = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      scopes: ["session.list"],
    }).store;
    const deniedEntry = deniedStore.createEntry("");
    expect(deniedEntry.permission.kind).toBe("denied");

    const denied = renderDirectoryList(deniedStore.directoryPage(), {
      createPermissionNote: deniedEntry.permission.note,
    });
    expect(denied).toContain('data-create-permission-note="true"');
    expect(denied).toContain(CREATE_DENIED_NOTE);

    // 有权限时页面传 `null`（组件的缺省也是不留痕）。
    const grantedEntry = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      scopes: ["session.create"],
    }).store.createEntry("");
    expect(grantedEntry.permission.kind).toBe("granted");

    const granted = renderDirectoryList(deniedStore.directoryPage());
    expect(granted).not.toContain("data-create-permission-note");
    expect(granted).not.toContain(CREATE_DENIED_NOTE);
  });

  it("断连且有上次同步时刻：来源旁给出同步时间；从未同步过时不编造时间", () => {
    const offline = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
    });
    const withClock = renderDirectoryList(offline);
    expect(withClock).toContain('data-content-origin="last_sync"');
    // 精确值经属性进 DOM；展示文本是 **UTC 固定切片**，因此可以逐字断言（不随机器时区漂移）。
    expect(withClock).toContain('data-synced-at="1700000000000"');
    expect(withClock).toContain("已同步 2023-11-14 22:13 UTC");

    // 从未成功替换过快照：`lastSyncedAtMs` 为 null → 不得出现任何同步时间。
    const never = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "disconnected",
      lastSyncedAtMs: null,
    });
    const withoutClock = renderDirectoryList(never);
    expect(withoutClock).not.toContain("data-synced-at");
    expect(withoutClock).not.toContain("已同步");
  });
});

describe("R15 目录详情：链接失效与「目录存在但无会话」是两种呈现", () => {
  it("目录存在但没有任何会话：渲染「还没有会话」，不渲染「链接已失效」的告警", () => {
    // 生产路径：模型经 ClientStore 取出（与页面同一路径），不直调构建函数。
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="empty"');
    expect(html).toContain("这个目录里还没有会话");
    // 反例：若详情模型把「有目录但无会话」当成别名失效，这里渲染的是告警，断言变红。
    expect(html).not.toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain("可能已被删除");
  });

  it("对照：别名不在快照里才是「链接已失效」——两种呈现互不冒充", () => {
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("not-there"));

    expect(html).toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain('data-directory-sessions="empty"');
  });
});
