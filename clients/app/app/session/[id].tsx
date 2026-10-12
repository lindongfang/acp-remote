/**
 * 会话对话页（路由 `#/session/<id>`）。
 *
 * ## 页面只做三件事
 *
 * 取模型（`store.conversationPage`）、把交互上抛（每条交互对应一条既有命令）、渲染组件。
 * 这里**没有**业务判定：发送能不能点、授权能不能答、模型能不能切、变更文件有没有统计，全部由
 * `ConversationPageModel` 里的判别值决定（`src/features/**` 与 `src/domain/**` 的纯函数），
 * 页面只是把这些值接给组件、把回调接回仓库。
 *
 * ## 两个页面级视图状态
 *
 * `secondaryView`（窄屏的次级视图）与 `openChangeDiagnostics`（哪一处改动的 ACP 原文是展开的）
 * 是**视图状态**：它们不改变任何数据判定、也不需要跨页存活，因此留在这里（与目录页的折叠同理），
 * 而不是新增 store 字段。
 *
 * ## 明细一律现取
 *
 * 进入页面时若本地没有该会话的明细就发一条 `session.read`；两套翻页各自续取自己的游标
 * （消息用 `(createdAt, messageId)`，结构化历史用会话内事件序号）。页面不构造任何载荷。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { ConversationStream, DiagnosticsDrawer, RuntimeUnavailable } from "../../src/components";
import type { ConversationSecondaryView } from "../../src/components";
import { useClientState, useClientStore } from "../../src/features/runtime";

export default function SessionRoute(): ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const sessionId = typeof id === "string" ? id : null;
  const [secondaryView, setSecondaryView] = useState<ConversationSecondaryView | null>(null);
  const [openChangeDiagnostics, setOpenChangeDiagnostics] = useState<string | null>(null);

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
        // 两套翻页：消息一页、结构化历史一页（各自续取自己的游标，互不影响）。
        onLoadEarlier={() => {
          store.loadEarlier(sessionId);
        }}
        onLoadEarlierHistory={() => {
          store.loadEarlierHistory(sessionId);
        }}
        // 降级条目的原文与元数据走既有诊断抽屉（按 `eventId` 打开）。
        onOpenDiagnostics={(eventId) => {
          store.openDiagnostics(eventId);
        }}
        // 草稿只在内存（imported 会话也不落盘），提交时把当前草稿交给仓库。
        onDraftChange={(text) => {
          store.setDraft(sessionId, text);
        }}
        onSubmitPrompt={(draft) => {
          store.submitPrompt(sessionId, draft);
        }}
        onCancelTurn={() => {
          store.cancelTurn(sessionId);
        }}
        // 应答的两支是**两种命令载荷**：授权用 `optionId`，提问用 `action`（表单值不由本客户端构造）。
        onAnswerInteraction={(interactionId, answer) => {
          if (answer.via === "option") {
            store.resolvePermission(sessionId, interactionId, answer.optionId);
            return;
          }
          store.respondToElicitation(sessionId, interactionId, answer.action, null);
        }}
        // `configId` 来自设置模型（配置项 id，不是取值）；`modeId` 来自模式分节。
        onSelectModel={(configId, value) => {
          store.setConfigValue(sessionId, configId, value);
        }}
        onSelectMode={(modeId) => {
          store.setMode(sessionId, modeId);
        }}
        onOpenChangeDiagnostics={setOpenChangeDiagnostics}
        openChangeDiagnosticsEventId={openChangeDiagnostics}
        secondaryView={secondaryView}
        onSecondaryViewChange={setSecondaryView}
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
