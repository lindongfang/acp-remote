<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 7。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-7"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7 / testing-1..testing-4）"
target_revision: "plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 独立检视任务、Coverage Index 与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order。本轮重点：(1) Round 6 的 F17–F20 四项是否在当前字节上真正关闭；(2) tasks.md §5/§6 重排与新增 MU1b 组是否引入编号或交叉引用回归；(3) Round 6 的全部实质结论（无环、依赖属实性、波次合法、写入归属、handoff 完整性、reviewer 独立性）对当前字节是否继续成立。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。仅新增本报告文件。过程中未创建临时脚本（判据全部以内联 node -e 复现，不落盘）。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\Project\acp-remote），本轮执行 2 次"
    result: "退出码 1；planningDigest=plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad（两次一致）；contractDigest=sha256:e3c24bfd3ca9038202459dcbc86c1b03e357ae99355ff1faa3cae3dda9b23bfc；requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（与 Round 4/5/6 相同）。errors 共 6 条：5 条「缺少执行者接收确认」+ 1 条 DDR Plan Revision 未绑定当前摘要"
    note: "plan.md mtime 2026-10-03T13:22:27Z、tasks.md 13:23:34Z（均早于本轮开始且本轮期间未再改写），verification.md 12:49:16Z，design.md/proposal.md 07:15:15Z。本轮全部实质结论基于该组字节。变更目录未被 Git 跟踪，故仍以「按当前字节逐条复现判据」替代 diff。"
  - id: "Contract Freeze 引用 ⊆ Dependencies（F17 复核，11 包逐单元格）"
    scope: "plan.md:860-870 的 Contract Freeze 列 vs 同表 Dependencies 列"
    evidence: "剥离反引号后逐条比对 21 条 `@WPn` 引用与所属包的依赖目标集合：0 条越界。TP1（plan.md:867）为 `schemas/sync/v1/**@WP1`、`crates/sync-protocol/src/sync.rs@WP1`、`crates/sync-protocol/src/command.rs@WP1`、`crates/sync-protocol/src/views.rs@WP2`，四个 token 均在 TP1 的 Dependencies（code:WP1, code:WP2）内；WP1/WP2 自身为 none+NOT_APPLICABLE，无引用。已无任何交付单元 ID 出现在 @ 位置"
  - id: "Contract Freeze 路径与 Write Scope 归属一致性（F17 第二问）"
    evidence: `sync.rs@WP1`/`command.rs@WP1` 落在 WP1 的 Write Scope（plan.md:860 含 crates/sync-protocol/src/{command,sync}.rs）；`views.rs@WP2` 落在 WP2 的 Write Scope（plan.md:861 含 crates/sync-protocol/src/views.rs）；`schemas/sync/v1/**@WP1` 由 plan.md:913 的区域收窄行排除 event-views.schema.json（该文件归 WP2 独占）。四条引用与 Write Scope 归属逐条吻合"
  - id: "冻结语义迁移是否丢失（F17 第三问 / F18 复核）"
    scope: "plan.md:939（Dependency Handoffs 的 TP1 行）与 plan.md:965（MU1b Start/Readiness）、tasks.md:79（5.4）、tasks.md:92（6.9）"
    evidence: `plan.md:939` 的 Accepted Revision 写「MU1a 的合入提交（其 premerge PASS 与独立候选检视绑定的候选提交）」，Start Revision 写「MU1a 合入后的 refs/heads/main」，Transfer 列写「TP1 从该合入提交分支；crates/sync-protocol/src/{sync,command,views}.rs 的新枚举与分页类型在该提交中可编译」。plan.md:965、tasks.md:5.4、tasks.md:6.9 三处与之一致。冻结语义未丢失"
  - id: "Dependency Handoffs 完整性（F18 复核）"
    scope: "plan.md:932-940 共 9 行"
    evidence: `plan.md:939` 新增 TP1 行，Upstream=WP1, WP2 与 TP1 的依赖目标集合相等；9 条 handoff 中 8 条 Upstream 等于依赖目标集合，唯一例外仍是 WP7 行漏 WP1（F14 结转）。同时 TP1 的 Invalidation 列写明「MU1a 的契约变更 → TP1 的向量与 schema_drift.rs 断言失效，需重跑 PV1」，与两个计数常量的共享写点登记（plan.md:923）方向一致"
  - id: "Execution Waves 与依赖列一致性（双口径重算 earliest + 无环）"
    scope: "plan.md:892-902 的 11 行 vs Work Packages 依赖列"
    evidence: "按 workflow-check.mjs:205-221 的 hardDeps 口径复算：11 行声明波次与 earliest 逐行相等（WP1/WP2=1，WP3/WP4/WP5/TP1=2，WP6/TP2=3，WP7=4，TP3/TP4=5）；上游同批/更早批检查 11/11 通过；依赖图 DFS cycles=[]；11 行 Serialization Reason 全为 NOT_APPLICABLE 且无 waveIndex>minimum 的行。池容量：W1=2 coder、W2=3 coder+1 tester、W3=1 coder+1 tester、W4=1 coder、W5=2 tester，对 openspec/agentic.yaml 的 coding=3 / testing=2 均未超容"
  - id: "tasks.md 编号与交叉引用回归（F20 第二问）"
    scope: "tasks.md 全部 - [ ] 6.x 行 + Coverage Index 的 tasks 引用 + 任务正文内的编号引用"
    evidence: `- [ ] ` 行解析得 96 个任务 ID，1.1–1.5/2.1–2.7/3.1–3.22/4.1–4.15/5.1–5.5/6.1–6.29/7.1/8.1–8.3/9.1 全部唯一、无重复、无跳号；§6 为 6.1–6.29 共 29 项连续编号。Coverage Index 的 agentic-coverage 块引用的 17 个任务 ID（2.1–2.7、4.1–4.5、4.8、4.10、4.11、4.13、4.14）全部解析；checks 引用仅取 {PV1,PV2,PV3,AC1,AC2,AC3}。tasks.md:84（6.1）正文引用的「6.6 合入瞬间再核实」指向存在的 6.6"
  - id: "Shared File Ownership 覆盖与登记充分性（按字面展开写入范围）"
    scope: "plan.md:910-926 共 17 行 vs 11 个包 Write Scope 的字面两两重叠"
    evidence: "字面展开（scopeOverlap 的前缀语义）得 8 对重叠：WP1×WP2、WP3×WP4、WP5×WP6、WP5×WP7、WP1×TP1、WP3×TP2、WP5×TP3、WP5×TP4，逐对均命中已登记行且 Writers/Merge Owner/Merge Order/Re-verify 四列齐备。工具口径（workflow-check.mjs:112 的 scopePaths 不做 glob 展开）下唯一命中的是 WP1×WP2 的 docs/SYNC_PROTOCOL.md（plan.md:912）。ownershipProblem() 对多写者行求值返回空串"
  - id: "code/contract 依赖逐条属实性（对照仓库事实）"
    scope: "8 条 code: 边与 WP3/WP4/WP5/TP1/TP2 的纯 contract 声明"
    evidence: `crates/core/Cargo.toml` 的 [dependencies] 仍只有 async-trait/thiserror/p256/sha2；`crates/agent-host/Cargo.toml` 无 sync-protocol；`scripts/check-crate-boundaries.mjs:92-113` 的 CORE_FORBIDDEN 含 sync-protocol（第 106 行）；`docs/MODULE_ARCHITECTURE.md` §5 依赖矩阵（起于 :476）的 core 行整行空白、agent-host 行仅 core/acp-protocol。故 WP3/WP4/WP5 的纯 contract 降级与 TP1/TP3 的 code: 升级均非虚假声明"
  - id: "交付单元解析与 Order 复现"
    scope: "plan.md:962-967 的 Merge Strategy 全节"
    evidence: "以 node 直接调用 @dongfanglin/openspec-agentic 的 deliveryUnits(plan)：返回 4 个单元 MU1a/MU1b/MU2/MU3，mode 均 independent，members 依次 {WP1,WP2}/{TP1}/{WP3,WP4,TP2}/{WP5,WP6,WP7,TP3,TP4}，order='1'/'2'/'3'/'4'、orderGroup 1/2/3/4，无抛错；并集恰为 11 个已声明 ID，无重复归属、无遗漏。MU2/MU3 的 Start/Readiness 列写「MU1b 已合入」，与 scheduling-status.mjs:150-151 的前序 orderGroup 阻塞方向一致，无反向依赖"
  - id: "reviewer 独立性与独立检视任务"
    scope: "11 个包的 Owner/Reviewer/Role 列 + tasks.md §3/§4 的检视任务"
    evidence: "11 个包 Owner 与 Reviewer 无一相同（TP1=testing-1/review-1、TP2=testing-2/review-3、TP3=testing-3/review-5、TP4=testing-4/review-7）；按 workflow-check.mjs:159-169 的口径复算，11/11 至少一条引用它且不含 [wp:…] 标签、含检视标记的任务；每个包恰好一条 [wp:WPn] 派发任务"
  - id: "Coverage Index 与 5 份 delta spec 的对应"
    scope: "`agentic-coverage` 块的 102 行"
    evidence: "block() 解析出 102 行、ID 无重复、target_ref=refs/heads/main；102 行的 tasks 全部存在于 tasks.md；checks 仅取 {PV1,PV2,PV3,AC1,AC2,AC3}"
  - id: "MU1a 计数门禁归属核实（6.3 所述 check:assets）"
    scope: `package.json` 的 check:assets → scripts/check-contract-assets.mjs；crates/sync-protocol/tests/envelope_fixtures.rs:25-27"
    evidence: `package.json` 的 `check:assets` = `node scripts/check-contract-assets.mjs`；该脚本无任何硬编码用例计数断言（计数只出现在 :632-635 的成功摘要里），对 manifest 只校验 transcriptVectors 文件存在（:596-605）。两个计数常量 EXPECTED_VALID_MESSAGE_CASES / EXPECTED_BODY_REJECTED 实际位于 crates/sync-protocol/tests/envelope_fixtures.rs:25,27，由 every_manifest_case_behaves_as_declared（:117，断言于 :196-207）在 cargo test 下执行，即 check:rust（PV1 的 `npm run verify` = check + check:rust）"
  - id: "MU1b 组合在 wp1 工作树上的常量取值"
    evidence: `git show HEAD:crates/sync-protocol/tests/envelope_fixtures.rs` 为 67/9，`.worktrees/wp1/...` 为 73/14——证实 WP1 已把常量抬到自身向量对应的中间值，与 plan.md:923 的「MU1a 由 WP1 定其中间值、MU1b 由 TP1 在 MU1a 合入提交上定最终值」一致，也证实 tasks.md:86 要求 MU1a 候选实测该门禁是必要而非多虑"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘任何脚本）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-7"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 7
    stage: plan
    target_revision: "plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r7.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\Project\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / design.md / proposal.md 与 5 份 delta spec 做只读复核；逐项核实 Round 6 的 F17（TP1 Contract Freeze 的 @ 引用改为工作包 ID）是否在其 Dependencies 列内且与 Write Scope 归属吻合、F18（TP1 的 Dependency Handoffs 行）是否补齐且冻结语义未丢失、F19（Completion Criteria 与 Premerge History 命名四个单元）是否修正、F20（tasks.md §5/§6 按拆分重排并新增 MU1b 组、§6 连续 29 项）是否同步；并全量复算无环、earliest 双口径、Contract Freeze ⊆ Dependencies、字面写入重叠与 Shared File Ownership、交付单元解析与 Order、reviewer 独立性、Coverage Index 引用有效性。planningDigest 由 workflow check --stage plan --json 本轮 2 次重新推导，两次一致（8eb6a3eb…dd4ad）。判定 PASS 的依据是 F17（唯一阻断项）与 F18 已关闭、F19/F20 的 Completion Criteria 与 tasks.md 编号部分已关闭，余下 F19/F20 的残留与 F13/F14/F21–F24 均为 MINOR、非阻断。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–6 的 Review ID） |
| Round | 7 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-7`（不拥有任何 WP/TP；owners 为 `coding-1..coding-7`、`testing-1..testing-4`，独立性成立） |
| Work Package | 全部（WP1–WP7、TP1–TP4）的依赖、写入归属与交付单元成员声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad` |
| 版本稳定性 | 本轮未发生目标版本变动。`workflow check --stage plan --json` 本轮执行 2 次，`planningDigest` 恒为 `8eb6a3eb…dd4ad`。`plan.md` mtime 2026-10-03T13:22:27Z、`tasks.md` 13:23:34Z（均早于本轮开始），`verification.md` 12:49:16Z，`design.md`/`proposal.md` 07:15:15Z；本轮期间均未再改写 |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`roles/merger.md`、`templates/tasks.md`、`procedures/acceptance.md` 的依赖类型与 Execution Waves 判据、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:980-990`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1004-1022`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 Round 1 的 F1–F11 修正 |
| Previous Findings | F1–F11（Round 1）、F12/F13（Round 3）、F14（Round 4）、F15/F16（Round 5）、F17（MAJOR）/F18/F19/F20（Round 6），逐项复核见下 |
| 实际检查范围 | (a) F17：11 个包 21 条 `@` 引用逐条比对 Dependencies 列 + 冻结路径与 Write Scope 归属吻合性；(b) F18：Dependency Handoffs 9 行与 TP1 跨单元 `code:` 边；(c) F19：Completion Criteria 与 Premerge History 命名 + 全篇旧 `MU1` 表述扫描；(d) F20：tasks.md 96 个任务 ID 的唯一性/连续性、§6 的 29 项、Coverage Index 与正文交叉引用、新增 MU1b 组的完整性；(e) 无环、earliest 双口径、池容量、Serialization Reason 取值域；(f) 字面写入重叠 8 对与 Shared File Ownership 登记；(g) code/contract 属实性对照 `crates/core/Cargo.toml`、`crates/agent-host/Cargo.toml`、`scripts/check-crate-boundaries.mjs`、`docs/MODULE_ARCHITECTURE.md` §5；(h) `deliveryUnits()` 实跑四单元与 Order 组序；(i) reviewer 独立性与每包独立检视任务；(j) Coverage Index 102 行引用有效性；(k) 6.3 所述 `check:assets` 计数门禁的真实归属 |
| 未验证内容 | 未执行任何构建/测试；未读 E2E 运行证据；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 MU1b 的候选构建与 receipt 能否实际产出（属 apply 阶段 merger 职责）；变更目录 `git status` 为 `?? openspec/changes/sync-scope-and-pwa-client/`（未跟踪），**因此无法用版本控制逐行 diff 出 Round 6 之后的改动**，本轮以「按当前字节重新复现全部机械判据 + 逐项核实本轮声明的修复」替代 diff |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r7.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 7） |

### 本轮声明的修复逐项核实（F17–F20）

| 问题 | 位置 | 结论 | 依据 |
| --- | --- | --- | --- |
| **F17（Round 6 MAJOR）** TP1 的 Contract Freeze 由 `@{sync,views,command}.rs@MU1a` 改为按 Write Scope 归属命名工作包 ID | `plan.md:867` | **已解决** | 单元格现为 `schemas/sync/v1/**@WP1`、`crates/sync-protocol/src/sync.rs@WP1`、`command.rs@WP1`、`views.rs@WP2`，四条 token 全部落在 TP1 的 Dependencies（`code:WP1, code:WP2`）内；11 个包 21 条 `@` 引用去反引号后逐条比对**零越界**，已无任何交付单元 ID 出现在 `@` 位置。归属亦吻合：`sync.rs`/`command.rs` 在 WP1 的 Write Scope（`plan.md:860`），`views.rs` 在 WP2 的（`plan.md:861`），`schemas/sync/v1/**` 由 `plan.md:913` 的区域收窄排除 event-views（归 WP2） |
| **F17 第三问** 冻结语义是否随 `@MU1a` 一起丢失 | `plan.md:939` | **未丢失** | TP1 的 handoff 行 Accepted Revision 写「MU1a 的合入提交（其 premerge PASS 与独立候选检视绑定的候选提交）」，Start Revision 写「MU1a 合入后的 `refs/heads/main`」，Transfer 列写明「TP1 从该合入提交分支；`crates/sync-protocol/src/{sync,command,views}.rs` 的新枚举与分页类型在该提交中可编译」；与 `plan.md:965`、`tasks.md:5.4`、`tasks.md:6.9` 四处一致 |
| **F18（Round 6 MINOR）** TP1 缺 Dependency Handoffs 行 | `plan.md:939` | **已解决** | 新增行 Upstream=`WP1, WP2`，与 TP1 依赖目标集合相等；Invalidation 列写明「MU1a 的契约变更 → TP1 的向量与 `schema_drift.rs` 断言失效，需重跑 PV1」，与 `plan.md:923` 的 `envelope_fixtures.rs` 共享写点登记方向一致 |
| **F19（Round 6 MINOR）** Completion Criteria 写「三个交付单元」、Premerge History 只列 MU1/MU2/MU3 | `plan.md:1038`/`:1039` | **核心已解决，残留见 F22** | `:1038` 现为「全部四个交付单元（MU1a / MU1b / MU2 / MU3）已合入」，`:1039` 现为「MU1a/MU1b/MU2/MU3 各有对应 PASS」。但 Coverage Index 的 R1–R4 Closure Unit 列（`plan.md:29-32`）与其余叙述性位置仍为旧 `MU1` |
| **F20（Round 6 MINOR）** tasks.md §5/§6 未随拆分同步、§6 编号 | `tasks.md:76-113` | **编号与单元任务已解决，残留见 F21/F23/F24** | §5 现为 5.1/5.2(MU1a)/5.3(MU2)/5.4(MU1b)/5.5(MU3)；§6 为 6.1–6.8(MU1a)、6.9–6.13(MU1b)、6.14–6.21(MU2)、6.22–6.29(MU3)，共 29 项**连续、无重复、无跳号**（96 个任务 ID 全局唯一）。6.3 已按要求显式要求实测 `check:assets` 计数门禁。但 `tasks.md:88`（6.5）仍写旧单元名 `MU1` |

### Round 6 结论在当前字节的保持情况

| Round 6 结论 | 本轮结论 | 复核依据（当前文本 + 仓库/工具事实） |
| --- | --- | --- |
| F17（MAJOR）TP1 的 `@MU1a` 不合格 | **已解决** | 见上表；21 条 `@` 引用零越界 |
| F18 TP1 无 handoff 行 | **已解决** | `plan.md:939` |
| F19 单元计数/命名陈旧 | **部分解决** | Completion Criteria 与 Premerge History 已改；残留见 F22 |
| F20 tasks.md 单元名与成员未同步 | **部分解决** | 编号与单元任务已同步；残留见 F21/F23/F24 |
| F13（Contract Freeze 机械不可核对） | **维持未解决（非阻断）** | 按 `workflow-check.mjs:91-98` 复现，11 个包的 `frozenContractReadable` 仍全为 false（多路径单元格不匹配 `^(\S+?)@(\S+)$`；单路径单元格因反引号使 `path.resolve` 落空）。11 条 `Serialization Reason` 全为 `NOT_APPLICABLE`，与「冻结不可读 ⇒ contract 视为硬依赖」口径下 11 行波次逐行相等自洽 |
| F14（WP7 的 handoff 漏 `WP1`） | **维持未解决（非阻断）** | `plan.md:936` 的 WP7 行 Upstream 仍为 `WP5, WP6, WP2`，而 `plan.md:866` 的 Dependencies 含 `contract:WP1` |
| F1/F2（WP3/WP4 的 `code:` 虚假）已解决 | 维持 | `crates/core/Cargo.toml` 依赖闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`；`crates/agent-host/Cargo.toml` 仍无 `sync-protocol`；`CORE_FORBIDDEN` 仍含 `sync-protocol` |
| F3/F8（`command.rs` 归属、TP3 依赖与波次）已解决 | 维持 | `plan.md:860`/`:918` 在位；`plan.md:869` 仍为 `code:WP5, code:WP6, code:WP7, contract:WP1`，波次 W5，7 条 `@` 引用全部命中依赖列 |
| F5（AC1 不可满足）已解决 | 维持 | `plan.md:984`/`:1017`、`tasks.md:4.1` 仍显式不验证广播 |
| F6/F7/F9/F10/F11/F12/F15/F16 已解决 | 维持 | `plan.md:913`/`:921`/`:922`/`:923`/`:924`/`:925`/`:926` 七行四列齐备；`tasks.md:11`/`:17` 已含 `sync.rs` |
| 无环、波次合法、池容量、写入归属、reviewer 独立性 | 全部维持 | 见 `checks` 各项：cycles=[]；11 行波次逐行等于 earliest；W1–W5 未超 `coding=3/testing=2`；字面 8 对写入重叠全部已登记；11 个包 Owner≠Reviewer 且各有独立检视任务 |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F21 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:88` | `tasks.md:88` 的 `6.5` 仍写 `6.5 MU1、main；核对 Coverage Index 中归属本单元的 Scenario 的实现…`，单元名为已不存在的 `MU1`。同组其余 7 项（`tasks.md:84/85/86/87/89/90/91`）均为 `MU1a`，`MU1b` 组（`:92-96`）与 `MU2`/`MU3` 组亦全部使用新单元名。F20 的修复覆盖了 §5 与 §6 的**编号与绝大多数单元名**，此行是唯一漏改 | 单元覆盖核对这一步落在无归属的单元名上：main 执行 6.5 时无法确定它核对的应是 MU1a 还是 MU1a+MU1b 的合并集合。**不构成 MAJOR**：不涉及依赖类型、波次或写入归属的声明不实；权威的单元成员与顺序在 `plan.md:962-967` 已正确声明，且 `workflow check --stage premerge` 的 `rowContext`（`workflow-check.mjs:908-919`）按 Merge Strategy 表而非 tasks.md 取单元成员，故机械门禁不受影响 | 把 `tasks.md:88` 的单元名改为 `MU1a`（本行位于 6.1–6.8 的 MU1a 组内，且 `tasks.md:95` 已有 MU1b 自己的 `6.12` 覆盖核对任务，二者不重复） | 新发现（Round 7） |
| DDR-sync-scope-and-pwa-client-r1-F22 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:29-32`（R1–R4 的 Closure Unit 列）；同类残留另见 `plan.md:997`、`:1001`、`:1031`、`:1034`、`:1042` | F19 的修正只落在 Completion Criteria（`plan.md:1038/1039`）。R1–R4 的 Closure Unit 仍为 `MU1/premerge`，而这四行的 Responsible Units 含 TP1（现属 MU1b）与 WP1（现属 MU1a），拆分后不再有单一的「MU1」单元可闭合。`plan.md:997` 的 IV1 Target Revision 仍写「MU1–MU3 全部合入后」、`:1001` 的 Code Review 分层仍写「MU1 只审合同资产与文档口径」、`:1031` 的下游失效范围仍写「MU1 契约变更」、`:1034`/`:1042` 仍写「MU1–MU3 已合入主分支」 | 单元级核对的可追溯性下降：`Closure Unit` 列不被任何机械判据读取（已 grep `workflow-check.mjs` 确认无 `closure` 相关读取），故不影响门禁；但 main 在 6.5/6.12/6.18/6.26 按 Coverage Index 做覆盖闭环时，R1–R4 的闭合单元名指向一个不存在的单元。**不构成 MAJOR**：与 Round 6 对 F19 的判定一致（叙述性单元名陈旧，非声明不实） | R1–R4 的 Closure Unit 改为 `MU1a+MU1b/premerge`（或按 R1–R4 各自的 Responsible Units 分别落到 MU1a/MU1b）；`plan.md:997`/`:1031`/`:1034`/`:1042` 的「MU1–MU3」改为「MU1a/MU1b–MU3」，`:1001` 的「MU1 只审合同资产」明确为 MU1a；`plan.md:960` 的拆分说明句中的「MU1」是描述被拆对象，**不应改** | 新发现（Round 7；F19 的残留部分） |
| DDR-sync-scope-and-pwa-client-r1-F23 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:78` | `tasks.md:78` 的 `5.3` 写「MU2、main 复核 independent 模式的 WP3/WP4/TP2 组成，确认上游 **WP1/WP2 已合入主分支**且契约冻结版本可读」，而 `plan.md:966` 的 MU2 Start/Readiness 列写的是「**MU1b** 已合入主分支且契约冻结可读」。MU2 的 `Order=3` 在 MU1b 的 `Order=2` 之后，权威的就绪条件以 plan.md 为准。同组的 `tasks.md:79`（5.4，MU1b）与 `plan.md:965` 一致 | main 若按 tasks.md 的字面复核 MU2 的就绪，会在 TP1 尚未合入时就把 5.3 勾掉，而 plan.md 的权威声明要求 MU1b 先合入。**不构成 MAJOR**：这不改变任何依赖类型、波次或写入归属；且 `scheduling-status.mjs:150-151` 对 `orderGroup` 更小的未合入单元会机械地产生 blocker「等待计划前序单元 MU1b 合入及回归」，实际合入顺序不可能被 tasks.md 的措辞倒置。属 F20 重排时 §5 与 plan.md 的一处未对齐 | 把 `tasks.md:78` 的就绪条件与 `plan.md:966` 对齐，写成「MU1a/MU1b 已合入主分支（MU1a 提供 WP1/WP2 的契约冻结、TP1 归 MU1b）且契约冻结版本可读」 | 新发现（Round 7；F20 重排的残留） |
| DDR-sync-scope-and-pwa-client-r1-F24 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:92-96`（MU1b 的 §6 组） | MU1b 组只有 5 项：6.9 合入负责人（重建 worktree、跑 PV1、复算计数、固定候选）、6.10 检查执行者（候选 PV1）、6.11 独立 reviewer（候选只读审查）、6.12 main（覆盖核对）、6.13 merger（premerge/receipt/合入/主分支回归）。对照 `templates/tasks.md:63-66` 的八项单元模板与 MU1a/MU2/MU3 三组的实际写法，MU1b 组缺两项：(a) 与 `tasks.md:84`（6.1）、`:98`（6.14）、`:106`（6.22）对应的**候选构建前机械核实目标仓库/主分支引用**；(b) 与 `tasks.md:91`（6.8）、`:105`（6.21）、`:113`（6.29）对应的**独立 reviewer 审查合并新增差异**。另 6.13 把「主分支回归」并入 merger 一项，而 MU1a/MU2/MU3 均把它单列为检查执行者任务（`tasks.md:90`/`:104`/`:112`） | MU1b 是四个交付单元之一，但计划中没有任何任务为它登记合入后的主分支差异复检；MU1a/MU2/MU3 都有。同时 6.13 让 merger 自行执行主分支回归，与另外三组「回归由检查执行者独立执行」的分工不一致。**不构成 MAJOR**：缺的是任务登记而非依赖声明；`roles/merger.md` 第 5 步本身要求 merger「核对实际合入结果与候选一致性，执行计划内必要主分支 Project Verify，返回新增差异供独立检视」，且 `procedures/acceptance.md:87-88` 允许「无新增差异不强制重复同范围人工 review，但需原结论及差异依据」——MU1b 候选建立在 MU1a 合入提交之上、只新增 TP1 自有差异，实践上很可能走复用路径。但该路径的依据目前只能由执行者临场判断，没有计划内的落点 | 在 MU1b 组补两项，与其余三组同构：候选构建前的机械核实（对照 `tasks.md:84`）、合入后由独立 reviewer 审查合并新增差异或由 main 记复用依据（对照 `tasks.md:91`）；并把 6.13 的「主分支回归」拆为检查执行者任务，与 `tasks.md:90`/`:104`/`:112` 对齐 | 新发现（Round 7；F20 新增 MU1b 组的结构差异） |
| DDR-sync-scope-and-pwa-client-r1-F14 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:936` | Round 4 提出、维持未解决。WP7 的 Dependencies 为 `code:WP6, code:WP5, contract:WP1, contract:WP2`（`plan.md:866`），但 handoff 行的 Upstream 只写 `WP5, WP6, WP2`，漏 `WP1`；9 条 handoff 中其余 8 条的 Upstream 集合均等于该包依赖目标集合 | `contract:WP1` 的上游在 WP7 行没有 Accepted Revision 与 Invalidation 落点。**不构成 MAJOR**：WP7 的两个代码上游 WP5/WP6 已在 Upstream 中，且 WP7 在 W4 而 WP1 在 W1，波次已排序，缺的是一条已满足的 contract 边的失效登记 | 在 `plan.md:936` 的 WP7 行 Upstream 补 `WP1`，Invalidation 列补「WP1 的 `schemas/sync/v1/**` 变更 → WP7 重跑 PV3」 | 未解决（Round 4 提出，Round 5/6/7 复核仍未修正；严重度与阻断判定不变） |
| DDR-sync-scope-and-pwa-client-r1-F13 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:859-870`（11 个包的 Contract Freeze 单元格） | Round 3 提出、维持未解决。复现 `workflow-check.mjs:91-98` 的 `frozenContractReadable`：11 个包的多路径单元格不匹配 `^(\S+?)@(\S+)$`，单路径单元格因反引号使 `path.resolve` 落空，**11 个包全部 readable=false** | 冻结可核对性依赖人工而非机械判据。**不构成 MAJOR**：所引路径逐个存在、无一条写错；且当前波次合法性正是在「冻结不可读 ⇒ contract 视为硬依赖」这一口径下逐行自洽（见 Assessment） | 去掉单元格内的反引号并把多路径引用拆到单条，或改用工具接受的分隔写法；**注意**：若规范化后冻结变为可读，必须同步为受影响包补合法 `Serialization Reason` 码，否则 `workflow check --stage plan` 会转 FAIL（耦合关系见 Assessment） | 未解决（Round 3 提出，Round 4/5/6/7 复核仍未修正；严重度与阻断判定不变） |

## 说明（不计为问题）

- **`workflow check --stage plan` 报出的五条「缺少执行者接收确认」（WP2 Attempt 1 coding / Attempt 1 reviewing / Attempt 2 fixing、WP1 第 2 轮 reviewing、WP2 第 3 轮 reviewing）属已知、用户已接受的派发台账过程缺口，不是规划缺陷。** 根因是派发说明未要求执行者运行 `dispatch --ack`，attempt 推进后无事后补录入口；五个执行者均已确认收到并实际交付（`verification.md` 的 `## Dispatch Reconciliation`），用户 2026-10-03 裁定选 (a) 接受为已知偏离。本报告不将其计为 Finding，亦不影响本轮结论；按该裁决，final/archive 阶段须在 `## Final Assessment` 中一并复述。
- **门禁的第六条 error「`Dependency Declaration Review 的 DDR-sync-scope-and-pwa-client-r1 Plan Revision 未绑定当前规划契约摘要」是登记滞后，非规划缺陷。** `verification.md` 的 `## Dependency Declaration Review` 最新行仍为 Round 5 绑 `plan-v2:sha256:6751091d…`，而当前摘要为 `8eb6a3eb…dd4ad`。按 `procedures/scheduling.md`「机械格式检查失败先修记录」，由主 Agent 在导入本报告时把该表补一行 Round 7（Reviewer `reviewer-plan-ddr-7`、Plan Revision `plan-v2:sha256:8eb6a3ebabd0b59bdffd345be20887ef4af905f572b4943da72dcd55e00dd4ad`、Result PASS、Report Path 不带反引号的纯路径）即可消除该条。
- **`tasks.md:86`（6.3）要求 MU1a 候选实测 `check:assets` 的用例计数门禁——方向正确，但门禁的落点写错了一处。** 该风险的实体是 `crates/sync-protocol/tests/envelope_fixtures.rs:25,27` 的两个计数常量（主分支 67/9，`.worktrees/wp1` 已抬到 73/14），由 `every_manifest_case_behaves_as_declared`（`:117`，断言于 `:196-207`）在 `cargo test` 下断言，即 `check:rust`；而 `package.json` 的 `check:assets`（`scripts/check-contract-assets.mjs`）没有任何硬编码计数断言，只在成功摘要里打印计数、对 manifest 只校验 transcriptVectors 文件存在（`:596-605`）。**不构成 Finding**：6.3 的完成条件是「按 [PV1] 完成候选 Project Verify」，而 PV1 = `npm run verify` = `check` + `check:rust`，真实计数门禁确实在该任务下被跑到；只是正文点名的脚本与实际断言所在不符，属措辞精度问题，不改变该任务必须实测的事实。顺带确认：MU1a 只装 WP1+WP2 而 TP1 的 9 条向量不在其中，组装后计数是否仍等于 73/14 确实只能实测得知——6.3 保留这一步是必要的，不是多虑。
- **TP1 的波次声明未因拆分而失真。** `Execution Waves` 的波次是静态最早可开工层级，Merge Strategy 的 Order 是合入顺序，两者本就是不同维度；TP1 声明 W2（`code:` 上游 WP1/WP2 在 W1，earliest=2）并以 Enter Condition 文字记录「实际开工落在 MU1a 合入之后」，是把合入顺序如实写进开工条件。`workflow-check.mjs:324-326` 只在 `waveIndex > minimum` 时才要求理由码，TP1 恰为相等，故 `NOT_APPLICABLE` 合法。
- **`plan.md:960` 拆分说明句中的「MU1」是正确的**（描述被拆的对象），不计入 F22；`verification.md` 中「MU1 前批量修复」「随 MU1 前同步」等表述是对合入前时点的历史记账，保留原样正确，不计入 F22。
- **`plan.md:963-967` 的 `Owners: Implementation / Test / Review / Merge` 列在四个单元里各只列一名 reviewer**（MU2 列 `review-3` 而成员含 WP4 的 `review-4`，MU3 列 `review-5` 而成员含 WP6/WP7 的 `review-6`/`review-7`）。该列不被任何机械判据读取（`deliveryUnits()` 只取 `Delivery Unit / Mode` 与 `WP / TP`），权威的 reviewer 分配在 Work Packages 的 Reviewer 列与 `tasks.md` §3 的检视任务中逐条列明且无自审。属摘要列的简写，不计为 Finding（与 Round 4/5/6 同一口径）。
- **Coverage Index 的 `Final Checks` 列出现 `EV1`（`plan.md:29-49` 共 19 行），但 `EV1` 未在本变更的 `## Project Verify` 表（`plan.md:980-990`，仅 PV1/PV2/PV3/IV1/AC1/AC2/AC3）中定义，也未出现在 `tasks.md` 与 `verification.md`。** 它同样不出现在 `agentic-coverage` 结构化块的 `checks` 字段中（该字段 102 行只取 {PV1,PV2,PV3,AC1,AC2,AC3}），因此不参与任何机械判据，也不影响本轮结论。仅记录供主 Agent 在最终验收时确认 `EV1` 是历史遗留标签还是有待定义的检查。
- **MU1a 的 Merge Worktree 措辞与实际记录的落差维持 Round 6 的判断。** `plan.md:969` 规定候选在 `.worktrees/mu1a-merge` 构建，而 `tasks.md:85`（6.2）如实记录实际路径为 `.worktrees/mu1-merge`；执行记录与计划措辞的对账事项，不改变任何依赖结论。

## Assessment

### 本轮结论

**PASS**。Round 6 的唯一阻断项 F17（MAJOR）已真正关闭，F18（MINOR）同步关闭；F19/F20 的**核心部分**（Completion Criteria 与 Premerge History 的四单元命名、tasks.md §5/§6 的单元任务与连续编号）已关闭，其残留降级为非阻断的 MINOR。Round 6 的全部实质结论对当前字节继续成立。

逐项依据：

1. **F17 真正解决，三问全部核实通过。** (a) 21 条 `@` 引用逐条落在各自 Dependencies 列内，零越界，已无任何交付单元 ID 出现在 `@` 位置；(b) TP1 新列的四条冻结路径与 Write Scope 归属逐条吻合（`sync.rs`/`command.rs`→WP1、`views.rs`→WP2、`schemas/sync/v1/**`→WP1 且由 `plan.md:913` 的区域收窄排除 WP2 独占的 event-views）；(c) 冻结语义未随 `@MU1a` 丢失——「在 MU1a 的合入提交上冻结」已由 `plan.md:939` 的 Accepted Revision 承载，并与 `plan.md:965`、`tasks.md:5.4`、`tasks.md:6.9` 一致。Round 6 据此判 FAIL 的唯一依据已消除。
2. **F18 关闭。** `plan.md:939` 新增 TP1 的 handoff 行，Upstream=`WP1, WP2` 与其依赖目标集合相等，Accepted Revision 为 MU1a 的合入提交，Invalidation 写明 MU1a 契约变更的重跑责任。9 条 handoff 中 8 条 Upstream 等于依赖目标集合，唯一例外仍是 F14（WP7 漏 WP1，非阻断）。
3. **F19 核心关闭、残留为 MINOR。** `plan.md:1038`/`:1039` 已逐一命名 MU1a/MU1b/MU2/MU3，main 收尾时逐条核对的清单不再漏项。残留是 Coverage Index 的 R1–R4 Closure Unit 与五处叙述性位置仍用旧 `MU1`（F22）——该列不被任何机械判据读取，与 Round 6 对 F19 的 MINOR 判定一致。
4. **F20 核心关闭、残留为 MINOR。** tasks.md 96 个任务 ID 全局唯一、§6 为 6.1–6.29 连续 29 项、Coverage Index 引用的 17 个任务 ID 全部解析、`tasks.md:84` 对 6.6 的正文引用有效——重编号未引入悬空引用。新增的 MU1b 组（6.9–6.13）覆盖了候选构建、PV1、独立 review、覆盖核对、premerge/receipt/合入，与 MU1a 的拆分意图一致；6.3 已按要求显式要求实测计数门禁。残留三处：`tasks.md:88` 仍写旧单元名 `MU1`（F21）、`tasks.md:78` 的 MU2 就绪条件未与 `plan.md:966` 的「MU1b 已合入」对齐（F23）、MU1b 组较模板少「候选前机械核实」与「合入后差异复检」两项且把主分支回归并入 merger（F24）。
5. **依赖声明属实性 —— 维持，双向核实。** 8 条 `code:` 边成立（TP1 的测试必须链接 WP1/WP2 的 Rust 类型化镜像；WP6/WP7/TP3 必须 import 上游 TS 模块；TP4 针对 WP7 的页面与选择器）。反向侧同样成立：`crates/core/Cargo.toml` 依赖闭包仍只有 `async-trait`/`thiserror`/`p256`/`sha2`，`crates/agent-host/Cargo.toml` 无 `sync-protocol`，`CORE_FORBIDDEN`（`check-crate-boundaries.mjs:106`）含 `sync-protocol`，`docs/MODULE_ARCHITECTURE.md` §5 矩阵的 core 行整行空白——故 WP3/WP4/WP5 的纯 `contract:` 降级不是虚假声明。
6. **波次、无环、池容量、Serialization Reason —— 全部通过。** cycles=[]；11 行声明波次与 earliest 逐行相等；上游同批/更早批 11/11 通过；11 行理由全为 `NOT_APPLICABLE` 且无 `waveIndex > minimum` 的行；W1=2 coder、W2=3 coder+1 tester、W3=1 coder+1 tester、W4=1 coder、W5=2 tester，对 `coding=3/testing=2` 均未超容。MU2/MU3 的「MU1b 已合入」与 `orderGroup` 阻塞方向一致，无反向依赖。
7. **写入归属 —— 通过。** 按字面展开得 8 对重叠（工具口径 1 对 + 人工判断 7 对），逐对命中已登记行且四列齐备；`ownershipProblem()` 对多写者行返回空串。无「同一处却同批未登记」的情形。
8. **reviewer 独立性与独立检视任务 —— 通过。** 11 个包 Owner 与 Reviewer 无一相同；11/11 至少一条引用它且不含 `[wp:…]` 标签、含检视标记的任务；每个包恰好一条派发任务。
9. **交付单元解析与 Order —— 通过。** `deliveryUnits()` 实跑返回 MU1a/MU1b/MU2/MU3 四单元、mode 均 `independent`、无抛错，成员并集恰为 11 个已声明 ID，orderGroup 1/2/3/4。MU1b 的两条 `code:` 边指向 MU1a，与 `scheduling-evidence.mjs:157-162`（跨单元 `code:` 上游须 `merged`）和 `scheduling-status.mjs:150-151`（前序 orderGroup 阻塞）方向一致。
10. **Coverage Index 引用有效性 —— 通过。** 102 行、ID 无重复、`target_ref=refs/heads/main`；tasks 引用全部存在；checks 仅取 {PV1,PV2,PV3,AC1,AC2,AC3}。

按角色判定规则（存在已确认且未解决的 CRITICAL/MAJOR 问题时为 FAIL），本轮无 CRITICAL/MAJOR，故结论为 **PASS**。本轮 5 项发现（F21–F24 为新登记缺口，F22/F23/F24 分别承接 F19/F20 的残留，F14/F13 为结转）**全部为 MINOR**，不阻断交付；建议与 F13/F14 一并在同一轮内修正——修改 `plan.md`/`tasks.md` 会改变 `planningDigest`，届时本轮 PASS 对新摘要失效，须按同一 Review ID 递增 Round 复核。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify` 与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足 | 否 |
| AC1 | 待补（apply 阶段）；可满足性已核对 | 四项断言逐条对照 `crates/server/src/node_link/resource.rs:151-155`/`:1026-1032` 后确认不依赖广播（Round 1 F5 的不可满足性已消除），本轮该处字节未变 | 否 |
| Main E2E 判据 | 已核对（沿用 Round 4/5/6） | `not-applicable` 的理由与用户降级批准原话在 `plan.md:1004-1013` 与 `verification.md` 同名节；本轮未发现与该判定冲突的新事实 | 否 |
| F17 三问（引用 ⊆ 依赖 / 归属吻合 / 语义未丢） | **全部通过** | 21 条引用零越界；四条路径与 Write Scope 逐条吻合；冻结语义由 `plan.md:939` 承载 | 否（已关闭原阻断项） |
| F18（TP1 的 handoff 行） | **通过** | `plan.md:939` 四列齐备、Upstream 等于依赖目标集合 | 否 |
| F19（单元计数/命名） | **部分通过** | Completion Criteria 与 Premerge History 已改；R1–R4 Closure Unit 及五处叙述性位置仍为旧 `MU1`（F22） | 是（已计入 F22，非阻断） |
| F20（tasks.md 同步与编号） | **部分通过** | 96 个 ID 唯一、§6 连续 29 项、交叉引用无悬空；残留 `tasks.md:88` 旧单元名（F21）、`tasks.md:78` 就绪条件未对齐（F23）、MU1b 组结构缺两项（F24） | 是（已计入 F21/F23/F24，非阻断） |
| 无环 / earliest 双口径 / 池容量 / 理由码 | **通过** | cycles=[]；11 行波次逐行相等；W1–W5 未超池；11 行理由 `NOT_APPLICABLE` 且无需理由码 | 否 |
| Contract Freeze 引用 ⊆ Dependencies | **通过** | 11 包 21 条引用零越界（本轮已从 Round 6 的 18/19 提升到 21/21） | 否 |
| Contract Freeze 路径可读性 | **不通过（机械口径）** | 11 个包 readable 全部 false（F13）；路径本身存在，引用 ⊆ Dependencies 本轮全通过 | 是（已计入 F13，非阻断） |
| Shared File Ownership 覆盖 | **通过** | 字面 8 对重叠逐对已登记且四列齐备；工具口径唯一命中的 `docs/SYNC_PROTOCOL.md` 由 `plan.md:912` 覆盖 | 否 |
| Dependency Handoffs 完整性 | **部分通过** | 9 条中 8 条 Upstream 等于依赖目标集合；WP7 行漏 `WP1`（F14） | 是（已计入 F14，非阻断） |
| Reviewer 独立性与检视任务 | **通过** | 11 个包 Owner≠Reviewer；每包至少一条独立 review 任务、恰好一条派发任务 | 否 |
| 交付单元解析与 Order | **通过** | `deliveryUnits()` 返回四单元、mode `independent`、无抛错、成员并集=11、无重复、orderGroup 1–4 | 否 |
| Coverage Index 引用有效性 | **通过** | 102 行 ID 无重复；tasks 引用全部存在；checks 仅取已声明 Check ID | 否 |
| WP2/WP1 派发台账 ack 缺口 | **上下文，非规划缺陷** | `verification.md` 的 `## Dispatch Reconciliation` 记录根因、五个执行者的实际接收证据与用户 2026-10-03 的 (a) 裁决 | 否 |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md` DDR 表最新行仍绑 Round 5 摘要 `6751091d…`；导入本报告后补 Round 7 行即可消除该条 error | 否（属主 Agent 登记刷新） |

### 不确定性与后续建议

- **修正建议会改变 planningDigest。** F21–F24 与 F13/F14 的任何修正都会改动 `plan.md` 或 `tasks.md`，使当前摘要 `8eb6a3eb…dd4ad` 失效。修正后须重跑 `npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json` 固定新摘要，并按同一 Review ID 递增 Round 8 复核。建议在同一轮内一并修正，避免多次改摘要。
- **F13 与波次的耦合（沿用 Round 3–6）。** 当前波次合法性依赖「冻结不可读 ⇒ contract 视为硬依赖」这一事实。若按 F13 建议规范化 Contract Freeze 单元格，必须同步为受影响包补合法理由码，否则 `workflow check --stage plan` 会转 FAIL。规范化不是本轮要求，仅记录该耦合关系。
- **MU1b 的候选与 receipt 能否实际产出，本轮无法验证。** `tasks.md:85` 记录 MU1a 候选已组装为固定提交 `683dbbb…` 并在等本轮 DDR 绑定；TP1 的交付在 MU1a 合入提交上的完整门禁尚未执行，MU1b 的 receipt 尚不存在，`envelope_fixtures.rs` 的最终计数（TP1 的 9 条向量叠加在 WP1 的 73/14 之上）只能实测得出。这是 apply 阶段 merger 的职责，不影响本轮规划判据的结论。
- **`MU1a` 是否会被再次重命名或拆分，属计划演进风险而非当前缺陷。** 本轮的结论基于当前四单元声明；F21/F22/F24 的修正方向恰恰是让 tasks 与 plan 的单元名一致，从而使后续再拆分时的对账面更小。
- **本报告不宣称任何实现、测试、E2E 或最终验收通过。** MU1a 的候选 PV1、独立 review 与 premerge 门禁均未执行；`## Checks` 六行仍全为 NOT_APPLICABLE。

PASS
