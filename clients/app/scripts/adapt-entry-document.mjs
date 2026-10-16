#!/usr/bin/env node
/**
 * 把 `expo export --platform web` 产出的入口文档（`clients/app/dist/index.html`）从内联样式形态改成
 * 同源外部样式表形态，并写出那两张样式表。这是变更 `ui-csp-inline-styles` 的前端侧落点
 * （`openspec/changes/archive/2026-10-16-ui-csp-inline-styles/design.md` 的 D1/D2）。
 *
 * ## 为什么得在构建期改入口文档
 *
 * Daemon 的静态面下发固定的 CSP（`crates/server/src/transport/net/static_ui.rs` 的
 * `CONTENT_SECURITY_POLICY`），其中没有 `style-src`，于是它回落到 `default-src 'self'`：入口文档里的
 * 内联 `<style>` 元素与 `style=` 属性会被浏览器整条拒掉。这类形态在加载后的页面里恰好有两处（一处来自
 * 模板、一处在页面运行时才出现），缺一都让 UI **完全没有样式**（不是「少了点样式」）：
 *
 * 1. `expo export` 自带的 `<style id="expo-reset">`，承载 `html`/`body`/`#root` 的铺满布局；
 * 2. `react-native-web` 在**模块求值期**建出来的 `react-native-stylesheet`——被拒后
 *    `element.sheet === null`，此后全部 `insertRule` 静默失败（RNW 自己吞掉异常），于是原子类
 *    （UI 样式的全部来源）一条都没进表。
 *
 * 同源的 `<link rel="stylesheet">` 不受这条 CSP 限制，因此本脚本把两处都换成外链。reset 的内容
 * **从生成的入口文档里抽出来**，不另抄一份：Expo 模板改了 reset 就不会与本脚本漂移。RNW 那份走它
 * 自己的复用分支——`createCSSStyleSheet` 先 `getElementById('react-native-stylesheet')`，命中就复用
 * 该元素的 `.sheet`（这条分支是它给 SSR/hydration 留的），因此不需要改 `node_modules`、也不需要
 * metro 别名。
 *
 * ## 两条硬约束（实测，不是风格偏好）
 *
 * - **RNW 那条 link 必须排在 bundle 的 `<script defer>` 之前**：RNW 取的是 `link.sheet`，样式表未就绪
 *   时它是 `null`，而此时 RNW 既不退回内联 `<style>` 也不重试（`sheet` 在模块求值期就固定了）。实测
 *   Chromium 会阻塞 `defer` 脚本直到它前面的样式表就绪（把 CSS 延迟 300ms，脚本在 321ms 才跑），
 *   因此「link 在前」是样式能不能生效的前提。
 * - **`rnw-stylesheet.css` 必须保持为空**：RNW 的 hydration 会遍历这张表已有的规则并按
 *   `[stylesheet-group="N"]` 标记归组，遇到没有标记的规则会走 `groups[undefined].rules` 抛 TypeError
 *   （整页空白 + 控制台报错）。空表的 `cssRules` 是 0 条，这段遍历一次都不进。
 *
 * ## 失败关闭：三条断言（任一不成立即非零退出，绝不静默产出）
 *
 * 1. 抽不到内联 reset 块 → 失败（Expo 模板形态变了，得有人来更新本脚本的定位口径）；
 * 2. 改写后的入口文档缺任一 `<link>`（或 RNW 那条没排在 bundle 之前）→ 失败；
 * 3. 改写后的入口文档仍含内联样式形态（`<style` 元素或 `style=` 属性）→ 失败。匹配按「标签名 /
 *    属性名」口径，HTML 注释里的「style reset」与 `rel="stylesheet"` 里的 `style` 都不会被误伤。
 *
 * 这三条同时是「有人把样式改回内联形态」时的构建期证据：它们跑在 CI 的「前端检查（clients/app）」里
 * （`npm run check` → `export:web`）。样式在浏览器里是否**真的**生效由 `clients/app/e2e` 与真机验收
 * 断言计算样式，不在这里——CSP 由浏览器内核执行，构建期看不到。
 *
 * ## 为什么不写单测
 *
 * `clients/app` 的 vitest `include` 只收 `src/` 下的 `*.test.ts`，tsconfig 的 `include` 也不含
 * `scripts/`：写进 `scripts/` 的测试既不会被 vitest 跑、也不会被 tsc 检查（静默跳过，比没有测试
 * 更坏）。转换的正确性因此由上面三条构建期断言承担，取舍见 `design.md` D2。
 *
 * ## 与 `write-build-version.mjs` 的关系
 *
 * 互不依赖：本脚本改入口文档的样式形态，那个脚本写 `build-version.json`，谁先谁后都行。两者都必须在
 * `expo export` 之后——本脚本要读它产出的 `dist/index.html`。
 *
 * 用法（cwd 任意）：node scripts/adapt-entry-document.mjs
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DIST_DIR = join(APP_ROOT, "dist");
const ENTRY_DOCUMENT = join(DIST_DIR, "index.html");

/**
 * 抽出来的 reset 写成的产物文件名。
 * 入口文档里的 href 由它加 `/` 前缀得到：产物的托管口径就是根相对绝对路径（与产物里
 * `/_expo/static/js/web/entry-*.js` 的口径一致，见 `static_ui.rs` 的路径注释），文件名与 href
 * 必须同源，因此只在这一处写字面量。
 */
export const ENTRY_RESET_FILE = "entry-reset.css";

/** RNW 运行时复用的那张空表的产物文件名；「必须为空」的理由见文件头。 */
export const RNW_STYLESHEET_FILE = "rnw-stylesheet.css";

/**
 * `dist/rnw-stylesheet.css` 的内容：**只允许**这一条注释。
 *
 * CSSOM 里注释不成规则（注释在解析期被丢弃，不进 `cssRules`），因此这张表仍然是 0 条规则，空表约束
 * 不被破坏。写这条注释是因为「往空文件里补点东西」看起来太像个改进，而代价是整页空白。
 */
export const RNW_STYLESHEET_CONTENT =
  "/* 此文件必须保持为空：react-native-web 加载后复用这张表、按既有规则归组，非空会让页面整页空白。勿填。 */\n";

const RNW_STYLESHEET_ID = "react-native-stylesheet";

const RESET_LINK = `<link rel="stylesheet" href="/${ENTRY_RESET_FILE}">`;
const RNW_LINK = `<link id="${RNW_STYLESHEET_ID}" rel="stylesheet" href="/${RNW_STYLESHEET_FILE}">`;

/** 内联 reset 块：Expo 模板里的 `<style id="expo-reset">…</style>`。只按 id 定位，不按内容或缩进——
 * 内容是要原样抽走的，缩进与注释都归模板所有。 */
const RESET_BLOCK = /<style\b[^>]*\sid\s*=\s*["']expo-reset["'][^>]*>([\s\S]*?)<\/style>/i;

/**
 * 转换本体：入口文档字符串 → `{ html, resetCss }`，不碰文件系统（IO 只在本文件末尾的 `main()` 里）。
 * 抽成函数是为了让三条断言能被单独核对与复用，而不是散落在 IO 之间。
 */
export function adaptEntryDocument(html) {
  // ① 抽出内联 reset 并原位置换成同源外链。抽不到就是模板形态变了：静默产出一份没有 reset 的入口
  //    文档会让页面布局错乱（而且「没有 reset」这种缺失在产物里看不出来），因此这里必须失败。
  const reset = RESET_BLOCK.exec(html);
  if (reset === null) {
    throw new Error(
      "入口文档里找不到内联 reset 块（`<style id=\"expo-reset\">…</style>`）：Expo 的入口文档模板形态变了，" +
        "需要更新 scripts/adapt-entry-document.mjs 的定位口径。",
    );
  }
  const resetCss = reset[1];
  let adapted = html.slice(0, reset.index) + RESET_LINK + html.slice(reset.index + reset[0].length);

  // ② 在 bundle 之前声明 RNW 的复用表（顺序是硬约束，见文件头）。
  const bundle = findBundleScript(adapted);
  adapted = adapted.slice(0, bundle.index) + RNW_LINK + adapted.slice(bundle.index);

  assertExternalStylesheets(adapted);
  assertNoInlineStyles(adapted);

  return { html: adapted, resetCss };
}

/** 定位 bundle 的 `<script … defer>`。必须带 `defer` 且 src 指向 Expo 的产物目录（哈希名不硬编码）：
 * 找不到就没有可信的插入点，而「随便找个位置插」的失败形态是静默无样式（RNW 读到 `null` sheet），
 * 因此这里失败关闭。 */
function findBundleScript(html) {
  const candidates = [...html.matchAll(/<script\b[^>]*>/gi)].filter((match) =>
    (attribute(match[0], "src") ?? "").includes("_expo/static/js/"),
  );
  const bundle = candidates.find((match) => /\bdefer\b/i.test(match[0]));
  if (bundle === undefined) {
    throw new Error(
      `入口文档里找不到 bundle 的 \`<script defer>\`：src 指向 \`_expo/static/js/\` 的 script 标签有 ` +
        `${candidates.length} 个，其中带 defer 的 0 个。link 必须排在 bundle 之前才能被 RNW 复用（见文件头），` +
        "无法确定插入点即失败；模板形态变了就需要更新 scripts/adapt-entry-document.mjs。",
    );
  }
  return { index: bundle.index };
}

/** 断言②：两份外部样式表都在（且都是 `rel="stylesheet"`），并且 RNW 那条排在 bundle 之前。
 * 顺序在改写后的文档上再核一次、而不是相信上面的插入逻辑：这条是「静默无样式」的唯一防线。 */
function assertExternalStylesheets(html) {
  const links = [...html.matchAll(/<link\b[^>]*>/gi)].map((match) => match[0]);
  const isStylesheet = (tag, href) =>
    attribute(tag, "rel")?.toLowerCase() === "stylesheet" && attribute(tag, "href") === href;

  const resetLink = links.find((tag) => isStylesheet(tag, `/${ENTRY_RESET_FILE}`));
  const rnwLink = links.find(
    (tag) => attribute(tag, "id") === RNW_STYLESHEET_ID && isStylesheet(tag, `/${RNW_STYLESHEET_FILE}`),
  );
  if (resetLink === undefined || rnwLink === undefined) {
    throw new Error(
      "改写后的入口文档没有带齐外部样式表：reset " +
        `${resetLink === undefined ? "缺失" : "在"}、RNW 复用表 ${rnwLink === undefined ? "缺失" : "在"}。` +
        "两份都必须在（reset 管布局，RNW 那份承接运行时的全部原子类），否则页面在既定 CSP 下没有样式。",
    );
  }

  const rnwPosition = html.indexOf(RNW_LINK);
  const bundle = findBundleScript(html);
  if (!(rnwPosition >= 0 && rnwPosition < bundle.index)) {
    throw new Error(
      "RNW 复用表的 `<link>` 没有排在 bundle 的 `<script defer>` 之前：RNW 在模块求值期取 `link.sheet`，" +
        "样式表未就绪时它是 null 且不会重试——顺序反了的失败形态是静默无样式。",
    );
  }
}

/** 断言③：改写后的入口文档不得再有内联样式形态。
 *
 * 匹配口径是「标签名 / 属性名」而不是裸子串：`<style` 要跟一个空白或 `>` 才算标签；`style=` 前一位
 * 不能是词字符或 `-`（`(?<![\w-])`），因此 `data-style=` 不命中。HTML 注释里的「style reset」两种
 * 形态都不含；`rel="stylesheet"` 里那个 `style` 后面跟的是 `sheet`，也不会命中 `style\s*=`。 */
function assertNoInlineStyles(html) {
  const inline = /<style[\s>]/i.exec(html) ?? /(?<![\w-])style\s*=/i.exec(html);
  if (inline === null) return;
  throw new Error(
    `改写后的入口文档仍含内联样式形态（偏移 ${inline.index}）：${snippet(html, inline.index)}。` +
      "CSP 没有 `style-src`，它回落到 `default-src 'self'`，这类形态会被浏览器整条拒掉——入口文档只能通过" +
      "同源 `<link rel=\"stylesheet\">` 引样式；若模板新增了内联样式，需要更新 scripts/adapt-entry-document.mjs。",
  );
}

/** 取标签里某个属性的值。属性顺序不固定（`src`/`defer`/`id` 谁在前都可能），因此一律按属性解析，
 * 不用字符串前缀匹配；`(?<![\w-])` 让 `id` 不会命中 `data-id` 这类后缀同名的属性。 */
function attribute(tag, name) {
  const match = new RegExp(`(?<![\\w-])${name}\\s*=\\s*["']([^"']*)["']`, "i").exec(tag);
  return match === null ? null : match[1];
}

/** 报错时给出一小段可定位的原文（偏移 + 归一化空白）。 */
function snippet(html, index) {
  return `${JSON.stringify(html.slice(index, index + 80).replace(/\s+/g, " "))}…`;
}

function main() {
  let source;
  try {
    source = readFileSync(ENTRY_DOCUMENT, "utf8");
  } catch (error) {
    throw new Error(
      `读不到入口文档 ${ENTRY_DOCUMENT}（${error instanceof Error ? error.message : String(error)}）：` +
        "本脚本必须接在 `expo export --platform web` 之后。",
    );
  }

  const { html, resetCss } = adaptEntryDocument(source);

  // 先写两张表、最后写入口文档：中途失败时留下的是「入口文档仍引用内联样式」而不是「入口文档引用
  // 不存在的样式表」——前者构建就会失败（本脚本非零退出），后者要到浏览器里才看得出来。
  writeFileSync(join(DIST_DIR, ENTRY_RESET_FILE), resetCss, "utf8");
  writeFileSync(join(DIST_DIR, RNW_STYLESHEET_FILE), RNW_STYLESHEET_CONTENT, "utf8");
  writeFileSync(ENTRY_DOCUMENT, html, "utf8");

  console.log(
    `入口文档样式形态：reset → /${ENTRY_RESET_FILE}（${Buffer.byteLength(resetCss, "utf8")} 字节，` +
      `抽自入口文档），RNW 复用表 → /${RNW_STYLESHEET_FILE}（空）`,
  );
}

try {
  main();
} catch (error) {
  // 失败关闭：这三条断言不成立时**不能**产出入口文档——静默产出的后果是「页面完全没有样式」或
  // 「整页空白」，而这两种形态在 CI 的其它门禁里都看不出来。
  // 打栈而不是只打 message：断言失败的原因在 message 里，而本脚本自身的 bug（例如正则写错导致的
  // TypeError）只有栈能定位。退出码用 `process.exitCode` 而不是 `process.exit(1)`——后者可能在
  // stderr 是管道（CI）时把还没写完的错误信息截掉。
  const reason = error instanceof Error ? error.stack ?? error.message : String(error);
  console.error(`入口文档样式形态转换失败：${reason}`);
  process.exitCode = 1;
}
