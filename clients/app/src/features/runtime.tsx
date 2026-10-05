/**
 * feature 层与 React 的接线（`src/features/` 的最后一环）。
 *
 * ## 为什么这里只有一个 Context
 *
 * 页面需要的是「一份可订阅的客户端状态」。`ClientStore` 本身已经是不可变状态 + 订阅源，
 * 因此这里只做一件事：把 store 放进 Context，并让页面用 `useSyncExternalStore` 订阅它。
 * 组件仍然是纯函数（只吃视图模型），因此页面之外没有任何东西需要 React。
 *
 * ## store 为 `null` 的含义
 *
 * `null` 表示**组合根尚未装配**（平台端口没有接线，见交付报告的缺口登记），不是「未连接」。
 * 页面据此渲染一个显式的说明，而不是伪造一份空数据让用户以为「这台电脑上一个目录都没有」——
 * 后者会与 R15 的「本机没有任何目录」空态混淆。
 */

import { createContext, useCallback, useContext, useSyncExternalStore } from "react";
import type { ReactElement, ReactNode } from "react";

import type { ClientState, ClientStore } from "./client-store";

/** 组合根注入的 store；`null` = 尚未装配。 */
const ClientStoreContext = createContext<ClientStore | null>(null);

/** 提供 store。组合根（或测试）在这里挂一次。 */
export function ClientStoreProvider({
  store,
  children,
}: {
  readonly store: ClientStore | null;
  readonly children: ReactNode;
}): ReactElement {
  return <ClientStoreContext.Provider value={store}>{children}</ClientStoreContext.Provider>;
}

/** 取 store 本身（调用命令派发与用户意图的方法）。 */
export function useClientStore(): ClientStore | null {
  return useContext(ClientStoreContext);
}

/**
 * 订阅状态。
 *
 * `subscribe`/`getSnapshot` 都用 `useCallback` 固定成稳定引用：`useSyncExternalStore`
 * 在快照函数每次返回新对象时会判定「一直在变」并死循环，因此快照必须是 `store.state`
 * 这个不可变引用。
 */
export function useClientState(): ClientState | null {
  const store = useContext(ClientStoreContext);
  const subscribe = useCallback(
    (listener: () => void) => (store === null ? () => {} : store.subscribe(listener)),
    [store],
  );
  const snapshot = useCallback(() => store?.state ?? null, [store]);
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
