/**
 * 配对页的明暗切换按钮。
 *
 * ## 为什么是页内按钮
 *
 * 共享外壳在配对入口隐藏顶栏（`ShellView` 的 `topBarVisible`），顶栏里的主题开关不可用；原型在
 * 配对页右上角放了一个独立的明暗按钮。本组件只负责这个按钮的呈现与一次写回，不持有主题状态。
 *
 * ## 复用，而不是第二份状态
 *
 * 读写都走 `themePreferenceStore`：`useTheme()` 解析当前调色板，`useThemePreference()` 只用于
 * 诊断锚点；点击执行与共享外壳相同的 `set(theme.dark ? "light" : "dark")`。因此配对页、外壳和
 * 其它页面永远读同一份偏好，刷新后也保持一致。
 *
 * ## 位置
 *
 * 固定浮在页面右上角（桌面 18px、窄屏 12px），不参与配对状态的居中布局。位置、圆角、阴影与
 * 图标的亮橙色都取自桌面原型 `.pairtheme`（`prototypes/acp-remote-pwa.html:485-495`）；橙色是原型
 * 里唯一不来自调色板变量的主题色（白天 `#d9480f`、黑夜 `#ffa94d`），因此在本文件按深浅色各给一次。
 */

import type { ReactElement } from "react";

import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  View,
  palettes,
  themePreferenceStore,
  useTheme,
  useThemePreference,
  useWindowDimensions,
  webStyle,
  zIndex,
} from "./theme";

/** 原型 `.pairtheme[data-ic="orange"]`：图标颜色按主题固定为亮橙。 */
const THEME_ICON_COLORS = { light: "#d9480f", dark: "#ffa94d" } as const;

/** 按调色板建样式表：浅/深各建一次，渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette, dark: boolean) {
  const iconColor = dark ? THEME_ICON_COLORS.dark : THEME_ICON_COLORS.light;
  return StyleSheet.create({
    button: {
      width: 36,
      height: 36,
      alignItems: "center",
      justifyContent: "center",
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: 10,
      backgroundColor: colors.panel,
      boxShadow: colors.shadow,
      ...webStyle({ position: "fixed", top: 18, right: 18 }),
      zIndex: zIndex.topBar,
    },
    buttonCompact: {
      ...webStyle({ top: 12, right: 12 }),
    },
    // 与共享外壳一致：太阳/月亮用图形画，不依赖字体里有没有那个字形。
    sun: {
      width: 10,
      height: 10,
      borderRadius: 5,
      backgroundColor: iconColor,
    },
    moon: {
      width: 14,
      height: 14,
      overflow: "hidden",
    },
    moonBody: {
      width: 13,
      height: 13,
      borderRadius: 7,
      backgroundColor: iconColor,
    },
    moonBite: {
      position: "absolute",
      top: -4,
      left: 5,
      width: 13,
      height: 13,
      borderRadius: 7,
      backgroundColor: colors.panel,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light, false), dark: buildStyles(palettes.dark, true) };

/** 配对页右上角的明暗切换；与共享外壳共用 `themePreferenceStore`。 */
export function PairingThemeToggle(): ReactElement {
  const theme = useTheme();
  const preference = useThemePreference();
  const { width } = useWindowDimensions();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <Pressable
      role="button"
      style={[styles.button, width < 600 ? styles.buttonCompact : null]}
      aria-label={theme.dark ? "当前：黑夜，点击切换" : "当前：白天，点击切换"}
      dataSet={{
        action: "toggle-theme",
        themePreference: preference,
        pairingThemeToggle: "true",
      }}
      onPress={() => {
        themePreferenceStore.set(theme.dark ? "light" : "dark");
      }}
    >
      {theme.dark ? (
        <View style={styles.sun} />
      ) : (
        <View style={styles.moon}>
          <View style={styles.moonBody} />
          <View style={styles.moonBite} />
        </View>
      )}
    </Pressable>
  );
}
