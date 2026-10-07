/**
 * R18 在**生产管线**上的降级判定（MAJOR-1 / MAJOR-2）。
 *
 * ## 为什么另立一份用例文件
 *
 * `r18-degradation.test.ts` 的全部用例都直调 `buildDegradedEventModel`，因此两处生产缺陷
 * 带着「338/338 全绿」通过：
 *
 * - **MAJOR-1**：`ClientStore.degradedModelFor` 把 `raw` 硬编码成 `{kind:"absent"}`，
 *   于是「事件类型已知 + 原文因单条上限/保留期未下发」在生产路径上判不出降级，
 *   `buildDegradedEventModel` 的 `raw_not_synced` 分支不可达——用户看到**静默丢失**，
 *   正是 R18 MUST 禁止的。模型本身是对的，错的是上游的筛选。
 * - **MAJOR-2**：`ConversationStream` 把所有非 unsupported 原因兜底成 `"client"`，
 *   降级卡片对「事件已收到、图片没下发」谎称「客户端未提供专用视图」。
 *
 * 本文件因此一律走 `store.apply({kind:"event"})` → `store.conversationPage(...)` → 渲染
 * `ConversationStream`（它内部渲染 `DegradedEventCard`）。模型层与抽屉层的断言仍在原文件。
 *
 * ## 判别力来源
 *
 * - MAJOR-1 的用例在修复前 `降级条目数` 为 0（HTML 里既无 `data-raw-not-synced` 也无
 *   `data-degraded-entry`），修复后为 1 且带正确的 reason。
 * - MAJOR-2 的用例在修复前 HTML 含 `data-unsupported-by="client"` 与「当前客户端未提供专用视图」，
 *   修复后两者都消失，而「内容没下发」的说明仍在。
 * - 每条都配一条**对照**：`unsupported` 仍带三类归因、`available` 原文不产生降级卡片，
 *   证明断言不是靠「永远不渲染」蒙混。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream } from "./index";
import type { ClientStore } from "../features/client-store";
import { RAW_NOT_SYNCED_LABELS } from "../features/degradation-model";
import { eventMessage, resources, session, storeWith } from "../features/testing/fixtures";

/** 夹具会话标识（`storeWith` 的快照里必须有这条会话，`conversationPage` 才返回非 null）。 */
const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000009";

/**
 * 经 `store.apply({kind:"event"})` 注入一条事件，读回该会话的对话页模型。
 *
 * 这是生产路径：`ClientStore.#withEvent` → `degradedModelFor` → 选择器。
 */
function pageAfterEvent(store: ClientStore): NonNullable<ReturnType<ClientStore["conversationPage"]>> {
  const model = store.conversationPage(SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null：快照里已有该会话");
  return model;
}

/** 建一个已喂入快照（含目标会话）的 store。 */
function storeWithSession(): ClientStore {
  const { store } = storeWith({ resources: resources({ sessions: [session({ id: SESSION_ID })] }) });
  return store;
}

describe("R18 生产管线：原文未下发必须产生未同步说明", () => {
  it("已知事件类型 + rawUnavailable：经 ClientStore 产生 raw_not_synced 卡片并说明原因", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        sessionId: SESSION_ID,
        view: { displayPath: "src/a.ts" },
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

    const model = pageAfterEvent(store);
    // 反例（MAJOR-1 修复点）：筛选把 raw 写死成 absent 时这里是 0 条 → 断言变红。
    expect(model.degraded).toHaveLength(1);
    expect(model.degraded[0]?.reason).toEqual({ kind: "raw_not_synced", reason: "size_limit" });

    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model,
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );
    expect(html).toContain('data-raw-not-synced="size_limit"');
    expect(html).toContain(RAW_NOT_SYNCED_LABELS.size_limit);
    // 原文未下发时不得出现原文块（R18：不得渲染占位内容假装完整）。
    expect(html).not.toContain('data-raw-json="available"');
  });

  it("原文可用且事件类型已知：不产生降级卡片（对照，证明上一条不是因为一律降级）", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "session.updated",
        sessionId: SESSION_ID,
        acp: { mediaType: "application/json", rawJson: '{"phase":"review"}', byteLength: "18", sha256: "a".repeat(43) },
      }),
    });

    // 反例：若 `degradedModelFor` 改成「一律产生卡片」，这里会 ≥1 条 → 断言变红。
    expect(pageAfterEvent(store).degraded).toEqual([]);
  });
});

describe("R18 生产管线：非 unsupported 的原因不得谎称「客户端不支持」", () => {
  it("二进制未下发：卡片不输出 data-unsupported-by，也不说「未提供专用视图」", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.message.completed",
        sessionId: SESSION_ID,
        view: { block: { type: "image_ref", mimeType: "image/png", displayState: "not_fetched" } },
      }),
    });

    const model = pageAfterEvent(store);
    expect(model.degraded[0]?.reason).toMatchObject({ kind: "binary_not_delivered", mimeType: "image/png" });

    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model,
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );
    // 内容确实没下发，这一句必须在。
    expect(html).toContain('data-binary-not-delivered="image/png"');
    expect(html).toContain("结构化事件已收到");
    // 反例（MAJOR-2 修复点）：兜底成 `"client"` 时这里会出现 → 断言变红。
    expect(html).not.toContain("data-unsupported-by");
    expect(html).not.toContain("当前客户端未提供专用视图");
    expect(html).not.toContain("data-unsupported-reason");
  });

  it("原文未下发：同样不谎称「客户端不支持」（两类内容缺失互不污染）", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "file.changed",
        sessionId: SESSION_ID,
        acp: {
          mediaType: "application/json",
          rawUnavailable: { reason: "retention_expired", byteLength: "1024", sha256: "b".repeat(43) },
        },
      }),
    });

    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model: pageAfterEvent(store),
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );
    expect(html).toContain('data-raw-not-synced="retention_expired"');
    // 反例（MAJOR-2 修复点）：同上。
    expect(html).not.toContain("data-unsupported-by");
    expect(html).not.toContain("当前客户端未提供专用视图");
  });

  it("显式不支持：三类归因仍在卡片上（对照，证明修复没有把归因一起删掉）", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "agent.message.completed",
        sessionId: SESSION_ID,
        view: { error: { code: "capability.unsupported_by_broker" }, reason: "服务端未开放" },
      }),
    });

    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model: pageAfterEvent(store),
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );
    expect(html).toContain('data-unsupported-by="broker"');
    expect(html).toContain("当前客户端未提供专用视图");
    expect(html).toContain("服务端未开放");
  });

  it("未登记的事件类型：保留「未提供专用视图」，但不冒充三类能力不支持之一", () => {
    const store = storeWithSession();
    store.apply({
      kind: "event",
      event: eventMessage({
        eventType: "acp.agent.vendor_status",
        sessionId: SESSION_ID,
        view: { phase: "review" },
        acp: { mediaType: "application/json", rawJson: '{"vendorExt":{"ticket":918}}', byteLength: "27", sha256: "c".repeat(43) },
      }),
    });

    const html = renderToStaticMarkup(
      createElement(ConversationStream, {
        model: pageAfterEvent(store),
        onLoadEarlier: () => undefined,
        onOpenDiagnostics: () => undefined,
      }),
    );
    expect(html).toContain("acp.agent.vendor_status");
    expect(html).toContain("当前客户端未提供专用视图");
    // 未登记事件**不是**「客户端/服务端/Agent 不支持」三者之一，不得被归到某一类。
    expect(html).not.toContain("data-unsupported-by");
  });
});