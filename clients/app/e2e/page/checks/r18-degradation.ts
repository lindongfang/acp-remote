/**
 * R18「未识别与不可用的结构化内容显式降级」的浏览器级验证。
 *
 * 事件不是构造出来的模型，而是**页内假主机在真实连接上投递的 `event` 帧**：经真实
 * `SyncConnection` 解码、真实去重账本判重排序、`ClientStore.apply` 归并，最后由
 * `app/session/[id].tsx` → `ConversationStream` → `DegradedEventCard` / `DiagnosticsDrawer` 渲染。
 *
 * ## 判别力
 *
 * - 「原文未下发」用**负向断言**：`pre[data-raw-json]` 必须不存在（占位原文会在这里出现）。
 * - 「内容未下发 ≠ 客户端不支持」是 WP7 第 1 轮的 MAJOR-2：断言整页**不出现**
 *   `data-unsupported-by` 与「当前客户端未提供专用视图」，只允许出现在真正的不支持卡片上。
 * - 每条负向断言都配**对照**事件（原文可用且事件已知 → 不产生降级卡片），
 *   否则「什么都不渲染」也能蒙混过关。
 */

import {
  AGENTS,
  SESSION_ALPHA,
  WORKSPACES_WITH_PATHS,
  emptyReadResult,
  sessionRoute,
  wireEvent,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  click,
  domHtml,
  expectEqual,
  expectNone,
  expectThat,
  mountScenario,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";

/** 原文未下发（单条上限）。 */
const EVENT_RAW_NOT_SYNCED = "eeeeeeee-0000-4000-8000-000000000001";
/** 二进制引用未下发。 */
const EVENT_BINARY = "eeeeeeee-0000-4000-8000-000000000002";
/** 三类能力不支持。 */
const EVENT_UNSUPPORTED_CLIENT = "eeeeeeee-0000-4000-8000-000000000003";
const EVENT_UNSUPPORTED_BROKER = "eeeeeeee-0000-4000-8000-000000000004";
const EVENT_UNSUPPORTED_AGENT = "eeeeeeee-0000-4000-8000-000000000005";
/** 未登记事件类型，无原文。 */
const EVENT_UNKNOWN_TYPE = "eeeeeeee-0000-4000-8000-000000000006";
/** 未登记事件类型，但原文可用：诊断抽屉据此给出 rawJson。 */
const EVENT_UNKNOWN_WITH_RAW = "eeeeeeee-0000-4000-8000-000000000007";
/** 对照：事件类型已知且原文可用 → 不得产生降级卡片。 */
const EVENT_NO_DEGRADATION = "eeeeeeee-0000-4000-8000-000000000008";

/** 抽屉里要逐字出现的原文。 */
const RAW_JSON_TEXT = '{"widget":"vendor","n":7}';

/** 原文未下发事件的字节长度与摘要（诊断抽屉据此呈现**元数据**而不是原文）。 */
const RAW_BYTE_LENGTH = "281344";
const RAW_SHA256 = "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0aB2cD5eF8gH1iJ4kL7mN0p";

/** 会话页场景：一条会话 + 上面那批事件。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: [
      {
        sessionId: SESSION_ALPHA,
        title: "降级验证会话",
        agent: { agentId: "codex-a", name: "codex" },
        state: "idle",
        origin: { kind: "local" },
        currentMode: null,
        version: "1",
        createdAt: "2026-10-01T00:00:00.000Z",
        updatedAt: "2026-10-01T00:00:00.000Z",
        workspace: { alias: "alpha", displayName: "alpha" },
      },
    ],
    agents: AGENTS,
    scopes: ["session.list", "session.read"],
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: degradationEvents(),
    readResult: (sessionId) => emptyReadResult(sessionId),
    route: sessionRoute(SESSION_ALPHA),
    ...overrides,
  };
}

/** 七条降级事件 + 一条对照事件；序号严格递增且大于屏障。 */
function degradationEvents(): readonly Record<string, unknown>[] {
  return [
    wireEvent({
      globalSequence: 11,
      eventId: EVENT_RAW_NOT_SYNCED,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: { block: { type: "text", text: "结果" } },
      acp: {
        mediaType: "application/json",
        rawUnavailable: { reason: "size_limit", byteLength: RAW_BYTE_LENGTH, sha256: RAW_SHA256 },
      },
    }),
    wireEvent({
      globalSequence: 12,
      eventId: EVENT_BINARY,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: { block: { type: "image_ref", mimeType: "image/png", displayState: "not_fetched" } },
    }),
    wireEvent({
      globalSequence: 13,
      eventId: EVENT_UNSUPPORTED_CLIENT,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: {
        error: { code: "capability.unsupported_by_client", message: "客户端没有这个块的渲染器" },
        reason: "客户端缺渲染器",
      },
    }),
    wireEvent({
      globalSequence: 14,
      eventId: EVENT_UNSUPPORTED_BROKER,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: {
        error: { code: "capability.unsupported_by_broker", message: "服务端不认识这个块类型" },
        reason: "服务端不认识",
      },
    }),
    wireEvent({
      globalSequence: 15,
      eventId: EVENT_UNSUPPORTED_AGENT,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: {
        error: { code: "capability.unsupported_by_agent", message: "Agent 不支持这个动作" },
        reason: "Agent 不支持",
      },
    }),
    wireEvent({
      globalSequence: 16,
      eventId: EVENT_UNKNOWN_TYPE,
      sessionId: SESSION_ALPHA,
      eventType: "vendor.custom.widget",
      view: { widget: "vendor" },
    }),
    wireEvent({
      globalSequence: 17,
      eventId: EVENT_UNKNOWN_WITH_RAW,
      sessionId: SESSION_ALPHA,
      eventType: "vendor.custom.chart",
      view: { chart: "vendor" },
      acp: { mediaType: "application/json", rawJson: RAW_JSON_TEXT, byteLength: "27" },
    }),
    wireEvent({
      globalSequence: 18,
      eventId: EVENT_NO_DEGRADATION,
      sessionId: SESSION_ALPHA,
      eventType: "agent.message.completed",
      view: { block: { type: "text", text: "一切正常" } },
      acp: { mediaType: "application/json", rawJson: '{"ok":true}', byteLength: "11" },
    }),
  ];
}

/** 挂场景并等到降级列表出现。 */
async function withDegradedPage(
  body: (container: HTMLElement) => Promise<void>,
  overrides: Partial<ScenarioOptions> = {},
): Promise<void> {
  const mounted = await mountScenario(baseScenario(overrides));
  try {
    await waitFor(
      () => queryAll(mounted.container, "li[data-degraded-entry]").length >= 7,
      "降级卡片渲染",
    );
    await body(mounted.container);
  } finally {
    await mounted.unmount();
  }
}

/** 打开某条事件的诊断抽屉。 */
async function openDiagnostics(container: HTMLElement, eventId: string): Promise<void> {
  click(query(container, `button[data-action="open-diagnostics"][data-event-id="${eventId}"]`), `打开 ${eventId} 的抽屉`);
  await waitFor(
    () => query(container, "[data-diagnostics-drawer]") !== null,
    `诊断抽屉 ${eventId}`,
  );
}

const checks: readonly Check[] = [
  {
    name: "R18-1 原文未下发：明确说明原因与元数据，且不渲染任何原文占位",
    async run() {
      await withDegradedPage(async (container) => {
        expectThat(
          query(container, `[data-degraded-entry="${EVENT_RAW_NOT_SYNCED}"]`) !== null,
          "原文未下发的事件必须有一张降级卡片",
        );
        const entry = query(container, `[data-degraded-entry="${EVENT_RAW_NOT_SYNCED}"]`);
        expectEqual(
          query(entry, '[data-raw-not-synced="size_limit"]') !== null,
          true,
          "卡片必须给出未下发的原因",
        );
        expectThat(
          textOf(entry).includes("原文超出单条上限"),
          "卡片必须用明确的文案说明原文未下发及原因",
        );

        await openDiagnostics(container, EVENT_RAW_NOT_SYNCED);
        // MUST NOT：占位原文会出现在这个槽位上，它必须不存在。
        expectThat(
          query(container, "pre[data-raw-json]") === null,
          "原文未下发时不得渲染原文占位",
        );
        const unavailable = query(container, '[data-diagnostics-drawer] [data-raw-unavailable="size_limit"]');
        expectThat(unavailable !== null, "诊断抽屉必须说明原文未下发");
        const drawerText = textOf(unavailable);
        expectThat(
          drawerText.includes(RAW_BYTE_LENGTH) && drawerText.includes(RAW_SHA256),
          "诊断抽屉必须给出字节长度与摘要（元数据），而不是原文",
        );
        expectNone(textOf(unavailable), ['"'], "未下发原文的槽位里出现了疑似 JSON 原文");
      });
    },
  },
  {
    name: "R18-2 三类不支持可区分：client / broker / agent 三张卡片的归因与文案互不相同",
    async run() {
      await withDegradedPage(async (container) => {
        const pairs = [
          [EVENT_UNSUPPORTED_CLIENT, "client"],
          [EVENT_UNSUPPORTED_BROKER, "broker"],
          [EVENT_UNSUPPORTED_AGENT, "agent"],
        ] as const;

        const reasons = new Set<string>();
        const labels = new Set<string>();
        for (const [eventId, by] of pairs) {
          const entry = query(container, `[data-degraded-entry="${eventId}"]`);
          expectThat(entry !== null, `事件 ${eventId} 应有降级卡片`);
          const reason = query(entry, `[data-unsupported-reason="${by}"]`);
          expectThat(reason !== null, `事件 ${eventId} 的归因应为 ${by}`);
          reasons.add(reason?.getAttribute("data-unsupported-reason") ?? "");
          // 归因文案由降级条目里的详细说明给出（卡片上的 `data-unsupported-reason`
          // 是给定位用的选择器，本身不带文字）：三类的措辞必须互不相同。
          labels.add(textOf(query(entry, "[data-unsupported-detail]")));
          expectThat(
            query(entry, "[data-unsupported-detail]") !== null,
            `事件 ${eventId} 应给出不支持的详细说明`,
          );
        }
        expectEqual(reasons.size, 3, "三类归因必须互不相同");
        expectEqual(labels.size, 3, "三类文案必须互不相同");
      });
    },
  },
  {
    name: "R18-3 未登记事件类型：呈现「没有专用视图」，且不冒充任何一类能力不支持",
    async run() {
      await withDegradedPage(async (container) => {
        const entry = query(container, `[data-degraded-entry="${EVENT_UNKNOWN_TYPE}"]`);
        expectThat(entry !== null, "未登记事件类型也必须显式降级，不得静默隐藏");
        expectThat(
          query(entry, '[data-degraded-view="no-dedicated-view"]') !== null,
          "未登记事件类型必须说明客户端没有专用视图",
        );
        // 未登记 ≠ 「客户端能力不支持」：后者带 unsupportedBy，这里必须是空。
        expectThat(
          query(entry, "[data-unsupported-reason]") === null,
          "未登记事件类型不得被呈现为三类能力不支持之一",
        );
        expectEqual(
          query(entry, "[data-degraded-event]")?.getAttribute("data-degraded-event") ?? null,
          "vendor.custom.widget",
          "未登记的事件类型必须原样显示",
        );
      });
    },
  },
  {
    name: "R18-4 二进制未下发：呈现为「内容没下发」，不得谎称客户端不支持（MAJOR-2 回归）",
    async run() {
      await withDegradedPage(async (container) => {
        const entry = query(container, `[data-degraded-entry="${EVENT_BINARY}"]`);
        expectThat(entry !== null, "二进制未下发的事件必须有降级卡片");
        expectThat(
          query(entry, '[data-binary-not-delivered="image/png"]') !== null,
          "必须说明二进制内容没有下发",
        );
        expectThat(query(entry, "[data-unsupported-reason]") === null, "不得谎称客户端能力不支持");
        expectThat(
          query(entry, '[data-degraded-view="no-dedicated-view"]') === null,
          "内容未下发时不得宣称「客户端未提供专用视图」",
        );
        expectNone(domHtml(entry ?? container), ["data-unsupported-by"], "内容未下发时不得输出 data-unsupported-by");
      });
    },
  },
  {
    name: "R18-5 原文未下发同样不得谎称客户端不支持（两类内容缺失互不污染）",
    async run() {
      await withDegradedPage(async (container) => {
        const entry = query(container, `[data-degraded-entry="${EVENT_RAW_NOT_SYNCED}"]`);
        expectThat(query(entry, "[data-unsupported-reason]") === null, "原文未下发不得被归为能力不支持");
        expectThat(
          query(entry, '[data-degraded-view="no-dedicated-view"]') === null,
          "原文未下发时客户端是有视图的，不得宣称没有专用视图",
        );
        expectNone(domHtml(entry ?? container), ["data-unsupported-by"], "原文未下发不得输出 data-unsupported-by");
      });
    },
  },
  {
    name: "R18-6 诊断抽屉对原文可用的事件给出 rawJson 原文",
    async run() {
      await withDegradedPage(async (container) => {
        await openDiagnostics(container, EVENT_UNKNOWN_WITH_RAW);
        const raw = query(container, 'pre[data-raw-json="available"]');
        expectThat(raw !== null, "原文可用时诊断抽屉必须给出 rawJson");
        expectEqual(textOf(raw), RAW_JSON_TEXT, "抽屉里的原文必须与服务端下发的逐字一致");
        // 元数据无论原文是否可用都必须完整呈现。
        expectEqual(
          textOf(query(container, '[data-metadata="event-id"]')),
          EVENT_UNKNOWN_WITH_RAW,
          "抽屉的事件 ID 元数据",
        );
        expectEqual(
          textOf(query(container, '[data-metadata="event-type"]')),
          "vendor.custom.chart",
          "抽屉的事件类型元数据",
        );
        expectThat(
          textOf(query(container, '[data-degraded-reason="no_dedicated_view"]')).length > 0,
          "抽屉必须说明降级原因",
        );
      });
    },
  },
  {
    name: "R18-7 对照：事件类型已知且原文可用时不产生降级卡片（证明上一批不是因为一律降级）",
    async run() {
      await withDegradedPage(async (container) => {
        expectThat(
          query(container, `[data-degraded-entry="${EVENT_NO_DEGRADATION}"]`) === null,
          "事件已知且原文可用时不得产生降级卡片",
        );
        const entries = queryAll(container, "li[data-degraded-entry]");
        expectEqual(entries.length, 7, "降级卡片总数（八条事件里只有一条不降级）");
        expectEqual(
          queryAll(container, "pre[data-raw-json]").length,
          0,
          "抽屉未打开时页面上不得出现原文槽位",
        );
      });
    },
  },
];

export const R18_CHECKS = checks;