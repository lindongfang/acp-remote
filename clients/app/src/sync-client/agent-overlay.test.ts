/**
 * R19：Agent 列表来自快照、连接状态是可选覆盖层。
 *
 * 判别力来源：
 * - 把覆盖层缺席当作已断开 → 「缺席即状态未知」用例变红；
 * - 用会话活跃度推断连接状态 → 「只有 agent.* 事件改变覆盖层」用例变红；
 * - 按展示名合并 → 「同名 Agent 以标识区分」用例变红；
 * - 覆盖层缺席时移除 Agent → 列表长度断言变红。
 */

import { describe, expect, it } from "vitest";

import { AgentConnectionOverlayStore, overlayFromEvent } from "./agent-overlay";
import { buildAgentCatalogView } from "../domain";
import type { AgentCatalogEntry } from "../protocol";
import { makeEvent } from "./testing/harness";

const AGENTS: readonly AgentCatalogEntry[] = [
  { agentId: "claude", displayName: "Claude", default: true },
  { agentId: "codex", displayName: "Codex", default: false },
];

describe("R19：覆盖层缺席即状态未知", () => {
  it("无任何连接事件时全部条目为 unknown，且列表不缩减", () => {
    const store = new AgentConnectionOverlayStore();
    const view = buildAgentCatalogView(AGENTS, store.entries());

    expect(view.entries.map((entry) => entry.connection)).toEqual(["unknown", "unknown"]);
    // 反例：若把缺席当已断开，这里会是 ["disconnected", ...] → 断言变红。
    expect(view.entries.map((entry) => entry.connection)).not.toContain("disconnected");
    // 反例：若因覆盖层缺席而移除 Agent，这里长度会是 0 → 断言变红。
    expect(view.entries).toHaveLength(2);
    expect(view.overlayPresent).toBe(false);
  });

  it("连接事件只改对应条目，其它保持未知", () => {
    const store = new AgentConnectionOverlayStore();
    store.ingest(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.connected", view: { agentId: "claude", state: "connected" } }));

    const view = buildAgentCatalogView(AGENTS, store.entries());
    expect(view.entries.map((entry) => [entry.id, entry.connection])).toEqual([
      ["claude", "connected"],
      ["codex", "unknown"],
    ]);
    expect(view.overlayPresent).toBe(true);
  });

  it("断开事件携带公开错误时记录原因", () => {
    const store = new AgentConnectionOverlayStore();
    store.ingest(
      makeEvent({
        globalSequence: "1",
        eventId: "e-1",
        eventType: "agent.disconnected",
        view: {
          agentId: "codex",
          state: "disconnected",
          error: { code: "capability.unsupported_by_agent", message: "运行时退出", retryable: false, details: {} },
        },
      }),
    );

    const entry = buildAgentCatalogView(AGENTS, store.entries()).entries.find((item) => item.id === "codex");
    expect(entry?.connection).toBe("disconnected");
    expect(entry?.disconnectReason).toBe("运行时退出");
  });

  it("断开事件无 error 时原因保持 null（不编造原因）", () => {
    const store = new AgentConnectionOverlayStore();
    store.ingest(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.disconnected", view: { agentId: "codex", state: "disconnected" } }));
    const entry = buildAgentCatalogView(AGENTS, store.entries()).entries.find((item) => item.id === "codex");
    expect(entry?.disconnectReason).toBeNull();
  });
});

describe("R19：不以其它信号推断连接状态", () => {
  it("会话与 turn 事件不改变覆盖层", () => {
    const store = new AgentConnectionOverlayStore();
    const sessionEvent = makeEvent({
      globalSequence: "1",
      eventId: "e-1",
      eventType: "session.updated",
      view: { session: { agent: { agentId: "claude", name: "Claude" }, state: "running" } },
    });
    const turnEvent = makeEvent({
      globalSequence: "2",
      eventId: "e-2",
      eventType: "turn.started",
      view: { turnId: "t-1", state: "running" },
    });

    // 反例：若从 `session.state = running` 推断 Agent 已连接，这里会返回覆盖层条目 → 断言变红。
    expect(store.ingest(sessionEvent)).toBeNull();
    expect(store.ingest(turnEvent)).toBeNull();
    expect(store.entries()).toHaveLength(0);
    expect(buildAgentCatalogView(AGENTS, store.entries()).entries.every((entry) => entry.connection === "unknown")).toBe(true);
  });

  it("未识别的 Agent 事件类型不产生覆盖层条目", () => {
    expect(overlayFromEvent(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "file.changed" }))).toBeNull();
  });

  it("视图缺 agentId 时不产生覆盖层条目", () => {
    expect(
      overlayFromEvent(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.connected", view: { state: "connected" } })),
    ).toBeNull();
  });

  it("会话处于 running 也不得推断 Agent 已连接", () => {
    const store = new AgentConnectionOverlayStore();
    // 一个正在跑 turn 的会话：会话活跃度**不是** Agent 进程状态（`design.md` D6）。
    store.ingest(
      makeEvent({
        globalSequence: "1",
        eventId: "e-1",
        eventType: "session.updated",
        view: {
          session: {
            sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
            agent: { agentId: "claude", name: "Claude" },
            state: "running",
            origin: { kind: "local" },
            currentMode: null,
            version: "3",
            createdAt: "2026-10-01T00:00:00.000Z",
            updatedAt: "2026-10-01T00:00:05.000Z",
          },
        },
      }),
    );

    // 反例：若从 `session.agent` 或 `session.state === "running"` 推断，这里会有覆盖层条目 → 断言变红。
    expect(store.entries()).toHaveLength(0);
    const view = buildAgentCatalogView(AGENTS, store.entries());
    expect(view.entries.find((entry) => entry.id === "claude")?.connection).toBe("unknown");
  });
});

describe("R19：同名 Agent 以标识区分", () => {
  it("两个同名 Agent 按标识各自保留连接状态", () => {
    const sameName: readonly AgentCatalogEntry[] = [
      { agentId: "claude-a", displayName: "Claude", default: true },
      { agentId: "claude-b", displayName: "Claude", default: false },
    ];
    const store = new AgentConnectionOverlayStore();
    store.ingest(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.connected", view: { agentId: "claude-a", state: "connected" } }));

    const view = buildAgentCatalogView(sameName, store.entries());
    // 反例：若按展示名合并，列表会只剩一项且两个状态相同 → 断言变红。
    expect(view.entries).toHaveLength(2);
    expect(view.entries.map((entry) => [entry.id, entry.connection])).toEqual([
      ["claude-a", "connected"],
      ["claude-b", "unknown"],
    ]);
  });
});

describe("R19：切换窗口的重复事件不污染覆盖层", () => {
  it("同一事件重投后覆盖层仍只有一条", () => {
    const store = new AgentConnectionOverlayStore();
    const event = makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.connected", view: { agentId: "claude", state: "connected" } });
    store.ingest(event);
    store.ingest(event);

    expect(store.entries()).toHaveLength(1);
  });

  it("后到的断开事件覆盖先前的连接状态（按 agentId 覆盖）", () => {
    const store = new AgentConnectionOverlayStore();
    store.ingest(makeEvent({ globalSequence: "1", eventId: "e-1", eventType: "agent.connected", view: { agentId: "claude", state: "connected" } }));
    store.ingest(makeEvent({ globalSequence: "2", eventId: "e-2", eventType: "agent.disconnected", view: { agentId: "claude", state: "disconnected" } }));

    const entries = store.entries();
    expect(entries).toHaveLength(1);
    expect(entries[0]?.state).toBe("disconnected");
  });
});