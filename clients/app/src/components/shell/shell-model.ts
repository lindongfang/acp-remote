/**
 * 共享外壳的视图模型（`design.md` D1）。
 *
 * ## 为什么判定在这里而不在组件里
 *
 * 外壳要回答的问题只有三个：**要不要呈现全局顶栏**（路由判定）、**连接状态该怎么表达**
 * （胶囊档位与短词）、**这一页是不是明文托管**（开发模式横幅与徽标）。三者都是「输入 → 输出」的
 * 纯函数，因此全部落在本文件：组件只渲染模型，测试与浏览器用例断言的都是同一份判定。
 *
 * ## 判据只有一处
 *
 * 顶栏判据是 `usePathname()` 的取值（配对入口隐藏），**不**由各路由自己声明；连接状态只由胶囊
 * 表达（`spec` 的「连接状态 MUST 只由设备胶囊这一个控件表达」），因此这里不产出第二个状态徽标
 * 需要的任何东西。
 */

import { CONNECTION_STATE_LABELS } from "../../features/host-panel-model";
import type { ConnectionStateName } from "../../state";

/** 胶囊圆点的三档：正常 / 需要注意 / 阻断。 */
export type CapsuleTone = "ok" | "warn" | "danger";

/** 连接健康（不追加短词）的状态。 */
type HealthyState = "online" | "connecting" | "authenticating" | "replaying";

function isHealthyState(state: ConnectionStateName): state is HealthyState {
  return state === "online" || state === "connecting" || state === "authenticating" || state === "replaying";
}

/**
 * 连接状态 → 胶囊档位。
 *
 * 只有连接**确实不健康**的状态才升级：握手与追平阶段仍是在线路径（原型把 online 与 syncing
 * 合并成一个正常态），因此它们不追加短词（`spec` 的第一个场景）。`unpaired` 按原型的口径算阻断
 * ——那时没有任何连接可用，胶囊就是「还没配对」的入口。
 */
const TONE_BY_STATE: Readonly<Record<ConnectionStateName, CapsuleTone>> = {
  online: "ok",
  connecting: "ok",
  authenticating: "ok",
  replaying: "ok",
  pairing: "warn",
  disconnected: "warn",
  reconnecting: "warn",
  unpaired: "danger",
  revoked: "danger",
  incompatible: "danger",
  identity_changed: "danger",
  replaced: "danger",
};

/**
 * 需要注意与阻断态在胶囊里追加的短词：口径取自原型 `CONN` 表的 `chip` 字段
 * （「正在重连」「离线」「已被移除」「密钥已变化」「版本不兼容」「已在别处接管」「未配对」）。
 * 键集合由类型保证完整——少一个非健康状态就编译不过。
 */
const SHORT_WORD_BY_STATE: Readonly<Record<Exclude<ConnectionStateName, HealthyState>, string>> = {
  pairing: "配对中",
  disconnected: "离线",
  reconnecting: "正在重连",
  unpaired: "未配对",
  revoked: "已被移除",
  incompatible: "版本不兼容",
  identity_changed: "密钥已变化",
  replaced: "已在别处接管",
};

/** 设备胶囊的呈现模型。 */
export interface DeviceCapsuleModel {
  /** 连接状态机的当前状态；`null` = 组合根尚未装配，状态未知。 */
  readonly state: ConnectionStateName | null;
  readonly tone: CapsuleTone;
  /** 胶囊上的标识：主机的 host 段，或一个明确的「未知主机」。 */
  readonly deviceLabel: string;
  /** 完整连接状态文案（`aria-label`；短词只是它的浓缩）。 */
  readonly statusLabel: string;
  /** 需要注意与阻断态追加的短词；健康态为 `null`。 */
  readonly shortWord: string | null;
}

/** 外壳的呈现模型。 */
export interface ShellModel {
  readonly topBarVisible: boolean;
  /** 本页不在安全上下文（未加密托管）：顶部呈现开发模式横幅，顶栏呈现开发模式徽标。 */
  readonly insecure: boolean;
  readonly capsule: DeviceCapsuleModel;
}

/** 配对入口的两条路径（`/node-link/pair` 复用同一份入口主体）。 */
const PAIRING_PATHS: Readonly<Record<string, true>> = {
  "/pair": true,
  "/node-link/pair": true,
};

/** 路径是否落在配对入口上：查询串与片段不参与判定，尾斜杠归一后再逐字比对。 */
export function isPairingPath(pathname: string | null): boolean {
  if (pathname === null) return false;
  const pathOnly = pathname.split("?")[0]?.split("#")[0] ?? "";
  const normalized = pathOnly.length > 1 ? pathOnly.replace(/\/+$/, "") : pathOnly;
  return PAIRING_PATHS[normalized] === true;
}

/**
 * 是否呈现全局顶栏：配对入口呈现自己的头部，全局顶栏不呈现（`spec` 的第四个场景）。
 *
 * 这是**唯一**一条顶栏判据，根布局据此装配；各路由不再各自声明一次。
 */
export function showsGlobalTopBar(pathname: string | null): boolean {
  return !isPairingPath(pathname);
}

/**
 * 胶囊上的标识：主机地址（规范化来源）的 host 段。
 *
 * 未配对（还没有主机身份）或地址不可解析时给一个明确的说法，**不**编造设备名——原型胶囊里那
 * 三个字是硬编码 mock，不是产品数据。
 */
export function deviceLabelOf(address: string | null): string {
  if (address === null || address === "") return "未知主机";
  try {
    const host = new URL(address).host;
    return host === "" ? "未知主机" : host;
  } catch {
    return "未知主机";
  }
}

/** 连接状态未知（组合根尚未装配）时的胶囊。 */
const UNKNOWN_CAPSULE: DeviceCapsuleModel = {
  state: null,
  tone: "warn",
  deviceLabel: "未知主机",
  statusLabel: "连接状态未知",
  shortWord: "状态未知",
};

/** 构造设备胶囊模型。 */
export function buildDeviceCapsule(input: {
  readonly state: ConnectionStateName | null;
  readonly hostAddress: string | null;
}): DeviceCapsuleModel {
  const deviceLabel = deviceLabelOf(input.hostAddress);
  if (input.state === null) return { ...UNKNOWN_CAPSULE, deviceLabel };
  const tone = TONE_BY_STATE[input.state];
  return {
    state: input.state,
    tone,
    deviceLabel,
    statusLabel: CONNECTION_STATE_LABELS[input.state],
    shortWord: isHealthyState(input.state) ? null : SHORT_WORD_BY_STATE[input.state],
  };
}

/** 构造外壳模型。 */
export function buildShellModel(input: {
  readonly pathname: string | null;
  readonly connectionState: ConnectionStateName | null;
  readonly hostAddress: string | null;
  readonly secureContext: boolean;
}): ShellModel {
  return {
    topBarVisible: showsGlobalTopBar(input.pathname),
    insecure: !input.secureContext,
    capsule: buildDeviceCapsule({ state: input.connectionState, hostAddress: input.hostAddress }),
  };
}
