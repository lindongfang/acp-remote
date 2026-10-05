/**
 * `schemas/sync/v1/` 的公开入口。
 *
 * 每个模块镜像一份 schema，字段名、可选性与取值域与 schema 一一对应；本文件只做重导出，
 * 不新增规则。`message.schema.json` 的顶层 `oneOf` 在这里表达为 `SyncWireMessage`。
 *
 * 上游消费方（`src/sync-client/`、`src/state/`、`src/features/`）只从这里 import，
 * 不直接指向某个 schema 模块——这样 schema 拆分变化时只有本文件需要改。
 *
 * **运行期面与类型面分离**：本文件被浏览器（`expo export --platform web` 的 Metro bundle）
 * 与 Node 测试共同消费，因此这里**只**重导出两类东西——纯类型，以及不触碰 Node 内置模块的
 * 纯计算函数。任何 `node:fs`/`node:path`/`node:url` 依赖都不得进入本文件的运行期导出：
 * Metro 对 web 平台把 Node 内置模块垫成空模块，一旦被牵连，`fileURLToPath(import.meta.url)`
 * 与 `readFileSync` 在浏览器里就是 `undefined`，协议层公开导入面会在模块求值期直接失效。
 *
 * 「读仓库根文件」这一类入口（其中 `resolveFixtureCase` 的类型仍在这里导出）只对 Node 侧
 * （契约测试、工具）开放，按需从子路径取：
 *   - 仓库根路径解析：`./paths`
 *   - manifest 严格读取：`./manifest`
 */

export type * from "./common";
export type * from "./auth";
export type * from "./sync";
export type * from "./command";
export type * from "./event";
export type * from "./error";
export type * from "./pairing";

// 纯计算 + 编译期常量：无 Node 依赖，浏览器与 `src/domain/` 都可安全消费。
export { projectView, isKnownEventType, KNOWN_EVENT_TYPE_COUNT } from "./event";
export type { FixtureCase, FixtureManifest, ResolvedFixtureCase } from "./manifest";

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
