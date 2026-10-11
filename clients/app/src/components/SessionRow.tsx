/**
 * 会话行（目录页的内联会话行与目录详情页共用）。
 *
 * 会话行**只**呈现标题、Agent 名称、状态与时间：它的 props 里没有任何路径字段，因此
 * 「顺手把路径也显示出来」在类型上就不成立（R15 的负向断言在 `r15-directory-list.test.ts`
 * 与 e2e 里对渲染结果再核对一次）。
 *
 * 组件是纯函数：所有交互通过回调上抛，导航由页面负责。
 */

import type { ReactElement } from "react";

import type { SessionRowModel } from "../features/directory-model";
import { StatusPill } from "./StatusPill";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface SessionRowProps {
  readonly session: SessionRowModel;
  /** 打开这个会话（页面负责导航）；缺省时本行只是展示。 */
  readonly onOpen?: () => void;
}

/**
 * ISO 时刻的展示形式：只做**位置固定**的字符串截取（`YYYY-MM-DDTHH:MM…` → `MM-DD HH:MM`）。
 *
 * 为什么不在组件里算相对时刻（「3 分钟前」）：`renderToStaticMarkup` 没有时钟、e2e 也不该
 * 依赖运行时刻，同一份视图模型必须渲染出同一段文本，否则组件测试与浏览器断言同时失去判别力。
 * 时间字段本身经 `dataSet` 原样进入 DOM，需要精确值时可从属性取。
 */
function formatUpdatedAt(updatedAt: string): string {
  if (updatedAt.length < 16) return updatedAt;
  return `${updatedAt.slice(5, 10)} ${updatedAt.slice(11, 16)}`;
}

/**
 * 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。
 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    main: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    title: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    agent: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    offline: {
      color: colors.warn,
      backgroundColor: colors.warnSoft,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      paddingVertical: 2,
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
    },
    time: {
      marginLeft: "auto",
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一条会话行。 */
export function SessionRow({ session, onOpen }: SessionRowProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    // 根节点带 `role="listitem"`：RNW 会渲染成 `<li>`，调用方的列表容器必须标 `role="list"`
    // （D1：`ul`/`li` 的列表语义用 role 补回）。`data-session-id` 只出现在这一个元素上。
    <View role="listitem" style={styles.row} dataSet={{ sessionId: session.id }}>
      <Pressable
        role="button"
        style={styles.main}
        dataSet={{ action: "open-session" }}
        onPress={() => {
          onOpen?.();
        }}
      >
        <Text style={styles.title} dataSet={{ sessionTitle: "true" }} numberOfLines={1}>
          {session.title}
        </Text>
        <Text style={styles.agent} dataSet={{ sessionAgent: "true" }} numberOfLines={1}>
          {session.agentName}
        </Text>
      </Pressable>
      <StatusPill state={session.state} />
      {session.originOfflineLabel === null ? null : (
        <Text style={styles.offline} dataSet={{ originOffline: "true" }}>
          {session.originOfflineLabel}
        </Text>
      )}
      <Text style={styles.time} dataSet={{ sessionUpdatedAt: session.updatedAt }}>
        {formatUpdatedAt(session.updatedAt)}
      </Text>
    </View>
  );
}
