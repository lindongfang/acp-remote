import { RawJsonDocument } from "../views/raw-json";
import { ViewDecodeError } from "../views/errors";
import { requireSessionId, requireSessionU64 } from "./session-protocol";

export interface ViewStatusRecord {
  readonly document: RawJsonDocument;
  readonly segmentId: string | null;
  readonly summary: string;
  readonly requiresReopen: boolean;
}

const SEGMENT_FIELDS = [
  "type", "phase", "segmentId", "segmentViewEpoch", "readEpoch", "origin",
  "readMode", "freshness", "join", "sourceBindingId", "sourceId",
  "beginSequence", "throughSequence", "failure", "projection",
  "completionResponseSequence", "open", "readContext", "relation", "limitation",
] as const;
const CRITICAL = new Set<string>([
  ...SEGMENT_FIELDS, "previousStreamId", "streamId", "viewEpoch", "snapshotRevision",
]);
function invalid(): never { throw new ViewDecodeError("invalid_record"); }

/** Platform metadata is inspected without converting native or unknown JSON to an execution object. */
export function readSessionViewStatus(bytes: Uint8Array): ViewStatusRecord {
  const document = RawJsonDocument.parse(bytes);
  if (document.tokens[0]?.kind !== "object") invalid();
  const members = new Map<string, number>();
  let key: string | null = null;
  for (let index = 1; index < document.tokens.length; ++index) {
    const token = document.tokens[index];
    if (token?.parent !== 0) continue;
    if (token.role === "key") {
      key = document.stringValue(index);
      if (CRITICAL.has(key) && members.has(key)) invalid();
    } else {
      if (key === null) invalid();
      members.set(key, index);
      key = null;
    }
  }
  function value(name: string): number {
    const index = members.get(name);
    if (index === undefined) invalid();
    return index;
  }
  function text(name: string): string {
    const index = value(name);
    if (document.tokens[index]?.kind !== "string") invalid();
    return document.stringValue(index);
  }
  function nullable(name: string, validate: (text: string) => unknown): void {
    const index = value(name);
    if (document.tokens[index]?.kind !== "null") validate(text(name));
  }
  function id(name: string): string {
    try { return requireSessionId(text(name)); } catch { return invalid(); }
  }
  function decimal(name: string): void {
    try { requireSessionU64(text(name)); } catch { invalid(); }
  }
  const typeIndex = members.get("type");
  const type = typeIndex !== undefined && document.tokens[typeIndex]?.kind === "string"
    ? document.stringValue(typeIndex) : null;
  if (type === "stream_rebound") {
    for (const name of ["previousStreamId", "streamId", "viewEpoch", "sourceBindingId", "sourceId"]) {
      if (members.has(name)) id(name);
    }
    if (members.has("readEpoch")) nullable("readEpoch", (v) => {
      try { requireSessionId(v); } catch { invalid(); }
    });
    if (members.has("snapshotRevision")) decimal("snapshotRevision");
    return Object.freeze({ document, segmentId: null, requiresReopen: true,
      summary: "收到读取流换代通知；当前页面未建立新流绑定，请重新打开共享视图。" });
  }
  if (type !== "view_segment") {
    return Object.freeze({ document, segmentId: null, requiresReopen: false,
      summary: "收到未识别的视图状态，原始结构已保留；未据此改变读取绑定或执行状态。" });
  }
  for (const name of SEGMENT_FIELDS) value(name);
  const segmentId = id("segmentId");
  if (id("segmentViewEpoch") !== segmentId) invalid();
  id("sourceBindingId"); id("sourceId");
  nullable("readEpoch", (v) => { try { requireSessionId(v); } catch { invalid(); } });
  for (const name of ["beginSequence", "throughSequence", "completionResponseSequence"]) {
    const index = value(name);
    if (document.tokens[index]?.kind !== "null") decimal(name);
  }
  for (const name of ["failure", "limitation"]) nullable(name, () => text(name));
  const open = document.tokens[value("open")]?.kind;
  if (open !== "true" && open !== "false") invalid();
  for (const name of ["phase", "origin", "readMode", "freshness", "join", "projection", "readContext", "relation"]) text(name);
  const descriptions: string[] = ["已保留来源分段状态；各段仅按节点提供的身份区分。"];
  if (text("readMode") === "CurrentOnly") descriptions.push("仅当前连接内容，历史不完整；缓存丢失后不保证能够补回。");
  if (text("freshness") === "Stale") descriptions.push("此分段是旧视图，尚未重新校验。");
  if (text("projection") === "ProjectionUnavailable") descriptions.push("结构投影不可用，可查看已保留的原始结构。");
  if (document.tokens[value("failure")]?.kind !== "null") descriptions.push("来源读取报告失败；当前内容不能冒充完整历史。");
  return Object.freeze({ document, segmentId, requiresReopen: false, summary: descriptions.join(" ") });
}
