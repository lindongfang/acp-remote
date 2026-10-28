// tests.md只登记用例；校验器不执行其中命令，也不证明独立验收已经完成。
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { parseDocument } from "yaml";

export const LEVELS = ["L1", "L2", "L3", "L4", "L5", "L6", "LX"];
const FIELDS = ["Level", "Requirement", "Executor", "Tool", "Preconditions", "Environment", "Steps", "Expected", "Cleanup", "Schedule", "Entry"];
const EXECUTORS = ["automation", "agent", "human"];
const SCHEDULES = ["acceptance", "pr", "daily", "weekly", "release", "on-demand"];
// Requirement 里形如 specs/<capability>/spec.md 的引用；openspec/ 前缀指长期能力。
const SPEC_REF_PATTERN = /(?:openspec\/)?specs\/[A-Za-z0-9][A-Za-z0-9/_-]*\/spec\.md/g;

function filled(value) {
  const text = value?.replace(/<!--[^]*?-->/g, "").trim();
  return Boolean(text) && !/^(?:TODO|TBD|待填写|替换为.*|<[^>]*>)$/i.test(text);
}

// 去掉说明性注释和围栏中的示例；保留行号，供用例与真实任务共同使用。
function markdownLines(text, preserveFences = false) {
  const source = text.replace(/\r\n?/g, "\n").replace(/^\uFEFF/, "");
  const lines = [];
  let fence = null;
  let inComment = false;
  for (const line of source.split("\n")) {
    if (fence) {
      const marker = /^\s*(`{3,}|~{3,})/.exec(line);
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length && !line.slice(marker[0].length).trim()) fence = null;
      lines.push(preserveFences ? line : "");
      continue;
    }
    let rest = line;
    let visible = "";
    while (rest) {
      const boundary = rest.indexOf(inComment ? "-->" : "<!--");
      if (boundary < 0) { if (!inComment) visible += rest; break; }
      if (!inComment) visible += rest.slice(0, boundary);
      rest = rest.slice(boundary + (inComment ? 3 : 4));
      inComment = !inComment;
    }
    const marker = /^\s*(`{3,}|~{3,})/.exec(visible);
    if (marker) {
      fence = marker[1];
      lines.push(preserveFences ? visible : "");
    } else lines.push(visible);
  }
  return { lines, unclosedFence: Boolean(fence) };
}

/**
 * Fixed Markdown format; returned diagnostics include file/case/field.
 * specRoot 是长期 openspec 根，changeRoot 是活动变更根。裸 specs 路径只对当前
 * changeRoot（suite无changeRoot时对specRoot），显式openspec/specs只对specRoot。
 * requireSpecRef只要求L5存在文件引用；场景名称和覆盖仍由独立reviewer复核。
 */
export function validateTestDocument(text, label = "tests.md", options = {}) {
  const { specRoot, changeRoot } = options;
  const requireSpecRef = options.requireSpecRef ?? false;
  const errors = [];
  const fail = (message) => errors.push(`${label}: ${message}`);
  const { lines, unclosedFence } = markdownLines(text);
  if (unclosedFence) fail("未关闭的代码围栏");
  for (const title of ["Levels", "Cases"]) {
    if (lines.filter((line) => line === `## ${title}`).length !== 1) fail(`需要且仅允许一个 ## ${title}`);
  }
  const levelStart = lines.indexOf("## Levels");
  const caseStart = lines.indexOf("## Cases");
  if (levelStart >= caseStart) fail("Levels 必须先于 Cases");
  const applicability = new Map();
  const levelLines = lines.slice(levelStart + 1, caseStart);
  if (!levelLines.some((line) => /^\|\s*Level\s*\|\s*Applicability\s*\|\s*Reason\s*\|\s*$/.test(line))) fail("Levels 缺少固定表头");
  for (const line of levelLines) {
    if (!line.trim().startsWith("|")) continue;
    const cells = line.trim().split("|").slice(1, -1).map((cell) => cell.trim());
    if (cells[0] === "Level" || cells.every((cell) => /^:?-+:?$/.test(cell))) continue;
    const [level, mode, reason] = cells;
    if (cells.length !== 3 || !LEVELS.includes(level)) { fail(`非法层级行 ${line.trim()}`); continue; }
    if (applicability.has(level)) fail(`重复层级 ${level}`);
    applicability.set(level, mode);
    if (!["required", "not-applicable"].includes(mode)) fail(`${level} 非法 Applicability`);
    if (!filled(reason)) fail(`${level} 缺少 Reason`);
  }
  for (const level of LEVELS) if (!applicability.has(level)) fail(`缺少层级 ${level}`);

  const ids = new Set();
  const covered = new Set();
  const cases = [];
  let current;
  for (const line of lines.slice(caseStart + 1)) {
    const heading = /^###\s+(.+?)\s*$/.exec(line);
    if (heading) {
      const parsed = /^([A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+)\s+(.+)$/.exec(heading[1]);
      if (!parsed || !filled(parsed[2])) { fail(`非法用例ID或标题 ${heading[1]}`); current = null; continue; }
      const id = parsed[1];
      if (ids.has(id)) fail(`重复用例ID ${id}`);
      ids.add(id);
      current = { id, fields: new Map() };
      cases.push(current);
      continue;
    }
    const field = /^- ([A-Za-z]+):[ \t]*(.*)$/.exec(line);
    if (!field) continue;
    if (!current) { fail(`字段不属于合法用例 ${field[1]}`); continue; }
    if (!FIELDS.includes(field[1])) fail(`${current.id} 未知字段 ${field[1]}`);
    if (current.fields.has(field[1])) fail(`${current.id} 重复字段 ${field[1]}`);
    current.fields.set(field[1], field[2]);
  }
  if (cases.length === 0) fail("Cases 至少需要一条可执行用例");
  for (const { id, fields } of cases) {
    for (const field of FIELDS) if (!filled(fields.get(field))) fail(`${id} 缺少或空白字段 ${field}`);
    const level = fields.get("Level");
    if (!LEVELS.includes(level)) fail(`${id} 非法 Level`);
    else {
      covered.add(level);
      if (applicability.get(level) !== "required") fail(`${id} 对应 ${level} 必须为 required`);
    }
    if (!EXECUTORS.includes(fields.get("Executor"))) fail(`${id} 非法 Executor`);
    if (level === "LX" && fields.get("Executor") !== "human") fail(`${id} LX 必须由 human 执行`);
    const schedule = fields.get("Schedule")?.split(",").map((value) => value.trim()) ?? [];
    if (schedule.length === 0 || schedule.some((value) => !SCHEDULES.includes(value))) fail(`${id} 非法 Schedule`);
    const requirement = fields.get("Requirement") ?? "";
    const refs = [...requirement.matchAll(SPEC_REF_PATTERN)].map((match) => match[0]);
    for (const ref of refs) {
      const relative = ref.replace(/^openspec\//, "");
      const dir = ref.startsWith("openspec/") ? specRoot : (changeRoot ?? specRoot);
      const candidate = dir && join(dir, relative);
      const resolved = candidate && existsSync(candidate) && statSync(candidate).isFile();
      if (!resolved) fail(`${id} Requirement 引用的 ${ref} 不存在`);
    }
    if (requireSpecRef && fields.get("Level") === "L5" && refs.length === 0) {
      fail(`${id} L5 用例必须引用 spec 文件路径（specs/<capability>/spec.md）`);
    }
  }
  for (const [level, mode] of applicability) if (mode === "required" && !covered.has(level)) fail(`${level} required 但没有用例`);
  return errors;
}

/** proposal 固定必备节：标题存在且注释外正文非空。 */
function requireSection(text, title, label) {
  const heading = `## ${title}`;
  const match = text.match(new RegExp(`^${heading}\\s*$`, "m"));
  if (!match) return [`${label}: 缺少 ${heading} 节`];
  const rest = text.slice(text.indexOf(match[0]) + match[0].length);
  const next = rest.search(/^## /m);
  const body = (next >= 0 ? rest.slice(0, next) : rest).replace(/<!--[^]*?-->/g, "").trim();
  if (!body) return [`${label}: ${heading} 节缺少正文`];
  return [];
}

const PLAN_BODY_SECTIONS = ["Execution Model", "Parallel Assessment", "Merge Order", "Project Verify", "Code Review", "Handoff"];
const PLAN_HEADING_SECTIONS = ["Work Packages", "Main E2E", "Findings", "Verification Results"];

/**
 * plan.md 最小结构门禁：必备节、非空正文、工作包表（7列、唯一ID、无空白列）、
 * Main E2E 的 yaml mode 一致性，以及每个工作包ID被 tasks.md 引用。
 * tasks.md 尚未产生时（plan 先于 tasks 合法）跳过引用核对，存在性由引擎 apply 闸门负责。
 */
export function validatePlanDocument(text, label = "plan.md", options = {}) {
  const errors = [];
  const fail = (message) => errors.push(`${label}: ${message}`);
  const { lines: planLines, unclosedFence } = markdownLines(text, true);
  if (unclosedFence) fail("plan.md 未关闭的代码围栏");
  const sections = new Map();
  let current = null;
  let sectionFence = null;
  for (const line of planLines) {
    const marker = /^\s*(`{3,}|~{3,})/.exec(line);
    if (marker) {
      if (!sectionFence) sectionFence = marker[1];
      else if (marker[1][0] === sectionFence[0] && marker[1].length >= sectionFence.length && !line.slice(marker[0].length).trim()) sectionFence = null;
    }
    const heading = !sectionFence && /^## (.+?)\s*$/.exec(line);
    if (heading) {
      current = heading[1];
      if (!sections.has(current)) sections.set(current, []);
      continue;
    }
    if (current) sections.get(current).push(line);
  }
  const bodyOf = (title) => (sections.get(title) ?? []).join("\n").trim();
  for (const title of [...PLAN_HEADING_SECTIONS, ...PLAN_BODY_SECTIONS]) {
    if (!sections.has(title)) fail(`缺少 ## ${title} 节`);
  }
  for (const title of PLAN_BODY_SECTIONS) {
    if (sections.has(title) && !bodyOf(title)) fail(`## ${title} 节缺少正文`);
  }
  if (sections.has("Work Packages")) {
    const tableLines = (sections.get("Work Packages") ?? []).filter((line) => line.trim().startsWith("|"));
    const cells = (row) => row.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((cell) => cell.trim());
    const headerIndex = tableLines.findIndex((row) => cells(row)[0] === "ID");
    if (headerIndex < 0) fail("Work Packages 缺少 ID 表头");
    const ids = new Set();
    const wpIds = [];
    for (const row of tableLines.slice(headerIndex + 1)) {
      const rowCells = cells(row);
      if (rowCells.length > 1 && rowCells.every((cell) => /^:?-+:?$/.test(cell))) continue;
      const id = rowCells[0];
      if (rowCells.length !== 7) { fail(`工作包 ${id || "(无ID)"} 必须 7 列`); continue; }
      if (!/^[A-Za-z][A-Za-z0-9_-]*$/.test(id)) { fail(`非法工作包ID ${id}`); continue; }
      if (ids.has(id)) { fail(`重复工作包ID ${id}`); continue; }
      ids.add(id);
      wpIds.push(id);
      if (rowCells.some((cell) => !cell)) fail(`工作包 ${id} 存在空白列`);
    }
    if (headerIndex >= 0 && wpIds.length === 0) fail("Work Packages 至少需要一个工作包");
    if (options.tasksText != null) {
      const { lines, unclosedFence } = markdownLines(options.tasksText);
      if (unclosedFence) fail("tasks.md 未关闭的代码围栏");
      // 与引擎正常任务的列表标记/单字符checkbox兼容；项目关联不计示例。
      const tasks = lines.flatMap((line) => {
        const match = /^\s*(?:[-*+]|\d{1,9}[.)])\s*\[(?:\s*([^\]\s]?)\s*\](?![([])|\s+\])\s*(.*)/.exec(line);
        return match ? [new Set(match[2].match(/[A-Za-z0-9_-]+/g) ?? [])] : [];
      });
      for (const id of wpIds) {
        if (!tasks.some((refs) => refs.has(id))) fail(`工作包 ${id} 未被 tasks.md 引用（须为真实任务中的完整ID）`);
      }
      tasks.forEach((refs, index) => {
        if (!wpIds.some((id) => refs.has(id))) fail(`tasks.md 第 ${index + 1} 个任务未引用已声明工作包`);
      });
    }
  }
  if (sections.has("Main E2E")) {
    const fences = [...bodyOf("Main E2E").matchAll(/^[ \t]*(`{3,}|~{3,})ya?ml[ \t]*\n([^]*?)^[ \t]*\1[ \t]*$/gm)];
    if (fences.length !== 1) fail("Main E2E 需要且仅允许一个 yaml 配置块");
    else {
      let values;
      try {
        const doc = parseDocument(fences[0][2]);
        if (doc.errors.length || doc.warnings.length) throw new Error("YAML语法、重复键或不支持的tag");
        values = doc.toJS({ maxAliasCount: 0 });
        if (!values || typeof values !== "object" || Array.isArray(values)) throw new Error("配置必须是映射");
      } catch {
        fail("Main E2E 非法 YAML 配置（不允许重复键或别名）");
        return errors;
      }
      const mode = values.mode;
      if (mode !== "required" && mode !== "not-applicable") fail("Main E2E mode 必须为 required 或 not-applicable");
      for (const field of ["reason", "basis", "alternative_checks"]) {
        const value = values[field];
        const validType = value == null || (field === "alternative_checks"
          ? Array.isArray(value) && value.every((item) => typeof item === "string" && item.trim().length > 0)
          : typeof value === "string");
        if (!validType) { fail(`Main E2E ${field} 类型错误`); continue; }
        const nonEmpty = field === "alternative_checks" ? Boolean(value?.length) : Boolean(value?.trim().length);
        if (mode === "not-applicable" && !nonEmpty) fail(`Main E2E ${field} 在 not-applicable 时必填`);
        if (mode === "required" && nonEmpty) fail(`Main E2E ${field} 在 required 时必须留空`);
      }
      for (const key of Object.keys(values)) {
        if (!["mode", "reason", "basis", "alternative_checks"].includes(key)) {
          fail(`Main E2E 未知字段 ${key}`);
        }
      }
    }
  }
  return errors;
}

/** Only active changes and durable suites; archive is immutable historical evidence. */
export function validateWorkflowAssets(root) {
  const errors = [];
  const required = ["openspec/GLOSSARY.md", "openspec/role/coder.md", "openspec/role/tester.md", "openspec/role/reviewer.md"];
  for (const relative of required) {
    const path = join(root, relative);
    if (!existsSync(path) || !statSync(path).isFile() || !filled(readFileSync(path, "utf8"))) errors.push(`${relative}: 缺少或空白的必需资产`);
  }
  for (const relative of ["openspec/adr", "openspec/testing/suites"]) {
    if (!existsSync(join(root, relative)) || !statSync(join(root, relative)).isDirectory()) errors.push(`${relative}: 缺少目录`);
  }
  const changes = join(root, "openspec/changes");
  if (existsSync(changes)) for (const entry of readdirSync(changes, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name === "archive" || entry.name.startsWith(".")) continue;
    const changeDir = join(changes, entry.name);
    const label = `openspec/changes/${entry.name}`;
    const has = (name) => existsSync(join(changeDir, name)) && statSync(join(changeDir, name)).isFile();
    // 允许保存规划前缀；已经出现的下游产物不能绕过其前置。
    const dependencies = {
      "design.md": ["proposal.md"],
      "tests.md": ["proposal.md", "design.md"],
      "plan.md": ["proposal.md", "design.md", "tests.md"],
      "tasks.md": ["proposal.md", "design.md", "tests.md", "plan.md"],
    };
    for (const [artifact, prerequisites] of Object.entries(dependencies)) {
      if (has(artifact)) for (const prerequisite of prerequisites) {
        if (!has(prerequisite)) errors.push(`${label}/${prerequisite}: ${artifact} 已存在，必须具备此前置产物`);
      }
    }
    const proposalPath = join(changeDir, "proposal.md");
    if (has("proposal.md")) {
      const proposal = readFileSync(proposalPath, "utf8");
      errors.push(...requireSection(proposal, "Exploration", `${label}/proposal.md`));
      errors.push(...requireSection(proposal, "Success Criteria", `${label}/proposal.md`));
    }
    const openspecYaml = join(changeDir, ".openspec.yaml");
    const skipSpecs = existsSync(openspecYaml)
      && /^skip_specs:\s*true\b/m.test(readFileSync(openspecYaml, "utf8"));
    const path = join(changeDir, "tests.md");
    if (has("tests.md")) errors.push(...validateTestDocument(readFileSync(path, "utf8"), `${label}/tests.md`, {
      changeRoot: changeDir, specRoot: join(root, "openspec"),
      requireSpecRef: !skipSpecs,
    }));
    const planPath = join(changeDir, "plan.md");
    if (existsSync(planPath) && statSync(planPath).isFile()) {
      const tasksPath = join(changeDir, "tasks.md");
      const tasksText = existsSync(tasksPath) && statSync(tasksPath).isFile() ? readFileSync(tasksPath, "utf8") : null;
      errors.push(...validatePlanDocument(readFileSync(planPath, "utf8"), `${label}/plan.md`, { tasksText }));
    }
  }
  const suites = join(root, "openspec/testing/suites");
  function scan(dir, prefix) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name), label = `${prefix}/${entry.name}`;
      if (entry.isDirectory()) scan(path, label);
      else if (entry.isFile() && entry.name.endsWith(".md")) {
        errors.push(...validateTestDocument(readFileSync(path, "utf8"), label, { specRoot: join(root, "openspec") }));
      }
    }
  }
  if (existsSync(suites) && statSync(suites).isDirectory()) scan(suites, "openspec/testing/suites");
  return errors;
}
