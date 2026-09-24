// 依赖方向门禁：以 docs/MODULE_ARCHITECTURE.md §5 的依赖矩阵为唯一判据，逐 crate 校验实际依赖。
//
// 为什么读文档而不是硬编码一份矩阵：矩阵是权威、且会随架构决策变化（ADR/文档修订）；在脚本里复制
// 一份就立刻产生第二处定义，二者迟早漂移。这里解析文档表格，再把每个 crate 的 `[dependencies]` /
// `[dev-dependencies]` / `[build-dependencies]` 中指向**本工作区成员**的边逐条比对。
//
// 依赖来源用 `cargo metadata`（Cargo 自己的解析结果），不自己解析 TOML：多行 inline table、
// workspace 继承、`path` 依赖这些形态都交给 Cargo。
//
// 规则：
// - 表头列出的 crate 是「允许被依赖的对象」，行是「发起方」；`✓` 允许、`—` 是自身、空白禁止。
// - 工作区里尚未落地的 crate 不参与矩阵比对（成员随增量增长，见 §3.1），但其依赖必须都是矩阵认识的 crate。
// - 第三方依赖不在此门禁范围（只判项目内 crate 之间的方向）；core 的 runtime/DB/HTTP/子进程/wire
//   依赖是硬约束（§4.1），单独列出。

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const architecture = readFileSync(join(root, "docs", "MODULE_ARCHITECTURE.md"), "utf8");

/** docs/MODULE_ARCHITECTURE.md §5 的矩阵：`allowed.get(from).get(to) === true` 表示允许。 */
function readDependencyMatrix(document) {
  const lines = document.split(/\r?\n/);
  const heading = lines.findIndex((line) => line.trim() === "## 5. 依赖矩阵");
  if (heading < 0) throw new Error("docs/MODULE_ARCHITECTURE.md 缺少 §5 依赖矩阵");
  const headerIndex = lines.findIndex(
    (line, index) => index > heading && line.trim().startsWith("| From / To |"),
  );
  if (headerIndex < 0) throw new Error("§5 找不到依赖矩阵表头");

  const columns = lines[headerIndex]
    .split("|")
    .slice(1, -1)
    .map((cell) => cell.trim())
    .filter((cell) => cell !== "" && cell !== "From / To");

  const allowed = new Map();
  for (let index = headerIndex + 2; index < lines.length; index += 1) {
    const line = lines[index].trim();
    if (!line.startsWith("|")) break;
    const cells = line
      .split("|")
      .slice(1, -1)
      .map((cell) => cell.trim());
    const from = cells[0];
    // 每一行都要收：§5 矩阵当前有 9 个「列」（可被依赖的对象：core、acp-protocol、agent-host、
    // 两个协议 crate、acpr-transcript、acpr-wire、identity-auth、identity-keystore）；
    // `node-link-client` / `storage-sqlite` / `server` / `app` 仍只作为「行」存在，跳过它们会让
    // 这些 crate 的依赖边无人检查（`MODULE_ARCHITECTURE.md` §5 表下同样披露了这一点）。
    const row = new Map();
    columns.forEach((to, columnIndex) => {
      if (cells[columnIndex + 1] === "✓") row.set(to, true);
    });
    allowed.set(from, row);
  }
  return { columns, allowed };
}

/** 一次 `cargo metadata`：返回工作区成员的全部声明依赖与其中的工作区内依赖。 */
function readWorkspace() {
  const raw = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--locked"],
    {
      cwd: root,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_TERM_COLOR: "never" },
    },
  );
  const metadata = JSON.parse(raw);
  const declared = new Map();
  const workspaceEdges = new Map();
  for (const pkg of metadata.packages) {
    const names = new Set();
    const edges = new Set();
    for (const dependency of pkg.dependencies ?? []) {
      names.add(dependency.name);
      // `path` 存在 = 指向工作区成员；registry 依赖不参与方向门禁。
      if (typeof dependency.path === "string") edges.add(dependency.name);
    }
    declared.set(pkg.name, names);
    workspaceEdges.set(pkg.name, edges);
  }
  return { declared, workspaceEdges };
}

/** core 的硬约束（§4.1 与 docs/CORE_PORTS_AND_STORAGE.md §2）。 */
const CORE_FORBIDDEN = [
  "tokio",
  "async-std",
  "smol",
  "sqlx",
  "rusqlite",
  "libsqlite3-sys",
  "axum",
  "hyper",
  "tower",
  "warp",
  "tokio-tungstenite",
  "tungstenite",
  "acp-protocol",
  "sync-protocol",
  "node-link-protocol",
  "storage-sqlite",
  "agent-host",
  "node-link-client",
  "server",
  "app",
];

/** 所有依赖（含 dev/build）的名字，用于 core 的硬约束检查。 */
function allDeclaredDependencies() {
  const raw = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--locked"],
    {
      cwd: root,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, CARGO_TERM_COLOR: "never" },
    },
  );
  const metadata = JSON.parse(raw);
  const result = new Map();
  for (const pkg of metadata.packages) {
    const names = new Set((pkg.dependencies ?? []).map((dependency) => dependency.name));
    result.set(pkg.name, names);
  }
  return result;
}

const errors = [];
const { columns, allowed } = readDependencyMatrix(architecture);
const { declared, workspaceEdges } = readWorkspace();

for (const [name, dependencies] of workspaceEdges) {
  const row = allowed.get(name);
  for (const dependency of dependencies) {
    if (!columns.includes(dependency)) {
      errors.push(
        `${name}：依赖 ${dependency} 不在 §5 的矩阵列里——新增 crate 必须同时更新矩阵（docs/MODULE_ARCHITECTURE.md §5）`,
      );
      continue;
    }
    if (!row) continue;
    if (!row.get(dependency)) {
      errors.push(`${name}：依赖 ${dependency} 违反 §5 依赖矩阵（该格为空白）`);
    }
  }
}

const coreDependencies = declared.get("core");
if (coreDependencies) {
  for (const forbidden of CORE_FORBIDDEN) {
    if (coreDependencies.has(forbidden)) {
      errors.push(
        `core：不允许依赖 ${forbidden}（§4.1：core 不依赖 runtime/DB/HTTP/子进程/wire protocol）`,
      );
    }
  }
}

/**
 * `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13 的前半：core 的**普通依赖闭包**必须与冻结 allow-list 逐项相等。
 * 判据原文写的是「`cargo tree -p core --edges normal` 的输出逐行相等（黄金文件）」——这里比的是名字集合而不是
 * 版本号：版本随补丁升级变动，而「多出一个 crate」才是需要人做决定的事件（`AGENTS.md` §7）。新增依赖必须同时
 * 改这里与合同 §9 判据 13 —— 与「矩阵只有一处定义」同一原则。
 */
const CORE_ALLOWED_CLOSURE = [
  // core 的直接依赖（§9 判据 13：四个）——`p256`/`sha2` 是 §11.5 的 `PeerPublicKey` 构造期校验与指纹派生所需。
  "async-trait",
  "thiserror",
  "p256",
  "sha2",
  // 上面四者的 proc-macro 与曲线栈传递依赖（`p256` 只开 `arithmetic`：曲线点校验需要的
  // `sec1`/`elliptic-curve`/`primeorder`/`group`/`ff` 等；`sha2` 的 `digest`/`block-buffer`；
  // 以及它们共用的 `generic-array`/`hybrid-array`/`subtle`/`zeroize`）。它们随上述依赖出现，
  // 不是独立选择；新增任何一个都必须先按 AGENTS.md §7 审查。**签名与编码栈不在闭包内**：
  // core 既不签名也不验签，因此 `ecdsa`/`rfc6979`/`hmac`/`signature`/`pkcs8`/`spki`/`pem-rfc7468`/
  // `base64ct` 都不应出现在这里——出现即说明有人把 workspace 的默认 feature 又放开了（`sec1` 自带的
  // `base16ct` 十六进制解码仍在闭包内）。
  "base16ct",
  "block-buffer",
  "cfg-if",
  "const-oid",
  "cpufeatures",
  "crypto-bigint",
  "crypto-common",
  "der",
  "digest",
  "elliptic-curve",
  "ff",
  "generic-array",
  "group",
  "hybrid-array",
  "primeorder",
  "proc-macro2",
  "quote",
  "rand_core",
  "sec1",
  "subtle",
  "syn",
  "thiserror-impl",
  "typenum",
  "unicode-ident",
  "zeroize",
];

/** `cargo tree -p core --edges normal` 的可执行 crate 名集合（不含 core 自身）。 */
function coreNormalClosure() {
  const raw = execFileSync("cargo", ["tree", "-p", "core", "--edges", "normal", "--prefix", "none", "--no-dedupe"], {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
  });
  const names = new Set();
  for (const line of raw.split(/\r?\n/)) {
    const match = line.match(/^([A-Za-z0-9_-]+) v[0-9]/);
    if (match && match[1] !== "core") names.add(match[1]);
  }
  return names;
}

if (workspaceEdges.has("core")) {
  const closure = coreNormalClosure();
  for (const name of CORE_ALLOWED_CLOSURE) {
    if (!closure.has(name)) errors.push(`core：冻结 allow-list 里的 ${name} 已不在依赖闭包中（更新 §9 判据 13 与本表）`);
  }
  for (const name of closure) {
    if (!CORE_ALLOWED_CLOSURE.includes(name)) {
      errors.push(`core：依赖闭包出现未登记成员 ${name}——新增依赖必须先按 AGENTS.md §7 审查并在 §9 判据 13 与本表登记`);
    }
  }
}

const registered = [...workspaceEdges.keys()].filter((name) => allowed.has(name));
if (errors.length > 0) {
  console.error(`crate boundary check failed with ${errors.length} problem(s):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}
console.log(
  `crate boundaries OK: ${workspaceEdges.size} 个 crate 的依赖方向与 §5 矩阵一致（${registered.length} 个已在矩阵登记）`,
);
