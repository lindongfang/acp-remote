/**
 * 会话对话页（路由 `#/session/<id>`）。
 *
 * 明细一律现取（`design.md` D8）：进入页面时若本地没有该会话的明细就发一条 `session.read`，
 * 向上滚动按 `(createdAt, messageId)` 复合游标续取（`src/features/client-store.ts` 的
 * `loadSession` / `loadEarlier`）。页面本身不构造任何载荷。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect } from "react";
import type { ReactElement } from "react";

import { ConversationStream, DiagnosticsDrawer, RuntimeUnavailable } from "../../src/components";
import { useClientState, useClientStore } from "../../src/features/runtime";

export default function SessionRoute(): ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const sessionId = typeof id === "string" ? id : null;

  // 明细现取：只在 store 就绪且还没有线程时发一次（store 内部有在途请求的去重）。
  useEffect(() => {
    if (store === null || sessionId === null) return;
    store.loadSession(sessionId);
  }, [store, sessionId]);

  if (store === null || state === null || sessionId === null) return <RuntimeUnavailable route="session" />;

  const model = store.conversationPage(sessionId);
  if (model === null) {
    return (
      <section data-route="session" data-session-id={sessionId} data-session-state="unknown">
        <p role="alert">最近一次同步的快照里没有这个会话，它可能已被删除，或本设备还没有同步到它。</p>
      </section>
    );
  }

  return (
    <main>
      <nav>
        <button type="button" data-action="back" onClick={() => router.back()}>
          返回
        </button>
      </nav>
      <ConversationStream
        model={model}
        onLoadEarlier={() => {
          store.loadEarlier(sessionId);
        }}
        onOpenDiagnostics={(eventId) => {
          store.openDiagnostics(eventId);
        }}
      />
      <DiagnosticsDrawer
        model={store.diagnostics()}
        onClose={() => {
          store.closeDiagnostics();
        }}
      />
    </main>
  );
}
