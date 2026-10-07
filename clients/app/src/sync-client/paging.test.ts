/**
 * R2/R3/D2：`session.read` 分页与 `(createdAt, messageId)` 复合游标。
 *
 * 判别力来源：
 * - 游标换成单用 `createdAt` → 「同一时刻的多条」用例变红（会漏或重）；
 * - 游标换成单用 `messageId` → 「分量缺失即抛错」用例变红；
 * - 游标构造按 `messageId` 排序 → 「不按标识排序」用例变红；
 * - `hasEarlier` 缺失时默认 false → 「缺 hasEarlier 抛错」用例变红。
 */

import { describe, expect, it } from "vitest";

import {
  IncompleteReadCursorError,
  assertPagesDoNotOverlap,
  buildReadPayload,
  cursorOf,
  earliestCursor,
  toLoadedPage,
} from "./paging";
import type { SnapshotItemMessages } from "../protocol";

/** 造一条消息；`createdAt`/`messageId` 可控以便构造边界。 */
function message(input: {
  readonly messageId: string;
  readonly createdAt: string;
  readonly role?: "user" | "agent";
}): SnapshotItemMessages {
  return {
    messageId: input.messageId,
    sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    role: input.role ?? "user",
    content: [{ type: "text", text: "内容" }],
    status: "completed",
    createdAt: input.createdAt,
    turnId: null,
  };
}

const T0 = "2026-10-01T00:00:00.000Z";
const T1 = "2026-10-01T00:00:01.000Z";

describe("R3：游标由时间与标识复合构成", () => {
  it("游标同时含 createdAt 与 messageId", () => {
    const cursor = earliestCursor([message({ messageId: "m-2", createdAt: T1 }), message({ messageId: "m-1", createdAt: T0 })]);
    expect(cursor).toEqual({ createdAt: T0, messageId: "m-1" });
    // 反例：若只保留 messageId，`createdAt` 会缺席 → 断言变红。
    expect(Object.keys(cursor ?? {}).sort()).toEqual(["createdAt", "messageId"]);
  });

  it("任一分量缺失即抛错，不猜测或补全", () => {
    const withoutCreatedAt = { ...message({ messageId: "m-1", createdAt: T0 }), createdAt: "" };
    const withoutMessageId = { ...message({ messageId: "m-1", createdAt: T0 }), messageId: "" };

    expect(() => cursorOf(withoutCreatedAt)).toThrow(IncompleteReadCursorError);
    // 反例：若客户端补全缺失分量，这里不会抛 → 断言变红。
    expect(() => cursorOf(withoutMessageId)).toThrow(IncompleteReadCursorError);
  });

  it("同一时刻的多条消息不重不漏：游标取页内最早一条并保留其标识", () => {
    // 服务端按创建时间升序返回，同一时刻内顺序稳定。
    const page = [
      message({ messageId: "m-a", createdAt: T0 }),
      message({ messageId: "m-b", createdAt: T0 }),
      message({ messageId: "m-c", createdAt: T1 }),
    ];
    const cursor = earliestCursor(page);
    // 页内最早一条是 m-a；下一页从它之前取，因此 m-b/m-c 不会被跳过。
    expect(cursor).toEqual({ createdAt: T0, messageId: "m-a" });

    const older = [message({ messageId: "m-x", createdAt: T0 })];
    // 相邻两页不重叠：下一页的消息标识与本页交集为空。
    expect(() => assertPagesDoNotOverlap(page, older)).not.toThrow();
  });

  it("不按标识排序：标识次序与时间次序相反时仍取最早时间", () => {
    const page = [
      message({ messageId: "zzz", createdAt: T0 }),
      message({ messageId: "aaa", createdAt: T1 }),
    ];
    const cursor = earliestCursor(page);
    // 反例：若按 messageId 排序，会取到 "aaa"（时间较晚）→ 断言变红。
    expect(cursor).toEqual({ createdAt: T0, messageId: "zzz" });
  });

  it("页内同刻消息按出现顺序取第一条（前置条件：服务端按 (createdAt, messageId) 升序）", () => {
    // 该前置条件由服务端契约保证，客户端不用 messageId 排序（spec 场景「不按标识排序」）。
    const page = [
      message({ messageId: "m-first", createdAt: T0 }),
      message({ messageId: "m-second", createdAt: T0 }),
    ];
    // 反例：若改成按 messageId 排序或用「后出现者胜」，这里会变成 m-second → 断言变红。
    expect(earliestCursor(page)).toEqual({ createdAt: T0, messageId: "m-first" });
  });

  it("时间比较按 epoch 毫秒而非字典序（形状固定时两者等价）", () => {
    const page = [
      message({ messageId: "m-early", createdAt: "2026-10-01T00:00:00.000Z" }),
      message({ messageId: "m-late", createdAt: "2026-10-01T00:00:01.000Z" }),
      message({ messageId: "m-mid", createdAt: "2026-10-01T00:00:00.500Z" }),
    ];
    expect(earliestCursor(page)).toEqual({ createdAt: "2026-10-01T00:00:00.000Z", messageId: "m-early" });
  });

  it("空页返回 null 游标", () => {
    expect(earliestCursor([])).toBeNull();
  });
});

describe("R2：hasEarlier 驱动向上翻页", () => {
  it("hasEarlier 为真时给出下一页游标", () => {
    const page = toLoadedPage({
      sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
      resources: { messages: [message({ messageId: "m-20", createdAt: T1 })] },
      hasEarlier: true,
    });
    expect(page.hasEarlier).toBe(true);
    expect(page.nextBefore).toEqual({ createdAt: T1, messageId: "m-20" });
  });

  it("hasEarlier 为假时不再给出游标（不返回空占位）", () => {
    const page = toLoadedPage({
      sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
      resources: { messages: [message({ messageId: "m-1", createdAt: T0 })] },
      hasEarlier: false,
    });
    expect(page.hasEarlier).toBe(false);
    expect(page.nextBefore).toBeNull();
  });

  it("hasEarlier 缺失时抛错（它是必填布尔）", () => {
    const malformed = {
      sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
      resources: { messages: [] },
    } as unknown as Parameters<typeof toLoadedPage>[0];
    // 反例：若缺省当 false，用户会以为已到最早一条 → 断言变红。
    expect(() => toLoadedPage(malformed)).toThrow(/hasEarlier/);
  });

  it("hasEarlier 为真但无消息时抛错（自相矛盾的结果）", () => {
    expect(() =>
      toLoadedPage({
        sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
        resources: {},
        hasEarlier: true,
      }),
    ).toThrow(/更早内容/);
  });

  it("相邻两页出现同一 messageId 时抛错（防止重复呈现）", () => {
    const newer = [message({ messageId: "m-5", createdAt: T1 })];
    const older = [message({ messageId: "m-5", createdAt: T0 })];
    // 反例：若不检测重叠，拼接后同一条消息会显示两次 → 断言变红。
    expect(() => assertPagesDoNotOverlap(newer, older)).toThrow(/重叠/);
  });

  it("较早页含有比最新页最早一条更新的消息时判漏读并抛错", () => {
    const newer = [message({ messageId: "m-5", createdAt: T1 })];
    // 标识不重叠，但「较早页」里出现了**更新**的消息：续取游标越过了一部分消息。
    const older = [message({ messageId: "m-9", createdAt: "2026-10-01T00:00:02.000Z" })];
    // 反例：若只查 messageId 重叠而不看时间边界，这里静默通过 → 断言变红。
    expect(() => assertPagesDoNotOverlap(newer, older)).toThrow(/漏读/);
  });

  it("正常升序的相邻两页不抛错", () => {
    const newer = [message({ messageId: "m-5", createdAt: T1 })];
    const older = [message({ messageId: "m-4", createdAt: T0 })];
    expect(() => assertPagesDoNotOverlap(newer, older)).not.toThrow();
  });
});

describe("R2：payload 只在提供时带分页参数", () => {
  it("省略 before/limit 时 payload 只有 include", () => {
    const payload = buildReadPayload({ sessionId: "s", include: ["messages"] });
    expect(payload).toEqual({ include: ["messages"] });
    expect("before" in payload).toBe(false);
    expect("limit" in payload).toBe(false);
  });

  it("带 before 与 limit 时两者都出现", () => {
    const payload = buildReadPayload({
      sessionId: "s",
      include: ["messages", "turns"],
      before: { createdAt: T1, messageId: "m-20" },
      limit: 20,
    });
    expect(payload).toEqual({ include: ["messages", "turns"], before: { createdAt: T1, messageId: "m-20" }, limit: 20 });
  });

  it("只给 messageId 的游标无法构造（不接受单分量）", () => {
    const partial = { messageId: "m-20" } as unknown as { createdAt: string; messageId: string };
    expect(() => cursorOf(message({ messageId: partial.messageId, createdAt: partial.createdAt }))).toThrow(
      IncompleteReadCursorError,
    );
  });
});