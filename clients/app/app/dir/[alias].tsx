/**
 * 目录详情路由（空壳，WP7 实现）。
 *
 * 原型路由为 `#/dir/<alias>`；`alias` 是目录别名（不含路径）。
 * TP4 的浏览器用例按 `data-route="dir-detail"` 定位。
 */

import { useLocalSearchParams } from "expo-router";
import type { ReactElement } from "react";

export default function DirectoryDetailRoute(): ReactElement {
  const { alias } = useLocalSearchParams<{ alias: string }>();
  return <div data-route="dir-detail" data-directory-alias={alias} />;
}
