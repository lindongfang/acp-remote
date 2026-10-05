/**
 * 会话创建：受限入口（R16「会话创建是受限入口」）。
 *
 * ## 载荷为什么可以是「精确两键」而不是「至少两键」
 *
 * `schemas/sync/v1/command.schema.json#/$defs/sessionCreate` 的 `payload` 是精确两键对象，
 * 多写 `cwd`/`exportId`/`templateParams`/绝对路径/MCP 配置/凭据即被服务端按
 * `protocol.schema_invalid` 拒绝。因此本模块的 `CreateSessionRefs` 是精确形状，
 * 并配一条**运行期**断言 `assertCreateSessionRefs`：类型系统保护编译期，
 * 断言保护「从别处拼出来的对象」这一路（例如后续有人从 JSON 里读回 payload 再转发）。
 *
 * ## 无权限态为什么不是「隐藏入口」
 *
 * spec 场景「无权限时入口不可用且说明原因」MUST NOT 仅隐藏入口而不作说明。
 * 因此 `CreateSessionPermission` 的两个分支都带**非空**的说明文案，组件必须把
 * `denied` 分支的说明渲染出来；「入口不可用」由 `enabled: false` 表达。
 *
 * ## 结果不确定为什么不会自动消解
 *
 * `uncertain` 终态由服务端确认（`src/state/command-machine.ts` 的
 * `serverConfirmedUncertain`）。本模型把它建模成**没有时间维度**的分支：
 * 既不存时间戳也不存「稍后重试」的定时器，因此不可能随时间或重连自动清除；
 * 只有用户主动处理（`dismiss`）才会离开该分支。
 */

import type { CommandMessage, ErrorCode, PublicError, Uuid } from "../protocol";
import type { CommandRecord, CommandState } from "../state";
import { isTerminalState, mayDisplayAsAccepted, shouldPresentUncertain } from "../state";
import type { AgentCatalogEntryView } from "../domain";

/** 创建会话所需的 scope（`compatibility/commands/v1/commands.json` 的 `session.create.scope`）。 */
export const CREATE_SESSION_SCOPE = "session.create";

/** 提交载荷的两个引用：目录别名 + Agent 标识。**只有这两项**。 */
export interface CreateSessionRefs {
  readonly workspaceAlias: string;
  readonly agentId: string;
}

/** 载荷越界：出现了路径、目录追加、MCP 配置或凭据字段。 */
export class CreateSessionPayloadViolation extends Error {
  constructor(detail: string) {
    super(`session.create 载荷越界：${detail}`);
    this.name = "CreateSessionPayloadViolation";
  }
}

/** 精确两键的载荷键表；任何其它键都是越界。 */
const ALLOWED_PAYLOAD_KEYS: readonly string[] = ["workspaceAlias", "agentId"];

/**
 * 运行期校验：载荷**恰好**是两个引用。
 *
 * 存在的理由不是「类型已经保证了」——它保证的是**本模块内部**写出来的对象；
 * 断言拦的是「对象来自别处」（从 JSON 读回、从缓存恢复、从 UI 事件里解出来的字段）。
 */
export function assertCreateSessionRefs(payload: unknown): CreateSessionRefs {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) {
    throw new CreateSessionPayloadViolation("载荷不是对象");
  }
  const record = payload as Record<string, unknown>;
  const extra = Object.keys(record).filter((key) => !ALLOWED_PAYLOAD_KEYS.includes(key));
  if (extra.length > 0) {
    throw new CreateSessionPayloadViolation(`出现越界字段 ${extra.join("、")}`);
  }
  const workspaceAlias = record["workspaceAlias"];
  const agentId = record["agentId"];
  if (typeof workspaceAlias !== "string" || workspaceAlias.length === 0) {
    throw new CreateSessionPayloadViolation("workspaceAlias 缺失或为空");
  }
  if (typeof agentId !== "string" || agentId.length === 0) {
    throw new CreateSessionPayloadViolation("agentId 缺失或为空");
  }
  return { workspaceAlias, agentId };
}

/**
 * 构造 `session.create` 命令。
 *
 * `payload` 是**就地构造**的两键对象，不接受调用方传入的任意 payload：
 * 这样「多带一个字段」在本模块内没有表达方式。
 */
export function buildCreateSessionCommand(input: {
  readonly refs: CreateSessionRefs;
  readonly requestId: Uuid;
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  /**
   * 出站连接序号。
   *
   * `$defs.command` 的信封把它列为必填，因此这里缺省给占位 `"0"`；真正的客户端方向序号由
   * `SyncConnection.sendCommand` 覆盖（§4.1），调用方**不需要**自己维护计数器。
   */
  readonly connectionSequence?: string;
}): CommandMessage {
  const payload = assertCreateSessionRefs({
    workspaceAlias: input.refs.workspaceAlias,
    agentId: input.refs.agentId,
  });
  return {
    protocolVersion: 1,
    type: "command",
    messageId: input.messageId,
    connectionId: input.connectionId,
    connectionSequence: input.connectionSequence ?? "0",
    body: {
      requestId: input.requestId,
      command: "session.create",
      payload,
    },
  };
}

/** 入口权限。两个分支都带说明——「不可用」不允许等于「不解释」。 */
export type CreateSessionPermission =
  | { readonly kind: "granted"; readonly scope: string; readonly note: string }
  | { readonly kind: "denied"; readonly reason: string; readonly note: string };

/** 无权限时的固定说明：必须由电脑端授予，且不得换个说法伪装成可用。 */
export const CREATE_DENIED_NOTE = "这台设备没有创建会话的权限，需要在电脑端授予后才能使用。";

/**
 * 由本次认证授予的 scopes 判定入口权限。
 *
 * `scopes` 为 `null` 表示尚未认证：同样不可用，说明理由是「尚未连接」，而不是「没有权限」——
 * 两者必须能被用户区分。
 */
export function createSessionPermission(scopes: readonly string[] | null): CreateSessionPermission {
  if (scopes === null) {
    return { kind: "denied", reason: "尚未连接主机，权限未知", note: CREATE_DENIED_NOTE };
  }
  if (scopes.includes(CREATE_SESSION_SCOPE)) {
    return {
      kind: "granted",
      scope: CREATE_SESSION_SCOPE,
      note: "只会列出这台电脑上已配置的 Agent；凭据不会进入浏览器。",
    };
  }
  return { kind: "denied", reason: `权限中不含 ${CREATE_SESSION_SCOPE}`, note: CREATE_DENIED_NOTE };
}

/** 提交状态。`uncertain` 没有时间维度，因此不会被自动消解。 */
export type CreateSubmissionModel =
  | { readonly kind: "idle" }
  | { readonly kind: "creating" }
  | { readonly kind: "accepted"; readonly acceptedAt: string | null }
  | { readonly kind: "failed"; readonly error: PublicError }
  | { readonly kind: "uncertain"; readonly requestId: Uuid }
  | { readonly kind: "created"; readonly sessionId: Uuid };

/** 提交按钮是否可点：只有 idle/失败/结果不确定（用户主动重试）时可点。 */
export function isSubmissionAvailable(submission: CreateSubmissionModel): boolean {
  if (submission.kind === "creating") return false;
  if (submission.kind === "accepted") return false;
  return true;
}

/**
 * 由命令记录投影提交状态。
 *
 * - `submitting`/`draft` → 创建中（呈现「创建中」且**不接受重复提交**）；
 * - `completed` → 已创建（带新会话标识，供页面跳转）；
 * - `failed`/`rejected` → 结构化失败原因（`PublicError` 的 code/message/retryable 逐项可区分）；
 * - `uncertain` → 结果不确定，**只有服务端确认过**才这么呈现（spec 场景「结果不确定不被自行断定」）。
 */
export function createSubmissionModel(
  record: CommandRecord | null,
  input: { readonly connectionOnline: boolean },
): CreateSubmissionModel {
  if (record === null) return { kind: "idle" };
  switch (record.state as CommandState) {
    case "draft":
    case "submitting":
      return { kind: "creating" };
    case "accepted":
      // 未拿到服务端确认的 `acceptedAt` 前不得显示为「已接受」。
      return mayDisplayAsAccepted(record, input.connectionOnline)
        ? { kind: "accepted", acceptedAt: record.acceptedAt }
        : { kind: "creating" };
    case "completed": {
      const sessionId = typeof record.result?.["sessionId"] === "string" ? record.result["sessionId"] : null;
      return sessionId === null
        ? { kind: "failed", error: missingSessionIdError() }
        : { kind: "created", sessionId };
    }
    case "failed":
    case "rejected":
      return { kind: "failed", error: record.error ?? genericFailureError(record.state) };
    case "uncertain":
      return shouldPresentUncertain(record)
        ? { kind: "uncertain", requestId: record.requestId }
        : { kind: "creating" };
  }
}

/** `completed` 却没带 `sessionId`：协议违规，按结构化失败呈现而不是假装成功。 */
function missingSessionIdError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "创建会话已完成但未返回会话标识，无法进入新会话。",
    retryable: false,
    details: {},
  };
}

/** 没有 `error` 的失败终态：保留状态名，不编造原因。 */
function genericFailureError(state: CommandState): PublicError {
  return {
    code: "internal.unavailable" satisfies ErrorCode,
    message: `服务端未给出失败原因（${state}）。`,
    retryable: false,
    details: {},
  };
}

/** 创建弹层模型：目录 + 权限 + Agent 候选 + 提交状态。 */
export interface CreateSessionEntryModel {
  readonly directoryAlias: string;
  readonly permission: CreateSessionPermission;
  /** Agent 候选：**只**来自本机已配置的 Agent 目录（快照 `agents` 资源）。 */
  readonly agents: readonly AgentCatalogEntryView[];
  readonly submission: CreateSubmissionModel;
  /** 终态判定：结果不确定时为真，页面据此保留显眼提示直到用户处理。 */
  readonly uncertainRequiresUserAction: boolean;
}

/**
 * 构造创建弹层模型。
 *
 * @param agents 本机已配置的 Agent 目录（快照 `agents`）。传入目录之外的 Agent 不会被补进来，
 *   因此「候选只来自已配置 Agent」在数据流上成立。
 */
export function buildCreateSessionEntryModel(input: {
  readonly directoryAlias: string;
  readonly scopes: readonly string[] | null;
  readonly agents: readonly AgentCatalogEntryView[];
  readonly record: CommandRecord | null;
  readonly connectionOnline: boolean;
}): CreateSessionEntryModel {
  const submission = createSubmissionModel(input.record, { connectionOnline: input.connectionOnline });
  return {
    directoryAlias: input.directoryAlias,
    permission: createSessionPermission(input.scopes),
    agents: input.agents,
    submission,
    uncertainRequiresUserAction: submission.kind === "uncertain",
  };
}

/** 终态判定是否已到（页面据此离开「创建中」）。 */
export function isSubmissionTerminal(submission: CreateSubmissionModel): boolean {
  if (submission.kind === "creating" || submission.kind === "idle") return false;
  return isTerminalState(submission.kind === "created" ? "completed" : submission.kind);
}
