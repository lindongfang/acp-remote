/**
 * 共享外壳的**容器**（`design.md` D1）：全部路由共用一份，装在根布局里。
 *
 * ## 一份外壳，一处判据
 *
 * 顶栏是否呈现由 `usePathname()` 决定（配对入口呈现自己的头部），连接状态只由顶栏里的设备胶囊
 * 表达——这两条只在 `shell-model.ts` 里判定一次，路由文件不再各自拼导航。
 *
 * ## 容器只做接线
 *
 * 这里只读三样东西（路由、连接状态、环境事实）并把三类交互写回：
 *
 * - 主题开关 / 抽屉里的三态控件 → `themePreferenceStore.set()`（持久化在 `theme.ts` 的 UI 偏好槽位）；
 * - 提示 → `toastStore.show()`（模块级订阅源，见 `./toast.ts`）；
 * - 胶囊 → `router.push("/hosts")`（主机与连接面板）。
 *
 * ## 首屏认证闸门与连接问题
 *
 * 业务路由（`/`、`/dir/...`、`/session/...`）在本次会话尚未认证时只呈现 `AuthTransition`：
 * 不画顶栏、不挂业务内容。认证失败复用 `features/connection-problem.ts` 的同一份模型与同一
 * 个弹层组件；已认证后的连接问题则盖在内容区之上（顶栏保持可见，方便进主机页排查）。
 *
 * 呈现全部交给 `ShellView`（`./ShellView.tsx`）：它只吃模型与回调，因此本文件不参与组件测试，
 * 也不把 `expo-router` 拖进 `vitest` 的 node 环境。
 */

import { usePathname, useRouter } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement, ReactNode } from "react";

import type { ClientState } from "../../features/client-store";
import { buildConnectionProblem, shouldShowConnectionProblem } from "../../features/connection-problem";
import type { ConnectionProblemModel } from "../../features/connection-problem";
import { useClientState, useClientStore } from "../../features/runtime";
import { AuthTransition } from "../AuthTransition";
import type { ThemePreference } from "../theme";
import { THEME_PREFERENCE_LABELS, themePreferenceStore, useTheme, useThemePreference } from "../theme";
import { useReducedMotion } from "./reduced-motion";
import { buildShellModel, isBusinessPath, isPairingPath } from "./shell-model";
import { ShellView } from "./ShellView";
import { toastStore, useToastMessage } from "./toast";

export interface ShellProps {
  readonly children?: ReactNode;
}

/**
 * 外壳容器：读 `usePathname()` 与连接状态，把主题/提示/导航写回。
 *
 * **不接受业务 props**（`plan.md` 的冻结接口）：模型全部由这里的输入推出。
 */
export function Shell({ children }: ShellProps): ReactElement {
  const pathname = usePathname();
  const state: ClientState | null = useClientState();
  const store = useClientStore();
  const themePreference = useThemePreference();
  const theme = useTheme();
  const reducedMotion = useReducedMotion();
  const toastMessage = useToastMessage();
  const router = useRouter();
  const [helpOpen, setHelpOpen] = useState(false);

  // 等组合根读完本地记录再判断；保留设备与节点两种配对入口，避免循环跳转。
  useEffect(() => {
    if (state !== null && state.host === null && !isPairingPath(pathname)) {
      router.replace("/pair");
    }
  }, [state?.host, state === null, pathname, router]);

  // 连接问题的文案/动作只有一份（`features/connection-problem.ts`）。首屏尚未认证时由
  // `AuthTransition` 呈现同一个弹层；已认证后只在未被排除的业务路由上盖在内容区之上。
  const problem: ConnectionProblemModel | null =
    state === null
      ? null
      : buildConnectionProblem({
          connection: state.connection.state,
          hasAuthenticated: state.hasAuthenticated,
          lastClose: state.lastClose,
          reconnect: state.reconnect,
          nowMs: store?.nowMs() ?? Date.now(),
        });
  const authGateOpen = state !== null && !state.hasAuthenticated && isBusinessPath(pathname);
  const overlayProblem =
    state !== null && state.hasAuthenticated && shouldShowConnectionProblem(pathname) ? problem : null;

  const onRepair = (): void => {
    router.push("/pair");
  };
  const onReload = (): void => {
    if (typeof globalThis.location !== "undefined") globalThis.location.reload();
  };

  // 未认证的业务路由不呈现顶栏与业务内容：认证失败时 `AuthTransition` 复用同一个问题弹层。
  if (authGateOpen) {
    return <AuthTransition problem={problem} onRepair={onRepair} onReload={onReload} />;
  }

  const model = buildShellModel({
    pathname,
    connectionState: state?.connection.state ?? null,
    hostAddress: state?.host?.address ?? null,
    hasAuthenticated: state?.hasAuthenticated ?? false,
    // 环境事实与组合根 `CompositionInputs.secureContext` 读的是**同一个**浏览器全局
    // （`docs/FRONTEND_DESIGN.md` §4.4：无 HTTPS 时 UI 必须明确显示「开发/不安全连接」）。
    // 没有这个全局的运行时（node / 组件测试）按安全处理：那里既没有页面托管，也没有明文连接。
    secureContext: globalThis.isSecureContext !== false,
  });

  const selectTheme = (next: ThemePreference): void => {
    themePreferenceStore.set(next);
    toastStore.show(`主题已切换为「${THEME_PREFERENCE_LABELS[next]}」`);
  };

  return (
    <ShellView
      model={model}
      themePreference={themePreference}
      themeDark={theme.dark}
      reducedMotion={reducedMotion}
      helpOpen={helpOpen}
      toastMessage={toastMessage}
      onToggleTheme={() => {
        selectTheme(theme.dark ? "light" : "dark");
      }}
      onSelectTheme={selectTheme}
      onOpenHelp={() => {
        setHelpOpen(true);
      }}
      onCloseHelp={() => {
        setHelpOpen(false);
      }}
      onOpenHosts={() => {
        router.push("/hosts");
      }}
      problem={overlayProblem}
      onRepair={onRepair}
      onReload={onReload}
    >
      {children}
    </ShellView>
  );
}
