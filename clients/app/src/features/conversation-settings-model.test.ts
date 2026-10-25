/**
 * 设置分节里的**配置项 id**（`SettingsSectionModel.configId`）——页面接线模型切换的输入。
 *
 * ## 为什么需要这个字段
 *
 * `ClientStore.setConfigValue(sessionId, configId, value)` 的第二个参数是该**配置项**的
 * `ConfigOptionView.id`（线路上是 `"model"`），而 `choices[].id` 是**可选项取值**（`value`，
 * 如 `"fast"`）。分节若不把配置项 id 带出来，页面手里只有可选项取值，接不成线；自己拼一个
 * `"model"` 则是把线路上的 id 硬编码进展示层。
 *
 * ## 判别力
 *
 * - **configId 是配置项的 id，不是取值**：断言 `configId === option.id` 且 `choices[].id` 里
 *   **不含**它。旧实现没有这个字段（读到 `undefined`）→ 变红；用 `currentValue` 或首个可选项
 *   顶替的实现（`"fast"`）→ 也变红。
 * - **未提供配置项时为 `null`**：`configOptions` 为空时没有可写的配置项，`configId` 必须是
 *   `null` 而不是编造一个 id（旧实现 `undefined` → 变红）。
 * - **模式分节恒为 `null`**：模式切换走 `setMode(sessionId, modeId)`，压根没有配置项；模式分节
 *   借用模型分节的 configId（或任意非 `null` 值）→ 变红。
 * - **接线路径**：仓库级用例证明，把分节给出的 `configId` 原样交给 `setConfigValue` 就能派发出
 *   `session.config.set { configId: "model" }`——字段值不需要任何变换。
 */

import { describe, expect, it } from "vitest";

import type { ConfigOptionView, ModeState } from "../protocol";
import { buildConversationSettingsModel } from "./conversation-settings-model";
import { ONLINE_SESSION_ID, eventMessage, interactionStore, lastDispatch } from "./testing/fixtures";

/** 配置项的样子：`id` 是配置项 id，`options[].value` 才是可选项取值（两者不得混用）。 */
const MODEL_OPTION: ConfigOptionView = {
  id: "model",
  name: "模型",
  description: null,
  category: "model",
  type: "select",
  currentValue: "fast",
  options: [
    { value: "fast", name: "快", description: null },
    { value: "smart", name: "聪明", description: null },
  ],
};

/** 同一条配置项、但没有可选项：配置项**已提供**，因此 configId 仍有值。 */
const MODEL_OPTION_WITHOUT_CHOICES: ConfigOptionView = {
  ...MODEL_OPTION,
  options: [],
};

/** 模式清单（`session.mode.list` 的结果）。 */
const MODE_STATE: ModeState = {
  currentModeId: "code",
  availableModes: [
    { modeId: "code", displayName: "写代码" },
    { modeId: "ask", displayName: "只回答" },
  ],
  version: "1",
};

/** 直接构造设置模型：只关心分节本身，切换中/已确认等状态由别的用例覆盖。 */
function settings(input: {
  readonly configOptions: readonly ConfigOptionView[];
  readonly mode?: ModeState | null;
}) {
  return buildConversationSettingsModel({
    configOptions: input.configOptions,
    mode: input.mode ?? null,
    modeChange: null,
    summaryMode: null,
    modelSwitchCommand: null,
    modeSwitchCommand: null,
  });
}

describe("模型分节的 configId 是配置项 id", () => {
  it("思考配置只认明确分类，保留真实配置项 id 和 Agent 可选值", () => {
    const option: ConfigOptionView = { ...MODEL_OPTION, id: "reasoning-effort-custom", category: "thought_level" };
    const view = settings({ configOptions: [MODEL_OPTION, option] });
    expect(view.thought?.configId).toBe("reasoning-effort-custom");
    expect(view.thought?.currentId).toBe("fast");
    expect(view.thought?.choices.map((choice) => choice.id)).toEqual(["fast", "smart"]);
    expect(view.model.configId).toBe("model");
    const missing = settings({ configOptions: [{ ...option, category: null }] });
    expect(missing.thought?.configId).toBeNull();
    expect(missing.thought?.choices).toEqual([]);
  });
  it("configId 取配置项的 id（不是当前取值，也不是可选项取值）", () => {
    const model = settings({ configOptions: [MODEL_OPTION] }).model;
    expect(model.state).toBe("available");
    expect(model.configId).toBe(MODEL_OPTION.id);
    // 可选项取值是另一件事：`choices[].id` 是 `setConfigValue` 的 `value`。
    expect(model.currentId).toBe("fast");
    expect(model.choices.map((choice) => choice.id)).toEqual(["fast", "smart"]);
    expect(model.choices.map((choice) => choice.id)).not.toContain(model.configId);
    // 配置项已提供但没有可选项时，configId 仍是它的 id（`state` 说「没有可选项」，不代表没有配置项）。
    expect(settings({ configOptions: [MODEL_OPTION_WITHOUT_CHOICES] }).model.configId).toBe(MODEL_OPTION.id);
  });

  it("配置项未加载时 configId 为 null（不编造）", () => {
    const model = settings({ configOptions: [] }).model;
    expect(model.state).toBe("unavailable");
    expect(model.configId).toBeNull();
  });

  it("模式分节恒为 null（模式切换走 setMode，不需要配置项）", () => {
    expect(settings({ configOptions: [MODEL_OPTION], mode: MODE_STATE }).mode.configId).toBeNull();
    // 模式清单未加载同样如此：`not_loaded` 不借模型分节的 configId。
    expect(settings({ configOptions: [MODEL_OPTION], mode: null }).mode.configId).toBeNull();
    // 同一份模型里，模型分节带着自己的 configId，两个分节不互相借用。
    expect(settings({ configOptions: [MODEL_OPTION], mode: MODE_STATE }).model.configId).toBe(MODEL_OPTION.id);
  });

  it("分节的 configId 可以原样交给 setConfigValue（接线路径）", () => {
    const { store, gateway } = interactionStore();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.config.changed",
        eventId: "dddddddd-0000-4000-8000-0000000000d3",
        sessionId: ONLINE_SESSION_ID,
        globalSequence: "13",
        view: { configOptions: [MODEL_OPTION], version: "1" },
      }),
    });

    const model = store.conversationPage(ONLINE_SESSION_ID)?.settings.model;
    if (model?.configId === null || model === undefined) throw new Error("模型分节应带出配置项 id");
    // 页面只把它转手：没有拼接、没有大小写改写。
    expect(store.setConfigValue(ONLINE_SESSION_ID, model.configId, "smart").ok).toBe(true);

    const body = lastDispatch(gateway).body;
    if (body.command !== "session.config.set") throw new Error("命令不是 session.config.set");
    expect(body.payload).toEqual({ configId: MODEL_OPTION.id, value: "smart" });
  });
});
