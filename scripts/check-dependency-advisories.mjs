// 依赖 advisory 门禁：对**每个** npm 工程跑一次 `npm audit`，只放行**已登记**的 advisory，其余一律阻塞。
//
// 为什么需要它而不是直接跑 `npm audit`：`npm audit` 没有 ignore 机制（`npm audit --help`
// 里没有这个选项），而仓库当前有**上游无修复**的 advisory——`braces` 的
// GHSA-vfj7-8cjw-p6xm（栈耗尽 DoS）在 npm 上的受影响范围是 `*`，3.x 线最新版 3.0.3
// 就是被通报的版本，`fixAvailable` 为 false。直接跑 audit 会让 `deps` job 永远红灯，
// 于是每次合并都得走管理员 bypass —— 那会让「绕过门禁」变成习惯，是比这条 DoS 更大的风险。
//
// 判定面 = 下面的 `PROJECTS`：仓库根与 `clients/app`。后者是**独立 npm 工程**
// （自带 package.json/package-lock.json），根目录的 `npm ci` / `npm run check` 都不覆盖它；
// 扩判定范围**不需要** `npm ci`，因为 `npm audit` 只读锁文件（按 cwd 取当地 package-lock.json）。
//
// 豁免的**边界**（刻意收窄，不是「关掉 npm audit」）：
// - 只按 **advisory ID** 放行，不按包名、不按 severity：新增任意一条未登记的 advisory
//   就会失败，即使它落在同一个包上。
// - 放行项必须**同时**匹配 advisory ID 与**适用工程**：同号 advisory 出现在另一个工程时仍需单独登记，
//   因为它在不同工程里可达性可以完全不同（本文件里 `braces` 就是这个情况：两个工程各登记一条）。
// - 只放行 `high` 及以下；出现 `critical` 一律失败（当前豁免项是 moderate/high）。
// - 放行项必须在下面的 `ALLOWED` 里逐条登记，含理由与**到期条件**；登记不全即失败。
// - 传播包（`via` 指向另一个包名而非 advisory 对象）不单独判定：只要根因 advisory 被放行，
//   由它传播出来的条目一并放行；反之只要有**任一**根因未放行，整体失败。
//
// 到期条件：每条豁免各自的 `expires` 字段；上游发布修复或换掉相关依赖链时应删除对应条目。

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * 判定覆盖的 npm 工程。`path` 相对仓库根、用 POSIX 斜杠，它同时是 `npm audit` 的 `cwd`
 * 和失败输出里的定位名（不带路径就无法区分「根的问题」与「前端的问题」）。
 */
const PROJECTS = [
  { path: "." },
  { path: "clients/app" },
];

/**
 * 已登记放行的 advisory。
 *
 * `id` 用 GitHub advisory 的 GHSA 号（`npm audit --json` 的 `via[].url` 末段）。
 * `project` 是**必填**的适用工程（见上方 `PROJECTS` 的 `path`）；判定按 (id, project) 双匹配，
 * 它不可省略——省略即对任何工程都不匹配（会被下面的登记自检判为配置错误），
 * 不允许「不写就等于对所有工程生效」的隐式放行。
 */
const ALLOWED = [
  {
    id: "GHSA-vfj7-8cjw-p6xm",
    project: ".",
    package: "braces",
    severity: "high",
    reason:
      "上游无修复：受影响范围是 `*`，braces 3.x 线最新版（3.0.3）就是被通报的版本，npm audit 报 fixAvailable=false。本工程不可达：它只在 devDependencies 里（仓库根 package.json 的 dependencies 为空），是本地合同校验与提交钩子的工具链，不随产品分发；调用方是 @fission-ai/openspec 经 fast-glob → micromatch 扫描本地 openspec/**，传给 braces 的 glob 模式是仓库自己的常量，不由外部输入构造，该栈耗尽 DoS 所需的攻击者可控嵌套模式在本仓库不成立。",
    expires:
      "上游发布含修复的 braces（受影响范围不再为 `*`），或 @fission-ai/openspec 换掉 fast-glob/micromatch 链时删除本条。",
  },
  {
    id: "GHSA-vfj7-8cjw-p6xm",
    project: "clients/app",
    package: "braces",
    severity: "high",
    reason:
      "上游无修复：与根工程同一条 advisory（受影响范围 `*`，3.x 线最新版 3.0.3 即被通报版本，fixAvailable=false）。本工程不可达：它经 expo → @expo/metro → metro-file-map → micromatch 进入**构建期**工具链，glob 模式来自 Expo 自身配置而非外部输入，且不进 web 产物（已导出产物的字符级核对中 micromatch 0 命中）。",
    expires:
      "上游发布含修复的 braces（受影响范围不再为 `*`），或本工程的 metro-file-map 链换掉 micromatch 时删除本条。",
  },
  {
    id: "GHSA-86w9-cpqp-85rv",
    project: "clients/app",
    package: "node-forge",
    severity: "high",
    reason:
      "上游无修复（无已发布的修复版）。本工程不可达：它位于 @expo/code-signing-certificates，属 Expo Updates 的代码签名路径，而本项目只做 `expo export --platform web`、不产出 Updates 签名包，且不进 web 产物（已导出产物的字符级核对中 node-forge 0 命中）。",
    expires:
      "上游为 node-forge 发布修复版，或 Expo 换掉 @expo/code-signing-certificates 链时删除本条。",
  },
  {
    id: "GHSA-w5hq-g745-h8pq",
    project: "clients/app",
    package: "uuid",
    severity: "moderate",
    reason:
      "漏洞前提在本仓库不成立：该 advisory 只影响 v3/v5/v6 在传入 `buf` 时的缺失边界检查，而本工程里唯一的调用方 xcode 只调用 uuid.v4() 且不传 `buf`，不触碰 v3/v5/v6；且它属构建期工具链，不进 web 产物（产物里的 uuid 调用经核对都是 Expo 自身的 helper）。",
    expires:
      "上游 xcode 升到含修复的 uuid（>= 11.1.1）时删除本条。",
  },
  {
    id: "GHSA-vcc3-ghjq-m6fr",
    project: "clients/app",
    package: "decode-uri-component",
    severity: "moderate",
    reason:
      "**真实可达、本次不修**（与上面三条的「不可达」性质不同，不得混为一谈）：它在 web 产物里，路径是 expo-router 的 parseQueryParams → query-string.parse → decode，打开带畸形百分号编码 query 的链接即可触发指数级解码 DoS。本次不修的理由是修复代价高于收益：唯一修复版 0.5.0 是 ESM、与 query-string@7 的 CJS require() 不兼容，强压会直接崩溃；有效路径只有升到 Expo SDK 58（expo-router@58 移除 query-string），而 SDK 58 当前是 npm `next` 标签、期望 react-native 0.88.0-rc.4（RC），且实测其依赖树里 braces/node-forge/uuid 原样保留——付出跨 SDK major 的代价也只清掉 4 条里的一条。",
    expires:
      "Expo SDK 58 进入 npm `latest` 时升级 expo-router 并删除本条（当前 `latest` 仍是 SDK 57，本条到期条件未满足）。",
  },
];

const severityRank = { info: 0, low: 1, moderate: 2, high: 3, critical: 4 };

// 判定键是 (适用工程, advisory ID) 二元组，不是单独的 advisory ID。
const allowKey = (project, id) => `${project}\u0000${id}`;
const allowedByKey = new Map(ALLOWED.map((entry) => [allowKey(entry.project, entry.id), entry]));

function auditProject(project) {
  const cwd = resolve(ROOT, project.path);
  // 失败信息必须带工程路径：`npm audit` 按 cwd 读当地锁文件，不带路径就分不清是谁出的问题。
  const undecided = (detail) =>
    new Error(`${project.path}：npm audit 未能产出可判定结果（${detail}）`);
  // Windows 上 `.cmd` 不能直接 spawn（Node 对 .cmd/.bat 的保护会抛 EINVAL），必须经 shell；
  // 因此那里传整条命令行字符串而不是 (command, args)——与 `scripts/pre-commit.mjs` 同一处理。
  const npm = process.platform === "win32" ? "npm.cmd" : "npm";
  const result =
    process.platform === "win32"
      ? spawnSync([npm, "audit", "--json"].join(" "), {
          cwd,
          encoding: "utf8",
          windowsHide: true,
          maxBuffer: 64 * 1024 * 1024,
          shell: true,
        })
      : spawnSync(npm, ["audit", "--json"], {
          cwd,
          encoding: "utf8",
          windowsHide: true,
          maxBuffer: 64 * 1024 * 1024,
        });
  // `npm audit` 在发现漏洞时以非零码退出，但 stdout 仍是完整 JSON。
  if (typeof result.stdout !== "string" || !result.stdout.trim().startsWith("{")) {
    throw undecided(result.error?.message ?? `exit ${result.status}`);
  }
  const report = JSON.parse(result.stdout);
  // 审计本身失败（例如锁文件缺失的 ENOLOCK）也会把错误对象以 JSON 回在 stdout；
  // 不识别它就会被当成「零漏洞」静默通过——那正是本门禁要防的「看不见的缺口」。
  if (report.error) {
    const code = report.error.code ?? "unknown";
    const summary = report.error.summary ?? "";
    throw undecided(summary ? `${code}：${summary}` : code);
  }
  return report;
}

const errors = [];

// 登记自检：`project` 是必填字段，缺失或写了不存在的工程时该条目对任何工程都不匹配
// （等于静默失效）。这里直接报配置错误，而不是把它降级成「对所有工程生效」。
for (const rule of ALLOWED) {
  if (typeof rule.project !== "string" || rule.project.length === 0) {
    errors.push(`豁免登记 ${rule.id} 缺少「适用工程」字段——登记项必须声明它覆盖哪个工程`);
  } else if (!PROJECTS.some((project) => project.path === rule.project)) {
    errors.push(
      `豁免登记 ${rule.id} 的适用工程 ${rule.project} 不在审计范围内（${PROJECTS.map((project) => project.path).join("、")}）`,
    );
  }
}

const cleared = [];
const seenKeys = new Set();
const audited = [];

for (const project of PROJECTS) {
  let report;
  try {
    report = auditProject(project);
  } catch (error) {
    // 审计本身失败（锁文件缺失、npm 不可用等）不能当作「零漏洞」——那正是本门禁要防的
    // 「看不见的缺口」。错误信息已带工程路径（见 `auditProject`），这里收集后统一输出。
    errors.push(String(error?.message ?? error));
    continue;
  }
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

  for (const raw of rootIds) {
    const { id, package: name, severity, title } = JSON.parse(raw);
    const rule = allowedByKey.get(allowKey(project.path, id));
    if (!rule) {
      errors.push(
        `${project.path}：未登记的 advisory：${id}（${name}，${severity}）${title ? ` — ${title}` : ""}`,
      );
      continue;
    }
    if (rule.package !== name) {
      errors.push(
        `${project.path}：豁免 ${id} 登记的包是 ${rule.package}，实际出现在 ${name}——请复核豁免是否仍然对得上`,
      );
      continue;
    }
    if (severityRank[severity] > severityRank[rule.severity]) {
      errors.push(
        `${project.path}：豁免 ${id} 登记的严重度是 ${rule.severity}，实际为 ${severity}——请复核`,
      );
      continue;
    }
    seenKeys.add(allowKey(project.path, id));
    cleared.push(`${id}（${name}，${severity}，${project.path}）`);
  }

  const counts = report.metadata?.vulnerabilities ?? {};
  const critical = Number(counts.critical ?? 0);
  if (critical > 0) {
    errors.push(`${project.path}：出现 ${critical} 条 critical advisory——critical 一律不豁免`);
  }
  audited.push({
    project: project.path,
    total: Object.values(counts).reduce((sum, value) => sum + Number(value ?? 0), 0),
  });
}

// 已登记但本次未命中的豁免：不算错误（上游修复后条目就该被删），但提示出来，
// 避免「放行项早已失效却一直留着」。匹配同样按 (advisory ID, 工程)——
// 某个工程的同号 advisory 被判过，不代表另一个工程的也被判过。
const unused = ALLOWED.filter((rule) => !seenKeys.has(allowKey(rule.project, rule.id))).map(
  (rule) => `${rule.id}@${rule.project}`,
);

if (errors.length) {
  console.error(`dependency advisories 未通过（已审计：${PROJECTS.map((p) => p.path).join("、")}）：`);
  for (const line of errors) console.error(`  ✖ ${line}`);
  if (cleared.length) console.error(`  （已按登记放行：${cleared.join("；")}）`);
  process.exit(1);
}

const total = audited.reduce((sum, entry) => sum + entry.total, 0);
console.log(
  `dependency advisories OK：审计 ${audited.length} 个 npm 工程，共 ${total} 条，其中 ${cleared.length} 条按登记放行，无未登记项`,
);
for (const { project, total: projectTotal } of audited) {
  console.log(`  工程 ${project}：${projectTotal} 条`);
}
for (const rule of ALLOWED) {
  if (seenKeys.has(allowKey(rule.project, rule.id))) {
    console.log(`  放行 ${rule.id}（${rule.package}，适用工程 ${rule.project}）：${rule.reason}`);
    console.log(`       到期条件：${rule.expires}`);
  }
}
if (unused.length) {
  console.log(`  注意：以下豁免本次未命中，上游可能已修复，请复核后删除：${unused.join("、")}`);
}
