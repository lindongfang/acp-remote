/**
 * 会话对话页路由（空壳，WP7 实现）。
 *
 * 会话 id 是 wire 上的 `sessionId`；`session.read` 的分页由 `src/domain/` 的
 * `SessionReadPageView` 承载，页面本身由 WP7 实现。
 */

import { useLocalSearchParams } from "expo-router";
import type { ReactElement } from "react";

export default function SessionRoute(): ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <div data-route="session" data-session-id={id} />;
}
