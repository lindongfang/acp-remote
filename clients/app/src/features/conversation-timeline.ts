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
 * 所有已知位置按 Owner 固定的同域序号排序，与时间戳无关；旧历史缺少序号时独立前置。
 * 正式条目（含从本地回显接替而来、带 `localAnchor` 的正文）一律走这条规则，不得按 `turnId`
 * 或时间前置——那会改写两条权威记录的相对顺序。只有**尚无权威位置**的本地回显单独作临时呈现，
 * 插在对应轮次的可见输出之前或末尾，不参与权威相对顺序。
 * 相邻连续消息片段由后端分配标识，本层不跨工具或不同消息重新合并。
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
  readonly sequence?: string;
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
    readonly sequence?: string;
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
      ...(input.sequence === undefined ? {} : { sequence: input.sequence }),
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
    ...(input.sequence !== undefined && (state.sequence === undefined || BigInt(input.sequence) < BigInt(state.sequence))
      ? { sequence: input.sequence, createdAt: input.createdAt } : {}),
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

/**
 * 本设备这次提交在时间线上的本地阶段（尚未被权威用户消息接替时的实情）。
 *
 * - `uploading` / `sealing`：附件还在上传/封存，**Agent 尚未收到任何内容**；
 * - `submitted`：命令已发出，服务端尚未确认受理；
 * - `accepted`：服务端已受理（已知 `turnId`），等待 Agent 输出；
 * - `uncertain`：受理结果不确定（可能已保存），不自动重发；
 * - `failed`：本次提交未成立（上传失败/取消或受理前失败），输入仍可修正。
 */
export type SentPromptPhase = "uploading" | "sealing" | "submitted" | "accepted" | "uncertain" | "failed";

/** 本地条目的阶段与说明（组件据此在用户气泡下补一行实情）。 */
export interface LocalPromptState {
  readonly requestId: string;
  readonly phase: SentPromptPhase;
  readonly notice: string | null;
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
  /**
   * 这条用户消息只存在于本设备（还没有权威正文接替它）时的本地阶段；权威消息为 `null`。
   * 组件只按它说明「上传中 / 已提交 / 未发送」，不得据此伪造已受理。
   */
  readonly local?: LocalPromptState | null;
  /**
   * 本设备这次提交的**稳定锚点**（`requestId`）。
   *
   * 本地回显与它的权威接替（同一 `messageId`/`turnId` 已确认）共享同一个锚点，因此 `key` 一直是
   * `sent:<requestId>`；接替只改属性，React 原位更新该节点而不是把它移除后另建一条（D2）。
   *
   * 锚点**只**决定身份与键，**不**参与排序：取得权威位置后这条正文与其它正式条目一样按
   * `compareTimelinePosition` 落位（不得为维持分组把它前置到整轮之前）。不是本设备提交的消息
   * 为 `null`。
   *
   * 该字段只描述身份来源，不参与条目键的计算（条目键只看 `key`），因此历史模型可以不写它。
   */
  readonly localAnchor?: string | null;
}

/** 一张结构化卡片条目。 */
export type CardTimelineEntry = TimelineEntryBase & TimelineCard;

/** 一条降级条目（覆盖未命中或内容缺失）。 */
export interface DegradedTimelineEntry extends TimelineEntryBase {
  readonly kind: "degraded";
  readonly degraded: DegradedEventModel;
}

/** 时间线条目：消息、结构化卡片、降级卡片。 */
export type TimelineEntry = MessageTimelineEntry | CardTimelineEntry | DegradedTimelineEntry | PendingInteractionTimelineEntry;

/** 缺少请求事件的读回交互也参与时间排序，不单独置顶。 */
export interface PendingInteractionTimelineEntry extends TimelineEntryBase {
  readonly kind: "pending_interaction";
  readonly interaction: InteractionView;
}

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
    sequence: record.sessionSequence,
    degraded: buildDegradedRecordModel(record, { unusableView }),
  };
}

/**
 * 本设备刚提交、尚未被权威读取/回显接替的用户消息（视图层只读）。
 *
 * 身份来自可靠关联：`messageId`（首条的准备关联或读取确认的首条正文）与 `turnId`
 * （`session.prompt` 的 `accepted { turnId }`）。**没有**按文字或时间推断重复的字段——
 * 同一段文字发送两次是两条独立输入，各自保留。
 */
export interface SentPromptView {
  readonly requestId: string;
  readonly text: string;
  readonly attachmentNames: readonly string[];
  readonly createdAt: string;
  /** 本地阶段（上传中/封存中/待确认/已受理/不确定/未成立）。 */
  readonly phase: SentPromptPhase;
  /** 服务端受理后给出的轮次标识；尚未受理为 `null`。 */
  readonly turnId: string | null;
  /** 已确认的正式用户消息标识（首条准备关联或首条权威读取）；未知为 `null`。 */
  readonly messageId: string | null;
  /** 本地实情说明（上传失败原因、取消说明等）；没有为 `null`。 */
  readonly notice: string | null;
}

/**
 * 组装时间线。
 *
 * @param messages 权威消息（`session.read` 已合并的页）；它们的 `messageId` 会压制同 id 的易失条目。
 * @param records 已去重的结构化记录（`mergeStructuredRecords` 之后，含实时与历史）。
 * @param volatile 该会话的易失叠加层。
 * @param interactions 该会话的交互视图：授权/提问卡片的可应答性由它提供。
 * @param sentPrompts 本设备已提交、尚未被权威用户消息接替的输入；只按 `messageId`/`turnId`
 *   的同身份接替，**不**按相同文字抵消。
 * @param promptAnchors 已确认的接替关联（`messageId → requestId`）：本设备这次提交的权威消息
 *   沿用本地条目的稳定键与分组，节点原位更新。关联只来自可靠标识，绝不按文字或时间推断。
 */
export function buildTimeline(input: {
  readonly messages: readonly MessageView[];
  readonly records: readonly StructuredRecord[];
  readonly volatile: readonly VolatileMessageState[];
  readonly interactions: readonly InteractionView[];
  readonly sentPrompts?: readonly SentPromptView[];
  /** 该会话已确认的接替关联；没有时显式给 `undefined`（调用点直接取会话分桶）。 */
  readonly promptAnchors?: ReadonlyMap<string, string> | undefined;
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
    // 已确认的本地锚点（`messageId → requestId`）：这条正式用户消息就是本设备那次提交的接替，
    // 因此沿用本地条目的稳定键（父级始终是同一个平级列表），节点原位更新而不是被移除重建。
    const localAnchor = message.role === "user" ? (input.promptAnchors?.get(message.id) ?? null) : null;
    entries.push({
      kind: "message",
      key: localAnchor === null ? `message:${message.id}` : `sent:${localAnchor}`,
      createdAt: message.createdAt,
      sequence: message.sequence ?? null,
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
      local: null,
      localAnchor,
    });
  }

  for (const tail of input.volatile) {
    // 读取结果已经给出同 id 的正文：残留增量已在上面并入那条权威消息，不再另起一条。
    if (readIds.has(tail.messageId)) continue;
    const localAnchor = tail.channel === "user" ? (input.promptAnchors?.get(tail.messageId) ?? null) : null;
    entries.push({
      kind: "message",
      key: localAnchor === null ? `message:${tail.messageId}` : `sent:${localAnchor}`,
      createdAt: tail.createdAt,
      sequence: tail.sequence ?? null,
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
      local: null,
      localAnchor,
    });
  }

  // 本设备刚提交的用户消息：Agent 不回显时也要立刻可见。**只有**权威用户消息或用户增量事件
  // 带着同一 `messageId`/`turnId` 到位时，本地条目才让位——相同文字、相近时间都不是身份。
  if (input.sentPrompts !== undefined && input.sentPrompts.length > 0) {
    const claimedMessageIds = new Set<string>();
    const claimedTurnIds = new Set<string>();
    for (const message of input.messages) {
      if (message.role !== "user") continue;
      claimedMessageIds.add(message.id);
      if (message.turnId !== null) claimedTurnIds.add(message.turnId);
    }
    for (const tail of input.volatile) {
      if (tail.channel !== "user") continue;
      claimedMessageIds.add(tail.messageId);
      if (tail.turnId !== null) claimedTurnIds.add(tail.turnId);
    }
    const ordered = [...input.sentPrompts].sort((left, right) =>
      left.createdAt < right.createdAt ? -1 : left.createdAt > right.createdAt ? 1 : 0);
    for (const prompt of ordered) {
      const claimed =
        (prompt.messageId !== null && claimedMessageIds.has(prompt.messageId)) ||
        (prompt.turnId !== null && claimedTurnIds.has(prompt.turnId));
      if (claimed) continue;
      const text = [prompt.text, ...prompt.attachmentNames.map((name) => `📎 ${name}`)]
        .filter((part) => part.length > 0)
        .join("\n");
      entries.push({
        kind: "message",
        // 稳定内容身份（D2）：本地回显与它的权威接替共用 `sent:<requestId>`，因此正式消息到达时
        // React 复用同一个节点（属性更新，不卸载再挂载）。
        key: `sent:${prompt.requestId}`,
        createdAt: prompt.createdAt,
        sequence: null,
        channel: "user",
        message: {
          id: prompt.requestId,
          role: "user",
          channel: "reply",
          text,
          content: { text, media: [], hasStructuredContent: false },
          hasStructuredContent: false,
          partial: null,
          createdAt: prompt.createdAt,
          turnId: prompt.turnId,
        },
        volatile: false,
        streamComplete: true,
        partial: null,
        local: { requestId: prompt.requestId, phase: prompt.phase, notice: prompt.notice },
        localAnchor: prompt.requestId,
      });
    }
  }

  const availability = new Map<string, InteractionAvailability>();
  for (const view of input.interactions) availability.set(view.interactionId, view.availability);
  const availabilityFor = (interactionId: string): InteractionAvailability | null =>
    availability.get(interactionId) ?? null;

  const cardRecords: StructuredRecord[] = [];
  for (const record of input.records) {
    // 会话管理通知由状态层接收，不进入聊天区（包括原先的诊断兜底卡）。
    if (record.eventType === "session.mode.changed" ||
        record.eventType === "session.config.changed" ||
        record.eventType === "session.info.changed" ||
        record.eventType === "session.commands.changed") continue;
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

  // 结果已更新到原请求卡片；仅当请求不在当前历史窗口中时保留独立结果。
  const resolvedPermissions = new Set(entries.flatMap(entry =>
    entry.kind === "permission_requested" &&
      (entry.availability.kind === "handled_here" || entry.availability.kind === "handled_elsewhere")
      ? [entry.interactionId] : []));
  for (let index = entries.length - 1; index >= 0; index--) {
    const entry = entries[index];
    if (entry?.kind === "permission_resolved" && resolvedPermissions.has(entry.interactionId)) {
      entries.splice(index, 1);
    }
  }

  const shownInteractions = new Set(entries.flatMap(entry =>
    entry.kind === "permission_requested" || entry.kind === "elicitation_requested" ? [entry.interactionId] : []));
  for (const interaction of input.interactions) {
    if (shownInteractions.has(interaction.interactionId)) continue;
    entries.push({ kind: "pending_interaction", key: `pending:${interaction.interactionId}`,
      createdAt: interaction.requestedAt, sequence: interaction.sequence ?? null, interaction });
  }
  // 权威顺序只由 `compareTimelinePosition` 决定（ADR 0015、`docs/FRONTEND_DESIGN.md`
  // 「Agent 输出顺序」）：**所有**正式消息——包括从本设备本地回显接替而来、带 `localAnchor` 的
  // 那条——与卡片、交互条目一样按 Owner 序号排序。不得按 `turnId` 或创建时间把正式消息前置：
  // 那会改写两条权威记录之间的相对顺序（思考序号 10、用户消息 11、回复 12 必须仍是 10/11/12，
  // 无论那条用户消息有没有 `turnId`）。
  //
  // 只有**尚无权威位置**的本地回显单独占一个临时位置：它还不是任何一条权威记录（`sequence`
  // 为 `null`），所以不参与上面的相对顺序；插进有序序列也不会改变任何两条权威记录的相对次序。
  // 取得可靠轮次关联（`accepted { turnId }`）时放在该轮可见输出之前；还没有关联时按创建时刻
  // 回到它进入时间线的位置（第一条不早于它的条目前，否则末尾），不猜 `turnId`。被正式消息
  // 接替后这条临时条目消失，同一条正文回到权威序号。
  const ordered = entries.filter((entry) => !isTemporaryLocalEcho(entry)).sort(compareTimelinePosition);
  const temporary = entries.filter(isTemporaryLocalEcho);
  // 同一轮的多次提交按发起顺序落位：先按创建时间排队（`sort` 稳定，同一时刻保持原序）。
  temporary.sort((left, right) =>
    left.createdAt < right.createdAt ? -1 : left.createdAt > right.createdAt ? 1 : 0);
  for (const entry of temporary) {
    const turnId = entryTurnId(entry);
    const related = turnId === null ? -1 : ordered.findIndex(candidate => entryTurnId(candidate) === turnId);
    if (related < 0) {
      // 尚无可靠轮次关联（或该轮还没有任何条目）：回到**只决定临时占位**的创建时间回退——
      // 插在第一条创建时刻不早于它的条目前，否则留在末尾。没有 `turnId` 的流式回复因此仍在
      // 这条输入之后（过程模型按条目顺序判断「等待响应」，回复排到用户之前会被误判成还没回复）。
      // 候选可以包含正式条目：临时条目插进有序序列不会改变任何两条正式记录的相对次序。
      const provisional = ordered.findIndex(candidate =>
        Date.parse(candidate.createdAt) >= Date.parse(entry.createdAt));
      ordered.splice(provisional < 0 ? ordered.length : provisional, 0, entry);
      continue;
    }
    // 插在该轮第一条条目之前；同轮已落位的临时回显之后再插，保持发起顺序。
    let index = related;
    while (index < ordered.length) {
      const current = ordered[index];
      if (current === undefined || !isTemporaryLocalEcho(current) || entryTurnId(current) !== turnId) break;
      index += 1;
    }
    ordered.splice(index, 0, entry);
  }
  return ordered;
}

/**
 * 尚未取得权威位置的本地回显。
 *
 * `local` 非空说明它只是本设备的临时呈现，还没有正式消息接替它；`sequence` 为 `null` 说明它
 * 没有 Owner 序号。两者同时成立才允许临时落位。**正式**消息（含从本地回显接替而来的那条，
 * `local` 为 `null`）永远按权威序号排序，即使它带着 `localAnchor`。
 */
function isTemporaryLocalEcho(entry: TimelineEntry): boolean {
  return entry.kind === "message" && entry.local != null && entry.sequence === null;
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
 * 一个**轮次分组**：同一可靠轮次标识的连续条目。
 *
 * 这里的「归属」只来自可靠标识（消息的 `turnId`、工具/授权/提问卡片的 `turnId`、轮次卡片自身），
 * **不**按呈现顺序猜测。标识相同的连续条目进同一组；组序即它们在时间线上的位置。没有标识的条目
 * （终端输出块、计划、文件改动、降级卡片等）保持独立按时间出现——宁可多一个独立块，也不猜分组。
 * 本设备提交的正文（`localAnchor`）与其它条目一视同仁地按 `turnId` 归组；它是**呈现资料**，
 * 不是 DOM 父级（视图里条目平级渲染），因此组键怎么变都不会重挂正文。
 */
export interface TimelineTurnGroup {
  /**
   * 组键：只由轮次标识或条目自身的稳定身份决定，与组内成员、组的位置无关
   * （规则见 `groupTimelineEntries`）。同一份时间线里唯一，仅用于呈现资料。
   */
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
    case "pending_interaction":
      return entry.interaction.turnId;
    default:
      return null;
  }
}

/**
 * 把已排序的时间线条目切成**逻辑**轮次分组。
 *
 * 纯函数：同一份条目换个次序时该不变量仍成立（「同一标识连续出现才同组」），因此组件只负责画，
 * 分组的对错可以在单测里直接断言。
 *
 * ## 分组只是呈现资料，不是 DOM 父级（D2）
 *
 * 条目在视图里**平级**渲染：每个条目以自己的稳定键（`entry.key`）作为列表项键，消息条目在任何
 * 时候都不换父容器。分组键、组内计数与轮次状态只作为每条的呈现属性交给视图，因此「一条正式
 * 消息落进哪一段轮次」怎么变，都不会让已有正文卸载重挂。
 *
 * 组键来源：
 *
 * - 有轮次标识的条目按 `turn:<turnId>` 成组；同一轮被无标识条目（授权结果、计划、文件改动等）
 *   切成多段时按出现次序编号（`turn:<turnId>#2`…），组键因此仍唯一。
 * - 没有标识的条目自成一组，键是它自己的条目键（`group:<key>`）。
 *
 * 本设备提交的正文（本地回显与它的权威接替）不再独占组：它按自己的 `turnId` 与同轮条目连续
 * 成组，与其它条目一视同仁。它**不**为了「同组」或「在输出之前」被移动——顺序只来自
 * `buildTimeline` 的权威排序。
 *
 * 同一 `turnId` 的连续条目进同一组；中间夹着无标识条目时切成多个组——宁可多一个独立块，也不
 * 猜分组，更不为了「同组」而移动条目改变顺序。
 */
export function groupTimelineEntries(entries: readonly TimelineEntry[]): readonly TimelineTurnGroup[] {
  const groups: TimelineTurnGroup[] = [];
  /** 同一 `turnId` 已切出的组数：编号只用于让多段组的键唯一，不影响哪些条目同组。 */
  const turnOrdinals = new Map<string, number>();
  for (const entry of entries) {
    const turnId = entryTurnId(entry);
    const last = groups[groups.length - 1];
    if (turnId !== null && last !== undefined && last.turnId === turnId) {
      const state = entry.kind === "turn" ? entry.state : last.state;
      groups[groups.length - 1] = { ...last, entries: [...last.entries, entry], state };
      continue;
    }
    let key: string;
    if (turnId !== null) {
      const ordinal = (turnOrdinals.get(turnId) ?? 0) + 1;
      turnOrdinals.set(turnId, ordinal);
      key = ordinal === 1 ? `turn:${turnId}` : `turn:${turnId}#${ordinal}`;
    } else {
      key = `group:${entry.key}`;
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

/** 两个时间线条目值是否结构相同（含嵌套的正文、状态、媒体与诊断数据）。 */
function sameTimelineValue(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (typeof left !== typeof right || typeof left !== "object" || left === null || right === null) return false;
  // 内建类型没有自有字段，逐字段比较会把它们判成「空且相等」（两个不同时刻的 `Date`、
  // 两份不同内容的 `Map` 会因此被当成没变化）。这里逐个显式比较；未列出的类型走自有字段比较，
  // 并由原型一致性拦住「有内容的实例 vs 空对象」这类误判。
  if (left instanceof Date || right instanceof Date) {
    return left instanceof Date && right instanceof Date && left.getTime() === right.getTime();
  }
  if (left instanceof Map || right instanceof Map) {
    if (!(left instanceof Map) || !(right instanceof Map) || left.size !== right.size) return false;
    // 键按标识比较（与 `Map` 自身的查找语义一致），值递归比较。
    for (const [key, value] of left) {
      if (!right.has(key) || !sameTimelineValue(value, right.get(key))) return false;
    }
    return true;
  }
  if (left instanceof Set || right instanceof Set) {
    if (!(left instanceof Set) || !(right instanceof Set) || left.size !== right.size) return false;
    for (const value of left) if (!right.has(value)) return false;
    return true;
  }
  if (left instanceof ArrayBuffer || right instanceof ArrayBuffer) {
    if (!(left instanceof ArrayBuffer) || !(right instanceof ArrayBuffer)) return false;
    if (left.byteLength !== right.byteLength) return false;
    const beforeBytes = new Uint8Array(left);
    const afterBytes = new Uint8Array(right);
    for (let index = 0; index < beforeBytes.length; index += 1) {
      if (beforeBytes[index] !== afterBytes[index]) return false;
    }
    return true;
  }
  if (Object.getPrototypeOf(left) !== Object.getPrototypeOf(right)) return false;
  const leftIsArray = Array.isArray(left);
  if (leftIsArray !== Array.isArray(right)) return false;
  if (leftIsArray) {
    const before = left as readonly unknown[];
    const after = right as readonly unknown[];
    if (before.length !== after.length) return false;
    for (let index = 0; index < before.length; index += 1) {
      if (!sameTimelineValue(before[index], after[index])) return false;
    }
    return true;
  }
  const before = left as Record<string, unknown>;
  const after = right as Record<string, unknown>;
  const keys = Object.keys(before);
  if (keys.length !== Object.keys(after).length) return false;
  for (const key of keys) {
    if (!Object.prototype.hasOwnProperty.call(after, key)) return false;
    if (!sameTimelineValue(before[key], after[key])) return false;
  }
  return true;
}

/**
 * 复用**内容没变**的条目对象（`design.md` D1「语义稳定的条目模型」）。
 *
 * `buildTimeline` 每次都会重建全部条目；如果不复用，`memo` 化的条目视图会被新对象全部击穿，
 * 一条流式增量就会让所有历史正文重新渲染。这里按键与顺序逐条比对：键或顺序一变直接返回新
 * 数组；只有内容（正文、状态、媒体、诊断等**全部字段**）都没变时才返回上一次的对象。
 *
 * 比较是**结构化**的：对象按自有字段递归比较（不是手挑字段的白名单，因此新增的可见字段不会
 * 被漏比），数组按元素递归比较，`Date`/`Map`/`Set`/`ArrayBuffer` 这类没有自有字段的内建值显式
 * 比较内容——否则它们会被判成「空且相等」，内容真变了却被复用成旧对象。未列入的内建类型由原型
 * 一致性拦住「有内容的实例 vs 空对象」这类误判。命中时整份数组也可以保持同一引用。
 */
export function stabilizeTimelineEntries(
  previous: readonly TimelineEntry[] | undefined,
  next: readonly TimelineEntry[],
): readonly TimelineEntry[] {
  if (previous === undefined || previous.length !== next.length) return next;
  const stabilized: TimelineEntry[] = [];
  let changed = false;
  for (let index = 0; index < next.length; index += 1) {
    const before = previous[index];
    const current = next[index];
    if (current === undefined) return next;
    if (before !== undefined && before.key === current.key && sameTimelineValue(before, current)) {
      stabilized.push(before);
      continue;
    }
    stabilized.push(current);
    changed = true;
  }
  return changed ? stabilized : previous;
}

/** 条目时刻的共享格式化器：浅/深主题无关，时区与 24 小时制固定（`HH:MM`）。 */
let entryTimeFormatter: Intl.DateTimeFormat | null = null;

/** 已格式化结果的缓存上限：时间线上的原始时刻种类远少于这个数。 */
const ENTRY_TIME_CACHE_LIMIT = 512;

const entryTimeCache = new Map<string, string>();

/**
 * 条目时刻的展示形式：只做**位置固定**的字符串截取（`HH:MM`）。
 *
 * 与 `SessionRow` 同一条纪律：`renderToStaticMarkup` 没有时钟，同一份模型必须渲染出同一段文本。
 * 原始时刻经 `data-timeline-entry-at` 原样进 DOM，需要精确值时从属性取。
 *
 * `Intl.DateTimeFormat` 的构造有明显开销，因此**整个应用共用一个实例**；同一时刻再次显示时
 * 直接命中结果缓存（流式更新里绝大多数时刻根本没有变）。
 */
export function formatTimelineTime(createdAt: string): string {
  const cached = entryTimeCache.get(createdAt);
  if (cached !== undefined) return cached;
  const timestamp = Date.parse(createdAt);
  let label: string;
  if (!Number.isFinite(timestamp)) {
    label = "—";
  } else {
    entryTimeFormatter ??= new Intl.DateTimeFormat("zh-CN", {
      timeZone: "Asia/Shanghai", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
    });
    label = entryTimeFormatter.format(timestamp);
  }
  if (entryTimeCache.size >= ENTRY_TIME_CACHE_LIMIT) entryTimeCache.clear();
  entryTimeCache.set(createdAt, label);
  return label;
}
