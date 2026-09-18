import { readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

// The error code registry is the machine source of truth for the error-code list of every
// protocol. JSON Schema cannot import JSON, so each protocol schema carries its own enum;
// this check keeps the registry, the schema enum, and the protocol document listing equal,
// and refuses any second inline definition of a public error `code` property.

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const registryPath = join(root, "compatibility", "errors", "v1", "errors.json");
const registrySchemaPath = join(root, "schemas", "errors", "errors.schema.json");

const protocolSources = [
  {
    key: "sync",
    doc: join(root, "docs", "SYNC_PROTOCOL.md"),
    schemaFiles: [
      join(root, "schemas", "sync", "v1", "common.schema.json"),
      join(root, "schemas", "sync", "v1", "error.schema.json"),
      join(root, "schemas", "sync", "v1", "pairing.schema.json"),
      join(root, "schemas", "sync", "v1", "command.schema.json"),
      join(root, "schemas", "sync", "v1", "sync.schema.json"),
      join(root, "schemas", "sync", "v1", "event.schema.json"),
      join(root, "schemas", "sync", "v1", "event-views.schema.json"),
    ],
    errorCodeDef: join(root, "schemas", "sync", "v1", "common.schema.json"),
  },
  {
    key: "node_link",
    doc: join(root, "docs", "NODE_LINK_PROTOCOL.md"),
    schemaFiles: [
      join(root, "schemas", "node-link", "v1", "common.schema.json"),
      join(root, "schemas", "node-link", "v1", "error.schema.json"),
      join(root, "schemas", "node-link", "v1", "pairing.schema.json"),
      join(root, "schemas", "node-link", "v1", "command.schema.json"),
      join(root, "schemas", "node-link", "v1", "resource.schema.json"),
      join(root, "schemas", "node-link", "v1", "catalog.schema.json"),
      join(root, "schemas", "node-link", "v1", "handshake.schema.json"),
      join(root, "schemas", "node-link", "v1", "message.schema.json"),
    ],
    errorCodeDef: join(root, "schemas", "node-link", "v1", "common.schema.json"),
  },
];

const errors = [];

function readText(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    errors.push(`cannot read ${relative(root, path)}: ${error.message}`);
    return null;
  }
}

function readJson(path) {
  const text = readText(path);
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch (error) {
    errors.push(`${relative(root, path)} is not valid JSON: ${error.message}`);
    return null;
  }
}

// The document listing is the fenced ```text block that follows the "错误码族" heading.
function errorCodeBlock(text, label) {
  const lines = text.split(/\r?\n/);
  const heading = lines.findIndex((line) => line.includes("错误码族"));
  if (heading < 0) {
    errors.push(`${label}: no 错误码族 heading found`);
    return null;
  }
  let start = -1;
  for (let i = heading + 1; i < lines.length; i += 1) {
    if (/^#{1,3} /.test(lines[i])) break;
    if (/^```text\s*$/.test(lines[i])) {
      start = i + 1;
      break;
    }
  }
  if (start < 0) {
    errors.push(`${label}: no \`\`\`text block after the 错误码族 heading`);
    return null;
  }
  const codes = [];
  for (let i = start; i < lines.length; i += 1) {
    if (/^```/.test(lines[i])) break;
    const value = lines[i].trim();
    if (/^[a-z0-9_.-]+$/.test(value)) codes.push(value);
  }
  return codes;
}

// Any object property literally named `code` that is itself a string schema is a second,
// unregistered definition of the public error code space.
function inlineCodeDefinitions(node, path, hits) {
  if (Array.isArray(node)) {
    node.forEach((value, index) => inlineCodeDefinitions(value, `${path}[${index}]`, hits));
    return;
  }
  if (node === null || typeof node !== "object") return;
  for (const [key, value] of Object.entries(node)) {
    if (key === "code" && value !== null && typeof value === "object" && !Array.isArray(value)) {
      if (value.type === "string" || value.pattern !== undefined || value.enum !== undefined || value.maxLength !== undefined) {
        hits.push(`${path}.code`);
      }
    }
    inlineCodeDefinitions(value, `${path}.${key}`, hits);
  }
}

function compareSequence(label, actual, wanted) {
  if (actual.length !== wanted.length) {
    errors.push(`${label}: ${actual.length} entries, registry has ${wanted.length}`);
    return;
  }
  for (let i = 0; i < wanted.length; i += 1) {
    if (actual[i] !== wanted[i]) {
      errors.push(`${label}: entry ${i} is ${actual[i]}, registry has ${wanted[i]}`);
      return;
    }
  }
}

// `#### details 登记` 之后的第一张表：每行 `| code | 字段、字段 |`，空表体表示该协议没有 code
// 登记 details 字段。表必须与 registry 的 details 片段逐条相等，避免"文档登记了字段、registry
// 没有"或反向的漂移。
function detailsTable(text, label) {
  const lines = text.split(/\r?\n/);
  const heading = lines.findIndex((line) => line.trim() === "#### details 登记");
  if (heading < 0) {
    errors.push(`${label}: no "#### details 登记" section`);
    return null;
  }
  const rows = [];
  let inTable = false;
  for (let index = heading + 1; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (!line.startsWith("|")) {
      if (inTable) break;
      continue;
    }
    inTable = true;
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((cell) => cell.trim());
    if (cells.every((cell) => /^-{2,}$/.test(cell))) continue;
    rows.push(cells);
  }
  if (rows.length === 0) {
    errors.push(`${label}: details 登记表缺失（表头行必须在，无登记字段时留空表体）`);
    return null;
  }
  return rows.slice(1);
}

function detailsFieldNames(fragment) {
  return Object.keys(fragment?.properties ?? {});
}

const registry = readJson(registryPath);
const registrySchema = readJson(registrySchemaPath);

const ajv = new Ajv2020({ allErrors: true, strict: false });

if (registrySchema) {
  const validate = ajv.compile(registrySchema);
  if (registry && !validate(registry)) {
    for (const issue of validate.errors ?? []) {
      errors.push(`compatibility/errors/v1/errors.json${issue.instancePath} ${issue.message}`);
    }
  }
}

let codeTotal = 0;

if (registry && registry.protocols) {
  for (const source of protocolSources) {
    const entry = registry.protocols[source.key];
    const label = relative(root, registryPath).replace(/\\/g, "/");
    if (!entry) {
      errors.push(`${label}: missing protocol "${source.key}"`);
      continue;
    }
    const codes = entry.errors.map((item) => item.code);
    codeTotal += codes.length;

    const duplicates = codes.filter((code, index) => codes.indexOf(code) !== index);
    if (duplicates.length > 0) {
      errors.push(`registry ${source.key}: duplicate codes ${[...new Set(duplicates)].join(", ")}`);
    }

    // The schema enum lives once per protocol, in that protocol's common schema.
    const common = readJson(source.errorCodeDef);
    const enumValues = common?.$defs?.errorCode?.enum;
    if (!Array.isArray(enumValues)) {
      errors.push(`${relative(root, source.errorCodeDef)}: missing $defs.errorCode.enum`);
    } else {
      compareSequence(`${relative(root, source.errorCodeDef)} $defs.errorCode.enum`, enumValues, codes);
    }

    for (const schemaFile of source.schemaFiles) {
      const schema = readJson(schemaFile);
      if (!schema) continue;
      const hits = [];
      inlineCodeDefinitions(schema, "$", hits);
      for (const hit of hits) {
        errors.push(
          `${relative(root, schemaFile)} ${hit}: inline error code definition; reference $defs.errorCode instead`,
        );
      }
    }

    const docText = readText(source.doc);
    if (docText !== null) {
      const documented = errorCodeBlock(docText, relative(root, source.doc));
      if (documented) compareSequence(`${relative(root, source.doc)} 错误码列表`, documented, codes);
    }

    // details 登记：每个 code 要么不登记字段（registry 里为 null，发送方必须送 {}），要么登记一组
    // 字段；登记表与片段必须逐条相等，片段本身必须是合法的开放 object schema。
    const registered = entry.errors.filter((item) => item.details !== null && item.details !== undefined);
    for (const item of registered) {
      const fragment = item.details;
      if (typeof fragment !== "object" || Array.isArray(fragment)) {
        errors.push(`registry ${item.code}: details 必须是 null 或 JSON Schema 对象`);
        continue;
      }
      if (fragment.type !== "object") {
        errors.push(`registry ${item.code}: details 片段必须是 object 类型`);
      }
      const names = detailsFieldNames(fragment);
      const required = Array.isArray(fragment.required) ? fragment.required : [];
      for (const name of required) {
        if (!names.includes(name)) {
          errors.push(`registry ${item.code}: required 里的 ${name} 不在 properties`);
        }
      }
      if (fragment.additionalProperties !== true) {
        errors.push(`registry ${item.code}: details 是开放扩展点，additionalProperties 必须为 true`);
      }
      try {
        ajv.compile(fragment);
      } catch (error) {
        errors.push(`registry ${item.code}: details 片段不是合法 JSON Schema: ${error.message}`);
      }
    }

    if (docText !== null) {
      const rows = detailsTable(docText, relative(root, source.doc));
      if (rows) {
        if (rows.length !== registered.length) {
          errors.push(
            `${relative(root, source.doc)} details 登记表有 ${rows.length} 行，registry 有 ${registered.length} 个登记字段的 code`,
          );
        }
        const length = Math.max(rows.length, registered.length);
        for (let index = 0; index < length; index += 1) {
          const row = rows[index];
          const item = registered[index];
          if (!row || !item) continue;
          const code = (row[0] ?? "").replace(/`/g, "");
          if (code !== item.code) {
            errors.push(
              `${relative(root, source.doc)} details 登记表第 ${index + 1} 行是 ${code}，registry 是 ${item.code}`,
            );
            continue;
          }
          const declared = (row[1] ?? "")
            .replace(/`/g, "")
            .split(/[、,]/)
            .map((part) => part.trim())
            .filter((part) => part.length > 0)
            .map((part) => {
              const match = part.match(/^([A-Za-z0-9_]+)\s*（(必需|可选)）$/);
              return match ? { name: match[1], marker: match[2] } : { name: part, marker: null };
            });
          const registeredNames = detailsFieldNames(item.details);
          const requiredNames = Array.isArray(item.details?.required) ? item.details.required : [];
          if (declared.map((field) => field.name).join(",") !== registeredNames.join(",")) {
            errors.push(
              `${relative(root, source.doc)} ${code} 的 details 字段为 [${declared.map((field) => field.name).join(", ")}]，registry 为 [${registeredNames.join(", ")}]`,
            );
          }
          for (const field of declared) {
            if (field.marker === null) {
              errors.push(`${relative(root, source.doc)} ${code}.${field.name}: 必须标注（必需）或（可选）`);
              continue;
            }
            const isRequired = requiredNames.includes(field.name);
            if (field.marker === "必需" && !isRequired) {
              errors.push(`${relative(root, source.doc)} ${code}.${field.name}: 标为必需，但 registry 的 required 未包含它`);
            }
            if (field.marker === "可选" && isRequired) {
              errors.push(`${relative(root, source.doc)} ${code}.${field.name}: 标为可选，但 registry 的 required 包含它`);
            }
          }
        }
      }
    }

    const subprotocol = entry.subprotocol;
    if (docText !== null && !docText.includes(subprotocol)) {
      errors.push(`${relative(root, source.doc)}: does not mention subprotocol "${subprotocol}"`);
    }
  }

  const syncClose = registry.protocols.sync?.closeCodes?.map((item) => item.code) ?? [];
  const linkClose = registry.protocols.node_link?.closeCodes?.map((item) => item.code) ?? [];
  if (syncClose.join(",") !== linkClose.join(",")) {
    errors.push(
      `close codes differ between protocols: sync [${syncClose.join(",")}] vs node_link [${linkClose.join(",")}] ` +
        "(NODE_LINK_PROTOCOL.md §14.2 reuses the Sync set)",
    );
  }
}

if (errors.length > 0) {
  console.error(`error registry check failed with ${errors.length} problem(s):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(`error registry OK: ${codeTotal} codes across ${protocolSources.length} protocols`);
