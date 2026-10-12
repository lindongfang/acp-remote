/**
 * WP5 · 模型与模式分节（spec「模型与模式的呈现与切换」；design D5）。
 *
 * ## 判别力从哪来
 *
 * - **未提供时不得展示虚假选项**：`unavailable` 的渲染结果里一个 `<button>`-类选项都没有
 *   （断言 `data-model-choice` 计数为 0）——若把「没有可选项」画成一个可点的空列表，断言变红。
 * - **切换中不预先改当前值**：`switching` 时当前值仍是旧值，且选项呈现为不可点；确认后才由服务端的
 *   配置事件给出新值（这条与 feature 层单测同一口径，这里钉的是渲染结果）。
 * - **`configId` 原样可写**：分节把配置项 id 带进 DOM（`data-settings-config-id`），页面接线
 *   `setConfigValue(sessionId, configId, value)` 时**不需要**任何映射——生产路径用例直接用它派发，
 *   并断言出站载荷是 `{ configId: "model", value: "smart" }`。
 * - **`not_loaded` 与 `unavailable` 是两件事**：措辞不同（「尚未加载」vs「未提供」），不可合并。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationSettingsPanel, ConversationStream } from "./index";
import type { ConversationSettingsModel } from "../features/conversation-settings-model";
import type { ConversationPageModel } from "../features/conversation-model";
import type { ClientStore } from "../features/client-store";
import { ONLINE_SESSION_ID, eventMessage, lastDispatch, onlineStore, resources, session } from "../features/testing/fixtures";
import type { StoreFixture } from "../features/testing/fixtures";

/** 一类分节的缺省（未提供）。 */
const UNAVAILABLE_SECTION = {
  state: "unavailable",
  currentId: null,
  currentName: null,
  choices: [],
  notice: "该会话的 Agent 未提供模型可选项。",
  configId: null,
} as const;

/** 渲染设置面板。 */
function render(settings: ConversationSettingsModel): string {
  return renderToStaticMarkup(
    createElement(ConversationSettingsPanel, {
      settings,
      onSelectModel: () => undefined,
      onSelectMode: () => undefined,
    }),
  );
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

/** 一个已在线、协商到结构化历史的仓库。 */
function storeOf(): StoreFixture {
  return onlineStore({
    resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }),
  });
}

/** 取对话页模型。 */
function pageOf(store: ClientStore): ConversationPageModel {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null");
  return model;
}

/** 一条 `session.config.changed`：带一个 `category=model` 的配置项。 */
function configChanged(version: string): { readonly kind: "event"; readonly event: ReturnType<typeof eventMessage> } {
  return {
    kind: "event",
    event: eventMessage({
      eventType: "session.config.changed",
      eventId: "dddddddd-0000-4000-8000-000000000301",
      sessionId: ONLINE_SESSION_ID,
      globalSequence: "51",
      view: {
        version,
        configOptions: [
          {
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
          },
        ],
      },
    }),
  };
}

describe("模型与模式分节：四态各就各位", () => {
  it("未提供：呈现「未提供」，且没有任何可点选项", () => {
    const html = render({ model: { ...UNAVAILABLE_SECTION }, mode: { ...UNAVAILABLE_SECTION } });

    expect(html).toContain('data-settings-state="unavailable"');
    expect(html).toContain('data-settings-state-label="unavailable"');
    expect(html).toContain("未提供模型可选项");
    // 反例：把「没有可选项」画成可点列表 → 下面两条会红。
    expect(html.match(/data-model-choice=/g) ?? []).toHaveLength(0);
    expect(html.match(/data-mode-choice=/g) ?? []).toHaveLength(0);
  });

  it("可切换：当前值与选项都在，当前项被标出、选项可点", () => {
    const html = render({
      model: {
        state: "available",
        configId: "model",
        currentId: "fast",
        currentName: "快",
        choices: [
          { id: "fast", name: "快", description: null, current: true },
          { id: "smart", name: "聪明", description: "更贵", current: false },
        ],
        notice: null,
      },
      mode: { ...UNAVAILABLE_SECTION },
    });

    expect(html).toContain('data-settings-config-id="model"');
    expect(html).toContain('data-settings-current-name="快"');
    expect(html.match(/data-model-choice=/g) ?? []).toHaveLength(2);
    expect(tagContaining(html, 'data-model-choice="fast"')).toContain('data-choice-current="true"');
    expect(tagContaining(html, 'data-model-choice="fast"')).toContain('data-choice-selectable="true"');
    expect(tagContaining(html, 'data-model-choice="smart"')).toContain('data-choice-current="false"');
  });

  it("切换中：当前值仍是旧值，选项在位但不可点，并标注「下一轮生效」", () => {
    const html = render({
      model: {
        state: "switching",
        configId: "model",
        currentId: "fast",
        currentName: "快",
        choices: [{ id: "smart", name: "聪明", description: null, current: false }],
        notice: "已提交切换，等待服务端确认；变更在下一轮生效。",
      },
      mode: { ...UNAVAILABLE_SECTION },
    });

    expect(html).toContain('data-settings-state="switching"');
    expect(html).toContain("下一轮生效");
    // 反例：本地乐观改值 → 这里会变成「聪明」。
    expect(html).toContain('data-settings-current-name="快"');
    expect(tagContaining(html, 'data-model-choice="smart"')).toContain('data-choice-selectable="false"');
    expect(tagContaining(html, 'data-model-choice="smart"')).toContain("disabled");
  });

  it("模式清单尚未加载与「服务端未提供模式」可区分", () => {
    const notLoaded = render({
      model: { ...UNAVAILABLE_SECTION },
      mode: { ...UNAVAILABLE_SECTION, state: "not_loaded", notice: "模式清单尚未加载。" },
    });
    const plain = render({ model: { ...UNAVAILABLE_SECTION }, mode: { ...UNAVAILABLE_SECTION } });

    // 模式分节自己的锚点取值不同（模型分节在这两次里都是 unavailable，因此只比模式分节）。
    expect(tagContaining(notLoaded, 'data-settings-section="mode"')).toContain('data-settings-state="not_loaded"');
    expect(tagContaining(plain, 'data-settings-section="mode"')).toContain('data-settings-state="unavailable"');
    expect(notLoaded).toContain("模式清单尚未加载");
    expect(plain).not.toContain("模式清单尚未加载");
    // 反例：把「尚未加载」并进「未提供」时，两次渲染会相同 → 断言变红。
    expect(notLoaded).not.toEqual(plain);
  });
});

describe("分节与页面的接线：configId 原样可写", () => {
  it("配置事件给出可选项后，分节带出 configId；用它切换即派出 session.config.set", () => {
    const { store, gateway } = storeOf();
    store.apply(configChanged("1"));

    const before = pageOf(store).settings.model;
    expect(before.state).toBe("available");
    expect(before.configId).toBe("model");
    expect(before.choices.map((choice) => choice.id)).toEqual(["fast", "smart"]);

    const html = renderToStaticMarkup(
      createElement(ConversationSettingsPanel, { settings: pageOf(store).settings, onSelectModel: () => undefined }),
    );
    expect(html).toContain('data-settings-config-id="model"');

    // 页面接线的写法：`configId` 取自分节，`value` 取自被点的选项——没有任何映射或拼接。
    const configId = before.configId;
    if (configId === null) throw new Error("可切换的模型分节必须给出 configId");
    const outcome = store.setConfigValue(ONLINE_SESSION_ID, configId, "smart");
    expect(outcome.ok).toBe(true);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.config.set") throw new Error("命令不是 session.config.set");
    expect(body.payload).toEqual({ configId: "model", value: "smart" });

    const switching = pageOf(store).settings.model;
    expect(switching.state).toBe("switching");
    const switchingHtml = renderToStaticMarkup(
      createElement(ConversationSettingsPanel, { settings: pageOf(store).settings, onSelectModel: () => undefined }),
    );
    expect(switchingHtml).toContain('data-settings-state="switching"');
    expect(switchingHtml).toContain("下一轮生效");
    expect(switchingHtml).toContain('data-settings-current-name="快"');
  });

  it("模式清单加载后分节给出可选项与当前值；切换派出 session.mode.set", () => {
    const { store, gateway } = storeOf();
    store.loadModes(ONLINE_SESSION_ID);
    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-0000000000f1",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: lastDispatch(gateway).body.requestId,
          command: "session.mode.list",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: {
            currentModeId: "code",
            version: "2",
            availableModes: [
              { modeId: "code", displayName: "编码" },
              { modeId: "ask", displayName: "问答" },
            ],
          },
          error: null,
        },
      },
    });

    const mode = pageOf(store).settings.mode;
    expect(mode.state).toBe("available");
    expect(mode.currentId).toBe("code");
    expect(mode.configId).toBeNull();
    const html = renderToStaticMarkup(
      createElement(ConversationSettingsPanel, { settings: pageOf(store).settings, onSelectMode: () => undefined }),
    );
    expect(html).toContain('data-mode-choice="ask"');
    expect(tagContaining(html, 'data-mode-choice="code"')).toContain('data-choice-current="true"');

    const outcome = store.setMode(ONLINE_SESSION_ID, "ask");
    expect(outcome.ok).toBe(true);
    const body = lastDispatch(gateway).body;
    if (body.command !== "session.mode.set") throw new Error("命令不是 session.mode.set");
    expect(body.payload).toEqual({ modeId: "ask" });
    expect(pageOf(store).settings.mode.state).toBe("switching");
  });
});

describe("设置分节：在对话页里总是呈现（未提供也是明确状态）", () => {
  it("没有配置事件与模式清单时，页面上出现「未提供」而不是空白", () => {
    const { store } = storeOf();
    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model: pageOf(store),
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );

    expect(html).toContain('data-settings="true"');
    expect(html).toContain('data-settings-section="model"');
    expect(html).toContain('data-settings-section="mode"');
    expect(html).toContain('data-settings-state-label="unavailable"');
    expect(html.match(/data-model-choice=/g) ?? []).toHaveLength(0);
  });
});
