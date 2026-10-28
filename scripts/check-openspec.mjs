// OpenSpec 资产门禁：变更与规范的 `--strict` 校验，以及项目本地 workflow schema 的结构校验。
//
// 为什么需要它（而不是靠人记得跑）：
//   `openspec/` 是本仓库的需求权威入口（`npm run check:openspec` 之前的空档期里，
//   `openspec/specs/**` 与 `openspec/changes/**` 完全没有机器判定——写错的 requirement 标题、
//   少于四个井号的 scenario、缺失的 capability 路径都只会在归档时才暴露）。本脚本把
//   "变更与规范仍然自洽" 变成每次 `npm run check` 都跑的判据。
//
// 为什么必须显式设遥测环境变量：
//   `@fission-ai/openspec` 默认**开启**遥测与 npm 更新检查（opt-out 模型），只在检测到 CI 环境变量时
//   自动关闭。`AGENTS.md` §3 要求不引入未授权的遥测或云依赖，因此这里显式 opt-out，
//   让门禁在开发机与 CI 上都是离线语义。
//
// 判据边界（写清楚以免被当成更强的保证）：
//   通过本脚本只等于"资产结构合法"——不等于需求描述正确、不等于 specs 与代码一致。
//   代码与合同的绑定由其余九道门禁负责。
//
// 六项检查：
//   1) 长期规范全量strict，已有specs或进入tests/plan/tasks的变更逐个strict；
//   2) 项目本地 schema 的结构合法（artifact 依赖、apply.requires、模板引用）；
//   3) config.yaml 的 `schema:` 指向真实可解析的 schema；
//   4) fork 出来的 schema 里「照搬引擎」的模板与上游逐字节一致（分歧只允许在自写模板里）；
//   5) rules / operations.guidance 的条目不会被 YAML 静默丢弃。
//   6) 独立角色/术语、活动变更的proposal必备节、plan结构（必备节、工作包表、
//      Main E2E一致性、工作包被tasks引用）与tests结构（spec文件引用存在性；场景
//      名称与覆盖仍由reviewer复核）、长期tests结构，以及apply对tests的依赖。

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { validateWorkflowAssets } from "./openspec-test-cases.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const engine = join(root, "node_modules", "@fission-ai", "openspec", "bin", "openspec.js");

// 引擎的遥测与更新检查以这三个变量为硬开关（env 优先级高于全局配置）。
const env = {
  ...process.env,
  OPENSPEC_TELEMETRY: "0",
  OPENSPEC_NO_UPDATE_CHECK: "1",
  DO_NOT_TRACK: "1",
};

const errors = [];

if (!existsSync(engine)) {
  console.error("OpenSpec 门禁：缺少 openspec CLI，请先在仓库根运行 npm ci。");
  process.exit(1);
}

function run(label, args) {
  const result = spawnSync(process.execPath, [engine, ...args], {
    cwd: root,
    encoding: "utf8",
    env,
  });
  if (result.error) {
    errors.push(`${label}：启动失败：${result.error.message}`);
    return null;
  }
  if (result.status !== 0) {
    const detail = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim();
    errors.push(`${label}：退出码 ${result.status}\n${detail}`);
    return null;
  }
  return `${result.stdout ?? ""}`;
}

// 1) 规范始终strict；只有空/proposal/design规划前缀可暂缺delta。
// 任何specs路径（即使目录为空/损坏）、tests及后续产物均不能借此绕过strict。
run("openspec validate --specs --strict", ["validate", "--specs", "--strict"]);
const changesRoot = join(root, "openspec", "changes");
if (existsSync(changesRoot)) {
  for (const entry of readdirSync(changesRoot, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    if (!entry.isDirectory() || entry.name === "archive" || entry.name.startsWith(".")) continue;
    const changeDir = join(changesRoot, entry.name);
    const metadata = join(changeDir, ".openspec.yaml");
    const skipsSpecs = existsSync(metadata) && /^skip_specs:\s*true\b/m.test(readFileSync(metadata, "utf8"));
    if (skipsSpecs || ["specs", "tests.md", "plan.md", "tasks.md"].some((path) => existsSync(join(changeDir, path)))) {
      run(`openspec validate ${entry.name} --type change --strict`, ["validate", entry.name, "--type", "change", "--strict"]);
    }
  }
}

// 2) 项目本地 workflow schema 的结构校验（artifact 依赖、apply.requires、模板引用）。
//    逐目录调用：`schema validate` 不接受 `--all`，只接受单个 schema 名。
const schemaRoot = join(root, "openspec", "schemas");
if (existsSync(schemaRoot)) {
  const names = readdirSync(schemaRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(join(schemaRoot, entry.name, "schema.yaml")))
    .map((entry) => entry.name)
    .sort();
  for (const name of names) {
    run(`openspec schema validate ${name}`, ["schema", "validate", name]);
  }
  if (names.length === 0) {
    errors.push("openspec/schemas/ 存在但其中没有任何 schema.yaml");
  }
}

// 3) `openspec/config.yaml` 的 `schema` 必须指向一个可解析的 schema。
//    引擎对未知 schema 只在具体 command 上失败，这样一处笔误会让所有 `/opsx-*` 命令坏掉，
//    因此在门禁里提前判定：项目本地优先，其次包内置。
const configPath = join(root, "openspec", "config.yaml");
if (existsSync(configPath)) {
  // 不引第三方 YAML 解析：只取顶层 `schema:` 标量，避免为一行读取引入依赖。
  const match = readFileSync(configPath, "utf8").match(/^schema:[ \t]*(\S+)[ \t]*$/m);
  if (!match) {
    errors.push("openspec/config.yaml 缺少顶层 `schema:` 键");
  } else {
    const declared = match[1];
    const resolved = [
      join(schemaRoot, declared, "schema.yaml"),
      join(root, "node_modules", "@fission-ai", "openspec", "schemas", declared, "schema.yaml"),
    ].some((candidate) => existsSync(candidate));
    if (!resolved) {
      errors.push(
        `openspec/config.yaml 的 schema '${declared}' 在项目本地与引擎内置里都不存在`
          + "（新增 schema 时用 openspec schema fork，并把它提交入库）",
      );
    }
  }
}

// 4) fork 出来的 schema，其「照搬引擎」的模板必须与上游逐字节一致。
//
//    背景：`openspec schema fork` 会把源 schema 的 templates/ 整个复制过来，而引擎的
//    `template` 字段是必填、且拒绝指向 schema 目录之外（`relativePathSchema` 禁止 `..` 与绝对路径），
//    所以 fork 必须自带模板——没有「复用引擎版」这条路。
//
//    本仓库的策略是：只让新增 artifact 用自写模板（plan/tests），其余保持与上游逐字节相同，
//    分歧面归零、升级时只需 `cp`。这个策略必须被机器判定，否则升级 `@fission-ai/openspec` 后
//    上游模板若变化，本地的副本会安静地过期而没人发现。
//
//    判定方式：拿本仓库 schema 的**源 schema 名**（从 fork 记录不出来，因此对照同名的引擎内置 schema）。
//    声明在下面这张表里，避免猜：键是本地 schema 名 → 值是要对照的上游 schema 名。
//    同名覆写（本地也叫 spec-driven）也是这张表的合法输入：基准始终是**引擎内置目录**。
const FORKED_FROM = { "spec-driven": "spec-driven" };

/** 本仓库在覆写 schema 里自写的 artifact（引擎内置版没有的）。新增时同步此表。 */
const AUTHORED_ARTIFACTS = ["plan", "tests"];
if (existsSync(schemaRoot)) {
  for (const [local, upstream] of Object.entries(FORKED_FROM)) {
    const localDir = join(schemaRoot, local);
    const localTemplates = join(localDir, "templates");
    const upstreamDir = join(
      root, "node_modules", "@fission-ai", "openspec", "schemas", upstream,
    );
    const upstreamTemplates = join(upstreamDir, "templates");
    if (!existsSync(localTemplates) || !existsSync(upstreamTemplates)) continue;

    // 4a) 自写 artifact 必须仍然被声明。
    //
    //     为什么单独判这一条：同名覆写下最容易犯的错是把内置 schema 原样 `cp -r` 回来，
    //     那次覆盖**不会报任何错**，但会静默抹掉自写 artifact（本仓库的 plan），
    //     让它从 schema 里消失、连同 apply 门禁一起退回内置行为。
    //     下面 4b 的模板比对抓不到它（被删的是 schema.yaml 里的条目，不是模板文件）。
    if (existsSync(join(upstreamDir, "schema.yaml")) && existsSync(join(localDir, "schema.yaml"))) {
      const declaredArtifacts = (text) => new Set(
        [...text.matchAll(/^\s*-\s*id:\s*([A-Za-z0-9_-]+)\s*$/gm)].map((m) => m[1]),
      );
      const upstreamIds = declaredArtifacts(readFileSync(join(upstreamDir, "schema.yaml"), "utf8"));
      const localIds = declaredArtifacts(readFileSync(join(localDir, "schema.yaml"), "utf8"));
      const authored = [...upstreamIds].length > 0
        ? [...localIds].filter((id) => !upstreamIds.has(id))
        : [];
      for (const id of AUTHORED_ARTIFACTS) {
        if (!localIds.has(id)) {
          errors.push(
            `${local}/schema.yaml 缺少本仓库自写的 artifact '${id}'。`
              + "同名覆写时最容易犯的错是从引擎内置目录 `cp -r` 整个 schema 回来——"
              + "那次覆盖不会报错，但会静默抹掉自写 artifact 与依赖它的 apply 门禁。"
              + "请从备份或 git 历史恢复 schema.yaml。",
          );
        }
      }
      for (const id of authored) {
        if (!AUTHORED_ARTIFACTS.includes(id)) {
          errors.push(
            `${local}/schema.yaml 声明了引擎内置版没有的 artifact '${id}'，但它不在本脚本的`
              + " AUTHORED_ARTIFACTS 表里。新增自写 artifact 时请同步该表（并补模板与文档）。",
          );
        }
      }
    }

    // 4b) 「照搬」的模板必须与上游逐字节一致。
    for (const entry of readdirSync(localTemplates, { withFileTypes: true })) {
      if (!entry.isFile()) continue;
      const upstreamPath = join(upstreamTemplates, entry.name);
      // 上游没有这个文件 = 本仓库新增的 artifact（如 plan.md），允许自由定制。
      if (!existsSync(upstreamPath)) continue;
      const same = readFileSync(join(localTemplates, entry.name)).equals(readFileSync(upstreamPath));
      if (!same) {
        errors.push(
          `${local}/templates/${entry.name} 与引擎 ${upstream} 的同名模板不一致。`
            + "本仓库的策略是照搬模板保持零分歧、分歧只放在自写模板里。"
            + "若这次差异是有意的，请把该文件名从本脚本的对照表里排除并说明理由；"
            + "若升级引擎导致上游模板变化，请用 `cp` 重新同步。",
        );
      }
    }
  }
}

// 5) rules 与 operations.guidance 的每条必须是字符串。
//    这两处用裸标量写含有 ": " 的句子时 YAML 会把它解析成映射，引擎只打印一行 warning
//    并**静默丢弃整条规则/指引**——门禁里显式判定，避免规则悄悄失效。
if (existsSync(configPath)) {
  const raw = readFileSync(configPath, "utf8");
  for (const section of ["rules", "operations"]) {
    if (!new RegExp(`^${section}:`, "m").test(raw)) continue;
    const parsed = parseSimpleSection(raw, section);
    if (parsed === null) {
      errors.push(`openspec/config.yaml 的 \`${section}:\` 段落无法解析`);
      continue;
    }
    for (const [owner, entries] of parsed) {
      entries.forEach((text, index) => {
        // 引号包裹的标量由 YAML 解析为字符串，不会退化成映射。
        if (/^["']/.test(text)) return;
        // 两种情况都会把这一条变成映射、被引擎静默丢弃整条规则：
        //   `- 文本: 值`  → { "文本": "值" }
        //   `- 文本:`     → { "文本": null }   （裸尾冒号，同样解析成映射）
        const becomesMapping = text.includes(": ") || text.endsWith(":");
        if (becomesMapping) {
          errors.push(
            `openspec/config.yaml 的 ${section}.${owner}[${index}] 会被 YAML 解析成映射`
              + "并被引擎静默丢弃（含 \": \" 或以 \":\" 结尾都会）；请改写或用引号包裹："
              + `${text.slice(0, 40)}…`,
          );
        }
      });
    }
  }
}

/**
 * 极简解析：只识别本仓库 config.yaml 用到的嵌套列表结构
 * （`rules.<artifact>: [- 文本]` 与 `operations.<op>.guidance: [- 文本]`）。
 * 返回 `Map<ownerPath, string[]>`；解析不出任何条目时返回 null。
 */
function parseSimpleSection(text, section) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trimEnd() === `${section}:`);
  if (start < 0) return null;
  const result = new Map();
  /** 当前层级的键路径，例如 ["operations", "apply", "guidance"]。 */
  const keyPath = [];
  for (let i = start + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.trim() === "" || line.trimStart().startsWith("#")) continue;
    if (!line.startsWith(" ")) break;
    const indent = line.length - line.trimStart().length;
    const body = line.trimStart();
    if (body.startsWith("- ")) {
      const owner = keyPath.join(".");
      if (!owner) return null;
      if (!result.has(owner)) result.set(owner, []);
      result.get(owner).push(body.slice(2));
      continue;
    }
    // 键行：按缩进回退到它所属的层级，再记录键名。
    // 段落下的第一层键缩进为 2（depth 1），它的父级是段落本身，所以要再减一层。
    const keyMatch = body.match(/^([A-Za-z0-9_-]+):[ \t]*$/);
    if (keyMatch) {
      const depth = Math.floor(indent / 2) - 1;
      keyPath.length = Math.max(0, depth);
      keyPath.push(keyMatch[1]);
      continue;
    }
    // 内联值或无法识别的形态：跳过，不在这里判定。
  }
  return result.size > 0 ? result : null;
}

// 6) 活动变更和长期用例的固定结构；不回写或追溯要求历史归档。
errors.push(...validateWorkflowAssets(root));
// 产物存在不代表apply仍然依赖它：防止升级schema时门禁被静默卸掉。
const workflowPath = join(schemaRoot, "spec-driven", "schema.yaml");
if (existsSync(workflowPath)) {
  const workflow = readFileSync(workflowPath, "utf8");
  const apply = workflow.match(/^apply:\s*\r?\n\s+requires:\s*\[([^\]]+)\]/m);
  if (!apply || !apply[1].split(",").map((id) => id.trim()).includes("tests")) errors.push("spec-driven apply.requires 必须包含 tests");
}

if (errors.length > 0) {
  console.error(`openspec check failed with ${errors.length} problem(s):`);
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}
console.log("openspec OK: 规范/schema/config与活动及长期测试用例结构合法（不代表验收已执行）");
