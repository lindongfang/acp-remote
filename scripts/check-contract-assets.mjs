import { createHash, webcrypto } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schemaRoot = join(root, "schemas", "sync", "v1");
const fixtureRoot = join(root, "fixtures", "sync", "v1");
const parsed = new Map();
const errors = [];

function walkJson(directory) {
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

for (const path of [...walkJson(schemaRoot), ...walkJson(fixtureRoot)]) readJson(path);

const ids = new Map();
for (const path of walkJson(schemaRoot)) {
  const schema = readJson(path);
  if (!schema) continue;
  inspectRefs(schema, path);
  if (schema.$id) {
    const previous = ids.get(schema.$id);
    if (previous) errors.push(`duplicate $id ${schema.$id}: ${previous} and ${path}`);
    ids.set(schema.$id, relative(root, path));
  }
}

const manifestPath = join(fixtureRoot, "manifest.json");
const manifest = readJson(manifestPath);
if (manifest) {
  for (const testCase of manifest.cases ?? []) {
    const fixturePath = resolve(fixtureRoot, testCase.fixture);
    const schemaPath = resolve(fixtureRoot, testCase.schema);
    if (!existsSync(fixturePath)) errors.push(`manifest: missing fixture ${testCase.fixture}`);
    if (!existsSync(schemaPath)) errors.push(`manifest: missing schema ${testCase.schema}`);
    const fixture = existsSync(fixturePath) ? readJson(fixturePath) : undefined;
    if (fixture) checkRawAcp(fixture, fixturePath);
  }
  for (const vector of manifest.transcriptVectors ?? []) {
    const vectorPath = resolve(fixtureRoot, vector);
    if (!existsSync(vectorPath)) errors.push(`manifest: missing transcript vector ${vector}`);
  }
}

const hostVectorPath = join(fixtureRoot, "transcripts", "host-challenge.json");
const hostVector = readJson(hostVectorPath);
if (hostVector) {
  const transcript = Buffer.from(hostVector.expected.transcriptBase64url, "base64url");
  const digest = createHash("sha256").update(transcript).digest("hex");
  if (digest !== hostVector.expected.transcriptSha256Hex) {
    errors.push("host-challenge vector: transcript SHA-256 mismatch");
  }
  const publicKey = await webcrypto.subtle.importKey(
    "raw",
    Buffer.from(hostVector.expected.publicKey, "base64url"),
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["verify"],
  );
  const verified = await webcrypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    publicKey,
    Buffer.from(hostVector.expected.p1363Signature, "base64url"),
    transcript,
  );
  if (!verified) errors.push("host-challenge vector: P1363 signature verification failed");
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `contract assets OK: ${walkJson(schemaRoot).length} schemas, ${walkJson(fixtureRoot).length} fixture files`,
  );
}
