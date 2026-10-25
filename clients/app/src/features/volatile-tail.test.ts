/**
 * D6「实时尾巴 = 按消息标识的易失叠加层」，以及「正文只来自 `session.read`」这条不变量。
 *
 * ## 判别力
 *
 * - **先增量、后读取**这一序列是本文件的骨架：读取结果里出现同 id 时必须只剩一处、且以读取
 *   结果为准。若实现是「追加」而不是「按 `messageId` 替换」，文本会叠成两段 → 断言变红。
 * - 增量按 `deltaIndex` 去重：重投同一条增量不会让正文变成两份（上游 `eventId` 去重之外的
 *   第二道防线，也是网络重投的实际形态）。
 * - `turn.delta_compacted` 之后正文继续可见，直到权威读取在同一状态更新中替换它；
 *   迟到的增量不再改变正在展示的内容。
 * - `agent.message.completed` **不**贡献正文：它只标「增量流结束」——正文权威始终是
 *   `session.read`（D3 因此把消息类事件排除在历史条目之外）。
 */

import { describe, expect, it } from "vitest";

import type { EventMessage } from "../protocol";
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

/** 一条消息增量事件（`block` 是 `agent.message.delta` 可选的媒体/资源块）。 */
function delta(input: {
  readonly eventId: string;
  readonly globalSequence: string;
  readonly eventType: string;
  readonly messageId: string;
  readonly deltaIndex: string;
  readonly text: string;
  readonly turnId?: string;
  readonly block?: Record<string, unknown>;
}): { readonly kind: "event"; readonly event: EventMessage } {
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
        ...(input.block === undefined ? {} : { block: input.block }),
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

  it("turn.delta_compacted 保留可见正文直到权威替换，迟到的增量不再改变它", () => {
    const { store, gateway } = storeWithSession();
    store.setActiveConversationSession(SESSION_ID);
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

    // 移除流式条目会造成正文先消失、再异步回填的可见闪动。
    expect(messageEntries(store)).toHaveLength(1);
    expect(messageEntries(store)[0]?.message.text).toBe("会被压缩掉的半句");
    expect(store.state.volatile.size).toBe(1);

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
    expect(messageEntries(store)).toHaveLength(1);
    expect(messageEntries(store)[0]?.message.text).toBe("会被压缩掉的半句");

    store.apply({
      kind: "command_result",
      result: readResult({
        requestId: lastDispatch(gateway).body.requestId,
        sessionId: SESSION_ID,
        messages: [],
      }),
    });
    expect(messageEntries(store)).toHaveLength(0);
    expect(store.state.volatile.has(messageId)).toBe(false);
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

/**
 * 纯媒体增量（`text` 为空、只有 `block`）。
 *
 * schema 的 `text` 无 `minLength`，媒体块正是「仅非文本内容」的载体；正文投影为空串。判空若
 * 发生在块解析之前（或把空串当缺席），图片/音频/文件增量会被整体丢弃——实时会话里这条媒体
 * 在手动读取前永远不会出现。三条用例分别钉住三种块，另有一条对照证明判空没有放宽成「什么都收」。
 */
describe("D6 纯媒体增量进入易失叠加层", () => {
  const ATTACHMENT_ID = "eeeeeeee-0000-4000-8000-000000000001";

  it("图片增量：正文为空串，媒体块原样进入易失内容", () => {
    const { store } = storeWithSession();
    const messageId = "cccccccc-0000-4000-8000-0000000000f1";
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000f1",
        globalSequence: "31",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "0",
        text: "",
        block: {
          type: "image_ref",
          attachmentId: ATTACHMENT_ID,
          mimeType: "image/png",
          byteLength: "12",
          displayState: "available",
        },
      }),
    );

    const entries = messageEntries(store);
    expect(entries).toHaveLength(1);
    expect(entries[0]?.volatile).toBe(true);
    expect(entries[0]?.message.text).toBe("");
    expect(entries[0]?.message.hasStructuredContent).toBe(true);
    expect(entries[0]?.message.content).toEqual({
      text: "",
      media: [
        {
          kind: "image",
          mimeType: "image/png",
          byteLength: "12",
          availability: "available",
          attachmentId: ATTACHMENT_ID,
        },
      ],
      hasStructuredContent: true,
    });
  });

  it("音频与文件增量各自保留种类（按到达顺序累积）", () => {
    const { store } = storeWithSession();
    const messageId = "cccccccc-0000-4000-8000-0000000000f2";
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000f2",
        globalSequence: "32",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "0",
        text: "",
        block: {
          type: "audio_ref",
          attachmentId: "eeeeeeee-0000-4000-8000-000000000002",
          mimeType: "audio/mpeg",
          byteLength: "99",
          displayState: "available",
        },
      }),
    );
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000f3",
        globalSequence: "33",
        eventType: "agent.message.delta",
        messageId,
        deltaIndex: "1",
        text: "",
        block: {
          type: "file_ref",
          attachmentId: "eeeeeeee-0000-4000-8000-000000000003",
          mimeType: "application/pdf",
          byteLength: "34",
          displayName: "报告.pdf",
          displayState: "not_fetched",
        },
      }),
    );

    const media = messageEntries(store)[0]?.message.content.media;
    expect(media).toEqual([
      {
        kind: "audio",
        mimeType: "audio/mpeg",
        byteLength: "99",
        availability: "available",
        attachmentId: "eeeeeeee-0000-4000-8000-000000000002",
      },
      {
        kind: "file",
        attachmentId: "eeeeeeee-0000-4000-8000-000000000003",
        mimeType: "application/pdf",
        byteLength: "34",
        displayName: "报告.pdf",
        availability: "not_fetched",
      },
    ]);
  });

  it("空文本且没有块仍被丢弃：判空只放行「有可呈现内容」的增量", () => {
    const { store } = storeWithSession();
    store.apply(
      delta({
        eventId: "dddddddd-0000-4000-8000-0000000000f4",
        globalSequence: "34",
        eventType: "agent.message.delta",
        messageId: "cccccccc-0000-4000-8000-0000000000f3",
        deltaIndex: "0",
        text: "",
      }),
    );

    expect(messageEntries(store)).toHaveLength(0);
    expect(store.state.volatile.size).toBe(0);
  });
});
