/**
 * 对话页左栏：同目录会话与目录上下文（规格「对话页左栏呈现同目录会话」）。
 *
 * 左栏的数据只从**快照资源**来（与目录页共用 `buildSessionRowsByDirectoryId` 这一条行投影），
 * 本文件据此断言两件事：
 *
 * 1. **只读快照、不派发新命令**：走完整条 `conversationPage()` 读取路径后 `dispatches` 为空；
 * 2. **两种「列不出来」可区分**：会话没有目录归属 → 说明；有目录但本机快照里没有同目录会话
 *    → 目录上下文照常呈现、列表为空，不编造也不冒充。
 *
 * 判别力来源：把左栏换成「列全部会话」的实现时「只列同目录」会红；把「没有目录归属」写成
 * 隐藏左栏（`directory` 与 `notice` 同时为 null/空）时那一条会红。
 */

import { describe, expect, it } from "vitest";

import type { SessionSummary, WorkspaceCatalogEntry } from "../protocol";
import {
  CONVERSATION_NO_DIRECTORY_NOTICE,
  CONVERSATION_SIDEBAR_PAGE_SIZE,
  CONVERSATION_UNKNOWN_DIRECTORY_NOTICE,
} from "./conversation-model";
import {
  ONLINE_SESSION_ID,
  WORKSPACES,
  WORKSPACES_WITH_PATHS,
  resources,
  session,
  storeWith,
} from "./testing/fixtures";

/** 同目录的两个会话 + 另一个目录的一个会话（用来证明「只列同目录」）。 */
const OTHER_DIR_SESSION_ID = "dddddddd-0000-4000-8000-0000000000b2";
const SIBLING_SESSION_ID = "dddddddd-0000-4000-8000-0000000000b3";

const SESSIONS: readonly SessionSummary[] = [
  session({ id: ONLINE_SESSION_ID, title: "当前这条", state: "running", directoryId: "work-api" }),
  session({ id: SIBLING_SESSION_ID, title: "同目录的另一条", state: "idle", directoryId: "work-api" }),
  session({ id: OTHER_DIR_SESSION_ID, title: "别的目录的一条", state: "idle", directoryId: "personal-api" }),
];

describe("左栏只列同目录会话", () => {
  it("按最近会话更新时间倒序排列，而不是按输入顺序或标题排列", () => {
    const { store } = storeWith({ resources: resources({ workspaces: WORKSPACES, sessions: [
      session({ id: ONLINE_SESSION_ID, title: "A", directoryId: "work-api", updatedAt: "2026-10-01T00:00:00.000Z" }),
      session({ id: SIBLING_SESSION_ID, title: "Z", directoryId: "work-api", updatedAt: "2026-10-02T00:00:00.000Z" }),
    ] }) });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.sidebar.rows.map((row) => row.id))
      .toEqual([SIBLING_SESSION_ID, ONLINE_SESSION_ID]);
  });
  it("列出同目录会话（时间倒序），当前会话在模型里被标记为选中", () => {
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const sidebar = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (sidebar === undefined) throw new Error("会话应在模型里");

    expect(sidebar.directory?.directoryId).toBe("work-api");
    expect(sidebar.rows.map((row) => row.id).sort()).toEqual([SIBLING_SESSION_ID, ONLINE_SESSION_ID].sort());
    // 反例：把别的目录的会话也列出来时这里是三行 → 断言变红。
    expect(sidebar.rows.some((row) => row.id === OTHER_DIR_SESSION_ID)).toBe(false);
    expect(sidebar.selectedSessionId).toBe(ONLINE_SESSION_ID);
    // 行上带状态与相对时间（组件据此画圆点与时间）。
    expect(sidebar.rows.map((row) => [row.title, row.state])).toContainEqual(["当前这条", "running"]);
    expect(sidebar.rows.every((row) => typeof row.updatedAtLabel === "string")).toBe(true);
    expect(sidebar.notice).toBeNull();
  });

  it("目录上下文给展示名与**逐字**路径（快照下发的那个字符串）", () => {
    const { store } = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
    });
    const sidebar = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (sidebar === undefined) throw new Error("会话应在模型里");

    expect(sidebar.directory).toEqual({
      directoryId: "work-api",
      displayName: "api",
      path: "D:\\work\\api",
    });
  });

  it("路径始终在场（逐字来自快照条目，不拿展示名拼一段）", () => {
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const sidebar = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (sidebar === undefined) throw new Error("会话应在模型里");

    expect(sidebar.directory?.path).toBe("D:\\work\\api");
  });

  it("目录不在最近一次快照里：不呈现目录上下文，列表为空且说明原因", () => {
    const onlyWorkApi = [
      { directoryId: "work-api", path: "D:\\work\\api" },
    ] as const satisfies readonly WorkspaceCatalogEntry[];
    const { store } = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: onlyWorkApi }),
    });
    const sidebar = store.conversationPage(OTHER_DIR_SESSION_ID)?.sidebar;
    if (sidebar === undefined) throw new Error("会话应在模型里");

    // 反例：退回目录标识（或编一段展示名）承担界面文本时这里会非 null → 断言变红。
    expect(sidebar.directory).toBeNull();
    expect(sidebar.notice).toBe(CONVERSATION_UNKNOWN_DIRECTORY_NOTICE);
    expect(sidebar.rows).toEqual([]);
    expect(sidebar.total).toBe(0);
    expect(sidebar.canNextPage).toBe(false);
  });
});

describe("没有目录归属的会话", () => {
  it("说明它没有目录归属，而不是隐藏左栏；中栏照常", () => {
    const homeless: readonly SessionSummary[] = [session({ id: ONLINE_SESSION_ID, title: "没有目录", directoryId: null })];
    const { store } = storeWith({ resources: resources({ sessions: homeless, workspaces: WORKSPACES }) });
    const page = store.conversationPage(ONLINE_SESSION_ID);
    if (page === null) throw new Error("会话应在模型里");

    // 反例：把左栏静默藏起来（或填一个空目录上下文）时这里会是 null / 空文案 → 断言变红。
    expect(page.sidebar.directory).toBeNull();
    expect(page.sidebar.notice).toBe(CONVERSATION_NO_DIRECTORY_NOTICE);
    expect(page.sidebar.rows).toEqual([]);
    expect(page.sidebar.canNextPage).toBe(false);
    expect(page.sidebar.selectedSessionId).toBe(ONLINE_SESSION_ID);

    // 中栏不受影响：会话头与时间线照常给出。
    expect(page.header.sessionId).toBe(ONLINE_SESSION_ID);
    expect(page.header.title).toBe("没有目录");
    expect(page.timeline.entries).toEqual([]);
  });
});

describe("已呈现数与总数、继续加载", () => {
  /** 同一目录 20 条会话（超过一批）；第 0 条就是当前会话。 */
  function manySessions(count: number): readonly SessionSummary[] {
    const base = Date.parse("2026-10-02T12:00:00.000Z");
    return Array.from({ length: count }, (_, index) =>
      session({
        id:
          index === 0
            ? ONLINE_SESSION_ID
            : `eeeeeeee-0000-4000-8000-${(index + 1).toString().padStart(12, "0")}`,
        title: `会话 ${index + 1}`,
        directoryId: "work-api",
        updatedAt: new Date(base - index * 60_000).toISOString(),
      }),
    );
  }

  it("超过一批时给出「已呈现 N / M」并可继续加载（不丢会话）", () => {
    const sessions = manySessions(20);
    const { store, gateway } = storeWith({ resources: resources({ sessions, workspaces: WORKSPACES }) });
    const first = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (first === undefined) throw new Error("会话应在模型里");

    expect(first.total).toBe(20);
    expect(first.shown).toBe(CONVERSATION_SIDEBAR_PAGE_SIZE);
    expect(first.rows).toHaveLength(CONVERSATION_SIDEBAR_PAGE_SIZE);
    expect(first.canNextPage).toBe(true);
    // 反例：把「已呈现」当成全部时，用户会以为这个目录只有 15 条会话。
    expect(first.shown).toBeLessThan(first.total);

    store.setConversationSidebarPage("2");
    const second = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (second === undefined) throw new Error("会话应在模型里");

    expect(second.shown).toBe(5);
    expect(second.page).toBe(2);
    expect(second.pageCount).toBe(2);
    expect(second.rows.map((row) => row.id)).not.toContain(first.rows[0]?.id);
    store.setConversationSidebarPage("1");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.sidebar.rows).toEqual(first.rows);
    store.setConversationSidebarPage("99");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.sidebar.page).toBe(2);
    store.setConversationSidebarQuery("会话 1");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.sidebar.page).toBe(1);
    expect(second.total).toBe(20);
    expect(second.canNextPage).toBe(false);
    // 继续加载只是呈现更多行，不派发任何命令。
    expect(gateway.dispatches).toEqual([]);
  });

  it("不足一批时没有「继续加载」入口", () => {
    const { store } = storeWith({
      resources: resources({ sessions: manySessions(3), workspaces: WORKSPACES }),
    });
    const sidebar = store.conversationPage(ONLINE_SESSION_ID)?.sidebar;
    if (sidebar === undefined) throw new Error("会话应在模型里");

    expect(sidebar.shown).toBe(3);
    expect(sidebar.total).toBe(3);
    expect(sidebar.canNextPage).toBe(false);
  });
});

describe("左栏搜索只影响左栏", () => {
  it("搜索词过滤左栏列表，中栏当前会话与时间线不变", () => {
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    const before = store.conversationPage(ONLINE_SESSION_ID);
    if (before === null) throw new Error("会话应在模型里");

    store.setConversationSidebarQuery("同目录的另一条");
    const after = store.conversationPage(ONLINE_SESSION_ID);
    if (after === null) throw new Error("会话应在模型里");

    expect(after.sidebar.query).toBe("同目录的另一条");
    expect(after.sidebar.rows.map((row) => row.id)).toEqual([SIBLING_SESSION_ID]);
    expect(after.sidebar.total).toBe(1);
    // 反例：把左栏搜索接到中栏上时，这里的中栏内容会跟着变。
    expect(after.header).toEqual(before.header);
    expect(after.messages).toEqual(before.messages);
    expect(after.timeline).toEqual(before.timeline);
    expect(after.sidebar.selectedSessionId).toBe(ONLINE_SESSION_ID);
  });

  it("搜索词与目录页的查询词、详情页的页码互不影响", () => {
    const { store } = storeWith({ resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES }) });
    store.setDirectoryQuery("别的目录检索词");
    store.setDirectoryDetailPage("work-api", "1");

    store.setConversationSidebarQuery("左栏检索词");

    expect(store.state.directoryQuery).toBe("别的目录检索词");
    expect(store.state.directoryDetailPage).toBe("1");
  });
});

describe("左栏数据只来自快照", () => {
  it("读取对话页模型（含左栏）不派发任何命令", () => {
    const { store, gateway } = storeWith({
      resources: resources({ sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS }),
    });

    const page = store.conversationPage(ONLINE_SESSION_ID);
    store.setConversationSidebarQuery("会话");
    store.setConversationSidebarPage("2");
    store.conversationPage(ONLINE_SESSION_ID);

    // 反例：左栏若改走 `session.list` 之类，这里会出现派发记录。
    expect(gateway.dispatches).toEqual([]);
    expect(page?.sidebar.rows.length).toBeGreaterThan(0);
  });
});
