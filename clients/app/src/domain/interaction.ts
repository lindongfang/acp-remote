/**
 * 授权与提问应答的**可用边界**（`design.md` D7；spec「授权与提问应答的可用边界」）。
 *
 * ## 为什么「可选项」只能来自亲历的请求
 *
 * `session.read` 读回的 `pending_interactions` 元素只有 `interactionId`/`kind`/`state`/`schema`
 * /`createdAt`——**没有** `options`。核心层刻意不持久化可选项（它只存在于 `permission.requested`
 * 的事件 payload 里，那份 payload 是它的唯一权威来源）。因此：
 *
 * - 只有**本设备连接期间收到过** `permission.requested` 的客户端才持有 `optionId`，才谈得上提交；
 * - 仅从历史读到的待处理项没有可选项，呈现为不可操作并说明原因，MUST NOT 画出无法成功的应答按钮。
 *
 * ## 取值即边界
 *
 * 本模块把这件事收敛成判别联合，四种边界各是一种取值，另加两个「已提交/已处理」态：
 *
 * 1. 亲历且持有可选项 → `answerable`；
 * 2. 仅从历史读到 → `not_answerable` / `read_from_history`；
 * 3. 已被其它设备处理 → `handled_elsewhere`（且不可重复提交）；
 * 4. 会话已关闭 → `not_answerable` / `session_closed`；
 * 5. 本设备已提交、尚未收到终态 → `submitting`（**不得**呈现为已处理）；
 * 6. 本设备提交并已被服务端确认 → `handled_here`。
 *
 * 判定**只看输入**（是否亲历、是否有可选项、是否已处理、会话是否开放），不看具体组件，
 * 因此每种边界都可以脱离 UI 单测。
 */

import type { InteractionOption } from "../protocol/common";
import type { ElicitationResolutionAction } from "../protocol/event";

/** 待处理交互的两类（`snapshotItem.pending_interactions.kind`）。 */
export type InteractionKind = "permission" | "elicitation";

/** 不可应答的原因（MUST 可区分：措辞不同、可核对）。 */
export type InteractionUnavailableReason = "read_from_history" | "session_closed" | "options_missing";

/** 不可应答原因的展示文案。 */
export const INTERACTION_UNAVAILABLE_LABELS: Readonly<Record<InteractionUnavailableReason, string>> = {
  read_from_history: "这条待处理项是从历史读到的，本设备没有收到过它的请求内容，无法应答",
  session_closed: "会话已关闭，不能再应答",
  options_missing: "这次请求没有下发可选项，无法应答",
};

/**
 * 一个可点击的应答选项。
 *
 * 两支是两种**不同的命令载荷**，刻意不合一：授权用 `permission.resolve`（`optionId`），
 * 提问用 `elicitation.respond`（`action`，值来自表单）。判别键是 `via`，组件据此决定点下去
 * 派发哪条命令、要不要收集表单值。
 */
export type InteractionAnswer =
  | { readonly via: "option"; readonly optionId: string; readonly label: string }
  | { readonly via: "action"; readonly action: ElicitationResolutionAction; readonly label: string };

/** 提问的三条动作（`elicitation.respond` 的载荷固定，不来自服务端可选项）。 */
export const ELICITATION_ANSWERS: readonly InteractionAnswer[] = [
  { via: "action", action: "submit", label: "提交" },
  { via: "action", action: "decline", label: "拒绝" },
  { via: "action", action: "cancel", label: "取消" },
];

/** 一次已到达的终态（来自 `permission.resolved` / `elicitation.resolved`）。 */
export interface InteractionResolution {
  /** 服务端给出的结论原样呈现（`resolution` 或 `action`）。 */
  readonly detail: string;
  /** 是否由**本设备**处理（`resolvedByDeviceId` 与本设备一致，或本设备的提交被确认）。 */
  readonly resolvedByThisDevice: boolean;
  readonly resolvedByDeviceId: string | null;
}

/** 应答的可用性（判别联合；组件按 `kind` 分支，不存在「有按钮但点了必然失败」的组合）。 */
export type InteractionAvailability =
  | { readonly kind: "answerable"; readonly answers: readonly InteractionAnswer[] }
  | { readonly kind: "submitting"; readonly requestId: string }
  | { readonly kind: "handled_here"; readonly resolution: string }
  | {
      readonly kind: "handled_elsewhere";
      readonly resolution: string;
      readonly resolvedByDeviceId: string | null;
    }
  | { readonly kind: "not_answerable"; readonly reason: InteractionUnavailableReason };

/**
 * 由输入构造可用性。
 *
 * 优先级是刻意的：**已到达的终态**优先于「本设备提交过」——提交后必然会有终态，先到就先呈现
 * 已处理，避免把已确认的应答呈现回「待确认」。未到达终态前的本设备提交呈现为 `submitting`，
 * 此后才轮到不可应答的三种原因。
 */
export function buildInteractionAvailability(input: {
  readonly kind: InteractionKind;
  /** 亲历请求里下发的可选项；历史读回的待处理项为 `null`。 */
  readonly options: readonly InteractionOption[] | null;
  /** 本设备连接期间是否收到过该请求的内容。 */
  readonly witnessed: boolean;
  readonly sessionOpen: boolean;
  readonly resolution: InteractionResolution | null;
  /** 本设备已提交应答的 `requestId`；未提交为 `null`。 */
  readonly pendingRequestId: string | null;
}): InteractionAvailability {
  const resolution = input.resolution;
  if (resolution !== null) {
    return resolution.resolvedByThisDevice
      ? { kind: "handled_here", resolution: resolution.detail }
      : {
          kind: "handled_elsewhere",
          resolution: resolution.detail,
          resolvedByDeviceId: resolution.resolvedByDeviceId,
        };
  }
  if (input.pendingRequestId !== null) return { kind: "submitting", requestId: input.pendingRequestId };
  if (!input.sessionOpen) return { kind: "not_answerable", reason: "session_closed" };
  if (!input.witnessed) return { kind: "not_answerable", reason: "read_from_history" };
  if (input.kind === "elicitation") return { kind: "answerable", answers: ELICITATION_ANSWERS };
  const options = input.options;
  if (options === null || options.length === 0) {
    return { kind: "not_answerable", reason: "options_missing" };
  }
  return {
    kind: "answerable",
    answers: options.map((option) => ({ via: "option", optionId: option.optionId, label: option.label })),
  };
}

/** 一条待处理交互的呈现模型（时间线卡片与应答区共用同一份判定）。 */
export interface InteractionView {
  readonly interactionId: string;
  /** 归属会话；读回项与事件都带它，但历史条目缺 `sessionId` 时为 `null`（不编造）。 */
  readonly sessionId: string | null;
  readonly kind: InteractionKind;
  /** 亲历请求里的标题；仅历史读到的待处理项没有它，为 `null`。 */
  readonly title: string | null;
  readonly description: string | null;
  /** 提问的表单结构（`elicitation.requested.schema` 或读回项的 `schema`）；两类都可能有。 */
  readonly schema: Readonly<Record<string, unknown>> | null;
  /** 请求时间：亲历请求用事件时间，读回的待处理项用它的 `createdAt`。 */
  readonly requestedAt: string;
  readonly turnId: string | null;
  readonly witnessed: boolean;
  readonly availability: InteractionAvailability;
}
