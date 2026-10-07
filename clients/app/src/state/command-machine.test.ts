/**
 * R13：命令状态机的终态收敛判别力。
 *
 * 判别力来源：
 * - 断线即置 `uncertain` → 「连接中断不自行断定」用例变红；
 * - 未确认接受即显示已接受 → 「未确认前不显示已接受」用例变红；
 * - 重试换新 ID → 「重试复用同一 requestId」用例变红；
 * - 把 `command.status` 的新查询 ID 当成新意图 → 「查询指向原 requestId」用例变红；
 * - 终态被后续结果覆盖 → 「终态不可被自动改写」用例变红。
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
import type { Uuid } from "../protocol";
import type { CommandState } from "./index";

const REQUEST_ID: Uuid = "4c4dafda-dd98-442e-8d55-252b75bac72d";

function submitted(): CommandRecord {
  return markSubmitting(createCommand({ requestId: REQUEST_ID, command: "session.prompt" }));
}

function result(
  status: "accepted" | "completed" | "failed" | "rejected" | "uncertain",
): Parameters<typeof applyCommandResult>[1] {
  const rejected = status === "rejected";
  return {
    requestId: REQUEST_ID,
    status,
    acceptedAt: rejected ? null : "2026-10-01T00:00:01.000Z",
    terminalEventId: status === "accepted" ? null : "aaaaaaaa-0000-4000-8000-000000000009",
    result: status === "completed" ? { turnId: "6601828b-3eca-4cec-9a58-18ae1e0a3a14" } : null,
    error: rejected
      ? { code: "session.busy", message: "会话忙", retryable: true, details: {} }
      : status === "failed"
        ? { code: "internal.unavailable", message: "失败", retryable: true, details: {} }
        : null,
  };
}

describe("R13：命令状态集合", () => {
  it("七个状态与 spec 枚举一致", () => {
    expect(Object.keys(COMMAND_STATES).sort()).toEqual(
      ["accepted", "completed", "draft", "failed", "rejected", "submitting", "uncertain"].sort(),
    );
  });

  it("终态是 completed/failed/rejected/uncertain", () => {
    expect(isTerminalState("draft")).toBe(false);
    expect(isTerminalState("submitting")).toBe(false);
    expect(isTerminalState("accepted")).toBe(false);
    for (const state of ["completed", "failed", "rejected", "uncertain"] as const) {
      expect(isTerminalState(state), state).toBe(true);
    }
  });
});

describe("R13：未确认接受前不显示已接受", () => {
  it("提交中既不显示为已接受，也带明确的未确认文案", () => {
    const record = submitted();
    expect(record.state).toBe("submitting");
    expect(record.acceptedAt).toBeNull();
    // 反例：若提交即视为 accepted，这里会是 true → 断言变红。
    expect(mayDisplayAsAccepted(record, true)).toBe(false);
    expect(PENDING_CONFIRMATION_LABEL).toBe("已发出，等待服务器确认");
  });

  it("服务端确认 accepted 后才可显示为已接受（且需在线）", () => {
    const accepted = applyCommandResult(submitted(), result("accepted"));
    expect(accepted.state).toBe("accepted");
    expect(accepted.acceptedAt).toBe("2026-10-01T00:00:01.000Z");
    // 反例：若不看连接状态，断线时也会显示已接受 → 断言变红。
    expect(mayDisplayAsAccepted(accepted, true)).toBe(true);
    expect(mayDisplayAsAccepted(accepted, false)).toBe(false);
  });

  it("重复提交同一 requestId 的结果不会把状态回退", () => {
    const completed = applyCommandResult(submitted(), result("completed"));
    const again = applyCommandResult(completed, result("accepted"));
    // 反例：若终态可被迟到的 accepted 改写，用户会看到完成又变回进行中 → 断言变红。
    expect(again.state).toBe("completed");
  });

  it("结果的 requestId 不匹配时抛错（防止把 A 的结果写进 B）", () => {
    const record = submitted();
    expect(() => applyCommandResult(record, { ...result("accepted"), requestId: "99999999-9999-4999-8999-999999999999" })).toThrow();
  });
});

describe("R13：结果不确定不被自行断定", () => {
  it("连接中断后状态保持 submitting，不进 uncertain", () => {
    const afterLoss = markConnectionLost(submitted());
    // 反例：若断线即置 uncertain，这里会变成 "uncertain" → 断言变红。
    expect(afterLoss.state).toBe("submitting");
    expect(afterLoss.serverConfirmedUncertain).toBe(false);
    expect(shouldPresentUncertain(afterLoss)).toBe(false);
  });

  it("连接中断不改动其它字段（仍在等待服务端结论）", () => {
    const record = submitted();
    const afterLoss = markConnectionLost(record);
    expect(afterLoss).toEqual(record);
  });

  it("只有服务端明确确认 uncertain 才呈现该终态", () => {
    const uncertain = applyCommandResult(submitted(), result("uncertain"));
    expect(uncertain.state).toBe("uncertain");
    expect(uncertain.serverConfirmedUncertain).toBe(true);
    expect(shouldPresentUncertain(uncertain)).toBe(true);
  });

  it("command.status 查询到 uncertain 同样构成服务端确认", () => {
    const queried = applyStatusRecord(submitted(), {
      targetRequestId: REQUEST_ID,
      state: "uncertain",
      acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalAt: "2026-10-01T00:00:09.000Z",
      terminalEventId: "aaaaaaaa-0000-4000-8000-000000000009",
      result: null,
      error: null,
    });
    expect(shouldPresentUncertain(queried)).toBe(true);
  });

  it("服务端确认 uncertain 后不被后续事件自动消解", () => {
    const uncertain = applyCommandResult(submitted(), result("uncertain"));
    // 重连、再次查询都不会让它变回非终态：`retrySameRequest` 对终态抛错。
    expect(() => retrySameRequest(uncertain)).toThrow(/已终结/);
    expect(uncertain.state).toBe("uncertain");
  });

  it("command.status 的 targetRequestId 不匹配时抛错", () => {
    const record = submitted();
    expect(() =>
      applyStatusRecord(record, {
        targetRequestId: "99999999-9999-4999-8999-999999999999",
        state: "completed",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalAt: "2026-10-01T00:00:05.000Z",
        terminalEventId: "aaaaaaaa-0000-4000-8000-000000000009",
        result: null,
        error: null,
      }),
    ).toThrow();
  });
});

describe("R13：重连后以同一标识确认结果", () => {
  it("重试复用同一 requestId，只增加派发次数", () => {
    const first = submitted();
    const retried = retrySameRequest(first);
    // 反例：若重试生成新 ID，这里会不相等 → 断言变红。
    expect(retried.requestId).toBe(REQUEST_ID);
    expect(retried.dispatchCount).toBe(2);
    expect(retried.state).toBe("submitting");
  });

  it("重试后仍可用同一 ID 收到终态", () => {
    const retried = retrySameRequest(submitted());
    const completed = applyCommandResult(retried, result("completed"));
    expect(completed.state).toBe("completed");
    expect(completed.requestId).toBe(REQUEST_ID);
  });

  it("草稿之外的状态不可再次提交（防止绕过幂等键）", () => {
    const accepted = applyCommandResult(submitted(), result("accepted"));
    expect(() => markSubmitting(accepted)).toThrow(/不可再次提交/);
  });

  it("终态不可重试", () => {
    for (const status of ["completed", "failed", "rejected", "uncertain"] as const) {
      const terminal = applyCommandResult(submitted(), result(status));
      // 反例：若终态可重试，同一意图会被再次派发 → 断言变红。
      expect(() => retrySameRequest(terminal), status).toThrow(/已终结/);
    }
  });
});

describe("MINOR-3：已接受的命令不可被本地回退", () => {
  it("accepted 不接受重试（服务端已确认，不退回等待确认）", () => {
    const accepted = applyCommandResult(submitted(), result("accepted"));
    // 反例：若 accepted 可重试，retrySameRequest 会把它变成 submitting、acceptedAt 保留，
    // 而 mayDisplayAsAccepted 变 false → 用户从「已接受」闪回「等待服务器确认」→ 断言变红。
    expect(() => retrySameRequest(accepted)).toThrow(/已被服务器接受/);
    expect(accepted.state).toBe("accepted");
    expect(mayDisplayAsAccepted(accepted, true)).toBe(true);
  });
});

describe("MINOR-4：呈现已接受仍以 online 为前提（FRONTEND_DESIGN §5）", () => {
  it("断线后不显示已接受——设计文档要求只有 online 才可显示", () => {
    const accepted = applyCommandResult(submitted(), result("accepted"));
    const offline = markConnectionLost(accepted);
    // `docs/FRONTEND_DESIGN.md` §5：「只有进入 `online` 后，客户端才能把命令显示为已被服务器接受」。
    // spec 只禁止「确认前显示已接受」，未要求断线后撤回服务端结论——此处的 `false` 由设计文档决定。
    expect(mayDisplayAsAccepted(accepted, true)).toBe(true);
    expect(mayDisplayAsAccepted(offline, false)).toBe(false);
  });
});

describe("R13：各终态的载荷语义", () => {
  it("rejected 不带结果但带结构化错误", () => {
    const rejected = applyCommandResult(submitted(), result("rejected"));
    expect(rejected.state).toBe("rejected");
    expect(rejected.result).toBeNull();
    expect(rejected.error?.code).toBe("session.busy");
    expect(rejected.acceptedAt).toBeNull();
  });

  it("completed 携带结果与终态事件标识", () => {
    const completed = applyCommandResult(submitted(), result("completed"));
    expect(completed.result).toEqual({ turnId: "6601828b-3eca-4cec-9a58-18ae1e0a3a14" });
    expect(completed.terminalEventId).toBe("aaaaaaaa-0000-4000-8000-000000000009");
    expect(completed.error).toBeNull();
  });

  it("accepted 不带终态事件标识", () => {
    const accepted = applyCommandResult(submitted(), result("accepted"));
    expect(accepted.terminalEventId).toBeNull();
    expect(accepted.state).not.toBe("completed");
  });

  it("命令状态与 src/state 的 CommandState 联合一致", () => {
    const every: readonly CommandState[] = Object.keys(COMMAND_STATES) as CommandState[];
    expect(every).toHaveLength(7);
  });
});