/**
 * 会话时间按相对口径呈现（规格「会话时间按相对口径呈现」，design D3）。
 *
 * 时钟是**注入的输入**（不 mock 全局 `Date`）：本文件直接给纯函数传 `nowMs`，并断言
 * 「同一份输入渲染同一段文本」——原来那条「不依赖运行时刻」的性质因此仍然成立。
 *
 * 判别力来源：
 *
 * - 三档：夹具里同一条会话在三个不同的 `nowMs` 下分别落在「刚刚」「N 分钟前」「年月日」，
 *   任何一档写错（例如把阈值写成 1 分钟/h 之外的数、或用本地时区换算日期）都会撞上；
 * - 同一份输入：连调两次必须逐字相同；只改 `nowMs` 时文案与时间差一致；
 * - 三页同源：目录页内联行、目录详情行与对话页左栏行对同一条会话给出同一段文案——
 *   三个页面各写一套口径时这条会红。
 */

import { describe, expect, it } from "vitest";

import type { SessionSummary } from "../protocol";
import { UNTITLED_SESSION_LABEL } from "../domain";
import { buildDirectoryDetailModel, buildSessionRowsByDirectoryId, formatRelativeTime } from "./directory-model";
import { ONLINE_SESSION_ID, WORKSPACES, resources, session, storeWith } from "./testing/fixtures";

/** 会话时刻与几个基准时刻：`NOW` 与它相差 30 秒 / 5 分钟 / 3 天。 */
const AT = "2026-10-02T12:00:00.000Z";
const AT_MS = Date.parse(AT);

describe("formatRelativeTime：三档口径", () => {
  it("不足一分钟是「刚刚」", () => {
    expect(formatRelativeTime(AT, AT_MS)).toBe("刚刚");
    expect(formatRelativeTime(AT, AT_MS + 59_000)).toBe("刚刚");
  });

  it("一小时内是「N 分钟前」，N 与时间差一致", () => {
    expect(formatRelativeTime(AT, AT_MS + 60_000)).toBe("1 分钟前");
    expect(formatRelativeTime(AT, AT_MS + 5 * 60_000)).toBe("5 分钟前");
    // 与原型同口径：分钟数四舍五入，1 分 40 秒给 2 分钟。
    expect(formatRelativeTime(AT, AT_MS + 100_000)).toBe("2 分钟前");
    expect(formatRelativeTime(AT, AT_MS + 59 * 60_000)).toBe("59 分钟前");
    // 尚未到一小时的分档里，四舍五入会把 59 分 30 秒写成 60 分钟（原型同样如此）。
    expect(formatRelativeTime(AT, AT_MS + 59.5 * 60_000)).toBe("60 分钟前");
  });

  it("超过一小时给该时刻的年月日（UTC，不按本地时区换算）", () => {
    expect(formatRelativeTime(AT, AT_MS + 3_600_000)).toBe("2026-10-02");
    expect(formatRelativeTime(AT, AT_MS + 3 * 24 * 3_600_000)).toBe("2026-10-02");
    // 跨零点的一小时：UTC 日期仍是会话自身那一天，本地时区换算会把它挪到 10-03。
    expect(formatRelativeTime("2026-10-02T23:30:00.000Z", AT_MS + 30 * 3_600_000)).toBe("2026-10-02");
  });

  it("同一份输入渲染同一段文本", () => {
    const first = formatRelativeTime(AT, AT_MS + 7 * 60_000);
    const second = formatRelativeTime(AT, AT_MS + 7 * 60_000);
    expect(first).toBe(second);
    expect(first).toBe("7 分钟前");
  });

  it("时刻形状不合规时原样返回，不假装算出了相对时间", () => {
    expect(formatRelativeTime("不是时刻", AT_MS)).toBe("不是时刻");
  });
});

describe("会话行的时间字段由模型给出相对文案", () => {
  const sessions: readonly SessionSummary[] = [
    session({ id: "cccccccc-0000-4000-8000-00000000000a", updatedAt: AT }),
    session({ id: "cccccccc-0000-4000-8000-00000000000b", updatedAt: "2026-10-01T07:00:00.000Z" }),
  ];

  it("行上同时给出精确时刻与相对文案（组件只渲染后者）", () => {
    const rows = buildSessionRowsByDirectoryId({
      resources: resources({ sessions }),
      ownerLabelOf: () => "工作站",
      nowMs: AT_MS + 5 * 60_000,
    }).get("work-api");
    if (rows === undefined) throw new Error("work-api 应有行");

    expect(rows.map((row) => row.updatedAtLabel)).toEqual(["5 分钟前", "2026-10-01"]);
    // 精确值仍在（`data-*` 与排序用它），因此组件没有理由自己去格式化它。
    expect(rows.map((row) => row.updatedAt)).toEqual([AT, "2026-10-01T07:00:00.000Z"]);
    expect(rows[0]?.title).toBe(`会话 ${"cccccccc-0000-4000-8000-00000000000a".slice(-4)}`);
  });

  it("只改时钟就得到与时间差一致的文案（同一份会话数据）", () => {
    const at = (nowMs: number): readonly string[] =>
      (buildSessionRowsByDirectoryId({ resources: resources({ sessions }), ownerLabelOf: () => "工作站", nowMs }).get("work-api") ?? [])
        .map((row) => row.updatedAtLabel);

    expect(at(AT_MS)).toEqual(["刚刚", "2026-10-01"]);
    expect(at(AT_MS + 2 * 60_000)).toEqual(["2 分钟前", "2026-10-01"]);
    expect(at(AT_MS + 2 * 3_600_000)).toEqual(["2026-10-02", "2026-10-01"]);
  });
});

describe("三处（目录页 / 详情页 / 对话页左栏）共用同一个相对口径", () => {
  it("同一条会话在三个模型里得到同一段文案", () => {
    const sessions: readonly SessionSummary[] = [session({ id: ONLINE_SESSION_ID, updatedAt: AT })];
    const nowMs = AT_MS + 9 * 60_000;
    const { store } = storeWith({
      resources: resources({ sessions, workspaces: WORKSPACES }),
      now: () => nowMs,
    });

    const inline = store.directoryPageSessions().get("work-api")?.[0]?.updatedAtLabel;
    const detail = store.directoryDetail("work-api");
    const sidebar = store.conversationPage(ONLINE_SESSION_ID)?.sidebar.rows[0]?.updatedAtLabel;

    expect(inline).toBe("9 分钟前");
    expect(detail.kind === "ready" ? detail.sessions[0]?.updatedAtLabel : null).toBe("9 分钟前");
    expect(sidebar).toBe("9 分钟前");
  });

  it("未命名会话的标题口径不变（空标题按下发口径呈现，不因时间改动而变）", () => {
    const sessions: readonly SessionSummary[] = [
      session({ id: ONLINE_SESSION_ID, title: null, updatedAt: AT }),
    ];
    const model = buildDirectoryDetailModel({
      resources: resources({ sessions }),
      directoryId: "work-api",
      ownerLabelOf: () => "工作站",
      nowMs: AT_MS + 30_000,
    });
    if (model.kind !== "ready") throw new Error(`期望 ready，实际是 ${model.kind}`);

    expect(model.sessions.map((row) => row.title)).toEqual([UNTITLED_SESSION_LABEL]);
    expect(model.sessions.map((row) => row.updatedAtLabel)).toEqual(["刚刚"]);
  });
});
