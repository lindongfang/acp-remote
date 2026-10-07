/**
 * 明细现取与向上翻页（`design.md` D8 + `tasks.md` 2.8 的「滚动加载更早的用户输入」）。
 *
 * 判别力来源：
 * - 游标必须是 `(createdAt, messageId)` **两键**——只带 `messageId` 或只带 `createdAt`
 *   的载荷会让断言变红（`$defs.sessionReadBefore` 要求两键都必需）；
 * - 续取游标取上一页**最早**一条，而不是最新一条（用构造出来的两页区分）；
 * - 相邻两页重叠（重复的 `messageId`）必须抛错而不是静默拼接——这正是「不重」的判据；
 * - 断连时不发读取命令（信封需要 `connectionId`）。
 */

import { describe, expect, it } from "vitest";

import type { CommandMessage, CommandResultMessage } from "../protocol";
import { resources, session, storeWith, messageItem } from "./testing/fixtures";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";

/** 造一条 `session.read` 的 completed 结果。 */
function readResult(input: {
  readonly requestId: string;
  readonly messages: ReturnType<typeof messageItem>[];
  readonly hasEarlier: boolean;
}): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-0000000000ff",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      requestId: input.requestId,
      command: "session.read",
      status: "completed",
      acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalEventId: null,
      result: { sessionId: SESSION_ID, resources: { messages: input.messages }, hasEarlier: input.hasEarlier },
      error: null,
    },
  };
}

/** 取出最后一条派发的 `session.read` 命令。 */
function lastReadCommand(dispatches: readonly CommandMessage[]): CommandMessage {
  const command = dispatches[dispatches.length - 1];
  if (command === undefined || command.body.command !== "session.read") {
    throw new Error("没有派发 session.read");
  }
  return command;
}

/** 最近一页（服务端按 `(createdAt, messageId)` 升序返回）：两条消息，声称还有更早内容。 */
const LATEST_PAGE = [
  messageItem({ id: "m-2", createdAt: "2026-10-01T00:00:02.000Z", text: "第二条" }),
  messageItem({ id: "m-3", createdAt: "2026-10-01T00:00:03.000Z", text: "第三条" }),
];

/** 更早一页：一条消息，没有更早内容了。 */
const EARLIER_PAGE = [messageItem({ id: "m-1", createdAt: "2026-10-01T00:00:01.000Z", text: "第一条" })];

/** 取出最后一条派发的 `session.read` 命令。 */
function storeFixture(connectionId: string | null): ReturnType<typeof storeWith> {
  return storeWith({
    resources: resources({ sessions: [session({ id: SESSION_ID, state: "running" })], agents: [] }),
    scopes: ["session.read"],
    connectionId,
  });
}

describe("D8 明细现取与复合游标翻页", () => {
  it("首次进入会话发一条不带游标的 session.read", () => {
    const { store, gateway } = storeFixture("2de54db7-74ae-4c21-88e3-05df0dc2e637");

    store.loadSession(SESSION_ID);

    expect(gateway.dispatches).toHaveLength(1);
    const body = lastReadCommand(gateway.dispatches).body;
    if (body.command !== "session.read") throw new Error("命令不是 session.read");
    // 反例：首屏就带游标，这里会出现 before，断言变红。
    expect(body.payload).toEqual({ include: ["messages"] });
  });

  it("向上翻页带的是上一页最早一条的 (createdAt, messageId) 复合游标", () => {
    const { store, gateway } = storeFixture("2de54db7-74ae-4c21-88e3-05df0dc2e637");
    store.loadSession(SESSION_ID);
    const firstRequestId = lastReadCommand(gateway.dispatches).body.requestId;
    store.apply({ kind: "command_result", result: readResult({ requestId: firstRequestId, messages: LATEST_PAGE, hasEarlier: true }) });

    store.loadEarlier(SESSION_ID);

    expect(gateway.dispatches).toHaveLength(2);
    const body = lastReadCommand(gateway.dispatches).body;
    if (body.command !== "session.read") throw new Error("命令不是 session.read");
    // 反例：用最新一条（m-3）当游标，这里会变成 00:00:03，断言变红。
    expect(body.payload).toEqual({
      include: ["messages"],
      before: { createdAt: "2026-10-01T00:00:02.000Z", messageId: "m-2" },
    });
    // 复合游标两键都必须在（单字段游标按协议拒绝）。
    expect(Object.keys(body.payload.before ?? {}).sort()).toEqual(["createdAt", "messageId"]);
  });

  it("更早一页并入后按时间顺序排列，且 hasEarlier 随之更新", () => {
    const { store, gateway } = storeFixture("2de54db7-74ae-4c21-88e3-05df0dc2e637");
    store.loadSession(SESSION_ID);
    const firstRequestId = lastReadCommand(gateway.dispatches).body.requestId;
    store.apply({ kind: "command_result", result: readResult({ requestId: firstRequestId, messages: LATEST_PAGE, hasEarlier: true }) });

    store.loadEarlier(SESSION_ID);
    const secondRequestId = lastReadCommand(gateway.dispatches).body.requestId;
    store.apply({ kind: "command_result", result: readResult({ requestId: secondRequestId, messages: EARLIER_PAGE, hasEarlier: false }) });

    const model = store.conversationPage(SESSION_ID);
    expect(model?.messages.map((item) => item.id)).toEqual(["m-1", "m-2", "m-3"]);
    expect(model?.paging.hasEarlier).toBe(false);
    expect(model?.paging.nextBefore).toBeNull();
  });

  it("相邻两页重叠（同一 messageId 出现两次）时抛错而不是静默拼接", () => {
    const { store, gateway } = storeFixture("2de54db7-74ae-4c21-88e3-05df0dc2e637");
    store.loadSession(SESSION_ID);
    const firstRequestId = lastReadCommand(gateway.dispatches).body.requestId;
    store.apply({ kind: "command_result", result: readResult({ requestId: firstRequestId, messages: LATEST_PAGE, hasEarlier: true }) });

    store.loadEarlier(SESSION_ID);
    const secondRequestId = lastReadCommand(gateway.dispatches).body.requestId;
    // 服务端把已经给过的 m-3 又给了一次。
    expect(() =>
      store.apply({
        kind: "command_result",
        result: readResult({
          requestId: secondRequestId,
          messages: [messageItem({ id: "m-3", createdAt: "2026-10-01T00:00:03.000Z" })],
          hasEarlier: false,
        }),
      }),
    ).toThrow(/重叠/);
  });

  it("没有已建立连接时不派发读取命令（信封需要 connectionId）", () => {
    const { store, gateway } = storeFixture(null);

    store.loadSession(SESSION_ID);

    // 反例：若凭空编一个 connectionId，这里会出现派发记录，断言变红。
    expect(gateway.dispatches).toEqual([]);
  });

  it("读取失败时呈现结构化失败原因并停止 loading", () => {
    const { store, gateway } = storeFixture("2de54db7-74ae-4c21-88e3-05df0dc2e637");
    store.loadSession(SESSION_ID);
    const requestId = lastReadCommand(gateway.dispatches).body.requestId;

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000fe",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId,
          command: "session.read",
          status: "failed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: null,
          error: { code: "session.not_found", message: "会话不存在", retryable: false, details: {} },
        },
      },
    });

    const model = store.conversationPage(SESSION_ID);
    expect(model?.paging.error).toContain("session.not_found");
    expect(model?.paging.loading).toBe(false);
  });
});
