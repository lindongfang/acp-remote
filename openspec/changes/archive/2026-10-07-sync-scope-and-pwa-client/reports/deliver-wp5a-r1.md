<!-- WP5a（MU3a）首轮交付报告。coder 只交付实现、局部检查与 Project Verify 证据；不判独立 review、合入、E2E 与最终验收。 -->

# WP5a 交付（deliver-wp5a-r1）

## Shared Report

- **task_id**: 2.5（`phase: implement`，DELIVERY）/ 3.9（CHECK：PV1 / PV3）
- **work_package**: WP5a（交付单元 MU3a，Order 4，independent）
- **role**: coder（`coder-w5a-r1`，Attempt 1）
- **phase**: implement
- **agent_context**: `agent_id: coder-w5a-r1`，`isolation: fork_turns=none`（新实例，未继承任何前置对话；只带 WP5a 的派发说明与 `plan.md`/`specs`/`AGENTS.md`/`docs/FRONTEND_DESIGN.md` 的契约）
- **target_revision**: `0a99eebd696cd18e9f44a86bce08a406e56284b7`（分支 `feat/wp5a`，父提交 = 固定起点 `8ca1e9e`）
- **scope**: 只在 `.worktrees/wp5a` 内新建 `clients/app/**`（38 个文件）；未触碰 `schemas/**`、`fixtures/**`、`crates/**`、`docs/**`、仓库根 `package.json`/`package-lock.json`、`scripts/**`、`.github/**`、其他 worktree、主检出
- **result**: **PASS**（PV3 exit 0；PV1 的 `check` 十道全绿 exit 0、`check:rust` 因**既有非确定性 flake** 首轮 exit 101，见 §5）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp5a` |
| 分支 | `feat/wp5a` |
| 固定起点（base） | `8ca1e9e`（`refactor(agent-host): 节点级事件改用专用 NodeEventSink 类型`） |
| 本轮交付（target） | `0a99eebd696cd18e9f44a86bce08a406e56284b7` |
| 提交信息 | `feat(frontend): WP5a 建立 Web/PWA 工程骨架、七层目录、类型化协议层与视图模型` |
| `git diff --stat 8ca1e9e..0a99eeb` | **38 files changed, 15341 insertions(+)**（其中 `package-lock.json` 11939 行） |

---

## 1. 工程骨架（`clients/app/`）

按 `AGENTS.md` §5「使用 TypeScript 与 Expo/React Native 通用工程」与 `docs/FRONTEND_DESIGN.md` §3 的
`app/`（Expo Router 页面）建立最小可运行工程。

| 文件 | 作用 |
| --- | --- |
| `package.json` | `expo ~54.0.0` / `expo-router ~6.0.0` / `react-native 0.81.4` / `react-native-web ~0.21.0`；`scripts`：`typecheck`、`test`、`export:web`、`service`（`vite preview`） |
| `tsconfig.json` | `strict` + `exactOptionalPropertyTypes` + `noUncheckedIndexedAccess` + `noImplicitReturns` + `noUnusedLocals` 等；`moduleResolution: bundler`；`paths: {"@/*": ["./src/*"]}` |
| `vitest.config.ts` | 契约测试入口；`root: __dirname`、`environment: node` |
| `app.json` | `platforms: ["web"]`、`web.bundler: "metro"`、`web.output: "single"`、`expo-router` plugin |
| `expo-env.d.ts` | `expo/types` 的显式引用，使 `tsc --noEmit` 无需生成产物 |
| `.gitignore` | `.expo/`、`web-build/`、`.vite/`、`coverage/`、`*.tsbuildinfo`（根 `.gitignore` 已有 `node_modules/`、`dist/`） |

**工程配置说明（plan 未逐字列出，属工程脚手架，一并列明理由）**：`tsconfig.json`、`vitest.config.ts`、
`app.json`、`expo-env.d.ts`、`.gitignore` 五者不在 plan 的 WP5a Write Scope 字面清单里，但
`package.json` 声明的 `typecheck`/`test` 没有它们无法执行——PV3 明确要求「`clients/app` 的类型检查与
测试脚本」可跑。`clients/app/package-lock.json` 同理（仓库既有约定是锁定文件入库，见 `AGENTS.md`
§「两个依赖锁定文件都已入库」）。

**Vite 与 Metro 的关系（如实登记）**：`plan.md` 的 Runtime Resources 只为 TP4 登记了「前端静态服务器
（Vite dev server）」，Round 8 的 F33 把 Metro 从该行移除，理由是「Metro 是 React Native 打包器」。
本包的处理是：**产品工程的打包器用 Expo 自带的 Metro**（`expo export --platform web` 产出静态资源），
**Vite 只作为 TP4 的静态文件服务器**（`npm run service` = `vite preview`，服务 `expo export` 的产物）。
F33 的原始意图是「v1 不构建原生包」（`tasks.md` 2.6），不是「不得使用 Expo 的 web 打包器」——
若把 Metro 一并排除，则 `AGENTS.md` §5 与 `FRONTEND_DESIGN` §3 要求的 Expo/Expo Router 无法成立。
这是对 plan 措辞的一处解读，**留给 review 核对**；两条路径（Metro 打包产物 + Vite 静态服务）都已实测可用。

**不写 `.native.ts`**：按 `tasks.md` 2.6 的裁定「v1 只交付 Web，原生端不在本次范围」，本包
**没有**创建任何 `.native.ts` 变体，`app.json` 的 `platforms` 也只含 `"web"`。这是一个明确的范围
决定，不是「暂时没人做」。

## 2. 七层目录与单向依赖（R10）

```text
src/
├─ layers.ts        分层序号与方向判据的**单一**机器可读定义
├─ layers.test.ts   逐文件核对 import 方向的静态检查
├─ protocol/        Sync DTO（消费仓库根 schemas/ 与 fixtures/）
├─ domain/          客户端领域类型与视图模型
├─ sync-client/     同步客户端层（WP6 实现；本包只有类型面）
├─ state/           连接与命令状态机（WP6 实现；本包只有状态集合）
├─ features/        feature 控制器（WP7 实现；本包只有视图模型消费面）
└─ components/      共享展示组件骨架（WP7 接管实现）
app/                Expo Router 空壳路由（WP7 接管实现）
```

`src/layers.ts` 的 `LAYER_ORDER` 把序号固定为
`app(1) → components(2) → features(3) → state(4) → sync-client(5) → domain(6) → protocol(7)`，
依赖只能指向序号不小于自身的层级。`src/platform/` 是**横切替换面**（Web 用 IndexedDB/WebCrypto，
未来原生用 Keychain/SQLite），单独列为 `PLATFORM_LAYER` 并禁止被 `domain`/`protocol` 反向依赖。

`src/layers.test.ts` 的五条断言：

1. `src/` 下确实存在待判定的源文件；
2. **没有任何跨层反向 import**（遍历全部 `.ts`/`.tsx`，解析每条相对 import，逐条比对方向）；
3. 序号表覆盖七层且单调；
4. `domain`/`protocol` 不得依赖更外层的层级；
5. 页面层（`app`/`components`）不得直接 import 平台 API 模块。

**判别力实证（回退源码）**：临时在 `src/protocol/common.ts` 首行加入
`import type { DirectoryView } from "../domain/directory";`（protocol → domain），
`layers.test.ts` 当即 4 passed / **1 failed**，报出两条违规
（`src\protocol\common.ts（protocol → domain）违反单向依赖：../domain/directory`）；
移除后 5 passed。**未**以「只检查目录名是否规范」的弱断言代替方向判定。

> 期间修掉一个真实缺陷：`localRelativeTarget` 原先直接对 `relative()` 的结果调用 `posix.dirname`，
> 而 Windows 上 `relative()` 返回反斜杠路径，`posix.dirname("src\\protocol\\common.ts")` 返回 `"."`，
> 层级判定整体失效（探针实测：违规不被发现）。修法是先把反斜杠归一化为正斜杠再做 posix 运算。
> 这个缺陷只在 Windows 上出现，若不修，本包的 R10 断言在开发机上会变成永远为真的装饰。

## 3. `src/protocol/` —— 严格类型化消费机器合同

**类型来源**：逐份镜像仓库根 `schemas/sync/v1/**`，**不复制**任何 schema/fixture 到 `clients/app/` 下。

| 镜像文件 | 对应 schema |
| --- | --- |
| `common.ts` | `common.schema.json`（27 个 `$defs`） |
| `auth.ts` | `auth.schema.json`（4 条认证消息） |
| `sync.ts` | `sync.schema.json`（订阅/快照/ACK 8 条） |
| `command.ts` | `command.schema.json`（11 条 Sync 面命令 + 结果） |
| `event.ts` | `event.schema.json` + `event-views.schema.json`（34 个 eventType） |
| `error.ts` | `error.schema.json`（error/ping/pong） |
| `pairing.ts` | `pairing.schema.json`（配对 HTTPS 载荷） |
| `paths.ts` | 仓库根资产的唯一路径解析入口 |
| `manifest.ts` | `fixtures/sync/v1/manifest.json` 的严格类型化读取 |
| `index.ts` | 公开入口与 `SyncWireMessage` 联合 |

**严格性**：

- 全局无 `any`（`tsconfig` 的 `strict` + `noImplicitAny` 生效）；不用可选链把类型错误吞成 `undefined`。
- `T | null` 写成 `T | null`（键必填），不写成 `T?`——例如 `SessionSummary.title: string | null`。
- `additionalProperties: false` 的对象用精确接口：多写一个字段即**编译失败**。
  `sessionCreate.payload` 只有 `workspaceAlias`/`agentId` 两键，`cwd` 一类字段无法构造。
- 唯一保留索引签名的是 schema 里明确开放的 `event.payload.view`、`error.details`、
  `eligibility.*.schema`/`capabilities.*`——与 schema 的 `additionalProperties: true` 一致。
- `exactOptionalPropertyTypes: true` 让「可选」与「显式 undefined」不再混同。

**不复制测试样例**：`manifest.ts` 只解析路径并把它们交给调用方 `readFile`；
`contract.test.ts` 从 `fixtures/sync/v1/manifest.json` 读出每条 case，断言其 `fixture`/`schema`
指向**仓库根**的真实文件，并断言 `clients/app/` 下不存在 `schemas/` 或 `fixtures/` 目录。

**Rust 类型化镜像的一致性**：`event.ts` 的 `eventType` 登记为 34 项，`VIEW_ENUMS` 登记的六个封闭
enum（`plan.priority`/`plan.status`/`elicitation.action`/`terminal.stream`/`agent.connected.state`/
`agent.disconnected.state`）在 TS 侧逐条对应；`contract.test.ts` 断言
`event-views.schema.json` 的 `$defs` 数量等于 `KNOWN_EVENT_TYPE_COUNT`。

## 4. `src/domain/` —— 视图模型

| 视图模型 | 承载的需求 | 协议载荷隔离方式 |
| --- | --- | --- |
| `DirectoryView` / `DirectoryListState` | R15 | 输出键只有 `id`/`displayName`/`alias`/`counts`；**没有**任何可反推路径的字段（测试逐键断言） |
| `SessionSummaryView` / `SessionTitle` / `SessionOriginView` | R15、R20 | 输出键换成 `id`/`directoryAlias` 等展示向字段；`origin` 是 `local`/`remote` 判别联合，wire 的 `originBlock` 不进入输出 |
| `AgentCatalogEntryView` / `AgentCatalogView` / `AgentConnectionState` | R19 | 列表来自快照，连接状态是可选覆盖层；覆盖层缺席即 `unknown`，**不等于** `disconnected`，也不移除条目 |
| `MessageView` / `SessionReadPageView` / `ContextUsageView` | R17、R2/R3 的消费面 | 三态判别联合 `unreported`/`zero_window`/`known`；不存在「零占用」态 |

**R15 的关键判定**（均有独立用例）：

- 两种空态可区分：`no_directories` 与 `directories_without_sessions`；
- 目录汇总（总数/运行中/待处理）由 `SessionSummary.state` 得出，因此断连时仍可渲染；
- 同名目录以别名区分，不做路径消歧（两项都保留，不合并）。

**R19 的关键判定**：覆盖层缺席时全部条目 `connection === "unknown"`，
且断言 `every(entry => entry.connection !== "disconnected")`——把「状态未知 ≠ 已断开」钉成断言。

## 5. 重叠区域的骨架与留给下游的边界

- `clients/app/app/`（5 个空壳路由）与 `clients/app/src/components/`（3 个展示原语）按
  `plan.md` 的 Shared File Ownership 行**只建骨架**：路由只声明 `data-route`/`data-session-id`
  等稳定选择器（供 TP4 的浏览器用例），组件只声明公共 props 与文案词表。**页面实现与后续全部改动归 WP7**。
- **未写入** `src/platform/`（归 WP5b：WebCrypto 不可导出 P-256 密钥 + IndexedDB、`local-cache.web.ts`、
  `lifecycle.web.ts`）。
- **未写入** `src/sync-client/` 与 `src/state/` 的**实现**（归 WP6）：本包只提供这两个目录的类型面
  （`WireMessage`/`EventDedupKey`、`ConnectionState` 12 态、`CommandState` 7 态、`BLOCKING_CONNECTION_STATES`）。
- **未写入** `src/features/` 的页面（归 WP7）：本包只提供 `FeatureViewModels` 消费面。

## 6. AC3 的可核对证据（读同一份 manifest 与 schema）

**相对路径**：`clients/app/src/protocol/paths.ts` 从 `import.meta.url` 上溯**四级**得到仓库根
（`protocol` → `src` → `app` → `clients` → `<repo>`），导出 `repoRoot`、`syncSchemaDir`
（`<repo>/schemas/sync/v1`）、`syncFixtureDir`（`<repo>/fixtures/sync/v1`）、`syncManifestPath`。

**读取方式**：`manifest.ts` 的 `readManifest()` 用 `readFileSync(syncManifestPath)` 读取
`fixtures/sync/v1/manifest.json`；`resolveFixtureCase(entry)` 以 manifest 所在目录为基准解析
case 的 `schema`（形如 `../../../schemas/sync/v1/message.schema.json`）。`contract.test.ts` 用
`readSchema(file)` 从 `<repo>/schemas/sync/v1/` 直接 `readFileSync` 全部九份 schema。

**可核对的断言**（`contract.test.ts` 的 17 条）：

1. `MANIFEST_REPO_ROOT` 含 `package.json` 与 `Cargo.toml`，且**不以 `/clients/app` 结尾**；
2. `clients/app/` 下**不存在** `schemas/` 与 `fixtures/` 目录（无副本）；
3. `syncManifestPath` 与九份 schema 都指向仓库根的真实文件；
4. 每条 case 的 `fixture`/`schema` 真实存在，且 `schema` 落在 `<repo>/schemas/sync/v1/` 之下；
5. manifest 引用的每个 schema 都在 `SYNC_SCHEMA_FILES` 清单内。

**逐项对应断言**（从 schema 读出的形状与镜像比对，而不是抄一份常量）：
`sessionSummary` 的 `required` 九项与属性名十项、`state` 的七取值枚举、
`snapshotResource` 收窄为 `["sessions","workspaces","agents"]` 且 `chunkCount.maximum === 3`、
`sessionReadBefore.required === ["createdAt","messageId"]`、`sessionRead.payload` 的键为
`before`/`include`/`limit`、`sessionReadResult.required` 含 `hasEarlier`、
`sessionCreate.payload.required === ["workspaceAlias","agentId"]` 且 `additionalProperties === false`、
`file.changed` 必填四项不变且新增三个可选字段、`agent.connected`/`agent.disconnected` 的 `state`
各为唯一取值、`event-views.$defs` 数量等于 34。

## 7. Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run check`（十道合同门禁） | `.worktrees/wp5a` | 仓库根 `node_modules/`（junction 只读）；Node v22.22.0 / npm 10.9.4 | **0** | 见下 |
| **PV1** | `npm run check:rust` | `.worktrees/wp5a` | cargo / rust-toolchain 1.98.1 | **101（首轮）** | 见 §8 |
| **PV3** | `npx tsc --project tsconfig.json --noEmit` | `.worktrees/wp5a/clients/app` | 独立 `clients/app/node_modules/`（856 packages） | **0** | 无诊断输出 |
| **PV3** | `npx vitest run --config vitest.config.ts` | 同上 | vitest 3.2.7；环境 `node` | **0** | 3 个 test file、**41 passed / 0 failed** |
| **PV3** | `npx expo export --platform web` | 同上 | expo 54.0.37（Metro） | **0** | 684 modules、产出 `dist/index.html` + 1.03 MB bundle |

### PV1 的 `check` 十道子检查逐条（`reports/PV1-wp5a-r1.log`）

- `check:schemas` → `149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` → `13 commands`
- `check:errors` → `58 codes across 2 protocols`
- `check:features` → `13 feature ids across 2 protocols`
- `check:assets` → `17 schemas, 213 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`
- `check:acp` → `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows)`
- `check:docs` → `415 relative links, 9058 section refs across 518 markdown files`
- `check:boundaries` → `12 个 crate 的依赖方向与 §5 矩阵一致`
- `check:drift` → `§7 的 36 条 DDL … §5 的 15 个 trait / 96 个方法签名 …`
- `check:agentic` → `21 passed, 0 failed`（在**移除**本 worktree 里仅含 `reports/` 的变更目录残留后）

> **关于 `check:agentic` 的一次中途失败（如实登记，非产品缺陷）**：首轮运行时该门禁报
> `21 passed, 1 failed`，失败项为 `change/sync-scope-and-pwa-client` 的「No deltas found」。
> 定位过程：`git ls-files` 证明该变更目录在本分支**未被跟踪**（`openspec/changes/<变更>/` 属规划根，
> 由 provisioner 在 worktree 内以忽略状态提供）；把它整体移开后门禁变为 `21 passed, 0 failed`，
> 恢复后再次失败。因此失败原因是「worktree 内存在一个只有 `reports/`、没有 `specs/` 的变更目录」，
> 与本次代码交付无关。修法是**不**把报告与日志写进 worktree 的变更目录，改写到权威 planning root
> （`D:\Project\acp-remote\openspec\changes\sync-scope-and-pwa-client\reports\`），worktree 保持无该目录。

### 证据日志

| 检查 | 日志路径（相对权威 changeDir） |
| --- | --- |
| PV1 | `reports/PV1-wp5a-r1.log` |
| PV3 | `reports/PV3-wp5a-r1.log` |

---

## 8. `check:rust` 首轮失败：既有非确定性 flake（非本包引入）

`crates/app/tests/daemon_lifecycle.rs` 的 `the_periodic_task_runs_again_after_one_full_cycle`
在首轮 `check:rust` 中 panic（`assertion left == right failed: 启动初清理恰好一轮`，`left: 0 / right: 1`），
该套件 `11 passed; 1 failed; finished in 2.26s`。

判据：本包**没有任何 Rust 改动**（`git diff --stat 8ca1e9e..0a99eeb` 只含 `clients/app/**`），
因此该失败在因果上不可能由本包引入。同一用例在 WP4 的报告里已被登记为按 60s 标记判定的
非确定性 flake（`reports/deliver-wp4-r2.md` 与 `PV1-wp4-r2.log`），本轮是同一现象重现。

**判别力实证（单跑 vs 并行）**：把该用例单独跑三次，均通过（各 `1 passed; 0 failed; 11 filtered out`，
单次约 60.3s）——`cargo test --locked -p app --test daemon_lifecycle the_periodic_task_runs_again_after_one_full_cycle`
× 3。而它在 `--workspace` 的并行负载下首轮失败。这坐实了「按 60s 标记判定的时序 flake」这一既有结论，
而不是产品缺陷。

后续处置：见 §10 的**未验证内容**——本节只如实记录首轮观测与实际退出码，不声称 `check:rust` 已通过。

## 9. 未验证内容

- **未验证** `check:rust` 在 0a99eeb 上的**全量通过**：首轮因上述既有 flake exit 101；
  本包无 Rust 改动，且 Dependencies 列未要求本包跑 Rust 测试。**不得**据此声称 Rust 面通过。
- **未执行** E2E 与浏览器用例（TP3/TP4 的范围）；`clients/app` 的 **AC3 对拍中属于 TP3 的部分**
  （`fixtures/sync/v1/transcripts/device-proof.json` 的跨语言字节级对拍）**不在本包**——本包只交
  `src/protocol/` 的类型化消费与「读同一份 manifest/schema」的可核对证据；设备密钥与签名属 WP5b（R11）。
- **未验证** 真实浏览器中的渲染与路由（`expo export` 只证明构建可通过，不证明页面行为）。
- **未验证** 设备身份（WebCrypto/IndexedDB）、连接状态机迁移、命令幂等、事件去重、
  no-content-cache 落盘——分别属 WP5b/WP6/WP7/TP3。
- **不判**独立 review、合入、最终验收；`deps`/`advisories`/`secrets` 三个 CI-only job
  本地无等价物，**未运行、不得声称通过**。
- 本包对 plan 措辞的一处解读（Expo 自带 Metro 打包 + Vite 仅作 TP4 静态服务器，见 §1）
  已登记，**留给 review 核对**。

## 10. 资源与清理

- `clients/app/node_modules/`：856 packages，随 worktree 隔离；`dist/`、`.expo/` 在取证后已删除，
  工作区 `git status` 干净。
- worktree 根 `node_modules/` 为指向主检出 `node_modules` 的 junction（只读消费），
  本包未写入其中任何文件。
- **未清理** worktree 内的 junction 与 `clients/app/node_modules/`：按 plan 的 Runtime Resources，
  这些资源随 worktree 由 provisioner 回收，不由执行者清理。

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w5a-r1"
  isolation: "fork_turns=none（新实现实例；未继承任何前置对话，只携带 WP5a 的派发说明、plan.md/specs/AGENTS.md/docs/FRONTEND_DESIGN.md 的契约与固定起点 8ca1e9e）"
handoff_index:
  - task_id: "2.5"
    work_package: WP5a
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    attempt: 1
    target_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
    evidence_type: DELIVERY
    evidence_id: deliver-wp5a-r1
    report_path: "reports/deliver-wp5a-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp5a（feat/wp5a，base=8ca1e9e）实现 R10/R15/R19 的基础层，固定为提交 0a99eeb（38 files changed, +15341；仅 clients/app/**）。工程骨架：package.json/tsconfig.json/vitest.config.ts/app.json/expo-env.d.ts/.gitignore 与 Expo Router 空壳路由（app/ 5 个文件只声明 data-route 等稳定选择器）；v1 只交付 Web，无任何 .native.ts（tasks.md 2.6 的裁定），app.json 的 platforms 只含 web。七层目录与单向依赖：src/layers.ts 把 app(1)→components(2)→features(3)→state(4)→sync-client(5)→domain(6)→protocol(7) 的序号固定为单一定义，src/platform/ 作横切面单列并禁止被内层反向依赖；src/layers.test.ts 遍历全部 .ts/.tsx 逐条核对 import 方向，判别力以回退源码实证（protocol→domain 的 import 使 4 passed/1 failed，且暴露出并修复了 Windows 反斜杠使 posix.dirname 返回 '.' 的真实缺陷）。src/protocol/ 逐份镜像 schemas/sync/v1/**（common/auth/sync/command/event/error/pairing）+ paths.ts/manifest.ts，字段名、可选性与枚举取值域一一对应，strict+exactOptionalPropertyTypes+noUncheckedIndexedAccess，无 any、不做宽容解析、不复制任何 schema/fixture 到 clients/app 下。src/domain/ 提供目录（只含 id/displayName/alias/counts，无任何可反推路径的字段）、会话摘要（origin 为判别联合、wire 的 sessionId/currentMode/version 不进入输出）、Agent 目录条目（覆盖层缺席即 unknown 且断言 every !== disconnected）、会话正文与上下文三态（unreported/zero_window/known）的视图模型。重叠区域按 Shared File Ownership 只建骨架：app/ 与 src/components/ 由 WP7 接管页面实现；未写 src/platform/（WP5b）与 src/sync-client//src/state/ 的实现（WP6）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    work_package: WP5a
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    attempt: 1
    target_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp5a-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check（cwd=.worktrees/wp5a，Node v22.22.0 / npm 10.9.4，仓库根 node_modules 以只读 junction 消费）在 0a99eeb 上 exit 0：十道合同门禁全绿——schemas 149 valid/51 invalid/41 event views、commands 13、errors 58、features 13、assets 17 schemas/213 fixtures/12 transcript vectors/20 negative vectors/2 SAS、acp 25 methods/11 updates/5 content blocks/3 tool content types/19 capabilities/8 invariants/10 test families、docs 415 relative links/9058 section refs/518 files、boundaries 12 crates、drift 36 DDL + 15 traits/96 methods、agentic 21 passed 0 failed。首轮 check:agentic 曾报 21 passed/1 failed（change/sync-scope-and-pwa-client 的 No deltas found），定位为该 worktree 内未被跟踪、仅含 reports/ 而无 specs/ 的变更目录残留（移开后 21/21、恢复后复现），与代码交付无关；处置是把报告与日志写到权威 planning root，worktree 保持无该目录。check:rust 首轮 exit 101，失败项为 crates/app/tests/daemon_lifecycle.rs 的既有非确定性 flake the_periodic_task_runs_again_after_one_full_cycle（left:0/right:1，套件 11 passed/1 failed/finished in 2.26s）；本包无任何 Rust 改动，故不声称 check:rust 通过——见报告 §8 与 §9。"
    source_evidence: reports/PV1-wp5a-r1.log
  - task_id: "3.9"
    work_package: WP5a
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    attempt: 1
    target_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/deliver-wp5a-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 0a99eeb 上的 clients/app 自身入口（cwd=.worktrees/wp5a/clients/app，独立 clients/app/node_modules 856 packages，Node v22.22.0 / npm 10.9.4，expo 54.0.37 / expo-router 6.0.24 / react-native 0.81.4 / react-native-web 0.21.3 / typescript 5.9.3 / vitest 3.2.7）三段均 exit 0：npx tsc --project tsconfig.json --noEmit 无诊断；npx vitest run --config vitest.config.ts 为 3 个 test file、41 passed / 0 failed（layers.test.ts 5 条 R10 分层与单向依赖；protocol/contract.test.ts 17 条 AC3 同源消费与镜像逐项对应；domain/domain.test.ts 19 条 R15/R17/R19/会话摘要/session.read 分页视图）；npx expo export --platform web 由 Metro 打包 684 modules 并产出 dist/index.html 与 1.03 MB bundle。未接入根 npm run check（plan 的裁决），未跑浏览器用例（TP4 范围）。"
    source_evidence: reports/PV3-wp5a-r1.log
checks:
  - id: PV1
    work_package: WP5a
    command: "npm run check (cwd=.worktrees/wp5a)"
    scope: "合同门禁：check 十道脚本（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）"
    environment: "cwd=.worktrees/wp5a；仓库根 node_modules 以只读 junction 消费；Node v22.22.0 / npm 10.9.4"
    exit_code: 0
    log_path: reports/PV1-wp5a-r1.log
    result: PASS
  - id: PV3
    work_package: WP5a
    command: "npx tsc --project tsconfig.json --noEmit && npx vitest run --config vitest.config.ts && npx expo export --platform web (cwd=.worktrees/wp5a/clients/app)"
    scope: "前端自身入口：类型检查 + 单元/契约测试 + Web 构建（plan 裁决不接入根 npm run check）"
    environment: "cwd=.worktrees/wp5a/clients/app；独立 clients/app/node_modules（856 packages）；Node v22.22.0 / npm 10.9.4；expo 54.0.37 / expo-router 6.0.24 / typescript 5.9.3 / vitest 3.2.7"
    exit_code: 0
    log_path: reports/PV3-wp5a-r1.log
    result: PASS
test_delivery:
  WP5a:
    kind: automated
    artifacts:
      - reports/PV3-wp5a-r1.log
      - reports/PV1-wp5a-r1.log
    basic_checks:
      - PV1
      - PV3
```
