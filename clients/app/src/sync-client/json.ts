/**
 * 出站 JSON 序列化：在 `JSON.stringify` 之前拦截精确整数字面量。
 *
 * `JSON.stringify` 会把 `JsonNumberLiteral` 序列化成 `{"text":"…"}`，那等于**改了表单值的类型**。
 * 本函数递归地识别它并写出裸数字文本；其余语义尽量与 `JSON.stringify` 一致（对象键顺序、
 * `undefined` 成员省略、数组里的 `undefined` 变 `null`）。
 *
 * 为什么不用 `JSON.stringify` 的 `replacer`：`replacer` 只能返回字符串或对象，无法让一个值以
 * 「裸数字」形态落进结果；先换成哨兵字符串再替换会与正文里的同一字符串撞车。
 */

import { isJsonNumberLiteral } from "../protocol/json-number";

/** 出站消息 / 帧的规范序列化；调用方不得再套一层 `JSON.stringify`。 */
export function stringifyJson(value: unknown): string {
  const text = encode(value);
  return text === undefined ? "null" : text;
}

function encode(value: unknown): string | undefined {
  if (value === null) return "null";
  if (isJsonNumberLiteral(value)) return value.text;
  switch (typeof value) {
    case "string":
      return JSON.stringify(value);
    case "number":
      return Number.isFinite(value) ? String(value) : "null";
    case "boolean":
      return value ? "true" : "false";
    case "bigint":
      return value.toString();
    case "undefined":
    case "function":
    case "symbol":
      return undefined;
    default:
      break;
  }
  if (Array.isArray(value)) {
    return `[${value
      .map((item) => {
        const encoded = encode(item);
        return encoded === undefined ? "null" : encoded;
      })
      .join(",")}]`;
  }
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    const members: string[] = [];
    for (const key of Object.keys(record)) {
      const encoded = encode(record[key]);
      if (encoded === undefined) continue;
      members.push(`${JSON.stringify(key)}:${encoded}`);
    }
    return `{${members.join(",")}}`;
  }
  return undefined;
}
