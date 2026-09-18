import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

// structural contract → schemas/acp/compatibility-matrix.schema.json (validated by ajv)
// semantic contract  → the checks below, which a schema cannot express: upstream
// coverage sets, per-row uniqueness, test-family membership and fixture existence.

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const matrixPath = resolve(root, "compatibility", "acp", "v1", "matrix.json");
const schemaPath = resolve(root, "schemas", "acp", "compatibility-matrix.schema.json");
const errors = [];

function readJson(path, label = relative(root, path)) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    errors.push(`${label}: invalid JSON: ${error.message}`);
    return undefined;
  }
}

function summarize(validateErrors) {
  if ((validateErrors ?? []).length === 0) return "";
  const list = [...validateErrors];
  const depth = (error) => (error.instancePath || "").split("/").length;
  const deepest = Math.max(...list.map(depth));
  return list
    .filter((error) => depth(error) === deepest)
    .slice(0, 3)
    .map((error) => `${error.instancePath || "/"} ${error.keyword}${error.message ? ` ${error.message}` : ""}`)
    .join(" | ");
}

const matrix = readJson(matrixPath);
const matrixSchema = readJson(schemaPath);

if (matrix && matrixSchema) {
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateFormats: true });
  addFormats(ajv);
  try {
    const validate = ajv.compile(matrixSchema);
    if (!validate(matrix)) {
      errors.push(`matrix.json violates its schema: ${summarize(validate.errors)}`);
      for (const error of validate.errors ?? []) {
        errors.push(`  schema: ${error.instancePath || "/"} ${error.keyword} ${JSON.stringify(error.params ?? {})}`);
      }
    }
  } catch (error) {
    errors.push(`cannot compile matrix schema: ${error.message}`);
  }
}

// Names that must exist because the pinned upstream snapshot defines them.
const expected = {
  methods: [
    "$/cancel_request", "authenticate", "elicitation/complete", "elicitation/create",
    "fs/read_text_file", "fs/write_text_file", "initialize", "logout", "session/cancel",
    "session/close", "session/delete", "session/list", "session/load", "session/new",
    "session/prompt", "session/request_permission", "session/resume", "session/set_config_option",
    "session/set_mode", "session/update", "terminal/create", "terminal/kill", "terminal/output",
    "terminal/release", "terminal/wait_for_exit"
  ],
  sessionUpdates: [
    "agent_message_chunk", "agent_thought_chunk", "available_commands_update",
    "config_option_update", "current_mode_update", "plan", "session_info_update",
    "tool_call", "tool_call_update", "usage_update", "user_message_chunk"
  ],
  contentBlocks: ["audio", "image", "resource", "resource_link", "text"],
  toolCallContents: ["content", "diff", "terminal"],
  capabilities: [
    "agentCapabilities.auth.logout",
    "agentCapabilities.loadSession",
    "agentCapabilities.mcpCapabilities.http",
    "agentCapabilities.mcpCapabilities.sse",
    "agentCapabilities.promptCapabilities.audio",
    "agentCapabilities.promptCapabilities.embeddedContext",
    "agentCapabilities.promptCapabilities.image",
    "agentCapabilities.sessionCapabilities.additionalDirectories",
    "agentCapabilities.sessionCapabilities.close",
    "agentCapabilities.sessionCapabilities.delete",
    "agentCapabilities.sessionCapabilities.list",
    "agentCapabilities.sessionCapabilities.resume",
    "clientCapabilities.auth.terminal",
    "clientCapabilities.elicitation.form",
    "clientCapabilities.elicitation.url",
    "clientCapabilities.fs.readTextFile",
    "clientCapabilities.fs.writeTextFile",
    "clientCapabilities.session.configOptions.boolean",
    "clientCapabilities.terminal"
  ]
};

const nodeLinkSliceMethods = ["initialize", "session/new", "session/prompt", "session/cancel", "session/update"];

const requiredInvariants = [
  "invariant.unknown_fields_byte_exact",
  "invariant.tool_call_stays_structured",
  "invariant.capability_truthful",
  "invariant.future_update_visible",
  "invariant.meta_fields_byte_exact",
  "invariant.extension_method_explicit_unsupported",
  "invariant.node_link_raw_byte_exact",
  "invariant.node_link_capability_intersection"
];

const groups = [
  { name: "methods", rows: matrix?.methods, key: "wireName", wanted: expected.methods },
  { name: "sessionUpdates", rows: matrix?.sessionUpdates, key: "wireValue", wanted: expected.sessionUpdates },
  { name: "contentBlocks", rows: matrix?.contentBlocks, key: "wireValue", wanted: expected.contentBlocks },
  { name: "toolCallContents", rows: matrix?.toolCallContents, key: "wireValue", wanted: expected.toolCallContents },
  { name: "capabilities", rows: matrix?.capabilities, key: "path", wanted: expected.capabilities },
];

for (const group of groups) {
  const keys = (group.rows ?? []).map((row) => row[group.key]);
  const missing = group.wanted.filter((value) => !keys.includes(value));
  const extra = keys.filter((value) => !group.wanted.includes(value));
  if (missing.length) errors.push(`${group.name}: missing ${missing.join(", ")}`);
  if (extra.length) errors.push(`${group.name}: unexpected ${extra.join(", ")}`);
  const seen = new Set();
  for (const key of keys) {
    if (seen.has(key)) errors.push(`${group.name}: duplicate ${group.key} ${key}`);
    seen.add(key);
  }
  const ids = new Set();
  for (const row of group.rows ?? []) {
    if (!row.id || ids.has(row.id)) errors.push(`${group.name}: missing/duplicate id ${row.id}`);
    ids.add(row.id);
  }
}

if (matrix) {
  const familyIds = new Set((matrix.testFamilies ?? []).map((family) => family.id));
  const allRows = [
    ...(matrix.methods ?? []), ...(matrix.sessionUpdates ?? []), ...(matrix.contentBlocks ?? []),
    ...(matrix.toolCallContents ?? []), ...(matrix.capabilities ?? []), ...(matrix.invariants ?? [])
  ];
  for (const row of allRows) {
    for (const test of row.tests ?? []) {
      if (!familyIds.has(test)) errors.push(`${row.id}: unknown test family ${test}`);
    }
  }
  for (const test of matrix.nodeLinkPolicy?.tests ?? []) {
    if (!familyIds.has(test)) errors.push(`nodeLinkPolicy: unknown test family ${test}`);
  }

  for (const wireName of nodeLinkSliceMethods) {
    const method = (matrix.methods ?? []).find((row) => row.wireName === wireName);
    if (!method?.tests?.includes("node_link.contract")) {
      errors.push(`${wireName}: first Node Link vertical slice requires node_link.contract`);
    }
  }

  const invariantIds = new Set();
  for (const item of matrix.invariants ?? []) {
    invariantIds.add(item.id);
    const fixturePath = resolve(dirname(matrixPath), item.fixture);
    if (!existsSync(fixturePath)) {
      errors.push(`${item.id}: missing fixture ${item.fixture}`);
    } else {
      readJson(fixturePath, item.fixture);
    }
  }
  for (const id of requiredInvariants) {
    if (!invariantIds.has(id)) errors.push(`invariants: missing ${id}`);
  }

  // The matrix pins the upstream ACP snapshot by commit + sha256. As long as that snapshot is
  // not vendored under schemas/acp/v1/upstream/, the digest cannot be recomputed, so say so
  // explicitly instead of implying the pin was verified.
  const snapshotPath = resolve(root, "schemas", "acp", "v1", "upstream", "schema.json");
  if (existsSync(snapshotPath)) {
    const actual = createHash("sha256").update(readFileSync(snapshotPath)).digest("hex");
    if (actual !== matrix.protocol?.schemaSha256) {
      errors.push(
        `schemas/acp/v1/upstream/schema.json: sha256 ${actual} != matrix protocol.schemaSha256 ${matrix.protocol?.schemaSha256}`,
      );
    }
    const sourceCommit = String(matrix.protocol?.sourceCommit ?? "");
    if (sourceCommit === "" || !String(matrix.protocol?.schemaUrl ?? "").includes(sourceCommit)) {
      errors.push("protocol.schemaUrl must contain protocol.sourceCommit");
    }
    // The vendored snapshot carries no version field of its own (top level is
    // $schema/title/anyOf/$defs), so the pinned path segment `schema/v1/` plus
    // protocol.wireVersion is the only machine-checkable version statement.
    const wireVersion = matrix.protocol?.wireVersion;
    if (wireVersion !== 1) {
      errors.push(`protocol.wireVersion must be 1 for the pinned snapshot, got ${JSON.stringify(wireVersion)}`);
    }
    if (!String(matrix.protocol?.schemaUrl ?? "").includes(`/v${wireVersion}/`)) {
      errors.push(`protocol.schemaUrl must pin the major version directory /v${wireVersion}/`);
    }
  } else {
    console.warn(
      "warning: ACP upstream snapshot is not vendored (schemas/acp/v1/upstream/schema.json); " +
        "matrix protocol.schemaSha256 未重算校验",
    );
  }
}

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  const counted = groups.map((group) => group.rows.length);
  const rowCount = counted.reduce((total, value) => total + value, 0) + (matrix.invariants?.length ?? 0);
  console.log(
    `ACP compatibility matrix OK: ${counted[0]} methods, ${counted[1]} updates, ${counted[2]} content blocks, ` +
      `${counted[3]} tool content types, ${counted[4]} capabilities, ${matrix.invariants.length} invariants, ` +
      `${matrix.testFamilies.length} test families (${rowCount} rows, schema validated by ajv)`,
  );
}
