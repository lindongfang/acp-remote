<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 10。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-10"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7、coding-5b、testing-1..testing-4）"
target_revision: "plan-v2:sha256:1c254656319f7b64e9a5a2fa7f2add9d48aec661bd19465a968d7c1bd07a4171"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记完整性、Contract Freeze 引用 ⊆ Dependencies、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 的独立检视任务、Coverage Index（叙述表 + agentic-coverage 结构化块）与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order、Runtime Resources 六列完整性、行为契约可满足且无冲突；并逐项复核主 Agent 声称的 7 处 Round 9 修正（含 (a)–(e) 五个回归面）。"
changes: "只读检视，未修改任何规划文件；仅新增本报告。全部机械判据以内联 node --input-type=module -e 复现，不落盘临时脚本；未执行任何 Project Verify / 构建 / 测试 / E2E。"
issues: "新发现 5 项（F41–F45），全部为 MINOR/SUGGESTION，不阻断。"
result: PASS
evidence_paths: NOT_APPLICABLE
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘脚本）"

checks:
  - id: "planningDigest 重新推导"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "退出码 1。planningDigest=plan-v2:sha256:1c254656319f7b64e9a5a2fa7f2add9d48aec661bd19465a968d7c1bd07a4171（与调度者给定 Target Revision 逐字一致，摘要未变化）；contractDigest=sha256:c24668fca597451399ca5e86358f72faf043e959ec457ab6972a13eecd0ddfc7、requirementsDigest=sha256:c74ff6ab598f425d100e1c095fac1d4e3465c8ef9dad932222060636a9881e68（均与给定值一致）。errors 恰为两条，均为 DDR 绑定行待刷新：Plan Revision 未绑定当前摘要、Result 必须为 PASS（当前为带括注的 PASS）。无 ownership/waves/freeze/coverage 类错误。另以 --stage premerge 复跑，errors 仍恰为同样两条。"
  - id: "taskPackages 口径实跑（F35 修正与 4.10 归属）"
    command: "node --input-type=module -e（复刻 planning-model.mjs:37-40 的正则 (^|[^\\w-])<ID>(?![\\w-]) 逐任务行取包）"
    result: "2.1→[WP1]、2.2→[WP2]、2.3→[WP3]、2.4→[WP4]、2.5→[WP5a]、2.6→[WP5b]、2.7→[WP6]、2.8→[WP7]、4.4→[WP1,WP2,TP1]、4.10→[WP1,WP5a,WP5b,WP6,WP7,TP3]、4.11→[TP3]、4.13→[WP7,TP4]、4.14→[TP4]、3.1→[WP1]、4.7→[WP3,TP2]、4.8→[TP2]、4.9→[TP2]、4.12→[TP3]、4.15→[TP4]；1.x、4.1–4.3、4.5–4.6、5.x–9.x 为空。F35 的 2.6 修正成立：2.6 只映射 WP5b。但 4.10 同时映射 WP5a/WP5b，见 F41。"
  - id: "Coverage 归属膨胀量化（planningHash 输入）"
    command: "node --input-type=module -e（按同一 taskPackages 口径统计 102 行 coverage 的逐包 coverage 集）"
    result: "引用 4.10 的 37 行 = R042–R061、R072–R088。含 4.10 与不含 4.10 的逐包覆盖行数：WP1 83→46、WP5a 42→12、WP5b 37→10、WP6 47→31、WP7 47→22、TP3 37→37。即 WP5a 仍被分到 R045–R049 与 R084–R088（与 F35 声称已消除的 10 行重合），另有 R050–R061、R072–R079 一并计入。"
  - id: "Coverage 结构化块与 5 份 delta spec 的逐标题对应"
    command: "node --input-type=module -e（解析 agentic-coverage 块与 5 份 spec 的 ### Requirement / #### Scenario 标题集）"
    result: "102 行 / 102 个标题：heading 精确匹配缺失 0、requirement 归属错配 0、spec 标题未被引用 0、重复 id 0。spec 实测计数：sync-snapshot-scope 4 需求/13 场景、core-derived-events 5/19、pwa-web-client 11/36、workspace-resolution 1/4（MODIFIED）、local-agent-host 2/7。plan.md:5 的「Specs Revision」句写 15/35/6，与实测不符（见 F43）。"
  - id: "叙述 Coverage 22 行的 Closure Unit 与 Responsible Units"
    command: "node --input-type=module -e（逐行取 Responsible Units 与 Merge Strategy 的 order 映射比对 R1–R22）"
    result: "22 行全部读出；逐行 Closure Unit = 该行 Responsible Units 中 Order 最大的交付单元，0 处不等（R1/R4→MU1b、R5–R9/R21/R22→MU2、R19→MU3d、R2/R3/R10–R18/R20→MU3e）。"
  - id: "Contract Freeze 引用 ⊆ Dependencies"
    command: "node --input-type=module -e（剥离反引号后逐包比对 @ 目标与 code:/contract: 目标）"
    result: "越界数 0。22 条 @ 引用全部落在依赖列内（WP3 @WP1/@WP2、WP4 @WP2、WP5a @WP1×2、WP5b @WP1、WP6 @WP1、WP7 @WP2、TP1 @WP1×3/@WP2、TP2 @WP3/@WP2、TP3 @WP1×2/@WP5a/@WP5b/@WP6×2/@WP7×2）；WP1/WP2/TP4 无引用。"
  - id: "Execution Waves 与依赖列一致性（hardDeps 重算）"
    command: "node --input-type=module -e（code 与 contract 边均计入硬依赖，重算 earliest）"
    result: "12 行逐行相等：WP1 1/1、WP2 1/1、WP3 2/2、WP4 2/2、WP5a 2/2、WP5b 3/3、WP6 4/4、WP7 5/5、TP1 2/2、TP2 3/3、TP3 6/6、TP4 6/6；无环、无「早于最早批次」、无无理由延后行。"
  - id: "Write Scope 重叠与 Shared File Ownership 登记（含 F40 复核）"
    command: "node --input-type=module -e（按 workflow-check.mjs:114-117 的 scopePaths/scopeOverlap 语义复算，并核对登记行的 Writers/Merge Owner/Merge Order/Re-verify）"
    result: "字面重叠 6 组（WP1×WP2 三处、WP3×WP4、WP5a×WP7、TP1×WP1、TP1×WP2、TP2×WP3）全部有登记行且四列齐备、Merge Order 覆盖全部写入者；WP5a×WP7 的新登记行（plan.md:929，六列）与 Write Scope、Wave（W2/W5）、code:WP5a 依赖排序一致。5 条资源行均 6 格；F33 行只保留 Vite。"
  - id: "Dependency Handoffs 与依赖列一致性"
    command: "node --input-type=module -e（10 行 handoff 的 Upstream 集合 vs 对应包的 code:/contract: 目标集合）"
    result: "9 行相等；WP5b 不等：Dependencies=code:WP5a, contract:WP1，Upstream 只写 WP5a（见 F42）。"
  - id: "交付单元解析与 Reviewer 独立性"
    command: "node --input-type=module -e（实跑 scheduling-evidence.deliveryUnits(plan)）+ 表列比对"
    result: "8 单元全部 independent：MU1a(order1,WP1|WP2)、MU1b(2,TP1)、MU2(3,WP3|WP4|TP2)、MU3a(4,WP5a)、MU3b(5,WP5b)、MU3c(6,WP6)、MU3d(7,WP7)、MU3e(8,TP3|TP4)；并集恰为 12 个已声明 ID。12 包的 Owner 与 Reviewer 无一相同（WP5a=coding-5/review-5、WP5b=coding-5b/review-5b 等）。"
  - id: "tasks.md §6 重编号与交叉引用"
    command: "node --input-type=module -e（按 (6.x, 单元) 分组计数）"
    result: "MU1a 6.1–6.8(8)、MU1b 6.9–6.15(7)、MU2 6.16–6.23(8)、MU3a 6.24–6.31(8)、MU3b 6.32–6.39(8)、MU3c 6.40–6.47(8)、MU3d 6.48–6.55(8)、MU3e 6.56–6.63(8)；正文交叉引用（6.1→「6.6」、7.1→「6.58」）均指向真实存在的项。"
  - id: "依赖类型声明属实性（对照仓库事实）"
    command: "read crates/core/Cargo.toml、crates/agent-host/Cargo.toml、scripts/check-crate-boundaries.mjs；grep core 投递路径"
    result: "crates/core 的 [dependencies] 仅 async-trait/thiserror/p256/sha2；crates/agent-host 无 sync-protocol；CORE_FORBIDDEN 含 sync-protocol（scripts/check-crate-boundaries.mjs:92-98）。crates/server/src/node_link/resource.rs:1029-1032 对 event.session.is_none() 直接 return。WP3/WP4 的 contract 降级与 WP5a/WP5b/WP6/WP7/TP3/TP4 的 code 声明均非虚假。"
  - id: "残留 WP5 与「9 态」全文搜索"
    command: "grep -n 'WP5\\b' / '9 态' 于 plan.md、tasks.md、proposal.md、design.md、verification.md、specs/**"
    result: "现行依赖声明中无裸 WP5 残留：tasks.md 仅 1 处（第 31 行，Round 9 修正说明中作为历史引用）、verification.md 的 10 处全部位于 Change Plan Changes / Review Findings 的历史登记行、plan/design/specs 为 0。「9 态」残留 1 处：proposal.md:87（见 F44）。"

```agentic-handoff
version: 1
agent_context:
  agent_id: "reviewer-plan-ddr-10"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；不拥有任何工作包）"
handoff_index:
  - task_id: "propose:DDR-sync-scope-and-pwa-client-r1"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 10
    stage: plan
    target_revision: "plan-v2:sha256:1c254656319f7b64e9a5a2fa7f2add9d48aec661bd19465a968d7c1bd07a4171"
    evidence_type: REVIEW
    evidence_id: DDR-sync-scope-and-pwa-client-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r10.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 D:\\Project\\acp-remote 对 openspec/changes/sync-scope-and-pwa-client/ 的 plan.md / tasks.md / verification.md / proposal.md / design.md / 5 份 delta spec 做只读复审。planningDigest 由 workflow check --stage plan --json 重新推导为 1c254656…a4171，与调度者给定值逐字一致。逐项复核主 Agent 声称的 7 处 Round 9 修正：5 处完全关闭、2 处部分达成（F35 类的 MU3a 归属、F37 类的统一 12 态）。另独立复现全部机械判据（102 行 Coverage 逐标题、22 行 Closure Unit、12 行波次、Contract Freeze ⊆ Dependencies、Write Scope 重叠登记、交付单元、§6 重编号），未发现 MAJOR/CRITICAL。新发现 5 项，全部 MINOR/SUGGESTION，不阻断。"
    source_evidence: NOT_APPLICABLE
```

## Review Context

| 项 | 值 |
| --- | --- |
| Review ID | DDR-sync-scope-and-pwa-client-r1（沿用 Round 1–9 的 Review ID） |
| Round | 10 |
| Review Type | plan |
| Review Stage | 计划/依赖声明阶段（Round 9 判 PASS 后主 Agent 修正其全部 MINOR 并刷新摘要，本轮为该批修正的 recheck + 当前固定摘要的完整复审） |
| Reviewer | `reviewer-plan-ddr-10`（不拥有任何 WP/TP；owners 为 `coding-1..7`、`coding-5b`、`testing-1..4`，独立性成立） |
| Work Package | NOT_APPLICABLE（范围是全部 12 包 + 8 个交付单元的声明） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | NOT_APPLICABLE（规划审查不针对代码 diff） |
| Target Revision | `plan-v2:sha256:1c254656319f7b64e9a5a2fa7f2add9d48aec661bd19465a968d7c1bd07a4171`（本轮第 1 次执行 `workflow check --stage plan --json` 即得此值，与调度者给定值一致；随后未再修改任何规划文件） |
| Requirements | `proposal.md`、`design.md`（D1–D8）、`specs/sync-snapshot-scope/spec.md`、`specs/core-derived-events/spec.md`、`specs/pwa-web-client/spec.md`、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`、`plan.md`、`tasks.md`、`verification.md`（同目录当前字节） |
| Project Rules | `AGENTS.md`；`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`procedures/scheduling.md`；`openspec/agentic.yaml`；`docs/FRONTEND_DESIGN.md` §5 |
| Verification Evidence | 本轮不执行任何 Project Verify。`verification.md` 的 `## Checks` 中 PV1/PV2/PV3/IV1/AC1–AC3 为 NOT_APPLICABLE（apply 阶段执行）；MU1a 候选一行记 PASS（证据 `reports/PV1.log`），`reports/PV1-main-mu1a.log` 仍不存在 |
| Check Plan | `plan.md` 的 `## Verification Strategy`（PV1/PV2/PV3/IV1/AC1/AC2/AC3）；`verification.md` 的 `## Checks` 与 `## Check Plan Changes` |
| Previous Findings | Round 9 报告（F35–F39 + 结转 F13）、Round 8 报告（F13–F34）、`verification.md` 的 `## Review Findings` |
| 实际检查范围 | (a) 主 Agent 声称的 7 处 Round 9 修正逐项复核；(b) 回归面 (a) taskPackages(2.6) 与 MU3a 归属；(c) 回归面 (b) tasks 2.8/4.10 vs plan WP7/TP3 逐项；(d) 回归面 (c) 12 态三处一致性 vs spec 与 `docs/FRONTEND_DESIGN.md` §5；(e) 回归面 (d) 新登记行六列与 Wave/依赖一致性；(f) 回归面 (e) 资源行六列；(g) 连续性项：22 行 Closure Unit、§6 的 6.1–6.63 与交叉引用、102 行 Coverage vs 5 份 spec 标题、12 行波次 vs hardDeps earliest、Contract Freeze ⊆ Dependencies；(h) 残留 `WP5`/「9 态」全文搜索；(i) 依赖声明属实性的仓库事实核对 |
| 未验证内容 | 未执行任何构建/测试/E2E；未评估用例覆盖充分性（归 validator 与 main 的 Coverage Index）；未判断 MU3a–MU3e 的候选与 receipt 能否实际产出（apply 阶段 merger 职责） |
| 本轮报告路径 | `openspec/changes/sync-scope-and-pwa-client/reports/ddr-sync-scope-and-pwa-client-r10.md` |
| 核对的 Check / E2E ID | PV1、PV2、PV3、IV1、AC1、AC2、AC3（均未执行，规划阶段）；Main E2E `not-applicable` 的降级判据与三条替代检查入口 |

## Findings

### 一、主 Agent 声称的 7 处修正逐项复核

注：调度者表格的编号与 Round 9 报告实际编号不同（Round 9 报告只有 F35–F39 五个新发现）。下表按调度者表格的 ID 复核，并注明对应的 Round 9 原始编号。

| ID（调度者） | Round 9 原始发现 | 本轮复核 | 复核依据 |
| --- | --- | --- | --- |
| F35（2.6 派发行含 WP5a） | r9-F35（MINOR） | **部分解决**：2.6 派发行本身已改写且实跑只映射 WP5b；「MU3a 不再被分到 R045–R049/R084–R088」未成立，见 F41 | `tasks.md:26` 现为「派发 WP5b coder，按 apply 协议登记开工/接收；依赖其上游包的交付提交已合入（MU3a）」，包 ID 只留在子条目行。实跑 taskPackages：`2.6 -> ["WP5b"]`（对照输出见 checks 第 2 条全文）。WP5a 的归属改由 `tasks.md:71`（4.10）的 `code:WP5a` 产生：WP5a 覆盖集 42 行，其中 30 行仅由 4.10 贡献，含 R045–R049 与 R084–R088 |
| F36（2.8/4.10 写 `code:WP5`） | r9-F37（MINOR） | **已解决** | `tasks.md:31`（2.8）现为「依赖：`code:WP6`、`code:WP5a`、`code:WP5b`、`contract:WP1`、`contract:WP2`」，与 `plan.md:866` WP7 行 `code:WP6, code:WP5a, code:WP5b, contract:WP1, contract:WP2` **逐项一致**；`tasks.md:71`（4.10）现为 `code:WP5a`/`code:WP5b`/`code:WP6`/`code:WP7` 与 `contract:WP1`，与 `plan.md:869` TP3 行 `code:WP5a, code:WP5b, code:WP6, code:WP7, contract:WP1` **逐项一致**。全文搜索：现行依赖声明无裸 `WP5`；仅 `tasks.md:31` 的修正说明把它作为历史引用保留（verification.md 的 10 处均在历史登记行内） |
| F37（「连接 9 态」→ 12 态） | r9-F36（MINOR） | **部分解决**：三处已一致，但 Requirements 输入的第四处未改，见 F44 | `plan.md:40`（R12 行）为「WP6: 12 态（8 常规态 + 4 阻断态）迁移实现」；`tasks.md:29`（2.7）逐项列出 8 常规态（未配对/配对中/已断开/连接中/认证中/重放追平中/在线/重连中）+ 4 阻断态（被撤销/版本不兼容/主机身份变化/被其它标签页顶替）；`tasks.md:71`（4.10）为「连接 12 态（8 常规态 + 4 阻断态，逐项取自 specs/pwa-web-client/spec.md 的枚举）」。与 `specs/pwa-web-client/spec.md:49` 的 12 个名称逐个对照：无多写、无漏写、无写错；与 `docs/FRONTEND_DESIGN.md:185-193` 的 12 个英文状态图（unpaired/pairing/disconnected/connecting/authenticating/replaying/online/reconnecting + revoked/incompatible/identity_changed/replaced）一一对应。残留：`proposal.md:87` 仍写「连接 9 态」 |
| F38（`## Review Findings` 缺 F14/F33 两行） | r9-F38（MINOR） | **已解决** | `verification.md:213-214` 两行均存在，Review ID / Round=8 / 发现内容 / Severity=MINOR / Resolution（含「本行补登记」说明）/ Report Path 六列齐备，与 Round 8 报告的 F14/F33 逐条对应 |
| F39（`## Review Findings` 缺 Round 9 登记） | r9-F39 的另一半（登记） | **已解决（行数齐备），但编号与 r9 报告不对应，见 F45** | `verification.md:215-221` 已补 7 行：Round 9 的 F35、F36、F37、F38、F39、F40 与 F13。行数上无遗漏（r9 报告的 5 项新发现 + 结转 F13 均已登记），但 F39/F40 的内容与 r9 报告的 F38/F39 语义错位（r9 报告无 F40 行；r9 的 F39 是 WP5a×WP7 重叠项，台账记为 F40；台账 F39「缺 Round 9 登记行」在 r9 报告中不存在），且 r9 报告 checks 节引用的「F41」在 r9 报告中无对应发现行 |
| F40（WP5a×WP7 重叠未登记） | r9-F39（SUGGESTION） | **已解决** | `plan.md:929` 新行六列齐备：File=`clients/app/app/` 与 `clients/app/src/components/`、Writers (WP)=WP5a, WP7、Merge Owner=merger、Merge Order=WP5a → WP7、Re-verify After Merge=WP7: PV3、Region Note 含区域收窄。与 `plan.md:863`（WP5a Write Scope 含 `clients/app/app/`（空壳路由）与 `src/components/`）、`plan.md:866`（WP7 含同两处）一致；Wave W2 vs W5、MU3a vs MU3d，且 WP7 声明 `code:WP5a`，不构成同批冲突；机械复算重叠 6 组全部有登记行，`ownershipProblem` 口径无缺项 |
| F33（残留：Vite/Metro + 限定语义） | r9 结转（MINOR） | **已解决** | `plan.md:953` 的 Resource 现为「前端静态服务器（Vite dev server）」，「Metro」已移除；该行 6 格齐备；Use / Cleanup Boundary 列含「分片结束即终止其服务器进程；服务器只服务该分片的 worktree」并把「仅验证环境、不连真实 Daemon、只服务 fixtures」写入说明。`verification.md` 的 `## Runtime Resources` 对应行同步为「只服务该分片的 worktree，不连真实 Daemon」 |

### 二、Round 9 声称的「连续性项」独立复验

| 连续性项 | 结果 | 依据 |
| --- | --- | --- |
| 叙述 Coverage 22 行 Closure Unit | 通过 | 22/22 行等于该行 Responsible Units 中 Order 最大者（0 处不等） |
| §6 的 6.1–6.63 重编号与交叉引用 | 通过 | 单元分组 8/7/8/8/8/8/8/8；6.1→6.6、7.1→6.58 均指向真实存在的项 |
| 结构化 Coverage 102 行 vs 5 份 delta spec | 通过 | 102 行 / 102 标题，缺失 0、归属错配 0、未被引用 0 |
| 12 行 Wave vs hardDeps 重算 earliest | 通过 | 12/12 逐行相等（WP1..TP4 = 1,1,2,2,2,3,4,5,2,3,6,6），无环 |
| Contract Freeze ⊆ Dependencies | 通过 | 越界数 0（22 条 @ 引用全部落在依赖列内） |
| 回归面 (a) taskPackages(2.6) 只映射 WP5b | 通过（MU3a 归属另有残余，见 F41） | `2.6 -> ["WP5b"]` |
| 回归面 (b) tasks 2.8/4.10 与 plan WP7/TP3 逐项一致 | 通过 | 集合逐项相等（见上表 F36 行） |
| 回归面 (c) 12 态三处一致且与 spec 枚举逐项一致 | 通过（proposal 残留见 F44） | 三处一致；12 个名称与 spec:49 及 FRONTEND_DESIGN §5 逐项对应 |
| 回归面 (d) 新登记行六列与 Wave/依赖一致 | 通过 | 六列齐备，WP5a(W2) → WP7(W5)，WP7 有 `code:WP5a` |
| 回归面 (e) 资源行六列齐备 | 通过 | `plan.md:953` 6 格；5 条资源行均 6 格 |

### 三、本轮新发现（自 F41 起编号）

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F41 | MINOR | `openspec/changes/sync-scope-and-pwa-client/tasks.md:71`（4.10 的任务行文本；机制见 `node_modules/@dongfanglin/openspec-agentic/src/planning-model.mjs:37-40` 与 `:46` 的 refs 过滤） | 4.10 为 TP3 的用例设计任务，其依赖文本经 F36 修正后含 `code:WP5a`/`code:WP5b`/`code:WP6`/`code:WP7` 与 `contract:WP1`。taskPackages 只按任务行文本取包 ID，故实跑得 `4.10 -> ["WP1","WP5a","WP5b","WP6","WP7","TP3"]`；引用了 4.10 的 37 行（R042–R061、R072–R088）因此进入这些包的 `coverage` 与 `sources`，使 WP5a 的覆盖行数由 12 涨到 42、WP5b 由 10 涨到 37、WP1 由 46 涨到 83、WP6 由 31 涨到 47、WP7 由 22 涨到 47 | F35 的修复目标之一是「MU3a 不再被分到 R045–R049/R084–R088」，该目标**未达成**：MU3a（=WP5a）的 `planningHash` 仍把 R045–R049/R084–R088 计为自身覆盖，而 plan.md:863 明确 WP5a 的目标仅 R10/R15/R19。后果局限于语义规划摘要与 `comparePlanning` 的影响集（修改任一只由 4.10 关联的行会把 WP5a/WP5b/WP6/WP7/WP1 一并标记为受影响），不影响任何机械门禁（本轮 `workflow check --stage plan` 与 `--stage premerge` 均未报相关错误）。**不构成 MAJOR**：4.10 声明上游 code 依赖本身是 plan.md:869 TP3 行的要求，声明内容正确，失配来自工具把「声明依赖」等同于「覆盖验证」的粗口径 | 若要让归属与「本包目标」一致，可把 4.10 的依赖清单移到紧随其后的子条目行（与 F35 对 2.6 的处置同型），任务行只保留 `[PV3]` 与描述，使 taskPackages(4.10) 不含上游包 ID；若判定该工具口径可接受，则应在 verification 的 `## Review Findings` 里以「工具口径，非计划缺陷」显式登记并撤销 F35 中「MU3a 不再被分到」的表述 | 新发现（Round 10） |
| DDR-sync-scope-and-pwa-client-r1-F42 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:938`（`## Dependency Handoffs` 的 WP5b 行） | WP5b 的 Dependencies 为 `code:WP5a, contract:WP1`（`plan.md:864`），Contract Freeze 引用 `fixtures/sync/v1/transcripts/device-proof.json@WP1`；但 WP5b 的 handoff 行 Upstream 只写 `WP5a`，Invalidation 也只列 WP5a 的 transcript 编码变更。逐行比对 10 条 handoff：其余 9 条的 Upstream 集合等于该包依赖目标集合，仅 WP5b 不等（WP1 属 MU1a、WP5b 属 MU3b，是跨交付单元的 contract 边） | 与 Round 4/9 的 F14（WP7 的 handoff Upstream 漏 WP1）同类：跨单元的 contract 前置缺登记其失效路径，WP1 的 schema 变更时 WP5b 的 transcript 编码依据失效不可从 handoff 表直接读到。`Dependency Handoffs` 不被机械判据读取，故不阻断 | 把该行 Upstream 改为 `WP1, WP5a`，并在 Invalidation 补一条「WP1 的 `fixtures/sync/v1/transcripts/device-proof.json` 变更 → WP5b 的签名/编码结果失效，需重跑 PV3 与 AC3」 | 新发现（Round 10） |
| DDR-sync-scope-and-pwa-client-r1-F43 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md:5`（`## Scope and Contracts` 的「Specs Revision」句） | 该句写 sync-snapshot-scope 4 需求 / **15** 场景、pwa-web-client 11 需求 / **35** 场景、local-agent-host 2 需求 / **6** 场景。按 `^### Requirement:` / `^#### Scenario:` 逐份统计实际为 4/13、11/36、2/7（core-derived-events 5/19 与 workspace-resolution 1/4 相符） | 规划契约清单与 delta spec 自身计数不一致；该句被 `planning-model.mjs` 的 `scope` 段显式排除（`Specs Revision`/`Design Revision` 行被过滤），故不影响摘要与门禁，但最终验收逐条核对需求/场景计数时会读到三个不符的数字。Round 9 报告的 checks 节已引用「见 F41」指出同一事实，但该报告未登记 F41 行，故修正轮从未处理 | 将三处改为实测值：`4 需求 / 13 场景`、`11 需求 / 36 场景`、`ADDED 2 需求 / 7 场景`（或改口径为「不含场景数」并与 verification 的计数说明统一） | 新发现（Round 10） |
| DDR-sync-scope-and-pwa-client-r1-F44 | MINOR | `openspec/changes/sync-scope-and-pwa-client/proposal.md:87` | 该行仍写「Sync Client、**连接 9 态**与命令 6 态状态机」，而 `specs/pwa-web-client/spec.md:49` 枚举 12 个状态（8 常规态 + 4 阻断态）、`docs/FRONTEND_DESIGN.md:185-193` 的状态图同样 12 个。Round 9 的 F36 与主 Agent 的修正声明均把范围限定为 plan R12 行与 tasks 2.7/4.10，这三处已改为 12 态，proposal 未改 | proposal 是本变更的 Requirements 输入并进入 `requirementsDigest`；同一变更内「9 态」与「12 态」并存，TP3/WP6 的验收口径在需求侧仍留有一个可被读成 9 的口径。**不构成 MAJOR**：权威行为定义在 spec（12 态）且已一致，proposal 行是概述性描述 | 把该行的「连接 9 态」改为「连接 12 态」（或改为「连接状态机」不带数字），使需求侧口径唯一 | 新发现（Round 10，F37 类残留） |
| DDR-sync-scope-and-pwa-client-r1-F45 | SUGGESTION | `openspec/changes/sync-scope-and-pwa-client/verification.md:215-220`（`## Review Findings` 的 Round 9 行） | Round 9 报告（`reports/ddr-sync-scope-and-pwa-client-r9.md:141-145`）实际只有 5 个新发现 F35–F39，其中 F39 = WP5a×WP7 重叠（SUGGESTION）；报告 checks 节还引用了不存在的「F41」。`verification.md` 的 Round 9 行却登记 6 条（F35、F36、F37、F38、F39、F40）+ 结转 F13，其中 F39 的内容是「缺 Round 9 的登记行」（r9 报告中无此发现）、F40 的内容是重叠项（对应 r9 的 F39），且 F40 被记为 MINOR 而已修 | 台账与原始报告的问题 ID 不能一一对上，最终验收按问题 ID 逐项闭环时会读到「r9 的 F39 未闭环」并多出一个 r9 中不存在的 F40；调度者本轮派发也沿用了 F40 的编号（新发现要求从 F41 起）。属登记措辞问题，不改变依赖声明结论 | 以 r9 报告为准校正台账：把重叠项记为 F39（并注明主 Agent 修正轮实际编号为 F40），删除或改注「缺 Round 9 登记行」一行；若保留双编号，在行内写明映射关系 | 新发现（Round 10） |

## Assessment

### 本轮结论

**PASS**。当前固定摘要 `plan-v2:sha256:1c254656…a4171` 下：依赖类型声明属实（`code:` 对照 `crates/core/Cargo.toml`、`crates/agent-host/Cargo.toml`、`scripts/check-crate-boundaries.mjs:92-98` 的仓库事实；`contract:` 的冻结路径逐条存在）、Contract Freeze ⊆ Dependencies 越界数为 0、12 行波次与 hardDeps 重算的 earliest 逐行相等且无环、Write Scope 的 6 组字面重叠全部有登记行且四列齐备、8 个交付单元解析为 independent 且并集恰为 12 个已声明 ID、Owner/Reviewer 无自审、叙述 Coverage 22 行 Closure Unit 与结构化 Coverage 102 行对 5 份 delta spec 的 102 个标题全部命中、tasks.md §6 的 6.1–6.63 与交叉引用一致。无已确认的 CRITICAL/MAJOR。

7 处声称修正的复核结果：**F36（2.8/4.10 的 code:WP5）、F38（补 F14/F33）、F39（补 Round 9 行）、F40（WP5a×WP7 登记）、F33（Metro 与限定语义）五处完全关闭**；**F35 在 2.6 本身已关闭**，但其「MU3a 不再被分到 R045–R049/R084–R088」的目标因 4.10 的依赖文本未达成（F41）；**F37 的三处已统一为 12 态**，但 proposal.md:87 仍写「9 态」（F44）。

### 新发现与严重度

新发现 5 项（F41–F45），全部 MINOR/SUGGESTION，均有文件:行证据与内联复现输出，可定位、可动作，且都属于登记/措辞/工具口径层面，不改变「依赖声明属实、批次与归属自洽、契约可满足」这一结论：F41（4.10 造成 MU3a/MU3b 等覆盖归属膨胀，仅影响语义摘要与影响集，不影响门禁；4.10 的声明内容本身符合 plan TP3 行，故不判 MAJOR）、F42（WP5b 的 handoff Upstream 漏 WP1，与 Round 4/9 的 F14 同类同severity）、F43（plan.md:5 的三处场景计数与 spec 实际不符）、F44（proposal 仍写「连接 9 态」）、F45（台账 Round 9 编号与 r9 报告不对应）。

### 证据与待补项

| 检查 ID | 状态 | 证据 / 差异 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1 / PV2 / PV3 | 待补（apply 阶段） | `verification.md` 的 `## Checks` 三行为 NOT_APPLICABLE（apply 阶段执行）；PV3 的目标工程 `clients/app` 尚无代码 | 否。本轮为规划审查，结论基于声明本身及其对仓库事实的引用 |
| AC1 / AC2 / AC3 | 待补（apply 阶段） | 三行 NOT_APPLICABLE；入口（`cargo test -p app --test node_link_e2e`、`npm run verify`、`clients/app` 契约测试）均不依赖尚不存在的 Sync 入站面 | 否 |
| IV1 | 待补（apply 阶段） | `## Independent Validation` 行 Result 为 NOT_APPLICABLE；目标版本表述为「MU1a–MU3e 全部合入后」 | 否 |
| MU1a 候选 PASS 行 | 规划外记录 | `verification.md` 的 `## Checks` 有一行 MU1a 候选 683dbbb8 记 PASS（Executor 记为 merger，证据 `reports/PV1.log`）；`reports/PV1-main-mu1a.log` 不存在 | 否（属主分支回归的待补证据，须在 MU1a 合入后的门禁前由 main 核对执行者与版本） |
| `workflow check --stage plan` 的两条 error | 预期内的登记待办 | 恰为「DDR Plan Revision 未绑定当前摘要」与「DDR Result 必须为 PASS（当前带括注）」；本轮报告给出裸词结论，由 main 写入 verification 后刷新 | 否（由 main 在登记本轮结论时消除） |
| Contract Freeze 机械可读性（F13） | 维持非阻断 | 12 包的 Contract Freeze 单元格仍不匹配 `^(\S+?)@(\S+)$`（多路径/反引号）；22 条 `@` 引用路径逐个存在。Round 9 已独立判定为非阻断且明确不升级，本轮未发现事实前提变化 | 否（沿用 Round 9 判定，不重开） |
| 变更目录的版本控制 diff | 不可用 | `git status --porcelain` 显示 `openspec/changes/sync-scope-and-pwa-client/` 未跟踪，无法逐行 diff；本轮以「按当前字节重跑全部机械判据 + 逐项核实 7 处修正」替代 | 否（已在 Review Context 记录该限制） |

### 不确定性与后续建议

- 本轮不宣称任何实现、测试、E2E 或最终验收通过；`## Checks` 的 PV1/PV2/PV3/IV1/AC1–AC3 仍待 apply 阶段补齐。
- F41 的两条出路（改 4.10 的行文以消除上游包 ID，或显式登记为工具口径并撤销 F35 的相应表述）互相排斥，需由 main 选定其一，避免下一轮复核对同一事实给出相反判断。
- F43 是 Round 9 报告 checks 节引用「F41」却未登记成行所遗留的事实；建议连同 F45 的编号校正一并处理，使问题 ID 与原始报告可一一对应。

## Dependency Declaration Review

PASS

PASS
