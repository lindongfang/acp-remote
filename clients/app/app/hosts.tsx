/**
 * 主机与连接页（路由 `/hosts`，`data-route="hosts"`）。
 *
 * 与目录页上的抽屉共用同一个组件与同一份模型：Agent 列表来自快照（R19），
 * 连接状态是可选的事件覆盖层，缺席时呈现「状态未知」而不是「已断开」。
 *
 * ## 外壳与页头
 *
 * 这是**非配对路由**：全局顶栏（标识、设备胶囊、主题开关、帮助入口）由根布局装配的共享外壳
 * 承担（`design.md` D1），本页因此不再自建导航，也不再渲染第二个连接状态徽标——连接状态只由
 * 顶栏那一个设备胶囊表达，本页的内容里只有主机面板自己的头部（那是「这一面在说什么」，
 * 不是第二个状态徽标）。页头只留离开本页的入口（与目录详情页同一形态）。
 */

import { useRouter } from "expo-router";
import type { ReactElement } from "react";

import { HostConnectionPanel, RuntimeUnavailable } from "../src/components";
import type { Palette } from "../src/components/theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
} from "../src/components/theme";
import { useClientState, useClientStore } from "../src/features/runtime";

/** 页面级布局（与目录页、目录详情页同一份骨架）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      flex: 1,
      gap: spacing.md,
      padding: spacing.lg,
      backgroundColor: colors.bg,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.md,
    },
    back: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.sm,
    },
    backText: {
      color: colors.accent,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: fontWeight.medium,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export default function HostsRoute(): ReactElement {
  const store = useClientStore();
  const state = useClientState();
  const router = useRouter();
  const theme = useTheme();

  if (store === null || state === null) return <RuntimeUnavailable route="hosts" />;

  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View role="main" style={styles.page}>
      <View style={styles.head} dataSet={{ routeHeader: "hosts" }}>
        <Pressable
          role="button"
          style={styles.back}
          dataSet={{ action: "back-to-dirs" }}
          onPress={() => router.back()}
        >
          <Text style={styles.backText}>‹ 目录</Text>
        </Pressable>
        <Text style={styles.title}>主机与连接</Text>
      </View>

      <HostConnectionPanel model={store.hostPanel()} />
    </View>
  );
}
