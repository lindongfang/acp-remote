import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// 文档链接、锚点与章节引用的门禁。
//
// 为什么需要它：本仓库把 `docs/**` 当作唯一权威（`AGENTS.md` §1），文档之间靠相对链接和
// `§X.Y` 章节引用互相支撑。章节重编号、文档改名或拆分后，这些引用会静默失效——渲染出来的
// 文档仍然是「能读」的，只是把读者引到了错误或不存在的地方。这个脚本把「还能不能解析到」
// 变成机器判据。
//
// 它检查三类东西：
//   1. 相对链接的目标文件存在（`[x](./y.md)`、`![](./z.png)`、目录链接）；绝对 URL、`mailto:`、
//      纯 `#fragment` 跳过。
//   2. 链接片段（`./y.md#anchor`）在目标文档里能找到对应标题或显式锚点。
//   3. `§N.M` 章节引用：**只在该引用同一行指名了某个仓库内文档时**才判定，
//      该文档必须存在对应小节（允许引用父节，例如 `§3.2` 与 `3.2.1` 同存时算命中）。
//
// 明确不做的事（避免把合法内容误判成错误）：
//   - 不猜「隐式归属」的章节引用。像 README 表格里写「§7 的表结构」这种由上下文决定目标文档
//     的写法只会被统计，不参与判定；这类引用数量与去向在这里只作为信息输出。
//   - 不做严格的 GitHub slug 复刻：两边都去掉非字母数字后比较，因此标题里标点/空格的差异不会
//     造成误报（代价是两个仅标点不同的标题算同一节）。
//   - 不请求网络：外部 URL 只做存在性无关的跳过，不判断远端是否可达。
//
// 判据边界（写清楚以免被当成更强的保证）：通过本脚本不等于链接内容正确，只等于「目标存在、
// 锚点可在目标里找到、指名文档的章节引用能解析」。

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// 只扫描仓库自己的文本，跳过构建产物与依赖目录（与 .gitignore 的意图一致，但这里不解析 git）。
// 注意不要整体跳过以 `.` 开头的目录：`.agents/`、`.github/`、`.pi/` 里也有需要判定的文档。
const SKIP_DIRS = new Set([".git", ".husky", "node_modules", "target", "dist"]);

const errors = [];
const info = [];

function listMarkdown(dir) {
  const found = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      found.push(...listMarkdown(join(dir, entry.name)));
      continue;
    }
    // agentic 受管文件也是文档，同样纳入；它们由 `openspec-agentic update` 维护，
    // 若这里的判定失败，处理方式是升级扩展或调整本脚本，而不是手改受管文件。
    if (entry.name.endsWith(".md")) found.push(join(dir, entry.name));
  }
  return found;
}

function toRepoPath(abs) {
  return relative(root, abs).split("\\").join("/");
}

// 标题 slug：去掉 HTML 标签与 GitHub 会剔除的标点，空白转 `-`。
// 比较时再丢掉全部非字母数字字符（loose），这样标点差异不会造成误报。
function slug(heading) {
  return heading
    .toLowerCase()
    .trim()
    .replace(/<[^>]*>/g, "")
    .replace(/[\u2000-\u206F\u2E00-\u2E7F\\'!"#$%&()*+,./:;<=>?@[\]^`{|}~]/g, "")
    .replace(/\s+/g, "-");
}

function loose(text) {
  return text.replace(/[^\p{L}\p{N}]/gu, "");
}

const documents = new Map();

function loadDocument(abs) {
  const cached = documents.get(abs);
  if (cached) return cached;
  const lines = readFileSync(abs, "utf8").split(/\r?\n/);
  const anchors = new Set();
  const sections = new Set();
  let inFence = false;
  lines.forEach((line) => {
    if (/^\s*(```|~~~)/.test(line)) inFence = !inFence;
    if (inFence) return;
    const heading = /^#{1,6}\s+(.+?)\s*$/.exec(line);
    if (heading) {
      anchors.add(loose(slug(heading[1])));
      anchors.add(loose(heading[1]));
    }
    for (const explicit of line.matchAll(/<a\s+(?:name|id)\s*=\s*"([^"]+)"/gi)) {
      anchors.add(loose(explicit[1]));
    }
    for (const explicit of line.matchAll(/\{#([A-Za-z0-9_-]+)\}/g)) {
      anchors.add(loose(explicit[1]));
    }
    const numbered = /^#{1,6}\s+(\d+(?:\.\d+)*)[.\s]/.exec(line);
    if (numbered) sections.add(numbered[1]);
  });
  const document = { lines, anchors, sections, inFenceFlags: null };
  documents.set(abs, document);
  return document;
}

function sectionExists(sections, number) {
  if (sections.has(number)) return true;
  // 引用父节时接受「子节存在」：`§3.2` 在有 `3.2.1` 的文档里算命中。
  for (const candidate of sections) {
    if (candidate.startsWith(`${number}.`)) return true;
  }
  return false;
}

function resolveDocumentName(name, fromFile) {
  const cleaned = name.replace(/^\.\//, "");
  const candidates = [
    resolve(dirname(fromFile), cleaned),
    resolve(root, cleaned),
    resolve(root, "docs", cleaned),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
  }
  return null;
}

const files = listMarkdown(root).sort();

// 文档别名表：文档里既写全名（`SYNC_PROTOCOL.md`），也写简称（`SYNC §11.5`、`SECURITY §10.2`）。
// 别名只从文档名机械派生（全名，或下划线前的首段且长度 ≥ 3），不手工维护一份会漂移的对照表。
// 有歧义的别名（例如多处存在的 `README`）直接丢弃——宁可漏判，不可错判。
const aliasCandidates = new Map();
for (const abs of files) {
  const base = abs.split(/[\\/]/).pop().replace(/\.md$/, "");
  for (const alias of [base, base.split("_")[0]]) {
    if (alias === base && alias.length < 3) continue;
    if (alias !== base && (alias.length < 3 || !/^[A-Z][A-Z0-9_]*$/.test(alias))) continue;
    if (!aliasCandidates.has(alias)) aliasCandidates.set(alias, []);
    aliasCandidates.get(alias).push(abs);
  }
}
const aliasToDocument = new Map();
for (const [alias, abs] of aliasCandidates) {
  if (abs.length === 1) aliasToDocument.set(alias, abs[0]);
}

// 引用归属：只认「同一子句内、紧邻 `§` 之前」的文档名或别名。
// 跨子句（被 `、`/`，`/括号等隔开）就不再顺手认领——这正是先前把「同一行任意文档名」
// 当目标时误报的来源。找不到归属就返回 null，由调用方按「不判定」处理。
function attributeDocument(line, index, fromFile) {
  const head = line.slice(Math.max(0, index - 48), index);
  const clause = head.split(/[、，。；：（）()「」【】]/).pop();
  let found = null;
  for (const match of clause.matchAll(/([\w.-]+\.md)|([A-Z][A-Z0-9_]{2,})/g)) {
    const fileName = match[1];
    const alias = match[2];
    const target = fileName
      ? resolveDocumentName(fileName, fromFile)
      : (aliasToDocument.get(alias) ?? null);
    if (target) found = target;
  }
  return found;
}
let linkCount = 0;
let sectionRefCount = 0;
let unattributedSectionRefs = 0;

for (const abs of files) {
  const { lines } = loadDocument(abs);
  const repoPath = toRepoPath(abs);
  let inFence = false;

  lines.forEach((line, index) => {
    if (/^\s*(```|~~~)/.test(line)) inFence = !inFence;
    if (inFence) return; // 代码块里的 `[x](y)` 是示例内容，不是链接。
    const where = `${repoPath}:${index + 1}`;

    for (const match of line.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
      const target = match[1];
      if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(target)) continue; // 绝对 URL 与 mailto:
      linkCount++;
      const [pathPart, fragment] = target.split("#");
      if (pathPart === "") continue; // 纯 `#fragment`：同文档锚点，另行检查成本高于收益。
      const targetAbs = resolve(dirname(abs), decodeURIComponent(pathPart));
      if (!existsSync(targetAbs)) {
        errors.push(`${where}: 链接目标不存在 -> ${target}`);
        continue;
      }
      if (!fragment || !targetAbs.endsWith(".md")) continue;
      const targetDocument = loadDocument(targetAbs);
      const wanted = loose(decodeURIComponent(fragment));
      if (!targetDocument.anchors.has(wanted) && !targetDocument.anchors.has(loose(slug(fragment)))) {
        errors.push(`${where}: 锚点在 ${toRepoPath(targetAbs)} 中不存在 -> ${target}`);
      }
    }

    for (const match of line.matchAll(/§\s?(\d+(?:\.\d+)*)/g)) {
      sectionRefCount++;
      const number = match[1];
      if (sectionExists(loadDocument(abs).sections, number)) continue;
      const target = attributeDocument(line, match.index, abs);
      if (target === null) {
        // 隐式归属：目标文档由上下文决定（典型是文档里的表格——「权威来源」列已经写明文档，
        // 引用本身不再重复），本脚本不猜。这类引用只统计，不判定。
        unattributedSectionRefs++;
        if (process.env.DOC_LINKS_VERBOSE === "1") {
          info.push(`${where}: §${number} 无法归因（本行未在近处指名文档），未判定`);
        }
        continue;
      }
      if (!sectionExists(loadDocument(target).sections, number)) {
        errors.push(
          `${where}: §${number} 在 ${toRepoPath(target)} 中不存在（该引用指向它，但该文档没有这个小节）`,
        );
      }
    }
  });
}

if (errors.length > 0) {
  for (const error of errors) console.error(`error: ${error}`);
  console.error(`doc link check failed: ${errors.length} problem(s)`);
  process.exit(1);
}

console.log(
  `doc links OK: ${linkCount} relative links, ${sectionRefCount} section refs across ${files.length} markdown files`,
);
if (unattributedSectionRefs > 0) {
  info.push(
    `${unattributedSectionRefs} section refs 的归属文档由上下文决定（未在同一子句内指名文档），按设计未判定；` +
      `设 DOC_LINKS_VERBOSE=1 可列出位置`,
  );
}
for (const line of info) console.log(`note: ${line}`);
