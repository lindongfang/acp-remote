import "./timeline.css";
import { ViewContentStore, type FieldMaterial, type RawValue } from "./content-store";
import { PROJECTION_FIELDS, type ProjectedItem } from "./projection";
import { buildRawIndex, visibleRawText, type RawIndex } from "./raw-inspector-data";
import { createRawInspector, type RawInspector } from "./raw-inspector";
import type { RawJsonDocument } from "./raw-json";

export interface StructuredTimeline {
  readonly element: HTMLElement;
  refresh(): void;
  rebindGeneration?(store: ViewContentStore, segmentId: string): void;
  dispose(): void;
}
type TextPart =
  | { readonly kind: "text"; readonly text: string; readonly extra: boolean }
  | { readonly kind: "raw" };
interface SourceDocument {
  readonly document: RawJsonDocument;
  readonly label: string;
}
interface Card {
  readonly element: HTMLLIElement;
  readonly heading: HTMLHeadingElement;
  readonly status: HTMLSpanElement;
  readonly body: HTMLDivElement;
  readonly error: HTMLParagraphElement;
  readonly toggle: HTMLButtonElement;
  readonly update: HTMLButtonElement;
  readonly select: HTMLSelectElement;
  readonly choose: HTMLButtonElement;
  readonly rawNotice: HTMLParagraphElement;
  readonly rawHost: HTMLDivElement;
  readonly listeners: AbortController;
  item: ProjectedItem;
  bodyKey: string;
  documents: Map<string, SourceDocument>;
  latest: RawJsonDocument | null;
  inspector: RawInspector | null;
  opened: RawJsonDocument | null;
}

const STATUS = [
  ["pending", "等待中"], ["in_progress", "执行中"], ["completed", "已完成"],
  ["failed", "失败"], ["cancelled", "已取消"],
] as const;
const KIND = { tool: "工具调用", message: "消息", state: "状态记录", unknownStructured: "未解析条目" } as const;
let controlId = 0n;

function visibleText(value: string): string {
  return visibleRawText(value).replace(/[\uD800-\uDFFF]/gu, (character) =>
    `⟦U+${character.charCodeAt(0).toString(16).toUpperCase().padStart(4, "0")}⟧`);
}
function bodyText(value: string): string {
  return value.split(/(\n|\t)/).map((part) => part === "\n" || part === "\t" ? part : visibleText(part)).join("");
}
function string(value: RawValue | null): string | null {
  if (value === null || value.document.tokens[value.token]?.kind !== "string") return null;
  return value.document.stringValue(value.token);
}
function textObject(value: RawValue, index: RawIndex, wrapper: boolean): Extract<TextPart, { kind: "text" }> | null {
  if (value.document.tokens[value.token]?.kind !== "object") return null;
  const members = index.children[value.token];
  if (members === undefined) return null;
  const protectedKeys = new Map<string, number>();
  for (const member of members) {
    if (member.key === null) return null;
    const key = value.document.stringValue(member.key);
    if (key === "type" || key === "text" || key === "content") {
      if (protectedKeys.has(key)) return null;
      protectedKeys.set(key, member.value);
    }
  }
  const type = protectedKeys.get("type");
  if (type === undefined) return null;
  const label = string({ document: value.document, token: type });
  if (label === "text") {
    const token = protectedKeys.get("text");
    if (token === undefined) return null;
    const text = string({ document: value.document, token });
    return text === null ? null : { kind: "text", text, extra: members.length !== 2 };
  }
  if (wrapper && label === "content") {
    const token = protectedKeys.get("content");
    if (token === undefined) return null;
    const inner = textObject({ document: value.document, token }, index, false);
    return inner === null ? null : { ...inner, extra: inner.extra || members.length !== 2 };
  }
  return null;
}
function textParts(value: RawValue): readonly TextPart[] {
  const token = value.document.tokens[value.token];
  if (token?.kind === "string") return [{ kind: "text", text: value.document.stringValue(value.token), extra: false }];
  const index = buildRawIndex(value.document.tokens);
  const values = token?.kind === "array"
    ? (index.children[value.token] ?? []).map((member) => ({ document: value.document, token: member.value }))
    : [value];
  return values.map((entry) => {
    const direct = string(entry);
    if (direct !== null) return { kind: "text", text: direct, extra: false };
    return textObject(entry, index, true) ?? { kind: "raw" };
  });
}
function partKey(part: TextPart): string {
  return part.kind === "text" ? `t:${part.extra ? 1 : 0}:${part.text.length}:${part.text}` : "r";
}
function rawValueKey(entry: NonNullable<FieldMaterial["primary"]>): string {
  if (entry.value === null) return `${entry.raw.append}/missing`;
  const parts = textParts(entry.value).map(partKey).join("|");
  const hasRaw = parts.includes("r") ? `:${entry.raw.byteEnd - entry.raw.byteStart}` : "";
  return `${entry.raw.append}/ready:${parts}${hasRaw}`;
}
function materialKey(value: FieldMaterial): string {
  const reference = (entry: FieldMaterial["primary"]): string =>
    entry === null ? "none" : rawValueKey(entry);
  return `${value.knowledge}:${value.enumCode}:${value.expectedEvidenceCount}:${value.referencesReady}:` +
    reference(value.primary) + ":" + value.evidence.map((entry) => `${entry.index}=${reference(entry)}`).join(";");
}
function statusText(value: FieldMaterial): string {
  if (value.primary !== null && value.primary.value === null) return "状态片段未收到";
  const raw = value.primary?.value ?? null;
  if (raw !== null && raw.document.tokens[raw.token]?.kind === "null") return "未提供状态";
  if (value.knowledge !== "known" || value.enumCode === null) return "未解析状态";
  const label = STATUS[value.enumCode];
  return label !== undefined && string(raw) === label[0] ? label[1] : "未解析状态";
}

class Timeline {
  readonly element: HTMLElement;
  readonly #list: HTMLOListElement;
  readonly #notice: HTMLParagraphElement;
  readonly #live: HTMLParagraphElement;
  #owner: Document | null;
  #store: ViewContentStore | null;
  #segment: string;
  #cards: Map<bigint, Card>;
  #disposed = false;

  constructor(owner: Document, store: ViewContentStore, segment: string) {
    this.#owner = owner;
    this.#store = store;
    this.#segment = segment;
    this.#cards = new Map();
    this.element = this.node("section", "acp-timeline");
    this.element.dataset.segmentId = segment;
    this.element.setAttribute("aria-label", "会话内容");
    const heading = this.node("h2", "acp-timeline__heading");
    heading.textContent = "会话内容";
    this.#notice = this.node("p", "acp-timeline__notice");
    this.#notice.textContent = "显示当前已收到的内容；未解析结构可查看原始记录。";
    this.#live = this.node("p", "acp-timeline__live");
    this.#live.setAttribute("role", "status");
    this.#live.setAttribute("aria-atomic", "true");
    this.#list = this.node("ol", "acp-timeline__items");
    this.element.append(heading, this.#notice, this.#live, this.#list);
  }

  private node<K extends keyof HTMLElementTagNameMap>(tag: K, className: string): HTMLElementTagNameMap[K] {
    if (this.#owner === null) throw new Error("时间线已关闭");
    const node = this.#owner.createElement(tag);
    node.className = className;
    return node;
  }
  private paragraph(parent: Node, text: string, className = "acp-timeline__text"): void {
    const node = this.node("p", className);
    node.textContent = text;
    parent.appendChild(node);
  }
  private button(text: string, listeners: AbortController, action: () => void): HTMLButtonElement {
    const node = this.node("button", "acp-timeline__button");
    node.type = "button";
    node.textContent = text;
    node.addEventListener("click", () => {
      if (this.#disposed || node.disabled) return;
      try { action(); } catch { this.#live.textContent = "本次展示失败，已保留已有内容。"; }
    }, { signal: listeners.signal });
    return node;
  }
  private createCard(item: ProjectedItem): Card {
    const listeners = new AbortController();
    try {
      const element = this.node("li", "acp-timeline__item");
      element.dataset.anchor = item.anchorId.toString();
      const header = this.node("div", "acp-timeline__item-header");
      const heading = this.node("h3", "acp-timeline__item-heading");
      const status = this.node("span", "acp-timeline__state");
      header.append(heading, status);
      const body = this.node("div", "acp-timeline__body");
      const error = this.node("p", "acp-timeline__error");
      error.hidden = true;
      const toolbar = this.node("div", "acp-timeline__toolbar");
      const rawHost = this.node("div", "acp-timeline__raw");
      rawHost.id = `acp-timeline-raw-${++controlId}`;
      const rawNotice = this.node("p", "acp-timeline__notice");
      rawNotice.hidden = true;
      const select = this.node("select", "acp-timeline__select");
      select.setAttribute("aria-label", "原始记录");
      let card: Card;
      const toggle = this.button("查看原始结构", listeners, () => this.toggleRaw(card));
      toggle.setAttribute("aria-controls", rawHost.id);
      toggle.setAttribute("aria-expanded", "false");
      const update = this.button("查看新记录", listeners, () => {
        if (card.latest !== null) this.openRaw(card, card.latest, true);
      });
      update.hidden = true;
      const choose = this.button("查看选定记录", listeners, () => {
        const selected = card.documents.get(select.value);
        if (selected !== undefined) this.openRaw(card, selected.document, true);
      });
      toolbar.append(toggle, select, choose, update);
      element.append(header, body, error, toolbar, rawNotice, rawHost);
      card = { element, heading, status, body, error, toggle, select, choose, update,
        rawNotice, rawHost, listeners, item, bodyKey: "", documents: new Map(), latest: null,
        inspector: null, opened: null };
      return card;
    } catch (error: unknown) { listeners.abort(); throw error; }
  }

  private renderValue(parent: Node, value: RawValue | null): void {
    if (value === null) { this.paragraph(parent, "内容片段尚未收到。", "acp-timeline__notice"); return; }
    const parts = textParts(value);
    if (parts.length === 0) this.paragraph(parent, "无文字内容。", "acp-timeline__notice");
    for (const part of parts) {
      if (part.kind === "raw") {
        this.paragraph(parent, "尚未解析的原生结构，可从原始记录查看。", "acp-timeline__notice");
      } else {
        this.paragraph(parent, bodyText(part.text));
        if (part.extra) this.paragraph(parent, "另含结构信息，原件可查看。", "acp-timeline__notice");
      }
    }
  }
  private renderMessagePart(parent: Node, value: RawValue | null, previous: HTMLParagraphElement | null): HTMLParagraphElement | null {
    const parts = value === null ? [] : textParts(value);
    const part = parts.length === 1 ? parts[0] : undefined;
    if (part?.kind !== "text" || part.extra) {
      this.renderValue(parent, value);
      return null;
    }
    if (previous !== null) {
      previous.append(bodyText(part.text));
      return previous;
    }
    const paragraph = this.node("p", "acp-timeline__text");
    paragraph.textContent = bodyText(part.text);
    parent.appendChild(paragraph);
    return paragraph;
  }
  private renderField(parent: Node, field: FieldMaterial, contentEvidence = false, messageText = false): void {
    if (contentEvidence && field.expectedEvidenceCount > 0n) {
      let expected = 0n;
      let previous: HTMLParagraphElement | null = null;
      for (const entry of field.evidence) {
        if (entry.index !== expected) {
          this.paragraph(parent, "部分内容片段尚未收到。", "acp-timeline__notice");
          previous = null;
        }
        if (messageText) previous = this.renderMessagePart(parent, entry.value, entry.raw.append ? previous : null);
        else this.renderValue(parent, entry.value);
        expected = entry.index + 1n;
      }
    } else if (field.primary !== null) this.renderValue(parent, field.primary.value);
    else this.paragraph(parent, field.knowledge === "knownEmpty" ? "内容为空。" : "未提供可显示的内容。", "acp-timeline__notice");
    if (!field.referencesReady) this.paragraph(parent, "内容尚未完整收到。", "acp-timeline__notice");
  }

  private itemBodyKey(store: ViewContentStore, item: ProjectedItem): string {
    const content = store.field(item, "content");
    const output = store.field(item, "rawOutput");
    return `${item.kind}|${materialKey(content)}|${item.kind === "tool" ? materialKey(output) : ""}`;
  }

  private refreshCard(card: Card, item: ProjectedItem): string | null {
    if (this.#owner === null || this.#store === null) return null;
    const title = this.#store.field(item, "title");
    const status = this.#store.field(item, "status");
    const content = this.#store.field(item, "content");
    const output = this.#store.field(item, "rawOutput");
    const bodyKey = `${item.kind}|${materialKey(content)}|${item.kind === "tool" ? materialKey(output) : ""}`;
    let body: DocumentFragment | null = null;
    if (bodyKey !== card.bodyKey) {
      body = this.#owner.createDocumentFragment();
      this.renderField(body, content, true, item.kind === "message");
      if (item.kind === "tool" && (output.primary !== null || output.expectedEvidenceCount > 0n || output.knowledge === "knownEmpty")) {
        this.paragraph(body, "结果", "acp-timeline__field-heading");
        this.renderField(body, output);
      }
    }
    const label = string(title.primary?.value ?? null);
    const heading = label === null ? KIND[item.kind] : visibleText(label);
    const state = item.kind === "tool" ? statusText(status) : "";
    const documents = new Map<string, SourceDocument>();
    const latest = this.#store.rawDocument(item);
    if (latest !== null) documents.set(item.rawRecordId.toString(), { document: latest, label: "当前原始记录" });
    for (const field of PROJECTION_FIELDS) {
      const material = this.#store.field(item, field);
      for (const reference of [material.primary, ...material.evidence]) {
        if (reference?.value === null || reference === null) continue;
        const key = reference.raw.rawRecordId.toString();
        if (!documents.has(key)) documents.set(key, { document: reference.value.document, label: "相关内容记录" });
      }
    }
    if (card.opened !== null && ![...documents.values()].some((value) => value.document === card.opened)) {
      documents.set("opened", { document: card.opened, label: "先前查看的记录" });
    }
    const selection = card.select.value;
    const options = this.#owner.createDocumentFragment();
    let number = 0;
    for (const [key, source] of documents) {
      const option = this.#owner.createElement("option");
      option.value = key;
      option.textContent = source.label + (source.label === "相关内容记录" ? ` ${++number}` : "");
      options.append(option);
    }
    const changed = item.kind === "tool" && card.status.textContent !== state ? `${heading}：${state}` : null;
    card.heading.textContent = heading;
    card.status.textContent = state;
    card.status.hidden = item.kind !== "tool";
    card.status.dataset.state = item.kind === "tool" ? state : "";
    if (body !== null) { card.body.replaceChildren(body); card.bodyKey = bodyKey; }
    card.documents = documents;
    card.latest = latest;
    card.item = item;
    card.select.replaceChildren(options);
    if (documents.has(selection)) card.select.value = selection;
    card.select.disabled = documents.size === 0;
    card.choose.disabled = documents.size === 0;
    card.toggle.disabled = documents.size === 0 && card.inspector === null;
    card.update.hidden = card.inspector === null || card.opened === latest;
    card.update.disabled = latest === null;
    card.rawNotice.hidden = card.inspector === null || card.opened === latest;
    card.rawNotice.textContent = latest === null
      ? "当前原始记录尚未收到，正在查看先前缓存的记录。"
      : "正在查看先前或所选的原始记录，可显式切换到当前记录。";
    card.error.hidden = true;
    return changed;
  }

  private openRaw(card: Card, document: RawJsonDocument, explicit: boolean): void {
    if (this.#owner === null) return;
    const inspector = createRawInspector(this.#owner, document);
    try { card.rawHost.replaceChildren(inspector.element); }
    catch (error: unknown) { inspector.dispose(); throw error; }
    const previous = card.inspector;
    card.inspector = inspector;
    card.opened = document;
    previous?.dispose();
    card.toggle.textContent = "关闭原始结构";
    card.toggle.setAttribute("aria-expanded", "true");
    card.update.hidden = card.opened === card.latest;
    card.rawNotice.hidden = card.opened === card.latest;
    if (explicit) card.toggle.focus();
  }
  private toggleRaw(card: Card): void {
    if (card.inspector !== null) {
      card.inspector.dispose();
      card.inspector = null;
      card.opened = null;
      card.toggle.textContent = "查看原始结构";
      card.toggle.setAttribute("aria-expanded", "false");
      card.update.hidden = true;
      card.rawNotice.hidden = true;
      return;
    }
    const selected = card.documents.get(card.select.value);
    if (selected !== undefined) this.openRaw(card, selected.document, false);
  }

  refresh(): void {
    if (this.#disposed || this.#store === null) return;
    try {
      const items = this.#store.items(this.#segment);
      const notices: string[] = [];
      for (const item of items) {
        let card = this.#cards.get(item.anchorId);
        const created = card === undefined;
        if (card === undefined) card = this.createCard(item);
        try {
          const notice = this.refreshCard(card, item);
          if (created) {
            const cards = new Map(this.#cards);
            cards.set(item.anchorId, card);
            const following = items.slice(items.indexOf(item) + 1).map((value) => this.#cards.get(value.anchorId)).find((value) => value !== undefined);
            this.#list.insertBefore(card.element, following?.element ?? null);
            this.#cards = cards;
          }
          if (notice !== null) notices.push(notice);
        } catch {
          if (created) { card.listeners.abort(); card.inspector?.dispose(); }
          else {
            card.error.textContent = "本条内容展示失败，已保留上一次有效内容。";
            card.error.hidden = false;
          }
        }
      }
      if (notices.length !== 0) this.#live.textContent = notices.length === 1 ? notices[0] ?? "内容已更新" : "会话内容已更新。";
      this.#notice.textContent = items.length === 0 ? "当前尚无已收到的内容。" : "显示当前已收到的内容；未解析结构可查看原始记录。";
    } catch { this.#live.textContent = "本次展示失败，已保留已有内容。"; }
  }

  rebindGeneration(store: ViewContentStore, segmentId: string): void {
    if (this.#disposed) return;
    this.#store = store;
    this.#segment = segmentId;
    this.element.dataset.segmentId = segmentId;
    try {
      const items = store.items(segmentId);
      const oldList = [...this.#cards.values()];
      const claimed = new Set<Card>();
      const matched: Array<Card | null> = new Array(items.length).fill(null);

      // Match surviving cards across segments: first exact (anchorId, bodyKey), then cross-segment
      // identical bodyKey (preserving <li> & Selection on compressed replays), then (anchorId, kind).
      const itemKeys = items.map((item) => this.itemBodyKey(store, item));
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        const key = itemKeys[i];
        if (item === undefined || key === undefined) continue;
        const candidate = this.#cards.get(item.anchorId);
        if (
          candidate !== undefined &&
          !claimed.has(candidate) &&
          candidate.item.kind === item.kind &&
          candidate.bodyKey === key
        ) {
          claimed.add(candidate);
          matched[i] = candidate;
        }
      }
      for (let i = 0; i < items.length; i++) {
        if (matched[i] !== null) continue;
        const item = items[i];
        const key = itemKeys[i];
        if (item === undefined || key === undefined) continue;
        const candidate = oldList.find((c) => !claimed.has(c) && c.bodyKey === key);
        if (candidate !== undefined) {
          claimed.add(candidate);
          matched[i] = candidate;
        }
      }
      for (let i = 0; i < items.length; i++) {
        if (matched[i] !== null) continue;
        const item = items[i];
        if (item === undefined) continue;
        const candidate = this.#cards.get(item.anchorId);
        if (candidate !== undefined && !claimed.has(candidate) && candidate.item.kind === item.kind) {
          claimed.add(candidate);
          matched[i] = candidate;
        }
      }

      for (const old of oldList) {
        if (!claimed.has(old)) {
          old.listeners.abort();
          old.inspector?.dispose();
          old.element.remove();
        }
      }

      const orderedCards: Card[] = [];
      const nextCards = new Map<bigint, Card>();
      const notices: string[] = [];

      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item === undefined) continue;
        let card = matched[i] ?? null;
        const created = card === null;
        if (card === null) card = this.createCard(item);
        else card.item = item;
        const notice = this.refreshCard(card, item);
        if (notice !== null) notices.push(notice);
        orderedCards.push(card);
        nextCards.set(item.anchorId, card);
        if (created) {
          // Inserted in order below without disturbing surviving siblings.
        }
      }

      for (let i = orderedCards.length - 1; i >= 0; i--) {
        const card = orderedCards[i];
        if (card === undefined) continue;
        const nextEl = i + 1 < orderedCards.length ? (orderedCards[i + 1]?.element ?? null) : null;
        const kids = Array.from(this.#list.children);
        const idx = kids.indexOf(card.element);
        const alreadyPlaced = idx !== -1 && (nextEl === null ? idx === kids.length - 1 : kids[idx + 1] === nextEl);
        if (!alreadyPlaced) {
          this.#list.insertBefore(card.element, nextEl);
        }
      }

      this.#cards = nextCards;
      if (notices.length !== 0) {
        this.#live.textContent = notices.length === 1 ? notices[0] ?? "内容已更新" : "会话内容已更新。";
      }
      this.#notice.textContent = items.length === 0
        ? "当前尚无已收到的内容。"
        : "显示当前已收到的内容；未解析结构可查看原始记录。";
    } catch {
      this.#live.textContent = "本次重绑定展示失败，已保留已有内容。";
    }
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    for (const card of this.#cards.values()) { card.listeners.abort(); card.inspector?.dispose(); }
    this.#cards.clear();
    this.#store = null;
    this.#owner = null;
    this.element.replaceChildren();
    this.element.remove();
  }
}

export function createStructuredTimeline(owner: Document, store: ViewContentStore, segmentId: string): StructuredTimeline {
  let timeline: Timeline | null = null;
  try {
    timeline = new Timeline(owner, store, segmentId);
    timeline.refresh();
    const instance = timeline;
    return Object.freeze({
      element: instance.element,
      refresh: () => instance.refresh(),
      rebindGeneration: (nextStore: ViewContentStore, nextSegmentId: string) =>
        instance.rebindGeneration(nextStore, nextSegmentId),
      dispose: () => instance.dispose(),
    });
  } catch {
    try { timeline?.dispose(); } catch { /* A broken DOM cannot promise disposal. */ }
    const error = new Error("无法建立会话时间线");
    error.name = "TimelineError";
    throw error;
  }
}
