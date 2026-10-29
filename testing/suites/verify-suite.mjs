#!/usr/bin/env node
// Checks definitions, never product behavior or acceptance results.
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8').replaceAll('\r\n', '\n');
const changeRoot = 'openspec/changes/archive/2026-10-30-establish-node-access';
const sourcePath = `${changeRoot}/tests.md`;
const source = read(sourcePath);
const fields = ['用例级别', '操作步骤', '预期结果'];
const expectedIds = Array.from({length:33}, (_, i) => `NA-${String(i+1).padStart(3, '0')}`);
const errors = [];
const check = (valid, message) => { if (!valid) errors.push(message); };
function cases(text, location) {
  const headings = [...text.matchAll(/^### (NA-\d{3}) (.+)$/gm)];
  return headings.map((heading, index) => {
    const body = text.slice(heading.index, headings[index+1]?.index ?? text.length);
    const result = {id:heading[1], title:heading[2].trim()};
    for (const name of fields) {
      const values = [...body.matchAll(new RegExp(`^- ${name}：(.*)$`, 'gm'))];
      check(values.length === 1 && values[0][1].trim(), `${location} ${result.id}: ${name}须非空且唯一`);
      result[name] = values[0]?.[1]?.trim() ?? '';
    }
    check(/^(L[1-6]|LX)$/.test(result['用例级别']), `${result.id}: 未知级别`);
    check((result['用例级别'] === 'LX') === (result.id === 'NA-033'), `${result.id}: 当前仅NA-033为LX`);
    check(!/^- (Status:|执行结果：)/m.test(body), `${result.id}: 结果须另存`);
    return result;
  });
}
const sourceCases = cases(source, sourcePath);
check(JSON.stringify(sourceCases.map(item => item.id)) === JSON.stringify(expectedIds), '源用例应为唯一NA-001–033');
const suiteCases = [];
const scenarios = new Set();
for (const capability of ['node-access', 'client-trust']) {
  const suitePath = `testing/suites/${capability}/tests.md`;
  const suite = read(suitePath);
  check(suite.includes(`Source definitions: ${sourcePath}`), `${suitePath}: 缺少共同来源`);
  check(suite.includes(`openspec/specs/${capability}/spec.md`), `${suitePath}: 缺少长期规格来源`);
  suiteCases.push(...cases(suite, suitePath).map(item => ({...item, capability})));
  const delta = read(`${changeRoot}/specs/${capability}/spec.md`);
  const longTerm = read(`openspec/specs/${capability}/spec.md`);
  const body = text => text.slice(text.indexOf('### Requirement:')).trim();
  check(delta.includes('### Requirement:') && longTerm.includes('### Requirement:'), `${capability}: 缺少Requirement`);
  check(body(delta) === body(longTerm), `${capability}: 长期与增量规格正文不同`);
  const requirements = [...delta.matchAll(/^### Requirement: (.+)$/gm)];
  for (const [index, requirement] of requirements.entries()) {
    const block = delta.slice(requirement.index, requirements[index+1]?.index ?? delta.length);
    const names = [...block.matchAll(/^#### Scenario: (.+)$/gm)];
    check(names.length > 0, `${capability}/${requirement[1]}: 无Scenario`);
    for (const name of names) scenarios.add(`${capability}\0${requirement[1].trim()}\0${name[1].trim()}`);
  }
}
for (const item of sourceCases) {
  const copies = suiteCases.filter(entry => entry.id === item.id);
  check(copies.length === 1, `${item.id}: 长期出现${copies.length}次`);
  if (copies.length === 1) for (const key of ['title', ...fields]) check(copies[0][key] === item[key], `${item.id}: ${key}不同`);
}
for (const item of suiteCases) check(expectedIds.includes(item.id), `长期未知ID ${item.id}`);
const associations = new Map();
for (const line of source.split('\n').filter(line => /^\| NA-\d/.test(line))) {
  const [ids, definition] = line.slice(1, -1).split('|').map(value => value.trim());
  const reference = definition.match(/^specs\/(node-access|client-trust)\/spec\.md — Requirement: (.+?)；(.+)$/);
  check(Boolean(reference), `无法解析需求关联：${line}`);
  if (!reference) continue;
  for (const name of [...reference[3].matchAll(/Scenario: (.+?)(?=；|$)/g)]) {
    const key = `${reference[1]}\0${reference[2]}\0${name[1].trim()}`;
    check(scenarios.has(key), `关联场景不存在：${key.replaceAll('\0', '/')}`);
    const values = associations.get(key) ?? new Set();
    for (const id of ids.match(/NA-\d{3}/g) ?? []) {
      check(suiteCases.some(item => item.id === id && item.capability === reference[1]), `${id}: 能力分配或ID错误`);
      values.add(id);
    }
    associations.set(key, values);
  }
}
const matrix = read('testing/suites/traceability/establish-node-access.md');
check(matrix.includes('33 stable cases.'), '矩阵总数应为33');
const seen = new Set();
for (const line of matrix.split('\n').filter(line => /^\| (node-access|client-trust) \|/.test(line))) {
  const [capability, requirement, scenario, rawIds, status] = line.slice(1, -1).split('|').map(value => value.trim());
  const key = `${capability}\0${requirement}\0${scenario}`;
  const ids = [...rawIds.matchAll(/`(NA-\d{3})`/g)].map(match => match[1]);
  const expected = [...(associations.get(key) ?? [])].sort();
  check(scenarios.has(key) && !seen.has(key), `矩阵未知或重复场景：${key.replaceAll('\0', '/')}`);
  check(expected.length > 0 && JSON.stringify(ids) === JSON.stringify(expected), `矩阵ID不同或无关联：${scenario}`);
  check(status === '已引用', `矩阵状态错误：${scenario}`);
  seen.add(key);
}
for (const key of scenarios) check(seen.has(key), `矩阵缺少：${key.replaceAll('\0', '/')}`);
if (errors.length) {
  console.error(errors.map(error => `- ${error}`).join('\n'));
  process.exitCode = 1;
} else {
  console.log(`Definition consistency passed: ${sourceCases.length} cases, ${scenarios.size} scenarios, 2 capabilities; only NA-033 is LX.`);
  console.log('Scope: stable IDs, three fields, definition equality and spec/matrix references only; no semantic sufficiency, behavior or execution results verified.');
}
