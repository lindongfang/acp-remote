/**
 * 对话页「变更文件右栏」「来源离线降级」「富文本按不可信输入渲染」的浏览器级验证
 * （`specs/pwa-web-client/spec.md` 的三条 Requirement；`plan.md` 的 Main E2E 后三条）。
 *
 * ## 判别力（把实现反着写，哪一条会红）
 *
 * - **右栏不含差异正文**：夹具的 `acp.rawJson` 是一份真的 unified diff（含两行绝不会自然出现在
 *   页面上的标记行与一条本机绝对路径）。未展开诊断时断言整页**一个标记片段都不出现**，
 *   并断言 `[data-change-diagnostics]` 数为 0；展开后断言原文**逐字**出现在诊断槽位里。
 *   把原文解析成差异行渲染，或把诊断槽位当成默认呈现，两种回退都会红。
 * - **来源离线降级**：会话摘要的来源声明为其它节点且离线，断言结构化历史的不可用说明、
 *   输入区「草稿只在内存」的标注、发送入口禁用与不可点，以及**没有任何 `data-submission`**。
 *   同一用例另挂一个本机会话做**对照**（同样的历史条目 → 没有不可用说明、卡片照常呈现）：
 *   证明那句说明不是无条件渲染的。
 * - **富文本不可信**：断言消息条目里没有 `script`/`img`/`iframe`，没有任何 `onerror`/`src`/`href`
 *   属性；原始 HTML 只作为**不可信文本**出现（不产生元素），地址不进任何可加载/可导航位置，
 *   且 `window.__pwned` 仍是 `undefined`
 *   （脚本真的没执行——只看「渲染出文本」证明不了这一点）。
 *
 *   登记：本页现在按生产口径带 CSP（取值为 `e2e/run-e2e.mjs` 的 `STATIC_HEADERS`，其中
 *   `script-src 'self'` 不含 `unsafe-inline`），因此 `window.__pwned` 这条**运行时**断言由
 *   浏览器内核保证：夹具 `RICH_TEXT` 里的两条执行路径——`<script>window.__pwned = true</script>`
 *   （经 innerHTML 插入本就不执行）与 `<img … onerror="window.__pwned = true">`（内联事件属性被
 *   `script-src` 拦下，且远端图又被回落到 `default-src 'self'` 的 `img-src` 一并拦下）——内联事件
 *   属性与远端图都不是渲染器能选择执行与否的东西，内核会连它们一起挡掉；即使渲染器真的把这段
 *   HTML 注进 DOM，这条断言也不可能变红。
 *   判别力因此改由同用例的 DOM 断言承担：条目里 `script`/`img`/`iframe` 计数为 0、没有
 *   `onerror`/`src` 属性、原始 HTML 只作为文本出现（渲染器一旦把它当标记解析，这几条就红）。
 *   保留这条运行时断言，是因为在**不带 CSP** 的面上（本地静态服务或将来别的托管形态）它仍是
 *   唯一的运行时证据，而它的夹具与断言在那一面上同样成立。
 */

import {
  DIRECTORY_ID_ALPHA,
  SESSION_ALPHA,
  historyItem,
  readMessage,
  readPage,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  click,
  domHtml,
  domText,
  expectAbsent,
  expectEqual,
  expectNone,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check } from "../support";
import { FILE_DIFF_UNAVAILABLE_LABELS } from "../../../src/domain";
import {
  MESSAGE_ID,
  TOOL_CALL_ID,
  TURN_ID,
  commandBody,
  sessionScenario,
  withSessionPage,
} from "./conversation-timeline";

declare global {
  interface Window {
    /** 富文本夹具里的脚本会写的标记；不执行时必须是 `undefined`。 */
    __pwned?: unknown;
  }
}

/** 其它节点（来源离线会话的 owner）。 */
const OTHER_NODE = "f0000000-0000-4000-8000-0000000000aa";
/** 导出世代（来源块的必填字段）。 */
const ORIGIN_EPOCH = "f0000000-0000-4000-8000-0000000000bb";

/** 一处改动的标识与事件标识。 */
const CHANGE_ID = "f0000000-0000-4000-8000-000000000101";
const EVENT_FILE_CHANGED = "f0000000-0000-4000-8000-000000000111";
const EVENT_FILE_CHANGED_NO_STATS = "f0000000-0000-4000-8000-000000000112";
/** 只有不可用标记（未下发差异行）的那一处。 */
const CHANGE_ID_UNAVAILABLE = "f0000000-0000-4000-8000-000000000103";
const EVENT_FILE_CHANGED_UNAVAILABLE = "f0000000-0000-4000-8000-000000000113";

/** 未下发的差异正文（只会作为**不可信原文**出现在诊断槽位里）。 */
const RAW_DIFF = [
  "--- a/D:\\secret\\raw-diff-only\\app.ts",
  "+++ b/D:\\secret\\raw-diff-only\\app.ts",
  "@@ -1,2 +1,3 @@",
  "-DIFF_BODY_LINE_REMOVED",
  "+DIFF_BODY_LINE_ADDED",
].join("\n");

/** 诊断槽位里的原文必须是**逐字**这一份。 */
const DIFF_MARKERS = ["DIFF_BODY_LINE_REMOVED", "DIFF_BODY_LINE_ADDED"] as const;

/**
 * 服务端随事件下发的**结构化**差异行（三档各一行）。
 *
 * 行内容刻意与上面那份原文里的标记不同：右栏出现的必须是这一份（结构化来源），而不是从原文里
 * 还原出来的东西——「未展开诊断时原文片段一个都不出现」因此才有判别力。
 */
const STRUCTURED_DIFF_LINES = [
  { kind: "context", text: "const opts = resolveOptions(input);" },
  { kind: "deleted", text: "const idleTimeout = 30_000;" },
  { kind: "added", text: "const idleTimeout = opts.idleTimeoutMs ?? 10_000;" },
] as const;

/** 三档差异行的行首前缀（由客户端按 kind 补上；夹具的 `text` 只放裸原文）。 */
const STRUCTURED_DIFF_PREFIXES = ["  ", "- ", "+ "] as const;

/** 结构化差异行里出现过的文本片段（右栏必须逐字呈现它们）。 */
const STRUCTURED_DIFF_MARKERS = ["const opts = resolveOptions(input);", "const idleTimeout = 30_000;", "opts.idleTimeoutMs ?? 10_000;"] as const;

/** Markdown 的原文能力（`rawJson` 分支要求 sha256；客户端不校验，但夹具按协议给全）。 */
function rawAcp(rawJson: string): Record<string, unknown> {
  return {
    mediaType: "application/json",
    rawJson,
    byteLength: String(rawJson.length),
    sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0a",
  };
}

/** 一条 `file.changed` 视图：结构化统计 + 相对化展示路径；差异区由调用方按形态给出。 */
function fileChangedView(changeId: string, summary: string, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    changeId,
    kind: "update",
    displayPath: "src/app.ts",
    summary,
    addedLines: "2",
    deletedLines: "1",
    outsideWorkspace: false,
    ...extra,
  };
}

/** 会话页里的富文本消息：原始 HTML、内联事件属性、远端图片与链接、带语言标记的代码块。 */
const RICH_TEXT = [
  "# 部署说明",
  "",
  "普通段落。",
  "",
  "<script>window.__pwned = true</script>",
  "",
  '<img src="https://evil.example/tracker.png" onerror="window.__pwned = true">',
  "",
  "![远端图](https://evil.example/tracker.png)",
  "",
  "[远端链接](https://evil.example/page)",
  "",
  "```ts",
  "const answer = 42;",
  "```",
].join("\n");

/** 实时与历史共用的那一条工具调用（用于本机对照场景）。 */
function localHistory(): readonly Record<string, unknown>[] {
  return [
    historyItem({
      eventId: "f0000000-0000-4000-8000-000000000121",
      eventType: "tool.call.completed",
      sessionSequence: 2,
      createdAt: "2026-10-01T00:16:00.000Z",
      view: { toolCallId: TOOL_CALL_ID, turnId: TURN_ID, title: "读取部署脚本", state: "completed" },
    }),
  ];
}

const checks: readonly Check[] = [
  {
    name: "CP-7 时间线文件改动：只呈现有结构化来源的统计与相对化路径，未下发的差异正文一次都不出现",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_FILE_CHANGED,
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              view: fileChangedView(CHANGE_ID, "调整入口", { diffLines: STRUCTURED_DIFF_LINES }),
              acp: rawAcp(RAW_DIFF),
            }),
            wireEvent({
              globalSequence: 12,
              eventId: EVENT_FILE_CHANGED_NO_STATS,
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              view: { changeId: "f0000000-0000-4000-8000-000000000102", kind: "create", displayPath: "src/new.ts", summary: "新增文件" },
            }),
            wireEvent({
              globalSequence: 13,
              eventId: EVENT_FILE_CHANGED_UNAVAILABLE,
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              view: fileChangedView(CHANGE_ID_UNAVAILABLE, "diff 未能同步", {
                displayPath: "src/legacy/report.ts",
                diffUnavailable: { reason: "size_limit" },
              }),
            }),
          ],
        }),
        (container) => queryAll(container, '[data-timeline-card="file-changed"]').length === 3,
        "时间线文件改动渲染",
        async (mounted) => {
          const container = mounted.container;
          expectEqual(
            queryAll(container, '[data-timeline-card="file-changed"]').length,
            3,
            "三处改动都进入时间线",
          );
          // 文件栏不再恢复旧的文件改动清单。
          expectEqual(
            queryAll(container, '[data-conversation-column="files"] [data-file-change]').length,
            0,
            "文件栏不得再展示旧的文件改动清单",
          );

          const changed = query(container, `[data-timeline-card="file-changed"][data-change-id="${CHANGE_ID}"]`);
          expectThat(changed !== null, "结构化统计的那一处必须渲染");
          expectEqual(changed?.getAttribute("data-change-path"),
            "src/app.ts", "完整相对路径保留");
          expectEqual(
            query(changed, "[data-change-added-lines]")?.getAttribute("data-change-added-lines"),
            "2",
            "新增行数必须来自结构化统计",
          );
          expectEqual(
            query(changed, "[data-change-deleted-lines]")?.getAttribute("data-change-deleted-lines"),
            "1",
            "删除行数必须来自结构化统计",
          );
          // 判定不出的那一项：省略而不是填零。
          const noStats = query(container, '[data-timeline-card="file-changed"][data-change-id="f0000000-0000-4000-8000-000000000102"]');
          expectThat(
            query(noStats, "[data-change-line-count-unavailable]") !== null,
            "没有下发行数统计时必须明说，而不是填 +0/-0",
          );
          expectEqual(
            queryAll(container, '[data-timeline-card="file-changed"] [data-change-added-lines]').length,
            2,
            "只有有统计的那两项才出现行数锚点",
          );
          expectThat(
            textOf(query(container, "[data-change-statistics-scope]")).includes("不解析 ACP 原文"),
            "必须说明差异行只来自结构化来源、不解析 ACP 原文",
          );

          // 差异区的三种形态各就各位（退回「只有统计」的形态时，下面这一组同时变红）。
          const diffRegion = query(changed, "[data-diff-region]");
          expectEqual(
            diffRegion?.getAttribute("data-diff-region"),
            "lines",
            "下发了结构化差异行时必须逐行呈现",
          );
          expectEqual(
            queryAll(diffRegion, "[data-diff-line-kind]").map((node) => node.getAttribute("data-diff-line-kind")),
            ["context", "deleted", "added"],
            "新增 / 删除 / 上下文三档必须各自成立",
          );
          // 每行渲染文本必须**恰好**是「一个前缀 + 下发的裸原文」：夹具把前缀写进 text、
          // 或客户端补两次，都会让这里读到双前缀而判红。
          const renderedLines = queryAll(diffRegion, "[data-diff-line-kind]").map(textOf);
          expectEqual(
            renderedLines,
            STRUCTURED_DIFF_LINES.map((line, index) => `${STRUCTURED_DIFF_PREFIXES[index]}${line.text}`),
            "差异行必须只带一个由 kind 推出的行首前缀",
          );
          for (const [index, rendered] of renderedLines.entries()) {
            const prefix = STRUCTURED_DIFF_PREFIXES[index];
            expectThat(prefix !== undefined, `第 ${String(index + 1)} 行没有登记前缀`);
            expectThat(rendered.startsWith(prefix), `第 ${String(index + 1)} 行缺少行首前缀`);
            expectAbsent(rendered, `${prefix}${prefix}`, `第 ${String(index + 1)} 行出现了双前缀`);
          }
          const timelineText = domText(container);
          for (const marker of STRUCTURED_DIFF_MARKERS) {
            expectThat(timelineText.includes(marker), `差异行内容必须逐字来自结构化来源：${marker}`);
          }
          // 先取到那一项再断言它存在：缺元素时要报「有不可用标记的那处改动没渲染」。
          const unavailableChange = query(container, `[data-timeline-card="file-changed"][data-change-id="${CHANGE_ID_UNAVAILABLE}"]`);
          expectThat(unavailableChange !== null, "有不可用标记的那处改动必须渲染");
          expectEqual(
            query(unavailableChange, "[data-diff-unavailable]")?.getAttribute("data-diff-unavailable"),
            "size_limit",
            "有不可用标记时必须呈现该原因的说明",
          );
          expectThat(
            domText(unavailableChange).includes(FILE_DIFF_UNAVAILABLE_LABELS.size_limit),
            "不可用标记的文案必须来自已登记的词表",
          );
          expectThat(
            query(noStats, "[data-diff-not-delivered]") !== null,
            "两者都没有时必须明确说明，而不是留一块空的差异区",
          );

          // 每处改动都有诊断入口。
          expectEqual(
            queryAll(container, '[data-timeline-card="file-changed"] button[data-action="open-file-change-diagnostics"]').length,
            3,
            "每处改动都要有查看 ACP 原文的诊断入口",
          );

          // MUST NOT：未展开诊断时，差异正文（含绝对路径）一个片段都不得出现。
          expectNone(domText(container), DIFF_MARKERS, "未展开诊断时页面上出现了未下发的差异正文");
          expectAbsent(
            domHtml(container),
            "D:\\secret\\raw-diff-only",
            "呈现了未下发的差异正文里的本机路径",
          );
          expectEqual(
            queryAll(container, "[data-change-diagnostics]").length,
            0,
            "未展开诊断时不得渲染任何诊断面板",
          );

          // 展开诊断：原文按不可信文本逐字呈现，且只出现在这一个槽位里。
          click(
            query(container, `button[data-action="open-file-change-diagnostics"][data-event-id="${EVENT_FILE_CHANGED}"]`),
            "展开诊断",
          );
          await waitFor(
            () => query(container, `[data-change-diagnostics="${EVENT_FILE_CHANGED}"]`) !== null,
            "诊断面板展开",
          );
          const raw = query(container, '[data-change-diagnostics] [data-raw-json="available"]');
          expectThat(raw !== null, "原文可用时诊断面板必须给出原文");
          expectEqual(textOf(raw), RAW_DIFF, "诊断面板里的原文必须与服务端下发的逐字一致");
          expectEqual(
            queryAll(container, '[data-raw-json="available"]').length,
            1,
            "原文只允许出现在诊断槽位里",
          );
          // 原文是诊断入口而不是渲染源：展开原文不会多出任何差异行（那三行只来自结构化来源）。
          expectEqual(
            queryAll(container, "[data-diff-line-kind]").length,
            3,
            "原文不得被解析成差异行",
          );
        },
      );
    },
  },
  {
    name: "CP-8 来源为其它节点的会话：结构化历史显式降级，输入不被标记为已发送（本机会话作对照）",
    async run() {
      const remoteOptions = sessionScenario({
        sessions: [
          sessionSummary({
            id: SESSION_ALPHA,
            directoryId: DIRECTORY_ID_ALPHA,
            state: "idle",
            origin: { kind: "remote", ownerNodeId: OTHER_NODE, exportId: "export-1", originEpoch: ORIGIN_EPOCH, online: false },
          }),
        ],
        // 本机没有它的结构化历史：只读得到消息正文。
        readResult: (sessionId) =>
          readPage({
            sessionId,
            messages: [
              readMessage({
                messageId: MESSAGE_ID,
                sessionId,
                role: "agent",
                text: "这条会话的正文由本机缓存。",
                createdAt: "2026-10-01T00:17:00.000Z",
                turnId: TURN_ID,
              }),
            ],
          }),
      });

      await withSessionPage(
        remoteOptions,
        (container) => query(container, "[data-structured-history]") !== null,
        "结构化历史不可用的说明渲染",
        async (mounted) => {
          const container = mounted.container;
          const notice = textOf(query(container, '[data-structured-history="unavailable"]'));
          expectThat(notice.includes("不可达"), `来源不可达时必须说明不可达；实际「${notice}」`);
          expectThat(
            notice.includes("结构化内容"),
            `必须说清「取不到 ≠ 该会话没有结构化内容」；实际「${notice}」`,
          );
          expectEqual(
            queryAll(container, "[data-timeline-card]").length,
            0,
            "本机取不到结构化历史时不得凭空出现结构化卡片",
          );
          // 来源离线不再在会话头标注（该标注已移除）；功能性原因由输入区说明。
          expectEqual(query(container, "[data-origin-offline]"), null, "会话头不得再呈现来源离线标注");

          // 输入：草稿只在内存、发送入口禁用，且**不是**「已发送」。
          expectEqual(
            query(container, "[data-composer]")?.getAttribute("data-composer-draft-in-memory-only"),
            "true",
            "来源为其它节点的会话，草稿只在内存",
          );
          expectThat(
            textOf(query(container, "[data-draft-notice]")).includes("只在内存"),
            "草稿必须标注为只在内存、尚未提交",
          );
          expectThat(
            textOf(query(container, "[data-submit-unavailable]")).includes("来源节点离线"),
            "不可提交时必须说明来源节点离线",
          );
          const submit = query(container, 'button[data-action="submit-prompt"]');
          expectEqual(submit?.hasAttribute("disabled") ?? false, true, "来源为其它节点的会话不得可提交");
          click(submit, "禁用的发送入口");
          expectEqual(
            commandBody(mounted.host?.sent ?? [], "session.prompt"),
            null,
            "来源为其它节点的会话不得发出 session.prompt",
          );
          expectEqual(
            query(container, "[data-submission]"),
            null,
            "输入不得被标记为已发送（没有 submission 呈现）",
          );
        },
      );

      // 对照：同样的历史条目、来源为本机时，没有那句不可用说明，卡片照常呈现。
      await withSessionPage(
        sessionScenario({
          readResult: (sessionId) => readPage({ sessionId, history: localHistory() }),
        }),
        (container) => query(container, '[data-timeline-card="tool-call"]') !== null,
        "本机会话的结构化历史条目呈现",
        async (mounted) => {
          expectEqual(
            query(mounted.container, "[data-structured-history]"),
            null,
            "来源为本机且协商到 feature 时不得给出结构化历史不可用的说明",
          );
          expectEqual(
            queryAll(mounted.container, `[data-tool-call-id="${TOOL_CALL_ID}"]`).length,
            1,
            "本机会话的历史条目必须呈现",
          );
        },
      );
    },
  },
  {
    name: "CP-9 富文本按不可信输入渲染：脚本不执行、原始 HTML 不注入、远端图片与链接不被加载",
    async run() {
      await withSessionPage(
        sessionScenario({
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: MESSAGE_ID,
                  sessionId,
                  role: "agent",
                  text: RICH_TEXT,
                  createdAt: "2026-10-01T00:18:00.000Z",
                  turnId: TURN_ID,
                }),
              ],
            }),
        }),
        (container) => query(container, `[data-message-id="${MESSAGE_ID}"]`) !== null,
        "消息正文渲染",
        async (mounted) => {
          const container = mounted.container;
          const entry = query(container, `[data-message-id="${MESSAGE_ID}"]`);
          expectThat(entry !== null, "消息条目必须渲染");

          // 不执行：原始 HTML 片段按纯文本呈现（`data-markdown-omitted="html"`），
          // 且标记脚本真的没有跑起来（只断言「渲染出文本」证明不了这一点）。
          expectThat(
            queryAll(entry, '[data-markdown-omitted="html"]').length >= 1,
            "原始 HTML 片段必须按纯文本呈现",
          );
          expectThat(
            domText(entry).includes("<script>window.__pwned = true</script>"),
            "原始 HTML 片段必须原样作为**文本**出现，而不是被执行或吞掉",
          );
          expectEqual(window.__pwned, undefined, "Markdown 里的脚本不得执行");

          // 不注入：消息条目里不得出现任何可执行元素或内联事件属性。
          expectEqual(
            queryAll(entry, "script,img,iframe,object,embed,link").length,
            0,
            "消息正文不得产生可执行或可加载的元素",
          );
          expectEqual(
            queryAll(entry, "[onerror],[onload],[onclick],[src],[href]").length,
            0,
            "消息正文不得带内联事件属性或任何地址属性",
          );

          // 不加载远端资源：远端图片只出「未加载」说明，链接只出标签与说明。
          // 判据是「地址不进任何**可动作**位置」——原始 HTML 按纯文本呈现时，
          // 它的源码里当然含地址（那是被展示的不可信文本，不是加载目标）。
          expectThat(
            queryAll(entry, '[data-markdown-omitted="image"]').length >= 1,
            "远端图片必须降级为「未加载」说明",
          );
          expectThat(
            queryAll(entry, '[data-markdown-omitted="link"]').length >= 1,
            "远端链接必须降级为文本 + 说明",
          );
          // 地址只允许出现在被转义的**文本**里，不得出现在任何元素/属性上（上面两条已断言元素与属性集合）。
          expectEqual(
            queryAll(entry, "[data-markdown-omitted] a, [data-markdown-omitted] img").length,
            0,
            "降级说明里不得夹带可加载元素",
          );
          // 先取到那个元素再断言它存在：`domText` 只接受非 null，缺元素时要报「链接降级说明没渲染」
          // 这条可定位的失败，而不是让 `domText(null)` 抛一个看不出原因的 TypeError。
          const omittedLink = query(entry, '[data-markdown-omitted="link"]');
          expectThat(omittedLink !== null, "远端链接的降级说明必须渲染");
          expectThat(
            domText(omittedLink).includes("evil.example") === false,
            "链接降级只呈现标签与说明，不把地址当链接目标呈现",
          );

          // 代码块可读且可复制（同一 Requirement 的第三条场景）。
          const codeBlock = query(entry, "[data-code-block]");
          expectThat(codeBlock !== null, "带语言标记的代码块必须呈现");
          expectEqual(
            query(codeBlock, "[data-code-language]")?.getAttribute("data-code-language"),
            "ts",
            "代码块必须呈现语言标记",
          );
          expectThat(
            textOf(query(codeBlock, "[data-code-text]")).includes("const answer = 42;"),
            "代码块必须呈现等宽正文",
          );
          expectThat(
            query(codeBlock, 'button[data-action="copy-code"]') !== null,
            "代码块必须提供复制入口",
          );
        },
      );
    },
  },
];

export const CONVERSATION_CONTENT_CHECKS = checks;
