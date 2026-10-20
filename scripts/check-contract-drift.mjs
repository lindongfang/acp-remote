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
const migratePath = resolve(root, "crates", "storage-sqlite", "src", "migrate.rs");

const errors = [];
const contractLabel = relative(root, contractPath);
const portsLabel = relative(root, portsPath);
const migrateLabel = relative(root, migratePath);

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
  const constNames = ["OWNED_SCHEMA_V1", "IMPORTED_SCHEMA_V1"];
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

  if (total < 20) {
    errors.push(`contract drift: DDL 提取结果异常（文档 ${total} 条），门禁解析规则可能已过期`);
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
// C. 计划合同的「尚未实现」断言
//
// `conversation-area-complete` 的 WP1 需要在冻结端口与表结构的同时声明它们**尚未实现**——
// 若把计划形状写进 §5 的 ```rust / §7 的 ```sql 块，drift 会立刻报「合同声明了代码里没有的
// trait / 语句」。因此计划块使用独立围栏 ```rust-planned / ```sql-planned，并由本节断言：
//   - 计划 trait/struct 若已出现在 ports.rs，则必须与声明逐项相等（实现后允许直接移入 §5）；
//   - 计划 DDL 语句若已出现在 migrate.rs，则失败并提示把块移入 §7 的常规 DDL 与 migration；
//   - 计划块的提取数量必须非零，避免围栏改名后这条判据静默失效。
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

function checkImplementedDdl(contract) {
  const text = section(contract, "## 7.");
  if (!text.includes("v12 = 12") || !text.includes("当前 12")) {
    errors.push(`contract drift: ${contractLabel} §7.2 必须写明当前版本 v12 = 12 / owned_schema_version 12`);
  }
  for (const table of [
    "owned_attachment_transfer",
    "owned_attachment_submission",
    "owned_attachment_submission_item",
    "owned_message",
    "owned_media_ref",
  ]) {
    if (!text.toLowerCase().includes(table)) {
      errors.push(`contract drift: ${contractLabel} §7.3 缺少已实现的表 ${table}`);
    }
  }
  if (text.includes("sql-planned")) {
    errors.push(`contract drift: ${contractLabel} §7 不得再含 sql-planned 块（已实现）`);
  }
  return 5;
}

// ---------------------------------------------------------------------------------------------

const contractText = readText(contractPath, contractLabel);
const portsText = readText(portsPath, portsLabel);
const migrateText = readText(migratePath, migrateLabel);
const ddl = checkDdl();
const ports = checkPorts();
const implementedPorts = contractText ? checkImplementedPorts(contractText) : { traits: 0, types: 0 };
const implementedDdl = contractText ? checkImplementedDdl(contractText) : 0;

if (errors.length > 0) {
  for (const error of errors) console.error(error);
  process.exitCode = 1;
} else {
  console.log(
    `contract drift OK: §7 的 ${ddl.statements} 条 DDL 与 ${migrateLabel} 逐条一致；` +
      `§5 的 ${ports.traits} 个 trait / ${ports.methods} 个方法签名与 ${portsLabel} 一致；` +
      `§5.8 的 ${implementedPorts.traits} 个 trait / ${implementedPorts.types} 个类型已实现；§7 的 ${implementedDdl} 张新表已进入 ${migrateLabel}`,
  );
}
