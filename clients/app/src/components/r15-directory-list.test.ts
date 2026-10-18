/**
 * R15：目录页渲染层的负向断言。
 *
 * 每条 MUST / MUST NOT 都写成「构造能违反它的输入 → 断言渲染结果里没有违规内容」：
 *
 * - 路径只逐字取自服务端下发 → 快照里带上 `path`/`root`，断言渲染结果里只有**尾段**、
 *   没有整条路径、父段、盘符或任何分隔符（客户端拼接的路径一定带分隔符，因此这条能红）；
 * - 展示名唯一时不呈现路径 → 换一份展示名唯一的目录，断言连尾段锚点都不出现；
 * - 目录标识不出现在界面上 → 断言标识标签锚点与标识的可见文本都不在，而 `data-directory-id`
 *   仍每项恰好一处（它只作路由与命令引用的内部标识）；
 * - 两种空态可区分 → 两段文案与 `data-empty-state` 都不同；
 * - 断连仍可渲染并标注来源 → 断连时目录行仍在，且标注「来自上次同步」；
 * - 目录页不再呈现最近活动时间（该需求已随原型撤销）→ 整页不得出现 `data-directory-latest-at`；
 * - 空态指引指向列表 → 有目录时空态与目录列表**同时**呈现（含目录项），一个目录都没有时
 *   不得出现任何目录项；
 * - 每个目录项都可进入详情页 → 展示名入口**与会话数无关**，且与「查看全部 ›」的动作取值
 *   可区分（`[data-directory-id]` 仍每项恰好一处）；
 * - 三项汇总由会话摘要得出 → 会话总数胶囊恒在，运行中/待处理徽标只在非零时追加；
 * - 会话行五字段 → 圆点（带 `data-session-state`）、标题、状态词、Agent、相对时间；
 *   等待授权/等待输入各出现对应状态词，其余状态只留圆点；
 * - 目录内创建入口 → 调用方给了回调才呈现（缺省不留痕）。
 * - 顶部创建入口（工具条右端）→ 给了回调才呈现，位置在搜索框/筛选段之后、目录列表之前；
 *   空态与加载中同样在（一个目录都没登记时它是目录页上唯一的创建路径）。
 *
 * 逐项的断言用 `cardSegments` / `rowSegments` 把 HTML 切开：整页 `toContain` 分不清「这一项有、
 * 那一项没有」。
 *
 * 判别力：断言的是**渲染后的 HTML 字符串**而不是模型字段——模型里没有路径字段并不能
 * 保证组件不会去渲染别的东西（例如从展示名派生一个「父目录」标签）。
 *
 * 本文件用 `createElement` 而非 JSX：`vitest.config.ts` 的 `include` 只收 `*.test.ts`，
 * 组件本身仍是 `.tsx`（在 `src/components/` 下照常被类型检查与 web 打包）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { DirectoryDetail } from "./DirectoryDetail";
import { DirectoryList } from "./DirectoryList";
import { UNTITLED_SESSION_LABEL } from "../domain";
import type { SessionStateView } from "../domain/session-summary";
import { DIRECTORY_EMPTY_COPY, buildDirectoryPageModel } from "../features/directory-model";
import type {
  DirectoryDetailModel,
  DirectoryFilter,
  DirectoryPageModel,
  SessionRowModel,
} from "../features/directory-model";
import { WORKSPACES, WORKSPACES_WITH_PATHS, resources, session, storeWith } from "../features/testing/fixtures";
import type { WorkspaceCatalogEntry } from "../protocol";

const SESSIONS = [
  session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000002", state: "waiting_permission" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000003", state: "idle", directoryId: "personal-api" }),
];

/**
 * 渲染目录页。点击回调在渲染测试里不会被触发。
 *
 * 新 props 都带上：搜索框与筛选段只有传了回调才呈现，缺省（不传）就是既有调用点的样子。
 * `sessionsByDirectoryId` 缺省不带这一键（`exactOptionalPropertyTypes` 不允许显式传 `undefined`），
 * 等价于「不列内联会话行」。`onCreateSession` 是**必填**（入口恒可用），因此无条件传入。
 */
function renderDirectoryList(
  model: DirectoryPageModel,
  options: {
    readonly sessionsByDirectoryId?: ReadonlyMap<string, readonly SessionRowModel[]>;
    readonly query?: string;
    readonly filter?: DirectoryFilter;
    /** 顶部创建入口（工具条右端）的回调；**缺省不传** = 该入口不呈现（既有调用点的样子）。 */
    readonly onCreateSessionToBrowse?: () => void;
  } = {},
): string {
  return renderToStaticMarkup(
    createElement(DirectoryList, {
      model,
      onOpenDirectory: () => undefined,
      query: options.query ?? "",
      onQueryChange: () => undefined,
      filter: options.filter ?? "all",
      onFilterChange: () => undefined,
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      // `exactOptionalPropertyTypes`：缺省时不能显式传 `undefined`。
      ...(options.onCreateSessionToBrowse === undefined
        ? {}
        : { onCreateSessionToBrowse: options.onCreateSessionToBrowse }),
      ...(options.sessionsByDirectoryId === undefined ? {} : { sessionsByDirectoryId: options.sessionsByDirectoryId }),
    }),
  );
}

/**
 * 把渲染结果按**卡片根节点**切成一段一段：每段的起点就是该项根节点上的 `data-directory-id="`。
 *
 * 用**前瞻**切分（而不是把那个属性本身当分隔符吃掉），于是段内还能继续断言「这一项上一共有几处
 * 目录标识」——标识一旦被顺手挂到标题入口或「⊕ 新建」按钮上，段内计数就 > 1。逐段断言才能表达
 * 「**这一项**有没有那个入口/那个数字」：整页 `toContain` 分不清「A 项有、B 项没有」。
 * 段数即目录项数。
 */
function cardSegments(html: string): readonly string[] {
  return html.split(/(?=data-directory-id=")/).slice(1);
}

/** 把渲染结果按**会话行根节点**切开：段数即会话行数（同一手法，见 `cardSegments`）。 */
function rowSegments(html: string): readonly string[] {
  return html.split(/(?=data-session-id=")/).slice(1);
}

/**
 * 一个目录项上的三项汇总（按锚点取文本）。
 *
 * 运行中/待处理只在**非零**时占槽位，因此缺席即空串——`["2", "", ""]` 读作「只有会话总数，
 * 另两项为零」，与「汇总没渲染」是两回事（调用方断言具体的非空值）。
 */
function countsOf(card: string): readonly string[] {
  return ["total", "active", "pending"].map((kind) => {
    const matched = new RegExp(`data-directory-count="${kind}"[^>]*>([^<]*)<`).exec(card);
    return matched?.[1] ?? "";
  });
}

/** 渲染一个只含**一条**会话的目录页，并返回那条会话行的 HTML（用于逐状态断言行内五字段）。 */
function renderSingleSessionRow(state: SessionStateView, title: string | null = "一条会话"): string {
  const { store } = storeWith({
    resources: resources({
      sessions: [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state, title })],
      workspaces: WORKSPACES,
    }),
  });
  const html = renderDirectoryList(store.directoryPage(), {
    sessionsByDirectoryId: store.directoryPageSessions(),
  });
  // 只列了一条会话，且它在 `work-api` 的卡片里——卡片头在它**之前**，因此段内只剩这一行本身
  // （段尾可能带上后面那张零会话卡片的卡片头，但那里面没有状态词）。
  return rowSegments(html)[0] ?? "";
}

/**
 * 渲染目录详情。模型由 `ClientStore.directoryDetail` 取（与页面同一条取数路径），
 * 因此这里断言的是用户真正看到的那句话，而不是模型字段。
 */
function renderDirectoryDetail(model: DirectoryDetailModel): string {
  return renderToStaticMarkup(
    createElement(DirectoryDetail, {
      model,
      // 渲染测试只断言静态标记，交互不被触发：回调传不产生副作用的空实现。
      onBack: () => undefined,
      onOpenSession: () => undefined,
      onCreateSession: () => undefined,
      onPageChange: () => undefined,
    }),
  );
}

describe("R15 目录项：路径只逐字取自服务端下发", () => {
  it("同名目录呈现各自的路径尾段，且渲染结果里没有整条路径、父段、盘符或分隔符", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
        connection: "online",
        lastSyncedAtMs: 1_700_000_000_000,
      }),
    );

    // 两条目录的展示名都是 `api` → 消歧尾段逐字取自快照里的 `path`（`D:\work\api` → `work`）。
    expect(html).toContain('data-directory-path-tail="work"');
    expect(html).toContain('data-directory-path-tail="personal"');
    // 反例：若把整条路径（或它的父段、盘符）渲染出来，下面每一条都会变红。
    expect(html).not.toContain("D:\\work");
    expect(html).not.toContain("D:\\personal");
    expect(html).not.toContain("D:");
    // 客户端没有拼接任何路径分隔符：出现反斜杠就说明有一段路径是本地拼出来的。
    expect(html).not.toContain("\\");
    expect(html).not.toContain("path=");
    expect(html).not.toContain("root=");
    // 展示名仍然必须在（否则「呈现展示名与同名消歧尾段」会被实现成「什么都不显示」）。
    expect(html.match(/data-directory-display-name="api"/g)).toHaveLength(2);
  });

  it("目录标识不再作为界面元素：只以根节点的属性出现，MUST NOT 出现在可见文本里", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    // `data-directory-id` 仍是路由与命令引用的机器锚点：每项恰好一处。
    expect(html.match(/data-directory-id="/g)).toHaveLength(2);
    // 反例：标识标签或标识的可见文本一旦回来（这就是「目录标识呈现在界面上」），下面三条变红。
    expect(html).not.toContain("data-directory-id-label");
    expect(html).not.toContain(">work-api<");
    expect(html).not.toContain(">personal-api<");
  });

  it("展示名唯一时不呈现任何路径片段", () => {
    // 单独造一份**展示名唯一**的目录：feature 层据「同名才给尾段」把 `pathTail` 判成 `null`。
    const solo = [
      { directoryId: "solo", path: "E:\\secret\\solo-api" },
    ] as const satisfies readonly WorkspaceCatalogEntry[];

    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({
          sessions: [session({ id: "aaaaaaaa-0000-4000-8000-000000000009", directoryId: "solo" })],
          workspaces: solo,
        }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    // 反例：若「同名才给」这条判定失效（唯一名也画路径），这里会出现尾段锚点与路径片段。
    expect(html).not.toContain("data-directory-path-tail");
    expect(html).not.toContain("E:\\");
    expect(html).not.toContain("secret");
    // 对照：那一项仍然呈现出来（否则上一条可能只是「什么都没渲染」）。
    expect(html).toContain('data-directory-display-name="solo-api"');
  });

  it("同名消歧只呈现路径段：整条路径与分隔符都不出现", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    // 两条目录的尾段都是 `api`，消歧段分别是父目录那一段。
    expect(html).toContain('data-directory-path-tail="work"');
    expect(html).toContain('data-directory-path-tail="personal"');
    // 反例：把整条路径（或父段、盘符）当成文本呈现时下面逐条变红。
    expect(html).not.toContain("D:\\work\\api");
    expect(html).not.toContain("D:\\personal\\api");
    expect(html).not.toContain("D:");
    expect(html).not.toContain("\\");
  });

  it("三项汇总：会话总数胶囊恒在，运行中/待处理徽标只在非零时追加", () => {
    const html = renderDirectoryList(
      buildDirectoryPageModel({
        resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
        connection: "online",
        lastSyncedAtMs: null,
      }),
    );

    const cards = cardSegments(html);
    expect(cards).toHaveLength(2);
    const workApi = cards.find((card) => card.startsWith('data-directory-id="work-api"')) ?? "";
    const personalApi = cards.find((card) => card.startsWith('data-directory-id="personal-api"')) ?? "";

    expect(countsOf(workApi)).toEqual(["2", "1", "1"]);
    expect(countsOf(personalApi)).toEqual(["1", "", ""]);
    expect(workApi).toContain("● 运行中");
    expect(workApi).toContain("⏸ 待处理");
    // 反例：零值若也占槽位，这里会出现「● 运行中 0」/「⏸ 待处理 0」与两个空徽标。
    expect(personalApi).not.toContain("● 运行中");
    expect(personalApi).not.toContain("⏸ 待处理");
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
    expect(html).not.toContain("内容来自上次同步");
    // 反例：断连时不渲染目录项，这里会是空列表，断言变红。
    expect(html).toContain('data-directory-id="work-api"');
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
    // 反例：若无快照时渲染「还没有可用的目录」，这里会出现 data-empty-state，断言变红。
    expect(html).toContain('data-directory-state="loading"');
    expect(html).not.toContain("data-empty-state");
  });

  it("有目录但零会话：空态与目录列表**同时**呈现，列表在空态文案之后", () => {
    // 这就是那条死路：空态指引写着「在下面选一个目录进入详情页」，此前下面什么也没有。
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: [], workspaces: WORKSPACES }),
      connection: "online",
      lastSyncedAtMs: null,
    });
    expect(model.kind).toBe("empty");

    const html = renderDirectoryList(model);

    // 空态标记与文案仍按既有取值出现……
    expect(html).toContain('data-empty-state="directories_without_sessions"');
    expect(html).toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance);
    // ……指引下方的**列表也必须真的在**，否则指引指向空处。
    expect(html).toContain('data-directory-list="directories"');
    expect(html.match(/data-directory-id="/g)).toHaveLength(2);
    expect(html.indexOf(DIRECTORY_EMPTY_COPY.directories_without_sessions.guidance)).toBeLessThan(
      html.indexOf('data-directory-id="'),
    );
    // 每个目录项都带展示名入口 → 零会话的机器上仍能进入某个详情页（死路的另一半修复）。
    expect(html.match(/data-action="open-directory"/g)).toHaveLength(2);
    // 零会话的目录项：卡体是那一行「这个目录还没有会话」，而不是一个空块（原型 `.emptyline`）。
    expect(html).toContain("这个目录还没有会话");
    // 这些目录一条会话都没有 → 不得呈现任何活动时间（不拿当前时间/同步时刻顶替）。
    expect(html).not.toContain("data-directory-latest-at");
    expect(html).not.toContain("最近");
  });

  it("一个目录都没有的空态：不呈现任何目录项，也不呈现目录列表容器", () => {
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: [], workspaces: [] }),
      connection: "online",
      lastSyncedAtMs: null,
    });
    expect(model.kind).toBe("empty");

    const html = renderDirectoryList(model);

    expect(html).toContain('data-empty-state="no_directories"');
    // 反例：若空态无条件渲染列表容器，这里会出现空的 `data-directory-list`（消费者会误以为有列表）。
    expect(html).not.toContain("data-directory-list");
    expect(html).not.toContain("data-directory-id");
    expect(html).not.toContain('data-action="open-directory"');
  });
});

describe("R15 目录页：内联会话行 / 搜索无结果 / 查看全部", () => {
  it("目录项内联该目录的会话行（五字段都在），且不出现任何路径信息", () => {
    // 生产路径：模型与会话行都经 ClientStore 取（与页面同一条取数路径）。
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDirectoryList(store.directoryPage(), {
      sessionsByDirectoryId: store.directoryPageSessions(),
    });

    expect(html).toContain('data-session-id="aaaaaaaa-0000-4000-8000-000000000001"');
    expect(html).toContain('data-session-title="true"');
    expect(html).toContain('data-session-agent="true"');
    expect(html).toContain('data-session-state="running"');
    expect(html).toContain('data-session-updated-at=');
    // 反例：会话行一旦带上路径（或任何本地拼出的片段），这里会变红。
    expect(html).not.toContain("D:\\work\\api");
    expect(html).not.toContain("D:");
  });

  it("会话时间渲染的是模型给的相对文案（「20 分钟前」），不是组件自算的日期切片", () => {
    // 注入时钟让这条会话与 `nowMs` 正好差 20 分钟：`updatedAtLabel` 落在「20 分钟前」这个
    // **只有相对口径才写得出的取值**上。组件若绕过模型自己格式化 `updatedAt`，任何绝对口径
    // 都只会给出 `2026-10-05`（`slice(0, 10)` / `toISOString`）或 `10-05 09:00`（`MM-DD HH:MM`），
    // 于是这里断言的那段文案不会出现，用例变红。
    const updatedAt = "2026-10-05T09:00:00.000Z";
    const nowMs = Date.parse(updatedAt) + 20 * 60_000;
    const { store } = storeWith({
      resources: resources({
        sessions: [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", updatedAt })],
        workspaces: WORKSPACES,
      }),
      now: () => nowMs,
    });

    const html = renderDirectoryList(store.directoryPage(), {
      sessionsByDirectoryId: store.directoryPageSessions(nowMs),
    });

    // 精确值仍原样进 `data-*`（排序与逐字断言用它），与「渲染哪个文案」是两回事。
    expect(html).toContain(`data-session-updated-at="${updatedAt}"`);
    // 取时间元素**渲染出来的文本**：它与承载精确值的属性同处一个标签。
    const rendered = /data-session-updated-at="[^"]*"[^>]*>([^<]*)</.exec(html)?.[1] ?? "";
    expect(rendered).toBe("20 分钟前");
  });

  it("等待授权 / 等待输入各呈现对应状态词；其余状态只留状态圆点", () => {
    const permission = renderSingleSessionRow("waiting_permission");
    expect(permission).toContain('data-session-state="waiting_permission"');
    expect(permission).toContain("待授权");

    const waiting = renderSingleSessionRow("waiting_input");
    expect(waiting).toContain('data-session-state="waiting_input"');
    expect(waiting).toContain("等你回复");

    // 对照：非等待状态的圆点仍在（状态标识是机器证据），但**不**追加状态词——原型 `stword`
    // 同样只在这两档非空；把状态词一律画出来（或一律不画）都会在这里红。
    const idle = renderSingleSessionRow("idle");
    expect(idle).toContain('data-session-state="idle"');
    expect(idle).not.toContain("空闲");
    const running = renderSingleSessionRow("running");
    expect(running).toContain('data-session-state="running"');
    expect(running).not.toContain("运行中");
  });

  it("空标题呈现为「未命名会话」", () => {
    expect(renderSingleSessionRow("idle", null)).toContain(UNTITLED_SESSION_LABEL);
    // 对照：有标题时就渲染标题本身（否则上一条可能只是「恒定渲染那五个字」）。
    expect(renderSingleSessionRow("idle", "有标题的会话")).toContain("有标题的会话");
  });

  it("内联行按目录页的查询/筛选过滤；数据源本身不变，且不发任何命令", () => {
    const { store, gateway } = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
    });
    const before = [...(store.directoryPageSessions().get("work-api") ?? [])].map((row) => row.id);

    // 目录页的查询/筛选是 store 的 UI 状态：选择器本身仍给出该目录的全部行，
    // 可见性由展示层按原型 `passFilter` + `passQuery` 判定。
    store.setDirectoryQuery("api");
    store.setDirectoryFilter("running");
    expect([...(store.directoryPageSessions().get("work-api") ?? [])].map((row) => row.id)).toEqual(before);

    // 查询 "api" 命中两个目录的展示名，running 只留下 work-api 的 running 会话：
    // 同一张卡片里 waiting_permission / idle 的行必须被隐藏。
    const html = renderDirectoryList(store.directoryPage(), {
      sessionsByDirectoryId: store.directoryPageSessions(),
      query: "api",
      filter: "running",
    });
    expect(html).toContain('data-session-id="aaaaaaaa-0000-4000-8000-000000000001"');
    expect(html).not.toContain('data-session-id="aaaaaaaa-0000-4000-8000-000000000002"');

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
    // 与两种空态断言互相污染——本条就是防这个的。
    expect(html).not.toContain("data-empty-state");
    // 反例：若两种空态的词表被顺手复用，这里会出现它们的标题。
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.no_directories.title);
    expect(html).not.toContain(DIRECTORY_EMPTY_COPY.directories_without_sessions.title);
    // 「搜索无结果」时必须留下工具条：否则用户没有把关键词/筛选改回去的入口。
    expect(html).toContain('data-directory-search="true"');
  });

  it("会话数超过内联展示上限：出现「查看全部 ›」入口（文案不带 N）", () => {
    const many = Array.from({ length: 7 }, (_, index) =>
      session({
        id: `bbbbbbbb-0000-4000-8000-00000000000${String(index)}`,
        title: `会话 ${String(index)}`,
      }),
    );
    const withMany = { resources: resources({ sessions: many, workspaces: WORKSPACES }) };
    const model = buildDirectoryPageModel({ ...withMany, connection: "online", lastSyncedAtMs: null });
    const html = renderDirectoryList(model, {
      sessionsByDirectoryId: storeWith(withMany).store.directoryPageSessions(),
    });

    expect(html).toContain("查看全部 ›");
    // 反例：文案若退回带 N 的形态（原型这一轮去掉了 N），这两条之一会变红。
    expect(html).not.toContain("查看全部 7");
    expect(html).not.toContain("查看全部 7 个");
    // 反例：若内联不限条数地全列出来，这里会出现 7 条会话行。
    expect(html.match(/data-session-id="/g)).toHaveLength(5);
    // 超过上限时必须有进入详情页的入口，否则「更早的会话」无处可看；这里断言的是**查看全部**
    // 这一个取值（`open-directory` 归展示名入口，两者必须可区分）。
    expect(html).toContain('data-action="open-directory-all"');
    // 原型 `.footline`：卡体底部再给一次同样的入口，使用独立动作取值以免两处被数成一个控件。
    expect(html).toContain('data-action="open-directory-all-footer"');
  });

  it("对照：0 会话的目录项不呈现「查看全部」，但仍有展示名入口（与会话数无关）", () => {
    // 只给 `work-api` 一条会话：`personal-api` 的会话数是 0（列表仍非空 → 仍是 ready）。
    const onlyWorkApi = [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" })];
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: onlyWorkApi, workspaces: WORKSPACES }),
      connection: "online",
      lastSyncedAtMs: null,
    });

    const html = renderDirectoryList(model);

    // 反例：若无条件渲染入口，两个目录项会各出一个「查看全部 ›」。
    expect(html.match(/data-action="open-directory-all"/g)).toHaveLength(1);
    expect(html).not.toContain("查看全部 0");
    // 另一条路径任何时候都在（每项一处）：0 会话的 `personal-api` 靠它进详情页。
    expect(html.match(/data-action="open-directory"/g)).toHaveLength(2);

    const cards = cardSegments(html);
    expect(cards).toHaveLength(2);
    for (const card of cards) {
      // 每个目录项**恰好一处**展示名入口；`data-directory-id` 也只在这里的根节点出现一次。
      expect(card.match(/data-action="open-directory"/g)).toHaveLength(1);
      expect(card.match(/data-directory-id="/g)).toHaveLength(1);
    }
    // 反例：若展示名入口被实现成「有会话才可点」，0 会话那一项就没有入口，断言变红。
    const noSessions = cards.filter((card) => !card.startsWith('data-directory-id="work-api"'));
    expect(noSessions).toHaveLength(1);
    expect(noSessions[0]).toContain('data-action="open-directory"');
    expect(noSessions[0]).not.toContain("open-directory-all");
    // 0 会话的卡体是那一行说明，而不是空白（原型 `.emptyline`）。
    expect(noSessions[0]).toContain("这个目录还没有会话");
  });

  it("创建入口恒可用：每个目录项都呈现「⊕ 新建」，且不出现任何权限文案", () => {
    // 设备不再有创建会话的权限维度：`auth.authenticated` 根本没有授权集合字段，
    // 入口因此不依赖任何 scope 就无条件呈现。
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const html = renderDirectoryList(store.directoryPage());
    const cards = cardSegments(html);
    expect(cards).toHaveLength(2);
    for (const card of cards) {
      // 反例：入口若因权限被摘掉，这里每张卡片都会缺 `create-session` → 断言变红。
      expect(card).toContain('data-action="create-session"');
    }
    // 反例：权限门控若残留，页面上会出现提示条或那句「需要电脑端授予」。
    expect(html).not.toContain("data-create-permission-note");
    expect(html).not.toContain("需要电脑端授予");
    expect(html).not.toContain("没有创建会话的权限");
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
    expect(withClock).not.toContain("已同步");

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

describe("R15 目录页工具条右端的顶部创建入口", () => {
  const ready = buildDirectoryPageModel({
    resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
    connection: "online",
    lastSyncedAtMs: null,
  });

  it("画在工具条右端：搜索框与筛选段之后、目录列表之前，且只在给了回调时呈现", () => {
    const html = renderDirectoryList(ready, { onCreateSessionToBrowse: () => undefined });

    const entry = html.indexOf('data-action="create-session-browse"');
    expect(entry).toBeGreaterThan(html.indexOf('data-directory-search="true"'));
    expect(entry).toBeGreaterThan(html.indexOf('data-segment="筛选"'));
    expect(entry).toBeLessThan(html.indexOf('data-directory-list="directories"'));
    expect(html).toContain("⊕ 新建");
    // 与卡片上的目录内入口是两个不同的锚点：这条在整个页面里只出现一次（就在工具条里）。
    expect(html.match(/data-action="create-session-browse"/g)).toHaveLength(1);
    expect(html.match(/data-action="create-session"/g)).toHaveLength(2);
    // 反例：没有回调还把它画出来（点了没反应）→ 断言变红。
    expect(renderDirectoryList(ready)).not.toContain('data-action="create-session-browse"');
  });

  it("空态与加载中都不消失：那时它是目录页上唯一的创建路径", () => {
    const emptyModel = buildDirectoryPageModel({
      resources: resources({ sessions: [], workspaces: [] }),
      connection: "online",
      lastSyncedAtMs: null,
    });
    expect(emptyModel.kind).toBe("empty");

    const empty = renderDirectoryList(emptyModel, { onCreateSessionToBrowse: () => undefined });
    expect(empty).toContain('data-empty-state="no_directories"');
    expect(empty).toContain('data-action="create-session-browse"');
    // 工具条在所有状态都保留搜索、筛选与右上角创建入口；空态块里另有常规尺寸的同一入口。
    expect(empty).toContain('data-directory-search="true"');
    expect(empty).toContain('data-segment="筛选"');
    expect(empty).toContain('data-directory-toolbar="true"');

    // 从未同步过：还没拿到快照，但入口照旧在（它是页面上的创建路径）。
    const loading = renderDirectoryList(
      buildDirectoryPageModel({ resources: null, connection: "online", lastSyncedAtMs: null }),
      { onCreateSessionToBrowse: () => undefined },
    );
    expect(loading).toContain('data-directory-state="loading"');
    expect(loading).toContain('data-action="create-session-browse"');
  });
});

describe("R15 目录项：不再呈现最近活动时间 + 目录内创建入口", () => {
  it("目录项不呈现任何活动时间（该需求已随原型撤销）", () => {
    // 走与目录页同一条取数路径：模型与会话行都经 `ClientStore` 取（内联会话行来自
    // `directoryPageSessions()`，而不是页模型本身），否则「对照」那句会因为没有会话行而假红。
    const { store } = storeWith({
      resources: resources({
        sessions: [session({ id: "cccccccc-0000-4000-8000-000000000005", updatedAt: "2026-10-02T08:00:00.000Z" })],
        workspaces: WORKSPACES,
      }),
    });

    const html = renderDirectoryList(store.directoryPage(), {
      sessionsByDirectoryId: store.directoryPageSessions(),
    });

    // 反例：把目录行的活动时间加回来（属性或「最近 …」文案）都会在这里红。
    expect(html).not.toContain("data-directory-latest-at");
    expect(html).not.toContain("最近");
    // 对照：那一项仍然渲染出会话行与会话数——不是「谁都没渲染」。
    expect(html).toContain('data-directory-id="work-api"');
    expect(html).toContain('data-session-id="cccccccc-0000-4000-8000-000000000005"');
  });

  it("每个目录项都呈现一处目录内创建入口（必填 props 后不再有「不给回调」的分支）", () => {
    const model = buildDirectoryPageModel({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }),
      connection: "online",
      lastSyncedAtMs: null,
    });

    const granted = renderDirectoryList(model);
    expect(granted.match(/data-action="create-session"/g)).toHaveLength(2);
    expect(granted).toContain("⊕ 新建");
    // 新入口不得把 `data-directory-id` 再挂一份（R15 数它的个数）。
    expect(granted.match(/data-directory-id="/g)).toHaveLength(2);
    // 每个目录项各一处：入口是卡片级的，不会因为内联会话行多出几份。
    for (const card of cardSegments(granted)) {
      expect(card.match(/data-action="create-session"/g)).toHaveLength(1);
    }
  });
});

describe("R15 目录详情：链接失效与「目录存在但无会话」是两种呈现", () => {
  it("目录存在但没有任何会话：渲染「还没有会话」，不渲染「链接已失效」的告警", () => {
    // 生产路径：模型经 ClientStore 取出（与页面同一路径），不直调构建函数。
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("work-api"));

    expect(html).toContain('data-directory-sessions="empty"');
    expect(html).toContain("这个目录还没有会话");
    // 反例：若详情模型把「有目录但无会话」当成目录标识失效，这里渲染的是告警，断言变红。
    expect(html).not.toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain("可能已被删除");
  });

  it("对照：目录标识不在快照里才是「链接已失效」——两种呈现互不冒充", () => {
    const { store } = storeWith({ resources: resources({ sessions: [], workspaces: WORKSPACES }) });
    const html = renderDirectoryDetail(store.directoryDetail("not-there"));

    expect(html).toContain('data-directory-detail-state="unknown_directory"');
    expect(html).not.toContain('data-directory-sessions="empty"');
  });
});
