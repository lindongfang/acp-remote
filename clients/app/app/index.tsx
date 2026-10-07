/**
 * 目录页（路由 `#/`，`data-route="dirs"`）。
 *
 * 目录是列表的主要组织单位（R15）：本页只做三件事——取模型、渲染共享组件、把点击上抛给路由。
 * 任何数据判定都在 `src/features/directory-model.ts` 里，页面里没有业务分支。
 */

import { useRouter } from "expo-router";
import type { ReactElement } from "react";

import { ConnectionBadge, DirectoryList, HostConnectionPanel, RuntimeUnavailable } from "../src/components";
import { useClientState, useClientStore } from "../src/features/runtime";

export default function DirectoriesRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();

  if (store === null || state === null) return <RuntimeUnavailable route="dirs" />;

  return (
    <main>
      <nav>
        <button type="button" data-action="open-hosts" onClick={() => router.push("/hosts")}>
          主机与连接
        </button>
        <button type="button" data-action="open-pairing" onClick={() => router.push("/pair")}>
          配对
        </button>
        <ConnectionBadge panel={store.hostPanel()} onOpenHosts={() => router.push("/hosts")} />
      </nav>
      <DirectoryList
        model={store.directoryPage()}
        onOpenDirectory={(alias) => {
          router.push(`/dir/${encodeURIComponent(alias)}`);
        }}
      />
      <HostConnectionPanel model={store.hostPanel()} />
    </main>
  );
}
