/**
 * Expo Router 根布局：挂载共享的展示组件树与客户端 store。
 *
 * 组合根在这里**装配一次**：`src/composition.ts` 调用 `openPlatform()`、实现 WP6 登记但
 * `PlatformPorts` 缺失的五个端口（`TranscriptCodec` / `HostIdentityPort` / `DigestPort` /
 * `RandomPort` / `ClockPort`）与 `PairingTransport` 的 HTTPS 面，并把 `SyncClient` 的事件流
 * 接到 `ClientStore`。
 *
 * ## 为什么组合根在模块作用域建立
 *
 * store 必须**先于**首帧存在：`ClientStoreProvider` 的 `store={null}` 会让每个路由渲染
 * `RuntimeUnavailable`，页面在真实运行时一条都跑不到。装配是纯构造（`openPlatform()` 不做
 * IO），因此模块求值期安全；真正需要 IO 的 `start()`（读 IndexedDB 里的设备身份与配对记录）
 * 在组件挂载后触发。
 *
 * `store` 在设备身份或配对记录缺失时**不是** `null`，而是状态机停在 `unpaired` 的一个真实
 * store：页面据此呈现「尚未配对」，而不是伪造空目录（那会与 R15 的第一种空态混淆）。
 *
 * ## 路由形态
 *
 * 下方的 `Stack` 是 Expo Router 默认的 Web 链接：路由按文件约定走**正常路径**
 * （`/`、`/pair`、`/hosts`、`/dir/<directoryId>`、`/session/<id>`），地址里的片段不参与路由命中，
 * 只承载配对二维码的内容（design D4）。核实方式与产物证据见 `src/routing.test.ts` 的文件头。
 */

import { Stack } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { Shell } from "../src/components/shell/Shell";
import { createComposition } from "../src/composition";
import type { Composition } from "../src/composition";
import { ClientStoreProvider } from "../src/features/runtime";

/** 进程内唯一的组合根：一次装配，多个路由共享同一个 store。 */
const composition: Composition = createComposition();

export default function RootLayout(): ReactElement {
  const [runtime, setRuntime] = useState<Composition | null>(null);

  useEffect(() => {
    let cancelled = false;
    // `start()` 失败（IndexedDB 不可用、非安全上下文）时保持 `runtime === null`：
    // 页面据此呈现运行时不可用，而不是把一条没有身份的空 store 当成正常状态。
    void composition.start().catch(() => undefined).then(() => {
      if (!cancelled) setRuntime(composition);
    });
    return () => {
      cancelled = true;
      composition.dispose();
    };
  }, []);

  return (
    <ClientStoreProvider store={runtime?.store ?? null}>
      {/* 共享外壳（design D1）：一份外壳装在这里，「开发模式横幅 → 顶栏 → 路由内容 →
          抽屉/遮罩/提示」的装配顺序只有这一处；路由不各自拼导航。 */}
      <Shell>
        <Stack screenOptions={{ headerShown: false }} />
      </Shell>
    </ClientStoreProvider>
  );
}