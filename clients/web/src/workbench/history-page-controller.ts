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
  constructor(private doc: Document, private target: HistoryRequest["target"], private port: Port,
    private transport: ViewDisplayTransport, private child?: string,
    private reload?: (turns?: string) => void, private current = () => true, private retireInitial?: () => void) {
    this.view = new HistoryPageView(doc, direction => void this.navigate(direction), () => void this.first());
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

  turnStarted(turn: string): void {
    if (!this.#page?.liveTail || this.#owned) return;
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
    if (!this.#active) return;
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
    if (this.#child?.accept(frame)) return true;
    if (!this.#active || !this.#owned || frame.streamId !== this.#display?.streamId) return false;
    this.#display.accept(frame);
    return true;
  }
  suspend(): void {
    this.#active = false;
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
