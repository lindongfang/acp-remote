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
 * `src/sync-client/`，由组合根装配后从这里进入（`AGENTS.md` §5）。
 */

import type { CommandMessage } from "../protocol";
import type { AuthenticatedInfo, CompletedSnapshot, EventDedupKey } from "../sync-client";

/** 快照仓库的只读面：断连时目录页与主机面板的唯一数据来源（R15/R19）。 */
export interface SnapshotGateway {
  /** 最后一次**验证通过并完成替换**的快照；从未成功同步过为 `null`。 */
  readonly current: CompletedSnapshot | null;
}

/** 命令派发面。 */
export interface CommandGateway {
  /** 以稳定 `requestId` 派发一条命令；重试复用同一 ID。 */
  dispatch(command: CommandMessage): void;
}

/** 认证结果（scopes 决定创建会话入口是否可用，R16）。 */
export interface AuthenticatedGateway {
  /** 本次连接授予的 scopes；未认证为 `null`。 */
  readonly authenticatedInfo: AuthenticatedInfo | null;
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
export type SyncGateway = SnapshotGateway & CommandGateway & AuthenticatedGateway;

/** 标识生成面（`requestId` / `messageId`）。 */
export interface IdentifierSource {
  uuid(): string;
}

/** 去重键别名（feature 层按 `eventId` 去重的键，与 `src/sync-client` 同源）。 */
export type { EventDedupKey };
