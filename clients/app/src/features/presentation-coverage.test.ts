/**
 * D5「呈现覆盖由客户端自持」：覆盖表条目清单 + 「协议已知但覆盖未命中 → 降级卡片」的实测。
 *
 * ## 判别力
 *
 * - 第一条逐条比对协议词表 `KNOWN_EVENT_TYPES` 与「覆盖表 ∪ 未覆盖清单」：漏登记一个类型
 *   （无论落在哪一边）即变红——它钉住的是「每个协议已知类型都必须有明确结论」。
 * - 第二条钉住「走时间线卡片的类型 = WP1 的 17 类历史条目」：若覆盖表漏掉其中一类，
 *   重开对话页时那一类会整片变成降级卡片（design 的风险表第一条）。
 * - 第三条与第四条**经 `ClientStore` 这条生产路径**证明：协议已知但覆盖未命中的事件真的产出
 *   降级卡片（而不是被静默丢掉——那正是本次要拆开 `isKnownEventType` 的原因）。
 * - 第五条钉住**节点级**未覆盖事件（`device.revoked`，没有 `sessionId`）：它没有会话时间线可挂，
 *   必须在主机/连接面（`HostPanelModel.nodeDegraded`）有可到达的降级条目——「被消费却既不呈现
 *   也不降级」是本变更 MUST NOT 的第三种结果。
 * - 两条对照证明断言不是因为「一律降级」而平凡成立。
 */

import { describe, expect, it } from "vitest";

import { KNOWN_EVENT_TYPES, isKnownEventType } from "../protocol";
import {
  PRESENTATION_COVERAGE,
  TIMELINE_CARD_EVENT_TYPES,
  UNCOVERED_EVENT_TYPES,
  hasDedicatedView,
  presentationRouteFor,
} from "./presentation-coverage";
import type { StoreFixture } from "./testing/fixtures";
import { TEST_DEVICE_ID, eventMessage, onlineStore, resources, session } from "./testing/fixtures";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";

/** WP1 的 17 类历史条目（`docs/SYNC_PROTOCOL.md` §10.2.1 与 design D3 的清单）。 */
const HISTORY_ENTRY_TYPES = [
  "turn.queued",
  "turn.started",
  "turn.completed",
  "turn.cancelled",
  "turn.failed",
  "tool.call.started",
  "tool.call.updated",
  "tool.call.completed",
  "permission.requested",
  "permission.resolved",
  "elicitation.requested",
  "elicitation.resolved",
  "terminal.output",
  "file.changed",
  "session.plan.changed",
  "session.mode.changed",
  "session.config.changed",
];

/** 建一个已在线、快照里有目标会话的仓库。 */
function storeWithSession(): StoreFixture {
  return onlineStore({ resources: resources({ sessions: [session({ id: SESSION_ID })] }) });
}

describe("D5 覆盖表：每个协议已知类型都有明确结论", () => {
  it("覆盖表 ∪ 未覆盖清单 == 协议词表（没有第三种结果）", () => {
    const covered: string[] = [];
    const uncovered: string[] = [];
    for (const eventType of KNOWN_EVENT_TYPES) {
      if (hasDedicatedView(eventType)) covered.push(eventType);
      else uncovered.push(eventType);
    }
    // 反例：某个类型两个清单都没登记 / 两个都登记，这里就会变红。
    expect([...covered, ...uncovered].sort()).toEqual([...KNOWN_EVENT_TYPES].sort());
    expect(uncovered.sort()).toEqual([...UNCOVERED_EVENT_TYPES].sort());
    expect(covered.length + uncovered.length).toBe(KNOWN_EVENT_TYPES.length);
  });

  it("时间线卡片通道恰好覆盖 17 类历史条目，其余通道各司其职", () => {
    // 反例：覆盖表漏掉 `terminal.output` 之类的历史类型 → 重开对话页会整片降级。
    expect([...TIMELINE_CARD_EVENT_TYPES].sort()).toEqual([...HISTORY_ENTRY_TYPES].sort());
    // 消息通道承载增量与「增量流结束」标记；压缩事件也在其中（它作废累加内容）。
    expect(presentationRouteFor("agent.message.delta")).toBe("message_lane");
    expect(presentationRouteFor("agent.thought.delta")).toBe("message_lane");
    expect(presentationRouteFor("agent.message.completed")).toBe("message_lane");
    expect(presentationRouteFor("turn.delta_compacted")).toBe("message_lane");
    // 摘要/仪表面、覆盖层、命令终态三面各自承载不该进时间线的事件。
    expect(presentationRouteFor("session.origin.online_changed")).toBe("session_dashboard");
    expect(presentationRouteFor("agent.connected")).toBe("agent_overlay");
    expect(presentationRouteFor("command.uncertain")).toBe("command_status");
    // 覆盖表里没有的类型：任何取值都不会被当成「有视图」。
    expect(presentationRouteFor("future.unknown.event")).toBeNull();
    expect(Object.keys(PRESENTATION_COVERAGE)).toHaveLength(KNOWN_EVENT_TYPES.length - UNCOVERED_EVENT_TYPES.length);
  });

  it("未覆盖清单里的类型都是协议已知、而覆盖表未命中的", () => {
    for (const eventType of UNCOVERED_EVENT_TYPES) {
      expect(isKnownEventType(eventType)).toBe(true);
      expect(hasDedicatedView(eventType)).toBe(false);
      expect(presentationRouteFor(eventType)).toBeNull();
    }
    // 不能反过来把「协议不认识」混进这张清单：那是另一件事（它同样降级，但不是「已知缺口」）。
    expect(UNCOVERED_EVENT_TYPES).not.toContain("future.unknown.event");
  });
});

describe("D5 生产管线：已知但覆盖未命中 → 降级卡片", () => {
  it("已由会话状态层处理的 session.info.changed 不额外插入聊天诊断卡片", () => {
    const { store } = storeWithSession();

    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.info.changed",
        sessionId: SESSION_ID,
        view: { title: "改过的标题", updatedAt: "2026-10-01T00:00:03.000Z" },
      }),
    });

    const page = store.conversationPage(SESSION_ID);
    expect(page?.degraded).toHaveLength(0);
    expect(page?.timeline.entries.map((entry) => entry.kind)).not.toContain("degraded");
  });

  it("多个未覆盖类型各自产出自己的降级卡片（不合并、不覆盖）", () => {
    const { store } = storeWithSession();
    for (const [index, eventType] of UNCOVERED_EVENT_TYPES.entries()) {
      store.apply({
        kind: "event",
        event: eventMessage({
          eventType,
          sessionId: eventType === "device.revoked" ? null : SESSION_ID,
          eventId: `dddddddd-0000-4000-8000-00000000000${index}`,
          globalSequence: String(index + 1),
        }),
      });
    }
    const page = store.conversationPage(SESSION_ID);
    // `device.revoked` 是节点级事件（没有会话），不进任何会话时间线；会话级的两个各一条。
    expect(page?.degraded).toEqual([]);
    // 它也不是「没有出口」：节点级的那一条落在主机/连接面的节点级降级列表上。
    expect(store.nodeDegraded().map((item) => item.eventType)).toEqual(["device.revoked"]);
  });

  it("节点级未覆盖事件在主机/连接面有可到达的降级条目（且可诊断）", () => {
    const { store } = storeWithSession();
    // 两条**故意按到达顺序倒置**：出口必须按时间位置排序，而不是按到达顺序。
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "device.revoked",
        sessionId: null,
        eventId: "dddddddd-0000-4000-8000-0000000000b2",
        globalSequence: "9",
        view: { deviceId: TEST_DEVICE_ID, revokedAt: "2026-10-01T00:00:09.000Z" },
      }),
    });
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "device.revoked",
        sessionId: null,
        eventId: "dddddddd-0000-4000-8000-0000000000b1",
        globalSequence: "7",
        view: { deviceId: TEST_DEVICE_ID, revokedAt: "2026-10-01T00:00:07.000Z" },
      }),
    });

    // 反例：只登记进未覆盖清单却不给出口（旧实现）时这里是 0 条 → 断言变红。
    const node = store.nodeDegraded();
    expect(node.map((item) => item.eventId)).toEqual([
      "dddddddd-0000-4000-8000-0000000000b1",
      "dddddddd-0000-4000-8000-0000000000b2",
    ]);
    expect(node[0]?.reason).toEqual({ kind: "no_dedicated_view" });
    expect(node[0]?.sessionId).toBeNull();
    // 出口接在主机/连接面上（组件如何渲染它留给 WP5）。
    expect(store.hostPanel().nodeDegraded).toEqual(node);
    // 会话时间线与诊断列表都不含节点级条目：它没有会话可挂。
    expect(store.conversationPage(SESSION_ID)?.degraded).toEqual([]);

    const first = node[0];
    if (first === undefined) throw new Error("节点级降级条目应存在");
    store.openDiagnostics(first.eventId);
    expect(store.diagnostics()?.eventType).toBe("device.revoked");
  });

  it("对照：覆盖命中的卡片类型不产生降级卡片", () => {
    const { store } = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "terminal.output",
        sessionId: SESSION_ID,
        view: { terminalId: "t-1", chunkIndex: "0", stream: "stdout", text: "构建中\n", truncated: false },
      }),
    });
    const page = store.conversationPage(SESSION_ID);
    // 反例：若「一律产生降级卡片」，这里会 ≥1 条 → 断言变红。
    expect(page?.degraded).toEqual([]);
    expect(page?.timeline.entries.map((entry) => entry.kind)).toEqual(["terminal_output"]);
  });

  it("对照：协议不认识的类型同样降级，且不被归到三类「不支持」", () => {
    const { store } = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({ eventType: "acp.agent.vendor_status", sessionId: SESSION_ID, view: { phase: "review" } }),
    });
    const page = store.conversationPage(SESSION_ID);
    expect(page?.degraded).toHaveLength(1);
    expect(page?.degraded[0]?.reason.kind).toBe("no_dedicated_view");
  });
});
