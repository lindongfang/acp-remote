/**
 * 固定样例门禁：`fixtures/sync/v1/manifest.json` 里每一条「WSS 消息」正向样例都必须被
 * 解码边界（`./wire.ts` 的 `decodeWireMessage`）受理。
 *
 * 样例与 schema 都从**仓库根**读（`../protocol/manifest` 与 `../protocol/paths`），不在
 * `clients/app/` 下复制。`manifest.ts` 依赖 `node:fs`，而 `../protocol` 的 barrel 按
 * `contract.test.ts` 的 F1 不得把 Node 内置模块带进 web bundle，因此这里**直接**导入
 * `../protocol/manifest` 模块文件，不走 barrel。
 *
 * ## 判别力来源
 *
 * - `wire.ts` 的 `EVENT_ORIGIN_KINDS` 若退回四值（丢掉 `node`）：`valid/event-node-origin.json`
 *   会被判 `field_type`（`event.origin.kind 不在封闭词表内`）→ 逐条断言变红；
 * - `wire.ts` 若把 `error` 放回「认证后必须有 connectionId」的集合：
 *   `valid/error-before-auth.json` 会被判 `envelope_violation` → 逐条断言变红；
 * - 若删掉 `origin` 的 node 配对规则：两条定点负例会双双变成 `ok: true` → 负例断言变红。
 *
 * ## 为什么**不**设「所有 invalid 样例都必须被拒收」的门禁
 *
 * `./wire.ts` 的文件头已写明本层口径：只判**信封 + 判别字段 + 必填键的存在与基本类型**，
 * 视图与载荷内部的**值域**归 `schemas/sync/v1/**` 与 `projectView()` 的降级路径
 * （`design.md` D8 要求协议载荷不直接进视图模型，而不是要求这一层实现全量 JSON Schema 校验）。
 * 于是多数 `invalid` 样例本就会被受理：按同一份 manifest 核对，`valid === false` 且
 * `schema` 指向 `message.schema.json` 的 35 条里就有 20 条如此（例如
 * `invalid/command-session-create-with-session-id.json`、`invalid/command-unknown-field.json`
 * 违反的是命令**请求**载荷的键集合，`invalid/command-workspace-browse-path-non-string.json`
 * 违反的是请求载荷内部的字段类型），
 * 另有 13 条 `invalid/view-*.json` 违反的是 `event-views` 的值域，通篇与消息信封无关。
 * 把「全部拒收」写成门禁，等于要求客户端在这一层实现全量 JSON Schema 校验，与既定口径
 * 直接冲突；因此这里只对**本次修正涉及的** node 配对形状做定点失败断言。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import {
  readManifest,
  resolveFixtureCase,
  type FixtureCase,
  type ResolvedFixtureCase,
} from "../protocol/manifest";
import { syncSchemaDir } from "../protocol/paths";
import { decodeWireMessage } from "./wire";

/** `schema` 以它结尾的样例才是 WSS 消息；`pairing.schema.json` 那一类是 HTTP 配对载荷。 */
const MESSAGE_SCHEMA_SUFFIX = "message.schema.json";

/** 一条被门禁覆盖的正向样例。 */
interface MessageSample {
  readonly case: FixtureCase;
  /** 仓库根之下的 fixture 绝对路径（`resolveFixtureCase` 已约束在 `fixtures/sync/v1/` 之下）。 */
  readonly fixturePath: string;
  /** 顶层 `type`；样例形状意外时是 `null`，由逐条解码断言变成可见失败。 */
  readonly type: string | null;
}

/** 读一个已解析的 JSON 值的顶层 `type`（非字符串即 `null`）。 */
function topLevelType(parsed: unknown): string | null {
  if (typeof parsed !== "object" || parsed === null) return null;
  const type = (parsed as Record<string, unknown>)["type"];
  return typeof type === "string" ? type : null;
}

const manifest = readManifest();

/**
 * `valid === true` 且 `schema` 指向 `message.schema.json` 的样例。
 *
 * 这个筛选排除了两类非 WSS 消息样例：`pairing.schema.json` 的 5 条 HTTP 配对载荷（它们被
 * schema 后缀挡掉），以及 `event-views.schema.json` 的视图定义（它们在 manifest 里全部是
 * `valid: false`，被 `valid === true` 挡掉）。
 */
const samples: readonly MessageSample[] = manifest.cases
  .filter((entry) => entry.valid === true && entry.schema.endsWith(MESSAGE_SCHEMA_SUFFIX))
  .map((entry) => {
    const resolved = resolveFixtureCase(entry);
    const parsed: unknown = JSON.parse(readFileSync(resolved.fixturePath, "utf8"));
    return { case: entry, fixturePath: resolved.fixturePath, type: topLevelType(parsed) };
  });

/**
 * 解码器支持的消息类型集合。
 *
 * 从 `schemas/sync/v1/message.schema.json` 的 `oneOf` 出发，收集被引用 schema 里作为消息
 * 判别字段出现的 `properties.type.const`——**不抄字面量常量**：wire 合同增删一个消息类型
 * 而正向样例没跟上时，下面的覆盖度断言变红。
 */
function messageTypesFromSchemas(): ReadonlySet<string> {
  const root = JSON.parse(readFileSync(join(syncSchemaDir, "message.schema.json"), "utf8")) as {
    readonly oneOf?: readonly { readonly $ref?: unknown }[];
  };
  const types = new Set<string>();
  for (const entry of root.oneOf ?? []) {
    if (typeof entry.$ref !== "string") continue;
    const schema: unknown = JSON.parse(readFileSync(join(syncSchemaDir, entry.$ref), "utf8"));
    collectTypeConsts(schema, types);
  }
  return types;
}

/** 深度遍历一个 schema 片段，收集 `properties.type.const` 的字符串取值。 */
function collectTypeConsts(value: unknown, into: Set<string>): void {
  if (typeof value !== "object" || value === null) return;
  if (Array.isArray(value)) {
    for (const item of value as readonly unknown[]) collectTypeConsts(item, into);
    return;
  }
  const record = value as Record<string, unknown>;
  const properties = record["properties"];
  if (typeof properties === "object" && properties !== null) {
    const discriminant = (properties as Record<string, unknown>)["type"];
    if (typeof discriminant === "object" && discriminant !== null) {
      const constValue = (discriminant as Record<string, unknown>)["const"];
      if (typeof constValue === "string") into.add(constValue);
    }
  }
  for (const child of Object.values(record)) collectTypeConsts(child, into);
}

describe("固定样例门禁：WSS 消息正向样例必须被解码边界受理", () => {
  for (const sample of samples) {
    it(`受理 ${sample.case.fixture}`, () => {
      const result = decodeWireMessage(readFileSync(sample.fixturePath, "utf8"));
      const rejection = result.ok ? "" : `拒绝原因 ${result.reason}：${result.detail}`;
      // 失败信息带上样例路径与实测拒绝原因，便于定位是哪一类形状漂了。
      expect(result.ok, `${sample.case.fixture} 未被受理（${rejection}）`).toBe(true);
    });
  }

  it("正向样例覆盖解码器支持的每一个消息类型", () => {
    const fromSamples = new Set<string>();
    for (const sample of samples) {
      if (sample.type !== null) fromSamples.add(sample.type);
    }
    const missing = [...messageTypesFromSchemas()]
      .filter((type) => !fromSamples.has(type))
      .sort();
    expect(missing, `没有正向样例的消息类型：${missing.join(", ")}`).toEqual([]);
  });
});

/** 从同一份 manifest 取一条反向样例，并解析到仓库根的真实路径。 */
function invalidCase(fixture: string): ResolvedFixtureCase {
  const entry = manifest.cases.find(
    (candidate) => candidate.valid === false && candidate.fixture === fixture,
  );
  if (entry === undefined) throw new Error(`manifest 缺反向样例：${fixture}`);
  return resolveFixtureCase(entry);
}

describe("定点负例：origin 的 node 身份字段配对", () => {
  it("node 来源缺 nodeId 键 → missing_field", () => {
    const resolved = invalidCase("invalid/event-origin-node-missing-node-id.json");
    const result = decodeWireMessage(readFileSync(resolved.fixturePath, "utf8"));
    // 反例：删掉 node 配对规则后这条样例会被受理（`ok: true`）→ 断言变红。
    expect(result).toMatchObject({ ok: false, reason: "missing_field" });
  });

  it("node 来源的 nodeId 为 null（键在场、值非 UUID）→ field_type", () => {
    const resolved = invalidCase("invalid/event-origin-node-without-node-id.json");
    const result = decodeWireMessage(readFileSync(resolved.fixturePath, "utf8"));
    // 反例：同上；若实现把「键在场但值为 null」也判成 missing_field，这里的 reason 比对变红。
    expect(result).toMatchObject({ ok: false, reason: "field_type" });
  });
});
