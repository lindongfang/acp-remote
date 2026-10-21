/**
 * 远程来源运行期：来源离线只清除该 Owner 的数据，owned 与其它来源不受影响。
 *
 * 判别力：
 * - 若清除按「所有 remote」或「按会话 id 粗暴删除」，B 来源或本机会话会一起消失；
 * - 若离线后仍从缓存标题/结构化记录渲染，`conversationPage` 的标题仍是旧标题或录非空；
 * - 若输入仍被标记为可发送，`canSend` 会为真。
 */

import { describe, expect, it } from "vitest";

import type { RemoteEventItem, SessionSummary } from "../protocol";
import { onlineStore, resources, session } from "./testing/fixtures";

const STATELESS_ORIGIN_FEATURE = "resource.stateless-origin.v1";

const OWNER_A = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
const OWNER_B = "c3d4e5f6-a7b8-4c9d-8e0f-1a2b3c4d5e6f";
const SESSION_A = "5d73cd10-a465-43cd-b3f1-704e2d49e99e";
const SESSION_B = "6e84de21-b576-44de-a4a2-815e3e5a0aa0";
const LOCAL_SESSION = "aaaaaaaa-0000-4000-8000-000000000001";
const ORIGIN_EPOCH = "c1d2e3f4-5061-4728-8394-95a6b7c8d9e0";

function remoteSummary(id: string, owner: string): SessionSummary {
  return session({
    id,
    title: `远程 ${id.slice(-4)}`,
    origin: { kind: "remote", ownerNodeId: owner, exportId: "export-1", originEpoch: ORIGIN_EPOCH, online: true },
  });
}

function remoteEvent(input: {
  readonly owner: string;
  readonly sessionId: string;
  readonly eventId: string;
  readonly sequence: string;
  readonly eventType: string;
  readonly view: Record<string, unknown>;
}): RemoteEventItem {
  return {
    originEventId: input.eventId,
    originEpoch: ORIGIN_EPOCH,
    originSequence: input.sequence,
    sessionRef: { ownerNodeId: input.owner, sessionId: input.sessionId },
    eventType: input.eventType,
    payloadDigest: "td6ZltnIg8vb-SZF_ZSHDPkxDCHmSxRs6ZH5SoV3BtU",
    createdAt: "2026-10-01T00:00:02.000Z",
    payload: { view: input.view },
  };
}

describe("远程来源状态只清除对应 Owner", () => {
  it("来源 A 离线：A 的摘要/记录清除，B 与 owned 保留，输入不再可发送", () => {
    const { store } = onlineStore({
      resources: resources({
        sessions: [remoteSummary(SESSION_A, OWNER_A), remoteSummary(SESSION_B, OWNER_B), session({ id: LOCAL_SESSION })],
      }),
      features: [STATELESS_ORIGIN_FEATURE],
    });
    store.setActiveConversationSession(SESSION_A);
    store.apply({
      kind: "remote_subscribing",
      sessionRef: { ownerNodeId: OWNER_A, sessionId: SESSION_A },
      subscriptionId: "1b2c3d4e-5f60-4b71-8d82-9fa0b1c2d3e4",
      cursor: null,
    });
    store.apply({
      kind: "remote_event",
      sessionRef: { ownerNodeId: OWNER_A, sessionId: SESSION_A },
      subscriptionId: "1b2c3d4e-5f60-4b71-8d82-9fa0b1c2d3e4",
      event: remoteEvent({
        owner: OWNER_A,
        sessionId: SESSION_A,
        eventId: "da702ba1-f49f-4e16-a677-a7744dfedba6",
        sequence: "2318",
        eventType: "tool.call.started",
        view: { toolCallId: "tool-1", title: "读取文件", kind: "read" },
      }),
    });
    expect([...store.state.structured.values()].some((record) => record.sessionId === SESSION_A)).toBe(true);

    store.apply({ kind: "remote_source_state", ownerNodeId: OWNER_A, state: "offline" });

    expect(store.state.resources?.sessions.some((item) => item.sessionId === SESSION_A)).toBe(false);
    expect(store.state.resources?.sessions.some((item) => item.sessionId === SESSION_B)).toBe(true);
    expect(store.state.resources?.sessions.some((item) => item.sessionId === LOCAL_SESSION)).toBe(true);
    expect([...store.state.structured.values()].some((record) => record.ownerNodeId === OWNER_A)).toBe(false);
    expect(store.state.remoteSessions.get(SESSION_A)?.sourceState).toBe("offline");

    const pageA = store.conversationPage(SESSION_A);
    expect(pageA?.header.title).toBe("远程会话不可用");
    expect(pageA?.header.title).not.toBe("远程 e99e");
    expect(pageA?.input.canSubmit).toBe(false);
    expect(pageA?.capabilities.canSend).toBe(false);

    const pageB = store.conversationPage(SESSION_B);
    expect(pageB?.header.title).toBe("远程 0aa0");
    expect(pageB?.capabilities.canSend).toBe(true);
    expect(store.conversationPage(LOCAL_SESSION)?.capabilities.canSend).toBe(true);
  });

  it("连接关闭：所有远程展示数据清除，owned 摘要保留", () => {
    const { store } = onlineStore({
      resources: resources({
        sessions: [remoteSummary(SESSION_A, OWNER_A), remoteSummary(SESSION_B, OWNER_B), session({ id: LOCAL_SESSION })],
      }),
      features: [STATELESS_ORIGIN_FEATURE],
    });
    store.apply({ kind: "closed", code: 1000, retryable: true });

    expect(store.state.resources?.sessions.some((item) => item.origin.kind === "remote")).toBe(false);
    expect(store.state.resources?.sessions.some((item) => item.sessionId === LOCAL_SESSION)).toBe(true);
  });
});
