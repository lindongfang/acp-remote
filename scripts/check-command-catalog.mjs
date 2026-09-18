import { existsSync, readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalogPath = resolve(root, "compatibility", "commands", "v1", "commands.json");
const syncSchemaPath = resolve(root, "schemas", "sync", "v1", "command.schema.json");
const nodeLinkSchemaPath = resolve(root, "schemas", "node-link", "v1", "command.schema.json");
const syncDocPath = resolve(root, "docs", "SYNC_PROTOCOL.md");
const securityDocPath = resolve(root, "docs", "SECURITY_DESIGN.md");
const coreBrokerPath = resolve(root, "crates", "core", "src", "broker.rs");

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

// `crates/core/src/broker.rs::required_grant` 是 commands.json 的**手工镜像**（core 无 serde，读不了
// JSON），而 Node actor 的 Export/Import 交集判定依赖它。镜像漂移等于授权漂移，因此这里按源码解析
// match 臂逐条比对：`"a" | "b" => "grant.x"`。解析失败即报错——形状变了必须同步改本门禁。
function parseRequiredGrant(text) {
  const start = text.indexOf("pub fn required_grant(");
  if (start < 0) {
    errors.push("crates/core/src/broker.rs: missing required_grant");
    return undefined;
  }
  const tail = text.slice(start);
  const stop = tail.indexOf("_ => return None,");
  if (stop < 0) {
    errors.push("crates/core/src/broker.rs: required_grant has no `_ => return None` arm (gate parser out of date)");
    return undefined;
  }
  const arms = tail.slice(tail.indexOf("match command {"), stop);
  const mirrored = new Map();
  const armPattern = /((?:"[0-9a-z_.]+"\s*\|?\s*)+)=>\s*"([a-z0-9_.-]+)"/g;
  for (const match of arms.matchAll(armPattern)) {
    const names = [...match[1].matchAll(/"([0-9a-z_.]+)"/g)].map((name) => name[1]);
    for (const name of names) {
      if (mirrored.has(name)) errors.push(`crates/core/src/broker.rs: ${name} appears in two grant arms`);
      mirrored.set(name, match[2]);
    }
  }
  if (mirrored.size === 0) {
    errors.push("crates/core/src/broker.rs: required_grant parsed to zero arms (gate parser out of date)");
    return undefined;
  }
  return mirrored;
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
    if (!command.pack && command.name !== "session.create") {
      errors.push(`commands.json: ${command.name} has no pack`);
    }
  }
  commandTotal = names.size;

  const referenced = new Set([
    ...Object.values(catalog.packs ?? {}).flat(),
    ...Object.values(catalog.grants ?? {}).flat(),
  ]);
  compareSets("commands.json packs/grants", referenced, names);

  for (const [pack, members] of Object.entries(catalog.packs ?? {})) {
    for (const member of members) {
      const command = (catalog.commands ?? []).find((c) => c.name === member);
      if (command && command.pack !== pack) {
        errors.push(`commands.json: ${member} listed in ${pack} but declares pack ${command.pack}`);
      }
    }
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
    compareSets("docs/SECURITY_DESIGN.md §10.2", tableCommands(securityDoc, "### 10.2 命令、scope、pack 与 grant"), names);
  }

  const broker = readText(coreBrokerPath);
  if (broker) {
    const mirrored = parseRequiredGrant(broker);
    if (mirrored) {
      compareSets("crates/core/src/broker.rs required_grant", new Set(mirrored.keys()), names);
      for (const command of catalog.commands ?? []) {
        const grant = mirrored.get(command.name);
        if (grant !== undefined && command.grant !== undefined && grant !== command.grant) {
          errors.push(
            `crates/core/src/broker.rs: ${command.name} maps to ${grant} but commands.json declares ${command.grant}`,
          );
        }
        if (command.grant !== undefined && !(command.grant in (catalog.grants ?? {}))) {
          errors.push(`commands.json: ${command.name} declares unregistered grant ${command.grant}`);
        }
      }
    }
  }
}

if (!existsSync(syncSchemaPath)) errors.push("missing schemas/sync/v1/command.schema.json");
if (!existsSync(nodeLinkSchemaPath)) errors.push("missing schemas/node-link/v1/command.schema.json");

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(`command catalog OK: ${commandTotal} commands`);
}