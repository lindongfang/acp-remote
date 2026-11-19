export interface ActiveControlSelectionSnapshot {
  readonly id: string;
  readonly value: string;
  readonly start: number | null;
  readonly end: number | null;
  readonly direction: "forward" | "backward" | "none" | null;
}

function isTextControl(
  node: unknown,
): node is HTMLInputElement | HTMLTextAreaElement {
  if (
    typeof HTMLInputElement !== "undefined" &&
    node instanceof HTMLInputElement
  ) {
    return true;
  }
  if (
    typeof HTMLTextAreaElement !== "undefined" &&
    node instanceof HTMLTextAreaElement
  ) {
    return true;
  }
  if (typeof node === "object" && node !== null && "tagName" in node) {
    const tag = (node as { tagName?: string }).tagName;
    return tag === "INPUT" || tag === "TEXTAREA";
  }
  return false;
}

export function captureActiveControlSelection(
  doc: Document = globalThis.document,
  container?: Element | null,
): ActiveControlSelectionSnapshot | null {
  const focused = doc.activeElement;
  const control = isTextControl(focused) ? focused : null;
  if (
    control === null ||
    control.id === "" ||
    (container !== undefined &&
      container !== null &&
      !container.contains(control))
  ) {
    return null;
  }
  return {
    id: control.id,
    value: control.value,
    start: control.selectionStart,
    end: control.selectionEnd,
    direction: control.selectionDirection,
  };
}

export function restoreActiveControlSelection(
  doc: Document = globalThis.document,
  saved: ActiveControlSelectionSnapshot | null,
): void {
  if (saved === null) return;
  const replacement = doc.getElementById(saved.id);
  if (isTextControl(replacement)) {
    replacement.value = saved.value;
    replacement.focus({ preventScroll: true });
    if (saved.start !== null && saved.end !== null) {
      replacement.setSelectionRange(
        saved.start,
        saved.end,
        saved.direction ?? undefined,
      );
    }
  }
}

export function delay(
  milliseconds: number,
  signal: AbortSignal,
): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const timeout = globalThis.setTimeout(finish, milliseconds);
    function finish(): void {
      globalThis.clearTimeout(timeout);
      signal.removeEventListener("abort", finish);
      resolve();
    }
    signal.addEventListener("abort", finish, { once: true });
  });
}

export function formatHex(value: string): string {
  return value.match(/.{1,8}/gu)?.join(" ") ?? value;
}

export function formatTimestamp(seconds: number): string {
  const date = new Date(seconds * 1_000);
  return Number.isNaN(date.getTime())
    ? String(seconds)
    : date.toLocaleString("zh-CN", { timeZone: "Asia/Shanghai" });
}

export function eyebrow(
  text: string,
  doc: Document = globalThis.document,
): HTMLElement {
  return element("p", "panel-eyebrow", text, doc);
}

export function title(
  text: string,
  doc: Document = globalThis.document,
): HTMLElement {
  const heading = element("h2", undefined, text, doc);
  heading.id = "panel-title";
  return heading;
}

export function paragraph(
  text: string,
  className?: string,
  doc: Document = globalThis.document,
): HTMLElement {
  return element("p", className, text, doc);
}

export function statusLine(
  text: string,
  tone: "working" | "notice" | "error",
  doc: Document = globalThis.document,
): HTMLElement {
  const line = element("p", `status-line status-${tone}`, undefined, doc);
  line.setAttribute("role", tone === "error" ? "alert" : "status");
  const marker = element("span", "status-marker", undefined, doc);
  marker.setAttribute("aria-hidden", "true");
  line.append(marker, doc.createTextNode(text));
  return line;
}

export function detailBlock(
  label: string,
  value: string,
  doc: Document = globalThis.document,
): HTMLElement {
  const block = element("div", "detail-block", undefined, doc);
  block.append(
    element("span", "detail-label", label, doc),
    element("code", undefined, value, doc),
  );
  return block;
}

export function button(
  text: string,
  className?: string,
  doc: Document = globalThis.document,
): HTMLButtonElement {
  const value = element("button", className, text, doc) as HTMLButtonElement;
  value.type = "button";
  return value;
}

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
  doc: Document = globalThis.document,
): HTMLElementTagNameMap[K] {
  const value = doc.createElement(tag);
  if (className !== undefined) {
    value.className = className;
  }
  if (text !== undefined) {
    value.textContent = text;
  }
  return value;
}

export function requiredElement<T extends Element>(
  id: string,
  constructor: { new (): T },
  doc: Document = globalThis.document,
): T {
  const value = doc.getElementById(id);
  if (!(value instanceof constructor)) {
    throw new Error(`required element ${id} is missing`);
  }
  return value;
}
