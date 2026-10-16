/**
 * 会话对话页（路由 `/session/<id>`）。
 *
 * ## 页面只做四件事
 *
 * 取模型（`store.conversationPage`）、取页面级的事实（连接状态、上次同步时刻、Agent 覆盖层）、
 * 把交互上抛（每条交互对应一条既有命令）、渲染组件。这里**没有**业务判定：发送能不能点、授权
 * 能不能答、模型能不能切、变更文件有没有统计、左栏列哪些会话，全部由 `ConversationPageModel`
 * 里的判别值决定（`src/features/**` 与 `src/domain/**` 的纯函数）。
 *
 * ## 没有自建导航，也没有第二个连接状态徽标
 *
 * 顶栏（标识、设备胶囊、主题开关、帮助入口）由根布局装配的**共享外壳**承担，连接状态只由那一个
 * 设备胶囊表达。此前本页另拼了一条 `<main><nav>` 导航，与前几页的自建导航一样是「路由自行拼出
 * 的重复表达」——同一状态有两处判据，必然漂移，因此整条移除。本页的返回入口由浏览器/路由栈与
 * 左栏的「全部 ›」承担。
 *
 * ## 次级视图住在仓库里
 *
 * 窄屏的次级视图（`all | timeline | sessions | files`）是**单份** UI 状态：它跨路由存活，读写都在
 * `ClientStore`（`state.conversationSecondaryView` / `setConversationSecondaryView`），本页不另持
 * 一份 `useState`——否则同一时刻会有两个「当前视图」。
 *
 * ## 复制与链接在这里接线
 *
 * `Markdown` / `CodeBlock` 只把目标上抛（它们不写 `href`、不碰剪贴板），安全口径由页面决定：
 * 复制走 `navigator.clipboard`（失败时明确提示，不假装成功）；链接只放开 `http(s)` 并交给浏览器
 * 新标签打开（Agent 产出的正文是**不可信输入**，其余 scheme 一律不打开）。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { ConversationStream, CreateSessionSheet, DiagnosticsDrawer, RuntimeUnavailable } from "../../src/components";
import { toastStore } from "../../src/components/shell/toast";
import type { Palette } from "../../src/components/theme";
import { StyleSheet, View, palettes, useTheme } from "../../src/components/theme";
import { useClientState, useClientStore } from "../../src/features/runtime";

/** 页面级布局：浅/深各建一次（与其余路由同一形态）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      flexGrow: 1,
      backgroundColor: colors.bg,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function SessionRoute(): ReactElement {
  const { id } = useLocalSearchParams<{ id: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();
  const sessionId = typeof id === "string" ? id : null;
  // 创建弹层里选中的 Agent 是页面局部状态（不跨路由存活，与弹层的开关无关）。
  const [agentId, setAgentId] = useState<string | null>(null);
  // 「哪一处改动的 ACP 原文是展开的」是**页面级视图状态**：它不改变任何数据判定、也不需要
  // 跨页存活，因此留在这里（次级视图那一个是跨路由的 UI 偏好，因此住在仓库里）。
  const [openChangeDiagnostics, setOpenChangeDiagnostics] = useState<string | null>(null);
  // 折叠块的显式展开状态（键为条目键）与「已选中的那一处改动」同属页面级视图状态：默认展开值由
  // 条目状态推出（`ConversationTimeline` 的判定），页面只记用户**显式**给出的那一份覆盖值；
  // 选中项让时间线上带文件标识的条目与右栏改动清单共用同一个文件（D10）。
  const [expandedTimelineEntries, setExpandedTimelineEntries] = useState<Readonly<Record<string, boolean>>>({});
  const [selectedChangeId, setSelectedChangeId] = useState<string | null>(null);

  // 明细现取：只在 store 就绪且还没有线程时发一次（store 内部有在途请求的去重）。
  useEffect(() => {
    if (store === null || sessionId === null) return;
    store.loadSession(sessionId);
  }, [store, sessionId]);

  if (store === null || state === null || sessionId === null) return <RuntimeUnavailable route="session" />;

  const model = store.conversationPage(sessionId);
  if (model === null) {
    return (
      <View role="main" style={theme.dark ? STYLES.dark.page : STYLES.light.page}>
        <section data-route="session" data-session-id={sessionId} data-session-state="unknown">
          <p role="alert">最近一次同步的快照里没有这个会话，它可能已被删除，或本设备还没有同步到它。</p>
        </section>
      </View>
    );
  }

  /**
   * 要呈现的创建弹层：`null` 表示本页不呈现。
   *
   * 唯一来源是 store 的 `createSheetAlias`（本页不另持开关）；本页只认**本会话所属目录**那一个
   * ——左栏的「⊕ 新建会话」就是在这个目录里创建，因此弹层与列表是同一个目录上下文。再要求该别名
   * 落在本机已登记的目录里（别名全集取自快照）：否则左栏的目录引用已经失效时，弹层会画在一个
   * 不存在的目录上。
   */
  const requestedSheetAlias = state.createSheetAlias;
  const directoryAlias = model.header.directoryAlias;
  const sheetAlias =
    directoryAlias !== null &&
    requestedSheetAlias === directoryAlias &&
    (state.resources?.workspaces.some((workspace) => workspace.alias === requestedSheetAlias) ?? false)
      ? requestedSheetAlias
      : null;

  return (
    <View role="main" style={theme.dark ? STYLES.dark.page : STYLES.light.page}>
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
        // 右栏每处改动的 ACP 原文诊断：展开态是页面级视图状态（按 `eventId`）。
        onOpenChangeDiagnostics={setOpenChangeDiagnostics}
        openChangeDiagnosticsEventId={openChangeDiagnostics}
        // 折叠块的展开状态：只记显式覆盖值（按条目键），默认值由条目状态在时间线那一侧推出。
        expandedTimelineEntries={expandedTimelineEntries}
        onToggleTimelineEntry={(key, expanded) => {
          setExpandedTimelineEntries((current) => ({ ...current, [key]: expanded }));
        }}
        // 选中项：时间线条目与右栏清单共用同一份（从条目点进来时右栏已经选中同一个文件）。
        selectedChangeId={selectedChangeId}
        onSelectChange={setSelectedChangeId}
        // 左栏：目录上下文、同目录会话、目录内搜索（只影响左栏）、继续加载与目录内新建。
        onOpenDirectory={(alias) => {
          router.push(`/dir/${encodeURIComponent(alias)}`);
        }}
        onOpenSession={(otherSessionId) => {
          router.push(`/session/${encodeURIComponent(otherSessionId)}`);
        }}
        onSidebarQueryChange={(query) => {
          store.setConversationSidebarQuery(query);
        }}
        onLoadMoreSidebar={() => {
          store.loadMoreSidebarSessions();
        }}
        onCreateSession={(alias) => {
          store.openCreateSheet(alias);
        }}
        // 会话头的 Agent 胶囊沿用主机面板的同一份连接事件覆盖层（覆盖层缺席即「状态未知」）。
        hostPanel={store.hostPanel()}
        connectionState={state.connection.state}
        lastSyncedAtMs={state.lastSyncedAtMs}
        // 复制：走剪贴板 API，结果用全局提示说明（失败不假装成功）。
        onCopyCode={(code) => {
          const clipboard: Clipboard | undefined = globalThis.navigator?.clipboard;
          if (clipboard === undefined) {
            toastStore.show("复制失败：当前环境不提供剪贴板");
            return;
          }
          void clipboard.writeText(code).then(
            () => {
              toastStore.show("代码已复制");
            },
            () => {
              toastStore.show("复制失败：浏览器不允许访问剪贴板");
            },
          );
        }}
        // 链接：只放开 http(s)，交给浏览器新标签打开（正文是不可信输入，其余 scheme 不打开）。
        onLinkPress={(href) => {
          let protocol: string | null = null;
          try {
            protocol = new URL(href).protocol;
          } catch {
            protocol = null;
          }
          if (protocol !== "http:" && protocol !== "https:") {
            toastStore.show("这个链接的协议不受支持，未打开");
            return;
          }
          if (globalThis.open(href, "_blank", "noopener,noreferrer") === null) {
            toastStore.show("浏览器拦下了新标签，链接未打开");
          }
        }}
        secondaryView={state.conversationSecondaryView}
        onSecondaryViewChange={(view) => {
          store.setConversationSecondaryView(view);
        }}
      />

      {/* 左栏底部的「⊕ 新建会话」落在这里：弹层的开关是 store 的**单份**状态
          （`createSheetAlias`），只有它落在本会话所属目录上时才呈现——目录页与详情页呈现的
          也是它。提交载荷原样上抛（目录里的「先建目录」那类附加键因此不必在本页重复一遍）。 */}
      {sheetAlias === null ? null : (
        <CreateSessionSheet
          model={store.createEntry(sheetAlias)}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onSubmit={(input) => {
            // 目录别名由本页给出（页面用的那个目录上下文），载荷里的其余键原样上抛。
            const outcome = store.submitCreateSession({ ...input, directoryAlias: sheetAlias });
            if (!outcome.ok) setAgentId(null);
          }}
          onDismissUncertain={() => {
            const requestId = state.createRequestIds.get(sheetAlias);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            store.closeCreateSheet();
          }}
        />
      )}

      <DiagnosticsDrawer
        model={store.diagnostics()}
        onClose={() => {
          store.closeDiagnostics();
        }}
      />
    </View>
  );
}
