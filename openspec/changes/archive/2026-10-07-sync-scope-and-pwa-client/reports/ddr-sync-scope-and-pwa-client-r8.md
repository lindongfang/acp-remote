<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 8。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-8"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7、coding-5b、testing-1..testing-4）"
target_revision: "plan-v2:sha256:b2363f45d2d79263566e707a87668649384c045c4e5572dce1b3feb9769c41d3"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用 ⊆ Dependencies、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 独立检视任务、Coverage Index（含 agentic-coverage 结构化块）与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order、Runtime Resources 六列完整性。本轮重点：(1) 变更一（WP5 拆为 WP5a/WP5b、MU3 拆为 MU3a–MU3e）是否在 plan/tasks/verification 三处收尾干净；(2) 变更二（新增前端静态服务器运行时资源行）六列是否齐备、消费者是否与改名后的包一致；(3) tasks.md 是否属于 planningDigest（对附带观察的确认或反驳）。"
changes: "只读检视；未修改任何规划文件、代码、任务状态或 verification.md。仅新增本报告文件。全部判据以内联 node -e / node --input-type=module -e 复现，不落盘临时脚本；未执行任何构建、测试或 E2E。"
checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "退出码 1；planningDigest=plan-v2:sha256:b2363f45d2d79263566e707a87668649384c045c4e5572dce1b3feb9769c41d3；contractDigest=sha256:55043114f021a33691b606243f9c27595b6b879b629aa75f28089e02af6699db；requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（与 Round 4–7 相同）。errors 共 6 条：5 条「缺少执行者接收确认」+ 1 条 DDR Plan Revision 未绑定当前摘要"
    note: "与 Round 7 的 8eb6a3eb…dd4ad 不同，两处变更已刷新摘要。变更目录未被 Git 跟踪，无法逐行 diff，本轮以「按当前字节逐条复现全部机械判据 + 逐项核实本轮声明的两处变更」替代 diff。"
  - id: "交付单元解析与 Order（实跑 deliveryUnits）"
    scope: "plan.md:967-976 的 Merge Strategy 全节"
    evidence: "以 node 直接调用 @dongfanglin/openspec-agentic/src/scheduling-evidence.mjs 的 deliveryUnits(plan)：返回 8 个单元 MU1a/MU1b/MU2/MU3a/MU3b/MU3c/MU3d/MU3e，mode 全部 independent，members 依次 {WP1,WP2}/{TP1}/{WP3,WP4,TP2}/{WP5a}/{WP5b}/{WP6}/{WP7}/{TP3,TP4}，order='1'..'8'、orderGroup 1..8 与表序 position 0..7 严格同向，无抛错；并集恰为 12 个已声明 ID（WP1、WP2、WP3、WP4、WP5a、WP5b、WP6、WP7、TP1、TP2、TP3、TP4），无重复归属、无遗漏"
  - id: "Execution Waves 与依赖列一致性（按 workflow-check.mjs:205-221 的 hardDeps 口径复算 earliest）"
    scope: "plan.md:889-905 的 12 行 vs Work Packages（plan.md:858-871）依赖列"
    evidence: "复算 earliest：WP1=1、WP2=1、WP3=2、WP4=2、WP5a=2、WP5b=3、TP2=3、WP6=4、WP7=5、TP3=6、TP4=6、TP1=2；声明波次逐行相等（W1/W1/W2/W2/W2/W3/W3/W4/W5/W6/W6/W2）。上游同批或更早批检查 12/12 通过；依赖图 DFS 无环；12 行 Serialization Reason 全为 NOT_APPLICABLE 且无 waveIndex>minimum 的行。池容量：W1=2 coder、W2=3 coder+1 tester、W3=1 coder+1 tester、W4/W5=1 coder、W6=2 tester，对 openspec/agentic.yaml 的 coding=3 / testing=2 均未超容"
  - id: "Contract Freeze @ 引用 ⊆ 声明包 Dependencies 列（12 包逐单元格）"
    scope: "plan.md:858-871 的 Contract Freeze 列 vs 同表 Dependencies 列"
    evidence: "剥离反引号后逐条比对：WP3 的 @WP2/@WP1、WP4 的 @WP2、WP5a 的 @WP1×2、WP6 的 @WP1、WP7 的 @WP2、TP1 的 @WP1×3/@WP2、TP2 的 @WP3/@WP2、TP3 的 @WP1×2/@WP5a/@WP5b/@WP6×2/@WP7×2 全部落在依赖列内；**WP5b 的 fixtures/sync/v1/transcripts/device-proof.json@WP1 不在其 Dependencies（仅 code:WP5a）内 —— 唯一越界项，见 F27**；WP1/WP2/TP4 为 none + NOT_APPLICABLE，无引用"
  - id: "WP5a/WP5b 写入范围与 Shared File Ownership（按 workflow-check.mjs:114-117 的 scopePaths/scopeOverlap 语义 + 人工字面展开）"
    scope: "plan.md:864-865 的 Write Scope 列 vs plan.md:909-929 的 17 行登记"
    evidence: "WP5a 字面范围 clients/app/package.json、clients/app/app/（空壳路由）、clients/app/src/{domain,protocol,components}；WP5b 为 clients/app/src/platform。工具口径下二者无重叠（brace 表达式不做展开）；人工展开后 WP5a 的 src/components/ 与 WP7 的 clients/app/src/components/ 重叠、WP5a 的 app/ 与 WP7 的 clients/app/app/ 重叠，均跨波次且由 WP7 的 code:WP5a/code:WP6 排序，不构成同批冲突。**但 tasks.md:27 仍把 src/platform/secure-storage.web.ts 指派给 WP5a（2.5），与 plan.md:864/865 直接冲突且无登记行，见 F25**"
  - id: "crates/sync-protocol 仍不可从前端包到达"
    scope: "WP5a/WP5b/WP6/WP7/TP3/TP4 的 Dependencies、Contract Freeze 与 Write Scope"
    evidence: "五个前端包与 TP3 的依赖目标集合为 {WP1, WP2, WP5a, WP5b, WP6, WP7}，无任何 crate 目标；Write Scope 全部在 clients/app/ 内。WP5b 的 Contract Freeze 单元格对 crates/sync-protocol/src/… 的唯一提及带显式否定说明「不 import，仅按 transcript 规范实现」，且因多路径不匹配 ^(\\S+?)@(\\S+)$（planning-model.mjs:59 的 freeze 正则）不会成为冻结依赖。前端仍只把 schema/fixture/transcript 当数据读取"
  - id: "code/contract 依赖逐条属实性（对照仓库事实，Round 7 结论复核）"
    scope: "14 条 code: 边（WP5b→WP5a、WP6→WP5a/WP5b、WP7→WP6/WP5a/WP5b、TP1→WP1/WP2、TP2→WP3、TP3→WP5a/WP5b/WP6/WP7、TP4→WP7）与 10 条 contract 边的逐条属实性"
    evidence: "crates/core/Cargo.toml 的 [dependencies] 仍只有 async-trait/thiserror/p256/sha2；crates/agent-host/Cargo.toml 无 sync-protocol；scripts/check-crate-boundaries.mjs 的 CORE_FORBIDDEN 含 sync-protocol；docs/MODULE_ARCHITECTURE.md §5 矩阵 core 行整行空白、agent-host 行仅 core/acp-protocol。故 WP3/WP4 的 contract 降级与 WP5a/WP5b 的 contract 声明均非虚假声明"
  - id: "reviewer 独立性与独立检视任务"
    scope: "12 个包的 Owner/Reviewer/Role 列 + tasks.md §3/§4 的检视任务"
    evidence: "12 个包 Owner 与 Reviewer 无一相同（WP5a=coding-5/review-5、WP5b=coding-5b/review-5b，其余同前）；按 workflow-check.mjs:159-169 的口径复算 12/12 至少一条引用它、不含 [wp:…] 派发标签、含检视标记的任务（WP5a→tasks.md:44 的 3.10、WP5b→tasks.md:46 的 3.12）；按 planning-model.mjs:37-40 的 taskPackages 口径，12 个包各有且仅有一条 [wp:…] 派发任务（2.1–2.5、2.6、2.7、2.8、4.4、4.7、4.11、4.14）"
  - id: "Coverage Index 结构化块的任务映射（重编号回归检查）"
    scope: "plan.md 的 agentic-coverage 块 102 行的 tasks 字段 vs tasks.md 当前任务编号"
    evidence: "统计得任务引用分布：2.1×17、2.2×29、2.3×29、2.4×19、2.5×22、2.6×31、2.7×22、2.3/4.x 同上；**2.8（WP7 的新派发任务）被 0 行引用**。引用 2.7 的 22 行恰为 R062–R083（目录页、会话创建、上下文三态、降级卡片、Agent 目录两层 = plan.md:867 WP7 的 R15–R19 全集）；引用 2.6 的 31 行恰为 R005–R014 + R050–R061 + R080–R083 + R084–R088 = plan.md:866 WP6 的 R2/R3/R12–R14/R19/R20 全集。拆分前 2.6=WP6、2.7=WP7，重编号后语义整体错位一格，见 F28"
  - id: "tasks.md 是否参与 planningDigest（附带观察的机制核实）"
    scope: "node_modules/@dongfanglin/openspec-agentic/src/planning-model.mjs 的 planningModel()"
    evidence: "planningDigest = digest({ globalDigest, packages: {WP→planningHash} })；globalDigest 读 proposal.md、design.md、.openspec.yaml 与 plan.md 的若干节，不含 tasks.md；planningHash 含 declaration/wave/sources/coverage/frozenContract，其中 tasks.md 只以 taskPackages（由每个 `- [ ] N.M` 行文本推导该任务提及哪些 WP ID）间接影响 coverage 行到包的归属。verificationHash 确实收录任务正文全文，但 verificationHash 不参与 planningDigest。故「tasks.md 的正文改动不改变 planningDigest」这一观察成立且有明确机制（见 Assessment 的附带确认）"
  - id: "Round 7 的 F13/F14/F22/F23/F24 结转复核"
    scope: "各轮 MINOR 项在当前字节的状态"
    evidence: "F21 已关闭（tasks.md:92 的 6.5 现为 MU1a）；F13 未解决（多路径 Contract Freeze 单元格仍不匹配 ^(\\S+?)@(\\S+)$）；F14 未解决且范围扩大（plan.md:940 的 WP7 行 Upstream 仍为 WP5, WP6, WP2，漏 WP1，且 WP5 已不存在，WP5a/WP5b 均未登记）；F22 未解决（R1–R4 Closure Unit 仍为 MU1/premerge，R10–R20 仍为 MU3/premerge|MU3/final）；F23 未解决（tasks.md:82 的 5.3 仍写「上游 WP1/WP2 已合入」，plan.md:971 要求 MU1b 已合入）；F24 未解决（tasks.md:96-100 的 MU1b 组仍为 5 项，主分支回归并入 6.13 的 merger）"
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘任何脚本）"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-8"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 8
    stage: plan
    target_revision: "plan-v2:sha256:b2363f45d2d79263566e707a87668649384c045c4e5572dce1b3feb9769c41d3"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r8.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md 做只读复核，逐项核实本轮两处声明的变更（WP5→WP5a/WP5b 与 MU3→MU3a–MU3e；新增前端静态服务器运行时资源行），并全量复算无环、earliest 波次、Contract Freeze ⊆ Dependencies、字面写入重叠与 Shared File Ownership、交付单元解析与 Order、reviewer 独立性、Coverage Index 引用有效性、tasks.md 对 planningDigest 的参与方式。planningDigest 由 workflow check --stage plan --json 重新推导为 b2363f45…c41d3。判定 FAIL 的依据是 5 项已确认的 MAJOR：F25（tasks.md:27 仍把 src/platform/secure-storage.web.ts 指派给 WP5a，与 plan.md:865 的 WP5b 归属冲突且无登记行）、F26（tasks.md:26 的 WP5b 描述丢失六处行内标识，tasks.md:3 的 2.6 游离于 §2 之外且无范围/依赖/完成条件）、F27（plan.md:865 的 WP5b Contract Freeze 引用 @WP1 但 Dependencies 无 WP1）、F28（agentic-coverage 块 102 行中 53 行的任务引用未随重编号，2.8 被 0 行引用）、F29（tasks.md:84/110-117 仍按单一 MU3 单元组装候选并复用同一合入 worktree，与 plan.md:967-976 的 8 个独立单元直接矛盾，且 MU3b–MU3e 无任何合入任务）。其余 8 项为 MINOR，不阻断但建议同轮修正。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–7 的 Review ID） |
| Round | 8 |
| Review Type | plan |
| Review Stage | 计划/依赖声明（`## Dependency Declaration Review` 门禁） |
| Reviewer | `reviewer-plan-ddr-8`（不拥有任何 WP/TP；owners 为 `coding-1..7`、`coding-5b`、`testing-1..4`，独立性成立） |
| Work Package | 全部 12 个包（WP1、WP2、WP3、WP4、WP5a、WP5b、WP6、WP7、TP1–TP4）的依赖、写入归属、交付单元成员与运行时资源声明 |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:b2363f45d2d79263566e707a87668649384c045c4e5572dce1b3feb9769c41d3` |
| 版本稳定性 | 本轮未发生目标版本变动。`workflow check --stage plan --json` 本轮执行 1 次，`planningDigest` 为 `b2363f45…c41d3`，与 Round 7 的 `8eb6a3eb…dd4ad` 不同（两处变更已刷新摘要）。变更目录未被 Git 跟踪，**无法用版本控制逐行 diff 出 Round 7 之后的改动**，本轮以「按当前字节重新复现全部机械判据 + 逐项核实本轮声明的两处变更」替代 diff |
| Requirements | `specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D1–D8）、`plan.md`、`tasks.md`、`verification.md`（同目录当前版本） |
| Project Rules | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`templates/tasks.md`、`procedures/acceptance.md` 的依赖类型与 Execution Waves 判据、`openspec/agentic.yaml`、`docs/MODULE_ARCHITECTURE.md` §5 |
| Verification Evidence | 本轮为规划阶段，无测试执行证据；PV1/PV2/PV3、IV1、AC1–AC3 在 `verification.md` `## Checks` 六行全为 NOT_APPLICABLE（apply 阶段待补） |
| Check Plan | `plan.md:988-998`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）与 `plan.md:1025-1032`（替代检查表）；`verification.md` `## Check Plan Changes` 记录 Round 1 的 F1–F11 修正 |
| Previous Findings | F1–F11（Round 1）、F12/F13（Round 3）、F14（Round 4）、F15/F16（Round 5）、F17（MAJOR）/F18/F19/F20（Round 6）、F21–F24（Round 7），逐项复核见下 |
| 实际检查范围 | (a) 两处变更的收尾完整性（plan/tasks/verification 三处的旧 `WP5` 与旧 `MU3` 引用、tasks 重编号、Branch Validation 的 WP5a/WP5b 配对）；(b) 12 包的 Owner≠Reviewer 与独立检视任务；(c) waves 重算 earliest、无环、池容量、Serialization Reason 取值域；(d) 8 个交付单元解析、成员并集、orderGroup；(e) WP5a/WP5b 写入范围与登记覆盖；(f) `crates/sync-protocol` 对前端不可达；(g) Contract Freeze ⊆ Dependencies；(h) 新增运行时资源行的六列与消费者；(i) Round 7 实质结论（依赖属实性对照四个仓库事实、Shared File Ownership 覆盖）对当前字节的保持；(j) `agentic-coverage` 块 102 行的任务映射；(k) tasks.md 对 planningDigest 的参与机制 |
| 未验证内容 | 未执行任何构建/测试/E2E；未评估用例覆盖充分性（归 validator 与主 Agent Coverage Index）；未判断 MU3a–MU3e 的候选构建与 receipt 能否实际产出（属 apply 阶段 merger 职责）；`approved-plan.json` 仍是 Round 3 审批快照（其 `planningDigest` 为 `b484b561…`、包集合含已退役的 `WP5`），但仓库与 `node_modules/@dongfanglin/openspec-agentic` 内**无任何代码读取该文件**，故不构成规划缺陷，仅作记录 |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r8.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行）；Main E2E `DDR-sync-scope-and-pwa-client-r1`（本报告，Round 8） |

### 本轮声明的两处变更逐项核实

**变更一：WP5 → WP5a/WP5b，MU3 → MU3a–MU3e**

| 声明 | 结论 | 依据 |
| --- | --- | --- |
| `WP5` 改为 `WP5a`（骨架 + `src/protocol/` + `src/domain/` + 空壳路由）与新增 `WP5b`（`src/platform/` 三个 `.web.ts`） | **plan 侧成立，tasks 侧未收尾** | `plan.md:864-865` 两行齐备且 Write Scope 不相交（工具口径与人工展开均不重叠）；但 `tasks.md:27` 仍把 `src/platform/secure-storage.web.ts` 写进 2.5（WP5a）的正文（F25），`tasks.md:26` 的 WP5b 描述丢失六处行内标识（F26） |
| `WP6`/`WP7`/`TP3` 依赖更新 | **成立** | `plan.md:866`（`code:WP5a, code:WP5b, contract:WP1`）、`:867`（`code:WP6, code:WP5a, code:WP5b, contract:WP1, contract:WP2`）、`:870`（`code:WP5a, code:WP5b, code:WP6, code:WP7, contract:WP1`），三条 Contract Freeze 引用全部落在依赖列内 |
| 波次改为 W2/W3/W4/W5/W6 | **成立** | 12 行声明波次与重算 earliest 逐行相等；无 waveIndex>minimum 的行，故 12 行 `NOT_APPLICABLE` 全部合法 |
| Merge Strategy 以 MU3a–MU3e 替换 MU3，Order 4–8 | **成立** | `deliveryUnits()` 实跑 8 单元、mode 全 `independent`、orderGroup 1–8 与表序同向、成员并集恰为 12 个已声明 ID |
| Completion Criteria 与 Premerge History 句命名全部八个单元 | **部分成立** | 单元名逐一列出正确（`plan.md:1047`/`:1048`），但同句量词仍写「全部**四个**交付单元」而括号内列了八个（F30）；`plan.md:1006`/`:1043`/`:1051` 仍写「MU1–MU3」 |
| 新增 WP5b 的 Dependency Handoffs 行 | **成立** | `plan.md:938` 四列齐备；相邻的 WP6/WP7/TP3 三行仍以已退役的 `WP5` 为上游（F31） |
| tasks.md 2.5 拆为 2.5/2.6 并下游重编号 | **编号成立，内容未收尾** | 12 个包各有且仅有一条 `[wp:…]` 派发任务（2.1–2.5、2.6、2.7、2.8、4.4、4.7、4.11、4.14），`§3` 的 3.9/3.10 与 3.11/3.12 构成 WP5a/WP5b 配对；但 2.6 被放到 `tasks.md:3`（§1 之前，F26），2.5 的正文仍以旧 `WP5` 名义覆盖平台层（F25），`plan.md` 的 agentic-coverage 块未同步重编号（F28），`§5`/`§6` 仍按单一 MU3 单元编排（F29） |
| 全部旧 `WP5` 引用已消失 | **不成立** | plan.md 11 处、tasks.md 9 处、verification.md 2 处仍写 `WP5`（其中 Shared File Ownership 与 Dependency Handoffs 属在册登记而非历史记账），详见 F31 |

**变更二：新增前端静态服务器运行时资源行**

| 声明 | 结论 | 依据 |
| --- | --- | --- |
| `## Runtime Resources` 新增 TP4 消费的 Vite/Metro dev server 行 | **六列齐备，机制正确** | `plan.md:953` 六列全部填写：Checks/Work Packages=`TP4`、Resource=`前端静态服务器（Vite/Metro dev server）`、Isolation=`独立端口；进程生命周期随分片，端口需错开`、Exclusive Scheduling=`NOT_APPLICABLE`、Allocate/Isolate/Start/Ready/Cleanup=`provisioner 为每个 TP4 分片分配端口、启动并做就绪检查`、Use/Cleanup Boundary=`分片结束即终止其服务器进程；服务器只服务该分片的 worktree`。它确实闭合了「TP4 的浏览器验证无已登记 HTTP 入口」的缺口 |
| 消费者列表与改名后的包一致 | **本行一致，相邻行不一致** | 本行消费者 `TP4` 正确（TP4 只依赖 `code:WP7`，未改名）；但同表 `plan.md:952` 的 `clients/app/node_modules/` 行写 `WP5a–WP7`，该区间在重命名后是否含 WP5b 不可判定（F32）；`verification.md:82-91` 的同名节未新增该行且仍写 `WP5`（F33） |
| 该行声明「验证环境而非产品路径」 | **行内未见该限定** | `plan.md:953` 的六个单元格均无「不能连真实 Daemon、只服务 fixtures」的表述；该语义目前只存在于 `plan.md:1059`（User Deliverables 的「未配置：缺 `server::sync` 与 `/ui` 托管」）与 `## Main E2E` 的 `not-applicable` 段。二者不冲突，但资源登记本身未承载该限定（F33） |
| 与 Main E2E `not-applicable` 判定一致 | **一致** | 登记的是 TP4 的分片内静态服务器，与产品路径 `/ui` 托管无关；`plan.md:1060` 的 User Deliverables 仍明确「静态托管入口：不产出」，无新增矛盾 |

### Round 7 结论在当前字节的保持情况

| Round 7 结论 | 本轮结论 | 复核依据 |
| --- | --- | --- |
| F17（MAJOR）TP1 的 `@MU1a` 不合格 | **维持已解决** | `plan.md:868` 的四条冻结路径全部按工作包 ID 命名且落在 `code:WP1, code:WP2` 内；本轮 12 包的 22 条 `@` 引用中仅 WP5b 一条越界（F27，与 F17 不同源） |
| F18 TP1 的 handoff 行 | **维持已解决** | `plan.md:943` 在位 |
| F19（单元计数/命名陈旧） | **维持未解决，并因本次拆分加重** | `plan.md:1047` 的量词「四个」与所列八个单元自相矛盾（F30）；R1–R4 的 Closure Unit 仍为 `MU1/premerge`、R10–R20 为 `MU3/premerge`/`MU3/final`，而 MU1/MU3 均已不是交付单元（F22） |
| F20（tasks.md 单元名与成员未同步） | **维持未解决，同类问题在 MU3 侧重现并升级** | `tasks.md:84` 与 `:110-117` 仍按单一 MU3 单元编排，且与 `plan.md:967-976` 直接矛盾（F29） |
| F21（`tasks.md:88` 旧单元名 MU1） | **已解决** | `tasks.md:92` 的 6.5 现为 `MU1a`；该行即附带观察所指的一行修正，且未刷新 planningDigest |
| F13（Contract Freeze 机械不可核对） | **维持未解决（非阻断）** | 多路径单元格仍不匹配 `^(\S+?)@(\S+)$`；所引路径逐个存在。当前波次合法性正是在「冻结不可读 ⇒ contract 视为硬依赖」口径下逐行自洽 |
| F14（WP7 的 handoff 漏 WP1） | **维持未解决，范围扩大** | `plan.md:940` 的 Upstream 仍为 `WP5, WP6, WP2`：漏 `WP1`，且 `WP5` 已不存在、WP5a/WP5b 两个真实上游均未登记 |
| F1/F2/F3/F5/F6/F7/F9/F10/F11/F12 已解决 | **维持** | 依赖属实性对照四个仓库事实未变；`plan.md:916`/`:924`-`:929` 登记行在位 |
| 无环、波次合法、池容量、写入归属、reviewer 独立性、单元解析 | **全部维持** | cycles=[]；12 行波次逐行等于 earliest；W1–W6 未超 `coding=3/testing=2`；12 包 Owner≠Reviewer 且各有独立检视任务；8 单元解析无抛错、orderGroup 无倒置 |
| Coverage Index 引用有效性（102 行 tasks 均存在于 tasks.md） | **形式通过、语义失效** | 102 行引用的 17 个任务 ID 全部存在，但 `2.6`/`2.7` 在重编号后指向了不同的包，`2.8` 被 0 行引用（F28） |

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F25 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:25`（`2.5` 标题行）与 `tasks.md:27`（2.5 的 WP5 正文子条目）；权威声明在 `plan.md:864`（WP5a Write Scope）与 `plan.md:865`（WP5b Write Scope） | `tasks.md:25` 的标题行是「派发 **WP5** coder」而标签为 `[wp:WP5a]`；其正文子条目 `tasks.md:27` 写「**WP5**：实现 R10/R11/R15/R19/R20 的基础层……`src/platform/secure-storage.web.ts` 用 WebCrypto 生成不可导出的 P-256 `CryptoKey` 并持久化到 IndexedDB」，并把 R11（设备身份）列入本任务。`plan.md:864` 的 WP5a Write Scope 为 `clients/app/package.json`、`clients/app/app/`（空壳路由）、`clients/app/src/{domain,protocol,components}/`，**不含 `src/platform/`**；`plan.md:865` 明确 `src/platform/` 三个 `.web.ts` 归 WP5b，且该行的 Inputs/Outputs 列写明「与 WP5a 分包是为了把跨语言字节对拍这块最高风险单独隔离」。`plan.md:909-929` 的 17 行 Shared File Ownership 中没有 WP5a×WP5b 的任何行 | 拆分 Change 1 的核心目的（把最高风险的设备身份任务从 WP5a 隔离出去）在 tasks 层被撤销：coding-5 的权威任务文本要求它写 WP5b 的文件，而 WP5b 的任务（`tasks.md:3`）没有给出任何范围说明。两个执行者会拿到互相矛盾的写入指令——要么重复实现 `secure-storage.web.ts`，要么该文件在 MU3a/MU3b 之间无人交付；且 WP5a 与 WP5b 分处 W2/W3、属 MU3a/MU3b 两个交付单元，一旦双方都写，跨单元合入时的冲突没有登记的合并负责人与重跑项（`ownershipProblem()` 无行可依） | 把 `tasks.md:25` 改为「派发 **WP5a** coder」；把 `tasks.md:27` 的正文收窄到 `plan.md:864` 的范围（骨架 + `src/protocol/` + `src/domain/` + 空壳路由）与 R10/R15/R19，删除其中的 `src/platform/secure-storage.web.ts` 段与 R11；R11/R20 的平台侧内容只保留在 WP5b 的任务里 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F26 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:26`（2.5 下的 WP5b 子条目）与 `tasks.md:3`（游离的 `2.6` 行） | `tasks.md:3` 的 `- [ ] 2.6 [wp:WP5b] [PV3] 派发 WP5b coder，按 apply 协议登记开工/接收；依赖 WP5a 的交付提交已合入（MU3a）` 出现在文件第 3 行，即 `## 1. Dependency and Resource Setup`（`tasks.md:4`）**之前**，不在 `## 2. Implementation` 段内；而 §2 内的 2.5 与 2.7 之间没有任何 2.6 行。原本承载 WP5b 范围说明的子条目留在 `tasks.md:26`，但它的六处行内标识已被清空：`WP5b（独立包，）`（缺依赖类型）、`生成**不可导出**的 P-256  并持久化`（缺 `CryptoKey`）、`以及 、。`（缺 `local-cache.web.ts` 与 `lifecycle.web.ts`）、`**不写 **`（缺被排除的对象）、`跨语言字节对拍（）`（缺 `device-proof.json`/`AC3`）。全文 `` `` `` 计数为 0，说明不是渲染问题而是字节本身缺失 | WP5b 在权威任务文件里没有任何可执行内容：其 `[wp:WP5b]` 派发任务不写范围、不写依赖、不写完成条件，唯一描述它的子条目挂在另一个包（WP5a）的任务下且关键标识全空。派发 coding-5b 时无据可依——`plan.md:865` 的三个文件、`code:WP5a` 的依赖与「本包只交实现、对拍由 TP3 执行」的分工都无法从 tasks.md 传达；`local-cache.web.ts` 与 `lifecycle.web.ts` 尤其危险：它们既不在 `tasks.md:26` 的（已损坏）文本里，也不在 2.6 的行里，而 R20（imported 资源不落盘、`local-cache` 固定上限淘汰）依赖前者 | 把 `tasks.md:26` 的 WP5b 子条目移入 §2，恢复为独立的 `- [ ] 2.6 [wp:WP5b] [PV3]` 行并补齐六处标识（`code:WP5a`、`CryptoKey`、`local-cache.web.ts`、`lifecycle.web.ts`、被排除的原生 adapter、`device-proof.json`/`AC3`），内容与 `plan.md:865` 的 Inputs/Outputs 列逐项对应；删除 `tasks.md:3` 的游离行 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F27 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:865`（WP5b 行的 Contract Freeze 列；对照同表 Dependencies 列与 `plan.md:938` 的 handoff 行） | WP5b 的 Contract Freeze 为 `` `crates/sync-protocol/src/…`（不 import，仅按 transcript 规范实现）, `fixtures/sync/v1/transcripts/device-proof.json@WP1` ``，但 Dependencies 列只有 `code:WP5a`，**没有 `contract:WP1`**。本变更自订的规则在 `verification.md` `## Check Plan Changes` 的 F4 与 F10 两行被明文写下（「freeze 引用必须在依赖列内」），Round 6 更以该规则把 TP1 的同类问题判为 MAJOR（F17）。12 包 22 条 `@` 引用中这是唯一越界项 | WP5b 的全部存在理由是「与 `device-proof.json` 固定向量对拍」，该向量由 WP1 产出并冻结（`plan.md:938` 的 handoff 行也写「固定向量路径可读」）。这条契约边既不在 Dependencies，也就不在 `hardDeps` 内：一旦 WP1 的 transcript 向量在 MU1a 之后变更，WP5b 的波次、就绪条件与失效路径都没有依据——`plan.md:938` 的 Invalidation 列只写了「WP5a 的 transcript 编码变更」，没有 WP1 变更的路径，而向量文件恰恰归 WP1（`plan.md:860` 的 `fixtures/sync/v1/`）。跨单元（MU1a → MU3b）的失效传播因此缺一条登记 | 在 `plan.md:865` 的 Dependencies 列补 `contract:WP1`（与 WP5a/TP3 同型），并在 `plan.md:938` 的 Upstream 补 `WP1`、Invalidation 补「WP1 的 `fixtures/sync/v1/transcripts/**` 变更 → WP5b 重跑 PV3 与 AC3」。核对：补边后 earliest(WP5b)=1+max(earliest(WP5a)=2, earliest(WP1)=1)=3，与现有 W3 相等，故**无需改波次、无需补 Serialization Reason** | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F28 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/plan.md` 的 `agentic-coverage` 结构化块（102 行的 `tasks` 字段）；受影响的编号定义在 `tasks.md:25-31` | 拆分把 §2 的编号从 2.1–2.7 变为 2.1–2.8（2.6=WP5b、2.7=WP6、2.8=WP7），但 Coverage Index 的结构化块未同步。实测引用分布：`2.8` 被 **0 行**引用；引用 `2.7` 的 22 行恰为 R062–R083，对应 `plan.md:867` WP7 的 R15–R19 全集（目录页、会话创建、上下文三态、降级卡片、Agent 目录两层）；引用 `2.6` 的 31 行恰为 R005–R014 + R050–R061 + R080–R083 + R084–R088，对应 `plan.md:866` WP6 的 R2/R3/R12–R14/R19/R20 全集。拆分前 2.6=WP6、2.7=WP7，故这 53 行整体错位一格；此外 R045–R049（设备身份）与 R084–R088（imported 缓存）应同时引用 WP5b 的任务，却仍只引 `2.5` | 53/102 条覆盖行被归属到错误的工作包：main 在 `tasks.md:99`/`:114`（覆盖核对）与 `plan.md:1006` 的 IV1、`tasks.md:131` 的 9.1 做闭环时会按错误的任务核对场景；按 `planning-model.mjs:37-40` 的 `taskPackages` 口径重算，引用 `2.6` 的 31 行会同时挂到 WP5a 与 WP5b（`taskPackages.get('2.6')=['WP5a','WP5b']`，因 `tasks.md:3` 的正文提到 WP5a），WP6 的场景被记在设备身份包名下。R045–R049 的字节对拍场景没有任何行指向 WP5b 的任务，WP5b 在覆盖台账中缺位。机械门禁不读这一层（`workflow-check.mjs:292` 只读 `checks` 字段），故不会被 `workflow check` 拦下 | 把引用 `2.6` 的 31 行改为 `2.7`、引用 `2.7` 的 22 行改为 `2.8`；R045–R049 与 R084–R088 另补 `2.6`（与 F26 修复后的 WP5b 任务一致）。同批把 `plan.md:39-49` 叙述表的 Responsible Units 中的 `WP5` 改为 `WP5a`/`WP5b`（R11、R20 归 WP5b；R10、R15、R19 归 WP5a） | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F29 | MAJOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:84`（`5.5`）与 `tasks.md:110-117`（`6.22`–`6.29`）；权威声明在 `plan.md:967-976`（Merge Strategy）与 `plan.md:978`（Merge Worktree） | `plan.md:972-976` 已把前端拆成 MU3a(WP5a)/MU3b(WP5b)/MU3c(WP6)/MU3d(WP7)/MU3e(TP3,TP4) 五个独立单元（Order 4–8），`plan.md:978` 写明「MU3a–MU3e 各自独立构建候选 worktree，逐单元串行合入」。而 `tasks.md:84` 仍写「**MU3**、main 复核 independent 模式的 **WP5**/WP6/WP7/TP3/TP4 组成，确认 **WP5** 已进入已验收集成基线」；`tasks.md:111` 写「构造 **MU3** 候选（**WP5 + WP6 + WP7**）……本单元内 WP5→WP6→WP7 串行合入，**复用同一合入 worktree**」，`tasks.md:110`/`:112`-`:117` 全部以 MU3 为单元名。全文没有任何 MU3a/MU3b/MU3c/MU3d/MU3e 的 §5 复核任务或 §6 合入任务 | 两处权威文件对前端如何合入给出互相矛盾的指令：按 tasks.md 执行会把 WP5a/WP5b/WP6/WP7 组装成单一候选并复用同一合入 worktree，正是 MU1 拆分要消除的「验证对象退化为漂移的工作区」问题；`plan.md:1048` 的 Completion Criteria 要求「`## Premerge History` 中 MU1a/MU1b/MU2/**MU3a–MU3e** 各有对应 PASS」，而现有 §6 只能产出一个 MU3 的 premerge receipt，五项中的四项无处产生。MU3b 的候选还必须等 MU3a 合入（`plan.md:973`），而 `tasks.md:111` 的串行合入写法使该等待不可见 | 按 `plan.md:972-976` 把 `tasks.md:84` 拆为 MU3a–MU3e 五条复核任务，把 `tasks.md:110-117` 拆为五组 §6 任务（每组至少覆盖候选基线机械核实、候选 PV3、独立 review、覆盖核对、premerge/receipt/合入、主分支回归），并把 `tasks.md:111` 的「复用同一合入 worktree」改为「各自独立构建候选 worktree、逐单元串行合入」 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F30 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:1047` | 该行为「最终目标：`refs/heads/main` 的固定提交，全部**四个**交付单元（MU1a / MU1b / MU2 / MU3a / MU3b / MU3c / MU3d / MU3e）已合入」——量词「四个」与同句列出的八个单元自相矛盾（八个是本轮两处变更后的正确值，见 `deliveryUnits()` 实跑） | 完成判据的计数词与枚举自相矛盾，main 在最终验收逐字核对时会读到两种互斥读法。**不构成 MAJOR**：同一行的枚举本身完整正确，且 `plan.md:1048` 的 Premerge History 句已正确写为「MU3a–MU3e」 | 把「四个」改为「八个」 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F31 | MINOR | 已退役的 `WP5` 在 `plan.md:39,40,44,48,49`（Coverage Index 叙述表，Responsible Units 与 Closure Unit）、`plan.md:876`（code 依赖依据）、`plan.md:919,920,928,929`（Shared File Ownership 四行）、`plan.md:939,940,942`（Dependency Handoffs 三行）、`plan.md:994`（PV3 的 Stages 列）；`tasks.md:11,25,27,29,31,71,84,111`；`verification.md:80,90` | plan.md 中 11 处、tasks.md 中 9 处、verification.md 中 2 处仍写 `WP5`。其中 `plan.md:919`（`clients/app/package.json` 的 Writers=`WP5, WP6, WP7`、Merge Order=`WP5 → WP6 → WP7`）、`:920`（`clients/app/src/protocol/` 的 Writers=`WP5`、Merge Owner=`coding-5`）、`:939`/`:940`/`:942`（WP6/WP7/TP3 的 handoff 上游）属**在册登记**而非历史记账：`record.writers.includes(wp.id)` 对已退役的 `WP5` 永不成立，`clients/app/package.json` 的合入顺序因此少了真实写入者 WP5a（并完全漏掉 WP5b）；WP6/WP7/TP3 三条 handoff 的 Accepted Revision 写「WP5 已验收集成基线提交」，而 WP6/WP7 的真实上游是 WP5a 与 WP5b 两个单元。`plan.md:994` 的 PV3 仍写「MU3 候选、主分支（WP5–WP7…）」，与 8 单元不符。`verification.md:90` 的资源行与 `:80` 的 handoff 占位行同样写 `WP5` | 在册登记指向不存在的包，使 Shared File Ownership 的合入顺序与 Dependency Handoffs 的失效路径无法按字面执行。**不构成 MAJOR**：这些重叠与边都跨波次且已由 `code:` 依赖排序（WP5a→WP5b→WP6→WP7），波次与交付单元成员声明本身正确；`plan.md:895-901` 的 Enter Condition 文字已正确写出 MU3a/MU3b/MU3c/MU3d | 把 `plan.md:919`/`:920` 的 Writers 与 Merge Order 改为 `WP5a, WP5b, WP6, WP7`（顺序 WP5a → WP5b → WP6 → WP7）；把 `:939`/`:940`/`:942` 的 Upstream 改为 WP5a/WP5b；把 `:994` 改为「MU3a–MU3e 候选、主分支（WP5a、WP5b、WP6、WP7、TP3、TP4）」；`tasks.md:11`、`:29`、`:31`、`:71`、`:84`、`:111` 的旧名随 F25/F29 一并修正。`verification.md:64`、`:138` 等 Round 1/2 的历史记账行保留原文正确，不在此列 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F32 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:952`；同构行 `verification.md:90` | 该行的 Checks / Work Packages 写 `WP5a–WP7, TP3, TP4`（`verification.md:90` 写 `WP5, WP6, WP7, TP3, TP4`）。重命名后 `WP5a–WP7` 是区间写法，是否包含 WP5b 不可判定；WP5b 恰恰是本次新增的、需要独立 worktree 与依赖安装目录的包 | 资源登记对 WP5b 可能落空：WP5b 在 MU3b 有自己的 worktree（`plan.md:978`），若 provisioner 按字面把 `WP5a–WP7` 读作 {WP5a, WP6, WP7}，WP5b 的 `clients/app/node_modules/` 就没有隔离声明。**不构成 MAJOR**：该资源非独占，`workflow-check.mjs:365-379` 只对 `Exclusive Scheduling` 非 NOT_APPLICABLE 的行做同批互斥判定，本行为 NOT_APPLICABLE，无机械后果 | 改为逐个列举：`WP5a, WP5b, WP6, WP7, TP3, TP4`；`verification.md:90` 同步 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F33 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:953`（新增资源行）；未同步的镜像 `verification.md:82-91` | 新增行六列齐备（见变更二核实表），但两点不足：(a) 六个单元格均未承载「验证环境而非产品路径、不能连真实 Daemon、只服务 fixtures」的限定，该语义只存在于 `plan.md:1059` 的 User Deliverables 与 `## Main E2E` 段；(b) Resource 写「Vite/**Metro** dev server」，而 Metro 是 React Native 的打包器，`tasks.md:26` 与 `plan.md:864` 均已裁定「v1 只交付 Web，原生端不在本次范围」。`verification.md` 的 `## Runtime Resources` 四行仍是 plan.md 改动前的镜像（无静态服务器行） | 资源登记与范围裁定的关系只靠读者跨节推断；为已裁出范围的平台登记打包器会让 provisioner 误以为需要 Metro 运行时。**不构成 MAJOR**：与 Main E2E 的 `not-applicable` 判定不冲突，也未新增任何产品路径承诺 | 在 `plan.md:953` 的 Resource 列写「前端静态服务器（Vite dev server，web-only）」，并在 Use/Cleanup Boundary 列补「只服务该分片的 fixture，不连真实 Daemon，不构成产品静态托管入口」；在 `verification.md` `## Runtime Resources` 增同构行 | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F34 | MINOR | `openspec/changes/sync-scope-and-pwa-client/verification.md:42` | `## Dependency Declaration Review` 的 Round 5 行被并进了 Round 7 行的同一物理行：该行有 8 个单元格，而表头（`verification.md:35`）只有 6 列。Round 7 行的前三格（`DDR-sync-scope-and-pwa-client-r1` / `5` / `reviewer-plan-ddr-5`）连同行首的 `|` 一并丢失，`workflow-check.mjs` 的 `pick()` 按列名取前 6 格，故 Round 5 的历史记录（`plan-v2:6751091d…`、PASS、`reports/ddr-…-r5.md`）在机械读取中不存在 | Round 5 的历史结论不可机械读取；这也是 `workflow check --stage plan` 报「Plan Revision 未绑定当前规划契约摘要」的直接来源（表内可读的最大 Round 是 7，绑 `8eb6a3eb…dd4ad`，非当前 `b2363f45…c41d3`）。**不构成 MAJOR**：属 `procedures/scheduling.md`「机械格式检查失败先修记录」的登记问题，由主 Agent 在导入本报告时一并修正 | 把 Round 5 行拆回独立的 6 格行（Review ID / Round 5 / `reviewer-plan-ddr-5` / `plan-v2:6751091de39cd3dd86425f8e674416ef4799d43e845a707399daba4884f59fef` / PASS / `reports/ddr-sync-scope-and-pwa-client-r5.md`），并在末行补 Round 8（Reviewer `reviewer-plan-ddr-8`、Plan Revision `plan-v2:sha256:b2363f45d2d79263566e707a87668649384c045c4e5572dce1b3feb9769c41d3`、Result FAIL、本报告路径，不带反引号） | 新发现（Round 8） |
| DDR-sync-scope-and-pwa-client-r1-F14 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:940` | Round 4 提出、维持未解决，范围因本次拆分扩大：WP7 的 Dependencies 为 `code:WP6, code:WP5a, code:WP5b, contract:WP1, contract:WP2`（`plan.md:867`），handoff 行的 Upstream 仍写 `WP5, WP6, WP2`——既漏 `WP1`（原 F14），又因 `WP5` 已退役而漏掉 WP5a 与 WP5b 两个真实代码上游 | `contract:WP1` 的上游没有 Accepted Revision 与 Invalidation 落点；WP5a/WP5b 的上游同样只在字面失效的 `WP5` 名下。**不构成 MAJOR**：WP7 的波次（W5）已晚于两个上游（W2/W3）且有 `code:` 依赖排序，缺的是失效登记 | Upstream 改为 `WP5a, WP5b, WP6, WP1, WP2`，Invalidation 补「WP1 的 `schemas/sync/v1/**` 变更 → WP7 重跑 PV3」 | 未解决（Round 4 提出，Round 5/6/7/8 复核仍未修正；本轮范围扩大） |
| DDR-sync-scope-and-pwa-client-r1-F13 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:858-871`（12 个包的 Contract Freeze 单元格） | Round 3 提出、维持未解决。按 `workflow-check.mjs:91-98` 与 `planning-model.mjs:59` 的同一正则复现：多路径单元格不匹配 `^(\S+?)@(\S+)$`，故 12 个包全部 `readable=false`（含本轮新增的 WP5b） | 冻结可核对性依赖人工而非机械判据。**不构成 MAJOR**：22 条 `@` 引用的路径逐个存在、无一条写错，且引用 ⊆ Dependencies 仅 F27 一处越界；当前波次合法性正是在「冻结不可读 ⇒ contract 视为硬依赖」口径下逐行自洽 | 同 Round 3–7 的建议：去掉单元格内反引号并把多路径拆到单条。**注意耦合**：若规范化后冻结变为可读，必须同步为受影响包补合法 `Serialization Reason` 码，否则 `workflow check --stage plan` 会转 FAIL | 未解决（Round 3 提出，Round 4/5/6/7/8 复核仍未修正） |
| DDR-sync-scope-and-pwa-client-r1-F22 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:29-32`（R1–R4 的 Closure Unit 列）、`plan.md:39-49`（R10–R20 的 Closure Unit 与 Responsible Units）、`plan.md:1006`、`plan.md:1043`、`plan.md:1051` | Round 7 提出、维持未解决。R1–R4 的 Closure Unit 仍为 `MU1/premerge`（MU1 已拆为 MU1a/MU1b）、R10–R20 仍为 `MU3/premerge`/`MU3/final`（MU3 已拆为 MU3a–MU3e）；`plan.md:1006` 的 IV1 Target Revision 仍写「MU1–MU3 全部合入后」，`:1043`/`:1051` 仍写「MU1–MU3」 | 单元级核对的可追溯性下降：Closure Unit 列不被任何机械判据读取（已确认 `workflow-check.mjs` 无 `closure` 相关读取），但 main 在 `tasks.md:81`/`:99`/`:106`/`:114` 与 IV1 按 Coverage Index 做闭环时，这些列指向不存在的单元。**不构成 MAJOR**：叙述性单元名陈旧，非声明不实 | R1–R4 改为 `MU1a+MU1b/premerge`，R10–R20 按实际闭合单元改为 MU3a/MU3d/MU3e 等；`plan.md:1006`/`:1043`/`:1051` 改为逐一列名 | 未解决（Round 7 提出，Round 8 复核仍未修正；本轮范围因 MU3 拆分扩大） |
| DDR-sync-scope-and-pwa-client-r1-F23 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:82` | Round 7 提出、维持未解决。`5.3` 写「MU2、main 复核 independent 模式的 WP3/WP4/TP2 组成，确认上游 **WP1/WP2 已合入主分支**且契约冻结版本可读」，而 `plan.md:971` 的 MU2 Start/Readiness 写的是「**MU1b** 已合入主分支且契约冻结可读」。MU2 的 Order=3 在 MU1b 的 Order=2 之后 | main 若按 tasks.md 的字面复核 MU2 就绪，会在 TP1 尚未合入时勾掉 5.3。**不构成 MAJOR**：`scheduling-status.mjs:150-151` 对 orderGroup 更小的未合入单元会机械产生 blocker，实际合入顺序不可能被措辞倒置 | 与 `plan.md:971` 对齐，写成「MU1a/MU1b 已合入主分支（MU1a 提供 WP1/WP2 的契约冻结，TP1 归 MU1b）」 | 未解决（Round 7 提出，Round 8 复核仍未修正） |
| DDR-sync-scope-and-pwa-client-r1-F24 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:96-100` | Round 7 提出、维持未解决。MU1b 组仍为 5 项（6.9 合入负责人/6.10 候选 PV3/6.11 独立 review/6.12 覆盖核对/6.13 premerge+合入+主分支回归），对照 `templates/tasks.md` 的八项单元模板与 MU1a/MU2/MU3 三组写法，仍缺「候选构建前机械核实目标仓库/主分支引用」与「合入后独立差异复检」两项，且 6.13 把主分支回归并入 merger | MU1b 缺合入后的主分支差异复检登记，与其余单元的分工不一致。**不构成 MAJOR**：缺的是任务登记而非依赖声明，`roles/merger.md` 自身要求 merger 核对实际合入结果 | 按 MU1a 组的结构补两项，并把主分支回归单列为检查执行者任务 | 未解决（Round 7 提出，Round 8 复核仍未修正） |

## 说明（不计为问题）

- **`workflow check --stage plan` 报出的五条「缺少执行者接收确认」（WP2 Attempt 1 coding / Attempt 1 reviewing / Attempt 2 fixing、WP1 第 2 轮 reviewing、WP2 第 3 轮 reviewing）属已知、用户已接受的派发台账过程缺口，不是规划缺陷。** 根因是派发说明未要求执行者运行 `dispatch --ack`，attempt 推进后无事后补录入口；五个执行者均已确认收到并实际交付（`verification.md` 的 `## Dispatch Reconciliation`），用户 2026-10-03 裁定选 (a) 接受为已知偏离。本报告不将其计为 Finding，亦不影响本轮结论；按该裁决，final/archive 阶段须在 `## Final Assessment` 中一并复述。
- **门禁的第六条 error（DDR Plan Revision 未绑定当前规划契约摘要）是登记滞后，非规划缺陷**，其直接成因是 F34 的表格行合并。按 `procedures/scheduling.md`「机械格式检查失败先修记录」，由主 Agent 在导入本报告时一并修正。
- **`plan.md:902` 的空行把 `## Execution Waves` 切成两张 Markdown 表**（第一张 W1–W6 九行，第二张 W2/WP3、W2/WP4、W2/TP1 三行且无表头）。解析侧无影响：`workflow-check.mjs:67-80` 的 `tableRows()` 跳过空行继续收集 `|` 开头行，12 行全部读入并完成 12/12 的波次校验（故 `workflow check` 未报「工作包未归入 Execution Waves」）；但渲染侧第二块不是表格。该问题先于本轮两处变更存在，不计为 Finding，仅记录供格式化时合并。
- **`plan.md:951` 的 `cargo` 构建目录消费者为 `WP3, WP4, WP2, TP1, TP2`，未含 WP1**，而 WP1 的 Verification 列有 PV1（`npm run verify` 含 `check:rust`）。该行先于本轮两处变更存在，且 `plan.md:887` 说明各 WP 使用独立 `CARGO_TARGET_DIR`，不影响归属判据，不计为 Finding。
- **`approved-plan.json` 仍是 Round 3 的审批快照**（`model.planningDigest` 为 `b484b561…`，`packages` 含已退役的 `WP5` 及其对 WP1/WP2 的依赖声明）。已在 `openspec/` 与 `node_modules/@dongfanglin/openspec-agentic/` 全目录检索：**无任何代码读取该文件**，它不参与 `planningDigest`、不参与任何门禁，也不被 `workflow record` 消费。故不计为规划缺陷，仅提示主 Agent：该快照与当前 12 包模型已不同步，归档前应说明它是历史审批记录还是需要重建。
- **变更二与 Main E2E 的 `not-applicable` 判定不冲突。** 新登记的是 TP4 分片内、只服务该分片 worktree 的静态服务器；`plan.md:1060` 的 User Deliverables 仍明确「静态托管入口：不产出」，`## Main E2E` 的 `server::sync` 未落地与无浏览器驱动两条判据未因本次登记而改变。AC1–AC3 三条替代检查的入口（`npm run verify`、`cargo test -p app --test node_link_e2e`、`clients/app` 契约测试）均不依赖该服务器，可满足性不变。
- **`plan.md:965` 的拆分说明段仍只叙述 MU1 的拆分依据，未补 MU3 的拆分理由**（变更一记录的理由是「MU1 的拆分由验证目标问题驱动，MU3 的问题是风险分布不均」）。该理由写在 WP5b 行的 Inputs/Outputs 列（`plan.md:865`），但未进入 Merge Strategy 的模式说明段。不计为 Finding：单元成员与 Order 已由表声明并经 `deliveryUnits()` 复现通过。
- **`plan.md:876` 的 code 依赖依据条目把 WP6 的上游写成 `code:WP5` 并称其同时消费 `src/protocol/` 与 `src/platform/`**，而 `src/platform/` 现归 WP5b。该行属叙述性依据（与 `:881`、`:882` 的移除说明同类），已并入 F31 一并修正。

## Assessment

### 本轮结论

**FAIL**。本轮两处变更在 `plan.md` 的结构层（工作包行、依赖列、波次、交付单元、运行时资源行）**全部成立并经机械复现通过**；但两处变更在 `tasks.md` 与 `plan.md` 的 Coverage Index 层的收尾**未完成**，产生 5 项有证据的 MAJOR。

逐项依据：

1. **结构层全部通过（变更一、二的声明成立）。** 8 个交付单元经 `deliveryUnits()` 实跑解析为 `independent`，成员并集恰为 12 个已声明 ID，无重复、无遗漏，orderGroup 1–8 与表序同向无倒置；12 行波次与按 `workflow-check.mjs:205-221` 口径重算的 earliest 逐行相等，无环、无 `waveIndex>minimum` 的行，W1–W6 均未超 `coding=3/testing=2`；WP5a/WP5b 的 Write Scope 工具口径与人工展开均不重叠；`crates/sync-protocol` 仍不可从前端包到达（依赖目标集合为 {WP1, WP2, WP5a, WP5b, WP6, WP7}，无 crate 目标，唯一提及带显式「不 import」否定说明）；新增的静态服务器资源行六列齐备、消费者 `TP4` 与改名后的包一致，且与 Main E2E 的 `not-applicable` 判定不冲突；12 个包 Owner≠Reviewer 且各有恰好一条 `[wp:…]` 派发任务与至少一条独立检视任务。
2. **F25（MAJOR）：tasks.md 仍把 WP5b 的核心文件指派给 WP5a。** 这是对变更一核心目的的直接撤销——`plan.md:865` 用 Inputs/Outputs 列写明「与 WP5a 分包是为了把跨语言字节对拍这块最高风险单独隔离」，而 `tasks.md:27` 让 coding-5 写 `src/platform/secure-storage.web.ts` 并承担 R11。两个包分处 W2/W3、属 MU3a/MU3b 两个交付单元，且 `plan.md:909-929` 无 WP5a×WP5b 的登记行，跨单元冲突无合并负责人与重跑项。
3. **F26（MAJOR）：WP5b 的任务内容在 tasks.md 中不存在。** `[wp:WP5b]` 派发行漂到 `tasks.md:3`（§1 之前）且只有一句派发语；真正的范围说明留在 2.5 的子条目里并丢失六处行内标识（含依赖类型与三个文件名）。派发 coding-5b 时无据可依，`local-cache.web.ts` 与 `lifecycle.web.ts` 尤其无归属叙述。
4. **F27（MAJOR）：WP5b 的 Contract Freeze 引用不在其 Dependencies 列。** 这是本变更自订规则（F4/F10）与 Round 6 F17 判例所覆盖的同一类，而 WP5b 的全部价值就是与 WP1 的 `device-proof.json` 对拍。补 `contract:WP1` 后 earliest 仍为 3，**不需要改波次**，修复成本极低。
5. **F28（MAJOR）：Coverage Index 结构化块未随重编号，53/102 行归属错误。** 引用 `2.8` 的行数为 0；引用 `2.7` 的 22 行恰是 WP7 的 R15–R19 全集；引用 `2.6` 的 31 行恰是 WP6 的 R2/R3/R12–R14/R19/R20 全集。该层不被任何机械门禁读取，只会在 main 的覆盖闭环与 IV1 中生效，因此必须在合入前修正。
6. **F29（MAJOR）：tasks.md 的 §5/§6 与 8 单元的 Merge Strategy 直接矛盾。** 按 tasks.md 执行会把 WP5a/WP5b/WP6/WP7 组装为单一候选并复用同一合入 worktree，而 `plan.md:978` 要求各自独立构建候选 worktree；`plan.md:1048` 要求的 MU3a–MU3e 五条 Premerge PASS 中有四条在现有 §6 下无处产生。这是 R6 F20 同类问题在 MU3 侧的重现，因已升级为「与权威表矛盾 + 四单元无合入任务」而判 MAJOR。
7. **其余 9 项发现（F30–F34 与结转的 F13/F14/F22/F23/F24）均为 MINOR**，其中 F31（旧 `WP5` 在在册登记中的残留）是 F25/F29 之外的第三类收尾遗漏，F34 是导致门禁第六条 error 的登记格式问题。
8. **Round 7 的全部实质结论对当前字节继续成立**：依赖属实性（对照 `crates/core/Cargo.toml`、`crates/agent-host/Cargo.toml`、`scripts/check-crate-boundaries.mjs`、`docs/MODULE_ARCHITECTURE.md` §5 四项仓库事实）、无环与波次合法性、池容量、写入归属登记、reviewer 独立性、单元解析与 Order。

按角色判定规则（存在已确认且未解决的 CRITICAL/MAJOR 问题时为 FAIL），本轮 5 项 MAJOR 均已确认成立且未解决，故结论为 **FAIL**。

### 附带确认：`tasks.md` 是否属于 planningDigest

**观察成立，且机制明确。** `planning-model.mjs` 的 `planningModel()` 构造 `planningDigest = digest({ globalDigest, packages: {WP→planningHash} })`：

- `globalDigest` 读 `proposal.md`、`design.md`、`.openspec.yaml` 与 `plan.md` 的若干节（Scope/Contract Changes 之外的固定节集合、Runtime Resources、Shared File Ownership、Merge Strategy、Target Repository、Main E2E 等），**不含 tasks.md**；
- 每个包的 `planningHash` 含 `declaration`（该包的 Work Packages 行，忽略 Owner/Reviewer/Branch/Verification 四列）、`wave`、来源 spec 摘要、`coverage`（映射到该包的需求行 ID）、`frozenContract`，**不含任务正文**；
- tasks.md 只以 `taskPackages` 间接参与：由每个 `- [ ] N.M` 行的**正文**推导该任务提及哪些 WP ID，用于把 coverage 行归到包。该映射只取「行文本中出现的包 ID」，因此改动任务正文里不涉及包 ID 的文字（例如把 `MU1` 改成 `MU1a`）不改变任何 `taskPackages` 条目，也就不改变摘要；
- 任务正文全文确实进入 `verificationHash`，但 `verificationHash` **不参与 planningDigest**。

实证：Round 7 的 F21 建议正是「把 `tasks.md` 中 6.5 的单元名由 `MU1` 改为 `MU1a`」，该行现已改为 MU1a，而摘要从 `8eb6a3eb…dd4ad` 变为 `b2363f45…c41d3` 的原因只可能来自 plan.md 的两处变更（F25/F29/F27/F28 的 counterparts 均在 plan.md），tasks.md 的这一行修正对摘要零贡献——与观察一致。

**对本轮处置的直接含义**：F25、F26、F29 三项 MAJOR 全部位于 tasks.md，单独修正它们**不会**刷新 planningDigest；F27、F28 位于 plan.md，修正后**会**刷新。因此建议一次改完 plan.md 与 tasks.md 后重跑 `workflow check --stage plan --json` 固定新摘要，再按同一 Review ID 递增 Round 9 复核；若 main 选择先只修 tasks.md 的三项，则 Round 9 仍绑定当前摘要 `b2363f45…c41d3` 亦为有效（按角色约定「摘要未变且原审查有效可复用/延续」）。

### 已核对证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` `## Checks` 三行均为 NOT_APPLICABLE | 否。本轮为规划审查，结论基于声明本身与其对仓库事实的引用 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE；其 Target Revision 仍写「MU1–MU3 全部合入后」（F22） | 否 |
| AC1 | 待补（apply 阶段）；可满足性维持 Round 1 F5 的结论 | `crates/server/src/node_link/resource.rs` 的会话级投递路径仍丢弃会话标识为空的事件，本轮该处字节未变 | 否 |
| AC2 / AC3 | 待补（apply 阶段） | 入口 `npm run verify` 与 `clients/app` 契约测试均不依赖尚不存在的 Sync 入口，可满足；AC3 的对拍向量即 F27 所指的 `device-proof.json` | 否（F27 只影响 WP5b 的失效登记，不影响 AC3 可满足性） |
| 变更一（WP5a/WP5b、MU3a–MU3e） | **plan 层通过，tasks/Coverage 层不通过** | 8 单元解析、波次、写入范围、reviewer 独立性全部通过；F25/F26/F28/F29 定位在 tasks.md 与 Coverage Index | 是（已计入 4 项 MAJOR） |
| 变更二（静态服务器资源行） | **通过（含 2 项 MINOR）** | `plan.md:953` 六列齐备、消费者 TP4 正确、与 Main E2E 判定不冲突；F32/F33 为措辞与镜像同步 | 否（MINOR 不阻断） |
| Contract Freeze ⊆ Dependencies | **不通过（1/27 越界）** | WP5b 的 `@WP1` 不在其依赖列（F27）；其余 26 条全部通过 | 是（已计入 F27） |
| Contract Freeze 路径可读性 | **不通过（机械口径）** | 12 个包 readable 全部 false（F13，结转）；路径本身逐个存在 | 是（已计入 F13，非阻断） |
| Shared File Ownership 覆盖 | **部分不通过** | 17 行覆盖了当前字面重叠；但 `:919`/`:920` 的 Writers 指向已退役的 `WP5`，`clients/app/package.json` 的合入顺序缺真实写入者（F31） | 是（已计入 F31，MINOR） |
| Dependency Handoffs 完整性 | **部分不通过** | 11 行中 WP6/WP7/TP3 的 Upstream 指向 `WP5`；WP7 行另漏 WP1（F14、F31）；WP5b 行缺 WP1（F27） | 是（已计入 F27/F31/F14） |
| tasks.md 与 plan.md 的交付编排一致性 | **不通过** | §5/§6 仍按单一 MU3 单元，与 8 单元 Merge Strategy 矛盾（F29） | 是（已计入 F29） |
| Coverage Index 任务映射 | **不通过** | 53/102 行错位，`2.8` 零引用（F28） | 是（已计入 F28） |
| WP2/WP1 派发台账 ack 缺口 | **上下文，非规划缺陷** | `verification.md` 的 `## Dispatch Reconciliation` 记录根因、五个执行者的实际接收证据与用户 2026-10-03 的 (a) 裁决 | 否 |
| 规划门禁登记 | 待主 Agent 刷新 | `verification.md:42` 的 Round 5 行被并入 Round 7 行，Round 7 绑 `8eb6a3eb…dd4ad`，非当前摘要（F34） | 否（属主 Agent 登记刷新） |

### 不确定性与后续建议

- **F25/F26/F29 的修复会改动 tasks.md，但不刷新 planningDigest**（见上方附带确认）；F27/F28 的修复会改动 plan.md 并刷新摘要。建议一次改完再重跑 `workflow check --stage plan --json`，避免多轮摘要抖动。
- **F27 的修复不触发波次或理由码的连锁反应**：补 `contract:WP1` 后 earliest(WP5b) 仍为 3，与声明的 W3 相等，`Serialization Reason` 继续合法 `NOT_APPLICABLE`。
- **F28 的修复会改变摘要**：coverage 行的 `tasks` 字段进入各包的 `planningHash`，重编号后必然刷新摘要；同时 `2.6` 的 `taskPackages` 会从 `{WP5a, WP5b}` 收敛为 `{WP5b}`（前提是 F26 把 WP5b 描述移入 2.6 的任务正文后不再在 WP5a 的任务里提到 WP5b），归属才正确。
- **F13 与波次的耦合维持不变**：若按 F13 建议规范化 Contract Freeze 单元格，必须同步为受影响包补合法理由码，否则 `workflow check --stage plan` 会转 FAIL。规范化不是本轮要求。
- **MU3a–MU3e 的候选与 receipt 能否实际产出，本轮无法验证**，属 apply 阶段 merger 职责；但在 F29 修正前，tasks.md 无法产出其中四个单元的 receipt。
- **本报告不宣称任何实现、测试、E2E 或最终验收通过。** MU1a 的候选 PV1、独立 review 与 premerge 门禁均未执行；`## Checks` 六行仍全为 NOT_APPLICABLE。

FAIL
