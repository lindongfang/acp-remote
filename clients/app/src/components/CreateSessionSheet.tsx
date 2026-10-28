/**
 * 新建会话弹层（R16）。
 *
 * ## MUST 的渲染落点
 *
 * 1. **载荷只含 `agentId` 与一个目录引用**：本组件没有凭据、MCP 配置的
 *    输入框；Agent 候选来自 `model.agents`（本机已配置的 Agent 目录），不是自由文本。
 * 2. **目录不由用户手工输入**：目录块只有两种形态——**已知目录**（从目录卡片/详情页进来：目录、
 *    路径两行）与**顶部入口**（目录未知：选择文件夹控件 + 已选路径 + 说明 + 四类分支提示）。
 *    两条形态都**没有**路径输入框：宿主路径要么逐字来自服务端，要么来自「浏览选定/新建」的结果，
 *    客户端不会凭空拼一段路径让用户去改。不能逐层浏览时，说明文字给出**是哪一种**原因
 *    （还没连上主机 / 服务端未提供该能力，见 `BROWSE_UNAVAILABLE_NOTICES`）——未协商不等于服务端不支持。
 * 3. **入口恒可用**：设备不再有创建会话的权限维度，因此这里**没有**任何权限门控，也不再出现
 *    「需要由电脑端授予」类说明。副作用说明照常呈现（`CREATE_EFFECT_NOTE`）。
 * 4. **列表里不出现目录标识**：目录块呈现派生展示名与路径（标识只作机器标识，见根节点的 `data-create-sheet`）。
 * 5. **创建中与终态分别呈现**：`creating` 分支显示「创建中」且按钮禁用（不接受重复提交）；
 *    失败分支把**参数类**与**服务端不可用类**分开说明（`CREATE_FAILURE_NOTES`）。
 * 6. **结果不确定显眼且不被自动消解**：`uncertain` 分支是一个独立的、醒目的区块，
 *    不提供会误导用户以为结果已经处理完毕的关闭按钮。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text`/`Pressable` + `StyleSheet`。可点控件一律
 * `role="button"`（RNW 据此渲染 `<button>`，`disabled` 也随之成为原生属性），列表语义用
 * `role="list"`/`role="listitem"` 补回；所有 `data-*` 属性名与取值原样经 `dataSet` 进 DOM。
 */

import { useEffect, useState } from "react";
import type { ReactElement } from "react";
import { displayDirectoryPath } from "../domain/directory";
import type { TextProps } from "react-native";

import type { CreateSessionEntryModel, CreateSubmissionModel } from "../features/create-session-model";
import {
  BROWSE_UNAVAILABLE_NOTICES,
  CREATE_FAILURE_NOTES,
  createDirectoryNoticeText,
  createFailureKind,
  isSubmissionAvailable,
} from "../features/create-session-model";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";
import { useModalFocus } from "./modal-focus";
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

/**
 * 提交时要带上的两样东西：选中的 Agent 和用户看到的那个目录路径。
 *
 * `directoryPath` 留空表示「用入口所属的那个**已记录目录**」（目录标识引用）；顶部入口在回退列表里
 * 选中一项时同样留空——目录标识不在表单载荷里，而是由仓库从它自己的单份浏览状态里读。
 */
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
  /**
   * 打开目录选择器（顶部入口形态的「选择文件夹」控件）。
   *
   * 可选：目录已知的形态用不到它。省略时该控件不呈现——目录块因此不会画一个点了没反应的按钮。
   */
  readonly onOpenDirectoryPicker?: (() => void) | undefined;
  readonly onClose: () => void;
  /**
   * 提交在**本地闸门**就被拒绝的原因（未连接、所选 Agent 不在目录里、目录引用缺失）。
   *
   * 这类拒绝没有命令记录，因此不能靠 `model.submission` 呈现——页码只负责把原因带进来，
   * 组件把它画成与命令失败同款的红块，避免用户点了按钮却看不到任何反应。
   */
  readonly submitIssue?: string | null;
}

/**
 * 弹层标题仍然渲染成 `<h2>`，不能退化成 `<div>`。
 *
 * RNW 按 `role` 选语义标签：`heading` + `aria-level: 2` → `<h2>`。`aria-level` 不在
 * `react-native` 的公开类型里（RNW 比 RN 多认它），因此这里显式声明一次、把这个**运行时**
 * 属性原样透传：属性名与取值不得改名。语义标签是读屏与文档结构的依据，退化成 `div` 会把
 * 「这是弹层标题」这件事丢掉；同时标题上另带 `data-create-sheet-title="true"`（见下面
 * `dataSet`），消费方按**属性**取这段文字，不必依赖 RNW 的标签映射。
 *
 * 文案与原型一致（就是「新建会话」四个字）：目录的展示名与路径在下方的目录块里，标题里不再复述——
 * 目录标识也因此没有出现在标题里的机会。
 */
const TITLE_PROPS = { role: "heading", "aria-level": 2 } as unknown as TextProps;

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    overlay: {
      ...webStyle({ position: "fixed" }),
      top: 0, right: 0, bottom: 0, left: 0,
      alignItems: "center",
      justifyContent: "center",
      padding: 20,
      backgroundColor: "rgba(10,12,16,.45)",
      zIndex: zIndex.modal,
    },
    sheet: {
      width: 540,
      ...webStyle({ maxWidth: "100%", maxHeight: "88vh", overflowY: "auto" }),
      backgroundColor: colors.panel,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: 14,
      boxShadow: colors.shadow,
    },
    head: {
      paddingVertical: 15,
      paddingHorizontal: 20,
      borderBottomWidth: 1,
      borderBottomColor: colors.line,
      flexShrink: 0,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "center",
      gap: 10,
    },
    body: {
      paddingVertical: 18,
      paddingHorizontal: 20,
      gap: 14,
    },
    foot: {
      flexDirection: "row",
      justifyContent: "flex-end",
      gap: 10,
      paddingVertical: 13,
      paddingHorizontal: 20,
      borderTopWidth: 1,
      borderTopColor: colors.line,
      flexShrink: 0,
    },
    cancel: {
      paddingVertical: 8,
      paddingHorizontal: 16,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: 9,
      backgroundColor: colors.panel,
    },
    cancelText: {
      fontFamily: fontFamily.sans,
      fontSize: 14,
      lineHeight: 22.4,
      color: colors.ink,
    },
    title: {
      flexGrow: 1,
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
      fontWeight: "600",
    },
    close: {
      width: 30,
      height: 30,
      alignItems: "center",
      justifyContent: "center",
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line,
    },
    closeText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    note: {
      backgroundColor: colors.warnSoft,
      padding: 12,
      borderRadius: 9,
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    // 目录块：目录 / 路径 两行（标签淡、值醒目；路径用等宽字体）。
    dirBlock: {
      gap: 7,
    },
    dirRow: {
      flexDirection: "row",
      alignItems: "baseline",
      gap: spacing.sm,
      minWidth: 0,
    },
    dirKey: {
      flexShrink: 0,
      minWidth: 34,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: 12,
      lineHeight: 19.2,
    },
    dirValue: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: 15,
      lineHeight: 24,
      fontWeight: "600",
    },
    dirPath: {
      flexShrink: 1,
      minWidth: 0,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
      // 原型 `.dv.mono` 的等宽「值」行是 500（`fontWeight.value`）——静态路径值的档位，不写数字字面量。
      fontWeight: fontWeight.value,
    },
    // 「选择文件夹」控件：按钮 + 右侧的选择结果。
    picker: {
      flexDirection: "row",
      alignItems: "center",
      flexShrink: 1,
      gap: spacing.sm,
      minWidth: 0,
    },
    pickerButton: {
      flexShrink: 0,
      paddingVertical: 6,
      paddingHorizontal: spacing.md,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
      borderWidth: 1,
      borderColor: colors.line,
    },
    pickerButtonText: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    pickerPlaceholder: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: 12,
      lineHeight: 18.6,
    },
    noticeInfo: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    noticeWarn: {
      color: colors.warn,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    options: {
      gap: 8,
    },
    optionRow: {
      flexDirection: "row",
    },
    optionButton: {
      flexGrow: 1,
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "flex-start",
      gap: 10,
      paddingVertical: 11,
      paddingHorizontal: 13,
      borderRadius: 10,
      borderWidth: 1,
      borderColor: colors.line,
      backgroundColor: colors.panel,
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
    radio: {
      width: 15, height: 15, borderRadius: 8,
      borderWidth: 1.5, borderColor: colors.line,
      marginTop: 3, alignItems: "center", justifyContent: "center",
      flexShrink: 0,
    },
    radioSelected: { borderColor: colors.accent },
    radioDot: { width: 7, height: 7, borderRadius: 4, backgroundColor: colors.accent },
    optionContent: { flex: 1, minWidth: 0, gap: 2 },
    optionHeading: { flexDirection: "row", alignItems: "baseline", flexWrap: "wrap", gap: 8 },
    fieldLabel: { fontFamily: fontFamily.sans, fontSize: 13, fontWeight: "600", color: colors.ink, marginBottom: 7 },
    info: { backgroundColor: colors.accentSoft, borderRadius: 9, padding: 12, gap: 6 },
    optionName: {
      fontWeight: "600",
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
    },
    optionId: {
      color: colors.ink3,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    optionDefault: {
      paddingHorizontal: spacing.xs,
      borderRadius: radius.sm,
      backgroundColor: colors.accentSoft,
      color: colors.accentInk,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    optionConnection: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
      lineHeight: 17.6,
    },
    submission: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    failure: {
      gap: spacing.xs,
      padding: spacing.md,
      borderWidth: 1,
      borderColor: colors.danger,
      borderRadius: radius.sm,
      backgroundColor: colors.dangerSoft,
    },
    failureText: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },
    failureNote: {
      color: colors.danger,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
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
      lineHeight: 22.4,
      fontWeight: "600",
    },
    uncertainBody: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      lineHeight: 20.8,
    },

    submit: {
      alignSelf: "flex-start",
      paddingVertical: spacing.sm,
      paddingHorizontal: spacing.lg,
      borderRadius: radius.sm,
      backgroundColor: colors.accent,
      borderWidth: 1,
      borderColor: colors.accent,
    },
    submitText: {
      color: "#ffffff",
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      lineHeight: 22.4,
      fontWeight: "600",
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 提交状态区块。 */
function Submission({
  submission,
}: {
  readonly submission: CreateSubmissionModel;
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
    // 显眼且不自动消解：没有会误导用户以为结果已经核实的关闭按钮。
    return (
      <View style={styles.uncertain} role="alert" dataSet={{ createSubmission: "uncertain" }}>
        <Text style={styles.uncertainTitle}>结果不确定</Text>
        <Text style={styles.uncertainBody}>服务器无法确认这次创建是否已经生效。原请求会保留，创建入口继续锁定，避免重复创建。</Text>
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
    // 失败的两类分开呈现：形状非法是**参数类**（服务端不触碰文件系统），目录不可达是
    // **服务端不可用类**（父层不存在或没有写权限）。把两者写成一句话会让用户以为要改目录名。
    const kind = createFailureKind(submission.error);
    const extra = CREATE_FAILURE_NOTES[kind];
    return (
      <View
        role="alert"
        style={styles.failure}
        dataSet={{
          createSubmission: "failed",
          failureCode: submission.error.code,
          failureRetryable: String(submission.error.retryable),
          failureKind: kind,
        }}
      >
        <Text style={styles.failureText}>{submission.error.message}</Text>
        {extra.length === 0 ? null : <Text style={styles.failureNote}>{extra}</Text>}
      </View>
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
  selectedAgentId: requestedAgentId,
  onSelectAgent,
  onSubmit,
  onOpenDirectoryPicker,
  onClose,
  submitIssue = null,
}: CreateSessionSheetProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const selectedAgentId = requestedAgentId ?? model.agents.find((agent) => agent.isDefault)?.id ?? null;
  const [directoryRequired, setDirectoryRequired] = useState(false);
  useModalFocus('[data-create-modal="open"]', onClose);

  // 顶部入口的选定结果：目录引用由它给出（已记录目录用 `directoryId`，选定的路径用 `workspacePath`）。
  // 目录块先取成局部常量：下面按 `kind` 分支时 narrowing 只依赖这一个不可变绑定。
  const directory = model.directory;
  const selection = directory.kind === "browse" ? directory.selection : null;
  const picked = selection !== null && selection.kind === "path" ? selection : null;
  // 留空 = 用入口所属的已记录目录（或回退列表里选中的那一项）——目录标识不在表单载荷里。
  const directoryPath = picked === null ? "" : picked.path;
  const selectedName =
    selection === null ? null : selection.kind === "registered" ? selection.displayName : selection.name;
  const selectedPath = selection === null ? null : selection.path;

  // 缺少目录通过点击后的行内校验反馈；不派发创建请求。
  const directorySelected = directory.kind === "known" || selection !== null;
  const disabled = selectedAgentId === null || !isSubmissionAvailable(model.submission);
  useEffect(() => {
    if (directorySelected) setDirectoryRequired(false);
  }, [directorySelected]);

  return (
    <View role="dialog" aria-modal={true} aria-label={"新建会话"} style={styles.overlay} dataSet={{ createModal: "open" }}>
    <View style={styles.sheet} dataSet={{ createSheet: model.sheetKey }}>
      <View style={styles.head}>
        <Text {...TITLE_PROPS} style={styles.title} dataSet={{ createSheetTitle: "true" }}>
          新建会话
        </Text>
        <Pressable role="button" style={styles.close} dataSet={{ action: "close-create-sheet" }} onPress={onClose}>
          <Text style={styles.closeText}>✕</Text>
        </Pressable>
      </View>

      <View style={styles.body}>
      {/* 副作用说明**始终**呈现：入口恒可用，因此这句说明不再与任何权限态成对出现。 */}


      {directory.kind === "known" ? (
        // 形态一：目录已知（从目录卡片/详情页进来）。目录与路径各一行，都是服务端下发的原文。
        <View style={styles.dirBlock} dataSet={{ createDirectoryBlock: "known" }}>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>目录</Text>
            <Text numberOfLines={1} style={styles.dirValue} dataSet={{ createDirectoryName: directory.displayName }}>
              {directory.displayName}
            </Text>
          </View>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>路径</Text>
            <Text numberOfLines={1} style={styles.dirPath} dataSet={{ createDirectoryPath: directory.path }}>
              {displayDirectoryPath(directory.path)}
            </Text>
          </View>
        </View>
      ) : (
        // 形态二：顶部入口，目录还没定（选择文件夹控件 + 已选路径 + 说明 + 分支提示）。
        <View style={styles.dirBlock} dataSet={{ createDirectoryBlock: "browse" }}>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>目录</Text>
            <View style={styles.picker}>
              {onOpenDirectoryPicker === undefined ? null : (
                <Pressable
                  role="button"
                  style={styles.pickerButton}
                  dataSet={{
                    action: "open-directory-picker",
                    pickerSupported: String(directory.unavailable === null),
                  }}
                  onPress={onOpenDirectoryPicker}
                >
                  {/* 不能浏览时控件改名为「选择已登记的目录」：它通向的正是那条保留下来的路。 */}
                  <Text style={styles.pickerButtonText}>
                    {directory.unavailable === null ? "选择文件夹" : "选择已登记的目录"}
                  </Text>
                </Pressable>
              )}
              <Text
                numberOfLines={1}
                style={[selectedName === null ? styles.pickerPlaceholder : styles.dirValue,
                  directoryRequired && !directorySelected ? { color: theme.colors.danger } : null]}
                role={directoryRequired && !directorySelected ? "alert" : undefined}
                dataSet={{ createSelectedName: selectedName ?? "", directoryRequired: String(directoryRequired && !directorySelected) }}
              >
                {selectedName ?? "未选择目录"}
              </Text>
            </View>
          </View>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>路径</Text>
            <Text numberOfLines={1} style={styles.dirPath} dataSet={{ createDirectoryPath: selectedPath ?? "" }}>
              {selectedPath === null ? "—" : displayDirectoryPath(selectedPath)}
            </Text>
          </View>
          {directory.unavailable === null ? null : (
            // spec 场景「服务端不支持浏览」：明确说明该能力不可用，并保留已登记目录这条路。
            // 锚点带上**原因**：未认证的起点（还没连上）不能把「还不知道服务端支持什么」写成
            // 「服务端没有提供该能力」，两句文案各有一条（见模型的 `BROWSE_UNAVAILABLE_NOTICES`）。
            <Text
              role="alert"
              style={styles.noticeWarn}
              dataSet={{ createBrowseUnavailable: directory.unavailable }}
            >
              {BROWSE_UNAVAILABLE_NOTICES[directory.unavailable]}
            </Text>
          )}
          {/* 分支提示：还没选定 / 会先新建 / 存在但未登记 / 已登记（四类，见模型层）。 */}
          {directory.notice.kind === "unregistered" || directory.notice.kind === "unselected" ? null : <Text
            style={directory.notice.kind === "registered" ? styles.noticeInfo : styles.noticeWarn}
            dataSet={{ createDirectoryNotice: directory.notice.kind }}
          >
            {createDirectoryNoticeText(directory.notice)}
          </Text>}
        </View>
      )}

      <View>
      <Text style={styles.fieldLabel}>选择 Agent</Text>
      <View role="list" style={styles.options} dataSet={{ agentOptions: "true" }}>
        {model.agents.map((agent) => (
          <View key={agent.id} role="listitem" style={styles.optionRow}>
            <Pressable
              role="button"
              aria-pressed={agent.id === selectedAgentId}
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
              <View style={[styles.radio, agent.id === selectedAgentId ? styles.radioSelected : null]}>
                {agent.id === selectedAgentId ? <View style={styles.radioDot} /> : null}
              </View>
              <View style={styles.optionContent}>
              <View style={styles.optionHeading}>
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
              </View>
              <Text style={styles.optionConnection} dataSet={{ agentConnection: agent.connection }}>
                {agent.connection === "unknown" ? UNKNOWN_CONNECTION_LABEL : agent.connection}
              </Text>
              </View>
            </Pressable>
          </View>
        ))}
      </View>
      {model.agents.length === 0 ? (
        <Text style={styles.noticeInfo}>这台电脑还没有配置 Agent，请先在电脑上完成配置。</Text>
      ) : null}

      </View>
      <Text style={styles.note} dataSet={{ createNote: "true" }}>
        {model.effectNote}
      </Text>
      {submitIssue === null ? null : (
        <View role="alert" style={styles.failure} dataSet={{ createSubmitIssue: "true" }}>
          <Text style={styles.failureText}>{submitIssue}</Text>
        </View>
      )}
      <Submission submission={model.submission} />
      </View>
      <View style={styles.foot}>
        <Pressable role="button" style={styles.cancel} onPress={onClose} dataSet={{ action: "cancel-create-sheet" }}>
          <Text style={styles.cancelText}>取消</Text>
        </Pressable>

      <Pressable
        role="button"
        disabled={disabled}
        style={styles.submit}
        dataSet={{ action: "submit-create" }}
        onPress={() => {
          if (!directorySelected) {
            setDirectoryRequired(true);
            return;
          }
          if (!disabled && selectedAgentId !== null) onSubmit({ agentId: selectedAgentId, directoryPath });
        }}
      >
        <Text style={styles.submitText}>创建会话</Text>
      </Pressable>
      </View>
    </View>
    </View>
  );
}
