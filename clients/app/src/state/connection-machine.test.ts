/**
 * R12：连接互斥状态机的迁移判别力。
 *
 * 每个用例的「反例」就是一个具体的错误实现：
 * - 用布尔组合表达状态 → 「同一时刻只有一个状态」用例变红；
 * - 阻断态仍自动重连 → 「阻断态停止自动重连」用例变红；
 * - `replaying` 直接当在线 → 「追平期不是在线」用例变红；
 * - 阻断态不说明下一步 → 「每个阻断态都有原因与下一步」用例变红；
 * - 静默接受非法迁移 → 「非法迁移抛错」用例变红。
 */

import { describe, expect, it } from "vitest";

import {
  BLOCKING_STATES,
  CONNECTION_STATES,
  IllegalConnectionTransition,
  allowedSourceStates,
  canMarkInputAsSent,
  initialConnectionState,
  isBlockingState,
  shouldAutoReconnect,
  transition,
} from "./connection-machine";
import type { BlockingCause, BlockingState, ConnectionEvent, ConnectionMachineState, ConnectionStateName } from "./connection-machine";
import { BLOCKING_CONNECTION_STATES } from "./index";

/** 12 个状态全集（与 spec 枚举逐字对应）。 */
const ALL_STATES: readonly ConnectionStateName[] = [
  "unpaired",
  "pairing",
  "disconnected",
  "connecting",
  "authenticating",
  "replaying",
  "online",
  "reconnecting",
  "revoked",
  "incompatible",
  "identity_changed",
  "replaced",
];

/** 走一条事件序列。 */
function run(start: ConnectionMachineState, events: readonly ConnectionEvent[]): ConnectionMachineState {
  return events.reduce(transition, start);
}

describe("R12：状态集合是 12 个互斥态", () => {
  it("封闭清单恰好覆盖 spec 枚举的 12 态", () => {
    expect(Object.keys(CONNECTION_STATES).sort()).toEqual([...ALL_STATES].sort());
    expect(Object.keys(CONNECTION_STATES)).toHaveLength(12);
  });

  it("四个阻断态与 src/state/index.ts 的表一致", () => {
    for (const state of ALL_STATES) {
      expect(isBlockingState(state), state).toBe(BLOCKING_CONNECTION_STATES[state] === true);
    }
    expect(Object.keys(BLOCKING_STATES).sort()).toEqual([
      "identity_changed",
      "incompatible",
      "replaced",
      "revoked",
    ]);
  });

  it("任何时刻只有一个状态：机器状态里没有可同时成立的布尔组合", () => {
    const state = initialConnectionState();
    expect(Object.keys(state).sort()).toEqual(["blocking", "caughtUp", "state"]);
    // `state` 是单值；`caughtUp` 由 `state === "online"` 派生，不是独立可写标记。
    expect(state.caughtUp).toBe(false);
    const online = run(state, [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    expect(online.state).toBe("online");
    expect(online.caughtUp).toBe(true);
    expect(online.blocking).toBeNull();
  });
});

describe("R12：常规路径的合法迁移", () => {
  it("完整路径：未配对 → 配对中 → 已断开 → 连接中 → 认证中 → 追平 → 在线", () => {
    const state = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);

    expect(state.state).toBe("online");
    expect(state.caughtUp).toBe(true);
  });

  it("追平期处于 replaying 而不是 online，且未标注追平", () => {
    const state = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
    ]);

    // 反例：若 `subscribed` 直接进 online，本断言变红。
    expect(state.state).toBe("replaying");
    expect(state.state).not.toBe("online");
    // 呈现层据此标注「本地内容尚未追平」。
    expect(state.caughtUp).toBe(false);
  });

  it("断线进入重连中，重连的连接尝试仍记为 reconnecting", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    const reconnecting = transition(online, { kind: "connection_closed", retryable: true });
    expect(reconnecting.state).toBe("reconnecting");

    const again = transition(reconnecting, { kind: "connect_started", fromReconnect: true });
    expect(again.state).toBe("reconnecting");
  });

  it("不可重试的关闭落到已断开，等用户动作", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    expect(transition(online, { kind: "connection_closed", retryable: false }).state).toBe("disconnected");
  });

  it("本地身份丢失回到未配对", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    expect(transition(online, { kind: "identity_lost" }).state).toBe("unpaired");
  });
});

describe("R12：阻断态停止自动重连并说明恢复路径", () => {
  const causes: readonly BlockingCause[] = [
    "device_revoked",
    "version_incompatible",
    "host_identity_changed",
    "replaced_by_other_connection",
  ];

  it("四个阻断原因各自产生对应状态，并携带原因与下一步", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);

    for (const cause of causes) {
      const blocked = transition(online, { kind: "blocked", cause });
      expect(blocked.blocking?.cause).toBe(cause);
      // 反例：若阻断不带说明，UI 只能显示「连不上」→ 断言变红。
      expect(blocked.blocking).not.toBeNull();
      expect(blocked.blocking?.reason.length ?? 0).toBeGreaterThan(0);
      expect(blocked.blocking?.nextStep.length ?? 0).toBeGreaterThan(0);
      // 反例：若阻断态仍自动重连，这里会是 true → 断言变红。
      expect(shouldAutoReconnect(blocked)).toBe(false);
    }
  });

  it("原因与阻断状态一一对应", () => {
    const mapping: Record<BlockingCause, BlockingState> = {
      device_revoked: "revoked",
      version_incompatible: "incompatible",
      host_identity_changed: "identity_changed",
      replaced_by_other_connection: "replaced",
    };
    const paired = run(initialConnectionState(), [{ kind: "pair_started" }, { kind: "pair_succeeded" }]);
    for (const cause of causes) {
      const blocked = transition(paired, { kind: "blocked", cause });
      expect(blocked.state).toBe(mapping[cause]);
      expect(blocked.blocking?.state).toBe(mapping[cause]);
    }
  });

  it("从任意常规态都可进入阻断态（含追平中）", () => {
    const replaying = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
    ]);
    expect(transition(replaying, { kind: "blocked", cause: "device_revoked" }).state).toBe("revoked");
  });

  it("顶替后只能由显式解除回到已断开，不能自动回到在线", () => {
    const paired = run(initialConnectionState(), [{ kind: "pair_started" }, { kind: "pair_succeeded" }]);
    const replaced = transition(paired, { kind: "blocked", cause: "replaced_by_other_connection" });
    const cleared = transition(replaced, { kind: "blocking_cleared" });

    // 反例：若解除阻断直接回 online，两个标签页会立刻再次互相顶替 → 断言变红。
    expect(cleared.state).toBe("disconnected");
    expect(shouldAutoReconnect(cleared)).toBe(true);
    // 必须再走一次握手与追平才能到在线。
    expect(transition(cleared, { kind: "connect_started", fromReconnect: false }).state).toBe("connecting");
  });

  it("未配对时不自动重连", () => {
    expect(shouldAutoReconnect(initialConnectionState())).toBe(false);
  });
});

describe("R12：离线时输入不被标记为已发送", () => {
  it("只有 online 允许把输入标记为已发送或已排队", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    const disconnected = transition(online, { kind: "connection_closed", retryable: true });

    expect(canMarkInputAsSent(online)).toBe(true);
    // 反例：若断线/重连态也允许标记，用户会以为输入已发出 → 断言变红。
    expect(canMarkInputAsSent(disconnected)).toBe(false);
    expect(canMarkInputAsSent(initialConnectionState())).toBe(false);
    expect(canMarkInputAsSent(transition(initialConnectionState(), { kind: "pair_started" }))).toBe(false);
  });

  it("追平期也不允许标记（尚未确认在线）", () => {
    const replaying = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
    ]);
    expect(canMarkInputAsSent(replaying)).toBe(false);
  });
});

describe("R12：迁移表是显式约束", () => {
  it("在线只能由 caught_up 从 replaying 产生", () => {
    const table = allowedSourceStates();
    expect(table.caught_up).toEqual(["replaying"]);
    // 反例：若 `caught_up` 也接受 online 来源，重复到达的 caught_up 会掩盖状态 → 断言变红。
    expect(table.caught_up).not.toContain("online");
  });

  it("socket_opened 只接受 connecting 与 reconnecting", () => {
    expect([...allowedSourceStates().socket_opened].sort()).toEqual(["connecting", "reconnecting"]);
  });

  it("未配对不得直接连接（没有身份就没有可签名的握手）", () => {
    // 反例：若允许 unpaired --connect_started--> connecting，握手必然失败 → 断言变红。
    expect(allowedSourceStates().connect_started).not.toContain("unpaired");
    expect(() => transition(initialConnectionState(), { kind: "connect_started", fromReconnect: false })).toThrow(
      IllegalConnectionTransition,
    );
  });

  it("非法迁移抛错而不是静默接受", () => {
    const online = run(initialConnectionState(), [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
      { kind: "caught_up" },
    ]);
    // 在线时收到第二个 caught_up（重连切换窗口里的迟到消息）。
    expect(() => transition(online, { kind: "caught_up" })).toThrow(IllegalConnectionTransition);
    // 在线时直接进入配对中。
    expect(() => transition(online, { kind: "pair_started" })).toThrow(IllegalConnectionTransition);
  });

  it("迁移表的每个事件都有非空的来源集合（无孤立事件）", () => {
    for (const [kind, sources] of Object.entries(allowedSourceStates())) {
      expect(sources.length, kind).toBeGreaterThan(0);
    }
  });
});

describe("MINOR-1：阻断态不得直接发起连接", () => {
  it("四个阻断态都不接受 connect_started（恢复必须先 blocking_cleared）", () => {
    const sources = allowedSourceStates().connect_started;
    for (const blocking of Object.keys(BLOCKING_STATES) as BlockingState[]) {
      // 反例：若阻断态仍在来源里，调用方发一次 connect_started 就会把 blocking 清成 null，
      // R12 要求携带的原因与下一步被静默丢弃 → 断言变红。
      expect(sources, blocking).not.toContain(blocking);
      const current: ConnectionMachineState = {
        state: blocking,
        blocking: {
          state: blocking,
          cause: "device_revoked",
          reason: "设备已被撤销",
          nextStep: "重新配对",
        },
        caughtUp: false,
      };
      expect(() => transition(current, { kind: "connect_started", fromReconnect: true }), blocking).toThrow(
        IllegalConnectionTransition,
      );
    }
  });

  it("blocking_cleared 之后才可 connect_started", () => {
    const blocked: ConnectionMachineState = {
      state: "replaced",
      blocking: { state: "replaced", cause: "replaced_by_other_connection", reason: "被顶替", nextStep: "手动重连" },
      caughtUp: false,
    };
    expect(transition(transition(blocked, { kind: "blocking_cleared" }), { kind: "connect_started", fromReconnect: false }).state).toBe(
      "connecting",
    );
  });
});