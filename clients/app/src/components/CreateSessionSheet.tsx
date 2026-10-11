/**
 * 新建会话弹层（R16「会话创建是受限入口」）。
 *
 * ## 四条 MUST 的渲染落点
 *
 * 1. **载荷只含两个引用**：本组件没有路径、MCP 配置、凭据的任何输入框；Agent 候选来自
 *    `model.agents`（本机已配置的 Agent 目录），不是自由文本。
 * 2. **无权限时入口不可用且说明原因**：`data-create-permission="denied"` 的分支**照常渲染
 *    提交按钮**（`disabled`）并把说明摆在旁边——只隐藏按钮不作说明是不允许的。
 * 3. **创建中与终态分别呈现**：`creating` 分支显示「创建中」且按钮禁用（不接受重复提交）。
 * 4. **结果不确定显眼且不被自动消解**：`uncertain` 分支是一个独立的、醒目的区块，
 *    它只由用户点击「我知道了」才消失。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text`/`Pressable` + `StyleSheet`。可点控件一律
 * `role="button"`（RNW 据此渲染 `<button>`，`disabled` 也随之成为原生属性），列表语义用
 * `role="list"`/`role="listitem"` 补回；所有 `data-*` 属性名与取值原样经 `dataSet` 进 DOM。
 */

import type { ReactElement } from "react";
import type { TextProps } from "react-native";

import type { CreateSessionEntryModel, CreateSubmissionModel } from "../features/create-session-model";
import { isSubmissionAvailable } from "../features/create-session-model";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme } from "./theme";

export interface CreateSessionSheetProps {
  readonly model: CreateSessionEntryModel;
  /** 当前选中的 Agent 标识；`null` 表示尚未选择（此时不允许提交）。 */
  readonly selectedAgentId: string | null;
  readonly onSelectAgent: (agentId: string) => void;
  readonly onSubmit: (agentId: string) => void;
  readonly onDismissUncertain: () => void;
  readonly onClose: () => void;
}

/**
 * 弹层标题仍然渲染成 `<h2>`，不能退化成 `<div>`。
 *
 * RNW 按 `role` 选语义标签：`heading` + `aria-level: 2` → `<h2>`。`aria-level` 不在
 * `react-native` 的公开类型里（RNW 比 RN 多认它），因此这里显式声明一次、把这个**运行时**
 * 属性原样透传：属性名与取值不得改名。R16 的浏览器用例用 `[data-create-sheet] h2` 读目录
 * 别名（`e2e/page/checks/r16-create.ts:155`），退化成 `div` 会让它读到空串。
 */
const TITLE_PROPS = { role: "heading", "aria-level": 2 } as unknown as TextProps;

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    sheet: {
      gap: spacing.md,
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      padding: spacing.lg,
      boxShadow: colors.shadow,
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: spacing.sm,
    },
    title: {
      flexGrow: 1,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.lg,
      fontWeight: "600",
    },
    close: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line2,
    },
    closeText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    note: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    denied: {
      padding: spacing.sm,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    options: {
      gap: spacing.xs,
    },
    optionRow: {
      flexDirection: "row",
    },
    optionButton: {
      flexGrow: 1,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line,
      backgroundColor: colors.panel2,
    },
    optionButtonSelected: {
      flexGrow: 1,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    optionName: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    optionId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
    },
    optionDefault: {
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    optionConnection: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    submission: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    uncertain: {
      gap: spacing.sm,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.warn,
      borderRadius: radius.sm,
      backgroundColor: colors.warnSoft,
    },
    uncertainTitle: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    uncertainBody: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    dismiss: {
      alignSelf: "flex-start",
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.warn,
    },
    dismissText: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    submit: {
      alignSelf: "flex-start",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      borderWidth: 1,
      borderColor: colors.accent,
    },
    submitText: {
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 提交状态区块。 */
function Submission({
  submission,
  onDismissUncertain,
}: {
  readonly submission: CreateSubmissionModel;
  readonly onDismissUncertain: () => void;
}): ReactElement | null {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  if (submission.kind === "idle") return null;
  if (submission.kind === "creating") {
    // `role="status"`：创建中是一个「会自己结束」的过程状态，不该像终态那样打断用户。
    return (
      <Text style={styles.submission} role="status" dataSet={{ createSubmission: "creating" }}>
        创建中……正在等待服务器确认。
      </Text>
    );
  }
  if (submission.kind === "uncertain") {
    // 显眼且不自动消解：只由下面的按钮（用户主动动作）结束。
    return (
      <View style={styles.uncertain} role="alert" dataSet={{ createSubmission: "uncertain" }}>
        <Text style={styles.uncertainTitle}>结果不确定</Text>
        <Text style={styles.uncertainBody}>服务器无法确认这次创建是否已经生效。它不会自动消失，也不会随重连清除。</Text>
        <Pressable
          role="button"
          style={styles.dismiss}
          dataSet={{ action: "dismiss-uncertain" }}
          onPress={onDismissUncertain}
        >
          <Text style={styles.dismissText}>我知道了</Text>
        </Pressable>
      </View>
    );
  }
  if (submission.kind === "created") {
    return (
      <Text
        style={styles.submission}
        dataSet={{ createSubmission: "created", createdSessionId: submission.sessionId }}
      >
        会话已创建。
      </Text>
    );
  }
  if (submission.kind === "failed") {
    return (
      <Text
        style={styles.submission}
        dataSet={{
          createSubmission: "failed",
          failureCode: submission.error.code,
          failureRetryable: String(submission.error.retryable),
        }}
      >
        {submission.error.message}
      </Text>
    );
  }
  return (
    <Text style={styles.submission} dataSet={{ createSubmission: "accepted" }}>
      {submission.acceptedAt === null ? "已接受" : `已接受（${submission.acceptedAt}）`}
    </Text>
  );
}

/** 新建会话弹层。 */
export function CreateSessionSheet({
  model,
  selectedAgentId,
  onSelectAgent,
  onSubmit,
  onDismissUncertain,
  onClose,
}: CreateSessionSheetProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  const denied = model.permission.kind === "denied";
  const disabled = denied || selectedAgentId === null || !isSubmissionAvailable(model.submission);

  return (
    <View
      style={styles.sheet}
      dataSet={{ createSheet: model.directoryAlias, createPermission: model.permission.kind }}
    >
      <View style={styles.head}>
        <Text {...TITLE_PROPS} style={styles.title}>
          {`在「${model.directoryAlias}」里新建会话`}
        </Text>
        <Pressable role="button" style={styles.close} dataSet={{ action: "close-create-sheet" }} onPress={onClose}>
          <Text style={styles.closeText}>关闭</Text>
        </Pressable>
      </View>

      <Text style={styles.note} dataSet={{ createNote: "true" }}>
        {model.permission.note}
      </Text>
      {denied ? (
        <Text style={styles.denied} dataSet={{ createDeniedReason: "true" }}>
          {model.permission.reason}
        </Text>
      ) : null}

      <View role="list" style={styles.options} dataSet={{ agentOptions: "true" }}>
        {model.agents.map((agent) => (
          <View key={agent.id} role="listitem" style={styles.optionRow}>
            <Pressable
              role="button"
              style={agent.id === selectedAgentId ? styles.optionButtonSelected : styles.optionButton}
              dataSet={{
                action: "select-agent",
                agentId: agent.id,
                selected: String(agent.id === selectedAgentId),
              }}
              onPress={() => {
                onSelectAgent(agent.id);
              }}
            >
              {/* 同名 Agent 以标识区分：展示名之外必须能看到 id。 */}
              <Text style={styles.optionName} dataSet={{ agentDisplayName: agent.displayName }}>
                {agent.displayName}
              </Text>
              <Text style={styles.optionId} dataSet={{ agentIdLabel: "true" }}>
                {agent.id}
              </Text>
              {agent.isDefault ? (
                <Text style={styles.optionDefault} dataSet={{ agentDefault: "true" }}>
                  默认
                </Text>
              ) : null}
              <Text style={styles.optionConnection} dataSet={{ agentConnection: agent.connection }}>
                {agent.connection === "unknown" ? UNKNOWN_CONNECTION_LABEL : agent.connection}
              </Text>
            </Pressable>
          </View>
        ))}
      </View>

      <Submission submission={model.submission} onDismissUncertain={onDismissUncertain} />

      {/* 无权限时这个按钮照常渲染、只是 `disabled`——「不解释就藏起来」是不允许的。 */}
      <Pressable
        role="button"
        disabled={disabled}
        style={styles.submit}
        dataSet={{ action: "submit-create" }}
        onPress={() => {
          if (selectedAgentId !== null) onSubmit(selectedAgentId);
        }}
      >
        <Text style={styles.submitText}>创建会话</Text>
      </Pressable>
    </View>
  );
}
