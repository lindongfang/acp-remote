/**
 * AC3 的可核对证据：`clients/app` 读**同一份**仓库根 `fixtures/sync/v1/manifest.json`
 * 与 `schemas/sync/v1/**`（`tasks.md` 4.3 的判据）。
 *
 * 本用例不复制样例、不链接 Rust 编译产物，只证明三件事：
 *
 * 1. 解析出的路径落在**仓库根**之下，且 `clients/app/` 下不存在任何 schema/fixture 副本；
 * 2. manifest 的每条 case 都能解析到真实存在的 fixture 与 schema 文件；
 * 3. `src/protocol/` 的 DTO 字段、可选性与枚举取值域与 schema **一一对应**
 *    （逐条从 schema 读出 `required`/`enum` 与镜像比对，而不是抄一份常量）。
 */

import { existsSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  MANIFEST_REPO_RELATIVE_PATH,
  MANIFEST_REPO_ROOT,
  MANIFEST_SCHEMA_VERSION,
  readManifest,
  resolveFixtureCase,
  type FixtureCase,
  type ValidFixtureCase,
} from "./manifest";
import { repoRoot, syncFixtureDir, syncSchemaDir, syncManifestPath, SYNC_SCHEMA_FILES } from "./paths";
import { KNOWN_EVENT_TYPE_COUNT, isKnownEventType, projectView } from "./event";

/** 读取 `schemas/sync/v1/<file>` 并解析。 */
function readSchema(file: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(syncSchemaDir, file), "utf8")) as Record<string, unknown>;
}

/** 读取 `$defs` 条目。 */
function defs(schema: Record<string, unknown>): Record<string, unknown> {
  const value = schema["$defs"];
  expect(typeof value).toBe("object");
  return value as Record<string, unknown>;
}

describe("AC3：消费同一份仓库根协议资产", () => {
  it("路径解析落在仓库根之下，而不是 clients/app 内部", () => {
    // 仓库根必须是含 package.json 与 schemas/ 的那一级。
    expect(existsSync(join(MANIFEST_REPO_ROOT, "package.json"))).toBe(true);
    expect(existsSync(join(MANIFEST_REPO_ROOT, "Cargo.toml"))).toBe(true);
    expect(MANIFEST_REPO_ROOT).toBe(repoRoot);
    expect(MANIFEST_REPO_ROOT.endsWith(join("clients", "app"))).toBe(false);
    expect(MANIFEST_REPO_ROOT.replaceAll("\\", "/")).not.toMatch(/\/clients\/app$/);
  });

  it("clients/app 下不存在 schemas/ 或 fixtures/ 的副本", () => {
    const appDir = resolve(repoRoot, "clients", "app");
    expect(existsSync(join(appDir, "schemas"))).toBe(false);
    expect(existsSync(join(appDir, "fixtures"))).toBe(false);
  });

  it("manifest 与 schema 路径都指向仓库根的真实文件", () => {
    expect(syncManifestPath).toBe(join(syncFixtureDir, "manifest.json"));
    expect(existsSync(syncManifestPath)).toBe(true);
    expect(MANIFEST_REPO_RELATIVE_PATH).toBe("fixtures/sync/v1/manifest.json");

    for (const file of SYNC_SCHEMA_FILES) {
      expect(existsSync(join(syncSchemaDir, file)), `缺少 ${file}`).toBe(true);
    }
  });

  it("严格解析 manifest，且每条 case 的 fixture/schema 都存在", () => {
    const manifest = readManifest();
    expect(manifest.schemaVersion).toBe(MANIFEST_SCHEMA_VERSION);
    expect(manifest.cases.length).toBeGreaterThan(0);

    for (const entry of manifest.cases) {
      const resolved = resolveFixtureCase(entry);
      expect(existsSync(resolved.fixturePath), `fixture 不存在：${entry.fixture}`).toBe(true);
      expect(existsSync(resolved.schemaPath), `schema 不存在：${entry.schema}`).toBe(true);
      // schema 路径必须落在仓库根 schemas/ 之下。
      expect(resolved.schemaPath.replaceAll("\\", "/")).toContain("/schemas/sync/v1/");
      // viewSchema 同样必须落在仓库根 schemas/ 之下，且指向 event-views 那一份。
      if (resolved.viewSchemaPath !== null) {
        expect(resolved.viewSchemaPath.replaceAll("\\", "/")).toContain("/schemas/sync/v1/");
        expect(resolved.viewSchemaPath).toBe(join(syncSchemaDir, "event-views.schema.json"));
      }
    }
  });

  it("每条 viewDef 都指向 event-views.schema.json 的 $defs 且与 fixture 的 eventType 一致", () => {
    const viewDefs = defs(readSchema("event-views.schema.json"));
    // 先用类型谓词收窄，避免在 `InvalidFixtureCase` 上读 `viewDef`。
    const hasViewDef = (
      entry: FixtureCase,
    ): entry is ValidFixtureCase & { readonly viewDef: string } =>
      entry.valid && entry.viewDef !== undefined;
    const casesWithView = readManifest().cases.filter(hasViewDef);
    // 视图绑定确实被使用（不是一份空声明）。
    expect(casesWithView.length).toBeGreaterThan(0);

    for (const entry of casesWithView) {
      const viewDef = entry.viewDef;
      expect(viewDef).toMatch(/^#\/\$defs\/[A-Za-z0-9._-]+$/);
      const eventType = viewDef.slice("#/$defs/".length);
      expect(Object.hasOwn(viewDefs, eventType), `$defs 缺少 ${eventType}`).toBe(true);

      // viewSchema 必须同时给出，并被解析到 event-views 那一份。
      const resolved = resolveFixtureCase(entry);
      expect(resolved.viewSchemaPath).toBe(join(syncSchemaDir, "event-views.schema.json"));
      // `viewDef` 指定的 $defs 键就是该 fixture 的 `body.eventType`。
      const fixture = JSON.parse(readFileSync(resolved.fixturePath, "utf8")) as {
        body?: { eventType?: unknown };
      };
      expect(fixture.body?.eventType).toBe(eventType);
    }
  });

  it("viewSchema 越出 schemas/sync/v1/ 会被拒绝（绝对路径与上溯都不采用）", () => {
    const outsideAbsolute = "C:/Windows/System32/drivers/etc/hosts";
    expect(() =>
      resolveFixtureCase({
        fixture: "valid/event-agent-delta.json",
        schema: "../../../schemas/sync/v1/message.schema.json",
        valid: true,
        viewSchema: outsideAbsolute,
        viewDef: "#/$defs/agent.message.delta",
      }),
    ).toThrow(/viewSchema 越出 schemas\/sync\/v1\//);

    expect(() =>
      resolveFixtureCase({
        fixture: "valid/event-agent-delta.json",
        schema: "../../../schemas/sync/v1/message.schema.json",
        valid: true,
        viewSchema: "../../../../docs/FRONTEND_DESIGN.md",
        viewDef: "#/$defs/agent.message.delta",
      }),
    ).toThrow(/viewSchema 越出 schemas\/sync\/v1\//);
  });

  it("assertManifest 拒绝半声明的 viewSchema/viewDef 与非法 viewDef 形状", () => {
    const dir = mkdtempSync(join(tmpdir(), "wp5a-manifest-"));
    const writeCase = (caseValue: Record<string, unknown>): string => {
      const file = join(dir, `${Math.random().toString(36).slice(2)}.json`);
      writeFileSync(file, JSON.stringify({ schemaVersion: 1, cases: [caseValue] }), "utf8");
      return file;
    };
    const base = {
      fixture: "valid/event-agent-delta.json",
      schema: "../../../schemas/sync/v1/message.schema.json",
      valid: true,
    };

    // 只有 viewDef、没有 viewSchema：旧实现静默解析为 viewSchemaPath: null，viewDef 被丢弃。
    expect(() => readManifest(writeCase({ ...base, viewDef: "#/$defs/agent.message.delta" }))).toThrow(
      /viewSchema 与 viewDef 必须成对声明/,
    );
    // 只有 viewSchema、没有 viewDef。
    expect(() =>
      readManifest(writeCase({ ...base, viewSchema: "../../../schemas/sync/v1/event-views.schema.json" })),
    ).toThrow(/viewSchema 与 viewDef 必须成对声明/);
    // viewDef 不是 `#/$defs/<eventType>` 形状。
    expect(() =>
      readManifest(
        writeCase({
          ...base,
          viewSchema: "../../../schemas/sync/v1/event-views.schema.json",
          viewDef: "$defs/agent.message.delta",
        }),
      ),
    ).toThrow(/#\/\$defs\/<eventType>/);
  });

  it("manifest 的 schema 引用与 SYNC_SCHEMA_FILES 的清单一致", () => {
    const referenced = new Set<string>();
    for (const entry of readManifest().cases) {
      const resolved = resolveFixtureCase(entry);
      referenced.add(resolved.schemaPath);
      if (resolved.viewSchemaPath !== null) referenced.add(resolved.viewSchemaPath);
    }
    const known = new Set(SYNC_SCHEMA_FILES.map((file) => join(syncSchemaDir, file)));
    for (const path of referenced) {
      expect(known.has(path), `manifest 引用了未登记的 schema：${path}`).toBe(true);
    }
  });
});

describe("AC3：protocol 镜像与 schema 逐项对应", () => {
  it("common.sessionSummary 的 required 集合与镜像一致", () => {
    const schema = readSchema("common.schema.json");
    const summary = defs(schema)["sessionSummary"] as { required: string[]; properties: Record<string, unknown> };
    expect(summary.required).toEqual([
      "sessionId",
      "title",
      "agent",
      "state",
      "origin",
      "currentMode",
      "version",
      "createdAt",
      "updatedAt",
    ]);
    expect(Object.keys(summary.properties).sort()).toEqual([
      "agent",
      "createdAt",
      "currentMode",
      "origin",
      "sessionId",
      "state",
      "title",
      "updatedAt",
      "version",
      "workspace",
    ]);
  });

  it("sessionSummary.state 的枚举取值范围与 SessionStateView 一致", () => {
    const schema = readSchema("common.schema.json");
    const summary = defs(schema)["sessionSummary"] as {
      properties: { state: { enum: string[] } };
    };
    expect(summary.properties.state.enum).toEqual([
      "idle",
      "queued",
      "running",
      "waiting_input",
      "waiting_permission",
      "failed",
      "closed",
    ]);
  });

  it("snapshotResource 收窄为三类清单资源（design D1）", () => {
    const schema = readSchema("sync.schema.json");
    const resource = defs(schema)["snapshotResource"] as { enum: string[] };
    expect(resource.enum).toEqual(["sessions", "workspaces", "agents"]);
    const chunkCount = defs(schema)["snapshotChunkCount"] as { minimum: number; maximum: number };
    expect(chunkCount.minimum).toBe(1);
    expect(chunkCount.maximum).toBe(resource.enum.length);
  });

  it("sessionRead 的 before 是 (createdAt, messageId) 复合游标", () => {
    const schema = readSchema("command.schema.json");
    const before = defs(schema)["sessionReadBefore"] as { required: string[] };
    expect(before.required).toEqual(["createdAt", "messageId"]);
    const read = defs(schema)["sessionRead"] as { properties: { payload: { properties: Record<string, unknown> } } };
    const payloadKeys = Object.keys(read.properties.payload.properties).sort();
    expect(payloadKeys).toEqual(["before", "include", "limit"]);
  });

  it("sessionReadResult 必填 hasEarlier", () => {
    const schema = readSchema("command.schema.json");
    const result = defs(schema)["sessionReadResult"] as { required: string[] };
    expect(result.required).toEqual(["sessionId", "resources", "hasEarlier"]);
  });

  it("sessionCreate 的 payload 只允许两个引用", () => {
    const schema = readSchema("command.schema.json");
    const create = defs(schema)["sessionCreate"] as {
      properties: { payload: { required: string[]; additionalProperties: boolean } };
    };
    expect(create.properties.payload.required).toEqual(["workspaceAlias", "agentId"]);
    expect(create.properties.payload.additionalProperties).toBe(false);
  });

  it("file.changed 的三个新可选字段存在，必填集合不变", () => {
    const schema = readSchema("event-views.schema.json");
    const changed = defs(schema)["file.changed"] as { required: string[]; properties: Record<string, unknown> };
    expect(changed.required).toEqual(["changeId", "kind", "displayPath", "summary"]);
    expect(Object.keys(changed.properties).sort()).toEqual([
      "addedLines",
      "changeId",
      "deletedLines",
      "displayPath",
      "kind",
      "outsideWorkspace",
      "summary",
    ]);
  });

  it("agent.connected / agent.disconnected 的 state 是唯一取值的封闭枚举", () => {
    const schema = readSchema("event-views.schema.json");
    const connected = defs(schema)["agent.connected"] as { properties: { state: { enum: string[] } } };
    const disconnected = defs(schema)["agent.disconnected"] as { properties: { state: { enum: string[] } } };
    expect(connected.properties.state.enum).toEqual(["connected"]);
    expect(disconnected.properties.state.enum).toEqual(["disconnected"]);
  });

  it("event-views 的 $defs 数量与 eventType 登记数量一致", () => {
    const schema = readSchema("event-views.schema.json");
    expect(Object.keys(defs(schema)).length).toBe(KNOWN_EVENT_TYPE_COUNT);
  });

  it("未知事件类型走 unknown 分支且不抛错（未知事件不崩溃会话）", () => {
    expect(isKnownEventType("agent.message.delta")).toBe(true);
    expect(isKnownEventType("future.unknown.event")).toBe(false);

    const projected = projectView("future.unknown.event", { anything: 1 });
    expect(projected.kind).toBe("unknown");
    if (projected.kind === "unknown") {
      expect(projected.eventType).toBe("future.unknown.event");
      expect(projected.view).toEqual({ anything: 1 });
    }
  });

  it("已知事件类型原样保留未知字段（开放对象）", () => {
    const view = { messageId: "m", turnId: "t", deltaIndex: "0", text: "hi", futureField: { a: 1 } };
    const projected = projectView("agent.message.delta", view);
    expect(projected.kind).toBe("known");
    expect(projected.view).toHaveProperty("futureField");
  });
});

describe("AC3：fixture 目录无本地副本", () => {
  it("fixtures/sync/v1 下的样例文件只在仓库根出现", () => {
    const rootFixtures = readdirSync(syncFixtureDir);
    expect(rootFixtures).toContain("manifest.json");
    expect(rootFixtures).toContain("valid");
    expect(rootFixtures).toContain("invalid");
    expect(rootFixtures).toContain("transcripts");
  });
});

describe("F1：protocol barrel 不得把 Node 内置模块带进 web bundle", () => {
  const protocolDir = join(repoRoot, "clients", "app", "src", "protocol");
  const barrelPath = join(protocolDir, "index.ts");

  /** `src/protocol/index.ts` 里全部**运行期**（非 `export type`）的 `from "…"` 目标。 */
  function runtimeReexportTargets(source: string): string[] {
    const targets: string[] = [];
    const pattern = /(?:^|\n)\s*export\s+(?!type\b)[^;]*?\bfrom\s+["']([^"']+)["']/g;
    for (const match of source.matchAll(pattern)) {
      const target = match[1];
      if (target !== undefined) targets.push(target);
    }
    return targets;
  }

  /** 一个模块里全部**运行期**（非 `import type`）import 的说明符。 */
  function runtimeImportSpecifiers(source: string): string[] {
    const specifiers: string[] = [];
    const patterns = [
      /(?:^|\n)\s*import\s+(?!type\b)[^;]*?\bfrom\s+["']([^"']+)["']/g,
      /(?:^|\n)\s*(?:import|export)\s*["']([^"']+)["']/g,
      /(?:^|\n)\s*import\s*\(\s*["']([^"']+)["']\s*\)/g,
    ];
    for (const pattern of patterns) {
      for (const match of source.matchAll(pattern)) {
        const specifier = match[1];
        if (specifier !== undefined) specifiers.push(specifier);
      }
    }
    return specifiers;
  }

  /**
   * 从 barrel 的运行期重导出出发，按**运行期** import 做传递闭包，收集触达的 `node:*` 模块。
   * 只查直接重导出会漏掉 `./event → ./x → node:fs` 这类一步之遥的牵连。
   */
  function nodeBuiltinsReachableFromBarrel(): string[] {
    const reachable = new Set<string>();
    const seen = new Set<string>([barrelPath]);
    const queue = runtimeReexportTargets(readFileSync(barrelPath, "utf8")).map((target) =>
      resolve(protocolDir, target),
    );
    while (queue.length > 0) {
      const file = queue.pop();
      if (file === undefined) continue;
      const source = readFileSync(`${file}.ts`, "utf8");
      for (const specifier of runtimeImportSpecifiers(source)) {
        if (specifier.startsWith("node:")) {
          reachable.add(specifier);
          continue;
        }
        if (!specifier.startsWith(".")) continue;
        const next = resolve(protocolDir, specifier);
        if (seen.has(next)) continue;
        seen.add(next);
        queue.push(next);
      }
    }
    return [...reachable].sort();
  }

  it("barrel 的运行期重导出只含不依赖 node:* 的模块（含传递闭包）", () => {
    expect(runtimeReexportTargets(readFileSync(barrelPath, "utf8"))).toEqual(["./event"]);
    expect(nodeBuiltinsReachableFromBarrel()).toEqual([]);
  });

  it("类型面仍从 barrel 公开（`export type *` 保留）", () => {
    const barrel = readFileSync(barrelPath, "utf8");
    for (const layer of ["common", "auth", "sync", "command", "event", "error", "pairing"]) {
      expect(barrel).toContain(`export type * from "./${layer}"`);
    }
    // 读仓库根文件的两个入口只能以**类型**形式出现（`export type` 被 TS 变换整体擦除），
    // 不得有任何运行期重导出——否则 Node 内置模块会被带进 web bundle。
    expect(runtimeReexportTargets(barrel)).not.toContain("./paths");
    expect(runtimeReexportTargets(barrel)).not.toContain("./manifest");
    expect(barrel).toContain('export type { FixtureCase, FixtureManifest, ResolvedFixtureCase } from "./manifest"');
  });
});
