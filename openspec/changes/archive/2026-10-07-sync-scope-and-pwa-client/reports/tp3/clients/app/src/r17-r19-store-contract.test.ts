// 【交接副本】来源 worktree .worktrees/tp3 的 clients/app/src/r17-r19-store-contract.test.ts
// 交付提交 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a（分支 feat/tp3），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R17 / R19：上下文三态与 Agent 覆盖层缺席，**经 `ClientStore` 这条生产管线**判定。
 *
 * ## 为什么走仓库而不是直调模型
 *
 * `review-wp7-r1` 指出既有用例「直调模型、从不经过 `ClientStore`」，于是两处生产缺陷带着
 * 全绿通过（一次是降级分支不可达，一次是快照屏障污染恢复点）。本文件因此把判定放在
 * `ClientStore.apply()` → 选择器 → 组件渲染这条**完整路径**上：
 *
 * - R17：`session.usage.changed` 事件经 store 落到 `usage` 映射，再经 `conversationPage()`
 *   投影成三态，最后渲染成 HTML；
 * - R19：`agent.connected`/`agent.disconnected` 事件经 store 落到覆盖层，再经
 *   `agentCatalog()` 与 `hostPanel()` 呈现。
 *
 * 若 store 忘了把用量写进 `usage` 映射（而不是模型算错），本文件的断言同样会红。
 *
 * ## 判别力
 *
 * - R17：未上报与零窗口必须给出**互不相同**的 `kind`，且渲染结果里不出现 `0`、`0%`、
 *   `NaN`、`Infinity`——任何「以零冒充」的实现都会在这里被抓住；
 * - R19：覆盖层缺席时 `connection` 必须是 `unknown` 而不是 `disconnected`，条目不得减少，
 *   且会话处于 `running` 也不得被推断成已连接。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ContextUsageBar } from "./components/ContextUsageBar";
import { buildContextUsageView } from "./domain";
import { ClientStore } from "./features/client-store";
import { eventMessage, recordingGateway, resources } from "./features/testing/fixtures";

/** 会话用量视图的类型（由 domain 层定义）。 */
type ContextUsageViewShape = NonNullable<ReturnType<ClientStore["conversationPage"]>>["usage"];

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";
const AGENT_A = "codex-a";
const AGENT_B = "codex-b";

/** 会话必须存在于快照里（否则三态无从判定）。 */
function requiredUsage(page: ReturnType<ClientStore["conversationPage"]>): ContextUsageViewShape {
  if (page === null) throw new Error("会话不在快照中：测试前提不成立");
  return page.usage;
}

/** 建一个已喂入快照的仓库。 */
function storeWithSnapshot(): ClientStore {
  const gateway = recordingGateway({ resources: resources({}) });
  let counter = 0;
  const store = new ClientStore({
    sync: gateway,
    ids: {
      uuid: (): string => {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    now: () => 1_700_000_000_000,
    identity: { connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637" },
  });
  store.apply({
    kind: "snapshot_verified",
    snapshot: {
      ...(gateway.current as NonNullable<typeof gateway.current>),
      chunkCount: 3,
      receivedChunks: 3,
      snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
    },
  });
  // 走完整条连接路径（unpaired → pairing → disconnected → connecting → authenticating
  // → replaying → online）：单独发 `online` 会被迁移表拒绝，状态停在 unpaired，
  // 下游能力判定全部失真——那正是「用例绕过了状态机」会出现的假绿。
  store.pairSucceeded();
  store.apply({ kind: "authenticating" });
  store.apply({ kind: "replaying", cursor: null });
  store.apply({ kind: "online", cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" } });
  return store;
}

describe("R17：上下文三态经 ClientStore 投影后仍可区分", () => {
  it("从未收到用量事件：呈现未上报，渲染结果里没有任何数字", () => {
    const store = storeWithSnapshot();
    const page = store.conversationPage(SESSION_ID);
    expect(page).not.toBeNull();
    expect(page?.usage.kind).toBe("unreported");

    const html = renderToStaticMarkup(createElement(ContextUsageBar, { usage: requiredUsage(page) }));
    // 反例：任何「以零冒充」的渲染都会在这里出现 0 / 0% → 断言变红。
    expect(html).not.toContain("0%");
    expect(html).not.toContain("data-context-used");
    expect(html).not.toContain("data-context-size");
    expect(html).not.toContain("NaN");
    expect(html).not.toContain("Infinity");
    // 整段 HTML 不含任何阿拉伯数字：未上报态没有数字槽位。
    expect(/[0-9]/.test(html.replace(/data-context-state="unreported"/u, ""))).toBe(false);
  });

  it("窗口大小为零：同样呈现未上报，但与「从未上报」可区分", () => {
    const store = storeWithSnapshot();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.usage.changed",
        eventId: "dddddddd-0000-4000-8000-000000000009",
        sessionId: SESSION_ID,
        globalSequence: "11",
        view: { used: "1234", size: "0" },
      }),
    });

    const page = store.conversationPage(SESSION_ID);
    expect(page?.usage.kind).toBe("zero_window");
    // 与第一态不同：判别联合的 `kind` 不相等 → 组件会走另一个分支。
    expect(page?.usage.kind).not.toBe("unreported");

    const html = renderToStaticMarkup(createElement(ContextUsageBar, { usage: requiredUsage(page) }));
    // 反例：若零窗口被算成占比，这里会出现 Infinity% 或 NaN% → 断言变红。
    expect(html).not.toContain("Infinity");
    expect(html).not.toContain("NaN");
    expect(html).not.toContain("0%");
    expect(html).not.toContain("data-context-percent");
  });

  it("已知用量：呈现已用、上限与占比（对照：前两条不是因为组件永远不出数字）", () => {
    const store = storeWithSnapshot();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.usage.changed",
        eventId: "dddddddd-0000-4000-8000-00000000000a",
        sessionId: SESSION_ID,
        globalSequence: "12",
        view: { used: "2500", size: "10000" },
      }),
    });

    const page = store.conversationPage(SESSION_ID);
    expect(page?.usage.kind).toBe("known");
    const html = renderToStaticMarkup(createElement(ContextUsageBar, { usage: requiredUsage(page) }));
    expect(html).toContain("25.0%");
    expect(html).toContain("2500");
    expect(html).toContain("10000");
  });

  it("三态的 kind 互不相同，且没有任何一态的占比是非有限数", () => {
    const states = [null, { used: "1", size: "0" }, { used: "1", size: "4" }] as const;
    const kinds = states.map((state) => buildContextUsageView(state).kind);
    // 反例：若三态塌缩成两种，Set 的大小会是 2 → 断言变红。
    expect(new Set(kinds).size).toBe(3);
    for (const state of states) {
      const view = buildContextUsageView(state);
      if (view.kind === "known") {
        expect(Number.isFinite(view.percent)).toBe(true);
      } else {
        expect("percent" in view).toBe(false);
      }
    }
  });

  it("未上报不影响其它交互：历史、发送、取消仍可用", () => {
    const store = storeWithSnapshot();
    const page = store.conversationPage(SESSION_ID);
    // 会话处于 running（夹具默认），因此可发送、可取消；用量状态与之无关。
    expect(page?.usage.kind).toBe("unreported");
    expect(page?.capabilities.canViewHistory).toBe(true);
    expect(page?.capabilities.canCancel).toBe(true);
    expect(page?.capabilities.canSend).toBe(true);

    // 窗口为零时同样不禁用任何交互。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.usage.changed",
        eventId: "dddddddd-0000-4000-8000-00000000000b",
        sessionId: SESSION_ID,
        globalSequence: "13",
        view: { used: "0", size: "0" },
      }),
    });
    const zeroWindow = store.conversationPage(SESSION_ID);
    expect(zeroWindow?.usage.kind).toBe("zero_window");
    expect(zeroWindow?.capabilities).toEqual(page?.capabilities);
  });

  it("未上报态不推算：客户端不产生任何自造的用量数字", () => {
    const store = storeWithSnapshot();
    // 仓库的 usage 映射里根本没有这个会话的键 → 「从未收到」由**缺席**表达。
    expect(store.state.usage.has(SESSION_ID)).toBe(false);
    expect(store.conversationPage(SESSION_ID)?.usage).toEqual({ kind: "unreported" });
  });
});

describe("R19：覆盖层缺席经 ClientStore 后仍是状态未知", () => {
  it("无任何 Agent 连接事件时全部条目为 unknown，列表不缩减", () => {
    const store = storeWithSnapshot();
    const catalog = store.agentCatalog();

    // 反例：若把缺席当作已断开，这里会是 "disconnected" → 断言变红。
    expect(catalog.map((entry) => entry.connection)).toEqual(["unknown", "unknown"]);
    // 反例：若因覆盖层缺席而移除 Agent，长度会是 0 → 断言变红。
    expect(catalog).toHaveLength(2);
    expect(store.state.overlay).toHaveLength(0);
  });

  it("主机面板在覆盖层缺席时说明「状态未知」而不是「已断开」", () => {
    const store = storeWithSnapshot();
    const panel = store.hostPanel();

    expect(panel.overlayPresent).toBe(false);
    // 说明文案必须与「已断开」明确不同：缺席不是断开。
    expect(panel.unknownStateNote).not.toBeNull();
    expect(panel.unknownStateNote).toContain("未知");
    // 文案里的「不等于已断开」是**否定式**说明；逐行的连接标签才是真正的呈现口径。
    expect(panel.agents.map((row) => row.connectionLabel)).toEqual(["状态未知", "状态未知"]);
    // 逐行也不得出现 disconnected。
    expect(panel.agents.map((row) => row.connection)).toEqual(["unknown", "unknown"]);
  });

  it("会话全部运行中也不得被推断成「已连接」", () => {
    const store = storeWithSnapshot();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        eventId: "dddddddd-0000-4000-8000-00000000000c",
        globalSequence: "14",
        view: {
          session: {
            sessionId: SESSION_ID,
            agent: { agentId: AGENT_A, name: "codex" },
            state: "running",
            origin: { kind: "local" },
            currentMode: null,
            version: "3",
            createdAt: "2026-10-01T00:00:00.000Z",
            updatedAt: "2026-10-01T00:00:05.000Z",
          },
        },
      }),
    });

    // 反例：若实现从 `session.agent` 或 `state === "running"` 推断连接状态 → 这里会红。
    expect(store.state.overlay).toHaveLength(0);
    expect(store.agentCatalog().map((entry) => entry.connection)).toEqual(["unknown", "unknown"]);
  });

  it("收到 agent.connected 后只覆盖对应条目，其余保持未知", () => {
    const store = storeWithSnapshot();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.connected",
        eventId: "dddddddd-0000-4000-8000-00000000000d",
        globalSequence: "15",
        view: { agentId: AGENT_A, state: "connected" },
      }),
    });

    expect(store.agentCatalog().map((entry) => [entry.id, entry.connection])).toEqual([
      [AGENT_A, "connected"],
      [AGENT_B, "unknown"],
    ]);
    // 覆盖层存在后，「状态未知」的整体说明不再出现。
    const panel = store.hostPanel();
    expect(panel.overlayPresent).toBe(true);
    expect(panel.unknownStateNote).toBeNull();
  });

  it("同名 Agent 以标识区分，状态互不覆盖", () => {
    const gateway = recordingGateway({
      resources: resources({
        agents: [
          { agentId: AGENT_A, displayName: "codex", default: true },
          { agentId: AGENT_B, displayName: "codex", default: false },
        ],
      }),
    });
    let counter = 0;
    const store = new ClientStore({
      sync: gateway,
      ids: {
        uuid: (): string => {
          counter += 1;
          return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
        },
      },
      now: () => 1_700_000_000_000,
      identity: { connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637" },
    });
    store.apply({
      kind: "snapshot_verified",
      snapshot: {
        ...(gateway.current as NonNullable<typeof gateway.current>),
        chunkCount: 3,
        receivedChunks: 3,
        snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
      },
    });

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.connected",
        eventId: "dddddddd-0000-4000-8000-00000000000e",
        globalSequence: "16",
        view: { agentId: AGENT_A, state: "connected" },
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.disconnected",
        eventId: "dddddddd-0000-4000-8000-00000000000f",
        globalSequence: "17",
        view: {
          agentId: AGENT_B,
          state: "disconnected",
          error: { code: "capability.unsupported_by_agent", message: "运行时退出", retryable: false, details: {} },
        },
      }),
    });

    const catalog = store.agentCatalog();
    // 反例：若按展示名合并，两行会塌成一行 → 断言变红。
    expect(catalog).toHaveLength(2);
    expect(catalog.map((entry) => [entry.id, entry.displayName, entry.connection])).toEqual([
      [AGENT_A, "codex", "connected"],
      [AGENT_B, "codex", "disconnected"],
    ]);
    // 断开原因按公开错误呈现，未连接的条目没有原因。
    expect(store.hostPanel().agents.map((row) => row.disconnectReason)).toEqual([null, "运行时退出"]);
  });

  it("同一事件重投不污染覆盖层（按 eventId 去重的下游表现）", () => {
    const store = storeWithSnapshot();
    const event = eventMessage({
      eventType: "agent.connected",
      eventId: "dddddddd-0000-4000-8000-000000000010",
      globalSequence: "18",
      view: { agentId: AGENT_A, state: "connected" },
    });
    store.apply({ kind: "event", event });
    store.apply({ kind: "event", event });
    // 反例：若覆盖层按事件条数累积，这里会是 2 条 → 断言变红。
    expect(store.state.overlay).toHaveLength(1);
  });

  it("断连时主机面板仍列出已配置 Agent（列表来自快照，不来自事件）", () => {
    const store = storeWithSnapshot();
    store.apply({ kind: "closed", code: 4500, retryable: true });

    const panel = store.hostPanel();
    expect(panel.connectionState).toBe("reconnecting");
    // 反例：若断连时清空列表，这里会是 0 行 → 断言变红。
    expect(panel.agents).toHaveLength(2);
    expect(panel.offlineSnapshotNote).not.toBeNull();
  });
});