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
  EventOriginKind,
  OriginCursor,
  SessionRef,
  SyncAck,
  SyncCaughtUp,
  SyncRemoteAck,
  SyncRemoteCatalogChanged,
  SyncRemoteCaughtUp,
  SyncRemoteEvent,
  SyncRemoteResetRequired,
  SyncRemoteSourceState,
  SyncRemoteSubscribe,
  SyncRemoteUnavailable,
  SyncRemoteUnsubscribe,
  SyncResetRequired,
  SyncSessionView,
  SyncSessionViewResult,
  SyncSessionPreparation,
  SyncSessionPreparationResult,
  SyncSnapshotBegin,
  SyncSnapshotChunk,
  SyncSnapshotEnd,
  SyncSubscribe,
  SyncWireMessage,
} from "../protocol";
import { isLocalActivity } from "../protocol";

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
  // `core.local-session-lifecycle.v1` 的查看控制帧（§11.8）：只在已认证连接上合法。
  "session.view": true,
  "session.view.result": true,
  // `core.session-preparation.v1` 的准备控制面（§11.9）：只在已认证连接上合法，未协商该 feature
  // 的连接既不发送也不接收它们。
  "session.preparation": true,
  "session.preparation.result": true,
  // `resource.stateless-origin.v1` 的远程来源流（§9.7）：九条消息都只在认证后出现，信封与其余
  // post-auth 帧逐字相同（`$defs.postAuthBase`）。
  "sync.remote_subscribe": true,
  "sync.remote_event": true,
  "sync.remote_caught_up": true,
  "sync.remote_ack": true,
  "sync.remote_reset_required": true,
  "sync.remote_unsubscribe": true,
  "sync.remote_unavailable": true,
  "sync.remote_catalog_changed": true,
  "sync.remote_source_state": true,
  "event": true,
  "command": true,
  "command.result": true,
  "control.ping": true,
  "control.pong": true,
  // `error` 不在此表：它是 preAuthBase 与 postAuthBase 的 oneOf，单独判定（见 decodeWireMessage）。
};

/** 认证前消息：这些**不得**带 `connectionId`（`$defs.preAuthBase`）。 */
const PRE_AUTH_TYPES: Record<string, true> = {
  "auth.client_hello": true,
  "auth.server_challenge": true,
  "auth.client_proof": true,
};

/**
 * `$defs.remoteResetRequired.body.reason`（§9.7）：复用 reset 词表，但**不含** `initial_sync`。
 *
 * `cache_incompatible` 只经这一帧到达客户端——`sync.cursor_invalid` 的 `details.reason` 没有该取值。
 */
const REMOTE_RESET_REASONS: Record<string, true> = {
  epoch_mismatch: true,
  cursor_expired: true,
  cache_incompatible: true,
};

/** `$defs.resetRequired.reason` 与 `$defs/snapshotRequest.body.reason` 的封闭词表。 */
const RESET_REASONS: Record<string, true> = {
  initial_sync: true,
  ...REMOTE_RESET_REASONS,
};

/** `$defs.remoteSourceState.body.state`。 */
const REMOTE_SOURCE_STATES: Record<string, true> = {
  online: true,
  offline: true,
  revoked: true,
};

/** `$defs.remoteEvent` 的 `event.eventType`：`^[a-z0-9_.-]{1,128}$`。 */
const REMOTE_EVENT_TYPE_PATTERN = /^[a-z0-9_.-]{1,128}$/;

/** `$defs.commandName`：只含 `transport` 包含 `sync` 的命令（与 `commands.json` 同集合，21 条）。 */
const COMMAND_NAMES: Record<string, true> = {
  // `session.start`（`core.session-preparation.v1`，§11.9）是 Sync-only 的首条提交 mutation：
  // `kind: initial` 保存并派发首条，`kind: continue` 只做显式继续。
  "session.start": true,
  "session.list": true,
  "session.read": true,
  "session.title.set": true,
  "command.status": true,
  "session.mode.list": true,
  "session.config.list": true,
  "workspace.browse": true,
  "workspace.mkdir": true,
  "session.prompt": true,
  "session.cancel": true,
  "elicitation.respond": true,
  "session.mode.set": true,
  "session.config.set": true,
  "permission.resolve": true,
  "session.create": true,
  "session.resume": true,
  "session.files.list": true,
  "session.files.read": true,
  "session.git.status": true,
  "session.git.diff": true,
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

/**
 * `$defs.eventOrigin.kind` 的运行期投影。
 *
 * 声明为 `Record<EventOriginKind, true>`：`EventOriginKind` 已被
 * `clients/app/src/protocol/contract.test.ts` 的编译期镜像钉在
 * `schemas/sync/v1/event.schema.json` 的 enum 上，因此本表与 schema 之间形成
 * 「schema → 类型 → 运行期词表」的闭合链——类型新增取值而本表未跟上会直接编译失败。
 */
const EVENT_ORIGIN_KINDS: Record<EventOriginKind, true> = {
  agent: true,
  device: true,
  daemon: true,
  local_cli: true,
  node: true,
};

/** `event.body.eventType` 是开放字符串（未知类型走降级卡片），但必须非空。 */
const EVENT_TYPE_PATTERN = /^[a-z]+(\.[a-z_]+)+$/;

/** `$defs.preparationAction` 的闭合词表（§11.9）。 */
const PREPARATION_ACTIONS: Record<string, true> = {
  allocate: true,
  restore: true,
  configure: true,
  renew: true,
  release: true,
};

/** `$defs.sessionPreparationResult.body.status` 的闭合五态。 */
const PREPARATION_RESULT_STATUSES: Record<string, true> = {
  ready: true,
  committed: true,
  stale: true,
  released: true,
  failed: true,
};

/** `session.preparation.result.body.reason`（`status = "stale"`）的闭合词表。 */
const PREPARATION_STALE_REASONS: Record<string, true> = {
  runtime_epoch_mismatch: true,
  generation_mismatch: true,
  released: true,
};

/**
 * `$defs.preparationReceiptClaims` 的闭合键集合（字段顺序即 transcript tag 顺序）。
 *
 * 与 `src/protocol/attachment.ts` 的同一份集合逐项相同；这一层只判 **wire 形状**：
 * 签名有效性由 Owner 用专用 transcript domain 判定，客户端不验签、也不把它当登录凭证。
 */
const PREPARATION_RECEIPT_CLAIM_KEYS = [
  "version",
  "ownerNodeId",
  "storeEpoch",
  "origin",
  "deviceId",
  "tabNonce",
  "sessionId",
  "agentId",
  "normalizedWorkspacePath",
  "workspaceSource",
] as const;

/** 32 字节无填充 base64url 的字符数；64 字节 P1363 签名的字符数。 */
const BASE64URL_32_CHARS = 43;
const BASE64URL_64_CHARS = 86;
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;

/** `$defs.positiveDecimalString`：无前导零且大于零（`generation` 的 1..=2^63−1）。 */
function isPositiveDecimalString(value: unknown): value is string {
  return isDecimalString(value) && value !== "0";
}

function isBase64UrlOfLength(value: unknown, length: number): value is string {
  return typeof value === "string" && value.length === length && BASE64URL_PATTERN.test(value);
}

/** `$defs.preparationReceipt`：闭合 claims + 64 字节 P1363 签名（只判形状）。 */
function isPreparationReceipt(value: unknown): boolean {
  if (!isObject(value) || !hasExactKeys(value, ["claims", "signature"])) return false;
  const claims = value["claims"];
  if (!isObject(claims) || !hasExactKeys(claims, PREPARATION_RECEIPT_CLAIM_KEYS)) return false;
  if (claims["version"] !== 1) return false;
  for (const key of ["ownerNodeId", "storeEpoch", "deviceId", "sessionId"] as const) {
    if (!isUuid(claims[key])) return false;
  }
  const origin = claims["origin"];
  if (!isNonEmptyString(origin) || origin.length > 255) return false;
  if (!isBase64UrlOfLength(claims["tabNonce"], BASE64URL_32_CHARS)) return false;
  const agentId = claims["agentId"];
  if (!isNonEmptyString(agentId) || agentId.length > 128) return false;
  const workspacePath = claims["normalizedWorkspacePath"];
  if (!isNonEmptyString(workspacePath) || workspacePath.length > 4096) return false;
  const source = claims["workspaceSource"];
  if (source !== "directoryId" && source !== "workspacePath") return false;
  return isBase64UrlOfLength(value["signature"], BASE64URL_64_CHARS);
}

/** `$defs.preparationWorkspaceRef`：`directoryId` / `workspacePath` **恰好一个**。 */
function isPreparationWorkspaceRef(value: unknown): boolean {
  if (!isObject(value)) return false;
  const hasDirectory = Object.hasOwn(value, "directoryId");
  const hasPath = Object.hasOwn(value, "workspacePath");
  if (hasDirectory === hasPath) return false;
  if (hasDirectory) {
    return hasExactKeys(value, ["directoryId"]) && isNonEmptyString(value["directoryId"]);
  }
  const path = value["workspacePath"];
  return hasExactKeys(value, ["workspacePath"]) && isNonEmptyString(path) && path.length <= 4096;
}

/** 只允许封闭对象的**这些键**（`additionalProperties: false`，但没有"恰好"要求）。 */
function hasOnlyKeys(value: JsonObject, keys: readonly string[]): boolean {
  return Object.keys(value).every((key) => keys.includes(key));
}

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

/** 类型守卫：`decimalString`。导出是为了让 `EventLedger` 复用同一份判定，避免账本与解码器对序号形状各说各话。 */
export function isDecimalString(value: unknown): value is string {
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

/** `$defs.originCursor`：Owner 来源位置，两个键都必填（§9.7）。 */
function isOriginCursor(value: unknown): value is OriginCursor {
  if (!isObject(value)) return false;
  return isUuid(value["originEpoch"]) && isDecimalString(value["originSequence"]);
}

/** `$defs.sessionRef`：`(ownerNodeId, sessionId)` 两键都必填且是 UUID。 */
function isSessionRef(value: unknown): value is SessionRef {
  if (!isObject(value)) return false;
  return isUuid(value["ownerNodeId"]) && isUuid(value["sessionId"]);
}

/**
 * `$defs.publicError`：`{code, message, retryable, details}` 四键必填，`details` 是开放对象。
 *
 * 注意与 `error` 消息的 body 区分：后者另有 `correlationId`，本类型没有。
 */
function isPublicError(value: unknown): boolean {
  if (!isObject(value)) return false;
  if (!hasKeys(value, ["code", "message", "retryable", "details"])) return false;
  const code = value["code"];
  const message = value["message"];
  return (
    isNonEmptyString(code) &&
    ERROR_CODE_PATTERN.test(code) &&
    typeof message === "string" &&
    message.length <= 1024 &&
    typeof value["retryable"] === "boolean" &&
    isObject(value["details"])
  );
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
  } else if (type === "error") {
    // `schemas/sync/v1/error.schema.json#/$defs/error` 是 preAuthBase 与 postAuthBase 的 `oneOf`：
    // 认证前省略 connectionId 与 connectionSequence（`docs/SYNC_PROTOCOL.md` §4.1），
    // 认证后两者都必填。因此这里要求两键**同时出现或同时缺席**。
    const hasConnectionId = "connectionId" in parsed;
    const hasConnectionSequence = "connectionSequence" in parsed;
    if (hasConnectionId !== hasConnectionSequence) {
      return reject("envelope_violation", "error 的 connectionId 与 connectionSequence 必须同时出现或同时缺席");
    }
    if (hasConnectionId) {
      if (!isUuid(parsed["connectionId"])) return reject("envelope_violation", "error 的 connectionId 不是 UUID");
      if (!isDecimalString(parsed["connectionSequence"])) {
        return reject("envelope_violation", "error 的 connectionSequence 不是 decimalString");
      }
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
    case "session.view":
      return decodeSessionView(parsed, body);
    case "session.view.result":
      return decodeSessionViewResult(parsed, body);
    case "session.preparation":
      return decodeSessionPreparation(parsed, body);
    case "session.preparation.result":
      return decodeSessionPreparationResult(parsed, body);
    case "sync.remote_subscribe":
      return decodeRemoteSubscribe(parsed, body);
    case "sync.remote_event":
      return decodeRemoteEvent(parsed, body);
    case "sync.remote_caught_up":
      return decodeRemoteCaughtUp(parsed, body);
    case "sync.remote_ack":
      return decodeRemoteAck(parsed, body);
    case "sync.remote_reset_required":
      return decodeRemoteResetRequired(parsed, body);
    case "sync.remote_unsubscribe":
      return decodeRemoteUnsubscribe(parsed, body);
    case "sync.remote_unavailable":
      return decodeRemoteUnavailable(parsed, body);
    case "sync.remote_catalog_changed":
      return decodeRemoteCatalogChanged(parsed, body);
    case "sync.remote_source_state":
      return decodeRemoteSourceState(parsed, body);
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
      if (resource === "workspaces") {
        if (!hasKeys(item, ["directoryId", "path"])) {
          return reject("missing_field", "workspaces 元素缺 directoryId/path");
        }
        // 旧形状（`alias`/`displayName`）与「路径不可见」的显式空值都按
        // `protocol.schema_invalid` 拒收整条 chunk：`workspaceCatalogEntry` 只认
        // `directoryId` 与非空的规范化 `path`，客户端不得猜测或补全缺失的身份。
        if ("alias" in item || "displayName" in item) {
          return reject("field_type", "workspaces 元素仍是旧 alias/displayName 形状");
        }
        if (!isNonEmptyString(item["directoryId"]) || !isNonEmptyString(item["path"])) {
          return reject("field_type", "workspaces 元素的 directoryId/path 必须是非空字符串");
        }
      }
      if (resource === "agents" && !hasKeys(item, ["agentId", "displayName", "default"])) {
        return reject("field_type", "agents 元素缺 agentId/displayName/default");
      }
      // `localActivity` 是可选键，但出现在摘要里就必须是封闭词表之一；未知取值（例如旧客户端
      // 不认识的未来状态）按协议错误拒收，而不是当成某个「看起来合理」的状态呈现。
      if (resource === "sessions" && "localActivity" in item && !isLocalActivity(item["localActivity"])) {
        return reject("field_type", "sessions 元素的 localActivity 不在 idle/active/pending 内");
      }
    }
    const normalized = items.map((item) => {
      if (!isObject(item)) return item;
      if (resource === "workspaces") {
        const directoryId = String(item["directoryId"]);
        const path = String(item["path"]);
        const displayName = path.split(/[\\/]/).filter(Boolean).at(-1) ?? directoryId;
        return { ...item, alias: directoryId, displayName };
      }
      if (resource === "sessions" && isObject(item["workspace"])) {
        const directoryId = item["workspace"]["directoryId"];
        if (typeof directoryId === "string") {
          return { ...item, workspace: { ...item["workspace"], alias: directoryId, displayName: directoryId } };
        }
      }
      return item;
    });
    return { ok: true, message: { ...message, body: { ...inner, items: normalized } } as unknown as SyncSnapshotChunk };
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

/** `$defs.viewVersion`：0..=2^53−1 的整数（schema 的 `maximum`）。 */
const MAX_VIEW_VERSION = 2 ** 53 - 1;

function isViewVersion(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= MAX_VIEW_VERSION;
}

/** 只允许封闭对象的**恰好这些键**（`additionalProperties: false` 的帧在这里硬判）。 */
function hasExactKeys(value: JsonObject, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => key in value);
}

/**
 * `session.view`（§11.8）：body 是 closed object，`sessionId` 为 UUID 或 `null`。
 *
 * 服务端不会向客户端下发这一帧；解码只用于让「角色搞错」在连接层变成明确的拒绝，而不是
 * 一个被静默丢弃的未知类型。
 */
function decodeSessionView(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasExactKeys(inner, ["sessionId", "viewVersion"])) {
      return reject("field_type", "session.view 只允许 sessionId/viewVersion 两个键");
    }
    const sessionId = inner["sessionId"];
    if (sessionId !== null && !isUuid(sessionId)) return reject("field_type", "session.view.sessionId 形状非法");
    if (!isViewVersion(inner["viewVersion"])) {
      return reject("field_type", "session.view.viewVersion 不是 0..=2^53-1 的整数");
    }
    return { ok: true, message: message as SyncSessionView };
  });
}

/**
 * `session.view.result`（§11.8）：`accepted` 必须带未来时刻，`stale` 必须把租约置 `null`。
 *
 * 两条约束在这里硬判：客户端会把 `leaseExpiresAt` 用于诊断与「声明已生效」的呈现，接受一个
 * 缺时刻的 `accepted` 就等于把不合法应答当成成功。
 */
function decodeSessionViewResult(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["sessionId", "viewVersion", "status", "leaseExpiresAt"] as const;
    if (!hasExactKeys(inner, keys)) {
      return reject("field_type", `session.view.result 只允许 ${keys.join(",")}`);
    }
    const sessionId = inner["sessionId"];
    if (sessionId !== null && !isUuid(sessionId)) {
      return reject("field_type", "session.view.result.sessionId 形状非法");
    }
    if (!isViewVersion(inner["viewVersion"])) {
      return reject("field_type", "session.view.result.viewVersion 不是 0..=2^53-1 的整数");
    }
    const status = inner["status"];
    if (status !== "accepted" && status !== "stale") {
      return reject("field_type", "session.view.result.status 不在 accepted/stale 内");
    }
    const lease = inner["leaseExpiresAt"];
    if (status === "accepted") {
      if (!isTimestamp(lease)) return reject("field_type", "accepted 的 leaseExpiresAt 必须是 timestamp");
    } else if (lease !== null) {
      return reject("field_type", "stale 的 leaseExpiresAt 必须为 null");
    }
    return { ok: true, message: message as SyncSessionViewResult };
  });
}

/** `$defs.preparationConfigSelection`：1..=64 项的 `string | boolean` 表（值不做进一步解释）。 */
function isConfigSelection(value: unknown): boolean {
  if (!isObject(value)) return false;
  const keys = Object.keys(value);
  if (keys.length === 0 || keys.length > 64) return false;
  return keys.every((key) => typeof value[key] === "string" || typeof value[key] === "boolean");
}

/**
 * `session.preparation`（§11.9）：准备控制请求。
 *
 * 五个 action 各自闭合：`allocate` 只带 Agent/目录与标签页归属（**不得**带 `sessionId`，正式 ID
 * 由 Owner 分配）；`restore`/`configure`/`renew`/`release` 都带 `sessionId` 与凭据，
 * `configure` 另需资源代与至少一个待应用的值。这一层只判**分支形状**；租约、容量与业务结果
 * 归服务端与 feature 层。
 */
function decodeSessionPreparation(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["operationId", "action"])) {
      return reject("missing_field", "session.preparation 缺 operationId/action");
    }
    if (!isUuid(inner["operationId"])) return reject("field_type", "operationId 不是 UUID");
    const action = inner["action"];
    if (!isNonEmptyString(action) || !Object.hasOwn(PREPARATION_ACTIONS, action)) {
      return reject("field_type", "preparation action 不在封闭词表内");
    }
    if (action === "allocate") {
      const allowed = ["operationId", "action", "agentId", "workspace", "tabNonce"] as const;
      if (!hasExactKeys(inner, allowed)) {
        return reject("field_type", "allocate 只允许 operationId/action/agentId/workspace/tabNonce");
      }
      const agentId = inner["agentId"];
      if (!isNonEmptyString(agentId) || agentId.length > 128) {
        return reject("field_type", "allocate.agentId 长度必须在 1..=128");
      }
      if (!isPreparationWorkspaceRef(inner["workspace"])) {
        return reject("field_type", "allocate.workspace 必须是 directoryId/workspacePath 之一");
      }
      if (!isBase64UrlOfLength(inner["tabNonce"], BASE64URL_32_CHARS)) {
        return reject("field_type", "allocate.tabNonce 必须是 32 字节 base64url");
      }
      return { ok: true, message: message as SyncSessionPreparation };
    }
    if (!hasKeys(inner, ["sessionId", "receipt"])) {
      return reject("missing_field", `${action} 缺 sessionId/receipt`);
    }
    if (!isUuid(inner["sessionId"])) return reject("field_type", "sessionId 不是 UUID");
    if (!isPreparationReceipt(inner["receipt"])) return reject("field_type", "receipt 形状非法");
    if (action === "restore") {
      const allowed = [
        "operationId",
        "action",
        "sessionId",
        "receipt",
        "knownRuntimeEpoch",
        "knownGeneration",
      ] as const;
      if (!hasOnlyKeys(inner, allowed)) return reject("field_type", "restore 出现未登记字段");
      const knownEpoch = inner["knownRuntimeEpoch"];
      if (knownEpoch !== undefined && !isUuid(knownEpoch)) {
        return reject("field_type", "restore.knownRuntimeEpoch 不是 UUID");
      }
      const knownGeneration = inner["knownGeneration"];
      if (knownGeneration !== undefined && !isPositiveDecimalString(knownGeneration)) {
        return reject("field_type", "restore.knownGeneration 不是正十进制字符串");
      }
      return { ok: true, message: message as SyncSessionPreparation };
    }
    if (!hasKeys(inner, ["runtimeEpoch", "generation"])) {
      return reject("missing_field", `${action} 缺 runtimeEpoch/generation`);
    }
    if (!isUuid(inner["runtimeEpoch"])) return reject("field_type", "runtimeEpoch 不是 UUID");
    if (!isPositiveDecimalString(inner["generation"])) {
      return reject("field_type", "generation 不是正十进制字符串");
    }
    if (action === "configure") {
      const allowed = [
        "operationId",
        "action",
        "sessionId",
        "receipt",
        "runtimeEpoch",
        "generation",
        "mode",
        "configOptions",
      ] as const;
      if (!hasOnlyKeys(inner, allowed)) return reject("field_type", "configure 出现未登记字段");
      const hasMode = Object.hasOwn(inner, "mode");
      const hasConfig = Object.hasOwn(inner, "configOptions");
      if (!hasMode && !hasConfig) {
        return reject("missing_field", "configure 至少要给出 mode 或 configOptions");
      }
      if (hasMode) {
        const mode = inner["mode"];
        if (!isNonEmptyString(mode) || mode.length > 256) {
          return reject("field_type", "configure.mode 长度必须在 1..=256");
        }
      }
      if (hasConfig && !isConfigSelection(inner["configOptions"])) {
        return reject("field_type", "configure.configOptions 必须是 1..=64 项的 string/boolean 表");
      }
      return { ok: true, message: message as SyncSessionPreparation };
    }
    const allowed = [
      "operationId",
      "action",
      "sessionId",
      "receipt",
      "runtimeEpoch",
      "generation",
    ] as const;
    if (!hasExactKeys(inner, allowed)) {
      return reject(
        "field_type",
        `${action} 只允许 operationId/action/sessionId/receipt/runtimeEpoch/generation`,
      );
    }
    return { ok: true, message: message as SyncSessionPreparation };
  });
}

/**
 * `session.preparation.result`（§11.9）：闭合的 `status` 判别联合。
 *
 * 每个分支的键集合都**恰定**：`committed` 不带临时代/租约/配置/凭据，`released` 不带任何额外
 * 字段。这一层不把 `committed` 当成功终态，也不推断派发结果。
 */
function decodeSessionPreparationResult(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const base = ["operationId", "action", "sessionId", "status"] as const;
    if (!hasKeys(inner, base)) {
      return reject("missing_field", `session.preparation.result 缺必填字段：${base.join(",")}`);
    }
    if (!isUuid(inner["operationId"])) return reject("field_type", "operationId 不是 UUID");
    const action = inner["action"];
    if (!isNonEmptyString(action) || !Object.hasOwn(PREPARATION_ACTIONS, action)) {
      return reject("field_type", "preparation result 的 action 不在封闭词表内");
    }
    if (!isUuid(inner["sessionId"])) return reject("field_type", "sessionId 不是 UUID");
    const status = inner["status"];
    if (!isNonEmptyString(status) || !Object.hasOwn(PREPARATION_RESULT_STATUSES, status)) {
      return reject("field_type", "preparation status 不在封闭词表内");
    }
    switch (status) {
      case "ready": {
        const keys = [
          ...base,
          "receipt",
          "runtimeEpoch",
          "generation",
          "modes",
          "configOptions",
          "leaseExpiresAt",
        ] as const;
        if (!hasExactKeys(inner, keys)) return reject("field_type", `ready 只允许 ${keys.join(",")}`);
        if (!isPreparationReceipt(inner["receipt"])) return reject("field_type", "ready.receipt 形状非法");
        if (!isUuid(inner["runtimeEpoch"])) return reject("field_type", "ready.runtimeEpoch 不是 UUID");
        if (!isPositiveDecimalString(inner["generation"])) {
          return reject("field_type", "ready.generation 不是正十进制字符串");
        }
        const modes = inner["modes"];
        if (!Array.isArray(modes)) return reject("field_type", "ready.modes 不是数组");
        for (const mode of modes) {
          if (!isObject(mode) || !isNonEmptyString(mode["modeId"]) || typeof mode["displayName"] !== "string") {
            return reject("field_type", "ready.modes 的元素必须是 modeRef");
          }
        }
        const configOptions = inner["configOptions"];
        if (!Array.isArray(configOptions)) return reject("field_type", "ready.configOptions 不是数组");
        for (const option of configOptions) {
          if (!isObject(option) || !isNonEmptyString(option["id"])) {
            return reject("field_type", "ready.configOptions 的元素必须带 id");
          }
        }
        if (!isTimestamp(inner["leaseExpiresAt"])) {
          return reject("field_type", "ready.leaseExpiresAt 不是 timestamp");
        }
        return { ok: true, message: message as SyncSessionPreparationResult };
      }
      case "committed": {
        const keys = [...base, "firstSubmissionId", "firstRequestId", "currentRequestId", "firstMessageId"] as const;
        if (!hasExactKeys(inner, keys)) {
          return reject("field_type", `committed 只允许 ${keys.join(",")}`);
        }
        for (const key of ["firstSubmissionId", "firstRequestId", "currentRequestId", "firstMessageId"] as const) {
          if (!isUuid(inner[key])) return reject("field_type", `committed.${key} 不是 UUID`);
        }
        return { ok: true, message: message as SyncSessionPreparationResult };
      }
      case "stale": {
        const keys = [...base, "reason"] as const;
        if (!hasExactKeys(inner, keys)) return reject("field_type", "stale 只允许 reason");
        const reason = inner["reason"];
        if (!isNonEmptyString(reason) || !Object.hasOwn(PREPARATION_STALE_REASONS, reason)) {
          return reject("field_type", "stale.reason 不在封闭词表内");
        }
        return { ok: true, message: message as SyncSessionPreparationResult };
      }
      case "released": {
        if (!hasExactKeys(inner, base)) return reject("field_type", "released 不带额外字段");
        return { ok: true, message: message as SyncSessionPreparationResult };
      }
      case "failed": {
        const keys = [...base, "error"] as const;
        if (!hasExactKeys(inner, keys)) return reject("field_type", "failed 只允许 error");
        if (!isPublicError(inner["error"])) return reject("field_type", "failed.error 必须是 publicError");
        return { ok: true, message: message as SyncSessionPreparationResult };
      }
      default:
        // 不可达：上面的封闭词表已经拦住了第五个以外的取值。
        return reject("field_type", "preparation status 未登记");
    }
  });
}

/**
 * `sync.remote_*`（§9.7）：只判必填键与基本类型。
 *
 * 来源语义——订阅是否存在、`originEpoch` 是否与该来源一致、`remote_ack` 是否越界、当前是否协商了
 * `resource.stateless-origin.v1`——都属于连接状态机，不在这一层判定。
 */
function decodeRemoteSubscribe(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "sessionRef", "cursor"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_subscribe 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    if (!isSessionRef(inner["sessionRef"])) return reject("field_type", "sessionRef 缺 ownerNodeId/sessionId");
    const cursor = inner["cursor"];
    if (cursor !== null && !isOriginCursor(cursor)) {
      return reject("field_type", "remote_subscribe 的 cursor 既非 null 也非 originCursor");
    }
    return { ok: true, message: message as SyncRemoteSubscribe };
  });
}

function decodeRemoteEvent(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "event"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_event 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    const event = inner["event"];
    if (!isObject(event)) return reject("missing_field", "remote_event 缺 event 对象");
    const eventKeys = [
      "originEventId",
      "originEpoch",
      "originSequence",
      "sessionRef",
      "eventType",
      "payloadDigest",
      "createdAt",
      "payload",
    ] as const;
    if (!hasKeys(event, eventKeys)) {
      return reject("missing_field", `remote_event.event 缺必填字段：${eventKeys.join(",")}`);
    }
    if (!isUuid(event["originEventId"])) return reject("field_type", "originEventId 不是 UUID");
    if (!isUuid(event["originEpoch"])) return reject("field_type", "originEpoch 不是 UUID");
    if (!isDecimalString(event["originSequence"])) {
      return reject("field_type", "originSequence 不是 decimalString");
    }
    if (!isSessionRef(event["sessionRef"])) return reject("field_type", "event.sessionRef 形状非法");
    const eventType = event["eventType"];
    if (!isNonEmptyString(eventType) || !REMOTE_EVENT_TYPE_PATTERN.test(eventType)) {
      return reject("field_type", "event.eventType 不在 `^[a-z0-9_.-]{1,128}$` 形状内");
    }
    if (!isNonEmptyString(event["payloadDigest"])) {
      return reject("field_type", "event.payloadDigest 不是非空字符串");
    }
    if (!isTimestamp(event["createdAt"])) return reject("field_type", "event.createdAt 不是 timestamp");
    const payload = event["payload"];
    if (!isObject(payload)) return reject("field_type", "event.payload 不是对象");
    const hasView = "view" in payload;
    const hasAcp = "acp" in payload;
    if (!hasView && !hasAcp) {
      return reject("missing_field", "event.payload 至少要出现 view 或 acp");
    }
    if (hasView && !isObject(payload["view"])) return reject("field_type", "event.payload.view 不是对象");
    if (hasAcp && !isObject(payload["acp"])) return reject("field_type", "event.payload.acp 不是对象");
    return { ok: true, message: message as SyncRemoteEvent };
  });
}

function decodeRemoteCaughtUp(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "cursor"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_caught_up 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    if (!isOriginCursor(inner["cursor"])) return reject("field_type", "remote_caught_up 的 cursor 形状非法");
    return { ok: true, message: message as SyncRemoteCaughtUp };
  });
}

function decodeRemoteAck(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "cursor"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_ack 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    if (!isOriginCursor(inner["cursor"])) return reject("field_type", "remote_ack 的 cursor 形状非法");
    return { ok: true, message: message as SyncRemoteAck };
  });
}

function decodeRemoteResetRequired(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "reason", "cursor"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_reset_required 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    const reason = inner["reason"];
    if (!isNonEmptyString(reason) || !(reason in REMOTE_RESET_REASONS)) {
      return reject("field_type", "remote_reset_required 的 reason 不在封闭词表内");
    }
    const cursor = inner["cursor"];
    if (cursor !== null && !isOriginCursor(cursor)) {
      return reject("field_type", "remote_reset_required 的 cursor 既非 null 也非 originCursor");
    }
    return { ok: true, message: message as SyncRemoteResetRequired };
  });
}

function decodeRemoteUnsubscribe(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    if (!hasKeys(inner, ["subscriptionId"])) {
      return reject("missing_field", "remote_unsubscribe 缺 subscriptionId");
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    return { ok: true, message: message as SyncRemoteUnsubscribe };
  });
}

function decodeRemoteUnavailable(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["subscriptionId", "error"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_unavailable 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["subscriptionId"])) return reject("field_type", "subscriptionId 不是 UUID");
    if (!isPublicError(inner["error"])) return reject("field_type", "remote_unavailable 缺结构化 publicError");
    return { ok: true, message: message as SyncRemoteUnavailable };
  });
}

function decodeRemoteCatalogChanged(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["ownerNodeId", "revision"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_catalog_changed 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["ownerNodeId"])) return reject("field_type", "ownerNodeId 不是 UUID");
    if (!isDecimalString(inner["revision"])) return reject("field_type", "revision 不是 decimalString");
    return { ok: true, message: message as SyncRemoteCatalogChanged };
  });
}

function decodeRemoteSourceState(parsed: JsonObject, body: unknown): WireDecodeResult {
  return withBody(parsed, body, (message, inner) => {
    const keys = ["ownerNodeId", "state"] as const;
    if (!hasKeys(inner, keys)) {
      return reject("missing_field", `remote_source_state 缺必填字段：${keys.join(",")}`);
    }
    if (!isUuid(inner["ownerNodeId"])) return reject("field_type", "ownerNodeId 不是 UUID");
    const state = inner["state"];
    if (!isNonEmptyString(state) || !(state in REMOTE_SOURCE_STATES)) {
      return reject("field_type", "remote_source_state 的 state 不在封闭词表内");
    }
    return { ok: true, message: message as SyncRemoteSourceState };
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
    // schema 的 `if/then`：`kind: "node"` 时 nodeId 必填且为非空 UUID，deviceId 必须为 null。
    // 这是 `docs/SYNC_PROTOCOL.md` §10.1 的唯一配对规则；其余四种 kind 不新增任何身份字段必填性
    // （未协商 core.event-origin.v1 的连接收到的冻结形状不含 nodeId 键，且发送方不得产生
    // `{kind: "device", deviceId: null}` 但接收方不得拒绝它）。
    if (originKind === "node") {
      if (!("nodeId" in origin)) return reject("missing_field", "event.origin.kind=node 缺 nodeId");
      if (!isUuid(origin["nodeId"])) return reject("field_type", "event.origin.nodeId 不是非空 UUID");
      if (originDeviceId !== null) return reject("field_type", "event.origin.kind=node 时 deviceId 必须为 null");
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
    // 摘要类事件的 `view.session.localActivity` 是可选的封闭词表：未知值不得被当成某个状态。
    const summary = payload["view"]["session"];
    if (isObject(summary) && "localActivity" in summary && !isLocalActivity(summary["localActivity"])) {
      return reject("field_type", "event.view.session.localActivity 不在 idle/active/pending 内");
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
  // body 的必填集合与 `$defs.authenticated` 逐字对应：设备方向没有授权维度，因此**没有**
  // `scopes`（schema 的 `additionalProperties: false` 不容许该键）——本层既不要求它，也不读它。
  const keys = [
    "deviceId",
    "serverEpoch",
    "headGlobalSequence",
    "heartbeatIntervalMs",
    "limits",
  ] as const;
  const missing = preAuthRequired(body, keys);
  if (missing) return missing;
  if (!isUuid(body["deviceId"])) return reject("field_type", "deviceId 非 UUID");
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