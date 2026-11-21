export class FakeTextNode {
  textContent: string;
  constructor(text: string) {
    this.textContent = text;
  }
}

export class FakeElement {
  readonly tagName: string;
  id = "";
  className = "";
  type = "";
  name = "";
  htmlFor = "";
  value = "";
  placeholder = "";
  autocomplete = "";
  spellcheck = false;
  required = false;
  disabled = false;
  rows = 0;
  maxLength = 0;
  tabIndex = 0;
  hidden = false;
  selectionStart: number | null = 0;
  selectionEnd: number | null = 0;
  selectionDirection: "forward" | "backward" | "none" | null = "none";
  dataset: Record<string, string> = {};
  children: Array<FakeElement | FakeTextNode> = [];
  attributes = new Map<string, string>();
  listeners = new Map<string, Array<(event: unknown) => void>>();
  ownerDoc: FakeDocument | null = null;
  #textContent = "";

  constructor(tag = "DIV", ownerDoc: FakeDocument | null = null) {
    this.tagName = tag.toUpperCase();
    this.ownerDoc = ownerDoc;
  }

  get textContent(): string {
    if (this.children.length === 0) {
      return this.#textContent;
    }
    return [
      this.#textContent,
      ...this.children.map((c) => c.textContent),
    ].join("");
  }

  set textContent(val: string) {
    this.#textContent = val;
    this.children = [];
  }

  setAttribute(key: string, val: string): void {
    this.attributes.set(key, val);
    if (key === "id") this.id = val;
    if (key === "class") this.className = val;
    if (key.startsWith("data-")) {
      const prop = key
        .slice(5)
        .replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
      this.dataset[prop] = val;
    }
  }

  getAttribute(key: string): string | null {
    if (key === "id" && this.id !== "") return this.id;
    if (key === "class" && this.className !== "") return this.className;
    return this.attributes.get(key) ?? null;
  }

  removeAttribute(key: string): void {
    this.attributes.delete(key);
    if (key.startsWith("data-")) {
      const prop = key
        .slice(5)
        .replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
      delete this.dataset[prop];
    }
  }

  append(...items: Array<FakeElement | FakeTextNode | string>): void {
    for (const item of items) {
      if (typeof item === "string") {
        this.children.push(new FakeTextNode(item));
      } else if (item instanceof FakeElement && item.tagName === "#FRAGMENT") {
        this.children.push(...item.children);
        item.children = [];
      } else {
        this.children.push(item);
      }
    }
  }

  appendChild<T extends FakeElement | FakeTextNode>(child: T): T {
    this.append(child);
    return child;
  }

  insertBefore(child: FakeElement, next: FakeElement | null): void {
    const at = next === null ? this.children.length : this.children.indexOf(next);
    if (at < 0) throw new Error("missing reference child");
    this.children.splice(at, 0, child);
  }

  replaceChildren(...items: Array<FakeElement | FakeTextNode | string>): void {
    this.#textContent = "";
    this.children = [];
    this.append(...items);
  }

  remove(): void {}

  contains(target: unknown): boolean {
    if (target === this) return true;
    for (const child of this.children) {
      if (child instanceof FakeElement && child.contains(target)) {
        return true;
      }
    }
    return false;
  }

  focus(_options?: { preventScroll?: boolean }): void {
    if (this.ownerDoc !== null) {
      this.ownerDoc.activeElement = this;
    }
  }

  setSelectionRange(
    start: number,
    end: number,
    direction?: "forward" | "backward" | "none",
  ): void {
    this.selectionStart = start;
    this.selectionEnd = end;
    this.selectionDirection = direction ?? "none";
  }

  addEventListener(type: string, listener: (event: unknown) => void): void {
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
  }

  dispatchEvent(type: string, event: unknown = { preventDefault: () => {} }): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }

  click(): void {
    if (this.disabled) return;
    this.dispatchEvent("click", { preventDefault: () => {} });
  }

  querySelector(selector: string): FakeElement | null {
    if (selector.startsWith("#")) {
      return this.findById(selector.slice(1));
    }
    return null;
  }

  findById(targetId: string): FakeElement | null {
    if (this.id === targetId) return this;
    for (const child of this.children) {
      if (child instanceof FakeElement) {
        const hit = child.findById(targetId);
        if (hit !== null) return hit;
      }
    }
    return null;
  }
}

export class FakeDivElement extends FakeElement {
  constructor(ownerDoc: FakeDocument | null = null) {
    super("DIV", ownerDoc);
  }
}

export class FakeButtonElement extends FakeElement {
  constructor(ownerDoc: FakeDocument | null = null) {
    super("BUTTON", ownerDoc);
  }
}

export class FakeDocument {
  readonly body: FakeDivElement;
  readonly documentElement: FakeElement;
  activeElement: FakeElement | null = null;

  constructor() {
    this.documentElement = new FakeElement("HTML", this);
    this.body = new FakeDivElement(this);
    this.documentElement.append(this.body);
  }

  createElement(tag: string): FakeElement {
    const upper = tag.toUpperCase();
    if (upper === "DIV") return new FakeDivElement(this);
    if (upper === "BUTTON") return new FakeButtonElement(this);
    return new FakeElement(upper, this);
  }

  createTextNode(text: string): FakeTextNode {
    return new FakeTextNode(text);
  }

  createDocumentFragment(): FakeElement {
    return new FakeElement("#FRAGMENT", this);
  }

  getElementById(id: string): FakeElement | null {
    return this.documentElement.findById(id);
  }
}

export function installFakeDocument(): {
  doc: Document;
  fakeDoc: FakeDocument;
  restore: () => void;
} {
  const prev = (globalThis as { document?: Document }).document;
  const fakeDoc = new FakeDocument();
  const doc = fakeDoc as unknown as Document;
  (globalThis as { document?: Document }).document = doc;
  return {
    doc,
    fakeDoc,
    restore: () => {
      if (prev === undefined) {
        delete (globalThis as { document?: Document }).document;
      } else {
        (globalThis as { document?: Document }).document = prev;
      }
    },
  };
}
