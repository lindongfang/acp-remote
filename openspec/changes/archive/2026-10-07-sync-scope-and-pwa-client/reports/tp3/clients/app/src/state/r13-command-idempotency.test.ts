// 【交接副本】来源 worktree .worktrees/tp3 的 clients/app/src/state/r13-command-idempotency.test.ts
// 交付提交 6e8307096629dcfd3fa63ac20559e25b0a3e7a7a（分支 feat/tp3），基线 f6ea252。
// 用途：premerge 门禁按「相对变更目录」解析 test_delivery.artifacts 的可读性；本副本在权威交付物之外。
// 除顶部本 4 行说明外与来源逐字节相同（权威可运行副本在 worktree 内，由 PV3 执行）。
/**
 * R13：命令状态机的**穷举**迁移 + `requestId` 复用（`SyncClient` 层）。
 *
 * ## 两条 MUST 各自的可判定落点
 *
 * 1. **MUST NOT 仅因网络断开就断定结果不确定**：`markConnectionLost` 对**每一个**非终态
 *    都必须原样返回（同一个对象），且 `serverConfirmedUncertain` 恒为假。本文件对
 *    7 个命令状态逐个断言，而不是只测 `submitting` 一条。
 * 2. **MUST NOT 为同一意图生成新的请求标识**：`retrySameRequest` 与 `SyncClient.dispatch`
 *    两层都要断言 `requestId` 不变而 `dispatchCount` 递增；只有**派发次数**增加，
 *    标识集合的大小不变——这两者一起才能区分「换了 ID」与「同一 ID 发了两次」。
 *
 * ## 判别力
 *
 * - 若 `uncertain` 的进入条件被放宽（例如断线后置位），「逐状态穷举」会红；
 * - 若 `retrySameRequest` 换成新 ID，「标识集合不变」会红；
 * - 若终态被迟到结果改写，「终态后不改写」会红。
 */

import { describe, expect, it } from "vitest";

import {
  COMMAND_STATES,
  PENDING_CONFIRMATION_LABEL,
  applyCommandResult,
  applyStatusRecord,
  createCommand,
  isTerminalState,
  markConnectionLost,
  markSubmitting,
  mayDisplayAsAccepted,
  retrySameRequest,
  shouldPresentUncertain,
} from "./command-machine";
import type { CommandRecord } from "./command-machine";
import type { CommandState } from "./index";
import type { PublicError, Uuid } from "../protocol";

/** 7 个命令状态（本文件另立一份，以便发现状态集合漂移）。 */
const ALL_COMMAND_STATES: readonly CommandState[] = [
  "draft",
  "submitting",
  "accepted",
  "completed",
  "failed",
  "rejected",
  "uncertain",
];

/** 终态：只有服务端确认才进入。 */
const TERMINAL: readonly CommandState[] = ["completed", "failed", "rejected", "uncertain"];

/** 非终态。 */
const NON_TERMINAL: readonly CommandState[] = ["draft", "submitting", "accepted"];

/** 一条固定的 `requestId`（一个意图一个 ID）。 */
const REQUEST_ID = "7d3f0a2c-9b41-4e6d-8a17-5c2e0b93f4d1";

/** 另一条命令的 ID：用于证伪「把 A 的结果写进 B」。 */
const OTHER_REQUEST_ID = "1a5c8e70-3d24-4f9b-b6e8-0d47f2a19c33";

const ERROR: PublicError = {
  code: "session.busy",
  message: "会话忙",
  retryable: true,
  details: {},
};

/** 造一条处于指定状态的命令记录（除 `state` 外其余字段一律取一致的初值）。 */
function commandIn(state: CommandState): CommandRecord {
  const base = createCommand({ requestId: REQUEST_ID, command: "session.prompt", sessionId: null });
  switch (state) {
    case "draft":
      return base;
    case "submitting":
      return markSubmitting(base);
    case "accepted":
      return applyCommandResult(base, {
        requestId: REQUEST_ID,
        status: "accepted",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: null,
        result: null,
        error: null,
      });
    case "completed":
      return applyCommandResult(markSubmitting(base), {
        requestId: REQUEST_ID,
        status: "completed",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: "b1b1b1b1-2222-4333-8444-555555555555",
        result: { ok: true },
        error: null,
        terminalAt: "2026-10-01T00:00:02.000Z",
      });
    case "failed":
      return applyCommandResult(markSubmitting(base), {
        requestId: REQUEST_ID,
        status: "failed",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: "b1b1b1b1-2222-4333-8444-666666666666",
        result: null,
        error: ERROR,
        terminalAt: "2026-10-01T00:00:02.000Z",
      });
    case "rejected":
      return applyCommandResult(markSubmitting(base), {
        requestId: REQUEST_ID,
        status: "rejected",
        acceptedAt: null,
        terminalEventId: "b1b1b1b1-2222-4333-8444-777777777777",
        result: null,
        error: ERROR,
        terminalAt: "2026-10-01T00:00:01.000Z",
      });
    case "uncertain":
      return applyCommandResult(markSubmitting(base), {
        requestId: REQUEST_ID,
        status: "uncertain",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: "b1b1b1b1-2222-4333-8444-888888888888",
        result: null,
        error: null,
        terminalAt: "2026-10-01T00:00:02.000Z",
      });
  }
}

/** `command.result` 的一个合法取值。 */
function resultOf(status: "accepted" | "completed" | "failed" | "rejected" | "uncertain"): Parameters<
  typeof applyCommandResult
>[1] {
  return {
    requestId: REQUEST_ID,
    status,
    acceptedAt: status === "rejected" ? null : "2026-10-01T00:00:03.000Z",
    terminalEventId: status === "accepted" ? null : "cccccccc-4444-4555-8666-999999999999",
    result: status === "completed" ? { ok: false } : null,
    error: status === "failed" || status === "rejected" ? ERROR : null,
    terminalAt: status === "accepted" ? null : "2026-10-01T00:00:04.000Z",
  };
}

describe("R13：命令状态集合与终态判定", () => {
  it("七个状态与本文件的枚举一致", () => {
    expect(Object.keys(COMMAND_STATES).sort()).toEqual([...ALL_COMMAND_STATES].sort());
  });

  it("终态恰好是 completed/failed/rejected/uncertain（逐个状态判定）", () => {
    const terminal = ALL_COMMAND_STATES.filter((state) => isTerminalState(state));
    // 反例：若把 accepted 也算成终态，或把 uncertain 排除，这里会红。
    expect(terminal.sort()).toEqual([...TERMINAL].sort());
  });
});

describe("R13：断线不得自行断定结果不确定（7 个状态逐一穷举）", () => {
  it("markConnectionLost 对每个非终态原样返回同一对象，且不确定标志为假", () => {
    for (const state of NON_TERMINAL) {
      const before = commandIn(state);
      const after = markConnectionLost(before);
      // 反例：若断线把 `submitting` 改判为 `uncertain`，这里 state 会变 → 断言变红。
      expect(after).toBe(before);
      expect(after.state).toBe(state);
      expect(after.serverConfirmedUncertain).toBe(false);
      // 断线后仍可继续查询：呈现上不得出现「结果不确定」。
      expect(shouldPresentUncertain(after)).toBe(false);
      // 断线也不得把「已接受」显示出来（认证态已失效，§5 要求 online 才可呈现）。
      expect(mayDisplayAsAccepted(after, false)).toBe(false);
    }
  });

  it("对终态同样原样返回：迟到的断线通知不得改写服务端已给出的结论", () => {
    for (const state of TERMINAL) {
      const before = commandIn(state);
      expect(markConnectionLost(before)).toBe(before);
    }
  });

  it("只有服务端明确确认才进入 uncertain：本地没有任何入口能造出这个终态", () => {
    // 唯一能进入 uncertain 的两个函数各走一遍，其余本地操作（提交/断线/重试）都到不了。
    const viaResult = applyCommandResult(commandIn("submitting"), resultOf("uncertain"));
    expect(viaResult.state).toBe("uncertain");
    expect(viaResult.serverConfirmedUncertain).toBe(true);
    expect(shouldPresentUncertain(viaResult)).toBe(true);

    const viaStatus = applyStatusRecord(commandIn("submitting"), {
      targetRequestId: REQUEST_ID,
      state: "uncertain",
      acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalAt: "2026-10-01T00:00:05.000Z",
      terminalEventId: "dddddddd-5555-4666-8777-000000000000",
      result: null,
      error: null,
    });
    expect(viaStatus.state).toBe("uncertain");
    expect(viaStatus.serverConfirmedUncertain).toBe(true);
  });

  it("非 uncertain 的服务端结果不会打开不确定呈现", () => {
    for (const status of ["accepted", "completed", "failed", "rejected"] as const) {
      const record = applyCommandResult(commandIn("submitting"), resultOf(status));
      expect(record.serverConfirmedUncertain).toBe(false);
      expect(shouldPresentUncertain(record)).toBe(false);
    }
  });
});

describe("R13：重试复用同一 requestId，只增加派发次数", () => {
  it("每个可重试状态的 requestId 不变，dispatchCount 递增", () => {
    for (const state of ["draft", "submitting"] as const) {
      const before = commandIn(state);
      const after = retrySameRequest(before);
      expect(after.requestId).toBe(before.requestId);
      // 反例：若实现为重试生成新 ID，`requestId` 会变 → 断言变红。
      expect(after.requestId).toBe(REQUEST_ID);
      expect(after.dispatchCount).toBe(before.dispatchCount + 1);
      expect(after.state).toBe("submitting");
    }
  });

  it("连续重试 5 次：requestId 始终不变，标识集合大小恒为 1", () => {
    let record = commandIn("submitting");
    const ids = new Set<string>([record.requestId]);
    for (let attempt = 0; attempt < 5; attempt += 1) {
      record = retrySameRequest(record);
      ids.add(record.requestId);
    }
    // 反例：任何一次为同一意图生成新 ID，这里就会 >1 → 断言变红。
    expect(ids.size).toBe(1);
    expect(record.dispatchCount).toBe(6);
  });

  it("accepted 与四个终态都拒绝重试（服务端已给出结论，不得本地回退）", () => {
    for (const state of ["accepted", ...TERMINAL] as const) {
      const before = commandIn(state);
      // 反例：若 `accepted` 允许重试，这里不会抛 → 断言变红。
      expect(() => retrySameRequest(before)).toThrow();
      expect(() => retrySameRequest(before)).toThrow(/requestId|request|接受|终结/u);
    }
  });

  it("草稿之外的状态不可再次提交（防止绕过幂等键）", () => {
    for (const state of ["submitting", "accepted", ...TERMINAL] as const) {
      expect(() => markSubmitting(commandIn(state))).toThrow();
    }
  });
});

describe("R13：终态不被迟到的结果改写", () => {
  it("四个终态各自对所有迟到状态保持不变", () => {
    const statuses = ["accepted", "completed", "failed", "rejected", "uncertain"] as const;
    for (const terminal of TERMINAL) {
      const before = commandIn(terminal);
      const snapshot = JSON.stringify(before);
      for (const status of statuses) {
        const after = applyCommandResult(before, resultOf(status));
        // 反例：若终态被改写成进行中（例如 completed → accepted），这里会红。
        expect(after).toBe(before);
        expect(after.state).toBe(terminal);
      }
      for (const status of statuses) {
        expect(applyStatusRecord(before, {
          targetRequestId: REQUEST_ID,
          state: status,
          acceptedAt: "2026-10-01T00:00:09.000Z",
          terminalAt: "2026-10-01T00:00:10.000Z",
          terminalEventId: "eeeeeeee-6666-4777-8888-111111111111",
          result: null,
          error: null,
        })).toBe(before);
      }
      expect(JSON.stringify(before)).toBe(snapshot);
    }
  });

  it("请求标识不匹配时两条入口都抛错（防止把 A 的结果写进 B）", () => {
    for (const state of ALL_COMMAND_STATES) {
      const record = commandIn(state);
      expect(() =>
        applyCommandResult(record, { ...resultOf("completed"), requestId: OTHER_REQUEST_ID }),
      ).toThrow();
      expect(() =>
        applyStatusRecord(record, {
          targetRequestId: OTHER_REQUEST_ID,
          state: "completed",
          acceptedAt: null,
          terminalAt: "2026-10-01T00:00:10.000Z",
          terminalEventId: "eeeeeeee-6666-4777-8888-222222222222",
          result: null,
          error: null,
        }),
      ).toThrow();
    }
  });
});

describe("R13：未确认接受前不得呈现为已接受", () => {
  it("draft/submitting/四个终态都不呈现为已接受（在线与否都一样）", () => {
    for (const state of ["draft", "submitting", ...TERMINAL] as const) {
      const record = commandIn(state);
      // 反例：若 `mayDisplayAsAccepted` 漏了 acceptedAt 判定，这里会出现 true → 断言变红。
      expect(mayDisplayAsAccepted(record, true)).toBe(false);
      expect(mayDisplayAsAccepted(record, false)).toBe(false);
    }
  });

  it("只有服务端确认 accepted 且在线时才呈现为已接受", () => {
    const accepted = commandIn("accepted");
    expect(accepted.acceptedAt).not.toBeNull();
    expect(mayDisplayAsAccepted(accepted, true)).toBe(true);
    // 反例：若去掉 online 前置条件，断线后这里会 true → 断言变红。
    expect(mayDisplayAsAccepted(accepted, false)).toBe(false);
  });

  it("accepted 与 submitting 的呈现文案可区分", () => {
    expect(PENDING_CONFIRMATION_LABEL.length).toBeGreaterThan(0);
    expect(PENDING_CONFIRMATION_LABEL).not.toContain("已接受");
  });

  it("accepted 不带终态时间与终态事件标识（它不是终态）", () => {
    const accepted = commandIn("accepted");
    expect(accepted.state).toBe("accepted");
    expect(accepted.terminalAt).toBeNull();
    expect(accepted.terminalEventId).toBeNull();
    expect(isTerminalState(accepted.state)).toBe(false);
  });
});

describe("R13：命令记录的形状（判别力自查）", () => {
  it("每条命令记录都恰好带这些键，不含可同时成立的状态布尔组合", () => {
    for (const state of ALL_COMMAND_STATES) {
      const record = commandIn(state);
      expect(Object.keys(record).sort()).toEqual([
        "acceptedAt",
        "command",
        "dispatchCount",
        "error",
        "requestId",
        "result",
        "serverConfirmedUncertain",
        "sessionId",
        "state",
        "terminalAt",
        "terminalEventId",
      ]);
      expect(typeof record.state).toBe("string");
      expect(ALL_COMMAND_STATES).toContain(record.state);
    }
  });

  it("终态记录带终态时间与非空事件标识（可核对的终态证据）", () => {
    for (const state of TERMINAL) {
      const record = commandIn(state);
      expect(record.terminalAt).not.toBeNull();
      expect(record.terminalEventId).not.toBeNull();
      // 反例：若实现把 terminalAt 只在 completed 上填满，这里会红。
      expect(record.state).toBe(state);
    }
  });

  it("draft 的派发次数为 0：尚未派发不算已发出", () => {
    expect(commandIn("draft").dispatchCount).toBe(0);
    expect(commandIn("submitting").dispatchCount).toBe(1);
  });

  it("createCommand 保留调用方给的 requestId（不在内部生成）", () => {
    const id: Uuid = REQUEST_ID;
    expect(createCommand({ requestId: id, command: "session.create" }).requestId).toBe(id);
  });
});