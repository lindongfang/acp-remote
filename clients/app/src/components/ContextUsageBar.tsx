/**
 * 上下文占用胶囊（R17「未上报的上下文不得以零冒充」；`design.md` D7）。
 *
 * ## 形态：胶囊 + 展开明细（原型 `.ctx` / `.ctx .pop`）
 *
 * 已上报时胶囊是「进度条 + 占比 + 已用 / 上限」，点胶囊展开明细（已用、上限、占比 + 数字来源的
 * 说明）。明细内容**始终在 DOM**（收起只是 `display: none`），与同页的模型/模式弹层同一手法：
 * 判定因此能在静置的渲染结果里断言，不依赖交互。
 *
 * ## 三态在渲染上如何保持可区分
 *
 * `ContextUsageView` 是判别联合，本组件按 `kind` 渲染**结构不同**的分支：
 *
 * - `unreported`：没有进度条、**没有**数字槽位——占比与数值两个位置都是「—」，另给一句
 *   「未上报」的措辞；
 * - `zero_window`：同样是「—」，措辞为「未上报（窗口为零）」，与「从未上报」可区分；
 * - `known`：进度条 + 占比 + 已用 / 上限，数字经 `data-*` 原样进 DOM。
 *
 * 关键在于「进度条 + 两个数字」这一组槽位**只存在于已上报分支**：把未上报渲染成 `0 / 0` 或 `0%`
 * 在这个组件里没有表达方式，因此「未上报以零冒充」不是靠约定避免的。
 *
 * ## 两档状态色与阈值的唯一出处
 *
 * 原型按占比给胶囊加类（`pct>=85 → crit`、`pct>=60 → warn`）：`.ctx.warn` / `.ctx.crit` 换掉
 * 边框色、crit 另换底色，两支都把进度条填充色换成 `--warn` / `--danger`。这里的档位来自
 * `src/domain/context-usage.ts`——同一份阈值也是输入区告警条的判据。**未上报与窗口为零没有
 * 占比**，因此它们没有档位：不写档位锚点、不加任何状态色（原型这两态同样没有类），也就没有
 * 「凭 0 上色」的表达方式。
 *
 * ## 进度条宽度为什么走 `StyleSheet.create`
 *
 * 宽度是数据驱动的（`used / size` 算出的占比）。CSP 基线下没有 `style-src`，运行时拼出来的
 * `style=` 属性会被内核拒掉（`ui-stylesheet` 的判据正是「规则必须落在同源样式表里」），
 * 而 `react-native` 的 `DimensionValue` 是模板字面量类型、不接受运行期字符串。因此宽度按整
 * 百分点编译成原子类并缓存：同一档在整页里只产生一条规则，视觉上最多差 1%（精确占比在文字里）。
 * 缓存（而不是在模块加载期把 101 档全建出来）是刻意的：没有已上报用量的页面不必多挂 101 条规则。
 *
 * 本组件**不**禁用任何交互（也不接收能力参数）：R17 要求未上报不得阻断其它交互，
 * 而能力由对话页模型单独呈现，两者不混在一起。
 */

import { useState } from "react";
import type { ViewStyle } from "react-native";
import type { ReactElement } from "react";

import type { ContextUsageView } from "../domain";
import { contextUsageLabel, contextUsageLevel } from "../domain";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  View,
  fontFamily,
  fontSize,
  fontWeight,
  palettes,
  radius,
  spacing,
  useTheme,
  webStyle,
  zIndex,
} from "./theme";

export interface ContextUsageBarProps {
  readonly usage: ContextUsageView;
  readonly compact?: boolean;
}

/** 进度条尺寸（原型 `.ctxbar`：66 × 6）。 */
const TRACK_WIDTH = 66;
const TRACK_HEIGHT = 6;

/** 明细里那句「数字从哪来」的说明（两种状态共用；不含阿拉伯数字，未上报态整段没有数字）。 */
const CONTEXT_DETAIL_NOTE =
  "数字来自 Agent 上报的 usage，网页端不做推算；单位与统计口径由 Agent 决定。Agent 未上报时以「—」表示，不会用零冒充。";

/** 已编译的填充宽度类：按整百分点缓存，同一档只编译一次。 */
const FILL_WIDTH_CLASSES = new Map<number, ViewStyle>();
const RING_CLASSES = new Map<string, ViewStyle>();

function ringStyle(percent: number, colors: Palette): ViewStyle {
  const step = Math.max(0, Math.min(100, Math.round(percent)));
  const key = `${step}|${colors.panel2}`;
  const cached = RING_CLASSES.get(key);
  if (cached !== undefined) return cached;
  const compiled = StyleSheet.create({ ring: webStyle({
    borderWidth: 0,
    backgroundImage: `radial-gradient(closest-side, ${colors.panel2} 65%, transparent 70%), conic-gradient(${colors.accent} ${step * 3.6}deg, ${colors.line} 0deg)`,
  }) }).ring;
  RING_CLASSES.set(key, compiled);
  return compiled;
}

/** 占比 → 进度条填充的宽度类（四舍五入到整百分点，见文件头）。 */
function fillWidthStyle(percent: number): ViewStyle {
  const step = Math.max(0, Math.min(100, Math.round(percent)));
  const cached = FILL_WIDTH_CLASSES.get(step);
  if (cached !== undefined) return cached;
  // `width` 的百分比串在 `react-native` 的类型里是模板字面量：运行期拼出来的字符串只能经
  // `webStyle` 这个唯一断言点进去（它同样是网页专属取值与动态取值的出口）。
  const compiled: ViewStyle = StyleSheet.create({ fill: webStyle({ width: `${step}%` }) }).fill;
  FILL_WIDTH_CLASSES.set(step, compiled);
  return compiled;
}

/** 明细里的一行：标签 + 值（值带自己的锚点，便于逐条断言）。 */
function DetailRow({
  label,
  value,
  anchor,
}: {
  readonly label: string;
  readonly value: string;
  readonly anchor: "used" | "size" | "percent";
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const dataSet: Record<string, string> =
    anchor === "used"
      ? { contextDetailUsed: value }
      : anchor === "size"
        ? { contextDetailSize: value }
        : { contextDetailPercent: value };

  return (
    <View style={styles.detailRow}>
      <Text style={styles.detailKey}>{label}</Text>
      <Text style={styles.detailValue} dataSet={dataSet}>
        {value}
      </Text>
    </View>
  );
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    wrap: {
      position: "relative",
    },
    compactChip: {
      minWidth: 40,
      height: 40,
      alignItems: "center",
      justifyContent: "center",
    },
    ring: {
      width: 20,
      height: 20,
      borderRadius: 10,
      borderWidth: 3,
      borderColor: colors.line,
      borderTopColor: colors.ink3,
    },
    chip: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel2,
    },
    track: {
      width: TRACK_WIDTH,
      height: TRACK_HEIGHT,
      borderRadius: 4,
      backgroundColor: colors.line,
      overflow: "hidden",
    },
    fill: {
      height: TRACK_HEIGHT,
      borderRadius: 4,
      backgroundColor: colors.ok,
    },
    // 两档状态色（原型 `.ctx.warn` / `.ctx.crit` 与 `.ctx.warn .ctxbar i` /
    // `.ctx.crit .ctxbar i`）：warn 只换边框与填充色，crit 另把胶囊底色换成 `--danger-soft`。
    chipWarn: {
      borderColor: colors.warn,
    },
    chipCrit: {
      borderColor: colors.danger,
      backgroundColor: colors.dangerSoft,
    },
    fillWarn: {
      backgroundColor: colors.warn,
    },
    fillCrit: {
      backgroundColor: colors.danger,
    },
    pct: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
    },
    num: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    state: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    // 明细浮在胶囊**上方**（原型 `.ctx .pop`：`position:absolute;bottom:calc(100% + 8px)`），
    // 因此展开不会把输入区推下去；收起时 `display:none`，布局没有变化。
    detail: {
      position: "absolute",
      right: 0,
      bottom: "100%",
      gap: spacing.xs,
      minWidth: 270,
      marginBottom: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      boxShadow: colors.shadow,
      zIndex: zIndex.topBar,
    },
    detailHidden: {
      display: "none",
    },
    detailTitle: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.medium,
    },
    detailRow: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.md,
    },
    detailKey: {
      minWidth: 84,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    detailValue: {
      flexShrink: 1,
      color: colors.ink2,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      fontWeight: fontWeight.value,
    },
    detailNote: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 上下文占用胶囊。 */
export function ContextUsageBar({ usage, compact = false }: ContextUsageBarProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [expanded, setExpanded] = useState(false);
  const label = contextUsageLabel(usage);
  // 只有已上报才拿得到数字：其余两态走「—」那一支，进度条与数字槽位在那支里不存在。
  const known = usage.kind === "known" ? usage : null;
  // 档位同样只在已上报时判：未上报 / 窗口为零没有占比可判，也不得凭 0 上色。
  const level = known === null ? null : contextUsageLevel(known.percent);
  // 档位 → 两处样式覆盖（胶囊的边框/底色、进度条填充色）。`ok` 档与原型「不加类」的形态一致：
  // 不加任何覆盖。
  const tone =
    level === "crit"
      ? { chip: styles.chipCrit, fill: styles.fillCrit }
      : level === "warn"
        ? { chip: styles.chipWarn, fill: styles.fillWarn }
        : null;

  return (
    <View style={styles.wrap} dataSet={{ contextState: usage.kind, contextLabel: label }}>
      <Pressable
        role="button"
        aria-label={`上下文占用：${label}（点击展开明细）`}
        style={compact ? styles.compactChip : tone === null ? styles.chip : [styles.chip, tone.chip]}
        dataSet={{
          action: "toggle-context-detail",
          contextChip: "true",
          contextChipOpen: String(expanded),
          // 未上报 / 窗口为零**不写**档位键：没有占比就没有档位，也就没有「凭 0 上色」的表达方式。
          ...(level === null ? {} : { contextChipLevel: level }),
        }}
        onPress={() => {
          setExpanded(!expanded);
        }}
      >
        {compact ? (
          <View style={known === null ? styles.ring : [styles.ring, ringStyle(known.percent, palettes[theme.dark ? "dark" : "light"])]}
            role="img" aria-label={known === null ? "上下文未上报" : `上下文已用 ${known.percent.toFixed(1)}%`} />
        ) : known === null ? (
          <>
            {/* 原型只在「窗口为零」时才补一句可区分说明；从未上报就只留「—」，不把状态词堆进胶囊。 */}
            {usage.kind === "zero_window" ? <Text style={styles.state}>{label}</Text> : null}
            {/* 占比与数值的位置以「—」表示：未上报没有数字可显示，也不用零占位。 */}
            <Text style={styles.pct}>—</Text>
            <Text style={styles.num}>上下文 — / —</Text>
          </>
        ) : (
          <>
            <View
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={known.percent}
              style={styles.track}
              dataSet={{ contextTrack: "true" }}
            >
              <View
                style={[styles.fill, fillWidthStyle(known.percent), tone?.fill]}
                dataSet={{ contextFill: "true", ...(level === null ? {} : { contextFillLevel: level }) }}
              />
            </View>
            <Text style={styles.pct} dataSet={{ contextPercent: known.percent.toFixed(1) }}>
              {`${known.percent.toFixed(1)}%`}
            </Text>
            <Text style={styles.num} dataSet={{ contextUsed: known.used, contextSize: known.size }}>
              {`上下文 ${known.used} / ${known.size}`}
            </Text>
          </>
        )}
      </Pressable>

      {/* 明细始终在 DOM：收起只是 `display: none`（与模型/模式弹层同一形态）。 */}
      <View
        style={expanded ? styles.detail : [styles.detail, styles.detailHidden]}
        dataSet={{ contextDetail: "true", contextDetailOpen: String(expanded) }}
      >
        <Text style={styles.detailTitle}>上下文占用</Text>
        <DetailRow label="已用" value={known === null ? "未上报" : known.used} anchor="used" />
        <DetailRow label="上限" value={known === null ? "未知" : known.size} anchor="size" />
        <DetailRow
          label="占比"
          value={known === null ? "无法计算" : `${known.percent.toFixed(1)}%`}
          anchor="percent"
        />
        <Text style={styles.detailNote}>{CONTEXT_DETAIL_NOTE}</Text>
      </View>
    </View>
  );
}
