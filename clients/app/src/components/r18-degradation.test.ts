/**
 * R18：未识别与不可用的结构化内容显式降级（渲染层）。
 *
 * 逐条 MUST NOT 的负向用例：
 *
 * - **原文未下发时不渲染占位**：构造 `rawUnavailable` 的事件 → 断言诊断抽屉里**没有**
 *   原文块、没有任何看起来像 JSON 的占位内容，却明确说明了原因与元数据；
 * - **三类不支持可区分**：三个 `capability.unsupported_by_*` 各自渲染出**互不相同**的
 *   说明（若合并成一句笼统的「不可用」，互异性断言会红）；
 * - **未识别事件保留原文可查**：未登记的 `eventType` 仍渲染降级卡片，且抽屉给出原文；
 * - **未知事件不得让整个会话渲染失败**：同一次渲染里消息与降级卡片都在；
 * - **二进制内容未下发**：只说明「事件已收到、内容没下发」，不渲染图片位。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream, DiagnosticsDrawer } from "./index";
import { buildConversationModel } from "../features/conversation-model";
import type { ConversationPageModel } from "../features/conversation-model";
import { buildDegradedEventModel } from "../features/degradation-model";
import type { DegradedEventModel } from "../features/degradation-model";
import { RAW_NOT_SYNCED_LABELS, UNSUPPORTED_LABELS } from "../features/degradation-model";
import { eventMessage } from "../features/testing/fixtures";

/** 渲染诊断抽屉。 */
function renderDrawer(model: DegradedEventModel | null): string {
  return renderToStaticMarkup(
    createElement(DiagnosticsDrawer, { model, onClose: () => undefined }),
  );
}

/** 渲染会话流（含降级卡片）。 */
function renderStream(degraded: readonly DegradedEventModel[]): string {
  const model: ConversationPageModel = buildConversationModel({
    header: {
      sessionId: "s-1",
      title: "修复登录超时",
      agentName: "Claude",
      state: "idle",
      directoryAlias: "work-api",
      originOfflineLabel: null,
    },
    page: {
      sessionId: "s-1",
      hasEarlier: false,
      earliestCursor: null,
      messages: [
        { id: "m-1", role: "user", text: "开始", hasStructuredContent: false, createdAt: "2026-10-01T00:00:00.000Z", turnId: null },
      ],
    },
    usage: null,
    degraded,
    loadingEarlier: false,
    pagingError: null,
    capabilities: { canSend: true, canCancel: false, canViewHistory: true },
  });
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model,
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
    }),
  );
}

describe("R18 原文未下发时不渲染占位", () => {
  it("rawUnavailable 的事件：说明原因与元数据，但没有原文块也没有占位 JSON", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({
        eventType: "file.changed",
        sessionId: "s-1",
        view: { displayPath: "src/a.ts", summary: "diff 未能同步" },
        acp: {
          mediaType: "application/json",
          rawUnavailable: {
            reason: "size_limit",
            byteLength: "281344",
            sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p",
          },
        },
      }),
    });

    const html = renderDrawer(model);

    expect(model.raw.kind).toBe("not_synced");
    expect(html).toContain('data-degraded-reason="raw_not_synced"');
    expect(html).toContain(RAW_NOT_SYNCED_LABELS.size_limit);
    // 元数据仍在（诊断需要它们）。
    expect(html).toContain("281344");
    // 反例：若用占位内容假装完整，这里会出现原文块或一段 JSON，断言变红。
    expect(html).not.toContain('data-raw-json="available"');
    expect(html).not.toContain("{}");
    expect(html).not.toContain("undefined");
  });

  it("对照：原文可用时给出原文块（证明上一条不是因为抽屉永远不出原文）", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({
        eventType: "acp.agent.custom_status",
        acp: { mediaType: "application/json", rawJson: '{"phase":"review"}', byteLength: "18", sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p" },
      }),
    });
    const html = renderDrawer(model);
    expect(html).toContain('data-raw-json="available"');
    expect(html).toContain("{&quot;phase&quot;:&quot;review&quot;}");
  });
});

describe("R18 三类不支持可区分", () => {
  const cases = [
    { code: "capability.unsupported_by_client", by: "client" },
    { code: "capability.unsupported_by_broker", by: "broker" },
    { code: "capability.unsupported_by_agent", by: "agent" },
  ] as const;

  it("三类原因各自渲染出互不相同的说明", () => {
    const labels = cases.map((item) => {
      const model = buildDegradedEventModel({
        event: eventMessage({ eventType: "agent.message.completed", view: { block: { type: "image_ref", displayState: "unsupported" } } }),
        error: { code: item.code, message: `不可用：${item.by}`, retryable: false, details: {} },
      });
      expect(model.reason).toMatchObject({ kind: "unsupported", by: item.by });
      const html = renderDrawer(model);
      expect(html).toContain(`data-unsupported-by="${item.by}"`);
      return html;
    });

    // 反例：若三类合并为一句笼统的「不可用」，三段 HTML 会相同，去重后只剩 1 个，断言变红。
    expect(new Set(labels).size).toBe(3);
    // 三类文案本身也必须互不相同。
    expect(new Set(Object.values(UNSUPPORTED_LABELS)).size).toBe(3);
  });

  it("三类原因在降级卡片上同样带 data-unsupported-by", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({ eventType: "agent.message.completed", sessionId: "s-1" }),
      error: { code: "capability.unsupported_by_broker", message: "服务端未开放", retryable: false, details: {} },
    });
    const html = renderStream([model]);
    expect(html).toContain('data-unsupported-by="broker"');
    expect(html).toContain('data-unsupported-detail="broker"');
    expect(html).toContain("服务端未开放");
  });
});

describe("R18 未识别事件与未知内容", () => {
  it("未登记的事件类型：呈现降级卡片并保留原文入口", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({
        eventType: "acp.agent.vendor_status",
        sessionId: "s-1",
        view: { phase: "review", progress: 0.62 },
        acp: { mediaType: "application/json", rawJson: '{"vendorExt":{"ticket":918}}', byteLength: "27", sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p" },
      }),
    });

    expect(model.reason).toEqual({ kind: "no_dedicated_view" });
    const html = renderStream([model]);
    // 事件类型原样显示（不替换成占位名称）。
    expect(html).toContain("acp.agent.vendor_status");
    expect(html).toContain("当前客户端未提供专用视图");
    expect(html).toContain('data-action="open-diagnostics"');
    // 反例：若静默隐藏该事件，这里不会有降级条目，断言变红。
    expect(html).toContain('data-degraded-entry=');
  });

  it("未知事件不得让整个会话渲染失败：消息与降级卡片同时在", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({ eventType: "acp.unknown.thing", sessionId: "s-1" }),
    });
    const html = renderStream([model]);

    expect(html).toContain('data-message-id="m-1"');
    expect(html).toContain('data-degraded-entry=');
  });

  it("二进制内容未下发：说明事件已收到、内容没下发，且不渲染图片位", () => {
    const model = buildDegradedEventModel({
      event: eventMessage({
        eventType: "agent.message.completed",
        sessionId: "s-1",
        view: { block: { type: "image_ref", mimeType: "image/png", displayState: "not_fetched" } },
      }),
    });

    expect(model.reason).toMatchObject({ kind: "binary_not_delivered" });
    const html = renderDrawer(model);
    expect(html).toContain('data-degraded-reason="binary_not_delivered"');
    expect(html).toContain("结构化事件已收到");
    // 反例：若渲染一个假的图片位，这里会出现 <img，断言变红。
    expect(html).not.toContain("<img");
  });
});
