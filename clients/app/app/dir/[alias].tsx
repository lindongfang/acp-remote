/**
 * 目录详情页（路由 `#/dir/<alias>`）。
 *
 * `alias` 是**目录别名**，不是路径：路由参数里因此不存在任何可以反推本机路径的片段。
 * 页面负责两件交互——进入会话、打开创建弹层——其余判定都在 feature 层。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useState } from "react";
import type { ReactElement } from "react";

import { ConnectionBadge, CreateSessionSheet, DirectoryDetail, RuntimeUnavailable } from "../../src/components";
import { useClientState, useClientStore } from "../../src/features/runtime";

export default function DirectoryDetailRoute(): ReactElement {
  const { alias } = useLocalSearchParams<{ alias: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const [sheetOpen, setSheetOpen] = useState(false);
  const [agentId, setAgentId] = useState<string | null>(null);

  if (store === null || state === null) return <RuntimeUnavailable route="dir-detail" />;
  if (typeof alias !== "string" || alias.length === 0) return <RuntimeUnavailable route="dir-detail" />;

  const entry = store.createEntry(alias);

  return (
    <main>
      <nav>
        <button type="button" data-action="back-to-dirs" onClick={() => router.back()}>
          返回目录
        </button>
        <ConnectionBadge panel={store.hostPanel()} onOpenHosts={() => router.push("/hosts")} />
      </nav>

      <DirectoryDetail
        model={store.directoryDetail(alias)}
        createEntry={entry}
        canCreate={entry.permission.kind === "granted"}
        onOpenSession={(sessionId) => {
          router.push(`/session/${encodeURIComponent(sessionId)}`);
        }}
        onCreateSession={() => {
          store.openCreateSheet(alias);
          setSheetOpen(true);
        }}
      />

      {sheetOpen ? (
        <CreateSessionSheet
          model={entry}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onSubmit={(selected) => {
            const outcome = store.submitCreateSession({ workspaceAlias: alias, agentId: selected });
            if (!outcome.ok) setAgentId(null);
          }}
          onDismissUncertain={() => {
            const requestId = state.createRequestIds.get(alias);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            store.closeCreateSheet();
            setSheetOpen(false);
          }}
        />
      ) : null}
    </main>
  );
}
