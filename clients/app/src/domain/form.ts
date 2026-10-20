/**
 * 可填写表单（elicitation）的客户端模型：从 ACP schema 的快照支持子集构造控件、校验值、产出载荷。
 *
 * ## 支持子集与「整体不可用」
 *
 * 支持文本、整数/数值、布尔、单选（`enum` / 单值 `oneOf`）、字符串数组多选（`items.enum`），
 * 以及必填、`minLength`/`maxLength`/`minimum`/`maximum`。**不支持**嵌套对象、任意数组、未知类型
 * 与 schema 外键。任何**必需**字段落在不支持集合里时，整个提交不可用并说明原因——不得以空对象
 * 或 `null` 冒充填写（`docs/SYNC_PROTOCOL.md` §11.7）。
 *
 * ## 精确整数
 *
 * 整数控件在页面上是文本框，校验通过后包成 `JsonNumberLiteral`；序列化器把它写成裸 JSON 数字。
 * 因此 `9007199254740993` 不会被 `Number` 再变成 `…992`。
 *
 * ## 不在客户端执行的校验
 *
 * `pattern` 来自 Agent（不可信输入）。本层**不编译**它：任意正则可以在浏览器里造成灾难性回溯。
 * 字符串模式仍由服务端校验；客户端只做长度与必填，避免把「没本地报错」说成「服务端一定接受」。
 */

import { JsonNumberLiteral } from "../protocol/json-number";

/** 单选/多选的一个可选项；`value` 是回传值，`label` 只用于呈现。 */
export interface FormChoice {
  readonly value: string;
  readonly label: string;
}

interface FormFieldBase {
  readonly name: string;
  readonly label: string;
  readonly description: string | null;
  readonly required: boolean;
}

/** 一个可呈现的字段。 */
export type ElicitationField =
  | (FormFieldBase & { readonly kind: "text"; readonly minLength: number | null; readonly maxLength: number | null })
  | (FormFieldBase & { readonly kind: "integer"; readonly minimum: string | null; readonly maximum: string | null })
  | (FormFieldBase & { readonly kind: "number"; readonly minimum: string | null; readonly maximum: string | null })
  | (FormFieldBase & { readonly kind: "boolean" })
  | (FormFieldBase & { readonly kind: "enum"; readonly choices: readonly FormChoice[] })
  | (FormFieldBase & {
      readonly kind: "multi_enum";
      readonly choices: readonly FormChoice[];
      readonly minItems: number | null;
      readonly maxItems: number | null;
    });

/** 表单的初始值（页面把它放进受控控件）。 */
export type FormInputValue = string | boolean | readonly string[];

export type ElicitationForm =
  | {
      readonly kind: "form";
      readonly fields: readonly ElicitationField[];
      /** 可选但不可表达的字段名（不阻断提交，只是不收集）。 */
      readonly unsupportedOptional: readonly string[];
      readonly initial: Readonly<Record<string, FormInputValue>>;
    }
  | {
      readonly kind: "unavailable";
      readonly reason: "schema_missing" | "schema_invalid" | "unsupported_required";
      readonly detail: string;
    };

/** 校验结果；`values` 是命令载荷里的 `values` 对象。 */
export type FormValidation =
  | { readonly ok: true; readonly values: Record<string, unknown> }
  | { readonly ok: false; readonly errors: Readonly<Record<string, string>>; readonly detail: string };

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

/** 读一个有限的非负整数（schema 的 `minLength` 等）；不合法返回 `null`。 */
function nonNegativeInteger(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}

/** 读一个数值边界并按十进制文本保存（精确整数字段用 BigInt 比较）。 */
function numericBound(value: unknown): string | null {
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  if (typeof value === "string" && /^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/.test(value)) return value;
  return null;
}

/**
 * 读选项：`enum`（值必须全是字符串）优先，其次 `oneOf` 里每一项是 `{const, title?}`。
 * 其他形状返回 `null`（该字段不可表达）。
 */
function parseChoices(schema: Record<string, unknown>): readonly FormChoice[] | null {
  const enumValues = schema["enum"];
  if (Array.isArray(enumValues)) {
    if (!enumValues.every((value) => typeof value === "string")) return null;
    const enumNames = schema["enumNames"];
    return enumValues.map((value, index) => ({
      value,
      label: Array.isArray(enumNames) && typeof enumNames[index] === "string" ? enumNames[index] : value,
    }));
  }
  const oneOf = schema["oneOf"];
  if (Array.isArray(oneOf)) {
    const choices: FormChoice[] = [];
    for (const item of oneOf) {
      if (!isPlainObject(item) || typeof item["const"] !== "string") return null;
      const title = item["title"];
      choices.push({ value: item["const"], label: typeof title === "string" && title.length > 0 ? title : item["const"] });
    }
    return choices.length === 0 ? null : choices;
  }
  return null;
}

function baseField(
  name: string,
  schema: Record<string, unknown>,
  required: boolean,
): FormFieldBase {
  const title = schema["title"];
  const description = schema["description"];
  return {
    name,
    label: typeof title === "string" && title.length > 0 ? title : name,
    description: typeof description === "string" ? description : null,
    required,
  };
}

/** 单个属性 → 字段；不可表达返回 `null`。 */
function parseField(name: string, schema: unknown, required: boolean): ElicitationField | null {
  if (!isPlainObject(schema)) return null;
  const base = baseField(name, schema, required);
  const type = schema["type"];
  const choices = parseChoices(schema);

  if (type === "boolean") return { ...base, kind: "boolean" };

  if (type === "string" || (type === undefined && choices !== null)) {
    if (choices !== null) return { ...base, kind: "enum", choices };
    return {
      ...base,
      kind: "text",
      minLength: nonNegativeInteger(schema["minLength"]),
      maxLength: nonNegativeInteger(schema["maxLength"]),
    };
  }

  if (type === "integer") {
    return {
      ...base,
      kind: "integer",
      minimum: numericBound(schema["minimum"]),
      maximum: numericBound(schema["maximum"]),
    };
  }

  if (type === "number") {
    return {
      ...base,
      kind: "number",
      minimum: numericBound(schema["minimum"]),
      maximum: numericBound(schema["maximum"]),
    };
  }

  if (type === "array") {
    const items = schema["items"];
    if (!isPlainObject(items)) return null;
    const itemChoices = parseChoices(items);
    if (itemChoices === null) return null;
    return {
      ...base,
      kind: "multi_enum",
      choices: itemChoices,
      minItems: nonNegativeInteger(schema["minItems"]),
      maxItems: nonNegativeInteger(schema["maxItems"]),
    };
  }

  return null;
}

/**
 * 从 ACP schema 构造表单。
 *
 * `schema === null`（permission 交互或缺失）返回 `unavailable / schema_missing`；根不是
 * `{type:"object", properties:{…}}` 返回 `schema_invalid`；任何**必填**字段不可表达返回
 * `unsupported_required`（整表不可提交）。
 */
export function buildElicitationForm(schema: Readonly<Record<string, unknown>> | null): ElicitationForm {
  if (schema === null) {
    return { kind: "unavailable", reason: "schema_missing", detail: "服务端没有给出这张表单的结构。" };
  }
  if (!isPlainObject(schema)) {
    return { kind: "unavailable", reason: "schema_invalid", detail: "表单 schema 不是对象。" };
  }
  const type = schema["type"];
  if (type !== undefined && type !== "object") {
    return { kind: "unavailable", reason: "schema_invalid", detail: "表单根节点必须是 object。" };
  }
  const properties = schema["properties"];
  if (properties === undefined) {
    return { kind: "form", fields: [], unsupportedOptional: [], initial: {} };
  }
  if (!isPlainObject(properties)) {
    return { kind: "unavailable", reason: "schema_invalid", detail: "表单 properties 不是对象。" };
  }
  const requiredRaw = schema["required"];
  if (requiredRaw !== undefined && (!Array.isArray(requiredRaw) || !requiredRaw.every((item) => typeof item === "string"))) {
    return { kind: "unavailable", reason: "schema_invalid", detail: "表单 required 必须是字符串数组。" };
  }
  const required = new Set<string>(Array.isArray(requiredRaw) ? (requiredRaw as string[]) : []);

  const fields: ElicitationField[] = [];
  const unsupportedOptional: string[] = [];
  for (const name of Object.keys(properties)) {
    const field = parseField(name, properties[name], required.has(name));
    if (field === null) {
      if (required.has(name)) {
        return {
          kind: "unavailable",
          reason: "unsupported_required",
          detail: `必填字段「${name}」的类型不在当前客户端支持的表单子集里，整张表单不能提交。`,
        };
      }
      unsupportedOptional.push(name);
      continue;
    }
    fields.push(field);
  }

  const initial: Record<string, FormInputValue> = {};
  for (const field of fields) {
    if (field.kind === "boolean") initial[field.name] = false;
    else if (field.kind === "multi_enum") initial[field.name] = [];
    else initial[field.name] = "";
  }
  return { kind: "form", fields, unsupportedOptional, initial };
}

/**
 * 整数文本校验：`0` 或 `-?[1-9][0-9]*`。
 *
 * 它必须与 `JsonNumberLiteral` 的口径一致：拒绝前导零，也拒绝 `-0`（后者会让构造器抛错，
 * 而表单校验应当返回「必须是整数」这条普通错误，不能让点击提交变成未捕获异常）。
 */
const INTEGER_PATTERN = /^(?:0|-?[1-9][0-9]*)$/;
/** 数值文本：允许小数与指数以外的十进制写法（服务端仍会再校验）。 */
const NUMBER_PATTERN = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?$/;

/**
 * 十进制文本 → `unscaled / 10^scale`。
 *
 * 接受 JSON Schema 里可能出现的指数写法（`1e+21`，`String(number)` 也会这样产出），因此在
 * 整数字段的 `minimum: 1.5` 这类合法小数边界上仍能做**精确**比较，不经过 `Number`、也不让
 * `BigInt("1.5")` 抛错。
 */
function decimalParts(text: string): { readonly negative: boolean; readonly unscaled: bigint; readonly scale: number } | null {
  const match = /^(-?)(\d+)(?:\.(\d+))?(?:[eE]([+-]?\d+))?$/.exec(text);
  if (match === null) return null;
  const negative = match[1] === "-";
  const whole = match[2] ?? "0";
  const fraction = match[3] ?? "";
  const exponent = Number(match[4] ?? "0");
  if (!Number.isSafeInteger(exponent)) return null;
  const digits = whole + fraction;
  let scale = fraction.length - exponent;
  if (scale < 0) {
    const unscaled = BigInt(digits + "0".repeat(-scale));
    return { negative: negative && unscaled !== 0n, unscaled, scale: 0 };
  }
  const unscaled = BigInt(digits);
  return { negative: negative && unscaled !== 0n, unscaled, scale };
}

/** 按十进制文本精确比较（不经过 Number；边界可含小数与指数写法）。 */
function compareDecimal(a: string, b: string): number {
  const left = decimalParts(a);
  const right = decimalParts(b);
  // 调用点的输入都来自 `INTEGER_PATTERN` 或 `numericBound`；不可解析时退回数值比较（不让表单崩掉）。
  if (left === null || right === null) {
    const leftValue = Number(a);
    const rightValue = Number(b);
    return leftValue === rightValue ? 0 : leftValue < rightValue ? -1 : 1;
  }
  const scale = Math.max(left.scale, right.scale);
  const leftValue = left.unscaled * 10n ** BigInt(scale - left.scale);
  const rightValue = right.unscaled * 10n ** BigInt(scale - right.scale);
  const signedLeft = left.negative ? -leftValue : leftValue;
  const signedRight = right.negative ? -rightValue : rightValue;
  return signedLeft === signedRight ? 0 : signedLeft < signedRight ? -1 : 1;
}

/**
 * 校验并构造提交值。
 *
 * 未知字段（不在当前表单里）同样报错：静默丢弃会让「用户填了却没回传」无从发现。
 * 可选字段留空时**省略**（不是传空串）；布尔始终显式回传。
 */
export function validateElicitationForm(
  form: ElicitationForm,
  input: Readonly<Record<string, FormInputValue>>,
): FormValidation {
  if (form.kind !== "form") {
    return { ok: false, errors: {}, detail: form.detail };
  }
  const errors: Record<string, string> = {};
  const values: Record<string, unknown> = {};
  const names = new Set(form.fields.map((field) => field.name));
  for (const key of Object.keys(input)) {
    if (!names.has(key)) errors[key] = "这张表单没有这个字段。";
  }

  for (const field of form.fields) {
    const raw = input[field.name];
    switch (field.kind) {
      case "boolean":
        if (typeof raw !== "boolean") {
          errors[field.name] = "必须是布尔值。";
          break;
        }
        values[field.name] = raw;
        break;
      case "text": {
        const text = typeof raw === "string" ? raw : "";
        if (text.length === 0) {
          if (field.required) errors[field.name] = "必填。";
          break;
        }
        if (field.minLength !== null && text.length < field.minLength) {
          errors[field.name] = `至少 ${String(field.minLength)} 个字符。`;
          break;
        }
        if (field.maxLength !== null && text.length > field.maxLength) {
          errors[field.name] = `至多 ${String(field.maxLength)} 个字符。`;
          break;
        }
        values[field.name] = text;
        break;
      }
      case "integer": {
        const text = typeof raw === "string" ? raw.trim() : "";
        if (text.length === 0) {
          if (field.required) errors[field.name] = "必填。";
          break;
        }
        if (!INTEGER_PATTERN.test(text)) {
          errors[field.name] = "必须是整数。";
          break;
        }
        if (field.minimum !== null && compareDecimal(text, field.minimum) < 0) {
          errors[field.name] = `不能小于 ${field.minimum}。`;
          break;
        }
        if (field.maximum !== null && compareDecimal(text, field.maximum) > 0) {
          errors[field.name] = `不能大于 ${field.maximum}。`;
          break;
        }
        // 精确整数：保存原文，序列化时写成裸 JSON 数字（不经 Number）。
        values[field.name] = new JsonNumberLiteral(text);
        break;
      }
      case "number": {
        const text = typeof raw === "string" ? raw.trim() : "";
        if (text.length === 0) {
          if (field.required) errors[field.name] = "必填。";
          break;
        }
        if (!NUMBER_PATTERN.test(text)) {
          errors[field.name] = "必须是数值。";
          break;
        }
        const numeric = Number(text);
        if (!Number.isFinite(numeric)) {
          errors[field.name] = "必须是有限数值。";
          break;
        }
        if (field.minimum !== null && numeric < Number(field.minimum)) {
          errors[field.name] = `不能小于 ${field.minimum}。`;
          break;
        }
        if (field.maximum !== null && numeric > Number(field.maximum)) {
          errors[field.name] = `不能大于 ${field.maximum}。`;
          break;
        }
        // `number` 字段按数值回传（不是字符串）：Agent 按自身 `type: number` 校验，
        // 同表里的整数用 `JsonNumberLiteral` 保持原文，两者都必须是 JSON 数字。
        values[field.name] = numeric;
        break;
      }
      case "enum": {
        const text = typeof raw === "string" ? raw : "";
        if (text.length === 0) {
          if (field.required) errors[field.name] = "必填。";
          break;
        }
        if (!field.choices.some((choice) => choice.value === text)) {
          errors[field.name] = "不是这张表单允许的取值。";
          break;
        }
        values[field.name] = text;
        break;
      }
      case "multi_enum": {
        const selected = Array.isArray(raw) ? raw.filter((item): item is string => typeof item === "string") : [];
        if (selected.length === 0) {
          if (field.required) errors[field.name] = "必填。";
          break;
        }
        if (field.minItems !== null && selected.length < field.minItems) {
          errors[field.name] = `至少选择 ${String(field.minItems)} 项。`;
          break;
        }
        if (field.maxItems !== null && selected.length > field.maxItems) {
          errors[field.name] = `至多选择 ${String(field.maxItems)} 项。`;
          break;
        }
        if (new Set(selected).size !== selected.length) {
          errors[field.name] = "不能重复选择同一项。";
          break;
        }
        if (!selected.every((value) => field.choices.some((choice) => choice.value === value))) {
          errors[field.name] = "包含这张表单不允许的取值。";
          break;
        }
        values[field.name] = selected;
        break;
      }
    }
  }

  if (Object.keys(errors).length > 0) {
    return { ok: false, errors, detail: "表单里有未通过校验的字段。" };
  }
  return { ok: true, values };
}
