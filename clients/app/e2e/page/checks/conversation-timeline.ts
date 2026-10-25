/**
 * 对话页「时间线 / 结构化历史 / 呈现覆盖」的浏览器级验证（`specs/pwa-web-client/spec.md` 的
 * 「呈现覆盖由客户端自持」「对话页按时间线呈现消息与结构化条目」；`plan.md` 的 Main E2E 前两条
 * 与「已知但客户端没有专用视图」那一条）。
 *
 * 每条用例都跑在**真实页面**上：`app/session/[id].tsx` → `ConversationStream` → `ConversationTimeline`
 * → `src/features/` 的模型；事件是**页内假主机在真实连接上投递的 `event` 帧**，历史条目是
 * **假主机对 `session.read` 的应答**，两者都经真实 `SyncConnection` 解码、真实去重账本判序、
 * `ClientStore.apply` 归并。
 *
 * ## 判别力（每条都能被实现回退弄红）
 *
 * - 「两条来源只呈现一处」不是靠「模型里没有重复」证明的：实时投递的 `eventId` 与历史条目里的
 *   **同一个 `eventId`** 同时在册，断言的是**该 `toolCallId` 的卡片数恰好为 1**。历史若不参与归并，
 *   数量仍会是 1（那条来自实时）——因此同一用例另断言「只在历史里的终端输出与计划卡片确实渲染」
 *   与「出站 `session.read` 的 include 真的带了 `history`」：把历史来源整条掐掉，这两条立刻变红。
 * - 「重开仍见历史条目」的场景里实时**一条结构化事件都不投递**：卡片只可能来自 `session.read`
 *   的历史那一页。离开对话页（目录页真的渲染出来）再回来、以及整页重挂（store 全部重建）两条
 *   路径都断言；把历史 include 去掉或把历史记录丢掉，两条都红。
 * - 「未覆盖类型产出降级卡片」用的是**协议词表里存在、客户端覆盖表里没有**的类型
 *   （`session.info.changed` / `session.commands.changed` / 节点级的 `device.revoked`）。
 *   覆盖判定若退回协议词表（早先的缺陷），这些事件会既不走卡片也不降级、静默消失，
 *   断言「卡片存在」即红。
 *
 * ## 本文件同时承载本工作包共用的机关
 *
 * `sessionScenario` / `withSessionPage` / `commandBody` 由另外两个新文件单向导入（与
 * `r15-directory.ts` 承载目录页共用机关、`r15-directory-detail.ts` 反向导入同一口径）。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  DIRS_ROUTE,
  HOSTS_ROUTE,
  SESSION_ALPHA,
  WORKSPACES,
  historyItem,
  readMessage,
  readPage,
  sessionRoute,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectNone,
  expectThat,
  mountScenario,
  navigate,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, Mounted, ScenarioOptions } from "../support";

/** 轮次标识：工具调用卡片与「进行中」判定都挂在它上面。 */
export const TURN_ID = "d0000000-0000-4000-8000-000000000001";
/** 工具调用标识（`tool.call.*` 按它归并成一张卡片）。 */
export const TOOL_CALL_ID = "d0000000-0000-4000-8000-000000000002";
/** 终端标识。 */
const TERMINAL_ID = "d0000000-0000-4000-8000-000000000003";
/** 消息标识（权威正文来自 `session.read`）。 */
export const MESSAGE_ID = "d0000000-0000-4000-8000-000000000004";

/** 事件标识：实时与历史两条来源共用的那一条。 */
export const EVENT_TOOL_CALL = "d0000000-0000-4000-8000-000000000101";
/** 只出现在历史里的终端输出。 */
const EVENT_TERMINAL = "d0000000-0000-4000-8000-000000000102";
/** 只出现在历史里的计划变更。 */
const EVENT_PLAN = "d0000000-0000-4000-8000-000000000103";
/** 协议已知、客户端没有专用视图的会话级类型（两个样本）。 */
const EVENT_INFO_CHANGED = "d0000000-0000-4000-8000-000000000201";
const EVENT_COMMANDS_CHANGED = "d0000000-0000-4000-8000-000000000202";
/** 协议已知、客户端没有专用视图的**节点级**类型（没有会话可挂）。 */
const EVENT_DEVICE_REVOKED = "d0000000-0000-4000-8000-000000000203";

/** 折叠块与右栏联动检查用的事件标识（每类一条，互不重复）。 */
const EVENT_TOOL_RUN = "d0000000-0000-4000-8000-000000000301";
const EVENT_TOOL_DONE = "d0000000-0000-4000-8000-000000000302";
const EVENT_TERMINAL_TWO = "d0000000-0000-4000-8000-000000000303";
const EVENT_FILE_CHANGED = "d0000000-0000-4000-8000-000000000304";
/** 第二条**同样进行中**（默认同样展开）的工具调用：用来证明显式操作不牵连同状态的另一条。 */
const EVENT_TOOL_RUN_TWO = "d0000000-0000-4000-8000-000000000305";
/** 进行中 / 已完成的两个工具调用标识。 */
const TOOL_RUN_ID = "d0000000-0000-4000-8000-000000000311";
const TOOL_DONE_ID = "d0000000-0000-4000-8000-000000000312";
/** 第二条进行中的工具调用标识。 */
const TOOL_RUN_TWO_ID = "d0000000-0000-4000-8000-000000000314";
/** 第二块终端输出（默认收起，用于证明「显式操作只影响本条」）。 */
const TERMINAL_TWO_ID = "d0000000-0000-4000-8000-000000000313";
/** 右栏那一处改动。 */
const CHANGE_ID = "d0000000-0000-4000-8000-000000000321";
/**
 * 结构化差异行的**原文**（不含行首前缀）：前缀由客户端按 kind 自动补上，因此夹具里的 `text`
 * 必须是服务端下发的裸内容。夹具若把前缀也写进 `text`，渲染结果会出现两个前缀——这正是本用例
 * 用「逐行文本恰好等于 前缀 + 原文」来钉住的那条。
 */
const DIFF_TEXT = [
  "const opts = resolveOptions(input);",
  "const idleTimeout = 30_000;",
  "const idleTimeout = opts.idleTimeoutMs ?? 10_000;",
] as const;
/** 三档差异行的行首前缀（与 `ChangeDiff` 的 `DIFF_LINE_PREFIXES` 同值）。 */
const DIFF_PREFIXES = ["  ", "- ", "+ "] as const;
/** 未下发的差异正文（只可能作为不可信原文出现在诊断槽位里）。 */
const RAW_DIFF = JSON.stringify({ params: { update: { sessionUpdate: "tool_call_update", rawInput: { path: "docs/RAW_ONLY_MARKER.md" } } } });

/** 条目形态检查（CP-6）用的事件与轮次标识。 */
const EVENT_TURN_RUNNING = "d0000000-0000-4000-8000-000000000401";
const EVENT_TURN_FAILED = "d0000000-0000-4000-8000-000000000402";
const EVENT_TERMINAL_THREE = "d0000000-0000-4000-8000-000000000403";
const TURN_RUNNING_ID = "d0000000-0000-4000-8000-000000000411";
const TURN_FAILED_ID = "d0000000-0000-4000-8000-000000000412";

/** 工具调用视图：卡片只用到这三个字段（`domain/timeline.ts` 的 `buildTimelineCard`）。 */
function toolCallView(state: string): Record<string, unknown> {
  return { toolCallId: TOOL_CALL_ID, turnId: TURN_ID, title: "读取部署脚本", state };
}

/** 一条工具调用的 `event` 帧（实时来源）。 */
function toolCallEvent(globalSequence: number): Record<string, unknown> {
  return wireEvent({
    globalSequence,
    eventId: EVENT_TOOL_CALL,
    sessionId: SESSION_ALPHA,
    eventType: "tool.call.started",
    createdAt: "2026-10-01T00:10:00.000Z",
    view: toolCallView("running"),
  });
}

/** 历史页：同一个 `eventId` 的工具调用（实时那份必须在归并里胜出且只留一处）+ 两条只在历史里的条目。 */
function historyWithDuplicateToolCall(): readonly Record<string, unknown>[] {
  return [
    historyItem({
      eventId: EVENT_TOOL_CALL,
      eventType: "tool.call.started",
      sessionSequence: 3,
      createdAt: "2026-10-01T00:10:00.000Z",
      view: toolCallView("completed"),
    }),
    historyItem({
      eventId: EVENT_TERMINAL,
      eventType: "terminal.output",
      sessionSequence: 4,
      createdAt: "2026-10-01T00:11:00.000Z",
      view: { terminalId: TERMINAL_ID, chunkIndex: "0", stream: "stdout", text: "deploy ok", truncated: false },
    }),
    historyItem({
      eventId: EVENT_PLAN,
      eventType: "session.plan.changed",
      sessionSequence: 5,
      createdAt: "2026-10-01T00:12:00.000Z",
      view: { entries: [{ content: "跑一遍冒烟用例", priority: "high", status: "pending" }] },
    }),
  ];
}

/** 一条消息（正文权威来自读取结果，不来自任何事件）。 */
function conversationMessage(): Record<string, unknown> {
  return readMessage({
    messageId: MESSAGE_ID,
    sessionId: SESSION_ALPHA,
    role: "user",
    text: "看一下部署脚本",
    createdAt: "2026-10-01T00:09:00.000Z",
    turnId: TURN_ID,
  });
}

/** Agent 消息标识（与用户那条成对，用于比对两种气泡的形态）。 */
const AGENT_MESSAGE_ID = "d0000000-0000-4000-8000-000000000005";

/** Agent 的消息（同上，只用于气泡形态对照）。 */
function agentMessage(): Record<string, unknown> {
  return readMessage({
    messageId: AGENT_MESSAGE_ID,
    sessionId: SESSION_ALPHA,
    role: "agent",
    text: "先看 `session.ts` 的关闭路径。",
    createdAt: "2026-10-01T00:09:30.000Z",
    turnId: TURN_ID,
  });
}

/** ACP 原文（只有诊断槽位会呈现它；差异行必须来自结构化来源而不是解析它）。 */
function rawAcp(rawJson: string): Record<string, unknown> {
  return {
    mediaType: "application/json",
    rawJson,
    byteLength: String(rawJson.length),
    sha256: "kQ8x2mB7yV1cJ0nP5rT9wZa3dE6fH8iL1sU4vX7yZ0a",
  };
}

/** 某个折叠块的有效展开值（`data-entry-expanded`；折叠块缺席即用例外加一次定位判红）。 */
function foldState(container: HTMLElement, eventId: string): string {
  const fold = query(container, `[data-fold-entry="event:${eventId}"]`);
  if (fold === null) throw new CheckFailure(`容器里找不到 event:${eventId} 的折叠块`);
  return fold.getAttribute("data-entry-expanded") ?? "";
}

/** 某个锚点的计算样式；锚点缺席即判红（与 `ui-stylesheet` 同一做法）。 */
function computedStyleOf(container: HTMLElement, selector: string, label: string): CSSStyleDeclaration {
  const element = query(container, selector);
  if (element === null) throw new CheckFailure(`${label}：容器里找不到 ${selector}`);
  return getComputedStyle(element);
}

/** 会话页默认场景：一条本机自有、进行中的会话 + 两个 scope；事件与读回应答由各用例给出。 */
export function sessionScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES,
    sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" })],
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: (sessionId) => readPage({ sessionId }),
    route: sessionRoute(SESSION_ALPHA),
    ...overrides,
  };
}

/** 出站帧里某条命令的全部 `body`，按发出次序（`R16` 的口径：断言的是实际发出的那一帧字节）。 */
export function commandBodies(sent: readonly string[], command: string): readonly Record<string, unknown>[] {
  const frames: Record<string, unknown>[] = [];
  for (const text of sent) {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null) continue;
    const body = (parsed as { body?: unknown }).body;
    if (typeof body !== "object" || body === null) continue;
    const record = body as Record<string, unknown>;
    if (record["command"] === command) frames.push(record);
  }
  return frames;
}

/** 某条命令的第一帧 `body`；没有发出过则为 `null`。 */
export function commandBody(sent: readonly string[], command: string): Record<string, unknown> | null {
  return commandBodies(sent, command)[0] ?? null;
}

/** 出站 `session.read` 帧里的 include 集合（历史那一份是不是真的被要过来了，只有帧原文能回答）。 */
export function readInclude(sent: readonly string[]): readonly string[] {
  const body = commandBody(sent, "session.read");
  const payload = body === null ? null : body["payload"];
  if (typeof payload !== "object" || payload === null || !("include" in payload)) return [];
  const include: unknown = payload.include;
  if (!Array.isArray(include)) return [];
  const names: string[] = [];
  for (const value of include) {
    if (typeof value === "string") names.push(value);
  }
  return names;
}

/**
 * 进入目标页的**真实路径**：先挂目录页并等到快照落地，再导航过去。
 *
 * ## 为什么不是把会话页直接当初始路由（这是本工作包踩到的第一件事）
 *
 * `app/session/[id].tsx` 的 `useEffect` 只调用一次 `store.loadSession(sessionId)`，而组合根的
 * `start()` **不等认证**（`composition.ts:586-590` 写明了：认证之前不发任何命令），
 * `#dispatchIntent` 在没有 `connectionId` 时**静默返回 null**（既不派发也不登记线程，
 * `client-store.ts:1502`）。于是「初始就挂会话页」时，那次读取与握手是**并发**的：一旦落在
 * 认证之前就被静默丢掉，而 effect 只跑一次、此后再没有任何东西会重试——页面只剩事件带来的内容，
 * 消息与结构化历史一条都读不回来。用例因此不依赖那一刻的调度：它按真实用户的顺序
 * （先到目录页、连上之后再点进会话页）进入，那才是被测行为。
 *
 * 因此这里按用户的顺序走：`[data-directory-list="directories"]` 只在快照提交后才渲染，
 * 而快照在 `auth.authenticated` 之后才来，所以等到它就等价于「认证已完成、`connectionId` 已就位」。
 * 随后 `navigate` 与页面里的 `router.push` 是同一条路径查找。
 */
export async function enterRoute(options: ScenarioOptions): Promise<Mounted> {
  const mounted = await mountScenario({
    ...options,
    route: DIRS_ROUTE,
    extraRoutes: [options.route, ...(options.extraRoutes ?? [])],
  });
  await waitFor(
    () => query(mounted.container, '[data-directory-list="directories"]') !== null,
    "目录页等到快照（等价于认证完成）",
  );
  navigate(options.route.path);
  return mounted;
}

/** 进入目标页并等到某一条件成立，然后交给用例（`finally` 卸载：`dispose()` 后的组合根不可复活）。 */
export async function withSessionPage(
  options: ScenarioOptions,
  ready: (container: HTMLElement) => boolean,
  label: string,
  body: (mounted: Mounted) => Promise<void>,
): Promise<void> {
  const mounted = await enterRoute(options);
  try {
    await waitFor(() => ready(mounted.container), label);
    await body(mounted);
  } finally {
    await mounted.unmount();
  }
}

const checks: readonly Check[] = [
  {
    name: "CP-1 时间线同源：消息与结构化条目同框，且同一条目在实时与历史两条来源下只出现一次",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [toolCallEvent(11)],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [conversationMessage()],
              history: historyWithDuplicateToolCall(),
              historyHasEarlier: false,
            }),
        }),
        (container) => query(container, '[data-timeline-card="plan"]') !== null,
        "历史里的计划卡片渲染",
        async (mounted) => {
          const container = mounted.container;
          const timeline = query(container, '[data-timeline="entries"]');
          expectThat(timeline !== null, "对话页必须有时间线容器");

          // 同一条时间线上消息与三类结构化条目**同时**在：不是两条各自独立的列表。
          expectThat(
            query(timeline, `[data-message-id="${MESSAGE_ID}"]`) !== null,
            "消息条目必须落在时间线里",
          );
          expectThat(
            query(timeline, `[data-timeline-card="tool-call"][data-tool-call-id="${TOOL_CALL_ID}"]`) !== null,
            "工具调用卡片必须落在时间线里",
          );
          expectThat(
            query(timeline, `[data-timeline-card="terminal-output"][data-terminal-id="${TERMINAL_ID}"]`) !== null,
            "只在历史里的终端输出卡片必须落在时间线里",
          );
          expectThat(query(timeline, '[data-timeline-card="plan"]') !== null, "只在历史里的计划卡片必须落在时间线里");

          // 同一个 eventId / toolCallId 只呈现一处：历史里那一份不得再加一张卡片。
          expectEqual(
            queryAll(timeline, `[data-tool-call-id="${TOOL_CALL_ID}"]`).length,
            1,
            "同一个 toolCallId 的卡片数（实时 + 历史各来一份时应为 1）",
          );
          expectEqual(queryAll(timeline, "[data-timeline-card]").length, 3, "结构化卡片总数");
          expectEqual(
            timeline.getAttribute("data-timeline-entry-count"),
            "4",
            "时间线条目总数（1 条消息 + 3 张去重后的卡片）",
          );

          // 上面的历史条目不是「碰巧来自实时」：读取确实要了历史那一份。
          expectThat(
            commandBody(mounted.host?.sent ?? [], "session.read") !== null,
            "进入会话页必须发出 session.read",
          );
          const include = readInclude(mounted.host?.sent ?? []);
          expectThat(
            include.includes("history") && include.includes("messages"),
            `协商到 core.session-history.v1 时读取必须同时要 messages 与 history；实际 ${JSON.stringify(include)}`,
          );
        },
      );
    },
  },
  {
    name: "CP-2 重开对话页：只存在于结构化历史里的条目仍然呈现（跨路由与整页重挂两条路径）",
    async run() {
      const options = sessionScenario({
        // 实时一条结构化事件都不投递：卡片只可能来自 `session.read` 的历史那一页。
        events: [],
        readResult: (sessionId) =>
          readPage({ sessionId, messages: [conversationMessage()], history: historyWithDuplicateToolCall() }),
      });

      // 进入路径与真实用户一致（先目录页、连上之后再进会话页）：首屏读取因此真的被发出来。
      const first = await enterRoute(options);
      try {
        await waitFor(
          () => query(first.container, '[data-timeline-card="tool-call"]') !== null,
          "历史条目首次呈现",
        );

        // 离开对话页：目录页**真的**渲染出来（否则「离开」只是一句空话）。
        navigate("/");
        await waitFor(() => query(first.container, '[data-route="session"]') === null, "离开对话页");
        await waitFor(
          () => query(first.container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`) !== null,
          "目录页渲染",
        );

        // 回到同一个会话：结构化历史条目仍在，且没有因为再读一次而变成两条。
        navigate(`/session/${SESSION_ALPHA}`);
        await waitFor(
          () => query(first.container, '[data-timeline-card="terminal-output"]') !== null,
          "重进后历史条目仍在",
        );
        expectEqual(
          queryAll(first.container, `[data-tool-call-id="${TOOL_CALL_ID}"]`).length,
          1,
          "重进后同一个 toolCallId 仍只出现一处",
        );
        const rereadInclude = readInclude(first.host?.sent ?? []);
        expectThat(
          rereadInclude.includes("history"),
          `重进后的 session.read 仍必须带 history include；实际 ${JSON.stringify(rereadInclude)}`,
        );
        // 重进是**真的重新读了一次**（不是只把内存里的旧状态再画一遍）：
        // 也正因如此，「仍然呈现」才说明服务端的历史那一页确实回得来。
        expectThat(
          commandBodies(first.host?.sent ?? [], "session.read").length >= 2,
          "重进对话页必须重新发出一次 session.read",
        );
      } finally {
        await first.unmount();
      }

      // 整页重挂（等价于重新加载页面）：内存里什么都不剩，只有读取到的历史能提供这些条目。
      const second = await enterRoute(options);
      try {
        await waitFor(
          () => query(second.container, '[data-timeline-card="terminal-output"]') !== null,
          "整页重挂后历史条目仍在",
        );
        expectThat(
          query(second.container, '[data-structured-history="unavailable"]') === null,
          "协商到 feature 且来源为本机时不得给出「结构化历史不可用」的说明",
        );
        expectEqual(
          queryAll(second.container, "[data-timeline-card]").length,
          3,
          "整页重挂后的结构化卡片总数",
        );
      } finally {
        await second.unmount();
      }
    },
  },
  {
    name: "CP-3 协议已知但客户端没有专用视图的事件产出降级卡片（会话级与节点级各自都有出口）",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_INFO_CHANGED,
              sessionId: SESSION_ALPHA,
              eventType: "session.info.changed",
              view: { session: { title: "另一个标题" } },
            }),
            wireEvent({
              globalSequence: 12,
              eventId: EVENT_COMMANDS_CHANGED,
              sessionId: SESSION_ALPHA,
              eventType: "session.commands.changed",
              view: { commands: [{ name: "/deploy", description: "部署" }] },
            }),
          ],
        }),
        (container) => queryAll(container, "[data-degraded-entry]").length >= 2,
        "两条未覆盖事件都产出降级卡片",
        async (mounted) => {
          const container = mounted.container;
          const timeline = query(container, '[data-timeline="entries"]');
          expectThat(timeline !== null, "对话页必须有时间线容器");

          const samples = [
            [EVENT_INFO_CHANGED, "session.info.changed"],
            [EVENT_COMMANDS_CHANGED, "session.commands.changed"],
          ] as const;
          for (const [eventId, eventType] of samples) {
            const card = query(container, `[data-degraded-entry="${eventId}"]`);
            expectThat(card !== null, `${eventType} 必须产出降级卡片，不得静默消失`);
            expectThat(
              query(timeline, `[data-degraded-entry="${eventId}"]`) !== null,
              `${eventType} 的降级卡片必须落在会话时间线上`,
            );
            expectEqual(
              card?.getAttribute("data-degraded-event-type"),
              eventType,
              `${eventType} 必须原样显示事件类型`,
            );
            expectThat(
              query(card, '[data-degraded-view="no-dedicated-view"]') !== null,
              `${eventType} 必须说明客户端没有专用视图`,
            );
            // 「没有专用视图」≠「客户端/服务端/Agent 能力不支持」：不得冒充三类归因之一。
            expectThat(
              query(card, "[data-unsupported-reason]") === null,
              `${eventType} 不得被归为三类能力不支持`,
            );
            expectThat(
              query(card, `button[data-action="open-diagnostics"][data-event-id="${eventId}"]`) !== null,
              `${eventType} 必须保留原文与元数据的入口`,
            );
          }
          expectEqual(
            queryAll(container, "[data-node-degraded-entry]").length,
            0,
            "会话级事件不得落进节点级降级列表",
          );
        },
      );

      // 节点级（没有会话的）未覆盖事件：出口在主机/连接面的节点级降级列表（没有会话时间线可挂）。
      const revokedView = { deviceId: "d0000000-0000-4000-8000-0000000000ff", revokedAt: "2026-10-01T00:13:00.000Z" };
      await withSessionPage(
        sessionScenario({
          route: HOSTS_ROUTE,
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_DEVICE_REVOKED,
              sessionId: null,
              eventType: "device.revoked",
              view: revokedView,
            }),
          ],
        }),
        (container) => query(container, `[data-node-degraded-entry="${EVENT_DEVICE_REVOKED}"]`) !== null,
        "节点级降级条目渲染",
        async (mounted) => {
          const entry = query(mounted.container, `[data-node-degraded-entry="${EVENT_DEVICE_REVOKED}"]`);
          expectEqual(
            entry?.getAttribute("data-node-degraded-event-type"),
            "device.revoked",
            "节点级降级条目必须原样显示事件类型",
          );
          expectThat(
            query(entry, '[data-degraded-view="no-dedicated-view"]') !== null,
            "节点级未覆盖事件同样必须说明没有专用视图",
          );
          expectThat(
            query(entry, "[data-unsupported-reason]") === null,
            "节点级未覆盖事件不得被归为三类能力不支持",
          );
        },
      );
    },
  },
  {
    name: "CP-4 结构化条目的展开收起：默认值由状态决定，显式操作只影响本条",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_TOOL_RUN,
              sessionId: SESSION_ALPHA,
              eventType: "tool.call.started",
              createdAt: "2026-10-01T00:12:00.000Z",
              view: { toolCallId: TOOL_RUN_ID, turnId: TURN_ID, title: "编辑 src/auth/session.ts", state: "in_progress" },
            }),
            wireEvent({
              globalSequence: 12,
              eventId: EVENT_TOOL_DONE,
              sessionId: SESSION_ALPHA,
              eventType: "tool.call.completed",
              createdAt: "2026-10-01T00:13:00.000Z",
              view: { toolCallId: TOOL_DONE_ID, turnId: TURN_ID, title: "读取 src/auth/config.ts", state: "completed" },
            }),
            wireEvent({
              globalSequence: 13,
              eventId: EVENT_TERMINAL_TWO,
              sessionId: SESSION_ALPHA,
              eventType: "terminal.output",
              createdAt: "2026-10-01T00:14:00.000Z",
              view: { terminalId: TERMINAL_TWO_ID, chunkIndex: "0", stream: "stdout", text: "deploy ok", truncated: false },
            }),
            // 第二条**同样进行中**的条目：它与 EVENT_TOOL_RUN 默认同样展开，因此「点一条却把另一条
            // 也改了」会被下面「另一条原封不动」的断言抓住（异状态的条目默认就是 false，抓不住）。
            wireEvent({
              globalSequence: 14,
              eventId: EVENT_TOOL_RUN_TWO,
              sessionId: SESSION_ALPHA,
              eventType: "tool.call.started",
              createdAt: "2026-10-01T00:14:30.000Z",
              view: { toolCallId: TOOL_RUN_TWO_ID, turnId: TURN_ID, title: "编辑 src/auth/token.ts", state: "in_progress" },
            }),
          ],
        }),
        (container) =>
          query(container, `[data-fold-entry="event:${EVENT_TOOL_RUN}"]`) !== null &&
          query(container, `[data-fold-entry="event:${EVENT_TOOL_RUN_TWO}"]`) !== null &&
          query(container, `[data-fold-entry="event:${EVENT_TOOL_DONE}"]`) !== null &&
          query(container, `[data-fold-entry="event:${EVENT_TERMINAL_TWO}"]`) !== null,
        "四个折叠块渲染",
        async (mounted) => {
          const container = mounted.container;

          // 默认值只跟条目自己的状态走：进行中的展开、已完成的收起、终端块（没有「进行中」这一档）收起。
          expectEqual(foldState(container, EVENT_TOOL_RUN), "true", "进行中的工具调用必须默认展开");
          expectEqual(foldState(container, EVENT_TOOL_RUN_TWO), "true", "同为 in_progress 的第二条也必须默认展开");
          expectEqual(foldState(container, EVENT_TOOL_DONE), "false", "已完成的工具调用必须默认收起");
          expectEqual(foldState(container, EVENT_TERMINAL_TWO), "false", "终端输出块必须默认收起");

          // 收起是「留在 DOM 里但不可见」：内容不因为收起而消失。
          expectEqual(
            computedStyleOf(
              container,
              `[data-fold-entry="event:${EVENT_TOOL_DONE}"] [data-fold-body]`,
              "已完成工具调用的正文",
            ).display,
            "none",
            "收起的折叠块正文必须不可见",
          );

          // 显式操作：点开进行中的那一条 → 只有它收起，其它条目不变。
          click(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TOOL_RUN}"]`),
            "收起进行中的工具调用",
          );
          await waitFor(() => foldState(container, EVENT_TOOL_RUN) === "false", "该条目收起");
          expectEqual(foldState(container, EVENT_TOOL_RUN), "false", "显式收起必须覆盖默认展开");
          // 判别力：这条与上一条**默认同样展开**。若实现把「任一点击」做成「收起全部」，这里会读到
          // `false` 而变红；异状态的条目默认就是 `false`，抓不住这种实现。
          expectEqual(foldState(container, EVENT_TOOL_RUN_TWO), "true", "同为 in_progress 的另一条不受影响");
          expectEqual(foldState(container, EVENT_TOOL_DONE), "false", "同状态之外的条目不受影响");
          expectEqual(foldState(container, EVENT_TERMINAL_TWO), "false", "另一个条目不受影响");

          // 再点同一条 → 回到展开：翻转依据的是**当前生效值**（含刚写下的那条覆盖），而不是条目状态
          // 推出的默认值。判别力：把子组件上抛的 `!expanded` 写成 `expanded`、或省掉第二个参数
          // （页面收到 `undefined`，该条退回默认值），上面那次收起就等不到 `false`；若按默认值翻转，
          // 这一次会停在 `false`。
          click(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TOOL_RUN}"]`),
            "再点进行中的工具调用（回到展开）",
          );
          await waitFor(() => foldState(container, EVENT_TOOL_RUN) === "true", "该条目重新展开");
          expectThat(
            computedStyleOf(
              container,
              `[data-fold-entry="event:${EVENT_TOOL_RUN}"] [data-fold-body]`,
              "进行中工具调用的正文",
            ).display !== "none",
            "再次点选后正文必须重新可见",
          );
          expectEqual(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TOOL_RUN}"]`)
              ?.getAttribute("data-entry-expanded"),
            "true",
            "表头锚点必须与折叠块的有效展开值一致",
          );

          // 反向同样成立：收起同状态的**另一条** → 先前那条的有效值原封不动。若实现把「任一点击」
          // 做成「切换全部」，这里读到 `false` 就会变红。
          click(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TOOL_RUN_TWO}"]`),
            "收起另一条进行中的工具调用",
          );
          await waitFor(() => foldState(container, EVENT_TOOL_RUN_TWO) === "false", "另一条收起");
          expectEqual(foldState(container, EVENT_TOOL_RUN_TWO), "false", "另一条显式收起");
          expectEqual(foldState(container, EVENT_TOOL_RUN), "true", "同为 in_progress 的另一条仍原封不动");

          // 反方向也成立：显式展开覆盖默认收起。
          click(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TERMINAL_TWO}"]`),
            "展开终端输出块",
          );
          await waitFor(() => foldState(container, EVENT_TERMINAL_TWO) === "true", "终端块展开");
          expectThat(
            computedStyleOf(
              container,
              `[data-fold-entry="event:${EVENT_TERMINAL_TWO}"] [data-fold-body]`,
              "终端块正文",
            ).display !== "none",
            "展开后折叠块的正文必须可见",
          );
        },
      );
    },
  },
  {
    name: "CP-5 文件改动在时间线：逐行结构化差异、真实统计与 ACP 原文诊断",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_FILE_CHANGED,
              sessionId: SESSION_ALPHA,
              eventType: "file.changed",
              createdAt: "2026-10-01T00:15:00.000Z",
              view: {
                changeId: CHANGE_ID,
                kind: "update",
                displayPath: "src/auth/session.ts",
                summary: "替换 1 行",
                addedLines: "2",
                deletedLines: "1",
                outsideWorkspace: false,
                diffLines: [
                  { kind: "context", text: DIFF_TEXT[0] },
                  { kind: "deleted", text: DIFF_TEXT[1] },
                  { kind: "added", text: DIFF_TEXT[2] },
                ],
              },
              acp: rawAcp(RAW_DIFF),
            }),
          ],
        }),
        (container) => query(container, '[data-timeline-card="file-changed"] [data-diff-region="lines"]') !== null,
        "时间线文件改动卡片",
        async (mounted) => {
          const container = mounted.container;
          const card = query(container, '[data-timeline-card="file-changed"]');
          expectThat(card !== null, "文件改动必须落在时间线");
          if (card === null) throw new CheckFailure("文件改动卡片缺失");

          // 逐行呈现结构化差异行：退回「只有统计」的形态时，下面这几条同时变红。
          expectEqual(
            query(card, "[data-diff-region]")?.getAttribute("data-diff-region"),
            "lines",
            "时间线必须有结构化差异区",
          );
          expectEqual(queryAll(card, "[data-diff-line-kind]").length, 3, "差异行必须逐行呈现");
          expectEqual(
            queryAll(card, "[data-diff-line-kind]").map((node) => node.getAttribute("data-diff-line-kind")),
            ["context", "deleted", "added"],
            "新增 / 删除 / 上下文三档必须各自成立",
          );
          const cardText = domText(card);
          for (const line of DIFF_TEXT) {
            expectThat(cardText.includes(line), `差异行内容必须逐字来自结构化来源：${line}`);
          }
          // 前缀由客户端按 kind 自动补一次：每行渲染文本必须**恰好**是「前缀 + 原文」。
          const renderedLines = queryAll(card, "[data-diff-line-kind]").map(textOf);
          expectEqual(
            renderedLines,
            DIFF_TEXT.map((text, index) => `${DIFF_PREFIXES[index]}${text}`),
            "差异行必须只带一个由 kind 推出的行首前缀",
          );
          for (const [index, rendered] of renderedLines.entries()) {
            const prefix = DIFF_PREFIXES[index];
            expectThat(prefix !== undefined, `第 ${String(index + 1)} 行没有登记前缀`);
            expectThat(rendered.startsWith(prefix), `第 ${String(index + 1)} 行缺少行首前缀`);
            expectAbsent(rendered, `${prefix}${prefix}`, `第 ${String(index + 1)} 行出现了双前缀`);
          }
          expectThat(
            textOf(query(card, "[data-change-added-lines]")).includes("+2"),
            "真实统计保留",
          );
          expectThat(
            textOf(query(card, "[data-change-deleted-lines]")).includes("-1"),
            "真实统计保留（删除）",
          );
          // 差异行不来自解析 ACP 原文：未展开诊断时，原文片段一个都不出现。
          expectNone(domText(container), ["RAW_ONLY_MARKER"], "未展开诊断时页面上出现了未下发的 ACP 原文");

          // 文件栏没有会话变更清单或联动入口；条目本身仍可展开诊断。
          expectEqual(
            queryAll(container, '[data-conversation-column="files"] [data-session-file]').length,
            0,
            "文件栏不得出现会话文件清单",
          );
          expectEqual(
            query(container, '[data-action="select-file-change"]'),
            null,
            "不得再把条目联动到会话来源",
          );
          click(query(card, '[data-action="open-file-change-diagnostics"]'), "展开文件改动诊断");
          await waitFor(() => query(container, '[data-raw-json="available"]') !== null, "原文诊断展开");
          expectThat(
            textOf(query(container, '[data-raw-json="available"]')).includes("RAW_ONLY_MARKER"),
            "原文作为不可信文本呈现",
          );
          expectEqual(
            queryAll(container, "[data-diff-line-kind]").length,
            3,
            "原文不得被解析成额外差异行",
          );
        },
      );
    },
  },
  {
    name: "CP-6 条目形态：用户气泡靠右、代码卡有语言条与复制、失败条各就各位；进行中不写文案",
    async run() {
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_TURN_RUNNING,
              sessionId: SESSION_ALPHA,
              eventType: "turn.started",
              createdAt: "2026-10-01T00:10:00.000Z",
              view: { turnId: TURN_RUNNING_ID, state: "running" },
            }),
            wireEvent({
              globalSequence: 12,
              eventId: EVENT_TURN_FAILED,
              sessionId: SESSION_ALPHA,
              eventType: "turn.failed",
              createdAt: "2026-10-01T00:16:00.000Z",
              view: {
                turnId: TURN_FAILED_ID,
                state: "failed",
                error: { code: "session.busy", message: "Agent 进程异常退出", retryable: true, details: {} },
              },
            }),
            wireEvent({
              globalSequence: 13,
              eventId: EVENT_TERMINAL_THREE,
              sessionId: SESSION_ALPHA,
              eventType: "terminal.output",
              createdAt: "2026-10-01T00:11:00.000Z",
              view: { terminalId: TERMINAL_TWO_ID, chunkIndex: "0", stream: "stdout", text: "deploy ok", truncated: false },
            }),
          ],
          readResult: (sessionId) =>
            readPage({ sessionId, messages: [conversationMessage(), agentMessage()] }),
        }),
        (container) =>
          query(container, `[data-message-id="${MESSAGE_ID}"]`) !== null &&
          query(container, `[data-fold-entry="event:${EVENT_TERMINAL_THREE}"]`) !== null,
        "消息与折叠块渲染",
        async (mounted) => {
          const container = mounted.container;

          // 用户气泡靠右（整条条目右对齐），Agent 气泡不靠右——两档样式的机器证据取计算样式。
          expectEqual(
            computedStyleOf(container, `[data-message-id="${MESSAGE_ID}"]`, "用户消息条目").alignItems,
            "flex-end",
            "用户消息必须靠右",
          );
          expectThat(
            computedStyleOf(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`, "Agent 消息条目").alignItems !==
              "flex-end",
            "Agent 气泡不得靠右",
          );

          // 进行中的轮次不另写「正在输出」：输出证据是实际的回复/思考/工具条目。
          expectEqual(query(container, '[data-turn-running="true"]'), null, "不再用文案提示输出");
          expectThat(!domText(container).includes("Agent 正在输出"), "不得出现「Agent 正在输出」文案");
          // 失败条：失败轮次的错误在一条明确的条里。
          const failure = query(container, "[data-turn-failure]");
          expectThat(failure !== null, "失败的轮次必须是一条失败条");
          expectThat(domText(failure).includes("轮次失败"), "失败条必须说明这是轮次失败");

          // 代码卡：语言条 + 复制入口，且展开后真的可见（不是 display:none 里的漂亮结构）。
          expectThat(query(container, "[data-code-block]") !== null, "终端输出必须是代码卡");
          expectEqual(
            query(container, "[data-code-language]")?.getAttribute("data-code-language"),
            "stdout",
            "代码卡必须带语言标记",
          );
          expectThat(
            query(container, '[data-code-block] [data-action="copy-code"]') !== null,
            "代码卡必须提供复制入口",
          );
          click(
            query(container, `[data-action="toggle-timeline-entry"][data-entry-key="event:${EVENT_TERMINAL_THREE}"]`),
            "展开终端输出块",
          );
          await waitFor(
            () =>
              computedStyleOf(
                container,
                `[data-fold-entry="event:${EVENT_TERMINAL_THREE}"] [data-fold-body]`,
                "终端块正文",
              ).display !== "none",
            "终端输出块展开",
          );
        },
      );
    },
  },
];

export const CONVERSATION_TIMELINE_CHECKS = checks;
