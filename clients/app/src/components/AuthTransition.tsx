/**
 * 首屏认证闸门（`design.md` D1）。
 *
 * 已配对但本次会话尚未认证完成时，业务路由**不**呈现：没有顶栏、没有目录/会话内容，只有这一块
 * 居中舞台。等待认证时仅保留主题底色；认证失败（不兼容、主机身份变化、离线）时复用与
 * `ShellView` **同一个**连接问题弹层组件——文案与动作只有一份实现。
 */

import type { ReactElement } from "react";

import type { ConnectionProblemModel } from "../features/connection-problem";
import type { Palette } from "./theme";
import { StyleSheet, View, palettes, spacing, useTheme } from "./theme";
import { ConnectionProblemDialog } from "./shell/ConnectionProblemDialog";

function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      flexGrow: 1,
      flexShrink: 1,
      minHeight: 0,
      alignItems: "center",
      justifyContent: "center",
      paddingVertical: spacing.xl * 2,
      paddingHorizontal: spacing.xl,
      backgroundColor: colors.bg,
    },
    problemSlot: {
      width: "100%",
      // 与 ConnectionProblemDialog 的卡片宽度保持一致，避免认证失败时第二次变窄。
      maxWidth: 560,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export interface AuthTransitionProps {
  /** 首屏认证失败时的连接问题；`null` = 仍在认证，呈现进度屏。 */
  readonly problem: ConnectionProblemModel | null;
  readonly onRepair: () => void;
  readonly onReload: () => void;
}

/** 首次认证期间的整页舞台：认证失败时原样复用连接问题弹层。 */
export function AuthTransition({ problem, onRepair, onReload }: AuthTransitionProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View role="main" style={styles.page} dataSet={{ authTransition: problem === null ? "pending" : "problem" }}>
      {problem !== null && (
        <View style={styles.problemSlot}>
          <ConnectionProblemDialog problem={problem} onRepair={onRepair} onReload={onReload} />
        </View>
      )}
    </View>
  );
}
