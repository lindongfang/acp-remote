/**
 * 配对页模型（`docs/FRONTEND_DESIGN.md` §4.1「主机连接与首次配对」+ R11「设备身份由不可导出密钥承载」）。
 *
 * ## 配对是 HTTPS 面，不是 Sync 面
 *
 * `schemas/sync/v1/pairing.schema.json` 定义的是 `claim`/`status` 两个 HTTPS 端点。
 * 本模块因此只做**状态机与视图模型**，`PairingTransport` 端口由组合根用 `fetch` 实现；
 * feature 层自己不发起任何网络请求（`AGENTS.md` §5：平台能力只由组合根装配）。
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
 */

import type { ErrorCode, PairingQrPayload, PublicError } from "../protocol";

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

/** 一次配对状态轮询的结果。 */
export type PairingPollResult =
  | { readonly kind: "pending_confirmation" }
  | { readonly kind: "approved"; readonly deviceId: string }
  | {
      readonly kind: "terminal";
      readonly status: "rejected" | "expired" | "consumed";
      readonly error: PublicError;
    }
  | { readonly kind: "error"; readonly error: PublicError };

/** 配对传输端口：由组合根用 HTTPS 实现（`fetch`），feature 层不接触网络 API。 */
export interface PairingTransport {
  /** 读取二维码内容并解析为 `pairing.qr` 载荷。 */
  readQrPayload(text: string): PairingQrPayload;
  /** 提交 claim（设备首次声明配对，`proof` 由设备私钥签名）。 */
  claim(input: { readonly qr: PairingQrPayload; readonly canonicalOrigin: string }): Promise<PublicError | null>;
  /** 轮询状态；`requestNonce` 每次轮询必须换新（schema 要求）。 */
  poll(input: {
    readonly qr: PairingQrPayload;
    readonly requestNonce: string;
  }): Promise<PairingPollResult>;
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
};
