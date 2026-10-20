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
import type {
  CommandBody,
  CommandName,
  SessionCreatePayload,
  SessionReadInclude,
  WorkspaceBrowseEntry,
  WorkspaceBrowseResult,
} from "./command";
import type { AuthAuthenticated } from "./auth";
import type { WorkspaceRef } from "./common";
import type { EventOriginKind, FileChangedDiff, FileChangedFields, ToolCallFields, ToolCallStartedView } from "./event";
import type { SnapshotItemHistory, SnapshotItemWorkspaces } from "./sync";

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
 * 联合型的**全部**键（`keyof` 对联合取交集，因此不能用它枚举 XOR 联合的键：两侧独有的键会互相
 * 消掉，正是本文件最需要盯住的那两个——`directoryId`/`workspacePath`。
 */
type UnionKeys<T> = T extends unknown ? keyof T : never;

/**
 * `CommandName` 词表的编译期镜像（18 条 Sync 面命令）。
 *
 * 与 `INCLUDE_MIRROR` 同理：多一个或少一个取值都会让本文件编译失败，运行期那句比对的是
 * 「TS 联合 ↔ `command.schema.json` 的 `commandName` enum」这一对。新增/删除命令时既要在
 * `compatibility/commands/v1/commands.json` 登记，也要同步 schema 与本联合。
 */
const COMMAND_NAME_MIRROR: Record<CommandName, true> = {
  "session.list": true,
  "session.read": true,
  "command.status": true,
  "session.mode.list": true,
  "session.config.list": true,
  "workspace.browse": true,
  "workspace.mkdir": true,
  "session.prompt": true,
  "session.cancel": true,
  "elicitation.respond": true,
  "session.mode.set": true,
  "session.config.set": true,
  "permission.resolve": true,
  "session.create": true,
  "session.files.list": true,
  "session.files.read": true,
  "session.git.status": true,
  "session.git.diff": true,
};

/**
 * `SessionCreatePayload` 的键集合的编译期镜像（联合两支的键**并**在一起）。
 *
 * 运行期用它比对 schema 的 `payload.properties`：schema 再增删一个键而镜像没跟上时断言变红。
 */
const SESSION_CREATE_PAYLOAD_MIRROR: Record<UnionKeys<SessionCreatePayload>, true> = {
  agentId: true,
  directoryId: true,
  workspacePath: true,
  createIfMissing: true,
};

/** `FileChangedView` 的**命名字段**集合的编译期镜像（开放对象尾巴 `OpenView` 不算字段）。 */
const FILE_CHANGED_FIELD_MIRROR: Record<keyof FileChangedFields, true> = {
  changeId: true,
  kind: true,
  displayPath: true,
  summary: true,
  addedLines: true,
  deletedLines: true,
  outsideWorkspace: true,
};

/**
 * `tool.call.*` 的命名字段集合的编译期镜像（`toolDetail` 是唯一可选键）。
 *
 * 运行期与 `event-views.schema.json` 的三个 `$defs` 比对：schema 增删键而镜像没跟上时断言变红。
 */
const TOOL_CALL_FIELD_MIRROR: Record<keyof ToolCallFields, true> = {
  toolCallId: true,
  turnId: true,
  title: true,
  state: true,
  toolDetail: true,
};

/**
 * `file.changed` 差异行面的键集合的编译期镜像。
 *
 * `FileChangedDiff` 的每一支都把另一支的键声明成 `never`（互斥在编译期成立），而 `keyof` 对联合
 * 取交集后仍是这两个键，因此这里能双向钉住：少一个键或改名都会让本文件编译失败。
 */
const FILE_CHANGED_DIFF_KEY_MIRROR: Record<keyof FileChangedDiff, true> = {
  diffLines: true,
  diffUnavailable: true,
};

/** `WorkspaceCatalogEntry` 的键集合的编译期镜像（`directoryId` 与 `path` 都是必填键）。 */
const WORKSPACE_CATALOG_KEY_MIRROR: Record<keyof SnapshotItemWorkspaces, true> = {
  directoryId: true,
  path: true,
};

/**
 * `WorkspaceRef`（会话摘要的目录引用）的键集合的编译期镜像。
 *
 * 它必须**只有**标识：路径只随快照的 `workspaces` 条目下发，MUST NOT 复制到会话摘要里。
 */
const WORKSPACE_REF_KEY_MIRROR: Record<keyof WorkspaceRef, true> = {
  directoryId: true,
};

/**
 * 编译期：`command.body` 联合的判别字段与 `CommandName` **双向**相等。
 *
 * 少一支（schema 加了命令而镜像没补 body 分支）或多一支（镜像凭空造了命令）都会让这个条件类型
 * 退化成 `false`，于是下面的 `= true` 赋不进去、编译失败。
 */
type CommandBodyCoversNames = CommandName extends CommandBody["command"]
  ? CommandBody["command"] extends CommandName
    ? true
    : false
  : false;

/** 运行期可见的那一半：类型是 `true`，`= true` 因此才成立。 */
const COMMAND_BODY_COVERS_NAMES: CommandBodyCoversNames = true;

/** `WorkspaceBrowseEntry` 的键集合的编译期镜像。 */
const WORKSPACE_BROWSE_ENTRY_KEY_MIRROR: Record<keyof WorkspaceBrowseEntry, true> = {
  name: true,
  path: true,
  registered: true,
};

/** `WorkspaceBrowseResult` 的键集合的编译期镜像。 */
const WORKSPACE_BROWSE_RESULT_KEY_MIRROR: Record<keyof WorkspaceBrowseResult, true> = {
  path: true,
  entries: true,
  truncated: true,
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

/** 从 `unknown` 对象按属性名读取；不是对象时返回 `undefined`（schema 是外部 JSON，逐层确认）。 */
function schemaField(value: unknown, key: string): unknown {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  return Object.getOwnPropertyDescriptor(value, key)?.value;
}

/** 读取一个必须是对象的 schema 字段；形状不对时直接失败，后续断言才有意义。 */
function schemaObjectField(value: unknown, key: string): object {
  const field = schemaField(value, key);
  if (typeof field !== "object" || field === null || Array.isArray(field)) {
    throw new Error(`schema 字段 ${key} 不是对象`);
  }
  return field;
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

  it("sessionCreate 的 payload 键集与镜像逐项相等，且 createIfMissing 只与 workspacePath 相容", () => {
    const schema = readSchema("command.schema.json");
    const create = defs(schema)["sessionCreate"] as {
      properties: {
        payload: {
          required: string[];
          additionalProperties: boolean;
          oneOf: unknown;
          not: unknown;
          properties: Record<string, unknown>;
        };
      };
    };
    const payload = create.properties.payload;
    // 期望值从 schema 读出，再与「编译期钉住的镜像」比对：schema 增删一个键而镜像没跟上时这里变红。
    expect(payload.required).toEqual(["agentId"]);
    expect(payload.additionalProperties).toBe(false);
    expect(Object.keys(payload.properties).sort()).toEqual(Object.keys(SESSION_CREATE_PAYLOAD_MIRROR).sort());
    // 目录来源**恰好一个**：schema 用两支 oneOf 各要一个键表达，镜像用联合的两支表达同一件事。
    expect(payload.oneOf).toEqual([{ required: ["directoryId"] }, { required: ["workspacePath"] }]);
    // `createIfMissing` 与 `directoryId` 互斥（镜像里前者只出现在 workspacePath 那一支）。
    expect(payload.not).toEqual({ required: ["directoryId", "createIfMissing"] });
    expect(payload.properties["createIfMissing"]).toMatchObject({ type: "boolean" });
    expect(payload.properties["workspacePath"]).toMatchObject({ type: "string", maxLength: 4096 });
  });

  it("file.changed 的键集与镜像逐项相等，差异行与不可用标记互斥", () => {
    const schema = readSchema("event-views.schema.json");
    const changed = defs(schema)["file.changed"] as {
      required: string[];
      not: unknown;
      properties: Record<string, unknown>;
    };
    // 必填集合一字未动：三个新键都是**可选**的，未协商 feature 的客户端不该被拒收。
    expect(changed.required).toEqual(["changeId", "kind", "displayPath", "summary"]);
    // 期望值从 schema 读出，再与镜像的键集比对（命名字段七个 + 差异行面两个）。
    expect(Object.keys(changed.properties).sort()).toEqual(
      [...Object.keys(FILE_CHANGED_FIELD_MIRROR), ...Object.keys(FILE_CHANGED_DIFF_KEY_MIRROR)].sort(),
    );
    // 互斥：镜像里两支各把对方的键声明成 `never`，与 schema 的 `not` 表达同一件事。
    expect(changed.not).toEqual({ required: ["diffLines", "diffUnavailable"] });

    const diffLines = changed.properties["diffLines"] as {
      minItems: number;
      maxItems: number;
      items: { properties: { kind: { enum: string[] }; text: { maxLength: number } } };
    };
    // 键在场即至少一行；行数上限 400、单行上限 4096 字符都是固定 v1 常量（`SYNC_PROTOCOL.md` §14）。
    expect(diffLines.minItems).toBe(1);
    expect(diffLines.maxItems).toBe(400);
    expect(diffLines.items.properties.kind.enum).toEqual(["added", "deleted", "context"]);
    expect(diffLines.items.properties.text.maxLength).toBe(4096);

    const unavailable = changed.properties["diffUnavailable"] as {
      required: string[];
      properties: { reason: { enum: string[] } };
    };
    expect(unavailable.required).toEqual(["reason"]);
    // `reason` 是封闭词表：v1 只有 `size_limit` 一档，镜像的 `FileDiffUnavailableReason` 与它同值域。
    expect(unavailable.properties.reason.enum).toEqual(["size_limit"]);
  });

  it("tool.call.* 的命名字段含可选 toolDetail，三个视图形状一致", () => {
    const schema = readSchema("event-views.schema.json");
    const defsMap = defs(schema);
    for (const eventType of ["tool.call.started", "tool.call.updated", "tool.call.completed"] as const) {
      const view = defsMap[eventType];
      const viewProperties = schemaObjectField(view, "properties");
      // 新增字段是**可选**的：必填集合一字未动，旧服务端的事件仍被接受。
      expect(schemaField(view, "required")).toEqual(["toolCallId", "turnId", "title", "state"]);
      expect(Object.keys(viewProperties).sort()).toEqual(Object.keys(TOOL_CALL_FIELD_MIRROR).sort());
      expect(schemaField(view, "additionalProperties")).toBe(true);

      const toolDetail = schemaObjectField(viewProperties, "toolDetail");
      expect(schemaField(toolDetail, "type")).toBe("object");
      expect(schemaField(toolDetail, "minProperties")).toBe(1);
      // 未知键按值保留：客户端只读已知部分，不因未来新增字段拒收。
      expect(schemaField(toolDetail, "additionalProperties")).toBe(true);
      const detailProperties = schemaObjectField(toolDetail, "properties");
      expect(Object.keys(detailProperties).sort()).toEqual(["input", "output", "text", "truncatedParts"]);
      // `input`/`output` 在场时不得是 JSON `null`：缺席才是「没有提供」。
      expect(schemaField(detailProperties, "input")).toEqual({ not: { type: "null" } });
      expect(schemaField(detailProperties, "output")).toEqual({ not: { type: "null" } });
      const text = schemaObjectField(detailProperties, "text");
      expect(schemaField(text, "type")).toBe("array");
      expect(schemaField(text, "minItems")).toBe(1);
      expect(schemaField(schemaObjectField(text, "items"), "type")).toBe("string");
      const truncated = schemaObjectField(detailProperties, "truncatedParts");
      expect(schemaField(truncated, "type")).toBe("array");
      expect(schemaField(truncated, "minItems")).toBe(1);
      expect(schemaField(truncated, "uniqueItems")).toBe(true);
      expect(schemaField(schemaObjectField(truncated, "items"), "enum")).toEqual(["input", "output", "text"]);
    }
  });

  it("commandName 词表与镜像逐项相等，body.oneOf 覆盖全部 18 条 Sync 命令", () => {
    const schema = readSchema("command.schema.json");
    const defsMap = defs(schema);
    const commandName = defsMap["commandName"] as { enum: string[] };
    expect([...commandName.enum].sort()).toEqual(Object.keys(COMMAND_NAME_MIRROR).sort());
    expect(Object.keys(COMMAND_NAME_MIRROR)).toContain("workspace.browse");
    // 编译期：`body` 联合的判别字段与 `CommandName` 双向相等（少一支或多一支都赋不进 `true`）。
    expect(COMMAND_BODY_COVERS_NAMES).toBe(true);

    // 命令名从 `body.oneOf` 的每个 `$ref` 解出来：期望值由 schema 派生，不抄常量表。
    const command = defsMap["command"] as {
      properties: { body: { oneOf: { $ref: string }[] } };
    };
    const names = command.properties.body.oneOf.map((entry) => {
      const branch = defsMap[entry.$ref.slice("#/$defs/".length)] as {
        properties: { command: { const: string } };
      };
      return branch.properties.command.const;
    });
    expect([...names].sort()).toEqual(Object.keys(COMMAND_NAME_MIRROR).sort());
  });

  it("快照 workspaces 条目是 workspaceCatalogEntry：directoryId + 必填 path，workspaceRef 只有标识", () => {
    const syncSchema = readSchema("sync.schema.json");
    const item = defs(syncSchema)["snapshotItem.workspaces"] as { $ref: string };
    expect(item.$ref).toBe("common.schema.json#/$defs/workspaceCatalogEntry");

    const commonSchema = readSchema("common.schema.json");
    const catalog = defs(commonSchema)["workspaceCatalogEntry"] as {
      required: string[];
      additionalProperties: boolean;
      properties: Record<string, unknown>;
    };
    expect(catalog.required).toEqual(["directoryId", "path"]);
    expect(catalog.additionalProperties).toBe(false);
    expect(Object.keys(catalog.properties).sort()).toEqual(Object.keys(WORKSPACE_CATALOG_KEY_MIRROR).sort());
    // `path` 是必填、非空的规范化路径，没有 `null` 这一档，也不受 feature 门控。
    expect(catalog.properties["path"]).toMatchObject({ type: "string", minLength: 1, maxLength: 4096 });

    // 会话摘要的目录引用只含标识：路径不在那里重复（镜像的 `WorkspaceRef` 也只有一键）。
    const ref = defs(commonSchema)["workspaceRef"] as { required: string[]; properties: Record<string, unknown> };
    expect(ref.required).toEqual(["directoryId"]);
    expect(Object.keys(ref.properties).sort()).toEqual(Object.keys(WORKSPACE_REF_KEY_MIRROR).sort());
  });

  it("workspace.browse 的请求与结果形状与镜像一致", () => {
    const defsMap = defs(readSchema("command.schema.json"));

    const browsePath = defsMap["workspaceBrowsePath"] as { type: string; minLength: number; maxLength: number };
    expect(browsePath.type).toBe("string");
    expect(browsePath.minLength).toBe(1);
    expect(browsePath.maxLength).toBe(4096);

    const browse = defsMap["workspaceBrowse"] as {
      required: string[];
      additionalProperties: boolean;
      properties: {
        command: unknown;
        payload: { required: string[]; additionalProperties: boolean; properties: Record<string, unknown> };
      };
    };
    expect(browse.required).toEqual(["requestId", "command", "payload"]);
    expect(browse.properties.command).toEqual({ const: "workspace.browse" });
    // 请求**只含**一个父路径字段，且该键必填：`null`（根列表）不能靠缺省表达。
    expect(browse.properties.payload.required).toEqual(["path"]);
    expect(browse.properties.payload.additionalProperties).toBe(false);
    expect(Object.keys(browse.properties.payload.properties)).toEqual(["path"]);
    expect(browse.properties.payload.properties["path"]).toEqual({
      oneOf: [{ $ref: "#/$defs/workspaceBrowsePath" }, { type: "null" }],
    });

    const entry = defsMap["workspaceBrowseEntry"] as { required: string[]; properties: Record<string, unknown> };
    expect(entry.required).toEqual(["name", "path", "registered"]);
    expect(Object.keys(entry.properties).sort()).toEqual(Object.keys(WORKSPACE_BROWSE_ENTRY_KEY_MIRROR).sort());

    const result = defsMap["workspaceBrowseResult"] as {
      required: string[];
      // 键集合是运行时从 schema 读出来的（`entries` / `path` / `truncated`），
      // 只有 `entries` 需要具体形状；其余键按任意键取，避免为每个键再抄一遍断言类型。
      properties: Record<string, unknown> & { entries: { maxItems: number; items: { $ref: string } } };
    };
    expect([...result.required].sort()).toEqual(Object.keys(WORKSPACE_BROWSE_RESULT_KEY_MIRROR).sort());
    // 单层条目上限是固定 v1 常量 128（`SYNC_PROTOCOL.md` §14）：超限时回上限内条目并置 `truncated`。
    expect(result.properties.entries.maxItems).toBe(128);
    expect(result.properties.entries.items.$ref).toBe("#/$defs/workspaceBrowseEntry");
    // 结果里的 `path` 与请求共用同一份定义 + `null`：逐字回显本次请求的父路径。
    expect(result.properties["path"]).toEqual({
      oneOf: [{ $ref: "#/$defs/workspaceBrowsePath" }, { type: "null" }],
    });

    // `completed` 的结果与命令名的绑定，以及「查询类命令没有 terminalEventId」这条既有口径。
    const commandResult = defsMap["commandResult"] as {
      properties: { body: { allOf: { if?: unknown; then?: { properties?: Record<string, unknown> } }[] } };
    };
    const allOf = commandResult.properties.body.allOf.map((branch) => ({
      if: branch.if as { properties?: { command?: { const?: string; enum?: string[] } } } | undefined,
      then: branch.then,
    }));
    const bound = allOf.find((branch) => branch.if?.properties?.command?.const === "workspace.browse");
    expect(bound?.then?.properties?.["result"]).toEqual({ $ref: "#/$defs/workspaceBrowseResult" });
    const query = allOf.find((branch) => branch.if?.properties?.command?.enum?.includes("workspace.browse") === true);
    expect(query?.then?.properties?.["terminalEventId"]).toEqual({ type: "null" });
  });

  it("session.files.* / session.git.* 的请求与结果形状与镜像一致", () => {
    const defsMap = defs(readSchema("command.schema.json"));

    // 根内相对路径：上限 4096，且 pattern 拒绝父引用/盘符/控制字符与 `.git` 分量。
    const relativePath = defsMap["sessionRelativePath"] as { type: string; minLength: number; maxLength: number; pattern: string };
    expect(relativePath.type).toBe("string");
    expect(relativePath.minLength).toBe(1);
    expect(relativePath.maxLength).toBe(4096);
    expect(relativePath.pattern).toContain("[gG][iI][tT]");

    const list = defsMap["sessionFilesList"] as {
      required: string[];
      properties: { command: unknown; payload: { required: string[]; additionalProperties: boolean; properties: Record<string, unknown> } };
    };
    expect(list.required).toEqual(["requestId", "command", "sessionId", "payload"]);
    expect(list.properties.command).toEqual({ const: "session.files.list" });
    expect(list.properties.payload.required).toEqual(["path", "hideIgnored"]);
    expect(list.properties.payload.additionalProperties).toBe(false);
    expect(Object.keys(list.properties.payload.properties).sort()).toEqual(["after", "hideIgnored", "path"]);

    const read = defsMap["sessionFilesRead"] as {
      properties: { command: unknown; payload: { required: string[]; additionalProperties: boolean; properties: Record<string, unknown> } };
    };
    expect(read.properties.command).toEqual({ const: "session.files.read" });
    expect(read.properties.payload.required).toEqual(["path", "purpose", "offset"]);
    expect(read.properties.payload.additionalProperties).toBe(false);
    expect(Object.keys(read.properties.payload.properties).sort()).toEqual(["offset", "path", "purpose", "version"]);
    expect(defsMap["sessionFilesReadPurpose"]).toEqual({ enum: ["preview", "download"] });

    const gitStatus = defsMap["sessionGitStatus"] as {
      properties: {
        command: unknown;
        payload: { required?: string[]; additionalProperties: boolean; properties: Record<string, unknown> };
      };
    };
    expect(gitStatus.properties.command).toEqual({ const: "session.git.status" });
    // 空 payload 的 `required` 键可以缺省（schema 不写 `required: []`）：两种写法都表示「无必填键」。
    expect(gitStatus.properties.payload.required ?? []).toEqual([]);
    expect(Object.keys(gitStatus.properties.payload.properties)).toEqual(["after"]);

    const gitDiff = defsMap["sessionGitDiff"] as {
      properties: { command: unknown; payload: { required: string[]; additionalProperties: boolean; properties: Record<string, unknown> } };
    };
    expect(gitDiff.properties.command).toEqual({ const: "session.git.diff" });
    expect(gitDiff.properties.payload.required).toEqual(["path"]);
    expect(Object.keys(gitDiff.properties.payload.properties)).toEqual(["path"]);

    const listResult = defsMap["sessionFilesListResult"] as { required: string[]; properties: Record<string, unknown> };
    expect([...listResult.required].sort()).toEqual(["entries", "gitState", "path"]);
    expect(schemaField(defsMap["gitState"], "enum")).toEqual(["available", "not_repository", "unavailable"]);
    expect(defsMap["sessionFileEntryKind"]).toEqual({ enum: ["file", "directory", "link", "other"] });
    expect(defsMap["sessionFileRestrictedReason"]).toEqual({ enum: ["outside_root", "unreadable", "unsupported"] });
    expect(defsMap["sessionFileContentKind"]).toEqual({ enum: ["text", "image", "binary"] });

    const readResult = defsMap["sessionFilesReadResult"] as { required: string[] };
    expect([...readResult.required].sort()).toEqual([
      "dataBase64",
      "eof",
      "kind",
      "offset",
      "path",
      "previewTruncated",
      "sizeBytes",
      "version",
    ]);
    expect(defsMap["gitFileStatus"]).toEqual({
      enum: ["added", "modified", "deleted", "renamed", "untracked"],
    });
    expect(defsMap["gitDiffBase"]).toEqual({ enum: ["head", "empty"] });
    expect(defsMap["gitDiffKind"]).toEqual({ enum: ["text", "binary", "conflict"] });
    expect(defsMap["gitDiffUnavailableReason"]).toEqual({
      enum: ["not_repository", "git_unavailable", "too_large"],
    });

    const statusResult = defsMap["sessionGitStatusResult"] as { required: string[] };
    expect([...statusResult.required].sort()).toEqual(["entries", "gitState"]);
    const diffResult = defsMap["sessionGitDiffResult"] as { required: string[] };
    expect([...diffResult.required].sort()).toEqual(["base", "kind", "path", "truncated"]);

    // completed 结果与命令名绑定，且四条查询都没有 terminalEventId。
    const commandResult = defsMap["commandResult"] as {
      properties: { body: { allOf: { if?: unknown; then?: { properties?: Record<string, unknown> } }[] } };
    };
    const allOf = commandResult.properties.body.allOf.map((branch) => ({
      if: branch.if as { properties?: { command?: { const?: string; enum?: string[] } } } | undefined,
      then: branch.then,
    }));
    const bindings: readonly [string, string][] = [
      ["session.files.list", "sessionFilesListResult"],
      ["session.files.read", "sessionFilesReadResult"],
      ["session.git.status", "sessionGitStatusResult"],
      ["session.git.diff", "sessionGitDiffResult"],
    ];
    for (const [command, resultDef] of bindings) {
      const bound = allOf.find((branch) => branch.if?.properties?.command?.const === command);
      expect(bound?.then?.properties?.["result"]).toEqual({ $ref: `#/$defs/${resultDef}` });
      const query = allOf.find((branch) => branch.if?.properties?.command?.enum?.includes(command) === true);
      expect(query?.then?.properties?.["terminalEventId"]).toEqual({ type: "null" });
    }
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

  it("tool.call.* 的 toolDetail 可缺席，在场时原样透传（含未知字段）", () => {
    // 编译期：构造一个显式类型的视图——`toolDetail` 可选，已知部分按声明类型，未知键由 OpenView 保留。
    const viewWithDetail: ToolCallStartedView = {
      toolCallId: "tc-1",
      turnId: "t-1",
      title: "读取文件",
      state: "running",
      toolDetail: {
        input: { path: "src/a.ts" },
        text: ["第一块"],
        truncatedParts: ["output"],
        futureField: { nested: true },
      },
    };
    const projected = projectView("tool.call.started", viewWithDetail);
    if (projected.kind !== "known") throw new Error("tool.call.started 应被识别");
    // `projectView` 只做 eventType 判别，不校验字段：`toolDetail`（含未知键）按值留存。
    const view: Readonly<Record<string, unknown>> = projected.view;
    expect(view["toolDetail"]).toEqual(viewWithDetail.toolDetail);

    const withoutDetail: ToolCallStartedView = {
      toolCallId: "tc-1",
      turnId: "t-1",
      title: "读取文件",
      state: "running",
    };
    expect(projectView("tool.call.updated", withoutDetail).kind).toBe("known");
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
    expect(runtimeReexportTargets(readFileSync(barrelPath, "utf8"))).toEqual([
      "./event",
      "./session-files",
      "./attachment",
      "./media",
      "./json-number",
      "./device-qr",
    ]);
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
