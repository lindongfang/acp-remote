/**
 * 命令状态机（R13「命令以稳定标识与终态收敛」）。
 *
 * ## 三条不可让步的规则
 *
 * 1. **`requestId` 在一个意图的生命周期内恒定。** 重连后重试或 `command.status` 查询都用它；
 *    换新 ID 会让服务端把同一条命令当成新意图再派发一次（`docs/SYNC_PROTOCOL.md` §11.1/§11.2）。
 * 2. **只有服务端明确确认才进终态。** 网络断开**不**推出 `uncertain`：连接中断时命令停在
 *    `submitting`，等重连后用同一 ID 查询或重试（spec 场景「结果不确定不被自行断定」）。
 * 3. **未确认接受前不得显示已接受。** `submitting` 的呈现是「已发出，未确认」，
 *    `accepted` 只能由 `command.result {status:"accepted"}` 或 `command.status` 的
 *    `state:"accepted"` 产生（spec 场景「未确认接受前不显示已接受」）。
 *
 * ## 为什么状态是数据而不是散落的回调
 *
 * 命令的生命周期跨连接：提交、连接断开、重连、结果到达可能发生在不同的 socket 上。
 * 把状态收敛成一条不可变记录，重试逻辑就能读「这条命令当前的 `requestId` 与状态」，
 * 而不是试图从 UI 状态反推。
 */

import type { CommandState } from "./index";
import type { PublicError, Uuid } from "../protocol";

/** 命令记录的不可变快照。 */
export interface CommandRecord {
  /** 稳定请求标识：一个意图一个 ID，重试与查询都复用（§11.1）。 */
  readonly requestId: Uuid;
  readonly command: string;
  readonly sessionId: Uuid | null;
  readonly state: CommandState;
  /** 服务端确认被持久化接受的时间；`accepted` 之前恒为 `null`。 */
  readonly acceptedAt: string | null;
  /** 终态时间；非终态恒为 `null`。 */
  readonly terminalAt: string | null;
  /** 该 request 唯一的 command terminal event 的 `eventId`。 */
  readonly terminalEventId: Uuid | null;
  readonly result: Readonly<Record<string, unknown>> | null;
  readonly error: PublicError | null;
  /**
   * 已派发次数（含重试）。
   *
   * 存在的理由是**可核对**：断言「重试没有增加 ID 数量，只增加派发次数」需要这个计数；
   * 没有它就无法区分「换了 ID」与「同一 ID 发了两次」。
   */
  readonly dispatchCount: number;
  /**
   * 结果是否已被服务端明确确认为 `uncertain`。
   *
   * 与 `state` 分开是有意的：`uncertain` 是**服务端的事实**，而 `state` 还包含本地阶段。
   */
  readonly serverConfirmedUncertain: boolean;
}

/** 创建一条草稿命令（尚未派发）。 */
export function createCommand(input: {
  readonly requestId: Uuid;
  readonly command: string;
  readonly sessionId?: Uuid | null;
}): CommandRecord {
  return {
    requestId: input.requestId,
    command: input.command,
    sessionId: input.sessionId ?? null,
    state: "draft",
    acceptedAt: null,
    terminalAt: null,
    terminalEventId: null,
    result: null,
    error: null,
    dispatchCount: 0,
    serverConfirmedUncertain: false,
  };
}

/** 提交中：记录已发出。`requestId` 不变。 */
export function markSubmitting(record: CommandRecord): CommandRecord {
  if (record.state !== "draft") {
    throw new Error(`命令 ${record.requestId} 处于 ${record.state}，不可再次提交`);
  }
  return { ...record, state: "submitting", dispatchCount: record.dispatchCount + 1 };
}

/**
 * 连接中断。
 *
 * **这里刻意不动 `state`**：命令停在 `submitting`（可继续查询），不进入 `uncertain`。
 * spec 场景「结果不确定不被自行断定」直接断言这一点，因此本函数不接受任何
 * 「因为断了所以不确定」的参数——它不存在。
 */
export function markConnectionLost(record: CommandRecord): CommandRecord {
  return record;
}

/** 服务端 `command.result` 到达。 */
export function applyCommandResult(
  record: CommandRecord,
  result: {
    readonly requestId: Uuid;
    readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
    readonly acceptedAt: string | null;
    readonly terminalEventId: Uuid | null;
    readonly result: Readonly<Record<string, unknown>> | null;
    readonly error: PublicError | null;
    readonly terminalAt?: string | null;
  },
): CommandRecord {
  // 幂等键匹配由调用方保证；这里再挡一次，防止把 A 的结果写进 B 的记录。
  if (result.requestId !== record.requestId) {
    throw new Error(`command.result 的 requestId ${result.requestId} 与记录 ${record.requestId} 不符`);
  }
  if (isTerminalState(record.state)) {
    // 终态是服务端已经给出的结论。迟到的消息（例如重试的响应、或重复派发被幂等去重后
    // 回放的 accepted）不得把它改写成进行中——那会让用户看到「已完成」又变回「处理中」。
    return record;
  }
  const next: CommandState =
    result.status === "accepted"
      ? "accepted"
      : result.status === "completed"
        ? "completed"
        : result.status === "failed"
          ? "failed"
          : result.status === "rejected"
            ? "rejected"
            : "uncertain";
  return {
    ...record,
    state: next,
    acceptedAt: result.acceptedAt,
    terminalAt: result.status === "accepted" ? null : (result.terminalAt ?? null),
    terminalEventId: result.terminalEventId,
    result: result.status === "completed" ? result.result : null,
    error: result.error,
    serverConfirmedUncertain: next === "uncertain",
  };
}

/** `command.status` 的查询结果（同一 `targetRequestId`，§11.4）。 */
export function applyStatusRecord(
  record: CommandRecord,
  status: {
    readonly targetRequestId: Uuid;
    readonly state: "accepted" | "completed" | "failed" | "uncertain" | "rejected";
    readonly acceptedAt: string | null;
    readonly terminalAt: string | null;
    readonly terminalEventId: Uuid | null;
    readonly result: Readonly<Record<string, unknown>> | null;
    readonly error: PublicError | null;
  },
): CommandRecord {
  if (status.targetRequestId !== record.requestId) {
    throw new Error(`command.status 的 targetRequestId ${status.targetRequestId} 与记录 ${record.requestId} 不符`);
  }
  if (isTerminalState(record.state)) {
    return record;
  }
  const next: CommandState = status.state === "accepted" ? "accepted" : status.state;
  return {
    ...record,
    state: next,
    acceptedAt: status.acceptedAt,
    terminalAt: status.terminalAt,
    terminalEventId: status.terminalEventId,
    result: next === "completed" ? status.result : null,
    error: status.error,
    serverConfirmedUncertain: next === "uncertain",
  };
}

/**
 * 重试：同一 `requestId` 再发一次。
 *
 * 返回的记录**必须** `requestId` 不变而 `dispatchCount` 递增——这正是「不得为同一意图
 * 生成新标识」的可核对落点。
 *
 * `accepted` 同样拒绝重试：服务端已经确认接受过这条命令，把它推回 `submitting`
 * 会让 `mayDisplayAsAccepted` 变 false，用户从「已接受」闪回「等待服务器确认」——
 * 那是**本地**回退了一个服务端结论。终态之后只能靠 `command.status` 查询，不靠重发。
 */
export function retrySameRequest(record: CommandRecord): CommandRecord {
  if (isTerminalState(record.state)) {
    throw new Error(`命令 ${record.requestId} 已终结为 ${record.state}，不可重试`);
  }
  if (record.state === "accepted") {
    throw new Error(`命令 ${record.requestId} 已被服务器接受，不可重试（改用 command.status 查询）`);
  }
  return { ...record, state: "submitting", dispatchCount: record.dispatchCount + 1 };
}

/** 终态判定：这些状态不再自动推进。 */
export function isTerminalState(state: CommandState): boolean {
  return state === "completed" || state === "failed" || state === "rejected" || state === "uncertain";
}

/**
 * 呈现口径：命令是否可以显示为「已被服务器接受」。
 *
 * `docs/FRONTEND_DESIGN.md` §5「只有进入 `online` 后，客户端才能把命令显示为已被服务器接受」，
 * 且 `acceptedAt` 非空是服务端确认过的唯一证据。
 */
export function mayDisplayAsAccepted(record: CommandRecord, connectionOnline: boolean): boolean {
  return connectionOnline && record.state === "accepted" && record.acceptedAt !== null;
}

/**
 * 是否呈现「结果不确定」。
 *
 * 只看服务端确认：本地断线、重连、重新提交都不构成不确定的证据。
 * 该终态按 spec「显眼且不被自动消解」保持，直到用户主动处理。
 */
export function shouldPresentUncertain(record: CommandRecord): boolean {
  return record.state === "uncertain" && record.serverConfirmedUncertain;
}

/** 7 个命令状态的封闭清单。 */
export const COMMAND_STATES = {
  draft: true,
  submitting: true,
  accepted: true,
  completed: true,
  failed: true,
  rejected: true,
  uncertain: true,
} as const satisfies Record<CommandState, true>;

/** 未确认接受时的呈现文案（`submitting` 与 `accepted` 必须可区分）。 */
export const PENDING_CONFIRMATION_LABEL = "已发出，等待服务器确认";