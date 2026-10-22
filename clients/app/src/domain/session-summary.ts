/**
 * 会话摘要视图模型。
 *
 * 输出类型里**没有**协议载荷：没有 `origin.ownerNodeId` 之外的原始嵌套块、没有
 * `currentMode` 的 wire 形状、没有 `DecimalString` 形式的 `version`。组件只消费这里导出的字段。
 *
 * 目录页需要的三项汇总（`state` 派生）与「来源离线」标记都在这一层定形，
 * 因此断连时目录页仍可渲染（数据来自快照）。
 */

import type { LocalActivity, SessionSummary, Timestamp, Uuid } from "../protocol/common";

/** 会话的稳定标识（wire 上的 `sessionId`）。 */
export type SessionId = Uuid;

/**
 * `sessionSummary.state` 在视图层的同名取值域。
 *
 * 三态只表达活跃程度：有效未解决交互 → `pending`；未结束任务（含已接受的排队任务）→
 * `running`；其余 → `idle`。排队细节、等待原因与任务结果由会话内容另行表达，不属于这里。
 */
export type SessionStateView = "idle" | "running" | "pending";

/**
 * 协商 `core.local-session-lifecycle.v1` 后 owned 会话的**本机**活跃程度（视图层同名取值域）。
 *
 * `idle` / `active` / `pending` 只表达资源与交互事实，不是任务结果；任务执行、排队与等待原因
 * 仍由时间线与会话内容独立表达。
 */
export type LocalActivityView = LocalActivity;

/**
 * 会话主状态的**唯一**判定结果。
 *
 * - `local`：协商后的 owned 会话，主标签用本机活跃程度；
 * - `task`：旧合同 / imported 会话，保持任务三态。
 *
 * 两个分支互斥，组件因此无法同时读两套词表；`null`（摘要未取得）由调用方单独表达，
 * MUST NOT 落进任何一支冒充「空闲」。
 */
export type SessionMainActivityView =
  | { readonly kind: "local"; readonly value: LocalActivityView }
  | { readonly kind: "task"; readonly value: SessionStateView };

/**
 * 会话标题：`SessionSummary.title` 是服务端合并后的**有效值**（自定义标题优先 → Agent
 * 标题 → 未命名）。客户端不复制 Agent 标题、不另建优先级；空值呈现为未命名（D1/D7）。
 */
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
  /**
   * 协商后的 owned 会话的主状态来源；`null` = 本机活跃程度不适用（未协商 / imported /
   * 服务端未提供该字段）。此时主状态回落任务三态（`state`）。
   */
  readonly localActivity: LocalActivityView | null;
  /** 由上面两者推出的主状态（`state` 仍是任务事实，不被替换）。 */
  readonly activity: SessionMainActivityView;
  readonly origin: SessionOriginView;
  /** 目录归属：目录标识或 `null`（未登记目录的会话不归入任何目录项）；标识不进界面文本。 */
  readonly directoryId: string | null;
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
  options: { readonly localLifecycleNegotiated?: boolean } = {},
): SessionSummaryView {
  const localActivity = localActivityOf(summary, options.localLifecycleNegotiated === true);
  return {
    id: summary.sessionId,
    title:
      summary.title === null || summary.title.length === 0
        ? { kind: "untitled" }
        : { kind: "titled", text: summary.title },
    agentName: summary.agent.name,
    agentId: summary.agent.agentId,
    state: summary.state,
    localActivity,
    activity: mainActivityOf(summary.state, localActivity),
    origin:
      summary.origin.kind === "local"
        ? { kind: "local" }
        : { kind: "remote", online: summary.origin.online, ownerLabel },
    directoryId: summary.workspace?.directoryId ?? null,
    updatedAt: summary.updatedAt,
    createdAt: summary.createdAt,
  };
}

/**
 * 本机活跃程度的**唯一**可采信判定。
 *
 * 三重门：连接协商了该 feature、摘要是 owned、字段取值在封闭词表内。任一不成立都返回 `null`
 * ——旧 peer 的字段（如果有）与 imported 摘要一律不进入主状态；`null` 时回落任务三态。
 */
export function localActivityOf(
  summary: SessionSummary,
  negotiated: boolean,
): LocalActivityView | null {
  if (!negotiated) return null;
  if (summary.origin.kind !== "local") return null;
  const value = summary.localActivity;
  if (value === "idle" || value === "active" || value === "pending") return value;
  return null;
}

/** 由任务事实与本机活跃程度推出主状态。 */
export function mainActivityOf(
  state: SessionStateView,
  localActivity: LocalActivityView | null,
): SessionMainActivityView {
  return localActivity === null
    ? { kind: "task", value: state }
    : { kind: "local", value: localActivity };
}
