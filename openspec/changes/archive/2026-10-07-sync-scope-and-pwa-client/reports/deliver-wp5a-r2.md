<!-- WP5a 修复轮次（Attempt 2）交付报告。coder 只交付修复、判别力证据与检查证据；不判独立 review、合入、E2E 与最终验收。 -->

# WP5a 修复轮次交付（deliver-wp5a-r2）

## Shared Report

- **task_id**: 2.5（`phase: fix`，DELIVERY）/ 3.9（CHECK：PV1 / PV3）
- **work_package**: WP5a（交付单元 MU3a）
- **role**: coder（`coder-w5a-r2`，Attempt 2）
- **phase**: fix（只修 `review-wp5a-r1` 的 F1–F4 与两条 SUGGESTION，不重写已通过检视的七层定义、协议层类型与五项视图模型断言）
- **agent_context**: `agent_id: coder-w5a-r2`，`isolation: fork_turns=none`（新实现实例接管 `.worktrees/wp5a`；未继承 `coder-w5a-r1` 的任何对话，只带原提交 `0a99eeb`、`review-wp5a-r1` 报告与 Main 的派发说明）
- **target_revision**: `a4440d683dc44ecb69d2ffeb596fbff53170272c`（分支 `feat/wp5a`，父提交 = 已检视交付 `0a99eeb`）
- **scope**: 只在 `.worktrees/wp5a` 内改 5 个文件；未触碰 `schemas/**`、`fixtures/**`、`crates/**`、`docs/**`、根 `package.json`、`scripts/**`、`.github/**`、`src/platform/`（WP5b）、`src/sync-client/`/`src/state/`/`src/features/` 的实现（WP6/WP7）、其他 worktree、主检出。报告与日志写入权威 planning root（worktree 内无 `openspec/changes/`，保持 `check:agentic` 无变更目录残留）
- **result**: **PASS**（PV1 / PV3 在目标提交上 exit 0；F1–F4 各自带「注入违规 → 检查失败 → 移除 → 通过」的判别力对照）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp5a` |
| 分支 | `feat/wp5a` |
| 已检视交付（base） | `0a99eebd696cd18e9f44a86bce08a406e56284b7` |
| 本轮交付（target） | `a4440d683dc44ecb69d2ffeb596fbff53170272c` |
| 提交信息 | `fix(frontend): WP5a 第二轮——protocol barrel 剥离 node 依赖、分层检测覆盖裸 barrel/副作用导入与 app/、manifest 校验 viewDef/viewSchema` |

`git diff --stat 0a99eeb..a4440d6`（5 files changed, **453 insertions(+), 48 deletions(-)**）：

```
 clients/app/src/layers.test.ts            | 212 ++++++++++++++++++++++----
 clients/app/src/layers.ts                 |  19 ++-
 clients/app/src/protocol/contract.test.ts | 173 +++++++++++++++++++++++-
 clients/app/src/protocol/index.ts         |  14 +-
 clients/app/src/protocol/manifest.ts      |  83 ++++++++--
```

`git diff --name-only 0a99eeb..a4440d6 | grep -v '^clients/app/'` 为空：写入范围无越界。改动文件全部落在 WP5a 自己的写入面（`src/protocol/` 与 `src/layers*`），未触 WP5b/WP6/WP7 的实现目录。

---

## 1. F1（MINOR）—— `protocol` barrel 运行期牵连 Node 内置模块

### 根因复核（成立）

`src/protocol/index.ts:19,22` 以**运行期**重导出 `./manifest`、`./paths`，二者分别 `import node:fs`/`node:path` 与 `node:url`/`node:path`。该文件是 `src/sync-client/`、`src/state/`、`src/features/` 的**唯一**导入面，因此任何按约定从 barrel 取值的消费方都会把 node 模块拖进 web bundle；Metro 对 web 平台把 `node:*` 垫成空模块，`fileURLToPath(import.meta.url)`/`readFileSync` 在运行期即 `undefined`。当前 `expo export` 之所以 exit 0，只因 expeo-router 链尚无模块 import 该 barrel。

### 修法（运行期面与类型面分离）

`src/protocol/index.ts` 现在只保留：

- `export type *`（7 个 schema 模块）+ `export type { FixtureCase, FixtureManifest, ResolvedFixtureCase } from "./manifest"`；
- 非 node 的**纯计算**运行期导出：`export { projectView, isKnownEventType, KNOWN_EVENT_TYPE_COUNT } from "./event"`。

删除了两条运行期导出：

- ~~`export { readManifest, resolveFixtureCase, MANIFEST_SCHEMA_VERSION } from "./manifest"`~~
- ~~`export { repoRoot, syncFixtureDir, syncSchemaDir, syncManifestPath, SYNC_SCHEMA_FILES } from "./paths"`~~

「读仓库根文件」入口改由**子路径**按需取（`./paths`、`./manifest`），文件头注释显式写明两条路径与「任何 `node:*` 依赖不得进入运行期导出」的规则。`./event` 本来就是纯计算模块（`projectView`/`isKnownEventType`/`KNOWN_EVENT_TYPE_COUNT`，无 `node:` import），对浏览器与 `src/domain/` 安全。

**没有**用「反正现在没人导入」搪塞：`contract.test.ts` 新增 `F1：protocol barrel 不得把 Node 内置模块带进 web bundle` 两组断言，把「barrel 运行期重导出集合恰好为 `["event"]`」以及「从 barrel 出发按运行期 import 做传递闭包触达的 `node:*` 为空」钉成机器断言。

### 判别力对照（注入 → 失败 → 移除 → 通过）

| # | 注入形态 | 注入后 | 移除后 |
| --- | --- | --- | --- |
| F1-a | 把两条运行期重导出写回 barrel（`./manifest`、`./paths`） | **FAIL**：`expected [ './event', './manifest', './paths' ] to deeply equal [ './event' ]` | PASS |
| F1-b | 间接：在 `./event` 里加 `import { syncManifestPath } from "./paths"`（一步之遥的传递依赖） | **FAIL**：`expected [ 'node:path', 'node:url' ] to deeply equal []` | PASS |

**端到端复核（web bundle，真实构建）**：在 `app/index.tsx` 里按设计 import `../src/protocol` 的 `projectView` 后 `expo export --platform web`，产物 bundle 中 `node:fs`/`node:url`/`readFileSync`/`fileURLToPath` 计数为 **0**；把 barrel 换回 `0a99eeb` 版本后同一次构建的 bundle 中出现 `fileURLToPath` ×1 与 `readFileSync` ×1（即 F1 描述的运行期失效面）。

---

## 2. F2（MINOR）—— 分层方向检测可被裸 barrel / 副作用导入绕过

### 根因复核（成立）

- `layerOfSourcePath(src/domain)` 因路径无斜杠返回 `null` → `layers.test.ts` 的 `continue` 使方向判定根本不被求值；
- `importedSpecifiers` 的正则要求 `from` 子句，对 `import "../domain/x"` 与 `await import("../domain")` 返回空数组；
- 仓库内 `src/features/index.ts:17`（`} from "../domain"`）与 `src/sync-client/index.ts:12`（`from "../protocol"`）已在用裸 barrel。

### 修法

- `layers.ts::layerOfSourcePath` 改为取**第一段**（`prefix.split("/")[0]`），使 `src/<layer>`（裸 barrel / 目录目标）与 `src/<layer>/…` 同样取到层级；`src/layers.ts`、`src/layers.test.ts` 仍返回 `null`。
- `layers.test.ts::importedSpecifiers` 扩为三条正则：`from` 子句（import/export）、无 `from` 的副作用导入、动态 `import("…")`。
- `localRelativeTarget` 增加 `@/*` 别名解析（对齐 `tsconfig.json` 的 `paths`），并先把相对说明符里的 `\` 归一化为 `/`，与 `fromRelative` 的归一化一致（堵住反斜杠写法绕过）。
- **本仓库的两处裸 barrel 已核对为合规**：`features(3) → domain(6)`、`sync-client(5) → protocol(7)` 均 `isDependencyAllowed === true`；`layers.test.ts` 新增用例断言二者「被解析为层级且方向合规」，不再依赖旧实现的漏检。

### 判别力对照（本机实测，"ALL CAUGHT: True"）

在当前实现上注入三种绕过形态 + 两种页面层形态，`layers.test.ts` 全部失败；把 `layers.ts`/`layers.test.ts` 换回 `0a99eeb` 版本对同一组注入全部**未检出**：

| 注入 | 形态 | 当前实现 | 0a99eeb（修复前） |
| --- | --- | --- | --- |
| A | 裸 barrel 反向依赖：`domain/directory.ts` 加 `import "../state";` | **FAIL（检出）** | PASS（漏检） |
| B | 副作用导入反向依赖：`protocol/common.ts` 加 `import "../domain/directory";`（无 `from`） | **FAIL（检出）** | PASS（漏检） |
| C | 目录目标反向依赖：`sync-client/index.ts` 加 `import "../features";` | **FAIL（检出）** | PASS（漏检） |
| D | 页面层直接 import 平台 API：`app/index.tsx` 加 `import "idb";` | **FAIL（检出）** | PASS（漏检） |
| E | 页面层 import 平台层：`app/index.tsx` 加 `import "../src/platform/secure-storage.web";` | **FAIL（检出）** | PASS（漏检） |

失败信息均为 `expected [ Array(1) ] to deeply equal []`（即 `violations` 非空），方向判定确实被求值。**未**靠缩小扫描范围或放宽判定「通过」。

---

## 3. F3（MINOR）—— `app` 层未进入扫描根，页面层约束实为空断言

### 根因复核（成立）

旧扫描根 `REPO_APP_ROOT = resolve(APP_ROOT, "src")` 只收集 `clients/app/src`，`clients/app/app/**`（5 个路由）从未进入 `files`；`src/<layer>/…` 的 `relative(APP_ROOT, file)` 首段不可能是 `app`。因此「跨层反向 import」与「页面层不得直接 import 平台 API」两条断言对 `app/` 永不产生 `from`。

### 修法

- 扫描根改为**两个**：`APP_LAYER_ROOT = <app>/app` 与 `SRC_LAYER_ROOT = <app>/src`（`SCAN_ROOTS`）；
- `layerOfSourcePath` 同时识别 `app/…`（首段 `app`，见 F2 的第一段取法）；
- 新增用例断言扫描集合**确实覆盖 `app` 层与六个内层**（`components/features/state/sync-client/domain/protocol`），防止将来再退回空断言；
- 平台 API 禁令现对 `from === "app"` 真实生效（判别力见上表 D、E）。

### 判别力对照

见 §2 表格的 D（`app → idb`）与 E（`app → src/platform`）两行：当前实现**FAIL（检出）**、`0a99eeb` **PASS（漏检）**。这就是 F3 要求的「在 `app/` 里注入一次违规 import，检查必须失败」。

---

## 4. F4（MINOR）—— `manifest` 的 `viewDef` 未校验、`viewSchema` 路径未约束

### 根因复核（成立）

`manifest.ts` 声明 `viewSchema?`/`viewDef?`，但 `assertManifest` 只核对 `fixture`/`schema`/`valid`/`expectedKeyword`；`viewDef` 声明后从未被读取；`viewSchema` 解析后不校验落在 `syncSchemaDir` 之下（绝对路径被 `resolve` 直接采用）；含 `viewDef` 而缺 `viewSchema` 的 case 被静默解析为 `viewSchemaPath: null`。

### 修法（选择「真正校验」而非删字段）

`manifest.ts`：

- `assertViewBinding(item, index, path)`（在 `assertManifest` 内逐 case 调用）：`viewSchema`/`viewDef` 必须**成对**出现（只出现一个即抛错）；`viewSchema` 必须是非空字符串；`viewDef` 必须匹配 `#/$defs/<eventType>`（导出 `VIEW_DEF_PREFIX`）；
- `resolveFixtureCase` 用新的 `isInsideDir(dir, path)` 约束**三类**解析结果：`fixture` 落在 `fixtures/sync/v1/`、`schema`/`viewSchema` 落在 `schemas/sync/v1/`，越界或异盘即抛错（`relative` 结果为 `""`/以 `..` 开头/仍为绝对路径均判越界）；
- `ValidFixtureCase` 的文档注明成对规则。

`contract.test.ts`：

- 每条 case 的 `viewSchemaPath`（非 null 时）必须等于 `join(syncSchemaDir, "event-views.schema.json")`；
- 新增用例遍历全部 38 条带 `viewDef` 的 case：断言 `viewDef` 形状、`$defs` 键存在，且 `viewDef` 指定的 `$defs` 键**就是该 fixture 的 `body.eventType`**（`viewDef` 从此真正被读取/比较）；
- 新增用例断言越界 `viewSchema`（绝对路径、`../../../../docs/…`）抛错；
- 新增用例用临时 manifest 文件断言半声明（只有 `viewDef` / 只有 `viewSchema`）与非法 `viewDef` 形状被 `assertManifest` 拒绝。

### 判别力对照

| # | 注入 | 当前实现 | 修复前 |
| --- | --- | --- | --- |
| F4-a | 删除 `resolveFixtureCase` 的 `viewSchema` 越界守卫 | **FAIL**：`expected [Function] to throw an error` | 无对应断言（空断言） |
| F4-b | 删除 `assertViewBinding(...)` 调用 | **FAIL**：`expected [Function] to throw an error` | 无对应断言（空断言） |

（F4-a/F4-b 的「修复前」列指 `0a99eeb` 不存在这些断言，故注入无效果；当前实现下两者都被检出。）

---

## 5. 两条 SUGGESTION 的处置

- **平台 API 禁令白名单覆盖不足**（`layers.test.ts:133-134`）：改为 `PLATFORM_API_SPECIFIERS: Record<string, true>` 查表 + `isPlatformApiSpecifier()`：命中 `node:*` 前缀、`pkg/subpath`（任一段命中）与 `@scope/…` 端口目录名，覆盖 `ws`/`socket.io-client`/`idb`/`localforage`/`indexeddb`/`local-cache`/`local-storage`/`expo-secure-store`/`secure-store`/`secure-storage`/`expo-sqlite`/`sqlite`/`sqlite3`，与 `docs/FRONTEND_DESIGN.md` §3 的 `platform/` 计划面及 plan WP5b 的 `secure-storage`/`local-cache`/`lifecycle` 端口对齐；注释写明「新增平台通道必须同步此表」。新增用例正/负双向断言 12 个应拦 + 6 个不应拦。
- **三个 `.test.ts` 未在计划归属中登记**：属计划登记缺口，**未**改 `plan.md`（已由 main 登记备查）。**强调：`src/layers.test.ts` 是 R10 唯一的机器断言，必须保留**——它承载 F2/F3 的全部判别力（裸 barrel、副作用导入、动态导入、`app/` 页面层与平台禁令），若被移除，R10 在代码层将无任何可核对约束。

---

## 6. Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run check` + `check:rust` | `.worktrees/wp5a` | 仓库根 `node_modules/`（只读 junction）；Node v22.22.0 / npm 10.9.4；rust-toolchain 1.98.1 | **0** | 见下 |
| **PV3** | `npx tsc --noEmit` + `npx vitest run` + `npx expo export --platform web` | `.worktrees/wp5a/clients/app` | 独立 `clients/app/node_modules/`（436 顶层条目）；Node v22.22.0 / npm 10.9.4 | **0** | 见下 |

### PV1 子检查逐条（`reports/PV1-wp5a-r2.log`）

- `check:schemas` → `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` → `command catalog OK: 13 commands`
- `check:errors` → `error registry OK: 58 codes across 2 protocols`
- `check:features` → `feature registry OK: 13 feature ids across 2 protocols`
- `check:assets` → `contract assets OK: 17 schemas, 213 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`
- `check:acp` → `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`
- `check:docs` → `doc links OK: 415 relative links, 9058 section refs across 518 markdown files`
- `check:boundaries` → `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`
- `check:drift` → `contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`
- `check:agentic` → `Totals: 21 passed, 0 failed (21 items)`
- `check:rust` → `cargo fmt --all -- --check`（exit 0）+ `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（exit 0，零 warning）+ `cargo test --locked --workspace --all-features`（exit 0，**96 个 `test result: ok`**）
- `CARGO_TARGET_DIR`：复用 `.worktrees/wp5a/target`（既有目录，未新建）。首轮曾在 `--workspace` 全量负载下命中 `daemon_lifecycle.rs` 的既有非确定性 flake；单独复跑该用例两次均通过（`1 passed; 0 failed`），第二轮全量串行运行**未命中**（见 §7）。

### PV3 子检查逐条（`reports/PV3-wp5a-r2.log`）

- `npx tsc --project tsconfig.json --noEmit` → exit 0（无诊断）
- `npx vitest run --config vitest.config.ts` → exit 0；3 个 test file、**50 passed / 0 failed**（`layers.test.ts` 9、`domain/domain.test.ts` 19、`protocol/contract.test.ts` 22）；0a99eeb 时为 3 file / 41 passed
- `npx expo export --platform web` → exit 0；Metro 打包 **684 modules**，产出 `dist/index.html` + 1.03 MB bundle
- 未接入根 `npm run check`（plan 裁决）；未跑浏览器用例（TP4 范围）

### 证据日志

| 检查 | 日志路径（变更目录相对） |
| --- | --- |
| PV1（`npm run check` + `check:rust`，串行 exit 0） | `reports/PV1-wp5a-r2.log` |
| PV3（typecheck / test / export:web） | `reports/PV3-wp5a-r2.log` |

---

## 7. `check:rust` 的既有 flake（如实登记，非本包引入）

本轮第一次全量 `cargo test --locked --workspace --all-features` 中 `crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown` panic（`启动后必须已经跑过一轮清理`，套件 11 passed / 1 failed / finished in 60.42s）。判据：本包**无任何 `crates/**` 改动**（`git diff --name-only 0a99eeb..a4440d6` 全部在 `clients/app/**`），因果上不可能由本包引入；该 60s 标记的时序 flake 已在 WP4 报告与 `review-wp5a-r1` §check 中登记为既有非确定性现象。

**判别力实证（单跑 vs 全量负载）**：把该用例单独跑两次，均通过（各 `1 passed; 0 failed; 11 filtered out`）。此后连续三次全量 `cargo test --locked --workspace --all-features` 均 **exit 0**（第三次的完整日志见 `reports/PV1-wp5a-r2-rust-reruns.log`），本轮**未再命中**该 flake。**因此 PV1 的最终证据取自 exit 0 的全量串行运行 `reports/PV1-wp5a-r2.log`**。说明：本报告**未**保留首轮 flake 的完整日志（该次输出在后续运行中被覆盖），首轮观测只以本节的转述与退出码记录，不作为可复核的证据文件；这一点已登记在 §8。

---

## 8. 未验证内容

- **未执行** E2E 与浏览器用例（TP3/TP4 范围）；`expo export` 只证明构建可通过，**不证明**真实浏览器中的渲染、路由与运行期行为。
- **未验证** 设备身份（WebCrypto/IndexedDB）、连接状态机迁移、命令幂等、事件去重、no-content-cache 落盘——分别属 WP5b/WP6/WP7/TP3（本包未写这些实现）。
- **未验证** `src/platform/`（WP5b 尚未创建该目录）；平台 API 禁令的通道表按设计面覆盖，WP5b 落地新通道时须同步 `PLATFORM_API_SPECIFIERS`。
- **未判**独立 review、合入、最终验收；`deps`/`advisories`/`secrets` 三个 CI-only job 本地无等价物，**未运行、不得声称通过**。
- F1 的「web bundle 运行期失效」以真实构建的产物扫描（bundle 中出现/不出现 `fileURLToPath`/`readFileSync`）为证据，**未**在浏览器里实际加载并触发该模块求值。
- **首轮 `check:rust` flake 的完整日志未被保留**（见 §7）：该次失败只以转述与退出码记录；可复核的 PV1 证据取自 exit 0 的全量串行运行，另有 `reports/PV1-wp5a-r2-rust-reruns.log` 佐证 flake 未再复现。
- 本轮为取证在临时目录复制过注入探针脚本与被改文件（均在系统临时目录、未提交、跑完移除）；工作区在本提交后干净（`git status --porcelain` 为空）。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w5a-r2"
  isolation: "fork_turns=none（新实现实例接管 .worktrees/wp5a 的 feat/wp5a；未继承 coder-w5a-r1 的对话，只携带已检视交付 0a99eeb、review-wp5a-r1 报告与 Main 的派发说明）"
handoff_index:
  - task_id: "2.5"
    work_package: WP5a
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
    evidence_type: DELIVERY
    evidence_id: deliver-wp5a-r2
    report_path: "reports/deliver-wp5a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp5a（feat/wp5a，base=0a99eeb）定点修复 review-wp5a-r1 的 4 项 MINOR 与 2 条 SUGGESTION，固定为提交 a4440d6（5 files changed, +453/−48，全部在 clients/app/**）。F1：protocol barrel 删除 './manifest'、'./paths' 两条运行期重导出（二者 import node:fs/node:path/node:url），只留 export type * 与纯计算 './event'；新增 contract.test.ts 断言 barrel 运行期重导出集合 == ['./event'] 且按运行期 import 传递闭包触达的 node:* 为空，并以真实 expo export 产物扫描复核（修复后 bundle 无 fileURLToPath/readFileSync，0a99eeb 版本出现）。F2：layerOfSourcePath 改取首段使裸 barrel/目录目标（src/domain）取到层级；importedSpecifiers 扩为 from 子句/无 from 副作用导入/动态 import 三条正则并补 @/* 别名与反斜杠归一化；仓库既有两处裸 barrel（features→domain、sync-client→protocol）经解析核对为合规并有断言。F3：扫描根扩为 <app>/app 与 <app>/src 两个，app/** 页面层进入方向判定与平台禁令，新增覆盖集合断言。F4：assertManifest 增加 assertViewBinding（viewSchema/viewDef 成对、viewDef 形状 #/$defs/<eventType>）；resolveFixtureCase 用 isInsideDir 把 fixture/schema/viewSchema 约束在各自资产目录之下；contract.test.ts 断言 38 条 viewDef 的 $defs 键存在且等于 fixture 的 body.eventType、越界 viewSchema 抛错、半声明被拒。SUGGESTION：平台通道白名单改 Record 查表 + isPlatformApiSpecifier（node:* 前缀、pkg/subpath、@scope/端口名），覆盖 src/platform/ 设计面并注明新增须同步；未改 plan.md。判别力：F2/F3 五种注入（裸 barrel 反向依赖、副作用导入反向依赖、目录目标反向依赖、app→idb、app→src/platform）在当前实现全部 FAIL 检出、在 0a99eeb 全部漏检；F4 两种注入（去越界守卫、去成对校验）均 FAIL 检出；F1 两种注入（恢复运行期 node 重导出、event.ts 间接 import ./paths）均 FAIL 检出。未重写已通过检视的七层定义、协议层类型与五项视图模型断言。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r1.md"
  - task_id: "3.9"
    work_package: WP5a
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp5a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check + check:rust（cwd=.worktrees/wp5a，CARGO_TARGET_DIR 复用 .worktrees/wp5a/target，Node v22.22.0 / npm 10.9.4 / rust-toolchain 1.98.1）在 a4440d6 上串行 exit 0：十道合同门禁全绿（schemas 149 valid/51 invalid/41 views、commands 13、errors 58、features 13、assets 17 schemas/213 fixtures/12 transcript vectors/20 negative vectors/2 SAS、acp 25 methods/11 updates/5 content blocks/3 tool content types/19 capabilities/8 invariants/10 test families、docs 415 relative links/9058 section refs/518 files、boundaries 12 crates、drift 36 DDL + 15 traits/96 methods、agentic 21 passed 0 failed）与 check:rust 三条全绿（fmt exit 0、clippy -D warnings exit 0、workspace test exit 0，96 个 test result: ok）。本轮第一次全量 workspace test 曾命中 crates/app/tests/daemon_lifecycle.rs 的既有非确定性 60s flake（本包无任何 crates/** 改动，单跑该用例两次均通过），此后连续三次全量均 exit 0、未再命中；首轮 flake 完整日志未保留（详见报告 §7/§8），可复核证据取自 exit 0 的全量运行，另有 reports/PV1-wp5a-r2-rust-reruns.log 佐证。"
    source_evidence: reports/PV1-wp5a-r2.log
  - task_id: "3.9"
    work_package: WP5a
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "a4440d683dc44ecb69d2ffeb596fbff53170272c"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-wp5a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 a4440d6 上的 clients/app 自身入口（cwd=.worktrees/wp5a/clients/app，独立 clients/app/node_modules，Node v22.22.0 / npm 10.9.4，expo 54.0.37 / typescript 5.9.3 / vitest 3.2.7）三段均 exit 0：npx tsc --project tsconfig.json --noEmit 无诊断；npx vitest run --config vitest.config.ts 为 3 个 test file、50 passed / 0 failed（layers.test.ts 9：含 F2/F3 的判别力用例、扫描覆盖与平台通道表；protocol/contract.test.ts 22：含 F1 barrel 卫生与 F4 viewDef/viewSchema 校验；domain/domain.test.ts 19 不变）；npx expo export --platform web 由 Metro 打包 684 modules 并产出 dist/index.html 与 1.03 MB bundle。未接入根 npm run check（plan 裁决），未跑浏览器用例（TP4 范围）。"
    source_evidence: reports/PV3-wp5a-r2.log
checks:
  - id: PV1
    work_package: WP5a
    command: "npm run check && cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features (cwd=.worktrees/wp5a)"
    scope: "合同门禁十道 + check:rust 三条（fmt / clippy / workspace test）"
    environment: "cwd=.worktrees/wp5a；仓库根 node_modules 以只读 junction 消费；CARGO_TARGET_DIR 复用 .worktrees/wp5a/target；Node v22.22.0 / npm 10.9.4；rust-toolchain 1.98.1"
    exit_code: 0
    log_path: reports/PV1-wp5a-r2.log
    result: PASS
  - id: PV3
    work_package: WP5a
    command: "npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web (cwd=.worktrees/wp5a/clients/app)"
    scope: "前端自身入口：类型检查 + 单元/契约测试 + Web 构建（plan 裁决不接入根 npm run check）"
    environment: "cwd=.worktrees/wp5a/clients/app；独立 clients/app/node_modules；Node v22.22.0 / npm 10.9.4"
    exit_code: 0
    log_path: reports/PV3-wp5a-r2.log
    result: PASS
test_delivery:
  WP5a:
    kind: automated
    artifacts:
      - reports/PV1-wp5a-r2.log
      - reports/PV3-wp5a-r2.log
    basic_checks:
      - PV1
      - PV3
```
