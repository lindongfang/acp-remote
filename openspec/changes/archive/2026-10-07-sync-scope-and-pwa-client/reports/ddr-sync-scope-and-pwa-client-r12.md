<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 12。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-12"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7、coding-5b、testing-1..testing-4）"
target_revision: "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用 ⊆ Dependencies、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 的独立检视任务、Coverage Index（叙述表 + agentic-coverage 结构化块）与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order、Runtime Resources 六列完整性、行为契约可满足且无冲突；重点为 Round 11 的 F46 修正复核（a）、同一正则的穷举收敛独立复现（b）与逐包 Coverage 归属独立复算（c），并回归 Round 9/10/11 的连续性项。"
changes: "只读检视，未修改任何规划文件；仅新增本报告。全部机械判据以内联 node --input-type=module -e 复现，不落盘临时脚本；未执行任何 Project Verify / 构建 / 测试 / E2E。"
issues: "F46（Round 11，MINOR）本轮复核为已解决，且同类机制已按同一正则穷举收敛、无遗漏实例。本轮新发现 0 项（F47 起无编号）。Assessment 记录 2 项非阻断观察（叙述 Coverage 的 Responsible Units 与结构化块归属存在 6 行不一致；`EV1` 无定义），均不影响任何机械门禁，亦非本轮修正引入。"
result: PASS
evidence_paths: NOT_APPLICABLE
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘脚本）"

checks:
  - id: "planningDigest / contractDigest / requirementsDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "退出码 1。planningDigest=plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1（与调度者给定 Target Revision 逐字一致）；contractDigest=sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5、requirementsDigest=sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752（均与给定值一致）。errors 恰为一条，即『DDR Plan Revision 未绑定当前规划契约摘要』。无 ownership/waves/freeze/coverage 类错误。"
  - id: "(a) F46 复核：4.4 的 taskPackages"
    command: "node --input-type=module -e（复刻 planning-model.mjs:37-40 的 (^|[^\\w-])<ID>(?![\\w-]) 逐任务行取包）"
    result: "4.4 -> [\"TP1\"]（Round 11 为 [\"WP1\",\"WP2\",\"TP1\"]）。4.5 -> [\"TP1\"]、4.6 -> [\"TP1\"] 亦只映射 TP1；4.4 的依赖/并行说明已下移到无复选框的缩进子条目，不匹配 /^\\s*[-*]\\s*\\[[ xX]\\]\\s+(\\d+\\.\\d+)\\s+(.+)$/，故不再被 taskPackages 采样。"
  - id: "(b) 同一正则的穷举复现（全部任务行 + Coverage 实际引用交叉）"
    command: "node --input-type=module -e（遍历 tasks.md 全部 - [ ] N.M 行共 129 行，并按 agentic-coverage 块实际引用的任务号交叉）"
    result: "129 条任务行全部读出。Coverage 块实际引用 18 个任务号：2.1→[WP1]、2.2→[WP2]、2.3→[WP3]、2.4→[WP4]、2.5→[WP5a]、2.6→[WP5b]、2.7→[WP6]、2.8→[WP7]、4.1→[]、4.2→[]、4.3→[]、4.4→[TP1]、4.5→[TP1]、4.8→[TP2]、4.10→[TP3]、4.11→[TP3]、4.13→[TP4]、4.14→[TP4]。无一行映射到多于一个包；仅 4.1/4.2/4.3（[AC1]/[AC2]/[AC3] 替代检查，属 main 执行项）不映射任何包，故 R001–R004、R018–R021、R030–R041、R045–R049、R098–R102 这些行只经 2.x 任务归属，未被 AC 任务误引。结论：被 Coverage 引用的任务中，没有任何一行携带上游包 ID，无遗漏实例。"
  - id: "(c) 逐包 Coverage 行数独立复算"
    command: "node --input-type=module -e（102 行 coverage 的逐包归属；与 planningModel().packages[id].coverage 交叉核对）"
    result: "WP1 17、WP2 29、WP3 29、WP4 19、WP5a 12、WP5b 10、WP6 31、WP7 22、TP1 46、TP2 34、TP3 37、TP4 14（总 102 行），与主 Agent 复算逐包相等，且与 planningModel 的 coverage 长度一致。行段明细：WP1=R1–R4；WP2/WP3=R5–R9 的 24 行 + workspace-resolution『路径不泄漏』5 行；WP4=R5 的 4 行 + R8 的 6 行 + local-agent-host 9 行；WP5a=R10 3 + R15 5 + R19 4；WP5b=R11 5 + R20 5；WP6=R2/R3 10 + R12–R14 12 + R19/R20 9；WP7=R15 5 + R16 5 + R17 4 + R18 4 + R19 4；TP1=R1–R9 41 + workspace-resolution 5；TP2=R6–R9 20 + workspace-resolution 5 + local-agent-host 9；TP3=R10 3 + R11 5 + R12–R14 12 + R17 4 + R18 4 + R19 4 + R20 5；TP4=R15 5 + R16 5 + R18 4。"
  - id: "planningModel 逐包 sources 与声明区间核对（F46 的语义后果）"
    command: "node --input-type=module -e（import planningModel，读 sources 与 deps）"
    result: "WP1 sources 现仅 [specs/sync-snapshot-scope/spec.md]（Round 11 额外含 core-derived-events 与 workspace-resolution）；WP2 sources=[specs/core-derived-events/spec.md, specs/workspace-resolution/spec.md]（R7 跨两份 spec，符合声明 R5–R9）；TP1 sources 含三份 spec（符合其 R1–R9 声明）。12 包 sources 与各自 Goal 列的区间逐包自洽。"
  - id: "叙述 Coverage 22 行 Closure Unit"
    command: "node --input-type=module -e（逐行取 Responsible Units，按 Merge Strategy 的单元→包映射取 Order 最大者比对 R1–R22）"
    result: "22/22 行 Closure Unit = 该行 Responsible Units 中 Order 最大的交付单元，0 处不等（R1/R4→MU1b、R5–R9/R21/R22→MU2、R19→MU3d、R2/R3/R10–R18/R20→MU3e）。"
  - id: "结构化 Coverage 102 行 vs 5 份 delta spec 标题"
    command: "node --input-type=module -e（解析 agentic-coverage 与 5 份 spec 的 ### Requirement / #### Scenario 标题集）"
    result: "102 行 / 102 标题：heading 精确匹配缺失 0、requirement 归属错配 0（按最近的前置 ### 标题复算）、spec 标题未被引用 0、重复 id 0。spec 实测计数 sync-snapshot-scope 4/13、core-derived-events 5/19、pwa-web-client 11/36、workspace-resolution 1/4、local-agent-host 2/7，与 plan.md:5 逐字一致。"
  - id: "Execution Waves 与 hardDeps 重算 earliest"
    command: "node --input-type=module -e（contract 依赖按冻结路径存在性判定；code 与未冻结 contract 边计入硬依赖）"
    result: "12 行逐行相等（WP1 1、WP2 1、WP3 2、WP4 2、WP5a 2、WP5b 3、WP6 4、WP7 5、TP1 2、TP2 3、TP3 6、TP4 6），无环、无越级。注：12 包 Contract Freeze 单元格因多路径/反引号/glob 均不满足 ^(\\S+?)@(\\S+)$，故 contract 边全部按硬依赖计入，与 Round 9/10/11 同口径。"
  - id: "Contract Freeze ⊆ Dependencies"
    command: "node --input-type=module -e（提取单元格内 <path>@<WP> 引用并与该包 code:/contract: 目标比对）"
    result: "越界数 0。17 条 @ 引用（含 glob 形态）的目标全部落在依赖列内。路径可读性：12 条为可逐个 fs.access 的实体文件（schemas/sync/v1/{event-views,command}.schema.json、fixtures/sync/v1/{manifest.json,transcripts/device-proof.json}、crates/sync-protocol/src/{sync,command,views}.rs、crates/core/src/ports.rs），5 条为 glob（schemas/sync/v1/**、fixtures/sync/v1/transcripts/**、clients/app/src/{protocol,platform,sync-client,state,features,components}/**）——其父目录在规划阶段尚未创建（clients/app 未落地），属 apply 交付物，与 F13 的既有非阻断判定一致。"
  - id: "Write Scope 重叠与 Shared File Ownership 登记"
    command: "node --input-type=module -e（按 scopePaths/scopeOverlap 语义 + 容忍 {} 内逗号与中文括注的分词复算，再与登记行比对）"
    result: "语义重叠 6 组（WP1×WP2、WP3×WP4、WP5a×WP7、TP1×WP1、TP1×WP2、TP2×WP3），全部有 Shared File Ownership 登记行；按检查器原样语义（不剥离反引号、不去中文括注）为 1 组（WP1×WP2），同样已登记。18 行登记表四列齐备；单写者行的 Merge Order 写 `—` 且 Writers 为其自身，检查器只对多写者行校验合入顺序，不构成问题。"
  - id: "Dependency Handoffs 与依赖列一致性"
    command: "node --input-type=module -e（10 行 Upstream 集合 vs 该包 code:/contract: 目标集合）"
    result: "10/10 行集合相等（WP3=[WP1,WP2]、WP4=[WP2]、WP5a=[WP1,WP2]、WP5b=[WP1,WP5a]、WP6=[WP1,WP5a,WP5b]、WP7=[WP1,WP2,WP5a,WP5b,WP6]、TP1=[WP1,WP2]、TP2=[WP2,WP3]、TP3=[WP1,WP5a,WP5b,WP6,WP7]、TP4=[WP7]）。"
  - id: "交付单元解析、Reviewer 独立性与 §6 重编号"
    command: "node --input-type=module -e（Merge Strategy 单元→包映射；tasks.md §6 的 6.x 序列）"
    result: "8 单元全部 independent，并集恰为 12 个已声明 ID（WP1..WP7、WP5a/WP5b、TP1..TP4），无多无缺；12 包 Owner 与 Reviewer 无一相同、Reviewer 无空值。6.1–6.63 连续无缺号无重号；单元分组 MU1a 8、MU1b 7、MU2/MU3a–MU3e 各 8；交叉引用 6.1→『6.6』、7.1→『6.58』均指向真实存在的项。"
  - id: "F46 的修正文本自洽性"
    command: "node --input-type=module -e + 逐行读取 tasks.md:63-79"
    result: "tasks.md:65 派发行保留 [wp:TP1] [PV1]；依赖与并行说明位于 tasks.md:66/74/79 的缩进子条目。子条目文本内虽出现 WP1/WP2/WP5a/WP5b/WP6/WP7 等包 ID，但无复选框前缀，不被 taskPackages 采样（实证见 (a)(b)）。同一体例已应用于 4.10（tasks.md:72-75）与 4.13（tasks.md:77-79）。"
  - id: "行为契约可满足性（proposal/specs/design 对同一条件的结果一致性）"
    command: "node --input-type=module -e + grep 于 specs/design/proposal 的状态与字段口径"
    result: "连接状态：specs/pwa-web-client/spec.md『连接状态是互斥状态机』枚举 8 常规态 + 4 阻断态 = 12 态，与 plan R12、tasks 2.7、4.10 的『12 态（8 常规态 + 4 阻断态）』一致。分页：sync-snapshot-scope 的 before/limit/hasEarlier 口径与 Contract Changes 表、WP1/TP1/TP3 一致。路径越界：core-derived-events『工作区外的路径被显式标记』与 workspace-resolution MODIFIED『路径不泄漏』无矛盾。快照收窄：D1 三资源与 R1/WP1/TP1 一致。未发现同一条件在 proposal/specs/design 之间给出互相矛盾的结果或不可能场景。"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-12"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；不拥有任何工作包）"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 12
    stage: plan
    target_revision: "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r12.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / proposal.md / design.md / 5 份 delta spec 做只读复审。planningDigest 由 workflow check --stage plan --json 重新推导为 4e2fb347…88e1，与调度者给定值逐字一致。Round 11 的唯一发现 F46 独立复核为已解决（4.4 -> [TP1]），并按规定正则穷举复现同类收敛、无遗漏实例；逐包 Coverage 行数与主 Agent 复算逐包相等。未发现 CRITICAL/MAJOR。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–11 的 Review ID） |
| Round | 12 |
| Review Type | plan |
| Review Stage | 计划/依赖声明阶段（Round 11 判 PASS 后主 Agent 修正其唯一发现 F46 并用穷举扫描收敛同类缺陷；本轮为绑定当前摘要的最终复核 + F46 的 recheck + (a)(b)(c) 独立复现） |
| Reviewer | `reviewer-plan-ddr-12`（不拥有任何 WP/TP；owners 为 `coding-1..7`、`coding-5b`、`testing-1..4`，独立性成立） |
| Work Package | NOT_APPLICABLE（范围是全部 12 包 + 8 个交付单元的声明） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1`（本轮第 1 次执行 `workflow check --stage plan --json` 即得此值，与调度者给定值一致；随后未修改任何规划文件） |
| Requirements | `proposal.md`、`design.md`（D1–D8）、`specs/{sync-snapshot-scope,core-derived-events,pwa-web-client,workspace-resolution,local-agent-host}/spec.md`、`plan.md`、`tasks.md`、`verification.md`（同目录当前字节） |
| Project Rules | `AGENTS.md`；`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`procedures/scheduling.md` |
| Verification Evidence | 本轮不执行任何 Project Verify。`verification.md` 的 `## Checks` 中 PV2/PV3/IV1/AC1–AC3 为 NOT_APPLICABLE（apply 阶段执行）；MU1a 候选一行记 PASS（证据 `reports/PV1.log`，`reports/PV1-main-mu1a.log` 不存在） |
| Check Plan | `plan.md` 的 `### Project Verify`（PV1/PV2/PV3/IV1/AC1–AC3）；`verification.md` 的 `## Checks` 与 `## Check Plan Changes` |
| Previous Findings | `-r8.md`（FAIL，F25–F34、F13、F14、F22、F23、F24 共 15 项）、`-r9.md`（PASS，F35–F39 + 结转 F13）、`-r10.md`（PASS，F41–F45）、`-r11.md`（PASS，F46）、`verification.md` 的 `## Review Findings` |
| 实际检查范围 | (a) F46：`tasks.md:65-66` 的拆分与 `taskPackages` 实跑、逐包 Coverage 与 sources；(b) 同一正则穷举：129 条任务行 + Coverage 引用的 18 个任务号交叉；(c) 逐包 Coverage 行数与 Work Packages 各行声明区间核对；连续性项：叙述 Coverage 22 行 Closure Unit、§6 的 6.1–6.63 与交叉引用、结构化 Coverage 102 行对 5 份 spec 标题（含 requirement 归属）、12 行 Wave 与 hardDeps earliest、Contract Freeze ⊆ Dependencies 与路径可读性、字面/语义写重叠登记、8 单元并集、Owner≠Reviewer、10 行 handoff、行为契约一致性 |
| 未验证内容 | 未执行任何构建/测试/E2E；未评估用例覆盖充分性（归 validator 与 main 的 Coverage Index）；未判断 MU3a–MU3e 的候选与 receipt 能否实际产出（apply 阶段 merger 职责）；`clients/app/**` 的 glob 冻结路径在规划阶段尚无实体（apply 交付物） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r12.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行，规划阶段） |

## Findings

### 一、F46 复核（Round 11 的唯一发现）

| ID | Severity | Location | 复核结论 | 复核依据 |
| --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F46（`tasks.md:65`，4.4 的派发行携带上游包 ID `WP1`/`WP2`，使 WP1 的 Coverage 由语义正确的 17 行涨到 46 行、WP2 由 29 行涨到 46 行，sources 被对方 spec 污染） | MINOR | `tasks.md:65-66`（判据机制见 `node_modules/@dongfanglin/openspec-agentic/src/planning-model.mjs:37-40`） | **已解决** | 4.4 现为派发行（`tasks.md:65`，保留 `[wp:TP1] [PV1]`）+ 缩进子条目（`tasks.md:66` 承载并行/冻结说明）。实跑 taskPackages：`4.4 -> ["TP1"]`（Round 11 为 `["WP1","WP2","TP1"]`）。逐包 Coverage 回到语义正确值：**WP1 17**（R001–R017，仅 `specs/sync-snapshot-scope/spec.md`）、**WP2 29**（R018–R041 + R089–R093）；`planningModel().packages` 的 sources 与之一致，WP1 不再包含 `core-derived-events` 与 `workspace-resolution`。 |

### 二、(a)(b)(c) 三项独立结论

**(a) 4.4 的 taskPackages 实跑**：`4.4 -> ["TP1"]`。同一任务号在 Coverage 块中的 46 行引用（R001–R041、R089–R093）不再进入 WP1/WP2 的 `coverage`，WP1 由此回到 17 行、WP2 回到 29 行。修正体例与 4.10（`tasks.md:72-75`）、4.13（`tasks.md:77-79`）一致，且子条目无复选框前缀，故不被 `taskPackages` 采样。

**(b) 同一正则的穷举复现**：遍历 `tasks.md` 全部 129 条 `- [ ] N.M` 行，并与 `agentic-coverage` 块实际引用的 18 个任务号交叉。18 个被引用任务号的映射为：

```
2.1->[WP1]  2.2->[WP2]  2.3->[WP3]  2.4->[WP4]  2.5->[WP5a] 2.6->[WP5b]
2.7->[WP6]  2.8->[WP7]  4.1->[]     4.2->[]     4.3->[]     4.4->[TP1]
4.5->[TP1]  4.8->[TP2]  4.10->[TP3] 4.11->[TP3] 4.13->[TP4] 4.14->[TP4]
```

结论：被 Coverage 引用的任务行**没有任何一行携带上游包 ID**，无遗漏实例。未被引用的多映射行（`1.2/1.3/1.5->[WP1,WP2]`、`1.4`、`3.8->[WP3,WP4]`、`4.7->[WP3,TP2]`、`5.2/5.3/5.6/5.7/5.9`、`6.2/6.6/6.17/6.57`）均为「调度/集成/合入」类任务，其提到上游包 ID 是自述范围（如 5.3 声明 MU2 组成、6.6 声明 MU1a 的 WP1+WP2），不参与 Coverage 归属，故不构成 F35/F41/F46 同类残留；其中 `4.7->[WP3,TP2]` 的 `code:WP3` 是其真实依赖声明，且 4.7 未被任何 Coverage 行引用。

**(c) 逐包 Coverage 归属复算**（实跑输出，与主 Agent 复算逐包相等）：

```
WP1=17  WP2=29  WP3=29  WP4=19  WP5a=12  WP5b=10
WP6=31  WP7=22  TP1=46  TP2=34  TP3=37   TP4=14   （合计 102）
```

与 Work Packages 各行声明区间的核对结果：WP1=R1–R4 ✓（17 行恰为 R1–R4）；WP2/WP3=R5–R9 ✓（24 行 + workspace-resolution 的 R7 5 行）；WP5a=R10/R15/R19 ✓（3+5+4=12）；WP5b=R11/R20 ✓（5+5=10）；WP6=R2/R3/R12–R14/R19/R20 ✓（10+12+9=31）；WP7=R15–R19 ✓（5+5+4+4+4=22）；TP1=R1–R9 ✓（41 行 + workspace-resolution 5 行）；TP2=R5–R9 的行为判定 ✓（20+5+9=34）；TP3=R11–R14、R17–R20 ✓ 的主要部分（37 行 = 3+5+12+4+4+4+5，其中额外含 R10 的 3 行，见 Assessment 非阻断观察 1）；TP4=R15/R16/R18 ✓（5+5+4=14）。**没有一行 Coverage 的归属与上述行数不符**。

## Assessment

### 本轮结论

**PASS**。当前固定摘要 `plan-v2:sha256:4e2fb347…88e1` 下：依赖类型声明属实，`contract:` 的冻结引用逐条落在依赖列内（越界数 0），12 行波次与 hardDeps 重算的 earliest 逐行相等且无环，Write Scope 的 6 组语义重叠（检查器原样语义下 1 组）全部有登记行，8 个交付单元解析为 independent 且并集恰为 12 个已声明 ID，Owner/Reviewer 无自审，叙述 Coverage 22 行 Closure Unit 与结构化 Coverage 102 行对 5 份 delta spec 的 102 个标题（含 requirement 归属）全部命中，tasks.md §6 的 6.1–6.63 与交叉引用一致，10 行 handoff 的 Upstream 与依赖目标 10/10 相等。无已确认的 CRITICAL/MAJOR。

### F46 复核结论

**已解决**。`tasks.md:65` 的派发行不再承载上游包 ID：`4.4 -> ["TP1"]`，WP1 由 46 行回到语义正确的 17 行、WP2 由 46 行回到 29 行，sources 不再把对方 spec 计为自身关联需求。同类机制经穷举复现确认已收敛至被 Coverage 引用的全部任务行，无遗漏实例。

### (a)–(c) 逐项独立结论

| 项 | 结论 |
| --- | --- |
| (a) 4.4 的 taskPackages 为 [TP1]，WP1/WP2 行数回到合理值 | **通过**：`4.4->["TP1"]`；WP1 17、WP2 29，与语义正确值逐包相等 |
| (b) 同一正则穷举、被 Coverage 引用的任务中无任何一行携带上游包 ID | **通过**：129 行全部读出，18 个被引用任务号无一行多映射或含上游包 ID；未引用行的多映射均属调度/集成语境，不参与 Coverage |
| (c) 逐包 Coverage 行数与 Work Packages 声明区间 | **通过**：12 包行数 17/29/29/19/12/10/31/22/46/34/37/14，合计 102，与主 Agent 复算逐包相等；各包行段落在声明区间内（WP4/TP3 的 Goal 列区间偏窄，属语义表述，见下） |

### 非阻断项与观察（不影响任何机械门禁）

1. **叙述 Coverage 的 `Responsible Units` 列与结构化块的 tasks 推导存在 6 行不一致，且 WP4/TP3 的 Goal 列区间窄于其实际 Coverage。** 具体为：`plan.md:34`（R6）、`:35`（R7）、`:36`（R8）、`:37`（R9）的 Responsible Units 未列 `TP2`，而结构化块中这些行的 `tasks` 含 `4.8`（→TP2）；`plan.md:39`（R11）列了 `TP4`，而 TP3/TP4 的 tasks 不含 `4.13/4.14`，R11 实际只由 WP5b+TP3 承担；`plan.md:47`（R19）未列 `TP3`，而结构化块 R080–R083 的 `tasks` 含 `4.10/4.11`（→TP3）。与之同源，`plan.md:862`（WP4）Goal 列写 `R8/R21/R22` 但其 Coverage 含 R5 的 4 行（2.4 交付 Diff 元素）、`plan.md:869`（TP3）Goal 列写 `R11–R14、R17–R20` 但其 Coverage 含 R10 的 3 行。三点均**不改变任何行的 Closure Unit**（各行 Closure Unit 与合并单元归属仍 22/22 正确），也**不被任何工具消费**——`planning-model.mjs` 只读取 `## Work Packages`、`agentic-coverage` 结构化块、Execution Waves、Shared File Ownership、Dependency Handoffs、Runtime Resources、Merge Strategy、Target/Scope/preable 等节，叙述 Coverage 表不在其中，不进 `globalDigest`/`planningHash`。按本轮粒度提示，登记为 MINOR 级观察而不单列 Finding；**建议处置**：由 main 记入 verification 的已知项，或在下一轮（若开）把 R6–R9/R19 的 Responsible Units 补 `TP2`/`TP3`、R11 去掉 `TP4`，并把 WP4 Goal 列改为 `R5/R8/R21/R22`、TP3 改为 `R10–R14、R17–R20`。
2. **`EV1` 无定义**：叙述 Coverage 的 `Final Checks` 列在 19 行出现 `EV1`（`plan.md:29-48`），但 `## Project Verify` 表只有 PV1/PV2/PV3/IV1/AC1–AC3，结构化块的 `checks` 亦仅取这 7 个 ID，`tasks.md`/`verification.md` 中无 `EV1`。该标签不参与任何机械判据。Round 7 已记录同一观察，本轮事实未变，非本轮修正引入，不单列 Finding。
3. **`plan.md:5` 的 `Skip Specs: no` 与 local-agent-host 的 2 个 ADDED 需求**：delta `specs/local-agent-host/spec.md` 的 2 个 ADDED 需求（文件改动的转发现源限于类型化 Diff、profile 进程的连接生命周期可被观察）在结构化块中由 R094–R102 覆盖，但叙述 Coverage（声明「每个增量 Requirement 一行」）未为其编号，反而为既有未改动的 R21/R22 各设一行。结构化块与门禁不受影响，属叙述表与声明粒度的偏差，记录为观察。

### 证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` 的 `## Checks` 三行为 NOT_APPLICABLE；PV3 的目标工程 `clients/app` 在规划阶段无实现 | 否 |
| AC1 / AC2 / AC3 | 待补（apply 阶段） | 三行 NOT_APPLICABLE；三条替代检查已在 tasks 4.1–4.3 以 `[AC1]`/`[AC2]`/`[AC3]` 声明 | 否 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE | 否 |
| MU1a 候选 PASS 行 | 规划外记录 | 有一行 MU1a 候选 683dbbb8 记 PASS（证据 `reports/PV1.log`）；`reports/PV1-main-mu1a.log` 不存在，属主分支回归待补证据 | 否 |
| `workflow check --stage plan` 的单条 error | 预期内的登记待办 | 恰为「DDR Plan Revision 未绑定当前摘要」；本轮报告给出裸词结论，由 main 登记 r12 行后消除 | 否 |
| Contract Freeze 机械可读性（F13） | 维持非阻断 | 12 包单元格仍不匹配 `^(\S+?)@(\S+)$`；17 条 `@` 引用目标全部在依赖列内、路径语义可核对。Round 9 已独立判定为非阻断且明确不升级，本轮未发现事实前提变化，按指示不重开 | 否 |
| 变更目录的版本控制 diff | 不可用 | `git status --porcelain` 显示该目录未跟踪，无法逐行 diff；以「按当前字节重跑全部机械判据 + 逐项核实 F46 与 (a)(b)(c)」替代 | 否 |

### 不确定性与后续建议

- 本轮不宣称任何实现、测试、E2E 或最终验收通过；`## Checks` 的 PV1/PV2/PV3/IV1/AC1–AC3 仍待 apply 阶段补齐。
- F46 已闭环，无需再开修正轮。Observation 1/2/3 均为语义层面、不阻断 apply 派发（无具体文件与行号导致的执行歧义：4.4 的依赖已下移，WP4/TP3 的依赖列与 Coverage 归属一致），建议由 main 记入 verification 的已知项即可。

## Dependency Declaration Review

PASS
