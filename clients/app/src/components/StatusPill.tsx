/**
 * 会话状态徽标（共享展示原语）。
 *
 * 消费 `src/domain/` 的 `SessionStateView`，**不**接收任何协议字段。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`Text` + `StyleSheet`，配色取自主题 token。
 * `data-session-state` 与 `SESSION_STATE_LABEL` 词表是**三态**：前者是 e2e 的机器证据，
 * 后者是对外契约；旧七态取值不再出现。
 */

import type { ReactElement } from "react";
import type { TextStyle } from "react-native";

import type { LocalActivityView, SessionStateView } from "../domain/session-summary";
import type { ColorToken, Palette } from "./theme";
import { StyleSheet, Text, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/**
 * 状态 → 展示文案的静态词表。
 *
 * 导出给目录页的会话行使用：那里按原型呈现**纯文字**状态词（`.stword`），不能为了复用
 * 一份文案就再抄一张表；胶囊与本行共用同一份词表，新增状态时两处都不会漂移。
 */
export const SESSION_STATE_LABEL: Record<SessionStateView, string> = {
  idle: "空闲",
  running: "运行中",
  pending: "待处理",
};

/**
 * 状态 → 胶囊配色（前景 token / 背景 token）。
 *
 * 键取自同一个 `SessionStateView` 联合，因此新增状态时这里会编译不过——配色不会与
 * `SESSION_STATE_LABEL` 词表漂移。映射的是**呈现**（哪种状态用哪种底色），不改变状态语义，
 * 也不参与任何判定。
 */
const TONE_BY_STATE: Record<SessionStateView, { readonly fg: ColorToken; readonly bg: ColorToken }> = {
  idle: { fg: "ink2", bg: "panel2" },
  running: { fg: "ok", bg: "okSoft" },
  // 「有有效未解决交互」在等我们处理：用警告色（原型 `.state.waiting` 的口径）。
  pending: { fg: "warn", bg: "warnSoft" },
};

/**
 * 状态字形：补一个图形标记，让徽标不靠颜色与文字才能区分（原型的状态圆点/字形同一口径）。
 *
 * 键集合同样由 `SessionStateView` 保证完整：新增状态时这里会编译不过，不会漏一个字形。
 */
const GLYPH_BY_STATE: Record<SessionStateView, string> = {
  idle: "○",
  running: "●",
  // 有待处理交互：与状态词的「⏸」同一口径。
  pending: "⏸",
};

/** 每个状态在样式表里的键（显式列出，避免用字符串现拼样式名）。 */
type ToneStyleKey = "idle" | "running" | "pending";

const STYLE_KEY_BY_STATE: Record<SessionStateView, ToneStyleKey> = {
  idle: "idle",
  running: "running",
  pending: "pending",
};

/**
 * 本机活跃程度的展示词表（`core.local-session-lifecycle.v1`）。
 *
 * `活跃` 与任务态的 `运行中` **不是同义词**：资源已恢复但没有任务时仍是 `active`，用户据此
 * 知道可以继续发送；任务执行、排队与等待原因仍由时间线与胶囊旁的细节表达。
 */
export const SESSION_ACTIVITY_LABEL: Record<LocalActivityView, string> = {
  idle: "空闲",
  active: "活跃",
  pending: "待处理",
};

/** 活跃程度的字形：与任务三态共用一个视觉语言（不新增第四个符号）。 */
const GLYPH_BY_ACTIVITY: Record<LocalActivityView, string> = {
  idle: "○",
  active: "●",
  pending: "⏸",
};

/** 活跃程度的配色：语义相同的档共用同一组 token。 */
const TONE_BY_ACTIVITY: Record<LocalActivityView, { readonly fg: ColorToken; readonly bg: ColorToken }> = {
  idle: TONE_BY_STATE.idle,
  active: TONE_BY_STATE.running,
  pending: TONE_BY_STATE.pending,
};

/** 活跃程度 → 样式键（沿用任务三态的样式表）。 */
const STYLE_KEY_BY_ACTIVITY: Record<LocalActivityView, ToneStyleKey> = {
  idle: "idle",
  active: "running",
  pending: "pending",
};

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  /** 一种状态的配色：前景色 + 底色取同一组 token（浅/深各自成立）。 */
  const tone = (state: SessionStateView): TextStyle => ({
    color: colors[TONE_BY_STATE[state].fg],
    backgroundColor: colors[TONE_BY_STATE[state].bg],
  });
  return StyleSheet.create({
    pill: {
      // 单行会话头里与标题/Agent 名同轴对齐（唯一消费方是会话头）。
      alignSelf: "center",
      paddingVertical: 2,
      paddingHorizontal: spacing.sm,
      borderRadius: radius.lg,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    idle: tone("idle"),
    running: tone("running"),
    pending: tone("pending"),
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export interface StatusPillProps {
  /** 任务三态（始终是任务事实；`localActivity` 缺席时它就是主标签）。 */
  readonly state: SessionStateView;
  /** 协商后的本机活跃程度；非空时覆盖主标签的文案/字形，`data-session-state` 仍是任务态。 */
  readonly localActivity?: LocalActivityView | null;
}

/**
 * 渲染一个状态徽标。
 *
 * 文案是 `SESSION_STATE_LABEL` 的那三个词，`data-session-state` 仍是**状态标识**（不是文案）：
 * 两处都不能被这块胶囊的样式改动碰到。字形（`● / ⏸ / ○`）只是文案前缀，另以
 * `data-session-state-glyph` 进 DOM；色调以 `data-session-state-tone`（底色 token）留作机器证据。
 */
export function StatusPill({ state, localActivity }: StatusPillProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  const local = localActivity ?? null;
  const glyph = local === null ? GLYPH_BY_STATE[state] : GLYPH_BY_ACTIVITY[local];
  const label = local === null ? SESSION_STATE_LABEL[state] : SESSION_ACTIVITY_LABEL[local];
  const tone = local === null ? TONE_BY_STATE[state].bg : TONE_BY_ACTIVITY[local].bg;
  const styleKey = local === null ? STYLE_KEY_BY_STATE[state] : STYLE_KEY_BY_ACTIVITY[local];
  return (
    <Text
      style={[styles.pill, styles[styleKey]]}
      dataSet={{
        // 任务事实始终在 DOM 里；本机活跃程度另有一份机器证据，二者不互相冒充。
        sessionState: state,
        sessionStateGlyph: glyph,
        // 配色的机器证据：`pending` 的底色是 `warnSoft`。
        sessionStateTone: tone,
        sessionStateSource: local === null ? "task" : "local",
        ...(local === null ? {} : { sessionActivity: local }),
      }}
    >
      {`${glyph} ${label}`}
    </Text>
  );
}
