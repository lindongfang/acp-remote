/**
 * Expo Router 根布局（空壳路由，WP7 接管页面实现）。
 *
 * 区域收窄：`plan.md` 的 Shared File Ownership 行把 `clients/app/app/` 同时列给 WP5a 与 WP7；
 * WP5a 只建**空壳路由**，WP7 接管页面实现与后续全部改动。
 */

import { Stack } from "expo-router";
import type { ReactElement } from "react";

export default function RootLayout(): ReactElement {
  return <Stack screenOptions={{ headerShown: false }} />;
}
