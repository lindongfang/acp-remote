/**
 * 「运行时未装配」的显式说明。
 *
 * ## 为什么需要它，而不是让页面渲染空数据
 *
 * 组合根（平台端口 + 同步客户端的装配）尚未接线时，store 为 `null`。若页面照常渲染
 * 「还没有可用的目录」，用户会把它读成「这台电脑上确实没有目录」——那是 R15 的
 * **第一种**空态，与事实不符，也可能诱导用户去删配置。
 *
 * 因此这里给第三种、语义完全不同的呈现：明确说明客户端运行时尚未装配，并保留
 * `data-runtime="unavailable"` 供浏览器用例定位。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`（`role="alert"`）+ `Text` + `StyleSheet`；
 * `data-route` / `data-runtime` 两个属性名与取值原样经 `dataSet` 进 DOM。
 */

import type { ReactElement } from "react";

import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface RuntimeUnavailableProps {
  /** 缺失装配的路由名，便于定位。 */
  readonly route: string;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    root: {
      backgroundColor: colors.dangerSoft,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.md,
      padding: spacing.lg,
      gap: spacing.sm,
    },
    title: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: "600",
    },
    body: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    loading: {
      flex: 1,
      backgroundColor: colors.bg,
      padding: spacing.lg,
      justifyContent: "center",
      alignItems: "center",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 初始化期间仅保留主题底色，避免刷新时闪现过渡文案。 */
export function RuntimeLoading(): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  return (
    <View style={styles.loading} dataSet={{ runtime: "loading" }} />
  );
}

/** 运行时未装配提示。 */
export function RuntimeUnavailable({ route }: RuntimeUnavailableProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View role="alert" style={styles.root} dataSet={{ route, runtime: "unavailable" }}>
      {/* `role="heading"` 不带层级：RNW 据此渲染 `<h1>`（原实现就是 `<h1>`）。 */}
      <Text role="heading" style={styles.title}>
        客户端初始化失败
      </Text>
      <Text style={styles.body}>
        无法准备连接所需的本地数据。请确认使用 HTTPS 访问，并允许浏览器存储，然后刷新重试。
      </Text>
    </View>
  );
}
