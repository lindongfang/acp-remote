import { existsSync, readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalogPath = resolve(root, "compatibility", "commands", "v1", "commands.json");
const syncSchemaPath = resolve(root, "schemas", "sync", "v1", "command.schema.json");
const nodeLinkSchemaPath = resolve(root, "schemas", "node-link", "v1", "command.schema.json");
const syncDocPath = resolve(root, "docs", "SYNC_PROTOCOL.md");
const securityDocPath = resolve(root, "docs", "SECURITY_DESIGN.md");
const localAdminDocPath = resolve(root, "docs", "LOCAL_ADMIN_PROTOCOL.md");
const localAdminSchemaPath = resolve(root, "schemas", "local-admin", "v1", "envelope.schema.json");

const errors = [];

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    errors.push(`${relative(root, path)}: invalid JSON: ${error.message}`);
    return undefined;
  }
}

function readText(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    errors.push(`${relative(root, path)}: unreadable: ${error.message}`);
    return undefined;
  }
}

function compareSets(label, actual, wanted) {
  const a = [...actual].sort();
  const w = [...wanted].sort();
  if (JSON.stringify(a) === JSON.stringify(w)) return;
  for (const value of w.filter((v) => !a.includes(v))) errors.push(`${label}: missing ${value}`);
  for (const value of a.filter((v) => !w.includes(v))) errors.push(`${label}: unexpected ${value}`);
}

function collectCommandNames(schema, found = new Set()) {
  const enumValues = schema?.$defs?.commandName?.enum;
  if (Array.isArray(enumValues)) for (const value of enumValues) found.add(value);
  const consts = collectCommandConsts(schema, new Set());
  for (const value of consts) found.add(value);
  return found;
}

function collectCommandConsts(value, found = new Set()) {
  if (!value || typeof value !== "object") return found;
  if (typeof value.command === "string") found.add(value.command);
  if (value.command && typeof value.command === "object" && typeof value.command.const === "string") {
    found.add(value.command.const);
  }
  for (const child of Object.values(value)) collectCommandConsts(child, found);
  return found;
}

function tableCommands(text, heading) {
  const start = text.indexOf(heading);
  if (start < 0) {
    errors.push(`missing section ${heading}`);
    return new Set();
  }
  const rest = text.slice(start + heading.length);
  const nextHeading = rest.search(/^### /m);
  const section = nextHeading < 0 ? rest : rest.slice(0, nextHeading);
  const names = new Set();
  for (const line of section.split(/\r?\n/)) {
    const match = line.match(/^\|\s*`([a-z][a-z0-9_.:-]*)`\s*\|/);
    if (match) names.add(match[1]);
  }
  return names;
}

// 递归收集 JSON 里全部对象键与字符串叶子（附各自的路径），用于「已删除词汇不得回潮」的扫描：
// 键名与取值都只看**字面量**，因此不会因为嵌套位置不同而漏判。
function vocabulary(value, path = "$", found = []) {
  if (typeof value === "string") {
    found.push([path, value]);
    return found;
  }
  if (Array.isArray(value)) {
    for (const [index, item] of value.entries()) vocabulary(item, `${path}[${index}]`, found);
    return found;
  }
  if (value && typeof value === "object") {
    for (const [key, child] of Object.entries(value)) {
      found.push([`${path}.${key}`, key]);
      vocabulary(child, `${path}.${key}`, found);
    }
  }
  return found;
}

// 本地管理通道的封闭词表：方法名与本地错误码。它们**不属于** commands.json（那些是 WSS 业务命令），
// 也不属于 compatibility/errors/v1/errors.json（它只登记 WSS wire 上的错误码）。机器定义因此放在
// `schemas/local-admin/v1/envelope.schema.json`：本文档的 `#### `method`` 小节与 §6 表格是它的说明，
// 两者一致由本函数断言；framing 常量与信封版本号同样在这里对照。
function sliceSection(text, startHeading, endHeading) {
  const start = text.indexOf(startHeading);
  if (start < 0) {
    errors.push(`docs/LOCAL_ADMIN_PROTOCOL.md: missing section ${startHeading}`);
    return "";
  }
  const rest = text.slice(start + startHeading.length);
  const stop = rest.indexOf(endHeading);
  return stop < 0 ? rest : rest.slice(0, stop);
}

function checkLocalAdmin(doc, catalog) {
  const schema = readJson(localAdminSchemaPath);
  if (!schema) return;

  const methods = new Set();
  // 方法名允许段内连字符（文档 §4 的方法名正则），因此小节标题的抽取同样要能捕获连字符名字。
  for (const match of doc.matchAll(/^#### `([a-z][a-z0-9]*(?:\.[a-z0-9]+(-[a-z0-9]+)*)*)`/gm)) methods.add(match[1]);
  compareSets(
    "docs/LOCAL_ADMIN_PROTOCOL.md 方法小节",
    methods,
    schema.$defs?.methodName?.enum ?? [],
  );

  const capabilitySection = sliceSection(doc, "### 5.1 ", "### 5.2 ");
  const capabilities = new Set(
    [...capabilitySection.matchAll(/`(local\.[a-z0-9.-]+)`/g)].map((match) => match[1]),
  );
  compareSets(
    "compatibility/commands/v1/commands.json localCapabilities",
    capabilities,
    catalog.localCapabilities ?? [],
  );
  for (const match of capabilitySection.matchAll(/`([a-z][a-z0-9]*(?:\.[a-z0-9]+(-[a-z0-9]+)*)+)`/g)) {
    if (match[1].startsWith("local.")) continue; // `local.*` 是能力名，不是方法名
    if (/\.(json|md|mjs|rs|toml|sh)$/.test(match[1])) continue;
    if (!methods.has(match[1])) {
      errors.push(`docs/LOCAL_ADMIN_PROTOCOL.md §5.1: 表内方法 ${match[1]} 没有对应小节`);
    }
  }

  const codeSection = sliceSection(doc, "## 6. 本地错误码", "## 7. ");
  const codes = new Set(
    [...codeSection.matchAll(/^\|\s*`(local\.[a-z_]+)`\s*\|/gm)].map((match) => match[1]),
  );
  compareSets("docs/LOCAL_ADMIN_PROTOCOL.md §6", codes, schema.$defs?.errorCode?.enum ?? []);

  const framing = schema.$defs?.framing?.const ?? {};
  const equal = (label, actual, wanted) => {
    if (actual !== wanted) errors.push(`${label}: ${actual} != ${wanted}`);
  };
  equal("schemas/local-admin/v1 framing.lengthPrefixBytes", framing.lengthPrefixBytes, 4);
  equal("schemas/local-admin/v1 framing.maxPayloadBytes", framing.maxPayloadBytes, 1048576);
  equal("schemas/local-admin/v1 framing.maxInFlightRequests", framing.maxInFlightRequests, 32);
  equal("schemas/local-admin/v1 framing.channelLocalAdmin", framing.channelLocalAdmin, 1);
  equal("schemas/local-admin/v1 framing.channelAcpStream", framing.channelAcpStream, 2);
  if (!/u32be length/.test(doc)) errors.push("docs/LOCAL_ADMIN_PROTOCOL.md: 缺少 u32be length framing 描述");
  if (!/1_048_576/.test(doc)) errors.push("docs/LOCAL_ADMIN_PROTOCOL.md: 缺少帧上限 1_048_576");
  for (const branch of ["request", "successResponse", "errorResponse"]) {
    equal(`schemas/local-admin/v1 ${branch}.v`, schema.$defs?.[branch]?.properties?.v?.const, 1);
  }
}

const catalog = readJson(catalogPath);
let commandTotal = 0;

if (catalog) {
  const names = new Set();
  for (const command of catalog.commands ?? []) {
    if (!command.name || names.has(command.name)) errors.push(`commands.json: missing/duplicate command ${command.name}`);
    names.add(command.name);
    if (!Array.isArray(command.transport) || command.transport.length === 0) {
      errors.push(`commands.json: ${command.name} has empty transport`);
    }
    if (command.kind !== "query" && command.kind !== "mutation") {
      errors.push(`commands.json: ${command.name} has invalid kind ${command.kind}`);
    }
    if (command.name !== "session.create" && command.scope !== command.name) {
      errors.push(`commands.json: ${command.name} scope must equal command name`);
    }
  }
  commandTotal = names.size;

  // 设备权限层已删除（`pairing-model`）：命令登记表只剩两处授权词汇——跨节点导出授权 `grants` 与本地
  // 管理能力 `localCapabilities`。命令条目不再有 `pack` 字段，顶层不再有 `packs`/`presets` 两个输入
  // 映射。这一条是「不得回潮」的判据：资产删掉之后它仍然会因为词汇重新出现而独立变红（不是空断言）。
  const removedKeys = new Set(["pack", "packs", "presets", "requestedPacks", "requestedScopes", "sas"]);
  const reappeared = [];
  for (const [keyPath, text] of vocabulary(catalog)) {
    if (removedKeys.has(text)) reappeared.push(`${keyPath}（键名 ${text}）`);
    else if (/^(pack|preset)\./.test(text)) reappeared.push(`${keyPath} = ${text}`);
  }
  if (reappeared.length > 0) {
    errors.push(`commands.json: 设备授权词汇（pack/preset/sas）与设备方向请求字段必须已删除，实际仍有 ${reappeared.join("、")}`);
  }

  const nodeLinkTransport = (catalog.commands ?? []).filter((c) => (c.transport ?? []).includes("node_link"));
  if (nodeLinkTransport.length === 0) errors.push("commands.json: no node_link transport commands");
  const syncTransport = (catalog.commands ?? []).filter((c) => (c.transport ?? []).includes("sync"));
  if (syncTransport.length === 0) errors.push("commands.json: no sync transport commands");

  const syncSchema = readJson(syncSchemaPath);
  if (syncSchema) {
    // Each protocol schema enumerates only the commands that protocol accepts; the union of
    // both transports, not the Sync schema alone, is the full catalog.
    compareSets(
      "schemas/sync/v1/command.schema.json",
      collectCommandNames(syncSchema),
      syncTransport.map((command) => command.name),
    );
  }

  const nodeLinkSchema = readJson(nodeLinkSchemaPath);
  if (nodeLinkSchema) {
    compareSets(
      "schemas/node-link/v1/command.schema.json",
      collectCommandNames(nodeLinkSchema),
      nodeLinkTransport.map((command) => command.name),
    );
  }

  const syncDoc = readText(syncDocPath);
  if (syncDoc) {
    compareSets("docs/SYNC_PROTOCOL.md §11.5", tableCommands(syncDoc, "### 11.5 v1 Command Schema"), names);
  }

  const securityDoc = readText(securityDocPath);
  if (securityDoc) {
    // 该节标题由 `SECURITY_DESIGN.md` 自持：本变更删掉 pack/preset 后标题的措辞必然改写，因此这里
    // **只按编号前缀定位**，判据仍是「节内表格的命令名与登记表相等」。改标题不会误伤这条判据，
    // 删掉整节或改编号仍会失败。
    compareSets("docs/SECURITY_DESIGN.md §10.2", tableCommands(securityDoc, "### 10.2 "), names);
  }

  const localAdminDoc = readText(localAdminDocPath);
  if (localAdminDoc) checkLocalAdmin(localAdminDoc, catalog);
}

if (!existsSync(syncSchemaPath)) errors.push("missing schemas/sync/v1/command.schema.json");
if (!existsSync(nodeLinkSchemaPath)) errors.push("missing schemas/node-link/v1/command.schema.json");

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(`command catalog OK: ${commandTotal} commands`);
}
