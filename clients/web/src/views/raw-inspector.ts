import "./raw-inspector.css";
import { RawJsonDocument, type RawJsonKind, type RawJsonToken } from "./raw-json";
import {
  buildRawIndex, indexLiteral, literalWindow, shortLiteral, visibleRawText,
  LITERAL_WINDOW, MEMBER_WINDOW,
  type LiteralPaging, type RawIndex,
} from "./raw-inspector-data";

export interface RawInspector {
  readonly element: HTMLElement;
  dispose(): void;
}

interface Location {
  readonly token: number;
  readonly page: number;
  readonly depth: number;
  readonly parent: Location | null;
}
interface OpenLiteral {
  readonly token: number;
  readonly paging: LiteralPaging;
}

const TYPE_LABEL: Readonly<Record<RawJsonKind, string>> = {
  object: "对象", array: "数组", string: "字符串", number: "数字",
  true: "true", false: "false", null: "null",
};

class Inspector {
  #owner: Document | null;
  #raw: RawJsonDocument | null;
  #tokens: readonly RawJsonToken[] | null = null;
  #index: RawIndex | null = null;
  #root: HTMLElement | null = null;
  #heading: HTMLHeadingElement | null = null;
  #viewport: HTMLDivElement | null = null;
  #error: HTMLParagraphElement | null = null;
  #location: Location | null = null;
  #literal: OpenLiteral | null = null;
  #listeners: AbortController | null = null;
  #disposed = false;
  #rendered = false;

  constructor(owner: Document, raw: RawJsonDocument) {
    this.#owner = owner;
    this.#raw = raw;
  }

  initialize(): HTMLElement {
    if (!(this.#raw instanceof RawJsonDocument)) throw new Error("原始结构不可用");
    this.#tokens = this.#raw.tokens;
    this.#index = buildRawIndex(this.#tokens);
    const root = this.node("section", "acp-raw-inspector");
    this.#root = root;
    root.setAttribute("aria-label", "原始 JSON 结构");
    const heading = this.node("h3", "acp-raw-inspector__heading");
    heading.textContent = "原始 JSON 结构";
    heading.tabIndex = -1;
    this.#heading = heading;
    const notice = this.node("p", "acp-raw-inspector__notice");
    notice.textContent = "以下为不可信原始数据，仅检查结构。链接、附件和媒体访问尚未接入。不可见控制字符显示为码点标记，原始数据保留。";
    const error = this.node("p", "acp-raw-inspector__error");
    error.setAttribute("role", "status");
    error.hidden = true;
    this.#error = error;
    const viewport = this.node("div", "acp-raw-inspector__viewport");
    this.#viewport = viewport;
    root.append(heading, notice, error, viewport);
    this.render({ token: 0, page: 0, depth: 0, parent: null }, false);
    return root;
  }

  private node<K extends keyof HTMLElementTagNameMap>(
    tag: K, className: string,
  ): HTMLElementTagNameMap[K] {
    if (this.#owner === null) throw new Error("原始结构已关闭");
    const element = this.#owner.createElement(tag);
    element.className = className;
    return element;
  }

  private token(index: number): RawJsonToken {
    const token = this.#tokens?.[index];
    if (token === undefined) throw new Error("原始结构不可用");
    return token;
  }

  private button(
    label: string, signal: AbortSignal, action: () => void, disabled = false,
  ): HTMLButtonElement {
    const button = this.node("button", "acp-raw-inspector__button");
    button.type = "button";
    button.textContent = label;
    button.disabled = disabled;
    button.addEventListener("click", () => {
      if (this.#disposed || button.disabled) return;
      try { action(); } catch { this.showFailure(); }
    }, { signal });
    return button;
  }

  private showFailure(): void {
    if (this.#disposed || this.#error === null) return;
    this.#error.textContent = this.#rendered
      ? "本次展示失败，已保留上一次显示内容。可再次操作或关闭此查看器。"
      : "本次原始结构暂无法显示。可关闭此查看器后重新打开。";
    this.#error.hidden = false;
  }

  private render(location: Location, focus: boolean): void {
    if (this.#disposed) return;
    const listeners = new AbortController();
    try {
      if (this.#owner === null || this.#viewport === null) throw new Error("原始结构已关闭");
      const fragment = this.#owner.createDocumentFragment();
      const token = this.token(location.token);
      const context = this.node("p", "acp-raw-inspector__position");
      context.textContent = `第 ${location.depth} 层 · ${token.role === "key" ? "键 · " : ""}${TYPE_LABEL[token.kind]}`;
      fragment.append(context);
      const controls = this.node("div", "acp-raw-inspector__controls");
      controls.append(this.button("返回上层", listeners.signal, () => {
        const parent = this.#location?.parent;
        if (parent !== null && parent !== undefined) this.render(parent, true);
      }, location.parent === null));
      fragment.append(controls);
      let literal: OpenLiteral | null = null;
      if (token.kind === "object" || token.kind === "array") {
        this.renderMembers(fragment, controls, location, listeners.signal);
      } else {
        if (this.#raw === null) throw new Error("原始结构已关闭");
        literal = this.#literal?.token === location.token ? this.#literal : {
          token: location.token,
          paging: indexLiteral(this.#raw.tokenLexeme(location.token)),
        };
        this.renderLiteral(fragment, controls, location, literal.paging, listeners.signal);
      }
      this.#viewport.replaceChildren(fragment);
      this.#listeners?.abort();
      this.#listeners = listeners;
      this.#location = location;
      this.#literal = literal;
      this.#rendered = true;
      if (this.#error !== null) {
        this.#error.textContent = "";
        this.#error.hidden = true;
      }
      if (focus) this.#heading?.focus();
    } catch {
      listeners.abort();
      this.showFailure();
    }
  }

  private enter(token: number): void {
    const parent = this.#location;
    if (parent === null || this.#disposed) return;
    this.render({ token, page: 0, depth: parent.depth + 1, parent }, true);
  }

  private page(page: number): void {
    if (this.#location !== null && !this.#disposed) {
      this.render({ ...this.#location, page }, true);
    }
  }

  private pagingButtons(
    controls: HTMLElement, signal: AbortSignal, current: number, last: number,
    labels: readonly [string, string, string, string],
  ): void {
    controls.append(
      this.button(labels[0], signal, () => this.page(0), current === 0),
      this.button(labels[1], signal, () => this.page(current - 1), current === 0),
      this.button(labels[2], signal, () => this.page(current + 1), current === last),
      this.button(labels[3], signal, () => this.page(last), current === last),
    );
  }

  private renderMembers(
    fragment: DocumentFragment, controls: HTMLElement, location: Location, signal: AbortSignal,
  ): void {
    const members = this.#index?.children[location.token];
    const raw = this.#raw;
    if (members === undefined || raw === null) throw new Error("原始结构不可用");
    const last = Math.max(0, Math.ceil(members.length / MEMBER_WINDOW) - 1);
    const start = location.page * MEMBER_WINDOW;
    if (location.page < 0 || location.page > last) throw new Error("原始结构位置无效");
    this.pagingButtons(controls, signal, location.page, last, ["首页", "上一页", "下一页", "末页"]);
    const position = this.node("p", "acp-raw-inspector__position");
    position.textContent = members.length === 0 ? "空容器 · 0 个成员"
      : `成员 ${start + 1}–${Math.min(start + MEMBER_WINDOW, members.length)} / ${members.length}`;
    fragment.append(position);
    const list = this.node("ol", "acp-raw-inspector__members");
    list.setAttribute("aria-label", "直接成员");
    const end = Math.min(start + MEMBER_WINDOW, members.length);
    for (let index = start; index < end; ++index) {
      const member = members[index];
      if (member === undefined) throw new Error("原始结构不可用");
      const row = this.node("li", "acp-raw-inspector__member");
      const label = this.node("code", "acp-raw-inspector__key");
      if (member.key === null) label.textContent = `索引 ${index}`;
      else {
        const key = this.token(member.key);
        const bytes = key.endByte - key.startByte;
        // Large keys stay lazy; their complete literal has the same explicit viewer.
        label.textContent = bytes > 640 ? `长键 · 原文 ${bytes} 字节（查看键）`
          : shortLiteral(raw.tokenLexeme(member.key), 160);
      }
      const kind = this.node("span", "acp-raw-inspector__kind");
      kind.textContent = TYPE_LABEL[this.token(member.value).kind];
      const actions = this.node("div", "acp-raw-inspector__controls");
      const key = member.key;
      if (key !== null) actions.append(this.button("查看键", signal, () => this.enter(key)));
      actions.append(this.button("查看值", signal, () => this.enter(member.value)));
      row.append(label, kind, actions);
      list.append(row);
    }
    fragment.append(list);
  }

  private renderLiteral(
    fragment: DocumentFragment, controls: HTMLElement, location: Location,
    paging: LiteralPaging, signal: AbortSignal,
  ): void {
    const last = paging.starts.length - 1;
    this.pagingButtons(controls, signal, location.page, last, ["首段", "上一段", "下一段", "末段"]);
    const position = this.node("p", "acp-raw-inspector__position");
    const start = location.page * LITERAL_WINDOW;
    position.textContent = `原文字符 ${paging.codePoints === 0 ? 0 : start + 1}–${Math.min(start + LITERAL_WINDOW, paging.codePoints)} / ${paging.codePoints} · 保留 JSON 词法`;
    const text = this.node("pre", "acp-raw-inspector__literal");
    text.tabIndex = 0;
    text.setAttribute("aria-label", "原始词法文本");
    text.textContent = visibleRawText(literalWindow(paging, location.page));
    fragment.append(position, text);
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#listeners?.abort();
    this.#listeners = null;
    this.#raw = null;
    this.#tokens = null;
    this.#index = null;
    this.#location = null;
    this.#literal = null;
    const root = this.#root;
    this.#root = null;
    this.#heading = null;
    this.#viewport = null;
    this.#error = null;
    this.#owner = null;
    root?.replaceChildren();
    root?.remove();
  }
}

export function createRawInspector(owner: Document, raw: RawJsonDocument): RawInspector {
  const inspector = new Inspector(owner, raw);
  try {
    const element = inspector.initialize();
    return Object.freeze({ element, dispose: () => inspector.dispose() });
  } catch {
    try { inspector.dispose(); } catch { /* An unusable DOM cannot promise disposal. */ }
    const error = new Error("无法建立原始结构查看器");
    error.name = "RawInspectorError";
    throw error;
  }
}
