import { parseStrictJsonObject } from "../protocol/jws";
import { turnBoundary } from "./session-protocol/history";
import { HistoryTurnGroups } from "./history-page-view";
import type { ViewDataFrame } from "../views/binary";
import { ViewDecodeError } from "../views/errors";
import { ViewRecordAssembler } from "../views/records";
import { ViewContentStore } from "../views/content-store";
import { decodeProjectionRecord } from "../views/projection";
import { createStructuredTimeline, type StructuredTimeline } from "../views/timeline";
import { createRawInspector, type RawInspector } from "../views/raw-inspector";
import { RawJsonDocument } from "../views/raw-json";
import { readSessionViewStatus } from "./session-view-status";
import { TranscriptCacheBanner } from "./transcript-cache-banner";
import type { SessionViewOpened, TranscriptCacheState } from "./session-protocol";

export interface ViewDisplayTransport {
  credit(streamId: string, packetSequence: bigint): void;
  detach(streamId: string): void;
  turnStarted?(turn: string): void;
  snapshotReady?(): void;
  readFailed?(reason: string): void;
  reopenRequested?(fullHistory?: boolean): void;
  retrySyncRequested?(): void | Promise<void>;
}

interface StagingRebindState {
  readonly streamId: string;
  readonly opened: SessionViewOpened;
  readonly assembler: ViewRecordAssembler;
  readonly store: ViewContentStore;
  readonly transport: ViewDisplayTransport;
  readonly segments: Set<string>;
  readonly segmentOrder: string[];
  readonly rawEntries: Array<{ readonly bytes: Uint8Array; readonly summary: string; readonly label: string }>;
  cacheState: TranscriptCacheState | null;
}

function limitationsCopy(opened: SessionViewOpened): string {
  const state = opened.status;
  return `读取模式 ${state.mode}；新鲜度 ${state.freshness}；结构 ${state.projection}；衔接 ${state.join}。` +
    (state.historyIncomplete ? " 历史不完整。" : "") +
    (!state.cacheRecoverable ? " 缓存丢失后无法保证补回。" : "");
}

/** One original stream's local display owner. No input, approval or native control port is available. */
export class SessionViewDisplay {
  readonly element: HTMLElement;
  #streamId: string;
  readonly #owner: Document;
  #assembler: ViewRecordAssembler;
  #store: ViewContentStore;
  #transport: ViewDisplayTransport;
  readonly #notice: HTMLParagraphElement;
  readonly #banner: TranscriptCacheBanner;
  #limitations: string;
  readonly #groups?: HistoryTurnGroups;
  readonly #paged: boolean;
  readonly #staticPage: boolean;
  #snapshotReady = false;
  readonly #segments: HTMLDivElement;
  readonly #statuses: HTMLDivElement;
  readonly #timelines = new Map<string, StructuredTimeline>();
  readonly #inspectors = new Set<RawInspector>();
  readonly #listeners = new AbortController();
  #accepting = true;
  #detached = false;
  #disposed = false;
  #started = false;
  #staging: StagingRebindState | null = null;

  get streamId(): string {
    return this.#streamId;
  }

  get detached(): boolean {
    return this.#detached;
  }

  constructor(owner: Document, opened: SessionViewOpened, transport: ViewDisplayTransport) {
    this.#owner = owner;
    this.#streamId = opened.streamId;
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
    this.#limitations = limitationsCopy(opened);
    this.#notice.textContent = this.#limitations;
    this.#statuses = owner.createElement("div");
    this.#banner = new TranscriptCacheBanner(owner, {
      onRetrySync: () => this.#transport.retrySyncRequested?.(),
    });
    this.#banner.update(opened.status);
    this.#segments = owner.createElement("div");
    this.#staticPage = opened.page?.liveTail === false;
    this.#paged = opened.page !== undefined;
    if (opened.page?.turnStatus === "provable") this.#groups = new HistoryTurnGroups(owner, this.#segments, opened.page);
    this.element.append(this.#notice, this.#statuses, this.#banner.element, this.#segments);
  }

  start(): void {
    if (this.#disposed || !this.#accepting || this.#started) return;
    this.#started = true;
    try { this.#transport.credit(this.#streamId, 0n); }
    catch (error) { this.suspend("无法开始共享视图读取；已停止该流，请重新打开。"); throw error; }
  }

  beginStagingRebind(opened: SessionViewOpened, transport: ViewDisplayTransport): boolean {
    if (this.#disposed || this.#paged || opened.page) return false;
    this.clearStagingOnly();
    const staging: StagingRebindState = {
      streamId: opened.streamId,
      opened,
      assembler: new ViewRecordAssembler(opened.streamId),
      store: new ViewContentStore(opened.streamId),
      transport,
      segments: new Set(),
      segmentOrder: [],
      rawEntries: [],
      cacheState: null,
    };
    this.#staging = staging;
    try {
      transport.credit(opened.streamId, 0n);
      return true;
    } catch {
      this.abortStagingRebind("无法开始重绑定同步，保留当前缓存视图。");
      return false;
    }
  }

  acceptStagedFrame(frame: ViewDataFrame): boolean {
    const staging = this.#staging;
    if (this.#disposed || staging === null || frame.streamId !== staging.streamId) return false;
    try {
      const completed = staging.assembler.accept(frame);
      if (completed !== null) {
        if (completed.kind === "status") {
          const status = readSessionViewStatus(completed.bytes());
          staging.rawEntries.push({
            bytes: completed.bytes(),
            summary: status.summary,
            label: "查看视图状态原始结构",
          });
          if (status.segmentId !== null && !staging.segments.has(status.segmentId)) {
            staging.segments.add(status.segmentId);
            staging.segmentOrder.push(status.segmentId);
          }
          if (status.cacheState !== undefined && status.cacheState !== null) {
            staging.cacheState = status.cacheState;
          }
          if (status.requiresReopen) {
            staging.transport.credit(staging.streamId, frame.packetSequence);
            this.abortStagingRebind(status.summary);
            return true;
          }
        } else if (completed.kind === "snapshot_end") {
          staging.transport.credit(staging.streamId, frame.packetSequence);
          this.commitStagingRebind(staging);
          return true;
        } else {
          const changed = staging.store.accept(completed);
          if (changed && completed.kind === "original_raw") {
            staging.rawEntries.push({
              bytes: completed.bytes(),
              summary: `原始记录 ${completed.recordId}；原生字段仅供查看，不获得管理权限。`,
              label: "查看记录原始结构",
            });
          }
          if (completed.kind === "item_projection") {
            const seg = decodeProjectionRecord(completed).segmentId;
            if (!staging.segments.has(seg)) {
              staging.segments.add(seg);
              staging.segmentOrder.push(seg);
            }
          }
        }
      }
      staging.transport.credit(staging.streamId, frame.packetSequence);
    } catch {
      this.abortStagingRebind("共享视图重绑定同步失败，保留此前缓存内容。");
    }
    return true;
  }

  abortStagingRebind(reason = "共享视图重绑定中断，保留此前缓存内容。"): void {
    const staging = this.#staging;
    if (staging === null) return;
    this.#staging = null;
    staging.assembler.seal();
    staging.store.dispose();
    try { staging.transport.detach(staging.streamId); } catch { /* Closed connection. */ }
    this.element.dataset.stale = "true";
    this.#notice.textContent = this.#limitations + " " + reason;
  }

  private clearStagingOnly(): void {
    const staging = this.#staging;
    if (staging === null) return;
    this.#staging = null;
    staging.assembler.seal();
    staging.store.dispose();
    try { staging.transport.detach(staging.streamId); } catch { /* Best effort. */ }
  }

  private commitStagingRebind(staging: StagingRebindState): void {
    this.#staging = null;
    const oldEntries = [...this.#timelines.entries()];
    const nextSegments = staging.segmentOrder;
    const rebound = new Map<string, StructuredTimeline>();

    const count = Math.min(oldEntries.length, nextSegments.length);
    for (let i = 0; i < count; i++) {
      const pair = oldEntries[i];
      const nextSeg = nextSegments[i];
      if (pair === undefined || nextSeg === undefined) continue;
      const [oldSeg, timeline] = pair;
      if (timeline.rebindGeneration !== undefined) {
        timeline.rebindGeneration(staging.store, nextSeg);
        timeline.element.setAttribute("aria-label", `来源分段 ${nextSeg}`);
        rebound.set(nextSeg, timeline);
      } else {
        timeline.dispose();
        this.#timelines.delete(oldSeg);
      }
    }
    for (let i = count; i < oldEntries.length; i++) {
      const pair = oldEntries[i];
      if (pair !== undefined) pair[1].dispose();
    }
    this.#timelines.clear();
    for (const [seg, tl] of rebound) this.#timelines.set(seg, tl);
    for (const seg of nextSegments) {
      if (!this.#timelines.has(seg)) this.timelineFor(seg, staging.store);
    }

    this.#assembler.seal();
    this.#store.dispose();
    this.#streamId = staging.opened.streamId;
    this.element.dataset.streamId = staging.opened.streamId;
    this.#assembler = staging.assembler;
    this.#store = staging.store;
    this.#transport = staging.transport;
    this.#accepting = true;
    this.#detached = false;
    this.#started = true;
    this.#limitations = limitationsCopy(staging.opened);
    this.#notice.textContent =
      this.#limitations + (this.#staticPage ? " 本页已接收。" : " 本次快照已接收，继续等待增量；快照结束不代表原生历史重放已完整。");
    this.#banner.setCallbacks({
      onRetrySync: () => this.#transport.retrySyncRequested?.(),
    });
    this.#banner.update(staging.cacheState ?? staging.opened.status);
    for (const raw of staging.rawEntries) {
      this.addRawContent(() => RawJsonDocument.parse(raw.bytes), raw.summary, raw.label);
    }
    if (typeof (this.element as { removeAttribute?: unknown }).removeAttribute === "function") {
      this.element.removeAttribute("data-stale");
    } else {
      delete this.element.dataset.stale;
    }
    staging.transport.snapshotReady?.();
  }

  accept(frame: ViewDataFrame): void {
    if (this.#disposed) return;
    if (this.#staging !== null && frame.streamId === this.#staging.streamId) {
      this.acceptStagedFrame(frame);
      return;
    }
    if (!this.#accepting || frame.streamId !== this.#streamId) return;
    try {
      const completed = this.#assembler.accept(frame);
      if (completed !== null) {
        if (completed.kind === "status") {
          const status = readSessionViewStatus(completed.bytes());
          this.addRawContent(() => status.document, status.summary, "查看视图状态原始结构");
          if (status.segmentId !== null) this.timeline(status.segmentId);
          if (status.cacheState !== undefined && status.cacheState !== null) {
            this.#banner.update(status.cacheState);
          }
          if (status.requiresReopen) {
            this.#transport.credit(this.#streamId, frame.packetSequence);
            this.suspend(status.summary);
            this.detach();
            this.#transport.readFailed?.(status.summary);
            this.#transport.reopenRequested?.(status.historyUnprovable);
            return;
          }
        } else if (completed.kind === "turn_start") {
          if (!this.#groups || this.#staticPage) throw new Error("unexpected_turn");
          const boundary = turnBoundary(parseStrictJsonObject(completed.bytes()));
          if (this.#groups.append(boundary)) this.#transport.turnStarted?.(boundary.turn);
        } else if (completed.kind === "snapshot_end") {
          this.#snapshotReady = true;
          this.#notice.textContent =
            this.#limitations + (this.#staticPage ? " 本页已接收。" : " 本次快照已接收，继续等待增量；快照结束不代表原生历史重放已完整。");
          this.#transport.credit(this.#streamId, frame.packetSequence);
          this.#transport.snapshotReady?.();
          return;
        } else {
          const changed = this.#store.accept(completed);
          if (changed && completed.kind === "original_raw") {
            this.addRawContent(
              () => RawJsonDocument.parse(completed.bytes()),
              `原始记录 ${completed.recordId}；原生字段仅供查看，不获得管理权限。`,
              "查看记录原始结构",
            );
          }
          if (completed.kind === "item_projection") {
            const record = decodeProjectionRecord(completed);
            if (record.type === "item_projection") this.timeline(record.segmentId, record.firstPosition);
          }
          for (const timeline of this.#timelines.values()) timeline.refresh();
        }
      }
      this.#transport.credit(this.#streamId, frame.packetSequence);
    } catch (error) {
      this.suspend("共享视图读取或展示准备失败，已保留此前收到的内容；未取消原生执行，请重新打开。");
      this.detach();
      this.#transport.readFailed?.(error instanceof ViewDecodeError
        ? `共享视图接收失败（${error.code}）` : "共享视图读取或展示准备失败");
    }
  }

  private timeline(segment: string, position?: bigint): void {
    if (!this.#groups) { this.timelineFor(segment, this.#store); return; }
    if (position === undefined) return;
    for (const group of this.#groups.groups) {
      if (position < group.start || (group.end !== undefined && position >= group.end)) continue;
      const key = segment + "/" + group.start;
      if (this.#timelines.has(key)) continue;
      const timeline = createStructuredTimeline(this.#owner, this.#store, segment,
        position => position >= group.start && (group.end === undefined || position < group.end));
      this.#timelines.set(key, timeline);
      group.host.append(timeline.element);
    }
  }

  private timelineFor(segment: string, store: ViewContentStore): StructuredTimeline {
    const previous = this.#timelines.get(segment);
    if (previous !== undefined) return previous;
    const timeline = createStructuredTimeline(this.#owner, store, segment);
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
    try { this.#transport.detach(this.#streamId); } catch { /* The closed connection cannot accept detach. */ }
  }

  suspend(reason = "连接已中断；以下为此前收到的旧视图，尚未重新校验。"): void {
    if (this.#disposed) return;
    this.#accepting = false;
    this.#assembler.seal();
    this.clearStagingOnly();
    this.#notice.textContent = this.#limitations + " " + reason + (this.#paged && !this.#snapshotReady ? " 此页仍有未加载部分。" : "");
    this.element.dataset.stale = "true";
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true; this.#accepting = false;
    this.#assembler.seal();
    this.clearStagingOnly();
    this.#listeners.abort();
    this.#banner.dispose();
    for (const inspector of this.#inspectors) inspector.dispose();
    this.#inspectors.clear();
    for (const timeline of this.#timelines.values()) timeline.dispose();
    this.#timelines.clear(); this.#store.dispose();
    this.element.replaceChildren(); this.element.remove();
  }
}
