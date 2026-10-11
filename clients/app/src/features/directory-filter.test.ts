/**
 * 目录页搜索/筛选与目录详情按月分档的判定（design D3/D4/D5/D6）。
 *
 * 渲染层拿不到事件（`vitest.config.ts` 只收 `*.test.ts`，组件测试用
 * `renderToStaticMarkup`），因此「输入查询词 → 列表变化」这类判定只能在**纯函数**这层证伪。
 * 本文件直接调用 `buildDirectoryPageModel` / `buildDirectoryDetailModel`。
 *
 * 判别力来源：
 *
 * - 命中口径：三种命中方式各给一个只有它能命中的查询词，任何漏掉一种匹配的实现都会少一项；
 * - 筛选：三个取值分别建了只有它能命中的目录，漏掉汇总口径会多留或多删目录；
 * - 分档：用了 2026-09 / 2026-10 两个月份与一条 2020 年的会话——若实现改用「当前时间」，
 *   三个月份会并成一档，`months` 与「今天所在的档」两条断言都会变红；
 * - `no_match`：只在非默认查询/筛选下构造可达用例，并对照「缺省输入永不进入」。
 * - 最近活动时间：夹具里 `work-api` 的三条会话**最新的一条在中间**、`personal-api` 有一条**全局最晚**
 *   的会话——取第一条 / 取最后一条 / 取全局最大值 / 取当前时间的实现各会得到不同的值，只有
 *   「该目录下 `updatedAt` 的最大值」等于 `10-02`。
 * - 月份档对账：构造「请求的档已不在派生档集合里」的可达用例（目录内查询收缩档集合、
 *   跨目录残留上一个目录的档），断言生效月份回落到能被控件表示的 `"all"`——没有对账时
 *   这些用例会得到空白列表 + 无选中控件，正是本文件要挡住的错态。
 */

import { describe, expect, it } from "vitest";

import { UNTITLED_SESSION_LABEL } from "../domain";
import type { SessionSummary, WorkspaceRef } from "../protocol";
import type { DirectoryDetailModel, DirectoryFilter, DirectoryPageModel } from "./directory-model";
import { buildDirectoryDetailModel, buildDirectoryPageModel, monthKeyOf } from "./directory-model";
import { ACTIVITY_SESSIONS, CROSS_MONTH_SESSIONS, WORKSPACES, resources, session, storeWith } from "./testing/fixtures";

/** 三个目录，展示名、别名、名下会话标题两两不相交，便于把「命中来自哪一处」分离出来。 */
const DIRS = [
  { alias: "core-eu", displayName: "欧洲核心" },
  { alias: "web-ui", displayName: "界面工程" },
  { alias: "lab", displayName: "实验台" },
] as const satisfies readonly WorkspaceRef[];

const DIR_SESSIONS: readonly SessionSummary[] = [
  session({ id: "cccccccc-0000-4000-8000-000000000001", title: "订单重构", state: "running", alias: "core-eu" }),
  // 第二条给 core-eu 添上「运行中 2」，用来证明汇总数字不因筛选变小。
  session({ id: "cccccccc-0000-4000-8000-000000000002", title: "排队中", state: "queued", alias: "core-eu" }),
  session({
    id: "cccccccc-0000-4000-8000-000000000003",
    title: "Design Tokens",
    state: "waiting_permission",
    alias: "web-ui",
  }),
  session({ id: "cccccccc-0000-4000-8000-000000000004", title: null, state: "idle", alias: "lab" }),
];

/** 全部目录都只有 idle 会话：用来构造「筛选后一个都不命中」。 */
const QUIET_SESSIONS: readonly SessionSummary[] = [
  session({ id: "cccccccc-0000-4000-8000-00000000000a", title: "闲着", state: "idle", alias: "core-eu" }),
  session({ id: "cccccccc-0000-4000-8000-00000000000b", title: "也闲着", state: "idle", alias: "web-ui" }),
];

function page(input: {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceRef[];
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

function detail(input: {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceRef[];
  readonly alias: string;
  readonly query?: string;
  readonly month?: string | "all";
}): DirectoryDetailModel {
  return buildDirectoryDetailModel({
    resources: resources({ sessions: input.sessions, workspaces: input.workspaces ?? WORKSPACES }),
    connection: "online",
    alias: input.alias,
    ownerLabelOf: () => "工作站",
    query: input.query ?? "",
    month: input.month ?? "all",
  });
}

/** 取 ready 目录页里留下的目录别名；非 ready 直接失败，避免断言在错类型上静默通过。 */
function aliasesOf(model: DirectoryPageModel): readonly string[] {
  if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
  return model.directories.map((directory) => directory.alias);
}

/** 取 ready 目录页里某个目录的最近活动时间；目录不在页面里时直接失败。 */
function activityOf(model: DirectoryPageModel, alias: string): string | null {
  if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
  const row = model.directories.find((directory) => directory.alias === alias);
  if (row === undefined) throw new Error(`目录页里没有 ${alias}`);
  return row.latestActivityAt;
}

describe("目录页搜索：展示名 / 别名 / 会话标题三种命中", () => {
  it("命中展示名", () => {
    // 反例：若只匹配别名，这里会得到空集合。
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "欧洲" }))).toEqual(["core-eu"]);
  });

  it("命中别名", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "web-ui" }))).toEqual(["web-ui"]);
  });

  it("命中该目录下会话的标题", () => {
    // 反例：若只看目录的展示名与别名，会话标题「订单重构」就查不到任何目录。
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "订单重构" }))).toEqual(["core-eu"]);
  });

  it("trim 首尾空白并忽略大小写", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "  DESIGN tokens " }))).toEqual(["web-ui"]);
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "CORE-EU" }))).toEqual(["core-eu"]);
  });

  it("空标题按行上呈现的「未命名会话」参与匹配", () => {
    // 匹配的就是看得见的文字：`lab` 的会话没有标题，行上写「未命名会话」。
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "未命名会话" }))).toEqual(["lab"]);
  });

  it("全空白的查询词等价于不过滤", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "   " }))).toEqual([
      "core-eu",
      "web-ui",
      "lab",
    ]);
  });
});

describe("目录页状态筛选", () => {
  it("all 不过滤", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "all" }))).toEqual([
      "core-eu",
      "web-ui",
      "lab",
    ]);
  });

  it("running 只留存在排队中或运行中会话的目录", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "running" }))).toEqual(["core-eu"]);
  });

  it("pending 只留存在等待输入或等待授权会话的目录", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "pending" }))).toEqual(["web-ui"]);
  });

  it("汇总数字不因筛选改变", () => {
    const model = page({ sessions: DIR_SESSIONS, workspaces: DIRS, filter: "running" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
    // core-eu 有两条会话（running + queued）：留下的项仍报自己的全部会话数，而不是命中数。
    // 反例：若把汇总改写成「命中数」，这里会变成 {total: 1, active: 1}。
    expect(model.directories[0]?.counts).toEqual({ total: 2, active: 2, pending: 0 });
  });

  it("查询与筛选可叠加", () => {
    expect(aliasesOf(page({ sessions: DIR_SESSIONS, workspaces: DIRS, query: "核心", filter: "running" }))).toEqual([
      "core-eu",
    ]);
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
    const model = detail({ sessions: ACTIVITY_SESSIONS, alias: "work-api" });
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
    expect(page({ sessions: [], workspaces: WORKSPACES, query: "任意" })).toMatchObject({
      kind: "empty",
      empty: "directories_without_sessions",
    });
  });
});

describe("目录详情按月分档", () => {
  it("两个不同月份各自成档，档按时间倒序，档内也倒序", () => {
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    // 反例：若按「当前时间」分档，两条会话会落进同一档，这里只剩一个元素。
    expect(model.months).toEqual([
      { key: "2026-10", count: 1 },
      { key: "2026-09", count: 1 },
    ]);
    expect(model.sessions.map((row) => row.id)).toEqual([
      "eeeeeeee-0000-4000-8000-000000000002",
      "eeeeeeee-0000-4000-8000-000000000001",
    ]);
    expect(model.shown).toBe(2);
    expect(model.total).toBe(2);
  });

  it("按会话自身时间而不是当前时间：2020 年的会话落在 2020-03，不在今天的档", () => {
    const longAgo = session({
      id: "eeeeeeee-0000-4000-8000-00000000000f",
      title: "老项目",
      updatedAt: "2020-03-15T12:00:00.000Z",
    });
    const model = detail({ sessions: [longAgo], alias: "work-api" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(monthKeyOf("2020-03-15T12:00:00.000Z")).toBe("2020-03");
    expect(model.months).toEqual([{ key: "2020-03", count: 1 }]);
    // 反例：若档取自当前时间，这里会出现今天所在的月份。
    expect(model.months.map((month) => month.key)).not.toContain(monthKeyOf(new Date().toISOString()));
  });

  it("切换到某个月份后 shown 变小而 total 与 months 不变", () => {
    const september = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api", month: "2026-09" });
    if (september.kind !== "ready") throw new Error(`期望 ready，实际是 ${september.kind}`);

    expect(september.sessions.map((row) => row.id)).toEqual(["eeeeeeee-0000-4000-8000-000000000001"]);
    expect(september.shown).toBe(1);
    // 分子随月份变，分母与月份档集合不随月份选择变。
    expect(september.total).toBe(2);
    expect(september.months).toHaveLength(2);
  });
});

describe("目录详情的目录内搜索", () => {
  it("只匹配本目录会话的标题", () => {
    const hit = detail({ sessions: DIR_SESSIONS, workspaces: DIRS, alias: "web-ui", query: "design" });
    if (hit.kind !== "ready") throw new Error(`期望 ready，实际是 ${hit.kind}`);
    expect(hit.total).toBe(1);
    expect(hit.sessions.map((row) => row.title)).toEqual(["Design Tokens"]);

    // core-eu 没有标题含 design 的会话：命中数归零，且不把其它目录的会话拉进来。
    const miss = detail({ sessions: DIR_SESSIONS, workspaces: DIRS, alias: "core-eu", query: "design" });
    if (miss.kind !== "ready") throw new Error(`期望 ready，实际是 ${miss.kind}`);
    expect(miss.total).toBe(0);
    expect(miss.sessions).toEqual([]);
  });

  it("months 受查询影响：查询命中后档集合随之收缩", () => {
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api", query: "十月的发布" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
    expect(model.months).toEqual([{ key: "2026-10", count: 1 }]);
    expect(model.total).toBe(1);
    expect(model.shown).toBe(1);
  });

  it("空标题呈现为「未命名会话」（null 与空串都算空）", () => {
    const blank = detail({
      sessions: [
        session({ id: "cccccccc-0000-4000-8000-00000000001a", title: null, alias: "work-api" }),
        session({ id: "cccccccc-0000-4000-8000-00000000001b", title: "", alias: "work-api" }),
      ],
      alias: "work-api",
    });
    if (blank.kind !== "ready") throw new Error(`期望 ready，实际是 ${blank.kind}`);
    expect(blank.sessions.map((row) => row.title)).toEqual([UNTITLED_SESSION_LABEL, UNTITLED_SESSION_LABEL]);
  });
});

describe("目录详情：请求的月份档与派生档集合对账", () => {
  it("已选 2026-09、再用只命中 2026-10 的关键词搜索：档集合收缩，生效月份回落 all，列表不再被挡空", () => {
    // 可达路径 (a)：详情页已选 2026-09，随后输入的查询词只命中 2026-10 的会话——`months` 随之收缩，
    // 被选中的那个档从选项集合里消失。
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api", month: "2026-09", query: "十月的发布" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(model.months).toEqual([{ key: "2026-10", count: 1 }]);
    // 反例（修复前）：仍按请求的 "2026-09" 过滤，于是 sessions 为空、shown 为 0，而 months 里
    // 已经没有这个档——控件表示不了它，用户看到空白列表却看不到是哪个档在挡着。
    expect(model.month).toBe("all");
    expect(model.total).toBe(1);
    expect(model.shown).toBe(1);
    expect(model.sessions.map((row) => row.id)).toEqual(["eeeeeeee-0000-4000-8000-000000000002"]);
    // 三者自洽：生效值可被档集合表示（"all" 恒可）、shown 等于列表长度、各档 count 之和等于 total。
    expect(model.month === "all" || model.months.some((bucket) => bucket.key === model.month)).toBe(true);
    expect(model.shown).toBe(model.sessions.length);
    expect(model.months.reduce((sum, bucket) => sum + bucket.count, 0)).toBe(model.total);
  });

  it("跨目录残留的月份档：进入只含别的月份的目录时同样回落 all", () => {
    // 可达路径 (b)：在目录 A 选了 2026-09 后进入目录 B，而 B 只有 2026-10 的会话
    // （`directoryDetailMonth` 是单份 store 字段，跨路由存活；对账不依赖调用方是否复位）。
    const onlyOctober = [
      ...CROSS_MONTH_SESSIONS,
      session({
        id: "eeeeeeee-0000-4000-8000-000000000003",
        title: "另一目录的十月会话",
        alias: "personal-api",
        updatedAt: "2026-10-05T00:00:00.000Z",
      }),
    ];
    const model = detail({ sessions: onlyOctober, alias: "personal-api", month: "2026-09" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(model.months).toEqual([{ key: "2026-10", count: 1 }]);
    expect(model.month).toBe("all");
    expect(model.total).toBe(1);
    expect(model.shown).toBe(1);
    expect(model.sessions.map((row) => row.id)).toEqual(["eeeeeeee-0000-4000-8000-000000000003"]);
  });

  it("对照：请求的档仍在 months 里时保持原样——对账不会吞掉合法的月份选择", () => {
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api", month: "2026-09" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    // 反例：若对账被写成「一律回落 all」，这里会变成 all，断言变红。
    expect(model.month).toBe("2026-09");
    expect(model.shown).toBe(1);
    expect(model.total).toBe(2);
  });

  it("对照：缺省请求（未提供 month）下生效值就是 all", () => {
    const model = detail({ sessions: CROSS_MONTH_SESSIONS, alias: "work-api" });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
    expect(model.month).toBe("all");
  });
});

describe("进入目录时复位详情页视图状态（行为对齐原型）", () => {
  it("resetDirectoryDetailView() 清空目录内查询词与月份档，且不碰目录页状态与连接/资源快照", () => {
    const { store } = storeWith({
      resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }),
    });
    store.setDirectoryQuery("欧洲核心");
    store.setDirectoryFilter("running");
    store.setDirectoryDetailQuery("十月的发布");
    store.setDirectoryDetailMonth("2026-09");
    const before = store.state;

    store.resetDirectoryDetailView();
    const after = store.state;

    expect(after.directoryDetailQuery).toBe("");
    expect(after.directoryDetailMonth).toBe("all");
    // 目录页的查询词与筛选是**另外两个**字段：复位详情页不得把它们也清掉（两页互不影响）。
    expect(after.directoryQuery).toBe("欧洲核心");
    expect(after.directoryFilter).toBe("running");
    // 引用级断言：这一方法只改那两处 UI 状态，连接机快照与资源快照必须是同一个对象。
    expect(after.connection).toBe(before.connection);
    expect(after.resources).toBe(before.resources);
  });

  it("复位后详情模型回到「全部」档并呈现全部命中行", () => {
    const { store } = storeWith({
      resources: resources({ sessions: CROSS_MONTH_SESSIONS, workspaces: WORKSPACES }),
    });
    store.setDirectoryDetailMonth("2026-09");
    store.setDirectoryDetailQuery("十月的发布");

    store.resetDirectoryDetailView();
    const model = store.directoryDetail("work-api");
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(model.month).toBe("all");
    expect(model.total).toBe(2);
    expect(model.shown).toBe(2);
  });
});
