/**
 * 会话创建（R16）。
 *
 * ## 载荷为什么可以是「精确列出」而不是「至少列出」
 *
 * `schemas/sync/v1/command.schema.json#/$defs/sessionCreate` 的 `payload` 只允许 `agentId`
 * 与**一个**目录引用：`workspaceAlias`（已登记别名）或 `workspacePath`（客户端提供的绝对目录），
 * 二者恰好一个。多写 `cwd`/`exportId`/`templateParams`/MCP 配置/凭据即被服务端按
 * `protocol.schema_invalid` 拒绝。因此本模块的 `CreateSessionRefs` 是精确形状，
 * 并配一条**运行期**断言 `assertCreateSessionRefs`：类型系统保护编译期，
 * 断言保护「从别处拼出来的对象」这一路（例如后续有人从 JSON 里读回 payload 再转发）。
 *
 * ## 目录不再有权限门控
 *
 * 设备不再有创建会话的权限维度（`pairing-model` 删除设备权限层）：入口**始终**可用，
 * 也不再有任何「需要由电脑端授予」的说明。会话开在哪个目录由用户承担——浏览器没有获取宿主
 * 绝对路径的能力，因此目录来源在界面上就是一次文本输入（另可选用本目录的已登记别名）。
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

/** 提交载荷的两个引用：目录引用（别名或客户端提供的绝对目录）+ Agent 标识。**只有这两项**。 */
export type CreateSessionRefs =
  | { readonly workspaceAlias: string; readonly agentId: string }
  | { readonly workspacePath: string; readonly agentId: string };

/** 载荷越界：出现了路径、目录追加、MCP 配置或凭据字段。 */
export class CreateSessionPayloadViolation extends Error {
  constructor(detail: string) {
    super(`session.create 载荷越界：${detail}`);
    this.name = "CreateSessionPayloadViolation";
  }
}

/** 载荷允许出现的键；`agentId` 之外只允许一个目录引用键。 */
const ALLOWED_PAYLOAD_KEYS: readonly string[] = ["workspaceAlias", "workspacePath", "agentId"];
const DIRECTORY_REF_KEYS: readonly string[] = ["workspaceAlias", "workspacePath"];

/**
 * 运行期校验：载荷**恰好**是 `agentId` 加一个目录引用。
 *
 * 存在的理由不是「类型已经保证了」——它保证的是**本模块内部**写出来的对象；
 * 断言拦的是「对象来自别处」（从 JSON 读回、从缓存恢复、从 UI 事件里解出来的字段）。
 *
 * 目录引用必须**恰好一个**：同时给出别名与路径、或两者都不给，都在这里当场失败——
 * 与 `sync-session-create` 的「目录引用二选一」同一条口径（不猜以哪一个为准）。
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
  const present = DIRECTORY_REF_KEYS.filter((key) => record[key] !== undefined);
  if (present.length !== 1) {
    throw new CreateSessionPayloadViolation("目录引用必须恰好给出一个（workspaceAlias 或 workspacePath）");
  }
  const agentId = record["agentId"];
  if (typeof agentId !== "string" || agentId.length === 0) {
    throw new CreateSessionPayloadViolation("agentId 缺失或为空");
  }
  const key = present[0];
  const value = key === undefined ? undefined : record[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new CreateSessionPayloadViolation(`${String(key)} 缺失或为空`);
  }
  return key === "workspacePath"
    ? { workspacePath: value, agentId }
    : { workspaceAlias: value, agentId };
}

/**
 * 构造 `session.create` 命令。
 *
 * `payload` 是**就地构造**的对象，不接受调用方传入的任意 payload：
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
  const payload = assertCreateSessionRefs(input.refs);
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

/**
 * 创建入口旁**始终**呈现的副作用说明（spec「创建入口的副作用说明」）。
 *
 * 提交会在电脑上真的启动一个 Agent 进程；目录由用户输入，因此把会话开在敏感目录的风险
 * 由用户承担，这句提示是唯一的缓解——它 MUST NOT 声称服务端会替用户拦截。
 */
export const CREATE_EFFECT_NOTE = "将在你的电脑上启动一个 Agent 进程。";

/**
 * 目录输入的提示：说清「留空用本目录、填写则是客户端指定的绝对目录」与两类失败，
 * 并且**不**声称服务端会拦截敏感目录。
 */
export const CREATE_DIRECTORY_HINT =
  "留空则使用本目录；也可以填写电脑上的绝对目录（如 D:\\work\\api）另开一处。" +
  "相对路径或含「..」的路径会被拒绝；目录不存在或不是目录会以服务端不可用类错误失败。" +
  "请自行确认这个目录不是敏感目录——服务端不会替你做这个判断。";

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

/** 创建弹层模型：目录 + Agent 候选 + 提交状态。 */
export interface CreateSessionEntryModel {
  readonly directoryAlias: string;
  /** 入口旁始终呈现的副作用说明（{@link CREATE_EFFECT_NOTE}）。 */
  readonly effectNote: string;
  /** 目录输入的提示（{@link CREATE_DIRECTORY_HINT}）。 */
  readonly directoryHint: string;
  /** Agent 候选：**只**来自本机已配置的 Agent 目录（快照 `agents` 资源）。 */
  readonly agents: readonly AgentCatalogEntryView[];
  readonly submission: CreateSubmissionModel;
  /** 终态判定：结果不确定时为真，页面据此保留显眼提示直到用户处理。 */
  readonly uncertainRequiresUserAction: boolean;
}

/**
 * 构造创建弹层模型。
 *
 * 入口**没有**权限维度：删掉设备权限层后，创建入口恒可用（spec「入口不因权限不可用」），
 * 因此这里不接任何 scopes 入参。
 *
 * @param agents 本机已配置的 Agent 目录（快照 `agents`）。传入目录之外的 Agent 不会被补进来，
 *   因此「候选只来自已配置 Agent」在数据流上成立。
 */
export function buildCreateSessionEntryModel(input: {
  readonly directoryAlias: string;
  readonly agents: readonly AgentCatalogEntryView[];
  readonly record: CommandRecord | null;
  readonly connectionOnline: boolean;
}): CreateSessionEntryModel {
  const submission = createSubmissionModel(input.record, { connectionOnline: input.connectionOnline });
  return {
    directoryAlias: input.directoryAlias,
    effectNote: CREATE_EFFECT_NOTE,
    directoryHint: CREATE_DIRECTORY_HINT,
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
