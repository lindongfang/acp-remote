import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

// The feature-ID registry (compatibility/features/v1/features.json) is the single machine
// definition of the closed feature vocabulary of both protocols. Feature IDs also appear in
// the protocol documents and inside negotiation fixtures, so this check keeps the registry,
// the two document tables, and every fixture occurrence equal — a feature ID can no longer be
// invented in a document, a schema or a fixture without failing `npm run check`.

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const registryPath = join(root, "compatibility", "features", "v1", "features.json");
const registrySchemaPath = join(root, "schemas", "features", "features.schema.json");

const sources = [
  {
    key: "sync",
    doc: join(root, "docs", "SYNC_PROTOCOL.md"),
    heading: "### 5.2 Feature ID",
    fixtureRoot: join(root, "fixtures", "sync", "v1"),
  },
  {
    key: "node_link",
    doc: join(root, "docs", "NODE_LINK_PROTOCOL.md"),
    heading: "### 11.3 Feature ID",
    fixtureRoot: join(root, "fixtures", "node-link", "v1"),
  },
];

// The wire field names that carry feature lists (auth handshake, negotiation transcripts).
const featureArrayKeys = [
  "supportedFeatures",
  "requiredFeatures",
  "selectedFeatures",
  "negotiatedFeatures",
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
    errors.push(`${relative(root, path)}: invalid JSON: ${error.message}`);
    return null;
  }
}

function listJson(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return listJson(path);
    return entry.isFile() && entry.name.endsWith(".json") ? [path] : [];
  });
}

const registrySchema = readJson(registrySchemaPath);
const registry = readJson(registryPath);

if (registrySchema && registry) {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const validate = ajv.compile(registrySchema);
  if (!validate(registry)) {
    errors.push(`${relative(root, registryPath)}: does not match features.schema.json: ${ajv.errorsText(validate.errors)}`);
  }
}

const registered = new Map();
for (const source of sources) {
  const features = registry?.protocols?.[source.key]?.features ?? [];
  const seen = new Set();
  for (const feature of features) {
    if (seen.has(feature?.id)) {
      errors.push(`${relative(root, registryPath)}: ${source.key} 的 feature ID 重复：${feature?.id}`);
    }
    seen.add(feature?.id);
  }
  registered.set(source.key, features);
}

/** 返回 `heading` 之后第一张 markdown 表的行（含表头），单元格已 trim。 */
function tableRowsAfterHeading(text, heading) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === heading);
  if (start < 0) return undefined;
  const rows = [];
  let inTable = false;
  for (let index = start + 1; index < lines.length; index += 1) {
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
  return rows.length > 0 ? rows : undefined;
}

function parseDocumentTable(source) {
  const text = readText(source.doc);
  if (text === null) return undefined;
  const rows = tableRowsAfterHeading(text, source.heading);
  if (!rows) {
    errors.push(`${relative(root, source.doc)}: 找不到 ${source.heading} 之后的 feature 表`);
    return undefined;
  }
  const [header, ...body] = rows;
  const expectedHeader = ["feature", "说明", "delivery", "必需"];
  if (header.join("|") !== expectedHeader.join("|")) {
    errors.push(
      `${relative(root, source.doc)}: ${source.heading} 表头应为 ${expectedHeader.join(" | ")}，实际 ${header.join(" | ")}`,
    );
  }
  return body.map((cells) => ({
    id: (cells[0] ?? "").replace(/`/g, ""),
    delivery: cells[2] ?? "",
    required: (cells[3] ?? "").trim() === "是",
  }));
}

for (const source of sources) {
  const declared = parseDocumentTable(source);
  if (!declared) continue;
  const expected = (registered.get(source.key) ?? []).map((feature) => ({
    id: feature.id,
    delivery: feature.delivery,
    required: feature.required,
  }));

  if (declared.length !== expected.length) {
    errors.push(
      `${relative(root, source.doc)}: ${source.heading} 有 ${declared.length} 行，registry 有 ${expected.length} 条`,
    );
  }
  const length = Math.max(declared.length, expected.length);
  for (let index = 0; index < length; index += 1) {
    const left = declared[index];
    const right = expected[index];
    if (!left || !right) continue;
    if (left.id !== right.id || left.delivery !== right.delivery || left.required !== right.required) {
      errors.push(
        `${relative(root, source.doc)}: 第 ${index + 1} 行与 registry 不一致：` +
          `文档 ${JSON.stringify(left)} vs registry ${JSON.stringify(right)}`,
      );
    }
  }
}

/** 收集 fixture 中出现过的 feature ID。 */
function collectFeatureIds(node, found) {
  if (Array.isArray(node)) {
    for (const item of node) collectFeatureIds(item, found);
    return;
  }
  if (!node || typeof node !== "object") return;
  for (const [key, value] of Object.entries(node)) {
    if (featureArrayKeys.includes(key) && Array.isArray(value)) {
      for (const item of value) {
        if (typeof item === "string") found.add(item);
      }
    }
    collectFeatureIds(value, found);
  }
}

for (const source of sources) {
  const known = new Set((registered.get(source.key) ?? []).map((feature) => feature.id));
  for (const path of listJson(source.fixtureRoot)) {
    const document = readJson(path);
    if (!document) continue;
    const found = new Set();
    collectFeatureIds(document, found);
    for (const id of found) {
      if (!known.has(id)) {
        errors.push(`${relative(root, path)}: feature ${id} 未在 registry 的 ${source.key} 列表中登记`);
      }
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(`error: ${error}`);
  console.error(`feature registry check failed: ${errors.length} problem(s)`);
  process.exit(1);
}

const total = [...registered.values()].reduce((sum, features) => sum + features.length, 0);
console.log(`feature registry OK: ${total} feature ids across ${sources.length} protocols`);
