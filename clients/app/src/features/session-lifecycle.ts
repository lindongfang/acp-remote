/**
 * 本机会话生命周期（`core.local-session-lifecycle.v1`）的 **feature 层模型**。
 *
 * 合同见 `docs/SYNC_PROTOCOL.md` §9.8/§11.5/§11.8 与 `docs/FRONTEND_DESIGN.md` §5：
 *
 * - 页面经平台 adapter 报告可见性与 `pagehide`；feature 层在**同一连接**上先声明当前查看
 *   （`session.view`，版本单调递增）再派发 `session.resume`；
 * - 可见时每 10 秒续租（服务端租期 30 秒），隐藏/离页/锁屏立即声明 `null`；
 * - 连接代（每次认证成功递增）与查看版本共同抑制旧续租与旧响应；
 * - 恢复中历史可读、发送禁用、草稿保留；失败保留草稿并给出可区分的明确原因。
 *
 * 本文件只放**纯判定**：没有计时器、没有网络出口、没有平台 API。计时与派发由 `ClientStore`
 * 持有，平台事件由组合根桥接（`src/platform/` 不被本层 import）。
 */

import type { PublicError, SessionSummary, Uuid } from "../protocol";
import { LOCAL_SESSION_LIFECYCLE_FEATURE } from "../sync-client";

/** feature ID（与 `compatibility/features/v1/features.json` 同值）。 */
export const SESSION_LIFECYCLE_FEATURE = LOCAL_SESSION_LIFECYCLE_FEATURE;

/** 可见页面续租间隔（§11.8 的固定 v1 常量）。 */
export const SESSION_VIEW_RENEW_INTERVAL_MS = 10_000;

/** `viewVersion` 的上界（`$defs.viewVersion`：0..=2^53−1）。 */
export const MAX_VIEW_VERSION = 2 ** 53 - 1;

/**
 * 平台 adapter 转来的页面生命周期事实。
 *
 * 定义在 feature 层而不是 import `src/platform/`：分层规则禁止 features 依赖 platform，
 * 组合根负责把两者桥接起来（`docs/FRONTEND_DESIGN.md` §3）。
 */
export type SessionPageEvent =
  | { readonly kind: "visibility"; readonly state: "visible" | "hidden" }
  | { readonly kind: "pagehide" };

/** 一次有效查看声明的不可变记录。 */
export interface SessionViewDeclaration {
  readonly sessionId: Uuid | null;
  readonly viewVersion: number;
  /** 发出该声明时的认证连接代；旧代的应答一律不生效。 */
  readonly connectionGeneration: number;
  /** 发起声明的选择（`selected` 会话）；离开声明为 `null`。 */
  readonly leaseExpiresAt: string | null;
}

/** 激活失败的**可区分**类别（`docs/SYNC_PROTOCOL.md` §11.5）。 */
export type ActivationFailureKind =
  /** 连接未协商该 feature（旧服务端）：明确显示「自动激活不可用」。 */
  | "feature_unavailable"
  /** Agent 未宣告 `sessionCapabilities.resume`，或本机没有恢复数据。 */
  | "unsupported_by_agent"
  /** 占用名额的会话都受保护、没有可回收候选（`session.capacity_exceeded`）。 */
  | "capacity"
  /** 会话不存在或不属于本机。 */
  | "not_found"
  /** 结果无法证明（`command.uncertain`）：不自动换 `requestId` 重派。 */
  | "uncertain"
  /** 其它明确失败（格式、内部不可用等）。 */
  | "failed";

/** 一次失败的完整事实：类别 + 服务端错误码 + 可读原因。 */
export interface ActivationFailure {
  readonly kind: ActivationFailureKind;
  readonly code: string | null;
  readonly message: string;
}

/**
 * 一次激活尝试的阶段。
 *
 * `failed` 表示**尚未**可用；`ready` 才是「资源已确认可用、允许发送」。`idle` 是尚未尝试
 * （例如未可见），不得被呈现成可用。
 */
export type ActivationPhase = "idle" | "activating" | "ready" | "failed";

/** 一个会话最近一次激活尝试。 */
export interface SessionActivationState {
  readonly sessionId: Uuid;
  readonly phase: ActivationPhase;
  readonly requestId: Uuid | null;
  /**
   * 发出这次尝试时的认证连接代（`SessionLifecycleState.generation`）。
   *
   * 结果按连接核对，所以旧代的在途尝试**不可能**再收到终态：仓库在连接断开/换代时必须终结
   * 这些条目，否则页面会永远停在「正在恢复」。保存它是为了让「是否仍在同一代里等结果」成为
   * 可判定的条件，而不是靠时间假设。
   */
  readonly connectionGeneration: number;
  readonly failure: ActivationFailure | null;
}

/** 呈现给页面的激活状态（与「会话是否存在」「是否可见」无关，只是本次尝试）。 */
export type SessionActivationView =
  /** 连接未协商该 feature（旧服务端）：自动激活明确不可用，也不发送任何查看/恢复帧。 */
  | { readonly kind: "unavailable"; readonly notice: string }
  | { readonly kind: "inactive"; readonly notice: string }
  | { readonly kind: "activating"; readonly notice: string }
  | {
      readonly kind: "failed";
      readonly reason: string;
      readonly code: string | null;
      /** `false` 表示重试不会有意义（未协商 / Agent 不支持），但用户仍可显式重试。 */
      readonly retryable: boolean;
    }
  | { readonly kind: "ready" };

/** 尚未尝试（例如页面在后台打开）：说明会在可见时自动激活。 */
export const ACTIVATION_INACTIVE_NOTICE = "会话尚未激活；页面可见时会自动恢复原会话。";

/** 恢复中：历史可读，发送不可用。 */
export const ACTIVATION_PROGRESS_NOTICE = "正在恢复会话：历史可照常阅读，恢复完成前不会发送消息。";

/** 未协商该 feature 时的明确降级（旧服务端）。 */
export const ACTIVATION_UNAVAILABLE_NOTICE =
  "此主机未协商本机会话生命周期（core.local-session-lifecycle.v1），自动激活不可用。";

/** 递增查看版本；到上界后保持上界（不再接受新声明，而不是回绕成旧版本）。 */
export function nextViewVersion(current: number): number {
  if (!Number.isInteger(current) || current < 0) return 1;
  return Math.min(current + 1, MAX_VIEW_VERSION);
}

/** 该版本是否还能发出（到上界后无法继续单调递增）。 */
export function canAdvanceViewVersion(version: number): boolean {
  return version < MAX_VIEW_VERSION;
}

/** 会话是否属于本机（只有 owned 路径参与生命周期；imported 保持原任务三态）。 */
export function isOwnedSummary(summary: SessionSummary | null | undefined): boolean {
  return summary !== undefined && summary !== null && summary.origin.kind === "local";
}

/**
 * `command.result` / `command.status` 的失败分类。
 *
 * 顺序有讲究：`uncertain` 是「不知道有没有生效」，它不能被更泛的 `failed` 吞掉；feature
 * 缺失也不是参数错误，必须在界面里说成「自动激活不可用」。错误码按 §11.5 的封闭映射判定，
 * 不解析 message 文本。
 */
export function classifyActivationFailure(input: {
  readonly status: "accepted" | "completed" | "failed" | "rejected" | "uncertain";
  readonly error: PublicError | null;
}): ActivationFailure | null {
  if (input.status === "accepted" || input.status === "completed") return null;
  const code = input.error?.code ?? null;
  if (input.status === "uncertain" || code === "command.uncertain") {
    return {
      kind: "uncertain",
      code,
      message: "激活结果不确定：会话可能已经恢复。系统不会自动重发；请先确认状态再重试。",
    };
  }
  switch (code) {
    case "command.unsupported":
    case "protocol.feature_required":
      return {
        kind: "feature_unavailable",
        code,
        message: ACTIVATION_UNAVAILABLE_NOTICE,
      };
    case "capability.unsupported_by_agent":
      return {
        kind: "unsupported_by_agent",
        code,
        message: "当前 Agent 不支持恢复原会话（或本机没有可用的恢复数据），历史仍可阅读。",
      };
    case "session.capacity_exceeded":
      return {
        kind: "capacity",
        code,
        message: "本机可运行的会话名额已满（其余会话都在使用中）。历史仍可阅读，稍后可重试。",
      };
    case "session.not_found":
      return {
        kind: "not_found",
        code,
        message: "会话不存在或不属于本机，无法恢复；历史仍可阅读。",
      };
    default:
      return {
        kind: "failed",
        code,
        message: input.error?.message ?? "激活失败，历史仍可阅读；可稍后重试。",
      };
  }
}

/**
 * 失败是否给出**显式**重试入口。
 *
 * `uncertain` 同样可重试，但它的重试路径是 `command.status` 核实原请求（不换 `requestId`
 * 重派）；`feature_unavailable` / `unsupported_by_agent` / `not_found` 重试不会有意义。
 */
export function isRetryableFailure(failure: ActivationFailure): boolean {
  return failure.kind === "capacity" || failure.kind === "failed" || failure.kind === "uncertain";
}

/** 把激活尝试转成页面可渲染的状态。 */
export function activationViewOf(input: {
  readonly phase: ActivationPhase;
  readonly failure: ActivationFailure | null;
}): SessionActivationView {
  switch (input.phase) {
    case "ready":
      return { kind: "ready" };
    case "activating":
      return { kind: "activating", notice: ACTIVATION_PROGRESS_NOTICE };
    case "failed": {
      const failure = input.failure ?? {
        kind: "failed" as const,
        code: null,
        message: "激活失败，历史仍可阅读；可稍后重试。",
      };
      return {
        kind: "failed",
        reason: failure.message,
        code: failure.code,
        retryable: isRetryableFailure(failure),
      };
    }
    case "idle":
      return { kind: "inactive", notice: ACTIVATION_INACTIVE_NOTICE };
  }
}

/**
 * 当前是否允许向该会话派发 prompt。
 *
 * - imported 会话与未协商 feature 的连接：沿用既有判定（本函数不参与）；
 * - 协商后的本机会话：只有 `ready` 才允许。`active` 标签**不是**发送依据——资源没恢复时
 *   `localActivity` 也可能因为任务/交互而显示 `active`。
 */
export function isActivationReady(phase: ActivationPhase | null): boolean {
  return phase === "ready";
}
