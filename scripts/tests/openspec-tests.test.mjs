import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, cpSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { LEVELS, validateTestDocument, validateWorkflowAssets } from "../openspec-test-cases.mjs";

const fields = {
  Level: "L2", Requirement: "proposal验收条目R1", Executor: "automation", Tool: "Node",
  Preconditions: "独立临时数据", Environment: "Node测试进程", Steps: "运行指定入口", Expected: "退出0且有断言",
  Cleanup: "删除本例临时目录", Schedule: "acceptance,pr", Entry: "node --test",
};
const block = (values = fields) => `### WF-001 检查入口\n\n${Object.entries(values).map(([key, value]) => `- ${key}: ${value}`).join("\n")}\n`;
const levels = `## Levels\n\n| Level | Applicability | Reason |\n| --- | --- | --- |\n${LEVELS.map((level) => `| ${level} | ${level === "L2" ? "required" : "not-applicable"} | 由本次范围决定 |`).join("\n")}\n\n`;
const valid = `# Tests\n\n${levels}## Cases\n\n${block()}`;
const proposal = "# Proposal\n\n## Why\n\n背景说明。\n\n## Success Criteria\n\n用户原始要求与成功判据。\n\n## Exploration\n\n决策分支已确认，无阻断未知。\n\n## Impact\n\n临时目录。\n";
const plan = [
  "# Plan", "",
  "## Execution Model", "", "主执行者统筹；tester/reviewer独立。", "",
  "## Parallel Assessment", "", "WP1无并行项。", "",
  "## Work Packages", "",
  "| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |",
  "| --- | --- | --- | --- | --- | --- | --- |",
  "| WP1 | 实现X | 方案检视 | coder | src/** | diff | npm run check |", "",
  "## Merge Order", "", "无重叠，按WP1顺序执行。", "",
  "## Project Verify", "", "npm run check；引用tests.md的L2用例。", "",
  "## Code Review", "", "独立reviewer检视方案与交付。", "",
  "## Main E2E", "", "```yaml", "mode: not-applicable", "reason: 工具链变更，无产品链路", "basis: 无用户可见行为", "alternative_checks: [L2]", "```", "",
  "## Handoff", "", "交接记录规则路径与快照。", "",
  "## Findings", "", "## Verification Results", "",
].join("\n");
const reject = (source, expected) => {
  const errors = validateTestDocument(source);
  assert.ok(errors.length > 0, "invalid fixture must fail independently");
  if (expected) assert.match(errors.join("\n"), expected);
};

test("合法文档、中文和LF/CRLF/BOM；示例注释不参与解析", () => {
  assert.deepEqual(validateTestDocument(valid), []);
  assert.deepEqual(validateTestDocument("\uFEFF" + valid.replaceAll("\n", "\r\n")), []);
  assert.deepEqual(validateTestDocument(valid + "\n<!-- ### WF-001 示例 -->\n```md\n### WF-001 示例\n```\n"), []);
});

for (const field of Object.keys(fields)) {
  test(`独立拒绝缺少 ${field}`, () => {
    assert.deepEqual(validateTestDocument(valid), []);
    const values = { ...fields }; delete values[field];
    reject(`# Tests\n${levels}## Cases\n${block(values)}`, new RegExp(field));
  });
  test(`独立拒绝空白/占位 ${field}`, () => {
    for (const empty of ["", "   ", "<!-- only comment -->", "TODO", "<填写>", "替换为实际内容"]) {
      reject(`# Tests\n${levels}## Cases\n${block({ ...fields, [field]: empty })}`, new RegExp(field));
    }
  });
}

test("逐项非法枚举、重复字段和人工边界", () => {
  for (const [field, value] of [["Level", "L7"], ["Executor", "robot"], ["Schedule", "monthly"]]) {
    reject(`# Tests\n${levels}## Cases\n${block({ ...fields, [field]: value })}`, new RegExp(field));
  }
  reject(valid + "- Tool: Node\n", /重复字段 Tool/);
  reject(valid.replace("- Tool:", "- Tools:"), /未知字段 Tools/);
  reject(valid.replace("| LX | not-applicable", "| LX | required").replace("- Level: L2", "- Level: LX"), /LX 必须由 human/);
});

test("逐项拒绝重复ID、非法/缺ID、无用例和错误层级表", () => {
  reject(valid + block(), /重复用例ID/);
  reject(valid.replace("WF-001", "invalid"), /非法用例ID/);
  reject(valid.replace("WF-001 ", ""), /非法用例ID/);
  reject(`# Tests\n${levels}## Cases\n`, /至少需要一条/);
  for (const level of LEVELS) reject(valid.replace(new RegExp(`^\\| ${level} \\|.*\\n`, "m"), ""), new RegExp(`缺少层级 ${level}`));
  reject(valid.replace("## Cases", "| L1 | not-applicable | 不适用 |\n\n## Cases"), /重复层级 L1/);
  reject(valid.replace("| L1 | not-applicable", "| L1 | optional"), /Applicability/);
  reject(valid.replace("| L1 | not-applicable | 由本次范围决定 |", "| L1 | not-applicable |  |"), /Reason/);
  reject(valid.replace("| L3 | not-applicable", "| L3 | required"), /L3 required 但没有用例/);
  reject(valid.replace("| L2 | required", "| L2 | not-applicable"), /L2 必须为 required/);
  reject(valid.replace("## Levels", "## Level"), /Levels/);
  reject(valid.replace("## Cases", "## Case"), /Cases/);
});

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "acpr-workflow-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const put = (path, content) => { mkdirSync(dirname(join(root, path)), { recursive: true }); writeFileSync(join(root, path), content); };
  for (const path of ["openspec/GLOSSARY.md", "openspec/role/coder.md", "openspec/role/tester.md", "openspec/role/reviewer.md"]) put(path, "# 已填写职责\n");
  mkdirSync(join(root, "openspec/adr"), { recursive: true });
  put("openspec/changes/sample/proposal.md", proposal);
  put("openspec/changes/sample/tests.md", valid);
  put("openspec/changes/sample/plan.md", plan);
  put("openspec/changes/sample/tasks.md", "# Tasks\n\n- [ ] 1.1 WP1 实现X；完成判据为npm run check通过。\n");
  put("openspec/testing/suites/sample.md", valid);
  return { root, put };
}

test("活动用例和长期套件真实磁盘校验；仅精确archive豁免", (t) => {
  const { root, put } = fixture(t);
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/archive/old/proposal.md", "旧归档不要求tests");
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/archive-lookalike/proposal.md", "普通活动变更");
  assert.match(validateWorkflowAssets(root).join("\n"), /archive-lookalike.*tests.md/);
});

for (const path of ["openspec/GLOSSARY.md", "openspec/role/coder.md", "openspec/role/tester.md", "openspec/role/reviewer.md", "openspec/changes/sample/proposal.md", "openspec/changes/sample/tests.md", "openspec/adr", "openspec/testing/suites"]) {
  test(`资产缺失独立拒绝 ${path}`, (t) => {
    const { root } = fixture(t);
    assert.deepEqual(validateWorkflowAssets(root), []);
    rmSync(join(root, path), { recursive: true });
    assert.ok(validateWorkflowAssets(root).some((error) => error.includes(path)));
  });
}

test("损坏长期套件不能借归档豁免", (t) => {
  const { root, put } = fixture(t);
  put("openspec/testing/suites/sample.md", "# 空用例\n");
  assert.ok(validateWorkflowAssets(root).some((error) => error.includes("suites/sample.md")));
});

test("活动proposal必备节独立拒绝", (t) => {
  const { root, put } = fixture(t);
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/proposal.md", "# Proposal\n\n## Why\n\n缺两个必备节。\n");
  const missing = validateWorkflowAssets(root).join("\n");
  assert.match(missing, /缺少 ## Exploration/);
  assert.match(missing, /缺少 ## Success Criteria/);
  put("openspec/changes/sample/proposal.md", "# Proposal\n\n## Success Criteria\n\n<!-- 仅注释 -->\n\n## Exploration\n\n有内容。\n");
  assert.match(validateWorkflowAssets(root).join("\n"), /## Success Criteria 节缺少正文/);
});

test("L5用例Requirement的spec引用门禁", (t) => {
  const { root, put } = fixture(t);
  put("openspec/changes/sample/specs/demo/spec.md", "# Spec Delta\n");
  put("openspec/specs/durable/spec.md", "# Durable Spec\n");
  const levels5 = levels.replace("| L2 | required", "| L2 | not-applicable").replace("| L5 | not-applicable", "| L5 | required");
  const doc = (requirement) => `# Tests\n\n${levels5}## Cases\n\n${block({ ...fields, Level: "L5", Requirement: requirement })}`;
  const options = { baseDirs: [join(root, "openspec/changes/sample"), join(root, "openspec")], requireSpecRef: true };
  assert.deepEqual(validateTestDocument(doc("`specs/demo/spec.md` :: 成功场景"), "t", options), []);
  assert.deepEqual(validateTestDocument(doc("`openspec/specs/durable/spec.md` :: 场景"), "t", options), []);
  assert.match(validateTestDocument(doc("`specs/missing/spec.md`"), "t", options).join("\n"), /specs\/missing\/spec\.md 不存在/);
  assert.match(validateTestDocument(doc("config.yaml rules.tests"), "t", options).join("\n"), /L5 用例必须引用/);
});

test("skip_specs变更的L5可引用proposal验收条目", (t) => {
  const { root, put } = fixture(t);
  const levels5 = levels.replace("| L2 | required", "| L2 | not-applicable").replace("| L5 | not-applicable", "| L5 | required");
  const doc = `# Tests\n\n${levels5}## Cases\n\n${block({ ...fields, Level: "L5", Requirement: "proposal.md 的 Success Criteria 条目 R1" })}`;
  put("openspec/changes/sample/tests.md", doc);
  put("openspec/changes/sample/.openspec.yaml", "schema: spec-driven\nskip_specs: true\n");
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/.openspec.yaml", "schema: spec-driven\n");
  assert.match(validateWorkflowAssets(root).join("\n"), /L5 用例必须引用/);
});

test("活动plan结构与工作包引用门禁", (t) => {
  const { root, put } = fixture(t);
  assert.deepEqual(validateWorkflowAssets(root), []);
  const mutate = (fn) => {
    put("openspec/changes/sample/plan.md", fn(plan));
    return validateWorkflowAssets(root);
  };
  assert.match(mutate((text) => text.replace("## Merge Order", "## Merged Order")).join("\n"), /plan\.md: 缺少 ## Merge Order/);
  assert.match(mutate((text) => text.replace("WP1无并行项。", "")).join("\n"), /## Parallel Assessment 节缺少正文/);
  assert.match(mutate((text) => text.replace("| WP1 | 实现X | 方案检视 | coder | src/** | diff | npm run check |", "| WP1 | 实现X | 方案检视 | coder | src/** | diff |")).join("\n"), /必须 7 列/);
  assert.match(mutate((text) => text.replace("| WP1 |", "| WP2 |")).join("\n"), /工作包 WP2 未被 tasks\.md 引用/);
});

test("Main E2E模式一致性门禁", (t) => {
  const { root, put } = fixture(t);
  assert.deepEqual(validateWorkflowAssets(root), []);
  const mutate = (fn) => {
    put("openspec/changes/sample/plan.md", fn(plan));
    return validateWorkflowAssets(root);
  };
  assert.match(mutate((text) => text.replace("mode: not-applicable", "mode: maybe")).join("\n"), /Main E2E mode 必须为/);
  assert.match(mutate((text) => text.replace("reason: 工具链变更，无产品链路", 'reason: ""')).join("\n"), /reason 在 not-applicable 时必填/);
  assert.match(mutate((text) => text.replace("mode: not-applicable", "mode: required")).join("\n"), /reason 在 required 时必须留空/);
  const requiredEmpty = mutate((text) => text
    .replace("mode: not-applicable", "mode: required")
    .replace("reason: 工具链变更，无产品链路", 'reason: ""')
    .replace("basis: 无用户可见行为", 'basis: ""')
    .replace("alternative_checks: [L2]", "alternative_checks: []"));
  assert.deepEqual(requiredEmpty, []);
});

test("真实CLI：tests先于plan、缺失阻断apply、补齐恢复及归档保真", (t) => {
  const root = mkdtempSync(join(tmpdir(), "acpr-workflow-cli-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repo = fileURLToPath(new URL("../../", import.meta.url));
  const engine = join(repo, "node_modules/@fission-ai/openspec/bin/openspec.js");
  const spec = join(root, "openspec");
  mkdirSync(spec);
  writeFileSync(join(spec, "config.yaml"), "schema: spec-driven\n");
  cpSync(join(repo, "openspec/schemas"), join(spec, "schemas"), { recursive: true });
  function cli(...args) {
    const result = spawnSync(process.execPath, [engine, ...args], {
      cwd: root, encoding: "utf8", timeout: 30000,
      env: { ...process.env, OPENSPEC_TELEMETRY: "0", OPENSPEC_NO_UPDATE_CHECK: "1", DO_NOT_TRACK: "1" },
    });
    assert.equal(result.status, 0, `${args.join(" ")}\n${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  }
  cli("new", "change", "workflow-smoke");
  const change = join(spec, "changes/workflow-smoke");
  writeFileSync(join(change, ".openspec.yaml"), "schema: spec-driven\nskip_specs: true\n");
  writeFileSync(join(change, "proposal.md"), "# Proposal\n\n## Why\n\n隔离验证工作流，无产品行为变化。\n\n## What Changes\n\n验证本地工具链。\n\n## Capabilities\n\n无，skip_specs。\n\n## Impact\n\n临时目录。\n");
  writeFileSync(join(change, "design.md"), "# Design\n\n隔离临时目录，仅验证CLI依赖。\n");
  const status = JSON.parse(cli("status", "--change", "workflow-smoke", "--json"));
  assert.equal(status.artifacts.find((item) => item.id === "tests").status, "ready");
  assert.equal(status.artifacts.find((item) => item.id === "plan").status, "blocked");
  const instructions = JSON.parse(cli("instructions", "tests", "--change", "workflow-smoke", "--json"));
  assert.match(instructions.template, /## Levels/);
  assert.match(instructions.instruction, /independent tester/);
  // Existence of downstream files must not let apply skip its required tests.
  writeFileSync(join(change, "plan.md"), "# Plan\n\n临时验证。\n");
  writeFileSync(join(change, "tasks.md"), "# Tasks\n\n- [x] 1.1 临时验证完成\n");
  const blocked = JSON.parse(cli("instructions", "apply", "--change", "workflow-smoke", "--json"));
  assert.equal(blocked.state, "blocked");
  assert.ok(blocked.missingArtifacts.includes("tests"));
  writeFileSync(join(change, "tests.md"), valid);
  const ready = JSON.parse(cli("instructions", "apply", "--change", "workflow-smoke", "--json"));
  assert.equal(ready.state, "all_done");
  assert.ok(ready.contextFiles.tests.some((path) => path.endsWith("tests.md")));
  cli("archive", "workflow-smoke", "--yes");
  const archived = readdirSync(join(spec, "changes/archive")).find((name) => name.endsWith("-workflow-smoke"));
  assert.ok(archived);
  assert.equal(readFileSync(join(spec, "changes/archive", archived, "tests.md"), "utf8"), valid);
});

test("文档门禁忽略原始运行报告，但拒绝现行文档断链", (t) => {
  const root = mkdtempSync(join(tmpdir(), "acpr-workflow-docs-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "scripts"));
  const entry = join(root, "scripts/check-doc-links.mjs");
  writeFileSync(entry, readFileSync(new URL("../check-doc-links.mjs", import.meta.url)));
  writeFileSync(join(root, "README.md"), "# 正式文档\n");
  mkdirSync(join(root, "openspec/testing/runs/example"), { recursive: true });
  writeFileSync(join(root, "openspec/testing/runs/example/report.md"), "[本机报告引用](missing-local-file.md)\n");
  const invoke = () => spawnSync(process.execPath, [entry], { cwd: root, encoding: "utf8", timeout: 30000 });
  assert.equal(invoke().status, 0, "ignored run reports must not affect repository checks");
  writeFileSync(join(root, "README.md"), "# 正式文档\n\n[断链](missing-current-document.md)\n");
  const result = invoke();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /missing-current-document/);
});
