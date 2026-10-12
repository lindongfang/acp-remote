/**
 * WP5 · 输入区（spec「输入与提交只支持文本且以终态为准」「取消当前轮次以终态为准」；design D6/D10）。
 *
 * ## 判别力从哪来
 *
 * 断言的是**渲染结果**，而且专门钉住两组 MUST NOT 的措辞：
 *
 * - 提交在终态之前只能是「等待服务端确认」：渲染结果里**没有**「已发送」「已排队」这类词
 *   （本地乐观置态会立刻让这两条断言变红）；
 * - 取消未确认前只能是「取消请求已提交」：那时渲染结果里**没有**可点的取消按钮，也没有
 *   「已空闲」的呈现。
 *
 * 三态与关闭态另用「输入框是否只读 + 提交按钮是否可点」两组属性钉住；附件不受支持是**显式说明**
 * （断言说明文案与 `attachmentsSupported=false` 同时在），而不是「没提」。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationComposer } from "./index";
import { ConversationStream } from "./index";
import type { ClientStore } from "../features/client-store";
import type { ConversationInputModel } from "../features/conversation-input-model";
import { resources, session, storeWith } from "../features/testing/fixtures";

/** 输入区模型夹具：只覆盖被测字段，其余取「不需要标注」的缺省。 */
function input(overrides: Partial<ConversationInputModel>): ConversationInputModel {
  return {
    state: "editable",
    draft: "帮我改一下",
    canSubmit: true,
    disabledReason: null,
    attachmentsSupported: false,
    attachmentNotice: "v1 只支持文本输入：附件（图片/文件）不受支持，请改用文本描述。",
    draftInMemoryOnly: false,
    draftNotice: null,
    submission: { kind: "none" },
    cancel: { kind: "unavailable" },
    ...overrides,
  };
}

/** 渲染输入区。 */
function render(model: ConversationInputModel): string {
  return renderToStaticMarkup(
    createElement(ConversationComposer, {
      input: model,
      onDraftChange: () => undefined,
      onSubmit: () => undefined,
      onCancel: () => undefined,
    }),
  );
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

describe("输入区三态与附件边界", () => {
  it("可输入：输入框可编辑、提交按钮可点、附件不受支持是显式说明", () => {
    const html = render(input({ state: "editable" }));

    expect(html).toContain('data-composer="editable"');
    expect(html).toContain('data-submit-enabled="true"');
    expect(tagContaining(html, 'data-action="submit-prompt"')).not.toContain("disabled");
    expect(tagContaining(html, 'data-composer-input="editable"')).not.toContain("readonly");
    // 附件：说明与标记同时在（「没提附件」与「说了不支持」是两件事）。
    expect(html).toContain('data-attachments-supported="false"');
    expect(html).toContain('data-attachment-notice="true"');
    expect(html).toContain("附件（图片/文件）不受支持");
    // 三态之外没有「不可提交原因」的残留。
    expect(html).not.toContain("data-submit-unavailable");
  });

  it("进行中：呈现进行中态，提交按钮不可点（未收到终态前不能再次提交）", () => {
    const html = render(input({ state: "in_progress", canSubmit: false, disabledReason: "连接不可用，不能提交" }));

    expect(html).toContain('data-composer="in_progress"');
    expect(html).toContain('data-submit-enabled="false"');
    expect(tagContaining(html, 'data-action="submit-prompt"')).toContain("disabled");
    expect(html).toContain("data-submit-unavailable");
    expect(html).toContain("连接不可用，不能提交");
  });

  it("已关闭：输入框只读、提交不可点，并说明原因", () => {
    const html = render(input({ state: "closed", canSubmit: false, disabledReason: "会话已关闭，不能提交" }));

    expect(html).toContain('data-composer="closed"');
    expect(html).toContain('data-submit-enabled="false"');
    expect(tagContaining(html, 'data-composer-input="closed"')).toMatch(/readonly/i);
    expect(html).toContain("会话已关闭，不能提交");
  });
});

describe("提交与取消以服务端终态为准", () => {
  it("待确认：只说等待服务端确认，不写「已发送」「已排队」", () => {
    const html = render(input({ state: "in_progress", submission: { kind: "pending_confirmation", requestId: "req-1" } }));

    expect(html).toContain('data-submission="pending_confirmation"');
    expect(html).toContain('data-submission-request-id="req-1"');
    expect(html).toContain("等待终态确认");
    // 反例：本地乐观置态会把这两句写进渲染结果 → 断言变红。
    expect(html).not.toContain("已发送");
    expect(html).not.toContain("已排队");
  });

  it("服务端已接受 / 已完成 / 失败：三种终态各有自己的措辞", () => {
    const accepted = render(input({ submission: { kind: "accepted", acceptedAt: "2026-10-01T00:00:01.000Z" } }));
    expect(accepted).toContain('data-submission="accepted"');
    expect(accepted).toContain('data-submission-accepted-at="2026-10-01T00:00:01.000Z"');

    const completed = render(input({ submission: { kind: "completed" } }));
    expect(completed).toContain('data-submission="completed"');

    const failed = render(input({ submission: { kind: "failed", detail: "session.busy：Agent 正忙" } }));
    expect(failed).toContain('data-submission="failed"');
    expect(failed).toContain("session.busy：Agent 正忙");
    expect(new Set([accepted, completed, failed]).size).toBe(3);
  });

  it("结果不确定：显著保留（alert）并给出说明", () => {
    const html = render(input({ submission: { kind: "uncertain", detail: "请求可能已生效" } }));

    expect(html).toContain('data-submission="uncertain"');
    expect(tagContaining(html, 'data-submission="uncertain"')).toContain('role="alert"');
    expect(html).toContain("结果不确定");
  });

  it("取消：可取消时有按钮；请求已提交后没有按钮且呈现等待终态；确认后才说已确认", () => {
    const available = render(input({ cancel: { kind: "available", failure: null } }));
    expect(available).toContain('data-action="cancel-turn"');
    expect(available).toContain('data-cancel="available"');

    const requested = render(input({ cancel: { kind: "requested", requestId: "req-c" } }));
    expect(requested).toContain('data-cancel="requested"');
    expect(requested).toContain('data-cancel-request-id="req-c"');
    expect(requested).toContain("等待服务端终态");
    // 反例：未确认就呈现为空闲/再次可取消 → 这里会再次出现按钮。
    expect(requested).not.toContain('data-action="cancel-turn"');

    const confirmed = render(input({ cancel: { kind: "confirmed" } }));
    expect(confirmed).toContain('data-cancel="confirmed"');
    expect(confirmed).toContain("服务端已确认取消");
  });

  it("取消失败：回到可取消并带上失败原因，而不是冒充成功", () => {
    const html = render(input({ cancel: { kind: "available", failure: "session.busy：Agent 正忙" } }));

    expect(html).toContain('data-action="cancel-turn"');
    expect(html).toContain('data-cancel-failure="session.busy：Agent 正忙"');
    expect(html).not.toContain('data-cancel="confirmed"');
  });
});

describe("草稿的内存边界", () => {
  it("来源为其它节点的会话：草稿标注只在内存、未提交", () => {
    const html = render(
      input({ draftInMemoryOnly: true, draftNotice: "草稿只在内存中保留、尚未提交：来源为其它节点的会话不落盘。" }),
    );

    expect(html).toContain('data-composer-draft-in-memory-only="true"');
    expect(html).toContain('data-draft-notice="true"');
    expect(html).toContain("草稿只在内存中保留、尚未提交");
  });
});

describe("输入区：生产路径（仓库模型 → 组件）", () => {
  /** 渲染一个会话的输入区（取仓库给出的输入模型）。 */
  function renderInputOf(store: ClientStore, sessionId: string): string {
    const model = store.conversationPage(sessionId);
    if (model === null) throw new Error("会话页模型不应为 null");
    return renderToStaticMarkup(
      createElement(ConversationStream, {
        model,
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
        onSubmitPrompt: () => undefined,
      }),
    );
  }

  it("已关闭的会话：输入区是 closed 且不可提交（关闭是快照给的，不是页面猜的）", () => {
    const sessionId = "aaaaaaaa-0000-4000-8000-0000000000c9";
    const { store } = storeWith({ resources: resources({ sessions: [session({ id: sessionId, state: "closed" })] }) });
    const html = renderInputOf(store, sessionId);

    expect(html).toContain('data-composer="closed"');
    expect(html).toContain('data-submit-enabled="false"');
    expect(html).toContain("会话已关闭，不能提交");
  });

  it("来源为其它节点且离线的会话：草稿只在内存（R20 的输入侧）", () => {
    const sessionId = "aaaaaaaa-0000-4000-8000-0000000000ca";
    const { store } = storeWith({
      resources: resources({
        sessions: [
          session({
            id: sessionId,
            state: "idle",
            origin: { kind: "remote", ownerNodeId: "node-1", exportId: "e", originEpoch: "o", online: false },
          }),
        ],
      }),
    });
    const html = renderInputOf(store, sessionId);

    expect(html).toContain('data-composer-draft-in-memory-only="true"');
    expect(html).toContain('data-submit-enabled="false"');
    expect(html).toContain('data-attachments-supported="false"');
  });
});
