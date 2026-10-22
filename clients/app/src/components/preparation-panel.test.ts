/**
 * 准备页（组件层）用例：只渲染模型、不自己判定。
 *
 * 断言落在**渲染结果**上：真实模式/配置逐项出现、不可用的选择显眼且提交禁用、
 * 结果不确定显眼保留并提供「核实原请求」而不是「重发」。组件本身不发起网络/存储调用
 * （它只接受回调），因此这里只做静态渲染断言。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { PreparationSessionState } from "../features/preparation-model";
import {
  PREPARATION_SETTINGS_UNAVAILABLE_NOTICE,
  PREPARATION_UNCERTAIN_NOTICE,
  buildPreparationPageModel,
} from "../features/preparation-model";
import { PreparationPanel } from "./PreparationPanel";

const SESSION_ID = "6f1c2d3e-4a5b-4c6d-8e7f-901122334455";

function sessionState(overrides: Partial<PreparationSessionState> = {}): PreparationSessionState {
  return {
    sessionId: SESSION_ID,
    agentId: "codex-a",
    workspace: { directoryId: "work-api", path: "D:\\work\\api", source: "directoryId" },
    tabNonce: "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8",
    receipt: null,
    runtimeEpoch: "3f2a1b0c-9d8e-4f7a-8b6c-5d4e3f2a1b0c",
    generation: "1",
    leaseExpiresAt: "2026-10-22T08:00:30.000Z",
    modes: [
      { modeId: "default", displayName: "Default" },
      { modeId: "compact", displayName: "Compact" },
    ],
    configOptions: [
      { id: "sandbox", name: "Sandbox", description: null, category: null, type: "boolean", currentValue: true },
    ],
    selectedMode: "compact",
    selectedConfig: {},
    phase: "ready",
    association: null,
    error: null,
    failureOrigin: null,
    canContinue: false,
    notice: null,
    restartRequired: false,
    ...overrides,
  };
}

function render(input: {
  readonly session: PreparationSessionState;
  readonly text?: string;
  readonly connected?: boolean;
}): string {
  const model = buildPreparationPageModel({
    session: input.session,
    text: input.text ?? "一段首条消息",
    attachments: [],
    connected: input.connected ?? true,
    attachmentsSupported: true,
  });
  return renderToStaticMarkup(
    createElement(PreparationPanel, {
      model,
      agents: [{ id: "codex-a", displayName: "codex", isDefault: true, connection: "connected", disconnectReason: null }],
      directories: [{ directoryId: "work-api", displayName: "api", path: "D:\\work\\api" }],
      onDraftChange: () => undefined,
      onSubmit: () => undefined,
      onSelectMode: () => undefined,
      onSelectConfig: () => undefined,
      onSelectAgent: () => undefined,
      onSelectDirectory: () => undefined,
      onPickAttachment: () => undefined,
      onRemoveAttachment: () => undefined,
      onRetryAttachments: () => undefined,
      onRetry: () => undefined,
      onLeave: () => undefined,
    }),
  );
}

describe("准备页：真实设置与发送闸门", () => {
  it("逐项呈现服务端回报的模式与配置，并标出当前选择", () => {
    const html = render({ session: sessionState() });
    expect(html).toContain('data-preparation-mode="default"');
    expect(html).toContain('data-preparation-mode="compact"');
    expect(html).toContain("Compact");
    expect(html).toContain('data-preparation-config="sandbox"');
    // 发送可用：有文字、设置无不可用项、已连接。
    expect(html).toContain('data-action="preparation-submit"');
    expect(html).toContain('data-submit-enabled="true"');
  });

  it("没有宣告任何设置时明确说明，不伪造选项", () => {
    const html = render({
      session: sessionState({ modes: [], configOptions: [] }),
    });
    expect(html).toContain('data-preparation-modes="none"');
    expect(html).toContain('data-preparation-config="none"');
    expect(html).not.toContain('data-preparation-mode="default"');
  });

  it("重新准备后选项不可用：显眼提示并禁止发送，不静默回落", () => {
    const html = render({
      session: sessionState({ selectedMode: "gone", selectedConfig: { removed: "x" } }),
    });
    expect(html).toContain('data-preparation-settings-unavailable="true"');
    expect(html).toContain("已不可用");
    expect(html).toContain('data-submit-enabled="false"');
    expect(html).toContain(PREPARATION_SETTINGS_UNAVAILABLE_NOTICE.slice(0, 12));
  });

  it("结果不确定显眼保留，只提供核实原请求，不提供重发", () => {
    const html = render({
      session: sessionState({
        phase: "uncertain",
        notice: PREPARATION_UNCERTAIN_NOTICE,
        association: {
          requestId: "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e",
          firstSubmissionId: null,
          firstRequestId: null,
          currentRequestId: null,
          firstMessageId: null,
        },
      }),
    });
    expect(html).toContain('data-preparation-uncertain="true"');
    expect(html).toContain("结果不确定");
    expect(html).toContain('data-action="preparation-verify"');
    // 没有任何「重发/重新发送」入口：不确定时唯一动作是核实。
    expect(html).not.toContain("preparation-resend");
    expect(html).toContain('data-submit-enabled="false"');
  });
});
