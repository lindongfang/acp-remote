import type { ViewDataFrame } from "../views/binary";
import { ViewRecordAssembler } from "../views/records";
import { ViewContentStore } from "../views/content-store";
import { decodeProjectionRecord } from "../views/projection";
import { createStructuredTimeline, type StructuredTimeline } from "../views/timeline";
import { createRawInspector, type RawInspector } from "../views/raw-inspector";
import { RawJsonDocument } from "../views/raw-json";
import { readSessionViewStatus } from "./session-view-status";
import type { SessionViewOpened } from "./session-protocol";

export interface ViewDisplayTransport {
  credit(streamId: string, packetSequence: bigint): void;
  detach(streamId: string): void;
  snapshotReady?(): void;
  reopenRequested?(): void;
}

/** One original stream's local display owner. No input, approval or native control port is available. */
export class SessionViewDisplay {
  readonly element: HTMLElement;
  readonly streamId: string;
  readonly #owner: Document;
  readonly #assembler: ViewRecordAssembler;
  readonly #store: ViewContentStore;
  readonly #transport: ViewDisplayTransport;
  readonly #notice: HTMLParagraphElement;
  readonly #limitations: string;
  readonly #segments: HTMLDivElement;
  readonly #statuses: HTMLDivElement;
  readonly #timelines = new Map<string, StructuredTimeline>();
  readonly #inspectors = new Set<RawInspector>();
  readonly #listeners = new AbortController();
  #accepting = true;
  #detached = false;
  #disposed = false;
  #started = false;

  get detached(): boolean {
    return this.#detached;
  }

  constructor(owner: Document, opened: SessionViewOpened, transport: ViewDisplayTransport) {
    this.#owner = owner;
    this.streamId = opened.streamId;
    this.#assembler = new ViewRecordAssembler(opened.streamId);
    this.#store = new ViewContentStore(opened.streamId);
    this.#transport = transport;
    this.element = owner.createElement("section");
    this.element.setAttribute("aria-label", "共享会话视图");
    this.element.dataset.streamId = opened.streamId;
    this.#notice = owner.createElement("p");
    this.#notice.className = "field-help";
    this.#notice.setAttribute("role", "status");
    this.#notice.setAttribute("aria-atomic", "true");
    const state = opened.status;
    this.#limitations = `读取模式 ${state.mode}；新鲜度 ${state.freshness}；结构 ${state.projection}；衔接 ${state.join}。` +
      (state.historyIncomplete ? " 历史不完整。" : "") +
      (!state.cacheRecoverable ? " 缓存丢失后无法保证补回。" : "");
    this.#notice.textContent = this.#limitations;
    this.#statuses = owner.createElement("div");
    this.#segments = owner.createElement("div");
    this.element.append(this.#notice, this.#statuses, this.#segments);
  }

  start(): void {
    if (this.#disposed || !this.#accepting || this.#started) return;
    this.#started = true;
    try { this.#transport.credit(this.streamId, 0n); }
    catch (error) { this.suspend("无法开始共享视图读取；已停止该流，请重新打开。"); throw error; }
  }

  accept(frame: ViewDataFrame): void {
    if (this.#disposed || !this.#accepting || frame.streamId !== this.streamId) return;
    try {
      const completed = this.#assembler.accept(frame);
      if (completed !== null) {
        if (completed.kind === "status") {
          const status = readSessionViewStatus(completed.bytes());
          this.addRawContent(() => status.document, status.summary, "查看视图状态原始结构");
          if (status.segmentId !== null) this.timeline(status.segmentId);
          if (status.requiresReopen) {
            this.#transport.credit(this.streamId, frame.packetSequence);
            this.suspend(status.summary);
            this.detach();
            this.#transport.reopenRequested?.();
            return;
          }
        } else if (completed.kind === "snapshot_end") {
          this.#notice.textContent = this.#limitations + " 本次快照已接收，继续等待增量；快照结束不代表原生历史重放已完整。";
          this.#transport.snapshotReady?.();
        } else {
          const changed = this.#store.accept(completed);
          if (changed && completed.kind === "original_raw") {
            this.addRawContent(() => RawJsonDocument.parse(completed.bytes()),
              `原始记录 ${completed.recordId}；原生字段仅供查看，不获得管理权限。`, "查看记录原始结构");
          }
          if (completed.kind === "item_projection") this.timeline(decodeProjectionRecord(completed).segmentId);
          for (const timeline of this.#timelines.values()) timeline.refresh();
        }
      }
      this.#transport.credit(this.streamId, frame.packetSequence);
    } catch {
      this.suspend("共享视图读取或展示准备失败，已保留此前收到的内容；未取消原生执行，请重新打开。");
      this.detach();
    }
  }

  private timeline(segment: string): StructuredTimeline {
    const previous = this.#timelines.get(segment);
    if (previous !== undefined) return previous;
    const timeline = createStructuredTimeline(this.#owner, this.#store, segment);
    timeline.element.setAttribute("aria-label", `来源分段 ${segment}`);
    this.#timelines.set(segment, timeline);
    this.#segments.append(timeline.element);
    return timeline;
  }

  private addRawContent(read: () => RawJsonDocument, summary: string, label: string): void {
    const section = this.#owner.createElement("section");
    const text = this.#owner.createElement("p");
    text.textContent = summary;
    const toggle = this.#owner.createElement("button");
    toggle.type = "button"; toggle.className = "secondary-button";
    toggle.textContent = label;
    toggle.setAttribute("aria-expanded", "false");
    const host = this.#owner.createElement("div");
    let inspector: RawInspector | null = null;
    toggle.addEventListener("click", () => {
      if (inspector !== null) {
        inspector.dispose(); this.#inspectors.delete(inspector); inspector = null;
        toggle.setAttribute("aria-expanded", "false");
        return;
      }
      try {
        inspector = createRawInspector(this.#owner, read());
        host.append(inspector.element); this.#inspectors.add(inspector);
        toggle.setAttribute("aria-expanded", "true");
      } catch { text.textContent = "无法显示此记录原始结构；已保留其它内容。"; }
    }, { signal: this.#listeners.signal });
    section.append(text, toggle, host); this.#statuses.append(section);
  }

  private detach(): void {
    if (this.#detached) return;
    this.#detached = true;
    try { this.#transport.detach(this.streamId); } catch { /* The closed connection cannot accept detach. */ }
  }

  suspend(reason = "连接已中断；以下为此前收到的旧视图，尚未重新校验。"): void {
    if (this.#disposed) return;
    this.#accepting = false;
    this.#assembler.seal();
    this.#notice.textContent = this.#limitations + " " + reason;
    this.element.dataset.stale = "true";
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true; this.#accepting = false;
    this.#assembler.seal(); this.#listeners.abort();
    for (const inspector of this.#inspectors) inspector.dispose();
    this.#inspectors.clear();
    for (const timeline of this.#timelines.values()) timeline.dispose();
    this.#timelines.clear(); this.#store.dispose();
    this.element.replaceChildren(); this.element.remove();
  }
}
