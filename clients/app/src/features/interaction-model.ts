/**
 * 待处理交互（授权请求 / 提问请求）的状态归并（`design.md` D7）。
 *
 * ## 状态不是一份副本，而是从两份权威**推导**出来的
 *
 * 交互的两半来自不同地方，因此本模块不维护自己的「交互表」，而是每次从以下输入推导：
 *
 * - `permission.requested` / `elicitation.requested` / `*.resolved` 的**结构化记录**
 *   （实时事件与历史条目同形）：请求内容与已到达的终态；
 * - `session.read` 读回的 `pending_interactions`：仍然待处理、但**没有可选项**的项；
 * - 本设备提交的应答命令（`responseRequestIds` + 命令记录）：`submitting` 这一态 ONLY 由它产生。
 *
 * 这样「已被其它设备处理」不需要客户端记住什么——服务端的 `*.resolved` 一到，判定就变。
 *
 * ## 亲历与否只看来源
 *
 * `witnessed` 取「该请求记录来自**实时**事件」。历史条目虽然后来也可能带上 `options`，但它
 * 是本设备**事后读回**的，不满足 spec 的「本设备连接期间收到过请求内容」，因此不可应答。
 * 这条判定把「只有亲历的请求可应答」从一句约定变成了结构上的事实。
 */

import type { InteractionOption } from "../protocol/common";
import type { SnapshotItemPendingInteractions } from "../protocol";
import type { CommandRecord } from "../state";
import { isTerminalState } from "../state";
import type {
  InteractionAvailability,
  InteractionKind,
  InteractionResolution,
  InteractionView,
} from "../domain/interaction";
import { buildInteractionAvailability } from "../domain/interaction";
import type { StructuredRecord } from "../domain/timeline";
import { compareTimelinePosition, positionOf, viewText } from "../domain/timeline";

/** 请求侧的累积（同一 `interactionId` 只保留最新一条请求）。 */
interface RequestAccumulator {
  readonly interactionId: string;
  readonly kind: InteractionKind;
  readonly record: StructuredRecord;
  readonly title: string | null;
  readonly description: string | null;
  readonly schema: Readonly<Record<string, unknown>> | null;
  readonly options: readonly InteractionOption[] | null;
  readonly turnId: string | null;
}

/** 读 `permission.requested.options`；形状不完整即返回 `null`（不给「点了会失败」的按钮数据）。 */
function optionsField(view: Readonly<Record<string, unknown>>): readonly InteractionOption[] | null {
  const value = view["options"];
  if (!Array.isArray(value)) return null;
  const options: InteractionOption[] = [];
  for (const item of value) {
    if (typeof item !== "object" || item === null) return null;
    const record = item as Readonly<Record<string, unknown>>;
    const optionId = record["optionId"];
    const label = record["label"];
    const kind = record["kind"];
    if (typeof optionId !== "string" || typeof label !== "string" || typeof kind !== "string") return null;
    options.push({ optionId, label, kind });
  }
  return options;
}

/** 读 `elicitation.requested.schema`（开放对象）；不是对象时返回 `null`。 */
function schemaField(view: Readonly<Record<string, unknown>>): Readonly<Record<string, unknown>> | null {
  const value = view["schema"];
  return typeof value === "object" && value !== null ? (value as Readonly<Record<string, unknown>>) : null;
}

/** 应答命令的完成结果里的一句话结论（`resolution` / `action` / 兜底）。 */
function commandResultDetail(record: CommandRecord): string {
  const result = record.result;
  const detail = result === null ? null : (viewText(result, "resolution") ?? viewText(result, "action"));
  return detail ?? "已由本设备提交";
}

/**
 * 推导某会话当前的交互视图列表。
 *
 * @param records 已去重的结构化记录（`mergeStructuredRecords` 之后）；
 * @param pending 本次 `session.read` 读回的待处理项；
 * @param responseRequestIds 本设备提交的应答：`interactionId → requestId`；
 * @param deviceId 本设备标识（`auth.authenticated.deviceId`）；未知为 `null`。
 */
export function buildInteractionViews(input: {
  readonly records: readonly StructuredRecord[];
  readonly pending: readonly SnapshotItemPendingInteractions[];
  readonly responseRequestIds: ReadonlyMap<string, string>;
  readonly commands: ReadonlyMap<string, CommandRecord>;
  readonly deviceId: string | null;
  readonly sessionOpen: boolean;
}): readonly InteractionView[] {
  const requests = new Map<string, RequestAccumulator>();
  const resolutions = new Map<string, { readonly resolution: InteractionResolution; readonly record: StructuredRecord }>();

  const placeRequest = (candidate: RequestAccumulator): void => {
    const existing = requests.get(candidate.interactionId);
    // 同一交互的最新请求胜出（协议允许重发请求内容）；位置比较与时间线同一口径。
    if (existing !== undefined && compareTimelinePosition(positionOf(existing.record), positionOf(candidate.record)) >= 0) {
      return;
    }
    requests.set(candidate.interactionId, candidate);
  };

  for (const record of input.records) {
    const view = record.view;
    switch (record.eventType) {
      case "permission.requested":
      case "elicitation.requested": {
        const interactionId = viewText(view, "interactionId");
        if (interactionId === null) break;
        const kind: InteractionKind = record.eventType === "permission.requested" ? "permission" : "elicitation";
        placeRequest({
          interactionId,
          kind,
          record,
          title: viewText(view, "title"),
          description: viewText(view, "description"),
          schema: schemaField(view),
          options: kind === "permission" ? optionsField(view) : null,
          turnId: viewText(view, "turnId"),
        });
        break;
      }
      case "permission.resolved":
      case "elicitation.resolved": {
        const interactionId = viewText(view, "interactionId");
        const detail = viewText(view, "resolution") ?? viewText(view, "action");
        if (interactionId === null || detail === null) break;
        const existing = resolutions.get(interactionId);
        if (existing !== undefined && compareTimelinePosition(positionOf(existing.record), positionOf(record)) >= 0) break;
        resolutions.set(interactionId, {
          resolution: {
            detail,
            // 本设备处理与否先按 `resolvedByDeviceId` 判；该字段缺席时由调用点用「本设备提交过」
            // 兜底（见下面 `resolvedByThisDevice` 的修正），因为此时无法从事件本身区分。
            resolvedByThisDevice: false,
            resolvedByDeviceId: viewText(view, "resolvedByDeviceId"),
          },
          record,
        });
        break;
      }
      default:
        break;
    }
  }

  for (const item of input.pending) {
    if (requests.has(item.interactionId)) continue;
    // 读回的待处理项没有请求内容：没有可选项、没有标题，且 `witnessed` 由「没有实时请求记录」
    // 表达为假——它因此只能呈现为不可操作。
    requests.set(item.interactionId, {
      interactionId: item.interactionId,
      kind: item.kind,
      record: pendingRecordOf(item),
      title: null,
      description: null,
      schema: item.kind === "elicitation" ? item.schema : null,
      options: null,
      turnId: null,
    });
  }

  const views: InteractionView[] = [];
  for (const request of requests.values()) {
    const requestId = input.responseRequestIds.get(request.interactionId);
    const command = requestId === undefined ? undefined : input.commands.get(requestId);
    // 不确定（`uncertain`）同样算「尚未收到终态」：那时继续呈现待确认比放开重提更诚实。
    const inFlight =
      requestId !== undefined &&
      (command === undefined || !isTerminalState(command.state) || command.state === "uncertain");
    const ownResolution =
      command !== undefined && command.state === "completed"
        ? {
            detail: commandResultDetail(command),
            resolvedByThisDevice: true,
            resolvedByDeviceId: input.deviceId,
          }
        : null;
    const serverResolution = resolutions.get(request.interactionId)?.resolution ?? null;
    const resolution =
      serverResolution === null
        ? ownResolution
        : {
            ...serverResolution,
            // 服务端的 `resolvedByDeviceId` 缺席时，用「本设备提交过这条交互」兜底。
            resolvedByThisDevice:
              serverResolution.resolvedByDeviceId !== null
                ? serverResolution.resolvedByDeviceId === input.deviceId
                : requestId !== undefined,
          };

    const availability: InteractionAvailability = buildInteractionAvailability({
      kind: request.kind,
      options: request.options,
      witnessed: request.record.source === "realtime",
      sessionOpen: input.sessionOpen,
      resolution,
      pendingRequestId: inFlight && requestId !== undefined ? requestId : null,
    });

    views.push({
      interactionId: request.interactionId,
      sessionId: request.record.sessionId,
      kind: request.kind,
      title: request.title,
      description: request.description,
      schema: request.schema,
      requestedAt: request.record.createdAt,
      turnId: request.turnId,
      witnessed: request.record.source === "realtime",
      availability,
    });
  }

  return views.sort((a, b) =>
    a.requestedAt === b.requestedAt ? (a.interactionId < b.interactionId ? -1 : 1) : a.requestedAt < b.requestedAt ? -1 : 1,
  );
}

/** 读回待处理项的位置（它的 `createdAt` 参与「最新请求胜出」的比较）。 */
function pendingRecordOf(item: SnapshotItemPendingInteractions): StructuredRecord {
  return {
    source: "history",
    eventId: `pending:${item.interactionId}`,
    eventType: item.kind === "permission" ? "permission.requested" : "elicitation.requested",
    sessionId: item.sessionId,
    createdAt: item.createdAt,
    sessionSequence: null,
    globalSequence: null,
    originKind: null,
    view: {},
  };
}
