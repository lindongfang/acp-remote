/**
 * 对话过程状态（`design.md` D3；spec「对话过程持续可见」）。
 *
 * ## 判别力
 *
 * - **互斥**：同一时刻只能有一个阶段。提交尚未受理时是 `submitting`，受理后没有内容时是
 *   `awaiting`，一有真实条目就换成实际内容阶段；恢复中只在没有内容占屏时出现。
 * - **不伪造**：`awaiting` 的文案是「等待响应」，不是「Agent 正在思考」；上传/封存阶段明确
 *   写「Agent 尚未收到任何内容」。
 * - **不重复占行**：只有 `submitting`/`restoring`/`awaiting` 有 `label`；实际内容与待处理卡片
 *   的 `label` 为 `null`（由折叠加/工具卡/回复气泡/授权卡自己表达），轮次失败/取消同理。
 * - **恢复完成即消失**：`activating` 时是 `restoring`，`ready` 后回到 `none`。
 */

import { describe, expect, it } from "vitest";

import type { CommandResultMessage } from "../protocol";
import { LOCAL_SESSION_LIFECYCLE_FEATURE } from "../sync-client";
import type { ConversationPageModel } from "./conversation-model";
import type { ClientStore } from "./client-store";
import type { StoreFixture } from "./testing/fixtures";
import {
  ONLINE_SESSION_ID,
  eventMessage,
  interactionStore,
  lastDispatch,
  onlineStore,
  resources,
  session,
} from "./testing/fixtures";

/** 一条 `session.prompt` 的受理结果（`accepted { turnId }`）。 */
function promptAccepted(requestId: string, turnId: string): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-0000000008a1",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      requestId,
      command: "session.prompt",
      status: "accepted",
      acceptedAt: "2026-10-01T00:00:02.000Z",
      terminalEventId: null,
      result: { turnId },
      error: null,
    },
  };
}

/** 取过程模型（会话一定在快照里）。 */
function processOf(store: ClientStore): ConversationPageModel["timeline"]["process"] {
  const page = store.conversationPage(ONLINE_SESSION_ID);
  if (page === null) throw new Error("会话页模型不应为 null");
  return page.timeline.process;
}

/** 提交一条输入并拿到受理后的请求标识。 */
function submit(store: ClientStore, text: string): string {
  const outcome = store.submitPrompt(ONLINE_SESSION_ID, text);
  if (!outcome.ok) throw new Error(outcome.reason);
  return outcome.requestId;
}

/** 注入一条实时事件（本会话）。 */
function applyEvent(
  store: ClientStore,
  input: {
    readonly eventType: string;
    readonly eventId: string;
    readonly view: Record<string, unknown>;
    readonly globalSequence?: string;
    readonly createdAt?: string;
  },
): void {
  store.apply({
    kind: "event",
    event: eventMessage({
      eventType: input.eventType,
      eventId: input.eventId,
      sessionId: ONLINE_SESSION_ID,
      view: input.view,
      ...(input.globalSequence === undefined ? {} : { globalSequence: input.globalSequence }),
      ...(input.createdAt === undefined ? {} : { createdAt: input.createdAt }),
    }),
  });
}

describe("过程阶段：提交中 → 等待响应 → 实际思考/工具/回复", () => {
  it("已提交但未受理时是「等待服务端确认」，不冒充已受理", () => {
    const { store } = interactionStore();
    submit(store, "帮我看看");
    const process = processOf(store);
    expect(process.phase).toBe("submitting");
    expect(process.label).toBe("已提交，等待服务端确认");
  });

  it("受理后没有输出：一条「等待响应」，不写成思考中", () => {
    const { store } = interactionStore();
    const requestId = submit(store, "帮我看看");
    store.apply({ kind: "command_result", result: promptAccepted(requestId, "turn-1") });

    const process = processOf(store);
    expect(process.phase).toBe("awaiting");
    expect(process.label).toBe("等待响应");
    expect(process.detail).toContain("Agent 尚未返回内容");
    expect(process.detail).not.toContain("思考");
  });

  it("思考、工具、回复各自接替等待阶段，且都不再另起过程行", () => {
    const { store } = interactionStore();
    const requestId = submit(store, "帮我看看");
    store.apply({ kind: "command_result", result: promptAccepted(requestId, "turn-1") });

    applyEvent(store, {
      eventType: "agent.thought.delta",
      eventId: "dddddddd-0000-4000-8000-0000000008b1",
      globalSequence: "10",
      createdAt: "2026-10-01T00:00:02.000Z",
      view: { messageId: "thought-1", turnId: "turn-1", deltaIndex: "0", text: "先看目录" },
    });
    const thinking = processOf(store);
    expect(thinking.phase).toBe("thought");
    expect(thinking.label).toBeNull();

    applyEvent(store, {
      eventType: "tool.call.started",
      eventId: "dddddddd-0000-4000-8000-0000000008b2",
      globalSequence: "11",
      createdAt: "2026-10-01T00:00:03.000Z",
      view: { toolCallId: "tool-1", turnId: "turn-1", title: "读取 package.json", state: "in_progress" },
    });
    expect(processOf(store).phase).toBe("tool");

    applyEvent(store, {
      eventType: "agent.message.delta",
      eventId: "dddddddd-0000-4000-8000-0000000008b3",
      globalSequence: "12",
      createdAt: "2026-10-01T00:00:04.000Z",
      view: { messageId: "reply-1", turnId: "turn-1", deltaIndex: "0", text: "看完了" },
    });
    const replying = processOf(store);
    expect(replying.phase).toBe("reply");
    expect(replying.label).toBeNull();
  });

  it("授权卡在等待时让位（attention），不与「等待响应」重叠", () => {
    const { store } = interactionStore();
    const requestId = submit(store, "帮我改");
    store.apply({ kind: "command_result", result: promptAccepted(requestId, "turn-1") });
    applyEvent(store, {
      eventType: "permission.requested",
      eventId: "dddddddd-0000-4000-8000-0000000008c1",
      view: {
        interactionId: "99999999-0000-4000-8000-000000000001",
        turnId: "turn-1",
        title: "允许写入 src/ 吗？",
        description: null,
        options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
      },
    });
    const process = processOf(store);
    expect(process.phase).toBe("attention");
    expect(process.label).toBeNull();
  });

  it("轮次失败且没有输出：不留下「等待响应」", () => {
    const { store } = interactionStore();
    const requestId = submit(store, "会失败");
    store.apply({ kind: "command_result", result: promptAccepted(requestId, "turn-1") });
    applyEvent(store, {
      eventType: "turn.failed",
      eventId: "dddddddd-0000-4000-8000-0000000008d1",
      view: { turnId: "turn-1", state: "failed", error: { code: "agent.error", message: "执行失败", retryable: false } },
    });
    expect(processOf(store).phase).toBe("none");
    expect(processOf(store).label).toBeNull();
  });

  it("历史用户消息没有配套轮次记录时不显示等待", () => {
    const { store } = interactionStore();
    applyEvent(store, {
      eventType: "user.message.delta",
      eventId: "dddddddd-0000-4000-8000-0000000008d3",
      createdAt: "2026-10-01T00:00:02.000Z",
      view: { messageId: "user-history", turnId: "turn-old", deltaIndex: "0", text: "旧消息" },
    });
    expect(processOf(store).phase).toBe("none");
    expect(processOf(store).label).toBeNull();
  });

  it("取消后同样不残留等待行", () => {
    const { store } = interactionStore();
    const requestId = submit(store, "会被取消");
    store.apply({ kind: "command_result", result: promptAccepted(requestId, "turn-1") });
    applyEvent(store, {
      eventType: "turn.cancelled",
      eventId: "dddddddd-0000-4000-8000-0000000008d2",
      view: { turnId: "turn-1", state: "cancelled" },
    });
    expect(processOf(store).phase).toBe("none");
  });
});

describe("过程阶段：会话恢复", () => {
  it("恢复中给一条恢复说明；恢复成功后撤掉它", () => {
    const fixture: StoreFixture = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" })] }),
      features: [LOCAL_SESSION_LIFECYCLE_FEATURE],
    });
    fixture.store.setActiveConversationSession(ONLINE_SESSION_ID);

    const restoring = processOf(fixture.store);
    expect(restoring.phase).toBe("restoring");
    expect(restoring.label).toBe("会话恢复中");

    const resume = lastDispatch(fixture.gateway);
    if (resume.body.command !== "session.resume") throw new Error("应先派发 session.resume");
    fixture.store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000008e1",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: resume.body.requestId,
          command: "session.resume",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: {
            sessionId: ONLINE_SESSION_ID,
            session: session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" }),
          },
          error: null,
        },
      },
    });

    expect(processOf(fixture.store).phase).toBe("none");
    expect(processOf(fixture.store).label).toBeNull();
  });

  it("恢复失败不留下恢复提示（失败原因由输入区/会话头说明）", () => {
    const fixture = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle", localActivity: "active" })] }),
      features: [LOCAL_SESSION_LIFECYCLE_FEATURE],
    });
    fixture.store.setActiveConversationSession(ONLINE_SESSION_ID);
    const resume = lastDispatch(fixture.gateway);
    if (resume.body.command !== "session.resume") throw new Error("应先派发 session.resume");
    fixture.store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000008e2",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: resume.body.requestId,
          command: "session.resume",
          status: "failed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: null,
          error: { code: "session.capacity_exceeded", message: "名额已满", retryable: false, details: {} },
        },
      },
    });

    expect(processOf(fixture.store).phase).toBe("none");
    expect(processOf(fixture.store).label).toBeNull();
    // 失败原因在输入区（同一个模型），不在过程行重复。
    expect(fixture.store.conversationPage(ONLINE_SESSION_ID)?.input.disabledReason).toContain("名额已满");
  });
});
