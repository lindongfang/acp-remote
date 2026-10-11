/**
 * 目录页（路由 `#/`，`data-route="dirs"`）。
 *
 * 目录是列表的主要组织单位（R15）：本页只做三件事——取模型、渲染共享组件、把点击上抛给路由。
 * 任何数据判定都在 `src/features/` 里，页面里没有业务分支。
 *
 * 搜索框与筛选段是**受控**的：值来自 store 的 `directoryQuery`/`directoryFilter`，输入回到同名
 * setter。两者都只是 UI 状态，不发任何命令（spec「搜索与筛选不依赖在线查询」）。
 */

import { useRouter } from "expo-router";
import type { ReactElement } from "react";

import { ConnectionBadge, DirectoryList, HostConnectionPanel, RuntimeUnavailable } from "../src/components";
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

  if (store === null || state === null) return <RuntimeUnavailable route="dirs" />;

  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const page = store.directoryPage();

  /**
   * 创建会话的权限只由**本次认证的 scopes** 决定（`createSessionPermission` 不看目录别名），
   * 因此这里任取一个已知别名当 `createEntry` 的入参即可：优先取页面模型里的第一个目录，
   * 还没有快照时退回快照里登记的第一个 workspace 别名，都没有则用空串（对判定没有影响）。
   *
   * 提示条只在已经拿到快照（页面模型不是 `loading`）时给出：未认证/未配对时 scopes 为 `null`，
   * 那时画「没有创建会话的权限，需要电脑端授予」会把「权限未知」说成「已被拒绝」。
   */
  const permissionAlias =
    page.kind === "ready"
      ? (page.directories[0]?.alias ?? "")
      : (state.resources?.workspaces[0]?.alias ?? "");
  const permission = store.createEntry(permissionAlias).permission;
  const createPermissionNote =
    page.kind === "loading" || permission.kind === "granted" ? null : permission.note;

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
        createPermissionNote={createPermissionNote}
        onOpenSession={(sessionId) => {
          router.push(`/session/${encodeURIComponent(sessionId)}`);
        }}
        onOpenDirectory={(alias) => {
          router.push(`/dir/${encodeURIComponent(alias)}`);
        }}
      />

      <HostConnectionPanel model={store.hostPanel()} />
    </View>
  );
}
