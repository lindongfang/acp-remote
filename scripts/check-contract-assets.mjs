import { createHash, createHmac, webcrypto } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Everything a JSON Schema validator cannot decide: digests and lengths that must be
// recomputed from the bytes themselves (ACP rawJson, transcript vectors, HMAC, P-256
// signatures), plus a static $ref net. Structural validation against schemas lives in
// scripts/check-schema-fixtures.mjs (ajv, Draft 2020-12).

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const assetRoots = [
  { name: "sync", schemaRoot: join(root, "schemas", "sync", "v1"), fixtureRoot: join(root, "fixtures", "sync", "v1") },
  { name: "node-link", schemaRoot: join(root, "schemas", "node-link", "v1"), fixtureRoot: join(root, "fixtures", "node-link", "v1") },
];

const parsed = new Map();
const errors = [];
let schemaCount = 0;
let fixtureCount = 0;

function walkJson(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return walkJson(path);
    return entry.isFile() && entry.name.endsWith(".json") ? [path] : [];
  });
}

function readJson(path) {
  if (parsed.has(path)) return parsed.get(path);
  try {
    const value = JSON.parse(readFileSync(path, "utf8"));
    parsed.set(path, value);
    return value;
  } catch (error) {
    errors.push(`${relative(root, path)}: invalid JSON: ${error.message}`);
    return undefined;
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

// ajv resolves every $ref it compiles, but compiles $defs lazily: a broken $ref that
// nothing references would stay invisible. This static pass closes that gap.
function inspectRefs(value, sourcePath) {
  if (!value || typeof value !== "object") return;
  if (typeof value.$ref === "string") {
    const [filePart, fragment = ""] = value.$ref.split("#", 2);
    const targetPath = filePart ? resolve(dirname(sourcePath), filePart) : sourcePath;
    if (!existsSync(targetPath)) {
      errors.push(`${relative(root, sourcePath)}: missing $ref target ${value.$ref}`);
    } else {
      const target = readJson(targetPath);
      const pointer = fragment ? `#${fragment}` : "#";
      if (target && resolvePointer(target, pointer) === undefined) {
        errors.push(`${relative(root, sourcePath)}: missing JSON pointer ${value.$ref}`);
      }
    }
  }
  for (const child of Object.values(value)) inspectRefs(child, sourcePath);
}
function checkRawAcp(value, fixturePath) {
  if (!value || typeof value !== "object") return;
  if (typeof value.rawJson === "string") {
    const bytes = Buffer.from(value.rawJson, "utf8");
    const digest = createHash("sha256").update(bytes).digest("base64url");
    if (String(bytes.length) !== value.byteLength) {
      errors.push(`${relative(root, fixturePath)}: rawJson byteLength mismatch`);
    }
    if (digest !== value.sha256) {
      errors.push(`${relative(root, fixturePath)}: rawJson sha256 mismatch`);
    }
  }
  for (const child of Object.values(value)) checkRawAcp(child, fixturePath);
}

async function checkTranscriptVector(vectorPath) {
  const label = relative(root, vectorPath);
  const vector = readJson(vectorPath);
  if (!vector) return;
  const expected = vector.expected;
  if (!expected || typeof expected.transcriptBase64url !== "string") {
    errors.push(`${label}: expected.transcriptBase64url missing`);
    return;
  }
  const transcript = Buffer.from(expected.transcriptBase64url, "base64url");
  const digest = createHash("sha256").update(transcript).digest("hex");
  if (digest !== expected.transcriptSha256Hex) {
    errors.push(`${label}: transcript SHA-256 mismatch`);
  }
  if (typeof expected.hmacSha256 === "string") {
    if (typeof expected.hmacKeyBase64url !== "string") {
      errors.push(`${label}: hmacSha256 present without hmacKeyBase64url`);
    } else {
      const key = Buffer.from(expected.hmacKeyBase64url, "base64url");
      const mac = createHmac("sha256", key).update(transcript).digest("base64url");
      if (mac !== expected.hmacSha256) {
        errors.push(`${label}: HMAC-SHA256 mismatch`);
      }
    }
  }
  if (typeof expected.p1363Signature === "string") {
    if (typeof expected.publicKey !== "string") {
      errors.push(`${label}: p1363Signature present without publicKey`);
      return;
    }
    const publicKey = await webcrypto.subtle.importKey(
      "raw",
      Buffer.from(expected.publicKey, "base64url"),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    const verified = await webcrypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      publicKey,
      Buffer.from(expected.p1363Signature, "base64url"),
      transcript,
    );
    if (!verified) errors.push(`${label}: P1363 signature verification failed`);
  }
}

await checkRoots();

async function checkRoots() {
  for (const assetRoot of assetRoots) {
    const schemaPaths = walkJson(assetRoot.schemaRoot);
    const fixturePaths = walkJson(assetRoot.fixtureRoot);
    schemaCount += schemaPaths.length;
    fixtureCount += fixturePaths.length;

    if (schemaPaths.length === 0) errors.push(`${assetRoot.name}: no schema files under ${relative(root, assetRoot.schemaRoot)}`);
    if (fixturePaths.length === 0) errors.push(`${assetRoot.name}: no fixture files under ${relative(root, assetRoot.fixtureRoot)}`);

    for (const path of schemaPaths) {
      const schema = readJson(path);
      if (!schema) continue;
      inspectRefs(schema, path);
    }

    for (const path of fixturePaths) {
      const fixture = readJson(path);
      if (fixture) checkRawAcp(fixture, path);
    }

    const manifestPath = join(assetRoot.fixtureRoot, "manifest.json");
    const manifest = readJson(manifestPath);
    if (!manifest) {
      errors.push(`${assetRoot.name}: missing manifest at ${relative(root, manifestPath)}`);
      continue;
    }
    for (const vector of manifest.transcriptVectors ?? []) {
      const vectorPath = resolve(assetRoot.fixtureRoot, vector);
      if (!existsSync(vectorPath)) {
        errors.push(`manifest: missing transcript vector ${vector}`);
      } else {
        await checkTranscriptVector(vectorPath);
      }
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(`contract assets OK: ${schemaCount} schemas, ${fixtureCount} fixture files`);
}
