/**
 * 共享外壳的视图模型（`design.md` D1）。
 *
 * ## 为什么判定在这里而不在组件里
 *
 * 外壳要回答的问题只有三个：**要不要呈现全局顶栏**（路由判定）、**连接状态该怎么表达**
 * （胶囊只分在线/离线两档；阻断原因交给连接问题弹层）、**这一页是不是明文托管**（开发模式
 * 横幅与徽标）。三者都是「输入 → 输出」的纯函数，因此全部落在本文件：组件只渲染模型，
 * 测试与浏览器用例断言的都是同一份判定。
 *
 * ## 判据只有一处
 *
 * 顶栏判据是 `usePathname()` 的取值（配对入口隐藏），**不**由各路由自己声明；连接状态在胶囊
 * 里只表达网络可用性（在线/离线），不产出第二个状态徽标，也不在胶囊里塞阻断原因短词。
 */

import { networkAvailabilityOf } from "../../features/connection-problem";
import type { ConnectionAvailability } from "../../features/connection-problem";
import type { ConnectionStateName } from "../../state";

/** 胶囊圆点的三档：正常 / 需要注意 / 阻断。 */
export type CapsuleTone = "ok" | "warn" | "danger";

/** 设备胶囊的呈现模型。 */
export interface DeviceCapsuleModel {
  /** 连接状态机的当前状态；`null` = 组合根尚未装配，状态未知。 */
  readonly state: ConnectionStateName | null;
  /** 机器可读的网络可用性：在线 / 离线；尚未认证或状态未知时为 `null`。 */
  readonly availability: ConnectionAvailability | null;
  readonly tone: CapsuleTone;
  /** 胶囊上的标识：主机的 host 段，或一个明确的「未知主机」。 */
  readonly deviceLabel: string;
  /** 完整状态文案（`aria-label`）：已认证后只有「在线」「离线」两档。 */
  readonly statusLabel: string;
}

/** 外壳的呈现模型。 */
export interface ShellModel {
  readonly topBarVisible: boolean;
  /** 本页不在安全上下文（未加密托管）：顶部呈现开发模式横幅，顶栏呈现开发模式徽标。 */
  readonly insecure: boolean;
  /**
   * 本页按**视口高度**填满（对话页的三栏各自滚动、输入区吸在中栏底部）。
   *
   * 只有对话路由 `/session/<id>` 为 `true`；目录页、目录详情页、主机页与配对入口保持文档流
   * ——把它们也按视口高度固定，会让长列表失去「整页随文档滚动」的既有形态。
   */
  readonly contentFill: boolean;
  readonly capsule: DeviceCapsuleModel;
}

/** 配对入口的两条路径（`/node-link/pair` 复用同一份入口主体）。 */
const PAIRING_PATHS: Readonly<Record<string, true>> = {
  "/pair": true,
  "/node-link/pair": true,
};

/** 去掉查询串、片段与尾斜杠之后的路径（各判据共用的**唯一**一份归一化）。 */
function normalizePath(pathname: string | null): string | null {
  if (pathname === null) return null;
  const pathOnly = pathname.split("?")[0]?.split("#")[0] ?? "";
  return pathOnly.length > 1 ? pathOnly.replace(/\/+$/, "") : pathOnly;
}

/** 路径是否落在配对入口上：查询串与片段不参与判定，尾斜杠归一后再逐字比对。 */
export function isPairingPath(pathname: string | null): boolean {
  const normalized = normalizePath(pathname);
  return normalized !== null && PAIRING_PATHS[normalized] === true;
}

/** 业务路由的形态：根目录页、目录详情页与会话页。 */
const BUSINESS_PATH = /^\/(?:dir|session)(?:\/|$)/;

/**
 * 路径是否需要「已认证」才能呈现：目录页（`/`）、目录详情（`/dir/...`）与会话页
 * （`/session/...`）。
 *
 * 配对入口与主机页**不**在其中：配对本身就是认证之前的流程，主机页是连接面板本身
 * （它要能在首屏认证失败时被用户看到）。首屏认证闸门与连接问题弹层都用这一条判据。
 */
export function isBusinessPath(pathname: string | null): boolean {
  const normalized = normalizePath(pathname);
  if (normalized === null) return false;
  return normalized === "/" || BUSINESS_PATH.test(normalized);
}

/** 对话路由的形态：`/session/<id>`，会话标识恰好一段。 */
const CONVERSATION_PATH = /^\/session\/[^/]+$/;

/**
 * 路径是否是对话页（`/session/<id>`）。
 *
 * 这是 `contentFill` 的**唯一**判据：只有对话页需要按视口高度装配三栏与吸底输入区，其余路由
 * 继续随文档滚动（见 `design.md` D7/D10 与 `plan.md` WP3 的验收口径）。
 */
export function isConversationPath(pathname: string | null): boolean {
  const normalized = normalizePath(pathname);
  return normalized !== null && CONVERSATION_PATH.test(normalized);
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
  availability: null,
  tone: "warn",
  deviceLabel: "未知主机",
  statusLabel: "连接状态未知",
};

/**
 * 构造设备胶囊模型。
 *
 * 已认证后只分**在线 / 离线**两档：阻断原因、重连次数与下一步动作都由连接问题弹层表达，
 * 胶囊不再追加「正在重连 / 已被移除 / 版本不兼容 …」这类短词——同一个状态两处判据必然漂移。
 * 尚未认证（握手/配对）时给「状态未知」这一档，不谎称在线或离线。
 */
export function buildDeviceCapsule(input: {
  readonly state: ConnectionStateName | null;
  readonly hostAddress: string | null;
  /** 本次页面是否已经成功认证过；重连中的重新认证仍按离线呈现。 */
  readonly hasAuthenticated?: boolean;
}): DeviceCapsuleModel {
  const deviceLabel = deviceLabelOf(input.hostAddress);
  if (input.state === null) return { ...UNKNOWN_CAPSULE, deviceLabel };
  const availability = networkAvailabilityOf(input.state, input.hasAuthenticated === true);
  if (availability === null) {
    return {
      state: input.state,
      availability: null,
      tone: "warn",
      deviceLabel,
      statusLabel: "连接状态未知",
    };
  }
  if (availability === "online") {
    return { state: input.state, availability, tone: "ok", deviceLabel, statusLabel: "在线" };
  }
  return { state: input.state, availability, tone: "danger", deviceLabel, statusLabel: "离线" };
}

/** 构造外壳模型。 */
export function buildShellModel(input: {
  readonly pathname: string | null;
  readonly connectionState: ConnectionStateName | null;
  readonly hostAddress: string | null;
  readonly hasAuthenticated?: boolean;
  readonly secureContext: boolean;
}): ShellModel {
  return {
    topBarVisible: showsGlobalTopBar(input.pathname),
    insecure: !input.secureContext,
    contentFill: isConversationPath(input.pathname),
    capsule: buildDeviceCapsule({
      state: input.connectionState,
      hostAddress: input.hostAddress,
      ...(input.hasAuthenticated === undefined ? {} : { hasAuthenticated: input.hasAuthenticated }),
    }),
  };
}
