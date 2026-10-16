# Design

## Context

动机见 `proposal.md` 的 Why；行为要求见 `specs/ui-asset-hosting/spec.md`。本节只写**本次实测**到的现状与它给方案划的边界。

### 现象与复现（2026-10-16，本机 Windows x64）

复现面：Bun 起的极简静态服务，响应头**逐字复制** `crates/server/src/transport/net/static_ui.rs` 的 `STATIC_HEADERS`（含 `CONTENT_SECURITY_POLICY` 常量原文），托管 `npm run export:web` 产出的 `clients/app/dist`；用真实 Chromium 打开 `http://127.0.0.1:18871/` 后在页面里求值。

| 观测点 | 修复前 | 修复后（同一复现面） |
| --- | --- | --- |
| `document.querySelectorAll('style')` | 2 条：`react-native-stylesheet`（0 字符，`sheet === null`）、`expo-reset`（379 字符，`sheet === null`） | 0 条 |
| `document.styleSheets.length` | `0` | `2` |
| `document.documentElement` 计算高度 | `8px`（reset 未生效） | `768px` |
| `body` 计算 `overflow` / `margin` | `visible` / `8px` | `hidden` / `0px` |
| `#root` 计算 `display` / 高度 | `block` / `0px` | `flex` / `768px` |
| 首个子元素的 RNW 原子类（`css-g5y9jx r-13awgt0`）计算样式 | `display: block` | `display: flex; flex-direction: column` |

「修复后」这一列是把 `dist/index.html` 按 D1 手工改写后重测的结果——即本设计的可行性与判据都已实测，不是推论。修复前 `react-native-stylesheet` 存在但 0 字符，说明样式**不是"少了一点"而是整表丢失**：UI 在真实托管下完全无样式。

### 机制：为什么两条 `<style>` 都被拦

CSP 里没有 `style-src`，`style-src` 与 `style-src-attr` 都回落到 `default-src 'self'`，因此内联 `<style>` 元素与 `style` 属性一律被拒。两条样式表都属这一类：

- `expo-reset`：由 `expo export` 生成的入口文档自带的内联 `<style id="expo-reset">`（`html,body{height:100%}`、`body{overflow:hidden}`、`#root{display:flex;height:100%;flex:1}`）。
- `react-native-stylesheet`：`react-native-web` 0.21 的 `StyleSheet` 在**模块求值期**执行 `createSheet()` → `createCSSStyleSheet('react-native-stylesheet')`，没有现成元素时 `document.createElement('style')` 并插进 `<head>`；该元素被 CSP 拦下后 `element.sheet === null`，此后全部 `insertRule` 静默失败（`insertRuleAt` 的 `catch` 吞掉异常），样式表因此恒为空。UI 的原子类全部来自这里。

同一段 `createCSSStyleSheet` 里有一条**复用分支**：先 `document.getElementById('react-native-stylesheet')`，**找到就用它的 `.sheet`**，找不到才新建 `<style>`。这条分支是 RNW 为 SSR/hydration 保留的（服务端渲染出的样式元素在客户端被复用），本设计正是走它。

### 已知边界（实测）

- `crates/server/src/transport/net/static_ui.rs` 的 `MIME_TYPES` **已含** `css → text/css; charset=utf-8`，因此托管层不需要为外部样式表改任何响应头或内容类型映射，CSP 也不必动。
- 入口文档由 `expo export --platform web` 生成：`app/+html.tsx` 在 `web.output: "single"` 下**不被使用**（加了这个文件再导出，产物里没有它的痕迹）；`public/` 目录会被原样拷进产物根（实测）。
- `default-src 'self'` 允许同源的 `<link rel="stylesheet">`；样式表**加载**受允许，而 `insertRule` 本身不是 CSP 的检查点。

## Goals / Non-Goals

**Goals:**

- 在**既定 CSP**（一字节不改）下让前端呈现完整样式，包括 RNW 运行时生成的原子类。
- 留下**可证伪**的回归证据：在带该 CSP 的面上断言**计算样式**真的生效，而不是断言 DOM 里有 `<style>` 元素。
- 让「样式形态必须落在前端侧」成为构建期与运行期都能被检查的事实，而不是靠人记得。

**Non-Goals:**

- 不改 CSP、安全响应头集合、协议与安全模型（`proposal.md` 的 What Changes 已把这几条列为非目标）。
- 不引入 nonce/hash 机制：那同样要改 CSP 下发逻辑，属本变更的非目标。
- 不重写样式体系（仍是 RNW 的 `StyleSheet`；不迁移到普通 CSS/`css` 导入）。
- 不把浏览器级 E2E 接进 CI：`clients/app` 的浏览器级检查现在仍需本地手动跑（`.github/workflows/ci.yml` 的既有登记），本变更不改变这一点，只让新断言进入**既有的**那份检查集。
- 不改变开发态（`expo start`/静默无 CSP 的托管）的样式结果：外部样式表在无 CSP 的面上同样生效。

## Decisions

### D1：把两种内联样式形态都换成同源外部样式表

两条独立的改动，缺一不可：

1. **`expo-reset` → 外部文件 + `<link>`**：入口文档里那段内联 `<style>` 抽成 `dist/entry-reset.css`，原位置换成 `<link rel="stylesheet" href="/entry-reset.css">`。内容**从生成的入口文档里抽取**，不另抄一份：Expo 模板改了 reset 就不会漂移；抽不到（模板形态变了）就**构建失败**，而不是静默产出一份没有 reset 的入口文档。
2. **RNW 运行时样式表 → 复用同源空表的 `<link>`**：入口文档在 bundle 的 `<script defer>` **之前**声明 `<link id="react-native-stylesheet" rel="stylesheet" href="/rnw-stylesheet.css">`；RNW 模块求值期按 id 找到这个元素、取它的 `sheet`，把初始 reset 与全部原子类 `insertRule` 进去。该文件**必须保持为空**（见 R2）。

两处落点都在前端侧，托管层的响应头与 CSP 一字不动。

**为什么 link 必须在 bundle 之前**：RNW 取的是 `link.sheet`，而样式表未加载完时它是 `null`（此时 RNW 既不会退回内联 `<style>`，也不会重试——`sheet` 在模块求值期就固定了）。实测 Chromium 会阻塞 `defer` 脚本直到前面的样式表就绪：把 CSS 响应人为延迟 300ms，脚本在 321.5ms 才执行且 `link.sheet` 非空。因此「link 在 bundle 之前」是硬性顺序约束，不是风格问题。

**为什么不改 node_modules**：这条复用分支是 RNW 自己的既有路径（`id` 是它的默认 id），不需要 patch 第三方包，也不需要 metro resolver 别名。

### D2：落点是构建期转换，且转换自身失败关闭

新增 `clients/app/scripts/` 下的转换模块（入口是「入口文档字符串 → 改写后的入口文档 + 两份 CSS 内容」的纯函数，便于被脚本自身与将来的调用方复用），并接进 `export:web` 的链条。它与 `write-build-version.mjs` 分工不同，不合并：前者改入口文档的样式形态，后者写构建版本。

**不另写单测**：`clients/app` 的 vitest `include` 只收 `src/**/*.test.ts`，`tsconfig` 也不含 `scripts/**`，写进 `scripts/` 的测试会被**静默跳过**（既不被 vitest 跑、也不被 tsc 检查），比没有测试更坏。转换的正确性由下面三条构建期断言承担——它们在 CI 的前端 job 里真的会执行；要覆盖「模板形态变了」这条分支，靠的是让构建失败而不是断言一个不存在的运行期。

失败关闭的三条断言（都在构建期执行）：抽不到内联 reset 块 → 失败；改写后的入口文档缺任一 `<link>` → 失败；`dist/index.html` **仍含任何内联 `<style>`/`style=`** → 失败。这三条同时是「把样式改回内联就变红」的构建期那一半证据。

### D3：回归证据分两层，浏览器层落在既有的 e2e 检查集

- **构建期（CI 可见，无需浏览器）**：`npm run check` = typecheck + vitest + `expo export`（`typecheck:e2e` 由后续的 #134 追加进这条链），转换串在 `export:web` 上，因此 D2 的三条断言在 CI 的 `前端检查（clients/app）` 里真的会执行。
- **浏览器级（判据在这里）**：`clients/app/e2e` 的假主机面按生产口径加上**同一组静态响应头（含 CSP）**，页面按 D1 声明 link，新增一条检查断言**计算样式**生效并断言页面里没有内联 `<style>` 元素。

为什么浏览器层是判据：CSP 由浏览器内核执行，vitest 的 node/jsdom 环境复现不了这条拦截（jsdom 里 `sheet` 不为 `null`，样式照样生效）。而 CI 的前端 job 不装浏览器内核，所以这一层只能进 `clients/app/e2e` 这份**既有**检查集（`task 2.2` 的二选一里正是这一支）。

同一份 e2e 面还要做两件配套事，否则加了 CSP 会自己把自己拦坏：把 `run-e2e.mjs` 注入的**内联**页面错误收集脚本挪成外部文件（`script-src 'self'` 会拦内联脚本），并让服务器为所有响应统一下发那组头。假主机的 WebSocket 与页面同源，`connect-src 'self'` 覆盖它。

### 备选与否决理由

- **A1 放宽 CSP（加 `style-src 'unsafe-inline'` 或给 `<style>` 发 nonce）**：`spec.md` 的第二个 scenario 明确禁止把放宽 CSP 当规避手段；且它要改的是安全基线，得先按 `AGENTS.md` §2 请求确认并同步 `docs/SECURITY_DESIGN.md`。否决。
- **A2 构建期静态抽取 RNW 的 CSS**：RNW 的 `getTextContent()` 确实能做构建期抽取，但它只覆盖**抽取时渲染到**的样式；本应用的样式里有随状态变化的原子类，静态抽取必然漏，漏掉的部分在既定 CSP 下**静默**不生效——与「呈现完整样式」不符。否决。
- **A3 让 RNW 写进 `adoptedStyleSheets`**：实测在既定 CSP 下可行（`new CSSStyleSheet().replaceSync` 不是 CSP 检查点），但要替换 RNW 的 `dom/createCSSStyleSheet` 才能让 RNW 用它，等于用 metro 别名打第三方包内部路径，或引入补丁工具。改动面与耦合都比 D1 大。否决。
- **A4 改 `web.output` 为 `static` 或把样式体系迁到普通 CSS**：前者要把所有路由做成可预渲染，后者是重写整个样式体系；两者都与「让既定 CSP 下样式生效」的范围不相称。否决。

## Risks / Trade-offs

- **R1 依赖 RNW 的内部约定**：`id = "react-native-stylesheet"` 与「先查已存在元素」这条复用分支是 RNW 的实现细节（SSR 支持的一部分）。RNW 升级若改掉它，失败形态是**静默无样式**（不是报错），因此必须由 D3 的浏览器级计算样式断言钉住，而不是靠人记得。
- **R2 那份 CSS 文件必须保持为空**：RNW 的 hydration 会遍历该表已有规则并按 `[stylesheet-group="N"]` 标记归组；遇到没有标记的规则会走到 `groups[undefined].rules` 抛 TypeError。空表让这段遍历一次都不进，是**硬约束**。反过来它的失败形态是 fail-loud（模块求值期抛错 → 页面空白 + 控制台报错），比静默无样式更容易被发现。
- **R3 同源与内容类型**：link 指向的文件必须是同源且以 `text/css` 下发。实测把 CSS 响应写成 `application/octet-stream` + `nosniff` 时浏览器拒绝该表，而 RNW 读 `cssRules` 直接抛 SecurityError（整页空白）。托管层现有的 `MIME_TYPES` 已含 `css`，本条是**回归**风险而不是待补的缺口；D3 的 e2e 面会覆盖它。
- **R4 `insertRule` 不是 CSP 的检查点**——安全含义要如实写进文档，不能含糊成「CSP 放宽了」：样式文本的来源是同源 bundle（`script-src 'self'` 已信任它），因此校验口径没有变化；但也**不能**据此声称「CSP 阻止了样式注入」——这条控制在内联形态下本来就已失效（正是本次要修的冲突）。
- **R5 入口文档改写与 Expo 模板的耦合**：转换按固定结构定位那段内联 reset 块。模板变化会让构建失败（D2 的失败关闭），代价是需要人来更新转换，收益是不会有第二份 reset 定义悄悄漂移。
- **R6 e2e 面加了 CSP 之后的连带风险**：既有检查集全部改在带 CSP 的面上跑。已识别的两处必须同批处理（内联 bootstrap 脚本外置、`connect-src 'self'` 覆盖同源 WebSocket）；其余风险由「既有检查集全绿」这条判据兜住——它们此前就是靠真实 DOM 断言行为的。

## Migration Plan

- **发布形态**：前端产物与托管它的二进制仍是同一次构建（`rust-embed` 在编译期选表），因此没有「新旧产物混用」的新窗口；升级路径不变——先 `npm run export:web` 产出 `dist/`，再重新构建/发布 Daemon。
- **回滚**：回滚本变更即回到「内联样式被 CSP 拦下」的旧行为（无样式），不涉及数据迁移、协议版本或安全基线的回滚——CSP 与响应头集合在整个变更里没有被改动过。
- **需要重建的资产**：`clients/app/dist`（入口文档形态变了）与嵌入它的二进制。缓存不受影响：静态响应一律带 `no-store`。

## Open Questions

无。设计期可安全延后的未知项（是否把浏览器级 e2e 接进 CI、RNW 升级策略）已在 `docs/FRONTEND_DESIGN.md` 与 `.github/workflows/ci.yml` 的既有登记里，不属于本变更的范围。
