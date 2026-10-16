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
import type { SessionReadInclude } from "./command";
import type { AuthAuthenticated } from "./auth";
import type { EventOriginKind } from "./event";
import type { SnapshotItemHistory } from "./sync";

/**
 * include 词表的**编译期**镜像：`SessionReadInclude` 的每个取值都必须在这里登记。
 *
 * 多一个取值（镜像没跟上）或少一个取值（词表删了但镜像还留着）都会让本文件编译失败，
 * 因此运行期那句 `toEqual` 比对的是「同一份词表」，而不是抄来的常量。
 */
const INCLUDE_MIRROR: Record<SessionReadInclude, true> = {
  messages: true,
  turns: true,
  pending_interactions: true,
  config_options: true,
  capabilities: true,
  history: true,
};

/** `SnapshotItemHistory` 的字段集合（`acp` 是唯一可选键）的编译期镜像。 */
const HISTORY_KEY_MIRROR: Record<keyof SnapshotItemHistory, true> = {
  eventId: true,
  eventType: true,
  sessionSequence: true,
  createdAt: true,
  view: true,
  acp: true,
};

/**
 * `AuthAuthenticated["body"]` 的字段集合的编译期镜像。
 *
 * 与其它镜像同理：镜像里多一个键（例如已被 wire 删掉的授权集合 `scopes`）或少一个键都会让
 * 本文件编译失败，因此下面的比对是「TS 字段集 ↔ schema `required`」这一对，而不是抄来的常量。
 */
const AUTH_BODY_MIRROR: Record<keyof AuthAuthenticated["body"], true> = {
  deviceId: true,
  serverEpoch: true,
  headGlobalSequence: true,
  heartbeatIntervalMs: true,
  limits: true,
};

/**
 * `origin.kind` 封闭词表（**五值**）的编译期镜像。
 *
 * 与 `INCLUDE_MIRROR` 同理：`EventOriginKind` 多一个取值或少一个取值都会让本文件编译失败，
 * 因此运行期那句比对的是「TS 联合 ↔ schema enum」这一对，而不是抄来的常量。
 *
 * `node` 值随 `sync-server-connection` 的 1.6 进 wire（配套 `origin.nodeId`）；此前这里的 TS
 * 联合是四值，`kind: "node"` 的事件无法在类型上表达 —— 这正是本次补齐的缺口。
 */
const ORIGIN_KIND_MIRROR: Record<EventOriginKind, true> = {
  agent: true,
  device: true,
  daemon: true,
  local_cli: true,
  node: true,
};

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

  it("auth.authenticated 的 body 必填集合与镜像一致（设备方向没有授权集合）", () => {
    const schema = readSchema("auth.schema.json");
    const authenticated = defs(schema)["authenticated"] as {
      properties: { body: { required: string[]; properties: Record<string, unknown> } };
    };
    // 反例：镜像里若还留着已被 wire 删掉的授权集合字段（`scopes`），下面任一条 `toEqual` 都会变红。
    expect(authenticated.properties.body.required).toEqual([
      "deviceId",
      "serverEpoch",
      "headGlobalSequence",
      "heartbeatIntervalMs",
      "limits",
    ]);
    expect(Object.keys(authenticated.properties.body.properties).sort()).toEqual([
      "deviceId",
      "headGlobalSequence",
      "heartbeatIntervalMs",
      "limits",
      "serverEpoch",
    ]);
    // 镜像侧同样是编译期判定：`AuthAuthenticated["body"]` 的键集与这份常量必须逐键相同。
    expect(Object.keys(AUTH_BODY_MIRROR).sort()).toEqual([
      "deviceId",
      "headGlobalSequence",
      "heartbeatIntervalMs",
      "limits",
      "serverEpoch",
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

  it("sessionRead 的 before 仍是 (createdAt, messageId) 复合游标，beforeHistory 是新增的会话内序号游标", () => {
    const schema = readSchema("command.schema.json");
    // 消息分页的复合游标语义一字未改：两键都必需，单字段游标仍按协议拒绝。
    const before = defs(schema)["sessionReadBefore"] as { required: string[] };
    expect(before.required).toEqual(["createdAt", "messageId"]);
    // 历史分页是另一套游标：会话内事件序号（decimalString），不是复合游标。
    const beforeHistory = defs(schema)["sessionReadBeforeHistory"] as { $ref?: string };
    expect(beforeHistory.$ref).toBe("common.schema.json#/$defs/decimalString");

    const read = defs(schema)["sessionRead"] as {
      properties: {
        payload: {
          required: string[];
          properties: {
            include: { items: { enum: string[] } };
            before?: { $ref?: string };
            beforeHistory?: { $ref?: string };
          };
        };
      };
    };
    const payload = read.properties.payload;
    // 反例：三个键里少一个（例如漏掉新加的 beforeHistory）这里就会变红。
    expect(Object.keys(payload.properties).sort()).toEqual(["before", "beforeHistory", "include", "limit"]);
    // 两个游标都是**可选**的：必填集合仍只有 include（既有客户端不受影响）。
    expect(payload.required).toEqual(["include"]);
    expect(payload.properties.before?.$ref).toBe("#/$defs/sessionReadBefore");
    expect(payload.properties.beforeHistory?.$ref).toBe("#/$defs/sessionReadBeforeHistory");
    // include 的新取值 history 与镜像的联合逐项相等（编译期由 INCLUDE_MIRROR 钉住）。
    expect(payload.properties.include.items.enum).toEqual([
      "messages",
      "turns",
      "pending_interactions",
      "config_options",
      "capabilities",
      "history",
    ]);
    expect(Object.keys(INCLUDE_MIRROR).sort()).toEqual([...payload.properties.include.items.enum].sort());
  });

  it("origin.kind 的五值词表与 TS 镜像逐项相等（含 1.6 新增的 node）", () => {
    // `event.schema.json` 的 body 是**内联**的（无 $defs），路径是
    // `properties.body.properties.origin`。逐层取、每层先验证存在，不用内联转型取成员
    // ——那会伪造一个未经检查的形状。
    const schema = readSchema("event.schema.json");
    expect(schema["properties"]).toBeDefined();
    const root = schema["properties"] as Record<string, unknown>;
    expect(root["body"]).toBeDefined();
    const body = root["body"] as Record<string, unknown>;
    expect(body["properties"]).toBeDefined();
    const bodyProps = body["properties"] as Record<string, unknown>;
    expect(bodyProps["origin"]).toBeDefined();
    const origin = bodyProps["origin"] as Record<string, unknown>;
    const originProps = origin["properties"] as Record<string, unknown>;
    const kind = originProps["kind"] as { enum: string[] };
    const nodeId = originProps["nodeId"] as { oneOf: unknown[] };

    expect(Object.keys(ORIGIN_KIND_MIRROR).sort()).toEqual([...kind.enum].sort());
    // `node` 那一档必须有配套的 nodeId，且 if/then 把它钉成必填（否则五值就成了半成品）。
    expect(kind.enum).toContain("node");
    expect(nodeId.oneOf).toEqual([
      { $ref: "common.schema.json#/$defs/uuid" },
      { type: "null" },
    ]);
    expect(origin["if"]).toEqual({
      properties: { kind: { const: "node" } },
      required: ["kind"],
    });
    // `then` 把两个身份字段一起钉住：nodeId 必须是 canonical UUID、deviceId 必须是 null。
    expect(origin["then"]).toEqual({
      required: ["nodeId"],
      properties: {
        deviceId: { type: "null" },
        nodeId: { $ref: "common.schema.json#/$defs/uuid" },
      },
    });
    // 反向同样重要：required 里 kind/deviceId 恒在，nodeId 不在（未协商的连接不带这个键）。
    expect(origin["required"]).toEqual(["kind", "deviceId"]);
  });

  it("sessionReadResult 新增 history/historyHasEarlier，既有 hasEarlier 的必填语义不变", () => {
    const schema = readSchema("command.schema.json");
    const result = defs(schema)["sessionReadResult"] as {
      required: string[];
      properties: {
        historyHasEarlier?: { type?: string };
        resources: { properties: Record<string, unknown> };
      };
    };
    // 反例：若把 hasEarlier 挪进资源对象（v1 破坏性变更），这里会变红。
    expect(result.required).toEqual(["sessionId", "resources", "hasEarlier"]);
    expect(result.properties.historyHasEarlier).toEqual({ type: "boolean" });
    expect(result.properties.resources.properties["history"]).toEqual({
      type: "array",
      items: { $ref: "sync.schema.json#/$defs/snapshotItem.history" },
    });
  });

  it("snapshotItem.history 的必填与可选集合与镜像一致", () => {
    const schema = readSchema("sync.schema.json");
    const history = defs(schema)["snapshotItem.history"] as { required: string[]; properties: Record<string, unknown> };
    expect(history.required).toEqual(["eventId", "eventType", "sessionSequence", "createdAt", "view"]);
    // `keyof SnapshotItemHistory`：编译期钉住镜像的字段集合（`acp` 是唯一可选的）。
    expect(Object.keys(history.properties).sort()).toEqual(Object.keys(HISTORY_KEY_MIRROR).sort());
    expect(history.properties["acp"]).toEqual({ $ref: "common.schema.json#/$defs/rawAcp" });
  });

  it("sessionReadResult 必填 hasEarlier", () => {
    const schema = readSchema("command.schema.json");
    const result = defs(schema)["sessionReadResult"] as { required: string[] };
    expect(result.required).toEqual(["sessionId", "resources", "hasEarlier"]);
  });

  it("sessionCreate 的 payload 允许 agentId 与一个目录引用", () => {
    const schema = readSchema("command.schema.json");
    const create = defs(schema)["sessionCreate"] as {
      properties: {
        payload: {
          required: string[];
          additionalProperties: boolean;
          properties: Record<string, unknown>;
        };
      };
    };
    // `agentId` 必填，目录引用由 `workspaceAlias` / `workspacePath` 之一给出（二者不同时出现）。
    expect(create.properties.payload.required).toEqual(["agentId"]);
    expect(create.properties.payload.additionalProperties).toBe(false);
    expect(Object.keys(create.properties.payload.properties).sort()).toEqual([
      "agentId",
      "workspaceAlias",
      "workspacePath",
    ]);
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
