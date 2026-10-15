/**
 * 配对页模型（`docs/FRONTEND_DESIGN.md` §4.1「主机连接与首次配对」+ R11「设备身份由不可导出密钥承载」）。
 *
 * ## 配对是 HTTPS 面，不是 Sync 面
 *
 * `schemas/sync/v1/pairing.schema.json` 定义的是 `claim`/`status` 两个 HTTPS 端点。
 * 本模块因此只做**状态机与视图模型**，`PairingTransport` 端口由组合根用 `fetch` 实现；
 * feature 层自己不发起任何网络请求（`AGENTS.md` §6：平台能力只由组合根装配）。
 *
 * ## 页面不得提供凭据输入
 *
 * 设备私钥由 `src/platform/secure-storage.web.ts` 生成并**不可导出**，浏览器端只出示公钥；
 * MCP 配置与 Agent 凭据一律在电脑上配置。因此本模型里没有凭据输入项，
 * `PairingPageModel` 的字段中也不存在任何可以承载秘密的槽位。
 *
 * ## 四种非成功终态必须可区分
 *
 * `rejected`（电脑端拒绝）、`expired`（二维码过期）、`consumed`（已被别的设备用过）、
 * `failed`（结构化错误）是四件不同的事，措辞与可执行的下一步各不相同。
 *
 * ## 秘密只活在一次配对里
 *
 * 二维码载荷含 `pairingSecret`。它从地址片段读入、用完（claim 与每次轮询的 `proof` 都要它）
 * 即弃：载荷由调用方持有在**私有**字段里，绝不进任何状态快照，也不落日志与地址历史
 * （§7.1 读取后立即清 fragment；§7.3 成功或终态后清除内存中的 secret）。
 * 视图模型 `PairingPageModel` 里因此没有任何可以承载秘密的槽位。
 */

import type { ErrorCode, PairingQrPayload, PublicError } from "../protocol";
import type { PairedHostRecord } from "../sync-client";

/** 配对阶段。`approved` 之后进入已配对（连接状态机由 `pair_succeeded` 接手）。 */
export type PairingPhase =
  | "idle"
  | "awaiting_qr"
  | "claiming"
  | "pending_confirmation"
  | "approved"
  | "rejected"
  | "expired"
  | "consumed"
  | "failed";

/** 配对页模型。 */
export interface PairingPageModel {
  readonly phase: PairingPhase;
  /** 扫码得到的主机出示信息；`null` 表示尚未扫码。 */
  readonly host: { readonly address: string; readonly hostId: string | null } | null;
  /** 已批准时的本机设备标识（wire 上的 `deviceId`）。 */
  readonly deviceId: string | null;
  /** 是否必须由用户在电脑上确认（`pending_confirmation`）。 */
  readonly awaitingDesktopConfirmation: boolean;
  /** 失败/被拒时的结构化原因。 */
  readonly error: PublicError | null;
  /** 当前阶段的下一步指引；各阶段文案互不相同。 */
  readonly guidance: string;
  /** 是否可以重新开始（重试/重新扫码）。 */
  readonly canRestart: boolean;
}

/** 已批准结果：设备元数据**与**主机身份材料（§7.3：`approved` 响应两者强制给出）。 */
export interface PairingApprovedResult {
  /** 本机设备标识（wire 上的 `deviceId`）。 */
  readonly deviceId: string;
  /** 主机标识；必须与二维码里的 `hostId` 是同一台。 */
  readonly hostId: string;
  /** 主机公钥（base64url-65 字节）：后续握手验证主机证明的唯一信任根。 */
  readonly hostPublicKey: string;
}

/** 一次配对状态轮询的结果。 */
export type PairingPollResult =
  | { readonly kind: "pending_confirmation" }
  | ({ readonly kind: "approved" } & PairingApprovedResult)
  | {
      readonly kind: "terminal";
      readonly status: "rejected" | "expired" | "consumed";
      readonly error: PublicError;
    }
  | { readonly kind: "error"; readonly error: PublicError };

/** 配对传输端口：由组合根用 HTTPS 实现（`fetch`），feature 层不接触网络 API。 */
export interface PairingTransport {
  /**
   * 二维码内容 → `pairing.qr` 载荷。
   *
   * 接受二维码地址、地址片段与 base64url 载荷三种**容器**形态；**载荷**校验只有一套，
   * 任何不符（JSON 非法、协议版本不符、字段缺失或长度不符、来源形状非法、已过期）
   * 都以 `PairingQrInvalidError` 明确失败，绝不尽力解析。
   */
  readQrPayload(text: string): PairingQrPayload;
  /** 提交 claim（设备首次声明配对，`proof` 由设备私钥签名）。 */
  claim(input: { readonly qr: PairingQrPayload; readonly canonicalOrigin: string }): Promise<PublicError | null>;
  /**
   * 轮询状态。
   *
   * 已批准时**必须**同时返回主机身份材料：缺材料、材料形状不符或主机标识与二维码不是同一台
   * 一律返回 `kind: "error"`，而不是「已批准」——没有公钥的已配对记录会让此后**每次**连接
   * 都进「主机身份变化」阻断态，比在配对当场明确失败更难诊断（design D2）。
   *
   * `requestNonce` 每次轮询必须换新（§7.3 的 schema 要求）。
   */
  poll(input: {
    readonly qr: PairingQrPayload;
    readonly requestNonce: string;
  }): Promise<PairingPollResult>;
}

/**
 * 配对会话需要的平台出口（组合根实现）。
 *
 * 为什么是这一组窄接口而不是「把组合根整只交给 feature 层」：每一项都是一次可单独替换的
 * 平台能力（网络、地址栏、本地存储、时钟、随机源），测试替身可以只替换关心的那一项，
 * 而被测的仍然是本模块真实的配对状态机与用例。
 *
 * 页面**不**接触这些出口：它只调 `ClientStore` 的配对用例。
 */
export interface PairingWiring {
  /** HTTPS 面：二维码解析、claim 与状态轮询。 */
  readonly transport: PairingTransport;
  /**
   * 读取地址片段里的配对内容并**立即**清除该片段（§7.1：秘密位于 fragment，
   * 读取后立即 `history.replaceState`）；没有配对内容时返回 `null`。
   */
  takeQrTextFromFragment(): string | null;
  /**
   * 写入已配对主机记录：主机标识 + 主机公钥 + 规范化来源（来源在键里，见组合根）。
   *
   * 落盘成功才允许把设备当作已配对——否则后续握手没有可用的信任根。
   */
  rememberPairedHost(record: PairedHostRecord): Promise<void>;
  /** 已配对后的收口：装配同步连接并触发一次连接（MUST NOT 停在等待扫码）。 */
  onPaired(record: PairedHostRecord): Promise<void>;
  /** 安排下一次状态轮询；返回取消函数。 */
  schedule(delayMs: number, task: () => void): () => void;
  /** 每次轮询换新的 `requestNonce`（base64url-32 字节）；随机源只由组合根持有。 */
  nextRequestNonce(): string;
}

/** 状态轮询的节奏：§7.3 的状态查询可安全重复，两秒足以让「电脑上确认」后只剩一个窗口的延迟。 */
export const PAIRING_POLL_INTERVAL_MS = 2_000;

/**
 * 二维码内容结构校验失败。
 *
 * `code` 取自机器登记的错误码表（`protocol.invalid_json` / `protocol.schema_invalid`），
 * 因此页面呈现的原因与协议词表同源；`message` 只说**哪一项**不合法，绝不回显内容或秘密。
 */
export class PairingQrInvalidError extends Error {
  readonly code: Extract<ErrorCode, "protocol.invalid_json" | "protocol.schema_invalid">;

  constructor(
    code: Extract<ErrorCode, "protocol.invalid_json" | "protocol.schema_invalid">,
    message: string,
  ) {
    super(message);
    this.name = "PairingQrInvalidError";
    this.code = code;
  }
}

const GUIDANCE: Readonly<Record<PairingPhase, string>> = {
  idle: "在电脑上打开「添加设备」，用本页面扫描它出示的二维码。",
  awaiting_qr: "把摄像头对准电脑上的配对二维码，或粘贴二维码内容。",
  claiming: "正在向这台电脑声明本设备，请稍候。",
  pending_confirmation: "请到电脑上确认这台设备的配对请求。确认后本页会自动继续。",
  approved: "配对已完成，正在建立同步连接。",
  rejected: "电脑端拒绝了这次配对请求。如需继续，请在电脑上重新发起并扫码。",
  expired: "这次配对二维码已过期。请在电脑上重新生成二维码后再扫。",
  consumed: "这次配对二维码已被使用过。请在电脑上重新生成一个新的二维码。",
  failed: "配对未完成。请按下面的原因处理后重试。",
};


/** 配对页初始状态。 */
export function initialPairingPageModel(): PairingPageModel {
  return {
    phase: "idle",
    host: null,
    deviceId: null,
    awaitingDesktopConfirmation: false,
    error: null,
    guidance: GUIDANCE.idle,
    canRestart: false,
  };
}

/** 阶段 → 模型（唯一的组装点，避免各分支各写一份文案）。 */
function modelFor(phase: PairingPhase, overrides: Partial<PairingPageModel>): PairingPageModel {
  return {
    phase,
    host: overrides.host ?? null,
    deviceId: overrides.deviceId ?? null,
    awaitingDesktopConfirmation: phase === "pending_confirmation",
    error: overrides.error ?? null,
    guidance: GUIDANCE[phase],
    canRestart: phase === "rejected" || phase === "expired" || phase === "consumed" || phase === "failed",
  };
}

/** 二维码载荷 → 主机出示信息（只取地址与主机标识，不展示也不推导任何路径）。 */
export function applyScannedQr(current: PairingPageModel, qr: PairingQrPayload): PairingPageModel {
  return modelFor("awaiting_qr", { ...current, host: { address: qr.canonicalOrigin, hostId: qr.hostId } });
}

/** claim 已发出。 */
export function applyClaiming(current: PairingPageModel): PairingPageModel {
  return modelFor("claiming", current);
}

/**
 * 配对是否**正在进行**（内容已读到、还没走到终态）。
 *
 * 进行中意味着「这一刻持有一份含 `pairingSecret` 的二维码载荷」，因此这一段时间里
 * 不接受第二次提交（否则会出现两条并行的 claim 与轮询），页面也据此禁用输入。
 */
export function isPairingInFlight(model: PairingPageModel): boolean {
  return model.phase === "awaiting_qr" || model.phase === "claiming" || model.phase === "pending_confirmation";
}

/**
 * 明确失败：结构校验不过、claim 被拒、已配对记录写不进去，都落到 `failed` 阶段并带上原因。
 *
 * 与「四种非成功终态」分开：那些是**主机的**判定（拒绝/过期/已用），这里是本客户端这一侧
 * 明确的失败，措辞与可执行的下一步不同。
 */
export function applyPairingFailure(current: PairingPageModel, error: PublicError): PairingPageModel {
  return modelFor("failed", { ...current, error });
}

/**
 * **收口**失败：已配对记录已落盘、连接机已推进到「已配对」，但组合根没能把连接带起来。
 *
 * 与 {@link applyPairingFailure} 刻意分开：这里**不**把阶段退回 `failed`——配对本身已经成功，
 * 已配对记录也已经写好，声称「配对未完成」会与主机面板已经显示的主机身份自相矛盾；
 * 失败的只是「把同步连接装配起来」这一步。因此只补上可读原因，阶段与可重来性都保持不变。
 */
export function applyPairingCloseoutFailure(current: PairingPageModel, error: PublicError): PairingPageModel {
  return modelFor(current.phase, { ...current, error });
}

/** 配对失败的可读原因：`details` 一律留空，避免任何来源的内容经它外溢。 */
export function publicErrorForQrInput(error: unknown): PublicError {
  if (error instanceof PairingQrInvalidError) {
    return { code: error.code, message: error.message, retryable: false, details: {} };
  }
  return {
    code: "protocol.schema_invalid",
    message: "内容不是合法的配对二维码载荷。",
    retryable: false,
    details: {},
  };
}

/**
 * 传输层**拒绝**（`claim` / `poll` 的 promise 抛出，而不是按端口约定返回 `PublicError`）→ 可读原因。
 *
 * 与 {@link publicErrorForQrInput} 并列：那是「内容不合法」，这里是「这次请求没能完成」——
 * 网络不可达、`fetch` 失败、响应不是 JSON、本地存储不可用都会走到这里。
 *
 * 设备身份已经绑在**另一个来源**上时（`SecureStorageError.kind === "origin_mismatch"`，
 * R11 场景 3：身份不得跨来源迁移）给出专门的码与文案：那不是偶发故障，重扫同一个二维码
 * 也不会好。其余一律落到**可重试**的 `internal.unavailable`——上面这些原因下一次的结果可能不同。
 *
 * 平台层的错误只按**结构化形状**识别（`kind` 字段），因此 feature 层不 import `src/platform/`：
 * 那是被分层测试判死的反向依赖（`src/layers.ts`），而这里需要的只是「来源不符」这一个事实。
 * `details` 一律留空（同 `publicErrorForQrInput`）：底层错误文本可能带着请求上下文，不能外溢。
 */
export function publicErrorForTransportFailure(error: unknown): PublicError {
  // 「设备身份已绑在另一个来源」只按**结构化形状**识别（`SecureStorageError` 的 `kind` 字段）：
  // `in` 收窄把 `kind` 读成 `unknown`，因此这里只做一次字符串比较，不假设任何其它形状。
  const originMismatch =
    typeof error === "object" && error !== null && "kind" in error && error.kind === "origin_mismatch";
  if (originMismatch) {
    return {
      code: "auth.origin_mismatch",
      message: PAIRING_ERROR_HINTS["auth.origin_mismatch"] ?? "这个二维码来自另一个地址：请确认扫码对象。",
      retryable: false,
      details: {},
    };
  }
  return {
    code: "internal.unavailable",
    message: "配对请求未能完成（网络或本地存储不可用）：请重试。",
    retryable: true,
    details: {},
  };
}

/**
 * **收口**失败（组合根的 `onPaired` 抛出）→ 可读原因。
 *
 * 与 {@link publicErrorForTransportFailure} 刻意分开，尽管码、可重试性与 `details` 三者完全一致：
 * 走到这条路径时配对**已经完成**——已配对记录已落盘、连接机已推进到「已配对」、页面正在显示
 * `approved` 阶段的引导语「配对已完成，正在建立同步连接。」。通用文案「配对请求未能完成」在那一刻
 * 既与事实不符（记录写好了、主机身份也要显示了），又与同页的引导语正面对撞；因此这里只换 `message`，
 * 说清「配对成了、只有这条同步连接没起来」。
 *
 * `_error` 刻意不读取：收口失败的原因（装配连接、读设备身份、本地存储不可用）一律不外溢平台原文，
 * `details` 因此保持为空对象（同 {@link publicErrorForQrInput}）。
 */
export function publicErrorForCloseoutFailure(_error: unknown): PublicError {
  return {
    code: "internal.unavailable",
    message: "配对已完成，但同步连接未能建立（网络或本地存储不可用）：请重试。",
    retryable: true,
    details: {},
  };
}

/** 已配对主机记录写不进去（本地存储不可用）：这次配对不算完成。 */
export function pairedHostWriteFailure(error: unknown): PublicError {
  const detail = error instanceof Error && error.message.length > 0 ? error.message : "本地存储不可用";
  return {
    code: "internal.unavailable",
    message: `已配对主机记录未能写入（${detail}）：请重试。`,
    retryable: true,
    details: {},
  };
}

/**
 * 已批准结果 + 二维码来源 → 待落盘的已配对主机记录。
 *
 * 三个字段缺一不可：主机标识与规范化来源决定「验证谁」，主机公钥是验签的唯一信任根。
 * 材料取自**已批准响应**（§7.3：`approved` 响应返回设备非秘密元数据与 Host identity），
 * 二维码只提供规范化来源——它决定设备身份绑定的来源（`docs/FRONTEND_DESIGN.md` §4.5）。
 */
export function pairedHostRecordFor(
  approved: PairingApprovedResult,
  canonicalOrigin: string,
): PairedHostRecord {
  return { hostId: approved.hostId, publicKeyBase64Url: approved.hostPublicKey, canonicalOrigin };
}

/** 轮询结果 → 阶段。四种非成功结果各自落到自己的阶段上。 */
export function applyPollResult(current: PairingPageModel, result: PairingPollResult): PairingPageModel {
  switch (result.kind) {
    case "pending_confirmation":
      return modelFor("pending_confirmation", current);
    case "approved":
      return modelFor("approved", { ...current, deviceId: result.deviceId });
    case "terminal":
      return modelFor(result.status, { ...current, error: result.error });
    case "error":
      return modelFor("failed", { ...current, error: result.error });
  }
}

/** 传输层错误码 → 建议动作的文案（错误码封闭词表，不自造分类）。 */
export const PAIRING_ERROR_HINTS: Readonly<Partial<Record<ErrorCode, string>>> = {
  "pairing.expired": "二维码已过期：请在电脑上重新生成。",
  "pairing.consumed": "二维码已被使用：请在电脑上重新生成。",
  "pairing.already_claimed": "这台电脑已经登记过本设备，可直接尝试连接。",
  "auth.origin_mismatch": "这个二维码来自另一个地址：身份不得跨来源迁移，请确认扫码对象。",
  "authorization.scope_denied": "电脑端没有授予本设备所需权限：请在电脑上调整权限后重试。",
  "protocol.invalid_json": "这段内容不是二维码里的配对载荷：请重新复制完整的二维码内容。",
  "protocol.schema_invalid": "这段内容不是本版本的配对载荷：请在电脑上重新生成二维码后重试。",
};
