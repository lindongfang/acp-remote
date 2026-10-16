/**
 * 新建会话弹层（R16）。
 *
 * ## MUST 的渲染落点
 *
 * 1. **载荷只含 `agentId` 与一个目录引用**：本组件没有凭据、MCP 配置的输入框；Agent 候选来自
 *    `model.agents`（本机已配置的 Agent 目录），不是自由文本。目录既可以用本目录的已登记别名
 *    （输入框留空），也允许用户直接输入电脑上的绝对目录（`workspacePath`）。
 * 2. **入口恒可用**：设备不再有创建会话的权限维度，因此这里**没有**任何权限门控，也不再出现
 *    「需要由电脑端授予」类说明。副作用说明照常呈现（{@link create-session-model 的 CREATE_EFFECT_NOTE}）。
 * 3. **目录失败要有可理解的中文说明**：提示与失败原因都摆在输入框与提交状态旁；提示 MUST NOT
 *    声称服务端会拦截敏感目录。
 * 4. **创建中与终态分别呈现**：`creating` 分支显示「创建中」且按钮禁用（不接受重复提交）。
 * 5. **结果不确定显眼且不被自动消解**：`uncertain` 分支是一个独立的、醒目的区块，
 *    它只由用户点击「我知道了」才消失。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text`/`Pressable` + `StyleSheet`。可点控件一律
 * `role="button"`（RNW 据此渲染 `<button>`，`disabled` 也随之成为原生属性），列表语义用
 * `role="list"`/`role="listitem"` 补回；所有 `data-*` 属性名与取值原样经 `dataSet` 进 DOM。
 */

import { useState } from "react";
import type { ReactElement } from "react";
import type { TextProps } from "react-native";

import type { CreateSessionEntryModel, CreateSubmissionModel } from "../features/create-session-model";
import { isSubmissionAvailable } from "../features/create-session-model";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";
import type { Palette } from "./theme";
import {
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
  fontFamily,
  fontSize,
  palettes,
  radius,
  spacing,
  useTheme,
} from "./theme";

/** 提交时要带上的两样东西：选中的 Agent，与用户输入的目录（留空表示用本目录别名）。 */
export interface CreateSessionSubmit {
  readonly agentId: string;
  readonly directoryPath: string;
}

export interface CreateSessionSheetProps {
  readonly model: CreateSessionEntryModel;
  /** 当前选中的 Agent 标识；`null` 表示尚未选择（此时不允许提交）。 */
  readonly selectedAgentId: string | null;
  readonly onSelectAgent: (agentId: string) => void;
  readonly onSubmit: (input: CreateSessionSubmit) => void;
  readonly onDismissUncertain: () => void;
  readonly onClose: () => void;
}

/**
 * 弹层标题仍然渲染成 `<h2>`，不能退化成 `<div>`。
 *
 * RNW 按 `role` 选语义标签：`heading` + `aria-level: 2` → `<h2>`。`aria-level` 不在
 * `react-native` 的公开类型里（RNW 比 RN 多认它），因此这里显式声明一次、把这个**运行时**
 * 属性原样透传：属性名与取值不得改名。语义标签是读屏与文档结构的依据，退化成 `div` 会把
 * 「这是弹层标题」这件事丢掉；同时标题上另带 `data-create-sheet-title="true"`（见下面
 * `dataSet`），消费方按**属性**取这段文字，不必依赖 RNW 的标签映射。
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
    field: {
      gap: spacing.xs,
    },
    fieldLabel: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    input: {
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      borderWidth: 1,
      borderColor: colors.line2,
      backgroundColor: colors.panel2,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
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

  /**
   * 用户输入的目录：留空表示「就用本目录的已登记别名」。这是弹层内部的临时状态——它不跨路由、
   * 也不参与任何数据判定，因此住在这里而不是 store（弹层每次打开都会重新挂载，状态自然复位）。
   */
  const [directoryPath, setDirectoryPath] = useState("");

  // 入口**没有**权限判断：只有「还没选 Agent」与「正在提交/已接受」两个理由会让按钮不可点。
  const disabled = selectedAgentId === null || !isSubmissionAvailable(model.submission);

  return (
    <View style={styles.sheet} dataSet={{ createSheet: model.directoryAlias }}>
      <View style={styles.head}>
        <Text {...TITLE_PROPS} style={styles.title} dataSet={{ createSheetTitle: "true" }}>
          {`在「${model.directoryAlias}」里新建会话`}
        </Text>
        <Pressable role="button" style={styles.close} dataSet={{ action: "close-create-sheet" }} onPress={onClose}>
          <Text style={styles.closeText}>关闭</Text>
        </Pressable>
      </View>

      {/* 副作用说明**始终**呈现：入口恒可用，因此这句说明不再与任何权限态成对出现。 */}
      <Text style={styles.note} dataSet={{ createNote: "true" }}>
        {model.effectNote}
      </Text>

      {/* 目录来源：留空用本目录别名，填写则以客户端提供的绝对目录为准（`workspacePath`）。 */}
      <View style={styles.field}>
        <Text style={styles.fieldLabel}>要使用的目录（可选）</Text>
        <TextInput
          value={directoryPath}
          onChangeText={setDirectoryPath}
          placeholder="留空使用本目录，或输入电脑上的绝对目录"
          placeholderTextColor={theme.colors.ink3}
          accessibilityLabel="要使用的目录"
          style={styles.input}
          // `data-create-directory` 是定位输入框的锚点；当前值另存一份，便于断言（受控控件的
          // value 在 DOM 里只有属性值可读）。
          dataSet={{ createDirectory: "true", createDirectoryValue: directoryPath }}
        />
        <Text style={styles.hint} dataSet={{ createDirectoryHint: "true" }}>
          {model.directoryHint}
        </Text>
      </View>

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

      <Pressable
        role="button"
        disabled={disabled}
        style={styles.submit}
        dataSet={{ action: "submit-create" }}
        onPress={() => {
          if (selectedAgentId !== null) onSubmit({ agentId: selectedAgentId, directoryPath });
        }}
      >
        <Text style={styles.submitText}>创建会话</Text>
      </Pressable>
    </View>
  );
}
