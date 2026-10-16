/**
 * 会话行（目录页的内联会话行与目录详情页共用）。
 *
 * 五字段与原型 `.row` 一致：**状态圆点、标题、状态词、Agent 名称、相对时间**。空标题呈现为
 * 「未命名会话」——那一段由视图模型给出（`SessionRowModel.title`），组件不再自己判断。
 *
 * ## 两条 MUST NOT 在渲染上的落点
 *
 * - **不呈现路径**：props 里只有 `SessionRowModel`，它的字段里没有任何路径；组件因此连
 *   「顺手把路径也显示出来」的表达方式都没有（R15 的负向断言在 `r15-directory-list.test.ts`
 *   与 e2e 里对渲染结果再核对一次）。
 * - **相对时间不自算**：时间渲染的是模型给出的 `updatedAtLabel`（由 `formatRelativeTime`
 *   按**注入的**时钟算出，D3）。组件里再写一遍格式化就等于各页各一套时区/相对口径；
 *   精确值另经 `data-session-updated-at` 原样进 DOM，需要逐字断言时从属性取。
 *
 * ## 状态的两处呈现
 *
 * - **圆点**：所有状态都画，颜色按状态取主题 token（原型 `.dot.<state>`）。它同时带
 *   `data-session-state`（状态标识，不是文案）——那是 e2e 的机器证据。
 * - **状态词**：只有「等待输入 / 等待授权」两档追加，复用 `StatusPill` 的**唯一一份**词表与
 *   配色，于是用户不必进入会话就知道在等什么（原型的 `stword` 也只在这两档非空）。
 *
 * 组件是纯函数：所有交互通过回调上抛，导航由页面负责。
 */

import type { ReactElement } from "react";
import type { ViewStyle } from "react-native";

import type { SessionStateView } from "../domain/session-summary";
import type { SessionRowModel } from "../features/directory-model";
import { StatusPill } from "./StatusPill";
import type { ColorToken, Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/**
 * 状态圆点需要的、**解释「在等什么」**的两档：只有它们追加状态词。
 *
 * 用「状态 → 是否有状态词」的显式映射（与 `domain/directory.ts` 的 `IS_ACTIVE` 同一写法），
 * 而不是在渲染路径上写 `=== "waiting_input" || === "waiting_permission"`：新增状态时这里会
 * 编译不过，从而必须显式决定它有没有状态词。
 */
const NEEDS_STATE_WORD: Readonly<Record<SessionStateView, true | undefined>> = {
  waiting_input: true,
  waiting_permission: true,
  idle: undefined,
  queued: undefined,
  running: undefined,
  failed: undefined,
  closed: undefined,
};

/**
 * 状态 → 圆点颜色 token。
 *
 * 与 `StatusPill` 的 `TONE_BY_STATE` **同一组** token（`ok`/`warn`/`danger`/`accent`/`ink3`），
 * 只是圆点要的是实心色而不是「前景 + 底色」一对。键取自同一个 `SessionStateView` 联合，
 * 因此新增状态时两处都会编译不过，配色不会单方面漂移。
 */
const DOT_TOKEN_BY_STATE: Record<SessionStateView, ColorToken> = {
  idle: "ink3",
  queued: "accent",
  running: "ok",
  waiting_input: "warn",
  waiting_permission: "warn",
  failed: "danger",
  closed: "ink3",
};

/** 每个状态在样式表里的键（显式列出，避免用字符串现拼样式名）。 */
type DotStyleKey =
  | "dotIdle"
  | "dotQueued"
  | "dotRunning"
  | "dotWaitingInput"
  | "dotWaitingPermission"
  | "dotFailed"
  | "dotClosed";

const DOT_STYLE_KEY_BY_STATE: Record<SessionStateView, DotStyleKey> = {
  idle: "dotIdle",
  queued: "dotQueued",
  running: "dotRunning",
  waiting_input: "dotWaitingInput",
  waiting_permission: "dotWaitingPermission",
  failed: "dotFailed",
  closed: "dotClosed",
};

/** 按调色板建样式表：浅/深各建一次（模块加载时），渲染路径上不再新建样式对象。 */
function buildStyles(colors: Palette) {
  /**
   * 一种状态的圆点：实心色取该状态的 token（浅/深各自成立）。
   *
   * 圆点在语义上是**视图**元素，因此这里显式标注 `ViewStyle`：`styles[DOT_STYLE_KEY_BY_STATE[...]]`
   * 的推导结果才会只落在 `ViewStyle` 家族，`View` 的 `style` 才不需要断言。若标成 `TextStyle`，
   * 圆点各键的样式类型会带上文字属性，索引出来就成了 `ViewStyle | TextStyle`，`exactOptionalPropertyTypes`
   * 下 `View` 的重载会拒绝（这正是本文件此前那条 `tsc` 错误的根因）。
   */
  const dot = (state: SessionStateView): ViewStyle => ({ backgroundColor: colors[DOT_TOKEN_BY_STATE[state]] });
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
      alignItems: "center",
      gap: spacing.sm,
    },
    dot: {
      width: 8,
      height: 8,
      borderRadius: 4,
      flexShrink: 0,
    },
    dotIdle: dot("idle"),
    dotQueued: dot("queued"),
    dotRunning: dot("running"),
    dotWaitingInput: dot("waiting_input"),
    dotWaitingPermission: dot("waiting_permission"),
    dotFailed: dot("failed"),
    // 已关闭的会话圆点更淡（原型 `.dot.closed{opacity:.55}`）。
    dotClosed: { ...dot("closed"), opacity: 0.55 },
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

export interface SessionRowProps {
  readonly session: SessionRowModel;
  /** 打开这个会话（页面负责导航）；缺省时本行只是展示。 */
  readonly onOpen?: () => void;
}

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
        {/* 状态圆点：颜色按状态取；`data-session-state` 是**状态标识**（机器证据），不是文案。
            `accessibilityLabel` 与原型圆点上的 `title` 同一口径（给出状态标识）。 */}
        <View
          style={[styles.dot, styles[DOT_STYLE_KEY_BY_STATE[session.state]]]}
          accessibilityLabel={session.state}
          dataSet={{ sessionState: session.state }}
        />
        <Text style={styles.title} dataSet={{ sessionTitle: "true" }} numberOfLines={1}>
          {session.title}
        </Text>
        {/* 状态词只补「在等什么」：词表与配色都来自 `StatusPill`，不在这里另抄一份。 */}
        {NEEDS_STATE_WORD[session.state] === true ? <StatusPill state={session.state} /> : null}
        <Text style={styles.agent} dataSet={{ sessionAgent: "true" }} numberOfLines={1}>
          {session.agentName}
        </Text>
      </Pressable>
      {session.originOfflineLabel === null ? null : (
        <Text style={styles.offline} dataSet={{ originOffline: "true" }}>
          {session.originOfflineLabel}
        </Text>
      )}
      <Text style={styles.time} dataSet={{ sessionUpdatedAt: session.updatedAt }}>
        {session.updatedAtLabel}
      </Text>
    </View>
  );
}
