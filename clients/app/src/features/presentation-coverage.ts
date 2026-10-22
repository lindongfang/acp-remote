/**
 * 客户端自有的**呈现覆盖表**（`design.md` D5；`pwa-web-client` 的「呈现覆盖由客户端自持」）。
 *
 * ## 为什么它必须与协议词表分开
 *
 * `src/protocol/event.ts` 的 `KNOWN_EVENT_TYPES` / `isKnownEventType` 回答的是
 * 「这条事件的 `view` 能不能按已知视图形状解析」——那是**解析**的问题，由协议决定。
 * 「客户端拿它画什么」是**呈现**的问题，只能由客户端自己维护：服务端多发一个事件类型，
 * 不该自动让客户端宣称有了它的视图。
 *
 * 早先 `degradation-model.ts` 直接把 `isKnownEventType` 当成「客户端有专用视图」，于是
 * 「协议已知 + 客户端没有视图」的事件既不走卡片也不走降级卡（被静默丢掉）——这正是 R18
 * 禁止的第三种结果。本表把这件事拆开：**词表只决定解析，覆盖决定呈现**。
 *
 * ## 判定只有两种结果
 *
 * 对任一被消费的事件：命中本表 → 按对应通道呈现；未命中 → 降级呈现（见
 * `degradation-model.ts` 的 `no_dedicated_view`）——会话级事件走时间线降级卡片，节点级事件
 * （没有 `sessionId`）走主机/连接面的节点级降级列表。**没有第三种结果。**
 *
 * 内容缺失（原文未下发、二进制未下发、显式不支持）优先级更高：那时即便命中本表也要走降级
 * 卡片，因为缺的是内容而不是视图，用户必须看到明确说明（R18）。
 *
 * ## 这份表本身就是「还缺哪些卡片」的清单
 *
 * `UNCOVERED_EVENT_TYPES` 是协议已知、而客户端**尚无**专用呈现的类型。每实现一张卡片，
 * 就从这里划掉一项——它同时是检视点：`presentation-coverage.test.ts` 逐条比对
 * `KNOWN_EVENT_TYPES` 与「覆盖表 ∪ 未覆盖清单」，漏登记一个类型即变红。
 */

/**
 * 呈现通道：一个事件类型在客户端被哪个面承载。
 *
 * - `message_lane`：消息通道。流式增量累加成**易失**正文（`design.md` D6），
 *   `agent.message.completed` 只标记该条消息的增量流结束，`turn.delta_compacted` 作废该轮次的累加；
 *   **正文权威始终是 `session.read`**。
 * - `timeline_card`：对话页时间线上的结构化卡片。
 * - `session_dashboard`：会话摘要/仪表类面（目录行、标题、用量条、来源可达性标注）。
 * - `agent_overlay`：Agent 连接覆盖层（主机面板；列表本体仍来自快照，R19）。
 * - `command_status`：命令记录面（`command.result`/`command.status` 的终态呈现）。
 */
export type PresentationRoute =
  | "message_lane"
  | "timeline_card"
  | "session_dashboard"
  | "agent_overlay"
  | "command_status";

/**
 * 覆盖表：事件类型 → 呈现通道。
 *
 * 纪律：**只有真的实现了对应呈现**的类型才能在这里登记。登记了却没实现（例如把
 * `session.info.changed` 写进来而标题并不随它更新）会让事件被静默吞掉——那就把
 * 「命中覆盖」变成了新的第三种结果。
 */
export const PRESENTATION_COVERAGE: Readonly<Record<string, PresentationRoute>> = {
  // 摘要/仪表面：目录与对话头、上下文条、来源可达性标注。
  "session.created": "session_dashboard",
  "session.updated": "session_dashboard",
  // 本机生命周期事件只改会话主状态的展示依据（摘要里的 `localActivity`），没有独立卡片。
  "session.activity.changed": "session_dashboard",
  "session.usage.changed": "session_dashboard",
  "session.origin.online_changed": "session_dashboard",

  // 时间线卡片：WP1 的 17 类历史条目**逐条**都有卡片，否则重开对话页会看到成片降级卡。
  "turn.queued": "timeline_card",
  "turn.started": "timeline_card",
  "turn.completed": "timeline_card",
  "turn.cancelled": "timeline_card",
  "turn.failed": "timeline_card",
  "tool.call.started": "timeline_card",
  "tool.call.updated": "timeline_card",
  "tool.call.completed": "timeline_card",
  "permission.requested": "timeline_card",
  "permission.resolved": "timeline_card",
  "elicitation.requested": "timeline_card",
  "elicitation.resolved": "timeline_card",
  "terminal.output": "timeline_card",
  "file.changed": "timeline_card",
  "session.plan.changed": "timeline_card",
  "session.mode.changed": "timeline_card",
  "session.config.changed": "timeline_card",

  // 消息通道：增量与完成标记都不承载正文权威，压缩事件作废对应轮次的累加。
  "user.message.delta": "message_lane",
  "agent.message.delta": "message_lane",
  "agent.thought.delta": "message_lane",
  "agent.message.completed": "message_lane",
  "turn.delta_compacted": "message_lane",

  // Agent 连接覆盖层。
  "agent.connected": "agent_overlay",
  "agent.disconnected": "agent_overlay",

  // 命令终态面。
  "command.completed": "command_status",
  "command.failed": "command_status",
  "command.uncertain": "command_status",
};

/**
 * 协议已知、但客户端**尚无**专用呈现的类型：它们一律产出降级呈现（`no_dedicated_view`）。
 *
 * 「降级呈现」落在哪一面取决于事件有没有会话：会话级事件产出时间线降级卡片
 * （`ConversationPageModel.degraded` / `timeline`），**节点级**事件（`sessionId` 为 `null`）
 * 没有会话时间线可挂，落在主机/连接面的节点级降级列表（`ClientStore.nodeDegraded()` →
 * `HostPanelModel.nodeDegraded`）。两面都必须真的到得了——被消费的事件既不能没有专用呈现、
 * 也不能没有**可到达**的降级呈现。
 *
 * - `session.info.changed`：标题目前只随 `session.updated` 的摘要更新，没有独立的标题变更呈现。
 * - `session.commands.changed`：斜杠命令清单还没有呈现面。
 * - `device.revoked`：**节点级**事件，因此不产出时间线降级卡片，而落在节点级降级列表；吊销
 *   导致的连接关闭本身另由连接状态机与主机面板呈现（那是另一条事实）。
 *
 * 每项都是**已知缺口**，不是「不支持」——三者都不是「客户端/服务端/Agent 能力不支持」，
 * 因此降级卡片 MUST NOT 把它们归到三类不支持里（卡片文案区分见 `degradation-model.ts`）。
 */
export const UNCOVERED_EVENT_TYPES: readonly string[] = [
  "session.info.changed",
  "session.commands.changed",
  "device.revoked",
];

/** 某个事件类型在客户端的呈现通道；`null` 表示没有专用呈现（必须走降级卡片）。 */
export function presentationRouteFor(eventType: string): PresentationRoute | null {
  return PRESENTATION_COVERAGE[eventType] ?? null;
}

/** 客户端是否有该事件类型的专用呈现。这是覆盖判定，**不**看协议词表。 */
export function hasDedicatedView(eventType: string): boolean {
  return presentationRouteFor(eventType) !== null;
}

/** 走时间线卡片的类型清单（WP1 的 17 类历史条目应全部落在这里）。 */
export const TIMELINE_CARD_EVENT_TYPES: readonly string[] = Object.entries(PRESENTATION_COVERAGE)
  .filter(([, route]) => route === "timeline_card")
  .map(([eventType]) => eventType);
