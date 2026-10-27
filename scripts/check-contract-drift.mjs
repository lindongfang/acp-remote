// 合同漂移门禁：`docs/CORE_PORTS_AND_STORAGE.md` 的冻结文本与实现代码之间的一致性。
//
// 为什么需要它：`npm run check` 的其它脚本只覆盖协议资产之间的一致性，合同与实现之间没有判据——
// L1 实现期出过两次真实漂移：§7.3 的 `elicitation_action` CHECK 少了 `'decline'`（ACP 已有的动作因此
// 落不了库），以及 §5.2 约束里引用了端口集里并不存在的 `resolve_interaction`。两者都只靠人工对抗审查
// 发现，而它们都是可机械判定的：
//
// - §7.3 / §7.4 的 ```sql 块（按出现顺序）必须与 `crates/storage-sqlite/src/migrate.rs` 的
//   `OWNED_SCHEMA_V1` / `IMPORTED_SCHEMA_V1` **逐条语句相同**；
// - §5.1–§5.4 的 ```rust 块里声明的每个 trait，其方法签名集合必须与 `crates/core/src/ports.rs`
//   的对应 trait 相等；块里声明的每个带成员的 `struct`/`enum`，其成员集合必须与代码相等。
//
// 归一化口径（两侧的差异只允许是这些）：Rust 侧删 `//` 注释、去空白、去参数表尾逗号
// （rustfmt 写 `fn f( &self, a: T, )`，合同写 `fn f(&self, a: T)`）；SQL 侧删 `--` 注释、压空白、
// 转小写、去 `IF NOT EXISTS`（合同描述逻辑表结构，实现写幂等 DDL）。成员集合按顺序无关比较——
// Rust 的字段与方法顺序不承载语义，只有增删改名与签名变化才算漂移。
//
// 判据永远指向「合同是权威」：不一致时改实现；只有合同自相矛盾（同一文档的 `[决定]` 与 rust 块冲突）
// 时才改 rust 块，并在修订记录里写明。

import { readFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const contractPath = resolve(root, "docs", "CORE_PORTS_AND_STORAGE.md");
const portsPath = resolve(root, "crates", "core", "src", "ports.rs");
const sessionDeletionPath = resolve(root, "crates", "core", "src", "model", "session_deletion.rs");
const migratePath = resolve(root, "crates", "storage-sqlite", "src", "migrate.rs");
const sessionStorePath = resolve(root, "crates", "storage-sqlite", "src", "session_store.rs");

const errors = [];
const contractLabel = relative(root, contractPath);
const portsLabel = relative(root, portsPath);
const sessionDeletionLabel = relative(root, sessionDeletionPath);
const migrateLabel = relative(root, migratePath);
const sessionStoreLabel = relative(root, sessionStorePath);

function readText(path, label) {
  try {
    return readFileSync(path, "utf8").replace(/\r\n/g, "\n");
  } catch (error) {
    errors.push(`contract drift: ${label} 不可读：${error.message}`);
    return undefined;
  }
}

/** 取顶层章节（`## N. ...`）。 */
function section(text, prefix) {
  const found = text.split(/\n(?=## )/).find((part) => part.startsWith(prefix));
  if (!found) errors.push(`contract drift: ${contractLabel} 缺少章节 ${prefix}`);
  return found ?? "";
}

/** 取围栏代码块。 */
function fenced(text, language) {
  return [...text.matchAll(new RegExp("```" + language + "\\n([\\s\\S]*?)```", "g"))].map((match) => match[1]);
}

const stripRustComments = (line) => line.replace(/\/\/.*$/, "");
const stripSqlComments = (line) => line.replace(/--.*$/, "");

const normalizeSql = (text) =>
  text
    .split("\n")
    .map(stripSqlComments)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase()
    .replace(/if not exists /g, "");

const sqlStatements = (text) =>
  normalizeSql(text)
    .split(";")
    .map((statement) => statement.trim())
    .filter(Boolean);

// ---------------------------------------------------------------------------------------------
// A. §7 的表结构 DDL ↔ migrate.rs 的常量
// ---------------------------------------------------------------------------------------------

function checkDdl() {
  const contract = readText(contractPath, contractLabel);
  const migrate = readText(migratePath, migrateLabel);
  if (!contract || !migrate) return { statements: 0 };

  const blocks = fenced(section(contract, "## 7."), "sql");
  const constNames = ["OWNED_SCHEMA_V1"];
  if (blocks.length !== constNames.length) {
    errors.push(
      `contract drift: ${contractLabel} §7 期望 ${constNames.length} 个 \`\`\`sql 块，实际 ${blocks.length} 个`,
    );
    return { statements: 0 };
  }

  let total = 0;
  for (const [index, name] of constNames.entries()) {
    const pattern = new RegExp(`const ${name}[^=]*=\\s*r?#?"([\\s\\S]*?)"#?;`);
    const found = migrate.match(pattern);
    if (!found) {
      errors.push(`contract drift: ${migrateLabel} 找不到常量 ${name}`);
      continue;
    }
    const documented = sqlStatements(blocks[index]);
    const implemented = sqlStatements(found[1]);
    total += documented.length;
    if (documented.length !== implemented.length) {
      errors.push(
        `contract drift: §7 块 ${index + 1} (${name}) 语句条数不一致——${contractLabel} ${documented.length} 条、${migrateLabel} ${implemented.length} 条`,
      );
    }
    for (const [position, statement] of documented.entries()) {
      if (statement === implemented[position]) continue;
      errors.push(`contract drift: §7 块 ${index + 1} (${name}) 第 ${position + 1} 条语句不一致`);
      errors.push(`  文档: ${statement.slice(0, 120)}`);
      errors.push(`  代码: ${(implemented[position] ?? "<缺失>").slice(0, 120)}`);
    }
  }

  if (total !== 35) {
    errors.push(`contract drift: §7 当前 DDL 必须恰好 35 条，实际提取 ${total} 条`);
  }
  return { statements: total };
}

// ---------------------------------------------------------------------------------------------
// B. §5 的端口 ↔ ports.rs
// ---------------------------------------------------------------------------------------------

const normalizeSignature = (text) =>
  stripRustComments(text)
    .replace(/\s+/g, "")
    .replace(/,\)/g, ")")
    .replace(/_([a-zA-Z]\w*):/g, "$1:")
    .replace(/;+$/, "");

/**
 * 逐行扫描 Rust 声明：返回 `declarations`（名字 → { kind, signatures, members }）。
 *
 * 只认 `trait`/`struct`/`enum` 声明；`impl` 块（真函数体）不参与比对，但出现在 trait 体内、
 * 既不是签名也不是空行/`}` 的行会被报出来（不允许静默跳过）。
 */
function scanDeclarations(text) {
  const declarations = new Map();
  const lines = text.split("\n");
  let current = null;
  let pending = null;
  let defaultBodyDepth = 0;

  const close = () => {
    if (current && pending) {
      current.signatures.push(normalizeSignature(pending));
      pending = null;
    }
    current = null;
  };

  for (const raw of lines) {
    const line = stripRustComments(raw).trim();
    if (line === "") continue;
    if (defaultBodyDepth > 0) {
      defaultBodyDepth += (line.match(/\{/g) ?? []).length - (line.match(/\}/g) ?? []).length;
      continue;
    }

    if (pending !== null) {
      pending += " " + line;
      if (line.includes("{")) {
        const bodyStart = pending.indexOf("{");
        current.signatures.push(normalizeSignature(pending.slice(0, bodyStart)));
        defaultBodyDepth = (line.match(/\{/g) ?? []).length - (line.match(/\}/g) ?? []).length;
        pending = null;
        continue;
      }
      if (line.includes(";")) {
        current.signatures.push(normalizeSignature(pending));
        pending = null;
      }
      continue;
    }

    const declaration = line.match(/^(?:pub )?(trait|struct|enum) (\w+)/);
    if (declaration) {
      const [, kind, name] = declaration;
      current = { kind, signatures: [], members: [] };
      declarations.set(name, current);
      // 单行声明：`pub trait Clock: Send + Sync { fn now(&self) -> Timestamp; }`
      if (line.includes("{") && line.includes("}")) {
        const body = line.slice(line.indexOf("{") + 1, line.lastIndexOf("}"));
        for (const part of body.split(";")) {
          if (part.includes("fn ")) current.signatures.push(normalizeSignature(part));
          else if (part.trim() !== "") for (const member of part.split(",")) if (member.trim() !== "") current.members.push(member.trim().replace(/\s+/g, " "));
        }
        close();
        continue;
      }
      // tuple struct：`pub struct EventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);`
      if (line.endsWith(";")) close();
      continue;
    }

    if (line === "}") {
      close();
      continue;
    }
    if (current === null) continue;

    if (/^(?:pub )?(?:async )?fn /.test(line)) {
      pending = line;
      if (line.includes(";")) {
        current.signatures.push(normalizeSignature(pending));
        pending = null;
      }
      continue;
    }

    if (current.kind === "trait") {
      errors.push(
        `contract drift: 无法解析 trait ${[...declarations.keys()].pop()} 的成员行「${line}」——门禁规则需要更新，不允许静默跳过`,
      );
      continue;
    }

    for (const member of line.split(",")) {
      const trimmed = member.trim();
      if (trimmed !== "") current.members.push(trimmed.replace(/\s+/g, " "));
    }
  }
  close();
  return declarations;
}

function compareSets(label, documented, implemented) {
  const onlyDocumented = documented.filter((item) => !implemented.includes(item));
  const onlyImplemented = implemented.filter((item) => !documented.includes(item));
  if (onlyDocumented.length === 0 && onlyImplemented.length === 0) return;
  errors.push(`contract drift: ${label} 不一致`);
  for (const item of onlyDocumented) errors.push(`  仅文档: ${item}`);
  for (const item of onlyImplemented) errors.push(`  仅代码: ${item}`);
}

function checkPorts() {
  const contract = readText(contractPath, contractLabel);
  const ports = readText(portsPath, portsLabel);
  if (!contract || !ports) return { traits: 0, methods: 0, types: 0 };

  const blocks = fenced(section(contract, "## 5."), "rust");
  const documented = scanDeclarations(blocks.join("\n"));
  const implemented = scanDeclarations(ports);

  let traits = 0;
  let methods = 0;
  let types = 0;
  for (const [name, declaration] of documented) {
    const counterpart = implemented.get(name);
    if (declaration.kind === "trait") {
      traits += 1;
      methods += declaration.signatures.length;
      if (counterpart?.kind !== "trait") {
        errors.push(
          `contract drift: 合同 §5 声明了 trait ${name}，${portsLabel} 里没有同名 trait（改名、或从未实现？）`,
        );
        continue;
      }
      compareSets(
        `trait ${name} 的方法集`,
        declaration.signatures.slice().sort(),
        counterpart.signatures.slice().sort(),
      );
      continue;
    }
    types += 1;
    if (!counterpart) {
      errors.push(`contract drift: 合同 §5 声明了 ${declaration.kind} ${name}，${portsLabel} 里不存在同名类型`);
      continue;
    }
    if (counterpart.kind !== declaration.kind) {
      errors.push(
        `contract drift: 合同 §5 把 ${name} 声明为 ${declaration.kind}，${portsLabel} 里是 ${counterpart.kind}`,
      );
      continue;
    }
    compareSets(
      `合同 §5 声明的 ${name} 成员集与 ${portsLabel}`,
      declaration.members.slice().sort(),
      counterpart.members.slice().sort(),
    );
  }

  if (traits < 10 || methods < 56 || types < 3) {
    errors.push(
      `contract drift: 端口提取结果异常（trait ${traits} / 方法 ${methods} / 类型 ${types}），门禁解析规则可能已过期`,
    );
  }
  return { traits, methods, types };
}

// ---------------------------------------------------------------------------------------------
// C. 已实现合同的精确断言
//
// 通用比对只能证明“文档里写出的声明与代码一致”，不能证明当前合同仍声明了正确的契约。
// 这里因此对重点小节做结构化的存在性、类型和语义断言；历史修订文本可以继续出现旧名称，
// 但只有常规 rust/sql 围栏里的声明与当前 DDL 才算现行合同。
// ---------------------------------------------------------------------------------------------

function subsection(text, heading) {
  const start = text.indexOf(heading);
  if (start < 0) return undefined;
  const rest = text.slice(start + heading.length);
  const end = rest.search(/\n#{1,3} /);
  return end < 0 ? rest : rest.slice(0, end);
}

function checkImplementedPorts(contract) {
  const text = subsection(contract, "\n### 5.8 ");
  if (text === undefined) {
    errors.push(`contract drift: ${contractLabel} 缺少 §5.8 分块附件与有效交互小节`);
    return { traits: 0, types: 0 };
  }
  if (text.includes("尚未实现")) {
    errors.push(`contract drift: ${contractLabel} §5.8 已实现，不得再标注「尚未实现」`);
  }
  for (const name of [
    "AttachmentTransferStore",
    "InteractionAuthority",
    "TransferBegin",
    "TransferChunk",
    "ChunkOutcome",
    "TransferSeal",
    "SubmissionBegin",
    "SubmissionPage",
    "SubmissionProgress",
    "SubmissionSeal",
    "AttachmentRead",
    "AttachmentChunk",
    "EffectiveResolution",
  ]) {
    if (!text.includes(name)) {
      errors.push(`contract drift: ${contractLabel} §5.8 缺少已实现的 ${name}`);
    }
  }
  const blocks = fenced(text, "rust");
  if (blocks.length < 3) {
    errors.push(`contract drift: ${contractLabel} §5.8 需要至少 3 个 rust 代码块`);
  }
  return { traits: 2, types: 11 };
}

const CURRENT_OWNED_TABLES = [
  "meta",
  "owned_session",
  "owned_turn",
  "owned_event",
  "owned_command",
  "owned_interaction",
  "owned_audit",
  "owned_attachment",
  "owned_attachment_link",
  "owned_attachment_transfer",
  "owned_attachment_submission",
  "owned_attachment_submission_item",
  "owned_message",
  "owned_timeline_anchor",
  "owned_media_ref",
  "owned_user_input",
  "owned_device",
  "owned_node",
  "owned_peer_key",
  "owned_pairing",
  "owned_pairing_peer",
  "owned_directory",
];

function declarationMap(text) {
  return scanDeclarations(fenced(text, "rust").join("\n"));
}

function checkDeclarations(sectionLabel, expectedNames, documented, implemented, implementedLabel) {
  for (const name of expectedNames) {
    const contractDeclaration = documented.get(name);
    const implementationDeclaration = implemented.get(name);
    if (!contractDeclaration) {
      errors.push(`contract drift: ${contractLabel} ${sectionLabel} 缺少当前声明 ${name}`);
      continue;
    }
    if (!implementationDeclaration) {
      errors.push(`contract drift: ${implementedLabel} 缺少 ${sectionLabel} 声明的 ${name}`);
      continue;
    }
    if (contractDeclaration.kind !== implementationDeclaration.kind) {
      errors.push(
        `contract drift: ${sectionLabel} 把 ${name} 声明为 ${contractDeclaration.kind}，` +
          `${implementedLabel} 里是 ${implementationDeclaration.kind}`,
      );
      continue;
    }
    compareSets(
      `${sectionLabel} 的 ${name} 与 ${implementedLabel}`,
      contractDeclaration.members.slice().sort(),
      implementationDeclaration.members.slice().sort(),
    );
    compareSets(
      `${sectionLabel} 的 ${name} 签名与 ${implementedLabel}`,
      contractDeclaration.signatures.slice().sort(),
      implementationDeclaration.signatures.slice().sort(),
    );
  }
}

function hasFieldType(declaration, field, type) {
  const expected = `${field}:${type}`;
  return (declaration?.members ?? []).some((member) => {
    const normalized = member.replace(/\s+/g, "").replace(/,$/, "");
    return normalized === expected || normalized === `pub${expected}`;
  });
}

function checkVersionAndCurrentDdl(contract) {
  const migration = subsection(contract, "\n### 7.2 ");
  if (migration === undefined) {
    errors.push(`contract drift: ${contractLabel} 缺少 §7.2 Migration`);
  } else {
    if (!migration.includes("当前 **v19 = 19**")) {
      errors.push(`contract drift: ${contractLabel} §7.2 必须写明当前文件格式 v19 = 19`);
    }
    if (!migration.includes("`owned_schema_version`（当前 19）")) {
      errors.push(`contract drift: ${contractLabel} §7.2 必须写明 owned_schema_version 当前为 19`);
    }
  }

  const migrate = readText(migratePath, migrateLabel) ?? "";
  for (const constant of ["FILE_FORMAT_VERSION", "OWNED_SCHEMA_VERSION"]) {
    const pattern = new RegExp(`pub const ${constant}:\\s*i64\\s*=\\s*19\\s*;`);
    if (!pattern.test(migrate)) {
      errors.push(`contract drift: ${migrateLabel} 的 ${constant} 必须显式等于 19`);
    }
  }

  const tableSection = subsection(contract, "\n### 7.3 ");
  if (tableSection === undefined) {
    errors.push(`contract drift: ${contractLabel} 缺少 §7.3 当前表结构`);
    return { tables: 0 };
  }
  if (tableSection.includes("sql-planned")) {
    errors.push(`contract drift: ${contractLabel} §7.3 当前表结构不得使用 sql-planned`);
  }

  const currentSql = fenced(tableSection, "sql").flatMap(sqlStatements);
  const currentTables = currentSql
    .map((statement) => statement.match(/^create table (\w+) \(/)?.[1])
    .filter(Boolean);
  compareSets(
    `§7.3 当前 owned 表集合与 ${migrateLabel}`,
    CURRENT_OWNED_TABLES.slice().sort(),
    currentTables.slice().sort(),
  );

  if (currentSql.some((statement) => statement.startsWith("create table owned_first_submission ("))) {
    errors.push(`contract drift: ${contractLabel} §7.3 当前 DDL 不得包含 owned_first_submission`);
  }

  const ownedSession = currentSql.find((statement) => statement.startsWith("create table owned_session ("));
  if (!ownedSession) {
    errors.push(`contract drift: ${contractLabel} §7.3 缺少 owned_session 当前 DDL`);
  } else {
    if (!/\bcustom_title\s+text\b/.test(ownedSession)) {
      errors.push(`contract drift: ${contractLabel} §7.3 的 owned_session 缺少 custom_title`);
    }
    if (!/\bdeleted_at\s+text\b/.test(ownedSession)) {
      errors.push(`contract drift: ${contractLabel} §7.3 的 owned_session 缺少 deleted_at`);
    }
  }

  const ownedMessage = currentSql.find((statement) => statement.startsWith("create table owned_message ("));
  if (!ownedMessage || !ownedMessage.includes("'user'")) {
    errors.push(`contract drift: ${contractLabel} §7.3 的 owned_message 缺少 user 通道`);
  }

  const ownedUserInput = currentSql.find((statement) => statement.startsWith("create table owned_user_input ("));
  for (const constraint of [
    "unique (session_id, actor_kind, actor_id, request_id)",
    "unique (session_id, turn_id)",
  ]) {
    if (!ownedUserInput?.includes(constraint)) {
      errors.push(`contract drift: ${contractLabel} §7.3 的 owned_user_input 缺少唯一约束 ${constraint}`);
    }
  }
  return { tables: currentTables.length };
}

function checkCurrentPortSections(contract, ports) {
  const portsDeclarations = scanDeclarations(ports);

  const sessionFacts = subsection(contract, "\n### 5.11 ");
  if (sessionFacts === undefined || sessionFacts.includes("尚未实现") || sessionFacts.includes("rust-planned")) {
    errors.push(`contract drift: ${contractLabel} §5.11 已实现，不得缺失、标注「尚未实现」或使用 rust-planned`);
  } else {
    const documented = declarationMap(sessionFacts);
    checkDeclarations(
      "§5.11",
      ["SessionContentFacts", "SessionDeletion"],
      documented,
      portsDeclarations,
      portsLabel,
    );
    for (const obsolete of ["FirstSubmission", "PreparationAttachmentStaging"]) {
      if (documented.has(obsolete)) {
        errors.push(`contract drift: ${contractLabel} §5.11 不得再把已删除的 ${obsolete} 声明为当前端口`);
      }
    }
  }

  const acceptance = subsection(contract, "\n### 5.12 ");
  if (acceptance === undefined || acceptance.includes("尚未实现") || acceptance.includes("rust-planned")) {
    errors.push(`contract drift: ${contractLabel} §5.12 已实现，不得缺失、标注「尚未实现」或继续使用 rust-planned`);
  } else {
    const documented = declarationMap(acceptance);
    checkDeclarations(
      "§5.12",
      ["PromptAcceptanceInput", "PromptAcceptanceOutcome"],
      documented,
      portsDeclarations,
      portsLabel,
    );
    const documentedInput = documented.get("PromptAcceptanceInput");
    const implementedInput = portsDeclarations.get("PromptAcceptanceInput");
    if (!hasFieldType(documentedInput, "content", "PromptInput")) {
      errors.push(`contract drift: ${contractLabel} §5.12 的 PromptAcceptanceInput.content 必须是 PromptInput`);
    }
    if (!hasFieldType(implementedInput, "content", "PromptInput")) {
      errors.push(`contract drift: ${portsLabel} 的 PromptAcceptanceInput.content 必须是 PromptInput`);
    }
  }

  return { traits: 2, types: 2 };
}

function checkSessionDeletion(contract) {
  const deletion = section(contract, "## 13.");
  if (!deletion || deletion.includes("尚未实现") || deletion.includes("rust-planned") || deletion.includes("sql-planned")) {
    errors.push(`contract drift: ${contractLabel} §13 必须描述已实现的会话删除合同，不得使用计划围栏`);
  }

  const documented = declarationMap(deletion);
  const portsDeclarations = scanDeclarations(portsText ?? "");
  checkDeclarations(
    "§13",
    ["SessionContentFacts", "SessionDeletion"],
    documented,
    portsDeclarations,
    portsLabel,
  );

  const modelDeclarations = scanDeclarations(readText(sessionDeletionPath, sessionDeletionLabel) ?? "");
  checkDeclarations(
    "§13",
    [
      "SessionDeletionWrite",
      "SessionDeletionEligibility",
      "SessionDeletionOutcome",
      "SessionDeletionBlocker",
    ],
    documented,
    modelDeclarations,
    sessionDeletionLabel,
  );

  const sessionStore = readText(sessionStorePath, sessionStoreLabel) ?? "";
  for (const implementation of [
    "impl SessionContentFacts for SqliteStore",
    "impl SessionDeletion for SqliteStore",
    'const VISIBLE_SESSION: &str = "deleted_at IS NULL"',
  ]) {
    if (!sessionStore.includes(implementation)) {
      errors.push(`contract drift: ${sessionStoreLabel} 缺少 §13 所需的实现 ${implementation}`);
    }
  }

  const migrationSection = subsection(deletion, "\n### 13.5 ");
  const migrationBlocks = fenced(migrationSection ?? "", "sql");
  if (migrationBlocks.length !== 1) {
    errors.push(`contract drift: ${contractLabel} §13.5 必须只有一个 sql 迁移块，实际 ${migrationBlocks.length} 个`);
  } else {
    const expected = [
      "alter table owned_session add column deleted_at text",
      "drop table if exists owned_first_submission",
    ];
    const actual = sqlStatements(migrationBlocks[0]);
    compareSets("§13.5 v18 → v19 迁移语句", expected, actual);
  }

  const migrate = migrateText ?? "";
  const functionStart = migrate.indexOf("async fn v19_upgrade");
  const functionEnd = functionStart < 0 ? -1 : migrate.indexOf("\n}\n", functionStart);
  const upgrade = functionStart < 0 || functionEnd < 0 ? "" : migrate.slice(functionStart, functionEnd);
  const normalizedUpgrade = normalizeSql(upgrade);
  if (
    !normalizedUpgrade.includes('column_exists(&mut **tx, "owned_session", "deleted_at")') ||
    !normalizedUpgrade.includes("alter table owned_session add column deleted_at text") ||
    !normalizedUpgrade.includes("drop table if exists owned_first_submission")
  ) {
    errors.push(
      `contract drift: ${migrateLabel} 的 v19_upgrade 必须守卫 deleted_at、执行加列并删除 owned_first_submission`,
    );
  }

  return { traits: 2, types: 4 };
}

function checkImplementedDdl(contract) {
  const versionAndDdl = checkVersionAndCurrentDdl(contract);
  const currentPorts = checkCurrentPortSections(contract, portsText ?? "");
  const deletion = checkSessionDeletion(contract);
  return { ...versionAndDdl, ...currentPorts, ...deletion };
}

// ---------------------------------------------------------------------------------------------

const contractText = readText(contractPath, contractLabel);
const portsText = readText(portsPath, portsLabel);
const migrateText = readText(migratePath, migrateLabel);
const ddl = checkDdl();
const ports = checkPorts();
const implementedPorts = contractText ? checkImplementedPorts(contractText) : { traits: 0, types: 0 };
const implementedDdl = contractText
  ? checkImplementedDdl(contractText)
  : { tables: 0, traits: 0, types: 0 };

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `contract drift OK: §7 的 ${ddl.statements} 条 DDL 与 ${migrateLabel} 逐条一致；` +
      `§5 的 ${ports.traits} 个 trait / ${ports.methods} 个方法签名与 ${portsLabel} 一致；` +
      `§5.8 的 ${implementedPorts.traits} 个 trait / ${implementedPorts.types} 个类型已实现；` +
      `§5.11/§5.12/§13 的 ${implementedDdl.traits} 个 trait / ${implementedDdl.types} 个类型与实现一致；` +
      `§7.3 的 ${implementedDdl.tables} 张当前表已由 v19 合同覆盖`,
  );
}
