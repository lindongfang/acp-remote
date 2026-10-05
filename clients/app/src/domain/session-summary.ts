/**
 * 会话摘要视图模型。
 *
 * 输出类型里**没有**协议载荷：没有 `origin.ownerNodeId` 之外的原始嵌套块、没有
 * `currentMode` 的 wire 形状、没有 `DecimalString` 形式的 `version`。组件只消费这里导出的字段。
 *
 * 目录页需要的三项汇总（`state` 派生）与「来源离线」标记都在这一层定形，
 * 因此断连时目录页仍可渲染（数据来自快照）。
 */

import type { SessionSummary, Timestamp, Uuid } from "../protocol/common";

/** 会话的稳定标识（wire 上的 `sessionId`）。 */
export type SessionId = Uuid;

/** `sessionSummary.state` 在视图层的同名取值域。 */
export type SessionStateView =
  | "idle"
  | "queued"
  | "running"
  | "waiting_input"
  | "waiting_permission"
  | "failed"
  | "closed";

/** 会话标题：标题由 Agent 通知单向写入，空标题呈现为未命名（`design.md` D7）。 */
export type SessionTitle = { readonly kind: "titled"; readonly text: string } | { readonly kind: "untitled" };

/** 未命名会话的展示文案（原型 `acp-remote-pwa.html` 的 `noname` 文案）。 */
export const UNTITLED_SESSION_LABEL = "未命名会话";

/**
 * 会话来源的两态。
 *
 * `local` 是本节点自有；`remote` 是 imported，`online` 为假时必须只显示元数据、
 * 不把输入标记为已发送（R20）。视图层因此把两者做成判别联合，
 * 组件无法在 `local` 分支上误读 `online`。
 */
export type SessionOriginView =
  | { readonly kind: "local" }
  | { readonly kind: "remote"; readonly online: boolean; readonly ownerLabel: string };

/** 会话摘要视图模型。 */
export interface SessionSummaryView {
  readonly id: SessionId;
  readonly title: SessionTitle;
  readonly agentName: string;
  readonly agentId: string;
  readonly state: SessionStateView;
  readonly origin: SessionOriginView;
  /** 目录归属：别名或 `null`（未登记目录的会话不归入任何目录项）。 */
  readonly directoryAlias: string | null;
  readonly updatedAt: Timestamp;
  readonly createdAt: Timestamp;
}

/**
 * 把快照/`session.list` 的 `SessionSummary` 投影成视图模型。
 *
 * `ownerLabel` 是 imported 会话的来源展示名；本层不知道节点目录，
 * 因此由调用方传入已解析好的展示名，`buildSessionSummaryView` 只负责形状转换。
 */
export function buildSessionSummaryView(
  summary: SessionSummary,
  ownerLabel: string,
): SessionSummaryView {
  return {
    id: summary.sessionId,
    title:
      summary.title === null || summary.title.length === 0
        ? { kind: "untitled" }
        : { kind: "titled", text: summary.title },
    agentName: summary.agent.name,
    agentId: summary.agent.agentId,
    state: summary.state,
    origin:
      summary.origin.kind === "local"
        ? { kind: "local" }
        : { kind: "remote", online: summary.origin.online, ownerLabel },
    directoryAlias: summary.workspace?.alias ?? null,
    updatedAt: summary.updatedAt,
    createdAt: summary.createdAt,
  };
}
