/**
 * 类型化媒体与部分结果的**视图模型**（`docs/SYNC_PROTOCOL.md` §10.4）。
 *
 * ## 只消费专用视图
 *
 * 本层只把 `AgentContentBlock` 投影成「文本 + 若干媒体块」：媒体块带类型、MIME、字节长度、
 * 可用性与（可选）附件标识。它**不解析** `acp.rawJson`——原文只在诊断抽屉里按不可信文本出现。
 *
 * ## 不可用是显式事实
 *
 * `expired` / `not_fetched` / `unsupported` 各有独立标注，MUST NOT 用空字节或空 URL 冒充内容。
 * `not_fetched` 只出现在 Agent 给出外部 URI 的情形：展示资源链接本身不会触发任何网络请求。
 */

import type { AgentContentBlock } from "../protocol/common";

/** 消息通道视图：`reply`（正文）或 `thought`（思考）。 */
export type MessageChannelView = "reply" | "thought";

/** 媒体类别（与协议 `mediaReference.kind` 同义）。 */
export type MediaKindView = "image" | "audio" | "file";

/** 媒体可用性（与协议 `availability` / `displayState` 的并集）。 */
export type MediaAvailabilityView = "available" | "expired" | "not_fetched" | "unsupported";

/** 一个媒体/资源块。 */
export type MediaBlockView =
  | {
      readonly kind: "image" | "audio";
      readonly mimeType: string;
      readonly byteLength: string | null;
      readonly availability: MediaAvailabilityView;
      /** 已抽取为 Owner 附件时的标识；`null` = 只有外部 URI 或未取得内容。 */
      readonly attachmentId: string | null;
    }
  | {
      readonly kind: "file";
      readonly attachmentId: string;
      readonly mimeType: string;
      readonly byteLength: string;
      readonly displayName: string | null;
      readonly availability: MediaAvailabilityView;
    }
  | {
      readonly kind: "resource";
      readonly uri: string;
      readonly name: string | null;
      readonly availability: MediaAvailabilityView;
    }
  | {
      readonly kind: "unsupported";
      readonly originalType: string;
    };

/** 一条消息的完整内容视图。 */
export interface AgentMessageContent {
  /** 文本块按到达顺序拼成一段（媒体块不混进正文）。 */
  readonly text: string;
  readonly media: readonly MediaBlockView[];
  readonly hasStructuredContent: boolean;
}

/** 部分结果的视图（`partialResult`）。 */
export interface PartialResultView {
  readonly state: "completed" | "partial" | "failed" | "cancelled";
  readonly reason: string | null;
  /** 呈现文案；`completed` 为 `null`（不需要额外标注）。 */
  readonly label: string | null;
}

/** 部分结果/失败的标注（逐状态一句，原因另起）。 */
export const PARTIAL_RESULT_LABELS: Readonly<Record<PartialResultView["state"], string | null>> = {
  completed: null,
  partial: "输出未完成：已收到的内容已保留，不得当作完整回复。",
  failed: "本轮失败：已收到的部分内容已保留。",
  cancelled: "本轮已停止：已收到的部分内容已保留。",
};

/** 不可用媒体的说明（显式，不画空占位）。 */
export const MEDIA_UNAVAILABLE_LABELS: Readonly<Record<Exclude<MediaAvailabilityView, "available">, string>> = {
  expired: "内容已超出保留期或已随消息清理，无法再读取。",
  not_fetched: "只有外部资源地址，未自动获取；不会因展示而发起网络请求。",
  unsupported: "当前客户端或 Agent 能力不支持这一类内容。",
};

/**
 * 一条附件媒体的读取/呈现状态。
 *
 * `ready` 只带对象 URL 与元数据，字节不进入公开状态；`loading` 与 `failed` 都是显式状态，
 * 不用空 URL 或空字节冒充内容。对象 URL 由平台端口创建，关闭/替换时立即撤销。
 */
export type AttachmentMediaView =
  | { readonly kind: "loading"; readonly sessionId: string }
  | {
      readonly kind: "ready";
      readonly sessionId: string;
      readonly url: string;
      readonly mimeType: string;
      readonly byteLength: string;
    }
  | { readonly kind: "failed"; readonly sessionId: string; readonly detail: string };

function availabilityOf(value: unknown): MediaAvailabilityView {
  return value === "available" || value === "expired" || value === "not_fetched" || value === "unsupported"
    ? value
    : "unsupported";
}

/**
 * 把 `AgentContentBlock[]` 投影成内容视图。
 *
 * 媒体块保持到达顺序；文本块拼接时用换行分隔，空文本块不产生空行。未知块只可能来自未来的
 * ACP 内容：已由协议层归成 `unsupported`（带原始类型），这里原样保留为不可用说明。
 */
export function buildAgentMessageContent(blocks: readonly AgentContentBlock[]): AgentMessageContent {
  const texts: string[] = [];
  const media: MediaBlockView[] = [];
  for (const block of blocks) {
    switch (block.type) {
      case "text":
        if (block.text.length > 0) texts.push(block.text);
        break;
      case "image_ref":
      case "audio_ref":
        media.push({
          kind: block.type === "image_ref" ? "image" : "audio",
          mimeType: block.mimeType,
          byteLength: block.byteLength,
          availability: availabilityOf(block.displayState),
          attachmentId: block.attachmentId ?? null,
        });
        break;
      case "file_ref":
        media.push({
          kind: "file",
          attachmentId: block.attachmentId,
          mimeType: block.mimeType,
          byteLength: block.byteLength,
          displayName: block.displayName,
          availability: availabilityOf(block.displayState),
        });
        break;
      case "resource_ref":
        media.push({
          kind: "resource",
          uri: block.uri,
          name: block.name,
          availability: availabilityOf(block.displayState),
        });
        break;
      case "unsupported":
        media.push({ kind: "unsupported", originalType: block.originalType });
        break;
    }
  }
  return { text: texts.join("\n"), media, hasStructuredContent: media.length > 0 };
}

function stringField(value: object, key: string): string | null {
  const field: unknown = Reflect.get(value, key);
  return typeof field === "string" && field.length > 0 ? field : null;
}

function nullableStringField(value: object, key: string): string | null | undefined {
  const field: unknown = Reflect.get(value, key);
  return field === null ? null : typeof field === "string" ? field : undefined;
}

/**
 * `agent.message.delta.block` → 媒体块。
 *
 * 增量里的块来自 wire，形状可能不完整；本函数逐字段确认后**只**产出可呈现的媒体块，
 * 不把半个对象当成合法内容。图片/音频的 `attachmentId` 只在确认是字符串时保留。
 */
export function mediaBlocksOfDelta(value: unknown): readonly MediaBlockView[] {
  if (typeof value !== "object" || value === null || !("type" in value)) return [];
  const type = value["type"];
  if (type === "image_ref" || type === "audio_ref") {
    const mimeType = stringField(value, "mimeType");
    if (mimeType === null) return [];
    const byteLength = nullableStringField(value, "byteLength") ?? null;
    const attachmentId = nullableStringField(value, "attachmentId");
    return [
      {
        kind: type === "image_ref" ? "image" : "audio",
        mimeType,
        byteLength,
        availability: availabilityOf(("displayState" in value ? value["displayState"] : undefined)),
        attachmentId: attachmentId ?? null,
      },
    ];
  }
  if (type === "file_ref") {
    const attachmentId = stringField(value, "attachmentId");
    const mimeType = stringField(value, "mimeType");
    const byteLength = stringField(value, "byteLength");
    if (attachmentId === null || mimeType === null || byteLength === null) return [];
    return [
      {
        kind: "file",
        attachmentId,
        mimeType,
        byteLength,
        displayName: nullableStringField(value, "displayName") ?? null,
        availability: availabilityOf(("displayState" in value ? value["displayState"] : undefined)),
      },
    ];
  }
  if (type === "resource_ref") {
    const uri = stringField(value, "uri");
    if (uri === null) return [];
    return [
      {
        kind: "resource",
        uri,
        name: nullableStringField(value, "name") ?? null,
        availability: availabilityOf(("displayState" in value ? value["displayState"] : undefined)),
      },
    ];
  }
  if (type === "unsupported") {
    const originalType = stringField(value, "originalType");
    if (originalType === null) return [];
    return [{ kind: "unsupported", originalType }];
  }
  return [];
}
