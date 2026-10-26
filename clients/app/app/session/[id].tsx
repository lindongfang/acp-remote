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
 * 左栏的「‹ 目录」承担。
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
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import type { ReactElement } from "react";

import {
  ConversationStream,
  CreateSessionSheet,
  DiagnosticsDrawer,
  RuntimeUnavailable,
} from "../../src/components";
import type { ConversationSecondaryView, FilePreviewArtifact } from "../../src/components";
import { toastStore } from "../../src/components/shell/toast";
import type { Palette } from "../../src/components/theme";
import {
  StyleSheet,
  View,
  breakpoints,
  palettes,
  useTheme,
  useWindowDimensions,
} from "../../src/components/theme";
import type { FileSource } from "../../src/features";
import { currentSessionFileDownloader } from "../../src/features/session-file-download";
import { useClientState, useClientStore } from "../../src/features/runtime";

/** 页面级布局：浅/深各建一次（与其余路由同一形态）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      // 对话路由由外壳按视口高度固定：页面容器只负责吃掉剩余高度，滚动交给三栏自己的容器。
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      flexDirection: "column",
      backgroundColor: colors.bg,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function SessionRoute(): ReactElement {
  const { id, diagnostics } = useLocalSearchParams<{ id: string; diagnostics?: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();
  const windowWidth = useWindowDimensions().width;
  const narrow = windowWidth < breakpoints.wide;
  const sessionId = typeof id === "string" ? id : null;
  /**
   * 窄屏从文件列表打开文件后要返回的次级视图。宽屏不用它（三栏不切换）；
   * 会话切换时清空，避免把上一个会话的返回位置带到新会话。
   */
  const [fileReturnView, setFileReturnView] = useState<ConversationSecondaryView | null>(null);
  /**
   * 主动下载与图片预览的服务：组合根装配一次，页面只拿它的能力面。
   * 未装配时为 `null`，对应按钮显式禁用（不伪造可下载状态）。
   */
  const downloads = currentSessionFileDownloader();
  const downloadState = useSyncExternalStore(
    useCallback((listener: () => void) => downloads?.subscribe(listener) ?? (() => undefined), [downloads]),
    useCallback(() => downloads?.state ?? null, [downloads]),
    useCallback(() => downloads?.state ?? null, [downloads]),
  );
  const createPreviewUrl = useCallback(
    (bytes: Uint8Array, mimeType: string): FilePreviewArtifact | null =>
      downloads?.previewUrl(bytes, mimeType) ?? null,
    [downloads],
  );
  /**
   * 一条待保存的消息附件：先让仓库按已认证的附件连接读取并生成 Blob URL，再交给浏览器保存。
   * 页面只触发交互，不自己发网络请求。
   */
  const pendingAttachmentDownload = useRef<{ readonly attachmentId: string; readonly fileName: string } | null>(
    null,
  );
  useEffect(() => {
    const pending = pendingAttachmentDownload.current;
    if (pending === null || store === null || state === null) return;
    const media = state.attachmentMedia.get(pending.attachmentId);
    if (media === undefined || media.kind === "loading") return;
    pendingAttachmentDownload.current = null;
    if (media.kind !== "ready") {
      toastStore.show(`下载失败：${media.detail}`);
      return;
    }
    if (typeof document === "undefined") {
      store.releaseAttachmentMedia(pending.attachmentId);
      return;
    }
    const anchor = document.createElement("a");
    anchor.href = media.url;
    anchor.download = pending.fileName;
    anchor.rel = "noopener";
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    // Blob URL 由仓库持有：保存点完立即撤销会取消下载，留出浏览器取走字节的时间再回收。
    const attachmentId = pending.attachmentId;
    setTimeout(() => {
      store.releaseAttachmentMedia(attachmentId);
    }, 1000);
  }, [state, store]);
  // 离开会话（切换或卸载）时回收本会话的全部对象 URL：不留永久公开地址。
  useEffect(() => {
    if (store === null || sessionId === null) return;
    return () => {
      store.releaseSessionMedia(sessionId);
    };
  }, [store, sessionId]);
  // 创建弹层里选中的 Agent 是页面局部状态（不跨路由存活，与弹层的开关无关）。
  const [agentId, setAgentId] = useState<string | null>(null);
  // 本地闸门拒绝提交的原因（未连接/Agent 不在目录/目录引用缺失）。这类拒绝不产生命令记录，
  // 必须由页面带回弹层，否则点击按钮看起来毫无反应。
  const [createIssue, setCreateIssue] = useState<string | null>(null);
  // 「哪一处改动的 ACP 原文是展开的」是**页面级视图状态**：它不改变任何数据判定、也不需要
  // 跨页存活，因此留在这里（次级视图那一个是跨路由的 UI 偏好，因此住在仓库里）。
  const [openChangeDiagnostics, setOpenChangeDiagnostics] = useState<string | null>(null);
  // 折叠块的显式展开状态（键为条目键）属于页面级视图状态：默认展开值由条目状态推出
  // （`ConversationTimeline` 的判定），页面只记用户**显式**给出的那一份覆盖值。
  const [expandedTimelineEntries, setExpandedTimelineEntries] = useState<Readonly<Record<string, boolean>>>({});

  // 明细现取：只在 store 就绪且还没有线程时发一次（store 内部有在途请求的去重）。
  // 同时把「当前对话页属于哪个会话」告诉仓库：路由切换时旧会话的文件读取会被丢弃，
  // 迟到的响应不能重新打开文件（spec「切换文件时旧响应迟到」）。
  useEffect(() => {
    setFileReturnView(null);
    if (store === null || sessionId === null) return;
    store.setActiveConversationSession(sessionId);
    // 先按本标签页的非敏感定位找回唯一准备槽位（A→B→刷新 B 时也要能新建回 A）。
    void store.restoreTabPreparation();
    // 本标签页有合法准备时按准备页呈现；没有则按正式会话处理（未知 URL 只在权威不存在后回目录）。
    // 正式读取**只**在没有准备时开始；受理/`committed` 之后的读取由仓库的转换副作用发起
    // （`#enterFormalSession`），挂载本身不能替准备页伪造一个线程条目。
    let cancelled = false;
    void store.openPreparationSession(sessionId).then((hasPreparation) => {
      if (cancelled || hasPreparation) return;
      store.loadSession(sessionId);
    });
    // 离开本会话（切换/返回目录/卸载）时先尝试提交原会话的标题编辑，再放弃未保存输入；
    // 已发出的命令按原 requestId 继续归属，迟到结果不会写进新会话。
    return () => {
      cancelled = true;
      store.leaveConversationSession(sessionId);
    };
  }, [store, sessionId]);

  /**
   * 陌生 URL 回目录：只有**权威结论**是「不存在」才 `replace`，不留失效历史条目。
   *
   * 「存在」的判据必须是权威读取页（`threads.page`），不能用 `conversationPage`——读取派发本身
   * 就会建立线程条目，用它会永远认为会话存在，回目录分支永远不可达。
   *
   * 必须排除的误判来源：连接不在 online（离线/认证未完成）、快照尚未就绪、本标签页的准备
   * 仍在恢复或是准备页、会话其实在快照里、以及读取尚未给出权威失败结论。
   */
  useEffect(() => {
    if (store === null || state === null || sessionId === null) return;
    if (state.connection.state !== "online" || state.resources === null) return;
    if (store.preparationRestoring(sessionId) || store.preparationPage(sessionId) !== null) return;
    const thread = state.threads.get(sessionId);
    if (thread?.page != null) return;
    // 首屏读取尚未返回：等它的权威失败（或成功页）再判定，不能按「还没到」跳走。
    // 只有 `session.not_found` 才是「权威不存在」；服务失败/离线只显示真实状态。
    if (
      thread?.pagingError == null ||
      !thread.pagingError.startsWith("session.not_found")
    ) {
      return;
    }
    if (state.resources.sessions.some((summary) => summary.sessionId === sessionId)) return;
    router.replace("/");
  }, [store, state, sessionId, router]);

  /**
   * 旧准备材料整体失效（`preparation.store_rebuilt`）：仓库已丢弃 receipt，页面回目录重新开始。
   * 不在渲染期导航，也不伪造一个可继续点击的准备页。
   */
  useEffect(() => {
    if (store === null || sessionId === null) return;
    const preparation = store.preparationPage(sessionId);
    if (preparation === null || !preparation.restartRequired) return;
    store.leavePreparation(sessionId);
    router.replace("/");
  }, [store, state, sessionId, router]);

  if (store === null || state === null || sessionId === null) return <RuntimeUnavailable route="session" />;

  const model = store.conversationPage(sessionId);
  const preparation = store.preparationPage(sessionId);
  const beforeFirstMessage = preparation !== null && preparation.phase !== "accepted" && preparation.phase !== "committed";
  const thread = state.threads.get(sessionId);
  const remote = state.remoteSessions.get(sessionId);
  // 摘要先于正文到达时，不展示尚未就绪的会话头、分页与配置占位文案。
  // 首次读取失败仍交给正常页面呈现错误，不能一直留在空白等待态。
  if (preparation === null && model !== null && thread?.page == null && thread?.pagingError == null && remote?.error == null &&
      remote?.sourceState !== "offline" && remote?.sourceState !== "revoked") {
    return <View style={theme.dark ? STYLES.dark.page : STYLES.light.page} />;
  }
  if (model === null) {
    if (state.resources === null) {
      return <View style={theme.dark ? STYLES.dark.page : STYLES.light.page} />;
    }
    return (
      <View role="main" style={theme.dark ? STYLES.dark.page : STYLES.light.page}>
        <section data-route="session" data-session-id={sessionId} data-session-state="unknown">
          <p role="alert">最近一次同步的快照里没有这个会话，它可能已被删除，或本设备还没有同步到它。</p>
        </section>
      </View>
    );
  }

  const filesPanel = store.sessionFilesPage(sessionId);
  /**
   * 打开文件：宽屏只替换中栏；窄屏把次级视图切到「对话」（查看区在中栏），并记下原来那一档，
   * 关闭后返回原文件列表（展开目录与滚动位置由右栏自身的 DOM 保留）。
   */
  const openFile = (source: FileSource, path: string): void => {
    const current = store.sessionFilesPage(sessionId).content;
    if (narrow && current.kind !== "file" && state.conversationSecondaryView !== "timeline") {
      setFileReturnView(state.conversationSecondaryView);
    }
    store.openFileContent(sessionId, source, path);
    if (narrow) store.setConversationSecondaryView("timeline");
  };

  /** 关闭文件：恢复对话（DOM 未卸载，滚动锚点/草稿/展开状态都在），并归还焦点。 */
  const closeFile = (): void => {
    const current = store.sessionFilesPage(sessionId).content;
    const context = current.kind === "file" ? current.context : null;
    store.closeFileContent();
    if (fileReturnView !== null) {
      if (narrow) store.setConversationSecondaryView(fileReturnView === "all" ? "timeline" : fileReturnView);
      setFileReturnView(null);
    }
    if (context !== null && typeof document !== "undefined") {
      const wanted = `${context.source}:${context.path}`;
      setTimeout(() => {
        for (const row of document.querySelectorAll<HTMLElement>("[data-file-row]")) {
          if (row.dataset["fileRow"] === wanted) {
            row.focus();
            break;
          }
        }
      }, 0);
    }
  };

  /** 过时/失败后重新读取同一个文件（不改变来源与路径，也不加页签）。 */
  const retryFile = (): void => {
    const current = store.sessionFilesPage(sessionId).content;
    if (current.kind !== "file") return;
    store.openFileContent(sessionId, current.context.source, current.context.path);
  };

  /**
   * 要呈现的创建弹层：`null` 表示本页不呈现。
   *
   * 唯一来源是 store 的 `createSheetDirectoryId`（本页不另持开关）；本页只认**本会话所属目录**那一个
   * ——左栏的「⊕ 新建会话」就是在这个目录里创建，因此弹层与列表是同一个目录上下文。再要求该标识
   * 落在本机已记录的目录里（标识全集取自快照）：否则左栏的目录引用已经失效时，弹层会画在一个
   * 不存在的目录上。
   */
  const requestedSheetDirectoryId = state.createSheetDirectoryId;
  const selectedPath = state.directoryBrowse.selection;
  const pathSheetOpen = state.createSheetBrowses && selectedPath?.kind === "path" &&
    selectedPath.path === (preparation?.workspace.path ?? model.sidebar.directory?.path);
  const directoryId = model.header.directoryId;
  const sheetDirectoryId =
    directoryId !== null &&
    requestedSheetDirectoryId === directoryId &&
    (state.resources?.workspaces.some((workspace) => workspace.directoryId === requestedSheetDirectoryId) ?? false)
      ? requestedSheetDirectoryId
      : null;

  /**
   * 普通新建入口的统一前置：已有唯一准备槽位（或分配在途）时只导航回它。
   *
   * 点当前临时会话本身不是导航（不打断正在进行的编辑）。
   */
  const openExistingPreparation = (): boolean => {
    const route = store.preparationSlotRoute();
    if (route.kind === "session") {
      if (route.sessionId !== sessionId) {
        store.leaveConversationSession(sessionId);
        router.push(`/session/${encodeURIComponent(route.sessionId)}`);
      }
      return true;
    }
    if (route.kind === "allocating") {
      void route.outcome.then((outcome) => {
        // 这一支没有打开弹层，失败原因只能走页面外提示（否则点击看起来毫无反应）。
        if (!outcome.ok) {
          toastStore.show(outcome.reason);
          return;
        }
        setCreateIssue(null);
        if (outcome.sessionId !== sessionId) router.push(`/session/${encodeURIComponent(outcome.sessionId)}`);
      });
      return true;
    }
    return false;
  };

  return (
    <View role="main" style={theme.dark ? STYLES.dark.page : STYLES.light.page}>
      <ConversationStream
        diagnosticsMode={diagnostics === "1"}
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
        // 顶部标题：双击进入原地编辑；Enter/失焦提交、Escape 取消；判定与草稿都在仓库里。
        titleEditor={store.titleEditorView(sessionId)}
        onStartTitleEdit={() => {
          store.beginTitleEdit(sessionId);
        }}
        onTitleDraftChange={(draft) => {
          store.setTitleDraft(sessionId, draft);
        }}
        onCommitTitle={(reason) => {
          store.commitTitleEdit(sessionId, reason);
        }}
        onCancelTitleEdit={() => {
          store.cancelTitleEdit(sessionId);
        }}
        // 草稿只在内存（imported 会话也不落盘），提交时把当前草稿交给仓库。
        onDraftChange={(text) => {
          if (beforeFirstMessage) store.setPreparationDraft(sessionId, text);
          else store.setDraft(sessionId, text);
        }}
        onSubmitPrompt={(draft) => {
          if (beforeFirstMessage) {
            const outcome = store.submitPreparation(sessionId, draft);
            if (!outcome.ok) toastStore.show(outcome.reason);
          } else store.submitPrompt(sessionId, draft);
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
          const outcome = beforeFirstMessage ? store.setPreparationConfigValue(sessionId, configId, value) : store.setConfigValue(sessionId, configId, value);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        onSelectMode={(modeId) => {
          const outcome = beforeFirstMessage ? store.selectPreparationMode(sessionId, modeId) : store.setMode(sessionId, modeId);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        // 右栏每处改动的 ACP 原文诊断：展开态是页面级视图状态（按 `eventId`），
        // 入口在对话时间线的文件改动卡片里，原文按不可信文本就地呈现。
        onOpenChangeDiagnostics={(eventId) => {
          setOpenChangeDiagnostics(eventId);
        }}
        openChangeDiagnosticsEventId={openChangeDiagnostics}
        // 折叠块的展开状态：只记显式覆盖值（按条目键），默认值由条目状态在时间线那一侧推出。
        expandedTimelineEntries={expandedTimelineEntries}
        onToggleTimelineEntry={(key, expanded) => {
          setExpandedTimelineEntries((current) => ({ ...current, [key]: expanded }));
        }}
        // 文件栏两来源与中间只读查看：判定全在 store，页面只接线与处理窄屏返回。
        filesPanel={filesPanel}
        onSelectFileSource={(source) => {
          store.selectFileSource(sessionId, source);
        }}
        onRefreshFiles={() => {
          store.refreshFiles(sessionId);
        }}
        onExpandedFileDirectoriesChange={(paths) => {
          store.setExpandedFileDirectories(sessionId, paths);
        }}
        onToggleHideIgnored={() => {
          store.toggleHideIgnored(sessionId);
        }}
        onLoadMoreAllFiles={(path) => {
          store.loadMoreAllFiles(sessionId, path);
        }}
        onLoadMoreGitChanges={() => {
          store.loadMoreGitChanges(sessionId);
        }}
        onOpenFile={openFile}
        onCloseFile={closeFile}
        onRetryFile={retryFile}
        onDownloadFile={(path, sizeBytes, mimeType) => {
          if (downloads === null) return;
          void downloads.start({ sessionId, path, mimeType, sizeBytes });
        }}
        onCancelDownload={() => {
          void downloads?.cancel();
        }}
        onReleaseDownload={() => {
          downloads?.releaseArtifact();
        }}
        downloadState={downloadState}
        createPreviewUrl={createPreviewUrl}
        // 左栏：返回目录列表、同目录会话、目录内搜索、分页与新建。
        onOpenDirectoryList={() => {
          store.leaveConversationSession(sessionId);
          router.push("/");
        }}
        onOpenSession={(otherSessionId) => {
          // 点当前会话不是导航：不打断正在进行的编辑。
          if (otherSessionId === sessionId) return;
          // 先为原会话尝试提交（不等待网络结果），再正常导航；不把草稿带到新会话。
          store.leaveConversationSession(sessionId);
          router.push(`/session/${encodeURIComponent(otherSessionId)}`);
        }}
        onSidebarQueryChange={(query) => {
          store.setConversationSidebarQuery(query);
        }}
        onSidebarPageChange={(pageInput) => {
          store.setConversationSidebarPage(pageInput);
        }}
        onCreateSession={(directoryId) => {
          if (openExistingPreparation()) return;
          setCreateIssue(null);
          store.openCreateSheet(directoryId);
        }}
        onCreateSessionAtPath={(path) => {
          if (openExistingPreparation()) return;
          setCreateIssue(null);
          setAgentId(null);
          store.openCreateSheetAtPath(path);
        }}
        // 会话头的 Agent 胶囊沿用主机面板的同一份连接事件覆盖层（覆盖层缺席即「状态未知」）。
        hostPanel={store.hostPanel()}
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
        // Markdown 里的工作目录路径：沿用文件查看的同一条入口（窄屏会切到对话栏并记住返回位置）。
        onFilePathPress={(path) => {
          openFile("all", path);
        }}
        // 类型化媒体：读取走仓库的附件连接；卸载时组件上抛释放，仓库撤销对象 URL。
        media={model.media}
        onLoadMedia={(attachmentId, mediaSessionId, mimeType) => {
          void store.loadAttachmentMedia(mediaSessionId, attachmentId, mimeType);
        }}
        onReleaseMedia={(attachmentId) => {
          store.releaseAttachmentMedia(attachmentId);
        }}
        onDownloadAttachment={(attachmentId, mimeType, fileName) => {
          pendingAttachmentDownload.current = { attachmentId, fileName };
          void store.loadAttachmentMedia(sessionId, attachmentId, mimeType);
        }}
        // 提问表单：客户端已校验的值交给仓库再校验并派发；刷新时用同一次 `session.read` 重新确认。
        onSubmitElicitation={(interactionId, values) => {
          const outcome = store.submitElicitationForm(sessionId, interactionId, values);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        onRefreshInteractions={() => {
          const outcome = store.refreshInteractions(sessionId);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        // 附件选择只在本地入草稿；发送后由仓库上传并在全部封存后才派发 prompt。
        onPickAttachment={(kind) => {
          void store.pickAttachments(sessionId, kind).then((outcome) => {
            if (!outcome.ok) toastStore.show(outcome.reason);
          });
        }}
        onPasteAttachments={(event) => {
          void store.pasteAttachments(sessionId, event).then((outcome) => {
            if (!outcome.ok) toastStore.show(outcome.reason);
          });
        }}
        onRemoveAttachment={(localId) => {
          store.removeAttachment(sessionId, localId);
        }}
        onRetryAttachments={() => {
          const outcome = store.retryAttachments(sessionId);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        onCancelAttachmentUpload={() => {
          const outcome = store.cancelAttachmentUpload(sessionId);
          if (!outcome.ok) toastStore.show(outcome.reason);
        }}
        onAddPathRef={beforeFirstMessage ? undefined : (path) => {
          const outcome = store.addPathRef(sessionId, path);
          return outcome.ok ? null : outcome.reason;
        }}
        onRemovePathRef={(path) => {
          store.removePathRef(sessionId, path);
        }}
        secondaryView={state.conversationSecondaryView}
        onSecondaryViewChange={(view) => {
          store.setConversationSecondaryView(view);
        }}
      />

      {/* 左栏底部的「⊕ 新建会话」落在这里：弹层的开关是 store 的**单份**状态
          （`createSheetDirectoryId`），只有它落在本会话所属目录上时才呈现——目录页与详情页呈现的
          也是它。提交载荷原样上抛（目录里的「先建目录」那类附加键因此不必在本页重复一遍）。 */}
      {sheetDirectoryId === null && !pathSheetOpen ? null : (
        <CreateSessionSheet
          model={store.createEntry(sheetDirectoryId)}
          selectedAgentId={agentId}
          submitIssue={createIssue}
          onSelectAgent={(next) => {
            setCreateIssue(null);
            setAgentId(next);
          }}
          onSubmit={({ agentId: selected, directoryPath }) => {
            // 目录标识由本页给出；入口只做 prepare → allocate，首条发送前不保存会话。
            void store
              .beginPreparation({ directoryId: sheetDirectoryId, agentId: selected, directoryPath })
              .then((outcome) => {
                if (!outcome.ok) {
                  setCreateIssue(outcome.reason);
                  setAgentId(null);
                  return;
                }
                setCreateIssue(null);
                store.closeCreateSheet();
                router.push(`/session/${encodeURIComponent(outcome.sessionId)}`);
              });
          }}
          onDismissUncertain={() => {
            const requestId = sheetDirectoryId === null ? undefined : state.createRequestIds.get(sheetDirectoryId);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            setCreateIssue(null);
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
