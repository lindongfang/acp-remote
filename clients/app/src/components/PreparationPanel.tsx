/**
 * 会话准备页（`/session/<id>` 的准备模式）。
 *
 * 页面只渲染模型、把交互上抛：能不能发、哪些设置不可用、阶段文案全部来自
 * `PreparationPageModel`（`src/features/preparation-model.ts`）。这里不读网络、不碰浏览器存储、
 * 不自己判断「是否已保存」——首条保存成功只是 `phase: "accepted"`，正式会话由仓库另行读取。
 *
 * 与正式会话共用同一套展示原语（RNW `View`/`Text`/`TextInput`/`Pressable`），因此宽窄屏、
 * 主题与 `data-*` 机器标识的口径都与对话页一致。
 */

import type { ReactElement } from "react";

import type { AgentCatalogEntryView } from "../domain";
import type { DraftAttachment } from "../features/attachment-model";
import type { RegisteredDirectoryModel } from "../features/create-session-model";
import type { PreparationPageModel } from "../features/preparation-model";
import { PREPARATION_UNCERTAIN_NOTICE } from "../features/preparation-model";
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
  webStyle,
} from "./theme";

export interface PreparationPanelProps {
  readonly model: PreparationPageModel;
  /** 本机已配置的 Agent（换 Agent 的候选；只来自快照）。 */
  readonly agents: readonly AgentCatalogEntryView[];
  /** 已登记的目录（换目录的候选；只来自快照）。 */
  readonly directories: readonly RegisteredDirectoryModel[];
  readonly onDraftChange: (text: string) => void;
  readonly onSubmit: () => void;
  readonly onSelectMode: (modeId: string) => void;
  readonly onSelectConfig: (configId: string, value: string | boolean) => void;
  readonly onSelectAgent: (agentId: string) => void;
  readonly onSelectDirectory: (directoryId: string) => void;
  readonly onPickAttachment: (kind: "image" | "audio" | "file") => void;
  readonly onRemoveAttachment: (localId: string) => void;
  readonly onRetryAttachments: () => void;
  /** 失败/不确定后的显式动作：先核实原请求，不自动重发。 */
  readonly onRetry: () => void;
  /** 主动离开：作废草稿并释放临时资源。 */
  readonly onLeave: () => void;
}

function buildStyles(colors: Palette) {
  return StyleSheet.create({
    page: {
      flexGrow: 1,
      flexShrink: 1,
      flexBasis: 0,
      minHeight: 0,
      backgroundColor: colors.bg,
      padding: spacing.lg,
      gap: spacing.md,
      ...webStyle({ overflowY: "auto" }),
    },
    header: { gap: 4 },
    title: {
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xl,
      fontWeight: "700",
      color: colors.ink,
    },
    subtitle: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.ink2 },
    section: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      padding: spacing.md,
      gap: spacing.sm,
    },
    sectionTitle: {
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "700",
      color: colors.ink,
    },
    chips: { flexDirection: "row", flexWrap: "wrap", gap: 6 },
    chip: {
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      paddingVertical: 4,
      paddingHorizontal: 8,
    },
    chipActive: { borderColor: colors.accent, backgroundColor: colors.accentSoft },
    chipText: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.ink },
    optionRow: { flexDirection: "row", alignItems: "center", gap: 8, flexWrap: "wrap" },
    optionName: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.ink2 },
    note: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.ink2 },
    warning: {
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      padding: spacing.sm,
      gap: 4,
    },
    warningText: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.danger },
    inputRow: { flexDirection: "row", gap: 8, alignItems: "flex-end" },
    input: {
      flexGrow: 1,
      minHeight: 64,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      padding: spacing.sm,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      backgroundColor: colors.panel,
    },
    primary: {
      borderRadius: radius.sm,
      paddingVertical: 8,
      paddingHorizontal: 14,
      backgroundColor: colors.accent,
    },
    primaryText: { fontFamily: fontFamily.sans, fontSize: fontSize.md, fontWeight: "700", color: colors.accentInk },
    disabled: { opacity: 0.5 },
    ghost: {
      borderRadius: radius.sm,
      paddingVertical: 6,
      paddingHorizontal: 10,
      borderWidth: 1,
      borderColor: colors.line,
    },
    ghostText: { fontFamily: fontFamily.sans, fontSize: fontSize.sm, color: colors.ink },
    attachmentRow: { flexDirection: "row", alignItems: "center", gap: 8, flexWrap: "wrap" },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 附件状态的可读文案（上传/封存/失败逐项区分，不把失败画成空列表）。 */
function attachmentLabel(attachment: DraftAttachment): string {
  switch (attachment.status) {
    case "uploading":
      return "上传中";
    case "sealed":
      return "已上传";
    case "failed":
      return "上传失败";
    case "cancelled":
      return "已取消";
    case "expired":
      return "已过期";
    case "selected":
      return "待上传";
  }
}

/** 准备页本体。 */
export function PreparationPanel(props: PreparationPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const { model } = props;
  const sending = model.phase === "submitting";

  return (
    <View data-route="session" data-session-id={model.sessionId} data-session-state="preparing" style={styles.page}>
      <View style={styles.header}>
        <Text role="heading" style={styles.title} dataSet={{ preparationTitle: "true" }}>
          新会话（准备中）
        </Text>
        <Text style={styles.subtitle} dataSet={{ preparationPhase: model.phase }}>
          {model.phaseLabel}
        </Text>
        {model.workspace.path.length === 0 ? null : (
          <Text style={styles.subtitle} dataSet={{ preparationWorkspace: model.workspace.path }}>
            {model.workspace.path}
          </Text>
        )}
      </View>

      {model.notice === null ? null : (
        <View style={styles.section} dataSet={{ preparationNotice: "true" }}>
          <Text style={styles.note}>{model.notice}</Text>
        </View>
      )}

      {model.error === null ? null : (
        <View role="alert" style={styles.warning} dataSet={{ preparationError: model.error.code }}>
          <Text style={styles.warningText}>{model.error.message}</Text>
        </View>
      )}

      {model.restartRequired ? (
        <View style={styles.section}>
          <Text style={styles.note}>旧准备材料已失效：回目录页重新开始（不会使用旧 ID 或旧凭据）。</Text>
        </View>
      ) : null}

      {model.leaseExpired ? (
        <View role="alert" style={styles.warning} dataSet={{ preparationLeaseExpired: "true" }}>
          <Text style={styles.warningText}>
            临时资源租约已过期：正在续租，请稍候；若一直失败，请离开准备页后重新打开。
          </Text>
        </View>
      ) : null}

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Agent</Text>
        <View style={styles.chips}>
          {props.agents.map((agent) => (
            <Pressable
              key={agent.id}
              role="button"
              disabled={sending || model.phase === "accepted" || model.phase === "committed"}
              style={agent.id === model.agentId ? [styles.chip, styles.chipActive] : styles.chip}
              dataSet={{ action: "preparation-select-agent", preparationAgent: agent.id }}
              onPress={() => {
                props.onSelectAgent(agent.id);
              }}
            >
              <Text style={styles.chipText}>{agent.displayName}</Text>
            </Pressable>
          ))}
        </View>
        <Text style={styles.sectionTitle}>目录</Text>
        <View style={styles.chips}>
          {props.directories.map((directory) => (
            <Pressable
              key={directory.directoryId}
              role="button"
              disabled={sending || model.phase === "accepted" || model.phase === "committed"}
              style={
                directory.directoryId === model.workspace.directoryId
                  ? [styles.chip, styles.chipActive]
                  : styles.chip
              }
              dataSet={{ action: "preparation-select-directory", preparationDirectory: directory.directoryId }}
              onPress={() => {
                props.onSelectDirectory(directory.directoryId);
              }}
            >
              <Text style={styles.chipText}>{directory.path}</Text>
            </Pressable>
          ))}
        </View>
        {props.directories.length === 0 && model.workspace.source === "workspacePath" ? (
          <Text style={styles.note}>当前目录来自浏览选择；已登记目录会列在这里供切换。</Text>
        ) : null}
      </View>

      <View style={styles.section} dataSet={{ preparationSettings: "true" }}>
        <Text style={styles.sectionTitle}>模式</Text>
        {model.settings.modes.length === 0 ? (
          <Text style={styles.note} dataSet={{ preparationModes: "none" }}>
            这个 Agent 没有宣告可切换的模式。
          </Text>
        ) : (
          <View style={styles.chips}>
            {model.settings.modes.map((mode) => (
              <Pressable
                key={mode.modeId}
                role="button"
                disabled={sending}
                style={
                  mode.modeId === model.settings.selectedMode
                    ? [styles.chip, styles.chipActive]
                    : styles.chip
                }
                dataSet={{ action: "preparation-select-mode", preparationMode: mode.modeId }}
                onPress={() => {
                  props.onSelectMode(mode.modeId);
                }}
              >
                <Text style={styles.chipText}>{mode.displayName}</Text>
              </Pressable>
            ))}
          </View>
        )}

        <Text style={styles.sectionTitle}>配置</Text>
        {model.settings.configOptions.length === 0 ? (
          <Text style={styles.note} dataSet={{ preparationConfig: "none" }}>
            这个 Agent 没有宣告可调整的配置项。
          </Text>
        ) : (
          model.settings.configOptions.map((entry) => (
            <View key={entry.option.id} style={styles.optionRow}>
              <Text style={styles.optionName}>{entry.option.name}</Text>
              {entry.option.type === "boolean" ? (
                <Pressable
                  role="button"
                  disabled={sending}
                  style={styles.chip}
                  dataSet={{
                    action: "preparation-toggle-config",
                    preparationConfig: entry.option.id,
                    preparationConfigValue: String(entry.value === true),
                  }}
                  onPress={() => {
                    props.onSelectConfig(entry.option.id, entry.value !== true);
                  }}
                >
                  <Text style={styles.chipText}>{entry.value === true ? "开" : "关"}</Text>
                </Pressable>
              ) : (
                <View style={styles.chips}>
                  {(entry.option.options ?? []).map((choice) => (
                    <Pressable
                      key={choice.value}
                      role="button"
                      disabled={sending}
                      style={
                        choice.value === entry.value ? [styles.chip, styles.chipActive] : styles.chip
                      }
                      dataSet={{
                        action: "preparation-select-config",
                        preparationConfig: entry.option.id,
                        preparationConfigValue: choice.value,
                      }}
                      onPress={() => {
                        props.onSelectConfig(entry.option.id, choice.value);
                      }}
                    >
                      <Text style={styles.chipText}>{choice.name}</Text>
                    </Pressable>
                  ))}
                </View>
              )}
            </View>
          ))
        )}
        {model.settings.unavailable.length === 0 ? null : (
          <View role="alert" style={styles.warning} dataSet={{ preparationSettingsUnavailable: "true" }}>
            <Text style={styles.warningText}>
              这些选择在新会话里已不可用：{model.settings.unavailable.join("、")}。请重新选择后再发送。
            </Text>
          </View>
        )}
      </View>

      <View style={styles.section}>
        <Text style={styles.sectionTitle}>附件</Text>
        {model.attachments.length === 0 ? (
          <Text style={styles.note}>还没有选择附件。点击发送后才会上传。</Text>
        ) : (
          model.attachments.map((attachment) => (
            <View key={attachment.localId} style={styles.attachmentRow}>
              <Text style={styles.optionName}>{attachment.fileName}</Text>
              <Text style={styles.note} dataSet={{ preparationAttachment: attachment.status }}>
                {attachmentLabel(attachment)}
              </Text>
              {attachment.error === null ? null : <Text style={styles.warningText}>{attachment.error}</Text>}
              <Pressable
                role="button"
                disabled={sending}
                style={styles.ghost}
                dataSet={{ action: "preparation-remove-attachment", preparationAttachment: attachment.localId }}
                onPress={() => {
                  props.onRemoveAttachment(attachment.localId);
                }}
              >
                <Text style={styles.ghostText}>移除</Text>
              </Pressable>
            </View>
          ))
        )}
        <View style={styles.chips}>
          <Pressable
            role="button"
            disabled={!model.input.canEditText}
            style={styles.ghost}
            dataSet={{ action: "preparation-add-file" }}
            onPress={() => {
              props.onPickAttachment("file");
            }}
          >
            <Text style={styles.ghostText}>添加文件</Text>
          </Pressable>
          <Pressable
            role="button"
            disabled={!model.input.canEditText}
            style={styles.ghost}
            dataSet={{ action: "preparation-add-image" }}
            onPress={() => {
              props.onPickAttachment("image");
            }}
          >
            <Text style={styles.ghostText}>添加图片</Text>
          </Pressable>
          {model.attachments.some(
            (attachment) =>
              attachment.status === "failed" ||
              attachment.status === "cancelled" ||
              attachment.status === "expired",
          ) ? (
            <Pressable
              role="button"
              style={styles.ghost}
              dataSet={{ action: "preparation-retry-attachments" }}
              onPress={props.onRetryAttachments}
            >
              <Text style={styles.ghostText}>重试附件</Text>
            </Pressable>
          ) : null}
        </View>
      </View>

      <View style={styles.inputRow}>
        <TextInput
          value={model.text}
          multiline
          placeholder="输入首条消息（点击发送后才保存正式会话）"
          editable={model.input.canEditText}
          style={styles.input}
          dataSet={{ preparationInput: model.phase }}
          onChangeText={props.onDraftChange}
        />
        <Pressable
          role="button"
          disabled={!model.input.canSend}
          style={model.input.canSend ? styles.primary : [styles.primary, styles.disabled]}
          dataSet={{ action: "preparation-submit", submitEnabled: String(model.input.canSend) }}
          onPress={props.onSubmit}
        >
          <Text style={styles.primaryText}>{sending ? "保存中…" : "发送首条"}</Text>
        </Pressable>
      </View>
      {model.input.reason === null ? null : (
        <Text style={styles.note} dataSet={{ preparationSubmitBlocked: "true" }}>
          {model.input.reason}
        </Text>
      )}

      {model.phase === "uncertain" ? (
        <View role="alert" style={styles.warning} dataSet={{ preparationUncertain: "true" }}>
          <Text style={styles.warningText}>{PREPARATION_UNCERTAIN_NOTICE}</Text>
        </View>
      ) : null}

      <View style={styles.chips}>
        {model.phase === "failed" || model.phase === "uncertain" || model.phase === "submitting" ||
        model.phase === "accepted" ? (
          <Pressable
            role="button"
            style={styles.ghost}
            dataSet={{ action: "preparation-verify" }}
            onPress={props.onRetry}
          >
            <Text style={styles.ghostText}>核实原请求</Text>
          </Pressable>
        ) : null}
        <Pressable
          role="button"
          style={styles.ghost}
          dataSet={{ action: "preparation-leave" }}
          onPress={props.onLeave}
        >
          <Text style={styles.ghostText}>离开并释放准备</Text>
        </Pressable>
      </View>
    </View>
  );
}
