/**
 * 目录详情页（路由 `#/dir/<alias>`）。
 *
 * `alias` 是**目录别名**，不是路径：路由参数里因此不存在任何可以反推本机路径的片段。
 * 页面负责四件事——进入会话、打开创建弹层、把目录内搜索词与月份档写进仓库、把上次同步
 * 时刻交给详情组件；其余判定都在 feature 层。
 *
 * ## 为什么搜索词与月份档存在仓库里
 *
 * `DirectoryDetail` 是纯函数组件（只吃 props、交互一律上抛），因此这两个 UI 状态由页面读写：
 * 读 `state.directoryDetailQuery`（月份档由模型对账后的 `model.month` 呈现），写它们各自的
 * setter。目录页的查询词是**另一个字段**（`directoryQuery`），两页互不影响（spec 的
 * 「与目录页的查询词互不影响」：进详情页不清空目录页的查询词，返回时它仍在）。
 *
 * 这两个字段是**单份**的（store 单例跨路由存活），因此进入某个目录时要复位
 * （`resetDirectoryDetailView`）——否则在目录 A 选的月份档/敲的查询词会带进目录 B。
 */

import { useLocalSearchParams, useRouter } from "expo-router";
import { useEffect, useState } from "react";
import type { ReactElement } from "react";

import { ConnectionBadge, CreateSessionSheet, DirectoryDetail, RuntimeUnavailable } from "../../src/components";
import type { Palette } from "../../src/components/theme";
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
} from "../../src/components/theme";
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
    nav: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    backButton: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    backText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function DirectoryDetailRoute(): ReactElement {
  const { alias } = useLocalSearchParams<{ alias: string }>();
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [sheetOpen, setSheetOpen] = useState(false);
  const [agentId, setAgentId] = useState<string | null>(null);

  // 目录内查询词与月份档是单份 store 状态、跨路由存活；进入（或切换到）某个目录时复位一次，
  // 否则上一个目录的筛选会把新目录的内容整片挡住。以 alias 为依赖：换目录重跑。store 起初可能
  // 是 null（组合根尚未装配），因此把 store 也放进依赖，装配完成后补跑一次。
  useEffect(() => {
    if (store === null || typeof alias !== "string") return;
    store.resetDirectoryDetailView();
  }, [store, alias]);

  if (store === null || state === null) return <RuntimeUnavailable route="dir-detail" />;
  if (typeof alias !== "string" || alias.length === 0) return <RuntimeUnavailable route="dir-detail" />;

  const entry = store.createEntry(alias);

  return (
    <View role="main" style={styles.root}>
      <View role="navigation" style={styles.nav}>
        <Pressable
          role="button"
          style={styles.backButton}
          dataSet={{ action: "back-to-dirs" }}
          onPress={() => {
            router.back();
          }}
        >
          <Text style={styles.backText}>返回目录</Text>
        </Pressable>
        <ConnectionBadge
          panel={store.hostPanel()}
          onOpenHosts={() => {
            router.push("/hosts");
          }}
        />
      </View>

      <DirectoryDetail
        model={store.directoryDetail(alias)}
        createEntry={entry}
        canCreate={entry.permission.kind === "granted"}
        query={state.directoryDetailQuery}
        lastSyncedAtMs={state.lastSyncedAtMs}
        onQueryChange={(next) => {
          store.setDirectoryDetailQuery(next);
        }}
        onMonthChange={(next) => {
          store.setDirectoryDetailMonth(next);
        }}
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
    </View>
  );
}
