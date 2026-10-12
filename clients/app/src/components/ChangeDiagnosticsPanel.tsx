/**
 * 一处改动的 ACP 原文与元数据（spec「变更文件的呈现边界」的场景「原文是诊断入口而不是渲染源」；
 * `design.md` D8）。
 *
 * ## 原文在这里是**不可信文本**，不是渲染源
 *
 * `rawJson` 只经 `Text` 原样呈现（RNW 会转义），既不做 JSON 解析、也不据它生成差异行——
 * 「把原文解析后当作结构化差异渲染」在这里没有表达方式。原文未下发（`not_synced`）时只有原因与
 * 元数据，**没有**可渲染的原文槽位：渲染一段占位 JSON 假装完整会违反 spec。
 *
 * ## 与降级抽屉的分工
 *
 * `DiagnosticsDrawer` 承载的是**降级条目**（覆盖未命中或内容缺失）的诊断；走专用视图的改动没有
 * 降级原因，因此它的诊断入口落在本组件（右栏），而不是硬塞进抽屉去谎报归因。
 */

import type { ReactElement } from "react";

import type { FileChangeDiagnostics } from "../features/conversation-timeline";
import { RAW_NOT_SYNCED_LABELS } from "../features/degradation-model";
import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface ChangeDiagnosticsPanelProps {
  readonly diagnostics: FileChangeDiagnostics;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      gap: spacing.xs,
      marginTop: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    kicker: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    row: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    label: {
      minWidth: 88,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    value: {
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    raw: {
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一行「标签 + 取值」。 */
function MetaRow({ label, value }: { readonly label: string; readonly value: string }): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.row}>
      <Text style={styles.label}>{label}</Text>
      <Text style={styles.value}>{value}</Text>
    </View>
  );
}

/** 一处改动的原文与元数据。 */
export function ChangeDiagnosticsPanel({ diagnostics }: ChangeDiagnosticsPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const raw = diagnostics.raw;

  return (
    <View style={styles.panel} dataSet={{ changeDiagnostics: diagnostics.eventId }}>
      <View style={styles.head}>
        <Text style={styles.kicker} dataSet={{ diagnosticsEventType: diagnostics.eventType }}>
          {diagnostics.eventType}
        </Text>
        <Text style={styles.kicker} dataSet={{ diagnosticsCreatedAt: diagnostics.createdAt }}>
          {diagnostics.createdAt}
        </Text>
      </View>
      <MetaRow label="事件 ID" value={diagnostics.eventId} />
      <MetaRow label="会话内序号" value={diagnostics.sessionSequence ?? "不适用"} />
      <MetaRow label="全局序号" value={diagnostics.globalSequence ?? "不适用"} />

      {raw.kind === "available" ? (
        // 原文按不可信文本原样呈现（不解析、不据此渲染差异行）。
        <Text style={styles.raw} dataSet={{ rawJson: "available", rawByteLength: raw.byteLength }}>
          {raw.rawJson}
        </Text>
      ) : raw.kind === "not_synced" ? (
        <Text style={styles.note} dataSet={{ rawUnavailable: raw.reason, rawByteLength: raw.byteLength }}>
          {`${RAW_NOT_SYNCED_LABELS[raw.reason]}；字节长度 ${raw.byteLength}${
            raw.sha256 === null ? "" : `；摘要 ${raw.sha256}`
          }`}
        </Text>
      ) : (
        <Text style={styles.note} dataSet={{ rawUnavailable: "absent" }}>
          该事件没有随附原文。
        </Text>
      )}

      {/* 事件视图：与抽屉同一口径（原样 JSON 文本，供诊断核对）。 */}
      <Text style={styles.raw} dataSet={{ rawView: "event-view" }}>
        {JSON.stringify(diagnostics.view, null, 2)}
      </Text>
    </View>
  );
}
