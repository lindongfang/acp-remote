import type { ReactElement, ReactNode } from "react";
import { useRouter } from "expo-router";

import { useClientState, useClientStore } from "../features/runtime";
import { buildConnectionProblem } from "../features/connection-problem";
import { AuthTransition } from "./AuthTransition";
import { isBusinessPath } from "./shell/shell-model";

interface AuthenticatedScreenProps {
  readonly children: ReactNode;
  readonly route: { readonly name: string };
}

/** 路由会直接调用此回调；Hook 必须留在返回的组件元素中。 */
export function renderAuthenticatedScreen(props: AuthenticatedScreenProps): ReactElement {
  return <AuthenticatedScreen {...props} />;
}

/** 只拦截业务页面内容，不能卸载负责完成跳转的导航容器。 */
function AuthenticatedScreen({ children, route }: AuthenticatedScreenProps): ReactElement {
  const state = useClientState();
  const store = useClientStore();
  const router = useRouter();
  const pathname = route.name === "index" ? "/" : `/${route.name}`;
  if (state === null || state.hasAuthenticated || !isBusinessPath(pathname)) return <>{children}</>;
  const problem = buildConnectionProblem({
    connection: state.connection.state,
    hasAuthenticated: state.hasAuthenticated,
    lastClose: state.lastClose,
    reconnect: state.reconnect,
    nowMs: store?.nowMs() ?? Date.now(),
  });
  return <AuthTransition
    problem={problem}
    onRepair={() => router.push("/pair")}
    onReload={() => { if (typeof globalThis.location !== "undefined") globalThis.location.reload(); }}
  />;
}
