import "../../src/styles.css";
import { ViewContentStore } from "../../src/views/content-store";
import { createStructuredTimeline, type StructuredTimeline } from "../../src/views/timeline";
import { STREAM, SEGMENT, record, ref, item, evidence } from "../support/timeline-fixtures";

const host = document.querySelector<HTMLDivElement>("#timeline-host");
if (host === null) throw new Error("Fixture host missing");
let store: ViewContentStore | null = null;
let timeline: StructuredTimeline | null = null;
const initial = '{"toolCallId":"T","title":"读取文件","status":"in_progress","content":[{"type":"content","content":{"type":"text","text":"正在检查目录\\n保留原始位置。"}}],"rawInput":{"path":"file:///private/synthetic.txt"},"huge":1234567890123456789012345678901234567890}';
const completed = '{"toolCallId":"T","status":"completed","rawOutput":[{"type":"text","text":"读取完成。"}],"unknown":{"z":1,"a":2,"z":3}}';
const content = '[{"type":"content","content":{"type":"text","text":"正在检查目录\\n保留原始位置。"}}]';
function baseFields() {
  return {
    title: { knowledge: "known", raw: ref(1n, initial, '"读取文件"') },
    content: { knowledge: "known", raw: ref(1n, initial, content) },
    rawInput: { knowledge: "known", raw: ref(1n, initial, '{"path":"file:///private/synthetic.txt"}') },
  };
}
function reset(): void {
  timeline?.dispose(); store?.dispose();
  store = new ViewContentStore(STREAM);
  store.accept(record("original_raw", 1n, initial));
  store.accept(item(0n, 1n, 1n, 1n, { ...baseFields(),
    status: { knowledge: "known", enumCode: 1, raw: ref(1n, initial, '"in_progress"') },
  }));
  for (const [anchor, text] of [[1n, "后续消息 A：工具仍在原位置。"], [2n, "后续消息 B：保留文字的来源顺序。"]] as const) {
    const source = JSON.stringify({ content: { type: "text", text } });
    store.accept(record("original_raw", anchor + 1n, source));
    store.accept(item(anchor, anchor + 1n, anchor + 1n, anchor + 1n,
      { content: { knowledge: "known", raw: ref(anchor + 1n, source, JSON.stringify({ type: "text", text })) } }, "message"));
  }
  timeline = createStructuredTimeline(document, store, SEGMENT);
  host?.append(timeline.element);
}
function advance(): void {
  if (store === null || timeline === null) throw new Error("Not mounted");
  store.accept(record("original_raw", 4n, completed));
  store.accept(item(0n, 1n, 10n, 4n, { ...baseFields(),
    status: { knowledge: "known", enumCode: 2, raw: ref(4n, completed, '"completed"') },
    rawOutput: { knowledge: "known", raw: ref(4n, completed, '[{"type":"text","text":"读取完成。"}]') },
  }));
  timeline.refresh();
}
function unknowns(): void {
  reset();
  if (store === null || timeline === null) throw new Error("Not mounted");
  const source = '[{"type":"text","text":"安全文字 <img src=x onerror=alert(1)>\\n\\t下一行。","vendor":42},{"type":"image","data":"data:image/svg+xml,synthetic"},{"type":"text","ty\\u0070e":"image","text":"DUPLICATE_MUST_NOT_RENDER"},["NESTED_MUST_NOT_FLATTEN"],"控制符：\\u202e\\ud800。"]';
  store.accept(record("original_raw", 5n, source));
  store.accept(item(3n, 4n, 4n, 5n,
    { content: { knowledge: "uninterpreted", raw: ref(5n, source, source) } }, "unknownStructured"));
  timeline.refresh();
}
function boundaries(): void {
  reset();
  if (store === null || timeline === null) throw new Error("Not mounted");
  const title = JSON.stringify("标题\n\t\ud800");
  const raw = `{"title":${title},"output":"PRIMARY_OUTPUT"}`;
  const extra = '"EVIDENCE_NOT_PRIMARY"';
  store.accept(record("original_raw", 6n, raw));
  store.accept(record("original_raw", 7n, extra));
  store.accept(item(0n, 1n, 20n, 6n, {
    title: { knowledge: "known", raw: ref(6n, raw, title) },
    rawOutput: { knowledge: "known", raw: ref(6n, raw, '"PRIMARY_OUTPUT"'), evidenceCount: "1" },
  }));
  store.accept(evidence(0n, 20n, 0n, ref(7n, extra, extra), "rawOutput"));
  timeline.refresh();
}
const CHUNKS = ['{"type":"text","text":"Hel"}', '"l"', '[{"type":"text","text":"o"}]'] as const;
function chunks(kind: "message" | "tool" = "message", gap = false): void {
  reset();
  if (store === null || timeline === null) throw new Error("Not mounted");
  for (const [index, source] of CHUNKS.entries()) store.accept(record("original_raw", 8n + BigInt(index), source));
  store.accept(item(3n, 4n, 30n, 10n, {
    content: { knowledge: "partial", raw: ref(10n, CHUNKS[2], CHUNKS[2], true), evidenceCount: "3" },
  }, kind));
  for (const [index, source] of CHUNKS.entries()) {
    if (gap && index === 1) continue;
    store.accept(evidence(3n, 30n, BigInt(index), ref(8n + BigInt(index), source, source, index !== 0)));
  }
  timeline.refresh();
}
function fillGap(): void {
  if (store === null || timeline === null) throw new Error("Not mounted");
  store.accept(evidence(3n, 30n, 1n, ref(9n, CHUNKS[1], CHUNKS[1], true)));
  timeline.refresh();
}
document.querySelector("#late-completion")?.addEventListener("click", advance);
document.querySelector("#theme")?.addEventListener("click", () => {
  document.documentElement.dataset.theme = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
});
const api = Object.freeze({ reset, advance, unknowns, boundaries, chunks, fillGap, refresh: () => timeline?.refresh(),
  dispose: () => { timeline?.dispose(); timeline = null; store?.dispose(); store = null; } });
(window as Window & { timelineDevelopment?: typeof api }).timelineDevelopment = api;
reset();
