/**
 * 会话创建（R16）：载荷形状、弹层模型与目录浏览模型。
 *
 * ## 载荷为什么可以是「精确列出」而不是「至少列出」
 *
 * `schemas/sync/v1/command.schema.json#/$defs/sessionCreate` 的 `payload` 只允许 `agentId`、
 * **一个**目录引用（`directoryId`（已记录目录的标识）或 `workspacePath`（绝对目录），二者恰好一个）
 * 与可选的 `createIfMissing`（显式的「目录不存在则先创建」开关）三类字段。
 * 多写 `cwd`/`exportId`/`templateParams`/MCP 配置/凭据即被服务端按 `protocol.schema_invalid` 拒绝；
 * `createIfMissing` 与 `directoryId` 同时出现同样被拒（已记录的目录必然已存在）。
 * 因此本模块的 `CreateSessionRefs` 是精确形状，并配一条**运行期**断言 `assertCreateSessionRefs`：
 * 类型系统保护编译期，断言保护「从别处拼出来的对象」这一路（例如后续有人从 JSON 里读回 payload 再转发）。
 *
 * ## 目录不由用户手工输入，而是「选定」出来的
 *
 * 浏览器拿不到宿主路径，客户端也不许拼接、规范化或反查它。因此浏览/选择出来的目录**一律**逐字来自
 * 服务端：`workspace.browse` 的结果条目（名称 + 绝对路径 + 是否已登记）与快照 `workspaces` 条目的
 * 目录标识与逐字路径。唯一由客户端算出来的路径是「在当前层新建一层目录」的候选路径——它由**服务端回显的
 * 当前层**加**用户输入的那一段名字**构成（见 {@link buildCreateFolderPath}），既不做规范化，也不
 * 去反查父层是否存在；那一段名字不合规时直接明确拒绝，不构造、不提交。
 *
 * ## 四类分支提示
 *
 * 原型的 `dirBranchNotice` 有四个分支（形状非法 / 主机不可用 / 目录还不存在 / 目录已存在但未登记）。
 * 本客户端能在**提交前**知道的只有三类（不存在的目录：会先新建；存在但未登记：会话归属不到任何目录项；
 * 已登记），加上「还没选定」。
 *
 * 「归属不到任何目录项」是本客户端真实呈现的后果：本客户端**没有**「未分组」这个分组
 * （`src/domain/directory.ts`：未指定目录的会话不计入任何目录项，也不凭空造一个），原型里那个
 * `unsorted` 目录条目不属本变更，因此提示文案 MUST NOT 承诺它存在。
 *
 * 形状非法与「主机不可用」两类都无法在提交前得知——前者只可能来自客户端构造候选路径时的名字校验
 * （{@link CREATE_FOLDER_NAME_RULE}），后者只有服务端给出 `internal.unavailable` 之后才谈得上
 * （见 {@link CREATE_FAILURE_NOTES}）。MUST NOT 靠猜把这两支提前画出来。
 *
 * ## 结果不确定为什么不会自动消解
 *
 * `uncertain` 终态由服务端确认（`src/state/command-machine.ts` 的
 * `serverConfirmedUncertain`）。本模型把它建模成**没有时间维度**的分支：
 * 既不存时间戳也不存「稍后重试」的定时器，因此不可能随时间或重连自动清除；
 * 只有用户主动处理（`dismiss`）才会离开该分支。
 */

import type { CommandMessage, ErrorCode, PublicError, Uuid } from "../protocol";
import type { CommandRecord, CommandState } from "../state";
import { isTerminalState, mayDisplayAsAccepted, shouldPresentUncertain } from "../state";
import type { AgentCatalogEntryView } from "../domain";

/** 提交载荷的两个引用：目录引用（目录标识或绝对目录）+ Agent 标识，另可带「不存在则先创建」。**只有这三项**。 */
export type CreateSessionRefs =
  | { readonly directoryId: string; readonly agentId: string; readonly createIfMissing?: never }
  | { readonly workspacePath: string; readonly agentId: string; readonly createIfMissing?: boolean };

/** 载荷越界：出现了相对路径、目录追加、MCP 配置或凭据字段。 */
export class CreateSessionPayloadViolation extends Error {
  constructor(detail: string) {
    super(`session.create 载荷越界：${detail}`);
    this.name = "CreateSessionPayloadViolation";
  }
}

/** 载荷允许出现的键；`agentId` 之外只允许一个目录引用键，另加可选的建目录开关。 */
const ALLOWED_PAYLOAD_KEYS: readonly string[] = ["directoryId", "workspacePath", "agentId", "createIfMissing"];
const DIRECTORY_REF_KEYS: readonly string[] = ["directoryId", "workspacePath"];

/**
 * 运行期校验：载荷**恰好**是 `agentId` 加一个目录引用（可带 `createIfMissing`）。
 *
 * 存在的理由不是「类型已经保证了」——它保证的是**本模块内部**写出来的对象；
 * 断言拦的是「对象来自别处」（从 JSON 读回、从缓存恢复、从 UI 事件里解出来的字段）。
 *
 * 目录引用必须**恰好一个**：同时给出目录标识与路径、或两者都不给，都在这里当场失败——
 * 与 `sync-session-create` 的「目录引用二选一」同一条口径（不猜以哪一个为准）。
 * `createIfMissing` 只在**路径**那一支成立，且缺席即假：拿目录标识加这个开关一律拒绝，
 * 不猜测调用方想建哪个目录。
 */
export function assertCreateSessionRefs(payload: unknown): CreateSessionRefs {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) {
    throw new CreateSessionPayloadViolation("载荷不是对象");
  }
  const record = payload as Record<string, unknown>;
  const extra = Object.keys(record).filter((key) => !ALLOWED_PAYLOAD_KEYS.includes(key));
  if (extra.length > 0) {
    throw new CreateSessionPayloadViolation(`出现越界字段 ${extra.join("、")}`);
  }
  const present = DIRECTORY_REF_KEYS.filter((key) => record[key] !== undefined);
  if (present.length !== 1) {
    throw new CreateSessionPayloadViolation("目录引用必须恰好给出一个（directoryId 或 workspacePath）");
  }
  const agentId = record["agentId"];
  if (typeof agentId !== "string" || agentId.length === 0) {
    throw new CreateSessionPayloadViolation("agentId 缺失或为空");
  }
  const createIfMissing = record["createIfMissing"];
  if (createIfMissing !== undefined && typeof createIfMissing !== "boolean") {
    throw new CreateSessionPayloadViolation("createIfMissing 不是布尔");
  }
  const key = present[0];
  const value = key === undefined ? undefined : record[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new CreateSessionPayloadViolation(`${String(key)} 缺失或为空`);
  }
  if (key === "directoryId") {
    if (createIfMissing !== undefined) {
      throw new CreateSessionPayloadViolation("createIfMissing 与 directoryId 互斥（已记录的目录必然已存在）");
    }
    return { directoryId: value, agentId };
  }
  // 缺席即假：只有显式为真才把键写进载荷，因此「不建目录」那一支的键集与引入本键之前逐字相同。
  return createIfMissing === true
    ? { workspacePath: value, agentId, createIfMissing: true }
    : { workspacePath: value, agentId };
}

/**
 * 构造 `session.create` 命令。
 *
 * `payload` 是**就地构造**的对象，不接受调用方传入的任意 payload：
 * 这样「多带一个字段」在本模块内没有表达方式。
 */
export function buildCreateSessionCommand(input: {
  readonly refs: CreateSessionRefs;
  readonly requestId: Uuid;
  readonly messageId: Uuid;
  readonly connectionId: Uuid;
  /**
   * 出站连接序号。
   *
   * `$defs.command` 的信封把它列为必填，因此这里缺省给占位 `"0"`；真正的客户端方向序号由
   * `SyncConnection.sendCommand` 覆盖（§4.1），调用方**不需要**自己维护计数器。
   */
  readonly connectionSequence?: string;
}): CommandMessage {
  const payload = assertCreateSessionRefs(input.refs);
  return {
    protocolVersion: 1,
    type: "command",
    messageId: input.messageId,
    connectionId: input.connectionId,
    connectionSequence: input.connectionSequence ?? "0",
    body: {
      requestId: input.requestId,
      command: "session.create",
      payload,
    },
  };
}

// ── 目录浏览（`workspace.browse`）────────────────────────────────────────────

/**
 * 目录浏览所需的 feature（`compatibility/features/v1/features.json` 的 sync 段）。
 *
 * 未协商时服务端会在触碰文件系统**之前**回 `protocol.feature_required`，因此客户端必须在发命令
 * 之前就判定它（与 `core.session-history.v1` 的 include 门控同一手法）：发出去才看反应会白跑一趟，
 * 也会给用户一个必然失败的控件。
 */
export const WORKSPACE_BROWSE_FEATURE = "core.workspace-browse.v1";

/**
 * 顶部入口「不能逐层浏览」的两种原因——两件**不同**的事实，MUST NOT 混为一谈。
 *
 * - `offline`：还没连上主机（连接尚未走到 `online`）。此时本机**不知道**服务端支持什么：
 *   未认证 / 还没拿到协商结果时 `selectedFeatures` 本来就是空的，凭它说「服务端不支持浏览」
 *   是在断言一件本机无从知道的事；
 * - `unsupported`：连接已到 `online`（认证与追平都完成了）却没选中 {@link WORKSPACE_BROWSE_FEATURE}，
 *   这一支才是「服务端没有提供该能力」。
 */
export type BrowseUnavailableReason = "offline" | "unsupported";

/**
 * 顶部入口的创建弹层没有已记录目录，它自己的状态键。
 *
 * 用法只有两处：`data-create-sheet` 的取值，以及 `createRequestIds` 的键（提交状态要挂在一个稳定的
 * 键上，且不能与任何目录标识相撞）。目录标识是 32 位十六进制，因此含 `@` 的字面量不会与它相撞。
 */
export const CREATE_SHEET_BROWSE_KEY = "@browse";

/** 已记录目录：`directoryId` 与逐字路径来自快照 `workspaces`（不支持浏览时的回退选择）。 */
export interface RegisteredDirectoryModel {
  /** 目录标识：只作机器标识与路由/命令引用，MUST NOT 呈现为界面文本。 */
  readonly directoryId: string;
  /** 由服务端下发的路径派生的展示名。 */
  readonly displayName: string;
  /** 服务端下发的逐字路径。 */
  readonly path: string;
}

/** 浏览到的一层里的一个子目录：名称与路径都逐字来自服务端下发的条目。 */
export interface BrowseEntryModel {
  readonly name: string;
  readonly path: string;
  /** 该子目录是否已是本机登记的 workspace（服务端下发的登记状态）。 */
  readonly registered: boolean;
}

/** 面包屑的一段：根段是「此电脑」（路径为 `null`），其余各段的路径逐字来自服务端。 */
export interface DirectoryCrumbModel {
  /** 段名：根段固定为「此电脑」，其余取自该层条目的 `name`（或用户自己新建的那一段名字）。 */
  readonly name: string;
  readonly path: string | null;
  readonly registered: boolean;
}

/**
 * 选定结果：两条来源各自对应一种目录引用。
 *
 * - `registered`：从已记录目录集合里选中的一项 → 载荷用 `directoryId`；
 * - `path`：浏览选定的绝对路径（或「在当前层新建」构造出的候选路径）→ 载荷用 `workspacePath`。
 *
 * 两者的路径/名字都逐字来自服务端下发（`path` 那一支的 `name` 在新建时来自用户输入）。
 */
export type DirectorySelection =
  | {
      readonly kind: "registered";
      readonly directoryId: string;
      readonly displayName: string;
      readonly path: string;
    }
  | {
      readonly kind: "path";
      readonly path: string;
      readonly name: string;
      /** 该目录已是本机记录的目录（只影响提示口径，不改变目录引用用哪一支）。 */
      readonly registered: boolean;
      /** 该目录在主机上还不存在，提交时要先新建它（载荷的 `createIfMissing`）。 */
      readonly willCreate: boolean;
    };

/** 创建弹层的目录块：已知目录（目录/路径两行）或顶部入口（选择文件夹控件 + 已选路径 + 说明）。 */
export type CreateDirectoryBlock =
  | {
      readonly kind: "known";
      /** 由路径派生的展示名（目录标识只作机器标识，模型里不带它——它不会成为界面文本）。 */
      readonly displayName: string;
      /** 服务端下发的逐字路径。 */
      readonly path: string;
    }
  | {
      readonly kind: "browse";
      /**
       * 能不能逐层浏览；不能的话是哪一种原因（见 {@link BrowseUnavailableReason}）。
       *
       * `null` = 协商到了该能力，入口正常提供浏览；非 `null` 时入口说明不可用并保留已登记目录这条路。
       */
      readonly unavailable: BrowseUnavailableReason | null;
      readonly selection: DirectorySelection | null;
      /** 重新打开选择器时的初始层：当前选定层的服务端路径；没有可用路径时为 `null`（根列表）。 */
      readonly initialPath: string | null;
      readonly notice: CreateDirectoryNotice;
    };

/** 目录块旁的分支提示（见文件头的「四类分支提示」）。 */
export type CreateDirectoryNotice =
  | { readonly kind: "unselected" }
  | { readonly kind: "will_create"; readonly path: string }
  | { readonly kind: "unregistered"; readonly path: string }
  | { readonly kind: "registered"; readonly displayName: string; readonly path: string };

/** 分支提示的文案；四类各一条，由模型给出（组件不另写一套判断）。 */
export function createDirectoryNoticeText(notice: CreateDirectoryNotice): string {
  switch (notice.kind) {
    case "unselected":
      return "还没有选定目录：点「选择文件夹」浏览主机上的目录并选中一层；服务端不支持浏览时从已登记的目录里选一个。";
    case "will_create":
      return `${notice.path} 在主机上还不存在。创建会话时会先在主机上建这个目录，再在它下面启动 Agent；新建出来的目录不登记到目录列表，因此这个会话不会出现在任何目录项下。`;
    case "unregistered":
      return `将使用主机上的 ${notice.path}。它不在已登记目录里，因此这个会话不会出现在任何目录项下。`;
    case "registered":
      return "这个目录已是本机登记的目录（不会重复登记）。";
  }
}

/** 目录块的说明（顶部入口形态下始终呈现）。 */
export const CREATE_DIRECTORY_HINT =
  "点「选择文件夹」浏览主机上的目录；也可以先逐层新建目录，最后单独选择。取消会话不会删除已建目录。";

/** 「新建文件夹」的名字规则；违反即**参数类**拒绝（不构造候选路径、不提交）。 */
export const CREATE_FOLDER_NAME_RULE = "文件夹名字不能为空，也不能含 \\、/ 或 :。";

/** 服务端不支持浏览时入口的说明（顶部入口形态）。 */
export const BROWSE_UNSUPPORTED_NOTICE =
  "服务端没有提供目录浏览能力（未协商 core.workspace-browse.v1），不能逐层浏览主机上的目录；你仍然可以从已登记的目录里选一个。";

/**
 * 还没连上主机时入口的说明（顶部入口形态）。
 *
 * 与 {@link BROWSE_UNSUPPORTED_NOTICE} 是**两条**文案：未连上时本机不知道服务端支持什么
 * （`selectedFeatures` 为空只是「还没协商」），把它写成「服务端没有提供该能力」是在断言一件
 * 无从知道的事，建议「升级服务端」更是答非所问。
 */
export const BROWSE_OFFLINE_NOTICE =
  "还没有连上主机（也还没有拿到主机下发的目录列表），因此暂时不能逐层浏览主机上的目录；连上之后可以逐层浏览并选定一层，现在也可以从已登记的目录里选一个。";

/** 不能逐层浏览时的说明文案：两种原因各一条，由模型给出（组件不另写一套判断）。 */
export const BROWSE_UNAVAILABLE_NOTICES: Readonly<Record<BrowseUnavailableReason, string>> = {
  offline: BROWSE_OFFLINE_NOTICE,
  unsupported: BROWSE_UNSUPPORTED_NOTICE,
};

/**
 * 回退列表（已登记目录）为空时的补充说明：同样按原因各一条。
 *
 * 「把服务端升级到支持目录浏览的版本」只在服务端**确实**没提供该能力时才成立；还没连上时那条路
 * 走不通（本机连服务端选中了什么都不知道），下一步是先连上（或先在本机登记一个目录）。
 */
export const BROWSE_EMPTY_FALLBACK_NOTICES: Readonly<Record<BrowseUnavailableReason, string>> = {
  offline: "本机还没有已登记的目录：先在主机上登记一个目录。",
  unsupported: "本机还没有已登记的目录：先在主机上登记一个目录，或把服务端升级到支持目录浏览的版本。",
};

/** 该层条目被单层上限截断时的说明：还有更多没列出来，MUST NOT 静默丢弃。 */
export const BROWSE_TRUNCATED_NOTICE = "这一层还有更多目录没有列出（服务端单层最多列出 128 个）。";

/** 结果形状不合法时的失败原因（服务端回了非浏览结果，或条目缺字段）。 */
export const BROWSE_RESULT_INVALID_ERROR: PublicError = {
  code: "internal.unavailable",
  message: "服务端返回的目录列表不符合协议形状，无法呈现。",
  retryable: false,
  details: {},
};

/**
 * 目录浏览的原始状态（住在这个模块里，由 `ClientStore` 持有；`buildDirectoryBrowserModel` 把它投影成视图模型）。
 *
 * 它**只**由服务端下发与本模块的纯函数写入：`path`/`entries`/`crumbs` 里没有一处是客户端拼出来的
 * 路径（「新建文件夹」的候选路径除外，见 {@link buildCreateFolderPath}，且它以 `willCreate` 标出来）。
 */
export interface DirectoryBrowseState {
  /** 选择器是否打开。 */
  readonly open: boolean;
  /** 是否已经收到过这一层的结果（`false` = 还没请求过，「空闲」与「这一层是空的」因此可区分）。 */
  readonly loaded: boolean;
  /** 当前层：服务端结果里**逐字回显**的路径；`null` = 该节点的根列表（此电脑）。 */
  readonly here: string | null;
  readonly crumbs: readonly DirectoryCrumbModel[];
  readonly entries: readonly BrowseEntryModel[];
  /** 该层条目被单层上限截断。 */
  readonly truncated: boolean;
  /** 在途浏览请求的 `requestId`；`null` 表示没有在途请求。 */
  readonly pendingRequestId: Uuid | null;
  /** 上一次浏览的结构化失败；参数类与服务端不可用类由 `code` 区分。 */
  readonly error: PublicError | null;
  /** 「新建文件夹」被拒的原因；`null` 表示没有被拒。 */
  readonly folderError: string | null;
  /** 已确认的选定结果；`null` 表示还没选定。 */
  readonly selection: DirectorySelection | null;
}

/** 根段的固定文案：它不是目录，因此没有路径，也不能被选为工作目录。 */
export const ROOT_CRUMB: DirectoryCrumbModel = { name: "此电脑", path: null, registered: false };

/** 初始（未打开）的浏览状态。 */
export function initialDirectoryBrowseState(): DirectoryBrowseState {
  return {
    open: false,
    loaded: false,
    here: null,
    crumbs: [ROOT_CRUMB],
    entries: [],
    truncated: false,
    pendingRequestId: null,
    error: null,
    folderError: null,
    selection: null,
  };
}

/**
 * 重开选择器保留当前位置、已读条目和命令状态，避免丢失尚待确认的创建结果。
 *
 * 保留选定是刻意的：用户按 Esc/取消再打开时要能从原处继续；选定本身的清除只发生在
 * 关掉整个创建弹层（或换一个目录入口）时。
 */
export function openedDirectoryBrowseState(state: DirectoryBrowseState): DirectoryBrowseState {
  return { ...state, open: true };
}

/** 目录浏览的视图模型（`DirectoryPicker` 只吃它）。 */
export interface FolderCreationModel {
  readonly requestId: Uuid;
  readonly parentPath: string;
  readonly name: string;
  readonly state: string;
  readonly error: string | null;
}
export interface DirectoryBrowserModel {
  readonly mkdirSupported?: boolean;
  readonly folder?: FolderCreationModel | null;
  /** 服务端是否协商了 `core.workspace-browse.v1`；为假时入口说明不可用并回退到已登记目录。 */
  readonly supported: boolean;
  readonly status: "unsupported" | "idle" | "loading" | "ready" | "failed";
  /** 当前层：服务端逐字回显的路径；`null` = 根列表。 */
  readonly here: string | null;
  readonly crumbs: readonly DirectoryCrumbModel[];
  /** 当前层在面包屑上的那一段（根列表为「此电脑」）：名字与登记状态取自服务端下发的条目。 */
  readonly current: DirectoryCrumbModel;
  /** 「上一级」的目标段；`null` 表示已经在根列表（它本身不是目录，没有上一级）。 */
  readonly parent: DirectoryCrumbModel | null;
  readonly entries: readonly BrowseEntryModel[];
  readonly truncated: boolean;
  readonly error: PublicError | null;
  readonly folderError: string | null;
  /** 当前层是否可选为工作目录：根列表不是目录，因此不可选；未加载/失败时同样不可选。 */
  readonly canConfirm: boolean;
}

/** UI 目录名按中文界面习惯排列，忽略大小写，数字按数值排序；不改变 wire 结果顺序。 */
const DIRECTORY_NAME_ORDER = new Intl.Collator("zh-CN", { sensitivity: "accent", numeric: true });

/** 把原始状态投影成选择器视图模型；`supported` 来自服务端协商的 feature。 */
export function buildDirectoryBrowserModel(input: {
  readonly supported: boolean;
  readonly browse: DirectoryBrowseState;
  readonly mkdirSupported?: boolean;
  readonly folder?: FolderCreationModel | null;
}): DirectoryBrowserModel {
  const { browse } = input;
  const status: DirectoryBrowserModel["status"] = !input.supported
    ? "unsupported"
    : browse.pendingRequestId !== null
      ? "loading"
      : browse.error !== null
        ? "failed"
        : browse.loaded
          ? "ready"
          : "idle";
  const crumbs = browse.crumbs.length === 0 ? [ROOT_CRUMB] : browse.crumbs;
  const current = crumbs[crumbs.length - 1] ?? ROOT_CRUMB;
  return {
    supported: input.supported,
    mkdirSupported: input.mkdirSupported ?? false,
    folder: input.folder ?? null,
    status,
    here: browse.here,
    crumbs,
    current,
    parent: crumbs.length > 1 ? (crumbs[crumbs.length - 2] ?? null) : null,
    entries: [...browse.entries].sort((left, right) =>
      DIRECTORY_NAME_ORDER.compare(left.name, right.name)
      || (left.path < right.path ? -1 : left.path > right.path ? 1 : 0),
    ),
    truncated: browse.truncated,
    error: browse.error,
    folderError: browse.folderError,
    canConfirm: status === "ready" && browse.here !== null,
  };
}

/**
 * 目标层对应的面包屑。
 *
 * 三种情况：目标是根列表 → 只剩根段；目标层已在面包屑上（点面包屑或「上一级」回去）→ 截到那一段；
 * 目标是刚进入的一层 → 名字与登记状态取自**服务端下发的条目**（其次取自当前选定的记录）。
 * 一条路径都不会被切开或重新拼装；最后那条兜底分支只对「没有任何来源记录的路径」生效，
 * 它以路径原文当段名，不猜、不裁、不补。
 */
export function crumbsFor(input: {
  readonly current: readonly DirectoryCrumbModel[];
  readonly entries: readonly BrowseEntryModel[];
  readonly selection: DirectorySelection | null;
  readonly path: string | null;
}): readonly DirectoryCrumbModel[] {
  if (input.path === null) return [ROOT_CRUMB];
  const path = input.path;
  const seen = input.current.findIndex((crumb) => crumb.path === path);
  if (seen >= 0) return input.current.slice(0, seen + 1);
  const entry = input.entries.find((item) => item.path === path);
  if (entry !== undefined) {
    return [...input.current, { name: entry.name, path: entry.path, registered: entry.registered }];
  }
  const selection = input.selection;
  if (selection !== null && selection.kind === "path" && selection.path === path) {
    return [...input.current, { name: selection.name, path: selection.path, registered: selection.registered }];
  }
  return [...input.current, { name: path, path, registered: false }];
}

/**
 * 「选择此文件夹」的选定结果：路径取当前层的**服务端回显**，名字与登记状态取面包屑上的那一段。
 *
 * 根列表不是目录，因此不可选（返回 `null`）。
 */
export function selectionForCurrentLevel(browser: DirectoryBrowserModel): DirectorySelection | null {
  if (!browser.canConfirm || browser.here === null) return null;
  return {
    kind: "path",
    path: browser.here,
    name: browser.current.name,
    registered: browser.current.registered,
    willCreate: false,
  };
}

/** 「新建文件夹」的候选路径：成功给出候选路径与规范化后的名字，失败给出原因（参数类）。 */
export type CreateFolderPathResult =
  | { readonly ok: true; readonly path: string; readonly name: string }
  | { readonly ok: false; readonly reason: string };

/**
 * 在当前层下构造「新建一层目录」的候选路径。
 *
 * 这是全仓**唯一**由客户端构造路径的地方，它的每一步都必须能指着来源说清楚：
 *
 * - 父层是服务端**逐字回显**的当前层（`DirectoryBrowseState.here`），不做任何规范化；
 * - 名字是用户输入的那一段，只 `trim` 一次（去首尾空白），不含分隔符或卷标冒号——含则当场拒绝；
 * - 中间只补一个分隔符：取父层自己出现过的那个（`\` 或 `/`），父层已以分隔符结尾时不重复补。
 *
 * 它 MUST NOT 解析 `..`、折叠重复分隔符、改大小写、也不去反查父层是否存在——那些都是服务端的事；
 * 候选路径只作为 `session.create` 的 `workspacePath` 送出去，由服务端重新规范化并以自己的结果为准。
 */
export function buildCreateFolderPath(input: {
  readonly parent: string;
  readonly name: string;
}): CreateFolderPathResult {
  const name = input.name.trim();
  if (name.length === 0 || name === "." || name === ".." || name.includes("\0") || [...name].length > 255) return { ok: false, reason: CREATE_FOLDER_NAME_RULE };
  if (name.includes("\\") || name.includes("/") || name.includes(":")) {
    return { ok: false, reason: CREATE_FOLDER_NAME_RULE };
  }
  const parent = input.parent;
  if (parent.length === 0) return { ok: false, reason: CREATE_FOLDER_NAME_RULE };
  if (parent.endsWith("\\") || parent.endsWith("/")) return { ok: true, path: `${parent}${name}`, name };
  const separator = parent.lastIndexOf("\\") > parent.lastIndexOf("/") ? "\\" : "/";
  return { ok: true, path: `${parent}${separator}${name}`, name };
}

/** 协议要求的最小形状判据：`workspace.browse` 的路径/条目名都是非空字符串。 */
function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

/** 解析后的浏览结果（条目已收窄成 {@link BrowseEntryModel}）。 */
export interface ParsedBrowseResult {
  readonly path: string | null;
  readonly entries: readonly BrowseEntryModel[];
  readonly truncated: boolean;
}

/**
 * 把 `workspace.browse` 的 `completed` 结果收窄成视图模型。
 *
 * 逐字段确认形状而不是就地断言：结果来自 wire，形状不符时调用方要能给出一条明确的失败提示，
 * MUST NOT 把 `undefined` 当路径渲染、也 MUST NOT 静默当成空层（那会让「服务端答非所问」看起来
 * 像「这一层没有子目录」）。`path` 是请求的**逐字回显**，因此这里不解析、不规范化它。
 */
export function parseBrowseResult(value: unknown): ParsedBrowseResult | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Readonly<Record<string, unknown>>;
  const path = record["path"];
  if (path !== null && !isNonEmptyString(path)) return null;
  const truncated = record["truncated"];
  if (typeof truncated !== "boolean") return null;
  const raw = record["entries"];
  if (!Array.isArray(raw)) return null;
  const entries: BrowseEntryModel[] = [];
  for (const item of raw) {
    if (typeof item !== "object" || item === null || Array.isArray(item)) return null;
    const entry = item as Readonly<Record<string, unknown>>;
    const name = entry["name"];
    const entryPath = entry["path"];
    const registered = entry["registered"];
    if (!isNonEmptyString(name) || !isNonEmptyString(entryPath) || typeof registered !== "boolean") return null;
    entries.push({ name, path: entryPath, registered });
  }
  return { path, entries, truncated };
}

// ── 提交状态 ────────────────────────────────────────────────────────────────

/** 创建入口旁**始终**呈现的副作用说明（spec「创建入口的副作用说明」）。 */
export const CREATE_EFFECT_NOTE = "将在你的电脑上启动一个 Agent 进程。";

/** 提交状态的失败分支可以分成两类（见文件头的「四类分支提示」）。 */
export type CreateFailureKind = "parameter" | "unavailable" | "other";

/** 失败原因的分类：参数类与服务端不可用类必须能分开呈现。 */
export function createFailureKind(error: PublicError): CreateFailureKind {
  if (error.code === "protocol.schema_invalid") return "parameter";
  if (error.code === "internal.unavailable") return "unavailable";
  return "other";
}

/** 两类失败的补充说明（`other` 没有额外说明——只呈现服务端给的 `message`）。 */
export const CREATE_FAILURE_NOTES: Readonly<Record<CreateFailureKind, string>> = {
  parameter:
    "服务端按参数类错误（protocol.schema_invalid）拒绝了这次提交：目录必须是绝对路径，且不能包含「..」。没有创建会话，也没有触碰主机上的文件系统。",
  unavailable:
    "服务端无法在这个目录创建会话：父层可能不存在，或没有写入权限。没有新建目录，也没有创建会话，也不会留下半成品。",
  other: "",
};

/** 提交状态。`uncertain` 没有时间维度，因此不会被自动消解。 */
export type CreateSubmissionModel =
  | { readonly kind: "idle" }
  | { readonly kind: "creating" }
  | { readonly kind: "accepted"; readonly acceptedAt: string | null }
  | { readonly kind: "failed"; readonly error: PublicError }
  | { readonly kind: "uncertain"; readonly requestId: Uuid }
  | { readonly kind: "created"; readonly sessionId: Uuid };

/** 提交按钮是否可点：只有 idle/失败/结果不确定（用户主动重试）时可点。 */
export function isSubmissionAvailable(submission: CreateSubmissionModel): boolean {
  if (submission.kind === "creating") return false;
  if (submission.kind === "accepted") return false;
  return true;
}

/**
 * 由命令记录投影提交状态。
 *
 * - `submitting`/`draft` → 创建中（呈现「创建中」且**不接受重复提交**）；
 * - `completed` → 已创建（带新会话标识，供页面跳转）；
 * - `failed`/`rejected` → 结构化失败原因（`PublicError` 的 code/message/retryable 逐项可区分）；
 * - `uncertain` → 结果不确定，**只有服务端确认过**才这么呈现（spec 场景「结果不确定不被自行断定」）。
 */
export function createSubmissionModel(
  record: CommandRecord | null,
  input: { readonly connectionOnline: boolean },
): CreateSubmissionModel {
  if (record === null) return { kind: "idle" };
  switch (record.state as CommandState) {
    case "draft":
    case "submitting":
      return { kind: "creating" };
    case "accepted":
      // 未拿到服务端确认的 `acceptedAt` 前不得显示为「已接受」。
      return mayDisplayAsAccepted(record, input.connectionOnline)
        ? { kind: "accepted", acceptedAt: record.acceptedAt }
        : { kind: "creating" };
    case "completed": {
      const sessionId = typeof record.result?.["sessionId"] === "string" ? record.result["sessionId"] : null;
      return sessionId === null
        ? { kind: "failed", error: missingSessionIdError() }
        : { kind: "created", sessionId };
    }
    case "failed":
    case "rejected":
      return { kind: "failed", error: record.error ?? genericFailureError(record.state) };
    case "uncertain":
      return shouldPresentUncertain(record)
        ? { kind: "uncertain", requestId: record.requestId }
        : { kind: "creating" };
  }
}

/** `completed` 却没带 `sessionId`：协议违规，按结构化失败呈现而不是假装成功。 */
function missingSessionIdError(): PublicError {
  return {
    code: "internal.unavailable",
    message: "创建会话已完成但未返回会话标识，无法进入新会话。",
    retryable: false,
    details: {},
  };
}

/** 没有 `error` 的失败终态：保留状态名，不编造原因。 */
function genericFailureError(state: CommandState): PublicError {
  return {
    code: "internal.unavailable" satisfies ErrorCode,
    message: `服务端未给出失败原因（${state}）。`,
    retryable: false,
    details: {},
  };
}

// ── 弹层模型 ────────────────────────────────────────────────────────────────

/** 创建弹层模型：目录块 + Agent 候选 + 提交状态。 */
export interface CreateSessionEntryModel {
  /** 弹层的机器标识：已知目录用别名，顶部入口用 {@link CREATE_SHEET_BROWSE_KEY}（`data-create-sheet`）。 */
  readonly sheetKey: string;
  readonly directory: CreateDirectoryBlock;
  /** 入口旁始终呈现的副作用说明（{@link CREATE_EFFECT_NOTE}）。 */
  readonly effectNote: string;
  /** Agent 候选：**只**来自本机已配置的 Agent 目录（快照 `agents` 资源）。 */
  readonly agents: readonly AgentCatalogEntryView[];
  readonly submission: CreateSubmissionModel;
  /** 终态判定：结果不确定时为真，页面据此保留显眼提示直到用户处理。 */
  readonly uncertainRequiresUserAction: boolean;
}

/** 构造创建弹层模型的输入（全部来自快照、浏览状态与命令记录）。 */
export interface CreateSessionEntryInput {
  /** 入口所属的已记录目录标识；顶部入口为 `null`（目录由浏览/选择给出）。 */
  readonly directoryId: string | null;
  /**
   * 该标识对应的展示名与逐字路径（快照 `workspaces` 里的那一项）；标识不在快照里时为 `null`。
   *
   * 为 `null` 时目录块退回**浏览/选择**形态：目录标识只是机器标识，拿它当展示名会把标识写成
   * 界面文本，编一段名字或路径更是规格明确禁止的。
   */
  readonly knownDirectory: {
    readonly displayName: string;
    readonly path: string;
  } | null;
  /** 服务端是否提供了目录浏览能力。 */
  readonly browsingSupported: boolean;
  /** 当前浏览选定（顶部入口用）；`null` 表示还没选定。 */
  readonly selection: DirectorySelection | null;
  readonly agents: readonly AgentCatalogEntryView[];
  readonly record: CommandRecord | null;
  /**
   * 连接是否已到 `online`。
   *
   * 两处用它：提交状态（未连接时不派发命令）与顶部入口的「不能逐层浏览」原因
   * （{@link BrowseUnavailableReason}：未 `online` 时本机还不知道服务端支持什么）。
   */
  readonly connectionOnline: boolean;
}

/** 目录块旁的分支提示（四类，见文件头）。 */
function noticeFor(selection: DirectorySelection | null): CreateDirectoryNotice {
  if (selection === null) return { kind: "unselected" };
  if (selection.kind === "registered") {
    return { kind: "registered", displayName: selection.displayName, path: selection.path };
  }
  if (selection.willCreate) return { kind: "will_create", path: selection.path };
  return selection.registered
    ? { kind: "registered", displayName: selection.name, path: selection.path }
    : { kind: "unregistered", path: selection.path };
}

/**
 * 重新打开选择器时的初始层。
 *
 * 只有**服务端下发过**的路径才配当初始层：它要么是浏览出来的条目路径，要么是回退列表里那一项。
 * 「在当前层新建」出来的候选路径在主机上还不存在，拿它去请求列举只会换来一次必然的失败，
 * 因此那一支回到根列表。
 */
function browseInitialPath(selection: DirectorySelection | null): string | null {
  if (selection === null || selection.kind !== "path" || selection.willCreate) return null;
  return selection.path;
}

/**
 * 构造创建弹层模型。
 *
 * 入口**没有**权限维度：删掉设备权限层后，创建入口恒可用（spec「入口不因权限不可用」），
 * 因此这里不接任何 scopes 入参。
 *
 * @param input.agents 本机已配置的 Agent 目录（快照 `agents`）。传入目录之外的 Agent 不会被补进来，
 *   因此「候选只来自已配置 Agent」在数据流上成立。
 */
export function buildCreateSessionEntryModel(input: CreateSessionEntryInput): CreateSessionEntryModel {
  const submission = createSubmissionModel(input.record, { connectionOnline: input.connectionOnline });
  // 不能逐层浏览的两种原因必须分开：还没连上时本机不知道服务端支持什么（`selectedFeatures` 为空只是
  // 「还没协商」），只有连接已到 `online` 却仍没协商到该能力，才是服务端没有提供它。
  const unavailable: BrowseUnavailableReason | null = input.browsingSupported
    ? null
    : input.connectionOnline
      ? "unsupported"
      : "offline";
  const directory: CreateDirectoryBlock =
    input.directoryId !== null && input.knownDirectory !== null
      ? {
          kind: "known",
          displayName: input.knownDirectory.displayName,
          path: input.knownDirectory.path,
        }
      : {
          kind: "browse",
          unavailable,
          selection: input.selection,
          initialPath: browseInitialPath(input.selection),
          notice: noticeFor(input.selection),
        };
  return {
    sheetKey: input.directoryId ?? CREATE_SHEET_BROWSE_KEY,
    directory,
    effectNote: CREATE_EFFECT_NOTE,
    agents: input.agents,
    submission,
    uncertainRequiresUserAction: submission.kind === "uncertain",
  };
}

/** 终态判定是否已到（页面据此离开「创建中」）。 */
export function isSubmissionTerminal(submission: CreateSubmissionModel): boolean {
  if (submission.kind === "creating" || submission.kind === "idle") return false;
  return isTerminalState(submission.kind === "created" ? "completed" : submission.kind);
}
