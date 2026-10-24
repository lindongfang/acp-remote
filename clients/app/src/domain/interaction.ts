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
 * 本模块把这件事收敛成判别联合，每种边界各是一种取值，另加两个「已提交/已处理」态：
 *
 * 1. 亲历且持有可选项 → `answerable`；
 * 2. 仅从历史读到 → `not_answerable` / `read_from_history`；
 * 3. 已被其它设备处理 → `handled_elsewhere`（且不可重复提交）；
 * 4. 已达过期时间 → `not_answerable` / `expired`；
 * 5. 本设备已提交、尚未收到终态 → `submitting`（**不得**呈现为已处理）；
 * 6. 本设备提交并已被服务端确认 → `handled_here`。
 *
 * 判定**只看输入**（是否亲历、是否有可选项、是否已处理、是否过期），不看具体组件；会话主状态
 * 不参与——`idle`/`running`/`pending` 都不是「不能再应答」的理由，有效交互由服务端权威归约。
 * 因此每种边界都可以脱离 UI 单测。
 */

import type { InteractionOption } from "../protocol/common";
import type { ElicitationResolutionAction } from "../protocol/event";

/** 待处理交互的两类（`snapshotItem.pending_interactions.kind`）。 */
export type InteractionKind = "permission" | "elicitation";

/** 不可应答的原因（MUST 可区分：措辞不同、可核对）。 */
export type InteractionUnavailableReason =
  | "turn_ended"
  | "read_from_history"
  | "options_missing"
  | "expired"
  | "not_effective"
  | "request_version_unknown";

/** 不可应答原因的展示文案。 */
export const INTERACTION_UNAVAILABLE_LABELS: Readonly<Record<InteractionUnavailableReason, string>> = {
  turn_ended: "这条请求所属的轮次已结束，不能再应答",
  read_from_history: "这条待处理项只从历史读到，尚未取得服务端确认的当前有效请求，无法应答",
  options_missing: "这次请求没有下发可选项，无法应答",
  expired: "这次请求已过期或上游等待方已不存在，不能再应答",
  not_effective: "服务端当前不再确认这条请求仍然有效（可能已由其它设备处理），不能应答",
  request_version_unknown:
    "连接已协商 core.pending-interactions.v1，但这次请求尚未取得服务端确认的请求版本；重新取得当前有效请求后才能提交",
};

/**
 * 一个可点击的应答选项。
 *
 * 两支是两种**不同的命令载荷**，刻意不合一：授权用 `permission.resolve`（`optionId`），
 * 提问用 `elicitation.respond`（`action`，值来自表单）。判别键是 `via`，组件据此决定点下去
 * 派发哪条命令、要不要收集表单值。
 *
 * 授权那一支另带 `kind`（ACP 的 `PermissionOptionKind`，`schemas/acp/v1/upstream/schema.json`
 * 的 `allow_once` / `allow_always` / `reject_once` / `reject_always`）。它**不参与命令载荷**
 * （`permission.resolve` 只发 `optionId`），只用于呈现：原型 `.perm .opts .btn.p-*` 按严重度
 * 给三档底色，若不把它带出来，组件就只能把三个按钮画成同一档。取值原样透传，组件负责归一与
 * 未知取值的降级。
 *
 * `kind` 是普通可枚举只读字段：它是应答模型的一部分，`{ ...answer }` / `JSON.stringify(answer)`
 * 都会原样带上它。它不参与任何数据判定，越过 `via` 判别键之后的相等比较本就该看见它。
 */
export type InteractionAnswer =
  | { readonly via: "option"; readonly optionId: string; readonly label: string; readonly kind: string }
  | { readonly via: "action"; readonly action: ElicitationResolutionAction; readonly label: string };

/** 提问的三条动作（`elicitation.respond` 的载荷固定，不来自服务端可选项）。 */
export const ELICITATION_ANSWERS: readonly InteractionAnswer[] = [
  { via: "action", action: "submit", label: "提交" },
  { via: "action", action: "decline", label: "拒绝" },
  { via: "action", action: "cancel", label: "取消" },
];

/** 授权选项 → 应答选项。 */
function optionAnswer(option: InteractionOption): InteractionAnswer {
  return { via: "option", optionId: option.optionId, label: option.label, kind: option.kind };
}

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
  /** 当前请求下发的可选项；只有历史卡片时为 `null`。 */
  readonly options: readonly InteractionOption[] | null;
  /** 本设备连接期间是否收到过该请求的实时事件。 */
  readonly witnessed: boolean;
  /** 是否由本次 `session.read` 的服务端确认（`effectiveInteraction`）。 */
  readonly effective?: boolean;
  /** 服务端确认的请求版本；旧 peer / 仅历史卡片为 `null`。 */
  readonly requestVersion?: string | null;
  /**
   * 本连接是否协商了 `core.pending-interactions.v1`（该 feature 协商后应答 MUST 携带
   * `requestVersion`，服务端不得因缺席放宽校验）。
   *
   * 协商后版本未知（实时事件没带、也还没经 `session.read` 确认）时**不得**判为可应答：
   * 那样画出的按钮点下去只会被服务端拒绝。
   */
  readonly requiresRequestVersion?: boolean;
  /** 服务端确认的有效期；过期即不可应答（`null` = 未给出）。 */
  readonly expiresAt?: string | null;
  /**
   * 是否已向服务端核对过当前有效请求（本会话完成过一次 `refresh_interactions`）。
   *
   * 核对后仍未确认的历史卡片改报 `not_effective`（服务端说它已不再有效），而不是继续报
   * 「尚未取得确认」——否则刷新按钮在结果为空时没有任何可见效果。
   */
  readonly pendingChecked?: boolean;
  /** 服务端后续核实已排除此请求，覆盖此前亲历的旧状态。 */
  readonly confirmedIneffective?: boolean;
  readonly turnEnded?: boolean;
  /** 当前时刻（epoch 毫秒），用于判断 `expiresAt`；缺省不判过期。 */
  readonly nowMs?: number;
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
  if (input.turnEnded === true) return { kind: "not_answerable", reason: "turn_ended" };
  if (input.confirmedIneffective === true) return { kind: "not_answerable", reason: "not_effective" };
  if (input.expiresAt !== null && input.expiresAt !== undefined && input.nowMs !== undefined) {
    if (Date.parse(input.expiresAt) <= input.nowMs) return { kind: "not_answerable", reason: "expired" };
  }
  // 亲历的实时请求，或服务端本次重新确认过且取得完整内容的请求，才谈得上应答。
  if (!input.witnessed && input.effective !== true) {
    // 已经向服务端核对过：仍未确认就是「不再有效」的事实，而不是「还没核对」。
    return {
      kind: "not_answerable",
      reason: input.pendingChecked === true ? "not_effective" : "read_from_history",
    };
  }
  // 协商后应答必须原样回传 `requestVersion`（§11.7）。版本未知时按钮点了必然失败，
  // 因此不判可应答，而是引导用户先重新取得当前有效请求（同一张卡片上的刷新入口）。
  if (input.requiresRequestVersion === true && (input.requestVersion ?? null) === null) {
    return { kind: "not_answerable", reason: "request_version_unknown" };
  }
  if (input.kind === "elicitation") return { kind: "answerable", answers: ELICITATION_ANSWERS };
  const options = input.options;
  if (options === null || options.length === 0) {
    return { kind: "not_answerable", reason: "options_missing" };
  }
  return {
    kind: "answerable",
    answers: options.map(optionAnswer),
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
  readonly sequence?: string;
  readonly turnId: string | null;
  /** 本设备连接期间收到过该请求的实时事件。 */
  readonly witnessed: boolean;
  /** 服务端本次读取确认过（`effectiveInteraction`）。 */
  readonly effective: boolean;
  /** 请求版本：实时事件自带或被 `session.read` 确认；应答时必须原样回传（`null` = 两处都没有）。 */
  readonly requestVersion: string | null;
  /** 服务端给出的有效期（`null` = 未给出）。 */
  readonly expiresAt: string | null;
  /** 当前请求下发的可选项（授权）；提问为 `null`。 */
  readonly options: readonly InteractionOption[] | null;
  readonly availability: InteractionAvailability;
}
