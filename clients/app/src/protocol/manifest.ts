/**
 * `fixtures/sync/v1/manifest.json` 的**严格**类型化读取。
 *
 * `AGENTS.md` §5 要求 Rust 与 TypeScript 消费同一 manifest；`plan.md` 的 PV3/AC3 判据是
 * 「读同一份 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`，不复制样例」。
 *
 * 因此这个模块只做两件事：
 *
 * 1. 按 v1 的字段约定把 manifest 解析成判别联合（见 `fixtures/sync/v1/README.md`）；
 * 2. 逐条解析出**仓库根的真实路径**，交给契约测试 `readFile`。
 *
 * 它**不**把样例内联进来，也不在 `clients/app/` 下缓存副本——复制会立刻漂移，
 * 而 `check:schemas` 只校验仓库根的那一份。
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

import { repoRoot, syncFixtureDir, syncManifestPath } from "./paths";

/** manifest 的 `schemaVersion`（当前冻结为 1）。 */
export const MANIFEST_SCHEMA_VERSION = 1;

/**
 * 一条 fixture case 的公共字段。
 *
 * `schema` 是**相对 manifest 自身**的路径（形如 `../../../schemas/sync/v1/message.schema.json`），
 * 因此解析基准是 `fixtures/sync/v1/`，正好是仓库根 `schemas/` 的上溯位置。
 */
interface FixtureCaseBase {
  /** 相对 `fixtures/sync/v1/` 的 fixture 文件路径。 */
  readonly fixture: string;
  /** 相对 manifest 的 schema 路径（`../../../schemas/sync/v1/…`）。 */
  readonly schema: string;
}

/** 正向样例：必须通过对应 schema。 */
export interface ValidFixtureCase extends FixtureCaseBase {
  readonly valid: true;
  /** 事件样例可追加：`payload.view` 还须用该视图 `$defs` 校验。 */
  readonly viewSchema?: string;
  /** 形如 `#/$defs/agent.message.delta` 的 JSON Pointer。 */
  readonly viewDef?: string;
}

/**
 * 反向样例：必须以 `expectedKeyword` 指定的关键字失败。
 * 该断言由仓库根 `scripts/check-schema-fixtures.mjs` 用 ajv 执行，不在本包内复制。
 */
export interface InvalidFixtureCase extends FixtureCaseBase {
  readonly valid: false;
  readonly expectedKeyword:
    | "enum"
    | "maximum"
    | "minimum"
    | "required"
    | "additionalProperties"
    | "type"
    | "const"
    | "oneOf"
    | "anyOf"
    | "pattern"
    | "maxLength"
    | "minLength"
    | "uniqueItems"
    | "maxItems"
    | "minItems"
    | "not"
    | "allOf"
    | "if";
}

/** manifest 的一条 case（判别联合，按 `valid` 收窄）。 */
export type FixtureCase = ValidFixtureCase | InvalidFixtureCase;

/** manifest 的顶层形状。 */
export interface FixtureManifest {
  readonly schemaVersion: typeof MANIFEST_SCHEMA_VERSION;
  readonly cases: readonly FixtureCase[];
}

/** 解析后的 fixture case：把相对路径都解析成仓库根的绝对路径。 */
export interface ResolvedFixtureCase {
  readonly case: FixtureCase;
  /** fixture 文件的绝对路径（仓库根之下）。 */
  readonly fixturePath: string;
  /** 主 schema 的绝对路径（仓库根之下）。 */
  readonly schemaPath: string;
  /** 事件视图 schema 的绝对路径；非事件样例为 `null`。 */
  readonly viewSchemaPath: string | null;
}

/**
 * 读取并严格解析 manifest。
 *
 * **不做宽容解析**：`JSON.parse` 的结果按 v1 形状逐条核对，字段缺失或类型不符直接抛错——
 * 静默降级会让「前端消费了漂移的 manifest」在运行期才暴露。
 */
export function readManifest(path: string = syncManifestPath): FixtureManifest {
  const raw: unknown = JSON.parse(readFileSync(path, "utf8"));
  assertManifest(raw, path);
  return raw;
}

/** 检查一个已解析的 JSON 值是否是 v1 manifest。 */
function assertManifest(value: unknown, path: string): asserts value is FixtureManifest {
  if (typeof value !== "object" || value === null) {
    throw new Error(`manifest 不是对象：${path}`);
  }
  const record = value as Record<string, unknown>;
  if (record["schemaVersion"] !== MANIFEST_SCHEMA_VERSION) {
    throw new Error(
      `manifest.schemaVersion 期望 ${MANIFEST_SCHEMA_VERSION}，实际 ${String(record["schemaVersion"])}：${path}`,
    );
  }
  if (!Array.isArray(record["cases"])) {
    throw new Error(`manifest.cases 不是数组：${path}`);
  }
  for (const [index, entry] of record["cases"].entries()) {
    if (typeof entry !== "object" || entry === null) {
      throw new Error(`manifest.cases[${index}] 不是对象：${path}`);
    }
    const item = entry as Record<string, unknown>;
    if (typeof item["fixture"] !== "string" || item["fixture"].length === 0) {
      throw new Error(`manifest.cases[${index}].fixture 缺失或为空：${path}`);
    }
    if (typeof item["schema"] !== "string" || item["schema"].length === 0) {
      throw new Error(`manifest.cases[${index}].schema 缺失或为空：${path}`);
    }
    if (typeof item["valid"] !== "boolean") {
      throw new Error(`manifest.cases[${index}].valid 缺失或非布尔：${path}`);
    }
    if (item["valid"] === false && typeof item["expectedKeyword"] !== "string") {
      throw new Error(`manifest.cases[${index}]（invalid）缺少 expectedKeyword：${path}`);
    }
  }
}

/**
 * 把 manifest 的 case 解析成仓库根的绝对路径。
 *
 * `schema` 的解析基准是 manifest 所在目录（`fixtures/sync/v1/`）——
 * `../../../schemas/sync/v1/x.schema.json` 由此落到 `<repo>/schemas/sync/v1/x.schema.json`。
 */
export function resolveFixtureCase(
  entry: FixtureCase,
  manifestPath: string = syncManifestPath,
): ResolvedFixtureCase {
  const base = dirname(manifestPath);
  return {
    case: entry,
    fixturePath: resolve(syncFixtureDir, entry.fixture),
    schemaPath: resolve(base, entry.schema),
    viewSchemaPath:
      entry.valid && entry.viewSchema !== undefined
        ? resolve(base, entry.viewSchema)
        : null,
  };
}

/** manifest 相对仓库根的路径；报告与测试据此证明「读的是同一份资产」。 */
export const MANIFEST_REPO_RELATIVE_PATH = "fixtures/sync/v1/manifest.json";

/** 仓库根绝对路径；报告与测试据此证明「没有复制到 `clients/app/`」。 */
export const MANIFEST_REPO_ROOT: string = repoRoot;
