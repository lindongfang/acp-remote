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
 * - **状态词**：只有 `pending` 追加，词表复用 `StatusPill` 的**唯一一份**
 *   `SESSION_STATE_LABEL`，但按原型 `.stword` 渲染成纯文字（不是带底色的胶囊），于是用户
 *   不必进入会话就知道有请求在等自己处理（等待原因另由会话内容说明）。
 *
 * 组件是纯函数：所有交互通过回调上抛，导航由页面负责。
 */

import type { ReactElement } from "react";
import type { ViewStyle } from "react-native";

import type { LocalActivityView, SessionStateView } from "../domain/session-summary";
import type { SessionRowModel } from "../features/directory-model";
import { SESSION_ACTIVITY_LABEL, SESSION_STATE_LABEL } from "./StatusPill";
import type { ColorToken, Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  spacing,
  useTheme,
} from "./theme";

/**
 * 状态圆点需要的、**解释「在等我们处理」**的一档：只有它追加状态词。
 *
 * 用「状态 → 是否有状态词」的显式映射（与 `domain/directory.ts` 的 `IS_ACTIVE` 同一写法），
 * 而不是在渲染路径上写 `=== "pending"`：新增状态时这里会编译不过，从而必须显式决定它有没有
 * 状态词。等待的具体原因（授权 / 提问）由会话内容表达，不再占用主状态词表。
 */
const NEEDS_STATE_WORD: Readonly<Record<SessionStateView, true | undefined>> = {
  pending: true,
  idle: undefined,
  running: undefined,
};

/**
 * 状态 → 圆点颜色 token。
 *
 * 与 `StatusPill` 的 `TONE_BY_STATE` **同一组** token（`ok`/`warn`/`ink3`），
 * 只是圆点要的是实心色而不是「前景 + 底色」一对。键取自同一个 `SessionStateView` 联合，
 * 因此新增状态时两处都会编译不过，配色不会单方面漂移。
 */
const DOT_TOKEN_BY_STATE: Record<SessionStateView, ColorToken> = {
  idle: "ink3",
  running: "ok",
  pending: "warn",
};

/** 每个状态在样式表里的键（显式列出，避免用字符串现拼样式名）。 */
type DotStyleKey = "dotIdle" | "dotRunning" | "dotPending";

const DOT_STYLE_KEY_BY_STATE: Record<SessionStateView, DotStyleKey> = {
  idle: "dotIdle",
  running: "dotRunning",
  pending: "dotPending",
};

/** 本机活跃程度 → 圆点样式键（语义相同的档共用一套 token）。 */
const DOT_STYLE_KEY_BY_ACTIVITY: Record<LocalActivityView, DotStyleKey> = {
  idle: "dotIdle",
  active: "dotRunning",
  pending: "dotPending",
};

/**
 * 本机活跃程度 → 是否追加状态词。
 *
 * 只有「待处理」需要文字（有请求在等自己处理）；`活跃` 与 `空闲` 只留圆点，避免把资源保留期
 * 画成一个新的任务状态。
 */
const ACTIVITY_NEEDS_STATE_WORD: Readonly<Record<LocalActivityView, true | undefined>> = {
  pending: true,
  idle: undefined,
  active: undefined,
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
    // 原型 `.row`：单行五字段，行高由 9px 上下内边距给出。
    row: {
      flexDirection: "row",
      // 原型桌面是单行；窄屏空间不足时允许换行，避免固定宽度的状态词/时间挤出容器。
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.md,
      paddingVertical: 9,
      paddingHorizontal: 18,
      borderBottomWidth: 1,
      borderBottomColor: colors.line2,
    },
    // 对话页左栏的紧凑行（原型 `.srow`）：只有圆点 / 标题 / 状态词 / 时间，没有行分隔线。
    // 目录页与详情页仍走 `row`（五字段 + 底边），两种形态互不影响。
    rowCompact: {
      flexDirection: "row",
      flexWrap: "nowrap",
      alignItems: "center",
      gap: spacing.sm + 2,
      paddingVertical: 9,
      paddingHorizontal: 16,
      borderBottomWidth: 0,
    },
    main: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.md,
    },
    ordinal: {
      minWidth: 18,
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: 11.5,
      textAlign: "right",
    },
    mainCompact: {
      flexWrap: "nowrap",
      gap: spacing.sm + 2,
    },
    dot: {
      width: 8,
      height: 8,
      borderRadius: 4,
      flexShrink: 0,
    },
    dotIdle: dot("idle"),
    dotRunning: dot("running"),
    dotPending: dot("pending"),
    title: {
      flexGrow: 1,
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    titleCompact: {
      fontSize: fontSize.sm + 0.5,
      lineHeight: 20,
    },
    // 原型 `.stword`：纯文字（不是胶囊），固定 70px 右对齐、警告色、半粗。
    stateWord: {
      flexShrink: 0,
      width: 70,
      textAlign: "right",
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: 11.5,
      lineHeight: 18.4,
      fontWeight: fontWeight.medium,
    },
    agent: {
      flexShrink: 0,
      width: 64,
      textAlign: "right",
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: 12.5,
      lineHeight: 20,
    },
    // 紧凑行的状态词不占固定 70px（左栏只有 300px，标题才是主体）。
    stateWordCompact: {
      width: "auto",
    },
    // 原型 `.when`：固定 96px 右对齐。
    time: {
      flexShrink: 0,
      width: 96,
      textAlign: "right",
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: 12.5,
      lineHeight: 20,
    },
    // 原型 `.srow .w`：紧凑行的相对时间不占固定宽度。
    timeCompact: {
      width: "auto",
      fontSize: 11.5,
      lineHeight: 18.4,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/**
 * 会话行的两种形态：
 *
 * - `default`：目录页与目录详情页的五字段行（原型 `.row`）；
 * - `compact`：对话页左栏的紧凑行（原型 `.srow`）——只有状态圆点、标题、状态词与相对时间，
 *   不渲染 Agent 名与行底边。目录页的既有断言不因这一档改变。
 */
export type SessionRowVariant = "default" | "compact";

export interface SessionRowProps {
  readonly ordinal?: number;
  readonly session: SessionRowModel;
  /** 打开这个会话（页面负责导航）；缺省时本行只是展示。 */
  readonly onOpen?: () => void;
  /** 行的形态；缺省 `default`（目录页五字段行）。 */
  readonly variant?: SessionRowVariant;
}

/** 一条会话行。 */
export function SessionRow({ session, onOpen, variant = "default", ordinal }: SessionRowProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const compact = variant === "compact";
  const main = session.activity;
  const needsStateWord = main.kind === "local"
    ? ACTIVITY_NEEDS_STATE_WORD[main.value] === true
    : NEEDS_STATE_WORD[main.value] === true;
  const stateWord = main.kind === "local"
    ? SESSION_ACTIVITY_LABEL[main.value]
    : SESSION_STATE_LABEL[main.value];

  return (
    // 根节点带 `role="listitem"`：RNW 会渲染成 `<li>`，调用方的列表容器必须标 `role="list"`
    // （D1：`ul`/`li` 的列表语义用 role 补回）。`data-session-id` 只出现在这一个元素上。
    <View
      role="listitem"
      style={compact ? [styles.row, styles.rowCompact] : styles.row}
      dataSet={{ sessionId: session.id, sessionRowVariant: variant }}
    >
      <Pressable
        role="button"
        style={compact ? [styles.main, styles.mainCompact] : styles.main}
        dataSet={{ action: "open-session" }}
        onPress={() => {
          onOpen?.();
        }}
      >
        {ordinal === undefined ? null : (
          <Text style={styles.ordinal} dataSet={{ sessionOrdinal: String(ordinal) }}>{ordinal}</Text>
        )}
        {/* 状态圆点：颜色按状态取；`data-session-state` 是**状态标识**（机器证据），不是文案。
            `accessibilityLabel` 与原型圆点上的 `title` 同一口径（给出状态标识）。 */}
        <View
          style={[
            styles.dot,
            styles[
              session.activity.kind === "local"
                ? DOT_STYLE_KEY_BY_ACTIVITY[session.activity.value]
                : DOT_STYLE_KEY_BY_STATE[session.activity.value]
            ],
          ]}
          accessibilityLabel={
            session.activity.kind === "local" ? SESSION_ACTIVITY_LABEL[session.activity.value] : session.activity.value
          }
          dataSet={{
            // 任务事实与主状态各留一份机器证据：旧合同/imported 时两者取值相同。
            sessionState: session.state,
            sessionStateSource: session.activity.kind,
            ...(session.activity.kind === "local"
              ? { sessionActivity: session.activity.value }
              : {}),
          }}
        />
        <Text
          style={compact ? [styles.title, styles.titleCompact] : styles.title}
          // 紧凑行不复用 `data-session-title`：对话页的会话头与左栏行同时在 DOM 里，两者共用
          // 同一个锚点会让「会话头的标题」在整页里不再是第一个命中。目录页五字段行保持原锚点。
          dataSet={compact ? { sessionRowTitle: "true" } : { sessionTitle: "true" }}
          numberOfLines={1}
        >
          {session.title}
        </Text>
        {/* 状态词只补「有请求在等」：词表来自 `StatusPill` 的同一份，但这里按原型画成纯文字
            （`.stword`），不是带底色的胶囊。 */}
        {!compact && needsStateWord ? (
          <Text
            style={compact ? [styles.stateWord, styles.stateWordCompact] : styles.stateWord}
            dataSet={{
              sessionState: session.state,
              ...(session.activity.kind === "local"
                ? { sessionActivity: session.activity.value }
                : {}),
            }}
          >
            {stateWord}
          </Text>
        ) : null}
        {/* 紧凑行按原型 `.srow` 只保留四项：Agent 名不在行上（它就是用户要求去掉的那一列）。 */}
        {compact ? null : (
          <Text style={styles.agent} dataSet={{ sessionAgent: "true" }} numberOfLines={1}>
            {session.agentName}
          </Text>
        )}
      </Pressable>
      <Text
        style={compact ? [styles.time, styles.timeCompact] : styles.time}
        dataSet={{ sessionUpdatedAt: session.updatedAt }}
      >
        {session.updatedAtLabel}
      </Text>
    </View>
  );
}
