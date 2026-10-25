/**
 * feature 层向组合根索取的**端口**（`docs/FRONTEND_DESIGN.md` §3 的 `features/` 层）。
 *
 * ## 为什么是窄接口而不是 `SyncClient`
 *
 * 组合根持有真正的 `SyncClient`，但 feature 层只用到「派发命令」「读最后一次已完成快照」
 * 「读本次认证结果」三件事。把它们声明成窄接口有三个直接好处：
 *
 * 1. **R15 的负向断言可执行**：目录页的视图模型只能经由这个窄接口拿数据，而窄接口**不含**
 *    任何查询命令，因此「目录数据 MUST 来自快照、MUST NOT 依赖额外的在线查询命令」不再只是
 *    约定——测试注入一个「任何 dispatch 都记账并抛错」的替身即可证伪。
 * 2. **R19 的负向断言可执行**：连接状态覆盖层只能来自事件（`ingest`），窄接口不提供
 *    「按会话活跃度查询 Agent 状态」这类入口。
 * 3. 组合根与 feature 解耦：换原生端时 feature 的依赖面不变。
 *
 * **本层不触碰任何平台 API**：WebSocket、IndexedDB、WebCrypto 只存在于 `src/platform/` 与
 * `src/sync-client/`，由组合根装配后从这里进入（`AGENTS.md` §6）。
 */

import type {
  CommandMessage,
  PreparationAttachmentTarget,
  PreparationReceipt,
  SessionPreparationBody,
  SessionRef,
} from "../protocol";
import type { AuthenticatedInfo, CompletedSnapshot, EventDedupKey } from "../sync-client";

/** 快照读取与刷新面：断连时保留已验证清单，刷新复用同步协议。 */
export interface SnapshotGateway {
  /** 最后一次**验证通过并完成替换**的快照；从未成功同步过为 `null`。 */
  readonly current: CompletedSnapshot | null;
  /** 资源清单缺项时请求完整快照；复用当前连接，不重新认证。 */
  refreshSnapshot(): void;
}

/** 命令派发面。 */
export interface CommandGateway {
  /** 以稳定 `requestId` 派发一条命令；重试复用同一 ID。 */
  dispatch(command: CommandMessage): void;
}

/**
 * 查看控制面（`session.view`，`core.local-session-lifecycle.v1`）。
 *
 * 只有协商了该 feature 的连接才调用它；没有已认证连接时返回 `false`（调用方据此不把声明
 * 当成已生效），而不是凭空记一条「看起来发出去了」的状态。
 */
export interface SessionViewGateway {
  setSessionView(input: { readonly sessionId: string | null; readonly viewVersion: number }): boolean;
}

/** 认证结果（`deviceId` / `serverEpoch` / 限额；wire 上没有授权集合，创建入口也不依据它做任何判定）。 */
export interface AuthenticatedGateway {
  /** 本次连接的认证结果；未认证为 `null`。 */
  readonly authenticatedInfo: AuthenticatedInfo | null;
}

/**
 * 会话准备控制面（`core.session-preparation.v1`，§11.9）。
 *
 * 只在服务端选中该 feature 后调用；返回 `false` 表示帧未发出（没有已认证连接），
 * 调用方据此把操作停在「未发出」而不是假装已分配。
 */
export interface PreparationGateway {
  sendPreparation(body: SessionPreparationBody): boolean;
}

/**
 * 远程来源订阅面（`resource.stateless-origin.v1`，§9.7）。
 *
 * 门面保存当前运行期的最后连续位置 C 与去重窗口；feature 层只表达「哪条来源会话要订阅」。
 * **没有**任何持久化或离线队列入口——刷新后重新订阅并回源读取重建。
 */
export interface RemoteSourceGateway {
  subscribeRemote(sessionRef: SessionRef): void;
  unsubscribeRemote(sessionRef: SessionRef): void;
}


/**
 * 已建立连接的标识。
 *
 * 命令信封的 `connectionId` 是**必填**字段（`$defs.command`），因此 store 在派发任何命令前
 * 必须先拿到它；未连接时为 `null`，此时 store **不派发**命令而不是凭空编一个标识。
 */
export interface ConnectionIdentity {
  readonly connectionId: string | null;
}
/**
 * feature 层面对同步客户端的**全部**依赖面。
 *
 * `SyncClient` 结构上满足它（`snapshots` 是只读属性），组合根可直接传入；
 * 测试则注入记账替身来证明「目录渲染不发任何命令」。
 */
export type SyncGateway = SnapshotGateway &
  CommandGateway &
  AuthenticatedGateway &
  PreparationGateway &
  RemoteSourceGateway &
  SessionViewGateway;

/** 标识生成面（`requestId` / `messageId`）。 */
export interface IdentifierSource {
  uuid(): string;
}

/**
 * 平台侧的**附件/媒体端口**（由组合根注入；feature 层不碰 DOM/Blob/File）。
 *
 * `AttachmentFileHandle` 是平台句柄的**不透明箱**：它带展示所需的元数据，真正的字节只在
 * `readChunk` 里被读出。因此选择阶段只做本机预览，MUST NOT 上传（spec「发送后才上传」）。
 */
export interface AttachmentFileHandle {
  /** 页内稳定标识；一次选择内唯一。 */
  readonly localId: string;
  readonly kind: "image" | "audio" | "file";
  readonly fileName: string;
  readonly mediaType: string;
  /** 十进制字符串；平台不把它读成 number 再交回。 */
  readonly byteLength: string;
}

export interface AttachmentMediaPort {
  /** 打开平台选择器；取消返回空数组（不是错误）。 */
  pick(kind: "image" | "audio" | "file"): Promise<readonly AttachmentFileHandle[]>;
  /** 从粘贴事件里取出图片/文件；没有可粘贴内容返回空数组。 */
  fromPaste(event: unknown): Promise<readonly AttachmentFileHandle[]>;
  /** 读取 `[offset, offset+length)`；调用方保证 32 KiB 以内。 */
  readChunk(handle: AttachmentFileHandle, offset: bigint, length: number): Promise<Uint8Array>;
  /** 选择阶段的本机预览 URL；平台不支持返回 `null`（UI 明确说明，不伪造）。 */
  previewUrl(handle: AttachmentFileHandle): string | null;
  /** 媒体字节 → 对象 URL；平台不支持返回 `null`。 */
  createObjectUrl(bytes: Uint8Array, mimeType: string): string | null;
  /** 立即撤销对象 URL（关闭/替换/卸载时调用）。 */
  releaseUrl(url: string): void;
}

/** 一次附件上传的输入（正文只在 `readChunk` 里按需读取）。 */
export interface AttachmentUploadRequest {
  readonly sessionId: string;
  readonly subjectId: string;
  readonly transferId: string;
  readonly fileName: string;
  readonly mediaType: string;
  readonly totalLength: string;
  /**
   * 准备目标（`design.md` D6.1）：出现时 `attachment.begin` 走 `target` 分支，
   * 顶层 `sessionId` 不再出现。首条提交之前的上传**只有**这一种合法形式。
   */
  readonly target?: PreparationAttachmentTarget;
  readonly readChunk: (offset: bigint, length: number) => Promise<Uint8Array>;
}

/** 附件封存事实（`attachment.sealed`）。 */
export interface AttachmentUploadSealed {
  readonly attachmentId: string;
  readonly sha256: string;
  readonly sealedAt: string;
}

/** 一次清单封存的输入。 */
export interface AttachmentManifestRequest {
  readonly sessionId: string;
  readonly subjectId: string;
  readonly submissionId: string;
  readonly transferIds: readonly string[];
  /** 准备清单（与上传同一目标）：出现时 `attachment.manifest_begin` 走 `target` 分支。 */
  readonly target?: PreparationAttachmentTarget;
}

/** 清单封存事实（`attachment.manifest_sealed`）。 */
export interface AttachmentManifestSealedResult {
  readonly attachmentCount: string;
  readonly manifestDigest: string;
}

/** 一次附件读取请求。 */
export interface AttachmentReadRequest {
  readonly attachmentId: string;
  readonly sessionId: string;
  readonly offset: string;
  readonly maxLength: number;
}

/** 读取到的一块（`attachment.read_chunk`）；`eof` 为真表示这是最后一块。 */
export interface AttachmentReadResult {
  readonly offset: string;
  readonly length: string;
  readonly totalLength: string;
  readonly data: Uint8Array;
  readonly eof: boolean;
}

/** 一次附件上传/读取的窄接口（由 `sync-client` 的附件连接适配，组合根装配）。 */
export interface AttachmentTransferGateway {
  upload(input: AttachmentUploadRequest): Promise<AttachmentUploadSealed>;
  sealManifest(input: AttachmentManifestRequest): Promise<AttachmentManifestSealedResult>;
  read(input: AttachmentReadRequest): Promise<AttachmentReadResult>;
  abort(transferId: string, reason: "client_cancelled" | "budget_exceeded" | "revoked" | "digest_mismatch"): void;
}

/** 去重键别名（feature 层按 `eventId` 去重的键，与 `src/sync-client` 同源）。 */
export type { EventDedupKey };

// ── 浏览器准备草稿端口（`web-session-preparation` D5；平台实现由组合根注入）──────────
//
// 这些声明是 `src/platform/preparation-draft.shared.ts` 的**结构镜像**：feature 层不得
// import 平台层（`src/layers.test.ts`），因此端口形状在这里再声明一次，由组合根把平台实现
// 原样装配进来（结构相同即赋值成立）。字段含义见 WP6 的共享声明。

/** 草稿分区键：canonical origin + ownerNodeId + deviceId + tabSlotId + sessionId。 */
export interface PreparationDraftScope {
  readonly canonicalOrigin: string;
  readonly ownerNodeId: string;
  readonly deviceId: string;
  readonly tabSlotId: string;
  readonly sessionId: string;
}

/** 工作目录引用（服务端下发的标识与逐字路径）。 */
export interface PreparationDraftWorkspace {
  readonly directoryId: string | null;
  readonly path: string;
  readonly source: "directoryId" | "workspacePath";
}

/** 可恢复的草稿正文（准备中的那一份）。 */
export interface PreparationDraftContent {
  readonly text: string;
  readonly mode: string | null;
  readonly configOptions: Readonly<Record<string, string | boolean>>;
  readonly agentId: string;
  readonly workspace: PreparationDraftWorkspace;
  readonly receipt: PreparationReceipt;
  readonly pendingRequestId: string | null;
}

/** 已持久化的附件元数据（字节单独按块存储）。 */
export interface PreparationDraftAttachmentMeta {
  readonly localId: string;
  readonly kind: "image" | "audio" | "file";
  readonly fileName: string;
  readonly mediaType: string;
  readonly byteLength: string;
  readonly chunkSize: number;
  readonly chunkCount: number;
}

/** 保存一项附件时的流式输入。 */
export interface PreparationDraftAttachmentInput extends PreparationDraftAttachmentMeta {
  readonly readChunk: (offset: number, length: number) => Promise<Uint8Array>;
}

/** 首次提交已受理后的请求关联（正文/Blob 已清除）。 */
export interface PreparationRequestAssociation {
  readonly requestId: string;
  readonly firstSubmissionId: string | null;
  readonly firstMessageId: string | null;
  readonly acceptedAtMs: number;
}

/** 一次草稿读写的访问句柄。 */
export interface PreparationDraftAccess {
  readonly scope: PreparationDraftScope;
  readonly instanceId: string;
}

/**
 * 本标签页在 sessionStorage 里的**非敏感**定位（结构镜像）。
 *
 * 只用于刷新/重新挂载后找回唯一准备槽位：不含正文、凭据、配置或附件字节。
 */
export interface PreparationTabLocatorSnapshot {
  readonly canonicalOrigin: string;
  readonly ownerNodeId: string;
  readonly deviceId: string;
  readonly tabSlotId: string;
  readonly sessionId: string;
}

/** `beginTab` 的结果（只保留 feature 层用到的两支）。 */
export type PreparationTabBeginResult =
  | {
      readonly status: "owner";
      readonly access: PreparationDraftAccess;
      readonly restorable: boolean;
      readonly restored: boolean;
    }
  | { readonly status: "duplicate"; readonly reason: "live_tab_owner" | "unexpected_navigation" };

/** 草稿载荷：准备中，或已受理但请求尚未终结（与平台端口的联合同形）。 */
export type PreparationDraftPayload =
  | {
      readonly status: "active";
      readonly content: PreparationDraftContent;
      readonly attachments: readonly PreparationDraftAttachmentMeta[];
    }
  | {
      readonly status: "accepted";
      readonly association: PreparationRequestAssociation;
    };

/** 草稿读取结果（只保留出现的分支）。 */
export type PreparationDraftLoadResult =
  | { readonly status: "absent" }
  | { readonly status: "invalidated"; readonly reason: string }
  | {
      readonly status: "active";
      readonly record: { readonly revision: number; readonly payload: PreparationDraftPayload };
    }
  | {
      readonly status: "accepted";
      readonly record: { readonly revision: number };
      readonly association: PreparationRequestAssociation;
    };

/** 浏览器准备草稿端口（组合根注入；缺省时准备流程只保留内存草稿并如实说明）。 */
export interface PreparationDraftGateway {
  /**
   * 读本标签页的非敏感定位。
   *
   * 刷新后 feature 层据此找回唯一准备槽位的 `sessionId`；没有定位时不猜任何会话。
   */
  readTabLocator(): Promise<PreparationTabLocatorSnapshot | null>;
  /** 清掉本标签页定位（显式放弃/身份失效）；不删除草稿记录本身。 */
  clearTabLocator(): Promise<void>;
  beginTab(input: {
    readonly scope: PreparationDraftScope;
    readonly nowMs: number;
  }): Promise<PreparationTabBeginResult>;
  loadDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<PreparationDraftLoadResult>;
  saveDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly content: PreparationDraftContent;
    readonly attachments?: readonly PreparationDraftAttachmentInput[];
    readonly nowMs: number;
  }): Promise<{ readonly revision: number; readonly restorable: boolean }>;
  readAttachment(input: {
    readonly access: PreparationDraftAccess;
    readonly localId: string;
  }): Promise<{
    readonly attachment: PreparationDraftAttachmentMeta;
    readonly bytes: Uint8Array;
  }>;
  markAccepted(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly association: PreparationRequestAssociation;
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }>;
  completeRequest(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number;
    readonly requestId: string;
    readonly nowMs: number;
  }): Promise<void>;
  invalidateDraft(input: {
    readonly access: PreparationDraftAccess;
    readonly expectedRevision: number | null;
    readonly reason: "navigation" | "agent_changed" | "directory_changed" | "replaced" | "abandoned";
    readonly nowMs: number;
  }): Promise<{ readonly revision: number }>;
  releaseTab(input: {
    readonly access: PreparationDraftAccess;
    readonly nowMs: number;
  }): Promise<void>;
}
