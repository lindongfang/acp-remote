/**
 * 目录页（路由 `/`，`data-route="dirs"`）。
 *
 * 目录是列表的主要组织单位（R15）：本页只做三件事——取模型、渲染共享组件、把点击上抛给路由。
 * 任何数据判定都在 `src/features/` 里，页面里没有业务分支。
 *
 * **本页不自建导航、也不自建连接状态徽标**：顶栏（标识、设备胶囊、主题开关、帮助入口）由根布局
 * 装配的共享外壳承担，连接状态只由那一个设备胶囊表达。此前这里另有一条自建导航与第二个状态
 * 徽标，两者都是「路由自行拼出的重复表达」——同一状态有两处判据，必然漂移。
 * 本页因此只剩下目录内容本身、**顶部创建入口**与涉及创建链路的那两块。顶部创建入口是
 * `DirectoryList` 工具条右端的那一个（本页只把回调交给它）——它不是页面自建的一行，位置由列表
 * 组件按原型给出。
 *
 * 搜索框与筛选段是**受控**的：值来自 store 的 `directoryQuery`/`directoryFilter`，输入回到同名
 * setter。两者都只是 UI 状态，不发任何命令（spec「搜索与筛选不依赖在线查询」）。
 *
 * ## 两条创建路径共用同一份状态
 *
 * - **目录层级入口**（卡片/详情页的 `⊕ 新建`）：目录已知 → 弹层呈现「目录/路径两行」，
 *   载荷用该目录的标识（`openCreateSheet(directoryId)`）；
 * - **顶部入口**（目录列表工具条右端的 `⊕ 新建`）：目录未知 → 弹层呈现「选择文件夹控件 + 已选路径 + 说明」，
 *   目录由 `DirectoryPicker` 逐层浏览或从已登记目录里选出来（`openCreateSheetToBrowse()`）。
 *
 * 两条路径的弹层开关与浏览/选择状态都只有**一份**（住在 store 里），因此同一时刻不可能出现两个弹层、
 * 两个选择器，页面也不各持一个布尔量。选择器与弹层一样在这里渲染：它的开关是跨路由存活的单份状态，
 * 任何一条渲染弹层的路由都要能把它画出来。
 */

import { useRouter } from "expo-router";
import { useCallback, useEffect, useState } from "react";
import type { ReactElement } from "react";

import {
  CreateSessionSheet,
  DirectoryList,
  DirectoryPicker,
  HostConnectionPanel,
  RuntimeUnavailable,
} from "../src/components";
import type { Palette } from "../src/components/theme";
import { StyleSheet, View, palettes, spacing, useTheme } from "../src/components/theme";
import { CREATE_SHEET_BROWSE_KEY } from "../src/features/create-session-model";
import type { DirectorySelection } from "../src/features/create-session-model";
import { useClientState, useClientStore } from "../src/features/runtime";

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      flexGrow: 1,
      gap: spacing.md,
      padding: spacing.lg,
      backgroundColor: colors.bg,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function DirectoriesRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();
  // 选中的 Agent 是页面局部状态（不跨路由存活，与弹层的开关无关）。
  const [agentId, setAgentId] = useState<string | null>(null);

  // 创建弹层的开关是**单份** store 状态、跨路由存活（详情页与目录页共用它）。正因如此，
  // 进入本页时先复位一次：否则会把别的路由上打开的弹层带回目录页。store 起初可能为 null
  // （组合根尚未装配），把它放进依赖，装配完成后补跑一次。
  useEffect(() => {
    if (store === null) return;
    store.closeCreateSheet();
  }, [store]);

  // 选择器的回调定住身份：选择器挂载时按 `initialPath` 请求初始层，依赖里带 `onBrowse`，
  // 每次渲染换一个新函数会让那次请求反复重跑。
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
  const onOpenBrowseSheet = useCallback(() => {
    if (store !== null) store.openCreateSheetToBrowse();
  }, [store]);

  if (store === null || state === null) return <RuntimeUnavailable route="dirs" />;

  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const page = store.directoryPage();

  /**
   * 要呈现的创建弹层：`null` 表示本页不呈现。
   *
   * 唯一来源是 store 的两支单份状态——顶部入口（`createSheetBrowses`）或某个**已记录目录**
   * （`createSheetDirectoryId`）。后者再要求这个标识落在本机已记录的目录里（标识全集取自快照，
   * **不**经过查询/筛选）：否则打开弹层后再敲一个查询词，会把正在提交或等待用户处理的弹层卸掉。
   */
  const browseSheet = state.createSheetBrowses;
  const requestedSheetDirectoryId = state.createSheetDirectoryId;
  const sheetDirectoryId =
    state.resources?.workspaces.find((workspace) => workspace.directoryId === requestedSheetDirectoryId)
      ?.directoryId ?? null;
  const entry =
    browseSheet ? store.createEntry(null) : sheetDirectoryId === null ? null : store.createEntry(sheetDirectoryId);
  // 提交状态按弹层的机器键读写（已记录目录用目录标识，顶部入口用 `CREATE_SHEET_BROWSE_KEY`）。
  const sheetKey = browseSheet ? CREATE_SHEET_BROWSE_KEY : sheetDirectoryId;
  const directoryBlock = entry !== null && entry.directory.kind === "browse" ? entry.directory : null;

  return (
    <View role="main" style={styles.page}>
      <DirectoryList
        model={page}
        // 内联会话行只依赖快照：与详情页的查询词/页码无关（store 的选择器保证）。
        sessionsByDirectoryId={store.directoryPageSessions()}
        query={state.directoryQuery}
        onQueryChange={(query) => {
          store.setDirectoryQuery(query);
        }}
        filter={state.directoryFilter}
        onFilterChange={(filter) => {
          store.setDirectoryFilter(filter);
        }}
        onOpenSession={(sessionId) => {
          router.push(`/session/${encodeURIComponent(sessionId)}`);
        }}
        onOpenDirectory={(directoryId) => {
          router.push(`/dir/${encodeURIComponent(directoryId)}`);
        }}
        // 目录卡片的创建入口与详情页是同一条链路（同一个弹层模型、同一条提交命令）。
        // 入口恒可用：设备不再有创建会话的权限维度，页面因此无条件给出回调。
        onCreateSession={(directoryId: string) => {
          store.openCreateSheet(directoryId);
        }}
        // **顶部**创建入口：目录未知，由用户浏览/选择一层。它画在目录列表工具条的右端（原型的位置），
        // 与卡片上的目录内入口是两条不同的路径。
        onCreateSessionToBrowse={onOpenBrowseSheet}
      />

      <HostConnectionPanel model={store.hostPanel()} />

      {entry === null ? null : (
        <CreateSessionSheet
          model={entry}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onOpenDirectoryPicker={() => {
            store.openDirectoryPicker();
          }}
          onSubmit={({ agentId: selected, directoryPath, createIfMissing }) => {
            const outcome = store.submitCreateSession({
              directoryId: browseSheet ? null : sheetDirectoryId,
              agentId: selected,
              directoryPath,
              createIfMissing,
            });
            if (!outcome.ok) setAgentId(null);
          }}
          onDismissUncertain={() => {
            const requestId = sheetKey === null ? undefined : state.createRequestIds.get(sheetKey);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            store.closeCreateSheet();
          }}
        />
      )}

      {/* 目录选择器与弹层共用一份状态：开关在 store 上，任何渲染弹层的路由都要能画出它。
          「不能逐层浏览」的说明口径同样取自弹层模型的目录块（`unavailable`）：未连上主机时说
          「还没连上」，只有已连上却没协商到该能力才说「服务端没有提供」。目录块不是浏览形态
          （本页的弹层来自某个已登记别名）时给不出更准的事实，按后者呈现。 */}
      {state.directoryBrowse.open ? (
        <DirectoryPicker
          initialPath={directoryBlock === null ? null : directoryBlock.initialPath}
          registeredDirectories={store.registeredDirectories()}
          browser={store.directoryBrowser()}
          unavailable={directoryBlock?.unavailable ?? undefined}
          onBrowse={onBrowse}
          onCreateFolder={onCreateFolder}
          onConfirm={onConfirmSelection}
          onCancel={onCancelPicker}
        />
      ) : null}
    </View>
  );
}
