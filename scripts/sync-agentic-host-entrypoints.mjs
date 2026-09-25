import { mkdir, readFile, writeFile } from "node:fs/promises";

const options = process.argv.slice(2);
const write = options.includes("--write");
if (options.some((arg) => arg !== "--write" && arg !== "--check")) {
  console.error("用法：node scripts/sync-agentic-host-entrypoints.mjs [--check|--write]");
  process.exit(2);
}

const root = new URL("../", import.meta.url);
const routes = [
  {
    anchor: "Implement tasks from an OpenSpec change.",
    instruction: "**本项目的 agentic 入口：** `openspec status` 返回 `schemaName: agentic` 时，先读取仓库 `AGENTS.md`、`openspec/config.yaml` 与 agentic schema 的 apply 指令，按其中的计划、候选合入检查、E2E、角色交接和最终验收要求执行。报告可归档前须运行 `.agents/skills/agentic-verify/SKILL.md`；`all_done` 只表示任务复选框完成。下方通用步骤仅在不与上述规则冲突时适用。",
    files: [
      ".agents/skills/openspec-apply-change/SKILL.md",
      ".pi/prompts/opsx-apply.md",
      ".pi/skills/openspec-apply-change/SKILL.md",
      ".omp/commands/opsx-apply.md",
      ".omp/skills/openspec-apply-change/SKILL.md",
    ],
  },
  {
    anchor: "Verify that an implementation matches the change artifacts (specs, tasks, design).",
    instruction: "**本项目的 agentic 入口：** `openspec status` 返回 `schemaName: agentic` 时，先完整读取并执行 `.agents/skills/agentic-verify/SKILL.md` 及其引用的验收程序。CLI 状态与证据验收的 PASS / FAIL / BLOCKED 分别报告；下方通用评分不能作为该变更可归档的结论。",
    files: [
      ".pi/prompts/opsx-verify.md",
      ".omp/commands/opsx-verify.md",
      ".omp/skills/openspec-verify-change/SKILL.md",
    ],
  },
  {
    anchor: "Archive a completed change in the experimental workflow.",
    instruction: "**本项目的 agentic 入口：** `openspec status` 返回 `schemaName: agentic` 时，先完整读取 `.agents/skills/agentic-verify/SKILL.md` 并执行归档前检查；对选定规划根运行 `npx --quiet --no-install openspec-agentic workflow check --change <name> --stage archive --json`。结构检查和当前版本的语义验收均为 PASS 后才能归档，下方通用确认步骤不能豁免这两道检查。",
    files: [
      ".agents/skills/openspec-archive-change/SKILL.md",
      ".pi/prompts/opsx-archive.md",
      ".pi/skills/openspec-archive-change/SKILL.md",
      ".omp/commands/opsx-archive.md",
      ".omp/skills/openspec-archive-change/SKILL.md",
    ],
  },
];

let stale = 0;
let blocked = 0;
for (const route of routes) {
  for (const file of route.files) {
    const path = new URL(file, root);
    let source;
    try {
      source = await readFile(path, "utf8");
    } catch (error) {
      console.error(`${file}: 无法读取：${error.message}`);
      stale += 1;
      blocked += 1;
      continue;
    }
    const eol = source.includes("\r\n") ? "\r\n" : "\n";
    const lines = source.split(/\r?\n/);
    const anchor = lines.indexOf(route.anchor);
    if (anchor < 0) {
      console.error(`${file}: 缺少 OpenSpec 原始入口`);
      stale += 1;
      blocked += 1;
      continue;
    }
    const current = lines.findIndex((line) => line.startsWith("**本项目的 agentic 入口：**"));
    if (current >= 0 && lines[current] === route.instruction && lines[current - 1] === "" && lines[current + 1] === "") continue;
    stale += 1;
    if (!write) {
      console.error(`${file}: 缺少当前中文 agentic 路由；运行 npm run sync:agentic-hosts 修复`);
      continue;
    }
    if (current >= 0) lines.splice(current, 1);
    const position = lines.indexOf(route.anchor) + 1;
    if (lines[position] !== "") lines.splice(position, 0, "");
    lines.splice(position + 1, 0, route.instruction);
    if (lines[position + 2] !== "") lines.splice(position + 2, 0, "");
    await writeFile(path, lines.join(eol), "utf8");
    console.log(`${file}: 已同步`);
  }
}

for (const host of [
  { directory: ".pi", name: "Pi" },
  { directory: ".omp", name: "Oh My Pi" },
]) {
  const file = `${host.directory}/skills/agentic-verify/SKILL.md`;
  const path = new URL(file, root);
  const expected = `---\nname: agentic-verify\ndescription: 验收本项目 agentic 变更的最终实现和证据；用于最终验收、/opsx-verify 与归档前检查。\n---\n\n# 本项目 agentic 验收入口\n\n完整读取仓库根目录的 \`.agents/skills/agentic-verify/SKILL.md\`，按其中步骤执行。\n它是扩展维护的唯一验收规则；本文件只负责让 ${host.name} 发现该入口，不复制验收判据。\n`;
  const current = await readFile(path, "utf8").catch(() => null);
  if (current?.replaceAll("\r\n", "\n") === expected) continue;
  stale += 1;
  if (!write) {
    console.error(`${file}: 缺少当前中文 agentic 验收入口；运行 npm run sync:agentic-hosts 修复`);
    continue;
  }
  await mkdir(new URL(`${host.directory}/skills/agentic-verify/`, root), { recursive: true });
  await writeFile(path, expected, "utf8");
  console.log(`${file}: 已同步`);
}

if (blocked > 0 || (stale > 0 && !write)) process.exitCode = 1;
else console.log(`agentic 宿主入口${write ? "同步" : "检查"}完成：${routes.reduce((n, group) => n + group.files.length, 2)} 个文件`);
