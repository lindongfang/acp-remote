#!/usr/bin/env node
// Universal structural & traceability validator for OpenSpec changes and durable test suites.
// Checks artifact definitions, WP↔tasks bindings, Main E2E YAML, and spec/test coverage only.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CASE_HEADING_RE = /^### ([A-Z][A-Z0-9]*-[A-Z0-9-]*\d{2,3}) (.+)$/gm;
const CASE_ID_TOKEN_RE = /\b[A-Z][A-Z0-9]*-[A-Z0-9-]*\d{2,3}\b/g;
const CASE_LEVEL_RE = /^(?:L[1-6]|LX)(?:[、/]\s*(?:L[1-6]|LX))*(?:（[^）]+）|\([^)]+\))?$/;
const CASE_FIELDS = ['用例级别', '操作步骤', '预期结果'];
const PROPOSAL_SECTIONS = [
  'Why',
  'Success Criteria',
  'What Changes',
  'Capabilities',
  'Impact',
  'Exploration',
];
const PLAN_SECTIONS = [
  'Execution Model',
  'Parallel Assessment',
  'Work Packages',
  'Merge Order',
  'Project Verify',
  'Code Review',
  'Main E2E',
  'Risks',
  'Handoff',
  'Findings',
  'Verification Results',
];
const ALLOWED_NA_REASONS = new Set([
  'pure-refactor',
  'docs-only',
  'no-user-surface',
  'isolated-internal-module',
]);

const abs = relPath => path.join(root, relPath);
const exists = relPath => fs.existsSync(abs(relPath));
const read = relPath => fs.readFileSync(abs(relPath), 'utf8').replaceAll('\r\n', '\n');

function parseArgs(argv) {
  const options = {change: null, specOnly: false, strictCoverage: false, help: false};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--all') options.change = null;
    else if (arg === '--spec-only') options.specOnly = true;
    else if (arg === '--strict-coverage') options.strictCoverage = true;
    else if (arg === '--change') {
      const value = argv[i + 1];
      if (!value || value.startsWith('--')) {
        throw new Error('--change 需要指定变更目录名，例如 --change deliver-session-capacity');
      }
      options.change = value;
      i += 1;
    } else if (arg.startsWith('--change=')) {
      options.change = arg.slice('--change='.length);
    } else {
      throw new Error(`未知参数: ${arg}`);
    }
  }
  return options;
}

function stripComments(markdown) {
  return markdown.replace(/<!--[\s\S]*?-->/g, '');
}

function stripCommentsAndFences(markdown) {
  return stripComments(markdown).replace(/^```[^\n]*\n[\s\S]*?^```[ \t]*$/gm, '');
}

function stripYamlInlineComment(line) {
  let quote = null;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (quote) {
      if (ch === '\\' && quote === '"') {
        i += 1;
      } else if (ch === quote) {
        quote = null;
      }
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === '#' && (i === 0 || /\s/.test(line[i - 1]))) {
      return line.slice(0, i).trimEnd();
    }
  }
  return line.trimEnd();
}

function extractSubsections(markdown, levelHashes) {
  const headings = [...markdown.matchAll(new RegExp(`^${levelHashes} (.+)$`, 'gm'))];
  const map = new Map();
  for (let i = 0; i < headings.length; i += 1) {
    const name = headings[i][1].trim();
    const start = headings[i].index + headings[i][0].length;
    const end = headings[i + 1]?.index ?? markdown.length;
    const body = markdown.slice(start, end).trim();
    if (!map.has(name)) map.set(name, body);
  }
  return map;
}

function extractH2Sections(markdown, keepFences = false) {
  const cleaned = keepFences ? stripComments(markdown) : stripCommentsAndFences(markdown);
  return extractSubsections(cleaned, '##');
}

function splitMarkdownCells(line) {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map(cell => cell.trim());
}

function isSeparatorRow(cells) {
  return cells.length > 0 && cells.every(cell => /^:?-{3,}:?$/.test(cell));
}

function parseMarkdownTables(sectionText) {
  const lines = sectionText.split('\n');
  const tables = [];
  let current = [];
  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (line.startsWith('|') && line.endsWith('|') && line.length >= 2) {
      current.push(splitMarkdownCells(line));
    } else if (current.length > 0) {
      tables.push(current);
      current = [];
    }
  }
  if (current.length > 0) tables.push(current);
  return tables;
}

function parseSkipSpecs(changeDir) {
  const metaPath = `${changeDir}/.openspec.yaml`;
  if (!exists(metaPath)) return false;
  return /^skip_specs:\s*true\s*$/m.test(read(metaPath));
}

function parseSpecScenarios(specText, specPath, check, deltaOnly = false) {
  const cleaned = stripCommentsAndFences(specText);
  const scenarioSet = new Set();
  const byRequirement = new Map();

  const parseBlock = (blockText, activeForCoverage) => {
    const reqs = [...blockText.matchAll(/^### Requirement:\s*(.+)$/gm)];
    for (let i = 0; i < reqs.length; i += 1) {
      const reqName = reqs[i][1].trim();
      const sub = blockText.slice(reqs[i].index, reqs[i + 1]?.index ?? blockText.length);
      const scenarios = [...sub.matchAll(/^#### Scenario:\s*(.+)$/gm)].map(m => m[1].trim());
      check(scenarios.length > 0, `${specPath} Requirement "${reqName}": 缺少 #### Scenario:`);
      if (!activeForCoverage) continue;
      const set = byRequirement.get(reqName) ?? new Set();
      for (const sc of scenarios) {
        set.add(sc);
        scenarioSet.add(`${specPath}\0${reqName}\0${sc}`);
      }
      byRequirement.set(reqName, set);
    }
  };

  if (deltaOnly) {
    const h2 = [...cleaned.matchAll(/^## (.+)$/gm)];
    for (let i = 0; i < h2.length; i += 1) {
      const title = h2[i][1].trim();
      const body = cleaned.slice(h2[i].index, h2[i + 1]?.index ?? cleaned.length);
      const isAddedOrModified = /^(ADDED|MODIFIED) Requirements$/.test(title);
      if (isAddedOrModified) {
        check(/^### Requirement:/m.test(body), `${specPath} "## ${title}": 缺少 ### Requirement:`);
        parseBlock(body, true);
      }
    }
  } else {
    check(/^### Requirement:/m.test(cleaned), `${specPath}: 缺少 ### Requirement:`);
    parseBlock(cleaned, true);
  }
  return {scenarioSet, byRequirement};
}

function parseCases(text, location, check) {
  const cleaned = stripComments(text);
  const headings = [...cleaned.matchAll(CASE_HEADING_RE)];
  const seenIds = new Set();
  const list = [];
  for (let i = 0; i < headings.length; i += 1) {
    const id = headings[i][1].trim();
    const title = headings[i][2].trim();
    const body = cleaned.slice(headings[i].index, headings[i + 1]?.index ?? cleaned.length);
    check(!seenIds.has(id), `${location}: 重复用例 ID ${id}`);
    seenIds.add(id);
    const entry = {id, title};
    for (const field of CASE_FIELDS) {
      const matches = [...body.matchAll(new RegExp(`^- ${field}：(.*)$`, 'gm'))];
      check(
        matches.length === 1 && Boolean(matches[0][1].trim()),
        `${location} ${id}: "${field}" 必须唯一且非空`,
      );
      entry[field] = matches[0]?.[1]?.trim() ?? '';
    }
    check(CASE_LEVEL_RE.test(entry['用例级别']), `${location} ${id}: 非法用例级别 "${entry['用例级别']}"`);
    check(!/^- (Status:|执行结果：)/m.test(body), `${location} ${id}: 执行结果必须另存 testing/runs/，不得写入用例定义`);
    list.push(entry);
  }
  return list;
}

function parseMainE2eYaml(yamlText, location, check) {
  const lines = yamlText.split('\n');
  const data = {};
  const seenKeys = new Set();
  let currentListKey = null;

  for (const rawLine of lines) {
    const raw = stripYamlInlineComment(rawLine);
    const trimmed = raw.trim();
    if (!trimmed) continue;
    check(
      !/(^|\s)[&*][A-Za-z0-9_-]+/.test(trimmed),
      `${location}: Main E2E YAML 禁止使用锚点或别名 (${trimmed})`,
    );

    if (/^\s*-\s+/.test(raw)) {
      check(Boolean(currentListKey), `${location}: Main E2E YAML 列表项缺少父键 (${trimmed})`);
      if (currentListKey) {
        const val = trimmed.replace(/^-\s+/, '').replace(/^['"]|['"]$/g, '').trim();
        check(Boolean(val), `${location}: Main E2E YAML ${currentListKey} 列表项不能为空`);
        data[currentListKey].push(val);
      }
      continue;
    }

    const kv = raw.match(/^([a-z_][a-z0-9_]*):\s*(.*)$/);
    check(Boolean(kv), `${location}: Main E2E YAML 无法解析行 "${trimmed}"`);
    if (!kv) continue;
    const [, key, rawVal] = kv;
    check(!seenKeys.has(key), `${location}: Main E2E YAML 存在重复键 "${key}"`);
    seenKeys.add(key);

    const cleanVal = rawVal.replace(/^['"]|['"]$/g, '').trim();
    if (!cleanVal) {
      data[key] = [];
      currentListKey = key;
    } else if (cleanVal.startsWith('[') && cleanVal.endsWith(']')) {
      const items = cleanVal
        .slice(1, -1)
        .split(',')
        .map(item => item.replace(/^['"]|['"]$/g, '').trim())
        .filter(Boolean);
      data[key] = items;
      currentListKey = null;
    } else {
      data[key] = cleanVal;
      currentListKey = null;
    }
  }

  check(
    data.mode === 'required' || data.mode === 'not-applicable',
    `${location}: Main E2E mode 必须为 required 或 not-applicable`,
  );
  if (data.mode === 'required') {
    for (const forbidden of ['reason', 'basis', 'alternative_checks']) {
      check(!(forbidden in data), `${location}: mode=required 时不得包含 ${forbidden}`);
    }
  } else if (data.mode === 'not-applicable') {
    check(
      typeof data.reason === 'string' && ALLOWED_NA_REASONS.has(data.reason),
      `${location}: mode=not-applicable 时 reason 必须属于 ${[...ALLOWED_NA_REASONS].join('|')}`,
    );
    check(
      typeof data.basis === 'string' && data.basis.length > 0 && !Array.isArray(data.basis),
      `${location}: mode=not-applicable 时 basis 必须为非空字符串`,
    );
    check(
      Array.isArray(data.alternative_checks) && data.alternative_checks.length > 0,
      `${location}: mode=not-applicable 时 alternative_checks 必须为非空字符串数组`,
    );
  }
}

function parseCapabilityBullets(blockText) {
  if (!blockText) return [];
  return [...blockText.matchAll(/^- `?([a-z0-9]+(?:-[a-z0-9]+)*)`?(?=\s|[：:—-]|$)/gm)].map(
    m => m[1],
  );
}

function recordMappingEntry({
  specRef,
  reqName,
  scenarioNames,
  rawCaseCell,
  testsPath,
  caseIdSet,
  deltaSpecMap,
  getLongTermSpec,
  coveredDelta,
  check,
}) {
  const ids = [...rawCaseCell.matchAll(CASE_ID_TOKEN_RE)].map(m => m[0]);
  check(ids.length > 0, `${testsPath}: 需求关联行缺少用例 ID (${specRef} / ${reqName})`);
  for (const id of ids) {
    check(caseIdSet.has(id), `${testsPath}: 需求关联引用了不存在的用例 ID "${id}"`);
  }

  const isLongTerm = specRef.startsWith('openspec/specs/');
  const reqMap = isLongTerm ? getLongTermSpec(specRef) : deltaSpecMap.get(specRef);
  check(Boolean(reqMap), `${testsPath}: 需求关联引用了不存在的规格文件 "${specRef}"`);
  if (!reqMap) return;

  const scenSet = reqMap.get(reqName);
  check(Boolean(scenSet), `${testsPath}: "${specRef}" 中不存在 Requirement "${reqName}"`);
  if (!scenSet) return;

  const resolvedScenarios =
    scenarioNames.length === 1 && !scenSet.has(scenarioNames[0]) && scenarioNames[0].includes('、')
      ? scenarioNames[0].split('、').map(s => s.trim()).filter(Boolean)
      : scenarioNames;

  for (const scName of resolvedScenarios) {
    check(
      scenSet.has(scName),
      `${testsPath}: "${specRef}" Requirement "${reqName}" 中不存在 Scenario "${scName}"`,
    );
    if (!isLongTerm && scenSet.has(scName) && ids.length > 0) {
      coveredDelta.add(`${specRef}\0${reqName}\0${scName}`);
    }
  }
}

function validateChange(changeName, requireImplementationArtifacts, forceStrictCoverage, check, warn, stats) {
  const changeDir = `openspec/changes/${changeName}`;
  check(exists(changeDir), `变更目录不存在: ${changeDir}`);
  if (!exists(changeDir)) return;

  const proposalPath = `${changeDir}/proposal.md`;
  const designPath = `${changeDir}/design.md`;
  check(exists(proposalPath), `${changeDir}: 缺少 proposal.md`);
  check(exists(designPath), `${changeDir}: 缺少 design.md`);
  if (!exists(proposalPath)) return;

  const proposalText = read(proposalPath);
  const proposalSections = extractH2Sections(proposalText);
  for (const section of PROPOSAL_SECTIONS) {
    check(
      Boolean(proposalSections.get(section)),
      `${proposalPath}: 缺少或为空的章节 "## ${section}"`,
    );
  }
  if (exists(designPath)) {
    check(read(designPath).trim().length > 0, `${designPath}: 内容不能为空`);
  }

  const skipSpecs = parseSkipSpecs(changeDir);
  const deltaSpecMap = new Map();
  const allDeltaScenarios = new Set();

  if (!skipSpecs && proposalSections.get('Capabilities')) {
    const capSubsections = extractSubsections(proposalSections.get('Capabilities'), '###');
    const newCaps = parseCapabilityBullets(capSubsections.get('New Capabilities') ?? '');
    const modCaps = parseCapabilityBullets(capSubsections.get('Modified Capabilities') ?? '');
    const declaredCaps = new Set([...newCaps, ...modCaps]);
    check(declaredCaps.size > 0, `${proposalPath}: Capabilities 未声明任何有效能力路径`);

    for (const cap of newCaps) {
      const relSpec = `specs/${cap}/spec.md`;
      const fullSpec = `${changeDir}/${relSpec}`;
      check(exists(fullSpec), `${proposalPath}: New Capability "${cap}" 缺少 ${fullSpec}`);
    }
    for (const cap of modCaps) {
      const relSpec = `specs/${cap}/spec.md`;
      const fullSpec = `${changeDir}/${relSpec}`;
      const longTermSpec = `openspec/specs/${cap}/spec.md`;
      check(exists(fullSpec), `${proposalPath}: Modified Capability "${cap}" 缺少 ${fullSpec}`);
      check(
        exists(longTermSpec),
        `${proposalPath}: Modified Capability "${cap}" 缺少长期规格 ${longTermSpec}`,
      );
    }

    const specsRoot = `${changeDir}/specs`;
    if (exists(specsRoot)) {
      const diskCaps = fs
        .readdirSync(abs(specsRoot), {withFileTypes: true})
        .filter(d => d.isDirectory())
        .map(d => d.name);
      for (const diskCap of diskCaps) {
        const relSpec = `specs/${diskCap}/spec.md`;
        const fullSpec = `${changeDir}/${relSpec}`;
        check(declaredCaps.has(diskCap), `${fullSpec}: 未在 proposal.md Capabilities 中声明`);
        if (exists(fullSpec)) {
          const parsed = parseSpecScenarios(read(fullSpec), relSpec, check, true);
          deltaSpecMap.set(relSpec, parsed.byRequirement);
          for (const key of parsed.scenarioSet) allDeltaScenarios.add(key);
        }
      }
    }
  }

  const implFiles = ['tests.md', 'plan.md', 'tasks.md'];
  const presentImpl = implFiles.filter(name => exists(`${changeDir}/${name}`));
  if (!requireImplementationArtifacts && presentImpl.length === 0) {
    stats.specOnlyChanges += 1;
    return;
  }

  stats.implChanges += 1;
  for (const name of implFiles) {
    check(
      exists(`${changeDir}/${name}`),
      `${changeDir}: 进入实施准备门禁时必须同时具备 tests.md、plan.md、tasks.md（当前缺少 ${name}）`,
    );
  }
  if (presentImpl.length < implFiles.length) return;

  // 1. Validate plan.md
  const planPath = `${changeDir}/plan.md`;
  const planRaw = read(planPath);
  const planSections = extractH2Sections(planRaw, false);
  const planSectionsWithFences = extractH2Sections(planRaw, true);
  for (const section of PLAN_SECTIONS) {
    check(Boolean(planSectionsWithFences.get(section)), `${planPath}: 缺少或为空的章节 "## ${section}"`);
  }

  const wpIds = [];
  const wpTables = parseMarkdownTables(planSections.get('Work Packages') ?? '');
  const wpTable = wpTables.find(t => t.length >= 3 && t[0].length === 7 && isSeparatorRow(t[1]));
  check(Boolean(wpTable), `${planPath}: "## Work Packages" 必须包含 7 列 Markdown 表`);
  if (wpTable) {
    const wpSeen = new Set();
    for (const row of wpTable.slice(2)) {
      check(row.length === 7, `${planPath}: Work Packages 行列数不为 7 (${row[0] ?? ''})`);
      const wpId = row[0]?.replace(/^`|`$/g, '').trim() ?? '';
      check(/^WP[A-Za-z0-9_-]*\d+[A-Za-z0-9_-]*$/.test(wpId), `${planPath}: 非法 Work Package ID "${wpId}"`);
      check(!wpSeen.has(wpId), `${planPath}: 重复 Work Package ID "${wpId}"`);
      if (wpId) {
        wpSeen.add(wpId);
        wpIds.push(wpId);
      }
    }
    check(wpIds.length > 0, `${planPath}: Work Packages 至少定义一个工作包`);
  }

  const mainE2eSection = planSectionsWithFences.get('Main E2E') ?? '';
  const yamlBlocks = [...mainE2eSection.matchAll(/^```yaml[ \t]*\n([\s\S]*?)^```[ \t]*$/gm)];
  check(yamlBlocks.length === 1, `${planPath}: "## Main E2E" 必须包含且仅包含 1 个 \`\`\`yaml 代码块`);
  if (yamlBlocks.length === 1) {
    parseMainE2eYaml(yamlBlocks[0][1], planPath, check);
  }

  // 2. Validate tasks.md <-> plan.md bidirectional WP binding
  const tasksPath = `${changeDir}/tasks.md`;
  const tasksCleaned = stripCommentsAndFences(read(tasksPath));
  const followUpIdx = tasksCleaned.search(/^## Workflow follow-up\b/m);
  const trackedTasksText = followUpIdx >= 0 ? tasksCleaned.slice(0, followUpIdx) : tasksCleaned;
  const followUpText = followUpIdx >= 0 ? tasksCleaned.slice(followUpIdx) : '';

  check(
    !/^- \[[xX]\] /m.test(followUpText),
    `${tasksPath}: "## Workflow follow-up" 不得包含已勾选的 tracked checkbox`,
  );

  const checkboxes = [...trackedTasksText.matchAll(/^- \[[ xX]\] (.+)$/gm)].map(m => m[1].trim());
  const checkedCount = [...trackedTasksText.matchAll(/^- \[[xX]\] (.+)$/gm)].length;
  check(checkboxes.length > 0, `${tasksPath}: 至少包含一个真实任务 checkbox`);

  const wpReferencedCount = new Map(wpIds.map(id => [id, 0]));
  for (const line of checkboxes) {
    const matchedWps = wpIds.filter(wpId =>
      new RegExp(`(?<![A-Za-z0-9_-])${wpId}(?![A-Za-z0-9_-])`).test(line),
    );
    check(
      matchedWps.length > 0,
      `${tasksPath}: checkbox 未按完整词元引用任何已声明 Work Package ID -> "${line.slice(0, 60)}..."`,
    );
    for (const wpId of matchedWps) {
      wpReferencedCount.set(wpId, (wpReferencedCount.get(wpId) ?? 0) + 1);
    }
  }
  for (const [wpId, count] of wpReferencedCount.entries()) {
    check(count > 0, `${planPath} <-> ${tasksPath}: 工作包 ${wpId} 未被任何真实任务 checkbox 引用`);
  }

  // 3. Validate tests.md & delta Requirement/Scenario coverage
  const testsPath = `${changeDir}/tests.md`;
  const testsRaw = read(testsPath);
  const parsedCases = parseCases(testsRaw, testsPath, check);
  check(parsedCases.length > 0, `${testsPath}: 至少定义一个验收用例`);
  const caseIdSet = new Set(parsedCases.map(item => item.id));
  stats.activeChangeCases += parsedCases.length;

  if (!skipSpecs) {
    const longTermCache = new Map();
    const getLongTermSpec = specRel => {
      if (!longTermCache.has(specRel)) {
        if (!exists(specRel)) {
          longTermCache.set(specRel, null);
        } else {
          longTermCache.set(
            specRel,
            parseSpecScenarios(read(specRel), specRel, check, false).byRequirement,
          );
        }
      }
      return longTermCache.get(specRel);
    };

    const coveredDelta = new Set();
    const tables = parseMarkdownTables(stripCommentsAndFences(testsRaw));
    for (const table of tables) {
      if (table.length < 3 || !isSeparatorRow(table[1])) continue;
      for (const row of table.slice(2)) {
        // Format A: Canonical 4-column mapping table: | specPath | Requirement | Scenario(s) | Case IDs |
        if (
          row.length === 4 &&
          /^(openspec\/)?specs\/[a-z0-9-]+\/spec\.md$/.test(row[0].replace(/^`|`$/g, ''))
        ) {
          recordMappingEntry({
            specRef: row[0].replace(/^`|`$/g, ''),
            reqName: row[1].trim(),
            scenarioNames: [row[2].trim()],
            rawCaseCell: row[3].trim(),
            testsPath,
            caseIdSet,
            deltaSpecMap,
            getLongTermSpec,
            coveredDelta,
            check,
          });
        } else if (row.length === 2) {
          // Format B: Legacy 2-column slash format: | specs/<cap>/spec.md / Requirement / Scenario(s) | Case IDs |
          const slashMatch = row[0]
            .replace(/^`|`$/g, '')
            .match(/^((?:openspec\/)?specs\/[a-z0-9-]+\/spec\.md)\s*\/\s*([^/]+?)\s*\/\s*(.+)$/);
          if (slashMatch) {
            recordMappingEntry({
              specRef: slashMatch[1].trim(),
              reqName: slashMatch[2].trim(),
              scenarioNames: [slashMatch[3].trim()],
              rawCaseCell: row[1].trim(),
              testsPath,
              caseIdSet,
              deltaSpecMap,
              getLongTermSpec,
              coveredDelta,
              check,
            });
            continue;
          }

          // Format C: Legacy 2-column dash format (either column order):
          // | Case IDs | specs/<cap>/spec.md — Requirement: ...；Scenario: ... | OR reversed
          const defIdx = row[0].includes('Requirement:') ? 0 : row[1].includes('Requirement:') ? 1 : -1;
          if (defIdx >= 0) {
            const defCell = row[defIdx].replace(/^`|`$/g, '');
            const caseCell = row[1 - defIdx];
            const ref = defCell.match(
              /^((?:openspec\/)?specs\/[a-z0-9-]+\/spec\.md)\s*[—-]\s*Requirement:\s*(.+?)；(.+)$/,
            );
            check(Boolean(ref), `${testsPath}: 无法解析需求关联 "${defCell}"`);
            if (!ref) continue;
            const [, specRef, reqName, rest] = ref;
            const scenarioNames = [...rest.matchAll(/Scenario:\s*(.+?)(?=；|$)/g)].map(m =>
              m[1].trim(),
            );
            recordMappingEntry({
              specRef: specRef.trim(),
              reqName: reqName.trim(),
              scenarioNames,
              rawCaseCell: caseCell.trim(),
              testsPath,
              caseIdSet,
              deltaSpecMap,
              getLongTermSpec,
              coveredDelta,
              check,
            });
          }
        }
      }
    }

    const enforceCoverageAsError =
      requireImplementationArtifacts || forceStrictCoverage || checkedCount > 0;
    for (const key of allDeltaScenarios) {
      if (!coveredDelta.has(key)) {
        const msg = `${testsPath}: 增量规格场景缺少验收用例映射 -> ${key.replaceAll('\0', ' / ')}`;
        if (enforceCoverageAsError) {
          check(false, msg);
        } else {
          warn(`${msg}（当前 tasks 进度 0/${checkboxes.length}，进入编码前门禁 --change ${changeName} 前必须补齐）`);
        }
      }
    }
  }
}

function parseSourceDefinitionPaths(suiteText) {
  const paths = [];
  for (const match of suiteText.matchAll(/Source definitions:\s*(.+)$/gm)) {
    const tokens = match[1]
      .split(/[、,，;；\s]+/)
      .map(token => token.replace(/^`|`$/g, '').trim())
      .filter(Boolean);
    for (const token of tokens) {
      if (!paths.includes(token)) paths.push(token);
    }
  }
  return paths;
}

function validateDurableSuites(check, stats) {
  const suitesDir = 'testing/suites';
  const specsDir = 'openspec/specs';

  if (exists(specsDir)) {
    const specCapabilities = fs
      .readdirSync(abs(specsDir), {withFileTypes: true})
      .filter(d => d.isDirectory())
      .map(d => d.name)
      .sort();
    for (const capability of specCapabilities) {
      const specPath = `${specsDir}/${capability}/spec.md`;
      const suitePath = `${suitesDir}/${capability}/tests.md`;
      if (exists(specPath)) {
        parseSpecScenarios(read(specPath), specPath, check, false);
        check(
          exists(suitePath),
          `长期规格 ${specPath} 缺少对应的长期验收套件 ${suitePath}`,
        );
      }
    }
  }

  if (!exists(suitesDir)) return;

  const archiveSourceCache = new Map();
  const globalCaseOwner = new Map();
  const capabilityDirs = fs
    .readdirSync(abs(suitesDir), {withFileTypes: true})
    .filter(d => d.isDirectory() && d.name !== 'traceability')
    .map(d => d.name)
    .sort();

  for (const capability of capabilityDirs) {
    const suitePath = `${suitesDir}/${capability}/tests.md`;
    const specPath = `openspec/specs/${capability}/spec.md`;
    check(exists(suitePath), `长期套件缺少文件: ${suitePath}`);
    check(exists(specPath), `长期套件对应规格不存在: ${specPath}`);
    if (!exists(suitePath)) continue;

    const suiteText = read(suitePath);
    check(suiteText.includes(specPath), `${suitePath}: 缺少长期规格引用 "${specPath}"`);

    const suiteCases = parseCases(suiteText, suitePath, check);
    check(suiteCases.length > 0, `${suitePath}: 长期套件至少包含一个用例`);
    stats.durableSuites += 1;
    stats.durableCases += suiteCases.length;

    for (const item of suiteCases) {
      const prev = globalCaseOwner.get(item.id);
      check(
        !prev,
        `长期套件用例 ID "${item.id}" 同时在 ${prev} 与 ${capability} 中重复定义正文（跨能力共用用例应在单一主能力套件定义正文，其他套件按 ID 引用）`,
      );
      globalCaseOwner.set(item.id, capability);
    }

    const declaredStable = suiteText.match(/^- Stable IDs:\s*(.+)$/m);
    if (declaredStable) {
      const declaredIds = [...declaredStable[1].matchAll(CASE_ID_TOKEN_RE)].map(m => m[0]);
      const actualIds = suiteCases.map(item => item.id);
      check(
        JSON.stringify(declaredIds) === JSON.stringify(actualIds),
        `${suitePath}: Stable IDs 声明列表与实际用例标题顺序不一致`,
      );
    }

    const srcPaths = parseSourceDefinitionPaths(suiteText);
    check(srcPaths.length > 0, `${suitePath}: 缺少 Source definitions 来源声明`);
    if (srcPaths.length > 0) {
      const mergedArchiveCases = new Map();
      const perSourceIds = new Map();
      for (const srcPath of srcPaths) {
        check(exists(srcPath), `${suitePath}: Source definitions 文件不存在 "${srcPath}"`);
        if (!exists(srcPath)) continue;
        if (!archiveSourceCache.has(srcPath)) {
          const map = new Map(
            parseCases(read(srcPath), srcPath, check).map(item => [item.id, item]),
          );
          archiveSourceCache.set(srcPath, map);
        }
        const srcMap = archiveSourceCache.get(srcPath);
        perSourceIds.set(srcPath, new Set(srcMap.keys()));
        for (const [id, item] of srcMap.entries()) {
          mergedArchiveCases.set(id, {item, srcPath});
        }
      }

      const usedSources = new Set();
      for (const item of suiteCases) {
        const resolved = mergedArchiveCases.get(item.id);
        check(
          Boolean(resolved),
          `${suitePath} ${item.id}: 未在声明的 Source definitions (${srcPaths.join('、')}) 中找到`,
        );
        if (resolved) {
          usedSources.add(resolved.srcPath);
          for (const key of ['title', ...CASE_FIELDS]) {
            check(
              resolved.item[key] === item[key],
              `${suitePath} ${item.id}: "${key}" 与最新归档来源 ${resolved.srcPath} 不一致`,
            );
          }
        }
      }
      for (const srcPath of perSourceIds.keys()) {
        const hasAny = suiteCases.some(c => perSourceIds.get(srcPath)?.has(c.id));
        check(hasAny, `${suitePath}: 声明的 Source definitions "${srcPath}" 未向本套件提供任何用例`);
      }
    }
  }
}

function main() {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(err.message);
    process.exitCode = 1;
    return;
  }

  if (options.help) {
    console.log(
      [
        'Usage:',
        '  node testing/suites/verify-suite.mjs                             # 校验全部活动变更与长期套件',
        '  node testing/suites/verify-suite.mjs --change <name>             # 按实施门禁校验指定活动变更（要求 tests/plan/tasks 齐全且 100% 场景映射）',
        '  node testing/suites/verify-suite.mjs --change <name> --spec-only # 仅按需求/设计阶段校验指定变更',
        '  node testing/suites/verify-suite.mjs --strict-coverage           # 全仓模式下对 0 进度草稿变更也强制 100% 场景映射报错',
      ].join('\n'),
    );
    return;
  }

  const errors = [];
  const warnings = [];
  const check = (valid, message) => {
    if (!valid) errors.push(message);
  };
  const warn = message => warnings.push(message);
  const stats = {
    specOnlyChanges: 0,
    implChanges: 0,
    activeChangeCases: 0,
    durableSuites: 0,
    durableCases: 0,
  };

  if (options.change) {
    validateChange(options.change, !options.specOnly, options.strictCoverage, check, warn, stats);
  } else {
    const changesRoot = 'openspec/changes';
    if (exists(changesRoot)) {
      const changeDirs = fs
        .readdirSync(abs(changesRoot), {withFileTypes: true})
        .filter(d => d.isDirectory() && d.name !== 'archive')
        .map(d => d.name)
        .sort();
      for (const name of changeDirs) {
        validateChange(name, false, options.strictCoverage, check, warn, stats);
      }
    }
    validateDurableSuites(check, stats);
  }

  if (warnings.length > 0) {
    console.warn(warnings.map(w => `WARN: ${w}`).join('\n'));
  }

  if (errors.length > 0) {
    console.error(errors.map(err => `- ${err}`).join('\n'));
    process.exitCode = 1;
    return;
  }

  if (options.change) {
    const modeLabel = options.specOnly ? 'spec-only' : 'implementation-ready';
    console.log(
      `Spec & suite structural check passed for "${options.change}" (${modeLabel}, ${stats.activeChangeCases} cases).`,
    );
  } else {
    console.log(
      `Spec & suite structural check passed: ${stats.implChanges} in-progress change(s) (${stats.activeChangeCases} cases), ${stats.specOnlyChanges} spec-only change(s), ${stats.durableSuites} durable suite(s) (${stats.durableCases} cases).`,
    );
  }
}

main();
