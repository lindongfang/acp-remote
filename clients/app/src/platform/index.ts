/**
 * `src/platform/` 的公开入口：平台端口的**唯一**装配面。
 *
 * 归属 WP5b。`AGENTS.md` §6 与 `docs/FRONTEND_DESIGN.md` §3 要求页面与 feature 层
 * 不直接触碰 WebSocket/IndexedDB/WebCrypto/生命周期；`src/layers.test.ts` 也把
 * `src/platform/` 定为「不得被内层依赖」的平台层。因此上层消费的**唯一**合法形状是
 * 这里导出的端口**接口**与它们的装配函数——端口形状与平台实现分离，
 * 将来原生端接入时上层签名不变。
 *
 * 本文件同时是**装配点**：`openPlatform()` 返回一束端口，由组合根（WP6/WP7 的接线）
 * 在启动时调用一次。散落的全局访问（直接 `indexedDB.open`/`crypto.subtle`/`document.addEventListener`）
 * 在 `src/platform/` 之外没有任何入口。
 *
 * v1 只交付 Web/PWA：这里装配的全是 `.web.ts` 实现，**没有** `.native.ts` 变体
 * （`tasks.md` 2.6 的裁定，不是「没人做」）。
 */

import { openLifecycle } from "./lifecycle.web";
import { openLocalCache } from "./local-cache.web";
import { openDeviceIdentityStore } from "./secure-storage.web";
import { openDownloadPort } from "./download.web";
import { openAttachmentMediaPort } from "./attachment.web";
import { openPreparationDraftStore } from "./preparation-draft.web";

export type { LifecycleEvent, LifecyclePort, Unsubscribe, VisibilityState } from "./lifecycle";
export type { CacheEntry, CacheHit, CacheStats, LocalCachePort } from "./local-cache";
export {
  CachePolicyError,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  assertLocalSummary,
  estimateEntryBytes,
  evictToFit,
} from "./local-cache";
export type { DeviceIdentity, DeviceIdentityPort, SecureStorageErrorKind } from "./secure-storage";
export { DEVICE_IDENTITY_STORE, SecureStorageError } from "./secure-storage";
export type {
  DownloadArtifact,
  DownloadEnvironment,
  DownloadErrorKind,
  DownloadMode,
  DownloadPort,
  DownloadSink,
  DownloadTarget,
  PreviewArtifact,
  WritableFileLike,
} from "./download.shared";
export { BLOB_DOWNLOAD_LIMIT_BYTES, DownloadError } from "./download.shared";
export { browserDownloadEnvironment, openDownloadPort } from "./download.web";
export type {
  PlatformAttachmentHandle,
  PlatformAttachmentKind,
  PlatformAttachmentPort,
} from "./attachment";
export { openAttachmentMediaPort } from "./attachment.web";
export type {
  PreparationDraftAccess,
  PreparationDraftAttachment,
  PreparationDraftAttachmentInput,
  PreparationDraftAttachmentKind,
  PreparationDraftCleanupReport,
  PreparationDraftContent,
  PreparationDraftEnvironment,
  PreparationDraftErrorKind,
  PreparationDraftInvalidation,
  PreparationDraftInvalidationReason,
  PreparationDraftLoadResult,
  PreparationDraftPayload,
  PreparationDraftPort,
  PreparationDraftRecord,
  PreparationDraftSaveResult,
  PreparationDraftScope,
  PreparationLifecycleEvent,
  PreparationNavigationKind,
  PreparationRequestAssociation,
  PreparationSessionStorageLike,
  PreparationStoredChunk,
  PreparationStoredDraft,
  PreparationTabBeginResult,
  PreparationTabClaimDecision,
  PreparationTabLocator,
  PreparationTabOwnership,
  PreparationWorkspaceRef,
} from "./preparation-draft";
export {
  PREPARATION_DRAFT_CHUNK_SIZE,
  PREPARATION_DRAFT_DB_NAME,
  PREPARATION_DRAFT_DB_VERSION,
  PREPARATION_DRAFT_ORPHAN_WINDOW_MS,
  PREPARATION_DRAFT_RECORD_VERSION,
  PREPARATION_TAB_STORAGE_KEY,
  PreparationDraftError,
  assertPreparationAttachmentInput,
  assertPreparationDraftAttachment,
  assertPreparationDraftContent,
  assertPreparationDraftScope,
  decideTabClaim,
  orphanCleanupPlan,
  parsePreparationByteLength,
  preparationChunkKey,
  preparationDraftKey,
  preparationTabOwnershipKey,
} from "./preparation-draft";
export { openPreparationDraftStore } from "./preparation-draft.web";

import type { LifecyclePort } from "./lifecycle";
import type { LocalCachePort } from "./local-cache";
import type { DeviceIdentityPort } from "./secure-storage";
import type { DownloadPort } from "./download.shared";
import type { PlatformAttachmentPort } from "./attachment";
import type { PreparationDraftPort } from "./preparation-draft";

/** 平台端口束：组合根装配一次，向下传给 Sync Client / 状态机 / feature。 */
export interface PlatformPorts {
  readonly deviceIdentity: DeviceIdentityPort;
  readonly localCache: LocalCachePort;
  readonly lifecycle: LifecyclePort;
  /** 主动下载的保存/取消/对象 URL 生命周期端口（正文不落持久缓存）。 */
  readonly download: DownloadPort;
  /**
   * 附件选择/预览端口（可选：注入的替身平台束可以没有它）。
   *
   * 缺省时组合根**不**向仓库装配附件：`attachmentsSupported` 为假并给出说明。生产实现由
   * `openPlatform()` 提供。
   */
  readonly attachments?: PlatformAttachmentPort;
  /**
   * 浏览器准备草稿端口（`web-session-preparation` D5）。
   *
   * 与 `attachments` 一样按可选装配：注入的替身平台束可以没有它；生产实现由
   * `openPlatform()` 提供。只有本机 Web 准备页会在首次提交前读写它。
   */
  readonly preparationDrafts?: PreparationDraftPort;
}

/**
 * 装配 Web/PWA 平台端口。
 *
 * 纯构造、不做 IO；能力探测发生在各端口的首次用法，失败以分类错误返回，
 * 不会让模块求值期崩溃（与 `docs/FRONTEND_DESIGN.md` §4.5 的「安全能力不足时
 * 只能进入显式开发模式，不能静默降低正式认证要求」一致）。
 */
export function openPlatform(): PlatformPorts {
  return {
    deviceIdentity: openDeviceIdentityStore(),
    localCache: openLocalCache(),
    lifecycle: openLifecycle(),
    download: openDownloadPort(),
    // 页内句柄标识由平台生成：与设备身份/配对无关，只是本次选择内的稳定键。
    attachments: openAttachmentMediaPort(() => globalThis.crypto.randomUUID()),
    preparationDrafts: openPreparationDraftStore(),
  };
}
