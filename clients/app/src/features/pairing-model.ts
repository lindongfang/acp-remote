/**
 * 配对页模型（`docs/FRONTEND_DESIGN.md` §4.1「主机连接与首次配对」+ R11「设备身份由不可导出密钥承载」）。
 *
 * ## 配对是 HTTPS 面，不是 Sync 面
 *
 * `schemas/sync/v1/pairing.schema.json` 定义的是 `claim`/`status` 两个 HTTPS 端点。
 * 本模块因此只做**状态机与视图模型**，`PairingTransport` 端口由组合根用 `fetch` 实现；
 * feature 层自己不发起任何网络请求（`AGENTS.md` §6：平台能力只由组合根装配）。
 *
 * ## 页面不得提供设备凭据输入
 *
 * 设备私钥由 `src/platform/secure-storage.web.ts` 生成并**不可导出**，浏览器端只出示公钥；
 * MCP 配置与 Agent 凭据一律在电脑上配置。配对码入口只接受一个一次性配对码，它由组件内存
 * 持有并交给 `submitPairingCode`，用完即弃；`PairingPageModel` 的字段中因此仍然不存在任何
 * 可以承载秘密的槽位。
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
 *
 * ## 两类入口：类型由路由声明，入口只校验
 *
 * 本机生成两类配对二维码：sync 设备配对（`/pair#data=…`）与 node-link 配对
 * （`/node-link/pair#data=…`，`NODE_LINK_PROTOCOL.md` §13.1）。两类入口共用这一套状态机与视图
 * 模型，`PairingKind` 因此是**入口的入参**（由路由给出），而载荷类型由组合根在读取时标注；
 * 入口只把两者对上（`kindMismatchFailure`），不靠载荷形状猜（design D2/D3）。
 */

import type { ErrorCode, NodeLinkQrPayload, PairingQrPayload, PublicError } from "../protocol";
import type { PairedHostRecord } from "../sync-client";

/**
 * 配对阶段。
 *
 * - `reading`：页面刚挂载、还没读地址片段。它与「没有载荷」刻意分开，避免首帧把
 *   「还没读地址」误呈现成失败（design D2）。
 * - `idle`：读完了、地址里没有配对内容（`no_payload`）。
 * - `awaiting_code`：`/pair` 没有地址片段，等待用户输入 8 位配对码（原型 S0）。
 * - `looking_up`：配对码已提交，正在用同源 lookup 取回绑定与秘密（原型 S1）。
 * - `approved` 之后进入已配对（连接状态机由 `pair_succeeded` 接手）。
 *
 * 地址片段与配对码是两种**载荷获取方式**（design D1），之后共用同一套状态机；
 * `node-link` 入口没有配对码，无片段时仍是 `idle`。
 */
export type PairingPhase =
  | "reading"
  | "idle"
  | "awaiting_code"
  | "looking_up"
  | "claiming"
  | "pending_confirmation"
  | "approved"
  | "rejected"
  | "expired"
  | "consumed"
  /** 已读到 node-link 配对内容：本客户端不是它的参与方，只呈现对端信息（见 `applyScannedNodeLink`）。 */
  | "node_link_scanned"
  | "failed";

/**
 * 本客户端这一侧的失败类别（design D6）。
 *
 * 与四种终态（`rejected` / `expired` / `consumed` 与 `node_link_scanned`）分开：那些是主机或载荷
 * 类型给出的结论，这里回答的是「为什么本页没能把配对推进下去」。每一类对应一套固定文案。
 */
export type PairingFailureKind =
  | "no_payload"
  | "unrecognized"
  | "kind_mismatch"
  | "origin_mismatch"
  /**
   * 配对码入口的首次信任边界（design D5）：同一 canonical origin 已登记**另一把**主机公钥。
   * 与上面的 `origin_mismatch` 分开：来源没变，变的是这台电脑的身份。
   */
  | "host_identity_changed"
  | "insecure_context"
  | "request_failed"
  /**
   * 配对码入口的任何失败（design D6）：格式不符、lookup 失败或响应丢失、码已被消耗，
   * 以及会话在等待确认期间按 `expiresAt` 过期（含服务端返回 `pairing.expired`）。
   */
  | "code_invalid";

/**
 * 配对页的固定呈现（design D2/D6）：组件只渲染这一份内容，不再按 `phase` 自行分支文案。
 *
 * 文案是产品口径的固定文本（原型 S0–S8），因此后端错误 `message` 与任何载荷内容都没有
 * 进入页面的通道；`error.code` 仍留在模型上，供诊断与 `data-pairing-error` 锚点使用。
 */
export interface PairingPresentation {
  /** `progress` 居中转圈；`code_entry` 配对码输入；`failure` 失败页；`info` 只呈现对端信息。 */
  readonly kind: "progress" | "code_entry" | "failure" | "info";
  readonly title: string;
  readonly lede: string;
  /** 标题下方的补充说明；没有时为 `null`。 */
  readonly note: string | null;
  /** 失败/提示页的警示条；没有时为 `null`。 */
  readonly warning: string | null;
  readonly tone: "neutral" | "warning" | "danger";
  /** progress 时是否在圆环内显示剩余有效期。 */
  readonly countdown: boolean;
}

/** 两类配对入口（design D2）：由**路由**声明，入口据此校验载荷类型，而不是从载荷形状去猜。 */
export type PairingKind = "sync" | "node-link";

/**
 * 这一次配对从哪个**入口**取得载荷（design D1/D6）：`qr` = 地址片段里的二维码载荷，
 * `code` = `/pair` 无片段时的 8 位配对码（lookup 成功后与二维码共用同一条 claim/轮询链路）。
 *
 * 过期呈现按它分流：二维码入口的倒计时归零与服务端 `pairing.expired` 都呈现
 * 「配对地址已过期」（S7，恢复路径是重新打开配对地址）；配对码入口则一律呈现固定文案
 * 「配对码不对或已过期，请重新生成配对码」（S6，恢复路径是重新生成配对码）。
 */
export type PairingEntry = "qr" | "code";

/**
 * 二维码内容读出的结果：载荷 + 它的类型。
 *
 * 类型由组合根在读取时标注（`readQrPayload`），因此「这是哪一类配对内容」是读出的事实，
 * 不是入口对载荷形状的猜测。入口把它的 `kind` 与路由声明的 {@link PairingKind} 对上。
 */
export type PairingQrRead =
  | { readonly kind: "sync"; readonly qr: PairingQrPayload }
  | { readonly kind: "node-link"; readonly qr: NodeLinkQrPayload };

/** 配对页模型。 */
export interface PairingPageModel {
  readonly phase: PairingPhase;
  /** 本页路由声明的配对类型；决定「没有载荷」等文案的类别名。 */
  readonly kind: PairingKind;
  /**
   * 这次配对从哪个入口取得载荷（见 {@link PairingEntry}）；决定过期时呈现 S7 还是 S6。
   *
   * 在进入 claim 时与载荷一起写入，之后整条 claim/轮询链路保持不变。
   */
  readonly entry: PairingEntry;
  /** 读到的对端信息；`null` 表示还没有可呈现的对端。 */
  readonly host: { readonly address: string; readonly hostId: string | null } | null;
  /** 已批准时的本机设备标识（wire 上的 `deviceId`）。 */
  readonly deviceId: string | null;
  /** 配对载荷的绝对截止时间（毫秒）；配对码输入态、node-link 与无载荷为 `null`。 */
  readonly expiresAtMs: number | null;
  /**
   * 本页的 canonical origin（配对码输入态显示「将与此电脑配对」）。
   *
   * 只有配对码入口与无载荷重写会写入它；fragment 入口在读取前保持 `null`。它只是**公开**的
   * 来源字符串，配对码本身永远不会进入模型（码只活在组件内存与私有字段里）。
   */
  readonly origin: string | null;
  /** 本页是否处于安全上下文；配对码输入态据此显示 HTTPS 标识。 */
  readonly secureContext: boolean;
  /** 本客户端这一侧的失败类别；非失败阶段为 `null`。 */
  readonly failure: PairingFailureKind | null;
  /** 失败/被拒时的结构化原因（只供诊断与锚点，不直接渲染 `message`）。 */
  readonly error: PublicError | null;
  /** 页面唯一渲染来源：固定标题/说明/警示/色调/是否显示倒计时。 */
  readonly presentation: PairingPresentation;
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

/**
 * 一次配对码 lookup 的结果。
 *
 * 成功时返回与二维码载荷**同形**的 `qr`：之后的状态机、claim/status 与 transcript 因此
 * 只有一套实现（design D6）。失败不带任何后端正文——页面只呈现固定文案，不据此推断
 * 配对是否存在（design D4）。
 */
export type PairingLookupOutcome =
  | { readonly kind: "ok"; readonly qr: PairingQrPayload }
  | { readonly kind: "error" };

/** 配对传输端口：由组合根用 HTTPS 实现（`fetch`），feature 层不接触网络 API。 */
export interface PairingTransport {
  /**
   * 二维码内容 → 载荷 + 它的**类型**（design D3 的读取口）。
   *
   * 接受二维码地址、地址片段与 base64url 载荷三种**容器**形态；**载荷**校验按类型各一套，
   * 任何不符（JSON 非法、协议版本不符、字段缺失或长度不符、来源/端点形状非法、已过期）
   * 都以 `PairingQrInvalidError` 明确失败，绝不尽力解析。
   *
   * 这里只**标注**类型，不判定「入口是否接受它」：同一份 node-link 内容在 sync 入口是
   * 明确失败、在 node-link 入口是已读到的内容，那是入口的判定（design D2/D3）。
   */
  readQrPayload(text: string): PairingQrRead;
  /**
   * `POST /sync/v1/pairing/lookup`（§7.5）：用 8 位配对码一次性取回绑定与秘密。
   *
   * 只对同源 `canonicalOrigin` 发请求；任何失败（HTTP 非 200、正文形状不符、字段长度不符、
   * 响应里的来源与请求来源不同、网络丢失）都收敛成 `kind: "error"`，不携带正文。
   * 成功即消耗配对码，调用方不得重试或缓存响应。
   */
  lookup(input: {
    readonly canonicalOrigin: string;
    readonly pairingCode: string;
  }): Promise<PairingLookupOutcome>;
  /**
   * 提交 claim（设备首次声明配对，`proof` 由设备私钥签名）。
   *
   * 认领响应携带主机证明 `hostProof`，实现 MUST 用二维码里**固定**的主机公钥验证它（§7.2），
   * 且 MUST NOT 用响应自带的材料「自证」。缺材料或验签失败都返回明确错误，调用方据此既不进行
   * 轮询，也不写入任何已配对主机记录（design D2 的 L1）。
   */
  claim(input: { readonly qr: PairingQrPayload; readonly canonicalOrigin: string }): Promise<PublicError | null>;
  /**
   * 轮询状态。
   *
   * 已批准时**必须**同时返回主机身份材料：缺材料、材料形状不符、主机标识与二维码不是同一台，
   * 或主机公钥与二维码里那把**不是逐字节相同**，都一律返回 `kind: "error"`，而不是「已批准」——
   * 已批准响应没有任何主机签名，因此那把公钥只能靠这条比对来确立；放水会让此后**每次**连接
   * 都进「主机身份变化」阻断态，比在配对当场明确失败更难诊断（design D2 的 L3）。
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
  /** 已配对后的收口：装配同步连接并触发一次连接（MUST NOT 停在读地址阶段）。 */
  onPaired(record: PairedHostRecord): Promise<void>;
  /** 安排下一次状态轮询；返回取消函数。 */
  schedule(delayMs: number, task: () => void): () => void;
  /** 每次轮询换新的 `requestNonce`（base64url-32 字节）；随机源只由组合根持有。 */
  nextRequestNonce(): string;
  /**
   * 环境事实：本页是否处于安全上下文（`isSecureContext`；PWA 形态即 https 或 loopback）。
   *
   * 与网络、时钟一样由组合根从 `CompositionInputs` 注入（design D2）：非安全上下文在正常
   * 测试环境里造不出来，必须可注入才能写成断言；`src/platform/**` 的成员表是冻结的装配面，
   * 因此这类一次性环境事实不走平台端口。
   */
  isSecureContext(): boolean;
  /** 环境事实：本页实际来源（§3.2 的 canonical origin；生产实现取 `location.origin`）。
   *
   * 与 {@link isSecureContext} 同一注入面：入口据此校验二维码声明的 `canonicalOrigin`。
   */
  currentOrigin(): string;
  /**
   * 配对码入口的主机身份边界（design D5）：同一 canonical origin 已有**不同**主机公钥的
   * 已配对记录时返回错误，调用方据此失败关闭，绝不静默覆盖既有记录。
   *
   * 二维码入口不调用它：那条路径的公钥来自 fragment 的带外载荷，校验强度不变。
   */
  checkCodeEntryHost(input: {
    readonly canonicalOrigin: string;
    readonly hostPublicKey: string;
  }): Promise<PublicError | null>;
}

/** 状态轮询的节奏：§7.3 的状态查询可安全重复，两秒足以让「电脑上确认」后只剩一个窗口的延迟。 */
export const PAIRING_POLL_INTERVAL_MS = 2_000;

/**
 * Crockford base32 字母表（design D2）：不含 `I`/`L`/`O`/`U`，40 bit 恰好 8 个字符。
 * 输入归一化把 `I`/`L → 1`、`O → 0`；映射后仍在表外的字符（如 `U`）判为格式不符。
 */
const PAIRING_CODE_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/**
 * 二维码内容校验失败。
 *
 * `code` 取自机器登记的错误码表（`protocol.invalid_json` / `protocol.schema_invalid`，以及
 * 打开时已过期用的 `pairing.expired`），因此页面呈现的原因与协议词表同源；
 * `message` 只说**哪一项**不合法，绝不回显内容或秘密。
 */
export class PairingQrInvalidError extends Error {
  readonly code: Extract<
    ErrorCode,
    "protocol.invalid_json" | "protocol.schema_invalid" | "pairing.expired"
  >;

  constructor(
    code: Extract<
      ErrorCode,
      "protocol.invalid_json" | "protocol.schema_invalid" | "pairing.expired"
    >,
    message: string,
  ) {
    super(message);
    this.name = "PairingQrInvalidError";
    this.code = code;
  }
}

/**
 * 「没有配对信息」的标题与说明按入口类型分开：node-link 入口不是 sync 设备配对，
 * 说「没有节点配对信息」比复用 sync 的措辞更准确（design D8）。
 */
const NO_PAYLOAD_COPY: Readonly<Record<PairingKind, { readonly title: string; readonly lede: string }>> = {
  sync: {
    title: "这个地址里没有配对信息",
    lede: "配对信息只随电脑生成的配对地址传入；当前地址里没有可读的配对内容。",
  },
  "node-link": {
    title: "这个地址里没有节点配对信息",
    lede: "节点配对信息只随节点生成的配对地址传入；当前地址里没有可读的配对内容。",
  },
};

/** 「没有载荷」之外的本侧失败类别 → 固定文案（原型 S5–S8）。 */
const FAILURE_COPY: Readonly<
  Record<
    Exclude<PairingFailureKind, "no_payload" | "kind_mismatch">,
    { readonly title: string; readonly lede: string; readonly warning: string }
  >
> = {
  unrecognized: {
    title: "无法识别这个地址",
    lede: "地址里的配对信息不完整或已被破坏，本页没有发出任何请求。",
    warning: "常见原因：链接在复制时被截断，或者二维码识别不完整。",
  },
  origin_mismatch: {
    title: "这个地址属于另一台电脑",
    lede: "这个配对地址来自另一个来源，而一个浏览器身份只绑定一个地址，不能跨来源迁移。",
    warning: "请确认扫码对象：打开这台电脑自己的配对地址。",
  },
  // 配对码入口的首次信任（design D5）：来源没变，但这台电脑出示的主机公钥与已登记的不同。
  // 与 `origin_mismatch` 分开，才能让用户分清「开错了地址」与「这台电脑的身份变了」。
  host_identity_changed: {
    title: "这台电脑的主机身份已变化",
    lede: "这个地址此前已登记过另一把主机公钥：本页不会覆盖既有记录，也没有继续这次配对。",
    warning: "请在电脑上重新发起配对；如果这台电脑确实更换过身份，请先确认新身份可信。",
  },
  insecure_context: {
    title: "需要 HTTPS 才能配对",
    lede: "当前页面不是安全上下文（https 或 loopback），浏览器不提供配对所需的加密能力。",
    warning: "请改用 https 打开本页后再配对；不能在这里降级成不安全的连接。",
  },
  request_failed: {
    title: "配对请求未能完成",
    lede:
      "没能把配对请求送到这台电脑（网络或本地存储不可用）。本页已经结束这次配对，可以安全地重新开始。",
    warning: "重新打开配对地址只会重新走一次流程，不会产生第二条配对记录。",
  },
  // 配对码入口的唯一失败文案（design D6）：不区分「不存在 / 过期 / 已被认领 / 已被消耗 /
  // 来源不符 / 响应丢失」，也不透露这次配对是否存在。页面不提供页内重试。
  code_invalid: {
    title: "配对码不对或已过期，请重新生成配对码",
    lede: "本页没有获得任何访问能力，也没有留下可用的主机记录。",
    warning: "请在电脑上重新发起配对，生成新的配对码后再输入。",
  },
};

/** 类型不匹配的固定兜底说明：只在调用点没有给出本地构造的消息时使用。 */
const KIND_MISMATCH_LEDE = "地址里的配对内容与这个入口不匹配：请由电脑重新生成对应入口的配对地址。";

/**
 * 阶段 + 失败类别 → 固定呈现（原型 S0–S8）。
 *
 * 这是文案的**唯一**来源：组件不再按阶段另行分支。失败页不渲染 `error.message`，
 * 因此后端消息或载荷内容没有进入页面的通道。
 */
function presentationFor(
  phase: PairingPhase,
  kind: PairingKind,
  failure: PairingFailureKind | null,
  error: PublicError | null,
): PairingPresentation {
  switch (phase) {
    case "reading":
      return {
        kind: "progress",
        title: "配对中",
        lede: "正在读取配对地址里的配对信息…",
        note: null,
        warning: null,
        tone: "neutral",
        countdown: false,
      };
    case "awaiting_code":
      return {
        kind: "code_entry",
        title: "输入配对码",
        lede: "在电脑上运行配对命令，输入终端里显示的 8 位配对码。",
        // 固定静态提示：lookup 之前页面不知道真实 `expiresAt`，因此**不**显示数字倒计时。
        note: "配对码 5 分钟内有效。",
        warning: null,
        tone: "neutral",
        countdown: false,
      };
    case "looking_up":
      return {
        kind: "progress",
        title: "配对中",
        lede: "正在用配对码取回这台电脑的配对信息，请稍候。",
        note: null,
        warning: null,
        tone: "neutral",
        // lookup 成功之前没有服务端 `expiresAt`，因此倒计时尚未开始。
        countdown: false,
      };
    case "claiming":
      return {
        kind: "progress",
        title: "配对中",
        lede: "正在向这台电脑声明本设备，请稍候。",
        note: "正在连接这台电脑…",
        warning: null,
        tone: "neutral",
        countdown: true,
      };
    case "pending_confirmation":
      return {
        kind: "progress",
        title: "配对中",
        lede: "正在等待这台电脑确认这次配对请求。通过后会自动进入目录，不需要再操作本页。",
        note: "正在等待电脑确认…",
        warning: null,
        tone: "neutral",
        countdown: true,
      };
    case "approved":
      return {
        kind: "progress",
        title: "已通过审批，正在进入目录",
        lede: "配对已完成，正在建立同步连接并载入目录…",
        note: "正在连接这台电脑…",
        warning: null,
        tone: "neutral",
        countdown: false,
      };
    case "rejected":
      return {
        kind: "failure",
        title: "审批未通过",
        lede: "电脑端没有批准这次配对请求。本页没有获得任何访问能力，也没有留下可用的主机记录。",
        note: null,
        warning: "如需继续，请在电脑上重新发起配对，再用新的地址打开本页。",
        tone: "danger",
        countdown: false,
      };
    case "expired":
      return {
        kind: "failure",
        title: "配对地址已过期",
        lede: "这次配对地址已经超过有效期（5 分钟），或者被电脑端作废。",
        note: null,
        warning: "请在电脑上重新发起配对，再用新的地址打开本页。",
        tone: "warning",
        countdown: false,
      };
    case "consumed":
      return {
        kind: "failure",
        title: "这个配对地址已经被使用",
        lede: "每个配对地址只能成功使用一次。再次打开不会建立第二条连接。",
        note: null,
        warning: "请在电脑上重新生成一个配对地址。",
        tone: "warning",
        countdown: false,
      };
    case "node_link_scanned":
      return {
        kind: "info",
        title: "已读到 node-link 配对内容",
        lede: "本页只呈现对端信息。node-link 由节点之间配对，浏览器不是它的参与方。",
        note: "请到节点的界面上继续完成配对。",
        warning: null,
        tone: "neutral",
        countdown: false,
      };
    // `idle` 只与「没有配对内容」一起出现；`failed` 的类别决定其余失败文案。
    case "idle":
    case "failed": {
      const kindFailure = phase === "idle" ? "no_payload" : (failure ?? "request_failed");
      if (kindFailure === "no_payload") {
        const copy = NO_PAYLOAD_COPY[kind];
        return {
          kind: "failure",
          title: copy.title,
          lede: copy.lede,
          note: null,
          warning: "请用电脑重新生成的配对地址打开本页。",
          tone: "warning",
          countdown: false,
        };
      }
      if (kindFailure === "kind_mismatch") {
        return {
          kind: "failure",
          title: "这不是本入口的配对地址",
          // 消息由 `kindMismatchFailure` 本地构造，只含两个类别名，不含任何载荷内容。
          lede: error?.message ?? KIND_MISMATCH_LEDE,
          note: null,
          warning: "请由电脑重新生成对应入口的配对地址后再打开本页。",
          tone: "warning",
          countdown: false,
        };
      }
      const copy = FAILURE_COPY[kindFailure];
      return {
        kind: "failure",
        title: copy.title,
        lede: copy.lede,
        note: null,
        warning: copy.warning,
        tone: "warning",
        countdown: false,
      };
    }
  }
}

/** 配对页初始状态：还没读地址片段（`reading`），先呈现与「配对中」一致的进度。 */
export function initialPairingPageModel(kind: PairingKind = "sync"): PairingPageModel {
  return modelFor("reading", { kind });
}

/** 阶段 → 模型（唯一的组装点，避免各分支各写一份文案）。 */
function modelFor(phase: PairingPhase, overrides: Partial<PairingPageModel>): PairingPageModel {
  const kind = overrides.kind ?? "sync";
  const failure = overrides.failure ?? null;
  const error = overrides.error ?? null;
  return {
    phase,
    kind,
    // 缺省是地址片段入口：`awaiting_code` / `applyClaiming` 会写明各自的入口。
    entry: overrides.entry ?? "qr",
    host: overrides.host ?? null,
    deviceId: overrides.deviceId ?? null,
    expiresAtMs: overrides.expiresAtMs ?? null,
    origin: overrides.origin ?? null,
    secureContext: overrides.secureContext ?? false,
    failure,
    error,
    presentation: presentationFor(phase, kind, failure, error),
  };
}

/**
 * 读到 sync 载荷、claim 即将发出：一次写入对端信息与截止时间。
 *
 * 截止时间取载荷里的绝对 `expiresAt`（不是「现在 + 5 分钟」）：页面可以在二维码生成一段时间后
 * 才被打开，倒计时必须反映真实剩余时间（design D3）。`entry` 由调用方按载荷来源给出（fragment
 * 或配对码 lookup），过期呈现据此分流（S7 / S6），不靠阶段历史去猜。
 */
export function applyClaiming(
  current: PairingPageModel,
  qr: PairingQrPayload,
  entry: PairingEntry,
): PairingPageModel {
  return modelFor("claiming", {
    ...current,
    entry,
    host: { address: qr.canonicalOrigin, hostId: qr.hostId },
    expiresAtMs: Date.parse(qr.expiresAt),
    failure: null,
    error: null,
  });
}

/**
 * `/pair` 没有地址片段：进入配对码输入（原型 S0）。
 *
 * 只呈现**公开**事实（canonical origin 与安全上下文）；配对码不在模型里。`expiresAtMs`
 * 保持 `null`：页面在 lookup 之前没有服务端有效期，因此不显示数字倒计时（design D6）。
 */
export function applyAwaitingCode(
  current: PairingPageModel,
  kind: PairingKind,
  origin: string,
  secureContext: boolean,
): PairingPageModel {
  return modelFor("awaiting_code", {
    ...current,
    kind,
    entry: "code",
    host: null,
    deviceId: null,
    expiresAtMs: null,
    origin,
    secureContext,
    failure: null,
    error: null,
  });
}

/** 配对码已提交：同源 lookup 在途（原型 S1）；仍没有服务端 `expiresAt`。 */
export function applyLookingUp(current: PairingPageModel): PairingPageModel {
  return modelFor("looking_up", {
    ...current,
    expiresAtMs: null,
    failure: null,
    error: null,
  });
}

/**
 * 配对码格式（design D6）：大小写不敏感、`I`/`L → 1`、`O → 0`，接受 `XXXX-XXXX`
 * 或连续 8 位；其余一律判为不合法，调用方据此**在发请求前**失败。
 *
 * 返回带单个 `-` 的 canonical 形式（schema 的 `lookupRequest.pairingCode` 允许它），
 * 不合法返回 `null`。Crockford 字母表不含 `I`/`L`/`O`/`U`：映射后仍出现 `U` 即拒绝。
 */
export function normalizePairingCode(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
  let compact = trimmed.toUpperCase();
  if (compact.length === 9 && compact.charAt(4) === "-") {
    compact = `${compact.slice(0, 4)}${compact.slice(5)}`;
  } else if (compact.includes("-")) {
    return null;
  }
  if (compact.length !== 8) return null;
  const normalized = compact.replaceAll("I", "1").replaceAll("L", "1").replaceAll("O", "0");
  for (const character of normalized) {
    if (!PAIRING_CODE_ALPHABET.includes(character)) return null;
  }
  return `${normalized.slice(0, 4)}-${normalized.slice(4)}`;
}

/**
 * 配对码入口的固定失败（design D6）：lookup 的任何失败、格式不符、lookup 后的已过期，
 * 都落到这**一个**错误与固定文案，不区分后端原因，也不回显 `message`。
 */
export function codeInvalidFailure(): PublicError {
  return {
    code: "pairing.code_invalid",
    message: "配对码不对或已过期，请重新生成配对码。",
    retryable: false,
    details: {},
  };
}

/**
 * 地址里没有配对内容：`/node-link/pair` 无片段时仍是这一呈现（它不提供配对码输入）。
 * sync 入口无片段改走 {@link applyAwaitingCode}；本函数仍保留给 node-link 与直接调用方。
 * 明确呈现「没有配对信息」并指引用户用电脑重新生成的地址打开，不发任何请求，也不保留
 * 上一轮的对端信息。
 */
export function applyNoPayload(_current: PairingPageModel, kind: PairingKind): PairingPageModel {
  return modelFor("idle", {
    kind,
    entry: "qr",
    host: null,
    deviceId: null,
    expiresAtMs: null,
    origin: null,
    secureContext: false,
    failure: "no_payload",
    error: null,
  });
}


/**
 * node-link 载荷 → 入口呈现（design D2/D3）。
 *
 * 本客户端没有 node-link 配对面：§13.2/§13.3 的 claim 与 status 由节点发起，浏览器不是它的参与方。
 * 因此这里只把载荷里**公开**的对端信息读给入口（端点 host 派生的 https 来源 + owner 节点标识），
 * 停在入口——不代替任何一方发请求，也不把含 `pairingSecret` 的载荷留在仓库里（它用到即弃）。
 *
 * 与 sync 载荷（`canonicalOrigin` / `hostId`，紧接着进入认领流程）刻意分开：
 * 这里是另一类载荷的另一套呈现口径，且不显示倒计时（`expiresAtMs` 为 `null`）。
 *
 * 端点派生不出 https 来源时（见 {@link nodeLinkAddressOf}）落到 `failed`，而不是抛异常：
 * 异常会从调用方 `#commit(...)` 的求值中途穿出，页面停在上一状态。
 */
export function applyScannedNodeLink(
  current: PairingPageModel,
  qr: NodeLinkQrPayload,
): PairingPageModel {
  // §13.1：node-link 的 HTTP origin 由 `endpoint` 的 host 加 `https` 得到（claim/status 的目标来源）。
  const address = nodeLinkAddressOf(qr.endpoint);
  if (address === null) {
    // 明确失败：码取 `protocol.schema_invalid`（与结构校验同族——这份载荷对本客户端不可用），
    // `details` 留空（同 `publicErrorForQrInput`，不外溢载荷内容）。
    return applyPairingFailure(current, {
      code: "protocol.schema_invalid",
      message: "node-link 配对载荷的端点无法解析出地址：请在节点上重新生成二维码后重试。",
      retryable: false,
      details: {},
    });
  }
  return modelFor("node_link_scanned", {
    ...current,
    host: { address, hostId: qr.ownerNodeId },
    deviceId: null,
    // 浏览器不参与 node-link 配对，也没有需要等待的 claim：不显示倒计时。
    expiresAtMs: null,
    failure: null,
    error: null,
  });
}

/**
 * §13.1 的 `endpoint`（`wss://<host>/node-link/v1`）→ 它派生的 https 来源；派生不出来时返回 `null`。
 *
 * 组合根按 `schemas/node-link/v1/pairing.schema.json` 的 `$defs.endpoint`
 * （`^wss://[^/?#]+/node-link/v1$`）校验端点形状。那条正则比 `new URL` **宽松**：端口越界
 * （`wss://h:999999/node-link/v1`）与主机里的非法百分号转义（`wss://%zz/node-link/v1`）都能过正则，
 * 却会让 `new URL` 抛出。异常若在这里冒泡，就会从调用方 `client-store` 的 `#commit(...)` 求值中途
 * 穿出——页面停在上一状态（`idle`，既没有失败态、也没有可执行的下一步），与「非法输入落到可断言的
 * 明确失败」相矛盾。
 *
 * 因此这里**防御式**解析，把抛出的端点收敛成 `null`，由调用方落成明确失败态。
 */
function nodeLinkAddressOf(endpoint: string): string | null {
  try {
    return `https://${new URL(endpoint).host}`;
  } catch {
    return null;
  }
}

/** 两类载荷/入口在文案里的名字（只用于说清「哪一类」，不回显任何载荷内容）。 */
const KIND_LABELS: Readonly<Record<PairingKind, string>> = {
  sync: "设备（sync）",
  "node-link": "node-link",
};

/**
 * 载荷类型与入口声明的类型不一致（design D2）：明确失败，且**在读载荷之后、发任何请求之前**。
 *
 * 文案只说清两类名字，不回显内容（它可能含一次性配对秘密）；`details` 一律留空（同
 * `publicErrorForQrInput`）。码取 `protocol.schema_invalid`：这份内容对本入口而言不是合法载荷。
 */
export function kindMismatchFailure(expected: PairingKind, actual: PairingKind): PublicError {
  return {
    code: "protocol.schema_invalid",
    message: `这段内容是 ${KIND_LABELS[actual]} 配对载荷，不是本入口的 ${KIND_LABELS[expected]} 配对载荷：请重新确认二维码。`,
    retryable: false,
    details: {},
  };
}

/**
 * 本页不在安全上下文（design D1/D3/D5）：明确失败，**不**回退到可导出形式的密钥、也不静默降级。
 *
 * 码取新增的 `auth.insecure_context`（封闭词表按 §6/§9 的同步清单登记，见
 * `compatibility/errors/v1/errors.json`）。文案沿用词表里的建议动作，只说清环境不满足与下一步，
 * 不回显任何载荷内容；`details` 一律留空（同 {@link kindMismatchFailure}）。
 */
export function insecureContextFailure(): PublicError {
  return {
    code: "auth.insecure_context",
    message:
      PAIRING_ERROR_HINTS["auth.insecure_context"] ??
      "当前页面不在安全上下文（https 或 loopback）中：配对与设备密钥都要求安全上下文，请改用 https 打开本页后再配对。",
    retryable: false,
    details: {},
  };
}

/**
 * §3.2 的 canonical origin：小写 scheme、浏览器规范化 hostname 与显式有效 port，不含
 * path/query/fragment 与尾部 `/`，默认端口省略。
 *
 * 复用浏览器自己的 `URL.origin`（与 `location.origin`、身份绑定检查同一条口径），因此这里
 * **不**新造字符串比较；解析不出合法来源时返回 `null`，由调用方落成明确失败而不是抛异常。
 */
export function canonicalOriginOf(value: string): string | null {
  try {
    return new URL(value).origin;
  } catch {
    return null;
  }
}

/**
 * 二维码声明的来源与本页实际来源不符（design D1/D3/D4）：**复用**既有的 `auth.origin_mismatch`。
 *
 * 入口的前置校验与事后的身份绑定（{@link publicErrorForTransportFailure}）落到同一条码与文案上；
 * 文案来自封闭词表的建议动作，不回显任何载荷内容，`details` 留空。
 */
export function originMismatchFailure(): PublicError {
  return {
    code: "auth.origin_mismatch",
    message: PAIRING_ERROR_HINTS["auth.origin_mismatch"] ?? "这个二维码来自另一个地址：请确认扫码对象。",
    retryable: false,
    details: {},
  };
}

/** 错误码 → 本侧失败类别（显式 reason 缺省时的分类，design D6）。 */
function failureKindFor(code: ErrorCode): PairingFailureKind {
  if (code === "pairing.code_invalid") return "code_invalid";
  if (code === "auth.origin_mismatch" || code === "auth.host_mismatch") return "origin_mismatch";
  if (code === "auth.insecure_context") return "insecure_context";
  if (code.startsWith("protocol.")) return "unrecognized";
  return "request_failed";
}

/**
 * 明确失败：结构校验不过、claim/poll 被拒、已配对记录写不进去，都落到失败阶段并带上原因。
 *
 * 先按错误码决定阶段：`pairing.expired` → `expired`（配对码入口除外，见下）；`pairing.consumed` /
 * `pairing.already_claimed` → `consumed`；其余 → `failed`。失败类别优先取调用点显式
 * 传入的 `reason`（类型不匹配、环境前置条件都在调用点就知道是哪一类），否则按错误码分类。
 * 这样 `kind_mismatch` 不会被混成 `unrecognized`（design D6）。
 *
 * **过期按入口分流**：配对码入口的会话（`entry === "code"`）没有可重新打开的地址，
 * 本地倒计时归零与服务端 `pairing.expired` 都落到「配对码不对或已过期」的固定呈现（S6）；
 * 二维码入口保持「配对地址已过期」（S7）。两条入口因此共用同一个到期与轮询收口，
 * 不需要各自复制一份。
 */
export function applyPairingFailure(
  current: PairingPageModel,
  error: PublicError,
  reason?: PairingFailureKind,
): PairingPageModel {
  if (error.code === "pairing.expired") {
    if (current.entry === "code") {
      return modelFor("failed", {
        ...current,
        failure: "code_invalid",
        error: codeInvalidFailure(),
      });
    }
    return modelFor("expired", { ...current, failure: null, error });
  }
  if (error.code === "pairing.consumed" || error.code === "pairing.already_claimed") {
    return modelFor("consumed", { ...current, failure: null, error });
  }
  return modelFor("failed", {
    ...current,
    failure: reason ?? failureKindFor(error.code),
    error,
  });
}

/**
 * **收口**失败：已配对记录已落盘、连接机已推进到「已配对」，但组合根没能把连接带起来。
 *
 * 与 {@link applyPairingFailure} 刻意分开：这里**不**把阶段退回 `failed`——配对本身已经成功，
 * 已配对记录也已经写好，声称「配对未完成」会与主机面板已经显示的主机身份自相矛盾；
 * 失败的只是「把同步连接装配起来」这一步。因此只补上可读原因，阶段与呈现都保持不变
 * （仍是 `approved` 的「正在进入目录」进度）。
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
    return originMismatchFailure();
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

/** 轮询结果 → 阶段。终态与错误都走各自的固定呈现。 */
export function applyPollResult(current: PairingPageModel, result: PairingPollResult): PairingPageModel {
  switch (result.kind) {
    case "pending_confirmation":
      return modelFor("pending_confirmation", { ...current, failure: null, error: null });
    case "approved":
      return modelFor("approved", {
        ...current,
        deviceId: result.deviceId,
        failure: null,
        error: null,
      });
    case "terminal":
      // 服务端宣布的过期与本地倒计时归零同一口径：交回 `applyPairingFailure` 按入口分流
      // （配对码入口 → S6；二维码入口 → S7）。其余终态各有自己的固定呈现。
      //
      // 过期的**事实**是 `status`，不是 `error.code`：§7.3 规定 `expired` 也是 HTTP 200，
      // 组合根按 HTTP 状态推导错误码时只能合成 `pairing.failed`。若照码分类，配对码入口会
      // 落到「配对请求未能完成」（S12）而不是 S6，二维码入口也会从 S7 退回 S12。因此在
      // 交给入口分流的错误里把码改写为 `pairing.expired`，只改写分类字段，保留原因文本。
      if (result.status === "expired") {
        return applyPairingFailure(current, { ...result.error, code: "pairing.expired" });
      }
      return modelFor(result.status, { ...current, failure: null, error: result.error });
    case "error":
      // 与 claim 相同的分类：`pairing.expired`/`consumed` 走终态，其余按错误码归到本侧失败。
      return applyPairingFailure(current, result.error);
  }
}

/** 传输层错误码 → 建议动作的文案（错误码封闭词表，不自造分类）。 */
export const PAIRING_ERROR_HINTS: Readonly<Partial<Record<ErrorCode, string>>> = {
  "pairing.expired": "二维码已过期：请在电脑上重新生成。",
  "pairing.consumed": "二维码已被使用：请在电脑上重新生成。",
  "pairing.already_claimed": "这台电脑已经登记过本设备，可直接尝试连接。",
  "pairing.code_invalid": "配对码不对或已过期，请重新生成配对码。",
  "auth.origin_mismatch": "这个二维码来自另一个地址：身份不得跨来源迁移，请确认扫码对象。",
  "auth.insecure_context":
    "当前页面不在安全上下文（https 或 loopback）中：配对与设备密钥都要求安全上下文，请改用 https 打开本页后再配对。",
  "protocol.invalid_json": "这段内容不是二维码里的配对载荷：请重新复制完整的二维码内容。",
  "protocol.schema_invalid": "这段内容不是本版本的配对载荷：请在电脑上重新生成二维码后重试。",
};
