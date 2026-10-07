/**
 * `schemas/sync/v1/command.schema.json` 的严格 TypeScript 镜像：11 条 Sync 面命令与结果。
 *
 * `session.read` 的 `before`/`limit` **只**在 Sync 传输面生效（`design.md` D3）；Node Link 面的
 * 同名命令不含这两个键。`session.create` 的 `payload` **只**允许 `workspaceAlias` 与 `agentId`
 * 两个引用（`design.md` D8 / `pwa-web-client` 的「会话创建是受限入口」）。
 *
 * `session.resume` 只经 Node Link 接受，因此不在本镜像的 `CommandName` 里。
 */

import type {
  ConfigOptionView,
  DecimalString,
  ModeState,
  PublicError,
  SessionSummary,
  Timestamp,
  Uuid,
} from "./common";
import type {
  SnapshotItemCapabilities,
  SnapshotItemConfigOptions,
  SnapshotItemMessages,
  SnapshotItemPendingInteractions,
  SnapshotItemTurns,
} from "./sync";

/** `$defs.commandName`：只含 `transport` 包含 `sync` 的命令。 */
export type CommandName =
  | "session.list"
  | "session.read"
  | "command.status"
  | "session.mode.list"
  | "session.config.list"
  | "session.prompt"
  | "session.cancel"
  | "elicitation.respond"
  | "session.mode.set"
  | "session.config.set"
  | "permission.resolve"
  | "session.create";

/** 命令消息的公共信封（`$defs.command` 的固有字段）。 */
export interface CommandEnvelope {
  readonly protocolVersion: 1;
  readonly type: "command";
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
}

/** `$defs.sessionList`。 */
export interface SessionListCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.list";
    readonly payload: Record<string, never>;
  };
}

/**
 * `$defs.sessionCreate`：Sync 面 `body` 顶层不含 `sessionId`；`payload` 是精确两键对象，
 * 多写 `cwd`/`exportId`/`templateParams`/绝对路径/MCP 配置/凭据即编译失败。
 */
export interface SessionCreateCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.create";
    readonly payload: {
      readonly workspaceAlias: string;
      readonly agentId: string;
    };
  };
}

/**
 * `$defs.sessionReadBefore`：`(createdAt, messageId)` 复合游标，两键都必需——
 * 单字段游标按 `protocol.schema_invalid` 拒绝（`design.md` D2）。
 */
export interface SessionReadBefore {
  readonly createdAt: Timestamp;
  readonly messageId: Uuid;
}

/** `$defs.sessionReadLimit`：结构性计数；省略时服务端用配置默认页，无 `maximum`。 */
export type SessionReadLimit = number;

/** `session.read` 可请求的明细资源名（`payload.include` 的取值域）。 */
export type SessionReadInclude =
  | "messages"
  | "turns"
  | "pending_interactions"
  | "config_options"
  | "capabilities";

/** `$defs.sessionRead`。 */
export interface SessionReadCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.read";
    readonly sessionId: Uuid;
    readonly payload: {
      readonly include: readonly SessionReadInclude[];
      readonly before?: SessionReadBefore;
      readonly limit?: SessionReadLimit;
    };
  };
}

/** `$defs.sessionPrompt`：v1 只发送文本 prompt（图片/文件必须显式报不支持）。 */
export interface SessionPromptCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.prompt";
    readonly sessionId: Uuid;
    readonly payload: {
      readonly content: readonly { readonly type: "text"; readonly text: string }[];
    };
  };
}

/** `$defs.sessionCancel`。 */
export interface SessionCancelCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.cancel";
    readonly sessionId: Uuid;
    readonly payload: { readonly turnId: Uuid };
  };
}

/** `$defs.configList`（`session.config.list`）。 */
export interface ConfigListCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.config.list";
    readonly sessionId: Uuid;
    readonly payload: Record<string, never>;
  };
}

/** `$defs.configSet`（`session.config.set`）：乐观并发由 `expectedVersion` 承担。 */
export interface ConfigSetCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.config.set";
    readonly sessionId: Uuid;
    readonly expectedVersion: DecimalString;
    readonly payload: {
      readonly configId: string;
      readonly value: string | boolean;
    };
  };
}

/** `$defs.modeList`（`session.mode.list`）。 */
export interface ModeListCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.mode.list";
    readonly sessionId: Uuid;
    readonly payload: Record<string, never>;
  };
}

/** `$defs.modeSet`（`session.mode.set`）。 */
export interface ModeSetCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.mode.set";
    readonly sessionId: Uuid;
    readonly expectedVersion: DecimalString;
    readonly payload: { readonly modeId: string };
  };
}

/** `$defs.permissionResolve`。 */
export interface PermissionResolveCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "permission.resolve";
    readonly sessionId: Uuid;
    readonly payload: {
      readonly interactionId: Uuid;
      readonly optionId: string;
    };
  };
}

/** `$defs.elicitationRespond.payload`：三支 `oneOf`，`action` 决定 `values` 的可空性。 */
export type ElicitationRespondPayload =
  | {
      readonly interactionId: Uuid;
      readonly action: "submit";
      readonly values: Record<string, unknown> | null;
    }
  | {
      readonly interactionId: Uuid;
      readonly action: "decline";
      readonly values: null;
    }
  | {
      readonly interactionId: Uuid;
      readonly action: "cancel";
      readonly values: null;
    };

/** `$defs.elicitationRespond`。 */
export interface ElicitationRespondCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "elicitation.respond";
    readonly sessionId: Uuid;
    readonly payload: ElicitationRespondPayload;
  };
}

/** `$defs.commandStatus`：用稳定 `requestId` 查终态，不生成新标识。 */
export interface CommandStatusCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "command.status";
    readonly payload: { readonly targetRequestId: Uuid };
  };
}

/** `command` 消息的 `body` 联合（`$defs.command.body` 的 `oneOf`）。 */
export type CommandBody =
  | SessionListCommand["body"]
  | SessionCreateCommand["body"]
  | SessionReadCommand["body"]
  | SessionPromptCommand["body"]
  | SessionCancelCommand["body"]
  | ConfigListCommand["body"]
  | ConfigSetCommand["body"]
  | ModeListCommand["body"]
  | ModeSetCommand["body"]
  | PermissionResolveCommand["body"]
  | ElicitationRespondCommand["body"]
  | CommandStatusCommand["body"];

/** `$defs.command`：`body` 由上面的判别联合（按 `command` 字段）表达。 */
export type CommandMessage = CommandEnvelope & { readonly body: CommandBody };

// ── 结果 ────────────────────────────────────────────────────────────────────

/** `$defs.sessionListResult`。 */
export interface SessionListResult {
  readonly sessions: readonly SessionSummary[];
}

/**
 * `$defs.sessionReadResult`：`resources` 的每个键都可缺席；`hasEarlier` 是必填布尔值，
 * 客户端据此决定是否继续用最早一条的 `(createdAt, messageId)` 构造 `before` 翻页。
 */
export interface SessionReadResult {
  readonly sessionId: Uuid;
  readonly resources: {
    readonly messages?: readonly SnapshotItemMessages[];
    readonly turns?: readonly SnapshotItemTurns[];
    readonly pending_interactions?: readonly SnapshotItemPendingInteractions[];
    readonly config_options?: readonly SnapshotItemConfigOptions[];
    readonly capabilities?: readonly SnapshotItemCapabilities[];
  };
  readonly hasEarlier: boolean;
}

/** `$defs.configListResult`。 */
export interface ConfigListResult {
  readonly configOptions: readonly ConfigOptionView[];
  readonly version: DecimalString;
}

/** `$defs.modeListResult`（即 `common.$defs.modeState`）。 */
export type ModeListResult = ModeState;

/** `$defs.sessionCreateResult`。 */
export interface SessionCreateResult {
  readonly sessionId: Uuid;
  readonly session: SessionSummary;
}

/** `$defs.commandStatusRecord.state` 的封闭词表。 */
export type CommandStatusRecordState =
  | "accepted"
  | "completed"
  | "failed"
  | "uncertain"
  | "rejected";

/** `$defs.commandStatusRecord`。 */
export interface CommandStatusRecord {
  readonly targetRequestId: Uuid;
  readonly state: CommandStatusRecordState;
  readonly acceptedAt: Timestamp | null;
  readonly terminalAt: Timestamp | null;
  readonly terminalEventId: Uuid | null;
  readonly result: Record<string, unknown> | null;
  readonly error: PublicError | null;
}

/** `$defs.commandResult.body.status` 的封闭词表。 */
export type CommandResultStatus =
  | "accepted"
  | "completed"
  | "failed"
  | "rejected"
  | "uncertain";

/**
 * `$defs.commandResult.body`。
 *
 * schema 用 `allOf`/`if`/`then` 把 `result` 的具体形状与 `command`+`status` 绑定，
 * 这里保留 `result` 为结构化联合以表达「只有 `completed` 才有具体结果」，
 * 并在 `protocol/result.ts` 提供按 `command` 收窄的读取器。
 */
export interface CommandResultBody {
  readonly requestId: Uuid;
  readonly command: CommandName;
  readonly status: CommandResultStatus;
  readonly acceptedAt: Timestamp | null;
  readonly terminalEventId: Uuid | null;
  readonly result: CommandResultValue;
  readonly error: PublicError | null;
}

/** `command.result` 的 `result` 取值联合（缺席/各命令的具体结果）。 */
export type CommandResultValue =
  | null
  | SessionListResult
  | SessionReadResult
  | SessionCreateResult
  | ConfigListResult
  | ModeListResult
  | CommandStatusRecord
  | Record<string, unknown>;

/** `$defs.commandResult`：`command.result` 消息。 */
export interface CommandResultMessage {
  readonly protocolVersion: 1;
  readonly type: "command.result";
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  readonly connectionSequence: DecimalString;
  readonly body: CommandResultBody;
}

/** `command.schema.json` 的顶层 `oneOf`。 */
export type CommandSchemaMessage = CommandMessage | CommandResultMessage;
