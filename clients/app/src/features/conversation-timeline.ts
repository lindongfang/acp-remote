/**
 * 对话页时间线：把**消息**与**结构化条目**合成一条按时间排序的序列（`design.md` D1/D2/D6；
 * spec「对话页按时间线呈现消息与结构化条目」「流式回复的易失呈现与权威正文」）。
 *
 * ## 三种条目、两条来源、一个不变量
 *
 * - `message`：正文。**权威只来自 `session.read`**（D6/D8）；易失叠加层只是把流式增量先显示
 *   出来，读取结果一到就以它为准（`volatile` 标记告诉组件这是未持久化的呈现）。
 * - 卡片（`turn` / `tool_call` / `terminal_output` / `plan` / `file_changed` / `mode_changed` /
 *   `config_changed` / `permission_*` / `elicitation_*`）：结构化条目的专用呈现。
 * - `degraded`：覆盖未命中或内容缺失时的降级卡片——**没有第三种结果**。
 *
 * 结构化记录已经归一（`StructuredRecord`，实时与历史同形）且按 `eventId` 去过重，因此
 * 「同一标识在实时与历史两条来源下只呈现一处」在这里是结构性的：同一 `eventId` 在记录阶段
 * 就只剩一条；同一 `messageId` 在读取结果与易失叠加层之间也只保留一处（读取结果优先）。
 *
 * ## 排序
 *
 * 以 `createdAt` 为主键、`sequence`（会话内序号）为次键；两者都相同则保持构造次序
 * （`Array.prototype.sort` 稳定）。消息没有会话内序号，因此同一毫秒里排在结构化条目之前。
 */

import type { MessageView } from "../domain/conversation";
import type { AgentMessageContent, MediaBlockView, PartialResultView } from "../domain/media";
import type { InteractionAvailability, InteractionView } from "../domain/interaction";
import type { FileDiffView, StructuredRecord, TimelineCard } from "../domain/timeline";
import { buildTimelineCard, compareTimelinePosition, groupStructuredRecords, positionOf, recordGroupOf } from "../domain/timeline";
import type { DegradedEventModel, RawAvailability } from "./degradation-model";
import { buildDegradedRecordModel, isDegradedRecord, rawAvailabilityOf } from "./degradation-model";
import { presentationRouteFor } from "./presentation-coverage";

/** 消息通道的三条频道：用户消息、Agent 消息、思考。 */
export type MessageChannel = "user" | "agent" | "thought";

/** 增量事件类型 → 频道。 */
const DELTA_CHANNELS: Readonly<Record<string, MessageChannel>> = {
  "user.message.delta": "user",
  "agent.message.delta": "agent",
  "agent.thought.delta": "thought",
};

/**
 * 易失叠加层的一条消息：按 `messageId` 累加的流式正文。
 *
 * `deltas` 的键是 `deltaIndex`，值是该次增量的正文；迭代次序即到达次序，因此正文是
 * 「按到达顺序拼接」。键去重让**重投的同一条增量不会把文本变成两份**（上游 `eventId` 去重
 * 之外的第二次防线）。`completed` 只表示「本条消息的增量流已被标记结束」，**不**表示
 * 正文已持久化——正文权威始终是 `session.read`（D6）。
 */
export interface VolatileMessageState {
  readonly messageId: string;
  readonly sessionId: string | null;
  readonly channel: MessageChannel;
  readonly turnId: string | null;
  /** 首条增量的时间：它决定易失条目在时间线上的位置。 */
  readonly createdAt: string;
  readonly deltas: ReadonlyMap<string, string>;
  readonly completed: boolean;
  /** 增量里携带的媒体/资源块（按到达顺序）。 */
  readonly mediaBlocks: readonly MediaBlockView[];
  /** 终态的部分结果（`agent.message.completed.partial` 或 turn 失败/停止的标注）。 */
  readonly partial: PartialResultView | null;
}

/** 易失正文：按 `deltaIndex` 数值升序拼接（来源重放可能乱序到达）。 */
export function volatileText(state: VolatileMessageState): string {
  return [...state.deltas.entries()]
    .sort((a, b) => (BigInt(a[0]) < BigInt(b[0]) ? -1 : 1))
    .map(([, text]) => text)
    .join("");
}

/**
 * 读取结果之后**仍然有效**的易失增量（按 `deltaIndex` 数值升序）。
 *
 * 读取结果会清掉「发起读取时已经存在」的增量；此后新到的增量留在叠加层里。时间线把这一段
 * 接在权威正文之后，因此旧的读取结果不会把新内容抹掉（D5 的屏障语义）。没有残留时为 `""`。
 */
export function residualVolatileText(state: VolatileMessageState): string {
  return [...state.deltas.entries()]
    .sort((a, b) => (BigInt(a[0]) < BigInt(b[0]) ? -1 : 1))
    .map(([, text]) => text)
    .join("");
}

/**
 * 把读取屏障之后到达的媒体块并入权威内容（按 `attachmentId` / `uri` 去重）。
 *
 * 读取屏障只清 `deltas`、不清 `mediaBlocks`；读取结果本身也可能已经带出同一附件。因此纯媒体
 * 增量（`text` 为空、只有 `block`）在读取结果之后到达时，只能把**尚未出现过的**块追加进去。
 * 没有稳定标识的块（无附件标识的图片/音频、`unsupported`）无法判断是否重复，按到达顺序保留。
 */
function mergeMediaBlocks(content: AgentMessageContent, blocks: readonly MediaBlockView[]): AgentMessageContent {
  if (blocks.length === 0) return content;
  const seen = new Set<string>();
  for (const block of content.media) {
    switch (block.kind) {
      case "image":
      case "audio":
        if (block.attachmentId !== null) seen.add(`attachment:${block.attachmentId}`);
        break;
      case "file":
        seen.add(`attachment:${block.attachmentId}`);
        break;
      case "resource":
        seen.add(`resource:${block.uri}`);
        break;
      case "unsupported":
        break;
    }
  }
  const added: MediaBlockView[] = [];
  for (const block of blocks) {
    let identity: string | null = null;
    switch (block.kind) {
      case "image":
      case "audio":
        identity = block.attachmentId === null ? null : `attachment:${block.attachmentId}`;
        break;
      case "file":
        identity = `attachment:${block.attachmentId}`;
        break;
      case "resource":
        identity = `resource:${block.uri}`;
        break;
      case "unsupported":
        break;
    }
    if (identity !== null) {
      if (seen.has(identity)) continue;
      seen.add(identity);
    }
    added.push(block);
  }
  if (added.length === 0) return content;
  return { ...content, media: [...content.media, ...added], hasStructuredContent: true };
}

/**
 * 追加一条增量。
 *
 * `state` 为 `undefined` 表示这是该消息的第一条增量。同一 `deltaIndex` 重复到达时保留先到
 * 的那份（重投不改变正文），因此本函数**不**追加重复文本。
 */
export function appendDelta(
  state: VolatileMessageState | undefined,
  input: {
    readonly messageId: string;
    readonly sessionId: string | null;
    readonly channel: MessageChannel;
    readonly turnId: string | null;
    readonly deltaIndex: string;
    readonly text: string;
    readonly createdAt: string;
    readonly mediaBlocks?: readonly MediaBlockView[];
  },
): VolatileMessageState {
  if (state === undefined) {
    return {
      messageId: input.messageId,
      sessionId: input.sessionId,
      channel: input.channel,
      turnId: input.turnId,
      createdAt: input.createdAt,
      deltas: new Map([[input.deltaIndex, input.text]]),
      completed: false,
      mediaBlocks: input.mediaBlocks ?? [],
      partial: null,
    };
  }
  if (state.deltas.has(input.deltaIndex)) return state;
  const deltas = new Map(state.deltas);
  deltas.set(input.deltaIndex, input.text);
  return {
    ...state,
    deltas,
    mediaBlocks:
      input.mediaBlocks === undefined || input.mediaBlocks.length === 0
        ? state.mediaBlocks
        : [...state.mediaBlocks, ...input.mediaBlocks],
  };
}

/** 增量事件类型 → 频道；不是增量事件时返回 `null`。 */
export function deltaChannelOf(eventType: string): MessageChannel | null {
  return DELTA_CHANNELS[eventType] ?? null;
}

/** 时间线条目的公共定位字段。 */
export interface TimelineEntryBase {
  /** 稳定标识：消息为 `message:<messageId>`，其余为记录组键（`event:<eventId>` / `turn:<id>` …）。 */
  readonly key: string;
  readonly createdAt: string;
  readonly sequence: string | null;
}

/** 一条消息条目（`volatile` 为真时它的正文来自易失叠加层）。 */
export interface MessageTimelineEntry extends TimelineEntryBase {
  readonly kind: "message";
  readonly channel: MessageChannel;
  readonly message: MessageView;
  /** 正文来自易失叠加层：尚未被 `session.read` 确认（D6）。 */
  readonly volatile: boolean;
  /** 增量流是否已被 `agent.message.completed` 标记结束（与「已持久化」无关）。 */
  readonly streamComplete: boolean;
  /** 停止/失败/断线时的部分结果标注；完成或未标注为 `null`。 */
  readonly partial: PartialResultView | null;
}

/** 一张结构化卡片条目。 */
export type CardTimelineEntry = TimelineEntryBase & TimelineCard;

/** 一条降级条目（覆盖未命中或内容缺失）。 */
export interface DegradedTimelineEntry extends TimelineEntryBase {
  readonly kind: "degraded";
  readonly degraded: DegradedEventModel;
}

/** 时间线条目：消息、结构化卡片、降级卡片。 */
export type TimelineEntry = MessageTimelineEntry | CardTimelineEntry | DegradedTimelineEntry;

/** 卡片条目（把卡片与它的定位字段合成一条）。 */
function cardEntry(card: TimelineCard, record: StructuredRecord): CardTimelineEntry {
  const position = positionOf(record);
  return { ...card, key: `event:${record.eventId}`, createdAt: position.createdAt, sequence: position.sequence };
}

/** 降级条目。 */
function degradedEntry(record: StructuredRecord, unusableView: boolean): DegradedTimelineEntry {
  return {
    kind: "degraded",
    key: `event:${record.eventId}`,
    createdAt: record.createdAt,
    sequence: record.sessionSequence ?? record.globalSequence,
    degraded: buildDegradedRecordModel(record, { unusableView }),
  };
}

/**
 * 组装时间线。
 *
 * @param messages 权威消息（`session.read` 已合并的页）；它们的 `messageId` 会压制同 id 的易失条目。
 * @param records 已去重的结构化记录（`mergeStructuredRecords` 之后，含实时与历史）。
 * @param volatile 该会话的易失叠加层。
 * @param interactions 该会话的交互视图：授权/提问卡片的可应答性由它提供。
 */
export function buildTimeline(input: {
  readonly messages: readonly MessageView[];
  readonly records: readonly StructuredRecord[];
  readonly volatile: readonly VolatileMessageState[];
  readonly interactions: readonly InteractionView[];
}): readonly TimelineEntry[] {
  const entries: TimelineEntry[] = [];
  const volatileById = new Map(input.volatile.map((tail) => [tail.messageId, tail]));
  const readIds = new Set(input.messages.map((message) => message.id));

  for (const message of input.messages) {
    // 读取结果之后到达的增量：接在权威正文之后，而不是被这条（较早的）读取结果抹掉。
    const tail = volatileById.get(message.id);
    const residual = tail === undefined ? "" : residualVolatileText(tail);
    const mergedText =
      residual.length === 0
        ? message.text
        : // 服务端读取与本地增量可能重叠：读到尾巴里已经包含这一段时不重复拼接。
          message.text.endsWith(residual)
          ? message.text
          : message.text + residual;
    // 媒体块同样跨越读取屏障：纯媒体增量（空文本 + 块）不会产生残留正文，若只按 `residual`
    // 判断就会连同第二条循环一起丢掉。读取结果已带出的同一附件按标识去重。
    const textContent =
      mergedText === message.text ? message.content : { ...message.content, text: mergedText };
    const content = mergeMediaBlocks(textContent, tail?.mediaBlocks ?? []);
    entries.push({
      kind: "message",
      key: `message:${message.id}`,
      createdAt: message.createdAt,
      sequence: null,
      channel: message.role === "user" ? "user" : message.channel === "thought" ? "thought" : "agent",
      message: {
        ...message,
        text: mergedText,
        content,
        hasStructuredContent: message.hasStructuredContent || content.hasStructuredContent,
      },
      volatile: residual.length > 0,
      streamComplete: tail?.completed ?? true,
      partial: tail?.partial ?? message.partial,
    });
  }

  for (const tail of input.volatile) {
    // 读取结果已经给出同 id 的正文：残留增量已在上面并入那条权威消息，不再另起一条。
    if (readIds.has(tail.messageId)) continue;
    entries.push({
      kind: "message",
      key: `message:${tail.messageId}`,
      createdAt: tail.createdAt,
      sequence: null,
      channel: tail.channel,
      message: {
        id: tail.messageId,
        // 思考也是 Agent 产生的：`role` 只表达「谁说的」，`channel` 才区分正文/思考。
        role: tail.channel === "user" ? "user" : "agent",
        channel: tail.channel === "thought" ? "thought" : "reply",
        text: volatileText(tail),
        content: {
          text: volatileText(tail),
          media: tail.mediaBlocks,
          hasStructuredContent: tail.mediaBlocks.length > 0,
        },
        hasStructuredContent: tail.mediaBlocks.length > 0,
        partial: tail.partial,
        createdAt: tail.createdAt,
        turnId: tail.turnId,
      },
      volatile: true,
      streamComplete: tail.completed,
      partial: tail.partial,
    });
  }

  const availability = new Map<string, InteractionAvailability>();
  for (const view of input.interactions) availability.set(view.interactionId, view.availability);
  const availabilityFor = (interactionId: string): InteractionAvailability | null =>
    availability.get(interactionId) ?? null;

  const cardRecords: StructuredRecord[] = [];
  for (const record of input.records) {
    if (isDegradedRecord(record)) {
      entries.push(degradedEntry(record, false));
      continue;
    }
    // 覆盖表说是时间线卡片的才进时间线；其余通道（摘要/覆盖层/命令/消息通道）各有自己的面。
    if (presentationRouteFor(record.eventType) === "timeline_card") cardRecords.push(record);
  }

  for (const group of groupStructuredRecords(cardRecords)) {
    const card = buildTimelineCard(group, availabilityFor);
    if (card === null) {
      // 覆盖命中但视图缺呈现所需字段：客户端对该条事件确实没有可用视图，走降级卡片而不是静默丢。
      entries.push(degradedEntry(group.latest, true));
      continue;
    }
    entries.push(cardEntry(card, group.earliest));
  }

  return entries.sort((a, b) => compareTimelinePosition(a, b));
}

/** 一条改动的诊断入口数据：原文按不可信文本呈现，**不**解析成结构化差异（D8）。 */
export interface FileChangeDiagnostics {
  readonly eventId: string;
  readonly eventType: string;
  readonly createdAt: string;
  readonly sessionSequence: string | null;
  readonly globalSequence: string | null;
  /** 原文可用性：`not_synced`/`absent` 时只能说明原因，MUST NOT 渲染占位原文。 */
  readonly raw: RawAvailability;
  /** 事件视图原样带出（诊断按 JSON 呈现）。 */
  readonly view: Readonly<Record<string, unknown>>;
}

/** 变更文件的呈现模型：结构化统计 + 相对化路径 + 结构化差异区 + 诊断入口的数据（D8/D6）。 */
export interface FileChangeView {
  readonly changeId: string;
  readonly kind: string;
  /** core 已相对化的展示路径；客户端 MUST NOT 自己相对化（它没有路径规范化的知识）。 */
  readonly displayPath: string;
  readonly summary: string;
  /** 判定不出时为 `null`（「省略而不是填零」）——`0` 是断言，`null` 是「没有依据」。 */
  readonly addedLines: string | null;
  readonly deletedLines: string | null;
  readonly outsideWorkspace: boolean;
  /**
   * 差异区：`lines`（随事件下发的结构化差异行）/ `unavailable`（服务端说明了为什么没有）
   * / `null`（两者都没有，即「只有统计」）。三态互斥，见 `domain/timeline.ts` 的 `FileDiffView`。
   */
  readonly diff: FileDiffView | null;
  /** 诊断入口：查看这一处改动的完整 ACP 原文与元数据。 */
  readonly diagnostics: FileChangeDiagnostics;
}

/**
 * 改动清单（右栏）。
 *
 * 只投影 `file.changed` 的**结构化**字段，MUST NOT 解析 ACP 原文去补差异正文——原文只在诊断
 * 入口里作为不可信文本出现。视图缺必填字段（构造不出卡片）时该条不进清单：时间线那边会为它
 * 产出降级卡片，用户不会因此以为改动消失了。
 */
export function buildFileChangeViews(records: readonly StructuredRecord[]): readonly FileChangeView[] {
  const views: FileChangeView[] = [];
  for (const record of records) {
    if (record.eventType !== "file.changed") continue;
    // 卡片构造器是唯一的结构化解析入口：它已经做了逐字段的形状确认。
    const card = buildTimelineCard(recordGroupOf(record), () => null);
    if (card === null || card.kind !== "file_changed") continue;
    views.push({
      changeId: card.changeId,
      kind: card.changeKind,
      displayPath: card.displayPath,
      summary: card.summary,
      addedLines: card.addedLines,
      deletedLines: card.deletedLines,
      outsideWorkspace: card.outsideWorkspace,
      diff: card.diff,
      diagnostics: {
        eventId: record.eventId,
        eventType: record.eventType,
        createdAt: record.createdAt,
        sessionSequence: record.sessionSequence,
        globalSequence: record.globalSequence,
        raw: rawAvailabilityOf(record),
        view: record.view,
      },
    });
  }
  return views;
}

/**
 * 一个**轮次分组**：由用户发送及其引发的条目组成；未归属轮次的条目单独成组。
 *
 * 这里的「归属」只来自可靠标识（消息的 `turnId`、工具/授权/提问卡片的 `turnId`、轮次卡片自身），
 * **不**按呈现顺序猜测。标识相同的连续条目进同一组；组序即它们在时间线上的位置。没有标识的条目
 * （终端输出块、计划、文件改动、降级卡片等）保持独立按时间出现——宁可多一个独立块，也不猜分组。
 */
export interface TimelineTurnGroup {
  readonly key: string;
  readonly turnId: string | null;
  /** 轮次状态（组内最新的轮次卡片）；没有轮次卡片为 `null`。 */
  readonly state: string | null;
  readonly entries: readonly TimelineEntry[];
}

/** 条目所属的轮次标识（只认可靠标识；取不到返回 `null`）。 */
function entryTurnId(entry: TimelineEntry): string | null {
  switch (entry.kind) {
    case "message":
      return entry.message.turnId;
    case "turn":
      return entry.turnId;
    case "tool_call":
      return entry.turnId;
    case "permission_requested":
    case "elicitation_requested":
      return entry.turnId;
    default:
      return null;
  }
}

/**
 * 把已排序的时间线条目切成轮次分组。
 *
 * 纯函数：同一份条目换个次序时不变量仍成立（「同一标识连续出现才同组」），因此组件只负责画，
 * 分组的对错可以在单测里直接断言。
 */
export function groupTimelineEntries(entries: readonly TimelineEntry[]): readonly TimelineTurnGroup[] {
  const groups: TimelineTurnGroup[] = [];
  // 同一 turnId 可能因中间夹着无标识条目（授权结果、计划、文件改动等）而切成**多个**组。
  // 这些组的 `turnId` 合法地相同，但 React 兄弟节点的 key 必须唯一：第一次出现保持
  // `turn:<turnId>`（常见路径不变，前置历史时键也不漂移），再次出现追加出现序号。
  const occurrences = new Map<string, number>();
  for (const entry of entries) {
    const turnId = entryTurnId(entry);
    const last = groups[groups.length - 1];
    if (turnId !== null && last !== undefined && last.turnId === turnId) {
      const state = entry.kind === "turn" ? entry.state : last.state;
      groups[groups.length - 1] = { ...last, entries: [...last.entries, entry], state };
      continue;
    }
    let key: string;
    if (turnId === null) {
      key = `entry:${entry.key}`;
    } else {
      const ordinal = occurrences.get(turnId) ?? 0;
      occurrences.set(turnId, ordinal + 1);
      key = ordinal === 0 ? `turn:${turnId}` : `turn:${turnId}#${String(ordinal)}`;
    }
    groups.push({
      key,
      turnId,
      state: entry.kind === "turn" ? entry.state : null,
      entries: [entry],
    });
  }
  return groups;
}
