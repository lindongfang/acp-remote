/**
 * `schemas/sync/v1/command.schema.json` 的严格 TypeScript 镜像：13 条 Sync 面命令与结果。
 *
 * `session.read` 的 `before`/`limit`/`beforeHistory`/`include` 里的 `history` **只**在 Sync
 * 传输面生效（`design.md` D3、D11）；Node Link 面的同名命令不含这些键。`session.create` 的
 * `payload` **只**允许 `agentId`、目录引用与建目录开关三类字段：目录引用由 `directoryId`
 * （已记录目录的标识）与 `workspacePath`（客户端提供的绝对目录）**恰好一个**给出，可选的
 * `createIfMissing` 只与 `workspacePath` 相容（`design.md` D4 / `pwa-web-client` 的
 * 「会话创建入口与载荷」）。`workspace.browse` 是只读的逐层目录列举（feature
 * `core.workspace-browse.v1` 门控，§11.5）。
 *
 * `session.resume` 只经 Node Link 接受，因此不在本镜像的 `CommandName` 里。
 */

import type {
  ConfigOptionView,
  DecimalString,
  MessageCoverage,
  ModeState,
  OriginCursor,
  PromptContentBlock,
  PublicError,
  SessionSummary,
  Timestamp,
  Uuid,
} from "./common";
import type { EffectiveInteraction } from "./media";
import type {
  SnapshotItemCapabilities,
  SnapshotItemConfigOptions,
  SnapshotItemHistory,
  SnapshotItemMessages,
  SnapshotItemTurns,
} from "./sync";
import type {
  SessionFilesListCommand,
  SessionFilesListResult,
  SessionFilesReadCommand,
  SessionFilesReadResult,
  SessionGitDiffCommand,
  SessionGitDiffResult,
  SessionGitStatusCommand,
  SessionGitStatusResult,
} from "./session-files";

/**
 * `$defs.commandName`：只含 `transport` 包含 `sync` 的命令（当前 18 条，与
 * `compatibility/commands/v1/commands.json` 里 `transport` 含 `sync` 的条目一一对应）。
 * 四条 `session.files.*` / `session.git.*` 的严格形状与解码见 `./session-files`。
 */
export type CommandName =
  | "session.list"
  | "session.read"
  | "command.status"
  | "session.mode.list"
  | "session.config.list"
  | "workspace.browse"
  | "workspace.mkdir"
  | "session.prompt"
  | "session.cancel"
  | "elicitation.respond"
  | "session.mode.set"
  | "session.config.set"
  | "permission.resolve"
  | "session.create"
  | "session.files.list"
  | "session.files.read"
  | "session.git.status"
  | "session.git.diff";

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
 * `$defs.workspaceBrowsePath`：`workspace.browse` 的父路径——**绝对**路径（1..=4096 字符）。
 * 它同时是请求字段与结果回显字段：结果里的取值必须与请求**逐字相同**，客户端 MUST NOT 把它
 * 读成「服务端规范化过的权威值」（权威值是条目里的 `path`）。
 */
export type WorkspaceBrowsePath = string;

/** `$defs.workspaceBrowseEntry`：该层的**直接子目录**一项；只含名称、绝对路径与登记状态。 */
export interface WorkspaceBrowseEntry {
  /** 子目录的名称（不含路径分隔符）。 */
  readonly name: string;
  /** 该子目录的下发形式绝对路径（规范化结果剥掉平台 verbatim 前缀）。 */
  readonly path: WorkspaceBrowsePath;
  /** 该子目录是否已是本机已登记的 workspace（只反映登记状态，不产生登记副作用）。 */
  readonly registered: boolean;
}

/**
 * `$defs.workspaceBrowse`：只读的逐层目录列举（`docs/SYNC_PROTOCOL.md` §11.5）。
 *
 * 请求只含一个父路径字段，且该键**必填**：`null` 表示「该节点的根列表」，缺席不是它的同义词
 * （缺席按 `protocol.schema_invalid` 拒收）。受 feature `core.workspace-browse.v1` 门控，未协商
 * 时服务端在触碰文件系统之前回 `protocol.feature_required`。
 */
export interface WorkspaceMkdirCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "workspace.mkdir";
    readonly payload: { readonly parentPath: WorkspaceBrowsePath; readonly name: string };
  };
}
export interface WorkspaceMkdirResult { readonly created: true; }

export interface WorkspaceBrowseCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "workspace.browse";
    readonly payload: {
      readonly path: WorkspaceBrowsePath | null;
    };
  };
}

/**
 * `$defs.sessionCreate` 的 payload（Sync 面）。
 *
 * `agentId`（必填，本机已配置的 Agent 标识）与目录引用：目录引用由 `directoryId`
 * （本机已记录目录的标识）与 `workspacePath`（客户端提供的**绝对**目录）**恰好一个**
 * 给出——同时给出或都不给出一律被服务端以 `protocol.schema_invalid` 拒绝，客户端 MUST NOT
 * 猜测以哪一个为准。
 *
 * 可选的 `createIfMissing` 是显式的「目录不存在则先创建」开关，它与 `directoryId`
 * **互斥**（已记录的目录必然已存在）：联合型把它钉在 `workspacePath` 那一支上，
 * 另一支把该键声明成 `never`，因此「目录标识 + createIfMissing」这种形状在编译期就无法构造。
 */
export type SessionCreatePayload =
  | {
      readonly directoryId: string;
      readonly agentId: string;
      readonly createIfMissing?: never;
    }
  | {
      readonly workspacePath: string;
      readonly agentId: string;
      readonly createIfMissing?: boolean;
    };

/**
 * `$defs.sessionCreate`：Sync 面 `body` 顶层不含 `sessionId`；`payload` 只允许 `agentId`、目录引用
 * 与建目录开关三类字段，多写 `cwd`/`exportId`/`templateParams`/MCP 配置/凭据即编译失败。
 */
export interface SessionCreateCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.create";
    readonly payload: SessionCreatePayload;
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

/**
 * `$defs.sessionReadBeforeHistory`：历史条目的游标——**会话内事件序号**（十进制字符串，D2）。
 *
 * 它与消息的 `before`（`(createdAt, messageId)` 复合游标）是两套**独立**游标：同一请求可
 * 同时给出两者，各自翻各自的页，任一方都不改变另一方的页边界。序号在提交时分配、天然单调
 * 唯一，因此不需要并列次序规则。服务端 MUST NOT 用事件标识代替它。
 */
export type SessionReadBeforeHistory = DecimalString;

/** `session.read` 可请求的明细资源名（`payload.include` 的取值域）。 */
export type SessionReadInclude =
  | "messages"
  | "turns"
  | "pending_interactions"
  | "config_options"
  | "capabilities"
  | "history";

/** `$defs.sessionRead`。 */
export interface SessionReadCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.read";
    readonly sessionId: Uuid;
    readonly payload: {
      readonly include: readonly SessionReadInclude[];
      readonly before?: SessionReadBefore;
      /** 历史条目的续取游标；与 `before` 互不影响（两套独立分页，D2）。 */
      readonly beforeHistory?: SessionReadBeforeHistory;
      readonly limit?: SessionReadLimit;
    };
  };
}

/**
 * `$defs.sessionPrompt`：内容块按协商结果表达（文本 / 已封存附件引用 / 工作目录路径引用）。
 *
 * `submissionId` 只在本会话/本主体已 `manifest_sealed` 的附件清单存在时出现，且只在协商
 * `core.attachment-transfer.v1` 后被接受；旧 peer 省略该键，行为与本变更前逐字相同。
 */
export interface SessionPromptCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.prompt";
    readonly sessionId: Uuid;
    readonly payload: {
      readonly content: readonly PromptContentBlock[];
      readonly submissionId?: Uuid;
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
      /** 协商 `core.pending-interactions.v1` 后必须携带；旧 peer 可省略。 */
      readonly requestVersion?: DecimalString;
      readonly optionId: string;
    };
  };
}

/** `$defs.elicitationRespond.payload`：三支 `oneOf`，`action` 决定 `values` 的可空性。 */
export type ElicitationRespondPayload =
  | {
      readonly interactionId: Uuid;
      /** 协商 `core.pending-interactions.v1` 后必须携带；旧 peer 可省略。 */
      readonly requestVersion?: DecimalString;
      readonly action: "submit";
      readonly values: Record<string, unknown> | null;
    }
  | {
      readonly interactionId: Uuid;
      readonly requestVersion?: DecimalString;
      readonly action: "decline";
      readonly values: null;
    }
  | {
      readonly interactionId: Uuid;
      readonly requestVersion?: DecimalString;
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

/**
 * `$defs.remoteTarget`：远程命令/查询的来源定位（`resource.stateless-origin.v1`，§9.7/§11.4）。
 *
 * `sessionId` 为 `null` 只用于尚未建成会话的 `session.create`；其他会话操作必须给出原来源会话。
 * 该键只在 feature 协商后出现，未协商的连接 MUST NOT 发送它。
 */
export interface RemoteTarget {
  readonly ownerNodeId: Uuid;
  readonly sessionId: Uuid | null;
}

/** `$defs.commandStatus`：用稳定 `requestId` 查终态，不生成新标识。 */
export interface CommandStatusCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "command.status";
    readonly payload: {
      readonly targetRequestId: Uuid;
      /** 协商 `resource.stateless-origin.v1` 后必填：直接向原 Owner 查询，不凭本地记录判断。 */
      readonly remoteTarget?: RemoteTarget;
    };
  };
}

/** `command` 消息的 `body` 联合（`$defs.command.body` 的 `oneOf`）。 */
export type CommandBody =
  | SessionListCommand["body"]
  | SessionCreateCommand["body"]
  | WorkspaceMkdirCommand["body"]
  | WorkspaceBrowseCommand["body"]
  | SessionReadCommand["body"]
  | SessionPromptCommand["body"]
  | SessionCancelCommand["body"]
  | ConfigListCommand["body"]
  | ConfigSetCommand["body"]
  | ModeListCommand["body"]
  | ModeSetCommand["body"]
  | PermissionResolveCommand["body"]
  | ElicitationRespondCommand["body"]
  | CommandStatusCommand["body"]
  | SessionFilesListCommand["body"]
  | SessionFilesReadCommand["body"]
  | SessionGitStatusCommand["body"]
  | SessionGitDiffCommand["body"];

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
 *
 * `historyHasEarlier` 是可选布尔值，**只**描述 `resources.history` 这一页之前是否仍有更早
 * 条目（对应的续取游标是最早一条的 `sessionSequence`）。两者各自分页、互不影响（D2）。
 *
 * `readCursor`/`sessionSummary`/`messageCoverage` 是 `resource.stateless-origin.v1` 的加性字段
 * （§9.7/§11.5）：协商后由 Owner 在**同一次**只读事务里原子取得；未协商时三者一起缺席，页边界
 * 与既有字段逐字不变。
 */
export interface SessionReadResult {
  readonly sessionId: Uuid;
  readonly resources: {
    readonly messages?: readonly SnapshotItemMessages[];
    readonly turns?: readonly SnapshotItemTurns[];
    /** 服务端本次读取时确认过的**当前有效**请求（含 `requestVersion`/`options`/`schema`）。 */
    readonly pending_interactions?: readonly EffectiveInteraction[];
    readonly config_options?: readonly SnapshotItemConfigOptions[];
    readonly capabilities?: readonly SnapshotItemCapabilities[];
    /** 结构化历史条目：与实时事件同形的 `view`，见 `sync.ts` 的 `SnapshotItemHistory`。 */
    readonly history?: readonly SnapshotItemHistory[];
  };
  readonly hasEarlier: boolean;
  readonly historyHasEarlier?: boolean;
  /** 本次读取原子捕获的来源位置 R，只描述这一页的读取边界。 */
  readonly readCursor?: OriginCursor;
  /** 与 `readCursor` 同次读取取得的完整会话摘要，用于防止清单与来源屏障之间的竞态。 */
  readonly sessionSummary?: SessionSummary;
  /** 每条返回消息恰有一项；`through` 等于 `readCursor`。 */
  readonly messageCoverage?: readonly MessageCoverage[];
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

/**
 * `$defs.workspaceBrowseResult`：`completed` 的结果。
 *
 * `path` **逐字回显**请求的父路径（`null` = 根列表），客户端 MUST NOT 把它当权威路径；
 * `entries` 只含该层的直接子目录，不含文件条目、内容、大小、时间戳或权限，按名称以确定顺序
 * 排列；单层条目数上限是固定 v1 常量 128（`docs/SYNC_PROTOCOL.md` §14），超过上限时只返回
 * 上限内的条目并把 `truncated` 置真——不得静默丢弃、不得整次失败。
 */
export interface WorkspaceBrowseResult {
  readonly path: WorkspaceBrowsePath | null;
  readonly entries: readonly WorkspaceBrowseEntry[];
  readonly truncated: boolean;
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
 * 按 `command` 收窄的运行期读取器在 `src/sync-client/`（例如 `isCommandStatusRecord`），
 * 而不是在协议镜像层凭空解析 `result`。
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
  | WorkspaceMkdirResult
  | WorkspaceBrowseResult
  | ConfigListResult
  | ModeListResult
  | CommandStatusRecord
  | SessionFilesListResult
  | SessionFilesReadResult
  | SessionGitStatusResult
  | SessionGitDiffResult
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
