/**
 * 模型与模式分节（spec「模型与模式的呈现与切换」；`design.md` D5 的配置面）。
 *
 * ## 形态：胶囊 + 弹层（原型 `.pick`）
 *
 * 两节各是一个胶囊（`模型 <当前值> ▾` / `模式 <当前值> ▾`），点开是弹层：可选项、分节状态与
 * 说明都在弹层里。弹层顶部固定一行「由 Agent 提供，下一轮生效」的来源说明，选项排在它之后。
 * 弹层内容**始终在 DOM**（收起时只 `display: none`），因此「未提供时不展示虚假可选项」
 * 「切换中当前值不变」这些判定在静置的渲染结果里就断言得到。
 *
 * ## 组件不做判定
 *
 * 「当前值 / 可选项 / 切换中 / 未提供」四态与「未提供时不展示可点选项」这条 MUST 都在
 * `features/conversation-settings-model.ts` 里判定：未提供时模型给的 `choices` 本身就是空数组，
 * 因此「画一个点了必然失败的选项」在这里没有数据可画。
 *
 * ## 切换值原样上抛
 *
 * 模型分节的 `configId` 是**配置项 id**（`session.config.set` 的 `configId`），可选项的 `id` 是
 * **取值**（`value`）——两者由模型分别给出，组件不做任何映射或拼接，直接原样上抛给页面。
 *
 * ## 切换中仍然呈现可选项，只是不可点
 *
 * `switching` 时新值还没被服务端确认，本地**不**预先改当前值（模型保证），选项按钮保持可见但
 * `disabled`：把选项藏起来会让用户以为「这个会话没有可选项」。
 */

import { useState } from "react";
import type { ReactElement } from "react";

import type {
  ConversationSettingsModel,
  SettingsChoice,
  SettingsSectionModel,
} from "../features/conversation-settings-model";
import type { Palette } from "./theme";
import { Pressable, StyleSheet, Text, View, fontFamily, fontSize, palettes, radius, spacing, useTheme, zIndex } from "./theme";

export interface ConversationSettingsPanelProps {
  readonly settings: ConversationSettingsModel;
  /** 切换模型：原样上抛该分节的 `configId` 与选中的取值（页面派发 `session.config.set`）。 */
  readonly onSelectModel?: ((configId: string, value: string) => void) | undefined;
  /** 切换模式：上抛 `modeId`（页面派发 `session.mode.set`）。 */
  readonly onSelectMode?: ((modeId: string) => void) | undefined;
}

/** 分节状态的一句话说明（四态措辞互不相同，`switching` 由模型另给 notice）。 */
const SECTION_STATE_LABELS: Readonly<Record<SettingsSectionModel["state"], string>> = {
  available: "可选择",
  switching: "切换中",
  unavailable: "未提供",
  not_loaded: "尚未加载",
};

/** 分节的说明文案（胶囊上的「模型」「模式」）。 */
const SECTION_KICKERS: Readonly<Record<"model" | "mode", string>> = {
  model: "模型",
  mode: "模式",
};

/**
 * 弹层顶部的一行来源说明：取值由 Agent 提供，切换只对下一轮生效。
 *
 * 与四态无关（`unavailable` 时同样说明「谁提供」），因此放在选项之前、始终渲染。
 */
const SECTION_SOURCE_NOTES: Readonly<Record<"model" | "mode", string>> = {
  model: "模型（由 Agent 提供，下一轮生效）",
  mode: "模式（由 Agent 提供，下一轮生效）",
};

/** 按调色板建样式表：浅/深各建一次（模块加载时）。 */
function buildStyles(colors: Palette) {
  return StyleSheet.create({
    panel: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "flex-start",
      gap: spacing.sm,
    },
    section: {
      position: "relative",
    },
    pill: {
      flexDirection: "row",
      alignItems: "center",
      gap: spacing.xs,
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.lg,
      backgroundColor: colors.panel2,
    },
    kicker: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    value: {
      color: colors.ink,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
      fontWeight: "600",
    },
    caret: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    popover: {
      // 弹层浮在胶囊**上方**（原型 `.pick .pop`：`position:absolute;bottom:calc(100% + 6px)`），
      // 因此展开不会把输入区推下去。收起时 `display:none`，布局没有变化。
      position: "absolute",
      left: 0,
      bottom: "100%",
      gap: spacing.xs,
      minWidth: 220,
      marginBottom: spacing.xs,
      padding: spacing.sm,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.md,
      backgroundColor: colors.panel,
      zIndex: zIndex.topBar,
    },
    popoverHidden: {
      display: "none",
    },
    head: {
      flexDirection: "row",
      flexWrap: "wrap",
      alignItems: "baseline",
      gap: spacing.sm,
    },
    state: {
      paddingHorizontal: spacing.sm,
      paddingVertical: 2,
      borderRadius: radius.sm,
      color: colors.ink2,
      backgroundColor: colors.line2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
    stateSwitching: {
      color: colors.warn,
      backgroundColor: colors.warnSoft,
    },
    choices: {
      gap: spacing.xs,
    },
    choice: {
      paddingVertical: spacing.xs,
      paddingHorizontal: spacing.md,
      borderWidth: 1,
      borderColor: colors.line,
      borderRadius: radius.sm,
      backgroundColor: colors.panel2,
    },
    choiceCurrent: {
      borderColor: colors.accent,
      backgroundColor: colors.accentSoft,
    },
    choiceText: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    choiceTextCurrent: {
      color: colors.accentInk,
      fontWeight: "600",
    },
    notice: {
      color: colors.ink2,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.sm,
    },
    choiceDescription: {
      color: colors.ink3,
      fontFamily: fontFamily.sans,
      fontSize: fontSize.xs,
    },
  });
}

const STYLES = { light: buildStyles(palettes.light), dark: buildStyles(palettes.dark) };

/** 一个可选项按钮：切换中 / 未提供 / 未接线时不可点，但仍在位（避免「看起来没有可选项」）。 */
function ChoiceButton({
  section,
  choice,
  onSelect,
  testKey,
}: {
  readonly section: SettingsSectionModel;
  readonly choice: SettingsChoice;
  readonly onSelect: ((id: string) => void) | null;
  /** `data-*` 锚点的键（模型分节与模式分节各一个，便于逐条断言）。 */
  readonly testKey: "modelChoice" | "modeChoice";
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // 只有「可切换」且页面确实接了回调的选项才可点：其余状态拒绝出现「点了必然失败」的按钮。
  const selectable = section.state === "available" && onSelect !== null;

  return (
    <Pressable
      role="button"
      disabled={!selectable}
      style={choice.current ? [styles.choice, styles.choiceCurrent] : styles.choice}
      dataSet={{
        [testKey]: choice.id,
        choiceCurrent: String(choice.current),
        choiceSelectable: String(selectable),
      }}
      onPress={() => {
        onSelect?.(choice.id);
      }}
    >
      <Text style={choice.current ? [styles.choiceText, styles.choiceTextCurrent] : styles.choiceText}>{choice.name}</Text>
      {choice.description === null ? null : <Text style={styles.choiceDescription}>{choice.description}</Text>}
    </Pressable>
  );
}

/** 一个分节（模型或模式）：胶囊 + 弹层。 */
function SettingsSection({
  name,
  section,
  onSelect,
  testKey,
}: {
  readonly name: "model" | "mode";
  readonly section: SettingsSectionModel;
  readonly onSelect: ((id: string) => void) | null;
  readonly testKey: "modelChoice" | "modeChoice";
}): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  const [open, setOpen] = useState(false);

  return (
    <View
      style={styles.section}
      dataSet={{
        settingsSection: name,
        settingsState: section.state,
        settingsCurrentId: section.currentId ?? "none",
        // 模型分节的配置项 id（页面接线 `session.config.set` 的输入）；模式分节恒为 `none`。
        settingsConfigId: section.configId ?? "none",
      }}
    >
      <Pressable
        role="button"
        style={styles.pill}
        dataSet={{ action: "toggle-settings-popover", settingsPill: name, settingsPillOpen: String(open) }}
        onPress={() => {
          setOpen(!open);
        }}
      >
        <Text style={styles.kicker}>{SECTION_KICKERS[name]}</Text>
        <Text style={styles.value} dataSet={{ settingsCurrentName: section.currentName ?? "none" }}>
          {section.currentName ?? "未提供"}
        </Text>
        <Text style={styles.caret}>▾</Text>
      </Pressable>

      {/* 弹层内容始终在 DOM：收起只是 `display: none`（与原型 `.pick .pop` 同一形态）。 */}
      <View
        style={open ? styles.popover : [styles.popover, styles.popoverHidden]}
        dataSet={{ settingsPopover: name, settingsPopoverOpen: String(open) }}
      >
        <View style={styles.head}>
          <Text
            style={section.state === "switching" ? [styles.state, styles.stateSwitching] : styles.state}
            dataSet={{ settingsStateLabel: section.state }}
          >
            {SECTION_STATE_LABELS[section.state]}
          </Text>
        </View>

        {/* 来源与生效时机：选项之前的一条固定说明（不随四态变化）。 */}
        <Text style={styles.notice} dataSet={{ settingsSourceNote: name }}>
          {SECTION_SOURCE_NOTES[name]}
        </Text>

        <View style={styles.choices}>
          {section.choices.map((choice) => (
            <ChoiceButton key={choice.id} section={section} choice={choice} onSelect={onSelect} testKey={testKey} />
          ))}
        </View>

        {section.notice === null ? null : (
          <Text style={styles.notice} dataSet={{ settingsNotice: section.state }}>
            {section.notice}
          </Text>
        )}
      </View>
    </View>
  );
}

/** 模型与模式分节。 */
export function ConversationSettingsPanel({
  settings,
  onSelectModel,
  onSelectMode,
}: ConversationSettingsPanelProps): ReactElement {
  const theme = useTheme();
  const styles = theme.dark ? STYLES.dark : STYLES.light;
  // `configId` 缺席时没有可写的配置项（类型上就写不出命令），因此不把它变成「有按钮但点了失败」。
  const modelConfigId = settings.model.configId;
  const selectModel =
    onSelectModel === undefined || modelConfigId === null
      ? null
      : (value: string) => {
          onSelectModel(modelConfigId, value);
        };

  return (
    <View style={styles.panel} dataSet={{ settings: "true" }}>
      <SettingsSection name="model" section={settings.model} onSelect={selectModel} testKey="modelChoice" />
      <SettingsSection
        name="mode"
        section={settings.mode}
        onSelect={onSelectMode === undefined ? null : onSelectMode}
        testKey="modeChoice"
      />
    </View>
  );
}
