import { ManagementRequestError, type ManagementConnection } from "../management/client";
import { HistoryPageView, type HistoryDirection } from "../management/history-page-view";
import { historyTurns, type HistoryPage, type HistoryRequest } from "../management/session-protocol/history";
import type { SessionViewOpened } from "../management/session-protocol";
import { SessionViewDisplay, type ViewDisplayTransport } from "../management/session-view-display";
import type { ViewDataFrame } from "../views/binary";
import { managementErrorMessage } from "./error-formatting";

interface Port {
  readHistoryPage: ManagementConnection["readHistoryPage"];
  closeSessionView(streamId: string): Promise<unknown>;
  detachSessionStream(streamId: string): void;
}
interface HistoryRefresh {
  open(): Promise<SessionViewOpened>;
  retrySync(): void;
  promote(opened: SessionViewOpened, display: SessionViewDisplay): void;
}

export class HistoryPageController {
  readonly view: HistoryPageView;
  readonly element: HTMLElement;
  #attempt = 0;
  #active = true;
  #page?: HistoryPage;
  #display?: SessionViewDisplay;
  #child?: HistoryPageController;
  #owned = false;
  #closed = false;
  #invalid = false;
  #staging?: SessionViewDisplay;
  constructor(private doc: Document, private target: HistoryRequest["target"], private port: Port,
    private transport: ViewDisplayTransport, private child?: string,
    private reload?: (turns?: string) => void, private current = () => true, private retireInitial?: () => void, private refreshLatest?: HistoryRefresh) {
    this.view = new HistoryPageView(doc, direction => void this.navigate(direction), () => void this.first());
    this.view.refresh.addEventListener("click", () => void this.refresh());
    this.element = this.view.element;
    if (child === undefined) this.childControls();
  }

  private childControls(): void {
    const label = this.doc.createElement("label");
    label.textContent = "子会话原生标识";
    const id = this.doc.createElement("input");
    label.append(id);
    const toggle = this.doc.createElement("button");
    toggle.type = "button";
    const host = this.doc.createElement("div");
    const update = () => {
      toggle.textContent = this.#child ? "折叠子会话历史" : "展开子会话历史";
      toggle.setAttribute("aria-expanded", String(!!this.#child));
      id.disabled = !!this.#child;
    };
    toggle.addEventListener("click", () => {
      if (!this.#active || !this.current()) return;
      if (this.#child) {
        this.#child.dispose();
        this.#child = undefined;
      } else if (id.value.length) {
        this.#child = new HistoryPageController(this.doc, this.target, this.port, this.transport, id.value, undefined, this.current);
        host.append(this.#child.element);
        void this.#child.first();
      } else {
        this.view.notice.textContent = "请输入已记录的子会话原生标识。";
        id.focus();
      }
      update();
    });
    update();
    this.element.append(label, toggle, host);
  }

  attach(opened: SessionViewOpened, display: SessionViewDisplay): void {
    if (!opened.page) throw new Error("缺少历史页信息");
    this.#page = opened.page;
    this.#display = display;
    this.view.show(opened.page, display.element);
  }

  invalidate(id: string): void {
    if (!this.#active || !this.current()) return;
    if (id === this.#staging?.streamId) {
      this.refreshFailed("刷新期间历史再次更新。");
      return;
    }
    if (this.#invalid || this.#display?.streamId !== id || !this.#page?.liveTail) return;
    ++this.#attempt;
    this.#invalid = true;
    this.view.invalidated();
  }

  private discardStaging(): void {
    if (!this.#staging) return;
    this.close(this.#staging.streamId);
    this.#staging.dispose();
    this.#staging = undefined;
  }

  private refreshFailed(reason: string): void {
    ++this.#attempt;
    this.discardStaging();
    this.view.refresh.disabled = false;
    this.view.invalidated(reason);
  }

  private async refresh(): Promise<void> {
    const refresh = this.refreshLatest;
    if (!this.#invalid || !this.#active || !this.current() || !refresh || this.view.refresh.disabled) return;
    const attempt = ++this.#attempt;
    const current = () => this.#active && this.current() && attempt === this.#attempt;
    const failed = (reason: string) => {
      if (!current() || !this.#invalid) return;
      this.refreshFailed(reason);
    };
    this.view.refresh.disabled = true;
    try {
      const opened = await refresh.open();
      if (!current()) { this.close(opened.streamId); return; }
      if (!opened.page) { this.close(opened.streamId); throw new Error("节点未返回分页元数据"); }
      let display: SessionViewDisplay;
      try {
        display = new SessionViewDisplay(this.doc, opened, {
          ...this.transport,
          turnStarted: turn => this.turnStarted(turn),
          readFailed: failed,
          retrySyncRequested: () => {
            if (this.#display === display && this.#active && this.current() && !this.#invalid) refresh.retrySync();
          },
          reopenRequested: full => {
            if (this.#display === display && this.#active && this.current()) this.transport.reopenRequested?.(full);
          },
          snapshotReady: () => {
            if (!current() || this.#staging !== display) return;
            this.#staging = undefined;
            if (!this.#owned) this.retireInitial?.();
            this.release();
            this.#closed = false;
            this.#invalid = false;
            this.view.refresh.disabled = false;
            this.attach(opened, display);
            refresh.promote(opened, display);
          },
        });
      } catch (error) { this.close(opened.streamId); throw error; }
      this.#staging = display;
      display.start();
    } catch (error) { failed(managementErrorMessage(error)); }
  }

  turnStarted(turn: string): void {
    if (!this.#page?.liveTail || this.#invalid) return;
    this.#page = { ...this.#page, lastTurn: turn, totalTurns: turn, firstTurn: this.#page.firstTurn ?? turn };
    this.view.describe(this.#page);
  }

  first(): Promise<void> { return this.request({ kind: "first" }); }

  async navigate(direction: HistoryDirection): Promise<void> {
    const page = this.#page;
    if (!page) return;
    const cursor = direction === "older" ? page.olderCursor : page.newerCursor;
    const boundary = direction === "older" ? page.firstTurn : page.lastTurn;
    if (cursor === null || boundary === null) return;
    const coordinate = BigInt(boundary) + (direction === "older" ? -1n : 1n);
    await this.request({ kind: "continue", cursor, direction }, coordinate.toString());
  }

  private async request(mode: HistoryRequest["mode"], coordinate?: string): Promise<void> {
    if (!this.#active || this.#invalid) return;
    const attempt = ++this.#attempt;
    let turns: string | undefined;
    try { if (this.view.turns.value) turns = historyTurns(this.view.turns.value); }
    catch { this.view.notice.textContent = "每页轮数须为 1 至 50 的整数。"; return; }
    if (!this.current()) return;
    if (mode.kind === "first" && this.reload) { this.reload(turns); return; }
    const request = { target: this.target, ...(turns ? { turns } : {}), ...(this.child ? { child: this.child } : {}) };
    try {
      let opened: SessionViewOpened;
      try { opened = await this.port.readHistoryPage({ ...request, mode }); }
      catch (error) {
        if (attempt !== this.#attempt || !this.current()) return;
        if (!(error instanceof ManagementRequestError && error.code === "stale_cursor") ||
            mode.kind !== "continue" || !coordinate) throw error;
        opened = await this.port.readHistoryPage({ ...request, mode: { kind: "locate", direction: mode.direction, fromTurn: coordinate } });
      }
      if (attempt !== this.#attempt || !this.current()) {
        this.close(opened.streamId);
        return;
      }
      let display: SessionViewDisplay | undefined;
      try {
        if (!opened.page) throw new Error("节点未返回分页元数据");
        display = new SessionViewDisplay(this.doc, opened, { credit: this.transport.credit, detach: this.transport.detach });
        display.start();
      } catch (error) {
        this.close(opened.streamId);
        display?.dispose();
        throw error;
      }
      if (!this.#owned && this.#display) this.retireInitial?.();
      this.release();
      this.#owned = true;
      this.#closed = false;
      this.attach(opened, display);
    } catch (error) {
      if (attempt !== this.#attempt || !this.current()) return;
      this.element.dataset.stale = "true";
      const code = error instanceof ManagementRequestError ? error.code : null;
      if (this.child && code === "invalid_target") this.release();
      this.view.notice.textContent = code === "not_found"
        ? "目标轮次在最新原生历史中已不存在，当前显示旧视图。"
        : managementErrorMessage(error);
    }
  }
  private close(streamId: string): void {
    try { this.port.detachSessionStream(streamId); } catch { /* disconnected */ }
    void this.port.closeSessionView(streamId).catch(() => undefined);
  }
  private release(): void {
    if (this.#owned && this.#display) {
      if (!this.#closed) this.close(this.#display.streamId);
      this.#display.dispose();
    }
    this.#display = undefined;
    this.#owned = false;
  }
  accept(frame: ViewDataFrame): boolean {
    if (this.#staging?.streamId === frame.streamId) {
      if (this.current()) this.#staging.accept(frame);
      return true;
    }
    if (this.#invalid && frame.streamId === this.#display?.streamId) return true;
    if (this.#child?.accept(frame)) return true;
    if (!this.#active || !this.#owned || frame.streamId !== this.#display?.streamId) return false;
    this.#display.accept(frame);
    return true;
  }
  suspend(): void {
    this.#active = false;
    this.discardStaging();
    this.view.refresh.hidden = true;
    ++this.#attempt;
    this.element.dataset.stale = "true";
    this.view.notice.textContent = "连接已中断，当前显示旧视图。";
    this.#child?.dispose();
    this.#child = undefined;
    this.#display?.suspend();
    if (this.#owned && this.#display && !this.#closed) {
      this.close(this.#display.streamId);
      this.#closed = true;
    }
  }
  dispose(): void {
    this.suspend();
    this.release();
    this.element.remove();
  }
}
