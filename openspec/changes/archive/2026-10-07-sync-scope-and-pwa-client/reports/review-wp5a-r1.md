<!-- WP5a（MU3a 唯一成员）首轮独立代码检视报告（review-wp5a-r1，Round 1）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "2.5"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-wp5a-r1"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP5a 实现，也未继承作者实例 coder-w5a-r1 的任何对话；只携带派发说明与 plan.md/specs/design.md/docs/FRONTEND_DESIGN.md/AGENTS.md 的契约）"
target_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
scope: "WP5a 交付提交 0a99eeb 相对固定基线 8ca1e9e 的完整 diff（38 files / +15341，全部在 clients/app/**）。逐条核对 specs/pwa-web-client 的 R10/R15/R17/R19/R20、design.md D8、docs/FRONTEND_DESIGN.md §3/§5，检查七层单向依赖与 layers.test.ts 的判别力、src/protocol/ 与仓库根 schemas/sync/v1/** 的逐字段同形性、src/domain/ 视图模型的载荷隔离、工程范围与 v1 决策、AC3 证据的可核对性。不判用例集合是否齐备（交主 Agent Coverage Index / validator），不执行 E2E，不判 TP3/TP4 范围。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并、未运行任何写文件的构建或测试。"
issues: "0 CRITICAL / 0 MAJOR；4 MINOR / 2 SUGGESTION。无 patch 引入的阻断缺陷。"
result: PASS
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp5a-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1-wp5a-r1.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV3-wp5a-r1.log"
resource_cleanup: NOT_APPLICABLE（只读检视，未创建资源）

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-wp5a-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.5"
    work_package: WP5a
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "0a99eebd696cd18e9f44a86bce08a406e56284b7"
    evidence_type: REVIEW
    evidence_id: review-wp5a-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-wp5a-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "新建独立子 Agent（未参与 WP5a 实现）对固定交付提交 0a99eeb（相对固定基线 8ca1e9e，38 files/+15341，全部在 clients/app/**）做只读检视；逐条核对 R10/R15/R17/R19/R20 与 D8/FRONTEND_DESIGN §3，逐字段对照仓库根 schemas/sync/v1/** 核验 src/protocol/ 镜像，逐层核对 src/domain/ 视图模型的载荷隔离与 layers.test.ts 的判别力（含以只读 Node 复现反向依赖与绕检路径），核实工程范围与 v1 决策、AC3 证据链。PV1/PV3 证据读自主 Agent 的交付报告与日志，未自行复跑。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-wp5a-r1` / 1 |
| Review Type / Stage | `branch`（工作包交付前检视；MU3a 候选之前） |
| Work Package | WP5a（交付单元 MU3a 唯一成员，Order 4，independent） |
| Repository / 检视工作区 | `D:\Project\acp-remote` / `D:\Project\acp-remote\.worktrees\wp5a`（只读；`git status --porcelain` 为空） |
| Base Revision | `8ca1e9e`（`refactor(agent-host): 节点级事件改用专用 NodeEventSink 类型`） |
| Target Revision | `0a99eebd696cd18e9f44a86bce08a406e56284b7`（分支 `feat/wp5a`，单提交） |
| 真实 diff | `git diff --stat 8ca1e9e 0a99eeb` = 38 files changed, 15341 insertions(+)（含 `package-lock.json` 11939 行）；`git diff --name-only` 过滤 `clients/app/` 后为空 |
| 读取的规则 | `AGENTS.md` §3/§5、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md` |
| 读取的需求 | `specs/pwa-web-client/spec.md`（R10、R15、R17、R19、R20）、`design.md` D1/D2/D3/D6/D7/D8、`docs/FRONTEND_DESIGN.md` §2.1/§3/§5/§6/§7、`tasks.md` 2.5/2.6/4.3、`plan.md`（WP5a/WP5b/WP6/WP7 行、Shared File Ownership、Runtime Resources） |
| 机器合同（被检对象的上游） | `schemas/sync/v1/{message,common,auth,sync,command,event,event-views,error,pairing}.schema.json`、`fixtures/sync/v1/manifest.json` 与其 `README.md`、`compatibility/errors/v1/errors.json` |
| 使用的验证证据 | `reports/deliver-wp5a-r1.md`（仅作待核实的自评读取，**不作为检视证据**）；`reports/PV1-wp5a-r1.log`、`reports/PV3-wp5a-r1.log`（主 Agent 的 Project Verify 证据，只读消费）。本 reviewer **未**自行复跑任何构建或测试。 |
| 限制 | 不判「用例是否充分」与风险盲区（交 validator / 主 Agent Coverage Index）；不执行 E2E（TP3/TP4 范围）；不判 `check:rust` 的全量通过（本包无 Rust 改动）；不判设备身份与真实浏览器渲染（WP5b/TP4）。 |

### 证据核对

- `PV3`（`reports/PV3-wp5a-r1.log`）：目标差异 = `0a99eeb` 在 `clients/app` 内 `npm run typecheck`（exit 0）、`npm run test`（3 test files / 41 passed，exit 0）、`npx expo export --platform web`（684 modules，产出 `dist/index.html`，exit 0）。适用依据 = 该提交 + `clients/app/node_modules/`（856 packages）。本 reviewer **未**复跑。
- `PV1`（`reports/PV1-wp5a-r1.log`）：`npm run check` 十道合同门禁 exit 0（`check:schemas` 149 valid / 51 invalid / 41 views bound；`check:commands` 13；`check:errors` 58 codes；`check:features` 13；`check:assets` 17 schemas / 213 fixture files；`check:acp`；`check:docs` 415 links / 9058 refs；`check:boundaries` 12 crates；`check:drift`；`check:agentic` 21 passed / 0 failed）。同日志 `check:rust` 首轮 exit 101，失败项为 `crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_again_after_one_full_cycle`（`left: 0 / right: 1`）。
- `check:rust` 归因（独立核对，不依赖作者转述）：本 diff **不含任何 `crates/**` 改动**（`git diff --name-only 8ca1e9e 0a99eeb` 过滤 `clients/app/` 后为空），因此该失败在因果上不可能由本包引入；作者另报该用例既往已登记为按 60s 标记判定的非确定性 flake（WP4 报告）。本 reviewer 认领该归因，但**不**把 `check:rust` 记为已通过。

---

## A. 分层与依赖方向（R10） — **符合，含 3 项非阻断缺口**

### A.1 `layers.ts` 与 `docs/FRONTEND_DESIGN.md` 的分层定义是否一致 — **一致**

`src/layers.ts:25-33` 的 `LAYER_ORDER`（外层→内层，序号递增）与 `docs/FRONTEND_DESIGN.md:45-57` 的结构图逐级对齐：

| FRONTEND_DESIGN §3 结构图（自上而下） | `layerOfSourcePath` 的目录 | `LAYER_ORDER` |
| --- | --- | --- |
| Screens / Components | `app/`、`src/components/` | 1、2 |
| Feature Controllers + Client State Machines | `src/features/`、`src/state/` | 3、4 |
| Client Application Services | `src/sync-client/` | 5 |
| Sync Client / Protocol Types | `src/domain/`、`src/protocol/` | 6、7 |
| Platform Ports | `src/platform/`（横切，`PLATFORM_LAYER`，不在序号链） | — |

七层数量、职责与顺序一致；`src/platform/` 按 `FRONTEND_DESIGN.md:95`（「平台差异集中在 `platform/`」）与 `design.md` D8 单列为横切面不占序号，`layers.ts:36-42` 的注释与之一致。`src/` 下实际存在的首段目录集合恰为 `components/domain/features/protocol/state/sync-client`（加 `layers.ts`/`layers.test.ts` 两个非层文件），与七层减 `app/` 完全对应，无遗漏层、无未登记目录。

**但**：`app` 层在实现里是 `clients/app/app/`，而测试的扫描根 `REPO_APP_ROOT = resolve(APP_ROOT, "src")`（`src/layers.test.ts:34,73`）；见 A.4 / `review-wp5a-r1-F3`。

### A.2 判别力对照是否成立、断言是否非恒真 — **成立，且断言非恒真**

- **断言非恒真**：`isDependencyAllowed(from, to) = LAYER_ORDER[to] >= LAYER_ORDER[from]`（`src/layers.ts:82-87`）是真值依赖的判定，不是常量。`src/layers.test.ts:129-130` 显式断言 `isDependencyAllowed("domain","state") === false` 与 `isDependencyAllowed("protocol","domain") === false`，把「反向为假」钉死；`layers.test.ts:110-122` 另断言序号表覆盖七层且单调。这三条使「序号表为空/全等/倒序」一类的退化实现无法通过。
- **判别力对照成立**：作者报「在 `src/protocol/common.ts` 注入 `import type { DirectoryView } from "../domain/directory"` 后 4 passed / 1 failed」。本 reviewer 以只读 Node 复现该解析路径：`localRelativeTarget(src/protocol/common.ts, "../domain/directory") → "src/domain/directory" → layer = domain`，而 `isDependencyAllowed("protocol","domain") === false`，故必然落入 `violations`（`layers.test.ts:99-103`）并使 `expect(violations).toEqual([])` 失败。对照有效，方向判定确实由序号表驱动而非目录名规范。

### A.3 反斜杠缺陷的修复是否彻底 — **彻底（POSIX 语义下）**

原缺陷：Windows 上 `relative()` 返回反斜杠路径，`posix.dirname("src\\protocol\\common.ts")` 返回 `"."`。修复（`src/layers.test.ts:64-68`）先 `replaceAll("\\", "/")` 再 `posix.normalize(posix.join(posix.dirname(fromRelative), specifier))`。逐点核对：

- `fromRelative` 已归一化，`posix.dirname` 不再遇到反斜杠；
- `specifier` 来自源文件源码，无论作者用 `/` 还是 `\` 书写（TS 允许 `import x from "..\\domain"`），`posix.join` 中的 `\` 在 posix 语义下是**普通字符**而非分隔符——`posix.join("src/protocol", "..\\domain")` 得 `"src/protocol/..\\domain"`，`posix.normalize` 不消该 `..`，结果为 `"src/protocol/..\\domain"`，`startsWith("src/")` 为真 → `layerOfSourcePath` 取首段 `"protocol"` → **误判为同级/自身层**。
- **触发条件**：新代码用反斜杠书写相对 import 说明符。**影响**：`protocol → domain` 之类的反向依赖可被静默判为合法（`protocol → protocol`）。**评估**：仓库内**无**反斜杠说明符的既有先例，TS/ESLint 生态默认禁止；检出难度也低。按 `P3`（Info）登记到 `review-wp5a-r1-F2` 的建议项，不作为独立发现。
- **`localRelativeTarget` 的 `!target.startsWith("src/")` 守卫**：同一层内的裸 barrel（`"../domain"`）会命中此守卫并返回 `null` —— 见 `review-wp5a-r1-F2`，这是比反斜杠更现实的绕检路径。

### A.4 `app` 层的方向与平台禁令是否被实际执行 — **未被执行（F3）**

`collectSourceFiles(REPO_APP_ROOT)` 的根是 `clients/app/src`，因此 `clients/app/app/**`（5 个路由）**从未进入** `files`；而 `layerOfSourcePath(relative(APP_ROOT, file))` 对 `src/<layer>/…` 只可能返回 `components/domain/features/protocol/state/sync-client/platform`。两条后果：

1. `layers.test.ts:79-108` 的「没有任何跨层反向 import」永远不会对 `app` 层产生 `from`，页面层的方向约束实为空断言；
2. `layers.test.ts:133-147` 的「页面层不得直接 import 平台 API 模块」对 `from === "app"` 永不成立，只剩 `components` 生效——而该用例标题（`layers.test.ts:133`）与文件头注释（`layers.test.ts:22-23`「页面与组件」）都声称覆盖页面。

本 reviewer 以只读 Node 复现确认（`collectSourceFiles` 的 26 个文件首段集合不含 `app`）。当前 `app/` 只有空壳路由、不违反约束，故为 MINOR。

### A.5 `platform` 作为跨切层的限制 — **方向限制正确**

- 平台层**不得被任何层 import**：`layers.test.ts:93-98` 对 `to === PLATFORM_LAYER` 一律记违规（「平台能力只能由更外层装配」）。`src/platform/` 在本 diff 中不存在（`git ls-files` 无 `platform`），因此当前无触发。
- 平台层自身的方向不受检：`layers.test.ts:85` 对 `from === PLATFORM_LAYER` 直接 `continue`。按 `layers.ts:10-16` 的声明（平台端口在链条之外、由组合根装配），这是**有意的**设计而非缺口；但它意味着 WP5b 的 `src/platform/*.ts` 可反向 import 任意层而不被该测试发现。仓库内**不存在**被指名的组合根（`docs/FRONTEND_DESIGN.md`/`plan.md` 均无「组合根」字样），故此处按「有意取舍，建议登记」处理，不单列发现。
- 平台 API 的**包名**禁令仅覆盖 5 个裸说明符（`layers.test.ts:134` 的 `["ws","socket.io-client","idb","localforage","expo-secure-store"]`），不含 `node:*`、`expo-sqlite`、`*/secure-store` 等；见 `review-wp5a-r1-F5`（SUGGESTION）。

---

## B. 协议层的严格性与单一来源（R10） — **符合，含 1 项 MINOR（F4）**

**抽查方式**：以 Python 直接解析仓库根 `schemas/sync/v1/*.schema.json`，与 `src/protocol/*.ts` 逐字段对照，并统计集合差。

### B.1 逐字段同形性（抽查 ≥3 个 `$defs` 的必填、可选性、枚举取值域）

| 被查对象 | schema 侧（真实字节） | TS 镜像 | 结论 |
| --- | --- | --- | --- |
| `file.changed` 的可选三字段 | `required = ["changeId","kind","displayPath","summary"]`；`properties` 恰 7 键，`addedLines`/`deletedLines`/`outsideWorkspace` 均不在 `required`；`additionalProperties: true` | `event.ts` `FileChangedView`：4 必填 + `addedLines?`/`deletedLines?`/`outsideWorkspace?` + `& OpenView` | **一致**（`contract.test.ts` 的 17 条中亦有专项断言） |
| `agent.connected` / `agent.disconnected` 的 `state` | 各为 `enum: ["connected"]` / `enum: ["disconnected"]`，且**两键 `required` 不含 `error`** | `AgentConnectedState = "connected"`、`AgentDisconnectedState = "disconnected"`（`event.ts:114-118`）；`error?: PublicError` | **一致**（取值唯一、`error` 可选） |
| `sessionRead` 的 `before`/`limit` 与结果 `hasEarlier` | `sessionReadBefore.required = ["createdAt","messageId"]`、`additionalProperties:false`；`sessionRead.payload.properties = {include,before,limit}`、`required = ["include"]`、`additionalProperties:false`；`sessionReadResult.required = ["sessionId","resources","hasEarlier"]` | `SessionReadBefore` 两键必填；`payload` 上 `include` 必填、`before?`/`limit?`；`SessionReadResult.hasEarlier: boolean` 必填 | **一致**（`limit` 无 `maximum` 亦一致） |

**全量集合差核对（机器）**：

- `EventType` 联合 34 项 = `event-views.schema.json` 的 34 个 `$defs` 键（TS-only 与 schema-only 均为空集）；`KNOWN_EVENT_TYPES` 34 项无重复且与 `EventType` 同集；`EventViewByType` 34 键同集。
- `ErrorCode` 34 项 = `common.schema.json#/$defs/errorCode.enum` 34 项（双向差为空，含 `protocol.sequence_invalid`、`internal.unavailable`）。
- `CommandName` 12 项 = `command.schema.json#/$defs/commandName.enum` 12 项（与 `compatibility/errors` 的 `session.resume` 不在其列的设计一致）。
- `SessionState` 7 项 = `common.sessionSummary.state.enum` 7 项。
- `snapshotResource = "sessions"|"workspaces"|"agents"`、`SnapshotChunkCount = 1|2|3` 与 schema 一致；`SessionReadResult.resources` 五键可选、`configOptions`/`version` 归组形状与 `snapshotItem.config_options` 一致。
- `OriginBlock` 两支（local 仅 `kind`；remote 另带 `ownerNodeId`/`exportId`/`originEpoch`/`online`）与 `originBlock.oneOf` 一致；`RawAcp` 两支与 `rawAcp` 一致；`AgentContentBlock` 四支与 `agentContentBlock.oneOf` 一致。

### B.2 「无 `any`、`T | null` 而非 `T?`、`additionalProperties:false` 的对象用精确接口」 — **部分成立，须区分「必填可空键」与「可选键」**

`strict` + `noImplicitAny` 确在 `clients/app/tsconfig.json:23-24` 开启，且 `npx tsc --noEmit` 由 `package.json` 的 `typecheck` 实际执行（PV3 exit 0）。全 `src/protocol/`、`src/domain/` 无 `any` 出现（仅注释里出现「不用 `any`」字样）。**但**「`T | null` 写成 `T | null` 而不是 `T?`」只对**schema `required` 内的可空键**成立；schema 里**不在 `required`** 的键，镜像写成 `?:` 是**正确的**：

- 正确用 `T | null`：`SessionSummary.title`、`ModeState.currentModeId`、`ConfigOptionView.description`/`category`、`PublicError` 各键、`SessionReadBefore` 两键、`commandStatusRecord.acceptedAt`/`terminalAt`/`terminalEventId`/`error`。
- 正确用 `?:`（schema 未 required）：`sessionRead.payload.before/limit`、`sessionReadResult.resources.*`（5 键）、`configOptionView.options`、`sessionSummary.workspace`、`event.payload.acp`、`agent.message.delta.block`、`file.changed` 三键、`agent.disconnected.error`、`manifest` 的 `viewSchema`/`viewDef`。
- **未发现**把 `required` 内的键误写成 `?:`，或把可选键误写成必填的站点。

### B.3 34 个 eventType 的开放视图是否**只对开放对象**保留索引签名 — **是**

`event-views.schema.json` 的 34 个 `$defs` **全部** `additionalProperties: true`（机器核对无例外），因此统一附加 `OpenView = Record<string, unknown>` 与 schema 一致，不构成「对封闭对象滥加索引签名」。`session.plan.changed.entries[]` 与 `session.commands.changed.commands[]` 的**内层元素**同样是 `additionalProperties: true`，镜像对它们也加了 `& OpenView`，同形。反向地，`event.schema.json` 是 `additionalProperties: false`（信封与 `body`、`payload` 均为 closed），镜像逐字段精确建模、不加索引签名——方向正确。`event.payload.view` 保留 `Record<string, unknown>` 与 schema 的 `view: {type:"object"}` 一致。

### B.4 单一来源 — **成立**

`src/protocol/paths.ts:19-31` 由 `import.meta.url` 上溯四级得仓库根（`protocol → src → app → clients → <repo>`），`clients/app/` 下**无** `schemas/`、`fixtures/` 副本（`contract.test.ts:53-57` 断言 `existsSync` 为假）。`src/protocol/` 九份入口文件与 `SYNC_SCHEMA_FILES`（`paths.ts:34-44`）逐一对应 `schemas/sync/v1/` 实存文件（README.md 除外），无缺无余。

### B.5 `manifest` 的严格解析与 `viewDef` 未校验 — **F4（MINOR）**

`src/protocol/manifest.ts` 声明「**不做宽容解析**：字段缺失或类型不符直接抛错」，`assertManifest` 逐条核对 `fixture`/`schema`/`valid`/（invalid 的）`expectedKeyword`，且对 **`viewSchema`/`viewDef` 的成对性与格式不作任何核对**。后果：

- `viewSchema` 已声明（`ValidFixtureCase.viewSchema?: string`），但 `resolveFixtureCase`（`manifest.ts:66-71`）把它 `resolve(base, entry.viewSchema)` 后**不校验是否落在 `schemas/sync/v1/` 之下**——若 schema 值为绝对路径，`resolve` 直接采用，可越过「同一份资产」边界；
- `viewDef` 在 `manifest.ts:41-43` 声明后**从未被读取、解析或比较**——既无 `#/$defs/<eventType>` 形状校验，也不核对 `$defs` 键存在性。含 `viewDef` 而缺 `viewSchema` 的 case 会被静默解析为 `viewSchemaPath: null`（`viewDef` 被丢弃）；
- `valid` 为 `false` 的 case 携带 `viewSchema` 时同样静默丢弃。

**触发**：manifest 演进为「用 `viewDef` 但不写 `viewSchema`」或 `viewSchema` 写成绝对/越界路径；**影响**：合同门禁（仓库根 ajv 脚本）与前端「读同一份资产」的自证同时失真，且 `assertManifest` 的「严格」声明与实现不符；**建议**：在 `assertManifest` 内补 `viewDef`/`viewSchema` 的成对性与 `#/$defs/` 前缀校验，并对解析后路径断言落在 `syncSchemaDir` 之下。

### B.6 严格模式标志是否被实际执行 — **是**

`clients/app/tsconfig.json:23-38` 含 `strict`、`noImplicitAny`、`strictNullChecks`、`exactOptionalPropertyTypes`、`noUncheckedIndexedAccess`、`noImplicitOverride`、`noImplicitReturns`、`noFallthroughCasesInSwitch`、`noPropertyAccessFromIndexSignature`、`noUnusedLocals`、`noUnusedParameters`、`useUnknownInCatchVariables`，由 PV3 的 `npx tsc --project tsconfig.json --noEmit`（exit 0）**实际执行**。`include` 覆盖 `app/**`、`src/**`、`expo-env.d.ts`、`vitest.config.ts`，故被检文件集合与测试/构建一致。

---

## C. 视图模型不暴露协议载荷（R10 / R15 / R19） — **符合**

### C.1 `DirectoryView` 不含任何可还原路径的字段 — **成立**

`src/domain/directory.ts:32-40` 的输出键恰为 `id`/`displayName`/`alias`/`counts`；`counts` 为 `total`/`active`/`pending` 三个计数。`id === alias`（`directory.ts:106`），`alias` 是 `WorkspaceRef.alias`（wire 上唯一的目录标识），`displayName` 可重复。无 `path`/`cwd`/`displayPath`/父目录/末段/`id` 派生自路径的字段。`domain.test.ts` 的「目录项只含展示名与别名」用例对键集合做了穷尽断言（`["alias","counts","displayName","id"]`）。同名目录不以路径消歧：`directory.ts:103-113` 逐项 `map`，不按 `displayName` 分组或合并，`domain.test.ts` 的对照用例断言两项都被保留。

### C.2 两种空态可区分 — **成立**

`DirectoryEmptyState = "no_directories" | "directories_without_sessions"`（`directory.ts:43`），`buildDirectoryViews` 在 `workspaces.length === 0` 时返回前者（`directory.ts:87-89`），在「有目录但 `total` 全为零」时返回后者（`directory.ts:114-118`）。两条分支的判据互斥且均有独立用例（`domain.test.ts` 的「没有任何目录时是 no_directories」与「有目录但都无会话时是 directories_without_sessions」）。汇总由 `SessionSummary.state` 得出（`IS_ACTIVE`/`IS_PENDING` 静态词表，`directory.ts:49-70`），故断连时仍可渲染（数据来自快照资源参数）。

### C.3 `AgentCatalogEntryView` 的连接状态是可选覆盖层 — **成立**

列表来自快照 `agents` 资源（`buildAgentCatalogView(agents, overlay)`，`agent-catalog.ts:62-85`），覆盖层由调用方（WP6）构造。覆盖层缺席时 `state?.state ?? "unknown"`（`agent-catalog.ts:75`），既不移除条目也不改写为 `disconnected`；`disconnectReason` 仅在 `disconnected` 且带 `reason` 时非 `null`（`agent-catalog.ts:76`）。`domain.test.ts` 断言覆盖层缺席时全部条目 `connection === "unknown"` 且 `every(entry => entry.connection !== "disconnected")`——把「状态未知 ≠ 已断开」钉成断言。同名 Agent 以 `id` 区分、不按名合并（`agent-catalog.ts:62-85` 无按名分组），`domain.test.ts` 有专项用例。

### C.4 `ContextUsageView` 三态无「零即已用」非法态 — **成立**

`ContextUsageView = {kind:"unreported"} | {kind:"zero_window"} | {kind:"known"; used; size; percent}`（`conversation.ts:57-66`）。`buildContextUsageView(null) → unreported`；`size === 0 → zero_window`（在算 `percent` 之前返回，`conversation.ts:84-89`），不产生 `NaN`/`Infinity`；仅 `size > 0` 才给 `known`。不存在「`used=0` 即已用」的态。`contextUsageLabel` 三态各有文案（`conversation.ts:70-79`）。

### C.5 `SessionSummaryView` 的载荷隔离 — **成立（附 1 项 SUGGESTION）**

输出键为 `id`/`title`/`agentName`/`agentId`/`state`/`origin`/`directoryAlias`/`updatedAt`/`createdAt`（`session-summary.ts:51-62`），不含 wire 的 `sessionId`/`currentMode`/`version`/`originBlock`；`origin` 是 `local`/`remote` 判别联合，`local` 分支**不含** `online`（`session-summary.ts:43-46`），组件无法在 `local` 上误读 `online`。`domain.test.ts` 逐键断言 + 「local 分支不含 online」专项用例。**SUGGESTION**：`SessionSummaryView.agentId: string`（`session-summary.ts:54`）原样透出 wire 的 `agentId`；它属标识而非路径/凭据，与 `AgentCatalogEntryView.id` 同源，但若被视作「协议载荷」，建议在 `domain/index.ts` 的注释里明确登记为有意保留的标识面。

---

## D. 工程范围与 v1 决策 — **符合**

### D.1 只交付 Web — **成立，且裁决有出处**

`clients/app/app.json` 的 `platforms: ["web"]`；全仓无 `.native.ts`/`.native.tsx`（`git ls-files` 命中的唯一含 `native.` 的文件是 `.agents/skills/ui-ux-pro-max/data/stacks/react-native.csv`，与本 diff 无关，且在基线即存在）。「只交付 Web、原生端不在本次范围」的裁决**确实存在于** `tasks.md` 2.6（「**不写 `.native.ts`**：v1 只交付 Web，原生端不在本次范围，该决定写进本任务而非靠「没人做」达成」）与 `docs/FRONTEND_DESIGN.md` §2.1（「前端的首个交付只实现 Web/PWA，不实现 Android/iOS 原生包」）。**非作者自行缩小范围。**

### D.2 Vite 与 Metro 的关系 — **作者对 plan 措辞的解读成立**

- `plan.md` 的 Runtime Resources 只为 **TP4** 登记「前端静态服务器（Vite dev server）」，并在同格注明 Round 8 的 F33 把 Metro 从该行移除、理由是「Metro 是 React Native 打包器」。该行的限定语义是「**仅验证环境**、不连真实 Daemon、只服务 fixtures」。
- 本包的处理：产品打包用 Expo 自带 Metro（`package.json` 的 `export:web = expo export --platform web`，PV3 exit 0，684 modules），Vite 只作为 TP4 的静态文件服务器（`service = vite preview`）。
- **判定**：F33 的意图是「v1 不构建原生包」，不是「禁止使用 Expo 的 web 打包器」；`docs/FRONTEND_DESIGN.md` §2.1 与 `AGENTS.md` §5 要求 Expo/Expo Router 通用工程，若一并排除 Metro 则无法成立。作者的解读与 plan 原文不冲突，属**有依据的措辞解读**，非范围收缩。`react-native 0.81.4`/`react-native-web ~0.21.0` 是 Expo 通用工程的传递与 web 别名需要（`expo-router` 的依赖链），不是引入原生打包路径：`app.json` 只含 `web` 平台，`expo export --platform web` 产出单页静态资源，无原生产物。

### D.3 写入范围未越界 — **成立**

`git diff --name-only 8ca1e9e 0a99eeb` 过滤 `clients/app/` 后为空；`schemas/**`、`fixtures/**`、`crates/**`、`docs/**`、根 `package.json`/`package-lock.json`、`scripts/**`、`.github/**` 均**零改动**（`git diff --name-only | grep -E '^(schemas|fixtures|crates|docs|scripts|\.github)/|^package(-lock)?\.json$'` 无输出）。

- `src/platform/`（WP5b）：**不存在**（`git ls-files clients/app/src | grep platform` 无输出）；
- `src/sync-client/`、`src/state/`（WP6）：仅类型面（`sync-client/index.ts` 27 行、`state/index.ts` 59 行，均为类型/常量，无实现）；
- `src/features/`（WP7）：仅 `FeatureViewModels` 消费面（31 行）；
- `app/` 与 `src/components/`（与 WP7 重叠区）：`app/` 5 个文件共 65 行、`src/components/` 3 个文件共 112 行，全部只声明 `data-route`/`data-session-id` 等稳定选择器与公共 props/文案词表，**无页面实现**。区域收窄与 `plan.md` Shared File Ownership 的「WP5a 只建空壳路由与共享展示组件的骨架，WP7 接管」登记一致。

**范围外观察（SUGGESTION，不计为本包缺陷）**：`src/layers.test.ts`、`src/domain/domain.test.ts`、`src/protocol/contract.test.ts` 三个 `.test.ts` 落在 plan 登记的 WP5a Write Scope 之外（`plan.md` 的 Shared File Ownership 明确「TP3 独占 `*.test.ts`」；WP5a 的 Write Scope 只列 `package.json`、`app/`、`src/{domain,protocol,components}/`）。由 TD-4 取；见 `review-wp5a-r1-F6`。另外，作者自报的五个工程配置文件（`tsconfig.json`/`vitest.config.ts`/`app.json`/`expo-env.d.ts`/`.gitignore`）同样不在字面清单内——其理由（PV3 要求类型检查与测试脚本可跑）成立，且不与他包重叠。

### D.4 未把 Vite 当作打包器 — **成立**（`vite` 只在 `devDependencies`，用法仅 `vite preview`；`vitest` 是测试运行器）

---

## E. AC3 的可核对证据 — **成立且可独立复核**

`tasks.md` 4.3 判 AC3 为「`clients/app` 读**同一份** `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`」。证据链：

1. **路径解析**：`paths.ts:19-31` 由 `import.meta.url` 上溯四级；`contract.test.ts:41-48` 断言 `MANIFEST_REPO_ROOT` 含 `package.json` 与 `Cargo.toml`、且不以 `/clients/app` 结尾（同时覆盖 Windows 反斜杠形式的 `join("clients","app")`）。
2. **无副本**：`contract.test.ts:53-57` 断言 `clients/app/schemas` 与 `clients/app/fixtures` 均不存在。本 reviewer 独立确认 `schemas/`、`fixtures/` 在 `clients/app/` 下确实不存在。
3. **真实文件**：`contract.test.ts:59-70` 断言 `syncManifestPath === join(syncFixtureDir,"manifest.json")`、manifest 存在、九份 `SYNC_SCHEMA_FILES` 均存在。
4. **逐 case 可解析**：`contract.test.ts:72-84` 对 manifest 的**全部** case 断言 `resolveFixtureCase` 的 `fixturePath`/`schemaPath` 存在，且 `schemaPath` 落在 `/schemas/sync/v1/` 之下。本 reviewer 独立核对：`manifest.json` 的 126 条 case、`schema` 值集合为 `common/event-views/message/pairing` 四个 schema（均位于仓库根 `schemas/sync/v1/`）、`viewSchema` 仅 `event-views.schema.json`——与 `SYNC_SCHEMA_FILES` 清单断言一致。
5. **清单一致**：`contract.test.ts:86-97` 断言 manifest 引用的每个 schema 都在 `SYNC_SCHEMA_FILES` 内。
6. **逐项对应**：`contract.test.ts:100-205` 的 12 条用例**从 schema 现场读出** `required`/`properties`/`enum` 与镜像比对（`sessionSummary` 九项 required + 十项 properties、`state` 七取值、`snapshotResource` 三取值与 `chunkCount.maximum === 3`、`sessionReadBefore.required`、`sessionRead.payload` 三键、`sessionReadResult.required` 含 `hasEarlier`、`sessionCreate.payload` 两引用 + `additionalProperties:false`、`file.changed` 四项 required + 七项 properties、两个 `state` 唯一取值、`event-views.$defs` 数量 = `KNOWN_EVENT_TYPE_COUNT`），而非抄常量。
7. **测试侧不复制样例**：`readSchema` 从 `syncSchemaDir` 直读；`manifest.ts` 只解析路径并把结果交给调用方。断言「未复制」本身有机器证据（第 2 项）。

**结论**：AC3 的证据可核对、可复核、真实。**TP3 属本包的部分**（`fixtures/sync/v1/transcripts/device-proof.json` 的跨语言字节级对拍）**不在**本包范围（`tasks.md` 4.3 的该半部分依赖 `code:WP5a`/`code:WP5b`/`code:WP6`/`code:WP7` 与固定向量），本包只交类型化消费与上述可核对证据——与作者的自述一致。

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| `review-wp5a-r1-F1` | MINOR | `clients/app/src/protocol/index.ts:19-22`（`export { … } from "./manifest"`、`export { repoRoot, … } from "./paths"`）；被 `src/protocol/paths.ts:16-17` 与 `src/protocol/manifest.ts:16-17` 的 `node:url`/`node:fs`/`node:path` 支撑 | `index.ts:10-13` 把本文件声明为「上游消费方（`src/sync-client/`、`src/state/`、`src/features/`）只从这里 import，不直接指向某个 schema 模块」的**唯一**导入面。该导入面在第 19-22 行引入了**运行期**（非 `export type *`）对 Node 内置模块的依赖。Expo web 导出用 Metro，其 `@expo/cli` 的 Node externals 解析对非 server 平台（含 `web`）把 Node 内置模块**垫成空模块**（`node_modules/expo/node_modules/@expo/cli/build/src/start/server/metro/withMetroMultiPlatform.js:400-414`：`result ?? { type: 'empty' }`），仓库内无任何 `node-libs-*` / `readable-stream` 之类 polyfill 包；`paths.ts:19` 的 `dirname(fileURLToPath(import.meta.url))` 在模块求值期调用被垫空的 `node:url`。本 reviewer 已读该解析器源码确认垫空分支，且 `expo export` 之所以 exit 0 是因为当前唯一入口（`app/_layout.tsx` 的 `expo-router` 链）**没有**任何模块 import 该 barrel。 | 触发条件：WP6/WP7 按设计从 `../protocol`（或 `@/protocol`）取值时，`paths.ts`/`manifest.ts` 一并进入 web bundle；`protocol/` 的公开导入面在**运行时**不可用（模块求值期 `fileURLToPath`/`dirname` 失效，或调用 `readManifest` 时 `readFileSync` 为 `undefined`）。当前不触发（无消费者），故不阻断本包 PV3。 | 把 Node-only 的 `paths.ts`/`manifest.ts` 从 `index.ts` 的运行期导出面移出（例如只保留 `export type *`，或以独立子路径 `protocol/node` 暴露给测试），使 `SyncWireMessage` 等纯类型/纯函数入口不牵连 `node:*`。 | 不适用（首轮） |
| `review-wp5a-r1-F2` | MINOR | `clients/app/src/layers.test.ts:62-70`（`localRelativeTarget` 的 `!target.startsWith("src/")` 守卫）、`src/layers.ts:63-74`（`layerOfSourcePath` 的 `slash === -1 → null`）、`src/layers.test.ts:51-59`（`importedSpecifiers` 正则） | 已在本仓库**实际存在**两类绕过方向：<br>(a) 说明符为**裸 barrel** `"../domain"`：`localRelativeTarget` 得 `"src/domain"`，`layerOfSourcePath` 因 `indexOf("/") === -1` 返回 `null`，`layers.test.ts:92` 直接 `continue`。`src/features/index.ts:17`（`} from "../domain";`）与 `src/sync-client/index.ts:12`（`from "../protocol"`）**已经**使用这种写法——即 `protocol → domain` 一类反向依赖只要写成裸 barrel 即可不被发现。本 reviewer 以只读 Node 复现：`layerOfSourcePath("src/domain") → null`，`isDependencyAllowed` 根本不被求值。<br>(b) 说明符为**副作用/动态导入** `import "../domain/x"` / `await import("../domain")`：`importedSpecifiers` 的正则（`layers.test.ts:55-57`）要求 `from` 子句，二者不匹配，返回空数组。本 reviewer 以只读 Node 复现该正则对两类写法均返回 `[]`。 | R10 的单向依赖判定（`layers.test.ts:79-108` 的 4 passed/1 failed 判别力对照只覆盖 `from "…"` 形式）可被上述任一写法静默绕过；作者报告的「回退源码实证」对照因此不足以说明全量覆盖。当前无违规，故不阻断。 | 在 `layerOfSourcePath` 前先 `posix.dirname`/剥掉末段文件名，或在 `localRelativeTarget` 内对 `target` 追加 `/index` 重解析；并把 `importedSpecifiers` 同时匹配 `import "…"`、`import("…")`、`require("…")` 三种形式（或改用 TS AST）。 | 不适用（首轮） |
| `review-wp5a-r1-F3` | MINOR | `clients/app/src/layers.test.ts:34,73`（`REPO_APP_ROOT = resolve(APP_ROOT,"src")` 作为扫描根）、`src/layers.test.ts:83`（`layerOfSourcePath(relative(APP_ROOT,file))` 永不返回 `"app"`）、`src/layers.test.ts:133-147` | 扫描根是 `clients/app/src`，`clients/app/app/**`（5 个路由）从未进入 `files`；`src/<layer>/…` 的 `relative(APP_ROOT,file)` 首段不可能是 `app`。本 reviewer 以只读 Node 复现 `collectSourceFiles`：26 个文件的首段集合为 `{components,domain,features,protocol,state,sync-client}`（加两个非层文件），**无 `app`**。 | `layers.test.ts:79-108` 对页面层的方向约束、以及 `layers.test.ts:133-147` 标题为「**页面层**不得直接 import 平台 API 模块」的平台禁令，对 `app/` **实为空断言**（仅 `components` 生效）；文件头注释 `layers.test.ts:22-23`「页面与组件」的表述强于实现。当前 `app/` 为空壳路由、不违反约束。 | 让 `collectSourceFiles` 同时覆盖 `clients/app/app`（并对 `app/` 前缀单独映射到 `app` 层），或在断言里显式声明「`app/` 由 WP7 的用例覆盖」并降级本用例标题。 | 不适用（首轮） |
| `review-wp5a-r1-F4` | MINOR | `clients/app/src/protocol/manifest.ts:38-44`（`viewSchema?`/`viewDef?` 声明）、`manifest.ts:66-71`（`resolveFixtureCase` 只用 `viewSchema`）、`manifest.ts:106-137`（`assertManifest` 不核对二者）；实际使用点 `src/protocol/contract.test.ts:150` | `viewDef` 声明后**从未被读取、解析或比较**：无 `#/$defs/<eventType>` 形状校验，也不核对 `$defs` 键存在性（manifest 中 9 条 `schemaPointer` 与全部 `viewDef` 均未被前端消费）；`viewSchema` 解析后**不校验落在 `syncSchemaDir` 之下**（`resolve` 对绝对路径直接采用）；含 `viewDef` 而缺 `viewSchema` 的 case 会被静默解析为 `viewSchemaPath: null`；`valid:false` 携带二者时同样静默丢弃。 | 与本模块自述的「**不做宽容解析**：字段缺失或类型不符直接抛错」不一致：`manifest` 的演进（新增 `viewDef` 用法、`viewSchema` 写成越界/绝对路径）不会在 `assertManifest` 处暴露，`contract.test.ts:150` 对 `viewSchemaPath` 的「schema 必须落在仓库根 schemas/ 之下」的意图也因此不完整（该断言只对 `schemaPath` 生效）。 | 在 `assertManifest` 内补：`viewDef` 若存在则必须以 `#/$defs/` 起头且存在于 `event-views` 的 `$defs`；`viewSchema` 若存在则必须与 `viewDef` 成对；并在 `resolveFixtureCase` 后断言解析出的 `viewSchemaPath` 落在 `syncSchemaDir` 之下。 | 不适用（首轮） |
| `review-wp5a-r1-F5` | SUGGESTION | `clients/app/src/layers.test.ts:134` | 平台 API 禁令用**裸包名白名单**：`["ws","socket.io-client","idb","localforage","expo-secure-store"]`。它不覆盖 `node:*`（`node:fs`/`node:crypto`/`node:ws`）、`expo-sqlite`、`*/secure-store`、`*/local-cache` 等本工程 `src/platform/` 计划使用的通道；`importedSpecifiers` 返回的是原始说明符，子路径/别名写法（`expo-secure-store/build/x`）亦不命中。 | 白名单式禁令无法覆盖「页面直接读 IndexedDB/密钥存储」这一 R10 禁令的实际面；新增平台通道时需人工记得扩白名单，否则禁令静默失效。 | 改为「允许清单 + 相对说明符只允许指向 `src/`」的黑名单/正向约束，或对 `app`/`components` 层统一断言「说明符只以 `.` 开头」（与 F3 的扫描根修复一并）。 | 不适用（首轮） |
| `review-wp5a-r1-F6` | SUGGESTION | 新增文件 `clients/app/src/layers.test.ts`、`clients/app/src/domain/domain.test.ts`、`clients/app/src/protocol/contract.test.ts` | `plan.md` 的 Shared File Ownership 明确「TP3 独占 `*.test.ts`」；WP5a 的 Write Scope 只列 `clients/app/package.json`、`clients/app/app/`、`clients/app/src/{domain,protocol,components}/`。三份测试文件介于两者之间：目录级范围覆盖，但 `*.test.ts` 归属行未把 WP5a 列入。 | R10 的强制机制（作者报告、本报告 A.2 复现其判别力）**正是** `layers.test.ts`；若被判越界移除，R10 在代码层将失去唯一机器断言。不阻断交付，但归属应在计划层显式化，避免 WP5b/WP6/WP7/TP3 同名文件冲突时无据可依。 | 由 main 在 Shared File Ownership 或 WP5a 行补登记「WP5a 建 `src/**/*.test.ts` 骨架，TP3 接管」，或按现约定在 MU3a 候选前确认。 | 不适用（首轮） |

---

## Assessment

### 本轮检视结论

**PASS**（对应 Target Revision `0a99eebd696cd18e9f44a86bce08a406e56284b7`）。**0 CRITICAL / 0 MAJOR**，4 MINOR / 2 SUGGESTION。

- **A（R10 分层）符合**：`layers.ts` 与 `docs/FRONTEND_DESIGN.md` §3 的七层数量/职责/顺序一致；方向判定非恒真（`layers.test.ts:129-130` 钉死反向为假）、判别力对照成立（反向 import 必落 `violations`，本 reviewer 以只读 Node 复现解析路径）；反斜杠缺陷修复在 POSIX 语义下彻底（`layers.ts` 的 `layerOfSourcePath` 与 `layers.test.ts` 的归一化均正确）。三项非阻断缺口：F1（barrel 运行期牵连 `node:*`）、F2（裸 barrel / 副作用导入可绕检，且仓库内已存在该写法）、F3（`app` 层的方向与平台禁令实为空断言）——三者当前均无违规代码触发。
- **B（协议层严格性与单一来源）符合**：抽查的 `file.changed` 三可选字段、`agent.connected`/`disconnected` 的封闭唯一 `state`、`sessionRead` 的 `before`/`limit`/`hasEarlier` 均与 schema 逐字段同形；`EventType`/`ErrorCode`/`CommandName`/`SessionState` 全量集合双向差为空；34 个开放视图只对 `additionalProperties:true` 的对象保留索引签名，closed 的信封精确建模；`strict`/`exactOptionalPropertyTypes`/`noUncheckedIndexedAccess` 确开启且由 PV3 实际执行；无副本、无 `any`。唯一 MINOR 是 F4（`viewDef` 声明未校验、`viewSchema` 未做落在 `syncSchemaDir` 之下的断言）。
- **C（视图模型载荷隔离）符合**：`DirectoryView` 无路径字段、空态两态可区分、Agent 连接状态为可选覆盖层且缺席即 `unknown`、`ContextUsageView` 三态无「零即已用」、`SessionSummaryView` 换成展示向字段且 `local` 分支不含 `online`；均以独立用例断言。
- **D（工程范围与 v1 决策）符合**：`platforms: ["web"]`、全仓无 `.native.ts`，且该裁决**确在** `tasks.md` 2.6 与 `FRONTEND_DESIGN` §2.1，非自行收缩；Expo Metro 作打包器 + Vite 仅作 TP4 静态服务器是对 plan 措辞的有据解读，不与 F33 冲突；`src/platform/` 未写、`sync-client`/`state`/`features` 只交类型面、`app/` 与 `src/components/` 只交骨架；对 `schemas/**`/`fixtures/**`/`crates/**`/`docs/**`/根 `package.json`/`scripts/**`/`.github/**` 零改动。
- **E（AC3 证据）成立且可核对**：路径上溯四级、无副本、逐 case 存在性、schema 落在仓库根、清单一致、逐项从 schema 现场读出比对，7 项均有机器断言；TP3 的字节对拍部分不属本包。

### 待补 / 未由本 reviewer 执行的检查

| Check ID | 状态 | 说明 | 是否影响本轮判断 | 应在何门禁前补齐 |
| --- | --- | --- | --- | --- |
| `PV1`（`npm run check` 十道） | 已由主 Agent 执行，本 reviewer 只读日志（exit 0） | 未复跑 | 否 | — |
| `PV1` 的 `check:rust` | 首轮 exit 101（既有 flake），**未**记为通过 | 归因经独立核对成立（本 diff 无 `crates/**` 改动） | 否 | MU3a 候选前的集成门禁 |
| `PV3`（typecheck/test/export:web） | 已由主 Agent 执行，本 reviewer 只读日志（exit 0 / 41 passed / 684 modules） | 未复跑 | 否 | — |
| 真实浏览器渲染与路由（TP4） | 未执行（范围外） | `expo export` 只证明构建可通过 | 否 | TP4 / 最终验收 |
| 设备身份（WebCrypto/IndexedDB）、连接状态机迁移、幂等、事件去重、no-content-cache | 未验证（WP5b/WP6/WP7/TP3 范围） | 本包不含这些实现 | 否 | 相应工作包门禁 |
| `deps`/`advisories`/`secrets` 三个 CI-only job | 本地无等价物，未运行 | 不得声称通过 | 否 | CI |

### 实际检查范围

- **版本与范围**：`git log`/`git rev-parse`/`git diff --name-only|--stat 8ca1e9e 0a99eeb`（38 files/+15341，全部在 `clients/app/**`）；`git ls-files` 查 `.native.*` 与 `src/platform`。
- **逐字读取**（`clients/app/`，全部新文件）：`src/layers.ts`、`src/layers.test.ts`、`src/protocol/{index,paths,manifest,common,auth,sync,command,event,error,pairing}.ts`、`src/protocol/contract.test.ts`、`src/domain/{index,directory,session-summary,agent-catalog,conversation}.ts`、`src/domain/domain.test.ts`、`src/{state,sync-client,features}/index.ts`、`src/components/{index,EmptyState,StatusPill,DegradedEventCard}.tsx`、`app/{_layout,index,pair,dir/[alias],session/[id]}.tsx`、`app.json`、`package.json`、`tsconfig.json`、`vitest.config.ts`、`expo-env.d.ts`、`.gitignore`。
- **机器对照**：以 Python 直接解析 `schemas/sync/v1/*.schema.json` 的 `$defs`，与 TS 导出做集合差与逐字段比对（`EventType`/`ErrorCode`/`CommandName`/`SessionState`/`snapshotResource`/`snapshotChunkCount`/`file.changed`/`agent.*`/`sessionRead*`/`originBlock`/`rawAcp`/`agentContentBlock`/`sessionSummary`/`configOptionView`/`pairing.*`/`auth.*`/`sync.*`/`commandResult*`）；解析 `fixtures/sync/v1/manifest.json`（126 cases、字段集合、schema/viewSchema 取值集合）。
- **只读复现（Node，不写文件）**：`layerOfSourcePath` 对 `src/domain`/`src/domain/index`/`src/platform`/`app/_layout.tsx` 的返回值；`localRelativeTarget` 对 `../domain/directory`/`../domain`/`./event` 的解析；`importedSpecifiers` 对副作用导入/动态导入/`require`/多行/`export from` 五种写法的匹配结果；`collectSourceFiles` 的扫描集合与首段集合。
- **上游解析器源码**：`node_modules/expo/node_modules/@expo/cli/build/src/start/server/metro/withMetroMultiPlatform.js:389-503`（Node externals 垫空分支）、`externals.js:93-94`（`isNodeExternal`）、`metro-config/src/defaults/index.js:52-72`（`extraNodeModules: {}`、`polyfillModuleNames: []`）。
- **契约与规划**：`specs/pwa-web-client/spec.md`（R10/R15/R17/R19/R20 与被删/新增字段相关的段落）、`design.md`（D1–D8）、`docs/FRONTEND_DESIGN.md`（§2.1/§2.2/§3/§5/§6/§7）、`tasks.md`（2.5/2.6/4.3）、`plan.md`（Write Scope、Shared File Ownership、Runtime Resources、Dependency Handoffs、PV3 行）、`fixtures/sync/v1/README.md`、`compatibility/errors/v1/errors.json`。
- **证据只读消费**：`reports/deliver-wp5a-r1.md`、`reports/PV1-wp5a-r1.log`、`reports/PV3-wp5a-r1.log`、`reports/review-wp4-r2.md`（flake 既往登记）。

### 未验证内容

- **未执行**任何编译、类型检查、测试、`expo export` 或 E2E（只读边界）；`PV1`/`PV3` 结论基于主 Agent 的日志，本轮未复跑。
- **未复跑** `check:rust`；不得据本报告声称 Rust 面通过。
- **未判**用例集合齐备性与风险盲区（交主 Agent Coverage Index / validator）。
- **未判** `server::sync` 与 `/ui` 的服务端语义实现（未落地）；未判 WP5b/WP6/WP7/TP3/TP4 的实现。
- **未验证** F1 在真实浏览器 bundle 中的实际失败形态（本轮只读解析器源码推断，未执行 `expo export` + 浏览器加载）；F1 的触发链以 `expo export` 现无消费者为前提。
- **未验证** `src/protocol/` 的少数 `$defs` 与 schema 的长文本约束（`minLength`/`maxLength`/`pattern`）在 TS 侧的一致表达——TS 无法表达这些约束，按 `common.ts` 的注释属 Rust/运行时校验范畴，本轮只核对**形状**（字段名、可选性、枚举取值域、必填集合）。
- 本报告只针对 Target Revision `0a99eeb`；`layers.test.ts` 的三处绕检（F2/F3）与 F1/F4 均为**静态可判定**的非阻断项，修复后需新的独立 reviewer 按 ID 复核。

PASS
