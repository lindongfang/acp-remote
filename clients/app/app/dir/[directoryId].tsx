/**
 * 目录详情页（路由 `/dir/<directoryId>`）。
 *
 * `directoryId` 是**目录标识**，不是路径：路由参数里因此不存在任何可以反推本机路径的片段。
 * 页面负责五件事——进入会话、打开创建弹层、把页码写进仓库、把上次同步时刻交给详情组件，
 * 以及把目录选择器的开关与结果接起来；其余判定都在 feature 层（分页、页码纠正、路径的取舍都由
 * `DirectoryDetailModel` 给出）。
 *
 * ## 没有自建导航
 *
 * 本页曾经自己拼一条导航（返回按钮 + 连接状态徽标）。现在顶栏与连接状态只由**共享外壳**
 * 表达（`design.md` D1：一份外壳只有一处判据，装在 `app/_layout.tsx` 里），本页因此不再
 * 渲染 `ConnectionBadge`，也不再有自己的返回按钮——返回入口在页头的详情组件里
 * （`data-action="back-to-dirs"`），导航仍由本页的 `router.back()` 执行。
 *
 * ## 为什么页码存在仓库里，以及「首帧就该是第一页」是怎么做到的
 *
 * `DirectoryDetail` 是纯函数组件（只吃 props、交互一律上抛），因此页码由页面读写：读
 * `store.directoryDetail(directoryId)`（组件回填的是模型纠正后的值），写 `setDirectoryDetailPage`。
 * 目录页的查询词与筛选是**另外两个字段**（`directoryQuery`/`directoryFilter`），两页互不影响：
 * 进详情页不清空目录页的查询词，返回时它仍在。
 *
 * 页码是**单份**状态（store 单例跨路由存活），而它的含义与目录绑定，因此判据分成两处：
 *
 * - **读取侧**（`ClientState.directoryDetailDirectoryId`）：页码只在属于**这个**目录时才被采用，否则按
 *   第一页算。从别的目录进来时首帧因此直接正确——不必等挂载后的复位；
 * - **本页的挂载 effect**：按约定「进入某个目录时从第一页开始」复位一次，**包括再次进入同一个目录**
 *   （r15 的 R15-25 就是这条）。它与读取侧的分工很清楚：只有「同一个目录」那一支可能在复位前先
 *   渲染一帧旧页，而那帧上的页码属于本目录、指向本目录的内容，**绝不会**是别的目录的页码；
 *   跨目录进入则由读取侧在首帧直接给出第一页。
 *
 * 两处合起来的结果：跨目录进入首帧就是第一页；同目录再次进入由复位收敛到第一页。
 *
 * ## 创建弹层的开关为什么不在本页
 *
 * `ClientState.createSheetDirectoryId`（目录层级入口）与 `createSheetBrowses`（顶部入口）共同构成
 * 「弹层是否打开」这**一个**判据：目录页的卡片创建入口与详情页的创建入口读写的是前者，两页各自
 * 只做一次派生读取（本页即 `createSheetDirectoryId === directoryId`）。本页因此**没有**本地 `sheetOpen`——
 * 否则两页各持一份，同一时刻可能出现两个弹层。同样因为它跨路由存活，进入本页时先 `closeCreateSheet()`
 * 复位一次，避免把别处打开的弹层带进来。
 *
 * 目录选择器（`DirectoryPicker`）也在这里渲染：它的开关同样是单份 store 状态，任何渲染弹层的路由
 * 都要能把它画出来（顶部入口在本页之外，而选择器可能还开着）。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";

import { CreateSessionSheet, DirectoryDetail, DirectoryPicker, RuntimeUnavailable } from "../../src/components";
import type { Palette } from "../../src/components/theme";
import { StyleSheet, View, palettes, spacing, useTheme } from "../../src/components/theme";
import type { DirectorySelection } from "../../src/features/create-session-model";
import { useClientState, useClientStore } from "../../src/features/runtime";

/** 页面级布局：与详情组件内的样式表同构（浅/深各建一次）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      flex: 1,
      gap: spacing.md,
      padding: spacing.lg,
      backgroundColor: colors.bg,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function DirectoryDetailRoute(): ReactElement {
  const { directoryId } = useLocalSearchParams<{ directoryId: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [agentId, setAgentId] = useState<string | null>(null);

  /**
   * 进入本页时复位页码：页码是**单份** store 状态，「进入某个目录时从第一页开始」这条约定
   * （含**再次**进入同一个目录，见 r15 的 R15-25）由它保证。
   *
   * 首帧为什么也是对的：`directoryDetail(directoryId)` 只在页码属于**这个**目录时才采用它
   * （`ClientState.directoryDetailDirectoryId`）。从别的目录进来时首帧直接按第一页算，复位晚一帧也不会
   * 让用户先看到上一个目录的页码——那正是这条 effect 做不到的事，读取侧的判定补上了它。
   */
  useEffect(() => {
    if (store === null || typeof directoryId !== "string") return;
    store.resetDirectoryDetailView();
  }, [store, directoryId]);

  // 创建弹层的开关**不落在本页**：它由 store 的单份状态派生（见下方 `sheetOpen`），目录页呈现的
  // 那一份也是它。同样因为它跨路由存活，进入本页时先复位一次，否则会把别的路由上打开的弹层带进详情页。
  useEffect(() => {
    if (store === null || typeof directoryId !== "string") return;
    store.closeCreateSheet();
  }, [store, directoryId]);

  // 选择器的回调定住身份：它挂载时按 `initialPath` 请求初始层，回调换身份会让那次请求重跑。
  const onBrowse = useCallback(
    (path: string | null) => {
      if (store !== null) store.browseDirectory(path);
    },
    [store],
  );
  const onCreateFolder = useCallback(
    (name: string) => {
      if (store !== null) store.createFolderInBrowse(name);
    },
    [store],
  );
  const onConfirmSelection = useCallback(
    (selection: DirectorySelection) => {
      if (store !== null) store.selectBrowsedDirectory(selection);
    },
    [store],
  );
  const onCancelPicker = useCallback(() => {
    if (store !== null) store.closeDirectoryPicker();
  }, [store]);

  if (store === null || state === null) return <RuntimeUnavailable route="dir-detail" />;
  if (typeof directoryId !== "string" || directoryId.length === 0) return <RuntimeUnavailable route="dir-detail" />;

  const entry = store.createEntry(directoryId);

  /**
   * 弹层的打开状态：**唯一来源**是 store 的单份状态（等于本页的目录别名时才呈现）。
   * 本页不再持有 `useState` 开关——否则目录页与详情页会各持一份，同一时刻可能出现两个弹层。
   */
  const sheetOpen = state.createSheetDirectoryId === directoryId;

  return (
    <View role="main" style={styles.root}>
      <DirectoryDetail
        model={store.directoryDetail(directoryId)}
        lastSyncedAtMs={state.lastSyncedAtMs}
        onBack={() => {
          router.back();
        }}
        onPageChange={(requested) => {
          store.setDirectoryDetailPage(directoryId, requested);
        }}
        onOpenSession={(sessionId) => {
          router.push(`/session/${encodeURIComponent(sessionId)}`);
        }}
        onCreateSession={() => {
          store.openCreateSheet(directoryId);
        }}
      />

      {sheetOpen ? (
        <CreateSessionSheet
          model={entry}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onSubmit={({ agentId: selected, directoryPath, createIfMissing }) => {
            const outcome = store.submitCreateSession({
              directoryId,
              agentId: selected,
              directoryPath,
              createIfMissing,
            });
            if (!outcome.ok) setAgentId(null);
          }}
          onOpenDirectoryPicker={() => {
            store.openDirectoryPicker();
          }}
          onDismissUncertain={() => {
            const requestId = state.createRequestIds.get(directoryId);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            store.closeCreateSheet();
          }}
        />
      ) : null}

      {/* 选择器与弹层共用一份状态：开关在 store 上，任何渲染弹层的路由都要能画出它。
          「不能逐层浏览」的说明口径取自**顶部入口**的目录块：本页的弹层通常属于某个已登记目录
          （那时没有这一支），但选择器可能是从顶部入口那条链路带过来的，口径必须跟着它走——未连上
          主机时说「还没连上」，而不是「服务端没有提供该能力」。 */}
      {state.directoryBrowse.open ? (
        <DirectoryPicker
          initialPath={entry.directory.kind === "browse" ? entry.directory.initialPath : null}
          registeredDirectories={store.registeredDirectories()}
          browser={store.directoryBrowser()}
          unavailable={entry.directory.kind === "browse" ? (entry.directory.unavailable ?? undefined) : undefined}
          onBrowse={onBrowse}
          onCreateFolder={onCreateFolder}
          onConfirm={onConfirmSelection}
          onCancel={onCancelPicker}
        />
      ) : null}
    </View>
  );
}
