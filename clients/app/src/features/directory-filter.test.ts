/**
 * 目录页搜索/筛选与目录详情平铺列表的判定（design D3/D4/D5）。
 *
 * 渲染层拿不到事件（`vitest.config.ts` 只收 `*.test.ts`，组件测试用
 * `renderToStaticMarkup`），因此「输入查询词 → 列表变化」这类判定只能在**纯函数**这层证伪。
 * 本文件直接调用 `buildDirectoryPageModel` / `buildDirectoryDetailModel`。
 *
 * 判别力来源：
 *
 * - 命中口径：展示名与会话标题各给一个只有它能命中的查询词；目录标识给一个**只命中标识**的
 *   查询词，任何把 `directoryId` 纳入匹配的实现都会多留一项；
 * - 筛选：三个取值分别建了只有它能命中的目录，漏掉汇总口径会多留或多删目录；
 * - `no_match`：只在非默认查询/筛选下构造可达用例，并对照「缺省输入永不进入」。
 * - 最近活动时间：夹具里 `work-api` 的三条会话**最新的一条在中间**、`personal-api` 有一条**全局最晚**
 *   的会话——取第一条 / 取最后一条 / 取全局最大值 / 取当前时间的实现各会得到不同的值，只有
 *   「该目录下 `updatedAt` 的最大值」等于 `10-02`。
 * - 详情列表：跨月的会话必须落在**同一条**倒序平铺的列表里（规格用「目录详情按时间倒序分页」
 *   替换了旧的月份分档与目录内搜索），且只收录本目录的行——按月份分档、或按标题再过滤一次
 *   的实现都会在「行数与顺序」上露馅。
 */

import { describe, expect, it } from "vitest";

import { UNTITLED_SESSION_LABEL } from "../domain";
import type { SessionSummary, WorkspaceCatalogEntry } from "../protocol";
import type { DirectoryDetailModel, DirectoryFilter, DirectoryPageModel } from "./directory-model";
import {
  DIRECTORY_DETAIL_PAGE_SIZE,
  buildDirectoryDetailModel,
  buildDirectoryPageModel,
  buildSessionRowsByDirectoryId,
  filterSessionsForDirectory,
} from "./directory-model";
import { ACTIVITY_SESSIONS, CROSS_MONTH_SESSIONS, WORKSPACES, resources, session, storeWith } from "./testing/fixtures";

/**
 * 三个目录：路径尾段两两不同，展示名因此就是这三个名字。
 * 目录标识（`directoryId`）与展示名互不相交，便于把「命中来自哪一处」分离出来。
 */
const DIRS = [
  { directoryId: "core-eu", path: "D:\\work\\欧洲核心" },
  { directoryId: "web-ui", path: "D:\\work\\界面工程" },
  { directoryId: "lab", path: "/home/dev/实验台" },
] as const satisfies readonly WorkspaceCatalogEntry[];

const DIR_SESSIONS: readonly SessionSummary[] = [
  session({ id: "cccccccc-0000-4000-8000-000000000001", title: "订单重构", state: "running", directoryId: "core-eu" }),
  // 第二条给 core-eu 添上「运行中 2」，用来证明汇总数字不因筛选变小。
  session({ id: "cccccccc-0000-4000-8000-000000000002", title: "排队中", state: "queued", directoryId: "core-eu" }),
  session({
    id: "cccccccc-0000-4000-8000-000000000003",
    title: "Design Tokens",
    state: "waiting_permission",
    directoryId: "web-ui",
  }),
  session({ id: "cccccccc-0000-4000-8000-000000000004", title: null, state: "idle", directoryId: "lab" }),
];

/** 全部目录都只有 idle 会话：用来构造「筛选后一个都不命中」。 */
const QUIET_SESSIONS: readonly SessionSummary[] = [
  session({ id: "cccccccc-0000-4000-8000-00000000000a", title: "闲着", state: "idle", directoryId: "core-eu" }),
  session({ id: "cccccccc-0000-4000-8000-00000000000b", title: "也闲着", state: "idle", directoryId: "web-ui" }),
];

function page(input: {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceCatalogEntry[];
  readonly query?: string;
  readonly filter?: DirectoryFilter;
}): DirectoryPageModel {
  // 补成具体值再传：`exactOptionalPropertyTypes` 下不能把 `undefined` 显式赋给可选属性。
  return buildDirectoryPageModel({
    resources: resources({ sessions: input.sessions, workspaces: input.workspaces ?? WORKSPACES }),
    connection: "online",
    lastSyncedAtMs: 1_700_000_000_000,
    query: input.query ?? "",
    filter: input.filter ?? "all",
  });
}

/** 详情模型：注入固定时钟（相对时间与列表顺序无关，但模型要求它作为输入）。 */
function detail(input: {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceCatalogEntry[];
  readonly directoryId: string;
}): DirectoryDetailModel {
  return buildDirectoryDetailModel({
    resources: resources({ sessions: input.sessions, workspaces: input.workspaces ?? WORKSPACES }),
    connection: "online",
    directoryId: input.directoryId,
    ownerLabelOf: () => "工作站",
    nowMs: 1_700_000_000_000,
  });
}

/** 取 ready 目录页里留下的目录标识；非 ready 直接失败，避免断言在错类型上静默通过。 */
function directoryIdsOf(model: DirectoryPageModel): readonly string[] {
  if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
  return model.directories.map((directory) => directory.directoryId);
}

/** 取 ready 目录页里某个目录的最近活动时间；目录不在页面里时直接失败。 */
function activityOf(model: DirectoryPageModel, directoryId: string): string | null {
  if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
  const row = model.directories.find((directory) => directory.directoryId === directoryId);
  if (row === undefined) throw new Error(`目录页里没有 ${directoryId}`);
  return row.latestActivityAt;
}

describe("目录页搜索：展示名 / 会话标题，目录标识不参与", () => {
  it("命中展示名（路径尾段）", () => {
    // 反例：若仍按旧的服务端展示名匹配，这里会得到空集合。
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "欧洲" }))).toEqual(["core-eu"]);
  });

  it("目录标识本身不命中", () => {
    // 反例：把 `directoryId` 纳入匹配（旧目录别名分支的残影）时这里会留下这两项。
    expect(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "web-ui" }).kind).toBe("no_match");
    expect(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "core-eu" }).kind).toBe("no_match");
  });

  it("命中该目录下会话的标题", () => {
    // 反例：若只看目录的展示名，会话标题「订单重构」就查不到任何目录。
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "订单重构" }))).toEqual(["core-eu"]);
  });

  it("trim 首尾空白并忽略大小写", () => {
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "  DESIGN tokens " }))).toEqual(["web-ui"]);
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "欧洲核心" }))).toEqual(["core-eu"]);
  });

  it("空标题按行上呈现的「未命名会话」参与匹配", () => {
    // 匹配的就是看得见的文字：`lab` 的会话没有标题，行上写「未命名会话」。
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "未命名会话" }))).toEqual(["lab"]);
  });

  it("全空白的查询词等价于不过滤", () => {
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "   " }))).toEqual([
      "core-eu",
      "web-ui",
      "lab",
    ]);
  });
});

describe("目录页状态筛选", () => {
  it("all 不过滤", () => {
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "all" }))).toEqual([
      "core-eu",
      "web-ui",
      "lab",
    ]);
  });

  it("running 只留存在排队中或运行中会话的目录", () => {
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "running" }))).toEqual(["core-eu"]);
  });

  it("pending 只留存在等待输入或等待授权会话的目录", () => {
    expect(directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "pending" }))).toEqual(["web-ui"]);
  });

  it("汇总数字不因筛选改变", () => {
    const model = page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "running" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
    // core-eu 有两条会话（running + queued）：留下的项仍报自己的全部会话数，而不是命中数。
    // 反例：若把汇总改写成「命中数」，这里会变成 {total: 1, active: 1}。
    expect(model.directories[0]?.counts).toEqual({ total: 2, active: 2, pending: 0 });
  });

  it("查询与筛选可叠加", () => {
    expect(
      directoryIdsOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "核心", filter: "running" })),
    ).toEqual(["core-eu"]);
    // 叠加后无命中即 no_match——不是 ready。
    expect(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "核心", filter: "pending" }).kind).toBe("no_match");
  });
});

describe("目录页最近活动时间：该目录下 updatedAt 的最大值", () => {
  it("取最大值：既不是数组第一条，也不是最后一条，更不是当前时间", () => {
    const model = page({ sessions: ACTIVITY_SESSIONS });
    // `work-api` 的三条时间依次为 09-30（首）/ 10-02（中，最晚）/ 08-15（末）。
    // 反例：取第一条 → 09-30；取最后一条 → 08-15；取当前时间 → 运行当天。三者都不等于 10-02。
    expect(activityOf(model, "work-api")).toBe("2026-10-02T08:00:00.000Z");
    expect(activityOf(model, "work-api")).not.toBe(ACTIVITY_SESSIONS[0]?.updatedAt);
    expect(activityOf(model, "work-api")).not.toBe(ACTIVITY_SESSIONS[2]?.updatedAt);
    // 原值直出：模型给出会话自身的 `updatedAt`（不做格式化、也不做本地时区换算）。
    expect(activityOf(model, "work-api")).toBe(ACTIVITY_SESSIONS[1]?.updatedAt);
  });

  it("按目录各算各的：别的目录更晚的会话不算进来", () => {
    const model = page({ sessions: ACTIVITY_SESSIONS });
    // `personal-api` 的那条是**全局**最晚（12-31）；反例：按整份快照取最大值时，下面第一条会变成 12-31。
    expect(activityOf(model, "personal-api")).toBe("2026-12-31T23:59:59.000Z");
    expect(activityOf(model, "work-api")).not.toBe("2026-12-31T23:59:59.000Z");
  });

  it("该目录没有会话 → null（不呈现，也不用当前时间顶替）", () => {
    // `CROSS_MONTH_SESSIONS` 全在 `work-api` 下：`personal-api` 一条会话都没有。
    const model = page({ sessions: CROSS_MONTH_SESSIONS });
    expect(activityOf(model, "personal-api")).toBeNull();
    // 对照：有会话的目录仍给出时间，且取的是最晚的那条而不是第一条（数组里第一条更早）。
    expect(activityOf(model, "work-api")).toBe("2026-10-02T08:00:00.000Z");
    expect(activityOf(model, "work-api")).not.toBe(CROSS_MONTH_SESSIONS[0]?.updatedAt);
  });

  it("目录详情也带上该目录的最近活动时间（与列表页同一条投影）", () => {
    const model = detail({ sessions: ACTIVITY_SESSIONS, directoryId: "work-api" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
    expect(model.directory.latestActivityAt).toBe("2026-10-02T08:00:00.000Z");
  });
});

describe("目录页 no_match 分支", () => {
  it("非空查询无命中时出现，且带上来源信息", () => {
    const model = page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "查无此项" });
    expect(model).toMatchObject({ kind: "no_match", contentOrigin: "live", lastSyncedAtMs: 1_700_000_000_000 });
  });

  it("非默认筛选无命中时出现", () => {
    // 反例：若把筛选无结果并进 empty，这里会是 empty，断言变红。
    expect(page({ sessions: QUIET_SESSIONS, workspaces: DIRS, filter: "running" }).kind).toBe("no_match");
    expect(page({ sessions: QUIET_SESSIONS, workspaces: DIRS, filter: "pending" }).kind).toBe("no_match");
  });

  it("缺省查询与筛选下永远不会出现（缺省输入的命中就是全部目录）", () => {
    expect(page({ sessions: QUIET_SESSIONS, workspaces: DIRS }).kind).toBe("ready");
    expect(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "", filter: "all" }).kind).toBe("ready");
  });

  it("断连时照常给出同样的匹配结果，只改来源标注", () => {
    const offline = buildDirectoryPageModel({
      resources: resources({ sessions: DIR_SESSIONS, workspaces: DIRS }),
      connection: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
      query: "查无此项",
    });
    expect(offline).toMatchObject({ kind: "no_match", contentOrigin: "last_sync" });
  });

  it("不冒充两种既有空态：无快照仍是 loading，无目录/无会话仍是各自的 empty", () => {
    // 反例：若 no_match 抢占了这些情形，用户会把「还没来得及同步」看成「查不到」。
    expect(
      buildDirectoryPageModel({ resources: null, connection: "online", lastSyncedAtMs: null, query: "任意" }).kind,
    ).toBe("loading");
    expect(page({ sessions: [], workspaces: [], query: "任意" })).toMatchObject({
      kind: "empty",
      empty: "no_directories",
    });
    expect(page({ sessions: [], workspaces: WORKSPACES, query: "任意" })).toMatchObject({ kind: "no_match" });
  });

  it("没有会话时仍按目录展示名搜索，忽略大小写和首尾空格", () => {
    const result = page({ sessions: [], workspaces: DIRS, query: " 界面工程 " });
    expect(result).toMatchObject({ kind: "empty", empty: "directories_without_sessions" });
    if (result.kind !== "empty") throw new Error("应保留命中的空目录");
    expect(result.directories.map((directory) => directory.directoryId)).toEqual(["web-ui"]);
    expect(page({ sessions: [], workspaces: DIRS, query: "web-ui" }).kind).toBe("no_match");
  });

  it("没有会话时，运行中和待处理不保留任何目录；全部仍显示所有目录", () => {
    for (const filter of ["running", "pending"] as const) {
      expect(page({ sessions: [], workspaces: DIRS, filter }).kind).toBe("no_match");
      expect(page({ sessions: [], workspaces: DIRS, filter, query: "界面工程" }).kind).toBe("no_match");
    }
    const all = page({ sessions: [], workspaces: DIRS, query: "  " });
    expect(all.kind).toBe("empty");
    if (all.kind !== "empty") throw new Error("应保留全部空目录");
    expect(all.directories).toHaveLength(3);
  });
});

describe("目录详情：按时间倒序平铺（一条列表，没有月份档与目录内查询）", () => {
  it("跨月的会话落在同一条列表里，且按时间倒序", () => {
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, directoryId: "work-api" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    // 反例：按月份分档的实现会把两个月的会话切开（每档各只剩一条，甚至列表被空档挡空）。
    expect(model.sessions.map((row) => row.id)).toEqual([
      "eeeeeeee-0000-4000-8000-000000000002",
      "eeeeeeee-0000-4000-8000-000000000001",
    ]);
    expect(model.shown).toBe(2);
    expect(model.total).toBe(2);
    // 两条会话只有一页：条数、页码、总页数三者自洽。
    expect(model.page).toBe(1);
    expect(model.pageCount).toBe(1);
    expect(model.pageSize).toBe(DIRECTORY_DETAIL_PAGE_SIZE);
  });

  it("只列该目录下的会话：别的目录的会话既不进列表也不进点数", () => {
    // 反例：把整份快照的会话都列出来时，web-ui 会多出两张不属于它的行。
    const webUi = detail({ sessions: DIR_SESSIONS, workspaces: DIRS, directoryId: "web-ui" });
    if (webUi.kind !== "ready") throw new Error(`期望 ready，实际是 ${webUi.kind}`);
    expect(webUi.total).toBe(1);
    expect(webUi.sessions.map((row) => row.title)).toEqual(["Design Tokens"]);

    const coreEu = detail({ sessions: DIR_SESSIONS, workspaces: DIRS, directoryId: "core-eu" });
    if (coreEu.kind !== "ready") throw new Error(`期望 ready，实际是 ${coreEu.kind}`);
    expect(coreEu.total).toBe(2);
    expect(coreEu.sessions.map((row) => row.id)).toEqual([
      "cccccccc-0000-4000-8000-000000000001",
      "cccccccc-0000-4000-8000-000000000002",
    ]);
    expect(coreEu.sessions.some((row) => row.id === "cccccccc-0000-4000-8000-000000000003")).toBe(false);
  });

  it("空标题呈现为「未命名会话」（null 与空串都算空）", () => {
    const blank = detail({
      sessions: [
        session({ id: "cccccccc-0000-4000-8000-00000000001a", title: null, directoryId: "work-api" }),
        session({ id: "cccccccc-0000-4000-8000-00000000001b", title: "", directoryId: "work-api" }),
      ],
      directoryId: "work-api",
    });
    if (blank.kind !== "ready") throw new Error(`期望 ready，实际是 ${blank.kind}`);
    expect(blank.sessions.map((row) => row.title)).toEqual([UNTITLED_SESSION_LABEL, UNTITLED_SESSION_LABEL]);
  });

  it("对照：目录页自己的查询词与筛选不进入详情列表", () => {
    const { store } = storeWith({
      resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }),
    });
    const before = store.directoryDetail("work-api");

    // 目录页的两个字段都只属于目录页：这个查询词命中的是**别**的目录，筛选又会把 work-api 排除，
    // 于是目录页自己落到 no_match。
    store.setDirectoryQuery("personal");
    store.setDirectoryFilter("running");
    expect(store.directoryPage()).toMatchObject({ kind: "no_match" });

    // 反例：详情列表若借道目录页的查询/筛选，这里会少行甚至变空。
    expect(store.directoryDetail("work-api")).toEqual(before);
  });
});

describe("进入目录时复位详情页视图状态（行为对齐原型）", () => {
  it("resetDirectoryDetailView() 只把页码复位，且不碰目录页状态与连接/资源快照", () => {
    const { store } = storeWith({
      resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }),
    });
    store.setDirectoryQuery("欧洲核心");
    store.setDirectoryFilter("running");
    // 该夹具只有两条会话（一页）：越界的页码在提交时就被夹到 1，这里只关心复位把它清空。
    store.setDirectoryDetailPage("work-api", "2");
    const before = store.state;

    store.resetDirectoryDetailView();
    const after = store.state;

    // 页码是单份、跨路由存活的 UI 状态：不复位会把它带进下一个目录（可能直接越界）。
    expect(after.directoryDetailPage).toBe("");
    // 目录页的查询词与筛选是**另外两个**字段：复位详情页不得把它们也清掉。
    expect(after.directoryQuery).toBe("欧洲核心");
    expect(after.directoryFilter).toBe("running");
    // 引用级断言：这一方法只改那一处 UI 状态的字符串，连接机快照与资源快照必须是同一个对象。
    expect(after.connection).toBe(before.connection);
    expect(after.resources).toBe(before.resources);
  });

  it("复位后详情模型回到第一页并呈现该目录的全部会话", () => {
    const { store } = storeWith({
      resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }),
    });
    store.setDirectoryDetailPage("work-api", "2");

    store.resetDirectoryDetailView();
    const model = store.directoryDetail("work-api");
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(model.page).toBe(1);
    expect(model.total).toBe(2);
    expect(model.shown).toBe(2);
  });
});

describe("目录页卡内会话行：搜索与筛选只影响可见行", () => {
  const rowsOf = (directoryId: string) =>
    buildSessionRowsByDirectoryId({
      resources: resources({ sessions: DIR_SESSIONS, workspaces: DIRS }),
      ownerLabelOf: () => "工作站",
      nowMs: 1_700_000_000_000,
    }).get(directoryId) ?? [];

  it("状态筛选只保留对应状态，目录汇总数字不由它改变", () => {
    const rows = rowsOf("core-eu");
    // core-eu 有一条 running、一条 queued：两档都属于「运行中」的可见行。
    expect(
      filterSessionsForDirectory({
        rows,
        displayName: "欧洲核心",
        query: "",
        filter: "running",
      }).map((row) => row.id),
    ).toEqual(expect.arrayContaining([
      "cccccccc-0000-4000-8000-000000000001",
      "cccccccc-0000-4000-8000-000000000002",
    ]));
    // 没有等待中的会话：「待处理」筛选下这一目录的可见行为空（卡片本身由目录模型决定去留）。
    expect(
      filterSessionsForDirectory({
        rows,
        displayName: "欧洲核心",
        query: "",
        filter: "pending",
      }),
    ).toHaveLength(0);
  });

  it("查询命中展示名时保留（筛选后的）全部行；只命中标题时只留命中的行", () => {
    const rows = rowsOf("core-eu");
    expect(
      filterSessionsForDirectory({
        rows,
        displayName: "欧洲核心",
        query: "欧洲",
        filter: "all",
      }),
    ).toHaveLength(rows.length);
    expect(
      filterSessionsForDirectory({
        rows,
        displayName: "欧洲核心",
        query: "排队",
        filter: "all",
      }).map((row) => row.title),
    ).toEqual(["排队中"]);
  });

  it("目录标识不参与行匹配", () => {
    expect(
      filterSessionsForDirectory({
        rows: rowsOf("core-eu"),
        displayName: "欧洲核心",
        query: "core-eu",
        filter: "all",
      }),
    ).toHaveLength(0);
  });
});
