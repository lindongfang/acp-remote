/**
 * WSS 消息的**严格**解码（`src/sync-client/` 层）。
 *
 * `docs/FRONTEND_DESIGN.md` §3 与 `design.md` D8 要求「协议载荷 MUST NOT 直接进入视图模型」，
 * 而 `src/protocol/` 只描述类型、不做运行期校验，因此**解码与逐字段判定是本层的责任**。
 *
 * ## 校验口径（为什么不是全量 JSON Schema）
 *
 * 这里校验的是**信封 + 判别字段 + 必填键的存在与基本类型**，即客户端逻辑真正依赖的部分：
 * 客户端要用 `type` 分派、用 `protocolVersion` 拒绝版本、用 `cursor`/`globalSequence`/`requestId`
 * 做连续性与幂等判定。任何一处放过都会直接变成「拿错误形状当正确形状用」。
 * 视图载荷内部的**值域**（例如某个事件 view 的可选字段组合）由 `schemas/sync/v1/**` 与
 * `projectView()` 的降级路径负责——那里已经为未知键保留了原始值，宽容解析反而会掩盖漂移。
 *
 * 明确不做的事：不做「猜测服务端语义」的兼容分支（`design.md` D8 的前端风险控制）。
 *
 * 解码失败一律返回结构化拒绝原因，**不抛异常**：上层要据此决定「丢弃并继续」还是
 * 「进入阻断态」，而不是被迫走 try/catch 控制流。
 */

import type {
  AuthAuthenticated,
  AuthClientHello,
  AuthClientProof,
  AuthServerChallenge,
  CommandResultMessage,
  CommandStatusRecord,
  Cursor,
  ErrorCode,
  ErrorMessage,
  EventMessage,
  SyncAck,
  SyncCaughtUp,
  SyncResetRequired,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  SyncSubscribe,
  SyncWireMessage,
} from "../protocol";

/** 解码失败的原因分类（供上层映射到连接状态机与诊断）。 */
export type WireRejectReason =
  | "invalid_json"
  | "not_an_object"
  | "protocol_version"
  | "unknown_type"
  | "missing_field"
  | "field_type"
  | "envelope_violation";

/** 解码成功。 */
export interface WireDecoded {
  readonly ok: true;
  readonly message: SyncWireMessage;
}

/** 解码失败。 */
export interface WireRejected {
  readonly ok: false;
  readonly reason: WireRejectReason;
  /** 只含结构信息，不含 wire 原文（原文可能含用户内容，不得进日志）。 */
  readonly detail: string;
}

export type WireDecodeResult = WireDecoded | WireRejected;

/** 解码后的 JSON 对象（索引签名，因此本文件按 `["key"]` 访问，见 tsconfig 的 `noPropertyAccessFromIndexSignature`）。 */
type JsonObject = Record<string, unknown>;

/** `protocolVersion` 是 v1 封闭值（`schemas/sync/v1/*` 的 `const 1`）。 */
const PROTOCOL_VERSION = 1;

/** `$defs.uuid` 的形状。 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** `$defs.decimalString`：`^(0|[1-9][0-9]*)$`。 */
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/;

/** `$defs.timestamp` 的固定形状（Daemon 持久化时间）。 */
const TIMESTAMP_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/;

/** 错误码形状：`family.reason`；值域由 `errors.json` 门禁保证，本层只判形状。 */
const ERROR_CODE_PATTERN = /^[a-z]+\.[a-z_]+$/;

/** 认证后消息：这些必须带 `connectionId` 与 `connectionSequence`（`$defs.postAuthBase`）。 */
const POST_AUTH_TYPES: Record<string, true> = {
  "auth.authenticated": true,
  "sync.subscribe": true,
  "sync.caught_up": true,
  "sync.reset_required": true,
  "sync.snapshot_request": true,
  "sync.snapshot_begin": true,
  "sync.snapshot_chunk": true,
  "sync.snapshot_end": true,
  "sync.ack": true,
  "event": true,
  "command": true,
  "command.result": true,
  "error": true,
  "control.ping": true,
  "control.pong": true,
};

/** 认证前消息：这些**不得**带 `connectionId`（`$defs.preAuthBase`）。 */
const PRE_AUTH_TYPES: Record<string, true> = {
  "auth.client_hello": true,
  "auth.server_challenge": true,
  "auth.client_proof": true,
};

/** `$defs.resetRequired.reason` 与 `$defs.snapshotRequest.body.reason` 的封闭词表。 */
const RESET_REASONS: Record<string, true> = {
  initial_sync: true,
  epoch_mismatch: true,
  cursor_expired: true,
  cache_incompatible: true,
};

/** `$defs.commandName`。 */
const COMMAND_NAMES: Record<string, true> = {
  "session.list": true,
  "session.read": true,
  "command.status": true,
  "session.mode.list": true,
  "session.config.list": true,
  "session.prompt": true,
  "session.cancel": true,
  "elicitation.respond": true,
  "session.mode.set": true,
  "session.config.set": true,
  "permission.resolve": true,
  "session.create": true,
};

/** `$defs.commandResult.body.status`。 */
const RESULT_STATUSES: Record<string, true> = {
  accepted: true,
  completed: true,
  failed: true,
  rejected: true,
  uncertain: true,
};

/** `$defs.commandStatusRecord.state`。 */
const STATUS_RECORD_STATES: Record<string, true> = {
  accepted: true,
  completed: true,
  failed: true,
  uncertain: true,
  rejected: true,
};

/** `$defs.eventOrigin.kind`。 */
const EVENT_ORIGIN_KINDS: Record<string, true> = {
  agent: true,
  device: true,
  daemon: true,
  local_cli: true,
};

/** `event.body.eventType` 是开放字符串（未知类型走降级卡片），但必须非空。 */
const EVENT_TYPE_PATTERN = /^[a-z]+(\.[a-z_]+)+$/;

function reject(reason: WireRejectReason, detail: string): WireRejected {
  return { ok: false, reason, detail };
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 类型守卫：canonical UUID 文本（`$defs.uuid`）。 */
function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

/** 类型守卫：`decimalString`。 */
function isDecimalString(value: unknown): value is string {
  return typeof value === "string" && DECIMAL_PATTERN.test(value);
}

/** 类型守卫：非空字符串（feature id、nonce、digest、事件类型都要求非空）。 */
function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

/** 类型守卫：`$defs.timestamp`。 */
function isTimestamp(value: unknown): value is string {
  return typeof value === "string" && TIMESTAMP_PATTERN.test(value);
}

/** `Cursor`：两键都必填且都是 decimalString（§9.1）。 */
export function isCursor(value: unknown): value is Cursor {
  if (!isObject(value)) return false;
  return isUuid(value["serverEpoch"]) && isDecimalString(value["globalSequence"]);
}

/** 两个 cursor 是否逐字段相等（快照 begin/end 的 cursor 必须一致，§9.4）。 */
export function cursorsEqual(left: Cursor, right: Cursor): boolean {
  return left.serverEpoch === right.serverEpoch && left.globalSequence === right.globalSequence;
}

/** `hasKeys` 在本文件有数十个调用点：必填键的存在性判定需要 lockstep 行为（先全查再判定）。 */
function hasKeys(value: JsonObject, keys: readonly string[]): boolean {
  return keys.every((key) => key in value);
}

/** 对 `sync.*` 消息：body 必须是对象，再交给各自的判定。 */
function withBody(
  parsed: JsonObject,
  body: unknown,
  check: (message: SyncWireMessage, body: JsonObject) => WireDecodeResult,
): WireDecodeResult {
  if (!isObject(body)) return reject("missing_field", "缺少 body 对象");
  return check(parsed as unknown as SyncWireMessage, body);
}

/** 认证前 body 的必填键判定（`auth.client_hello` / `.server_challenge` / `.client_proof`）。 */
function preAuthRequired(value: JsonObject, keys: readonly string[]): WireRejected | null {
  return hasKeys(value, keys) ? null : reject("missing_field", `缺必填字段：${keys.join(",")}`);
}

/**
 * 解码一条 WSS 文本消息。
 *
 * @param text socket 交付的原始 UTF-8 文本（**未经 JSON 预解析**，快照 digest 需要原始字节）。
 */
export function decodeWireMessage(text: string): WireDecodeResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text) as unknown;
  } catch {
    return reject("invalid_json", "不是合法 JSON");
  }
  if (!isObject(parsed)) return reject("not_an_object", "顶层不是对象");

  if (parsed["protocolVersion"] !== PROTOCOL_VERSION) {
    return reject("protocol_version", `protocolVersion=${String(parsed["protocolVersion"])}`);
  }
  const typeValue = parsed["type"];
  if (!isNonEmptyString(typeValue)) return reject("field_type", "type 缺失或非字符串");
  const type = typeValue;

  if (!isUuid(parsed["messageId"])) return reject("field_type", "messageId 不是 UUID");
  if (type in POST_AUTH_TYPES) {
    if (!isUuid(parsed["connectionId"])) return reject("envelope_violation", `${type} 缺 connectionId`);
    if (!isDecimalString(parsed["connectionSequence"])) {
      return reject("envelope_violation", `${type} 的 connectionSequence 不是 decimalString`);
    }
  } else if (type in PRE_AUTH_TYPES) {
    if ("connectionId" in parsed || "connectionSequence" in parsed) {
      return reject("envelope_violation", `${type} 是认证前消息，不得带 connectionId/connectionSequence`);
    }
  }

  return decodeByType(parsed, type, parsed["body"]);
}

/** 按 `type` 分派到各自的必填键判定。 */
function decodeByType(parsed: JsonObject, type: string, body: unknown): WireDecodeResult {
  switch (type) {
    case "auth.client_hello":
      return decodeClientHello(parsed, body);
    case "auth.server_challenge":
      return decodeServerChallenge(parsed, body);
    case "auth.client_proof":
      return decodeClientProof(parsed, body);
    case "auth.authenticated":
      return decodeAuthenticated(parsed, body);
    case "sync.subscribe":
      return decodeSubscribe(parsed, body);
    case "sync.caught_up":
      return decodeCaughtUp(parsed, body);
    case "sync.reset_required":
      return decodeResetRequired(parsed, body);
    case "sync.snapshot_request":
      return decodeSnapshotRequest(parsed, body);
    case "sync.snapshot_begin":
      return decodeSnapshotBegin(parsed, body);
    case "sync.snapshot_chunk":
      return decodeSnapshotChunk(parsed, body);
    case "sync.snapshot_end":
      return decodeSnapshotEnd(parsed, body);
    case "sync.ack":
      return decodeAck(parsed, body);
    case "event":
      return decodeEvent(parsed, body);
    case "command":
      return decodeCommand(parsed, body);
    case "command.result":
      return decodeCommandResult(parsed, body);
    case "error":
      return decodeError(parsed, body);
    case "control.ping":
    case "control.pong":
      return decodeControl(parsed, body);
    default:
      return reject("unknown_type", `未登记的 type：${type}`);
  }
}

function decodeSubscribe(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["cursor", "scope"])) return reject("missing_field", "subscribe 缺 cursor/scope");
    const cursor = inner["cursor"];
    if (cursor !== null && !isCursor(cursor)) return reject("field_type", "cursor 形状非法");
    if (inner["scope"] !== "machine") return reject("field_type", "scope 只允许 machine");
    return { ok: true, message: message as SyncSubscribe };
  });
}

function decodeCaughtUp(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["cursor"])) return reject("missing_field", "caught_up 缺 cursor");
    if (!isCursor(inner["cursor"])) return reject("field_type", "caught_up 的 cursor 形状非法");
    return { ok: true, message: message as SyncCaughtUp };
  });
}

function decodeResetRequired(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["reason", "snapshotAvailable"])) {
      return reject("missing_field", "reset_required 缺 reason/snapshotAvailable");
    }
    const reason = inner["reason"];
    if (!isNonEmptyString(reason) || !(reason in RESET_REASONS)) {
      return reject("field_type", "reset_required 的 reason 不在封闭词表内");
    }
    if (typeof inner["snapshotAvailable"] !== "boolean") {
      return reject("field_type", "snapshotAvailable 不是布尔");
    }
    return { ok: true, message: message as SyncResetRequired };
  });
}

function decodeSnapshotRequest(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["reason"])) return reject("missing_field", "snapshot_request 缺 reason");
    const reason = inner["reason"];
    if (!isNonEmptyString(reason) || !(reason in RESET_REASONS)) {
      return reject("field_type", "snapshot_request 的 reason 不在封闭词表内");
    }
    return { ok: true, message };
  });
}

/** `chunkCount` 是 `1..3` 的整数（§9.4：三类清单资源，`sessions` chunk 恒发）。 */
function isChunkCount(value: unknown): value is 1 | 2 | 3 {
  return value === 1 || value === 2 || value === 3;
}

function decodeSnapshotBegin(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["snapshotId", "cursor", "schemaVersion", "chunkCount"] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `snapshot_begin 缺必填字段：${keys.join(",")}`);
    if (!isUuid(inner["snapshotId"])) return reject("field_type", "snapshotId 不是 UUID");
    if (!isCursor(inner["cursor"])) return reject("field_type", "snapshot_begin 的 cursor 形状非法");
    if (inner["schemaVersion"] !== 1) return reject("field_type", "snapshot schemaVersion 只允许 1");
    if (!isChunkCount(inner["chunkCount"])) return reject("field_type", "chunkCount 必须是 1..3 的整数");
    return { ok: true, message: message as SyncSnapshotBegin };
  });
}

/**
 * `sync.snapshot_chunk`：资源种类是封闭词表（D1 只有清单三类），且元素形状与资源绑定。
 *
 * 五类会话明细资源（`messages`/`turns`/`pending_interactions`/`config_options`/`capabilities`）
 * 在这里被**明确拒绝**——客户端不会为它们建暂存区，即使某个服务端发来（§9.4 的 MUST NOT）。
 */
function decodeSnapshotChunk(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["snapshotId", "chunkIndex", "resource", "items"] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `snapshot_chunk 缺必填字段：${keys.join(",")}`);
    if (!isUuid(inner["snapshotId"])) return reject("field_type", "snapshotId 不是 UUID");
    if (!isDecimalString(inner["chunkIndex"])) return reject("field_type", "chunkIndex 不是 decimalString");
    const items = inner["items"];
    if (!Array.isArray(items)) return reject("field_type", "items 不是数组");
    const resource = inner["resource"];
    if (resource !== "sessions" && resource !== "workspaces" && resource !== "agents") {
      return reject("field_type", `snapshot resource 只允许 sessions/workspaces/agents，收到 ${String(resource)}`);
    }
    for (const item of items) {
      if (!isObject(item)) return reject("field_type", "snapshot 元素不是对象");
      if (resource === "workspaces" && !hasKeys(item, ["alias", "displayName"])) {
        return reject("field_type", "workspaces 元素缺 alias/displayName");
      }
      if (resource === "agents" && !hasKeys(item, ["agentId", "displayName", "default"])) {
        return reject("field_type", "agents 元素缺 agentId/displayName/default");
      }
    }
    return { ok: true, message: message as SyncSnapshotChunk };
  });
}

function decodeSnapshotEnd(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["snapshotId", "cursor", "chunkCount", "snapshotDigest"] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `snapshot_end 缺必填字段：${keys.join(",")}`);
    if (!isUuid(inner["snapshotId"])) return reject("field_type", "snapshotId 不是 UUID");
    if (!isCursor(inner["cursor"])) return reject("field_type", "snapshot_end 的 cursor 形状非法");
    if (!isChunkCount(inner["chunkCount"])) return reject("field_type", "chunkCount 必须是 1..3 的整数");
    if (!isNonEmptyString(inner["snapshotDigest"])) return reject("field_type", "snapshotDigest 不是字符串");
    return { ok: true, message: message as SyncSnapshotEnd };
  });
}

function decodeAck(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["cursor"])) return reject("missing_field", "ack 缺 cursor");
    if (!isCursor(inner["cursor"])) return reject("field_type", "ack 的 cursor 形状非法");
    return { ok: true, message: message as SyncAck };
  });
}

function decodeEvent(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = [
      "globalSequence",
      "sessionSequence",
      "eventId",
      "sessionId",
      "eventType",
      "causationRequestId",
      "origin",
      "remoteOrigin",
      "createdAt",
      "payload",
    ] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `event 缺必填字段：${keys.join(",")}`);
    if (!isDecimalString(inner["globalSequence"])) {
      return reject("field_type", "globalSequence 不是 decimalString");
    }
    const sessionSequence = inner["sessionSequence"];
    if (sessionSequence !== null && !isDecimalString(sessionSequence)) {
      return reject("field_type", "sessionSequence 既非 null 也非 decimalString");
    }
    if (!isUuid(inner["eventId"])) return reject("field_type", "eventId 不是 UUID");
    const sessionId = inner["sessionId"];
    if (sessionId !== null && !isUuid(sessionId)) return reject("field_type", "sessionId 形状非法");
    const eventType = inner["eventType"];
    if (!isNonEmptyString(eventType) || !EVENT_TYPE_PATTERN.test(eventType)) {
      return reject("field_type", "eventType 不是 `family.reason` 形状");
    }
    const causationRequestId = inner["causationRequestId"];
    if (causationRequestId !== null && !isUuid(causationRequestId)) {
      return reject("field_type", "causationRequestId 形状非法");
    }
    const origin = inner["origin"];
    if (!isObject(origin) || !hasKeys(origin, ["kind", "deviceId"])) {
      return reject("field_type", "event.origin 缺 kind/deviceId");
    }
    const originKind = origin["kind"];
    if (!isNonEmptyString(originKind) || !(originKind in EVENT_ORIGIN_KINDS)) {
      return reject("field_type", "event.origin.kind 不在封闭词表内");
    }
    const originDeviceId = origin["deviceId"];
    if (originDeviceId !== null && !isUuid(originDeviceId)) {
      return reject("field_type", "event.origin.deviceId 既非 null 也非 UUID");
    }
    const remoteOrigin = inner["remoteOrigin"];
    if (remoteOrigin !== null && !isObject(remoteOrigin)) {
      return reject("field_type", "remoteOrigin 既非 null 也非对象");
    }
    if (!isTimestamp(inner["createdAt"])) return reject("field_type", "createdAt 不是 timestamp");
    const payload = inner["payload"];
    if (!isObject(payload) || !hasKeys(payload, ["view"]) || !isObject(payload["view"])) {
      return reject("field_type", "payload.view 必须是对象");
    }
    return { ok: true, message: message as EventMessage };
  });
}

/**
 * `command`：服务端不会向客户端下发命令，本层只做形状校验以便诊断——
 * 收到它说明对端角色搞错了，按「未登记 type」之外的可辨识形状上报。
 */
function decodeCommand(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["requestId", "command"])) return reject("missing_field", "command 缺 requestId/command");
    if (!isUuid(inner["requestId"])) return reject("field_type", "requestId 不是 UUID");
    const command = inner["command"];
    if (!isNonEmptyString(command) || !(command in COMMAND_NAMES)) {
      return reject("field_type", "command 不在封闭词表内");
    }
    return { ok: true, message };
  });
}

function decodeCommandResult(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["requestId", "command", "status", "acceptedAt", "terminalEventId", "result", "error"] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `command.result 缺必填字段：${keys.join(",")}`);
    if (!isUuid(inner["requestId"])) return reject("field_type", "requestId 不是 UUID");
    const command = inner["command"];
    if (!isNonEmptyString(command) || !(command in COMMAND_NAMES)) {
      return reject("field_type", "command 不在封闭词表内");
    }
    const status = inner["status"];
    if (!isNonEmptyString(status) || !(status in RESULT_STATUSES)) {
      return reject("field_type", "status 不在封闭词表内");
    }
    // §11.2：`rejected` 的 `acceptedAt` 固定为 null 且必须带结构化 error；
    // 其余状态都必须是该命令首次被持久化接受的时间——这条判定同时支撑
    // 「MUST NOT 在服务端确认已接受之前把命令显示为已被接受」。
    if (status === "rejected") {
      if (inner["acceptedAt"] !== null) return reject("field_type", "rejected 的 acceptedAt 必须为 null");
      if (!isObject(inner["error"])) return reject("field_type", "rejected 必须带结构化 error");
    } else if (!isTimestamp(inner["acceptedAt"])) {
      return reject("field_type", "非 rejected 状态必须带 acceptedAt timestamp");
    }
    const error = inner["error"];
    if (error !== null && !isObject(error)) return reject("field_type", "error 既非 null 也非对象");
    const terminalEventId = inner["terminalEventId"];
    if (terminalEventId !== null && !isUuid(terminalEventId)) {
      return reject("field_type", "terminalEventId 形状非法");
    }
    return { ok: true, message: message as CommandResultMessage };
  });
}

function decodeError(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["code", "message", "retryable", "correlationId", "details"] as const;
    if (!hasKeys(inner, keys)) return reject("missing_field", `error 缺必填字段：${keys.join(",")}`);
    const code = inner["code"];
    if (!isNonEmptyString(code) || !ERROR_CODE_PATTERN.test(code)) {
      return reject("field_type", "error.code 不在错误码形状内");
    }
    if (!isNonEmptyString(inner["message"])) return reject("field_type", "error.message 非空字符串");
    if (typeof inner["retryable"] !== "boolean") return reject("field_type", "error.retryable 不是布尔");
    const correlationId = inner["correlationId"];
    if (correlationId !== null && !isUuid(correlationId)) {
      return reject("field_type", "correlationId 形状非法");
    }
    if (!isObject(inner["details"])) return reject("field_type", "error.details 不是对象");
    return { ok: true, message: message as ErrorMessage };
  });
}

function decodeControl(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["nonce"])) return reject("missing_field", "control 消息缺 nonce");
    if (!isNonEmptyString(inner["nonce"])) return reject("field_type", "nonce 不是非空字符串");
    return { ok: true, message };
  });
}

function decodeClientHello(parsed: JsonObject, body: unknown): WireDecodeResult {
  if (!isObject(body)) return reject("missing_field", "client_hello 缺 body");
  const keys = [
    "minProtocolVersion",
    "maxProtocolVersion",
    "hostId",
    "deviceId",
    "clientKind",
    "clientNonce",
    "supportedFeatures",
    "requiredFeatures",
  ] as const;
  const missing = preAuthRequired(body, keys);
  if (missing) return missing;
  const min = body["minProtocolVersion"];
  const max = body["maxProtocolVersion"];
  if (typeof min !== "number" || typeof max !== "number" || !Number.isInteger(min) || !Number.isInteger(max)) {
    return reject("field_type", "协议版本区间必须是整数");
  }
  if (min > max) return reject("field_type", "minProtocolVersion 不得大于 maxProtocolVersion");
  if (!isUuid(body["hostId"]) || !isUuid(body["deviceId"])) return reject("field_type", "hostId/deviceId 不是 UUID");
  if (!isNonEmptyString(body["clientNonce"])) return reject("field_type", "clientNonce 非法");
  if (!Array.isArray(body["supportedFeatures"]) || !Array.isArray(body["requiredFeatures"])) {
    return reject("field_type", "feature 列表必须是数组");
  }
  return { ok: true, message: parsed as unknown as AuthClientHello };
}

function decodeServerChallenge(parsed: JsonObject, body: unknown): WireDecodeResult {
  if (!isObject(body)) return reject("missing_field", "server_challenge 缺 body");
  const keys = [
    "selectedProtocolVersion",
    "hostId",
    "connectionId",
    "serverNonce",
    "selectedFeatures",
    "hostProof",
  ] as const;
  const missing = preAuthRequired(body, keys);
  if (missing) return missing;
  if (typeof body["selectedProtocolVersion"] !== "number") {
    return reject("field_type", "selectedProtocolVersion 非 number");
  }
  if (!isUuid(body["hostId"]) || !isUuid(body["connectionId"])) {
    return reject("field_type", "hostId/connectionId 非 UUID");
  }
  if (!isNonEmptyString(body["serverNonce"])) return reject("field_type", "serverNonce 非法");
  if (!Array.isArray(body["selectedFeatures"])) return reject("field_type", "selectedFeatures 非数组");
  if (!isNonEmptyString(body["hostProof"])) return reject("field_type", "hostProof 非法");
  return { ok: true, message: parsed as unknown as AuthServerChallenge };
}

function decodeClientProof(parsed: JsonObject, body: unknown): WireDecodeResult {
  if (!isObject(body)) return reject("missing_field", "client_proof 缺 body");
  const keys = ["connectionId", "deviceId", "deviceProof"] as const;
  const missing = preAuthRequired(body, keys);
  if (missing) return missing;
  if (!isUuid(body["connectionId"]) || !isUuid(body["deviceId"])) {
    return reject("field_type", "connectionId/deviceId 非 UUID");
  }
  if (!isNonEmptyString(body["deviceProof"])) return reject("field_type", "deviceProof 非法");
  return { ok: true, message: parsed as unknown as AuthClientProof };
}

function decodeAuthenticated(parsed: JsonObject, body: unknown): WireDecodeResult {
  if (!isObject(body)) return reject("missing_field", "authenticated 缺 body");
  const keys = [
    "deviceId",
    "scopes",
    "serverEpoch",
    "headGlobalSequence",
    "heartbeatIntervalMs",
    "limits",
  ] as const;
  const missing = preAuthRequired(body, keys);
  if (missing) return missing;
  if (!isUuid(body["deviceId"])) return reject("field_type", "deviceId 非 UUID");
  const scopes = body["scopes"];
  if (!Array.isArray(scopes) || scopes.some((scope) => typeof scope !== "string")) {
    return reject("field_type", "scopes 必须是字符串数组");
  }
  if (!isUuid(body["serverEpoch"])) return reject("field_type", "serverEpoch 非 UUID");
  if (!isDecimalString(body["headGlobalSequence"])) {
    return reject("field_type", "headGlobalSequence 非 decimalString");
  }
  if (typeof body["heartbeatIntervalMs"] !== "number") return reject("field_type", "heartbeatIntervalMs 非 number");
  const limits = body["limits"];
  const limitKeys = ["maxMessageBytes", "maxPromptBytes", "maxReplayEventsPerBatch"] as const;
  if (!isObject(limits) || !hasKeys(limits, limitKeys)) return reject("field_type", "limits 缺必填字段");
  for (const key of limitKeys) {
    if (typeof limits[key] !== "number") return reject("field_type", `limits.${key} 非 number`);
  }
  return { ok: true, message: parsed as unknown as AuthAuthenticated };
}

/**
 * `CommandStatusRecord` 的运行期收窄（`command.status` 的完成结果）。
 *
 * 导出是因为上层要据此把 `submitting` 推进到终态，**不能**只信 `state` 字符串。
 */
export function isCommandStatusRecord(value: unknown): value is CommandStatusRecord {
  if (!isObject(value)) return false;
  if (!isUuid(value["targetRequestId"])) return false;
  const state = value["state"];
  if (!isNonEmptyString(state) || !(state in STATUS_RECORD_STATES)) return false;
  const keys = ["acceptedAt", "terminalAt", "terminalEventId", "result", "error"] as const;
  return hasKeys(value, keys);
}

/** `ErrorCode` 的运行期守卫：形状判定（值域由 `errors.json` 的门禁保证）。 */
export function isErrorCode(value: unknown): value is ErrorCode {
  return isNonEmptyString(value) && ERROR_CODE_PATTERN.test(value);
}