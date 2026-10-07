/**
 * 仓库根协议资产的**唯一**解析入口。
 *
 * `AGENTS.md` §5 要求「Rust 和 TypeScript 必须消费同一 manifest，不能各自复制一套测试样例」，
 * 而 `design.md` D8 与 `plan.md` 的 WP5a 行把 `schemas/sync/v1/**@WP1` 与
 * `fixtures/sync/v1/manifest.json@WP1` 定为**数据**依赖（`contract:`），不是编译产物。
 *
 * 因此这里只做路径解析：把相对 `clients/app` 的仓库根真实路径算出来，交给调用方
 * `readFile`/`JSON.parse`。**不复制**任何 schema 或 fixture 到 `clients/app/` 之下——
 * 复制会立刻漂移，而 `check:schemas` 只校验仓库根那一份。
 *
 * 解析规则：本文件位于 `<repo>/clients/app/src/protocol/paths.ts`，仓库根因此是上溯四级
 * （`protocol` → `src` → `app` → `clients` → `<repo>`）。
 */

import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/** ACP Remote 仓库根（`Cargo.toml` / `package.json` / `schemas/` 所在目录）。 */
export const repoRoot: string = resolve(here, "..", "..", "..", "..");

/** `schemas/sync/v1/` —— 机器合同的权威目录。 */
export const syncSchemaDir: string = resolve(repoRoot, "schemas", "sync", "v1");

/** `fixtures/sync/v1/` —— 正反样例与 transcript 固定向量的唯一来源。 */
export const syncFixtureDir: string = resolve(repoRoot, "fixtures", "sync", "v1");

/** `fixtures/sync/v1/manifest.json` —— fixture → schema → 预期结果的映射。 */
export const syncManifestPath: string = resolve(syncFixtureDir, "manifest.json");

/** `schemas/sync/v1/` 下的全部入口文件名（与 `schemas/sync/v1/README.md` 的清单一致）。 */
export const SYNC_SCHEMA_FILES = [
  "message.schema.json",
  "pairing.schema.json",
  "common.schema.json",
  "auth.schema.json",
  "sync.schema.json",
  "event.schema.json",
  "event-views.schema.json",
  "command.schema.json",
  "error.schema.json",
] as const;
