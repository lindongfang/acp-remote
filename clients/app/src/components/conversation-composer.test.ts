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
 * 三态另用「输入框是否可编辑 + 发送/停止按钮是否出现」两组属性钉住：三种状态**都**可编辑
 * 草稿，`in_progress`/`unavailable` 只是发送禁用，且 `unavailable` 必须说明当前实际原因；
 * 附件不受支持是**显式说明**（断言说明文案与 `attachmentsSupported=false` 同时在），而不是
 * 「没提」。
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
    blockedBy: null,
    draft: "帮我改一下",
    canSubmit: true,
    disabledReason: null,
    attachmentsSupported: false,
    attachmentNotice: "📎 附件暂不支持 · 仅文本",
    attachments: {
      attachments: [],
      phase: "empty",
      canSubmit: true,
      blockedReason: null,
      capabilityNotice: null,
      uploadedBytes: "0",
      totalBytes: "0",
      error: null,
    },
    pathRefs: [],
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
    expect(tagContaining(html, 'data-action="pick-attachment"')).toContain('aria-disabled="true"');
    expect(html).not.toContain("data-attachment-toolbar");
    // 三态之外没有「不可提交原因」的残留。
    expect(html).not.toContain("data-submit-unavailable");
  });

  it("只有附件/路径引用、没有文字时仍可发送（与仓库「文字或附件或路径非空」同源）", () => {
    const withAttachment = input({
      draft: "",
      attachments: {
        attachments: [
          {
            localId: "11111111-0000-4000-8000-000000000001",
            transferId: "22222222-0000-4000-8000-000000000002",
            attachmentId: null,
            kind: "image",
            fileName: "a.png",
            mediaType: "image/png",
            byteLength: "10",
            status: "selected",
            uploadedBytes: "0",
            sha256: null,
            error: null,
            previewUrl: null,
          },
        ],
        phase: "selected",
        canSubmit: true,
        blockedReason: null,
        capabilityNotice: null,
        uploadedBytes: "0",
        totalBytes: "10",
        error: null,
      },
    });
    const attachmentHtml = render(withAttachment);
    expect(attachmentHtml).toContain('data-submit-enabled="true"');
    expect(tagContaining(attachmentHtml, 'data-action="submit-prompt"')).not.toContain("disabled");

    const withPathRef = render(input({ draft: "   ", pathRefs: ["src/a.ts"] }));
    expect(withPathRef).toContain('data-submit-enabled="true"');
    expect(tagContaining(withPathRef, 'data-action="submit-prompt"')).not.toContain("disabled");

    // 对照：文字、附件、路径全空时按钮仍然禁用（闸门不是无条件放开）。
    const empty = render(input({ draft: "   " }));
    expect(empty).toContain('data-submit-enabled="false"');
    expect(tagContaining(empty, 'data-action="submit-prompt"')).toContain('aria-disabled="true"');
  });

  it("只有图片用图片预览；音频附件显示名称与大小，不把音频 Blob 塞进 <img>", () => {
    const base = {
      status: "selected" as const,
      uploadedBytes: "0",
      sha256: null,
      error: null,
    };
    const html = render(
      input({
        attachments: {
          attachments: [
            {
              ...base,
              localId: "11111111-0000-4000-8000-000000000011",
              transferId: "22222222-0000-4000-8000-000000000011",
              attachmentId: null,
              kind: "image",
              fileName: "shot.png",
              mediaType: "image/png",
              byteLength: "10",
              previewUrl: "blob:image-preview",
            },
            {
              ...base,
              localId: "11111111-0000-4000-8000-000000000012",
              transferId: "22222222-0000-4000-8000-000000000012",
              attachmentId: null,
              kind: "audio",
              fileName: "voice.mp3",
              mediaType: "audio/mpeg",
              byteLength: "2048",
              previewUrl: "blob:audio-preview",
            },
          ],
          phase: "selected",
          canSubmit: true,
          blockedReason: null,
          capabilityNotice: null,
          uploadedBytes: "0",
          totalBytes: "2058",
          error: null,
        },
      }),
    );

    // 图片条目有一张预览；音频条目没有（反例：无条件渲染 <img> 时这里会是 2 张）。
    expect((html.match(/<img /g) ?? []).length).toBe(1);
    expect(html).toContain('data-attachment-preview="image"');
    expect(html).not.toContain('data-attachment-preview="audio"');
    expect(html).not.toContain("blob:audio-preview");
    // 音频条目的名称、种类与大小仍在清单里（去掉破图不等于去掉信息）。
    expect(html).toContain('data-attachment-name="voice.mp3"');
    expect(html).toContain("音频 · 2.0 KiB");
  });

  it("进行中：可编辑下一条草稿、发送禁用不排队，并呈现停止（可取消时）", () => {
    const html = render(
      input({
        state: "in_progress",
        blockedBy: "turn_in_progress",
        canSubmit: false,
        disabledReason: "会话进行中，完成后可继续输入",
        cancel: { kind: "available", failure: null },
      }),
    );

    expect(html).toContain('data-composer="in_progress"');
    // 执行中仍可编辑：输入框**不**只读，草稿可以写，但不建立队列。
    expect(tagContaining(html, 'data-composer-input="in_progress"')).not.toMatch(/readonly/i);
    // 发送按钮仍呈现但禁用（不是「消失」：状态与入口都在，闸门由 canSubmit 决定）。
    expect(html).not.toContain('data-action="submit-prompt"');
    expect(html).toContain('data-action="cancel-turn"');
    expect(html).not.toContain("data-submit-unavailable");
    expect(html).toContain("输入消息，Enter 发送，Shift+Enter 换行");
    expect(html).not.toContain("进行中 · 可编辑但发送禁用，不排队");
  });

  it("进行中且没有可取消的轮次：发送仍禁用，只有停止不呈现（不是点了没反应）", () => {
    const html = render(
      input({
        state: "in_progress",
        blockedBy: "turn_in_progress",
        canSubmit: false,
        disabledReason: "会话进行中，完成后可继续输入",
        cancel: { kind: "unavailable" },
      }),
    );

    expect(html).toContain('data-submit-enabled="false"');
    expect(tagContaining(html, 'data-action="submit-prompt"')).toContain('aria-disabled="true"');
    expect(html).not.toContain('data-action="cancel-turn"');
    expect(tagContaining(html, 'data-composer-input="in_progress"')).not.toMatch(/readonly/i);
  });

  it("不可用（来源离线）：输入框仍可编辑，提交不可点，并说明当前实际原因", () => {
    const html = render(
      input({
        state: "unavailable",
        blockedBy: "source_offline",
        canSubmit: false,
        disabledReason: "连接不可用，不能提交",
      }),
    );

    expect(html).toContain('data-composer="unavailable"');
    expect(html).toContain('data-submit-enabled="false"');
    // 反例：把「不可用」实现成只读 → 下面两条会红。
    expect(tagContaining(html, 'data-composer-input="unavailable"')).not.toMatch(/readonly/i);
    expect(html).not.toContain('data-composer-state-hint="unavailable"');
    expect(html).toContain("连接不可用，不能提交");
    expect(html).not.toContain("当前不可提交 · 草稿仍可编辑");
  });
});

describe("提交与取消以服务端终态为准", () => {
  it("正常提交与轮次执行不插入禁用原因行", () => {
    for (const blockedBy of ["submission_in_flight", "turn_in_progress"] as const) {
      const html = render(input({ state: "in_progress", canSubmit: false, blockedBy,
        disabledReason: "消息已提交，等待服务端确认后才能继续发送" }));
      expect(html).not.toContain("data-submit-unavailable");
      expect(html).toContain('data-submit-enabled="false"');
    }
  });
  it("待确认：只说等待服务端确认，不写「已发送」「已排队」", () => {
    const html = render(
      input({
        state: "in_progress",
        canSubmit: false,
        disabledReason: "会话进行中，完成后可继续输入",
        submission: { kind: "pending_confirmation", requestId: "req-1" },
      }),
    );

    expect(html).not.toContain('data-submission="pending_confirmation"');
    expect(html).not.toContain("等待终态确认");
    // 反例：本地乐观置态会把这两句写进渲染结果 → 断言变红。
    expect(html).not.toContain("已发送");
    expect(html).not.toContain("已排队");
  });

  it("服务端已接受 / 已完成 / 失败：三种终态各有自己的措辞", () => {
    const accepted = render(input({ submission: { kind: "accepted", acceptedAt: "2026-10-01T00:00:01.000Z" } }));
    expect(accepted).not.toContain('data-submission="accepted"');


    const completed = render(input({ submission: { kind: "completed" } }));
    expect(completed).not.toContain('data-submission="completed"');

    const failed = render(input({ submission: { kind: "failed", detail: "session.busy：Agent 正忙" } }));
    expect(failed).toContain('data-submission="failed"');
    expect(failed).toContain("session.busy：Agent 正忙");
    expect(failed).not.toBe(completed);
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
    expect(available).not.toContain('data-cancel="available"');

    const requested = render(input({ cancel: { kind: "requested", requestId: "req-c" } }));
    expect(requested).toContain('data-cancel="requested"');
    expect(requested).toContain('data-cancel-request-id="req-c"');
    expect(requested).toContain("等待服务端终态");
    // 反例：未确认就呈现为空闲/再次可取消 → 这里会再次出现按钮。
    expect(tagContaining(requested, 'data-action="cancel-turn"')).toContain('aria-disabled="true"');

    const confirmed = render(input({ cancel: { kind: "confirmed" } }));
    expect(confirmed).not.toContain('data-cancel="confirmed"');
    expect(confirmed).not.toContain("服务端已确认取消");
  });

  it("取消请求已提交/已确认时，状态提示不与取消行自相矛盾", () => {
    const inProgress = {
      state: "in_progress" as const,
      canSubmit: false,
      disabledReason: "会话进行中，完成后可继续输入",
    };

    const requested = render(input({ ...inProgress, cancel: { kind: "requested", requestId: "req-c" } }));
    expect(requested).toContain('data-cancel="requested"');
    expect(requested).toContain("取消请求已提交");
    // 反例：状态提示仍写「当前轮次不可取消」→ 与上一行互相否证。
    expect(requested).not.toContain("当前轮次不可取消");
    expect(requested).not.toContain("data-composer-hint");

    const confirmed = render(input({ ...inProgress, cancel: { kind: "confirmed" } }));
    expect(confirmed).not.toContain('data-cancel="confirmed"');
    expect(confirmed).not.toContain("当前轮次不可取消");

    // 只有真正没有可取消轮次时才说「不可取消」。
    const unavailable = render(input({ ...inProgress, cancel: { kind: "unavailable" } }));
    expect(unavailable).not.toContain('data-action="cancel-turn"');
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
    expect(html).not.toContain('data-draft-notice="true"');
    expect(html).not.toContain("草稿只在内存中保留、尚未提交");
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

  it("未连接：输入区是 unavailable 且不可提交，原因来自当前连接而不是会话主状态", () => {
    const sessionId = "aaaaaaaa-0000-4000-8000-0000000000c9";
    const { store } = storeWith({ resources: resources({ sessions: [session({ id: sessionId, state: "idle" })] }) });
    const html = renderInputOf(store, sessionId);

    expect(html).toContain('data-composer="unavailable"');
    expect(html).toContain('data-submit-enabled="false"');
    expect(html).not.toContain("当前不能提交。");
    expect(html).not.toContain("连接不可用或来源节点离线，不能提交");
    expect(tagContaining(html, 'data-composer-input="unavailable"')).not.toMatch(/readonly/i);
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
