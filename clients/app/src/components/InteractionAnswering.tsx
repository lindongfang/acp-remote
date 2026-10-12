/**
 * 待处理交互（授权 / 提问）的**应答呈现**：可选项按钮组与四种边界的说明。时间线上的授权卡与
 * 「本设备没有收到过请求内容」的待处理项清单共用这一份，避免同一条判定在两处给出两种措辞。
 *
 * ## 为什么按钮组只在 `answerable` 时存在
 *
 * `InteractionAvailability` 是判别联合（`domain/interaction.ts`）：`answerable` 之外的四态
 * （仅历史读到 / 会话已关闭 / 没有可选项 / 已提交待确认 / 已由某设备处理）**都没有**可选项数据，
 * 因此这里画不出按钮——这正是 spec MUST NOT「呈现无法成功的应答按钮」的结构性保证。
 *
 * 各态的措辞互不相同（`submitting` / `handled_here` / `handled_elsewhere` / `not_answerable`
 * 的三种原因），合并成一句笼统的「不可用」会让用户按错误原因排查。
 */

import type { ReactElement } from "react";

import type { InteractionAnswer, InteractionAvailability } from "../domain/interaction";
import { INTERACTION_UNAVAILABLE_LABELS } from "../domain/interaction";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    actions: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
    answer: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.accent,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
    },
    answerText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
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
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

export interface InteractionAnswerButtonsProps {
  readonly interactionId: string;
  readonly availability: InteractionAvailability;
  /** 应答上抛；缺省时**不**渲染按钮（不是渲染一个点了没反应的按钮）。 */
  readonly onAnswerInteraction?: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
}

/** 应答按钮组：可选项只来自这次请求自身的下发内容（模型给的可选项）。 */
export function InteractionAnswerButtons({
  interactionId,
  availability,
  onAnswerInteraction,
}: InteractionAnswerButtonsProps): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (availability.kind !== "answerable" || onAnswerInteraction === undefined) return null;

  return (
    <View style={styles.actions}>
      {availability.answers.map((answer) => (
        <Pressable
          key={answer.via === "option" ? `option:${answer.optionId}` : `action:${answer.action}`}
          role="button"
          style={styles.answer}
          dataSet={
            answer.via === "option"
              ? { answerOption: answer.optionId, answerInteractionId: interactionId }
              : { answerAction: answer.action, answerInteractionId: interactionId }
          }
          onPress={() => {
            onAnswerInteraction(interactionId, answer);
          }}
        >
          <Text style={styles.answerText}>{answer.label}</Text>
        </Pressable>
      ))}
    </View>
  );
}

/** 一条交互边界的说明；`answerable` 时没有说明可讲（返回 `null`）。 */
export function InteractionAvailabilityNote({
  availability,
}: {
  readonly availability: InteractionAvailability;
}): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (availability.kind === "answerable") return null;
  if (availability.kind === "submitting") {
    return (
      <Text style={styles.note} dataSet={{ interactionSubmitting: availability.requestId }}>
        应答已提交，等待服务端确认（在收到终态前不呈现为已处理）。
      </Text>
    );
  }
  if (availability.kind === "handled_here") {
    return (
      <Text style={styles.note} dataSet={{ interactionHandled: "this-device" }}>
        {`已由本设备处理：${availability.resolution}`}
      </Text>
    );
  }
  if (availability.kind === "handled_elsewhere") {
    return (
      <Text
        style={styles.alert}
        dataSet={{ interactionHandled: "other-device", resolvedByDeviceId: availability.resolvedByDeviceId ?? "unknown" }}
      >
        {`已被其它设备处理（不是本设备所为）：${availability.resolution}`}
      </Text>
    );
  }
  return (
    <Text style={styles.note} dataSet={{ interactionUnavailable: availability.reason }}>
      {INTERACTION_UNAVAILABLE_LABELS[availability.reason]}
    </Text>
  );
}
