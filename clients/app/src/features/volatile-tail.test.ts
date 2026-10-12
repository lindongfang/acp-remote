/**
 * D6「实时尾巴 = 按消息标识的易失叠加层」，以及「正文只来自 `session.read`」这条不变量。
 *
 * ## 判别力
 *
 * - **先增量、后读取**这一序列是本文件的骨架：读取结果里出现同 id 时必须只剩一处、且以读取
 *   结果为准。若实现是「追加」而不是「按 `messageId` 替换」，文本会叠成两段 → 断言变红。
 * - 增量按 `deltaIndex` 去重：重投同一条增量不会让正文变成两份（上游 `eventId` 去重之外的
 *   第二道防线，也是网络重投的实际形态）。
 * - `turn.delta_compacted` 之后该轮次的累加内容必须消失，且迟到的增量不再把它带回来。
 * - `agent.message.completed` **不**贡献正文：它只标「增量流结束」——正文权威始终是
 *   `session.read`（D3 因此把消息类事件排除在历史条目之外）。
 */

import { describe, expect, it } from "vitest";

import type { MessageTimelineEntry } from "./conversation-timeline";
import type { ClientStore } from "./client-store";
import { SESSION_HISTORY_FEATURE } from "../sync-client";
import type { StoreFixture } from "./testing/fixtures";
import {
  eventMessage,
  lastDispatch,
  messageItem,
  onlineStore,
  readResult,
  resources,
  session,
} from "./testing/fixtures";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";
const TURN_ID = "turn-1";

function storeWithSession(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: SESSION_ID, state: "running" })] }),
    features: [SESSION_HISTORY_FEATURE],
  });
}

/** 一条消息增量事件。 */
function delta(input: {
  readonly eventId: string;
  readonly globalSequence: string;
  readonly eventType: string;
  readonly messageId: string;
  readonly deltaIndex: string;
  readonly text: string;
  readonly turnId?: string;
}): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: input.eventType,
      eventId: input.eventId,
      sessionId: SESSION_ID,
      globalSequence: input.globalSequence,
      view: {
        messageId: input.messageId,
        turnId: input.turnId ?? TURN_ID,
        deltaIndex: input.deltaIndex,
        text: input.text,
      },
    }),
  };
}

/** 该会话时间线上的消息条目（按 `kind` 收窄，不做未经检查的类型断言）。 */
function messageEntries(store: ClientStore): readonly MessageTimelineEntry[] {
  const page = store.conversationPage(SESSION_ID);
  if (page === null) throw new Error("会话页模型不应为 null");
  return page.timeline.entries.filter((entry): entry is MessageTimelineEntry => entry.kind === "message");
}

describe("D6 易失叠加层：累加、替换、作废", () => {
  it("多条增量累加成一条易失消息，正文按到达次序拼接", () => {
    const { store } = storeWithSession();
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b1",
        globalSequence: "11",
        eventType: "agent.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000e1",
        deltaIndex: "0",
        text: "你好",
      }),
    );
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b2",
        globalSequence: "12",
        eventType: "agent.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000e1",
        deltaIndex: "1",
        text: "，世界",
      }),
    );

    const entries = messageEntries(store);
    expect(entries).toHaveLength(1);
    expect(entries[0]?.volatile).toBe(true);
    expect(entries[0]?.message.text).toBe("你好，世界");
    expect(entries[0]?.channel).toBe("agent");
  });

  it("同一 messageId 的增量重投（同一 deltaIndex）不把正文变成两份", () => {
    const { store } = storeWithSession();
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b3",
        globalSequence: "13",
        eventType: "agent.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000e2",
        deltaIndex: "0",
        text: "一次",
      }),
    );
    // 同一 `eventId` 的重投由上游去重；这里换一个 `eventId`、同一 `deltaIndex`，模拟
    // 「同一次增量被另一条事件重复承载」这一形态。
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b4",
        globalSequence: "14",
        eventType: "agent.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000e2",
        deltaIndex: "0",
        text: "一次",
      }),
    );

    const entries = messageEntries(store);
    expect(entries).toHaveLength(1);
    expect(entries[0]?.message.text).toBe("一次");
  });

  it("先增量、后读取：以读取结果为准，易失条目不再呈现（不重复、不残留）", () => {
    const { store, gateway } = storeWithSession();
    const messageId = "cccccccc-0000-4000-8000-0000000000e3";
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b5",
        globalSequence: "15",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "0",
        text: "半句",
      }),
    );
    expect(messageEntries(store)).toHaveLength(1);

    store.loadSession(SESSION_ID);
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [messageItem({ id: messageId, createdAt: "2026-10-01T00:00:02.000Z", role: "agent", text: "完整正文" })],
      }),
    });

    const entries = messageEntries(store);
    // 反例：追加式实现会留下两条（一条易失、一条权威）→ 断言变红。
    expect(entries).toHaveLength(1);
    expect(entries[0]?.volatile).toBe(false);
    expect(entries[0]?.message.text).toBe("完整正文");
    // 残留检查：易失叠加层里那条确实被清掉了（不是渲染层过滤掉的）。
    expect(store.state.volatile.has(messageId)).toBe(false);
  });

  it("增量流结束标记只改「已结束」，不把增量拼出来的文本当成正文", () => {
    const { store } = storeWithSession();
    const messageId = "cccccccc-0000-4000-8000-0000000000e4";
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b6",
        globalSequence: "16",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "0",
        text: "草稿",
      }),
    );
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.message.completed",
        eventId: "dddddddd-0000-4000-8000-0000000000b7",
        sessionId: SESSION_ID,
        globalSequence: "17",
        view: { messageId, turnId: TURN_ID, content: [{ type: "text", text: "服务端给的完整正文" }] },
      }),
    });

    const entry = messageEntries(store)[0];
    expect(entry?.streamComplete).toBe(true);
    // 正文仍是累加出来的易失内容——它**没有**因为完成事件而变成权威正文。
    expect(entry?.volatile).toBe(true);
    expect(entry?.message.text).toBe("草稿");
  });

  it("turn.delta_compacted 作废该轮次的累加内容，迟到的增量也不再带回来", () => {
    const { store } = storeWithSession();
    const messageId = "cccccccc-0000-4000-8000-0000000000e5";
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000b8",
        globalSequence: "18",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "0",
        text: "会被压缩掉的半句",
      }),
    );
    expect(messageEntries(store)).toHaveLength(1);

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "turn.delta_compacted",
        eventId: "dddddddd-0000-4000-8000-0000000000b9",
        sessionId: SESSION_ID,
        globalSequence: "19",
        view: { turnId: TURN_ID, deltaCount: "1" },
      }),
    });

    // 反例：不清叠加层时这里仍是 1 条残留 → 断言变红。
    expect(messageEntries(store)).toHaveLength(0);
    expect(store.state.volatile.size).toBe(0);

    // 迟到的增量不得让作废的内容复活。
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000ba",
        globalSequence: "20",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "1",
        text: "迟到的一句",
      }),
    );
    expect(messageEntries(store)).toHaveLength(0);
  });

  it("压缩只作废本会话本轮的累加：别的轮次不受影响", () => {
    const { store } = storeWithSession();
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000bb",
        globalSequence: "21",
        eventType: "user.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000e6",
        deltaIndex: "0",
        text: "用户消息",
      }),
    );
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "turn.delta_compacted",
        eventId: "dddddddd-0000-4000-8000-0000000000bc",
        sessionId: SESSION_ID,
        globalSequence: "22",
        // 另一个轮次：与上面那条增量的 `turnId` 不同。
        view: { turnId: "turn-other", deltaCount: "3" },
      }),
    });

    expect(messageEntries(store)).toHaveLength(1);
  });
});
