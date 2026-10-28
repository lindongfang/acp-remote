import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, cpSync, rmSync, symlinkSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { LEVELS, validatePlanDocument, validateTestDocument, validateWorkflowAssets } from "../openspec-test-cases.mjs";

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
  put("openspec/changes/sample/design.md", "# Design\n\n测试规划依据。\n");
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
  assert.match(validateWorkflowAssets(root).join("\n"), /archive-lookalike.*Exploration/);
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
  const options = { changeRoot: join(root, "openspec/changes/sample"), specRoot: join(root, "openspec"), requireSpecRef: true };
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

test("工作包只关联真实任务完整ID，兼容引擎列表和checkbox形式", () => {
  const check = (tasksText) => validatePlanDocument(plan, "plan", { tasksText });
  for (const text of [
    "- [ ] 1.1 `WP1`；检查通过", "* [x] WP1", "+ [] WP1", "1. [~] WP1", "  2) [ x ] WP1",
    "- [ ] WP1、WP1（中文标点）", "- [ ] WP1\r\n",
  ]) assert.deepEqual(check(text), [], text);
  for (const text of [
    "- [ ] WP10", "- [ ] WP1-extra", "- [ ] WP1_extra", "- [ ] 1WP1", "- [ ] otherWP1",
    "<!-- - [ ] WP1 -->", "<!--\n- [ ] WP1\n-->", "```md\n- [ ] WP1\n```",
    "~~~md\n- [ ] WP1\n~~~", "## WP1", "WP1", "- WP1", "- [WIP] WP1", "- [1](WP1)",
  ]) assert.match(check(text).join("\n"), /工作包 WP1 未被/, text);
  assert.match(check("- [ ] WP1\n- [ ] WP99").join("\n"), /第 2 个任务未引用/);
  assert.match(check("- [ ] WP1\n- [ ] 没有关联").join("\n"), /第 2 个任务未引用/);
  assert.match(check("- [ ] WP1\n```md\nexample").join("\n"), /未关闭/);
  const multi = plan.replace("| WP1 | 实现X", "| WP2 | 实现X")
    .replace("| WP2 | 实现X", "| WP1 | 实现X | 方案检视 | coder | src/** | diff | npm run check |\n| WP2 | 实现X");
  assert.deepEqual(validatePlanDocument(multi, "plan", { tasksText: "- [ ] WP1、`WP2` 验证" }), []);
});

test("Main E2E实际YAML空值、类型与语法，不凭字面值放行", () => {
  const e2e = (yaml) => plan.replace(/```yaml\n[^]*?```/, `\`\`\`yaml\n${yaml}\n\`\`\``);
  const validValues = { mode: "not-applicable", reason: "工具链变更", basis: "无产品变化", alternative_checks: "[L2]" };
  const doc = (overrides) => e2e(Object.entries({ ...validValues, ...overrides }).map(([key, value]) => `${key}: ${value}`).join("\n"));
  assert.deepEqual(validatePlanDocument(doc({})), []);
  for (const field of ["reason", "basis", "alternative_checks"]) {
    for (const empty of ["", "null", "Null", "NULL", "~", "''", '\"\"', "'   '", "[]", "[ ]"]) {
      assert.match(validatePlanDocument(doc({ [field]: empty })).join("\n"), new RegExp(field), `${field}: ${empty}`);
    }
    const omitted = doc({}).replace(new RegExp(`^${field}:.*\\n`, "m"), "");
    assert.match(validatePlanDocument(omitted).join("\n"), new RegExp(field));
    for (const value of ["42", "false", "{}", "[{}]", "[[L2]]"]) {
      assert.match(validatePlanDocument(doc({ [field]: value })).join("\n"), /类型错误/, `${field}: ${value}`);
    }
  }
  for (const value of ["'null'", "'~'", "'case #1: valid'", "|\n  多行理由"]) {
    assert.deepEqual(validatePlanDocument(doc({ reason: value })), [], value);
  }
  for (const field of ["reason", "basis"]) {
    for (const value of ["TODO", "TBD", "'<!-- explanation -->'"]) {
      assert.deepEqual(validatePlanDocument(doc({ [field]: value })), [], `${field}: ${value}`);
      const required = doc({ mode: "required", reason: "null", basis: "''", alternative_checks: "[]", [field]: value });
      assert.match(validatePlanDocument(required).join("\n"), new RegExp(`${field} 在 required 时必须留空`));
    }
  }
  assert.deepEqual(validatePlanDocument(doc({ reason: "工具链\n## Handoff" })), []);
  assert.deepEqual(validatePlanDocument(doc({ alternative_checks: "\n  - L2\n  - L3" })), []);
  for (const value of ["[null]", "[false]", "['']", "['   ']"]) {
    assert.match(validatePlanDocument(doc({ alternative_checks: value })).join("\n"), /类型错误/);
  }
  for (const yaml of [
    "mode: required", "mode: required\nreason: null\nbasis: ''\nalternative_checks: [ ]",
    "mode: required\nreason: ' '\nbasis: ~\nalternative_checks: null",
  ]) assert.deepEqual(validatePlanDocument(e2e(yaml)), []);
  for (const yaml of [
    "mode: required\nmode: not-applicable", "mode: [", "[]", "mode: &value required\nreason: *value",
    "mode: required\nreason: {}", "mode: required\nalternative_checks: ''", "mode: required\nreasonn: x",
  ]) assert.ok(validatePlanDocument(e2e(yaml)).length > 0, yaml);
  const duplicate = doc({}).replace("## Handoff", "```yaml\nmode: required\n```\n\n## Handoff");
  assert.match(validatePlanDocument(duplicate).join("\n"), /仅允许一个/);
});

test("规范路径只解析指定根，同名文件不得互相补位", (t) => {
  const { root, put } = fixture(t);
  const changeRoot = join(root, "openspec/changes/sample"), specRoot = join(root, "openspec");
  const check = (ref, options = { changeRoot, specRoot }) => validateTestDocument(
    valid.replace(fields.Requirement, ref), "tests", options);
  put("openspec/changes/sample/specs/demo/spec.md", "# Delta\n");
  assert.deepEqual(check("specs/demo/spec.md"), []);
  assert.match(check("openspec/specs/demo/spec.md").join("\n"), /不存在/);
  put("openspec/specs/demo/spec.md", "# Durable\n");
  assert.deepEqual(check("openspec/specs/demo/spec.md"), []);
  assert.deepEqual(check("specs/demo/spec.md", { specRoot }), []);
  rmSync(join(changeRoot, "specs/demo/spec.md"));
  assert.match(check("specs/demo/spec.md").join("\n"), /不存在/);
  assert.deepEqual(check("openspec/specs/demo/spec.md"), []);
  rmSync(join(specRoot, "specs/demo/spec.md"));
  mkdirSync(join(specRoot, "specs/demo/spec.md"));
  assert.match(check("openspec/specs/demo/spec.md").join("\n"), /不存在/);
});

test("规划前缀可保存，下游产物必须具备前置且已有内容须合法", (t) => {
  const { root, put } = fixture(t);
  const change = join(root, "openspec/changes/sample");
  const remove = (name) => rmSync(join(change, name), { force: true });
  for (const name of ["tasks.md", "plan.md", "tests.md", "design.md", "proposal.md"]) remove(name);
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/proposal.md", proposal);
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/design.md", "# Design\n");
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/tests.md", valid);
  assert.deepEqual(validateWorkflowAssets(root), []);
  put("openspec/changes/sample/plan.md", plan);
  assert.deepEqual(validateWorkflowAssets(root), []);
  const tasks = "- [ ] WP1 实现并验证";
  put("openspec/changes/sample/tasks.md", tasks);
  assert.deepEqual(validateWorkflowAssets(root), []);
  for (const [name, value] of [["proposal.md", proposal], ["design.md", "# Design\n"], ["tests.md", valid], ["plan.md", plan]]) {
    remove(name);
    assert.match(validateWorkflowAssets(root).join("\n"), new RegExp(`${name}:.*前置产物`));
    put(`openspec/changes/sample/${name}`, value);
  }
  for (const name of ["tasks.md", "plan.md", "tests.md", "proposal.md"]) remove(name);
  assert.match(validateWorkflowAssets(root).join("\n"), /proposal.md: design.md.*前置产物/);
});

test("真实完整门禁允许规划前缀，但不豁免坏delta、缺前置或长期规范", (t) => {
  const { root, put } = fixture(t);
  const repo = fileURLToPath(new URL("../../", import.meta.url));
  // 只共享只读依赖；测试创建的规划与脚本在独立临时项目中。
  symlinkSync(join(repo, "node_modules"), join(root, "node_modules"), "junction");
  cpSync(join(repo, "openspec/schemas"), join(root, "openspec/schemas"), { recursive: true });
  put("openspec/config.yaml", "schema: spec-driven\n");
  for (const file of ["check-openspec.mjs", "openspec-test-cases.mjs"]) {
    put(`scripts/${file}`, readFileSync(join(repo, "scripts", file), "utf8"));
  }
  put("package.json", '{"type":"module"}\n');
  const change = join(root, "openspec/changes/sample");
  for (const name of ["tasks.md", "plan.md", "tests.md", "design.md", "proposal.md"]) rmSync(join(change, name));
  const invoke = () => spawnSync(process.execPath, [join(root, "scripts/check-openspec.mjs")], {
    cwd: root, encoding: "utf8", timeout: 30000,
    env: { ...process.env, OPENSPEC_TELEMETRY: "0", OPENSPEC_NO_UPDATE_CHECK: "1", DO_NOT_TRACK: "1" },
  });
  const passes = () => {
    const result = invoke();
    assert.equal(result.status, 0, `${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
  };
  const fails = (pattern) => {
    const result = invoke();
    assert.notEqual(result.status, 0);
    assert.match(`${result.stdout}\n${result.stderr}`, pattern);
  };
  passes();
  put("openspec/changes/sample/proposal.md", proposal);
  passes();
  put("openspec/changes/sample/design.md", "# Design\n");
  passes();
  mkdirSync(join(change, "specs"));
  fails(/No deltas|at least one delta/i);
  put("openspec/changes/sample/specs/demo/spec.md", "# Broken delta\n");
  fails(/No delta sections|No deltas/i);
  rmSync(join(change, "specs"), { recursive: true });
  put("openspec/changes/sample/tests.md", valid);
  fails(/No deltas|at least one delta/i);
  put("openspec/changes/sample/.openspec.yaml", "schema: spec-driven\nskip_specs: true\n");
  passes();
  put("openspec/changes/sample/plan.md", plan);
  passes();
  put("openspec/changes/sample/tasks.md", "- [ ] 1.1 WP1 实现并验证\n");
  passes();
  rmSync(join(change, "tests.md"));
  fails(/tests.md:.*前置产物/);
  put("openspec/changes/sample/tests.md", valid);
  put("openspec/specs/broken/spec.md", "# Broken durable spec\n");
  fails(/broken/);
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
