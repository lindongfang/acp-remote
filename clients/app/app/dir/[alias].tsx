/**
 * 目录详情页（路由 `/dir/<alias>`）。
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
 *
 * ## 创建弹层的开关为什么不在本页
 *
 * `ClientState.createSheetAlias` 是**单份**状态：目录页的卡片创建入口与详情页的创建入口
 * 读写的是它，两页各自只做一次派生读取（本页即 `createSheetAlias === alias`）。本页因此
 * **没有**本地 `sheetOpen`——否则两页各持一份，同一时刻可能出现两个弹层。同样因为它跨路由
 * 存活，进入本页时先 `closeCreateSheet()` 复位一次，避免把别处打开的弹层带进来。
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
  const [agentId, setAgentId] = useState<string | null>(null);

  // 目录内查询词与月份档是单份 store 状态、跨路由存活；进入（或切换到）某个目录时复位一次，
  // 否则上一个目录的筛选会把新目录的内容整片挡住。以 alias 为依赖：换目录重跑。store 起初可能
  // 是 null（组合根尚未装配），因此把 store 也放进依赖，装配完成后补跑一次。
  useEffect(() => {
    if (store === null || typeof alias !== "string") return;
    store.resetDirectoryDetailView();
  }, [store, alias]);

  // 创建弹层的开关**不落在本页**：它由 store 的单份 `createSheetAlias` 派生（见下方
  // `sheetOpen`），目录页呈现的那一份也是它。同样因为它跨路由存活，进入本页时先复位一次，
  // 否则会把别的路由上打开的弹层带进详情页。
  useEffect(() => {
    if (store === null || typeof alias !== "string") return;
    store.closeCreateSheet();
  }, [store, alias]);

  if (store === null || state === null) return <RuntimeUnavailable route="dir-detail" />;
  if (typeof alias !== "string" || alias.length === 0) return <RuntimeUnavailable route="dir-detail" />;

  const entry = store.createEntry(alias);

  /**
   * 弹层的打开状态：**唯一来源**是 store 的 `createSheetAlias`（等于本页的目录别名时才呈现）。
   * 本页不再持有 `useState` 开关——否则目录页与详情页会各持一份，同一时刻可能出现两个弹层。
   */
  const sheetOpen = state.createSheetAlias === alias;

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
        }}
      />

      {sheetOpen ? (
        <CreateSessionSheet
          model={entry}
          selectedAgentId={agentId}
          onSelectAgent={setAgentId}
          onSubmit={({ agentId: selected, directoryPath }) => {
            const outcome = store.submitCreateSession({
              directoryAlias: alias,
              agentId: selected,
              ...(directoryPath === "" ? {} : { directoryPath }),
            });
            if (!outcome.ok) setAgentId(null);
          }}
          onDismissUncertain={() => {
            const requestId = state.createRequestIds.get(alias);
            if (requestId !== undefined) store.dismissUncertain(requestId);
          }}
          onClose={() => {
            store.closeCreateSheet();
          }}
        />
      ) : null}
    </View>
  );
}
