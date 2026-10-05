/**
 * R12：连接状态机的**穷举**迁移覆盖（12 态 × 11 事件 = 132 条组合逐条判定）。
 *
 * ## 为什么既有测试不够
 *
 * `connection-machine.test.ts`（WP6）逐条覆盖了「完整路径」「阻断」「追平」等场景，
 * 但没有任何一条断言把 `(state, event)` 的**全部 132 种组合**跑一遍。因此：
 * - 迁移表里**多写**一条不该有的来源（例如让 `revoked` 接受 `connect_started`）不会被发现；
 * - 某条组合本该抛 `IllegalConnectionTransition` 却静默接受，也不会被发现。
 *
 * 本文件用一个**在测试里独立写出的**期望表（不复用生产代码的 `ALLOWED_SOURCES`）逐条比对，
 * 因此「实现与期望表一致」这件事本身是被判定的，而不是自证。
 *
 * ## 判别力
 *
 * 期望表是本文件手写的常量。任何一条迁移被增删，两侧立刻不等 → 用例变红。
 * 反之，若把断言弱化成「不抛错即可」，任何多出来的迁移都会静默通过——因此每条
 * 合法组合都同时断言**目标状态**与**阻断载荷**，每条非法组合都断言异常类型与两个字段。
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
} from "./index";
import type {
  BlockingCause,
  BlockingState,
  ConnectionEvent,
  ConnectionMachineState,
  ConnectionStateName,
} from "./index";

/** 12 个状态（与 `CONNECTION_STATES` 的键一一对应，本文件另立一份以便发现漂移）。 */
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

/** 4 个阻断态。 */
const ALL_BLOCKING: readonly BlockingState[] = ["revoked", "incompatible", "identity_changed", "replaced"];

/** 阻断原因 → 阻断状态（因果名与状态名不是同一组取值）。 */
const CAUSE_BY_STATE: Readonly<Record<BlockingState, BlockingCause>> = {
  revoked: "device_revoked",
  incompatible: "version_incompatible",
  identity_changed: "host_identity_changed",
  replaced: "replaced_by_other_connection",
};

/**
 * 每个事件的合法来源（**手写期望表**，不复用 `ALLOWED_SOURCES`）。
 *
 * 每条都可回溯到 spec 或协议文档：
 * - `pair_started`：只有「尚未持有设备身份」或「已断开 / 阻断」才可能重新配对；
 * - `pair_succeeded`：配对成功后停在 `disconnected`（不直接在线）；
 * - `identity_lost`：本地身份没了，任何持有身份的态都能退到 `unpaired`；
 * - `connect_started`：**不含阻断态**——恢复必须先 `blocking_cleared`；
 * - `socket_opened`：只有已发起连接尝试（`connecting`/`reconnecting`）；
 * - `subscribed`：只有认证成功后；
 * - `caught_up`：**只有 `replaying`**——在线只能由追平完成产生；
 * - `connection_closed`：连接中的四个态；
 * - `disconnect_requested`：任何已建立的态与未配对/已断开；
 * - `blocked`：任何态（含阻断态之间横跳）；
 * - `blocking_cleared`：只从阻断态解除。
 */
const EXPECTED_SOURCES: Readonly<Record<ConnectionEvent["kind"], readonly ConnectionStateName[]>> = {
  pair_started: ["unpaired", "disconnected", "revoked", "identity_changed", "incompatible"],
  pair_succeeded: ["pairing"],
  identity_lost: ["online", "disconnected", "reconnecting", "revoked", "identity_changed"],
  connect_started: ["disconnected", "connecting", "authenticating", "replaying", "online", "reconnecting"],
  socket_opened: ["connecting", "reconnecting"],
  subscribed: ["authenticating"],
  caught_up: ["replaying"],
  connection_closed: ["connecting", "authenticating", "replaying", "online"],
  disconnect_requested: [
    "connecting",
    "authenticating",
    "replaying",
    "online",
    "reconnecting",
    "unpaired",
    "disconnected",
  ],
  blocked: [
    "connecting",
    "authenticating",
    "replaying",
    "online",
    "reconnecting",
    "disconnected",
    "revoked",
    "incompatible",
    "identity_changed",
    "replaced",
  ],
  blocking_cleared: ["revoked", "incompatible", "identity_changed", "replaced"],
};

/** 每个事件的全部取值（`connect_started`/`connection_closed`/`blocked` 带参数）。 */
function everyEvent(kind: ConnectionEvent["kind"]): readonly ConnectionEvent[] {
  switch (kind) {
    case "connect_started":
      return [{ kind, fromReconnect: false }, { kind, fromReconnect: true }];
    case "connection_closed":
      return [{ kind, retryable: true }, { kind, retryable: false }];
    case "blocked":
      return ALL_BLOCKING.map((state) => ({ kind, cause: CAUSE_BY_STATE[state] }) as ConnectionEvent);
    default:
      return [{ kind } as ConnectionEvent];
  }
}

const ALL_EVENT_KINDS: readonly ConnectionEvent["kind"][] = [
  "pair_started",
  "pair_succeeded",
  "identity_lost",
  "connect_started",
  "socket_opened",
  "subscribed",
  "caught_up",
  "connection_closed",
  "disconnect_requested",
  "blocked",
  "blocking_cleared",
];

/** 造一个处于给定状态、且 `blocking` 载荷与状态自洽的机器状态。 */
function machineIn(state: ConnectionStateName): ConnectionMachineState {
  if (ALL_BLOCKING.includes(state as BlockingState)) {
    const blocking = state as BlockingState;
    return {
      state: blocking,
      blocking: { state: blocking, cause: CAUSE_BY_STATE[blocking], reason: "r", nextStep: "n" },
      caughtUp: false,
    };
  }
  return { state, blocking: null, caughtUp: state === "online" };
}

/** 合法来源的完整集合（本文件自己算，不查生产代码）。 */
function expectedSourcesOf(kind: ConnectionEvent["kind"]): ReadonlySet<ConnectionStateName> {
  return new Set(EXPECTED_SOURCES[kind]);
}

describe("R12：迁移表与手写期望表逐条一致（132 种组合穷举）", () => {
  it("状态集合恰好是 12 个，与本文件的枚举无差集", () => {
    expect(Object.keys(CONNECTION_STATES).sort()).toEqual([...ALL_STATES].sort());
    expect(Object.keys(BLOCKING_STATES).sort()).toEqual([...ALL_BLOCKING].sort());
  });

  it("每个事件的合法来源与期望表逐条相等（覆盖 12 态 × 11 事件）", () => {
    const mismatches: string[] = [];
    for (const kind of ALL_EVENT_KINDS) {
      const expected = [...expectedSourcesOf(kind)].sort();
      const actual = [...(allowedSourceStates()[kind] ?? [])].sort();
      if (JSON.stringify(expected) !== JSON.stringify(actual)) {
        mismatches.push(`${kind}: 期望 ${expected.join(",")} 实际 ${actual.join(",")}`);
      }
    }
    // 反例：迁移表里多写或漏写一条来源，这里立刻列出差异 → 断言变红。
    expect(mismatches).toEqual([]);
  });

  it("每条合法组合都真的被接受，且不抛错", () => {
    const rejected: string[] = [];
    for (const kind of ALL_EVENT_KINDS) {
      const sources = expectedSourcesOf(kind);
      for (const state of ALL_STATES) {
        if (!sources.has(state)) continue;
        for (const event of everyEvent(kind)) {
          try {
            transition(machineIn(state), event);
          } catch (error) {
            rejected.push(`${state} --${kind}--> 抛错：${String(error)}`);
          }
        }
      }
    }
    // 反例：期望表说合法、实现说非法（或抛别的错），这里会红。
    expect(rejected).toEqual([]);
  });

  it("每条非法组合都抛 IllegalConnectionTransition，且 from/event 字段正确", () => {
    const accepted: string[] = [];
    const wrongFields: string[] = [];
    for (const kind of ALL_EVENT_KINDS) {
      const sources = expectedSourcesOf(kind);
      for (const state of ALL_STATES) {
        if (sources.has(state)) continue;
        for (const event of everyEvent(kind)) {
          try {
            transition(machineIn(state), event);
            // 反例：非法迁移被静默接受，这里会红。
            accepted.push(`${state} --${kind}-->`);
          } catch (error) {
            if (!(error instanceof IllegalConnectionTransition)) {
              accepted.push(`${state} --${kind}--> 抛的不是 IllegalConnectionTransition`);
              continue;
            }
            if (error.from !== state || error.event !== kind) {
              wrongFields.push(`${state} --${kind}--> from=${error.from} event=${error.event}`);
            }
          }
        }
      }
    }
    expect(accepted).toEqual([]);
    expect(wrongFields).toEqual([]);
  });

  it("非法迁移不产生任何状态变更（异常抛出前状态对象不被就地改写）", () => {
    const before = machineIn("online");
    const snapshot = JSON.stringify(before);
    expect(() => transition(before, { kind: "pair_succeeded" })).toThrow(IllegalConnectionTransition);
    expect(JSON.stringify(before)).toBe(snapshot);
  });
});

describe("R12：迁移后的状态仍是单值枚举（不是可同时成立的布尔组合）", () => {
  it("每条合法迁移的结果都只有一个状态名，且取值落在 12 态内", () => {
    for (const kind of ALL_EVENT_KINDS) {
      const sources = expectedSourcesOf(kind);
      for (const state of ALL_STATES) {
        if (!sources.has(state)) continue;
        for (const event of everyEvent(kind)) {
          const next = transition(machineIn(state), event);
          expect(ALL_STATES).toContain(next.state);
          // 状态字段是**字符串枚举**，不是布尔：`state === true` 这类形状不可能通过。
          expect(typeof next.state).toBe("string");
          // 反例：若实现改成 `{ online: true, connecting: false }` 之类，这里会红。
          expect(Object.keys(next).sort()).toEqual(["blocking", "caughtUp", "state"]);
        }
      }
    }
  });

  it("blocking 载荷只在阻断态非空，且与 state 同名", () => {
    for (const kind of ALL_EVENT_KINDS) {
      const sources = expectedSourcesOf(kind);
      for (const state of ALL_STATES) {
        if (!sources.has(state)) continue;
        for (const event of everyEvent(kind)) {
          const next = transition(machineIn(state), event);
          if (isBlockingState(next.state)) {
            expect(next.blocking?.state).toBe(next.state);
          } else {
            expect(next.blocking).toBeNull();
          }
        }
      }
    }
  });

  it("caughtUp 只在 online 为真：追平期不得谎称已在线", () => {
    for (const kind of ALL_EVENT_KINDS) {
      const sources = expectedSourcesOf(kind);
      for (const state of ALL_STATES) {
        if (!sources.has(state)) continue;
        for (const event of everyEvent(kind)) {
          const next = transition(machineIn(state), event);
          // 反例：若 `caughtUp` 与状态脱钩（例如 replaying 也置真），这里会红。
          expect(next.caughtUp).toBe(next.state === "online");
        }
      }
    }
  });
});

describe("R12：阻断态停止自动重连并携带原因与下一步", () => {
  it("四个阻断态都不自动重连，且都带非空的 reason 与 nextStep", () => {
    for (const blocking of ALL_BLOCKING) {
      const state = machineIn(blocking);
      // 反例：若 `shouldAutoReconnect` 只看 `blocking === null` 而漏了某个态，这里会红。
      expect(shouldAutoReconnect(state)).toBe(false);
      expect(state.blocking).not.toBeNull();
      expect((state.blocking?.reason ?? "").length).toBeGreaterThan(0);
      expect((state.blocking?.nextStep ?? "").length).toBeGreaterThan(0);
    }
  });

  it("由 blocked 事件产生的四个阻断态逐一满足上一条（不只对构造出来的状态成立）", () => {
    for (const blocking of ALL_BLOCKING) {
      const cause = CAUSE_BY_STATE[blocking];
      for (const from of ALL_STATES) {
        const sources = expectedSourcesOf("blocked");
        if (!sources.has(from)) continue;
        const next = transition(machineIn(from), { kind: "blocked", cause });
        expect(next.state).toBe(blocking);
        expect(next.blocking?.cause).toBe(cause);
        expect((next.blocking?.reason ?? "").length).toBeGreaterThan(0);
        expect((next.blocking?.nextStep ?? "").length).toBeGreaterThan(0);
        expect(shouldAutoReconnect(next)).toBe(false);
      }
    }
  });

  it("非阻断态中只有 unpaired 不自动重连（其余 7 个常规态都应重连）", () => {
    const auto: ConnectionStateName[] = [];
    for (const state of ALL_STATES) {
      if (shouldAutoReconnect(machineIn(state))) auto.push(state);
    }
    // 反例：若实现把 `disconnected` 也判成不重连（或把 `replaying` 判成不重连），这里会红。
    expect(auto.sort()).toEqual(
      ["pairing", "disconnected", "connecting", "authenticating", "replaying", "online", "reconnecting"].sort(),
    );
  });

  it("blocking_cleared 回到 disconnected 且载荷清空：恢复不得直接跳到在线", () => {
    for (const blocking of ALL_BLOCKING) {
      const next = transition(machineIn(blocking), { kind: "blocking_cleared" });
      expect(next.state).toBe("disconnected");
      expect(next.blocking).toBeNull();
      expect(next.caughtUp).toBe(false);
      // 反例：若解除后直接置 online，这里会红。
      expect(canMarkInputAsSent(next)).toBe(false);
    }
  });

  it("阻断态不得直接 connect_started：恢复必须先 blocking_cleared", () => {
    for (const blocking of ALL_BLOCKING) {
      expect(() => transition(machineIn(blocking), { kind: "connect_started", fromReconnect: true })).toThrow(
        IllegalConnectionTransition,
      );
    }
  });
});

describe("R12：离线/重连态的用户输入不得被标记为已发送或已排队", () => {
  it("只有 online 允许标记已发送（其余 11 态一律不允许）", () => {
    const allowed: ConnectionStateName[] = [];
    for (const state of ALL_STATES) {
      if (canMarkInputAsSent(machineIn(state))) allowed.push(state);
    }
    // 反例：若把 `replaying`/`reconnecting` 也算成可标记，这里会红。
    expect(allowed).toEqual(["online"]);
  });

  it("迁移到 online 之前，输入始终不可标记；追平完成才可", () => {
    let state = initialConnectionState();
    const seen: Array<{ readonly state: ConnectionStateName; readonly mayMark: boolean }> = [];
    const path: readonly ConnectionEvent[] = [
      { kind: "pair_started" },
      { kind: "pair_succeeded" },
      { kind: "connect_started", fromReconnect: false },
      { kind: "socket_opened" },
      { kind: "subscribed" },
    ];
    for (const event of path) {
      state = transition(state, event);
      seen.push({ state: state.state, mayMark: canMarkInputAsSent(state) });
    }
    // 反例：若 `subscribed` 之后即允许标记已发送，最后一项会是 true → 断言变红。
    expect(seen).toEqual([
      { state: "pairing", mayMark: false },
      { state: "disconnected", mayMark: false },
      { state: "connecting", mayMark: false },
      { state: "authenticating", mayMark: false },
      { state: "replaying", mayMark: false },
    ]);

    const online = transition(state, { kind: "caught_up" });
    expect(online.state).toBe("online");
    expect(canMarkInputAsSent(online)).toBe(true);
  });

  it("断线后立即退回不可标记：reconnecting 与 disconnected 都不得标记", () => {
    let state = transition(initialConnectionState(), { kind: "pair_started" });
    state = transition(state, { kind: "pair_succeeded" });
    state = transition(state, { kind: "connect_started", fromReconnect: false });
    state = transition(state, { kind: "socket_opened" });
    state = transition(state, { kind: "subscribed" });
    state = transition(state, { kind: "caught_up" });
    expect(canMarkInputAsSent(state)).toBe(true);

    for (const retryable of [true, false]) {
      const closed = transition(state, { kind: "connection_closed", retryable });
      expect(canMarkInputAsSent(closed)).toBe(false);
      expect(closed.caughtUp).toBe(false);
    }
  });
});

describe("R12：可达性与不可达状态（判别力自查）", () => {
  it("12 个状态都从 unpaired 可达（否则迁移表里有死状态）", () => {
    // 广度优先：只沿期望表里的合法边走，看能到达哪些状态。
    const reachable = new Set<ConnectionStateName>(["unpaired"]);
    const queue: ConnectionStateName[] = ["unpaired"];
    while (queue.length > 0) {
      const current = queue.shift() as ConnectionStateName;
      for (const kind of ALL_EVENT_KINDS) {
        if (!expectedSourcesOf(kind).has(current)) continue;
        for (const event of everyEvent(kind)) {
          const next = transition(machineIn(current), event).state;
          if (!reachable.has(next)) {
            reachable.add(next);
            queue.push(next);
          }
        }
      }
    }
    // 反例：某个状态从任何路径都到不了 → 这里会红。
    expect([...reachable].sort()).toEqual([...ALL_STATES].sort());
  });

  it("初态是 unpaired、无阻断载荷、未追平", () => {
    const initial = initialConnectionState();
    expect(initial.state).toBe("unpaired");
    expect(initial.blocking).toBeNull();
    expect(initial.caughtUp).toBe(false);
  });
});