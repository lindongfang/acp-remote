/**
 * 浏览器会话准备草稿的**专用平台端口**（`web-session-preparation` D5/6.1–6.4）。
 *
 * 本文件只有类型、固定常量与**纯策略**，不触碰 IndexedDB / sessionStorage：
 * Web 实现住在 `preparation-draft.web.ts`，`preparation-draft.ts` 只是无平台变体的转出入口。
 * 共享声明必须住在这种模块里，原因同 [`./local-cache.shared`](./local-cache.shared.ts)：
 * Metro 在 Web 平台把 `./preparation-draft` 解析到 `.web.ts`，共享符号若留在那里，
 * 真实 Web 产物会取到 `undefined`。
 *
 * ## 为什么是独立端口，而不是扩大摘要缓存
 *
 * `LocalCachePort` 只接受本节点自有会话的**精简摘要**（`docs/FRONTEND_DESIGN.md` §7）。
 * 准备草稿包含用户尚未提交的文字与附件，属于**唯一**允许的客户端正文窄例外，
 * 因此必须有独立的数据库、独立的配额反馈与独立的删除语义；不得借用摘要缓存，
 * 也不得扩展到 imported/远程会话或普通正式会话草稿。
 *
 * ## 分区键
 *
 * `canonical origin + ownerNodeId + deviceId + tabSlotId + sessionId`。前四项都是
 * 不可跨来源/节点/设备/标签页复用的定位信息，`sessionId` 是 Owner 分配的正式身份。
 * 标签页之间即使准备同一个 `sessionId`（复制/opener 场景）也不会共享草稿。
 *
 * ## 敏感信息边界
 *
 * 草稿只接受文字、精确模式/配置选择、工作目录引用、准备凭据、附件字节与未确认的
 * `requestId`。私钥、配对码/配对秘密、Provider/MCP 凭据、imported 内容、普通会话草稿
 * 一律在运行期拒绝（`assertPreparationDraftContent` 的精确键检查）。
 */

import type {
  PreparationConfigSelection,
  PreparationReceipt,
} from "../protocol";

/** 独立 IndexedDB 数据库名（与设备密钥库、摘要缓存库分开）。 */
export const PREPARATION_DRAFT_DB_NAME = "acp-remote-preparation-v1";

/** 数据库结构版本。记录本身另有 `recordVersion`，两者独立升级。 */
export const PREPARATION_DRAFT_DB_VERSION = 1;

/** 单条草稿记录的版本。未知版本的记录不会被恢复。 */
export const PREPARATION_DRAFT_RECORD_VERSION = 1;

/** 对象存储：草稿记录与逻辑删除墓碑。 */
export const PREPARATION_DRAFT_STORE = "drafts";

/** 对象存储：附件分块字节（`Uint8Array`，绝不保存 Blob URL/File 句柄）。 */
export const PREPARATION_DRAFT_CHUNK_STORE = "chunks";

/** 对象存储：标签页归属心跳。 */
export const PREPARATION_DRAFT_OWNERSHIP_STORE = "ownership";

/** 孤儿清理窗口：24 小时（`design.md` D5）。 */
export const PREPARATION_DRAFT_ORPHAN_WINDOW_MS = 24 * 60 * 60 * 1000;

/** 持久分块大小。只影响存储读写，不构成附件产品额度。 */
export const PREPARATION_DRAFT_CHUNK_SIZE = 128 * 1024;

/** sessionStorage 中**唯一**保存的非敏感标签页定位键。 */
export const PREPARATION_TAB_STORAGE_KEY = "acp-remote.preparation.tab.v1";

/** 草稿分区键的五个组成部分。 */
export interface PreparationDraftScope {
  /** 规范来源：`https://host[:port]`，不带路径/查询。 */
  readonly canonicalOrigin: string;
  readonly ownerNodeId: string;
  readonly deviceId: string;
  /** 每个标签页一份的随机定位（复制/opener 标签页会复制它，因此还要归属握手）。 */
  readonly tabSlotId: string;
  readonly sessionId: string;
}

/** 工作目录引用：服务端下发的目录标识与规范化路径，不保存客户端自造路径。 */
export interface PreparationWorkspaceRef {
  readonly directoryId: string | null;
  readonly path: string;
  readonly source: "directoryId" | "workspacePath";
}

/**
 * 可恢复的草稿正文。
 *
 * `configOptions` 复用协议的精确选择形状（`string | boolean`）；不在这里引入
 * JavaScript `number` 以避免大整数被改写。`receipt` 是 D2 的完整准备凭据对象。
 */
export interface PreparationDraftContent {
  readonly text: string;
  readonly mode: string | null;
  readonly configOptions: PreparationConfigSelection;
  readonly agentId: string;
  readonly workspace: PreparationWorkspaceRef;
  readonly receipt: PreparationReceipt;
  /** 已发出但尚未确认的 `session.start` 请求；响应丢失时据此先核实再清理。 */
  readonly pendingRequestId: string | null;
}

/** 附件种类（与平台句柄/协议同义）。 */
export type PreparationDraftAttachmentKind = "image" | "audio" | "file";

/** 已持久化的附件元数据（字节单独按块存储）。 */
export interface PreparationDraftAttachment {
  readonly localId: string;
  readonly kind: PreparationDraftAttachmentKind;
  readonly fileName: string;
  readonly mediaType: string;
  /** 十进制字符串；不读成 number 再写回。 */
  readonly byteLength: string;
  readonly chunkSize: number;
  readonly chunkCount: number;
}

/**
 * 保存一项附件时的**流式**输入。
 *
 * 调用方（feature）只提供按区间读取的回调；端口按固定块大小逐块落盘，避免把整个文件
 * 读进内存。恢复后的字节由 `readAttachmentChunk` / `readAttachment` 提供，绝不返回
 * 原 `File`/`Blob` 句柄或临时 URL。
 */
export interface PreparationDraftAttachmentInput {
  readonly localId: string;
  readonly kind: PreparationDraftAttachmentKind;
  readonly fileName: string;
  readonly mediaType: string;
  readonly byteLength: string;
  readonly readChunk: (offset: number, length: number) => Promise<Uint8Array>;
}

/** 首次提交已受理后的请求关联：正文/Blob 已清除，只留未终结请求。 */
export interface PreparationRequestAssociation {
  readonly requestId: string;
  readonly firstSubmissionId: string | null;
  readonly firstMessageId: string | null;
  readonly acceptedAtMs: number;
}

/** 草稿载荷：准备中，或已受理但请求尚未终结。 */
export type PreparationDraftPayload =
  | {
      readonly status: "active";
      readonly content: PreparationDraftContent;
      readonly attachments: readonly PreparationDraftAttachment[];
    }
  | {
      readonly status: "accepted";
      readonly association: PreparationRequestAssociation;
    };

interface PreparationDraftBase {
  readonly recordVersion: typeof PREPARATION_DRAFT_RECORD_VERSION;
  /** 逻辑版本：每次替换 +1；迟到写必须精确匹配期望版本。 */
  readonly revision: number;
  readonly scope: PreparationDraftScope;
}

/** 一条可恢复草稿。`storageId` 指向本次版本独占的附件分块命名空间。 */
export interface PreparationDraftRecord extends PreparationDraftBase {
  readonly status: "active" | "accepted";
  readonly storageId: string | null;
  readonly createdAtMs: number;
  readonly updatedAtMs: number;
  readonly payload: PreparationDraftPayload;
}

/** 逻辑删除墓碑：保留版本号，使删除前的迟到写无法复活内容。 */
export interface PreparationDraftInvalidation extends PreparationDraftBase {
  readonly status: "invalidated";
  readonly reason: PreparationDraftInvalidationReason;
  readonly invalidatedAtMs: number;
}

/** 存储层里一条记录的两种形态。 */
export type PreparationStoredDraft = PreparationDraftRecord | PreparationDraftInvalidation;

/**
 * 草稿作废原因。
 *
 * `navigation`/`directory_changed`/`abandoned` 保留给旧记录读取；当前界面不再提供放弃动作，
 * 新代码只在更换 Agent（`agent_changed`）与身份/存储失效（`replaced`）时写墓碑。
 */
export type PreparationDraftInvalidationReason =
  | "navigation"
  | "agent_changed"
  | "directory_changed"
  | "replaced"
  | "abandoned";

/** 标签页定位：sessionStorage 中只保存这些非敏感标识，不含凭据/文字/配置。 */
export interface PreparationTabLocator {
  readonly version: typeof PREPARATION_DRAFT_RECORD_VERSION;
  readonly canonicalOrigin: string;
  readonly ownerNodeId: string;
  readonly deviceId: string;
  readonly tabSlotId: string;
  /** 本次文档实例；刷新会更换，复制/opener 会复制旧值（由导航类型区分）。 */
  readonly instanceId: string;
  readonly sessionId: string;
  readonly savedAtMs: number;
}

/** 访问句柄：分区键 + 当前文档实例。 */
export interface PreparationDraftAccess {
  readonly scope: PreparationDraftScope;
  readonly instanceId: string;
}

/** 标签页归属心跳记录（与草稿分开，清理时用于判断是否仍有活标签页）。 */
export interface PreparationTabOwnership {
  readonly recordVersion: typeof PREPARATION_DRAFT_RECORD_VERSION;
  readonly canonicalOrigin: string;
  readonly tabSlotId: string;
  readonly instanceId: string;
  readonly ownerNodeId: string;
  readonly deviceId: string;
  readonly sessionId: string;
  readonly claimedAtMs: number;
  readonly heartbeatAtMs: number;
  /** 隐藏/冻结/进入 BFCache 的时间；只用于诊断与清理，不是删除信号。 */
  readonly suspendedAtMs: number | null;
  readonly releasedAtMs: number | null;
}

/** 导航类型：刷新才允许沿同一标签页定位恢复；复制/opener 是普通导航。 */
export type PreparationNavigationKind = "reload" | "navigate" | "back_forward";

/** 标签页归属握手的判定；纯函数可脱离浏览器测试。 */
export type PreparationTabClaimDecision =
  | { readonly kind: "new_tab" }
  | { readonly kind: "scope_changed" }
  | { readonly kind: "reload_takeover" }
  | { readonly kind: "resume" }
  | {
      readonly kind: "duplicate";
      readonly reason: "live_tab_owner" | "unexpected_navigation";
    };

/** `beginTab` 的结果。 */
export type PreparationTabBeginResult =
  | {
      readonly status: "owner";
      readonly access: PreparationDraftAccess;
      readonly locator: PreparationTabLocator | null;
      /** 本标签页是否真的能跨刷新恢复（sessionStorage 是否可用）。 */
      readonly restorable: boolean;
      /** 已存在可恢复草稿（含已受理关联）时为真。 */
      readonly restored: boolean;
    }
  | {
      readonly status: "duplicate";
      readonly reason: "live_tab_owner" | "unexpected_navigation";
    };

/** 读草稿的结果（明确区分缺失、墓碑与可恢复状态）。 */
export type PreparationDraftLoadResult =
  | { readonly status: "absent" }
  | {
      readonly status: "invalidated";
      readonly revision: number;
      readonly reason: PreparationDraftInvalidationReason;
      readonly invalidatedAtMs: number;
    }
  | { readonly status: "active"; readonly record: PreparationDraftRecord }
  | {
      readonly status: "accepted";
      readonly record: PreparationDraftRecord;
      readonly association: PreparationRequestAssociation;
    };

/** 保存结果；`restorable` 为假时页面必须明确告知刷新无法恢复。 */
export interface PreparationDraftSaveResult {
  readonly revision: number;
  readonly restorable: boolean;
}

/** 生命周期事件：只更新归属心跳，绝不删除草稿。 */
export type PreparationLifecycleEvent =
  | "visibility_hidden"
  | "visibility_visible"
  | "frozen"
  | "resumed";

/** 孤儿清理报告（用于诊断/测试，不对外承诺物理删除一定成功）。 */
export interface PreparationDraftCleanupReport {
  readonly draftsDeleted: number;
  readonly ownershipDeleted: number;
  readonly chunksDeleted: number;
}

/** 端口错误分类：调用方按 `kind` 决定 UI 文案，不解析 message。 */
export type PreparationDraftErrorKind =
  | "invalid_scope"
  | "invalid_argument"
  | "forbidden_content"
  | "revision_conflict"
  | "invalidated"
  | "already_accepted"
  | "tab_not_owner"
  | "not_found"
  | "storage_unavailable"
  | "quota_exceeded"
  | "attachment_incomplete"
  | "attachment_unreadable"
  | "invalid_record";

/** 端口错误。配额失败按 `itemLocalIds` 逐项反馈，不截断、不静默降级。 */
export class PreparationDraftError extends Error {
  readonly kind: PreparationDraftErrorKind;
  readonly itemLocalIds: readonly string[];

  constructor(
    kind: PreparationDraftErrorKind,
    message: string,
    itemLocalIds: readonly string[] = [],
  ) {
    super(message);
    this.name = "PreparationDraftError";
    this.kind = kind;
    this.itemLocalIds = itemLocalIds;
  }
}

/** 端口依赖的存储形状：测试注入内存替身，生产注入浏览器 `Storage`。 */
export interface PreparationSessionStorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** 可注入的浏览器环境（Node 测试只用替身，不访问真实 IndexedDB/sessionStorage）。 */
export interface PreparationDraftEnvironment {
  readonly indexedDb?: IDBFactory | null;
  readonly sessionStorage?: PreparationSessionStorageLike | null;
  readonly now?: () => number;
  readonly newId?: () => string;
  readonly navigationKind?: () => PreparationNavigationKind;
  readonly estimateQuota?: () => Promise<{
    readonly usage: number;
    readonly quota: number;
  } | null>;
}

/** 浏览器的准备草稿端口。页面/feature 只依赖这个接口，不直接触碰 IndexedDB。 */
export interface PreparationDraftPort {
  /**
   * 读本标签页在 sessionStorage 里的非敏感定位。
   *
   * 刷新/重新挂载后 feature 层只凭它找回唯一准备槽位的 `sessionId`；它不含正文、
   * 凭据或配置，丢失或损坏时按「本标签页没有可恢复槽位」处理。
   */
  readTabLocator(): Promise<PreparationTabLocator | null>;

  /** 清掉本标签页定位（生命周期清理/身份失效）；草稿与分块由各自路径清理。 */
  clearTabLocator(): Promise<void>;

  /**
   * 标签页归属握手。
   *
   * 只有 `status === "owner"` 才能继续读写；重复/opener 标签页得到 `duplicate`
   * 且它的 sessionStorage 定位被清掉，不会接管原准备或凭据。
   */
  beginTab(input: {
    readonly scope: PreparationDraftScope;
    /**
     * 缺省时 Web 适配器读取 Navigation Timing：只有 `reload` 能沿同一标签页定位恢复；
     * 无法读取时按普通导航处理（不恢复残留）。
     */
    readonly navigationKind?: PreparationNavigationKind;
    readonly nowMs: number;
  }): Promise<PreparationTabBeginResult>;

  /** 读取当前标签页自己的草稿（墓碑/缺失/已受理分别返回）。 */
  loadDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<PreparationDraftLoadResult>;

  /**
   * 原子保存一个新版本。
   *
   * `expectedRevision` 必须与当前记录精确匹配；删除/作废后的迟到写因此被拒绝。
   * 新版本的全部附件块写完且记录提交成功前，旧版本保持可读。任何一项写失败
   * （含配额）都整体失败，不提交部分清单、不截断附件。
   */
  saveDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly content: PreparationDraftContent;
    readonly attachments?: readonly PreparationDraftAttachmentInput[];
    readonly nowMs: number;
  }): Promise<PreparationDraftSaveResult>;

  /** 按区间读取已持久化附件字节（恢复后不依赖原 File/Blob URL）。 */
  readAttachmentChunk(input: {
    readonly access: PreparationDraftAccess;
    readonly localId: string;
    readonly offset: number;
    readonly length: number;
  }): Promise<Uint8Array>;

  /** 读出完整附件字节（预览用；调用方自行遵守内存工作集）。 */
  readAttachment(input: {
    readonly access: PreparationDraftAccess;
    readonly localId: string;
  }): Promise<{ readonly attachment: PreparationDraftAttachment; readonly bytes: Uint8Array }>;

  /**
   * 权威 `accepted` 且读取验证完整消息之后调用：删除正文/Blob，只保留请求关联。
   * 保存响应丢失时**不要**调用；先按 `pendingRequestId` 查询。
   */
  markAccepted(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly association: PreparationRequestAssociation;
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }>;

  /** 命令终结后删除已受理的恢复记录。 */
  completeRequest(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly requestId: string;
    readonly nowMs: number;
  }): Promise<void>;

  /** 逻辑删除：先写墓碑与版本号，再清正文/全部分块；不能删除更新的版本。 */
  invalidateDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly reason: PreparationDraftInvalidationReason;
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }>;

  /** 可见性/冻结变化只更新归属心跳，不删除草稿。 */
  noteLifecycle(input: {
    readonly access: PreparationDraftAccess;
    readonly event: PreparationLifecycleEvent;
    readonly nowMs: number;
  }): Promise<void>;

  /** 主动离开：标记归属释放并清掉本标签页定位（草稿应先 `invalidateDraft`）。 */
  releaseTab(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<void>;

  /** 24 小时孤儿清理；活标签页持有的草稿不会被删。 */
  cleanupOrphans(input: {
    readonly nowMs: number;
  }): Promise<PreparationDraftCleanupReport>;
}

/** 稳定的草稿分区键（JSON 数组避免分隔符碰撞）。 */
export function preparationDraftKey(scope: PreparationDraftScope): string {
  return JSON.stringify([
    scope.canonicalOrigin,
    scope.ownerNodeId,
    scope.deviceId,
    scope.tabSlotId,
    scope.sessionId,
  ]);
}

/** 归属记录键：浏览器存储本身按 origin 隔离，仍显式带上 origin 便于诊断。 */
export function preparationTabOwnershipKey(
  canonicalOrigin: string,
  tabSlotId: string,
): string {
  return JSON.stringify([canonicalOrigin, tabSlotId]);
}

/** 分块键：`storageId` 让并发/新旧版本拥有互不覆盖的命名空间。 */
export function preparationChunkKey(
  draftKey: string,
  storageId: string,
  localId: string,
  index: number,
): string {
  return JSON.stringify([draftKey, storageId, localId, index]);
}

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const CANONICAL_ORIGIN_PATTERN = /^https:\/\/[^/?#]+$/;
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/;
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;

function isUuid(value: unknown): value is string {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isCanonicalOrigin(value: unknown): value is string {
  return typeof value === "string" && CANONICAL_ORIGIN_PATTERN.test(value);
}

function fail(kind: PreparationDraftErrorKind, message: string): never {
  throw new PreparationDraftError(kind, message);
}

/** 精确键检查：未知字段（可能夹带凭据/imported 标记）一律拒绝。 */
function assertExactKeys(
  value: unknown,
  allowed: readonly string[],
  path: string,
): asserts value is Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail("forbidden_content", `${path} 必须是对象`);
  }
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) {
      fail("forbidden_content", `${path} 不允许字段 ${key}`);
    }
  }
}

/** 校验草稿分区键的五项取值。 */
export function assertPreparationDraftScope(
  value: unknown,
): asserts value is PreparationDraftScope {
  assertExactKeys(
    value,
    ["canonicalOrigin", "ownerNodeId", "deviceId", "tabSlotId", "sessionId"],
    "scope",
  );
  if (!isCanonicalOrigin(value["canonicalOrigin"])) {
    fail("invalid_scope", "canonicalOrigin 必须是 https://host[:port]");
  }
  for (const key of ["ownerNodeId", "deviceId", "tabSlotId", "sessionId"] as const) {
    if (!isUuid(value[key])) fail("invalid_scope", `scope.${key} 必须是 UUID`);
  }
}

/** 校验准备凭据 claims 的精确形状（web 端口只保存/回传，不验签）。 */
function assertReceiptClaims(value: unknown): void {
  assertExactKeys(
    value,
    [
      "version",
      "ownerNodeId",
      "storeEpoch",
      "origin",
      "deviceId",
      "tabNonce",
      "sessionId",
      "agentId",
      "normalizedWorkspacePath",
      "workspaceSource",
    ],
    "receipt.claims",
  );
  if (value["version"] !== 1) fail("invalid_record", "receipt.claims.version 必须为 1");
  for (const key of ["ownerNodeId", "storeEpoch", "deviceId", "sessionId"] as const) {
    if (!isUuid(value[key])) fail("invalid_record", `receipt.claims.${key} 必须是 UUID`);
  }
  if (!isCanonicalOrigin(value["origin"])) {
    fail("invalid_record", "receipt.claims.origin 必须是规范来源");
  }
  const tabNonce = value["tabNonce"];
  if (typeof tabNonce !== "string" || !BASE64URL_PATTERN.test(tabNonce)) {
    fail("invalid_record", "receipt.claims.tabNonce 必须是 base64url");
  }
  const agentId = value["agentId"];
  if (typeof agentId !== "string" || agentId.length === 0) {
    fail("invalid_record", "receipt.claims.agentId 必须非空");
  }
  const normalizedWorkspacePath = value["normalizedWorkspacePath"];
  if (
    typeof normalizedWorkspacePath !== "string" ||
    normalizedWorkspacePath.length === 0
  ) {
    fail("invalid_record", "receipt.claims.normalizedWorkspacePath 必须非空");
  }
  if (
    value["workspaceSource"] !== "directoryId" &&
    value["workspaceSource"] !== "workspacePath"
  ) {
    fail("invalid_record", "receipt.claims.workspaceSource 非法");
  }
}

/** 校验准备凭据对象的精确形状。 */
function assertReceipt(value: unknown): asserts value is PreparationReceipt {
  assertExactKeys(value, ["claims", "signature"], "receipt");
  assertReceiptClaims(value["claims"]);
  const signature = value["signature"];
  if (typeof signature !== "string" || !BASE64URL_PATTERN.test(signature)) {
    fail("invalid_record", "receipt.signature 必须是 base64url");
  }
}

/** 校验工作目录引用。 */
function assertWorkspace(value: unknown): asserts value is PreparationWorkspaceRef {
  assertExactKeys(value, ["directoryId", "path", "source"], "content.workspace");
  const directoryId = value["directoryId"];
  if (directoryId !== null && typeof directoryId !== "string") {
    fail("forbidden_content", "content.workspace.directoryId 必须是字符串或 null");
  }
  const path = value["path"];
  if (typeof path !== "string" || path.length === 0) {
    fail("forbidden_content", "content.workspace.path 必须非空");
  }
  const source = value["source"];
  if (source !== "directoryId" && source !== "workspacePath") {
    fail("forbidden_content", "content.workspace.source 非法");
  }
  if (source === "directoryId" && directoryId === null) {
    fail("forbidden_content", "目录来源必须带 directoryId");
  }
}

/**
 * 草稿正文的**唯一**运行期守门。
 *
 * 结构化类型允许更宽的对象经变量流入，因此这里不仅检查已知字段的取值，还拒绝所有
 * 未知键与未知嵌套键：一次错误调用不能把私钥、配对材料、Provider/MCP 凭据或 imported
 * 标记写进准备库。`scope` 传入时同时核对凭据绑定（来源/节点/设备/会话/Agent/目录）。
 */
export function assertPreparationDraftContent(
  value: unknown,
  scope?: PreparationDraftScope,
): asserts value is PreparationDraftContent {
  assertExactKeys(
    value,
    [
      "text",
      "mode",
      "configOptions",
      "agentId",
      "workspace",
      "receipt",
      "pendingRequestId",
    ],
    "content",
  );
  if (typeof value["text"] !== "string") fail("forbidden_content", "content.text 必须是字符串");
  if (value["mode"] !== null && typeof value["mode"] !== "string") {
    fail("forbidden_content", "content.mode 必须是字符串或 null");
  }
  const configOptions = value["configOptions"];
  if (
    configOptions === null ||
    typeof configOptions !== "object" ||
    Array.isArray(configOptions)
  ) {
    fail("forbidden_content", "content.configOptions 必须是对象");
  }
  for (const [key, option] of Object.entries(configOptions)) {
    if (typeof option !== "string" && typeof option !== "boolean") {
      fail("forbidden_content", `content.configOptions.${key} 只能是字符串或布尔值`);
    }
  }
  const agentId = value["agentId"];
  if (typeof agentId !== "string" || agentId.length === 0) {
    fail("forbidden_content", "content.agentId 必须非空");
  }
  const workspace = value["workspace"];
  assertWorkspace(workspace);
  const receipt = value["receipt"];
  assertReceipt(receipt);
  const pendingRequestId = value["pendingRequestId"];
  if (pendingRequestId !== null && !isUuid(pendingRequestId)) {
    fail("forbidden_content", "content.pendingRequestId 必须是 UUID 或 null");
  }

  const claims = receipt.claims;
  if (claims.agentId !== agentId) {
    fail("forbidden_content", "凭据 Agent 与草稿 Agent 不一致");
  }
  if (
    claims.normalizedWorkspacePath !== workspace.path ||
    claims.workspaceSource !== workspace.source
  ) {
    fail("forbidden_content", "凭据工作目录与草稿目录引用不一致");
  }
  if (scope !== undefined) {
    if (
      claims.origin !== scope.canonicalOrigin ||
      claims.ownerNodeId !== scope.ownerNodeId ||
      claims.deviceId !== scope.deviceId ||
      claims.sessionId !== scope.sessionId
    ) {
      fail("forbidden_content", "准备凭据与草稿分区不一致");
    }
  }
}

/** 解析附件长度为安全整数；超过安全整数明确拒绝，避免静默截断。 */
export function parsePreparationByteLength(value: unknown): number {
  if (typeof value !== "string" || !DECIMAL_PATTERN.test(value)) {
    fail("invalid_argument", "附件长度必须是十进制字符串");
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    fail("invalid_argument", "附件长度超过 JavaScript 安全整数");
  }
  return parsed;
}

/** 校验已持久化附件元数据的精确形状。 */
export function assertPreparationDraftAttachment(
  value: unknown,
): asserts value is PreparationDraftAttachment {
  assertExactKeys(
    value,
    ["localId", "kind", "fileName", "mediaType", "byteLength", "chunkSize", "chunkCount"],
    "attachment",
  );
  const localId = value["localId"];
  if (typeof localId !== "string" || localId.length === 0) {
    fail("invalid_record", "attachment.localId 必须非空");
  }
  if (
    value["kind"] !== "image" &&
    value["kind"] !== "audio" &&
    value["kind"] !== "file"
  ) {
    fail("invalid_record", "attachment.kind 非法");
  }
  for (const key of ["fileName", "mediaType"] as const) {
    const field = value[key];
    if (typeof field !== "string" || field.length === 0) {
      fail("invalid_record", `attachment.${key} 必须非空`);
    }
  }
  parsePreparationByteLength(value["byteLength"]);
  const chunkSize = value["chunkSize"];
  if (
    typeof chunkSize !== "number" ||
    !Number.isSafeInteger(chunkSize) ||
    chunkSize <= 0
  ) {
    fail("invalid_record", "attachment.chunkSize 非法");
  }
  const chunkCount = value["chunkCount"];
  if (
    typeof chunkCount !== "number" ||
    !Number.isSafeInteger(chunkCount) ||
    chunkCount < 0
  ) {
    fail("invalid_record", "attachment.chunkCount 非法");
  }
}

/** 校验保存输入的附件元数据（未知字段同样拒绝）。 */
export function assertPreparationAttachmentInput(
  value: unknown,
): asserts value is PreparationDraftAttachmentInput {
  assertExactKeys(
    value,
    ["localId", "kind", "fileName", "mediaType", "byteLength", "readChunk"],
    "attachment",
  );
  const localId = value["localId"];
  if (typeof localId !== "string" || localId.length === 0) {
    fail("invalid_argument", "attachment.localId 必须非空");
  }
  if (
    value["kind"] !== "image" &&
    value["kind"] !== "audio" &&
    value["kind"] !== "file"
  ) {
    fail("invalid_argument", "attachment.kind 非法");
  }
  for (const key of ["fileName", "mediaType"] as const) {
    const field = value[key];
    if (typeof field !== "string" || field.length === 0) {
      fail("invalid_argument", `attachment.${key} 必须非空`);
    }
  }
  parsePreparationByteLength(value["byteLength"]);
  if (typeof value["readChunk"] !== "function") {
    fail("invalid_argument", "attachment.readChunk 必须是函数");
  }
}

/** 一条分块记录（值里带定位信息，孤儿清理无需反解键）。 */
export interface PreparationStoredChunk {
  readonly scopeKey: string;
  readonly storageId: string;
  readonly localId: string;
  readonly index: number;
  /** 本次保存意图落到的草稿版本；清理只删严格更旧的版本。 */
  readonly revision: number;
  readonly bytes: Uint8Array;
  readonly storedAtMs: number;
}

/**
 * 标签页归属握手的**纯决策**。
 *
 * 规则：
 * - 普通导航（包括复制标签页/opener）永远不恢复残留，也不能接管活归属；
 * - 只有刷新（`reload`）能沿同一标签页定位接管；
 * - BFCache 恢复（`back_forward`）仅当文档实例仍是原归属时继续持有；
 * - 同一标签页换了准备分区（不同 sessionId）按新标签页处理，旧草稿留给孤儿清理。
 */
export function decideTabClaim(input: {
  readonly locator: PreparationTabLocator | null;
  readonly ownership: PreparationTabOwnership | null;
  readonly scope: PreparationDraftScope;
  readonly navigationKind: PreparationNavigationKind;
}): PreparationTabClaimDecision {
  const { locator, ownership, scope, navigationKind } = input;
  if (locator === null) return { kind: "new_tab" };
  const sameScope =
    locator.canonicalOrigin === scope.canonicalOrigin &&
    locator.ownerNodeId === scope.ownerNodeId &&
    locator.deviceId === scope.deviceId &&
    locator.tabSlotId === scope.tabSlotId &&
    locator.sessionId === scope.sessionId;
  if (!sameScope) return { kind: "scope_changed" };

  if (
    ownership !== null &&
    navigationKind === "back_forward" &&
    ownership.releasedAtMs === null &&
    ownership.instanceId === locator.instanceId
  ) {
    return { kind: "resume" };
  }
  if (navigationKind === "reload") return { kind: "reload_takeover" };
  if (ownership !== null && ownership.releasedAtMs === null) {
    return { kind: "duplicate", reason: "live_tab_owner" };
  }
  return { kind: "duplicate", reason: "unexpected_navigation" };
}

/**
 * 24 小时孤儿清理的**纯计划**。
 *
 * - 墓碑不按窗口物理删除：它只含版本与原因，必须保留以阻止删除前的迟到写；
 * - 活跃草稿只有超过窗口且**没有活归属**（未释放且心跳仍在窗口内）时才删除；
 * - 已受理的请求关联不按草稿 TTL 删除，等命令终结后由调用方 `completeRequest`；
 * - 归属记录释放或心跳超过窗口后回收，但仍有活跃/已受理草稿时必须保留。
 *
 * 隐藏/冻结本身不进入删除条件：它只影响心跳时间，不能单独导致草稿被删。
 */
export function orphanCleanupPlan(input: {
  readonly drafts: readonly PreparationStoredDraft[];
  readonly ownership: readonly PreparationTabOwnership[];
  readonly nowMs: number;
  readonly orphanWindowMs?: number;
}): {
  readonly draftKeys: readonly string[];
  readonly ownershipKeys: readonly string[];
} {
  const windowMs = input.orphanWindowMs ?? PREPARATION_DRAFT_ORPHAN_WINDOW_MS;
  const owners = new Map<string, PreparationTabOwnership>();
  for (const owner of input.ownership) {
    owners.set(preparationTabOwnershipKey(owner.canonicalOrigin, owner.tabSlotId), owner);
  }

  const draftKeys: string[] = [];
  for (const draft of input.drafts) {
    const key = preparationDraftKey(draft.scope);
    // 墓碑只含版本与原因，体积很小；必须保留以拒绝删除前的迟到写，
    // 不能按正文草稿的 24 小时窗口物理删除，否则迟到的首次保存可能复活内容。
    if (draft.status === "invalidated") continue;
    if (draft.status === "accepted") continue;
    if (input.nowMs - draft.updatedAtMs < windowMs) continue;
    const owner = owners.get(
      preparationTabOwnershipKey(draft.scope.canonicalOrigin, draft.scope.tabSlotId),
    );
    const ownerLive =
      owner !== undefined &&
      owner.releasedAtMs === null &&
      input.nowMs - owner.heartbeatAtMs < windowMs;
    if (!ownerLive) draftKeys.push(key);
  }

  const ownershipKeys: string[] = [];
  const deletedDraftKeys = new Set(draftKeys);
  const keptOwnerKeys = new Set<string>();
  for (const draft of input.drafts) {
    const key = preparationDraftKey(draft.scope);
    if (draft.status === "invalidated" || deletedDraftKeys.has(key)) continue;
    keptOwnerKeys.add(
      preparationTabOwnershipKey(draft.scope.canonicalOrigin, draft.scope.tabSlotId),
    );
  }
  for (const owner of input.ownership) {
    const key = preparationTabOwnershipKey(owner.canonicalOrigin, owner.tabSlotId);
    // 仍有草稿（含已受理关联）存活时必须保留归属，否则活标签页再也读不到它。
    if (keptOwnerKeys.has(key)) continue;
    const releasedLongAgo =
      owner.releasedAtMs !== null && input.nowMs - owner.releasedAtMs >= windowMs;
    const heartbeatStale = input.nowMs - owner.heartbeatAtMs >= windowMs;
    if (releasedLongAgo || heartbeatStale) ownershipKeys.push(key);
  }

  return { draftKeys, ownershipKeys };
}
