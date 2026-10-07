<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 4 复核。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-4"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:43f9e6726b78a9496d32f34b92ea526631057a9ad8913790e40b16797af3ffe8"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Merge Strategy 交付单元的解析与成员归属、Reviewer 独立性与 tasks.md 独立检视任务、AC1 断言可满足性、Contract Freeze 的可核对性、Coverage Index 与 5 份 delta spec 的逐条对应，以及 core-derived-events / pwa-web-client 需求在 spec/design/plan/tasks 间的收窄一致性。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote），连续 3 次"
    result: "退出码 1；planningDigest=plan-v2:sha256:43f9e672…effe8（三次一致）；contractDigest=sha256:5d6545a2…2d09；requirementsDigest=sha256:c74ff6ab…1e68"
    note: "本轮开始与结束各取一次，中间复取一次，摘要恒为 43f9e672…；plan.md mtime=2026-10-03 16:49:52，verification.md=16:18:49，design.md/proposal.md=15:15:15，tasks.md=14:42:15，5 份 spec=12:05–12:07。本轮全部实质核对基于该组字节。"
  - id: "Round 3 结论在当前字节的重验"
    scope: "Round 1 的 F1–F7、Round 2 的 F8–F11、Round 3 的 F12/F13 共 13 项，逐项按当前 plan.md/tasks.md/verification.md 与仓库事实复核"
    evidence: "见下文「Round 3 之后的两处变更确认」与 Assessment；F12 判定为已解决，F13 维持未解决（非阻断）"
  - id: "Merge Strategy 单元/mode 单元格解析复现"
    scope: "workflow-check.mjs:527/785 与 scheduling-evidence.mjs:9/14-35（deliveryUnits）对 `Delivery Unit / Mode` 单元格的取值"
    evidence: "两处均为 `/[,，、\\s\\/]+/` 切分；node 复现：`MU1 · independent` → [\"MU1\",\"·\",\"independent\"]，`MU1 / independent` → [\"MU1\",\"independent\"]；以当前 plan.md 调 `deliveryUnits()` 返回 3 个单元（MU1/MU2/MU3，mode 均为 independent，members 分别为 WP1,WP2,TP1 / WP3,WP4,TP2 / WP5,WP6,WP7,TP3,TP4），不抛错；把分隔符改回 `·` 后同一函数抛 `Merge Strategy 的单元 ID、模式或组成无效`"
  - id: "交付单元成员覆盖与重复归属"
    scope: "MU1/MU2/MU3 的 WP / TP 列 vs Work Packages 的 11 个 ID"
    evidence: "并集恰为 {WP1..WP7, TP1..TP4}，无重复、无遗漏；`deliveryUnits()` 自身的 `members.has(id)` 判重亦通过"
  - id: "F12 新增两行 Shared File Ownership 的登记充分性"
    scope: "plan.md:923-924 两行 vs 按字面展开的 9 组写入重叠"
    evidence: "自写 node 复现（剥离反引号与括注、保留 glob 语义、按 scopeOverlap 前缀判定）：9 组重叠全部有登记行；两新行的 Writers / Merge Owner / Merge Order / Re-verify 四列齐备且 `ownershipProblem()`（workflow-check.mjs:300-307）逐行返回空串；Region Note 声明的收窄区域（WP5 不写 `e2e/`、TP3 独占 `*.test.ts`）与 WP5 在 tasks.md:2.5 的七层骨架清单一致"
  - id: "code/contract 依赖逐条属实性（对照仓库事实）"
    scope: "WP6 code:WP5、WP7 code:WP6/code:WP5、TP2 code:WP3、TP3 code:WP5/WP6/WP7、TP4 code:WP7；WP3/WP4/WP5/TP1/TP2 的纯 contract 声明"
    evidence: "crates/core/Cargo.toml:18-28（仅 async-trait/thiserror/p256/sha2）、crates/agent-host/Cargo.toml:11-25（core/acp-protocol/tokio/async-trait/serde_json/thiserror/tracing/sha2/base64，无 sync-protocol）、scripts/check-crate-boundaries.mjs:92-112（CORE_FORBIDDEN 含 sync-protocol）、:173-176（CORE_ALLOWED_CLOSURE 前四项）、docs/MODULE_ARCHITECTURE.md §5 矩阵 core 行整行空白、agent-host 行仅 core/acp-protocol"
  - id: "波次与依赖列一致性（双口径重算 earliest）"
    scope: "plan.md:888-902 的 11 行 vs Work Packages 依赖列"
    evidence: "node 重算 workflow-check.mjs:203-221 的 earliest：按「全部 contract 视为硬依赖」口径 11 行逐行相等（W1/W2/W3/W4/W5）；按「contract 视为软依赖」口径各行 ≤ 声明波次，不触发「早于最早批次」；池容量按 openspec/agentic.yaml:12-13（coding=3 / testing=2）核对：W2 = 3 coder + 1 tester、W5 = 2 tester，均未超容"
  - id: "依赖图无环与 reviewer 独立性"
    scope: "依赖列构成的有向图拓扑；Work Packages 的 Owner/Reviewer 列 vs tasks.md §3 的 22 条分支验证任务"
    evidence: "自写 node DFS：`cycles: []`；11 个包的 Owner 与 Reviewer 无一相同；§3 每个包各有 1 条 Project Verify + 1 条独立 review 任务，review 行内无 `[wp:…]` 派发标签"
  - id: "Contract Freeze 引用与路径存在性"
    scope: "11 个包的 Contract Freeze 单元格"
    evidence: "workflow-check.mjs:93-101 的 `frozenContractReadable` 复现：11 个包全部 readable=false（多路径单元格不匹配单路径正则；单路径单元格因反引号使 `path.resolve` 落空）；`schemas/sync/v1/event-views.schema.json`、`schemas/sync/v1/command.schema.json`、`crates/sync-protocol/src/views.rs`、`crates/core/src/ports.rs`、`fixtures/sync/v1/manifest.json`、`fixtures/sync/v1/transcripts/device-proof.json`、`docs/SYNC_PROTOCOL.md` 逐个 `fs.accessSync` 均存在；冻结引用的 `@WPn` 全部落在该包 Dependencies 列内"
  - id: "Coverage Index 与 5 份 delta spec 的逐条对应"
    scope: "`agentic-coverage` 块的 102 行 vs 五份 spec 的全部 Requirement/Scenario 标题"
    evidence: "YAML 解析后逐 path 分组：sync-snapshot-scope 17/17、core-derived-events 24/24、pwa-web-client 47/47、workspace-resolution 5/5、local-agent-host 9/9，全部「有标题必有行、无行必有标题」；102 行的 tasks 全部存在于 tasks.md，checks 仅取 {PV1,PV2,PV3,AC1,AC2,AC3}，evidence 全部为 `reports/*.log`"
  - id: "AC1 可满足性与 node_link 会话级投递过滤"
    scope: "plan.md:981/1014、tasks.md:56、verification.md AC1 行的四项断言"
    evidence: "crates/server/src/node_link/resource.rs:151-155（`fan_out` 的 `let Some(session) = … else { return }`）与 :1026-1032（`publish` 对 `event.session.is_none()` 直接 return，注释写明非会话级事件没有会话级 origin cursor）；crates/storage-sqlite/src/migrate.rs:84-113（`session_id` 可空 + 三条成对 CHECK）；crates/core/src/broker.rs:7461-7497（既有用例 `non_session_event_has_no_session_scoped_identifiers` 证明非会话级事件可提交并进入 `replay()` 流）；crates/app/tests/node_link_e2e.rs 存在（46330 字节）"
  - id: "需求收窄口径一致性"
    scope: "core-derived-events 的「产生与持久化」段、pwa-web-client 的「状态未知」段、design.md D6 与 Risks 首条、plan.md AC1 与 Main E2E 替代检查表、tasks.md 2.6/2.7/4.1"
    evidence: "specs/core-derived-events/spec.md:66-72 明确「这些事件当前没有投递通道…MUST 把缺失的覆盖层呈现为状态未知」；specs/pwa-web-client/spec.md:192「覆盖层缺席——包括服务端尚不具备该类事件的投递通道时——MUST 呈现为状态未知」；design.md:97 与 :131 同口径；五处一致，无相互矛盾"
  - id: "Main E2E not-applicable 判据"
    scope: "plan.md `### Main E2E` 的 yaml 块与 verification.md 同名节"
    evidence: "crates/server/src/lib.rs:28-30 的模块声明只有 `local_admin`/`node_link`/`transport`，无 `sync`；openspec/agentic.yaml:14-15 `e2e.enabled: true` 与 `e2e.command: \"\"`；verification.md 已记录用户降级批准原话与来源；AC1/AC2/AC3 三条替代检查均不依赖尚不存在的 Sync 入口"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-4"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 4
    stage: plan
    target_revision: "plan-v2:sha256:43f9e6726b78a9496d32f34b92ea526631057a9ad8913790e40b16797af3ffe8"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r4.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份增量 spec 做只读复核，并对照 crates/core、crates/agent-host、crates/storage-sqlite、crates/server、crates/core/src/broker.rs 的实际代码、docs/MODULE_ARCHITECTURE.md §5、scripts/check-crate-boundaries.mjs 核实依赖声明属实性；对 Merge Strategy 的交付单元解析以 node 直接调用 @dongfanglin/openspec-agentic 的 deliveryUnits() 复现。planningDigest 由 workflow check --stage plan --json 在本轮连续 3 次重新推导，三次一致（43f9e672…effe8）。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1/2/3 的 Review ID） |
| Round | 4 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-4`（不拥有任何 WP/TP；owners 为 `coding-1..coding-7`、`testing-1..testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖、写入归属与交付单元成员声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:43f9e6726b78a9496d32f34b92ea526631057a9ad8913790e40b16797af3ffe8` |
| 版本稳定性 | 本轮未发生目标版本变动。`plan.md` mtime=2026-10-03 16:49:52（本轮唯一在 Round 3 之后被改写的规划文件），`verification.md`=16:18:49，`design.md`/`proposal.md`=15:15:15，`tasks.md`=14:42:15，5 份 spec=12:05–12:07。`workflow check --stage plan --json` 在本轮共执行 4 次，`planningDigest` 恒为 `43f9e672…effe8`。本报告每一条实质结论均以上述字节为准 |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`schemas/agentic/schema.yaml:186-215`（依赖类型与 Execution Waves 判据）、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` §5/§7 |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:975-987`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1010-1016`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 F1–F11 的修正 |
| Previous Findings | F1–F11（Round 1 MAJOR×5 + MINOR×2；Round 2 MAJOR×1 + MINOR×3）、F12/F13（Round 3 MINOR×2），逐项复核见下 |
| 实际检查范围 | (a) 8 条 `code:` 与 11 条 `contract:` 声明的逐条属实性；(b) Contract Freeze 引用 ⊆ Dependencies 列 + 路径存在性 + 机械可读性；(c) 依赖图无环与 Execution Waves 一致性（双口径重算 earliest）+ 池容量 + Serialization Reason 取值域；(d) Write Scope 9 组重叠与 Shared File Ownership 15 行登记的路径级充分性；(e) Merge Strategy 三个交付单元的 id/mode 解析、成员覆盖与重复归属；(f) Reviewer 独立性、tasks.md 独立检视任务与 `[wp:…]` 派发标签；(g) Coverage Index 102 行与 5 份 spec 标题的双向对应 + tasks/checks/evidence 引用有效性；(h) AC1 四项断言可满足性与 Main E2E `not-applicable` 判据；(i) core-derived-events / pwa-web-client 收窄需求在 spec/design/plan/tasks 的口径一致性；(j) Dependency Handoffs 的 8 条行与各自 Dependencies 列的逐行比对 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未核对 main E2E 之外的真实 Daemon 行为；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 WP3/WP4 落地后节点级事件的具体生产入口形态（属实现设计，见 Assessment 的不确定性说明）；未核对 TP3 与 TP4 在 `clients/app/e2e` 下的文件命名约定是否会使 TP4 产物落入 TP3 的 `**/*.test.ts` glob（计划未规定该约定，属未证实项，不计为问题） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r4.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 4） |

### Round 3 之后的两处变更确认

| 变更 | 位置 | 结论 | 依据 |
| --- | --- | --- | --- |
| `## Merge Strategy` 的 unit/mode 单元格由 `MU1 · independent` 改为 `MU1 / independent`（MU2/MU3 同改） | `plan.md:961-963` | **正确，且必要** | 工具对该单元格用 `/[,，、\s\/]+/` 切分（`workflow-check.mjs:580` 的 `checkTokens`、`scheduling-evidence.mjs:9` 的 `tokens`）。`·` 不在分隔符内，`MU1 · independent` 切出 `["MU1","·","independent"]`，`deliveryUnits()`（`scheduling-evidence.mjs:18`）取 `declaration[1]` 得 `"·"`，不落在 `['independent','integrated']` 白名单，`scheduling-evidence.mjs:27` 抛 `Merge Strategy 的单元 ID、模式或组成无效`。改为 `/` 后切出 `["MU1","independent"]`。node 复现：当前 plan.md → `deliveryUnits()` 正常返回 3 单元；把分隔符替换回 `·` → 同一函数抛错 |
| Shared File Ownership 新增两行，登记 WP5 的目录级 `clients/app/` 写入范围分别对 TP3 的 `*.test.ts` 与 TP4 的 `e2e/` | `plan.md:923-924` | **准确且充分，F12 关闭** | 见下节「F12 逐项复核」 |

### F12 逐项复核（Round 3 的 MINOR，本轮关闭）

| 原问题 ID | 原严重度 | 本轮结论 | 复核依据（当前文本 + 仓库事实） |
| --- | --- | --- | --- |
| DDR-…-r1-F12 | MINOR | **已解决** | `plan.md:923` 新增行：File = `clients/app/`（WP5 的目录级范围）对 `clients/app/**/*.test.ts`（TP3）；Writers = `WP5, TP3`；Merge Owner = `merger`；Merge Order = `WP5 → TP3`（覆盖全部写入者）；Re-verify = `TP3: PV3`（`PV3` 在 Coverage Index 的 `checks` 集合中）。`plan.md:924` 新增行：File = `clients/app/` 对 `clients/app/e2e/`（TP4）；Writers = `WP5, TP4`；Merge Order = `WP5 → TP4`；Re-verify = `TP4: PV3`。Region Note 声明 WP5 只写 `src/{domain,protocol,sync-client,state,features,components,platform}/`、`app/`、`package.json` 且不写 `e2e/`，TP3 独占 `*.test.ts`、TP4 独占 `e2e/`——与 `tasks.md:2.5` 的七层骨架清单、`tasks.md:4.11`/`4.14` 的产物路径一致。按字面展开 11 个包的 Write Scope 得 9 组重叠，逐组核对均有「同时列出两个写入者」的登记行（`plan.md:910-924` 共 15 行），`ownershipProblem()` 对全部 15 行求值均返回空串 |

### Round 1 / Round 2 结论在当前字节的保持情况

Round 3 已逐项确认 F1–F11 解决。本轮按当前字节重新对照仓库事实，结论维持：

| 原问题 ID | 原严重度 | 本轮结论 | 复核依据（当前文本 + 仓库事实） |
| --- | --- | --- | --- |
| F1（WP3 的 `code:WP1/WP2` 虚假） | MAJOR | **维持已解决** | `plan.md:862` 仍为 `contract:WP1, contract:WP2`。`crates/core/Cargo.toml:18-28` 依赖闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`，`scripts/check-crate-boundaries.mjs:173-176` 的 `CORE_ALLOWED_CLOSURE` 前四项与之相等，`:92-112` 的 `CORE_FORBIDDEN` 仍含 `sync-protocol`，`docs/MODULE_ARCHITECTURE.md` §5 的 `core` 行仍整行空白 |
| F2（WP4 的 `code:WP2` 虚假） | MAJOR | **维持已解决** | `plan.md:863` 仍为 `contract:WP2`。`crates/agent-host/Cargo.toml:11-25` 仍无 `sync-protocol`；§5 的 `agent-host` 行仍仅 `core`/`acp-protocol` |
| F3（`command.rs` 无归属） | MAJOR | **维持已解决** | `plan.md:860` 的 WP1 Write Scope 仍含 `crates/sync-protocol/src/command.rs`；`plan.md:918` 的单写者登记行仍在；无第二个 WP 声明该文件 |
| F4 / F8（TP3 依赖类型与波次） | MAJOR | **维持已解决** | `plan.md:869` 仍为 `code:WP5, code:WP6, code:WP7, contract:WP1`，波次仍为 W5（`plan.md:901`）；8 条 `@WPn` 冻结引用全部落在依赖列内；重叠已由 `plan.md:921`（WP7×TP3）与 `plan.md:923`（WP5×TP3）登记 |
| F5（AC1 不可满足） | MAJOR | **维持已解决** | `plan.md:981`/`1014`、`tasks.md:56`、`verification.md` AC1 行仍为「落库 + 按事件库读回 + 会话标识为空 + 会话级投递路径不误收」并显式声明不验证广播；`crates/server/src/node_link/resource.rs:151-155`/`:1026-1032` 仍丢弃会话标识为空的事件 |
| F6 / F7 / F9 / F10 / F11 | MINOR | **维持已解决** | `plan.md:913`（event-views 区域收窄）、`:920`（`crates/core/src/**/tests.rs`）、`:921`（`clients/app/**/*.test.ts`）、`:922`（`crates/storage-sqlite/tests/`）四行仍在；`plan.md:868` 的 TP2 仍含 `contract:WP2` 且 `plan.md:935` 的 handoff Upstream 为 `WP2, WP3` 并含 WP2 的失效路径；`verification.md` DDR 表的 Report Path 仍为不带反引号的纯路径 |
| F13（Contract Freeze 机械不可核对） | MINOR | **维持未解决（非阻断）** | 11 个包的 `frozenContractReadable` 仍全部为 false；路径本身逐个 `fs.accessSync` 均存在，无一条写错 |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F14 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:934`（`## Dependency Handoffs` 表的 WP7 行） | WP7 的 Dependencies 为 `code:WP6, code:WP5, contract:WP1, contract:WP2`（`plan.md:866`），但 WP7 行的 Upstream 只写 `WP5, WP6, WP2`，**漏 `WP1`**。逐行比对 8 条 handoff：WP3/WP4/WP5/WP6/TP2/TP3/TP4 的 Upstream 集合均等于该包的依赖目标集合，仅 WP7 不等（WP1 属 MU1、WP7 属 MU3，是跨交付单元的 contract 边） | WP7 的 `contract:WP1` 上游（`schemas/sync/v1/**@WP1`）在 WP7 行没有 Accepted Revision 与 Invalidation 落点；若 WP1 的冻结契约在 WP7 开工后变更，WP7 的失效路径无据可依。**不构成 MAJOR**：WP7 的两个代码上游 WP5/WP6 已在 Upstream 中，且 WP7 位于 W4 而 WP1/WP2 位于 W1，波次已排序；缺的是一条已满足的 contract 边的失效登记，既非「声明不实」也非「同一处却同批」 | 在 `plan.md:934` 的 WP7 行 Upstream 补 `WP1`，并在 Invalidation 列补一条「WP1 的 `schemas/sync/v1/**` 变更 → WP7 重跑 PV3」 | 不适用（Round 4 新发现，编号在 F13 之后接续） |

## 说明（不计为问题）

- **WP7 的 Contract Freeze 单元格只列 `event-views.schema.json@WP2`，未覆盖其 `contract:WP1` 边。** 按机械口径这不是错误：`workflow-check.mjs:198-201` 只要求「声明了 contract 依赖就必须填 Contract Freeze」，不要求逐条 contract 边都有对应冻结路径。语义上该声明略宽于必要（WP7 实际经 WP5 的 `src/protocol/` 间接消费 WP1 的 schema），但方向保守（多声明一条已满足的前置），不产生错误调度。已并入 F14 一并处理，不单列。
- **`verification.md` `## Check Plan Changes` 开头仍写「Round 1 独立依赖声明审查（）判 FAIL，报 5 项 MAJOR + 2 项 MINOR」（括号为空）**，未记录 Round 2 的 FAIL 与其 1 项 MAJOR（F8）、Round 3 的 2 项 MINOR，也未记录本轮 F12 的关闭。实质修正内容齐备且与 `plan.md` 当前字节逐条一致，属登记表述陈旧而非规划语义缺陷，按前三轮同一口径不计入 Findings。
- **`verification.md` `## Dependency Declaration Review` 的 Round 1/2/3 行 Plan Revision 分别为 `plan-v2:…771a55f2`、`…2fb86ed0`、`…b484b561`，均非当前的 `43f9e672…`。** `workflow check --stage plan` 当前唯一的 error 即由此产生（`Dependency Declaration Review 的 DDR-sync-scope-and-pwa-client-r1 Plan Revision 未绑定当前规划契约摘要`），是「Round 4 尚未登记」的预期状态，按 `procedures/scheduling.md`「机械格式检查失败先修记录」由主 Agent 在导入本报告时刷新，不计为规划缺陷。
- **`plan.md:961-963` 的 `Owners: Implementation / Test / Review / Merge` 列在三个单元里各只列一名 reviewer**（MU1 列 `review-1` 而成员含 WP2 的 `review-2`；MU2 列 `review-3` 而成员含 WP4 的 `review-4`；MU3 列 `review-5` 而成员含 WP6/WP7 的 `review-6`/`review-7`）。该列不被任何机械判据读取（`deliveryUnits()` 只取 `Delivery Unit / Mode` 与 `WP / TP`；`planning-model.mjs:98` 只把它纳入摘要计算），且权威的 reviewer 分配在 Work Packages 的 Reviewer 列与 `tasks.md` §3 的 22 条任务中逐条列明且无自审。属摘要列的简写，不构成登记缺失，不计为 Findings。
- **TP1 没有 Dependency Handoffs 行属正确**：TP1 的上游 WP1/WP2 与它同属 MU1，同一交付单元内部的合入证据由 MU1 的候选/合入记录承载，不需要跨单元交接行。

## Assessment

### 本轮结论

PASS。Round 3 的 PASS 结论对当前字节**继续成立**，Round 3 之后的两处变更经核实**正确、无回归**：

1. **Merge Strategy 分隔符改写**——核实通过。`MU1 / independent` 在 `checkTokens`/`tokens` 的 `/[,，、\s\/]+/` 口径下切出 `["MU1","independent"]`，id 为 `MU1`、mode 为合法值 `independent`；MU2/MU3 同样成立。`deliveryUnits()` 以当前 plan.md 实跑返回三个单元、无抛错；把分隔符还原为 `·` 后同一函数抛 `Merge Strategy 的单元 ID、模式或组成无效`，反向证明该改动是必需的修复而非风格调整。成员归属核对：MU1 = {WP1, WP2, TP1}、MU2 = {WP3, WP4, TP2}、MU3 = {WP5, WP6, WP7, TP3, TP4}，并集恰为 Work Packages 声明的 11 个 ID，**无重复归属、无遗漏覆盖**。该修复同时解除了 `schedulingEvidence()` 在 apply 阶段读取交付单元时的抛错风险。
2. **F12 的两条 Shared File Ownership 新增行**——核实通过，**F12 判定为已解决**。`plan.md:923`/`924` 两行的 Writers / Merge Owner / Merge Order / Re-verify 四列齐备，`ownershipProblem()` 逐行求值返回空串（合入顺序覆盖全部写入者；后合入方重跑项引用的 `PV3` 确实存在于 Coverage Index 的 `checks` 集合中）；Region Note 的收窄与 `tasks.md:2.5`/`4.11`/`4.14` 一致。按字面展开 Write Scope 后的 9 组重叠现已**全部**有登记行。
3. **`code:` 依赖逐条属实**。WP6 `code:WP5`、WP7 `code:WP6`/`code:WP5`、TP2 `code:WP3`、TP3 `code:WP5`/`code:WP6`/`code:WP7`、TP4 `code:WP7` 五组 8 条均成立。反向侧同样成立：`crates/core/Cargo.toml:18-28` 的依赖闭包只有 `async-trait`/`thiserror`/`p256`/`sha2`，`crates/agent-host/Cargo.toml:11-25` 无 `sync-protocol`，`scripts/check-crate-boundaries.mjs:92-112` 的 `CORE_FORBIDDEN` 明列 `sync-protocol`，`docs/MODULE_ARCHITECTURE.md` §5 的 `core` 行整行空白、`agent-host` 行仅 `core`/`acp-protocol`。故 WP3/WP4/WP5/TP1/TP2 的纯 `contract:` 声明不是虚假降级。
4. **Execution Waves、Contract Freeze 引用、Serialization Reason、无环、独立性全部维持**。按 `workflow-check.mjs:203-221` 重算 earliest，「全部 contract 视为硬依赖」口径下 11 行的声明波次与 earliest **逐行相等**（W1/W2/W3/W4/W5），「contract 视为软依赖」口径下各行均不早于最早批次；11 行 Serialization Reason 全为 `NOT_APPLICABLE`（`NONE_LABEL` 覆盖的合法空值），唯一带理由码的 `resource-exclusive`（浏览器实例）在 `plan.md:944` 的 Runtime Resources 行、属合法码且有隔离说明；依赖图 DFS 结果 `cycles: []`；池容量 W2 = 3 coder + 1 tester、W5 = 2 tester，对 `dispatch.pool.coding: 3` / `pool.testing: 2` 均未超容；11 个包的 Owner 与 Reviewer 无一相同，`tasks.md` §3 为每包各配一条 Project Verify 与一条独立 review 任务且 review 行无 `[wp:…]` 派发标签。
5. **Coverage Index 与五份 delta spec 双向对应**（本轮新增核对）：`agentic-coverage` 的 102 行按 path 分组后与各 spec 的 Requirement/Scenario 标题**逐条相等且无多余行**（17/24/47/5/9），102 行的 `tasks` 全部存在于 `tasks.md`，`checks` 只取 {PV1, PV2, PV3, AC1, AC2, AC3}，`evidence` 全部为 `reports/*.log` 形态。
6. **AC1 可满足性与需求收窄**：`crates/server/src/node_link/resource.rs:151-155`（`fan_out` 先 `let Some(session) = … else { return }`）与 `:1026-1032`（`publish` 对 `event.session.is_none()` 直接 return，注释写明「非会话级事件没有会话级 origin cursor」）共同保证「会话级投递路径不误收」可观察；`crates/storage-sqlite/src/migrate.rs:84-113` 的 `session_id` 可空且三条成对 CHECK 自洽；`crates/core/src/broker.rs:7461-7497` 的既有用例 `non_session_event_has_no_session_scoped_identifiers` 证明非会话级事件可提交并进入 `replay()` 流，故「可按事件库读回」可断言。需求侧五处口径一致：`specs/core-derived-events/spec.md:66-72`、`specs/pwa-web-client/spec.md:192`、`design.md:97` 与 `:131`、`plan.md:981`/`1014`、`tasks.md:2.6`/`2.7`/`4.1`，无相互矛盾。Main E2E `not-applicable` 判据复核成立：`crates/server/src/lib.rs:28-30` 无 `sync` 模块，`openspec/agentic.yaml:14-15` 的 `e2e.command` 为空，AC2/AC3 的入口（`npm run verify`、`clients/app` 契约测试）不依赖尚不存在的 Sync 入口。

本轮新发现 1 项 MINOR（F14），不构成 MAJOR：WP7 的 Dependency Handoffs 行漏列 `contract:WP1` 的上游，缺的是**一条已满足的 contract 边的失效登记**——WP7 的两个代码上游 WP5/WP6 已在 Upstream 中、WP7 位于 W4 而 WP1/WP2 位于 W1，波次已排序，既非「声明不实」也非「同一处却同批」，按角色约定的 MAJOR 三要件均不成立。

**Round 3 的 F13（Contract Freeze 机械不可核对）维持未解决**，严重度仍为 MINOR、非阻断：11 个包的冻结路径在仓库中确实全部存在且逐条可人工核对，无一条写错或指向不存在的文件；机械口径下 readable 全为 false 属格式脆弱性，且当前波次合法性正是在该口径下逐行自洽（见上）。该问题与 F14 一样不改变本轮 PASS 结论。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用，不依赖执行证据 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify`（`package.json:13` = `check` 十道脚本 + `check:rust`；`:14` 的 `check:schemas`、`:22` 的 `check:drift` 双向门禁）与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；**可满足性已核对** | 四项断言逐条对照 `crates/storage-sqlite/src/migrate.rs:84-113`、`crates/core/src/broker.rs:7461-7497`、`crates/server/src/node_link/resource.rs:151-155/1026-1032` 后确认可满足且不依赖广播；`crates/app/tests/node_link_e2e.rs` 存在（46330 字节） | 否（Round 1 F5 的不可满足性已消除） |
| Main E2E 判据 | 已核对 | `crates/server/src/lib.rs:28-30` 无 `sync` 模块；`openspec/agentic.yaml:14-15` 的 `e2e.command` 为空；降级批准原话与来源已记录 | 否 |
| Merge Strategy 交付单元解析 | **通过（本轮重点）** | `deliveryUnits()` 实跑返回 MU1/MU2/MU3，mode 均 `independent`；成员并集 = 全部 11 个已声明工作包，无重复、无遗漏 | 否 |
| 写入重叠登记 | **通过（F12 已关闭）** | 按字面展开的 9 组重叠全部有登记行；新增两行的 `ownershipProblem()` 求值为空 | 否 |
| Contract Freeze 路径可核对性 | **不通过（机械口径）** | 11 个包 readable 全部 false（F13）；路径本身真实存在，冻结引用 ⊆ Dependencies 列逐项通过 | 是（已计入 F13，非阻断） |
| Dependency Handoffs 完整性 | **部分不通过** | 8 条 handoff 中 WP7 的 Upstream 漏 `WP1`（F14） | 是（已计入 F14，非阻断） |
| Coverage Index 覆盖 | **通过（本轮新增）** | 102 行与五份 spec 标题双向一一对应；tasks/checks/evidence 引用全部有效 | 否 |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md` DDR 表 Round 1/2/3 行的 Plan Revision 均非当前 `43f9e672…`，是本报告尚未登记的预期状态 | 否（属主 Agent 登记刷新），但会阻断机械门禁转 PASS |

### 不确定性与后续建议

- **AC1 中节点级事件的生产入口**：`crates/core/src/broker.rs:3143` 的 `commit_owned` 非 `pub`，公开面（`:606` 的 `sink(&SessionId)`、`:1733-1841` 的 `read_view/head/replay/read_session/flush/pump`）全部以 `SessionId` 或只读为入口；同时 `crates/app/tests/support/owner.rs:169-173` 用 `ScriptedBackends`/`ScriptedCatalog` 整体替换了 agent-host，因此 AC1 的执行路径**不经由 profile 进程生命周期**产生 `agent.connected`。AC1 的可执行性依赖 WP3 在 `crates/core/src/`（其 Write Scope 内）暴露一个可直接调用的节点级提交入口。这落在 WP3 的实现设计内、不是计划缺陷，故不计为 Findings；但 AC1 派发前需确认该入口存在，否则该项会在执行期才暴露。建议 WP3 的完成条件显式写明「提供可从集成测试直接调用的节点级事件提交入口」。
- **F13 与波次的耦合**：当前波次合法性依赖「冻结不可读 ⇒ contract 视为硬依赖」这一事实（见 Assessment 中 earliest 的双口径重算）。若后续按建议规范化 Contract Freeze 单元格以使其机械可读，必须同步为 W2/W3 受影响包补合法理由码，否则门禁转 FAIL。规范化不是本轮要求，仅记录该耦合关系。
- **对主 Agent 的处置建议**：F13 与 F14 均为非阻断项，且修改 `plan.md` 会改变 `planningDigest`（当前 `43f9e672…effe8`）。建议二者在同一轮一并修正，修正后重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新 `planningDigest`；若摘要因此变化，本轮 PASS 结论对新摘要失效，须按同一 Review ID 递增 Round 复核。若决定不修正 F13/F14，则须在导入本报告时把 `## Dependency Declaration Review` 的 Round 4 行 Plan Revision 填为 `plan-v2:sha256:43f9e6726b78a9496d32f34b92ea526631057a9ad8913790e40b16797af3ffe8`，使 `workflow check --stage plan` 的唯一 error 消除。本轮不宣称任何实现、E2E 或最终验收通过。

PASS
