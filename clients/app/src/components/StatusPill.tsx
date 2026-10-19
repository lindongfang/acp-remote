/**
 * 会话状态徽标（共享展示原语）。
 *
 * 消费 `src/domain/` 的 `SessionStateView`，**不**接收任何协议字段。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`Text` + `StyleSheet`，配色取自主题 token。
 * `data-session-state` 与 `SESSION_STATE_LABEL` 词表都**不变**——前者是 e2e 的机器证据，
 * 后者是对外契约。
 */

import type { ReactElement } from "react";
import type { TextStyle } from "react-native";

import type { SessionStateView } from "../domain/session-summary";
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
  queued: "排队中",
  running: "运行中",
  waiting_input: "等你回复",
  waiting_permission: "待授权",
  failed: "失败",
  closed: "已关闭",
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
  queued: { fg: "accentInk", bg: "accentSoft" },
  running: { fg: "ok", bg: "okSoft" },
  // 等你回复与待授权同属「在等我们处理」：两档都用警告色（原型 `.state.waiting` 的口径），
  // 此前 `waiting_input` 用的思考紫会把「等你」说得比「待授权」更轻。
  waiting_input: { fg: "warn", bg: "warnSoft" },
  waiting_permission: { fg: "warn", bg: "warnSoft" },
  failed: { fg: "danger", bg: "dangerSoft" },
  closed: { fg: "ink3", bg: "panel2" },
};

/**
 * 状态字形：补一个图形标记，让徽标不靠颜色与文字才能区分（原型的状态圆点/字形同一口径）。
 *
 * 键集合同样由 `SessionStateView` 保证完整：新增状态时这里会编译不过，不会漏一个字形。
 */
const GLYPH_BY_STATE: Record<SessionStateView, string> = {
  idle: "○",
  queued: "●",
  running: "●",
  waiting_input: "⏸",
  waiting_permission: "⏸",
  failed: "✕",
  closed: "○",
};

/** 每个状态在样式表里的键（显式列出，避免用字符串现拼样式名）。 */
type ToneStyleKey =
  | "idle"
  | "queued"
  | "running"
  | "waitingInput"
  | "waitingPermission"
  | "failed"
  | "closed";

const STYLE_KEY_BY_STATE: Record<SessionStateView, ToneStyleKey> = {
  idle: "idle",
  queued: "queued",
  running: "running",
  waiting_input: "waitingInput",
  waiting_permission: "waitingPermission",
  failed: "failed",
  closed: "closed",
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
    queued: tone("queued"),
    running: tone("running"),
    waitingInput: tone("waiting_input"),
    waitingPermission: tone("waiting_permission"),
    failed: tone("failed"),
    closed: tone("closed"),
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export interface StatusPillProps {
  readonly state: SessionStateView;
}

/**
 * 渲染一个状态徽标。
 *
 * 文案仍是 `SESSION_STATE_LABEL` 的那七个词，`data-session-state` 仍是**状态标识**（不是文案）：
 * 两处都不能被这块胶囊的样式改动碰到。字形（`● / ⏸ / ✕ / ○`）只是文案前缀，另以
 * `data-session-state-glyph` 进 DOM；色调以 `data-session-state-tone`（底色 token）留作机器证据。
 */
export function StatusPill({ state }: StatusPillProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  const glyph = GLYPH_BY_STATE[state];
  return (
    <Text
      style={[styles.pill, styles[STYLE_KEY_BY_STATE[state]]]}
      dataSet={{
        sessionState: state,
        sessionStateGlyph: glyph,
        // 配色的机器证据：`waiting_input` 与 `waiting_permission` 都是 `warnSoft`。
        sessionStateTone: TONE_BY_STATE[state].bg,
      }}
    >
      {`${glyph} ${SESSION_STATE_LABEL[state]}`}
    </Text>
  );
}
