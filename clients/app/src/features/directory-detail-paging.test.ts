/**
 * 目录详情分页（规格「目录详情按时间倒序分页」，design D4）。
 *
 * 判定全在 feature 层的纯函数里，因此这里直调 `buildDirectoryDetailModel` / `correctPageNumber`，
 * 再用 `ClientStore` 走一遍「提交页码 → 列表不变」的生产路径（组件的渲染断言在 WP4 的文件里）。
 *
 * 判别力来源：
 *
 * - 切片：夹具的 25 条会话每条相隔一分钟，倒序即编号顺序；第一页取 11–20 条、第二页取 21–25 条
 *   时各页的 id 集合互不相同（改错 offset 或改错方向都会撞上别的页）；
 * - 夹紧：页码 `99` 与 `0` 分别落在上界与下界，若实现只是「不合法就当第一页」，`99` 那条会红；
 * - 非数字回落：经 store 提交 `abc`（当时在第 2 页）后页码仍是 2、切片逐字不变——只有「回落
 *   到当前页」的实现能同时满足这两条；把非数字一律当第一页会让切片变成第一页。
 */

import { describe, expect, it } from "vitest";

import type { SessionSummary, WorkspaceCatalogEntry } from "../protocol";
import type { DirectoryDetailModel } from "./directory-model";
import { DIRECTORY_DETAIL_PAGE_SIZE, buildDirectoryDetailModel, correctPageNumber } from "./directory-model";
import { WORKSPACES, resources, session, storeWith } from "./testing/fixtures";

/** 注入的固定时钟（相对时间与分页无关，但模型要求它作为输入）。 */
const NOW_MS = Date.parse("2026-10-02T12:00:00.000Z");

/** 25 条会话，每条相隔一分钟：倒序即编号顺序，因此各页的 id 是可预期的。 */
function manySessions(count: number): readonly SessionSummary[] {
  const base = Date.parse("2026-10-02T12:00:00.000Z");
  return Array.from({ length: count }, (_, index) =>
    session({
      id: `bbbbbbbb-0000-4000-8000-${(index + 1).toString().padStart(12, "0")}`,
      title: `会话 ${index + 1}`,
      updatedAt: new Date(base - index * 60_000).toISOString(),
    }),
  );
}

function ready(model: DirectoryDetailModel): Extract<DirectoryDetailModel, { kind: "ready" }> {
  if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);
  return model;
}

function detail(input: {
  readonly sessions: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceCatalogEntry[];
  readonly directoryId?: string;
  readonly pageInput?: string;
}): DirectoryDetailModel {
  return buildDirectoryDetailModel({
    resources: resources({ sessions: input.sessions, workspaces: input.workspaces ?? WORKSPACES }),
    connection: "online",
    directoryId: input.directoryId ?? "work-api",
    ownerLabelOf: () => "工作站",
    nowMs: NOW_MS,
    ...(input.pageInput === undefined ? {} : { pageInput: input.pageInput }),
  });
}

/** 一页里的会话编号（`会话 N` → N）。 */
function numbersOf(model: Extract<DirectoryDetailModel, { kind: "ready" }>): readonly number[] {
  return model.sessions.map((row) => Number.parseInt(row.title.replace("会话 ", ""), 10));
}

describe("页码纠正（纯函数）", () => {
  it("能解析成正整数时夹紧到 [1, 总页数]", () => {
    expect(correctPageNumber({ requested: "2", pageCount: 3, fallback: 2 })).toBe(2);
    // 越界 → 边界而不是回落：规格说「回填为当前合法页码**或范围边界**」。
    expect(correctPageNumber({ requested: "99", pageCount: 3, fallback: 2 })).toBe(3);
    expect(correctPageNumber({ requested: "0", pageCount: 3, fallback: 2 })).toBe(1);
  });

  it("解析不出时回落到调用方给的当前页（不是硬编码的第一页）", () => {
    for (const requested of ["", "  ", "abc", "第2页", "2 页", "-1", "1.5", "٣"]) {
      expect(correctPageNumber({ requested, pageCount: 3, fallback: 2 })).toBe(2);
    }
  });

  it("首尾空白被接受，超大数字落在上界", () => {
    expect(correctPageNumber({ requested: " 2 ", pageCount: 3, fallback: 1 })).toBe(2);
    expect(correctPageNumber({ requested: "99999999999999999999", pageCount: 3, fallback: 1 })).toBe(3);
  });

  it("总页数恒 ≥ 1：没有内容时任何输入都落在第 1 页", () => {
    expect(correctPageNumber({ requested: "5", pageCount: 0, fallback: 1 })).toBe(1);
  });
});

describe("目录详情分页：每页条数、总页数与切片", () => {
  it("按时间倒序平铺，第一页是最新的 10 条", () => {
    const model = ready(detail({ sessions: manySessions(25) }));

    expect(model.pageSize).toBe(DIRECTORY_DETAIL_PAGE_SIZE);
    expect(model.total).toBe(25);
    expect(model.pageCount).toBe(3);
    expect(model.page).toBe(1);
    expect(model.shown).toBe(DIRECTORY_DETAIL_PAGE_SIZE);
    expect(numbersOf(model)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  });

  it("请求第 3 页只给该页的 5 条（最后一页不满也照常）", () => {
    const model = ready(detail({ sessions: manySessions(25), pageInput: "3" }));

    expect(model.page).toBe(3);
    expect(numbersOf(model)).toEqual([21, 22, 23, 24, 25]);
    // 页码与切片自洽：`shown` 是本页长度，`total` 是分页前的行数。
    expect(model.shown).toBe(model.sessions.length);
    expect(model.total).toBe(25);
  });

  it("各页拼起来恰好是过滤后的全部行，不重不漏", () => {
    const sessions = manySessions(25);
    const seen = [1, 2, 3].flatMap((page) => numbersOf(ready(detail({ sessions, pageInput: String(page) }))));

    expect(seen).toEqual(manySessions(25).map((_, index) => index + 1));
    expect(new Set(seen).size).toBe(25);
  });

  it("只有一页时总页数为 1（组件固定渲染翻页，但两个按钮禁用）", () => {
    const model = ready(detail({ sessions: manySessions(3) }));

    expect(model.pageCount).toBe(1);
    expect(model.total).toBe(3);
    expect(model.shown).toBe(3);
    // 越界输入也被夹到那唯一一页上。
    expect(ready(detail({ sessions: manySessions(3), pageInput: "5" })).page).toBe(1);
  });

  it("空目录：总页数 1、列表为空，页码不越界", () => {
    const model = ready(detail({ sessions: [], pageInput: "7" }));

    expect(model.total).toBe(0);
    expect(model.pageCount).toBe(1);
    expect(model.page).toBe(1);
    expect(model.sessions).toEqual([]);
  });
});

describe("非法页码的就地纠正（生产路径）", () => {
  it("非数字提交后页码仍是当前页，列表内容逐字不变", () => {
    const { store, gateway } = storeWith({ resources: resources({ sessions: manySessions(25) }) });
    store.setDirectoryDetailPage("work-api", "2");
    const before = ready(store.directoryDetail("work-api"));

    store.setDirectoryDetailPage("work-api", "abc");
    const after = ready(store.directoryDetail("work-api"));

    // 反例：把非数字一律当成第一页时，这里会变成 1 且切片换成第 1 页 → 断言变红。
    expect(after.page).toBe(2);
    expect(after.sessions.map((row) => row.id)).toEqual(before.sessions.map((row) => row.id));
    // 输入框回填的就是纠正后的值（存的是纠正后的字符串）。
    expect(store.state.directoryDetailPage).toBe("2");
    // 翻页、纠正都只是渲染状态，不派发任何命令。
    expect(gateway.dispatches).toEqual([]);
  });

  it("越界页码夹紧到边界，并回填边界值", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(25) }) });

    store.setDirectoryDetailPage("work-api", "99");
    const model = ready(store.directoryDetail("work-api"));

    expect(model.page).toBe(3);
    expect(store.state.directoryDetailPage).toBe("3");
    expect(numbersOf(model)).toEqual([21, 22, 23, 24, 25]);
  });

  it("要跨路由存活的页码只有一份：进入新目录时复位到第一页", () => {
    const { store } = storeWith({ resources: resources({ sessions: manySessions(25) }) });
    store.setDirectoryDetailPage("work-api", "3");
    expect(store.state.directoryDetailPage).toBe("3");
    expect(ready(store.directoryDetail("work-api")).page).toBe(3);

    store.resetDirectoryDetailView();

    // 反例：不复位页码时进入另一个目录会停在上一个目录的第 3 页（可能越界）。
    expect(store.state.directoryDetailPage).toBe("");
    expect(ready(store.directoryDetail("work-api")).page).toBe(1);
  });
});
