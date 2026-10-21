/**
 * 连接问题的视图模型（`design.md` D1/D5；spec「连接状态是互斥状态机」的阻断态说明）。
 *
 * ## 为什么单独一份
 *
 * 顶栏只表达「在线 / 离线」两档，阻断原因与下一步动作**不再**挤进胶囊的短词。那部分信息由
 * 一个不可关闭的连接问题弹层承担，并且只在这里判定一次：`AuthTransition`（首屏认证失败）与
 * `ShellView`（已认证后的连接问题）渲染的是同一份模型与同一个组件。
 *
 * ## 诊断详情的边界
 *
 * `technicalDetail` 只写机读诊断：错误码、关闭码、归一后的关闭原因与发生时刻。关闭原因来自
 * 对端，可能夹带密钥/令牌/配对口令，因此只接受**机读词表形态**且不含敏感词的原因；其余一律
 * 写成 `[redacted]`。配对载荷本身从不进入 `ClientState`，这里也不读它。
 */

import type { ConnectionStateName } from "../state";
import type { ConnectionCloseRecord, ReconnectProgress } from "./client-store";

/** 网络可用性：只有在线/离线两档；尚未认证或正在握手时为「未知」。 */
export type ConnectionAvailability = "online" | "offline";

/**
 * 连接状态的网络可用性。
 *
 * 阻断态也区分在线与离线：`incompatible`/`identity_changed`/`replaced` 时 socket 曾经建立
 * （问题出在身份或版本），`revoked` 则意味着这台设备不再被主机接受——按离线呈现，危险档。
 */
const AVAILABILITY_BY_STATE: Readonly<Record<ConnectionStateName, ConnectionAvailability | null>> = {
  replaying: "online",
  online: "online",
  incompatible: "online",
  identity_changed: "online",
  replaced: "online",
  disconnected: "offline",
  reconnecting: "offline",
  revoked: "offline",
  unpaired: null,
  pairing: null,
  connecting: null,
  authenticating: null,
};

/**
 * 连接状态 → 网络可用性；`null` = 尚未进入可判定的已认证连接。
 *
 * `hasAuthenticated` 区分「首次认证」与「已认证后的重新认证」：后者发生在断线重连中，
 * 顶栏与弹层必须继续显示离线，而不是退回「状态未知」。首次认证仍由认证过渡屏承接。
 */
export function networkAvailabilityOf(
  state: ConnectionStateName,
  hasAuthenticated = false,
): ConnectionAvailability | null {
  if (hasAuthenticated && (state === "connecting" || state === "authenticating")) {
    return "offline";
  }
  return AVAILABILITY_BY_STATE[state];
}

/** 连接问题的五种变体（阻断态四种 + 普通离线）。 */
export type ConnectionProblemKind = "offline" | "revoked" | "incompatible" | "identity_changed" | "replaced";

/** 唯一的下一步动作：普通离线自动重连，不提供按钮。 */
export type ConnectionProblemAction = "none" | "repair" | "reload";

/** 连接问题弹层的呈现模型（五种变体共用这一份形状）。 */
export interface ConnectionProblemModel {
  readonly kind: ConnectionProblemKind;
  readonly title: string;
  /** 面向用户的一句话说明（含可行的下一步）。 */
  readonly description: string;
  readonly errorCode: string | null;
  readonly closeCode: number | null;
  /** 单条机读诊断：错误码 + 关闭码 + 关闭原因 + 时刻。**不含**任何密钥/令牌/配对秘密。 */
  readonly technicalDetail: string;
  readonly action: ConnectionProblemAction;
  /** 普通离线的自动重连进度；阻断态没有可用的自动重连，为 `null`。 */
  readonly reconnect: ReconnectProgress | null;
  readonly occurredAtMs: number | null;
}

/** 构造连接问题模型的输入（全部是状态事实，不含任何 UI 局部状态）。 */
export interface ConnectionProblemInput {
  readonly connection: ConnectionStateName;
  readonly hasAuthenticated: boolean;
  readonly lastClose: ConnectionCloseRecord | null;
  readonly reconnect: ReconnectProgress;
  readonly nowMs: number;
}

/** 阻断态的错误码（与协议 §12.3 的关闭码族一一对应）。 */
const BLOCKING_ERROR_CODES = {
  revoked: "auth.device_revoked",
  incompatible: "protocol.version_unsupported",
  identity_changed: "auth.host_mismatch",
  replaced: "connection.replaced",
} as const;

/** 普通离线与阻断态之外的模型字段：标题、说明与动作。 */
interface ProblemCopy {
  readonly title: string;
  readonly description: string;
  readonly action: ConnectionProblemAction;
}

const PROBLEM_COPY: Readonly<Record<ConnectionProblemKind, ProblemCopy>> = {
  offline: {
    title: "连接已断开",
    description: "与电脑的连接已断开；客户端会自动重试，恢复后回到当前页面。重试期间无法提交或读取新内容。",
    action: "none",
  },
  revoked: {
    title: "设备已被撤销",
    description: "这台设备已被主机撤销；请在电脑端重新配对后再继续。",
    action: "repair",
  },
  incompatible: {
    title: "版本或能力不兼容",
    description: "主机与客户端的协议版本或必需能力不兼容；请升级客户端或主机后重新加载。",
    action: "reload",
  },
  identity_changed: {
    title: "主机身份已变化",
    description: "主机身份与配对时记录的不一致；为安全起见不会自动信任，请回到主机侧确认后重新配对。",
    action: "repair",
  },
  replaced: {
    title: "已被其他标签页顶替",
    description: "同一设备的另一个标签页已接管连接；请关闭其他标签页后重新加载。",
    action: "reload",
  },
};

/** 重连中的标题单独给出：同一变体、不同阶段，标题要描述当前正在发生的事。 */
const RECONNECTING_TITLE = "正在重连";

/** 关闭原因只接受机读词表形态，且不得出现任何密钥/令牌/配对相关字样。 */
const SAFE_REASON = /^[a-z][a-z0-9_.-]{0,63}$/;
const SENSITIVE_REASON = /secret|token|key|password|credential|pairing/i;

function safeCloseReason(reason: string | null): string | null {
  if (reason === null) return null;
  if (!SAFE_REASON.test(reason) || SENSITIVE_REASON.test(reason)) return "[redacted]";
  return reason;
}

/** 单条诊断：机读、可复制、不夹带秘密。 */
function technicalDetailOf(input: {
  readonly errorCode: string | null;
  readonly closeCode: number | null;
  readonly reason: string | null;
  readonly occurredAtMs: number | null;
}): string {
  const at = input.occurredAtMs === null ? "unknown" : new Date(input.occurredAtMs).toISOString();
  return [
    `error=${input.errorCode ?? "none"}`,
    `close=${input.closeCode === null ? "none" : String(input.closeCode)}`,
    `reason=${safeCloseReason(input.reason) ?? "none"}`,
    `at=${at}`,
  ].join("; ");
}

/**
 * 构造连接问题模型。
 *
 * 返回 `null` 表示「没有需要打断操作的问题」：在线/追平中，以及尚未认证的握手阶段
 * （那时由 `AuthTransition` 的进度屏表达等待）。
 */
export function buildConnectionProblem(input: ConnectionProblemInput): ConnectionProblemModel | null {
  let kind: ConnectionProblemKind;
  switch (input.connection) {
    case "online":
    case "replaying":
    case "unpaired":
    case "pairing":
      return null;
    case "connecting":
    case "authenticating":
      // 首次认证由 AuthTransition 的进度屏承接；已认证后的重新认证属于离线重连的一部分。
      if (!input.hasAuthenticated) return null;
      kind = "offline";
      break;
    case "disconnected":
    case "reconnecting":
      kind = "offline";
      break;
    case "revoked":
    case "incompatible":
    case "identity_changed":
    case "replaced":
      kind = input.connection;
      break;
  }

  const copy = PROBLEM_COPY[kind];
  const errorCode =
    kind === "offline"
      ? null
      : kind === "incompatible" && input.lastClose?.reason === "required_features_missing"
        ? "protocol.feature_required"
        : BLOCKING_ERROR_CODES[kind];
  const occurredAtMs = input.lastClose?.atMs ?? input.nowMs;
  return {
    kind,
    title: kind === "offline" && input.connection === "reconnecting" ? RECONNECTING_TITLE : copy.title,
    description: copy.description,
    errorCode,
    closeCode: input.lastClose?.code ?? null,
    technicalDetail: technicalDetailOf({
      errorCode,
      closeCode: input.lastClose?.code ?? null,
      reason: input.lastClose?.reason ?? null,
      occurredAtMs,
    }),
    action: copy.action,
    reconnect: kind === "offline" ? input.reconnect : null,
    occurredAtMs,
  };
}

/** 不呈现连接问题弹层的路由：配对入口自己说明状态，主机页就是连接面板本身。 */
const PROBLEM_EXCLUDED_PATHS: Readonly<Record<string, true>> = {
  "/pair": true,
  "/node-link/pair": true,
  "/hosts": true,
};

/** 去掉查询串、片段与尾斜杠之后的路径（与外壳的判据同一套归一化规则）。 */
function normalizedPathOf(pathname: string | null): string | null {
  if (pathname === null) return null;
  const pathOnly = pathname.split("?")[0]?.split("#")[0] ?? "";
  return pathOnly.length > 1 ? pathOnly.replace(/\/+$/, "") : pathOnly;
}

/**
 * 当前路径是否允许呈现连接问题弹层。
 *
 * 只回答路由问题；调用方还必须要求 `hasAuthenticated`（首屏认证失败由 `AuthTransition`
 * 承担，不在业务页上重复弹层）。
 */
export function shouldShowConnectionProblem(pathname: string | null): boolean {
  const normalized = normalizedPathOf(pathname);
  return normalized === null || PROBLEM_EXCLUDED_PATHS[normalized] !== true;
}
