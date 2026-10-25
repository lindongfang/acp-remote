/**
 * Web/PWA 会话准备的**页面状态机与视图模型**（`web-session-preparation` WP7，spec
 * `pwa-web-client`／`web-session-preparation`）。
 *
 * ## 三个事实分别建模
 *
 * - 准备（本模块）：后端已分配 `sessionId`、有临时 Agent 资源与真实设置，但**没有**正式会话行；
 * - 正式会话：首条 `session.start` 的受理事务之后才存在（`phase: "accepted"` 起）；
 * - 浏览器草稿：由平台端口 `PreparationDraftPort` 保存（本模块只描述它的内容形状）。
 *
 * `phase` 是唯一的状态判据，页面不另存一份「是不是还在创建」。`committed` 不是成功终态：
 * 首条可能仍 `accepted`/`failed`/`uncertain`，派发结果必须再看 `currentRequestId`。
 *
 * ## 为什么请求构造在这里
 *
 * `allocate` 只带 Agent、目录与标签页归属——**不得**带 `sessionId`（正式 ID 由 Owner 分配）；
 * `restore`/`configure`/`renew`/`release` 带凭据与资源代；`session.start` 的 `initial` 分支带
 * 凭据、完整内容、模式/配置与清单标识。把这些形状写在一处，页面与仓库都没有第二种拼法。
 */

import type {
  ConfigOptionView,
  ModeRef,
  PreparationConfigSelection,
  PreparationReceipt,
  PromptContentBlock,
  PublicError,
  SessionStartCommand,
  SessionStartProjection,
  Uuid,
} from "../protocol";
import { PREPARATION_FEATURE } from "../protocol";
import type { DraftAttachment } from "./attachment-model";

/** 准备能力的 feature 标识（与 `src/protocol/attachment.ts` 同一份常量）。 */
export const SESSION_PREPARATION_FEATURE = PREPARATION_FEATURE;

/** 准备页的阶段（闭合）。 */
export type PreparationPhase =
  /** `allocate` 已发出，等待 Owner 分配 ID。 */
  | "allocating"
  /** 已有草稿，正在用 `restore` 重建同 ID 的临时资源。 */
  | "restoring"
  /** 临时资源就绪：可调设置、可发送。 */
  | "ready"
  /** 附件上传 / 首条保存进行中（一个在途尝试）。 */
  | "submitting"
  /** 首条已持久受理；派发结果看所属请求。 */
  | "accepted"
  /** 所属节点确认结果不确定：显眼保留，不自动重试。 */
  | "uncertain"
  /** 可证明未派发的失败：保留内容，等待用户显式继续或重试。 */
  | "failed"
  /** `restore` 发现同 ID 已被正式受理。 */
  | "committed"
  /** 临时资源已释放（主动离开/释放成功）。 */
  | "released"
  /** 当前连接没有该能力，或凭据所属存储已整体重建：明确不可用，不退回旧创建路径。 */
  | "unavailable";

/** 目录引用的来源（与凭据 claims 同义）。 */
export type PreparationWorkspaceSource = "directoryId" | "workspacePath";

/** 准备页展示的目录引用：标识/路径**逐字**来自服务端，客户端不构造。 */
export interface PreparationWorkspace {
  readonly directoryId: string | null;
  readonly path: string;
  readonly source: PreparationWorkspaceSource;
}

/** 一次首条提交的请求关联（`committed` / 已受理后保留的窄记录）。 */
export interface PreparationAssociation {
  readonly requestId: Uuid;
  readonly firstSubmissionId: Uuid | null;
  readonly firstRequestId: Uuid | null;
  readonly currentRequestId: Uuid | null;
  readonly firstMessageId: Uuid | null;
}

/** 一个准备会话的仓库态（不可变快照）。 */
export interface PreparationSessionState {
  readonly sessionId: Uuid;
  readonly agentId: string;
  readonly workspace: PreparationWorkspace;
  /** 标签页归属（凭据 claims 的一部分；不进入 URL）。 */
  readonly tabNonce: string;
  readonly receipt: PreparationReceipt | null;
  readonly runtimeEpoch: Uuid | null;
  readonly generation: string | null;
  readonly leaseExpiresAt: string | null;
  /** Agent **真实**回报的模式与配置项（未提供的保持空数组，不伪造）。 */
  readonly modes: readonly ModeRef[];
  readonly configOptions: readonly ConfigOptionView[];
  /** 用户显式选择的模式；`null` = 采用 Agent 的当前值/默认。 */
  readonly selectedMode: string | null;
  /** 用户显式选择的配置值（只含用户改过的 id）。 */
  readonly selectedConfig: PreparationConfigSelection;
  readonly phase: PreparationPhase;
  readonly association: PreparationAssociation | null;
  readonly error: PublicError | null;
  /**
   * 失败来源：本地上传/保存失败（从未派发，同一 `requestId` 可重试）或服务端报告的
   * `session.start` 失败（显式继续必须走 `kind:"continue"` + 新 `requestId`）。
   */
  readonly failureOrigin: "local_upload" | "server_start" | null;
  /**
   * 服务端投影确认「首条可证明未派发且已有 failed 终态」时才允许 `kind:"continue"`。
   *
   * `firstSubmissionId` 只证明归属；`canContinue` 才是准入（`command.status.preparation`）。
   */
  readonly canContinue: boolean;
  /** 就地说明（不可用原因、恢复提示）；不是服务端错误。 */
  readonly notice: string | null;
  /** 需要用户回目录重新分配（`preparation.store_rebuilt`）。 */
  readonly restartRequired: boolean;
}

/** 一个准备设置项的呈现：真实取值 + 是否被用户改过 + 不可用原因。 */
export interface PreparationConfigOptionView {
  readonly option: ConfigOptionView;
  readonly value: string | boolean;
  /** 用户改过（与 Agent 回报的 `currentValue` 不同）。 */
  readonly changed: boolean;
}

/** 准备设置的视图（模式与配置各一份）。 */
export interface PreparationSettingsView {
  readonly modes: readonly ModeRef[];
  readonly selectedMode: string | null;
  readonly configOptions: readonly PreparationConfigOptionView[];
  /**
   * 重新准备后不再可用的选择（模式或配置 id）。非空时**阻止发送**并说明，
   * MUST NOT 静默回落到默认值（spec「重建后的选项改变」）。
   */
  readonly unavailable: readonly string[];
}

/** 准备页的输入可用性。 */
export interface PreparationInputAvailability {
  readonly canEditText: boolean;
  readonly canSend: boolean;
  readonly reason: string | null;
}

/** 准备页模型（页面只消费它，不做业务判定）。 */
export interface PreparationPageModel {
  readonly sessionId: Uuid;
  readonly agentId: string;
  readonly workspace: PreparationWorkspace;
  readonly phase: PreparationPhase;
  /** 阶段的中文说明（与 `phase` 一一对应，不随连接状态另写一套）。 */
  readonly phaseLabel: string;
  readonly settings: PreparationSettingsView;
  readonly text: string;
  readonly attachments: readonly DraftAttachment[];
  readonly input: PreparationInputAvailability;
  readonly error: PublicError | null;
  readonly notice: string | null;
  readonly association: PreparationAssociation | null;
  readonly restartRequired: boolean;
  /** 已发出但尚未确认的 `session.start` 请求（响应丢失时先核实）。 */
  readonly pendingRequestId: Uuid | null;
  /** `accepted` 之后命令的呈现阶段（`terminal` 由结果查询推进）。 */
  readonly commandState: "submitting" | "accepted" | "uncertain" | "failed" | "completed" | null;
  /** 临时资源租约已过期（续租未及时生效）：必须在页面上显式呈现，不能装作可以发送。 */
  readonly leaseExpired: boolean;
}

/** 阶段文案：一份映射，页面不另写 if/else。 */
export const PREPARATION_PHASE_LABELS: Readonly<Record<PreparationPhase, string>> = {
  allocating: "正在分配会话",
  restoring: "正在恢复准备",
  ready: "准备中（首条发送后才保存正式会话）",
  submitting: "正在保存并派发首条消息",
  accepted: "首条消息已保存",
  uncertain: "结果不确定：请勿重复发送",
  failed: "首条消息未能派发",
  committed: "首条消息已保存",
  released: "准备已释放",
  unavailable: "准备能力不可用",
};

/**
 * 设置项不可用时阻止发送的统一说明。
 *
 * 重新准备后 Agent 不再提供用户原来的选择：保留输入与附件，要求用户处理设置，
 * 不自动发送、不静默回退（spec「重建后的选项改变」）。
 */
export const PREPARATION_SETTINGS_UNAVAILABLE_NOTICE =
  "所选模式或配置在新会话中已不可用；请重新选择后再发送。";

/** 未协商准备能力时创建入口的明确说明（**不**退回旧 `session.create`）。 */
export const PREPARATION_UNSUPPORTED_NOTICE =
  "当前服务端未协商会话准备能力（core.session-preparation.v1），无法新建会话。请更新服务端后重试。";

/** 组合根没有注入标签页归属端口：不能凭空拼出一个 nonce，入口必须明确不可用。 */
export const PREPARATION_NO_TAB_NONCE_NOTICE =
  "当前装配没有标签页准备端口，无法发起准备（不能凭空生成凭据归属）。";

/** 可见时的准备续租间隔（design D3：固定租期 30 秒、可见时 10 秒续租）。 */
export const PREPARATION_RENEW_INTERVAL_MS = 10_000;

/** 租约已过期时的发送拦截说明（续租仍在跑；持续失败要离开后重开）。 */
export const PREPARATION_LEASE_EXPIRED_NOTICE =
  "临时资源租约已过期：正在续租，请稍候；若一直失败，请离开准备页后重新打开。";

/** `beginPreparation` 的结果：成功时已经拿到 Owner 分配的最终 `sessionId`。 */
export type PreparationBeginOutcome =
  | { readonly ok: true; readonly sessionId: Uuid }
  | { readonly ok: false; readonly reason: string };

/**
 * 普通新建入口的统一判定：当前标签页只有一个准备槽位。
 *
 * - `session`：已有未转正临时会话，入口只导航到它（不弹创建、不改目录/Agent）；
 * - `allocating`：槽位分配在途，入口等同一个结果，不再发第二帧；
 * - `none`：没有槽位，可以打开创建弹层（这是唯一会发 `allocate` 的分支）。
 */
export type PreparationSlotRoute =
  | { readonly kind: "none" }
  | { readonly kind: "session"; readonly sessionId: Uuid }
  | { readonly kind: "allocating"; readonly outcome: Promise<PreparationBeginOutcome> };

/** 同一标签页已被另一份准备占用（复制/opener 标签页）时的说明。 */
export const PREPARATION_DUPLICATE_TAB_NOTICE =
  "这个地址属于另一个标签页的准备；当前标签页不会接管它的草稿，确认不存在后返回目录页。";

/** 凭据所属存储已整体重建：旧材料不可用，认证后回目录重新开始。 */
export const PREPARATION_STORE_REBUILT_NOTICE =
  "服务端存储已重建，旧的准备材料失效；请回目录重新开始（已输入的内容无法恢复）。";

/** 首条结果不确定的固定说明（不自动消解）。 */
export const PREPARATION_UNCERTAIN_NOTICE =
  "所属节点无法确认首条消息是否已派发。页面不会自动重发；请先核实原请求再决定下一步。";

/** 浏览器不能保存草稿（端口未装配或不可用）：必须明确说明刷新无法恢复。 */
export const PREPARATION_STORAGE_FAILED_NOTICE =
  "当前浏览器无法保存准备草稿：页面内输入仍然有效，但刷新后无法恢复。";

/** 草稿保存成功但定位不可持久（例如 sessionStorage 不可用）：刷新不会找回它。 */
export const PREPARATION_NOT_RESTORABLE_NOTICE =
  "当前浏览器不能持久保存标签页定位：本次输入可用，但刷新后无法恢复。";

/**
 * 一个服务端错误对应的客户端**固定动作**（`design.md` D6.3/D8.1）。
 *
 * - `restart`：凭据所属存储已重建 → 丢弃 receipt、回目录重新 allocate；
 * - `restore`：资源代过期/已释放 → 先核实原 start，再用 receipt restore；
 * - `already_committed`：已正式受理 → restore 取关联，同页读正式会话，不再次 start；
 * - `verify`：结果不明 → 用原 `requestId` 查 `command.status`，不重发；
 * - `retry`：可重试的暂时失败 → 保留草稿，用户显式重试；
 * - `stop`：不可安全继续（认证/绑定/形状）→ 停止这次传输并保留输入。
 */
export type PreparationErrorAction = "restart" | "restore" | "already_committed" | "verify" | "retry" | "stop";

/** 错误码 → 固定动作；未登记的码按「有 retryable 重试、否则停止」处理。 */
export function preparationErrorAction(error: PublicError): PreparationErrorAction {
  switch (error.code) {
    case "preparation.store_rebuilt":
      return "restart";
    case "preparation.stale":
      return "restore";
    case "preparation.already_committed":
      return "already_committed";
    case "command.uncertain":
      return "verify";
    default:
      return error.retryable ? "retry" : "stop";
  }
}

/** 从 Agent 回报的真实配置项推出「未修改时」的取值（不伪造缺失项）。 */
export function defaultConfigSelection(
  options: readonly ConfigOptionView[],
): PreparationConfigSelection {
  const selection: Record<string, string | boolean> = {};
  for (const option of options) selection[option.id] = option.currentValue;
  return selection;
}

/**
 * 在当前 Agent 选项上应用用户的显式选择。
 *
 * 返回 `unavailable`：用户此前的模式/配置在新选项里已不存在。这一支**不**删除选择，
 * 由视图层提示并要求重选；静默丢弃会让用户以为设置已生效。
 */
export function mergePreparationSettings(input: {
  readonly modes: readonly ModeRef[];
  readonly configOptions: readonly ConfigOptionView[];
  readonly selectedMode: string | null;
  readonly selectedConfig: PreparationConfigSelection;
}): PreparationSettingsView {
  const unavailable: string[] = [];
  const modeIds = new Set(input.modes.map((mode) => mode.modeId));
  const optionIds = new Set(input.configOptions.map((option) => option.id));
  if (input.selectedMode !== null && !modeIds.has(input.selectedMode)) unavailable.push(input.selectedMode);
  for (const id of Object.keys(input.selectedConfig)) {
    if (!optionIds.has(id)) unavailable.push(id);
  }
  const configOptions = input.configOptions.map((option) => {
    const own = input.selectedConfig[option.id];
    const value = own === undefined ? option.currentValue : own;
    return { option, value, changed: own !== undefined && own !== option.currentValue };
  });
  // 用户没选模式时呈现 Agent 的当前模式（清单里的第一项是服务端的默认顺序，不另排序）。
  const selectedMode =
    input.selectedMode !== null && modeIds.has(input.selectedMode)
      ? input.selectedMode
      : (input.modes[0]?.modeId ?? null);
  return {
    modes: input.modes,
    selectedMode,
    configOptions,
    unavailable,
  };
}

/**
 * 发送闸门：阶段、设置可用性、内容非空与附件状态。
 *
 * 准备阶段**不能**发送 prompt；只有 `ready` 与可证明未派发的 `failed` 允许发起首条/继续。
 */
export function preparationInputAvailability(input: {
  readonly phase: PreparationPhase;
  readonly settings: PreparationSettingsView;
  readonly text: string;
  readonly attachments: readonly DraftAttachment[];
  readonly connected: boolean;
  readonly attachmentsSupported: boolean;
  readonly leaseExpired?: boolean;
}): PreparationInputAvailability {
  if (input.phase === "unavailable") {
    return { canEditText: false, canSend: false, reason: "准备能力不可用" };
  }
  if (input.phase === "allocating" || input.phase === "restoring" || input.phase === "submitting") {
    return { canEditText: false, canSend: false, reason: PREPARATION_PHASE_LABELS[input.phase] };
  }
  if (input.phase === "accepted" || input.phase === "committed") {
    return { canEditText: false, canSend: false, reason: "首条消息已保存，后续内容请使用正式会话的输入框" };
  }
  if (input.phase === "uncertain") {
    return { canEditText: false, canSend: false, reason: PREPARATION_UNCERTAIN_NOTICE };
  }
  if (input.phase === "released") {
    return { canEditText: false, canSend: false, reason: "准备已释放" };
  }
  if (!input.connected) {
    return { canEditText: true, canSend: false, reason: "还没连上主机" };
  }
  if (input.settings.unavailable.length > 0) {
    return { canEditText: true, canSend: false, reason: PREPARATION_SETTINGS_UNAVAILABLE_NOTICE };
  }
  if (input.leaseExpired === true) {
    return { canEditText: true, canSend: false, reason: PREPARATION_LEASE_EXPIRED_NOTICE };
  }
  const uploading = input.attachments.some((attachment) => attachment.status === "uploading");
  if (uploading) {
    return { canEditText: true, canSend: false, reason: "附件正在上传，等待这一批结束" };
  }
  const failed = input.attachments.find(
    (attachment) =>
      attachment.status === "failed" ||
      attachment.status === "cancelled" ||
      attachment.status === "expired",
  );
  if (failed !== undefined) {
    return { canEditText: true, canSend: false, reason: `${failed.fileName} 上传失败，请重试附件` };
  }
  if (input.attachments.length > 0 && !input.attachmentsSupported) {
    return { canEditText: true, canSend: false, reason: "当前环境不支持附件上传" };
  }
  if (input.text.trim().length === 0 && input.attachments.length === 0) {
    return { canEditText: true, canSend: false, reason: "不能提交空内容" };
  }
  return { canEditText: true, canSend: true, reason: null };
}

/** 组装页面模型（唯一判定入口；页面与组件不再各自判断）。 */
export function buildPreparationPageModel(input: {
  readonly session: PreparationSessionState;
  readonly text: string;
  readonly attachments: readonly DraftAttachment[];
  readonly connected: boolean;
  readonly attachmentsSupported: boolean;
  readonly commandState?: PreparationPageModel["commandState"];
  readonly leaseExpired?: boolean;
}): PreparationPageModel {
  const settings = mergePreparationSettings({
    modes: input.session.modes,
    configOptions: input.session.configOptions,
    selectedMode: input.session.selectedMode,
    selectedConfig: input.session.selectedConfig,
  });
  const availability = preparationInputAvailability({
    phase: input.session.phase,
    settings,
    text: input.text,
    attachments: input.attachments,
    connected: input.connected,
    attachmentsSupported: input.attachmentsSupported,
    leaseExpired: input.leaseExpired === true,
  });
  return {
    sessionId: input.session.sessionId,
    agentId: input.session.agentId,
    workspace: input.session.workspace,
    phase: input.session.phase,
    phaseLabel: PREPARATION_PHASE_LABELS[input.session.phase],
    settings,
    text: input.text,
    attachments: input.attachments,
    input: availability,
    error: input.session.error,
    notice: input.session.notice,
    association: input.session.association,
    restartRequired: input.session.restartRequired,
    pendingRequestId: input.session.association?.requestId ?? null,
    commandState: input.commandState ?? null,
    leaseExpired: input.leaseExpired === true,
  };
}

// ── 请求构造（形状写在一处，页面与仓库都没有第二种拼法）────────────────────

/** `allocate` 的目录输入：已登记目录标识，或客户端选定的绝对路径（**一个**）。 */
export type PreparationWorkspaceInput =
  | { readonly directoryId: string }
  | { readonly workspacePath: string };

/** `session.preparation` 的 `allocate` body（只含 Agent、目录与标签页归属）。 */
export function buildAllocateBody(input: {
  readonly operationId: Uuid;
  readonly agentId: string;
  readonly workspace: PreparationWorkspaceInput;
  readonly tabNonce: string;
}): {
  readonly operationId: Uuid;
  readonly action: "allocate";
  readonly agentId: string;
  readonly workspace: PreparationWorkspaceInput;
  readonly tabNonce: string;
} {
  return {
    operationId: input.operationId,
    action: "allocate",
    agentId: input.agentId,
    workspace: input.workspace,
    tabNonce: input.tabNonce,
  };
}

/** `restore` body：凭据是必需项；已知的代只是旧状态提示，不参与拒绝判定。 */
export function buildRestoreBody(input: {
  readonly operationId: Uuid;
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly knownRuntimeEpoch?: string | null;
  readonly knownGeneration?: string | null;
}): {
  readonly operationId: Uuid;
  readonly action: "restore";
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly knownRuntimeEpoch?: string;
  readonly knownGeneration?: string;
} {
  return {
    operationId: input.operationId,
    action: "restore",
    sessionId: input.sessionId,
    receipt: input.receipt,
    ...(input.knownRuntimeEpoch === null || input.knownRuntimeEpoch === undefined
      ? {}
      : { knownRuntimeEpoch: input.knownRuntimeEpoch }),
    ...(input.knownGeneration === null || input.knownGeneration === undefined
      ? {}
      : { knownGeneration: input.knownGeneration }),
  };
}

/** `configure` body：资源代 + 至少一个待应用的值。 */
export function buildConfigureBody(input: {
  readonly operationId: Uuid;
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
  readonly mode?: string;
  readonly configOptions?: PreparationConfigSelection;
}): {
  readonly operationId: Uuid;
  readonly action: "configure";
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
  readonly mode?: string;
  readonly configOptions?: PreparationConfigSelection;
} {
  return {
    operationId: input.operationId,
    action: "configure",
    sessionId: input.sessionId,
    receipt: input.receipt,
    runtimeEpoch: input.runtimeEpoch,
    generation: input.generation,
    ...(input.mode === undefined ? {} : { mode: input.mode }),
    ...(input.configOptions === undefined ? {} : { configOptions: input.configOptions }),
  };
}

/** `release` body（临时资源与首条竞争时服务端可能回 `committed`）。 */
export function buildReleaseBody(input: {
  readonly operationId: Uuid;
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
}): {
  readonly operationId: Uuid;
  readonly action: "release";
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
} {
  return {
    operationId: input.operationId,
    action: "release",
    sessionId: input.sessionId,
    receipt: input.receipt,
    runtimeEpoch: input.runtimeEpoch,
    generation: input.generation,
  };
}

/** `renew` body：可见时的租约续期（当前凭据 + 当前资源代）；成功回 `ready`。 */
export function buildRenewBody(input: {
  readonly operationId: Uuid;
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
}): {
  readonly operationId: Uuid;
  readonly action: "renew";
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
} {
  return {
    operationId: input.operationId,
    action: "renew",
    sessionId: input.sessionId,
    receipt: input.receipt,
    runtimeEpoch: input.runtimeEpoch,
    generation: input.generation,
  };
}

/** `session.start` 的 `initial` 载荷（不带 `firstSubmissionId`，不携带任何凭据之外的秘密）。 */
export function buildInitialStartBody(input: {
  readonly requestId: Uuid;
  readonly sessionId: Uuid;
  readonly receipt: PreparationReceipt;
  readonly runtimeEpoch: string;
  readonly generation: string;
  readonly content: readonly PromptContentBlock[];
  readonly submissionId: string | null;
  readonly mode: string | null;
  readonly configOptions: PreparationConfigSelection;
}): SessionStartCommand["body"] {
  return {
    requestId: input.requestId,
    command: "session.start",
    sessionId: input.sessionId,
    payload: {
      kind: "initial",
      receipt: input.receipt,
      runtimeEpoch: input.runtimeEpoch,
      generation: input.generation,
      content: input.content,
      ...(input.submissionId === null ? {} : { submissionId: input.submissionId }),
      ...(input.mode === null ? {} : { mode: input.mode }),
      ...(Object.keys(input.configOptions).length === 0
        ? {}
        : { configOptions: input.configOptions }),
    },
  };
}

/**
 * `session.start` 的 `continue` 载荷（D8.1）：**新的**请求 ID、首次提交身份与失败尝试的引用。
 *
 * 不带正文/附件/凭据/运行期字段——继续只重新建立临时端点并派发同一份首条内容，正文始终只有一份。
 */
export function buildContinueStartBody(input: {
  readonly requestId: Uuid;
  readonly sessionId: Uuid;
  readonly firstSubmissionId: Uuid;
  readonly retryOfRequestId: Uuid;
  readonly mode: string | null;
  readonly configOptions: PreparationConfigSelection;
}): SessionStartCommand["body"] {
  return {
    requestId: input.requestId,
    command: "session.start",
    sessionId: input.sessionId,
    payload: {
      kind: "continue",
      firstSubmissionId: input.firstSubmissionId,
      retryOfRequestId: input.retryOfRequestId,
      ...(input.mode === null ? {} : { mode: input.mode }),
      ...(Object.keys(input.configOptions).length === 0
        ? {}
        : { configOptions: input.configOptions }),
    },
  };
}

/**
 * `$defs.sessionStartProjection` 的运行期收窄（`session.start` 完成结果与
 * `command.status.preparation` 同形）。
 */
export function isSessionStartProjection(value: unknown): value is SessionStartProjection {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  const stage = record["stage"];
  if (stage !== "saved_not_dispatched" && stage !== "dispatch_started" && stage !== "terminal") return false;
  for (const key of ["firstSubmissionId", "firstRequestId", "currentRequestId"] as const) {
    if (typeof record[key] !== "string") return false;
  }
  return typeof record["everDispatched"] === "boolean" && typeof record["canContinue"] === "boolean";
}

/**
 * 首条内容块：附件引用在前、正文、路径引用，顺序即用户看到的顺序。
 *
 * 附件必须已封存且带摘要；未封存的项拒绝组装（不发送半条消息）。
 */
export function buildInitialContent(input: {
  readonly text: string;
  readonly attachments: readonly DraftAttachment[];
  readonly pathRefs: readonly string[];
}): { readonly ok: true; readonly content: readonly PromptContentBlock[] } | { readonly ok: false; readonly reason: string } {
  const content: PromptContentBlock[] = [];
  for (const attachment of input.attachments) {
    if (attachment.status !== "sealed" || attachment.attachmentId === null || attachment.sha256 === null) {
      return { ok: false, reason: `${attachment.fileName} 还没有封存，不能提交` };
    }
    content.push({
      type: "attachment_ref",
      attachmentId: attachment.attachmentId,
      mediaType: attachment.mediaType,
      byteLength: attachment.byteLength,
      sha256: attachment.sha256,
    });
  }
  const text = input.text.trim();
  if (text.length > 0) content.push({ type: "text", text });
  for (const path of input.pathRefs) content.push({ type: "workspace_path_ref", path });
  if (content.length === 0) return { ok: false, reason: "不能提交空内容" };
  return { ok: true, content };
}
