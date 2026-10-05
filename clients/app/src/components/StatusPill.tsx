/**
 * 会话状态徽标（共享展示原语，WP5a 只建骨架，WP7 接管实现与后续改动）。
 *
 * 消费 `src/domain/` 的 `SessionStateView`，**不**接收任何协议字段。
 */

import type { ReactElement } from "react";

import type { SessionStateView } from "../domain/session-summary";

/** 状态 → 展示文案的静态词表。 */
const LABEL_BY_STATE: Record<SessionStateView, string> = {
  idle: "空闲",
  queued: "排队中",
  running: "运行中",
  waiting_input: "等你回复",
  waiting_permission: "待授权",
  failed: "失败",
  closed: "已关闭",
};

export interface StatusPillProps {
  readonly state: SessionStateView;
}

/**
 * 渲染一个状态徽标。
 *
 * 说明：WP5a 提供结构骨架与词表，具体视觉由 WP7 决定；这里保持零样式依赖，
 * 让 WP7 可以在不改变公共 API 的前提下替换表现。
 */
export function StatusPill({ state }: StatusPillProps): ReactElement {
  return <span data-session-state={state}>{LABEL_BY_STATE[state]}</span>;
}
