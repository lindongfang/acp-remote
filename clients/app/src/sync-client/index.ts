/**
 * 同步客户端层（`docs/FRONTEND_DESIGN.md` §3 的 `sync-client/`）。
 *
 * **归属 WP6**（`plan.md` 的 WP6 行）：连接、逐连接签名握手、`subscribe`、ACK、
 * 增量重放、快照暂存与原子替换、`eventId` 去重、`requestId` 幂等重试都由 WP6 实现。
 * WP5a 只固定这一层的**位置**与依赖方向（见 `../layers.ts`），不写实现。
 *
 * 本模块导出该层对外消费 `src/protocol/` 的类型别名，使 WP6 的模块签名在本层可读，
 * 同时不引入任何实现。
 */

import type { Cursor, EventMessage, SyncWireMessage } from "../protocol";

/** 已确认的全局游标（ACK 的语义载体）。 */
export type GlobalCursor = Cursor;

/** 从 wire 收到的持久化事件。 */
export type IncomingEvent = EventMessage;

/** 任意一条 WSS 消息（解码前反而是 `unknown`；WP6 负责严格解码）。 */
export type WireMessage = SyncWireMessage;

/**
 * 事件去重的键：稳定 `eventId`（`AGENTS.md` §3「客户端按稳定 `eventId` 去重」）。
 * 单独命名以避免 WP6 用 `messageId` 或连接序号误作去重键。
 */
export type EventDedupKey = EventMessage["body"]["eventId"];
