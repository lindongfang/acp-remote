<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 2 复核。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-2"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:2fb86ed036e7230a89a3e3e049a016a6f5065ac414e6e4310bb736e73fc94b1a"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Reviewer 独立性与 tasks.md 独立检视任务、AC1 断言可满足性、以及 core-derived-events / pwa-web-client 需求在 spec/design/plan/tasks 间的收窄一致性。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote，本轮开始与结束各执行一次，摘要一致）"
    result: "退出码 1；planningDigest=plan-v2:sha256:2fb86ed0…4b1a；contractDigest=sha256:1d1cfb15…0774；errors 仅 3 条，全部指向 verification 的 DDR 登记行"
    note: "本报告绑定当前 planningDigest。3 条 error 中两条（Plan Revision 未绑定当前摘要、Result 必须 PASS）是本轮审查尚未登记的预期状态；第三条「报告不可读取」是登记格式缺陷，见 F11。"
  - id: "code 依赖逐条属实性（对照仓库事实）"
    scope: "WP6 code:WP5、WP7 code:WP5/WP6、TP2 code:WP3、TP4 code:WP7，以及 TP3 的 contract:WP5/WP6"
    evidence: "crates/core/Cargo.toml:18-28（仅 async-trait/thiserror/p256/sha2）、crates/agent-host/Cargo.toml:11-25（无 sync-protocol）、scripts/check-crate-boundaries.mjs:92-113（CORE_FORBIDDEN 含 sync-protocol）、docs/MODULE_ARCHITECTURE.md:479-491（core 行整行空白、agent-host 行仅 core/acp-protocol）、crates/agent-host/src/mapper.rs:311-327 与 session.rs:178-181（EndpointEvent + EventSink）、crates/core/src/model/json.rs:30-48（ViewJson）"
  - id: "契约冻结路径可核对性"
    scope: "11 个工作包的 Contract Freeze 与 Dependencies 交叉核对"
    evidence: "plan.md:860-870；schemas/sync/v1/{command,event-views}.schema.json 与 crates/sync-protocol/src/{command.rs:383-388,906-912}（deny_unknown_fields）、crates/sync-protocol/src/views.rs:30-40（VIEW_ENUMS）、crates/sync-protocol/tests/schema_drift.rs:19-21"
  - id: "批次与波次一致性"
    scope: "plan.md:896-907 Execution Waves（11 行）vs Work Packages 依赖列"
    evidence: "openspec/agentic.yaml dispatch.pool.coding=3 / testing=2；schema.yaml:194-199（理由码取值域与写入重叠不作串行理由）"
  - id: "写入归属登记完整性"
    scope: "11 个 Write Scope 两两重叠（按字面 glob 展开）与 Shared File Ownership 12 行"
    evidence: "plan.md:860-870 与 plan.md:908-923；node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:353-395（重放同一套 scopeOverlap/ownershipProblem 判据）"
  - id: "Reviewer 独立性与独立检视任务"
    scope: "11 个包的 Owner/Reviewer 列 vs tasks.md:31-51 §3 的 11 条 review 任务"
  - id: "AC1 可满足性与需求收窄一致性"
    scope: "AC1 四项断言 vs crates/server/src/node_link/resource.rs:152-155/1029-1032、crates/storage-sqlite/src/migrate.rs:85-113、crates/core/src/broker.rs:7462-7499、crates/app/tests/support/owner.rs:12-13"
  - id: "门禁登记可解析性复现"
    scope: "verification.md:35 Report Path 单元格的路径解析"
    evidence: "workflow-check.mjs:45-53（looksLikePath/existsAt 不剥离反引号）；node 复现：带反引号 path.resolve 后 existsSync=false，去掉反引号为 true"
issues: "1 × MAJOR（DDR-sync-scope-and-pwa-client-r1-F4 的未解决部分）、3 × MINOR（F7 的残余、F11 登记格式）。存在未解决的 MAJOR → 本轮结论 FAIL。"
result: FAIL
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/plan.md"
  - "openspec/changes/sync-scope-and-pwa-client/tasks.md"
  - "openspec/changes/sync-scope-and-pwa-client/verification.md"
  - "openspec/changes/sync-scope-and-pwa-client/specs/core-derived-events/spec.md"
  - "openspec/changes/sync-scope-and-pwa-client/specs/pwa-web-client/spec.md"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源；临时目录 /tmp/ddrchk 为只读副本，已随会话结束丢弃）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-2"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 2
    stage: plan
    target_revision: "plan-v2:sha256:2fb86ed036e7230a89a3e3e049a016a6f5065ac414e6e4310bb736e73fc94b1a"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r2.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份增量 spec 做只读复核，并对照 crates/core、crates/agent-host、crates/storage-sqlite、crates/server、crates/app、crates/sync-protocol 的实际代码与 docs/MODULE_ARCHITECTURE.md §5、scripts/check-crate-boundaries.mjs 核实依赖声明属实性；planningDigest 由 `workflow check --stage plan --json` 在本轮开始与结束各重新推导一次，两次一致。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1 的 Review ID） |
| Round | 2 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-2`（不拥有任何 WP/TP；owners 为 `coding-1`..`coding-7`、`testing-1`..`testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖与写入归属声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:2fb86ed036e7230a89a3e3e049a016a6f5065ac414e6e4310bb736e73fc94b1a` |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`procedures/scheduling.md`、`procedures/workflow-check.md`、`schemas/agentic/schema.yaml:190-215`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` §5/§7、`openspec/agentic.yaml` |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:975-983`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1013-1017`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 Round 1 的 F1–F7 修正 |
| Previous Findings | F1–F7（Round 1 MAJOR×5 + MINOR×2），逐项复核见下表 |
| 实际检查范围 | (a) 4 条 `code:` 与 7 条 `contract:` 声明的逐条属实性；(b) 依赖图无环、无整批依赖整批；(c) 11 个 WP/TP 的 Reviewer≠Owner 与 tasks.md 独立检视任务；(d) 11 行 Execution Waves 与依赖列一致性 + Serialization Reason 取值域；(e) Write Scope 两两重叠与 Shared File Ownership 登记；(f) AC1 四项断言可满足性与 Main E2E `not-applicable` 判据；(g) core-derived-events / pwa-web-client 需求在 spec/design/plan/tasks 的收窄一致性；(h) DDR 登记行的机械可解析性 |
| 未验证内容 | 未执行任何构建/测试；未核对 `main` E2E 之外的真实 Daemon 行为；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 TP3 在 AC1 场景下具体以何种 API 产生节点级事件（属 WP3 实现设计，见 Assessment 的不确定性说明） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r2.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 2） |

### Round 1 逐项复核结论

| 原问题 ID | 原严重度 | 本轮结论 | 复核依据（当前文本 + 仓库事实） |
| --- | --- | --- | --- |
| DDR-…-r1-F1 | MAJOR | **已解决** | `plan.md:862` WP3 依赖改为 `contract:WP1, contract:WP2`，`plan.md:874-878` 给出可核对的依据。仓库侧复核成立：`crates/core/Cargo.toml:18-28` 依赖闭包只有 `async-trait`/`thiserror`/`p256`/`sha2`；`scripts/check-crate-boundaries.mjs:92-113` 的 `CORE_FORBIDDEN` 含 `sync-protocol`；`docs/MODULE_ARCHITECTURE.md:479` 的 `core` 行整行为空。core 不可能编译引用 `sync-protocol`，`contract:` 属实 |
| DDR-…-r1-F2 | MAJOR | **已解决** | `plan.md:863` WP4 依赖为 `contract:WP2`。`crates/agent-host/Cargo.toml:11-25` 无 `sync-protocol`；`docs/MODULE_ARCHITECTURE.md:481` 的 `agent-host` 行仅 `core`/`acp-protocol`；`crates/agent-host/src/mapper.rs:311-327`、`session.rs:178-181` 经 `EndpointEvent`+`EventSink` 上报，载荷为 `crates/core/src/model/json.rs:30` 的 `ViewJson`。声明属实 |
| DDR-…-r1-F3 | MAJOR | **已解决** | `plan.md:860` WP1 的 Write Scope 已含 `crates/sync-protocol/src/command.rs`；`plan.md:920` 有对应登记行（单写者 WP1，区域说明与 WP2 的 `views.rs` 不重叠）；`tasks.md:15` 同步写明该文件必须与 schema 同批更新。`crates/sync-protocol/src/command.rs:383-388`、`:906-912` 的 `deny_unknown_fields` 镜像归属已有着落，且无第二个 WP 声明该文件 |
| DDR-…-r1-F4 | MAJOR | **未解决（部分）** | 已解决：Contract Freeze 的 `@WP1` 已进依赖列（`plan.md:869`）；`clients/app/**/*.test.ts` 的同波写入重叠已登记（`plan.md:923`，merger / WP7 → TP3 / TP3: PV3）。**未解决**：`contract:WP5/WP6` 仍是接口型声明，而 TP3 交付的是可运行测试（`tasks.md:71-72`）；覆盖行 `plan.md:613/621/629/637/644/652/660/668/675/683/691/699` 仍把 TP3 的 4.10/4.11 与 WP7 的 2.7 绑在一起，而 TP3 依赖列没有 WP7。详见 F8 |
| DDR-…-r1-F5 | MAJOR | **已解决** | AC1（`plan.md:982`、`plan.md:1015`、`tasks.md:56`、`verification.md:57` 的 AC1 行）已改为「落库 + 按事件库读回 + 会话标识为空 + 会话级投递路径不误收」，并在四处显式声明不验证广播及原因。仓库侧断言可满足：`crates/storage-sqlite/src/migrate.rs:85-113` 的 `owned_event.session_id` 可空且三条成对 CHECK 自洽；`crates/core/src/broker.rs:7462-7499` 的既有用例证明非会话级事件进入 replay 流；`crates/server/src/node_link/resource.rs:152-155` 与 `:1029-1032` 确实丢弃会话标识为空的事件，故「不被误收」可观察。需求侧收窄一致：`specs/core-derived-events/spec.md:70` 新增收窄段、`:82-85` 新增场景、`:76/80` 去掉广播；`specs/pwa-web-client/spec.md:192` 同步；`design.md:97`、`:132` 记录裁定 |
| DDR-…-r1-F6 | MINOR | **已解决** | `plan.md:915` 的 `schemas/sync/v1/event-views.schema.json` 行 Region Note 已记录「WP1 的 Write Scope 是目录级、字面包含本文件；按区域收窄：WP1 只改 `command.schema.json`/`sync.schema.json`/`common.schema.json`」；`plan.md:913` 的 `fixtures/sync/v1/valid/` 行也已有前缀划分说明 |
| DDR-…-r1-F7 | MINOR | **部分解决** | `plan.md:922` 已按建议新增 `crates/core/src/**/tests.rs`（WP3、TP2，含区域收窄说明）。但 TP1 的 `crates/*/tests/` glob（`plan.md:867`）字面覆盖 `crates/storage-sqlite/tests/`，而该目录是 TP2 的 Write Scope（`plan.md:868`），两者重叠未登记。详见 F9 |

### 已核对且通过的判据

- **(b) 无环、无整批依赖整批**：依赖图为 WP1/WP2（无入边）→ WP3/WP4/WP5/TP1 → WP6/TP2 → WP7/TP3 → TP4，逐条指向具体 WP；`plan.md:929-936` 的 Dependency Handoffs 逐 WP 列出 Upstream，无 MU 对 MU 的整批声明。
- **(c) Reviewer 独立性与独立检视任务**：`plan.md:860-870` 中 11 个包的 Owner（`coding-1`..`coding-7` / `testing-1`..`testing-4`）与 Reviewer（`review-1`..`review-7`）无一相同；`tasks.md:31-51`（§3）为每个包各有一条独立 review 任务（3.2/3.4/3.6/3.8/3.10/3.12/3.14/3.16/3.18/3.20/3.22），无自审。
- **(d) 波次一致性与 Serialization Reason**：11 行 Wave 与依赖列逐条吻合（W1 无前置；W2 的 WP3/WP4/WP5/TP1 依赖全在 W1；W3 的 WP6←WP5、TP2←WP3；W4 的 WP7←WP5+WP6；W5 的 TP4←WP7）。全部 Serialization Reason 为 `NOT_APPLICABLE`；唯一带理由码的串行列在 Runtime Resources（`plan.md:945`）且用的是合法码 `resource-exclusive`。池容量核对：W2 的 3 个 coder 恰等于 `dispatch.pool.coding: 3`，W2 的 1 个 tester 不超 `testing: 2`。
- **(e) Main E2E 判据与 AC1/AC2/AC3 连贯性**：`openspec/agentic.yaml` 的 `e2e.enabled: true` 与 `e2e.command: ""` 成立；`crates/server/src/lib.rs:14,27-29` 确认 `server::sync` 未落地；降级批准原话与来源已记录。AC2/AC3 的入口（`npm run verify`、`clients/app` 契约测试）均不依赖尚不存在的 Sync 入口，可满足。
- **(g) 需求收窄一致性**：`core-derived-events` R8 新增的收窄段与「节点级事件不被会话级投递路径误收」场景，与 `pwa-web-client` R19 的「覆盖层缺席（含服务端尚不具备投递通道时）呈现状态未知」，在 `design.md` D6/Risks、`plan.md` AC1、`tasks.md:71-72` 四处口径一致，无相互矛盾的断言。

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F8 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:869`（TP3 的 Dependencies 列；关联 `plan.md:613/621/629/637/644/652/660/668/675/683/691/699` 的覆盖行与 `plan.md:906` 的 W4 行） | TP3 声明 `contract:WP1, contract:WP5, contract:WP6`，但它交付的是**可运行的** `clients/app` 测试（`tasks.md:71`「设计覆盖…连接 9 态的全部合法迁移与阻断态停止重连、命令 6 态…`eventId` 去重与快照暂存失败不覆盖已完成状态」、`tasks.md:72`「交付可运行的 `clients/app` 测试」），Write Scope 为 `clients/app/**/*.test.ts`——与被测模块同处一个 TS 工程，必然 import 并执行 WP5 的 `src/protocol/`/`src/platform/` 与 WP6 的 `src/sync-client/`/`src/state/`。这与计划自己为 `plan.md:873-874` 的 `code:WP5` 给出的理由（「缺上游无法编译或运行」）同型，`contract:` 不构成开工门槛。同时 TP3 的 Contract Freeze 只列 `fixtures/sync/v1/transcripts/**@WP1` 与 `schemas/sync/v1/**@WP1` 两条，`contract:WP5`/`contract:WP6` 没有任何指向 WP5/WP6 的冻结产物。更关键的是覆盖行仍把 TP3 与 WP7 绑在一起：`plan.md:613/621/629/637`（R17 上下文三态）、`:644/652/660/668`（R18 三类不支持可区分）、`:675/683/691/699`（R19 状态未知）的 tasks 均含 `2.7`（WP7）与 `4.10`/`4.11`（TP3），而 `plan.md:906` 把 TP3 与 WP7 同放 W4、依赖列无 WP7。`verification.md:57` 声称「Coverage 中 TP3 相关行移除 `2.7`（WP7）」，当前文本中并未发生 | 依赖类型与真实需求相反（代码依赖被写成契约依赖），TP3 可在其被测对象落地前开工；R17/R18/R19 三组场景的测试对象是 WP7 的 `clients/app/src/components/`、`src/features/`、`app/` 下的实现，TP3 无 `code:WP7` 却在同层 W4，apply 阶段会在「测试引用尚不存在的模块」或 PV3 红灯上硬失败；`verification.md` 的 Check Plan Changes 与当前文本不符，使复核记录失真 | TP3 的 Dependencies 改为 `code:WP5, code:WP6, code:WP7`（Contract Freeze 相应补 WP5/WP6/WP7 侧的冻结路径，`schemas/sync/v1/**@WP1` 保留）；或保持现状但把 R17/R18/R19 中 WP7 实现的场景整体移交 TP4（其已有 `code:WP7`），并从 `plan.md:613`…`:699` 的 tasks 中移除 `4.10`/`4.11`。若改判为 `code:WP7`，TP3 应移至 W5 并与 TP4 同层核对池容量。同步修正 `verification.md:57` 的表述 | 未解决（Round 1 F4 的残留部分） |
| DDR-sync-scope-and-pwa-client-r1-F9 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:923`（Shared File Ownership 表尾）与 `plan.md:867-868`（TP1/TP2 的 Write Scope） | TP1 的 Write Scope 是 glob `crates/*/tests/`（`plan.md:867`），字面覆盖 `crates/storage-sqlite/tests/`；该目录是 TP2 的显式 Write Scope（`plan.md:868`）。Shared File Ownership 的 12 行中，`crates/storage-sqlite/tests/` 的 Writers 未同时包含 TP1 与 TP2（`plan.md:921` 的合并行覆盖的是 `views.rs` 与 `crates/*/tests/`，`plan.md:922` 覆盖的是 `crates/core/src/**/tests.rs`）。机械检查器不做 glob 展开（`workflow-check.mjs:44` 的 `scopeOverlap` 只做字面前缀比较），因此不会报错——这正是需要独立 reviewer 判断的部分 | TP1 在 MU1、TP2 在 MU3，W2 与 W3 的先后由 `code:WP3` 依赖保证，不构成同批冲突；但登记表按角色约定应覆盖全部重叠写范围，缺项会让后续 `workflow impact` 与合并登记缺少依据，也使「谁先改 `crates/storage-sqlite/tests/`」在两个测试包同时新增文件时无据可依 | 在 Shared File Ownership 增加 `crates/storage-sqlite/tests/` 行：Writers = TP1, TP2；Merge Owner = merger；Merge Order = TP1 → TP2；Re-verify After Merge = TP2: PV2；Region Note 写明 TP1 只新增契约向量文件、TP2 只新增行为测试文件 | 未解决（Round 1 F7 的残留部分） |
| DDR-sync-scope-and-pwa-client-r1-F10 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:868`（TP2 的 Contract Freeze 列；关联 `plan.md:934` 的 Dependency Handoffs 行） | TP2 的 Contract Freeze 列出 `` `crates/sync-protocol/src/views.rs@WP2` ``，但 TP2 的 Dependencies 只有 `code:WP3`，依赖列没有 WP2；`plan.md:934` 的 Dependency Handoffs 行 Upstream 也只写「WP3」。Round 1 对 TP3 的同类问题给出的判据是「freeze 引用必须在依赖列内」，修正后 `verification.md:57` 把该判据写成了计划自身的规则，本条是同一规则在 TP2 上的未应用实例。（TP2 的 Write Scope 为 `crates/core/src/**/tests.rs` 与 `crates/storage-sqlite/tests/`，两者按 `scripts/check-crate-boundaries.mjs:92-113` 的 `CORE_FORBIDDEN` 与 `docs/MODULE_ARCHITECTURE.md:483` 都无法 link `sync-protocol`，所以该引用只能作为「冻结版本的形状基准」，不能是 `code:` 依赖） | 波次上 WP2（W1）早于 TP2（W3），开工顺序不受影响；但 TP2 对 `file.changed` 新字段与 `state` 封闭枚举的断言实际以 WP2 的 `views.rs` 镜像为基准（`crates/sync-protocol/src/views.rs:30-40` 的 `VIEW_ENUMS` 与 `:506` 的 `FileChanged`），而 Dependency Handoffs 的 Invalidation 列（`plan.md:934`）只登记了「core 端口签名变更」，WP2 重开时 TP2 的失效路径没有依据 | 要么在 TP2 的 Dependencies 补 `contract:WP2` 并在 `plan.md:934` 的 Upstream 与 Invalidation 中补 WP2 的变更路径；要么删掉 Contract Freeze 中的 `views.rs@WP2`，把「以 WP2 的镜像为形状基准」写进 `tasks.md:66` 的 TP2 任务描述，避免出现无依赖支撑的冻结引用 | 本轮新增 |
| DDR-sync-scope-and-pwa-client-r1-F11 | MINOR | `openspec/changes/sync-scope-and-pwa-client/verification.md:35`（`## Dependency Declaration Review` 的 Report Path 单元格） | 该单元格写作 `` `reports/ddr-sync-scope-and-pwa-client-r1.md` ``。`workflow-check.mjs:45` 的 `looksLikePath` 与 `:48-53` 的 `existsAt` 直接对单元格文本做 `path.resolve`，**不剥离反引号**，因此 `path.resolve(changeRoot, "`reports/…`")` 必然落空，`workflow check --stage plan` 报「报告不可读取」。已用 node 直接复现：带反引号 `existsSync=false`，去掉反引号 `true`（该文件确实存在于 `openspec/chages/sync-scope-and-pwa-client/reports/`）。这是三条 error 中唯一一条在补齐 Plan Revision 与 Result 后仍会残留的 | 规划门禁无法转 PASS：`## Dependency Declaration Review` 的 Result=要求 PASS、Report Path=必须可解析，登记行不修正则 `workflow check --stage plan` 持续 FAIL，Round 2 结论无法被机械登记 | 把 Report Path 写成不带反引号的路径（`reports/ddr-…-r2.md`），或写成相对仓库根的可解析形式（`openspec/changes/sync-scope-and-pwa-client/reports/ddr-…-r2.md`）；Round 1 与 Round 2 两行一并修正 | 本轮新增 |

### 说明（不计为问题）

- `verification.md:35` 的 Plan Revision 仍登记 `plan-v2:sha256:771a55f2…2cbc5`、Result 为 `PENDING`，`## Handoff Index` 的 Executor/Result 亦为 `PENDING`。按 `procedures/scheduling.md`「机械格式检查失败先修记录……Plan Revision 直接填 CLI 返回的当前 planningDigest」，这是主 Agent 的登记刷新项，不是规划语义缺陷，故未计入 Findings；但它与 F11 叠加会使门禁持续 FAIL，主 Agent 须与 F11 一并处理。
- 覆盖行 `plan.md:701`–`:741`（R20 imported 资源）与 `:380`–`:529`（R10–R14）已不含 `2.7`，说明 F8 涉及的收窄在部分需求上确已执行，R17/R18/R19 三组是遗漏而非整体未做。

## Assessment

### 本轮结论

FAIL。Round 1 的 5 项 MAJOR 中 4 项（F1、F2、F3、F5）与 2 项 MINOR 中 1 项（F6）已在本轮版本中确认解决；F4 仅解决了三处补救中的两处，其核心——TP3 的依赖类型与 WP7 缺口——原样保留，且 `verification.md` 声称已做的覆盖行改动在当前 `plan.md` 中不存在。因此存在 1 项已确认且未解决的 MAJOR（F8），按 `roles/reviewer.md` 的判定规则阻断交付。

问题集中在**同一类根因**：把「需要对方代码」写成「只需要对方冻结的接口」。F1/F2 是 core 与 agent-host 侧的反向错误（把接口写成代码），本轮已改为 `contract:`；F8 是同一判据在 TP3 上的正向错误仍未修正——TP3 的 `clients/app/**/*.test.ts` 与被测模块同处一个 TS 工程，`contract:` 无法成为开工门槛，而依赖列又缺 WP7。Waves 因此把 TP3 与 WP7 放在同层 W4。F10 是同一判据在 TP2 上的记账缺口（冻结引用无依赖支撑），F9 是 F7 修复后仍存的一处写范围重叠未登记，F11 则是门禁登记格式缺陷。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE（apply 阶段执行） | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| IV1 | 待补（apply 阶段） | 同上，`## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify` 与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；**可满足性已核对** | 四项断言逐条对照 `crates/storage-sqlite/src/migrate.rs:85-113`、`crates/core/src/broker.rs:7462-7499`、`crates/server/src/node_link/resource.rs:152-155/1029-1032` 后确认可满足且不依赖广播 | 否（Round 1 F5 的不可满足性已消除） |
| Main E2E 判据 | 已核对 | `e2e.enabled: true`、`e2e.command: ""`；`server::sync` 未落地；`downgrade_approval` 已记录原话与来源 | 否 |
| Contract Freeze 路径可核对性 | 部分不通过 | 10 个包的冻结引用落在依赖列内；TP2 的 `views.rs@WP2` 无对应依赖（F10） | 是（已计入 F10，非阻断） |
| 写入重叠登记 | 部分不通过 | 12 行登记覆盖了全部同波重叠；`crates/storage-sqlite/tests/` 的 TP1×TP2 重叠缺失（F9） | 是（已计入 F9，非阻断） |
| 规划门禁登记 | 未通过 | `verification.md:35` 的 Plan Revision 与 Result 待刷新、Report Path 因反引号不可解析（F11） | 否（属主 Agent 登记刷新），但会阻断机械门禁转 PASS |

### 不确定性与后续建议

- **AC1 中节点级事件的产生路径**：当前 `Broker` 的公开 API（`crates/core/src/broker.rs:606` 的 `sink`、`flush`、`pump`）全部以 `SessionId` 为入口，`commit_owned`（`:3143`）非 `pub`；因此在 WP3/WP4 落地前，`crates/app/tests/node_link_e2e.rs` 无法自行造出节点级 `agent.connected`。这落在 WP3 的 Write Scope（`crates/core/src/`）内，属实现设计而非计划缺陷，故不计入 Findings；但 AC1 的可执行性依赖 WP3 提供一条公开的节点级提交入口，main 在派发 AC1 前应确认该入口存在，否则该项会在执行期才暴露。
- **修正后的连带影响**：若按 F8 的建议把 TP3 改为 `code:WP5, code:WP6, code:WP7`，TP3 的最早可开工层级将由 W4 抬到 W5，与 TP4 同层；按 `openspec/agentic.yaml` 的 `dispatch.pool.testing: 2` 核对仍在容量内，但 W5 从 1 个包变为 2 个包，需重跑 `workflow check --stage plan` 确认 `ownershipProblem` 的 Re-verify 引用仍命中 Check ID。
- **对主 Agent 的处置建议**：F8 属产品与计划缺陷，交实现 Agent 与主 Agent 协调责任人，在 plan/tasks 内集中一次修正并同步 `## Check Plan Changes`；F9/F10/F11 为非阻断项，建议在同一轮一并修正，避免再次刷新 planningDigest。修正后须重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新 `planningDigest`，再按同一 Review ID、递增 Round 派发独立复核；`verification.md` 的 DDR 行需同时登记 Round 1（FAIL）与 Round 2 的结论并刷新 Plan Revision。本轮不宣称任何实现、E2E 或最终验收通过。

FAIL
