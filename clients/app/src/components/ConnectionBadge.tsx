/**
 * 连接状态徽标（R12 的连接状态在每一页都可见）。
 *
 * 徽标的数据来自连接状态机（R19 的主机面板把同一份状态渲染得更完整），
 * 组件本身不判定连接状态，也不从会话活跃度推断任何东西——**样式也一样**：
 * 这里只给出一套中性配色，不按连接状态换色（换色就等于由组件断定「什么状态算坏」）。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`Pressable`（`role="button"`，RNW 据此渲染成
 * `<button>`）+ `Text` + `StyleSheet`；`data-connection-state` / `data-action` /
 * `data-connection-label` 三个属性名与取值都原样经 `dataSet` 进 DOM。
 */

import type { ReactElement } from "react";

import type { HostPanelModel } from "../features/host-panel-model";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface ConnectionBadgeProps {
  readonly panel: HostPanelModel;
  readonly onOpenHosts?: () => void;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    badge: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.xs,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel,
    },
    label: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 连接状态徽标。 */
export function ConnectionBadge({ panel, onOpenHosts }: ConnectionBadgeProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    // 交互一律回调上抛：这里不导航、不读路由。
    <Pressable
      role="button"
      style={styles.badge}
      dataSet={{ connectionState: panel.connectionState, action: "open-hosts" }}
      onPress={() => {
        onOpenHosts?.();
      }}
    >
      <Text style={styles.label} dataSet={{ connectionLabel: "true" }}>
        {panel.connectionLabel}
      </Text>
    </Pressable>
  );
}
