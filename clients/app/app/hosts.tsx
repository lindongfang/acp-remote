/**
 * 主机与连接页（路由 `#/hosts`，`data-route="hosts"`）。
 *
 * 与目录页上的抽屉共用同一个组件与同一份模型：Agent 列表来自快照（R19），
 * 连接状态是可选的事件覆盖层，缺席时呈现「状态未知」而不是「已断开」。
 */

import { useRouter } from "expo-router";
import type { ReactElement } from "react";

import { HostConnectionPanel, RuntimeUnavailable } from "../src/components";
import { useClientState, useClientStore } from "../src/features/runtime";

export default function HostsRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();

  if (store === null || state === null) return <RuntimeUnavailable route="hosts" />;

  return (
    <main>
      <nav>
        <button type="button" data-action="back-to-dirs" onClick={() => router.back()}>
          返回目录
        </button>
      </nav>
      <HostConnectionPanel model={store.hostPanel()} />
    </main>
  );
}
