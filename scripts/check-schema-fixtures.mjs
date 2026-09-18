import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

// Standard Draft 2020-12 validation of every manifest case in every asset tree.
// The manifest drives a real validator: `valid` fixtures must pass, `invalid`
// fixtures must fail with the keyword they declare in `expectedKeyword`, and
// event fixtures bound through `viewSchema`/`viewDef` must satisfy their view.

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const assetRoots = [
  { name: "sync", schemaRoot: join(root, "schemas", "sync", "v1"), fixtureRoot: join(root, "fixtures", "sync", "v1") },
  { name: "node-link", schemaRoot: join(root, "schemas", "node-link", "v1"), fixtureRoot: join(root, "fixtures", "node-link", "v1") },
  {
    name: "acp",
    schemaRoot: join(root, "schemas", "acp", "v1", "upstream"),
    fixtureRoot: join(root, "fixtures", "acp", "v1"),
  },
];

const errors = [];
let validChecked = 0;
let invalidChecked = 0;
let viewChecked = 0;

const ajv = new Ajv2020({ allErrors: true, strict: false, unevaluated: true, validateFormats: true });
addFormats(ajv);

// The pinned ACP snapshot marks integer widths with `format: uint16|uint32|uint64|int64`.
// Registering them keeps the check silent and, more importantly, enforces the width and
// sign instead of silently ignoring the constraint as an unknown format.
const integerFormats = {
  uint16: (value) => Number.isInteger(value) && value >= 0 && value <= 65535,
  uint32: (value) => Number.isInteger(value) && value >= 0 && value <= 4294967295,
  uint64: (value) => Number.isInteger(value) && value >= 0,
  int64: (value) => Number.isInteger(value),
};
for (const [name, validate] of Object.entries(integerFormats)) {
  ajv.addFormat(name, { type: "number", validate });
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    errors.push(`${relative(root, path)}: invalid JSON: ${error.message}`);
    return undefined;
  }
}

function listJson(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return listJson(path);
    return entry.isFile() && entry.name.endsWith(".json") ? [path] : [];
  });
}

function summarize(validateErrors) {
  const list = [...(validateErrors ?? [])];
  if (list.length === 0) return "";
  const depth = (error) => (error.instancePath || "").split("/").length;
  const deepest = Math.max(...list.map(depth));
  return list
    .filter((error) => depth(error) === deepest)
    .slice(0, 3)
    .map((error) => `${error.instancePath || "/"} ${error.keyword}${error.message ? ` ${error.message}` : ""} ${JSON.stringify(error.params ?? {})}`.trim())
    .join(" | ");
}

// Pass 1: register every schema so cross-file $ref targets exist before compiling.
const schemaEntries = new Map();
const entriesByKey = new Map();
for (const assetRoot of assetRoots) {
  for (const schemaPath of listJson(assetRoot.schemaRoot)) {
    const schema = readJson(schemaPath);
    if (!schema) continue;
    const key = schema.$id ?? pathToFileURL(schemaPath).href;
    try {
      ajv.addSchema(schema, key);
    } catch (error) {
      errors.push(`${relative(root, schemaPath)}: cannot register schema: ${error.message}`);
      continue;
    }
    const entry = { path: schemaPath, doc: schema, key };
    schemaEntries.set(schemaPath, entry);
    entriesByKey.set(key, entry);
  }
}

function resolvePointer(document, pointer) {
  if (!pointer || pointer === "#") return document;
  if (!pointer.startsWith("#/")) return undefined;
  return pointer
    .slice(2)
    .split("/")
    .map((part) => part.replaceAll("~1", "/").replaceAll("~0", "~"))
    .reduce((value, part) => value?.[part], document);
}

function resolveRef(ref, fromEntry) {
  const [filePart, fragment = ""] = ref.split("#", 2);
  let entry;
  if (!filePart) entry = fromEntry;
  else if (entriesByKey.has(filePart)) entry = entriesByKey.get(filePart);
  else entry = schemaEntries.get(resolve(dirname(fromEntry.path), filePart));
  if (!entry) return undefined;
  const node = resolvePointer(entry.doc, fragment ? `#${fragment}` : "#");
  return node ? { entry, node } : undefined;
}

// A union branch may be nothing but a $ref to another file whose own union declares
// the message types; collect every `type` const reachable from a branch.
function declaredTypes(schema, entry, out = new Set(), depth = 0) {
  if (!schema || typeof schema !== "object" || depth > 12) return out;
  if (typeof schema.$ref === "string") {
    const target = resolveRef(schema.$ref, entry);
    if (target) declaredTypes(target.node, target.entry, out, depth + 1);
  }
  const constType = schema.properties?.type?.const;
  if (typeof constType === "string") out.add(constType);
  for (const keyword of ["allOf", "oneOf", "anyOf"]) {
    for (const branch of schema[keyword] ?? []) declaredTypes(branch, entry, out, depth + 1);
  }
  return out;
}

// Pass 2: build one validator per union branch, tagged with the `type` const it declares.
const validators = new Map();

function runValidator(entry, value) {
  const ok = entry.validate(value);
  return { ok, errors: ok ? [] : (entry.validate.errors ?? []) };
}

function checkValue(schemaPath, value) {
  const entries = validators.get(schemaPath);
  if (!entries) return { ok: false, errors: [] };
  const results = entries.map((entry) => ({ entry, ...runValidator(entry, value) }));
  if (results.some((result) => result.ok)) return { ok: true, errors: [] };
  // A union schema reports every branch's errors; prefer the branch that declares
  // the value's `type`, so the reported failure names the real constraint.
  const intended = results.find((result) => result.entry.declaredTypes.has(value?.type));
  const chosen = intended ?? results.reduce((best, result) => (result.errors.length < best.errors.length ? result : best));
  return { ok: false, errors: chosen.errors };
}

for (const [schemaPath, entry] of schemaEntries) {
  if (Array.isArray(entry.doc.oneOf)) {
    try {
      validators.set(
        schemaPath,
        entry.doc.oneOf.map((branch, index) => {
          const ref = `${entry.key}#/oneOf/${index}`;
          return {
            declaredTypes: declaredTypes({ $ref: ref }, entry),
            validate: ajv.compile({ $ref: ref }),
          };
        }),
      );
    } catch (error) {
      errors.push(`${relative(root, schemaPath)}: cannot compile union branch: ${error.message}`);
    }
  } else {
    try {
      const validate = ajv.getSchema(entry.key);
      if (!validate) {
        errors.push(`${relative(root, schemaPath)}: schema not registered with ajv`);
        continue;
      }
      validators.set(schemaPath, [{ declaredTypes: new Set(), validate }]);
    } catch (error) {
      errors.push(`${relative(root, schemaPath)}: cannot compile schema: ${error.message}`);
    }
  }
}

const viewCache = new Map();

// A case may target a subschema through `schemaPointer` instead of the document root.
// The pinned ACP snapshot is a loose union across message kinds, so a negative case that
// must isolate one constraint (for example a uint32 field) has to name the `$defs` entry.
const pointerCache = new Map();

function checkValueAtPointer(schemaPath, pointer, value) {
  const entry = schemaEntries.get(schemaPath);
  if (!entry) return { ok: false, errors: [] };
  const key = `${entry.key}${pointer}`;
  if (!pointerCache.has(key)) {
    try {
      pointerCache.set(key, ajv.compile({ $ref: key }));
    } catch (error) {
      errors.push(`cannot compile ${relative(root, schemaPath)}${pointer}: ${error.message}`);
      return { ok: false, errors: [] };
    }
  }
  const validate = pointerCache.get(key);
  const ok = validate(value);
  return { ok, errors: ok ? [] : (validate.errors ?? []) };
}

function checkView(schemaPath, viewDef, value, label, fixturePath) {
  const entry = schemaEntries.get(schemaPath);
  if (!entry) {
    errors.push(`manifest: view schema not registered: ${relative(root, schemaPath)}`);
    return;
  }
  const cacheKey = `${entry.key}${viewDef}`;
  if (!viewCache.has(cacheKey)) {
    try {
      viewCache.set(cacheKey, ajv.compile({ $ref: cacheKey }));
    } catch (error) {
      errors.push(`${label}: cannot compile view ${viewDef}: ${error.message}`);
      return;
    }
  }
  const validateView = viewCache.get(cacheKey);
  if (!validateView(value)) {
    errors.push(`fixture view rejected: ${relative(root, fixturePath)}: ${summarize(validateView.errors)}`);
  }
}

// Event view coverage is a property of the view schema, not of one asset root: both protocols
// share schemas/sync/v1/event-views.schema.json, and each `$defs` entry needs one real fixture.
const viewBindings = new Map();

for (const assetRoot of assetRoots) {
  const manifestPath = join(assetRoot.fixtureRoot, "manifest.json");
  const manifest = readJson(manifestPath);
  if (!manifest) continue;

  const listedFixtures = new Set();

  for (const testCase of manifest.cases ?? []) {
    const fixturePath = resolve(assetRoot.fixtureRoot, testCase.fixture);
    const schemaPath = resolve(assetRoot.fixtureRoot, testCase.schema);
    listedFixtures.add(fixturePath);

    if (!existsSync(fixturePath)) {
      errors.push(`manifest: missing fixture ${testCase.fixture}`);
      continue;
    }
    if (!existsSync(schemaPath)) {
      errors.push(`manifest: missing schema ${testCase.schema}`);
      continue;
    }

    const fixture = readJson(fixturePath);
    if (!fixture) continue;
    const label = relative(root, fixturePath);
    const { ok, errors: caseErrors } =
      typeof testCase.schemaPointer === "string"
        ? checkValueAtPointer(schemaPath, testCase.schemaPointer, fixture)
        : checkValue(schemaPath, fixture);

    if (testCase.valid === true) {
      validChecked += 1;
      if (!ok) errors.push(`valid fixture rejected: ${label}: ${summarize(caseErrors)}`);
    } else {
      invalidChecked += 1;
      if (ok) {
        errors.push(`invalid fixture accepted: ${label}`);
      } else if (typeof testCase.expectedKeyword === "string") {
        const keywords = new Set(caseErrors.map((error) => error.keyword));
        if (!keywords.has(testCase.expectedKeyword)) {
          errors.push(
            `invalid fixture failed with unexpected keyword: ${label}: expected ${testCase.expectedKeyword}, got ${[...keywords].join("/")} (${summarize(caseErrors)})`,
          );
        }
      }
    }

    if (typeof testCase.viewSchema === "string" && typeof testCase.viewDef === "string") {
      viewChecked += 1;
      checkView(resolve(assetRoot.fixtureRoot, testCase.viewSchema), testCase.viewDef, fixture?.body?.payload?.view, label, fixturePath);
    }
  }

  // Coverage gate: a declared message type or event view with no fixture is an unverified
  // contract surface. The document promises this coverage; without the assertion a new type
  // could ship silently unchecked.
  const unionPath = join(assetRoot.schemaRoot, "message.schema.json");
  const unionEntry = schemaEntries.get(unionPath);
  if (unionEntry) {
    const declared = declaredTypes({ $ref: unionEntry.key }, unionEntry);
    const covered = new Set();
    for (const testCase of manifest.cases ?? []) {
      const fixturePath = resolve(assetRoot.fixtureRoot, testCase.fixture);
      if (!existsSync(fixturePath)) continue;
      const fixture = readJson(fixturePath);
      if (typeof fixture?.type === "string") covered.add(fixture.type);
    }
    for (const type of declared) {
      if (!covered.has(type)) errors.push(`${assetRoot.name}: message type has no fixture: ${type}`);
    }
    for (const type of covered) {
      if (!declared.has(type)) errors.push(`${assetRoot.name}: fixture uses unregistered type: ${type}`);
    }
  }

  for (const testCase of manifest.cases ?? []) {
    if (typeof testCase.viewSchema !== "string" || typeof testCase.viewDef !== "string") continue;
    const viewSchemaPath = resolve(assetRoot.fixtureRoot, testCase.viewSchema);
    if (!viewBindings.has(viewSchemaPath)) viewBindings.set(viewSchemaPath, new Set());
    viewBindings.get(viewSchemaPath).add(testCase.viewDef);
  }

  for (const vector of manifest.transcriptVectors ?? []) {
    listedFixtures.add(resolve(assetRoot.fixtureRoot, vector));
  }
  for (const fixturePath of listJson(assetRoot.fixtureRoot)) {
    if (fixturePath === manifestPath) continue;
    if (!listedFixtures.has(fixturePath)) {
      errors.push(`${assetRoot.name}: fixture not listed in manifest: ${relative(root, fixturePath)}`);
    }
  }
}

for (const [viewSchemaPath, bound] of viewBindings) {
  const viewEntry = schemaEntries.get(viewSchemaPath);
  if (!viewEntry) continue;
  for (const def of Object.keys(viewEntry.doc.$defs ?? {})) {
    if (!bound.has(`#/$defs/${def}`)) {
      errors.push(`event view has no fixture: ${def} (${relative(root, viewSchemaPath)})`);
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `schema fixtures OK: ${validChecked} valid, ${invalidChecked} invalid (ajv Draft 2020-12), ${viewChecked} event views bound`,
  );
}
