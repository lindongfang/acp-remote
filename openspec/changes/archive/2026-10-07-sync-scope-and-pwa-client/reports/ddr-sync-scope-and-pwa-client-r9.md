<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 9。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-9"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7、coding-5b、testing-1..testing-4）"
target_revision: "plan-v2:sha256:b30b2f8aad775393c0ffac3352f478f8ebecd3d8769fb43fc81e9c681dbe0906"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用 ⊆ Dependencies、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 的独立检视任务、Coverage Index（叙述表 + agentic-coverage 结构化块）与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order、Runtime Resources 六列完整性，以及工作包所依赖的行为契约可满足且无冲突。本轮为 Round 8 FAIL 后的 recheck：逐项复核 F13–F34，并独立核实本轮新增的两处改动（叙述 Coverage 的 Closure Unit 重写、tasks.md §6 的 6.1–6.63 机械重编号）。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。仅新增本报告文件。全部机械判据以内联 node --input-type=module -e / node -e 复现，不落盘临时脚本；未执行任何构建、测试、E2E 或 Project Verify。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "退出码 1；planningDigest=plan-v2:sha256:b30b2f8aad775393c0ffac3352f478f8ebecd3d8769fb43fc81e9c681dbe0906（与调度者给定值一致）；contractDigest=sha256:7195f765bc78184e86535810c4e795e19c9dc840d7da0ca9649fd2935a0c3d64（与给定值一致）；requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（与给定值一致）。errors 恰为两条：DDR Plan Revision 未绑定当前摘要、DDR 结果必须为 PASS（当前 FAIL）。无「缺少执行者接收确认」类错误（台账已重置）"
    note: "变更目录未被 Git 跟踪，无法逐行 diff；本轮以「按当前字节重新复现全部机械判据 + 逐项核实本轮声明的两处改动」替代 diff"
  - id: "交付单元解析与 Order（实跑 deliveryUnits）"
    command: "node --input-type=module -e \"import('./node_modules/@dongfanglin/openspec-agentic/src/scheduling-evidence.mjs').then(m=>console.log(m.deliveryUnits(plan)))\""
    result: "返回数组 8 项：MU1a(order1,{WP1,WP2})、MU1b(order2,{TP1})、MU2(order3,{WP3,WP4,TP2})、MU3a(order4,{WP5a})、MU3b(order5,{WP5b})、MU3c(order6,{WP6})、MU3d(order7,{WP7})、MU3e(order8,{TP3,TP4})；mode 全为 independent；并集恰为 12 个已声明 ID，无重复、无遗漏"
  - id: "Coverage 结构化块与 5 份 delta spec 的逐标题对应（102 行全量解析）"
    command: "node --input-type=module -e（解析 plan.md 的 agentic-coverage 块与 5 份 spec 的 ### Requirement / #### Scenario 标题集）"
    result: "102 行（R001–R102）全部有 source.path 与 source.heading；对 spec 标题的精确匹配缺失 0、场景行 requirement 归属错配 0；5 份 spec 的 58 个 ### Requirement/#### Scenario 标题（4+13+5+19+11+36+1+4+2+7 中的 Requirement+Scenario）全部被至少一行引用，无孤立标题、无重复 ID"
  - id: "Coverage 结构化块的任务映射（重编号回归检查）"
    command: "node --input-type=module -e（统计 102 行的 tasks 引用分布，并按 planning-model.mjs:37-40 的 taskPackages 口径重算任务→包映射）"
    result: "引用分布：2.1×17、2.2×29、2.3×29、2.4×19、2.5×12、2.6×10、2.7×31、2.8×22、4.1×21、4.2×4、4.3×5、4.4×46、4.5×21、4.8×34、4.10×37、4.11×37、4.13×14、4.14×14。任务→包：2.5→[WP5a]、2.6→[WP5a,WP5b]、2.7→[WP6]、2.8→[WP7]；引用 2.6 的 10 行恰为 R045–R049 与 R084–R088，引用 2.7 的 31 行恰为 R005–R014+R050–R061+R080–R088，引用 2.8 的 22 行恰为 R062–R083。旧 F28 的「2.8 零引用 / 2.6=WP6 / 2.7=WP7」错位已消除；但 2.6 额外把 WP5a 计入（见 F35）"
  - id: "叙述 Coverage 的 Closure Unit = 该行 Responsible Units 中 Order 最大的单元"
    command: "node --input-type=module -e（逐行取 Responsibility 列与 Merge Strategy 的 order 映射比对 R1–R22）"
    result: "22/22 逐行相等：R1/R4→MU1b、R2/R3/R10–R18/R20→MU3e、R5–R9/R21/R22→MU2、R19→MU3d。与 deliveryUnits 实跑的 order 一一对应，无一行指向不存在的单元"
  - id: "tasks.md §6 的 6.1–6.63 重编号与交叉引用"
    command: "node --input-type=module -e（按 (6.x 前缀, 单元名) 分组计数）"
    result: "MU1a 6.1–6.8（8 项）、MU1b 6.9–6.15（7 项）、MU2 6.16–6.23（8）、MU3a 6.24–6.31（8）、MU3b 6.32–6.39（8）、MU3c 6.40–6.47（8）、MU3d 6.48–6.55（8）、MU3e 6.56–6.63（8）。正文交叉引用：6.1 的「6.6 合入瞬间再核实」指向 MU1a 的 premerge/合入行（正确）；7.1 的「依赖 6.58」指向 MU3e 的候选 Project Verify（正确）；8.1/8.2/8.3、9.1 依赖链均指向真实存在的编号"
  - id: "Contract Freeze 引用 ⊆ Dependencies（12 包逐单元格）"
    command: "node --input-type=module -e（解析 Work Packages 表，剥离反引号后逐条比对 Contract Freeze 的 @ 目标与 Dependencies 的 code:/contract: 目标）"
    result: "22 条 @ 引用：WP3 的 @WP2/@WP1、WP4 的 @WP2、WP5a 的 @WP1×2、WP5b 的 @WP1、WP6 的 @WP1、WP7 的 @WP2、TP1 的 @WP1×3/@WP2、TP2 的 @WP3/@WP2、TP3 的 @WP1×2/@WP5a/@WP5b/@WP6×2/@WP7×2 全部落在依赖列内；WP1/WP2/TP4 为 none + NOT_APPLICABLE，无引用。Round 8 的 F27（WP5b 越界）已消除，本轮越界数 0"
  - id: "Contract Freeze 机械可读性（F13 复核）"
    command: "node --input-type=module -e（按 workflow-check.mjs:91-98 与 planning-model.mjs:59 的同一正则 ^(\\S+?)@(\\S+)$ 逐包判定，并对匹配到的路径做 fs.accessSync）"
    result: "12 个包全部 readable=false（多路径单元格与反引号均导致不匹配）；22 条 @ 引用路径逐个 fs.accessSync 存在（device-proof.json、command.schema.json、event-views.schema.json、manifest.json、views.rs、sync.rs、command.rs、ports.rs 等）"
  - id: "Execution Waves 与依赖列一致性（hardDeps 口径复算）"
    command: "node --input-type=module -e（按 workflow-check.mjs:205-221 复算 earliest，并模拟「规范化冻结后」的对照场景）"
    result: "当前口径（frozen 全 false）：earliest = WP1:1/WP2:1/WP3:2/WP4:2/WP5a:2/WP5b:3/WP6:4/WP7:5/TP1:2/TP2:3/TP3:6/TP4:6，与 12 行声明波次逐行相等；无环。对照：若任一包冻结变为可读（如 WP4 单包），其 earliest 由 2 降为 1，声明波次即不合法（须补合法 Serialization Reason + 证据路径）——这是 F13 不得单独规范化的机械依据"
  - id: "Runtime Resources 六列完整性与独占互斥"
    command: "node --input-type=module -e（逐行统计单元格数）+ 人工核对消费者"
    result: "7 行（含表头分隔行）全部 6 格；5 条资源行的 Checks/Work Packages 逐个列出（`WP1, WP2, TP1`、`WP3, WP4, WP2, TP1, TP2`、`WP5a, WP5b, WP6, WP7, TP3, TP4`、`TP4`、`TP4`），无区间写法残留（F32 已消除）；唯一标为独占的「浏览器运行环境」消费者只有 TP4，同批无第二个使用者"
  - id: "依赖类型声明属实性（对照仓库事实）"
    command: "read crates/core/Cargo.toml、crates/agent-host/Cargo.toml、scripts/check-crate-boundaries.mjs"
    result: "crates/core 的 [dependencies] 仅 async-trait/thiserror/p256/sha2；crates/agent-host 依赖 core/acp-protocol/tokio/async-trait/serde_json/thiserror/tracing/sha2/base64，无 sync-protocol；check-crate-boundaries.mjs:92 的 CORE_FORBIDDEN 含 sync-protocol。WP3/WP4 的 contract 降级与 WP5a/WP5b 的 contract 声明均非虚假"
  - id: "5 份 delta spec 的场景计数与 plan「Specs Revision」句"
    command: "node --input-type=module -e / grep -c（逐份统计 ### Requirement 与 #### Scenario 标题数）"
    result: "sync-snapshot-scope 4 需求/13 场景、core-derived-events 5/19、pwa-web-client 11/36、workspace-resolution 1/4（MODIFIED）、local-agent-host 2/7（ADDED）。plan.md:5 的「Specs Revision」句写 sync-snapshot-scope 4/15、core-derived-events 5/19、pwa-web-client 11/35、workspace-resolution 4、local-agent-host 6——两处场景数与实际不符（见 F41）"
  - id: "MU1b 缺失的工作包交付检查证据"
    command: "grep -n 'main-mu1a\\|MU1a 候选 683dbbb8' verification.md checks 节"
    result: "verification.md 的 `## Checks` 中 MU1a 候选（683dbbb8）一行 Result 为 PASS 但 Executor 记为 merger、Evidence 指向 reports/PV1.log（npm 第 109 行、cargo 第 229 行）；reports/PV1-main-mu1a.log 尚不存在。属规划外执行记录缺口，不改变本轮静态结论"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘任何脚本）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-9"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；不拥有任何工作包）"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 9
    stage: plan
    target_revision: "plan-v2:sha256:b30b2f8aad775393c0ffac3352f478f8ebecd3d8769fb43fc81e9c681dbe0906"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r9.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / proposal.md / design.md / 5 份 delta spec 做只读复审。planningDigest 由 workflow check --stage plan --json 重新推导为 b30b2f8a…be0906，与调度者给定值一致。逐项复核 Round 8 的 F13–F34：13 项中 11 项已解决、F13 判定为非阻断未解决、F31 部分解决（残留两处失效 ID）。另独立核实本轮两处新增改动：叙述 Coverage 的 Closure Unit 22/22 行等于该行 Responsible Units 中 Order 最大者；tasks.md §6 的 6.1–6.63 重编号、单元内编号前缀、正文交叉引用（6.6、6.58）与 7.1/8.x/9.1 依赖均指向真实存在的项。新发现 5 项，全部为 MINOR/SUGGESTION，不阻断。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–8 的 Review ID） |
| Round | 9 |
| Review Type | plan |
| Review Stage | 计划/依赖声明阶段（Round 8 FAIL 后主 Agent 修正规划并刷新摘要，本轮既为 recheck 也为当前固定摘要的完整复审） |
| Reviewer | `reviewer-plan-ddr-9`（不拥有任何 WP/TP；owners 为 `coding-1..7`、`coding-5b`、`testing-1..4`，独立性成立） |
| Work Package | NOT_APPLICABLE（本轮范围是全部 12 包 + 8 个交付单元的声明） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:b30b2f8aad775393c0ffac3352f478f8ebecd3d8769fb43fc81e9c681dbe0906`（本轮第 1 次执行 `workflow check --stage plan --json` 即得此值，随后未再执行，版本稳定） |
| Requirements | `proposal.md`、`design.md`（D1–D8）、`specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`、`plan.md`、`tasks.md`、`verification.md`（同目录当前字节） |
| Project Rules | `AGENTS.md`；`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`templates/tasks.md`、`procedures/scheduling.md`；`openspec/agentic.yaml`；`docs/MODULE_ARCHITECTURE.md` §5；`docs/FRONTEND_DESIGN.md` §5 |
| Verification Evidence | 本轮不重复执行任何 Project Verify；`verification.md` 的 `## Checks` 六行为 NOT_APPLICABLE/PENDING（apply 阶段补），MU1a 候选一行记 PASS 指向 `reports/PV1.log`；`reports/PV1-main-mu1a.log` 尚不存在 |
| Check Plan | `plan.md` 的 `## Verification Strategy`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）；`verification.md` 的 `## Checks` 与 `## Check Plan Changes` |
| Previous Findings | F1–F11（Round 1）、F12/F13（Round 3）、F14（Round 4）、F15/F16（Round 5）、F17–F20（Round 6）、F21–F24（Round 7）、F25–F34（Round 8）；本轮按 Round 8 报告的 13 项（F13/F22/F23/F24/F25/F26/F27/F28/F29/F30/F31/F32/F34）逐项复核，并补核 Round 8 报告另列的 F14 与 F33 |
| 实际检查范围 | (a) Round 8 的 13 项逐项复核；(b) 本轮新增改动一：叙述 Coverage 的 Closure Unit 与 Responsible Units；(c) 本轮新增改动二：tasks.md §6 的 6.1–6.63 重编号、单元内分工与交叉引用；(d) 8 个交付单元解析与 Order；(e) 12 包波次与依赖列一致性、无环；(f) Contract Freeze ⊆ Dependencies 与机械可读性；(g) 结构化 Coverage 块 102 行与 5 份 spec 的逐标题对应；(h) Shared File Ownership 登记覆盖；(i) 运行时资源六列与独占互斥；(j) 依赖类型声明的仓库事实核对；(k) proposal/specs/design/plan 对同一条件的结果一致性（错误码、状态集合、快照范围、路径口径、投递通道） |
| 未验证内容 | 未执行任何构建/测试/E2E；未评估用例覆盖充分性（归 validator 与主 Agent 的 Coverage Index）；未判断 MU3a–MU3e 的候选能否实际产出（apply 阶段 merger 职责）；未核 Director 审批与角色独立性（门禁 boundary 已声明其不证明这两项） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r9.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行，规划阶段）；Main E2E `not-applicable` 的降级判据与三条替代检查入口 |

## Findings

复核沿用原问题 ID 并注明本轮 target 与复核依据；新发现自 F35 起连续编号。

### Round 8 发现逐项复核（13 + 1）

| ID | Round 8 级别 | Round 8 结论 | 本轮复核 | 复核依据（文件:行） |
| --- | --- | --- | --- | --- |
| F25 | MAJOR | tasks.md 2.5 把 `src/platform/secure-storage.web.ts` 与 R11 指派给 WP5a | **已解决** | `tasks.md:24` 标题为 `- [ ] 2.5 [wp:WP5a] [PV3] 派发 WP5a coder`；`tasks.md:25` 正文写「实现 R10/R15/R19 的基础层……写入范围**不含** `src/platform/`（归 WP5b）」，R11 已移出。与 `plan.md:863`（WP5a Write Scope 不含 `src/platform/`）一致。**修复引入的映射副作用见 F35** |
| F26 | MAJOR | 游离的 `2.6` 行落在 `## 1.` 之前，WP5b 正文子条目被清空 | **已解决** | `tasks.md:26` 的 `- [ ] 2.6 [wp:WP5b] [PV3]` 现位于 `## 2. Implementation`（`tasks.md:13`）之内；`tasks.md:27` 的 WP5b 正文完整恢复六处行内标识：`CryptoKey`、`local-cache.web.ts`、`lifecycle.web.ts`、`.native.ts`、`fixtures/sync/v1/transcripts/device-proof.json`、`` 依赖：`code:WP5a` ``。全文 `` `` `` 计数不再是 0 |
| F27 | MAJOR | WP5b 的 Contract Freeze 引用 `device-proof.json@WP1` 但 Dependencies 无 `contract:WP1` | **已解决** | `plan.md:864` 的 WP5b Dependencies 现为 `code:WP5a, contract:WP1`；同行的 Contract Freeze 仍引 `fixtures/sync/v1/transcripts/device-proof.json@WP1`，引用 ⊆ 依赖列成立（本轮 22 条 @ 引用越界数 0）。Inputs/Outputs 列写明依据 |
| F28 | MAJOR | `agentic-coverage` 结构化块任务编号未随拆分重编（`2.8` 零引用） | **已解决** | 机械复算：`2.6` 被 10 行引用（R045–R049、R084–R088）、`2.7` 被 31 行（R005–R014、R050–R061、R080–R088）、`2.8` 被 22 行（R062–R083），三者均非零且分别对应 WP5b/WP6/WP7 的需求集。旧错位消除 |
| F29 | MAJOR | tasks.md §5/§6 仍按单一 MU3 单元，与 plan 的 MU3a–MU3e 矛盾 | **已解决** | `tasks.md:84-88` 新增 5.5–5.9 逐单元（MU3a–MU3e）复核；`tasks.md:116-155` 的 6.24–6.63 为 MU3a–MU3e 各 8 项、逐单元独立构造候选；`tasks.md:159` 的 7.1 依赖改为 6.58（MU3e 的候选 Project Verify）。与 `plan.md:971-975` 的 Order 4–8 一致 |
| F30 | MINOR | Completion Criteria 量词「四个」与同句枚举的八个单元矛盾 | **已解决** | `plan.md:1046` 现为「全部八个交付单元（MU1a / MU1b / MU2 / MU3a / MU3b / MU3c / MU3d / MU3e）已合入」，量词与枚举一致 |
| F31 | MINOR | plan/tasks/verification 共 22 处仍写已退役的 `WP5` | **部分解决** | `plan.md` 与 `verification.md` 的全部**在册登记**已无裸 `WP5`（Shared File Ownership、Dependency Handoffs、code 依赖依据、PV3 Stages、Runtime Resources 均已改）；但 `tasks.md:31`（WP7 正文「依赖：`code:WP5`、`code:WP6`、契约冻结」）与 `tasks.md:71`（4.10「依赖 `code:WP5`/`code:WP6`/`code:WP7`」）仍写失效 ID——两处都是**现行**依赖声明而非历史记账，见 F37 |
| F32 | MINOR | Runtime Resources 用区间 `WP5a–WP7`，无法判定是否含 WP5b | **已解决** | `plan.md:951` 与 `verification.md` 同名节现为逐个列举 `WP5a, WP5b, WP6, WP7, TP3, TP4`；同表 5 条资源行的 Checks 列均无区间写法 |
| F22 | MINOR | 叙述 Coverage 的 Closure Unit 与 IV1/交付状态仍写 `MU1`/`MU3` | **已解决** | 22 行 Closure Unit 逐行等于该行 Responsible Units 中 Order 最大者（机械复算 22/22）；`plan.md:1005` 的 IV1 Target Revision、`:1042`、`:1050` 均已改为 `MU1a–MU3e`。Responsible Units 的 WP5a/WP5b 归属与 Work Packages 表一致（R10/R15/R19→WP5a，R11/R20→WP5b） |
| F23 | MINOR | tasks.md 5.3 的 MU2 就绪条件只写 WP1/WP2 | **已解决** | `tasks.md:82` 现写「确认上游 **MU1a/MU1b 已合入主分支**且契约冻结版本可读」，并显式引用 `plan.md` 的 MU2 行 Start/Readiness |
| F24 | MINOR | MU1b 的 §6 组缺合入后主分支回归与合并差异复检 | **已解决（登记口径）** | `tasks.md:105`（6.14 检查执行者、主分支回归）与 `tasks.md:106`（6.15 独立 reviewer、合并新增差异复检）已补齐。两点残留：该组为 7 项而其余单元为 8 项（缺其他单元都有的「候选构建前机械核实目标仓库/主分支引用并留证」项），且 `verification.md:92`/`:193` 仍称新增了 `6.13a/6.13b`（重编号后实际为 6.14/6.15）。均为登记措辞，非依赖声明问题 |
| F34 | MINOR | `## Dependency Declaration Review` 的 Round 5 行被并入 Round 7 行（8 格 vs 6 列） | **已解决** | 解析 `verification.md:35-44`：表头 6 列，数据行 11 行全部 6 格；Round 5（`:42`，`reviewer-plan-ddr-5`，`6751091d…`，PASS，r5 报告路径）与 Round 8（`:44`，FAIL）各自独立成行；机械读取按最大 Round 取到 8 |
| F13 | MINOR | Contract Freeze 多路径单元格不匹配 `^(\S+?)@(\S+)$`，12 包全部 `readable=false` | **未解决（本轮判定：非阻断，可留后续）** | 12 包仍全部不匹配、`readable=false`（`plan.md:859-870`）；22 条 @ 引用路径逐个 `fs.accessSync` 存在且无写错；越界数 0。判定理由见下方「F13 取舍的独立判定」 |
| F14 | MINOR | WP7 的 handoff 行 Upstream 漏 WP1 且仍写已退役的 `WP5` | **已解决（但未登记）** | `plan.md:939` 的 WP7 Upstream 现为 `WP5a, WP5b, WP6, WP1, WP2`，Invalidation 列已补 WP1 schema 变更路径。但 `verification.md` 的 `## Review Findings` 中无该轮 F14 行，见 F38 |
| F33 | MINOR | 新增静态服务器行的 Resource 写「Vite/**Metro** dev server」，且六列未承载「仅验证环境」限定 | **部分解决（未登记）** | `verification.md` 的 `## Runtime Resources` 已补该行（F33 的第二部分已处理）；`plan.md:952` 的 Resource 仍为「前端静态服务器（Vite/Metro dev server）」，「Metro」保留。同样未登记进 `## Review Findings` |

### F13 取舍的独立判定

调度者要求独立判定「F13 本轮必须修（按 MAJOR 报）还是可留作后续非阻断项」。**判定：非阻断，属 MINOR，可留后续。** 依据：

1. **路径事实层面无错。** 22 条 `@` 引用逐个 `fs.accessSync` 存在，引用 ⊆ Dependencies 本轮越界数为 0。冻结的可核对性损失是机械口径而非事实错误。
2. **同类正则的执行侧不产生虚假绑定。** `planning-model.mjs:59` 用同一正则解析 Contract Freeze 生成 `frozenContract`；不匹配即 `null`，不会把错误内容写进 `planningHash`，也不会形成虚假的「已冻结」结论。
3. **规范化有实质连锁成本，且非本变更能力范围内。** 我按 `workflow-check.mjs:205-221` 的口径做了对照复算：把冻结改为可读后，`contract` 边不再计入 `hardDeps`，`earliest` 普遍下降（对照场景下 WP4 由 2 降为 1，WP6 由 4 降为 3，WP7 由 5 降为 4，TP2 由 3 降为 2，TP3 由 6 降为 5）。这些包将立即触发「批次早于最早可开工批次」或「Serialization Reason 与计划声明不符」，必须同步改写 Execution Waves 并为受影响包补合法理由码与可读证据路径。该改动面已超出「冻结引用可核对」本身的收益。
4. **现状自洽。** 当前波次合法性恰是在「冻结不可读 ⇒ contract 视为硬依赖」口径下逐行自洽（12/12 行等于 earliest，无 `waveIndex>minimum` 行）。

故我不同意把 F13 升级为 MAJOR，也不同意要求本轮必须修；建议在 apply 开工前或最终验收时一并决策，且**必须**与 Execution Waves 的改写同批进行（单独规范化会使门禁转 FAIL）。

### 新发现（Round 9）

本轮新发现 5 项，编号自 F35 起。

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F35 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:26`（2.6 的 WP5b 派发行，**判据限该行文本**）；映射证据见 `node_modules/@dongfanglin/openspec-agentic/src/planning-model.mjs:37-40` | `planning-model.mjs:37-40` 的 `taskPackages` 只取每个 `- [ ] N.M` 行的**同一行文本**，其中出现的包 ID 即算作该任务提及的包。`tasks.md:26` 的行文为「派发 WP5b coder，按 apply 协议登记开工/接收；**依赖 WP5a 的交付提交已合入（MU3a）**」，因此在提到 WP5b 之外也提到了 `WP5a`。实测该口径下 `2.6 -> ["WP5a","WP5b"]`（而 `2.5 -> ["WP5a"]` 干净）。于是 Coverage 中引用 `2.6` 的 10 行（R045–R049 设备身份、R084–R088 imported 不落盘）被同时归入 **WP5a** 的 `coverage` 与 `sources` | WP5a 的 `planningHash` 把 WP5b 的 R11/R20 计为自身覆盖：按该口径 WP5a 落得 22 行覆盖（R042–R049、R062–R066、R080–R088），其中 R045–R049、R084–R088 共 10 行属 MU3b 的交付物；而 `plan.md:863` 明确 WP5a 的目标是 R10/R15/R19。后果是语义摘要与「一个需求只应归入其生产者包」的意图不符，且 main 在按 Coverage Index 为 MU3a 做单元闭环（`tasks.md:120` 的 6.28「核对归属本单元的 Scenario 的闭环」）时会为 MU3a 索取 R11/R20 的证据，而这两条要等 MU3b/MU3e。**不构成 MAJOR**：真实依赖声明与 Write Scope 均正确，偏差只影响规划摘要与人工闭环 | 保留派发行内的依赖叙述但避免把上游包 ID 写成与下游同等的「提及」，例如改为「派发 WP5b coder，按 apply 协议登记开工/接收；依赖其上游包的交付提交已合入（MU3a）」，或把依赖转到同任务的子条目行（子条目不计入 `taskPackages`） | 新发现（Round 9） |
| DDR-sync-scope-and-pwa-client-r1-F36 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:71`（4.10 的 TP3 用例设计任务）与 `plan.md:40`（叙述 Coverage 的 R12 行 Contribution） | 两处都要求按「连接 **9 态**」实现/覆盖全部迁移。而本变更自己的规范 `specs/pwa-web-client/spec.md` 的「连接状态是互斥状态机」需求枚举了 12 个名称（未配对、配对中、已断开、连接中、认证中、重放追平中、在线、重连中 + 被撤销、版本不兼容、主机身份变化、被其它标签页顶替），`docs/FRONTEND_DESIGN.md` §5 的状态图同样列出 12 个英文状态（unpaired/pairing/disconnected/connecting/authenticating/replaying/online/reconnecting + revoked/incompatible/identity_changed/replaced）。「9」的来源是原型 UI 的下拉选项数（`prototypes/acp-remote-pwa.html:632-639` 恰 9 项：online/syncing/reconnecting/offline/revoked/identity/incompatible/replaced/unpaired）与 `prototypes/IMPLEMENTATION-GAPS.md:37` | TP3 的用例设计与 WP6 的验收若按「9 态」的集合去判定「全部合法迁移」，会漏掉 `pairing`/`connecting`/`authenticating` 三个规范明确要求的状态及其迁移；这与 `proposal.md` 的 success_criteria 及 IV1 的覆盖判定直接相关。**不构成 MAJOR**：规范本身用「MUST 至少覆盖」并逐项枚举了正确集合，实现者以 spec 为准不会做错，受影响的是测试覆盖的完整性判定 | 把 `tasks.md:71` 与 `plan.md:40` 的「9 态」改为与规范一致的表述，例如「连接状态机（`specs/pwa-web-client` 的『连接状态是互斥状态机』需求枚举的全部状态）的全部合法迁移与阻断态停止重连」 | 新发现（Round 9） |
| DDR-sync-scope-and-pwa-client-r1-F37 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:31` 与 `tasks.md:71` | F31 声称「plan/tasks/verification 共 22 处逐处改为 WP5a/WP5b」，但 `tasks.md:31` 的 WP7 正文仍写「依赖：`code:WP5`、`code:WP6`、契约冻结」，`tasks.md:71` 的 4.10 仍写「依赖 `code:WP5`/`code:WP6`/`code:WP7`」。`WP5` 已不是合法包 ID（`plan.md:863-864` 只定义 WP5a/WP5b） | 这两处是**现行**任务依赖声明而非历史记账：按 `tasks.md` 派发 WP7/TP3 时，执行者读到的上游是三个 ID 中的两个不存在（`WP5` 无法解析到任何 worktree、契约冻结或交付提交）。与 `plan.md:866`（WP7 依赖 `code:WP6, code:WP5a, code:WP5b, contract:WP1, contract:WP2`）和 `plan.md:869`（TP3 依赖 `code:WP5a, code:WP5b, code:WP6, code:WP7, contract:WP1`）不一致。**不构成 MAJOR**：权威声明在 plan.md 且正确，tasks.md 的依赖不是机械判据的输入 | `tasks.md:31` 改为「依赖：`code:WP5a`、`code:WP6`、`contract:WP1`、`contract:WP2`」；`tasks.md:71` 改为「依赖 `code:WP5a`/`code:WP5b`/`code:WP6`/`code:WP7` 与 `contract:WP1`」 | 新发现（Round 9，F31 的残留） |
| DDR-sync-scope-and-pwa-client-r1-F38 | MINOR | `openspec/changes/sync-scope-and-pwa-client/verification.md` 的 `## Review Findings` 节（`:183-195`） | Round 8 报告共记 15 项发现（新增 F25–F34 与结转 F13/F14/F22/F23/F24），而 `verification.md` 该节只登记了 13 行：F25、F26、F27、F28、F29、F30、F31、F32、F22、F23、F24、F34、F13——**F14 与 F33 缺席**。该项节是 main 按「按问题 ID 逐项闭环」核对闭环的权威记录（`roles/reviewer.md` 的流程第 5 步与交付约定） | F14 的修复（`plan.md:939` 已改为 `WP5a, WP5b, WP6, WP1, WP2`）与 F33 的部分修复在台账中不可见，最终验收按问题 ID 核对时会读到「Round 8 的 F14/F33 未闭环」或直接漏检；F33 的残留（`plan.md:952` 的「Metro」）也因未登记而失去追踪点。**不构成 MAJOR**：属 `procedures/scheduling.md`「机械格式检查失败先修记录」同类的登记问题 | 在 `## Review Findings` 补 F14（已修：`plan.md:939`）与 F33（部分修：`verification.md` 资源节已镜像、`plan.md:952` 的 Resource 仍含「Metro」，需按 web-only 收窄）两行 | 新发现（Round 9） |
| DDR-sync-scope-and-pwa-client-r1-F39 | SUGGESTION | `openspec/changes/sync-scope-and-pwa-client/plan.md:863`（WP5a Write Scope）与 `plan.md:866`（WP7 Write Scope）对 `plan.md:918-929` 的登记表 | WP5a 的 Write Scope 含 `clients/app/app/` 与 `clients/app/src/{domain,protocol,components}/`，WP7 的含 `clients/app/app/`、`clients/app/src/features/`、`clients/app/src/components/`——`clients/app/app/` 与 `clients/app/src/components/` 两处字面重叠。登记表中覆盖前端目录级范围的行只有「`clients/app/`（WP5a/WP5b 的目录级范围）对 `clients/app/**/*.test.ts`（TP3）」与「…对 `clients/app/e2e/`（TP4）」两条，**没有** WP5a×WP7 的重叠行 | 两个包分处 W2 与 W5、MU3a 与 MU3d，且 WP7 有 `code:WP5a` 依赖排序，因此不构成同批冲突（Round 8 亦作同样判断）；登记缺口只影响「同一处」的可核对性——main 在 MU3d 闭环时无法从登记表直接读到这两处的合并顺序与后合入方重跑项。**非阻断**：跨单元串行合入已由 Merge Strategy 的 Order 与 `code:` 依赖保证 | 补一行登记：File=`clients/app/app/` 与 `clients/app/src/components/`（WP5a 的目录级范围对 WP7），Writers=`WP5a, WP7`，Merge Owner=`merger`，Merge Order=`WP5a → WP7`，Re-verify After Merge=`WP7: PV3`，Region Note 写明「WP5a 只建空壳路由与基础组件，WP7 只写页面与业务组件」 | 新发现（Round 9） |

## Assessment

### 本轮结论

**PASS**。按 `roles/_shared/role-report.md` 的判定：没有已确认的失败、必要输入齐备、目标版本固定且可读、机械与语义判据均已复现，故记 PASS，而非 BLOCKED 或 FAIL。

逐项依据：

1. **Round 8 的 5 项 MAJOR 全部关闭。** F25（`tasks.md:24-25` 的写入范围与需求已归位）、F26（`tasks.md:26-27` 的 2.6 落回 §2 且正文完整）、F27（`plan.md:864` 补 `contract:WP1`）、F28（结构化 Coverage 的 `2.6`/`2.7`/`2.8` 分布 10/31/22 行且分别对应 WP5b/WP6/WP7）、F29（`tasks.md:84-88`、`:116-155`、`:159` 全面对齐 MU3a–MU3e 与 7.1 的 6.58）。8 个交付单元经 `deliveryUnits()` 实跑为 `independent`，成员并集恰为 12 个已声明 ID，Order 1–8 与表序同向。
2. **本轮新增改动一（叙述 Coverage 的 Closure Unit 重写）经独立复算通过。** 22/22 行的 Closure Unit 等于该行 Responsible Units 中 Order 最大的单元（R1/R4→MU1b、R5–R9/R21/R22→MU2、R19→MU3d、R2/R3/R10–R18/R20→MU3e）；Responsible Units 的 WP5a/WP5b 归属与 Work Packages 表逐条一致（R10/R15/R19→WP5a，R11/R20→WP5b，无遗漏、无越界）；`plan.md:1005`/`:1042`/`:1046`/`:1050` 的单元名与量词均已同步。
3. **本轮新增改动二（tasks.md §6 的 6.1–6.63 重编号）经独立复算通过。** 单元内编号前缀与 Merge Strategy 的 Order 一一匹配；MU3a–MU3e 各恰 8 项；正文交叉引用（`tasks.md:92` 的「6.6 合入瞬间再核实」、`:159` 的「依赖 6.58」）与 `## 7.`/`## 8.`/`## 9.` 的依赖链全部指向真实存在的项。唯一不对称是 MU1b 组为 7 项（见 F24 行与 F35–F39 的说明），非依赖声明问题。
4. **结构化 Coverage 与 5 份 delta spec 的对应无误。** 102 行（R001–R102）逐标题精确匹配 spec 的 Requirement/Scenario 标题：缺失 0、`requirement` 归属错配 0、spec 标题未被引用 0。5 份 spec 的需求/场景计数为 4/13、5/19、11/36、1/4、2/7。
5. **声明属实性与行为契约一致性成立。** `code:`/`contract:` 声明对照 `crates/core/Cargo.toml`（仅 async-trait/thiserror/p256/sha2）、`crates/agent-host/Cargo.toml`（无 sync-protocol）、`scripts/check-crate-boundaries.mjs:92` 的 `CORE_FORBIDDEN` 三项仓库事实成立；快照范围（D1 三资源 × spec 单类缩为 `sessions` 时 `chunkCount` 1..3）、游标口径（D2 复合键与「排序不得依赖 UUID」）、路径口径（D5 规范化前缀 + `outsideWorkspace`）、节点级事件（D6 落库且不被会话级路径误收、覆盖层缺席呈现「状态未知」）在 proposal/specs/design/plan 四处表述一致，未发现互斥的错误码或断言。
6. **资源与波次自洽。** 12 行声明波次逐行等于按 `hardDeps` 重算的 earliest，无环；Runtime Resources 5 条资源行六列齐备、消费者逐个列出、唯一独占资源的同批使用者只有 TP4 一个。
7. **新发现 5 项（F35–F39）全部为 MINOR/SUGGESTION**，均有明确证据、可定位、可动作，但均为登记与措辞层面的偏差，不改变「依赖声明属实、批次与归属自洽、契约可满足」这一结论。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 | 待补（apply 阶段） | `verification.md` 的 `## Checks` 中 PV1/PENDING 行为 NOT_APPLICABLE；MU1a 候选一行记 PASS（证据 `reports/PV1.log`，npm 第 109 行 / cargo 第 229 行，Executor 记为 merger，属规划外执行记录） | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| PV2 / PV3 | 待补（apply 阶段） | 两行均 NOT_APPLICABLE；PV3 的入口与 `clients/app` 尚无代码 | 否 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE；目标版本已表述正确为「MU1a–MU3e 全部合入后」 | 否 |
| AC1 可满足性 | 静态核对通过 | `crates/server/src/node_link/resource.rs:144/154-158` 的会话级投递路径对 `None` 会话直接返回；plan/specs/design 已按此把需求收窄为「落库 + 不被会话级路径误收」，AC1 不含广播断言，与 design 的 Risks 段一致 | 否（本轮已作为一致性检查通过） |
| AC2 / AC3 可满足性 | 静态核对通过 | 入口分别为 `npm run verify` 与 `clients/app` 契约测试（读仓库根 `fixtures/sync/v1/`）；`fixtures/sync/v1/transcripts/device-proof.json` 存在（`plan.md:864` 与 `:1030` 引用同一路径）；两条入口均不依赖尚不存在的 Sync 入站面 | 否 |
| `reports/PV1-main-mu1a.log` | 不存在 | 由任务范围告知；`verification.md` 的 `## Checks` 亦无该行 | 否（属主分支回归的待补证据，应在 MU1a 合入后的门禁前补齐） |
| 变更目录的版本控制 diff | 不可用 | 目录未被 Git 跟踪（`git status --porcelain` 仅显示未跟踪），故本轮以按当前字节复现全部判据 + 逐项核实两处声明改动替代 diff | 否（已在 Review Context 记录该限制） |
| Contract Freeze 机械可读性 | 不通过（F13） | 12 包 `readable=false`；路径本身逐个存在；判定与理由见「F13 取舍的独立判定」 | 否（本轮判定为非阻断；如需规范化须与 Execution Waves 改写同批） |

### 不确定性与后续建议

- **F35 的修复会刷新 `planningDigest`**（`taskPackages` 与各包 `coverage`/`sources` 进入 `planningHash`），F36/F37/F38/F39 若不触及 `plan.md` 的已知节与 `tasks.md` 的包 ID 字面，则多数不刷新摘要。建议一次改完再重跑 `workflow check --stage plan --json` 固定新摘要，并按同一 Review ID 递增一轮复核，避免摘要抖动。
- **F13 与波次的耦合是本轮最重要的机械结论**：任何「规范化 Contract Freeze」的尝试都会同时改变 5 个以上包的 `earliest`，必须与 Execution Waves 及合法理由码同批处理（对照复算见 `checks` 的「Execution Waves 与依赖列一致性」条）。
- **MU3a–MU3e 的候选与 receipt 能否实际产出，本轮无法验证**，属 apply 阶段 merger 职责；但 F29 已关闭，tasks.md 现能产出五个单元各自的 receipt。
- **本报告不宣称任何实现、测试、E2E 或最终验收通过**；`## Checks` 的 PV1/PV2/PV3/IV1/AC1–AC3 仍待 apply 阶段补齐，MU1a 的候选 PASS 属规划外记录，须由主 Agent 在对应门禁前核对执行者与版本。

PASS
