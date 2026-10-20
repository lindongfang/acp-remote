/**
 * 精确 JSON 数字字面量。
 *
 * ## 为什么需要它
 *
 * 表单里的整数可能超过 JavaScript 的 safe integer（`docs/SYNC_PROTOCOL.md` §11.7「表单整数不得
 * 先经 JavaScript number 再序列化」）。如果把它读成 `number`，`9007199254740993` 会被静默改写成
 * `9007199254740992`，再声称「已按原值回传」就是错的。
 *
 * 因此表单模型把这类值保存成 **十进制文本 + 本类的实例**；协议序列化器（`src/sync-client/json.ts`）
 * 识别本类时直接写出数字字面量（不加引号），其余值仍走 `JSON.stringify`。它只用于出站表单值，
 * 不参与任何 `Number` 运算。
 */

import type { DecimalString } from "./common";

/** 有符号十进制整数的规范化文本（`-?(0|[1-9][0-9]*)`，不接受 `-0`）。 */
const SIGNED_DECIMAL_PATTERN = /^-?(0|[1-9][0-9]*)$/;

/**
 * 一个必须按原文写成 JSON **数字**的精确整数。
 *
 * 构造时规范化：去掉前导零（若存在）、拒绝 `-0` 与任何非整数文本。`toString()` 返回 JSON 数字
 * 文本，`JSON.stringify` 看到的是本类实例（序列化器必须在它之前拦截）。
 */
export class JsonNumberLiteral {
  readonly text: string;

  constructor(text: string) {
    if (!SIGNED_DECIMAL_PATTERN.test(text) || text === "-0") {
      throw new Error(`不是合法的精确整数文本：${text}`);
    }
    this.text = text;
  }

  /** 十进制字符串形式（无符号时也是 `DecimalString` 的合法取值）。 */
  asDecimal(): DecimalString {
    return this.text;
  }

  toString(): string {
    return this.text;
  }
}

/** 判别一个精确整数（序列化器与表单校验共用同一处判定）。 */
export function isJsonNumberLiteral(value: unknown): value is JsonNumberLiteral {
  return value instanceof JsonNumberLiteral;
}
