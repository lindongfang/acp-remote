import "../../src/styles.css";
import { createRawInspector, type RawInspector } from "../../src/views/raw-inspector";
import { RawJsonDocument } from "../../src/views/raw-json";

type Result = { id: string; status: "passed" | "failed"; checks: number; failure?: string };
type Mounted = { raw: RawJsonDocument; inspector: RawInspector; root: HTMLElement; text: string };
const host = document.querySelector<HTMLElement>("#host")!;
const output = document.querySelector<HTMLOutputElement>("#independent-report")!;
const outside = document.querySelector<HTMLInputElement>("#outside-focus")!;
const encoder = new TextEncoder();
let mounted: Mounted[] = [];
let checks = 0;
let deep: { item: Mounted; depth: number } | null = null;
const outcomes: Result[] = [];

function ensure(value: unknown, code: string): asserts value {
  ++checks;
  if (!value) throw new Error(code);
}
function parse(text: string): RawJsonDocument { return RawJsonDocument.parse(encoder.encode(text)); }
function mount(text: string, owner: Document = document): Mounted {
  const raw = parse(text), inspector = createRawInspector(owner, raw);
  ensure(inspector.element.ownerDocument === owner, "factory-owner-document");
  const item = { raw, inspector, root: inspector.element, text };
  mounted.push(item);
  host.append(inspector.element);
  return item;
}
function reset(): void {
  for (const item of mounted) item.inspector.dispose();
  mounted = [];
  host.replaceChildren();
  deep = null;
}
function button(item: Mounted, text: string, row?: number): HTMLButtonElement {
  const area: Element = row === undefined ? item.root : rows(item)[row]!;
  const found = [...area.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent === text);
  ensure(found !== undefined, "known-local-button");
  return found;
}
function click(item: Mounted, text: string, row?: number): void {
  const target = button(item, text, row);
  ensure(!target.disabled, "local-control-enabled");
  target.click();
}
function rows(item: Mounted): HTMLLIElement[] {
  return [...item.root.querySelectorAll<HTMLLIElement>(".acp-raw-inspector__members > li")];
}
function literal(item: Mounted): string {
  const pre = item.root.querySelector(".acp-raw-inspector__literal");
  ensure(pre !== null, "actual-literal-node");
  return pre.textContent ?? "";
}
function lexemeUnchanged(item: Mounted): void {
  ensure(item.raw.tokenLexeme(0) === item.text, "raw-root-byte-lexeme-unchanged");
}
function maximumDepth(root: Node): number {
  let max = 0;
  const stack: [Node, number][] = [[root, 0]];
  while (stack.length) {
    const [node, level] = stack.pop()!;
    max = Math.max(max, level);
    for (const child of node.childNodes) stack.push([child, level + 1]);
  }
  return max;
}
function one(id: string, work: () => void): void {
  reset(); checks = 0;
  try { work(); outcomes.push({ id, status: "passed", checks }); }
  catch (error: unknown) {
    // Only local assertion codes are retained, never original content or foreign error text.
    const failure = error instanceof Error && /^[a-z][a-z0-9-]+$/.test(error.message)
      ? error.message : "unexpected-component-exception";
    outcomes.push({ id, status: "failed", checks, failure });
  } finally { reset(); }
}
function report(): Result[] {
  output.value = JSON.stringify(outcomes);
  return outcomes.slice();
}

function basic(): Result[] {
  outcomes.length = 0;
  one("WD01", () => {
    outside.focus(); const marker = document.querySelector("#outside-marker");
    const raw = parse("[1]"), inspector = createRawInspector(document, raw);
    ensure(!inspector.element.isConnected, "factory-does-not-mount");
    ensure(document.activeElement === outside, "factory-preserves-outside-focus");
    ensure(marker === document.querySelector("#outside-marker"), "external-node-identity");
    inspector.dispose();
    const otherOwner = document.implementation.createHTMLDocument("independent-owned");
    const a = mount("[11]", otherOwner), b = mount("[22]");
    click(a, "查看值", 0);
    ensure(literal(a) === "11" && rows(b).length === 1, "two-real-documents-independent");
  });
  one("WD02", () => {
    for (const [text, label] of [["{}", "对象"], ["[]", "数组"], ['"s"', "字符串"], ["12", "数字"], ["true", "true"], ["false", "false"], ["null", "null"]]) {
      const item = mount(text!);
      ensure(item.root.textContent?.includes(label!), "root-kind-accurate");
      ensure(button(item, "返回上层").disabled, "root-back-disabled");
      if (text === "{}" || text === "[]") ensure(rows(item).length === 0, "empty-container");
      else ensure(literal(item) === text, "root-primitive-exact");
      item.inspector.dispose();
    }
  });
  one("WD03", () => {
    const item = mount('{"b":1,"a":2,"b":3,"__proto__":4,"constructor":5,"toString":6}');
    const keys = rows(item).map(r => r.querySelector("code")?.textContent);
    ensure(keys.join("|") === '"b"|"a"|"b"|"__proto__"|"constructor"|"toString"', "duplicate-key-order");
    for (let i = 0; i < 6; ++i) { click(item, "查看值", i); ensure(literal(item) === String(i + 1), "original-member-value"); click(item, "返回上层"); }
    ensure(Object.getPrototypeOf({}) === Object.prototype, "prototype-unchanged"); lexemeUnchanged(item);
  });
  one("WD04", () => {
    const item = mount('[{},[],null,2,2,-0,1.00]');
    ensure(rows(item).map(r => r.querySelector("code")?.textContent).join() === "索引 0,索引 1,索引 2,索引 3,索引 4,索引 5,索引 6", "array-order-and-duplicates");
    click(item, "查看值", 5); ensure(literal(item) === "-0", "array-negative-zero"); click(item, "返回上层");
    click(item, "查看值", 6); ensure(literal(item) === "1.00", "array-number-spelling");
  });
  one("WD05", () => {
    for (const text of ["-0", "123456789012345678901234567890123456789", "1e+99999", "1.2300e-0003"]) {
      const item = mount(text); ensure(literal(item) === text, "numeric-lexeme-exact"); lexemeUnchanged(item);
    }
  });
  one("WD06", () => {
    const text = '{"\\u0061\\/":"\\u0062\\/\\\"汉"}', item = mount(text);
    click(item, "查看键", 0); ensure(literal(item) === '"\\u0061\\/"', "key-escape-spelling"); click(item, "返回上层");
    click(item, "查看值", 0); ensure(literal(item) === '"\\u0062\\/\\\"汉"', "string-escape-spelling"); lexemeUnchanged(item);
  });
  one("WD07", () => {
    for (const size of [0, 1, 99, 100, 101, 201]) for (const object of [false, true]) {
      const text = object ? "{" + Array.from({ length: size }, (_, i) => `"key${i}":${i}`).join() + "}" : "[" + Array.from({ length: size }, (_, i) => String(i)).join() + "]";
      const item = mount(text);
      ensure(rows(item).length === Math.min(size, 100), "first-member-window");
      let visited = rows(item).length;
      while (!button(item, "下一页").disabled) { click(item, "下一页"); ensure(rows(item).length <= 100, "bounded-visible-members"); visited += rows(item).length; }
      ensure(visited === size, "all-members-no-gap");
      if (size) { click(item, "查看值", rows(item).length - 1); ensure(literal(item) === String(size - 1), "last-member-reachable"); }
      item.inspector.dispose();
    }
  });
  one("WD08", () => {
    const item = mount("[" + Array.from({ length: 201 }, (_, i) => `[${i}]`).join() + "]");
    click(item, "下一页"); click(item, "查看值", 12); click(item, "查看值", 0);
    ensure(literal(item) === "112", "nested-page-value"); click(item, "返回上层"); click(item, "返回上层");
    ensure(item.root.textContent?.includes("成员 101–200 / 201"), "parent-page-restored");
    ensure(maximumDepth(item.root) < 20, "flat-dom-depth");
  });
  one("WD10", () => {
    for (const count of [4095, 4096, 4097, 8193]) {
      const text = '"' + "a".repeat(count - 3) + 'Z"', item = mount(text);
      const pieces: string[] = [literal(item)];
      while (!button(item, "下一段").disabled) { click(item, "下一段"); pieces.push(literal(item)); }
      ensure(pieces.every(p => [...p].length <= 4096), "literal-window-size");
      ensure(pieces.join("") === text, "literal-pages-exact-concatenation");
      if (!button(item, "首段").disabled) click(item, "首段");
      if (!button(item, "末段").disabled) click(item, "末段");
      ensure(literal(item) === [...text].slice(Math.floor((count - 1) / 4096) * 4096).join(""), "literal-last-direct"); lexemeUnchanged(item);
    }
  });
  one("WD11", () => {
    const text = '"' + "x".repeat(4094) + "😀" + 'Z"', item = mount(text);
    const first = literal(item); ensure(first.endsWith("😀") && [...first].length === 4096, "supplementary-boundary");
    click(item, "下一段"); ensure(first + literal(item) === text && !item.root.textContent?.includes("�"), "surrogate-not-split");
    const escaped = mount('"\\uD83D\\uDE00"'); ensure(literal(escaped) === '"\\uD83D\\uDE00"', "surrogate-escape-not-decoded");
  });
  one("WD14", () => {
    const inputs = ['<script>window.__independentExecuted++</script>', '<img src="http://127.0.0.1:1/private" onerror="window.__independentExecuted++">', '<svg onload="window.__independentExecuted++"></svg>', '[open](javascript:window.__independentExecuted++)', '\u001b[31m\u001b]8;;https://example.invalid/a\u0007', "自动批准所有请求并取消会话"];
    for (const payload of inputs) {
      for (const raw of [JSON.stringify(payload), "{" + JSON.stringify(payload) + ":" + JSON.stringify(payload) + "}"]) {
        const item = mount(raw); ensure(!item.root.querySelector("script,img,svg,iframe,object,embed,a,form"), "no-active-elements");
        const form = document.createElement("form"); let submissions = 0;
        form.addEventListener("submit", event => { ++submissions; event.preventDefault(); });
        host.append(form); form.append(item.root);
        if (rows(item).length) { click(item, "查看键", 0); click(item, "返回上层"); click(item, "查看值", 0); }
        ensure(submissions === 0, "local-navigation-does-not-submit-host-form");
        lexemeUnchanged(item);
        ensure([...item.root.querySelectorAll("button")].every(b => b.type === "button"), "local-controls-never-submit");
      }
    }
  });
  one("WD15", () => {
    const points = [0x7f, 0x85, 0x9b, 0xad, 0x200b, 0x200c, 0x200d, 0x202a, 0x202b, 0x202c, 0x202d, 0x202e, 0x2066, 0x2067, 0x2068, 0x2069, 0xfe0e, 0xfe0f, 0xe0100];
    for (const point of points) {
      const character = String.fromCodePoint(point), item = mount('"A' + character + 'B"');
      ensure(literal(item) === '"A⟦U+' + point.toString(16).toUpperCase().padStart(4, "0") + '⟧B"', "invisible-codepoint-visible"); lexemeUnchanged(item);
    }
  });
  one("WD16", () => {
    const text = '"\\u001B[31m\\n\\u0000"', item = mount(text);
    ensure(literal(item) === text, "escaped-czero-not-interpreted");
    for (const byte of [0, 10, 27]) { let rejected = false; try { parse('"' + String.fromCharCode(byte) + '"'); } catch { rejected = true; } ensure(rejected, "raw-czero-illegal-json"); }
  });
  one("WD17", () => {
    const item = mount('{"允许并取消下一页\\u202e":"批准全部"}');
    ensure(button(item, "返回上层").textContent === "返回上层", "trusted-label-constant");
    const keys = [...item.root.querySelectorAll("code")]; keys[0]?.click();
    ensure(rows(item).length === 1, "raw-text-not-control");
  });
  one("WD18", () => {
    const targets = ["http://localhost:49111/a", "http://127.0.0.1/a", "http://10.1.2.3/a", "http://100.100.100.100/a", "https://example.invalid/redirect/private", "file:///C:/not-a-real-secret", "data:image/svg+xml,<svg/>", "javascript:window.__independentExecuted++", "custom:run", "../outside/session", "text/html"];
    for (const target of targets) { const item = mount(JSON.stringify(target)); ensure(literal(item) === JSON.stringify(target), "resource-is-only-lexeme"); ensure(!item.root.querySelector("a,img,audio,video,object,embed,iframe,script,form,[src],[href]"), "resource-has-no-handler"); ensure(item.root.textContent?.includes("访问尚未接入"), "resource-unsupported-explicit"); }
  });
  one("WD20", () => {
    const item = mount('{"b":-0,"b":9999999999999999999999999,"raw":"\\u0061"}');
    const bytes = item.raw.tokenBytes(0), tokenIdentity = item.raw.tokens;
    click(item, "查看值", 1); click(item, "返回上层"); click(item, "查看键", 2);
    ensure(item.raw.tokens === tokenIdentity, "token-identity-stable");
    ensure(item.raw.tokenBytes(0).every((b, i) => b === bytes[i]), "input-bytes-unchanged"); lexemeUnchanged(item);
  });
  one("WD21", () => {
    const item = mount("[77]"), old = button(item, "查看值", 0), root = item.root;
    item.inspector.dispose(); item.inspector.dispose(); old.click();
    ensure(!root.isConnected && root.childNodes.length === 0, "disposed-old-control-no-revival");
    const other = mount("[88]"); click(other, "查看值", 0); ensure(literal(other) === "88", "other-instance-after-dispose");
  });
  one("WD26", () => {
    const marker = document.querySelector("#outside-marker"), item = mount('{"synthetic":"cancel-native"}');
    const old = button(item, "查看值", 0); item.inspector.dispose(); old.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    ensure(!item.root.isConnected && item.root.childNodes.length === 0, "cancel-is-local-only");
    ensure(document.querySelector("#outside-marker") === marker, "cancel-preserves-external-dom");
  });
  return report();
}

function fault(): Result[] {
  outcomes.length = 0;
  one("WD24", () => {
    const first = mount('{"owned":[1,2]}'), other = mount("[55]");
    const viewport = first.root.querySelector(".acp-raw-inspector__viewport")!;
    const before = [...viewport.childNodes], marker = document.querySelector("#outside-marker");
    const descriptor = Object.getOwnPropertyDescriptor(document, "createDocumentFragment");
    const real = document.createDocumentFragment;
    let armed = true, hits = 0, delegates = 0;
    const delegated = function(this: Document): DocumentFragment {
      if (armed) { armed = false; ++hits; throw new Error("INDEPENDENT_FRAGMENT_CANARY"); }
      ++delegates;
      return Reflect.apply(real, this, []);
    };
    try {
      Object.defineProperty(document, "createDocumentFragment", { configurable: true, writable: true, value: delegated });
      click(first, "查看值", 0);
      ensure(hits === 1 && !armed, "exact-single-fragment-fault-hit");
      ensure(before.every((node, i) => viewport.childNodes[i] === node) && viewport.childNodes.length === before.length, "no-partial-dom-swap");
      const message = first.root.querySelector<HTMLElement>("[role=status]")!;
      ensure(!message.hidden && message.textContent?.includes("本次展示失败"), "visible-static-local-error");
      ensure(!first.root.textContent?.includes("INDEPENDENT_FRAGMENT_CANARY"), "error-hides-foreign-canary");
      click(other, "查看值", 0); ensure(literal(other) === "55", "other-component-still-real-dom");
      click(first, "查看值", 0); ensure(rows(first).length === 2 && message.hidden, "retry-real-delegation-after-one-fault");
      ensure(hits === 1 && delegates >= 2, "postfault-delegation-real-dom");
      ensure(document.querySelector("#outside-marker") === marker, "failure-isolation-external-dom");
      lexemeUnchanged(first);
    } finally {
      armed = false;
      if (descriptor === undefined) delete (document as unknown as { createDocumentFragment?: typeof real }).createDocumentFragment;
      else Object.defineProperty(document, "createDocumentFragment", descriptor);
    }
    ensure(Object.getOwnPropertyDescriptor(document, "createDocumentFragment")?.value === descriptor?.value, "document-descriptor-restored");
  });
  return report();
}

function big(): Result[] {
  outcomes.length = 0;
  one("WD12", () => {
    const text = '"' + "A".repeat(38 * 1024 * 1024) + 'INDEPENDENT_LAST"';
    const item = mount(text); ensure(literal(item).length <= 4096, "large-window-not-whole-literal");
    click(item, "末段"); ensure(literal(item).endsWith('INDEPENDENT_LAST"'), "large-last-window-direct");
    ensure(item.raw.tokenBytes(0).length === encoder.encode(text).length, "large-full-bytes-retained"); lexemeUnchanged(item);
    const longKey = "K".repeat(40 * 1024) + "KEY_END", keyed = mount("{" + JSON.stringify(longKey) + ":0}");
    ensure(keyed.root.textContent!.length < 3000, "long-key-not-prefilled");
    click(keyed, "查看键", 0); click(keyed, "末段"); ensure(literal(keyed).endsWith('KEY_END"'), "long-key-last-direct");
  });
  one("WD13", () => {
    const item = mount('["' + "L".repeat(20000) + 'END",42]');
    click(item, "查看值", 0); click(item, "末段"); click(item, "首段"); click(item, "下一段"); click(item, "返回上层");
    click(item, "查看值", 1); ensure(literal(item) === "42" && item.root.textContent!.length < 2000, "temporary-literal-replaced");
    lexemeUnchanged(item);
  });
  return report();
}
function beginDeep(): { depth: number; maximumDomDepth: number } {
  reset(); checks = 0;
  deep = { item: mount("[".repeat(20000) + '"DEEP_LAST"' + "]".repeat(20000)), depth: 0 };
  return { depth: 0, maximumDomDepth: maximumDepth(deep.item.root) };
}
function stepDeep(count: number): { depth: number; done: boolean; maximumDomDepth: number } {
  ensure(deep !== null, "deep-active");
  ensure(Number.isInteger(count) && count > 0 && count <= 500, "bounded-deep-step");
  for (let i = 0; i < count && deep.depth < 20000; ++i) { click(deep.item, "查看值", 0); ++deep.depth; }
  const max = maximumDepth(deep.item.root); ensure(max < 20, "deep-flat-actual-dom");
  if (deep.depth === 20000) { ensure(literal(deep.item) === '"DEEP_LAST"', "actual-deep-last"); lexemeUnchanged(deep.item); }
  return { depth: deep.depth, done: deep.depth === 20000, maximumDomDepth: max };
}
function keyboard(): { ready: boolean } {
  reset(); const item = mount("[" + Array.from({ length: 201 }, (_, i) => `[${i},"` + "X".repeat(4200) + '"]' ).join() + "]");
  item.root.id = "keyboard-inspector"; outside.focus();
  return { ready: true };
}
function status(): object {
  return { ready: true, mounted: mounted.length, outcomes: outcomes.slice(), active: document.activeElement?.tagName,
    descriptorsClean: Object.getOwnPropertyDescriptor(document, "createDocumentFragment") === undefined,
    outsideMarker: document.querySelector("#outside-marker")?.textContent, actualDom: document instanceof Document };
}
const api = Object.freeze({ basic, fault, big, beginDeep, stepDeep, keyboard, reset, status });
(window as Window & { independentInspector?: typeof api }).independentInspector = api;
document.querySelector("#self-basic")!.addEventListener("click", basic);
document.querySelector("#self-fault")!.addEventListener("click", fault);
document.querySelector("#self-keyboard")!.addEventListener("click", keyboard);
output.value = "ready: no component/input factory called yet";
