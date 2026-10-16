/**
 * 目录页（路由 `/`，`data-route="dirs"`）。
 *
 * 目录是列表的主要组织单位（R15）：本页只做三件事——取模型、渲染共享组件、把点击上抛给路由。
 * 任何数据判定都在 `src/features/` 里，页面里没有业务分支。
 *
 * 搜索框与筛选段是**受控**的：值来自 store 的 `directoryQuery`/`directoryFilter`，输入回到同名
 * setter。两者都只是 UI 状态，不发任何命令（spec「搜索与筛选不依赖在线查询」）。
 *
 * 目录卡片的创建入口与详情页的创建入口是**同一条**链路：同一个 `CreateSessionSheet`、同一份
 * `store.createEntry(alias)`、同一条 `store.submitCreateSession(...)`。弹层的开关不落在本页：
 * 它由 store 的 `createSheetAlias` 派生（见下方 `sheetAlias`），因此目录页与详情页不可能各持
 * 一个布尔量。创建入口**没有**权限维度（设备权限层已删除），因此这里不再读任何 scope。
 */

import { useRouter } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import {
  ConnectionBadge,
  CreateSessionSheet,
  DirectoryList,
  HostConnectionPanel,
  RuntimeUnavailable,
} from "../src/components";
import type { Palette } from "../src/components/theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  palettes,
  radius,
  spacing,
  useTheme,
} from "../src/components/theme";
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
    nav: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    navButton: {
      paddingVertical: 6,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    navText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
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

  if (store === null || state === null) return <RuntimeUnavailable route="dirs" />;

  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const page = store.directoryPage();

  /**
   * 要呈现的创建弹层：`null` 表示本页不呈现。
   *
   * 唯一来源是 store 的 `createSheetAlias`——本页不另持开关，详情页呈现的那一份也是它。
   * 再要求这个别名落在**本机已登记的目录**里（别名全集取自快照，**不**经过查询/筛选）：
   * 否则打开弹层后再敲一个查询词，会把正在提交或等待用户处理的弹层卸掉。
   */
  const requestedSheetAlias = state.createSheetAlias;
  const sheetAlias =
    state.resources?.workspaces.find((workspace) => workspace.alias === requestedSheetAlias)?.alias ?? null;

  return (
    <View role="main" style={styles.page}>
      <View role="navigation" style={styles.nav}>
        <Pressable
          role="button"
          style={styles.navButton}
          dataSet={{ action: "open-hosts" }}
          onPress={() => {
            router.push("/hosts");
          }}
        >
          <Text style={styles.navText}>主机与连接</Text>
        </Pressable>
        <Pressable
          role="button"
          style={styles.navButton}
          dataSet={{ action: "open-pairing" }}
          onPress={() => {
            router.push("/pair");
          }}
        >
          <Text style={styles.navText}>配对</Text>
        </Pressable>
        <ConnectionBadge
          panel={store.hostPanel()}
          onOpenHosts={() => {
            router.push("/hosts");
          }}
        />
      </View>

      <DirectoryList
        model={page}
        // 内联会话行只依赖快照：与详情页的查询词/月份无关（store 的选择器保证）。
        sessionsByAlias={store.directoryPageSessions()}
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
        onOpenDirectory={(alias) => {
          router.push(`/dir/${encodeURIComponent(alias)}`);
        }}
        // 目录卡片的创建入口与详情页是同一条链路（同一个弹层模型、同一条提交命令）。
        // 入口恒可用：设备不再有创建会话的权限维度，页面因此无条件给出回调。
        onCreateSession={(alias: string) => {
          store.openCreateSheet(alias);
        }}
      />

      <HostConnectionPanel model={store.hostPanel()} />

      {sheetAlias === null ? null : (
        <CreateSessionSheet
          model={store.createEntry(sheetAlias)}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onSubmit={({ agentId: selected, directoryPath }) => {
            const outcome = store.submitCreateSession({
              directoryAlias: sheetAlias,
              agentId: selected,
              ...(directoryPath === "" ? {} : { directoryPath }),
            });
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
    </View>
  );
}
