import { existsSync, readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const matrixPath = resolve(root, "compatibility", "acp", "v1", "matrix.json");
const errors = [];

function readJson(path, label = relative(root, path)) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    errors.push(`${label}: invalid JSON: ${error.message}`);
    return undefined;
  }
}

const matrix = readJson(matrixPath);

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

function compareSet(label, actual, wanted) {
  const sortedActual = [...actual].sort();
  const sortedWanted = [...wanted].sort();
  if (JSON.stringify(sortedActual) !== JSON.stringify(sortedWanted)) {
    const missing = sortedWanted.filter((value) => !sortedActual.includes(value));
    const extra = sortedActual.filter((value) => !sortedWanted.includes(value));
    if (missing.length) errors.push(`${label}: missing ${missing.join(", ")}`);
    if (extra.length) errors.push(`${label}: unexpected ${extra.join(", ")}`);
  }
}

function checkRows(groupName, rows, key) {
  const allowedLayers = {
    acp: new Set(["native", "raw_preserve", "transport_control"]),
    broker: new Set(["project_and_preserve", "local_service", "pass_through", "explicit_unsupported", "not_applicable"]),
    sync: new Set(["command", "event", "snapshot", "raw_fallback", "explicit_unsupported", "not_exposed"]),
    pwa: new Set(["full", "view_only", "explicit_unsupported", "not_applicable"]),
    facade: new Set(["baseline", "advertise_if_end_to_end", "not_advertised", "not_applicable"])
  };
  const allowedDeliveries = new Set(["mvp", "conditional_mvp", "post_mvp", "always"]);
  const ids = new Set();
  const values = new Set();
  for (const row of rows ?? []) {
    if (!row.id || ids.has(row.id)) errors.push(`${groupName}: missing/duplicate id ${row.id}`);
    ids.add(row.id);
    if (!row[key] || values.has(row[key])) errors.push(`${groupName}: missing/duplicate ${key} ${row[key]}`);
    values.add(row[key]);
    if (!row.layers || Object.keys(row.layers).sort().join(",") !== "acp,broker,facade,pwa,sync") {
      errors.push(`${row.id}: layers must contain exactly acp, broker, sync, pwa, facade`);
    }
    for (const [layer, value] of Object.entries(row.layers ?? {})) {
      if (!allowedLayers[layer]?.has(value)) errors.push(`${row.id}: invalid ${layer} behavior ${value}`);
    }
    if (!allowedDeliveries.has(row.delivery)) errors.push(`${row.id}: invalid delivery ${row.delivery}`);
    if (!Array.isArray(row.tests) || row.tests.length === 0) errors.push(`${row.id}: tests must not be empty`);
    if (Object.values(row.layers ?? {}).includes("silent_drop")) errors.push(`${row.id}: silent_drop is forbidden`);
  }
  return values;
}

if (matrix) {
  if (matrix.protocol?.wireVersion !== 1) errors.push("protocol.wireVersion must be 1");
  if (!/^[0-9a-f]{40}$/.test(matrix.protocol?.sourceCommit ?? "")) errors.push("protocol.sourceCommit must be pinned");
  if (!/^[0-9a-f]{64}$/.test(matrix.protocol?.schemaSha256 ?? "")) errors.push("protocol.schemaSha256 must be pinned");
  if (matrix.nodeLinkPolicy?.hopLimit !== 1) errors.push("nodeLinkPolicy.hopLimit must be 1 for the first release");
  if (matrix.nodeLinkPolicy?.capabilityRule !== "end_to_end_intersection") errors.push("nodeLinkPolicy must require end-to-end capability intersection");
  if (matrix.nodeLinkPolicy?.rawAcp !== "byte_exact_or_explicit_raw_unavailable") errors.push("nodeLinkPolicy must preserve raw ACP or fail explicitly");
  if (matrix.nodeLinkPolicy?.contentPersistence !== "owner_only_default") errors.push("nodeLinkPolicy must keep session content on the Owner by default");
  if (matrix.nodeLinkPolicy?.trustModel !== "access_node_principal") errors.push("nodeLinkPolicy must use the Access Node as the first-release principal");
  if (matrix.nodeLinkPolicy?.remoteSessionCreate !== "exported_agent_and_workspace_template_only") errors.push("nodeLinkPolicy must constrain remote session creation to exported Agents and workspace templates");
  if (matrix.nodeLinkPolicy?.routeFencing !== "attachment_generation") errors.push("nodeLinkPolicy must fence stale session routes with attachment generations");

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

  compareSet("methods", checkRows("methods", matrix.methods, "wireName"), expected.methods);
  compareSet("sessionUpdates", checkRows("sessionUpdates", matrix.sessionUpdates, "wireValue"), expected.sessionUpdates);
  compareSet("contentBlocks", checkRows("contentBlocks", matrix.contentBlocks, "wireValue"), expected.contentBlocks);
  compareSet("toolCallContents", checkRows("toolCallContents", matrix.toolCallContents, "wireValue"), expected.toolCallContents);
  compareSet("capabilities", checkRows("capabilities", matrix.capabilities, "path"), expected.capabilities);

  for (const method of matrix.methods ?? []) {
    if (method.requirement === "optional" && !method.capability) errors.push(`${method.id}: optional method lacks capability gate`);
    if (method.layers?.facade === "baseline" && method.requirement !== "baseline") errors.push(`${method.id}: facade baseline overclaims a non-baseline method`);
  }
  for (const wireName of ["initialize", "session/new", "session/prompt", "session/cancel", "session/update"]) {
    const method = (matrix.methods ?? []).find((row) => row.wireName === wireName);
    if (!method?.tests?.includes("node_link.contract")) errors.push(`${wireName}: first Node Link vertical slice requires node_link.contract`);
  }

  const requiredInvariants = new Set([
    "invariant.unknown_fields_byte_exact", "invariant.tool_call_stays_structured",
    "invariant.capability_truthful", "invariant.future_update_visible",
    "invariant.node_link_raw_byte_exact", "invariant.node_link_capability_intersection"
  ]);
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
  for (const id of requiredInvariants) if (!invariantIds.has(id)) errors.push(`invariants: missing ${id}`);
}

if (errors.length) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `ACP compatibility matrix OK: ${matrix.methods.length} methods, ` +
    `${matrix.sessionUpdates.length} updates, ${matrix.contentBlocks.length} content blocks, ` +
    `${matrix.toolCallContents.length} tool content types, ${matrix.capabilities.length} capabilities`
  );
}
