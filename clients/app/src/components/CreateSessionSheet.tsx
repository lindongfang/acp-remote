/**
 * 新建会话弹层（R16）。
 *
 * ## MUST 的渲染落点
 *
 * 1. **载荷只含 `agentId`、一个目录引用与一个可选的建目录布尔**：本组件没有凭据、MCP 配置的
 *    输入框；Agent 候选来自 `model.agents`（本机已配置的 Agent 目录），不是自由文本。
 * 2. **目录不由用户手工输入**：目录块只有两种形态——**已知目录**（从目录卡片/详情页进来：目录、
 *    路径两行）与**顶部入口**（目录未知：选择文件夹控件 + 已选路径 + 说明 + 四类分支提示）。
 *    两条形态都**没有**路径输入框：宿主路径要么逐字来自服务端，要么来自「浏览选定/新建」的结果，
 *    客户端不会凭空拼一段路径让用户去改。不能逐层浏览时，说明文字给出**是哪一种**原因
 *    （还没连上主机 / 服务端未提供该能力，见 `BROWSE_UNAVAILABLE_NOTICES`）——未协商不等于服务端不支持。
 * 3. **入口恒可用**：设备不再有创建会话的权限维度，因此这里**没有**任何权限门控，也不再出现
 *    「需要由电脑端授予」类说明。副作用说明照常呈现（`CREATE_EFFECT_NOTE`）。
 * 4. **列表里不出现别名**：目录块呈现展示名与路径（别名只作机器标识，见根节点的 `data-create-sheet`）。
 * 5. **创建中与终态分别呈现**：`creating` 分支显示「创建中」且按钮禁用（不接受重复提交）；
 *    失败分支把**参数类**与**服务端不可用类**分开说明（`CREATE_FAILURE_NOTES`）。
 * 6. **结果不确定显眼且不被自动消解**：`uncertain` 分支是一个独立的、醒目的区块，
 *    它只由用户点击「我知道了」才消失。
 *
 * 迁移到 RNW 原语（`design.md` D1）：`View`/`Text`/`Pressable` + `StyleSheet`。可点控件一律
 * `role="button"`（RNW 据此渲染 `<button>`，`disabled` 也随之成为原生属性），列表语义用
 * `role="list"`/`role="listitem"` 补回；所有 `data-*` 属性名与取值原样经 `dataSet` 进 DOM。
 */

import type { ReactElement } from "react";
import type { TextProps } from "react-native";

import type { CreateSessionEntryModel, CreateSubmissionModel } from "../features/create-session-model";
import {
  BROWSE_UNAVAILABLE_NOTICES,
  CREATE_DIRECTORY_HINT,
  CREATE_FAILURE_NOTES,
  createDirectoryNoticeText,
  createFailureKind,
  isSubmissionAvailable,
} from "../features/create-session-model";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";
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
} from "./theme";

/**
 * 提交时要带上的三样东西：选中的 Agent、用户看到的那个目录路径、以及是否允许「不存在则先创建」。
 *
 * `directoryPath` 留空表示「用入口所属的那个**已登记目录**」（别名引用）；顶部入口在回退列表里
 * 选中一项时同样留空——别名不在表单载荷里，而是由仓库从它自己的单份浏览状态里读。
 */
export interface CreateSessionSubmit {
  readonly agentId: string;
  readonly directoryPath: string;
  readonly createIfMissing: boolean;
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
 *
 * 文案与原型一致（就是「新建会话」四个字）：目录的展示名与路径在下方的目录块里，标题里不再复述——
 * 别名也因此没有出现在标题里的机会。
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
    // 目录块：目录 / 路径 两行（标签淡、值醒目；路径用等宽字体）。
    dirBlock: {
      gap: spacing.xs,
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
      fontSize: fontSize.sm,
    },
    dirValue: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
      fontWeight: "600",
    },
    dirPath: {
      flexShrink: 1,
      color: colors.ink,
      fontFamily: fontFamily.mono,
      fontSize: fontSize.sm,
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
      paddingVertical: spacing.xs,
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
    },
    pickerPlaceholder: {
      flexShrink: 1,
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.md,
    },
    hint: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    noticeInfo: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    noticeWarn: {
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
    failure: {
      gap: spacing.xs,
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
        <Text style={styles.submission}>{submission.error.message}</Text>
        {extra.length === 0 ? null : <Text style={styles.noticeWarn}>{extra}</Text>}
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
  selectedAgentId,
  onSelectAgent,
  onSubmit,
  onOpenDirectoryPicker,
  onDismissUncertain,
  onClose,
}: CreateSessionSheetProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;

  // 顶部入口的选定结果：目录引用由它给出（已登记目录用别名，选定的路径用 `workspacePath`）。
  // 目录块先取成局部常量：下面按 `kind` 分支时 narrowing 只依赖这一个不可变绑定。
  const directory = model.directory;
  const selection = directory.kind === "browse" ? directory.selection : null;
  const picked = selection !== null && selection.kind === "path" ? selection : null;
  // 留空 = 用入口所属的已登记目录（或回退列表里选中的那一项）——别名不在表单载荷里。
  const directoryPath = picked === null ? "" : picked.path;
  const createIfMissing = picked !== null && picked.willCreate;
  const selectedName =
    selection === null ? null : selection.kind === "registered" ? selection.displayName : selection.name;
  const selectedPath = selection === null ? null : selection.path;

  // 入口**没有**权限判断：只有「还没选 Agent」「目录还没定下来」与「正在提交/已接受」会让按钮不可点。
  const directorySelected = directory.kind === "known" || selection !== null;
  const disabled = selectedAgentId === null || !directorySelected || !isSubmissionAvailable(model.submission);

  return (
    <View style={styles.sheet} dataSet={{ createSheet: model.sheetKey }}>
      <View style={styles.head}>
        <Text {...TITLE_PROPS} style={styles.title} dataSet={{ createSheetTitle: "true" }}>
          新建会话
        </Text>
        <Pressable role="button" style={styles.close} dataSet={{ action: "close-create-sheet" }} onPress={onClose}>
          <Text style={styles.closeText}>关闭</Text>
        </Pressable>
      </View>

      {/* 副作用说明**始终**呈现：入口恒可用，因此这句说明不再与任何权限态成对出现。 */}
      <Text style={styles.note} dataSet={{ createNote: "true" }}>
        {model.effectNote}
      </Text>

      {directory.kind === "known" ? (
        // 形态一：目录已知（从目录卡片/详情页进来）。目录与路径各一行，都是服务端下发的原文。
        <View style={styles.dirBlock} dataSet={{ createDirectoryBlock: "known" }}>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>目录</Text>
            <Text style={styles.dirValue} dataSet={{ createDirectoryName: directory.displayName }}>
              {directory.displayName}
            </Text>
          </View>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>路径</Text>
            <Text style={styles.dirPath} dataSet={{ createDirectoryPath: directory.path ?? "" }}>
              {directory.path ?? "—"}
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
                style={selectedName === null ? styles.pickerPlaceholder : styles.dirValue}
                dataSet={{ createSelectedName: selectedName ?? "" }}
              >
                {selectedName ?? "未选择任何文件"}
              </Text>
            </View>
          </View>
          <View style={styles.dirRow}>
            <Text style={styles.dirKey}>路径</Text>
            <Text style={styles.dirPath} dataSet={{ createDirectoryPath: selectedPath ?? "" }}>
              {selectedPath ?? "—"}
            </Text>
          </View>
          <Text style={styles.hint} dataSet={{ createDirectoryHint: "true" }}>
            {CREATE_DIRECTORY_HINT}
          </Text>
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
          <Text
            style={directory.notice.kind === "registered" ? styles.noticeInfo : styles.noticeWarn}
            dataSet={{ createDirectoryNotice: directory.notice.kind }}
          >
            {createDirectoryNoticeText(directory.notice)}
          </Text>
        </View>
      )}

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
          if (selectedAgentId !== null) onSubmit({ agentId: selectedAgentId, directoryPath, createIfMissing });
        }}
      >
        <Text style={styles.submitText}>创建会话</Text>
      </Pressable>
    </View>
  );
}
