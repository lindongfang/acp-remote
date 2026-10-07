<!-- WP5a（MU3a 唯一成员）第二轮复核报告（review-wp5a-r2，Round 2 recheck）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.5"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wp5a-r2"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP5a 实现，也未继承作者实例 coder-w5a-r2 的任何对话；只携带派发说明、原报告 review-wp5a-r1.md 与仓库内契约文档）"
target_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
base_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
scope: "仅复核 0a99eebd..a4440d683 的 5 files / +453 / −48（clients/app/src/{layers.ts,layers.test.ts,protocol/index.ts,protocol/manifest.ts,protocol/contract.test.ts}），外加回归检查。按原问题 ID（F1–F4）逐项核实修复是否成立，并独立核实派发说明中的 A–D 四点与两条 SUGGESTION。不重复第 1 轮已确认的部分（七层定义、协议层与冻结 schema 的逐字段同形、严格标志、五项视图模型、Web-only 裁决、写入范围、AC3 证据链）。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并、未运行任何写文件的构建或测试。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR；F7 起无新增发现。原 F1–F4 四项 MINOR 全部复核为已解决。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp5a-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1-wp5a-r2.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp5a-r2.log"
resource_cleanup: NOT_APPLICABLE（只读检视，未创建资源）

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-wp5a-r2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.5"
    work_package: WP5a
    role: reviewer
    phase: branch
    round: 2
    stage: work-package
    target_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
    evidence_type: REVIEW
    evidence_id: review-wp5a-r2
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新建独立子 Agent（未参与 WP5a 实现与修复）对 recheck 目标 a4440d6（base=已检视交付 0a99eeb，5 files/+453/−48，全部在 clients/app/**）做只读复核。按原问题 ID 逐项核实：F1 barrel 运行期面已剥离 node:*（独立静态闭包复现：barrel 运行期重导出 == ['./event']，传递闭包 node:* == []；模拟 0a99eeb 版 barrel 的闭包为 {node:fs,node:path,node:url}）；F2 layerOfSourcePath 取首段 + importedSpecifiers 三正则 + @/* 别名 + 反斜杠归一化（独立只读 Node 复现 5 种注入全部被检出、0a99eeb 版全部漏检，并自造 6 种新绕过形态均未逃逸）；F3 扫描根扩为 app/ 与 src/（独立复现文件集合 31 个、含 app 与六个内层，合法 app→components/features/domain/protocol/state 与 @/ 别名全部判为 allowed，无误报）；F4 assertViewBinding 成对+形状校验被 assertManifest 逐 case 调用、resolveFixtureCase 用 isInsideDir 约束三类路径（独立核对 126 case 全部落在各自资产目录、38 条 viewDef 与 fixture body.eventType 零不匹配）。未复跑 PV1/PV3，其结论以主 Agent 日志只读消费。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r1.md"
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp5a-r2` / 2（recheck） |
| Review Type / Stage | `branch`（修复轮次复核；MU3a 候选之前） |
| Work Package | WP5a（交付单元 MU3a 唯一成员） |
| Repository / 检视工作区 | `D:\Project\acp-remote` / `D:\Project\acp-remote\.worktrees\wp5a`（只读；`git status --porcelain` 为空） |
| Base of this recheck | `0a99eebd696cd18e9f44a86bce08a406e56284b7`（已检视交付，Round 1 target） |
| Target Revision | `a4440d683dc44ecb69d2ffeb596fbff53170272c`（分支 `feat/wp5a`，父提交 = `0a99eeb`，单提交） |
| 真实 diff | `git diff --stat 0a99eeb a4440d6` = 5 files changed, 453 insertions(+), 48 deletions(-)；`git diff --name-only ... | grep -v '^clients/app/'` 为空 |
| 读取的规则 | `AGENTS.md` §5、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` |
| 读取的上游 | `docs/FRONTEND_DESIGN.md` §3、`specs/pwa-web-client/spec.md`（R10）、`design.md` D8、`fixtures/sync/v1/manifest.json`（126 cases） |
| 使用的验证证据 | `reports/deliver-wp5a-r2.md`（仅作待核实的自评读取，**不作为检视证据**）；`reports/PV1-wp5a-r2.log`、`reports/PV3-wp5a-r2.log`（只读消费，本 reviewer **未**复跑） |
| 限制 | 不判用例集合齐备性（交 validator / main Coverage Index）；不执行 E2E（TP3/TP4）；不判 WP5b/WP6/WP7 实现 |

### 证据核对（只读消费，未复跑）

- `PV3-wp5a-r2.log`：`npx tsc --noEmit` exit 0；`npx vitest run` 3 files / 50 passed（`layers.test.ts` 9 / `domain.test.ts` 19 / `contract.test.ts` 22）；`expo export --platform web` exit 0（684 modules，1.03 MB）。适用依据 = 目标提交 + `clients/app/node_modules`。
- `PV1-wp5a-r2.log`：十道合同门禁 + `check:rust`（fmt/clippy/workspace test）exit 0（末行 `rust test exit=0`）。`check:rust` 首轮 flake 未保留日志（作者已如实登记），本 reviewer 认领其归因但不将 `check:rust` 记为「已独立验证」。
- **本 reviewer 未复跑任何构建或测试**；以下全部结论基于源码逐行阅读 + 只读 Node 探针（不写文件）。

---

## A. 判别力是否真的成立 — **成立，且覆盖原缺陷**

### A.1 五种注入的独立复现（当前实现 = 检出；0a99eeb = 漏检）

以只读 Node 探针（`cwd=.worktrees/wp5a/clients/app`，不写文件）按当前 `layers.test.ts` 的算法复现注入。对**未注入**的真实工作区：`FILES=31`，`DIR_VIOLATIONS=[]`、`PLAT_VIOLATIONS=[]`。

| 注入 | 目标解析 | 当前实现 | 0a99eeb 版算法（独立照抄复现） |
| --- | --- | --- | --- |
| A 裸 barrel 反向依赖 `src/domain/directory.ts` + `../state` | `src/state` → `state` | **检出**（dirViol=true） | **漏检**（`layerOfSourcePath("src/state")→null`，`continue`） |
| B 副作用导入反向依赖 `src/protocol/common.ts` + `../domain/directory` | `src/domain/directory` → `domain` | **检出** | **漏检**（`importedSpecifiers` 旧正则要求 `from` 子句 → 返回 `[]`） |
| C 目录目标反向依赖 `src/sync-client/index.ts` + `../features` | `src/features` → `features` | **检出** | **漏检**（同上，`/`-less 目标 + 旧守卫 `!startsWith("src/")`） |
| D 页面层平台 API `app/index.tsx` + `idb` | 非相对 → `isPlatformApiSpecifier` | **检出**（platViol=true） | **漏检**（`app/**` 不在 `collectSourceFiles(src)` 的 26 个文件内） |
| E 页面层平台层 `app/index.tsx` + `../src/platform/secure-storage.web` | `src/platform/...` → `platform` | **检出** | **漏检**（同上，文件不在扫描集） |

独立佐证：0a99eeb 的扫描集为 26 个文件、`appIncluded=false`（我复现 `collectSourceFiles(resolve(APP_ROOT,"src"))` 得 26，且无一文件路径以 `app/` 开头）；当前实现为 31（= 26 + `app/` 的 5 个路由）。因此「换回旧实现后同组注入全部漏检」**成立**，且这 5 种形态精确落在原 F2/F3 描述的缺陷面（裸 barrel、副作用导入、目录目标、`app/` 未入扫描根）上，不是无关形态。

### A.2 自造第 6 类绕过形态（独立构造，未逃逸）

我按派发要求另构造多种绕过形态，全部未能逃过当前实现：

| 新构造形态 | 解析结果 | 是否逃逸 |
| --- | --- | --- |
| 别名 `src/domain/conversation.ts` + `@/state` | `src/state` → `state` | 否（检出） |
| 反斜杠 `src/domain/conversation.ts` + `..\state` | `src/state` → `state` | 否（检出） |
| 冗余回溯 `./x/../../state` | `src/state` → `state` | 否（检出） |
| 多层/别名混排 `../../app/src/domain/x` | `app/src/domain/x` → `app`(1) vs `features`(3) | 否（检出，方向违规） |
| `.js` 后缀 `../state/index.js` | `src/state/index.js` → `state` | 否（检出） |
| 未归一化大/小写 `../STATE` | `src/STATE` → `null` | 见下（属约定外写法） |

仅当说明符为**非 relocatable** 写法（`../STATE` 大小写不符、`../state?raw` 带 URL 查询、`/` 类别裸标识符 `#state`）时才返回 `null`——这三类在 `moduleResolution: "bundler"` 的 TS/Metro 下**不可解析为真实模块**，不构成可利用的绕过，且所用正则/参数化未超出仓库既有代码的同类做法（「比例适当」）。相对路径中的 `.`/`..`/反斜杠/别名四种 `..` 变体均**未**逃逸。

**A 结论：判别力真实成立，覆盖原缺陷，且有新增判别力（自造形态未逃逸）。**

---

## B. F1 是否真的解耦 — **已解决**

### B.1 `./event` 确为纯计算、不传递引入 `./paths`/`./manifest`

`src/protocol/event.ts` 全文仅有一处 import：`:16 import type { … } from "./common";`（**类型**导入，`verbatimModuleSyntax` 下令 TS/Metro 整体擦除）。`export` 的运行期值仅 `projectView`（`:391`）、`isKnownEventType`（`:440`）、`KNOWN_EVENT_TYPE_COUNT`（`:445`）。

独立静态闭包复现（复刻 `contract.test.ts:327-380` 的 `runtimeReexportTargets`/`runtimeImportSpecifiers`/`nodeBuiltinsReachableFromBarrel`）：

- `runtimeReexportTargets(index.ts) == ["./event"]`；
- `CLOSURE_real == []`（从 `./event` 出发的运行期图无任何 `node:*`）；
- 模拟「`event.ts` 间接 `import { syncManifestPath } from "./paths"`」→ `CLOSURE == ["node:path","node:url"]`（**判别力成立**，一步之遥的传递依赖会被抓）；
- 模拟 0a99eeb 版 barrel（保留 `./manifest`、`./paths` 两条运行期重导出）→ 闭包触达 `{node:fs, node:path, node:url}`，即 F1 描述的运行期失效面。

仓库内无任何**运行期**从 barrel 的导入：全仓唯一 barrel 引用是 `src/sync-client/index.ts:12`（`import type { … } from "../protocol"`，纯类型）；`grep` 全仓（含 `scripts/`）无 `clients/app/src/protocol` 引用，删掉的两条运行期导出无消费者（**无静默丢弃**）。

### B.2 下游按约定从 barrel 取值的两条路径

- **类型面可用**：`index.ts:30` 保留 `export type * from "./event"` 等 7 条 + `:32 export type { FixtureCase, FixtureManifest, ResolvedFixtureCase } from "./manifest"`；`FixtureManifest.schemaVersion: typeof MANIFEST_SCHEMA_VERSION` 在类型层仍以 `1` 字面量表达（值侧常量由 manifest.ts 提供，`export type` 擦除后不牵连运行期）。故 `import type { SyncWireMessage } from "@/protocol"` 之类消费者路径类型层完整。
- **运行期不引入 node**：barrel 的运行期图 = `index.ts + event.ts`（`event.ts` 内部只有类型边），零 `node:*`。

`index.ts:16-19` 的文件头注释把「读仓库根文件」入口显式改为子路径 `./paths`/`./manifest`，与 `contract.test.ts:19-29` 的实际取法一致。**B 结论：F1 已解决，方法与自述一致。**

---

## C. F3 是否让页面层约束真正生效 — **已解决，且未引入误报**

- **`app/**` 真进入判定**：`layers.test.ts:46 const SCAN_ROOTS = [APP_LAYER_ROOT, SRC_LAYER_ROOT]`，`:166 …flatMap(root => collectSourceFiles(root))`，`:153 layerOfSourcePath(projectRelative(file))`。独立复现：文件集合 31 个，层级集合为 `{app, components, domain, features, protocol, state, sync-client, null}`（含 `app`）。`:148-162` 新增用例断言两个根存在、`app` 至少一个文件、且六个内层均在集合内——把「空断言」退路钉死。
- **页面层禁令不再恒真**：注入 D（`app/index.tsx + "idb"`）与 E（`app/index.tsx + "../src/platform/secure-storage.web"`）在当前实现下均**失败**（见 A.1 表）。`:218-231` 的 `if (from !== "app" && from !== "components") continue;` 现在对 `from === "app"` 真实产生分支。
- **扩根未引入误报**：独立核对合法的 app→内层与别名依赖，全部 `allowed === true`——
  `app/index.tsx → ../src/components`（2）、`→ @/components`（2）、`app/session/[id].tsx → ../../src/features`（3）、`app/index.tsx → @/protocol`（7）、`app/dir/[alias].tsx → ../src/state`（4）、`src/domain/index.ts → ./directory`（同级 6）、`src/features/index.ts → ../domain`（裸 barrel，6）、`src/sync-client/index.ts → ../protocol`（裸 barrel，7）。平台禁令对 `expo-router`/`react`/`react-native`/`expo-constants` 等页面既有依赖**不拦**（见 D 段表）。
- **`src/platform/` 的设计面不被误伤**：平台禁令只作用于 `app`/`components`，WP5b 在 `src/platform/secure-storage.web.ts` 内 import `expo-secure-store` 不会被拦（`from` 非 app/components）。

**C 结论：页面层约束真正生效，扩根后无误报，且 R10 唯一机器断言的判别力因此被扩大而非削弱。**

---

## D. F4 是否消除「声明了却不读」 — **已解决**

- **`viewDef` 被真校验**：`manifest.ts:45-66 assertViewBinding` 核对 `viewSchema`/`viewDef` **成对**（`:48-53`）、`viewSchema` 非空串（`:54-56`）、`viewDef` 匹配 `VIEW_DEF_PREFIX = "#/$defs/"` 且含非空前缀后内容（`:58-65`）；`:184` 在 `assertManifest` 的 cases 循环内**逐 case** 调用。不再「换个位置继续只声明不读」。
- **`viewDef` 被真消费**：`contract.test.ts:88-114` 遍历全部带 `viewDef` 的 case，断言 `viewDef` 形状、`$defs` 键存在（`Object.hasOwn`），且 `viewDef` 指定的键**等于该 fixture 的 `body.eventType`**。独立核对仓库根 `manifest.json`：126 cases，带 `viewDef` 的 38 条、带 `viewSchema` 的 38 条、坏配对 0 条、`viewDef` 形状全匹配、`body.eventType` 与 `$defs` 键**零不匹配**（38/38）——即该断言在真实数据上确实产生判定。
- **三类路径约束一致且严于修复前**：`manifest.ts:34-37 isInsideDir`（`relative` 结果非空、不以 `..` 开头、非绝对路径），`:203-217` 对 `fixture`（`syncFixtureDir`）、`schema`（`syncSchemaDir`）、`viewSchema`（`syncSchemaDir`）**三者**统一施加。修复前 `fixturePath = resolve(syncFixtureDir, …)` 与 `schemaPath = resolve(base, …)` 均**无** `isInsideDir` 校验。独立核对 126 case 逐一计算三类解析结果，**越界 0 条**（fixture 前缀仅 `valid`/`invalid`），故新约束不误伤既有数据。
- 越界/半声明/非法形状各有用例：`contract.test.ts:116-137`（绝对路径 + `../../../../docs/…` 抛错）、`:139-170`（只有 viewDef / 只有 viewSchema / 非法形状被拒）。

**D 结论：F4 已解决；`viewDef` 由「只声明」变为「校验 + 消费」；三路径约束统一且更严。**

---

## F2 复核 — **已解决**

`layers.ts:70-81` 的 `layerOfSourcePath` 改为取首段（`:76 const [head] = prefix.split("/")`），使 `src/<layer>`（裸 barrel/目录目标）与 `app/x` 均取到层级；`src/layers.ts`/`src/layers.test.ts` 仍返回 `null`（首段非层名）。`layers.test.ts:99-114` 的 `importedSpecifiers` 扩为三条正则（`from` 子句 / 无 `from` 副作用导入 / 动态 `import("…")`）；`:128-137 localRelativeTarget` 增 `@/*` 别名解析与反斜杠归一化。独立复现：`layerOfSourcePath("src/domain") → "domain"`、`("src/platform") → PLATFORM_LAYER`、`("app/session/[id].tsx") → "app"`、`("src/layers.ts") → null`；仓库既有两处裸 barrel 解析为层级且方向合规（`features→domain`、`sync-client→protocol`）。`layers.test.ts:270-281` 对该两处有显式断言。

判别力对照见 A.1（A/B/C 三行：当前检出、旧版漏检）。**F2 结论：已解决。**

---

## 两条 SUGGESTION 复核

1. **平台 API 白名单**：`layers.test.ts:57-81` 改为 `Record<string, true>` 查表 + `:139-142 isPlatformApiSpecifier`（`node:*` 前缀、任意段命中目录名）。独立复现：**应拦 12 个全部拦下**（`ws`/`socket.io-client`/`idb`/`localforage`/`expo-secure-store`/`expo-sqlite`/`node:fs`/`node:crypto`/`node:path`/`expo-secure-store/build/index`/`@scope/secure-store`/`foo/local-cache/build/x`），**不应拦 12 个全部放过**（`react`/`react-native`/`expo-router`/`expo-constants`/`vitest`/`../domain`/`@/protocol`/`../../src/features`/`src/state`/`react/jsx-runtime`/`expo-linking`/`expo-device`）。与 `docs/FRONTEND_DESIGN.md:94`（「页面不能直接读写 WebSocket、IndexedDB、SecureStore 或 SQLite」）及 `:80-86` 的 `platform/` 目录面（`secure-storage`/`local-cache`/`lifecycle`）对齐。**未发现误伤合法依赖。**
2. **`src/layers.test.ts` 归属与存续**：文件**仍然存在**（307 行，本轮 +212 行），且判别力**增强**（新增 A.1 表中 A–E 五类真实可检出的绕过形态 + 扫描覆盖断言 + 白名单双向断言）。**结论：R10 唯一机器断言仍在且判别力更强。**

---

## 回归判断

1. **F3 扩根后误报**：无。合法的 `app → components/features/domain/protocol/state` 与 `@/*` 别名依赖全部判为 `allowed`（C 段表）；页面既有 `expo-router`/`react` 依赖不被平台禁令命中。
2. **F2 归一化对 Windows 路径的影响**：无回归。`projectRelative`（`:123-125`）与 `localRelativeTarget`（`:135`）对文件名与说明符双向 `replaceAll("\\","/")`；独立复现 `..\state` 在 Windows 上解析为 `src/state`（检出反向依赖），未再出现「POSIX 语义把 `..\domain` 当普通文件名」的旧缺陷。
3. **F1 剥离是否损坏 Node 侧消费者**：无。`readManifest`/`resolveFixtureCase`/`MANIFEST_SCHEMA_VERSION` 仍由 `manifest.ts` 导出、`repoRoot` 等仍由 `paths.ts` 导出；`contract.test.ts:19-29` 改为直接取 `./manifest`/`./paths`，`typecheck` 与 22 条契约测试在目标提交上 exit 0（PV3 日志）。
4. **写入范围**：`git diff --name-only 0a99eeb a4440d6` 仅 5 个文件，全部在 `clients/app/**`；`grep -v '^clients/app/'` 为空。`schemas/**`/`fixtures/**`/`crates/**`/`docs/**`/`plan.md`/根 `package.json` 零改动，未越界（SUGGESTION 2 的 plan 登记缺口由 main 处理，作者未擅自改 plan）。
5. **未发现新引入的 CRITICAL/MAJOR/MINOR 缺陷**。

---

## Findings

**无新增发现**（F7 起编号未使用）。原 4 项 MINOR（`review-wp5a-r1-F1`~`F4`）经本回复核**全部解决**；`review-wp5a-r1-F5`（平台白名单）、`F6`（测试归属）分别复核为已落地 / 已由 main 登记。未发现补丁引入的回归。

---

## Assessment

### 结论

**PASS**（对应 Target Revision `a4440d683dc44ecb69d2ffeb596fbff53170272c`）。**0 CRITICAL / 0 MAJOR / 0 MINOR 新增**；F1–F4 全部复核为已解决。

- **A（判别力）成立**：五种注入在当前实现全部检出、在 `0a99eeb` 全部漏检（独立只读复现，非转述作者自评）；自造 6 类新绕过形态（别名/反斜杠/冗余回溯/多层混排/`.js` 后缀）均未逃逸。
- **B（F1）已解决**：barrel 运行期重导出恰为 `["./event"]`，传递闭包 `node:*` 为空；`./event` 纯计算，仅类型导入 `./common`；类型面经 `export type` 完整保留，运行期零 node。
- **C（F3）已解决**：扫描根含 `app/`（31 文件），页面层方向与平台禁令真实生效，扩根无误报。
- **D（F4）已解决**：`viewDef` 由 `assertViewBinding` 成对+形状校验并在契约测试中被真消费（38/38 与 `body.eventType` 一致）；`fixture`/`schema`/`viewSchema` 三路径统一受 `isInsideDir` 约束。
- **两条 SUGGESTION**：白名单双向 12 拦 + 12 不拦无误差；`src/layers.test.ts` 仍存在且判别力更强。

### 待补 / 未由本 reviewer 执行的检查

| Check ID | 状态 | 说明 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| `PV1`（`npm run check` + `check:rust`） | 主 Agent 日志 exit 0，本 reviewer 只读消费、未复跑 | `check:rust` 首轮 flake 完整日志未保留（作者已登记） | 否 |
| `PV3`（typecheck/test/export:web） | 主 Agent 日志 exit 0 / 50 passed / 684 modules，本 reviewer 未复跑 | — | 否 |
| F1 的 web bundle 运行期失效 | 未在浏览器加载验证 | 作者的 bundle 产物扫描**不可复核**：工作区无 `dist/`（`ls dist` → No such file）。本 reviewer 以静态闭包复现代替 | 否（静态闭包已足够判定） |
| 真实浏览器渲染与路由（TP4） | 未执行（范围外） | — | 否 |
| WP5b/WP6/WP7/TP3 实现 | 未验证 | 本包不含这些实现 | 否 |
| `deps`/`advisories`/`secrets` 三个 CI-only job | 未运行 | 不得声称通过 | 否 |

### 实际检查范围

- **版本与范围**：`git log/rev-parse/merge-base --is-ancestor/rev-list --count`、`git diff --name-only|--stat 0a99eeb a4440d6`、`git show 0a99eeb:…`（旧 `layers.ts`/`layers.test.ts`/`manifest.ts`/`contract.test.ts` 对照）。
- **逐字读取**（目标版本）：`clients/app/src/layers.ts`、`layers.test.ts`（307 行全文）、`protocol/index.ts`、`protocol/manifest.ts`（224 行全文）、`protocol/contract.test.ts`（400 行全文）、`protocol/event.ts`（导入面与值导出）、`protocol/paths.ts`；`app/{_layout,index,pair,dir/[alias],session/[id]}.tsx`、`src/{components,features,state,sync-client,domain}/index.ts` 的导入面；`clients/app/package.json`、`tsconfig.json`、根 `package.json`。
- **机器对照**：Python/Node 解析仓库根 `fixtures/sync/v1/manifest.json`（126 cases、38 条 `viewDef`、38 条 `viewSchema`、坏配对 0、`body.eventType` 与 `$defs` 键零不匹配）；对 126 case 逐条计算 `isInsideDir(fixture/schema/viewSchema)` 越界数（0）。
- **只读复现（Node，不写文件）**：复刻当前与 0a99eeb 两版 `collectSourceFiles`/`layerOfSourcePath`/`importedSpecifiers`/`localRelativeTarget`/`isPlatformApiSpecifier`/依赖方向判定，在工作区实测（`FILES=31`、零违规）并对 A–E 五种注入 + 6 类自造绕过形态 + 12/12 白名单双向断言逐一取结果；复刻 `contract.test.ts` 的 F1 运行期闭包扫描（真实 = `[]`，`event→paths` 注入 = `["node:path","node:url"]`，0a99eeb 版 barrel = 触达 node 内置）。
- **上游文档**：`docs/FRONTEND_DESIGN.md` §3（`:57-95`）、`openspec/changes/sync-scope-and-pwa-client/design.md`（`:128` 附近 protocol 消费约定）、`review-wp5a-r1.md`（原 F1–F6 定义）。
- **证据只读消费**：`reports/deliver-wp5a-r2.md`、`reports/PV1-wp5a-r2.log`、`reports/PV3-wp5a-r2.log`。

### 未验证内容

- **未执行**任何编译、类型检查、单元测试、`expo export` 或 E2E；`PV1`/`PV3` 结论基于主 Agent 日志，本轮未复跑。
- **未复跑** `check:rust`；不得据本报告声称 Rust 面经本 reviewer 验证通过。
- **未在浏览器加载** web bundle；F1 的端到端失效形态以静态闭包 + 读取产物逻辑复现，未实际触发 `fileURLToPath` 求值。作者报告中的 `dist/` 产物扫描**无留档**（工作区无 `dist/`），故该条自评**不可复核**，本报告不采信也不否定。
- **未判**用例集合齐备性与风险盲区（交主 Agent Coverage Index / validator）。
- **未判** WP5b/WP6/WP7/TP3/TP4 的实现；`src/platform/` 尚未落地，白名单按设计面核对。
- **未验证** 说明符的非 relocatable 写法（大小写不符、URL 查询、裸标识符）——按 `moduleResolution: "bundler"` 它们不解析为真实模块，未纳入缺陷。
- 本报告只针对 Target Revision `a4440d6`；判断为静态可判定项，不改动任何被测文件。

PASS
