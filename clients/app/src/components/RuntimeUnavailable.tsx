/**
 * 「运行时未装配」的显式说明。
 *
 * ## 为什么需要它，而不是让页面渲染空数据
 *
 * 组合根（平台端口 + 同步客户端的装配）尚未接线时，store 为 `null`。若页面照常渲染
 * 「本机没有任何目录」，用户会把它读成「这台电脑上确实没有目录」——那是 R15 的
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
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 运行时未装配提示。 */
export function RuntimeUnavailable({ route }: RuntimeUnavailableProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View role="alert" style={styles.root} dataSet={{ route, runtime: "unavailable" }}>
      {/* `role="heading"` 不带层级：RNW 据此渲染 `<h1>`（原实现就是 `<h1>`）。 */}
      <Text role="heading" style={styles.title}>
        同步运行时未装配
      </Text>
      <Text style={styles.body}>
        本页面需要组合根注入平台端口（设备身份、本地缓存、生命周期）与同步客户端后才能连接电脑。
        在装配完成前，这里不会显示任何目录、会话或 Agent 数据——以免把「尚未连接」显示成
        「本机没有任何目录」。
      </Text>
    </View>
  );
}
