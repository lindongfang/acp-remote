/**
 * 时间线的**结构化条目**：把「实时事件」与「按会话读取的历史条目」归一成同一种记录，
 * 再投影成同一套卡片（`design.md` D1、D2；spec「对话页按时间线呈现消息与结构化条目」）。
 *
 * ## 为什么只有一套记录与一套卡片
 *
 * `snapshotItem.history` 与 `event.payload` 同形（`view` 走同一个 `projectView`）。若历史另造
 * 形状，就等于两套解析、两套降级、两套测试——而它们描述的是同一批事件。因此这里先把两种来源
 * 归一成 {@link StructuredRecord}，之后「实时 vs 历史」只剩 `source` 这一个字段的差别，卡片
 * 与降级判定都**不知道**自己面对的是哪种来源。
 *
 * ## 归并（同一标识只呈现一处）
 *
 * 1. **去重键是 `eventId`**（`mergeStructuredRecords`）：主键来自 `event.schema.json` 的稳定
 *    事件标识，实时与历史都带它；同一条事件从两条来源进来时只保留一处。实时记录优先——
 *    它带 `globalSequence` 与 ACP 原文的可用性（历史条目的 `acp` 可能被保留策略裁掉）。
 * 2. **归并成一张卡片**（{@link groupStructuredRecords}）：`tool.call.{started,updated,completed}`
 *    按 `toolCallId`、`turn.{queued,started,completed,cancelled,failed}` 按 `turnId`。它们是同
 *    一个对象的生命周期，逐条画成一串卡片只会把时间线塞满（D1 的备选就这么否决的）。
 *
 * ## 时间顺序
 *
 * 以 `createdAt` 为主键；同一时刻退到**会话内序号**（历史与实时都有它，`beforeHistory` 用的是
 * 同一口径），再退到全局投递序号，最后按构造次序（`Array.prototype.sort` 稳定）。序号比较用
 * `BigInt`：它是十进制字符串，位数不同时字典序会给错答案。
 */

import type { ConfigOptionChoice, ConfigOptionView, PublicError, RawAcp } from "../protocol/common";
import type {
  EventMessage,
  PlanPriority,
  PlanStatus,
  TerminalStream,
  ToolDetailPart,
} from "../protocol/event";
import type { SnapshotItemHistory } from "../protocol/sync";
import type { RemoteEventItem } from "../protocol/sync";
import type { InteractionAvailability } from "./interaction";

/** 结构化来源：本连接的实时事件，或按会话读取的历史条目。 */
export type StructuredSource = "realtime" | "history";

/** 归一后的结构化记录：两种来源在卡片/降级判定眼里完全同形。 */
export interface StructuredRecord {
  readonly source: StructuredSource;
  /**
   * 来源 Owner 节点；本节点自有记录为 `null`。
   *
   * 远程去重键必须绑定来源节点：不同 Owner 的 `eventId` 属于不同命名空间，只按裸 `eventId`
   * 合并会把两个来源的同一标识当成一条（`design.md` D3 的来源命名空间要求）。
   */
  readonly ownerNodeId: string | null;
  readonly eventId: string;
  readonly eventType: string;
  readonly sessionId: string | null;
  readonly createdAt: string;
  /** 会话内序号：实时事件的 `body.sessionSequence`，历史条目**必有**。 */
  readonly sessionSequence: string | null;
  /** 全局投递序号：只有实时事件有（历史条目不是本次连接的投递记录）。 */
  readonly globalSequence: string | null;
  /** 事件来源种类（`body.origin.kind`）；历史条目没有 origin，为 `null`。 */
  readonly originKind: string | null;
  readonly view: Readonly<Record<string, unknown>>;
  readonly acp?: RawAcp;
}

/** 实时事件 → 结构化记录。 */
export function recordFromEvent(event: EventMessage): StructuredRecord {
  return {
    source: "realtime",
    ownerNodeId: null,
    eventId: event.body.eventId,
    eventType: event.body.eventType,
    sessionId: event.body.sessionId,
    createdAt: event.body.createdAt,
    sessionSequence: event.body.sessionSequence,
    globalSequence: event.body.globalSequence,
    originKind: event.body.origin.kind,
    view: event.body.payload.view,
    ...(event.body.payload.acp === undefined ? {} : { acp: event.body.payload.acp }),
  };
}

/**
 * 历史条目 → 结构化记录。
 *
 * `sessionId` 由调用方从**结果**给出：`snapshotItem.history` 自己没有这个字段（整页都属于同
 * 一次 `session.read`），因此它不是「缺失」，而是「页级信息」。
 */
export function recordFromHistoryItem(
  item: SnapshotItemHistory,
  sessionId: string,
  ownerNodeId: string | null = null,
): StructuredRecord {
  return {
    source: "history",
    ownerNodeId,
    eventId: item.eventId,
    eventType: item.eventType,
    sessionId,
    createdAt: item.createdAt,
    sessionSequence: item.sessionSequence,
    globalSequence: null,
    originKind: null,
    view: item.view,
    ...(item.acp === undefined ? {} : { acp: item.acp }),
  };
}

/**
 * 来源事件 → 结构化记录（`sync.remote_event`，§9.7）。
 *
 * 远程事件**没有** Access 的全局/会话投递序号；这里的 `sessionSequence` 直接使用 Owner 的
 * `originSequence`，只用于在时间线上稳定排序与历史去重，**不是** Access 本机序号，也从不参与
 * 本机 ACK/游标。`ownerNodeId` 是来源命名空间的一部分，清理某来源的展示数据时按它筛选。
 */
export function recordFromRemoteEvent(event: RemoteEventItem): StructuredRecord {
  return {
    source: "realtime",
    ownerNodeId: event.sessionRef.ownerNodeId,
    eventId: event.originEventId,
    eventType: event.eventType,
    sessionId: event.sessionRef.sessionId,
    createdAt: event.createdAt,
    sessionSequence: event.originSequence,
    globalSequence: null,
    originKind: "node",
    view: event.payload.view ?? {},
    ...(event.payload.acp === undefined ? {} : { acp: event.payload.acp }),
  };
}

/** 一条记录在时间线上的位置（`createdAt` + 次序分量）。 */
export interface TimelinePosition {
  readonly createdAt: string;
  readonly sequence: string | null;
}

/** 记录的位置：次序分量优先取会话内序号，其次全局序号。 */
export function positionOf(record: StructuredRecord): TimelinePosition {
  return { createdAt: record.createdAt, sequence: record.sessionSequence ?? record.globalSequence };
}

/**
 * 位置比较。
 *
 * 时间走 `Date.parse`（`createdAt` 是固定形状的 UTC 毫秒时间戳）；同一时刻按次序分量升序，
 * 分量缺席（`null`）排在有值的**前面**（它没有可比信息，保持「先构造先出现」更不容易制造错觉）。
 */
export function compareTimelinePosition(a: TimelinePosition, b: TimelinePosition): number {
  const delta = Date.parse(a.createdAt) - Date.parse(b.createdAt);
  if (delta !== 0) return delta;
  if (a.sequence === b.sequence) return 0;
  if (a.sequence === null) return -1;
  if (b.sequence === null) return 1;
  return BigInt(a.sequence) < BigInt(b.sequence) ? -1 : 1;
}

/**
 * 去重：同一 `eventId` 只保留一处，实时来源优先。
 *
 * 实时优先的理由是**信息更全**而不是「更新」：它带 `globalSequence`，且 `acp` 是本次投递
 * 携带的（历史条目的原文可能按保留策略裁掉）。两条记录的 `view` 描述同一件事，因此用哪条都
 * 不会呈现成不同内容——但把两条都留下就会在时间线上显示两次，spec MUST NOT。
 */
export function mergeStructuredRecords(
  realtime: readonly StructuredRecord[],
  history: readonly StructuredRecord[],
): readonly StructuredRecord[] {
  const byId = new Map<string, StructuredRecord>();
  for (const record of realtime) {
    if (!byId.has(record.eventId)) byId.set(record.eventId, record);
  }
  for (const record of history) {
    if (!byId.has(record.eventId)) byId.set(record.eventId, record);
  }
  return [...byId.values()];
}

/** 一个时间线条目对应的记录组：分组类型有多条（生命周期），其余只有一条。 */
export interface RecordGroup {
  /** 条目标识：分组类型为 `<家族>:<标识>`，其余为 `event:<eventId>`。 */
  readonly key: string;
  readonly records: readonly StructuredRecord[];
  /** 最早一条（决定条目在时间线上的位置）。 */
  readonly earliest: StructuredRecord;
  /** 最新一条（决定卡片呈现的状态）。 */
  readonly latest: StructuredRecord;
}

/** 分组家族：生命周期事件按对象标识合并，其余一事件一条目。 */
const GROUPED_EVENT_TYPES: Readonly<Record<string, { readonly family: string; readonly idField: string }>> = {
  "turn.queued": { family: "turn", idField: "turnId" },
  "turn.started": { family: "turn", idField: "turnId" },
  "turn.completed": { family: "turn", idField: "turnId" },
  "turn.cancelled": { family: "turn", idField: "turnId" },
  "turn.failed": { family: "turn", idField: "turnId" },
  "tool.call.started": { family: "tool_call", idField: "toolCallId" },
  "tool.call.updated": { family: "tool_call", idField: "toolCallId" },
  "tool.call.completed": { family: "tool_call", idField: "toolCallId" },
};

/** 读一个字符串字段（视图是开放对象，字段可能是任意东西，因此须在运行期确认）。 */
export function viewText(view: Readonly<Record<string, unknown>>, key: string): string | null {
  const value = view[key];
  return typeof value === "string" && value.length > 0 ? value : null;
}

/**
 * 一个**存在**的工具详情部分。
 *
 * 用 `present` 而不是「值是否为真」来判定在场：`null`、`false`、`0`、`""` 都是合法的 JSON 取值，
 * 把它们当成缺席会把「Agent 明确给出了空值」误报成「Agent 没有提供」。
 */
export interface ToolDetailPartValue {
  readonly present: true;
  readonly value: unknown;
}

/**
 * 归一后的工具详情：三个内容部分各自可缺席，`truncatedParts` 按 `input` → `output` → `text`
 * 的固定顺序去重。
 *
 * 只由 `view.toolDetail` 构造；`acp.rawJson` 不参与（客户端不得用原文补造结构化详情）。
 */
export interface ToolCallDetail {
  readonly input: ToolDetailPartValue | null;
  readonly output: ToolDetailPartValue | null;
  readonly text: readonly string[] | null;
  readonly truncatedParts: readonly ToolDetailPart[];
}

/** `toolDetail` 的三个内容部分（固定顺序，也用于截断集合的稳定排序）。 */
const TOOL_DETAIL_PARTS: readonly ToolDetailPart[] = ["input", "output", "text"];

const TOOL_DETAIL_PART_NAMES: Readonly<Record<string, true>> = { input: true, output: true, text: true };

/** 普通对象（JSON 解析产物）：数组与 `null` 都不是 `toolDetail` 的合法形状。 */
function isPlainRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

/** 读一个内容部分：按**键是否在场**判定，值本身可以是任意 JSON 值。 */
function toolDetailPartField(
  raw: Readonly<Record<string, unknown>>,
  key: "input" | "output",
): ToolDetailPartValue | null {
  if (!Object.hasOwn(raw, key) || raw[key] === undefined) return null;
  return { present: true, value: raw[key] };
}

/** 读 `text`：必须是**非空**的字符串数组；形状不合规时整部分按缺席处理。 */
function toolDetailTextField(raw: Readonly<Record<string, unknown>>): readonly string[] | null {
  if (!Object.hasOwn(raw, "text")) return null;
  const value = raw["text"];
  if (!Array.isArray(value) || value.length === 0) return null;
  const texts: string[] = [];
  for (const item of value) {
    if (typeof item !== "string") return null;
    texts.push(item);
  }
  return texts;
}

/** 读 `truncatedParts`：未知取值忽略，已知取值按固定顺序去重（不因为一条垃圾值丢掉其余登记）。 */
function toolDetailTruncatedPartsField(raw: Readonly<Record<string, unknown>>): readonly ToolDetailPart[] {
  if (!Object.hasOwn(raw, "truncatedParts")) return [];
  const value = raw["truncatedParts"];
  if (!Array.isArray(value)) return [];
  const present = new Set<ToolDetailPart>();
  for (const item of value) {
    if (typeof item === "string" && TOOL_DETAIL_PART_NAMES[item] === true) present.add(item as ToolDetailPart);
  }
  return TOOL_DETAIL_PARTS.filter((part) => present.has(part));
}

/**
 * 从一条工具调用视图读取并归一 `toolDetail`。
 *
 * 返回 `null` 表示「没有任何可用详情」：`toolDetail` 缺席/不是普通对象，或三个部分都缺席且没有
 * 截断登记。非法部分**逐部分**降级为缺席，不让一处坏形状吞掉其余合法部分，也不抛异常。
 */
export function toolDetailOf(view: Readonly<Record<string, unknown>>): ToolCallDetail | null {
  if (!Object.hasOwn(view, "toolDetail")) return null;
  const raw = view["toolDetail"];
  if (!isPlainRecord(raw)) return null;
  const detail: ToolCallDetail = {
    input: toolDetailPartField(raw, "input"),
    output: toolDetailPartField(raw, "output"),
    text: toolDetailTextField(raw),
    truncatedParts: toolDetailTruncatedPartsField(raw),
  };
  return hasToolDetail(detail) ? detail : null;
}

function hasToolDetail(detail: ToolCallDetail): boolean {
  return (
    detail.input !== null ||
    detail.output !== null ||
    detail.text !== null ||
    detail.truncatedParts.length > 0
  );
}

/**
 * 跨事件归并同一 `toolCallId` 的详情。
 *
 * `views` 必须已按时间升序：每个内容部分取**最新出现的非缺席值**（`started` 通常只有输入、
 * `completed` 才有输出，只取最新一条会丢掉输入）；`truncatedParts` 取所有记录的并集，避免
 * 后一条事件覆盖掉先前的截断说明。
 */
function mergeToolDetails(views: readonly Readonly<Record<string, unknown>>[]): ToolCallDetail | null {
  let input: ToolDetailPartValue | null = null;
  let output: ToolDetailPartValue | null = null;
  let text: readonly string[] | null = null;
  const truncated = new Set<ToolDetailPart>();
  for (const view of views) {
    const detail = toolDetailOf(view);
    if (detail === null) continue;
    if (detail.input !== null) input = detail.input;
    if (detail.output !== null) output = detail.output;
    if (detail.text !== null) text = detail.text;
    for (const part of detail.truncatedParts) truncated.add(part);
  }
  const detail: ToolCallDetail = {
    input,
    output,
    text,
    truncatedParts: TOOL_DETAIL_PARTS.filter((part) => truncated.has(part)),
  };
  return hasToolDetail(detail) ? detail : null;
}

/** 读结构化公开错误（`turn.failed.error`）；形状不对时返回 `null`，不编造码与消息。 */
function errorField(view: Readonly<Record<string, unknown>>): PublicError | null {
  const value = view["error"];
  if (typeof value !== "object" || value === null) return null;
  const code = (value as Readonly<Record<string, unknown>>)["code"];
  const message = (value as Readonly<Record<string, unknown>>)["message"];
  if (typeof code !== "string" || typeof message !== "string") return null;
  const retryable = (value as Readonly<Record<string, unknown>>)["retryable"];
  const details = (value as Readonly<Record<string, unknown>>)["details"];
  return {
    code: code as PublicError["code"],
    message,
    retryable: retryable === true,
    details: typeof details === "object" && details !== null ? (details as Record<string, unknown>) : {},
  };
}

/** 计划条目（`session.plan.changed.entries`）。 */
export interface PlanEntryView {
  readonly content: string;
  readonly priority: PlanPriority;
  readonly status: PlanStatus;
}

const PLAN_PRIORITIES: Readonly<Record<string, PlanPriority>> = { high: "high", medium: "medium", low: "low" };
const PLAN_STATUSES: Readonly<Record<string, PlanStatus>> = { pending: "pending", in_progress: "in_progress", completed: "completed" };

/** 读计划条目数组；任何一条缺必填字段即返回 `null`（宁可不画卡片，也不画一条残缺的计划）。 */
function planEntriesField(view: Readonly<Record<string, unknown>>): readonly PlanEntryView[] | null {
  const value = view["entries"];
  if (!Array.isArray(value)) return null;
  const entries: PlanEntryView[] = [];
  for (const item of value) {
    if (typeof item !== "object" || item === null) return null;
    const record = item as Readonly<Record<string, unknown>>;
    const content = record["content"];
    const priority = typeof record["priority"] === "string" ? PLAN_PRIORITIES[record["priority"]] : undefined;
    const status = typeof record["status"] === "string" ? PLAN_STATUSES[record["status"]] : undefined;
    if (typeof content !== "string" || priority === undefined || status === undefined) return null;
    entries.push({ content, priority, status });
  }
  return entries;
}

/** 读一个配置项的可选项数组（`configOptionView.options`）；形状不完整即返回 `null`。 */
function configChoicesField(value: unknown): readonly ConfigOptionChoice[] | null {
  if (!Array.isArray(value)) return null;
  const choices: ConfigOptionChoice[] = [];
  for (const item of value) {
    if (typeof item !== "object" || item === null) return null;
    const record = item as Readonly<Record<string, unknown>>;
    const choiceValue = record["value"];
    const name = record["name"];
    if (typeof choiceValue !== "string" || typeof name !== "string") return null;
    const description = record["description"];
    choices.push({ value: choiceValue, name, description: typeof description === "string" ? description : null });
  }
  return choices;
}

/** 读配置项数组（`session.config.changed.configOptions`）；形状不完整即返回 `null`。 */
function configOptionsField(view: Readonly<Record<string, unknown>>): readonly ConfigOptionView[] | null {
  const value = view["configOptions"];
  if (!Array.isArray(value)) return null;
  const options: ConfigOptionView[] = [];
  for (const item of value) {
    if (typeof item !== "object" || item === null) return null;
    const record = item as Readonly<Record<string, unknown>>;
    const id = record["id"];
    const name = record["name"];
    const type = record["type"];
    const currentValue = record["currentValue"];
    if (typeof id !== "string" || typeof name !== "string" || typeof type !== "string") return null;
    if (type !== "select" && type !== "boolean") return null;
    if (typeof currentValue !== "string" && typeof currentValue !== "boolean") return null;
    const description = record["description"];
    const category = record["category"];
    // 可选项必须带出来：模型/模式分节的「可选项」就是它——丢掉它会让「模型」永远显示「未提供」。
    const choices = record["options"] === undefined ? undefined : configChoicesField(record["options"]);
    if (choices === null) return null;
    options.push({
      id,
      name,
      description: typeof description === "string" ? description : null,
      category: typeof category === "string" ? category : null,
      type,
      currentValue,
      ...(choices === undefined ? {} : { options: choices }),
    });
  }
  return options;
}

/** 时间线上的结构化卡片（`kind` 与消息/降级互不重叠，可直接作为条目的判别键）。 */
export type TimelineCard =
  | TurnTimelineCard
  | ToolCallTimelineCard
  | TerminalOutputTimelineCard
  | PlanTimelineCard
  | FileChangedTimelineCard
  | ModeChangedTimelineCard
  | ConfigChangedTimelineCard
  | PermissionRequestedTimelineCard
  | ElicitationRequestedTimelineCard
  | PermissionResolvedTimelineCard
  | ElicitationResolvedTimelineCard;

/** 轮次生命周期卡片（同一 `turnId` 的多条事件合并成一张，状态取最新一条）。 */
export interface TurnTimelineCard {
  readonly kind: "turn";
  readonly turnId: string;
  readonly state: string;
  readonly error: PublicError | null;
}

/** 工具调用卡片（同一 `toolCallId` 的多条事件合并成一张）。 */
export interface ToolCallTimelineCard {
  readonly kind: "tool_call";
  readonly toolCallId: string;
  readonly turnId: string | null;
  readonly title: string;
  readonly state: string;
  /** 最新记录的事件标识：诊断入口按它打开原文；原文可用性由诊断层判定。 */
  readonly eventId: string;
  /** 跨事件归并后的详情；三部分都缺席且没有截断登记时为 `null`（不呈现空占位）。 */
  readonly detail: ToolCallDetail | null;
}

/** 终端输出卡片（按 `chunkIndex` 逐块呈现，不合并）。 */
export interface TerminalOutputTimelineCard {
  readonly kind: "terminal_output";
  readonly terminalId: string;
  readonly chunkIndex: string;
  readonly stream: TerminalStream;
  readonly text: string;
  readonly truncated: boolean;
}

/** 计划变更卡片。 */
export interface PlanTimelineCard {
  readonly kind: "plan";
  readonly entries: readonly PlanEntryView[];
}

/**
 * 文件改动卡片：结构化统计、相对化路径（D8）与**结构化差异区**（本变更新增）。
 *
 * `addedLines`/`deletedLines`/`outsideWorkspace` 在视图里是可选的——「判定不出时省略而不是
 * 填零」，因此这里是 `string | null` 而不是 `0`。`diff` 的三种形态同样互斥且都可缺席
 * （见 {@link FileDiffView}）。客户端 MUST NOT 为了填满这里去解析 ACP 原文。
 */
export interface FileChangedTimelineCard {
  readonly kind: "file_changed";
  readonly changeId: string;
  readonly changeKind: string;
  /** core 已相对化的展示路径；客户端不自己相对化（它没有路径规范化的知识）。 */
  readonly displayPath: string;
  readonly summary: string;
  readonly addedLines: string | null;
  readonly deletedLines: string | null;
  readonly outsideWorkspace: boolean;
  /** 差异区：有差异行 / 有不可用标记 / 两者都没有（`null`，即「只有统计」）。 */
  readonly diff: FileDiffView | null;
}

/** 差异行的一档（`$defs.file.changed.diffLines[].kind`，闭集）。 */
export type DiffLineKind = "added" | "deleted" | "context";

/** 一行结构化差异；`text` 是行内容，按**不可信文本**呈现（不解析、不拼成 HTML）。 */
export interface DiffLineView {
  readonly kind: DiffLineKind;
  readonly text: string;
}

/** 差异行未随事件下发时的原因（`$defs.file.changed.diffUnavailable.reason`，闭集）。 */
export type DiffUnavailableReason = "size_limit";

/**
 * 右栏差异区的三种形态，**结构性互斥**：
 *
 * - `lines`：服务端随事件下发了结构化差异行，客户端逐行呈现并区分档位；
 * - `unavailable`：服务端明确说明为什么没有差异行（例如超出单条上限）；
 * - `null`（整个字段缺席）：两者都没有，保持「只有统计」的形态。
 *
 * 三态之间没有第三种可能：判别键 `kind` 只有一个，而「都没有」由字段本身缺席表达——
 * 因此「有统计却渲染出一片空白的差异区」在类型上就写不出来。
 */
export type FileDiffView =
  | { readonly kind: "lines"; readonly lines: readonly DiffLineView[] }
  | { readonly kind: "unavailable"; readonly reason: DiffUnavailableReason };

/**
 * `diffUnavailable.reason` → 呈现文案（与 `INTERACTION_UNAVAILABLE_LABELS` 同一做法：
 * 词表取值封闭，文案与取值一一对应地放在一处）。
 */
export const FILE_DIFF_UNAVAILABLE_LABELS: Readonly<Record<DiffUnavailableReason, string>> = {
  size_limit: "这一处的差异行超过单条上限，未随事件下发；完整原文可在诊断里查看。",
};

/** `diffLines[].kind` 的闭集词表：未知取值即形状违规，不当作上下文行吞下。 */
const DIFF_LINE_KINDS: Readonly<Record<string, DiffLineKind>> = {
  added: "added",
  deleted: "deleted",
  context: "context",
};

/** `diffUnavailable.reason` 的闭集词表：未知取值同上。 */
const DIFF_UNAVAILABLE_REASONS: Readonly<Record<string, DiffUnavailableReason>> = { size_limit: "size_limit" };

/**
 * 读结构化差异区（`$defs.file.changed` 的 `diffLines` / `diffUnavailable`）。
 *
 * 两个键在 schema 里互斥且都可缺席（未协商 `core.file-diff.v1`、或差异行超限时都不下发）。
 * 形状不合规**不**伪造成另一种形态：返回 `null`（「两者都没有」），因为客户端 MUST NOT
 * 解析 `acp.rawJson` 补齐差异、也 MUST NOT 用占位行假装完整——这正是本函数不碰 `acp` 的原因。
 * `diffLines` 在场但形状不合规时同样得到 `null`，而不是回落去读互斥的那个标记。
 */
function fileDiffField(view: Readonly<Record<string, unknown>>): FileDiffView | null {
  const value = view["diffLines"];
  if (Array.isArray(value)) {
    if (value.length === 0) return null;
    const lines: DiffLineView[] = [];
    for (const item of value) {
      if (typeof item !== "object" || item === null) return null;
      const record = item as Readonly<Record<string, unknown>>;
      const kind = typeof record["kind"] === "string" ? DIFF_LINE_KINDS[record["kind"]] : undefined;
      const text = record["text"];
      if (kind === undefined || typeof text !== "string") return null;
      lines.push({ kind, text });
    }
    return { kind: "lines", lines };
  }

  const marker = view["diffUnavailable"];
  if (typeof marker !== "object" || marker === null) return null;
  const reason = (marker as Readonly<Record<string, unknown>>)["reason"];
  if (typeof reason !== "string") return null;
  const known = DIFF_UNAVAILABLE_REASONS[reason];
  return known === undefined ? null : { kind: "unavailable", reason: known };
}

/** 模式变更卡片。 */
export interface ModeChangedTimelineCard {
  readonly kind: "mode_changed";
  readonly currentModeId: string | null;
  readonly version: string;
}

/** 配置（含模型）变更卡片。 */
export interface ConfigChangedTimelineCard {
  readonly kind: "config_changed";
  readonly configOptions: readonly ConfigOptionView[];
  readonly version: string;
}

/** 授权请求卡片：可应答性由 `domain/interaction.ts` 的判定注入（历史来源没有可选项）。 */
export interface PermissionRequestedTimelineCard {
  readonly kind: "permission_requested";
  readonly interactionId: string;
  readonly turnId: string | null;
  readonly title: string;
  readonly description: string | null;
  readonly toolKind?: string | null;
  readonly detail?: ToolCallDetail | null;
  readonly availability: InteractionAvailability;
}

/** 提问请求卡片（`schema` 是开放对象，原样带出，由组件按不可信输入处理）。 */
export interface ElicitationRequestedTimelineCard {
  readonly kind: "elicitation_requested";
  readonly interactionId: string;
  readonly turnId: string | null;
  readonly title: string;
  readonly schema: Readonly<Record<string, unknown>>;
  readonly availability: InteractionAvailability;
}

/** 授权已处理卡片。 */
export interface PermissionResolvedTimelineCard {
  readonly kind: "permission_resolved";
  readonly interactionId: string;
  readonly resolution: string;
  readonly resolvedByDeviceId: string | null;
}

/** 提问已处理卡片。 */
export interface ElicitationResolvedTimelineCard {
  readonly kind: "elicitation_resolved";
  readonly interactionId: string;
  readonly action: string;
  readonly resolvedByDeviceId: string | null;
}

/**
 * 记录组 → 卡片。
 *
 * 返回 `null` 表示「这条事件的内容不足以画出卡片」（视图缺呈现所需的必填字段）。调用方**只**
 * 在覆盖表说是时间线卡片时才调用本函数，因此 `null` 只会来自形状违规，而不是「没有视图」。
 *
 * 授权/提问两类卡片刻意**不**在这里查表取可选项，而是通过 `availabilityFor` 回调向外取
 * 可用性判定：可选项是否存在、是否已处理、会话是否开放都取决于交互状态（本设备是否亲历、
 * 其它设备是否已处理），那属于 `features/interaction-model.ts` 的职责，domain 层不该猜。
 */
export function buildTimelineCard(
  group: RecordGroup,
  availabilityFor: (interactionId: string) => InteractionAvailability | null,
): TimelineCard | null {
  const latest = group.latest;
  const view = latest.view;
  switch (latest.eventType) {
    case "turn.queued":
    case "turn.started":
    case "turn.completed":
    case "turn.cancelled":
    case "turn.failed": {
      const turnId = viewText(group.earliest.view, "turnId");
      const state = viewText(view, "state");
      if (turnId === null || state === null) return null;
      return { kind: "turn", turnId, state, error: errorField(view) };
    }
    case "tool.call.started":
    case "tool.call.updated":
    case "tool.call.completed": {
      const toolCallId = viewText(group.earliest.view, "toolCallId");
      // 到达顺序不等于时间顺序；各部分组成取最新非缺席值，因此先按位置排序。
      const ordered = [...group.records].sort((a, b) =>
        compareTimelinePosition(positionOf(a), positionOf(b)),
      );
      // 真实 Agent 的 updated/completed 可能把 title 写成空串；空串不是「标题缺失」的
      // 权威事实，取组内最新的非空标题（通常来自 started），否则才降级。
      const title =
        ordered
          .map((record) => viewText(record.view, "title"))
          .filter((value): value is string => value !== null && value.length > 0)
          .at(-1) ?? null;
      const state = viewText(view, "state");
      if (toolCallId === null || title === null || state === null) return null;
      return {
        kind: "tool_call",
        toolCallId,
        turnId: viewText(view, "turnId"),
        title,
        state,
        eventId: group.latest.eventId,
        detail: mergeToolDetails(ordered.map((record) => record.view)),
      };
    }
    case "terminal.output": {
      const terminalId = viewText(view, "terminalId");
      const chunkIndex = viewText(view, "chunkIndex");
      const stream = view["stream"];
      const text = viewText(view, "text");
      if (terminalId === null || chunkIndex === null || text === null) return null;
      if (stream !== "stdout" && stream !== "stderr") return null;
      return { kind: "terminal_output", terminalId, chunkIndex, stream, text, truncated: view["truncated"] === true };
    }
    case "session.plan.changed": {
      const entries = planEntriesField(view);
      if (entries === null) return null;
      return { kind: "plan", entries };
    }
    case "file.changed": {
      const changeId = viewText(view, "changeId");
      const changeKind = viewText(view, "kind");
      const displayPath = viewText(view, "displayPath");
      const summary = viewText(view, "summary");
      if (changeId === null || changeKind === null || displayPath === null || summary === null) return null;
      return {
        kind: "file_changed",
        changeId,
        changeKind,
        displayPath,
        summary,
        addedLines: viewText(view, "addedLines"),
        deletedLines: viewText(view, "deletedLines"),
        outsideWorkspace: view["outsideWorkspace"] === true,
        diff: fileDiffField(view),
      };
    }
    case "session.mode.changed": {
      const currentModeId = viewText(view, "currentModeId");
      const version = viewText(view, "version");
      if (version === null) return null;
      return { kind: "mode_changed", currentModeId, version };
    }
    case "session.config.changed": {
      const configOptions = configOptionsField(view);
      const version = viewText(view, "version");
      if (configOptions === null || version === null) return null;
      return { kind: "config_changed", configOptions, version };
    }
    case "permission.requested": {
      const interactionId = viewText(view, "interactionId");
      const title = viewText(view, "title");
      const availability = interactionId === null ? null : availabilityFor(interactionId);
      if (interactionId === null || title === null || availability === null) return null;
      return {
        kind: "permission_requested",
        interactionId,
        turnId: viewText(view, "turnId"),
        title,
        description: viewText(view, "description"),
        toolKind: viewText(view, "toolKind"),
        detail: toolDetailOf(view),
        availability,
      };
    }
    case "elicitation.requested": {
      const interactionId = viewText(view, "interactionId");
      const title = viewText(view, "title");
      const schema = view["schema"];
      const availability = interactionId === null ? null : availabilityFor(interactionId);
      if (interactionId === null || title === null || availability === null) return null;
      if (typeof schema !== "object" || schema === null) return null;
      return {
        kind: "elicitation_requested",
        interactionId,
        turnId: viewText(view, "turnId"),
        title,
        schema: schema as Readonly<Record<string, unknown>>,
        availability,
      };
    }
    case "permission.resolved": {
      const interactionId = viewText(view, "interactionId");
      const resolution = viewText(view, "resolution");
      if (interactionId === null || resolution === null) return null;
      return { kind: "permission_resolved", interactionId, resolution, resolvedByDeviceId: viewText(view, "resolvedByDeviceId") };
    }
    case "elicitation.resolved": {
      const interactionId = viewText(view, "interactionId");
      const action = viewText(view, "action");
      if (interactionId === null || action === null) return null;
      return { kind: "elicitation_resolved", interactionId, action, resolvedByDeviceId: viewText(view, "resolvedByDeviceId") };
    }
    default:
      // 覆盖表之外的类型没有卡片；这里返回 `null` 只是二次防线（调用方本不该走到）。
      return null;
  }
}

/** 单条记录构成的记录组（不参与生命周期归并的调用方用它复用同一套卡片构造）。 */
export function recordGroupOf(record: StructuredRecord): RecordGroup {
  return { key: `event:${record.eventId}`, records: [record], earliest: record, latest: record };
}

/**
 * 记录 → 记录组（去重之后调用）。
 *
 * 分组键带对象标识：`tool_call:<toolCallId>` / `turn:<turnId>`；标识取不到时退化成
 * `event:<eventId>`（宁可多一张卡片，也不把无关的两条事件并成一张）。
 */
export function groupStructuredRecords(records: readonly StructuredRecord[]): readonly RecordGroup[] {
  const groups = new Map<string, { records: StructuredRecord[]; earliest: StructuredRecord; latest: StructuredRecord }>();
  for (const record of records) {
    const grouping = GROUPED_EVENT_TYPES[record.eventType];
    const id = grouping === undefined ? null : viewText(record.view, grouping.idField);
    const key = grouping === undefined || id === null ? `event:${record.eventId}` : `${grouping.family}:${id}`;
    const existing = groups.get(key);
    if (existing === undefined) {
      groups.set(key, { records: [record], earliest: record, latest: record });
      continue;
    }
    existing.records.push(record);
    if (compareTimelinePosition(positionOf(record), positionOf(existing.latest)) > 0) existing.latest = record;
    if (compareTimelinePosition(positionOf(record), positionOf(existing.earliest)) < 0) existing.earliest = record;
  }
  return [...groups.entries()]
    .map(([key, value]) => ({ key, records: value.records, earliest: value.earliest, latest: value.latest }))
    .sort((a, b) => compareTimelinePosition(positionOf(a.earliest), positionOf(b.earliest)));
}
