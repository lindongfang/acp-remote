/**
 * WP5 · 节点级降级面（spec「呈现覆盖由客户端自持」；design D5/R18）。
 *
 * ## 判别力从哪来
 *
 * `device.revoked` 这类事件**没有会话**：它既不会出现在任何会话时间线上，也不该被静默丢掉
 * （那正是「被消费却既不呈现也不降级」的第三种结果）。本用例钉住三件事：
 *
 * 1. 它出现在**主机/连接面**的节点级降级列表里，锚点是 `data-node-degraded-entry`（与会话时间线的
 *    `data-degraded-entry` **不同名**）；
 * 2. 它**不**混进会话时间线的降级列表：同一会话的页面渲染里没有它的降级条目；
 * 3. 诊断入口按 `eventId` 打开的是**同一份**既有抽屉：受控展开后抽屉里出现该事件的元数据；
 *    仓库侧 `openDiagnostics(eventId)` 也能取到同一条模型（会话页用的就是这条路径）。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationStream, HostConnectionPanel } from "./index";
import type { ClientStore } from "../features/client-store";
import type { HostPanelModel } from "../features/host-panel-model";
import { ONLINE_SESSION_ID, eventMessage, resources, session, storeWith } from "../features/testing/fixtures";
import type { StoreFixture } from "../features/testing/fixtures";

/** 节点级事件的标识（没有 `sessionId`）。 */
const NODE_EVENT_ID = "dddddddd-0000-4000-8000-000000000601";

/** 一个喂入了节点级降级事件（`device.revoked`）的仓库。 */
function storeWithNodeEvent(): StoreFixture {
  const fixture = storeWith({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "idle" })] }) });
  fixture.store.apply({
    kind: "event",
    event: eventMessage({
      eventType: "device.revoked",
      eventId: NODE_EVENT_ID,
      sessionId: null,
      globalSequence: "81",
      view: { deviceId: "another-device" },
    }),
  });
  return fixture;
}

/** 渲染主机面板（可受控注入展开态）。 */
function renderPanel(model: HostPanelModel, openDiagnosticsEventId?: string | null): string {
  return renderToStaticMarkup(
    createElement(HostConnectionPanel, {
      model,
      ...(openDiagnosticsEventId === undefined
        ? {}
        : { openDiagnosticsEventId, onOpenDiagnostics: () => undefined }),
    }),
  );
}

/** 渲染会话页（用来证明节点级事件没有混进会话时间线）。 */
function renderSessionPage(store: ClientStore): string {
  const model = store.conversationPage(ONLINE_SESSION_ID);
  if (model === null) throw new Error("会话页模型不应为 null");
  return renderToStaticMarkup(
    createElement(ConversationStream, {
      model,
      onLoadEarlier: () => undefined,
      onOpenDiagnostics: () => undefined,
    }),
  );
}

/** 含 `marker` 的那个标签的原文（从 `<` 到 `>`）。 */
function tagContaining(html: string, marker: string): string {
  const at = html.indexOf(marker);
  expect(at, `渲染结果里没有 ${marker}`).toBeGreaterThanOrEqual(0);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

describe("节点级降级：落在主机/连接面", () => {
  it("没有会话的降级事件出现在节点级列表里，带自己的锚点与诊断入口", () => {
    const { store } = storeWithNodeEvent();
    const model = store.hostPanel();
    expect(model.nodeDegraded.map((item) => item.eventId)).toEqual([NODE_EVENT_ID]);

    const html = renderPanel(model);

    expect(html).toContain('data-node-degraded-list="events"');
    expect(html).toContain('data-node-degraded-count="1"');
    expect(html).toContain(`data-node-degraded-entry="${NODE_EVENT_ID}"`);
    expect(html).toContain('data-node-degraded-event-type="device.revoked"');
    // 降级卡片本身的锚点仍在（事件类型原样呈现 + 没有专用视图的明确说明）。
    expect(html).toContain('data-degraded-event="device.revoked"');
    expect(html).toContain('data-degraded-view="no-dedicated-view"');
    expect(html).toContain("没有会话可挂");
    // 诊断入口按 `eventId`。
    expect(html).toContain('data-action="open-diagnostics"');
    expect(html).toContain(`data-event-id="${NODE_EVENT_ID}"`);
    // 列表语义：行是 `<li>`。
    expect(tagContaining(html, `data-node-degraded-entry="${NODE_EVENT_ID}"`).startsWith("<li")).toBe(true);
  });

  it("锚点与会话时间线的降级条目不重名（两个面不会互相顶替）", () => {
    const { store } = storeWithNodeEvent();
    const html = renderPanel(store.hostPanel());

    // 反例：节点级条目复用了会话时间线的锚点 → 下面这条会红（也就无法断言「不混」）。
    expect(html).not.toContain("data-degraded-entry=");
    expect(html).not.toContain('data-timeline-card="degraded"');
  });

  it("受控展开：同一条模型在既有诊断抽屉里出现（元数据 + 降级原因）", () => {
    const { store } = storeWithNodeEvent();
    const html = renderPanel(store.hostPanel(), NODE_EVENT_ID);

    expect(html).toContain('data-diagnostics-drawer="open"');
    expect(html).toContain('data-metadata="event-type"');
    expect(html).toContain("device.revoked");
    expect(html.match(/data-metadata="event-id"/g) ?? []).toHaveLength(1);
    expect(html).toContain(`>${NODE_EVENT_ID}<`);
    expect(html).toContain('data-degraded-reason="no_dedicated_view"');
    // 没有随附原文时抽屉里没有原文槽位（只有一句说明）。
    expect(html).toContain('data-raw-unavailable="absent"');
    expect(html).not.toContain('data-raw-json="available"');
  });

  it("未受控：初始不展开抽屉（入口在位，展开态是这一面的视图内部状态）", () => {
    const { store } = storeWithNodeEvent();
    const html = renderPanel(store.hostPanel());

    expect(html).not.toContain('data-diagnostics-drawer="open"');
    expect(html).toContain('data-action="open-diagnostics"');
  });

  it("没有节点级降级事件时不渲染这一节（R19 的既有呈现不受影响）", () => {
    const { store } = storeWith({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) });
    const html = renderPanel(store.hostPanel());

    expect(html).not.toContain("data-node-degraded-list");
    expect(html).not.toContain("data-node-degraded-entry");
  });
});

describe("节点级降级：不混进会话时间线", () => {
  it("会话页渲染里没有这条节点级事件的降级条目，而主机面有", () => {
    const { store } = storeWithNodeEvent();
    const sessionHtml = renderSessionPage(store);
    const hostHtml = renderPanel(store.hostPanel());

    // 会话页：时间线容器在（页面正常渲染），但没有这条事件的降级条目，也没有节点级锚点。
    expect(sessionHtml).toContain('data-timeline="entries"');
    expect(sessionHtml).not.toContain(`data-degraded-entry="${NODE_EVENT_ID}"`);
    expect(sessionHtml).not.toContain("data-node-degraded-entry");
    // 主机面：它在。
    expect(hostHtml).toContain(`data-node-degraded-entry="${NODE_EVENT_ID}"`);

    const page = store.conversationPage(ONLINE_SESSION_ID);
    expect(page?.degraded.some((item) => item.eventId === NODE_EVENT_ID)).toBe(false);
    expect(store.nodeDegraded().some((item) => item.eventId === NODE_EVENT_ID)).toBe(true);
  });

  it("仓库侧：`openDiagnostics(eventId)` 取到的就是同一条节点级模型（会话页走的同一路径）", () => {
    const { store } = storeWithNodeEvent();

    store.openDiagnostics(NODE_EVENT_ID);
    const model = store.diagnostics();

    expect(model?.eventId).toBe(NODE_EVENT_ID);
    expect(model?.eventType).toBe("device.revoked");
    expect(model?.reason).toEqual({ kind: "no_dedicated_view" });
    // 反例：`nodeDegraded()` 给出的 id 若与仓库可打开的 id 不一致，这里会是 null。
    expect(store.nodeDegraded().map((item) => item.eventId)).toContain(model?.eventId);
  });
});
