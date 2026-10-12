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
  expectEqual,
  expectThat,
  mountScenario,
  navigate,
  query,
  queryAll,
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

/** 会话页默认场景：一条本机自有、进行中的会话 + 两个 scope；事件与读回应答由各用例给出。 */
export function sessionScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES,
    sessions: [sessionSummary({ id: SESSION_ALPHA, alias: "alpha", state: "running" })],
    agents: AGENTS,
    scopes: ["session.list", "session.read"],
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
          () => query(first.container, '[data-directory-alias="alpha"]') !== null,
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
];

export const CONVERSATION_TIMELINE_CHECKS = checks;
