import { createHash, createHmac, webcrypto } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

// Everything a JSON Schema validator cannot decide: digests and lengths that must be
// recomputed from the bytes themselves (ACP rawJson, transcript vectors, HMAC, P-256
// signatures), plus a static $ref net. Structural validation of fixtures against schemas
// lives in scripts/check-schema-fixtures.mjs (ajv, Draft 2020-12); ajv is used here only
// for compatibility/transcripts/v1/transcripts.json, which no asset tree walks.

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const assetRoots = [
  { name: "sync", protocol: "sync", schemaRoot: join(root, "schemas", "sync", "v1"), fixtureRoot: join(root, "fixtures", "sync", "v1") },
  { name: "node-link", protocol: "node_link", schemaRoot: join(root, "schemas", "node-link", "v1"), fixtureRoot: join(root, "fixtures", "node-link", "v1") },
];

const transcriptRegistryPath = join(root, "compatibility", "transcripts", "v1", "transcripts.json");
const transcriptSchemaPath = join(root, "schemas", "transcripts", "transcripts.schema.json");

// Byte width of every fixed-size field type; utf8/nul-joined-utf8 are variable.
const fixedWidth = { u16be: 2, u64be: 8, uuid16: 16, "sec1-65": 65, bytes16: 16, bytes32: 32 };

const transcriptErrors = [
  "bad_magic",
  "bad_codec_version",
  "truncated_domain",
  "truncated_field",
  "field_order",
  "duplicate_tag",
  "unknown_tag",
  "length_mismatch",
  "trailing_bytes",
];
const publicKeyErrors = ["bad_point", "bad_length", "bad_base64url", "not_on_curve"];

// P-256 (SEC 2 / FIPS 186-4): y^2 = x^3 + ax + b over GF(p).
const p256 = {
  p: 2n ** 256n - 2n ** 224n + 2n ** 192n + 2n ** 96n - 1n,
  a: -3n,
  b: 0x5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604bn,
};

const parsed = new Map();
const schemaIds = new Map();
const errors = [];
let schemaCount = 0;
let fixtureCount = 0;
let reencodedCount = 0;
let negativeCount = 0;

// Registry lookups: domain specs per protocol, plus the tag -> {name, type} table of each
// protocol (the two tag tables are independent; a tag's meaning never crosses protocols).
const registry = readJson(transcriptRegistryPath);
const transcriptDomains = new Map();
const transcriptTags = new Map();
const coveredDomains = new Set();

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

// 与 check-command-catalog.mjs / check-contract-drift.mjs 同口径：读取失败记一条错误并返回
// `undefined`，由调用方跳过该文档的断言，而不是让整个脚本以 ReferenceError 崩掉。
function readText(path) {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    errors.push(`${relative(root, path)}: unreadable: ${error.message}`);
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
// ACPR-CJ1 (SYNC_PROTOCOL.md §3.3) is the deterministic serialization content digests are
// computed over. Reimplemented here so the fixture set proves the rule instead of trusting it:
// sorted member names, no whitespace, minimal string escaping, integers only.
// SYNC_PROTOCOL.md §3.3 的字符串规则：最小转义（`"` 与 `\\`），且 `< U+0020` 的控制字符写作**小写**
// `\u00xx`。`JSON.stringify` 会为 \b \t \n \f \r 输出短转义，与本条不符——因此这里手写。
function cj1String(text) {
  let out = '"';
  for (const char of text) {
    const code = char.codePointAt(0);
    if (char === '"') out += '\\"';
    else if (char === "\\") out += "\\\\";
    else if (code < 0x20) out += `\\u${code.toString(16).padStart(4, "0")}`;
    else if (code >= 0xd800 && code <= 0xdfff) {
      throw new Error("ACPR-CJ1 拒绝孤立代理项（不是合法 Unicode）");
    } else out += char;
  }
  return out + '"';
}

function acprCj1(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "string") return cj1String(value);
  if (typeof value === "number") {
    if (!Number.isInteger(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER) {
      throw new Error("ACPR-CJ1 只允许 |n| <= 2^53-1 的整数");
    }
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map(acprCj1).join(",")}]`;
  return `{${Object.keys(value)
    .sort()
    .map((key) => `${cj1String(key)}:${acprCj1(value[key])}`)
    .join(",")}}`;
}

function checkPayloadDigest(fixture, fixturePath) {
  const body = fixture?.body;
  if (!body || typeof body.payloadDigest !== "string" || body.payload === undefined) return;
  let canonical;
  try {
    canonical = acprCj1(body.payload);
  } catch (error) {
    errors.push(`${relative(root, fixturePath)}: ${error.message}`);
    return;
  }
  const digest = createHash("sha256").update(Buffer.from(canonical, "utf8")).digest("base64url");
  if (digest !== body.payloadDigest) {
    errors.push(`${relative(root, fixturePath)}: payloadDigest 与 payload 的 ACPR-CJ1 sha256 不一致`);
  }
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

// compatibility/transcripts/v1/transcripts.json is the machine form of the tag tables in
// SYNC_PROTOCOL.md §6.3 and NODE_LINK_PROTOCOL.md §9.3. Validating it here keeps the re-encode
// assertion below from trusting an unchecked table.
function loadTranscriptRegistry() {
  if (!registry) return;
  const schema = readJson(transcriptSchemaPath);
  if (!schema) return;
  try {
    const validate = new Ajv2020({ allErrors: true, strict: false }).compile(schema);
    if (!validate(registry)) {
      for (const issue of validate.errors ?? []) {
        errors.push(`compatibility/transcripts/v1/transcripts.json${issue.instancePath} ${issue.message}`);
      }
    }
  } catch (error) {
    errors.push(`schemas/transcripts/transcripts.schema.json: cannot compile: ${error.message}`);
  }

  for (const [index, spec] of (registry.domains ?? []).entries()) {
    const label = `transcripts registry: domains[${index}] ${spec?.domain}`;
    if (!spec || typeof spec !== "object") {
      errors.push(`${label}: not an object`);
      continue;
    }
    const key = `${spec.protocol}:${spec.domain}`;
    if (transcriptDomains.has(key)) errors.push(`${label}: duplicate domain for protocol ${spec.protocol}`);
    transcriptDomains.set(key, spec);
    if (!transcriptTags.has(spec.protocol)) transcriptTags.set(spec.protocol, new Map());
    const tagTable = transcriptTags.get(spec.protocol);
    let previousTag = 0;
    for (const field of spec.fields ?? []) {
      if (!(field.tag > previousTag)) {
        errors.push(`${label}: fields must be strictly ascending by tag (${field.tag} after ${previousTag})`);
      }
      previousTag = field.tag;
      const seen = tagTable.get(field.tag);
      if (!seen) {
        tagTable.set(field.tag, { name: field.name, type: field.type });
      } else if (seen.name !== field.name || seen.type !== field.type) {
        errors.push(`${label}: tag ${field.tag} is ${seen.name}/${seen.type} in another domain, not ${field.name}/${field.type}`);
      }
    }
  }
}

function resolveInputKey(input, keyPath) {
  let cursor = input;
  for (const part of keyPath.split(".")) {
    if (cursor === null || typeof cursor !== "object" || !(part in cursor)) return undefined;
    cursor = cursor[part];
  }
  return cursor;
}

// Binary inputs are hex when the input key says so (`*Hex`, the form the docs use for nonces)
// and unpadded base64url otherwise (the wire form for public keys). Anything else is refused
// rather than silently guessed.
function decodeBinaryInput(text, inputKey, label) {
  if (typeof text !== "string") {
    errors.push(`${label}: input ${inputKey} must be a string`);
    return undefined;
  }
  if (inputKey.endsWith("Hex")) {
    if (!/^(?:[0-9a-fA-F]{2})+$/.test(text)) {
      errors.push(`${label}: input ${inputKey} is not even-length hex`);
      return undefined;
    }
    return Buffer.from(text, "hex");
  }
  if (!/^[A-Za-z0-9_-]+$/.test(text) || text.length % 4 === 1) {
    errors.push(`${label}: input ${inputKey} is not unpadded base64url`);
    return undefined;
  }
  const bytes = Buffer.from(text, "base64url");
  if (bytes.toString("base64url") !== text) {
    errors.push(`${label}: input ${inputKey} is not canonical unpadded base64url`);
    return undefined;
  }
  return bytes;
}

function encodeFieldBytes(field, input, label) {
  // A null inputKey marks a field whose bytes would have to be derived from a protocol constant.
  // The registry has no slot for that value, so re-encoding cannot invent it: fail loudly instead
  // of emitting bytes the vectors were never checked against (assignment rule: a field the
  // registry cannot supply is a hard error).
  if (field.inputKey === null) {
    errors.push(`${label}: inputKey is null; the registry must name the input key supplying this field`);
    return undefined;
  }
  const value = resolveInputKey(input ?? {}, field.inputKey);
  if (value === undefined) {
    errors.push(`${label}: input is missing ${field.inputKey}`);
    return undefined;
  }
  switch (field.type) {
    case "u16be":
    case "u64be": {
      let big;
      try {
        big = typeof value === "bigint" ? value : BigInt(value);
      } catch {
        errors.push(`${label}: input ${field.inputKey} is not an integer`);
        return undefined;
      }
      const max = field.type === "u16be" ? 0xffffn : 0xffffffffffffffffn;
      if (big < 0n || big > max) {
        errors.push(`${label}: input ${field.inputKey} is out of ${field.type} range`);
        return undefined;
      }
      const buffer = Buffer.alloc(fixedWidth[field.type]);
      if (field.type === "u16be") buffer.writeUInt16BE(Number(big), 0);
      else buffer.writeBigUInt64BE(big, 0);
      return buffer;
    }
    case "uuid16": {
      const hex = String(value).replaceAll("-", "").toLowerCase();
      if (!/^[0-9a-f]{32}$/.test(hex)) {
        errors.push(`${label}: input ${field.inputKey} is not a UUID`);
        return undefined;
      }
      return Buffer.from(hex, "hex");
    }
    case "sec1-65":
    case "bytes16":
    case "bytes32": {
      const bytes = decodeBinaryInput(value, field.inputKey, label);
      if (!bytes) return undefined;
      if (bytes.length !== fixedWidth[field.type]) {
        errors.push(`${label}: input ${field.inputKey} is ${bytes.length} bytes, expected ${fixedWidth[field.type]} for ${field.type}`);
        return undefined;
      }
      return bytes;
    }
    case "utf8": {
      if (typeof value !== "string") {
        errors.push(`${label}: input ${field.inputKey} must be a string`);
        return undefined;
      }
      if (value.includes("\u0000")) {
        errors.push(`${label}: input ${field.inputKey} must not contain NUL`);
        return undefined;
      }
      return Buffer.from(value, "utf8");
    }
    case "nul-joined-utf8": {
      if (!Array.isArray(value) || value.length === 0 || value.some((item) => typeof item !== "string" || item.includes("\u0000"))) {
        errors.push(`${label}: input ${field.inputKey} must be a non-empty array of NUL-free strings`);
        return undefined;
      }
      const sorted = [...value].sort();
      if (sorted.some((item, index) => item !== value[index])) {
        errors.push(`${label}: input ${field.inputKey} must be sorted (SYNC_PROTOCOL.md §6.2)`);
        return undefined;
      }
      return Buffer.from(value.join("\u0000"), "utf8");
    }
    default:
      errors.push(`${label}: unknown field type ${field.type}`);
      return undefined;
  }
}

// Re-encode the transcript from codec/domain/input. The vectors, not the prose, are the arbiter:
// a wrong tag number, width, field set or ordering makes these bytes differ from the vector.
function encodeTranscript(protocol, domain, input, label) {
  const spec = transcriptDomains.get(`${protocol}:${domain}`);
  if (!spec) {
    errors.push(`${label}: domain ${domain} is not registered for protocol ${protocol}`);
    return undefined;
  }
  const magic = Buffer.from(registry?.codec?.magic ?? "", "ascii");
  if (magic.length !== 4) {
    errors.push(`${label}: registry codec magic is not 4 ASCII bytes`);
    return undefined;
  }
  const domainBytes = Buffer.from(domain, "utf8");
  const header = Buffer.alloc(7);
  magic.copy(header, 0, 0, 4);
  header[4] = registry.codec.codecVersion;
  header.writeUInt16BE(domainBytes.length, 5);
  const count = Buffer.alloc(2);
  count.writeUInt16BE(spec.fields.length, 0);

  const chunks = [header, domainBytes, count];
  const consumed = new Set();
  for (const field of spec.fields) {
    const bytes = encodeFieldBytes(field, input, `${label} (tag ${field.tag} ${field.name})`);
    if (!bytes) return undefined;
    if (field.inputKey) consumed.add(field.inputKey);
    const fieldHeader = Buffer.alloc(6);
    fieldHeader.writeUInt16BE(field.tag, 0);
    fieldHeader.writeUInt32BE(bytes.length, 2);
    chunks.push(fieldHeader, bytes);
  }

  // A vector key that no registered field consumes is a field the registry cannot account for;
  // silently ignoring it is how a wrong tag table passes unnoticed.
  const unused = Object.keys(input ?? {}).filter((key) => !consumed.has(key));
  if (unused.length > 0) {
    errors.push(`${label}: input keys not consumed by domain ${domain}: ${unused.join(", ")}`);
  }
  return Buffer.concat(chunks);
}

// Structural transcript decoder for negative vectors: magic, version, domain, ascending unique
// registered tags, field widths, declared lengths vs remaining bytes, and no trailing bytes.
function decodeTranscriptInput(text, domain, protocol) {
  if (typeof text !== "string" || !/^[A-Za-z0-9_-]+$/.test(text) || text.length % 4 === 1) return "bad_base64url";
  const bytes = Buffer.from(text, "base64url");
  if (bytes.length === 0 || bytes.toString("base64url") !== text) return "bad_base64url";
  if (!transcriptDomains.has(`${protocol}:${domain}`)) return "unknown_domain";
  const magic = registry?.codec?.magic ?? "ACPR";
  if (bytes.length < 4 || bytes.subarray(0, 4).toString("ascii") !== magic) return "bad_magic";
  if (bytes.length < 5 || bytes[4] !== registry?.codec?.codecVersion) return "bad_codec_version";
  if (bytes.length < 7) return "truncated_domain";
  const domainLength = bytes.readUInt16BE(5);
  if (bytes.length < 7 + domainLength) return "truncated_domain";
  if (bytes.subarray(7, 7 + domainLength).toString("utf8") !== domain) return "unknown_domain";

  let offset = 7 + domainLength;
  if (bytes.length < offset + 2) return "truncated_field";
  const fieldCount = bytes.readUInt16BE(offset);
  offset += 2;
  const tagTable = transcriptTags.get(protocol) ?? new Map();
  let previousTag = 0;
  for (let index = 0; index < fieldCount; index += 1) {
    if (bytes.length < offset + 6) return "truncated_field";
    const tag = bytes.readUInt16BE(offset);
    const byteLength = bytes.readUInt32BE(offset + 2);
    offset += 6;
    if (!tagTable.has(tag)) return "unknown_tag";
    if (tag === previousTag) return "duplicate_tag";
    if (tag < previousTag) return "field_order";
    previousTag = tag;
    const width = fixedWidth[tagTable.get(tag).type];
    if (width !== undefined && byteLength !== width) return "length_mismatch";
    if (bytes.length < offset + byteLength) return "truncated_field";
    offset += byteLength;
  }
  if (offset !== bytes.length) return "trailing_bytes";
  return null;
}

// SEC1 uncompressed point validation: unpadded base64url, exactly 65 bytes, 0x04 prefix, and a
// point that satisfies the P-256 curve equation (the infinite point is not representable here).
function checkPublicKeyEncoding(text) {
  if (typeof text !== "string" || !/^[A-Za-z0-9_-]+$/.test(text) || text.length % 4 === 1) return "bad_base64url";
  const bytes = Buffer.from(text, "base64url");
  if (bytes.length === 0 || bytes.toString("base64url") !== text) return "bad_base64url";
  if (bytes.length !== 65) return "bad_length";
  if (bytes[0] !== 0x04) return "bad_point";
  const x = BigInt(`0x${bytes.subarray(1, 33).toString("hex")}`);
  const y = BigInt(`0x${bytes.subarray(33, 65).toString("hex")}`);
  const { p, a, b } = p256;
  if (x >= p || y >= p) return "not_on_curve";
  const mod = (value) => ((value % p) + p) % p;
  return mod(y * y - (x * x * x + a * x + b)) === 0n ? null : "not_on_curve";
}

function checkNegativeVector(vector, vectorPath, protocol) {
  const label = relative(root, vectorPath);
  const transcriptCase = typeof vector.malformedTranscriptBase64url === "string";
  const publicKeyCase = typeof vector.malformedPublicKeyBase64url === "string";
  if (transcriptCase === publicKeyCase) {
    errors.push(`${label}: exactly one of malformedTranscriptBase64url / malformedPublicKeyBase64url is required`);
    return;
  }
  if (vector.codec !== registry?.codec?.name) {
    errors.push(`${label}: codec ${vector.codec} is not ${registry?.codec?.name}`);
    return;
  }
  const allowed = transcriptCase ? transcriptErrors : publicKeyErrors;
  if (!allowed.includes(vector.expectedError)) {
    errors.push(`${label}: expectedError ${vector.expectedError} is not one of ${allowed.join("/")}`);
    return;
  }
  negativeCount += 1;
  const actual = transcriptCase
    ? decodeTranscriptInput(vector.malformedTranscriptBase64url, vector.domain, protocol)
    : checkPublicKeyEncoding(vector.malformedPublicKeyBase64url);
  if (actual === null) {
    errors.push(`${label}: malformed vector was accepted, expected ${vector.expectedError}`);
  } else if (actual !== vector.expectedError) {
    errors.push(`${label}: expected ${vector.expectedError}, decoder reported ${actual}`);
  }
}

async function checkTranscriptVector(vectorPath, protocol) {
  const label = relative(root, vectorPath);
  const vector = readJson(vectorPath);
  if (!vector) return;
  const expected = vector.expected;
  if (!expected || typeof expected.transcriptBase64url !== "string") {
    errors.push(`${label}: expected.transcriptBase64url missing`);
    return;
  }
  const transcript = Buffer.from(expected.transcriptBase64url, "base64url");
  if (vector.codec !== registry?.codec?.name) {
    errors.push(`${label}: codec ${vector.codec} is not ${registry?.codec?.name}`);
  } else {
    const spec = transcriptDomains.get(`${protocol}:${vector.domain}`);
    const reencoded = encodeTranscript(protocol, vector.domain, vector.input, label);
    if (reencoded) {
      reencodedCount += 1;
      coveredDomains.add(`${protocol}:${vector.domain}`);
      if (!reencoded.equals(transcript)) {
        errors.push(`${label}: re-encoded transcript does not match expected.transcriptBase64url`);
      }
    }
    // The registry's proof kind is only meaningful if it matches the material the vector carries.
    if (spec) {
      const hasHmac = typeof expected.hmacSha256 === "string";
      const hasSignature = typeof expected.p1363Signature === "string";
      if (hasHmac && hasSignature) {
        errors.push(`${label}: expected carries both hmacSha256 and p1363Signature`);
      } else if (spec.proof === "hmac" && !hasHmac) {
        errors.push(`${label}: registry declares proof=hmac but expected has no hmacSha256`);
      } else if (spec.proof === "signature" && !hasSignature) {
        errors.push(`${label}: registry declares proof=signature but expected has no p1363Signature`);
      }
    }
  }
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

// ---------------------------------------------------------------------------------------------
// 已删除词汇的禁词扫描（`pairing-model`：两套 SAS 与整个设备权限层一起删除）
//
// 删除资产本身不会让任何既有判据失败，因此这里把「不得回潮」写成判据：登记表、固定向量、schema 与
// fixture 里再出现这些词汇都会红——这才是删除后的独立断言，而不是「删了就绿」。
//
// 范围限于本仓库自有的机器资产（ACP 上游快照不是我们的词汇表，不参与）。判定分两类：
// - 键名与取值：`sas`/`pack`/`packs`/`presets`/`requestedPacks`/`requestedScopes` 作为键名或字符串
//   取值出现即失败；`pack.*`/`preset.*` 形态的取值同样失败；`…-sas/v1` 形态的 domain 值失败。
// - 字段位置：`scopes` 在 Node Link/Export 面仍是合法词汇（grant 集合），只有**设备方向**的
//   `scopes` 被删，因此那两处按位置断言（属性与 `required` 两处都查）。
// ---------------------------------------------------------------------------------------------

const ownedAssetRoots = [
  join(root, "compatibility"),
  join(root, "schemas", "sync"),
  join(root, "schemas", "node-link"),
  join(root, "schemas", "local-admin"),
  join(root, "schemas", "transcripts"),
  join(root, "fixtures", "sync"),
  join(root, "fixtures", "node-link"),
  join(root, "fixtures", "local-admin"),
];

const removedVocabulary = new Set(["sas", "pack", "packs", "presets", "requestedPacks", "requestedScopes"]);
const removedValueShape = /^(pack|preset)\./;
const sasDomainShape = /[-/]sas\/v1$/;

// 递归收集 JSON 里全部对象键与字符串叶子（附路径）：键名与取值都要看，避免漏判嵌套位置。
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

// 断言 `pointer` 指向的对象里不再有字段 `name`：三处都查——对象自己携带该成员（fixture 的形态）、
// 它作为 `properties` 的键（schema 的形态）、它出现在 `required` 里（两种形态都可能）。位置不存在即
// 失败：那说明这段判据的位置口径已经过期，不能静默通过。
function assertFieldRemoved(label, path, pointer, name) {
  const document = readJson(path);
  const parent = pointer.reduce((value, part) => value?.[part], document);
  if (!parent || typeof parent !== "object") {
    errors.push(`${label}: ${relative(root, path)} 的 ${pointer.join("/")} 不存在（判据的位置口径已过期）`);
    return;
  }
  if (Object.hasOwn(parent, name)) {
    errors.push(`${label}: ${relative(root, path)} 的 ${pointer.join("/")} 仍携带字段 ${name}`);
  }
  if (parent.properties && Object.hasOwn(parent.properties, name)) {
    errors.push(`${label}: ${relative(root, path)} 的 ${pointer.join("/")}.properties 仍声明 ${name}`);
  }
  if (Array.isArray(parent.required) && parent.required.includes(name)) {
    errors.push(`${label}: ${relative(root, path)} 的 ${pointer.join("/")}.required 仍要求 ${name}`);
  }
}

// 设备方向被删除的字段位置（schema 与 fixture 各一处；fixture 的指针直接指向承载对象）。
const removedDeviceFields = [
  ["设备配对状态不再返回授权集合", "schemas/sync/v1/pairing.schema.json", ["$defs", "statusResponse", "properties", "device"], "scopes"],
  ["设备认证结果不携带授权集合", "schemas/sync/v1/auth.schema.json", ["$defs", "authenticated", "properties", "body"], "scopes"],
  ["设备配对状态 fixture", "fixtures/sync/v1/valid/pairing-status-response.json", ["device"], "scopes"],
  ["设备认证 fixture", "fixtures/sync/v1/valid/auth-authenticated.json", ["body"], "scopes"],
  ["本地管理设备配对确认 fixture", "fixtures/local-admin/v1/valid/request-device-pair-confirm.json", ["params"], "scopes"],
];

function scanRemovedVocabulary() {
  for (const directory of ownedAssetRoots) {
    for (const path of walkJson(directory)) {
      const document = readJson(path);
      if (!document) continue;
      for (const [keyPath, text] of vocabulary(document)) {
        const where = `${relative(root, path)} 的 ${keyPath}`;
        if (removedVocabulary.has(text)) {
          errors.push(`${where}: 已删除的设备授权/短验证码词汇 ${text} 不得再出现`);
        } else if (removedValueShape.test(text)) {
          errors.push(`${where}: 已删除的 ${text} 词汇不得再出现`);
        } else if (sasDomainShape.test(text)) {
          errors.push(`${where}: 已删除的短验证码 domain ${text} 不得再出现`);
        }
      }
    }
  }
  for (const [label, path, pointer, name] of removedDeviceFields) {
    assertFieldRemoved(label, join(root, path), pointer, name);
  }
}

// ---------------------------------------------------------------------------------------------
// 设备二维码资产切换（`device-pairing-qr-reference`）：旧设备 `#data=` 完整资料退出，`device.pair.begin`
// 的 `pairingUrl` 改为 `…/pair#q2=<87 chars>`。删除旧 fixture 本身不会让任何既有判据变红，因此这里把
// 「旧资产不得回潮」与「新 pairingUrl 形状」写成判据；节点资产不在此范围（node-link fixture 仍用
// `#data=` 且必须保持不变）。
// ---------------------------------------------------------------------------------------------

const legacyDeviceFixture = join(root, "fixtures", "sync", "v1", "valid", "pairing-qr-payload.json");
const localAdminDevicePairBegin = join(
  root,
  "fixtures",
  "local-admin",
  "v1",
  "valid",
  "response-device-pair-begin.json",
);
const devicePairingUrlShape = /^https:\/\/[^/?#@]+\/pair#q2=[A-Za-z0-9_-]{87}$/;

function checkDeviceQrAssets() {
  if (existsSync(legacyDeviceFixture)) {
    errors.push(
      "设备二维码资产回潮：fixtures/sync/v1/valid/pairing-qr-payload.json 是已退出的旧设备完整资料格式",
    );
  }

  for (const path of walkJson(join(root, "fixtures", "local-admin"))) {
    const document = readJson(path);
    if (!document) continue;
    for (const [keyPath, text] of vocabulary(document)) {
      if (text.includes("#data=")) {
        errors.push(
          `${relative(root, path)} 的 ${keyPath}: 本地管理设备配对资产不得再包含旧设备 #data= 地址`,
        );
      }
    }
  }

  const begin = readJson(localAdminDevicePairBegin);
  if (!begin) {
    errors.push(
      "设备配对资产缺失：fixtures/local-admin/v1/valid/response-device-pair-begin.json 必须给出新 q2 pairingUrl 示例",
    );
    return;
  }
  const result = begin.result;
  const keys = result && typeof result === "object" ? Object.keys(result).sort() : [];
  const expectedKeys = ["expiresAt", "pairingCode", "pairingId", "pairingUrl"];
  if (keys.length !== expectedKeys.length || keys.some((key, index) => key !== expectedKeys[index])) {
    errors.push(
      `本地管理 device.pair.begin fixture 必须保持四个返回键 ${expectedKeys.join(",")}，实际 ${keys.join(",")}`,
    );
  }
  const pairingUrl = result?.pairingUrl;
  if (typeof pairingUrl !== "string" || !devicePairingUrlShape.test(pairingUrl)) {
    errors.push(
      "本地管理 device.pair.begin fixture 的 pairingUrl 必须是 https://<origin>/pair#q2=<87 chars>",
    );
  }
}

// ---------------------------------------------------------------------------------------------
// 附件传输帧族登记（`conversation-area-complete` WP1）：commands.json 的 `transportFrames` 是帧名与
// 运行预算的唯一跨协议登记，schema 的 `frameType`/`transferBudget`、协议文档的帧表与预算表都必须与它
// 相等。帧不是业务命令（它们不产生 command.result，也不进 commands 列表），因此登记放在单独的
// `transportFrames` 段，而不是伪装成命令。
// ---------------------------------------------------------------------------------------------

const transportDocSources = {
  sync: join(root, "docs", "SYNC_PROTOCOL.md"),
  node_link: join(root, "docs", "NODE_LINK_PROTOCOL.md"),
};

const budgetRowKeys = {
  单块原始数据: "chunkSize",
  每传输未确认块: "maxUnacknowledgedChunks",
  单设备活动传输: "maxActiveTransfersPerDevice",
  单节点活动传输: "maxActiveTransfersPerNode",
};

// 非附件的帧族（查看控制帧）：schema 没有 `$defs.frameType`/`$defs.transferBudget`，帧名是
// `message.schema.json` 可达的消息 `type`，预算按 §11.8 的固定表登记。两者的判据同样是「登记 ↔
// schema ↔ 文档逐项相等」，只是表的形状不同；没有登记口径的非附件帧族一律报错，不静默跳过。
const frameFamilies = {
  "core.local-session-lifecycle.v1": {
    framesHeading: "#### 查看控制帧族",
    budgetHeading: "#### 查看控制预算",
    budgetRows: {
      查看租约: "viewLeaseMs",
      续租间隔: "viewRenewalIntervalMs",
      每连接查看者: "maxViewersPerConnection",
      控制帧速率: "maxControlFramesPerMinute",
    },
  },
};

/**
 * 从信封 `message.schema.json` 的顶层 union 出发，收集**可达**的消息 `type`（`properties.type.const`）。
 *
 * 只沿 `$ref`/`allOf`/`oneOf`/`anyOf` 递归，并跨文件解析 `$ref`；不遍历容器本身——`properties`/
 * `items`/`$defs` 的内容只有在被上述关键字（或 `$ref`）明确指向时才进入。因此定义在 `$defs` 里、却没有
 * 被顶层 union 引用到的分支**不是消息**——这正是本函数存在的理由：全树扫描会把「写进 schema 但永远
 * 不会被选中的判别子」当成已登记消息。
 *
 * `state.types` 收集可达的 `type`，`state.files` 记录沿路解析到的文件（用于断言帧族 schema 自身
 * 也被信封引用），`state.seen` 以 `文件 + JSON pointer` 去重，保证 `$ref` 环不会无限递归。
 */
function collectReachableMessageTypes(schemaPath, node, state) {
  if (!node || typeof node !== "object") return state;
  state.files.add(schemaPath);
  if (typeof node.$ref === "string") {
    const [filePart, fragment = ""] = node.$ref.split("#");
    const targetPath = filePart ? resolve(dirname(schemaPath), filePart) : schemaPath;
    const pointer = fragment ? `#${fragment}` : "#";
    const key = `${targetPath}\u0000${pointer}`;
    if (state.seen.has(key)) return state;
    state.seen.add(key);
    // 目标缺失由 inspectRefs 统一报错，这里只跳过，避免同一处缺陷报两遍。
    if (existsSync(targetPath)) {
      const target = resolvePointer(readJson(targetPath), pointer);
      if (target !== undefined) collectReachableMessageTypes(targetPath, target, state);
    }
    return state;
  }
  const declared = node.properties?.type?.const;
  if (typeof declared === "string") state.types.add(declared);
  for (const keyword of ["allOf", "oneOf", "anyOf"]) {
    const branches = node[keyword];
    if (Array.isArray(branches)) {
      for (const branch of branches) collectReachableMessageTypes(schemaPath, branch, state);
    }
  }
  return state;
}

/** `heading` 之后第一张 markdown 表的行（单元格 trim、去掉分隔行）。 */
function tableAfterHeading(text, heading) {
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
    if (cells.every((cell) => /^-{2,}:?$/.test(cell))) continue;
    rows.push(cells);
  }
  return rows.length > 0 ? rows : undefined;
}

function checkTransportFrames() {
  const catalog = readJson(join(root, "compatibility", "commands", "v1", "commands.json"));
  const features = readJson(join(root, "compatibility", "features", "v1", "features.json"));
  if (!catalog || !features) return;
  const entries = catalog.transportFrames;
  if (!Array.isArray(entries) || entries.length === 0) {
    errors.push("compatibility/commands/v1/commands.json: transportFrames 必须登记 attachment 帧族");
    return;
  }

  const seenFamilies = new Set();
  for (const entry of entries) {
    const protocol = entry?.protocol;
    const familyId = `${protocol ?? "?"}:${entry?.feature ?? "?"}`;
    const label = `commands.json transportFrames[${familyId}]`;
    if (typeof protocol !== "string" || typeof entry?.feature !== "string") {
      errors.push(`${label}: protocol/feature 缺失`);
      continue;
    }
    if (seenFamilies.has(familyId)) {
      errors.push(`${label}: protocol/feature 组合重复`);
      continue;
    }
    seenFamilies.add(familyId);
    if (!(protocol in transportDocSources)) {
      errors.push(`${label}: 未知 protocol ${protocol}`);
      continue;
    }

    const featuresForProtocol = features.protocols?.[protocol]?.features ?? [];
    if (!featuresForProtocol.some((feature) => feature.id === entry.feature)) {
      errors.push(`${label}: feature ${entry.feature} 未在 compatibility/features/v1/features.json 的 ${protocol} 列表登记`);
    }

    const schemaPath = join(root, entry.schema ?? "");
    const schema = readJson(schemaPath);
    if (!schema) {
      errors.push(`${label}: schema ${entry.schema} 不可读`);
      continue;
    }
    const declaredFrames = schema.$defs?.frameType?.enum;
    const docText = readText(transportDocSources[protocol]);
    if (docText === undefined) continue;

    if (!Array.isArray(declaredFrames)) {
      // 查看控制帧族：帧名必须是该 schema 可达的消息 type，预算与帧表由 §11.8 的固定表登记。
      const family = frameFamilies[entry.feature];
      if (!family) {
        errors.push(`${label}: ${entry.schema} 没有 $defs.frameType，且该 feature 没有登记非附件帧族的文档口径`);
        continue;
      }
      const messageSchemaPath = join(dirname(schemaPath), "message.schema.json");
      const messageSchema = readJson(messageSchemaPath);
      if (!messageSchema) {
        errors.push(`${label}: 缺少信封 ${relative(root, messageSchemaPath)}，无法判定帧名是否可达`);
        continue;
      }
      const reachable = collectReachableMessageTypes(messageSchemaPath, messageSchema, {
        types: new Set(),
        files: new Set(),
        seen: new Set(),
      });
      if (!reachable.files.has(resolve(schemaPath))) {
        errors.push(
          `${label}: ${entry.schema} 没有被 ${relative(root, messageSchemaPath)} 的顶层 union 引用，登记的帧不可能到达客户端`,
        );
      }
      const frames = Array.isArray(entry.frames) ? entry.frames : [];
      if (frames.length === 0) {
        errors.push(`${label}: frames 必须非空`);
      }
      for (const frame of frames) {
        if (!reachable.types.has(frame)) {
          errors.push(
            `${label}: 帧 ${frame} 不在 ${relative(root, messageSchemaPath)} 顶层 union 的**可达**消息 type 里` +
              `（未沿 $ref/allOf/oneOf/anyOf 引用到，或拼写不符）`,
          );
        }
      }
      const frameRows = tableAfterHeading(docText, family.framesHeading);
      if (!frameRows) {
        errors.push(`${label}: ${relative(root, transportDocSources[protocol])} 缺少「${family.framesHeading}」表`);
      } else {
        const docFrames = frameRows.slice(1).map((cells) => (cells[0] ?? "").replace(/`/g, ""));
        if (JSON.stringify(docFrames) !== JSON.stringify(frames)) {
          errors.push(
            `${label}: 协议文档帧表与 commands.json frames 不一致（文档 ${docFrames.length} 行，登记 ${frames.length} 个）`,
          );
        }
      }
      const budgetRows = tableAfterHeading(docText, family.budgetHeading);
      if (!budgetRows) {
        errors.push(`${label}: ${relative(root, transportDocSources[protocol])} 缺少「${family.budgetHeading}」表`);
      } else {
        const docBudget = new Map(budgetRows.slice(1).map((cells) => [cells[0], cells[1]]));
        for (const [rowLabel, key] of Object.entries(family.budgetRows)) {
          const cell = docBudget.get(rowLabel);
          const value = Number.parseInt(String(cell ?? "").replace(/[^0-9]/g, ""), 10);
          if (!Number.isInteger(value) || value !== entry.budget?.[key]) {
            errors.push(`${label}: 协议文档预算行「${rowLabel}」为 ${JSON.stringify(cell)}，登记为 ${entry.budget?.[key]}`);
          }
        }
      }
      continue;
    }

    if (JSON.stringify(declaredFrames) !== JSON.stringify(entry.frames)) {
      errors.push(
        `${label}: frames 与 ${entry.schema} 的 $defs.frameType.enum 不一致（登记 ${entry.frames?.length} 个，schema ${declaredFrames.length} 个）`,
      );
    }

    const documentedBudget = schema.$defs?.transferBudget?.properties ?? {};
    for (const [key, value] of Object.entries(entry.budget ?? {})) {
      const declared = documentedBudget[key]?.const;
      if (declared !== value) {
        errors.push(`${label}: budget.${key} 为 ${value}，${entry.schema} 为 ${JSON.stringify(declared)}`);
      }
    }

    const frameRows = tableAfterHeading(docText, "#### 帧族（`attachment.*`）");
    if (!frameRows) {
      errors.push(`${label}: ${relative(root, transportDocSources[protocol])} 缺少「#### 帧族（\`attachment.*\`）」表`);
    } else {
      const docFrames = frameRows.slice(1).map((cells) => (cells[0] ?? "").replace(/`/g, ""));
      if (JSON.stringify(docFrames) !== JSON.stringify(entry.frames)) {
        errors.push(`${label}: 协议文档帧表与 commands.json frames 不一致（文档 ${docFrames.length} 行，登记 ${entry.frames?.length} 个）`);
      }
    }

    const budgetRows = tableAfterHeading(docText, "#### 附件传输预算");
    if (!budgetRows) {
      errors.push(`${label}: ${relative(root, transportDocSources[protocol])} 缺少「#### 附件传输预算」表`);
    } else {
      const docBudget = new Map(budgetRows.slice(1).map((cells) => [cells[0], cells[1]]));
      for (const [rowLabel, key] of Object.entries(budgetRowKeys)) {
        const cell = docBudget.get(rowLabel);
        const value = Number.parseInt(String(cell ?? "").replace(/[^0-9]/g, ""), 10);
        if (!Number.isInteger(value) || value !== entry.budget?.[key]) {
          errors.push(`${label}: 协议文档预算行「${rowLabel}」为 ${JSON.stringify(cell)}，登记为 ${entry.budget?.[key]}`);
        }
      }
    }
  }

  for (const protocol of ["sync", "node_link"]) {
    if (!entries.some((entry) => entry?.protocol === protocol)) {
      errors.push("commands.json transportFrames 必须同时登记 sync 与 node_link 两个协议");
    }
  }
}

loadTranscriptRegistry();
scanRemovedVocabulary();
checkDeviceQrAssets();
checkTransportFrames();
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
      if (typeof schema.$id === "string") {
        const seen = schemaIds.get(schema.$id);
        if (seen) {
          errors.push(
            `${assetRoot.name}: duplicate $id "${schema.$id}" in ${relative(root, path)} and ${relative(root, seen)}`,
          );
        } else {
          schemaIds.set(schema.$id, path);
        }
      }
    }

    for (const path of fixturePaths) {
      const fixture = readJson(path);
      if (fixture) {
        checkRawAcp(fixture, path);
        checkPayloadDigest(fixture, path);
      }
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
        continue;
      }
      const listed = readJson(vectorPath);
      if (!listed) continue;
      if (typeof listed.malformedTranscriptBase64url === "string" || typeof listed.malformedPublicKeyBase64url === "string") {
        checkNegativeVector(listed, vectorPath, assetRoot.protocol);
      } else {
        await checkTranscriptVector(vectorPath, assetRoot.protocol);
      }
    }
  }

  // A registered domain with no vector is an unverified contract surface, and the registry is
  // the only machine copy of the tag tables the encoder trusts.
  for (const [key, spec] of transcriptDomains) {
    if (!coveredDomains.has(key)) {
      errors.push(`transcripts registry: domain ${spec.domain} (${spec.protocol}) has no fixture vector`);
    }
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `contract assets OK: ${schemaCount} schemas, ${fixtureCount} fixture files, ` +
      `${reencodedCount} transcript vectors re-encoded from input, ` +
      `${negativeCount} negative vectors rejected as declared, ` +
      `无 SAS/设备授权词汇残留`,
  );
}
