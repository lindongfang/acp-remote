/**
 * 一条降级条目的**内容**呈现（卡片 + 原因说明 + 诊断入口）：会话时间线与节点级降级面共用。
 *
 * ## 为什么共用而不是各画一遍
 *
 * 会话级降级（`ConversationPageModel.timeline.entries` 里的 `degraded`）与节点级降级
 * （`HostPanelModel.nodeDegraded`，没有会话的事件如 `device.revoked`）说的是同一件事——
 * 「被消费的事件没有专用呈现或内容缺失」。两处各画一遍会立刻分叉：一处记得区分三类不支持、
 * 另一处忘了，于是同一份数据在两个面上给出两种归因（R18 的 MAJOR-2 就是这么来的）。
 * 因此呈现只有这一份，两个面各自只提供**容器与锚点**
 * （时间线是 `data-degraded-entry`，节点级是 `data-node-degraded-entry`）。
 *
 * ## 归因逐条对齐（不得兜底）
 *
 * `no_dedicated_view` 与 `unsupported` 才宣称「当前客户端未提供专用视图」；原文未下发
 * （`raw_not_synced`）与二进制未下发（`binary_not_delivered`）时客户端**有**视图、缺的是内容——
 * 把它们兜底成「客户端不支持」会让用户按错误原因排查。
 *
 * ## 原文只在诊断出口里出现
 *
 * 本组件不渲染 `acp.rawJson`（也没有这样的入参）：内容缺失时只有原因与元数据，原文的查看入口
 * 由调用方接（会话级接仓库的 `openDiagnostics`，节点级由主机面板接同一份抽屉）。
 */

import type { ReactElement } from "react";

import type { DegradedEventModel } from "../features/degradation-model";
import { RAW_NOT_SYNCED_LABELS } from "../features/degradation-model";
import { DegradedEventCard } from "./DegradedEventCard";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface DegradedEntryContentProps {
  readonly model: DegradedEventModel;
  /** 打开这条事件的诊断抽屉；缺省时**不**渲染入口（不是渲染一个点了没反应的按钮）。 */
  readonly onOpenDiagnostics?: ((eventId: string) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    content: {
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    alert: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    entryButton: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    entryText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一条降级条目的内容。 */
export function DegradedEntryContent({ model, onOpenDiagnostics }: DegradedEntryContentProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const reason = model.reason;
  const unsupportedBy = reason.kind === "unsupported" ? reason.by : null;
  const noDedicatedView = reason.kind === "unsupported" || reason.kind === "no_dedicated_view";
  const raw =
    model.raw.kind === "available"
      ? ({ kind: "available", rawJson: model.raw.rawJson } as const)
      : model.raw.kind === "not_synced"
        ? ({ kind: "not_synced", reason: model.raw.reason } as const)
        : ({
            kind: "binary_not_fetched",
            // 非二进制未下发的降级原因走不到这里；万一走到，也不编造 mimeType。
            mimeType: reason.kind === "binary_not_delivered" ? (reason.mimeType ?? reason.uri ?? "unknown") : "unknown",
          } as const);

  return (
    <View style={styles.content} dataSet={{ degradedEventType: model.eventType }}>
      <DegradedEventCard
        eventType={model.eventType}
        unsupportedBy={unsupportedBy}
        noDedicatedView={noDedicatedView}
        raw={raw}
      />
      {reason.kind === "raw_not_synced" ? (
        <Text style={styles.note} dataSet={{ rawNotSynced: reason.reason }}>
          {RAW_NOT_SYNCED_LABELS[reason.reason]}
        </Text>
      ) : null}
      {reason.kind === "unsupported" ? (
        <Text style={styles.note} dataSet={{ unsupportedDetail: reason.by }}>
          {reason.detail}
        </Text>
      ) : null}
      {reason.kind === "binary_not_delivered" ? (
        <Text style={styles.note} dataSet={{ binaryNotDelivered: reason.mimeType ?? "unknown" }}>
          结构化事件已收到，但它引用的二进制内容没有下发到本设备。
        </Text>
      ) : null}
      {onOpenDiagnostics === undefined ? null : (
        <Pressable
          role="button"
          style={styles.entryButton}
          dataSet={{ action: "open-diagnostics", eventId: model.eventId }}
          onPress={() => {
            onOpenDiagnostics(model.eventId);
          }}
        >
          <Text style={styles.entryText}>查看原文与元数据</Text>
        </Pressable>
      )}
    </View>
  );
}
