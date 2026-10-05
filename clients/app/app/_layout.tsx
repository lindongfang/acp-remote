/**
 * Expo Router 根布局：挂载共享的展示组件树与客户端 store。
 *
 * ## 组合根的现状（WP7 交付报告的缺口登记）
 *
 * 组合根要做两件事：调用 `openPlatform()` 装配平台端口，再把 WP5a/WP6 交付的端口实现
 * （`TranscriptCodec` / `HostIdentityPort` / `DigestPort` / `RandomPort` / `ClockPort`）
 * 注入 `SyncClient`。`src/layers.ts` 的分层机检禁止**七层中的任何一层** import
 * `src/platform/`（`src/layers.test.ts` 的「没有任何跨层反向 import」把 `PLATFORM_LAYER`
 * 对所有层判为违规），因此组合根只能落在 `src/` 根目录的一个层级中立文件里——
 * 那不在 WP7 的 Write Scope（`clients/app/app/`、`src/features/`、`src/components/`）内。
 *
 * 在它落地之前，`store` 为 `null`，页面渲染 `RuntimeUnavailable`：
 * **不**伪造空目录/空 Agent 列表，那会被读成「这台电脑上确实什么都没有」（R15 第一种空态）。
 */

import { Stack } from "expo-router";
import type { ReactElement } from "react";

import { ClientStoreProvider } from "../src/features/runtime";

export default function RootLayout(): ReactElement {
  return (
    <ClientStoreProvider store={null}>
      <Stack screenOptions={{ headerShown: false }} />
    </ClientStoreProvider>
  );
}
