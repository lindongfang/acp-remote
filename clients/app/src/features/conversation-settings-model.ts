/**
 * 模型与模式的呈现与切换（spec「模型与模式的呈现与切换」；`design.md` D5 的配置面）。
 *
 * ## 取值来源与「未提供」不造假
 *
 * - **模型**来自配置项里 `category === "model"` 的那一项（`session.config.changed` 事件或
 *   `session.read` 的 `config_options` 资源），可选项来自它自己的 `options`。
 *   一项都没有时呈现 `unavailable`，**不**展示任何可点选的模型（spec MUST NOT）。
 * - **模式**当前值来自 `session.mode.changed`，回落顺序：最新的 `session.mode.changed` 事件 →
 *   `session.mode.list` 的结果 → 会话摘要的 `currentMode`；可选项来自 `session.mode.list`
 *   的结果（`ModeState.availableModes`）。清单还没取到时是 `not_loaded`——这与「服务端没有
 *   提供模式」是**两件事**，不能合并成一个「没有」。
 *
 *   服务端**不**产出 `session.updated` 来承载模式变更，因此 `session.mode.changed` 是当前模式
 *   唯一的活动信号：只读快照/清单会让「当前模式」停在旧值上，而时间线上同一事件已经出现一张
 *   显示新值的卡片——同一事件两处互相矛盾（与模型分节读 `session.config.changed` 对称）。
 *
 * ## 切换以服务端确认的结果为准
 *
 * `switching` 只由**未到终态**的切换命令产生；`completed` 之后才把「变更在下一轮生效」写上，
 * 且新值仍以服务端的配置/模式事件为准（客户端不预先改本地值，避免呈现出服务端没同意的取值）。
 */

import type { ConfigOptionView, ModeRef, ModeState } from "../protocol/common";
import type { ModeChangedTimelineCard } from "../domain";
import type { CommandRecord } from "../state";
import { isTerminalState } from "../state";

/** 一个可选项。 */
export interface SettingsChoice {
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly current: boolean;
}

/** 一类设置（模型或模式）的呈现。 */
export interface SettingsSectionModel {
  readonly state: "available" | "switching" | "unavailable" | "not_loaded";
  readonly currentId: string | null;
  readonly currentName: string | null;
  readonly choices: readonly SettingsChoice[];
  readonly notice: string | null;
  /**
   * 该分节切换时要写的**配置项 id**（`ConfigOptionView.id`，线路上如 `"model"`）。
   *
   * 它是页面调 `ClientStore.setConfigValue(sessionId, configId, value)` 的第二个参数，因此与
   * `choices[].id`（那是可选项**取值**，即 `value`）是两件事：同一个分节里两者并存，键名必须各自
   * 明确——拿 `choices[].id` 当 configId 会把 `session.config.set` 指到不存在的配置项上。
   *
   * 模式切换走 `setMode`（按 `modeId`），用不到配置项，故模式分节恒为 `null`；模型配置项
   * 未加载/未提供时也是 `null`（**不**编造一个 id）。
   */
  readonly configId: string | null;
}

/** 对话页的设置模型。 */
export interface ConversationSettingsModel {
  readonly model: SettingsSectionModel;
  readonly mode: SettingsSectionModel;
  /** 仅使用 Agent 明确提供的 thought_level 配置。 */
  readonly thought?: SettingsSectionModel;
}

/** 模型配置项：配置项里 `category === "model"` 的那一项（多个时取第一个，声明顺序即优先级）。 */
export function modelConfigOption(configOptions: readonly ConfigOptionView[]): ConfigOptionView | null {
  return configOptions.find((option) => option.category === "model") ?? null;
}

/** 模型分节。 */
function modelSection(input: {
  readonly option: ConfigOptionView | null;
  readonly switchCommand: CommandRecord | null;
  readonly label?: string;
}): SettingsSectionModel {
  const label = input.label ?? "模型";
  const switching = input.switchCommand !== null && (!isTerminalState(input.switchCommand.state) || input.switchCommand.state === "uncertain");
  const confirmed = input.switchCommand !== null && input.switchCommand.state === "completed";
  const option = input.option;
  if (option === null) {
    return {
      state: switching ? "switching" : "unavailable",
      currentId: null,
      currentName: null,
      choices: [],
      notice: switching ? "已提交切换，等待服务端确认；变更在下一轮生效。" : `该会话的 Agent 未提供${label}可选项。`,
      // 配置项未提供：没有可写的 configId，不能拿别的东西顶上。
      configId: null,
    };
  }
  const currentValue = String(option.currentValue);
  const choices: SettingsChoice[] = (option.options ?? []).map((choice) => ({
    id: choice.value,
    name: choice.name,
    description: choice.description,
    current: choice.value === currentValue,
  }));
  const current = choices.find((choice) => choice.current) ?? null;
  return {
    state: switching ? "switching" : choices.length === 0 ? "unavailable" : "available",
    currentId: current?.id ?? (currentValue.length === 0 ? null : currentValue),
    currentName: current?.name ?? (currentValue.length === 0 ? null : currentValue),
    choices,
    notice: switching
      ? "已提交切换，等待服务端确认；变更在下一轮生效。"
      : choices.length === 0
        ? `该会话的 Agent 未提供${label}可选项。`
        : confirmed
          ? "已切换；变更在下一轮生效。"
          : null,
    // 配置项本身的 id（不是当前取值、也不是可选项取值）：切换时它被原样当作 `configId` 写回。
    configId: option.id,
  };
}

/** 模式分节。 */
function modeSection(input: {
  readonly mode: ModeState | null;
  readonly modeChange: ModeChangedTimelineCard | null;
  readonly summaryMode: ModeRef | null;
  readonly switchCommand: CommandRecord | null;
}): SettingsSectionModel {
  const switching = input.switchCommand !== null && (!isTerminalState(input.switchCommand.state) || input.switchCommand.state === "uncertain");
  const confirmed = input.switchCommand !== null && input.switchCommand.state === "completed";
  // 当前值与模型分节同源：最新的 `session.mode.changed` → 清单结果 → 快照摘要。
  const currentModeId = input.modeChange?.currentModeId ?? input.mode?.currentModeId ?? input.summaryMode?.modeId ?? null;
  const modes = input.mode?.availableModes ?? [];
  const choices: SettingsChoice[] = modes.map((mode) => ({
    id: mode.modeId,
    name: mode.displayName,
    description: null,
    current: mode.modeId === currentModeId,
  }));
  const current = choices.find((choice) => choice.current) ?? null;
  // 摘要只带 `{modeId, displayName}`：仅当它说的**就是**当前这一项时才拿它的名字兜底，
  // 否则会把另一个模式的名字安到当前值上（当前值可能来自更新的事件或未加载的清单）。
  const summaryName = input.summaryMode !== null && input.summaryMode.modeId === currentModeId ? input.summaryMode.displayName : null;
  if (input.mode === null) {
    return {
      state: switching ? "switching" : "not_loaded",
      currentId: currentModeId,
      currentName: current?.name ?? summaryName,
      choices,
      notice: switching ? "已提交切换，等待服务端确认；变更在下一轮生效。" : "模式清单尚未加载。",
      // 模式切换走 `setMode`（按 `modeId`）：没有配置项可写，恒为 `null`。
      configId: null,
    };
  }
  return {
    state: switching ? "switching" : choices.length === 0 ? "unavailable" : "available",
    currentId: currentModeId,
    currentName: current?.name ?? summaryName,
    choices,
    notice: switching
      ? "已提交切换，等待服务端确认；变更在下一轮生效。"
      : choices.length === 0
        ? "该会话的 Agent 未提供模式可选项。"
        : confirmed
          ? "已切换；变更在下一轮生效。"
          : null,
    // 同上：模式分节不承载配置项 id。
    configId: null,
  };
}

/** 构造设置模型。 */
export function buildConversationSettingsModel(input: {
  readonly configOptions: readonly ConfigOptionView[];
  readonly mode: ModeState | null;
  /** 最新的 `session.mode.changed`（当前模式的活动信号）；没有这类事件时为 `null`。 */
  readonly modeChange: ModeChangedTimelineCard | null;
  readonly summaryMode: ModeRef | null;
  readonly modelSwitchCommand: CommandRecord | null;
  readonly modeSwitchCommand: CommandRecord | null;
}): ConversationSettingsModel {
  return {
    thought: modelSection({ option: input.configOptions.find((option) => option.category === "thought_level") ?? null, switchCommand: input.modelSwitchCommand, label: "思考模式" }),
    model: modelSection({ option: modelConfigOption(input.configOptions), switchCommand: input.modelSwitchCommand }),
    mode: modeSection({
      mode: input.mode,
      modeChange: input.modeChange,
      summaryMode: input.summaryMode,
      switchCommand: input.modeSwitchCommand,
    }),
  };
}
