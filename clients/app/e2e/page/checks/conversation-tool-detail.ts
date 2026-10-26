/**
 * 工具详情的浏览器级验证（`tool.call.*` 的 `toolDetail` 三部分、截断说明、ACP 原文抽屉）
 * 与代码卡的行号/复制载荷。
 *
 * ## 判别力
 *
 * - **详情只来自结构化视图**：夹具的 `acp.rawJson` 里埋了一个只该出现在诊断抽屉里的标记；
 *   未打开抽屉时整页文本不得出现它。把 `rawJson` 解析成详情、或用它补造输入/输出，两条
 *   `data-tool-detail-part` 断言与这条负向断言同时变红。
 * - **截断要说明**：`truncatedParts: ["output"]` 必须渲染成对应的「输出因超过上限未下发」，
 *   而不是静默缺一块；把截断登记丢掉时这条与 `data-tool-detail-truncated` 都红。
 * - **原文按钮打开非空抽屉**：点击后抽屉必须出现、`data-raw-json="available"` 的文本必须
 *   逐字等于下发的原文。只渲染一个按钮、或抽屉里是空槽，都会在等待/逐字比较上红。
 * - **行号不进复制载荷**：剪贴板替身收到的必须是源码本身；行号列单独渲染，不掺进
 *   `data-code-line-text`，也不掺进复制内容。
 */

import {
  DIRECTORY_ID_ALPHA,
  SESSION_ALPHA,
  WORKSPACES_WITH_PATHS,
  readMessage,
  readPage,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check } from "../support";
import { TURN_ID, diagnosticsRoute, sessionScenario, withSessionPage } from "./conversation-timeline";

/** 工具调用标识与事件标识（与其它检查集不重复）。 */
const TOOL_CALL_ID = "d0000000-0000-4000-8000-000000000601";
const EVENT_TOOL_DETAIL = "d0000000-0000-4000-8000-000000000602";

/** 只存在于 ACP 原文里的标记：未打开抽屉时不得出现在页面文本里。 */
const RAW_TOOL_JSON = JSON.stringify({
  toolDetail: {
    input: { path: "RAW_TOOL_MARKER" },
    output: { bytes: 0 },
  },
});

/** 代码消息：三行源码，行号由 `CodeBlock` 单独一列渲染。 */
const CODE_LINES = ["const a = 1;", "const answer = 42;", "export { answer };"] as const;
const CODE_SOURCE = CODE_LINES.join("\n");
const CODE_MESSAGE_ID = "d0000000-0000-4000-8000-000000000611";

/** Markdown 的原文能力（客户端不校验，但夹具按协议给全）。 */
function rawAcp(rawJson: string): Record<string, unknown> {
  return {
    mediaType: "application/json",
    rawJson,
    byteLength: String(rawJson.length),
    sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0a",
  };
}

const checks: readonly Check[] = [
  {
    name: "CP-14 工具详情：input/output/text、截断说明与打开非空原文抽屉；代码卡行号不掺进正文与复制载荷",
    async run() {
      await withSessionPage(
        sessionScenario({
          // `open-tool-call-diagnostics` 只在诊断模式（`?diagnostics=1`）渲染。
          route: diagnosticsRoute(),
          workspaces: WORKSPACES_WITH_PATHS,
          sessions: [
            sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" }),
          ],
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_TOOL_DETAIL,
              sessionId: SESSION_ALPHA,
              eventType: "tool.call.started",
              createdAt: "2026-10-01T00:10:00.000Z",
              view: {
                toolCallId: TOOL_CALL_ID,
                turnId: TURN_ID,
                title: "读取 src/a.ts",
                state: "in_progress",
                toolDetail: {
                  input: { path: "src/a.ts", limit: 10 },
                  output: { bytes: 42 },
                  text: ["第一段", "第二段"],
                  truncatedParts: ["output"],
                },
              },
              acp: rawAcp(RAW_TOOL_JSON),
            }),
          ],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: CODE_MESSAGE_ID,
                  sessionId,
                  role: "agent",
                  text: ["```ts", CODE_SOURCE, "```"].join("\n"),
                  createdAt: "2026-10-01T00:11:00.000Z",
                }),
              ],
            }),
        }),
        (container) =>
          query(container, `[data-tool-call-id="${TOOL_CALL_ID}"] [data-tool-detail="present"]`) !== null &&
          query(container, "[data-code-block]") !== null,
        "工具详情与代码卡渲染",
        async (mounted) => {
          const container = mounted.container;
          const card = query(container, `[data-tool-call-id="${TOOL_CALL_ID}"]`);
          expectThat(card !== null, "工具调用卡片必须渲染");

          // 进行中的工具调用默认展开：详情不是「在 DOM 里但不可见」。
          const foldBody = query(card, "[data-fold-body]");
          expectThat(foldBody !== null, "工具调用卡片必须有折叠块正文");
          expectEqual(
            foldBody.getAttribute("data-fold-body"),
            "expanded",
            "进行中的工具调用默认展开",
          );
          expectThat(
            getComputedStyle(foldBody).display !== "none",
            "默认展开的折叠块正文必须可见",
          );

          // 三部分各一处，且内容逐字来自结构化视图。
          expectEqual(
            queryAll(card, "[data-tool-detail-part]").map((node) => node.getAttribute("data-tool-detail-part")),
            ["input", "output", "text"],
            "参数 / 输出 / 文本内容三部分必须各呈现一次",
          );
          const inputText = textOf(query(card, '[data-tool-detail-value="input"]'));
          expectThat(inputText.includes('"path": "src/a.ts"'), `参数必须逐字来自结构化详情；实际「${inputText}」`);
          expectThat(inputText.includes('"limit": 10'), "参数必须保留结构化字段与取值");
          const outputText = textOf(query(card, '[data-tool-detail-value="output"]'));
          expectThat(outputText.includes('"bytes": 42'), `输出必须逐字来自结构化详情；实际「${outputText}」`);
          const textText = textOf(query(card, '[data-tool-detail-value="text"]'));
          expectThat(textText.includes("第一段") && textText.includes("第二段"), "文本内容必须逐段呈现");

          // 截断：只有 output 被截断，且明确说明它没有完整下发。
          expectEqual(
            queryAll(card, "[data-tool-detail-truncated]").map((node) =>
              node.getAttribute("data-tool-detail-truncated"),
            ),
            ["output"],
            "截断登记必须逐部分呈现",
          );
          expectThat(
            textOf(query(card, '[data-tool-detail-truncated="output"]')).includes("输出因超过上限未下发"),
            "截断的部分必须说明原因",
          );

          // 未打开抽屉：原文里的标记一次都不出现在页面文本里（详情不是从原文补造的）。
          expectEqual(
            query(container, '[data-diagnostics-drawer="open"]'),
            null,
            "未点击前不得打开诊断抽屉",
          );
          expectAbsent(domText(container), "RAW_TOOL_MARKER", "未打开诊断时页面上出现了 ACP 原文");

          // 原文按钮：打开非空抽屉，原文逐字呈现。
          click(
            query(
              card,
              `[data-action="open-tool-call-diagnostics"][data-event-id="${EVENT_TOOL_DETAIL}"]`,
            ),
            "查看 ACP 原文",
          );
          await waitFor(
            () => query(container, '[data-diagnostics-drawer="open"]') !== null,
            "诊断抽屉打开",
          );
          expectEqual(
            textOf(query(container, '[data-metadata="event-id"]')),
            EVENT_TOOL_DETAIL,
            "抽屉必须指向被查看的那条事件",
          );
          const raw = query(container, '[data-raw-json="available"]');
          expectThat(raw !== null, "原文可用时抽屉必须给出原文槽位");
          expectThat(textOf(raw).length > 0, "原文槽位不得为空");
          expectEqual(textOf(raw), RAW_TOOL_JSON, "抽屉里的原文必须与下发内容逐字一致");

          // ── 代码卡：逐行锚点、行号独立一列、复制载荷是源码本身。──────────────────
          const codeBlock = query(container, "[data-code-block]");
          expectThat(codeBlock !== null, "代码卡必须渲染");
          expectEqual(
            query(codeBlock, "[data-code-text]")?.getAttribute("data-code-line-count"),
            String(CODE_LINES.length),
            "代码卡的行数",
          );
          expectEqual(
            queryAll(codeBlock, "[data-code-line]").map((node) => node.getAttribute("data-code-line")),
            ["1", "2", "3"],
            "每行都必须带自己的 `data-code-line`",
          );
          expectEqual(
            queryAll(codeBlock, "[data-code-line-number]").map((node) => textOf(node)),
            ["1", "2", "3"],
            "行号列必须逐行呈现",
          );
          // 正文列恰好是源码，行号不掺进去。
          expectEqual(
            queryAll(codeBlock, "[data-code-line-text]").map((node) => textOf(node)),
            [...CODE_LINES],
            "逐行正文必须与源码一致（行号不属于正文）",
          );

          // 复制：用剪贴板替身读出真正交给 `navigator.clipboard` 的载荷。
          const ownClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
          const copied: string[] = [];
          Object.defineProperty(navigator, "clipboard", {
            configurable: true,
            value: {
              writeText: (text: string): Promise<void> => {
                copied.push(text);
                return Promise.resolve();
              },
            },
          });
          try {
            click(query(codeBlock, 'button[data-action="copy-code"]'), "复制代码");
            await waitFor(() => copied.length > 0, "复制载荷到达剪贴板");
            expectEqual(copied[0], CODE_SOURCE, "复制载荷必须是源码本身，不含行号");
          } finally {
            if (ownClipboard === undefined) Reflect.deleteProperty(navigator, "clipboard");
            else Object.defineProperty(navigator, "clipboard", ownClipboard);
          }
        },
      );
    },
  },
];

export const CONVERSATION_TOOL_DETAIL_CHECKS = checks;
