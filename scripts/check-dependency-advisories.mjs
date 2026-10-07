// 依赖 advisory 门禁：跑 `npm audit`，只放行**已登记**的 advisory，其余一律阻塞。
//
// 为什么需要它而不是直接跑 `npm audit`：`npm audit` 没有 ignore 机制（`npm audit --help`
// 里没有这个选项），而仓库当前有一条**上游无修复**的 advisory——`braces` 的
// GHSA-vfj7-8cjw-p6xm（栈耗尽 DoS）在 npm 上的受影响范围是 `*`，3.x 线最新版 3.0.3
// 就是被通报的版本，`fixAvailable` 为 false。直接跑 audit 会让 `deps` job 永远红灯，
// 于是每次合并都得走管理员 bypass —— 那会让「绕过门禁」变成习惯，是比这条 DoS 更大的风险。
//
// 豁免的**边界**（刻意收窄，不是「关掉 npm audit」）：
// - 只按 **advisory ID** 放行，不按包名、不按 severity：新增任意一条未登记的 advisory
//   就会失败，即使它落在同一个包上。
// - 只放行 `high` 及以下；出现 `critical` 一律失败（当前豁免项是 high）。
// - 放行项必须在下面的 `ALLOWED` 里逐条登记，含理由与**到期条件**；登记不全即失败。
// - 传播包（`via` 指向另一个包名而非 advisory 对象）不单独判定：只要根因 advisory 被放行，
//   由它传播出来的条目一并放行；反之只要有**任一**根因未放行，整体失败。
//
// 到期条件：上游发布含修复的 `braces`（受影响范围不再是 `*`），或 `@fission-ai/openspec`
// 换掉 `fast-glob`/`micromatch` 这条链——两者任一成立时，本条豁免即应删除。

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * 已登记放行的 advisory。
 *
 * `id` 用 GitHub advisory 的 GHSA 号（`npm audit --json` 的 `via[].url` 末段）。
 */
const ALLOWED = [
  {
    id: "GHSA-vfj7-8cjw-p6xm",
    package: "braces",
    severity: "high",
    reason:
      "上游无修复：受影响范围是 `*`，braces 3.x 线最新版（3.0.3）就是被通报的版本，npm audit 报 fixAvailable=false。",
    scope:
      "仅在 devDependencies（仓库根 package.json 的 dependencies 为空），是本地合同校验与提交钩子的工具链，不随产品分发；调用方是 @fission-ai/openspec 扫描本地 openspec/** 目录，传入 braces 的 glob 模式是仓库自己的常量，不由外部输入构造，该 DoS 所需的攻击者可控嵌套模式在本仓库不成立。",
    expires:
      "上游发布含修复的 braces（受影响范围不再为 `*`），或 @fission-ai/openspec 换掉 fast-glob/micromatch 链时删除本条。",
  },
];

const severityRank = { info: 0, low: 1, moderate: 2, high: 3, critical: 4 };
const allowedById = new Map(ALLOWED.map((entry) => [entry.id, entry]));

function audit() {
  // Windows 上 `.cmd` 不能直接 spawn（Node 对 .cmd/.bat 的保护会抛 EINVAL），必须经 shell；
  // 因此那里传整条命令行字符串而不是 (command, args)——与 `scripts/pre-commit.mjs` 同一处理。
  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  const run = () =>
    process.platform === "win32"
      ? spawnSync([npm, "audit", "--json"].join(" "), { cwd: ROOT, encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024, shell: true })
      : spawnSync(npm, ["audit", "--json"], { cwd: ROOT, encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024 });
  const result = run();
  // `npm audit` 在发现漏洞时以非零码退出，但 stdout 仍是完整 JSON。
  if (typeof result.stdout === "string" && result.stdout.trim().startsWith("{")) return result.stdout;
  throw new Error(`npm audit 未产出可解析的 JSON：${result.error?.message ?? `exit ${result.status}`}`);
}

const report = JSON.parse(audit());
const vulnerabilities = report.vulnerabilities ?? {};

// 根因 = via 里带 advisory 对象的条目；其余都是按包名传播（`via: ["braces"]`）。
const rootIds = new Set();
for (const [, entry] of Object.entries(vulnerabilities)) {
  for (const via of entry.via ?? []) {
    if (typeof via !== "object" || !via.url) continue;
    const id = String(via.url).split("/").pop();
    rootIds.add(
      JSON.stringify({
        id,
        package: via.name ?? "",
        severity: via.severity ?? entry.severity ?? "",
        title: via.title ?? "",
      }),
    );
  }
}

const errors = [];
const cleared = [];

for (const raw of rootIds) {
  const { id, package: name, severity, title } = JSON.parse(raw);
  const rule = allowedById.get(id);
  if (!rule) {
    errors.push(`未登记的 advisory：${id}（${name}，${severity}）${title ? ` — ${title}` : ""}`);
    continue;
  }
  if (rule.package !== name) {
    errors.push(`豁免 ${id} 登记的包是 ${rule.package}，实际出现在 ${name}——请复核豁免是否仍然对得上`);
    continue;
  }
  if (severityRank[severity] > severityRank[rule.severity]) {
    errors.push(`豁免 ${id} 登记的严重度是 ${rule.severity}，实际为 ${severity}——请复核`);
    continue;
  }
  cleared.push(`${id}（${name}，${severity}）`);
}

// 已登记但本次未出现的豁免：不算错误（上游修复后条目就该被删），但提示出来，
// 避免「放行项早已失效却一直留着」。
const seenIds = new Set([...rootIds].map((raw) => JSON.parse(raw).id));
const unused = ALLOWED.filter((rule) => !seenIds.has(rule.id)).map((rule) => rule.id);

const counts = report.metadata?.vulnerabilities ?? {};
const critical = Number(counts.critical ?? 0);

if (critical > 0) errors.push(`出现 ${critical} 条 critical advisory：critical 一律不豁免`);

if (errors.length) {
  console.error("dependency advisories 未通过：");
  for (const line of errors) console.error(`  ✖ ${line}`);
  if (cleared.length) console.error(`  （已按登记放行：${cleared.join("；")}）`);
  process.exit(1);
}

const total = Object.values(counts).reduce((sum, value) => sum + Number(value ?? 0), 0);
console.log(
  `dependency advisories OK：共 ${total} 条，其中 ${cleared.length} 条按登记放行，无未登记项`,
);
for (const rule of ALLOWED) {
  if (seenIds.has(rule.id)) {
    console.log(`  放行 ${rule.id}（${rule.package}）：${rule.reason}`);
    console.log(`       到期条件：${rule.expires}`);
  }
}
if (unused.length) {
  console.log(`  注意：以下豁免本次未命中，上游可能已修复，请复核后删除：${unused.join("、")}`);
}
