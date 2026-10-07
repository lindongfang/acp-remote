<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 5 复核。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-5"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 独立检视任务、Coverage Index 与 5 份 delta spec 的逐条对应、core-derived-events / pwa-web-client 收窄需求与 AC1 可满足性。本轮重点：Round 4 之后唯一的一处变更（crates/sync-protocol/src/sync.rs 归入 WP1 Write Scope 与 Shared File Ownership）是否成立、是否与 WP2 的 views.rs 冲突、是否引入新重叠或环。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。仅新增本报告文件。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote），本轮执行 2 次"
    result: "退出码 1；planningDigest=plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef（两次一致）；contractDigest=sha256:aa6c318363b344cca0e2d548f74dbd53126b64c85f352552c478a3cd9bb37473；requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（与 Round 4 相同）"
    note: "plan.md mtime 为本轮开始前约 16 分钟，verification.md 约 3 分钟，tasks.md 约 44 分钟，proposal.md/design.md 约 3 小时，5 份 spec 约 6 小时。本轮全部实质结论基于该组字节。"
  - id: "Round 4 之后变更的归属核实（本轮重点）"
    scope: "crates/sync-protocol/src/sync.rs 的模块声明、内容与 WP1 契约变更的关系；与 WP2 Write Scope 的一词级比对；11 个包 Write Scope 的两两重叠重算"
    evidence: "crates/sync-protocol/src/lib.rs:19 声明 `pub mod sync;`；crates/sync-protocol/src/sync.rs:1 模块文档明写「`sync.*` 家族 body（`schemas/sync/v1/sync.schema.json`）」；:80-81 指明 `SnapshotResource` 镜像 `sync.schema.json#/$defs/snapshotResource`；主分支 :83-104 为 8 值枚举，wp1 worktree 已收窄为 3 值；crates/sync-protocol/tests/schema_drift.rs:332 的 `snapshot_resources_match_schema_enum` 把 `SnapshotResource::ALL` 与该 enum 逐条同序钉死；plan.md:861 WP2 Write Scope 仅含 `crates/sync-protocol/src/views.rs`；自写 node 按 workflow-check.mjs:112 的 `scopeOverlap` 逐对重算 11 个包，唯一重叠仍是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`"
  - id: "code/contract 依赖逐条属实性（对照仓库事实）"
    scope: "WP6 code:WP5、WP7 code:WP6/code:WP5、TP2 code:WP3、TP3 code:WP5/WP6/WP7、TP4 code:WP7；WP3/WP4/WP5/TP1/TP2 的纯 contract 声明"
    evidence: "crates/core/Cargo.toml:18-28 依赖闭包仅 async-trait/thiserror/p256/sha2；crates/agent-host/Cargo.toml:11-33 无 sync-protocol；scripts/check-crate-boundaries.mjs:92-112 CORE_FORBIDDEN 含 sync-protocol；:173-176 CORE_ALLOWED_CLOSURE 前四项；docs/MODULE_ARCHITECTURE.md:479-485 §5 矩阵 core 行整行空白、agent-host 行仅 core/acp-protocol"
  - id: "Execution Waves 与依赖列一致性（双口径重算 earliest）"
    scope: "plan.md:888-903 的 11 行 vs Work Packages 依赖列"
    evidence: "按 workflow-check.mjs:203-221 复现：「全部 contract 视为硬依赖」口径下 11 行声明波次与 earliest 逐行相等（W1/W2/W3/W4/W5）；「contract 视为软依赖」口径下各行均不早于最早批次。依赖图 DFS 结果 cycles=[]。池容量：openspec/agentic.yaml:11-13 coding=3 / testing=2，W1=2 coder、W2=3 coder+1 tester、W5=2 tester，均未超容"
  - id: "Contract Freeze 引用 ⊆ Dependencies 与机械可读性"
    scope: "11 个包的 Contract Freeze 单元格"
    evidence: "自写 node 复现 workflow-check.mjs:93-101 的 `frozenContractReadable`：11 个包全部 readable=false（单元格含反引号或为多路径，`path.resolve` 落空）；全部 `@WPn` 引用逐条落在该包 Dependencies 列内（脚本对 11 行输出无一条 FREEZE REF NOT IN DEPS）；无「声明 contract 依赖但 Freeze 为空」的行"
  - id: "Shared File Ownership 覆盖与登记充分性"
    scope: "plan.md:910-925 共 16 行 vs 按字面展开的写入重叠"
    evidence: "工具口径（workflow-check.mjs:388-395）下唯一被命中的重叠对是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`，由 plan.md:912 覆盖，四列齐备且 ownershipProblem() 返回空串。新增行 plan.md:919 为单写者行（Merge Order 为「—」），工具只对被匹配的重叠行求值 ownershipProblem，故该行永不被求值，不影响门禁"
  - id: "Coverage Index 与 5 份 delta spec 的逐条对应"
    scope: "`agentic-coverage` 块的 102 行 vs 五份 spec 的 Requirement/Scenario 标题"
    evidence: "解析后逐 path 分组：sync-snapshot-scope 17/17、core-derived-events 24/24、pwa-web-client 47/47、workspace-resolution 5/5、local-agent-host 9/9，双向无缺无余（唯一未落行的是 `## Purpose` / `## ADDED Requirements` 段落标题，非 Requirement/Scenario）；102 行的 tasks 全部存在于 tasks.md；checks 仅取 {PV1,PV2,PV3,AC1,AC2,AC3}"
  - id: "Merge Strategy 交付单元解析复现"
    scope: "plan.md:962-964 的 Delivery Unit / Mode 与 WP / TP 单元格"
    evidence: "直接调用 @dongfanglin/openspec-agentic 的 deliveryUnits()：返回 MU1/MU2/MU3，mode 均 independent，成员分别 WP1,WP2,TP1 / WP3,WP4,TP2 / WP5,WP6,WP7,TP3,TP4；并集恰为 11 个已声明 ID，无重复归属、无遗漏"
  - id: "AC1 可满足性与 node_link 会话级投递过滤"
    scope: "plan.md:984/1017、tasks.md:56、verification.md AC1 行的四项断言"
    evidence: "crates/server/src/node_link/resource.rs:151-155（fan_out 的 `let Some(session) = … else { return }`）与 :1026-1032（publish 对 `event.session.is_none()` 直接 return，注释写明非会话级事件没有会话级 origin cursor）"
  - id: "需求收窄口径一致性"
    scope: "core-derived-events「产生与持久化」段、pwa-web-client「状态未知」段、design.md D6 与 Risks 首条、plan.md AC1、tasks.md 2.6/2.7/4.1"
    evidence: "specs/core-derived-events/spec.md 的「本能力覆盖事件的产生与持久化…MUST 把缺失的覆盖层呈现为状态未知」；specs/pwa-web-client/spec.md:192「覆盖层缺席——包括服务端尚不具备该类事件的投递通道时——MUST 呈现为状态未知」；design.md D6「投递通道的现状（Round 1 独立审查 F5 的发现）」与 Risks 首条；四处同口径，无相互矛盾"
  - id: "WP1 实际交付文件与声明写入范围的比对"
    scope: "git -C .worktrees/wp1 status --porcelain 与 WP1 声明的 Write Scope"
    evidence: "改动清单：crates/sync-protocol/src/command.rs、src/sync.rs、tests/envelope_fixtures.rs、docs/NODE_LINK_PROTOCOL.md、docs/SYNC_PROTOCOL.md、fixtures/sync/v1/**、schemas/sync/v1/{command,sync}.schema.json。除 tests/envelope_fixtures.rs 外全部落在 plan.md:860 的 Write Scope 内"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-5"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 5
    stage: plan
    target_revision: "plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r5.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份增量 spec 做只读复核；对照 crates/core/Cargo.toml、crates/agent-host/Cargo.toml、crates/sync-protocol/{src/lib.rs,src/sync.rs,tests/schema_drift.rs}、crates/server/src/node_link/resource.rs 的实际代码、docs/MODULE_ARCHITECTURE.md §5、scripts/check-crate-boundaries.mjs 核实依赖声明与新增写归属的属实性；Merge Strategy 交付单元以 node 直接调用 deliveryUnits() 复现；写入重叠按 workflow-check.mjs 的 scopeOverlap 重算。planningDigest 由 workflow check --stage plan --json 在本轮 2 次重新推导，两次一致（6751091d…9fef）。本轮 4 项发现中 2 项为新登记缺口（F15/F16，MINOR），2 项为 Round 4 结转（F13/F14，MINOR），无 CRITICAL/MAJOR。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–4 的 Review ID） |
| Round | 5 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-5`（不拥有任何 WP/TP；owners 为 `coding-1..coding-7`、`testing-1..testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖、写入归属与交付单元成员声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef` |
| 版本稳定性 | 本轮未发生目标版本变动。`workflow check --stage plan --json` 本轮执行 2 次，`planningDigest` 恒为 `6751091d…9fef`。`plan.md` mtime 为本轮开始前约 16 分钟（`verification.md` 约 3 分钟、`tasks.md` 约 44 分钟、`design.md`/`proposal.md` 约 3 小时、5 份 spec 约 6 小时），本轮期间均未再改写 |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`schemas/agentic/schema.yaml` 的依赖类型与 Execution Waves 判据、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:976-988`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1015-1019`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 Round 1 的 F1–F11 修正 |
| Previous Findings | F1–F11（Round 1 MAJOR×5 + MINOR×2；Round 2 MAJOR×1 + MINOR×3）、F12/F13（Round 3 MINOR×2）、F14（Round 4 MINOR×1），逐项复核见下 |
| 实际检查范围 | (a) Round 4 后唯一变更（`crates/sync-protocol/src/sync.rs` 归入 WP1）的归属真实性、与 WP2 `views.rs` 的一词级不重叠、新增重叠与环的重算；(b) 8 条 `code:` 与 11 条 `contract:` 声明逐条属实性；(c) Contract Freeze 引用 ⊆ Dependencies 列 + 路径存在性 + 机械可读性；(d) Execution Waves 双口径 earliest + 无环 + 池容量 + Serialization Reason 取值域；(e) Shared File Ownership 覆盖与登记充分性；(f) Dependency Handoffs 8 条行与各自 Dependencies 列的逐行比对；(g) Reviewer 独立性与 tasks.md §3 的 22 条验证/检视任务；(h) Coverage Index 102 行与 5 份 spec 标题的双向对应 + tasks/checks/evidence 引用有效性；(i) Merge Strategy 交付单元解析与成员覆盖；(j) AC1 可满足性与需求收窄口径；(k) WP1 已产生的工作树改动与声明 Write Scope 的比对 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 WP3 落地后节点级事件的具体生产入口形态（属实现设计，见 Assessment）；变更目录 `git status` 为 `?? openspec/changes/sync-scope-and-pwa-client/`（未跟踪），**因此无法用版本控制逐行 diff 出 Round 4 之后的改动**，本轮改以「按当前字节重新验证 Round 4 的每一条结论」的方式确认其仍成立（见下节） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r5.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 5） |

### Round 4 之后变更的确认（本轮重点）

| 变更 | 位置 | 结论 | 依据 |
| --- | --- | --- | --- |
| `crates/sync-protocol/src/sync.rs` 加入 WP1 的 Write Scope | `plan.md:860` | **正确** | 该文件是 `schemas/sync/v1/sync.schema.json` 的类型化镜像：`crates/sync-protocol/src/sync.rs:1` 的模块文档明写「`sync.*` 家族 body（`schemas/sync/v1/sync.schema.json`）」，`:80-81` 明写 `SnapshotResource` 镜像 `sync.schema.json#/$defs/snapshotResource`。WP1 的契约变更第一行（`plan.md:13`，`sync.schema.json#/$defs/snapshotResource` 下发范围收窄为 `sessions`/`workspaces`/`agents`）若无该文件同步，`crates/sync-protocol/tests/schema_drift.rs:332` 的 `snapshot_resources_match_schema_enum` 会以「`$defs.snapshotResource.enum` 与 `SnapshotResource::ALL` 不一致」失败。主分支该枚举为 8 值（`sync.rs:83-104`），`.worktrees/wp1` 中已被收窄为 3 值——实现确实在此文件落笔，补登记与事实一致 |
| 同文件加入 Shared File Ownership（单写者行） | `plan.md:919` | **正确，且与既有单写者行同构** | 该行为 Writers=WP1、Merge Owner=`coding-1`、Merge Order=`—`、Re-verify=`WP1: PV1`，与既有的 `plan.md:913`/`915`/`917`/`918` 四条单写者行写法一致。工具的 `ownershipProblem()` 只对「被实际重叠匹配到的行」求值（`workflow-check.mjs:388-395`），单写者行永不被求值，故 Merge Order 取「—」不影响门禁 |
| 不与 WP2 冲突 | `plan.md:861` vs `plan.md:919` | **确认无碰撞** | WP2 的 Write Scope 为 `schemas/sync/v1/event-views.schema.json`, `crates/sync-protocol/src/views.rs`, `fixtures/sync/v1/valid/view-*.json`, `fixtures/sync/v1/manifest.json`, `docs/SYNC_PROTOCOL.md`, `docs/ACP_COMPATIBILITY_MATRIX.md`，无 `sync.rs`。按工具的 `scopeOverlap`（`workflow-check.mjs:112`）逐对重算 11 个包，新增条目**不产生任何新的重叠对**：唯一被命中的重叠仍是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`（由 `plan.md:912` 覆盖，`ownershipProblem()` 返回空串） |
| 无新环 | 全表 | **确认无环** | WP1 的 Dependencies 为 `none`，写范围的扩大不进入依赖列；依赖图 DFS 结果 `cycles=[]` |

### Round 4 结论在当前字节的保持情况

变更目录未被 Git 跟踪，无法逐行 diff 改动，故按当前字节逐条重新验证 Round 4 的结论，**全部维持**：

| Round 4 结论 | 本轮结论 | 复核依据（当前文本 + 仓库事实） |
| --- | --- | --- |
| F1（WP3 的 `code:WP1/WP2` 虚假）已解决 | 维持 | `plan.md:862` 仍为 `contract:WP1, contract:WP2`；`crates/core/Cargo.toml:18-28` 依赖闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`，`scripts/check-crate-boundaries.mjs:92-112` 的 `CORE_FORBIDDEN` 仍含 `sync-protocol`，`:173-176` 的 `CORE_ALLOWED_CLOSURE` 前四项与之相等，`docs/MODULE_ARCHITECTURE.md:481` 的 `core` 行仍整行空白 |
| F2（WP4 的 `code:WP2` 虚假）已解决 | 维持 | `plan.md:863` 仍为 `contract:WP2`；`crates/agent-host/Cargo.toml:11-33` 仍无 `sync-protocol`；`docs/MODULE_ARCHITECTURE.md:483` 的 `agent-host` 行仍仅 `core`/`acp-protocol` |
| F3（`command.rs` 无归属）已解决 | 维持 | `plan.md:860` 与 `plan.md:918` 的登记行仍在，无第二个包声明该文件 |
| F4 / F8（TP3 依赖类型与波次）已解决 | 维持 | `plan.md:870` 仍为 `code:WP5, code:WP6, code:WP7, contract:WP1`，波次仍为 W5（`plan.md:902`）；冻结引用 ⊆ 依赖列；重叠由 `plan.md:922`/`924` 登记 |
| F5（AC1 不可满足）已解决 | 维持 | `plan.md:984`/`1017`、`tasks.md:56`、`verification.md` AC1 行仍为「落库 + 按事件库读回 + 会话标识为空 + 会话级投递路径不误收」并显式声明不验证广播；`crates/server/src/node_link/resource.rs:151-155`/`:1026-1032` 仍丢弃会话标识为空的事件 |
| F6 / F7 / F9 / F10 / F11 已解决 | 维持 | `plan.md:913`（event-views 区域收窄）、`:921`（`crates/core/src/**/tests.rs`）、`:922`（`clients/app/**/*.test.ts`）、`:923`（`crates/storage-sqlite/tests/`）四行仍在；`plan.md:868` 的 TP2 仍含 `contract:WP2` 且 `plan.md:936` 的 handoff Upstream 为 `WP2, WP3` 并含 WP2 的失效路径；`verification.md:38` DDR 表的 Report Path 仍为不带反引号的纯路径 |
| F12（Shared File Ownership 两行缺失）已解决 | 维持 | `plan.md:924`/`925` 两行的 Writers / Merge Owner / Merge Order / Re-verify 四列齐备；Region Note 的收窄与 `tasks.md:2.5`/`4.11`/`4.14` 一致 |
| Merge Strategy 分隔符改为 `/` | 维持 | 以当前 plan.md 实调 `deliveryUnits()` 返回 MU1/MU2/MU3 三单元、mode 均 `independent`，不抛错；成员并集 = 11 个已声明 ID，无重复、无遗漏 |
| Coverage Index 与五份 delta spec 双向对应 | 维持 | 102 行按 path 分组后与各 spec 的 Requirement/Scenario 标题逐条相等（17/24/47/5/9）；tasks 全部存在于 tasks.md；checks 只取 {PV1,PV2,PV3,AC1,AC2,AC3} |
| Reviewer 独立性与独立检视任务 | 维持 | 11 个包的 Owner 与 Reviewer 无一相同；`tasks.md:36-57`（§3）为每包各配一条 Project Verify 与一条独立 review 任务，review 行内无 `[wp:…]` 派发标签 |
| F13（Contract Freeze 机械不可核对） | **维持未解决（非阻断）** | 11 个包的 `frozenContractReadable` 仍全部为 false；冻结引用的路径逐个可核对且 `@WPn` 全部落在依赖列内，无一条写错 |
| F14（WP7 的 handoff 漏 `WP1`） | **维持未解决（非阻断）** | `plan.md:935` 的 WP7 行 Upstream 仍为 `WP5, WP6, WP2`，而 `plan.md:866` 的 Dependencies 仍含 `contract:WP1` |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F15 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:920`（`## Shared File Ownership` 的 `crates/sync-protocol/src/views.rs` 与 `crates/*/tests/` 行）；触发面见 `plan.md:869`（TP1 的 Write Scope）与 `plan.md:860`（WP1 的 Write Scope） | WP1 的实际工作树改动包含 `crates/sync-protocol/tests/envelope_fixtures.rs`（`git -C .worktrees/wp1 status --porcelain`，diff 为 `EXPECTED_VALID_MESSAGE_CASES 67→73`、`EXPECTED_BODY_REJECTED 9→14` 两个计数常量）。该路径落在 TP1 声明的 glob `crates/*/tests/` 内，而 `plan.md:920` 把 `crates/*/tests/` 登记为多写者时 Writers 只列 `WP2, TP1`、Merge Order 只列 `WP2 → TP1`，**未列 WP1**。与 Round 2 的 F7b（`crates/storage-sqlite/tests/` 的 TP1×TP2 重叠未登记）同类 | MU1 内 WP1 与 TP1 会对同一目录下的同一文件（计数常量）各自落笔，而登记的合入顺序与后合入方重跑项都不覆盖 WP1 一侧，冲突解决与重跑责任无归属。**不构成 MAJOR**：两个写入者同属 MU1（`plan.md:962`），由同一 merger 收口；`verification.md` 的 `## Test Design and Authoring` 显示 TP1 尚未开工，尚无并发写；且 `plan.md:860` 的 WP1 Write Scope 本身并未声明该目录，故这不是「声明不实」或「同一处却同批」 | 在 `plan.md:920` 的该行把 Writers 补为 `WP2, WP1, TP1`、Merge Order 补为覆盖三者（`WP2 → WP1 → TP1`）、Re-verify 保持 `TP1: PV1`，Region Note 写明「WP1 只改 `crates/sync-protocol/tests/` 下既有计数常量，TP1 只新增契约向量用例文件」；或收窄 TP1 的 Write Scope 为 `crates/{core,storage-sqlite,agent-host,node-link-protocol,sync-protocol}/tests/` 之外的新增文件路径 | 不适用（新发现） |
| DDR-sync-scope-and-pwa-client-r1-F16 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:11` 与 `openspec/changes/sync-scope-and-pwa-client/tasks.md:17` | `plan.md:860` 已把 `crates/sync-protocol/src/sync.rs` 列入 WP1 的 Write Scope，`plan.md:919` 已登记为单写者行；但任务层文本未同步：`tasks.md:11` 的冻结清单仍写 `crates/sync-protocol/src/{views,command}.rs`，`tasks.md:17` 的说明仍只写「WP1 的写入范围含 `crates/sync-protocol/src/command.rs`」。Round 1 的 F3 修复时这两处是与 `plan.md` 一并更新的，本次扩围未跟进 | 执行者与检视者按 tasks.md 的步骤文本工作，会看不到 `sync.rs` 归属的书面依据；`tasks.md:11` 的冻结清单也漏了该镜像文件，后续 WP/TP 引用冻结版本时无对应条目。**不构成 MAJOR**：权威的写入归属在 `plan.md`，本轮已正确登记；这是任务层文本陈旧 | 在 `tasks.md:11` 的冻结清单补 `sync.rs`，并在 `tasks.md:17` 追加一条与 `command.rs` 同构的说明（`sessionRead`/快照资源镜像必须与 schema 同批更新，否则 `tests/schema_drift.rs::snapshot_resources_match_schema_enum` 门禁失败） | 不适用（新发现） |
| DDR-sync-scope-and-pwa-client-r1-F14 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:935` | Round 4 提出、维持未解决。WP7 的 Dependencies 为 `code:WP6, code:WP5, contract:WP1, contract:WP2`（`plan.md:866`），但 handoff 行的 Upstream 只写 `WP5, WP6, WP2`，漏 `WP1`；8 条 handoff 中其余 7 条的 Upstream 集合均等于该包依赖目标集合 | `contract:WP1` 的上游（`schemas/sync/v1/**@WP1`）在 WP7 行没有 Accepted Revision 与 Invalidation 落点。**不构成 MAJOR**：WP7 的两个代码上游 WP5/WP6 已在 Upstream 中，且 WP7 在 W4 而 WP1/WP2 在 W1，波次已排序，缺的是一条已满足的 contract 边的失效登记 | 在 `plan.md:935` 的 WP7 行 Upstream 补 `WP1`，Invalidation 列补「WP1 的 `schemas/sync/v1/**` 变更 → WP7 重跑 PV3」 | 未解决（Round 4 提出，本轮按当前字节复核仍未修正；严重度与阻断判定不变） |
| DDR-sync-scope-and-pwa-client-r1-F13 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:859-870`（11 个包的 Contract Freeze 单元格） | Round 3 提出、维持未解决。复现 `workflow-check.mjs:93-101`：11 个包的 `frozenContractReadable` 全部为 false（多路径单元格不匹配单路径正则；单路径单元格因反引号使 `path.resolve` 落空） | 冻结可核对性依赖人工而非机械判据。**不构成 MAJOR**：所引路径逐个 `fs.accessSync` 均存在、无一条写错；且当前波次合法性正是在「冻结不可读 ⇒ contract 视为硬依赖」这一口径下逐行自洽（见 Assessment） | 去掉单元格内的反引号并把多路径引用拆到单条，或改用工具接受的分隔写法；**注意**：若规范化后冻结变为可读，必须同步为受影响包补合法 Serialization Reason 码，否则 `workflow check --stage plan` 会转 FAIL（耦合关系见 Assessment） | 未解决（Round 3 提出，Round 4、本轮复核仍未修正；严重度与阻断判定不变） |

## 说明（不计为问题）

- **`workflow check --stage plan` 报出的三条 WP2「缺少执行者接收确认」（Attempt 1 coding / Attempt 1 reviewing / Attempt 2 fixing）属已知、用户已接受的派发台账过程缺口，不是规划缺陷。** 根因是派发时未要求执行者运行 `dispatch --ack`，且 attempt 推进后无事后补录入口；三个执行者均已确认收到并实际交付（`verification.md:105-107`），用户 2026-10-03 裁定选 (a) 接受为已知偏离，完整记录在 `verification.md:110-118` 的 `## Dispatch Reconciliation`。本报告不将其计为 Finding，亦不影响本轮 PASS 结论；按该裁决，final/archive 阶段须在 `## Final Assessment` 中一并复述。
- **`workflow check --stage plan` 的第四条 error「`Dependency Declaration Review 的 DDR-sync-scope-and-pwa-client-r1 Plan Revision 未绑定当前规划契约摘要`是登记滞后，非规划缺陷。** `verification.md:38` 的 Round 4 行仍绑 `plan-v2:sha256:43f9e672…`，而当前摘要为 `6751091d…`；本报告尚未被主 Agent 导入。按 `procedures/scheduling.md`「机械格式检查失败先修记录」，由主 Agent 在导入本报告时把该表补一行 Round 5（Reviewer `reviewer-plan-ddr-5`、Plan Revision `plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef`、Result PASS、Report Path 不带反引号的纯路径）即可消除。
- **本轮对 Round 4 报告一处取证表述的更正（不影响任何结论）**：Round 4 称 `ownershipProblem()` 对 `plan.md` 全部登记行「逐行返回空串」。按 `workflow-check.mjs:300-307` 复现，`plan.md:913`/`915`/`917`/`918`/`919` 五条单写者行因 Merge Order 为「—」（落入 `NONE_LABEL`）实际会返回「缺少合并负责人、合入顺序或后合入方重跑项」。但该函数只在 `workflow-check.mjs:388-395` 中对「被实际重叠匹配到的登记行」求值，单写者行永不被匹配，故门禁结果不受影响，Round 4 的实质结论不变。本轮新增的 `plan.md:919` 属同一形态，同样安全。
- **变更目录未被 Git 跟踪（`git status --porcelain` 为 `?? openspec/changes/sync-scope-and-pwa-client/`），无法逐行 diff 出 Round 4 之后的改动。** 本轮因此以「按当前字节重新验证 Round 4 的每一条结论 + 直接比对 `.worktrees/wp1` 的实际改动集」替代 diff，结论见上两节；该替代路径已覆盖本轮指定的全部核对项。
- **`plan.md:962-964` 的 `Owners: Implementation / Test / Review / Merge` 列在三个单元里各只列一名 reviewer**（MU1 列 `review-1` 而成员含 WP2 的 `review-2`）。该列不被任何机械判据读取，权威的 reviewer 分配在 Work Packages 的 Reviewer 列（`plan.md:860-870`，11 行互不相同且均不等于 Owner）与 `tasks.md:36-57` 的 22 条任务中逐条列明且无自审。属摘要列的简写，不计为 Finding（与 Round 4 同一口径）。
- **WP1 新增归属的文件不需要在 `## Contract Changes` 表的「受影响安排」列新增条目**：该表首行（`plan.md:13`）的 `sync.schema.json#/$defs/snapshotResource` 已列 `WP1`，`sync.rs` 是其同属 WP1 的镜像，不引入新的受影响工作包。

## Assessment

### 本轮结论

PASS。Round 4 的实质结论对当前字节**全部继续成立**，Round 4 之后的唯一变更经核实**正确、无回归**：

1. **`crates/sync-protocol/src/sync.rs` 归入 WP1 —— 核实通过。** 归属真实性成立：该文件是 `schemas/sync/v1/sync.schema.json` 的类型化镜像（`crates/sync-protocol/src/sync.rs:1`、`:80-81`），而 WP1 的第一条契约变更（`plan.md:13`）正是收窄该 schema 的 `snapshotResource`；主分支已存在把两侧钉死的门禁 `crates/sync-protocol/tests/schema_drift.rs:332`（`snapshot_resources_match_schema_enum`，逐条且同序断言 `SnapshotResource::ALL` == `$defs.snapshotResource.enum`），因此「不同步改 Rust 镜像就会门禁失败」不是推测——主分支 `sync.rs:83-104` 仍是 8 值枚举，而 `.worktrees/wp1` 中已被收窄为 3 值，事实与补登记一致。Round 1 的 F3（`command.rs` 无归属）正是同一判据的先例，本次扩围是它的同类补正。
2. **不与 WP2 冲突、无新重叠、无新环 —— 核实通过。** WP2 的 Write Scope（`plan.md:861`）只含 `views.rs` 等六项，与 `sync.rs` 一词级不重叠；按工具的 `scopeOverlap` 逐对重算 11 个包，唯一被命中的重叠仍是 WP1×WP2 的 `docs/SYNC_PROTOCOL.md`（`plan.md:912` 覆盖，`ownershipProblem()` 返回空串）；WP1 的 Dependencies 仍为 `none`，依赖图 DFS `cycles=[]`。`plan.md:919` 的登记行写法与既有四条单写者行同构，工具不对其求值 `ownershipProblem()`，门禁结果不变。
3. **依赖声明属实性 —— 维持。** 8 条 `code:` 声明成立；反向侧同样成立：`crates/core/Cargo.toml:18-28`、`crates/agent-host/Cargo.toml:11-33`、`scripts/check-crate-boundaries.mjs:92-112`/`:173-176`、`docs/MODULE_ARCHITECTURE.md:481`/`:483` 均与 Round 4 的取证一致，故 WP3/WP4/WP5/TP1/TP2 的纯 `contract:` 声明不是虚假降级。
4. **Execution Waves、Contract Freeze 引用、无环、池容量、独立性 —— 全部维持。** 双口径重算 earliest：「全部 contract 视为硬依赖」口径下 11 行逐行相等（W1/W2/W3/W4/W5），软口径下各行不早于最早批次；`cycles=[]`；11 行 Serialization Reason 全为 `NOT_APPLICABLE`，唯一带理由码的 `resource-exclusive`（浏览器实例）在 `plan.md:947` 有隔离说明；W1=2 coder、W2=3 coder+1 tester、W5=2 tester，对 `openspec/agentic.yaml:11-13`（coding=3 / testing=2）均未超容；11 个包 Owner 与 Reviewer 无一相同，`tasks.md:36-57` 为每包各配一条 Project Verify 与一条独立 review 任务。
5. **Coverage Index 与五份 delta spec 双向对应 —— 维持。** 102 行分组后 17/24/47/5/9 与各 spec 标题逐条相等、无多余行；tasks 与 checks 引用全部有效。
6. **AC1 可满足性与需求收窄 —— 维持。** `crates/server/src/node_link/resource.rs:151-155` 与 `:1026-1032` 共同保证「不被会话级投递路径误收」可观察；`specs/core-derived-events/spec.md` 的「产生与持久化」段、`specs/pwa-web-client/spec.md:192`、`design.md` D6 与 Risks 首条、`plan.md:984`/`1017`、`tasks.md:56` 五处口径一致，无相互矛盾。

本轮新发现 2 项 MINOR（F15、F16），均为登记/文本同步缺口，**不构成 MAJOR**：F15 缺的是 `crates/*/tests/` 上一个**尚未开工**的写入者（TP1）的登记行，且两个写入者同属 MU1、由同一 merger 收口，既非「声明不实」也非「同一处却同批」——与 Round 2 把同类问题（F7b）定为 MINOR 的先例一致；F16 缺的是任务层文本同步，权威写入归属已在 `plan.md` 正确登记。F13、F14 自前轮结转，维持 MINOR、非阻断。按角色判定规则（存在已确认且未解决的 CRITICAL/MAJOR 才判 FAIL），本轮为 **PASS**。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify` 与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；可满足性已核对 | 四项断言逐条对照 `crates/server/src/node_link/resource.rs:151-155`/`:1026-1032` 后确认不依赖广播（Round 1 F5 的不可满足性已消除） | 否 |
| Main E2E 判据 | 已核对（沿用 Round 4） | `not-applicable` 的理由与用户降级批准原话在 `plan.md:1003-1013` 与 `verification.md` 同名节；本轮未发现与该判定冲突的新事实 | 否 |
| WP1 写归属扩围（本轮重点） | **通过** | `sync.rs` 的镜像关系、schema_drift 门禁、WP2 不重叠、无新重叠对、无新环，四项均以当前字节与仓库事实核实 | 否 |
| Shared File Ownership 覆盖 | **通过** | 工具口径下唯一被命中的重叠 WP1×WP2 `docs/SYNC_PROTOCOL.md` 已登记且 `ownershipProblem()` 为空串；另有 **1 处未登记的实际写入**（F15，非工具口径） | 是（已计入 F15，非阻断） |
| Contract Freeze 路径可核对性 | **不通过（机械口径）** | 11 个包 readable 全部 false（F13）；路径本身存在，冻结引用 ⊆ Dependencies 列逐项通过 | 是（已计入 F13，非阻断） |
| Dependency Handoffs 完整性 | **部分不通过** | 8 条 handoff 中 WP7 的 Upstream 漏 `WP1`（F14） | 是（已计入 F14，非阻断） |
| 任务层文本同步 | **部分不通过** | `tasks.md:11`/`17` 未含 `sync.rs`（F16） | 是（已计入 F16，非阻断） |
| WP2 派发台账 ack 缺口 | **上下文，非规划缺陷** | `verification.md:110-118` 记录根因、三个执行者的实际接收证据与用户 2026-10-03 的 (a) 裁决 | 否 |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md:38` 的 Round 4 行绑旧摘要；导入本报告后补 Round 5 行即可消除该 error | 否（属主 Agent 登记刷新），但会阻断机械门禁转 PASS |

### 不确定性与后续建议

- **AC1 中节点级事件的生产入口**：与 Round 4 相同的观察——`crates/core` 的公开面以 `SessionId` 或只读为入口，AC1 的执行路径依赖 WP3 在其 Write Scope（`crates/core/src/`）内暴露一个可从集成测试直接调用的节点级事件提交入口。这落在 WP3 的实现设计内、不是计划缺陷，故不计为 Findings；建议 WP3 的完成条件显式写明该入口，以避免 AC1 在执行期才暴露。
- **F13 与波次的耦合（沿用 Round 4）**：当前波次合法性依赖「冻结不可读 ⇒ contract 视为硬依赖」这一事实。若按 F13 建议规范化 Contract Freeze 单元格，必须同步为 W2/W3 受影响包补合法理由码，否则 `workflow check --stage plan` 会转 FAIL。规范化不是本轮要求，仅记录该耦合关系。
- **F15 / F16 / F13 / F14 的处置建议**：四项均为非阻断项，且修改 `plan.md` / `tasks.md` 会改变 `planningDigest`（当前 `6751091d…9fef`）。建议在同一轮内一并修正，修正后重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新摘要；若摘要因此变化，本轮 PASS 结论对新摘要失效，须按同一 Review ID 递增 Round 复核。若决定不修正，则须在导入本报告时把 `## Dependency Declaration Review` 补一行 Round 5 并使其 Plan Revision 等于 `plan-v2:sha256:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef`。本轮不宣称任何实现、E2E 或最终验收通过。

PASS
