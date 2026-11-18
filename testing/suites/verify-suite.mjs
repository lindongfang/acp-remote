#!/usr/bin/env node
// Universal structural & traceability validator for OpenSpec changes, archived changes, and durable test suites.
// Checks schema boundaries, x-deferred roadmap isolation, Living Document hygiene, relative links (no gitignored targets),
// 6-column Code Review table, WP↔tasks bindings, Main E2E YAML, global Case ID uniqueness, and 100% spec/SC coverage.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CASE_HEADING_RE = /^### ([A-Z][A-Z0-9]*-[A-Z0-9-]*\d{2,3}) (.+)$/gm;
const CASE_ID_TOKEN_RE = /\b[A-Z][A-Z0-9]*-[A-Z0-9-]*\d{2,3}\b/g;
const CASE_LEVEL_RE = /^(?:L[1-6]|LX)(?:[、/]\s*(?:L[1-6]|LX))*(?:（[^）]+）|\([^)]+\))?$/;
const CAPABILITY_NAME_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const ARCHIVE_DIR_NAME_RE = /^\d{4}-\d{2}-\d{2}-[a-z0-9]+(?:-[a-z0-9]+)*$/;
const ARCHIVE_SOURCE_PATH_RE =
  /^openspec\/changes\/archive\/(\d{4}-\d{2}-\d{2}-[a-z0-9]+(?:-[a-z0-9]+)*)\/tests\.md$/;
const DELTA_OP_TITLE_RE = /^(ADDED|MODIFIED|REMOVED|RENAMED) Requirements$/;
const CASE_FIELDS = ['用例级别', '操作步骤', '预期结果'];
const ALLOWED_SCHEMAS = new Set(['spec-driven']);
const ALLOWED_META_KEYS = new Set([
  'schema',
  'created',
  'skip_specs',
  'retire_capabilities',
  'x-deferred',
  'affected_areas',
  'initiative',
]);
const BOOLEAN_META_KEYS = new Set(['skip_specs', 'retire_capabilities', 'x-deferred']);
const EXTENSION_META_KEY_RE = /^x-\S+$/;
const IGNORED_LINK_PREFIX_RE =
  /^(?:testing\/runs|openspec\/testing\/runs|\.tmp|reports|test-evidence|\.worktrees|\.target-wt|target|app\/target|node_modules|dist)(?:\/|$)/;
// In-flight legacy change nearing closeout; exempt from newly introduced Living Document / extra-file / 6-col review / link checks until archived.
const LEGACY_HYGIENE_EXEMPT_CHANGES = new Set();
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

function walkMarkdownFiles(relDir, out = []) {
  if (!exists(relDir)) return out;
  for (const entry of fs.readdirSync(abs(relDir), {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
    const relPath = `${relDir}/${entry.name}`;
    if (entry.isDirectory()) {
      walkMarkdownFiles(relPath, out);
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      out.push(relPath);
    }
  }
  return out;
}

function parseArgs(argv) {
  const options = {change: null, deferred: false, help: false};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--all') options.change = null;
    else if (arg === '--deferred' || arg === '--spec-only') options.deferred = true;
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

function stripInlineCode(markdown) {
  return markdown.replace(/`+[^`\n]*`+/g, '');
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

function validateMarkdownRelativeLinks(relPath, check) {
  if (!exists(relPath)) return;
  const cleaned = stripInlineCode(stripCommentsAndFences(read(relPath)));
  const baseDir = path.dirname(abs(relPath));

  for (const match of cleaned.matchAll(/!?\[[^\]]*\]\(([^)\n]+)\)/g)) {
    const rawTarget = match[1].trim().replace(/^<|>$/g, '');
    const withoutTitle = rawTarget.split(/\s+['"]/)[0].trim();
    if (!withoutTitle || withoutTitle.startsWith('#')) continue;
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(withoutTitle)) continue;

    const cleanTarget = withoutTitle.split('#')[0].trim();
    if (!cleanTarget) continue;

    let decodedTarget = cleanTarget;
    try {
      decodedTarget = decodeURIComponent(cleanTarget);
    } catch {
      // Keep raw path if percent-decoding fails.
    }
    const resolved = path.resolve(baseDir, decodedTarget);
    const relTarget = path.relative(root, resolved).replaceAll('\\', '/');
    const isInsideRepo =
      relTarget !== '..' && !relTarget.startsWith('../') && !path.isAbsolute(relTarget);
    if (isInsideRepo && (IGNORED_LINK_PREFIX_RE.test(relTarget) || /\.log$/i.test(relTarget))) {
      check(
        false,
        `${relPath}: 禁止使用 Markdown 超链接指向 .gitignore 忽略路径 "${cleanTarget}"（请改用行内代码反引号路径）`,
      );
      continue;
    }

    check(
      fs.existsSync(resolved),
      `${relPath}: Markdown 相对链接目标不存在 -> "${cleanTarget}"`,
    );
  }
}

function validateOpenSpecConfig(check) {
  const configPath = 'openspec/config.yaml';
  check(exists(configPath), `缺少 ${configPath}`);
  const schemaPath = 'openspec/schemas/spec-driven/schema.yaml';
  check(exists(schemaPath), `缺少工作流 Schema: ${schemaPath}`);

  const roleDir = 'openspec/role';
  if (exists(roleDir)) {
    for (const entry of fs.readdirSync(abs(roleDir), {withFileTypes: true})) {
      if (entry.isFile() && entry.name.endsWith('.md')) {
        validateMarkdownRelativeLinks(`${roleDir}/${entry.name}`, check);
      }
    }
  }
  if (exists('openspec/GLOSSARY.md')) {
    validateMarkdownRelativeLinks('openspec/GLOSSARY.md', check);
  }

  if (!exists(configPath)) return;

  const lines = read(configPath).split('\n');
  let blockIndent = null;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const indent = line.match(/^ */)?.[0].length ?? 0;

    if (blockIndent !== null) {
      if (indent > blockIndent) continue;
      blockIndent = null;
    }
    if (/:\s*[|>][+-]?\s*$/.test(line)) {
      blockIndent = indent;
      continue;
    }

    const listMatch = line.match(/^\s*-\s+(.*)$/);
    if (listMatch) {
      const val = stripYamlInlineComment(listMatch[1]).trim();
      const isQuoted =
        (val.startsWith("'") && val.endsWith("'")) || (val.startsWith('"') && val.endsWith('"'));
      if (!isQuoted && /:\s/.test(val)) {
        check(
          false,
          `${configPath}:${i + 1}: 未加引号的 YAML 列表项包含 ": "（会导致 OpenSpec CLI 解析失败并静默忽略整个 config.yaml），请用单引号包裹该行`,
        );
      }
    }
  }
}

function validateRepoDocsLinks(check) {
  const topDocs = ['README.md', 'AGENTS.md', 'LESSONS.md'];
  for (const docPath of topDocs) {
    validateMarkdownRelativeLinks(docPath, check);
  }
  const exemptDocs = LEGACY_HYGIENE_EXEMPT_CHANGES.has('deliver-session-capacity')
    ? new Set([
        'docs/implementation/session-capacity.md',
        'docs/implementation/session-capacity-web.md',
      ])
    : new Set();
  for (const relDoc of walkMarkdownFiles('docs')) {
    if (exemptDocs.has(relDoc)) continue;
    validateMarkdownRelativeLinks(relDoc, check);
  }
}

function parseChangeMeta(changeDir, check) {
  const metaPath = `${changeDir}/.openspec.yaml`;
  check(exists(metaPath), `${changeDir}: 缺少 .openspec.yaml`);
  if (!exists(metaPath)) {
    return {schema: null, skipSpecs: false, retireCapabilities: false, deferred: false};
  }
  const raw = read(metaPath);
  const lines = raw.split('\n');
  const seenKeys = new Set();
  for (let i = 0; i < lines.length; i += 1) {
    const stripped = stripYamlInlineComment(lines[i]);
    const trimmed = stripped.trim();
    if (!trimmed) continue;
    if (/^\s+/.test(stripped)) continue;
    const kv = stripped.match(/^([A-Za-z_][A-Za-z0-9_-]*):\s*(.*)$/);
    check(Boolean(kv), `${metaPath}:${i + 1}: 无法解析顶层元数据行 "${trimmed}"`);
    if (!kv) continue;
    const [, key, rawVal] = kv;
    const isDeferredTypo = /^x-defer/i.test(key) && key !== 'x-deferred';
    const isValidKey =
      !isDeferredTypo && (ALLOWED_META_KEYS.has(key) || EXTENSION_META_KEY_RE.test(key));
    check(
      isValidKey,
      `${metaPath}:${i + 1}: 未知或拼写错误的元数据键 "${key}"（仅允许 ${[...ALLOWED_META_KEYS].join(', ')} 或合法小写 x-* 扩展键）`,
    );
    check(!seenKeys.has(key), `${metaPath}:${i + 1}: 重复元数据键 "${key}"`);
    seenKeys.add(key);
    if (BOOLEAN_META_KEYS.has(key)) {
      const val = rawVal.trim();
      check(
        val === 'true' || val === 'false',
        `${metaPath}:${i + 1}: ${key} 必须为布尔值 true 或 false（当前为 "${val}"）`,
      );
    }
  }

  const schemaMatch = raw.match(/^schema:\s*(\S+)\s*$/m);
  const schema = schemaMatch?.[1]?.trim() ?? '';
  check(
    ALLOWED_SCHEMAS.has(schema),
    `${metaPath}: schema 必须显式为 spec-driven（当前为 "${schema || '缺失'}"）`,
  );
  const skipSpecs = /^skip_specs:\s*true\s*$/m.test(raw);
  const retireCapabilities = /^retire_capabilities:\s*true\s*$/m.test(raw);
  const deferred = /^x-deferred:\s*true\s*$/m.test(raw);
  return {
    schema: ALLOWED_SCHEMAS.has(schema) ? schema : null,
    skipSpecs,
    retireCapabilities,
    deferred,
  };
}

function validateLivingDocumentMarkdown(relPath, check, allowDeltaH2Start = false) {
  if (!exists(relPath)) return;
  const raw = read(relPath);
  const withoutComments = stripComments(raw).trimStart();
  const validStart = allowDeltaH2Start
    ? /^(?:# \S|## (?:Purpose|(?:ADDED|MODIFIED|REMOVED|RENAMED) Requirements)\b)/.test(
        withoutComments,
      )
    : /^# \S/.test(withoutComments);
  check(
    validStart,
    allowDeltaH2Start
      ? `${relPath}: 增量规格必须以 "# <标题>" 或 "## Purpose / ## ADDED|MODIFIED|REMOVED|RENAMED Requirements" 起始（不得在顶部堆叠修订横幅）`
      : `${relPath}: Living Document 必须以一级标题 "# <标题>" 起始（不得在标题上方堆叠修订横幅或说明）`,
  );

  const cleaned = stripCommentsAndFences(raw);
  const seenH2 = new Set();
  for (const match of cleaned.matchAll(/^## (.+)$/gm)) {
    const heading = match[1].trim();
    check(
      !seenH2.has(heading),
      `${relPath}: 存在重复二级标题 "## ${heading}"（请原地合并章节内容，避免堆叠多轮补丁小节）`,
    );
    seenH2.add(heading);
  }
}

function validateChangeDirectoryHygiene(changeName, changeDir, deferred, skipSpecs, check) {
  if (LEGACY_HYGIENE_EXEMPT_CHANGES.has(changeName)) return;

  const allowedFiles = deferred
    ? new Set(['.openspec.yaml', 'proposal.md', 'design.md'])
    : new Set(['.openspec.yaml', 'proposal.md', 'design.md', 'tests.md', 'plan.md', 'tasks.md']);
  const entries = fs.readdirSync(abs(changeDir), {withFileTypes: true});
  for (const entry of entries) {
    if (entry.isDirectory()) {
      check(
        entry.name === 'specs' && !skipSpecs,
        `${changeDir}/${entry.name}: 变更目录根下仅允许 specs/ 子目录（skip_specs: true 时不允许 specs/）`,
      );
    } else if (entry.isFile()) {
      check(
        allowedFiles.has(entry.name),
        `${changeDir}/${entry.name}: 不允许在变更目录根下放置非 Schema 文件（契约请写入 design.md 子章节或 docs/implementation/，过程与审查记录请写入 plan.md 或 testing/runs/）`,
      );
    }
  }

  const mdArtifacts = deferred
    ? ['proposal.md', 'design.md']
    : ['proposal.md', 'design.md', 'tests.md', 'plan.md', 'tasks.md'];
  for (const name of mdArtifacts) {
    const relArtifact = `${changeDir}/${name}`;
    validateLivingDocumentMarkdown(relArtifact, check);
    validateMarkdownRelativeLinks(relArtifact, check);
  }
}

function parseSpecScenarios(specText, specPath, check, deltaOnly = false, deltaContext = null) {
  const cleaned = stripCommentsAndFences(specText);
  const scenarioSet = new Set();
  const byRequirement = new Map();
  let hasRemovedSection = false;
  const removedRequirements = [];

  const extractReqNames = blockText =>
    [...blockText.matchAll(/^### Requirement:\s*(.+)$/gm)].map(m => m[1].trim());

  const parseBlock = (blockText, activeForCoverage, requireScenario = true) => {
    const reqs = [...blockText.matchAll(/^### Requirement:\s*(.+)$/gm)];
    for (let i = 0; i < reqs.length; i += 1) {
      const reqName = reqs[i][1].trim();
      const sub = blockText.slice(reqs[i].index, reqs[i + 1]?.index ?? blockText.length);
      const scenarios = [...sub.matchAll(/^#### Scenario:\s*(.+)$/gm)].map(m => m[1].trim());
      if (requireScenario) {
        check(scenarios.length > 0, `${specPath} Requirement "${reqName}": 缺少 #### Scenario:`);
      }
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
    const blocks = h2.map((m, i) => ({
      title: m[1].trim(),
      body: cleaned.slice(m.index, h2[i + 1]?.index ?? cleaned.length),
      innerBody: cleaned.slice(m.index + m[0].length, h2[i + 1]?.index ?? cleaned.length).trim(),
    }));

    // First pass: collect all RENAMED FROM->TO declarations regardless of H2 ordering.
    const renamedPairs = [];
    const renamedToSet = new Set();
    for (const blk of blocks) {
      if (blk.title !== 'RENAMED Requirements') continue;
      const fromReqs = [
        ...blk.innerBody.matchAll(/^(?:-\s*)?FROM:\s*`?### Requirement:\s*(.+?)`?\s*$/gm),
      ].map(m => m[1].trim());
      const toReqs = [
        ...blk.innerBody.matchAll(/^(?:-\s*)?TO:\s*`?### Requirement:\s*(.+?)`?\s*$/gm),
      ].map(m => m[1].trim());
      const len = Math.min(fromReqs.length, toReqs.length);
      for (let i = 0; i < len; i += 1) {
        renamedPairs.push({from: fromReqs[i], to: toReqs[i]});
        renamedToSet.add(toReqs[i]);
      }
    }

    let deltaOpCount = 0;
    let hasPurpose = false;
    let nonEmptyPurpose = false;
    for (const blk of blocks) {
      const {title, body, innerBody} = blk;
      const isDeltaOp = DELTA_OP_TITLE_RE.test(title);
      check(
        title === 'Purpose' || isDeltaOp,
        `${specPath}: 非法增量规格二级标题 "## ${title}"（仅允许 Purpose 与 ADDED/MODIFIED/REMOVED/RENAMED Requirements）`,
      );
      if (title === 'Purpose') {
        hasPurpose = true;
        nonEmptyPurpose = innerBody.length > 0;
      }
      if (isDeltaOp) deltaOpCount += 1;

      if (title === 'ADDED Requirements') {
        check(/^### Requirement:/m.test(body), `${specPath} "## ${title}": 缺少 ### Requirement:`);
        parseBlock(body, true, true);
        if (deltaContext?.baseReqSet) {
          for (const reqName of extractReqNames(body)) {
            check(
              !deltaContext.baseReqSet.has(reqName),
              `${specPath} "## ADDED Requirements": Requirement "${reqName}" 已存在于长期规格 ${deltaContext.longTermSpec}（修改既有需求请使用 ## MODIFIED Requirements）`,
            );
          }
        }
      } else if (title === 'MODIFIED Requirements') {
        check(
          !deltaContext?.isNewCapability,
          `${specPath}: 新增能力增量规格不得使用 "## MODIFIED Requirements"`,
        );
        check(/^### Requirement:/m.test(body), `${specPath} "## ${title}": 缺少 ### Requirement:`);
        parseBlock(body, true, true);
        if (deltaContext?.baseReqSet) {
          for (const reqName of extractReqNames(body)) {
            check(
              deltaContext.baseReqSet.has(reqName) || renamedToSet.has(reqName),
              `${specPath} "## MODIFIED Requirements": Requirement "${reqName}" 在长期规格 ${deltaContext.longTermSpec} 中不存在（请核对需求标题是否完全一致）`,
            );
          }
        }
      } else if (title === 'REMOVED Requirements') {
        hasRemovedSection = true;
        check(
          !deltaContext?.isNewCapability,
          `${specPath}: 新增能力增量规格不得使用 "## REMOVED Requirements"`,
        );
        check(/^### Requirement:/m.test(body), `${specPath} "## ${title}": 缺少 ### Requirement:`);
        const reqs = [...body.matchAll(/^### Requirement:\s*(.+)$/gm)];
        for (let i = 0; i < reqs.length; i += 1) {
          const reqName = reqs[i][1].trim();
          removedRequirements.push(reqName);
          const sub = body.slice(reqs[i].index, reqs[i + 1]?.index ?? body.length);
          const scenarios = [...sub.matchAll(/^#### Scenario:\s*(.+)$/gm)].map(m => m[1].trim());
          // Allow tests.md regression cases to reference removed requirements without forcing mandatory coverage.
          const allowedSet = byRequirement.get(reqName) ?? new Set();
          allowedSet.add('REMOVED');
          allowedSet.add('（已移除）');
          allowedSet.add('已移除');
          for (const sc of scenarios) allowedSet.add(sc);
          byRequirement.set(reqName, allowedSet);
        }
        if (deltaContext?.baseReqSet) {
          for (const reqName of extractReqNames(body)) {
            check(
              deltaContext.baseReqSet.has(reqName),
              `${specPath} "## REMOVED Requirements": Requirement "${reqName}" 在长期规格 ${deltaContext.longTermSpec} 中不存在`,
            );
          }
        }
      } else if (title === 'RENAMED Requirements') {
        check(
          !deltaContext?.isNewCapability,
          `${specPath}: 新增能力增量规格不得使用 "## RENAMED Requirements"`,
        );
        const fromReqs = [
          ...innerBody.matchAll(/^(?:-\s*)?FROM:\s*`?### Requirement:\s*(.+?)`?\s*$/gm),
        ].map(m => m[1].trim());
        const toReqs = [
          ...innerBody.matchAll(/^(?:-\s*)?TO:\s*`?### Requirement:\s*(.+?)`?\s*$/gm),
        ].map(m => m[1].trim());
        check(
          fromReqs.length > 0 && fromReqs.length === toReqs.length,
          `${specPath} "## RENAMED Requirements": 必须包含成对的 FROM/TO Requirement 重命名声明`,
        );
        if (deltaContext?.baseReqSet) {
          for (const oldName of fromReqs) {
            check(
              deltaContext.baseReqSet.has(oldName),
              `${specPath} "## RENAMED Requirements": FROM Requirement "${oldName}" 在长期规格 ${deltaContext.longTermSpec} 中不存在`,
            );
          }
          for (const newName of toReqs) {
            check(
              !deltaContext.baseReqSet.has(newName),
              `${specPath} "## RENAMED Requirements": TO Requirement "${newName}" 已存在于长期规格 ${deltaContext.longTermSpec}`,
            );
          }
        }
        for (const pair of renamedPairs) {
          if (!byRequirement.has(pair.to)) {
            const inherited = new Set(['RENAMED', '（已重命名）', '已重命名']);
            const baseScenarios = deltaContext?.baseByRequirement?.get(pair.from);
            if (baseScenarios) {
              for (const sc of baseScenarios) inherited.add(sc);
            }
            byRequirement.set(pair.to, inherited);
          }
        }
      }
    }
    if (deltaContext) {
      if (deltaContext.isNewCapability) {
        check(
          hasPurpose && nonEmptyPurpose,
          `${specPath}: 新增能力增量规格必须包含非空 "## Purpose" 章节`,
        );
      } else {
        check(
          !hasPurpose,
          `${specPath}: 既有能力增量规格不得包含 "## Purpose" 章节（仅新增能力需要）`,
        );
      }
    }
    check(
      deltaOpCount > 0,
      `${specPath}: 增量规格必须至少包含一个 ## ADDED/MODIFIED/REMOVED/RENAMED Requirements 章节`,
    );
  } else {
    validateLivingDocumentMarkdown(specPath, check);
    const sections = extractH2Sections(specText, false);
    check(Boolean(sections.get('Purpose')), `${specPath}: 缺少或为空的章节 "## Purpose"`);
    check(Boolean(sections.get('Requirements')), `${specPath}: 缺少或为空的章节 "## Requirements"`);
    for (const heading of sections.keys()) {
      check(
        !DELTA_OP_TITLE_RE.test(heading),
        `${specPath}: 长期规格不得残留增量标题 "## ${heading}"`,
      );
    }
    check(/^### Requirement:/m.test(cleaned), `${specPath}: 缺少 ### Requirement:`);
    parseBlock(cleaned, true, true);
  }
  return {scenarioSet, byRequirement, hasRemovedSection, removedRequirements};
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

function collectDurableCaseOwners() {
  const owners = new Map();
  const suitesDir = 'testing/suites';
  if (!exists(suitesDir)) return owners;
  for (const entry of fs.readdirSync(abs(suitesDir), {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
    if (!entry.isDirectory()) continue;
    const suitePath = `${suitesDir}/${entry.name}/tests.md`;
    if (!exists(suitePath)) continue;
    for (const item of parseCases(read(suitePath), suitePath, () => {})) {
      if (!owners.has(item.id)) owners.set(item.id, entry.name);
    }
  }
  return owners;
}

function collectOtherActiveCaseOwners(targetChange) {
  const activeOwners = new Map();
  const changesRoot = 'openspec/changes';
  if (!exists(changesRoot)) return activeOwners;
  for (const entry of fs.readdirSync(abs(changesRoot), {withFileTypes: true}).sort((a, b) => a.name.localeCompare(b.name))) {
    if (!entry.isDirectory() || entry.name === 'archive' || entry.name === targetChange) continue;
    const testsPath = `${changesRoot}/${entry.name}/tests.md`;
    if (!exists(testsPath)) continue;
    for (const item of parseCases(read(testsPath), testsPath, () => {})) {
      if (!activeOwners.has(item.id)) activeOwners.set(item.id, entry.name);
    }
  }
  return activeOwners;
}

function validateChange(changeName, explicitMode, check, stats, collisionContext) {
  const changeDir = `openspec/changes/${changeName}`;
  check(exists(changeDir), `变更目录不存在: ${changeDir}`);
  if (!exists(changeDir)) return;

  const {schema, skipSpecs, retireCapabilities, deferred} = parseChangeMeta(changeDir, check);
  if (!schema) return;

  const proposalPath = `${changeDir}/proposal.md`;
  const designPath = `${changeDir}/design.md`;
  check(exists(proposalPath), `${changeDir}: 缺少 proposal.md`);
  check(exists(designPath), `${changeDir}: 缺少 design.md`);
  if (!exists(proposalPath)) return;

  validateChangeDirectoryHygiene(changeName, changeDir, deferred, skipSpecs, check);

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

  const deltaSpecMap = new Map();
  const allDeltaScenarios = new Set();
  let modCapSet = new Set();
  let hasAnyRemovedSection = false;

  if (skipSpecs) {
    check(
      !deferred,
      `${changeDir}/.openspec.yaml: x-deferred: true 远期变更不得设置 skip_specs: true`,
    );
    check(
      !retireCapabilities,
      `${changeDir}/.openspec.yaml: skip_specs: true 变更不得设置 retire_capabilities: true`,
    );
    check(
      !exists(`${changeDir}/specs`),
      `${changeDir}: skip_specs: true 时不得同时存在 ${changeDir}/specs 目录`,
    );
  } else if (proposalSections.get('Capabilities')) {
    const capSubsections = extractSubsections(proposalSections.get('Capabilities'), '###');
    const newCaps = parseCapabilityBullets(capSubsections.get('New Capabilities') ?? '');
    const modCaps = parseCapabilityBullets(capSubsections.get('Modified Capabilities') ?? '');
    const newCapSet = new Set(newCaps);
    modCapSet = new Set(modCaps);
    const declaredCaps = new Set([...newCaps, ...modCaps]);
    check(declaredCaps.size > 0, `${proposalPath}: Capabilities 未声明任何有效扁平 kebab-case 能力路径`);

    for (const cap of newCapSet) {
      check(
        !modCapSet.has(cap),
        `${proposalPath}: 能力 "${cap}" 不能同时声明在 New Capabilities 与 Modified Capabilities 中`,
      );
    }

    for (const cap of newCaps) {
      const relSpec = `specs/${cap}/spec.md`;
      const fullSpec = `${changeDir}/${relSpec}`;
      const longTermSpec = `openspec/specs/${cap}/spec.md`;
      check(exists(fullSpec), `${proposalPath}: New Capability "${cap}" 缺少 ${fullSpec}`);
      check(
        !exists(longTermSpec),
        `${proposalPath}: New Capability "${cap}" 已存在于长期规格 ${longTermSpec}（修改既有能力请归入 Modified Capabilities）`,
      );
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
    check(exists(specsRoot), `${changeDir}: 缺少 specs/ 目录`);
    if (exists(specsRoot)) {
      const entries = fs.readdirSync(abs(specsRoot), {withFileTypes: true});
      for (const entry of entries) {
        check(entry.isDirectory(), `${specsRoot}/${entry.name}: specs/ 下只允许单层能力目录`);
        if (!entry.isDirectory()) continue;
        const diskCap = entry.name;
        check(
          CAPABILITY_NAME_RE.test(diskCap),
          `${specsRoot}/${diskCap}: 能力目录名必须为扁平单层 kebab-case`,
        );
        const capEntries = fs.readdirSync(abs(`${specsRoot}/${diskCap}`), {withFileTypes: true});
        for (const subEntry of capEntries) {
          check(
            subEntry.isFile() && subEntry.name === 'spec.md',
            `${specsRoot}/${diskCap}/${subEntry.name}: 能力目录下只允许 spec.md，禁止多级嵌套或其他文件`,
          );
        }

        const relSpec = `specs/${diskCap}/spec.md`;
        const fullSpec = `${changeDir}/${relSpec}`;
        check(declaredCaps.has(diskCap), `${fullSpec}: 未在 proposal.md Capabilities 中声明`);
        if (exists(fullSpec)) {
          if (!LEGACY_HYGIENE_EXEMPT_CHANGES.has(changeName)) {
            validateLivingDocumentMarkdown(fullSpec, check, true);
            validateMarkdownRelativeLinks(fullSpec, check);
          }
          const isNewCapability = newCapSet.has(diskCap);
          const longTermSpec = `openspec/specs/${diskCap}/spec.md`;
          const baseParsed =
            !isNewCapability && exists(longTermSpec)
              ? parseSpecScenarios(read(longTermSpec), longTermSpec, () => {}, false)
              : null;
          const baseReqSet = baseParsed ? new Set(baseParsed.byRequirement.keys()) : null;
          const deltaContext = declaredCaps.has(diskCap)
            ? {
                isNewCapability,
                longTermSpec,
                baseReqSet,
                baseByRequirement: baseParsed?.byRequirement ?? null,
              }
            : null;
          const parsed = parseSpecScenarios(read(fullSpec), relSpec, check, true, deltaContext);
          if (parsed.hasRemovedSection) hasAnyRemovedSection = true;
          deltaSpecMap.set(relSpec, parsed.byRequirement);
          for (const key of parsed.scenarioSet) allDeltaScenarios.add(key);
        }
      }
    }

    if (retireCapabilities) {
      check(
        hasAnyRemovedSection,
        `${changeDir}/.openspec.yaml: 声明了 retire_capabilities: true，但增量规格中不包含任何 ## REMOVED Requirements`,
      );
    }
  }

  const implFiles = ['tests.md', 'plan.md', 'tasks.md'];
  if (deferred) {
    check(
      !retireCapabilities,
      `${changeDir}/.openspec.yaml: x-deferred: true 远期变更不得提前设置 retire_capabilities: true`,
    );
    for (const name of implFiles) {
      check(
        !exists(`${changeDir}/${name}`),
        `${changeDir}/${name}: x-deferred: true 远期变更不得包含施工产物（正式开工请先移除 .openspec.yaml 中的 x-deferred: true）`,
      );
    }
    if (explicitMode === 'impl') {
      check(
        false,
        `${changeDir}: 当前标记为 x-deferred: true 远期规划态，不能进入实施门禁；获准开工时请先移除 .openspec.yaml 中的 x-deferred: true 并补齐 tests.md、plan.md、tasks.md（若仅校验规划定义请加 --deferred）`,
      );
      return;
    }
    stats.deferredChanges += 1;
    return;
  }

  if (explicitMode === 'deferred') {
    stats.deferredChanges += 1;
    return;
  }

  stats.implChanges += 1;
  const presentImpl = implFiles.filter(name => exists(`${changeDir}/${name}`));
  for (const name of implFiles) {
    check(
      exists(`${changeDir}/${name}`),
      `${changeDir}: 未标记 x-deferred: true 的变更必须具备完整施工产物 tests.md、plan.md、tasks.md（当前缺少 ${name}；若属未排期远期规划请在 .openspec.yaml 添加 x-deferred: true）`,
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

  if (!LEGACY_HYGIENE_EXEMPT_CHANGES.has(changeName)) {
    const reviewTables = parseMarkdownTables(planSections.get('Code Review') ?? '');
    const reviewTable = reviewTables.find(
      t => t.length >= 4 && t[0].length === 6 && isSeparatorRow(t[1]),
    );
    check(
      Boolean(reviewTable),
      `${planPath}: "## Code Review" 必须包含 6 列 Markdown 表（含编码前门禁与集成交付门禁两行）`,
    );
    if (reviewTable) {
      for (const row of reviewTable.slice(2)) {
        check(row.length === 6, `${planPath}: "## Code Review" 行列数不为 6 (${row[0] ?? ''})`);
        const statusVal = (row[4] ?? '').trim();
        const snapshotVal = (row[5] ?? '').trim();
        check(
          /^(?:pending|passed|blocked)\b/i.test(statusVal),
          `${planPath}: "## Code Review" 第 5 列 Status 必须以 pending / passed / blocked 开头（当前为 "${statusVal}"）`,
        );
        check(
          snapshotVal.length > 0,
          `${planPath}: "## Code Review" 第 6 列 Reviewed Snapshot / Notes 不能为空`,
        );
      }
    }
  }

  const mainE2eSection = planSectionsWithFences.get('Main E2E') ?? '';
  const yamlBlocks = [...mainE2eSection.matchAll(/^```yaml[ \t]*\n([\s\S]*?)^```[ \t]*$/gm)];
  check(yamlBlocks.length === 1, `${planPath}: "## Main E2E" 必须包含且仅包含 1 个 \`\`\`yaml 代码块`);
  if (yamlBlocks.length === 1) {
    parseMainE2eYaml(yamlBlocks[0][1], planPath, check);
  }

  // 2. Validate tasks.md <-> plan.md bidirectional WP binding
  const tasksPath = `${changeDir}/tasks.md`;
  const tasksRaw = read(tasksPath);
  const tasksSections = extractH2Sections(tasksRaw, false);
  const followUpSection = tasksSections.get('Workflow follow-up') ?? '';
  check(
    !/^- \[[ xX]\] /m.test(followUpSection),
    `${tasksPath}: "## Workflow follow-up" 不得包含任何 "- [ ]" 或 "- [x]" checkbox（请使用普通列表）`,
  );

  const tasksCleaned = stripCommentsAndFences(tasksRaw);
  const followUpIdx = tasksCleaned.search(/^## Workflow follow-up\b/m);
  const trackedTasksText = followUpIdx >= 0 ? tasksCleaned.slice(0, followUpIdx) : tasksCleaned;

  const checkboxes = [...trackedTasksText.matchAll(/^- \[[ xX]\] (.+)$/gm)].map(m => m[1].trim());
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

  // 3. Validate tests.md, global Case ID collisions, and 100% delta Requirement/Scenario (or Success Criteria) coverage
  const testsPath = `${changeDir}/tests.md`;
  const testsRaw = read(testsPath);
  const parsedCases = parseCases(testsRaw, testsPath, check);
  check(parsedCases.length > 0, `${testsPath}: 至少定义一个验收用例`);
  const caseIdSet = new Set(parsedCases.map(item => item.id));
  stats.activeChangeCases += parsedCases.length;

  if (collisionContext) {
    const {durableCaseOwners, otherActiveOwners} = collisionContext;
    for (const item of parsedCases) {
      const otherChange = otherActiveOwners.get(item.id);
      check(
        !otherChange,
        `${testsPath} ${item.id}: 稳定用例 ID "${item.id}" 与其他活跃变更 "${otherChange}" 冲突`,
      );
      const durableOwner = durableCaseOwners.get(item.id);
      if (durableOwner && !skipSpecs) {
        check(
          modCapSet.has(durableOwner),
          `${testsPath} ${item.id}: 稳定用例 ID "${item.id}" 已被既有长期套件 ${durableOwner} 占用（仅当在 Modified Capabilities 中声明修改 ${durableOwner} 或 skip_specs: true 时允许复用既有 ID）`,
        );
      }
    }
  }

  const tables = parseMarkdownTables(stripCommentsAndFences(testsRaw));
  if (skipSpecs) {
    const scSection = proposalSections.get('Success Criteria') ?? '';
    const declaredScSet = new Set([...scSection.matchAll(/\b(SC-?\d+)\b/g)].map(m => m[1]));
    check(
      declaredScSet.size > 0,
      `${proposalPath}: skip_specs: true 变更必须在 "## Success Criteria" 中定义稳定判据编号（如 SC1、SC2）`,
    );
    const coveredScSet = new Set();
    let scMappedRows = 0;
    for (const table of tables) {
      if (table.length < 3 || !isSeparatorRow(table[1])) continue;
      for (const row of table.slice(2)) {
        if (
          row.length === 4 &&
          row[0].replace(/^`|`$/g, '').trim() === 'proposal.md' &&
          row[1].trim() === 'Success Criteria'
        ) {
          const scTokens = row[2]
            .split('、')
            .map(s => s.replace(/^`|`$/g, '').trim())
            .filter(Boolean);
          const ids = [...row[3].matchAll(CASE_ID_TOKEN_RE)].map(m => m[0]);
          check(scTokens.length > 0, `${testsPath}: Success Criteria 关联行缺少判据标识`);
          check(ids.length > 0, `${testsPath}: Success Criteria "${row[2].trim()}" 关联行缺少用例 ID`);
          for (const id of ids) {
            check(caseIdSet.has(id), `${testsPath}: Success Criteria 关联引用了不存在的用例 ID "${id}"`);
          }
          for (const scToken of scTokens) {
            if (declaredScSet.size > 0) {
              check(
                declaredScSet.has(scToken),
                `${testsPath}: Success Criteria 映射了 proposal.md 中不存在的判据 "${scToken}"`,
              );
            }
            if (ids.length > 0) coveredScSet.add(scToken);
          }
          if (scTokens.length > 0 && ids.length > 0) scMappedRows += 1;
        }
      }
    }
    check(
      scMappedRows > 0,
      `${testsPath}: skip_specs: true 变更必须包含四列映射行 "| proposal.md | Success Criteria | SCn | 用例 ID |"，以关联成功判据`,
    );
    for (const scToken of declaredScSet) {
      check(
        coveredScSet.has(scToken),
        `${testsPath}: proposal.md Success Criteria "${scToken}" 缺少验收用例映射`,
      );
    }
  } else {
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

    const isExemptLegacy = LEGACY_HYGIENE_EXEMPT_CHANGES.has(changeName);
    const coveredDelta = new Set();
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
            if (!isExemptLegacy) {
              check(
                false,
                `${testsPath}: 非豁免变更必须使用标准四列需求关联表 "| specs/<capability>/spec.md | Requirement | Scenario | 用例 |"，不再允许旧两列斜杠格式`,
              );
              continue;
            }
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
            if (!isExemptLegacy) {
              check(
                false,
                `${testsPath}: 非豁免变更必须使用标准四列需求关联表 "| specs/<capability>/spec.md | Requirement | Scenario | 用例 |"，不再允许旧两列格式`,
              );
              continue;
            }
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

    for (const key of allDeltaScenarios) {
      if (!coveredDelta.has(key)) {
        check(
          false,
          `${testsPath}: 增量规格场景缺少验收用例映射 -> ${key.replaceAll('\0', ' / ')}`,
        );
      }
    }
  }
}

function validateArchivedChanges(check, stats) {
  const archiveRoot = 'openspec/changes/archive';
  if (!exists(archiveRoot)) return;

  const entries = fs
    .readdirSync(abs(archiveRoot), {withFileTypes: true})
    .sort((a, b) => a.name.localeCompare(b.name));
  for (const entry of entries) {
    const archiveDir = `${archiveRoot}/${entry.name}`;
    check(entry.isDirectory(), `${archiveDir}: archive/ 下只允许归档变更目录`);
    if (!entry.isDirectory()) continue;

    check(
      ARCHIVE_DIR_NAME_RE.test(entry.name),
      `${archiveDir}: 归档目录名必须满足 YYYY-MM-DD-<change-name> 格式`,
    );

    const {schema, deferred} = parseChangeMeta(archiveDir, check);
    check(
      schema === 'spec-driven' && !deferred,
      `${archiveDir}/.openspec.yaml: 严禁归档带 x-deferred: true 的远期规划变更（已归档变更必须为 schema: spec-driven 且无 x-deferred: true）`,
    );

    for (const requiredFile of ['proposal.md', 'design.md', 'tests.md', 'plan.md', 'tasks.md']) {
      check(
        exists(`${archiveDir}/${requiredFile}`),
        `${archiveDir}: 已归档变更缺少核心产物 ${requiredFile}`,
      );
    }

    const tasksPath = `${archiveDir}/tasks.md`;
    if (exists(tasksPath)) {
      const cleanedTasks = stripCommentsAndFences(read(tasksPath));
      check(
        !/^- \[ \] /m.test(cleanedTasks),
        `${tasksPath}: 已归档变更不得保留未勾选的 "- [ ]" 任务`,
      );
    }

    for (const relMd of walkMarkdownFiles(archiveDir)) {
      validateMarkdownRelativeLinks(relMd, check);
    }
    stats.archivedChanges += 1;
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
      paths.push(token);
    }
  }
  return paths;
}

function validateDurableSuites(check, stats) {
  const suitesDir = 'testing/suites';
  const specsDir = 'openspec/specs';
  const durableSpecs = new Map();

  if (exists(specsDir)) {
    const specEntries = fs.readdirSync(abs(specsDir), {withFileTypes: true});
    for (const entry of specEntries) {
      check(entry.isDirectory(), `${specsDir}/${entry.name}: openspec/specs/ 下只允许单层能力目录`);
      if (!entry.isDirectory()) continue;
      const capability = entry.name;
      check(
        CAPABILITY_NAME_RE.test(capability),
        `${specsDir}/${capability}: 长期能力目录名必须为扁平单层 kebab-case`,
      );
      const specPath = `${specsDir}/${capability}/spec.md`;
      const suitePath = `${suitesDir}/${capability}/tests.md`;
      check(exists(specPath), `长期能力目录缺少规格文件: ${specPath}`);
      if (exists(specPath)) {
        validateMarkdownRelativeLinks(specPath, check);
        durableSpecs.set(capability, parseSpecScenarios(read(specPath), specPath, check, false));
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
  const suiteTexts = new Map();
  const capabilityDirs = fs
    .readdirSync(abs(suitesDir), {withFileTypes: true})
    .filter(d => d.isDirectory())
    .map(d => d.name)
    .sort();

  // Pass 1: Validate suite files, archive Source definitions ordering, and collect global stable Case IDs.
  for (const capability of capabilityDirs) {
    check(
      CAPABILITY_NAME_RE.test(capability),
      `${suitesDir}/${capability}: 长期套件目录名必须为扁平单层 kebab-case`,
    );
    const suitePath = `${suitesDir}/${capability}/tests.md`;
    const specPath = `openspec/specs/${capability}/spec.md`;
    check(exists(suitePath), `长期套件缺少文件: ${suitePath}`);
    check(exists(specPath), `长期套件对应规格不存在: ${specPath}`);
    if (!exists(suitePath)) continue;

    validateLivingDocumentMarkdown(suitePath, check);
    validateMarkdownRelativeLinks(suitePath, check);

    const suiteText = read(suitePath);
    suiteTexts.set(capability, suiteText);
    check(suiteText.includes(specPath), `${suitePath}: 缺少长期规格引用 "${specPath}"`);

    const suiteCases = parseCases(suiteText, suitePath, check);
    check(suiteCases.length > 0, `${suitePath}: 长期套件至少包含一个用例`);
    stats.durableSuites += 1;
    stats.durableCases += suiteCases.length;

    for (const item of suiteCases) {
      const prev = globalCaseOwner.get(item.id);
      check(
        !prev,
        `长期套件用例 ID "${item.id}" 同时在 ${prev} 与 ${capability} 中重复定义正文（跨能力共用用例应在单一主能力套件定义正文，其他套件仅在 ## 需求关联 表按 ID 引用）`,
      );
      globalCaseOwner.set(item.id, capability);
    }

    const declaredStable = suiteText.match(/^- Stable IDs:\s*(.+)$/m);
    check(Boolean(declaredStable), `${suitePath}: 缺少 "- Stable IDs:" 稳定用例编号声明行`);
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
      let prevArchiveDir = null;
      const mergedArchiveCases = new Map();
      const perSourceIds = new Map();
      for (const srcPath of srcPaths) {
        const arcMatch = srcPath.match(ARCHIVE_SOURCE_PATH_RE);
        check(
          Boolean(arcMatch),
          `${suitePath}: Source definitions 路径格式必须为 openspec/changes/archive/YYYY-MM-DD-<name>/tests.md（当前为 "${srcPath}"）`,
        );
        if (arcMatch) {
          const arcDir = arcMatch[1];
          if (prevArchiveDir !== null) {
            check(
              prevArchiveDir.localeCompare(arcDir) < 0,
              `${suitePath}: Source definitions 必须按归档目录日期严格升序排列且不得重复（"${prevArchiveDir}" 不应在 "${arcDir}" 之前或重复）`,
            );
          }
          prevArchiveDir = arcDir;
        }
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

      for (const item of suiteCases) {
        const resolved = mergedArchiveCases.get(item.id);
        check(
          Boolean(resolved),
          `${suitePath} ${item.id}: 未在声明的 Source definitions (${srcPaths.join('、')}) 中找到`,
        );
        if (resolved) {
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
        check(
          hasAny,
          `${suitePath}: 声明的 Source definitions "${srcPath}" 未向本套件提供任何本地 ### <ID> 用例正文（若仅在 ## 需求关联 跨套件按 ID 引用其他主套件用例，请勿追加 Source definitions）`,
        );
      }
    }
  }

  // Pass 2: Validate 4-column "## 需求关联" table in every durable suite against openspec/specs/<capability>/spec.md (100% scenario coverage).
  const globalCaseIdSet = new Set(globalCaseOwner.keys());
  for (const capability of capabilityDirs) {
    const suiteText = suiteTexts.get(capability);
    const parsedSpec = durableSpecs.get(capability);
    if (!suiteText || !parsedSpec) continue;

    const suitePath = `${suitesDir}/${capability}/tests.md`;
    const expectedSpecPath = `openspec/specs/${capability}/spec.md`;
    stats.durableTotalScenarios += parsedSpec.scenarioSet.size;

    const traceSection = extractH2Sections(suiteText).get('需求关联');
    check(
      Boolean(traceSection),
      `${suitePath}: 缺少 "## 需求关联" 章节（须包含四列映射表覆盖 ${expectedSpecPath} 的全部 Requirement 与 Scenario）`,
    );
    if (!traceSection) continue;

    const coveredDurable = new Set();
    const tables = parseMarkdownTables(traceSection);
    const mappingTables = tables.filter(t => t.length >= 3 && t[0].length === 4 && isSeparatorRow(t[1]));
    check(
      mappingTables.length > 0,
      `${suitePath}: "## 需求关联" 必须包含四列 Markdown 映射表（| 长期规格路径 | Requirement | Scenario | 用例 |）`,
    );

    for (const table of mappingTables) {
      for (const row of table.slice(2)) {
        check(row.length === 4, `${suitePath}: "## 需求关联" 行列数不为 4 (${row[0] ?? ''})`);
        if (row.length !== 4) continue;

        const specRef = row[0].replace(/^`|`$/g, '').trim();
        const reqName = row[1].trim();
        const rawScenario = row[2].trim();
        const rawCaseCell = row[3].trim();

        check(
          specRef === expectedSpecPath,
          `${suitePath}: "## 需求关联" 第一列必须指向本能力长期规格 "${expectedSpecPath}"（当前为 "${specRef}"）`,
        );

        const ids = [...rawCaseCell.matchAll(CASE_ID_TOKEN_RE)].map(m => m[0]);
        check(
          ids.length > 0,
          `${suitePath}: "## 需求关联" 行缺少稳定用例 ID (${reqName} / ${rawScenario})`,
        );
        for (const id of ids) {
          check(
            globalCaseIdSet.has(id),
            `${suitePath}: "## 需求关联" 引用了长期套件中不存在的用例 ID "${id}"`,
          );
        }

        const scenSet = parsedSpec.byRequirement.get(reqName);
        check(
          Boolean(scenSet),
          `${suitePath}: "${expectedSpecPath}" 中不存在 Requirement "${reqName}"`,
        );
        if (!scenSet) continue;

        const resolvedScenarios =
          !scenSet.has(rawScenario) && rawScenario.includes('、')
            ? rawScenario.split('、').map(s => s.trim()).filter(Boolean)
            : [rawScenario];

        for (const scName of resolvedScenarios) {
          check(
            scenSet.has(scName),
            `${suitePath}: "${expectedSpecPath}" Requirement "${reqName}" 中不存在 Scenario "${scName}"`,
          );
          if (specRef === expectedSpecPath && scenSet.has(scName) && ids.length > 0) {
            coveredDurable.add(`${expectedSpecPath}\0${reqName}\0${scName}`);
          }
        }
      }
    }

    stats.durableCoveredScenarios += coveredDurable.size;
    for (const key of parsedSpec.scenarioSet) {
      if (!coveredDurable.has(key)) {
        check(
          false,
          `${suitePath}: 长期规格场景缺少验收用例映射 -> ${key.replaceAll('\0', ' / ')}`,
        );
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
        '  node testing/suites/verify-suite.mjs                            # 校验全部活动变更、已归档变更、仓库文档链接与长期套件 100% 覆盖',
        '  node testing/suites/verify-suite.mjs --change <name>            # 按实施准备/门禁就绪状态校验指定活动变更',
        '  node testing/suites/verify-suite.mjs --change <name> --deferred # 仅按远期冻结需求/设计阶段校验指定变更',
      ].join('\n'),
    );
    return;
  }

  const errors = [];
  const check = (valid, message) => {
    if (!valid) errors.push(message);
  };
  const stats = {
    deferredChanges: 0,
    implChanges: 0,
    archivedChanges: 0,
    activeChangeCases: 0,
    durableSuites: 0,
    durableCases: 0,
    durableCoveredScenarios: 0,
    durableTotalScenarios: 0,
  };

  validateOpenSpecConfig(check);
  const durableCaseOwners = collectDurableCaseOwners();

  if (options.change) {
    const explicitMode = options.deferred ? 'deferred' : 'impl';
    const otherActiveOwners = collectOtherActiveCaseOwners(options.change);
    validateChange(options.change, explicitMode, check, stats, {
      durableCaseOwners,
      otherActiveOwners,
    });
  } else {
    validateRepoDocsLinks(check);
    const changesRoot = 'openspec/changes';
    if (exists(changesRoot)) {
      const changeDirs = fs
        .readdirSync(abs(changesRoot), {withFileTypes: true})
        .filter(d => d.isDirectory() && d.name !== 'archive')
        .map(d => d.name)
        .sort();
      for (const name of changeDirs) {
        const otherActiveOwners = collectOtherActiveCaseOwners(name);
        validateChange(name, 'auto', check, stats, {
          durableCaseOwners,
          otherActiveOwners,
        });
      }
    }
    validateArchivedChanges(check, stats);
    validateDurableSuites(check, stats);
  }

  if (errors.length > 0) {
    console.error(errors.map(err => `- ${err}`).join('\n'));
    process.exitCode = 1;
    return;
  }

  if (options.change) {
    const modeLabel = options.deferred ? 'deferred' : 'implementation-ready';
    console.log(
      `Spec & suite structural check passed for "${options.change}" (${modeLabel}, ${stats.activeChangeCases} cases).`,
    );
  } else {
    console.log(
      `Spec & suite structural check passed: ${stats.implChanges} spec-driven change(s) (${stats.activeChangeCases} cases), ${stats.deferredChanges} deferred change(s), ${stats.archivedChanges} archived change(s), ${stats.durableSuites} durable suite(s) (${stats.durableCases} cases, ${stats.durableCoveredScenarios}/${stats.durableTotalScenarios} scenarios mapped).`,
    );
  }
}

main();
