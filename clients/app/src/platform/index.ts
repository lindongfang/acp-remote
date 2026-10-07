/**
 * `src/platform/` 的公开入口：平台端口的**唯一**装配面。
 *
 * 归属 WP5b。`AGENTS.md` §5 与 `docs/FRONTEND_DESIGN.md` §3 要求页面与 feature 层
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

export type { LifecycleEvent, LifecyclePort, Unsubscribe, VisibilityState } from "./lifecycle";
export type {
  CacheEntry,
  CacheHit,
  CacheOrigin,
  CacheStats,
  ImportedMetadata,
  LocalCachePort,
  VolatileImportedContent,
} from "./local-cache";
export {
  CachePolicyError,
  IMPORTED_METADATA_MAX_BYTES,
  IMPORTED_METADATA_TTL_MS,
  SUMMARY_CACHE_MAX_BYTES,
  SUMMARY_CACHE_TTL_MS,
  assertPersistableForOrigin,
  estimateEntryBytes,
  evictToFit,
} from "./local-cache";
export type { DeviceIdentity, DeviceIdentityPort, SecureStorageErrorKind } from "./secure-storage";
export { DEVICE_IDENTITY_STORE, SecureStorageError } from "./secure-storage";
export { createVolatileImportedContent } from "./local-cache.web";

import type { LifecyclePort } from "./lifecycle";
import type { LocalCachePort, VolatileImportedContent } from "./local-cache";
import type { DeviceIdentityPort } from "./secure-storage";
import { createVolatileImportedContent as createVolatile } from "./local-cache.web";

/** 平台端口束：组合根装配一次，向下传给 Sync Client / 状态机 / feature。 */
export interface PlatformPorts {
  readonly deviceIdentity: DeviceIdentityPort;
  readonly localCache: LocalCachePort;
  /** imported 正文的内存载体：**不是**持久端口，刷新即失效（R20）。 */
  readonly importedContent: VolatileImportedContent;
  readonly lifecycle: LifecyclePort;
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
    importedContent: createVolatile(),
    lifecycle: openLifecycle(),
  };
}
