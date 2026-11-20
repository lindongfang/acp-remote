import type { HistoryPage } from "./session-protocol/history";

export type HistoryDirection = "older" | "newer";
export class HistoryPageView {
  readonly element: HTMLElement;
  readonly notice: HTMLElement;
  readonly body: HTMLElement;
  readonly turns: HTMLInputElement;
  readonly #bar: HTMLElement;
  readonly #older: HTMLButtonElement;
  readonly #newer: HTMLButtonElement;

  constructor(doc: Document, navigate: (direction: HistoryDirection) => void, reload: () => void) {
    this.element = doc.createElement("section");
    this.element.setAttribute("aria-label", "会话历史");
    this.#bar = doc.createElement("div");
    this.#bar.className = "history-controls";
    this.turns = doc.createElement("input");
    this.turns.inputMode = "numeric";
    this.turns.placeholder = "10";
    const label = doc.createElement("label");
    label.textContent = "每页轮数";
    label.append(this.turns);
    const button = (title: string, direction: HistoryDirection) => {
      const control = doc.createElement("button");
      control.type = "button";
      control.textContent = title;
      control.addEventListener("click", () => navigate(direction));
      return control;
    };
    this.#older = button("更早", "older");
    this.#newer = button("更新", "newer");
    const apply = doc.createElement("button");
    apply.type = "button";
    apply.textContent = "重新加载首页";
    apply.addEventListener("click", reload);
    this.#bar.append(label, apply, this.#older, this.#newer);
    this.#bar.hidden = true;
    this.notice = doc.createElement("p");
    this.notice.setAttribute("role", "status");
    this.body = doc.createElement("div");
    this.element.append(this.#bar, this.notice, this.body);
  }

  show(page: HistoryPage, content: HTMLElement): void {
    this.describe(page);
    this.element.removeAttribute("data-stale");
    this.body.replaceChildren(content);
  }

  describe(page: HistoryPage): void {
    this.#bar.hidden = page.turnStatus === "unprovable";
    this.#older.disabled = page.olderCursor === null;
    this.#newer.disabled = page.newerCursor === null;
    const messages: string[] = [];
    if (page.turnStatus === "unprovable") messages.push("当前历史无法证明轮次边界，已展示可读取的全部记录。");
    else if (page.firstTurn !== null) messages.push(`第 ${page.firstTurn}–${page.lastTurn} 轮`);
    if (page.source === "replay") messages.push("首次加载可能遍历全部历史。");
    if (page.relocated && page.continuity === "unproven") messages.push("已在最新同步代际中重新定位，与之前页面的连续性未经证明");
    this.notice.textContent = messages.join(" ");
  }
}

export class HistoryTurnGroups {
  readonly groups: { start: bigint; end?: bigint; host: HTMLElement }[] = [];
  #lastTurn = 0n;
  readonly #positions = new Map<string, bigint>();
  constructor(private doc: Document, private host: HTMLElement, page: HistoryPage) {
    if (page.prelude) this.add("前导记录", 0n);
    for (const boundary of page.turns) this.append(boundary);
  }
  append(boundary: { turn: string; firstPosition: string }): boolean {
    const turn = BigInt(boundary.turn);
    const start = BigInt(boundary.firstPosition);
    const existing = this.#positions.get(boundary.turn);
    if (existing === start) return false;
    if (turn <= this.#lastTurn || start < (this.groups.at(-1)?.start ?? 0n)) throw new Error("轮次边界顺序无效");
    this.#positions.set(boundary.turn, start);
    this.add(`第 ${turn} 轮`, start);
    this.#lastTurn = turn;
    return true;
  }
  private add(label: string, start: bigint): void {
    const previous = this.groups.at(-1);
    if (previous) previous.end = start;
    const host = this.doc.createElement("section");
    const heading = this.doc.createElement("h3");
    heading.textContent = label;
    host.append(heading);
    this.host.append(host);
    this.groups.push({ start, host });
  }
}
