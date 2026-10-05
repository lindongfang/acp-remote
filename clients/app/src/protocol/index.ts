/**
 * `schemas/sync/v1/` 的公开入口。
 *
 * 每个模块镜像一份 schema，字段名、可选性与取值域与 schema 一一对应；本文件只做重导出，
 * 不新增规则。`message.schema.json` 的顶层 `oneOf` 在这里表达为 `SyncWireMessage`。
 *
 * 上游消费方（`src/sync-client/`、`src/state/`、`src/features/`）只从这里 import，
 * 不直接指向某个 schema 模块——这样 schema 拆分变化时只有本文件需要改。
 */

export type * from "./common";
export type * from "./auth";
export type * from "./sync";
export type * from "./command";
export type * from "./event";
export type * from "./error";
export type * from "./pairing";

export { projectView, isKnownEventType, KNOWN_EVENT_TYPE_COUNT } from "./event";
export { readManifest, resolveFixtureCase, MANIFEST_SCHEMA_VERSION } from "./manifest";
export type { FixtureCase, FixtureManifest, ResolvedFixtureCase } from "./manifest";
export { repoRoot, syncFixtureDir, syncSchemaDir, syncManifestPath, SYNC_SCHEMA_FILES } from "./paths";

import type { AuthMessage } from "./auth";
import type { SyncMessage } from "./sync";
import type { EventMessage } from "./event";
import type { CommandSchemaMessage } from "./command";
import type { ErrorSchemaMessage } from "./error";

/**
 * `message.schema.json` 的顶层 `oneOf`：任何一条 WSS 消息都是这五类之一。
 *
 * 判别方式是 `type` 字段的前缀/取值，而**不是**对 JSON 做宽容解析：
 * 解码与逐字段校验由 `src/sync-client/`（WP6）负责，本层只描述形状。
 */
export type SyncWireMessage =
  | AuthMessage
  | SyncMessage
  | EventMessage
  | CommandSchemaMessage
  | ErrorSchemaMessage;
