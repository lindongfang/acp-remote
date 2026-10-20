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
 *
 * ## 授权按钮的底色按 `kind` 分档
 *
 * 原型给授权卡三个按钮三档底色（`.p-once` accent / `.p-always` warn / `.p-reject` danger），
 * 让「允许一次 / 始终允许 / 拒绝」在点下去之前就分得出严重度。`kind` 由 `domain/interaction.ts`
 * 带进应答模型，这里只做归一与查表：两条拒绝取值合到同一档，未知取值退到中性档（不崩、也不冒充
 * 允许或拒绝）。
 */

import type { ReactElement } from "react";

import type { InteractionAnswer, InteractionAvailability } from "../domain/interaction";
import { INTERACTION_UNAVAILABLE_LABELS } from "../domain/interaction";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  /** 实心按钮：底色即严重度，边框同底色（原型 `.perm .opts .btn.p-*` 的 `border-color:transparent`）。 */
  const solidAnswer = (background: string) => ({
    paddingVertical: spacing.xs,
    paddingHorizontal: spacing.md,
    borderWidth: 1,
    borderColor: background,
    borderRadius: radius.sm,
    backgroundColor: background,
  });
  /** 实心按钮上的文字：原型写死 `#fff`，这里用「表面色」token（浅色主题正好是白色，深色主题反过来）。 */
  const solidAnswerText = {
    color: colors.panel,
    fontFamily: fontFamily.sans,
    fontSize: fontSize.sm,
    fontWeight: "600" as const,
  };
  /** 提问动作的按钮（没有 `kind`）：沿用原来的浅色 chip。 */
  const plainAnswer = {
    paddingVertical: spacing.xs,
    paddingHorizontal: spacing.md,
    borderWidth: 1,
    borderColor: colors.accent,
    borderRadius: radius.sm,
    backgroundColor: colors.accentSoft,
  };
  const plainAnswerText = {
    color: colors.accentInk,
    fontFamily: fontFamily.sans,
    fontSize: fontSize.sm,
    fontWeight: "600" as const,
  };
  /** 未知/新增严重度取值的中性档：不借任一语义色，因此不会被读成「允许」或「拒绝」。 */
  const neutralAnswer = {
    paddingVertical: spacing.xs,
    paddingHorizontal: spacing.md,
    borderWidth: 1,
    borderColor: colors.line,
    borderRadius: radius.sm,
    backgroundColor: colors.panel,
  };
  const neutralAnswerText = {
    color: colors.ink2,
    fontFamily: fontFamily.sans,
    fontSize: fontSize.sm,
    fontWeight: "600" as const,
  };
  return StyleSheet.create({
    actions: {
      flexDirection: "row",
      flexWrap: "wrap",
      gap: spacing.sm,
    },
    answer: plainAnswer,
    answerText: plainAnswerText,
    /** 允许一次：推荐且最小权限（原型 `.p-once`，accent）。 */
    answerOnce: solidAnswer(colors.accent),
    answerOnceText: solidAnswerText,
    /** 始终允许：长期授权需留意（原型 `.p-always`，warn）。 */
    answerAlways: solidAnswer(colors.warn),
    answerAlwaysText: solidAnswerText,
    /** 拒绝：与「停止」同一套语义色（原型 `.p-reject`，danger）。 */
    answerReject: solidAnswer(colors.danger),
    answerRejectText: solidAnswerText,
    /** 未知/新增取值：确定性降级到中性档——既不是「允许」也不是「拒绝」的语义色。 */
    answerUnknown: neutralAnswer,
    answerUnknownText: neutralAnswerText,
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

/**
 * 授权按钮的严重度档位。
 *
 * 取值域来自 ACP 的 `PermissionOptionKind`（`schemas/acp/v1/upstream/schema.json` 的 `oneOf`）：
 * `allow_once` / `allow_always` / `reject_once` / `reject_always`。原型只画三档（`.p-once` /
 * `.p-always` / `.p-reject`），因此两条拒绝取值合到同一档。
 */
type AnswerSeverity = "once" | "always" | "reject" | "unknown";

/**
 * `option.kind` → 严重度档位。
 *
 * 未知取值（含未来新增的 `kind`）确定性降级到 `unknown`：它既不套用「允许」也不套用「拒绝」的
 * 语义色——把未知读成拒绝会在视觉上替用户做决定，比不区分更糟。
 */
function severityOfKind(kind: string): AnswerSeverity {
  switch (kind) {
    case "allow_once":
      return "once";
    case "allow_always":
      return "always";
    case "reject_once":
    case "reject_always":
      return "reject";
    default:
      return "unknown";
  }
}

export interface InteractionAnswerButtonsProps {
  readonly interactionId: string;
  readonly availability: InteractionAvailability;
  /** 应答上抛；缺省时**不**渲染按钮（不是渲染一个点了没反应的按钮）。 */
  readonly onAnswerInteraction?: ((interactionId: string, answer: InteractionAnswer) => void) | undefined;
  /**
   * 由外部控件接管的动作（例如表单自己渲染「提交」并带上值）。
   *
   * 被省略的动作不再出现在按钮组里：同一动作只有**一个**入口，避免「提交空值」与
   * 「提交表单值」两个按钮并排。
   */
  readonly omitActions?: readonly ("submit" | "decline" | "cancel")[] | undefined;
}

/** 应答按钮组：可选项只来自这次请求自身的下发内容（模型给的可选项）。 */
export function InteractionAnswerButtons({
  interactionId,
  availability,
  onAnswerInteraction,
  omitActions,
}: InteractionAnswerButtonsProps): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (availability.kind !== "answerable" || onAnswerInteraction === undefined) return null;
  const omitted = new Set(omitActions ?? []);
  const answers = availability.answers.filter(
    (answer) => answer.via !== "action" || !omitted.has(answer.action),
  );
  if (answers.length === 0) return null;

  /** 严重度档位 → 该档的按钮/文字样式（样式表已按主题选好，这里只做查表）。 */
  const paintsOf = (severity: AnswerSeverity) => {
    switch (severity) {
      case "once":
        return { button: styles.answerOnce, text: styles.answerOnceText };
      case "always":
        return { button: styles.answerAlways, text: styles.answerAlwaysText };
      case "reject":
        return { button: styles.answerReject, text: styles.answerRejectText };
      case "unknown":
        return { button: styles.answerUnknown, text: styles.answerUnknownText };
    }
  };

  return (
    <View style={styles.actions}>
      {answers.map((answer) => {
        // 授权：按钮底色按 `kind` 的严重度分档（原型 `.p-once` / `.p-always` / `.p-reject`）。
        const paints = answer.via === "option" ? paintsOf(severityOfKind(answer.kind)) : null;
        return (
          <Pressable
            key={answer.via === "option" ? `option:${answer.optionId}` : `action:${answer.action}`}
            role="button"
            style={paints === null ? styles.answer : paints.button}
            dataSet={
              answer.via === "option"
                ? { answerOption: answer.optionId, answerInteractionId: interactionId }
                : { answerAction: answer.action, answerInteractionId: interactionId }
            }
            onPress={() => {
              onAnswerInteraction(interactionId, answer);
            }}
          >
            <Text style={paints === null ? styles.answerText : paints.text}>{answer.label}</Text>
          </Pressable>
        );
      })}
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
