import "../../src/styles.css";
import { createRawInspector, type RawInspector } from "../../src/views/raw-inspector";
import { RawJsonDocument } from "../../src/views/raw-json";

function required<K extends keyof HTMLElementTagNameMap>(id: string, tag: K): HTMLElementTagNameMap[K] {
  const element = document.getElementById(id);
  if (element === null || element.tagName.toLowerCase() !== tag) {
    throw new Error("开发夹具缺少本地元素");
  }
  return element as HTMLElementTagNameMap[K];
}

const sample = required("fixture-sample", "select");
const input = required("fixture-json", "textarea");
const host = required("fixture-host", "div");
const result = required("fixture-result", "p");
let inspector: RawInspector | null = null;

function load(): void {
  switch (sample.value) {
    case "members": input.value = "[" + Array.from({ length: 201 }, (_, i) => i).join(",") + "]"; break;
    case "deep": input.value = "[".repeat(20_000) + '"deep-end"' + "]".repeat(20_000); break;
    case "literal": input.value = '"' + "x".repeat(1024 * 1024 + 17) + 'literal-end"'; break;
    case "controls": input.value = '{"path":"报告\u202Ecod.exe","joiner":"left\u200Dright","escaped":"\\u001B[31m"}'; break;
    default: input.value = '{"b":1,"a":[true,null,{"x":"示例文本"}],"b":3,"__proto__":4,"precise":123456789012345678901234567890,"exponent":1.2300e-0003}';
  }
}

required("fixture-load", "button").addEventListener("click", load);
required("fixture-theme", "button").addEventListener("click", () => {
  document.documentElement.dataset.theme = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
});
required("fixture-mount", "button").addEventListener("click", () => {
  try {
    const raw = RawJsonDocument.parse(new TextEncoder().encode(input.value));
    const next = createRawInspector(document, raw);
    inspector?.dispose();
    inspector = next;
    host.append(next.element);
    result.textContent = "已建立本地结构查看器。";
  } catch {
    result.textContent = "输入或本次显示不可用，原视图保留。";
  }
});
required("fixture-dispose", "button").addEventListener("click", () => {
  inspector?.dispose();
  inspector = null;
  result.textContent = "本地查看器已关闭。";
});
load();
