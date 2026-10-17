/**
 * 「解码边界受理每一条协议合法帧」与「事件来源类型的五值词表与 node 配对规则」的浏览器级验证
 * （`openspec/changes/fix-client-wire-frame-acceptance/specs/pwa-web-client/spec.md`，design D3）。
 *
 * 假主机在**真实连接**上投递一条 `origin.kind = "node"`（含 `nodeId`、`deviceId` 为 null）的
 * 协议合法事件，断言三件消费者可见的后果：页面呈现它、假主机收到游标推进到它的 `sync.ack`、
 * 该事件之后的下一条事件同样被呈现。
 *
 * ## 判别力来源（这条检查必须能抓到的实现错误）
 *
 * 把 `clients/app/src/sync-client/wire.ts` 的运行期词表 `EVENT_ORIGIN_KINDS` 改回四值时，
 * 本检查**必然变红**：
 *
 * 1. 该帧在解码边界被判 `field_type`（`decodeEvent` 的「`event.origin.kind` 不在封闭词表内」），
 *    于是 `SyncConnection.#onEventMessage` 根本走不到 `ledger.commit`，`ClientStore` 的结构化
 *    记录里没有这条事件 → 时间线上不会出现它的卡片；
 * 2. ACK **只**在账本提交后发出（`sync-client/connection.ts` 的 `#sendAck`），因此**不存在**
 *    游标恰为该事件序号的那条 `sync.ack`；而账本不检查序号连续性（`sync-client/dedupe.ts`
 *    的 `evaluate` 只与水位比较），下一条事件照样提交——最深的一条 ACK 会从屏障 10 直接跳到 12。
 *    所以第 2 条断言锚在**恰为该序号**的游标上：写成「存在一条 ≥ 11 的 ACK」会被跳号后的 12
 *    满足，那会是一条对本次缺陷永远为真的断言。
 *
 * 配对规则的**拒收**面（`nodeId` 键缺席 / `nodeId` 为空 / `deviceId` 非 null）不在本层：
 * 那里的被拒帧不会产生任何可观察的呈现与 ACK，本层的假主机也拿不到「必须是非法帧」的断言对象。
 * 它由 `src/sync-client/wire.test.ts`、连接级用例与固定样例门禁
 * （`src/sync-client/wire-fixtures.test.ts`）覆盖；本层只覆盖受理面，不重复。
 *
 * ## 为什么三件事只用一条用例
 *
 * 三者是同一次提交的下游结果、挂在同一份场景上。拆开要多挂两次真实连接，而「后续事件照常呈现」
 * 单独拿出来对本缺陷永远为真（见上：账本不做连续性检查）——多出来的那条不是证据，只是噪声。
 *
 * ## 自带场景
 *
 * 本检查集自己挂场景（已配对起点 + 真实握手 + 真实路由 + 真实快照管线）、自带容器，并在
 * `finally` 里卸载：它不写「未配对」记录、不清本地缓存、不占用 `mountRootLayout` 的唯一一次
 * 挂载，也不与任何别的检查集共用容器或状态。
 *
 * ## 呈现面为什么是时间线卡片而不是会话摘要行
 *
 * 事件在快照屏障之后紧接着投递（`fake-host.ts` 的 `#sendSnapshotEnd` 把 `events` 排在
 * `sync.snapshot_end` 之后、`sync.caught_up` 之前），而事件是逐帧 `queueMicrotask` 交付的
 * （同文件 `#deliver`），快照的提交却要等两级 SHA-256 摘要校验
 * （`sync-client/snapshot.ts` 的 `SnapshotStaging.complete`）——事件因此**先于**
 * `snapshot_verified` 被归并。于是「必须写进快照资源才算呈现」的类型（例如 `session.created`
 * 的会话摘要，`features/client-store.ts` 的 `withSessionSummary` 在 `resources` 为 null 时
 * 无处可写）在这一刻会落空，用它会变成一个由调度决定的用例。时间线卡片由**结构化记录**
 * （`ClientStore.structured`）派生，不受这条时序影响。
 *
 * 卡片上的 `toolCallId` 只来自这条事件的视图：快照里没有结构化记录，而本场景的 `session.read`
 * 返回空页（既无 messages 也无 history），因此「卡片在场」等价于「这条事件走完了
 * 解码 → 账本 → 归并 → 渲染」，不是「页面碰巧画了点什么」。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  SESSION_ALPHA,
  WORKSPACES,
  emptyReadResult,
  sessionRoute,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import type { FakeHost } from "../fake-host";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  waitFor,
} from "../support";
import type { Check, Mounted, ScenarioOptions } from "../support";

/** 屏障之后的第一个序号（`baseScenario` 的 `barrierGlobalSequence` 是 10，序号必须严格大于它）。 */
const NODE_EVENT_SEQUENCE = 11;
/** 紧随其后的第二个序号：用来证明游标没有卡在第一条上。 */
const FOLLOW_UP_EVENT_SEQUENCE = 12;

/** ACK 游标里的序号文本（`Cursor.globalSequence` 是 decimalString，比较按文本）。 */
const NODE_EVENT_CURSOR = String(NODE_EVENT_SEQUENCE);
const FOLLOW_UP_EVENT_CURSOR = String(FOLLOW_UP_EVENT_SEQUENCE);

/** 两条事件的标识（UUID，与夹具里的任何标识都不相同）。 */
const NODE_EVENT_ID = "f0000000-0000-4000-8000-000000000001";
const FOLLOW_UP_EVENT_ID = "f0000000-0000-4000-8000-000000000002";

/** `fixtures/sync/v1/valid/event-node-origin.json` 里的 `origin.nodeId`（合法 UUID，逐字取用）。 */
const NODE_ID = "9c1e0a58-3b71-4c52-8f2a-6d5e7f8091a2";

/** 两张卡片各自的 `toolCallId`：只出现在对应事件的视图里，别处（快照、读取结果）都没有它们。 */
const NODE_TOOL_CALL_ID = "f0000000-0000-4000-8000-000000000011";
const FOLLOW_UP_TOOL_CALL_ID = "f0000000-0000-4000-8000-000000000012";

/** 所属轮次（`tool.call.*` 的可选分组字段）。 */
const TURN_ID = "f0000000-0000-4000-8000-000000000021";

/**
 * 工具调用的视图字段。
 *
 * `domain/timeline.ts` 的 `buildTimelineCard` 对 `tool_call` 要求 `toolCallId` / `title` / `state`
 * 三者都能取到非空字符串，否则整组记录不产出卡片——这里逐条给全。
 */
function toolCallView(toolCallId: string, title: string, state: string): Record<string, unknown> {
  return { toolCallId, turnId: TURN_ID, title, state };
}

/**
 * 一条 `origin.kind = "node"` 的**协议合法**事件。
 *
 * 形状照 `fixtures/sync/v1/valid/event-node-origin.json`：`kind` 为 `node`、`nodeId` 是合法 UUID、
 * `deviceId` 为 null。
 */
function nodeOriginEvent(): Record<string, unknown> {
  return wireEvent({
    globalSequence: NODE_EVENT_SEQUENCE,
    eventId: NODE_EVENT_ID,
    sessionId: SESSION_ALPHA,
    eventType: "tool.call.started",
    createdAt: "2026-10-01T00:20:00.000Z",
    view: toolCallView(NODE_TOOL_CALL_ID, "由另一个节点下令的调用", "running"),
    origin: { kind: "node", deviceId: null, nodeId: NODE_ID },
  });
}

/** 该事件之后的下一条事件：来源取 `wireEvent` 的缺省（`agent`），只用来证明游标没有卡住。 */
function followUpEvent(): Record<string, unknown> {
  return wireEvent({
    globalSequence: FOLLOW_UP_EVENT_SEQUENCE,
    eventId: FOLLOW_UP_EVENT_ID,
    sessionId: SESSION_ALPHA,
    eventType: "tool.call.started",
    createdAt: "2026-10-01T00:21:00.000Z",
    view: toolCallView(FOLLOW_UP_TOOL_CALL_ID, "节点事件之后的下一条", "completed"),
  });
}

/** 某条事件的工具调用卡片锚点（`ConversationTimeline` 的既有 `data-*` 口径）。 */
function toolCallCard(toolCallId: string): string {
  return `[data-timeline-card="tool-call"][data-tool-call-id="${toolCallId}"]`;
}

/** 假主机收到的 `sync.ack` 游标序号，按收到次序（`Cursor.globalSequence` 是 decimalString）。 */
function ackedSequences(host: FakeHost | null): readonly string[] {
  if (host === null) throw new CheckFailure("场景没有注入假主机：读不到客户端发出的帧");
  const sequences: string[] = [];
  for (const message of host.received) {
    if (message["type"] !== "sync.ack") continue;
    const body = message["body"];
    if (typeof body !== "object" || body === null) continue;
    const cursor = (body as Record<string, unknown>)["cursor"];
    if (typeof cursor !== "object" || cursor === null) continue;
    const sequence = (cursor as Record<string, unknown>)["globalSequence"];
    if (typeof sequence === "string") sequences.push(sequence);
  }
  return sequences;
}

/**
 * 场景：一条本机自有、运行中的会话 + 上面两条事件。
 *
 * `session.read` 返回**空页**：会话页的时间线上因此只可能有两张卡片，且它们只可能来自实时事件
 * （实时事件经真实解码与真实账本，读取结果与快照都不提供结构化记录）。
 */
function baseScenario(): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES,
    sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" })],
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [nodeOriginEvent(), followUpEvent()],
    readResult: (sessionId) => emptyReadResult(sessionId),
    route: sessionRoute(SESSION_ALPHA),
  };
}

/**
 * 挂场景并把**就绪**的页面交给用例（自带容器；`finally` 卸载，`dispose()` 后的组合根不可复活）。
 *
 * 就绪判据用**下一条事件**的卡片：它不经过本轮要验的 node 受理面，因此拿它当门槛之后，
 * 「node 事件没被受理」会以一条具体断言的失败暴露出来，而不是让整条用例冒出一个看起来像
 * 「页面压根没起来」的等待超时。
 */
async function withFrameAcceptancePage(body: (mounted: Mounted) => Promise<void>): Promise<void> {
  const mounted = await mountScenario(baseScenario());
  try {
    await waitFor(
      () => query(mounted.container, toolCallCard(FOLLOW_UP_TOOL_CALL_ID)) !== null,
      "下一条事件的工具调用卡片（页面就绪的判据）",
    );
    await body(mounted);
  } finally {
    await mounted.unmount();
  }
}

const checks: readonly Check[] = [
  {
    name: "FA-1 origin.kind = \"node\" 的协议合法事件被受理：页面呈现它、游标推进到它之后、后续事件照常呈现",
    async run() {
      await withFrameAcceptancePage(async (mounted) => {
        const container = mounted.container;

        // 1) 呈现：卡片挂在**该事件视图独有**的 `toolCallId` 上（快照与读取结果里都没有这个值），
        //    因此「卡片在场」只可能是这条事件走完了解码 → 账本 → 归并 → 渲染。
        const nodeCard = query(container, toolCallCard(NODE_TOOL_CALL_ID));
        expectThat(
          nodeCard !== null,
          `origin.kind = "node" 的合法事件必须被呈现：时间线上应出现 ${toolCallCard(NODE_TOOL_CALL_ID)}`,
        );
        // 视图字段真的被搬进了 DOM（不是「有张卡片」就算数）：`state` 逐字来自该事件的
        // `payload.view.state`（`buildTimelineCard` → `data-tool-state`）。
        expectEqual(
          nodeCard?.getAttribute("data-tool-state"),
          "running",
          "node 来源事件卡片的 state 必须来自该事件的视图",
        );

        // 2) 游标推进到该事件之后。锚在**恰为该序号**的游标上：ACK 只在账本提交后发出，所以
        //    「存在游标 11 的 ACK」就是「这条事件真的进了账本」；写成 ≥ 11 会被跳号后的 12 满足。
        const acks = ackedSequences(mounted.host);
        expectThat(
          acks.includes(NODE_EVENT_CURSOR),
          `假主机必须收到游标恰为 ${NODE_EVENT_CURSOR} 的 sync.ack（该事件被提交进账本的证据）；实际收到 ${JSON.stringify(acks)}`,
        );

        // 3) 游标没有卡住：该事件之后的下一条事件同样被呈现、同样被 ACK。
        expectThat(
          query(container, toolCallCard(FOLLOW_UP_TOOL_CALL_ID)) !== null,
          "该事件之后的下一条事件同样必须被呈现（游标不得卡在 node 来源的事件上）",
        );
        expectThat(
          acks.includes(FOLLOW_UP_EVENT_CURSOR),
          `后续事件的 ACK 同样必须发出；实际收到 ${JSON.stringify(acks)}`,
        );
      });
    },
  },
];

export const FRAME_ACCEPTANCE_CHECKS = checks;
