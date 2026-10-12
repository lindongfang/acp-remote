/**
 * 「本设备没有收到过请求内容」的待处理交互清单（spec「授权与提问应答的可用边界」的场景
 * 「历史待处理项不可操作」；`design.md` D7）。
 *
 * ## 为什么这些条目需要单独的一面
 *
 * `session.read` 的 `pending_interactions` 读出项只有 `interactionId`/`kind`/`state`/`schema`
 * /`createdAt`——**没有**请求内容，也没有对应的事件（时间线上的授权卡是按事件画的）。若这一面
 * 不给出口，这些仍然等待处理的交互在客户端就**完全不可见**：用户会以为没有待处理项。
 * spec 要求它们呈现为**不可操作并说明原因**，而不是省略。
 *
 * ## 与时间线不重复
 *
 * 调用方只把「时间线上没有对应卡片」的交互交给本面板（有卡片的那条已经在时间线上带着同一份
 * 可用性呈现）。因此同一 `interactionId` 在页面上只出现一处。
 *
 * ## 仍然接应答按钮
 *
 * 面板照旧渲染 `InteractionAnswerButtons`（它在 `answerable` 之外返回 `null`）：万一某条**亲历**
 * 请求的时间线卡片因视图缺必填字段而退化成降级卡片，这条交互不会因此变成不可应答——按钮会在这里
 * 出现。正常情形下这些条目没有请求内容，`availability` 只能是 `not_answerable`，因此**没有**按钮。
 */

import type { ReactElement } from "react";

import type { InteractionAnswer, InteractionView } from "../domain/interaction";
import { InteractionAnswerButtons, InteractionAvailabilityNote } from "./InteractionAnswering";
import type { Palette } from "./theme";
import { StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface PendingInteractionsPanelProps {
  readonly interactions: readonly InteractionView[];
  /** 应答上抛（正常情形下这些条目没有可选项，因此不会渲染按钮）。 */
  readonly onAnswerInteraction?: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
}

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      gap: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.warn,
      borderRadius: radius.md,
      backgroundColor: colors.warnSoft,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    kicker: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    count: {
      color: colors.warn,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    rows: {
      gap: spacing.sm,
    },
    row: {
      gap: spacing.xs,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
    },
    title: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 没有请求内容的待处理交互清单。 */
export function PendingInteractionsPanel({
  interactions,
  onAnswerInteraction,
}: PendingInteractionsPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  return (
    <View style={styles.panel} dataSet={{ pendingInteractions: "list", pendingInteractionCount: interactions.length }}>
      <View style={styles.head}>
        <Text style={styles.kicker}>待处理交互</Text>
        <Text style={styles.count} dataSet={{ pendingInteractionCountLabel: String(interactions.length) }}>
          {`${interactions.length} 条`}
        </Text>
      </View>
      <Text style={styles.note} dataSet={{ pendingInteractionsNote: "true" }}>
        以下待处理项本设备没有收到过它的请求内容（时间线上没有对应事件），因此不可应答。
      </Text>
      <View style={styles.rows}>
        {interactions.map((interaction) => (
          <View
            key={interaction.interactionId}
            style={styles.row}
            dataSet={{
              pendingInteraction: interaction.interactionId,
              pendingInteractionKind: interaction.kind,
              pendingInteractionAvailability: interaction.availability.kind,
            }}
          >
            <Text style={styles.title} dataSet={{ pendingInteractionTitle: interaction.title ?? "unknown" }}>
              {interaction.title ?? (interaction.kind === "permission" ? "授权请求（请求内容未下发）" : "提问请求（请求内容未下发）")}
            </Text>
            <InteractionAnswerButtons
              interactionId={interaction.interactionId}
              availability={interaction.availability}
              onAnswerInteraction={onAnswerInteraction}
            />
            <InteractionAvailabilityNote availability={interaction.availability} />
          </View>
        ))}
      </View>
    </View>
  );
}
